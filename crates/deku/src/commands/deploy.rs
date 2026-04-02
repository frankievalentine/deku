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
        #[arg(long, help = "Force builder: dockerfile|nixpacks|pack|compose")]
        builder: Option<String>,
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
}

pub async fn run(args: DeployArgs, client: &DekuClient) -> Result<()> {
    match args.command {
        DeployCommands::Run { app, path, image, builder } => {
            if let Some(img) = image {
                println!("Deploying image '{img}' to app '{app}'...");
                let mut body = serde_json::json!({ "source": "image", "image": img });
                if let Some(b) = builder {
                    body["builder"] = serde_json::Value::String(b);
                }
                let resp = client
                    .post(&format!("/api/apps/{app}/deploy"), body)
                    .await?;
                let deploy_id = resp["deploy_id"].as_str().unwrap_or("?");
                println!("Deploy started (id: {deploy_id}). Streaming logs:");
                stream_deploy_logs(client, &app, deploy_id).await?;
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
                if let Some(b) = builder {
                    query.push_str(&format!("?builder={b}"));
                }
                let resp = client.post_archive(&query, archive).await?;
                let deploy_id = resp["deploy_id"].as_str().unwrap_or("?");
                println!("Deploy started (id: {deploy_id}). Streaming logs:");
                stream_deploy_logs(client, &app, deploy_id).await?;
            }
        }

        DeployCommands::List { app } => {
            let data = client.get(&format!("/api/apps/{app}/deployments")).await?;
            if let Some(deploys) = data.as_array() {
                if deploys.is_empty() {
                    println!("No deployments found.");
                } else {
                    println!("{:<12} {:<12} {:<20} {}", "ID", "STATUS", "IMAGE", "CREATED");
                    println!("{}", "-".repeat(72));
                    for d in deploys {
                        let id = &d["id"].as_str().unwrap_or("?")[..8.min(d["id"].as_str().unwrap_or("?").len())];
                        let status = d["status"].as_str().unwrap_or("-");
                        let image = d["image_tag"].as_str().unwrap_or("-");
                        let created = d["created_at"].as_str().unwrap_or("-");
                        println!("{:<12} {:<12} {:<20} {}", id, status, &image[..20.min(image.len())], &created[..19.min(created.len())]);
                    }
                }
            }
        }

        DeployCommands::Rollback { app, to } => {
            let mut body = serde_json::json!({});
            if let Some(id) = to {
                body["deployment_id"] = serde_json::Value::String(id);
            }
            let resp = client.post(&format!("/api/apps/{app}/rollback"), body).await?;
            let deploy_id = resp["deploy_id"].as_str().unwrap_or("?");
            println!("Rollback started (id: {deploy_id}).");
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

async fn stream_deploy_logs(client: &DekuClient, app: &str, deploy_id: &str) -> Result<()> {
    client
        .stream_sse(
            &format!("/api/apps/{app}/events/stream?since=0"),
            |data| {
                if let Ok(evt) = serde_json::from_str::<serde_json::Value>(data) {
                    let etype = evt["event_type"].as_str().unwrap_or("");
                    if etype.starts_with("build.") || etype.starts_with("deploy.") {
                        if let Some(line) = evt["payload"]["line"].as_str() {
                            println!("  {line}");
                        } else {
                            println!("[{etype}]");
                        }
                        if etype == "deploy.live" || etype == "deploy.failed" || etype == "deploy.rollback" {
                            // Signal to stop — we can't break out of a closure easily,
                            // so just let the SSE stream end naturally or timeout.
                            if let Some(url) = evt["payload"]["url"].as_str() {
                                println!("\nApp deployed: {url}");
                            }
                        }
                    }
                }
                let _ = deploy_id; // bind to suppress warning
            },
        )
        .await
}
