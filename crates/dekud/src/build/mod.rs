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

use crate::container::{create_tar_gz, tag_image};
use crate::events::EventSender;
use bollard::body_full;

// ── Output types ──────────────────────────────────────────────────────────────

pub struct BuiltImage {
    pub tag: String,
    pub exposed_ports: Vec<u16>,
    pub procfile: Vec<ProcfileEntry>,
}

pub fn deployment_image_repo(app_name: &str) -> String {
    format!("deku/{app_name}")
}

pub fn deployment_image_id(deploy_id: &str) -> String {
    deploy_id
        .chars()
        .filter(|c| c.is_ascii_hexdigit())
        .take(12)
        .collect()
}

pub fn deployment_image_tag(app_name: &str, deploy_id: &str) -> String {
    format!(
        "{}:{}",
        deployment_image_repo(app_name),
        deployment_image_id(deploy_id)
    )
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
        image_tag: &str,
    ) -> Result<BuiltImage>;
}

// ── Auto-detect ───────────────────────────────────────────────────────────────

pub fn select_builder(source: &Path, forced: Option<&str>) -> Result<Box<dyn Builder>> {
    if let Some(name) = forced {
        return match name {
            "dockerfile" => Ok(Box::new(DockerfileBuilder)),
            "railpack" => Ok(Box::new(RailpackBuilder)),
            "pack" => Ok(Box::new(PackBuilder)),
            "image" => Ok(Box::new(ImageBuilder)),
            "compose" => Ok(Box::new(ComposeBuilder)),
            "nixpacks" => Err(DekuError::BuildFailed(
                "builder 'nixpacks' was replaced by 'railpack'; set builder = \"railpack\" in deku.toml"
                    .to_string(),
            )),
            _ => Err(DekuError::BuildFailed(format!(
                "unknown builder '{name}'; expected one of: dockerfile, railpack, pack, image, compose"
            ))),
        };
    }

    let candidates: Vec<Box<dyn Builder>> = vec![
        Box::new(ComposeBuilder),
        Box::new(DockerfileBuilder),
        Box::new(PackBuilder),
    ];

    for b in candidates {
        if b.detect(source) {
            return Ok(b);
        }
    }

    // Railpack is the general-purpose fallback for sources without a Dockerfile,
    // Compose file, or pack config, matching the previous Nixpacks behavior.
    Ok(Box::new(RailpackBuilder))
}

// ── Helpers ───────────────────────────────────────────────────────────────────

pub(crate) async fn get_exposed_ports(docker: &Docker, image_tag: &str) -> Vec<u16> {
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
        .start_container(id, None::<bollard::query_parameters::StartContainerOptions>)
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
        image_tag: &str,
    ) -> Result<BuiltImage> {
        let app_name = &ctx.app.name;

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
            .t(image_tag)
            .rm(true);

        if !buildargs.is_empty() {
            build_opts_builder = build_opts_builder.buildargs(&buildargs);
        }

        let build_opts = build_opts_builder.build();

        let mut stream = docker.build_image(build_opts, None, Some(body_full(tar_bytes)));
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
                        let _ = aux.id;
                    }
                    if let Some(err) = info.error_detail.and_then(|e| e.message) {
                        return Err(DekuError::BuildFailed(err));
                    }
                }
                Err(e) => return Err(DekuError::BuildFailed(e.to_string())),
            }
        }

        let exposed_ports = get_exposed_ports(docker, image_tag).await;
        let procfile = extract_procfile(docker, image_tag).await;

        events.emit(
            Some(ctx.app.id.clone()),
            "build.complete",
            Some(serde_json::json!({ "image_tag": image_tag, "builder": "dockerfile" })),
        );

        Ok(BuiltImage {
            tag: image_tag.to_string(),
            exposed_ports,
            procfile,
        })
    }
}

// ── Railpack builder ──────────────────────────────────────────────────────────

pub struct RailpackBuilder;

#[async_trait]
impl Builder for RailpackBuilder {
    fn name(&self) -> &'static str {
        "railpack"
    }

    fn detect(&self, _source: &Path) -> bool {
        false
    }

    async fn build(
        &self,
        ctx: &BuildContext,
        deku_toml: Option<&DekuToml>,
        _docker: &Docker,
        events: &EventSender,
        image_tag: &str,
    ) -> Result<BuiltImage> {
        let app_name = &ctx.app.name;
        let source = ctx.source_dir.to_str().unwrap_or(".").to_string();
        let buildkit_host = ctx.buildkit_host.clone().ok_or_else(|| {
            DekuError::BuildFailed(
                "Railpack requires BuildKit, but no BuildKit host was provided".to_string(),
            )
        })?;

        events.emit(
            Some(ctx.app.id.clone()),
            "build.started",
            Some(serde_json::json!({ "builder": "railpack", "app": app_name })),
        );

        let plan_dir = tempfile::tempdir().map_err(|e| DekuError::BuildFailed(e.to_string()))?;
        let plan_path = plan_dir.path().join("railpack-plan.json");
        let plan_path_str = plan_path.to_string_lossy().to_string();

        let plan_output = tokio::process::Command::new(crate::buildkit::railpack_bin())
            .args(["plan", &source, "-o", &plan_path_str])
            .output()
            .await
            .map_err(|e| DekuError::BuildFailed(format!("railpack not found: {e}")))?;
        emit_output(ctx, events, &plan_output);
        if !plan_output.status.success() {
            return Err(DekuError::BuildFailed(format!(
                "railpack plan failed: {}",
                String::from_utf8_lossy(&plan_output.stderr)
            )));
        }

        let start_command = std::fs::read_to_string(&plan_path)
            .ok()
            .and_then(|contents| serde_json::from_str::<serde_json::Value>(&contents).ok())
            .and_then(|plan| {
                plan.get("deploy")
                    .and_then(|deploy| deploy.get("startCommand"))
                    .and_then(|value| value.as_str())
                    .map(|value| value.to_string())
            });

        if let Some(command) = start_command.as_deref() {
            events.emit(
                Some(ctx.app.id.clone()),
                "build.log",
                Some(serde_json::json!({ "line": format!("Detected start command: {command}") })),
            );
        }

        let mut command = tokio::process::Command::new(crate::buildkit::railpack_bin());
        command
            .args(["build", &source, "--name", image_tag, "--progress", "plain"])
            .env("BUILDKIT_HOST", &buildkit_host);

        if let Some(args) = deku_toml
            .and_then(|t| t.build.as_ref())
            .and_then(|b| b.args.as_ref())
        {
            for (key, value) in args {
                command.args(["--env", &format!("{key}={value}")]);
            }
        }

        let build_output = command
            .output()
            .await
            .map_err(|e| DekuError::BuildFailed(format!("railpack not found: {e}")))?;
        emit_output(ctx, events, &build_output);
        if !build_output.status.success() {
            return Err(DekuError::BuildFailed(format!(
                "railpack build failed: {}",
                String::from_utf8_lossy(&build_output.stderr)
            )));
        }

        let exposed_ports = start_command
            .as_deref()
            .and_then(infer_port_from_start_command)
            .map(|port| vec![port])
            .unwrap_or_default();

        events.emit(
            Some(ctx.app.id.clone()),
            "build.complete",
            Some(serde_json::json!({ "image_tag": image_tag, "builder": "railpack" })),
        );

        Ok(BuiltImage {
            tag: image_tag.to_string(),
            exposed_ports,
            procfile: vec![],
        })
    }
}

fn emit_output(ctx: &BuildContext, events: &EventSender, output: &std::process::Output) {
    for line in String::from_utf8_lossy(&output.stdout).lines() {
        events.emit(
            Some(ctx.app.id.clone()),
            "build.log",
            Some(serde_json::json!({ "line": line })),
        );
    }
    for line in String::from_utf8_lossy(&output.stderr).lines() {
        events.emit(
            Some(ctx.app.id.clone()),
            "build.log",
            Some(serde_json::json!({ "line": line })),
        );
    }
}

pub(crate) fn infer_port_from_start_command(command: &str) -> Option<u16> {
    const MARKERS: [&str; 6] = ["PORT:-", "--port=", "--port ", "-p ", "-p=", "0.0.0.0:"];

    for marker in MARKERS {
        if let Some(index) = command.find(marker) {
            if let Some(port) = leading_port(&command[index + marker.len()..]) {
                return Some(port);
            }
        }
    }

    if let Some(index) = command.find("listen ") {
        if let Some(port) = leading_port(&command[index + "listen ".len()..]) {
            return Some(port);
        }
    }

    None
}

fn leading_port(text: &str) -> Option<u16> {
    let digits: String = text.chars().take_while(|c| c.is_ascii_digit()).collect();
    if digits.is_empty() {
        return None;
    }
    digits.parse::<u16>().ok().filter(|port| *port > 0)
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
        image_tag: &str,
    ) -> Result<BuiltImage> {
        let app_name = &ctx.app.name;

        events.emit(
            Some(ctx.app.id.clone()),
            "build.started",
            Some(serde_json::json!({ "builder": "pack", "app": app_name })),
        );

        let output = tokio::process::Command::new("pack")
            .args([
                "build",
                image_tag,
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

        let exposed_ports = get_exposed_ports(docker, image_tag).await;
        let procfile = extract_procfile(docker, image_tag).await;

        events.emit(
            Some(ctx.app.id.clone()),
            "build.complete",
            Some(serde_json::json!({ "image_tag": image_tag, "builder": "pack" })),
        );

        Ok(BuiltImage {
            tag: image_tag.to_string(),
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
        image_tag: &str,
    ) -> Result<BuiltImage> {
        events.emit(
            Some(ctx.app.id.clone()),
            "build.started",
            Some(serde_json::json!({ "builder": "image", "app": ctx.app.name })),
        );

        let exposed_ports = get_exposed_ports(docker, image_tag).await;
        let procfile = extract_procfile(docker, image_tag).await;

        Ok(BuiltImage {
            tag: image_tag.to_string(),
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
        [
            "docker-compose.yml",
            "docker-compose.yaml",
            "compose.yml",
            "compose.yaml",
        ]
        .iter()
        .any(|f| source.join(f).exists())
    }

    async fn build(
        &self,
        ctx: &BuildContext,
        _deku_toml: Option<&DekuToml>,
        docker: &Docker,
        events: &EventSender,
        image_tag: &str,
    ) -> Result<BuiltImage> {
        let compose_path = [
            "docker-compose.yml",
            "docker-compose.yaml",
            "compose.yml",
            "compose.yaml",
        ]
        .iter()
        .map(|f| ctx.source_dir.join(f))
        .find(|p| p.exists())
        .ok_or_else(|| DekuError::BuildFailed("no compose file found".to_string()))?;

        let compose_content = std::fs::read_to_string(&compose_path)
            .map_err(|e| DekuError::BuildFailed(e.to_string()))?;

        let compose: serde_yaml_ng::Value = serde_yaml_ng::from_str(&compose_content)
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
                    k.as_str().is_some_and(|name| {
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
            let tar_bytes =
                create_tar_gz(&full_context).map_err(|e| DekuError::BuildFailed(e.to_string()))?;

            let build_opts = BuildImageOptionsBuilder::default()
                .dockerfile(dockerfile)
                .t(image_tag)
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
            let (target_repo, target_tag) =
                image_tag.rsplit_once(':').unwrap_or((image_tag, "latest"));
            tag_image(docker, &format!("{repo}:{tag}"), target_repo, target_tag)
                .await
                .map_err(|e| DekuError::BuildFailed(e.to_string()))?;
        }

        let exposed_ports = get_exposed_ports(docker, image_tag).await;
        let procfile = extract_procfile(docker, image_tag).await;

        events.emit(
            Some(ctx.app.id.clone()),
            "build.complete",
            Some(serde_json::json!({ "image_tag": image_tag, "builder": "compose" })),
        );

        Ok(BuiltImage {
            tag: image_tag.to_string(),
            exposed_ports,
            procfile,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::{
        deployment_image_id, deployment_image_tag, infer_port_from_start_command, select_builder,
    };

    #[test]
    fn auto_detect_falls_back_to_railpack_without_manifest() {
        let temp = tempfile::tempdir().expect("tempdir");
        let builder = select_builder(temp.path(), None).expect("builder");
        assert_eq!(builder.name(), "railpack");
    }

    #[test]
    fn auto_detect_prefers_dockerfile_over_railpack() {
        let temp = tempfile::tempdir().expect("tempdir");
        std::fs::write(temp.path().join("Dockerfile"), "FROM scratch\n").expect("write");
        let builder = select_builder(temp.path(), None).expect("builder");
        assert_eq!(builder.name(), "dockerfile");
    }

    #[test]
    fn explicit_nixpacks_is_rejected_with_migration_hint() {
        let temp = tempfile::tempdir().expect("tempdir");
        let error = select_builder(temp.path(), Some("nixpacks"))
            .err()
            .expect("error");
        assert!(error.to_string().contains("railpack"), "{error}");
    }

    #[test]
    fn explicit_unknown_builder_is_rejected() {
        let temp = tempfile::tempdir().expect("tempdir");
        assert!(select_builder(temp.path(), Some("bogus")).is_err());
    }

    #[test]
    fn deployment_image_id_is_deterministic_and_hex() {
        let id = "3f2504e0-4f89-11d3-9a0c-0305e82c3301";
        assert_eq!(deployment_image_id(id), "3f2504e04f89");
        assert_eq!(deployment_image_id(id), deployment_image_id(id));
        assert_eq!(deployment_image_id(id).len(), 12);
        assert!(deployment_image_id(id)
            .chars()
            .all(|c| c.is_ascii_hexdigit()));
    }

    #[test]
    fn deployment_image_tag_is_unique_per_deployment() {
        let first = deployment_image_tag("demo", "3f2504e0-4f89-11d3-9a0c-0305e82c3301");
        let second = deployment_image_tag("demo", "aaaaaaaa-bbbb-cccc-dddd-eeeeeeeeeeee");
        assert_eq!(first, "deku/demo:3f2504e04f89");
        assert_ne!(first, second);
        assert!(!first.ends_with(":latest"));
        assert!(!second.ends_with(":latest"));
    }

    #[test]
    fn infers_port_from_port_default() {
        assert_eq!(
            infer_port_from_start_command("gunicorn config.wsgi --bind 0.0.0.0:${PORT:-8000}"),
            Some(8000)
        );
    }

    #[test]
    fn infers_port_from_flags() {
        assert_eq!(
            infer_port_from_start_command("node server.js --port 4000"),
            Some(4000)
        );
        assert_eq!(
            infer_port_from_start_command("node server.js --port=4100"),
            Some(4100)
        );
        assert_eq!(infer_port_from_start_command("app -p 5000"), Some(5000));
        assert_eq!(
            infer_port_from_start_command("server listen 9000"),
            Some(9000)
        );
    }

    #[test]
    fn returns_no_port_when_start_command_has_none() {
        assert_eq!(infer_port_from_start_command("npm run start"), None);
        assert_eq!(infer_port_from_start_command("serve on $PORT"), None);
    }
}
