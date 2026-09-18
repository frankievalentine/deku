//! Settings for automatic certificates.
//!
//! These are daemon-wide: one ACME client covers every app's environment and
//! per-deployment hostnames under the configured global domain.

use axum::{extract::State, http::StatusCode, response::IntoResponse, Json};
use serde::Deserialize;

use super::{bad_request, internal_error, SharedState};
use crate::acme;
use crate::config::{AcmeConfig, TokenSource};

/// The settings as the dashboard needs them.
///
/// The token itself is never returned: a caller learns whether one is set and
/// which configuration supplied it, which is everything the UI has to render.
fn settings_json(
    cfg: &crate::config::DekuConfig,
    token_configured: bool,
    token_source: Option<TokenSource>,
    account_email: Option<String>,
) -> serde_json::Value {
    serde_json::json!({
        "enabled": cfg.acme.enabled,
        "directory": cfg.acme.directory,
        // The contact the ACME account registers is the host's certificate
        // account email, which has its own setting.
        "account_email": account_email,
        "provider": cfg.acme.provider,
        "wildcard": cfg.acme.wildcard,
        "client_path": cfg.acme.client_path,
        "api_token_file": cfg.acme.api_token_file,
        "token_configured": token_configured,
        "token_source": token_source,
    })
}

/// Read the daemon config and describe it for a UI.
async fn current_settings() -> anyhow::Result<serde_json::Value> {
    let cfg = crate::config::load()?;
    let token = cfg.acme_api_token()?;
    let account_email = crate::services::letsencrypt::get_global_email(&cfg).await?;
    Ok(settings_json(
        &cfg,
        token.is_some(),
        token.map(|token| token.source),
        account_email,
    ))
}

#[utoipa::path(
    get,
    path = "/api/acme",
    tag = "acme",
    responses((status = 200, description = "Automatic certificate settings"))
)]
pub async fn get_acme(State(_state): State<SharedState>) -> impl IntoResponse {
    match current_settings().await {
        Ok(settings) => (StatusCode::OK, Json(settings)).into_response(),
        Err(error) => internal_error(error).into_response(),
    }
}

#[derive(Deserialize, utoipa::ToSchema)]
pub struct AcmeSettingsBody {
    pub enabled: bool,
    pub directory: String,
    pub provider: String,
    pub wildcard: bool,
    /// A new provider token. Omit or leave empty to keep the stored one.
    #[serde(default)]
    pub api_token: Option<String>,
}

#[utoipa::path(
    put,
    path = "/api/acme",
    tag = "acme",
    request_body = AcmeSettingsBody,
    responses((status = 200, description = "Saved automatic certificate settings"))
)]
pub async fn put_acme(
    State(state): State<SharedState>,
    Json(body): Json<AcmeSettingsBody>,
) -> impl IntoResponse {
    let mut cfg = match crate::config::load() {
        Ok(cfg) => cfg,
        Err(error) => return internal_error(error).into_response(),
    };

    // A supplied token is written to its own file first, so the config records
    // only the path to it.
    if let Some(token) = body
        .api_token
        .as_deref()
        .filter(|value| !value.trim().is_empty())
    {
        match acme::store_api_token(&cfg, token) {
            Ok(path) => cfg.acme.api_token_file = Some(path),
            Err(error) => return bad_request(error.to_string()).into_response(),
        }
    }

    cfg.acme = AcmeConfig {
        enabled: body.enabled,
        directory: body.directory.trim().to_string(),
        provider: body.provider.trim().to_string(),
        // The token now lives in a file; clear any inline copy so the secret
        // cannot linger in the config.
        api_token: None,
        api_token_file: cfg.acme.api_token_file.clone(),
        client_path: cfg.acme.client_path.clone(),
        wildcard: body.wildcard,
    };

    // Reject a configuration that cannot work before it is written, so the UI
    // reports the reason instead of the daemon failing later.
    if let Err(error) = cfg.acme.validate(cfg.global_domain.as_deref()) {
        return bad_request(error.to_string()).into_response();
    }

    if let Err(error) = crate::config::save(&cfg) {
        return internal_error(error).into_response();
    }

    state.events.emit(
        None,
        "acme.configured",
        Some(serde_json::json!({
            "enabled": cfg.acme.enabled,
            "wildcard": cfg.acme.wildcard,
        })),
    );

    match current_settings().await {
        Ok(settings) => (StatusCode::OK, Json(settings)).into_response(),
        Err(error) => internal_error(error).into_response(),
    }
}

#[derive(Deserialize, utoipa::ToSchema)]
pub struct AcmeVerifyBody {
    /// A token to check before saving it. Omit to check the stored one.
    #[serde(default)]
    pub api_token: Option<String>,
}

#[utoipa::path(
    post,
    path = "/api/acme/verify",
    tag = "acme",
    request_body = AcmeVerifyBody,
    responses((status = 200, description = "The token is valid and the zone resolves"))
)]
pub async fn verify_acme(
    State(_state): State<SharedState>,
    Json(body): Json<AcmeVerifyBody>,
) -> impl IntoResponse {
    let cfg = match crate::config::load() {
        Ok(cfg) => cfg,
        Err(error) => return internal_error(error).into_response(),
    };

    // A token supplied here is checked without being stored, so a token can be
    // tested before it is committed.
    let token = match body
        .api_token
        .as_deref()
        .filter(|value| !value.trim().is_empty())
    {
        Some(token) => token.trim().to_string(),
        None => match cfg.acme_api_token() {
            Ok(Some(token)) => token.value,
            Ok(None) => {
                return bad_request("no API token is configured yet").into_response();
            }
            Err(error) => return bad_request(error.to_string()).into_response(),
        },
    };

    match cfg.acme.provider.as_str() {
        "cloudflare" => {}
        other => {
            return bad_request(format!(
                "unknown ACME DNS provider '{other}'; supported providers: cloudflare"
            ))
            .into_response();
        }
    }

    let client = match acme::CloudflareClient::new(token) {
        Ok(client) => client,
        Err(error) => return internal_error(error).into_response(),
    };

    if let Err(error) = client.verify_token().await {
        return bad_request(error.to_string()).into_response();
    }

    let Some(zone) = cfg.global_domain.as_deref() else {
        return bad_request("no global_domain is configured to look up a zone for").into_response();
    };

    match client.zone_id(zone).await {
        Ok(zone_id) => (
            StatusCode::OK,
            Json(serde_json::json!({ "ok": true, "zone_id": zone_id })),
        )
            .into_response(),
        Err(error) => bad_request(error.to_string()).into_response(),
    }
}
