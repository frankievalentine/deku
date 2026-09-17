use anyhow::Result;
use clap::{Args, Subcommand};

use crate::client::DekuClient;

#[derive(Debug, Args)]
pub struct BackupArgs {
    #[command(subcommand)]
    command: BackupCommands,
}

#[derive(Debug, Subcommand)]
enum BackupCommands {
    /// List configured backup schedules
    Schedules,
    /// Create or update a service's backup schedule
    Schedule {
        #[arg(help = "Service name")]
        service: String,
        #[arg(long, default_value_t = 24, help = "Hours between backups")]
        interval_hours: i64,
        #[arg(long = "keep", default_value_t = 7, help = "Number of backups to keep")]
        retention: i64,
    },
    /// Remove a service's backup schedule
    Unschedule {
        #[arg(help = "Service name")]
        service: String,
    },
    /// Show a service's backup schedule
    Status {
        #[arg(help = "Service name")]
        service: String,
    },
}

pub async fn run(args: BackupArgs, client: &DekuClient) -> Result<()> {
    match args.command {
        BackupCommands::Schedules => {
            let data = client.get("/api/backup-schedules").await?;
            let empty = Vec::new();
            let schedules = data.as_array().unwrap_or(&empty);
            if schedules.is_empty() {
                println!("No backup schedules configured.");
                return Ok(());
            }
            println!(
                "{:<20} {:<10} {:<8} {:<20} {:<7} NEXT",
                "SERVICE", "EVERY(h)", "KEEP", "LAST RUN", "STATUS"
            );
            println!("{}", "-".repeat(84));
            for schedule in schedules {
                println!(
                    "{:<20} {:<10} {:<8} {:<20} {:<7} {}",
                    schedule["service"].as_str().unwrap_or("-"),
                    schedule["interval_hours"].as_i64().unwrap_or(0),
                    schedule["retention"].as_i64().unwrap_or(0),
                    schedule["last_run_at"].as_str().unwrap_or("-"),
                    schedule["last_status"].as_str().unwrap_or("-"),
                    schedule["next_run_at"].as_str().unwrap_or("-"),
                );
            }
        }
        BackupCommands::Schedule {
            service,
            interval_hours,
            retention,
        } => {
            let data = client
                .post(
                    &format!("/api/services/{service}/backup-schedule"),
                    serde_json::json!({ "interval_hours": interval_hours, "retention": retention }),
                )
                .await?;
            println!(
                "Scheduled backups for '{service}' every {}h, keeping {} (next run: {}).",
                data["interval_hours"].as_i64().unwrap_or(interval_hours),
                data["retention"].as_i64().unwrap_or(retention),
                data["next_run_at"].as_str().unwrap_or("-"),
            );
        }
        BackupCommands::Unschedule { service } => {
            client
                .delete(&format!("/api/services/{service}/backup-schedule"))
                .await?;
            println!("Removed the backup schedule for '{service}'.");
        }
        BackupCommands::Status { service } => {
            let data = client
                .get(&format!("/api/services/{service}/backup-schedule"))
                .await?;
            if data["enabled"].as_bool().unwrap_or(false) {
                println!("Service: {service}");
                println!("Every: {}h", data["interval_hours"].as_i64().unwrap_or(0));
                println!("Keep: {}", data["retention"].as_i64().unwrap_or(0));
                println!("Last run: {}", data["last_run_at"].as_str().unwrap_or("-"));
                println!(
                    "Last status: {}",
                    data["last_status"].as_str().unwrap_or("-")
                );
                println!("Next run: {}", data["next_run_at"].as_str().unwrap_or("-"));
            } else {
                println!("No backup schedule configured for '{service}'.");
            }
        }
    }
    Ok(())
}
