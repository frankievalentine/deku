use deku_core::error::{DekuError, Result};
use deku_core::types::{App, AppStatus, NewApp};
use sqlx::SqlitePool;

pub async fn create_app(pool: &SqlitePool, new_app: &NewApp) -> Result<App> {
    let app = App::new(&new_app.name);

    sqlx::query!(
        r#"INSERT INTO apps (id, name, created_at, locked, status)
           VALUES (?1, ?2, ?3, ?4, ?5)"#,
        app.id,
        app.name,
        app.created_at,
        app.locked,
        app.status,
    )
    .execute(pool)
    .await?;

    Ok(app)
}

pub async fn get_app(pool: &SqlitePool, name: &str) -> Result<App> {
    sqlx::query_as!(
        App,
        r#"SELECT
            id       as "id!",
            name     as "name!",
            created_at as "created_at!: _",
            locked   as "locked!",
            status   as "status!: AppStatus"
           FROM apps WHERE name = ?1"#,
        name
    )
    .fetch_optional(pool)
    .await?
    .ok_or_else(|| DekuError::AppNotFound(name.to_string()))
}

pub async fn list_apps(pool: &SqlitePool) -> Result<Vec<App>> {
    let apps = sqlx::query_as!(
        App,
        r#"SELECT
            id       as "id!",
            name     as "name!",
            created_at as "created_at!: _",
            locked   as "locked!",
            status   as "status!: AppStatus"
           FROM apps ORDER BY name"#,
    )
    .fetch_all(pool)
    .await?;

    Ok(apps)
}

pub async fn delete_app(pool: &SqlitePool, name: &str) -> Result<()> {
    let result = sqlx::query!("DELETE FROM apps WHERE name = ?1", name)
        .execute(pool)
        .await?;

    if result.rows_affected() == 0 {
        return Err(DekuError::AppNotFound(name.to_string()));
    }

    Ok(())
}
