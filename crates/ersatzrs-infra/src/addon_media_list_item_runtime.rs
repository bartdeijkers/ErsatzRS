//! Bounded installed-package operations for saved-draft item work.
//!
//! Callers own durable admission, heartbeat and artifact validation. This adapter
//! never exposes provider diagnostics or treats a returned file descriptor as an
//! admitted artifact.

use std::{fmt::Write as _, time::Duration};

use ersatzrs_addon_contract::{
    validate_operator_source_url, AddonCapability, MediaListAccessCheckRequest,
    MediaListAccessCheckResult, MediaListIntervalMetadataRequest, MediaListIntervalMetadataResult,
    MediaListItemRuntimeInfo, MediaListItemRuntimeInfoRequest, MediaListPrepareRequest,
    MediaListPrepareResult, MediaListSubtitlesRequest, MediaListSubtitlesResult,
    RemoteStreamRetainedPlayRequest, CAPABILITY_MEDIA_LIST_ACCESS_CHECK_V1,
    CAPABILITY_MEDIA_LIST_INTERVAL_METADATA_V1, CAPABILITY_MEDIA_LIST_ITEM_RUNTIME_V1,
    CAPABILITY_MEDIA_LIST_PREPARE_V1, CAPABILITY_MEDIA_LIST_PREPARE_V2,
    CAPABILITY_MEDIA_LIST_SUBTITLES_V1, CAPABILITY_REMOTE_STREAM_PLAY_V4,
    MAX_MEDIA_LIST_OPERATION_OUTPUT_BYTES,
};
use serde::{de::DeserializeOwned, Serialize};
use sha2::{Digest as _, Sha256};

use crate::{
    addons::{
        load_installed_addon, resolve_enabled_addon_command, run_bounded_command_with_input,
        run_bounded_command_with_input_cancellable, AddonError, AddonRuntimeContext,
        InstalledAddon,
    },
    db::DbPool,
};

/// Stable host outcomes; neither provider stderr nor request data becomes an error.
#[derive(Debug, Clone, Copy, PartialEq, Eq, thiserror::Error)]
pub enum ItemRuntimeError {
    #[error("the item add-on is unavailable")]
    Unavailable,
    #[error("the item add-on changed")]
    PackageChanged,
    #[error("the item operation is unsupported")]
    Unsupported,
    #[error("the item operation request is invalid")]
    InvalidRequest,
    #[error("the item operation failed")]
    Failed,
    #[error("the item operation returned an invalid result")]
    InvalidResult,
    #[error("item preparation process termination could not be confirmed")]
    TerminationUnconfirmed,
}

/// One captured installation, re-fenced when assembling each command.
pub struct InstalledItemRuntime {
    pool: DbPool,
    installation: InstalledAddon,
    package_identity: String,
}

impl InstalledItemRuntime {
    /// Load the enabled package and optionally require a durable job's identity.
    ///
    /// # Errors
    /// Returns a static error when the package is missing, disabled or changed.
    pub async fn load(
        pool: &DbPool,
        addon_id: &str,
        expected_package_identity: Option<&str>,
    ) -> Result<Self, ItemRuntimeError> {
        let installation = load_installed_addon(pool, addon_id)
            .await
            .map_err(|_| ItemRuntimeError::Unavailable)?
            .filter(|installation| installation.enabled)
            .ok_or(ItemRuntimeError::Unavailable)?;
        let package_identity = package_identity(&installation);
        if expected_package_identity.is_some_and(|expected| expected != package_identity) {
            return Err(ItemRuntimeError::PackageChanged);
        }
        Ok(Self {
            pool: pool.clone(),
            installation,
            package_identity,
        })
    }

    #[must_use]
    pub fn package_identity(&self) -> &str {
        &self.package_identity
    }

    /// Require the installed declaration to match the saved schema exactly.
    ///
    /// # Errors
    /// Returns `PackageChanged` if the installed declaration is missing, invalid,
    /// or differs from the retained schema.
    pub fn validate_options(
        &self,
        options: &crate::addon_media_list_options::ResolvedAddonItemOptions,
    ) -> Result<(), ItemRuntimeError> {
        options
            .validate_installed_schema(&self.installation.manifest)
            .map_err(|_| ItemRuntimeError::PackageChanged)
    }

    /// Read the tool version without passing a source, cookies or profile paths.
    ///
    /// # Errors
    /// Rejects unavailable operations, failed processes and invalid version output.
    pub async fn runtime_info(&self) -> Result<MediaListItemRuntimeInfo, ItemRuntimeError> {
        self.runtime_info_inner(None).await
    }

    /// Cancel a runtime identity probe and await managed process cleanup.
    ///
    /// # Errors
    /// Rejects unavailable operations, cancelled probes and invalid version output.
    pub async fn runtime_info_cancellable(
        &self,
        cancellation: tokio::sync::oneshot::Receiver<()>,
    ) -> Result<MediaListItemRuntimeInfo, ItemRuntimeError> {
        self.runtime_info_inner(Some(cancellation)).await
    }

    async fn runtime_info_inner(
        &self,
        cancellation: Option<tokio::sync::oneshot::Receiver<()>>,
    ) -> Result<MediaListItemRuntimeInfo, ItemRuntimeError> {
        let result: MediaListItemRuntimeInfo = self
            .invoke(
                CAPABILITY_MEDIA_LIST_ITEM_RUNTIME_V1,
                "runtime-info",
                &MediaListItemRuntimeInfoRequest::default(),
                Duration::from_secs(15),
                &[],
                cancellation,
            )
            .await?;
        result
            .validate()
            .map_err(|_| ItemRuntimeError::InvalidResult)?;
        Ok(result)
    }

    /// Execute an access check using only a previously saved effective envelope.
    ///
    /// # Errors
    /// Rejects invalid requests, denied URLs and mismatched result identities.
    pub async fn test_access(
        &self,
        request: &MediaListAccessCheckRequest,
        item_is_finite: bool,
    ) -> Result<MediaListAccessCheckResult, ItemRuntimeError> {
        request
            .validate(self.schema()?, item_is_finite)
            .map_err(|_| ItemRuntimeError::InvalidRequest)?;
        self.validate_source(&request.source_url)?;
        let result: MediaListAccessCheckResult = self
            .invoke(
                CAPABILITY_MEDIA_LIST_ACCESS_CHECK_V1,
                "test-access",
                request,
                Duration::from_mins(1),
                &[],
                None,
            )
            .await?;
        result
            .validate(request)
            .map_err(|_| ItemRuntimeError::InvalidResult)?;
        Ok(result)
    }

    /// Discover finite source intervals without artifact storage or a download worker.
    /// Signal cancellation and await this future to confirm managed process cleanup.
    ///
    /// # Errors
    /// Rejects invalid requests, unsupported operations and mismatched or malformed results.
    pub async fn interval_metadata_cancellable(
        &self,
        request: &MediaListIntervalMetadataRequest,
        cancellation: tokio::sync::oneshot::Receiver<()>,
    ) -> Result<MediaListIntervalMetadataResult, ItemRuntimeError> {
        request
            .validate(self.schema()?, true)
            .map_err(|_| ItemRuntimeError::InvalidRequest)?;
        self.validate_source(&request.source_url)?;
        let result: MediaListIntervalMetadataResult = self
            .invoke(
                CAPABILITY_MEDIA_LIST_INTERVAL_METADATA_V1,
                "interval-metadata",
                request,
                Duration::from_mins(1),
                &[],
                Some(cancellation),
            )
            .await?;
        result
            .validate(request)
            .map_err(|_| ItemRuntimeError::InvalidResult)?;
        Ok(result)
    }

    /// Acquire requested manual subtitle sidecars inside host-owned scratch.
    /// The caller must signal cancellation and await cleanup before releasing
    /// scratch, then independently validate physical files before admission.
    ///
    /// # Errors
    /// Rejects invalid requests, unsupported packages, failed processes and
    /// mismatched results. Unconfirmed cleanup retains its typed failure.
    pub async fn subtitles_cancellable(
        &self,
        request: &MediaListSubtitlesRequest,
        cancellation: tokio::sync::oneshot::Receiver<()>,
    ) -> Result<MediaListSubtitlesResult, ItemRuntimeError> {
        request
            .validate(self.installation.manifest.media_list_option_schema(), true)
            .map_err(|_| ItemRuntimeError::InvalidRequest)?;
        self.validate_source(&request.source_url)?;
        let result: MediaListSubtitlesResult = self
            .invoke(
                CAPABILITY_MEDIA_LIST_SUBTITLES_V1,
                "subtitles",
                request,
                Duration::from_secs(u64::from(request.timeout_seconds)),
                std::slice::from_ref(&request.staging_directory),
                Some(cancellation),
            )
            .await?;
        result
            .validate(request)
            .map_err(|_| ItemRuntimeError::InvalidResult)?;
        Ok(result)
    }

    /// Assemble a command without starting a provider or a second transfer.
    /// The caller owns the shared startup deadline and managed process lifetime.
    ///
    /// # Errors
    /// Rejects unsupported packages, invalid requests and changed installations.
    pub async fn retained_play_command(
        &self,
        request: &RemoteStreamRetainedPlayRequest,
    ) -> Result<crate::remote_stream::RemoteStreamCommand, ItemRuntimeError> {
        request
            .validate(self.schema()?)
            .map_err(|_| ItemRuntimeError::InvalidRequest)?;
        self.validate_source(&request.source_url)?;
        if !self
            .installation
            .manifest
            .has_capability(CAPABILITY_REMOTE_STREAM_PLAY_V4)
        {
            return Err(ItemRuntimeError::Unsupported);
        }
        let runtime = AddonRuntimeContext {
            ffmpeg_path: Some(ersatzrs_ffmpeg::ffmpeg_path()),
            ffprobe_path: Some(ersatzrs_ffmpeg::ffprobe_path()),
        };
        let mut command = resolve_enabled_addon_command(
            &self.pool,
            &self.installation.addon_id,
            CAPABILITY_REMOTE_STREAM_PLAY_V4,
            "play-intervals",
            &runtime,
        )
        .await
        .map_err(|_| ItemRuntimeError::Unavailable)?;
        if command.state_revision != self.installation.state_revision {
            return Err(ItemRuntimeError::PackageChanged);
        }
        command.environment.insert(
            "ERSATZRS_ADDON_CAPABILITY".to_owned(),
            CAPABILITY_REMOTE_STREAM_PLAY_V4.to_owned(),
        );
        Ok(crate::remote_stream::RemoteStreamCommand {
            program: command.program,
            args: command.args,
            working_directory: command.working_directory,
            environment: command.environment,
        })
    }

    /// Execute finite preparation inside capacity already reserved by the host.
    /// The result still requires physical containment, size, digest and probe checks.
    ///
    /// # Errors
    /// Rejects invalid requests, failed processes and mismatched descriptors.
    pub async fn prepare(
        &self,
        request: &MediaListPrepareRequest,
    ) -> Result<MediaListPrepareResult, ItemRuntimeError> {
        self.prepare_inner(request, None, CAPABILITY_MEDIA_LIST_PREPARE_V1)
            .await
    }

    /// Request cancellation through the channel and await this future before
    /// releasing staging. A dropped sender also cancels the managed process.
    ///
    /// # Errors
    /// Returns the same preparation errors, including `TerminationUnconfirmed`
    /// when staging must remain reserved for explicit process recovery.
    pub async fn prepare_cancellable(
        &self,
        request: &MediaListPrepareRequest,
        cancellation: tokio::sync::oneshot::Receiver<()>,
    ) -> Result<MediaListPrepareResult, ItemRuntimeError> {
        self.prepare_inner(
            request,
            Some(cancellation),
            CAPABILITY_MEDIA_LIST_PREPARE_V1,
        )
        .await
    }

    /// Execute explicitly advertised generic finite preparation, including schema-less providers.
    ///
    /// # Errors
    /// Rejects unsupported packages, invalid options or failed managed execution.
    pub async fn prepare_generic_cancellable(
        &self,
        request: &MediaListPrepareRequest,
        cancellation: tokio::sync::oneshot::Receiver<()>,
    ) -> Result<MediaListPrepareResult, ItemRuntimeError> {
        self.prepare_inner(
            request,
            Some(cancellation),
            CAPABILITY_MEDIA_LIST_PREPARE_V2,
        )
        .await
    }

    async fn prepare_inner(
        &self,
        request: &MediaListPrepareRequest,
        cancellation: Option<tokio::sync::oneshot::Receiver<()>>,
        capability: &str,
    ) -> Result<MediaListPrepareResult, ItemRuntimeError> {
        if capability == CAPABILITY_MEDIA_LIST_PREPARE_V2 {
            request.validate_generic(self.installation.manifest.media_list_option_schema(), true)
        } else {
            request.validate(self.schema()?, true)
        }
        .map_err(|_| ItemRuntimeError::InvalidRequest)?;
        self.validate_source(&request.source_url)?;
        let result: MediaListPrepareResult = self
            .invoke(
                capability,
                "prepare",
                request,
                Duration::from_secs(u64::from(request.timeout_seconds)),
                std::slice::from_ref(&request.staging_directory),
                cancellation,
            )
            .await?;
        result
            .validate(request)
            .map_err(|_| ItemRuntimeError::InvalidResult)?;
        Ok(result)
    }

    fn schema(&self) -> Result<&AddonCapability, ItemRuntimeError> {
        self.installation
            .manifest
            .media_list_option_schema()
            .ok_or(ItemRuntimeError::Unsupported)
    }

    fn validate_source(&self, source: &str) -> Result<(), ItemRuntimeError> {
        if self
            .installation
            .manifest
            .permissions
            .network_hosts
            .is_empty()
        {
            return Err(ItemRuntimeError::InvalidRequest);
        }
        validate_operator_source_url(
            source,
            &self.installation.manifest.permissions.network_hosts,
        )
        .map(|_| ())
        .map_err(|_| ItemRuntimeError::InvalidRequest)
    }

    async fn invoke<T: Serialize, R: DeserializeOwned>(
        &self,
        capability: &str,
        operation: &str,
        request: &T,
        timeout: Duration,
        private_values: &[String],
        mut cancellation: Option<tokio::sync::oneshot::Receiver<()>>,
    ) -> Result<R, ItemRuntimeError> {
        let started = std::time::Instant::now();
        if !self.installation.manifest.has_capability(capability) {
            return Err(ItemRuntimeError::Unsupported);
        }
        let runtime = AddonRuntimeContext {
            ffmpeg_path: Some(ersatzrs_ffmpeg::ffmpeg_path()),
            ffprobe_path: Some(ersatzrs_ffmpeg::ffprobe_path()),
        };
        let resolution = tokio::time::timeout_at(
            tokio::time::Instant::from_std(started) + timeout,
            resolve_enabled_addon_command(
                &self.pool,
                &self.installation.addon_id,
                capability,
                operation,
                &runtime,
            ),
        );
        // Preflight owns no child yet, so cancellation can safely drop it.
        // Once spawned, the bounded runner instead awaits process cleanup.
        let resolved = if let Some(cancel) = cancellation.as_mut() {
            tokio::select! {
                biased;
                _ = cancel => return Err(ItemRuntimeError::Failed),
                result = resolution => result,
            }
        } else {
            resolution.await
        };
        let mut command = resolved
            .map_err(|_| ItemRuntimeError::Failed)?
            .map_err(|_| ItemRuntimeError::Unavailable)?;
        if command.state_revision != self.installation.state_revision {
            return Err(ItemRuntimeError::PackageChanged);
        }
        command.environment.insert(
            "ERSATZRS_ADDON_CAPABILITY".to_owned(),
            capability.to_owned(),
        );
        let input = serde_json::to_vec(request).map_err(|_| ItemRuntimeError::InvalidRequest)?;
        // Package resolution is part of the caller's budget, not extra time
        // preceding an independently reset operation timeout.
        let timeout = timeout
            .checked_sub(started.elapsed())
            .filter(|remaining| !remaining.is_zero())
            .ok_or(ItemRuntimeError::Failed)?;
        let output = if let Some(cancellation) = cancellation {
            run_bounded_command_with_input_cancellable(
                &command,
                timeout,
                MAX_MEDIA_LIST_OPERATION_OUTPUT_BYTES,
                &input,
                private_values,
                cancellation,
            )
            .await
        } else {
            run_bounded_command_with_input(
                &command,
                timeout,
                MAX_MEDIA_LIST_OPERATION_OUTPUT_BYTES,
                &input,
                private_values,
            )
            .await
        }
        .map_err(|error| match error {
            AddonError::TerminationUnconfirmed => ItemRuntimeError::TerminationUnconfirmed,
            _ => ItemRuntimeError::Failed,
        })?;
        // Provider IDs may themselves be source URLs and must be echoed. The
        // closed result types and exact echo validation own that boundary;
        // substring filtering would reject legitimate URL-based identities.
        // Every process/parse failure remains a static diagnostic above/below.
        serde_json::from_slice(&output).map_err(|_| ItemRuntimeError::InvalidResult)
    }
}

fn package_identity(installation: &InstalledAddon) -> String {
    package_identity_for_parts(
        &installation.addon_id,
        &installation.version.to_string(),
        &installation.package_sha256,
        &installation.content_sha256,
        &installation.state_revision,
    )
}

/// Also used inside the durable completion transaction, without loading secrets.
pub(crate) fn package_identity_for_parts(
    addon_id: &str,
    version: &str,
    package_sha256: &str,
    content_sha256: &str,
    state_revision: &str,
) -> String {
    let identity = serde_json::to_vec(&(
        addon_id,
        version,
        package_sha256,
        content_sha256,
        state_revision,
    ))
    .expect("package identity serializes");
    Sha256::digest(identity)
        .iter()
        .fold(String::with_capacity(64), |mut hex, byte| {
            write!(hex, "{byte:02x}").expect("write to string");
            hex
        })
}

// Synthetic shell packages exercise the shared Unix runner. Native Windows
// process ownership is a separate acceptance gate, not inferred from these tests.
#[cfg(all(test, unix))]
mod tests {
    use super::*;
    use ersatzrs_addon_contract::CAPABILITY_MEDIA_LIST_OPTIONS_V1;

    fn retained_request() -> RemoteStreamRetainedPlayRequest {
        serde_json::from_value(serde_json::json!({
            "source_url":"https://example.test/item", "provider_id":"item",
            "context_fingerprint":"a".repeat(64), "source_duration_milliseconds":20000,
            "options":{"schema":CAPABILITY_MEDIA_LIST_OPTIONS_V1,"values":{"enabled":true}},
            "max_video_height":1080,
            "retained_intervals_milliseconds":[{"start":1200,"end":19000}]
        }))
        .unwrap()
    }

    async fn fixture(script: &str, capable: bool) -> (tempfile::TempDir, InstalledItemRuntime) {
        let temporary = tempfile::tempdir().unwrap();
        let pool = crate::addon_media_list_preparation::tests::fixture_with_script(
            &temporary.path().join("package"),
            script,
        )
        .await;
        let original: String =
            sqlx::query_scalar(r#"SELECT "ManifestJson" FROM "AddonInstallation" LIMIT 1"#)
                .fetch_one(&pool)
                .await
                .unwrap();
        let mut manifest: serde_json::Value = serde_json::from_str(&original).unwrap();
        for capability in manifest["capabilities"].as_array_mut().unwrap().iter_mut() {
            if capability["id"] == CAPABILITY_MEDIA_LIST_OPTIONS_V1 {
                capability["options"][0]["operation_targets"] =
                    serde_json::json!(["preparation", "playback"]);
            }
        }
        if capable {
            manifest["capabilities"]
                .as_array_mut()
                .unwrap()
                .push(serde_json::json!({"id": CAPABILITY_REMOTE_STREAM_PLAY_V4}));
        }
        sqlx::query(r#"UPDATE "AddonInstallation" SET "ManifestJson"=?"#)
            .bind(manifest.to_string())
            .execute(&pool)
            .await
            .unwrap();
        let runtime = InstalledItemRuntime::load(&pool, "org.ersatzrs.addon.fixture", None)
            .await
            .unwrap();
        (temporary, runtime)
    }

    #[tokio::test]
    async fn retained_play_command_is_child_free_and_uses_v4() {
        let (_temporary, runtime) = fixture("#!/bin/sh\nexit 99\n", true).await;
        let command = runtime
            .retained_play_command(&retained_request())
            .await
            .unwrap();
        assert!(command.args.iter().any(|arg| arg == "play-intervals"));
        assert_eq!(
            command.environment["ERSATZRS_ADDON_CAPABILITY"],
            CAPABILITY_REMOTE_STREAM_PLAY_V4
        );
        let jobs: i64 =
            sqlx::query_scalar(r#"SELECT COUNT(*) FROM "AddonMediaListItemOperationJob""#)
                .fetch_one(&runtime.pool)
                .await
                .unwrap();
        assert_eq!(jobs, 0);
    }

    #[tokio::test]
    async fn retained_play_command_rejects_unsupported_and_invalid_requests() {
        let (_temporary, runtime) = fixture("#!/bin/sh\nexit 99\n", false).await;
        assert!(matches!(
            runtime.retained_play_command(&retained_request()).await,
            Err(ItemRuntimeError::Unsupported)
        ));
        let mut invalid = retained_request();
        invalid.retained_intervals_milliseconds.clear();
        assert!(matches!(
            runtime.retained_play_command(&invalid).await,
            Err(ItemRuntimeError::InvalidRequest)
        ));
    }
}
