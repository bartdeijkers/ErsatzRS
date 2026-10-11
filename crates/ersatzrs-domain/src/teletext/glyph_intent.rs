//! Finite original glyph assignments from EN 300 706 tables 35–48.
//!
//! Table identities retain raw repertoire/code positions, not guessed Unicode.
//! This is no font, raster, geometry, renderer or publication certificate. Blank
//! cells without an explicit table meaning remain pending. Nonzero diacritic
//! composition requires independently established alphabetic/mark evidence;
//! no repertoire is excluded merely because a reference decoder lacks support.

use std::mem::size_of;

use super::charset_execution_values::PendingFilter;

use super::{
    TeletextBaseDisplayCellKind, TeletextCharsetExecution, TeletextCharsetPair,
    TeletextColumnOperation, TeletextEnhancementSource, TeletextExecutedGlyphCharset,
    TeletextExecutedGlyphIntent, TeletextG0Glyph, TeletextG0Repertoire, TeletextG2Glyph,
    TeletextG2Repertoire, TeletextGlyphDisposition, TeletextGlyphIdentity, TeletextGlyphIntent,
    TeletextGlyphIntentReason, TeletextMosaic, TeletextNationalSubset, TeletextPageAddress,
    TeletextSpatialInvocationId, TeletextSpatialRootSource,
};

/// Static lookup rejection, preserving original numeric location/provenance.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
#[error("teletext glyph intent for {owner:?} at {position:?}: {reason}")]
pub struct TeletextGlyphIntentError {
    pub owner: TeletextPageAddress,
    pub position: Option<(u16, u16)>,
    pub location: Option<TeletextEnhancementSource>,
    pub root_source: Option<TeletextSpatialRootSource>,
    pub invocation: Option<TeletextSpatialInvocationId>,
    pub reason: TeletextGlyphIntentFailure,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, thiserror::Error)]
pub enum TeletextGlyphIntentFailure {
    #[error("candidate count {count} exceeds budget {maximum}")]
    CandidateBudget { count: usize, maximum: usize },
    #[error("glyph intent allocation size exceeds storage range")]
    AllocationSize,
    #[error("original charset record lacks consistent coded role or selection")]
    InconsistentContext,
}

/// Resolve all original coded identities without choosing a winning candidate.
///
/// `max_candidates` bounds ALL records, including the 992 non-header Base cells.
/// Exact count, linear storage and shared pending-scan arithmetic are checked
/// before allocating. Each pending selection's pair consensus is cached once;
/// raw aliases, ordering uncertainty and prior context remain unchanged.
/// Pending charset indexes reuse the supplied execution's registry, without a
/// registry clone or per-cell contender vector. The authoritative factory must
/// retain that same execution alongside this output; owner equality is not a
/// generation certificate. Extension primary selects Level 2.5 Base NOS; original
/// header bits and secondary G0 remain separate in the original context/metadata.
/// No ESC or national substitution for enhancement G0 is inferred.
/// # Errors
/// Rejects an exceeded trusted count/storage bound or inconsistent private input
/// record. Assignment/composability uncertainty remains typed per-record pending;
/// no partial result is returned and the original inputs remain unchanged.
pub fn resolve_teletext_glyph_intent(
    charset: &TeletextCharsetExecution,
    max_candidates: usize,
) -> Result<TeletextGlyphIntent, TeletextGlyphIntentError> {
    let count = charset.records().len();
    if count > max_candidates {
        return Err(lookup_error(
            charset,
            charset.records().get(max_candidates),
            TeletextGlyphIntentFailure::CandidateBudget {
                count,
                maximum: max_candidates,
            },
        ));
    }
    let allocation = count
        .checked_mul(size_of::<TeletextExecutedGlyphIntent>())
        .and_then(|bytes| {
            charset
                .pending()
                .len()
                .checked_mul(size_of::<Option<TeletextCharsetPair>>())
                .and_then(|cache| bytes.checked_add(cache))
        });
    let work = charset
        .shared
        .len()
        .checked_add(1)
        .and_then(|span| charset.pending().len().checked_mul(span))
        .and_then(|scans| scans.checked_add(count));
    if allocation.is_none_or(|bytes| isize::try_from(bytes).is_err()) || work.is_none() {
        return Err(lookup_error(
            charset,
            None,
            TeletextGlyphIntentFailure::AllocationSize,
        ));
    }
    let pairs = pending_pairs(charset)?;
    let mut records = Vec::with_capacity(count);
    for &context in charset.records() {
        records.push(resolve_record(charset, context, &pairs)?);
    }
    Ok(TeletextGlyphIntent {
        owner: charset.owner(),
        records,
    })
}

fn pending_pairs(
    charset: &TeletextCharsetExecution,
) -> Result<Vec<Option<TeletextCharsetPair>>, TeletextGlyphIntentError> {
    let mut pairs = Vec::with_capacity(charset.pending().len());
    for (index, pending) in charset.pending().iter().enumerate() {
        let mut pair = None;
        let mut agreed = true;
        if matches!(pending.filter, PendingFilter::Glyph { .. }) {
            let previous = pending.previous();
            pair = if let Some(value) = previous.value() {
                Some(value.pair())
            } else if let Some(earlier) = previous.pending().filter(|&earlier| earlier < index) {
                pairs[earlier]
            } else {
                return Err(lookup_error(
                    charset,
                    None,
                    TeletextGlyphIntentFailure::InconsistentContext,
                ));
            };
            // A disagreement in an applicable older context cannot disappear
            // merely because the current group's last two aliases agree.
            agreed = pair.is_some();
        }
        for contender in charset.pending_contenders(index) {
            let actual = contender.designation().pair();
            if let Some(expected) = pair {
                agreed &= expected == actual;
            } else {
                pair = Some(actual);
            }
        }
        pairs.push(if agreed { pair } else { None });
    }
    Ok(pairs)
}

fn resolve_record(
    charset: &TeletextCharsetExecution,
    context: TeletextExecutedGlyphCharset,
    pairs: &[Option<TeletextCharsetPair>],
) -> Result<TeletextExecutedGlyphIntent, TeletextGlyphIntentError> {
    let mut record = TeletextExecutedGlyphIntent {
        context,
        disposition: TeletextGlyphDisposition::Assigned,
        glyph: None,
        reason: None,
        selected_pair: None,
    };
    if let Some(cell) = context.candidate().base() {
        record.glyph = Some(match cell.kind() {
            TeletextBaseDisplayCellKind::Blank => TeletextGlyphIdentity::Blank,
            TeletextBaseDisplayCellKind::SpacingControl { .. } => {
                TeletextGlyphIdentity::SpacingControl { code: cell.code() }
            }
            TeletextBaseDisplayCellKind::HeaderOwned => {
                return Err(lookup_error(
                    charset,
                    Some(&context),
                    TeletextGlyphIntentFailure::InconsistentContext,
                ))
            }
            TeletextBaseDisplayCellKind::Glyph => {
                if cell.attributes().graphics() && matches!(cell.code(), 0x20..=0x3f | 0x60..=0x7f)
                {
                    let blocks = (cell.code() & 0x1f) | ((cell.code() & 0x40) >> 1);
                    let mosaic = TeletextMosaic::new(blocks).map_err(|_| {
                        lookup_error(
                            charset,
                            Some(&context),
                            TeletextGlyphIntentFailure::InconsistentContext,
                        )
                    })?;
                    TeletextGlyphIdentity::G1Mosaic { mosaic }
                } else {
                    record.selected_pair = Some(charset.primary().pair());
                    let (g0, _) = repertoires(charset.primary().pair());
                    let glyph = TeletextG0Glyph {
                        repertoire: g0,
                        code: cell.code(),
                        national_subset: if context.national_substitution()
                            && is_national_code(cell.code())
                        {
                            national_subset(charset.primary().get())
                        } else {
                            None
                        },
                    };
                    assign_g0(&mut record, glyph);
                    TeletextGlyphIdentity::G0 { glyph }
                }
            }
        });
        return Ok(record);
    }
    let operation = context.candidate().operation().ok_or_else(|| {
        lookup_error(
            charset,
            Some(&context),
            TeletextGlyphIntentFailure::InconsistentContext,
        )
    })?;
    match operation {
        TeletextColumnOperation::G1 { mosaic } => {
            record.glyph = Some(TeletextGlyphIdentity::G1Mosaic { mosaic });
        }
        TeletextColumnOperation::G3 { code, .. } => {
            record.glyph = Some(TeletextGlyphIdentity::G3 { code });
            // Table 48 note 2: explicitly unassigned and should not transmit.
            if matches!(code.get(), 0x6e | 0x6f | 0x7e | 0x7f) {
                prohibit(&mut record);
            }
        }
        TeletextColumnOperation::Drcs { reference } => {
            record.glyph = Some(TeletextGlyphIdentity::Drcs { reference });
        }
        _ => resolve_selected(charset, &mut record, operation, pairs)?,
    }
    Ok(record)
}

fn resolve_selected(
    charset: &TeletextCharsetExecution,
    record: &mut TeletextExecutedGlyphIntent,
    operation: TeletextColumnOperation,
    pairs: &[Option<TeletextCharsetPair>],
) -> Result<(), TeletextGlyphIntentError> {
    // Table 29 explicitly requires @ for mode 10000/code 2A regardless of
    // current G0. Retain original pending context without inventing a dependence.
    if matches!(operation, TeletextColumnOperation::G0Diacritic { code, diacritic }
        if code.get() == 0x2a && diacritic.get() == 0)
    {
        record.glyph = Some(TeletextGlyphIdentity::AtSymbol);
        return Ok(());
    }
    let Some(pair) = resolve_pair(charset, record, pairs)? else {
        return Ok(());
    };
    record.selected_pair = Some(pair);
    let (g0, g2) = repertoires(pair);
    match operation {
        TeletextColumnOperation::G0 { code } => {
            let glyph = TeletextG0Glyph {
                repertoire: g0,
                code: code.get(),
                national_subset: None,
            };
            record.glyph = Some(TeletextGlyphIdentity::G0 { glyph });
            assign_g0(record, glyph);
        }
        TeletextColumnOperation::G1Alpha { code } => {
            let glyph = TeletextG0Glyph {
                repertoire: g0,
                code: code.get(),
                national_subset: None,
            };
            record.glyph = Some(TeletextGlyphIdentity::G0 { glyph });
            assign_g0(record, glyph);
        }
        TeletextColumnOperation::G2 { code } => {
            let glyph = TeletextG2Glyph {
                repertoire: g2,
                code: code.get(),
            };
            record.glyph = Some(TeletextGlyphIdentity::G2 { glyph });
            assign_g2(record, glyph);
        }
        TeletextColumnOperation::G0Diacritic { code, diacritic } => {
            let base = TeletextG0Glyph {
                repertoire: g0,
                code: code.get(),
                national_subset: None,
            };
            if diacritic.get() == 0 {
                record.glyph = Some(TeletextGlyphIdentity::G0 { glyph: base });
                assign_g0(record, base);
            } else {
                let mark = TeletextG2Glyph {
                    repertoire: g2,
                    code: 0x40 | diacritic.get(),
                };
                record.glyph = Some(TeletextGlyphIdentity::Composite { base, mark });
                assign_composite(record, base, mark);
            }
        }
        _ => {
            return Err(lookup_error(
                charset,
                Some(&record.context),
                TeletextGlyphIntentFailure::InconsistentContext,
            ))
        }
    }
    Ok(())
}

fn resolve_pair(
    charset: &TeletextCharsetExecution,
    record: &mut TeletextExecutedGlyphIntent,
    pairs: &[Option<TeletextCharsetPair>],
) -> Result<Option<TeletextCharsetPair>, TeletextGlyphIntentError> {
    let inconsistent = || {
        lookup_error(
            charset,
            Some(&record.context),
            TeletextGlyphIntentFailure::InconsistentContext,
        )
    };
    let selection = record.context.selection().ok_or_else(inconsistent)?;
    if let Some(pending) = selection.pending() {
        let pair = *pairs.get(pending).ok_or_else(inconsistent)?;
        if pair.is_none() {
            record.disposition = TeletextGlyphDisposition::Pending;
            record.reason = Some(TeletextGlyphIntentReason::CharsetOrder { pending });
        }
        Ok(pair)
    } else {
        Ok(Some(selection.value().ok_or_else(inconsistent)?.pair()))
    }
}

fn assign_g0(record: &mut TeletextExecutedGlyphIntent, glyph: TeletextG0Glyph) {
    // Table 44 has no printed glyph or explicit prohibition at 7/E. A blank
    // font cell or decoder replacement is not assignment/SPACE evidence.
    if glyph.repertoire == TeletextG0Repertoire::Arabic && glyph.code == 0x7e {
        pending(record, TeletextGlyphIntentReason::UnmarkedTableCell);
    }
}

fn assign_g2(record: &mut TeletextExecutedGlyphIntent, glyph: TeletextG2Glyph) {
    // Audited literal table notes, independent of decoder fonts/Unicode maps.
    match (glyph.repertoire, glyph.code) {
        (TeletextG2Repertoire::Latin, 0x59 | 0x5a | 0x5b | 0x65)
        | (TeletextG2Repertoire::Cyrillic, 0x26 | 0x28) => prohibit(record),
        (
            TeletextG2Repertoire::Latin
            | TeletextG2Repertoire::Cyrillic
            | TeletextG2Repertoire::Greek,
            0x40,
        )
        | (TeletextG2Repertoire::Arabic, 0x7f) => {
            pending(record, TeletextGlyphIntentReason::UnmarkedTableCell);
        }
        _ => {}
    }
}

fn assign_composite(
    record: &mut TeletextExecutedGlyphIntent,
    base: TeletextG0Glyph,
    mark: TeletextG2Glyph,
) {
    // Table 29 composes an alphabetic G0 with a current-G2 column-4 mark.
    // Arabic G2 column 4 contains letters, not diacritical marks. E.10 is advice
    // about decoder support, not a permanent Latin-only authoring restriction.
    match alphabetic(base) {
        Some(false) => {
            record.disposition = TeletextGlyphDisposition::Unassigned;
            record.reason = Some(TeletextGlyphIntentReason::NonAlphabeticBase);
        }
        Some(true)
            if matches!(
                mark.repertoire,
                TeletextG2Repertoire::Latin
                    | TeletextG2Repertoire::Cyrillic
                    | TeletextG2Repertoire::Greek
            ) => {}
        // Arabic G2 letters are not established diacritical marks. Assignment
        // is separate from the decoder's chosen twelve-by-ten font policy.
        _ => pending(record, TeletextGlyphIntentReason::UnverifiedComposition),
    }
}

pub(super) fn alphabetic(base: TeletextG0Glyph) -> Option<bool> {
    Some(match base.repertoire {
        TeletextG0Repertoire::Latin => matches!(base.code, 0x41..=0x5a | 0x61..=0x7a),
        TeletextG0Repertoire::Cyrillic1 => matches!(base.code, 0x40..=0x7e),
        TeletextG0Repertoire::Cyrillic2 | TeletextG0Repertoire::Cyrillic3 => {
            matches!(base.code, 0x26 | 0x40..=0x7e)
        }
        TeletextG0Repertoire::Greek => matches!(base.code, 0x40..=0x51 | 0x53..=0x7e),
        TeletextG0Repertoire::Hebrew => matches!(base.code, 0x41..=0x5a | 0x60..=0x7a),
        TeletextG0Repertoire::Arabic => return None,
    })
}

fn pending(record: &mut TeletextExecutedGlyphIntent, reason: TeletextGlyphIntentReason) {
    record.disposition = TeletextGlyphDisposition::Pending;
    record.reason = Some(reason);
}
fn prohibit(record: &mut TeletextExecutedGlyphIntent) {
    record.disposition = TeletextGlyphDisposition::ShouldNotTransmit;
    record.reason = Some(TeletextGlyphIntentReason::ProhibitedTablePosition);
}

const fn repertoires(pair: TeletextCharsetPair) -> (TeletextG0Repertoire, TeletextG2Repertoire) {
    match pair {
        TeletextCharsetPair::LatinLatin => {
            (TeletextG0Repertoire::Latin, TeletextG2Repertoire::Latin)
        }
        TeletextCharsetPair::Cyrillic1Cyrillic => (
            TeletextG0Repertoire::Cyrillic1,
            TeletextG2Repertoire::Cyrillic,
        ),
        TeletextCharsetPair::Cyrillic2Cyrillic => (
            TeletextG0Repertoire::Cyrillic2,
            TeletextG2Repertoire::Cyrillic,
        ),
        TeletextCharsetPair::Cyrillic3Cyrillic => (
            TeletextG0Repertoire::Cyrillic3,
            TeletextG2Repertoire::Cyrillic,
        ),
        TeletextCharsetPair::GreekGreek => {
            (TeletextG0Repertoire::Greek, TeletextG2Repertoire::Greek)
        }
        TeletextCharsetPair::LatinArabic => {
            (TeletextG0Repertoire::Latin, TeletextG2Repertoire::Arabic)
        }
        TeletextCharsetPair::ArabicArabic => {
            (TeletextG0Repertoire::Arabic, TeletextG2Repertoire::Arabic)
        }
        TeletextCharsetPair::HebrewArabic => {
            (TeletextG0Repertoire::Hebrew, TeletextG2Repertoire::Arabic)
        }
    }
}

const fn is_national_code(code: u8) -> bool {
    matches!(code, 0x23 | 0x24 | 0x40 | 0x5b..=0x60 | 0x7b..=0x7e)
}
const fn national_subset(code: u8) -> Option<TeletextNationalSubset> {
    Some(match code {
        0x00 | 0x10 | 0x40 => TeletextNationalSubset::English,
        0x01 | 0x09 | 0x11 | 0x21 => TeletextNationalSubset::German,
        0x02 | 0x0a | 0x12 => TeletextNationalSubset::SwedishFinnishHungarian,
        0x03 | 0x0b | 0x13 => TeletextNationalSubset::Italian,
        0x04 | 0x0c | 0x14 | 0x44 => TeletextNationalSubset::French,
        0x05 | 0x15 => TeletextNationalSubset::PortugueseSpanish,
        0x06 | 0x0e | 0x26 => TeletextNationalSubset::CzechSlovak,
        0x08 => TeletextNationalSubset::Polish,
        0x16 | 0x36 => TeletextNationalSubset::Turkish,
        0x1d => TeletextNationalSubset::SerbianCroatianSlovenian,
        0x1f => TeletextNationalSubset::Romanian,
        0x22 => TeletextNationalSubset::Estonian,
        0x23 => TeletextNationalSubset::LatvianLithuanian,
        _ => return None,
    })
}

fn lookup_error(
    charset: &TeletextCharsetExecution,
    context: Option<&TeletextExecutedGlyphCharset>,
    reason: TeletextGlyphIntentFailure,
) -> TeletextGlyphIntentError {
    TeletextGlyphIntentError {
        owner: charset.owner(),
        position: context.map(|value| value.candidate().position()),
        location: context.and_then(|value| value.candidate().source()),
        root_source: context.and_then(|value| value.candidate().root_source()),
        invocation: context.and_then(|value| value.candidate().invocation()),
        reason,
    }
}
