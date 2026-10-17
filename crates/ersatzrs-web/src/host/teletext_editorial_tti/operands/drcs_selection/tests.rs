//! Literal replay/state oracles; sealed catalogue/marker authentication stays in `model::build`.
use super::*;
use ersatzrs_domain::{TeletextPageNumber, TeletextSubcode};
use std::time::Duration;
fn location() -> Location {
    Location {
        page: 124,
        subcode: 32,
        resource: None,
    }
}
fn owner() -> TeletextPageAddress {
    TeletextPageAddress {
        page: TeletextPageNumber::new(124).unwrap(),
        subcode: TeletextSubcode::new(32).unwrap(),
    }
}
fn deadline() -> Instant {
    Instant::now() + Duration::from_secs(30)
}
fn sel(scope: usize, s1: u8) -> u32 {
    0x28 | (24 << 6) | ((16 | u32::from(scope == 1) << 6 | u32::from(s1)) << 11)
}
fn cell(scope: usize, slot: u8) -> u32 {
    8 | (13 << 6) | ((u32::from(scope == 1) << 6 | u32::from(slot)) << 11)
}
fn glyph_ref(scope: usize, s1: u8, slot: u8) -> TeletextDrcsGlyphReference {
    reference(scope, s1, TeletextDrcsGlyphSlot::new(slot).unwrap()).unwrap()
}
fn packets(words: &[u32]) -> PacketWords {
    let mut result = [None; 16];
    let mut body = [0x3f | (31 << 6) | (7 << 11); 13];
    body[..words.len()].copy_from_slice(words);
    result[0] = Some(body);
    result
}
fn bind(model: &mut Model, word: usize, command: usize, scope: usize, s1: u8, slot: u8) {
    model.references[word] = Some(OriginalGlyph {
        key: Key {
            object: None,
            command,
        },
        column: 8,
        reference: glyph_ref(scope, s1, slot),
        word,
    });
}
fn compare(model: &Model, a: &PacketWords, b: &PacketWords) -> Result<Replay> {
    replay::compare(model, a, b, location(), deadline())
}
#[test]
fn shared_selector_derives_two_refs_even_when_both_glyph_words_are_unchanged() {
    let mut model = Model::default();
    model.selectors[0] = Some(1);
    bind(&mut model, 1, 0, 1, 0, 0);
    bind(&mut model, 2, 1, 1, 0, 1);
    let before = packets(&[sel(1, 0), cell(1, 0), cell(1, 1)]);
    let after = packets(&[sel(1, 3), cell(1, 0), cell(1, 1)]);
    let decoded = compare(&model, &before, &after).unwrap();
    assert_eq!(decoded.references[1], Some(glyph_ref(1, 3, 0)));
    assert_eq!(decoded.references[2], Some(glyph_ref(1, 3, 1)));
    assert_eq!(decoded.allowed[0], Some(sel(1, 3)));
    assert!(decoded.allowed[1].is_none() && decoded.allowed[2].is_none());
}
#[test]
fn normal_global_and_definition_latches_are_independent_and_never_inherited() {
    let mut model = Model::default();
    model.selectors[0] = Some(1);
    model.selectors[2] = Some(3);
    model.selectors[6] = Some(7);
    model.boundaries[5] = true;
    bind(&mut model, 1, 0, 1, 0, 0);
    bind(&mut model, 3, 1, 0, 2, 0);
    bind(&mut model, 4, 2, 1, 0, 1);
    bind(&mut model, 7, 3, 1, 2, 0);
    let marker = 0x28 | (21 << 6);
    let before = packets(&[
        sel(1, 0),
        cell(1, 0),
        sel(0, 2),
        cell(0, 0),
        cell(1, 1),
        marker,
        sel(1, 2),
        cell(1, 0),
    ]);
    let after = packets(&[
        sel(1, 1),
        cell(1, 0),
        sel(0, 2),
        cell(0, 0),
        cell(1, 1),
        marker,
        sel(1, 2),
        cell(1, 0),
    ]);
    let decoded = compare(&model, &before, &after).unwrap();
    assert_eq!(decoded.references[3], Some(glyph_ref(0, 2, 0)));
    assert_eq!(decoded.references[4], Some(glyph_ref(1, 1, 1)));
    assert_eq!(decoded.references[7], Some(glyph_ref(1, 2, 0)));
    let mut missing = before;
    missing[0].as_mut().unwrap()[6] = 0x29 | (4 << 6);
    assert!(compare(&model, &missing, &missing).is_err());
}
#[test]
fn scope_edits_use_the_destination_latch_and_refuse_unknown_or_unowned_consumers() {
    let mut model = Model::default();
    model.selectors[0] = Some(1);
    model.selectors[2] = Some(3);
    bind(&mut model, 1, 0, 1, 0, 0);
    bind(&mut model, 3, 1, 0, 2, 0);
    let before = packets(&[sel(1, 0), cell(1, 0), sel(0, 2), cell(0, 0)]);
    let after = packets(&[sel(1, 0), cell(1, 0), sel(0, 2), cell(1, 1)]);
    let decoded = compare(&model, &before, &after).unwrap();
    assert_eq!(decoded.references[3], Some(glyph_ref(1, 0, 1)));
    assert!(compare(
        &model,
        &before,
        &packets(&[sel(1, 0), cell(0, 0), sel(0, 2), cell(0, 0)])
    )
    .is_err());
    model.references[3] = None; // A generated/unbound glyph grants no original intent.
    assert!(compare(&model, &before, &after).is_err());
}
#[test]
fn unowned_dangling_reserved_and_address_selector_edits_are_refused() {
    let mut model = Model::default();
    model.selectors[0] = Some(1);
    bind(&mut model, 1, 0, 1, 0, 0);
    let before = packets(&[sel(1, 0), cell(1, 0)]);
    for bad in [sel(1, 1) ^ 1, sel(1, 1) ^ (1 << 15), sel(1, 1) | (1 << 18)] {
        assert!(compare(&model, &before, &packets(&[bad, cell(1, 0)])).is_err());
    }
    model.selectors[0] = None;
    assert!(compare(&model, &before, &packets(&[sel(1, 1), cell(1, 0)])).is_err());
    // A bundled scope edit must not launder an unused changed selector.
    let mut bundled = Model::default();
    bundled.selectors[0] = Some(1);
    bundled.selectors[2] = Some(3);
    bind(&mut bundled, 1, 0, 0, 0, 0);
    bind(&mut bundled, 3, 1, 1, 0, 0);
    assert!(compare(
        &bundled,
        &packets(&[sel(0, 0), cell(0, 0), sel(1, 0), cell(1, 0)]),
        &packets(&[sel(0, 0), cell(0, 0), sel(1, 1), cell(0, 0)])
    )
    .is_err());
    // Updating a latch but consuming no changed original reference is not an edit.
    let mut model = Model::default();
    model.selectors[0] = Some(2);
    model.selectors[1] = Some(2);
    bind(&mut model, 2, 0, 1, 0, 0);
    assert!(compare(
        &model,
        &packets(&[sel(1, 1), sel(1, 0), cell(1, 0)]),
        &packets(&[sel(1, 2), sel(1, 0), cell(1, 0)])
    )
    .is_err());
}
#[test]
fn canonical_selector_omission_is_checked_by_source_and_full_subtable_reference() {
    let mut old = Model::default();
    old.selectors[0] = Some(1);
    old.selectors[2] = Some(3);
    bind(&mut old, 1, 0, 1, 0, 0);
    bind(&mut old, 3, 1, 1, 1, 0);
    let before = packets(&[sel(1, 0), cell(1, 0), sel(1, 1), cell(1, 0)]);
    let uploaded = packets(&[sel(1, 1), cell(1, 0), sel(1, 1), cell(1, 0)]);
    let input = compare(&old, &before, &uploaded).unwrap();
    let mut new = Model::default();
    new.selectors[0] = Some(1);
    bind(&mut new, 1, 0, 1, 1, 0);
    bind(&mut new, 2, 1, 1, 1, 0);
    let canonical = packets(&[sel(1, 1), cell(1, 0), cell(1, 0)]);
    verify_final_references(&old, &input, &new, &canonical, location(), deadline()).unwrap();
    // Same final mode13 words cannot conceal an incorrect canonical S1.
    bind(&mut new, 1, 0, 1, 2, 0);
    bind(&mut new, 2, 1, 1, 2, 0);
    assert!(verify_final_references(
        &old,
        &input,
        &new,
        &packets(&[sel(1, 2), cell(1, 0), cell(1, 0)]),
        location(),
        deadline()
    )
    .is_err());
}
#[test]
fn unchanged_later_ref_can_gain_a_canonical_selector_after_scope_change() {
    let mut old = Model::default();
    old.selectors[0] = Some(1);
    old.selectors[2] = Some(3);
    bind(&mut old, 1, 0, 1, 0, 0);
    bind(&mut old, 3, 1, 0, 1, 0);
    bind(&mut old, 4, 2, 0, 1, 1);
    let before = packets(&[sel(1, 0), cell(1, 0), sel(0, 1), cell(0, 0), cell(0, 1)]);
    let uploaded = packets(&[sel(1, 0), cell(1, 0), sel(0, 1), cell(1, 0), cell(0, 1)]);
    let input = compare(&old, &before, &uploaded).unwrap();
    let mut new = Model::default();
    new.selectors[0] = Some(1);
    new.selectors[3] = Some(4);
    bind(&mut new, 1, 0, 1, 0, 0);
    bind(&mut new, 2, 1, 1, 0, 0);
    bind(&mut new, 4, 2, 0, 1, 1);
    let canonical = packets(&[sel(1, 0), cell(1, 0), cell(1, 0), sel(0, 1), cell(0, 1)]);
    verify_final_references(&old, &input, &new, &canonical, location(), deadline()).unwrap();
}
#[test]
fn partial_or_duplicate_propagated_intents_cannot_authorize_a_complete_upload() {
    let change = |command| {
        TeletextLocalProgramOperandOverlay::new(
            owner(),
            command,
            8,
            TeletextColumnOperation::Drcs {
                reference: glyph_ref(1, 0, 0),
            },
            TeletextColumnOperation::Drcs {
                reference: glyph_ref(1, 1, 0),
            },
        )
        .unwrap()
    };
    let expected = TeletextEditorialChanges {
        resource_operands: Vec::new(),
        operands: vec![change(0), change(1)],
        ..TeletextEditorialChanges::default()
    };
    exact_changes(
        &expected,
        (&expected.operands, &[]),
        owner(),
        location(),
        deadline(),
    )
    .unwrap();
    assert!(exact_changes(
        &expected,
        (&expected.operands[..1], &[]),
        owner(),
        location(),
        deadline()
    )
    .is_err());
    let duplicates = [change(0), change(0)];
    assert!(exact_changes(
        &expected,
        (&duplicates, &[]),
        owner(),
        location(),
        deadline()
    )
    .is_err());
}
