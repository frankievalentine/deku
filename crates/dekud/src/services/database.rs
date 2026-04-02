use std::collections::HashMap;

use anyhow::{anyhow, Result};
use bollard::models::{HostConfig, PortBinding};
use bollard::query_parameters::{
    CreateContainerOptionsBuilder, CreateImageOptionsBuilder, RemoveContainerOptionsBuilder,
};
use bollard::Docker;
use futures::StreamExt;
use sqlx::SqlitePool;

use crate::db::queries;

pub struct DbServiceSpec {
    pub plugin: &'static str,
    pub image: &'static str,
    pub container_port: u16,
    pub env_key: &'static str,
    pub make_env: fn(name: &str, password: &str) -> Vec<String>,
    pub make_url: fn(password: &str, host: &str, port: u16, name: &str) -> String,
}

pub fn postgres_spec() -> DbServiceSpec {
    DbServiceSpec {
        plugin: "postgres",
        image: "postgres:16-alpine",
        container_port: 5432,
        env_key: "DATABASE_URL",
        make_env: |name, password| {
            vec![
                "POSTGRES_USER=deku".into(),
                format!("POSTGRES_PASSWORD={password}"),
                format!("POSTGRES_DB={name}"),
            ]
        },
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
        make_env: |_name, _password| vec![],
        make_url: |_password, host, port, _name| format!("redis://{host}:{port}"),
    }
}

pub fn mysql_spec() -> DbServiceSpec {
    DbServiceSpec {
        plugin: "mysql",
        image: "mysql:8",
        container_port: 3306,
        env_key: "DATABASE_URL",
        make_env: |name, password| {
            vec![
                format!("MYSQL_ROOT_PASSWORD={password}"),
                format!("MYSQL_DATABASE={name}"),
                "MYSQL_USER=deku".into(),
                format!("MYSQL_PASSWORD={password}"),
            ]
        },
        make_url: |password, host, port, name| {
            format!("mysql://deku:{password}@{host}:{port}/{name}")
        },
    }
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

    let container_name = format!("deku-{}-{name}", spec.plugin);

    let body = bollard::models::ContainerCreateBody {
        image: Some(spec.image.to_string()),
        env: Some(envs),
        exposed_ports: Some(exposed),
        host_config: Some(HostConfig {
            port_bindings: Some(bindings),
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

    let config = format!(
        r#"{{"password":"{}","host_port":{},"name":"{}"}}"#,
        password, host_port, name
    );

    let svc = queries::create_service(pool, name, spec.plugin, Some(&container_id), &config).await?;
    Ok(svc)
}

pub async fn destroy(pool: &SqlitePool, docker: &Docker, name: &str) -> Result<()> {
    let svc = queries::get_service(pool, name).await?;
    let plugin = svc.plugin.clone();
    let links = queries::delete_service(pool, name).await?;

    for link in links {
        let _ = queries::unset_config_var(pool, &link.app_id, &link.env_key).await;
    }

    let container_name = format!("deku-{plugin}-{name}");
    let _ = docker
        .stop_container(&container_name, None::<bollard::query_parameters::StopContainerOptions>)
        .await;
    let _ = docker
        .remove_container(
            &container_name,
            Some(RemoveContainerOptionsBuilder::default().force(true).build()),
        )
        .await;

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

    // Parse config JSON manually
    let config = &svc.config;
    let password = extract_json_str(config, "password")
        .ok_or_else(|| anyhow!("missing password in service config"))?;
    let host_port: u16 = extract_json_u64(config, "host_port")
        .ok_or_else(|| anyhow!("missing host_port in service config"))? as u16;
    let db_name = extract_json_str(config, "name")
        .ok_or_else(|| anyhow!("missing name in service config"))?;

    let url = (spec.make_url)(&password, "127.0.0.1", host_port, &db_name);
    queries::set_config_var(pool, &app.id, spec.env_key, &url, false).await?;
    queries::link_service(pool, &svc.id, &app.id, spec.env_key).await?;
    Ok(())
}

pub async fn unlink(
    pool: &SqlitePool,
    service_name: &str,
    app_name: &str,
) -> Result<()> {
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

// Minimal JSON string field extractor — avoids pulling in serde_json at this level.
fn extract_json_str(json: &str, key: &str) -> Option<String> {
    let search = format!("\"{key}\":\"");
    let start = json.find(&search)? + search.len();
    let rest = &json[start..];
    let end = rest.find('"')?;
    Some(rest[..end].to_string())
}

fn extract_json_u64(json: &str, key: &str) -> Option<u64> {
    let search = format!("\"{key}\":");
    let start = json.find(&search)? + search.len();
    let rest = json[start..].trim_start();
    let end = rest.find(|c: char| !c.is_ascii_digit()).unwrap_or(rest.len());
    rest[..end].parse().ok()
}
