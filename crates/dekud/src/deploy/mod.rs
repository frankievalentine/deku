use std::collections::HashMap;

use deku_core::types::{
    AppStatus, BuilderType, ContainerRecord, DekuToml, DeployStatus, ObjectStoreConfig,
    ProcfileEntry,
};
use sqlx::SqlitePool;

use crate::build::{
    deployment_image_id, deployment_image_repo, deployment_image_tag, get_exposed_ports,
    select_builder, BuildContext, BuiltImage,
};
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
    /// Build from an uploaded tar.gz archive the daemon has already written to
    /// a local temp file. Never a caller-supplied path or URL.
    Archive { path: std::path::PathBuf },
}

#[derive(Debug, Clone)]
pub struct DeployRequest {
    pub app_id: String,
    pub app_name: String,
    pub source: DeploySource,
    pub force_builder: Option<String>,
    /// `None` uses the configured build host when one exists, `local` disables
    /// offloading for this deploy, and any other value must match the configured
    /// build host name.
    pub build_host: Option<String>,
    /// Target environment id. `None` deploys to production.
    pub environment_id: Option<String>,
}

/// Decide whether this deploy should build on the configured build host.
fn resolve_build_host<'a>(
    cfg: &'a DekuConfig,
    requested: Option<&str>,
) -> anyhow::Result<Option<&'a crate::config::BuildHostConfig>> {
    match requested {
        None => Ok(cfg.build_host.as_ref()),
        Some("local") => Ok(None),
        Some(name) => match cfg.build_host.as_ref() {
            Some(host) if host.name == name => Ok(Some(host)),
            Some(host) => Err(anyhow::anyhow!(
                "unknown build host '{name}'; configured host is '{}', or pass '--build-host local'",
                host.name
            )),
            None => Err(anyhow::anyhow!(
                "no build host is configured; run 'deku build-host setup' or pass '--build-host local'"
            )),
        },
    }
}

// ── Source build (local or remote build host) ─────────────────────────────────

#[allow(clippy::too_many_arguments)]
async fn build_from_source(
    docker: &DockerClient,
    events: &EventSender,
    cfg: &DekuConfig,
    req: &DeployRequest,
    app: &deku_core::types::App,
    deploy_id: &str,
    source_path: &std::path::Path,
    image_tag: &str,
    use_build_host: bool,
) -> anyhow::Result<(BuiltImage, Option<DekuToml>)> {
    let deku_toml = load_deku_toml(source_path);
    let forced = req.force_builder.as_deref().or_else(|| {
        deku_toml
            .as_ref()
            .and_then(|t| t.build.as_ref())
            .and_then(|b| b.builder.as_deref())
    });

    let builder = select_builder(source_path, forced).map_err(|e| anyhow::anyhow!("{e}"))?;
    let builder_name = builder.name();

    if use_build_host && crate::build_remote::supports_remote_build(builder_name) {
        let registry = cfg.registry.as_ref().ok_or_else(|| {
            anyhow::anyhow!(
                "remote builds require a registry: configure one with 'deku registry setup', or pass '--build-host local'"
            )
        })?;
        let reference = registry.image_reference(&app.name, deploy_id);
        let host_name = cfg
            .build_host
            .as_ref()
            .map(|host| host.name.as_str())
            .unwrap_or("build host");

        tracing::info!(
            app = %app.name,
            builder = builder_name,
            build_host = host_name,
            "building on remote build host"
        );
        crate::hooks::fire(
            cfg,
            crate::hooks::HookEvent::PreBuild,
            crate::hooks::HookEventData {
                app,
                deployment: None,
                detail: serde_json::json!({
                    "deploy_id": deploy_id,
                    "builder": builder_name,
                    "build_host": host_name,
                }),
            },
        )
        .await?;
        crate::build_remote::build_remote(
            source_path,
            builder_name,
            deku_toml.as_ref(),
            cfg,
            events,
            &app.id,
            &reference,
        )
        .await?;
        crate::hooks::fire(
            cfg,
            crate::hooks::HookEvent::PostBuild,
            crate::hooks::HookEventData {
                app,
                deployment: None,
                detail: serde_json::json!({ "deploy_id": deploy_id, "builder": builder_name }),
            },
        )
        .await?;

        events.emit(
            Some(app.id.clone()),
            "build.log",
            Some(serde_json::json!({ "line": format!("Pulling {reference} on the deploy host") })),
        );
        let events_clone = events.clone();
        let app_id_clone = app.id.clone();
        container::pull_image(docker, &reference, |status| {
            events_clone.emit(
                Some(app_id_clone.clone()),
                "build.log",
                Some(serde_json::json!({ "line": status })),
            );
        })
        .await?;
        container::tag_image(
            docker,
            &reference,
            &deployment_image_repo(&app.name),
            &deployment_image_id(deploy_id),
        )
        .await?;

        let exposed_ports = get_exposed_ports(docker, image_tag).await;
        let procfile = crate::build::extract_procfile(docker, image_tag).await;

        return Ok((
            BuiltImage {
                tag: image_tag.to_string(),
                exposed_ports,
                procfile,
            },
            deku_toml,
        ));
    }

    let buildkit_host = if builder_name == "railpack" {
        Some(crate::buildkit::resolve_host(docker, cfg).await?)
    } else {
        None
    };
    let ctx = BuildContext {
        app: app.clone(),
        source_dir: source_path.to_path_buf(),
        buildkit_host,
    };

    tracing::info!(app = %app.name, builder = builder_name, "selected builder");
    crate::hooks::fire(
        cfg,
        crate::hooks::HookEvent::PreBuild,
        crate::hooks::HookEventData {
            app,
            deployment: None,
            detail: serde_json::json!({ "deploy_id": deploy_id, "builder": builder_name }),
        },
    )
    .await?;
    let built = builder
        .build(&ctx, deku_toml.as_ref(), docker, events, image_tag)
        .await
        .map_err(|e| anyhow::anyhow!("{e}"))?;
    crate::hooks::fire(
        cfg,
        crate::hooks::HookEvent::PostBuild,
        crate::hooks::HookEventData {
            app,
            deployment: None,
            detail: serde_json::json!({ "deploy_id": deploy_id, "builder": builder_name }),
        },
    )
    .await?;
    Ok((built, deku_toml))
}

// ── Health check ──────────────────────────────────────────────────────────────
/// Replica indices that failed their readiness check, in order.
fn unready_replicas(results: &[(usize, u16, bool)]) -> Vec<usize> {
    results
        .iter()
        .filter(|(_, _, ready)| !ready)
        .map(|(replica, _, _)| *replica)
        .collect()
}

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

/// How many fresh ports a web replica tries before its deploy fails.
const WEB_PORT_ATTEMPTS: usize = 3;

/// Start a web replica on a free loopback port and return its container id
/// and host port.
///
/// The port is found by binding it and letting it go, so another process can
/// take it before Docker binds it. A start that loses that race is retried on a
/// fresh port instead of failing the deploy.
async fn start_web_replica(
    docker: &DockerClient,
    spec: &mut ContainerSpec<'_>,
    container_port: u16,
) -> anyhow::Result<(String, u16)> {
    let mut attempt = 1;
    loop {
        let port = find_free_port()?;
        spec.port_bindings = HashMap::from([(format!("{container_port}/tcp"), port)]);
        match container::start_container(docker, spec).await {
            Ok(id) => return Ok((id, port)),
            Err(error) if attempt < WEB_PORT_ATTEMPTS && container::is_port_conflict(&error) => {
                tracing::warn!(port, name = spec.name, "host port was taken; retrying");
                attempt += 1;
            }
            Err(error) => return Err(error),
        }
    }
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
    logs: &std::sync::Arc<crate::logs::LogBus>,
    cfg: &DekuConfig,
    req: DeployRequest,
) -> anyhow::Result<String> {
    let app_id = &req.app_id;
    let app_name = &req.app_name;

    // Determine builder type for record keeping
    let builder_type = match &req.source {
        DeploySource::Image { .. } => BuilderType::Image,
        DeploySource::Archive { .. } => BuilderType::Archive,
        DeploySource::Source { .. } => match req.force_builder.as_deref() {
            Some("railpack") => BuilderType::Railpack,
            Some("nixpacks") => BuilderType::Nixpacks,
            Some("pack") => BuilderType::Pack,
            Some("compose") => BuilderType::Compose,
            _ => BuilderType::Dockerfile,
        },
    };

    // The environment this rollout belongs to. Production is the target unless
    // the caller named one.
    let environment = match req.environment_id.as_deref() {
        Some(id) => queries::get_environment_by_id(pool, id).await?,
        None => queries::ensure_production_environment(pool, app_id).await?,
    };

    // Step 1: Create deployment record
    let mut deployment =
        queries::create_deployment(pool, app_id, &environment.id, builder_type).await?;
    let deploy_id = deployment.id.clone();

    // Build output travels the event bus; mirror this deployment's share of it
    // into the log store so build logs are searchable and survive the rollout.
    let build_log_mirror = crate::logs::spawn_build_log_mirror(
        events.subscribe(),
        logs.clone(),
        app_id.clone(),
        deploy_id.clone(),
        environment.id.clone(),
    );

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
    let result = do_deploy(
        pool,
        docker,
        events,
        logs,
        cfg,
        &req,
        &mut deployment,
        &environment,
    )
    .await;

    // Drains any build lines still buffered before returning.
    build_log_mirror.stop().await;

    // Resolve the app once for hook payloads; a hook failure never changes the
    // deploy outcome, so these two events are advisory.
    if let Ok(app) = queries::get_app(pool, app_name).await {
        let (event, detail) = match &result {
            Ok(_) => (
                crate::hooks::HookEvent::DeploySucceeded,
                serde_json::json!({ "deploy_id": deploy_id }),
            ),
            Err(error) => (
                crate::hooks::HookEvent::DeployFailed,
                serde_json::json!({ "deploy_id": deploy_id, "error": error.to_string() }),
            ),
        };
        if let Err(error) = crate::hooks::fire(
            cfg,
            event,
            crate::hooks::HookEventData {
                app: &app,
                deployment: Some(&deployment),
                detail,
            },
        )
        .await
        {
            tracing::warn!("{} hook reported an error: {error}", event.as_str());
        }
    }

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
            let still_serving =
                queries::get_latest_deployment_in_environment(pool, app_id, &environment.id)
                    .await?
                    .is_some();
            let app_status = if still_serving {
                AppStatus::Deployed
            } else {
                AppStatus::Error
            };
            queries::update_app_status(pool, app_id, app_status).await?;
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
#[allow(clippy::too_many_arguments)]
async fn do_deploy(
    pool: &SqlitePool,
    docker: &DockerClient,
    events: &EventSender,
    logs: &std::sync::Arc<crate::logs::LogBus>,
    cfg: &DekuConfig,
    req: &DeployRequest,
    deployment: &mut deku_core::types::Deployment,
    environment: &crate::db::queries::Environment,
) -> anyhow::Result<String> {
    let environment_id = environment.id.as_str();
    let app_id = &req.app_id;
    let app_name = &req.app_name;
    let deploy_id = &deployment.id;
    let app = queries::get_app_by_id(pool, app_id).await?;
    let use_build_host = resolve_build_host(cfg, req.build_host.as_deref())?.is_some();

    // ── Build phase ───────────────────────────────────────────────────────────

    queries::update_deployment(pool, deploy_id, DeployStatus::Building, None).await?;

    let image_tag = deployment_image_tag(app_name, deploy_id);

    let (built, deku_toml) = match &req.source {
        DeploySource::Image { reference } => {
            if container::image_exists(docker, reference).await {
                events.emit(
                    Some(app_id.clone()),
                    "build.log",
                    Some(serde_json::json!({ "line": format!("Using local image {reference}") })),
                );
            } else {
                events.emit(
                    Some(app_id.clone()),
                    "build.log",
                    Some(serde_json::json!({ "line": format!("Pulling image {reference}...") })),
                );

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
            }

            let recorded_tag =
                if reference.starts_with(&format!("{}:", deployment_image_repo(app_name))) {
                    reference.clone()
                } else {
                    let (repo_part, tag_part) = reference
                        .rsplit_once(':')
                        .unwrap_or((reference.as_str(), "latest"));

                    container::tag_image(
                        docker,
                        &format!("{repo_part}:{tag_part}"),
                        &deployment_image_repo(app_name),
                        &deployment_image_id(deploy_id),
                    )
                    .await?;

                    image_tag.clone()
                };

            let exposed_ports = get_exposed_ports(docker, &recorded_tag).await;

            (
                BuiltImage {
                    tag: recorded_tag,
                    exposed_ports,
                    procfile: vec![],
                },
                None,
            )
        }

        DeploySource::Archive { path } => {
            // Extract to a temp dir, then run source build
            let tmp_dir = tempfile::tempdir()?;
            let archive_bytes = std::fs::read(path)?;

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
            build_from_source(
                docker,
                events,
                cfg,
                req,
                &app,
                deploy_id,
                &source_path,
                &image_tag,
                use_build_host,
            )
            .await?
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

            build_from_source(
                docker,
                events,
                cfg,
                req,
                &app,
                deploy_id,
                path,
                &image_tag,
                use_build_host,
            )
            .await?
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

    // The release task and every replica join the shared network.
    container::ensure_deku_network(docker).await?;

    // ── Release phase ─────────────────────────────────────────────────────────

    if let Some(release) = release_entry {
        events.emit(
            Some(app_id.clone()),
            "deploy.release",
            Some(serde_json::json!({ "command": release.command })),
        );
        run_release_phase(
            pool,
            docker,
            events,
            logs,
            cfg,
            ReleaseRun {
                app_id,
                deployment_id: deploy_id.as_str(),
                environment_id,
                image_tag: &built.tag,
                command: &release.command,
            },
        )
        .await?;
    }

    // ── Deploy phase ──────────────────────────────────────────────────────────

    queries::update_deployment(pool, deploy_id, DeployStatus::Deploying, Some(&built.tag)).await?;
    deployment.status = DeployStatus::Deploying;
    deployment.image_tag = Some(built.tag.clone());

    // A blocking pre_deploy hook can stop the rollout before any container moves.
    crate::hooks::fire(
        cfg,
        crate::hooks::HookEvent::PreDeploy,
        crate::hooks::HookEventData {
            app: &app,
            deployment: Some(deployment),
            detail: serde_json::json!({ "image_tag": built.tag }),
        },
    )
    .await?;

    // Which of this environment's deployments stay reachable at their own URL:
    // the new one plus the newest (keep - 1) previous ones. A retained
    // deployment keeps one web replica for its URL; everything else it ran is
    // retired, as is everything older (see `plan_retirement`).
    //
    // Scoping to the environment matters: an app-wide list would include the
    // containers of every other environment, and retiring them would take those
    // environments down. Collected before the new containers exist, so what
    // remains after the filter is exactly the retiring set.
    let keep = cfg.previews.keep_deployments.max(1);
    let retained_previous = queries::list_recent_environment_deployments(
        pool,
        app_id,
        environment_id,
        Some(deploy_id.as_str()),
        (keep - 1) as i64,
    )
    .await?;

    let retirement = plan_retirement(
        &queries::list_containers_for_environment(pool, app_id, environment_id).await?,
        &retained_previous,
    );

    // Load per-environment config: app-wide values with this environment's
    // overrides applied.
    let config_vars =
        crate::secrets::resolve_config_vars(pool, cfg, app_id, Some(environment_id)).await?;
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
    // Every web replica, as (replica index, host port). Readiness must cover all
    // of them: a deploy that goes live with one broken replica is worse than one
    // that keeps the previous version serving.
    let mut web_replicas: Vec<(usize, u16)> = Vec::new();
    let mut web_host_port: Option<u16> = None;

    for entry in &run_entries {
        let proc_type = &entry.process_type;
        let scale = *scales.get(proc_type.as_str()).unwrap_or(&1) as usize;
        let is_web = proc_type == "web";

        for i in 0..scale {
            let container_name = format!("deku.{app_name}.{proc_type}.{}-{i}", &deploy_id[..8]);

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

            let mut spec = ContainerSpec {
                image: &built.tag,
                name: &container_name,
                env: env.clone(),
                port_bindings: HashMap::new(),
                volumes: volumes.clone(),
                memory,
                cpu_quota,
                cmd,
            };

            // Non-web processes don't need a host port binding.
            let (container_id, host_port) = if is_web {
                let (id, port) = start_web_replica(docker, &mut spec, web_container_port).await?;
                if i == 0 {
                    web_host_port = Some(port);
                }
                web_replicas.push((i, port));
                (id, port)
            } else {
                (container::start_container(docker, &spec).await?, 0)
            };

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

            // Follow this replica from its first line. The task ends when the
            // container stops, which is also how a retired deployment stops
            // collecting.
            crate::logs::spawn_runtime_collector(
                logs.clone(),
                docker.clone(),
                container_id.clone(),
                app_id.clone(),
                Some(deploy_id.to_string()),
                Some(environment_id.to_string()),
                crate::logs::CollectorStart::Beginning,
            );

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

        // Gate the whole rollout on every replica, not just the first one.
        let mut results: Vec<(usize, u16, bool)> = Vec::with_capacity(web_replicas.len());
        for (replica, replica_port) in &web_replicas {
            let ready =
                run_health_check(*replica_port, &health_path, health_timeout, health_attempts)
                    .await;
            events.emit(
                Some(app_id.clone()),
                "deploy.replica_ready",
                Some(serde_json::json!({
                    "replica": replica,
                    "port": replica_port,
                    "ready": ready,
                })),
            );
            results.push((*replica, *replica_port, ready));
        }

        let unready = unready_replicas(&results);
        if !unready.is_empty() {
            // Rollback: stop new containers, keep the previous version serving.
            events.emit(
                Some(app_id.clone()),
                "deploy.rollback",
                Some(serde_json::json!({
                    "reason": "health checks failed",
                    "unready_replicas": unready,
                })),
            );
            for id in &new_container_ids {
                let _ = container::stop_container(docker, id, 10).await;
                let _ = container::remove_container(docker, id).await;
                let _ = queries::update_container_status(pool, id, "removed").await;
            }
            queries::update_deployment(pool, deploy_id, DeployStatus::Failed, Some(&built.tag))
                .await?;
            return Err(anyhow::anyhow!(
                "health checks failed for web {} after {health_attempts} attempts",
                if unready.len() == 1 {
                    format!("replica {}", unready[0])
                } else {
                    format!("replicas {unready:?}")
                }
            ));
        }

        // Update Angie upstream
        let domains = queries::list_domain_names(pool, app_id).await?;
        let tls_enabled = queries::get_app_by_id(pool, app_id)
            .await
            .map(|app| app.tls_enabled)
            .unwrap_or(false);
        if proxy::has_routable_hosts(&domains, cfg.global_domain.as_deref()) {
            // One upstream per web replica, so scaling the web process actually
            // spreads traffic instead of leaving extra replicas idle.
            let upstreams: Vec<deku_core::types::Upstream> = web_replicas
                .iter()
                .map(|(_, replica_port)| deku_core::types::Upstream {
                    host: "127.0.0.1".to_string(),
                    port: *replica_port,
                })
                .collect();
            // Reading auth, maintenance, and redirects belongs to the routing
            // step: a failure here must not fall back to a config that silently
            // drops app authentication.
            let routing: anyhow::Result<()> = async {
                let extras = proxy::load_extras(pool, app_id).await?;
                proxy::apply_app_config(
                    pool,
                    &cfg.angie_conf_dir,
                    cfg.global_domain.as_deref(),
                    Some(&cfg.acme),
                    app_id,
                    app_name,
                    Some(proxy::DesiredAppConfig {
                        environment_id,
                        domains: &domains,
                        upstreams: &upstreams,
                        tls: tls_enabled,
                        auth: extras.auth.as_ref(),
                        maintenance: extras.maintenance,
                        maintenance_message: extras.maintenance_message.as_deref(),
                        redirects: &extras.redirects,
                    }),
                )
                .await?;
                Ok(())
            }
            .await;

            if let Err(e) = routing {
                events.emit(
                    Some(app_id.clone()),
                    "deploy.routing_failed",
                    Some(serde_json::json!({ "error": e.to_string() })),
                );
                for id in &new_container_ids {
                    let _ = container::stop_container(docker, id, 10).await;
                    let _ = container::remove_container(docker, id).await;
                    let _ = queries::update_container_status(pool, id, "removed").await;
                }
                return Err(anyhow::anyhow!(
                    "routing configuration failed; previous deployment left serving: {e}"
                ));
            }
        }

        // Update port mapping in DB only after the route is confirmed live
        let _ = queries::upsert_port_mapping(
            pool,
            app_id,
            port as i64,
            web_container_port as i64,
            "tcp",
        )
        .await;

        // Emit deploy URL info
        let scheme = if tls_enabled { "https" } else { "http" };
        let url = if let Some(domain) = domains.first() {
            format!("{scheme}://{domain}")
        } else {
            format!("http://127.0.0.1:{port}")
        };

        // The deployment's own address, which keeps serving this build while it
        // is retained even after a later deploy replaces it.
        let deployment_url = crate::proxy::deployment_hostname(
            app_name,
            &environment.slug,
            deploy_id,
            cfg.global_domain.as_deref(),
        )
        .map(|hostname| format!("http://{hostname}"));

        events.emit(
            Some(app_id.clone()),
            "deploy.live",
            Some(serde_json::json!({
                "url": url,
                "deployment_url": deployment_url,
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

    match database::verify_linked_services_post_deploy(pool, docker, cfg, app_id).await {
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

    if let Err(error) = crate::hooks::fire(
        cfg,
        crate::hooks::HookEvent::PostDeploy,
        crate::hooks::HookEventData {
            app: &app,
            deployment: Some(deployment),
            detail: serde_json::json!({ "image_tag": built.tag }),
        },
    )
    .await
    {
        // Post-deploy hooks are advisory; the rollout already succeeded.
        tracing::warn!("post_deploy hook reported an error: {error}");
    }

    // ── Retire old containers ─────────────────────────────────────────────────

    if !retirement.now.is_empty() {
        let docker_clone = docker.clone();
        let pool_clone = pool.clone();
        let ids = retirement.now;
        tokio::spawn(async move {
            retire_containers(&docker_clone, &pool_clone, ids).await;
        });
    }

    let old_ids = retirement.after_grace;

    if !old_ids.is_empty() {
        let docker_clone = docker.clone();
        let pool_clone = pool.clone();
        let cfg_clone = cfg.clone();
        let app_id_clone = app_id.clone();
        let app_name_clone = app_name.clone();
        tokio::spawn(async move {
            tokio::time::sleep(std::time::Duration::from_secs(retire_secs)).await;
            retire_containers(&docker_clone, &pool_clone, old_ids).await;

            // A retired deployment's vhost would otherwise keep pointing at a
            // port nothing listens on, so its URL would fail with a proxy error
            // instead of simply no longer resolving. Rewriting now makes the URL
            // disappear with the containers it named.
            if let Err(error) = crate::proxy::reconcile_app(
                &pool_clone,
                &cfg_clone.angie_conf_dir,
                cfg_clone.global_domain.as_deref(),
                Some(&cfg_clone.acme),
                &app_id_clone,
                &app_name_clone,
            )
            .await
            {
                tracing::warn!(app = %app_name_clone, "reconcile after retirement failed: {error}");
            }
        });
    }

    Ok(built.tag)
}

/// Stop and remove `ids` all at once. Each stop can wait out its whole timeout
/// for a process that ignores SIGTERM, so stopping them in turn would keep the
/// last one running for that timeout times the number before it.
async fn retire_containers(docker: &DockerClient, pool: &SqlitePool, ids: Vec<String>) {
    futures::future::join_all(ids.iter().map(|id| async move {
        let _ = container::stop_container(docker, id, 30).await;
        let _ = container::remove_container(docker, id).await;
        let _ = queries::update_container_status(pool, id, "removed").await;
    }))
    .await;
}

/// Which of an environment's running containers a new deployment retires, and when.
#[derive(Debug, Default, PartialEq)]
struct Retirement {
    /// Stopped as soon as the new deployment is live: every process other than
    /// web of the deployments it replaces. A worker left running would keep
    /// taking jobs with the old code, even after a migration changed the schema
    /// under it.
    now: Vec<String>,
    /// Stopped after the retire grace period, so requests in flight finish: the
    /// web replicas of deployments outside the retention window, and every web
    /// replica of a retained deployment beyond the one its URL is served from.
    after_grace: Vec<String>,
}

/// Plan the retirement of `containers`, an environment's running containers
/// collected before the new deployment started any, given the previous
/// deployments that stay `retained`.
///
/// A retained deployment keeps its first web replica, the one started first,
/// so its own URL keeps serving the build it names. Nothing else of it keeps
/// running: a per-deployment URL is for looking at a build, not for taking
/// production load, and a rollback starts a fresh deployment rather than
/// reusing these containers.
fn plan_retirement(containers: &[ContainerRecord], retained: &[String]) -> Retirement {
    let mut kept: HashMap<&str, &ContainerRecord> = HashMap::new();
    for container in containers
        .iter()
        .filter(|c| c.process_type == "web" && retained.contains(&c.deployment_id))
    {
        let first = kept
            .entry(container.deployment_id.as_str())
            .or_insert(container);
        if container.created_at < first.created_at {
            *first = container;
        }
    }

    let mut retirement = Retirement::default();
    for container in containers {
        if container.process_type != "web" {
            retirement.now.push(container.id.clone());
        } else if kept
            .get(container.deployment_id.as_str())
            .is_none_or(|first| first.id != container.id)
        {
            retirement.after_grace.push(container.id.clone());
        }
    }
    retirement
}

// ── Restart recovery ──────────────────────────────────────────────────────────

/// An app whose in-progress deployment was failed during restart recovery.
pub struct RecoveredApp {
    pub id: String,
    pub name: String,
}

/// Fail the deployments a previous daemon left in progress.
///
/// A deploy runs inside the daemon, so one that was mid-flight when the daemon
/// stopped never finishes on its own. Each is marked failed, the containers it
/// started are removed, and its app's status is set from whether a live
/// deployment remains. Returns the affected apps, whose routing the caller
/// rewrites from their live deployment. Must run before the API accepts
/// deploys, so it never touches a deploy that is actually running.
pub async fn recover_interrupted_deploys(
    pool: &SqlitePool,
    docker: &DockerClient,
) -> anyhow::Result<Vec<RecoveredApp>> {
    use sqlx::Row;

    // The statuses a deployment only holds while `run_deploy` is driving it.
    let rows = sqlx::query(
        "SELECT d.id AS id, d.app_id AS app_id, d.image_tag AS image_tag, a.name AS app_name
         FROM deployments d JOIN apps a ON a.id = d.app_id
         WHERE d.status IN ('pending', 'building', 'built', 'deploying', 'health_checking')",
    )
    .fetch_all(pool)
    .await?;

    let mut recovered: Vec<RecoveredApp> = Vec::new();
    for row in rows {
        let deploy_id: String = row.get("id");
        let app_id: String = row.get("app_id");
        let app_name: String = row.get("app_name");
        let image_tag: Option<String> = row.get("image_tag");

        let containers: Vec<String> = sqlx::query_scalar(
            "SELECT id FROM containers WHERE deployment_id = ?1 AND status != 'removed'",
        )
        .bind(&deploy_id)
        .fetch_all(pool)
        .await?;
        for container_id in &containers {
            // Already gone is fine: the goal is that it is not running.
            let _ = container::stop_container(docker, container_id, 10).await;
            let _ = container::remove_container(docker, container_id).await;
            queries::update_container_status(pool, container_id, "removed").await?;
        }

        queries::update_deployment(pool, &deploy_id, DeployStatus::Failed, image_tag.as_deref())
            .await?;

        let live: i64 = sqlx::query_scalar(
            "SELECT COUNT(*) FROM deployments WHERE app_id = ?1 AND status = 'live'",
        )
        .bind(&app_id)
        .fetch_one(pool)
        .await?;
        let app_status = if live > 0 {
            AppStatus::Deployed
        } else {
            AppStatus::Error
        };
        queries::update_app_status(pool, &app_id, app_status).await?;

        tracing::warn!(
            app = %app_name,
            deploy_id = %deploy_id,
            removed_containers = containers.len(),
            "failed a deployment the daemon was running when it stopped"
        );
        if !recovered.iter().any(|app| app.id == app_id) {
            recovered.push(RecoveredApp {
                id: app_id,
                name: app_name,
            });
        }
    }

    Ok(recovered)
}

// ── Release phase ─────────────────────────────────────────────────────────────

#[allow(clippy::too_many_arguments)]
/// A release command to run, and the deployment its output is stored under.
struct ReleaseRun<'a> {
    app_id: &'a str,
    deployment_id: &'a str,
    environment_id: &'a str,
    image_tag: &'a str,
    command: &'a str,
}

/// Run the release command in a fresh container and fail the deploy if it fails.
///
/// Its output is followed while it runs: each line is emitted for the CLI and
/// stored as a `release` log line of the deployment, so a failed migration's
/// error is still there to read after the deploy has ended.
async fn run_release_phase(
    pool: &SqlitePool,
    docker: &DockerClient,
    events: &EventSender,
    logs: &std::sync::Arc<crate::logs::LogBus>,
    cfg: &DekuConfig,
    run: ReleaseRun<'_>,
) -> anyhow::Result<()> {
    let ReleaseRun {
        app_id,
        deployment_id,
        environment_id,
        image_tag,
        command,
    } = run;
    use bollard::container::LogOutput;
    use bollard::models::ContainerCreateBody;
    use bollard::query_parameters::{
        CreateContainerOptionsBuilder, LogsOptionsBuilder, RemoveContainerOptionsBuilder,
        WaitContainerOptionsBuilder,
    };
    use futures::StreamExt;

    let config_vars =
        crate::secrets::resolve_config_vars(pool, cfg, app_id, Some(environment_id)).await?;
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
        // On the shared network so a migration can reach linked services.
        host_config: Some(bollard::models::HostConfig {
            network_mode: Some(container::DEKU_NETWORK.to_string()),
            ..Default::default()
        }),
        ..Default::default()
    };

    let resp = docker.create_container(Some(create_opts), body).await?;

    docker
        .start_container(
            &resp.id,
            None::<bollard::query_parameters::StartContainerOptions>,
        )
        .await?;

    // Following ends when the container exits, so every line is read before
    // the exit code is.
    let log_opts = LogsOptionsBuilder::default()
        .stdout(true)
        .stderr(true)
        .follow(true)
        .build();
    let mut log_stream = docker.logs(&resp.id, Some(log_opts));
    while let Some(Ok(log)) = log_stream.next().await {
        let (stream, message) = match log {
            LogOutput::StdOut { message } => ("stdout", message),
            LogOutput::StdErr { message } => ("stderr", message),
            _ => continue,
        };
        let text = String::from_utf8_lossy(&message);
        for line in text.trim_end().lines() {
            events.emit(
                Some(app_id.to_string()),
                "deploy.release.log",
                Some(serde_json::json!({ "line": line })),
            );
            if let Err(error) = logs
                .append(
                    app_id,
                    Some(deployment_id),
                    Some(environment_id),
                    crate::logs::SOURCE_RELEASE,
                    stream,
                    line,
                )
                .await
            {
                tracing::warn!(error = %error, "failed to store release log line");
            }
        }
    }

    let wait_opts = WaitContainerOptionsBuilder::default().build();
    let mut wait_stream = docker.wait_container(&resp.id, Some(wait_opts));
    let wait_result = wait_stream.next().await;

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
    logs: &std::sync::Arc<crate::logs::LogBus>,
    cfg: &DekuConfig,
    app_id: &str,
    app_name: &str,
    environment_id: Option<&str>,
    to_deployment_id: Option<&str>,
) -> anyhow::Result<()> {
    // A rollback happens inside one environment: the one named, or production.
    // Scoping the lookups keeps a preview deployment out of a production
    // rollback and the other way round.
    let environment = match environment_id {
        Some(id) => queries::get_environment_by_id(pool, id).await?,
        None => queries::ensure_production_environment(pool, app_id).await?,
    };

    let current = queries::get_latest_deployment_in_environment(pool, app_id, &environment.id)
        .await?
        .ok_or_else(|| {
            anyhow::anyhow!(
                "no live deployment in '{}' to roll back from",
                environment.slug
            )
        })?;

    let target = if let Some(id) = to_deployment_id {
        let target = queries::get_deployment(pool, id).await?;
        // Rolling back to an explicit deployment still happens in the
        // environment being rolled back, never in the target's own.
        let target_environment = queries::get_deployment_environment_id(pool, &target.id).await?;
        if target_environment.as_deref() != Some(environment.id.as_str()) {
            anyhow::bail!(
                "deployment {} does not belong to environment '{}'",
                &target.id[..8.min(target.id.len())],
                environment.slug
            );
        }
        target
    } else {
        queries::get_previous_deployment_in_environment(pool, app_id, &environment.id, &current.id)
            .await?
            .ok_or_else(|| {
                anyhow::anyhow!(
                    "no previous deployment in '{}' to roll back to",
                    environment.slug
                )
            })?
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
        build_host: Some("local".to_string()),
        environment_id: Some(environment.id.clone()),
    };

    run_deploy(pool, docker, events, logs, cfg, req).await?;

    // Mark the previous current deployment as rolled_back
    queries::update_deployment(
        pool,
        &current.id,
        DeployStatus::RolledBack,
        current.image_tag.as_deref(),
    )
    .await?;

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
pub(crate) fn parse_memory_bytes(mem: &str) -> Option<i64> {
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
pub(crate) fn parse_cpu_quota(cpu: &str) -> Option<i64> {
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

#[cfg(test)]
mod tests {
    use super::*;
    use deku_core::types::NewApp;

    fn running(id: &str, deployment: &str, process: &str, started_secs: i64) -> ContainerRecord {
        ContainerRecord {
            id: id.to_string(),
            app_id: "app-1".to_string(),
            deployment_id: deployment.to_string(),
            process_type: process.to_string(),
            status: "running".to_string(),
            host_port: (process == "web").then_some(30000 + started_secs),
            created_at: chrono::DateTime::from_timestamp(started_secs, 0).expect("timestamp"),
        }
    }

    #[test]
    fn a_retained_deployment_keeps_only_its_first_web_replica() {
        // Listed newest first, as the query returns them.
        let containers = vec![
            running("live-worker", "live", "worker", 12),
            running("live-web-1", "live", "web", 11),
            running("live-web-0", "live", "web", 10),
        ];
        let plan = super::plan_retirement(&containers, &["live".to_string()]);
        assert_eq!(
            plan.now,
            vec!["live-worker"],
            "the old worker stops at once"
        );
        assert_eq!(
            plan.after_grace,
            vec!["live-web-1"],
            "extra web replicas drain, and the first one keeps the URL serving"
        );
    }

    #[test]
    fn a_deployment_outside_the_window_is_retired_entirely() {
        let containers = vec![
            running("kept-web-0", "kept", "web", 20),
            running("old-worker", "old", "worker", 2),
            running("old-web-0", "old", "web", 1),
        ];
        let plan = super::plan_retirement(&containers, &["kept".to_string()]);
        assert_eq!(plan.now, vec!["old-worker"]);
        assert_eq!(plan.after_grace, vec!["old-web-0"]);
    }

    #[test]
    fn keeping_only_the_live_deployment_retires_everything_before_it() {
        let containers = vec![
            running("prev-scheduler", "prev", "scheduler", 3),
            running("prev-web-0", "prev", "web", 1),
        ];
        let plan = super::plan_retirement(&containers, &[]);
        assert_eq!(plan.now, vec!["prev-scheduler"]);
        assert_eq!(plan.after_grace, vec!["prev-web-0"]);
    }

    #[test]
    fn unready_replicas_reports_every_failing_index() {
        let results = vec![
            (0usize, 30001u16, true),
            (1, 30002, false),
            (2, 30003, false),
        ];
        assert_eq!(super::unready_replicas(&results), vec![1, 2]);
    }

    #[test]
    fn unready_replicas_is_empty_when_all_are_ready() {
        let results = vec![(0usize, 30001u16, true), (1, 30002, true)];
        assert!(super::unready_replicas(&results).is_empty());
        assert!(super::unready_replicas(&[]).is_empty());
    }

    #[test]
    fn parses_memory_and_cpu_limits() {
        assert_eq!(parse_memory_bytes("512m"), Some(512 * 1024 * 1024));
        assert_eq!(parse_memory_bytes("1g"), Some(1024 * 1024 * 1024));
        assert_eq!(parse_memory_bytes("1024k"), Some(1024 * 1024));
        assert_eq!(parse_memory_bytes("2048"), Some(2048));
        assert_eq!(parse_memory_bytes("512q"), None);

        assert_eq!(parse_cpu_quota("0.5"), Some(50_000));
        assert_eq!(parse_cpu_quota("500m"), Some(50_000));
        assert_eq!(parse_cpu_quota("1"), Some(100_000));
        assert_eq!(parse_cpu_quota("fast"), None);
    }

    #[tokio::test]
    async fn recovery_fails_only_deployments_left_in_progress() {
        let temp = tempfile::tempdir().expect("tempdir");
        let cfg = DekuConfig {
            data_dir: temp.path().to_path_buf(),
            ..DekuConfig::default()
        };
        let pool = crate::db::connect(&cfg).await.expect("connect");
        crate::db::migrate(&pool).await.expect("migrate");
        // No containers are recorded, so the client is never used.
        let docker = container::connect().expect("docker client");

        let app = queries::create_app(
            &pool,
            &deku_core::types::NewApp {
                name: "recover-me".to_string(),
            },
        )
        .await
        .expect("create app");
        let environment = queries::ensure_production_environment(&pool, &app.id)
            .await
            .expect("environment");
        let live = queries::create_deployment(&pool, &app.id, &environment.id, BuilderType::Image)
            .await
            .expect("live deployment");
        queries::update_deployment(&pool, &live.id, DeployStatus::Live, Some("app:1"))
            .await
            .expect("mark live");
        let stuck = queries::create_deployment(&pool, &app.id, &environment.id, BuilderType::Image)
            .await
            .expect("stuck deployment");
        queries::update_deployment(
            &pool,
            &stuck.id,
            DeployStatus::HealthChecking,
            Some("app:2"),
        )
        .await
        .expect("mark in progress");

        let recovered = recover_interrupted_deploys(&pool, &docker)
            .await
            .expect("recover");
        assert_eq!(recovered.len(), 1);
        assert_eq!(recovered[0].name, "recover-me");

        let stuck = queries::get_deployment(&pool, &stuck.id)
            .await
            .expect("stuck");
        assert!(matches!(stuck.status, DeployStatus::Failed));
        let live = queries::get_deployment(&pool, &live.id)
            .await
            .expect("live");
        assert!(matches!(live.status, DeployStatus::Live));
        let app = queries::get_app_by_id(&pool, &app.id).await.expect("app");
        assert!(matches!(app.status, AppStatus::Deployed));

        let again = recover_interrupted_deploys(&pool, &docker)
            .await
            .expect("recover again");
        assert!(again.is_empty(), "recovery is idempotent");
    }

    fn docker_tests_enabled() -> bool {
        std::env::var("DEKU_DOCKER_IT").is_ok()
    }

    struct Harness {
        _temp: tempfile::TempDir,
        pool: SqlitePool,
        docker: DockerClient,
        events: EventSender,
        logs: std::sync::Arc<crate::logs::LogBus>,
        cfg: DekuConfig,
    }

    impl Harness {
        async fn new() -> Option<Self> {
            if !docker_tests_enabled() {
                return None;
            }

            let docker = container::connect().ok()?;
            let temp = tempfile::tempdir().ok()?;
            let cfg = DekuConfig {
                data_dir: temp.path().to_path_buf(),
                angie_conf_dir: temp.path().join("angie"),
                api_port: 0,
                ..DekuConfig::default()
            };
            std::fs::create_dir_all(&cfg.angie_conf_dir).ok()?;

            let pool = crate::db::connect(&cfg).await.ok()?;
            crate::db::migrate(&pool).await.ok()?;
            let events = crate::events::EventBus::new(pool.clone());
            let logs = crate::logs::LogBus::new(pool.clone());

            Some(Self {
                _temp: temp,
                pool,
                docker,
                events,
                logs,
                cfg,
            })
        }

        async fn deploy_image(
            &self,
            app: &deku_core::types::App,
            reference: &str,
        ) -> anyhow::Result<String> {
            run_deploy(
                &self.pool,
                &self.docker,
                &self.events,
                &self.logs,
                &self.cfg,
                DeployRequest {
                    app_id: app.id.clone(),
                    app_name: app.name.clone(),
                    source: DeploySource::Image {
                        reference: reference.to_string(),
                    },
                    force_builder: None,
                    build_host: None,
                    environment_id: None,
                },
            )
            .await
        }

        async fn remove_running_containers(&self, app_id: &str) {
            if let Ok(containers) = queries::list_containers_for_app(&self.pool, app_id).await {
                for record in containers {
                    let _ = container::remove_container(&self.docker, &record.id).await;
                }
            }
        }
    }

    #[tokio::test]
    async fn rollback_restores_the_recorded_immutable_image() {
        let Some(h) = Harness::new().await else {
            return;
        };
        let app = queries::create_app(
            &h.pool,
            &NewApp {
                name: "rollback-it".to_string(),
            },
        )
        .await
        .expect("app should create");

        let first_id = h
            .deploy_image(&app, "nginx:alpine")
            .await
            .expect("first deploy");
        let first = queries::get_deployment(&h.pool, &first_id)
            .await
            .expect("first deployment row");
        let first_tag = first.image_tag.clone().expect("first image tag");
        assert!(!first_tag.ends_with(":latest"));
        assert!(first_tag.ends_with(&deployment_image_id(&first_id)));

        // Web replicas join the shared network and publish on loopback only.
        for record in queries::list_containers_for_app(&h.pool, &app.id)
            .await
            .expect("containers")
        {
            let inspect = h
                .docker
                .inspect_container(&record.id, None)
                .await
                .expect("inspect replica");
            let host_config = inspect.host_config.expect("host config");
            assert_eq!(
                host_config.network_mode.as_deref(),
                Some(container::DEKU_NETWORK)
            );
            let host_ips: Vec<String> = host_config
                .port_bindings
                .unwrap_or_default()
                .into_values()
                .flatten()
                .flatten()
                .filter_map(|binding| binding.host_ip)
                .collect();
            assert!(!host_ips.is_empty(), "a web replica publishes a port");
            assert!(
                host_ips.iter().all(|ip| ip == container::PUBLISH_HOST_IP),
                "{host_ips:?}"
            );
        }

        let second_id = h
            .deploy_image(&app, "nginx:alpine")
            .await
            .expect("second deploy");
        let second = queries::get_deployment(&h.pool, &second_id)
            .await
            .expect("second deployment row");
        let second_tag = second.image_tag.clone().expect("second image tag");
        assert_ne!(first_tag, second_tag, "each deployment owns its own tag");

        rollback(
            &h.pool,
            &h.docker,
            &h.events,
            &h.logs,
            &h.cfg,
            &app.id,
            &app.name,
            None,
            Some(&first_id),
        )
        .await
        .expect("rollback should succeed");

        let live = queries::get_latest_deployment(&h.pool, &app.id)
            .await
            .expect("latest query")
            .expect("a live deployment");
        assert!(matches!(live.status, DeployStatus::Live));
        assert_eq!(live.image_tag.as_deref(), Some(first_tag.as_str()));

        h.remove_running_containers(&app.id).await;
    }

    #[tokio::test]
    async fn routing_failure_fails_deploy_and_keeps_previous_containers() {
        let Some(h) = Harness::new().await else {
            return;
        };
        let app = queries::create_app(
            &h.pool,
            &NewApp {
                name: "routing-it".to_string(),
            },
        )
        .await
        .expect("app should create");
        queries::add_domain(&h.pool, &app.id, "routing-it.test")
            .await
            .expect("domain should add");

        h.deploy_image(&app, "nginx:alpine")
            .await
            .expect("first deploy");

        let before: std::collections::HashSet<String> =
            queries::list_containers_for_app(&h.pool, &app.id)
                .await
                .expect("containers")
                .into_iter()
                .map(|record| record.id)
                .collect();
        assert!(!before.is_empty(), "first deploy should have containers");

        let config_path = proxy::app_config_path(&h.cfg.angie_conf_dir, &app.name);
        std::fs::remove_file(&config_path).expect("config should exist");
        std::fs::create_dir(&config_path).expect("blocking dir should create");

        let result = h.deploy_image(&app, "nginx:alpine").await;
        assert!(result.is_err(), "routing failure must fail the deploy");

        let after: std::collections::HashSet<String> =
            queries::list_containers_for_app(&h.pool, &app.id)
                .await
                .expect("containers")
                .into_iter()
                .map(|record| record.id)
                .collect();
        assert!(
            after.iter().any(|id| before.contains(id)),
            "previous containers must survive a routing failure"
        );

        let stored = queries::get_app_by_id(&h.pool, &app.id).await.expect("app");
        assert!(matches!(stored.status, AppStatus::Deployed));

        h.remove_running_containers(&app.id).await;
    }
}
