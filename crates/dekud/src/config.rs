use anyhow::Result;
use deku_core::types::ObjectStoreConfig;
use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};
use tracing_appender::rolling;
use tracing_subscriber::{fmt, layer::SubscriberExt, util::SubscriberInitExt, EnvFilter};
use uuid::Uuid;

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct DekuConfig {
    #[serde(default = "default_data_dir")]
    pub data_dir: PathBuf,
    #[serde(default = "default_socket_path")]
    pub socket_path: PathBuf,
    #[serde(default = "default_api_port")]
    pub api_port: u16,
    #[serde(default = "default_dashboard_port")]
    pub dashboard_port: u16,
    #[serde(default = "default_ssh_port")]
    pub ssh_port: u16,
    pub global_domain: Option<String>,
    #[serde(default = "default_container_backend")]
    pub container_backend: String,
    /// HMAC secret used to sign CLI auth tokens. Generated on first startup.
    pub auth_secret: Option<String>,
    /// Directory where Angie per-app config fragments are written.
    #[serde(default = "default_angie_conf_dir")]
    pub angie_conf_dir: PathBuf,
    /// Directory where the built dashboard static files live.
    #[serde(default = "default_dashboard_dir")]
    pub dashboard_dir: PathBuf,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub object_store: Option<ObjectStoreConfig>,
}

#[derive(Debug, Default, Deserialize)]
struct RawDekuConfig {
    data_dir: Option<PathBuf>,
    socket_path: Option<PathBuf>,
    api_port: Option<u16>,
    dashboard_port: Option<u16>,
    ssh_port: Option<u16>,
    global_domain: Option<String>,
    container_backend: Option<String>,
    auth_secret: Option<String>,
    angie_conf_dir: Option<PathBuf>,
    dashboard_dir: Option<PathBuf>,
    object_store: Option<ObjectStoreConfig>,
}

fn default_config_dir() -> PathBuf {
    dirs_next::home_dir()
        .unwrap_or_else(|| PathBuf::from("/root"))
        .join(".deku")
}

fn config_dir() -> PathBuf {
    std::env::var_os("DEKU_CONFIG_DIR")
        .map(PathBuf::from)
        .unwrap_or_else(default_config_dir)
}

fn config_path() -> PathBuf {
    config_dir().join("config.toml")
}

fn default_data_dir() -> PathBuf {
    default_config_dir()
}

fn default_socket_path() -> PathBuf {
    default_socket_path_for(&default_data_dir())
}

fn default_socket_path_for(data_dir: &Path) -> PathBuf {
    data_dir.join("deku.sock")
}

fn default_api_port() -> u16 {
    2810
}

fn default_dashboard_port() -> u16 {
    2810
}

fn default_ssh_port() -> u16 {
    22
}

fn default_container_backend() -> String {
    "docker".to_string()
}

fn default_angie_conf_dir() -> PathBuf {
    PathBuf::from("/etc/angie/conf.d/deku")
}

fn default_dashboard_dir() -> PathBuf {
    default_dashboard_dir_for(&default_data_dir())
}

fn default_dashboard_dir_for(data_dir: &Path) -> PathBuf {
    data_dir.join("dashboard")
}

impl Default for DekuConfig {
    fn default() -> Self {
        Self {
            data_dir: default_data_dir(),
            socket_path: default_socket_path(),
            api_port: default_api_port(),
            dashboard_port: default_dashboard_port(),
            ssh_port: default_ssh_port(),
            global_domain: None,
            container_backend: default_container_backend(),
            auth_secret: None,
            angie_conf_dir: default_angie_conf_dir(),
            dashboard_dir: default_dashboard_dir(),
            object_store: None,
        }
    }
}

fn generate_secret() -> String {
    // Two UUIDs concatenated = 256 bits of randomness
    format!("{}{}", Uuid::new_v4().simple(), Uuid::new_v4().simple())
}

pub fn save(cfg: &DekuConfig) -> Result<()> {
    std::fs::create_dir_all(config_dir())?;
    std::fs::create_dir_all(&cfg.data_dir)?;
    let contents = toml::to_string_pretty(cfg)?;
    std::fs::write(config_path(), contents)?;
    Ok(())
}

pub fn load() -> Result<DekuConfig> {
    let config_path = config_path();

    let raw = if config_path.exists() {
        let contents = std::fs::read_to_string(&config_path)?;
        toml::from_str(&contents)?
    } else {
        RawDekuConfig::default()
    };

    let data_dir = raw.data_dir.unwrap_or_else(default_data_dir);

    let mut cfg = DekuConfig {
        data_dir: data_dir.clone(),
        socket_path: raw
            .socket_path
            .unwrap_or_else(|| default_socket_path_for(&data_dir)),
        api_port: raw.api_port.unwrap_or_else(default_api_port),
        dashboard_port: raw.dashboard_port.unwrap_or_else(default_dashboard_port),
        ssh_port: raw.ssh_port.unwrap_or_else(default_ssh_port),
        global_domain: raw.global_domain,
        container_backend: raw
            .container_backend
            .unwrap_or_else(default_container_backend),
        auth_secret: raw.auth_secret,
        angie_conf_dir: raw.angie_conf_dir.unwrap_or_else(default_angie_conf_dir),
        dashboard_dir: raw
            .dashboard_dir
            .unwrap_or_else(|| default_dashboard_dir_for(&data_dir)),
        object_store: raw.object_store,
    };

    // Generate auth secret if missing and persist it
    if cfg.auth_secret.is_none() {
        cfg.auth_secret = Some(generate_secret());
        save(&cfg)?;
    }

    Ok(cfg)
}

pub fn init_logging(cfg: &DekuConfig) -> Result<()> {
    let log_dir = cfg.data_dir.join("logs");
    std::fs::create_dir_all(&log_dir)?;

    let file_appender = rolling::daily(&log_dir, "dekud.log");

    let filter = EnvFilter::try_from_default_env().unwrap_or_else(|_| EnvFilter::new("info"));

    tracing_subscriber::registry()
        .with(filter)
        .with(fmt::layer().with_writer(file_appender).json())
        .with(fmt::layer().with_writer(std::io::stderr))
        .init();

    Ok(())
}
