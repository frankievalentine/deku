use anyhow::Result;
use tracing::info;

mod api;
mod build;
mod config;
mod container;
mod db;
mod deploy;
mod events;
mod plugins;
mod proxy;
mod ssh;

#[tokio::main]
async fn main() -> Result<()> {
    // Logging is initialised in config before anything else
    config::init_logging()?;
    info!("dekud starting");

    let cfg = config::load()?;
    let pool = db::connect(&cfg).await?;
    db::migrate(&pool).await?;

    let event_bus = events::EventBus::new();
    let state = api::AppState::new(cfg.clone(), pool, event_bus);

    tokio::try_join!(api::serve(state.clone()), ssh::serve(state.clone()),)?;

    Ok(())
}
