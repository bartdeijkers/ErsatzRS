//! Explicit manual subtitle acquisition without downloading the source video.

use crate::media_list_operations::{
    validate_context, validate_digest, validate_echo, validate_result_size,
    validate_tool_version_echo,
};
use crate::{
    invalid, is_safe_relative_path, validate_import_request_size, validate_tool_version,
    AddonCapability, AddonMediaListOptionOperation, AddonMediaListOptionValues, ContractError,
    MAX_MEDIA_LIST_IMPORT_ARCHIVE_PATH_BYTES, MAX_MEDIA_LIST_OPERATION_TIMEOUT_SECONDS,
    MAX_MEDIA_LIST_PREPARATION_BYTES,
};
use serde::{Deserialize, Serialize};

/// Invoked with `subtitles`; only explicitly requested manual tracks are acquired.
pub const CAPABILITY_MEDIA_LIST_SUBTITLES_V1: &str = "media-list.subtitles.v1";

/// Host-owned scratch request. Language aliases are canonicalized by the host.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct MediaListSubtitlesRequest {
    pub source_url: String,
    pub provider_id: String,
    pub context_fingerprint: String,
    pub tool_version: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub options: Option<AddonMediaListOptionValues>,
    pub requested_languages: Vec<String>,
    pub staging_directory: String,
    pub maximum_file_bytes: u64,
    pub maximum_staging_bytes: u64,
    pub timeout_seconds: u32,
}

impl MediaListSubtitlesRequest {
    /// Validate identity, explicit language selection, artifact options and budgets.
    /// # Errors
    /// Rejects implicit/all-language requests, unsafe scratch paths or invalid budgets.
    pub fn validate(
        &self,
        schema: Option<&AddonCapability>,
        item_is_finite: bool,
    ) -> Result<(), ContractError> {
        validate_tool_version(&self.tool_version)?;
        validate_context(
            &self.source_url,
            &self.provider_id,
            &self.context_fingerprint,
        )?;
        validate_languages(&self.requested_languages, false)?;
        match (schema, self.options.as_ref()) {
            (Some(schema), Some(options)) => schema.validate_option_values(
                options,
                AddonMediaListOptionOperation::Artifact,
                item_is_finite,
            )?,
            (None, None) => {}
            _ => {
                return invalid(
                    "media list subtitles options",
                    "schema and options must agree",
                )
            }
        }
        if self.staging_directory.trim().is_empty()
            || self.staging_directory.len() > MAX_MEDIA_LIST_IMPORT_ARCHIVE_PATH_BYTES
            || self.staging_directory.chars().any(char::is_control)
        {
            return invalid(
                "media list subtitles staging_directory",
                "requires a bounded host-owned staging path",
            );
        }
        if self.maximum_file_bytes == 0
            || self.maximum_staging_bytes < self.maximum_file_bytes
            || self.maximum_staging_bytes > MAX_MEDIA_LIST_PREPARATION_BYTES
            || self.timeout_seconds == 0
            || self.timeout_seconds > MAX_MEDIA_LIST_OPERATION_TIMEOUT_SECONDS
        {
            return invalid(
                "media list subtitles budgets",
                "requires positive bounded file, staging and timeout limits",
            );
        }
        validate_import_request_size(self)
    }
}

/// Subtitle formats admitted by this version of the contract.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum MediaListSubtitleFormat {
    Srt,
}

/// Automatic captions and machine translations are deliberately not admitted.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum MediaListSubtitleProvenance {
    Manual,
}

/// Coordinate system established by the provider's native postprocessor.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum MediaListSubtitleTimeline {
    Source,
    Prepared,
}

/// A manual sidecar. Hosts additionally verify containment, content and digest.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct MediaListSubtitleArtifact {
    pub language: String,
    pub format: MediaListSubtitleFormat,
    pub provenance: MediaListSubtitleProvenance,
    pub timeline: MediaListSubtitleTimeline,
    pub relative_path: String,
    pub byte_length: u64,
    pub sha256: String,
}

/// Successful absence is an empty subtitle list, not a fabricated artifact.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct MediaListSubtitlesResult {
    pub provider_id: String,
    pub context_fingerprint: String,
    pub tool_version: String,
    pub subtitles: Vec<MediaListSubtitleArtifact>,
}

impl MediaListSubtitlesResult {
    /// Bind admitted source-clock sidecars to this request and its quotas.
    /// # Errors
    /// Rejects wrong identity, language, timeline, paths, sizes or digests.
    pub fn validate(&self, request: &MediaListSubtitlesRequest) -> Result<(), ContractError> {
        validate_tool_version_echo(&self.tool_version, &request.tool_version)?;
        validate_echo(
            &self.provider_id,
            &self.context_fingerprint,
            &request.provider_id,
            &request.context_fingerprint,
        )?;
        validate_artifacts(
            &self.subtitles,
            &request.requested_languages,
            MediaListSubtitleTimeline::Source,
            request.maximum_file_bytes,
            request.maximum_staging_bytes,
        )?;
        validate_result_size(self)
    }
}

fn valid_language(value: &str) -> bool {
    let mut parts = value.split('-');
    let primary = parts.next().unwrap_or_default();
    value.len() <= 35
        && (2..=3).contains(&primary.len())
        && primary.bytes().all(|b| b.is_ascii_alphabetic())
        && parts.all(|part| {
            !part.is_empty() && part.len() <= 8 && part.bytes().all(|b| b.is_ascii_alphanumeric())
        })
        && !matches!(
            primary.to_ascii_lowercase().as_str(),
            "all" | "und" | "zxx" | "mul"
        )
}

fn logical_language(value: &str) -> String {
    value
        .split('-')
        .next()
        .unwrap_or_default()
        .to_ascii_lowercase()
}

/// Validate explicit primary/fallback tags, rejecting wildcards and duplicate logical languages.
/// Hosts resolve ISO language aliases before invoking this validation.
/// # Errors
/// Rejects empty selections unless allowed, more than two tags, or invalid tags.
pub fn validate_requested_subtitle_languages(
    languages: &[String],
    allow_empty: bool,
) -> Result<(), ContractError> {
    validate_languages(languages, allow_empty)
}

/// Return the lowercase primary subtag of a validated subtitle language tag.
/// This does not map ISO aliases; hosts canonicalize aliases before comparison.
/// # Errors
/// Rejects unspecified, wildcard, malformed or oversized language tags.
pub fn subtitle_logical_language(value: &str) -> Result<String, ContractError> {
    if !valid_language(value) {
        return invalid(
            "media list subtitles language",
            "requires an explicit canonical language tag",
        );
    }
    Ok(logical_language(value))
}

pub(crate) fn validate_languages(
    languages: &[String],
    allow_empty: bool,
) -> Result<(), ContractError> {
    let unique: std::collections::BTreeSet<_> = languages
        .iter()
        .map(|value| logical_language(value))
        .collect();
    if (!allow_empty && languages.is_empty())
        || languages.len() > 2
        || unique.len() != languages.len()
        || languages.iter().any(|value| !valid_language(value))
    {
        return invalid(
            "media list subtitles languages",
            "requires at most two distinct explicit canonical languages",
        );
    }
    Ok(())
}

// The wire contract requires normalized lowercase SRT artifact names on every OS.
#[allow(clippy::case_sensitive_file_extension_comparisons)]
pub(crate) fn validate_artifacts(
    artifacts: &[MediaListSubtitleArtifact],
    requested: &[String],
    timeline: MediaListSubtitleTimeline,
    maximum_file: u64,
    maximum_total: u64,
) -> Result<(), ContractError> {
    validate_languages(requested, true)?;
    let mut languages = std::collections::BTreeSet::new();
    let mut paths = std::collections::BTreeSet::new();
    let mut total = 0_u64;
    if artifacts.len() > requested.len() {
        return invalid("media list subtitles result", "contains unrequested tracks");
    }
    for artifact in artifacts {
        let language = logical_language(&artifact.language);
        if !valid_language(&artifact.language)
            || !languages.insert(language.clone())
            || !requested
                .iter()
                .any(|requested| logical_language(requested) == language)
            || artifact.timeline != timeline
        {
            return invalid(
                "media list subtitles result",
                "requires one requested manual track per language on the expected clock",
            );
        }
        let path = &artifact.relative_path;
        if !is_safe_relative_path(path)
            || path.chars().any(char::is_control)
            || path.contains(['\\', '<', '>', '"', '|', '?', '*'])
            || path.split('/').any(|part| part.ends_with(['.', ' ']))
            || !path.ends_with(".srt")
            || !paths.insert(path)
        {
            return invalid(
                "media list subtitles relative_path",
                "requires unique safe SRT file paths",
            );
        }
        validate_digest(&artifact.sha256)?;
        total = total
            .checked_add(artifact.byte_length)
            .ok_or_else(|| ContractError::Invalid {
                field: "media list subtitles size",
                message: "size overflow".to_owned(),
            })?;
        if artifact.byte_length == 0 || artifact.byte_length > maximum_file || total > maximum_total
        {
            return invalid(
                "media list subtitles size",
                "exceeds requested artifact limits",
            );
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn request() -> MediaListSubtitlesRequest {
        MediaListSubtitlesRequest {
            source_url: "https://example.invalid/video".into(),
            provider_id: "example:video".into(),
            context_fingerprint: "a".repeat(64),
            tool_version: "2026.08.19".into(),
            options: None,
            requested_languages: vec!["en".into(), "nl".into()],
            staging_directory: "staging/subtitles".into(),
            maximum_file_bytes: 1024,
            maximum_staging_bytes: 2048,
            timeout_seconds: 30,
        }
    }

    fn artifact() -> MediaListSubtitleArtifact {
        MediaListSubtitleArtifact {
            language: "en-US".into(),
            format: MediaListSubtitleFormat::Srt,
            provenance: MediaListSubtitleProvenance::Manual,
            timeline: MediaListSubtitleTimeline::Source,
            relative_path: "subtitles.en-US.srt".into(),
            byte_length: 100,
            sha256: "b".repeat(64),
        }
    }

    #[test]
    fn manual_subtitles_require_explicit_bounded_languages() {
        let mut request = request();
        request.validate(None, true).unwrap();
        for languages in [
            vec![],
            vec!["all"],
            vec!["en.*"],
            vec!["en", "nl", "de"],
            vec!["en", "en-US"],
        ] {
            request.requested_languages = languages.into_iter().map(String::from).collect();
            assert!(request.validate(None, true).is_err());
        }
    }

    #[test]
    fn manual_subtitles_admit_absence_but_reject_wrong_context_clock_and_tracks() {
        let request = request();
        let mut result = MediaListSubtitlesResult {
            provider_id: request.provider_id.clone(),
            context_fingerprint: request.context_fingerprint.clone(),
            tool_version: request.tool_version.clone(),
            subtitles: Vec::new(),
        };
        result.validate(&request).unwrap();
        result.subtitles.push(artifact());
        result.validate(&request).unwrap();
        result.subtitles[0].timeline = MediaListSubtitleTimeline::Prepared;
        assert!(result.validate(&request).is_err());
        result.subtitles[0] = artifact();
        result.subtitles[0].language = "de".into();
        assert!(result.validate(&request).is_err());
        result.subtitles[0] = artifact();
        result.subtitles.push(artifact());
        assert!(result.validate(&request).is_err());
        result.subtitles.pop();
        result.context_fingerprint = "c".repeat(64);
        assert!(result.validate(&request).is_err());
    }

    #[test]
    fn manual_subtitles_reject_unsafe_paths_and_oversized_sidecars() {
        let request = request();
        for path in [
            "../outside.srt",
            "/outside.srt",
            "folder\\outside.srt",
            "subtitle.vtt",
        ] {
            let mut artifact = artifact();
            artifact.relative_path = path.into();
            assert!(validate_artifacts(
                &[artifact],
                &request.requested_languages,
                MediaListSubtitleTimeline::Source,
                1024,
                2048
            )
            .is_err());
        }
        let mut artifact = artifact();
        artifact.byte_length = 1025;
        assert!(validate_artifacts(
            &[artifact],
            &request.requested_languages,
            MediaListSubtitleTimeline::Source,
            1024,
            2048
        )
        .is_err());
    }
}
