use anyhow::Result;
use clap::{Args, Subcommand};

use crate::client::DekuClient;

#[derive(Debug, Args)]
pub struct EnvArgs {
    #[command(subcommand)]
    command: EnvCommands,
}

#[derive(Debug, Subcommand)]
enum EnvCommands {
    /// List an app's environments
    List {
        #[arg(help = "App name")]
        app: String,
    },
    /// Create an environment
    Create {
        #[arg(help = "App name")]
        app: String,
        #[arg(help = "Display name, for example 'Staging'")]
        name: String,
        #[arg(
            long,
            help = "Slug used in hostnames; derived from the name by default"
        )]
        slug: Option<String>,
        #[arg(long, help = "Git ref this environment tracks (recorded only)")]
        branch: Option<String>,
    },
    /// Remove an environment
    Remove {
        #[arg(help = "App name")]
        app: String,
        #[arg(help = "Environment slug")]
        slug: String,
    },
}

pub async fn run(args: EnvArgs, client: &DekuClient) -> Result<()> {
    match args.command {
        EnvCommands::List { app } => {
            let data = client.get(&format!("/api/apps/{app}/environments")).await?;
            let empty = Vec::new();
            let environments = data.as_array().unwrap_or(&empty);
            if environments.is_empty() {
                println!("No environments recorded for '{app}'.");
                return Ok(());
            }

            println!("{:<16} {:<20} {:<16} STAGING", "SLUG", "NAME", "BRANCH");
            println!("{}", "-".repeat(70));
            for environment in environments {
                let production = environment["is_production"].as_bool().unwrap_or(false);
                println!(
                    "{:<16} {:<20} {:<16} {}",
                    environment["slug"].as_str().unwrap_or("-"),
                    environment["name"].as_str().unwrap_or("-"),
                    environment["branch"].as_str().unwrap_or("-"),
                    if production { "production" } else { "preview" },
                );
            }
        }
        EnvCommands::Create {
            app,
            name,
            slug,
            branch,
        } => {
            let mut body = serde_json::json!({ "name": name });
            if let Some(slug) = &slug {
                body["slug"] = serde_json::json!(slug);
            }
            if let Some(branch) = &branch {
                body["branch"] = serde_json::json!(branch);
            }

            let created = client
                .post(&format!("/api/apps/{app}/environments"), body)
                .await?;
            println!(
                "Created environment '{}' for '{app}'{}{}.",
                created["slug"].as_str().unwrap_or(&name),
                created["branch"]
                    .as_str()
                    .map(|branch| format!(" tracking '{branch}'"))
                    .unwrap_or_default(),
                if created["is_production"].as_bool().unwrap_or(false) {
                    " (production)"
                } else {
                    ""
                },
            );
        }
        EnvCommands::Remove { app, slug } => {
            client
                .delete(&format!("/api/apps/{app}/environments/{slug}"))
                .await?;
            println!("Removed environment '{slug}' from '{app}'.");
        }
    }
    Ok(())
}
