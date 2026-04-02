use anyhow::Result;
use clap::{Parser, Subcommand};

mod client;
mod commands;
mod prompt;

#[derive(Debug, Parser)]
#[command(name = "deku", about = "Deku PaaS CLI", version)]
struct Cli {
    #[command(subcommand)]
    command: Commands,
}

#[derive(Debug, Subcommand)]
enum Commands {
    /// First-run setup wizard
    Setup,
    /// Application management
    Apps(commands::apps::AppsArgs),
    /// Configuration variables
    Config(commands::config::ConfigArgs),
    /// Deployment operations
    Deploy(commands::deploy::DeployArgs),
    /// Process management
    Ps(commands::ps::PsArgs),
    /// Domain management
    Domains(commands::domains::DomainsArgs),
    /// SSH key management
    Ssh(commands::ssh::SshArgs),
    /// Stream application logs
    Logs(commands::logs::LogsArgs),
    /// Plugin management
    Plugins(commands::plugins::PluginsArgs),
}

#[tokio::main]
async fn main() -> Result<()> {
    let cli = Cli::parse();

    if let Commands::Setup = cli.command {
        return commands::setup::run();
    }

    let client = client::DekuClient::new()?;

    match cli.command {
        Commands::Setup => unreachable!(),
        Commands::Apps(args) => commands::apps::run(args, &client).await,
        Commands::Config(args) => commands::config::run(args, &client).await,
        Commands::Deploy(args) => commands::deploy::run(args, &client).await,
        Commands::Ps(args) => commands::ps::run(args, &client).await,
        Commands::Domains(args) => commands::domains::run(args, &client).await,
        Commands::Ssh(args) => commands::ssh::run(args, &client).await,
        Commands::Logs(args) => commands::logs::run(args, &client).await,
        Commands::Plugins(args) => commands::plugins::run(args, &client).await,
    }
}
