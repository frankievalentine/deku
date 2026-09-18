//! Certificate issuance through the DNS-01 challenge.
//!
//! Angie handles the ACME protocol, ordering, renewal, and serving. The one
//! thing it cannot do is write to a DNS provider, so for a wildcard certificate
//! the daemon answers Angie's `acme_hook` callback by creating and removing the
//! `_acme-challenge` TXT record the certificate authority looks up.
//!
//! Only that slice of provider access lives here. Everything about how the
//! certificate is requested and used stays in the proxy configuration.

pub mod cloudflare;
pub mod file;

/// The headers the hook location sends and the hook handler reads.
///
/// Spelled once because the two sides are written in different files: the
/// rendered Angie configuration sets these, and the daemon's hook handler reads
/// them. A mismatch between two separate literals would fail only at the moment
/// a certificate is requested, and nothing about it is obvious in either place.
///
/// HTTP header names are case-insensitive; these are lower case because that is
/// how the handler looks them up.
pub mod headers {
    /// Whether the challenge record should be added or removed.
    pub const ACTION: &str = "x-deku-acme-hook";
    /// The name being validated, without the `*.` prefix.
    pub const DOMAIN: &str = "x-deku-acme-domain";
    /// The value to publish for a DNS challenge.
    pub const KEYAUTH: &str = "x-deku-acme-keyauth";
    /// The validation type, which has to be `dns` here.
    pub const CHALLENGE: &str = "x-deku-acme-challenge";
    /// The ACME client that is asking.
    pub const CLIENT: &str = "x-deku-acme-client";

    /// Each header with the Angie variable it carries.
    ///
    /// The hook location is rendered from this, so a header cannot be sent
    /// without the variable that gives it a value.
    pub const SENT: &[(&str, &str)] = &[
        (ACTION, "$acme_hook_name"),
        (DOMAIN, "$acme_hook_domain"),
        (KEYAUTH, "$acme_hook_keyauth"),
        (CHALLENGE, "$acme_hook_challenge"),
        (CLIENT, "$acme_hook_client"),
    ];
}

use anyhow::{anyhow, Result};

use crate::config::DekuConfig;

pub use cloudflare::CloudflareClient;

/// Which half of a DNS-01 challenge Angie is asking for.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ChallengeAction {
    /// Publish the record the certificate authority will look up.
    Add,
    /// Remove it again once validation is over.
    Remove,
}

impl ChallengeAction {
    /// Parse the value Angie sends in `$acme_hook_name`.
    pub fn parse(value: &str) -> Result<Self> {
        match value {
            "add" => Ok(Self::Add),
            "remove" => Ok(Self::Remove),
            other => Err(anyhow!(
                "unknown ACME hook action '{other}'; expected 'add' or 'remove'"
            )),
        }
    }
}

/// The DNS name a challenge for `domain` is published under.
pub fn challenge_record_name(domain: &str) -> String {
    format!("_acme-challenge.{domain}")
}

/// Whether this daemon is responsible for a domain.
///
/// The hook endpoint is reachable by anything that can write to the daemon's
/// socket, so it refuses to touch records outside the operator's own domain.
/// The wildcard challenge arrives without its `*.` prefix, so the apex and any
/// subdomain of it are both legitimate.
pub fn authorize_domain(global_domain: Option<&str>, domain: &str) -> Result<()> {
    let Some(global_domain) = global_domain.filter(|value| !value.trim().is_empty()) else {
        anyhow::bail!("no global_domain is configured, so no ACME challenge can be answered");
    };

    if domain == global_domain || domain.ends_with(&format!(".{global_domain}")) {
        return Ok(());
    }

    anyhow::bail!("refusing to write a DNS challenge record for '{domain}', which is outside '{global_domain}'")
}

/// Answer one `acme_hook` call by writing or removing the challenge record.
pub async fn apply_challenge(
    client: &CloudflareClient,
    zone_id: &str,
    domain: &str,
    action: ChallengeAction,
    keyauth: &str,
) -> Result<()> {
    let name = challenge_record_name(domain);
    match action {
        ChallengeAction::Add => {
            client.add_challenge(zone_id, &name, keyauth).await?;
        }
        ChallengeAction::Remove => {
            client.remove_challenge(zone_id, &name, keyauth).await?;
        }
    }
    Ok(())
}

/// Where the provider API token is stored when it is set from a UI.
///
/// The token lives in its own `0600` file rather than in the daemon config, so
/// a copy of `config.toml` never carries the secret.
pub fn api_token_path(cfg: &DekuConfig) -> std::path::PathBuf {
    cfg.data_dir.join("acme-api-token")
}

/// Store the provider API token, returning the path it was written to.
pub fn store_api_token(cfg: &DekuConfig, token: &str) -> Result<std::path::PathBuf> {
    let token = token.trim();
    if token.is_empty() {
        anyhow::bail!("the API token cannot be empty");
    }

    let path = api_token_path(cfg);
    crate::config::write_private_file(&path, token.as_bytes())?;
    Ok(path)
}

/// The provider client for the configured provider, when ACME is enabled.
///
/// `Ok(None)` means the operator has not turned ACME on, which is not an error.
pub fn provider_client(cfg: &DekuConfig) -> Result<Option<CloudflareClient>> {
    // Reject a configuration that cannot work before a challenge is attempted.
    cfg.acme.validate(cfg.global_domain.as_deref())?;

    if !cfg.acme.enabled {
        return Ok(None);
    }

    let token = cfg.acme_api_token()?.ok_or_else(|| {
        anyhow!("ACME is enabled but no API token is configured; set api_token_file or DEKU_ACME_API_TOKEN")
    })?;

    match cfg.acme.provider.as_str() {
        "cloudflare" => Ok(Some(CloudflareClient::new(token.value)?)),
        other => Err(anyhow!(
            "unknown ACME DNS provider '{other}'; supported providers: cloudflare"
        )),
    }
}

#[cfg(test)]
mod tests {
    use super::{authorize_domain, challenge_record_name, ChallengeAction};

    #[test]
    fn a_challenge_record_is_named_under_the_domain() {
        assert_eq!(
            challenge_record_name("apps.test"),
            "_acme-challenge.apps.test"
        );
    }

    #[test]
    fn the_configured_domain_and_its_subdomains_are_authorized() {
        // The wildcard challenge arrives without the `*.` prefix.
        authorize_domain(Some("apps.test"), "apps.test").expect("the apex is ours");
        authorize_domain(Some("apps.test"), "demo-staging.apps.test").expect("a subdomain is ours");
    }

    #[test]
    fn a_domain_outside_the_configured_one_is_refused() {
        let error = authorize_domain(Some("apps.test"), "example.com")
            .expect_err("an unrelated domain must be refused");
        assert!(
            error.to_string().contains("example.com") && error.to_string().contains("apps.test"),
            "the refusal should name both domains: {error}"
        );

        // A suffix that merely looks like the domain must not pass.
        let error = authorize_domain(Some("apps.test"), "notapps.test")
            .expect_err("a lookalike domain must be refused");
        assert!(error.to_string().contains("refusing"), "{error}");
    }

    #[test]
    fn authorizing_without_a_configured_domain_is_an_error() {
        let error = authorize_domain(None, "apps.test")
            .expect_err("no global domain means nothing to authorize");
        assert!(error.to_string().contains("global_domain"), "{error}");
    }

    #[test]
    fn a_hook_action_is_parsed_or_rejected() {
        assert_eq!(
            ChallengeAction::parse("add").expect("add"),
            ChallengeAction::Add
        );
        assert_eq!(
            ChallengeAction::parse("remove").expect("remove"),
            ChallengeAction::Remove
        );
        let error = ChallengeAction::parse("publish").expect_err("unknown action");
        assert!(error.to_string().contains("publish"), "{error}");
    }
}
