//! Synthetic authored-value fixtures; no transport or decoder acceptance claim.

use serde_json::{json, Value};

use super::{
    TeletextColourReference, TeletextDrcsBitmap, TeletextDrcsGlyphSlot, TeletextDrcsSubtableIndex,
    TeletextEnhancementValueError, TeletextLevel25Palette, TeletextRgb,
};

#[test]
fn colour_references_preserve_every_valid_value_and_reject_high_bits() {
    for value in 0..=31 {
        let reference = TeletextColourReference::new(value).unwrap();
        assert_eq!(reference.get(), value);
        assert_eq!(u8::from(reference), value);
        assert_eq!(serde_json::to_value(reference).unwrap(), json!(value));
        assert_eq!(
            serde_json::from_value::<TeletextColourReference>(json!(value)).unwrap(),
            reference
        );
    }
    for value in 32..=255 {
        assert_eq!(
            TeletextColourReference::new(value),
            Err(TeletextEnhancementValueError::ColourReference)
        );
        assert!(serde_json::from_value::<TeletextColourReference>(json!(value)).is_err());
    }
    for raw in ["-1", "256", "1.5", "null", "\"1\"", "[]", "{}"] {
        assert!(serde_json::from_str::<TeletextColourReference>(raw).is_err());
    }
}

#[test]
fn rgb_checks_each_component_without_scaling_or_masking() {
    for component in 0..3 {
        for value in 0..=15 {
            let mut components = [0; 3];
            components[component] = value;
            let colour = TeletextRgb::new(components).unwrap();
            assert_eq!(colour.get(), components);
            assert_eq!(<[u8; 3]>::from(colour), components);
        }
        for value in 16..=255 {
            let mut components = [0; 3];
            components[component] = value;
            assert_eq!(
                TeletextRgb::new(components),
                Err(TeletextEnhancementValueError::RgbComponent { component })
            );
        }
    }
    assert_eq!(
        TeletextRgb::new([15, 16, 255]).unwrap_err().to_string(),
        "teletext RGB component 1 must be between zero and 15"
    );
}

#[test]
fn rgb_json_requires_exactly_three_four_bit_integer_components() {
    let colour: TeletextRgb = serde_json::from_str("[15,0,7]").unwrap();
    assert_eq!(colour.get(), [15, 0, 7]);
    assert_eq!(serde_json::to_string(&colour).unwrap(), "[15,0,7]");
    for raw in [
        "[]",
        "[0,0]",
        "[0,0,0,0]",
        "[16,0,0]",
        "[0,16,0]",
        "[0,0,16]",
        "[0,-1,0]",
        "[0,256,0]",
        "[0,1.5,0]",
        "[0,null,0]",
        "[0,\"1\",0]",
        "{\"red\":0,\"green\":0,\"blue\":0}",
    ] {
        assert!(serde_json::from_str::<TeletextRgb>(raw).is_err(), "{raw}");
    }
}

fn palette_fixture() -> Value {
    json!({
        "clut2": [[15,0,5], [15,7,0], [0,15,7], [15,15,11],
                  [0,12,10], [5,0,0], [6,5,2], [12,7,7]],
        "clut3": [[3,3,3], [15,7,7], [7,15,7], [15,15,7],
                  [7,7,15], [15,7,15], [7,15,15], [13,13,13]]
    })
}

#[test]
fn palette_roundtrip_preserves_table_entry_and_component_order() {
    // Explicit synthetic declaration; this test does not expose a default palette API.
    let fixture = palette_fixture();
    let palette: TeletextLevel25Palette = serde_json::from_value(fixture.clone()).unwrap();
    assert_eq!(palette.clut2[0].get(), [15, 0, 5]);
    assert_eq!(palette.clut2[7].get(), [12, 7, 7]);
    assert_eq!(palette.clut3[0].get(), [3, 3, 3]);
    assert_eq!(palette.clut3[7].get(), [13, 13, 13]);
    assert_eq!(serde_json::to_value(palette).unwrap(), fixture);
}

#[test]
fn palette_rejects_fixed_table_edits_missing_tables_and_wrong_sizes() {
    for field in ["clut0", "clut1", "alpha"] {
        let mut fixture = palette_fixture();
        fixture[field] = json!(vec![[0, 0, 0]; 8]);
        let error = serde_json::from_value::<TeletextLevel25Palette>(fixture)
            .unwrap_err()
            .to_string();
        assert!(error.contains("unknown field"));
        assert!(error.contains(field));
    }
    for field in ["clut2", "clut3"] {
        let mut missing = palette_fixture();
        missing.as_object_mut().unwrap().remove(field);
        assert!(serde_json::from_value::<TeletextLevel25Palette>(missing).is_err());
        for count in [0, 7, 9] {
            let mut fixture = palette_fixture();
            fixture[field] = json!(vec![[0, 0, 0]; count]);
            assert!(serde_json::from_value::<TeletextLevel25Palette>(fixture).is_err());
        }
    }
}

#[test]
fn nested_palette_deserialization_cannot_bypass_rgb_validation() {
    for field in ["clut2", "clut3"] {
        for component in 0..3 {
            let mut fixture = palette_fixture();
            fixture[field][7][component] = json!(16);
            assert!(serde_json::from_value::<TeletextLevel25Palette>(fixture).is_err());
        }
    }
}

#[test]
fn drcs_slots_are_checked_in_rust_and_json_without_wrapping() {
    for value in 0..=47 {
        let slot = TeletextDrcsGlyphSlot::new(value).unwrap();
        assert_eq!(slot.get(), value);
        assert_eq!(u8::from(slot), value);
        assert_eq!(serde_json::to_value(slot).unwrap(), json!(value));
        assert_eq!(
            serde_json::from_value::<TeletextDrcsGlyphSlot>(json!(value)).unwrap(),
            slot
        );
    }
    for value in 48..=255 {
        assert_eq!(
            TeletextDrcsGlyphSlot::new(value),
            Err(TeletextEnhancementValueError::DrcsGlyphSlot)
        );
        assert!(serde_json::from_value::<TeletextDrcsGlyphSlot>(json!(value)).is_err());
    }
    for raw in ["-1", "256", "1.5", "null", "\"1\"", "[]", "{}"] {
        assert!(serde_json::from_str::<TeletextDrcsGlyphSlot>(raw).is_err());
    }
}

#[test]
fn drcs_subtable_indices_do_not_accept_physical_header_bits() {
    for value in 0..=15 {
        let index = TeletextDrcsSubtableIndex::new(value).unwrap();
        assert_eq!(index.get(), value);
        assert_eq!(u8::from(index), value);
        assert_eq!(serde_json::to_value(index).unwrap(), json!(value));
        assert_eq!(
            serde_json::from_value::<TeletextDrcsSubtableIndex>(json!(value)).unwrap(),
            index
        );
    }
    for value in 16..=255 {
        assert_eq!(
            TeletextDrcsSubtableIndex::new(value),
            Err(TeletextEnhancementValueError::DrcsSubtableIndex)
        );
        assert!(serde_json::from_value::<TeletextDrcsSubtableIndex>(json!(value)).is_err());
    }
    for raw in ["-1", "256", "1.5", "null", "\"1\"", "[]", "{}"] {
        assert!(serde_json::from_str::<TeletextDrcsSubtableIndex>(raw).is_err());
    }
}

#[test]
fn bitmap_checks_every_row_and_reports_first_overwide_row() {
    assert_eq!(TeletextDrcsBitmap::new([0; 10]).unwrap().get(), [0; 10]);
    assert_eq!(
        TeletextDrcsBitmap::new([0x0fff; 10]).unwrap().get(),
        [0x0fff; 10]
    );
    for row in 0..10 {
        for value in [0x1000, 0xffff] {
            let mut rows = [0; 10];
            rows[row] = value;
            assert_eq!(
                TeletextDrcsBitmap::new(rows),
                Err(TeletextEnhancementValueError::DrcsBitmapRow { row })
            );
        }
    }
    let mut rows = [0; 10];
    rows[2] = 0x1000;
    rows[7] = 0xffff;
    assert_eq!(
        TeletextDrcsBitmap::new(rows).unwrap_err().to_string(),
        "teletext DRCS bitmap row 2 must contain at most twelve pixel bits"
    );
}

#[test]
fn bitmap_roundtrip_preserves_asymmetric_top_left_and_bottom_right_geometry() {
    // A left-to-right diagonal: top-left bit 11 through row nine's bit 2.
    // The added bottom-right pixel distinguishes row order from bit order.
    let raw = "[2048,1024,512,256,128,64,32,16,8,5]";
    let bitmap: TeletextDrcsBitmap = serde_json::from_str(raw).unwrap();
    let expected = [
        0x800, 0x400, 0x200, 0x100, 0x080, 0x040, 0x020, 0x010, 0x008, 0x005,
    ];
    assert_eq!(bitmap.get(), expected);
    assert_eq!(<[u16; 10]>::from(bitmap), expected);
    assert_eq!(serde_json::to_string(&bitmap).unwrap(), raw);
}

#[test]
fn bitmap_json_rejects_wrong_geometry_multicolour_shapes_and_nonintegers() {
    for count in [0, 9, 11, 20] {
        assert!(serde_json::from_value::<TeletextDrcsBitmap>(json!(vec![0; count])).is_err());
    }
    for value in [
        json!(4096),
        json!(65536),
        json!(-1),
        json!(1.5),
        Value::Null,
        json!("1"),
    ] {
        let mut rows = json!(vec![0; 10]);
        rows[9] = value;
        assert!(serde_json::from_value::<TeletextDrcsBitmap>(rows).is_err());
    }
    // One row per colour plane or pixel-array nesting is not monochrome row data.
    assert!(serde_json::from_value::<TeletextDrcsBitmap>(json!(vec![vec![0; 12]; 10])).is_err());
    assert!(serde_json::from_value::<TeletextDrcsBitmap>(json!({
        "mode": "12x10x2", "rows": vec![0; 10]
    }))
    .is_err());
}
