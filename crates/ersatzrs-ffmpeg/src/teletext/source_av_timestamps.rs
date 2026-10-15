//! Bounded raw AV PES-header timestamps, without presentation-origin inference.

use mpeg2ts_reader::pes::{PesContents, PesError, PesHeader, PtsDts};

use super::signalling::valid_packet;

const MAX_HEADER_BYTES: usize = 9 + 255;

/// Native-observed AV role, not a codec inferred from a partial stream-type list.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SourceAvStreamKind {
    /// One explicitly identified audio stream.
    Audio,
    /// One explicitly identified video stream.
    Video,
}

/// Caller-bound native stream identity; actual program membership is separate.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct SourceAvStreamIdentity {
    /// Native global stream index, not the PID or PES stream identifier.
    pub stream_index: i32,
    /// Actual elementary PID from the source-associated native stream record.
    pub pid: u16,
    /// Native audio/video classification.
    pub kind: SourceAvStreamKind,
}

/// One checked wire timestamp, retaining its actual header source positions.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct SourceAvTimestampSample {
    /// Absolute physical packet where this PES header began.
    pub source_pusi_packet: u64,
    /// Absolute physical packet where its complete optional header was available.
    pub header_completion_packet: u64,
    /// Raw 33-bit, 90 kHz PTS or DTS; neither unwrapped nor media-relative.
    pub raw_ticks_90khz: u64,
}

/// One completed optional header, without AV payload or full PES validation.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct SourceAvPesHeader {
    /// Absolute physical packet containing the PES start.
    pub source_pusi_packet: u64,
    /// Absolute physical packet completing this header.
    pub header_completion_packet: u64,
    /// Supplied PES stream identifier, distinct from native stream index/PID.
    pub stream_id: u8,
    /// Checked raw 33-bit PTS; absence is distinct from zero.
    pub pts: Option<u64>,
    /// Checked raw 33-bit DTS; absence is distinct from zero.
    pub dts: Option<u64>,
}

/// Historical header evidence; no decode, epoch or media-origin qualification.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct SourceAvTimestampReport {
    /// Immutable caller-supplied AV identity, not independently discovered here.
    pub stream: SourceAvStreamIdentity,
    /// Absolute first packet ordinal supplied at construction.
    pub first_source_packet: u64,
    /// All successful physical packets, including null and unrelated PIDs.
    pub packets_seen: u64,
    /// Complete nonduplicate PES headers, whether timestamps were present or not.
    pub headers_seen: u64,
    /// Complete headers explicitly carrying neither PTS nor DTS.
    pub headers_without_timestamps: u64,
    /// First actually observed PTS, independently of initial missing timestamps.
    pub first_pts: Option<SourceAvTimestampSample>,
    /// Most recently observed PTS in wire order; video reorder is allowed.
    pub last_pts: Option<SourceAvTimestampSample>,
    /// First actually observed DTS, independently of initial PTS-only headers.
    pub first_dts: Option<SourceAvTimestampSample>,
    /// Most recently observed DTS in wire order; no epoch is unwrapped here.
    pub last_dts: Option<SourceAvTimestampSample>,
    /// A selected PES header started but has not yet been completed.
    pub pending_header: bool,
    /// Selected payload preceded the first observed PES start and was skipped.
    pub unobserved_prefix: bool,
}

/// Static sticky failures; source bytes and paths are never retained in errors.
#[derive(Debug, Clone, Copy, PartialEq, Eq, thiserror::Error)]
pub enum SourceAvTimestampError {
    /// Invalid native index or reserved/null/out-of-range elementary PID.
    #[error("source AV timestamp identity is invalid")]
    Identity,
    /// Invalid TS188 header, adaptation extent, scrambling or transport error.
    #[error("source AV timestamp transport framing is invalid")]
    Framing,
    /// Selected PID signalled discontinuity outside its explicit first packet.
    #[error("source AV timestamp transport signals a discontinuity")]
    Discontinuity,
    /// Selected continuity counter or repeated payload bytes differ.
    #[error("source AV timestamp transport continuity is invalid")]
    Continuity,
    /// A new start interrupted a header, or a start had no payload.
    #[error("source AV timestamp PES start is unexpected")]
    UnexpectedStart,
    /// PES prefix, marker, scrambling, length or optional-header framing failed.
    #[error("source AV timestamp PES header is invalid")]
    PesHeader,
    /// Timestamp flags, required prefixes or marker bits failed validation.
    #[error("source AV timestamp protection is invalid")]
    Timestamp,
    /// A finite observation ended inside a selected PES header.
    #[error("source AV timestamp observation ended inside a PES header")]
    IncompleteHeader,
    /// A lifetime counter or absolute exclusive-end position overflowed.
    #[error("source AV timestamp arithmetic exceeds its bound")]
    Arithmetic,
    /// Further packets were supplied after successful finite closure.
    #[error("source AV timestamp observation is closed")]
    Closed,
}

/// Constant-memory header observer for one caller-owned unchanged source/PID.
///
/// Supply every TS188 packet exactly once in physical source order. The caller
/// must bind the native identity to current source PSI and retain source
/// ownership. All packets count; only the selected PID contributes PES headers.
/// At most 264 header bytes and one previous 188-byte payload packet are retained.
/// Adaptation plus payload and split headers are supported; AV payload is skipped.
///
/// Timestamp prefixes/markers and optional-header extent are checked using the
/// dependency parser. Other optional fields and AV payload are not fully
/// validated. Raw PTS/DTS may reorder or wrap; no monotonicity, unwrapping,
/// media origin, PCR association, subtitle fix or playback readiness is inferred.
#[derive(Debug)]
pub struct SourceAvTimestampObserver {
    report: SourceAvTimestampReport,
    header: [u8; MAX_HEADER_BYTES],
    header_len: usize,
    pusi: Option<u64>,
    started: bool,
    counter: Option<u8>,
    previous: Option<[u8; 188]>,
    initial_discontinuity_allowed: bool,
    closed: bool,
    failure: Option<SourceAvTimestampError>,
}

impl SourceAvTimestampObserver {
    /// Begin an explicit source range without admitting discontinuities.
    /// # Errors
    /// Rejects a negative native stream index or a reserved/null/invalid PID.
    pub fn new(
        stream: SourceAvStreamIdentity,
        first_source_packet: u64,
    ) -> Result<Self, SourceAvTimestampError> {
        if stream.stream_index < 0 || !(0x20..=0x1ffe).contains(&stream.pid) {
            return Err(SourceAvTimestampError::Identity);
        }
        Ok(Self {
            report: SourceAvTimestampReport {
                stream,
                first_source_packet,
                packets_seen: 0,
                headers_seen: 0,
                headers_without_timestamps: 0,
                first_pts: None,
                last_pts: None,
                first_dts: None,
                last_dts: None,
                pending_header: false,
                unobserved_prefix: false,
            },
            header: [0; MAX_HEADER_BYTES],
            header_len: 0,
            pusi: None,
            started: false,
            counter: None,
            previous: None,
            initial_discontinuity_allowed: false,
            closed: false,
            failure: None,
        })
    }

    /// Allow discontinuity only on the first selected-PID packet, with or
    /// without payload. Other PIDs do not consume the allowance.
    /// # Errors
    /// Rejects the same invalid native identities as [`Self::new`].
    pub fn new_source_epoch(
        stream: SourceAvStreamIdentity,
        first_source_packet: u64,
    ) -> Result<Self, SourceAvTimestampError> {
        let mut observer = Self::new(stream, first_source_packet)?;
        observer.initial_discontinuity_allowed = true;
        Ok(observer)
    }

    /// Inspect one actual packet, emitting only a newly completed PES header.
    /// Duplicate packets and incomplete headers emit nothing; the report retains
    /// pending/prefix state. Missing timestamps in a complete header are valid.
    /// # Errors
    /// Every transport, header, timestamp, closure or arithmetic error is sticky.
    pub fn push(
        &mut self,
        raw: &[u8; 188],
    ) -> Result<Option<SourceAvPesHeader>, SourceAvTimestampError> {
        if let Some(error) = self.failure {
            return Err(error);
        }
        let result = if self.closed {
            Err(SourceAvTimestampError::Closed)
        } else {
            self.inspect(raw)
        };
        if let Err(error) = result {
            self.failure = Some(error);
        }
        result
    }

    /// Read historical evidence without silently completing a pending header.
    /// # Errors
    /// Returns the first sticky failure, without exposing partial failed evidence.
    pub fn report(&self) -> Result<SourceAvTimestampReport, SourceAvTimestampError> {
        self.failure.map_or(Ok(self.report), Err)
    }

    /// Close a finite observation. This is idempotent after successful closure;
    /// it requires complete headers, not complete AV PES payloads or EOF.
    /// # Errors
    /// Returns the first sticky failure or fences an incomplete selected header.
    pub fn finish(&mut self) -> Result<SourceAvTimestampReport, SourceAvTimestampError> {
        if let Some(error) = self.failure {
            return Err(error);
        }
        if self.report.pending_header {
            self.failure = Some(SourceAvTimestampError::IncompleteHeader);
            return Err(SourceAvTimestampError::IncompleteHeader);
        }
        self.closed = true;
        self.report()
    }

    fn inspect(
        &mut self,
        raw: &[u8; 188],
    ) -> Result<Option<SourceAvPesHeader>, SourceAvTimestampError> {
        let packet = valid_packet(raw).map_err(|_| SourceAvTimestampError::Framing)?;
        let ordinal = self
            .report
            .first_source_packet
            .checked_add(self.report.packets_seen)
            .ok_or(SourceAvTimestampError::Arithmetic)?;
        ordinal
            .checked_add(1)
            .ok_or(SourceAvTimestampError::Arithmetic)?;
        self.report.packets_seen = self
            .report
            .packets_seen
            .checked_add(1)
            .ok_or(SourceAvTimestampError::Arithmetic)?;
        if u16::from(packet.pid()) != self.report.stream.pid {
            return Ok(None);
        }
        let initial = std::mem::replace(&mut self.initial_discontinuity_allowed, false);
        if packet
            .adaptation_field()
            .is_some_and(|field| field.discontinuity_indicator())
            && !initial
        {
            return Err(SourceAvTimestampError::Discontinuity);
        }
        let counter = packet.continuity_counter().count();
        let Some(payload) = packet.payload() else {
            if packet.payload_unit_start_indicator() {
                return Err(SourceAvTimestampError::UnexpectedStart);
            }
            if self.counter.is_some_and(|before| counter != before) {
                return Err(SourceAvTimestampError::Continuity);
            }
            return Ok(None);
        };
        if let Some(before) = self.counter {
            if counter == before && self.previous.as_ref().is_some_and(|last| last == raw) {
                return Ok(None);
            }
            if counter != (before + 1) & 15 {
                return Err(SourceAvTimestampError::Continuity);
            }
        }
        self.counter = Some(counter);
        self.previous = Some(*raw);
        if packet.payload_unit_start_indicator() {
            if self.report.pending_header {
                return Err(SourceAvTimestampError::UnexpectedStart);
            }
            self.pusi = Some(ordinal);
            self.header_len = 0;
            self.started = true;
            self.report.pending_header = true;
        } else if !self.report.pending_header {
            self.report.unobserved_prefix |= !self.started;
            return Ok(None);
        }
        self.append_header(payload, ordinal)
    }

    fn append_header(
        &mut self,
        mut payload: &[u8],
        ordinal: u64,
    ) -> Result<Option<SourceAvPesHeader>, SourceAvTimestampError> {
        loop {
            let target = match self.header_len {
                0..=5 => 6,
                6..=8 => 9,
                _ => 9 + usize::from(self.header[8]),
            };
            let added = payload.len().min(target - self.header_len);
            self.header[self.header_len..self.header_len + added]
                .copy_from_slice(&payload[..added]);
            self.header_len += added;
            payload = &payload[added..];
            if self.header_len < target {
                return Ok(None);
            }
            if target == 6 {
                PesHeader::from_bytes(&self.header[..6])
                    .ok_or(SourceAvTimestampError::PesHeader)?;
            } else if target == 9 {
                self.check_extent()?;
            }
            if self.header_len >= 9 && self.header_len == 9 + usize::from(self.header[8]) {
                let header = self.parse_header(ordinal)?;
                self.record(header)?;
                self.pusi = None;
                self.report.pending_header = false;
                return Ok(Some(header));
            }
            if payload.is_empty() {
                return Ok(None);
            }
        }
    }

    fn check_extent(&self) -> Result<(), SourceAvTimestampError> {
        let declared = usize::from(u16::from_be_bytes([self.header[4], self.header[5]]));
        let length = 9 + usize::from(self.header[8]);
        if (declared == 0 && self.report.stream.kind != SourceAvStreamKind::Video)
            || (declared != 0 && length > declared + 6)
            || self.header[6] & 0xc0 != 0x80
            || self.header[6] & 0x30 != 0
        {
            return Err(SourceAvTimestampError::PesHeader);
        }
        let flags = self.header[7] >> 6;
        let minimum = match flags {
            0 => 0,
            2 => 5,
            3 => 10,
            _ => return Err(SourceAvTimestampError::Timestamp),
        };
        if usize::from(self.header[8]) < minimum {
            return Err(SourceAvTimestampError::Timestamp);
        }
        Ok(())
    }

    fn parse_header(&self, ordinal: u64) -> Result<SourceAvPesHeader, SourceAvTimestampError> {
        let bytes = &self.header[..self.header_len];
        let header = PesHeader::from_bytes(bytes).ok_or(SourceAvTimestampError::PesHeader)?;
        let PesContents::Parsed(Some(parsed)) = header.contents() else {
            return Err(SourceAvTimestampError::PesHeader);
        };
        let flags = bytes[7] >> 6;
        if (flags != 0 && bytes[9] >> 4 != if flags == 2 { 2 } else { 3 })
            || (flags == 3 && bytes[14] >> 4 != 1)
        {
            return Err(SourceAvTimestampError::Timestamp);
        }
        let (pts, dts) = match parsed.pts_dts() {
            Err(PesError::FieldNotPresent) => (None, None),
            Ok(PtsDts::PtsOnly(Ok(pts))) => (Some(pts.value()), None),
            Ok(PtsDts::Both {
                pts: Ok(pts),
                dts: Ok(dts),
            }) => (Some(pts.value()), Some(dts.value())),
            _ => return Err(SourceAvTimestampError::Timestamp),
        };
        Ok(SourceAvPesHeader {
            source_pusi_packet: self.pusi.ok_or(SourceAvTimestampError::PesHeader)?,
            header_completion_packet: ordinal,
            stream_id: bytes[3],
            pts,
            dts,
        })
    }

    fn record(&mut self, header: SourceAvPesHeader) -> Result<(), SourceAvTimestampError> {
        self.report.headers_seen = self
            .report
            .headers_seen
            .checked_add(1)
            .ok_or(SourceAvTimestampError::Arithmetic)?;
        if header.pts.is_none() {
            self.report.headers_without_timestamps = self
                .report
                .headers_without_timestamps
                .checked_add(1)
                .ok_or(SourceAvTimestampError::Arithmetic)?;
        }
        if let Some(raw_ticks_90khz) = header.pts {
            let sample = timestamp_sample(header, raw_ticks_90khz);
            self.report.first_pts.get_or_insert(sample);
            self.report.last_pts = Some(sample);
        }
        if let Some(raw_ticks_90khz) = header.dts {
            let sample = timestamp_sample(header, raw_ticks_90khz);
            self.report.first_dts.get_or_insert(sample);
            self.report.last_dts = Some(sample);
        }
        Ok(())
    }
}

fn timestamp_sample(header: SourceAvPesHeader, raw_ticks_90khz: u64) -> SourceAvTimestampSample {
    SourceAvTimestampSample {
        source_pusi_packet: header.source_pusi_packet,
        header_completion_packet: header.header_completion_packet,
        raw_ticks_90khz,
    }
}

#[cfg(test)]
#[path = "source_av_timestamps_tests.rs"]
mod tests;
