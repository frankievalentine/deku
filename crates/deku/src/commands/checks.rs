use anyhow::Result;
use clap::{Args, Subcommand};

use crate::client::DekuClient;

#[derive(Debug, Args)]
pub struct ChecksArgs {
    #[command(subcommand)]
    command: ChecksCommands,
}

#[derive(Debug, Subcommand)]
enum ChecksCommands {
    /// Show the latest deployment status for an app
    Run { app: String },
}

pub async fn run(args: ChecksArgs, client: &DekuClient) -> Result<()> {
    match args.command {
        ChecksCommands::Run { app } => {
            let data = client
                .get(&format!("/api/apps/{app}/deployments"))
                .await?;
            if let Some(deployments) = data.as_array() {
                if let Some(latest) = deployments.first() {
                    println!("Latest deployment for '{app}':");
                    println!("  ID:     {}", latest["id"].as_str().unwrap_or("-"));
                    println!("  Status: {}", latest["status"].as_str().unwrap_or("-"));
                    println!(
                        "  Built:  {}",
                        latest["created_at"].as_str().unwrap_or("-")
                    );
                } else {
                    println!("No deployments found for '{app}'.");
                }
            }
        }
    }
    Ok(())
}
