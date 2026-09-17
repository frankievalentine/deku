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
    /// Per-app HTTP authentication
    Auth(commands::auth::AuthArgs),
    /// Scheduled service backups
    Backup(commands::backup::BackupArgs),
    /// Serve a 503 for an app (maintenance mode)
    Maintenance(commands::maintenance::MaintenanceArgs),
    /// Per-app redirects
    Redirects(commands::redirects::RedirectsArgs),
    /// Diagnose host and daemon health
    Doctor(commands::doctor::DoctorArgs),
    /// Show active alerts
    Alerts(commands::alerts::AlertsArgs),
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
    /// Run a one-off command in a fresh container
    Run(commands::console::RunArgs),
    /// Run a command inside a running app container
    Exec(commands::console::ExecArgs),
    /// Plugin management
    Plugins(commands::plugins::PluginsArgs),
    /// Postgres managed database
    Postgres(commands::postgres::PostgresArgs),
    /// Redis managed cache
    Redis(commands::redis::RedisArgs),
    /// MySQL managed database
    Mysql(commands::mysql::MysqlArgs),
    /// MariaDB managed database
    Mariadb(commands::service::ServiceArgs),
    /// MongoDB managed database
    Mongodb(commands::service::ServiceArgs),
    /// TLS certificate management
    Letsencrypt(commands::letsencrypt::LetsencryptArgs),
    /// Docker network management
    Network(commands::network::NetworkArgs),
    /// S3-compatible object storage configuration
    Objectstore(commands::objectstore::ObjectStoreArgs),
    /// SSH build host that offloads image builds
    BuildHost(commands::build_host::BuildHostArgs),
    /// Docker registry used to transfer images from a build host
    Registry(commands::registry::RegistryArgs),
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
                Commands::Auth(args) => commands::auth::run(args, &client).await,
                Commands::Backup(args) => commands::backup::run(args, &client).await,
                Commands::Maintenance(args) => commands::maintenance::run(args, &client).await,
                Commands::Redirects(args) => commands::redirects::run(args, &client).await,
                Commands::Doctor(args) => commands::doctor::run(args, &client).await,
                Commands::Alerts(args) => commands::alerts::run(args, &client).await,
                Commands::Config(args) => commands::config::run(args, &client).await,
                Commands::Deploy(args) => commands::deploy::run(args, &client).await,
                Commands::Ps(args) => commands::ps::run(args, &client).await,
                Commands::Domains(args) => commands::domains::run(args, &client).await,
                Commands::Ssh(args) => commands::ssh::run(args, &client).await,
                Commands::Logs(args) => commands::logs::run(args, &client).await,
                Commands::Run(args) => commands::console::run(args, &client).await,
                Commands::Exec(args) => commands::console::exec(args, &client).await,
                Commands::Plugins(args) => commands::plugins::run(args, &client).await,
                Commands::Postgres(args) => commands::postgres::run(args, &client).await,
                Commands::Redis(args) => commands::redis::run(args, &client).await,
                Commands::Mysql(args) => commands::mysql::run(args, &client).await,
                Commands::Mariadb(args) => commands::service::run("mariadb", args, &client).await,
                Commands::Mongodb(args) => commands::service::run("mongodb", args, &client).await,
                Commands::Letsencrypt(args) => commands::letsencrypt::run(args, &client).await,
                Commands::Network(args) => commands::network::run(args, &client).await,
                Commands::Objectstore(args) => commands::objectstore::run(args, &client).await,
                Commands::BuildHost(args) => commands::build_host::run(args, &client).await,
                Commands::Registry(args) => commands::registry::run(args, &client).await,
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
