use std::collections::HashMap;

use deku_core::types::{
    AppStatus, BuilderType, DekuToml, DeployStatus, ObjectStoreConfig, ProcfileEntry,
};
use deku_plugin_sdk::context::BuildContext;
use deku_plugin_sdk::context::DeployContext;
use sqlx::SqlitePool;

use crate::build::{select_builder, BuiltImage};
use crate::config::DekuConfig;
use crate::container::{self, ContainerSpec, DockerClient};
use crate::db::queries;
use crate::events::EventSender;
use crate::proxy;
use crate::services::{database, network};

// ── Deploy request types ──────────────────────────────────────────────────────

#[derive(Debug, Clone)]
pub enum DeploySource {
    /// Build from a source directory
    Source { path: std::path::PathBuf },
    /// Deploy a pre-built image directly
    Image { reference: String },
    /// Build from a tar archive (path or URL)
    Archive { path: String },
}

#[derive(Debug, Clone)]
pub struct DeployRequest {
    pub app_id: String,
    pub app_name: String,
    pub source: DeploySource,
    pub force_builder: Option<String>,
}

// ── Health check ──────────────────────────────────────────────────────────────

async fn run_health_check(host_port: u16, path: &str, timeout_secs: u64, attempts: u32) -> bool {
    let url = format!("http://127.0.0.1:{host_port}{path}");

    for attempt in 1..=attempts {
        tracing::debug!(%url, attempt, "health check");
        let result = tokio::time::timeout(
            std::time::Duration::from_secs(timeout_secs),
            reqwest::get(&url),
        )
        .await;

        match result {
            Ok(Ok(resp)) if resp.status().is_success() || resp.status().as_u16() < 400 => {
                tracing::info!(%url, "health check passed");
                return true;
            }
            _ => {
                if attempt < attempts {
                    tokio::time::sleep(std::time::Duration::from_secs(2)).await;
                }
            }
        }
    }

    false
}

// ── Port allocation ───────────────────────────────────────────────────────────

/// Find a free TCP port on localhost.
fn find_free_port() -> anyhow::Result<u16> {
    let listener = std::net::TcpListener::bind("127.0.0.1:0")?;
    let port = listener.local_addr()?.port();
    Ok(port)
}

fn release_artifact_key(
    object_store: &ObjectStoreConfig,
    app_name: &str,
    deploy_id: &str,
    file_name: &str,
) -> String {
    let prefix = object_store.normalized_prefix().unwrap_or_default();
    format!("{prefix}releases/{app_name}/{deploy_id}/{file_name}")
}

fn emit_build_note(events: &EventSender, app_id: &str, line: String) {
    events.emit(
        Some(app_id.to_string()),
        "build.log",
        Some(serde_json::json!({ "line": line })),
    );
}

async fn retain_release_artifact(
    events: &EventSender,
    app_id: &str,
    object_store: &ObjectStoreConfig,
    app_name: &str,
    deploy_id: &str,
    file_name: &str,
    payload: Vec<u8>,
) {
    let key = release_artifact_key(object_store, app_name, deploy_id, file_name);
    match crate::objectstore::put_bytes(object_store, &key, payload).await {
        Ok(()) => emit_build_note(
            events,
            app_id,
            format!(
                "Retained deploy artifact in object store: s3://{}/{}",
                object_store.bucket, key
            ),
        ),
        Err(error) => {
            tracing::warn!(
                app = app_name,
                deploy = deploy_id,
                "artifact retention skipped: {error}"
            );
            emit_build_note(
                events,
                app_id,
                format!("Deploy artifact retention skipped: {error}"),
            );
        }
    }
}

// ── Core deploy function ──────────────────────────────────────────────────────

/// Run the full deploy pipeline for an app. Returns the deployment ID on success.
#[allow(clippy::too_many_lines)]
pub async fn run_deploy(
    pool: &SqlitePool,
    docker: &DockerClient,
    events: &EventSender,
    cfg: &DekuConfig,
    plugins: &crate::plugins::PluginRegistry,
    req: DeployRequest,
) -> anyhow::Result<String> {
    let app_id = &req.app_id;
    let app_name = &req.app_name;

    // Determine builder type for record keeping
    let builder_type = match &req.source {
        DeploySource::Image { .. } => BuilderType::Image,
        DeploySource::Archive { .. } => BuilderType::Archive,
        DeploySource::Source { .. } => match req.force_builder.as_deref() {
            Some("nixpacks") => BuilderType::Nixpacks,
            Some("pack") => BuilderType::Pack,
            Some("compose") => BuilderType::Compose,
            _ => BuilderType::Dockerfile,
        },
    };

    // Step 1: Create deployment record
    let mut deployment = queries::create_deployment(pool, app_id, builder_type).await?;
    let deploy_id = deployment.id.clone();

    events.emit(
        Some(app_id.clone()),
        "deploy.started",
        Some(serde_json::json!({
            "deploy_id": deploy_id,
            "app": app_name,
        })),
    );

    // Step 2: Mark app as deploying
    queries::update_app_status(pool, app_id, AppStatus::Created).await?;

    // Wrap the rest in a closure so we can always update deployment status on failure
    let result = do_deploy(pool, docker, events, cfg, plugins, &req, &mut deployment).await;

    match result {
        Ok(ref _image_tag) => {
            queries::update_app_status(pool, app_id, AppStatus::Deployed).await?;
            events.emit(
                Some(app_id.clone()),
                "deploy.complete",
                Some(serde_json::json!({ "deploy_id": deploy_id, "app": app_name })),
            );
        }
        Err(ref e) => {
            let _ = queries::update_deployment(pool, &deploy_id, DeployStatus::Failed, None).await;
            queries::update_app_status(pool, app_id, AppStatus::Error).await?;
            events.emit(
                Some(app_id.clone()),
                "deploy.failed",
                Some(serde_json::json!({ "deploy_id": deploy_id, "error": e.to_string() })),
            );
            return Err(anyhow::anyhow!("{e}"));
        }
    }

    Ok(deploy_id)
}

#[allow(clippy::too_many_lines)]
async fn do_deploy(
    pool: &SqlitePool,
    docker: &DockerClient,
    events: &EventSender,
    cfg: &DekuConfig,
    plugins: &crate::plugins::PluginRegistry,
    req: &DeployRequest,
    deployment: &mut deku_core::types::Deployment,
) -> anyhow::Result<String> {
    let app_id = &req.app_id;
    let app_name = &req.app_name;
    let deploy_id = &deployment.id;
    let app = queries::get_app_by_id(pool, app_id).await?;

    // ── Build phase ───────────────────────────────────────────────────────────

    queries::update_deployment(pool, deploy_id, DeployStatus::Building, None).await?;

    let (built, deku_toml) = match &req.source {
        DeploySource::Image { reference } => {
            // Pull the image and wrap it
            events.emit(
                Some(app_id.clone()),
                "build.log",
                Some(serde_json::json!({ "line": format!("Pulling image {reference}...") })),
            );

            let ref_clone = reference.clone();
            let events_clone = events.clone();
            let app_id_clone = app_id.clone();

            container::pull_image(docker, reference, |status| {
                events_clone.emit(
                    Some(app_id_clone.clone()),
                    "build.log",
                    Some(serde_json::json!({ "line": status })),
                );
            })
            .await?;

            // Tag as deku image
            let image_tag = format!("deku/{app_name}:latest");
            let (repo_part, tag_part) = ref_clone
                .split_once(':')
                .unwrap_or((ref_clone.as_str(), "latest"));

            // Re-tag to our naming scheme
            container::tag_image(
                docker,
                &format!("{repo_part}:{tag_part}"),
                &format!("deku/{app_name}"),
                "latest",
            )
            .await?;

            let exposed_ports: Vec<u16> = vec![];

            (
                BuiltImage {
                    tag: image_tag,
                    exposed_ports,
                    procfile: vec![],
                },
                None,
            )
        }

        DeploySource::Archive { path } => {
            // Download/extract to temp dir, then run source build
            let tmp_dir = tempfile::tempdir()?;
            let archive_bytes = if path.starts_with("http://") || path.starts_with("https://") {
                reqwest::get(path.as_str()).await?.bytes().await?.to_vec()
            } else {
                std::fs::read(path)?
            };

            if let Some(object_store) = cfg.object_store.as_ref() {
                retain_release_artifact(
                    events,
                    app_id,
                    object_store,
                    app_name,
                    deploy_id,
                    "source-archive.tar.gz",
                    archive_bytes.clone(),
                )
                .await;
            }

            // Decompress tar.gz into tmp_dir
            let cursor = std::io::Cursor::new(archive_bytes);
            let decoder = flate2::read::GzDecoder::new(cursor);
            let mut tar_archive = tar::Archive::new(decoder);
            tar_archive.unpack(tmp_dir.path())?;

            let source_path = tmp_dir.path().to_path_buf();
            let deku_toml = load_deku_toml(&source_path);

            let ctx = BuildContext {
                app: app.clone(),
                source_dir: source_path.clone(),
                data_dir: cfg.data_dir.clone(),
            };

            let builder = select_builder(&source_path, req.force_builder.as_deref());
            tracing::info!(app = app_name, builder = builder.name(), "selected builder");
            plugins.run_pre_build(&ctx).await;
            let built = builder
                .build(&ctx, deku_toml.as_ref(), docker, events)
                .await
                .map_err(|e| anyhow::anyhow!("{e}"))?;
            plugins.run_post_build(&ctx).await;
            (built, deku_toml)
        }

        DeploySource::Source { path } => {
            if let Some(object_store) = cfg.object_store.as_ref() {
                match container::create_tar_gz(path) {
                    Ok(archive) => {
                        retain_release_artifact(
                            events,
                            app_id,
                            object_store,
                            app_name,
                            deploy_id,
                            "source.tar.gz",
                            archive.to_vec(),
                        )
                        .await;
                    }
                    Err(error) => {
                        tracing::warn!(
                            app = app_name,
                            deploy = deploy_id,
                            "artifact retention skipped: {error}"
                        );
                        emit_build_note(
                            events,
                            app_id,
                            format!("Deploy artifact retention skipped: {error}"),
                        );
                    }
                }
            }

            let deku_toml = load_deku_toml(path);
            let ctx = BuildContext {
                app: app.clone(),
                source_dir: path.clone(),
                data_dir: cfg.data_dir.clone(),
            };
            let forced = req.force_builder.as_deref().or_else(|| {
                deku_toml
                    .as_ref()
                    .and_then(|t| t.build.as_ref())
                    .and_then(|b| b.builder.as_deref())
            });

            let builder = select_builder(path, forced);
            tracing::info!(app = app_name, builder = builder.name(), "selected builder");
            plugins.run_pre_build(&ctx).await;
            let built = builder
                .build(&ctx, deku_toml.as_ref(), docker, events)
                .await
                .map_err(|e| anyhow::anyhow!("{e}"))?;
            plugins.run_post_build(&ctx).await;
            (built, deku_toml)
        }
    };

    let deploy_cfg = deku_toml.as_ref().and_then(|cfg| cfg.deploy.as_ref());
    let web_container_port = deploy_cfg
        .and_then(|cfg| cfg.port)
        .or_else(|| built.exposed_ports.first().copied())
        .unwrap_or(3000);
    let health_path = deploy_cfg
        .and_then(|cfg| cfg.healthcheck.clone())
        .filter(|path| !path.trim().is_empty())
        .unwrap_or_else(|| "/".to_string());
    let health_wait = deploy_cfg.and_then(|cfg| cfg.wait).unwrap_or(5);
    let health_timeout = deploy_cfg.and_then(|cfg| cfg.timeout).unwrap_or(30);
    let health_attempts = deploy_cfg.and_then(|cfg| cfg.attempts).unwrap_or(5);
    let retire_secs = deploy_cfg.and_then(|cfg| cfg.retire).unwrap_or(60);

    queries::update_deployment(pool, deploy_id, DeployStatus::Built, Some(&built.tag)).await?;

    // ── Determine process types ────────────────────────────────────────────────

    let process_entries = if built.procfile.is_empty() {
        vec![ProcfileEntry {
            process_type: "web".to_string(),
            command: String::new(), // use image CMD/ENTRYPOINT
        }]
    } else {
        built.procfile.clone()
    };

    // Filter out `release:` entry (run separately)
    let release_entry = process_entries
        .iter()
        .find(|e| e.process_type == "release")
        .cloned();

    let run_entries: Vec<&ProcfileEntry> = process_entries
        .iter()
        .filter(|e| e.process_type != "release")
        .collect();

    // ── Release phase ─────────────────────────────────────────────────────────

    if let Some(release) = release_entry {
        events.emit(
            Some(app_id.clone()),
            "deploy.release",
            Some(serde_json::json!({ "command": release.command })),
        );
        run_release_phase(pool, docker, events, app_id, &built.tag, &release.command).await?;
    }

    // ── Deploy phase ──────────────────────────────────────────────────────────

    queries::update_deployment(pool, deploy_id, DeployStatus::Deploying, Some(&built.tag)).await?;
    deployment.status = DeployStatus::Deploying;
    deployment.image_tag = Some(built.tag.clone());
    plugins
        .run_pre_deploy(&DeployContext {
            app: app.clone(),
            deployment: deployment.clone(),
            data_dir: cfg.data_dir.clone(),
        })
        .await;

    // Collect previous containers for retirement
    let previous_containers = queries::list_containers_for_app(pool, app_id).await?;

    // Load per-app config
    let config_vars = queries::get_config_vars(pool, app_id).await?;
    let env: Vec<String> = config_vars
        .iter()
        .map(|cv| format!("{}={}", cv.key, cv.value))
        .collect();

    let storage_mounts = queries::list_storage_mounts(pool, app_id).await?;
    let volumes: Vec<(String, String)> = storage_mounts
        .iter()
        .map(|m| (m.host_path.clone(), m.container_path.clone()))
        .collect();

    let scales = queries::get_process_scales(pool, app_id).await?;

    let mut new_container_ids = Vec::new();
    let mut web_host_port: Option<u16> = None;

    for entry in &run_entries {
        let proc_type = &entry.process_type;
        let scale = *scales.get(proc_type.as_str()).unwrap_or(&1) as usize;
        let is_web = proc_type == "web";

        for i in 0..scale {
            let host_port = if is_web {
                let p = find_free_port()?;
                if i == 0 {
                    web_host_port = Some(p);
                }
                p
            } else {
                0 // non-web processes don't need host port binding
            };

            let container_name = format!("deku.{app_name}.{proc_type}.{}-{i}", &deploy_id[..8]);

            let mut port_bindings = HashMap::new();
            if is_web && host_port > 0 {
                port_bindings.insert(format!("{web_container_port}/tcp"), host_port);
            }

            let cmd = if entry.command.is_empty() {
                None
            } else {
                Some(vec![
                    "sh".to_string(),
                    "-c".to_string(),
                    entry.command.clone(),
                ])
            };

            let resource_limits = queries::get_resource_limits(pool, app_id, proc_type).await?;
            let memory = resource_limits
                .as_ref()
                .and_then(|rl| rl.memory.as_deref())
                .and_then(parse_memory_bytes);
            let cpu_quota = resource_limits
                .as_ref()
                .and_then(|rl| rl.cpu.as_deref())
                .and_then(parse_cpu_quota);

            let spec = ContainerSpec {
                image: &built.tag,
                name: &container_name,
                env: env.clone(),
                port_bindings,
                volumes: volumes.clone(),
                memory,
                cpu_quota,
                cmd,
            };

            let container_id = container::start_container(docker, &spec).await?;

            queries::record_container(
                pool,
                &container_id,
                app_id,
                deploy_id,
                proc_type,
                if is_web && host_port > 0 {
                    Some(host_port as i64)
                } else {
                    None
                },
            )
            .await?;

            let attached_networks = match network::attach_container_to_configured_networks(
                pool,
                docker,
                app_id,
                &container_id,
            )
            .await
            {
                Ok(networks) => networks,
                Err(error) => {
                    let _ = container::stop_container(docker, &container_id, 10).await;
                    let _ = container::remove_container(docker, &container_id).await;
                    let _ = queries::update_container_status(pool, &container_id, "removed").await;
                    return Err(anyhow::anyhow!(
                        "failed to attach '{container_name}' to configured networks: {error}"
                    ));
                }
            };

            if !attached_networks.is_empty() {
                events.emit(
                    Some(app_id.clone()),
                    "deploy.network",
                    Some(serde_json::json!({
                        "container_id": container_id,
                        "process_type": proc_type,
                        "networks": attached_networks,
                    })),
                );
            }

            new_container_ids.push(container_id);
        }
    }

    // ── Health check phase ────────────────────────────────────────────────────

    queries::update_deployment(
        pool,
        deploy_id,
        DeployStatus::HealthChecking,
        Some(&built.tag),
    )
    .await?;

    if let Some(port) = web_host_port {
        events.emit(
            Some(app_id.clone()),
            "deploy.health_checking",
            Some(serde_json::json!({
                "port": port,
                "container_port": web_container_port,
                "path": health_path.clone(),
                "wait": health_wait,
                "timeout": health_timeout,
                "attempts": health_attempts,
            })),
        );

        tokio::time::sleep(std::time::Duration::from_secs(health_wait)).await;

        let healthy = run_health_check(port, &health_path, health_timeout, health_attempts).await;

        if !healthy {
            // Rollback: stop new containers
            events.emit(
                Some(app_id.clone()),
                "deploy.rollback",
                Some(serde_json::json!({ "reason": "health checks failed" })),
            );
            for id in &new_container_ids {
                let _ = container::stop_container(docker, id, 10).await;
                let _ = container::remove_container(docker, id).await;
                let _ = queries::update_container_status(pool, id, "removed").await;
            }
            queries::update_deployment(pool, deploy_id, DeployStatus::Failed, Some(&built.tag))
                .await?;
            return Err(anyhow::anyhow!(
                "health checks failed after {health_attempts} attempts"
            ));
        }

        // Update port mapping in DB
        let _ = queries::upsert_port_mapping(
            pool,
            app_id,
            port as i64,
            web_container_port as i64,
            "tcp",
        )
        .await;

        // Update Angie upstream
        let domains = queries::list_domain_names(pool, app_id).await?;
        let tls_enabled = queries::get_app_by_id(pool, app_id)
            .await
            .map(|app| app.tls_enabled)
            .unwrap_or(false);
        if !domains.is_empty() {
            let upstreams = vec![deku_core::types::Upstream {
                host: "127.0.0.1".to_string(),
                port,
            }];
            if let Err(e) = proxy::apply_app_config(
                &cfg.angie_conf_dir,
                app_name,
                Some(proxy::DesiredAppConfig {
                    domains: &domains,
                    upstreams: &upstreams,
                    tls: tls_enabled,
                }),
            )
            .await
            {
                tracing::warn!("failed to write angie config: {e}");
            }
        }

        // Emit deploy URL info
        let scheme = if tls_enabled { "https" } else { "http" };
        let url = if let Some(domain) = domains.first() {
            format!("{scheme}://{domain}")
        } else {
            format!("http://127.0.0.1:{port}")
        };

        events.emit(
            Some(app_id.clone()),
            "deploy.live",
            Some(serde_json::json!({
                "url": url,
                "dashboard": format!("http://127.0.0.1:{}", cfg.api_port),
            })),
        );

        tracing::info!(
            app = app_name,
            %url,
            dashboard = format!("http://127.0.0.1:{}", cfg.api_port),
            "Application deployed"
        );
    }

    // ── Mark deployment live ──────────────────────────────────────────────────

    queries::update_deployment(pool, deploy_id, DeployStatus::Live, Some(&built.tag)).await?;
    deployment.status = DeployStatus::Live;

    match database::verify_linked_services_post_deploy(pool, docker, app_id).await {
        Ok(lines) => {
            for line in lines {
                events.emit(
                    Some(app_id.clone()),
                    "deploy.service_check",
                    Some(serde_json::json!({ "line": line })),
                );
            }
        }
        Err(error) => {
            let line = format!("linked service verification skipped: {error}");
            tracing::warn!(app = app_name, "{line}");
            events.emit(
                Some(app_id.clone()),
                "deploy.service_check",
                Some(serde_json::json!({ "line": line })),
            );
        }
    }

    plugins
        .run_post_deploy(&DeployContext {
            app,
            deployment: deployment.clone(),
            data_dir: cfg.data_dir.clone(),
        })
        .await;

    // ── Retire old containers ─────────────────────────────────────────────────

    let old_ids: Vec<String> = previous_containers.iter().map(|c| c.id.clone()).collect();

    if !old_ids.is_empty() {
        let docker_clone = docker.clone();
        let pool_clone = pool.clone();
        tokio::spawn(async move {
            tokio::time::sleep(std::time::Duration::from_secs(retire_secs)).await;
            for id in old_ids {
                let _ = container::stop_container(&docker_clone, &id, 30).await;
                let _ = container::remove_container(&docker_clone, &id).await;
                let _ = queries::update_container_status(&pool_clone, &id, "removed").await;
            }
        });
    }

    Ok(built.tag)
}

// ── Release phase ─────────────────────────────────────────────────────────────

async fn run_release_phase(
    pool: &SqlitePool,
    docker: &DockerClient,
    events: &EventSender,
    app_id: &str,
    image_tag: &str,
    command: &str,
) -> anyhow::Result<()> {
    use bollard::container::LogOutput;
    use bollard::models::ContainerCreateBody;
    use bollard::query_parameters::{
        CreateContainerOptionsBuilder, LogsOptionsBuilder, RemoveContainerOptionsBuilder,
        WaitContainerOptionsBuilder,
    };
    use futures::StreamExt;

    let config_vars = queries::get_config_vars(pool, app_id).await?;
    let env: Vec<String> = config_vars
        .iter()
        .map(|cv| format!("{}={}", cv.key, cv.value))
        .collect();

    let container_name = format!(
        "deku.release.{}",
        &uuid::Uuid::new_v4().simple().to_string()[..8]
    );

    let create_opts = CreateContainerOptionsBuilder::default()
        .name(container_name.as_str())
        .build();

    let body = ContainerCreateBody {
        image: Some(image_tag.to_string()),
        cmd: Some(vec![
            "sh".to_string(),
            "-c".to_string(),
            command.to_string(),
        ]),
        env: Some(env),
        ..Default::default()
    };

    let resp = docker.create_container(Some(create_opts), body).await?;

    docker
        .start_container(
            &resp.id,
            None::<bollard::query_parameters::StartContainerOptions>,
        )
        .await?;

    let wait_opts = WaitContainerOptionsBuilder::default().build();
    let mut wait_stream = docker.wait_container(&resp.id, Some(wait_opts));
    let wait_result = wait_stream.next().await;

    // Stream logs
    let log_opts = LogsOptionsBuilder::default()
        .stdout(true)
        .stderr(true)
        .build();
    let mut log_stream = docker.logs(&resp.id, Some(log_opts));
    while let Some(Ok(log)) = log_stream.next().await {
        let line = match log {
            LogOutput::StdOut { message } | LogOutput::StdErr { message } => {
                String::from_utf8_lossy(&message).trim_end().to_string()
            }
            _ => continue,
        };
        events.emit(
            Some(app_id.to_string()),
            "deploy.release.log",
            Some(serde_json::json!({ "line": line })),
        );
    }

    // Check exit code
    let exit_code = wait_result
        .and_then(|r| r.ok())
        .map(|r| r.status_code)
        .unwrap_or(1);

    let remove_opts = RemoveContainerOptionsBuilder::default().force(true).build();
    docker.remove_container(&resp.id, Some(remove_opts)).await?;

    if exit_code != 0 {
        return Err(anyhow::anyhow!(
            "release phase failed with exit code {exit_code}"
        ));
    }

    Ok(())
}

// ── Rollback ──────────────────────────────────────────────────────────────────

/// Roll back an app to its previous live deployment.
#[allow(clippy::too_many_arguments)]
pub async fn rollback(
    pool: &SqlitePool,
    docker: &DockerClient,
    events: &EventSender,
    cfg: &DekuConfig,
    plugins: &crate::plugins::PluginRegistry,
    app_id: &str,
    app_name: &str,
    to_deployment_id: Option<&str>,
) -> anyhow::Result<()> {
    let current = queries::get_latest_deployment(pool, app_id)
        .await?
        .ok_or_else(|| anyhow::anyhow!("no live deployment to roll back from"))?;

    let target = if let Some(id) = to_deployment_id {
        queries::get_deployment(pool, id).await?
    } else {
        queries::get_previous_deployment(pool, app_id, &current.id)
            .await?
            .ok_or_else(|| anyhow::anyhow!("no previous deployment to roll back to"))?
    };

    let image_tag = target
        .image_tag
        .as_deref()
        .ok_or_else(|| anyhow::anyhow!("target deployment has no image tag"))?;

    events.emit(
        Some(app_id.to_string()),
        "deploy.rollback.started",
        Some(serde_json::json!({ "to_deploy_id": target.id, "image": image_tag })),
    );

    let req = DeployRequest {
        app_id: app_id.to_string(),
        app_name: app_name.to_string(),
        source: DeploySource::Image {
            reference: image_tag.to_string(),
        },
        force_builder: Some("image".to_string()),
    };

    run_deploy(pool, docker, events, cfg, plugins, req).await?;

    // Mark the previous current deployment as rolled_back
    queries::update_deployment(pool, &current.id, DeployStatus::RolledBack, None).await?;

    Ok(())
}

// ── Utilities ─────────────────────────────────────────────────────────────────

fn load_deku_toml(source_dir: &std::path::Path) -> Option<DekuToml> {
    let path = source_dir.join("deku.toml");
    if !path.exists() {
        return None;
    }
    let content = std::fs::read_to_string(&path).ok()?;
    toml::from_str(&content).ok()
}

/// Parse memory string like "512m", "1g", "1024k" into bytes.
fn parse_memory_bytes(mem: &str) -> Option<i64> {
    let mem = mem.trim().to_lowercase();
    if let Some(stripped) = mem.strip_suffix('g') {
        return stripped.parse::<i64>().ok().map(|n| n * 1024 * 1024 * 1024);
    }
    if let Some(stripped) = mem.strip_suffix('m') {
        return stripped.parse::<i64>().ok().map(|n| n * 1024 * 1024);
    }
    if let Some(stripped) = mem.strip_suffix('k') {
        return stripped.parse::<i64>().ok().map(|n| n * 1024);
    }
    mem.parse::<i64>().ok()
}

/// Parse CPU string like "0.5" (cores) or "500m" (millicores) into Docker cpu_quota.
/// Docker cpu_period defaults to 100_000 µs. quota = cores * period.
fn parse_cpu_quota(cpu: &str) -> Option<i64> {
    let cpu = cpu.trim().to_lowercase();
    const PERIOD: i64 = 100_000;

    if let Some(stripped) = cpu.strip_suffix('m') {
        // millicores
        let millicores: i64 = stripped.parse().ok()?;
        return Some(millicores * PERIOD / 1000);
    }
    // decimal cores
    let cores: f64 = cpu.parse().ok()?;
    Some((cores * PERIOD as f64) as i64)
}
