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
    /// Destroy an app
    Destroy {
        #[arg(help = "App name")]
        name: String,
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
            println!("{}", serde_json::to_string_pretty(&data)?);
        }
        AppsCommands::Create { name } => {
            let data = client.post("/api/apps", serde_json::json!({ "name": name })).await?;
            println!("{}", serde_json::to_string_pretty(&data)?);
        }
        AppsCommands::Destroy { name } => {
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
