//! Tier 1 remote builds: ship source to an SSH build host, build there, push the
//! immutable tag to a registry, then pull it on the deploy host.
//!
//! Railpack always `docker load`s on the machine running the CLI and has no
//! registry-push flag, so offloading means running the builder *on the build
//! host*. Every builder therefore produces a single image tagged with the
//! registry reference; the deploy host pulls and retags it, leaving the
//! container, retire, and rollback paths untouched.

use std::path::{Component, Path};
use std::process::Stdio;

use anyhow::{anyhow, Context, Result};
use deku_core::types::DekuToml;
use tokio::io::AsyncWriteExt;

use crate::build::{compose_web, dockerfile_params, ComposeWebTarget};
use crate::config::{BuildHostConfig, DekuConfig, RegistryConfig};
use crate::events::EventSender;

/// Remote probe run on the build host. Prints `key=value` lines for parsing.
const PROBE: &str = r#"if command -v docker >/dev/null 2>&1; then
  printf 'docker=%s\n' "$(docker version --format '{{.Server.Version}}' 2>/dev/null || echo daemon-unreachable)"
else
  printf 'docker=missing\n'
fi
if command -v railpack >/dev/null 2>&1; then
  printf 'railpack=%s\n' "$(railpack --version 2>&1 | head -n1 || echo unknown)"
else
  printf 'railpack=missing\n'
fi
printf 'buildkit=%s\n' "$(docker inspect -f '{{.State.Running}}' deku-buildkit 2>/dev/null || echo absent)""#;

/// Builders that produce an image from shipped source and can run remotely.
pub fn supports_remote_build(builder_name: &str) -> bool {
    matches!(builder_name, "railpack" | "dockerfile" | "compose" | "pack")
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CheckResult {
    pub name: String,
    pub status: String,
    pub detail: String,
}

impl CheckResult {
    fn new(name: &str, ok: bool, detail: impl Into<String>) -> Self {
        Self {
            name: name.to_string(),
            status: if ok {
                "ok".to_string()
            } else {
                "fail".to_string()
            },
            detail: detail.into(),
        }
    }
}

/// Registry host without any repository path, as `docker login` expects.
fn registry_host(server: &str) -> String {
    let trimmed = server.trim().trim_end_matches('/');
    let without_scheme = trimmed
        .strip_prefix("https://")
        .or_else(|| trimmed.strip_prefix("http://"))
        .unwrap_or(trimmed);
    without_scheme
        .split('/')
        .next()
        .unwrap_or(without_scheme)
        .to_string()
}

/// Single-quote a value for safe interpolation into a POSIX shell script.
fn sh_quote(value: &str) -> String {
    format!("'{}'", value.replace('\'', "'\\''"))
}

/// Resolve a path below `base` into a relative POSIX path, rejecting escapes.
fn safe_rel_path(base: &Path, candidate: &Path, label: &str) -> Result<String> {
    let relative = candidate.strip_prefix(base).map_err(|_| {
        anyhow!(
            "{label} '{}' escapes the source directory",
            candidate.display()
        )
    })?;

    let mut segments: Vec<String> = Vec::new();
    for component in relative.components() {
        match component {
            Component::Normal(part) => segments.push(part.to_string_lossy().to_string()),
            Component::CurDir => {}
            _ => {
                return Err(anyhow!(
                    "{label} '{}' escapes the source directory",
                    candidate.display()
                ))
            }
        }
    }

    if segments.is_empty() {
        Ok(".".to_string())
    } else {
        Ok(segments.join("/"))
    }
}

fn safe_dockerfile_name(dockerfile: &str) -> Result<String> {
    let path = Path::new(dockerfile);
    for component in path.components() {
        if !matches!(component, Component::Normal(_) | Component::CurDir) {
            return Err(anyhow!(
                "dockerfile '{dockerfile}' must stay inside its build context"
            ));
        }
    }
    Ok(dockerfile.to_string())
}

fn command_on_path(name: &str) -> bool {
    std::env::var_os("PATH")
        .is_some_and(|paths| std::env::split_paths(&paths).any(|dir| dir.join(name).is_file()))
}

fn require_registry(cfg: &DekuConfig) -> Result<&RegistryConfig> {
    cfg.registry.as_ref().ok_or_else(|| {
        anyhow!("remote builds require a registry: configure one with 'deku registry setup'")
    })
}

fn require_build_host(cfg: &DekuConfig) -> Result<&BuildHostConfig> {
    cfg.build_host
        .as_ref()
        .ok_or_else(|| anyhow!("no build host configured; run 'deku build-host setup' first"))
}

/// Render the remote shell program that produces and pushes the image.
///
/// The program reads the extracted source directory from `$1` and pushes to the
/// pre-set `ref` variable. Values sourced from the repository (`deku.toml`,
/// compose files) are shell-quoted so untrusted source cannot inject commands.
pub fn remote_build_script(
    source_dir: &Path,
    builder_name: &str,
    deku_toml: Option<&DekuToml>,
    buildkit_host: &str,
    image_reference: &str,
) -> Result<String> {
    let mut script = String::from("set -eu\nwork=\"$1\"\nref=");
    script.push_str(&sh_quote(image_reference));
    script.push('\n');

    match builder_name {
        "railpack" => {
            let mut env_args = String::new();
            if let Some(args) = deku_toml
                .and_then(|t| t.build.as_ref())
                .and_then(|b| b.args.as_ref())
            {
                for (key, value) in args {
                    env_args.push_str(" --env ");
                    env_args.push_str(&sh_quote(&format!("{key}={value}")));
                }
            }
            script.push_str(&format!(
                "BUILDKIT_HOST={} railpack build \"$work\" --name \"$ref\" --progress plain{}\n",
                sh_quote(buildkit_host),
                env_args
            ));
        }
        "dockerfile" => {
            let params = dockerfile_params(source_dir, deku_toml);
            let context = safe_rel_path(source_dir, &params.context, "build context")?;
            let dockerfile = safe_dockerfile_name(&params.dockerfile)?;
            let mut build_args = String::new();
            for (key, value) in &params.buildargs {
                build_args.push_str(&format!(
                    " --build-arg {}",
                    sh_quote(&format!("{key}={value}"))
                ));
            }
            script.push_str(&format!(
                "cd \"$work/{context}\"\ndocker build -f {} -t \"$ref\"{} .\n",
                sh_quote(&dockerfile),
                build_args
            ));
        }
        "compose" => {
            let web = compose_web(source_dir)?;
            match web.target {
                ComposeWebTarget::Build {
                    context,
                    dockerfile,
                } => {
                    let context = safe_rel_path(
                        source_dir,
                        &source_dir.join(&context),
                        "compose build context",
                    )?;
                    let dockerfile = safe_dockerfile_name(&dockerfile)?;
                    script.push_str(&format!(
                        "cd \"$work/{context}\"\ndocker build -f {} -t \"$ref\" .\n",
                        sh_quote(&dockerfile)
                    ));
                }
                ComposeWebTarget::Image { reference } => {
                    let quoted = sh_quote(&reference);
                    script.push_str(&format!(
                        "docker pull {quoted}\ndocker tag {quoted} \"$ref\"\n"
                    ));
                }
            }
        }
        "pack" => {
            script.push_str("pack build \"$ref\" --path \"$work\"\n");
        }
        other => return Err(anyhow!("builder '{other}' cannot run on a build host")),
    }

    script.push_str("echo \"Pushing $ref\"\ndocker push \"$ref\"\n");
    Ok(script)
}

struct SshRunner<'a> {
    host: &'a BuildHostConfig,
    log: Option<(EventSender, String)>,
}

impl SshRunner<'_> {
    fn command(&self) -> Result<tokio::process::Command> {
        let target = self.host.ssh_target();
        if target.destination.is_empty() || target.destination.starts_with('-') {
            return Err(anyhow!(
                "invalid build host destination '{}'",
                self.host.host
            ));
        }

        let mut cmd = tokio::process::Command::new("ssh");
        cmd.arg("-o")
            .arg("BatchMode=yes")
            .arg("-o")
            .arg("StrictHostKeyChecking=accept-new")
            .arg("-o")
            .arg("ConnectTimeout=15")
            .arg("-T");
        if let Some(key) = &self.host.identity_file {
            cmd.arg("-i").arg(key);
        }
        if let Some(port) = target.port {
            cmd.arg("-p").arg(port.to_string());
        }
        cmd.arg(&target.destination);
        Ok(cmd)
    }

    async fn run(
        &self,
        remote_command: &str,
        stdin: Option<Vec<u8>>,
    ) -> Result<std::process::ExitStatus> {
        let mut cmd = self.command()?;
        cmd.arg(remote_command);
        cmd.stdin(if stdin.is_some() {
            Stdio::piped()
        } else {
            Stdio::null()
        });
        cmd.stdout(Stdio::piped()).stderr(Stdio::piped());
        cmd.kill_on_drop(true);

        let mut child = cmd
            .spawn()
            .with_context(|| format!("spawning ssh to '{}'", self.host.host))?;

        let writer = match stdin {
            Some(bytes) => {
                let mut handle = child.stdin.take().context("ssh stdin unavailable")?;
                Some(tokio::spawn(async move {
                    let _ = handle.write_all(&bytes).await;
                    let _ = handle.shutdown().await;
                }))
            }
            None => None,
        };

        let stdout = child.stdout.take().context("ssh stdout unavailable")?;
        let stderr = child.stderr.take().context("ssh stderr unavailable")?;
        let out_task = self.spawn_reader(stdout);
        let err_task = self.spawn_reader(stderr);

        let status = child.wait().await.context("waiting for ssh")?;
        if let Some(writer) = writer {
            let _ = writer.await;
        }
        let _ = out_task.await;
        let _ = err_task.await;
        Ok(status)
    }

    async fn capture(&self, remote_command: &str) -> Result<String> {
        let mut cmd = self.command()?;
        cmd.arg(remote_command);
        cmd.stdin(Stdio::null());
        cmd.stdout(Stdio::piped()).stderr(Stdio::piped());
        let output = cmd
            .output()
            .await
            .with_context(|| format!("running command on build host '{}'", self.host.host))?;
        if !output.status.success() {
            return Err(anyhow!(
                "remote command failed: {}",
                String::from_utf8_lossy(&output.stderr).trim()
            ));
        }
        Ok(String::from_utf8_lossy(&output.stdout).to_string())
    }

    fn spawn_reader<R>(&self, reader: R) -> tokio::task::JoinHandle<()>
    where
        R: tokio::io::AsyncRead + Unpin + Send + 'static,
    {
        let log = self.log.clone();
        tokio::spawn(async move {
            use tokio::io::AsyncBufReadExt;
            let mut lines = tokio::io::BufReader::new(reader).lines();
            while let Ok(Some(line)) = lines.next_line().await {
                if let Some((events, app_id)) = &log {
                    events.emit(
                        Some(app_id.clone()),
                        "build.log",
                        Some(serde_json::json!({ "line": line })),
                    );
                }
            }
        })
    }

    fn note(&self, line: &str) {
        if let Some((events, app_id)) = &self.log {
            events.emit(
                Some(app_id.clone()),
                "build.log",
                Some(serde_json::json!({ "line": line })),
            );
        }
    }
}

/// Build `image_reference` on the configured build host and push it to the registry.
pub async fn build_remote(
    source_dir: &Path,
    builder_name: &str,
    deku_toml: Option<&DekuToml>,
    cfg: &DekuConfig,
    events: &EventSender,
    app_id: &str,
    image_reference: &str,
) -> Result<()> {
    let build_host = require_build_host(cfg)?;
    let registry = require_registry(cfg)?;

    if !supports_remote_build(builder_name) {
        return Err(anyhow!(
            "builder '{builder_name}' cannot run on a build host"
        ));
    }

    let runner = SshRunner {
        host: build_host,
        log: Some((events.clone(), app_id.to_string())),
    };

    let script = remote_build_script(
        source_dir,
        builder_name,
        deku_toml,
        &build_host.buildkit_host,
        image_reference,
    )?;

    let tarball =
        crate::container::create_tar_gz(source_dir).context("packing source for the build host")?;

    let work = format!(
        "/tmp/deku-build-{}",
        &uuid::Uuid::new_v4().simple().to_string()[..12]
    );
    let work_quoted = sh_quote(&work);

    runner.note(&format!(
        "Shipping source to build host '{}' ({}) for a {builder_name} build",
        build_host.name, build_host.host
    ));

    let prepare = format!(
        "set -e; rm -rf {work}; mkdir -p {work}; chmod 700 {work}; tar -xzf - -C {work}",
        work = work_quoted
    );
    let status = runner.run(&prepare, Some(tarball.to_vec())).await?;
    if !status.success() {
        let _ = cleanup(&runner, &work_quoted).await;
        return Err(anyhow!("transferring source to the build host failed"));
    }

    if registry.has_credentials() {
        let host = registry_host(&registry.server);
        let user = registry.username.as_deref().unwrap_or_default();
        let login = format!(
            "docker login -u {} --password-stdin {}",
            sh_quote(user),
            sh_quote(&host)
        );
        let password = registry.password.clone().unwrap_or_default().into_bytes();
        let status = runner.run(&login, Some(password)).await?;
        if !status.success() {
            let _ = cleanup(&runner, &work_quoted).await;
            return Err(anyhow!("docker login on the build host failed"));
        }
    } else {
        runner.note("No registry credentials configured; relying on the build host's docker login");
    }

    let build = format!("sh -s -- {work}", work = work_quoted);
    let status = runner.run(&build, Some(script.into_bytes())).await;

    // Always remove the remote workspace, even when the build failed.
    let cleanup_result = cleanup(&runner, &work_quoted).await;

    let status = status?;
    if !status.success() {
        return Err(anyhow!("remote build failed on host '{}'", build_host.name));
    }
    cleanup_result?;
    Ok(())
}

async fn cleanup(runner: &SshRunner<'_>, work_quoted: &str) -> Result<()> {
    runner.run(&format!("rm -rf {work_quoted}"), None).await?;
    Ok(())
}

/// Map remote probe output into checks.
fn parse_probe(output: &str) -> Vec<CheckResult> {
    let mut checks = Vec::new();
    for line in output.lines() {
        let Some((key, value)) = line.split_once('=') else {
            continue;
        };
        match (key, value) {
            ("docker", "missing") => {
                checks.push(CheckResult::new("docker", false, "docker is not installed"))
            }
            ("docker", "daemon-unreachable") => checks.push(CheckResult::new(
                "docker",
                false,
                "docker daemon is unreachable",
            )),
            ("docker", version) => checks.push(CheckResult::new(
                "docker",
                true,
                format!("server {version}"),
            )),
            ("railpack", "missing") => checks.push(CheckResult::new(
                "railpack",
                false,
                "railpack is not installed",
            )),
            ("railpack", "") => checks.push(CheckResult::new(
                "railpack",
                true,
                "installed (version unknown)",
            )),
            ("railpack", version) => {
                checks.push(CheckResult::new("railpack", true, version.to_string()))
            }
            ("buildkit", "true") => checks.push(CheckResult::new(
                "buildkit",
                true,
                "deku-buildkit container running",
            )),
            ("buildkit", "false") => checks.push(CheckResult::new(
                "buildkit",
                false,
                "deku-buildkit stopped; run 'deku build-host init'",
            )),
            ("buildkit", _) => checks.push(CheckResult::new(
                "buildkit",
                false,
                "deku-buildkit not created; run 'deku build-host init'",
            )),
            _ => {}
        }
    }
    checks
}

/// Probe the build host for the tools a remote build needs.
pub async fn check_build_host(cfg: &DekuConfig) -> Result<Vec<CheckResult>> {
    let mut checks = Vec::new();

    let Some(build_host) = cfg.build_host.as_ref() else {
        checks.push(CheckResult::new(
            "build_host",
            false,
            "not configured; run 'deku build-host setup'",
        ));
        checks.push(CheckResult::new(
            "registry",
            cfg.registry.is_some(),
            registry_detail(cfg),
        ));
        return Ok(checks);
    };

    checks.push(CheckResult::new(
        "build_host",
        true,
        format!("{} ({})", build_host.name, build_host.host),
    ));
    checks.push(CheckResult::new(
        "registry",
        cfg.registry.is_some(),
        registry_detail(cfg),
    ));

    if !command_on_path("ssh") {
        checks.push(CheckResult::new(
            "ssh_binary",
            false,
            "ssh client not found on the control plane",
        ));
        return Ok(checks);
    }
    checks.push(CheckResult::new("ssh_binary", true, "available"));

    let runner = SshRunner {
        host: build_host,
        log: None,
    };
    match runner.capture(PROBE).await {
        Ok(stdout) => checks.extend(parse_probe(&stdout)),
        Err(error) => checks.push(CheckResult::new("ssh", false, error.to_string())),
    }

    Ok(checks)
}

fn registry_detail(cfg: &DekuConfig) -> String {
    match cfg.registry.as_ref() {
        Some(registry) => format!("{} ({})", registry.server, registry.repository("<app>")),
        None => "not configured; remote builds require one".to_string(),
    }
}

/// Ensure a managed BuildKit container exists on the build host.
pub async fn init_build_host(cfg: &DekuConfig) -> Result<Vec<String>> {
    let build_host = require_build_host(cfg)?;
    let mut notes = Vec::new();

    let endpoint = build_host.buildkit_host.trim();
    let Some(container) = endpoint.strip_prefix("docker-container://") else {
        notes.push(format!(
            "BuildKit endpoint '{endpoint}' is externally managed; nothing to initialize"
        ));
        return Ok(notes);
    };

    if container.is_empty()
        || !container
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || c == '-' || c == '_')
    {
        return Err(anyhow!("invalid BuildKit container name '{container}'"));
    }

    let image = cfg.buildkit.clone().unwrap_or_default().image;
    let volume = format!("{container}-cache");
    let name = sh_quote(container);
    let script = format!(
        "set -e\n\
         exists=$(docker inspect -f '{{{{.State.Running}}}}' {name} 2>/dev/null || echo absent)\n\
         if [ \"$exists\" = absent ]; then\n\
           docker volume create {volume} >/dev/null\n\
           docker run -d --name {name} --privileged --restart unless-stopped -v {volume}:/var/lib/buildkit {image} >/dev/null\n\
           echo \"created BuildKit container '{container}' from {image_display}\"\n\
         elif [ \"$exists\" = false ]; then\n\
           docker start {name} >/dev/null\n\
           echo \"started BuildKit container '{container}'\"\n\
         else\n\
           echo \"BuildKit container '{container}' already running\"\n\
         fi",
        volume = sh_quote(&volume),
        image = sh_quote(&image),
        image_display = image,
    );

    let runner = SshRunner {
        host: build_host,
        log: None,
    };

    let output = runner.capture(&script).await?;
    notes.extend(
        output
            .lines()
            .filter(|line| !line.is_empty())
            .map(str::to_string),
    );
    Ok(notes)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn registry_host_strips_path_and_scheme() {
        assert_eq!(registry_host("ghcr.io/acme"), "ghcr.io");
        assert_eq!(
            registry_host("https://registry.example.com/team/"),
            "registry.example.com"
        );
        assert_eq!(registry_host("localhost:5000"), "localhost:5000");
    }

    #[test]
    fn sh_quote_escapes_single_quotes() {
        assert_eq!(sh_quote("plain"), "'plain'");
        assert_eq!(sh_quote("it's"), "'it'\\''s'");
    }

    #[test]
    fn railpack_script_sets_buildkit_and_env() {
        let source = tempfile::tempdir().expect("tempdir");
        let deku_toml: DekuToml =
            toml::from_str("[build]\nargs = { NODE_ENV = \"production\" }\n").expect("deku.toml");

        let script = remote_build_script(
            source.path(),
            "railpack",
            Some(&deku_toml),
            "docker-container://deku-buildkit",
            "ghcr.io/acme/deku/demo:abc123",
        )
        .expect("script");

        assert!(script.contains("BUILDKIT_HOST='docker-container://deku-buildkit'"));
        assert!(script.contains("railpack build \"$work\" --name \"$ref\" --progress plain"));
        assert!(script.contains("--env 'NODE_ENV=production'"));
        assert!(script.contains("docker push \"$ref\""));
    }

    #[test]
    fn dockerfile_script_chdirs_into_context_and_quotes_args() {
        let source = tempfile::tempdir().expect("tempdir");
        std::fs::create_dir_all(source.path().join("app")).expect("dir");
        let deku_toml: DekuToml = toml::from_str(
            "[build]\ndockerfile = \"Dockerfile.prod\"\ncontext = \"app\"\nargs = { A = \"1\" }\n",
        )
        .expect("deku.toml");

        let script = remote_build_script(
            source.path(),
            "dockerfile",
            Some(&deku_toml),
            "unused",
            "ghcr.io/acme/deku/demo:abc123",
        )
        .expect("script");

        assert!(script.contains("cd \"$work/app\""));
        assert!(
            script.contains("docker build -f 'Dockerfile.prod' -t \"$ref\" --build-arg 'A=1' .")
        );
        assert!(!script.contains("--context"));
    }

    #[test]
    fn context_escaping_the_source_is_rejected() {
        let source = tempfile::tempdir().expect("tempdir");
        let deku_toml: DekuToml =
            toml::from_str("[build]\ncontext = \"../../etc\"\n").expect("deku.toml");

        let error = remote_build_script(
            source.path(),
            "dockerfile",
            Some(&deku_toml),
            "unused",
            "ghcr.io/acme/deku/demo:abc123",
        )
        .expect_err("context escape must be rejected");
        assert!(error.to_string().contains("escapes the source directory"));
    }

    #[test]
    fn compose_image_target_pulls_then_tags() {
        let source = tempfile::tempdir().expect("tempdir");
        std::fs::write(
            source.path().join("compose.yaml"),
            "services:\n  web:\n    image: ghcr.io/acme/app:1.2.3\n",
        )
        .expect("write");

        let script = remote_build_script(
            source.path(),
            "compose",
            None,
            "unused",
            "ghcr.io/acme/deku/demo:abc123",
        )
        .expect("script");

        assert!(script.contains("docker pull 'ghcr.io/acme/app:1.2.3'"));
        assert!(script.contains("docker tag 'ghcr.io/acme/app:1.2.3' \"$ref\""));
    }

    #[test]
    fn probe_lines_map_to_checks() {
        let checks = parse_probe("docker=missing\nrailpack=1.2.3\nbuildkit=absent\n");
        assert_eq!(checks.len(), 3);
        assert_eq!(checks[0].name, "docker");
        assert_eq!(checks[0].status, "fail");
        assert_eq!(checks[1].status, "ok");
        assert_eq!(checks[1].detail, "1.2.3");
        assert_eq!(checks[2].name, "buildkit");
        assert_eq!(checks[2].status, "fail");
    }

    #[test]
    fn only_source_builders_run_remotely() {
        assert!(supports_remote_build("railpack"));
        assert!(supports_remote_build("dockerfile"));
        assert!(supports_remote_build("compose"));
        assert!(supports_remote_build("pack"));
        assert!(!supports_remote_build("image"));
    }
}
