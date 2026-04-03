use anyhow::Result;
use clap::{Args, Subcommand};

use crate::client::DekuClient;

#[derive(Debug, Args)]
pub struct CronArgs {
    #[command(subcommand)]
    command: CronCommands,
}

#[derive(Debug, Subcommand)]
enum CronCommands {
    /// List cron entries for an app
    List { app: String },
    /// Add a cron entry for an app
    Add {
        app: String,
        schedule: String,
        command: String,
    },
    /// Remove a cron entry
    Remove { app: String, id: String },
}

pub async fn run(args: CronArgs, client: &DekuClient) -> Result<()> {
    match args.command {
        CronCommands::List { app } => {
            let data = client.get(&format!("/api/apps/{app}/cron")).await?;
            if let Some(entries) = data.as_array() {
                if entries.is_empty() {
                    println!("No cron entries for '{app}'.");
                } else {
                    println!("{:<36} {:<20} {}", "ID", "SCHEDULE", "COMMAND");
                    println!("{}", "-".repeat(80));
                    for e in entries {
                        println!(
                            "{:<36} {:<20} {}",
                            e["id"].as_str().unwrap_or("-"),
                            e["schedule"].as_str().unwrap_or("-"),
                            e["command"].as_str().unwrap_or("-"),
                        );
                    }
                }
            }
        }
        CronCommands::Add {
            app,
            schedule,
            command,
        } => {
            client
                .post(
                    &format!("/api/apps/{app}/cron"),
                    serde_json::json!({ "schedule": schedule, "command": command }),
                )
                .await?;
            println!("Cron entry added to '{app}'.");
        }
        CronCommands::Remove { app, id } => {
            client.delete(&format!("/api/apps/{app}/cron/{id}")).await?;
            println!("Cron entry '{id}' removed from '{app}'.");
        }
    }
    Ok(())
}
