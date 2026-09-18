use anyhow::Result;
use deku_core::{auth::DashboardTokenState, types::ObjectStoreConfig};
use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};
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
    /// Reserved for future compatibility. Dashboard access no longer uses this field.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub auth_secret: Option<String>,
    /// Directory where Angie per-app config fragments are written.
    #[serde(default = "default_angie_conf_dir")]
    pub angie_conf_dir: PathBuf,
    /// Directory where the built dashboard static files live.
    #[serde(default = "default_dashboard_dir")]
    pub dashboard_dir: PathBuf,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub object_store: Option<ObjectStoreConfig>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub buildkit: Option<BuildkitConfig>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub dashboard_auth: Option<DashboardTokenState>,
    /// Docker registry used by remote build hosts to transfer images.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub registry: Option<RegistryConfig>,
    /// SSH build host that offloads image builds off the deploy host.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub build_host: Option<BuildHostConfig>,
    /// Out-of-process lifecycle hooks, delivered over HTTP.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub hooks: Vec<HookConfig>,
    /// Key material for encryption at rest: backups and secret config values.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub encryption: Option<EncryptionConfig>,
    /// Background alert evaluation.
    #[serde(default)]
    pub alerts: AlertsConfig,
    /// Log storage limits.
    #[serde(default)]
    pub logs: LogsConfig,
    /// Preview deployment retention.
    #[serde(default)]
    pub previews: PreviewsConfig,
    /// Automated certificate issuance for app and environment hostnames.
    #[serde(default)]
    pub acme: AcmeConfig,
}

/// Automated certificates, issued through Angie's ACME module.
///
/// Off by default: no request reaches a certificate authority until an operator
/// opts in with a provider and credentials. The credentials are used to answer
/// the DNS-01 challenge for wildcard certificates, which HTTP-01 cannot issue.
#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct AcmeConfig {
    /// Off by default. Nothing is requested from a CA until this is set.
    #[serde(default)]
    pub enabled: bool,
    /// ACME directory. Point this at a staging directory while testing.
    #[serde(default = "default_acme_directory")]
    pub directory: String,
    /// Contact address registered with the CA for expiry notices.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub email: Option<String>,
    /// DNS provider that answers the challenge. Only `cloudflare` is built in.
    #[serde(default = "default_acme_provider")]
    pub provider: String,
    /// Provider API token. Prefer `api_token_file` or `DEKU_ACME_API_TOKEN` so
    /// the secret stays out of the config file.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub api_token: Option<String>,
    /// File holding the provider API token, read on every use.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub api_token_file: Option<PathBuf>,
    /// Where Angie keeps ACME account keys and certificates. Used to tell
    /// whether a certificate has been issued yet.
    #[serde(default = "default_acme_client_path")]
    pub client_path: PathBuf,
    /// Issue a wildcard certificate for `<app>-<slug>.<global_domain>`, which
    /// covers every environment and per-deployment hostname.
    #[serde(default)]
    pub wildcard: bool,
}

impl Default for AcmeConfig {
    fn default() -> Self {
        Self {
            enabled: false,
            directory: default_acme_directory(),
            email: None,
            provider: default_acme_provider(),
            api_token: None,
            api_token_file: None,
            client_path: default_acme_client_path(),
            wildcard: false,
        }
    }
}

impl AcmeConfig {
    /// Reject a configuration that cannot work, before anything reaches a CA.
    ///
    /// `global_domain` is the daemon's configured domain, since a wildcard
    /// certificate has no name to cover without it.
    pub fn validate(&self, global_domain: Option<&str>) -> anyhow::Result<()> {
        if !self.enabled {
            return Ok(());
        }

        if self.provider != "cloudflare" {
            anyhow::bail!(
                "unknown ACME DNS provider '{}'; supported providers: cloudflare",
                self.provider
            );
        }

        if !self.directory.starts_with("https://") {
            anyhow::bail!("ACME directory '{}' must be an https URL", self.directory);
        }

        if self.api_token.is_none() && self.api_token_file.is_none() {
            anyhow::bail!(
                "ACME is enabled but no API token is configured; set api_token_file or DEKU_ACME_API_TOKEN"
            );
        }

        if self.wildcard && global_domain.is_none() {
            anyhow::bail!(
                "ACME wildcard is enabled but global_domain is not set, so there is no name to certify"
            );
        }

        Ok(())
    }
}

fn default_acme_directory() -> String {
    "https://acme-v02.api.letsencrypt.org/directory".to_string()
}

fn default_acme_provider() -> String {
    "cloudflare".to_string()
}

fn default_acme_client_path() -> PathBuf {
    PathBuf::from("/var/lib/angie/acme")
}

/// How many deployments per environment stay reachable at their own URL.
///
/// A replaced deployment's containers are kept running so its per-deployment
/// URL keeps serving the build it named. Only deployments beyond this count are
/// retired, and lowering it retires the excess on the next deploy.
#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct PreviewsConfig {
    /// Deployments kept per environment, counting the live one. A value below 1
    /// is treated as 1, which keeps only the live deployment.
    #[serde(default = "default_keep_deployments")]
    pub keep_deployments: usize,
}

impl Default for PreviewsConfig {
    fn default() -> Self {
        Self {
            keep_deployments: default_keep_deployments(),
        }
    }
}

fn default_keep_deployments() -> usize {
    3
}

/// Settings for stored deployment logs.
#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct LogsConfig {
    /// Lines kept per app; older lines are pruned on a timer.
    #[serde(default = "default_log_retain_lines")]
    pub retain_lines: i64,
}

impl Default for LogsConfig {
    fn default() -> Self {
        Self {
            retain_lines: default_log_retain_lines(),
        }
    }
}

fn default_log_retain_lines() -> i64 {
    crate::logs::DEFAULT_RETAIN_LINES
}

/// Settings for the fixed-rule alert watcher.
#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct AlertsConfig {
    /// Evaluate rules on a timer. Disable to rely on `/api/metrics` alone.
    #[serde(default = "default_alerts_enabled")]
    pub enabled: bool,
    /// Seconds between evaluations.
    #[serde(default = "default_alert_interval_secs")]
    pub interval_secs: u64,
    /// Filesystem usage that raises a warning.
    #[serde(default = "default_disk_warn_percent")]
    pub disk_warn_percent: u8,
    /// Filesystem usage that raises a critical alert.
    #[serde(default = "default_disk_critical_percent")]
    pub disk_critical_percent: u8,
}

impl Default for AlertsConfig {
    fn default() -> Self {
        Self {
            enabled: default_alerts_enabled(),
            interval_secs: default_alert_interval_secs(),
            disk_warn_percent: default_disk_warn_percent(),
            disk_critical_percent: default_disk_critical_percent(),
        }
    }
}

fn default_alerts_enabled() -> bool {
    true
}

fn default_alert_interval_secs() -> u64 {
    300
}

fn default_disk_warn_percent() -> u8 {
    85
}

fn default_disk_critical_percent() -> u8 {
    95
}

/// Key material for encryption at rest.
///
/// One key covers every encrypted payload the daemon writes: service backups
/// uploaded to an object store, and config var values stored in the database.
#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct EncryptionConfig {
    /// Inline key: 64 hex characters or base64-encoded 32 bytes.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub key: Option<String>,
    /// File containing the key, read on every use. Prefer this over `key` so
    /// the secret stays out of the config file.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub key_file: Option<PathBuf>,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct HookConfig {
    /// Endpoint that receives lifecycle events.
    pub url: String,
    /// Shared secret. When set, each request carries an HMAC-SHA256 signature
    /// of the body in `X-Deku-Signature`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub secret: Option<String>,
    /// Events to deliver, e.g. `["pre_deploy", "deploy.failed"]`. Omit for all.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub events: Option<Vec<String>>,
    /// When true, a failing hook fails a `pre_build` or `pre_deploy` event.
    #[serde(default)]
    pub blocking: bool,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct RegistryConfig {
    /// Registry host plus optional repository owner path, e.g. `ghcr.io/acme`.
    pub server: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub username: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub password: Option<String>,
    /// Repository prefix inserted between the server and the app name.
    #[serde(default = "default_registry_namespace")]
    pub namespace: String,
}

impl RegistryConfig {
    pub fn redacted(&self) -> Self {
        let mut clone = self.clone();
        clone.password = match self.password.as_deref() {
            Some(password) if !password.is_empty() => Some("********".to_string()),
            other => other.map(str::to_string),
        };
        clone
    }

    pub fn has_credentials(&self) -> bool {
        self.username
            .as_deref()
            .is_some_and(|u| !u.trim().is_empty())
            && self.password.as_deref().is_some_and(|p| !p.is_empty())
    }

    /// Repository (without tag) for an app, e.g. `ghcr.io/acme/deku/myapp`.
    pub fn repository(&self, app_name: &str) -> String {
        let server = self.server.trim().trim_end_matches('/');
        let namespace = self.namespace.trim().trim_matches('/');
        if namespace.is_empty() {
            format!("{server}/{app_name}")
        } else {
            format!("{server}/{namespace}/{app_name}")
        }
    }

    /// Immutable image reference for a deployment.
    pub fn image_reference(&self, app_name: &str, deploy_id: &str) -> String {
        format!(
            "{}:{}",
            self.repository(app_name),
            crate::build::deployment_image_id(deploy_id)
        )
    }
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct BuildHostConfig {
    /// Logical name accepted by `--build-host`.
    #[serde(default = "default_build_host_name")]
    pub name: String,
    /// SSH destination, `user@host` or `ssh://user@host[:port]`.
    pub host: String,
    /// Private key handed to `ssh -i`. Falls back to the local SSH config/agent.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub identity_file: Option<PathBuf>,
    /// BuildKit endpoint used by railpack on the build host.
    #[serde(default = "default_remote_buildkit_host")]
    pub buildkit_host: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SshTarget {
    pub destination: String,
    pub port: Option<u16>,
}

impl BuildHostConfig {
    pub fn ssh_target(&self) -> SshTarget {
        let trimmed = self.host.trim();
        let raw = trimmed.strip_prefix("ssh://").unwrap_or(trimmed);
        match raw.rsplit_once(':') {
            Some((host, port))
                if !host.contains(':')
                    && !port.is_empty()
                    && port.chars().all(|c| c.is_ascii_digit()) =>
            {
                SshTarget {
                    destination: host.to_string(),
                    port: port.parse().ok(),
                }
            }
            _ => SshTarget {
                destination: raw.to_string(),
                port: None,
            },
        }
    }
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct BuildkitConfig {
    /// When true, `dekud` starts and manages a BuildKit container for Railpack.
    #[serde(default = "default_buildkit_managed")]
    pub managed: bool,
    /// Container image used for the managed BuildKit daemon.
    #[serde(default = "default_buildkit_image")]
    pub image: String,
    /// Container name of the managed BuildKit daemon.
    #[serde(default = "default_buildkit_container_name")]
    pub container_name: String,
    /// Explicit BuildKit endpoint. When set, this overrides the managed container.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub host: Option<String>,
}

impl Default for BuildkitConfig {
    fn default() -> Self {
        Self {
            managed: default_buildkit_managed(),
            image: default_buildkit_image(),
            container_name: default_buildkit_container_name(),
            host: None,
        }
    }
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
    dashboard_auth: Option<DashboardTokenState>,
    buildkit: Option<BuildkitConfig>,
    registry: Option<RegistryConfig>,
    build_host: Option<BuildHostConfig>,
    hooks: Option<Vec<HookConfig>>,
    encryption: Option<EncryptionConfig>,
    alerts: Option<AlertsConfig>,
    logs: Option<LogsConfig>,
    previews: Option<PreviewsConfig>,
    acme: Option<AcmeConfig>,
}

fn default_config_dir() -> PathBuf {
    dirs::home_dir()
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
    2222
}

fn default_container_backend() -> String {
    "docker".to_string()
}

fn default_buildkit_managed() -> bool {
    true
}

fn default_buildkit_image() -> String {
    "moby/buildkit:v0.33.0".to_string()
}

fn default_buildkit_container_name() -> String {
    "deku-buildkit".to_string()
}

fn default_registry_namespace() -> String {
    "deku".to_string()
}

fn default_build_host_name() -> String {
    "builder".to_string()
}

fn default_remote_buildkit_host() -> String {
    "docker-container://deku-buildkit".to_string()
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
            buildkit: None,
            dashboard_auth: None,
            registry: None,
            build_host: None,
            hooks: Vec::new(),
            encryption: None,
            alerts: AlertsConfig::default(),
            logs: LogsConfig::default(),
            previews: PreviewsConfig::default(),
            acme: AcmeConfig::default(),
        }
    }
}

/// Where a resolved ACME API token was read from.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum TokenSource {
    Environment,
    Inline,
    File,
}

/// An ACME API token and where it came from, so a surface can report its origin
/// without ever returning the secret.
#[derive(Debug, Clone)]
pub struct AcmeToken {
    pub value: String,
    pub source: TokenSource,
}

impl DekuConfig {
    /// Resolve the ACME DNS provider token, if one is configured.
    ///
    /// Precedence is `DEKU_ACME_API_TOKEN`, then `[acme] api_token`, then
    /// `api_token_file`. `Ok(None)` means the token has not been provided yet,
    /// which is only an error once ACME is enabled.
    pub fn acme_api_token(&self) -> Result<Option<AcmeToken>> {
        if let Ok(value) = std::env::var("DEKU_ACME_API_TOKEN") {
            let value = value.trim();
            if !value.is_empty() {
                return Ok(Some(AcmeToken {
                    value: value.to_string(),
                    source: TokenSource::Environment,
                }));
            }
        }

        if let Some(token) = self.acme.api_token.as_deref() {
            let token = token.trim();
            if !token.is_empty() {
                return Ok(Some(AcmeToken {
                    value: token.to_string(),
                    source: TokenSource::Inline,
                }));
            }
        }

        match self.acme.api_token_file.as_ref() {
            Some(path) => {
                let value = std::fs::read_to_string(path).map_err(|error| {
                    anyhow::anyhow!(
                        "failed to read ACME API token file {}: {error}",
                        path.display()
                    )
                })?;
                let value = value.trim();
                if value.is_empty() {
                    anyhow::bail!("ACME API token file {} is empty", path.display());
                }
                Ok(Some(AcmeToken {
                    value: value.to_string(),
                    source: TokenSource::File,
                }))
            }
            None => Ok(None),
        }
    }

    /// Resolve the encryption-at-rest key, if one is configured.
    ///
    /// Precedence is `DEKU_ENCRYPTION_KEY`, then `[encryption] key`, then
    /// `key_file`. `Ok(None)` means payloads are stored in the clear.
    pub fn at_rest_cipher(&self) -> Result<Option<crate::crypto::AtRestCipher>> {
        let raw = match std::env::var("DEKU_ENCRYPTION_KEY") {
            Ok(value) if !value.trim().is_empty() => Some(value),
            _ => match self.encryption.as_ref() {
                Some(cfg) => match (cfg.key.as_ref(), cfg.key_file.as_ref()) {
                    (Some(key), _) => Some(key.clone()),
                    (None, Some(path)) => Some(std::fs::read_to_string(path).map_err(|error| {
                        anyhow::anyhow!(
                            "failed to read encryption key file {}: {error}",
                            path.display()
                        )
                    })?),
                    (None, None) => None,
                },
                None => None,
            },
        };

        match raw {
            Some(value) => {
                let key = crate::crypto::parse_key(&value)?;
                Ok(Some(crate::crypto::AtRestCipher::from_key_bytes(&key)?))
            }
            None => Ok(None),
        }
    }
}

pub fn save(cfg: &DekuConfig) -> Result<()> {
    let dir = config_dir();
    std::fs::create_dir_all(&dir)?;
    secure_dir(&dir)?;
    std::fs::create_dir_all(&cfg.data_dir)?;
    secure_dir(&cfg.data_dir)?;
    let contents = toml::to_string_pretty(cfg)?;
    write_private_file(&config_path(), contents.as_bytes())?;
    Ok(())
}

pub(crate) fn secure_dir(path: &Path) -> Result<()> {
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        std::fs::set_permissions(path, std::fs::Permissions::from_mode(0o700))?;
    }
    Ok(())
}

pub(crate) fn write_private_file(path: &Path, contents: &[u8]) -> Result<()> {
    use std::io::Write;

    #[cfg(unix)]
    let mut file = {
        use std::os::unix::fs::OpenOptionsExt;
        std::fs::OpenOptions::new()
            .write(true)
            .create(true)
            .truncate(true)
            .mode(0o600)
            .open(path)?
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

pub fn load() -> Result<DekuConfig> {
    let config_path = config_path();

    let raw = if config_path.exists() {
        let contents = std::fs::read_to_string(&config_path)?;
        toml::from_str(&contents)?
    } else {
        RawDekuConfig::default()
    };

    let data_dir = raw.data_dir.unwrap_or_else(default_data_dir);

    let cfg = DekuConfig {
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
        buildkit: raw.buildkit,
        dashboard_auth: raw.dashboard_auth,
        registry: raw.registry,
        build_host: raw.build_host,
        hooks: raw.hooks.unwrap_or_default(),
        encryption: raw.encryption,
        alerts: raw.alerts.unwrap_or_default(),
        logs: raw.logs.unwrap_or_default(),
        previews: raw.previews.unwrap_or_default(),
        acme: raw.acme.unwrap_or_default(),
    };

    Ok(cfg)
}

pub fn init_logging(cfg: &DekuConfig) -> Result<()> {
    let log_dir = cfg.data_dir.join("logs");
    std::fs::create_dir_all(&log_dir)?;
    secure_dir(&log_dir)?;

    let file_appender = rolling::daily(&log_dir, "dekud.log");

    let filter = EnvFilter::try_from_default_env().unwrap_or_else(|_| EnvFilter::new("info"));

    tracing_subscriber::registry()
        .with(filter)
        .with(fmt::layer().with_writer(file_appender).json())
        .with(fmt::layer().with_writer(std::io::stderr))
        .init();

    Ok(())
}

pub fn dashboard_assets_available(cfg: &DekuConfig) -> bool {
    cfg.dashboard_dir.join("index.html").is_file()
}

#[cfg(test)]
mod tests {

    #[test]
    fn at_rest_cipher_is_absent_without_configuration() {
        let cfg = DekuConfig {
            encryption: None,
            ..DekuConfig::default()
        };
        assert!(cfg.at_rest_cipher().expect("resolves").is_none());
    }

    #[test]
    fn at_rest_cipher_accepts_an_inline_hex_key() {
        let cfg = DekuConfig {
            encryption: Some(EncryptionConfig {
                key: Some("0123456789abcdef".repeat(4)),
                key_file: None,
            }),
            ..DekuConfig::default()
        };
        assert!(cfg.at_rest_cipher().expect("resolves").is_some());
    }

    #[test]
    fn at_rest_cipher_reads_a_key_file() {
        let dir = tempfile::tempdir().expect("tempdir");
        let path = dir.path().join("backup.key");
        std::fs::write(&path, "ab".repeat(32)).expect("write key");
        let cfg = DekuConfig {
            encryption: Some(EncryptionConfig {
                key: None,
                key_file: Some(path),
            }),
            ..DekuConfig::default()
        };
        assert!(cfg.at_rest_cipher().expect("resolves").is_some());
    }

    #[test]
    fn at_rest_cipher_rejects_a_bad_key() {
        let cfg = DekuConfig {
            encryption: Some(EncryptionConfig {
                key: Some("too-short".to_string()),
                key_file: None,
            }),
            ..DekuConfig::default()
        };
        assert!(cfg.at_rest_cipher().is_err());
    }

    #[test]
    fn at_rest_cipher_reports_a_missing_key_file() {
        let cfg = DekuConfig {
            encryption: Some(EncryptionConfig {
                key: None,
                key_file: Some(PathBuf::from("/nonexistent/deku/backup.key")),
            }),
            ..DekuConfig::default()
        };
        let error = cfg.at_rest_cipher().expect_err("missing file should fail");
        assert!(error
            .to_string()
            .contains("failed to read encryption key file"));
    }
    use super::{
        secure_dir, write_private_file, AcmeConfig, DekuConfig, EncryptionConfig, TokenSource,
    };
    use std::path::PathBuf;

    #[cfg(unix)]
    #[test]
    fn private_config_file_and_dir_permissions() {
        use std::os::unix::fs::PermissionsExt;

        let temp = tempfile::tempdir().expect("tempdir");
        let dir = temp.path().join("cfg");
        std::fs::create_dir_all(&dir).expect("dir");

        secure_dir(&dir).expect("secure dir");
        let dir_mode = std::fs::metadata(&dir)
            .expect("dir meta")
            .permissions()
            .mode()
            & 0o777;
        assert_eq!(dir_mode, 0o700);

        let file = dir.join("config.toml");
        write_private_file(&file, b"secret").expect("write");
        let file_mode = std::fs::metadata(&file)
            .expect("file meta")
            .permissions()
            .mode()
            & 0o777;
        assert_eq!(file_mode, 0o600);

        write_private_file(&file, b"secret-again").expect("rewrite");
        let file_mode = std::fs::metadata(&file)
            .expect("file meta")
            .permissions()
            .mode()
            & 0o777;
        assert_eq!(file_mode, 0o600);
    }

    #[test]
    fn registry_reference_uses_namespace_and_deploy_id() {
        let registry = super::RegistryConfig {
            server: "ghcr.io/acme/".to_string(),
            username: Some("acme".to_string()),
            password: Some("hunter2".to_string()),
            namespace: "deku".to_string(),
        };

        assert_eq!(registry.repository("demo"), "ghcr.io/acme/deku/demo");
        assert_eq!(
            registry.image_reference("demo", "3f2504e0-4f89-11d3-9a0c-0305e82c3301"),
            "ghcr.io/acme/deku/demo:3f2504e04f89"
        );
        assert!(registry.has_credentials());

        let redacted = registry.redacted();
        assert_eq!(redacted.password.as_deref(), Some("********"));
        assert_eq!(redacted.server, "ghcr.io/acme/");
    }

    #[test]
    fn registry_without_namespace_puts_app_after_server() {
        let registry = super::RegistryConfig {
            server: "registry.internal:5000".to_string(),
            username: None,
            password: None,
            namespace: String::new(),
        };

        assert_eq!(registry.repository("demo"), "registry.internal:5000/demo");
        assert!(!registry.has_credentials());
    }

    #[test]
    fn build_host_parses_ssh_scheme_and_port() {
        let target = super::BuildHostConfig {
            name: "builder".to_string(),
            host: "ssh://deku@builder.internal:2222".to_string(),
            identity_file: Some(PathBuf::from("/root/.deku/build_key")),
            buildkit_host: "docker-container://deku-buildkit".to_string(),
        }
        .ssh_target();

        assert_eq!(target.destination, "deku@builder.internal");
        assert_eq!(target.port, Some(2222));
    }

    #[test]
    fn build_host_defaults_port_when_absent() {
        let target = super::BuildHostConfig {
            name: "builder".to_string(),
            host: "deku@10.0.0.5".to_string(),
            identity_file: None,
            buildkit_host: "docker-container://deku-buildkit".to_string(),
        }
        .ssh_target();

        assert_eq!(target.destination, "deku@10.0.0.5");
        assert_eq!(target.port, None);
    }

    #[test]
    fn acme_is_off_and_unconfigured_by_default() {
        let cfg = AcmeConfig::default();
        assert!(
            !cfg.enabled,
            "nothing should reach a CA until it is turned on"
        );
        assert!(!cfg.wildcard);
        assert_eq!(cfg.provider, "cloudflare");
        assert_eq!(
            cfg.directory,
            "https://acme-v02.api.letsencrypt.org/directory"
        );
        assert_eq!(
            cfg.client_path,
            std::path::PathBuf::from("/var/lib/angie/acme")
        );
        // A disabled configuration needs nothing else to be valid.
        cfg.validate(Some("apps.test")).expect("disabled is valid");
        cfg.validate(None)
            .expect("disabled is valid without a domain");
    }

    #[test]
    fn acme_accepts_a_complete_configuration() {
        let cfg = AcmeConfig {
            enabled: true,
            api_token: Some("token".to_string()),
            wildcard: true,
            ..AcmeConfig::default()
        };
        cfg.validate(Some("apps.test")).expect("valid");
    }

    #[test]
    fn acme_rejects_a_provider_it_cannot_drive() {
        let cfg = AcmeConfig {
            enabled: true,
            provider: "route53".to_string(),
            api_token: Some("token".to_string()),
            ..AcmeConfig::default()
        };
        let error = cfg
            .validate(Some("apps.test"))
            .expect_err("unknown provider");
        assert!(error.to_string().contains("route53"), "{error}");
        assert!(
            error.to_string().contains("cloudflare"),
            "the error should name what is supported: {error}"
        );
    }

    #[test]
    fn acme_requires_a_token_and_an_https_directory() {
        let no_token = AcmeConfig {
            enabled: true,
            ..AcmeConfig::default()
        };
        assert!(no_token
            .validate(Some("apps.test"))
            .expect_err("no token")
            .to_string()
            .contains("API token"));

        let insecure = AcmeConfig {
            enabled: true,
            api_token: Some("token".to_string()),
            directory: "http://acme.test/directory".to_string(),
            ..AcmeConfig::default()
        };
        assert!(insecure
            .validate(Some("apps.test"))
            .expect_err("plain http")
            .to_string()
            .contains("https"));
    }

    #[test]
    fn acme_wildcard_needs_a_global_domain() {
        let cfg = AcmeConfig {
            enabled: true,
            api_token: Some("token".to_string()),
            wildcard: true,
            ..AcmeConfig::default()
        };
        let error = cfg.validate(None).expect_err("no domain to certify");
        assert!(error.to_string().contains("global_domain"), "{error}");
    }

    #[test]
    fn acme_token_reads_inline_and_from_a_file() {
        let inline = DekuConfig {
            acme: AcmeConfig {
                api_token: Some("inline-token".to_string()),
                ..AcmeConfig::default()
            },
            ..DekuConfig::default()
        };
        let token = inline.acme_api_token().expect("token").expect("token");
        assert_eq!(token.value, "inline-token");
        assert_eq!(token.source, TokenSource::Inline);

        let dir = tempfile::tempdir().expect("tempdir");
        let path = dir.path().join("cloudflare.token");
        // A trailing newline from an editor or `echo` must not become part of
        // the token.
        std::fs::write(&path, "file-token\n").expect("write token");
        let from_file = DekuConfig {
            acme: AcmeConfig {
                api_token_file: Some(path),
                ..AcmeConfig::default()
            },
            ..DekuConfig::default()
        };
        let token = from_file.acme_api_token().expect("token").expect("token");
        assert_eq!(token.value, "file-token");
        assert_eq!(token.source, TokenSource::File);
    }

    #[test]
    fn acme_token_file_that_cannot_be_read_is_an_error() {
        let dir = tempfile::tempdir().expect("tempdir");
        let cfg = DekuConfig {
            acme: AcmeConfig {
                api_token_file: Some(dir.path().join("missing.token")),
                ..AcmeConfig::default()
            },
            ..DekuConfig::default()
        };
        let error = cfg.acme_api_token().expect_err("missing file");
        assert!(error.to_string().contains("missing.token"), "{error}");
    }
}
