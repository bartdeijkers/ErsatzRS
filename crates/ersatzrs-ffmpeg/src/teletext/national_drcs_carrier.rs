//! Compiler-owned realization of one exact national identity through an existing
//! authored DRCS definition. This neither allocates resources nor changes their
//! ownership. A matching bitmap is evidence only AFTER the original identity is
//! established; it never makes arbitrary glyph identities interchangeable.

use ersatzrs_domain::{
    resolve_teletext_glyph_mask, TeletextDrcsBitmap, TeletextDrcsGlyphReference,
    TeletextDrcsGlyphSlot, TeletextDrcsSubtableIndex, TeletextDrcsTableScope, TeletextG0Repertoire,
    TeletextGlyphDisposition, TeletextGlyphIdentity, TeletextNationalSubset,
    TeletextResourceIdentity, TeletextResourceKind, TeletextSemanticService,
};

use super::TeletextMainComposition;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) struct NationalDrcsCarrier {
    original: TeletextGlyphIdentity,
    reference: TeletextDrcsGlyphReference,
    resource: TeletextResourceIdentity,
    bitmap: TeletextDrcsBitmap,
    underlined: bool,
}

impl NationalDrcsCarrier {
    pub(super) const fn original(self) -> TeletextGlyphIdentity {
        self.original
    }
    pub(super) const fn reference(self) -> TeletextDrcsGlyphReference {
        self.reference
    }
    pub(super) const fn resource(self) -> TeletextResourceIdentity {
        self.resource
    }
    pub(super) const fn bitmap(self) -> TeletextDrcsBitmap {
        self.bitmap
    }
    pub(super) const fn underlined(self) -> bool {
        self.underlined
    }
}

/// Conservative admission for both scope scans, ten-row comparisons and the
/// fixed mask lookup. The assembly's existing non-renewing credit is charged
/// before calling `resolve`, including unsuccessful searches in either pass.
pub(super) fn scan_work(service: &TeletextSemanticService) -> Option<usize> {
    service
        .drcs()
        .subtables()
        .iter()
        .try_fold(120_usize, |n, table| {
            table
                .glyphs()
                .len()
                .checked_mul(20)
                .and_then(|glyphs| glyphs.checked_add(2))
                .and_then(|work| n.checked_add(work))
        })
}

pub(super) fn needs_binding(composition: &TeletextMainComposition, origin: usize) -> bool {
    original_identity(composition, origin).is_some()
}

pub(super) fn required_bitmap(
    composition: &TeletextMainComposition,
    origin: usize,
) -> Option<TeletextDrcsBitmap> {
    let original = original_identity(composition, origin)?;
    let mask = resolve_teletext_glyph_mask(original, None, false).ok()?;
    let mask = if original_underline(composition, origin)? {
        mask.with_underline()
    } else {
        mask
    };
    TeletextDrcsBitmap::new(mask.rows()).ok()
}

/// These two fixed slots depend only on the established original glyph/effect,
/// never on consumer discovery order or which other variant is present.
pub(super) fn required_slot(
    composition: &TeletextMainComposition,
    origin: usize,
) -> Option<TeletextDrcsGlyphSlot> {
    original_identity(composition, origin)?;
    TeletextDrcsGlyphSlot::new(u8::from(original_underline(composition, origin)?)).ok()
}

fn original_underline(composition: &TeletextMainComposition, origin: usize) -> Option<bool> {
    composition
        .display_resolution()
        .origin_attributes(origin)?
        .underline_or_separated_mosaics()
        .value()
}

fn original_identity(
    composition: &TeletextMainComposition,
    origin: usize,
) -> Option<TeletextGlyphIdentity> {
    // Table 29 does not assign DRCS underline/separation. The original G0 D5
    // effect is baked into the exact mask; the physical DRCS always has D5=0.
    // Table 26 spacing mosaic separation has no painted effect on this assigned
    // G0 glyph. Keep its known original latch; the physical D5=0 cancels it.
    let attributes = composition.display_resolution().origin_attributes(origin)?;
    if attributes
        .underline_or_separated_mosaics()
        .value()
        .is_none()
        || attributes.spacing_separated().value().is_none()
    {
        return None;
    }
    let candidate_index = composition
        .display_resolution()
        .origins()
        .get(origin)?
        .candidate_index();
    let intent = composition.glyph_intent().records().get(candidate_index)?;
    let base = intent.context().candidate().base()?;
    if intent.disposition() != TeletextGlyphDisposition::Assigned
        || composition
            .base_replacement(base.position().row(), base.position().column())
            .is_some()
    {
        return None;
    }
    let original = intent.glyph()?;
    if !matches!(original, TeletextGlyphIdentity::G0 {glyph}
        if glyph.repertoire() == TeletextG0Repertoire::Latin
            && glyph.national_subset() == Some(TeletextNationalSubset::Polish)
            && glyph.code() == 0x5b && base.code() == 0x5b)
    {
        return None;
    }
    Some(original)
}

/// Only Table 36 Polish 5/B is established here. Its national identity remains
/// literal in the original service; no G0/G2 alias or editor metadata is trusted.
/// The existing pinned original font policy supplies its exact 12x10 bitmap.
/// Missing resources/mismatched pixels preserve the located conversion failure.
pub(super) fn resolve(
    service: &TeletextSemanticService,
    composition: &TeletextMainComposition,
    origin: usize,
) -> Option<NationalDrcsCarrier> {
    let original = original_identity(composition, origin)?;
    let bitmap = required_bitmap(composition, origin)?;
    let links = service
        .page_enhancements()
        .get(&composition.owner())?
        .resources
        .drcs;
    // Prefer the existing normal link, then global; authored table/slot order
    // breaks ties. No link, S1, slot, definition or lifecycle field is rewritten.
    for (scope, kind) in [
        (TeletextDrcsTableScope::Normal, TeletextResourceKind::Drcs),
        (TeletextDrcsTableScope::Global, TeletextResourceKind::Gdrcs),
    ] {
        let Some(page) = links.page(scope) else {
            continue;
        };
        for table in service.drcs().subtables() {
            let resource = table.identity();
            if resource.kind() != kind || resource.page() != page {
                continue;
            }
            for glyph in table.glyphs() {
                if glyph.bitmap == bitmap {
                    return Some(NationalDrcsCarrier {
                        original,
                        reference: TeletextDrcsGlyphReference {
                            table: scope,
                            subtable: TeletextDrcsSubtableIndex::new(
                                resource.logical_subpage().get(),
                            )
                            .ok()?,
                            slot: glyph.slot,
                        },
                        resource,
                        bitmap,
                        underlined: original_underline(composition, origin)?,
                    });
                }
            }
        }
    }
    None
}
