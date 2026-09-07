//! Options-aware operations separate from frozen import and playback contracts.

use crate::{
    invalid, is_safe_relative_path, validate_import_request_size, validate_provider_id,
    AddonCapability, AddonMediaListOptionOperation, AddonMediaListOptionValues, ContractError,
    MediaListEnrichRequest, MAX_MEDIA_LIST_IMPORT_ARCHIVE_PATH_BYTES,
};
use serde::{Deserialize, Serialize};

/// Invoked with `enrich-options`.
pub const CAPABILITY_MEDIA_LIST_ENRICH_V1: &str = "media-list.enrich.v1";
/// Invoked with `test-access`.
pub const CAPABILITY_MEDIA_LIST_ACCESS_CHECK_V1: &str = "media-list.access-check.v1";
/// Invoked with `prepare`.
pub const CAPABILITY_MEDIA_LIST_PREPARE_V1: &str = "media-list.prepare.v1";
/// Generic finite download with optional provider options, invoked with `prepare`.
pub const CAPABILITY_MEDIA_LIST_PREPARE_V2: &str = "media-list.prepare.v2";
/// Metadata-only source interval discovery, invoked with `interval-metadata`.
pub const CAPABILITY_MEDIA_LIST_INTERVAL_METADATA_V1: &str = "media-list.interval-metadata.v1";
/// Bound source exclusions before host normalization and publication.
pub const MAX_MEDIA_LIST_INTERVALS: usize = 1024;
/// Read-only local runtime version probe invoked with `runtime-info`.
pub const CAPABILITY_MEDIA_LIST_ITEM_RUNTIME_V1: &str = "media-list.item-runtime.v1";
/// Invoked with `play`, preserving the existing URL, seek and height variables.
pub const CAPABILITY_REMOTE_STREAM_PLAY_V3: &str = "remote-stream.play.v3";
/// JSON effective options envelope supplied only to playback v3.
pub const ENV_MEDIA_LIST_OPTIONS: &str = "ERSATZRS_MEDIA_LIST_OPTIONS";
/// Maximum serialized access-check or preparation result size.
pub const MAX_MEDIA_LIST_OPERATION_OUTPUT_BYTES: usize = 64 * 1024;
/// Maximum preparation duration and retry delay in seconds.
pub const MAX_MEDIA_LIST_OPERATION_TIMEOUT_SECONDS: u32 = 3600;
/// Protocol ceiling; hosts apply lower operational quotas separately.
pub const MAX_MEDIA_LIST_PREPARATION_BYTES: u64 = i64::MAX as u64;
/// Maximum preparation video height.
pub const MAX_MEDIA_LIST_PREPARATION_VIDEO_HEIGHT: u32 = 4320;

/// Empty request for a local version probe; no source or network access is needed.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct MediaListItemRuntimeInfoRequest {}

/// Bounded local tool identity used to invalidate item operation contexts.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct MediaListItemRuntimeInfo {
    pub tool_version: String,
}

impl MediaListItemRuntimeInfo {
    /// Validate a version identifier without paths or diagnostic prose.
    /// # Errors
    /// Rejects empty, non-ASCII, oversized or non-identifier versions.
    pub fn validate(&self) -> Result<(), ContractError> {
        validate_tool_version(&self.tool_version)
    }
}

/// Validate the provider-neutral tool version carried by item operations.
/// # Errors
/// Rejects anything beyond 128 ASCII letters, digits, dots, underscores or hyphens.
pub fn validate_tool_version(value: &str) -> Result<(), ContractError> {
    if value.is_empty()
        || value.len() > 128
        || !value
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || b"._-".contains(&byte))
    {
        return invalid(
            "media list operation tool_version",
            "requires a bounded ASCII version identifier",
        );
    }
    Ok(())
}

/// Legacy enrichment request wrapped with effective typed options.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct MediaListEnrichOptionsRequest {
    pub request: MediaListEnrichRequest,
    pub options: AddonMediaListOptionValues,
}
impl MediaListEnrichOptionsRequest {
    /// Validate identity, enrichment options and request size.
    /// # Errors
    /// Rejects invalid legacy requests or effective options.
    pub fn validate(
        &self,
        schema: &AddonCapability,
        item_is_finite: bool,
    ) -> Result<(), ContractError> {
        self.request.validate()?;
        schema.validate_option_values(
            &self.options,
            AddonMediaListOptionOperation::Enrich,
            item_is_finite,
        )?;
        validate_import_request_size(self)
    }
}

/// Access check without credential bytes or browser profile paths.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct MediaListAccessCheckRequest {
    pub source_url: String,
    pub provider_id: String,
    pub context_fingerprint: String,
    pub tool_version: String,
    pub options: AddonMediaListOptionValues,
}
impl MediaListAccessCheckRequest {
    /// Validate identity, context and access-check options.
    /// # Errors
    /// Rejects invalid identity, context, options or request size.
    pub fn validate(
        &self,
        schema: &AddonCapability,
        item_is_finite: bool,
    ) -> Result<(), ContractError> {
        validate_tool_version(&self.tool_version)?;
        validate_context(
            &self.source_url,
            &self.provider_id,
            &self.context_fingerprint,
        )?;
        schema.validate_option_values(
            &self.options,
            AddonMediaListOptionOperation::TestAccess,
            item_is_finite,
        )?;
        validate_import_request_size(self)
    }
}

/// Safe terminal outcomes with no freeform provider diagnostics.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum MediaListAccessCheckOutcome {
    Accessible,
    ProfileUnavailable,
    ProfileLocked,
    DecryptionFailed,
    ProviderRestricted,
    Failed,
}

/// Access evidence bound to the exact requested context.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct MediaListAccessCheckResult {
    pub provider_id: String,
    pub context_fingerprint: String,
    pub tool_version: String,
    pub outcome: MediaListAccessCheckOutcome,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub retry_after_seconds: Option<u32>,
}
impl MediaListAccessCheckResult {
    /// Validate identity echoes and bounded failure retry delay.
    /// # Errors
    /// Rejects mismatched identity, invalid delays or oversized results.
    pub fn validate(&self, request: &MediaListAccessCheckRequest) -> Result<(), ContractError> {
        validate_tool_version_echo(&self.tool_version, &request.tool_version)?;
        validate_echo(
            &self.provider_id,
            &self.context_fingerprint,
            &request.provider_id,
            &request.context_fingerprint,
        )?;
        if self.retry_after_seconds.is_some_and(|seconds| {
            seconds == 0 || seconds > MAX_MEDIA_LIST_OPERATION_TIMEOUT_SECONDS
        }) || (self.outcome == MediaListAccessCheckOutcome::Accessible
            && self.retry_after_seconds.is_some())
        {
            return invalid(
                "media list access retry_after_seconds",
                "requires a failure and a delay between 1 and 3600 seconds",
            );
        }
        validate_result_size(self)
    }
}

/// Discover source exclusions without downloading or staging the media file.
/// The fingerprint includes the host's requested item/chapter timing; the
/// provider returns full-source intervals and the host clips the saved window.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct MediaListIntervalMetadataRequest {
    pub source_url: String,
    pub provider_id: String,
    pub context_fingerprint: String,
    pub tool_version: String,
    pub options: AddonMediaListOptionValues,
}

impl MediaListIntervalMetadataRequest {
    /// Validate finite applicability, identity and effective processing options.
    /// # Errors
    /// Rejects invalid context, nonfinite media and unsupported options.
    pub fn validate(
        &self,
        schema: &AddonCapability,
        item_is_finite: bool,
    ) -> Result<(), ContractError> {
        if !item_is_finite {
            return invalid("media list interval metadata", "requires finite media");
        }
        validate_tool_version(&self.tool_version)?;
        validate_context(
            &self.source_url,
            &self.provider_id,
            &self.context_fingerprint,
        )?;
        schema.validate_option_values(
            &self.options,
            AddonMediaListOptionOperation::Preparation,
            true,
        )?;
        validate_import_request_size(self)
    }
}

/// A successful empty exclusion list is distinct from unsupported or failed discovery.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
pub enum MediaListIntervalMetadataOutcome {
    Ready {
        source_duration_milliseconds: u64,
        /// Half-open source ranges. Required even when no intervals were found.
        excluded_intervals_milliseconds: Vec<std::ops::Range<u64>>,
    },
    Unsupported,
    Failed {
        #[serde(default, skip_serializing_if = "Option::is_none")]
        retry_after_seconds: Option<u32>,
    },
}

/// Bounded metadata bound to the exact requested source/options/tool context.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct MediaListIntervalMetadataResult {
    pub provider_id: String,
    pub context_fingerprint: String,
    pub tool_version: String,
    pub outcome: MediaListIntervalMetadataOutcome,
}

impl MediaListIntervalMetadataResult {
    /// Validate identity echoes and source bounds before host timeline construction.
    /// # Errors
    /// Rejects excessive, empty, reversed or out-of-source intervals and invalid retries.
    pub fn validate(
        &self,
        request: &MediaListIntervalMetadataRequest,
    ) -> Result<(), ContractError> {
        validate_tool_version_echo(&self.tool_version, &request.tool_version)?;
        validate_echo(
            &self.provider_id,
            &self.context_fingerprint,
            &request.provider_id,
            &request.context_fingerprint,
        )?;
        match &self.outcome {
            MediaListIntervalMetadataOutcome::Ready {
                source_duration_milliseconds,
                excluded_intervals_milliseconds,
            } => {
                if *source_duration_milliseconds == 0
                    || *source_duration_milliseconds > i64::MAX as u64
                    || excluded_intervals_milliseconds.len() > MAX_MEDIA_LIST_INTERVALS
                    || excluded_intervals_milliseconds.iter().any(|range| {
                        range.start >= range.end || range.end > *source_duration_milliseconds
                    })
                {
                    return invalid(
                        "media list interval metadata",
                        "invalid bounded source intervals",
                    );
                }
            }
            MediaListIntervalMetadataOutcome::Failed {
                retry_after_seconds,
            } => {
                if retry_after_seconds.is_some_and(|seconds| {
                    seconds == 0 || seconds > MAX_MEDIA_LIST_OPERATION_TIMEOUT_SECONDS
                }) {
                    return invalid(
                        "media list interval metadata retry",
                        "requires a delay between 1 and 3600 seconds",
                    );
                }
            }
            MediaListIntervalMetadataOutcome::Unsupported => {}
        }
        validate_result_size(self)
    }
}

/// Host-owned staging request for finite processed media.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct MediaListPrepareRequest {
    /// Explicit primary/fallback languages; empty preserves the legacy request.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub requested_subtitle_languages: Vec<String>,
    pub source_url: String,
    pub provider_id: String,
    pub context_fingerprint: String,
    pub tool_version: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub options: Option<AddonMediaListOptionValues>,
    pub staging_directory: String,
    pub maximum_file_bytes: u64,
    pub maximum_staging_bytes: u64,
    pub timeout_seconds: u32,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub max_video_height: Option<u32>,
}
impl MediaListPrepareRequest {
    /// Validate finite applicability, option targets and host budgets.
    /// # Errors
    /// Rejects live items, invalid context, options, paths or budgets.
    pub fn validate(
        &self,
        schema: &AddonCapability,
        item_is_finite: bool,
    ) -> Result<(), ContractError> {
        self.validate_generic(Some(schema), item_is_finite)
    }

    /// Validate generic v2 delivery without inventing a provider options schema.
    ///
    /// # Errors
    /// Rejects mismatched schema/envelope presence, invalid options or budgets.
    pub fn validate_generic(
        &self,
        schema: Option<&AddonCapability>,
        item_is_finite: bool,
    ) -> Result<(), ContractError> {
        crate::media_list_subtitles::validate_languages(&self.requested_subtitle_languages, true)?;
        validate_tool_version(&self.tool_version)?;
        if !item_is_finite {
            return invalid("media list prepare", "requires finite media");
        }
        validate_context(
            &self.source_url,
            &self.provider_id,
            &self.context_fingerprint,
        )?;
        match (schema, self.options.as_ref()) {
            (Some(schema), Some(options)) => schema.validate_option_values(
                options,
                AddonMediaListOptionOperation::Preparation,
                true,
            )?,
            (None, None) => {}
            _ => {
                return invalid(
                    "media list prepare options",
                    "schema and options must agree",
                )
            }
        }
        if self.staging_directory.trim().is_empty()
            || self.staging_directory.len() > MAX_MEDIA_LIST_IMPORT_ARCHIVE_PATH_BYTES
            || self.staging_directory.chars().any(char::is_control)
        {
            return invalid(
                "media list prepare staging_directory",
                "requires a bounded host-owned staging path",
            );
        }
        if self.maximum_file_bytes == 0
            || self.maximum_staging_bytes < self.maximum_file_bytes
            || self.maximum_staging_bytes > MAX_MEDIA_LIST_PREPARATION_BYTES
            || self.timeout_seconds == 0
            || self.timeout_seconds > MAX_MEDIA_LIST_OPERATION_TIMEOUT_SECONDS
            || self.max_video_height.is_some_and(|height| {
                height == 0 || height > MAX_MEDIA_LIST_PREPARATION_VIDEO_HEIGHT
            })
        {
            return invalid(
                "media list prepare budgets",
                "requires positive bounded file, staging, timeout and height limits",
            );
        }
        validate_import_request_size(self)
    }
}

/// Prepared descriptor. Hosts must also verify containment, file type, actual
/// size, digest and probed duration before admitting the artifact.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct MediaListPrepareResult {
    /// Manual sidecars processed together with this media on its final clock.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub subtitles: Vec<crate::MediaListSubtitleArtifact>,
    pub provider_id: String,
    pub context_fingerprint: String,
    pub tool_version: String,
    /// Actual selected format identifier retained with the admitted artifact.
    pub selected_format: String,
    pub relative_path: String,
    pub byte_length: u64,
    pub sha256: String,
    pub duration_milliseconds: u64,
}
impl MediaListPrepareResult {
    /// Validate the descriptor against the exact preparation request.
    /// # Errors
    /// Rejects mismatched identity, unsafe paths, digest or media bounds.
    pub fn validate(&self, request: &MediaListPrepareRequest) -> Result<(), ContractError> {
        crate::media_list_subtitles::validate_artifacts(
            &self.subtitles,
            &request.requested_subtitle_languages,
            crate::MediaListSubtitleTimeline::Prepared,
            request.maximum_file_bytes,
            request
                .maximum_staging_bytes
                .saturating_sub(self.byte_length),
        )?;
        validate_tool_version_echo(&self.tool_version, &request.tool_version)?;
        if self.selected_format.is_empty()
            || self.selected_format.len() > 128
            || !self
                .selected_format
                .bytes()
                .all(|byte| byte.is_ascii_alphanumeric() || b"._+-".contains(&byte))
        {
            return invalid(
                "media list prepare selected_format",
                "requires a bounded ASCII format identifier",
            );
        }
        validate_echo(
            &self.provider_id,
            &self.context_fingerprint,
            &request.provider_id,
            &request.context_fingerprint,
        )?;
        if !is_safe_relative_path(&self.relative_path)
            || self.relative_path.chars().any(char::is_control)
            || self
                .relative_path
                .contains(['\\', '<', '>', '"', '|', '?', '*'])
            || self
                .relative_path
                .split('/')
                .any(|part| part.ends_with(['.', ' ']))
        {
            return invalid(
                "media list prepare relative_path",
                "requires a safe relative file path",
            );
        }
        validate_digest(&self.sha256)?;
        if self.byte_length == 0
            || self.byte_length > request.maximum_file_bytes
            || self.byte_length > request.maximum_staging_bytes
            || self.byte_length > MAX_MEDIA_LIST_PREPARATION_BYTES
            || self.duration_milliseconds == 0
            || self.duration_milliseconds > i64::MAX as u64
        {
            return invalid(
                "media list prepare result",
                "invalid file size or finite duration",
            );
        }
        validate_result_size(self)
    }
}

/// Validate the frozen effective JSON envelope before supplying playback v3.
/// # Errors
/// Rejects unknown schemas, unsupported options or item applicability.
pub fn validate_media_list_playback_options(
    options: &AddonMediaListOptionValues,
    schema: &AddonCapability,
    item_is_finite: bool,
) -> Result<(), ContractError> {
    schema.validate_option_values(
        options,
        AddonMediaListOptionOperation::Playback,
        item_is_finite,
    )
}

pub(crate) fn validate_context(
    source_url: &str,
    provider_id: &str,
    fingerprint: &str,
) -> Result<(), ContractError> {
    if source_url.trim().is_empty()
        || source_url.len() > 8192
        || source_url.chars().any(char::is_control)
    {
        return invalid(
            "media list operation source_url",
            "requires a bounded source URL",
        );
    }
    validate_provider_id(provider_id)?;
    if provider_id.chars().any(char::is_control) {
        return invalid("provider_id", "must not contain control characters");
    }
    validate_digest(fingerprint)
}
pub(crate) fn validate_tool_version_echo(value: &str, expected: &str) -> Result<(), ContractError> {
    validate_tool_version(value)?;
    if value != expected {
        return invalid(
            "media list operation tool_version",
            "must match the requested tool version",
        );
    }
    Ok(())
}
pub(crate) fn validate_digest(value: &str) -> Result<(), ContractError> {
    if value.len() != 64
        || !value
            .bytes()
            .all(|byte| byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte))
    {
        return invalid(
            "media list operation digest",
            "requires 64 lowercase hexadecimal characters",
        );
    }
    Ok(())
}
pub(crate) fn validate_echo(
    provider_id: &str,
    fingerprint: &str,
    expected_id: &str,
    expected_fingerprint: &str,
) -> Result<(), ContractError> {
    validate_provider_id(provider_id)?;
    validate_digest(fingerprint)?;
    if provider_id != expected_id || fingerprint != expected_fingerprint {
        return invalid(
            "media list operation context",
            "must match the requested identity and context fingerprint",
        );
    }
    Ok(())
}
pub(crate) fn validate_result_size(value: &impl Serialize) -> Result<(), ContractError> {
    let bytes = serde_json::to_vec(value).map_err(|_| ContractError::Invalid {
        field: "media list operation result",
        message: "could not serialize result".to_owned(),
    })?;
    if bytes.len() > MAX_MEDIA_LIST_OPERATION_OUTPUT_BYTES {
        return invalid("media list operation result", "exceeds the 64 KiB limit");
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{
        AddonMediaListOptionApplicability, AddonMediaListOptionDefinition,
        AddonMediaListOptionKind, AddonMediaListOptionScope, AddonMediaListOptionValue,
        CAPABILITY_MEDIA_LIST_OPTIONS_V1,
    };

    fn schema(operation: AddonMediaListOptionOperation) -> AddonCapability {
        AddonCapability {
            id: CAPABILITY_MEDIA_LIST_OPTIONS_V1.to_owned(),
            options: vec![AddonMediaListOptionDefinition {
                key: "process".to_owned(),
                label: [("en-US".to_owned(), "Process".to_owned())].into(),
                description: [("en-US".to_owned(), "Process finite media".to_owned())].into(),
                scope: AddonMediaListOptionScope::Both,
                operation_targets: vec![operation],
                applicability: AddonMediaListOptionApplicability::FiniteOnly,
                kind: AddonMediaListOptionKind::Boolean,
                default: AddonMediaListOptionValue::Boolean(false),
                choices: vec![],
                requires_preparation: false,
            }],
        }
    }

    fn access_request() -> MediaListAccessCheckRequest {
        MediaListAccessCheckRequest {
            source_url: "https://media.example.test/list".to_owned(),
            provider_id: "item-1".to_owned(),
            context_fingerprint: "a".repeat(64),
            tool_version: "2026.08.19".to_owned(),
            options: AddonMediaListOptionValues {
                schema: CAPABILITY_MEDIA_LIST_OPTIONS_V1.to_owned(),
                values: [(
                    "process".to_owned(),
                    AddonMediaListOptionValue::Boolean(true),
                )]
                .into(),
            },
        }
    }

    fn prepare_request() -> MediaListPrepareRequest {
        let access = access_request();
        MediaListPrepareRequest {
            requested_subtitle_languages: Vec::new(),
            tool_version: access.tool_version,
            source_url: access.source_url,
            provider_id: access.provider_id,
            context_fingerprint: access.context_fingerprint,
            options: Some(access.options),
            staging_directory: "staging/attempt-1".to_owned(),
            maximum_file_bytes: 2048,
            maximum_staging_bytes: 4096,
            timeout_seconds: 300,
            max_video_height: Some(1080),
        }
    }

    fn interval_request() -> MediaListIntervalMetadataRequest {
        let access = access_request();
        MediaListIntervalMetadataRequest {
            source_url: access.source_url,
            provider_id: access.provider_id,
            context_fingerprint: access.context_fingerprint,
            tool_version: access.tool_version,
            options: access.options,
        }
    }

    #[test]
    fn interval_metadata_requires_finite_context_and_explicit_success() {
        let request = interval_request();
        let schema = schema(AddonMediaListOptionOperation::Preparation);
        request.validate(&schema, true).unwrap();
        assert!(request.validate(&schema, false).is_err());
        let mut result = MediaListIntervalMetadataResult {
            provider_id: request.provider_id.clone(),
            context_fingerprint: request.context_fingerprint.clone(),
            tool_version: request.tool_version.clone(),
            outcome: MediaListIntervalMetadataOutcome::Ready {
                source_duration_milliseconds: 20_000,
                excluded_intervals_milliseconds: vec![],
            },
        };
        result.validate(&request).unwrap();
        let mut json = serde_json::to_value(&result).unwrap();
        json["outcome"]
            .as_object_mut()
            .unwrap()
            .remove("excluded_intervals_milliseconds");
        assert!(serde_json::from_value::<MediaListIntervalMetadataResult>(json).is_err());
        result.context_fingerprint = "b".repeat(64);
        assert!(result.validate(&request).is_err());
        result.context_fingerprint = request.context_fingerprint.clone();
        result.outcome = MediaListIntervalMetadataOutcome::Unsupported;
        result.validate(&request).unwrap();
        assert_ne!(
            serde_json::to_value(&result).unwrap()["outcome"]["kind"],
            "ready"
        );
    }

    #[test]
    #[allow(clippy::single_range_in_vec_init)] // Each element is one source interval, not its samples.
    fn interval_metadata_rejects_invalid_ranges_count_and_retry() {
        let request = interval_request();
        let result = |outcome| MediaListIntervalMetadataResult {
            provider_id: request.provider_id.clone(),
            context_fingerprint: request.context_fingerprint.clone(),
            tool_version: request.tool_version.clone(),
            outcome,
        };
        for ranges in [
            vec![1..1],
            vec![21..22],
            vec![0..1; MAX_MEDIA_LIST_INTERVALS + 1],
        ] {
            assert!(result(MediaListIntervalMetadataOutcome::Ready {
                source_duration_milliseconds: 20,
                excluded_intervals_milliseconds: ranges,
            })
            .validate(&request)
            .is_err());
        }
        for retry_after_seconds in [Some(0), Some(3601)] {
            assert!(result(MediaListIntervalMetadataOutcome::Failed {
                retry_after_seconds
            })
            .validate(&request)
            .is_err());
        }
        result(MediaListIntervalMetadataOutcome::Failed {
            retry_after_seconds: Some(3600),
        })
        .validate(&request)
        .unwrap();
    }

    #[test]
    fn access_rejects_unknown_options_context_and_oversized_identity() {
        let schema = schema(AddonMediaListOptionOperation::TestAccess);
        let valid = access_request();
        valid.validate(&schema, true).unwrap();
        assert!(valid.validate(&schema, false).is_err());
        for fingerprint in ["a".repeat(63), "A".repeat(64), "z".repeat(64)] {
            let mut request = valid.clone();
            request.context_fingerprint = fingerprint;
            assert!(request.validate(&schema, true).is_err());
        }
        let mut request = valid.clone();
        request.provider_id = "x".repeat(385);
        assert!(request.validate(&schema, true).is_err());
        request = valid.clone();
        request.source_url = "x".repeat(8193);
        assert!(request.validate(&schema, true).is_err());
        request = valid.clone();
        request.options.schema = "media-list.options.v2".to_owned();
        assert!(request.validate(&schema, true).is_err());
        request = valid;
        request.options.values.insert(
            "unknown".to_owned(),
            AddonMediaListOptionValue::Boolean(true),
        );
        assert!(request.validate(&schema, true).is_err());
        assert!(serde_json::from_str::<AddonMediaListOptionScope>("\"unknown\"").is_err());
    }

    #[test]
    fn access_outcomes_are_redacted_and_bound_to_context() {
        let request = access_request();
        let mut result = MediaListAccessCheckResult {
            tool_version: request.tool_version.clone(),
            provider_id: request.provider_id.clone(),
            context_fingerprint: request.context_fingerprint.clone(),
            outcome: MediaListAccessCheckOutcome::Failed,
            retry_after_seconds: Some(3600),
        };
        for outcome in [
            MediaListAccessCheckOutcome::ProfileUnavailable,
            MediaListAccessCheckOutcome::ProfileLocked,
            MediaListAccessCheckOutcome::DecryptionFailed,
            MediaListAccessCheckOutcome::ProviderRestricted,
            MediaListAccessCheckOutcome::Failed,
        ] {
            result.outcome = outcome;
            result.validate(&request).unwrap();
        }
        for delay in [0, 3601] {
            result.retry_after_seconds = Some(delay);
            assert!(result.validate(&request).is_err());
        }
        result.retry_after_seconds = None;
        result.outcome = MediaListAccessCheckOutcome::Accessible;
        result.validate(&request).unwrap();
        let mut json = serde_json::to_value(&result).unwrap();
        json["diagnostic"] = "untrusted".into();
        assert!(serde_json::from_value::<MediaListAccessCheckResult>(json).is_err());
        result.provider_id = "item-2".to_owned();
        assert!(result.validate(&request).is_err());
        result.provider_id = request.provider_id.clone();
        result.context_fingerprint = "b".repeat(64);
        assert!(result.validate(&request).is_err());
    }

    #[test]
    fn generic_preparation_accepts_no_schema_without_weakening_v1() {
        let schema = schema(AddonMediaListOptionOperation::Preparation);
        let mut request = prepare_request();
        let legacy = serde_json::to_value(&request).unwrap();
        assert!(legacy["options"].is_object());
        assert!(legacy.get("requested_subtitle_languages").is_none());
        request.validate(&schema, true).unwrap();
        request.validate_generic(Some(&schema), true).unwrap();
        assert!(request.validate_generic(None, true).is_err());
        request.options = None;
        request.validate_generic(None, true).unwrap();
        assert!(request.validate(&schema, true).is_err());
        assert!(request.validate_generic(None, false).is_err());
        assert!(serde_json::to_value(&request)
            .unwrap()
            .get("options")
            .is_none());
    }

    #[test]
    fn preparation_rejects_live_wrong_targets_and_bad_budgets() {
        let schema = schema(AddonMediaListOptionOperation::Preparation);
        let valid = prepare_request();
        valid.validate(&schema, true).unwrap();
        assert!(valid.validate(&schema, false).is_err());
        assert!(valid
            .validate(
                &super::tests::schema(AddonMediaListOptionOperation::Enrich),
                true
            )
            .is_err());
        for timeout in [0, 3601] {
            let mut request = valid.clone();
            request.timeout_seconds = timeout;
            assert!(request.validate(&schema, true).is_err());
        }
        for height in [0, 4321] {
            let mut request = valid.clone();
            request.max_video_height = Some(height);
            assert!(request.validate(&schema, true).is_err());
        }
        let mut request = valid.clone();
        request.maximum_file_bytes = 0;
        assert!(request.validate(&schema, true).is_err());
        request = valid.clone();
        request.maximum_staging_bytes = 2047;
        assert!(request.validate(&schema, true).is_err());
        request = valid.clone();
        request.maximum_staging_bytes = u64::MAX;
        assert!(request.validate(&schema, true).is_err());
        request = valid;
        request.staging_directory = "x".repeat(4097);
        assert!(request.validate(&schema, true).is_err());
    }

    #[test]
    fn prepared_descriptor_rejects_traversal_absolute_paths_and_bad_media() {
        let request = prepare_request();
        let valid = MediaListPrepareResult {
            subtitles: Vec::new(),
            tool_version: request.tool_version.clone(),
            selected_format: "137+140".to_owned(),
            provider_id: request.provider_id.clone(),
            context_fingerprint: request.context_fingerprint.clone(),
            relative_path: "media/prepared.mkv".to_owned(),
            byte_length: 1024,
            sha256: "f".repeat(64),
            duration_milliseconds: 120_000,
        };
        valid.validate(&request).unwrap();
        assert!(serde_json::to_value(&valid)
            .unwrap()
            .get("subtitles")
            .is_none());
        let mut subtitle_request = request.clone();
        subtitle_request.requested_subtitle_languages = vec!["en".into()];
        let mut with_subtitle = valid.clone();
        with_subtitle
            .subtitles
            .push(crate::MediaListSubtitleArtifact {
                language: "en-US".into(),
                format: crate::MediaListSubtitleFormat::Srt,
                provenance: crate::MediaListSubtitleProvenance::Manual,
                timeline: crate::MediaListSubtitleTimeline::Prepared,
                relative_path: "prepared.en-US.srt".into(),
                byte_length: 64,
                sha256: "d".repeat(64),
            });
        with_subtitle.validate(&subtitle_request).unwrap();
        assert!(with_subtitle.validate(&request).is_err());
        with_subtitle.subtitles[0].timeline = crate::MediaListSubtitleTimeline::Source;
        assert!(with_subtitle.validate(&subtitle_request).is_err());
        for path in [
            "../escape.mkv",
            "/absolute.mkv",
            "C:/absolute.mkv",
            "media/../escape",
            "media\\escape",
            "media//file",
            "media/file.",
            "media/file ",
            "media/file\0",
            "media/file:stream",
        ] {
            let mut result = valid.clone();
            result.relative_path = path.to_owned();
            assert!(result.validate(&request).is_err(), "accepted {path:?}");
        }
        let mut result = valid.clone();
        result.byte_length = 2049;
        assert!(result.validate(&request).is_err());
        result = valid.clone();
        result.duration_milliseconds = 0;
        assert!(result.validate(&request).is_err());
        result = valid.clone();
        result.sha256 = "F".repeat(64);
        assert!(result.validate(&request).is_err());
        result = valid;
        result.context_fingerprint = "b".repeat(64);
        assert!(result.validate(&request).is_err());
    }

    #[test]
    fn playback_requires_playback_target_and_keeps_envelope_shape() {
        let options = access_request().options;
        validate_media_list_playback_options(
            &options,
            &schema(AddonMediaListOptionOperation::Playback),
            true,
        )
        .unwrap();
        assert!(validate_media_list_playback_options(
            &options,
            &schema(AddonMediaListOptionOperation::Enrich),
            true
        )
        .is_err());
        assert_eq!(
            serde_json::to_string(&options).unwrap(),
            r#"{"schema":"media-list.options.v1","values":{"process":true}}"#
        );
    }

    #[test]
    fn enrichment_wraps_legacy_bytes_and_preserves_identity_validation() {
        let legacy = r#"{"source_url":"https://media.example.test/list","record_capability":"media-list.list.v5","provider_id":"item-1","overview_fingerprint":"overview-1","item":{"record_type":"item","provider_id":"item-1","rank":0,"display_title":"Item","title":"Item","kind":"movie"}}"#;
        let request: MediaListEnrichRequest = serde_json::from_str(legacy).unwrap();
        let before = serde_json::to_vec(&request).unwrap();
        let mut wrapped = MediaListEnrichOptionsRequest {
            request,
            options: access_request().options,
        };
        wrapped
            .validate(&schema(AddonMediaListOptionOperation::Enrich), true)
            .unwrap();
        assert_eq!(serde_json::to_vec(&wrapped.request).unwrap(), before);
        assert!(wrapped
            .validate(&schema(AddonMediaListOptionOperation::Playback), true)
            .is_err());
        wrapped.request.provider_id = "item-2".to_owned();
        assert!(wrapped
            .validate(&schema(AddonMediaListOptionOperation::Enrich), true)
            .is_err());
    }

    #[test]
    fn runtime_probe_rejects_diagnostics_paths_and_extra_fields() {
        assert_eq!(
            serde_json::to_string(&MediaListItemRuntimeInfoRequest {}).unwrap(),
            "{}"
        );
        assert!(serde_json::from_str::<MediaListItemRuntimeInfoRequest>(
            r#"{"source_url":"https://example.test"}"#
        )
        .is_err());
        assert!(serde_json::from_str::<MediaListItemRuntimeInfo>(
            r#"{"tool_version":"1","message":"extra"}"#
        )
        .is_err());
        MediaListItemRuntimeInfo {
            tool_version: "2026.08.19-rc_1".to_owned(),
        }
        .validate()
        .unwrap();
        for value in [
            "",
            "version 1",
            "path/version",
            "version\n1",
            "vérsion",
            "1+2",
        ] {
            assert!(validate_tool_version(value).is_err());
        }
        assert!(validate_tool_version(&"x".repeat(129)).is_err());
    }

    #[test]
    fn access_and_preparation_reject_tool_drift_and_unsafe_format_metadata() {
        let access = access_request();
        let mut result = MediaListAccessCheckResult {
            provider_id: access.provider_id.clone(),
            context_fingerprint: access.context_fingerprint.clone(),
            tool_version: "changed-version".to_owned(),
            outcome: MediaListAccessCheckOutcome::Accessible,
            retry_after_seconds: None,
        };
        assert!(result.validate(&access).is_err());
        result.tool_version = access.tool_version.clone();
        result.validate(&access).unwrap();
        let request = prepare_request();
        let mut result = MediaListPrepareResult {
            subtitles: Vec::new(),
            provider_id: request.provider_id.clone(),
            context_fingerprint: request.context_fingerprint.clone(),
            tool_version: "changed-version".to_owned(),
            selected_format: "137+140".to_owned(),
            relative_path: "prepared.mkv".to_owned(),
            byte_length: 1024,
            sha256: "a".repeat(64),
            duration_milliseconds: 1000,
        };
        assert!(result.validate(&request).is_err());
        result.tool_version = request.tool_version.clone();
        result.validate(&request).unwrap();
        for format in [
            "",
            "format 1",
            "https://example.test",
            "137/140",
            "137\n140",
        ] {
            result.selected_format = format.to_owned();
            assert!(result.validate(&request).is_err());
        }
        result.selected_format = "x".repeat(129);
        assert!(result.validate(&request).is_err());
        let mut access = access;
        access.tool_version = "bad version".to_owned();
        assert!(access
            .validate(&schema(AddonMediaListOptionOperation::TestAccess), true)
            .is_err());
        let mut request = request;
        request.tool_version = "bad version".to_owned();
        assert!(request
            .validate(&schema(AddonMediaListOptionOperation::Preparation), true)
            .is_err());
    }
}
