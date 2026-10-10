//! Bounded PSI framing around the library's packet and section-header readers.
//!
//! mpeg2ts-reader 0.18.2's `SectionPacketConsumer` does not buffer split common/
//! syntax headers or consume multiple sections following one pointer. Its TODOs
//! demonstrate the gap; this adapter also fences continuity before assembly.

use mpeg2ts_reader::{packet::Packet, psi::SectionCommonHeader};

use super::TeletextPsiError;

const MAX_SECTION_BYTES: usize = 1_024;

#[derive(Clone, Default)]
pub(super) struct SectionAssembler {
    pending: Vec<u8>,
    previous: Option<[u8; 188]>,
    established: bool,
}

impl SectionAssembler {
    pub(super) fn is_duplicate(&self, packet: &Packet<'_>) -> bool {
        self.previous
            .as_ref()
            .is_some_and(|previous| packet.buffer() == previous)
    }

    pub(super) fn is_pending(&self) -> bool {
        !self.pending.is_empty()
    }

    pub(super) fn push(&mut self, packet: &Packet<'_>) -> Result<Vec<Vec<u8>>, TeletextPsiError> {
        if packet
            .adaptation_field()
            .is_some_and(|field| field.discontinuity_indicator())
        {
            return Err(TeletextPsiError::Continuity);
        }
        let Some(payload) = packet.payload() else {
            if packet.payload_unit_start_indicator()
                || self
                    .previous
                    .as_ref()
                    .is_some_and(|previous| previous[3] & 15 != packet.continuity_counter().count())
            {
                return Err(TeletextPsiError::Continuity);
            }
            return Ok(Vec::new());
        };
        if let Some(previous) = &self.previous {
            let before = previous[3] & 15;
            let current = packet.continuity_counter().count();
            if current == before {
                if packet.buffer() == previous {
                    return Ok(Vec::new());
                }
                return Err(TeletextPsiError::Continuity);
            }
            if current != (before + 1) & 15 {
                return Err(TeletextPsiError::Continuity);
            }
        }
        if self.established && !self.is_pending() {
            if packet.payload_unit_start_indicator() {
                validate_section_start(packet)?;
            } else {
                stuffing(payload)?;
            }
        }
        self.previous = Some(
            packet
                .buffer()
                .try_into()
                .map_err(|_| TeletextPsiError::Framing)?,
        );
        let mut sections = Vec::new();
        if packet.payload_unit_start_indicator() {
            let pointer = usize::from(payload[0]);
            let data = &payload[1..];
            if pointer >= data.len() {
                return Err(TeletextPsiError::Framing);
            }
            // At startup the prefix can belong to an unseen previous section.
            // Once buffering begins it must finish that exact section.
            if self.is_pending() {
                self.finish_fragment(&data[..pointer], &mut sections)?;
                if self.is_pending() {
                    return Err(TeletextPsiError::Framing);
                }
            }
            self.start_sections(&data[pointer..], &mut sections)?;
        } else if self.is_pending() {
            self.finish_fragment(payload, &mut sections)?;
        }
        self.established |= !sections.is_empty();
        Ok(sections)
    }

    fn finish_fragment(
        &mut self,
        bytes: &[u8],
        sections: &mut Vec<Vec<u8>>,
    ) -> Result<(), TeletextPsiError> {
        let (consumed, complete) = self.append(bytes)?;
        if let Some(section) = complete {
            sections.push(section);
            stuffing(&bytes[consumed..])?;
        }
        Ok(())
    }

    fn start_sections(
        &mut self,
        mut bytes: &[u8],
        sections: &mut Vec<Vec<u8>>,
    ) -> Result<(), TeletextPsiError> {
        while !bytes.is_empty() {
            if bytes[0] == 0xff {
                return stuffing(bytes);
            }
            let (consumed, complete) = self.append(bytes)?;
            if let Some(section) = complete {
                sections.push(section);
                bytes = &bytes[consumed..];
            } else {
                break;
            }
        }
        Ok(())
    }

    fn append(&mut self, mut bytes: &[u8]) -> Result<(usize, Option<Vec<u8>>), TeletextPsiError> {
        let before = bytes.len();
        if self.pending.len() < 3 {
            let take = (3 - self.pending.len()).min(bytes.len());
            self.pending.extend_from_slice(&bytes[..take]);
            bytes = &bytes[take..];
            if self.pending.len() < 3 {
                return Ok((before, None));
            }
        }
        let header = SectionCommonHeader::new(&self.pending[..3]);
        let size = header.section_length + 3;
        if !(12..=MAX_SECTION_BYTES).contains(&size) || self.pending[1] & 0xf0 != 0xb0 {
            return Err(TeletextPsiError::Framing);
        }
        let take = (size - self.pending.len()).min(bytes.len());
        self.pending.extend_from_slice(&bytes[..take]);
        bytes = &bytes[take..];
        let complete = if self.pending.len() == size {
            Some(std::mem::take(&mut self.pending))
        } else {
            None
        };
        Ok((before - bytes.len(), complete))
    }
}

fn stuffing(bytes: &[u8]) -> Result<(), TeletextPsiError> {
    if bytes.iter().all(|&byte| byte == 0xff) {
        Ok(())
    } else {
        Err(TeletextPsiError::Framing)
    }
}

// A verified boundary cannot discard an unseen section's tail in its pointer.
// Discovery can join mid-section; installation and a sealed monitor cannot.
pub(super) fn validate_section_start(packet: &Packet<'_>) -> Result<(), TeletextPsiError> {
    if !packet.payload_unit_start_indicator() {
        return Err(TeletextPsiError::Framing);
    }
    let payload = packet.payload().ok_or(TeletextPsiError::Framing)?;
    let pointer = usize::from(payload[0]);
    if pointer >= payload.len() - 1 || payload[1..=pointer].iter().any(|&byte| byte != 0xff) {
        return Err(TeletextPsiError::Framing);
    }
    Ok(())
}
