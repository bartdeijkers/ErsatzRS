use ersatzrs_domain::HardwareAccelerationKind;
use ersatzrs_ffmpeg::{
    FfmpegCapabilityReport, FfmpegRuntimeReport, HardwareDecodeDenial, PixelFormat,
    SourceVideoCodec, SourceVideoStream,
};
use serde::{Deserialize, Serialize};
use sqlx::Row;
use tokio::sync::RwLock;

use crate::db::DbPool;
use crate::runtime_identity::PersistedRuntimeIdentity;

pub const CONFIG_KEY_HARDWARE_DECODE_DENIALS: &str = "ffmpeg.hardware.decode_denials";

#[derive(Debug, thiserror::Error)]
pub enum HardwareDecodeDenialError {
    #[error("database error while loading hardware decode denials: {0}")]
    Database(#[from] sqlx::Error),
    #[error("failed to encode hardware decode denials: {0}")]
    Encode(#[source] serde_json::Error),
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
struct PersistedHardwareDecodeDenial {
    acceleration: String,
    codec: String,
    bit_depth: u8,
}

impl From<&HardwareDecodeDenial> for PersistedHardwareDecodeDenial {
    fn from(value: &HardwareDecodeDenial) -> Self {
        Self {
            acceleration: acceleration_name(value.acceleration).to_owned(),
            codec: value.codec.clone(),
            bit_depth: value.bit_depth,
        }
    }
}

impl PersistedHardwareDecodeDenial {
    fn into_denial(self) -> Option<HardwareDecodeDenial> {
        Some(HardwareDecodeDenial::new(
            acceleration_from_name(&self.acceleration)?,
            self.codec,
            self.bit_depth,
        ))
    }
}

/// Persisted denial state: the denial rows plus the `ffmpeg` path they were
/// learned against. ADR 0014 clears denials when the capability report is
/// explicitly refreshed (path change, settings-save refresh, manual refresh);
/// the stored path lets startup distinguish "same binary as last run" (keep
/// the learned rows) from "binary changed since last run" (clear them).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
struct PersistedHardwareDecodeDenialState {
    #[serde(default)]
    runtime_identity: Option<PersistedRuntimeIdentity>,
    /// Legacy path-only identity. Retained solely for backwards-compatible
    /// deserialization; it is intentionally never considered reusable.
    #[serde(default)]
    ffmpeg_path: Option<String>,
    denials: Vec<PersistedHardwareDecodeDenial>,
}

fn parse_persisted_denial_state(raw: &str) -> PersistedHardwareDecodeDenialState {
    if let Ok(state) = serde_json::from_str::<PersistedHardwareDecodeDenialState>(raw) {
        return state;
    }
    // Rows written before the path was stored are a bare denial array; treat
    // them as learned against the current binary so existing installs keep
    // their rows until the next explicit refresh.
    PersistedHardwareDecodeDenialState {
        runtime_identity: None,
        ffmpeg_path: None,
        denials: serde_json::from_str(raw).unwrap_or_default(),
    }
}

/// Load persisted runtime-learned hardware decode denials.
///
/// Malformed rows are ignored so a bad historical value cannot break startup.
///
/// # Errors
///
/// Returns an error when the underlying query fails.
pub async fn load_hardware_decode_denials(
    pool: &DbPool,
) -> Result<Vec<HardwareDecodeDenial>, HardwareDecodeDenialError> {
    let Some(raw) = load_config(pool, CONFIG_KEY_HARDWARE_DECODE_DENIALS).await? else {
        return Ok(Vec::new());
    };

    Ok(parse_persisted_denial_state(&raw)
        .denials
        .into_iter()
        .filter_map(PersistedHardwareDecodeDenial::into_denial)
        .collect())
}

/// Apply persisted runtime-learned denial rows to the startup runtime report.
///
/// When the persisted rows were learned against a different `ffmpeg` binary
/// than `current_ffmpeg_path`, the path change counts as an explicit
/// capability refresh (ADR 0014): the rows are cleared instead of applied.
///
/// # Errors
///
/// Returns an error when the underlying query fails.
pub async fn apply_hardware_decode_denials(
    pool: &DbPool,
    report: &mut FfmpegCapabilityReport,
) -> Result<(), HardwareDecodeDenialError> {
    let Some(raw) = load_config(pool, CONFIG_KEY_HARDWARE_DECODE_DENIALS).await? else {
        return Ok(());
    };

    let state = parse_persisted_denial_state(&raw);
    if !state
        .runtime_identity
        .as_ref()
        .is_some_and(|identity| identity.matches(report))
    {
        tracing::info!(
            "ffmpeg runtime identity changed since hardware decode denials were learned; clearing them"
        );
        clear_hardware_decode_denials(pool).await?;
        return Ok(());
    }

    for denial in state
        .denials
        .into_iter()
        .filter_map(PersistedHardwareDecodeDenial::into_denial)
    {
        report.deny_hardware_decode(denial);
    }
    Ok(())
}

/// Clear persisted runtime-learned denials after an explicit capability
/// refresh (`FFmpeg` path change, settings-save refresh, manual refresh), per
/// ADR 0014: the underlying hardware/driver/binary may have changed, so the
/// fresh probe report decides alone until runtime evidence says otherwise.
///
/// # Errors
///
/// Returns an error when the underlying delete fails.
pub async fn clear_hardware_decode_denials(pool: &DbPool) -> Result<(), HardwareDecodeDenialError> {
    sqlx::query(r#"DELETE FROM "ConfigElement" WHERE "Key" = ?"#)
        .bind(CONFIG_KEY_HARDWARE_DECODE_DENIALS)
        .execute(pool)
        .await?;
    Ok(())
}

/// Learn one decode denial in a worker snapshot and the process-wide runtime,
/// then persist the merged runtime state.
///
/// The shared write guard is held through persistence so concurrent stream
/// supervisors cannot overwrite one another's complete denial snapshots.
/// Both in-memory reports remain updated if the database write fails.
///
/// # Errors
///
/// Returns an error when the denials cannot be encoded or the upsert fails.
pub async fn persist_hardware_decode_denial_into_runtime(
    pool: &DbPool,
    report: &mut FfmpegCapabilityReport,
    runtime: &RwLock<FfmpegRuntimeReport>,
    denial: HardwareDecodeDenial,
) -> Result<(), HardwareDecodeDenialError> {
    let mut runtime = runtime.write().await;
    if runtime_generation(report).is_some_and(|generation| generation != runtime.generation) {
        tracing::info!(
            observed_generation = ?runtime_generation(report),
            active_generation = runtime.generation,
            "ignoring stale hardware decode denial and rebuilding from the active runtime"
        );
        report.clone_from(&runtime.capabilities);
        return Ok(());
    }
    report.deny_hardware_decode(denial.clone());
    runtime.capabilities.deny_hardware_decode(denial);
    report.clone_from(&runtime.capabilities);
    persist_hardware_decode_denials(pool, &runtime.capabilities).await
}

fn runtime_generation(report: &FfmpegCapabilityReport) -> Option<u64> {
    report
        .runtime_identity
        .as_ref()
        .map(|identity| identity.generation)
}

async fn persist_hardware_decode_denials(
    pool: &DbPool,
    report: &FfmpegCapabilityReport,
) -> Result<(), HardwareDecodeDenialError> {
    let state = PersistedHardwareDecodeDenialState {
        runtime_identity: PersistedRuntimeIdentity::from_capabilities(report),
        ffmpeg_path: report
            .runtime_identity
            .as_ref()
            .map(|identity| identity.ffmpeg_path.clone()),
        denials: report
            .hardware_decode_denials
            .iter()
            .map(PersistedHardwareDecodeDenial::from)
            .collect(),
    };
    let value = serde_json::to_string(&state).map_err(HardwareDecodeDenialError::Encode)?;
    upsert_config(pool, CONFIG_KEY_HARDWARE_DECODE_DENIALS, &value).await?;
    Ok(())
}

/// Classify an abnormal `FFmpeg` exit into a runtime-learned hardware decode
/// denial (ADR 0014). Returns a denial only when the one-shot retry has not
/// been used, a hardware acceleration was selected, the source stream is
/// known, and the stderr tail matches the decode-unsupported signature class —
/// supervisors must not retry on arbitrary `FFmpeg` failures.
#[must_use]
pub fn learned_hardware_decode_denial(
    retry_already_used: bool,
    acceleration: HardwareAccelerationKind,
    source: Option<&SourceVideoStream>,
    stderr: &str,
) -> Option<HardwareDecodeDenial> {
    if retry_already_used
        || acceleration == HardwareAccelerationKind::None
        || !ffmpeg_stderr_is_hardware_decode_unsupported(stderr)
    {
        return None;
    }

    let source = source?;
    let codec = hardware_decode_denial_codec(source.codec)?;
    Some(HardwareDecodeDenial::new(
        acceleration,
        codec,
        hardware_decode_denial_bit_depth(source.pixel_format),
    ))
}

/// The `FFmpeg` stderr signature class that identifies a hardware decode the
/// runtime rejected (ADR 0014 runtime evidence, first recorded 2026-06-10).
#[must_use]
pub fn ffmpeg_stderr_is_hardware_decode_unsupported(stderr: &str) -> bool {
    let lower = stderr.to_ascii_lowercase();
    (lower.contains("doesn't support hardware accelerated") && lower.contains("decoding"))
        || lower.contains("your platform doesn't support hardware accelerated")
        || lower.contains("error submitting packet to decoder: function not implemented")
}

const fn hardware_decode_denial_codec(codec: SourceVideoCodec) -> Option<&'static str> {
    match codec {
        SourceVideoCodec::Av1 => Some("av1"),
        SourceVideoCodec::H264 => Some("h264"),
        SourceVideoCodec::Hevc => Some("hevc"),
        SourceVideoCodec::Mpeg2Video => Some("mpeg2video"),
        SourceVideoCodec::Mpeg4 => Some("mpeg4"),
        SourceVideoCodec::Vc1 => Some("vc1"),
        SourceVideoCodec::Vp8 => Some("vp8"),
        SourceVideoCodec::Vp9 => Some("vp9"),
        SourceVideoCodec::Mpeg1Video
        | SourceVideoCodec::MsMpeg4V2
        | SourceVideoCodec::MsMpeg4V3
        | SourceVideoCodec::RawVideo
        | SourceVideoCodec::Unknown => None,
    }
}

const fn hardware_decode_denial_bit_depth(pixel_format: Option<PixelFormat>) -> u8 {
    match pixel_format {
        Some(PixelFormat::Yuv420P10Le) => 10,
        Some(PixelFormat::Yuv420P | PixelFormat::Nv12 | PixelFormat::Vaapi | PixelFormat::Cuda)
        | None => 8,
    }
}

async fn load_config(
    pool: &DbPool,
    key: &str,
) -> Result<Option<String>, HardwareDecodeDenialError> {
    let row = sqlx::query(r#"SELECT "Value" FROM "ConfigElement" WHERE "Key" = ?"#)
        .bind(key)
        .fetch_optional(pool)
        .await?;
    row.map(|row| {
        row.try_get::<Option<String>, _>("Value")
            .map(Option::unwrap_or_default)
    })
    .transpose()
    .map_err(HardwareDecodeDenialError::Database)
}

async fn upsert_config(
    pool: &DbPool,
    key: &str,
    value: &str,
) -> Result<(), HardwareDecodeDenialError> {
    sqlx::query(r#"REPLACE INTO "ConfigElement" ("Key", "Value") VALUES (?, ?)"#)
        .bind(key)
        .bind(value)
        .execute(pool)
        .await?;
    Ok(())
}

const fn acceleration_name(acceleration: HardwareAccelerationKind) -> &'static str {
    match acceleration {
        HardwareAccelerationKind::None => "none",
        HardwareAccelerationKind::Qsv => "qsv",
        HardwareAccelerationKind::Nvenc => "nvenc",
        HardwareAccelerationKind::Vaapi => "vaapi",
        HardwareAccelerationKind::VideoToolbox => "videotoolbox",
        HardwareAccelerationKind::Amf => "amf",
        HardwareAccelerationKind::V4l2m2m => "v4l2m2m",
        HardwareAccelerationKind::Rkmpp => "rkmpp",
    }
}

fn acceleration_from_name(value: &str) -> Option<HardwareAccelerationKind> {
    match value.to_ascii_lowercase().as_str() {
        "none" => Some(HardwareAccelerationKind::None),
        "qsv" => Some(HardwareAccelerationKind::Qsv),
        "nvenc" => Some(HardwareAccelerationKind::Nvenc),
        "vaapi" => Some(HardwareAccelerationKind::Vaapi),
        "videotoolbox" => Some(HardwareAccelerationKind::VideoToolbox),
        "amf" => Some(HardwareAccelerationKind::Amf),
        "v4l2m2m" => Some(HardwareAccelerationKind::V4l2m2m),
        "rkmpp" => Some(HardwareAccelerationKind::Rkmpp),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[cfg(not(feature = "mysql"))]
    async fn test_pool() -> DbPool {
        let pool = sqlx::sqlite::SqlitePoolOptions::new()
            .max_connections(1)
            .connect("sqlite::memory:")
            .await
            .expect("open decode denial database");
        sqlx::query(
            r#"CREATE TABLE "ConfigElement" (
                "Key" TEXT PRIMARY KEY,
                "Value" TEXT NULL
            )"#,
        )
        .execute(&pool)
        .await
        .expect("create config table");
        pool
    }

    #[test]
    fn parses_legacy_denial_array_as_non_reusable_state() {
        let raw = r#"[{"acceleration":"nvenc","codec":"av1","bit_depth":10}]"#;
        let state = parse_persisted_denial_state(raw);

        assert_eq!(state.ffmpeg_path, None);
        assert_eq!(state.denials.len(), 1);
        assert_eq!(state.denials[0].codec, "av1");
        assert_eq!(state.runtime_identity, None);
    }

    #[test]
    fn path_only_denials_are_non_reusable_even_when_the_path_matches() {
        let raw = r#"{"ffmpeg_path":"/opt/old/ffmpeg","denials":[{"acceleration":"nvenc","codec":"av1","bit_depth":8}]}"#;
        let state = parse_persisted_denial_state(raw);

        assert_eq!(state.ffmpeg_path.as_deref(), Some("/opt/old/ffmpeg"));
        assert_eq!(state.runtime_identity, None);
    }

    #[test]
    fn malformed_denial_state_parses_to_empty() {
        let state = parse_persisted_denial_state("not json");
        assert_eq!(state.ffmpeg_path, None);
        assert!(state.denials.is_empty());
    }

    #[test]
    fn classified_decode_rejection_creates_denial_once() {
        let stderr = "Your platform doesn't support hardware accelerated AV1 decoding.";
        let mut source = SourceVideoStream::new(SourceVideoCodec::Av1);
        source.pixel_format = Some(PixelFormat::Yuv420P10Le);

        let denial = learned_hardware_decode_denial(
            false,
            HardwareAccelerationKind::Nvenc,
            Some(&source),
            stderr,
        )
        .expect("denial");
        assert_eq!(denial.acceleration, HardwareAccelerationKind::Nvenc);
        assert_eq!(denial.codec, "av1");
        assert_eq!(denial.bit_depth, 10);

        // The retry is one-shot.
        assert!(learned_hardware_decode_denial(
            true,
            HardwareAccelerationKind::Nvenc,
            Some(&source),
            stderr,
        )
        .is_none());
        // No acceleration selected means nothing to deny.
        assert!(learned_hardware_decode_denial(
            false,
            HardwareAccelerationKind::None,
            Some(&source),
            stderr,
        )
        .is_none());
        // Arbitrary failures must not trigger the fallback.
        assert!(learned_hardware_decode_denial(
            false,
            HardwareAccelerationKind::Nvenc,
            Some(&source),
            "Connection refused",
        )
        .is_none());
    }

    /// A concurrent worker can still hold the startup snapshot after another
    /// worker has published its denial. Persisting the stale snapshot must not
    /// erase the first denial from the restart state.
    #[cfg(not(feature = "mysql"))]
    #[tokio::test]
    async fn stale_worker_snapshot_persists_shared_decode_denials() {
        let pool = test_pool().await;
        let initial = FfmpegCapabilityReport::default();
        let first_denial = HardwareDecodeDenial::new(HardwareAccelerationKind::Nvenc, "av1", 10);
        let second_denial = HardwareDecodeDenial::new(HardwareAccelerationKind::Nvenc, "hevc", 10);

        let mut shared_report = FfmpegRuntimeReport::for_tests();
        shared_report.capabilities = initial.clone();
        shared_report
            .capabilities
            .deny_hardware_decode(first_denial.clone());
        persist_hardware_decode_denials(&pool, &shared_report.capabilities)
            .await
            .expect("persist first worker denial");
        let shared_runtime = RwLock::new(shared_report);

        let mut stale_worker = initial;
        persist_hardware_decode_denial_into_runtime(
            &pool,
            &mut stale_worker,
            &shared_runtime,
            second_denial.clone(),
        )
        .await
        .expect("persist merged worker denials");

        assert!(stale_worker.hardware_decode_denied(HardwareAccelerationKind::Nvenc, "av1", 10));
        assert!(stale_worker.hardware_decode_denied(HardwareAccelerationKind::Nvenc, "hevc", 10));

        let persisted = load_hardware_decode_denials(&pool)
            .await
            .expect("reload persisted denials");
        assert_eq!(persisted.len(), 2);
        assert!(persisted.contains(&first_denial));
        assert!(persisted.contains(&second_denial));
        let shared = shared_runtime.read().await;
        assert!(shared.capabilities.hardware_decode_denied(
            HardwareAccelerationKind::Nvenc,
            "av1",
            10
        ));
        assert!(shared.capabilities.hardware_decode_denied(
            HardwareAccelerationKind::Nvenc,
            "hevc",
            10
        ));
    }

    #[cfg(not(feature = "mysql"))]
    #[tokio::test]
    async fn a_previous_runtime_generation_cannot_poison_active_decode_capabilities() {
        let pool = test_pool().await;
        let identity = ersatzrs_ffmpeg::FfmpegRuntimeIdentity {
            generation: 1,
            ffmpeg_path: "/opt/ffmpeg".to_owned(),
            ffmpeg_sha256: Some("ffmpeg-digest".to_owned()),
            ffprobe_path: "/opt/ffprobe".to_owned(),
            ffprobe_sha256: Some("ffprobe-digest".to_owned()),
            selected_nvidia_device: None,
        };
        let mut stale_worker = FfmpegCapabilityReport {
            runtime_identity: Some(identity.clone()),
            ..FfmpegCapabilityReport::default()
        };
        let mut active = FfmpegRuntimeReport::for_tests();
        active.capabilities.runtime_identity = Some(ersatzrs_ffmpeg::FfmpegRuntimeIdentity {
            generation: 2,
            ..identity
        });
        active.generation = 2;
        let active_runtime = RwLock::new(active);

        persist_hardware_decode_denial_into_runtime(
            &pool,
            &mut stale_worker,
            &active_runtime,
            HardwareDecodeDenial::new(HardwareAccelerationKind::Nvenc, "hevc", 10),
        )
        .await
        .expect("stale feedback is ignored");

        assert_eq!(runtime_generation(&stale_worker), Some(2));
        assert!(stale_worker.hardware_decode_denials.is_empty());
        assert!(load_hardware_decode_denials(&pool)
            .await
            .expect("load decode state")
            .is_empty());
    }
}
