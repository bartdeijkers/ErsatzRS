use super::super::{alignment, raw, Location};
use super::{decode, kind};
use ersatzrs_domain::{
    TeletextColumnOperation, TeletextEnhancementCommand, TeletextEnhancementPosition,
    TeletextGlyphCode,
};
use std::time::{Duration, Instant};

fn word(column: u32, mode: u32, data: u32) -> u32 {
    column | (mode << 6) | (data << 11)
}
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
fn glyph(column: u8, code: u8) -> TeletextEnhancementCommand {
    TeletextEnhancementCommand::AtColumn {
        column,
        operation: TeletextColumnOperation::G0 {
            code: TeletextGlyphCode::new(code).unwrap(),
        },
    }
}
fn read(words: &[u32]) -> raw::Body {
    raw::Body::read(words, location(), deadline()).unwrap()
}

#[test]
fn literal_style_fields_preserve_all_checked_flags_and_refuse_reserved_bits() {
    for (mode, expected) in [(0, 0), (3, 3), (12, 12)] {
        assert_eq!(kind(&decode(word(39, mode, 0)).unwrap()), Some(expected));
    }
    assert_eq!(
        decode(word(8, 12, 0x77)),
        Some(TeletextEnhancementCommand::AtColumn {
            column: 8,
            operation: TeletextColumnOperation::Attributes {
                double_height: true,
                double_width: true,
                boxed: true,
                conceal: true,
                invert: true,
                underline_or_separated_mosaics: true,
            },
        })
    );
    for data in 0..128 {
        assert_eq!(decode(word(8, 12, data)).is_some(), data & 8 == 0);
        assert_eq!(decode(word(8, 0, data)).is_some(), data < 32);
        assert_eq!(decode(word(8, 3, data)).is_some(), data < 32);
    }
    for invalid in [word(40, 0, 0), word(8, 7, 0), word(8, 8, 0), 0x40000] {
        assert!(decode(invalid).is_none());
    }
    // Style decoding does not change the old glyph/selector raw classification.
    let body = read(&[word(8, 0, 1), word(8, 12, 0x77)]);
    assert!(body.entries.iter().all(|entry| entry.command.is_none()));
    assert!(body
        .entries
        .iter()
        .all(|entry| entry.editable(false).is_none()));
    assert!(body
        .entries
        .iter()
        .all(|entry| entry.editable(true).is_some()));
}

#[test]
fn unchanged_style_family_column_skeleton_keeps_legacy_scalar_reconciliation() {
    let original = read(&[word(8, 0, 1), word(8, 9, 65), word(9, 12, 0), word(9, 3, 2)]);
    let scalar = read(&[
        word(8, 0, 31),
        word(9, 9, 66),
        word(9, 12, 0x77),
        word(9, 3, 0),
    ]);
    assert!(!raw::style_topology_changed(&original, &scalar));
    // A scalar-shaped family or column mutation must keep its existing
    // refusal path rather than be reinterpreted as structural delete+insert.
    for scalar_shape in [
        read(&[word(8, 3, 1), word(8, 9, 65), word(9, 12, 0), word(9, 3, 2)]),
        read(&[word(9, 0, 1), word(9, 9, 65), word(9, 12, 0), word(9, 3, 2)]),
    ] {
        assert!(!raw::style_topology_changed(&original, &scalar_shape));
    }
    for changed in [
        read(&[word(8, 0, 1), word(8, 9, 65), word(9, 3, 2)]),
        read(&[
            word(8, 0, 1),
            word(8, 9, 65),
            word(9, 12, 0),
            word(9, 3, 2),
            word(10, 0, 1),
        ]),
    ] {
        assert!(raw::style_topology_changed(&original, &changed));
    }
}

#[test]
fn style_structure_retains_only_exact_styles_and_original_glyph_survivors() {
    let (foreground, background, replacement) = (
        decode(word(8, 0, 1)).unwrap(),
        decode(word(9, 3, 0)).unwrap(),
        decode(word(9, 0, 2)).unwrap(),
    );
    let (a, b) = (glyph(8, b'A'), glyph(9, b'B'));
    let old = [(0, foreground), (1, a), (2, background), (3, b)];
    let aligned = alignment::with_styles(
        &old,
        &[foreground, a, replacement, b],
        location(),
        deadline(),
    )
    .unwrap();
    assert_eq!(aligned.commands, [foreground, a, replacement, b]);
    assert_eq!(aligned.retained, [Some(0), Some(1), None, Some(3)]);
    let duplicate = alignment::with_styles(
        &[(0, foreground), (1, foreground), (2, a)],
        &[foreground, a],
        location(),
        deadline(),
    )
    .unwrap();
    assert_eq!(duplicate.retained, [Some(1), Some(2)]);
    assert!(alignment::with_anchors(&old, &[foreground, a, b], location(), deadline()).is_err());
}

#[test]
fn style_structure_keeps_hidden_positions_mandatory_and_all_bounds_checked() {
    let position = |column| TeletextEnhancementCommand::SetPosition {
        position: TeletextEnhancementPosition::new(0, column).unwrap(),
    };
    let (p8, p39) = (position(8), position(39));
    let (foreground, attributes) = (
        decode(word(8, 0, 1)).unwrap(),
        decode(word(39, 12, 0x77)).unwrap(),
    );
    let (a, b) = (glyph(8, b'A'), glyph(39, b'B'));
    let old = [(0, p8), (1, foreground), (2, a), (3, p39), (4, b)];
    let actual = alignment::with_styles(
        &old,
        &[foreground, a, attributes, b],
        location(),
        deadline(),
    )
    .unwrap();
    assert_eq!(actual.commands, [p8, foreground, a, p39, attributes, b]);
    assert_eq!(
        actual.retained,
        [Some(0), Some(1), Some(2), Some(3), None, Some(4)]
    );
    let at_capacity = vec![foreground; 208];
    assert!(alignment::with_styles(&[], &at_capacity, location(), deadline()).is_ok());
    assert!(alignment::with_styles(&[], &[foreground; 209], location(), deadline()).is_err());
    assert!(alignment::with_styles(&[(0, p8)], &at_capacity, location(), deadline()).is_err());
    assert!(alignment::with_styles(&[], &[], location(), Instant::now()).is_err());
    let physical_position = TeletextEnhancementCommand::SetPosition {
        position: TeletextEnhancementPosition::new(1, 8).unwrap(),
    };
    assert!(
        alignment::with_styles(&[(0, physical_position)], &[], location(), deadline()).is_err()
    );
}
