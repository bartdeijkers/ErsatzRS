//! Bounded epoch joining for one generated MPEG-TS programme.
//!
//! This transforms already muxed output, never source media or elementary
//! payloads. A host must own each producer, its CBR contract and cancellation.
//! Independent programme clocks, scrambled output, future PSI, PES CRC and embedded
//! programme-stream pack headers are rejected; they are not emitted by the generated final
//! mux contract. Codec/descriptor/elementary-stream inventories are preserved.

use std::collections::{BTreeMap, BTreeSet, VecDeque};

use mpeg2ts_reader::{
    mpegts_crc,
    packet::Packet,
    pes::{PesContents, PesHeader, PtsDts},
    psi::pmt::PmtSection,
};

/// The existing transport observation bound also bounds queued output packets.
pub const MAX_PENDING_PACKETS: usize =
    crate::teletext::source_observation::MAX_SOURCE_OBSERVATION_PACKETS;
const SIZE: usize = 188;
const MAX_SECTION: usize = 1024;
const MAX_PES_HEADER: usize = 264;
const CLOCK_MOD: i128 = (1_i128 << 33) * 300;
const PTS_MOD: i128 = 1_i128 << 33;
const CLOCK_FRACTION_BITS: u32 = 32;

/// A sticky failure prevents uncommitted packets from being drained.
#[derive(Debug, Clone, Copy, PartialEq, Eq, thiserror::Error)]
pub enum MpegTsEpochError {
    /// Producer epochs must begin and finish in order with a nonzero CBR rate.
    #[error("MPEG-TS epoch lifecycle or bitrate is invalid")]
    Lifecycle,
    /// Packet, adaptation, input continuity or protection is invalid.
    #[error("MPEG-TS epoch transport framing or continuity is invalid")]
    Transport,
    /// PSI framing, CRC, descriptors, sections or current-table identity failed.
    #[error("MPEG-TS epoch PSI is invalid")]
    Psi,
    /// This generated-output contract requires exactly one programme/PCR domain.
    #[error("MPEG-TS epoch requires one generated programme clock")]
    Programme,
    /// A PES header, timestamp marker or declared extent is invalid.
    #[error("MPEG-TS epoch PES header or timestamp is invalid")]
    Pes,
    /// Optional fields outside the generated mux contract cannot be rewritten.
    #[error("MPEG-TS epoch contains unsupported PES CRC or pack header fields")]
    UnsupportedPes,
    /// A nonempty producer ended before complete clock, PSI or pending headers.
    #[error("MPEG-TS epoch ended with incomplete clock, PSI or headers")]
    Incomplete,
    /// Packet storage or arithmetic exceeded its explicit bound.
    #[error("MPEG-TS epoch exceeded its bounded transport window")]
    Capacity,
}

type Result<T> = std::result::Result<T, MpegTsEpochError>;

#[derive(Debug)]
struct QueuedPacket {
    bytes: [u8; SIZE],
    payload_start: usize,
    duplicates: Vec<u64>,
}

#[derive(Debug, Default)]
struct InputPid {
    counter: Option<u8>,
    previous: Option<[u8; SIZE]>,
    previous_payload_start: usize,
    previous_ordinal: u64,
    rewritten: Option<[u8; SIZE]>,
    seen: bool,
}

#[derive(Debug)]
struct Header {
    bytes: Vec<u8>,
    positions: Vec<u64>,
    start: u64,
    kind: HeaderKind,
}

#[derive(Debug, Clone, Copy)]
enum HeaderKind {
    Psi,
    Pes { required: bool },
}

impl Header {
    fn new(start: u64, kind: HeaderKind) -> Self {
        Self {
            bytes: Vec::new(),
            positions: Vec::new(),
            start,
            kind,
        }
    }
}

#[derive(Debug)]
struct TablePart {
    bytes: Vec<u8>,
    positions: Vec<u64>,
    start: u64,
}

#[derive(Debug)]
struct TableInput {
    identity: (u16, u8, u8),
    parts: BTreeMap<u8, Vec<TablePart>>,
}

#[derive(Debug, Clone)]
struct TableOutput {
    pid: u16,
    sections: Vec<Vec<u8>>,
    version: u8,
}

#[derive(Debug, Clone, Copy)]
enum ClockField {
    Timestamp,
    Pcr,
    Escr,
}

#[derive(Debug)]
struct ClockPatch {
    positions: Vec<u64>,
    bytes: Vec<u8>,
    ticks: i128,
    kind: ClockField,
}

/// One HTTP response's clock, continuity and PSI versions, with bounded queues.
///
/// Drain ready packets after every push. No packet passes an unresolved split
/// header/section. Errors are sticky and discard access to uncommitted bytes.
#[derive(Debug, Default)]
pub struct MpegTsEpochRewriter {
    failure: Option<MpegTsEpochError>,
    active: bool,
    rate: u64,
    epoch_packets: usize,
    next_ordinal: u64,
    front_ordinal: u64,
    wire_clock: u128,
    last_pcr: Option<(i128, u128)>,
    input_pcr: Option<(i128, i128)>,
    offset: Option<i128>,
    queue: VecDeque<QueuedPacket>,
    holds: BTreeMap<u64, usize>,
    inputs: BTreeMap<u16, InputPid>,
    counters: BTreeMap<u16, u8>,
    headers: BTreeMap<u16, Header>,
    patches: Vec<ClockPatch>,
    tables: BTreeMap<u16, TableInput>,
    versions: BTreeMap<u16, u8>,
    pat: Option<TableOutput>,
    pmt: Option<TableOutput>,
    program: Option<(u16, u16)>,
    pcr_pid: Option<u16>,
    observed_pcr_pid: Option<u16>,
    streams: BTreeMap<u16, bool>,
    pat_ready: bool,
    pmt_ready: bool,
}

impl MpegTsEpochRewriter {
    /// Begin an independently started producer under its declared CBR rate.
    /// # Errors
    /// Rejects overlap, undrained output, zero rate or an earlier sticky failure.
    pub fn begin_epoch(&mut self, muxrate_bps: u64) -> Result<()> {
        self.checked(|this| {
            if this.active || !this.queue.is_empty() || muxrate_bps == 0 {
                return Err(MpegTsEpochError::Lifecycle);
            }
            this.active = true;
            this.rate = muxrate_bps;
            this.epoch_packets = 0;
            this.input_pcr = None;
            this.offset = None;
            this.inputs.clear();
            this.headers.clear();
            this.patches.clear();
            this.tables.clear();
            this.program = None;
            this.pcr_pid = None;
            this.observed_pcr_pid = None;
            this.streams.clear();
            this.pat_ready = false;
            this.pmt_ready = false;
            Ok(())
        })
    }

    /// Supply one physical TS188 packet in order, including null packets.
    /// # Errors
    /// Validates transport, complete PSI/PES headers and all queue bounds.
    pub fn push_packet(&mut self, packet: [u8; SIZE]) -> Result<()> {
        self.checked(|this| this.push(packet))
    }

    /// Drain one packet whose complete dependent headers have been validated.
    #[must_use]
    pub fn pop_ready_packet(&mut self) -> Option<[u8; SIZE]> {
        if self.failure.is_some()
            || self.offset.is_none()
            || !self.pat_ready
            || !self.pmt_ready
            || self
                .holds
                .first_key_value()
                .is_some_and(|(first, _)| *first <= self.front_ordinal)
        {
            return None;
        }
        let packet = self.queue.pop_front()?;
        let pid = pid(&packet.bytes);
        if let Some(input) = self.inputs.get_mut(&pid) {
            if input.previous_ordinal == self.front_ordinal {
                input.rewritten = Some(packet.bytes);
            }
        }
        self.front_ordinal += 1;
        Some(packet.bytes)
    }

    /// Close one producer at a real EOF before beginning its successor.
    /// Empty failed-start epochs may finish cleanly without changing the clock.
    /// # Errors
    /// A nonempty epoch requires complete current PSI, PCR and split headers.
    pub fn finish_epoch(&mut self) -> Result<()> {
        self.checked(|this| {
            if !this.active {
                return Err(MpegTsEpochError::Lifecycle);
            }
            if this.epoch_packets != 0
                && (this.offset.is_none()
                    || !this.pat_ready
                    || !this.pmt_ready
                    || !this.headers.is_empty()
                    || !this.holds.is_empty()
                    || !this.patches.is_empty())
            {
                return Err(MpegTsEpochError::Incomplete);
            }
            this.active = false;
            Ok(())
        })
    }

    fn checked<T>(&mut self, operation: impl FnOnce(&mut Self) -> Result<T>) -> Result<T> {
        if let Some(failure) = self.failure {
            return Err(failure);
        }
        let result = operation(self);
        if let Err(failure) = result {
            self.failure = Some(failure);
        }
        result
    }

    fn push(&mut self, mut raw: [u8; SIZE]) -> Result<()> {
        if !self.active {
            return Err(MpegTsEpochError::Lifecycle);
        }
        if self.queue.len() == MAX_PENDING_PACKETS {
            return Err(MpegTsEpochError::Capacity);
        }
        let (packet_pid, payload_start, start) = framing(&raw)?;
        let ordinal = self.next_ordinal;
        self.next_ordinal = ordinal
            .checked_add(1)
            .filter(|n| *n <= u64::MAX / SIZE as u64)
            .ok_or(MpegTsEpochError::Capacity)?;
        self.epoch_packets = self
            .epoch_packets
            .checked_add(1)
            .ok_or(MpegTsEpochError::Capacity)?;
        let position = self.wire_clock;
        let step = ((SIZE as u128 * 8 * 27_000_000) << CLOCK_FRACTION_BITS) / u128::from(self.rate);
        self.wire_clock = self
            .wire_clock
            .checked_add(step)
            .ok_or(MpegTsEpochError::Capacity)?;
        let (duplicate, original) = self.continuity(packet_pid, payload_start, &raw, ordinal)?;
        let original_raw = raw;
        if packet_pid != 0x1fff {
            let previous = self.counters.get(&packet_pid).copied();
            let current = if payload_start < SIZE && !duplicate {
                let next = previous.map_or(raw[3] & 15, |counter| (counter + 1) & 15);
                self.counters.insert(packet_pid, next);
                next
            } else {
                previous.unwrap_or(raw[3] & 15)
            };
            raw[3] = (raw[3] & 0xf0) | current;
        }
        if raw[3] & 0x20 != 0 && raw[4] > 0 {
            raw[5] &= !0x80;
        }
        self.queue.push_back(QueuedPacket {
            bytes: raw,
            payload_start,
            duplicates: Vec::new(),
        });
        if duplicate {
            self.copy_duplicate(original, ordinal, packet_pid, payload_start)?;
        } else if payload_start < SIZE && packet_pid != 0x1fff {
            let input = self
                .inputs
                .get_mut(&packet_pid)
                .ok_or(MpegTsEpochError::Transport)?;
            input.counter = Some(original_raw[3] & 15);
            input.previous = Some(original_raw);
            input.previous_payload_start = payload_start;
            input.previous_ordinal = ordinal;
            input.rewritten = None;
        }
        self.adaptation(&original_raw, ordinal, position)?;
        if payload_start == SIZE || duplicate {
            return Ok(());
        }
        if packet_pid == 0 || self.program.is_some_and(|(_, pmt)| pmt == packet_pid) {
            self.psi_payload(
                packet_pid,
                ordinal,
                payload_start,
                &original_raw[payload_start..],
                start,
            )?;
        } else if let Some(required) = self.streams.get(&packet_pid).copied() {
            self.pes_payload(
                packet_pid,
                ordinal,
                payload_start,
                &original_raw[payload_start..],
                start,
                required,
            )?;
        } else if packet_pid >= 0x20 && packet_pid != 0x1fff {
            // Generated muxes announce every elementary PID before its data.
            // Do not emit an unknown PES without translating its timestamps.
            return Err(MpegTsEpochError::Programme);
        }
        Ok(())
    }

    fn continuity(
        &mut self,
        packet_pid: u16,
        payload_start: usize,
        raw: &[u8; SIZE],
        ordinal: u64,
    ) -> Result<(bool, u64)> {
        if packet_pid == 0x1fff {
            return Ok((false, ordinal));
        }
        let input = self.inputs.entry(packet_pid).or_default();
        let discontinuity = raw[3] & 0x20 != 0 && raw[4] > 0 && raw[5] & 0x80 != 0;
        let previously_seen = input.seen;
        input.seen = true;
        let counter = raw[3] & 15;
        if let Some(previous) = input.counter {
            if payload_start == SIZE {
                if counter != previous {
                    return Err(MpegTsEpochError::Transport);
                }
            } else if counter == previous {
                if input.previous.as_ref().is_none_or(|last| {
                    last[1..3] != raw[1..3]
                        || last[input.previous_payload_start..] != raw[payload_start..]
                        || (discontinuity
                            && !(last[3] & 0x20 != 0 && last[4] > 0 && last[5] & 0x80 != 0))
                }) {
                    return Err(MpegTsEpochError::Transport);
                }
                return Ok((true, input.previous_ordinal));
            } else if counter != (previous + 1) & 15 {
                return Err(MpegTsEpochError::Transport);
            }
        }
        if discontinuity && previously_seen {
            return Err(MpegTsEpochError::Transport);
        }
        Ok((false, ordinal))
    }

    fn copy_duplicate(
        &mut self,
        original: u64,
        duplicate: u64,
        packet_pid: u16,
        start: usize,
    ) -> Result<()> {
        let previous = if original >= self.front_ordinal {
            let index = usize::try_from(original - self.front_ordinal)
                .map_err(|_| MpegTsEpochError::Capacity)?;
            let original = self
                .queue
                .get_mut(index)
                .ok_or(MpegTsEpochError::Transport)?;
            original.duplicates.push(duplicate);
            original.bytes
        } else {
            self.inputs[&packet_pid]
                .rewritten
                .ok_or(MpegTsEpochError::Transport)?
        };
        self.queue
            .back_mut()
            .ok_or(MpegTsEpochError::Transport)?
            .bytes[start..]
            .copy_from_slice(&previous[start..]);
        Ok(())
    }

    fn adaptation(&mut self, raw: &[u8; SIZE], ordinal: u64, position: u128) -> Result<()> {
        if raw[3] & 0x20 == 0 || raw[4] == 0 {
            return Ok(());
        }
        let end = 5 + usize::from(raw[4]);
        let flags = raw[5];
        let mut at = 6;
        for (flag, anchor) in [(0x10, true), (0x08, false)] {
            if flags & flag != 0 {
                let bytes = raw
                    .get(at..at + 6)
                    .filter(|_| at + 6 <= end)
                    .ok_or(MpegTsEpochError::Transport)?;
                let value = read_pcr(bytes)?;
                if anchor {
                    self.anchor(pid(raw), value, position)?;
                }
                self.clock_patch(ClockPatch {
                    positions: positions(ordinal, at, 6),
                    bytes: bytes.to_vec(),
                    ticks: value,
                    kind: ClockField::Pcr,
                })?;
                at += 6;
            }
        }
        if flags & 4 != 0 {
            at += 1;
        }
        if flags & 2 != 0 {
            let length = usize::from(
                *raw.get(at)
                    .filter(|_| at < end)
                    .ok_or(MpegTsEpochError::Transport)?,
            );
            at += 1 + length;
        }
        if flags & 1 != 0 {
            let length = usize::from(
                *raw.get(at)
                    .filter(|_| at < end)
                    .ok_or(MpegTsEpochError::Transport)?,
            );
            let extension_end = at + 1 + length;
            if extension_end > end {
                return Err(MpegTsEpochError::Transport);
            }
            at += 1;
            if length != 0 {
                let extension_flags = raw[at];
                at += 1;
                if extension_flags & 0x80 != 0 {
                    at += 2;
                }
                if extension_flags & 0x40 != 0 {
                    at += 3;
                }
                if extension_flags & 0x20 != 0 {
                    let bytes = raw
                        .get(at..at + 5)
                        .filter(|_| at + 5 <= extension_end)
                        .ok_or(MpegTsEpochError::Transport)?;
                    let ticks = read_timestamp(bytes, None)?;
                    self.clock_patch(ClockPatch {
                        positions: positions(ordinal, at, 5),
                        bytes: bytes.to_vec(),
                        ticks,
                        kind: ClockField::Timestamp,
                    })?;
                    at += 5;
                }
                if at > extension_end {
                    return Err(MpegTsEpochError::Transport);
                }
            }
        }
        if at > end {
            return Err(MpegTsEpochError::Transport);
        }
        Ok(())
    }

    fn anchor(&mut self, packet_pid: u16, raw: i128, position: u128) -> Result<()> {
        if self
            .observed_pcr_pid
            .is_some_and(|before| before != packet_pid)
            || self.pcr_pid.is_some_and(|expected| expected != packet_pid)
        {
            return Err(MpegTsEpochError::Programme);
        }
        self.observed_pcr_pid = Some(packet_pid);
        let unwrapped = if let Some((before_raw, before_unwrapped)) = self.input_pcr {
            let delta = (raw - before_raw).rem_euclid(CLOCK_MOD);
            if delta >= CLOCK_MOD / 2 {
                return Err(MpegTsEpochError::Transport);
            }
            before_unwrapped
                .checked_add(delta)
                .ok_or(MpegTsEpochError::Capacity)?
        } else {
            let target = if let Some((previous, before_position)) = self.last_pcr {
                let elapsed = i128::try_from((position - before_position) >> CLOCK_FRACTION_BITS)
                    .map_err(|_| MpegTsEpochError::Capacity)?;
                previous
                    .checked_add(elapsed)
                    .ok_or(MpegTsEpochError::Capacity)?
            } else {
                raw
            };
            // Quantize the shared offset, not each timestamp, to preserve exact
            // PTS-DTS and PCR extension relations. Error is less than one 90 kHz tick.
            let offset = (target - raw + 299).div_euclid(300);
            self.offset = Some(offset);
            for patch in std::mem::take(&mut self.patches) {
                self.apply_clock_patch(patch, offset)?;
            }
            raw
        };
        self.input_pcr = Some((raw, unwrapped));
        let output = unwrapped
            .checked_add(self.offset.ok_or(MpegTsEpochError::Incomplete)? * 300)
            .ok_or(MpegTsEpochError::Capacity)?;
        self.last_pcr = Some((output, position));
        Ok(())
    }

    fn clock_patch(&mut self, patch: ClockPatch) -> Result<()> {
        if let Some(offset) = self.offset {
            self.apply_clock_patch(patch, offset)
        } else {
            self.patches.push(patch);
            Ok(())
        }
    }

    fn apply_clock_patch(&mut self, mut patch: ClockPatch, offset: i128) -> Result<()> {
        match patch.kind {
            ClockField::Timestamp => {
                write_timestamp(&mut patch.bytes, (patch.ticks + offset).rem_euclid(PTS_MOD));
            }
            ClockField::Pcr => write_pcr(
                &mut patch.bytes,
                (patch.ticks + offset * 300).rem_euclid(CLOCK_MOD),
            ),
            ClockField::Escr => write_escr(
                &mut patch.bytes,
                (patch.ticks + offset * 300).rem_euclid(CLOCK_MOD),
            ),
        }
        self.patch(&patch.positions, &patch.bytes)
    }

    fn patch(&mut self, positions: &[u64], bytes: &[u8]) -> Result<()> {
        for (&position, &value) in positions.iter().zip(bytes) {
            let ordinal = position / SIZE as u64;
            let offset =
                usize::try_from(position % SIZE as u64).map_err(|_| MpegTsEpochError::Capacity)?;
            let index = usize::try_from(
                ordinal
                    .checked_sub(self.front_ordinal)
                    .ok_or(MpegTsEpochError::Transport)?,
            )
            .map_err(|_| MpegTsEpochError::Capacity)?;
            let packet = self
                .queue
                .get_mut(index)
                .ok_or(MpegTsEpochError::Transport)?;
            packet.bytes[offset] = value;
            let duplicates = if offset >= packet.payload_start {
                packet.duplicates.clone()
            } else {
                Vec::new()
            };
            for duplicate in duplicates {
                let index = usize::try_from(duplicate - self.front_ordinal)
                    .map_err(|_| MpegTsEpochError::Capacity)?;
                self.queue
                    .get_mut(index)
                    .ok_or(MpegTsEpochError::Transport)?
                    .bytes[offset] = value;
            }
        }
        Ok(())
    }

    fn hold(&mut self, ordinal: u64) {
        *self.holds.entry(ordinal).or_default() += 1;
    }
    fn release(&mut self, ordinal: u64) -> Result<()> {
        let count = self
            .holds
            .get_mut(&ordinal)
            .ok_or(MpegTsEpochError::Transport)?;
        *count -= 1;
        if *count == 0 {
            self.holds.remove(&ordinal);
        }
        Ok(())
    }

    fn pes_payload(
        &mut self,
        packet_pid: u16,
        ordinal: u64,
        offset: usize,
        bytes: &[u8],
        start: bool,
        required: bool,
    ) -> Result<()> {
        if start {
            if self.headers.contains_key(&packet_pid) {
                return Err(MpegTsEpochError::Pes);
            }
            self.hold(ordinal);
            self.headers.insert(
                packet_pid,
                Header::new(ordinal, HeaderKind::Pes { required }),
            );
        }
        let Some(mut header) = self.headers.remove(&packet_pid) else {
            return Ok(());
        };
        for (index, &byte) in bytes.iter().enumerate() {
            header.bytes.push(byte);
            header
                .positions
                .push(ordinal * SIZE as u64 + (offset + index) as u64);
            let length = header.bytes.len();
            if length == 3 && header.bytes != [0, 0, 1] {
                if matches!(header.kind, HeaderKind::Pes { required: true }) {
                    return Err(MpegTsEpochError::Pes);
                }
                return self.release(header.start);
            }
            if length == 6 {
                let parsed = PesHeader::from_bytes(&header.bytes).ok_or(MpegTsEpochError::Pes)?;
                if matches!(parsed.contents(), PesContents::Payload(_)) {
                    return self.release(header.start);
                }
            }
            if length >= 9 && length == 9 + usize::from(header.bytes[8]) {
                self.complete_pes(&header)?;
                return self.release(header.start);
            }
            if length >= MAX_PES_HEADER {
                return Err(MpegTsEpochError::Pes);
            }
        }
        self.headers.insert(packet_pid, header);
        Ok(())
    }

    fn complete_pes(&mut self, header: &Header) -> Result<()> {
        let bytes = &header.bytes;
        let declared = usize::from(u16::from_be_bytes([bytes[4], bytes[5]]));
        if bytes[6] & 0xc0 != 0x80
            || bytes[6] & 0x30 != 0
            || (declared != 0 && declared + 6 < bytes.len())
        {
            return Err(MpegTsEpochError::Pes);
        }
        if bytes[7] & 2 != 0 {
            return Err(MpegTsEpochError::UnsupportedPes);
        }
        let parsed = PesHeader::from_bytes(bytes).ok_or(MpegTsEpochError::Pes)?;
        let PesContents::Parsed(Some(parsed)) = parsed.contents() else {
            return Err(MpegTsEpochError::Pes);
        };
        match parsed.pts_dts() {
            Ok(
                PtsDts::PtsOnly(Ok(_))
                | PtsDts::Both {
                    pts: Ok(_),
                    dts: Ok(_),
                },
            ) => {}
            Err(mpeg2ts_reader::pes::PesError::FieldNotPresent) if bytes[7] >> 6 == 0 => {}
            _ => return Err(MpegTsEpochError::Pes),
        }
        let flags = bytes[7] >> 6;
        let mut at = 9;
        if flags == 1 {
            return Err(MpegTsEpochError::Pes);
        }
        if flags >= 2 {
            self.pes_timestamp(header, at, if flags == 2 { 2 } else { 3 })?;
            at += 5;
        }
        if flags == 3 {
            self.pes_timestamp(header, at, 1)?;
            at += 5;
        }
        if bytes[7] & 0x20 != 0 {
            let value = bytes.get(at..at + 6).ok_or(MpegTsEpochError::Pes)?;
            self.clock_patch(ClockPatch {
                positions: header.positions[at..at + 6].to_vec(),
                bytes: value.to_vec(),
                ticks: read_escr(value)?,
                kind: ClockField::Escr,
            })?;
            at += 6;
        }
        if bytes[7] & 0x10 != 0 {
            at += 3;
        }
        if bytes[7] & 8 != 0 {
            at += 1;
        }
        if bytes[7] & 4 != 0 {
            at += 1;
        }
        if bytes[7] & 1 != 0 {
            at = self.pes_extension(header, at)?;
        }
        if at > bytes.len() {
            return Err(MpegTsEpochError::Pes);
        }
        Ok(())
    }

    fn pes_extension(&mut self, header: &Header, mut at: usize) -> Result<usize> {
        let bytes = &header.bytes;
        let flags = *bytes.get(at).ok_or(MpegTsEpochError::Pes)?;
        at += 1;
        if flags & 0x80 != 0 {
            at += 16; // Private data is opaque, not a systems clock.
        }
        if flags & 0x40 != 0 {
            // A programme-stream pack contains another SCR domain; the final
            // TS mux does not emit embedded packs. Never retain a stale clock.
            return Err(MpegTsEpochError::UnsupportedPes);
        }
        if flags & 0x20 != 0 {
            let sequence = bytes.get(at..at + 2).ok_or(MpegTsEpochError::Pes)?;
            if sequence[0] & 0x80 == 0 || sequence[1] & 0x80 == 0 {
                return Err(MpegTsEpochError::Pes);
            }
            at += 2;
        }
        if flags & 0x10 != 0 {
            let buffer = bytes.get(at..at + 2).ok_or(MpegTsEpochError::Pes)?;
            if buffer[0] & 0xc0 != 0x40 {
                return Err(MpegTsEpochError::Pes);
            }
            at += 2;
        }
        if flags & 1 != 0 {
            let length = *bytes.get(at).ok_or(MpegTsEpochError::Pes)?;
            if length <= 0x80 {
                return Err(MpegTsEpochError::Pes);
            }
            at += 1;
            let end = at + usize::from(length & 0x7f);
            let extension = bytes.get(at..end).ok_or(MpegTsEpochError::Pes)?;
            if extension[0] & 0x81 == 0x80 {
                // stream_id_extension_flag=1, tref_extension_flag=0.
                let value = extension.get(1..6).ok_or(MpegTsEpochError::Pes)?;
                self.clock_patch(ClockPatch {
                    positions: header.positions[at + 1..at + 6].to_vec(),
                    bytes: value.to_vec(),
                    ticks: read_timestamp(value, None)?,
                    kind: ClockField::Timestamp,
                })?;
            }
            at = end;
        }
        Ok(at)
    }

    fn pes_timestamp(&mut self, header: &Header, at: usize, prefix: u8) -> Result<()> {
        let bytes = header.bytes.get(at..at + 5).ok_or(MpegTsEpochError::Pes)?;
        self.clock_patch(ClockPatch {
            positions: header.positions[at..at + 5].to_vec(),
            bytes: bytes.to_vec(),
            ticks: read_timestamp(bytes, Some(prefix))?,
            kind: ClockField::Timestamp,
        })
    }

    fn psi_payload(
        &mut self,
        packet_pid: u16,
        ordinal: u64,
        offset: usize,
        bytes: &[u8],
        start: bool,
    ) -> Result<()> {
        if start {
            let pointer = usize::from(*bytes.first().ok_or(MpegTsEpochError::Psi)?);
            if pointer + 1 > bytes.len() {
                return Err(MpegTsEpochError::Psi);
            }
            self.section_bytes(packet_pid, ordinal, offset + 1, &bytes[1..=pointer], false)?;
            if self.headers.contains_key(&packet_pid) {
                return Err(MpegTsEpochError::Psi);
            }
            self.section_bytes(
                packet_pid,
                ordinal,
                offset + 1 + pointer,
                &bytes[1 + pointer..],
                true,
            )
        } else {
            self.section_bytes(packet_pid, ordinal, offset, bytes, false)
        }
    }

    fn section_bytes(
        &mut self,
        packet_pid: u16,
        ordinal: u64,
        offset: usize,
        bytes: &[u8],
        allow_new: bool,
    ) -> Result<()> {
        let mut header = self.headers.remove(&packet_pid);
        for (index, &byte) in bytes.iter().enumerate() {
            if header.is_none() {
                if byte == 0xff {
                    if bytes[index..].iter().any(|value| *value != 0xff) {
                        return Err(MpegTsEpochError::Psi);
                    }
                    return Ok(());
                }
                if !allow_new {
                    return Err(MpegTsEpochError::Psi);
                }
                self.hold(ordinal);
                header = Some(Header::new(ordinal, HeaderKind::Psi));
            }
            let part = header.as_mut().ok_or(MpegTsEpochError::Psi)?;
            part.bytes.push(byte);
            part.positions
                .push(ordinal * SIZE as u64 + (offset + index) as u64);
            if part.bytes.len() >= 3 {
                let size = 3 + (usize::from(part.bytes[1] & 15) << 8) + usize::from(part.bytes[2]);
                if !(12..=MAX_SECTION).contains(&size) {
                    return Err(MpegTsEpochError::Psi);
                }
                if part.bytes.len() == size {
                    self.section(packet_pid, header.take().ok_or(MpegTsEpochError::Psi)?)?;
                }
            }
        }
        if let Some(header) = header {
            self.headers.insert(packet_pid, header);
        }
        Ok(())
    }

    fn section(&mut self, packet_pid: u16, header: Header) -> Result<()> {
        let bytes = &header.bytes;
        if bytes[1] & 0xf0 != 0xb0
            || bytes[5] & 0xc1 != 0xc1
            || bytes[6] > bytes[7]
            || mpegts_crc::sum32(bytes) != 0
            || bytes[0] != if packet_pid == 0 { 0 } else { 2 }
        {
            return Err(MpegTsEpochError::Psi);
        }
        if packet_pid != 0 && (bytes[6] != 0 || bytes[7] != 0) {
            return Err(MpegTsEpochError::Psi);
        }
        let identity = (
            u16::from_be_bytes([bytes[3], bytes[4]]),
            (bytes[5] >> 1) & 31,
            bytes[7],
        );
        let table = self.tables.entry(packet_pid).or_insert_with(|| TableInput {
            identity,
            parts: BTreeMap::new(),
        });
        if table.identity != identity {
            return Err(MpegTsEpochError::Psi);
        }
        let section_number = bytes[6];
        let copies = table.parts.entry(section_number).or_default();
        if copies.first().is_some_and(|before| before.bytes != *bytes) {
            return Err(MpegTsEpochError::Psi);
        }
        copies.push(TablePart {
            bytes: header.bytes,
            positions: header.positions,
            start: header.start,
        });
        if table.parts.len() != usize::from(identity.2) + 1 {
            return Ok(());
        }
        let table = self
            .tables
            .remove(&packet_pid)
            .ok_or(MpegTsEpochError::Psi)?;
        let sections = table
            .parts
            .values()
            .map(|copies| canonical_section(&copies[0].bytes))
            .collect::<Vec<_>>();
        if packet_pid == 0 {
            self.install_pat(&table)?;
        } else {
            self.install_pmt(packet_pid, &table)?;
        }
        let previous = if packet_pid == 0 {
            self.pat.as_ref()
        } else {
            self.pmt.as_ref()
        };
        let version = if let Some(previous) =
            previous.filter(|previous| previous.pid == packet_pid && previous.sections == sections)
        {
            previous.version
        } else {
            self.versions
                .get(&packet_pid)
                .map_or(identity.1, |version| (version + 1) & 31)
        };
        self.versions.insert(packet_pid, version);
        let record = TableOutput {
            pid: packet_pid,
            sections,
            version,
        };
        if packet_pid == 0 {
            self.pat = Some(record);
        } else {
            self.pmt = Some(record);
        }
        for copies in table.parts.into_values() {
            for mut part in copies {
                part.bytes[5] = (part.bytes[5] & 0xc1) | (version << 1);
                let end = part.bytes.len() - 4;
                let crc = mpegts_crc::sum32(&part.bytes[..end]);
                part.bytes[end..].copy_from_slice(&crc.to_be_bytes());
                self.patch(&part.positions, &part.bytes)?;
                self.release(part.start)?;
            }
        }
        Ok(())
    }

    fn install_pat(&mut self, table: &TableInput) -> Result<()> {
        let mut programs = BTreeMap::new();
        let mut pids = BTreeSet::new();
        for copies in table.parts.values() {
            let section = &copies[0].bytes;
            let entries = &section[8..section.len() - 4];
            if !entries.len().is_multiple_of(4) {
                return Err(MpegTsEpochError::Psi);
            }
            for entry in entries.chunks_exact(4) {
                if entry[2] & 0xe0 != 0xe0 {
                    return Err(MpegTsEpochError::Psi);
                }
                let program = u16::from_be_bytes([entry[0], entry[1]]);
                let pid = (u16::from(entry[2] & 31) << 8) | u16::from(entry[3]);
                if pid == 0 || pid == 1 || pid == 0x1fff || !pids.insert(pid) {
                    return Err(MpegTsEpochError::Psi);
                }
                if program != 0 && (pid < 0x20 || programs.insert(program, pid).is_some()) {
                    return Err(MpegTsEpochError::Psi);
                }
            }
        }
        if programs.len() != 1 {
            return Err(MpegTsEpochError::Programme);
        }
        let (&program, &pmt) = programs
            .first_key_value()
            .ok_or(MpegTsEpochError::Programme)?;
        if self.program.is_some_and(|before| before != (program, pmt)) {
            if self
                .headers
                .values()
                .any(|header| matches!(header.kind, HeaderKind::Pes { .. }))
                || self.tables.keys().any(|pid| *pid != 0)
            {
                return Err(MpegTsEpochError::Incomplete);
            }
            self.pmt_ready = false;
            self.pcr_pid = None;
            self.streams.clear();
        }
        self.program = Some((program, pmt));
        self.pat_ready = true;
        Ok(())
    }

    fn install_pmt(&mut self, packet_pid: u16, table: &TableInput) -> Result<()> {
        if self.program != Some((table.identity.0, packet_pid)) {
            return Err(MpegTsEpochError::Programme);
        }
        let bytes = &table.parts[&0][0].bytes;
        let body = &bytes[8..bytes.len() - 4];
        let pmt = PmtSection::from_bytes(body).map_err(|_| MpegTsEpochError::Psi)?;
        if body[0] & 0xe0 != 0xe0 || body[2] & 0xf0 != 0xf0 {
            return Err(MpegTsEpochError::Psi);
        }
        let pcr = u16::from(pmt.pcr_pid());
        if !(0x20..0x1fff).contains(&pcr)
            || pcr == packet_pid
            || self.observed_pcr_pid.is_some_and(|before| before != pcr)
        {
            return Err(MpegTsEpochError::Programme);
        }
        let descriptor_size = (usize::from(body[2] & 15) << 8) | usize::from(body[3]);
        validate_descriptors(
            body.get(4..4 + descriptor_size)
                .ok_or(MpegTsEpochError::Psi)?,
        )?;
        let mut at = 4 + descriptor_size;
        let mut streams = BTreeMap::new();
        for stream in pmt.streams() {
            let stream_pid = u16::from(stream.elementary_pid());
            if !(0x20..0x1fff).contains(&stream_pid)
                || stream_pid == packet_pid
                || body[at + 1] & 0xe0 != 0xe0
                || body[at + 3] & 0xf0 != 0xf0
            {
                return Err(MpegTsEpochError::Psi);
            }
            let length = (usize::from(body[at + 3] & 15) << 8) | usize::from(body[at + 4]);
            validate_descriptors(
                body.get(at + 5..at + 5 + length)
                    .ok_or(MpegTsEpochError::Psi)?,
            )?;
            if streams
                .insert(stream_pid, stream.stream_type().is_pes())
                .is_some()
            {
                return Err(MpegTsEpochError::Psi);
            }
            at += 5 + length;
        }
        if at != body.len() || streams.is_empty() {
            return Err(MpegTsEpochError::Psi);
        }
        self.streams = streams;
        self.pcr_pid = Some(pcr);
        self.pmt_ready = true;
        Ok(())
    }
}

fn framing(raw: &[u8; SIZE]) -> Result<(u16, usize, bool)> {
    let packet = Packet::try_new(raw).ok_or(MpegTsEpochError::Transport)?;
    let adaptation = raw[3] & 0x20 != 0;
    let payload = raw[3] & 0x10 != 0;
    if packet.transport_error_indicator()
        || packet.transport_scrambling_control().is_scrambled()
        || (!adaptation && !payload)
    {
        return Err(MpegTsEpochError::Transport);
    }
    let start = if adaptation {
        5 + usize::from(raw[4])
    } else {
        4
    };
    if start > SIZE || (!payload && start != SIZE) || (payload && start == SIZE) {
        return Err(MpegTsEpochError::Transport);
    }
    let offset = if payload { start } else { SIZE };
    if !payload && packet.payload_unit_start_indicator() {
        return Err(MpegTsEpochError::Transport);
    }
    Ok((
        u16::from(packet.pid()),
        offset,
        packet.payload_unit_start_indicator(),
    ))
}

fn pid(raw: &[u8; SIZE]) -> u16 {
    (u16::from(raw[1] & 31) << 8) | u16::from(raw[2])
}
fn positions(ordinal: u64, at: usize, count: usize) -> Vec<u64> {
    (at..at + count)
        .map(|offset| ordinal * SIZE as u64 + offset as u64)
        .collect()
}
fn canonical_section(bytes: &[u8]) -> Vec<u8> {
    let mut result = bytes[..bytes.len() - 4].to_vec();
    result[5] &= 0xc1;
    result
}
fn validate_descriptors(mut bytes: &[u8]) -> Result<()> {
    while !bytes.is_empty() {
        let length = usize::from(*bytes.get(1).ok_or(MpegTsEpochError::Psi)?) + 2;
        bytes = bytes.get(length..).ok_or(MpegTsEpochError::Psi)?;
    }
    Ok(())
}

fn read_timestamp(bytes: &[u8], prefix: Option<u8>) -> Result<i128> {
    if bytes.len() != 5
        || prefix.is_some_and(|prefix| bytes[0] >> 4 != prefix)
        || bytes[0] & 1 == 0
        || bytes[2] & 1 == 0
        || bytes[4] & 1 == 0
    {
        return Err(MpegTsEpochError::Pes);
    }
    Ok((i128::from(bytes[0] & 0x0e) << 29)
        | (i128::from(bytes[1]) << 22)
        | (i128::from(bytes[2] >> 1) << 15)
        | (i128::from(bytes[3]) << 7)
        | i128::from(bytes[4] >> 1))
}
fn byte(value: i128) -> u8 {
    u8::try_from(value & 255).expect("masked byte")
}
fn write_timestamp(bytes: &mut [u8], ticks: i128) {
    bytes[0] = (bytes[0] & 0xf0) | byte(((ticks >> 30) & 7) << 1) | 1;
    bytes[1] = byte(ticks >> 22);
    bytes[2] = byte(((ticks >> 15) & 127) << 1) | 1;
    bytes[3] = byte(ticks >> 7);
    bytes[4] = byte((ticks & 127) << 1) | 1;
}
fn read_pcr(bytes: &[u8]) -> Result<i128> {
    let extension = (i128::from(bytes[4] & 1) << 8) | i128::from(bytes[5]);
    if bytes[4] & 0x7e != 0x7e || extension >= 300 {
        return Err(MpegTsEpochError::Transport);
    }
    let base = (i128::from(bytes[0]) << 25)
        | (i128::from(bytes[1]) << 17)
        | (i128::from(bytes[2]) << 9)
        | (i128::from(bytes[3]) << 1)
        | i128::from(bytes[4] >> 7);
    Ok(base * 300 + extension)
}
fn write_pcr(bytes: &mut [u8], ticks: i128) {
    let base = ticks / 300;
    let ext = ticks % 300;
    bytes[0] = byte(base >> 25);
    bytes[1] = byte(base >> 17);
    bytes[2] = byte(base >> 9);
    bytes[3] = byte(base >> 1);
    bytes[4] = byte((base & 1) << 7) | 0x7e | byte(ext >> 8);
    bytes[5] = byte(ext);
}
fn read_escr(bytes: &[u8]) -> Result<i128> {
    if bytes[0] & 4 == 0 || bytes[2] & 4 == 0 || bytes[4] & 4 == 0 || bytes[5] & 1 == 0 {
        return Err(MpegTsEpochError::Pes);
    }
    let base = (i128::from((bytes[0] >> 3) & 7) << 30)
        | (i128::from(bytes[0] & 3) << 28)
        | (i128::from(bytes[1]) << 20)
        | (i128::from(bytes[2] >> 3) << 15)
        | (i128::from(bytes[2] & 3) << 13)
        | (i128::from(bytes[3]) << 5)
        | i128::from(bytes[4] >> 3);
    let extension = (i128::from(bytes[4] & 3) << 7) | i128::from(bytes[5] >> 1);
    if extension >= 300 {
        return Err(MpegTsEpochError::Pes);
    }
    Ok(base * 300 + extension)
}
fn write_escr(bytes: &mut [u8], ticks: i128) {
    let base = ticks / 300;
    let extension = ticks % 300;
    bytes[0] = (bytes[0] & 0xc0) | byte(((base >> 30) & 7) << 3) | 4 | byte((base >> 28) & 3);
    bytes[1] = byte(base >> 20);
    bytes[2] = byte(((base >> 15) & 31) << 3) | 4 | byte((base >> 13) & 3);
    bytes[3] = byte(base >> 5);
    bytes[4] = byte((base & 31) << 3) | 4 | byte(extension >> 7);
    bytes[5] = byte((extension & 127) << 1) | 1;
}

#[cfg(test)]
#[path = "mpegts_epoch_tests.rs"]
mod tests;
