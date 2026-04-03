use anyhow::Result;
use clap::{Args, Subcommand};

use crate::client::DekuClient;

#[derive(Debug, Args)]
pub struct PostgresArgs {
    #[command(subcommand)]
    command: PostgresCommands,
}

#[derive(Debug, Subcommand)]
enum PostgresCommands {
    /// Create a new postgres service
    Create { name: String },
    /// Destroy a postgres service
    Destroy { name: String },
    /// Link a postgres service to an app
    Link { service: String, app: String },
    /// Unlink a postgres service from an app
    Unlink { service: String, app: String },
    /// List all postgres services
    List,
    /// Show info about a postgres service
    Info { name: String },
    /// Print connection details for a postgres service
    Connect { name: String },
    /// Tail logs from a postgres service container
    Logs {
        name: String,
        #[arg(short = 'n', default_value_t = 100)]
        lines: usize,
    },
    /// Create a backup in the configured object store
    Backup { name: String },
    /// List backups for a postgres service
    Backups { name: String },
    /// Restore a backup into a postgres service
    Restore { name: String, backup: String },
}

pub async fn run(args: PostgresArgs, client: &DekuClient) -> Result<()> {
    match args.command {
        PostgresCommands::Create { name } => {
            client
                .post(
                    "/api/postgres/services",
                    serde_json::json!({ "name": name }),
                )
                .await?;
            println!("Postgres service '{name}' created.");
        }
        PostgresCommands::Destroy { name } => {
            client
                .delete(&format!("/api/postgres/services/{name}"))
                .await?;
            println!("Postgres service '{name}' destroyed.");
        }
        PostgresCommands::Link { service, app } => {
            client
                .post(
                    &format!("/api/postgres/services/{service}/link/{app}"),
                    serde_json::json!({}),
                )
                .await?;
            println!("Linked '{service}' to '{app}'.");
        }
        PostgresCommands::Unlink { service, app } => {
            client
                .delete(&format!("/api/postgres/services/{service}/link/{app}"))
                .await?;
            println!("Unlinked '{service}' from '{app}'.");
        }
        PostgresCommands::List => {
            let data = client.get("/api/postgres/services").await?;
            print_service_table(&data);
        }
        PostgresCommands::Info { name } => {
            let data = client
                .get(&format!("/api/postgres/services/{name}"))
                .await?;
            print_service_info(&data);
        }
        PostgresCommands::Connect { name } => {
            let data = client
                .get(&format!("/api/postgres/services/{name}"))
                .await?;
            print_connect_details(&data);
        }
        PostgresCommands::Logs { name, lines } => {
            let data = client
                .get(&format!("/api/postgres/services/{name}/logs?n={lines}"))
                .await?;
            if let Some(logs) = data["logs"].as_array() {
                for line in logs {
                    println!("{}", line.as_str().unwrap_or(""));
                }
            }
        }
        PostgresCommands::Backup { name } => {
            let data = client
                .post(
                    &format!("/api/postgres/services/{name}/backups"),
                    serde_json::json!({}),
                )
                .await?;
            println!(
                "Created backup {} ({} bytes)",
                data["id"].as_str().unwrap_or("-"),
                data["size_bytes"].as_i64().unwrap_or_default()
            );
        }
        PostgresCommands::Backups { name } => {
            let data = client
                .get(&format!("/api/postgres/services/{name}/backups"))
                .await?;
            print_backup_table(&data);
        }
        PostgresCommands::Restore { name, backup } => {
            client
                .post(
                    &format!("/api/postgres/services/{name}/restore/{backup}"),
                    serde_json::json!({}),
                )
                .await?;
            println!("Restored backup '{backup}' into '{name}'.");
        }
    }
    Ok(())
}

fn print_service_table(data: &serde_json::Value) {
    if let Some(services) = data.as_array() {
        if services.is_empty() {
            println!("No services.");
            return;
        }
        println!("{:<20} {:<10} CREATED", "NAME", "STATUS");
        println!("{}", "-".repeat(64));
        for s in services {
            println!(
                "{:<20} {:<10} {}",
                s["name"].as_str().unwrap_or("-"),
                s["status"].as_str().unwrap_or("-"),
                s["created_at"].as_str().unwrap_or("-"),
            );
        }
    }
}

fn print_service_info(data: &serde_json::Value) {
    println!("Name: {}", data["name"].as_str().unwrap_or("-"));
    println!("Status: {}", data["status"].as_str().unwrap_or("-"));
    println!("Created: {}", data["created_at"].as_str().unwrap_or("-"));
    println!(
        "Container: {}",
        data["container_id"].as_str().unwrap_or("-")
    );

    let connection = &data["connection"];
    println!("Host: {}", connection["host"].as_str().unwrap_or("-"));
    println!("Port: {}", connection["port"].as_u64().unwrap_or_default());
    println!(
        "Database: {}",
        connection["database"].as_str().unwrap_or("-")
    );
    println!(
        "Username: {}",
        connection["username"].as_str().unwrap_or("-")
    );
    println!("Env key: {}", connection["env_key"].as_str().unwrap_or("-"));
    println!("Volume: {}", connection["volume"].as_str().unwrap_or("-"));
    println!("URL: {}", connection["url"].as_str().unwrap_or("-"));

    println!("Linked apps:");
    if let Some(links) = data["links"].as_array() {
        if links.is_empty() {
            println!("- none");
        } else {
            for link in links {
                println!(
                    "- {} ({})",
                    link["name"].as_str().unwrap_or("-"),
                    link["env_key"].as_str().unwrap_or("-"),
                );
            }
        }
    }
}

fn print_connect_details(data: &serde_json::Value) {
    let name = data["name"].as_str().unwrap_or("-");
    let connection = &data["connection"];
    println!("{}", connection["url"].as_str().unwrap_or("-"));
    println!(
        "docker exec -it deku-postgres-{name} psql -U {} -d {}",
        connection["username"].as_str().unwrap_or("deku"),
        connection["database"].as_str().unwrap_or(name),
    );
}

fn print_backup_table(data: &serde_json::Value) {
    if let Some(backups) = data.as_array() {
        if backups.is_empty() {
            println!("No backups.");
            return;
        }
        println!("{:<36} {:<12} {:<20} RESTORED", "ID", "SIZE", "CREATED");
        println!("{}", "-".repeat(96));
        for backup in backups {
            println!(
                "{:<36} {:<12} {:<20} {}",
                backup["id"].as_str().unwrap_or("-"),
                backup["size_bytes"].as_i64().unwrap_or_default(),
                backup["created_at"].as_str().unwrap_or("-"),
                backup["restored_at"].as_str().unwrap_or("-"),
            );
        }
    }
}
