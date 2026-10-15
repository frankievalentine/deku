//! What Angie reports about its ACME clients.
//!
//! The status API says whether each client holds a valid certificate and
//! whether its last request failed, but not why: on failure its `details` only
//! points at the logs. The reason the certificate authority gave is in Angie's
//! error log, on a line naming the client, so that is read too.

use std::collections::HashMap;
use std::path::Path;

use anyhow::{bail, Context, Result};
use serde::Deserialize;
use tokio::io::{AsyncReadExt, AsyncWriteExt};

/// Where Angie's packages write the error log.
pub const ANGIE_ERROR_LOG: &str = "/var/log/angie/error.log";

/// How much of the end of the error log is searched for a client's last error.
const LOG_TAIL_BYTES: u64 = 512 * 1024;

/// One client's entry in `/status/http/acme_clients/`.
#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
pub struct ClientStatus {
    /// `ready`, `requesting`, `failed`, or `disabled`.
    pub state: String,
    /// `valid`, `expired`, `missing`, `mismatch`, or `error`.
    pub certificate: String,
    #[serde(default)]
    pub details: Option<String>,
    /// When Angie next requests or renews; absent while requesting or disabled.
    #[serde(default)]
    pub next_run: Option<String>,
}

impl ClientStatus {
    pub fn failed(&self) -> bool {
        self.state == "failed"
    }

    pub fn has_valid_certificate(&self) -> bool {
        self.certificate == "valid"
    }
}

/// Every ACME client's status, read from the socket Deku configures.
pub async fn fetch(socket: &Path) -> Result<HashMap<String, ClientStatus>> {
    let mut stream = tokio::net::UnixStream::connect(socket)
        .await
        .with_context(|| format!("connecting to Angie's status socket {}", socket.display()))?;
    stream
        .write_all(b"GET /acme/ HTTP/1.0\r\nHost: localhost\r\n\r\n")
        .await
        .context("asking Angie for certificate status")?;
    let mut response = Vec::new();
    stream
        .read_to_end(&mut response)
        .await
        .context("reading Angie's certificate status")?;
    parse_response(&response)
}

pub(crate) fn parse_response(response: &[u8]) -> Result<HashMap<String, ClientStatus>> {
    let text = std::str::from_utf8(response).context("Angie's status response is not UTF-8")?;
    let (head, body) = text
        .split_once("\r\n\r\n")
        .context("Angie's status response has no body")?;
    let status_line = head.lines().next().unwrap_or_default();
    if !status_line
        .split_whitespace()
        .nth(1)
        .is_some_and(|code| code == "200")
    {
        bail!("Angie's status API answered '{status_line}'");
    }
    serde_json::from_str(body).context("parsing Angie's certificate status")
}

/// The reason Angie's last failed request for `client` gave, from its error log.
pub fn last_failure(log: &Path, client: &str) -> Option<String> {
    use std::io::{Read, Seek, SeekFrom};

    let mut file = std::fs::File::open(log).ok()?;
    let length = file.metadata().ok()?.len();
    file.seek(SeekFrom::Start(length.saturating_sub(LOG_TAIL_BYTES)))
        .ok()?;
    let mut tail = String::new();
    file.take(LOG_TAIL_BYTES).read_to_string(&mut tail).ok()?;
    failure_in(&tail, client)
}

/// The last `[error]` line for `client`, reduced to what the certificate
/// authority said.
fn failure_in(log: &str, client: &str) -> Option<String> {
    let marker = format!("acme_client: \"{client}\"");
    let line = log
        .lines()
        .rev()
        .find(|line| line.contains("[error]") && line.contains(&marker))?;
    let message = line.split_once("ACME: ").map_or(line, |(_, rest)| rest);
    let message = message
        .rsplit_once(", acme_client:")
        .map_or(message, |(reason, _)| reason)
        .trim();
    let message = message
        .strip_prefix("ACME challenge failed, server returned: ")
        .unwrap_or(message);
    Some(message.to_string())
}

#[cfg(test)]
mod tests {
    use super::{failure_in, parse_response};

    #[test]
    fn the_status_response_is_read_per_client() {
        let response = b"HTTP/1.1 200 OK\r\nContent-Type: application/json\r\n\r\n{\
            \"deku_d_1\": {\"state\": \"ready\", \"certificate\": \"valid\", \"details\": \"ok\", \"next_run\": \"2026-12-14T22:01:24Z\"},\
            \"deku_d_2\": {\"state\": \"failed\", \"certificate\": \"missing\", \"details\": \"see logs\", \"next_run\": \"2026-10-16T00:01:29Z\"},\
            \"deku_d_3\": {\"state\": \"disabled\", \"certificate\": \"missing\", \"details\": \"off\"}\
        }";
        let statuses = parse_response(response).expect("parse");
        assert!(statuses["deku_d_1"].has_valid_certificate());
        assert!(statuses["deku_d_2"].failed());
        assert_eq!(statuses["deku_d_3"].next_run, None);
    }

    #[test]
    fn an_error_from_the_api_is_reported() {
        let error = parse_response(b"HTTP/1.1 404 Not Found\r\n\r\n{}").expect_err("404");
        assert!(error.to_string().contains("404"), "{error}");
    }

    #[test]
    fn the_last_error_for_a_client_is_what_the_authority_said() {
        // Lines as Angie 1.12 writes them.
        let log = "\
2026/10/15 22:00:12 [error] 1092#0: ACME: ACME challenge failed, server returned: Get \"http://plain.test/.well-known/acme-challenge/x\": dial tcp: i/o timeout, acme_client: \"deku_d_1\"
2026/10/15 22:00:12 [notice] 1092#0: will retry to obtain certificate on Fri Oct 16 00:00:12 2026, acme_client: \"deku_d_1\"
2026/10/15 22:01:07 [error] 1105#0: ACME: ACME challenge failed, server returned: Get \"http://bad.test:80/.well-known/acme-challenge/y\": dial tcp 172.30.0.99:80: connect: no route to host, acme_client: \"deku_d_2\"
";
        assert_eq!(
            failure_in(log, "deku_d_2").as_deref(),
            Some("Get \"http://bad.test:80/.well-known/acme-challenge/y\": dial tcp 172.30.0.99:80: connect: no route to host")
        );
        assert_eq!(
            failure_in(log, "deku_d_1").as_deref(),
            Some("Get \"http://plain.test/.well-known/acme-challenge/x\": dial tcp: i/o timeout")
        );
        assert_eq!(failure_in(log, "deku_d_9"), None);
    }
}
