use anyhow::Result;
use tracing::{info, warn};

mod acme;
mod alerts;
mod api;
mod app_name;
mod backup_scheduler;
mod build;
mod build_remote;
mod buildkit;
mod config;
mod console;
mod container;
mod crypto;
mod db;
mod deploy;
mod deploy_lock;
mod events;
mod hooks;
mod logs;
mod metrics;
mod objectstore;
mod plugins;
mod proxy;
mod secrets;
mod services;
mod ssh;
mod version;

#[tokio::main]
async fn main() -> Result<()> {
    let cfg = config::load()?;
    config::init_logging(&cfg)?;
    if config::dashboard_assets_available(&cfg) {
        info!(dashboard_dir = %cfg.dashboard_dir.display(), "dashboard assets available");
    } else {
        warn!(
            dashboard_dir = %cfg.dashboard_dir.display(),
            "dashboard assets missing; serving a fallback page until a dashboard bundle is staged"
        );
    }
    info!("dekud starting");

    let pool = db::connect(&cfg).await?;
    db::migrate(&pool).await?;

    let docker = container::connect()?;
    info!("connected to docker daemon");

    let plugins_dir = cfg.data_dir.join("plugins");
    let plugin_registry = plugins::PluginRegistry::new();
    if let Err(e) = plugin_registry.load_all(&plugins_dir).await {
        tracing::warn!("plugin load error: {e}");
    }

    let event_bus = events::EventBus::new(pool.clone());
    let log_bus = logs::LogBus::new(pool.clone());
    let state = api::AppState::new(
        cfg.clone(),
        pool,
        event_bus,
        log_bus,
        docker,
        plugin_registry,
    );

    backup_scheduler::spawn(state.clone());
    alerts::spawn(state.clone());
    logs::spawn_maintenance(state.clone());

    tokio::try_join!(api::serve(state.clone()), async {
        if let Err(error) = ssh::serve(state.clone()).await {
            warn!(
                error = %error,
                ssh_port = state.config.ssh_port,
                "SSH server unavailable; API remains online and CLI/API deploys still work"
            );
        }

        Ok::<(), anyhow::Error>(())
    },)?;

    Ok(())
}
