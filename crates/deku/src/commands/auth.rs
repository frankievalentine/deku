use anyhow::Result;
use clap::{Args, Subcommand};

use crate::client::DekuClient;

#[derive(Debug, Args)]
pub struct AuthArgs {
    #[command(subcommand)]
    command: AuthCommands,
}

#[derive(Debug, Subcommand)]
enum AuthCommands {
    /// Require HTTP basic auth (one shared username/password) for an app
    Enable {
        app: String,
        #[arg(long, help = "Username")]
        user: String,
        #[arg(long, help = "Password (prompted when omitted)")]
        password: Option<String>,
    },
    /// Delegate auth to an external forward-auth endpoint
    Forward {
        app: String,
        #[arg(long, help = "Forward-auth URL, e.g. https://auth.example/verify")]
        url: String,
    },
    /// Remove authentication from an app
    Disable { app: String },
    /// Show an app's authentication status
    Status { app: String },
}

pub async fn run(args: AuthArgs, client: &DekuClient) -> Result<()> {
    match args.command {
        AuthCommands::Enable {
            app,
            user,
            password,
        } => {
            let password = match password {
                Some(password) => password,
                None => cliclack::password("Password").interact()?,
            };
            client
                .post(
                    &format!("/api/apps/{app}/auth"),
                    serde_json::json!({ "mode": "basic", "username": user, "password": password }),
                )
                .await?;
            println!("Basic auth enabled for '{app}' (user: {user}).");
        }
        AuthCommands::Forward { app, url } => {
            client
                .post(
                    &format!("/api/apps/{app}/auth"),
                    serde_json::json!({ "mode": "forward", "forward_url": url }),
                )
                .await?;
            println!("Forward auth enabled for '{app}' (endpoint: {url}).");
        }
        AuthCommands::Disable { app } => {
            client.delete(&format!("/api/apps/{app}/auth")).await?;
            println!("Auth disabled for '{app}'.");
        }
        AuthCommands::Status { app } => {
            let data = client.get(&format!("/api/apps/{app}/auth")).await?;
            if data["configured"].as_bool().unwrap_or(false) {
                let mode = data["mode"].as_str().unwrap_or("-");
                println!("Mode: {mode}");
                if let Some(user) = data["username"].as_str() {
                    println!("Username: {user}");
                }
                if let Some(url) = data["forward_url"].as_str() {
                    println!("Forward URL: {url}");
                }
            } else {
                println!("No authentication configured for '{app}'.");
            }
        }
    }
    Ok(())
}
