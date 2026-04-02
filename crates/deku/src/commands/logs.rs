use crate::client::DekuClient;
use anyhow::Result;
use clap::Args;

#[derive(Debug, Args)]
pub struct LogsArgs {
    #[arg(help = "App name")]
    app: String,
    #[arg(short, long, help = "Follow log output")]
    follow: bool,
    #[arg(
        short = 'n',
        long,
        help = "Number of lines to show",
        default_value = "100"
    )]
    lines: u32,
}

pub async fn run(_args: LogsArgs, _client: &DekuClient) -> Result<()> {
    println!("logs: not yet implemented");
    Ok(())
}
