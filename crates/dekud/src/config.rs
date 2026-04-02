use anyhow::Result;
use serde::{Deserialize, Serialize};
use std::path::PathBuf;
use tracing_appender::rolling;
use tracing_subscriber::{fmt, layer::SubscriberExt, util::SubscriberInitExt, EnvFilter};

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
}

fn default_data_dir() -> PathBuf {
    dirs_next::home_dir()
        .unwrap_or_else(|| PathBuf::from("/root"))
        .join(".deku")
}

fn default_socket_path() -> PathBuf {
    default_data_dir().join("deku.sock")
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
        }
    }
}

pub fn load() -> Result<DekuConfig> {
    let config_path = default_data_dir().join("config.toml");

    if config_path.exists() {
        let contents = std::fs::read_to_string(&config_path)?;
        let cfg: DekuConfig = toml::from_str(&contents)?;
        Ok(cfg)
    } else {
        Ok(DekuConfig::default())
    }
}

pub fn init_logging() -> Result<()> {
    let log_dir = default_data_dir().join("logs");
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
