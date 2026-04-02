use chrono::{DateTime, Utc};
use deku_core::error::{DekuError, Result};
use deku_core::types::{
    App, AppStatus, BuilderType, ConfigVar, ContainerRecord, DeployStatus, Deployment, Domain,
    Event, NewApp, PortMapping, ResourceLimit, StorageMount,
};
use sqlx::SqlitePool;
use uuid::Uuid;

// ── Apps ─────────────────────────────────────────────────────────────────────

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
            id         as "id!",
            name       as "name!",
            created_at as "created_at!: _",
            locked     as "locked!",
            status     as "status!: AppStatus"
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
            id         as "id!",
            name       as "name!",
            created_at as "created_at!: _",
            locked     as "locked!",
            status     as "status!: AppStatus"
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
            id         as "id!",
            name       as "name!",
            created_at as "created_at!: _",
            locked     as "locked!",
            status     as "status!: AppStatus"
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

pub async fn update_app_status(
    pool: &SqlitePool,
    app_id: &str,
    status: AppStatus,
) -> Result<()> {
    sqlx::query!(
        "UPDATE apps SET status = ?1 WHERE id = ?2",
        status,
        app_id
    )
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

    sqlx::query!(
        r#"INSERT INTO deployments (id, app_id, status, builder, created_at)
           VALUES (?1, ?2, ?3, ?4, ?5)"#,
        dep.id,
        dep.app_id,
        dep.status,
        dep.builder,
        dep.created_at,
    )
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

pub async fn get_config_vars(pool: &SqlitePool, app_id: &str) -> Result<Vec<ConfigVar>> {
    let vars = sqlx::query_as!(
        ConfigVar,
        r#"SELECT
            app_id    as "app_id!",
            key       as "key!",
            value     as "value!",
            is_global as "is_global!"
           FROM config_vars
           WHERE app_id = ?1 OR is_global = TRUE
           ORDER BY key"#,
        app_id
    )
    .fetch_all(pool)
    .await?;
    Ok(vars)
}

pub async fn set_config_var(
    pool: &SqlitePool,
    app_id: &str,
    key: &str,
    value: &str,
    is_global: bool,
) -> Result<()> {
    sqlx::query!(
        r#"INSERT INTO config_vars (app_id, key, value, is_global)
           VALUES (?1, ?2, ?3, ?4)
           ON CONFLICT(app_id, key) DO UPDATE SET value = excluded.value, is_global = excluded.is_global"#,
        app_id,
        key,
        value,
        is_global,
    )
    .execute(pool)
    .await?;
    Ok(())
}

pub async fn unset_config_var(pool: &SqlitePool, app_id: &str, key: &str) -> Result<()> {
    sqlx::query!(
        "DELETE FROM config_vars WHERE app_id = ?1 AND key = ?2",
        app_id,
        key
    )
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
        return Err(DekuError::Internal(format!("port mapping '{id}' not found")));
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

pub async fn list_containers_for_deployment(
    pool: &SqlitePool,
    deployment_id: &str,
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
           WHERE deployment_id = ?1 AND status = 'running'"#,
        deployment_id
    )
    .fetch_all(pool)
    .await?;
    Ok(containers)
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
    pub public_key: String,
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
    .map_err(|e| DekuError::Database(e.to_string()))?;

    Ok(SshKey {
        id,
        name: name.to_string(),
        public_key: public_key.to_string(),
        fingerprint: fingerprint.to_string(),
    })
}

pub async fn list_ssh_keys(pool: &SqlitePool) -> Result<Vec<SshKey>> {
    let rows = sqlx::query!(
        r#"SELECT id, name, public_key, fingerprint FROM ssh_keys ORDER BY name"#
    )
    .fetch_all(pool)
    .await
    .map_err(|e| DekuError::Database(e.to_string()))?;

    Ok(rows
        .into_iter()
        .map(|r| SshKey {
            id: r.id,
            name: r.name,
            public_key: r.public_key,
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
    .map_err(|e| DekuError::Database(e.to_string()))?;

    Ok(row.map(|r| SshKey {
        id: r.id,
        name: r.name,
        public_key: r.public_key,
        fingerprint: r.fingerprint,
    }))
}

pub async fn remove_ssh_key(pool: &SqlitePool, name: &str) -> Result<()> {
    sqlx::query!(r#"DELETE FROM ssh_keys WHERE name = ?1"#, name)
        .execute(pool)
        .await
        .map_err(|e| DekuError::Database(e.to_string()))?;
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
