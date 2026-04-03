use crate::client::DekuClient;
use anyhow::Result;
use clap::{Args, Subcommand};

#[derive(Debug, Args)]
pub struct PluginsArgs {
    #[command(subcommand)]
    command: PluginsCommands,
}

#[derive(Debug, Subcommand)]
enum PluginsCommands {
    /// List installed plugins
    List,
    /// Install a plugin
    Install {
        #[arg(help = "Plugin name or path to .so")]
        plugin: String,
    },
    /// Uninstall a plugin
    Uninstall {
        #[arg(help = "Plugin name")]
        name: String,
    },
}

pub async fn run(args: PluginsArgs, client: &DekuClient) -> Result<()> {
    match args.command {
        PluginsCommands::List => {
            let data = client.get("/api/plugins").await?;
            if let Some(plugins) = data.as_array() {
                if plugins.is_empty() {
                    println!("No plugins installed.");
                } else {
                    println!("{:<24} VERSION", "NAME");
                    println!("{}", "-".repeat(40));
                    for p in plugins {
                        let name = p["name"].as_str().unwrap_or("-");
                        let ver = p["version"].as_str().unwrap_or("-");
                        println!("{:<24} {}", name, ver);
                    }
                }
            }
        }

        PluginsCommands::Install { plugin } => {
            client
                .post("/api/plugins", serde_json::json!({ "path": plugin }))
                .await?;
            println!("Plugin installed from '{plugin}'.");
        }

        PluginsCommands::Uninstall { name } => {
            client.delete(&format!("/api/plugins/{name}")).await?;
            println!("Plugin '{name}' uninstalled.");
        }
    }
    Ok(())
}
