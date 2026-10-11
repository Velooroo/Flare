//! Environment assembly for a running app.
//!
//! INFO: Precedence is: repo `.env` file first, then `[env]` from `flare.toml`
//!       overrides. A config value written as `$ENV:NAME` is resolved from the
//!       `.env` file and then from the daemon's own process environment.
//! NOTE: This environment is used when *spawning the app* (`[run]`). The build
//!       step does not use it.

use anyhow::Result;
use std::collections::HashMap;
use std::env;
use std::fs;
use std::path::Path;

pub fn prepare_env(
    dir: &Path,
    config_env: Option<&HashMap<String, String>>,
) -> Result<HashMap<String, String>> {
    let dot_env = load_dot_env_file(dir);
    let conf_env = config_env.cloned().unwrap_or_default();
    resolve_vars(&conf_env, &dot_env)
}

// INFO: Very small `.env` parser: `KEY=VALUE` per line, `#` comments, and
//       surrounding quotes stripped.
// WARN: No support for `export`, multi-line values or inline `#` comments.
fn load_dot_env_file(dir: &Path) -> HashMap<String, String> {
    let mut vars = HashMap::new();
    let env_path = dir.join(".env");

    if let Ok(content) = fs::read_to_string(env_path) {
        for line in content.lines() {
            let line = line.trim();
            if line.is_empty() || line.starts_with('#') {
                continue;
            }
            if let Some((k, v)) = line.split_once('=') {
                let clean_val = v.trim().trim_matches('"').trim_matches('\'');
                vars.insert(k.trim().to_string(), clean_val.to_string());
            }
        }
    }
    vars
}

fn resolve_vars(
    conf: &HashMap<String, String>,
    dot_env: &HashMap<String, String>,
) -> Result<HashMap<String, String>> {
    // INFO: Start from the .env values...
    let mut final_env = HashMap::new();

    for (k, v) in dot_env {
        final_env.insert(k.clone(), v.clone());
    }

    // ...then let flare.toml override them.
    for (k, v) in conf {
        if let Some(var_name) = v.strip_prefix("$ENV:") {
            // INFO: A missing variable is an error now (it used to silently become
            //       an empty string, which could blank out configuration).
            let val = if let Some(val) = dot_env.get(var_name) {
                val.clone()
            } else {
                env::var(var_name).map_err(|_| {
                    anyhow::anyhow!("env var '{}' referenced by '{}' is not set", var_name, k)
                })?
            };
            final_env.insert(k.clone(), val);
        } else {
            final_env.insert(k.clone(), v.clone());
        }
    }

    Ok(final_env)
}
