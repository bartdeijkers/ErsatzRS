use std::sync::Arc;

use chrono::{DateTime, Duration, NaiveTime, TimeZone, Utc};
use ersatzrs_core::hls::HlsSessionRegistry;
use ersatzrs_core::locking::{EntityLocker, PlayoutLockGuard};
use ersatzrs_core::maintenance::{
    scanner_library_refresh_interval, MaintenanceLoop, ScheduledLibraryScanRequester,
    ScheduledScanRequestOutcome, CACHE_CLEANUP_INTERVAL, PLAYOUT_BUILD_INTERVAL,
    PROVIDER_POLL_INTERVAL, SEARCH_INDEX_REBUILD_INTERVAL,
};
use ersatzrs_core::playout_builder::PlayoutBuilder;
use ersatzrs_core::repositories::{PlayoutRepository, RepositoryError};
use ersatzrs_core::scheduling::{
    audit_classic_refresh_checkpoint, audit_classic_runtime_state, PlayoutBuildMode,
    PlayoutBuildRequest, SchedulingError, SchedulingReferenceData, SchedulingRequest,
    SchedulingResult,
};
use ersatzrs_domain::{ChannelPlayoutMode, EntityId, PlayoutBuildStatus, PlayoutScheduleKind};
use reqwest::StatusCode;
use serde::{Deserialize, Serialize};
use sqlx::{Pool, Row};
use thiserror::Error;
use tracing::{debug, warn};

use crate::cache_cleanup::{
    clean_caches, CacheCleanupConfig, CacheCleanupError, CacheCleanupReport,
};
use crate::db::{DatabaseSchemaStatus, Db};
use crate::health::{run_and_persist_health_checks, PersistedHealthCheckRun};
use crate::repositories::common::{decode_datetime, decode_enum_column, encode_datetime};
use crate::repositories::{SqlitePlayoutRepository, SqliteSchedulingContentRepository};

const CONFIG_KEY_LIBRARY_REFRESH_INTERVAL: &str = "scanner.library_refresh_interval";
const CONFIG_KEY_PLAYOUT_DAYS_TO_BUILD: &str = "playout.days_to_build";
const LOOP_RUN_PREFIX: &str = "maintenance.loop.";
/// `ConfigElement` key holding the latest provider connectivity poll results.
pub const PROVIDER_CONNECTIVITY_KEY: &str = "maintenance.provider_connectivity";
const LIBRARY_DISPATCH_KEY: &str = "maintenance.library_refresh.last_dispatch";
const LIBRARY_DISPATCHED_IDS_KEY: &str = "maintenance.library_refresh.dispatched_ids";
const SEARCH_INDEX_DISPATCH_KEY: &str = "maintenance.search_index.last_dispatch";
const SEARCH_INDEX_COUNT_KEY: &str = "maintenance.search_index.dispatched_media_items";
const DEFAULT_PLAYOUT_DAYS_TO_BUILD: i64 = 2;
const DAILY_REBUILD_TOLERANCE: Duration = Duration::minutes(5);

/// One complete scheduler pass summary.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct MaintenancePassReport {
    /// Playouts successfully built.
    pub playouts_built: usize,
    /// Playouts skipped because their `EntityLocker` lock is currently held
    /// by another in-process worker.
    pub playouts_skipped_locked: usize,
    /// Playouts whose build failed.
    pub playouts_failed: usize,
    /// Playouts queued for `PlayoutBuildMode::Reset` because their
    /// `DailyRebuildTime` is within the tolerance window.
    pub playouts_reset: usize,
    /// Due libraries accepted by the shared application scan queue.
    pub libraries_dispatched: usize,
    /// Due libraries rejected by application validation or an unavailable
    /// worker. The detailed reason is logged.
    pub libraries_failed: usize,
    /// Provider endpoints polled.
    pub providers_polled: usize,
    /// Provider endpoints that responded with a reachable status.
    pub providers_reachable: usize,
    /// Cache cleanup result.
    pub cache_cleanup: CacheCleanupReport,
    /// Whether a search-index rebuild was dispatched.
    pub search_index_dispatched: bool,
    /// Bitmask of loops that actually ran during this pass.
    pub loops_ran: MaintenanceLoopFlags,
    /// Health warning/error counts after the automatic run.
    pub health_summary: Option<ersatzrs_core::health::HealthCheckSummary>,
}

/// Set of loops that executed during a [`run_maintenance_once`] pass.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct MaintenanceLoopFlags(u8);

impl MaintenanceLoopFlags {
    const PLAYOUT: u8 = 1 << 0;
    const LIBRARY_REFRESH: u8 = 1 << 1;
    const PROVIDER_POLL: u8 = 1 << 2;
    const CACHE_CLEANUP: u8 = 1 << 3;
    const SEARCH_INDEX: u8 = 1 << 4;

    fn set(&mut self, mask: u8) {
        self.0 |= mask;
    }

    fn contains(self, mask: u8) -> bool {
        self.0 & mask == mask
    }

    /// Whether the playout build loop ran.
    #[must_use]
    pub fn playout(self) -> bool {
        self.contains(Self::PLAYOUT)
    }

    /// Whether the library refresh dispatcher ran.
    #[must_use]
    pub fn library_refresh(self) -> bool {
        self.contains(Self::LIBRARY_REFRESH)
    }

    /// Whether the provider connectivity poll ran.
    #[must_use]
    pub fn provider_poll(self) -> bool {
        self.contains(Self::PROVIDER_POLL)
    }

    /// Whether the cache cleanup ran.
    #[must_use]
    pub fn cache_cleanup(self) -> bool {
        self.contains(Self::CACHE_CLEANUP)
    }

    /// Whether the search-index dispatcher ran.
    #[must_use]
    pub fn search_index(self) -> bool {
        self.contains(Self::SEARCH_INDEX)
    }
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

/// Run all scheduler-owned maintenance tasks once, gating each loop on its own
/// cadence stored in `ConfigElement`.
///
/// # Errors
///
/// Returns an error when database access, cache cleanup, or health persistence
/// fails. Individual playout build failures are recorded in
/// `PlayoutBuildStatus` and counted in the returned report.
pub async fn run_maintenance_once(
    pool: Pool<Db>,
    cache_config: &CacheCleanupConfig,
    library_scans: Option<&dyn ScheduledLibraryScanRequester>,
    locker: &EntityLocker,
    schema_status: DatabaseSchemaStatus,
    hls_sessions: Option<Arc<dyn HlsSessionRegistry>>,
) -> Result<MaintenancePassReport> {
    let now = Utc::now();
    let library_refresh_interval = load_library_refresh_interval(&pool).await?;
    let mut report = MaintenancePassReport::default();

    if loop_due(&pool, "playout", PLAYOUT_BUILD_INTERVAL, now).await? {
        let playout_report =
            build_due_playouts(&pool, locker, now, hls_sessions.as_deref()).await?;
        report.playouts_built = playout_report.built;
        report.playouts_skipped_locked = playout_report.skipped_locked;
        report.playouts_failed = playout_report.failed;
        report.playouts_reset = playout_report.reset_queued;
        report.loops_ran.set(MaintenanceLoopFlags::PLAYOUT);
        record_loop_run(&pool, "playout", now).await?;
    }

    if loop_due(&pool, "library_refresh", library_refresh_interval, now).await? {
        let library_report =
            dispatch_due_library_refreshes(&pool, library_refresh_interval, library_scans, now)
                .await?;
        report.libraries_dispatched = library_report.completed;
        report.libraries_failed = library_report.failed;
        report.loops_ran.set(MaintenanceLoopFlags::LIBRARY_REFRESH);
        record_loop_run(&pool, "library_refresh", now).await?;
    }

    if loop_due(&pool, "provider_poll", PROVIDER_POLL_INTERVAL, now).await? {
        let provider_report = poll_provider_connectivity(&pool).await?;
        report.providers_polled = provider_report.polled;
        report.providers_reachable = provider_report.reachable;
        report.loops_ran.set(MaintenanceLoopFlags::PROVIDER_POLL);
        record_loop_run(&pool, "provider_poll", now).await?;
    }

    if loop_due(&pool, "cache_cleanup", CACHE_CLEANUP_INTERVAL, now).await? {
        report.cache_cleanup =
            clean_caches(cache_config, &channel_guide_keepers(&pool).await?).await?;
        report.loops_ran.set(MaintenanceLoopFlags::CACHE_CLEANUP);
        record_loop_run(&pool, "cache_cleanup", now).await?;
    }

    if loop_due(&pool, "search_index", SEARCH_INDEX_REBUILD_INTERVAL, now).await? {
        report.search_index_dispatched = dispatch_search_index_rebuild(&pool, now).await?;
        report.loops_ran.set(MaintenanceLoopFlags::SEARCH_INDEX);
        record_loop_run(&pool, "search_index", now).await?;
    }

    let ffmpeg_check_config = ersatzrs_ffmpeg::FfmpegRuntimeCheckConfig::with_paths(
        ersatzrs_ffmpeg::ffmpeg_path(),
        ersatzrs_ffmpeg::ffprobe_path(),
    );
    match ersatzrs_ffmpeg::check_ffmpeg_runtime_with_config(&ffmpeg_check_config).await {
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

    Ok(value.and_then(|value| decode_datetime("maintenance loop last_run", &value).ok()))
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
    .bind(CONFIG_KEY_LIBRARY_REFRESH_INTERVAL)
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

/// Run one externally requested playout build through the scheduler-owned
/// build path.
///
/// This maps ErsatzTV's `WorkerService.cs` handling of `BuildPlayout` onto the
/// ErsatzRS scheduler loop: callers send a request over an in-process channel,
/// but the scheduler remains the only playout builder and reuses the same
/// `EntityLocker` as the periodic maintenance pass.
///
/// Returns `Ok(true)` when the build ran, `Ok(false)` when the playout lock was
/// already held, and `Err` when loading, building, or persistence failed.
///
/// # Errors
///
/// Returns an error when playout loading, scheduling reference-data loading,
/// build generation, result persistence, or failure-status persistence fails.
pub async fn run_requested_playout_build(
    pool: &Pool<Db>,
    locker: &EntityLocker,
    request: PlayoutBuildRequest,
    now: DateTime<Utc>,
    hls_sessions: Option<&dyn HlsSessionRegistry>,
) -> Result<bool> {
    let Some(_guard) = PlayoutLockGuard::try_acquire(locker, request.playout_id) else {
        return Ok(false);
    };

    match build_playout(pool, request.playout_id, request.mode, now, hls_sessions).await {
        Ok(()) => Ok(true),
        Err(error) => {
            let message = error.to_string();
            let repository = SqlitePlayoutRepository(pool.clone());
            repository
                .upsert_build_status(&PlayoutBuildStatus {
                    playout_id: request.playout_id,
                    last_build: now,
                    success: false,
                    message: Some(message),
                })
                .await?;
            Err(error)
        }
    }
}

/// Repair Classic playouts whose persisted enumerator state cannot safely continue.
///
/// The audit is intentionally run before the normal cadence gate so an upgraded
/// installation does not keep serving a corrupted future horizon until the next
/// periodic build.
///
/// # Errors
///
/// Returns an error when state auditing or failure-status persistence fails.
/// Individual Refresh failures are recorded and do not prevent the scheduler
/// from repairing other playouts or starting its normal cadence.
pub async fn repair_invalid_classic_playouts(
    pool: &Pool<Db>,
    locker: &EntityLocker,
    now: DateTime<Utc>,
    hls_sessions: Option<&dyn HlsSessionRegistry>,
) -> Result<u32> {
    let rows = sqlx::query(
        r#"SELECT "p"."Id", "p"."ScheduleKind"
           FROM "Playout" AS "p"
           WHERE EXISTS (
                 SELECT 1 FROM "PlayoutItem" AS "pi" WHERE "pi"."PlayoutId" = "p"."Id"
             )
           ORDER BY "p"."Id""#,
    )
    .fetch_all(pool)
    .await?;
    let mut playout_ids = Vec::new();
    for row in rows {
        if matches!(
            decode_enum_column::<PlayoutScheduleKind>(
                "playout schedule_kind",
                &row,
                "ScheduleKind"
            ),
            Ok(PlayoutScheduleKind::Classic)
        ) {
            playout_ids.push(row.try_get("Id")?);
        }
    }

    let mut repaired = 0u32;
    for playout_id in playout_ids {
        let repository = SqlitePlayoutRepository(pool.clone());
        let playout = repository.get_with_anchor(playout_id).await?;
        let max_playout_offset = load_max_mirror_playout_offset(pool, playout.channel_id).await?;
        let reference_data = SqliteSchedulingContentRepository::new(pool.clone())
            .classic_reference_data(playout_id, now, max_playout_offset)
            .await?;
        let runtime_complete =
            classic_runtime_state_is_complete(pool, playout_id, &reference_data).await?;
        let refresh_audit = audit_classic_refresh_checkpoint(&reference_data, playout_id, now);
        if runtime_complete && refresh_audit.is_complete() {
            continue;
        }

        if let Some(diagnostic) = refresh_audit.diagnostic {
            warn!(playout_id, %diagnostic, "refusing unsafe Classic startup repair");
            repository
                .upsert_build_status(&PlayoutBuildStatus {
                    playout_id,
                    last_build: now,
                    success: false,
                    message: Some(diagnostic),
                })
                .await?;
            continue;
        }

        let Some(_guard) = PlayoutLockGuard::try_acquire(locker, playout_id) else {
            debug!(
                playout_id,
                "Classic runtime-state repair skipped because playout is locked"
            );
            continue;
        };
        warn!(
            playout_id,
            "repairing inconsistent Classic runtime state with Refresh"
        );
        if let Err(error) = build_playout(
            pool,
            playout_id,
            PlayoutBuildMode::Refresh,
            now,
            hls_sessions,
        )
        .await
        {
            warn!(playout_id, %error, "Classic runtime-state repair failed");
            repository
                .upsert_build_status(&PlayoutBuildStatus {
                    playout_id,
                    last_build: now,
                    success: false,
                    message: Some(error.to_string()),
                })
                .await?;
            continue;
        }
        repaired = repaired.saturating_add(1);
    }
    Ok(repaired)
}

async fn loop_due(
    pool: &Pool<Db>,
    loop_name: &'static str,
    interval: Duration,
    now: DateTime<Utc>,
) -> Result<bool> {
    let last_run = load_loop_last_run(pool, loop_name).await?;
    let definition = MaintenanceLoop {
        name: loop_name,
        interval,
    };
    Ok(definition.is_due(last_run, now))
}

async fn build_due_playouts(
    pool: &Pool<Db>,
    locker: &EntityLocker,
    now: DateTime<Utc>,
    hls_sessions: Option<&dyn HlsSessionRegistry>,
) -> Result<PlayoutBuildReport> {
    let cutoff = encode_datetime(now - PLAYOUT_BUILD_INTERVAL);
    let rows = sqlx::query(
        r#"SELECT "p"."Id", "p"."DailyRebuildTime", "c"."Number"
           FROM "Playout" AS "p"
           JOIN "Channel" AS "c" ON "c"."Id" = "p"."ChannelId"
           LEFT JOIN "PlayoutBuildStatus" AS "pbs" ON "pbs"."PlayoutId" = "p"."Id"
           WHERE "p"."ScheduleKind" <> 0
             AND ("pbs"."LastBuild" IS NULL OR "pbs"."LastBuild" <= ?)"#,
    )
    .bind(cutoff)
    .fetch_all(pool)
    .await?;

    let mut due: Vec<DuePlayout> = rows
        .into_iter()
        .map(|row| {
            let number = row
                .try_get::<Option<String>, _>("Number")?
                .unwrap_or_default();
            Ok::<_, MaintenanceError>(DuePlayout {
                playout_id: row.try_get("Id")?,
                daily_rebuild_time: row.try_get::<Option<String>, _>("DailyRebuildTime")?,
                channel_number: number,
            })
        })
        .collect::<std::result::Result<_, _>>()?;
    due.sort_by(|a, b| compare_channel_numbers(&a.channel_number, &b.channel_number));

    let mut report = PlayoutBuildReport::default();
    for entry in due {
        // Mirror `EntityLocker.LockPlayout` from
        // `ErsatzTV.Infrastructure.Locking.EntityLocker`: skip silently when
        // another worker already owns this playout's lock.
        let Some(_guard) = PlayoutLockGuard::try_acquire(locker, entry.playout_id) else {
            report.skipped_locked += 1;
            continue;
        };

        let mode = if daily_rebuild_due(entry.daily_rebuild_time.as_deref(), now) {
            report.reset_queued += 1;
            PlayoutBuildMode::Reset
        } else {
            PlayoutBuildMode::Continue
        };

        match build_playout(pool, entry.playout_id, mode, now, hls_sessions).await {
            Ok(()) => report.built += 1,
            Err(error) => {
                report.failed += 1;
                warn!(playout_id = entry.playout_id, %error, "playout build failed");
                let repository = SqlitePlayoutRepository(pool.clone());
                repository
                    .upsert_build_status(&PlayoutBuildStatus {
                        playout_id: entry.playout_id,
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

async fn build_playout(
    pool: &Pool<Db>,
    playout_id: EntityId,
    mut mode: PlayoutBuildMode,
    now: DateTime<Utc>,
    hls_sessions: Option<&dyn HlsSessionRegistry>,
) -> Result<()> {
    let repository = SqlitePlayoutRepository(pool.clone());
    let scheduling = SqliteSchedulingContentRepository::new(pool.clone());
    let mut playout = repository.get_with_anchor(playout_id).await?;
    // Mirror ErsatzTV's `ResetPlayout(): playout.Seed = new Random().Next()`.
    // Without this, a Reset reshuffles from the same seed the playout has had
    // since creation, so every rebuild reproduces the identical schedule-item
    // and collection shuffle order forever.
    if mode == PlayoutBuildMode::Reset {
        playout.seed = ersatzrs_core::scheduling::next_playout_seed();
    }
    let days = load_playout_days_to_build(pool).await?;
    let window = Duration::days(days);
    let finish = now + window;

    let max_playout_offset = load_max_mirror_playout_offset(pool, playout.channel_id).await?;
    let mut reference_data = match playout.schedule_kind {
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
        PlayoutScheduleKind::Scripted => {
            let (definition_policies, definition_legacy_policy) = playout
                .schedule_file
                .as_deref()
                .and_then(|path| std::fs::read_to_string(path).ok())
                .and_then(|script| {
                    ersatzrs_core::scheduling::ScriptedSchedulingEngine::smart_shuffle_policies_from_script(
                        &script,
                    )
                    .ok()
                })
                .unwrap_or_default();
            scheduling
                .scripted_reference_data(playout.id, definition_policies, definition_legacy_policy)
                .await?
        }
        PlayoutScheduleKind::ExternalJson => scheduling.configured_playout_options().await?,
        PlayoutScheduleKind::None => return Ok(()),
    };
    reference_data.max_playout_offset = max_playout_offset;

    mode = safe_classic_build_mode(
        pool,
        playout.id,
        playout.schedule_kind,
        mode,
        now,
        &reference_data,
    )
    .await?;

    let result = PlayoutBuilder::default().build(SchedulingRequest {
        playout: playout.clone(),
        start: now,
        finish,
        mode,
        reference_data,
    })?;
    let channel_info = load_playout_channel_info(pool, playout.channel_id).await?;
    let should_notify_segmenter = mode != PlayoutBuildMode::Continue
        && (scheduling_result_has_persisted_changes(&result)
            || channel_info.playout_mode == ChannelPlayoutMode::OnDemand);
    repository
        .apply_build_result(&result, &playout, max_playout_offset)
        .await?;
    repository
        .upsert_build_status(&PlayoutBuildStatus {
            playout_id,
            last_build: now,
            success: true,
            message: None,
        })
        .await?;
    if let Err(error) = crate::subtitles::extract_embedded_playout_subtitles(pool, playout_id).await
    {
        warn!(
            playout_id,
            %error,
            "embedded subtitle extraction failed after playout build"
        );
    }
    if should_notify_segmenter {
        if let Some(hls_sessions) = hls_sessions {
            for channel_number in
                load_playout_dependent_channel_numbers(pool, playout.channel_id).await?
            {
                hls_sessions.playout_updated(&channel_number).await;
            }
        }
    }
    Ok(())
}

async fn safe_classic_build_mode(
    pool: &Pool<Db>,
    playout_id: EntityId,
    schedule_kind: PlayoutScheduleKind,
    requested_mode: PlayoutBuildMode,
    now: DateTime<Utc>,
    reference_data: &SchedulingReferenceData,
) -> Result<PlayoutBuildMode> {
    if schedule_kind != PlayoutScheduleKind::Classic || reference_data.existing_items.is_empty() {
        return Ok(requested_mode);
    }

    if requested_mode == PlayoutBuildMode::Refresh {
        require_safe_classic_refresh(pool, playout_id, now, reference_data).await?;
        return Ok(requested_mode);
    }
    if requested_mode != PlayoutBuildMode::Continue
        || classic_runtime_state_is_complete(pool, playout_id, reference_data).await?
    {
        return Ok(requested_mode);
    }

    require_safe_classic_refresh(pool, playout_id, now, reference_data).await?;

    let audit = audit_classic_runtime_state(reference_data);
    warn!(
        playout_id,
        expected_anchors = audit.expected_anchor_count,
        actual_anchors = audit.actual_anchor_count,
        expected_fill_groups = audit.expected_fill_group_count,
        actual_fill_groups = audit.actual_fill_group_count,
        "promoting Classic Continue build to Refresh because runtime state is inconsistent"
    );
    Ok(PlayoutBuildMode::Refresh)
}

async fn require_safe_classic_refresh(
    pool: &Pool<Db>,
    playout_id: EntityId,
    now: DateTime<Utc>,
    reference_data: &SchedulingReferenceData,
) -> Result<()> {
    let audit = audit_classic_refresh_checkpoint(reference_data, playout_id, now);
    let Some(checkpoint_date) = audit.checkpoint_date else {
        return Err(
            SchedulingError::UnsafeClassicRefresh(audit.diagnostic.unwrap_or_else(|| {
                "Classic Refresh refused because no dated checkpoint is available".to_owned()
            }))
            .into(),
        );
    };
    if let Some(diagnostic) = audit.diagnostic {
        return Err(SchedulingError::UnsafeClassicRefresh(diagnostic).into());
    }

    let missing_child_states: i64 = sqlx::query_scalar(
        r#"SELECT COUNT(*)
           FROM "PlayoutProgramScheduleAnchor" AS "a"
           LEFT JOIN "CollectionEnumeratorState" AS "s"
             ON "s"."PlayoutProgramScheduleAnchorId" = "a"."Id"
           WHERE "a"."PlayoutId" = ?
             AND "a"."AnchorDate" = ?
             AND "s"."PlayoutProgramScheduleAnchorId" IS NULL"#,
    )
    .bind(playout_id)
    .bind(encode_datetime(checkpoint_date))
    .fetch_one(pool)
    .await?;
    if missing_child_states != 0 {
        return Err(SchedulingError::UnsafeClassicRefresh(
            "Classic Refresh refused: the selected dated checkpoint has missing enumerator state. Use Reset Playout once to create a complete checkpoint before retrying Refresh".to_owned(),
        )
        .into());
    }
    Ok(())
}

pub(crate) async fn classic_runtime_state_is_complete(
    pool: &Pool<Db>,
    playout_id: EntityId,
    reference_data: &SchedulingReferenceData,
) -> Result<bool> {
    let audit = audit_classic_runtime_state(reference_data);
    if !audit.is_complete() {
        return Ok(false);
    }

    let missing_child_states: i64 = sqlx::query_scalar(
        r#"SELECT
             (SELECT COUNT(*)
                FROM "PlayoutProgramScheduleAnchor" AS "a"
                LEFT JOIN "CollectionEnumeratorState" AS "s"
                  ON "s"."PlayoutProgramScheduleAnchorId" = "a"."Id"
               WHERE "a"."PlayoutId" = ?
                 AND "s"."PlayoutProgramScheduleAnchorId" IS NULL)
             +
             (SELECT COUNT(*)
                FROM "PlayoutScheduleItemFillGroupIndex" AS "i"
                LEFT JOIN "FillGroupEnumeratorState" AS "s"
                  ON "s"."PlayoutScheduleItemFillGroupIndexId" = "i"."Id"
               WHERE "i"."PlayoutId" = ?
                 AND "s"."PlayoutScheduleItemFillGroupIndexId" IS NULL)"#,
    )
    .bind(playout_id)
    .bind(playout_id)
    .fetch_one(pool)
    .await?;
    Ok(missing_child_states == 0)
}

async fn load_max_mirror_playout_offset(
    pool: &Pool<Db>,
    source_channel_id: EntityId,
) -> Result<Duration> {
    let values = sqlx::query_scalar::<_, Option<String>>(
        r#"SELECT "PlayoutOffset"
           FROM "Channel"
           WHERE "PlayoutSource" = 1 AND "MirrorSourceChannelId" = ?"#,
    )
    .bind(source_channel_id)
    .fetch_all(pool)
    .await?;
    let mut maximum = Duration::zero();
    for value in values.into_iter().flatten() {
        let offset = crate::repositories::common::try_decode_signed_duration_text(&value)
            .map_err(MaintenanceError::Repository)?;
        if offset > maximum {
            maximum = offset;
        }
    }
    Ok(maximum)
}

async fn load_playout_dependent_channel_numbers(
    pool: &Pool<Db>,
    source_channel_id: EntityId,
) -> Result<Vec<String>> {
    Ok(sqlx::query_scalar::<_, Option<String>>(
        r#"SELECT "Number" FROM "Channel"
           WHERE "Id" = ?
              OR ("PlayoutSource" = 1 AND "MirrorSourceChannelId" = ?)
           ORDER BY "SortNumber", "Id""#,
    )
    .bind(source_channel_id)
    .bind(source_channel_id)
    .fetch_all(pool)
    .await?
    .into_iter()
    .flatten()
    .collect())
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct PlayoutChannelInfo {
    channel_number: String,
    playout_mode: ChannelPlayoutMode,
}

async fn load_playout_channel_info(
    pool: &Pool<Db>,
    channel_id: EntityId,
) -> Result<PlayoutChannelInfo> {
    let row = sqlx::query(r#"SELECT "Number", "PlayoutMode" FROM "Channel" WHERE "Id" = ?"#)
        .bind(channel_id)
        .fetch_one(pool)
        .await?;
    Ok(PlayoutChannelInfo {
        channel_number: row
            .try_get::<Option<String>, _>("Number")?
            .unwrap_or_default(),
        playout_mode: match row.try_get::<i64, _>("PlayoutMode").unwrap_or_default() {
            1 => ChannelPlayoutMode::OnDemand,
            _ => ChannelPlayoutMode::Continuous,
        },
    })
}

fn scheduling_result_has_persisted_changes(result: &SchedulingResult) -> bool {
    result.clear_items
        || result.remove_before.is_some()
        || !result.added_items.is_empty()
        || !result.added_gaps.is_empty()
        || !result.added_history.is_empty()
        || !result.history_to_remove.is_empty()
        || !result.items_to_remove.is_empty()
        || result.anchor.is_some()
        || !result.program_schedule_anchors.is_empty()
        || !result.fill_group_indices.is_empty()
}

async fn dispatch_due_library_refreshes(
    pool: &Pool<Db>,
    interval: Duration,
    library_scans: Option<&dyn ScheduledLibraryScanRequester>,
    now: DateTime<Utc>,
) -> Result<LibraryDispatchReport> {
    let cutoff = encode_datetime(now - interval);
    let due_ids = sqlx::query_scalar::<_, EntityId>(
        r#"SELECT "Id"
           FROM "Library"
           WHERE "LastScan" IS NULL OR "LastScan" <= ?
           ORDER BY "Id""#,
    )
    .bind(cutoff)
    .fetch_all(pool)
    .await?;

    upsert_config(pool, LIBRARY_DISPATCH_KEY, &encode_datetime(now)).await?;
    upsert_config(
        pool,
        LIBRARY_DISPATCHED_IDS_KEY,
        &serde_json::to_string(&due_ids).unwrap_or_else(|_| "[]".to_owned()),
    )
    .await?;

    if due_ids.is_empty() {
        return Ok(LibraryDispatchReport::default());
    }

    let mut report = LibraryDispatchReport::default();
    for library_id in due_ids {
        let Some(library_scans) = library_scans else {
            warn!(
                library_id,
                "library refresh admission failed because application services are unavailable"
            );
            report.failed += 1;
            continue;
        };
        match library_scans
            .request_scheduled_library_scan(library_id)
            .await
        {
            ScheduledScanRequestOutcome::Accepted => report.completed += 1,
            ScheduledScanRequestOutcome::AlreadyActive => {
                debug!(
                    library_id,
                    "library refresh coalesced with queued or running scan"
                );
            }
            ScheduledScanRequestOutcome::Rejected { message }
            | ScheduledScanRequestOutcome::Unavailable { message } => {
                warn!(
                    library_id,
                    error = %message,
                    "library refresh admission failed"
                );
                report.failed += 1;
            }
        }
    }
    Ok(report)
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
            provider: endpoint.provider.to_owned(),
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
    upsert_config(pool, SEARCH_INDEX_COUNT_KEY, &count.to_string()).await?;
    Ok(count > 0)
}

async fn channel_guide_keepers(pool: &Pool<Db>) -> Result<Vec<String>> {
    let rows = sqlx::query_scalar::<_, Option<String>>(r#"SELECT "Number" FROM "Channel""#)
        .fetch_all(pool)
        .await?;
    Ok(rows
        .into_iter()
        .filter_map(|number| number.map(|number| format!("{number}.xml")))
        .collect())
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
    for row in sqlx::query(crate::db::assert_sql_safe(sql))
        .fetch_all(pool)
        .await?
    {
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

async fn load_playout_days_to_build(pool: &Pool<Db>) -> Result<i64> {
    let value = sqlx::query_scalar::<_, Option<String>>(
        r#"SELECT "Value" FROM "ConfigElement" WHERE "Key" = ?"#,
    )
    .bind(CONFIG_KEY_PLAYOUT_DAYS_TO_BUILD)
    .fetch_optional(pool)
    .await?
    .flatten();
    Ok(value
        .and_then(|value| value.trim().parse::<i64>().ok())
        .filter(|days| *days > 0)
        .unwrap_or(DEFAULT_PLAYOUT_DAYS_TO_BUILD))
}

fn provider_status_is_reachable(status: StatusCode) -> bool {
    status.is_success() || status == StatusCode::UNAUTHORIZED || status == StatusCode::FORBIDDEN
}

fn loop_run_key(loop_name: &str) -> String {
    format!("{LOOP_RUN_PREFIX}{loop_name}.last_run")
}

fn compare_channel_numbers(a: &str, b: &str) -> std::cmp::Ordering {
    let parsed_a = a.parse::<f64>();
    let parsed_b = b.parse::<f64>();
    match (parsed_a, parsed_b) {
        (Ok(a), Ok(b)) => a.partial_cmp(&b).unwrap_or(std::cmp::Ordering::Equal),
        (Ok(_), Err(_)) => std::cmp::Ordering::Less,
        (Err(_), Ok(_)) => std::cmp::Ordering::Greater,
        (Err(_), Err(_)) => a.cmp(b),
    }
}

fn daily_rebuild_due(value: Option<&str>, now: DateTime<Utc>) -> bool {
    let Some(value) = value else {
        return false;
    };
    let Some(rebuild_time) = parse_daily_rebuild_time(value) else {
        return false;
    };
    let local_now = chrono::Local::now();
    let target_naive = local_now.date_naive().and_time(rebuild_time);
    let Some(target_local) = local_now
        .timezone()
        .from_local_datetime(&target_naive)
        .single()
    else {
        return false;
    };
    let target = target_local.with_timezone(&Utc);
    let delta = now.signed_duration_since(target);
    delta.num_seconds().unsigned_abs() <= DAILY_REBUILD_TOLERANCE.num_seconds().unsigned_abs()
}

fn parse_daily_rebuild_time(value: &str) -> Option<NaiveTime> {
    let trimmed = value.trim();
    NaiveTime::parse_from_str(trimmed, "%H:%M:%S")
        .or_else(|_| NaiveTime::parse_from_str(trimmed, "%H:%M:%S%.f"))
        .or_else(|_| NaiveTime::parse_from_str(trimmed, "%H:%M"))
        .ok()
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
struct PlayoutBuildReport {
    built: usize,
    skipped_locked: usize,
    failed: usize,
    reset_queued: usize,
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct DuePlayout {
    playout_id: EntityId,
    daily_rebuild_time: Option<String>,
    channel_number: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct ProviderEndpoint {
    provider: &'static str,
    media_source_id: EntityId,
    url: String,
}

/// Persisted provider connectivity poll result row, written under
/// [`PROVIDER_CONNECTIVITY_KEY`] and read by the health check engine.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ProviderConnectivityResult {
    /// Provider tag: `plex`, `jellyfin`, or `emby`.
    pub provider: String,
    /// `MediaSource` row id the connection belongs to.
    pub media_source_id: EntityId,
    /// Connection URL probed by the maintenance loop.
    pub url: String,
    /// Whether the last probe responded reachably.
    pub reachable: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
struct ProviderPollReport {
    polled: usize,
    reachable: usize,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
struct LibraryDispatchReport {
    completed: usize,
    failed: usize,
}
