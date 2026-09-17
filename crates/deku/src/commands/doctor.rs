use anyhow::Result;
use clap::Args;

use crate::client::DekuClient;

#[derive(Debug, Args)]
pub struct DoctorArgs {}

pub async fn run(_args: DoctorArgs, client: &DekuClient) -> Result<()> {
    let data = client.get("/api/doctor").await?;
    println!("Status: {}", data["status"].as_str().unwrap_or("unknown"));

    let empty = Vec::new();
    let checks = data["checks"].as_array().unwrap_or(&empty);
    if checks.is_empty() {
        println!("No checks reported.");
        return Ok(());
    }

    println!("{:<18} {:<6} DETAIL", "CHECK", "STATE");
    println!("{}", "-".repeat(72));
    for check in checks {
        println!(
            "{:<18} {:<6} {}",
            check["name"].as_str().unwrap_or("-"),
            check["status"].as_str().unwrap_or("-"),
            check["detail"].as_str().unwrap_or(""),
        );
    }

    if data["status"].as_str() == Some("degraded") {
        std::process::exit(1);
    }
    Ok(())
}
