use serde::{Deserialize, Serialize};
use std::collections::HashMap;

#[derive(Debug, Serialize, Deserialize)]
pub struct DeployRequest {
    pub msg_type: String,
    pub repo: String,
    pub forge: String,
    pub auth_user: Option<String>,
    pub auth_password: Option<String>,
    pub daemon_token: Option<String>,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct DeployResponse {
    pub success: bool,
    pub message: String,
    pub app_dir: Option<String>,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct ManageRequest {
    pub msg_type: String, // "manage"
    pub app: String,
    pub action: String, // "start", "stop", "restart"
}

#[derive(Debug, Serialize, Deserialize)]
pub struct ManageResponse {
    pub success: bool,
    pub message: String,
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

#[derive(Debug, Serialize, Deserialize)]
pub struct AppConfig {
    pub app: AppSection,
    pub build: Option<BuildSection>,
    pub run: Option<RunSection>,
    pub env: Option<HashMap<String, String>>,
    pub web: Option<WebSection>,
    pub health: Option<HealthSection>,
    pub isolation: Option<IsolationSection>,
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

#[derive(Debug, Serialize, Deserialize)]
pub struct WebSection {
    pub domain: String,
    pub root: Option<String>,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct HealthSection {
    pub url: String,
    pub timeout: Option<u64>,
}

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

#[derive(Debug, Serialize, Deserialize)]
pub struct DatabaseSection {
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

#[derive(Debug, Serialize, Deserialize)]
pub struct RegisterTokenRequest {
    pub msg_type: String,
    pub token_hash: String,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct RegisterTokenResponse {
    pub success: bool,
}
