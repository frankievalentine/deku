use anyhow::Result;
use sqlx::{sqlite::SqlitePoolOptions, SqlitePool};

use crate::config::DekuConfig;

pub mod queries;

pub async fn connect(cfg: &DekuConfig) -> Result<SqlitePool> {
    std::fs::create_dir_all(&cfg.data_dir)?;
    let db_path = cfg.data_dir.join("deku.db");
    let db_url = format!("sqlite://{}?mode=rwc", db_path.display());

    let pool = SqlitePoolOptions::new()
        .max_connections(5)
        .connect(&db_url)
        .await?;

    Ok(pool)
}

pub async fn migrate(pool: &SqlitePool) -> Result<()> {
    sqlx::migrate!("./migrations").run(pool).await?;
    Ok(())
}
