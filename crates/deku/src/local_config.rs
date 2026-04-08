use anyhow::{anyhow, Result};
use deku_core::{auth::DashboardTokenState, types::ObjectStoreConfig};
use serde::{Deserialize, Serialize};
use std::net::{Ipv4Addr, SocketAddr, UdpSocket};
use std::path::{Path, PathBuf};
use std::process::Command;

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
        self.ssh_port.unwrap_or(2222)
    }

    pub fn effective_dashboard_url(&self) -> String {
        format_dashboard_url(&self.effective_dashboard_host(), self.effective_api_port())
    }

    pub fn effective_dashboard_host(&self) -> String {
        detect_dashboard_host().unwrap_or_else(|| Ipv4Addr::LOCALHOST.to_string())
    }

    pub fn local_dashboard_url(&self) -> String {
        format_dashboard_url(&Ipv4Addr::LOCALHOST.to_string(), self.effective_api_port())
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

fn format_dashboard_url(host: &str, port: u16) -> String {
    if host.contains(':') {
        format!("http://[{host}]:{port}")
    } else {
        format!("http://{host}:{port}")
    }
}

fn detect_dashboard_host() -> Option<String> {
    std::env::var("DEKU_DASHBOARD_HOST")
        .ok()
        .map(|value| value.trim().to_string())
        .filter(|value| !value.is_empty())
        .or_else(detect_dashboard_host_udp)
        .or_else(detect_dashboard_host_hostname)
}

fn detect_dashboard_host_udp() -> Option<String> {
    let socket = UdpSocket::bind((Ipv4Addr::UNSPECIFIED, 0)).ok()?;
    socket.connect((Ipv4Addr::new(192, 0, 2, 1), 80)).ok()?;
    match socket.local_addr().ok()? {
        SocketAddr::V4(addr) if !addr.ip().is_loopback() && !addr.ip().is_unspecified() => {
            Some(addr.ip().to_string())
        }
        _ => None,
    }
}

fn detect_dashboard_host_hostname() -> Option<String> {
    let output = Command::new("hostname").arg("-I").output().ok()?;
    if !output.status.success() {
        return None;
    }

    String::from_utf8(output.stdout)
        .ok()?
        .split_whitespace()
        .find_map(|candidate| {
            candidate.parse::<Ipv4Addr>().ok().and_then(|ip| {
                if ip.is_loopback() || ip.is_unspecified() {
                    None
                } else {
                    Some(ip.to_string())
                }
            })
        })
}

#[cfg(test)]
mod tests {
    use super::{format_dashboard_url, LocalDekuConfig};

    #[test]
    fn local_dashboard_url_always_uses_loopback() {
        let config = LocalDekuConfig {
            api_port: Some(2810),
            ..LocalDekuConfig::default()
        };
        assert_eq!(config.local_dashboard_url(), "http://127.0.0.1:2810");
    }

    #[test]
    fn formats_ipv6_hosts_with_brackets() {
        assert_eq!(
            format_dashboard_url("2001:db8::10", 2810),
            "http://[2001:db8::10]:2810"
        );
    }
}
