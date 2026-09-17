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
    /// Show or set memory/CPU limits
    Limits {
        #[arg(help = "App name")]
        app: String,
        #[arg(long, help = "Process type (default: all processes)")]
        process: Option<String>,
        #[arg(long, help = "CPU limit, e.g. 0.5 or 500m")]
        cpu: Option<String>,
        #[arg(long, help = "Memory limit, e.g. 512m or 1g")]
        memory: Option<String>,
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

        PsCommands::Limits {
            app,
            process,
            cpu,
            memory,
        } => {
            if cpu.is_none() && memory.is_none() {
                let data = client.get(&format!("/api/apps/{app}/limits")).await?;
                let empty = Vec::new();
                let limits = data.as_array().unwrap_or(&empty);
                if limits.is_empty() {
                    println!("No limits set for '{app}'.");
                } else {
                    println!("{:<16} {:<10} {:<10}", "PROCESS", "CPU", "MEMORY");
                    println!("{}", "-".repeat(38));
                    for limit in limits {
                        println!(
                            "{:<16} {:<10} {:<10}",
                            limit["process_type"].as_str().unwrap_or("-"),
                            limit["cpu"].as_str().unwrap_or("-"),
                            limit["memory"].as_str().unwrap_or("-"),
                        );
                    }
                }
            } else {
                let data = client
                    .post(
                        &format!("/api/apps/{app}/limits"),
                        serde_json::json!({
                            "process_type": process,
                            "cpu": cpu,
                            "memory": memory,
                        }),
                    )
                    .await?;
                println!(
                    "Limits set for '{}' (cpu: {}, memory: {}). {}",
                    app,
                    data["cpu"].as_str().unwrap_or("-"),
                    data["memory"].as_str().unwrap_or("-"),
                    data["note"].as_str().unwrap_or("")
                );
            }
        }
    }
    Ok(())
}
