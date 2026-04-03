use anyhow::Result;
use tracing::info;

mod api;
mod build;
mod config;
mod container;
mod db;
mod deploy;
mod events;
mod objectstore;
mod plugins;
mod proxy;
mod services;
mod ssh;

#[tokio::main]
async fn main() -> Result<()> {
    let cfg = config::load()?;
    config::init_logging(&cfg)?;
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
    let state = api::AppState::new(cfg.clone(), pool, event_bus, docker, plugin_registry);

    tokio::try_join!(api::serve(state.clone()), ssh::serve(state.clone()),)?;

    Ok(())
}
