use crate::client::DekuClient;
use anyhow::Result;
use clap::Args;

#[derive(Debug, Args)]
pub struct DeployArgs {
    #[arg(help = "App name")]
    app: String,
    #[arg(long, help = "Docker image to deploy")]
    image: Option<String>,
}

pub async fn run(_args: DeployArgs, _client: &DekuClient) -> Result<()> {
    println!("deploy: not yet implemented");
    Ok(())
}
