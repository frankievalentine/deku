use anyhow::Result;
use clap::{Args, Subcommand};

use crate::client::DekuClient;

#[derive(Debug, Args)]
pub struct RedirectsArgs {
    #[command(subcommand)]
    command: RedirectsCommands,
}

#[derive(Debug, Subcommand)]
enum RedirectsCommands {
    /// List an app's redirects
    List { app: String },
    /// Add a redirect
    Add {
        app: String,
        /// Source path, e.g. /old
        source: String,
        /// Target absolute URL or path, e.g. https://example.com/new
        target: String,
        #[arg(long, default_value_t = 302, help = "301, 302, 307, or 308")]
        code: i64,
    },
    /// Remove a redirect by id
    Remove { app: String, id: String },
}

pub async fn run(args: RedirectsArgs, client: &DekuClient) -> Result<()> {
    match args.command {
        RedirectsCommands::List { app } => {
            let data = client.get(&format!("/api/apps/{app}/redirects")).await?;
            let empty = Vec::new();
            let redirects = data.as_array().unwrap_or(&empty);
            if redirects.is_empty() {
                println!("No redirects for '{app}'.");
                return Ok(());
            }
            println!("{:<36} {:<20} {:<6} TARGET", "ID", "SOURCE", "CODE");
            println!("{}", "-".repeat(90));
            for redirect in redirects {
                println!(
                    "{:<36} {:<20} {:<6} {}",
                    redirect["id"].as_str().unwrap_or("-"),
                    redirect["source_path"].as_str().unwrap_or("-"),
                    redirect["code"].as_i64().unwrap_or(0),
                    redirect["target"].as_str().unwrap_or("-"),
                );
            }
        }
        RedirectsCommands::Add {
            app,
            source,
            target,
            code,
        } => {
            let data = client
                .post(
                    &format!("/api/apps/{app}/redirects"),
                    serde_json::json!({ "source_path": source, "target": target, "code": code }),
                )
                .await?;
            println!(
                "Added redirect {} -> {} ({}).",
                data["source_path"].as_str().unwrap_or(&source),
                data["target"].as_str().unwrap_or(&target),
                data["code"].as_i64().unwrap_or(code),
            );
        }
        RedirectsCommands::Remove { app, id } => {
            client
                .delete(&format!("/api/apps/{app}/redirects/{id}"))
                .await?;
            println!("Removed redirect '{id}'.");
        }
    }
    Ok(())
}
