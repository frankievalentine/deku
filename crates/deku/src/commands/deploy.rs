use crate::client::DekuClient;
use anyhow::{anyhow, Result};
use clap::{Args, Subcommand};

#[derive(Debug, Args)]
pub struct DeployArgs {
    #[command(subcommand)]
    command: DeployCommands,
}

#[derive(Debug, Subcommand)]
enum DeployCommands {
    /// Deploy an app from the current directory (or --path)
    Run {
        #[arg(help = "App name")]
        app: String,
        #[arg(long, help = "Source directory", default_value = ".")]
        path: String,
        #[arg(long, help = "Docker image to deploy instead of building")]
        image: Option<String>,
        #[arg(long, help = "Force builder: dockerfile|railpack|pack|compose")]
        builder: Option<String>,
        #[arg(
            long,
            help = "Build host: omit for the configured default, 'local' to build here, or the configured build host name"
        )]
        build_host: Option<String>,
    },
    /// List deployments for an app
    List {
        #[arg(help = "App name")]
        app: String,
    },
    /// Roll back to the previous deployment
    Rollback {
        #[arg(help = "App name")]
        app: String,
        #[arg(long, help = "Specific deployment ID to roll back to")]
        to: Option<String>,
    },
    /// Manage per-app deploy tokens for CI and provider webhooks
    Token {
        #[command(subcommand)]
        command: TokenCommands,
    },
}

#[derive(Debug, Subcommand)]
enum TokenCommands {
    /// List an app's deploy tokens
    List {
        #[arg(help = "App name")]
        app: String,
    },
    /// Mint a deploy token; the token is printed once
    Create {
        #[arg(help = "App name")]
        app: String,
        #[arg(long, default_value = "ci", help = "Label for the token")]
        name: String,
    },
    /// Revoke a deploy token by id
    Revoke {
        #[arg(help = "App name")]
        app: String,
        #[arg(help = "Token id from `deploy token list`")]
        id: String,
    },
}

pub async fn run(args: DeployArgs, client: &DekuClient) -> Result<()> {
    match args.command {
        DeployCommands::Run {
            app,
            path,
            image,
            builder,
            build_host,
        } => {
            let since = chrono::Utc::now().to_rfc3339_opts(chrono::SecondsFormat::Millis, true);

            if let Some(img) = image {
                println!("Deploying image '{img}' to app '{app}'...");
                let mut body = serde_json::json!({ "source": "image", "image": img });
                if let Some(b) = builder {
                    body["builder"] = serde_json::Value::String(b);
                }
                if let Some(h) = build_host {
                    body["build_host"] = serde_json::Value::String(h);
                }
                client
                    .post(&format!("/api/apps/{app}/deploy"), body)
                    .await?;
                println!("Deploy started. Streaming logs:");
                stream_deploy_logs(client, &app, &since).await?;
            } else {
                // Build a tar.gz of the source directory and upload it
                let src = std::path::Path::new(&path)
                    .canonicalize()
                    .map_err(|e| anyhow!("cannot resolve path '{path}': {e}"))?;

                println!("Packaging '{}'...", src.display());
                let archive = build_archive(&src)?;
                let size_kb = archive.len() / 1024;
                println!("Uploading {size_kb}KB archive to daemon...");

                let mut query = format!("/api/apps/{app}/deploy/archive");
                match (builder, build_host) {
                    (Some(b), Some(h)) => {
                        query.push_str(&format!("?builder={b}&build_host={h}"));
                    }
                    (Some(b), None) => query.push_str(&format!("?builder={b}")),
                    (None, Some(h)) => query.push_str(&format!("?build_host={h}")),
                    (None, None) => {}
                }
                client.post_archive(&query, archive).await?;
                println!("Deploy started. Streaming logs:");
                stream_deploy_logs(client, &app, &since).await?;
            }
        }

        DeployCommands::List { app } => {
            let data = client.get(&format!("/api/apps/{app}/deployments")).await?;
            if let Some(deploys) = data.as_array() {
                if deploys.is_empty() {
                    println!("No deployments found.");
                } else {
                    println!("{:<12} {:<12} {:<20} CREATED", "ID", "STATUS", "IMAGE");
                    println!("{}", "-".repeat(72));
                    for d in deploys {
                        let id = &d["id"].as_str().unwrap_or("?")
                            [..8.min(d["id"].as_str().unwrap_or("?").len())];
                        let status = d["status"].as_str().unwrap_or("-");
                        let image = d["image_tag"].as_str().unwrap_or("-");
                        let created = d["created_at"].as_str().unwrap_or("-");
                        println!(
                            "{:<12} {:<12} {:<20} {}",
                            id,
                            status,
                            &image[..20.min(image.len())],
                            &created[..19.min(created.len())]
                        );
                    }
                }
            }
        }

        DeployCommands::Token { command } => match command {
            TokenCommands::List { app } => {
                let data = client
                    .get(&format!("/api/apps/{app}/deploy-tokens"))
                    .await?;
                let empty = Vec::new();
                let tokens = data.as_array().unwrap_or(&empty);
                if tokens.is_empty() {
                    println!("No deploy tokens for '{app}'.");
                } else {
                    println!("{:<38} {:<16} {:<20} LAST USED", "ID", "NAME", "CREATED");
                    println!("{}", "-".repeat(96));
                    for token in tokens {
                        println!(
                            "{:<38} {:<16} {:<20} {}",
                            token["id"].as_str().unwrap_or("-"),
                            token["name"].as_str().unwrap_or("-"),
                            token["created_at"].as_str().unwrap_or("-"),
                            token["last_used_at"].as_str().unwrap_or("never"),
                        );
                    }
                }
            }
            TokenCommands::Create { app, name } => {
                let data = client
                    .post(
                        &format!("/api/apps/{app}/deploy-tokens"),
                        serde_json::json!({ "name": name }),
                    )
                    .await?;
                let token = data["token"].as_str().unwrap_or("");
                println!("Deploy token created for '{app}' (name: {name}).");
                println!();
                println!("  {token}");
                println!();
                println!("This token is shown once and cannot be retrieved later. It can only");
                println!("trigger deploys for '{app}'. Example:");
                println!();
                println!("  curl -X POST -H \"Authorization: Bearer {token}\" \\");
                println!(
                    "    http://127.0.0.1:2810/api/apps/{app}/deploy --data '{{\"source\":\"image\",\"image\":\"nginx:alpine\"}}'"
                );
            }
            TokenCommands::Revoke { app, id } => {
                client
                    .delete(&format!("/api/apps/{app}/deploy-tokens/{id}"))
                    .await?;
                println!("Revoked deploy token '{id}' for '{app}'.");
            }
        },

        DeployCommands::Rollback { app, to } => {
            let mut body = serde_json::json!({});
            if let Some(id) = to {
                body["deployment_id"] = serde_json::Value::String(id);
            }
            client
                .post(&format!("/api/apps/{app}/rollback"), body)
                .await?;
            println!("Rollback started.");
        }
    }
    Ok(())
}

fn build_archive(source_dir: &std::path::Path) -> Result<Vec<u8>> {
    let mut buf = Vec::new();
    {
        let enc = flate2::write::GzEncoder::new(&mut buf, flate2::Compression::default());
        let mut tar = tar::Builder::new(enc);
        tar.append_dir_all(".", source_dir)?;
        let enc = tar.into_inner()?;
        enc.finish()?;
    }
    Ok(buf)
}

async fn stream_deploy_logs(client: &DekuClient, app: &str, since: &str) -> Result<()> {
    client
        .stream_sse(
            &format!("/api/apps/{app}/events/stream?since={since}"),
            |data| {
                if let Ok(evt) = serde_json::from_str::<serde_json::Value>(data) {
                    let etype = evt["event_type"].as_str().unwrap_or("");
                    if etype.starts_with("build.") || etype.starts_with("deploy.") {
                        let payload = crate::client::event_payload(&evt);
                        if let Some(line) = payload["line"].as_str() {
                            println!("  {line}");
                        } else if let Some(url) = payload["url"].as_str() {
                            println!("[{etype}] {url}");
                        } else if let Some(error) = payload["error"].as_str() {
                            println!("[{etype}] {error}");
                        } else {
                            println!("[{etype}]");
                        }

                        // `deploy.rollback` is mid-deploy: the terminal outcome and
                        // the reason arrive in the following `deploy.failed` event.
                        if matches!(etype, "deploy.live" | "deploy.failed") {
                            return false;
                        }
                    }
                }
                true
            },
        )
        .await
}
