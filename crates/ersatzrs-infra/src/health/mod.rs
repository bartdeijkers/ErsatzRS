use std::path::Path;

use async_trait::async_trait;
use ersatzrs_core::health::{
    HealthCheck, HealthCheckError, HealthCheckResult, HealthCheckService, HealthCheckStatus,
    HealthCheckSummary, Result,
};
use ersatzrs_domain::HardwareAccelerationKind;
use ersatzrs_ffmpeg::{
    FfmpegRuntimeIssue, FfmpegRuntimeReport, HardwareCapabilitiesFactory, REQUIRED_MAJOR,
    REQUIRED_MINOR,
};
use serde::{Deserialize, Serialize};
use sqlx::{Pool, Row};

use crate::db::{DatabaseSchemaStatus, Db};
use crate::maintenance::{ProviderConnectivityResult, PROVIDER_CONNECTIVITY_KEY};
mod checks;

use checks::{
    AddonListDiskUsageHealthCheck, AddonMediaListHealthCheck, AddonPlatformHealthCheck,
    ArtworkCardinalityHealthCheck, ArtworkOrphansHealthCheck, AudioScriptHealthCheck,
    ClassicPlayoutStateHealthCheck, DirtyMigrationHealthCheck, DolbyVisionHealthCheck,
    DowngradeHealthCheck, DynamicTextGraphicsHealthCheck, EmptyScheduleHealthCheck,
    EpisodeMetadataHealthCheck, FFmpegCapabilitiesHealthCheck, FFmpegReportsHealthCheck,
    FFmpegRuntimeAcquisitionHealthCheck, FFmpegVersionHealthCheck, FileNotFoundHealthCheck,
    HardwareAccelerationHealthCheck, HardwareGraphHealthCheck, HlsDeliveryHealthCheck,
    LocalTelevisionIdentityHealthCheck, MigrationChecksumHealthCheck, MirrorChannelHealthCheck,
    MovieMetadataHealthCheck, NextRuntimeHealthCheck, OutputDynamicRangeHealthCheck,
    PinnedStreamSelectionHealthCheck, PlaybackDataAccessHealthCheck,
    ProviderConnectivityHealthCheck, RemoteStreamAddonReadinessHealthCheck,
    RemoteStreamHealthCheck, RemoteStreamPlaylistHealthCheck, RemoteStreamScheduleHealthCheck,
    ScanRefreshStateHealthCheck, SearchProjectionHealthCheck, SourceOrientationHealthCheck,
    SqliteConnectionDefaultsHealthCheck, SubtitleCleanupHealthCheck, TsduckAvailabilityHealthCheck,
    UnavailableHealthCheck, VaapiDeviceHealthCheck, VaapiDriverHealthCheck,
    ZeroDurationHealthCheck,
};

/// `ConfigElement` key containing the latest health check result set as JSON.
pub const CONFIG_KEY_HEALTH_RESULTS: &str = "health.results";
/// `ConfigElement` key containing the latest health check summary as JSON.
pub const CONFIG_KEY_HEALTH_SUMMARY: &str = "health.summary";
/// `ConfigElement` key controlling whether `ffmpeg` writes troubleshooting reports.
const CONFIG_KEY_FFMPEG_SAVE_REPORTS: &str = "ffmpeg.save_reports";

const STATUS_FILE_NOT_FOUND: i32 = 1;
const STATUS_UNAVAILABLE: i32 = 2;
const STREAMING_MODE_HLS_DIRECT: i32 = 2;
const HW_ACCEL_NONE: i32 = 0;
const HW_ACCEL_QSV: i32 = 1;
const HW_ACCEL_NVENC: i32 = 2;
const HW_ACCEL_VAAPI: i32 = 3;
const HW_ACCEL_VIDEOTOOLBOX: i32 = 4;
const HW_ACCEL_AMF: i32 = 5;
const HW_ACCEL_V4L2M2M: i32 = 6;
const HW_ACCEL_RKMPP: i32 = 7;
const VAAPI_DRIVER_AUTO: i32 = 0;

#[cfg(test)]
const REGISTERED_HEALTH_CHECK_TITLES: [&str; 44] = [
    "ErsatzTV Downgrade",
    "Database Migration",
    "Database Migration Checksum",
    "SQLite Connection Defaults",
    "Search Projection",
    "FFmpeg Version",
    "FFmpeg Runtime",
    "FFmpeg Capabilities",
    "FFmpeg Reports",
    "Dynamic Text Graphics",
    "Next Runtime Availability",
    "JavaScript Audio Selection",
    "Hardware Acceleration",
    "Hardware Graph Capability",
    "HLS Live Delivery",
    "Playback Data Access",
    "Pinned Stream Selection",
    "Movie Metadata",
    "Episode Metadata",
    "Local television identity",
    "Artwork Cardinality",
    "Artwork Orphans",
    "Subtitle Cleanup",
    "Zero Duration",
    "Remote Streams",
    "Remote Stream add-on readiness",
    "Remote Stream schedule compatibility",
    "Remote Stream playlists",
    "Add-ons",
    "Add-on media lists",
    "Add-on list disk usage",
    "File Not Found",
    "Unavailable",
    "Empty Schedule",
    "Classic Playout State",
    "Scan Refresh State",
    "Mirror Channels",
    "Output Dynamic Range",
    "Dolby Vision",
    "Source Orientation",
    "VAAPI Driver",
    "VAAPI Device",
    "Provider Connectivity",
    "TSDuck MPEG-TS Troubleshooting",
];

/// Run the Output Dynamic Range check alone (ADR 0064).
///
/// Exposed so both backends can assert the same classification results. Its
/// query uses `SELECT DISTINCT`, which `MySQL` and `SQLite` disagree about when an
/// `ORDER BY` column is absent from the projection, so the check needs coverage
/// on both rather than on whichever one the unit tests happen to use.
///
/// # Errors
///
/// Returns an error when the database read fails.
pub async fn run_output_dynamic_range_check(pool: &Pool<Db>) -> Result<HealthCheckResult> {
    checks::OutputDynamicRangeHealthCheck::new(pool.clone())
        .check()
        .await
}

/// Run the Source Orientation check alone (ADR 0003, milestone `p8c-02`).
///
/// Exposed for the same reason as the Output Dynamic Range runner: its query
/// combines `SELECT DISTINCT` with a `COALESCE` join across `MediaVersion`'s
/// per-type id columns, which is exactly the shape `MySQL` and `SQLite` have
/// disagreed about before.
///
/// # Errors
///
/// Returns an error when the database read fails.
pub async fn run_source_orientation_check(pool: &Pool<Db>) -> Result<HealthCheckResult> {
    checks::SourceOrientationHealthCheck::new(pool.clone())
        .check()
        .await
}

/// Persisted view of a health run.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct PersistedHealthCheckRun {
    /// All health check results, including not-applicable rows.
    pub results: Vec<HealthCheckResult>,
    /// Warning/failure counts from `results`.
    pub summary: HealthCheckSummary,
}

/// SQL-backed health check store using ErsatzTV-compatible `ConfigElement`.
#[derive(Clone)]
pub struct SqlHealthCheckStore {
    pool: Pool<Db>,
}

impl SqlHealthCheckStore {
    /// Create a health check store.
    #[must_use]
    pub fn new(pool: Pool<Db>) -> Self {
        Self { pool }
    }

    /// Persist a completed health run.
    ///
    /// # Errors
    ///
    /// Returns an error when JSON serialization or the `ConfigElement` write fails.
    pub async fn save_results(&self, results: &[HealthCheckResult]) -> Result<HealthCheckSummary> {
        let summary = HealthCheckSummary::from_results(results);
        let results_json = serde_json::to_string(results)
            .map_err(|error| HealthCheckError::Serialization(error.to_string()))?;
        let summary_json = serde_json::to_string(&summary)
            .map_err(|error| HealthCheckError::Serialization(error.to_string()))?;

        upsert_config(&self.pool, CONFIG_KEY_HEALTH_RESULTS, &results_json).await?;
        upsert_config(&self.pool, CONFIG_KEY_HEALTH_SUMMARY, &summary_json).await?;

        Ok(summary)
    }

    /// Load the latest persisted summary.
    ///
    /// # Errors
    ///
    /// Returns an error when the `ConfigElement` read or JSON decoding fails.
    pub async fn load_summary(&self) -> Result<HealthCheckSummary> {
        let Some(value) = load_config(&self.pool, CONFIG_KEY_HEALTH_SUMMARY).await? else {
            return Ok(HealthCheckSummary::default());
        };
        serde_json::from_str(&value)
            .map_err(|error| HealthCheckError::Serialization(error.to_string()))
    }

    /// Load the latest persisted health run.
    ///
    /// # Errors
    ///
    /// Returns an error when the `ConfigElement` read or JSON decoding fails.
    pub async fn load_results(&self) -> Result<Vec<HealthCheckResult>> {
        let Some(value) = load_config(&self.pool, CONFIG_KEY_HEALTH_RESULTS).await? else {
            return Ok(Vec::new());
        };
        serde_json::from_str(&value)
            .map_err(|error| HealthCheckError::Serialization(error.to_string()))
    }
}

/// Build the current `ErsatzTV` parity health check set.
#[must_use]
#[allow(clippy::needless_pass_by_value)]
pub fn make_health_check_service(
    pool: Pool<Db>,
    ffmpeg_runtime: FfmpegRuntimeReport,
    schema_status: DatabaseSchemaStatus,
) -> HealthCheckService {
    HealthCheckService::new(registered_health_checks(
        pool,
        ffmpeg_runtime,
        schema_status,
    ))
}

#[allow(clippy::needless_pass_by_value)]
fn registered_health_checks(
    pool: Pool<Db>,
    ffmpeg_runtime: FfmpegRuntimeReport,
    schema_status: DatabaseSchemaStatus,
) -> Vec<Box<dyn HealthCheck>> {
    let downgrade_schema_status = schema_status.clone();
    let dirty_schema_status = schema_status.clone();
    vec![
        Box::new(DowngradeHealthCheck::new(downgrade_schema_status)),
        Box::new(DirtyMigrationHealthCheck::new(dirty_schema_status)),
        Box::new(MigrationChecksumHealthCheck::new(schema_status)),
        Box::new(SqliteConnectionDefaultsHealthCheck::new(pool.clone())),
        Box::new(SearchProjectionHealthCheck::new(pool.clone())),
        Box::new(FFmpegVersionHealthCheck::new(ffmpeg_runtime.clone())),
        Box::new(FFmpegRuntimeAcquisitionHealthCheck::new(
            ffmpeg_runtime.clone(),
        )),
        Box::new(FFmpegCapabilitiesHealthCheck::new(ffmpeg_runtime.clone())),
        Box::new(FFmpegReportsHealthCheck::new(pool.clone())),
        Box::new(DynamicTextGraphicsHealthCheck::new(pool.clone())),
        Box::new(NextRuntimeHealthCheck::new(pool.clone())),
        Box::new(AudioScriptHealthCheck),
        Box::new(HardwareAccelerationHealthCheck::new(
            pool.clone(),
            ffmpeg_runtime.clone(),
        )),
        Box::new(HardwareGraphHealthCheck::new(ffmpeg_runtime.clone())),
        Box::new(HlsDeliveryHealthCheck::new(pool.clone())),
        Box::new(PlaybackDataAccessHealthCheck::new(pool.clone())),
        Box::new(PinnedStreamSelectionHealthCheck::new(pool.clone())),
        Box::new(MovieMetadataHealthCheck::new(pool.clone())),
        Box::new(EpisodeMetadataHealthCheck::new(pool.clone())),
        Box::new(LocalTelevisionIdentityHealthCheck::new(pool.clone())),
        Box::new(ArtworkCardinalityHealthCheck::new(pool.clone())),
        Box::new(ArtworkOrphansHealthCheck::new(pool.clone())),
        Box::new(SubtitleCleanupHealthCheck::new(pool.clone())),
        Box::new(ZeroDurationHealthCheck::new(pool.clone())),
        Box::new(RemoteStreamHealthCheck::new(pool.clone())),
        Box::new(RemoteStreamAddonReadinessHealthCheck::new(pool.clone())),
        Box::new(RemoteStreamScheduleHealthCheck::new(pool.clone())),
        Box::new(RemoteStreamPlaylistHealthCheck::new(pool.clone())),
        Box::new(AddonPlatformHealthCheck::new(pool.clone())),
        Box::new(AddonMediaListHealthCheck::new(pool.clone())),
        Box::new(AddonListDiskUsageHealthCheck::new(pool.clone())),
        Box::new(FileNotFoundHealthCheck::new(pool.clone())),
        Box::new(UnavailableHealthCheck::new(pool.clone())),
        Box::new(EmptyScheduleHealthCheck::new(pool.clone())),
        Box::new(ClassicPlayoutStateHealthCheck::new(pool.clone())),
        Box::new(ScanRefreshStateHealthCheck::new(pool.clone())),
        Box::new(MirrorChannelHealthCheck::new(pool.clone())),
        Box::new(OutputDynamicRangeHealthCheck::new(pool.clone())),
        Box::new(DolbyVisionHealthCheck::new(
            pool.clone(),
            ffmpeg_runtime.clone(),
        )),
        Box::new(SourceOrientationHealthCheck::new(pool.clone())),
        Box::new(VaapiDriverHealthCheck::new(
            pool.clone(),
            ffmpeg_runtime.clone(),
        )),
        Box::new(VaapiDeviceHealthCheck::new(
            pool.clone(),
            ffmpeg_runtime.clone(),
        )),
        Box::new(ProviderConnectivityHealthCheck::new(pool)),
        Box::new(TsduckAvailabilityHealthCheck::new()),
    ]
}

/// Run the parity health checks and persist the result in `ConfigElement`.
///
/// # Errors
///
/// Returns an error when the health result set cannot be persisted.
pub async fn run_and_persist_health_checks(
    pool: Pool<Db>,
    ffmpeg_runtime: FfmpegRuntimeReport,
    schema_status: DatabaseSchemaStatus,
) -> Result<PersistedHealthCheckRun> {
    let service = make_health_check_service(pool.clone(), ffmpeg_runtime, schema_status);
    let results = service.perform_health_checks().await;
    let store = SqlHealthCheckStore::new(pool);
    let summary = store.save_results(&results).await?;
    Ok(PersistedHealthCheckRun { results, summary })
}

async fn source_name(
    pool: &Pool<Db>,
    provider: &str,
    id: ersatzrs_domain::EntityId,
) -> Result<Option<String>> {
    let sql = match provider {
        "plex" => r#"SELECT "ServerName" AS "Name" FROM "PlexMediaSource" WHERE "Id" = ?"#,
        "jellyfin" => r#"SELECT "ServerName" AS "Name" FROM "JellyfinMediaSource" WHERE "Id" = ?"#,
        "emby" => r#"SELECT "ServerName" AS "Name" FROM "EmbyMediaSource" WHERE "Id" = ?"#,
        _ => return Ok(None),
    };
    let row = sqlx::query(crate::db::assert_sql_safe(sql))
        .bind(id)
        .fetch_optional(pool)
        .await
        .map_err(sqlx_error)?;
    Ok(row.and_then(|row| row.try_get::<Option<String>, _>("Name").ok().flatten()))
}

fn provider_label(provider: &str) -> &str {
    match provider {
        "plex" => "Plex",
        "jellyfin" => "Jellyfin",
        "emby" => "Emby",
        other => other,
    }
}

async fn media_state_result(
    pool: &Pool<Db>,
    title: &str,
    state: i32,
    message_template: &str,
    brief_template: &str,
    link: Option<&str>,
) -> Result<HealthCheckResult> {
    let (count, items) = media_state_sample(pool, state, "").await?;
    if count == 0 {
        return Ok(HealthCheckResult::pass(title));
    }

    let message = message_template
        .replace("{count}", &count.to_string())
        .replace("{items}", &items.join(", "));
    let brief = brief_template.replace("{count}", &count.to_string());
    let result = HealthCheckResult::new(title, HealthCheckStatus::Warning, message, brief);

    Ok(if let Some(link) = link {
        result.with_link(link)
    } else {
        result
    })
}

/// Count media items in `state` and name up to five of them by path. The
/// optional `predicate` is a static SQL condition over the `mi` alias.
async fn media_state_sample(
    pool: &Pool<Db>,
    state: i32,
    predicate: &'static str,
) -> Result<(i64, Vec<String>)> {
    let count = sqlx::query_scalar::<_, i64>(crate::db::assert_sql_safe(format!(
        r#"SELECT COUNT(*) FROM "MediaItem" AS mi WHERE mi."State" = ? {predicate}"#
    )))
    .bind(state)
    .fetch_one(pool)
    .await
    .map_err(sqlx_error)?;
    if count == 0 {
        return Ok((0, Vec::new()));
    }

    let rows = sqlx::query(crate::db::assert_sql_safe(format!(
        r#"SELECT mi."Id", mf."Path"
           FROM "MediaItem" mi
           LEFT JOIN "Movie" m ON m."Id" = mi."Id"
           LEFT JOIN "Episode" e ON e."Id" = mi."Id"
           LEFT JOIN "MusicVideo" mvitem ON mvitem."Id" = mi."Id"
           LEFT JOIN "OtherVideo" ov ON ov."Id" = mi."Id"
           LEFT JOIN "Song" s ON s."Id" = mi."Id"
           LEFT JOIN "Image" i ON i."Id" = mi."Id"
           LEFT JOIN "RemoteStream" rs ON rs."Id" = mi."Id"
           LEFT JOIN "MediaVersion" mv ON mv."MovieId" = m."Id"
                OR mv."EpisodeId" = e."Id"
                OR mv."MusicVideoId" = mvitem."Id"
                OR mv."OtherVideoId" = ov."Id"
                OR mv."SongId" = s."Id"
                OR mv."ImageId" = i."Id"
                OR mv."RemoteStreamId" = rs."Id"
           LEFT JOIN "MediaFile" mf ON mf."MediaVersionId" = mv."Id"
           WHERE mi."State" = ? {predicate}
           ORDER BY mi."Id", mf."Id"
           LIMIT 5"#,
    )))
    .bind(state)
    .fetch_all(pool)
    .await
    .map_err(sqlx_error)?;

    let items = rows
        .iter()
        .map(|row| {
            row.try_get::<Option<String>, _>("Path")
                .ok()
                .flatten()
                .unwrap_or_else(|| {
                    format!(
                        "MediaItem {}",
                        row.try_get::<i64, _>("Id").unwrap_or_default()
                    )
                })
        })
        .collect::<Vec<_>>();
    Ok((count, items))
}

async fn missing_metadata_result(
    pool: &Pool<Db>,
    title: &str,
    item_table: &str,
    metadata_table: &str,
    version_fk: &str,
    item_label: &str,
) -> Result<HealthCheckResult> {
    let count_sql = format!(
        r#"SELECT COUNT(*)
           FROM "{item_table}" item
           WHERE NOT EXISTS (
               SELECT 1 FROM "{metadata_table}" metadata
               WHERE metadata."{item_table}Id" = item."Id"
           )"#
    );
    let count = sqlx::query_scalar::<_, i64>(crate::db::assert_sql_safe(&count_sql))
        .fetch_one(pool)
        .await
        .map_err(sqlx_error)?;

    if count == 0 {
        return Ok(HealthCheckResult::pass(title));
    }

    // Mirrors `EpisodeMetadataHealthCheck.cs` / `MovieMetadataHealthCheck.cs`
    // in ErsatzTV: take parent folders of missing items, deduplicate, then
    // keep the first five. The SQL `LIMIT` is generous so we have enough
    // raw rows to surface five distinct folders after deduplication.
    let paths_sql = format!(
        r#"SELECT mf."Path"
           FROM "{item_table}" item
           INNER JOIN "MediaVersion" mv ON mv."{version_fk}" = item."Id"
           INNER JOIN "MediaFile" mf ON mf."MediaVersionId" = mv."Id"
           WHERE NOT EXISTS (
               SELECT 1 FROM "{metadata_table}" metadata
               WHERE metadata."{item_table}Id" = item."Id"
           )
           ORDER BY mf."Path", mf."Id"
           LIMIT 500"#
    );
    let rows = sqlx::query(crate::db::assert_sql_safe(&paths_sql))
        .fetch_all(pool)
        .await
        .map_err(sqlx_error)?;
    let mut seen = std::collections::HashSet::new();
    let folders = rows
        .iter()
        .filter_map(|row| row.try_get::<Option<String>, _>("Path").ok().flatten())
        .filter_map(|path| parent_folder(&path))
        .filter(|folder| !folder.is_empty() && seen.insert(folder.clone()))
        .take(5)
        .collect::<Vec<_>>();

    Ok(HealthCheckResult::new(
        title,
        HealthCheckStatus::Warning,
        format!(
            "There are {count} {item_label} with missing metadata, including in the following folders: {}",
            folders.join(", ")
        ),
        format!("There are {count} {item_label} with missing metadata"),
    ))
}

async fn upsert_config(pool: &Pool<Db>, key: &str, value: &str) -> Result<()> {
    sqlx::query(r#"REPLACE INTO "ConfigElement" ("Key", "Value") VALUES (?, ?)"#)
        .bind(key)
        .bind(value)
        .execute(pool)
        .await
        .map_err(sqlx_error)?;
    Ok(())
}

async fn load_config(pool: &Pool<Db>, key: &str) -> Result<Option<String>> {
    let row = sqlx::query(r#"SELECT "Value" FROM "ConfigElement" WHERE "Key" = ?"#)
        .bind(key)
        .fetch_optional(pool)
        .await
        .map_err(sqlx_error)?;
    row.map(|row| {
        row.try_get::<Option<String>, _>("Value")
            .map(Option::unwrap_or_default)
    })
    .transpose()
    .map_err(sqlx_error)
}

fn runtime_issue_name(issue: &FfmpegRuntimeIssue) -> String {
    match issue {
        FfmpegRuntimeIssue::RequiredFilter { name }
        | FfmpegRuntimeIssue::RequiredEncoder { name }
        | FfmpegRuntimeIssue::RequiredDecoder { name }
        | FfmpegRuntimeIssue::RequiredHwaccel { name }
        | FfmpegRuntimeIssue::OptionalHwaccel { name } => name.clone(),
        FfmpegRuntimeIssue::RequiredConfigurationFlag { flag } => flag.clone(),
        FfmpegRuntimeIssue::RequiredEncoderOption { encoder, option } => {
            format!("{encoder} {option}")
        }
    }
}

fn supported_acceleration_kinds(runtime: &FfmpegRuntimeReport) -> Vec<i32> {
    let factory = HardwareCapabilitiesFactory::new(&runtime.capabilities);
    let mut result = Vec::new();
    if factory.has_hardware_acceleration(HardwareAccelerationKind::Qsv) {
        result.push(HW_ACCEL_QSV);
    }
    if factory.has_hardware_acceleration(HardwareAccelerationKind::Nvenc) {
        result.push(HW_ACCEL_NVENC);
    }
    if factory.has_hardware_acceleration(HardwareAccelerationKind::Vaapi) {
        result.push(HW_ACCEL_VAAPI);
    }
    if factory.has_hardware_acceleration(HardwareAccelerationKind::VideoToolbox) {
        result.push(HW_ACCEL_VIDEOTOOLBOX);
    }
    if factory.has_hardware_acceleration(HardwareAccelerationKind::Rkmpp) {
        result.push(HW_ACCEL_RKMPP);
    }
    if factory.has_hardware_acceleration(HardwareAccelerationKind::Amf) {
        result.push(HW_ACCEL_AMF);
    }
    if factory.has_hardware_acceleration(HardwareAccelerationKind::V4l2m2m) {
        result.push(HW_ACCEL_V4L2M2M);
    }
    result
}

fn acceleration_name(kind: i32) -> &'static str {
    match kind {
        HW_ACCEL_NONE => "None",
        HW_ACCEL_QSV => "Qsv",
        HW_ACCEL_NVENC => "Nvenc",
        HW_ACCEL_VAAPI => "Vaapi",
        HW_ACCEL_VIDEOTOOLBOX => "VideoToolbox",
        HW_ACCEL_AMF => "Amf",
        HW_ACCEL_V4L2M2M => "V4l2m2m",
        HW_ACCEL_RKMPP => "Rkmpp",
        _ => "Unknown",
    }
}

/// Return the directory component of `path` for the health-check warning
/// message. Mirrors `.NET`'s `Path.GetDirectoryName`, which understands both
/// `/` and `\` separators on every host. `ErsatzTV` stores raw provider
/// paths from Plex/Jellyfin/Emby/local hosts, so a Linux deployment of
/// `ErsatzRS` routinely sees Windows-shaped paths (e.g. `D:\Media\...`) and
/// cannot rely on `std::path::Path::parent()`, which only splits on `/`
/// on Unix.
fn parent_folder(path: &str) -> Option<String> {
    let trimmed = path.trim_end_matches(['/', '\\']);
    let split_at = trimmed.rfind(['/', '\\']).map(|idx| idx + 1)?;
    let parent = &trimmed[..split_at];
    // Strip the trailing separator unless we're left with a bare root
    // (`/`, `C:\`, `\\server\share\`). Drive-letter and UNC roots are
    // preserved so the message remains parseable to a human reader.
    let stripped = parent.trim_end_matches(['/', '\\']);
    if stripped.is_empty() {
        return Some(parent.to_owned());
    }
    if stripped.ends_with(':') {
        return Some(parent.to_owned());
    }
    Some(stripped.to_owned())
}

fn format_versions(versions: &[i64]) -> String {
    versions
        .iter()
        .map(i64::to_string)
        .collect::<Vec<_>>()
        .join(", ")
}

#[allow(clippy::needless_pass_by_value)]
fn sqlx_error(error: sqlx::Error) -> HealthCheckError {
    HealthCheckError::Database(error.to_string())
}

#[cfg(test)]
mod tests {
    use std::collections::BTreeSet;

    use chrono::{Duration, Utc};
    use ersatzrs_core::locking::EntityLocker;
    use ersatzrs_core::repositories::{PlayoutRepository, ProgramScheduleRepository};
    use ersatzrs_core::scheduling::{PlayoutBuildMode, PlayoutBuildRequest};
    use ersatzrs_domain::{CollectionType, PlayoutItem, ProgramScheduleItem};
    use ersatzrs_ffmpeg::FfmpegRuntimeReport;
    use sqlx::Executor;

    use super::*;

    #[tokio::test]
    async fn health_check_service_preserves_registered_set_order() {
        let pool = crate::db::lazy_sqlite_pool("sqlite::memory:").expect("lazy pool");
        let service = make_health_check_service(
            pool,
            FfmpegRuntimeReport::for_tests(),
            DatabaseSchemaStatus::default(),
        );

        let titles = service.titles();
        assert!(titles.iter().all(|title| !title.trim().is_empty()));
        assert_eq!(titles, REGISTERED_HEALTH_CHECK_TITLES);
    }

    #[tokio::test]
    async fn pinned_stream_health_check_fails_when_a_saved_stream_disappears() {
        let pool = test_pool().await;
        seed_library_path(&pool).await;
        for statement in [
            r#"INSERT INTO "MediaItem" ("Id", "LibraryPathId", "State")
               VALUES (70, 10, 0)"#,
            r#"INSERT INTO "Movie" ("Id") VALUES (70)"#,
            r#"INSERT INTO "MediaVersion" (
                   "Id", "DateAdded", "DateUpdated", "Duration", "Height",
                   "MovieId", "VideoScanKind", "Width"
               ) VALUES (
                   71, '2026-07-27T00:00:00Z', '2026-07-27T00:00:00Z',
                   '01:00:00', 1080, 70, 1, 1920
               )"#,
            r#"INSERT INTO "MediaStream" (
                   "Id", "Index", "Codec", "MediaStreamKind", "Channels",
                   "Default", "Forced", "MediaVersionId"
               ) VALUES (72, 0, 'h264', 1, 0, 1, 0, 71)"#,
            r#"INSERT INTO "PlaylistGroup" ("Id", "Name") VALUES (70, 'Health')"#,
            r#"INSERT INTO "Playlist" ("Id", "IsSystem", "Name", "PlaylistGroupId")
               VALUES (70, 0, 'Health', 70)"#,
            r#"INSERT INTO "PlaylistItem" (
                   "Id", "Index", "PlaylistId", "CollectionType", "MediaItemId",
                   "MediaVersionId", "VideoStreamIndex"
               ) VALUES (70, 0, 70, 10, 70, 71, 0)"#,
        ] {
            pool.execute(statement)
                .await
                .expect("seed pinned stream health row");
        }

        let check = PinnedStreamSelectionHealthCheck::new(pool.clone());
        assert_eq!(
            check.check().await.expect("check valid pin").status,
            HealthCheckStatus::Pass
        );

        pool.execute(r#"UPDATE "PlaylistItem" SET "VideoStreamIndex" = 9 WHERE "Id" = 70"#)
            .await
            .expect("stale pinned video stream");
        let result = check.check().await.expect("check stale pin");
        assert_eq!(result.status, HealthCheckStatus::Fail);
        assert_eq!(result.brief_message, "1 stale pinned stream selection(s)");
    }

    #[tokio::test]
    async fn run_and_persist_health_checks_writes_config_summary() {
        let pool = test_pool().await;
        let report = FfmpegRuntimeReport::for_tests();

        let run = run_and_persist_health_checks(
            pool.clone(),
            report,
            DatabaseSchemaStatus {
                dirty_version: None,
                unknown_migrations: vec![9999],
                mismatched_migrations: Vec::new(),
            },
        )
        .await
        .expect("run health checks");

        assert!(run
            .results
            .iter()
            .any(|result| result.title == "ErsatzTV Downgrade"
                && result.status == HealthCheckStatus::Fail));
        assert_eq!(run.summary.errors, 1);

        let store = SqlHealthCheckStore::new(pool);
        assert_eq!(
            store.load_summary().await.expect("load summary"),
            run.summary
        );
        assert_eq!(
            store.load_results().await.expect("load results").len(),
            run.results.len()
        );
    }

    #[tokio::test]
    async fn checksum_mismatch_reports_schema_drift_not_downgrade() {
        let schema_status = DatabaseSchemaStatus {
            dirty_version: None,
            unknown_migrations: Vec::new(),
            mismatched_migrations: vec![1],
        };

        let downgrade = DowngradeHealthCheck::new(schema_status.clone())
            .check()
            .await
            .expect("downgrade check");
        assert_eq!(downgrade.status, HealthCheckStatus::NotApplicable);

        let checksum = MigrationChecksumHealthCheck::new(schema_status)
            .check()
            .await
            .expect("checksum check");
        assert_eq!(checksum.status, HealthCheckStatus::Fail);
        assert_eq!(
            checksum.brief_message,
            "Database migration checksum mismatch"
        );
        assert!(checksum.message.contains("version(s): 1"));
    }

    #[tokio::test]
    async fn dirty_migration_reports_partial_migration() {
        let result = DirtyMigrationHealthCheck::new(DatabaseSchemaStatus {
            dirty_version: Some(42),
            unknown_migrations: Vec::new(),
            mismatched_migrations: Vec::new(),
        })
        .check()
        .await
        .expect("dirty migration check");

        assert_eq!(result.status, HealthCheckStatus::Fail);
        assert_eq!(
            result.brief_message,
            "Database migration is partially applied"
        );
        assert!(result.message.contains("version 42"));
    }

    #[tokio::test]
    async fn search_projection_health_check_requires_variant_and_stream_title_fields() {
        let pool = test_pool().await;
        let result = SearchProjectionHealthCheck::new(pool)
            .check()
            .await
            .expect("search projection check");

        assert_eq!(result.status, HealthCheckStatus::Pass);
        assert_eq!(result.brief_message, "Search projection ready");
    }

    #[tokio::test]
    async fn ffmpeg_reports_check_warns_when_save_reports_is_enabled() {
        let pool = test_pool().await;
        let check = FFmpegReportsHealthCheck::new(pool.clone());

        // Default state: no config row.
        let result = check.check().await.expect("default check");
        assert_eq!(result.status, HealthCheckStatus::Pass);

        // Disabled.
        upsert_config(&pool, CONFIG_KEY_FFMPEG_SAVE_REPORTS, "False")
            .await
            .expect("seed disabled save reports");
        let result = check.check().await.expect("disabled check");
        assert_eq!(result.status, HealthCheckStatus::Pass);

        // ErsatzTV writes booleans via `bool.ToString()`; accept the canonical
        // capitalization plus a couple of common variants.
        for raw in ["True", "true", "TRUE", "1"] {
            upsert_config(&pool, CONFIG_KEY_FFMPEG_SAVE_REPORTS, raw)
                .await
                .expect("seed enabled save reports");
            let result = check.check().await.expect("enabled check");
            assert_eq!(result.status, HealthCheckStatus::Warning);
            assert_eq!(
                result.brief_message, "FFmpeg troubleshooting reports are enabled",
                "raw value {raw} should warn",
            );
        }
    }

    #[tokio::test]
    async fn playback_data_access_health_distinguishes_missing_invalid_and_unavailable_config() {
        let pool = test_pool().await;
        for key in [
            ersatzrs_domain::CONFIG_KEY_FFMPEG_ACTIVE_TRANSCODE_LIMIT,
            ersatzrs_domain::CONFIG_KEY_FFMPEG_WORK_AHEAD_SEGMENTERS,
        ] {
            sqlx::query(r#"DELETE FROM "ConfigElement" WHERE "Key" = ?"#)
                .bind(key)
                .execute(&pool)
                .await
                .expect("remove seeded playback config");
        }
        let check = PlaybackDataAccessHealthCheck::new(pool.clone());
        assert_eq!(
            check.check().await.expect("check absent config").status,
            HealthCheckStatus::Pass
        );

        upsert_config(
            &pool,
            ersatzrs_domain::CONFIG_KEY_FFMPEG_ACTIVE_TRANSCODE_LIMIT,
            "invalid",
        )
        .await
        .expect("seed invalid active transcode limit");
        let result = check.check().await.expect("check invalid config");
        assert_eq!(result.status, HealthCheckStatus::Fail);
        assert_eq!(result.brief_message, "Invalid active transcode limit");

        pool.close().await;
        assert!(matches!(
            check.check().await,
            Err(HealthCheckError::Database(_))
        ));
    }

    #[tokio::test]
    async fn database_health_checks_report_current_parity_gaps() {
        let pool = test_pool().await;
        seed_library_path(&pool).await;
        seed_movie_with_zero_duration_and_missing_metadata(&pool).await;
        seed_unavailable_movie(&pool).await;
        seed_empty_schedule(&pool).await;

        let mut report = FfmpegRuntimeReport::for_tests();
        report.capabilities.hwaccels = BTreeSet::from(["cuda".to_owned()]);
        let service = make_health_check_service(pool, report, DatabaseSchemaStatus::default());
        let results = service.perform_health_checks().await;

        assert_result(
            &results,
            "File Not Found",
            HealthCheckStatus::Warning,
            "There are 1 items that do not exist on disk",
        );
        assert_result(
            &results,
            "Unavailable",
            HealthCheckStatus::Warning,
            "There are 1 items that are unavailable",
        );
        assert_result(
            &results,
            "Zero Duration",
            HealthCheckStatus::Warning,
            "There are 1 files with zero duration",
        );
        assert_result(
            &results,
            "Movie Metadata",
            HealthCheckStatus::Warning,
            "There are 2 movies with missing metadata",
        );
        assert_result(
            &results,
            "Empty Schedule",
            HealthCheckStatus::Warning,
            "There are 1 empty schedules in use, which are NOT supported and WILL cause errors",
        );
    }

    #[tokio::test]
    async fn classic_playout_state_health_fails_until_runtime_state_is_complete() {
        let pool = test_pool().await;
        seed_empty_schedule(&pool).await;
        seed_library_path(&pool).await;
        for statement in [
            r#"INSERT INTO "MediaItem" ("Id", "LibraryPathId", "State") VALUES (80, 10, 0)"#,
            r#"INSERT INTO "Movie" ("Id") VALUES (80)"#,
            r#"INSERT INTO "MediaVersion" (
                   "Id", "DateAdded", "DateUpdated", "Duration", "Height",
                   "MovieId", "VideoScanKind", "Width"
               ) VALUES (
                   80, '2026-08-24T00:00:00Z', '2026-08-24T00:00:00Z',
                   '00:01:00', 1080, 80, 1, 1920
               )"#,
            r#"INSERT INTO "Collection" ("Id", "Name", "UseCustomPlaybackOrder")
               VALUES (80, 'Classic Health', 0)"#,
            r#"INSERT INTO "CollectionItem" ("CollectionId", "MediaItemId") VALUES (80, 80)"#,
        ] {
            pool.execute(statement)
                .await
                .expect("seed Classic health content");
        }
        let repositories = crate::repositories::make_repositories(pool.clone());
        repositories
            .program_schedule
            .upsert_item(&ProgramScheduleItem {
                id: 80,
                program_schedule_id: 10,
                collection_type: CollectionType::Collection,
                collection_id: Some(80),
                ..ProgramScheduleItem::default()
            })
            .await
            .expect("seed Classic schedule item");
        let now = Utc::now();
        repositories
            .playout
            .upsert_item(&PlayoutItem {
                id: 80,
                playout_id: 10,
                media_item_id: 80,
                start: now,
                finish: now + Duration::minutes(1),
                out_point: Duration::minutes(1),
                ..PlayoutItem::default()
            })
            .await
            .expect("seed generated Classic item");
        pool.execute(r#"UPDATE "Playout" SET "ScheduleKind" = 'Classic' WHERE "Id" = 10"#)
            .await
            .expect("seed legacy textual Classic schedule kind");

        let check = ClassicPlayoutStateHealthCheck::new(pool.clone());
        let invalid = check.check().await.expect("audit invalid Classic state");
        assert_eq!(invalid.status, HealthCheckStatus::Fail);
        assert!(invalid.message.contains("50 - Health"));

        crate::maintenance::run_requested_playout_build(
            &pool,
            &EntityLocker::new(),
            PlayoutBuildRequest {
                playout_id: 10,
                mode: PlayoutBuildMode::Reset,
            },
            now,
            None,
        )
        .await
        .expect("build complete Classic checkpoint state");

        assert_eq!(
            check
                .check()
                .await
                .expect("audit repaired Classic state")
                .status,
            HealthCheckStatus::Pass
        );
    }

    #[tokio::test]
    async fn scan_refresh_state_health_ignores_recent_active_work() {
        let pool = test_pool().await;
        seed_empty_schedule(&pool).await;
        seed_library_path(&pool).await;
        let now = Utc::now().to_rfc3339();

        sqlx::query(
            r#"INSERT INTO "ScanRefreshResolution" (
                   "LibraryId", "Generation", "State", "Reason", "StartedAt", "UpdatedAt"
               ) VALUES (10, 1, 0, 'remote-stream-membership-added', ?, ?)"#,
        )
        .bind(&now)
        .bind(&now)
        .execute(&pool)
        .await
        .expect("seed active impact resolution");
        sqlx::query(
            r#"INSERT INTO "ScanPlayoutRefreshIntent" (
                   "PlayoutId", "LibraryId", "Generation", "State", "Reason",
                   "CreatedAt", "UpdatedAt"
               ) VALUES (10, 10, 1, 1, 'remote-stream-membership-added', ?, ?)"#,
        )
        .bind(&now)
        .bind(&now)
        .execute(&pool)
        .await
        .expect("seed active playout refresh intent");

        let result = ScanRefreshStateHealthCheck::new(pool)
            .check()
            .await
            .expect("audit active scan refresh work");
        assert_eq!(result.status, HealthCheckStatus::Pass);
    }

    #[tokio::test]
    async fn scan_refresh_state_health_reports_bounded_aggregate_failures() {
        let pool = test_pool().await;
        seed_empty_schedule(&pool).await;
        seed_library_path(&pool).await;
        for statement in [
            r#"INSERT INTO "Library" ("Id", "MediaKind", "MediaSourceId", "Name")
               VALUES (11, 1, 10, 'private-provider-8675309')"#,
            r#"INSERT INTO "Playout" ("Id", "ChannelId", "ScheduleKind", "Seed")
               VALUES (11, 50, 1, 0)"#,
        ] {
            pool.execute(statement)
                .await
                .expect("seed second scan refresh owner");
        }
        let diagnostic = format!("provider-item-id-secret:{}", "x".repeat(4_096));
        let old = "2020-01-01T00:00:00Z";
        sqlx::query(
            r#"INSERT INTO "ScanRefreshResolution" (
                   "LibraryId", "Generation", "State", "Reason", "Diagnostic",
                   "StartedAt", "UpdatedAt"
               ) VALUES (10, 1, 2, 'remote-stream-membership-added', ?, ?, ?)"#,
        )
        .bind(&diagnostic)
        .bind(old)
        .bind(old)
        .execute(&pool)
        .await
        .expect("seed failed impact resolution");
        sqlx::query(
            r#"INSERT INTO "ScanRefreshResolution" (
                   "LibraryId", "Generation", "State", "Reason", "Diagnostic",
                   "StartedAt", "UpdatedAt"
               ) VALUES (11, 1, 0, 'remote-stream-membership-removed', ?, ?, ?)"#,
        )
        .bind(&diagnostic)
        .bind(old)
        .bind(old)
        .execute(&pool)
        .await
        .expect("seed stale impact resolution");
        sqlx::query(
            r#"INSERT INTO "ScanPlayoutRefreshIntent" (
                   "PlayoutId", "LibraryId", "Generation", "State", "Reason", "Diagnostic",
                   "CreatedAt", "UpdatedAt"
               ) VALUES (10, 10, 1, 2, 'remote-stream-membership-added', ?, ?, ?)"#,
        )
        .bind(&diagnostic)
        .bind(old)
        .bind(old)
        .execute(&pool)
        .await
        .expect("seed failed playout refresh intent");
        sqlx::query(
            r#"INSERT INTO "ScanPlayoutRefreshIntent" (
                   "PlayoutId", "LibraryId", "Generation", "State", "Reason", "Diagnostic",
                   "CreatedAt", "UpdatedAt"
               ) VALUES (11, 11, 1, 1, 'remote-stream-membership-removed', ?, ?, ?)"#,
        )
        .bind(&diagnostic)
        .bind(old)
        .bind(old)
        .execute(&pool)
        .await
        .expect("seed stale playout refresh intent");

        let check = ScanRefreshStateHealthCheck::new(pool.clone());
        let result = check
            .check()
            .await
            .expect("audit unhealthy scan refresh work");
        assert_eq!(result.status, HealthCheckStatus::Fail);
        assert_eq!(result.brief_message, "4 scan refresh state error(s)");
        assert_eq!(
            result.link.as_ref().map(|link| link.link.as_str()),
            Some("/playouts")
        );
        assert_eq!(
            result.message,
            "Scan refresh state needs attention: 1 unresolved impact resolution(s), 1 stale impact resolution(s), 1 failed playout refresh intent(s), and 1 stale playout refresh intent(s). Review scanner and scheduling logs, then retry the affected scan or playout refresh."
        );
        assert!(!result.message.contains("private-provider-8675309"));
        assert!(!result.message.contains("provider-item-id-secret"));
        assert!(result.message.len() < diagnostic.len());

        pool.execute(r#"UPDATE "ScanRefreshResolution" SET "State" = 1"#)
            .await
            .expect("resolve impact state");
        pool.execute(r#"DELETE FROM "ScanPlayoutRefreshIntent""#)
            .await
            .expect("complete playout refresh intents");
        assert_eq!(
            check
                .check()
                .await
                .expect("audit repaired scan refresh work")
                .status,
            HealthCheckStatus::Pass
        );
    }

    #[tokio::test]
    async fn mirror_channel_health_reports_malformed_offset_and_own_playout() {
        let pool = test_pool().await;
        seed_empty_schedule(&pool).await;
        pool.execute(
            r#"INSERT INTO "Channel" (
                "Id", "UniqueId", "Number", "SortNumber", "Name", "FFmpegProfileId",
                "IdleBehavior", "MusicVideoCreditsMode", "PlayoutMode", "PlayoutSource",
                "MirrorSourceChannelId", "PlayoutOffset", "SongVideoMode", "StreamingMode",
                "StreamSelectorMode", "SubtitleMode", "TranscodeMode"
               ) VALUES (
                51, '00000000-0000-0000-0000-000000000051', '51', 51.0,
                'Broken Mirror', 50, 0, 0, 0, 1, 50, 'not-a-duration', 0, 1, 0, 0, 0
               )"#,
        )
        .await
        .expect("seed broken mirror");
        pool.execute(
            r#"INSERT INTO "Playout" ("Id", "ChannelId", "ScheduleKind", "Seed")
               VALUES (11, 51, 1, 0)"#,
        )
        .await
        .expect("seed conflicting mirror playout");

        let result = MirrorChannelHealthCheck::new(pool)
            .check()
            .await
            .expect("check mirror channels");
        assert_eq!(result.status, HealthCheckStatus::Warning);
        assert!(result.message.contains("offset is malformed"));
        assert!(result.message.contains("mirror also has its own playout"));
    }

    #[tokio::test]
    async fn provider_connectivity_check_warns_when_endpoints_are_unreachable() {
        let pool = test_pool().await;
        let check = ProviderConnectivityHealthCheck::new(pool.clone());

        // No persisted poll yet → not applicable.
        let result = check.check().await.expect("default check");
        assert_eq!(result.status, HealthCheckStatus::NotApplicable);

        // Seed a Plex media source so the check can resolve a friendly server name.
        pool.execute(r#"INSERT INTO "MediaSource" ("Id") VALUES (60)"#)
            .await
            .expect("seed media source");
        pool.execute(
            r#"INSERT INTO "PlexMediaSource" ("Id", "ServerName", "ClientIdentifier")
               VALUES (60, 'Living Room Plex', 'cid-60')"#,
        )
        .await
        .expect("seed plex media source");

        let payload = serde_json::to_string(&vec![
            ProviderConnectivityResult {
                provider: "plex".to_owned(),
                media_source_id: 60,
                url: "http://10.0.0.5:32400".to_owned(),
                reachable: false,
            },
            ProviderConnectivityResult {
                provider: "jellyfin".to_owned(),
                media_source_id: 70,
                url: "http://10.0.0.6:8096".to_owned(),
                reachable: true,
            },
        ])
        .expect("encode connectivity results");
        upsert_config(&pool, PROVIDER_CONNECTIVITY_KEY, &payload)
            .await
            .expect("seed connectivity results");

        let result = check.check().await.expect("warning check");
        assert_eq!(result.status, HealthCheckStatus::Warning);
        assert_eq!(result.brief_message, "1 provider connection is unreachable");
        assert!(
            result.message.contains("Living Room Plex"),
            "expected friendly source name in {}",
            result.message
        );

        // All reachable → pass.
        let payload = serde_json::to_string(&vec![ProviderConnectivityResult {
            provider: "plex".to_owned(),
            media_source_id: 60,
            url: "http://10.0.0.5:32400".to_owned(),
            reachable: true,
        }])
        .expect("encode connectivity results");
        upsert_config(&pool, PROVIDER_CONNECTIVITY_KEY, &payload)
            .await
            .expect("seed connectivity results");
        let result = check.check().await.expect("pass check");
        assert_eq!(result.status, HealthCheckStatus::Pass);
    }

    #[tokio::test]
    async fn zero_duration_check_ignores_remote_stream_placeholders() {
        let pool = test_pool().await;
        seed_library_path(&pool).await;
        seed_remote_stream_with_zero_duration(&pool).await;

        let check = ZeroDurationHealthCheck::new(pool);
        let result = check.check().await.expect("zero duration check");

        assert_eq!(result.status, HealthCheckStatus::Pass);
    }

    #[tokio::test]
    async fn remote_stream_check_is_structural_and_does_not_contact_source() {
        let pool = test_pool().await;
        seed_library_path(&pool).await;
        seed_remote_stream_with_zero_duration(&pool).await;

        let check = RemoteStreamHealthCheck::new(pool.clone());
        let result = check.check().await.expect("remote stream check");
        assert_eq!(result.status, HealthCheckStatus::Pass);

        sqlx::query(r#"UPDATE "RemoteStream" SET "Url" = NULL, "Script" = NULL WHERE "Id" = 22"#)
            .execute(&pool)
            .await
            .expect("invalidate remote stream");
        let result = check.check().await.expect("invalid remote stream check");
        assert_eq!(result.status, HealthCheckStatus::Fail);
        assert_eq!(
            result.brief_message,
            "One or more Remote Stream definitions are invalid"
        );
    }

    #[tokio::test]
    async fn zero_duration_check_ignores_static_image_versions() {
        let pool = test_pool().await;
        seed_library_path(&pool).await;
        seed_image_with_zero_duration(&pool, 23, "/media/stills/title-card.jpg").await;

        let check = ZeroDurationHealthCheck::new(pool);
        let result = check.check().await.expect("zero duration check");

        assert_eq!(result.status, HealthCheckStatus::Pass);
    }

    #[tokio::test]
    async fn zero_duration_check_warns_for_video_and_animated_image_candidates() {
        let pool = test_pool().await;
        seed_library_path(&pool).await;
        seed_movie_with_zero_duration_and_missing_metadata(&pool).await;
        seed_image_with_zero_duration(&pool, 23, "/media/animated/countdown.gif").await;

        let check = ZeroDurationHealthCheck::new(pool);
        let result = check.check().await.expect("zero duration check");

        assert_eq!(result.status, HealthCheckStatus::Warning);
        assert_eq!(result.brief_message, "There are 2 files with zero duration");
        assert!(result.message.contains("/media/missing/movie.mkv"));
        assert!(result.message.contains("/media/animated/countdown.gif"));
    }

    #[tokio::test]
    async fn vaapi_device_check_fails_when_device_is_missing() {
        let pool = test_pool().await;
        seed_library_path(&pool).await;
        seed_vaapi_profile_and_channel(&pool, "/dev/dri/renderD-does-not-exist").await;

        let mut report = FfmpegRuntimeReport::for_tests();
        report.capabilities.hwaccels = BTreeSet::from(["vaapi".to_owned()]);
        let check = VaapiDeviceHealthCheck::new(pool, report);

        let result = check.check().await.expect("device check");
        assert_eq!(result.status, HealthCheckStatus::Fail);
        assert_eq!(
            result.brief_message,
            "Configured VAAPI device does not exist"
        );
    }

    #[test]
    fn parent_folder_handles_unix_and_windows_paths() {
        // Unix host paths.
        assert_eq!(
            parent_folder("/media/tv/Show/Season 1/episode.mkv").as_deref(),
            Some("/media/tv/Show/Season 1")
        );
        // Windows host paths arriving from Jellyfin/Emby/Plex on Windows.
        // Without separator-agnostic parsing these collapse to "" because
        // `std::path::Path::parent` only splits on `/` on Unix, which is
        // what produced the empty folders in the /system/health message.
        assert_eq!(
            parent_folder(r"D:\Downloads\TV\3 Body Problem (2024)\Season 1\ep.mkv").as_deref(),
            Some(r"D:\Downloads\TV\3 Body Problem (2024)\Season 1")
        );
        // Trailing separators must not produce an empty parent.
        assert_eq!(
            parent_folder(r"D:\Media\Show\episode.mkv\").as_deref(),
            Some(r"D:\Media\Show")
        );
        // Mixed separators (Jellyfin sometimes normalizes one side).
        assert_eq!(
            parent_folder(r"D:\Media/Show\episode.mkv").as_deref(),
            Some(r"D:\Media/Show")
        );
        // A bare file name has no parent.
        assert_eq!(parent_folder("episode.mkv"), None);
        // Roots are preserved with their separator so the message stays
        // human-readable.
        assert_eq!(parent_folder("/episode.mkv").as_deref(), Some("/"));
        assert_eq!(parent_folder(r"C:\episode.mkv").as_deref(), Some(r"C:\"));
    }

    fn assert_result(
        results: &[HealthCheckResult],
        title: &str,
        status: HealthCheckStatus,
        brief: &str,
    ) {
        let result = results
            .iter()
            .find(|result| result.title == title)
            .unwrap_or_else(|| panic!("missing health result {title}"));
        assert_eq!(result.status, status);
        assert_eq!(result.brief_message, brief);
    }

    async fn test_pool() -> Pool<Db> {
        let temp_dir =
            std::env::temp_dir().join(format!("ersatzrs-health-{}", uuid::Uuid::new_v4()));
        std::fs::create_dir_all(&temp_dir).expect("create temp health db dir");
        let database_url = format!("sqlite:{}", temp_dir.join("ErsatzRS.db").display());
        crate::db::init(&database_url)
            .await
            .expect("initialize health test database")
    }

    async fn seed_library_path(pool: &Pool<Db>) {
        pool.execute(r#"INSERT INTO "MediaSource" ("Id") VALUES (10)"#)
            .await
            .expect("seed media source");
        pool.execute(r#"INSERT INTO "LocalMediaSource" ("Id") VALUES (10)"#)
            .await
            .expect("seed local media source");
        pool.execute(
            r#"INSERT INTO "Library" ("Id", "MediaKind", "MediaSourceId", "Name")
               VALUES (10, 1, 10, 'Movies')"#,
        )
        .await
        .expect("seed library");
        pool.execute(r#"INSERT INTO "LocalLibrary" ("Id") VALUES (10)"#)
            .await
            .expect("seed local library");
        pool.execute(
            r#"INSERT INTO "LibraryPath" ("Id", "LibraryId", "Path")
               VALUES (10, 10, '/media')"#,
        )
        .await
        .expect("seed library path");
    }

    async fn seed_movie_with_zero_duration_and_missing_metadata(pool: &Pool<Db>) {
        pool.execute(
            r#"INSERT INTO "MediaItem" ("Id", "LibraryPathId", "State") VALUES (20, 10, 1)"#,
        )
        .await
        .expect("seed media item");
        pool.execute(r#"INSERT INTO "Movie" ("Id") VALUES (20)"#)
            .await
            .expect("seed movie");
        pool.execute(
            r#"INSERT INTO "MediaVersion" (
                "Id", "DateAdded", "DateUpdated", "Duration", "Height", "MovieId",
                "VideoScanKind", "Width"
               ) VALUES (
                20, '2026-04-26T00:00:00Z', '2026-04-26T00:00:00Z',
                '00:00:00', 1080, 20, 1, 1920
               )"#,
        )
        .await
        .expect("seed media version");
        pool.execute(
            r#"INSERT INTO "MediaFile" ("MediaVersionId", "Path", "PathHash")
               VALUES (20, '/media/missing/movie.mkv', 'hash20')"#,
        )
        .await
        .expect("seed media file");
    }

    async fn seed_unavailable_movie(pool: &Pool<Db>) {
        pool.execute(
            r#"INSERT INTO "MediaItem" ("Id", "LibraryPathId", "State") VALUES (21, 10, 2)"#,
        )
        .await
        .expect("seed media item");
        pool.execute(r#"INSERT INTO "Movie" ("Id") VALUES (21)"#)
            .await
            .expect("seed movie");
        pool.execute(
            r#"INSERT INTO "MediaVersion" (
                "Id", "DateAdded", "DateUpdated", "Duration", "Height", "MovieId",
                "VideoScanKind", "Width"
               ) VALUES (
                21, '2026-04-26T00:00:00Z', '2026-04-26T00:00:00Z',
                '01:00:00', 1080, 21, 1, 1920
               )"#,
        )
        .await
        .expect("seed media version");
        pool.execute(
            r#"INSERT INTO "MediaFile" ("MediaVersionId", "Path", "PathHash")
               VALUES (21, '/media/unavailable/movie.mkv', 'hash21')"#,
        )
        .await
        .expect("seed media file");
    }

    async fn seed_remote_stream_with_zero_duration(pool: &Pool<Db>) {
        pool.execute(
            r#"INSERT INTO "MediaItem" ("Id", "LibraryPathId", "State") VALUES (22, 10, 1)"#,
        )
        .await
        .expect("seed remote stream media item");
        pool.execute(
            r#"INSERT INTO "RemoteStream" ("Id", "Duration", "Url", "Liveness", "IsLive")
               VALUES (22, '00:00:00', 'https://example.test/live.m3u8', 2, 1)"#,
        )
        .await
        .expect("seed remote stream");
        pool.execute(
            r#"INSERT INTO "MediaVersion" (
                "Id", "DateAdded", "DateUpdated", "Duration", "Height", "RemoteStreamId",
                "VideoScanKind", "Width"
               ) VALUES (
                22, '2026-04-26T00:00:00Z', '2026-04-26T00:00:00Z',
                '00:00:00', 0, 22, 1, 0
               )"#,
        )
        .await
        .expect("seed remote stream media version");
        pool.execute(
            r#"INSERT INTO "MediaFile" ("MediaVersionId", "Path", "PathHash")
               VALUES (22, 'https://example.test/live.m3u8', 'hash22')"#,
        )
        .await
        .expect("seed remote stream media file");
    }

    async fn seed_image_with_zero_duration(pool: &Pool<Db>, id: i64, path: &str) {
        sqlx::query(
            r#"INSERT INTO "MediaItem" ("Id", "LibraryPathId", "State") VALUES (?, 10, 0)"#,
        )
        .bind(id)
        .execute(pool)
        .await
        .expect("seed image media item");
        sqlx::query(r#"INSERT INTO "Image" ("Id") VALUES (?)"#)
            .bind(id)
            .execute(pool)
            .await
            .expect("seed image");
        sqlx::query(
            r#"INSERT INTO "MediaVersion" (
                "Id", "DateAdded", "DateUpdated", "Duration", "Height", "ImageId",
                "VideoScanKind", "Width"
               ) VALUES (
                ?, '2026-04-26T00:00:00Z', '2026-04-26T00:00:00Z',
                '00:00:00', 1080, ?, 1, 1920
               )"#,
        )
        .bind(id)
        .bind(id)
        .execute(pool)
        .await
        .expect("seed image media version");
        sqlx::query(
            r#"INSERT INTO "MediaFile" ("MediaVersionId", "Path", "PathHash")
               VALUES (?, ?, ?)"#,
        )
        .bind(id)
        .bind(path)
        .bind(format!("hash{id}"))
        .execute(pool)
        .await
        .expect("seed image media file");
    }

    async fn seed_empty_schedule(pool: &Pool<Db>) {
        pool.execute(
            r#"INSERT OR IGNORE INTO "Resolution" ("Id", "Name", "IsCustom", "Width", "Height")
               VALUES (50, '1920x1080', 0, 1920, 1080)"#,
        )
        .await
        .expect("seed resolution");
        pool.execute(
            r#"INSERT INTO "FFmpegProfile" (
                "Id", "AudioBitrate", "AudioBufferSize", "AudioChannels", "AudioFormat",
                "AudioSampleRate", "BitDepth", "HardwareAcceleration", "Name",
                "NormalizeFramerate", "NormalizeLoudnessMode", "ResolutionId",
                "ScalingBehavior", "ThreadCount", "VaapiDriver", "VideoBitrate",
                "VideoBufferSize", "VideoFormat"
               ) VALUES (50, 192, 384, 2, 1, 48, 0, 0, 'Health', 0, 0, 50, 0, 0, 0, 2000, 4000, 1)"#,
        )
        .await
        .expect("seed profile");
        pool.execute(
            r#"INSERT INTO "Channel" (
                "Id", "UniqueId", "Number", "SortNumber", "Name", "FFmpegProfileId",
                "IdleBehavior", "MusicVideoCreditsMode", "PlayoutMode", "PlayoutSource",
                "SongVideoMode", "StreamingMode", "StreamSelectorMode", "SubtitleMode",
                "TranscodeMode"
               ) VALUES (
                50, '00000000-0000-0000-0000-000000000050', '50', 50.0, 'Health', 50,
                0, 0, 1, 0, 0, 1, 0, 0, 0
               )"#,
        )
        .await
        .expect("seed channel");
        pool.execute(
            r#"INSERT INTO "ProgramSchedule" (
                "Id", "FixedStartTimeBehavior", "KeepMultiPartEpisodesTogether", "Name",
                "RandomStartPoint", "ShuffleScheduleItems", "TreatCollectionsAsShows"
               ) VALUES (10, 0, 0, 'Empty', 0, 0, 0)"#,
        )
        .await
        .expect("seed program schedule");
        pool.execute(
            r#"INSERT INTO "Playout" ("Id", "ChannelId", "ProgramScheduleId", "ScheduleKind", "Seed")
               VALUES (10, 50, 10, 1, 0)"#,
        )
        .await
        .expect("seed playout");
    }

    async fn seed_vaapi_profile_and_channel(pool: &Pool<Db>, vaapi_device: &str) {
        pool.execute(
            r#"INSERT OR IGNORE INTO "Resolution" ("Id", "Name", "IsCustom", "Width", "Height")
               VALUES (60, '1920x1080', 0, 1920, 1080)"#,
        )
        .await
        .expect("seed resolution");
        let vaapi = HW_ACCEL_VAAPI;
        let device = vaapi_device;
        let profile_sql = format!(
            r#"INSERT INTO "FFmpegProfile" (
                "Id", "AudioBitrate", "AudioBufferSize", "AudioChannels", "AudioFormat",
                "AudioSampleRate", "BitDepth", "HardwareAcceleration", "Name",
                "NormalizeFramerate", "NormalizeLoudnessMode", "ResolutionId",
                "ScalingBehavior", "ThreadCount", "VaapiDevice", "VaapiDriver",
                "VideoBitrate", "VideoBufferSize", "VideoFormat"
               ) VALUES (
                60, 192, 384, 2, 1, 48, 0, {vaapi}, 'Vaapi', 0, 0, 60, 0, 0,
                '{device}', 0, 2000, 4000, 1
               )"#,
        );
        sqlx::query(crate::db::assert_sql_safe(profile_sql))
            .execute(pool)
            .await
            .expect("seed vaapi profile");
        pool.execute(
            r#"INSERT INTO "Channel" (
                "Id", "UniqueId", "Number", "SortNumber", "Name", "FFmpegProfileId",
                "IdleBehavior", "MusicVideoCreditsMode", "PlayoutMode", "PlayoutSource",
                "SongVideoMode", "StreamingMode", "StreamSelectorMode", "SubtitleMode",
                "TranscodeMode"
               ) VALUES (
                60, '00000000-0000-0000-0000-000000000060', '60', 60.0, 'Vaapi', 60,
                0, 0, 1, 0, 0, 1, 0, 0, 0
               )"#,
        )
        .await
        .expect("seed channel");
    }
    /// Seed one HDR10-selected profile plus a channel that uses it.
    // Only used by the SQLite-backed HDR10 health tests below.
    async fn seed_hdr10_profile_and_channel(
        pool: &Pool<Db>,
        id: i64,
        name: &str,
        video_format: i32,
        bit_depth: i32,
        transcode_video: i32,
    ) {
        seed_hdr10_profile_with_acceleration(
            pool,
            id,
            name,
            video_format,
            bit_depth,
            transcode_video,
            0,
        )
        .await;
    }

    // Only used by the SQLite-backed HDR10 health tests below.
    #[allow(clippy::too_many_arguments)]
    async fn seed_hdr10_profile_with_acceleration(
        pool: &Pool<Db>,
        id: i64,
        name: &str,
        video_format: i32,
        bit_depth: i32,
        transcode_video: i32,
        acceleration: i32,
    ) {
        pool.execute(
            r#"INSERT OR IGNORE INTO "Resolution" ("Id", "Name", "IsCustom", "Width", "Height")
               VALUES (70, '1920x1080', 0, 1920, 1080)"#,
        )
        .await
        .expect("seed resolution");
        let profile_sql = format!(
            r#"INSERT INTO "FFmpegProfile" (
                "Id", "AudioBitrate", "AudioBufferSize", "AudioChannels", "AudioFormat",
                "AudioSampleRate", "BitDepth", "HardwareAcceleration", "Name",
                "NormalizeFramerate", "NormalizeLoudnessMode", "ResolutionId",
                "ScalingBehavior", "ThreadCount", "VaapiDriver",
                "VideoBitrate", "VideoBufferSize", "VideoFormat",
                "TranscodeVideo", "OutputDynamicRange"
               ) VALUES (
                {id}, 192, 384, 2, 1, 48, {bit_depth}, {acceleration}, '{name}', 0, 0, 70, 0, 0, 0,
                2000, 4000, {video_format}, {transcode_video}, 1
               )"#,
        );
        sqlx::query(crate::db::assert_sql_safe(profile_sql))
            .execute(pool)
            .await
            .expect("seed hdr10 profile");
        let channel_sql = format!(
            r#"INSERT INTO "Channel" (
                "Id", "UniqueId", "Number", "SortNumber", "Name", "FFmpegProfileId",
                "IdleBehavior", "MusicVideoCreditsMode", "PlayoutMode", "PlayoutSource",
                "SongVideoMode", "StreamingMode", "StreamSelectorMode", "SubtitleMode",
                "TranscodeMode"
               ) VALUES (
                {id}, '00000000-0000-0000-0000-0000000000{id}', '{id}', {id}.0, '{name}', {id},
                0, 0, 1, 0, 0, 1, 0, 0, 0
               )"#,
        );
        sqlx::query(crate::db::assert_sql_safe(channel_sql))
            .execute(pool)
            .await
            .expect("seed channel");
    }

    #[tokio::test]
    async fn output_dynamic_range_check_is_not_applicable_without_hdr10_profiles() {
        let pool = test_pool().await;
        let check = OutputDynamicRangeHealthCheck::new(pool);

        let result = check.check().await.expect("hdr10 check");
        assert_eq!(result.status, HealthCheckStatus::NotApplicable);
    }

    #[tokio::test]
    async fn output_dynamic_range_check_passes_for_a_valid_hdr10_profile() {
        let pool = test_pool().await;
        // Hevc (2), 10-bit (1), transcoding.
        seed_hdr10_profile_and_channel(&pool, 70, "Hdr10 Hevc", 2, 1, 1).await;
        let check = OutputDynamicRangeHealthCheck::new(pool);

        let result = check.check().await.expect("hdr10 check");
        assert_eq!(result.status, HealthCheckStatus::Pass);
    }

    #[tokio::test]
    async fn output_dynamic_range_check_fails_for_unsupported_codec_and_bit_depth() {
        let pool = test_pool().await;
        // H264 (1) cannot carry HDR10 static metadata.
        seed_hdr10_profile_and_channel(&pool, 71, "Hdr10 H264", 1, 1, 1).await;
        // Hevc but 8-bit (0).
        seed_hdr10_profile_and_channel(&pool, 72, "Hdr10 EightBit", 2, 0, 1).await;
        let check = OutputDynamicRangeHealthCheck::new(pool);

        let result = check.check().await.expect("hdr10 check");
        assert_eq!(result.status, HealthCheckStatus::Fail);
        assert_eq!(
            result.brief_message,
            "HDR10 output is configured but cannot be produced"
        );
        assert!(
            result
                .message
                .contains("codec cannot carry HDR10 static metadata"),
            "{}",
            result.message
        );
        assert!(
            result.message.contains("HDR10 requires 10-bit"),
            "{}",
            result.message
        );
    }

    /// Seed a channel with a playout containing one rotated video item.
    ///
    /// `transcode_video`/`video_format` decide whether the channel copies, which
    /// is the whole condition the check keys on.
    async fn seed_rotated_item_on_channel(
        pool: &Pool<Db>,
        id: i64,
        video_format: i32,
        transcode_video: i32,
        rotation: i32,
    ) {
        seed_library_path(pool).await;
        seed_hdr10_profile_with_acceleration(
            pool,
            id,
            "orientation",
            video_format,
            0,
            transcode_video,
            0,
        )
        .await;
        for statement in [
            format!(
                r#"INSERT INTO "MediaItem" ("Id", "LibraryPathId", "State") VALUES ({id}, 10, 0)"#
            ),
            format!(r#"INSERT INTO "Movie" ("Id") VALUES ({id})"#),
            format!(
                r#"INSERT INTO "MediaVersion" (
                       "Id", "DateAdded", "DateUpdated", "Duration", "Height",
                       "MovieId", "VideoScanKind", "Width"
                   ) VALUES (
                       {id}, '2026-08-08T00:00:00Z', '2026-08-08T00:00:00Z',
                       '01:00:00', 1920, {id}, 1, 1080
                   )"#
            ),
            format!(
                r#"INSERT INTO "MediaStream" (
                       "Id", "Index", "Codec", "MediaStreamKind", "Channels",
                       "Default", "Forced", "MediaVersionId", "Rotation"
                   ) VALUES ({id}, 0, 'h264', 2, 0, 1, 0, {id}, {rotation})"#
            ),
            format!(
                r#"INSERT INTO "MediaFile" ("MediaVersionId", "Path", "PathHash")
                   VALUES ({id}, '/media/portrait-{id}.mp4', 'hash-{id}')"#
            ),
            format!(
                r#"INSERT INTO "Playout" ("Id", "ChannelId", "ScheduleKind", "Seed")
                   VALUES ({id}, {id}, 1, 0)"#
            ),
            format!(
                r#"INSERT INTO "PlayoutItem" (
                       "Id", "DisableWatermarks", "FillerKind", "Finish", "GuideGroup",
                       "InPoint", "MediaItemId", "OutPoint", "PlayoutId", "Start"
                   ) VALUES (
                       {id}, 0, 0, '2026-08-08T01:00:00Z', 0, '00:00:00', {id},
                       '01:00:00', {id}, '2026-08-08T00:00:00Z'
                   )"#
            ),
        ] {
            sqlx::query(crate::db::assert_sql_safe(statement))
                .execute(pool)
                .await
                .expect("seed rotated playout item");
        }
    }

    /// ADR 0003 / `p8c-02`: MPEG-TS cannot carry a display matrix, so a
    /// stream-copied rotated item reaches clients sideways. Transcoding bakes
    /// the rotation into the pixels, so it must not be reported.
    #[tokio::test]
    async fn source_orientation_check_reports_only_copied_rotated_items() {
        let pool = test_pool().await;
        assert_eq!(
            super::run_source_orientation_check(&pool)
                .await
                .expect("no rotated items")
                .status,
            HealthCheckStatus::NotApplicable
        );

        // Rotated item on a transcoding channel: autorotation handles it.
        seed_rotated_item_on_channel(&pool, 90, 1, 1, 90).await;
        assert_eq!(
            super::run_source_orientation_check(&pool)
                .await
                .expect("transcoded rotation")
                .status,
            HealthCheckStatus::NotApplicable
        );

        // Unrotated item on a copy channel: nothing to lose.
        let pool = test_pool().await;
        seed_rotated_item_on_channel(&pool, 91, 99, 0, 0).await;
        assert_eq!(
            super::run_source_orientation_check(&pool)
                .await
                .expect("unrotated copy")
                .status,
            HealthCheckStatus::NotApplicable
        );

        // Rotated item on a copy channel: the reportable case.
        let pool = test_pool().await;
        seed_rotated_item_on_channel(&pool, 92, 99, 0, 270).await;
        let result = super::run_source_orientation_check(&pool)
            .await
            .expect("copied rotation");
        assert_eq!(result.status, HealthCheckStatus::Warning);
        assert!(
            result.message.contains("portrait-92.mp4") && result.message.contains("270"),
            "{}",
            result.message
        );
    }

    /// The same classification contract the `MySQL` integration test asserts.
    ///
    /// Duplicate profile names are seeded on purpose: they are what makes the
    /// `SELECT DISTINCT` meaningful, and they are the case the dropped `Id`
    /// tie-breaker existed for.
    #[tokio::test]
    async fn output_dynamic_range_check_classifies_every_row_shape_on_sqlite() {
        let pool = test_pool().await;
        assert_eq!(
            super::run_output_dynamic_range_check(&pool)
                .await
                .expect("no hdr10 profiles")
                .status,
            HealthCheckStatus::NotApplicable
        );

        // Two channels sharing one profile name.
        seed_hdr10_profile_with_acceleration(&pool, 80, "shared name", 1, 1, 1, 0).await;
        seed_hdr10_profile_with_acceleration(&pool, 81, "shared name", 1, 1, 1, 0).await;
        assert_eq!(
            super::run_output_dynamic_range_check(&pool)
                .await
                .expect("h264 hdr10")
                .status,
            HealthCheckStatus::Fail
        );

        let pool = test_pool().await;
        seed_hdr10_profile_with_acceleration(&pool, 82, "vaapi hdr10", 2, 1, 1, 3).await;
        assert_eq!(
            super::run_output_dynamic_range_check(&pool)
                .await
                .expect("vaapi hdr10")
                .status,
            HealthCheckStatus::Warning
        );

        let pool = test_pool().await;
        seed_hdr10_profile_with_acceleration(&pool, 83, "copy hdr10", 99, 0, 0, 0).await;
        assert_eq!(
            super::run_output_dynamic_range_check(&pool)
                .await
                .expect("copy hdr10")
                .status,
            HealthCheckStatus::Info
        );
    }

    #[tokio::test]
    async fn output_dynamic_range_check_warns_when_hdr10_demotes_to_software() {
        let pool = test_pool().await;
        // Hevc 10-bit HDR10 on VAAPI (3): no proven HDR10 graph, so the
        // pipeline demotes to software rather than emitting SDR.
        seed_hdr10_profile_with_acceleration(&pool, 74, "Hdr10 Vaapi", 2, 1, 1, 3).await;
        let check = OutputDynamicRangeHealthCheck::new(pool);

        let result = check.check().await.expect("hdr10 check");
        assert_eq!(result.status, HealthCheckStatus::Warning);
        assert_eq!(
            result.brief_message,
            "HDR10 output falls back to software encoding"
        );
        assert!(result.message.contains("Hdr10 Vaapi"), "{}", result.message);
    }

    #[tokio::test]
    async fn output_dynamic_range_check_passes_for_nvenc_hdr10() {
        let pool = test_pool().await;
        // NVENC (2) has a proven HDR10 graph.
        seed_hdr10_profile_with_acceleration(&pool, 75, "Hdr10 Nvenc", 2, 1, 1, 2).await;
        let check = OutputDynamicRangeHealthCheck::new(pool);

        let result = check.check().await.expect("hdr10 check");
        assert_eq!(result.status, HealthCheckStatus::Pass);
    }

    #[tokio::test]
    async fn output_dynamic_range_check_reports_copy_profiles_as_informational() {
        let pool = test_pool().await;
        // Video copy: the selection is inert, not wrong.
        seed_hdr10_profile_and_channel(&pool, 73, "Hdr10 Copy", 99, 0, 0).await;
        let check = OutputDynamicRangeHealthCheck::new(pool);

        let result = check.check().await.expect("hdr10 check");
        assert_eq!(result.status, HealthCheckStatus::Info);
        assert_eq!(
            result.brief_message,
            "HDR10 selection does not apply to copied video"
        );
    }
}
