use anyhow::Result;
use clap::{ArgAction, CommandFactory, Parser, Subcommand};
use deku_core::version::release_version;

mod client;
mod commands;
mod local_config;
mod prompt;

#[derive(Debug, Parser)]
#[command(
    name = "deku",
    about = "Deku PaaS CLI",
    disable_version_flag = true,
    arg_required_else_help = true
)]
struct Cli {
    #[arg(
        short = 'v',
        long = "version",
        action = ArgAction::SetTrue,
        global = true,
        help = "Print the current Deku release version"
    )]
    version_requested: bool,
    #[command(subcommand)]
    command: Option<Commands>,
}

#[derive(Debug, Subcommand)]
enum Commands {
    /// Print the current Deku release version
    Version(commands::version::VersionArgs),
    /// Restart the local Deku systemd service
    Restart(commands::restart::RestartArgs),
    /// First-run setup wizard
    Setup(commands::setup::SetupArgs),
    /// Show dashboard access details
    Dashboard(commands::dashboard::DashboardArgs),
    /// Remove a packaged Deku install from this host
    Uninstall(commands::uninstall::UninstallArgs),
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

    if cli.version_requested {
        println!("{}", release_version());
        return Ok(());
    }

    match cli.command {
        Some(Commands::Version(args)) => commands::version::run(args),
        Some(Commands::Restart(args)) => commands::restart::run(args),
        Some(Commands::Setup(args)) => commands::setup::run(args),
        Some(Commands::Uninstall(args)) => commands::uninstall::run(args).await,
        Some(command) => {
            let client = client::DekuClient::new()?;
            match command {
                Commands::Version(_)
                | Commands::Restart(_)
                | Commands::Setup(_)
                | Commands::Uninstall(_) => unreachable!(),
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
        None => {
            Cli::command().print_help()?;
            println!();
            Ok(())
        }
    }
}
