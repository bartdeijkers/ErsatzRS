use chrono::{DateTime, Duration, Utc};
use ersatzrs_core::maintenance::scanner_library_refresh_interval;
use ersatzrs_core::playout_builder::PlayoutBuilder;
use ersatzrs_core::repositories::{PlayoutRepository, RepositoryError};
use ersatzrs_core::scheduling::{PlayoutBuildMode, SchedulingError, SchedulingRequest};
use ersatzrs_domain::{EntityId, Playout, PlayoutBuildStatus, PlayoutScheduleKind};
use reqwest::StatusCode;
use serde::Serialize;
use sqlx::{Pool, Row};
use thiserror::Error;
use tracing::{debug, warn};

use crate::cache_cleanup::{
    clean_caches, CacheCleanupConfig, CacheCleanupError, CacheCleanupReport,
};
use crate::db::{DatabaseSchemaStatus, Db};
use crate::health::{run_and_persist_health_checks, PersistedHealthCheckRun};
use crate::repositories::{SqlitePlayoutRepository, SqliteSchedulingContentRepository};

const CONFIG_KEY_SCANNER_LIBRARY_REFRESH_INTERVAL: &str = "Scanner.LibraryRefreshInterval";
const LOOP_RUN_PREFIX: &str = "maintenance.loop.";
const PROVIDER_CONNECTIVITY_KEY: &str = "maintenance.provider_connectivity";
const LIBRARY_DISPATCH_KEY: &str = "maintenance.library_refresh.last_dispatch";
const SEARCH_INDEX_DISPATCH_KEY: &str = "maintenance.search_index.last_dispatch";

/// One complete scheduler pass summary.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct MaintenancePassReport {
    /// Playouts successfully built.
    pub playouts_built: usize,
    /// Playouts skipped because a channel lock marker is present.
    pub playouts_skipped_locked: usize,
    /// Playouts whose build failed.
    pub playouts_failed: usize,
    /// Libraries due for refresh and dispatched.
    pub libraries_dispatched: usize,
    /// Provider endpoints polled.
    pub providers_polled: usize,
    /// Provider endpoints that responded with a reachable status.
    pub providers_reachable: usize,
    /// Cache cleanup result.
    pub cache_cleanup: CacheCleanupReport,
    /// Whether a search-index rebuild was dispatched.
    pub search_index_dispatched: bool,
    /// Health warning/error counts after the automatic run.
    pub health_summary: Option<ersatzrs_core::health::HealthCheckSummary>,
}

/// Errors raised by maintenance dispatch.
#[derive(Debug, Error)]
pub enum MaintenanceError {
    /// SQL query failed.
    #[error(transparent)]
    Sqlx(#[from] sqlx::Error),
    /// Repository operation failed.
    #[error(transparent)]
    Repository(#[from] RepositoryError),
    /// A playout scheduling engine failed.
    #[error(transparent)]
    Scheduling(#[from] SchedulingError),
    /// Cache cleanup failed.
    #[error(transparent)]
    Cache(#[from] CacheCleanupError),
    /// Health check persistence failed.
    #[error(transparent)]
    Health(#[from] ersatzrs_core::health::HealthCheckError),
    /// HTTP client setup failed.
    #[error(transparent)]
    Http(#[from] reqwest::Error),
}

/// Result alias for maintenance operations.
pub type Result<T> = std::result::Result<T, MaintenanceError>;

/// Run all scheduler-owned maintenance tasks once.
///
/// # Errors
///
/// Returns an error when database access, cache cleanup, or health persistence
/// fails. Individual playout build failures are recorded in
/// `PlayoutBuildStatus` and counted in the returned report.
pub async fn run_maintenance_once(
    pool: Pool<Db>,
    cache_config: &CacheCleanupConfig,
    schema_status: DatabaseSchemaStatus,
) -> Result<MaintenancePassReport> {
    let now = Utc::now();
    let mut report = MaintenancePassReport::default();

    let playout_report = build_due_playouts(&pool, now).await?;
    report.playouts_built = playout_report.built;
    report.playouts_skipped_locked = playout_report.skipped_locked;
    report.playouts_failed = playout_report.failed;
    record_loop_run(&pool, "playout", now).await?;

    report.libraries_dispatched = dispatch_due_library_refreshes(&pool, now).await?;
    record_loop_run(&pool, "library_refresh", now).await?;

    let provider_report = poll_provider_connectivity(&pool).await?;
    report.providers_polled = provider_report.polled;
    report.providers_reachable = provider_report.reachable;
    record_loop_run(&pool, "provider_poll", now).await?;

    report.cache_cleanup = clean_caches(cache_config).await?;
    record_loop_run(&pool, "cache_cleanup", now).await?;

    report.search_index_dispatched = dispatch_search_index_rebuild(&pool, now).await?;
    record_loop_run(&pool, "search_index", now).await?;

    match ersatzrs_ffmpeg::version::check_ffmpeg_runtime().await {
        Ok(runtime) => {
            let health = run_and_persist_health_checks(pool, runtime, schema_status).await?;
            report.health_summary = Some(health.summary);
        }
        Err(error) => {
            warn!(%error, "skipping automatic health run because FFmpeg runtime probing failed");
        }
    }

    Ok(report)
}

/// Load the last successful loop timestamp from `ConfigElement`.
///
/// # Errors
///
/// Returns an error when the config read fails.
pub async fn load_loop_last_run(pool: &Pool<Db>, loop_name: &str) -> Result<Option<DateTime<Utc>>> {
    let key = loop_run_key(loop_name);
    let value = sqlx::query_scalar::<_, Option<String>>(
        r#"SELECT "Value" FROM "ConfigElement" WHERE "Key" = ?"#,
    )
    .bind(key)
    .fetch_optional(pool)
    .await?
    .flatten();

    Ok(value.and_then(|value| parse_datetime(&value)))
}

/// Load the configured library refresh interval.
///
/// # Errors
///
/// Returns an error when the config read fails.
pub async fn load_library_refresh_interval(pool: &Pool<Db>) -> Result<Duration> {
    let value = sqlx::query_scalar::<_, Option<String>>(
        r#"SELECT "Value" FROM "ConfigElement" WHERE "Key" = ?"#,
    )
    .bind(CONFIG_KEY_SCANNER_LIBRARY_REFRESH_INTERVAL)
    .fetch_optional(pool)
    .await?
    .flatten();
    Ok(scanner_library_refresh_interval(value.as_deref()))
}

/// Persist an externally produced health run summary for tests and callers that
/// already probed `FFmpeg`.
///
/// # Errors
///
/// Returns an error when health checks or persistence fail.
pub async fn run_health_once(
    pool: Pool<Db>,
    runtime: ersatzrs_ffmpeg::version::FfmpegRuntimeReport,
    schema_status: DatabaseSchemaStatus,
) -> Result<PersistedHealthCheckRun> {
    Ok(run_and_persist_health_checks(pool, runtime, schema_status).await?)
}

async fn build_due_playouts(pool: &Pool<Db>, now: DateTime<Utc>) -> Result<PlayoutBuildReport> {
    let rows = sqlx::query(
        r#"SELECT "p"."Id", "p"."ChannelId"
           FROM "Playout" AS "p"
           LEFT JOIN "PlayoutBuildStatus" AS "pbs" ON "pbs"."PlayoutId" = "p"."Id"
           WHERE "p"."ScheduleKind" <> 0
             AND ("pbs"."LastBuild" IS NULL OR "pbs"."LastBuild" <= ?)
           ORDER BY "p"."Id""#,
    )
    .bind(encode_datetime(
        now - ersatzrs_core::maintenance::PLAYOUT_BUILD_INTERVAL,
    ))
    .fetch_all(pool)
    .await?;

    let mut report = PlayoutBuildReport::default();
    for row in rows {
        let playout_id = row.try_get("Id")?;
        let channel_id = row.try_get("ChannelId")?;
        if is_channel_locked(pool, channel_id).await? {
            report.skipped_locked += 1;
            continue;
        }

        match build_playout(pool, playout_id, now).await {
            Ok(()) => report.built += 1,
            Err(error) => {
                report.failed += 1;
                warn!(playout_id, %error, "playout build failed");
                let repository = SqlitePlayoutRepository(pool.clone());
                repository
                    .upsert_build_status(&PlayoutBuildStatus {
                        playout_id,
                        last_build: now,
                        success: false,
                        message: Some(error.to_string()),
                    })
                    .await?;
            }
        }
    }

    Ok(report)
}

async fn build_playout(pool: &Pool<Db>, playout_id: EntityId, now: DateTime<Utc>) -> Result<()> {
    let repository = SqlitePlayoutRepository(pool.clone());
    let scheduling = SqliteSchedulingContentRepository(pool.clone());
    let playout = repository.get_with_anchor(playout_id).await?;
    let finish = now + Duration::hours(12);
    let max_playout_offset = Duration::hours(12);

    let reference_data = match playout.schedule_kind {
        PlayoutScheduleKind::Classic => {
            scheduling
                .classic_reference_data(playout.id, now, max_playout_offset)
                .await?
        }
        PlayoutScheduleKind::Block => {
            scheduling
                .block_reference_data(playout.id, now, 1, max_playout_offset)
                .await?
        }
        PlayoutScheduleKind::Sequential => {
            let schedule_file = playout.schedule_file.as_deref().unwrap_or_default();
            scheduling
                .yaml_reference_data(playout.id, schedule_file, max_playout_offset)
                .await?
        }
        PlayoutScheduleKind::Scripted | PlayoutScheduleKind::ExternalJson => {
            ersatzrs_core::scheduling::SchedulingReferenceData::default()
        }
        PlayoutScheduleKind::None => return Ok(()),
    };

    let result = PlayoutBuilder::default().build(SchedulingRequest {
        playout: playout.clone(),
        start: now,
        finish,
        mode: PlayoutBuildMode::Continue,
        reference_data,
    })?;
    repository.apply_build_result(&result, &playout).await?;
    repository
        .upsert_build_status(&PlayoutBuildStatus {
            playout_id,
            last_build: now,
            success: true,
            message: None,
        })
        .await?;
    Ok(())
}

async fn dispatch_due_library_refreshes(pool: &Pool<Db>, now: DateTime<Utc>) -> Result<usize> {
    let interval = load_library_refresh_interval(pool).await?;
    let cutoff = now - interval;
    let rows = sqlx::query(
        r#"SELECT "l"."Id"
           FROM "Library" AS "l"
           WHERE "l"."LastScan" IS NULL OR "l"."LastScan" <= ?
           ORDER BY "l"."Id""#,
    )
    .bind(encode_datetime(cutoff))
    .fetch_all(pool)
    .await?;

    let ids = rows
        .iter()
        .map(|row| row.try_get::<EntityId, _>("Id"))
        .collect::<std::result::Result<Vec<_>, _>>()?;
    upsert_config(pool, LIBRARY_DISPATCH_KEY, &encode_datetime(now)).await?;
    upsert_config(
        pool,
        "maintenance.library_refresh.dispatched_ids",
        &serde_json::to_string(&ids).unwrap_or_else(|_| "[]".to_owned()),
    )
    .await?;
    Ok(ids.len())
}

async fn poll_provider_connectivity(pool: &Pool<Db>) -> Result<ProviderPollReport> {
    let endpoints = provider_endpoints(pool).await?;
    let client = reqwest::Client::builder()
        .timeout(std::time::Duration::from_secs(5))
        .build()?;

    let mut results = Vec::with_capacity(endpoints.len());
    for endpoint in endpoints {
        let reachable = match client.get(&endpoint.url).send().await {
            Ok(response) => provider_status_is_reachable(response.status()),
            Err(error) => {
                debug!(
                    provider = endpoint.provider,
                    media_source_id = endpoint.media_source_id,
                    %error,
                    "provider connectivity poll failed"
                );
                false
            }
        };
        results.push(ProviderConnectivityResult {
            provider: endpoint.provider,
            media_source_id: endpoint.media_source_id,
            url: endpoint.url,
            reachable,
        });
    }

    let value = serde_json::to_string(&results).unwrap_or_else(|_| "[]".to_owned());
    upsert_config(pool, PROVIDER_CONNECTIVITY_KEY, &value).await?;
    Ok(ProviderPollReport {
        polled: results.len(),
        reachable: results.iter().filter(|result| result.reachable).count(),
    })
}

async fn dispatch_search_index_rebuild(pool: &Pool<Db>, now: DateTime<Utc>) -> Result<bool> {
    let count = sqlx::query_scalar::<_, i64>(r#"SELECT COUNT(*) FROM "MediaItem""#)
        .fetch_one(pool)
        .await?;
    upsert_config(pool, SEARCH_INDEX_DISPATCH_KEY, &encode_datetime(now)).await?;
    upsert_config(
        pool,
        "maintenance.search_index.dispatched_media_items",
        &count.to_string(),
    )
    .await?;
    Ok(count > 0)
}

async fn provider_endpoints(pool: &Pool<Db>) -> Result<Vec<ProviderEndpoint>> {
    let mut endpoints = Vec::new();
    append_endpoints(
        pool,
        &mut endpoints,
        "plex",
        r#"SELECT "PlexMediaSourceId" AS "MediaSourceId", "Uri" AS "Url"
           FROM "PlexConnection"
           WHERE "IsActive" = 1 AND "Uri" IS NOT NULL
           ORDER BY "Id""#,
    )
    .await?;
    append_endpoints(
        pool,
        &mut endpoints,
        "jellyfin",
        r#"SELECT "JellyfinMediaSourceId" AS "MediaSourceId", "Address" AS "Url"
           FROM "JellyfinConnection"
           WHERE "Address" IS NOT NULL
           ORDER BY "Id""#,
    )
    .await?;
    append_endpoints(
        pool,
        &mut endpoints,
        "emby",
        r#"SELECT "EmbyMediaSourceId" AS "MediaSourceId", "Address" AS "Url"
           FROM "EmbyConnection"
           WHERE "Address" IS NOT NULL
           ORDER BY "Id""#,
    )
    .await?;
    Ok(endpoints)
}

async fn append_endpoints(
    pool: &Pool<Db>,
    endpoints: &mut Vec<ProviderEndpoint>,
    provider: &'static str,
    sql: &str,
) -> Result<()> {
    for row in sqlx::query(sql).fetch_all(pool).await? {
        let Some(url) = row.try_get::<Option<String>, _>("Url")? else {
            continue;
        };
        endpoints.push(ProviderEndpoint {
            provider,
            media_source_id: row.try_get("MediaSourceId")?,
            url,
        });
    }
    Ok(())
}

async fn is_channel_locked(pool: &Pool<Db>, channel_id: EntityId) -> Result<bool> {
    let lock_keys = [
        format!("maintenance.channel_lock.{channel_id}"),
        format!("channel.lock.{channel_id}"),
    ];
    for key in lock_keys {
        let value = sqlx::query_scalar::<_, Option<String>>(
            r#"SELECT "Value" FROM "ConfigElement" WHERE "Key" = ?"#,
        )
        .bind(key)
        .fetch_optional(pool)
        .await?
        .flatten();
        if value
            .as_deref()
            .is_some_and(|value| matches!(value, "1" | "true" | "True" | "TRUE"))
        {
            return Ok(true);
        }
    }
    Ok(false)
}

async fn record_loop_run(pool: &Pool<Db>, loop_name: &str, now: DateTime<Utc>) -> Result<()> {
    upsert_config(pool, &loop_run_key(loop_name), &encode_datetime(now)).await
}

async fn upsert_config(pool: &Pool<Db>, key: &str, value: &str) -> Result<()> {
    sqlx::query(r#"REPLACE INTO "ConfigElement" ("Key", "Value") VALUES (?, ?)"#)
        .bind(key)
        .bind(value)
        .execute(pool)
        .await?;
    Ok(())
}

fn provider_status_is_reachable(status: StatusCode) -> bool {
    status.is_success() || status == StatusCode::UNAUTHORIZED || status == StatusCode::FORBIDDEN
}

fn loop_run_key(loop_name: &str) -> String {
    format!("{LOOP_RUN_PREFIX}{loop_name}.last_run")
}

fn encode_datetime(value: DateTime<Utc>) -> String {
    value.to_rfc3339_opts(chrono::SecondsFormat::Micros, true)
}

fn parse_datetime(value: &str) -> Option<DateTime<Utc>> {
    DateTime::parse_from_rfc3339(value)
        .ok()
        .map(|value| value.with_timezone(&Utc))
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
struct PlayoutBuildReport {
    built: usize,
    skipped_locked: usize,
    failed: usize,
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct ProviderEndpoint {
    provider: &'static str,
    media_source_id: EntityId,
    url: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
struct ProviderConnectivityResult {
    provider: &'static str,
    media_source_id: EntityId,
    url: String,
    reachable: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
struct ProviderPollReport {
    polled: usize,
    reachable: usize,
}

#[allow(dead_code)]
fn _assert_playout_send_sync(_: &Playout) {}
