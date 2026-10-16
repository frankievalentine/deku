use std::collections::HashMap;

use bollard::container::LogOutput;
use bollard::models::{
    HostConfig, Mount, MountType, PortBinding, PortMap, RestartPolicy, RestartPolicyNameEnum,
};
use bollard::query_parameters::{
    CreateContainerOptionsBuilder, CreateImageOptionsBuilder, LogsOptionsBuilder,
    RemoveContainerOptionsBuilder, StopContainerOptionsBuilder, TagImageOptionsBuilder,
};
use bollard::Docker;
use bytes::Bytes;
use futures::StreamExt;

pub use bollard::Docker as DockerClient;

/// The bridge network every Deku-managed app and service container joins.
///
/// Apps reach managed services on it by container name, so a service needs no
/// published host port. Docker's embedded DNS only answers inside containers,
/// which is why web replicas still publish a host port for Angie, bound to
/// loopback so it is not reachable from outside the host.
pub const DEKU_NETWORK: &str = deku_core::types::DEKU_NETWORK;

/// Interface web replicas publish their port on. Loopback, so traffic can only
/// arrive through Angie and its auth, maintenance and redirect rules.
pub const PUBLISH_HOST_IP: &str = "127.0.0.1";

/// Create the shared Deku network if it does not exist yet.
pub async fn ensure_deku_network(docker: &Docker) -> anyhow::Result<()> {
    if docker
        .inspect_network(
            DEKU_NETWORK,
            None::<bollard::query_parameters::InspectNetworkOptions>,
        )
        .await
        .is_ok()
    {
        return Ok(());
    }

    let created = docker
        .create_network(bollard::models::NetworkCreateRequest {
            name: DEKU_NETWORK.to_string(),
            driver: Some("bridge".to_string()),
            labels: Some(HashMap::from([(
                "deku.managed".to_string(),
                "true".to_string(),
            )])),
            ..Default::default()
        })
        .await;
    match created {
        Ok(_) => {
            tracing::info!(network = DEKU_NETWORK, "created the shared Deku network");
            Ok(())
        }
        // Another deploy or service creation got there first.
        Err(bollard::errors::Error::DockerResponseServerError {
            status_code: 409, ..
        }) => Ok(()),
        Err(error) => Err(error.into()),
    }
}

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

/// The environment an app process starts with: the app's own variables, plus
/// the setting that makes Docker's init signal the whole process group.
fn process_env(app_env: &[String]) -> Vec<String> {
    let mut env = app_env.to_vec();
    env.push("TINI_KILL_PROCESS_GROUP=1".to_string());
    env
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
                    host_ip: Some(PUBLISH_HOST_IP.to_string()),
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
            typ: Some(MountType::BIND),
            read_only: Some(false),
            ..Default::default()
        })
        .collect();

    let host_config = HostConfig {
        network_mode: Some(DEKU_NETWORK.to_string()),
        // A Procfile command runs as `sh -c <command>`, and sh does not pass
        // SIGTERM on to what it started, so a stop would wait out its whole
        // timeout and then kill the app mid-request or mid-job. Docker's init
        // runs as PID 1 instead and, with TINI_KILL_PROCESS_GROUP, signals the
        // shell's whole process group, which reaches every process of a
        // compound command such as `cd app && node server.js`.
        init: Some(true),
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
        env: Some(process_env(&spec.env)),
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
    if let Err(error) = docker
        .start_container(
            &resp.id,
            None::<bollard::query_parameters::StartContainerOptions>,
        )
        .await
    {
        // Leave nothing behind under this name, so a retry can reuse it.
        let _ = remove_container(docker, &resp.id).await;
        return Err(error.into());
    }

    tracing::info!(container_id = %resp.id, name = spec.name, "container started");
    Ok(resp.id)
}

/// Whether a container failed to start because its host port was taken.
pub fn is_port_conflict(error: &anyhow::Error) -> bool {
    let message = error.to_string();
    message.contains("port is already allocated") || message.contains("address already in use")
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

/// Return true when an image reference already exists in the local Docker daemon.
pub async fn image_exists(docker: &Docker, reference: &str) -> bool {
    docker.inspect_image(reference).await.is_ok()
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

#[cfg(test)]
mod tests {
    use super::is_port_conflict;

    #[test]
    fn recognizes_a_taken_host_port() {
        let allocated = anyhow::anyhow!(
            "Docker responded with status code 500: driver failed programming external connectivity on endpoint deku.web: Bind for 127.0.0.1:49153 failed: port is already allocated"
        );
        assert!(is_port_conflict(&allocated));
        assert!(is_port_conflict(&anyhow::anyhow!(
            "listen tcp4 127.0.0.1:49153: bind: address already in use"
        )));
        assert!(!is_port_conflict(&anyhow::anyhow!(
            "No such image: app:latest"
        )));
    }

    #[tokio::test]
    async fn a_compound_command_stops_on_sigterm() {
        if std::env::var("DEKU_DOCKER_IT").is_err() {
            return;
        }
        let docker = super::connect().expect("docker");
        super::ensure_deku_network(&docker).await.expect("network");
        super::pull_image(&docker, "alpine:3.22", |_| {})
            .await
            .expect("pull alpine");

        let name = format!(
            "deku.it.sigterm.{}",
            &uuid::Uuid::new_v4().simple().to_string()[..8]
        );
        // The shell runs `sleep` as a child, so SIGTERM only reaches it when the
        // whole process group is signaled.
        let spec = super::ContainerSpec {
            image: "alpine:3.22",
            name: &name,
            env: Vec::new(),
            port_bindings: std::collections::HashMap::new(),
            volumes: Vec::new(),
            memory: None,
            cpu_quota: None,
            cmd: Some(vec![
                "sh".to_string(),
                "-c".to_string(),
                "cd /tmp && sleep 600".to_string(),
            ]),
        };
        let id = super::start_container(&docker, &spec).await.expect("start");

        let started = std::time::Instant::now();
        super::stop_container(&docker, &id, 20).await.expect("stop");
        let took = started.elapsed();
        super::remove_container(&docker, &id).await.expect("remove");

        assert!(
            took < std::time::Duration::from_secs(10),
            "stopping took {took:?}, so SIGTERM never reached the command"
        );
    }
}
