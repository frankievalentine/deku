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
    /// Tail logs from a postgres service container
    Logs {
        name: String,
        #[arg(short = 'n', default_value_t = 100)]
        lines: usize,
    },
}

pub async fn run(args: PostgresArgs, client: &DekuClient) -> Result<()> {
    match args.command {
        PostgresCommands::Create { name } => {
            client
                .post("/api/postgres/services", serde_json::json!({ "name": name }))
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
            println!("{}", serde_json::to_string_pretty(&data)?);
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
