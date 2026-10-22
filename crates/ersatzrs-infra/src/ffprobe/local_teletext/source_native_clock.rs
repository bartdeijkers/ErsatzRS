//! Held first-global demux witness and default native program/PCR coordinates.

use ersatzrs_ffmpeg::teletext::{
    source_av_timestamps::{
        SourceAvPesHeader, SourceAvStreamIdentity, SourceAvStreamKind, SourceAvTimestampObserver,
    },
    source_native_epoch::SourceNativeEpoch,
    source_psi::ObservedSourcePsi,
};

use super::{
    source_clock::SourceSelection, SourceTeletextAvOriginEvidence, SourceTeletextPacketFormat,
};
use crate::ffprobe::{
    teletext_decode::SourceTeletextPageDecodeRequest,
    teletext_demux::{NativeSourceDemuxPacket, NativeSourceDemuxTrace},
    teletext_origin::NativeSourceOrigin,
    FfprobeError, Result,
};

/// Default MPEG-TS probe epoch corroborated against the actual held raw source.
///
/// Only the owned source walk constructs this evidence. The first GLOBAL native
/// packet, not a selected first AV guess, has a clean observed disposition,
/// matching raw header and complete stable selected PSI before its PES start.
/// The normal native probe uses positive probing and overflow correction. The
/// audited MPEG-TS NOHEADER/FIFO path initializes that first program before tail
/// estimation. This proof currently requires one stable program and an AV first
/// initializer; other cases remain unknown, not permanent product exclusions.
///
/// Source bookends retain their observed-filesystem limits. This is neither an
/// immutable-byte certificate nor authorization for arbitrary consumer flags.
/// Playback must qualify its actual input policy, effective cue bounds, gaps,
/// page headers and decoder timestamps before admission. No cue follows here.
#[derive(Debug, Clone)]
pub struct SourceTeletextNativeClockEvidence {
    request: SourceTeletextPageDecodeRequest,
    av: SourceTeletextAvOriginEvidence,
    epoch: SourceNativeEpoch,
    first_packet: NativeSourceDemuxPacket,
    first_header: SourceAvPesHeader,
    first_pcr: i64,
    last_pcr: i64,
}

impl SourceTeletextNativeClockEvidence {
    /// Exact prepared source binding, program, PID, global index and requested page.
    #[must_use]
    pub fn request(&self) -> &SourceTeletextPageDecodeRequest {
        &self.request
    }

    #[must_use]
    pub fn av_origin(&self) -> &SourceTeletextAvOriginEvidence {
        &self.av
    }
    #[must_use]
    pub const fn epoch(&self) -> SourceNativeEpoch {
        self.epoch
    }
    #[must_use]
    pub const fn first_packet(&self) -> NativeSourceDemuxPacket {
        self.first_packet
    }
    #[must_use]
    pub const fn first_header(&self) -> SourceAvPesHeader {
        self.first_header
    }
    /// Native program coordinate, not media-relative seek time.
    #[must_use]
    pub const fn first_pcr_ticks(&self) -> i64 {
        self.first_pcr
    }
    /// Last actually observed PCR; trailing packets do not become time coverage.
    #[must_use]
    pub const fn last_pcr_ticks(&self) -> i64 {
        self.last_pcr
    }
}

pub(super) struct NativeClockCollector {
    request: SourceTeletextPageDecodeRequest,
    selection: SourceSelection,
    trace: NativeSourceDemuxTrace,
    epoch: SourceNativeEpoch,
    first_packet: NativeSourceDemuxPacket,
    observer: SourceAvTimestampObserver,
    header: Option<SourceAvPesHeader>,
    mismatched: bool,
    format: Option<SourceTeletextPacketFormat>,
    first_source_packet: Option<u64>,
}

impl NativeClockCollector {
    pub(super) fn new(
        request: &SourceTeletextPageDecodeRequest,
        origin: &NativeSourceOrigin,
        trace: NativeSourceDemuxTrace,
        format: Option<SourceTeletextPacketFormat>,
    ) -> Option<Self> {
        if origin.format_name() != Some("mpegts") || trace.programs().len() != 1 {
            return None;
        }
        let program = trace.programs().first()?;
        if program.number() != request.program_number
            || !(0x20..=0x1ffe).contains(&program.pcr_pid())
        {
            return None;
        }
        if program.stream_indices().len() != trace.streams().len()
            || !trace
                .streams()
                .iter()
                .all(|stream| program.stream_indices().contains(&stream.index()))
            || origin.streams().len()
                != trace
                    .streams()
                    .iter()
                    .filter(|stream| matches!(stream.codec_type(), "audio" | "video"))
                    .count()
            || !origin.streams().iter().all(|native| {
                trace.streams().iter().any(|stream| {
                    stream.index() == native.identity().stream_index
                        && stream.pid() == native.identity().pid
                        && matches!(
                            (stream.codec_type(), native.identity().kind),
                            ("audio", SourceAvStreamKind::Audio)
                                | ("video", SourceAvStreamKind::Video)
                        )
                })
            })
        {
            return None;
        }
        let caption = trace
            .streams()
            .iter()
            .find(|stream| stream.index() == request.stream_index)?;
        if caption.pid() != request.pid || !program.stream_indices().contains(&caption.index()) {
            return None;
        }
        let first = *trace.packets().first()?;
        if first.is_corrupt() != Some(false)
            || first.is_discard() != Some(false)
            || first.position().is_none()
            || (first.pts().is_none() && first.dts().is_none())
            || !program.stream_indices().contains(&first.stream_index())
        {
            return None;
        }
        // A pending source still needs its physical format established inside
        // the owned walk. Explicit-format construction keeps strict alignment.
        let first_source_packet = match format {
            Some(format) => Some(format.source_packet_at(first.position()?)?),
            None => None,
        };
        let stream = trace
            .streams()
            .iter()
            .find(|stream| stream.index() == first.stream_index())?;
        let base = stream.time_base()?;
        if (base.numerator(), base.denominator()) != (1, 90_000) {
            return None;
        }
        let kind = match stream.codec_type() {
            "video" => SourceAvStreamKind::Video,
            "audio" => SourceAvStreamKind::Audio,
            _ => return None,
        };
        let identity = SourceAvStreamIdentity {
            stream_index: stream.index(),
            pid: stream.pid(),
            kind,
        };
        if !origin
            .streams()
            .iter()
            .any(|native| native.identity() == identity)
        {
            return None;
        }
        let selection = SourceSelection {
            pid: request.pid,
            program: program.number(),
            pmt: program.pmt_pid(),
            pcr: program.pcr_pid(),
        };
        let epoch = SourceNativeEpoch::from_first_demux_timestamps(first.pts(), first.dts());
        Some(Self {
            request: request.clone(),
            selection,
            epoch,
            first_packet: first,
            observer: SourceAvTimestampObserver::new_source_epoch(identity, 0).ok()?,
            trace,
            header: None,
            mismatched: false,
            format,
            first_source_packet,
        })
    }

    pub(super) fn bind_format(&mut self, format: SourceTeletextPacketFormat) -> Result<()> {
        if self.format.is_some() {
            return Err(invalid());
        }
        self.format = Some(format);
        self.first_source_packet = self
            .first_packet
            .position()
            .and_then(|position| format.source_packet_at(position));
        self.mismatched |= self.first_source_packet.is_none();
        Ok(())
    }

    pub(super) const fn epoch(&self) -> SourceNativeEpoch {
        self.epoch
    }

    pub(super) const fn selection(&self) -> SourceSelection {
        self.selection
    }

    pub(super) fn push(&mut self, raw: &[u8; 188]) -> Result<()> {
        if self.format.is_none() {
            return Err(invalid());
        }
        if let Some(header) = self.observer.push(raw).map_err(|_| invalid())? {
            let Some(first) = self.first_source_packet else {
                self.mismatched = true;
                return Ok(());
            };
            if header.source_pusi_packet < first {
                self.mismatched = true;
            }
            if header.source_pusi_packet == first {
                self.mismatched |= mapped(self.epoch, header.pts)? != self.first_packet.pts()
                    || mapped(self.epoch, header.dts.or(header.pts))? != self.first_packet.dts();
                self.header = Some(header);
            }
        }
        Ok(())
    }

    pub(super) fn finish(
        mut self,
        av: SourceTeletextAvOriginEvidence,
        psi: &ObservedSourcePsi,
    ) -> Result<Option<SourceTeletextNativeClockEvidence>> {
        self.observer.finish().map_err(|_| invalid())?;
        let Some(header) = self.header else {
            return Ok(None);
        };
        let clock = av.clock();
        if self.mismatched
            || clock.first_current_psi_packet() >= header.source_pusi_packet
            || psi.programs.len() != 1
        {
            return Ok(None);
        }
        let program = &psi.programs[0];
        if self.trace.streams().len() != program.streams.len()
            || !self
                .trace
                .streams()
                .iter()
                .all(|stream| program.streams.iter().any(|raw| raw.pid == stream.pid()))
        {
            return Ok(None);
        }
        // Corroborate the signed default-probe starts, rather than modulo-only
        // equality or selecting one audio/video track as container origin.
        if av.streams().is_empty() {
            return Ok(None);
        }
        for stream in av.streams() {
            let Some(sample) = stream.matched_native_start() else {
                return Ok(None);
            };
            let Some(base) = stream.native().time_base() else {
                return Ok(None);
            };
            let Some(start) = stream.native().start_pts() else {
                return Ok(None);
            };
            let value = i128::from(start) * i128::from(base.numerator()) * 90_000;
            let divisor = i128::from(base.denominator());
            if value % divisor != 0
                || mapped(self.epoch, Some(sample.raw_ticks_90khz))?.map(i128::from)
                    != Some(value / divisor)
            {
                return Ok(None);
            }
        }
        let report = clock.clock();
        let (Some(first), Some(last)) = (report.first, report.last) else {
            return Ok(None);
        };
        let first_pcr = self
            .epoch
            .apply_pcr_ticks(first.raw_ticks)
            .map_err(|_| invalid())?
            .ok_or_else(invalid)?;
        let last_pcr = self
            .epoch
            .apply_pcr_ticks(last.raw_ticks)
            .map_err(|_| invalid())?
            .ok_or_else(invalid)?;
        if first_pcr.checked_add(i64::try_from(last.elapsed_ticks).map_err(|_| invalid())?)
            != Some(last_pcr)
        {
            return Ok(None);
        }
        Ok(Some(SourceTeletextNativeClockEvidence {
            request: self.request,
            av,
            epoch: self.epoch,
            first_packet: self.first_packet,
            first_header: header,
            first_pcr,
            last_pcr,
        }))
    }
}

fn mapped(epoch: SourceNativeEpoch, ticks: Option<u64>) -> Result<Option<i64>> {
    ticks
        .map(|ticks| {
            epoch
                .apply_timestamp(i64::try_from(ticks).map_err(|_| invalid())?)
                .map_err(|_| invalid())
        })
        .transpose()
        .map(Option::flatten)
}

fn invalid() -> FfprobeError {
    FfprobeError::InvalidOutput {
        path: "source teletext decode window".into(),
    }
}

#[cfg(test)]
mod tests;
