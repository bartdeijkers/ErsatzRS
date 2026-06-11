use ersatzrs_domain::HardwareAccelerationKind;
use ersatzrs_ffmpeg::{FfmpegCapabilityReport, HardwareDecodeDenial};
use serde::{Deserialize, Serialize};
use sqlx::Row;

use crate::db::DbPool;

const CONFIG_KEY_HARDWARE_DECODE_DENIALS: &str = "ffmpeg.hardware.decode_denials";

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

    let parsed: Vec<PersistedHardwareDecodeDenial> = serde_json::from_str(&raw).unwrap_or_default();
    Ok(parsed
        .into_iter()
        .filter_map(PersistedHardwareDecodeDenial::into_denial)
        .collect())
}

/// Apply persisted runtime-learned denial rows to a fresh runtime report.
///
/// # Errors
///
/// Returns an error when the underlying query fails.
pub async fn apply_hardware_decode_denials(
    pool: &DbPool,
    report: &mut FfmpegCapabilityReport,
) -> Result<(), HardwareDecodeDenialError> {
    for denial in load_hardware_decode_denials(pool).await? {
        report.deny_hardware_decode(denial);
    }
    Ok(())
}

/// Persist a learned hardware decode denial and keep the in-memory report aligned.
///
/// # Errors
///
/// Returns an error when the denials cannot be encoded or the upsert fails.
pub async fn persist_hardware_decode_denial(
    pool: &DbPool,
    report: &mut FfmpegCapabilityReport,
    denial: HardwareDecodeDenial,
) -> Result<(), HardwareDecodeDenialError> {
    report.deny_hardware_decode(denial);
    let persisted: Vec<PersistedHardwareDecodeDenial> = report
        .hardware_decode_denials
        .iter()
        .map(PersistedHardwareDecodeDenial::from)
        .collect();
    let value = serde_json::to_string(&persisted).map_err(HardwareDecodeDenialError::Encode)?;
    upsert_config(pool, CONFIG_KEY_HARDWARE_DECODE_DENIALS, &value).await?;
    Ok(())
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
