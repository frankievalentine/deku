use anyhow::{anyhow, Result};
use deku_core::{auth::DashboardTokenState, types::ObjectStoreConfig};
use serde::{Deserialize, Serialize};
use std::net::{IpAddr, Ipv4Addr, SocketAddr, UdpSocket};
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
    /// Address the daemon's TCP API and dashboard listen on. Absent means every
    /// interface, which is what installs from before this setting used.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub api_bind: Option<IpAddr>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub dashboard_port: Option<u16>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub ssh_port: Option<u16>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub global_domain: Option<String>,
    /// Hostname the daemon publishes the dashboard at through Angie, over HTTPS.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub dashboard_domain: Option<String>,
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
    /// Every other setting in the file, owned by the daemon: hooks, encryption,
    /// ACME and the rest. Kept so saving from the CLI never drops them.
    #[serde(flatten)]
    pub extra: toml::Table,
}

pub fn config_dir() -> PathBuf {
    deku_core::paths::config_dir()
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
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        std::fs::set_permissions(config_dir(), std::fs::Permissions::from_mode(0o700))?;
    }
    std::fs::create_dir_all(config.effective_data_dir())?;
    let contents = toml::to_string_pretty(config)?;
    write_private_file(&config_path(), contents.as_bytes())?;
    Ok(())
}

/// Write `contents` readable by the owner only. The config holds the dashboard
/// token hash and can hold the encryption key and provider credentials.
fn write_private_file(path: &Path, contents: &[u8]) -> Result<()> {
    use std::io::Write;

    #[cfg(unix)]
    let mut file = {
        use std::os::unix::fs::{OpenOptionsExt, PermissionsExt};
        let file = std::fs::OpenOptions::new()
            .write(true)
            .create(true)
            .truncate(true)
            .mode(0o600)
            .open(path)?;
        // `mode` only applies when the file is created.
        file.set_permissions(std::fs::Permissions::from_mode(0o600))?;
        file
    };
    #[cfg(not(unix))]
    let mut file = std::fs::OpenOptions::new()
        .write(true)
        .create(true)
        .truncate(true)
        .open(path)?;

    file.write_all(contents)?;
    Ok(())
}

impl LocalDekuConfig {
    pub fn effective_data_dir(&self) -> PathBuf {
        self.data_dir
            .clone()
            .unwrap_or_else(|| deku_core::paths::default_data_dir_for(&config_dir()))
    }

    pub fn effective_socket_path(&self) -> PathBuf {
        self.socket_path
            .clone()
            .unwrap_or_else(|| self.effective_data_dir().join("deku.sock"))
    }

    pub fn effective_api_port(&self) -> u16 {
        self.api_port.unwrap_or(2810)
    }

    /// Whether the dashboard only listens on loopback, so a browser on another
    /// machine needs an SSH tunnel.
    pub fn api_is_loopback_only(&self) -> bool {
        self.api_bind.is_some_and(|ip| ip.is_loopback())
    }

    pub fn effective_ssh_port(&self) -> u16 {
        self.ssh_port.unwrap_or(2222)
    }

    /// The URL a browser opens. On a loopback-only install that is the
    /// tunnel's local end, since the server's own address does not answer.
    pub fn effective_dashboard_url(&self) -> String {
        if self.api_is_loopback_only() {
            return self.local_dashboard_url();
        }
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
    fn saving_keeps_settings_the_cli_does_not_model() {
        let original = r#"
api_port = 2810
api_bind = "127.0.0.1"

[encryption]
key_file = "/etc/deku/encryption.key"

[[hooks]]
url = "https://ci.example.test/hook"
"#;
        let config: LocalDekuConfig = toml::from_str(original).expect("parse");
        assert!(config.api_is_loopback_only());
        let written = toml::to_string_pretty(&config).expect("serialize");
        let reparsed: toml::Table = toml::from_str(&written).expect("reparse");
        assert_eq!(
            reparsed["encryption"]["key_file"].as_str(),
            Some("/etc/deku/encryption.key")
        );
        assert_eq!(
            reparsed["hooks"][0]["url"].as_str(),
            Some("https://ci.example.test/hook")
        );
        assert_eq!(reparsed["api_bind"].as_str(), Some("127.0.0.1"));
    }

    #[test]
    fn a_config_without_api_bind_listens_everywhere() {
        let config: LocalDekuConfig = toml::from_str("api_port = 2810").expect("parse");
        assert!(!config.api_is_loopback_only());
    }

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
