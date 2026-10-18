//! The daemon's control API, served over TLS by monolith.
//!
//! One struct implements [`common::control::Handler`] for every operation
//! (register_token / deploy / manage / health). The shared gateway route table
//! and the per-app pid map are updated in place.
//!
//! INFO: Handlers are `async`, so they can await the deploy pipeline directly and
//!       push the blocking `AppManager` work onto the blocking pool.

use std::collections::HashMap;
use std::sync::Arc;
use tokio::sync::RwLock;

use common::DeployRequest;
use common::{DeployReply, HealthReply, ManageReply, RegisterReply};

use crate::app::AppManager;
use crate::auth::TokenStore;
use crate::gateway::{Route, Routes};
use crate::pipeline;

// INFO: Shared map of app name -> last known pid, used by the health handler.
pub type HealthStore = Arc<RwLock<HashMap<String, Option<u32>>>>;

pub struct DaemonControl {
    routes: Routes,
    health_pids: HealthStore,
    /// One-time code required for the very first token enrollment, if the daemon
    /// started with no tokens.
    pairing_code: Option<String>,
}

impl DaemonControl {
    pub fn new(routes: Routes) -> Self {
        // INFO: Only mint a pairing code when nobody is paired yet. Once a token
        //       exists, enrollment requires that token instead.
        let pairing_code = if TokenStore::load().tokens.is_empty() {
            let code = common::generate_token()[..6].to_uppercase();
            tracing::info!("First-time pairing code: {}", code);
            Some(code)
        } else {
            None
        };

        Self {
            routes,
            health_pids: Arc::new(RwLock::new(HashMap::new())),
            pairing_code,
        }
    }

    // WARN: The token store is re-read from disk on every request (no cache).
    fn token_ok(token: &Option<String>) -> bool {
        TokenStore::load().verify(token.as_deref().unwrap_or(""))
    }
}

impl common::control::Handler for DaemonControl {
    async fn register_token(
        &self,
        token_hash: String,
        existing_token: Option<String>,
        pairing_code: Option<String>,
    ) -> RegisterReply {
        let mut store = TokenStore::load();

        let authorized = if store.tokens.is_empty() {
            // First-time pairing: compare against the startup code.
            match self.pairing_code.as_deref() {
                Some(expected) => pairing_code.as_deref() == Some(expected),
                // No code minted (e.g. tokens were cleared at runtime) -> allow.
                None => true,
            }
        } else {
            // Already paired: the caller must prove an existing token.
            store.verify(existing_token.as_deref().unwrap_or(""))
        };

        if !authorized {
            tracing::warn!("Rejected token enrollment (bad pairing code / existing token)");
            return RegisterReply { success: false };
        }

        match store.register(token_hash) {
            Ok(()) => RegisterReply { success: true },
            Err(e) => {
                tracing::warn!("Token enrollment failed: {}", e);
                RegisterReply { success: false }
            }
        }
    }

    async fn deploy(
        &self,
        token: Option<String>,
        repo: String,
        forge: String,
        branch: Option<String>,
        auth_user: Option<String>,
        auth_password: Option<String>,
    ) -> DeployReply {
        // INFO: Deploy is the only operation that also mutates the gateway.
        if !Self::token_ok(&token) {
            return DeployReply {
                success: false,
                message: "Invalid token".into(),
                app_dir: None,
            };
        }

        tracing::info!("Deploy: {}", repo);
        let req = DeployRequest {
            repo,
            forge,
            branch,
            auth_user,
            auth_password,
        };
        let outcome = pipeline::run(&req).await;

        // INFO: On success, publish the new route to the shared gateway table.
        //       Static sites are keyed by domain; apps are keyed by app name.
        if let pipeline::Outcome::Ok {
            static_site,
            name,
            pid,
            port,
            ..
        } = &outcome
        {
            if let Some((domain, root)) = static_site {
                self.routes
                    .write()
                    .await
                    .insert(domain.clone(), Route::Static { root: root.clone() });
            } else if let Some(p) = port {
                self.routes.write().await.insert(
                    name.clone(),
                    Route::App {
                        port: *p,
                        pid: *pid,
                    },
                );
            }
            self.health_pids.write().await.insert(name.clone(), *pid);
        }

        match outcome {
            pipeline::Outcome::Ok { dir, message, .. } => DeployReply {
                success: true,
                message,
                app_dir: Some(dir.to_string_lossy().into()),
            },
            pipeline::Outcome::Err { message } => DeployReply {
                success: false,
                message,
                app_dir: None,
            },
        }
    }

    async fn manage(&self, token: Option<String>, app: String, action: String) -> ManageReply {
        // INFO: Authenticate before doing anything else.
        if !Self::token_ok(&token) {
            return ManageReply {
                success: false,
                message: "Invalid token".into(),
            };
        }

        // INFO: AppManager is blocking (spawn/kill/wait), so run it on the
        //       blocking pool instead of stalling the async worker.
        let app_for_task = app.clone();
        let action_for_task = action.clone();
        let result = tokio::task::spawn_blocking(move || match action_for_task.as_str() {
            "start" => AppManager::start(&app_for_task),
            "stop" => AppManager::stop(&app_for_task),
            "restart" => AppManager::restart(&app_for_task),
            "rollback" => AppManager::rollback(&app_for_task),
            _ => Err(anyhow::anyhow!("Unknown action: {}", action_for_task)),
        })
        .await
        .unwrap_or_else(|e| Err(anyhow::anyhow!("manage task failed: {}", e)));

        // INFO: On success, re-publish the gateway route so a rollback/restart is
        //       reflected immediately (new version dir, new pid).
        if result.is_ok() {
            refresh_route(&app, &self.routes, &self.health_pids).await;
        }

        match result {
            Ok(message) => ManageReply {
                success: true,
                message,
            },
            Err(e) => ManageReply {
                success: false,
                message: e.to_string(),
            },
        }
    }

    async fn health(&self, token: Option<String>, app: String) -> HealthReply {
        if !Self::token_ok(&token) {
            return HealthReply {
                app,
                alive: false,
                pid: None,
                message: Some("Invalid token".into()),
            };
        }

        let pid = self.health_pids.read().await.get(&app).and_then(|p| *p);
        // INFO: `kill(pid, 0)` probes liveness without sending a signal.
        let alive = pid.map(AppManager::process_alive).unwrap_or(false);

        HealthReply {
            app,
            alive,
            pid,
            message: None,
        }
    }
}

// INFO: Re-reads the app's config/state and updates the shared route table.
async fn refresh_route(app: &str, routes: &Routes, health_pids: &HealthStore) {
    let root = match AppManager::resolve_root(app) {
        Ok(r) => r,
        Err(_) => return,
    };
    let live = match crate::versions::current(&root) {
        Some(v) => v,
        None => return,
    };
    let config = match common::load_app_config(&live) {
        Ok(c) => c,
        Err(_) => return,
    };

    let pid = common::load_state(&root).ok().flatten().and_then(|s| s.pid);
    let name = config.app.name.clone();

    if let Some(web) = &config.web {
        // Static site: point the domain at the (possibly new) version root.
        let site_root = live.join(web.root.as_deref().unwrap_or("."));
        routes.write().await.insert(
            web.domain.clone(),
            Route::Static {
                root: site_root.to_string_lossy().into(),
            },
        );
    } else if let Some(run) = &config.run {
        if let Some(port) = run.port {
            if pid.is_some() {
                routes
                    .write()
                    .await
                    .insert(name.clone(), Route::App { port, pid });
            } else {
                // Stopped: drop the route so the gateway reports "no route".
                routes.write().await.remove(&name);
            }
        }
    }

    health_pids.write().await.insert(name, pid);
}
