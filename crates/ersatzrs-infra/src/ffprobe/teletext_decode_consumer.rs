//! Same-source consumer-origin acquisition through the existing retained owner.

use ersatzrs_core::locking::TeletextWorkHold;
use tokio::time::Instant;

use super::{
    FfprobeCommand, FfprobeExtractor, PreparedLocalTeletextSource, SourceTeletextPageDecodeError,
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
        validate_enabled(enabled).map_err(|_| SourceTeletextPageDecodeError::InvalidRequest)?;
        self.capture_local_teletext_origin(
            source.guard.clone(),
            (source.original.clone(), source.resolved.clone()),
            self.consumer_origin_command(&source.resolved)?,
            deadline,
            hold,
            |bytes| {
                parse_consumer_origin(bytes, enabled)
                    .map_err(|_| SourceTeletextPageDecodeError::InvalidOutput)
            },
        )
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
