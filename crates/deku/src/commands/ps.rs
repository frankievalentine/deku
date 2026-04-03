use crate::client::DekuClient;
use anyhow::Result;
use clap::{Args, Subcommand};

#[derive(Debug, Args)]
pub struct PsArgs {
    #[command(subcommand)]
    command: PsCommands,
}

#[derive(Debug, Subcommand)]
enum PsCommands {
    /// Show running processes for an app
    List {
        #[arg(help = "App name")]
        app: String,
    },
    /// Scale process types (PROCESS=COUNT ...)
    Scale {
        #[arg(help = "App name")]
        app: String,
        #[arg(help = "PROCESS=COUNT pairs (e.g. web=2 worker=1)", num_args = 1..)]
        pairs: Vec<String>,
    },
}

pub async fn run(args: PsArgs, client: &DekuClient) -> Result<()> {
    match args.command {
        PsCommands::List { app } => {
            let data = client.get(&format!("/api/apps/{app}/ps")).await?;
            if let Some(procs) = data.as_array() {
                if procs.is_empty() {
                    println!("No processes running.");
                } else {
                    println!(
                        "{:<16} {:<8} {:<12} CONTAINER",
                        "PROCESS", "SCALE", "STATUS"
                    );
                    println!("{}", "-".repeat(60));
                    for p in procs {
                        let ptype = p["process_type"].as_str().unwrap_or("-");
                        let scale = p["scale"].as_i64().unwrap_or(0);
                        let status = p["status"].as_str().unwrap_or("-");
                        let cid = p["container_id"].as_str().unwrap_or("-");
                        let cid_short = &cid[..12.min(cid.len())];
                        println!("{:<16} {:<8} {:<12} {}", ptype, scale, status, cid_short);
                    }
                }
            }
        }

        PsCommands::Scale { app, pairs } => {
            let mut scale_map = serde_json::Map::new();
            for pair in &pairs {
                let (proc_type, count_str) = pair
                    .split_once('=')
                    .ok_or_else(|| anyhow::anyhow!("invalid PROCESS=COUNT: {pair}"))?;
                let count: u32 = count_str
                    .parse()
                    .map_err(|_| anyhow::anyhow!("invalid count '{count_str}'"))?;
                scale_map.insert(proc_type.to_string(), serde_json::json!(count));
            }
            client
                .post(
                    &format!("/api/apps/{app}/scale"),
                    serde_json::json!({ "scales": scale_map }),
                )
                .await?;
            println!("Scaling updated for '{app}'.");
        }
    }
    Ok(())
}
