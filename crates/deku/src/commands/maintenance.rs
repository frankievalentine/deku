use anyhow::Result;
use clap::{Args, Subcommand};

use crate::client::DekuClient;

#[derive(Debug, Args)]
pub struct MaintenanceArgs {
    #[command(subcommand)]
    command: MaintenanceCommands,
}

#[derive(Debug, Subcommand)]
enum MaintenanceCommands {
    /// Serve a 503 for an app instead of proxying to it
    On {
        app: String,
        #[arg(long, help = "Message returned with the 503")]
        message: Option<String>,
    },
    /// Resume normal serving
    Off { app: String },
    /// Show maintenance status
    Status { app: String },
}

pub async fn run(args: MaintenanceArgs, client: &DekuClient) -> Result<()> {
    match args.command {
        MaintenanceCommands::On { app, message } => {
            client
                .post(
                    &format!("/api/apps/{app}/maintenance"),
                    serde_json::json!({ "enabled": true, "message": message }),
                )
                .await?;
            println!("Maintenance mode enabled for '{app}'.");
        }
        MaintenanceCommands::Off { app } => {
            client
                .post(
                    &format!("/api/apps/{app}/maintenance"),
                    serde_json::json!({ "enabled": false }),
                )
                .await?;
            println!("Maintenance mode disabled for '{app}'.");
        }
        MaintenanceCommands::Status { app } => {
            let data = client.get(&format!("/api/apps/{app}/maintenance")).await?;
            if data["enabled"].as_bool().unwrap_or(false) {
                println!("Maintenance mode: enabled");
                if let Some(message) = data["message"].as_str() {
                    println!("Message: {message}");
                }
            } else {
                println!("Maintenance mode: disabled");
            }
        }
    }
    Ok(())
}
