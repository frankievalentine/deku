//! Persisted, searchable logs for every deployment.
//!
//! Two things were missing before this: runtime output was only ever a snapshot
//! of the running containers (so a retired deployment's logs were gone), and
//! nothing was searchable. Every line now lands in `log_lines` tagged with the
//! app, deployment, environment, source, and stream, plus a best-effort level,
//! and search runs over an FTS5 index the bundled SQLite compiles in.
//!
//! Storage is bounded by a per-app line cap so a chatty app cannot fill the disk.

use std::sync::Arc;

use anyhow::Result;
use bollard::query_parameters::LogsOptionsBuilder;
use bollard::Docker;
use chrono::{DateTime, Utc};
use futures::StreamExt;
use serde::{Deserialize, Serialize};
use sqlx::{Row, SqlitePool};
use tokio::sync::broadcast;
use tokio::time::interval;
use tracing::{info, warn};
use uuid::Uuid;

use crate::api::SharedState;
use crate::db::queries;

/// Where a line came from.
pub const SOURCE_BUILD: &str = "build";
/// Runtime container output.
pub const SOURCE_RUNTIME: &str = "runtime";

/// Default number of lines kept per app.
pub const DEFAULT_RETAIN_LINES: i64 = 20_000;

/// Lines delivered to live subscribers before the slowest one is dropped.
const BROADCAST_CAPACITY: usize = 2048;

/// Maximum lines returned by one query.
pub const MAX_QUERY_LINES: i64 = 2_000;

#[derive(Debug, Clone, Serialize, sqlx::FromRow)]
pub struct LogLine {
    pub id: String,
    pub app_id: String,
    pub deployment_id: Option<String>,
    pub environment_id: Option<String>,
    pub source: String,
    pub stream: String,
    pub level: String,
    pub message: String,
    pub created_at: DateTime<Utc>,
}

/// A stored line plus its FTS5 snippet, when the query came from search.
#[derive(Debug, Clone, Serialize)]
pub struct LogLineRecord {
    #[serde(flatten)]
    pub line: LogLine,
    /// Matching context with the search term delimited, from FTS5 `snippet()`.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub snippet: Option<String>,
}

/// Filter for a log query.
#[derive(Debug, Clone, Default, Deserialize)]
pub struct LogQuery {
    pub search: Option<String>,
    pub deployment: Option<String>,
    pub environment: Option<String>,
    pub source: Option<String>,
    pub stream: Option<String>,
    pub level: Option<String>,
    pub limit: Option<i64>,
}

impl LogQuery {
    /// Clamp and normalise caller input.
    fn bounds(&self) -> i64 {
        self.limit.unwrap_or(200).clamp(1, MAX_QUERY_LINES)
    }

    fn non_empty(value: &Option<String>) -> Option<&str> {
        value.as_deref().map(str::trim).filter(|v| !v.is_empty())
    }
}

/// Persist and broadcast log lines.
pub struct LogBus {
    tx: broadcast::Sender<LogLine>,
    pool: SqlitePool,
}

impl LogBus {
    pub fn new(pool: SqlitePool) -> Arc<Self> {
        let (tx, _) = broadcast::channel(BROADCAST_CAPACITY);
        Arc::new(Self { tx, pool })
    }

    pub fn subscribe(&self) -> broadcast::Receiver<LogLine> {
        self.tx.subscribe()
    }

    /// Store a line and hand it to live subscribers.
    ///
    /// Broadcasting happens before the write so a slow insert cannot delay live
    /// tailing; the row itself is what search and history read.
    pub async fn append(
        &self,
        app_id: &str,
        deployment_id: Option<&str>,
        environment_id: Option<&str>,
        source: &str,
        stream: &str,
        message: &str,
    ) -> Result<LogLine> {
        let line = LogLine {
            id: Uuid::new_v4().to_string(),
            app_id: app_id.to_string(),
            deployment_id: deployment_id.map(str::to_string),
            environment_id: environment_id.map(str::to_string),
            source: source.to_string(),
            stream: stream.to_string(),
            level: parse_level(message).to_string(),
            message: message.to_string(),
            created_at: Utc::now(),
        };

        sqlx::query(
            "INSERT INTO log_lines \
             (id, app_id, deployment_id, environment_id, source, stream, level, message, created_at) \
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9)",
        )
        .bind(&line.id)
        .bind(&line.app_id)
        .bind(&line.deployment_id)
        .bind(&line.environment_id)
        .bind(&line.source)
        .bind(&line.stream)
        .bind(&line.level)
        .bind(&line.message)
        .bind(line.created_at)
        .execute(&self.pool)
        .await?;

        // No subscribers is not an error.
        let _ = self.tx.send(line.clone());
        Ok(line)
    }
}

/// Start log maintenance: reattach to containers that are already running, then
/// prune each app to its line cap on a timer.
///
/// Reattaching matters after a daemon restart: containers keep running while the
/// daemon is down, and their new output should keep landing in the store.
pub fn spawn_maintenance(state: SharedState) {
    tokio::spawn(async move {
        if let Err(error) = attach_to_running_containers(&state).await {
            warn!(error = %error, "failed to reattach container log collectors");
        }

        let mut ticker = interval(std::time::Duration::from_secs(300));
        loop {
            ticker.tick().await;
            let keep = state.config.logs.retain_lines.max(1);
            match prune_all(&state.pool, keep).await {
                Ok(removed) if removed > 0 => {
                    info!(removed, keep, "pruned stored log lines");
                }
                Ok(_) => {}
                Err(error) => warn!(error = %error, "log pruning failed"),
            }
        }
    });
}

/// Follow every running app container's output from now on.
async fn attach_to_running_containers(state: &SharedState) -> Result<()> {
    let apps = queries::list_apps(&state.pool).await?;
    let mut attached = 0usize;
    for app in apps {
        // `list_containers_for_app` only returns running containers.
        for container in queries::list_containers_for_app(&state.pool, &app.id).await? {
            let environment_id =
                queries::get_deployment_environment_id(&state.pool, &container.deployment_id)
                    .await
                    .unwrap_or(None);

            spawn_runtime_collector(
                state.logs.clone(),
                state.docker.clone(),
                container.id.clone(),
                app.id.clone(),
                Some(container.deployment_id.clone()),
                environment_id,
                CollectorStart::Now,
            );
            attached += 1;
        }
    }

    if attached > 0 {
        info!(containers = attached, "following container output");
    }
    Ok(())
}

/// Whether a collector should replay a container's existing output.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CollectorStart {
    /// The container was just created: take its output from the beginning.
    Beginning,
    /// The container was already running (daemon restart): take only new output,
    /// so a restart does not re-ingest history that is already stored.
    Now,
}

/// Mirrors a deploy's `build.log` events into the log store.
///
/// Build output already travels the event bus, and the builders do not know
/// which deployment they are serving, so the deploy mirrors its own events
/// rather than threading a log handle through every builder. `stop` drains what
/// is buffered before the task exits, so the last build lines are not lost.
pub struct BuildLogMirror {
    done: Arc<std::sync::atomic::AtomicBool>,
    handle: tokio::task::JoinHandle<()>,
}

impl BuildLogMirror {
    /// Finish collecting and wait for the mirror to drain.
    pub async fn stop(self) {
        self.done.store(true, std::sync::atomic::Ordering::Relaxed);
        let _ = self.handle.await;
    }
}

/// Start mirroring one deployment's build output.
pub fn spawn_build_log_mirror(
    mut receiver: tokio::sync::broadcast::Receiver<deku_core::types::Event>,
    logs: Arc<LogBus>,
    app_id: String,
    deployment_id: String,
    environment_id: String,
) -> BuildLogMirror {
    let done = Arc::new(std::sync::atomic::AtomicBool::new(false));
    let done_flag = done.clone();

    let handle = tokio::spawn(async move {
        loop {
            // A short wait lets the loop notice `stop` once the deploy is done,
            // after everything already queued has been stored.
            match tokio::time::timeout(std::time::Duration::from_millis(500), receiver.recv()).await
            {
                Ok(Ok(event)) => {
                    if event.event_type != "build.log" || event.app_id.as_deref() != Some(&app_id) {
                        continue;
                    }
                    let Some(payload) = event.payload.as_deref() else {
                        continue;
                    };
                    let Ok(parsed) = serde_json::from_str::<serde_json::Value>(payload) else {
                        continue;
                    };
                    let Some(line) = parsed["line"].as_str() else {
                        continue;
                    };
                    if let Err(error) = logs
                        .append(
                            &app_id,
                            Some(&deployment_id),
                            Some(&environment_id),
                            SOURCE_BUILD,
                            "stdout",
                            line,
                        )
                        .await
                    {
                        warn!(error = %error, "failed to store build log line");
                    }
                }
                Ok(Err(tokio::sync::broadcast::error::RecvError::Lagged(skipped))) => {
                    warn!(skipped, "build log mirror fell behind");
                }
                Ok(Err(tokio::sync::broadcast::error::RecvError::Closed)) => break,
                Err(_) => {
                    if done_flag.load(std::sync::atomic::Ordering::Relaxed) {
                        break;
                    }
                }
            }
        }
    });

    BuildLogMirror { done, handle }
}

/// Follow a container's output and append every line until the stream ends.
///
/// The task ends when the container stops, which is also how a retired
/// deployment stops collecting: its container is removed and the stream closes.
pub fn spawn_runtime_collector(
    logs: Arc<LogBus>,
    docker: Docker,
    container_id: String,
    app_id: String,
    deployment_id: Option<String>,
    environment_id: Option<String>,
    start: CollectorStart,
) -> tokio::task::JoinHandle<()> {
    tokio::spawn(async move {
        // The builder consumes itself on every setter, so each mode is built
        // in one chain.
        let options = match start {
            CollectorStart::Beginning => LogsOptionsBuilder::default()
                .stdout(true)
                .stderr(true)
                .follow(true)
                .tail("all")
                .build(),
            // Only new output: whatever happened before the restart is stored.
            CollectorStart::Now => LogsOptionsBuilder::default()
                .stdout(true)
                .stderr(true)
                .follow(true)
                .since(Utc::now().timestamp() as i32)
                .tail("0")
                .build(),
        };

        let mut stream = docker.logs(&container_id, Some(options));
        while let Some(item) = stream.next().await {
            let (stream_name, message) = match item {
                Ok(bollard::container::LogOutput::StdOut { message }) => ("stdout", message),
                Ok(bollard::container::LogOutput::StdErr { message }) => ("stderr", message),
                Ok(_) => continue,
                Err(error) => {
                    tracing::debug!(
                        container = %container_id,
                        error = %error,
                        "stopped following container output"
                    );
                    break;
                }
            };

            let text = String::from_utf8_lossy(&message);
            for line in text.lines().filter(|line| !line.trim().is_empty()) {
                if let Err(error) = logs
                    .append(
                        &app_id,
                        deployment_id.as_deref(),
                        environment_id.as_deref(),
                        SOURCE_RUNTIME,
                        stream_name,
                        line,
                    )
                    .await
                {
                    tracing::warn!(container = %container_id, error = %error, "failed to store log line");
                }
            }
        }
    })
}

/// Read stored log lines, optionally filtered and searched.
///
/// Both statements are literals with null-tolerant predicates: a `NULL` bind
/// means "do not filter on this field". That keeps the SQL auditable and avoids
/// assembling a query string per combination of filters.
pub async fn query(
    pool: &SqlitePool,
    app_id: &str,
    filter: &LogQuery,
) -> Result<Vec<LogLineRecord>> {
    let limit = filter.bounds();
    let deployment = LogQuery::non_empty(&filter.deployment);
    let environment = LogQuery::non_empty(&filter.environment);
    let source = LogQuery::non_empty(&filter.source);
    let stream = LogQuery::non_empty(&filter.stream);
    let level = LogQuery::non_empty(&filter.level);

    let rows = if let Some(search) = LogQuery::non_empty(&filter.search) {
        // Search runs over the FTS index and joins back for the full record,
        // returning a snippet of the matching context.
        sqlx::query(
            "SELECT l.id, l.app_id, l.deployment_id, l.environment_id, l.source, l.stream, \
                    l.level, l.message, l.created_at, \
                    snippet(log_lines_fts, 0, '[', ']', '...', 12) AS snippet \
             FROM log_lines_fts \
             JOIN log_lines l ON l.rowid = log_lines_fts.rowid \
             WHERE log_lines_fts MATCH ?1 \
               AND l.app_id = ?2 \
               AND (?3 IS NULL OR l.deployment_id = ?3) \
               AND (?4 IS NULL OR l.environment_id = ?4) \
               AND (?5 IS NULL OR l.source = ?5) \
               AND (?6 IS NULL OR l.stream = ?6) \
               AND (?7 IS NULL OR l.level = ?7) \
             ORDER BY l.created_at DESC, l.rowid DESC LIMIT ?8",
        )
        .bind(fts_query(search))
        .bind(app_id)
        .bind(deployment)
        .bind(environment)
        .bind(source)
        .bind(stream)
        .bind(level)
        .bind(limit)
        .fetch_all(pool)
        .await?
    } else {
        sqlx::query(
            "SELECT l.id, l.app_id, l.deployment_id, l.environment_id, l.source, l.stream, \
                    l.level, l.message, l.created_at, NULL AS snippet \
             FROM log_lines l \
             WHERE l.app_id = ?1 \
               AND (?2 IS NULL OR l.deployment_id = ?2) \
               AND (?3 IS NULL OR l.environment_id = ?3) \
               AND (?4 IS NULL OR l.source = ?4) \
               AND (?5 IS NULL OR l.stream = ?5) \
               AND (?6 IS NULL OR l.level = ?6) \
             ORDER BY l.created_at DESC, l.rowid DESC LIMIT ?7",
        )
        .bind(app_id)
        .bind(deployment)
        .bind(environment)
        .bind(source)
        .bind(stream)
        .bind(level)
        .bind(limit)
        .fetch_all(pool)
        .await?
    };

    let mut records: Vec<LogLineRecord> = rows
        .into_iter()
        .map(|row| {
            Ok(LogLineRecord {
                line: LogLine {
                    id: row.try_get("id")?,
                    app_id: row.try_get("app_id")?,
                    deployment_id: row.try_get("deployment_id")?,
                    environment_id: row.try_get("environment_id")?,
                    source: row.try_get("source")?,
                    stream: row.try_get("stream")?,
                    level: row.try_get("level")?,
                    message: row.try_get("message")?,
                    created_at: row.try_get("created_at")?,
                },
                snippet: row.try_get("snippet").unwrap_or(None),
            })
        })
        .collect::<Result<Vec<_>, sqlx::Error>>()?;

    // Queried newest-first for the limit, returned oldest-first for reading.
    records.reverse();
    Ok(records)
}

/// Turn free text into an FTS5 query that cannot be a syntax error.
///
/// Every whitespace-separated term is quoted, so punctuation a user types (a
/// colon in a timestamp, a hyphen, a stray `*`) cannot break the MATCH parser.
fn fts_query(raw: &str) -> String {
    let terms: Vec<String> = raw
        .split_whitespace()
        .map(|term| format!("\"{}\"", term.replace('"', "\"\"")))
        .collect();
    if terms.is_empty() {
        return "\"\"".to_string();
    }
    terms.join(" ")
}

/// Delete the oldest lines for an app beyond `keep`.
pub async fn prune(pool: &SqlitePool, app_id: &str, keep: i64) -> Result<u64> {
    let keep = keep.max(1);
    let result = sqlx::query(
        "DELETE FROM log_lines WHERE app_id = ?1 AND rowid NOT IN ( \
             SELECT rowid FROM log_lines WHERE app_id = ?1 \
             ORDER BY created_at DESC, rowid DESC LIMIT ?2 \
         )",
    )
    .bind(app_id)
    .bind(keep)
    .execute(pool)
    .await?;
    Ok(result.rows_affected())
}

/// Prune every app that is over its cap.
pub async fn prune_all(pool: &SqlitePool, keep: i64) -> Result<u64> {
    let apps = sqlx::query_as::<_, (String,)>("SELECT id FROM apps")
        .fetch_all(pool)
        .await?;
    let mut removed = 0;
    for (app_id,) in apps {
        removed += prune(pool, &app_id, keep).await?;
    }
    Ok(removed)
}

/// Best-effort level extraction from a line.
///
/// Build output and app output are plain text, so the level is a heuristic:
/// a bracketed tag, a `LEVEL:` prefix, `LEVEL -`, or a `level=value` pair. This
/// mirrors how other self-hosted PaaS tools infer levels and is only used to
/// filter and colour, never to hide lines.
pub fn parse_level(message: &str) -> &'static str {
    let haystack = message.to_ascii_lowercase();
    for (needle, level) in [
        ("[error]", "ERROR"),
        ("[warn]", "WARNING"),
        ("[warning]", "WARNING"),
        ("[info]", "INFO"),
        ("[debug]", "DEBUG"),
        ("[critical]", "CRITICAL"),
        ("[fatal]", "CRITICAL"),
        ("error:", "ERROR"),
        ("warn:", "WARNING"),
        ("warning:", "WARNING"),
        ("error -", "ERROR"),
        ("warn -", "WARNING"),
        ("level=error", "ERROR"),
        ("level=warn", "WARNING"),
        ("level=debug", "DEBUG"),
        ("\"level\":\"error\"", "ERROR"),
        ("\"level\":\"warn\"", "WARNING"),
    ] {
        if haystack.contains(needle) {
            return level;
        }
    }
    "INFO"
}

#[cfg(test)]
mod tests {
    use super::{fts_query, parse_level, prune, query, LogBus, LogQuery};
    use sqlx::sqlite::SqlitePoolOptions;
    use sqlx::SqlitePool;

    async fn pool_with_app(name: &str) -> (SqlitePool, String) {
        let pool = SqlitePoolOptions::new()
            .max_connections(1)
            .connect("sqlite::memory:")
            .await
            .expect("pool");
        crate::db::migrate(&pool).await.expect("migrate");
        sqlx::query("INSERT INTO apps (id, name, status, created_at) VALUES ('app-1', ?1, 'created', CURRENT_TIMESTAMP)")
            .bind(name)
            .execute(&pool)
            .await
            .expect("app");
        (pool, "app-1".to_string())
    }

    #[test]
    fn parses_levels_from_common_shapes() {
        assert_eq!(parse_level("starting gunicorn"), "INFO");
        assert_eq!(parse_level("[ERROR] failed to bind port 80"), "ERROR");
        assert_eq!(parse_level("error: connection refused"), "ERROR");
        assert_eq!(parse_level("WARN - cache miss"), "WARNING");
        assert_eq!(
            parse_level("{\"level\":\"error\",\"msg\":\"boom\"}"),
            "ERROR"
        );
        assert_eq!(parse_level("[debug] request 12"), "DEBUG");
    }

    #[test]
    fn fts_queries_quote_every_term() {
        assert_eq!(fts_query("bind port"), "\"bind\" \"port\"");
        // Punctuation that would otherwise be FTS syntax stays inert.
        assert_eq!(fts_query("12:00:01"), "\"12:00:01\"");
        assert_eq!(fts_query("a\"b"), "\"a\"\"b\"");
        assert_eq!(fts_query("   "), "\"\"");
    }

    #[tokio::test]
    async fn stores_lines_with_metadata_and_reads_them_back() {
        let (pool, app_id) = pool_with_app("one").await;
        let logs = LogBus::new(pool.clone());

        logs.append(
            &app_id,
            Some("dep-1"),
            Some("env-1"),
            "runtime",
            "stdout",
            "[INFO] hello",
        )
        .await
        .expect("append");
        logs.append(
            &app_id,
            Some("dep-1"),
            Some("env-1"),
            "runtime",
            "stderr",
            "[ERROR] boom",
        )
        .await
        .expect("append");

        let lines = query(&pool, &app_id, &LogQuery::default())
            .await
            .expect("query");
        assert_eq!(lines.len(), 2);
        assert_eq!(lines[0].line.message, "[INFO] hello");
        assert_eq!(lines[0].line.level, "INFO");
        assert_eq!(lines[1].line.level, "ERROR");
        assert_eq!(lines[1].line.stream, "stderr");
        assert_eq!(lines[1].line.deployment_id.as_deref(), Some("dep-1"));
    }

    #[tokio::test]
    async fn search_matches_substrings_case_insensitively() {
        let (pool, app_id) = pool_with_app("two").await;
        let logs = LogBus::new(pool.clone());
        logs.append(
            &app_id,
            None,
            None,
            "runtime",
            "stdout",
            "listening on port 3000",
        )
        .await
        .expect("append");
        logs.append(
            &app_id,
            None,
            None,
            "build",
            "stdout",
            "Installing dependencies",
        )
        .await
        .expect("append");

        let hits = query(
            &pool,
            &app_id,
            &LogQuery {
                search: Some("LISTENING".to_string()),
                ..LogQuery::default()
            },
        )
        .await
        .expect("search");
        assert_eq!(hits.len(), 1);
        assert!(hits[0].line.message.contains("listening"));
        assert!(
            hits[0].snippet.is_some(),
            "search should return match context"
        );
    }

    #[tokio::test]
    async fn search_with_punctuation_is_not_a_syntax_error() {
        let (pool, app_id) = pool_with_app("three").await;
        let logs = LogBus::new(pool.clone());
        logs.append(
            &app_id,
            None,
            None,
            "runtime",
            "stdout",
            "started at 12:00:01",
        )
        .await
        .expect("append");

        // None of these should error, even though they are FTS syntax in raw form.
        for raw in ["12:00:01", "AND OR NOT", "(unbalanced", "trailing*"] {
            let result = query(
                &pool,
                &app_id,
                &LogQuery {
                    search: Some(raw.to_string()),
                    ..LogQuery::default()
                },
            )
            .await;
            assert!(result.is_ok(), "search {raw:?} should not error");
        }
    }

    #[tokio::test]
    async fn filters_narrow_by_source_stream_and_deployment() {
        let (pool, app_id) = pool_with_app("four").await;
        let logs = LogBus::new(pool.clone());
        logs.append(&app_id, Some("dep-1"), None, "build", "stdout", "compiling")
            .await
            .expect("append");
        logs.append(&app_id, Some("dep-1"), None, "runtime", "stdout", "serving")
            .await
            .expect("append");
        logs.append(
            &app_id,
            Some("dep-2"),
            None,
            "runtime",
            "stderr",
            "[ERROR] crashed",
        )
        .await
        .expect("append");

        let build_only = query(
            &pool,
            &app_id,
            &LogQuery {
                source: Some("build".to_string()),
                ..LogQuery::default()
            },
        )
        .await
        .expect("build");
        assert_eq!(build_only.len(), 1);
        assert_eq!(build_only[0].line.message, "compiling");

        let dep2 = query(
            &pool,
            &app_id,
            &LogQuery {
                deployment: Some("dep-2".to_string()),
                ..LogQuery::default()
            },
        )
        .await
        .expect("dep-2");
        assert_eq!(dep2.len(), 1);
        assert_eq!(dep2[0].line.message, "[ERROR] crashed");

        // Level is inferred from the line; `stream` is the precise way to isolate
        // stderr regardless of how the message reads.
        let errors = query(
            &pool,
            &app_id,
            &LogQuery {
                level: Some("ERROR".to_string()),
                ..LogQuery::default()
            },
        )
        .await
        .expect("errors");
        assert_eq!(errors.len(), 1);
        assert_eq!(errors[0].line.message, "[ERROR] crashed");

        let stderr = query(
            &pool,
            &app_id,
            &LogQuery {
                stream: Some("stderr".to_string()),
                ..LogQuery::default()
            },
        )
        .await
        .expect("stderr");
        assert_eq!(stderr.len(), 1);
    }

    #[tokio::test]
    async fn the_line_cap_is_applied_per_app() {
        let (pool, app_id) = pool_with_app("six").await;
        sqlx::query("INSERT INTO apps (id, name, status, created_at) VALUES ('app-2', 'other', 'created', CURRENT_TIMESTAMP)")
            .execute(&pool)
            .await
            .expect("second app");
        let logs = LogBus::new(pool.clone());
        for index in 0..5 {
            logs.append(
                &app_id,
                None,
                None,
                "runtime",
                "stdout",
                &format!("a{index}"),
            )
            .await
            .expect("append");
            logs.append(
                "app-2",
                None,
                None,
                "runtime",
                "stdout",
                &format!("b{index}"),
            )
            .await
            .expect("append");
        }

        let removed = super::prune_all(&pool, 2).await.expect("prune all");
        assert_eq!(removed, 6);
        assert_eq!(
            query(&pool, &app_id, &LogQuery::default())
                .await
                .expect("a")
                .len(),
            2
        );
        assert_eq!(
            query(&pool, "app-2", &LogQuery::default())
                .await
                .expect("b")
                .len(),
            2
        );
    }

    #[tokio::test]
    async fn pruning_keeps_the_newest_lines_and_the_index_in_step() {
        let (pool, app_id) = pool_with_app("five").await;
        let logs = LogBus::new(pool.clone());
        for index in 0..10 {
            logs.append(
                &app_id,
                None,
                None,
                "runtime",
                "stdout",
                &format!("line {index}"),
            )
            .await
            .expect("append");
        }

        let removed = prune(&pool, &app_id, 4).await.expect("prune");
        assert_eq!(removed, 6);

        let kept = query(&pool, &app_id, &LogQuery::default())
            .await
            .expect("query");
        assert_eq!(kept.len(), 4);
        assert_eq!(kept[3].line.message, "line 9");

        // Pruned rows must leave the search index too.
        let stale = query(
            &pool,
            &app_id,
            &LogQuery {
                search: Some("line 0".to_string()),
                ..LogQuery::default()
            },
        )
        .await
        .expect("search");
        assert!(stale.is_empty(), "pruned lines must not match search");
    }
}
