//! Synthetic relative values, without charset or resolved-display acceptance.

use serde::Deserialize;
use serde_json::json;

use super::{
    TeletextEnhancementGeometryError, TeletextEnhancementPosition, TeletextGlyphCode,
    TeletextObjectOffset,
};

#[test]
fn relative_positions_preserve_all_valid_coordinates_including_header_origin() {
    for row in 0..=24 {
        for column in 0..=39 {
            let position = TeletextEnhancementPosition::new(row, column).unwrap();
            assert_eq!((position.row(), position.column()), (row, column));
        }
    }
    for (row, column) in [(25, 0), (255, 39), (0, 40), (24, 255), (255, 255)] {
        assert_eq!(
            TeletextEnhancementPosition::new(row, column),
            Err(TeletextEnhancementGeometryError::Position { row, column })
        );
    }
    let origin: TeletextEnhancementPosition =
        serde_json::from_str(r#"{"row":0,"column":0}"#).unwrap();
    assert_eq!((origin.row(), origin.column()), (0, 0));
}

#[test]
fn object_offsets_preserve_all_valid_coordinates_without_origin_resolution() {
    for row in 0..=23 {
        for column in 0..=71 {
            let offset = TeletextObjectOffset::new(row, column).unwrap();
            assert_eq!((offset.row(), offset.column()), (row, column));
        }
    }
    for (row, column) in [(24, 0), (255, 71), (0, 72), (23, 255), (255, 255)] {
        assert_eq!(
            TeletextObjectOffset::new(row, column),
            Err(TeletextEnhancementGeometryError::ObjectOffset { row, column })
        );
    }
    // Both are representable relative inputs even though their sum needs later validation.
    assert!(TeletextEnhancementPosition::new(24, 39).is_ok());
    assert!(TeletextObjectOffset::new(23, 71).is_ok());
}

#[test]
fn glyph_codes_exhaustively_reject_controls_and_physical_high_bits() {
    for code in 0..=255 {
        let value = TeletextGlyphCode::new(code);
        let decoded = serde_json::from_value::<TeletextGlyphCode>(json!(code));
        if (0x20..=0x7f).contains(&code) {
            let glyph = value.unwrap();
            assert_eq!(glyph.get(), code);
            assert_eq!(u8::from(glyph), code);
            assert_eq!(decoded.unwrap(), glyph);
            assert_eq!(serde_json::to_value(glyph).unwrap(), json!(code));
        } else {
            assert_eq!(
                value,
                Err(TeletextEnhancementGeometryError::GlyphCode { code })
            );
            assert!(decoded.is_err());
        }
    }
    // This primitive preserves 0x7f as a glyph position, not a universal Unicode glyph.
    assert_eq!(TeletextGlyphCode::new(0x7f).unwrap().get(), 0x7f);
}

#[test]
fn glyph_json_requires_a_checked_integer_scalar() {
    for raw in [
        "-1", "256", "32.5", "null", "false", "\"32\"", "[]", "[32]", "{}",
    ] {
        assert!(
            serde_json::from_str::<TeletextGlyphCode>(raw).is_err(),
            "{raw}"
        );
    }
}

#[test]
fn coordinate_json_requires_closed_objects_with_explicit_integer_fields() {
    for raw in [
        r#"{"row":-1,"column":0}"#,
        r#"{"row":0,"column":-1}"#,
        r#"{"row":256,"column":0}"#,
        r#"{"row":0,"column":256}"#,
        r#"{"row":0.5,"column":0}"#,
        r#"{"row":0,"column":0.5}"#,
        r#"{"row":null,"column":0}"#,
        r#"{"row":0,"column":null}"#,
        r#"{"row":"0","column":0}"#,
        r#"{"row":false,"column":0}"#,
        r#"{"row":0}"#,
        r#"{"column":0}"#,
        r#"{"row":0,"column":0,"origin":0}"#,
        r#"{"row":0,"row":1,"column":0}"#,
        r#"{"row":0,"column":0,"column":1}"#,
        "{}",
        "null",
        "0",
        "[]",
        "[0,0]",
    ] {
        assert!(
            serde_json::from_str::<TeletextEnhancementPosition>(raw).is_err(),
            "{raw}"
        );
        assert!(
            serde_json::from_str::<TeletextObjectOffset>(raw).is_err(),
            "{raw}"
        );
    }
}

#[test]
fn coordinate_json_applies_each_distinct_boundary_and_preserves_asymmetry() {
    let position_raw = r#"{"row":7,"column":31}"#;
    let position: TeletextEnhancementPosition = serde_json::from_str(position_raw).unwrap();
    assert_eq!((position.row(), position.column()), (7, 31));
    assert_eq!(serde_json::to_string(&position).unwrap(), position_raw);
    let offset_raw = r#"{"row":23,"column":70}"#;
    let offset: TeletextObjectOffset = serde_json::from_str(offset_raw).unwrap();
    assert_eq!((offset.row(), offset.column()), (23, 70));
    assert_eq!(serde_json::to_string(&offset).unwrap(), offset_raw);

    for (row, column) in [(24, 39), (0, 0)] {
        assert!(serde_json::from_value::<TeletextEnhancementPosition>(
            json!({"row": row, "column": column})
        )
        .is_ok());
    }
    for (row, column) in [(25, 0), (0, 40), (128, 0), (0, 128)] {
        assert!(serde_json::from_value::<TeletextEnhancementPosition>(
            json!({"row": row, "column": column})
        )
        .is_err());
    }
    for (row, column) in [(23, 71), (0, 0)] {
        assert!(serde_json::from_value::<TeletextObjectOffset>(
            json!({"row": row, "column": column})
        )
        .is_ok());
    }
    for (row, column) in [(24, 0), (0, 72), (128, 0), (0, 128)] {
        assert!(serde_json::from_value::<TeletextObjectOffset>(
            json!({"row": row, "column": column})
        )
        .is_err());
    }
}

#[test]
fn nested_deserialization_cannot_bypass_geometry_or_glyph_checks() {
    #[derive(Deserialize)]
    struct Fixture {
        position: TeletextEnhancementPosition,
        offset: TeletextObjectOffset,
        glyph: TeletextGlyphCode,
    }

    let value = json!({
        "position": {"row": 24, "column": 39},
        "offset": {"row": 23, "column": 71},
        "glyph": 32
    });
    let fixture: Fixture = serde_json::from_value(value.clone()).unwrap();
    assert_eq!(
        (fixture.position.row(), fixture.position.column()),
        (24, 39)
    );
    assert_eq!((fixture.offset.row(), fixture.offset.column()), (23, 71));
    assert_eq!(fixture.glyph.get(), 32);
    for (field, bad_value) in [
        ("position", json!({"row": 25, "column": 39})),
        ("position", json!([24, 39])),
        ("offset", json!({"row": 24, "column": 71})),
        ("offset", json!([23, 71])),
        ("glyph", json!(0xa0)),
    ] {
        let mut invalid = value.clone();
        invalid[field] = bad_value;
        assert!(serde_json::from_value::<Fixture>(invalid).is_err());
    }
}

#[test]
fn geometry_errors_retain_numeric_positions_without_clipping() {
    assert_eq!(
        TeletextEnhancementPosition::new(25, 200)
            .unwrap_err()
            .to_string(),
        "teletext enhancement position row 25 column 200 requires row 0–24 and column 0–39"
    );
    assert_eq!(
        TeletextObjectOffset::new(128, 255).unwrap_err().to_string(),
        "teletext object offset row 128 column 255 requires row 0–23 and column 0–71"
    );
    assert_eq!(
        TeletextGlyphCode::new(0xa0).unwrap_err().to_string(),
        "teletext glyph code A0 must be between 20 and 7F hexadecimal"
    );
}
