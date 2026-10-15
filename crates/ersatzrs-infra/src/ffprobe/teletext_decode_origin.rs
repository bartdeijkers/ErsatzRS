//! Optional native AV origin, using the existing retained source/process owner.

use std::{path::PathBuf, sync::Arc};

use ersatzrs_core::locking::TeletextWorkHold;
use tokio::time::Instant;

use super::{
    check_deadline, local, local_teletext, probe_error, DecodedLocalTeletextPage, FfprobeCommand,
    FfprobeExtractor, LocalProbeCleanupScope, LocalTeletextSourceGuard,
    SourceTeletextAvOriginEvidence, SourceTeletextPageDecodeError,
};
use crate::ffprobe::teletext_origin::{parse_source_origin, NativeSourceOrigin};

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
        let origin = self
            .capture_local_teletext_native_origin(
                decoded.guard.clone(),
                (decoded.original.clone(), decoded.resolved.clone()),
                deadline,
                hold.clone(),
            )
            .await?;
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

    /// The same native source capture is available before bitmap decoding.
    /// Only the existing source association and actual process owner are used.
    pub(super) async fn capture_local_teletext_native_origin(
        &self,
        guard: Arc<LocalTeletextSourceGuard>,
        paths: (PathBuf, PathBuf),
        deadline: Instant,
        hold: TeletextWorkHold,
    ) -> Result<NativeSourceOrigin, SourceTeletextPageDecodeError> {
        self.capture_local_teletext_origin(
            guard,
            paths.clone(),
            self.source_origin_command(&paths.1)?,
            deadline,
            hold,
            |output| {
                parse_source_origin(output)
                    .map_err(|_| SourceTeletextPageDecodeError::InvalidOutput)
            },
        )
        .await
    }

    /// Shared original-source capture, with actual worker and source ownership.
    pub(super) async fn capture_local_teletext_origin<T>(
        &self,
        guard: Arc<LocalTeletextSourceGuard>,
        paths: (PathBuf, PathBuf),
        command: FfprobeCommand,
        deadline: Instant,
        hold: TeletextWorkHold,
        parse: impl FnOnce(&[u8]) -> Result<T, SourceTeletextPageDecodeError>,
    ) -> Result<T, SourceTeletextPageDecodeError> {
        check_deadline(deadline)?;
        if LocalProbeCleanupScope::current().is_none() {
            return Err(SourceTeletextPageDecodeError::NoCleanupScope);
        }
        local_teletext::check_source_guard_owned(
            guard.clone(),
            paths.0.clone(),
            paths.1.clone(),
            deadline,
            hold.clone(),
        )
        .await
        .map_err(probe_error)?;
        let output = local::capture_retained(
            command,
            "source teletext native AV origin".into(),
            Some(deadline),
            Some(local::ProbeRetention::new(hold.clone(), guard.clone())),
        )
        .await
        .map_err(probe_error)?;
        let origin = parse(&output)?;
        check_deadline(deadline)?;
        local_teletext::check_source_guard_owned(guard, paths.0, paths.1, deadline, hold)
            .await
            .map_err(probe_error)?;
        Ok(origin)
    }

    pub(super) fn source_origin_command(
        &self,
        source: &std::path::Path,
    ) -> Result<FfprobeCommand, SourceTeletextPageDecodeError> {
        let path = source
            .to_str()
            .ok_or(SourceTeletextPageDecodeError::SourceChanged)?;
        Ok(FfprobeCommand {
            program: self.ffprobe_path.clone(),
            args: [
                "-v",
                "error",
                "-probesize",
                "5000000",
                "-correct_ts_overflow",
                "1",
                "-show_format",
                "-show_streams",
                "-show_entries",
                "format=start_time,format_name:stream=index,id,codec_type,start_pts,time_base",
                "-of",
                "json",
                "-i",
            ]
            .into_iter()
            .map(str::to_owned)
            .chain(std::iter::once(path.to_owned()))
            .collect(),
        })
    }
}
