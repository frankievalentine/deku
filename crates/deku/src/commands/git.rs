use anyhow::{anyhow, bail, Context, Result};
use clap::{Args, Subcommand};
use russh::keys::ssh_key::{HashAlg, PublicKey};
use std::path::{Path, PathBuf};
use std::process::Command;

use crate::client::DekuClient;

#[derive(Debug, Args)]
pub struct GitArgs {
    #[command(subcommand)]
    command: GitCommands,
}

#[derive(Debug, Subcommand)]
enum GitCommands {
    /// Set a git-related config variable for an app
    Set {
        app: String,
        key: String,
        value: String,
    },
    /// Show git-related info for an app
    Report { app: String },
    /// Manage a local git remote for SSH deploys
    Remote {
        #[command(subcommand)]
        command: GitRemoteCommands,
    },
    /// Audit the local git remote and SSH key path for an app
    Doctor {
        #[arg(help = "App name. Defaults to the app inferred from the git remote.")]
        app: Option<String>,
        #[arg(long, default_value = "deku", help = "Git remote name to inspect")]
        remote: String,
        #[arg(long, help = "Expected SSH host for the remote")]
        host: Option<String>,
        #[arg(long, help = "Expected SSH port for the remote")]
        port: Option<u16>,
        #[arg(long, default_value = "git", help = "Expected SSH user for the remote")]
        user: String,
    },
}

#[derive(Debug, Subcommand)]
enum GitRemoteCommands {
    /// Add or update the local Deku git remote for an app
    Add {
        #[arg(help = "App name")]
        app: String,
        #[arg(long, default_value = "deku", help = "Git remote name to create")]
        remote: String,
        #[arg(long, help = "SSH host for the Deku server")]
        host: Option<String>,
        #[arg(long, help = "SSH port for the Deku server")]
        port: Option<u16>,
        #[arg(long, default_value = "git", help = "SSH user for the remote")]
        user: String,
        #[arg(long, help = "Update the remote if it already exists")]
        force: bool,
    },
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct RemoteSpec {
    user: String,
    host: String,
    port: u16,
    app: String,
    had_git_suffix: bool,
}

#[derive(Debug)]
struct LocalPublicKey {
    path: PathBuf,
    fingerprint: String,
}

pub async fn run(args: GitArgs, client: &DekuClient) -> Result<()> {
    match args.command {
        GitCommands::Set { app, key, value } => {
            let env_key = format!("_GIT_{}", key.to_uppercase());
            client
                .post(
                    &format!("/api/apps/{app}/config"),
                    serde_json::json!({ "key": env_key, "value": value }),
                )
                .await?;
            println!("Set {env_key}={value} for '{app}'.");
        }
        GitCommands::Report { app } => {
            let app_data = client.get(&format!("/api/apps/{app}")).await?;
            let config_data = client.get(&format!("/api/apps/{app}/config")).await?;

            println!("App: {}", app_data["name"].as_str().unwrap_or(&app));
            println!("Git config vars:");

            let git_vars: Vec<_> = config_data
                .as_array()
                .map(|vars| {
                    vars.iter()
                        .filter(|v| {
                            v["key"]
                                .as_str()
                                .map(|k| k.starts_with("_GIT_"))
                                .unwrap_or(false)
                        })
                        .collect()
                })
                .unwrap_or_default();

            if git_vars.is_empty() {
                println!("  (none)");
            } else {
                for v in git_vars {
                    println!(
                        "  {}={}",
                        v["key"].as_str().unwrap_or(""),
                        v["value"].as_str().unwrap_or("")
                    );
                }
            }

            println!("SSH port: {}", client.ssh_port());
            if let Some(host) = client.global_domain() {
                let spec = RemoteSpec {
                    user: "git".to_string(),
                    host: host.to_string(),
                    port: client.ssh_port(),
                    app: app.clone(),
                    had_git_suffix: false,
                };
                println!("Suggested remote: {}", build_remote_url(&spec));
            }
        }
        GitCommands::Remote { command } => match command {
            GitRemoteCommands::Add {
                app,
                remote,
                host,
                port,
                user,
                force,
            } => add_remote(client, &app, &remote, host, port, &user, force).await?,
        },
        GitCommands::Doctor {
            app,
            remote,
            host,
            port,
            user,
        } => doctor(client, app, &remote, host, port, &user).await?,
    }
    Ok(())
}

async fn add_remote(
    client: &DekuClient,
    app: &str,
    remote: &str,
    host: Option<String>,
    port: Option<u16>,
    user: &str,
    force: bool,
) -> Result<()> {
    ensure_git_worktree()?;
    client.get(&format!("/api/apps/{app}")).await?;

    let existing = git_remote_url(remote)?;
    let existing_spec = existing.as_deref().and_then(parse_remote_url);
    let host = resolve_host(host, existing_spec.as_ref(), client)?;
    let port = port
        .or_else(|| existing_spec.as_ref().map(|spec| spec.port))
        .unwrap_or_else(|| client.ssh_port());

    let spec = RemoteSpec {
        user: user.to_string(),
        host,
        port,
        app: app.to_string(),
        had_git_suffix: false,
    };
    let url = build_remote_url(&spec);

    if existing.is_some() {
        if !force {
            bail!("git remote '{remote}' already exists; rerun with --force to update it to {url}");
        }
        run_git(["remote", "set-url", remote, &url].as_slice())?;
        println!("Updated git remote '{remote}' -> {url}");
    } else {
        run_git(["remote", "add", remote, &url].as_slice())?;
        println!("Added git remote '{remote}' -> {url}");
    }

    println!("Push with: git push {remote} HEAD:main");
    Ok(())
}

async fn doctor(
    client: &DekuClient,
    app: Option<String>,
    remote: &str,
    host: Option<String>,
    port: Option<u16>,
    user: &str,
) -> Result<()> {
    ensure_git_worktree()?;

    let remote_url = git_remote_url(remote)?;
    let remote_spec = remote_url.as_deref().and_then(parse_remote_url);
    let app_name = app
        .or_else(|| remote_spec.as_ref().map(|spec| spec.app.clone()))
        .ok_or_else(|| anyhow!("unable to determine app name; pass it explicitly"))?;

    let app_exists = client.get(&format!("/api/apps/{app_name}")).await.is_ok();
    let registered_keys = client
        .get("/api/ssh-keys")
        .await?
        .as_array()
        .cloned()
        .unwrap_or_default();
    let registered_fingerprints = registered_keys
        .iter()
        .filter_map(|key| key["fingerprint"].as_str().map(ToOwned::to_owned))
        .collect::<Vec<_>>();
    let local_keys = discover_local_public_keys();
    let matching_local_keys = local_keys
        .iter()
        .filter(|key| {
            registered_fingerprints
                .iter()
                .any(|fp| fp == &key.fingerprint)
        })
        .collect::<Vec<_>>();
    let host = resolve_host(host, remote_spec.as_ref(), client).ok();
    let port = port
        .or_else(|| remote_spec.as_ref().map(|spec| spec.port))
        .unwrap_or_else(|| client.ssh_port());

    let mut issues = Vec::new();

    if remote_url.is_none() {
        issues.push(format!(
            "git remote '{remote}' is missing. Add it with: deku git remote add {app_name} --host <host>"
        ));
    }

    if let Some(spec) = &remote_spec {
        if spec.app != app_name {
            issues.push(format!(
                "git remote '{remote}' points at app '{}' instead of '{app_name}'",
                spec.app
            ));
        }
        if spec.had_git_suffix {
            issues.push(format!(
                "git remote '{remote}' uses a '.git' suffix. Deku now tolerates it, but '{app_name}' is the canonical remote path"
            ));
        }
        if spec.user != user {
            issues.push(format!(
                "git remote '{remote}' uses SSH user '{}'; expected '{user}'",
                spec.user
            ));
        }
    } else if remote_url.is_some() {
        issues.push(format!(
            "git remote '{remote}' is not in a recognized SSH format. Use git@HOST:{app_name} or ssh://{user}@HOST:{port}/{app_name}"
        ));
    }

    if !app_exists {
        issues.push(format!(
            "app '{app_name}' does not exist on this daemon. Create it with: deku apps create {app_name}"
        ));
    }

    if registered_fingerprints.is_empty() {
        issues.push("no SSH keys are registered with Deku".to_string());
    }

    if local_keys.is_empty() {
        issues.push("no local public keys were found under ~/.ssh/*.pub".to_string());
    } else if registered_fingerprints.is_empty() {
        issues.push(format!(
            "register one of your local public keys first, for example: deku ssh add laptop {}",
            local_keys[0].path.display()
        ));
    } else if matching_local_keys.is_empty() {
        issues.push(format!(
            "none of your local SSH public keys are registered with Deku. Try: deku ssh add laptop {}",
            local_keys[0].path.display()
        ));
    }

    if host.is_none() {
        issues.push(
            "no SSH host could be inferred. Pass --host or set global_domain in ~/.deku/config.toml"
                .to_string(),
        );
    }

    println!("Git doctor for '{app_name}'");
    println!(
        "  Git remote:       {}",
        remote_url.as_deref().unwrap_or("(missing)")
    );
    println!(
        "  SSH host:         {}",
        host.as_deref().unwrap_or("(unknown)")
    );
    println!("  SSH port:         {port}");
    println!("  App exists:       {}", yes_no(app_exists));
    println!("  Registered keys:  {}", registered_fingerprints.len());
    println!("  Local pubkeys:    {}", local_keys.len());
    println!(
        "  Matching pubkeys: {}",
        if matching_local_keys.is_empty() {
            "(none)".to_string()
        } else {
            matching_local_keys
                .iter()
                .map(|key| key.path.display().to_string())
                .collect::<Vec<_>>()
                .join(", ")
        }
    );

    if issues.is_empty() {
        if let Some(host) = host {
            let expected = build_remote_url(&RemoteSpec {
                user: user.to_string(),
                host,
                port,
                app: app_name,
                had_git_suffix: false,
            });
            println!("  Expected remote:  {expected}");
        }
        println!("Status: ok");
        Ok(())
    } else {
        println!("Status: issues found");
        for issue in &issues {
            println!("  - {issue}");
        }
        bail!("git doctor found {} issue(s)", issues.len())
    }
}

fn resolve_host(
    explicit_host: Option<String>,
    remote_spec: Option<&RemoteSpec>,
    client: &DekuClient,
) -> Result<String> {
    explicit_host
        .or_else(|| remote_spec.map(|spec| spec.host.clone()))
        .or_else(|| client.global_domain().map(ToOwned::to_owned))
        .ok_or_else(|| {
            anyhow!("missing SSH host; pass --host or set global_domain in ~/.deku/config.toml")
        })
}

fn ensure_git_worktree() -> Result<()> {
    let output = Command::new("git")
        .args(["rev-parse", "--is-inside-work-tree"])
        .output()
        .context("failed to run git")?;

    if !output.status.success() || String::from_utf8_lossy(&output.stdout).trim() != "true" {
        bail!("current directory is not a git work tree");
    }

    Ok(())
}

fn git_remote_url(remote: &str) -> Result<Option<String>> {
    let output = Command::new("git")
        .args(["remote", "get-url", remote])
        .output()
        .context("failed to run git")?;

    if output.status.success() {
        return Ok(Some(
            String::from_utf8_lossy(&output.stdout).trim().to_string(),
        ));
    }

    let stderr = String::from_utf8_lossy(&output.stderr);
    if stderr.contains("No such remote") {
        Ok(None)
    } else {
        Err(anyhow!(
            "git remote get-url {remote} failed: {}",
            stderr.trim()
        ))
    }
}

fn run_git(args: &[&str]) -> Result<()> {
    let output = Command::new("git")
        .args(args)
        .output()
        .with_context(|| format!("failed to run git {}", args.join(" ")))?;

    if output.status.success() {
        return Ok(());
    }

    let stderr = String::from_utf8_lossy(&output.stderr).trim().to_string();
    let stdout = String::from_utf8_lossy(&output.stdout).trim().to_string();
    let detail = if !stderr.is_empty() { stderr } else { stdout };
    Err(anyhow!("git {} failed: {detail}", args.join(" ")))
}

fn build_remote_url(spec: &RemoteSpec) -> String {
    if spec.port == 22 {
        format!("{}@{}:{}", spec.user, spec.host, spec.app)
    } else {
        format!(
            "ssh://{}@{}:{}/{}",
            spec.user, spec.host, spec.port, spec.app
        )
    }
}

fn parse_remote_url(url: &str) -> Option<RemoteSpec> {
    if let Some(rest) = url.strip_prefix("ssh://") {
        let (authority, path) = rest.split_once('/')?;
        let (user, host_port) = authority.split_once('@')?;
        let (host, port) = match host_port.rsplit_once(':') {
            Some((host, port)) => (host, port.parse().ok()?),
            None => (host_port, 22),
        };
        let (app, had_git_suffix) = normalize_remote_app(path)?;
        return Some(RemoteSpec {
            user: user.to_string(),
            host: host.to_string(),
            port,
            app,
            had_git_suffix,
        });
    }

    let (user_host, path) = url.rsplit_once(':')?;
    let (user, host) = user_host.split_once('@')?;
    let (app, had_git_suffix) = normalize_remote_app(path)?;
    Some(RemoteSpec {
        user: user.to_string(),
        host: host.to_string(),
        port: 22,
        app,
        had_git_suffix,
    })
}

fn normalize_remote_app(raw: &str) -> Option<(String, bool)> {
    let trimmed = raw.trim().trim_matches('\'').trim_matches('"');
    let trimmed = trimmed.trim_start_matches('/').trim_end_matches('/');
    if trimmed.is_empty() {
        return None;
    }
    let had_git_suffix = trimmed.ends_with(".git");
    let app = trimmed.strip_suffix(".git").unwrap_or(trimmed).trim();
    if app.is_empty() {
        return None;
    }
    Some((app.to_string(), had_git_suffix))
}

fn discover_local_public_keys() -> Vec<LocalPublicKey> {
    let ssh_dir = dirs_next::home_dir()
        .unwrap_or_else(|| PathBuf::from("/root"))
        .join(".ssh");
    if !ssh_dir.exists() {
        return Vec::new();
    }

    let mut keys = std::fs::read_dir(&ssh_dir)
        .ok()
        .into_iter()
        .flat_map(|entries| entries.flatten())
        .filter_map(|entry| parse_local_public_key(&entry.path()).ok().flatten())
        .collect::<Vec<_>>();
    keys.sort_by(|a, b| a.path.cmp(&b.path));
    keys
}

fn parse_local_public_key(path: &Path) -> Result<Option<LocalPublicKey>> {
    if path.extension().and_then(|ext| ext.to_str()) != Some("pub") {
        return Ok(None);
    }

    let contents = std::fs::read_to_string(path)
        .with_context(|| format!("failed to read {}", path.display()))?;
    let public_key = PublicKey::from_openssh(contents.trim())
        .with_context(|| format!("failed to parse {}", path.display()))?;

    Ok(Some(LocalPublicKey {
        path: path.to_path_buf(),
        fingerprint: public_key.fingerprint(HashAlg::Sha256).to_string(),
    }))
}

fn yes_no(value: bool) -> &'static str {
    if value {
        "yes"
    } else {
        "no"
    }
}

#[cfg(test)]
mod tests {
    use super::{build_remote_url, parse_remote_url, RemoteSpec};

    #[test]
    fn parses_scp_style_remote() {
        let parsed = parse_remote_url("git@example.com:my-app").expect("remote to parse");
        assert_eq!(
            parsed,
            RemoteSpec {
                user: "git".to_string(),
                host: "example.com".to_string(),
                port: 22,
                app: "my-app".to_string(),
                had_git_suffix: false,
            }
        );
    }

    #[test]
    fn parses_ssh_url_with_git_suffix() {
        let parsed =
            parse_remote_url("ssh://git@example.com:2222/my-app.git").expect("remote to parse");
        assert_eq!(parsed.app, "my-app");
        assert!(parsed.had_git_suffix);
        assert_eq!(parsed.port, 2222);
    }

    #[test]
    fn builds_remote_url_for_default_port() {
        let url = build_remote_url(&RemoteSpec {
            user: "git".to_string(),
            host: "example.com".to_string(),
            port: 22,
            app: "my-app".to_string(),
            had_git_suffix: false,
        });
        assert_eq!(url, "git@example.com:my-app");
    }

    #[test]
    fn builds_remote_url_for_custom_port() {
        let url = build_remote_url(&RemoteSpec {
            user: "git".to_string(),
            host: "example.com".to_string(),
            port: 2222,
            app: "my-app".to_string(),
            had_git_suffix: false,
        });
        assert_eq!(url, "ssh://git@example.com:2222/my-app");
    }
}
