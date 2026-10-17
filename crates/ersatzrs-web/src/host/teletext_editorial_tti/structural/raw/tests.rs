use super::{protect_fixed_families, qualify_selectors, Body, Location, OriginalSpan};
use ersatzrs_domain::{
    TeletextColumnOperation, TeletextDrcsTableScope, TeletextEnhancementCommand,
};
use std::time::{Duration, Instant};
fn location() -> Location {
    Location {
        page: 124,
        subcode: 32,
        resource: None,
    }
}
fn deadline() -> Instant {
    Instant::now() + Duration::from_secs(30)
}
fn word(column: u32, mode: u32, data: u32) -> u32 {
    column | (mode << 6) | (data << 11)
}
fn selector(normal: bool, subtable: u32) -> u32 {
    word(0x28, 24, 0x10 | (u32::from(normal) * 0x40) | subtable)
}
fn drcs(normal: bool, slot: u32) -> u32 {
    word(0, 13, (u32::from(normal) * 0x40) | slot)
}
fn read(words: &[u32]) -> Body {
    Body::read(words, location(), deadline()).unwrap()
}
fn references(body: &Body) -> Vec<(TeletextDrcsTableScope, u8, u8)> {
    body.entries
        .iter()
        .filter_map(|entry| match entry.command {
            Some(TeletextEnhancementCommand::AtColumn {
                operation: TeletextColumnOperation::Drcs { reference },
                ..
            }) => Some((
                reference.table,
                reference.subtable.get(),
                reference.slot.get(),
            )),
            _ => None,
        })
        .collect()
}
#[test]
fn shared_selection_crosses_attributes_and_independent_scopes_reset_at_definition() {
    let body = read(&[
        selector(true, 3),
        drcs(true, 0),
        word(0, 12, 0x10),
        selector(false, 5),
        drcs(false, 2),
        drcs(true, 1),
    ]);
    assert_eq!(
        references(&body),
        [
            (TeletextDrcsTableScope::Normal, 3, 0),
            (TeletextDrcsTableScope::Global, 5, 2),
            (TeletextDrcsTableScope::Normal, 3, 1)
        ]
    );
    assert!(body.selectors.iter().all(|selector| selector.used));
    assert!(Body::read(&[drcs(true, 0)], location(), deadline()).is_err());
    assert!(Body::read(&[word(0x29, 24, 0x50)], location(), deadline()).is_err());
    assert!(Body::read(&[word(0x28, 24, 0x40)], location(), deadline()).is_err());
}
#[test]
fn canonical_selector_compaction_preserves_complete_references() {
    let uploaded = read(&[
        selector(true, 3),
        drcs(true, 0),
        selector(true, 3),
        drcs(true, 1),
    ]);
    let canonical = read(&[selector(true, 3), drcs(true, 0), drcs(true, 1)]);
    assert_eq!(references(&uploaded), references(&canonical));
    assert!(uploaded.selectors.iter().all(|selector| selector.used));
    let other_scope = read(&[selector(false, 3), drcs(false, 0), drcs(false, 1)]);
    assert_ne!(references(&uploaded), references(&other_scope));
}
#[test]
fn orphan_original_prefix_requires_exact_order_span_and_deleted_all_consumers() {
    let before = read(&[selector(true, 0), drcs(true, 0), drcs(true, 1)]);
    let spans = [
        OriginalSpan {
            first: 0,
            word: 1,
            ordinal: 7,
        },
        OriginalSpan {
            first: 2,
            word: 2,
            ordinal: 8,
        },
    ];
    let orphan = read(&[selector(true, 0)]);
    assert!(qualify_selectors(&before, &orphan, &spans, &[], location(), deadline()).is_ok());
    assert!(
        qualify_selectors(&before, &orphan, &spans, &[Some(8)], location(), deadline()).is_err()
    );
    let survivor = read(&[selector(true, 0), drcs(true, 1)]);
    assert!(qualify_selectors(
        &before,
        &survivor,
        &spans,
        &[Some(8)],
        location(),
        deadline()
    )
    .is_ok());
    for bad in [
        read(&[selector(true, 1)]),
        read(&[selector(true, 0), selector(false, 0)]),
    ] {
        assert!(qualify_selectors(&before, &bad, &spans, &[], location(), deadline()).is_err());
    }
    let wrong_span = [OriginalSpan {
        first: 1,
        word: 1,
        ordinal: 7,
    }];
    assert!(qualify_selectors(&before, &orphan, &wrong_span, &[], location(), deadline()).is_err());
    let other_definition = read(&[selector(false, 0), drcs(false, 0)]);
    assert!(qualify_selectors(
        &other_definition,
        &orphan,
        &spans,
        &[],
        location(),
        deadline()
    )
    .is_err());
}
#[test]
fn fixed_topology_keeps_g3_compatibility_and_alpha_mosaic_boundaries_closed() {
    for (a, b) in [
        (word(0, 2, 0x21), word(0, 11, 0x21)),
        (word(0, 11, 0x21), word(0, 2, 0x21)),
        (word(0, 1, 0x41), word(0, 1, 0x21)),
        (word(0, 1, 0x21), word(0, 1, 0x41)),
    ] {
        let (before, after) = (read(&[a]), read(&[b]));
        assert!(protect_fixed_families(&before, &after, location()).is_err());
        let growth = read(&[a, b]);
        assert!(protect_fixed_families(&before, &growth, location()).is_ok());
        let moved = read(&[b | 1]);
        assert!(protect_fixed_families(&before, &moved, location()).is_ok());
    }
}
#[test]
fn raw_scalar_checks_do_not_admit_reserved_or_out_of_range_glyphs() {
    assert!(Body::read(&[selector(true, 0), drcs(true, 48)], location(), deadline()).is_err());
    assert!(Body::read(&[0x40000], location(), deadline()).is_err());
    assert!(Body::read(&[], location(), Instant::now()).is_err());
    assert!(Body::read(&[word(0, 9, 0x41); 209], location(), deadline()).is_err());
}

#[test]
fn checked_wide_families_decode_literal_table28_fields_without_aliases() {
    use ersatzrs_domain::{TeletextColumnGlyphFamily as Family, TeletextG3Compatibility};
    let words = [
        word(0, 1, 0x41),
        word(0, 15, 0x23),
        word(0, 2, 0x21),
        word(0, 11, 0x21),
        word(0, 18, 0x65),
    ];
    let expected = [
        Family::G1Alpha,
        Family::G2,
        Family::G3 {
            compatibility: TeletextG3Compatibility::Level15,
        },
        Family::G3 {
            compatibility: TeletextG3Compatibility::Level25,
        },
        Family::G0Diacritic,
    ];
    let body = read(&words);
    for (entry, expected) in body.entries.iter().zip(expected) {
        let Some(TeletextEnhancementCommand::AtColumn { operation, .. }) = entry.command else {
            panic!("expected literal assigned glyph operand");
        };
        assert_eq!(operation.glyph_family(), Some(expected));
    }
    assert!(
        matches!(body.entries[4].command, Some(TeletextEnhancementCommand::AtColumn { operation:TeletextColumnOperation::G0Diacritic {code,diacritic}, .. }) if code.get()==0x65 && diacritic.get()==2)
    );
}
