use anyhow::Result;
use clap::{Args, Subcommand};

use crate::client::DekuClient;

#[derive(Debug, Args)]
pub struct ChecksArgs {
    #[command(subcommand)]
    command: ChecksCommands,
}

#[derive(Debug, Subcommand)]
enum ChecksCommands {
    /// Show the latest deployment status for an app
    Run { app: String },
    /// Show consolidated routing and TLS status
    Routing { app: Option<String> },
}

pub async fn run(args: ChecksArgs, client: &DekuClient) -> Result<()> {
    match args.command {
        ChecksCommands::Run { app } => {
            let data = client.get(&format!("/api/apps/{app}/deployments")).await?;
            if let Some(deployments) = data.as_array() {
                if let Some(latest) = deployments.first() {
                    println!("Latest deployment for '{app}':");
                    println!("  ID:     {}", latest["id"].as_str().unwrap_or("-"));
                    println!("  Status: {}", latest["status"].as_str().unwrap_or("-"));
                    println!("  Built:  {}", latest["created_at"].as_str().unwrap_or("-"));
                } else {
                    println!("No deployments found for '{app}'.");
                }
            }
        }
        ChecksCommands::Routing { app } => {
            if let Some(app) = app {
                let data = client.get(&format!("/api/routing/status/{app}")).await?;
                let angie_valid = data["angie"]["config_valid"].as_bool().unwrap_or(false);
                println!(
                    "App:          {}",
                    data["app"]["app"].as_str().unwrap_or("-")
                );
                println!(
                    "Status:       {}",
                    data["app"]["status"].as_str().unwrap_or("-")
                );
                println!(
                    "TLS enabled:  {}",
                    yes_no(data["app"]["tls_enabled"].as_bool().unwrap_or(false))
                );
                println!(
                    "TLS ready:    {}",
                    yes_no(data["app"]["tls_ready"].as_bool().unwrap_or(false))
                );
                println!(
                    "Proxy config: {} ({})",
                    yes_no(
                        data["app"]["proxy_config_present"]
                            .as_bool()
                            .unwrap_or(false)
                    ),
                    data["app"]["proxy_config_path"].as_str().unwrap_or("-")
                );
                println!("Angie valid:  {}", yes_no(angie_valid));

                if let Some(domains) = data["app"]["domains"].as_array() {
                    println!("Domains:      {}", join_strings(domains));
                }
                if let Some(upstreams) = data["app"]["upstreams"].as_array() {
                    let rendered = upstreams
                        .iter()
                        .map(|upstream| {
                            format!(
                                "{}:{}",
                                upstream["host"].as_str().unwrap_or("-"),
                                upstream["port"].as_u64().unwrap_or(0)
                            )
                        })
                        .collect::<Vec<_>>()
                        .join(", ");
                    println!(
                        "Upstreams:    {}",
                        if rendered.is_empty() {
                            "-".to_string()
                        } else {
                            rendered
                        }
                    );
                }
                println!(
                    "Certificate:  {}",
                    data["app"]["certificate"]["path"].as_str().unwrap_or("-")
                );
                println!(
                    "Private key:  {}",
                    data["app"]["private_key"]["path"].as_str().unwrap_or("-")
                );

                if let Some(issues) = data["app"]["issues"].as_array() {
                    if issues.is_empty() {
                        println!("Issues:       none");
                    } else {
                        println!("Issues:       {}", join_strings(issues));
                    }
                }

                if let Some(error) = data["angie"]["validation_error"].as_str() {
                    println!("Angie error:  {error}");
                }
            } else {
                let data = client.get("/api/routing/status").await?;
                println!(
                    "Angie config valid: {}",
                    yes_no(data["angie"]["config_valid"].as_bool().unwrap_or(false))
                );
                if let Some(error) = data["angie"]["validation_error"].as_str() {
                    println!("Angie validation error: {error}");
                }
                println!();
                println!(
                    "{:<20} {:<10} {:<5} {:<5} {:<5} {:<7} {}",
                    "APP", "STATUS", "TLS", "CFG", "CERT", "UPS", "ISSUES"
                );
                println!("{}", "-".repeat(80));

                if let Some(apps) = data["apps"].as_array() {
                    for app in apps {
                        let issues = app["issues"]
                            .as_array()
                            .map(|items| items.len())
                            .unwrap_or(0);
                        let upstreams = app["upstreams"]
                            .as_array()
                            .map(|items| items.len())
                            .unwrap_or(0);
                        println!(
                            "{:<20} {:<10} {:<5} {:<5} {:<5} {:<7} {}",
                            app["app"].as_str().unwrap_or("-"),
                            app["status"].as_str().unwrap_or("-"),
                            yes_no(app["tls_enabled"].as_bool().unwrap_or(false)),
                            yes_no(app["proxy_config_present"].as_bool().unwrap_or(false)),
                            yes_no(app["tls_ready"].as_bool().unwrap_or(false)),
                            upstreams,
                            issues
                        );
                    }
                }
            }
        }
    }
    Ok(())
}

fn yes_no(value: bool) -> &'static str {
    if value {
        "yes"
    } else {
        "no"
    }
}

fn join_strings(items: &[serde_json::Value]) -> String {
    let joined = items
        .iter()
        .filter_map(serde_json::Value::as_str)
        .collect::<Vec<_>>()
        .join(", ");

    if joined.is_empty() {
        "-".to_string()
    } else {
        joined
    }
}
