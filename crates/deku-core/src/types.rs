use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

#[derive(Debug, Clone, Serialize, Deserialize, sqlx::FromRow)]
pub struct App {
    pub id: String,
    pub name: String,
    pub created_at: DateTime<Utc>,
    pub locked: bool,
    pub status: AppStatus,
    pub tls_enabled: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize, sqlx::Type)]
#[sqlx(type_name = "TEXT")]
#[serde(rename_all = "snake_case")]
pub enum AppStatus {
    Created,
    Deployed,
    Stopped,
    Error,
}

impl std::fmt::Display for AppStatus {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Created => write!(f, "created"),
            Self::Deployed => write!(f, "deployed"),
            Self::Stopped => write!(f, "stopped"),
            Self::Error => write!(f, "error"),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, sqlx::FromRow)]
pub struct Deployment {
    pub id: String,
    pub app_id: String,
    pub status: DeployStatus,
    pub builder: BuilderType,
    pub image_tag: Option<String>,
    pub created_at: DateTime<Utc>,
    pub finished_at: Option<DateTime<Utc>>,
}

#[derive(Debug, Clone, Serialize, Deserialize, sqlx::Type)]
#[sqlx(type_name = "TEXT")]
#[serde(rename_all = "snake_case")]
pub enum DeployStatus {
    Pending,
    Building,
    Built,
    Deploying,
    HealthChecking,
    Live,
    Failed,
    RolledBack,
}

impl std::fmt::Display for DeployStatus {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let s = match self {
            Self::Pending => "pending",
            Self::Building => "building",
            Self::Built => "built",
            Self::Deploying => "deploying",
            Self::HealthChecking => "health_checking",
            Self::Live => "live",
            Self::Failed => "failed",
            Self::RolledBack => "rolled_back",
        };
        write!(f, "{s}")
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, sqlx::Type)]
#[sqlx(type_name = "TEXT")]
#[serde(rename_all = "snake_case")]
pub enum BuilderType {
    Dockerfile,
    Nixpacks,
    Pack,
    Image,
    Archive,
    Compose,
}

impl std::fmt::Display for BuilderType {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let s = match self {
            Self::Dockerfile => "dockerfile",
            Self::Nixpacks => "nixpacks",
            Self::Pack => "pack",
            Self::Image => "image",
            Self::Archive => "archive",
            Self::Compose => "compose",
        };
        write!(f, "{s}")
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, sqlx::FromRow)]
pub struct ConfigVar {
    pub app_id: String,
    pub key: String,
    pub value: String,
    pub is_global: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize, sqlx::FromRow)]
pub struct Domain {
    pub id: String,
    pub app_id: String,
    pub domain: String,
    pub created_at: DateTime<Utc>,
}

#[derive(Debug, Clone, Serialize, Deserialize, sqlx::FromRow)]
pub struct PortMapping {
    pub id: String,
    pub app_id: String,
    pub host_port: i64,
    pub container_port: i64,
    pub protocol: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, sqlx::FromRow)]
pub struct StorageMount {
    pub id: String,
    pub app_id: String,
    pub host_path: String,
    pub container_path: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, sqlx::FromRow)]
pub struct ResourceLimit {
    pub app_id: String,
    pub process_type: String,
    pub cpu: Option<String>,
    pub memory: Option<String>,
    pub memory_swap: Option<String>,
    pub network: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, sqlx::FromRow)]
pub struct SshKey {
    pub id: String,
    pub name: String,
    pub public_key: String,
    pub created_at: DateTime<Utc>,
}

#[derive(Debug, Clone, Serialize, Deserialize, sqlx::FromRow)]
pub struct Event {
    pub id: String,
    pub app_id: Option<String>,
    pub event_type: String,
    pub payload: Option<String>,
    pub created_at: DateTime<Utc>,
}

#[derive(Debug, Clone, Serialize, Deserialize, sqlx::FromRow)]
pub struct ProcessScale {
    pub app_id: String,
    pub process_type: String,
    pub count: i64,
}

#[derive(Debug, Clone, Serialize, Deserialize, sqlx::FromRow)]
pub struct DockerOption {
    pub id: String,
    pub app_id: String,
    pub phase: String,
    pub option: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, sqlx::FromRow)]
pub struct Network {
    pub id: String,
    pub name: String,
    pub created_at: DateTime<Utc>,
}

#[derive(Debug, Clone, Serialize, Deserialize, sqlx::FromRow)]
pub struct AppNetwork {
    pub app_id: String,
    pub network_id: String,
    pub attach_phase: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct NewApp {
    pub name: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct ObjectStoreConfig {
    pub provider: String,
    pub bucket: String,
    pub region: String,
    pub endpoint: String,
    pub access_key_id: String,
    pub secret_access_key: String,
    #[serde(default = "default_object_store_path_style")]
    pub path_style: bool,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub prefix: Option<String>,
}

impl ObjectStoreConfig {
    pub fn redacted(&self) -> Self {
        let mut clone = self.clone();
        clone.secret_access_key = if self.secret_access_key.is_empty() {
            String::new()
        } else {
            "********".to_string()
        };
        clone
    }

    pub fn normalized_prefix(&self) -> Option<String> {
        self.prefix.as_ref().and_then(|prefix| {
            let trimmed = prefix.trim().trim_matches('/');
            if trimmed.is_empty() {
                None
            } else {
                Some(format!("{trimmed}/"))
            }
        })
    }
}

fn default_object_store_path_style() -> bool {
    true
}

/// Optional project-level configuration file (`deku.toml`).
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct DekuToml {
    pub build: Option<DekuBuildConfig>,
    pub deploy: Option<DekuDeployConfig>,
    pub processes: Option<std::collections::HashMap<String, u32>>,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct DekuBuildConfig {
    /// dockerfile | nixpacks | pack | image | compose | auto
    pub builder: Option<String>,
    pub dockerfile: Option<String>,
    pub context: Option<String>,
    pub args: Option<std::collections::HashMap<String, String>>,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct DekuDeployConfig {
    /// HTTP path for health checks
    pub healthcheck: Option<String>,
    /// Override auto-detected container port
    pub port: Option<u16>,
    /// Seconds to wait before running health checks
    pub wait: Option<u64>,
    /// Timeout per health check attempt (seconds)
    pub timeout: Option<u64>,
    /// Maximum health check attempts before rollback
    pub attempts: Option<u32>,
    /// Seconds to wait before retiring old containers
    pub retire: Option<u64>,
}

/// A parsed Procfile entry.
#[derive(Debug, Clone)]
pub struct ProcfileEntry {
    pub process_type: String,
    pub command: String,
}

/// Parses a Procfile from its raw text content.
pub fn parse_procfile(content: &str) -> Vec<ProcfileEntry> {
    content
        .lines()
        .filter_map(|line| {
            let line = line.trim();
            if line.is_empty() || line.starts_with('#') {
                return None;
            }
            let (proc_type, cmd) = line.split_once(':')?;
            Some(ProcfileEntry {
                process_type: proc_type.trim().to_string(),
                command: cmd.trim().to_string(),
            })
        })
        .collect()
}

/// A running (or previously running) container managed by Deku.
#[derive(Debug, Clone, Serialize, Deserialize, sqlx::FromRow)]
pub struct ContainerRecord {
    pub id: String,
    pub app_id: String,
    pub deployment_id: String,
    pub process_type: String,
    pub status: String,
    pub host_port: Option<i64>,
    pub created_at: DateTime<Utc>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Upstream {
    pub host: String,
    pub port: u16,
}

impl App {
    pub fn new(name: impl Into<String>) -> Self {
        Self {
            id: Uuid::new_v4().to_string(),
            name: name.into(),
            created_at: Utc::now(),
            locked: false,
            status: AppStatus::Created,
            tls_enabled: false,
        }
    }
}
