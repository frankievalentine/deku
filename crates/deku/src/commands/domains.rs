use crate::client::DekuClient;
use anyhow::Result;
use clap::{Args, Subcommand};

#[derive(Debug, Args)]
pub struct DomainsArgs {
    #[command(subcommand)]
    command: DomainsCommands,
}

#[derive(Debug, Subcommand)]
enum DomainsCommands {
    /// List domains for an app
    List {
        #[arg(help = "App name")]
        app: String,
    },
    /// Add a domain to an app
    Add {
        #[arg(help = "App name")]
        app: String,
        #[arg(help = "Domain name")]
        domain: String,
    },
    /// Remove a domain from an app
    Remove {
        #[arg(help = "App name")]
        app: String,
        #[arg(help = "Domain name")]
        domain: String,
    },
}

pub async fn run(args: DomainsArgs, client: &DekuClient) -> Result<()> {
    match args.command {
        DomainsCommands::List { app } => {
            let data = client.get(&format!("/api/apps/{app}/domains")).await?;
            if let Some(domains) = data.as_array() {
                if domains.is_empty() {
                    println!("No domains set.");
                } else {
                    for d in domains {
                        println!("{}", d["name"].as_str().unwrap_or("-"));
                    }
                }
            }
        }

        DomainsCommands::Add { app, domain } => {
            client
                .post(
                    &format!("/api/apps/{app}/domains"),
                    serde_json::json!({ "name": domain }),
                )
                .await?;
            println!("Domain '{domain}' added to '{app}'.");
        }

        DomainsCommands::Remove { app, domain } => {
            client
                .delete(&format!("/api/apps/{app}/domains/{domain}"))
                .await?;
            println!("Domain '{domain}' removed from '{app}'.");
        }
    }
    Ok(())
}
