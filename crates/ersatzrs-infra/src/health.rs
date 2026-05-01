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

/// `ConfigElement` key containing the latest health check result set as JSON.
pub const CONFIG_KEY_HEALTH_RESULTS: &str = "health.results";
/// `ConfigElement` key containing the latest health check summary as JSON.
pub const CONFIG_KEY_HEALTH_SUMMARY: &str = "health.summary";

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
pub fn make_health_check_service(
    pool: Pool<Db>,
    ffmpeg_runtime: FfmpegRuntimeReport,
    schema_status: DatabaseSchemaStatus,
) -> HealthCheckService {
    HealthCheckService::new(vec![
        Box::new(DowngradeHealthCheck::new(schema_status)),
        Box::new(FFmpegVersionHealthCheck::new(ffmpeg_runtime.clone())),
        Box::new(FFmpegCapabilitiesHealthCheck::new(ffmpeg_runtime.clone())),
        Box::new(HardwareAccelerationHealthCheck::new(
            pool.clone(),
            ffmpeg_runtime.clone(),
        )),
        Box::new(MovieMetadataHealthCheck::new(pool.clone())),
        Box::new(EpisodeMetadataHealthCheck::new(pool.clone())),
        Box::new(ZeroDurationHealthCheck::new(pool.clone())),
        Box::new(FileNotFoundHealthCheck::new(pool.clone())),
        Box::new(UnavailableHealthCheck::new(pool.clone())),
        Box::new(EmptyScheduleHealthCheck::new(pool.clone())),
        Box::new(VaapiDriverHealthCheck::new(pool, ffmpeg_runtime)),
    ])
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

struct FFmpegVersionHealthCheck {
    runtime: FfmpegRuntimeReport,
}

impl FFmpegVersionHealthCheck {
    fn new(runtime: FfmpegRuntimeReport) -> Self {
        Self { runtime }
    }
}

#[async_trait]
impl HealthCheck for FFmpegVersionHealthCheck {
    fn title(&self) -> &'static str {
        "FFmpeg Version"
    }

    async fn check(&self) -> Result<HealthCheckResult> {
        for tool in [&self.runtime.ffmpeg, &self.runtime.ffprobe] {
            if !tool.version.satisfies(REQUIRED_MAJOR, REQUIRED_MINOR) {
                return Ok(HealthCheckResult::new(
                    self.title(),
                    HealthCheckStatus::Fail,
                    format!(
                        "{} version {} is too old; please install {}.{} or newer!",
                        tool.tool, tool.version, REQUIRED_MAJOR, REQUIRED_MINOR
                    ),
                    format!("{} version is too old", tool.tool),
                ));
            }
        }

        Ok(HealthCheckResult::pass(self.title()))
    }
}

struct FFmpegCapabilitiesHealthCheck {
    runtime: FfmpegRuntimeReport,
}

impl FFmpegCapabilitiesHealthCheck {
    fn new(runtime: FfmpegRuntimeReport) -> Self {
        Self { runtime }
    }
}

#[async_trait]
impl HealthCheck for FFmpegCapabilitiesHealthCheck {
    fn title(&self) -> &'static str {
        "FFmpeg Capabilities"
    }

    async fn check(&self) -> Result<HealthCheckResult> {
        let missing: Vec<String> = self
            .runtime
            .issues
            .iter()
            .filter(|issue| issue.is_error())
            .map(runtime_issue_name)
            .collect();

        if missing.is_empty() {
            return Ok(HealthCheckResult::not_applicable(self.title()));
        }

        Ok(HealthCheckResult::new(
            self.title(),
            HealthCheckStatus::Fail,
            format!(
                "FFmpeg is missing required capabilities and will NOT work correctly: [{}]",
                missing.join(", ")
            ),
            "FFmpeg is missing required capabilities and will NOT work correctly",
        ))
    }
}

struct HardwareAccelerationHealthCheck {
    pool: Pool<Db>,
    runtime: FfmpegRuntimeReport,
}

impl HardwareAccelerationHealthCheck {
    fn new(pool: Pool<Db>, runtime: FfmpegRuntimeReport) -> Self {
        Self { pool, runtime }
    }
}

#[async_trait]
impl HealthCheck for HardwareAccelerationHealthCheck {
    fn title(&self) -> &'static str {
        "Hardware Acceleration"
    }

    async fn check(&self) -> Result<HealthCheckResult> {
        let supported = supported_acceleration_kinds(&self.runtime);
        if supported.is_empty() {
            return Ok(HealthCheckResult::new(
                self.title(),
                HealthCheckStatus::Info,
                "No compatible hardware acceleration kinds are supported by ffmpeg",
                "FFmpeg does not support hardware acceleration",
            ));
        }

        let rows = sqlx::query(
            r#"SELECT c."Number", c."Name", p."HardwareAcceleration"
               FROM "Channel" c
               INNER JOIN "FFmpegProfile" p ON p."Id" = c."FFmpegProfileId"
               WHERE c."StreamingMode" != ?
               ORDER BY c."SortNumber", c."Number", c."Id""#,
        )
        .bind(STREAMING_MODE_HLS_DIRECT)
        .fetch_all(&self.pool)
        .await
        .map_err(sqlx_error)?;

        let bad_channels: Vec<String> = rows
            .iter()
            .filter_map(|row| {
                let acceleration = row.try_get::<i32, _>("HardwareAcceleration").ok()?;
                (!supported.contains(&acceleration)).then(|| {
                    let number = row.try_get::<Option<String>, _>("Number").ok().flatten();
                    let name = row.try_get::<Option<String>, _>("Name").ok().flatten();
                    format!(
                        "{} - {}",
                        number.unwrap_or_else(|| "?".to_owned()),
                        name.unwrap_or_else(|| "Unnamed".to_owned())
                    )
                })
            })
            .collect();

        if bad_channels.is_empty() {
            return Ok(HealthCheckResult::pass(self.title()));
        }

        Ok(HealthCheckResult::new(
            self.title(),
            HealthCheckStatus::Warning,
            format!(
                "The following channels use ffmpeg profiles that are not configured for hardware acceleration ({}): {}",
                supported.iter().map(|kind| acceleration_name(*kind)).collect::<Vec<_>>().join(", "),
                bad_channels.join(", ")
            ),
            format!(
                "{} channels are not configured for hardware acceleration",
                bad_channels.len()
            ),
        ))
    }
}

struct VaapiDriverHealthCheck {
    pool: Pool<Db>,
    runtime: FfmpegRuntimeReport,
}

impl VaapiDriverHealthCheck {
    fn new(pool: Pool<Db>, runtime: FfmpegRuntimeReport) -> Self {
        Self { pool, runtime }
    }
}

#[async_trait]
impl HealthCheck for VaapiDriverHealthCheck {
    fn title(&self) -> &'static str {
        "VAAPI Driver"
    }

    async fn check(&self) -> Result<HealthCheckResult> {
        let rows = sqlx::query(
            r#"SELECT DISTINCT p."Name", p."VaapiDevice", p."VaapiDriver"
               FROM "Channel" c
               INNER JOIN "FFmpegProfile" p ON p."Id" = c."FFmpegProfileId"
               WHERE p."HardwareAcceleration" = ?
               ORDER BY p."Name", p."Id""#,
        )
        .bind(HW_ACCEL_VAAPI)
        .fetch_all(&self.pool)
        .await
        .map_err(sqlx_error)?;

        if rows.is_empty() {
            return Ok(HealthCheckResult::not_applicable(self.title()));
        }

        if !self.runtime.capabilities.has_hwaccel("vaapi") {
            return Ok(HealthCheckResult::new(
                self.title(),
                HealthCheckStatus::Fail,
                "FFmpeg Profile is configured for VAAPI, but ffmpeg reports no VAAPI hardware acceleration capability. Hardware Acceleration WILL NOT WORK as configured.",
                "Hardware acceleration WILL NOT WORK...",
            ));
        }

        let default_driver_count = rows
            .iter()
            .filter(|row| {
                row.try_get::<i32, _>("VaapiDriver")
                    .is_ok_and(|driver| driver == VAAPI_DRIVER_AUTO)
            })
            .count();

        if default_driver_count > 0 {
            return Ok(HealthCheckResult::new(
                self.title(),
                HealthCheckStatus::Info,
                format!(
                    "{} FFmpeg Profile{} set to use Default VAAPI Driver; selecting iHD (Gen 8+) or i965 (up to Gen 9) may offer better performance with Intel iGPU",
                    default_driver_count,
                    if default_driver_count > 1 { "s are" } else { " is" }
                ),
                "Default VAAPI driver is not optimal",
            ));
        }

        Ok(HealthCheckResult::pass(self.title()))
    }
}

struct FileNotFoundHealthCheck {
    pool: Pool<Db>,
}

impl FileNotFoundHealthCheck {
    fn new(pool: Pool<Db>) -> Self {
        Self { pool }
    }
}

#[async_trait]
impl HealthCheck for FileNotFoundHealthCheck {
    fn title(&self) -> &'static str {
        "File Not Found"
    }

    async fn check(&self) -> Result<HealthCheckResult> {
        media_state_result(
            &self.pool,
            self.title(),
            STATUS_FILE_NOT_FOUND,
            "There are {count} items that do not exist on disk, including the following: {items}",
            "There are {count} items that do not exist on disk",
            Some("media/trash"),
        )
        .await
    }
}

struct UnavailableHealthCheck {
    pool: Pool<Db>,
}

impl UnavailableHealthCheck {
    fn new(pool: Pool<Db>) -> Self {
        Self { pool }
    }
}

#[async_trait]
impl HealthCheck for UnavailableHealthCheck {
    fn title(&self) -> &'static str {
        "Unavailable"
    }

    async fn check(&self) -> Result<HealthCheckResult> {
        media_state_result(
            &self.pool,
            self.title(),
            STATUS_UNAVAILABLE,
            "There are {count} items that are unavailable because ErsatzTV cannot find them on disk, including the following: {items}",
            "There are {count} items that are unavailable",
            Some("search?query=state%3aUnavailable"),
        )
        .await
    }
}

struct EmptyScheduleHealthCheck {
    pool: Pool<Db>,
}

impl EmptyScheduleHealthCheck {
    fn new(pool: Pool<Db>) -> Self {
        Self { pool }
    }
}

#[async_trait]
impl HealthCheck for EmptyScheduleHealthCheck {
    fn title(&self) -> &'static str {
        "Empty Schedule"
    }

    async fn check(&self) -> Result<HealthCheckResult> {
        let rows = sqlx::query(
            r#"SELECT ps."Name"
               FROM "ProgramSchedule" ps
               WHERE NOT EXISTS (
                   SELECT 1 FROM "ProgramScheduleItem" psi
                   WHERE psi."ProgramScheduleId" = ps."Id"
               )
               AND (
                   EXISTS (
                       SELECT 1 FROM "Playout" p
                       WHERE p."ProgramScheduleId" = ps."Id"
                   )
                   OR EXISTS (
                       SELECT 1 FROM "ProgramScheduleAlternate" psa
                       WHERE psa."ProgramScheduleId" = ps."Id"
                   )
               )
               ORDER BY ps."Name", ps."Id""#,
        )
        .fetch_all(&self.pool)
        .await
        .map_err(sqlx_error)?;

        if rows.is_empty() {
            return Ok(HealthCheckResult::pass(self.title()));
        }

        let names: Vec<String> = rows
            .iter()
            .map(|row| {
                row.try_get::<Option<String>, _>("Name")
                    .ok()
                    .flatten()
                    .unwrap_or_else(|| "Unnamed".to_owned())
            })
            .collect();

        Ok(HealthCheckResult::new(
            self.title(),
            HealthCheckStatus::Warning,
            format!(
                "There are {} empty schedules in use, which are NOT supported and WILL cause errors, including: {}",
                names.len(),
                names.join(", ")
            ),
            format!(
                "There are {} empty schedules in use, which are NOT supported and WILL cause errors",
                names.len()
            ),
        ))
    }
}

struct ZeroDurationHealthCheck {
    pool: Pool<Db>,
}

impl ZeroDurationHealthCheck {
    fn new(pool: Pool<Db>) -> Self {
        Self { pool }
    }
}

#[async_trait]
impl HealthCheck for ZeroDurationHealthCheck {
    fn title(&self) -> &'static str {
        "Zero Duration"
    }

    async fn check(&self) -> Result<HealthCheckResult> {
        let rows = sqlx::query(
            r#"SELECT mf."Path"
               FROM "MediaVersion" mv
               INNER JOIN "MediaFile" mf ON mf."MediaVersionId" = mv."Id"
               WHERE mv."Duration" = '00:00:00'
                  OR mv."Duration" LIKE '00:00:00.%'
               ORDER BY mf."Path", mf."Id""#,
        )
        .fetch_all(&self.pool)
        .await
        .map_err(sqlx_error)?;

        if rows.is_empty() {
            return Ok(HealthCheckResult::pass(self.title()));
        }

        let paths = rows
            .iter()
            .take(5)
            .filter_map(|row| row.try_get::<Option<String>, _>("Path").ok().flatten())
            .collect::<Vec<_>>();

        Ok(HealthCheckResult::new(
            self.title(),
            HealthCheckStatus::Warning,
            format!(
                "There are {} files with zero duration, including the following: {}",
                rows.len(),
                paths.join(", ")
            ),
            format!("There are {} files with zero duration", rows.len()),
        ))
    }
}

struct MovieMetadataHealthCheck {
    pool: Pool<Db>,
}

impl MovieMetadataHealthCheck {
    fn new(pool: Pool<Db>) -> Self {
        Self { pool }
    }
}

#[async_trait]
impl HealthCheck for MovieMetadataHealthCheck {
    fn title(&self) -> &'static str {
        "Movie Metadata"
    }

    async fn check(&self) -> Result<HealthCheckResult> {
        missing_metadata_result(
            &self.pool,
            self.title(),
            "Movie",
            "MovieMetadata",
            "MovieId",
            "movies",
        )
        .await
    }
}

struct EpisodeMetadataHealthCheck {
    pool: Pool<Db>,
}

impl EpisodeMetadataHealthCheck {
    fn new(pool: Pool<Db>) -> Self {
        Self { pool }
    }
}

#[async_trait]
impl HealthCheck for EpisodeMetadataHealthCheck {
    fn title(&self) -> &'static str {
        "Episode Metadata"
    }

    async fn check(&self) -> Result<HealthCheckResult> {
        missing_metadata_result(
            &self.pool,
            self.title(),
            "Episode",
            "EpisodeMetadata",
            "EpisodeId",
            "episodes",
        )
        .await
    }
}

struct DowngradeHealthCheck {
    schema_status: DatabaseSchemaStatus,
}

impl DowngradeHealthCheck {
    fn new(schema_status: DatabaseSchemaStatus) -> Self {
        Self { schema_status }
    }
}

#[async_trait]
impl HealthCheck for DowngradeHealthCheck {
    fn title(&self) -> &'static str {
        "ErsatzTV Downgrade"
    }

    async fn check(&self) -> Result<HealthCheckResult> {
        if self.schema_status.failure_count() == 0 {
            return Ok(HealthCheckResult::not_applicable(self.title()));
        }

        Ok(HealthCheckResult::new(
            self.title(),
            HealthCheckStatus::Fail,
            "Downgrade detected; THIS IS NOT SUPPORTED AND WILL IMPACT STABILITY",
            "Downgrade detected",
        )
        .with_link("https://ersatztv.org/docs/installation/#downgrading"))
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
    let count =
        sqlx::query_scalar::<_, i64>(r#"SELECT COUNT(*) FROM "MediaItem" WHERE "State" = ?"#)
            .bind(state)
            .fetch_one(pool)
            .await
            .map_err(sqlx_error)?;

    if count == 0 {
        return Ok(HealthCheckResult::pass(title));
    }

    let rows = sqlx::query(
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
           WHERE mi."State" = ?
           ORDER BY mi."Id", mf."Id"
           LIMIT 5"#,
    )
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
    let count = sqlx::query_scalar::<_, i64>(&count_sql)
        .fetch_one(pool)
        .await
        .map_err(sqlx_error)?;

    if count == 0 {
        return Ok(HealthCheckResult::pass(title));
    }

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
           LIMIT 5"#
    );
    let rows = sqlx::query(&paths_sql)
        .fetch_all(pool)
        .await
        .map_err(sqlx_error)?;
    let folders = rows
        .iter()
        .filter_map(|row| row.try_get::<Option<String>, _>("Path").ok().flatten())
        .filter_map(|path| parent_folder(&path))
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

fn parent_folder(path: &str) -> Option<String> {
    Path::new(path)
        .parent()
        .map(|parent| parent.to_string_lossy().into_owned())
}

#[allow(clippy::needless_pass_by_value)]
fn sqlx_error(error: sqlx::Error) -> HealthCheckError {
    HealthCheckError::Database(error.to_string())
}

#[cfg(test)]
mod tests {
    use std::collections::BTreeSet;

    use ersatzrs_ffmpeg::FfmpegRuntimeReport;
    use sqlx::Executor;

    use super::*;

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
}
