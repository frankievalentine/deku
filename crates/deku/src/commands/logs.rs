use crate::client::DekuClient;
use anyhow::Result;
use clap::Args;

#[derive(Debug, Args)]
pub struct LogsArgs {
    #[arg(help = "App name")]
    app: String,
    #[arg(short, long, help = "Follow log output (SSE stream)")]
    follow: bool,
    #[arg(
        short = 'n',
        long,
        help = "Number of lines to show",
        default_value = "100"
    )]
    lines: u32,
}

pub async fn run(args: LogsArgs, client: &DekuClient) -> Result<()> {
    let LogsArgs { app, follow, lines } = args;

    if follow {
        println!("Streaming logs for '{app}' (Ctrl+C to stop):");
        client
            .stream_sse(&format!("/api/apps/{app}/events/stream"), |data| {
                if let Ok(evt) = serde_json::from_str::<serde_json::Value>(data) {
                    let etype = evt["event_type"].as_str().unwrap_or("");
                    if let Some(line) = evt["payload"]["line"].as_str() {
                        println!("[{etype}] {line}");
                    }
                }
            })
            .await?;
    } else {
        let data = client
            .get(&format!("/api/apps/{app}/logs?tail={lines}"))
            .await?;
        if let Some(log_lines) = data.as_array() {
            for line in log_lines {
                println!("{}", line.as_str().unwrap_or(""));
            }
        }
    }

    Ok(())
}
