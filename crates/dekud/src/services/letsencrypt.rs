use anyhow::Result;
use chrono::{DateTime, Utc};
use serde::Serialize;
use sqlx::SqlitePool;

use crate::config::DekuConfig;
use crate::db::queries;
use deku_core::types::Upstream;

pub async fn enable(pool: &SqlitePool, cfg: &DekuConfig, app_name: &str) -> Result<()> {
    let app = queries::get_app(pool, app_name).await?;
    let domains = queries::list_domain_names(pool, &app.id).await?;
    let ports = queries::list_port_mappings(pool, &app.id).await?;
    ensure_tls_enable_ready(app_name, &domains, &ports)?;
    let upstreams: Vec<Upstream> = ports
        .iter()
        .map(|p| Upstream {
            host: "127.0.0.1".into(),
            port: p.host_port as u16,
        })
        .collect();
    crate::proxy::apply_app_config(
        &cfg.angie_conf_dir,
        app_name,
        Some(crate::proxy::DesiredAppConfig {
            domains: &domains,
            upstreams: &upstreams,
            tls: true,
        }),
    )
    .await?;
    queries::set_app_tls(pool, &app.id, true).await?;
    Ok(())
}

pub async fn disable(pool: &SqlitePool, cfg: &DekuConfig, app_name: &str) -> Result<()> {
    let app = queries::get_app(pool, app_name).await?;
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
        crate::proxy::apply_app_config(
            &cfg.angie_conf_dir,
            app_name,
            Some(crate::proxy::DesiredAppConfig {
                domains: &domains,
                upstreams: &upstreams,
                tls: false,
            }),
        )
        .await?;
    } else {
        crate::proxy::apply_app_config(&cfg.angie_conf_dir, app_name, None).await?;
    }
    queries::set_app_tls(pool, &app.id, false).await?;
    Ok(())
}

pub async fn set_global_email(cfg: &DekuConfig, email: &str) -> Result<()> {
    let path = cfg.data_dir.join("letsencrypt-email");
    std::fs::write(path, email)?;
    Ok(())
}

pub async fn get_global_email(cfg: &DekuConfig) -> Result<Option<String>> {
    let path = cfg.data_dir.join("letsencrypt-email");
    if !path.exists() {
        return Ok(None);
    }

    let email = std::fs::read_to_string(path)?;
    let trimmed = email.trim();
    if trimmed.is_empty() {
        Ok(None)
    } else {
        Ok(Some(trimmed.to_string()))
    }
}

#[derive(Debug, Serialize)]
pub struct FileStatus {
    pub path: String,
    pub exists: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub modified_at: Option<DateTime<Utc>>,
}

#[derive(Debug, Serialize)]
pub struct CertificateStatus {
    pub enabled: bool,
    pub domains: Vec<String>,
    pub certificate: FileStatus,
    pub private_key: FileStatus,
    pub ready: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub not_before: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub not_after: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub subject: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub inspection_error: Option<String>,
}

pub async fn status(pool: &SqlitePool, app_name: &str) -> Result<CertificateStatus> {
    let app = queries::get_app(pool, app_name).await?;
    let domains = queries::list_domain_names(pool, &app.id).await?;
    let cert_path = crate::proxy::cert_path(app_name);
    let key_path = crate::proxy::key_path(app_name);
    let certificate = file_status(&cert_path)?;
    let private_key = file_status(&key_path)?;
    let mut status = CertificateStatus {
        enabled: app.tls_enabled,
        domains,
        ready: certificate.exists && private_key.exists,
        certificate,
        private_key,
        not_before: None,
        not_after: None,
        subject: None,
        inspection_error: None,
    };

    if status.certificate.exists {
        match inspect_certificate(&cert_path).await {
            Ok(info) => {
                status.not_before = info.not_before;
                status.not_after = info.not_after;
                status.subject = info.subject;
            }
            Err(error) => status.inspection_error = Some(error.to_string()),
        }
    }

    Ok(status)
}

fn ensure_tls_enable_ready(
    app_name: &str,
    domains: &[String],
    ports: &[deku_core::types::PortMapping],
) -> Result<()> {
    if domains.is_empty() {
        anyhow::bail!("cannot enable TLS for '{app_name}' without at least one domain");
    }

    if ports.is_empty() {
        anyhow::bail!(
            "cannot enable TLS for '{app_name}' without at least one upstream port mapping"
        );
    }

    let cert_path = crate::proxy::cert_path(app_name);
    let key_path = crate::proxy::key_path(app_name);
    if !cert_path.exists() || !key_path.exists() {
        anyhow::bail!(
            "cannot enable TLS for '{app_name}' because certificate files are missing (cert: {}, key: {})",
            cert_path.display(),
            key_path.display()
        );
    }

    Ok(())
}

fn file_status(path: &std::path::Path) -> Result<FileStatus> {
    let metadata = std::fs::metadata(path).ok();
    let modified_at = metadata
        .and_then(|meta| meta.modified().ok())
        .map(DateTime::<Utc>::from);

    Ok(FileStatus {
        path: path.display().to_string(),
        exists: path.exists(),
        modified_at,
    })
}

struct ParsedCertificateInfo {
    not_before: Option<String>,
    not_after: Option<String>,
    subject: Option<String>,
}

async fn inspect_certificate(path: &std::path::Path) -> Result<ParsedCertificateInfo> {
    let output = tokio::process::Command::new("openssl")
        .args([
            "x509",
            "-in",
            path.to_str().unwrap_or_default(),
            "-noout",
            "-startdate",
            "-enddate",
            "-subject",
        ])
        .output()
        .await?;

    if !output.status.success() {
        anyhow::bail!(
            "openssl x509 failed: {}",
            String::from_utf8_lossy(&output.stderr).trim()
        );
    }

    let stdout = String::from_utf8_lossy(&output.stdout);
    let mut info = ParsedCertificateInfo {
        not_before: None,
        not_after: None,
        subject: None,
    };

    for line in stdout.lines() {
        if let Some(value) = line.strip_prefix("notBefore=") {
            info.not_before = Some(value.trim().to_string());
        } else if let Some(value) = line.strip_prefix("notAfter=") {
            info.not_after = Some(value.trim().to_string());
        } else if let Some(value) = line.strip_prefix("subject=") {
            info.subject = Some(value.trim().to_string());
        }
    }

    Ok(info)
}

#[cfg(test)]
mod tests {
    use super::ensure_tls_enable_ready;
    use deku_core::types::PortMapping;

    fn sample_port() -> PortMapping {
        PortMapping {
            id: "port-1".to_string(),
            app_id: "app-1".to_string(),
            host_port: 8080,
            container_port: 3000,
            protocol: "tcp".to_string(),
        }
    }

    #[test]
    fn tls_enable_requires_domain() {
        let err = ensure_tls_enable_ready("demo", &[], &[sample_port()])
            .expect_err("missing domain should fail");
        assert!(
            err.to_string().contains("without at least one domain"),
            "error should mention missing domains"
        );
    }

    #[test]
    fn tls_enable_requires_upstream() {
        let err = ensure_tls_enable_ready("demo", &[String::from("example.com")], &[])
            .expect_err("missing upstream should fail");
        assert!(
            err.to_string()
                .contains("without at least one upstream port mapping"),
            "error should mention missing upstreams"
        );
    }

    #[test]
    fn tls_enable_requires_certificate_files() {
        let err = ensure_tls_enable_ready("demo", &[String::from("example.com")], &[sample_port()])
            .expect_err("missing certificate files should fail");
        assert!(
            err.to_string().contains("certificate files are missing"),
            "error should mention missing certificate files"
        );
    }
}
