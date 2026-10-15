//! Held source and native metadata before any page/window decode qualification.

use std::{path::PathBuf, sync::Arc};

use ersatzrs_core::locking::TeletextWorkHold;
use tokio::time::Instant;

use super::{
    check_deadline, local_teletext, probe_error, FfprobeExtractor, LocalProbeCleanupScope,
    LocalTeletextSourceGuard, SourceTeletextPageDecodeError, SourceTeletextPageDecodeRequest,
};
use crate::ffprobe::teletext_origin::NativeSourceOrigin;

/// Original held association plus native observations, independent of a bitmap.
///
/// Preparation requires no bounded source prefix or transmitted page. The
/// requested page/program/PID has only syntactic request validation here; raw
/// source association, clock epochs, window/PES and actual bitmap qualification
/// remain separate work. Native starts are observations, never cue readiness.
/// The held file retains the existing observed-filesystem limits and does not
/// force an external process to consume this handle or make bytes immutable.
pub struct PreparedLocalTeletextSource {
    pub(super) request: SourceTeletextPageDecodeRequest,
    pub(super) original: PathBuf,
    pub(super) resolved: PathBuf,
    pub(super) guard: Arc<LocalTeletextSourceGuard>,
    pub(super) native: NativeSourceOrigin,
}

impl std::fmt::Debug for PreparedLocalTeletextSource {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter
            .debug_struct("PreparedLocalTeletextSource")
            .field("stream_index", &self.request.stream_index)
            .field("pid", &self.request.pid)
            .field("program_number", &self.request.program_number)
            .field("page", &self.request.page)
            .field("native", &self.native)
            .finish_non_exhaustive()
    }
}

impl PreparedLocalTeletextSource {
    /// Explicit requested identity, without observed page/program readiness.
    #[must_use]
    pub fn request(&self) -> &SourceTeletextPageDecodeRequest {
        &self.request
    }

    /// All optional native start observations, without selecting a media origin.
    #[must_use]
    pub fn native_origin(&self) -> &NativeSourceOrigin {
        &self.native
    }

    /// Recheck both original and resolved locators under the actual work owner.
    /// # Errors
    /// Rejects missing ownership, stale association, deadline/cancellation or
    /// unconfirmed worker cleanup, without returning source paths.
    pub async fn check_source(
        &self,
        deadline: Instant,
        hold: TeletextWorkHold,
    ) -> Result<(), SourceTeletextPageDecodeError> {
        check_deadline(deadline)?;
        if LocalProbeCleanupScope::current().is_none() {
            return Err(SourceTeletextPageDecodeError::NoCleanupScope);
        }
        local_teletext::check_source_guard_owned(
            self.guard.clone(),
            self.original.clone(),
            self.resolved.clone(),
            deadline,
            hold,
        )
        .await
        .map_err(probe_error)
    }
}

impl FfprobeExtractor {
    /// Prepare held-source native observations before page/window decoding.
    ///
    /// One absolute budget of at most fifteen seconds covers owned source
    /// opening, the retained native probe and final source checks. No managed
    /// scope means no worker. Actual workers retain source/admission after
    /// requester loss; the returned owner retains no preparation admission.
    /// This grants no bitmap, source-clock, requested-cue or publication status.
    /// # Errors
    /// Rejects invalid requests, unsafe/stale associations, invalid native
    /// evidence, deadline/cancellation, or unconfirmed process/worker cleanup.
    pub async fn prepare_local_teletext_source(
        &self,
        request: SourceTeletextPageDecodeRequest,
        original: PathBuf,
        resolved: PathBuf,
        deadline: Instant,
        hold: TeletextWorkHold,
    ) -> Result<PreparedLocalTeletextSource, SourceTeletextPageDecodeError> {
        request.validate()?;
        check_deadline(deadline)?;
        if LocalProbeCleanupScope::current().is_none() {
            return Err(SourceTeletextPageDecodeError::NoCleanupScope);
        }
        let guard = local_teletext::open_source_guard_owned(
            original.clone(),
            resolved.clone(),
            request.binding.source_revision.clone(),
            deadline,
            hold.clone(),
        )
        .await
        .map_err(probe_error)?;
        let native = self
            .capture_local_teletext_native_origin(
                guard.clone(),
                (original.clone(), resolved.clone()),
                deadline,
                hold,
            )
            .await?;
        Ok(PreparedLocalTeletextSource {
            request,
            original,
            resolved,
            guard,
            native,
        })
    }
}
