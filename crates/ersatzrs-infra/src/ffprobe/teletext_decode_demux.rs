//! Original-source native demux observations through the existing process owner.

use ersatzrs_core::locking::TeletextWorkHold;
use tokio::time::Instant;

use super::{
    check_deadline, local, probe_error, FfprobeCommand, FfprobeExtractor,
    PreparedLocalTeletextSource, SourceTeletextPageDecodeError,
};
use crate::ffprobe::teletext_demux::{parse_source_demux, NativeSourceDemuxTrace, MAX_PACKETS};

impl FfprobeExtractor {
    /// Observe bounded original-source demux order before choosing a cue window.
    ///
    /// This reuses the prepared guard, actual process/source/work retention,
    /// source bookends and one absolute deadline. Stream-info estimation is
    /// disabled: its MPEG-TS tail seeks can initialize hidden wrap state. PES
    /// parsing/fill-in are disabled and no stream filter or initial seek occurs.
    /// The independent ordinary format-origin probe remains unchanged.
    ///
    /// This is an explicit clean demux observation, not a certificate that the
    /// ordinary native probe/playback initialized identical wrap state. Program
    /// order, actual raw PES positions/timestamps, shared-state inheritance and
    /// ordinary native starts must still be reconciled before PCR/cue admission.
    /// Missing positions/timestamps remain unknown and a capped trace proves no
    /// EOF, complete source inventory or absence of later streams/pages.
    /// # Errors
    /// Rejects missing ownership, stale source, malformed/bounded native output,
    /// deadline/cancellation or unconfirmed actual process cleanup.
    pub async fn observe_prepared_local_demux_trace(
        &self,
        source: &PreparedLocalTeletextSource,
        deadline: Instant,
        hold: TeletextWorkHold,
    ) -> Result<NativeSourceDemuxTrace, SourceTeletextPageDecodeError> {
        check_deadline(deadline)?;
        source.check_source(deadline, hold.clone()).await?;
        let command = self.source_demux_command(&source.resolved)?;
        let bytes = local::capture_retained(
            command,
            "source teletext native demux trace".into(),
            Some(deadline),
            Some(local::ProbeRetention::new(
                hold.clone(),
                source.guard.clone(),
            )),
        )
        .await
        .map_err(probe_error)?;
        let observed =
            parse_source_demux(&bytes).map_err(|_| SourceTeletextPageDecodeError::InvalidOutput)?;
        source.check_source(deadline, hold).await?;
        Ok(observed)
    }

    pub(super) fn source_demux_command(
        &self,
        source: &std::path::Path,
    ) -> Result<FfprobeCommand, SourceTeletextPageDecodeError> {
        let source = source
            .to_str()
            .ok_or(SourceTeletextPageDecodeError::SourceChanged)?;
        Ok(FfprobeCommand {
            program: self.ffprobe_path.clone(),
            args: [
                "-v", "error", "-nofind_stream_info",
                "-fflags", "+noparse+nofillin", "-correct_ts_overflow", "1",
                "-read_intervals", &format!("%+#{MAX_PACKETS}"),
                "-show_packets", "-show_streams", "-show_programs",
                "-show_entries", "packet=stream_index,pts,dts,pos:program=program_num,pmt_pid,pcr_pid:program_stream=index:stream=index,id,codec_type,time_base",
                "-of", "json", "-f", "mpegts", "-i", source,
            ].into_iter().map(str::to_owned).collect(),
        })
    }
}
