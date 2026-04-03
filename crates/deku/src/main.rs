use anyhow::Result;
use clap::{Parser, Subcommand};

mod client;
mod commands;
mod local_config;
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
    Setup(commands::setup::SetupArgs),
    /// Show dashboard access details
    Dashboard(commands::dashboard::DashboardArgs),
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
    /// Postgres managed database
    Postgres(commands::postgres::PostgresArgs),
    /// Redis managed cache
    Redis(commands::redis::RedisArgs),
    /// MySQL managed database
    Mysql(commands::mysql::MysqlArgs),
    /// TLS certificate management
    Letsencrypt(commands::letsencrypt::LetsencryptArgs),
    /// Docker network management
    Network(commands::network::NetworkArgs),
    /// S3-compatible object storage configuration
    Objectstore(commands::objectstore::ObjectStoreArgs),
    /// Persistent storage management
    Storage(commands::storage::StorageArgs),
    /// Cron job management
    Cron(commands::cron::CronArgs),
    /// Git deployment settings
    Git(commands::git::GitArgs),
    /// Deployment health checks
    Checks(commands::checks::ChecksArgs),
}

#[tokio::main]
async fn main() -> Result<()> {
    let cli = Cli::parse();

    if let Commands::Setup(args) = cli.command {
        return commands::setup::run(args);
    }

    let client = client::DekuClient::new()?;

    match cli.command {
        Commands::Setup(_) => unreachable!(),
        Commands::Dashboard(args) => commands::dashboard::run(args, &client).await,
        Commands::Apps(args) => commands::apps::run(args, &client).await,
        Commands::Config(args) => commands::config::run(args, &client).await,
        Commands::Deploy(args) => commands::deploy::run(args, &client).await,
        Commands::Ps(args) => commands::ps::run(args, &client).await,
        Commands::Domains(args) => commands::domains::run(args, &client).await,
        Commands::Ssh(args) => commands::ssh::run(args, &client).await,
        Commands::Logs(args) => commands::logs::run(args, &client).await,
        Commands::Plugins(args) => commands::plugins::run(args, &client).await,
        Commands::Postgres(args) => commands::postgres::run(args, &client).await,
        Commands::Redis(args) => commands::redis::run(args, &client).await,
        Commands::Mysql(args) => commands::mysql::run(args, &client).await,
        Commands::Letsencrypt(args) => commands::letsencrypt::run(args, &client).await,
        Commands::Network(args) => commands::network::run(args, &client).await,
        Commands::Objectstore(args) => commands::objectstore::run(args, &client).await,
        Commands::Storage(args) => commands::storage::run(args, &client).await,
        Commands::Cron(args) => commands::cron::run(args, &client).await,
        Commands::Git(args) => commands::git::run(args, &client).await,
        Commands::Checks(args) => commands::checks::run(args, &client).await,
    }
}
