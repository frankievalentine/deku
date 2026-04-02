use std::path::PathBuf;
use std::sync::Arc;

use anyhow::Result;
use russh::keys::ssh_key::PublicKey;
use russh::server::{Auth, Msg, Server as RusshServer, Session};
use russh::{Channel, ChannelReadHalf};
use tokio::io::{AsyncReadExt, AsyncWriteExt};

use crate::api::SharedState;
use crate::db::queries;

pub async fn serve(state: SharedState) -> Result<()> {
    let port = state.config.ssh_port;
    let data_dir = state.config.data_dir.clone();

    let host_key = load_or_generate_host_key(&data_dir).await?;

    let server_config = Arc::new(russh::server::Config {
        auth_rejection_time: std::time::Duration::from_secs(3),
        auth_rejection_time_initial: Some(std::time::Duration::from_secs(0)),
        keys: vec![host_key],
        ..Default::default()
    });

    tracing::info!(port, "SSH server listening");

    let mut server = SshServer { state };
    server
        .run_on_address(server_config, ("0.0.0.0", port))
        .await
        .map_err(|e| anyhow::anyhow!("SSH server error: {e}"))
}

async fn load_or_generate_host_key(data_dir: &std::path::Path) -> Result<russh::keys::PrivateKey> {
    use russh::keys::ssh_key::rand_core::OsRng;

    let key_path = data_dir.join("ssh_host_ed25519_key");
    if key_path.exists() {
        let pem = tokio::fs::read_to_string(&key_path).await?;
        let key = russh::keys::PrivateKey::from_openssh(pem.trim())
            .map_err(|e| anyhow::anyhow!("failed to parse host key: {e}"))?;
        return Ok(key);
    }

    let key = russh::keys::PrivateKey::random(&mut OsRng, russh::keys::ssh_key::Algorithm::Ed25519)
        .map_err(|e| anyhow::anyhow!("failed to generate host key: {e}"))?;

    tokio::fs::create_dir_all(data_dir).await?;
    let pem = key
        .to_openssh(russh::keys::ssh_key::LineEnding::LF)
        .map_err(|e| anyhow::anyhow!("failed to serialize host key: {e}"))?;
    tokio::fs::write(&key_path, pem.as_bytes()).await?;

    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let perms = std::fs::Permissions::from_mode(0o600);
        tokio::fs::set_permissions(&key_path, perms).await?;
    }

    Ok(key)
}

#[derive(Clone)]
struct SshServer {
    state: SharedState,
}

impl RusshServer for SshServer {
    type Handler = SshHandler;

    fn new_client(&mut self, peer: Option<std::net::SocketAddr>) -> SshHandler {
        tracing::debug!(?peer, "new SSH client");
        SshHandler {
            state: self.state.clone(),
        }
    }
}

struct SshHandler {
    state: SharedState,
}

impl russh::server::Handler for SshHandler {
    type Error = anyhow::Error;

    async fn auth_publickey(
        &mut self,
        _user: &str,
        public_key: &PublicKey,
    ) -> Result<Auth, Self::Error> {
        let fingerprint = public_key
            .fingerprint(russh::keys::ssh_key::HashAlg::Sha256)
            .to_string();

        match queries::find_ssh_key_by_fingerprint(&self.state.pool, &fingerprint).await {
            Ok(Some(_)) => {
                tracing::info!(%fingerprint, "SSH key accepted");
                Ok(Auth::Accept)
            }
            Ok(None) => {
                tracing::warn!(%fingerprint, "SSH key rejected: not found");
                Ok(Auth::Reject {
                    proceed_with_methods: None,
                    partial_success: false,
                })
            }
            Err(e) => {
                tracing::error!("DB error during SSH auth: {e}");
                Ok(Auth::Reject {
                    proceed_with_methods: None,
                    partial_success: false,
                })
            }
        }
    }

    async fn channel_open_session(
        &mut self,
        channel: Channel<Msg>,
        _session: &mut Session,
    ) -> Result<bool, Self::Error> {
        let state = self.state.clone();
        tokio::spawn(handle_channel(channel, state));
        Ok(true)
    }
}

async fn handle_channel(mut channel: Channel<Msg>, state: SharedState) {
    while let Some(msg) = channel.wait().await {
        if let russh::ChannelMsg::Exec { command, .. } = msg {
            let cmd = String::from_utf8_lossy(&command).to_string();
            tracing::info!(cmd = %cmd, "SSH exec");
            run_git_command(channel, &state, &cmd).await;
            return;
        }
    }
}

async fn run_git_command(channel: Channel<Msg>, state: &SharedState, cmd: &str) {
    let app_name = match parse_git_receive_pack(cmd) {
        Some(n) => n,
        None => {
            tracing::warn!("SSH: unsupported command: {cmd}");
            let _ = channel.close().await;
            return;
        }
    };

    let pool = &state.pool;
    let cfg = &state.config;

    let app = match queries::get_app_by_name(pool, &app_name).await {
        Ok(app) => app,
        Err(e) => {
            tracing::warn!("SSH: app '{app_name}' not found: {e}");
            let _ = channel.close().await;
            return;
        }
    };

    let git_dir = cfg.data_dir.join("git-repos").join(format!("{app_name}.git"));
    if !git_dir.exists() {
        if let Err(e) = init_bare_repo(&git_dir).await {
            tracing::error!("SSH: failed to init git repo: {e}");
            let _ = channel.close().await;
            return;
        }
    }

    let mut child = match tokio::process::Command::new("git-receive-pack")
        .arg(&git_dir)
        .stdin(std::process::Stdio::piped())
        .stdout(std::process::Stdio::piped())
        .stderr(std::process::Stdio::piped())
        .spawn()
    {
        Ok(c) => c,
        Err(e) => {
            tracing::error!("SSH: failed to spawn git-receive-pack: {e}");
            let _ = channel.close().await;
            return;
        }
    };

    let child_stdin = child.stdin.take().expect("stdin");
    let mut child_stdout = child.stdout.take().expect("stdout");
    let mut child_stderr = child.stderr.take().expect("stderr");

    // Split channel so the read half can be owned by a spawned task
    let (read_half, write_half) = channel.split();

    // writer handles are 'static (don't borrow write_half)
    let mut ssh_writer = write_half.make_writer();
    let mut ssh_stderr_writer = write_half.make_writer_ext(Some(1));

    // ssh → child stdin: spawn with owned read_half
    let stdin_task = tokio::spawn(pipe_read_to_write(read_half, child_stdin));

    // child stdout → ssh
    let stdout_task = tokio::spawn(async move {
        let mut buf = vec![0u8; 8192];
        loop {
            match child_stdout.read(&mut buf).await {
                Ok(0) | Err(_) => break,
                Ok(n) => {
                    if ssh_writer.write_all(&buf[..n]).await.is_err() {
                        break;
                    }
                }
            }
        }
    });

    // child stderr → ssh extended data (stream 1)
    let stderr_task = tokio::spawn(async move {
        let mut buf = vec![0u8; 8192];
        loop {
            match child_stderr.read(&mut buf).await {
                Ok(0) | Err(_) => break,
                Ok(n) => {
                    if ssh_stderr_writer.write_all(&buf[..n]).await.is_err() {
                        break;
                    }
                }
            }
        }
    });

    let exit_status = match child.wait().await {
        Ok(status) => status.code().unwrap_or(1) as u32,
        Err(e) => {
            tracing::error!("git-receive-pack wait error: {e}");
            1
        }
    };

    let _ = tokio::join!(stdin_task, stdout_task, stderr_task);

    let _ = write_half.exit_status(exit_status).await;
    let _ = write_half.close().await;

    if exit_status == 0 {
        trigger_deploy_from_git(state, &app, git_dir);
    }
}

async fn pipe_read_to_write(
    mut read_half: ChannelReadHalf,
    mut writer: tokio::process::ChildStdin,
) {
    let mut buf = vec![0u8; 8192];
    let mut reader = read_half.make_reader();
    loop {
        match reader.read(&mut buf).await {
            Ok(0) | Err(_) => break,
            Ok(n) => {
                if writer.write_all(&buf[..n]).await.is_err() {
                    break;
                }
            }
        }
    }
    let _ = writer.shutdown().await;
}

fn trigger_deploy_from_git(
    state: &SharedState,
    app: &deku_core::types::App,
    git_dir: PathBuf,
) {
    let pool = state.pool.clone();
    let docker = state.docker.clone();
    let events = state.events.clone();
    let cfg = state.config.clone();
    let app = app.clone();

    tokio::spawn(async move {
        let tmp = match tempfile::tempdir() {
            Ok(t) => t,
            Err(e) => {
                tracing::error!("tmpdir creation failed: {e}");
                return;
            }
        };

        let git_dir_str = git_dir.to_string_lossy().to_string();
        let work_tree = tmp.path().to_str().unwrap_or(".").to_string();

        let ok = tokio::process::Command::new("git")
            .args([
                "--work-tree",
                &work_tree,
                "--git-dir",
                &git_dir_str,
                "checkout",
                "HEAD",
                "--",
                ".",
            ])
            .status()
            .await
            .map(|s| s.success())
            .unwrap_or(false);

        if !ok {
            tracing::error!(app = app.name, "git checkout for deploy failed");
            return;
        }

        let req = crate::deploy::DeployRequest {
            app_id: app.id.clone(),
            app_name: app.name.clone(),
            source: crate::deploy::DeploySource::Source {
                path: tmp.path().to_path_buf(),
            },
            force_builder: None,
        };

        if let Err(e) = crate::deploy::run_deploy(&pool, &docker, &events, &cfg, req).await {
            tracing::error!(app = app.name, "git push deploy failed: {e}");
        }
    });
}

fn parse_git_receive_pack(cmd: &str) -> Option<String> {
    let rest = cmd.trim().strip_prefix("git-receive-pack")?;
    let rest = rest.trim().trim_matches('\'').trim_matches('"');
    let app_name = rest.trim_start_matches('/');
    if app_name.is_empty() {
        return None;
    }
    Some(app_name.to_string())
}

async fn init_bare_repo(path: &PathBuf) -> Result<()> {
    tokio::fs::create_dir_all(path).await?;
    let status = tokio::process::Command::new("git")
        .args(["init", "--bare", path.to_str().unwrap_or(".")])
        .status()
        .await?;
    if !status.success() {
        return Err(anyhow::anyhow!("git init --bare failed"));
    }
    Ok(())
}
