//! Hidden original identity positions remain mandatory source-lineage boundaries.
use super::{with_anchors, Location};
use ersatzrs_domain::{
    TeletextColumnOperation, TeletextEnhancementCommand, TeletextEnhancementPosition,
    TeletextGlyphCode, TeletextMosaic,
};
use std::time::{Duration, Instant};

fn glyph(column: u8, code: u8) -> TeletextEnhancementCommand {
    TeletextEnhancementCommand::AtColumn {
        column,
        operation: TeletextColumnOperation::G0 {
            code: TeletextGlyphCode::new(code).unwrap(),
        },
    }
}
fn mosaic(bits: u8) -> TeletextEnhancementCommand {
    TeletextEnhancementCommand::AtColumn {
        column: 1,
        operation: TeletextColumnOperation::G1 {
            mosaic: TeletextMosaic::new(bits).unwrap(),
        },
    }
}
fn identity() -> TeletextEnhancementCommand {
    TeletextEnhancementCommand::SetPosition {
        position: TeletextEnhancementPosition::new(0, 0).unwrap(),
    }
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
#[test]
fn hidden_identity_keeps_survivors_on_both_sides_of_an_unencoded_boundary() {
    let (a, b, q, zero) = (glyph(0, b'A'), glyph(0, b'B'), glyph(0, b'Q'), identity());
    let old = [(3, a), (4, zero), (5, b), (6, mosaic(1))];
    let new = [a, q, b, mosaic(1)];
    let aligned = with_anchors(&old, &new, location(), deadline()).unwrap();
    assert_eq!(aligned.commands, [a, zero, q, b, mosaic(1)]);
    assert_eq!(aligned.retained, [Some(3), Some(4), None, Some(5), Some(6)]);
}
#[test]
fn hidden_identity_and_duplicate_survivors_do_not_follow_a_column_one_suffix() {
    let (a, zero) = (glyph(0, b'A'), identity());
    let old = [(3, a), (4, zero), (5, a), (6, mosaic(1))];
    let new = [a, mosaic(2), mosaic(1), mosaic(7)];
    let aligned = with_anchors(&old, &new, location(), deadline()).unwrap();
    assert_eq!(aligned.commands, [zero, a, mosaic(2), mosaic(1), mosaic(7)]);
    assert_eq!(aligned.retained, [Some(4), Some(5), None, Some(6), None]);
}
#[test]
fn leading_trailing_and_repeated_identities_survive_even_without_glyphs() {
    let (a, b, q, zero) = (glyph(0, b'A'), glyph(0, b'B'), glyph(0, b'Q'), identity());
    let old = [(0, zero), (1, a), (2, zero), (3, zero), (4, b), (5, zero)];
    let aligned = with_anchors(&old, &[a, q, b], location(), deadline()).unwrap();
    assert_eq!(aligned.commands, [zero, a, zero, zero, q, b, zero]);
    assert_eq!(
        aligned.retained,
        [Some(0), Some(1), Some(2), Some(3), None, Some(4), Some(5)]
    );
    let aligned = with_anchors(&old, &[], location(), deadline()).unwrap();
    assert_eq!(aligned.commands, [zero; 4]);
    assert_eq!(aligned.retained, [Some(0), Some(2), Some(3), Some(5)]);
}
#[test]
fn mandatory_identity_uses_only_an_ap_zero_boundary_and_rejects_unknown_anchors() {
    let zero = identity();
    let old = [(0, glyph(0, b'A')), (1, zero), (2, glyph(0, b'B'))];
    let new = [glyph(1, b'A'), glyph(1, b'B')];
    let aligned = with_anchors(&old, &new, location(), deadline()).unwrap();
    assert_eq!(aligned.commands, [zero, new[0], new[1]]);
    assert_eq!(aligned.retained, [Some(1), None, Some(2)]);
    let unknown = TeletextEnhancementCommand::SetPosition {
        position: TeletextEnhancementPosition::new(1, 0).unwrap(),
    };
    assert!(with_anchors(&[(0, unknown)], &[], location(), deadline()).is_err());
}
#[test]
fn cluster_bounds_cover_inputs_rebuilt_commands_and_expired_empty_work() {
    let zero = identity();
    let a = glyph(0, b'A');
    assert!(with_anchors(&[(0, zero); 209], &[], location(), deadline()).is_err());
    assert!(with_anchors(&[], &[a; 209], location(), deadline()).is_err());
    assert!(with_anchors(&[(0, zero); 208], &[a], location(), deadline()).is_err());
    let at_limit = (0..208).map(|ordinal| (ordinal, zero)).collect::<Vec<_>>();
    let aligned = with_anchors(&at_limit, &[], location(), deadline()).unwrap();
    assert_eq!(aligned.commands.len(), 208);
    assert_eq!(aligned.retained, (0..208).map(Some).collect::<Vec<_>>());
    assert!(with_anchors(&[], &[], location(), Instant::now()).is_err());
}
