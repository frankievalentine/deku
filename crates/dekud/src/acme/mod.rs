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

use anyhow::{anyhow, Result};

use crate::config::DekuConfig;

use cloudflare::CloudflareClient;

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
        "cloudflare" => Ok(Some(CloudflareClient::new(token)?)),
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
