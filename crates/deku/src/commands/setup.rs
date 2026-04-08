use anyhow::Result;
use chrono::Utc;
use clap::Args;
use deku_core::{auth::issue_dashboard_token, types::ObjectStoreConfig};
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
    #[arg(long)]
    pub ssh_port: Option<u16>,
    #[arg(long)]
    pub angie_conf_dir: Option<String>,
    #[arg(long)]
    pub global_domain: Option<String>,
}

pub fn run(args: SetupArgs) -> Result<()> {
    cliclack::intro("Deku setup")?;
    let skip_systemd = args.no_systemd;
    let use_defaults = args.defaults;

    let config_dir = config_dir();
    let default_data = config_dir.to_string_lossy().to_string();

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

    let existing_dashboard_auth = load_optional()?.and_then(|config| config.dashboard_auth);
    let (dashboard_token, dashboard_auth) =
        issue_dashboard_token(Utc::now(), existing_dashboard_auth.as_ref())?;

    let cfg = LocalDekuConfig {
        data_dir: Some(PathBuf::from(&data_dir)),
        api_port: Some(api_port),
        ssh_port: Some(ssh_port),
        angie_conf_dir: Some(PathBuf::from(&angie_conf_dir)),
        global_domain,
        object_store,
        dashboard_auth: Some(dashboard_auth),
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

    println!();
    print_token_notice(&cfg, &dashboard_token, false);
    println!("Config file:      {}", normalize_path_string(&config_path));
    println!(
        "Dashboard assets: {}",
        normalize_path_string(&cfg.dashboard_dir_path())
    );
    cliclack::outro("Setup complete. Start `dekud` to serve the dashboard.")?;
    Ok(())
}

#[cfg(target_os = "linux")]
fn install_systemd_unit() -> Result<()> {
    let bin_dir = std::env::var("DEKU_BIN_DIR").unwrap_or_else(|_| "/usr/local/bin".to_string());
    let unit = format!(
        r#"[Unit]
Description=Deku PaaS daemon
After=network.target docker.service
Requires=docker.service

[Service]
Type=simple
ExecStart={bin_dir}/dekud
Environment=HOME=/root
Environment=RUST_LOG=info
Restart=on-failure
RestartSec=5
User=root

[Install]
WantedBy=multi-user.target
"#
    );

    let unit_path = std::path::Path::new("/etc/systemd/system/deku.service");
    std::fs::write(unit_path, unit)?;
    cliclack::log::success("systemd unit written to /etc/systemd/system/deku.service")?;
    cliclack::log::info("Run: systemctl enable --now deku")?;
    Ok(())
}
