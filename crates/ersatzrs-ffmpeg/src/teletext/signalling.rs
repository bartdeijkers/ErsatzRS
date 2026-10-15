use std::collections::BTreeSet;

use ersatzrs_domain::{
    TeletextLanguage, TeletextPageAddress, TeletextPageKind, TeletextPageNumber,
};
use mpeg2ts_reader::{
    descriptor::{DescriptorIter, UnknownDescriptor},
    mpegts_crc,
    packet::Packet,
    psi::{pmt::PmtSection, SectionCommonHeader, TableSyntaxHeader},
};

use super::{
    EncodedTeletextAuthoredService, EncodedTeletextPageSet, TeletextCarouselBatch,
    TeletextPacketizer, TeletextTimestamp,
};

const MAX_PMT_BYTES: usize = 1_024;
pub(super) const MAX_WINDOW_PACKETS: usize = 32_768;

#[cfg(test)]
#[path = "pmt_plan_reconfiguration_tests.rs"]
mod reconfiguration_tests;

mpeg2ts_reader::descriptor_enum! {
    RawDescriptors {
        Other 0..=255 => UnknownDescriptor,
    }
}

/// Preflight failures never partially overwrite a PMT or transport window.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum TeletextMultiplexError {
    /// A PMT has incorrect framing, reserved bits or incomplete stream records.
    #[error("teletext requires a complete current single-section PMT")]
    Pmt,
    /// A source section fails the MPEG CRC check.
    #[error("teletext source PMT CRC is invalid")]
    Crc,
    /// A descriptor has incomplete framing or an invalid CA PID field.
    #[error("teletext source PMT descriptor is invalid")]
    Descriptor,
    /// A complete rewritten section would exceed MPEG's PMT size limit.
    #[error("teletext descriptors exceed the PMT section capacity")]
    SectionCapacity,
    /// A supplied PID is reserved, null or outside the elementary range.
    #[error("teletext requires dedicated non-reserved PMT and elementary PIDs")]
    Pid,
    /// Source PSI or observed output already owns the proposed elementary PID.
    #[error("teletext PID {pid} is already occupied")]
    PidCollision { pid: u16 },
    /// A service without a PCR cannot anchor the authored page clock.
    #[error("teletext source PMT has no usable programme PCR")]
    Clock,
    /// PMT packetization cannot start from an invalid continuity counter.
    #[error("teletext PMT continuity counter exceeds four bits")]
    Continuity,
    /// A transport window has malformed or scrambled packets.
    #[error("teletext transport window contains an invalid or scrambled packet")]
    Packet,
    /// Input extent exceeds the bounded preflight window.
    #[error("teletext transport window exceeds its packet bound")]
    Window,
    /// Capacity is insufficient; neither input bytes nor counters were changed.
    #[error("teletext needs {needed} null packets but the window has {available}")]
    Capacity { needed: usize, available: usize },
}

/// Host-owned EN 300 468 teletext descriptors for an encoded page generation.
///
/// Advertise the initial display page and each subtitle page/language. Subpage
/// variants share their descriptor identity. Other authored pages remain
/// reachable by page number and links; a generic engine does not label their
/// editorial content as a programme schedule. Multiple descriptors retain all
/// entries without truncating the one-byte descriptor length.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TeletextServiceDescriptor {
    bytes: Vec<u8>,
}

impl TeletextServiceDescriptor {
    /// Derive signalling from validated host pages, never add-on transport bytes.
    #[must_use]
    pub fn from_page_set(set: &EncodedTeletextPageSet) -> Self {
        Self::from_displays(
            set.initial_page(),
            set.pages()
                .iter()
                .map(|page| (page.address(), page.kind(), page.language())),
        )
    }

    /// Advertise only actual initial/subtitle displays from this same candidate.
    /// Resource pages and M29 packets are not navigable descriptor entries.
    #[must_use]
    pub fn from_authored_service(service: &EncodedTeletextAuthoredService) -> Self {
        let base = service.declared().semantic().base();
        Self::from_displays(
            base.initial_page,
            base.pages
                .iter()
                .map(|page| (page.address, page.kind, &page.language)),
        )
    }

    fn from_displays<'a>(
        initial: TeletextPageNumber,
        pages: impl Iterator<Item = (TeletextPageAddress, TeletextPageKind, &'a TeletextLanguage)>,
    ) -> Self {
        let mut entries = BTreeSet::new();
        for (address, page_kind, language) in pages {
            let kind = if page_kind == TeletextPageKind::Subtitle {
                2
            } else if address.page == initial {
                1
            } else {
                continue;
            };
            let language = language.as_str().as_bytes();
            entries.insert([
                language[0],
                language[1],
                language[2],
                (kind << 3) | address.page.wire_magazine(),
                address.page.wire_page(),
            ]);
        }
        let entries: Vec<_> = entries.into_iter().collect();
        let mut bytes = Vec::new();
        for chunk in entries.chunks(51) {
            // Fifty-one five-byte entries fit the descriptor's 8-bit length.
            let [.., length] = (chunk.len() * 5).to_be_bytes();
            bytes.extend_from_slice(&[0x56, length]);
            bytes.extend(chunk.iter().flatten());
        }
        Self { bytes }
    }

    /// Complete descriptor fields, including each tag and its bounded length.
    #[must_use]
    pub fn bytes(&self) -> &[u8] {
        &self.bytes
    }
}

/// A validated source PMT with a complete additional private teletext stream.
///
/// Original program/stream descriptor bytes and PCR PID are retained. This plan
/// does not assemble input PSI, prove PAT/other-program PID ownership, install
/// the PMT, establish constant-rate timing or publish a service. Those duties
/// remain in the host transport adapter; a PMT alone is insufficient admission.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TeletextPmtPlan {
    source: Vec<u8>,
    section: Vec<u8>,
    program: u16,
    pmt_pid: u16,
    pid: u16,
    pcr_pid: u16,
}

impl TeletextPmtPlan {
    /// Retain the exact admitted raw source and reserve while changing signalling.
    ///
    /// The version advances from this plan's output, including after withdrawal;
    /// `None` emits the original source streams without the authored stream.
    /// This does not acknowledge delivery or establish receiver cache removal.
    /// # Errors
    /// Rejects an empty descriptor or a section exceeding the PMT size limit.
    pub fn reconfigure(
        &self,
        service: Option<&TeletextServiceDescriptor>,
    ) -> Result<Self, TeletextMultiplexError> {
        let extra = match service {
            Some(service) if service.bytes.is_empty() => {
                return Err(TeletextMultiplexError::SectionCapacity);
            }
            Some(service) => 5 + service.bytes.len(),
            None => 0,
        };
        let size = self
            .source
            .len()
            .checked_add(extra)
            .filter(|&size| size <= MAX_PMT_BYTES)
            .ok_or(TeletextMultiplexError::SectionCapacity)?;
        let mut section = self.source[..self.source.len() - 4].to_vec();
        if let Some(service) = service {
            let [high, low] = self.pid.to_be_bytes();
            let [length_high, length_low] = u16::try_from(service.bytes.len())
                .map_err(|_| TeletextMultiplexError::SectionCapacity)?
                .to_be_bytes();
            section.extend_from_slice(&[6, 0xe0 | high, low, 0xf0 | length_high, length_low]);
            section.extend_from_slice(&service.bytes);
        }
        let [high, low] = u16::try_from(size - 3)
            .map_err(|_| TeletextMultiplexError::SectionCapacity)?
            .to_be_bytes();
        section[1] = 0xb0 | high;
        section[2] = low;
        let version = (self.section[5] >> 1) & 31;
        section[5] = 0xc1 | (((version + 1) & 31) << 1);
        section.extend_from_slice(&mpegts_crc::sum32(&section).to_be_bytes());
        Ok(Self {
            source: self.source.clone(),
            section,
            program: self.program,
            pmt_pid: self.pmt_pid,
            pid: self.pid,
            pcr_pid: self.pcr_pid,
        })
    }

    pub(super) fn is_reconfiguration_of(&self, previous: &Self) -> bool {
        self.source == previous.source
            && self.program == previous.program
            && self.pmt_pid == previous.pmt_pid
            && self.pid == previous.pid
            && self.pcr_pid == previous.pcr_pid
            && ((self.section[5] >> 1) & 31) == (((previous.section[5] >> 1) + 1) & 31)
    }

    /// Admit an exact source PMT while reserving a future authored PID.
    /// No stream, descriptor or version change is emitted by this plan.
    /// # Errors
    /// Applies the same source framing, CRC, topology, clock and PID fences.
    pub(super) fn new_absent(
        source: &[u8],
        pmt_pid: u16,
        pid: u16,
    ) -> Result<Self, TeletextMultiplexError> {
        validate_pid(pmt_pid)?;
        validate_pid(pid)?;
        let (program, _, pcr_pid) = validate_pmt(source, pmt_pid, pid)?;
        Ok(Self {
            source: source.to_vec(),
            section: source.to_vec(),
            program,
            pmt_pid,
            pid,
            pcr_pid,
        })
    }

    /// Check the complete source section and append host-owned service signalling.
    /// # Errors
    /// Rejects invalid CRC/framing/records, unusable PCR, PID conflicts and an
    /// oversized result. The input and all existing stream bytes remain intact.
    pub fn new(
        source: &[u8],
        pmt_pid: u16,
        pid: u16,
        service: &TeletextServiceDescriptor,
    ) -> Result<Self, TeletextMultiplexError> {
        validate_pid(pmt_pid)?;
        validate_pid(pid)?;
        let (program, version, pcr_pid) = validate_pmt(source, pmt_pid, pid)?;
        let size = source.len() + 5 + service.bytes.len();
        if size > MAX_PMT_BYTES || service.bytes.is_empty() {
            return Err(TeletextMultiplexError::SectionCapacity);
        }
        let mut section = source[..source.len() - 4].to_vec();
        let [high, low] = pid.to_be_bytes();
        let [length_high, length_low] = u16::try_from(service.bytes.len())
            .map_err(|_| TeletextMultiplexError::SectionCapacity)?
            .to_be_bytes();
        section.extend_from_slice(&[6, 0xe0 | high, low, 0xf0 | length_high, length_low]);
        section.extend_from_slice(&service.bytes);
        let [high, low] = u16::try_from(size - 3)
            .map_err(|_| TeletextMultiplexError::SectionCapacity)?
            .to_be_bytes();
        section[1] = 0xb0 | high;
        section[2] = low;
        section[5] = 0xc1 | (((version + 1) & 31) << 1);
        section.extend_from_slice(&mpegts_crc::sum32(&section).to_be_bytes());
        Ok(Self {
            source: source.to_vec(),
            section,
            program,
            pmt_pid,
            pid,
            pcr_pid,
        })
    }

    /// Original section, including its CRC, for refresh/ownership comparison.
    #[must_use]
    pub fn source_section(&self) -> &[u8] {
        &self.source
    }

    /// Exact planned section and CRC: unchanged for initial absence, with an
    /// incremented output version after authored signalling changes.
    #[must_use]
    pub fn section(&self) -> &[u8] {
        &self.section
    }

    /// Source program identity; the host must match it to the selected PAT entry.
    #[must_use]
    pub const fn program_number(&self) -> u16 {
        self.program
    }

    /// Source PMT PID; the host must match it to the selected PAT entry.
    #[must_use]
    pub const fn pmt_pid(&self) -> u16 {
        self.pmt_pid
    }

    /// Source PCR PID, retained without modifying its packets.
    #[must_use]
    pub const fn pcr_pid(&self) -> u16 {
        self.pcr_pid
    }

    /// Dedicated elementary PID whose whole-stream ownership still needs proof.
    #[must_use]
    pub const fn elementary_pid(&self) -> u16 {
        self.pid
    }

    /// Packetize this complete PMT, including pointer zero and final stuffing.
    /// # Errors
    /// Rejects a starting continuity counter beyond four bits.
    pub fn packets(&self, continuity: u8) -> Result<Vec<[u8; 188]>, TeletextMultiplexError> {
        if continuity > 15 {
            return Err(TeletextMultiplexError::Continuity);
        }
        let [high, low] = self.pmt_pid.to_be_bytes();
        let mut payload = Vec::with_capacity(self.section.len() + 1);
        payload.push(0);
        payload.extend_from_slice(&self.section);
        Ok(payload
            .chunks(184)
            .zip(0_u8..6)
            .map(|(payload, index)| {
                let mut packet = [0xff; 188];
                packet[..4].copy_from_slice(&[
                    0x47,
                    high | if index == 0 { 0x40 } else { 0 },
                    low,
                    0x10 | ((continuity + index) & 15),
                ]);
                packet[4..4 + payload.len()].copy_from_slice(payload);
                packet
            })
            .collect())
    }

    /// Preflight a bounded output window, then consume only the required null slots.
    ///
    /// The caller must first install the rewritten PMT, prove whole-stream PID
    /// ownership and select this window from the batch's actual transmission
    /// deadline in verified constant-rate output. This primitive does not admit
    /// rates, pace delivery or create extra capacity. Returned positions support
    /// the adapter's deadline/buffer audit; all other packets stay byte-identical.
    /// # Errors
    /// Malformed input, PID conflict or insufficient capacity leaves both the
    /// window and packetizer continuity untouched.
    pub fn replace_null_packets(
        &self,
        window: &mut [[u8; 188]],
        batch: &TeletextCarouselBatch,
        pts: TeletextTimestamp,
        packetizer: &mut TeletextPacketizer,
    ) -> Result<Vec<usize>, TeletextMultiplexError> {
        if packetizer.pid() != self.pid {
            return Err(TeletextMultiplexError::Pid);
        }
        if window.len() > MAX_WINDOW_PACKETS {
            return Err(TeletextMultiplexError::Window);
        }
        let needed = batch.transport_packet_count();
        let mut slots = Vec::with_capacity(needed);
        let mut available = 0;
        for (index, raw) in window.iter().enumerate() {
            let packet = valid_packet(raw)?;
            let pid = u16::from(packet.pid());
            if pid == self.pid {
                return Err(TeletextMultiplexError::PidCollision { pid });
            }
            if pid == 0x1fff {
                available += 1;
                if slots.len() < needed {
                    slots.push(index);
                }
            }
        }
        if available < needed {
            return Err(TeletextMultiplexError::Capacity { needed, available });
        }
        for (&slot, packet) in slots.iter().zip(packetizer.packetize(batch, pts)) {
            window[slot] = packet;
        }
        Ok(slots)
    }
}

pub(super) fn validate_pid(pid: u16) -> Result<(), TeletextMultiplexError> {
    if !(0x20..=0x1ffe).contains(&pid) {
        return Err(TeletextMultiplexError::Pid);
    }
    Ok(())
}

pub(super) fn validate_pmt(
    source: &[u8],
    pmt_pid: u16,
    pid: u16,
) -> Result<(u16, u8, u16), TeletextMultiplexError> {
    validate_pmt_inner(source, pmt_pid, Some(pid))
}

/// Report-only input validation: permits an occupied source PID and no-PCR PMT.
/// Authored admission continues to use `validate_pmt` and its clock/PID fences.
pub(super) fn validate_source_pmt(
    source: &[u8],
    pmt_pid: u16,
) -> Result<(u16, u8, u16), TeletextMultiplexError> {
    validate_pmt_inner(source, pmt_pid, None)
}

fn validate_pmt_inner(
    source: &[u8],
    pmt_pid: u16,
    candidate: Option<u16>,
) -> Result<(u16, u8, u16), TeletextMultiplexError> {
    if !(16..=MAX_PMT_BYTES).contains(&source.len())
        || source[1] & 0xf0 != 0xb0
        || source[5] & 0xc1 != 0xc1
    {
        return Err(TeletextMultiplexError::Pmt);
    }
    let common = SectionCommonHeader::new(&source[..3]);
    let syntax = TableSyntaxHeader::new(&source[3..8]);
    if common.table_id != 2
        || common.section_length + 3 != source.len()
        || syntax.id() == 0
        || syntax.section_number() != 0
        || syntax.last_section_number() != 0
    {
        return Err(TeletextMultiplexError::Pmt);
    }
    if mpegts_crc::sum32(source) != 0 {
        return Err(TeletextMultiplexError::Crc);
    }
    let body = &source[8..source.len() - 4];
    let pmt = PmtSection::from_bytes(body).map_err(|_| TeletextMultiplexError::Pmt)?;
    let pcr = u16::from(pmt.pcr_pid());
    if (validate_pid(pcr).is_err() && !(candidate.is_none() && pcr == 0x1fff)) || pcr == pmt_pid {
        return Err(TeletextMultiplexError::Clock);
    }
    if let Some(pid) = candidate.filter(|&pid| pid == pcr || pid == pmt_pid) {
        return Err(TeletextMultiplexError::PidCollision { pid });
    }
    if body[0] & 0xe0 != 0xe0 || body[2] & 0xf0 != 0xf0 {
        return Err(TeletextMultiplexError::Pmt);
    }
    let mut offset = 4 + descriptors(pmt.descriptors::<RawDescriptors<'_>>(), candidate)?;
    let mut occupied = BTreeSet::new();
    for stream in pmt.streams() {
        let stream_pid = u16::from(stream.elementary_pid());
        validate_pid(stream_pid)?;
        if let Some(pid) = candidate.filter(|&pid| stream_pid == pid) {
            return Err(TeletextMultiplexError::PidCollision { pid });
        }
        if stream_pid == pmt_pid
            || !occupied.insert(stream_pid)
            || body[offset + 1] & 0xe0 != 0xe0
            || body[offset + 3] & 0xf0 != 0xf0
        {
            return Err(TeletextMultiplexError::Pmt);
        }
        offset += 5 + descriptors(stream.descriptors::<RawDescriptors<'_>>(), candidate)?;
    }
    // The library's stream iterator stops on malformed trailing records.
    // Require exact consumption so truncation cannot masquerade as a valid PMT.
    if offset != body.len() {
        return Err(TeletextMultiplexError::Pmt);
    }
    Ok((syntax.id(), syntax.version(), pcr))
}

fn descriptors<'a>(
    items: impl Iterator<Item = Result<RawDescriptors<'a>, mpeg2ts_reader::descriptor::DescriptorError>>,
    candidate: Option<u16>,
) -> Result<usize, TeletextMultiplexError> {
    let mut length = 0;
    for item in items {
        let RawDescriptors::Other(descriptor) =
            item.map_err(|_| TeletextMultiplexError::Descriptor)?;
        length += descriptor.payload.len() + 2;
        if descriptor.tag == 9 {
            if descriptor.payload.len() < 4 || descriptor.payload[2] & 0xe0 != 0xe0 {
                return Err(TeletextMultiplexError::Descriptor);
            }
            let ca_pid =
                (u16::from(descriptor.payload[2] & 0x1f) << 8) | u16::from(descriptor.payload[3]);
            validate_pid(ca_pid).map_err(|_| TeletextMultiplexError::Descriptor)?;
            if let Some(pid) = candidate.filter(|&pid| ca_pid == pid) {
                return Err(TeletextMultiplexError::PidCollision { pid });
            }
        }
    }
    Ok(length)
}

pub(super) fn validate_descriptor_bytes(
    bytes: &[u8],
    pid: u16,
) -> Result<(), TeletextMultiplexError> {
    validate_source_descriptor_bytes(bytes, Some(pid))
}

pub(super) fn validate_source_descriptor_bytes(
    bytes: &[u8],
    candidate: Option<u16>,
) -> Result<(), TeletextMultiplexError> {
    descriptors(DescriptorIter::<RawDescriptors<'_>>::new(bytes), candidate).map(|_| ())
}

pub(super) fn valid_packet(raw: &[u8; 188]) -> Result<Packet<'_>, TeletextMultiplexError> {
    let packet = Packet::try_new(raw).ok_or(TeletextMultiplexError::Packet)?;
    if packet.transport_error_indicator()
        || packet.transport_scrambling_control().is_scrambled()
        || raw[3] & 0x30 == 0
    {
        return Err(TeletextMultiplexError::Packet);
    }
    if packet.adaptation_control().has_payload() && packet.payload().is_none() {
        return Err(TeletextMultiplexError::Packet);
    }
    if packet.adaptation_control().has_adaptation_field()
        && !(packet.adaptation_control().has_payload() && raw[4] == 0)
        && packet.adaptation_field().is_none()
    {
        return Err(TeletextMultiplexError::Packet);
    }
    Ok(packet)
}
