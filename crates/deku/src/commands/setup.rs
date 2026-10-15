use anyhow::Result;
use chrono::Utc;
use clap::Args;
use deku_core::{auth::issue_dashboard_token, types::ObjectStoreConfig};
use std::net::{IpAddr, Ipv4Addr};
use std::path::PathBuf;

use crate::{
    commands::dashboard::print_token_notice,
    local_config::{config_dir, load_optional, normalize_path_string, save, LocalDekuConfig},
};

#[derive(Debug, Clone, Args)]
pub struct SetupArgs {
    /// Skip the interactive prompt to install a systemd unit.
    #[arg(long)]
    pub no_systemd: bool,
    /// Use provided flags or built-in defaults without interactive prompts.
    #[arg(long)]
    pub defaults: bool,
    #[arg(long)]
    pub data_dir: Option<String>,
    #[arg(long)]
    pub api_port: Option<u16>,
    /// Address the dashboard and TCP API listen on. Defaults to loopback, so
    /// reaching the dashboard from another machine takes an SSH tunnel; use
    /// `0.0.0.0` to listen on every interface.
    #[arg(long)]
    pub api_bind: Option<IpAddr>,
    #[arg(long)]
    pub ssh_port: Option<u16>,
    #[arg(long)]
    pub angie_conf_dir: Option<String>,
    #[arg(long)]
    pub global_domain: Option<String>,
    #[arg(long, hide = true)]
    pub installer: bool,
    #[arg(long, hide = true)]
    pub token_output: Option<PathBuf>,
}

pub fn run(args: SetupArgs) -> Result<()> {
    cliclack::intro("Deku setup")?;
    let skip_systemd = args.no_systemd;
    let use_defaults = args.defaults;

    let config_dir = config_dir();
    let default_data = deku_core::paths::default_data_dir_for(&config_dir)
        .to_string_lossy()
        .to_string();

    let data_dir = match args.data_dir {
        Some(value) => value,
        None if use_defaults => default_data.clone(),
        None => cliclack::input("Data directory")
            .default_input(&default_data)
            .interact()?,
    };

    let api_port = match args.api_port {
        Some(value) => value,
        None if use_defaults => 2810,
        None => {
            let api_port_str: String = cliclack::input("API port")
                .default_input("2810")
                .interact()?;
            api_port_str.parse().unwrap_or(2810)
        }
    };

    let api_bind = match args.api_bind {
        Some(value) => value,
        None if use_defaults => IpAddr::V4(Ipv4Addr::LOCALHOST),
        None => {
            let bind_str: String =
                cliclack::input("Dashboard listen address (0.0.0.0 for every interface)")
                    .default_input("127.0.0.1")
                    .interact()?;
            bind_str.parse().unwrap_or(IpAddr::V4(Ipv4Addr::LOCALHOST))
        }
    };

    let ssh_port = match args.ssh_port {
        Some(value) => value,
        None if use_defaults => 2222,
        None => {
            let ssh_port_str: String = cliclack::input("SSH port")
                .default_input("2222")
                .interact()?;
            ssh_port_str.parse().unwrap_or(2222)
        }
    };

    let angie_conf_dir = match args.angie_conf_dir {
        Some(value) => value,
        None if use_defaults => "/etc/angie/conf.d/deku".to_string(),
        None => cliclack::input("Angie config directory")
            .default_input("/etc/angie/conf.d/deku")
            .interact()?,
    };

    let global_domain = if let Some(global_domain) = args.global_domain {
        if global_domain.trim().is_empty() {
            None
        } else {
            Some(global_domain)
        }
    } else if use_defaults {
        None
    } else {
        let global_domain_str: String = cliclack::input("Global domain (leave blank to skip)")
            .default_input("")
            .interact()?;
        if global_domain_str.is_empty() {
            None
        } else {
            Some(global_domain_str)
        }
    };

    let configure_object_store = if use_defaults {
        false
    } else {
        cliclack::confirm("Configure object storage?")
            .initial_value(false)
            .interact()?
    };
    let object_store = if configure_object_store {
        let provider: String = cliclack::input("Object store provider")
            .default_input("r2")
            .interact()?;
        let bucket: String = cliclack::input("Object store bucket")
            .default_input("deku")
            .interact()?;
        let region_default = if provider == "r2" {
            "auto"
        } else {
            "us-east-1"
        };
        let region: String = cliclack::input("Object store region")
            .default_input(region_default)
            .interact()?;
        let endpoint: String = cliclack::input("Object store endpoint")
            .default_input("https://example.com")
            .interact()?;
        let access_key_id: String = cliclack::input("Object store access key ID").interact()?;
        let secret_access_key: String =
            cliclack::input("Object store secret access key").interact()?;
        let prefix_raw: String = cliclack::input("Object store prefix (leave blank to skip)")
            .default_input("")
            .interact()?;
        let path_style = cliclack::confirm("Use path-style bucket addressing?")
            .initial_value(provider == "r2")
            .interact()?;

        Some(ObjectStoreConfig {
            provider,
            bucket,
            region,
            endpoint,
            access_key_id,
            secret_access_key,
            path_style,
            prefix: if prefix_raw.trim().is_empty() {
                None
            } else {
                Some(prefix_raw)
            },
        })
    } else {
        None
    };

    let existing = load_optional()?;
    let fresh_install = existing.is_none();
    let existing_dashboard_auth = existing
        .as_ref()
        .and_then(|config| config.dashboard_auth.clone());
    // Settings setup does not ask about (hooks, encryption, ACME) survive a rerun.
    let mut existing_extra = existing.map(|config| config.extra).unwrap_or_default();
    ensure_encryption_key(&config_dir, &mut existing_extra)?;
    if fresh_install {
        enable_custom_domain_certificates(&mut existing_extra);
    }
    let (dashboard_token, dashboard_auth) =
        issue_dashboard_token(Utc::now(), existing_dashboard_auth.as_ref())?;

    if let Some(path) = &args.token_output {
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent)?;
        }
        std::fs::write(path, format!("{dashboard_token}\n"))?;
    }

    let cfg = LocalDekuConfig {
        data_dir: Some(PathBuf::from(&data_dir)),
        api_port: Some(api_port),
        api_bind: Some(api_bind),
        ssh_port: Some(ssh_port),
        angie_conf_dir: Some(PathBuf::from(&angie_conf_dir)),
        global_domain,
        object_store,
        dashboard_auth: Some(dashboard_auth),
        extra: existing_extra,
        ..LocalDekuConfig::default()
    };

    cfg.ensure_dirs()?;
    cfg.clear_legacy_token_file()?;
    std::fs::create_dir_all(&config_dir)?;
    let config_path = config_dir.join("config.toml");
    save(&cfg)?;

    cliclack::log::success(format!("Config written to {}", config_path.display()))?;

    #[cfg(target_os = "linux")]
    {
        let install_systemd = if skip_systemd {
            false
        } else {
            cliclack::confirm("Install systemd service?")
                .initial_value(true)
                .interact()?
        };

        if install_systemd {
            install_systemd_unit()?;
        }
    }

    #[cfg(not(target_os = "linux"))]
    let _ = skip_systemd;

    if args.installer {
        return Ok(());
    }

    println!();
    print_token_notice(&cfg, &dashboard_token, false);
    println!(
        "Dashboard assets: {}",
        normalize_path_string(&cfg.dashboard_dir_path())
    );
    cliclack::outro("Setup complete. Start `dekud` to serve the dashboard.")?;
    Ok(())
}

/// Give a new install an encryption-at-rest key, so config var values and
/// backups are sealed from the first one written.
///
/// The key lives in its own owner-only file next to the config rather than
/// inline, so it stays out of anything that copies `config.toml`, and outside
/// the state directory on a system install, so a backup of the database alone
/// does not carry the key that opens it. An existing `[encryption]` section or
/// a `DEKU_ENCRYPTION_KEY` in the environment is left alone.
/// Turn on automatic certificates for app domains in a new install's config.
///
/// Only a new install: an existing host serves its domains the way it already
/// does until the operator opts in, because turning this on changes what
/// answers on port 80 for those domains.
fn enable_custom_domain_certificates(extra: &mut toml::Table) {
    let acme = extra
        .entry("acme")
        .or_insert_with(|| toml::Value::Table(toml::Table::new()));
    if let toml::Value::Table(acme) = acme {
        acme.entry("enabled").or_insert(toml::Value::Boolean(true));
        acme.entry("custom_domains")
            .or_insert(toml::Value::Boolean(true));
    }
}

fn ensure_encryption_key(config_dir: &std::path::Path, extra: &mut toml::Table) -> Result<()> {
    if extra.contains_key("encryption") || std::env::var_os("DEKU_ENCRYPTION_KEY").is_some() {
        return Ok(());
    }

    let key_path = config_dir.join("encryption.key");
    if !key_path.exists() {
        use rand::RngExt;
        use std::io::Write;

        std::fs::create_dir_all(config_dir)?;
        let mut key = [0u8; 32];
        rand::rng().fill(&mut key);
        let hex: String = key.iter().map(|byte| format!("{byte:02x}")).collect();

        let mut options = std::fs::OpenOptions::new();
        options.write(true).create_new(true);
        #[cfg(unix)]
        {
            use std::os::unix::fs::OpenOptionsExt;
            options.mode(0o600);
        }
        let mut file = options.open(&key_path)?;
        file.write_all(format!("{hex}\n").as_bytes())?;
        cliclack::log::info(format!(
            "Encryption key written to {}; back it up separately from the database",
            key_path.display()
        ))?;
    }

    let mut encryption = toml::Table::new();
    encryption.insert(
        "key_file".to_string(),
        toml::Value::String(key_path.to_string_lossy().into_owned()),
    );
    extra.insert("encryption".to_string(), toml::Value::Table(encryption));
    Ok(())
}

#[cfg(target_os = "linux")]
fn install_systemd_unit() -> Result<()> {
    let bin_dir = if deku_core::dev_hooks::enabled() {
        std::env::var("DEKU_BIN_DIR").unwrap_or_else(|_| "/usr/local/bin".to_string())
    } else {
        "/usr/local/bin".to_string()
    };
    // The same unit the installer writes, so the two never drift apart.
    let unit = include_str!("../../../../scripts/package/deku.service")
        .replace("__DEKU_INSTALL_DIR__", &bin_dir);

    let unit_path = std::path::Path::new("/etc/systemd/system/deku.service");
    std::fs::write(unit_path, unit)?;
    cliclack::log::success("systemd unit written to /etc/systemd/system/deku.service")?;
    cliclack::log::info("Run: systemctl enable --now deku")?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::{enable_custom_domain_certificates, ensure_encryption_key};

    #[test]
    fn a_new_install_requests_certificates_for_app_domains() {
        let mut extra = toml::Table::new();
        enable_custom_domain_certificates(&mut extra);
        assert_eq!(extra["acme"]["enabled"].as_bool(), Some(true));
        assert_eq!(extra["acme"]["custom_domains"].as_bool(), Some(true));

        // Anything already chosen is kept.
        let mut chosen: toml::Table =
            toml::from_str("[acme]\ncustom_domains = false\n").expect("toml");
        enable_custom_domain_certificates(&mut chosen);
        assert_eq!(chosen["acme"]["custom_domains"].as_bool(), Some(false));
    }

    #[test]
    fn a_new_install_gets_an_owner_only_key_file() {
        let dir = tempfile::tempdir().expect("tempdir");
        let mut extra = toml::Table::new();
        ensure_encryption_key(dir.path(), &mut extra).expect("key");

        let key_path = dir.path().join("encryption.key");
        let key = std::fs::read_to_string(&key_path).expect("key file");
        assert_eq!(key.trim().len(), 64);
        assert!(key.trim().chars().all(|c| c.is_ascii_hexdigit()));
        assert_eq!(
            extra["encryption"]["key_file"].as_str(),
            Some(key_path.to_string_lossy().as_ref())
        );

        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            let mode = std::fs::metadata(&key_path)
                .expect("metadata")
                .permissions()
                .mode();
            assert_eq!(mode & 0o777, 0o600);
        }
    }

    #[test]
    fn an_existing_key_is_kept() {
        let dir = tempfile::tempdir().expect("tempdir");
        let mut extra = toml::Table::new();
        ensure_encryption_key(dir.path(), &mut extra).expect("first");
        let first = std::fs::read_to_string(dir.path().join("encryption.key")).expect("key");

        let mut rerun = toml::Table::new();
        ensure_encryption_key(dir.path(), &mut rerun).expect("rerun");
        let second = std::fs::read_to_string(dir.path().join("encryption.key")).expect("key");
        assert_eq!(first, second, "a rerun must not replace the key");
    }

    #[test]
    fn a_configured_encryption_section_is_left_alone() {
        let dir = tempfile::tempdir().expect("tempdir");
        let mut extra: toml::Table =
            toml::from_str("[encryption]\nkey_file = \"/elsewhere.key\"").expect("parse");
        ensure_encryption_key(dir.path(), &mut extra).expect("key");
        assert!(!dir.path().join("encryption.key").exists());
        assert_eq!(
            extra["encryption"]["key_file"].as_str(),
            Some("/elsewhere.key")
        );
    }
}
