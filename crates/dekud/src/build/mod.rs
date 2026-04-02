use std::collections::HashMap;
use std::path::Path;

use async_trait::async_trait;
use bollard::container::LogOutput;
use bollard::query_parameters::{
    BuildImageOptionsBuilder, CreateContainerOptionsBuilder, LogsOptionsBuilder,
    RemoveContainerOptionsBuilder, WaitContainerOptionsBuilder,
};
use bollard::Docker;
use deku_core::error::{DekuError, Result};
use deku_core::types::{parse_procfile, DekuToml, ProcfileEntry};
use deku_plugin_sdk::context::BuildContext;
use futures::StreamExt;

use bollard::body_full;
use crate::container::{create_tar_gz, tag_image};
use crate::events::EventSender;

// ── Output types ──────────────────────────────────────────────────────────────

pub struct BuiltImage {
    pub image_id: String,
    pub tag: String,
    pub exposed_ports: Vec<u16>,
    pub procfile: Vec<ProcfileEntry>,
}

// ── Builder trait ─────────────────────────────────────────────────────────────

#[async_trait]
pub trait Builder: Send + Sync {
    fn name(&self) -> &'static str;
    fn detect(&self, source: &Path) -> bool;
    async fn build(
        &self,
        ctx: &BuildContext,
        deku_toml: Option<&DekuToml>,
        docker: &Docker,
        events: &EventSender,
    ) -> Result<BuiltImage>;
}

// ── Auto-detect ───────────────────────────────────────────────────────────────

pub fn select_builder(source: &Path, forced: Option<&str>) -> Box<dyn Builder> {
    if let Some(name) = forced {
        return match name {
            "dockerfile" => Box::new(DockerfileBuilder),
            "nixpacks" => Box::new(NixpacksBuilder),
            "pack" => Box::new(PackBuilder),
            "image" => Box::new(ImageBuilder),
            "compose" => Box::new(ComposeBuilder),
            _ => Box::new(NixpacksBuilder),
        };
    }

    let candidates: Vec<Box<dyn Builder>> = vec![
        Box::new(ComposeBuilder),
        Box::new(DockerfileBuilder),
        Box::new(PackBuilder),
    ];

    for b in candidates {
        if b.detect(source) {
            return b;
        }
    }

    Box::new(NixpacksBuilder)
}

// ── Helpers ───────────────────────────────────────────────────────────────────

async fn get_exposed_ports(docker: &Docker, image_tag: &str) -> Vec<u16> {
    let Ok(info) = docker.inspect_image(image_tag).await else {
        return vec![];
    };

    info.config
        .as_ref()
        .and_then(|c| c.exposed_ports.as_ref())
        .map(|ports: &Vec<String>| {
            ports
                .iter()
                .filter_map(|p| p.split('/').next().and_then(|n| n.parse::<u16>().ok()))
                .collect()
        })
        .unwrap_or_default()
}

/// Spin up an ephemeral container to read the Procfile from the image.
async fn extract_procfile(docker: &Docker, image_tag: &str) -> Vec<ProcfileEntry> {
    let name = format!(
        "deku-procfile-{}",
        &uuid::Uuid::new_v4().simple().to_string()[..8]
    );

    let config = bollard::models::ContainerCreateBody {
        image: Some(image_tag.to_string()),
        cmd: Some(vec![
            "sh".to_string(),
            "-c".to_string(),
            "cat /app/Procfile 2>/dev/null || cat /Procfile 2>/dev/null || true".to_string(),
        ]),
        ..Default::default()
    };

    let opts = CreateContainerOptionsBuilder::default().name(&name).build();
    let Ok(create_resp) = docker.create_container(Some(opts), config).await else {
        return vec![];
    };

    let id = &create_resp.id;

    let _ = docker
        .start_container(
            id,
            None::<bollard::query_parameters::StartContainerOptions>,
        )
        .await;

    let wait_opts = WaitContainerOptionsBuilder::default().build();
    let mut wait_stream = docker.wait_container(id, Some(wait_opts));
    let _ = wait_stream.next().await;

    let log_opts = LogsOptionsBuilder::default().stdout(true).build();
    let mut content = String::new();
    let mut log_stream = docker.logs(id, Some(log_opts));
    while let Some(Ok(LogOutput::StdOut { message })) = log_stream.next().await {
        content.push_str(&String::from_utf8_lossy(&message));
    }

    let rm_opts = RemoveContainerOptionsBuilder::default().force(true).build();
    let _ = docker.remove_container(id, Some(rm_opts)).await;

    if content.is_empty() {
        vec![]
    } else {
        parse_procfile(&content)
    }
}

// ── Dockerfile builder ────────────────────────────────────────────────────────

pub struct DockerfileBuilder;

#[async_trait]
impl Builder for DockerfileBuilder {
    fn name(&self) -> &'static str {
        "dockerfile"
    }

    fn detect(&self, source: &Path) -> bool {
        source.join("Dockerfile").exists()
            || source.join("Dockerfile.deku").exists()
            || source.join("dockerfile").exists()
    }

    async fn build(
        &self,
        ctx: &BuildContext,
        deku_toml: Option<&DekuToml>,
        docker: &Docker,
        events: &EventSender,
    ) -> Result<BuiltImage> {
        let app_name = &ctx.app.name;
        let image_tag = format!("deku/{app_name}:latest");

        let dockerfile = deku_toml
            .and_then(|t| t.build.as_ref())
            .and_then(|b| b.dockerfile.as_deref())
            .unwrap_or("Dockerfile");

        let context_path = deku_toml
            .and_then(|t| t.build.as_ref())
            .and_then(|b| b.context.as_deref())
            .map(|c| ctx.source_dir.join(c))
            .unwrap_or_else(|| ctx.source_dir.clone());

        let buildargs: HashMap<String, String> = deku_toml
            .and_then(|t| t.build.as_ref())
            .and_then(|b| b.args.as_ref())
            .cloned()
            .unwrap_or_default();

        events.emit(
            Some(ctx.app.id.clone()),
            "build.started",
            Some(serde_json::json!({ "builder": "dockerfile", "app": app_name })),
        );

        let tar_bytes =
            create_tar_gz(&context_path).map_err(|e| DekuError::BuildFailed(e.to_string()))?;

        let mut build_opts_builder = BuildImageOptionsBuilder::default()
            .dockerfile(dockerfile)
            .t(&image_tag)
            .rm(true);

        if !buildargs.is_empty() {
            build_opts_builder = build_opts_builder.buildargs(&buildargs);
        }

        let build_opts = build_opts_builder.build();

        let mut stream = docker.build_image(build_opts, None, Some(body_full(tar_bytes)));
        let mut image_id = String::new();

        while let Some(item) = stream.next().await {
            match item {
                Ok(info) => {
                    if let Some(stream_line) = info.stream {
                        let line = stream_line.trim_end().to_string();
                        if !line.is_empty() {
                            events.emit(
                                Some(ctx.app.id.clone()),
                                "build.log",
                                Some(serde_json::json!({ "line": line })),
                            );
                        }
                    }
                    if let Some(aux) = info.aux {
                        if let Some(id) = aux.id {
                            image_id = id;
                        }
                    }
                    if let Some(err) = info.error_detail.and_then(|e| e.message) {
                        return Err(DekuError::BuildFailed(err));
                    }
                }
                Err(e) => return Err(DekuError::BuildFailed(e.to_string())),
            }
        }

        if image_id.is_empty() {
            if let Ok(info) = docker.inspect_image(&image_tag).await {
                image_id = info.id.unwrap_or_default();
            }
        }

        let exposed_ports = get_exposed_ports(docker, &image_tag).await;
        let procfile = extract_procfile(docker, &image_tag).await;

        events.emit(
            Some(ctx.app.id.clone()),
            "build.complete",
            Some(serde_json::json!({ "image_tag": image_tag, "builder": "dockerfile" })),
        );

        Ok(BuiltImage {
            image_id,
            tag: image_tag,
            exposed_ports,
            procfile,
        })
    }
}

// ── Nixpacks builder ──────────────────────────────────────────────────────────

pub struct NixpacksBuilder;

#[async_trait]
impl Builder for NixpacksBuilder {
    fn name(&self) -> &'static str {
        "nixpacks"
    }

    fn detect(&self, _source: &Path) -> bool {
        false // Nixpacks is the fallback
    }

    async fn build(
        &self,
        ctx: &BuildContext,
        _deku_toml: Option<&DekuToml>,
        docker: &Docker,
        events: &EventSender,
    ) -> Result<BuiltImage> {
        let app_name = &ctx.app.name;
        let image_tag = format!("deku/{app_name}:latest");

        events.emit(
            Some(ctx.app.id.clone()),
            "build.started",
            Some(serde_json::json!({ "builder": "nixpacks", "app": app_name })),
        );

        let output = tokio::process::Command::new("nixpacks")
            .args([
                "build",
                ctx.source_dir.to_str().unwrap_or("."),
                "--name",
                &image_tag,
            ])
            .output()
            .await
            .map_err(|e| DekuError::BuildFailed(format!("nixpacks not found: {e}")))?;

        for line in String::from_utf8_lossy(&output.stdout).lines() {
            events.emit(
                Some(ctx.app.id.clone()),
                "build.log",
                Some(serde_json::json!({ "line": line })),
            );
        }

        if !output.status.success() {
            return Err(DekuError::BuildFailed(format!(
                "nixpacks build failed: {}",
                String::from_utf8_lossy(&output.stderr)
            )));
        }

        let exposed_ports = get_exposed_ports(docker, &image_tag).await;
        let procfile = extract_procfile(docker, &image_tag).await;

        events.emit(
            Some(ctx.app.id.clone()),
            "build.complete",
            Some(serde_json::json!({ "image_tag": image_tag, "builder": "nixpacks" })),
        );

        Ok(BuiltImage {
            image_id: image_tag.clone(),
            tag: image_tag,
            exposed_ports,
            procfile,
        })
    }
}

// ── Pack (CNB) builder ────────────────────────────────────────────────────────

pub struct PackBuilder;

#[async_trait]
impl Builder for PackBuilder {
    fn name(&self) -> &'static str {
        "pack"
    }

    fn detect(&self, source: &Path) -> bool {
        source.join("project.toml").exists()
    }

    async fn build(
        &self,
        ctx: &BuildContext,
        _deku_toml: Option<&DekuToml>,
        docker: &Docker,
        events: &EventSender,
    ) -> Result<BuiltImage> {
        let app_name = &ctx.app.name;
        let image_tag = format!("deku/{app_name}:latest");

        events.emit(
            Some(ctx.app.id.clone()),
            "build.started",
            Some(serde_json::json!({ "builder": "pack", "app": app_name })),
        );

        let output = tokio::process::Command::new("pack")
            .args([
                "build",
                &image_tag,
                "--path",
                ctx.source_dir.to_str().unwrap_or("."),
            ])
            .output()
            .await
            .map_err(|e| DekuError::BuildFailed(format!("pack not found: {e}")))?;

        for line in String::from_utf8_lossy(&output.stdout).lines() {
            events.emit(
                Some(ctx.app.id.clone()),
                "build.log",
                Some(serde_json::json!({ "line": line })),
            );
        }

        if !output.status.success() {
            return Err(DekuError::BuildFailed(format!(
                "pack build failed: {}",
                String::from_utf8_lossy(&output.stderr)
            )));
        }

        let exposed_ports = get_exposed_ports(docker, &image_tag).await;
        let procfile = extract_procfile(docker, &image_tag).await;

        events.emit(
            Some(ctx.app.id.clone()),
            "build.complete",
            Some(serde_json::json!({ "image_tag": image_tag, "builder": "pack" })),
        );

        Ok(BuiltImage {
            image_id: image_tag.clone(),
            tag: image_tag,
            exposed_ports,
            procfile,
        })
    }
}

// ── Pre-built image ───────────────────────────────────────────────────────────

pub struct ImageBuilder;

#[async_trait]
impl Builder for ImageBuilder {
    fn name(&self) -> &'static str {
        "image"
    }

    fn detect(&self, _source: &Path) -> bool {
        false
    }

    async fn build(
        &self,
        ctx: &BuildContext,
        _deku_toml: Option<&DekuToml>,
        docker: &Docker,
        events: &EventSender,
    ) -> Result<BuiltImage> {
        let image_tag = format!("deku/{}:latest", ctx.app.name);

        events.emit(
            Some(ctx.app.id.clone()),
            "build.started",
            Some(serde_json::json!({ "builder": "image", "app": ctx.app.name })),
        );

        let exposed_ports = get_exposed_ports(docker, &image_tag).await;
        let procfile = extract_procfile(docker, &image_tag).await;

        Ok(BuiltImage {
            image_id: image_tag.clone(),
            tag: image_tag,
            exposed_ports,
            procfile,
        })
    }
}

// ── Docker Compose builder ────────────────────────────────────────────────────

pub struct ComposeBuilder;

#[async_trait]
impl Builder for ComposeBuilder {
    fn name(&self) -> &'static str {
        "compose"
    }

    fn detect(&self, source: &Path) -> bool {
        ["docker-compose.yml", "docker-compose.yaml", "compose.yml", "compose.yaml"]
            .iter()
            .any(|f| source.join(f).exists())
    }

    async fn build(
        &self,
        ctx: &BuildContext,
        _deku_toml: Option<&DekuToml>,
        docker: &Docker,
        events: &EventSender,
    ) -> Result<BuiltImage> {
        let app_name = &ctx.app.name;

        let compose_path = ["docker-compose.yml", "docker-compose.yaml", "compose.yml", "compose.yaml"]
            .iter()
            .map(|f| ctx.source_dir.join(f))
            .find(|p| p.exists())
            .ok_or_else(|| DekuError::BuildFailed("no compose file found".to_string()))?;

        let compose_content = std::fs::read_to_string(&compose_path)
            .map_err(|e| DekuError::BuildFailed(e.to_string()))?;

        let compose: serde_yaml::Value = serde_yaml::from_str(&compose_content)
            .map_err(|e| DekuError::BuildFailed(format!("invalid compose file: {e}")))?;

        let services = compose["services"]
            .as_mapping()
            .ok_or_else(|| DekuError::BuildFailed("no services in compose file".to_string()))?;

        // Find web service: prefer `web`, then `x-deku-web: true`, then first service
        let web_service_name = services
            .keys()
            .find(|k| k.as_str() == Some("web"))
            .or_else(|| {
                services.keys().find(|k| {
                    k.as_str().map_or(false, |name| {
                        compose["services"][name]
                            .get("x-deku-web")
                            .and_then(|v| v.as_bool())
                            .unwrap_or(false)
                    })
                })
            })
            .or_else(|| services.keys().next())
            .and_then(|k| k.as_str())
            .ok_or_else(|| DekuError::BuildFailed("no service found in compose".to_string()))?;

        let web_service = &compose["services"][web_service_name];
        let image_tag = format!("deku/{app_name}:latest");

        events.emit(
            Some(ctx.app.id.clone()),
            "build.started",
            Some(serde_json::json!({ "builder": "compose", "service": web_service_name })),
        );

        if web_service.get("build").is_some() {
            let build_context = web_service["build"]
                .get("context")
                .and_then(|v| v.as_str())
                .unwrap_or(".");
            let dockerfile = web_service["build"]
                .get("dockerfile")
                .and_then(|v| v.as_str())
                .unwrap_or("Dockerfile");

            let full_context = ctx.source_dir.join(build_context);
            let tar_bytes = create_tar_gz(&full_context)
                .map_err(|e| DekuError::BuildFailed(e.to_string()))?;

            let build_opts = BuildImageOptionsBuilder::default()
                .dockerfile(dockerfile)
                .t(&image_tag)
                .rm(true)
                .build();

            let mut stream = docker.build_image(build_opts, None, Some(body_full(tar_bytes)));
            while let Some(item) = stream.next().await {
                match item {
                    Ok(info) => {
                        if let Some(line) = info.stream {
                            let line = line.trim_end().to_string();
                            if !line.is_empty() {
                                events.emit(
                                    Some(ctx.app.id.clone()),
                                    "build.log",
                                    Some(serde_json::json!({ "line": line })),
                                );
                            }
                        }
                        if let Some(err) = info.error_detail.and_then(|e| e.message) {
                            return Err(DekuError::BuildFailed(err));
                        }
                    }
                    Err(e) => return Err(DekuError::BuildFailed(e.to_string())),
                }
            }
        } else if let Some(image) = web_service.get("image").and_then(|v| v.as_str()) {
            let (repo, tag) = image.rsplit_once(':').unwrap_or((image, "latest"));
            tag_image(docker, &format!("{repo}:{tag}"), &format!("deku/{app_name}"), "latest")
                .await
                .map_err(|e| DekuError::BuildFailed(e.to_string()))?;
        }

        let exposed_ports = get_exposed_ports(docker, &image_tag).await;
        let procfile = extract_procfile(docker, &image_tag).await;

        events.emit(
            Some(ctx.app.id.clone()),
            "build.complete",
            Some(serde_json::json!({ "image_tag": image_tag, "builder": "compose" })),
        );

        Ok(BuiltImage {
            image_id: image_tag.clone(),
            tag: image_tag,
            exposed_ports,
            procfile,
        })
    }
}
