use std::{
    collections::{BTreeMap, BTreeSet},
    env, fs,
    path::{Path, PathBuf},
    process::Command,
};

use anyhow::{anyhow, bail, Context, Result};
use async_trait::async_trait;
use bollard::{
    query_parameters::{
        ListContainersOptionsBuilder, RemoveContainerOptionsBuilder, RemoveImageOptionsBuilder,
        RemoveVolumeOptionsBuilder, StopContainerOptionsBuilder,
    },
    Docker,
};
use clap::Args;
use sqlx::{
    sqlite::{SqliteConnectOptions, SqlitePoolOptions},
    ConnectOptions, Row,
};

use crate::local_config::{config_dir, config_path, load_optional, LocalDekuConfig};

const DEFAULT_INSTALL_DIR: &str = "/usr/local/bin";
const DEFAULT_SYSTEMD_UNIT_PATH: &str = "/etc/systemd/system/deku.service";
const DEFAULT_ANGIE_BASE_CONF: &str = "/etc/angie/conf.d/deku-default.conf";
const DEFAULT_ANGIE_SSL_DIR: &str = "/etc/angie/ssl";
const DEFAULT_ANGIE_CONF_DIR: &str = "/etc/angie/conf.d/deku";
const DEFAULT_ANGIE_APT_SOURCE: &str = "/etc/apt/sources.list.d/angie.list";
const DEFAULT_ANGIE_KEYRING: &str = "/usr/share/keyrings/angie-signing.gpg";

const ENV_INSTALL_DIR: &str = "DEKU_UNINSTALL_INSTALL_DIR";
const ENV_SYSTEMD_UNIT_PATH: &str = "DEKU_UNINSTALL_SYSTEMD_UNIT_PATH";
const ENV_ANGIE_BASE_CONF: &str = "DEKU_UNINSTALL_ANGIE_BASE_CONF";
const ENV_ANGIE_SSL_DIR: &str = "DEKU_UNINSTALL_ANGIE_SSL_DIR";
const ENV_ANGIE_APT_SOURCE: &str = "DEKU_UNINSTALL_ANGIE_APT_SOURCE";
const ENV_ANGIE_KEYRING: &str = "DEKU_UNINSTALL_ANGIE_KEYRING";

const MANAGED_PREFIXES: &[&str] = &[
    "deku.",
    "deku-postgres-",
    "deku-redis-",
    "deku-mysql-",
    "deku-helper-",
];

#[derive(Debug, Clone, Args)]
pub struct UninstallArgs {
    #[arg(
        long,
        conflicts_with = "full_remove",
        help = "Remove host install artifacts but preserve persisted data"
    )]
    keep_data: bool,
    #[arg(
        long,
        conflicts_with = "keep_data",
        help = "Remove host install artifacts and Deku-owned persisted state"
    )]
    full_remove: bool,
    #[arg(long, help = "Skip destructive confirmation prompts")]
    yes: bool,
    #[arg(long, help = "Print the uninstall plan without making changes")]
    dry_run: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum UninstallMode {
    KeepData,
    FullRemove,
}

impl UninstallMode {
    fn label(self) -> &'static str {
        match self {
            Self::KeepData => "keep persisted data",
            Self::FullRemove => "full uninstall",
        }
    }
}

#[derive(Debug, Clone)]
struct Layout {
    config_dir: PathBuf,
    config_path: PathBuf,
    data_dir: PathBuf,
    dashboard_dir: PathBuf,
    socket_path: PathBuf,
    angie_conf_dir: PathBuf,
    install_dir: PathBuf,
    deku_bin: PathBuf,
    dekud_bin: PathBuf,
    systemd_unit: PathBuf,
    angie_base_conf: PathBuf,
    angie_ssl_dir: PathBuf,
    angie_apt_source: PathBuf,
    angie_keyring: PathBuf,
}

#[derive(Debug, Clone)]
struct HostArtifacts {
    deku_bin_exists: bool,
    dekud_bin_exists: bool,
    systemd_unit_exists: bool,
    angie_base_conf_exists: bool,
}

impl HostArtifacts {
    fn packaged_install_present(&self) -> bool {
        self.systemd_unit_exists
            || self.angie_base_conf_exists
            || self.deku_bin_exists
            || self.dekud_bin_exists
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct ManagedContainer {
    id: String,
    name: String,
}

#[derive(Debug, Clone, Default)]
struct DatabaseState {
    app_names: Vec<String>,
    app_container_ids: Vec<String>,
    service_container_ids: Vec<String>,
    service_volumes: Vec<String>,
    network_names: Vec<String>,
}

#[derive(Debug, Clone)]
struct UninstallPlan {
    mode: UninstallMode,
    layout: Layout,
    host_artifacts: HostArtifacts,
    containers: Vec<ManagedContainer>,
    image_tags: Vec<String>,
    network_names: Vec<String>,
    service_volumes: Vec<String>,
    tls_files: Vec<PathBuf>,
    full_remove_paths: Vec<PathBuf>,
    warnings: Vec<String>,
    docker_available: bool,
}

#[derive(Debug, Default)]
struct ExecutionSummary {
    completed: Vec<String>,
    failed: Vec<String>,
    skipped: Vec<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
enum Section {
    ServiceManager,
    DockerRuntime,
    AngieConfig,
    InstallArtifacts,
    LocalState,
}

impl Section {
    fn title(self) -> &'static str {
        match self {
            Self::ServiceManager => "Service manager",
            Self::DockerRuntime => "Docker runtime",
            Self::AngieConfig => "Angie config",
            Self::InstallArtifacts => "Install artifacts",
            Self::LocalState => "Local state",
        }
    }
}

#[async_trait]
trait DockerRuntime {
    async fn discover_managed_containers(&self) -> Result<Vec<ManagedContainer>>;
    async fn stop_container(&self, id: &str) -> Result<()>;
    async fn remove_container(&self, id: &str) -> Result<()>;
    async fn remove_image(&self, image: &str) -> Result<()>;
    async fn remove_network(&self, name: &str) -> Result<()>;
    async fn remove_volume(&self, name: &str) -> Result<()>;
}

trait SystemManager {
    fn disable_and_stop(&mut self, service: &str) -> Result<()>;
    fn daemon_reload(&mut self) -> Result<()>;
    fn is_active(&mut self, service: &str) -> Result<bool>;
    fn reload(&mut self, service: &str) -> Result<()>;
    fn restart(&mut self, service: &str) -> Result<()>;
    fn purge_package(&mut self, package: &str) -> Result<()>;
    fn apt_update(&mut self) -> Result<()>;
}

struct RealDockerRuntime {
    docker: Docker,
}

struct RealSystemManager;

pub async fn run(args: UninstallArgs) -> Result<()> {
    ensure_linux()?;
    ensure_root()?;

    let mode = resolve_mode(&args)?;
    let mut system = RealSystemManager;

    let docker_runtime = match RealDockerRuntime::connect() {
        Ok(runtime) => Some(runtime),
        Err(error) => {
            let mut plan = discover_plan(mode, None).await?;
            plan.warnings.push(format!(
                "docker discovery unavailable: {error}. Docker cleanup will be skipped."
            ));
            return finalize_uninstall(args, mode, plan, &mut system, None).await;
        }
    };

    let docker_ref = docker_runtime
        .as_ref()
        .map(|runtime| runtime as &dyn DockerRuntime);
    let plan = discover_plan(mode, docker_ref).await?;
    finalize_uninstall(args, mode, plan, &mut system, docker_ref).await
}

async fn finalize_uninstall(
    args: UninstallArgs,
    mode: UninstallMode,
    plan: UninstallPlan,
    system: &mut dyn SystemManager,
    docker_runtime: Option<&dyn DockerRuntime>,
) -> Result<()> {
    print_plan(&plan);

    if args.dry_run {
        println!();
        println!("Dry run only. No changes were made.");
        return Ok(());
    }

    if mode == UninstallMode::FullRemove && !args.yes {
        let confirmed = cliclack::confirm(
            "Proceed with full uninstall? This removes Deku-owned persisted state.",
        )
        .initial_value(false)
        .interact()?;
        if !confirmed {
            println!("Aborted.");
            return Ok(());
        }
    }

    let summary = execute_plan(&plan, system, docker_runtime).await;
    print_execution_summary(&summary);

    if summary.failed.is_empty() {
        Ok(())
    } else {
        bail!(
            "uninstall completed with {} failed actions",
            summary.failed.len()
        );
    }
}

fn ensure_linux() -> Result<()> {
    if cfg!(target_os = "linux") {
        Ok(())
    } else {
        bail!("`deku uninstall` currently supports packaged Linux installs only");
    }
}

fn ensure_root() -> Result<()> {
    let output = Command::new("id")
        .arg("-u")
        .output()
        .context("failed to determine current uid")?;

    if !output.status.success() {
        bail!("failed to determine current uid");
    }

    let uid = String::from_utf8_lossy(&output.stdout).trim().to_string();
    if uid == "0" {
        Ok(())
    } else {
        bail!("run `deku uninstall` as root");
    }
}

fn resolve_mode(args: &UninstallArgs) -> Result<UninstallMode> {
    if args.keep_data {
        return Ok(UninstallMode::KeepData);
    }
    if args.full_remove {
        return Ok(UninstallMode::FullRemove);
    }

    let mut prompt = cliclack::select("Choose uninstall mode")
        .item(
            UninstallMode::KeepData,
            "Keep persisted data",
            "Remove Deku install artifacts and runtime, preserve local data and Docker volumes",
        )
        .item(
            UninstallMode::FullRemove,
            "Full uninstall",
            "Remove Deku install artifacts, runtime, local state, and Deku-owned service volumes",
        )
        .initial_value(UninstallMode::KeepData);

    prompt.interact().map_err(Into::into)
}

async fn discover_plan(
    mode: UninstallMode,
    docker_runtime: Option<&dyn DockerRuntime>,
) -> Result<UninstallPlan> {
    let layout = discover_layout()?;
    let host_artifacts = discover_host_artifacts(&layout);
    if !host_artifacts.packaged_install_present() {
        bail!(
            "this host does not look like a packaged Deku install. `deku uninstall` supports the packaged Linux install path only."
        );
    }

    let mut warnings = Vec::new();
    let db_state = match read_database_state(&layout).await {
        Ok(state) => state,
        Err(error) => {
            warnings.push(format!(
                "local database discovery failed: {error}. Some Docker resources may not be discoverable."
            ));
            DatabaseState::default()
        }
    };

    let mut containers_by_id: BTreeMap<String, ManagedContainer> = BTreeMap::new();
    for id in db_state
        .app_container_ids
        .iter()
        .chain(db_state.service_container_ids.iter())
    {
        containers_by_id.insert(
            id.clone(),
            ManagedContainer {
                id: id.clone(),
                name: id.clone(),
            },
        );
    }

    let docker_available = docker_runtime.is_some();
    if let Some(runtime) = docker_runtime {
        match runtime.discover_managed_containers().await {
            Ok(discovered) => {
                for container in discovered {
                    containers_by_id.insert(container.id.clone(), container);
                }
            }
            Err(error) => warnings.push(format!(
                "docker container discovery failed: {error}. Docker cleanup may be incomplete."
            )),
        }
    }

    let mut image_tags = db_state
        .app_names
        .iter()
        .map(|name| format!("deku/{name}:latest"))
        .collect::<Vec<_>>();
    image_tags.sort();
    image_tags.dedup();

    let mut tls_files = Vec::new();
    for app_name in &db_state.app_names {
        tls_files.push(layout.angie_ssl_dir.join(format!("deku_{app_name}.crt")));
        tls_files.push(layout.angie_ssl_dir.join(format!("deku_{app_name}.key")));
    }

    let full_remove_paths = if mode == UninstallMode::FullRemove {
        dedupe_nested_paths(vec![layout.data_dir.clone(), layout.config_dir.clone()])
    } else {
        Vec::new()
    };

    Ok(UninstallPlan {
        mode,
        layout,
        host_artifacts,
        containers: containers_by_id.into_values().collect(),
        image_tags,
        network_names: db_state.network_names,
        service_volumes: db_state.service_volumes,
        tls_files,
        full_remove_paths,
        warnings,
        docker_available,
    })
}

fn discover_layout() -> Result<Layout> {
    let config = load_optional()?;
    let config_dir = config_dir();
    let config_path = config_path();

    let data_dir = config
        .as_ref()
        .map(LocalDekuConfig::effective_data_dir)
        .unwrap_or_else(|| config_dir.clone());
    let dashboard_dir = config
        .as_ref()
        .map(LocalDekuConfig::dashboard_dir_path)
        .unwrap_or_else(|| data_dir.join("dashboard"));
    let socket_path = config
        .as_ref()
        .map(LocalDekuConfig::effective_socket_path)
        .unwrap_or_else(|| data_dir.join("deku.sock"));
    let angie_conf_dir = config
        .as_ref()
        .and_then(|cfg| cfg.angie_conf_dir.clone())
        .unwrap_or_else(|| PathBuf::from(DEFAULT_ANGIE_CONF_DIR));

    let install_dir = env_path(ENV_INSTALL_DIR, DEFAULT_INSTALL_DIR);
    let systemd_unit = env_path(ENV_SYSTEMD_UNIT_PATH, DEFAULT_SYSTEMD_UNIT_PATH);
    let angie_base_conf = env_path(ENV_ANGIE_BASE_CONF, DEFAULT_ANGIE_BASE_CONF);
    let angie_ssl_dir = env_path(ENV_ANGIE_SSL_DIR, DEFAULT_ANGIE_SSL_DIR);
    let angie_apt_source = env_path(ENV_ANGIE_APT_SOURCE, DEFAULT_ANGIE_APT_SOURCE);
    let angie_keyring = env_path(ENV_ANGIE_KEYRING, DEFAULT_ANGIE_KEYRING);

    Ok(Layout {
        config_dir,
        config_path,
        data_dir,
        dashboard_dir,
        socket_path,
        angie_conf_dir,
        deku_bin: install_dir.join("deku"),
        dekud_bin: install_dir.join("dekud"),
        install_dir,
        systemd_unit,
        angie_base_conf,
        angie_ssl_dir,
        angie_apt_source,
        angie_keyring,
    })
}

fn discover_host_artifacts(layout: &Layout) -> HostArtifacts {
    HostArtifacts {
        deku_bin_exists: layout.deku_bin.exists(),
        dekud_bin_exists: layout.dekud_bin.exists(),
        systemd_unit_exists: layout.systemd_unit.exists(),
        angie_base_conf_exists: layout.angie_base_conf.exists(),
    }
}

async fn read_database_state(layout: &Layout) -> Result<DatabaseState> {
    let db_path = layout.data_dir.join("deku.db");
    if !db_path.exists() {
        return Ok(DatabaseState::default());
    }

    let options = SqliteConnectOptions::new()
        .filename(&db_path)
        .read_only(true)
        .disable_statement_logging();
    let pool = SqlitePoolOptions::new()
        .max_connections(1)
        .connect_with(options)
        .await
        .with_context(|| format!("failed to open {}", db_path.display()))?;

    let app_names = sqlx::query("SELECT name FROM apps ORDER BY name")
        .fetch_all(&pool)
        .await?
        .into_iter()
        .map(|row| row.get::<String, _>("name"))
        .collect::<Vec<_>>();

    let app_container_ids = sqlx::query("SELECT id FROM containers ORDER BY created_at DESC")
        .fetch_all(&pool)
        .await?
        .into_iter()
        .map(|row| row.get::<String, _>("id"))
        .collect::<Vec<_>>();

    let service_rows = sqlx::query(
        "SELECT container_id, config FROM services WHERE plugin IN ('postgres', 'redis', 'mysql')",
    )
    .fetch_all(&pool)
    .await?;

    let mut service_container_ids = Vec::new();
    let mut service_volumes = Vec::new();
    for row in service_rows {
        if let Some(container_id) = row.get::<Option<String>, _>("container_id") {
            service_container_ids.push(container_id);
        }

        let config = row.get::<String, _>("config");
        if let Ok(value) = serde_json::from_str::<serde_json::Value>(&config) {
            if let Some(volume) = value.get("volume").and_then(serde_json::Value::as_str) {
                service_volumes.push(volume.to_string());
            }
        }
    }

    let network_names = sqlx::query("SELECT name FROM networks ORDER BY name")
        .fetch_all(&pool)
        .await?
        .into_iter()
        .map(|row| row.get::<String, _>("name"))
        .collect::<Vec<_>>();

    pool.close().await;

    service_container_ids.sort();
    service_container_ids.dedup();
    service_volumes.sort();
    service_volumes.dedup();

    Ok(DatabaseState {
        app_names,
        app_container_ids,
        service_container_ids,
        service_volumes,
        network_names,
    })
}

async fn execute_plan(
    plan: &UninstallPlan,
    system: &mut dyn SystemManager,
    docker_runtime: Option<&dyn DockerRuntime>,
) -> ExecutionSummary {
    let mut summary = ExecutionSummary::default();

    if plan.host_artifacts.systemd_unit_exists {
        record_result(
            &mut summary,
            "disable and stop deku service",
            system.disable_and_stop("deku"),
        );
    }

    if let Some(runtime) = docker_runtime {
        for container in &plan.containers {
            let stop_result = runtime.stop_container(&container.id).await;
            if let Err(error) = stop_result {
                summary.failed.push(format!(
                    "stop container {} ({}) failed: {error}",
                    container.name, container.id
                ));
            } else {
                summary.completed.push(format!(
                    "stopped container {} ({})",
                    container.name, container.id
                ));
            }

            let remove_result = runtime.remove_container(&container.id).await;
            if let Err(error) = remove_result {
                summary.failed.push(format!(
                    "remove container {} ({}) failed: {error}",
                    container.name, container.id
                ));
            } else {
                summary.completed.push(format!(
                    "removed container {} ({})",
                    container.name, container.id
                ));
            }
        }

        for image_tag in &plan.image_tags {
            record_async_result(
                &mut summary,
                format!("remove image {image_tag}"),
                runtime.remove_image(image_tag).await,
            );
        }

        for network_name in &plan.network_names {
            record_async_result(
                &mut summary,
                format!("remove network {network_name}"),
                runtime.remove_network(network_name).await,
            );
        }

        if plan.mode == UninstallMode::FullRemove {
            for volume in &plan.service_volumes {
                record_async_result(
                    &mut summary,
                    format!("remove volume {volume}"),
                    runtime.remove_volume(volume).await,
                );
            }
        }
    } else {
        if !plan.containers.is_empty()
            || !plan.image_tags.is_empty()
            || !plan.network_names.is_empty()
            || (plan.mode == UninstallMode::FullRemove && !plan.service_volumes.is_empty())
        {
            summary.skipped.push(
                "docker cleanup skipped because docker discovery was unavailable".to_string(),
            );
        }
    }

    for tls_file in &plan.tls_files {
        record_result(
            &mut summary,
            format!("remove TLS file {}", tls_file.display()),
            remove_path_if_exists(tls_file),
        );
    }

    record_result(
        &mut summary,
        format!(
            "remove Angie base config {}",
            plan.layout.angie_base_conf.display()
        ),
        remove_path_if_exists(&plan.layout.angie_base_conf),
    );
    record_result(
        &mut summary,
        format!(
            "remove Angie app config directory {}",
            plan.layout.angie_conf_dir.display()
        ),
        remove_path_if_exists(&plan.layout.angie_conf_dir),
    );
    record_result(
        &mut summary,
        format!(
            "remove Angie apt source {}",
            plan.layout.angie_apt_source.display()
        ),
        remove_path_if_exists(&plan.layout.angie_apt_source),
    );
    record_result(
        &mut summary,
        format!(
            "remove Angie keyring {}",
            plan.layout.angie_keyring.display()
        ),
        remove_path_if_exists(&plan.layout.angie_keyring),
    );

    if plan.host_artifacts.systemd_unit_exists {
        record_result(
            &mut summary,
            format!("remove systemd unit {}", plan.layout.systemd_unit.display()),
            remove_path_if_exists(&plan.layout.systemd_unit),
        );
        record_result(
            &mut summary,
            "reload systemd manager",
            system.daemon_reload(),
        );
    }

    if plan.host_artifacts.deku_bin_exists {
        record_result(
            &mut summary,
            format!("remove binary {}", plan.layout.deku_bin.display()),
            remove_path_if_exists(&plan.layout.deku_bin),
        );
    }
    if plan.host_artifacts.dekud_bin_exists {
        record_result(
            &mut summary,
            format!("remove binary {}", plan.layout.dekud_bin.display()),
            remove_path_if_exists(&plan.layout.dekud_bin),
        );
    }

    if plan.mode == UninstallMode::FullRemove {
        for path in &plan.full_remove_paths {
            record_result(
                &mut summary,
                format!("remove local state {}", path.display()),
                remove_path_if_exists(path),
            );
        }
    }

    if plan.mode == UninstallMode::FullRemove {
        match system.is_active("angie") {
            Ok(true) => record_result(
                &mut summary,
                "disable and stop angie service",
                system.disable_and_stop("angie"),
            ),
            Ok(false) => summary
                .skipped
                .push("skipped disable/stop for angie because angie is not active".to_string()),
            Err(error) => summary.skipped.push(format!(
                "skipped disable/stop for angie because service state could not be read: {error}"
            )),
        }
        record_result(
            &mut summary,
            "purge apt package angie",
            system.purge_package("angie"),
        );
        record_result(
            &mut summary,
            "refresh apt package lists",
            system.apt_update(),
        );
    } else {
        match system.is_active("angie") {
            Ok(true) => {
                let reload = system.reload("angie");
                if reload.is_err() {
                    record_result(&mut summary, "restart angie", system.restart("angie"));
                } else {
                    record_result(&mut summary, "reload angie", reload);
                }
            }
            Ok(false) => summary
                .skipped
                .push("skipped angie reload because angie is not active".to_string()),
            Err(error) => summary.skipped.push(format!(
                "skipped angie reload because service state could not be read: {error}"
            )),
        }
    }

    summary
}

fn record_result(summary: &mut ExecutionSummary, label: impl Into<String>, result: Result<()>) {
    let label = label.into();
    match result {
        Ok(()) => summary.completed.push(label),
        Err(error) => summary.failed.push(format!("{label} failed: {error}")),
    }
}

fn record_async_result(
    summary: &mut ExecutionSummary,
    label: impl Into<String>,
    result: Result<()>,
) {
    record_result(summary, label, result);
}

fn remove_path_if_exists(path: &Path) -> Result<()> {
    match fs::symlink_metadata(path) {
        Ok(metadata) => {
            if metadata.is_dir() {
                fs::remove_dir_all(path)
                    .with_context(|| format!("failed to remove directory {}", path.display()))?;
            } else {
                fs::remove_file(path)
                    .with_context(|| format!("failed to remove file {}", path.display()))?;
            }
            Ok(())
        }
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(()),
        Err(error) => Err(error.into()),
    }
}

fn print_plan(plan: &UninstallPlan) {
    println!("Uninstall mode: {}", plan.mode.label());
    println!("Config path:     {}", plan.layout.config_path.display());
    println!("Data directory:  {}", plan.layout.data_dir.display());
    println!("Install dir:     {}", plan.layout.install_dir.display());

    for warning in &plan.warnings {
        eprintln!("warning: {warning}");
    }

    let sections = build_plan_sections(plan);
    for (section, entries) in sections {
        if entries.is_empty() {
            continue;
        }

        println!();
        println!("{}:", section.title());
        for entry in entries {
            println!("  - {entry}");
        }
    }
}

fn build_plan_sections(plan: &UninstallPlan) -> BTreeMap<Section, Vec<String>> {
    let mut sections = BTreeMap::new();

    let mut service_entries = Vec::new();
    if plan.host_artifacts.systemd_unit_exists {
        service_entries.push("disable and stop `deku`".to_string());
        service_entries.push("reload systemd after removing the unit".to_string());
    }
    sections.insert(Section::ServiceManager, service_entries);

    let mut docker_entries = Vec::new();
    if plan.docker_available {
        for container in &plan.containers {
            docker_entries.push(format!(
                "remove container {} ({})",
                container.name, container.id
            ));
        }
        for image_tag in &plan.image_tags {
            docker_entries.push(format!("remove image {image_tag}"));
        }
        for network_name in &plan.network_names {
            docker_entries.push(format!("remove network {network_name}"));
        }
        if plan.mode == UninstallMode::FullRemove {
            for volume in &plan.service_volumes {
                docker_entries.push(format!("remove volume {volume}"));
            }
        }
    } else {
        docker_entries
            .push("docker cleanup will be skipped because docker is unavailable".to_string());
    }
    sections.insert(Section::DockerRuntime, docker_entries);

    let mut angie_entries = vec![
        format!("remove {}", plan.layout.angie_base_conf.display()),
        format!("remove {}", plan.layout.angie_conf_dir.display()),
        format!("remove {}", plan.layout.angie_apt_source.display()),
        format!("remove {}", plan.layout.angie_keyring.display()),
    ];
    for tls_file in &plan.tls_files {
        angie_entries.push(format!("remove {}", tls_file.display()));
    }
    if plan.mode == UninstallMode::FullRemove {
        angie_entries.push("disable and stop angie if active".to_string());
        angie_entries.push("purge apt package angie".to_string());
        angie_entries.push("refresh apt package lists".to_string());
    } else {
        angie_entries.push("reload angie if active".to_string());
    }
    sections.insert(Section::AngieConfig, angie_entries);

    let mut artifact_entries = Vec::new();
    if plan.host_artifacts.deku_bin_exists {
        artifact_entries.push(format!("remove {}", plan.layout.deku_bin.display()));
    }
    if plan.host_artifacts.dekud_bin_exists {
        artifact_entries.push(format!("remove {}", plan.layout.dekud_bin.display()));
    }
    if plan.host_artifacts.systemd_unit_exists {
        artifact_entries.push(format!("remove {}", plan.layout.systemd_unit.display()));
    }
    sections.insert(Section::InstallArtifacts, artifact_entries);

    let local_entries = if plan.mode == UninstallMode::FullRemove {
        plan.full_remove_paths
            .iter()
            .map(|path| format!("remove {}", path.display()))
            .collect()
    } else {
        vec![
            format!("preserve {}", plan.layout.config_dir.display()),
            format!("preserve {}", plan.layout.data_dir.display()),
            format!("preserve {}", plan.layout.dashboard_dir.display()),
            format!("preserve {}", plan.layout.socket_path.display()),
        ]
    };
    sections.insert(Section::LocalState, local_entries);

    sections
}

fn print_execution_summary(summary: &ExecutionSummary) {
    println!();
    println!("Completed actions: {}", summary.completed.len());
    for entry in &summary.completed {
        println!("  ✓ {entry}");
    }

    if !summary.skipped.is_empty() {
        println!();
        println!("Skipped actions: {}", summary.skipped.len());
        for entry in &summary.skipped {
            println!("  - {entry}");
        }
    }

    if !summary.failed.is_empty() {
        println!();
        println!("Failed actions: {}", summary.failed.len());
        for entry in &summary.failed {
            println!("  - {entry}");
        }
    }
}

fn dedupe_nested_paths(paths: Vec<PathBuf>) -> Vec<PathBuf> {
    let mut unique = BTreeSet::new();
    for path in paths {
        unique.insert(path);
    }

    let mut ordered = unique.into_iter().collect::<Vec<_>>();
    ordered.sort_by_key(|path| std::cmp::Reverse(path.components().count()));

    let mut filtered = Vec::new();
    'outer: for path in ordered {
        for existing in &filtered {
            if path.starts_with(existing) {
                continue 'outer;
            }
        }
        filtered.push(path);
    }
    filtered
}

fn env_path(name: &str, default: &str) -> PathBuf {
    env::var_os(name)
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from(default))
}

impl RealDockerRuntime {
    fn connect() -> Result<Self> {
        let docker = Docker::connect_with_socket_defaults()
            .map_err(|error| anyhow!("failed to connect to docker daemon: {error}"))?;
        Ok(Self { docker })
    }
}

#[async_trait]
impl DockerRuntime for RealDockerRuntime {
    async fn discover_managed_containers(&self) -> Result<Vec<ManagedContainer>> {
        let containers = self
            .docker
            .list_containers(Some(
                ListContainersOptionsBuilder::default().all(true).build(),
            ))
            .await?;

        let mut managed = Vec::new();
        for container in containers {
            let id = container.id.unwrap_or_default();
            if id.is_empty() {
                continue;
            }

            let names = container
                .names
                .unwrap_or_default()
                .into_iter()
                .map(|name| name.trim_start_matches('/').to_string())
                .collect::<Vec<_>>();
            let labels = container.labels.unwrap_or_default();
            let label_managed = labels
                .get("deku.managed")
                .is_some_and(|value| value == "true");
            let prefix_managed = names.iter().any(|name| {
                MANAGED_PREFIXES
                    .iter()
                    .any(|prefix| name.starts_with(prefix))
            });

            if label_managed || prefix_managed {
                managed.push(ManagedContainer {
                    id,
                    name: names
                        .first()
                        .cloned()
                        .unwrap_or_else(|| "unknown".to_string()),
                });
            }
        }

        managed.sort_by(|left, right| left.name.cmp(&right.name).then(left.id.cmp(&right.id)));
        managed.dedup_by(|left, right| left.id == right.id);
        Ok(managed)
    }

    async fn stop_container(&self, id: &str) -> Result<()> {
        let options = StopContainerOptionsBuilder::default().t(5).build();
        self.docker
            .stop_container(id, Some(options))
            .await
            .map_err(|error| anyhow!("{error}"))
    }

    async fn remove_container(&self, id: &str) -> Result<()> {
        let options = RemoveContainerOptionsBuilder::default().force(true).build();
        self.docker
            .remove_container(id, Some(options))
            .await
            .map_err(|error| anyhow!("{error}"))
    }

    async fn remove_image(&self, image: &str) -> Result<()> {
        let options = RemoveImageOptionsBuilder::default().force(true).build();
        self.docker
            .remove_image(image, Some(options), None)
            .await
            .map(|_| ())
            .map_err(|error| anyhow!("{error}"))
    }

    async fn remove_network(&self, name: &str) -> Result<()> {
        self.docker
            .remove_network(name)
            .await
            .map_err(|error| anyhow!("{error}"))
    }

    async fn remove_volume(&self, name: &str) -> Result<()> {
        let options = RemoveVolumeOptionsBuilder::default().force(true).build();
        self.docker
            .remove_volume(name, Some(options))
            .await
            .map_err(|error| anyhow!("{error}"))
    }
}

impl RealSystemManager {
    fn run_systemctl(args: &[&str]) -> Result<()> {
        let output = Command::new("systemctl")
            .args(args)
            .output()
            .with_context(|| format!("failed to run `systemctl {}`", args.join(" ")))?;

        if output.status.success() {
            return Ok(());
        }

        let stderr = String::from_utf8_lossy(&output.stderr).trim().to_string();
        let stdout = String::from_utf8_lossy(&output.stdout).trim().to_string();
        let details = if !stderr.is_empty() {
            stderr
        } else if !stdout.is_empty() {
            stdout
        } else {
            "systemctl exited with a non-zero status".to_string()
        };
        Err(anyhow!(details))
    }

    fn run_apt_get(args: &[&str]) -> Result<()> {
        let output = Command::new("apt-get")
            .env("DEBIAN_FRONTEND", "noninteractive")
            .args(args)
            .output()
            .with_context(|| format!("failed to run `apt-get {}`", args.join(" ")))?;

        if output.status.success() {
            return Ok(());
        }

        let stderr = String::from_utf8_lossy(&output.stderr).trim().to_string();
        let stdout = String::from_utf8_lossy(&output.stdout).trim().to_string();
        let details = if !stderr.is_empty() {
            stderr
        } else if !stdout.is_empty() {
            stdout
        } else {
            "apt-get exited with a non-zero status".to_string()
        };
        Err(anyhow!(details))
    }
}

impl SystemManager for RealSystemManager {
    fn disable_and_stop(&mut self, service: &str) -> Result<()> {
        Self::run_systemctl(&["disable", "--now", service])
    }

    fn daemon_reload(&mut self) -> Result<()> {
        Self::run_systemctl(&["daemon-reload"])
    }

    fn is_active(&mut self, service: &str) -> Result<bool> {
        let status = Command::new("systemctl")
            .args(["is-active", "--quiet", service])
            .status()
            .with_context(|| format!("failed to query `systemctl is-active {service}`"))?;
        Ok(status.success())
    }

    fn reload(&mut self, service: &str) -> Result<()> {
        Self::run_systemctl(&["reload", service])
    }

    fn restart(&mut self, service: &str) -> Result<()> {
        Self::run_systemctl(&["restart", service])
    }

    fn purge_package(&mut self, package: &str) -> Result<()> {
        Self::run_apt_get(&["purge", "-y", package])
    }

    fn apt_update(&mut self) -> Result<()> {
        Self::run_apt_get(&["update", "-qq"])
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::{
        ffi::OsString,
        sync::{Arc, Mutex, OnceLock},
        time::{SystemTime, UNIX_EPOCH},
    };
    use tokio::sync::{Mutex as AsyncMutex, MutexGuard as AsyncMutexGuard};

    #[derive(Default)]
    struct FakeSystemManager {
        calls: Vec<String>,
        angie_active: bool,
    }

    #[derive(Clone, Default)]
    struct FakeDockerRuntime {
        discovered: Vec<ManagedContainer>,
        ops: Arc<Mutex<Vec<String>>>,
    }

    impl SystemManager for FakeSystemManager {
        fn disable_and_stop(&mut self, service: &str) -> Result<()> {
            self.calls.push(format!("disable-stop:{service}"));
            Ok(())
        }

        fn daemon_reload(&mut self) -> Result<()> {
            self.calls.push("daemon-reload".to_string());
            Ok(())
        }

        fn is_active(&mut self, service: &str) -> Result<bool> {
            self.calls.push(format!("is-active:{service}"));
            Ok(service == "angie" && self.angie_active)
        }

        fn reload(&mut self, service: &str) -> Result<()> {
            self.calls.push(format!("reload:{service}"));
            Ok(())
        }

        fn restart(&mut self, service: &str) -> Result<()> {
            self.calls.push(format!("restart:{service}"));
            Ok(())
        }

        fn purge_package(&mut self, package: &str) -> Result<()> {
            self.calls.push(format!("purge-package:{package}"));
            Ok(())
        }

        fn apt_update(&mut self) -> Result<()> {
            self.calls.push("apt-update".to_string());
            Ok(())
        }
    }

    #[async_trait]
    impl DockerRuntime for FakeDockerRuntime {
        async fn discover_managed_containers(&self) -> Result<Vec<ManagedContainer>> {
            Ok(self.discovered.clone())
        }

        async fn stop_container(&self, id: &str) -> Result<()> {
            self.ops.lock().unwrap().push(format!("stop:{id}"));
            Ok(())
        }

        async fn remove_container(&self, id: &str) -> Result<()> {
            self.ops
                .lock()
                .unwrap()
                .push(format!("remove-container:{id}"));
            Ok(())
        }

        async fn remove_image(&self, image: &str) -> Result<()> {
            self.ops
                .lock()
                .unwrap()
                .push(format!("remove-image:{image}"));
            Ok(())
        }

        async fn remove_network(&self, name: &str) -> Result<()> {
            self.ops
                .lock()
                .unwrap()
                .push(format!("remove-network:{name}"));
            Ok(())
        }

        async fn remove_volume(&self, name: &str) -> Result<()> {
            self.ops
                .lock()
                .unwrap()
                .push(format!("remove-volume:{name}"));
            Ok(())
        }
    }

    struct TestLayout {
        root: PathBuf,
        config_dir: PathBuf,
        data_dir: PathBuf,
        install_dir: PathBuf,
        angie_conf_dir: PathBuf,
        angie_ssl_dir: PathBuf,
        angie_apt_source: PathBuf,
        angie_keyring: PathBuf,
        systemd_unit: PathBuf,
        angie_base_conf: PathBuf,
        _guard: AsyncMutexGuard<'static, ()>,
        old_env: Vec<(&'static str, Option<OsString>)>,
    }

    #[tokio::test]
    async fn packaged_install_detection_rejects_custom_layout() {
        let _guard = test_mutex().lock().await;
        let temp = temp_root("unsupported");
        fs::create_dir_all(&temp).unwrap();
        let old_env = capture_env();
        env::set_var("DEKU_CONFIG_DIR", &temp);
        env::set_var(ENV_INSTALL_DIR, temp.join("bin"));
        env::set_var(ENV_SYSTEMD_UNIT_PATH, temp.join("systemd/deku.service"));
        env::set_var(
            ENV_ANGIE_BASE_CONF,
            temp.join("angie/conf.d/deku-default.conf"),
        );
        env::set_var(ENV_ANGIE_SSL_DIR, temp.join("angie/ssl"));

        let result = discover_plan(UninstallMode::KeepData, None).await;
        assert!(result.is_err());

        restore_envs(old_env);
        fs::remove_dir_all(&temp).unwrap();
    }

    #[tokio::test]
    async fn full_remove_discovers_packaged_targets() {
        let fixture = create_fixture(true).await;
        let docker = FakeDockerRuntime {
            discovered: vec![
                ManagedContainer {
                    id: "app-ctr".to_string(),
                    name: "deku.my-app.web.abcd1234-0".to_string(),
                },
                ManagedContainer {
                    id: "svc-ctr".to_string(),
                    name: "deku-postgres-db".to_string(),
                },
                ManagedContainer {
                    id: "helper-ctr".to_string(),
                    name: "deku-helper-123".to_string(),
                },
            ],
            ops: Arc::new(Mutex::new(Vec::new())),
        };

        let plan = discover_plan(UninstallMode::FullRemove, Some(&docker))
            .await
            .unwrap();

        assert_eq!(plan.mode, UninstallMode::FullRemove);
        assert!(plan.host_artifacts.systemd_unit_exists);
        assert_eq!(plan.image_tags, vec!["deku/my-app:latest".to_string()]);
        assert_eq!(plan.network_names, vec!["shared-net".to_string()]);
        assert_eq!(
            plan.service_volumes,
            vec!["deku-postgres-db-data".to_string()]
        );
        assert!(plan
            .tls_files
            .iter()
            .any(|path| path.ends_with("deku_my-app.crt")));
        assert!(plan.full_remove_paths.contains(&fixture.config_dir));
        assert!(plan.full_remove_paths.contains(&fixture.data_dir));
        assert_eq!(plan.containers.len(), 5);

        fixture.cleanup();
    }

    #[tokio::test]
    async fn keep_data_preserves_local_state() {
        let fixture = create_fixture(true).await;
        let docker = FakeDockerRuntime {
            discovered: vec![],
            ops: Arc::new(Mutex::new(Vec::new())),
        };

        let plan = discover_plan(UninstallMode::KeepData, Some(&docker))
            .await
            .unwrap();

        assert!(plan.full_remove_paths.is_empty());
        assert!(plan
            .service_volumes
            .contains(&"deku-postgres-db-data".to_string()));
        assert_eq!(plan.mode, UninstallMode::KeepData);

        fixture.cleanup();
    }

    #[tokio::test]
    async fn execute_plan_removes_packaged_artifacts_and_preserves_data_in_keep_mode() {
        let fixture = create_fixture(true).await;
        let docker = FakeDockerRuntime {
            discovered: vec![ManagedContainer {
                id: "app-ctr".to_string(),
                name: "deku.my-app.web.abcd1234-0".to_string(),
            }],
            ops: Arc::new(Mutex::new(Vec::new())),
        };
        let plan = discover_plan(UninstallMode::KeepData, Some(&docker))
            .await
            .unwrap();

        let mut system = FakeSystemManager {
            calls: Vec::new(),
            angie_active: true,
        };
        let summary = execute_plan(&plan, &mut system, Some(&docker)).await;

        assert!(summary.failed.is_empty());
        assert!(!fixture.systemd_unit.exists());
        assert!(!fixture.install_dir.join("deku").exists());
        assert!(!fixture.install_dir.join("dekud").exists());
        assert!(!fixture.angie_base_conf.exists());
        assert!(!fixture.angie_conf_dir.exists());
        assert!(!fixture.angie_apt_source.exists());
        assert!(!fixture.angie_keyring.exists());
        assert!(!fixture.angie_ssl_dir.join("deku_my-app.crt").exists());
        assert!(fixture.config_dir.exists());
        assert!(fixture.data_dir.exists());

        let ops = docker.ops.lock().unwrap().clone();
        assert!(ops.contains(&"stop:app-ctr".to_string()));
        assert!(ops.contains(&"remove-container:app-ctr".to_string()));
        assert!(system.calls.contains(&"reload:angie".to_string()));
        assert!(!system.calls.contains(&"purge-package:angie".to_string()));
        assert!(!system.calls.contains(&"apt-update".to_string()));

        fixture.cleanup();
    }

    #[tokio::test]
    async fn execute_plan_removes_local_state_in_full_mode() {
        let fixture = create_fixture(true).await;
        let docker = FakeDockerRuntime {
            discovered: vec![],
            ops: Arc::new(Mutex::new(Vec::new())),
        };
        let plan = discover_plan(UninstallMode::FullRemove, Some(&docker))
            .await
            .unwrap();

        let mut system = FakeSystemManager {
            calls: Vec::new(),
            angie_active: true,
        };
        let summary = execute_plan(&plan, &mut system, Some(&docker)).await;

        assert!(summary.failed.is_empty());
        assert!(!fixture.config_dir.exists());
        assert!(!fixture.data_dir.exists());
        assert!(system.calls.contains(&"disable-stop:angie".to_string()));
        assert!(system.calls.contains(&"purge-package:angie".to_string()));
        assert!(system.calls.contains(&"apt-update".to_string()));

        let ops = docker.ops.lock().unwrap().clone();
        assert!(ops.contains(&"remove-volume:deku-postgres-db-data".to_string()));
        assert!(ops.contains(&"remove-image:deku/my-app:latest".to_string()));
        assert!(ops.contains(&"remove-network:shared-net".to_string()));

        fixture.cleanup();
    }

    async fn create_fixture(include_packaged_artifacts: bool) -> TestLayout {
        let guard = test_mutex().lock().await;
        let root = temp_root("uninstall");
        let config_dir = root.join("config");
        let data_dir = root.join("data");
        let install_dir = root.join("bin");
        let angie_conf_dir = root.join("angie/conf.d/deku");
        let angie_ssl_dir = root.join("angie/ssl");
        let angie_apt_source = root.join("apt/sources.list.d/angie.list");
        let angie_keyring = root.join("keyrings/angie-signing.gpg");
        let systemd_unit = root.join("systemd/deku.service");
        let angie_base_conf = root.join("angie/conf.d/deku-default.conf");

        fs::create_dir_all(&config_dir).unwrap();
        fs::create_dir_all(&data_dir).unwrap();
        fs::create_dir_all(&install_dir).unwrap();
        fs::create_dir_all(&angie_conf_dir).unwrap();
        fs::create_dir_all(&angie_ssl_dir).unwrap();
        fs::create_dir_all(angie_apt_source.parent().unwrap()).unwrap();
        fs::create_dir_all(angie_keyring.parent().unwrap()).unwrap();
        fs::create_dir_all(systemd_unit.parent().unwrap()).unwrap();
        fs::create_dir_all(angie_base_conf.parent().unwrap()).unwrap();

        if include_packaged_artifacts {
            fs::write(install_dir.join("deku"), "bin").unwrap();
            fs::write(install_dir.join("dekud"), "bin").unwrap();
            fs::write(&systemd_unit, "unit").unwrap();
            fs::write(&angie_base_conf, "conf").unwrap();
            fs::write(angie_conf_dir.join("my-app.conf"), "server {}").unwrap();
            fs::write(&angie_apt_source, "deb ...").unwrap();
            fs::write(&angie_keyring, "gpg").unwrap();
        }

        let old_env = capture_env();
        env::set_var("DEKU_CONFIG_DIR", &config_dir);
        env::set_var(ENV_INSTALL_DIR, &install_dir);
        env::set_var(ENV_SYSTEMD_UNIT_PATH, &systemd_unit);
        env::set_var(ENV_ANGIE_BASE_CONF, &angie_base_conf);
        env::set_var(ENV_ANGIE_SSL_DIR, &angie_ssl_dir);
        env::set_var(ENV_ANGIE_APT_SOURCE, &angie_apt_source);
        env::set_var(ENV_ANGIE_KEYRING, &angie_keyring);

        let config = LocalDekuConfig {
            data_dir: Some(data_dir.clone()),
            angie_conf_dir: Some(angie_conf_dir.clone()),
            ..LocalDekuConfig::default()
        };
        crate::local_config::save(&config).unwrap();

        fs::write(angie_ssl_dir.join("deku_my-app.crt"), "crt").unwrap();
        fs::write(angie_ssl_dir.join("deku_my-app.key"), "key").unwrap();
        write_test_database(&data_dir.join("deku.db")).await;

        TestLayout {
            root,
            config_dir,
            data_dir,
            install_dir,
            angie_conf_dir,
            angie_ssl_dir,
            angie_apt_source,
            angie_keyring,
            systemd_unit,
            angie_base_conf,
            _guard: guard,
            old_env,
        }
    }

    async fn write_test_database(path: &Path) {
        let options = SqliteConnectOptions::new()
            .filename(path)
            .create_if_missing(true)
            .disable_statement_logging();
        let pool = SqlitePoolOptions::new()
            .max_connections(1)
            .connect_with(options)
            .await
            .unwrap();

        sqlx::query("CREATE TABLE apps (name TEXT NOT NULL)")
            .execute(&pool)
            .await
            .unwrap();
        sqlx::query(
            "CREATE TABLE containers (id TEXT NOT NULL, created_at TEXT NOT NULL DEFAULT CURRENT_TIMESTAMP)",
        )
        .execute(&pool)
        .await
        .unwrap();
        sqlx::query(
            "CREATE TABLE services (container_id TEXT, plugin TEXT NOT NULL, config TEXT NOT NULL)",
        )
        .execute(&pool)
        .await
        .unwrap();
        sqlx::query("CREATE TABLE networks (name TEXT NOT NULL)")
            .execute(&pool)
            .await
            .unwrap();

        sqlx::query("INSERT INTO apps (name) VALUES ('my-app')")
            .execute(&pool)
            .await
            .unwrap();
        sqlx::query("INSERT INTO containers (id) VALUES ('db-recorded-container')")
            .execute(&pool)
            .await
            .unwrap();
        sqlx::query("INSERT INTO services (container_id, plugin, config) VALUES (?1, ?2, ?3)")
            .bind("svc-recorded-container")
            .bind("postgres")
            .bind(r#"{"volume":"deku-postgres-db-data"}"#)
            .execute(&pool)
            .await
            .unwrap();
        sqlx::query("INSERT INTO networks (name) VALUES ('shared-net')")
            .execute(&pool)
            .await
            .unwrap();

        pool.close().await;
    }

    fn temp_root(prefix: &str) -> PathBuf {
        let stamp = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        env::temp_dir().join(format!("deku-{prefix}-{stamp}"))
    }

    fn capture_env() -> Vec<(&'static str, Option<OsString>)> {
        vec![
            ("DEKU_CONFIG_DIR", env::var_os("DEKU_CONFIG_DIR")),
            (ENV_INSTALL_DIR, env::var_os(ENV_INSTALL_DIR)),
            (ENV_SYSTEMD_UNIT_PATH, env::var_os(ENV_SYSTEMD_UNIT_PATH)),
            (ENV_ANGIE_BASE_CONF, env::var_os(ENV_ANGIE_BASE_CONF)),
            (ENV_ANGIE_SSL_DIR, env::var_os(ENV_ANGIE_SSL_DIR)),
            (ENV_ANGIE_APT_SOURCE, env::var_os(ENV_ANGIE_APT_SOURCE)),
            (ENV_ANGIE_KEYRING, env::var_os(ENV_ANGIE_KEYRING)),
        ]
    }

    fn restore_envs(values: Vec<(&'static str, Option<OsString>)>) {
        for (name, value) in values {
            if let Some(value) = value {
                env::set_var(name, value);
            } else {
                env::remove_var(name);
            }
        }
    }

    fn test_mutex() -> &'static AsyncMutex<()> {
        static LOCK: OnceLock<AsyncMutex<()>> = OnceLock::new();
        LOCK.get_or_init(|| AsyncMutex::new(()))
    }

    impl TestLayout {
        fn cleanup(&self) {
            restore_envs(self.old_env.clone());
            let _ = fs::remove_dir_all(&self.root);
        }
    }
}
