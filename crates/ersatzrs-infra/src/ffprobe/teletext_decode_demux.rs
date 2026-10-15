//! Original-source native demux observations through the existing process owner.

use ersatzrs_core::locking::TeletextWorkHold;
use tokio::time::Instant;

use super::{
    check_deadline, local, local_teletext, probe_error, DecodedLocalTeletextPage, FfprobeCommand,
    FfprobeExtractor, PreparedLocalTeletextSource, SourceTeletextCueEvidence,
    SourceTeletextCueRequest, SourceTeletextNativeClockEvidence, SourceTeletextPageDecodeError,
};
use crate::ffprobe::teletext_demux::{parse_source_demux, NativeSourceDemuxTrace, MAX_PACKETS};

impl FfprobeExtractor {
    /// Decode the located cue span and join its bytes back to the original
    /// native-format clock receipt. Exact source/request identity must match;
    /// the existing held raw walk checks the retained decoder digest and the
    /// original full-source digest under the same work/deadline ownership.
    /// The exact protected prelude header/PTS must survive snapshot filtering at
    /// its mapped physical completion position. This still grants no complete
    /// inherited page context or enabled-stream consumer normalization.
    /// # Errors
    /// Rejects mismatched requests, changed bytes, absent current headers/bitmaps,
    /// invalid transport, deadline/cancellation or uncertain actual cleanup.
    pub async fn decode_prepared_local_teletext_cue(
        &self,
        source: &PreparedLocalTeletextSource,
        cue: &SourceTeletextCueEvidence,
        deadline: Instant,
        hold: TeletextWorkHold,
    ) -> Result<DecodedLocalTeletextPage, SourceTeletextPageDecodeError> {
        if source.request() != cue.native_clock().request() {
            return Err(SourceTeletextPageDecodeError::SourceChanged);
        }
        let decoded = self
            .decode_prepared_local_teletext_page(source, cue.span(), deadline, hold.clone())
            .await?;
        let retained = decoded.headers().is_some_and(|headers| {
            cue.page_prelude().is_retained_in(
                headers,
                decoded.window().first_source_packet,
                decoded.window().skipped_selected_prefix_packets,
            )
        });
        if !retained {
            return Err(SourceTeletextPageDecodeError::UnobservedPage);
        }
        let actual = decoded.observe_source_clock(deadline, hold).await?;
        if actual.source_sha256() != cue.native_clock().av_origin().clock().source_sha256() {
            return Err(SourceTeletextPageDecodeError::SourceChanged);
        }
        Ok(decoded)
    }

    /// Locate bounded native-format cue bookends from the original held source.
    ///
    /// Native format-origin precision, first-global epoch, stable PSI/AV and
    /// selected PCR observations share the existing source/worker/deadline.
    /// The returned span still needs its own current headers/PES and decoding.
    /// Consumer enabled-stream origin and seek semantics are not granted here.
    /// # Errors
    /// Rejects stale sources, malformed transport, expired deadlines, lost
    /// cancellation ownership or unconfirmed worker cleanup. Unknown numeric,
    /// epoch, gap, reset/header prelude or bounded-span evidence returns `None`
    /// without complete cue admission.
    pub async fn observe_prepared_local_native_cue(
        &self,
        source: &PreparedLocalTeletextSource,
        cue: SourceTeletextCueRequest,
        deadline: Instant,
        hold: TeletextWorkHold,
    ) -> Result<Option<SourceTeletextCueEvidence>, SourceTeletextPageDecodeError> {
        let trace = self
            .observe_prepared_local_demux_trace(source, deadline, hold.clone())
            .await?;
        local_teletext::observe_native_cue_owned(
            source.guard.clone(),
            (source.original.clone(), source.resolved.clone()),
            (source.request.clone(), source.native.clone(), trace),
            cue,
            deadline,
            hold,
        )
        .await
        .map_err(probe_error)
    }

    /// Bind the clean first-global AV initializer to the held raw walk and the
    /// explicitly configured ordinary native probe's program epoch.
    /// Unknown dispositions or non-AV/shared/late initialization return `None`.
    /// This establishes no actual cue, bounded PCR gap, charset, page readiness
    /// or eligibility for arbitrary later consumer input options.
    /// # Errors
    /// Rejects missing ownership, stale sources, invalid transport, expired
    /// deadlines/cancellation and unconfirmed actual worker cleanup.
    pub async fn observe_prepared_local_native_clock(
        &self,
        source: &PreparedLocalTeletextSource,
        deadline: Instant,
        hold: TeletextWorkHold,
    ) -> Result<Option<SourceTeletextNativeClockEvidence>, SourceTeletextPageDecodeError> {
        let trace = self
            .observe_prepared_local_demux_trace(source, deadline, hold.clone())
            .await?;
        local_teletext::observe_native_clock_owned(
            source.guard.clone(),
            (source.original.clone(), source.resolved.clone()),
            (source.request.clone(), source.native.clone(), trace),
            deadline,
            hold,
        )
        .await
        .map_err(probe_error)
    }

    /// Observe bounded original-source demux order before choosing a cue window.
    ///
    /// This reuses the prepared guard, actual process/source/work retention,
    /// source bookends and one absolute deadline. Stream-info estimation is
    /// disabled: its MPEG-TS tail seeks can initialize hidden wrap state. PES
    /// parsing/fill-in are disabled and no stream filter or initial seek occurs.
    /// The independent format-origin probe retains ordinary stream-info behavior.
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
                "-show_entries", "packet=stream_index,pts,dts,pos,flags:program=program_num,pmt_pid,pcr_pid:program_stream=index:stream=index,id,codec_type,time_base",
                "-of", "json", "-f", "mpegts", "-i", source,
            ].into_iter().map(str::to_owned).collect(),
        })
    }
}
