use anyhow::Result;
use sqlx::SqlitePool;

use crate::config::DekuConfig;
use crate::db::queries;
use deku_core::types::Upstream;

pub async fn enable(pool: &SqlitePool, cfg: &DekuConfig, app_name: &str) -> Result<()> {
    let app = queries::get_app(pool, app_name).await?;
    queries::set_app_tls(pool, &app.id, true).await?;
    let domains = queries::list_domain_names(pool, &app.id).await?;
    let ports = queries::list_port_mappings(pool, &app.id).await?;
    if !domains.is_empty() && !ports.is_empty() {
        let upstreams: Vec<Upstream> = ports
            .iter()
            .map(|p| Upstream {
                host: "127.0.0.1".into(),
                port: p.host_port as u16,
            })
            .collect();
        crate::proxy::write_app_config(&cfg.angie_conf_dir, app_name, &domains, &upstreams, true)?;
        crate::proxy::reload().await?;
    }
    Ok(())
}

pub async fn disable(pool: &SqlitePool, cfg: &DekuConfig, app_name: &str) -> Result<()> {
    let app = queries::get_app(pool, app_name).await?;
    queries::set_app_tls(pool, &app.id, false).await?;
    let domains = queries::list_domain_names(pool, &app.id).await?;
    let ports = queries::list_port_mappings(pool, &app.id).await?;
    if !domains.is_empty() && !ports.is_empty() {
        let upstreams: Vec<Upstream> = ports
            .iter()
            .map(|p| Upstream {
                host: "127.0.0.1".into(),
                port: p.host_port as u16,
            })
            .collect();
        crate::proxy::write_app_config(
            &cfg.angie_conf_dir,
            app_name,
            &domains,
            &upstreams,
            false,
        )?;
        crate::proxy::reload().await?;
    }
    Ok(())
}

pub async fn set_global_email(cfg: &DekuConfig, email: &str) -> Result<()> {
    let path = cfg.data_dir.join("letsencrypt-email");
    std::fs::write(path, email)?;
    Ok(())
}
