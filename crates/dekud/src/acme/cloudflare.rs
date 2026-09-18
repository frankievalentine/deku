//! The slice of Cloudflare's DNS API needed to answer a DNS-01 challenge.

use anyhow::{anyhow, Context, Result};
use serde::Deserialize;
use std::time::Duration;

/// Cloudflare's API root. Tests point this at a local server.
const DEFAULT_BASE_URL: &str = "https://api.cloudflare.com/client/v4";
const REQUEST_TIMEOUT: Duration = Duration::from_secs(15);
/// The challenge record only has to outlive validation, so it is short-lived.
const CHALLENGE_TTL: u32 = 120;

/// Every Cloudflare response is wrapped in this envelope, including failures.
///
/// The result is kept as a raw value and decoded per call: a rejected request
/// has no result at all, and a `#[serde(default)]` generic field would demand
/// more of the caller than the API does.
#[derive(Deserialize)]
struct Envelope {
    success: bool,
    #[serde(default)]
    errors: Vec<ApiError>,
    #[serde(default)]
    result: serde_json::Value,
}

#[derive(Deserialize)]
struct ApiError {
    message: String,
}

#[derive(Deserialize)]
struct Zone {
    id: String,
}

#[derive(Deserialize)]
struct TokenStatus {
    status: String,
}

#[derive(Deserialize)]
struct RecordId {
    id: String,
}

#[derive(Deserialize)]
struct ExistingRecord {
    id: String,
    content: String,
}

/// Talks to Cloudflare's DNS API with an operator-supplied token.
pub struct CloudflareClient {
    http: reqwest::Client,
    token: String,
    base_url: String,
}

impl std::fmt::Debug for CloudflareClient {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        // The token must not reach a log line through a derived Debug.
        f.debug_struct("CloudflareClient")
            .field("base_url", &self.base_url)
            .field("token", &"<redacted>")
            .finish()
    }
}

impl CloudflareClient {
    pub fn new(token: impl Into<String>) -> Result<Self> {
        Self::with_base_url(token, DEFAULT_BASE_URL)
    }

    pub fn with_base_url(token: impl Into<String>, base_url: impl Into<String>) -> Result<Self> {
        let http = reqwest::Client::builder()
            .timeout(REQUEST_TIMEOUT)
            .build()
            .context("building the ACME DNS client")?;
        Ok(Self {
            http,
            token: token.into(),
            base_url: base_url.into().trim_end_matches('/').to_string(),
        })
    }

    /// A full endpoint URL, with query parameters encoded for us.
    fn endpoint(&self, path: &str, params: &[(&str, &str)]) -> Result<reqwest::Url> {
        let mut url = reqwest::Url::parse(&format!("{}{path}", self.base_url))
            .with_context(|| format!("building the Cloudflare URL for {path}"))?;
        if !params.is_empty() {
            url.query_pairs_mut().extend_pairs(params.iter().copied());
        }
        Ok(url)
    }

    /// Send a request and unwrap Cloudflare's envelope.
    ///
    /// A `success: false` body is an error even on HTTP 200, so the envelope is
    /// checked rather than the status code alone.
    async fn send<T: for<'de> Deserialize<'de>>(
        &self,
        request: reqwest::RequestBuilder,
        what: &str,
    ) -> Result<T> {
        let response = request
            .bearer_auth(&self.token)
            .send()
            .await
            .with_context(|| format!("{what}: request to Cloudflare failed"))?;

        let status = response.status();
        let body = response.text().await.unwrap_or_default();

        let envelope: Envelope = serde_json::from_str(&body).with_context(|| {
            format!(
                "{what}: unexpected Cloudflare response (HTTP {status}): {}",
                body.chars().take(200).collect::<String>()
            )
        })?;

        if !envelope.success {
            let detail = envelope
                .errors
                .iter()
                .map(|error| error.message.as_str())
                .collect::<Vec<_>>()
                .join("; ");
            anyhow::bail!("{what}: Cloudflare rejected the request (HTTP {status}): {detail}");
        }

        serde_json::from_value(envelope.result)
            .map_err(|error| anyhow!("{what}: Cloudflare returned an unexpected result: {error}"))
    }

    /// Confirm the token works, so a bad one is reported before it is relied on.
    pub async fn verify_token(&self) -> Result<()> {
        let status: TokenStatus = self
            .send(
                self.http.get(self.endpoint("/user/tokens/verify", &[])?),
                "verifying the Cloudflare token",
            )
            .await?;

        if status.status != "active" {
            anyhow::bail!(
                "verifying the Cloudflare token: token status is '{}', expected 'active'",
                status.status
            );
        }
        Ok(())
    }

    /// The zone id for a domain, looked up so the operator never pastes one.
    pub async fn zone_id(&self, zone: &str) -> Result<String> {
        let zones: Vec<Zone> = self
            .send(
                self.http.get(self.endpoint("/zones", &[("name", zone)])?),
                &format!("looking up the Cloudflare zone '{zone}'"),
            )
            .await?;

        zones.into_iter().next().map(|zone| zone.id).ok_or_else(|| {
            anyhow!(
                "looking up the Cloudflare zone '{zone}': no such zone is visible to this token"
            )
        })
    }

    /// Create the record an `add` challenge needs, returning its id.
    pub async fn add_challenge(&self, zone_id: &str, name: &str, value: &str) -> Result<String> {
        let created: RecordId = self
            .send(
                self.http
                    .post(self.endpoint(&format!("/zones/{zone_id}/dns_records"), &[])?)
                    .json(&serde_json::json!({
                        "type": "TXT",
                        "name": name,
                        "content": value,
                        "ttl": CHALLENGE_TTL,
                    })),
                &format!("creating the challenge record for '{name}'"),
            )
            .await?;
        Ok(created.id)
    }

    /// Remove the record an earlier `add` created.
    ///
    /// A record that is already gone is success: a retry after a partial failure
    /// must not fail because the work was done the first time.
    pub async fn remove_challenge(&self, zone_id: &str, name: &str, value: &str) -> Result<()> {
        let records: Vec<ExistingRecord> = self
            .send(
                self.http.get(self.endpoint(
                    &format!("/zones/{zone_id}/dns_records"),
                    &[("type", "TXT"), ("name", name)],
                )?),
                &format!("listing challenge records for '{name}'"),
            )
            .await?;

        for record in records.iter().filter(|record| record.content == value) {
            let _: serde_json::Value = self
                .send(
                    self.http.delete(
                        self.endpoint(&format!("/zones/{zone_id}/dns_records/{}", record.id), &[])?,
                    ),
                    &format!("removing the challenge record for '{name}'"),
                )
                .await?;
        }

        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::CloudflareClient;
    use axum::extract::{Path, Query, State};
    use axum::routing::{delete, get};
    use axum::{Json, Router};
    use std::collections::HashMap;
    use std::sync::{Arc, Mutex};

    /// What the mock Cloudflare recorded, so tests can assert on the calls.
    #[derive(Default)]
    struct Calls {
        requests: Vec<(String, String)>,
    }

    async fn record(state: &Arc<Mutex<Calls>>, method: &str, path: &str) {
        state
            .lock()
            .expect("calls lock")
            .requests
            .push((method.to_string(), path.to_string()));
    }

    async fn verify(State(state): State<Arc<Mutex<Calls>>>) -> Json<serde_json::Value> {
        record(&state, "GET", "/user/tokens/verify").await;
        Json(serde_json::json!({
            "success": true,
            "result": { "id": "token-1", "status": "active" },
        }))
    }

    async fn zones(
        State(state): State<Arc<Mutex<Calls>>>,
        Query(params): Query<HashMap<String, String>>,
    ) -> Json<serde_json::Value> {
        record(&state, "GET", "/zones").await;
        let result = if params.get("name").map(String::as_str) == Some("apps.test") {
            serde_json::json!([{ "id": "zone-1", "name": "apps.test" }])
        } else {
            serde_json::json!([])
        };
        Json(serde_json::json!({ "success": true, "result": result }))
    }

    async fn create_record(
        State(state): State<Arc<Mutex<Calls>>>,
        Path(zone_id): Path<String>,
        Json(body): Json<serde_json::Value>,
    ) -> Json<serde_json::Value> {
        record(&state, "POST", &format!("/zones/{zone_id}/dns_records")).await;
        assert_eq!(body["type"], "TXT");
        assert_eq!(body["ttl"], 120);
        Json(serde_json::json!({
            "success": true,
            "result": { "id": "record-1" },
        }))
    }

    async fn list_records(
        State(state): State<Arc<Mutex<Calls>>>,
        Path(zone_id): Path<String>,
    ) -> Json<serde_json::Value> {
        record(&state, "GET", &format!("/zones/{zone_id}/dns_records")).await;
        Json(serde_json::json!({
            "success": true,
            "result": [
                { "id": "record-1", "content": "keyauth-value" },
                { "id": "record-2", "content": "someone-elses-value" },
            ],
        }))
    }

    async fn delete_record(
        State(state): State<Arc<Mutex<Calls>>>,
        Path((zone_id, record_id)): Path<(String, String)>,
    ) -> Json<serde_json::Value> {
        record(
            &state,
            "DELETE",
            &format!("/zones/{zone_id}/dns_records/{record_id}"),
        )
        .await;
        Json(serde_json::json!({ "success": true, "result": { "id": record_id } }))
    }

    /// A stand-in Cloudflare, returning the given base url.
    async fn mock_cloudflare() -> (String, Arc<Mutex<Calls>>) {
        let state: Arc<Mutex<Calls>> = Arc::default();
        let app = Router::new()
            .route("/user/tokens/verify", get(verify))
            .route("/zones", get(zones))
            .route(
                "/zones/{zone_id}/dns_records",
                get(list_records).post(create_record),
            )
            .route(
                "/zones/{zone_id}/dns_records/{record_id}",
                delete(delete_record),
            )
            .with_state(state.clone());

        let listener = tokio::net::TcpListener::bind("127.0.0.1:0")
            .await
            .expect("bind");
        let addr = listener.local_addr().expect("addr");
        tokio::spawn(async move {
            axum::serve(listener, app).await.expect("mock server");
        });

        (format!("http://{addr}"), state)
    }

    async fn client() -> (CloudflareClient, Arc<Mutex<Calls>>) {
        let (base_url, calls) = mock_cloudflare().await;
        let client = CloudflareClient::with_base_url("test-token", base_url).expect("client");
        (client, calls)
    }

    #[tokio::test]
    async fn verifies_the_token_and_resolves_a_zone() {
        let (client, _calls) = client().await;

        client.verify_token().await.expect("token verifies");
        assert_eq!(client.zone_id("apps.test").await.expect("zone"), "zone-1");
    }

    #[tokio::test]
    async fn an_unknown_zone_names_the_zone() {
        let (client, _calls) = client().await;

        let error = client.zone_id("nope.test").await.expect_err("unknown zone");
        assert!(
            error.to_string().contains("nope.test"),
            "the error should name the zone: {error}"
        );
    }

    #[tokio::test]
    async fn adds_and_removes_only_the_matching_record() {
        let (client, calls) = client().await;

        let id = client
            .add_challenge("zone-1", "_acme-challenge.apps.test", "keyauth-value")
            .await
            .expect("create");
        assert_eq!(id, "record-1");

        client
            .remove_challenge("zone-1", "_acme-challenge.apps.test", "keyauth-value")
            .await
            .expect("remove");

        // Only the record whose content matches is deleted: another value under
        // the same name belongs to a different challenge.
        let requests = calls.lock().expect("calls lock").requests.clone();
        assert!(
            requests
                .iter()
                .any(|(method, path)| method == "POST" && path == "/zones/zone-1/dns_records"),
            "the challenge record was not created: {requests:?}"
        );
        assert!(
            requests
                .iter()
                .any(|(method, path)| method == "DELETE"
                    && path == "/zones/zone-1/dns_records/record-1"),
            "the matching record was not deleted: {requests:?}"
        );
        assert!(
            !requests.iter().any(|(_, path)| path.ends_with("/record-2")),
            "a record holding another value must be left alone: {requests:?}"
        );
    }

    #[tokio::test]
    async fn a_rejected_request_surfaces_cloudflares_message() {
        // A token that is not valid: Cloudflare answers 200 with success=false.
        let app = Router::new().route(
            "/zones",
            get(|| async {
                Json(serde_json::json!({
                    "success": false,
                    "errors": [{ "code": 1000, "message": "Invalid API Token" }],
                }))
            }),
        );

        let listener = tokio::net::TcpListener::bind("127.0.0.1:0")
            .await
            .expect("bind");
        let addr = listener.local_addr().expect("addr");
        tokio::spawn(async move {
            axum::serve(listener, app).await.expect("mock server");
        });

        let client =
            CloudflareClient::with_base_url("bad-token", format!("http://{addr}")).expect("client");
        let error = client
            .zone_id("apps.test")
            .await
            .expect_err("a rejected token must fail");
        assert!(
            error.to_string().contains("Invalid API Token"),
            "Cloudflare's message should be preserved: {error}"
        );
    }

    #[test]
    fn debug_does_not_leak_the_token() {
        let client = CloudflareClient::new("super-secret-token").expect("client");
        let rendered = format!("{client:?}");
        assert!(
            !rendered.contains("super-secret-token"),
            "the token must not appear in debug output: {rendered}"
        );
        assert!(rendered.contains("redacted"));
    }
}
