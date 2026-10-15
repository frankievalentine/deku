//! The certificate watcher.
//!
//! Angie requests and renews certificates on its own schedule and says nothing
//! when it is done, so the daemon looks every minute: it checks that pending
//! domains reach this server, reads what Angie reports for each domain's
//! client, moves domains between states, and rewrites the config of any app
//! whose domains should now be served differently. The same pass notices the
//! wildcard certificate being issued, which before this waited for the next
//! deploy to take effect.

use std::collections::BTreeSet;
use std::path::Path;
use std::time::Duration;

use anyhow::Result;
use chrono::Utc;
use tracing::{info, warn};

use crate::acme::domains::{
    advance, client_name, supports_http_challenge, wants_check, CertState, DomainTls, Observation,
    TlsMode, Transition,
};
use crate::acme::{file, reachability, status};
use crate::api::SharedState;
use crate::config::DekuConfig;
use crate::db::queries;
use crate::hooks::{self, HookEvent, HookEventData};

/// How often the watcher looks.
pub const INTERVAL: Duration = Duration::from_secs(60);

/// Start the watcher.
pub fn spawn(state: SharedState) {
    tokio::spawn(async move {
        let mut ticker = tokio::time::interval(INTERVAL);
        let mut wildcard_was = file::wildcard_issued(&state.config.acme);
        loop {
            ticker.tick().await;
            if let Err(error) = run_once(&state, &mut wildcard_was).await {
                warn!(%error, "the certificate watcher failed");
            }
        }
    });
}

/// One pass. `wildcard_was` carries whether the wildcard was issued on the last
/// pass, so its being issued (or lost) rewrites every app once.
pub async fn run_once(state: &SharedState, wildcard_was: &mut bool) -> Result<()> {
    let cfg = crate::api::current_config(state);
    let wildcard_now = file::wildcard_issued(&cfg.acme);
    let wildcard_changed = wildcard_now != *wildcard_was;
    *wildcard_was = wildcard_now;

    let mut changed_apps = BTreeSet::new();
    if cfg.acme.enabled && cfg.acme.custom_domains {
        changed_apps = advance_domains(state, &cfg, wildcard_now).await?;
    }

    let apps = if wildcard_changed {
        info!(
            issued = wildcard_now,
            "the wildcard certificate changed; rewriting every app"
        );
        queries::list_apps(&state.pool).await?
    } else {
        let mut apps = Vec::new();
        for app_id in &changed_apps {
            apps.push(queries::get_app_by_id(&state.pool, app_id).await?);
        }
        apps
    };

    for app in apps {
        // A deploy in progress renders from the same stored state, so it is
        // left to finish rather than waited on.
        let lock = state.deploy_locks.for_app(&app.id);
        let Ok(_guard) = lock.try_lock() else {
            continue;
        };
        if let Err(error) = crate::proxy::reconcile_app(
            &state.pool,
            &cfg.angie_conf_dir,
            cfg.global_domain.as_deref(),
            Some(&cfg.acme),
            &app.id,
            &app.name,
        )
        .await
        {
            warn!(app = %app.name, %error, "could not apply a certificate change");
        }
    }

    if wildcard_changed {
        if let Err(error) = crate::proxy::dashboard::apply(&state.pool, &cfg).await {
            warn!(%error, "could not apply the dashboard vhost after a wildcard change");
        }
    }
    Ok(())
}

/// Move every `auto` domain one step, and return the apps whose config should
/// be rewritten as a result.
async fn advance_domains(
    state: &SharedState,
    cfg: &DekuConfig,
    wildcard_issued: bool,
) -> Result<BTreeSet<String>> {
    let statuses = match status::fetch(&file::status_socket(&cfg.acme)).await {
        Ok(statuses) => Some(statuses),
        Err(error) => {
            // Angie not answering is not a reason to stop checking reachability.
            warn!(%error, "could not read certificate status from Angie");
            None
        }
    };

    let now = Utc::now();
    let mut changed = BTreeSet::new();
    for domain in queries::list_all_domain_tls(&state.pool).await? {
        if domain.mode != TlsMode::Auto || !supports_http_challenge(&domain.domain) {
            continue;
        }
        let covered = wildcard_issued
            && cfg
                .global_domain
                .as_deref()
                .is_some_and(|global| crate::proxy::covered_by_wildcard(&domain.domain, global));
        if covered {
            continue;
        }

        let client = client_name(&domain.domain);
        let certificate = cfg.acme.client_path.join(&client).join("certificate.pem");
        let issued = file::certificate_issued(&certificate);
        let expires_at = if issued {
            crate::services::letsencrypt::certificate_not_after(&certificate)
                .await
                .and_then(|raw| crate::services::letsencrypt::parse_openssl_timestamp(&raw))
        } else {
            None
        };
        let angie = statuses.as_ref().and_then(|statuses| statuses.get(&client));
        let failure = angie
            .filter(|status| status.failed())
            .and_then(|_| status::last_failure(Path::new(status::ANGIE_ERROR_LOG), &client));
        let reachability = if wants_check(&domain, now) {
            Some(reachability::check(&domain.domain).await)
        } else {
            None
        };

        let seen = Observation {
            angie,
            issued,
            expires_at,
            failure,
            reachability,
        };
        let Some(next) = advance(&domain, &seen, now) else {
            continue;
        };

        queries::update_domain_tls_state(
            &state.pool,
            &domain.domain_id,
            &queries::DomainTlsUpdate {
                state: next.state,
                details: next.details.as_deref(),
                failures: next.failures,
                expires_at: next.expires_at,
                checked_at: seen.reachability.is_some().then_some(now),
                next_retry_at: next.next_retry_at,
                observed_run: next.observed_run.as_deref(),
            },
        )
        .await?;

        if next.state != domain.state {
            changed.insert(domain.app_id.clone());
            announce(state, cfg, &domain, &next).await;
        }
    }
    Ok(changed)
}

/// Tell the dashboard, the event log, and hooks about a domain that became
/// active or stopped being retried.
async fn announce(state: &SharedState, cfg: &DekuConfig, domain: &DomainTls, next: &Transition) {
    let (event, hook) = match next.state {
        CertState::Active => ("certificate.issued", HookEvent::CertificateIssued),
        CertState::Failed => ("certificate.failed", HookEvent::CertificateFailed),
        _ => {
            state.events.emit(
                Some(domain.app_id.clone()),
                "certificate.state",
                Some(serde_json::json!({ "domain": domain.domain, "state": next.state })),
            );
            return;
        }
    };
    let detail = serde_json::json!({
        "domain": domain.domain,
        "state": next.state,
        "details": next.details,
        "expires_at": next.expires_at,
        "next_retry_at": next.next_retry_at,
    });
    info!(domain = %domain.domain, event, "certificate state changed");
    state
        .events
        .emit(Some(domain.app_id.clone()), event, Some(detail.clone()));

    match queries::get_app_by_id(&state.pool, &domain.app_id).await {
        Ok(app) => {
            let data = HookEventData {
                app: &app,
                deployment: None,
                detail,
            };
            if let Err(error) = hooks::fire(cfg, hook, data).await {
                warn!(%error, "a certificate hook failed");
            }
        }
        Err(error) => warn!(%error, "could not load the app for a certificate hook"),
    }
}
