use crate::client::DekuClient;
use anyhow::Result;
use clap::{Args, Subcommand};

#[derive(Debug, Args)]
pub struct SshArgs {
    #[command(subcommand)]
    command: SshCommands,
}

#[derive(Debug, Subcommand)]
enum SshCommands {
    /// Add an SSH public key
    Add {
        #[arg(help = "Key name")]
        name: String,
        #[arg(help = "Public key (or path to .pub file)")]
        key: String,
    },
    /// List SSH keys
    List,
    /// Remove an SSH key
    Remove {
        #[arg(help = "Key name")]
        name: String,
    },
}

pub async fn run(args: SshArgs, client: &DekuClient) -> Result<()> {
    match args.command {
        SshCommands::Add { name, key } => {
            // If `key` looks like a file path, read it
            let public_key = if key.ends_with(".pub") && std::path::Path::new(&key).exists() {
                std::fs::read_to_string(&key)?.trim().to_string()
            } else {
                key
            };
            client
                .post(
                    "/api/ssh-keys",
                    serde_json::json!({ "name": name, "public_key": public_key }),
                )
                .await?;
            println!("SSH key '{name}' added.");
        }

        SshCommands::List => {
            let data = client.get("/api/ssh-keys").await?;
            if let Some(keys) = data.as_array() {
                if keys.is_empty() {
                    println!("No SSH keys registered.");
                } else {
                    println!("{:<20} FINGERPRINT", "NAME");
                    println!("{}", "-".repeat(60));
                    for k in keys {
                        let name = k["name"].as_str().unwrap_or("-");
                        let fp = k["fingerprint"].as_str().unwrap_or("-");
                        println!("{:<20} {}", name, fp);
                    }
                }
            }
        }

        SshCommands::Remove { name } => {
            client.delete(&format!("/api/ssh-keys/{name}")).await?;
            println!("SSH key '{name}' removed.");
        }
    }
    Ok(())
}
