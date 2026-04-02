use anyhow::Result;
use clap::Args;
use crate::client::DekuClient;

#[derive(Debug, Args)]
pub struct DomainsArgs {
    #[arg(help = "App name")]
    app: String,
}

pub async fn run(_args: DomainsArgs, _client: &DekuClient) -> Result<()> {
    println!("domains: not yet implemented");
    Ok(())
}
