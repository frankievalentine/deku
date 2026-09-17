use anyhow::Result;
use clap::Args;

use crate::client::DekuClient;

#[derive(Debug, Args)]
pub struct AlertsArgs {
    /// Include resolved alerts instead of only active ones
    #[arg(long)]
    all: bool,
}

pub async fn run(args: AlertsArgs, client: &DekuClient) -> Result<()> {
    let path = if args.all {
        "/api/alerts?include_resolved=true"
    } else {
        "/api/alerts"
    };
    let data = client.get(path).await?;

    let empty = Vec::new();
    let alerts = data.as_array().unwrap_or(&empty);
    if alerts.is_empty() {
        if args.all {
            println!("No alerts recorded.");
        } else {
            println!("No active alerts.");
        }
        return Ok(());
    }

    println!(
        "{:<9} {:<28} {:<18} {:<20} MESSAGE",
        "SEVERITY", "RULE", "SUBJECT", "FIRST SEEN"
    );
    println!("{}", "-".repeat(120));
    for alert in alerts {
        let resolved = alert["resolved_at"]
            .as_str()
            .map(|_| " (resolved)")
            .unwrap_or("");
        println!(
            "{:<9} {:<28} {:<18} {:<20} {}{}",
            alert["severity"].as_str().unwrap_or("-"),
            alert["rule"].as_str().unwrap_or("-"),
            alert["subject"].as_str().unwrap_or("-"),
            alert["first_seen_at"].as_str().unwrap_or("-"),
            alert["message"].as_str().unwrap_or(""),
            resolved,
        );
    }

    Ok(())
}
