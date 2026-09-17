use anyhow::Result;
use clap::{Args, Subcommand};

use crate::client::DekuClient;

/// Shared command surface for managed services reached through the generic
/// `/api/services/{type}` routes. `postgres`, `redis`, and `mysql` keep their own
/// command groups for compatibility; newer providers reuse this one.
#[derive(Debug, Args)]
pub struct ServiceArgs {
    #[command(subcommand)]
    command: ServiceCommands,
}

#[derive(Debug, Subcommand)]
enum ServiceCommands {
    /// Create a new service
    Create { name: String },
    /// Destroy a service
    Destroy { name: String },
    /// Link a service to an app
    Link { service: String, app: String },
    /// Unlink a service from an app
    Unlink { service: String, app: String },
    /// List services of this type
    List,
    /// Show info about a service
    Info { name: String },
    /// Print connection details for a service
    Connect { name: String },
    /// Tail logs from a service container
    Logs {
        name: String,
        #[arg(short = 'n', default_value_t = 100)]
        lines: usize,
    },
    /// Create a backup in the configured object store
    Backup { name: String },
    /// List backups for a service
    Backups { name: String },
    /// Restore a backup into a service
    Restore { name: String, backup: String },
}

pub async fn run(service_type: &str, args: ServiceArgs, client: &DekuClient) -> Result<()> {
    let base = format!("/api/services/{service_type}");
    let label = display_name(service_type);

    match args.command {
        ServiceCommands::Create { name } => {
            client
                .post(&base, serde_json::json!({ "name": name }))
                .await?;
            println!("{label} service '{name}' created.");
        }
        ServiceCommands::Destroy { name } => {
            client.delete(&format!("{base}/{name}")).await?;
            println!("{label} service '{name}' destroyed.");
        }
        ServiceCommands::Link { service, app } => {
            client
                .post(
                    &format!("{base}/{service}/link/{app}"),
                    serde_json::json!({}),
                )
                .await?;
            println!("Linked '{service}' to '{app}'.");
        }
        ServiceCommands::Unlink { service, app } => {
            client
                .delete(&format!("{base}/{service}/link/{app}"))
                .await?;
            println!("Unlinked '{service}' from '{app}'.");
        }
        ServiceCommands::List => {
            let data = client.get(&base).await?;
            print_service_table(&data);
        }
        ServiceCommands::Info { name } | ServiceCommands::Connect { name } => {
            let data = client.get(&format!("{base}/{name}")).await?;
            print_service_info(&data);
        }
        ServiceCommands::Logs { name, lines } => {
            let data = client.get(&format!("{base}/{name}/logs?n={lines}")).await?;
            if let Some(log_lines) = data["logs"].as_array() {
                for line in log_lines {
                    println!("{}", line.as_str().unwrap_or(""));
                }
            }
        }
        ServiceCommands::Backup { name } => {
            let data = client
                .post(&format!("{base}/{name}/backups"), serde_json::json!({}))
                .await?;
            println!(
                "Backup created for '{name}' ({} bytes).",
                data["size_bytes"].as_i64().unwrap_or(0)
            );
        }
        ServiceCommands::Backups { name } => {
            let data = client.get(&format!("{base}/{name}/backups")).await?;
            super::render::print_backup_table(&data);
        }
        ServiceCommands::Restore { name, backup } => {
            client
                .post(
                    &format!("{base}/{name}/restore/{backup}"),
                    serde_json::json!({}),
                )
                .await?;
            println!("Restored backup '{backup}' into '{name}'.");
        }
    }

    Ok(())
}

fn display_name(service_type: &str) -> &str {
    match service_type {
        "mariadb" => "MariaDB",
        "mongodb" => "MongoDB",
        other => other,
    }
}

fn print_service_table(data: &serde_json::Value) {
    let Some(services) = data.as_array() else {
        println!("Unexpected response.");
        return;
    };
    if services.is_empty() {
        println!("No services.");
        return;
    }
    println!("{:<20} {:<10}", "NAME", "STATUS");
    println!("{}", "-".repeat(32));
    for service in services {
        println!(
            "{:<20} {:<10}",
            service["name"].as_str().unwrap_or("-"),
            service["status"].as_str().unwrap_or("-"),
        );
    }
}

fn print_service_info(data: &serde_json::Value) {
    println!("Name: {}", data["name"].as_str().unwrap_or("-"));
    println!("Type: {}", data["plugin"].as_str().unwrap_or("-"));
    println!("Status: {}", data["status"].as_str().unwrap_or("-"));

    let Some(connection) = data.get("connection") else {
        return;
    };
    println!("Env key: {}", connection["env_key"].as_str().unwrap_or("-"));
    println!("URL: {}", connection["url"].as_str().unwrap_or("-"));
    println!("Host: {}", connection["host"].as_str().unwrap_or("-"));
    println!("Port: {}", connection["port"].as_i64().unwrap_or(0));
    println!("Volume: {}", connection["volume"].as_str().unwrap_or("-"));
}
