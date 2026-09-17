use anyhow::{Context, Result};
use handlebars::Handlebars;
use serde_json::json;
use std::io::Write;
use std::path::Path;

use deku_core::types::Upstream;

static HTTP_TEMPLATE: &str = include_str!("templates/http_app.conf.hbs");
static HTTPS_TEMPLATE: &str = include_str!("templates/https_app.conf.hbs");

fn registry() -> Result<Handlebars<'static>> {
    let mut hbs = Handlebars::new();
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

/// Everything needed to render one app's Angie vhost fragment.
pub struct VhostConfig<'a> {
    pub app_name: &'a str,
    pub domains: &'a [String],
    pub upstreams: &'a [Upstream],
    pub tls: bool,
    pub auth: Option<&'a crate::db::queries::AppAuthRecord>,
    pub maintenance: bool,
    pub maintenance_message: Option<&'a str>,
    pub redirects: &'a [crate::db::queries::Redirect],
}

/// Render and write an Angie vhost config fragment.
///
/// Uses the HTTPS template when `tls` is `true`, HTTP otherwise. When `auth`
/// is set, the matching auth directive and (for basic auth) the htpasswd file
/// are written alongside the vhost config. Maintenance mode replaces the app
/// with a 503; redirects run before the proxy location.
pub fn write_app_config(conf_dir: &Path, config: VhostConfig<'_>) -> Result<()> {
    let VhostConfig {
        app_name,
        domains,
        upstreams,
        tls,
        auth,
        maintenance,
        maintenance_message,
        redirects,
    } = config;

    let hbs = registry()?;
    let template = if tls { "https_app" } else { "http_app" };

    let upstream_data: Vec<_> = upstreams
        .iter()
        .map(|u| json!({ "host": u.host, "port": u.port }))
        .collect();

    let redirect_data: Vec<_> = redirects
        .iter()
        .map(|redirect| {
            json!({
                "source_path": redirect.source_path,
                "target": redirect.target,
                "code": redirect.code,
            })
        })
        .collect();

    let htpasswd_path = conf_dir.join(format!("{app_name}.htpasswd"));
    let (auth_basic, auth_basic_user_file, forward_auth) = match auth {
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

    let data = json!({
        "app_name": app_name,
        "domains": domains,
        "upstreams": upstream_data,
        "auth_basic": auth_basic,
        "auth_basic_user_file": auth_basic_user_file,
        "forward_auth": forward_auth,
        "maintenance": maintenance,
        "maintenance_message": maintenance_message,
        "redirects": redirect_data,
    });

    let rendered = hbs
        .render(template, &data)
        .with_context(|| format!("rendering angie config for {app_name}"))?;
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
        app_config_path, read_app_config, remove_app_config, write_app_config, VhostConfig,
    };
    use deku_core::types::Upstream;

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
        let upstreams = vec![Upstream {
            host: "127.0.0.1".to_string(),
            port: 3000,
        }];

        write_app_config(
            conf_dir,
            VhostConfig {
                app_name: "demo",
                domains: &[String::from("example.com")],
                upstreams: &upstreams,
                tls: false,
                auth: None,
                maintenance: false,
                maintenance_message: None,
                redirects: &[],
            },
        )
        .expect("config should write");

        let config_path = app_config_path(conf_dir, "demo");
        assert!(config_path.exists(), "config file should exist");

        let contents = read_app_config(conf_dir, "demo")
            .expect("config should read")
            .expect("config should be present");
        let rendered = String::from_utf8(contents).expect("config should be utf8");
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

    fn base_config<'a>(
        domains: &'a [String],
        upstreams: &'a [Upstream],
        redirects: &'a [crate::db::queries::Redirect],
        maintenance: bool,
        maintenance_message: Option<&'a str>,
    ) -> VhostConfig<'a> {
        VhostConfig {
            app_name: "demo",
            domains,
            upstreams,
            tls: false,
            auth: None,
            maintenance,
            maintenance_message,
            redirects,
        }
    }

    #[test]
    fn maintenance_mode_replaces_the_proxy_location() {
        let temp = tempfile::tempdir().expect("tempdir should create");
        let conf_dir = temp.path();
        let upstreams = vec![Upstream {
            host: "127.0.0.1".to_string(),
            port: 3000,
        }];
        let domains = vec![String::from("example.com")];

        write_app_config(
            conf_dir,
            base_config(&domains, &upstreams, &[], true, Some("Back soon")),
        )
        .expect("config should write");

        let rendered = String::from_utf8(
            read_app_config(conf_dir, "demo")
                .expect("config should read")
                .expect("config should exist"),
        )
        .expect("utf8");
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
        let upstreams = vec![Upstream {
            host: "127.0.0.1".to_string(),
            port: 3000,
        }];
        let domains = vec![String::from("example.com")];
        let redirects = vec![redirect("/old", "https://example.com/new", 301)];

        write_app_config(
            conf_dir,
            base_config(&domains, &upstreams, &redirects, false, None),
        )
        .expect("config should write");

        let rendered = String::from_utf8(
            read_app_config(conf_dir, "demo")
                .expect("config should read")
                .expect("config should exist"),
        )
        .expect("utf8");
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
        let upstreams = vec![Upstream {
            host: "127.0.0.1".to_string(),
            port: 3000,
        }];
        let record = auth_record("basic");

        write_app_config(
            conf_dir,
            VhostConfig {
                app_name: "demo",
                domains: &[String::from("example.com")],
                upstreams: &upstreams,
                tls: false,
                auth: Some(&record),
                maintenance: false,
                maintenance_message: None,
                redirects: &[],
            },
        )
        .expect("config should write");

        let rendered = String::from_utf8(
            read_app_config(conf_dir, "demo")
                .expect("config should read")
                .expect("config should exist"),
        )
        .expect("utf8");
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
        let upstreams = vec![Upstream {
            host: "127.0.0.1".to_string(),
            port: 3000,
        }];
        std::fs::write(conf_dir.join("demo.htpasswd"), "stale\n").expect("seed htpasswd");
        let record = auth_record("forward");

        write_app_config(
            conf_dir,
            VhostConfig {
                app_name: "demo",
                domains: &[String::from("example.com")],
                upstreams: &upstreams,
                tls: false,
                auth: Some(&record),
                maintenance: false,
                maintenance_message: None,
                redirects: &[],
            },
        )
        .expect("config should write");

        let rendered = String::from_utf8(
            read_app_config(conf_dir, "demo")
                .expect("config should read")
                .expect("config should exist"),
        )
        .expect("utf8");
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
}
