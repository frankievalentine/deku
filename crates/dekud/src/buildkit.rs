use anyhow::{Context, Result};
use bollard::models::{HostConfig, Mount, MountType};
use bollard::query_parameters::{CreateContainerOptionsBuilder, InspectContainerOptions};
use std::collections::HashMap;

use crate::config::{BuildkitConfig, DekuConfig};
use crate::container::{self, DockerClient};

const BUILDKIT_CACHE_VOLUME: &str = "deku-buildkit-cache";

pub fn railpack_bin() -> String {
    if deku_core::dev_hooks::enabled() {
        if let Ok(value) = std::env::var("DEKU_RAILPACK_BIN") {
            return value;
        }
    }
    "railpack".to_string()
}

pub async fn resolve_host(docker: &DockerClient, cfg: &DekuConfig) -> Result<String> {
    let buildkit = cfg.buildkit.clone().unwrap_or_default();

    if let Some(host) = buildkit.host.as_deref() {
        if !host.trim().is_empty() {
            return Ok(host.trim().to_string());
        }
    }

    if !buildkit.managed {
        anyhow::bail!(
            "Railpack requires BuildKit, but buildkit.managed is false and no buildkit.host is set"
        );
    }

    ensure_container(docker, &buildkit).await?;

    Ok(format!("docker-container://{}", buildkit.container_name))
}

async fn ensure_container(docker: &DockerClient, buildkit: &BuildkitConfig) -> Result<()> {
    let name = buildkit.container_name.as_str();

    match docker
        .inspect_container(name, None::<InspectContainerOptions>)
        .await
    {
        Ok(info) => {
            let running = info.state.and_then(|state| state.running).unwrap_or(false);
            if running {
                return Ok(());
            }
            docker
                .start_container(
                    name,
                    None::<bollard::query_parameters::StartContainerOptions>,
                )
                .await
                .with_context(|| format!("starting BuildKit container '{name}'"))?;
            Ok(())
        }
        Err(_) => {
            if !container::image_exists(docker, &buildkit.image).await {
                container::pull_image(docker, &buildkit.image, |_| {})
                    .await
                    .with_context(|| format!("pulling BuildKit image '{}'", buildkit.image))?;
            }

            let body = bollard::models::ContainerCreateBody {
                image: Some(buildkit.image.clone()),
                labels: Some(HashMap::from([(
                    "deku.managed".to_string(),
                    "true".to_string(),
                )])),
                host_config: Some(HostConfig {
                    privileged: Some(true),
                    mounts: Some(vec![Mount {
                        typ: Some(MountType::VOLUME),
                        source: Some(BUILDKIT_CACHE_VOLUME.to_string()),
                        target: Some("/var/lib/buildkit".to_string()),
                        ..Default::default()
                    }]),
                    ..Default::default()
                }),
                ..Default::default()
            };
            let options = CreateContainerOptionsBuilder::default().name(name).build();
            docker
                .create_container(Some(options), body)
                .await
                .with_context(|| format!("creating BuildKit container '{name}'"))?;
            docker
                .start_container(
                    name,
                    None::<bollard::query_parameters::StartContainerOptions>,
                )
                .await
                .with_context(|| format!("starting BuildKit container '{name}'"))?;
            Ok(())
        }
    }
}
