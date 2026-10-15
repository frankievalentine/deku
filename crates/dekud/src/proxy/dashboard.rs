//! The dashboard's own Angie vhost.
//!
//! The daemon serves the dashboard over plain HTTP on `api_bind`, which new
//! installs keep on loopback. Setting `dashboard_domain` publishes it through
//! Angie instead, over HTTPS only: a dashboard reached over the network without
//! TLS would carry the token in the clear, so without a certificate nothing is
//! published and `deku doctor` says why.
//!
//! The certificate is the wildcard Angie maintains for the global domain, which
//! covers one label below it, so the dashboard domain must be such a name.

use std::net::IpAddr;
use std::path::Path;

use anyhow::{Context, Result};
use handlebars::Handlebars;
use serde_json::json;

use crate::config::DekuConfig;

/// The config's name in the app config directory, written as
/// `-dashboard.conf`. App names cannot start with '-', so no app's config can
/// ever share it.
pub const CONFIG_NAME: &str = "-dashboard";

const TEMPLATE: &str = include_str!("templates/dashboard.conf.hbs");

/// What the dashboard vhost should be.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Desired {
    /// No `dashboard_domain` is set: the dashboard is not published.
    Disabled,
    /// Publish at this domain.
    Published { domain: String },
    /// A domain is set but cannot be served over HTTPS yet, for this reason.
    Unavailable { domain: String, reason: String },
}

/// Decide what the dashboard vhost should be.
///
/// `wildcard_issued` is whether the wildcard certificate is on disk, and
/// `app_domains` every domain an app has claimed, which the dashboard must not
/// take over.
pub fn desired(cfg: &DekuConfig, wildcard_issued: bool, app_domains: &[String]) -> Desired {
    let Some(domain) = cfg
        .dashboard_domain
        .as_deref()
        .map(str::trim)
        .filter(|domain| !domain.is_empty())
        .map(str::to_ascii_lowercase)
    else {
        return Desired::Disabled;
    };

    let unavailable = |reason: String| Desired::Unavailable {
        domain: domain.clone(),
        reason,
    };

    if !domain
        .chars()
        .all(|c| c.is_ascii_alphanumeric() || c == '-' || c == '.')
    {
        return unavailable(format!("'{domain}' is not a valid hostname"));
    }

    let Some(global) = cfg
        .global_domain
        .as_deref()
        .map(str::trim)
        .filter(|global| !global.is_empty())
    else {
        return unavailable(
            "no global_domain is set, so there is no wildcard certificate to serve it with"
                .to_string(),
        );
    };
    let covered = domain
        .strip_suffix(&format!(".{}", global.to_ascii_lowercase()))
        .is_some_and(|label| !label.is_empty() && !label.contains('.'));
    if !covered {
        return unavailable(format!(
            "'{domain}' must be one label below the global domain ({global}) to use its wildcard certificate"
        ));
    }

    if !(cfg.acme.enabled && cfg.acme.wildcard) {
        return unavailable(
            "automatic certificates are off; enable the wildcard certificate in [acme]".to_string(),
        );
    }
    if !wildcard_issued {
        return unavailable(
            "the wildcard certificate has not been issued yet; it is published once it is"
                .to_string(),
        );
    }

    if app_domains
        .iter()
        .any(|app_domain| app_domain.eq_ignore_ascii_case(&domain))
    {
        return unavailable(format!("'{domain}' is already an app's domain"));
    }

    Desired::Published { domain }
}

/// Where Angie reaches the daemon: the bind address, or loopback when the API
/// listens on every interface.
fn upstream_host(cfg: &DekuConfig) -> String {
    let ip = cfg.api_listen_addr().ip();
    match ip {
        IpAddr::V4(v4) if v4.is_unspecified() => "127.0.0.1".to_string(),
        IpAddr::V6(v6) if v6.is_unspecified() => "127.0.0.1".to_string(),
        IpAddr::V6(v6) => format!("[{v6}]"),
        IpAddr::V4(v4) => v4.to_string(),
    }
}

/// Render the vhost for `domain`.
pub fn render(cfg: &DekuConfig, domain: &str) -> Result<String> {
    let mut hbs = Handlebars::new();
    hbs.register_escape_fn(handlebars::no_escape);
    hbs.register_template_string("dashboard", TEMPLATE)
        .context("failed to register the dashboard template")?;
    hbs.render(
        "dashboard",
        &json!({
            "domain": domain,
            "client": crate::acme::file::WILDCARD_CLIENT,
            "upstream_host": upstream_host(cfg),
            "upstream_port": cfg.api_port,
        }),
    )
    .context("rendering the dashboard vhost")
}

/// Write or remove the dashboard vhost to match `cfg`, and reload Angie. A
/// reload Angie rejects puts the previous file back.
pub async fn apply(pool: &sqlx::SqlitePool, cfg: &DekuConfig) -> Result<Desired> {
    let wildcard_issued = crate::acme::file::wildcard_issued(&cfg.acme);
    let app_domains = crate::db::queries::list_all_domain_names(pool).await?;
    let desired = desired(cfg, wildcard_issued, &app_domains);

    let conf_dir = &cfg.angie_conf_dir;
    let previous = super::read_app_config(conf_dir, CONFIG_NAME)?;
    let next = match &desired {
        Desired::Published { domain } => Some(render(cfg, domain)?),
        Desired::Disabled | Desired::Unavailable { .. } => None,
    };

    if previous.as_deref() == next.as_deref().map(str::as_bytes) {
        return Ok(desired);
    }

    match &next {
        Some(contents) => super::write_raw_app_config(conf_dir, CONFIG_NAME, contents.as_bytes())?,
        None => super::remove_app_config(conf_dir, CONFIG_NAME)?,
    }

    if let Err(error) = super::reload().await {
        restore(conf_dir, previous.as_deref())?;
        return Err(
            error.context("Angie rejected the dashboard vhost; the previous one was restored")
        );
    }

    Ok(desired)
}

/// Remove the dashboard vhost without reloading, for a caller that is about to
/// take the wildcard certificate away and reloads afterwards.
pub fn remove(conf_dir: &Path) -> Result<()> {
    super::remove_app_config(conf_dir, CONFIG_NAME)
}

fn restore(conf_dir: &Path, previous: Option<&[u8]>) -> Result<()> {
    match previous {
        Some(contents) => super::write_raw_app_config(conf_dir, CONFIG_NAME, contents),
        None => super::remove_app_config(conf_dir, CONFIG_NAME),
    }
}

#[cfg(test)]
mod tests {
    use super::{desired, render, Desired};
    use crate::config::{AcmeConfig, DekuConfig};

    fn cfg(dashboard_domain: Option<&str>) -> DekuConfig {
        DekuConfig {
            dashboard_domain: dashboard_domain.map(str::to_string),
            global_domain: Some("apps.example.com".to_string()),
            acme: AcmeConfig {
                enabled: true,
                wildcard: true,
                ..AcmeConfig::default()
            },
            ..DekuConfig::default()
        }
    }

    #[test]
    fn nothing_is_published_without_a_domain() {
        assert_eq!(desired(&cfg(None), true, &[]), Desired::Disabled);
    }

    #[test]
    fn a_covered_domain_with_an_issued_wildcard_is_published() {
        assert_eq!(
            desired(&cfg(Some("Deku.apps.example.com")), true, &[]),
            Desired::Published {
                domain: "deku.apps.example.com".to_string()
            }
        );
    }

    #[test]
    fn a_domain_the_wildcard_does_not_cover_is_refused() {
        for domain in [
            "deku.example.com",
            "a.b.apps.example.com",
            "apps.example.com",
        ] {
            assert!(
                matches!(
                    desired(&cfg(Some(domain)), true, &[]),
                    Desired::Unavailable { .. }
                ),
                "{domain}"
            );
        }
    }

    #[test]
    fn it_waits_for_the_certificate_and_never_falls_back_to_http() {
        assert!(matches!(
            desired(&cfg(Some("deku.apps.example.com")), false, &[]),
            Desired::Unavailable { .. }
        ));
        let mut off = cfg(Some("deku.apps.example.com"));
        off.acme.enabled = false;
        assert!(matches!(
            desired(&off, true, &[]),
            Desired::Unavailable { .. }
        ));
    }

    #[test]
    fn an_app_domain_is_not_taken_over() {
        let claimed = vec!["deku.apps.example.com".to_string()];
        assert!(matches!(
            desired(&cfg(Some("deku.apps.example.com")), true, &claimed),
            Desired::Unavailable { .. }
        ));
    }

    #[test]
    fn the_vhost_is_https_only_and_streams_unbuffered() {
        let mut config = cfg(Some("deku.apps.example.com"));
        config.api_port = 2810;
        let rendered = render(&config, "deku.apps.example.com").expect("render");
        assert!(rendered.contains("return 301 https://$host$request_uri;"));
        assert!(rendered.contains("listen 443 ssl;"));
        assert!(rendered.contains("ssl_certificate     $acme_cert_deku_wildcard;"));
        assert!(rendered.contains("proxy_pass         http://127.0.0.1:2810;"));
        assert!(rendered.contains("proxy_buffering    off;"));
        assert!(rendered.contains("proxy_set_header   X-Real-IP $remote_addr;"));
    }
}
