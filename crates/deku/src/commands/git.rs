use anyhow::Result;
use clap::{Args, Subcommand};

use crate::client::DekuClient;

#[derive(Debug, Args)]
pub struct GitArgs {
    #[command(subcommand)]
    command: GitCommands,
}

#[derive(Debug, Subcommand)]
enum GitCommands {
    /// Set a git-related config variable for an app
    Set {
        app: String,
        key: String,
        value: String,
    },
    /// Show git-related info for an app
    Report { app: String },
}

pub async fn run(args: GitArgs, client: &DekuClient) -> Result<()> {
    match args.command {
        GitCommands::Set { app, key, value } => {
            let env_key = format!("_GIT_{}", key.to_uppercase());
            client
                .post(
                    &format!("/api/apps/{app}/config"),
                    serde_json::json!({ "key": env_key, "value": value }),
                )
                .await?;
            println!("Set {env_key}={value} for '{app}'.");
        }
        GitCommands::Report { app } => {
            let app_data = client.get(&format!("/api/apps/{app}")).await?;
            let config_data = client.get(&format!("/api/apps/{app}/config")).await?;

            println!("App: {}", app_data["name"].as_str().unwrap_or(&app));
            println!("Git config vars:");

            let git_vars: Vec<_> = config_data
                .as_array()
                .map(|vars| {
                    vars.iter()
                        .filter(|v| {
                            v["key"]
                                .as_str()
                                .map(|k| k.starts_with("_GIT_"))
                                .unwrap_or(false)
                        })
                        .collect()
                })
                .unwrap_or_default();

            if git_vars.is_empty() {
                println!("  (none)");
            } else {
                for v in git_vars {
                    println!(
                        "  {}={}",
                        v["key"].as_str().unwrap_or(""),
                        v["value"].as_str().unwrap_or("")
                    );
                }
            }
        }
    }
    Ok(())
}
