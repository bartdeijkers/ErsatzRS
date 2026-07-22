//! FFmpeg-related `ConfigElement` keys and typed accessors.
//!
//! Mirrors the embedded-subtitle keys in
//! `ErsatzTV.Core/Domain/ConfigElementKey.cs`:
//! `ffmpeg.use_embedded_subtitles` (default `true`) and
//! `ffmpeg.extract_embedded_subtitles` (default `false`). ErsatzTV stores bool
//! `ConfigElement` values via `bool.ToString()` ("True"/"False"); ErsatzRS
//! reads them the same lenient way as `health.rs` does for `ffmpeg.save_reports`
//! (`eq_ignore_ascii_case("true") || "1"`).

use ersatzrs_core::language_codes;
use ersatzrs_core::repositories::Result;
use ersatzrs_domain::{
    StreamingMode, CONFIG_KEY_FFMPEG_FALLBACK_SUBTITLE_LANGUAGE_CODE,
    CONFIG_KEY_FFMPEG_PREFERRED_LANGUAGE_CODE, CONFIG_KEY_FFMPEG_PREFERRED_SUBTITLE_LANGUAGE_CODE,
};
use sqlx::{Pool, Row};

use crate::db::Db;

/// `ffmpeg.use_embedded_subtitles` — gate the whole embedded-subtitle feature.
pub const CONFIG_KEY_FFMPEG_USE_EMBEDDED_SUBTITLES: &str = "ffmpeg.use_embedded_subtitles";

/// `ffmpeg.extract_embedded_subtitles` — extract subtitles/fonts during scan.
pub const CONFIG_KEY_FFMPEG_EXTRACT_EMBEDDED_SUBTITLES: &str = "ffmpeg.extract_embedded_subtitles";

/// Global language preferences used by built-in playback stream selection.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PlaybackLanguagePreferences {
    pub default_audio_language: Option<String>,
    pub default_subtitle_language: Option<String>,
    pub fallback_subtitle_language: Option<String>,
}

impl Default for PlaybackLanguagePreferences {
    fn default() -> Self {
        Self {
            default_audio_language: Some("eng".to_owned()),
            default_subtitle_language: None,
            fallback_subtitle_language: None,
        }
    }
}

impl PlaybackLanguagePreferences {
    /// Resolve the built-in audio preference. HLS Direct retains all source
    /// streams only when neither a language nor a title is explicitly selected.
    #[must_use]
    pub fn audio_language(
        &self,
        item_language: Option<&str>,
        channel_language: Option<&str>,
        preferred_title: Option<&str>,
        streaming_mode: StreamingMode,
    ) -> Option<String> {
        let explicit_language = non_blank(item_language)
            .or_else(|| non_blank(channel_language))
            .map(str::to_owned);
        if explicit_language.is_some() {
            return explicit_language;
        }

        if streaming_mode == StreamingMode::HttpLiveStreamingDirect
            && non_blank(preferred_title).is_none()
        {
            return None;
        }

        self.default_audio_language.clone()
    }

    /// Resolve the ordered built-in subtitle language preference. Equivalent
    /// ISO-639 primary and fallback values are emitted only once.
    #[must_use]
    pub fn subtitle_languages(
        &self,
        item_language: Option<&str>,
        channel_language: Option<&str>,
    ) -> Vec<String> {
        let primary = non_blank(item_language)
            .or_else(|| non_blank(channel_language))
            .map(str::to_owned)
            .or_else(|| self.default_subtitle_language.clone());
        let mut result = primary.into_iter().collect::<Vec<_>>();
        if let Some(fallback) = self.fallback_subtitle_language.as_deref() {
            let duplicate = result
                .first()
                .is_some_and(|primary| languages_are_equivalent(primary, fallback));
            if !duplicate {
                result.push(fallback.to_owned());
            }
        }
        result
    }
}

fn non_blank(value: Option<&str>) -> Option<&str> {
    value.map(str::trim).filter(|value| !value.is_empty())
}

fn languages_are_equivalent(left: &str, right: &str) -> bool {
    let left = language_codes::all_language_codes([left]);
    let right = language_codes::all_language_codes([right]);
    left.iter().any(|code| right.contains(code))
}

async fn load_optional_string(pool: &Pool<Db>, key: &str) -> Result<Option<String>> {
    let row = sqlx::query(r#"SELECT "Value" FROM "ConfigElement" WHERE "Key" = ?"#)
        .bind(key)
        .fetch_optional(pool)
        .await?;
    let value = match row {
        Some(row) => row.try_get::<Option<String>, _>("Value")?,
        None => None,
    };
    Ok(value.and_then(|value| {
        let value = value.trim();
        (!value.is_empty()).then(|| value.to_owned())
    }))
}

/// Load global audio and subtitle language preferences from `ConfigElement`.
///
/// # Errors
///
/// Returns an error when any underlying configuration query fails.
pub async fn playback_language_preferences(pool: &Pool<Db>) -> Result<PlaybackLanguagePreferences> {
    let default_audio_language =
        load_optional_string(pool, CONFIG_KEY_FFMPEG_PREFERRED_LANGUAGE_CODE)
            .await?
            .or_else(|| Some("eng".to_owned()));
    let default_subtitle_language =
        load_optional_string(pool, CONFIG_KEY_FFMPEG_PREFERRED_SUBTITLE_LANGUAGE_CODE).await?;
    let fallback_subtitle_language =
        load_optional_string(pool, CONFIG_KEY_FFMPEG_FALLBACK_SUBTITLE_LANGUAGE_CODE).await?;
    Ok(PlaybackLanguagePreferences {
        default_audio_language,
        default_subtitle_language,
        fallback_subtitle_language,
    })
}

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

    #[test]
    fn language_preferences_apply_precedence_and_deduplicate_aliases() {
        let preferences = PlaybackLanguagePreferences {
            default_audio_language: Some("eng".to_owned()),
            default_subtitle_language: Some("nld".to_owned()),
            fallback_subtitle_language: Some("en".to_owned()),
        };

        assert_eq!(
            preferences.audio_language(None, None, None, StreamingMode::TransportStream),
            Some("eng".to_owned())
        );
        assert_eq!(
            preferences.audio_language(None, None, None, StreamingMode::HttpLiveStreamingDirect),
            None
        );
        assert_eq!(
            preferences.subtitle_languages(Some("eng"), None),
            vec!["eng".to_owned()]
        );
        assert_eq!(
            preferences.subtitle_languages(None, Some("jpn")),
            vec!["jpn".to_owned(), "en".to_owned()]
        );
        assert_eq!(
            preferences.subtitle_languages(None, None),
            vec!["nld".to_owned(), "en".to_owned()]
        );
    }

    #[test]
    fn hls_direct_title_only_audio_preference_uses_global_language() {
        let preferences = PlaybackLanguagePreferences::default();

        assert_eq!(
            preferences.audio_language(
                None,
                None,
                Some("commentary"),
                StreamingMode::HttpLiveStreamingDirect
            ),
            Some("eng".to_owned())
        );
        assert_eq!(
            preferences.audio_language(
                None,
                None,
                Some("  "),
                StreamingMode::HttpLiveStreamingDirect
            ),
            None
        );
    }
}
