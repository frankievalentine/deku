//! One-off command execution: `deku run` (fresh container from the app image)
//! and `deku exec` (inside an already-running app container).

use std::collections::HashMap;

use anyhow::Result;
use bollard::container::LogOutput;
use bollard::exec::{CreateExecOptions, StartExecOptions, StartExecResults};
use bollard::models::{ContainerCreateBody, HostConfig, Mount, MountType};
use bollard::query_parameters::{
    CreateContainerOptionsBuilder, InspectContainerOptions, LogsOptionsBuilder,
    RemoveContainerOptionsBuilder, StartContainerOptions,
};
use bollard::Docker;
use futures::StreamExt;

/// Which stream a chunk of command output came from.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum OutputStream {
    Stdout,
    Stderr,
}

impl OutputStream {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Stdout => "stdout",
            Self::Stderr => "stderr",
        }
    }
}

/// Run a command in a fresh, non-interactive container created from the app's
/// image, streaming output until it exits. The container is always removed.
pub async fn run_once(
    docker: &Docker,
    name: &str,
    image: &str,
    env: &[String],
    volumes: &[(String, String)],
    command: &[String],
    mut on_output: impl FnMut(OutputStream, &str),
) -> Result<i32> {
    let mounts: Vec<Mount> = volumes
        .iter()
        .map(|(host, target)| Mount {
            source: Some(host.clone()),
            target: Some(target.clone()),
            typ: Some(MountType::BIND),
            read_only: Some(false),
            ..Default::default()
        })
        .collect();

    let body = ContainerCreateBody {
        image: Some(image.to_string()),
        env: Some(env.to_vec()),
        cmd: Some(command.to_vec()),
        labels: Some(HashMap::from([(
            "deku.managed".to_string(),
            "true".to_string(),
        )])),
        host_config: Some(HostConfig {
            mounts: Some(mounts),
            ..Default::default()
        }),
        ..Default::default()
    };

    let options = CreateContainerOptionsBuilder::default().name(name).build();
    let created = docker.create_container(Some(options), body).await?;
    let id = created.id;

    if let Err(error) = docker
        .start_container(&id, None::<StartContainerOptions>)
        .await
    {
        let _ = remove(docker, &id).await;
        return Err(error.into());
    }

    let log_options = LogsOptionsBuilder::default()
        .stdout(true)
        .stderr(true)
        .follow(true)
        .build();
    {
        let mut stream = docker.logs(&id, Some(log_options));
        while let Some(item) = stream.next().await {
            match item {
                Ok(LogOutput::StdOut { message }) => {
                    on_output(OutputStream::Stdout, &String::from_utf8_lossy(&message))
                }
                Ok(LogOutput::StdErr { message }) => {
                    on_output(OutputStream::Stderr, &String::from_utf8_lossy(&message))
                }
                Ok(_) => {}
                Err(error) => {
                    let _ = remove(docker, &id).await;
                    return Err(error.into());
                }
            }
        }
    }

    let exit_code = docker
        .inspect_container(&id, None::<InspectContainerOptions>)
        .await
        .ok()
        .and_then(|info| info.state.and_then(|state| state.exit_code))
        .unwrap_or(0) as i32;

    let _ = remove(docker, &id).await;
    Ok(exit_code)
}

/// Run a command inside an already-running container, streaming output until it
/// exits.
pub async fn exec_in(
    docker: &Docker,
    container_id: &str,
    command: &[String],
    mut on_output: impl FnMut(OutputStream, &str),
) -> Result<i32> {
    let exec = docker
        .create_exec(
            container_id,
            CreateExecOptions {
                attach_stdout: Some(true),
                attach_stderr: Some(true),
                cmd: Some(command.to_vec()),
                ..Default::default()
            },
        )
        .await?;

    match docker
        .start_exec(&exec.id, None::<StartExecOptions>)
        .await?
    {
        StartExecResults::Attached { mut output, .. } => {
            while let Some(item) = output.next().await {
                match item? {
                    LogOutput::StdOut { message } => {
                        on_output(OutputStream::Stdout, &String::from_utf8_lossy(&message))
                    }
                    LogOutput::StdErr { message } => {
                        on_output(OutputStream::Stderr, &String::from_utf8_lossy(&message))
                    }
                    _ => {}
                }
            }
        }
        StartExecResults::Detached => {}
    }

    let inspect = docker.inspect_exec(&exec.id).await?;
    Ok(inspect.exit_code.unwrap_or(0) as i32)
}

async fn remove(docker: &Docker, id: &str) -> Result<()> {
    let options = RemoveContainerOptionsBuilder::default().force(true).build();
    docker.remove_container(id, Some(options)).await?;
    Ok(())
}
