use anyhow::Result;
use tracing::{info, warn};

mod acme;
mod alerts;
mod api;
mod app_name;
mod backup_scheduler;
mod build;
mod build_remote;
mod buildkit;
mod config;
mod console;
mod container;
mod crypto;
mod db;
mod deploy;
mod deploy_lock;
mod events;
mod hooks;
mod logs;
mod metrics;
mod objectstore;
mod proxy;
mod secrets;
mod services;
mod ssh;
mod version;

#[tokio::main]
async fn main() -> Result<()> {
    let cfg = config::load()?;
    config::init_logging(&cfg)?;
    if config::dashboard_assets_available(&cfg) {
        info!(dashboard_dir = %cfg.dashboard_dir.display(), "dashboard assets available");
    } else {
        warn!(
            dashboard_dir = %cfg.dashboard_dir.display(),
            "dashboard assets missing; serving a fallback page until a dashboard bundle is staged"
        );
    }
    info!("dekud starting");

    // Make sure Angie loads the directory app configs are written to. Whether
    // the distribution's configuration includes it is not something Deku
    // controls, and an app whose vhost is never read looks identical to one that
    // is, so this is checked rather than assumed.
    match proxy::ensure_app_config_include(&cfg.angie_conf_dir).await {
        Ok(proxy::IncludeAction::WriteDropIn) => {
            info!("added the Angie include for app configs");
        }
        Ok(proxy::IncludeAction::Unexplained) => {
            warn!(
                conf_dir = %cfg.angie_conf_dir.display(),
                "app configs are present but not loaded by Angie, and the include already exists; run `deku doctor`"
            );
        }
        Ok(_) => {}
        Err(error) => {
            warn!("could not check the Angie app config include: {error}");
        }
    }

    let pool = db::connect(&cfg).await?;
    db::migrate(&pool).await?;

    // Angie requests the wildcard certificate from a file of its own, so the
    // client exists from startup rather than from the next deploy. A failure is
    // reported, not fatal: a host that has not configured a certificate still
    // runs, and a rejected file is rolled back before Angie ever reads it.
    // Every app config answers the reachability check with this identifier,
    // so it is loaded before any config is written.
    if let Err(error) = acme::reachability::init(&cfg.data_dir) {
        warn!("could not load the host identifier for reachability checks: {error}");
    }

    let acme_email = match services::letsencrypt::get_global_email(&cfg).await {
        Ok(email) => email,
        Err(error) => {
            warn!("could not read the certificate account email: {error}");
            None
        }
    };
    match acme::file::apply(&pool, &cfg, acme_email.as_deref()).await {
        Ok(acme::file::Outcome::Unchanged) => {}
        Ok(outcome) => info!(?outcome, "synced the ACME configuration"),
        Err(error) => warn!("could not apply the ACME configuration: {error}"),
    }
    apply_dashboard_vhost(&pool, &cfg).await;

    let docker = container::connect()?;
    info!("connected to docker daemon");

    // Deploys and service creation also ensure the network, so a Docker daemon
    // that is not up yet only delays it rather than stopping the daemon.
    if let Err(error) = container::ensure_deku_network(&docker).await {
        warn!("could not create the shared Deku network: {error}");
    }

    let event_bus = events::EventBus::new(pool.clone());
    let log_bus = logs::LogBus::new(pool.clone());
    let state = api::AppState::new(cfg.clone(), pool, event_bus, log_bus, docker);

    // Before the API accepts a deploy, so only abandoned deployments are touched.
    match deploy::recover_interrupted_deploys(&state.pool, &state.docker).await {
        Ok(apps) => {
            let acme = config::load().map(|fresh| fresh.acme).ok();
            for app in apps {
                // Route to whatever is still live, in case the interrupted
                // deploy had already switched the proxy to its own replicas.
                if let Err(error) = proxy::reconcile_app(
                    &state.pool,
                    &state.config.angie_conf_dir,
                    state.config.global_domain.as_deref(),
                    acme.as_ref(),
                    &app.id,
                    &app.name,
                )
                .await
                {
                    warn!(app = %app.name, "could not restore routing after recovery: {error}");
                }
            }
        }
        Err(error) => warn!("could not recover interrupted deployments: {error}"),
    }

    {
        let state = state.clone();
        tokio::spawn(async move {
            let cfg = state.config.clone();
            match services::database::migrate_to_shared_network(&state.pool, &state.docker, &cfg)
                .await
            {
                Ok(apps) if !apps.is_empty() => warn!(
                    apps = ?apps,
                    "linked service URLs now use the shared network; redeploy these apps to apply them"
                ),
                Ok(_) => {}
                Err(error) => warn!("could not move services onto the shared network: {error}"),
            }
        });
    }

    // The wildcard certificate is issued by Angie in its own time, so a
    // dashboard domain waiting on it is retried until it can be published.
    {
        let state = state.clone();
        tokio::spawn(async move {
            let mut interval = tokio::time::interval(std::time::Duration::from_secs(300));
            interval.tick().await;
            loop {
                interval.tick().await;
                let cfg = api::current_config(&state);
                apply_dashboard_vhost(&state.pool, &cfg).await;
            }
        });
    }

    backup_scheduler::spawn(state.clone());
    acme::watcher::spawn(state.clone());
    alerts::spawn(state.clone());
    logs::spawn_maintenance(state.clone());

    let servers = async {
        tokio::try_join!(api::serve(state.clone()), async {
            if let Err(error) = ssh::serve(state.clone()).await {
                warn!(
                    error = %error,
                    ssh_port = state.config.ssh_port,
                    "SSH server unavailable; API remains online and CLI/API deploys still work"
                );
            }

            Ok::<(), anyhow::Error>(())
        },)
    };

    tokio::select! {
        result = servers => {
            result?;
        }
        () = shutdown_signal() => {
            info!("dekud stopping");
            // Log lines are written in batches; store what is still queued.
            if let Err(error) = state.logs.flush().await {
                warn!("could not store queued log lines before stopping: {error}");
            }
        }
    }

    Ok(())
}

/// Resolve on SIGTERM, which systemd sends to stop the unit, or Ctrl-C.
async fn shutdown_signal() {
    let terminate = async {
        match tokio::signal::unix::signal(tokio::signal::unix::SignalKind::terminate()) {
            Ok(mut signal) => {
                signal.recv().await;
            }
            Err(error) => {
                warn!("could not listen for SIGTERM: {error}");
                std::future::pending::<()>().await;
            }
        }
    };
    tokio::select! {
        () = terminate => {}
        result = tokio::signal::ctrl_c() => {
            if let Err(error) = result {
                warn!("could not listen for Ctrl-C: {error}");
                std::future::pending::<()>().await;
            }
        }
    }
}

/// Publish, update or withdraw the dashboard's Angie vhost, logging the outcome.
async fn apply_dashboard_vhost(pool: &sqlx::SqlitePool, cfg: &config::DekuConfig) {
    match proxy::dashboard::apply(pool, cfg).await {
        Ok(proxy::dashboard::Desired::Published { domain }) => {
            info!(%domain, "dashboard published through Angie");
        }
        Ok(proxy::dashboard::Desired::Unavailable { domain, reason }) => {
            warn!(%domain, "dashboard not published: {reason}");
        }
        Ok(proxy::dashboard::Desired::Disabled) => {}
        Err(error) => warn!("could not apply the dashboard vhost: {error}"),
    }
}
