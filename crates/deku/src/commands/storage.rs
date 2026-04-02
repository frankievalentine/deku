use anyhow::Result;
use clap::{Args, Subcommand};

use crate::client::DekuClient;

#[derive(Debug, Args)]
pub struct StorageArgs {
    #[command(subcommand)]
    command: StorageCommands,
}

#[derive(Debug, Subcommand)]
enum StorageCommands {
    /// Ensure a host directory exists
    EnsureDirectory { app: String, path: String },
    /// Mount a host path into the app container
    Mount {
        app: String,
        host_path: String,
        container_path: String,
    },
    /// Remove a storage mount
    Unmount { app: String, id: String },
    /// List storage mounts for an app
    List { app: String },
}

pub async fn run(args: StorageArgs, client: &DekuClient) -> Result<()> {
    match args.command {
        StorageCommands::EnsureDirectory { app, path } => {
            client
                .post(
                    &format!("/api/apps/{app}/storage/ensure"),
                    serde_json::json!({ "path": path }),
                )
                .await?;
            println!("Directory '{path}' ensured.");
        }
        StorageCommands::Mount {
            app,
            host_path,
            container_path,
        } => {
            client
                .post(
                    &format!("/api/apps/{app}/storage"),
                    serde_json::json!({ "host_path": host_path, "container_path": container_path }),
                )
                .await?;
            println!("Mounted '{host_path}' -> '{container_path}' for '{app}'.");
        }
        StorageCommands::Unmount { app, id } => {
            client
                .delete(&format!("/api/apps/{app}/storage/{id}"))
                .await?;
            println!("Mount '{id}' removed from '{app}'.");
        }
        StorageCommands::List { app } => {
            let data = client.get(&format!("/api/apps/{app}/storage")).await?;
            if let Some(mounts) = data.as_array() {
                if mounts.is_empty() {
                    println!("No storage mounts for '{app}'.");
                } else {
                    println!("{:<36} {:<30} {:<30}", "ID", "HOST PATH", "CONTAINER PATH");
                    println!("{}", "-".repeat(98));
                    for m in mounts {
                        println!(
                            "{:<36} {:<30} {:<30}",
                            m["id"].as_str().unwrap_or("-"),
                            m["host_path"].as_str().unwrap_or("-"),
                            m["container_path"].as_str().unwrap_or("-"),
                        );
                    }
                }
            }
        }
    }
    Ok(())
}
