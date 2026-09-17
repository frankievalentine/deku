use std::collections::HashMap;

use anyhow::{anyhow, Result};
use bollard::container::LogOutput;
use bollard::exec::{CreateExecOptions, StartExecOptions, StartExecResults};
use bollard::models::{HostConfig, Mount, MountType, PortBinding};
use bollard::query_parameters::{
    CreateContainerOptionsBuilder, CreateImageOptionsBuilder, LogsOptionsBuilder,
    RemoveContainerOptionsBuilder, RemoveVolumeOptionsBuilder, WaitContainerOptionsBuilder,
};
use bollard::Docker;
use futures::StreamExt;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use sqlx::SqlitePool;
use tokio::io::AsyncWriteExt;
use tokio::net::TcpStream;
use tokio::time::{sleep, timeout, Duration};
use uuid::Uuid;

use crate::config::DekuConfig;
use crate::db::queries;

pub struct DbServiceSpec {
    pub plugin: &'static str,
    pub image: &'static str,
    pub container_port: u16,
    pub env_key: &'static str,
    pub username: Option<&'static str>,
    pub data_dir: &'static str,
    pub make_env: fn(name: &str, password: &str) -> Vec<String>,
    pub make_cmd: fn(name: &str, password: &str) -> Option<Vec<String>>,
    pub make_url: fn(password: &str, host: &str, port: u16, name: &str) -> String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct ServiceConfig {
    pub password: String,
    pub host_port: u16,
    pub name: String,
    pub volume: String,
}

#[derive(Debug, Clone, Serialize)]
pub struct ServiceConnectionInfo {
    pub env_key: &'static str,
    pub host: String,
    pub port: u16,
    pub password: String,
    pub username: Option<String>,
    pub database: Option<String>,
    pub url: String,
    pub volume: String,
}

struct ExecResult {
    stdout: Vec<u8>,
    stderr: Vec<u8>,
    exit_code: i64,
}

pub fn postgres_spec() -> DbServiceSpec {
    DbServiceSpec {
        plugin: "postgres",
        image: "postgres:16-alpine",
        container_port: 5432,
        env_key: "DATABASE_URL",
        username: Some("deku"),
        data_dir: "/var/lib/postgresql/data",
        make_env: |name, password| {
            vec![
                "POSTGRES_USER=deku".into(),
                format!("POSTGRES_PASSWORD={password}"),
                format!("POSTGRES_DB={name}"),
            ]
        },
        make_cmd: |_name, _password| None,
        make_url: |password, host, port, name| {
            format!("postgres://deku:{password}@{host}:{port}/{name}")
        },
    }
}

pub fn redis_spec() -> DbServiceSpec {
    DbServiceSpec {
        plugin: "redis",
        image: "redis:7-alpine",
        container_port: 6379,
        env_key: "REDIS_URL",
        username: None,
        data_dir: "/data",
        make_env: |_name, _password| vec![],
        make_cmd: |_name, password| {
            Some(vec![
                "redis-server".into(),
                "--appendonly".into(),
                "yes".into(),
                "--requirepass".into(),
                password.to_string(),
            ])
        },
        make_url: |password, host, port, _name| format!("redis://:{password}@{host}:{port}"),
    }
}

pub fn mysql_spec() -> DbServiceSpec {
    DbServiceSpec {
        plugin: "mysql",
        image: "mysql:8",
        container_port: 3306,
        env_key: "DATABASE_URL",
        username: Some("deku"),
        data_dir: "/var/lib/mysql",
        make_env: |name, password| {
            vec![
                format!("MYSQL_ROOT_PASSWORD={password}"),
                format!("MYSQL_DATABASE={name}"),
                "MYSQL_USER=deku".into(),
                format!("MYSQL_PASSWORD={password}"),
            ]
        },
        make_cmd: |_name, _password| None,
        make_url: |password, host, port, name| {
            format!("mysql://deku:{password}@{host}:{port}/{name}")
        },
    }
}

/// MariaDB speaks the MySQL protocol and ships the same `mysqldump`-style
/// tooling, so it reuses the SQL backup path with a different image and client
/// binaries. It uses `MARIADB_URL` rather than `DATABASE_URL` so an app can link
/// a MySQL and a MariaDB service at the same time.
pub fn mariadb_spec() -> DbServiceSpec {
    DbServiceSpec {
        plugin: "mariadb",
        image: "mariadb:11",
        container_port: 3306,
        env_key: "MARIADB_URL",
        username: Some("deku"),
        data_dir: "/var/lib/mysql",
        make_env: |name, password| {
            vec![
                format!("MARIADB_ROOT_PASSWORD={password}"),
                format!("MARIADB_DATABASE={name}"),
                "MARIADB_USER=deku".into(),
                format!("MARIADB_PASSWORD={password}"),
            ]
        },
        make_cmd: |_name, _password| None,
        make_url: |password, host, port, name| {
            format!("mysql://deku:{password}@{host}:{port}/{name}")
        },
    }
}

/// MongoDB authenticates against the `admin` database, so the URL carries
/// `authSource=admin` and the app database stays a separate namespace.
pub fn mongodb_spec() -> DbServiceSpec {
    DbServiceSpec {
        plugin: "mongodb",
        image: "mongo:7",
        container_port: 27017,
        env_key: "MONGODB_URL",
        username: Some("deku"),
        data_dir: "/data/db",
        make_env: |name, password| {
            vec![
                "MONGO_INITDB_ROOT_USERNAME=deku".into(),
                format!("MONGO_INITDB_ROOT_PASSWORD={password}"),
                format!("MONGO_INITDB_DATABASE={name}"),
            ]
        },
        make_cmd: |_name, _password| None,
        make_url: |password, host, port, name| {
            format!("mongodb://deku:{password}@{host}:{port}/{name}?authSource=admin")
        },
    }
}

/// Resolve a managed service plugin to its spec.
pub fn spec_for(plugin: &str) -> Option<DbServiceSpec> {
    match plugin {
        "postgres" => Some(postgres_spec()),
        "redis" => Some(redis_spec()),
        "mysql" => Some(mysql_spec()),
        "mariadb" => Some(mariadb_spec()),
        "mongodb" => Some(mongodb_spec()),
        _ => None,
    }
}

pub fn parse_service_config(config: &str) -> Result<ServiceConfig> {
    serde_json::from_str(config).map_err(|e| anyhow!("invalid service config: {e}"))
}

pub fn connection_info(
    spec: &DbServiceSpec,
    service: &queries::Service,
) -> Result<ServiceConnectionInfo> {
    let config = parse_service_config(&service.config)?;
    Ok(ServiceConnectionInfo {
        env_key: spec.env_key,
        host: "127.0.0.1".to_string(),
        port: config.host_port,
        password: config.password.clone(),
        username: spec.username.map(str::to_string),
        database: if spec.plugin == "redis" {
            None
        } else {
            Some(config.name.clone())
        },
        url: (spec.make_url)(
            &config.password,
            "127.0.0.1",
            config.host_port,
            &config.name,
        ),
        volume: config.volume,
    })
}

/// Wait until the service accepts client connections.
///
/// An open TCP port is not enough: MariaDB and MySQL open the port before they
/// finish initializing, so a backup or restore issued immediately after
/// creation can hit a missing socket. Each plugin gets its own cheap probe.
async fn wait_for_service_ready(
    docker: &Docker,
    plugin: &str,
    container_name: &str,
    password: &str,
) -> Result<()> {
    let (args, env): (Vec<String>, Vec<String>) = match plugin {
        "postgres" => (
            vec![
                "pg_isready".to_string(),
                "-U".to_string(),
                "deku".to_string(),
            ],
            Vec::new(),
        ),
        "redis" => (
            vec!["redis-cli".to_string(), "ping".to_string()],
            vec![format!("REDISCLI_AUTH={password}")],
        ),
        // `mysqladmin ping` returns success even when authentication fails, and
        // the entrypoint runs a temporary server before the configured password
        // takes effect. Running a real query proves the credential works.
        "mysql" => (
            vec![
                "mysql".to_string(),
                "-u".to_string(),
                "root".to_string(),
                "-e".to_string(),
                "SELECT 1".to_string(),
            ],
            vec![format!("MYSQL_PWD={password}")],
        ),
        "mariadb" => (
            vec![
                "mariadb".to_string(),
                "-u".to_string(),
                "root".to_string(),
                "-e".to_string(),
                "SELECT 1".to_string(),
            ],
            vec![format!("MYSQL_PWD={password}")],
        ),
        "mongodb" => (
            vec![
                "mongosh".to_string(),
                format!(
                    "mongodb://deku:{password}@127.0.0.1:{}/admin?authSource=admin",
                    mongodb_spec().container_port
                ),
                "--quiet".to_string(),
                "--eval".to_string(),
                "db.runCommand({ping:1}).ok".to_string(),
            ],
            Vec::new(),
        ),
        _ => return Ok(()),
    };

    for _attempt in 0..60 {
        let probe = exec_in_container(
            docker,
            container_name,
            args.clone(),
            env.clone(),
            None,
            None,
        )
        .await;

        if probe.is_ok_and(|result| result.exit_code == 0) {
            return Ok(());
        }
        sleep(Duration::from_secs(1)).await;
    }

    Err(anyhow!(
        "{plugin} service on {container_name} did not become ready in time"
    ))
}

async fn wait_for_tcp_port(host_port: u16) -> Result<()> {
    let address = format!("127.0.0.1:{host_port}");
    for _attempt in 0..30 {
        if timeout(Duration::from_secs(1), TcpStream::connect(&address))
            .await
            .is_ok_and(|result| result.is_ok())
        {
            return Ok(());
        }
        sleep(Duration::from_secs(1)).await;
    }
    Err(anyhow!("service on {address} did not become ready in time"))
}

pub async fn create(
    pool: &SqlitePool,
    docker: &Docker,
    spec: DbServiceSpec,
    name: &str,
) -> Result<queries::Service> {
    // Pull image
    let mut stream = docker.create_image(
        Some(
            CreateImageOptionsBuilder::default()
                .from_image(spec.image)
                .build(),
        ),
        None,
        None,
    );
    while stream.next().await.is_some() {}

    // Generate password
    let password = uuid::Uuid::new_v4().simple().to_string();

    // Build port bindings
    let port_key = format!("{}/tcp", spec.container_port);
    let mut bindings: HashMap<String, Option<Vec<PortBinding>>> = HashMap::new();
    bindings.insert(
        port_key.clone(),
        Some(vec![PortBinding {
            host_ip: Some(String::new()),
            host_port: Some("0".to_string()),
        }]),
    );

    let exposed: Vec<String> = vec![port_key.clone()];

    let envs: Vec<String> = (spec.make_env)(name, &password);
    let cmd = (spec.make_cmd)(name, &password);

    let container_name = format!("deku-{}-{name}", spec.plugin);
    let volume_name = format!("deku-{}-{name}-data", spec.plugin);

    let body = bollard::models::ContainerCreateBody {
        image: Some(spec.image.to_string()),
        env: Some(envs),
        cmd,
        exposed_ports: Some(exposed),
        host_config: Some(HostConfig {
            port_bindings: Some(bindings),
            mounts: Some(vec![Mount {
                source: Some(volume_name.clone()),
                target: Some(spec.data_dir.to_string()),
                typ: Some(MountType::VOLUME),
                read_only: Some(false),
                ..Default::default()
            }]),
            ..Default::default()
        }),
        ..Default::default()
    };

    let resp = docker
        .create_container(
            Some(
                CreateContainerOptionsBuilder::default()
                    .name(&container_name)
                    .build(),
            ),
            body,
        )
        .await?;
    let container_id = resp.id;

    docker
        .start_container(
            &container_id,
            None::<bollard::query_parameters::StartContainerOptions>,
        )
        .await?;

    // Inspect to get assigned host port
    let inspect = docker.inspect_container(&container_id, None).await?;
    let host_port: u16 = inspect
        .network_settings
        .as_ref()
        .and_then(|ns| ns.ports.as_ref())
        .and_then(|ports| {
            let key = format!("{}/tcp", spec.container_port);
            ports.get(&key)
        })
        .and_then(|bindings| bindings.as_ref())
        .and_then(|b| b.first())
        .and_then(|b| b.host_port.as_deref())
        .and_then(|p| p.parse().ok())
        .ok_or_else(|| anyhow!("could not determine host port for service container"))?;

    if let Err(error) = wait_for_tcp_port(host_port).await {
        let _ = docker
            .remove_container(
                &container_name,
                Some(RemoveContainerOptionsBuilder::default().force(true).build()),
            )
            .await;
        let _ = docker
            .remove_volume(
                &volume_name,
                Some(RemoveVolumeOptionsBuilder::default().force(true).build()),
            )
            .await;
        return Err(error);
    }

    if let Err(error) =
        wait_for_service_ready(docker, spec.plugin, &container_name, &password).await
    {
        let _ = docker
            .remove_container(
                &container_name,
                Some(RemoveContainerOptionsBuilder::default().force(true).build()),
            )
            .await;
        let _ = docker
            .remove_volume(
                &volume_name,
                Some(RemoveVolumeOptionsBuilder::default().force(true).build()),
            )
            .await;
        return Err(error);
    }

    let config = serde_json::to_string(&ServiceConfig {
        password,
        host_port,
        name: name.to_string(),
        volume: volume_name,
    })?;

    let svc =
        queries::create_service(pool, name, spec.plugin, Some(&container_id), &config).await?;
    Ok(svc)
}

pub async fn destroy(pool: &SqlitePool, docker: &Docker, name: &str) -> Result<()> {
    let svc = queries::get_service(pool, name).await?;
    let plugin = svc.plugin.clone();
    let config = parse_service_config(&svc.config).ok();
    let links = queries::delete_service(pool, name).await?;

    for link in links {
        let _ = queries::unset_config_var(pool, &link.app_id, &link.env_key).await;
    }

    let container_name = format!("deku-{plugin}-{name}");
    let _ = docker
        .stop_container(
            &container_name,
            None::<bollard::query_parameters::StopContainerOptions>,
        )
        .await;
    let _ = docker
        .remove_container(
            &container_name,
            Some(RemoveContainerOptionsBuilder::default().force(true).build()),
        )
        .await;
    if let Some(config) = config {
        let _ = docker
            .remove_volume(
                &config.volume,
                Some(RemoveVolumeOptionsBuilder::default().force(true).build()),
            )
            .await;
    }

    Ok(())
}

pub async fn link(
    pool: &SqlitePool,
    cfg: &DekuConfig,
    service_name: &str,
    app_name: &str,
    spec: &DbServiceSpec,
) -> Result<()> {
    let svc = queries::get_service(pool, service_name).await?;
    let app = queries::get_app(pool, app_name).await?;
    if svc.plugin != spec.plugin {
        return Err(anyhow!(
            "service '{service_name}' is a {} service, not {}",
            svc.plugin,
            spec.plugin
        ));
    }

    let config = parse_service_config(&svc.config)?;

    let url = (spec.make_url)(
        &config.password,
        "127.0.0.1",
        config.host_port,
        &config.name,
    );
    crate::secrets::set_config_var(pool, cfg, &app.id, spec.env_key, &url, false).await?;
    queries::link_service(pool, &svc.id, &app.id, spec.env_key).await?;
    Ok(())
}

/// Seal a dump for upload and report the label to record in the database.
fn seal_backup(cfg: &DekuConfig, payload: Vec<u8>) -> Result<(Vec<u8>, &'static str)> {
    match cfg.at_rest_cipher()? {
        Some(cipher) => Ok((
            cipher.seal(crate::crypto::ENVELOPE_MAGIC_BACKUP, &payload)?,
            crate::crypto::ENCRYPTION_LABEL_AES256_GCM,
        )),
        None => Ok((payload, crate::crypto::ENCRYPTION_LABEL_NONE)),
    }
}

/// Decrypt a downloaded backup. Payloads written before encryption was enabled
/// carry no envelope and pass through untouched.
fn open_backup(cfg: &DekuConfig, payload: Vec<u8>) -> Result<Vec<u8>> {
    if !crate::crypto::is_encrypted(&payload, crate::crypto::ENVELOPE_MAGIC_BACKUP) {
        return Ok(payload);
    }
    let cipher = cfg.at_rest_cipher()?.ok_or_else(|| {
        anyhow!(
            "backup is encrypted but no key is configured; set DEKU_ENCRYPTION_KEY or [encryption]"
        )
    })?;
    cipher.open(crate::crypto::ENVELOPE_MAGIC_BACKUP, &payload)
}

pub async fn backup_postgres(
    pool: &SqlitePool,
    docker: &Docker,
    cfg: &DekuConfig,
    service_name: &str,
) -> Result<queries::ServiceBackup> {
    let object_store = cfg
        .object_store
        .as_ref()
        .ok_or_else(|| anyhow!("object store is not configured"))?;
    let service = queries::get_service_for_plugin(pool, service_name, "postgres").await?;
    let connection = connection_info(&postgres_spec(), &service)?;
    let container_name = format!("deku-postgres-{service_name}");

    let exec = exec_in_container(
        docker,
        &container_name,
        vec![
            "pg_dump".to_string(),
            "-U".to_string(),
            connection
                .username
                .clone()
                .unwrap_or_else(|| "deku".to_string()),
            "-d".to_string(),
            connection
                .database
                .clone()
                .unwrap_or_else(|| service_name.to_string()),
            "--clean".to_string(),
            "--if-exists".to_string(),
            "--no-owner".to_string(),
            "--no-privileges".to_string(),
        ],
        vec![format!("PGPASSWORD={}", connection.password)],
        None,
        Some("postgres".to_string()),
    )
    .await?;

    if exec.exit_code != 0 {
        return Err(anyhow!(
            "pg_dump failed with exit code {}: {}",
            exec.exit_code,
            String::from_utf8_lossy(&exec.stderr)
        ));
    }

    let object_key = format!(
        "{}backups/postgres/{}/{}-{}.sql",
        object_store.normalized_prefix().unwrap_or_default(),
        service_name,
        chrono::Utc::now().format("%Y%m%dT%H%M%SZ"),
        Uuid::new_v4().simple()
    );
    let (payload, encryption) = seal_backup(cfg, exec.stdout.clone())?;
    crate::objectstore::put_bytes(object_store, &object_key, payload.clone()).await?;

    let sha256 = hex::encode(Sha256::digest(&payload));
    queries::create_service_backup(
        pool,
        &service.id,
        &object_key,
        "postgres.sql",
        payload.len() as i64,
        &sha256,
        encryption,
    )
    .await
    .map_err(Into::into)
}

pub async fn list_backups(
    pool: &SqlitePool,
    service_name: &str,
    plugin: &str,
) -> Result<Vec<queries::ServiceBackup>> {
    let service = queries::get_service_for_plugin(pool, service_name, plugin).await?;
    queries::list_service_backups(pool, &service.id)
        .await
        .map_err(Into::into)
}

pub async fn restore_postgres(
    pool: &SqlitePool,
    docker: &Docker,
    cfg: &DekuConfig,
    service_name: &str,
    backup_id: &str,
) -> Result<queries::ServiceBackup> {
    let object_store = cfg
        .object_store
        .as_ref()
        .ok_or_else(|| anyhow!("object store is not configured"))?;
    let service = queries::get_service_for_plugin(pool, service_name, "postgres").await?;
    let backup = queries::get_service_backup_for_service(pool, &service.id, backup_id).await?;
    let connection = connection_info(&postgres_spec(), &service)?;
    let container_name = format!("deku-postgres-{service_name}");
    let payload = crate::objectstore::get_bytes(object_store, &backup.object_key).await?;
    let payload_sha256 = hex::encode(Sha256::digest(&payload));
    if payload_sha256 != backup.sha256 {
        return Err(anyhow!(
            "backup checksum mismatch for '{}': expected {}, got {}",
            backup.id,
            backup.sha256,
            payload_sha256
        ));
    }

    let payload = open_backup(cfg, payload)?;

    let database_name = connection
        .database
        .clone()
        .unwrap_or_else(|| service_name.to_string());

    let terminate_exec = exec_in_container(
        docker,
        &container_name,
        vec![
            "psql".to_string(),
            "-U".to_string(),
            connection.username.clone().unwrap_or_else(|| "deku".to_string()),
            "-d".to_string(),
            "postgres".to_string(),
            "-c".to_string(),
            format!(
                "SELECT pg_terminate_backend(pid) FROM pg_stat_activity WHERE datname = '{}' AND pid <> pg_backend_pid();",
                database_name.replace('\'', "''")
            ),
        ],
        vec![format!("PGPASSWORD={}", connection.password)],
        None,
        Some("postgres".to_string()),
    )
    .await?;
    if terminate_exec.exit_code != 0 {
        return Err(anyhow!(
            "failed to terminate active postgres connections: {}",
            String::from_utf8_lossy(&terminate_exec.stderr)
        ));
    }

    let restore_exec = exec_in_container(
        docker,
        &container_name,
        vec![
            "psql".to_string(),
            "-U".to_string(),
            connection
                .username
                .clone()
                .unwrap_or_else(|| "deku".to_string()),
            "-d".to_string(),
            database_name,
        ],
        vec![format!("PGPASSWORD={}", connection.password)],
        Some(payload),
        Some("postgres".to_string()),
    )
    .await?;
    if restore_exec.exit_code != 0 {
        return Err(anyhow!(
            "psql restore failed with exit code {}: {}",
            restore_exec.exit_code,
            String::from_utf8_lossy(&restore_exec.stderr)
        ));
    }

    queries::mark_service_backup_restored(pool, &backup.id).await?;
    queries::get_service_backup_for_service(pool, &service.id, backup_id)
        .await
        .map_err(Into::into)
}

/// SQL dump/restore shared by MySQL and MariaDB.
async fn backup_sql(
    pool: &SqlitePool,
    docker: &Docker,
    cfg: &DekuConfig,
    service_name: &str,
    spec: &DbServiceSpec,
    dump_binary: &str,
) -> Result<queries::ServiceBackup> {
    let object_store = cfg
        .object_store
        .as_ref()
        .ok_or_else(|| anyhow!("object store is not configured"))?;
    let service = queries::get_service_for_plugin(pool, service_name, spec.plugin).await?;
    let connection = connection_info(spec, &service)?;
    let container_name = format!("deku-{}-{service_name}", spec.plugin);

    let exec = exec_in_container(
        docker,
        &container_name,
        vec![
            dump_binary.to_string(),
            "-u".to_string(),
            "root".to_string(),
            "--all-databases".to_string(),
            "--single-transaction".to_string(),
            "--routines".to_string(),
            "--events".to_string(),
            "--no-tablespaces".to_string(),
        ],
        vec![format!("MYSQL_PWD={}", connection.password)],
        None,
        None,
    )
    .await?;

    if exec.exit_code != 0 {
        return Err(anyhow!(
            "{} failed with exit code {}: {}",
            dump_binary,
            exec.exit_code,
            String::from_utf8_lossy(&exec.stderr)
        ));
    }

    let object_key = format!(
        "{}backups/{}/{}/{}-{}.sql",
        object_store.normalized_prefix().unwrap_or_default(),
        spec.plugin,
        service_name,
        chrono::Utc::now().format("%Y%m%dT%H%M%SZ"),
        Uuid::new_v4().simple()
    );
    let (payload, encryption) = seal_backup(cfg, exec.stdout.clone())?;
    crate::objectstore::put_bytes(object_store, &object_key, payload.clone()).await?;

    let sha256 = hex::encode(Sha256::digest(&payload));
    queries::create_service_backup(
        pool,
        &service.id,
        &object_key,
        &format!("{}.sql", spec.plugin),
        payload.len() as i64,
        &sha256,
        encryption,
    )
    .await
    .map_err(Into::into)
}

async fn restore_sql(
    pool: &SqlitePool,
    docker: &Docker,
    cfg: &DekuConfig,
    service_name: &str,
    backup_id: &str,
    spec: &DbServiceSpec,
    restore_binary: &str,
) -> Result<queries::ServiceBackup> {
    let object_store = cfg
        .object_store
        .as_ref()
        .ok_or_else(|| anyhow!("object store is not configured"))?;
    let service = queries::get_service_for_plugin(pool, service_name, spec.plugin).await?;
    let backup = queries::get_service_backup_for_service(pool, &service.id, backup_id).await?;
    let connection = connection_info(spec, &service)?;
    let container_name = format!("deku-{}-{service_name}", spec.plugin);

    let payload = crate::objectstore::get_bytes(object_store, &backup.object_key).await?;
    let payload_sha256 = hex::encode(Sha256::digest(&payload));
    if payload_sha256 != backup.sha256 {
        return Err(anyhow!(
            "backup checksum mismatch for '{}': expected {}, got {}",
            backup.id,
            backup.sha256,
            payload_sha256
        ));
    }

    let payload = open_backup(cfg, payload)?;

    let restore_exec = exec_in_container(
        docker,
        &container_name,
        vec![
            restore_binary.to_string(),
            "-u".to_string(),
            "root".to_string(),
        ],
        vec![format!("MYSQL_PWD={}", connection.password)],
        Some(payload),
        None,
    )
    .await?;
    if restore_exec.exit_code != 0 {
        return Err(anyhow!(
            "{} restore failed with exit code {}: {}",
            restore_binary,
            restore_exec.exit_code,
            String::from_utf8_lossy(&restore_exec.stderr)
        ));
    }

    queries::mark_service_backup_restored(pool, &backup.id).await?;
    queries::get_service_backup_for_service(pool, &service.id, backup_id)
        .await
        .map_err(Into::into)
}

pub async fn backup_mysql(
    pool: &SqlitePool,
    docker: &Docker,
    cfg: &DekuConfig,
    service_name: &str,
) -> Result<queries::ServiceBackup> {
    backup_sql(pool, docker, cfg, service_name, &mysql_spec(), "mysqldump").await
}

pub async fn restore_mysql(
    pool: &SqlitePool,
    docker: &Docker,
    cfg: &DekuConfig,
    service_name: &str,
    backup_id: &str,
) -> Result<queries::ServiceBackup> {
    restore_sql(
        pool,
        docker,
        cfg,
        service_name,
        backup_id,
        &mysql_spec(),
        "mysql",
    )
    .await
}

pub async fn backup_mariadb(
    pool: &SqlitePool,
    docker: &Docker,
    cfg: &DekuConfig,
    service_name: &str,
) -> Result<queries::ServiceBackup> {
    backup_sql(
        pool,
        docker,
        cfg,
        service_name,
        &mariadb_spec(),
        "mariadb-dump",
    )
    .await
}

pub async fn restore_mariadb(
    pool: &SqlitePool,
    docker: &Docker,
    cfg: &DekuConfig,
    service_name: &str,
    backup_id: &str,
) -> Result<queries::ServiceBackup> {
    restore_sql(
        pool,
        docker,
        cfg,
        service_name,
        backup_id,
        &mariadb_spec(),
        "mariadb",
    )
    .await
}

/// Connection URI usable from inside the service container, where the database
/// listens on its container port rather than the host-mapped one.
fn internal_mongo_uri(connection: &ServiceConnectionInfo, spec: &DbServiceSpec) -> String {
    let database = connection.database.as_deref().unwrap_or("admin");
    format!(
        "mongodb://deku:{}@127.0.0.1:{}/{}?authSource=admin",
        connection.password, spec.container_port, database
    )
}

pub async fn backup_mongodb(
    pool: &SqlitePool,
    docker: &Docker,
    cfg: &DekuConfig,
    service_name: &str,
) -> Result<queries::ServiceBackup> {
    let object_store = cfg
        .object_store
        .as_ref()
        .ok_or_else(|| anyhow!("object store is not configured"))?;
    let spec = mongodb_spec();
    let service = queries::get_service_for_plugin(pool, service_name, spec.plugin).await?;
    let connection = connection_info(&spec, &service)?;
    let container_name = format!("deku-mongodb-{service_name}");

    let exec = exec_in_container(
        docker,
        &container_name,
        vec![
            "mongodump".to_string(),
            format!("--uri={}", internal_mongo_uri(&connection, &spec)),
            "--archive".to_string(),
            "--gzip".to_string(),
        ],
        Vec::new(),
        None,
        None,
    )
    .await?;

    if exec.exit_code != 0 {
        return Err(anyhow!(
            "mongodump failed with exit code {}: {}",
            exec.exit_code,
            String::from_utf8_lossy(&exec.stderr)
        ));
    }

    let object_key = format!(
        "{}backups/mongodb/{}/{}-{}.archive.gz",
        object_store.normalized_prefix().unwrap_or_default(),
        service_name,
        chrono::Utc::now().format("%Y%m%dT%H%M%SZ"),
        Uuid::new_v4().simple()
    );
    let (payload, encryption) = seal_backup(cfg, exec.stdout.clone())?;
    crate::objectstore::put_bytes(object_store, &object_key, payload.clone()).await?;

    let sha256 = hex::encode(Sha256::digest(&payload));
    queries::create_service_backup(
        pool,
        &service.id,
        &object_key,
        "mongodb.archive.gz",
        payload.len() as i64,
        &sha256,
        encryption,
    )
    .await
    .map_err(Into::into)
}

pub async fn restore_mongodb(
    pool: &SqlitePool,
    docker: &Docker,
    cfg: &DekuConfig,
    service_name: &str,
    backup_id: &str,
) -> Result<queries::ServiceBackup> {
    let object_store = cfg
        .object_store
        .as_ref()
        .ok_or_else(|| anyhow!("object store is not configured"))?;
    let spec = mongodb_spec();
    let service = queries::get_service_for_plugin(pool, service_name, spec.plugin).await?;
    let backup = queries::get_service_backup_for_service(pool, &service.id, backup_id).await?;
    let connection = connection_info(&spec, &service)?;
    let container_name = format!("deku-mongodb-{service_name}");

    let payload = crate::objectstore::get_bytes(object_store, &backup.object_key).await?;
    let payload_sha256 = hex::encode(Sha256::digest(&payload));
    if payload_sha256 != backup.sha256 {
        return Err(anyhow!(
            "backup checksum mismatch for '{}': expected {}, got {}",
            backup.id,
            backup.sha256,
            payload_sha256
        ));
    }

    let payload = open_backup(cfg, payload)?;

    let restore_exec = exec_in_container(
        docker,
        &container_name,
        vec![
            "mongorestore".to_string(),
            format!("--uri={}", internal_mongo_uri(&connection, &spec)),
            "--archive".to_string(),
            "--gzip".to_string(),
            "--drop".to_string(),
        ],
        Vec::new(),
        Some(payload),
        None,
    )
    .await?;
    if restore_exec.exit_code != 0 {
        return Err(anyhow!(
            "mongorestore failed with exit code {}: {}",
            restore_exec.exit_code,
            String::from_utf8_lossy(&restore_exec.stderr)
        ));
    }

    queries::mark_service_backup_restored(pool, &backup.id).await?;
    queries::get_service_backup_for_service(pool, &service.id, backup_id)
        .await
        .map_err(Into::into)
}

/// Run a backup using the implementation for `plugin`.
pub async fn backup_for_plugin(
    pool: &SqlitePool,
    docker: &Docker,
    cfg: &DekuConfig,
    plugin: &str,
    service_name: &str,
) -> Result<queries::ServiceBackup> {
    match plugin {
        "postgres" => backup_postgres(pool, docker, cfg, service_name).await,
        "redis" => backup_redis(pool, docker, cfg, service_name).await,
        "mysql" => backup_mysql(pool, docker, cfg, service_name).await,
        "mariadb" => backup_mariadb(pool, docker, cfg, service_name).await,
        "mongodb" => backup_mongodb(pool, docker, cfg, service_name).await,
        other => Err(anyhow!("backups are not supported for '{other}' services")),
    }
}

/// Restore a backup using the implementation for `plugin`.
pub async fn restore_for_plugin(
    pool: &SqlitePool,
    docker: &Docker,
    cfg: &DekuConfig,
    plugin: &str,
    service_name: &str,
    backup_id: &str,
) -> Result<queries::ServiceBackup> {
    match plugin {
        "postgres" => restore_postgres(pool, docker, cfg, service_name, backup_id).await,
        "redis" => restore_redis(pool, docker, cfg, service_name, backup_id).await,
        "mysql" => restore_mysql(pool, docker, cfg, service_name, backup_id).await,
        "mariadb" => restore_mariadb(pool, docker, cfg, service_name, backup_id).await,
        "mongodb" => restore_mongodb(pool, docker, cfg, service_name, backup_id).await,
        other => Err(anyhow!("backups are not supported for '{other}' services")),
    }
}

pub async fn backup_redis(
    pool: &SqlitePool,
    docker: &Docker,
    cfg: &DekuConfig,
    service_name: &str,
) -> Result<queries::ServiceBackup> {
    let object_store = cfg
        .object_store
        .as_ref()
        .ok_or_else(|| anyhow!("object store is not configured"))?;
    let service = queries::get_service_for_plugin(pool, service_name, "redis").await?;
    let connection = connection_info(&redis_spec(), &service)?;
    let container_name = format!("deku-redis-{service_name}");

    let save_exec = exec_in_container(
        docker,
        &container_name,
        vec!["redis-cli".to_string(), "SAVE".to_string()],
        vec![format!("REDISCLI_AUTH={}", connection.password)],
        None,
        None,
    )
    .await?;
    if save_exec.exit_code != 0 {
        return Err(anyhow!(
            "redis SAVE failed with exit code {}: {}",
            save_exec.exit_code,
            String::from_utf8_lossy(&save_exec.stderr)
        ));
    }

    let dump_exec = exec_in_container(
        docker,
        &container_name,
        vec!["cat".to_string(), "/data/dump.rdb".to_string()],
        Vec::new(),
        None,
        None,
    )
    .await?;
    if dump_exec.exit_code != 0 {
        return Err(anyhow!(
            "reading redis dump.rdb failed with exit code {}: {}",
            dump_exec.exit_code,
            String::from_utf8_lossy(&dump_exec.stderr)
        ));
    }

    let object_key = format!(
        "{}backups/redis/{}/{}-{}.rdb",
        object_store.normalized_prefix().unwrap_or_default(),
        service_name,
        chrono::Utc::now().format("%Y%m%dT%H%M%SZ"),
        Uuid::new_v4().simple()
    );
    let (payload, encryption) = seal_backup(cfg, dump_exec.stdout.clone())?;
    crate::objectstore::put_bytes(object_store, &object_key, payload.clone()).await?;

    let sha256 = hex::encode(Sha256::digest(&payload));
    queries::create_service_backup(
        pool,
        &service.id,
        &object_key,
        "redis.rdb",
        payload.len() as i64,
        &sha256,
        encryption,
    )
    .await
    .map_err(Into::into)
}

pub async fn restore_redis(
    pool: &SqlitePool,
    docker: &Docker,
    cfg: &DekuConfig,
    service_name: &str,
    backup_id: &str,
) -> Result<queries::ServiceBackup> {
    let object_store = cfg
        .object_store
        .as_ref()
        .ok_or_else(|| anyhow!("object store is not configured"))?;
    let service = queries::get_service_for_plugin(pool, service_name, "redis").await?;
    let service_config = parse_service_config(&service.config)?;
    let backup = queries::get_service_backup_for_service(pool, &service.id, backup_id).await?;
    let payload = crate::objectstore::get_bytes(object_store, &backup.object_key).await?;
    let payload_sha256 = hex::encode(Sha256::digest(&payload));
    if payload_sha256 != backup.sha256 {
        return Err(anyhow!(
            "backup checksum mismatch for '{}': expected {}, got {}",
            backup.id,
            backup.sha256,
            payload_sha256
        ));
    }

    let payload = open_backup(cfg, payload)?;

    let container_name = format!("deku-redis-{service_name}");
    let _ = docker
        .stop_container(
            &container_name,
            None::<bollard::query_parameters::StopContainerOptions>,
        )
        .await;

    let temp_dir = tempfile::tempdir()?;
    let backup_path = temp_dir.path().join("dump.rdb");
    std::fs::write(&backup_path, &payload)?;

    let restore_cmd = concat!(
        "rm -rf /data/appendonlydir /data/dump.rdb && ",
        "cp /restore/dump.rdb /data/dump.rdb && ",
        "chown redis:redis /data/dump.rdb 2>/dev/null || true"
    );
    let restore_output = run_helper_container(
        docker,
        "redis:7-alpine",
        vec!["sh".to_string(), "-c".to_string(), restore_cmd.to_string()],
        vec![
            Mount {
                source: Some(service_config.volume.clone()),
                target: Some("/data".to_string()),
                typ: Some(MountType::VOLUME),
                read_only: Some(false),
                ..Default::default()
            },
            Mount {
                source: Some(backup_path.to_string_lossy().to_string()),
                target: Some("/restore/dump.rdb".to_string()),
                typ: Some(MountType::BIND),
                read_only: Some(true),
                ..Default::default()
            },
        ],
    )
    .await;

    if let Err(error) = restore_output {
        let _ = docker
            .start_container(
                &container_name,
                None::<bollard::query_parameters::StartContainerOptions>,
            )
            .await;
        return Err(error);
    }

    docker
        .start_container(
            &container_name,
            None::<bollard::query_parameters::StartContainerOptions>,
        )
        .await?;
    wait_for_tcp_port(service_config.host_port).await?;

    queries::mark_service_backup_restored(pool, &backup.id).await?;
    queries::get_service_backup_for_service(pool, &service.id, backup_id)
        .await
        .map_err(Into::into)
}

pub async fn verify_linked_services_post_deploy(
    pool: &SqlitePool,
    docker: &Docker,
    cfg: &DekuConfig,
    app_id: &str,
) -> Result<Vec<String>> {
    let config_vars = crate::secrets::get_config_vars(pool, cfg, app_id).await?;
    let config_by_key: HashMap<String, String> = config_vars
        .into_iter()
        .map(|entry| (entry.key, entry.value))
        .collect();
    let links = queries::list_service_links_for_app(pool, app_id).await?;
    let mut lines = Vec::new();

    for link in links {
        let service = queries::get_service_by_id(pool, &link.service_id).await?;
        match service.plugin.as_str() {
            "postgres" => {
                lines.push(
                    verify_postgres_binding(
                        docker,
                        &service,
                        &link.env_key,
                        config_by_key.get(&link.env_key),
                    )
                    .await?,
                );
            }
            "redis" => {
                lines.push(
                    verify_redis_binding(
                        docker,
                        &service,
                        &link.env_key,
                        config_by_key.get(&link.env_key),
                    )
                    .await?,
                );
            }
            "mysql" => {
                lines.push(
                    verify_mysql_binding(
                        docker,
                        &service,
                        &link.env_key,
                        config_by_key.get(&link.env_key),
                    )
                    .await?,
                );
            }
            "mariadb" => {
                lines.push(
                    verify_mariadb_binding(
                        docker,
                        &service,
                        &link.env_key,
                        config_by_key.get(&link.env_key),
                    )
                    .await?,
                );
            }
            "mongodb" => {
                lines.push(
                    verify_mongodb_binding(
                        docker,
                        &service,
                        &link.env_key,
                        config_by_key.get(&link.env_key),
                    )
                    .await?,
                );
            }
            _ => {}
        }
    }

    Ok(lines)
}

async fn exec_in_container(
    docker: &Docker,
    container_name: &str,
    cmd: Vec<String>,
    env: Vec<String>,
    stdin: Option<Vec<u8>>,
    user: Option<String>,
) -> Result<ExecResult> {
    let exec = docker
        .create_exec(
            container_name,
            CreateExecOptions {
                attach_stdin: Some(stdin.is_some()),
                attach_stdout: Some(true),
                attach_stderr: Some(true),
                cmd: Some(cmd),
                env: if env.is_empty() { None } else { Some(env) },
                user,
                ..Default::default()
            },
        )
        .await?;

    let mut stdout = Vec::new();
    let mut stderr = Vec::new();

    match docker
        .start_exec(
            &exec.id,
            Some(StartExecOptions {
                detach: false,
                tty: false,
                output_capacity: Some(64 * 1024),
            }),
        )
        .await?
    {
        StartExecResults::Attached {
            mut output,
            mut input,
        } => {
            if let Some(bytes) = stdin {
                input.write_all(&bytes).await?;
            }
            input.shutdown().await?;

            while let Some(message) = output.next().await {
                match message? {
                    LogOutput::StdOut { message } | LogOutput::Console { message } => {
                        stdout.extend_from_slice(&message);
                    }
                    LogOutput::StdErr { message } => {
                        stderr.extend_from_slice(&message);
                    }
                    LogOutput::StdIn { .. } => {}
                }
            }
        }
        StartExecResults::Detached => {
            return Err(anyhow!(
                "unexpected detached exec for container '{container_name}'"
            ));
        }
    }

    let inspect = docker.inspect_exec(&exec.id).await?;
    Ok(ExecResult {
        stdout,
        stderr,
        exit_code: inspect.exit_code.unwrap_or(-1),
    })
}

async fn run_helper_container(
    docker: &Docker,
    image: &str,
    cmd: Vec<String>,
    mounts: Vec<Mount>,
) -> Result<ExecResult> {
    let container_name = format!("deku-helper-{}", Uuid::new_v4().simple());
    let body = bollard::models::ContainerCreateBody {
        image: Some(image.to_string()),
        cmd: Some(cmd),
        host_config: Some(HostConfig {
            mounts: Some(mounts),
            ..Default::default()
        }),
        ..Default::default()
    };

    let created = docker
        .create_container(
            Some(
                CreateContainerOptionsBuilder::default()
                    .name(&container_name)
                    .build(),
            ),
            body,
        )
        .await?;

    docker
        .start_container(
            &created.id,
            None::<bollard::query_parameters::StartContainerOptions>,
        )
        .await?;

    let wait_opts = WaitContainerOptionsBuilder::default().build();
    let mut wait_stream = docker.wait_container(&created.id, Some(wait_opts));
    let wait_result = wait_stream.next().await;

    let log_opts = LogsOptionsBuilder::default()
        .stdout(true)
        .stderr(true)
        .build();
    let mut log_stream = docker.logs(&created.id, Some(log_opts));
    let mut stdout = Vec::new();
    let mut stderr = Vec::new();
    while let Some(item) = log_stream.next().await {
        match item? {
            LogOutput::StdOut { message } | LogOutput::Console { message } => {
                stdout.extend_from_slice(&message);
            }
            LogOutput::StdErr { message } => stderr.extend_from_slice(&message),
            LogOutput::StdIn { .. } => {}
        }
    }

    let _ = docker
        .remove_container(
            &created.id,
            Some(RemoveContainerOptionsBuilder::default().force(true).build()),
        )
        .await;

    let exit_code = wait_result
        .and_then(|r| r.ok())
        .map(|r| r.status_code)
        .unwrap_or(1);
    if exit_code != 0 {
        return Err(anyhow!(
            "helper container failed with exit code {}: {}",
            exit_code,
            String::from_utf8_lossy(&stderr)
        ));
    }

    Ok(ExecResult {
        stdout,
        stderr,
        exit_code,
    })
}

async fn verify_postgres_binding(
    docker: &Docker,
    service: &queries::Service,
    env_key: &str,
    actual_config: Option<&String>,
) -> Result<String> {
    let connection = connection_info(&postgres_spec(), service)?;
    let expected_url = connection.url.clone();
    let config_state = match actual_config {
        Some(value) if value == &expected_url => "config ok",
        Some(_) => "config mismatch",
        None => "config missing",
    };

    let container_name = format!("deku-postgres-{}", service.name);
    let check = exec_in_container(
        docker,
        &container_name,
        vec![
            "pg_isready".to_string(),
            "-U".to_string(),
            connection
                .username
                .clone()
                .unwrap_or_else(|| "deku".to_string()),
            "-d".to_string(),
            connection
                .database
                .clone()
                .unwrap_or_else(|| service.name.clone()),
        ],
        vec![format!("PGPASSWORD={}", connection.password)],
        None,
        Some("postgres".to_string()),
    )
    .await?;

    let connectivity = if check.exit_code == 0 {
        "reachable"
    } else {
        "unreachable"
    };

    Ok(format!(
        "linked postgres '{}' verified: {} via {}, {}",
        service.name, env_key, config_state, connectivity
    ))
}

async fn verify_redis_binding(
    docker: &Docker,
    service: &queries::Service,
    env_key: &str,
    actual_config: Option<&String>,
) -> Result<String> {
    let connection = connection_info(&redis_spec(), service)?;
    let expected_url = connection.url.clone();
    let config_state = match actual_config {
        Some(value) if value == &expected_url => "config ok",
        Some(_) => "config mismatch",
        None => "config missing",
    };

    let container_name = format!("deku-redis-{}", service.name);
    let check = exec_in_container(
        docker,
        &container_name,
        vec!["redis-cli".to_string(), "PING".to_string()],
        vec![format!("REDISCLI_AUTH={}", connection.password)],
        None,
        None,
    )
    .await?;
    let connectivity =
        if check.exit_code == 0 && String::from_utf8_lossy(&check.stdout).contains("PONG") {
            "reachable"
        } else {
            "unreachable"
        };

    Ok(format!(
        "linked redis '{}' verified: {} via {}, {}",
        service.name, env_key, config_state, connectivity
    ))
}

/// SQL service connectivity check shared by MySQL and MariaDB.
async fn verify_sql_binding(
    docker: &Docker,
    service: &queries::Service,
    env_key: &str,
    actual_config: Option<&String>,
    spec: &DbServiceSpec,
    admin_binary: &str,
) -> Result<String> {
    let connection = connection_info(spec, service)?;
    let expected_url = connection.url.clone();
    let config_state = match actual_config {
        Some(value) if value == &expected_url => "config ok",
        Some(_) => "config mismatch",
        None => "config missing",
    };

    let container_name = format!("deku-{}-{}", spec.plugin, service.name);
    let check = exec_in_container(
        docker,
        &container_name,
        vec![
            admin_binary.to_string(),
            "ping".to_string(),
            "-u".to_string(),
            connection
                .username
                .clone()
                .unwrap_or_else(|| "deku".to_string()),
            format!("-p{}", connection.password),
        ],
        Vec::new(),
        None,
        None,
    )
    .await?;
    let connectivity = if check.exit_code == 0 {
        "reachable"
    } else {
        "unreachable"
    };

    Ok(format!(
        "linked {} '{}' verified: {} via {}, {}",
        spec.plugin, service.name, env_key, config_state, connectivity
    ))
}

async fn verify_mysql_binding(
    docker: &Docker,
    service: &queries::Service,
    env_key: &str,
    actual_config: Option<&String>,
) -> Result<String> {
    verify_sql_binding(
        docker,
        service,
        env_key,
        actual_config,
        &mysql_spec(),
        "mysqladmin",
    )
    .await
}

async fn verify_mariadb_binding(
    docker: &Docker,
    service: &queries::Service,
    env_key: &str,
    actual_config: Option<&String>,
) -> Result<String> {
    verify_sql_binding(
        docker,
        service,
        env_key,
        actual_config,
        &mariadb_spec(),
        "mariadb-admin",
    )
    .await
}

async fn verify_mongodb_binding(
    docker: &Docker,
    service: &queries::Service,
    env_key: &str,
    actual_config: Option<&String>,
) -> Result<String> {
    let spec = mongodb_spec();
    let connection = connection_info(&spec, service)?;
    let expected_url = connection.url.clone();
    let config_state = match actual_config {
        Some(value) if value == &expected_url => "config ok",
        Some(_) => "config mismatch",
        None => "config missing",
    };

    let container_name = format!("deku-mongodb-{}", service.name);
    let check = exec_in_container(
        docker,
        &container_name,
        vec![
            "mongosh".to_string(),
            internal_mongo_uri(&connection, &spec),
            "--quiet".to_string(),
            "--eval".to_string(),
            "db.runCommand({ping:1}).ok".to_string(),
        ],
        Vec::new(),
        None,
        None,
    )
    .await?;
    let connectivity = if check.exit_code == 0 {
        "reachable"
    } else {
        "unreachable"
    };

    Ok(format!(
        "linked mongodb '{}' verified: {} via {}, {}",
        service.name, env_key, config_state, connectivity
    ))
}

pub async fn unlink(pool: &SqlitePool, service_name: &str, app_name: &str) -> Result<()> {
    let svc = queries::get_service(pool, service_name).await?;
    let app = queries::get_app(pool, app_name).await?;
    if let Some(link) = queries::get_service_link(pool, &svc.id, &app.id).await? {
        let _ = queries::unset_config_var(pool, &app.id, &link.env_key).await;
        queries::unlink_service(pool, &svc.id, &app.id).await?;
    }
    Ok(())
}

pub async fn get_logs(docker: &Docker, plugin: &str, name: &str, n: usize) -> Result<Vec<String>> {
    let container_name = format!("deku-{plugin}-{name}");
    crate::container::get_container_logs(docker, &container_name, n).await
}

#[cfg(test)]
mod tests {
    use super::{open_backup, parse_service_config, redis_spec, seal_backup, ServiceConfig};
    use crate::config::{DekuConfig, EncryptionConfig};

    fn cfg_with_key() -> DekuConfig {
        DekuConfig {
            encryption: Some(EncryptionConfig {
                key: Some("ab".repeat(32)),
                key_file: None,
            }),
            ..DekuConfig::default()
        }
    }

    #[test]
    fn backup_payloads_round_trip_through_the_shared_key() {
        let cfg = cfg_with_key();
        let dump = b"-- pg_dump output".to_vec();

        let (sealed, label) = seal_backup(&cfg, dump.clone()).expect("seal");
        assert_eq!(label, crate::crypto::ENCRYPTION_LABEL_AES256_GCM);
        assert_ne!(sealed, dump, "the stored payload must not be the dump");
        assert_eq!(open_backup(&cfg, sealed).expect("open"), dump);
    }

    #[test]
    fn unencrypted_backups_still_open() {
        let cfg = cfg_with_key();
        let plain = b"-- pg_dump output".to_vec();
        let (stored, label) = seal_backup(&DekuConfig::default(), plain.clone()).expect("seal");
        assert_eq!(label, crate::crypto::ENCRYPTION_LABEL_NONE);
        assert_eq!(stored, plain);
        // A payload written before encryption existed must restore under a key.
        assert_eq!(open_backup(&cfg, stored).expect("open"), plain);
    }

    #[test]
    fn an_encrypted_backup_needs_the_key() {
        let (sealed, _) = seal_backup(&cfg_with_key(), b"dump".to_vec()).expect("seal");
        let error = open_backup(&DekuConfig::default(), sealed)
            .expect_err("encrypted backup must not open without a key");
        assert!(error.to_string().contains("no key is configured"));
    }

    #[test]
    fn parses_service_config_json() {
        let raw = r#"{"password":"secret","host_port":5432,"name":"app-db","volume":"deku-postgres-app-db-data"}"#;
        let parsed = parse_service_config(raw).expect("service config should parse");
        assert_eq!(
            parsed,
            ServiceConfig {
                password: "secret".to_string(),
                host_port: 5432,
                name: "app-db".to_string(),
                volume: "deku-postgres-app-db-data".to_string(),
            }
        );
    }

    #[test]
    fn redis_url_contains_password() {
        let spec = redis_spec();
        let url = (spec.make_url)("secret", "127.0.0.1", 6379, "cache");
        assert_eq!(url, "redis://:secret@127.0.0.1:6379");
    }
}
