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
    config::init_logging()?;
    info!("dekud starting");

    let cfg = config::load()?;
    let pool = db::connect(&cfg).await?;
    db::migrate(&pool).await?;

    let docker = container::connect()?;
    info!("connected to docker daemon");

    let event_bus = events::EventBus::new(pool.clone());
    let state = api::AppState::new(cfg.clone(), pool, event_bus, docker);

    tokio::try_join!(api::serve(state.clone()), ssh::serve(state.clone()),)?;

    Ok(())
}
