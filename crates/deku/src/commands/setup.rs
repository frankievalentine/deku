use anyhow::Result;
use serde::Serialize;
use std::path::PathBuf;

#[derive(Serialize)]
struct SetupConfig {
    data_dir: String,
    api_port: u16,
    ssh_port: u16,
    angie_conf_dir: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    global_domain: Option<String>,
}

pub fn run() -> Result<()> {
    cliclack::intro("Deku setup")?;

    let home = dirs_next::home_dir().unwrap_or_else(|| PathBuf::from("/root"));
    let default_data = home.join(".deku").to_string_lossy().to_string();

    let data_dir: String = cliclack::input("Data directory")
        .default_input(&default_data)
        .interact()?;

    let api_port_str: String = cliclack::input("API port")
        .default_input("2810")
        .interact()?;
    let api_port: u16 = api_port_str.parse().unwrap_or(2810);

    let ssh_port_str: String = cliclack::input("SSH port")
        .default_input("22")
        .interact()?;
    let ssh_port: u16 = ssh_port_str.parse().unwrap_or(22);

    let angie_conf_dir: String = cliclack::input("Angie config directory")
        .default_input("/etc/angie/conf.d/deku")
        .interact()?;

    let global_domain_str: String = cliclack::input("Global domain (leave blank to skip)")
        .default_input("")
        .interact()?;
    let global_domain = if global_domain_str.is_empty() {
        None
    } else {
        Some(global_domain_str)
    };

    let cfg = SetupConfig {
        data_dir: data_dir.clone(),
        api_port,
        ssh_port,
        angie_conf_dir,
        global_domain,
    };

    let data_path = PathBuf::from(&data_dir);
    std::fs::create_dir_all(&data_path)?;
    let config_path = data_path.join("config.toml");
    let contents = toml::to_string_pretty(&cfg)?;
    std::fs::write(&config_path, contents)?;

    cliclack::log::success(format!("Config written to {}", config_path.display()))?;

    #[cfg(target_os = "linux")]
    {
        let install_systemd: bool = cliclack::confirm("Install systemd service?")
            .initial_value(true)
            .interact()?;

        if install_systemd {
            install_systemd_unit()?;
        }
    }

    cliclack::outro("Setup complete. Run `dekud` to start the daemon.")?;
    Ok(())
}

#[cfg(target_os = "linux")]
fn install_systemd_unit() -> Result<()> {
    let unit = r#"[Unit]
Description=Deku PaaS daemon
After=network.target docker.service
Requires=docker.service

[Service]
Type=simple
ExecStart=/usr/local/bin/dekud
Restart=on-failure
RestartSec=5

[Install]
WantedBy=multi-user.target
"#;

    let unit_path = std::path::Path::new("/etc/systemd/system/deku.service");
    std::fs::write(unit_path, unit)?;
    cliclack::log::success("systemd unit written to /etc/systemd/system/deku.service")?;
    cliclack::log::info("Run: systemctl enable --now deku")?;
    Ok(())
}
