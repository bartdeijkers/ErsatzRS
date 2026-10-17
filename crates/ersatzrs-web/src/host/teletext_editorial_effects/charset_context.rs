//! Protected original glyph context, independent of masks and registry indexes.

use super::{check_deadline, lineage::ProtectedLineage, Result, TeletextMainComposition};
use ersatzrs_domain::{
    TeletextBaseAuthoredContent, TeletextBaseDisplayCellKind, TeletextCharsetDeclaration,
    TeletextCharsetExecution, TeletextDisplayCell, TeletextExecutedCharsetSelection,
    TeletextExecutedGlyphIntent, TeletextGlyphIdentity, TeletextGlyphIntentReason,
};
use std::time::Instant;

pub(super) fn same_cell(
    old: &TeletextMainComposition,
    new: &TeletextMainComposition,
    before: TeletextDisplayCell<'_>,
    after: TeletextDisplayCell<'_>,
    lineage: &ProtectedLineage<'_>,
    deadline: Instant,
) -> Result<bool> {
    check_deadline(deadline)?;
    let (before, after) = (intent(old, before), intent(new, after));
    let (Some(before), Some(after)) = (before, after) else {
        return Ok(before.is_none() && after.is_none());
    };
    if !same_selection(
        old.charset_execution(),
        new.charset_execution(),
        before.context().selection(),
        after.context().selection(),
        lineage,
        deadline,
    )? {
        return Ok(false);
    }
    // Physical Base overlays remove authored text annotation even on unchanged
    // spaces. Preserve compare_base's existing Glyph/Blank equivalence only for
    // that literal Base pair, after exact charset/provenance comparison.
    if equivalent_base_space(before, after) {
        return Ok(true);
    }
    let reasons_equal = match (before.reason(), after.reason()) {
        // glyph_intent::resolve_pair uses this exact context's pending choice.
        // Compare its semantic chain below, never its registry index.
        (
            Some(TeletextGlyphIntentReason::CharsetOrder { .. }),
            Some(TeletextGlyphIntentReason::CharsetOrder { .. }),
        ) => true,
        (before, after) => before == after,
    };
    if before.glyph() != after.glyph()
        || before.disposition() != after.disposition()
        || !reasons_equal
        || before.selected_pair() != after.selected_pair()
        || before.context().usage() != after.context().usage()
        || before.context().national_substitution() != after.context().national_substitution()
    {
        return Ok(false);
    }
    Ok(true)
}

fn equivalent_base_space(a: &TeletextExecutedGlyphIntent, b: &TeletextExecutedGlyphIntent) -> bool {
    let (Some(a_base), Some(b_base)) = (
        a.context().candidate().base(),
        b.context().candidate().base(),
    ) else {
        return false;
    };
    if a_base.code() != b' '
        || b_base.code() != b' '
        || a_base.position() != b_base.position()
        || a_base.attributes() != b_base.attributes()
        || a_base.attributes().graphics()
        || a.disposition() != b.disposition()
        || a.reason() != b.reason()
    {
        return false;
    }
    let (authored, coded) = match (a_base.kind(), b_base.kind()) {
        (TeletextBaseDisplayCellKind::Glyph, TeletextBaseDisplayCellKind::Blank) => (a, b),
        (TeletextBaseDisplayCellKind::Blank, TeletextBaseDisplayCellKind::Glyph) => (b, a),
        _ => return false,
    };
    authored
        .context()
        .candidate()
        .base()
        .and_then(ersatzrs_domain::TeletextBaseDisplayCell::authored)
        .is_some_and(|glyph| {
            glyph.content() == TeletextBaseAuthoredContent::Text { character: ' ' }
        })
        && matches!(authored.glyph(), Some(TeletextGlyphIdentity::G0 { glyph })
            if glyph.code() == b' ' && glyph.national_subset().is_none())
        && coded.glyph() == Some(TeletextGlyphIdentity::Blank)
}

fn intent<'a>(
    composition: &'a TeletextMainComposition,
    cell: TeletextDisplayCell<'_>,
) -> Option<&'a TeletextExecutedGlyphIntent> {
    let origin = composition
        .display_resolution()
        .origins()
        .get(cell.origin()?)?;
    composition
        .glyph_intent()
        .records()
        .get(origin.candidate_index())
}

pub(super) fn same_selection(
    old: &TeletextCharsetExecution,
    new: &TeletextCharsetExecution,
    before: Option<TeletextExecutedCharsetSelection>,
    after: Option<TeletextExecutedCharsetSelection>,
    lineage: &ProtectedLineage<'_>,
    deadline: Instant,
) -> Result<bool> {
    let (Some(mut before), Some(mut after)) = (before, after) else {
        return Ok(before.is_none() && after.is_none());
    };
    // Each previous link must consume a pending record. The sealed registries
    // are finite; the bound also refuses any malformed cyclic chain.
    for _ in 0..=old.pending().len().min(new.pending().len()) {
        check_deadline(deadline)?;
        match (before.pending(), after.pending()) {
            (None, None) => {
                return Ok(before.value() == after.value()
                    && same_known_declaration(old, new, before, after, lineage));
            }
            (Some(a), Some(b)) => {
                let (Some(a_record), Some(b_record)) = (old.pending().get(a), new.pending().get(b))
                else {
                    return Ok(false);
                };
                if a_record.position() != b_record.position()
                    || !same_contenders(old, new, a, b, lineage, deadline)?
                {
                    return Ok(false);
                }
                before = a_record.previous();
                after = b_record.previous();
            }
            _ => return Ok(false),
        }
    }
    Ok(false)
}

fn same_known_declaration(
    old: &TeletextCharsetExecution,
    new: &TeletextCharsetExecution,
    before: TeletextExecutedCharsetSelection,
    after: TeletextExecutedCharsetSelection,
    lineage: &ProtectedLineage<'_>,
) -> bool {
    match (before.declaration(), after.declaration()) {
        (None, None) => true,
        (Some(a), Some(b)) => old
            .declarations()
            .get(a)
            .zip(new.declarations().get(b))
            .is_some_and(|(a, b)| same_declaration(a, b, lineage)),
        _ => false,
    }
}

fn same_contenders(
    old: &TeletextCharsetExecution,
    new: &TeletextCharsetExecution,
    a: usize,
    b: usize,
    lineage: &ProtectedLineage<'_>,
    deadline: Instant,
) -> Result<bool> {
    let mut before = old.pending_contenders(a);
    let mut after = new.pending_contenders(b);
    loop {
        check_deadline(deadline)?;
        match (before.next(), after.next()) {
            (None, None) => return Ok(true),
            (Some(a), Some(b)) if same_declaration(a, b, lineage) => {}
            _ => return Ok(false),
        }
    }
}

fn same_declaration(
    a: &TeletextCharsetDeclaration,
    b: &TeletextCharsetDeclaration,
    lineage: &ProtectedLineage<'_>,
) -> bool {
    a.designation() == b.designation()
        && a.position() == b.position()
        && lineage.source_at_invocation(b.source(), b.invocation())
            == super::lineage::CanonicalSource::Existing(a.source())
        && a.root_source() == b.root_source()
        && a.invocation() == lineage.canonical_invocation(b.invocation())
}
