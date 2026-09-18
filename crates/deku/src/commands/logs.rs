use crate::client::DekuClient;
use anyhow::Result;
use clap::Args;

#[derive(Debug, Args)]
pub struct LogsArgs {
    #[arg(help = "App name")]
    app: String,
    #[arg(short, long, help = "Follow log output live")]
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
    #[arg(long, help = "Full-text search over stored lines")]
    search: Option<String>,
    #[arg(long, help = "Only lines from this deployment id")]
    deployment: Option<String>,
    #[arg(long, help = "Only lines from this environment slug")]
    environment: Option<String>,
    #[arg(long, help = "build or runtime (default: both)")]
    source: Option<String>,
    #[arg(long, help = "stdout or stderr")]
    stream: Option<String>,
    #[arg(long, help = "Inferred level, for example ERROR")]
    level: Option<String>,
}

pub async fn run(args: LogsArgs, client: &DekuClient) -> Result<()> {
    let LogsArgs {
        app,
        follow,
        lines,
        timeout,
        search,
        deployment,
        environment,
        source,
        stream,
        level,
    } = args;

    if follow {
        println!("Streaming logs for '{app}' (Ctrl+C to stop):");
        let mut handler = |data: &str| -> bool {
            if let Ok(line) = serde_json::from_str::<serde_json::Value>(data) {
                if let Some(message) = line["message"].as_str() {
                    println!("{message}");
                }
            }
            true
        };

        let path = format!(
            "/api/apps/{app}/logs/stream{}",
            filter_query(&source, &stream, &level, None, None, environment.as_deref())
        );
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
        // Stored lines cover build and runtime output for past deployments too,
        // not just what the running containers can still report.
        let path = format!(
            "/api/apps/{app}/logs?n={lines}{}",
            filter_query(
                &source,
                &stream,
                &level,
                search.as_deref(),
                deployment.as_deref(),
                environment.as_deref()
            )
        );
        let data = client.get(&path).await?;
        if let Some(log_lines) = data["logs"].as_array() {
            for line in log_lines {
                println!("{}", line.as_str().unwrap_or(""));
            }
        }
    }

    Ok(())
}

/// Build the shared filter query string, skipping unset values.
fn filter_query(
    source: &Option<String>,
    stream: &Option<String>,
    level: &Option<String>,
    search: Option<&str>,
    deployment: Option<&str>,
    environment: Option<&str>,
) -> String {
    let mut parts: Vec<String> = Vec::new();
    for (key, value) in [
        ("source", source.as_deref()),
        ("stream", stream.as_deref()),
        ("level", level.as_deref()),
        ("search", search),
        ("deployment", deployment),
        ("environment", environment),
    ] {
        if let Some(value) = value.map(str::trim).filter(|value| !value.is_empty()) {
            parts.push(format!("{key}={}", urlencode(value)));
        }
    }

    if parts.is_empty() {
        String::new()
    } else {
        format!("&{}", parts.join("&"))
    }
}

/// Percent-encode a query value. Search terms routinely contain spaces and
/// punctuation that would otherwise break the request.
fn urlencode(value: &str) -> String {
    let mut encoded = String::with_capacity(value.len());
    for byte in value.bytes() {
        match byte {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'_' | b'.' | b'~' => {
                encoded.push(byte as char)
            }
            _ => encoded.push_str(&format!("%{byte:02X}")),
        }
    }
    encoded
}

#[cfg(test)]
mod tests {
    use super::{filter_query, urlencode};

    #[test]
    fn encodes_values_that_would_break_a_query() {
        assert_eq!(urlencode("bind port"), "bind%20port");
        assert_eq!(urlencode("a&b=c"), "a%26b%3Dc");
        assert_eq!(urlencode("plain"), "plain");
    }

    #[test]
    fn builds_only_the_filters_that_are_set() {
        assert_eq!(
            filter_query(&None, &None, &None, None, None, None),
            "",
            "no filters should not append a query string"
        );
        assert_eq!(
            filter_query(&Some("build".to_string()), &None, &None, None, None, None),
            "&source=build"
        );
        assert_eq!(
            filter_query(
                &Some("runtime".to_string()),
                &None,
                &Some("ERROR".to_string()),
                Some("connection refused"),
                None,
                Some("staging")
            ),
            "&source=runtime&level=ERROR&search=connection%20refused&environment=staging"
        );
    }

    #[test]
    fn blank_filters_are_ignored() {
        assert_eq!(
            filter_query(&Some("   ".to_string()), &None, &None, None, None, None),
            ""
        );
    }
}
