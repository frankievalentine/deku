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
    Status { app: String },
    /// Set global ACME email address
    Config {
        #[arg(long)]
        email: String,
    },
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
        LetsencryptCommands::Status { app } => {
            let status = client
                .get(&format!("/api/letsencrypt/status/{app}"))
                .await?;
            println!("{}", serde_json::to_string_pretty(&status)?);
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
