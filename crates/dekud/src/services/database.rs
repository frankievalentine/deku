use std::collections::HashMap;

use anyhow::{anyhow, Result};
use bollard::container::LogOutput;
use bollard::exec::{CreateExecOptions, StartExecOptions, StartExecResults};
use bollard::models::{HostConfig, Mount, MountTypeEnum, PortBinding};
use bollard::query_parameters::{
    CreateContainerOptionsBuilder, CreateImageOptionsBuilder, RemoveContainerOptionsBuilder,
    RemoveVolumeOptionsBuilder,
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
    while let Some(_) = stream.next().await {}

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
                typ: Some(MountTypeEnum::VOLUME),
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
    queries::set_config_var(pool, &app.id, spec.env_key, &url, false).await?;
    queries::link_service(pool, &svc.id, &app.id, spec.env_key).await?;
    Ok(())
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
    crate::objectstore::put_bytes(object_store, &object_key, exec.stdout.clone()).await?;

    let sha256 = hex::encode(Sha256::digest(&exec.stdout));
    queries::create_service_backup(
        pool,
        &service.id,
        &object_key,
        "postgres.sql",
        exec.stdout.len() as i64,
        &sha256,
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
        exit_code: inspect.exit_code.unwrap_or(-1) as i64,
    })
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
    use super::{parse_service_config, redis_spec, ServiceConfig};

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
