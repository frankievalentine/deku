use anyhow::Result;
use clap::{Args, Subcommand};

use crate::client::DekuClient;

#[derive(Debug, Args)]
pub struct AppsArgs {
    #[command(subcommand)]
    command: AppsCommands,
}

#[derive(Debug, Subcommand)]
enum AppsCommands {
    /// List all apps
    List,
    /// Create a new app
    Create {
        #[arg(help = "App name")]
        name: String,
    },
    /// Destroy an app and all its containers
    Destroy {
        #[arg(help = "App name")]
        name: String,
        #[arg(long, help = "Skip confirmation")]
        force: bool,
    },
    /// Show app info
    Info {
        #[arg(help = "App name")]
        name: String,
    },
}

pub async fn run(args: AppsArgs, client: &DekuClient) -> Result<()> {
    match args.command {
        AppsCommands::List => {
            let data = client.get("/api/apps").await?;
            if let Some(apps) = data.as_array() {
                if apps.is_empty() {
                    println!("No apps found.");
                } else {
                    println!("{:<20} {:<12} {}", "NAME", "STATUS", "CREATED");
                    println!("{}", "-".repeat(60));
                    for app in apps {
                        let name = app["name"].as_str().unwrap_or("-");
                        let status = app["status"].as_str().unwrap_or("-");
                        let created = app["created_at"].as_str().unwrap_or("-");
                        println!("{:<20} {:<12} {}", name, status, &created[..19.min(created.len())]);
                    }
                }
            }
        }

        AppsCommands::Create { name } => {
            let data = client
                .post("/api/apps", serde_json::json!({ "name": name }))
                .await?;
            let id = data["id"].as_str().unwrap_or("?");
            println!("Created app '{name}' (id: {id})");
        }

        AppsCommands::Destroy { name, force } => {
            if !force {
                print!("Destroy app '{name}' and all its data? [y/N] ");
                use std::io::Write;
                std::io::stdout().flush()?;
                let mut input = String::new();
                std::io::stdin().read_line(&mut input)?;
                if !input.trim().eq_ignore_ascii_case("y") {
                    println!("Aborted.");
                    return Ok(());
                }
            }
            client.delete(&format!("/api/apps/{name}")).await?;
            println!("App '{name}' destroyed.");
        }

        AppsCommands::Info { name } => {
            let data = client.get(&format!("/api/apps/{name}")).await?;
            println!("{}", serde_json::to_string_pretty(&data)?);
        }
    }
    Ok(())
}
