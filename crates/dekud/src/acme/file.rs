//! The Angie configuration for the wildcard certificate.
//!
//! It lives in a file of its own rather than in an app's config because it is
//! about the daemon and not about any app: it names the ACME client, the names
//! the certificate covers, and the hook Angie calls to have the DNS challenge
//! records written. A deploy rewrites app configs and never touches this file,
//! so requesting a certificate does not depend on a deploy happening.

use anyhow::{Context, Result};
use std::io::Write;
use std::path::{Path, PathBuf};

use crate::config::{AcmeConfig, DekuConfig};

/// The ACME client Deku configures. Angie refers to it by this name, and a
/// vhost serves the certificate through `$acme_cert_deku_wildcard`.
pub const WILDCARD_CLIENT: &str = "deku_wildcard";

/// The file this module owns, in the same directory as the app configs.
///
/// The name contains `+`, which an app name cannot (they allow only letters,
/// digits, `-`, `_`, and `.`), so it can never be confused with the config of an
/// app called `deku+acme`, and it is reached by the same include as the app
/// configs rather than needing an include of its own.
pub const CONFIG_FILE_NAME: &str = "deku+acme.conf";

/// The path of the ACME configuration file.
pub fn config_path(conf_dir: &Path) -> PathBuf {
    conf_dir.join(CONFIG_FILE_NAME)
}

/// Whether a file in the config directory is an app's config.
///
/// The ACME file shares the directory but belongs to no app, so a listing of the
/// app configs must not expect one for it.
pub fn is_app_config_file(name: &str) -> bool {
    name.ends_with(".conf") && name != CONFIG_FILE_NAME
}

/// The file Angie writes the wildcard certificate to.
///
/// Angie keeps each client's files in a subdirectory named after the client, so
/// the certificate appearing there is what tells Deku that the hostnames it
/// covers can be served over HTTPS.
pub fn wildcard_certificate(acme: &AcmeConfig) -> PathBuf {
    acme.client_path
        .join(WILDCARD_CLIENT)
        .join("certificate.pem")
}

/// What syncing the file did.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Outcome {
    /// No certificate is wanted; a file left by an earlier configuration is gone.
    Removed,
    /// The file was written, because it did not match the configuration.
    Written,
    /// The file already matched, so Angie was left alone.
    Unchanged,
}

/// Everything the file is rendered from.
pub struct FileConfig<'a> {
    pub global_domain: &'a str,
    pub acme: &'a AcmeConfig,
    /// The account email registered with the certificate authority, when one is
    /// set. Deku keeps it with the Let's Encrypt settings rather than asking
    /// twice.
    pub email: Option<&'a str>,
    /// The daemon's own socket, which the hook location proxies to.
    pub deku_socket: &'a Path,
}

/// Write or remove the ACME configuration so it matches the daemon config.
///
/// Returns what changed, and Angie is reloaded only when something did. That
/// restraint is the point: a reload makes Angie re-request every certificate
/// that is not currently valid, ignoring its own retry delay, so reloading an
/// unchanged file while issuance is failing is how a host ends up hammering the
/// certificate authority.
pub async fn sync(cfg: &DekuConfig, email: Option<&str>) -> Result<Outcome> {
    let desired = match wanted(cfg, email)? {
        Some(config) => Some(render(&config)?),
        None => None,
    };
    sync_file(&cfg.angie_conf_dir, desired.as_deref()).await
}

/// Write, remove, or leave the file alone to match what is wanted.
///
/// Split from the configuration so the rule can be checked on its own: `None`
/// wants no file, and `Some` wants exactly that content.
async fn sync_file(conf_dir: &Path, desired: Option<&str>) -> Result<Outcome> {
    let path = config_path(conf_dir);
    let previous = std::fs::read(&path).ok();

    match &desired {
        Some(contents) => {
            if previous.as_deref() == Some(contents.as_bytes()) {
                return Ok(Outcome::Unchanged);
            }
            write_atomic(&path, contents.as_bytes())?;
        }
        None => {
            if previous.is_none() {
                return Ok(Outcome::Unchanged);
            }
            std::fs::remove_file(&path).with_context(|| format!("removing {}", path.display()))?;
        }
    }

    if let Err(error) = crate::proxy::reload().await {
        // The file is read by Angie for every app, so leaving a rejected one in
        // place would take the whole proxy configuration down with it.
        restore(&path, previous.as_deref())?;
        return Err(error).context("the ACME configuration was rejected and rolled back");
    }

    let outcome = if desired.is_some() {
        Outcome::Written
    } else {
        Outcome::Removed
    };
    tracing::info!(path = %path.display(), ?outcome, "synced the ACME configuration");
    Ok(outcome)
}

/// The configuration to render from, or `None` when no certificate is wanted.
///
/// A certificate is wanted only for a wildcard client with a global domain to
/// name; anything else would leave Angie collecting names for a certificate
/// nobody serves.
fn wanted<'a>(cfg: &'a DekuConfig, email: Option<&'a str>) -> Result<Option<FileConfig<'a>>> {
    if !cfg.acme.enabled || !cfg.acme.wildcard {
        return Ok(None);
    }

    let Some(global_domain) = cfg
        .global_domain
        .as_deref()
        .filter(|domain| !domain.trim().is_empty())
    else {
        return Ok(None);
    };

    // Nothing half-configured reaches Angie: a client without a token would
    // retry on every reload and never obtain anything.
    cfg.acme
        .validate(Some(global_domain))
        .context("the ACME configuration cannot be used")?;

    Ok(Some(FileConfig {
        global_domain,
        acme: &cfg.acme,
        email,
        deku_socket: &cfg.socket_path,
    }))
}

/// Render the file.
pub fn render(config: &FileConfig<'_>) -> Result<String> {
    let client = WILDCARD_CLIENT;
    let directory = directive_value(&config.acme.directory, "the ACME directory")?;
    let domain = directive_value(config.global_domain, "the global domain")?;

    // A path is checked before it is used, so it is owned rather than borrowed
    // from a `display()` temporary that would not outlive the check.
    let client_path_shown = config.acme.client_path.display().to_string();
    let client_path = directive_value(&client_path_shown, "the ACME client path")?;

    // Beside the certificates the client already writes there, so the directory
    // exists and Angie's master process can create the socket in it.
    let collector_shown = config
        .acme
        .client_path
        .join("collector.sock")
        .display()
        .to_string();
    let collector = directive_value(&collector_shown, "the collector socket")?;

    let socket_shown = config.deku_socket.display().to_string();
    let socket = directive_value(&socket_shown, "the daemon socket")?;

    let email = match config
        .email
        .map(str::trim)
        .filter(|value| !value.is_empty())
    {
        Some(email) => format!(" email={}", directive_value(email, "the account email")?),
        None => String::new(),
    };

    Ok(format!(
        "\
# Written by Deku for the wildcard certificate. Do not edit.
#
# Angie requests and renews a certificate covering {domain} and *.{domain}, and
# keeps it under {client_path}. The DNS-01 challenge is answered by Deku: Angie
# calls the hook location below, the daemon writes the TXT record through the
# provider's API, and the 200 it returns is what Angie waits for.

acme_client_path {client_path};

acme_client {client} {directory}
    challenge=dns{email};

# The names the certificate covers. Nothing is served from this block: it is
# bound to a socket nothing connects to, and it exists so the certificate is
# requested before any app has a hostname that needs it.
server {{
    listen unix:{collector};
    server_name {domain} *.{domain};
    acme {client};

    location @acme_dns_hook {{
        acme_hook {client} uri=/internal/acme/dns-hook;

        proxy_pass http://unix:{socket};
        proxy_set_header X-Deku-Acme-Hook      $acme_hook_name;
        proxy_set_header X-Deku-Acme-Domain    $acme_hook_domain;
        proxy_set_header X-Deku-Acme-Keyauth   $acme_hook_keyauth;
        proxy_set_header X-Deku-Acme-Challenge $acme_hook_challenge;
        proxy_set_header X-Deku-Acme-Client    $acme_hook_client;
    }}
}}
"
    ))
}

/// Reject a value that would end a directive early or add one.
///
/// The values come from the daemon config and the dashboard, and they are
/// written verbatim into a file the proxy loads, so a stray newline or semicolon
/// would become configuration.
fn directive_value<'a>(value: &'a str, what: &str) -> Result<&'a str> {
    if value
        .chars()
        .any(|character| character.is_whitespace() || character.is_control() || character == ';')
    {
        anyhow::bail!("{what} '{value}' cannot be written into the Angie configuration");
    }
    Ok(value)
}

fn write_atomic(path: &Path, contents: &[u8]) -> Result<()> {
    let dir = path
        .parent()
        .context("the ACME configuration path has no directory")?;
    std::fs::create_dir_all(dir).with_context(|| format!("creating {}", dir.display()))?;

    let mut temp = tempfile::NamedTempFile::new_in(dir)
        .with_context(|| format!("creating a temporary file in {}", dir.display()))?;
    temp.write_all(contents)
        .context("writing the ACME configuration")?;
    temp.flush().context("flushing the ACME configuration")?;
    temp.as_file()
        .sync_all()
        .context("syncing the ACME configuration")?;
    temp.persist(path)
        .map_err(|error| anyhow::anyhow!(error.error))
        .with_context(|| format!("persisting {}", path.display()))?;
    Ok(())
}

/// Put the file back the way it was, so a rejected configuration does not stay
/// where Angie reads it.
fn restore(path: &Path, previous: Option<&[u8]>) -> Result<()> {
    match previous {
        Some(contents) => write_atomic(path, contents),
        None => std::fs::remove_file(path).with_context(|| format!("removing {}", path.display())),
    }
}

#[cfg(test)]
mod tests {
    use super::{
        config_path, is_app_config_file, render, FileConfig, CONFIG_FILE_NAME, WILDCARD_CLIENT,
    };
    use crate::config::AcmeConfig;
    use std::path::{Path, PathBuf};

    fn acme() -> AcmeConfig {
        AcmeConfig {
            enabled: true,
            wildcard: true,
            // What makes the configuration usable at all: a token for the
            // provider that answers the challenge.
            api_token_file: Some(PathBuf::from("/var/lib/deku/acme-api-token")),
            ..AcmeConfig::default()
        }
    }

    fn rendered(email: Option<&str>) -> String {
        let acme = acme();
        render(&FileConfig {
            global_domain: "apps.test",
            acme: &acme,
            email,
            deku_socket: Path::new("/run/deku/deku.sock"),
        })
        .expect("the configuration should render")
    }

    #[test]
    fn the_file_asks_for_the_wildcard_and_the_names_it_covers() {
        let contents = rendered(Some("ops@example.test"));

        assert!(
            contents.contains(&format!(
                "acme_client {WILDCARD_CLIENT} https://acme-v02.api.letsencrypt.org/directory"
            )),
            "{contents}"
        );
        // A wildcard is only obtainable through DNS validation.
        assert!(contents.contains("challenge=dns"), "{contents}");
        assert!(contents.contains("email=ops@example.test"), "{contents}");
        assert!(
            contents.contains("server_name apps.test *.apps.test;"),
            "{contents}"
        );
        // The certificate lives where the client path says, which is also where
        // its readiness is read from.
        assert!(
            contents.contains("acme_client_path /var/lib/angie/acme;"),
            "{contents}"
        );
    }

    #[test]
    fn the_hook_calls_back_into_the_daemon() {
        let contents = rendered(None);

        assert!(
            contents.contains(&format!(
                "acme_hook {WILDCARD_CLIENT} uri=/internal/acme/dns-hook;"
            )),
            "{contents}"
        );
        assert!(
            contents.contains("proxy_pass http://unix:/run/deku/deku.sock;"),
            "{contents}"
        );
        // The daemon needs the action, the name, and the value to write, and it
        // reads them from these headers.
        for header in [
            "$acme_hook_name",
            "$acme_hook_domain",
            "$acme_hook_keyauth",
            "$acme_hook_challenge",
        ] {
            assert!(
                contents.contains(header),
                "{header} is missing from {contents}"
            );
        }
    }

    #[test]
    fn an_account_email_is_optional() {
        assert!(!rendered(None).contains(" email="), "{}", rendered(None));
    }

    #[test]
    fn a_value_that_would_end_a_directive_is_rejected() {
        let acme = acme();
        let error = render(&FileConfig {
            // A newline here would add a directive of the caller's choosing to a
            // file the proxy loads.
            global_domain: "apps.test;\nserver { listen 80; }",
            acme: &acme,
            email: None,
            deku_socket: Path::new("/run/deku/deku.sock"),
        })
        .expect_err("a domain with a newline should be rejected");

        assert!(error.to_string().contains("global domain"), "{error}");
    }

    #[tokio::test]
    async fn syncing_writes_only_what_changed() {
        // A write here reloads Angie, which signals the pid in the standard pid
        // file. On a host that is actually running Angie, that file exists and
        // the reload would try to validate these throwaway contents, so the case
        // is left to the live checks rather than made flaky here.
        if std::path::Path::new("/run/angie/angie.pid").exists() {
            return;
        }

        let temp = tempfile::tempdir().expect("tempdir");
        let conf_dir = temp.path();
        let path = conf_dir.join(CONFIG_FILE_NAME);

        // Nothing wanted and nothing there: no file, and nothing to reload for.
        assert_eq!(
            super::sync_file(conf_dir, None).await.expect("sync"),
            super::Outcome::Unchanged
        );
        assert!(!path.exists());

        assert_eq!(
            super::sync_file(conf_dir, Some("first"))
                .await
                .expect("sync"),
            super::Outcome::Written
        );
        assert_eq!(std::fs::read_to_string(&path).expect("read"), "first");

        // The same content is what keeps a start from re-requesting a
        // certificate: a write here would reload Angie, and a reload re-requests
        // every certificate that is not currently valid.
        assert_eq!(
            super::sync_file(conf_dir, Some("first"))
                .await
                .expect("sync"),
            super::Outcome::Unchanged
        );
        assert_eq!(std::fs::read_to_string(&path).expect("read"), "first");

        assert_eq!(
            super::sync_file(conf_dir, Some("second"))
                .await
                .expect("sync"),
            super::Outcome::Written
        );
        assert_eq!(std::fs::read_to_string(&path).expect("read"), "second");

        // Disabling removes it; removing again does nothing.
        assert_eq!(
            super::sync_file(conf_dir, None).await.expect("sync"),
            super::Outcome::Removed
        );
        assert!(!path.exists());
        assert_eq!(
            super::sync_file(conf_dir, None).await.expect("sync"),
            super::Outcome::Unchanged
        );
    }

    #[test]
    fn a_rejected_file_is_put_back_the_way_it_was() {
        let temp = tempfile::tempdir().expect("tempdir");
        let path = temp.path().join("deku+acme.conf");

        // There was a file before: its contents come back.
        std::fs::write(&path, "rejected").expect("write");
        super::restore(&path, Some(b"previous")).expect("restore");
        assert_eq!(std::fs::read_to_string(&path).expect("read"), "previous");

        // There was not: nothing is left behind for Angie to read.
        super::restore(&path, None).expect("restore");
        assert!(!path.exists());
    }

    #[test]
    fn the_acme_file_is_not_an_app_config() {
        let dir = Path::new("/etc/angie/conf.d/deku");
        assert_eq!(config_path(dir), dir.join(CONFIG_FILE_NAME));
        assert!(!is_app_config_file(CONFIG_FILE_NAME));
        assert!(is_app_config_file("demo.conf"));
        assert!(!is_app_config_file("notes.txt"));
    }
}
