//! Same-source consumer-origin acquisition through the existing retained owner.

use std::{path::PathBuf, sync::Arc};

use ersatzrs_core::locking::TeletextWorkHold;
use tokio::time::Instant;

use super::{
    DecodedLocalTeletextPage, FfprobeCommand, FfprobeExtractor, LocalTeletextSourceGuard,
    PreparedLocalTeletextSource, SourceTeletextPageDecodeError,
};
use crate::ffprobe::teletext_consumer_origin::{
    parse_consumer_origin, validate_enabled, NativeSourceConsumerOrigin,
};

impl FfprobeExtractor {
    /// Observe exact explicitly enabled streams from the original held source.
    ///
    /// The probe uses the native builder's standard format flags, normal stream
    /// discovery, default probe size and explicit overflow correction. It shares
    /// the existing retained process, work hold, source bookends and absolute
    /// deadline. Subtitle starts participate alongside AV; ordinary metadata
    /// and the independent AV-only origin observation remain unchanged.
    ///
    /// Callers must still bind these indices to the actual physical input and
    /// final command policy, including hardware fallback/reopened inputs. This
    /// observation is not proof of seek landing, cue normalization or admission.
    /// # Errors
    /// Rejects invalid enabled indices, missing ownership, changed source,
    /// malformed optional metadata, deadline/cancellation or uncertain cleanup.
    pub async fn observe_prepared_local_consumer_origin(
        &self,
        source: &PreparedLocalTeletextSource,
        enabled: &[i32],
        deadline: Instant,
        hold: TeletextWorkHold,
    ) -> Result<NativeSourceConsumerOrigin, SourceTeletextPageDecodeError> {
        self.capture_local_teletext_consumer_origin(
            source.guard.clone(),
            (source.original.clone(), source.resolved.clone()),
            enabled,
            deadline,
            hold,
        )
        .await
    }

    /// Observe explicitly enabled streams using an already decoded source guard.
    ///
    /// Reuses the receipt's exact held association and locators without opening
    /// another guard, preparing another source or probing the decoder snapshot.
    /// The existing retained worker owns source/admission through actual cleanup.
    /// Its fixed, unseeked origin-probe policy is identical to the prepared-source
    /// API above; caller-supplied enabled indices are observations, not evidence
    /// that arbitrary final decoder options or consumer policies are equivalent.
    /// This grants no seek landing, cue, bitmap presentation or playback admission.
    ///
    /// # Errors
    /// Rejects invalid enabled indices, missing ownership, changed source,
    /// malformed optional metadata, deadline/cancellation or uncertain cleanup.
    pub async fn observe_local_teletext_consumer_origin(
        &self,
        source: &DecodedLocalTeletextPage,
        enabled: &[i32],
        deadline: Instant,
        hold: TeletextWorkHold,
    ) -> Result<NativeSourceConsumerOrigin, SourceTeletextPageDecodeError> {
        self.capture_local_teletext_consumer_origin(
            source.guard.clone(),
            (source.original.clone(), source.resolved.clone()),
            enabled,
            deadline,
            hold,
        )
        .await
    }

    async fn capture_local_teletext_consumer_origin(
        &self,
        guard: Arc<LocalTeletextSourceGuard>,
        paths: (PathBuf, PathBuf),
        enabled: &[i32],
        deadline: Instant,
        hold: TeletextWorkHold,
    ) -> Result<NativeSourceConsumerOrigin, SourceTeletextPageDecodeError> {
        validate_enabled(enabled).map_err(|_| SourceTeletextPageDecodeError::InvalidRequest)?;
        let command = self.consumer_origin_command(&paths.1)?;
        self.capture_local_teletext_origin(guard, paths, command, deadline, hold, |bytes| {
            parse_consumer_origin(bytes, enabled)
                .map_err(|_| SourceTeletextPageDecodeError::InvalidOutput)
        })
        .await
    }

    pub(super) fn consumer_origin_command(
        &self,
        source: &std::path::Path,
    ) -> Result<FfprobeCommand, SourceTeletextPageDecodeError> {
        let mut command = self.source_origin_command(source)?;
        // Global options precede all input opening, exactly as in the builder.
        command.args.splice(
            0..0,
            ersatzrs_ffmpeg::standard_format_flags()
                .global_options()
                .iter()
                .cloned(),
        );
        Ok(command)
    }
}

#[cfg(test)]
#[path = "teletext_decode_consumer_tests.rs"]
mod tests;
