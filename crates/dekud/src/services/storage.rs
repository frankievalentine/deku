use anyhow::Result;
use deku_core::types::StorageMount;
use sqlx::SqlitePool;

use crate::db::queries;

pub fn ensure_directory(path: &str) -> Result<()> {
    std::fs::create_dir_all(path)?;
    Ok(())
}

pub async fn add_mount(
    pool: &SqlitePool,
    app_id: &str,
    host_path: &str,
    container_path: &str,
) -> Result<StorageMount> {
    queries::add_storage_mount(pool, app_id, host_path, container_path)
        .await
        .map_err(Into::into)
}

pub async fn remove_mount(pool: &SqlitePool, app_id: &str, mount_id: &str) -> Result<()> {
    queries::remove_storage_mount(pool, app_id, mount_id)
        .await
        .map_err(Into::into)
}

pub async fn list_mounts(pool: &SqlitePool, app_id: &str) -> Result<Vec<StorageMount>> {
    queries::list_storage_mounts(pool, app_id)
        .await
        .map_err(Into::into)
}
