use anyhow::Result;
use clap::{Args, Subcommand};
use crate::client::DekuClient;

#[derive(Debug, Args)]
pub struct PluginsArgs {
    #[command(subcommand)]
    command: PluginsCommands,
}

#[derive(Debug, Subcommand)]
enum PluginsCommands {
    /// List installed plugins
    List,
    /// Install a plugin
    Install {
        #[arg(help = "Plugin name or path")]
        plugin: String,
    },
    /// Uninstall a plugin
    Uninstall {
        #[arg(help = "Plugin name")]
        plugin: String,
    },
}

pub async fn run(_args: PluginsArgs, _client: &DekuClient) -> Result<()> {
    println!("plugins: not yet implemented");
    Ok(())
}
