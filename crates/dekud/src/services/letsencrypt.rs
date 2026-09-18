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
    // Derive upstreams from the running web containers so toggling TLS keeps
    // every replica in the pool, matching the deploy and reconcile paths.
    let environment = queries::ensure_production_environment(pool, &app.id).await?;
    let upstream_ports = queries::list_web_upstream_ports(pool, &app.id, &environment.id).await?;
    ensure_tls_enable_ready(app_name, &domains, &upstream_ports)?;
    let upstreams: Vec<Upstream> = upstream_ports
        .iter()
        .map(|port| Upstream {
            host: "127.0.0.1".into(),
            port: *port,
        })
        .collect();
    let extras = crate::proxy::load_extras(pool, &app.id).await?;
    crate::proxy::apply_app_config(
        &cfg.angie_conf_dir,
        app_name,
        Some(crate::proxy::DesiredAppConfig {
            domains: &domains,
            upstreams: &upstreams,
            tls: true,
            auth: extras.auth.as_ref(),
            maintenance: extras.maintenance,
            maintenance_message: extras.maintenance_message.as_deref(),
            redirects: &extras.redirects,
        }),
    )
    .await?;
    queries::set_app_tls(pool, &app.id, true).await?;
    Ok(())
}

pub async fn disable(pool: &SqlitePool, cfg: &DekuConfig, app_name: &str) -> Result<()> {
    let app = queries::get_app(pool, app_name).await?;
    let domains = queries::list_domain_names(pool, &app.id).await?;
    let environment = queries::ensure_production_environment(pool, &app.id).await?;
    let upstream_ports = queries::list_web_upstream_ports(pool, &app.id, &environment.id).await?;
    if !domains.is_empty() && !upstream_ports.is_empty() {
        let upstreams: Vec<Upstream> = upstream_ports
            .iter()
            .map(|port| Upstream {
                host: "127.0.0.1".into(),
                port: *port,
            })
            .collect();
        let extras = crate::proxy::load_extras(pool, &app.id).await?;
        crate::proxy::apply_app_config(
            &cfg.angie_conf_dir,
            app_name,
            Some(crate::proxy::DesiredAppConfig {
                domains: &domains,
                upstreams: &upstreams,
                tls: false,
                auth: extras.auth.as_ref(),
                maintenance: extras.maintenance,
                maintenance_message: extras.maintenance_message.as_deref(),
                redirects: &extras.redirects,
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

/// How close a certificate is to expiry, or why it cannot be read.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum CertLifecycle {
    /// Valid with more than [`CERT_EXPIRY_WARNING_DAYS`] left.
    Ok,
    /// Valid but inside the renewal window.
    Expiring,
    /// Past its `notAfter` time.
    Expired,
    /// Certificate or key file is absent.
    Missing,
    /// Present but could not be inspected or parsed.
    Unknown,
}

/// Warn once a certificate is this close to expiry. Renewal is an operator
/// concern, so the window is deliberately generous.
pub const CERT_EXPIRY_WARNING_DAYS: i64 = 14;

/// Classify a certificate from its `notAfter` value.
///
/// Returns `(expiry, days_remaining, lifecycle)`. Pure, so the boundary cases
/// are testable without a certificate on disk or an `openssl` process.
pub fn classify_expiry(
    not_after: Option<&str>,
    now: DateTime<Utc>,
) -> (Option<DateTime<Utc>>, Option<i64>, CertLifecycle) {
    let Some(expiry) = not_after.and_then(parse_openssl_timestamp) else {
        return (None, None, CertLifecycle::Unknown);
    };

    let remaining = expiry - now;
    let lifecycle = if remaining <= chrono::Duration::zero() {
        CertLifecycle::Expired
    } else if remaining <= chrono::Duration::days(CERT_EXPIRY_WARNING_DAYS) {
        CertLifecycle::Expiring
    } else {
        CertLifecycle::Ok
    };

    (Some(expiry), Some(remaining.num_days()), lifecycle)
}

/// Parse the `notAfter=Sep 17 12:00:00 2026 GMT` format openssl prints.
fn parse_openssl_timestamp(raw: &str) -> Option<DateTime<Utc>> {
    let trimmed = raw.trim();
    let without_zone = trimmed
        .strip_suffix(" GMT")
        .or_else(|| trimmed.strip_suffix(" UTC"))
        .unwrap_or(trimmed);
    chrono::NaiveDateTime::parse_from_str(without_zone, "%b %e %H:%M:%S %Y")
        .ok()
        .map(|naive| naive.and_utc())
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
    /// `notAfter` as RFC 3339 UTC, for machine consumers.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub expires_at: Option<String>,
    /// Whole days until expiry; negative once expired.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub days_remaining: Option<i64>,
    pub lifecycle: CertLifecycle,
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
        expires_at: None,
        days_remaining: None,
        lifecycle: CertLifecycle::Unknown,
        inspection_error: None,
    };

    if status.certificate.exists {
        match inspect_certificate(&cert_path).await {
            Ok(info) => {
                let (expiry, days_remaining, lifecycle) =
                    classify_expiry(info.not_after.as_deref(), Utc::now());
                status.not_before = info.not_before;
                status.not_after = info.not_after;
                status.subject = info.subject;
                status.expires_at = expiry.map(|expiry| expiry.to_rfc3339());
                status.days_remaining = days_remaining;
                status.lifecycle = lifecycle;
            }
            Err(error) => status.inspection_error = Some(error.to_string()),
        }
    } else {
        status.lifecycle = CertLifecycle::Missing;
    }

    Ok(status)
}

fn ensure_tls_enable_ready(
    app_name: &str,
    domains: &[String],
    upstream_ports: &[u16],
) -> Result<()> {
    if domains.is_empty() {
        anyhow::bail!("cannot enable TLS for '{app_name}' without at least one domain");
    }

    if upstream_ports.is_empty() {
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
    use super::{
        classify_expiry, ensure_tls_enable_ready, CertLifecycle, CERT_EXPIRY_WARNING_DAYS,
    };
    use chrono::{Duration, TimeZone, Utc};

    fn at(year: i32, month: u32, day: u32) -> chrono::DateTime<Utc> {
        Utc.with_ymd_and_hms(year, month, day, 0, 0, 0).unwrap()
    }

    #[test]
    fn tls_enable_requires_domain() {
        let err =
            ensure_tls_enable_ready("demo", &[], &[8080]).expect_err("missing domain should fail");
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
        let err = ensure_tls_enable_ready("demo", &[String::from("example.com")], &[8080])
            .expect_err("missing certificate files should fail");
        assert!(
            err.to_string().contains("certificate files are missing"),
            "error should mention missing certificate files"
        );
    }

    #[test]
    fn parses_the_openssl_not_after_format() {
        let (expiry, days, lifecycle) =
            classify_expiry(Some("Sep 17 12:00:00 2026 GMT"), at(2026, 9, 1));
        assert_eq!(expiry, Some(at(2026, 9, 17) + Duration::hours(12)));
        assert_eq!(days, Some(16));
        assert_eq!(lifecycle, CertLifecycle::Ok);
    }

    #[test]
    fn space_padded_single_digit_day_parses() {
        // openssl prints "Sep  7 ..." for single-digit days.
        let (expiry, _, _) = classify_expiry(Some("Sep  7 00:00:00 2026 GMT"), at(2026, 9, 1));
        assert_eq!(expiry, Some(at(2026, 9, 7)));
    }

    #[test]
    fn expires_inside_the_warning_window_is_expiring() {
        let now = at(2026, 9, 1);
        let (_, days, lifecycle) = classify_expiry(Some("Sep 15 00:00:00 2026 GMT"), now);
        assert_eq!(days, Some(14));
        assert_eq!(lifecycle, CertLifecycle::Expiring);

        let (_, _, just_outside) = classify_expiry(Some("Sep 16 00:00:01 2026 GMT"), now);
        assert_eq!(just_outside, CertLifecycle::Ok);
    }

    #[test]
    fn past_not_after_is_expired_with_negative_days() {
        let (_, days, lifecycle) =
            classify_expiry(Some("Aug 25 00:00:00 2026 GMT"), at(2026, 9, 1));
        assert_eq!(days, Some(-7));
        assert_eq!(lifecycle, CertLifecycle::Expired);
    }

    #[test]
    fn expiry_exactly_now_is_expired() {
        let now = at(2026, 9, 1);
        let (_, days, lifecycle) = classify_expiry(Some("Sep 1 00:00:00 2026 GMT"), now);
        assert_eq!(days, Some(0));
        assert_eq!(lifecycle, CertLifecycle::Expired);
    }

    #[test]
    fn missing_or_unparseable_expiry_is_unknown() {
        assert_eq!(
            classify_expiry(None, at(2026, 9, 1)).2,
            CertLifecycle::Unknown
        );
        assert_eq!(
            classify_expiry(Some("not a date"), at(2026, 9, 1)).2,
            CertLifecycle::Unknown
        );
    }

    #[test]
    fn warning_window_is_fourteen_days() {
        assert_eq!(CERT_EXPIRY_WARNING_DAYS, 14);
    }
}
