//! Exact repertoire agreement preserves unresolved aliases and complete history.

use super::*;

fn local(id: u16) -> TeletextObjectIdentity {
    TeletextObjectIdentity::local(owner(), TeletextObjectId::new(id))
}
fn active(id: u16, codes: &[u8]) -> TeletextEnhancementObjectDefinition {
    TeletextEnhancementObjectDefinition {
        identity: local(id),
        object_type: TeletextObjectType::Active,
        program: TeletextEnhancementProgram::new(
            codes
                .iter()
                .map(|&code| {
                    at(
                        0,
                        TeletextColumnOperation::DesignateCharset {
                            designation: TeletextEnhancementCharsetDesignation::new(code).unwrap(),
                        },
                    )
                })
                .collect(),
        )
        .unwrap(),
    }
}
fn invoke(id: u16) -> TeletextEnhancementCommand {
    TeletextEnhancementCommand::InvokeObject {
        target: local(id),
        object_type: TeletextObjectType::Active,
        offset: None,
    }
}
fn glyph(column: u8) -> TeletextEnhancementCommand {
    at(
        column,
        TeletextColumnOperation::G0 {
            code: TeletextGlyphCode::new(b'A').unwrap(),
        },
    )
}
fn record(result: &TeletextGlyphIntent, command: usize) -> &TeletextExecutedGlyphIntent {
    result
        .records()
        .iter()
        .find(|record| {
            record.context().candidate().source()
                == Some(TeletextEnhancementSource {
                    object: None,
                    command,
                })
        })
        .unwrap()
}
fn assert_latin(record: &TeletextExecutedGlyphIntent) {
    assert_eq!(
        record.selected_pair(),
        Some(TeletextCharsetPair::LatinLatin)
    );
    assert_eq!(record.disposition(), TeletextGlyphDisposition::Assigned);
    assert_eq!(record.reason(), None);
    let glyph = g0_identity(record);
    assert_eq!(glyph.repertoire(), TeletextG0Repertoire::Latin);
    assert_eq!(glyph.code(), b'A');
    assert_eq!(glyph.national_subset(), None);
    let raw = record.context().selection().unwrap();
    assert_eq!(raw.value(), None);
    assert_eq!(raw.declaration(), None);
    assert!(raw.pending().is_some());
}
fn assert_order_pending(record: &TeletextExecutedGlyphIntent) {
    let pending = record.context().selection().unwrap().pending().unwrap();
    assert_eq!(record.selected_pair(), None);
    assert_eq!(record.glyph(), None);
    assert_eq!(record.disposition(), TeletextGlyphDisposition::Pending);
    assert_eq!(
        record.reason(),
        Some(TeletextGlyphIntentReason::CharsetOrder { pending })
    );
}

#[test]
fn raw_aliases_at_same_point_resolve_same_cell_and_later_glyph_without_a_winner() {
    let original = context(
        0x37,
        vec![set(1, 8), designate(0), invoke(1), glyph(8), glyph(9)],
        vec![active(1, &[1])],
    );
    let saved = original.clone();
    let result = resolve_teletext_glyph_intent(&original, original.records().len()).unwrap();
    assert_latin(record(&result, 3));
    assert_latin(record(&result, 4));
    for command in [3, 4] {
        let pending = record(&result, command)
            .context()
            .selection()
            .unwrap()
            .pending()
            .unwrap();
        let aliases: Vec<_> = original
            .pending_contenders(pending)
            .map(|declaration| declaration.designation().get())
            .collect();
        assert_eq!(aliases, [0, 1]);
    }
    assert_eq!(original.primary().get(), 0x37);
    for (record, raw) in result.records().iter().zip(original.records()) {
        assert_eq!(record.context(), raw);
    }
    assert_eq!(original, saved);
    let error = resolve_teletext_glyph_intent(&original, original.records().len() - 1).unwrap_err();
    assert_eq!(
        error.reason,
        TeletextGlyphIntentFailure::CandidateBudget {
            count: original.records().len(),
            maximum: original.records().len() - 1
        }
    );
    assert_eq!(original, saved);
}

#[test]
fn same_cell_keeps_prior_context_and_excludes_own_future_designation() {
    let original = context(
        0x37,
        vec![set(1, 8), invoke(1), glyph(8), designate(1), glyph(9)],
        vec![active(1, &[0])],
    );
    let result = intent(&original);
    assert_order_pending(record(&result, 2));
    assert_latin(record(&result, 4));
    let pending = record(&result, 2)
        .context()
        .selection()
        .unwrap()
        .pending()
        .unwrap();
    assert_eq!(
        original
            .pending_contenders(pending)
            .map(|declaration| declaration.designation().get())
            .collect::<Vec<_>>(),
        [0]
    );
    assert_eq!(
        original.pending()[pending]
            .previous()
            .value()
            .unwrap()
            .pair(),
        TeletextCharsetPair::GreekGreek
    );
    assert!(original.declarations().iter().any(|declaration| declaration
        .source()
        .object
        .is_none()
        && declaration.source().command == 3
        && declaration.designation().get() == 1));
}

#[test]
fn earlier_pending_disagreement_survives_a_later_latin_contender() {
    let original = context(
        0,
        vec![
            set(1, 5),
            at(
                5,
                TeletextColumnOperation::DesignateCharset {
                    designation: TeletextEnhancementCharsetDesignation::new(0x37).unwrap(),
                },
            ),
            invoke(1),
            set(1, 10),
            invoke(2),
            glyph(10),
            at(
                10,
                TeletextColumnOperation::DesignateCharset {
                    designation: TeletextEnhancementCharsetDesignation::new(0).unwrap(),
                },
            ),
            glyph(11),
        ],
        vec![active(1, &[0]), active(2, &[1])],
    );
    let saved = original.clone();
    let result = intent(&original);
    assert_order_pending(record(&result, 5));
    assert_latin(record(&result, 7));
    let current = record(&result, 5)
        .context()
        .selection()
        .unwrap()
        .pending()
        .unwrap();
    let earlier = original.pending()[current].previous().pending().unwrap();
    assert!(earlier < current);
    assert_eq!(original.pending()[earlier].position(), (1, 5));
    assert_eq!(
        original
            .pending_contenders(earlier)
            .map(|declaration| declaration.designation().get())
            .collect::<Vec<_>>(),
        [0x37, 0]
    );
    assert_eq!(
        original
            .pending_contenders(current)
            .map(|declaration| declaration.designation().get())
            .collect::<Vec<_>>(),
        [1]
    );
    assert_eq!(original, saved);
}

#[test]
fn prior_pending_alias_agreement_resolves_same_cell_while_future_disagreement_remains() {
    let original = context(
        0x37,
        vec![
            set(1, 5),
            at(
                5,
                TeletextColumnOperation::DesignateCharset {
                    designation: TeletextEnhancementCharsetDesignation::new(0).unwrap(),
                },
            ),
            invoke(1),
            set(1, 10),
            invoke(2),
            glyph(10),
            at(
                10,
                TeletextColumnOperation::DesignateCharset {
                    designation: TeletextEnhancementCharsetDesignation::new(0x37).unwrap(),
                },
            ),
            glyph(11),
        ],
        vec![active(1, &[1]), active(2, &[0])],
    );
    let result = intent(&original);
    assert_latin(record(&result, 5));
    assert_order_pending(record(&result, 7));
    let current = record(&result, 5)
        .context()
        .selection()
        .unwrap()
        .pending()
        .unwrap();
    let earlier = original.pending()[current].previous().pending().unwrap();
    assert_eq!(
        original
            .pending_contenders(earlier)
            .map(|declaration| declaration.designation().get())
            .collect::<Vec<_>>(),
        [0, 1]
    );
    assert_eq!(
        original
            .pending_contenders(current)
            .map(|declaration| declaration.designation().get())
            .collect::<Vec<_>>(),
        [0]
    );
}

#[test]
fn all_final_sources_are_retained_even_when_last_two_pairs_agree() {
    let original = context(
        0,
        vec![set(1, 8), designate(0x37), invoke(1), invoke(2), glyph(9)],
        vec![active(1, &[0]), active(2, &[1])],
    );
    let result = intent(&original);
    assert_order_pending(record(&result, 4));
    let pending = record(&result, 4)
        .context()
        .selection()
        .unwrap()
        .pending()
        .unwrap();
    assert_eq!(
        original
            .pending_contenders(pending)
            .map(|declaration| declaration.designation().get())
            .collect::<Vec<_>>(),
        [0x37, 0, 1]
    );
}

#[test]
fn different_pairs_do_not_become_consensus_merely_through_equal_g0() {
    let original = context(
        0,
        vec![set(1, 8), designate(0), invoke(1), glyph(9)],
        vec![active(1, &[0x40])],
    );
    let result = intent(&original);
    assert_order_pending(record(&result, 3));
    let pending = record(&result, 3)
        .context()
        .selection()
        .unwrap()
        .pending()
        .unwrap();
    assert_eq!(
        original
            .pending_contenders(pending)
            .map(|declaration| declaration.designation().pair())
            .collect::<Vec<_>>(),
        [
            TeletextCharsetPair::LatinLatin,
            TeletextCharsetPair::LatinArabic
        ]
    );
}

#[test]
fn consensus_preserves_table_assignment_and_composite_rules() {
    let original = context(
        0,
        vec![
            set(1, 8),
            designate(0),
            invoke(1),
            at(
                9,
                TeletextColumnOperation::G1Alpha {
                    code: TeletextG1AlphaCode::new(b'A').unwrap(),
                },
            ),
            at(
                10,
                TeletextColumnOperation::G2 {
                    code: TeletextGlyphCode::new(0x59).unwrap(),
                },
            ),
            at(
                11,
                TeletextColumnOperation::G0Diacritic {
                    code: TeletextGlyphCode::new(b'A').unwrap(),
                    diacritic: TeletextDiacriticIndex::new(1).unwrap(),
                },
            ),
            at(
                12,
                TeletextColumnOperation::G0Diacritic {
                    code: TeletextGlyphCode::new(0x2a).unwrap(),
                    diacritic: TeletextDiacriticIndex::new(0).unwrap(),
                },
            ),
        ],
        vec![active(1, &[1])],
    );
    let result = intent(&original);
    assert_latin(record(&result, 3));
    let prohibited = record(&result, 4);
    assert_eq!(
        prohibited.selected_pair(),
        Some(TeletextCharsetPair::LatinLatin)
    );
    assert_eq!(
        prohibited.disposition(),
        TeletextGlyphDisposition::ShouldNotTransmit
    );
    assert_eq!(
        prohibited.reason(),
        Some(TeletextGlyphIntentReason::ProhibitedTablePosition)
    );
    assert!(
        matches!(prohibited.glyph(), Some(TeletextGlyphIdentity::G2 { glyph }) if glyph.repertoire() == TeletextG2Repertoire::Latin && glyph.code() == 0x59)
    );
    let composite = record(&result, 5);
    assert_eq!(
        composite.selected_pair(),
        Some(TeletextCharsetPair::LatinLatin)
    );
    assert_eq!(composite.disposition(), TeletextGlyphDisposition::Assigned);
    assert!(
        matches!(composite.glyph(), Some(TeletextGlyphIdentity::Composite { base, mark }) if base.repertoire() == TeletextG0Repertoire::Latin && base.code() == b'A' && mark.repertoire() == TeletextG2Repertoire::Latin && mark.code() == 0x41)
    );
    let special = record(&result, 6);
    assert_eq!(special.selected_pair(), None);
    assert_eq!(special.glyph(), Some(TeletextGlyphIdentity::AtSymbol));
    assert_eq!(special.disposition(), TeletextGlyphDisposition::Assigned);
}
