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
    #[arg(
        long,
        help = "With --follow, stop after this many seconds with no output"
    )]
    timeout: Option<u64>,
}

pub async fn run(args: LogsArgs, client: &DekuClient) -> Result<()> {
    let LogsArgs {
        app,
        follow,
        lines,
        timeout,
    } = args;

    if follow {
        println!("Streaming logs for '{app}' (Ctrl+C to stop):");
        let mut handler = |data: &str| -> bool {
            if let Ok(evt) = serde_json::from_str::<serde_json::Value>(data) {
                let etype = evt["event_type"].as_str().unwrap_or("");
                let payload = crate::client::event_payload(&evt);
                if let Some(line) = payload["line"].as_str() {
                    println!("[{etype}] {line}");
                }
            }
            true
        };
        let path = format!("/api/apps/{app}/events/stream");
        match timeout {
            Some(seconds) => {
                client
                    .stream_sse_idle(&path, std::time::Duration::from_secs(seconds), &mut handler)
                    .await?;
            }
            None => {
                client.stream_sse(&path, &mut handler).await?;
            }
        }
    } else {
        let data = client
            .get(&format!("/api/apps/{app}/logs?n={lines}"))
            .await?;
        if let Some(log_lines) = data["logs"].as_array() {
            for line in log_lines {
                println!("{}", line.as_str().unwrap_or(""));
            }
        }
    }

    Ok(())
}
