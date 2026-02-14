use std::collections::HashMap;
use std::env;
use std::fs;
use std::path::Path;

pub fn prepare_env(
    dir: &Path,
    config_env: Option<&HashMap<String, String>>,
) -> HashMap<String, String> {
    // 1. Load .env
    let dot_env = load_dot_env_file(dir);
    // 2. Take config
    let conf_env = config_env.cloned().unwrap_or_default();
    // 3. Merge
    resolve_vars(&conf_env, &dot_env)
}

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
) -> HashMap<String, String> {
    let mut final_env = HashMap::new();

    // Now we take all from .env (like DB)
    for (k, v) in dot_env {
        final_env.insert(k.clone(), v.clone());
    }

    // Then merge flare.toml with resolve
    for (k, v) in conf {
        if v.starts_with("$ENV:") {
            let var_name = &v[5..];
            let val = if let Some(val) = dot_env.get(var_name) {
                val.clone()
            } else {
                env::var(var_name).unwrap_or_default()
            };
            final_env.insert(k.clone(), val);
        } else {
            // Rewrite, if key have (flare.toml have more priority)
            final_env.insert(k.clone(), v.clone());
        }
    }
    final_env
}
