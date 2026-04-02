use anyhow::{Context, Result};
use handlebars::Handlebars;
use serde_json::json;
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
    std::fs::create_dir_all(conf_dir)
        .with_context(|| format!("creating angie conf dir {}", conf_dir.display()))?;

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

    let config_path = conf_dir.join(format!("{app_name}.conf"));
    std::fs::write(&config_path, rendered)
        .with_context(|| format!("writing {}", config_path.display()))?;

    tracing::info!(app = app_name, path = %config_path.display(), "angie config written");
    Ok(())
}

/// Remove the Angie vhost config for `app_name`.
pub fn remove_app_config(conf_dir: &Path, app_name: &str) -> Result<()> {
    let config_path = conf_dir.join(format!("{app_name}.conf"));
    if config_path.exists() {
        std::fs::remove_file(&config_path)
            .with_context(|| format!("removing {}", config_path.display()))?;
        tracing::info!(app = app_name, "angie config removed");
    }
    Ok(())
}
