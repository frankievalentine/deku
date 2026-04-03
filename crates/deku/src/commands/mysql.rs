use anyhow::Result;
use clap::{Args, Subcommand};

use crate::client::DekuClient;

#[derive(Debug, Args)]
pub struct MysqlArgs {
    #[command(subcommand)]
    command: MysqlCommands,
}

#[derive(Debug, Subcommand)]
enum MysqlCommands {
    /// Create a new mysql service
    Create { name: String },
    /// Destroy a mysql service
    Destroy { name: String },
    /// Link a mysql service to an app
    Link { service: String, app: String },
    /// Unlink a mysql service from an app
    Unlink { service: String, app: String },
    /// List all mysql services
    List,
    /// Show info about a mysql service
    Info { name: String },
    /// Print connection details for a mysql service
    Connect { name: String },
    /// Tail logs from a mysql service container
    Logs {
        name: String,
        #[arg(short = 'n', default_value_t = 100)]
        lines: usize,
    },
}

pub async fn run(args: MysqlArgs, client: &DekuClient) -> Result<()> {
    match args.command {
        MysqlCommands::Create { name } => {
            client
                .post("/api/mysql/services", serde_json::json!({ "name": name }))
                .await?;
            println!("MySQL service '{name}' created.");
        }
        MysqlCommands::Destroy { name } => {
            client
                .delete(&format!("/api/mysql/services/{name}"))
                .await?;
            println!("MySQL service '{name}' destroyed.");
        }
        MysqlCommands::Link { service, app } => {
            client
                .post(
                    &format!("/api/mysql/services/{service}/link/{app}"),
                    serde_json::json!({}),
                )
                .await?;
            println!("Linked '{service}' to '{app}'.");
        }
        MysqlCommands::Unlink { service, app } => {
            client
                .delete(&format!("/api/mysql/services/{service}/link/{app}"))
                .await?;
            println!("Unlinked '{service}' from '{app}'.");
        }
        MysqlCommands::List => {
            let data = client.get("/api/mysql/services").await?;
            print_service_table(&data);
        }
        MysqlCommands::Info { name } => {
            let data = client.get(&format!("/api/mysql/services/{name}")).await?;
            print_service_info(&data);
        }
        MysqlCommands::Connect { name } => {
            let data = client.get(&format!("/api/mysql/services/{name}")).await?;
            print_connect_details(&data);
        }
        MysqlCommands::Logs { name, lines } => {
            let data = client
                .get(&format!("/api/mysql/services/{name}/logs?n={lines}"))
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
        "docker exec -it deku-mysql-{name} mysql -u{} -p{} {}",
        connection["username"].as_str().unwrap_or("deku"),
        connection["password"].as_str().unwrap_or("-"),
        connection["database"].as_str().unwrap_or(name),
    );
}
