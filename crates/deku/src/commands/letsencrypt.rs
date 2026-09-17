use anyhow::Result;
use clap::{Args, Subcommand};

use crate::client::DekuClient;

#[derive(Debug, Args)]
pub struct LetsencryptArgs {
    #[command(subcommand)]
    command: LetsencryptCommands,
}

#[derive(Debug, Subcommand)]
enum LetsencryptCommands {
    /// Enable TLS for an app
    Enable { app: String },
    /// Disable TLS for an app
    Disable { app: String },
    /// Show TLS/certificate status for an app
    Status {
        app: String,
        /// Print the raw JSON payload instead of a summary
        #[arg(long)]
        json: bool,
    },
    /// Set global ACME email address
    Config {
        #[arg(long)]
        email: String,
    },
}

fn field(status: &serde_json::Value, key: &str) -> String {
    status[key]
        .as_str()
        .map(str::to_string)
        .unwrap_or_else(|| "unavailable".to_string())
}

fn nested_field(status: &serde_json::Value, outer: &str, inner: &str) -> String {
    status[outer][inner]
        .as_str()
        .map(str::to_string)
        .unwrap_or_else(|| "unavailable".to_string())
}

fn print_certificate_status(app: &str, status: &serde_json::Value) {
    let flag = |key: &str| {
        if status[key].as_bool().unwrap_or(false) {
            "yes"
        } else {
            "no"
        }
    };
    let domains = status["domains"]
        .as_array()
        .map(|entries| {
            entries
                .iter()
                .filter_map(|entry| entry.as_str())
                .collect::<Vec<_>>()
                .join(", ")
        })
        .filter(|joined| !joined.is_empty())
        .unwrap_or_else(|| "none".to_string());

    println!("TLS for '{app}'");
    println!("  enabled:      {}", flag("enabled"));
    println!("  files ready:  {}", flag("ready"));
    println!("  domains:      {domains}");
    println!("  lifecycle:    {}", field(status, "lifecycle"));
    println!("  not after:    {}", field(status, "not_after"));
    println!("  expires at:   {}", field(status, "expires_at"));
    if let Some(days) = status["days_remaining"].as_i64() {
        if days < 0 {
            println!("  days left:    expired {} day(s) ago", days.abs());
        } else {
            println!("  days left:    {days}");
        }
    }
    println!(
        "  certificate:  {}",
        nested_field(status, "certificate", "path")
    );
    println!(
        "  private key:  {}",
        nested_field(status, "private_key", "path")
    );

    // A missing or stale certificate is only a problem while TLS is on.
    let enabled = status["enabled"].as_bool().unwrap_or(false);
    let lifecycle = if enabled {
        status["lifecycle"].as_str()
    } else {
        None
    };

    match lifecycle {
        Some("expired") => println!(
            "warning: the certificate for '{app}' has expired; renew it now or TLS handshakes will fail."
        ),
        Some("expiring") => println!(
            "warning: the certificate for '{app}' expires soon; renew it before it lapses. Deku does not renew certificates itself."
        ),
        Some("missing") => println!(
            "warning: the certificate or key file for '{app}' is missing while TLS is enabled."
        ),
        Some("unknown") => println!(
            "warning: the certificate for '{app}' could not be inspected; check the files manually."
        ),
        _ => {}
    }

    if let Some(error) = status["inspection_error"].as_str() {
        println!("inspection error: {error}");
    }
}

pub async fn run(args: LetsencryptArgs, client: &DekuClient) -> Result<()> {
    match args.command {
        LetsencryptCommands::Enable { app } => {
            client
                .post(
                    &format!("/api/letsencrypt/enable/{app}"),
                    serde_json::json!({}),
                )
                .await?;
            println!("TLS enabled for '{app}'.");
        }
        LetsencryptCommands::Disable { app } => {
            client
                .post(
                    &format!("/api/letsencrypt/disable/{app}"),
                    serde_json::json!({}),
                )
                .await?;
            println!("TLS disabled for '{app}'.");
        }
        LetsencryptCommands::Status { app, json } => {
            let status = client
                .get(&format!("/api/letsencrypt/status/{app}"))
                .await?;
            if json {
                println!("{}", serde_json::to_string_pretty(&status)?);
            } else {
                print_certificate_status(&app, &status);
            }
        }
        LetsencryptCommands::Config { email } => {
            client
                .post(
                    "/api/letsencrypt/config",
                    serde_json::json!({ "email": email }),
                )
                .await?;
            println!("ACME email set to '{email}'.");
        }
    }
    Ok(())
}
