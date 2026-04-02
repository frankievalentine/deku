use anyhow::Result;
use bollard::models::{NetworkConnectRequest, NetworkCreateRequest, NetworkDisconnectRequest};
use bollard::Docker;
use sqlx::SqlitePool;

use crate::db::queries;

pub async fn create(pool: &SqlitePool, docker: &Docker, name: &str) -> Result<queries::Network> {
    let docker_name = format!("deku-{name}");
    docker
        .create_network(NetworkCreateRequest {
            name: docker_name,
            ..Default::default()
        })
        .await?;
    queries::create_network(pool, name).await.map_err(Into::into)
}

pub async fn destroy(pool: &SqlitePool, docker: &Docker, name: &str) -> Result<()> {
    let docker_name = format!("deku-{name}");
    let _ = docker.remove_network(&docker_name).await;
    queries::delete_network(pool, name).await.map_err(Into::into)
}

pub async fn attach(
    pool: &SqlitePool,
    docker: &Docker,
    app_name: &str,
    network_name: &str,
) -> Result<()> {
    let app = queries::get_app(pool, app_name).await?;
    let network = queries::get_network(pool, network_name).await?;
    let containers = queries::list_containers_for_app(pool, &app.id).await?;
    let docker_net = format!("deku-{network_name}");
    for c in &containers {
        let _ = docker
            .connect_network(
                &docker_net,
                NetworkConnectRequest {
                    container: c.id.clone(),
                    endpoint_config: None,
                },
            )
            .await;
    }
    queries::attach_app_to_network(pool, &app.id, &network.id, "deploy")
        .await
        .map_err(Into::into)
}

pub async fn detach(
    pool: &SqlitePool,
    docker: &Docker,
    app_name: &str,
    network_name: &str,
) -> Result<()> {
    let app = queries::get_app(pool, app_name).await?;
    let network = queries::get_network(pool, network_name).await?;
    let containers = queries::list_containers_for_app(pool, &app.id).await?;
    let docker_net = format!("deku-{network_name}");
    for c in &containers {
        let _ = docker
            .disconnect_network(
                &docker_net,
                NetworkDisconnectRequest {
                    container: c.id.clone(),
                    force: Some(true),
                },
            )
            .await;
    }
    queries::detach_app_from_network(pool, &app.id, &network.id)
        .await
        .map_err(Into::into)
}
