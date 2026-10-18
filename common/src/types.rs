//! Configuration schema shared by the CLI and the daemon, plus the typed
//! values that cross the control API.
//!
//! INFO: The control API itself is declared with `monolith::api!` in `lib.rs`.
//!       The `*Reply` structs here are its return types.

use serde::{Deserialize, Serialize};
use std::collections::HashMap;

/// Input for one deploy. The CLI fills this from its flags; the daemon feeds it
/// straight to `pipeline::run`.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DeployRequest {
    pub repo: String,
    pub forge: String,
    // INFO: Git branch/ref to fetch. Missing or empty means the forge default;
    //       the CLI defaults to "main".
    #[serde(default)]
    pub branch: Option<String>,
    pub auth_user: Option<String>,
    pub auth_password: Option<String>,
}

/// Result of a deploy.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DeployReply {
    pub success: bool,
    pub message: String,
    pub app_dir: Option<String>,
}

/// Result of a lifecycle action (start/stop/restart/rollback).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ManageReply {
    pub success: bool,
    pub message: String,
}

/// Liveness of one app.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HealthReply {
    pub app: String,
    pub alive: bool,
    pub pid: Option<u32>,
    /// Set when the call was refused (e.g. "Invalid token").
    pub message: Option<String>,
}

/// Result of enrolling a new CLI token.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RegisterReply {
    pub success: bool,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct AppState {
    pub name: String,
    pub version: String,
    pub status: String,
    pub pid: Option<u32>,
    pub port: Option<u16>,
    pub health_url: Option<String>,
    pub isolation: Option<String>,
}

/// Everything a project can declare in its `flare.toml`.
///
/// A section is `Option` unless it is required for every app. If a section is
/// absent from the file, serde simply leaves it as `None`.
///
/// Status of each section (what the daemon actually does today):
///   app       — required, used everywhere
///   build     — implemented (runs `sh -c <command>`)
///   run       — implemented (spawns the process)
///   env       — implemented, merged with the repo `.env`
///   web       — implemented as static-only virtual host
///   health    — partially implemented (see `HealthSection`)
///   isolation — `none` works; `systemd`/`chroot` are best-effort
///   database  — implemented via Docker (sqlite without Docker)
///   hooks     — implemented, but the allowlist is not a real sandbox
///   storage / notify / secrets / resource_limits / metrics / strategy
///             — parsed for forward compatibility, NOT acted on yet
#[derive(Debug, Serialize, Deserialize)]
pub struct AppConfig {
    pub app: AppSection,
    pub build: Option<BuildSection>,
    pub run: Option<RunSection>,
    pub env: Option<HashMap<String, String>>,
    pub web: Option<WebSection>,
    pub health: Option<HealthSection>,
    pub isolation: Option<IsolationSection>,
    // TODO: `storage`, `notify`, `secrets`, `resource_limits`, `metrics` and
    //       `strategy` are accepted here for forward compatibility but the
    //       pipeline never reads them. `database` and `hooks` ARE implemented.
    pub storage: Option<StorageSection>,
    pub database: Option<DatabaseSection>,
    pub notify: Option<NotifySection>,
    pub secrets: Option<SecretsSection>,
    pub resource_limits: Option<ResourceLimitsSection>,
    pub hooks: Option<HooksSection>,
    pub metrics: Option<MetricsSection>,
    pub strategy: Option<StrategySection>,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct AppSection {
    pub name: String,
    pub version: String,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct BuildSection {
    pub command: String,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct RunSection {
    pub command: String,
    pub port: Option<u16>,
}

/// `[web]` — serve a folder as a static site behind the built-in gateway.
#[derive(Debug, Serialize, Deserialize)]
pub struct WebSection {
    /// Host header the gateway matches on (virtual host).
    pub domain: String,
    /// Folder to serve, relative to the extracted app directory.
    pub root: Option<String>,
}

/// `[health]` — how the daemon should probe a freshly started app.
///
/// INFO: Both fields default, so a bare `[health]` block parses. `url` is only
///       required when `auto_add = false` (checked in the pipeline).
/// WARN: With `auto_add = true` the daemon probes `http://localhost:<run.port>/health`
///       (see `determine_health_url`). If the app has no `[run].port`, no probe
///       happens.
#[derive(Debug, Serialize, Deserialize)]
pub struct HealthSection {
    #[serde(default)]
    pub url: String,
    pub timeout: Option<u64>,
    #[serde(default)]
    pub auto_add: bool,
}

/// `[isolation]` — process isolation strategy.
///
/// WARN: Only `"none"` (or omitting the section) is reliable. `"systemd"` uses
///       `systemd-run --user` which needs a logind session; `"chroot"` runs the
///       app inside the extracted app dir, which usually lacks a usable shell.
#[derive(Debug, Serialize, Deserialize)]
pub struct IsolationSection {
    pub r#type: String,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct StorageSection {
    pub r#type: String,
    pub bucket: Option<String>,
    pub endpoint: Option<String>,
    pub access_key: Option<String>,
    pub secret_key: Option<String>,
    pub size: Option<String>,
    pub mount: Option<String>,
    pub public: Option<bool>,
}

/// `[database]` — auto-provision a database on deploy.
///
/// INFO: `postgres` and `mysql` are started as Docker containers named
///       `flare-<name>-db`; `sqlite` just creates a file. `preseed` is an SQL
///       file executed once after the database is ready.
#[derive(Debug, Serialize, Deserialize)]
pub struct DatabaseSection {
    /// One of: `postgres`, `mysql`, `sqlite`.
    pub r#type: String,
    pub name: Option<String>,
    pub user: Option<String>,
    pub password: Option<String>,
    pub port: Option<u16>,
    pub preseed: Option<String>,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct NotifySection {
    pub on_success: Option<Vec<String>>,
    pub on_fail: Option<Vec<String>>,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct SecretsSection {
    #[serde(flatten)]
    pub secrets: HashMap<String, String>,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct ResourceLimitsSection {
    pub memory: Option<String>,
    pub cpu: Option<String>,
    pub timeout: Option<String>,
}

/// `[hooks]` — shell snippets executed around a deployment.
///
/// WARN: The daemon runs hooks with `sh -c`, so the allowlist only inspects the
///       first word. `"echo hi; rm -rf /"` passes the check. Do not treat this as
///       a security boundary (see pipeline::run_hook).
#[derive(Debug, Serialize, Deserialize)]
pub struct HooksSection {
    pub pre_deploy: Option<String>,
    pub post_deploy: Option<String>,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct MetricsSection {
    pub pushgateway: Option<String>,
    pub collect: Option<Vec<String>>,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct StrategySection {
    pub r#type: String,
    pub percent: Option<u8>,
    pub wait_time: Option<String>,
}

/// A device saved by `flare sync` in `~/.flare/flare.conf`.
///
/// WARN: `token` is the *plain* daemon token. It is stored unencrypted on disk,
///       so the config file should be treated as a secret.
#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct Device {
    pub id: u32,
    pub name: Option<String>,
    pub host: String,
    pub port: u16,
    pub token: Option<String>,
}

#[derive(Debug, Serialize, Deserialize, Default)]
pub struct FlareConfig {
    #[serde(default)]
    pub devices: Vec<Device>,
}
