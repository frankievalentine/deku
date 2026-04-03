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

/// Render and write an Angie vhost config fragment for `app_name`.
///
/// Uses the HTTPS template when `tls` is `true`, HTTP otherwise.
pub fn write_app_config(
    conf_dir: &Path,
    app_name: &str,
    domains: &[String],
    upstreams: &[Upstream],
    tls: bool,
) -> Result<()> {
    let hbs = registry()?;
    let template = if tls { "https_app" } else { "http_app" };

    let upstream_data: Vec<_> = upstreams
        .iter()
        .map(|u| json!({ "host": u.host, "port": u.port }))
        .collect();

    let data = json!({
        "app_name": app_name,
        "domains": domains,
        "upstreams": upstream_data,
    });

    let rendered = hbs
        .render(template, &data)
        .with_context(|| format!("rendering angie config for {app_name}"))?;
    write_raw_app_config(conf_dir, app_name, rendered.as_bytes())
}

/// Remove the Angie vhost config for `app_name`.
pub fn remove_app_config(conf_dir: &Path, app_name: &str) -> Result<()> {
    let config_path = app_config_path(conf_dir, app_name);
    if config_path.exists() {
        std::fs::remove_file(&config_path)
            .with_context(|| format!("removing {}", config_path.display()))?;
        tracing::info!(app = app_name, "angie config removed");
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::{app_config_path, read_app_config, remove_app_config, write_app_config};
    use deku_core::types::Upstream;

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
            "demo",
            &[String::from("example.com")],
            &upstreams,
            false,
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
}
