//! Finite full-MOT neutralization before a successor's acquisition begins.
//! No authored declaration is changed, and planning never acknowledges delivery.

use ersatzrs_domain::{
    TeletextMotDefaultObject, TeletextMotDrcsLink, TeletextMotEnhancementCounts,
    TeletextMotFallback, TeletextMotLevel25Declaration, TeletextMotLevel25Links,
    TeletextMotObjectLink, TeletextMotSubpageExtent, TeletextRawDataPageNumber,
    TeletextResourceIdentity, TeletextResourceKind, TeletextResourceSubpageIndex,
    TeletextX27LinkTarget, TeletextX27Magazine,
};

use super::{TeletextOutputError, TeletextOutputGate};
use crate::teletext::{
    encode_teletext_mot_page, packets, EncodedTeletextAuthoredService, EncodedTeletextResourcePage,
    TeletextCarouselBatch, TeletextCarouselBatchKind, TeletextResourceFullUpdate,
    TeletextResourceTransmissionMode,
};

pub(super) struct MotRetirement {
    pub(super) pages: Vec<EncodedTeletextResourcePage>,
    cursor: usize,
}

impl MotRetirement {
    /// At most eight magazines times sixteen logical S1 owners. Counters come
    /// from this output's completed history, including effective warm headers.
    pub(super) fn prepare(
        gate: &TeletextOutputGate,
        next: &EncodedTeletextAuthoredService,
    ) -> Result<Option<Self>, TeletextOutputError> {
        let mut pages = Vec::new();
        for magazine in 1_u8..=8 {
            let mask = gate.acknowledged_nonempty_mots[usize::from(magazine & 7)];
            for subpage in 0_u8..16 {
                if mask & (1 << subpage) == 0 {
                    continue;
                }
                let declaration = neutral(magazine, subpage)?;
                if next
                    .declared()
                    .mots()
                    .iter()
                    .any(|mot| mot.identity() == declaration.identity())
                {
                    continue;
                }
                let counter = gate
                    .carousel
                    .as_ref()
                    .and_then(|carousel| carousel.mot_retirement_counter(declaration.identity()))
                    .ok_or(TeletextOutputError::Configuration)?;
                pages.push(
                    encode_teletext_mot_page(
                        &declaration,
                        &[[None; 2]; 8],
                        counter.next_modulo_eight(),
                        TeletextResourceFullUpdate::Changed,
                        TeletextResourceTransmissionMode::Serial,
                    )
                    .map_err(|_| TeletextOutputError::Configuration)?,
                );
            }
        }
        if pages.is_empty() {
            return Ok(None);
        }
        super::TeletextCarousel::validate_retirement_prefix(next, pages.len() * 2 + 1)?;
        Ok(Some(Self { pages, cursor: 0 }))
    }

    pub(super) fn complete(&self) -> bool {
        self.cursor == self.pages.len() * 2 + 1
    }

    /// A separate header and body interval retain the original erase delay.
    /// Once closed, emit nothing until the actual sink acknowledges the prefix.
    pub(super) fn next_batch(&mut self, ticks: u64) -> Option<TeletextCarouselBatch> {
        if self.complete() {
            return None;
        }
        let (kind, units) = if self.cursor == self.pages.len() * 2 {
            (
                TeletextCarouselBatchKind::Terminator,
                vec![packets::terminator(
                    self.pages[0].header().identity().page().wire_magazine(),
                )],
            )
        } else {
            let units = self.pages[self.cursor / 2].data_units();
            if self.cursor.is_multiple_of(2) {
                (TeletextCarouselBatchKind::Header, vec![units[0]])
            } else {
                (TeletextCarouselBatchKind::Page, units[1..].to_vec())
            }
        };
        self.cursor += 1;
        Some(TeletextCarouselBatch::retirement(ticks, kind, units))
    }
}

fn neutral(
    magazine: u8,
    subpage: u8,
) -> Result<TeletextMotLevel25Declaration, TeletextOutputError> {
    let target = TeletextX27LinkTarget::Unspecified {
        magazine: TeletextX27Magazine::new(magazine).map_err(invalid)?,
    };
    let extent = TeletextMotSubpageExtent::new(0).map_err(invalid)?;
    let object = TeletextMotObjectLink::new(
        target,
        extent,
        false,
        TeletextMotFallback::new(0).map_err(invalid)?,
        [TeletextMotDefaultObject::NoObject { raw_number: 0 }; 2],
    );
    let drcs = TeletextMotDrcsLink::new(target, extent, false);
    TeletextMotLevel25Declaration::new(
        TeletextResourceIdentity::new(
            TeletextResourceKind::Mot,
            TeletextRawDataPageNumber::new(magazine, 0xfe).map_err(invalid)?,
            TeletextResourceSubpageIndex::new(subpage).map_err(invalid)?,
        )
        .map_err(invalid)?,
        vec![],
        TeletextMotLevel25Links::new(
            object,
            [object; 7],
            drcs,
            [drcs; 7],
            TeletextMotEnhancementCounts::new(0, 0, 0),
        )
        .map_err(invalid)?,
    )
    .map_err(invalid)
}

fn invalid<E>(_: E) -> TeletextOutputError {
    TeletextOutputError::Configuration
}
