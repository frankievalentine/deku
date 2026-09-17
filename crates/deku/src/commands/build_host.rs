use anyhow::{anyhow, Result};
use clap::{Args, Subcommand};

use crate::client::DekuClient;

#[derive(Debug, Args)]
pub struct BuildHostArgs {
    #[command(subcommand)]
    command: BuildHostCommands,
}

#[derive(Debug, Subcommand)]
enum BuildHostCommands {
    /// Configure the SSH build host that offloads image builds
    Setup {
        #[arg(long, help = "SSH destination: user@host or ssh://user@host[:port]")]
        host: Option<String>,
        #[arg(
            long,
            help = "Logical name accepted by --build-host",
            default_value = "builder"
        )]
        name: String,
        #[arg(long, help = "Private key passed to ssh -i")]
        identity_file: Option<String>,
        #[arg(
            long,
            help = "BuildKit endpoint railpack uses on the build host",
            default_value = "docker-container://deku-buildkit"
        )]
        buildkit_host: String,
    },
    /// Show the configured build host and registry
    Info,
    /// Probe the build host for ssh, docker, railpack and BuildKit
    Check,
    /// Create or start the managed BuildKit container on the build host
    Init,
    /// Remove the build host configuration
    Unset,
}

pub async fn run(args: BuildHostArgs, client: &DekuClient) -> Result<()> {
    match args.command {
        BuildHostCommands::Setup {
            host,
            name,
            identity_file,
            buildkit_host,
        } => {
            let Some(host) = host else {
                return Err(anyhow!(
                    "--host is required (user@host or ssh://user@host[:port])"
                ));
            };
            if host.trim().is_empty() {
                return Err(anyhow!("--host cannot be empty"));
            }

            let body = serde_json::json!({
                "name": name,
                "host": host,
                "identity_file": identity_file,
                "buildkit_host": buildkit_host,
            });
            let response = client.post("/api/build-host", body).await?;
            if response["configured"].as_bool().unwrap_or(false) {
                println!(
                    "Build host '{}' configured.",
                    response["build_host"]["name"].as_str().unwrap_or(&name)
                );
            } else {
                println!("Build host configured.");
            }
            println!("Run 'deku build-host check' to verify the remote toolchain.");
        }
        BuildHostCommands::Info => {
            let response = client.get("/api/build-host").await?;
            print_info(&response);
        }
        BuildHostCommands::Check => {
            let response = client
                .post("/api/build-host/check", serde_json::json!({}))
                .await?;
            let ok = response["ok"].as_bool().unwrap_or(false);
            if let Some(checks) = response["checks"].as_array() {
                println!("{:<16} {:<8} DETAIL", "CHECK", "STATE");
                println!("{}", "-".repeat(72));
                for check in checks {
                    println!(
                        "{:<16} {:<8} {}",
                        check["name"].as_str().unwrap_or("-"),
                        check["status"].as_str().unwrap_or("-"),
                        check["detail"].as_str().unwrap_or("-"),
                    );
                }
            }
            if !ok {
                std::process::exit(1);
            }
        }
        BuildHostCommands::Init => {
            let response = client
                .post("/api/build-host/init", serde_json::json!({}))
                .await?;
            if let Some(notes) = response["notes"].as_array() {
                for note in notes {
                    println!("{}", note.as_str().unwrap_or("-"));
                }
            }
        }
        BuildHostCommands::Unset => {
            client.delete("/api/build-host").await?;
            println!("Build host configuration removed.");
        }
    }

    Ok(())
}

fn print_info(response: &serde_json::Value) {
    if !response["configured"].as_bool().unwrap_or(false) {
        println!("Build host is not configured.");
    } else {
        let host = &response["build_host"];
        println!("Build host: {}", host["name"].as_str().unwrap_or("-"));
        println!("  Host: {}", host["host"].as_str().unwrap_or("-"));
        println!(
            "  Identity file: {}",
            host["identity_file"].as_str().unwrap_or("-")
        );
        println!(
            "  BuildKit: {}",
            host["buildkit_host"].as_str().unwrap_or("-")
        );
    }

    match response["registry"].as_object() {
        Some(registry) => {
            let server = registry
                .get("server")
                .and_then(|value| value.as_str())
                .unwrap_or("-");
            let namespace = registry
                .get("namespace")
                .and_then(|value| value.as_str())
                .unwrap_or("");
            let repository = if namespace.is_empty() {
                format!("{}/<app>", server.trim_end_matches('/'))
            } else {
                format!("{}/{namespace}/<app>", server.trim_end_matches('/'))
            };
            println!("Registry: {server}");
            println!("  Repository: {repository}");
            println!("  Namespace: {namespace}");
            println!(
                "  Username: {}",
                registry
                    .get("username")
                    .and_then(|value| value.as_str())
                    .unwrap_or("-")
            );
            println!(
                "  Password: {}",
                registry
                    .get("password")
                    .and_then(|value| value.as_str())
                    .unwrap_or("-")
            );
        }
        None => println!("Registry is not configured (required for remote builds)."),
    }
}
