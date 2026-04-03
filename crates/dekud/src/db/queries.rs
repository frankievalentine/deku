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

pub async fn delete_app(pool: &SqlitePool, name: &str) -> Result<()> {
    let result = sqlx::query!("DELETE FROM apps WHERE name = ?1", name)
        .execute(pool)
        .await?;

    if result.rows_affected() == 0 {
        return Err(DekuError::AppNotFound(name.to_string()));
    }

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
    .map_err(DekuError::Database)?;

    Ok(SshKey {
        id,
        name: name.to_string(),
        public_key: public_key.to_string(),
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
    .map_err(DekuError::Database)?;

    Ok(row.map(|r| SshKey {
        id: r.id.unwrap_or_default(),
        name: r.name,
        public_key: r.public_key,
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
    pub id: String,
    pub service_id: String,
    pub app_id: String,
    pub env_key: String,
}

pub struct Network {
    pub id: String,
    pub name: String,
    pub created_at: chrono::DateTime<chrono::Utc>,
}

pub struct CronEntry {
    pub id: String,
    pub app_id: String,
    pub schedule: String,
    pub command: String,
    pub created_at: chrono::DateTime<chrono::Utc>,
}

pub struct ServiceBackup {
    pub id: String,
    pub service_id: String,
    pub object_key: String,
    pub format: String,
    pub size_bytes: i64,
    pub sha256: String,
    pub created_at: chrono::DateTime<chrono::Utc>,
    pub restored_at: Option<chrono::DateTime<chrono::Utc>>,
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

pub async fn update_service_status(
    pool: &SqlitePool,
    service_id: &str,
    container_id: &str,
    status: &str,
) -> Result<()> {
    sqlx::query!(
        "UPDATE services SET container_id = ?1, status = ?2 WHERE id = ?3",
        container_id,
        status,
        service_id,
    )
    .execute(pool)
    .await?;
    Ok(())
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
        id,
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
        id: r.id,
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
            id: r.id,
            service_id: r.service_id,
            app_id: r.app_id,
            env_key: r.env_key,
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
) -> Result<ServiceBackup> {
    let id = Uuid::new_v4().to_string();
    let now = chrono::Utc::now();
    sqlx::query(
        r#"INSERT INTO service_backups
           (id, service_id, object_key, format, size_bytes, sha256, created_at)
           VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)"#,
    )
    .bind(&id)
    .bind(service_id)
    .bind(object_key)
    .bind(format)
    .bind(size_bytes)
    .bind(sha256)
    .bind(now)
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
    })
}

pub async fn list_service_backups(
    pool: &SqlitePool,
    service_id: &str,
) -> Result<Vec<ServiceBackup>> {
    let rows = sqlx::query(
        r#"SELECT id, service_id, object_key, format, size_bytes, sha256, created_at, restored_at
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
        r#"SELECT id, service_id, object_key, format, size_bytes, sha256, created_at, restored_at
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
    })
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
        created_at: now,
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
        created_at: row.created_at,
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
            created_at: r.created_at,
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
            created_at: r.created_at,
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
            app_id: r.app_id,
            schedule: r.schedule,
            command: r.command,
            created_at: r.created_at,
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
        app_id: app_id.to_string(),
        schedule: schedule.to_string(),
        command: command.to_string(),
        created_at: now,
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
