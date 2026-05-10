/// Describes which fields from AppConfig are consumed by the deploy pipeline.
///
/// This is the single place that documents "what the deploy needs from flare.toml".
/// The deploy module reads from here instead of poking into AppConfig directly.
use common::{
    AppConfig, BuildSection, DatabaseSection, HealthSection, HooksSection, IsolationSection,
    RunSection, WebSection,
};
use std::collections::HashMap;

pub struct DeployInput<'a> {
    pub name: &'a str,
    pub version: &'a str,
    pub build: Option<&'a BuildSection>,
    pub database: Option<&'a DatabaseSection>,
    pub run: Option<&'a RunSection>,
    pub web: Option<&'a WebSection>,
    pub health: Option<&'a HealthSection>,
    pub env: Option<&'a HashMap<String, String>>,
    pub isolation: Option<&'a IsolationSection>,
    pub hooks: Option<&'a HooksSection>,
    pub port: Option<u16>,
}

impl<'a> From<&'a AppConfig> for DeployInput<'a> {
    fn from(cfg: &'a AppConfig) -> Self {
        Self {
            name: &cfg.app.name,
            version: &cfg.app.version,
            build: cfg.build.as_ref(),
            database: cfg.database.as_ref(),
            run: cfg.run.as_ref(),
            web: cfg.web.as_ref(),
            health: cfg.health.as_ref(),
            env: cfg.env.as_ref(),
            isolation: cfg.isolation.as_ref(),
            hooks: cfg.hooks.as_ref(),
            port: cfg.run.as_ref().and_then(|r| r.port),
        }
    }
}
