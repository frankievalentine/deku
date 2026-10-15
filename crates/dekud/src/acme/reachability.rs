//! Whether a domain reaches this server, checked before a certificate is
//! requested for it.
//!
//! The certificate authority allows only a few failed validations per hostname
//! per hour, and Angie re-requests every missing certificate on each reload, so
//! a domain whose DNS is not pointed here yet would use them up within a few
//! deploys. Angie answers a fixed path on each domain's port-80 server with an
//! identifier unique to this host; fetching it through the domain, the way the
//! certificate authority will, says whether a request can succeed. It follows
//! redirects and works through a CDN, where comparing A records would not.

use std::path::Path;
use std::sync::OnceLock;
use std::time::Duration;

use anyhow::{Context, Result};

/// The path Angie answers with the host identifier.
pub const CHECK_PATH: &str = "/.well-known/deku-check";

const TIMEOUT: Duration = Duration::from_secs(10);

static HOST_ID: OnceLock<String> = OnceLock::new();

/// Load this host's identifier, creating it on first start.
pub fn init(data_dir: &Path) -> Result<()> {
    let path = data_dir.join("host-id");
    let id = match std::fs::read_to_string(&path) {
        Ok(existing) if is_host_id(existing.trim()) => existing.trim().to_string(),
        _ => {
            let fresh = uuid::Uuid::new_v4().simple().to_string();
            crate::config::write_private_file(&path, fresh.as_bytes())
                .with_context(|| format!("writing {}", path.display()))?;
            fresh
        }
    };
    let _ = HOST_ID.set(id);
    Ok(())
}

fn is_host_id(value: &str) -> bool {
    value.len() == 32 && value.chars().all(|character| character.is_ascii_hexdigit())
}

/// What Angie answers on [`CHECK_PATH`]. Before [`init`] runs, as in tests, a
/// fixed value stands in, which still renders a valid configuration.
pub fn host_id() -> &'static str {
    HOST_ID.get().map_or("deku", String::as_str)
}

/// Check that `domain` reaches this server on port 80.
///
/// The error is written for the operator: it names what to fix.
pub async fn check(domain: &str) -> Result<(), String> {
    let client = reqwest::Client::builder()
        .timeout(TIMEOUT)
        .redirect(reqwest::redirect::Policy::limited(5))
        // The certificate authority follows a redirect to HTTPS without
        // checking the certificate there, so the check does too.
        .tls_danger_accept_invalid_certs(true)
        .build()
        .map_err(|error| format!("Could not start the reachability check: {error}"))?;

    let url = format!("http://{domain}{CHECK_PATH}");
    match client.get(&url).send().await {
        Ok(response) => {
            let status = response.status();
            let body = response.text().await.unwrap_or_default();
            if status.is_success() && body.trim() == host_id() {
                Ok(())
            } else {
                Err(format!(
                    "{domain} reaches a different server (it answered HTTP {}). Point its DNS at this server.",
                    status.as_u16()
                ))
            }
        }
        Err(error) => Err(explain(domain, &error).await),
    }
}

async fn explain(domain: &str, error: &reqwest::Error) -> String {
    let addresses = tokio::net::lookup_host((domain, 80)).await.map(|found| {
        found
            .map(|address| address.ip().to_string())
            .collect::<Vec<_>>()
    });
    match addresses {
        Err(_) => format!("{domain} has no DNS record yet. Add an A or AAAA record pointing at this server."),
        Ok(found) if found.is_empty() => {
            format!("{domain} has no DNS record yet. Add an A or AAAA record pointing at this server.")
        }
        Ok(found) if error.is_timeout() => format!(
            "{domain} resolves to {} but timed out on port 80. Check that the firewall allows port 80.",
            found.join(", ")
        ),
        Ok(found) => format!(
            "{domain} resolves to {} but nothing answered on port 80 there. Point the record at this server and allow port 80.",
            found.join(", ")
        ),
    }
}

#[cfg(test)]
mod tests {
    use super::{init, is_host_id};

    #[test]
    fn the_host_identifier_is_created_once_and_kept() {
        let dir = tempfile::tempdir().expect("tempdir");
        init(dir.path()).expect("init");
        let first = std::fs::read_to_string(dir.path().join("host-id")).expect("written");
        assert!(is_host_id(&first));
        init(dir.path()).expect("init again");
        let second = std::fs::read_to_string(dir.path().join("host-id")).expect("kept");
        assert_eq!(first, second, "a restart keeps the identifier");
    }
}
