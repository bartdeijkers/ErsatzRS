//! Optional native AV origin, using the existing retained source/process owner.

use ersatzrs_core::locking::TeletextWorkHold;
use tokio::time::Instant;

use super::{
    check_deadline, local, local_teletext, probe_error, DecodedLocalTeletextPage, FfprobeCommand,
    FfprobeExtractor, LocalProbeCleanupScope, SourceTeletextAvOriginEvidence,
    SourceTeletextPageDecodeError,
};
use crate::ffprobe::teletext_origin::parse_source_origin;

impl FfprobeExtractor {
    /// Preserve native format/AV start observations from the original source and
    /// compare their raw PES witnesses during the same owned continuous read.
    ///
    /// This probes the original resolved source, never the later private decoder
    /// snapshot. It reuses the process runner, original held association, work
    /// admission and one absolute deadline. Ordinary metadata is unchanged.
    /// Decimal format time, rational native starts, raw PTS/DTS and PCR remain
    /// distinct; this report alone confers no requested-cue or epoch admission.
    /// Source bookends retain their documented observed-filesystem limits.
    ///
    /// # Errors
    /// Rejects missing scope, stale source, invalid optional native evidence,
    /// raw source failures, deadline/cancellation or uncertain cleanup.
    pub async fn observe_local_teletext_av_origin(
        &self,
        decoded: &DecodedLocalTeletextPage,
        deadline: Instant,
        hold: TeletextWorkHold,
    ) -> Result<SourceTeletextAvOriginEvidence, SourceTeletextPageDecodeError> {
        check_deadline(deadline)?;
        if LocalProbeCleanupScope::current().is_none() {
            return Err(SourceTeletextPageDecodeError::NoCleanupScope);
        }
        decoded.check_source(deadline, hold.clone()).await?;
        let output = local::capture_retained(
            self.source_origin_command(&decoded.resolved),
            "source teletext native AV origin".into(),
            Some(deadline),
            Some(local::ProbeRetention::new(
                hold.clone(),
                decoded.guard.clone(),
            )),
        )
        .await
        .map_err(probe_error)?;
        let origin = parse_source_origin(&output)
            .map_err(|_| SourceTeletextPageDecodeError::InvalidOutput)?;
        decoded.check_source(deadline, hold.clone()).await?;
        local_teletext::observe_source_av_origin_owned(
            decoded.guard.clone(),
            (decoded.original.clone(), decoded.resolved.clone()),
            decoded.request.pid,
            decoded.window,
            origin,
            deadline,
            hold,
        )
        .await
        .map_err(probe_error)
    }

    fn source_origin_command(&self, source: &std::path::Path) -> FfprobeCommand {
        FfprobeCommand {
            program: self.ffprobe_path.clone(),
            args: [
                "-v",
                "error",
                "-show_format",
                "-show_streams",
                "-show_entries",
                "format=start_time:stream=index,id,codec_type,start_pts,time_base",
                "-of",
                "json",
                "-i",
            ]
            .into_iter()
            .map(str::to_owned)
            .chain(std::iter::once(source.to_string_lossy().into_owned()))
            .collect(),
        }
    }
}
