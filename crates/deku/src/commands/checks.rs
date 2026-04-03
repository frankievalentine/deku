use anyhow::Result;
use clap::{Args, Subcommand};
use percent_encoding::{utf8_percent_encode, NON_ALPHANUMERIC};

use crate::client::DekuClient;

#[derive(Debug, Args)]
pub struct ChecksArgs {
    #[command(subcommand)]
    command: ChecksCommands,
}

#[derive(Debug, Subcommand)]
enum ChecksCommands {
    /// Run live runtime and routing checks for an app
    Run {
        app: String,
        #[arg(long, help = "HTTP path to probe on the live app")]
        path: Option<String>,
        #[arg(long, help = "Per-request timeout in seconds", default_value_t = 5)]
        timeout: u64,
    },
    /// Show consolidated routing and TLS status
    Routing { app: Option<String> },
}

pub async fn run(args: ChecksArgs, client: &DekuClient) -> Result<()> {
    match args.command {
        ChecksCommands::Run { app, path, timeout } => {
            let mut endpoint = format!("/api/apps/{app}/checks?timeout_secs={timeout}");
            if let Some(path) = path.as_deref() {
                endpoint.push_str("&path=");
                endpoint.push_str(&utf8_percent_encode(path, NON_ALPHANUMERIC).to_string());
            }

            let data = client.get(&endpoint).await?;

            println!("App:            {}", data["app"].as_str().unwrap_or(&app));
            println!("Checks status:  {}", data["status"].as_str().unwrap_or("-"));

            if data["latest_deployment"].is_object() {
                println!(
                    "Deployment:     {} ({})",
                    data["latest_deployment"]["id"]
                        .as_str()
                        .map(short_id)
                        .unwrap_or_else(|| "-".to_string()),
                    data["latest_deployment"]["status"].as_str().unwrap_or("-")
                );
                println!(
                    "Builder:        {}",
                    data["latest_deployment"]["builder"].as_str().unwrap_or("-")
                );
                println!(
                    "Image:          {}",
                    data["latest_deployment"]["image_tag"]
                        .as_str()
                        .unwrap_or("-")
                );
            } else {
                println!("Deployment:     none");
            }

            let running = data["containers"]
                .as_array()
                .map(|items| items.len())
                .unwrap_or(0);
            println!("Containers:     {running} running");

            if let Some(ports) = data["port_mappings"].as_array() {
                let rendered = ports
                    .iter()
                    .map(|port| {
                        format!(
                            "{}->{}{}",
                            port["host_port"].as_i64().unwrap_or_default(),
                            port["container_port"].as_i64().unwrap_or_default(),
                            match port["protocol"].as_str() {
                                Some(protocol) if !protocol.is_empty() => format!("/{protocol}"),
                                _ => String::new(),
                            }
                        )
                    })
                    .collect::<Vec<_>>()
                    .join(", ");
                println!(
                    "Ports:          {}",
                    if rendered.is_empty() {
                        "-".to_string()
                    } else {
                        rendered
                    }
                );
            }

            println!(
                "Routing:        {}",
                data["routing"]["status"].as_str().unwrap_or("-")
            );
            if let Some(domains) = data["routing"]["domains"].as_array() {
                println!("Domains:        {}", join_strings(domains));
            }

            if data["probe"].is_object() {
                println!(
                    "HTTP probe:     {} {}{}",
                    yes_no(data["probe"]["ok"].as_bool().unwrap_or(false)),
                    data["probe"]["target"].as_str().unwrap_or("-"),
                    data["probe"]["path"].as_str().unwrap_or("-")
                );
                if let Some(status_code) = data["probe"]["status_code"].as_u64() {
                    println!("Probe status:   {status_code}");
                }
                if let Some(latency) = data["probe"]["latency_ms"].as_u64() {
                    println!("Probe latency:  {latency}ms");
                }
                if let Some(error) = data["probe"]["error"].as_str() {
                    println!("Probe error:    {error}");
                }
            } else {
                println!("HTTP probe:     no");
            }

            if let Some(issues) = data["issues"].as_array() {
                if issues.is_empty() {
                    println!("Issues:         none");
                } else {
                    println!("Issues:         {}", join_strings(issues));
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
                    "{:<20} {:<10} {:<5} {:<5} {:<5} {:<7} ISSUES",
                    "APP", "STATUS", "TLS", "CFG", "CERT", "UPS"
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

fn short_id(value: &str) -> String {
    value.chars().take(8).collect()
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
