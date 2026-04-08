use anyhow::{anyhow, Result};
use chrono::Utc;
use clap::{Args, Subcommand};
use deku_core::auth::issue_dashboard_token;

use crate::{
    client::DekuClient,
    local_config::{load_required, save, LocalDekuConfig},
};

#[derive(Debug, Clone, Args)]
pub struct DashboardArgs {
    #[arg(long, help = "Print non-secret dashboard metadata as JSON")]
    json: bool,
    #[command(subcommand)]
    command: Option<DashboardCommands>,
}

#[derive(Debug, Clone, Subcommand)]
enum DashboardCommands {
    /// Rotate the dashboard access token
    ResetToken(ResetTokenArgs),
}

#[derive(Debug, Clone, Args)]
struct ResetTokenArgs {
    #[arg(long, help = "Skip the confirmation prompt")]
    yes: bool,
}

pub async fn run(args: DashboardArgs, client: &DekuClient) -> Result<()> {
    match args.command {
        Some(DashboardCommands::ResetToken(reset)) => reset_token(reset, client).await,
        None => show_dashboard_info(args.json),
    }
}

fn show_dashboard_info(as_json: bool) -> Result<()> {
    let config = load_required()?;
    let url = config.effective_dashboard_url();
    let local_url = config.local_dashboard_url();
    let config_path = config.config_path();
    let token_configured = config.token_configured();
    let reset_command = "deku dashboard reset-token";

    if as_json {
        println!(
            "{}",
            serde_json::to_string_pretty(&serde_json::json!({
                "url": url,
                "local_url": local_url,
                "host": config.effective_dashboard_host(),
                "config_path": config_path,
                "token_configured": token_configured,
                "reset_command": reset_command,
            }))?
        );
        return Ok(());
    }

    println!("Dashboard URL: {}", url);
    println!("Local URL:     {}", local_url);
    println!("Config path:   {}", config_path.display());
    println!(
        "Token status:  {}",
        if token_configured {
            "configured (stored hashed at rest)"
        } else {
            "not configured"
        }
    );
    println!("Reset token:   {reset_command}");
    println!();
    print_access_guidance(&config);
    println!();
    println!("Dashboard tokens are only shown when first created or reset.");

    Ok(())
}

async fn reset_token(args: ResetTokenArgs, client: &DekuClient) -> Result<()> {
    if !args.yes {
        let confirmed = cliclack::confirm(
            "Reset the dashboard token? Existing browser sessions will stop working.",
        )
        .initial_value(false)
        .interact()?;
        if !confirmed {
            println!("Aborted.");
            return Ok(());
        }
    }

    let config = load_required()?;
    if let Ok(token) = reset_via_daemon(client).await {
        print_token_notice(&config, &token, true);
        return Ok(());
    }

    let now = Utc::now();
    let (token, dashboard_auth) = issue_dashboard_token(now, config.dashboard_auth.as_ref())?;
    let mut updated = config;
    updated.dashboard_auth = Some(dashboard_auth);
    updated.clear_legacy_token_file()?;
    save(&updated)?;
    print_token_notice(&updated, &token, false);
    Ok(())
}

async fn reset_via_daemon(client: &DekuClient) -> Result<String> {
    let payload = client
        .post("/api/dashboard/token", serde_json::json!({}))
        .await?;
    payload["token"]
        .as_str()
        .map(ToOwned::to_owned)
        .ok_or_else(|| anyhow!("daemon did not return a dashboard token"))
}

pub fn print_token_notice(config: &LocalDekuConfig, token: &str, active_now: bool) {
    println!("Dashboard URL: {}", config.effective_dashboard_url());
    println!("Local URL:     {}", config.local_dashboard_url());
    println!();
    println!("Save this dashboard token now. It will only be shown once.");
    println!("The token is stored hashed at rest and cannot be recovered later.");
    println!();
    println!("Dashboard token: {token}");
    println!("Reset command:   deku dashboard reset-token");
    if active_now {
        println!("Status:          active immediately");
    } else {
        println!("Status:          saved to config; restart `dekud` if it is not already running");
    }
    println!();
    print_access_guidance(config);
}

fn print_access_guidance(config: &LocalDekuConfig) {
    println!(
        "Remote access:  make TCP port {} reachable from your browser, or use an SSH tunnel",
        config.effective_api_port()
    );
    println!(
        "SSH tunnel:     ssh -L {port}:127.0.0.1:{port} root@{host}",
        port = config.effective_api_port(),
        host = config.effective_dashboard_host()
    );
    println!(
        "UFW allow:      sudo ufw allow {}/tcp",
        config.effective_api_port()
    );
    println!(
        "UFW restrict:   sudo ufw allow from YOUR_PUBLIC_IP to any port {} proto tcp",
        config.effective_api_port()
    );
}
