use chrono::{DateTime, Utc};
use deku_core::error::{DekuError, Result};
use deku_core::types::{
    App, AppStatus, BuilderType, ConfigVar, ContainerRecord, DeployStatus, Deployment, Domain,
    Event, NewApp, PortMapping, ResourceLimit, StorageMount,
};
use sqlx::Row;
use sqlx::SqlitePool;
use uuid::Uuid;

// ── Apps ─────────────────────────────────────────────────────────────────────

pub async fn create_app(pool: &SqlitePool, new_app: &NewApp) -> Result<App> {
    let app = App::new(&new_app.name);

    sqlx::query!(
        r#"INSERT INTO apps (id, name, created_at, locked, status, tls_enabled)
           VALUES (?1, ?2, ?3, ?4, ?5, ?6)"#,
        app.id,
        app.name,
        app.created_at,
        app.locked,
        app.status,
        app.tls_enabled,
    )
    .execute(pool)
    .await?;

    // Every app starts with the environment that plain `deku deploy run` targets.
    ensure_production_environment(pool, &app.id).await?;

    Ok(app)
}

pub async fn get_app(pool: &SqlitePool, name: &str) -> Result<App> {
    sqlx::query_as!(
        App,
        r#"SELECT
            id          as "id!",
            name        as "name!",
            created_at  as "created_at!: _",
            locked      as "locked!: bool",
            status      as "status!: AppStatus",
            tls_enabled as "tls_enabled!: bool"
           FROM apps WHERE name = ?1"#,
        name
    )
    .fetch_optional(pool)
    .await?
    .ok_or_else(|| DekuError::AppNotFound(name.to_string()))
}

pub async fn get_app_by_id(pool: &SqlitePool, id: &str) -> Result<App> {
    sqlx::query_as!(
        App,
        r#"SELECT
            id          as "id!",
            name        as "name!",
            created_at  as "created_at!: _",
            locked      as "locked!: bool",
            status      as "status!: AppStatus",
            tls_enabled as "tls_enabled!: bool"
           FROM apps WHERE id = ?1"#,
        id
    )
    .fetch_optional(pool)
    .await?
    .ok_or_else(|| DekuError::AppNotFound(id.to_string()))
}

pub async fn list_apps(pool: &SqlitePool) -> Result<Vec<App>> {
    let apps = sqlx::query_as!(
        App,
        r#"SELECT
            id          as "id!",
            name        as "name!",
            created_at  as "created_at!: _",
            locked      as "locked!: bool",
            status      as "status!: AppStatus",
            tls_enabled as "tls_enabled!: bool"
           FROM apps ORDER BY name"#,
    )
    .fetch_all(pool)
    .await?;

    Ok(apps)
}

/// Rename an app, reporting a taken name as `AppAlreadyExists`.
pub async fn rename_app(pool: &SqlitePool, app_id: &str, new_name: &str) -> Result<()> {
    match get_app(pool, new_name).await {
        Ok(existing) if existing.id != app_id => {
            return Err(DekuError::AppAlreadyExists(new_name.to_string()));
        }
        Ok(_) => {}
        Err(DekuError::AppNotFound(_)) => {}
        Err(other) => return Err(other),
    }

    let result = sqlx::query("UPDATE apps SET name = ?2 WHERE id = ?1")
        .bind(app_id)
        .bind(new_name)
        .execute(pool)
        .await;

    match result {
        Ok(_) => Ok(()),
        Err(sqlx::Error::Database(error)) if error.is_unique_violation() => {
            Err(DekuError::AppAlreadyExists(new_name.to_string()))
        }
        Err(error) => Err(error.into()),
    }
}

/// What a clone copied, and what it deliberately left behind.
#[derive(Debug, Default, Clone, serde::Serialize)]
pub struct CloneSummary {
    pub config_vars: u64,
    pub resource_limits: u64,
    pub redirects: u64,
    pub auth: bool,
    pub skipped: Vec<&'static str>,
}

/// Copy the portable settings of one app onto another.
///
/// Only configuration that cannot collide with the source or duplicate work is
/// copied. Hostnames, published ports, storage paths, cron entries, and service
/// links stay behind because copying them would conflict or cause surprise.
pub async fn clone_app_settings(
    pool: &SqlitePool,
    source_app_id: &str,
    target_app_id: &str,
) -> Result<CloneSummary> {
    let mut summary = CloneSummary::default();

    for var in get_config_vars_raw(pool, source_app_id).await? {
        set_config_var_raw(pool, target_app_id, &var.key, &var.value, var.is_global).await?;
        summary.config_vars += 1;
    }

    for limit in list_resource_limits(pool, source_app_id).await? {
        set_resource_limit(
            pool,
            target_app_id,
            &limit.process_type,
            limit.cpu.as_deref(),
            limit.memory.as_deref(),
        )
        .await?;
        summary.resource_limits += 1;
    }

    for entry in list_redirects(pool, source_app_id).await? {
        add_redirect(
            pool,
            target_app_id,
            &entry.source_path,
            &entry.target,
            entry.code,
        )
        .await?;
        summary.redirects += 1;
    }

    if let Some(auth) = get_app_auth(pool, source_app_id).await? {
        match (auth.mode.as_str(), auth.username, auth.password_hash) {
            ("basic", Some(username), Some(password_hash)) => {
                upsert_app_auth_basic(pool, target_app_id, &username, &password_hash).await?;
                summary.auth = true;
            }
            ("forward", _, _) => {
                if let Some(forward_url) = auth.forward_url.as_deref() {
                    upsert_app_auth_forward(pool, target_app_id, forward_url).await?;
                    summary.auth = true;
                }
            }
            _ => {}
        }
    }

    summary.skipped = vec![
        "domains",
        "port mappings",
        "storage mounts",
        "cron entries",
        "service links",
        "deploy tokens",
    ];
    Ok(summary)
}

pub async fn delete_app(pool: &SqlitePool, name: &str) -> Result<()> {
    let app = get_app(pool, name).await?;
    let mut tx = pool.begin().await?;

    // Older schema tables do not consistently use ON DELETE CASCADE, so app deletion
    // needs to clear dependent rows explicitly before removing the app record itself.
    for statement in [
        "DELETE FROM events WHERE app_id = ?",
        "DELETE FROM containers WHERE app_id = ?",
        "DELETE FROM deployments WHERE app_id = ?",
        "DELETE FROM domains WHERE app_id = ?",
        "DELETE FROM config_vars WHERE app_id = ?",
        "DELETE FROM port_mappings WHERE app_id = ?",
        "DELETE FROM storage_mounts WHERE app_id = ?",
        "DELETE FROM resource_limits WHERE app_id = ?",
        "DELETE FROM process_scale WHERE app_id = ?",
        "DELETE FROM docker_options WHERE app_id = ?",
        "DELETE FROM app_networks WHERE app_id = ?",
        "DELETE FROM service_links WHERE app_id = ?",
        "DELETE FROM cron_entries WHERE app_id = ?",
        "DELETE FROM app_auth WHERE app_id = ?",
        "DELETE FROM redirects WHERE app_id = ?",
        "DELETE FROM app_deploy_tokens WHERE app_id = ?",
        "DELETE FROM environments WHERE app_id = ?",
    ] {
        sqlx::query(statement)
            .bind(&app.id)
            .execute(tx.as_mut())
            .await?;
    }

    let result = sqlx::query("DELETE FROM apps WHERE id = ?")
        .bind(&app.id)
        .execute(tx.as_mut())
        .await?;

    if result.rows_affected() == 0 {
        return Err(DekuError::AppNotFound(name.to_string()));
    }

    tx.commit().await?;
    Ok(())
}

pub async fn update_app_status(pool: &SqlitePool, app_id: &str, status: AppStatus) -> Result<()> {
    sqlx::query!("UPDATE apps SET status = ?1 WHERE id = ?2", status, app_id)
        .execute(pool)
        .await?;
    Ok(())
}

// ── Events ───────────────────────────────────────────────────────────────────

pub async fn persist_event(pool: &SqlitePool, event: &Event) -> Result<()> {
    sqlx::query!(
        r#"INSERT INTO events (id, app_id, event_type, payload, created_at)
           VALUES (?1, ?2, ?3, ?4, ?5)"#,
        event.id,
        event.app_id,
        event.event_type,
        event.payload,
        event.created_at,
    )
    .execute(pool)
    .await?;
    Ok(())
}

pub async fn list_events(
    pool: &SqlitePool,
    app_id: Option<&str>,
    since: Option<DateTime<Utc>>,
) -> Result<Vec<Event>> {
    let events = sqlx::query_as!(
        Event,
        r#"SELECT
            id         as "id!",
            app_id     as "app_id",
            event_type as "event_type!",
            payload    as "payload",
            created_at as "created_at!: _"
           FROM events
           WHERE (?1 IS NULL OR app_id = ?1)
             AND (?2 IS NULL OR created_at > ?2)
           ORDER BY created_at DESC
           LIMIT 1000"#,
        app_id,
        since,
    )
    .fetch_all(pool)
    .await?;
    Ok(events)
}

// ── Deployments ───────────────────────────────────────────────────────────────

pub async fn create_deployment(
    pool: &SqlitePool,
    app_id: &str,
    environment_id: &str,
    builder: BuilderType,
) -> Result<Deployment> {
    let dep = Deployment {
        id: Uuid::new_v4().to_string(),
        app_id: app_id.to_string(),
        status: DeployStatus::Pending,
        builder,
        image_tag: None,
        created_at: Utc::now(),
        finished_at: None,
    };

    sqlx::query(
        "INSERT INTO deployments (id, app_id, environment_id, status, builder, created_at) \
         VALUES (?1, ?2, ?3, ?4, ?5, ?6)",
    )
    .bind(&dep.id)
    .bind(&dep.app_id)
    .bind(environment_id)
    .bind(dep.status.to_string())
    .bind(dep.builder.to_string())
    .bind(dep.created_at)
    .execute(pool)
    .await?;

    Ok(dep)
}

pub async fn update_deployment(
    pool: &SqlitePool,
    id: &str,
    status: DeployStatus,
    image_tag: Option<&str>,
) -> Result<()> {
    let now = Utc::now();
    sqlx::query!(
        r#"UPDATE deployments
           SET status = ?1, image_tag = ?2, finished_at = ?3
           WHERE id = ?4"#,
        status,
        image_tag,
        now,
        id,
    )
    .execute(pool)
    .await?;
    Ok(())
}

pub async fn get_deployment(pool: &SqlitePool, id: &str) -> Result<Deployment> {
    sqlx::query_as!(
        Deployment,
        r#"SELECT
            id            as "id!",
            app_id        as "app_id!",
            status        as "status!: DeployStatus",
            builder       as "builder!: BuilderType",
            image_tag     as "image_tag",
            created_at    as "created_at!: _",
            finished_at   as "finished_at: _"
           FROM deployments WHERE id = ?1"#,
        id
    )
    .fetch_optional(pool)
    .await?
    .ok_or_else(|| DekuError::Internal(format!("deployment {id} not found")))
}

pub async fn get_latest_deployment(pool: &SqlitePool, app_id: &str) -> Result<Option<Deployment>> {
    let dep = sqlx::query_as!(
        Deployment,
        r#"SELECT
            id            as "id!",
            app_id        as "app_id!",
            status        as "status!: DeployStatus",
            builder       as "builder!: BuilderType",
            image_tag     as "image_tag",
            created_at    as "created_at!: _",
            finished_at   as "finished_at: _"
           FROM deployments
           WHERE app_id = ?1 AND status = 'live'
           ORDER BY created_at DESC
           LIMIT 1"#,
        app_id
    )
    .fetch_optional(pool)
    .await?;
    Ok(dep)
}

pub async fn get_previous_deployment(
    pool: &SqlitePool,
    app_id: &str,
    exclude_id: &str,
) -> Result<Option<Deployment>> {
    let dep = sqlx::query_as!(
        Deployment,
        r#"SELECT
            id            as "id!",
            app_id        as "app_id!",
            status        as "status!: DeployStatus",
            builder       as "builder!: BuilderType",
            image_tag     as "image_tag",
            created_at    as "created_at!: _",
            finished_at   as "finished_at: _"
           FROM deployments
           WHERE app_id = ?1 AND id != ?2 AND status = 'live'
           ORDER BY created_at DESC
           LIMIT 1"#,
        app_id,
        exclude_id
    )
    .fetch_optional(pool)
    .await?;
    Ok(dep)
}

pub async fn list_deployments(pool: &SqlitePool, app_id: &str) -> Result<Vec<Deployment>> {
    let deps = sqlx::query_as!(
        Deployment,
        r#"SELECT
            id            as "id!",
            app_id        as "app_id!",
            status        as "status!: DeployStatus",
            builder       as "builder!: BuilderType",
            image_tag     as "image_tag",
            created_at    as "created_at!: _",
            finished_at   as "finished_at: _"
           FROM deployments
           WHERE app_id = ?1
           ORDER BY created_at DESC"#,
        app_id
    )
    .fetch_all(pool)
    .await?;
    Ok(deps)
}

// ── Config vars ───────────────────────────────────────────────────────────────

/// App-wide config vars, i.e. the ones with no environment override.
///
/// Per-environment overrides live in the same table, so every read that predates
/// environments must exclude them explicitly.
pub async fn get_config_vars_raw(pool: &SqlitePool, app_id: &str) -> Result<Vec<ConfigVar>> {
    let vars = sqlx::query_as::<_, ConfigVar>(
        "SELECT app_id, key, value, is_global FROM config_vars \
         WHERE (app_id = ?1 OR is_global = TRUE) AND environment_id IS NULL \
         ORDER BY key",
    )
    .bind(app_id)
    .fetch_all(pool)
    .await?;
    Ok(vars)
}

/// Config var overrides that apply only inside one environment.
pub async fn get_environment_config_vars_raw(
    pool: &SqlitePool,
    app_id: &str,
    environment_id: &str,
) -> Result<Vec<ConfigVar>> {
    let rows = sqlx::query(
        "SELECT app_id, environment_id, key, value, is_global \
         FROM config_vars WHERE app_id = ?1 AND environment_id = ?2 ORDER BY key",
    )
    .bind(app_id)
    .bind(environment_id)
    .fetch_all(pool)
    .await?;

    rows.into_iter()
        .map(|row| {
            Ok(ConfigVar {
                app_id: row.try_get("app_id")?,
                key: row.try_get("key")?,
                value: row.try_get("value")?,
                is_global: row.try_get("is_global")?,
            })
        })
        .collect()
}

pub async fn set_config_var_raw(
    pool: &SqlitePool,
    app_id: &str,
    key: &str,
    value: &str,
    is_global: bool,
) -> Result<()> {
    sqlx::query(
        "INSERT INTO config_vars (app_id, environment_id, key, value, is_global) \
         VALUES (?1, NULL, ?2, ?3, ?4) \
         ON CONFLICT(app_id, key) WHERE environment_id IS NULL \
         DO UPDATE SET value = excluded.value, is_global = excluded.is_global",
    )
    .bind(app_id)
    .bind(key)
    .bind(value)
    .bind(is_global)
    .execute(pool)
    .await?;
    Ok(())
}

pub async fn unset_config_var(pool: &SqlitePool, app_id: &str, key: &str) -> Result<()> {
    // App-wide only: an environment override is removed with its environment.
    sqlx::query(
        "DELETE FROM config_vars WHERE app_id = ?1 AND key = ?2 AND environment_id IS NULL",
    )
    .bind(app_id)
    .bind(key)
    .execute(pool)
    .await?;
    Ok(())
}

// ── Environments ──────────────────────────────────────────────────────────────

/// The slug every app's implicit production environment uses.
pub const PRODUCTION_ENVIRONMENT_SLUG: &str = "production";

#[derive(Debug, Clone, serde::Serialize, sqlx::FromRow)]
pub struct Environment {
    pub id: String,
    pub app_id: String,
    pub name: String,
    pub slug: String,
    pub branch: Option<String>,
    pub is_production: bool,
    pub created_at: DateTime<Utc>,
}

pub async fn create_environment(
    pool: &SqlitePool,
    app_id: &str,
    name: &str,
    slug: &str,
    branch: Option<&str>,
    is_production: bool,
) -> Result<Environment> {
    let environment = Environment {
        id: Uuid::new_v4().to_string(),
        app_id: app_id.to_string(),
        name: name.to_string(),
        slug: slug.to_string(),
        branch: branch.map(str::to_string),
        is_production,
        created_at: Utc::now(),
    };

    sqlx::query(
        "INSERT INTO environments (id, app_id, name, slug, branch, is_production, created_at) \
         VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)",
    )
    .bind(&environment.id)
    .bind(&environment.app_id)
    .bind(&environment.name)
    .bind(&environment.slug)
    .bind(&environment.branch)
    .bind(environment.is_production)
    .bind(environment.created_at)
    .execute(pool)
    .await?;

    Ok(environment)
}

pub async fn list_environments(pool: &SqlitePool, app_id: &str) -> Result<Vec<Environment>> {
    Ok(sqlx::query_as::<_, Environment>(
        "SELECT id, app_id, name, slug, branch, is_production, created_at FROM environments \
         WHERE app_id = ?1 ORDER BY is_production DESC, slug",
    )
    .bind(app_id)
    .fetch_all(pool)
    .await?)
}

pub async fn get_environment(pool: &SqlitePool, app_id: &str, slug: &str) -> Result<Environment> {
    sqlx::query_as::<_, Environment>(
        "SELECT id, app_id, name, slug, branch, is_production, created_at FROM environments \
         WHERE app_id = ?1 AND slug = ?2",
    )
    .bind(app_id)
    .bind(slug)
    .fetch_optional(pool)
    .await?
    .ok_or_else(|| DekuError::EnvironmentNotFound(slug.to_string()))
}

/// Every app has a production environment; create it on first use.
///
/// The migration backfills existing apps, so this only does work for an app
/// created on an older code path or one whose environment was removed.
pub async fn ensure_production_environment(pool: &SqlitePool, app_id: &str) -> Result<Environment> {
    if let Ok(environment) = get_environment(pool, app_id, PRODUCTION_ENVIRONMENT_SLUG).await {
        return Ok(environment);
    }
    create_environment(
        pool,
        app_id,
        "production",
        PRODUCTION_ENVIRONMENT_SLUG,
        None,
        true,
    )
    .await
}

/// Delete a non-production environment. Production always exists.
pub async fn delete_environment(pool: &SqlitePool, app_id: &str, slug: &str) -> Result<()> {
    let environment = get_environment(pool, app_id, slug).await?;
    if environment.is_production {
        return Err(DekuError::InvalidInput(
            "the production environment cannot be removed".to_string(),
        ));
    }

    sqlx::query("DELETE FROM environments WHERE id = ?1")
        .bind(&environment.id)
        .execute(pool)
        .await?;
    Ok(())
}

// ── Domains ──────────────────────────────────────────────────────────────────

pub async fn list_domains(pool: &SqlitePool, app_id: &str) -> Result<Vec<Domain>> {
    let domains = sqlx::query_as!(
        Domain,
        r#"SELECT
            id         as "id!",
            app_id     as "app_id!",
            domain     as "domain!",
            created_at as "created_at!: _"
           FROM domains WHERE app_id = ?1 ORDER BY domain"#,
        app_id
    )
    .fetch_all(pool)
    .await?;
    Ok(domains)
}

pub async fn add_domain(pool: &SqlitePool, app_id: &str, domain: &str) -> Result<Domain> {
    let row = Domain {
        id: Uuid::new_v4().to_string(),
        app_id: app_id.to_string(),
        domain: domain.to_string(),
        created_at: Utc::now(),
    };
    sqlx::query!(
        r#"INSERT INTO domains (id, app_id, domain, created_at)
           VALUES (?1, ?2, ?3, ?4)"#,
        row.id,
        row.app_id,
        row.domain,
        row.created_at,
    )
    .execute(pool)
    .await?;
    Ok(row)
}

pub async fn remove_domain(pool: &SqlitePool, app_id: &str, domain: &str) -> Result<()> {
    let result = sqlx::query!(
        "DELETE FROM domains WHERE app_id = ?1 AND domain = ?2",
        app_id,
        domain
    )
    .execute(pool)
    .await?;

    if result.rows_affected() == 0 {
        return Err(DekuError::Internal(format!(
            "domain '{domain}' not found for app"
        )));
    }
    Ok(())
}

pub async fn list_domain_names(pool: &SqlitePool, app_id: &str) -> Result<Vec<String>> {
    let rows = list_domains(pool, app_id).await?;
    Ok(rows.into_iter().map(|d| d.domain).collect())
}

// ── Port mappings ─────────────────────────────────────────────────────────────

pub async fn list_port_mappings(pool: &SqlitePool, app_id: &str) -> Result<Vec<PortMapping>> {
    let ports = sqlx::query_as!(
        PortMapping,
        r#"SELECT
            id             as "id!",
            app_id         as "app_id!",
            host_port      as "host_port!",
            container_port as "container_port!",
            protocol       as "protocol!"
           FROM port_mappings WHERE app_id = ?1 ORDER BY host_port"#,
        app_id
    )
    .fetch_all(pool)
    .await?;
    Ok(ports)
}

pub async fn upsert_port_mapping(
    pool: &SqlitePool,
    app_id: &str,
    host_port: i64,
    container_port: i64,
    protocol: &str,
) -> Result<PortMapping> {
    // Remove existing mapping for this container port to avoid conflicts
    sqlx::query!(
        "DELETE FROM port_mappings WHERE app_id = ?1 AND container_port = ?2 AND protocol = ?3",
        app_id,
        container_port,
        protocol
    )
    .execute(pool)
    .await?;

    add_port_mapping(pool, app_id, host_port, container_port, protocol).await
}

pub async fn add_port_mapping(
    pool: &SqlitePool,
    app_id: &str,
    host_port: i64,
    container_port: i64,
    protocol: &str,
) -> Result<PortMapping> {
    let row = PortMapping {
        id: Uuid::new_v4().to_string(),
        app_id: app_id.to_string(),
        host_port,
        container_port,
        protocol: protocol.to_string(),
    };
    sqlx::query!(
        r#"INSERT INTO port_mappings (id, app_id, host_port, container_port, protocol)
           VALUES (?1, ?2, ?3, ?4, ?5)"#,
        row.id,
        row.app_id,
        row.host_port,
        row.container_port,
        row.protocol,
    )
    .execute(pool)
    .await?;
    Ok(row)
}

pub async fn remove_port_mapping(pool: &SqlitePool, app_id: &str, id: &str) -> Result<()> {
    let result = sqlx::query!(
        "DELETE FROM port_mappings WHERE app_id = ?1 AND id = ?2",
        app_id,
        id
    )
    .execute(pool)
    .await?;

    if result.rows_affected() == 0 {
        return Err(DekuError::Internal(format!(
            "port mapping '{id}' not found"
        )));
    }
    Ok(())
}

// ── Storage mounts ────────────────────────────────────────────────────────────

pub async fn list_storage_mounts(pool: &SqlitePool, app_id: &str) -> Result<Vec<StorageMount>> {
    let mounts = sqlx::query_as!(
        StorageMount,
        r#"SELECT
            id             as "id!",
            app_id         as "app_id!",
            host_path      as "host_path!",
            container_path as "container_path!"
           FROM storage_mounts WHERE app_id = ?1"#,
        app_id
    )
    .fetch_all(pool)
    .await?;
    Ok(mounts)
}

// ── Resource limits ───────────────────────────────────────────────────────────

pub async fn get_resource_limits(
    pool: &SqlitePool,
    app_id: &str,
    process_type: &str,
) -> Result<Option<ResourceLimit>> {
    // Prefer process-specific limits; fall back to _all_
    let specific = sqlx::query_as!(
        ResourceLimit,
        r#"SELECT
            app_id       as "app_id!",
            process_type as "process_type!",
            cpu          as "cpu",
            memory       as "memory",
            memory_swap  as "memory_swap",
            network      as "network"
           FROM resource_limits
           WHERE app_id = ?1 AND process_type = ?2"#,
        app_id,
        process_type
    )
    .fetch_optional(pool)
    .await?;

    if specific.is_some() {
        return Ok(specific);
    }

    let fallback = sqlx::query_as!(
        ResourceLimit,
        r#"SELECT
            app_id       as "app_id!",
            process_type as "process_type!",
            cpu          as "cpu",
            memory       as "memory",
            memory_swap  as "memory_swap",
            network      as "network"
           FROM resource_limits
           WHERE app_id = ?1 AND process_type = '_all_'"#,
        app_id
    )
    .fetch_optional(pool)
    .await?;

    Ok(fallback)
}

// ── Containers ────────────────────────────────────────────────────────────────

pub async fn record_container(
    pool: &SqlitePool,
    container_id: &str,
    app_id: &str,
    deployment_id: &str,
    process_type: &str,
    host_port: Option<i64>,
) -> Result<ContainerRecord> {
    let now = Utc::now();
    sqlx::query!(
        r#"INSERT INTO containers (id, app_id, deployment_id, process_type, status, host_port, created_at)
           VALUES (?1, ?2, ?3, ?4, 'running', ?5, ?6)"#,
        container_id,
        app_id,
        deployment_id,
        process_type,
        host_port,
        now,
    )
    .execute(pool)
    .await?;

    Ok(ContainerRecord {
        id: container_id.to_string(),
        app_id: app_id.to_string(),
        deployment_id: deployment_id.to_string(),
        process_type: process_type.to_string(),
        status: "running".to_string(),
        host_port,
        created_at: now,
    })
}

pub async fn update_container_status(
    pool: &SqlitePool,
    container_id: &str,
    status: &str,
) -> Result<()> {
    sqlx::query!(
        "UPDATE containers SET status = ?1 WHERE id = ?2",
        status,
        container_id
    )
    .execute(pool)
    .await?;
    Ok(())
}

pub async fn list_containers_for_app(
    pool: &SqlitePool,
    app_id: &str,
) -> Result<Vec<ContainerRecord>> {
    let containers = sqlx::query_as!(
        ContainerRecord,
        r#"SELECT
            id            as "id!",
            app_id        as "app_id!",
            deployment_id as "deployment_id!",
            process_type  as "process_type!",
            status        as "status!",
            host_port     as "host_port",
            created_at    as "created_at!: _"
           FROM containers
           WHERE app_id = ?1 AND status = 'running'
           ORDER BY created_at DESC"#,
        app_id
    )
    .fetch_all(pool)
    .await?;
    Ok(containers)
}

/// Host ports of the app's serving web containers, oldest first.
///
/// This is the source of truth for upstreams: a deploy and a later reconcile both
/// derive the vhost's server list from it, so every web replica receives traffic
/// and the two paths cannot disagree.
///
/// Only the current deployment's containers count. After a rollout the previous
/// containers stay `running` for the retire window, and pooling them would send
/// traffic back to the version that was just replaced. When no deployment is
/// marked live yet (a crash mid-rollout, or the very first deploy) the newest
/// deployment's containers are used instead of serving nothing.
pub async fn list_web_upstream_ports(pool: &SqlitePool, app_id: &str) -> Result<Vec<u16>> {
    let rows = sqlx::query_as::<_, (i64,)>(
        "SELECT c.host_port FROM containers c \
         WHERE c.app_id = ?1 AND c.status = 'running' AND c.process_type = 'web' \
           AND c.host_port IS NOT NULL \
           AND c.deployment_id = COALESCE( \
                 (SELECT d.id FROM deployments d \
                  WHERE d.app_id = ?1 AND d.status = 'live' \
                  ORDER BY d.created_at DESC, d.rowid DESC LIMIT 1), \
                 (SELECT d.id FROM deployments d \
                  WHERE d.app_id = ?1 \
                  ORDER BY d.created_at DESC, d.rowid DESC LIMIT 1) \
               ) \
         ORDER BY c.created_at ASC",
    )
    .bind(app_id)
    .fetch_all(pool)
    .await?;
    Ok(rows.into_iter().map(|(port,)| port as u16).collect())
}

// ── Process scale ─────────────────────────────────────────────────────────────

pub async fn get_process_scales(
    pool: &SqlitePool,
    app_id: &str,
) -> Result<std::collections::HashMap<String, i64>> {
    let rows = sqlx::query!(
        r#"SELECT process_type as "process_type!", count as "count!"
           FROM process_scale WHERE app_id = ?1"#,
        app_id
    )
    .fetch_all(pool)
    .await?;

    Ok(rows
        .into_iter()
        .map(|r| (r.process_type, r.count))
        .collect())
}

/// Alias for `get_app` — look up an app by name.
pub async fn get_app_by_name(pool: &SqlitePool, name: &str) -> Result<App> {
    get_app(pool, name).await
}

// ── SSH keys ──────────────────────────────────────────────────────────────────

pub struct SshKey {
    pub id: String,
    pub name: String,
    pub fingerprint: String,
}

pub async fn add_ssh_key(
    pool: &SqlitePool,
    name: &str,
    public_key: &str,
    fingerprint: &str,
) -> Result<SshKey> {
    let id = Uuid::new_v4().to_string();
    sqlx::query!(
        r#"INSERT INTO ssh_keys (id, name, public_key, fingerprint) VALUES (?1, ?2, ?3, ?4)"#,
        id,
        name,
        public_key,
        fingerprint,
    )
    .execute(pool)
    .await
    .map_err(DekuError::Database)?;

    Ok(SshKey {
        id,
        name: name.to_string(),
        fingerprint: fingerprint.to_string(),
    })
}

pub async fn list_ssh_keys(pool: &SqlitePool) -> Result<Vec<SshKey>> {
    let rows =
        sqlx::query!(r#"SELECT id, name, public_key, fingerprint FROM ssh_keys ORDER BY name"#)
            .fetch_all(pool)
            .await
            .map_err(DekuError::Database)?;

    Ok(rows
        .into_iter()
        .map(|r| SshKey {
            id: r.id.unwrap_or_default(),
            name: r.name,
            fingerprint: r.fingerprint,
        })
        .collect())
}

pub async fn find_ssh_key_by_fingerprint(
    pool: &SqlitePool,
    fingerprint: &str,
) -> Result<Option<SshKey>> {
    let row = sqlx::query!(
        r#"SELECT id, name, public_key, fingerprint FROM ssh_keys WHERE fingerprint = ?1"#,
        fingerprint,
    )
    .fetch_optional(pool)
    .await
    .map_err(DekuError::Database)?;

    Ok(row.map(|r| SshKey {
        id: r.id.unwrap_or_default(),
        name: r.name,
        fingerprint: r.fingerprint,
    }))
}

pub async fn remove_ssh_key(pool: &SqlitePool, name: &str) -> Result<()> {
    sqlx::query!(r#"DELETE FROM ssh_keys WHERE name = ?1"#, name)
        .execute(pool)
        .await
        .map_err(DekuError::Database)?;
    Ok(())
}

pub async fn set_process_scale(
    pool: &SqlitePool,
    app_id: &str,
    process_type: &str,
    count: i64,
) -> Result<()> {
    sqlx::query!(
        r#"INSERT INTO process_scale (app_id, process_type, count)
           VALUES (?1, ?2, ?3)
           ON CONFLICT(app_id, process_type) DO UPDATE SET count = excluded.count"#,
        app_id,
        process_type,
        count
    )
    .execute(pool)
    .await?;
    Ok(())
}

// ── Storage mounts (write) ────────────────────────────────────────────────────

pub async fn add_storage_mount(
    pool: &SqlitePool,
    app_id: &str,
    host_path: &str,
    container_path: &str,
) -> Result<StorageMount> {
    let row = StorageMount {
        id: Uuid::new_v4().to_string(),
        app_id: app_id.to_string(),
        host_path: host_path.to_string(),
        container_path: container_path.to_string(),
    };
    sqlx::query!(
        r#"INSERT INTO storage_mounts (id, app_id, host_path, container_path)
           VALUES (?1, ?2, ?3, ?4)"#,
        row.id,
        row.app_id,
        row.host_path,
        row.container_path,
    )
    .execute(pool)
    .await?;
    Ok(row)
}

pub async fn remove_storage_mount(pool: &SqlitePool, app_id: &str, mount_id: &str) -> Result<()> {
    let result = sqlx::query!(
        "DELETE FROM storage_mounts WHERE app_id = ?1 AND id = ?2",
        app_id,
        mount_id
    )
    .execute(pool)
    .await?;
    if result.rows_affected() == 0 {
        return Err(DekuError::Internal(format!(
            "storage mount '{mount_id}' not found"
        )));
    }
    Ok(())
}

// ── Services ──────────────────────────────────────────────────────────────────

pub struct Service {
    pub id: String,
    pub name: String,
    pub plugin: String,
    pub container_id: Option<String>,
    pub status: String,
    pub config: String,
    pub created_at: chrono::DateTime<chrono::Utc>,
}

pub struct ServiceLink {
    pub service_id: String,
    pub app_id: String,
    pub env_key: String,
}

pub struct Network {
    pub id: String,
    pub name: String,
}

pub struct CronEntry {
    pub id: String,
    pub schedule: String,
    pub command: String,
}

#[derive(sqlx::FromRow)]
pub struct ServiceBackup {
    pub id: String,
    pub service_id: String,
    pub object_key: String,
    pub format: String,
    pub size_bytes: i64,
    pub sha256: String,
    pub created_at: chrono::DateTime<chrono::Utc>,
    pub restored_at: Option<chrono::DateTime<chrono::Utc>>,
    /// How the stored object is protected: `none` or `aes-256-gcm`.
    pub encryption: String,
}

pub async fn create_service(
    pool: &SqlitePool,
    name: &str,
    plugin: &str,
    container_id: Option<&str>,
    config: &str,
) -> Result<Service> {
    let id = Uuid::new_v4().to_string();
    let now = chrono::Utc::now();
    sqlx::query!(
        r#"INSERT INTO services (id, name, plugin, container_id, status, config, created_at)
           VALUES (?1, ?2, ?3, ?4, 'running', ?5, ?6)"#,
        id,
        name,
        plugin,
        container_id,
        config,
        now,
    )
    .execute(pool)
    .await?;
    Ok(Service {
        id,
        name: name.to_string(),
        plugin: plugin.to_string(),
        container_id: container_id.map(str::to_string),
        status: "running".to_string(),
        config: config.to_string(),
        created_at: now,
    })
}

pub async fn get_service(pool: &SqlitePool, name: &str) -> Result<Service> {
    let row = sqlx::query!(
        r#"SELECT id as "id!", name as "name!", plugin as "plugin!",
              container_id, status as "status!", config as "config!",
              created_at as "created_at!: chrono::DateTime<chrono::Utc>"
           FROM services WHERE name = ?1"#,
        name
    )
    .fetch_optional(pool)
    .await?
    .ok_or_else(|| DekuError::Database(sqlx::Error::RowNotFound))?;
    Ok(Service {
        id: row.id,
        name: row.name,
        plugin: row.plugin,
        container_id: row.container_id,
        status: row.status,
        config: row.config,
        created_at: row.created_at,
    })
}

pub async fn get_service_for_plugin(
    pool: &SqlitePool,
    name: &str,
    plugin: &str,
) -> Result<Service> {
    let service = get_service(pool, name).await?;
    if service.plugin != plugin {
        return Err(DekuError::Internal(format!(
            "service '{name}' is a {} service, not {plugin}",
            service.plugin
        )));
    }
    Ok(service)
}

pub async fn get_service_by_id(pool: &SqlitePool, id: &str) -> Result<Service> {
    let row = sqlx::query(
        r#"SELECT id, name, plugin, container_id, status, config, created_at
           FROM services WHERE id = ?1"#,
    )
    .bind(id)
    .fetch_optional(pool)
    .await?
    .ok_or_else(|| DekuError::Database(sqlx::Error::RowNotFound))?;
    Ok(Service {
        id: row.get("id"),
        name: row.get("name"),
        plugin: row.get("plugin"),
        container_id: row.get("container_id"),
        status: row.get("status"),
        config: row.get("config"),
        created_at: row.get("created_at"),
    })
}

pub async fn list_services(pool: &SqlitePool, plugin: &str) -> Result<Vec<Service>> {
    let rows = sqlx::query!(
        r#"SELECT id as "id!", name as "name!", plugin as "plugin!",
              container_id, status as "status!", config as "config!",
              created_at as "created_at!: chrono::DateTime<chrono::Utc>"
           FROM services WHERE plugin = ?1 ORDER BY name"#,
        plugin
    )
    .fetch_all(pool)
    .await?;
    Ok(rows
        .into_iter()
        .map(|r| Service {
            id: r.id,
            name: r.name,
            plugin: r.plugin,
            container_id: r.container_id,
            status: r.status,
            config: r.config,
            created_at: r.created_at,
        })
        .collect())
}

pub async fn delete_service(pool: &SqlitePool, name: &str) -> Result<Vec<ServiceLink>> {
    let svc = get_service(pool, name).await?;
    let links = list_service_links(pool, &svc.id).await?;
    sqlx::query!("DELETE FROM services WHERE id = ?1", svc.id)
        .execute(pool)
        .await?;
    Ok(links)
}

pub async fn link_service(
    pool: &SqlitePool,
    service_id: &str,
    app_id: &str,
    env_key: &str,
) -> Result<ServiceLink> {
    let id = Uuid::new_v4().to_string();
    sqlx::query!(
        r#"INSERT INTO service_links (id, service_id, app_id, env_key)
           VALUES (?1, ?2, ?3, ?4)"#,
        id,
        service_id,
        app_id,
        env_key,
    )
    .execute(pool)
    .await?;
    Ok(ServiceLink {
        service_id: service_id.to_string(),
        app_id: app_id.to_string(),
        env_key: env_key.to_string(),
    })
}

pub async fn get_service_link(
    pool: &SqlitePool,
    service_id: &str,
    app_id: &str,
) -> Result<Option<ServiceLink>> {
    let row = sqlx::query!(
        r#"SELECT id as "id!", service_id as "service_id!", app_id as "app_id!", env_key as "env_key!"
           FROM service_links WHERE service_id = ?1 AND app_id = ?2"#,
        service_id,
        app_id
    )
    .fetch_optional(pool)
    .await?;
    Ok(row.map(|r| ServiceLink {
        service_id: r.service_id,
        app_id: r.app_id,
        env_key: r.env_key,
    }))
}

pub async fn list_service_links(pool: &SqlitePool, service_id: &str) -> Result<Vec<ServiceLink>> {
    let rows = sqlx::query!(
        r#"SELECT id as "id!", service_id as "service_id!", app_id as "app_id!", env_key as "env_key!"
           FROM service_links WHERE service_id = ?1"#,
        service_id
    )
    .fetch_all(pool)
    .await?;
    Ok(rows
        .into_iter()
        .map(|r| ServiceLink {
            service_id: r.service_id,
            app_id: r.app_id,
            env_key: r.env_key,
        })
        .collect())
}

pub async fn list_service_links_for_app(
    pool: &SqlitePool,
    app_id: &str,
) -> Result<Vec<ServiceLink>> {
    let rows = sqlx::query(
        r#"SELECT service_id, app_id, env_key
           FROM service_links WHERE app_id = ?1"#,
    )
    .bind(app_id)
    .fetch_all(pool)
    .await?;
    Ok(rows
        .into_iter()
        .map(|r| ServiceLink {
            service_id: r.get("service_id"),
            app_id: r.get("app_id"),
            env_key: r.get("env_key"),
        })
        .collect())
}

pub async fn unlink_service(pool: &SqlitePool, service_id: &str, app_id: &str) -> Result<()> {
    sqlx::query!(
        "DELETE FROM service_links WHERE service_id = ?1 AND app_id = ?2",
        service_id,
        app_id
    )
    .execute(pool)
    .await?;
    Ok(())
}

pub async fn create_service_backup(
    pool: &SqlitePool,
    service_id: &str,
    object_key: &str,
    format: &str,
    size_bytes: i64,
    sha256: &str,
    encryption: &str,
) -> Result<ServiceBackup> {
    let id = Uuid::new_v4().to_string();
    let now = chrono::Utc::now();
    sqlx::query(
        r#"INSERT INTO service_backups
           (id, service_id, object_key, format, size_bytes, sha256, created_at, encryption)
           VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8)"#,
    )
    .bind(&id)
    .bind(service_id)
    .bind(object_key)
    .bind(format)
    .bind(size_bytes)
    .bind(sha256)
    .bind(now)
    .bind(encryption)
    .execute(pool)
    .await?;

    Ok(ServiceBackup {
        id,
        service_id: service_id.to_string(),
        object_key: object_key.to_string(),
        format: format.to_string(),
        size_bytes,
        sha256: sha256.to_string(),
        created_at: now,
        restored_at: None,
        encryption: encryption.to_string(),
    })
}

pub async fn list_service_backups(
    pool: &SqlitePool,
    service_id: &str,
) -> Result<Vec<ServiceBackup>> {
    let rows = sqlx::query(
        r#"SELECT id, service_id, object_key, format, size_bytes, sha256, created_at, restored_at, encryption
           FROM service_backups
           WHERE service_id = ?1
           ORDER BY created_at DESC"#,
    )
    .bind(service_id)
    .fetch_all(pool)
    .await?;

    rows.into_iter().map(service_backup_from_row).collect()
}

pub async fn get_service_backup_for_service(
    pool: &SqlitePool,
    service_id: &str,
    backup_id: &str,
) -> Result<ServiceBackup> {
    let row = sqlx::query(
        r#"SELECT id, service_id, object_key, format, size_bytes, sha256, created_at, restored_at, encryption
           FROM service_backups
           WHERE service_id = ?1 AND id = ?2"#,
    )
    .bind(service_id)
    .bind(backup_id)
    .fetch_optional(pool)
    .await?;

    row.map(service_backup_from_row)
        .transpose()?
        .ok_or_else(|| DekuError::Database(sqlx::Error::RowNotFound))
}

pub async fn mark_service_backup_restored(pool: &SqlitePool, backup_id: &str) -> Result<()> {
    sqlx::query("UPDATE service_backups SET restored_at = ?1 WHERE id = ?2")
        .bind(chrono::Utc::now())
        .bind(backup_id)
        .execute(pool)
        .await?;
    Ok(())
}

fn service_backup_from_row(row: sqlx::sqlite::SqliteRow) -> Result<ServiceBackup> {
    Ok(ServiceBackup {
        id: row.try_get("id")?,
        service_id: row.try_get("service_id")?,
        object_key: row.try_get("object_key")?,
        format: row.try_get("format")?,
        size_bytes: row.try_get("size_bytes")?,
        sha256: row.try_get("sha256")?,
        created_at: row.try_get("created_at")?,
        restored_at: row.try_get("restored_at")?,
        encryption: row.try_get("encryption")?,
    })
}

/// Upstreams for the app's running web containers, one per replica.
///
/// Every path that writes the proxy config derives its server list from here.
/// `port_mappings` records a single published port for display and CRUD; it is
/// not the serving set, and using it silently drops replicas.
pub async fn list_web_upstreams(
    pool: &SqlitePool,
    app_id: &str,
) -> Result<Vec<deku_core::types::Upstream>> {
    Ok(list_web_upstream_ports(pool, app_id)
        .await?
        .into_iter()
        .map(|port| deku_core::types::Upstream {
            host: "127.0.0.1".to_string(),
            port,
        })
        .collect())
}

/// Total config var values, and how many of them are stored as ciphertext.
pub async fn count_config_var_encryption(pool: &SqlitePool) -> Result<(i64, i64)> {
    let row = sqlx::query_as::<_, (i64, i64)>(
        "SELECT COUNT(*), SUM(CASE WHEN value LIKE 'enc:v1:%' THEN 1 ELSE 0 END) FROM config_vars",
    )
    .fetch_one(pool)
    .await?;
    Ok((row.0, row.1))
}

/// Apps that have at least one deployment but no running web container.
///
/// This is the "deployed but nothing is serving" condition, which is exactly
/// what an operator needs to hear about.
pub async fn list_apps_without_running_web_containers(pool: &SqlitePool) -> Result<Vec<String>> {
    let rows = sqlx::query_scalar::<_, String>(
        "SELECT a.name FROM apps a \
         WHERE EXISTS (SELECT 1 FROM deployments d WHERE d.app_id = a.id) \
           AND NOT EXISTS ( \
                 SELECT 1 FROM containers c \
                 WHERE c.app_id = a.id AND c.status = 'running' AND c.process_type = 'web' \
           ) \
         ORDER BY a.name",
    )
    .fetch_all(pool)
    .await?;
    Ok(rows)
}

// ── Alerts ────────────────────────────────────────────────────────────────────

#[derive(Debug, Clone, serde::Serialize, sqlx::FromRow)]
pub struct Alert {
    pub id: String,
    pub rule: String,
    pub severity: String,
    pub scope: String,
    pub subject: String,
    pub message: String,
    pub first_seen_at: DateTime<Utc>,
    pub last_seen_at: DateTime<Utc>,
    pub resolved_at: Option<DateTime<Utc>>,
}

/// Active alerts, newest first.
pub async fn list_active_alerts(pool: &SqlitePool) -> Result<Vec<Alert>> {
    Ok(sqlx::query_as::<_, Alert>(
        "SELECT id, rule, severity, scope, subject, message, \
                first_seen_at, last_seen_at, resolved_at \
         FROM alerts WHERE resolved_at IS NULL ORDER BY last_seen_at DESC",
    )
    .fetch_all(pool)
    .await?)
}

/// Alerts with history, most recent first.
pub async fn list_alerts(pool: &SqlitePool, limit: i64) -> Result<Vec<Alert>> {
    Ok(sqlx::query_as::<_, Alert>(
        "SELECT id, rule, severity, scope, subject, message, \
                first_seen_at, last_seen_at, resolved_at \
         FROM alerts ORDER BY last_seen_at DESC LIMIT ?1",
    )
    .bind(limit)
    .fetch_all(pool)
    .await?)
}

/// Insert a new active alert, or refresh the one already open for the same rule
/// and scope. Returns the row and whether it was newly created, so callers can
/// notify only on state changes.
pub async fn upsert_alert(
    pool: &SqlitePool,
    rule: &str,
    severity: &str,
    scope: &str,
    subject: &str,
    message: &str,
) -> Result<(Alert, bool)> {
    let now = Utc::now();
    let existing: Option<Alert> = sqlx::query_as::<_, Alert>(
        "SELECT id, rule, severity, scope, subject, message, \
                first_seen_at, last_seen_at, resolved_at \
         FROM alerts WHERE rule = ?1 AND scope = ?2 AND resolved_at IS NULL",
    )
    .bind(rule)
    .bind(scope)
    .fetch_optional(pool)
    .await?;

    if let Some(mut alert) = existing {
        sqlx::query(
            "UPDATE alerts SET severity = ?1, subject = ?2, message = ?3, last_seen_at = ?4 \
             WHERE id = ?5",
        )
        .bind(severity)
        .bind(subject)
        .bind(message)
        .bind(now)
        .bind(&alert.id)
        .execute(pool)
        .await?;
        alert.severity = severity.to_string();
        alert.subject = subject.to_string();
        alert.message = message.to_string();
        alert.last_seen_at = now;
        return Ok((alert, false));
    }

    let id = Uuid::new_v4().to_string();
    sqlx::query(
        "INSERT INTO alerts \
         (id, rule, severity, scope, subject, message, first_seen_at, last_seen_at) \
         VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8)",
    )
    .bind(&id)
    .bind(rule)
    .bind(severity)
    .bind(scope)
    .bind(subject)
    .bind(message)
    .bind(now)
    .bind(now)
    .execute(pool)
    .await?;

    Ok((
        Alert {
            id,
            rule: rule.to_string(),
            severity: severity.to_string(),
            scope: scope.to_string(),
            subject: subject.to_string(),
            message: message.to_string(),
            first_seen_at: now,
            last_seen_at: now,
            resolved_at: None,
        },
        true,
    ))
}

/// Mark one alert resolved. Returns the updated row when it was still open.
pub async fn resolve_alert(pool: &SqlitePool, id: &str) -> Result<Option<Alert>> {
    let now = Utc::now();
    let updated =
        sqlx::query("UPDATE alerts SET resolved_at = ?1 WHERE id = ?2 AND resolved_at IS NULL")
            .bind(now)
            .bind(id)
            .execute(pool)
            .await?;
    if updated.rows_affected() == 0 {
        return Ok(None);
    }

    let alert = sqlx::query_as::<_, Alert>(
        "SELECT id, rule, severity, scope, subject, message, \
                first_seen_at, last_seen_at, resolved_at \
         FROM alerts WHERE id = ?1",
    )
    .bind(id)
    .fetch_optional(pool)
    .await?;
    Ok(alert)
}

/// Counts of active alerts per severity.
pub async fn count_active_alerts_by_severity(pool: &SqlitePool) -> Result<Vec<(String, i64)>> {
    Ok(sqlx::query_as::<_, (String, i64)>(
        "SELECT severity, COUNT(*) FROM alerts WHERE resolved_at IS NULL GROUP BY severity",
    )
    .fetch_all(pool)
    .await?)
}

/// Service plugins with their service counts.
pub async fn count_services_by_plugin(pool: &SqlitePool) -> Result<Vec<(String, i64)>> {
    Ok(
        sqlx::query_as::<_, (String, i64)>("SELECT plugin, COUNT(*) FROM services GROUP BY plugin")
            .fetch_all(pool)
            .await?,
    )
}

/// Backup schedules whose next run is already in the past.
pub async fn count_overdue_backup_schedules(pool: &SqlitePool, now: DateTime<Utc>) -> Result<i64> {
    Ok(sqlx::query_scalar::<_, i64>(
        "SELECT COUNT(*) FROM backup_schedules WHERE next_run_at IS NOT NULL AND next_run_at < ?1",
    )
    .bind(now)
    .fetch_one(pool)
    .await?)
}

/// Backup schedules that have been configured, with the last run outcome.
pub async fn list_backup_schedules_for_alerts(
    pool: &SqlitePool,
) -> Result<Vec<(String, String, Option<String>, Option<DateTime<Utc>>)>> {
    Ok(
        sqlx::query_as::<_, (String, String, Option<String>, Option<DateTime<Utc>>)>(
            "SELECT s.name, s.plugin, b.last_status, b.next_run_at \
         FROM backup_schedules b JOIN services s ON s.id = b.service_id",
        )
        .fetch_all(pool)
        .await?,
    )
}

// ── Networks ──────────────────────────────────────────────────────────────────

pub async fn create_network(pool: &SqlitePool, name: &str) -> Result<Network> {
    let id = Uuid::new_v4().to_string();
    let now = chrono::Utc::now();
    sqlx::query!(
        r#"INSERT INTO networks (id, name, created_at) VALUES (?1, ?2, ?3)"#,
        id,
        name,
        now,
    )
    .execute(pool)
    .await?;
    Ok(Network {
        id,
        name: name.to_string(),
    })
}

pub async fn get_network(pool: &SqlitePool, name: &str) -> Result<Network> {
    let row = sqlx::query!(
        r#"SELECT id as "id!", name as "name!", created_at as "created_at!: chrono::DateTime<chrono::Utc>"
           FROM networks WHERE name = ?1"#,
        name
    )
    .fetch_optional(pool)
    .await?
    .ok_or_else(|| DekuError::Internal(format!("network '{name}' not found")))?;
    Ok(Network {
        id: row.id,
        name: row.name,
    })
}

pub async fn list_networks(pool: &SqlitePool) -> Result<Vec<Network>> {
    let rows = sqlx::query!(
        r#"SELECT id as "id!", name as "name!", created_at as "created_at!: chrono::DateTime<chrono::Utc>"
           FROM networks ORDER BY name"#
    )
    .fetch_all(pool)
    .await?;
    Ok(rows
        .into_iter()
        .map(|r| Network {
            id: r.id,
            name: r.name,
        })
        .collect())
}

pub async fn delete_network(pool: &SqlitePool, name: &str) -> Result<()> {
    let result = sqlx::query!("DELETE FROM networks WHERE name = ?1", name)
        .execute(pool)
        .await?;
    if result.rows_affected() == 0 {
        return Err(DekuError::Internal(format!("network '{name}' not found")));
    }
    Ok(())
}

pub async fn attach_app_to_network(
    pool: &SqlitePool,
    app_id: &str,
    network_id: &str,
    attach_phase: &str,
) -> Result<()> {
    sqlx::query!(
        r#"INSERT OR IGNORE INTO app_networks (app_id, network_id, attach_phase)
           VALUES (?1, ?2, ?3)"#,
        app_id,
        network_id,
        attach_phase,
    )
    .execute(pool)
    .await?;
    Ok(())
}

pub async fn detach_app_from_network(
    pool: &SqlitePool,
    app_id: &str,
    network_id: &str,
) -> Result<()> {
    sqlx::query!(
        "DELETE FROM app_networks WHERE app_id = ?1 AND network_id = ?2",
        app_id,
        network_id
    )
    .execute(pool)
    .await?;
    Ok(())
}

pub async fn list_app_networks(pool: &SqlitePool, app_id: &str) -> Result<Vec<Network>> {
    let rows = sqlx::query!(
        r#"SELECT n.id as "id!", n.name as "name!", n.created_at as "created_at!: chrono::DateTime<chrono::Utc>"
           FROM networks n
           JOIN app_networks an ON an.network_id = n.id
           WHERE an.app_id = ?1
           ORDER BY n.name"#,
        app_id
    )
    .fetch_all(pool)
    .await?;
    Ok(rows
        .into_iter()
        .map(|r| Network {
            id: r.id,
            name: r.name,
        })
        .collect())
}

// ── TLS ───────────────────────────────────────────────────────────────────────

pub async fn set_app_tls(pool: &SqlitePool, app_id: &str, enabled: bool) -> Result<()> {
    let tls = enabled as i64;
    sqlx::query!(
        "UPDATE apps SET tls_enabled = ?1 WHERE id = ?2",
        tls,
        app_id
    )
    .execute(pool)
    .await?;
    Ok(())
}

// ── Cron entries ──────────────────────────────────────────────────────────────

pub async fn list_cron_entries(pool: &SqlitePool, app_id: &str) -> Result<Vec<CronEntry>> {
    let rows = sqlx::query!(
        r#"SELECT id as "id!", app_id as "app_id!", schedule as "schedule!",
              command as "command!", created_at as "created_at!: chrono::DateTime<chrono::Utc>"
           FROM cron_entries WHERE app_id = ?1 ORDER BY created_at"#,
        app_id
    )
    .fetch_all(pool)
    .await?;
    Ok(rows
        .into_iter()
        .map(|r| CronEntry {
            id: r.id,
            schedule: r.schedule,
            command: r.command,
        })
        .collect())
}

pub async fn add_cron_entry(
    pool: &SqlitePool,
    app_id: &str,
    schedule: &str,
    command: &str,
) -> Result<CronEntry> {
    let id = Uuid::new_v4().to_string();
    let now = chrono::Utc::now();
    sqlx::query!(
        r#"INSERT INTO cron_entries (id, app_id, schedule, command, created_at)
           VALUES (?1, ?2, ?3, ?4, ?5)"#,
        id,
        app_id,
        schedule,
        command,
        now,
    )
    .execute(pool)
    .await?;
    Ok(CronEntry {
        id,
        schedule: schedule.to_string(),
        command: command.to_string(),
    })
}

pub async fn remove_cron_entry(pool: &SqlitePool, app_id: &str, entry_id: &str) -> Result<()> {
    let result = sqlx::query!(
        "DELETE FROM cron_entries WHERE app_id = ?1 AND id = ?2",
        app_id,
        entry_id
    )
    .execute(pool)
    .await?;
    if result.rows_affected() == 0 {
        return Err(DekuError::Internal(format!(
            "cron entry '{entry_id}' not found"
        )));
    }
    Ok(())
}

// ── App auth ──────────────────────────────────────────────────────────────────

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize, sqlx::FromRow)]
pub struct AppAuthRecord {
    pub app_id: String,
    pub mode: String,
    pub username: Option<String>,
    pub password_hash: Option<String>,
    pub forward_url: Option<String>,
    pub created_at: String,
    pub updated_at: String,
}

pub async fn get_app_auth(pool: &SqlitePool, app_id: &str) -> Result<Option<AppAuthRecord>> {
    let record = sqlx::query_as::<_, AppAuthRecord>(
        "SELECT app_id, mode, username, password_hash, forward_url, created_at, updated_at \
         FROM app_auth WHERE app_id = ?1",
    )
    .bind(app_id)
    .fetch_optional(pool)
    .await?;
    Ok(record)
}

pub async fn upsert_app_auth_basic(
    pool: &SqlitePool,
    app_id: &str,
    username: &str,
    password_hash: &str,
) -> Result<()> {
    let now = Utc::now().to_rfc3339();
    sqlx::query(
        "INSERT INTO app_auth (app_id, mode, username, password_hash, forward_url, created_at, updated_at) \
         VALUES (?1, 'basic', ?2, ?3, NULL, ?4, ?4) \
         ON CONFLICT(app_id) DO UPDATE SET mode = 'basic', username = ?2, password_hash = ?3, \
         forward_url = NULL, updated_at = ?4",
    )
    .bind(app_id)
    .bind(username)
    .bind(password_hash)
    .bind(&now)
    .execute(pool)
    .await?;
    Ok(())
}

pub async fn upsert_app_auth_forward(
    pool: &SqlitePool,
    app_id: &str,
    forward_url: &str,
) -> Result<()> {
    let now = Utc::now().to_rfc3339();
    sqlx::query(
        "INSERT INTO app_auth (app_id, mode, username, password_hash, forward_url, created_at, updated_at) \
         VALUES (?1, 'forward', NULL, NULL, ?2, ?3, ?3) \
         ON CONFLICT(app_id) DO UPDATE SET mode = 'forward', username = NULL, password_hash = NULL, \
         forward_url = ?2, updated_at = ?3",
    )
    .bind(app_id)
    .bind(forward_url)
    .bind(&now)
    .execute(pool)
    .await?;
    Ok(())
}

/// A per-app deploy token. The hash is deliberately not serialized: callers
/// that expose tokens build their own JSON from the display fields.
#[derive(Debug, Clone, sqlx::FromRow)]
pub struct DeployToken {
    pub id: String,
    pub app_id: String,
    pub name: String,
    pub token_prefix: String,
    pub created_at: DateTime<Utc>,
    pub last_used_at: Option<DateTime<Utc>>,
}

pub async fn create_deploy_token(
    pool: &SqlitePool,
    app_id: &str,
    name: &str,
    token_hash: &str,
    token_prefix: &str,
) -> Result<DeployToken> {
    let id = uuid::Uuid::new_v4().to_string();
    sqlx::query(
        "INSERT INTO app_deploy_tokens (id, app_id, name, token_hash, token_prefix) \
         VALUES (?1, ?2, ?3, ?4, ?5)",
    )
    .bind(&id)
    .bind(app_id)
    .bind(name)
    .bind(token_hash)
    .bind(token_prefix)
    .execute(pool)
    .await?;

    get_deploy_token(pool, &id)
        .await?
        .ok_or_else(|| DekuError::Internal("deploy token insert vanished".into()))
}

pub async fn get_deploy_token(pool: &SqlitePool, id: &str) -> Result<Option<DeployToken>> {
    let token = sqlx::query_as::<_, DeployToken>(
        "SELECT id, app_id, name, token_prefix, created_at, last_used_at \
         FROM app_deploy_tokens WHERE id = ?1",
    )
    .bind(id)
    .fetch_optional(pool)
    .await?;
    Ok(token)
}

/// Look up a presented token by digest so the plaintext is never stored.
pub async fn get_deploy_token_by_hash(
    pool: &SqlitePool,
    token_hash: &str,
) -> Result<Option<DeployToken>> {
    let token = sqlx::query_as::<_, DeployToken>(
        "SELECT id, app_id, name, token_prefix, created_at, last_used_at \
         FROM app_deploy_tokens WHERE token_hash = ?1",
    )
    .bind(token_hash)
    .fetch_optional(pool)
    .await?;
    Ok(token)
}

pub async fn list_deploy_tokens(pool: &SqlitePool, app_id: &str) -> Result<Vec<DeployToken>> {
    let tokens = sqlx::query_as::<_, DeployToken>(
        "SELECT id, app_id, name, token_prefix, created_at, last_used_at \
         FROM app_deploy_tokens WHERE app_id = ?1 ORDER BY created_at DESC",
    )
    .bind(app_id)
    .fetch_all(pool)
    .await?;
    Ok(tokens)
}

pub async fn delete_deploy_token(pool: &SqlitePool, app_id: &str, id: &str) -> Result<bool> {
    let result = sqlx::query("DELETE FROM app_deploy_tokens WHERE app_id = ?1 AND id = ?2")
        .bind(app_id)
        .bind(id)
        .execute(pool)
        .await?;
    Ok(result.rows_affected() > 0)
}

pub async fn touch_deploy_token(pool: &SqlitePool, id: &str) -> Result<()> {
    sqlx::query("UPDATE app_deploy_tokens SET last_used_at = CURRENT_TIMESTAMP WHERE id = ?1")
        .bind(id)
        .execute(pool)
        .await?;
    Ok(())
}

pub async fn delete_app_auth(pool: &SqlitePool, app_id: &str) -> Result<()> {
    sqlx::query("DELETE FROM app_auth WHERE app_id = ?1")
        .bind(app_id)
        .execute(pool)
        .await?;
    Ok(())
}

pub async fn list_resource_limits(pool: &SqlitePool, app_id: &str) -> Result<Vec<ResourceLimit>> {
    let limits = sqlx::query_as::<_, ResourceLimit>(
        "SELECT app_id, process_type, cpu, memory, memory_swap, network \
         FROM resource_limits WHERE app_id = ?1 ORDER BY process_type",
    )
    .bind(app_id)
    .fetch_all(pool)
    .await?;
    Ok(limits)
}

pub async fn set_resource_limit(
    pool: &SqlitePool,
    app_id: &str,
    process_type: &str,
    cpu: Option<&str>,
    memory: Option<&str>,
) -> Result<()> {
    sqlx::query(
        "INSERT INTO resource_limits (app_id, process_type, cpu, memory) VALUES (?1, ?2, ?3, ?4) \
         ON CONFLICT(app_id, process_type) DO UPDATE SET cpu = ?3, memory = ?4",
    )
    .bind(app_id)
    .bind(process_type)
    .bind(cpu)
    .bind(memory)
    .execute(pool)
    .await?;
    Ok(())
}

// ── Backup schedules ──────────────────────────────────────────────────────────

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize, sqlx::FromRow)]
pub struct BackupSchedule {
    pub service_id: String,
    pub interval_hours: i64,
    pub retention: i64,
    pub enabled: i64,
    pub last_run_at: Option<String>,
    pub last_status: Option<String>,
    pub next_run_at: Option<String>,
    pub created_at: String,
    pub updated_at: String,
}

pub async fn get_backup_schedule(
    pool: &SqlitePool,
    service_id: &str,
) -> Result<Option<BackupSchedule>> {
    let schedule = sqlx::query_as::<_, BackupSchedule>(
        "SELECT service_id, interval_hours, retention, enabled, last_run_at, last_status, \
         next_run_at, created_at, updated_at FROM backup_schedules WHERE service_id = ?1",
    )
    .bind(service_id)
    .fetch_optional(pool)
    .await?;
    Ok(schedule)
}

pub async fn list_backup_schedules(pool: &SqlitePool) -> Result<Vec<BackupSchedule>> {
    let schedules = sqlx::query_as::<_, BackupSchedule>(
        "SELECT service_id, interval_hours, retention, enabled, last_run_at, last_status, \
         next_run_at, created_at, updated_at FROM backup_schedules ORDER BY service_id",
    )
    .fetch_all(pool)
    .await?;
    Ok(schedules)
}

pub async fn list_due_backup_schedules(
    pool: &SqlitePool,
    now: DateTime<Utc>,
) -> Result<Vec<BackupSchedule>> {
    let schedules = sqlx::query_as::<_, BackupSchedule>(
        "SELECT service_id, interval_hours, retention, enabled, last_run_at, last_status, \
         next_run_at, created_at, updated_at FROM backup_schedules \
         WHERE enabled = 1 AND (next_run_at IS NULL OR next_run_at <= ?1) ORDER BY service_id",
    )
    .bind(now.to_rfc3339())
    .fetch_all(pool)
    .await?;
    Ok(schedules)
}

pub async fn upsert_backup_schedule(
    pool: &SqlitePool,
    service_id: &str,
    interval_hours: i64,
    retention: i64,
) -> Result<()> {
    let now = Utc::now();
    let next = now + chrono::Duration::hours(interval_hours);
    sqlx::query(
        "INSERT INTO backup_schedules (service_id, interval_hours, retention, enabled, next_run_at, created_at, updated_at) \
         VALUES (?1, ?2, ?3, 1, ?4, ?5, ?5) \
         ON CONFLICT(service_id) DO UPDATE SET interval_hours = ?2, retention = ?3, enabled = 1, \
         next_run_at = ?4, updated_at = ?5",
    )
    .bind(service_id)
    .bind(interval_hours)
    .bind(retention)
    .bind(next.to_rfc3339())
    .bind(now.to_rfc3339())
    .execute(pool)
    .await?;
    Ok(())
}

pub async fn mark_backup_schedule_run(
    pool: &SqlitePool,
    service_id: &str,
    status: &str,
    next_run_at: DateTime<Utc>,
) -> Result<()> {
    let now = Utc::now();
    sqlx::query(
        "UPDATE backup_schedules SET last_run_at = ?2, last_status = ?3, next_run_at = ?4, updated_at = ?2 \
         WHERE service_id = ?1",
    )
    .bind(service_id)
    .bind(now.to_rfc3339())
    .bind(status)
    .bind(next_run_at.to_rfc3339())
    .execute(pool)
    .await?;
    Ok(())
}

pub async fn delete_backup_schedule(pool: &SqlitePool, service_id: &str) -> Result<()> {
    sqlx::query("DELETE FROM backup_schedules WHERE service_id = ?1")
        .bind(service_id)
        .execute(pool)
        .await?;
    Ok(())
}

/// Backups beyond the newest `keep` for a service, oldest first.
pub async fn backups_beyond_retention(
    pool: &SqlitePool,
    service_id: &str,
    keep: i64,
) -> Result<Vec<ServiceBackup>> {
    let backups = sqlx::query_as::<_, ServiceBackup>(
        "SELECT id, service_id, object_key, format, size_bytes, sha256, created_at, restored_at, encryption \
         FROM service_backups WHERE service_id = ?1 ORDER BY created_at DESC LIMIT -1 OFFSET ?2",
    )
    .bind(service_id)
    .bind(keep)
    .fetch_all(pool)
    .await?;
    Ok(backups)
}

pub async fn delete_service_backup(pool: &SqlitePool, id: &str) -> Result<()> {
    sqlx::query("DELETE FROM service_backups WHERE id = ?1")
        .bind(id)
        .execute(pool)
        .await?;
    Ok(())
}

#[cfg(test)]
mod alert_tests {
    use super::{list_active_alerts, resolve_alert, upsert_alert};
    use sqlx::sqlite::SqlitePoolOptions;
    use sqlx::SqlitePool;

    async fn test_pool() -> SqlitePool {
        let pool = SqlitePoolOptions::new()
            .max_connections(1)
            .connect("sqlite::memory:")
            .await
            .expect("pool");
        crate::db::migrate(&pool).await.expect("migrate");
        pool
    }

    #[tokio::test]
    async fn creating_an_alert_reports_it_as_new() {
        let pool = test_pool().await;
        let (alert, is_new) = upsert_alert(
            &pool,
            "certificate_expiring",
            "warning",
            "app:web",
            "web",
            "expires in 3 days",
        )
        .await
        .expect("upsert");
        assert!(is_new);
        assert_eq!(alert.severity, "warning");
        assert!(alert.resolved_at.is_none());
    }

    #[tokio::test]
    async fn the_same_rule_and_scope_refreshes_instead_of_duplicating() {
        let pool = test_pool().await;
        upsert_alert(&pool, "disk_usage_high", "warning", "host", "h", "87% used")
            .await
            .expect("first");
        let (alert, is_new) = upsert_alert(
            &pool,
            "disk_usage_high",
            "critical",
            "host",
            "h",
            "96% used",
        )
        .await
        .expect("second");
        assert!(!is_new, "an open alert must be refreshed, not duplicated");
        assert_eq!(alert.severity, "critical");
        assert_eq!(alert.message, "96% used");

        let active = list_active_alerts(&pool).await.expect("list");
        assert_eq!(active.len(), 1);
    }

    #[tokio::test]
    async fn a_resolved_alert_can_reopen_as_a_new_row() {
        let pool = test_pool().await;
        let (first, _) = upsert_alert(
            &pool,
            "backup_failed",
            "warning",
            "service:pg",
            "pg",
            "boom",
        )
        .await
        .expect("first");
        assert!(resolve_alert(&pool, &first.id)
            .await
            .expect("resolve")
            .is_some());
        assert!(list_active_alerts(&pool).await.expect("list").is_empty());

        let (second, is_new) = upsert_alert(
            &pool,
            "backup_failed",
            "warning",
            "service:pg",
            "pg",
            "boom again",
        )
        .await
        .expect("reopen");
        assert!(is_new);
        assert_ne!(first.id, second.id);
    }

    #[tokio::test]
    async fn resolving_twice_is_a_no_op() {
        let pool = test_pool().await;
        let (alert, _) = upsert_alert(&pool, "r", "warning", "host", "h", "m")
            .await
            .expect("upsert");
        assert!(resolve_alert(&pool, &alert.id)
            .await
            .expect("first")
            .is_some());
        assert!(resolve_alert(&pool, &alert.id)
            .await
            .expect("second")
            .is_none());
    }
}

#[cfg(test)]
mod service_backup_tests {
    use super::{create_service_backup, list_service_backups};
    use sqlx::sqlite::SqlitePoolOptions;
    use sqlx::SqlitePool;

    async fn pool_with_service() -> SqlitePool {
        let pool = SqlitePoolOptions::new()
            .max_connections(1)
            .connect("sqlite::memory:")
            .await
            .expect("pool");
        crate::db::migrate(&pool).await.expect("migrate");
        sqlx::query(
            "INSERT INTO services (id, name, plugin, status, created_at) \
             VALUES ('svc-1', 'pg', 'postgres', 'running', CURRENT_TIMESTAMP)",
        )
        .execute(&pool)
        .await
        .expect("insert service");
        pool
    }

    #[tokio::test]
    async fn records_and_lists_the_encryption_label() {
        let pool = pool_with_service().await;
        create_service_backup(
            &pool,
            "svc-1",
            "k/one.sql",
            "postgres.sql",
            10,
            "aa",
            "aes-256-gcm",
        )
        .await
        .expect("create");
        create_service_backup(
            &pool,
            "svc-1",
            "k/two.sql",
            "postgres.sql",
            20,
            "bb",
            "none",
        )
        .await
        .expect("create");

        let backups = list_service_backups(&pool, "svc-1").await.expect("list");
        let mut labels: Vec<&str> = backups.iter().map(|b| b.encryption.as_str()).collect();
        labels.sort_unstable();
        assert_eq!(labels, vec!["aes-256-gcm", "none"]);
    }

    #[tokio::test]
    async fn existing_rows_default_to_unencrypted() {
        let pool = pool_with_service().await;
        // Simulates a row written before the encryption column existed.
        sqlx::query(
            "INSERT INTO service_backups (id, service_id, object_key, format, size_bytes, sha256, created_at) \
             VALUES ('b-1', 'svc-1', 'k/old.sql', 'postgres.sql', 5, 'cc', CURRENT_TIMESTAMP)",
        )
        .execute(&pool)
        .await
        .expect("insert legacy row");

        let backups = list_service_backups(&pool, "svc-1").await.expect("list");
        assert_eq!(backups.len(), 1);
        assert_eq!(backups[0].encryption, "none");
    }
}

#[cfg(test)]
mod environment_tests {
    use super::{create_app, delete_environment, ensure_production_environment, list_environments};
    use deku_core::types::NewApp;
    use sqlx::sqlite::SqlitePoolOptions;
    use sqlx::SqlitePool;

    async fn pool_with_app(name: &str) -> (SqlitePool, String) {
        let pool = SqlitePoolOptions::new()
            .max_connections(1)
            .connect("sqlite::memory:")
            .await
            .expect("pool");
        crate::db::migrate(&pool).await.expect("migrate");
        let app = create_app(
            &pool,
            &NewApp {
                name: name.to_string(),
            },
        )
        .await
        .expect("app");
        (pool, app.id)
    }

    #[tokio::test]
    async fn every_app_starts_with_a_production_environment() {
        let (pool, app_id) = pool_with_app("one").await;
        let environments = list_environments(&pool, &app_id).await.expect("list");
        assert_eq!(environments.len(), 1);
        assert_eq!(environments[0].slug, "production");
        assert!(environments[0].is_production);
    }

    #[tokio::test]
    async fn production_environment_creation_is_idempotent() {
        let (pool, app_id) = pool_with_app("two").await;
        let first = ensure_production_environment(&pool, &app_id)
            .await
            .expect("first");
        let second = ensure_production_environment(&pool, &app_id)
            .await
            .expect("second");
        assert_eq!(first.id, second.id);
        assert_eq!(
            list_environments(&pool, &app_id).await.expect("list").len(),
            1
        );
    }

    #[tokio::test]
    async fn production_cannot_be_deleted() {
        let (pool, app_id) = pool_with_app("three").await;
        let error = delete_environment(&pool, &app_id, "production")
            .await
            .expect_err("production must be protected");
        assert!(error.to_string().contains("cannot be removed"));
    }
}

#[cfg(test)]
mod upstream_port_tests {
    use super::{list_web_upstream_ports, list_web_upstreams};
    use sqlx::sqlite::SqlitePoolOptions;
    use sqlx::SqlitePool;

    async fn test_pool() -> SqlitePool {
        let pool = SqlitePoolOptions::new()
            .max_connections(1)
            .connect("sqlite::memory:")
            .await
            .expect("pool");
        crate::db::migrate(&pool).await.expect("migrate");
        pool
    }

    async fn insert_app(pool: &SqlitePool, id: &str, name: &str, deployment_id: &str) {
        sqlx::query(
            "INSERT INTO apps (id, name, status, created_at) VALUES (?1, ?2, 'created', ?3)",
        )
        .bind(id)
        .bind(name)
        .bind(chrono::Utc::now().to_rfc3339())
        .execute(pool)
        .await
        .expect("insert app");

        sqlx::query(
            "INSERT INTO deployments (id, app_id, status, builder, image_tag, created_at) \
             VALUES (?1, ?2, 'live', 'dockerfile', 'deku/x:1', ?3)",
        )
        .bind(deployment_id)
        .bind(id)
        .bind(chrono::Utc::now().to_rfc3339())
        .execute(pool)
        .await
        .expect("insert deployment");
    }

    async fn insert_container_in(
        pool: &SqlitePool,
        id: &str,
        app_id: &str,
        deployment_id: &str,
        host_port: Option<i64>,
    ) {
        sqlx::query(
            "INSERT INTO containers (id, app_id, deployment_id, process_type, status, host_port, created_at) \
             VALUES (?1, ?2, ?3, 'web', 'running', ?4, CURRENT_TIMESTAMP)",
        )
        .bind(id)
        .bind(app_id)
        .bind(deployment_id)
        .bind(host_port)
        .execute(pool)
        .await
        .expect("insert container");
    }

    async fn insert_container(
        pool: &SqlitePool,
        id: &str,
        app_id: &str,
        process_type: &str,
        status: &str,
        host_port: Option<i64>,
    ) {
        sqlx::query(
            "INSERT INTO containers (id, app_id, deployment_id, process_type, status, host_port, created_at) \
             VALUES (?1, ?2, 'dep-1', ?3, ?4, ?5, CURRENT_TIMESTAMP)",
        )
        .bind(id)
        .bind(app_id)
        .bind(process_type)
        .bind(status)
        .bind(host_port)
        .execute(pool)
        .await
        .expect("insert container");
    }

    #[tokio::test]
    async fn returns_every_running_web_replica_port() {
        let pool = test_pool().await;
        insert_app(&pool, "app-1", "one", "dep-1").await;
        insert_app(&pool, "app-2", "two", "dep-2").await;
        insert_container(&pool, "c1", "app-1", "web", "running", Some(30001)).await;
        insert_container(&pool, "c2", "app-1", "web", "running", Some(30002)).await;
        // Excluded: stopped, other process type, no port, and another app.
        insert_container(&pool, "c3", "app-1", "web", "stopped", Some(30003)).await;
        insert_container(&pool, "c4", "app-1", "worker", "running", Some(30004)).await;
        insert_container(&pool, "c5", "app-1", "web", "running", None).await;
        insert_container(&pool, "c6", "app-2", "web", "running", Some(30005)).await;

        let mut ports = list_web_upstream_ports(&pool, "app-1")
            .await
            .expect("ports");
        ports.sort_unstable();
        assert_eq!(ports, vec![30001, 30002]);
    }

    #[tokio::test]
    async fn containers_from_the_previous_deployment_are_not_pooled() {
        let pool = test_pool().await;
        insert_app(&pool, "app-1", "one", "dep-1").await;
        // A newer deployment is live; dep-1 is the version being retired.
        sqlx::query(
            "INSERT INTO deployments (id, app_id, status, builder, image_tag, created_at) \
             VALUES ('dep-2', 'app-1', 'live', 'dockerfile', 'deku/x:2', ?1)",
        )
        .bind((chrono::Utc::now() + chrono::Duration::seconds(1)).to_rfc3339())
        .execute(&pool)
        .await
        .expect("insert newer deployment");

        insert_container_in(&pool, "old", "app-1", "dep-1", Some(32001)).await;
        insert_container_in(&pool, "new", "app-1", "dep-2", Some(32002)).await;

        assert_eq!(
            list_web_upstream_ports(&pool, "app-1")
                .await
                .expect("ports"),
            vec![32002],
            "a retired deployment's containers must not keep serving traffic"
        );
    }

    #[tokio::test]
    async fn upstreams_cover_every_replica_and_point_at_loopback() {
        let pool = test_pool().await;
        insert_app(&pool, "app-1", "one", "dep-1").await;
        insert_container(&pool, "c1", "app-1", "web", "running", Some(31001)).await;
        insert_container(&pool, "c2", "app-1", "web", "running", Some(31002)).await;

        let upstreams = list_web_upstreams(&pool, "app-1").await.expect("upstreams");
        assert_eq!(
            upstreams.len(),
            2,
            "every running replica must be an upstream"
        );
        assert!(upstreams
            .iter()
            .all(|upstream| upstream.host == "127.0.0.1"));
        let mut ports: Vec<u16> = upstreams.iter().map(|upstream| upstream.port).collect();
        ports.sort_unstable();
        assert_eq!(ports, vec![31001, 31002]);
    }

    #[tokio::test]
    async fn upstreams_are_empty_for_an_app_that_is_not_serving() {
        let pool = test_pool().await;
        insert_app(&pool, "app-1", "one", "dep-1").await;
        insert_container(&pool, "c1", "app-1", "web", "stopped", Some(31001)).await;
        // A published port mapping alone must not produce an upstream.
        sqlx::query(
            "INSERT INTO port_mappings (id, app_id, host_port, container_port, protocol) \
             VALUES ('pm-1', 'app-1', 31001, 3000, 'tcp')",
        )
        .execute(&pool)
        .await
        .expect("insert port mapping");

        assert!(list_web_upstreams(&pool, "app-1")
            .await
            .expect("upstreams")
            .is_empty());
    }

    #[tokio::test]
    async fn returns_nothing_without_running_web_containers() {
        let pool = test_pool().await;
        insert_app(&pool, "app-1", "one", "dep-1").await;
        insert_container(&pool, "c1", "app-1", "web", "stopped", Some(30001)).await;
        assert!(list_web_upstream_ports(&pool, "app-1")
            .await
            .expect("ports")
            .is_empty());
    }
}

#[cfg(test)]
mod deploy_token_tests {
    use super::{
        create_deploy_token, delete_deploy_token, get_deploy_token_by_hash, list_deploy_tokens,
        touch_deploy_token,
    };
    use sqlx::sqlite::SqlitePoolOptions;
    use sqlx::SqlitePool;

    async fn test_pool() -> SqlitePool {
        let pool = SqlitePoolOptions::new()
            .max_connections(1)
            .connect("sqlite::memory:")
            .await
            .expect("pool");
        crate::db::migrate(&pool).await.expect("migrate");
        pool
    }

    async fn insert_app(pool: &SqlitePool, id: &str, name: &str) {
        sqlx::query(
            "INSERT INTO apps (id, name, status, created_at) VALUES (?1, ?2, 'created', ?3)",
        )
        .bind(id)
        .bind(name)
        .bind(chrono::Utc::now().to_rfc3339())
        .execute(pool)
        .await
        .expect("insert app");
    }

    #[tokio::test]
    async fn tokens_are_looked_up_by_digest_and_scoped_to_an_app() {
        let pool = test_pool().await;
        insert_app(&pool, "app-1", "one").await;
        insert_app(&pool, "app-2", "two").await;

        let created = create_deploy_token(&pool, "app-1", "ci", "hash-one", "dkt_AAAA")
            .await
            .expect("create");
        assert_eq!(created.name, "ci");
        assert_eq!(created.token_prefix, "dkt_AAAA");
        assert!(created.last_used_at.is_none());

        let found = get_deploy_token_by_hash(&pool, "hash-one")
            .await
            .expect("lookup")
            .expect("row");
        assert_eq!(found.app_id, "app-1");

        assert!(get_deploy_token_by_hash(&pool, "hash-missing")
            .await
            .expect("lookup")
            .is_none());

        let listed = list_deploy_tokens(&pool, "app-1").await.expect("list");
        assert_eq!(listed.len(), 1);
        assert!(list_deploy_tokens(&pool, "app-2")
            .await
            .expect("list")
            .is_empty());
    }

    #[tokio::test]
    async fn touching_records_last_use_and_deleting_is_scoped() {
        let pool = test_pool().await;
        insert_app(&pool, "app-1", "one").await;
        insert_app(&pool, "app-2", "two").await;

        let token = create_deploy_token(&pool, "app-1", "ci", "hash-one", "dkt_AAAA")
            .await
            .expect("create");

        touch_deploy_token(&pool, &token.id).await.expect("touch");
        let touched = get_deploy_token_by_hash(&pool, "hash-one")
            .await
            .expect("lookup")
            .expect("row");
        assert!(touched.last_used_at.is_some());

        // Another app cannot revoke this token.
        assert!(!delete_deploy_token(&pool, "app-2", &token.id)
            .await
            .expect("delete"));
        assert!(delete_deploy_token(&pool, "app-1", &token.id)
            .await
            .expect("delete"));
        assert!(list_deploy_tokens(&pool, "app-1")
            .await
            .expect("list")
            .is_empty());
    }

    #[tokio::test]
    async fn digests_are_unique() {
        let pool = test_pool().await;
        insert_app(&pool, "app-1", "one").await;

        create_deploy_token(&pool, "app-1", "ci", "same-hash", "dkt_AAAA")
            .await
            .expect("first insert");
        assert!(
            create_deploy_token(&pool, "app-1", "ci", "same-hash", "dkt_BBBB")
                .await
                .is_err()
        );
    }
}

#[cfg(test)]
mod schedule_tests {
    use super::{
        backups_beyond_retention, delete_backup_schedule, delete_service_backup,
        get_backup_schedule, list_due_backup_schedules, mark_backup_schedule_run,
        upsert_backup_schedule,
    };
    use chrono::{Duration, Utc};
    use sqlx::sqlite::SqlitePoolOptions;
    use sqlx::SqlitePool;

    async fn test_pool() -> SqlitePool {
        let pool = SqlitePoolOptions::new()
            .max_connections(1)
            .connect("sqlite::memory:")
            .await
            .expect("pool");
        crate::db::migrate(&pool).await.expect("migrate");
        pool
    }

    async fn insert_service(pool: &SqlitePool, id: &str, name: &str) {
        sqlx::query(
            "INSERT INTO services (id, name, plugin, container_id, status, config, created_at) \
             VALUES (?1, ?2, 'redis', NULL, 'running', '{}', ?3)",
        )
        .bind(id)
        .bind(name)
        .bind(Utc::now().to_rfc3339())
        .execute(pool)
        .await
        .expect("insert service");
    }

    async fn insert_backup(
        pool: &SqlitePool,
        service_id: &str,
        key: &str,
        created: chrono::DateTime<Utc>,
    ) {
        sqlx::query(
            "INSERT INTO service_backups (id, service_id, object_key, format, size_bytes, sha256, created_at) \
             VALUES (?1, ?2, ?3, 'redis.rdb', 10, 'abc', ?4)",
        )
        .bind(uuid::Uuid::new_v4().to_string())
        .bind(service_id)
        .bind(key)
        .bind(created.to_rfc3339())
        .execute(pool)
        .await
        .expect("insert backup");
    }

    #[tokio::test]
    async fn retention_keeps_newest_and_flags_older() {
        let pool = test_pool().await;
        insert_service(&pool, "svc-1", "cache").await;
        let base = Utc::now();
        insert_backup(&pool, "svc-1", "k1", base - Duration::hours(3)).await;
        insert_backup(&pool, "svc-1", "k2", base - Duration::hours(2)).await;
        insert_backup(&pool, "svc-1", "k3", base).await;

        let stale = backups_beyond_retention(&pool, "svc-1", 2)
            .await
            .expect("query");
        assert_eq!(stale.len(), 1, "only the oldest should be pruned");
        assert_eq!(stale[0].object_key, "k1");

        delete_service_backup(&pool, &stale[0].id)
            .await
            .expect("delete");
        let remaining = backups_beyond_retention(&pool, "svc-1", 2)
            .await
            .expect("query");
        assert!(remaining.is_empty(), "nothing left beyond retention");
    }

    #[tokio::test]
    async fn schedule_upsert_due_selection_and_mark_run() {
        let pool = test_pool().await;
        insert_service(&pool, "svc-2", "db").await;

        upsert_backup_schedule(&pool, "svc-2", 6, 3)
            .await
            .expect("upsert");
        assert!(
            list_due_backup_schedules(&pool, Utc::now())
                .await
                .expect("due")
                .is_empty(),
            "a fresh schedule is not due"
        );

        sqlx::query("UPDATE backup_schedules SET next_run_at = '2000-01-01T00:00:00Z' WHERE service_id = 'svc-2'")
            .execute(&pool)
            .await
            .expect("force due");

        let due = list_due_backup_schedules(&pool, Utc::now())
            .await
            .expect("due");
        assert_eq!(due.len(), 1);
        assert_eq!(due[0].interval_hours, 6);

        mark_backup_schedule_run(&pool, "svc-2", "ok", Utc::now() + Duration::hours(6))
            .await
            .expect("mark run");
        assert!(
            list_due_backup_schedules(&pool, Utc::now())
                .await
                .expect("due")
                .is_empty(),
            "schedule advances past now after a run"
        );

        let schedule = get_backup_schedule(&pool, "svc-2")
            .await
            .expect("get")
            .expect("present");
        assert_eq!(schedule.last_status.as_deref(), Some("ok"));

        delete_backup_schedule(&pool, "svc-2")
            .await
            .expect("delete");
        assert!(get_backup_schedule(&pool, "svc-2")
            .await
            .expect("get")
            .is_none());
    }
}

// ── Maintenance mode & redirects ──────────────────────────────────────────────

pub async fn get_app_maintenance(
    pool: &SqlitePool,
    app_id: &str,
) -> Result<(bool, Option<String>)> {
    let row = sqlx::query("SELECT maintenance, maintenance_message FROM apps WHERE id = ?1")
        .bind(app_id)
        .fetch_optional(pool)
        .await?
        .ok_or(DekuError::Database(sqlx::Error::RowNotFound))?;
    let enabled: i64 = row.try_get("maintenance")?;
    let message: Option<String> = row.try_get("maintenance_message")?;
    Ok((enabled != 0, message))
}

pub async fn set_app_maintenance(
    pool: &SqlitePool,
    app_id: &str,
    enabled: bool,
    message: Option<&str>,
) -> Result<()> {
    sqlx::query("UPDATE apps SET maintenance = ?2, maintenance_message = ?3 WHERE id = ?1")
        .bind(app_id)
        .bind(if enabled { 1 } else { 0 })
        .bind(message)
        .execute(pool)
        .await?;
    Ok(())
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize, sqlx::FromRow)]
pub struct Redirect {
    pub id: String,
    pub app_id: String,
    pub source_path: String,
    pub target: String,
    pub code: i64,
    pub created_at: chrono::DateTime<chrono::Utc>,
}

pub async fn list_redirects(pool: &SqlitePool, app_id: &str) -> Result<Vec<Redirect>> {
    let redirects = sqlx::query_as::<_, Redirect>(
        "SELECT id, app_id, source_path, target, code, created_at FROM redirects \
         WHERE app_id = ?1 ORDER BY source_path",
    )
    .bind(app_id)
    .fetch_all(pool)
    .await?;
    Ok(redirects)
}

pub async fn add_redirect(
    pool: &SqlitePool,
    app_id: &str,
    source_path: &str,
    target: &str,
    code: i64,
) -> Result<Redirect> {
    let id = Uuid::new_v4().to_string();
    sqlx::query(
        "INSERT INTO redirects (id, app_id, source_path, target, code) VALUES (?1, ?2, ?3, ?4, ?5)",
    )
    .bind(&id)
    .bind(app_id)
    .bind(source_path)
    .bind(target)
    .bind(code)
    .execute(pool)
    .await?;
    let redirect = sqlx::query_as::<_, Redirect>(
        "SELECT id, app_id, source_path, target, code, created_at FROM redirects WHERE id = ?1",
    )
    .bind(&id)
    .fetch_one(pool)
    .await?;
    Ok(redirect)
}

pub async fn remove_redirect(pool: &SqlitePool, app_id: &str, id: &str) -> Result<()> {
    let result = sqlx::query("DELETE FROM redirects WHERE app_id = ?1 AND id = ?2")
        .bind(app_id)
        .bind(id)
        .execute(pool)
        .await?;
    if result.rows_affected() == 0 {
        return Err(DekuError::Internal(format!("redirect '{id}' not found")));
    }
    Ok(())
}
