use anyhow::{anyhow, Result};
use clap::{Args, Subcommand};

use crate::client::DekuClient;

#[derive(Debug, Args)]
pub struct AcmeArgs {
    #[command(subcommand)]
    command: AcmeCommands,
}

#[derive(Debug, Subcommand)]
enum AcmeCommands {
    /// Show the certificate settings and whether one has been issued
    Status,
}

pub async fn run(args: AcmeArgs, client: &DekuClient) -> Result<()> {
    match args.command {
        AcmeCommands::Status => status(client).await,
    }
}

/// Report what automatic certificates were asked for and what happened.
///
/// The useful question is not only whether the settings are on, which a config
/// file answers, but whether the certificate is in place: until it is, the
/// environment and per-deployment hostnames are served over HTTP alone.
async fn status(client: &DekuClient) -> Result<()> {
    let data = client.get("/api/acme/status").await?;

    let enabled = data["enabled"].as_bool().unwrap_or(false);
    println!("Enabled:     {enabled}");
    if !enabled {
        println!("\nAutomatic certificates are off; no certificate is requested.");
        return Ok(());
    }

    println!(
        "Wildcard:    {}",
        data["wildcard"].as_bool().unwrap_or(false)
    );
    println!(
        "Provider:    {}",
        data["provider"].as_str().unwrap_or("unknown")
    );
    println!(
        "Directory:   {}",
        data["directory"].as_str().unwrap_or("unknown")
    );
    println!(
        "Account:     {}",
        data["account_email"].as_str().unwrap_or("(not set)")
    );
    println!(
        "Token:       {}",
        match data["token_configured"].as_bool().unwrap_or(false) {
            true => format!(
                "configured via {}",
                data["token_source"].as_str().unwrap_or("an unknown source")
            ),
            false => "not configured".to_string(),
        }
    );

    let request = &data["request_file"];
    println!(
        "Request:     {} ({})",
        request["path"].as_str().unwrap_or("unknown"),
        if request["written"].as_bool().unwrap_or(false) {
            "written"
        } else {
            "not written"
        }
    );

    let certificate = &data["certificate"];
    let path = certificate["path"].as_str().unwrap_or("unknown");
    if !certificate["exists"].as_bool().unwrap_or(false) {
        println!("Certificate: not issued yet ({path})");
        println!(
            "\nThe generated hostnames are served over HTTP until it is issued, and one deploy \
             afterwards puts it in front of them."
        );
        return Ok(());
    }

    let lifecycle = certificate["lifecycle"].as_str().unwrap_or("unknown");
    match certificate["days_remaining"].as_i64() {
        Some(days) => println!("Certificate: {lifecycle}, expires in {days} day(s) ({path})"),
        None => println!("Certificate: present but could not be read ({path})"),
    }
    if let Some(expires_at) = certificate["expires_at"].as_str() {
        println!("Expires:     {expires_at}");
    }

    if lifecycle == "expired" || lifecycle == "unknown" {
        return Err(anyhow!(
            "the wildcard certificate cannot be used ({lifecycle}); Angie re-requests it on reload"
        ));
    }
    Ok(())
}
