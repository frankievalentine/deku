use anyhow::{Context, Result};
use handlebars::Handlebars;
use serde_json::json;
use std::io::Write;
use std::path::{Path, PathBuf};

use deku_core::types::Upstream;

static HTTP_TEMPLATE: &str = include_str!("templates/http_app.conf.hbs");
static HTTPS_TEMPLATE: &str = include_str!("templates/https_app.conf.hbs");
static UPSTREAM_TEMPLATE: &str = include_str!("templates/upstream.conf.hbs");

fn registry() -> Result<Handlebars<'static>> {
    let mut hbs = Handlebars::new();
    hbs.register_template_string("upstream", UPSTREAM_TEMPLATE)
        .context("failed to register upstream template")?;
    hbs.register_template_string("http_app", HTTP_TEMPLATE)
        .context("failed to register http_app template")?;
    hbs.register_template_string("https_app", HTTPS_TEMPLATE)
        .context("failed to register https_app template")?;
    Ok(hbs)
}

pub fn app_config_path(conf_dir: &Path, app_name: &str) -> std::path::PathBuf {
    conf_dir.join(format!("{app_name}.conf"))
}

pub fn read_app_config(conf_dir: &Path, app_name: &str) -> Result<Option<Vec<u8>>> {
    let config_path = app_config_path(conf_dir, app_name);
    if !config_path.exists() {
        return Ok(None);
    }

    std::fs::read(&config_path)
        .map(Some)
        .with_context(|| format!("reading {}", config_path.display()))
}

pub fn write_raw_app_config(conf_dir: &Path, app_name: &str, contents: &[u8]) -> Result<()> {
    std::fs::create_dir_all(conf_dir)
        .with_context(|| format!("creating angie conf dir {}", conf_dir.display()))?;

    let config_path = app_config_path(conf_dir, app_name);
    let mut temp = tempfile::NamedTempFile::new_in(conf_dir)
        .with_context(|| format!("creating temp config in {}", conf_dir.display()))?;
    temp.write_all(contents)
        .with_context(|| format!("writing temp config for {app_name}"))?;
    temp.flush()
        .with_context(|| format!("flushing temp config for {app_name}"))?;
    temp.as_file()
        .sync_all()
        .with_context(|| format!("syncing temp config for {app_name}"))?;
    temp.persist(&config_path)
        .map_err(|e| anyhow::anyhow!(e.error))
        .with_context(|| format!("persisting {}", config_path.display()))?;

    tracing::info!(app = app_name, path = %config_path.display(), "angie config written");
    Ok(())
}

/// Where a vhost's certificate comes from.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum CertSource {
    /// A certificate for this vhost's own domains, kept on disk by the
    /// per-app certificate flow.
    File { cert: PathBuf, key: PathBuf },
    /// The wildcard certificate Angie maintains for the global domain. It is
    /// referenced by variable rather than by path, because it is Angie's to
    /// request and renew and the vhost is written before it exists.
    Acme { client: String },
}

/// One vhost inside an app's config file.
///
/// `key` is unique within the file: it names the upstream block and the log
/// files. Production uses the app name, so its rendering is unchanged from when
/// an app had a single vhost; each environment uses `<app>-<slug>`.
///
/// Several vhosts may share a key: an environment's domains are split by where
/// their certificates come from, and they all proxy to one upstream, which is
/// rendered once.
#[derive(Debug, Clone)]
pub struct VhostConfig {
    pub key: String,
    pub domains: Vec<String>,
    pub upstreams: Vec<Upstream>,
    /// `None` serves the vhost over HTTP alone.
    pub cert: Option<CertSource>,
    /// The ACME client whose certificate covers these domains. Angie puts every
    /// name of every server block that references a client into its
    /// certificate, so a vhost that references one holds that client's domain
    /// and nothing else.
    pub acme_client: Option<String>,
}

/// An ACME client declared in an app's config file, for one of its domains.
///
/// Declared beside the vhosts that reference it, so removing the app removes
/// the client with it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AcmeClient {
    pub name: String,
    pub directory: String,
    /// Shared by every client, so they register a single ACME account.
    pub account_key: PathBuf,
    /// Off stops Angie requesting a certificate for a domain that keeps
    /// failing, while the vhost that references the client stays valid.
    pub enabled: bool,
}

impl AcmeClient {
    fn render(&self) -> Result<String> {
        let directory = crate::acme::file::directive_value(&self.directory, "the ACME directory")?;
        let key_shown = self.account_key.display().to_string();
        let account_key = crate::acme::file::directive_value(&key_shown, "the ACME account key")?;
        Ok(format!(
            "acme_client {name} {directory} challenge=http account_key={account_key} retry_after_error={retry}{enabled};\n",
            name = self.name,
            retry = crate::acme::domains::ANGIE_RETRY_AFTER_ERROR,
            enabled = if self.enabled { "" } else { " enabled=off" },
        ))
    }
}

/// Directives shared by every vhost of an app.
///
/// Auth, maintenance, and redirects are app-scoped: an environment serves the
/// same policy as production, so only the key, hostnames, upstreams, and TLS
/// setting differ between vhosts.
pub struct AppVhostPolicy<'a> {
    pub auth: Option<&'a crate::db::queries::AppAuthRecord>,
    pub maintenance: bool,
    pub maintenance_message: Option<&'a str>,
    pub redirects: &'a [crate::db::queries::Redirect],
}

/// Render and write every vhost of an app into its single `<app>.conf`.
///
/// All vhosts live in one file on purpose: an environment hostname is derived as
/// `<app>-<slug>`, which can collide with the name of another app, so per-app
/// filenames (rather than per-vhost ones) keep the name space from overlapping.
///
/// Each vhost uses the HTTPS template when `tls` is `true` and HTTP otherwise.
/// When the policy carries auth, the matching directive is rendered into every
/// vhost and (for basic auth) the htpasswd file is written alongside the config.
/// Maintenance mode replaces the app with a 503; redirects run before the proxy
/// location.
pub fn write_app_config(
    conf_dir: &Path,
    app_name: &str,
    clients: &[AcmeClient],
    vhosts: &[VhostConfig],
    policy: &AppVhostPolicy<'_>,
) -> Result<()> {
    let hbs = registry()?;

    let htpasswd_path = conf_dir.join(format!("{app_name}.htpasswd"));
    let (auth_basic, auth_basic_user_file, forward_auth) = match policy.auth {
        Some(record) if record.mode == "basic" => {
            let (username, hash) =
                match (record.username.as_deref(), record.password_hash.as_deref()) {
                    (Some(username), Some(hash)) => (username, hash),
                    _ => anyhow::bail!("basic auth is missing a username or password hash"),
                };
            write_htpasswd(&htpasswd_path, username, hash)?;
            (true, Some(htpasswd_path.display().to_string()), None)
        }
        Some(record) if record.mode == "forward" => {
            let url = record
                .forward_url
                .as_deref()
                .ok_or_else(|| anyhow::anyhow!("forward auth is missing a URL"))?;
            remove_htpasswd(&htpasswd_path)?;
            (false, None, Some(url.to_string()))
        }
        _ => {
            remove_htpasswd(&htpasswd_path)?;
            (false, None, None)
        }
    };

    let redirect_data: Vec<_> = policy
        .redirects
        .iter()
        .map(|redirect| {
            json!({
                "source_path": redirect.source_path,
                "target": redirect.target,
                "code": redirect.code,
            })
        })
        .collect();

    let mut rendered = String::new();
    if !clients.is_empty() {
        rendered
            .push_str("# Certificates for this app's domains, requested and renewed by Angie.\n");
        for client in clients {
            rendered.push_str(&client.render()?);
        }
    }

    let mut upstreams_written = std::collections::HashSet::new();
    for vhost in vhosts {
        let (template, cert_file, cert_key_file) = match &vhost.cert {
            Some(CertSource::File { cert, key }) => (
                "https_app",
                cert.display().to_string(),
                key.display().to_string(),
            ),
            Some(CertSource::Acme { client }) => (
                "https_app",
                format!("$acme_cert_{client}"),
                format!("$acme_cert_key_{client}"),
            ),
            None => ("http_app", String::new(), String::new()),
        };

        let upstream_data: Vec<_> = vhost
            .upstreams
            .iter()
            .map(|u| json!({ "host": u.host, "port": u.port }))
            .collect();

        if upstreams_written.insert(vhost.key.clone()) {
            if !rendered.is_empty() {
                rendered.push('\n');
            }
            let block = hbs
                .render(
                    "upstream",
                    &json!({ "key": vhost.key, "upstreams": upstream_data }),
                )
                .with_context(|| format!("rendering the upstream for {}", vhost.key))?;
            rendered.push_str(&block);
        }

        let data = json!({
            "key": vhost.key,
            "domains": vhost.domains,
            "acme_client": vhost.acme_client,
            "check_path": crate::acme::reachability::CHECK_PATH,
            "host_id": crate::acme::reachability::host_id(),
            "cert_file": cert_file,
            "cert_key_file": cert_key_file,
            "auth_basic": auth_basic,
            "auth_basic_user_file": auth_basic_user_file,
            "forward_auth": forward_auth,
            "maintenance": policy.maintenance,
            "maintenance_message": policy.maintenance_message,
            "redirects": redirect_data,
        });

        let block = hbs
            .render(template, &data)
            .with_context(|| format!("rendering angie config for {}", vhost.key))?;
        if !rendered.is_empty() {
            rendered.push('\n');
        }
        rendered.push_str(&block);
    }

    write_raw_app_config(conf_dir, app_name, rendered.as_bytes())
}

fn write_htpasswd(path: &Path, username: &str, hash: &str) -> Result<()> {
    use std::os::unix::fs::PermissionsExt;

    let mut file = std::fs::File::create(path)
        .with_context(|| format!("creating htpasswd file {}", path.display()))?;
    file.write_all(format!("{username}:{hash}\n").as_bytes())
        .with_context(|| format!("writing htpasswd file {}", path.display()))?;
    file.sync_all()
        .with_context(|| format!("syncing htpasswd file {}", path.display()))?;
    std::fs::set_permissions(path, std::fs::Permissions::from_mode(0o600))
        .with_context(|| format!("setting htpasswd permissions on {}", path.display()))?;
    Ok(())
}

fn remove_htpasswd(path: &Path) -> Result<()> {
    match std::fs::remove_file(path) {
        Ok(()) => Ok(()),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(()),
        Err(error) => {
            Err(error).with_context(|| format!("removing htpasswd file {}", path.display()))
        }
    }
}

/// Remove the Angie vhost config and its credential file for `app_name`.
///
/// The htpasswd file is removed too: leaving it behind would keep a stale
/// credential next to the config, and would survive an app rename.
pub fn remove_app_config(conf_dir: &Path, app_name: &str) -> Result<()> {
    let config_path = app_config_path(conf_dir, app_name);
    if config_path.exists() {
        std::fs::remove_file(&config_path)
            .with_context(|| format!("removing {}", config_path.display()))?;
        tracing::info!(app = app_name, "angie config removed");
    }

    let htpasswd_path = conf_dir.join(format!("{app_name}.htpasswd"));
    if htpasswd_path.exists() {
        std::fs::remove_file(&htpasswd_path)
            .with_context(|| format!("removing {}", htpasswd_path.display()))?;
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::{
        app_config_path, read_app_config, remove_app_config, write_app_config, AcmeClient,
        AppVhostPolicy, CertSource, VhostConfig,
    };
    use deku_core::types::Upstream;

    fn demo_vhost(domains: &[String], upstreams: &[Upstream]) -> VhostConfig {
        VhostConfig {
            key: "demo".to_string(),
            domains: domains.to_vec(),
            upstreams: upstreams.to_vec(),
            cert: None,
            acme_client: None,
        }
    }

    fn policy<'a>(
        redirects: &'a [crate::db::queries::Redirect],
        maintenance: bool,
        maintenance_message: Option<&'a str>,
    ) -> AppVhostPolicy<'a> {
        AppVhostPolicy {
            auth: None,
            maintenance,
            maintenance_message,
            redirects,
        }
    }

    fn upstreams() -> Vec<Upstream> {
        vec![Upstream {
            host: "127.0.0.1".to_string(),
            port: 3000,
        }]
    }

    fn read_rendered(conf_dir: &Path) -> String {
        String::from_utf8(
            read_app_config(conf_dir, "demo")
                .expect("config should read")
                .expect("config should be present"),
        )
        .expect("config should be utf8")
    }

    use std::path::{Path, PathBuf};

    #[test]
    fn removing_an_app_config_also_removes_its_htpasswd() {
        let temp = tempfile::tempdir().expect("tempdir should create");
        let conf_dir = temp.path();
        let htpasswd = conf_dir.join("demo.htpasswd");
        std::fs::write(&htpasswd, "alice:$6$hash").expect("write htpasswd");
        std::fs::write(conf_dir.join("demo.conf"), "server {}").expect("write conf");

        remove_app_config(conf_dir, "demo").expect("remove");

        assert!(!htpasswd.exists(), "htpasswd should be removed");
        assert!(!conf_dir.join("demo.conf").exists());
    }

    #[test]
    fn writes_reads_and_removes_app_config() {
        let temp = tempfile::tempdir().expect("tempdir should create");
        let conf_dir = temp.path();
        let upstreams = upstreams();
        let domains = vec![String::from("example.com")];

        write_app_config(
            conf_dir,
            "demo",
            &[],
            &[demo_vhost(&domains, &upstreams)],
            &policy(&[], false, None),
        )
        .expect("config should write");

        assert!(
            app_config_path(conf_dir, "demo").exists(),
            "config file should exist"
        );
        let rendered = read_rendered(conf_dir);
        assert!(
            rendered.contains("server_name example.com;"),
            "config should include the domain"
        );
        assert!(
            rendered.contains("server 127.0.0.1:3000;"),
            "config should include the upstream"
        );

        remove_app_config(conf_dir, "demo").expect("config should remove");
        assert!(
            !app_config_path(conf_dir, "demo").exists(),
            "config file should be removed"
        );
    }

    #[test]
    fn a_vhost_serves_the_certificate_it_is_given() {
        let temp = tempfile::tempdir().expect("tempdir should create");
        let conf_dir = temp.path();
        let own = VhostConfig {
            key: "demo".to_string(),
            domains: vec![String::from("example.com")],
            upstreams: upstreams(),
            cert: Some(CertSource::File {
                cert: PathBuf::from("/etc/angie/ssl/deku_demo.crt"),
                key: PathBuf::from("/etc/angie/ssl/deku_demo.key"),
            }),
            acme_client: None,
        };
        let derived = VhostConfig {
            key: "demo-staging".to_string(),
            domains: vec![String::from("demo-staging.apps.test")],
            upstreams: upstreams(),
            cert: Some(CertSource::Acme {
                client: "deku_wildcard".to_string(),
            }),
            acme_client: None,
        };

        write_app_config(
            conf_dir,
            "demo",
            &[],
            &[own, derived],
            &policy(&[], false, None),
        )
        .expect("config should write");
        let rendered = read_rendered(conf_dir);

        assert!(
            rendered.contains("ssl_certificate     /etc/angie/ssl/deku_demo.crt;"),
            "an app's own domains are served the certificate on disk: {rendered}"
        );
        // The wildcard is referenced by variable, because it is Angie's to
        // request and renew, and the vhost may be written before it exists.
        assert!(
            rendered.contains("ssl_certificate     $acme_cert_deku_wildcard;"),
            "{rendered}"
        );
        assert!(
            rendered.contains("ssl_certificate_key $acme_cert_key_deku_wildcard;"),
            "{rendered}"
        );
        // A vhost without a certificate is served over HTTP alone.
        assert_eq!(
            rendered.matches("listen 443 ssl;").count(),
            2,
            "only the vhosts that name a certificate listen for TLS"
        );
    }

    #[test]
    fn per_domain_clients_are_declared_and_referenced_with_one_upstream() {
        let temp = tempfile::tempdir().expect("tempdir should create");
        let conf_dir = temp.path();
        let issued = VhostConfig {
            key: "demo".to_string(),
            domains: vec![String::from("shop.example.com")],
            upstreams: upstreams(),
            cert: Some(CertSource::Acme {
                client: "deku_d_aaaa".to_string(),
            }),
            acme_client: Some("deku_d_aaaa".to_string()),
        };
        let issuing = VhostConfig {
            key: "demo".to_string(),
            domains: vec![String::from("www.shop.example.com")],
            upstreams: upstreams(),
            cert: None,
            acme_client: Some("deku_d_bbbb".to_string()),
        };
        let clients = [
            AcmeClient {
                name: "deku_d_aaaa".to_string(),
                directory: "https://acme.test/directory".to_string(),
                account_key: PathBuf::from("/var/lib/angie/acme/deku-account.key"),
                enabled: true,
            },
            AcmeClient {
                name: "deku_d_bbbb".to_string(),
                directory: "https://acme.test/directory".to_string(),
                account_key: PathBuf::from("/var/lib/angie/acme/deku-account.key"),
                enabled: false,
            },
        ];

        write_app_config(
            conf_dir,
            "demo",
            &clients,
            &[issued, issuing],
            &policy(&[], false, None),
        )
        .expect("config should write");
        let rendered = read_rendered(conf_dir);

        assert!(rendered.contains(
            "acme_client deku_d_aaaa https://acme.test/directory challenge=http account_key=/var/lib/angie/acme/deku-account.key retry_after_error=15m;"
        ), "{rendered}");
        assert!(rendered.contains("acme_client deku_d_bbbb https://acme.test/directory challenge=http account_key=/var/lib/angie/acme/deku-account.key retry_after_error=15m enabled=off;"), "{rendered}");
        assert_eq!(
            rendered.matches("upstream deku_demo {").count(),
            1,
            "one upstream per key: {rendered}"
        );
        assert_eq!(
            rendered.matches("acme deku_d_aaaa;").count(),
            2,
            "both server blocks of the HTTPS vhost reference the client"
        );
        assert_eq!(rendered.matches("acme deku_d_bbbb;").count(), 1);
        assert!(rendered.contains("ssl_certificate     $acme_cert_deku_d_aaaa;"));
    }

    #[test]
    fn every_vhost_lands_in_one_file_with_its_own_upstream() {
        let temp = tempfile::tempdir().expect("tempdir should create");
        let conf_dir = temp.path();
        let production = VhostConfig {
            key: "demo".to_string(),
            domains: vec![String::from("example.com")],
            upstreams: vec![Upstream {
                host: "127.0.0.1".to_string(),
                port: 3000,
            }],
            cert: None,
            acme_client: None,
        };
        let staging = VhostConfig {
            key: "demo-staging".to_string(),
            domains: vec![String::from("demo-staging.example.test")],
            upstreams: vec![Upstream {
                host: "127.0.0.1".to_string(),
                port: 3100,
            }],
            cert: None,
            acme_client: None,
        };

        write_app_config(
            conf_dir,
            "demo",
            &[],
            &[production, staging],
            &policy(&[], false, None),
        )
        .expect("config should write");

        let rendered = read_rendered(conf_dir);
        assert!(
            rendered.contains("server_name example.com;"),
            "production vhost missing"
        );
        assert!(
            rendered.contains("server_name demo-staging.example.test;"),
            "environment vhost missing"
        );
        assert!(
            rendered.contains("upstream deku_demo-staging {"),
            "environment upstream must be named after its key"
        );
        assert!(
            rendered.contains("server 127.0.0.1:3100;"),
            "environment upstream port missing"
        );
        // Two vhosts means two upstream blocks, and they must not share a name.
        assert_eq!(
            rendered.matches("upstream deku_").count(),
            2,
            "each vhost needs its own upstream block"
        );
    }

    fn auth_record(mode: &str) -> crate::db::queries::AppAuthRecord {
        crate::db::queries::AppAuthRecord {
            app_id: "app-1".to_string(),
            mode: mode.to_string(),
            username: Some("alice".to_string()),
            password_hash: Some("$6$salt$hash".to_string()),
            forward_url: Some("https://auth.example/verify".to_string()),
            created_at: "now".to_string(),
            updated_at: "now".to_string(),
        }
    }

    fn redirect(source: &str, target: &str, code: i64) -> crate::db::queries::Redirect {
        crate::db::queries::Redirect {
            id: "r1".to_string(),
            app_id: "app-1".to_string(),
            source_path: source.to_string(),
            target: target.to_string(),
            code,
            created_at: chrono::Utc::now(),
        }
    }

    #[test]
    fn maintenance_mode_replaces_the_proxy_location() {
        let temp = tempfile::tempdir().expect("tempdir should create");
        let conf_dir = temp.path();
        let upstreams = upstreams();
        let domains = vec![String::from("example.com")];

        write_app_config(
            conf_dir,
            "demo",
            &[],
            &[demo_vhost(&domains, &upstreams)],
            &policy(&[], true, Some("Back soon")),
        )
        .expect("config should write");

        let rendered = read_rendered(conf_dir);
        assert!(
            rendered.contains("return 503 \"Back soon\";"),
            "expected maintenance 503 with message, got:\n{rendered}"
        );
        assert!(
            !rendered.contains("proxy_pass         http://deku_demo;"),
            "maintenance mode must not proxy"
        );
    }

    #[test]
    fn redirects_render_before_the_proxy_location() {
        let temp = tempfile::tempdir().expect("tempdir should create");
        let conf_dir = temp.path();
        let upstreams = upstreams();
        let domains = vec![String::from("example.com")];
        let redirects = vec![redirect("/old", "https://example.com/new", 301)];

        write_app_config(
            conf_dir,
            "demo",
            &[],
            &[demo_vhost(&domains, &upstreams)],
            &policy(&redirects, false, None),
        )
        .expect("config should write");

        let rendered = read_rendered(conf_dir);
        assert!(
            rendered.contains("location = /old {"),
            "missing redirect location"
        );
        assert!(
            rendered.contains("return 301 https://example.com/new;"),
            "missing redirect return"
        );
    }

    #[test]
    fn writes_basic_auth_directive_and_htpasswd() {
        let temp = tempfile::tempdir().expect("tempdir should create");
        let conf_dir = temp.path();
        let upstreams = upstreams();
        let record = auth_record("basic");
        let domains = vec![String::from("example.com")];

        write_app_config(
            conf_dir,
            "demo",
            &[],
            &[demo_vhost(&domains, &upstreams)],
            &AppVhostPolicy {
                auth: Some(&record),
                ..policy(&[], false, None)
            },
        )
        .expect("config should write");

        let rendered = read_rendered(conf_dir);
        assert!(
            rendered.contains("auth_basic \"Deku\";"),
            "missing auth_basic directive"
        );
        assert!(
            rendered.contains("auth_basic_user_file"),
            "missing auth_basic_user_file directive"
        );

        let htpasswd = conf_dir.join("demo.htpasswd");
        assert!(htpasswd.exists(), "htpasswd file should exist");
        assert_eq!(
            std::fs::read_to_string(&htpasswd).expect("read htpasswd"),
            "alice:$6$salt$hash\n"
        );
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            let mode = std::fs::metadata(&htpasswd)
                .expect("metadata")
                .permissions()
                .mode()
                & 0o777;
            assert_eq!(mode, 0o600, "htpasswd must be 0600");
        }
    }

    #[test]
    fn writes_forward_auth_block_and_removes_htpasswd() {
        let temp = tempfile::tempdir().expect("tempdir should create");
        let conf_dir = temp.path();
        let upstreams = upstreams();
        std::fs::write(conf_dir.join("demo.htpasswd"), "stale\n").expect("seed htpasswd");
        let record = auth_record("forward");
        let domains = vec![String::from("example.com")];

        write_app_config(
            conf_dir,
            "demo",
            &[],
            &[demo_vhost(&domains, &upstreams)],
            &AppVhostPolicy {
                auth: Some(&record),
                ..policy(&[], false, None)
            },
        )
        .expect("config should write");

        let rendered = read_rendered(conf_dir);
        assert!(
            rendered.contains("auth_request /__deku_auth;"),
            "missing auth_request directive"
        );
        assert!(
            rendered.contains("proxy_pass              https://auth.example/verify;"),
            "missing forward-auth proxy_pass"
        );
        assert!(
            !conf_dir.join("demo.htpasswd").exists(),
            "stale htpasswd should be removed for forward auth"
        );
    }

    #[test]
    fn auth_and_policy_apply_to_every_vhost_in_the_file() {
        let temp = tempfile::tempdir().expect("tempdir should create");
        let conf_dir = temp.path();
        let record = auth_record("basic");
        let production = VhostConfig {
            key: "demo".to_string(),
            domains: vec![String::from("example.com")],
            upstreams: upstreams(),
            cert: None,
            acme_client: None,
        };
        let staging = VhostConfig {
            key: "demo-staging".to_string(),
            domains: vec![String::from("demo-staging.example.test")],
            upstreams: upstreams(),
            cert: None,
            acme_client: None,
        };

        write_app_config(
            conf_dir,
            "demo",
            &[],
            &[production, staging],
            &AppVhostPolicy {
                auth: Some(&record),
                ..policy(&[], false, None)
            },
        )
        .expect("config should write");

        let rendered = read_rendered(conf_dir);
        assert_eq!(
            rendered.matches("auth_basic \"Deku\";").count(),
            2,
            "app auth must protect every vhost, not just production"
        );
    }
}
