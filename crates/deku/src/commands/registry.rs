use anyhow::{anyhow, Result};
use clap::{Args, Subcommand};

use crate::client::DekuClient;

#[derive(Debug, Args)]
pub struct RegistryArgs {
    #[command(subcommand)]
    command: RegistryCommands,
}

#[derive(Debug, Subcommand)]
enum RegistryCommands {
    /// Configure the Docker registry used to move images off a build host
    Setup {
        #[arg(long, help = "Registry host with optional owner, e.g. ghcr.io/acme")]
        server: Option<String>,
        #[arg(long, help = "Registry username")]
        username: Option<String>,
        #[arg(long, help = "Registry password or scoped token")]
        password: Option<String>,
        #[arg(
            long,
            help = "Repository namespace inserted before the app name",
            default_value = "deku"
        )]
        namespace: String,
    },
    /// Show the configured registry (password redacted)
    Info,
    /// Remove the registry configuration
    Unset,
}

pub async fn run(args: RegistryArgs, client: &DekuClient) -> Result<()> {
    match args.command {
        RegistryCommands::Setup {
            server,
            username,
            password,
            namespace,
        } => {
            let Some(server) = server else {
                return Err(anyhow!("--server is required, e.g. ghcr.io/acme"));
            };
            if server.trim().is_empty() {
                return Err(anyhow!("--server cannot be empty"));
            }

            let username = match username {
                Some(username) if !username.trim().is_empty() => Some(username),
                Some(_) => return Err(anyhow!("--username cannot be empty")),
                None => None,
            };

            let password = match (password, username.as_ref()) {
                (Some(password), _) => Some(password),
                (None, Some(_)) => {
                    let entered: String =
                        cliclack::password("Registry password or token").interact()?;
                    if entered.is_empty() {
                        return Err(anyhow!("password cannot be empty"));
                    }
                    Some(entered)
                }
                (None, None) => None,
            };

            let body = serde_json::json!({
                "server": server,
                "username": username,
                "password": password,
                "namespace": namespace,
            });
            let response = client.post("/api/registry", body).await?;
            let registry = &response["registry"];
            println!(
                "Registry '{}' configured (repository {}/{}/<app>).",
                registry["server"].as_str().unwrap_or(&server),
                registry["server"]
                    .as_str()
                    .unwrap_or(&server)
                    .trim_end_matches('/'),
                registry["namespace"].as_str().unwrap_or(&namespace),
            );
        }
        RegistryCommands::Info => {
            let response = client.get("/api/registry").await?;
            if !response["configured"].as_bool().unwrap_or(false) {
                println!("Registry is not configured.");
                return Ok(());
            }
            let registry = &response["registry"];
            println!("Server: {}", registry["server"].as_str().unwrap_or("-"));
            println!(
                "Namespace: {}",
                registry["namespace"].as_str().unwrap_or("-")
            );
            println!("Username: {}", registry["username"].as_str().unwrap_or("-"));
            println!("Password: {}", registry["password"].as_str().unwrap_or("-"));
        }
        RegistryCommands::Unset => {
            client.delete("/api/registry").await?;
            println!("Registry configuration removed.");
        }
    }

    Ok(())
}
