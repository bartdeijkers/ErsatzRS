//! FFmpeg-related `ConfigElement` keys and typed accessors.
//!
//! Mirrors the embedded-subtitle keys in
//! `ErsatzTV.Core/Domain/ConfigElementKey.cs`:
//! `ffmpeg.use_embedded_subtitles` (default `true`) and
//! `ffmpeg.extract_embedded_subtitles` (default `false`). ErsatzTV stores bool
//! `ConfigElement` values via `bool.ToString()` ("True"/"False"); ErsatzRS
//! reads them the same lenient way as `health.rs` does for `ffmpeg.save_reports`
//! (`eq_ignore_ascii_case("true") || "1"`).

use ersatzrs_core::repositories::Result;
use sqlx::{Pool, Row};

use crate::db::Db;

/// `ffmpeg.use_embedded_subtitles` — gate the whole embedded-subtitle feature.
pub const CONFIG_KEY_FFMPEG_USE_EMBEDDED_SUBTITLES: &str = "ffmpeg.use_embedded_subtitles";

/// `ffmpeg.extract_embedded_subtitles` — extract subtitles/fonts during scan.
pub const CONFIG_KEY_FFMPEG_EXTRACT_EMBEDDED_SUBTITLES: &str = "ffmpeg.extract_embedded_subtitles";

fn parse_bool(raw: &str) -> bool {
    raw.eq_ignore_ascii_case("true") || raw == "1"
}

async fn load_bool(pool: &Pool<Db>, key: &str, default: bool) -> Result<bool> {
    let row = sqlx::query(r#"SELECT "Value" FROM "ConfigElement" WHERE "Key" = ?"#)
        .bind(key)
        .fetch_optional(pool)
        .await?;
    Ok(row
        .and_then(|row| row.try_get::<Option<String>, _>("Value").ok().flatten())
        .map_or(default, |raw| parse_bool(&raw)))
}

/// Whether the embedded-subtitle feature is enabled. ErsatzTV default: `true`.
///
/// # Errors
///
/// Returns an error when the underlying query fails.
pub async fn use_embedded_subtitles(pool: &Pool<Db>) -> Result<bool> {
    load_bool(pool, CONFIG_KEY_FFMPEG_USE_EMBEDDED_SUBTITLES, true).await
}

/// Whether embedded subtitles/fonts are extracted during scan. ErsatzTV
/// default: `false`.
///
/// # Errors
///
/// Returns an error when the underlying query fails.
pub async fn extract_embedded_subtitles(pool: &Pool<Db>) -> Result<bool> {
    load_bool(pool, CONFIG_KEY_FFMPEG_EXTRACT_EMBEDDED_SUBTITLES, false).await
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_bool_accepts_etv_and_numeric_encodings() {
        for raw in ["True", "true", "TRUE", "1"] {
            assert!(parse_bool(raw), "expected {raw} to parse as true");
        }
        for raw in ["False", "false", "0", "", "no"] {
            assert!(!parse_bool(raw), "expected {raw} to parse as false");
        }
    }
}
