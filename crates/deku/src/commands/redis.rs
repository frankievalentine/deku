use anyhow::Result;
use clap::{Args, Subcommand};

use crate::client::DekuClient;

#[derive(Debug, Args)]
pub struct RedisArgs {
    #[command(subcommand)]
    command: RedisCommands,
}

#[derive(Debug, Subcommand)]
enum RedisCommands {
    /// Create a new redis service
    Create { name: String },
    /// Destroy a redis service
    Destroy { name: String },
    /// Link a redis service to an app
    Link { service: String, app: String },
    /// Unlink a redis service from an app
    Unlink { service: String, app: String },
    /// List all redis services
    List,
    /// Show info about a redis service
    Info { name: String },
    /// Tail logs from a redis service container
    Logs {
        name: String,
        #[arg(short = 'n', default_value_t = 100)]
        lines: usize,
    },
}

pub async fn run(args: RedisArgs, client: &DekuClient) -> Result<()> {
    match args.command {
        RedisCommands::Create { name } => {
            client
                .post("/api/redis/services", serde_json::json!({ "name": name }))
                .await?;
            println!("Redis service '{name}' created.");
        }
        RedisCommands::Destroy { name } => {
            client
                .delete(&format!("/api/redis/services/{name}"))
                .await?;
            println!("Redis service '{name}' destroyed.");
        }
        RedisCommands::Link { service, app } => {
            client
                .post(
                    &format!("/api/redis/services/{service}/link/{app}"),
                    serde_json::json!({}),
                )
                .await?;
            println!("Linked '{service}' to '{app}'.");
        }
        RedisCommands::Unlink { service, app } => {
            client
                .delete(&format!("/api/redis/services/{service}/link/{app}"))
                .await?;
            println!("Unlinked '{service}' from '{app}'.");
        }
        RedisCommands::List => {
            let data = client.get("/api/redis/services").await?;
            print_service_table(&data);
        }
        RedisCommands::Info { name } => {
            let data = client.get(&format!("/api/redis/services/{name}")).await?;
            println!("{}", serde_json::to_string_pretty(&data)?);
        }
        RedisCommands::Logs { name, lines } => {
            let data = client
                .get(&format!("/api/redis/services/{name}/logs?n={lines}"))
                .await?;
            if let Some(logs) = data["logs"].as_array() {
                for line in logs {
                    println!("{}", line.as_str().unwrap_or(""));
                }
            }
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
        println!("{:<20} {:<10}", "NAME", "STATUS");
        println!("{}", "-".repeat(32));
        for s in services {
            println!(
                "{:<20} {:<10}",
                s["name"].as_str().unwrap_or("-"),
                s["status"].as_str().unwrap_or("-")
            );
        }
    }
}
