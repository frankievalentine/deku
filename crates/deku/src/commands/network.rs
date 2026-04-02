use anyhow::Result;
use clap::{Args, Subcommand};

use crate::client::DekuClient;

#[derive(Debug, Args)]
pub struct NetworkArgs {
    #[command(subcommand)]
    command: NetworkCommands,
}

#[derive(Debug, Subcommand)]
enum NetworkCommands {
    /// Create a new Docker network
    Create { name: String },
    /// Destroy a Docker network
    Destroy { name: String },
    /// Attach an app to a network
    Attach { app: String, network: String },
    /// Detach an app from a network
    Detach { app: String, network: String },
    /// List all networks
    List,
    /// List networks for an app
    Report { app: String },
}

pub async fn run(args: NetworkArgs, client: &DekuClient) -> Result<()> {
    match args.command {
        NetworkCommands::Create { name } => {
            client
                .post("/api/networks", serde_json::json!({ "name": name }))
                .await?;
            println!("Network '{name}' created.");
        }
        NetworkCommands::Destroy { name } => {
            client.delete(&format!("/api/networks/{name}")).await?;
            println!("Network '{name}' destroyed.");
        }
        NetworkCommands::Attach { app, network } => {
            client
                .post(
                    &format!("/api/apps/{app}/networks/{network}"),
                    serde_json::json!({}),
                )
                .await?;
            println!("App '{app}' attached to network '{network}'.");
        }
        NetworkCommands::Detach { app, network } => {
            client
                .delete(&format!("/api/apps/{app}/networks/{network}"))
                .await?;
            println!("App '{app}' detached from network '{network}'.");
        }
        NetworkCommands::List => {
            let data = client.get("/api/networks").await?;
            if let Some(nets) = data.as_array() {
                if nets.is_empty() {
                    println!("No networks.");
                } else {
                    println!("{:<20}", "NAME");
                    println!("{}", "-".repeat(22));
                    for n in nets {
                        println!("{:<20}", n["name"].as_str().unwrap_or("-"));
                    }
                }
            }
        }
        NetworkCommands::Report { app } => {
            let data = client.get(&format!("/api/apps/{app}/networks")).await?;
            if let Some(nets) = data.as_array() {
                if nets.is_empty() {
                    println!("No networks attached to '{app}'.");
                } else {
                    println!("Networks for '{app}':");
                    for n in nets {
                        println!("  {}", n["name"].as_str().unwrap_or("-"));
                    }
                }
            }
        }
    }
    Ok(())
}
