use std::collections::HashMap;

use bollard::container::LogOutput;
use bollard::models::{
    HostConfig, Mount, MountTypeEnum, PortBinding, PortMap, RestartPolicy, RestartPolicyNameEnum,
};
use bollard::query_parameters::{
    CreateContainerOptionsBuilder, CreateImageOptionsBuilder, LogsOptionsBuilder,
    RemoveContainerOptionsBuilder, StopContainerOptionsBuilder, TagImageOptionsBuilder,
};
use bollard::Docker;
use bytes::Bytes;
use futures::StreamExt;

pub use bollard::Docker as DockerClient;

/// Connect to the Docker daemon using the system default socket.
pub fn connect() -> anyhow::Result<Docker> {
    Docker::connect_with_socket_defaults()
        .map_err(|e| anyhow::anyhow!("failed to connect to docker daemon: {e}"))
}

pub struct ContainerSpec<'a> {
    pub image: &'a str,
    /// Unique container name — e.g. `deku.myapp.web.abc123`
    pub name: &'a str,
    /// `KEY=VALUE` environment variable pairs
    pub env: Vec<String>,
    /// `{ "3000/tcp": host_port }` port bindings
    pub port_bindings: HashMap<String, u16>,
    /// Bind mounts as `(host_path, container_path)`
    pub volumes: Vec<(String, String)>,
    /// Memory limit in bytes
    pub memory: Option<i64>,
    /// CPU quota (microseconds per 100ms period)
    pub cpu_quota: Option<i64>,
    /// Command override (Procfile entry)
    pub cmd: Option<Vec<String>>,
}

/// Create and start a container. Returns the container ID.
pub async fn start_container(docker: &Docker, spec: &ContainerSpec<'_>) -> anyhow::Result<String> {
    let port_bindings: PortMap = spec
        .port_bindings
        .iter()
        .map(|(container_port_proto, host_port)| {
            (
                container_port_proto.clone(),
                Some(vec![PortBinding {
                    host_ip: Some("0.0.0.0".to_string()),
                    host_port: Some(host_port.to_string()),
                }]),
            )
        })
        .collect();

    let mounts: Vec<Mount> = spec
        .volumes
        .iter()
        .map(|(host, container)| Mount {
            source: Some(host.clone()),
            target: Some(container.clone()),
            typ: Some(MountTypeEnum::BIND),
            read_only: Some(false),
            ..Default::default()
        })
        .collect();

    let host_config = HostConfig {
        port_bindings: Some(port_bindings),
        mounts: Some(mounts),
        memory: spec.memory,
        cpu_quota: spec.cpu_quota,
        restart_policy: Some(RestartPolicy {
            name: Some(RestartPolicyNameEnum::UNLESS_STOPPED),
            maximum_retry_count: None,
        }),
        ..Default::default()
    };

    let config = bollard::models::ContainerCreateBody {
        image: Some(spec.image.to_string()),
        env: Some(spec.env.clone()),
        host_config: Some(host_config),
        cmd: spec.cmd.clone(),
        labels: Some(HashMap::from([(
            "deku.managed".to_string(),
            "true".to_string(),
        )])),
        ..Default::default()
    };

    let options = CreateContainerOptionsBuilder::default()
        .name(spec.name)
        .build();

    let resp = docker.create_container(Some(options), config).await?;
    docker
        .start_container(
            &resp.id,
            None::<bollard::query_parameters::StartContainerOptions>,
        )
        .await?;

    tracing::info!(container_id = %resp.id, name = spec.name, "container started");
    Ok(resp.id)
}

/// Stop a container gracefully. `timeout_secs` is seconds before SIGKILL.
pub async fn stop_container(docker: &Docker, id: &str, timeout_secs: i32) -> anyhow::Result<()> {
    let opts = StopContainerOptionsBuilder::default()
        .t(timeout_secs)
        .build();
    docker.stop_container(id, Some(opts)).await?;
    tracing::info!(container_id = id, "container stopped");
    Ok(())
}

/// Force-remove a container.
pub async fn remove_container(docker: &Docker, id: &str) -> anyhow::Result<()> {
    let opts = RemoveContainerOptionsBuilder::default().force(true).build();
    docker.remove_container(id, Some(opts)).await?;
    tracing::info!(container_id = id, "container removed");
    Ok(())
}

/// Pull an image from a registry. Calls `on_progress` for each status message.
pub async fn pull_image(
    docker: &Docker,
    image: &str,
    on_progress: impl Fn(&str),
) -> anyhow::Result<()> {
    let (image_name, tag) = image.rsplit_once(':').unwrap_or((image, "latest"));

    let opts = CreateImageOptionsBuilder::default()
        .from_image(image_name)
        .tag(tag)
        .build();

    let mut stream = docker.create_image(Some(opts), None, None);
    while let Some(item) = stream.next().await {
        let info = item?;
        if let Some(status) = &info.status {
            on_progress(status);
        }
    }

    Ok(())
}

/// Collect up to `tail` log lines from a container.
pub async fn get_container_logs(
    docker: &Docker,
    id: &str,
    tail: usize,
) -> anyhow::Result<Vec<String>> {
    let opts = LogsOptionsBuilder::default()
        .stdout(true)
        .stderr(true)
        .tail(&tail.to_string())
        .build();

    let mut lines = Vec::new();
    let mut stream = docker.logs(id, Some(opts));

    while let Some(item) = stream.next().await {
        match item? {
            LogOutput::StdOut { message } | LogOutput::StdErr { message } => {
                lines.push(String::from_utf8_lossy(&message).trim_end().to_string());
            }
            _ => {}
        }
    }

    Ok(lines)
}

/// Build a tar.gz archive of `source_dir` in memory.
pub fn create_tar_gz(source_dir: &std::path::Path) -> anyhow::Result<Bytes> {
    let mut buf = Vec::new();
    {
        let enc = flate2::write::GzEncoder::new(&mut buf, flate2::Compression::default());
        let mut tar = tar::Builder::new(enc);
        tar.append_dir_all(".", source_dir)?;
        let enc = tar.into_inner()?;
        enc.finish()?;
    }
    Ok(Bytes::from(buf))
}

/// Tag an existing image with a new repo:tag.
pub async fn tag_image(docker: &Docker, source: &str, repo: &str, tag: &str) -> anyhow::Result<()> {
    let opts = TagImageOptionsBuilder::default()
        .repo(repo)
        .tag(tag)
        .build();
    docker.tag_image(source, Some(opts)).await?;
    Ok(())
}
