use crate::client::DekuClient;
use anyhow::Result;
use clap::Args;

#[derive(Debug, Args)]
pub struct PsArgs {
    #[arg(help = "App name")]
    app: String,
}

pub async fn run(_args: PsArgs, _client: &DekuClient) -> Result<()> {
    println!("ps: not yet implemented");
    Ok(())
}
