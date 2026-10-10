use super::{packets, EncodedTeletextPageSet, TeletextTransportError};

// 31 units plus the data identifier fit within the 1,504-byte Bttx buffer.
pub(super) const MAX_BATCH_UNITS: usize = 31;
// EN 300 706 B.1/B.9: at least 20 ms after erase, at most 100 ms between packets.
const STEP_TICKS: u64 = 1_800;

/// Purpose of a bounded serial-mode transmission batch.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TeletextCarouselBatchKind {
    /// A page header, sent independently of its remaining packets.
    Header,
    /// Ordered links, designation, enhancements and display rows.
    Page,
    /// Reserved mFF header closing the final page of a complete cycle.
    Terminator,
}

/// Host-authored units with a relative 90 kHz presentation time.
///
/// This is a transmission plan, not evidence of delivery. The output adapter
/// must reserve capacity, pace packets on the programme clock, preserve the
/// erase interval and satisfy the DVB decoder buffer model. It must never
/// catch up overdue batches by emitting a burst or silently dropping units.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TeletextCarouselBatch {
    ticks: u64,
    kind: TeletextCarouselBatchKind,
    units: Vec<[u8; 46]>,
}

impl TeletextCarouselBatch {
    /// Presentation offset in 90 kHz ticks from the carousel's programme anchor.
    #[must_use]
    pub const fn presentation_ticks(&self) -> u64 {
        self.ticks
    }

    /// Whether these packets open, extend or close a page cycle.
    #[must_use]
    pub const fn kind(&self) -> TeletextCarouselBatchKind {
        self.kind
    }

    /// At most 31 immutable EN 300 472 units in their required wire order.
    #[must_use]
    pub fn data_units(&self) -> &[[u8; 46]] {
        &self.units
    }

    /// Exact number of payload-only TS packets required by this bounded batch.
    #[must_use]
    pub fn transport_packet_count(&self) -> usize {
        (self.units.len() + 1).div_ceil(4)
    }
}

/// Bounded, synchronous page/subpage carousel over an entirely encoded set.
///
/// Pages retain producer order and exact subpage identities. There is no task,
/// timer, input-media access or background queue here. Publication ownership,
/// generation fencing, cancellation and actual multiplex pacing belong to the
/// host adapter. Replacement is permitted only after the cycle's terminator.
#[derive(Debug)]
pub struct TeletextCarousel {
    set: EncodedTeletextPageSet,
    page: usize,
    unit: usize,
    ticks: u64,
    boundary: bool,
    first_cycle: bool,
}

impl TeletextCarousel {
    /// Start a plan at relative time zero from a validated, fully encoded set.
    #[must_use]
    pub fn new(set: EncodedTeletextPageSet) -> Self {
        Self {
            set,
            page: 0,
            unit: 0,
            ticks: 0,
            boundary: true,
            first_cycle: true,
        }
    }

    /// True before the first header or after the complete cycle's terminator.
    #[must_use]
    pub const fn is_cycle_boundary(&self) -> bool {
        self.boundary
    }

    /// Replace a complete set without mixing pages within a carousel cycle.
    ///
    /// The owner must first confirm the previous batch was actually delivered
    /// and apply its own generation fence. This method neither publishes nor
    /// acknowledges delivery; the programme-relative clock is retained.
    /// # Errors
    /// Returns an error between the opening header and closing terminator.
    pub fn replace_at_boundary(
        &mut self,
        set: EncodedTeletextPageSet,
    ) -> Result<(), TeletextTransportError> {
        if !self.boundary {
            return Err(TeletextTransportError::MidCycleReplacement);
        }
        self.set = set;
        self.first_cycle = true;
        Ok(())
    }

    /// Plan the next bounded batch, leaving a 20 ms presentation interval.
    /// # Errors
    /// Clock overflow leaves the cursor and encoded set unchanged.
    pub fn next_batch(&mut self) -> Result<TeletextCarouselBatch, TeletextTransportError> {
        let next_ticks = self
            .ticks
            .checked_add(STEP_TICKS)
            .ok_or(TeletextTransportError::ClockOverflow)?;
        let (kind, units) = if self.page == self.set.pages().len() {
            // A serial-mode page can be closed by a header from any magazine.
            let magazine = self.set.initial_page().wire_magazine();
            self.page = 0;
            self.unit = 0;
            self.boundary = true;
            self.first_cycle = false;
            (
                TeletextCarouselBatchKind::Terminator,
                vec![packets::terminator(magazine)],
            )
        } else {
            let units = self.set.pages()[self.page].data_units();
            self.boundary = false;
            if self.unit == 0 {
                self.unit = 1;
                let mut header = units[0];
                if !self.first_cycle {
                    // Repetition must not erase already acquired page content.
                    // C4 shares this Hamming nibble with subcode bits 4..6.
                    let low = self.set.pages()[self.page]
                        .address()
                        .subcode
                        .get()
                        .to_le_bytes()[0];
                    header[9] = packets::hamming_8_4(low >> 4).reverse_bits();
                }
                (TeletextCarouselBatchKind::Header, vec![header])
            } else {
                let end = (self.unit + MAX_BATCH_UNITS).min(units.len());
                let batch = units[self.unit..end].to_vec();
                self.unit = end;
                if end == units.len() {
                    self.page += 1;
                    self.unit = 0;
                }
                (TeletextCarouselBatchKind::Page, batch)
            }
        };
        let batch = TeletextCarouselBatch {
            ticks: self.ticks,
            kind,
            units,
        };
        self.ticks = next_ticks;
        Ok(batch)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::teletext::transport_tests::{draft, set};

    #[test]
    fn repeated_header_planning_preserves_the_carousels_exact_encoded_page_set() {
        let encoded = set(vec![draft(200, 0, false), draft(200, 0x3f7e, false)]);
        let original = encoded.clone();
        let mut carousel = TeletextCarousel::new(encoded);
        for _ in 0..4 {
            loop {
                let batch = carousel.next_batch().unwrap();
                assert_eq!(carousel.set, original);
                if batch.kind() == TeletextCarouselBatchKind::Terminator {
                    break;
                }
            }
        }
    }
}
