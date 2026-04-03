use crate::client::DekuClient;
use anyhow::Result;
use clap::{Args, Subcommand};

#[derive(Debug, Args)]
pub struct ConfigArgs {
    #[command(subcommand)]
    command: ConfigCommands,
}

#[derive(Debug, Subcommand)]
enum ConfigCommands {
    /// List all config vars for an app
    List {
        #[arg(help = "App name")]
        app: String,
    },
    /// Set one or more config vars (KEY=VALUE ...)
    Set {
        #[arg(help = "App name")]
        app: String,
        #[arg(help = "KEY=VALUE pairs", num_args = 1..)]
        pairs: Vec<String>,
    },
    /// Unset a config var
    Unset {
        #[arg(help = "App name")]
        app: String,
        #[arg(help = "Key name")]
        key: String,
    },
}

pub async fn run(args: ConfigArgs, client: &DekuClient) -> Result<()> {
    match args.command {
        ConfigCommands::List { app } => {
            let data = client.get(&format!("/api/apps/{app}/config")).await?;
            if let Some(obj) = data.as_object() {
                if obj.is_empty() {
                    println!("No config vars set.");
                } else {
                    for (k, v) in obj {
                        println!("{}={}", k, v.as_str().unwrap_or(""));
                    }
                }
            }
        }

        ConfigCommands::Set { app, pairs } => {
            for pair in &pairs {
                let (k, v) = pair
                    .split_once('=')
                    .ok_or_else(|| anyhow::anyhow!("invalid KEY=VALUE: {pair}"))?;
                client
                    .post(
                        &format!("/api/apps/{app}/config"),
                        serde_json::json!({
                            "key": k,
                            "value": v,
                        }),
                    )
                    .await?;
            }
            println!("Config vars set for '{app}'.");
        }

        ConfigCommands::Unset { app, key } => {
            client
                .delete(&format!("/api/apps/{app}/config/{key}"))
                .await?;
            println!("Unset '{key}' for '{app}'.");
        }
    }
    Ok(())
}
