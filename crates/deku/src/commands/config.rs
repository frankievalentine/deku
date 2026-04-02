use anyhow::Result;
use clap::Args;
use crate::client::DekuClient;

#[derive(Debug, Args)]
pub struct ConfigArgs {
    #[arg(help = "App name")]
    app: Option<String>,
}

pub async fn run(_args: ConfigArgs, _client: &DekuClient) -> Result<()> {
    println!("config: not yet implemented");
    Ok(())
}
