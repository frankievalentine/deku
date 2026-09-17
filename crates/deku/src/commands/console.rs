use crate::client::DekuClient;
use anyhow::Result;
use clap::Args;
use std::io::Write;

/// Run a one-off command in a fresh container from the app's image.
#[derive(Debug, Args)]
pub struct RunArgs {
    #[arg(help = "App name")]
    app: String,
    #[arg(
        required = true,
        trailing_var_arg = true,
        allow_hyphen_values = true,
        help = "Command and arguments, e.g. `deku run myapp python manage.py migrate`"
    )]
    command: Vec<String>,
}

/// Run a command inside a running app container.
#[derive(Debug, Args)]
pub struct ExecArgs {
    #[arg(help = "App name")]
    app: String,
    #[arg(
        required = true,
        trailing_var_arg = true,
        allow_hyphen_values = true,
        help = "Command and arguments, e.g. `deku exec myapp sh`"
    )]
    command: Vec<String>,
}

pub async fn run(args: RunArgs, client: &DekuClient) -> Result<()> {
    execute(client, &args.app, "run", args.command).await
}

pub async fn exec(args: ExecArgs, client: &DekuClient) -> Result<()> {
    execute(client, &args.app, "exec", args.command).await
}

async fn execute(client: &DekuClient, app: &str, action: &str, command: Vec<String>) -> Result<()> {
    let body = serde_json::json!({ "command": command });
    let mut exit_code: i32 = 0;

    client
        .stream_sse_post(&format!("/api/apps/{app}/{action}"), body, |data| {
            let Ok(value) = serde_json::from_str::<serde_json::Value>(data) else {
                return true;
            };

            if let Some(code) = value.get("exit_code").and_then(|value| value.as_i64()) {
                exit_code = code as i32;
                return false;
            }

            if let Some(line) = value.get("line").and_then(|value| value.as_str()) {
                let stream = value
                    .get("stream")
                    .and_then(|value| value.as_str())
                    .unwrap_or("stdout");
                if stream == "stderr" {
                    let _ = std::io::stderr().write_all(line.as_bytes());
                    let _ = std::io::stderr().flush();
                } else {
                    let _ = std::io::stdout().write_all(line.as_bytes());
                    let _ = std::io::stdout().flush();
                }
            }
            true
        })
        .await?;

    if exit_code != 0 {
        std::process::exit(exit_code);
    }
    Ok(())
}
