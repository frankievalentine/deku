use anyhow::{anyhow, Result};
use deku_core::{auth::DashboardTokenState, types::ObjectStoreConfig};
use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};

#[derive(Debug, Clone, Default, Deserialize, Serialize)]
pub struct LocalDekuConfig {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub data_dir: Option<PathBuf>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub socket_path: Option<PathBuf>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub api_port: Option<u16>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub dashboard_port: Option<u16>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub ssh_port: Option<u16>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub global_domain: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub container_backend: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub auth_secret: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub angie_conf_dir: Option<PathBuf>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub dashboard_dir: Option<PathBuf>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub object_store: Option<ObjectStoreConfig>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub dashboard_auth: Option<DashboardTokenState>,
}

pub fn default_config_dir() -> PathBuf {
    dirs_next::home_dir()
        .unwrap_or_else(|| PathBuf::from("/root"))
        .join(".deku")
}

pub fn config_dir() -> PathBuf {
    std::env::var_os("DEKU_CONFIG_DIR")
        .map(PathBuf::from)
        .unwrap_or_else(default_config_dir)
}

pub fn config_path() -> PathBuf {
    config_dir().join("config.toml")
}

pub fn load_optional() -> Result<Option<LocalDekuConfig>> {
    let path = config_path();
    if !path.exists() {
        return Ok(None);
    }

    let contents = std::fs::read_to_string(path)?;
    Ok(Some(toml::from_str(&contents)?))
}

pub fn load_required() -> Result<LocalDekuConfig> {
    load_optional()?.ok_or_else(|| anyhow!("Deku is not configured. Run `deku setup` first."))
}

pub fn save(config: &LocalDekuConfig) -> Result<()> {
    std::fs::create_dir_all(config_dir())?;
    std::fs::create_dir_all(config.effective_data_dir())?;
    let contents = toml::to_string_pretty(config)?;
    std::fs::write(config_path(), contents)?;
    Ok(())
}

impl LocalDekuConfig {
    pub fn effective_data_dir(&self) -> PathBuf {
        self.data_dir.clone().unwrap_or_else(config_dir)
    }

    pub fn effective_socket_path(&self) -> PathBuf {
        self.socket_path
            .clone()
            .unwrap_or_else(|| self.effective_data_dir().join("deku.sock"))
    }

    pub fn effective_api_port(&self) -> u16 {
        self.api_port.unwrap_or(2810)
    }

    pub fn effective_ssh_port(&self) -> u16 {
        self.ssh_port.unwrap_or(22)
    }

    pub fn effective_dashboard_url(&self) -> String {
        format!("http://127.0.0.1:{}", self.effective_api_port())
    }

    pub fn token_configured(&self) -> bool {
        self.dashboard_auth.is_some()
    }

    pub fn clear_legacy_token_file(&self) -> Result<()> {
        let token_path = self.effective_data_dir().join("cli-token");
        match std::fs::remove_file(&token_path) {
            Ok(()) => Ok(()),
            Err(err) if err.kind() == std::io::ErrorKind::NotFound => Ok(()),
            Err(err) => Err(err.into()),
        }
    }

    pub fn config_path(&self) -> PathBuf {
        config_path()
    }

    pub fn dashboard_dir_path(&self) -> PathBuf {
        self.dashboard_dir
            .clone()
            .unwrap_or_else(|| self.effective_data_dir().join("dashboard"))
    }

    pub fn ensure_dirs(&self) -> Result<()> {
        std::fs::create_dir_all(self.effective_data_dir())?;
        std::fs::create_dir_all(config_dir())?;
        Ok(())
    }
}

pub fn normalize_path_string(path: &Path) -> String {
    path.to_string_lossy().to_string()
}
