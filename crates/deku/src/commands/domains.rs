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
    /// Choose how a domain gets its certificate: auto (Angie requests and
    /// renews one), manual (the files you install), or off (HTTP only)
    Tls {
        #[arg(help = "App name")]
        app: String,
        #[arg(help = "Domain name")]
        domain: String,
        #[arg(value_parser = ["auto", "manual", "off"], help = "auto, manual, or off")]
        mode: String,
    },
    /// Check a domain again and request its certificate now, after fixing
    /// whatever stopped it
    Retry {
        #[arg(help = "App name")]
        app: String,
        #[arg(help = "Domain name")]
        domain: String,
    },
}

/// One line describing how a domain is served over TLS.
fn describe_tls(tls: &serde_json::Value) -> String {
    let mode = tls["mode"].as_str().unwrap_or("auto");
    let state = tls["state"].as_str().unwrap_or("pending");
    let mut line = match (mode, state) {
        ("manual", _) => "HTTPS with your certificate".to_string(),
        ("off", _) => "HTTP only".to_string(),
        (_, "active") => match tls["expires_at"].as_str() {
            Some(expiry) => format!(
                "HTTPS, renews automatically (expires {})",
                &expiry[..expiry.len().min(10)]
            ),
            None => "HTTPS, renews automatically".to_string(),
        },
        (_, "issuing") => "requesting a certificate".to_string(),
        (_, "checking") => "waiting until it reaches this server".to_string(),
        (_, "failed") => "certificate requests paused".to_string(),
        _ => "HTTP (automatic certificates are off)".to_string(),
    };
    if let Some(details) = tls["details"].as_str() {
        line.push_str(&format!(": {details}"));
    }
    line
}

pub async fn run(args: DomainsArgs, client: &DekuClient) -> Result<()> {
    match args.command {
        DomainsCommands::List { app } => {
            let data = client.get(&format!("/api/apps/{app}/domains")).await?;
            if let Some(domains) = data.as_array() {
                if domains.is_empty() {
                    println!("No domains set.");
                } else {
                    let width = domains
                        .iter()
                        .filter_map(|d| d["domain"].as_str())
                        .map(str::len)
                        .max()
                        .unwrap_or(0);
                    for d in domains {
                        let name = d["domain"].as_str().unwrap_or("-");
                        println!("{name:<width$}  {}", describe_tls(&d["tls"]));
                    }
                }
            }
        }

        DomainsCommands::Add { app, domain } => {
            client
                .post(
                    &format!("/api/apps/{app}/domains"),
                    serde_json::json!({ "domain": domain }),
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

        DomainsCommands::Tls { app, domain, mode } => {
            let tls = client
                .put(
                    &format!("/api/apps/{app}/domains/{domain}/tls"),
                    serde_json::json!({ "mode": mode }),
                )
                .await?;
            println!("{domain}: {}", describe_tls(&tls));
        }

        DomainsCommands::Retry { app, domain } => {
            client
                .post(
                    &format!("/api/apps/{app}/domains/{domain}/tls/retry"),
                    serde_json::json!({}),
                )
                .await?;
            println!("{domain} is checked again within a minute. Follow it with `deku domains list {app}`.");
        }
    }
    Ok(())
}
