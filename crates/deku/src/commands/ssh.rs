use anyhow::Result;
use clap::{Args, Subcommand};
use crate::client::DekuClient;

#[derive(Debug, Args)]
pub struct SshArgs {
    #[command(subcommand)]
    command: SshCommands,
}

#[derive(Debug, Subcommand)]
enum SshCommands {
    /// Add an SSH key
    Add {
        #[arg(help = "Key name")]
        name: String,
    },
    /// List SSH keys
    List,
    /// Remove an SSH key
    Remove {
        #[arg(help = "Key name")]
        name: String,
    },
}

pub async fn run(_args: SshArgs, _client: &DekuClient) -> Result<()> {
    println!("ssh: not yet implemented");
    Ok(())
}
