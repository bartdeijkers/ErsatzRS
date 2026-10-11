//! Synthetic defaults/panel metadata, without packet or rendering acceptance.

use serde_json::{json, Value};

use super::{
    TeletextColourRemapping, TeletextLevel25PresentationDefaults, TeletextPageCharsetDesignation,
    TeletextPanelStatus, TeletextPanelWidth, TeletextPresentationValueError, TeletextSidePanels,
};

fn presentation() -> Value {
    json!({
        "primary": 85,
        "secondary": 68,
        "palette": {
            "clut2": [[1,2,3], [4,5,6], [7,8,9], [10,11,12],
                [13,14,15], [0,1,2], [3,4,5], [6,7,8]],
            "clut3": [[15,14,13], [12,11,10], [9,8,7], [6,5,4],
                [3,2,1], [0,15,0], [15,0,15], [2,8,14]]
        },
        "screen_colour": 31,
        "row_colour": 8,
        "black_background_substitution": true,
        "remapping": "fg_clut2_bg_clut3",
        "panels": {"left": true, "right": false,
            "status": "level25_and35", "width": 0}
    })
}

#[test]
fn panel_width_checks_every_scalar_without_masking() {
    for width in 0..=255 {
        let constructed = TeletextPanelWidth::new(width);
        let parsed = serde_json::from_value::<TeletextPanelWidth>(json!(width));
        if width <= 15 {
            let checked = constructed.unwrap();
            assert_eq!(checked.get(), width);
            assert_eq!(parsed.unwrap(), checked);
            assert_eq!(serde_json::to_value(checked).unwrap(), json!(width));
        } else {
            assert_eq!(
                constructed,
                Err(TeletextPresentationValueError::PanelWidth { width })
            );
            assert!(parsed.is_err());
        }
    }
    for raw in [json!(-1), json!(256), json!(1.5), json!(null), json!([0])] {
        assert!(serde_json::from_value::<TeletextPanelWidth>(raw).is_err());
    }
}

#[test]
fn all_panel_flags_widths_and_levels_preserve_raw_metadata() {
    for width in 0..=15 {
        let expected = [
            (false, false, (0, 0)),
            (true, false, (if width == 0 { 16 } else { width }, 0)),
            (false, true, (0, 16 - width)),
            (true, true, (width, 16 - width)),
        ];
        for (left, right, widths) in expected {
            for (status, name) in [
                (TeletextPanelStatus::Level35Only, "level35_only"),
                (TeletextPanelStatus::Level25And35, "level25_and35"),
            ] {
                let raw = json!({"left": left, "right": right,
                    "status": name, "width": width});
                let panels: TeletextSidePanels = serde_json::from_value(raw.clone()).unwrap();
                assert_eq!(panels.left, left);
                assert_eq!(panels.right, right);
                assert_eq!(panels.status, status);
                assert_eq!(panels.width.get(), width);
                assert_eq!(panels.level35_widths(), widths);
                assert_eq!(
                    panels.level25_widths(),
                    if status == TeletextPanelStatus::Level35Only {
                        (0, 0)
                    } else {
                        widths
                    }
                );
                assert_eq!(serde_json::to_value(panels).unwrap(), raw);
            }
        }
    }
}

#[test]
fn remapping_accepts_exactly_the_eight_encodable_table_pairs() {
    let cases = [
        (
            TeletextColourRemapping::FgClut0BgClut0,
            "fg_clut0_bg_clut0",
            (0, 0),
        ),
        (
            TeletextColourRemapping::FgClut0BgClut1,
            "fg_clut0_bg_clut1",
            (0, 1),
        ),
        (
            TeletextColourRemapping::FgClut0BgClut2,
            "fg_clut0_bg_clut2",
            (0, 2),
        ),
        (
            TeletextColourRemapping::FgClut1BgClut1,
            "fg_clut1_bg_clut1",
            (1, 1),
        ),
        (
            TeletextColourRemapping::FgClut1BgClut2,
            "fg_clut1_bg_clut2",
            (1, 2),
        ),
        (
            TeletextColourRemapping::FgClut2BgClut1,
            "fg_clut2_bg_clut1",
            (2, 1),
        ),
        (
            TeletextColourRemapping::FgClut2BgClut2,
            "fg_clut2_bg_clut2",
            (2, 2),
        ),
        (
            TeletextColourRemapping::FgClut2BgClut3,
            "fg_clut2_bg_clut3",
            (2, 3),
        ),
    ];
    for (remapping, name, pair) in cases {
        assert_eq!(remapping.cluts(), pair);
        assert_eq!(serde_json::to_value(remapping).unwrap(), json!(name));
        assert_eq!(
            serde_json::from_value::<TeletextColourRemapping>(json!(name)).unwrap(),
            remapping
        );
    }
    for raw in [
        json!("fg_clut1_bg_clut0"),
        json!("fg_clut3_bg_clut3"),
        json!(0),
        json!({"fg_clut0_bg_clut0": null}),
        json!(null),
    ] {
        assert!(serde_json::from_value::<TeletextColourRemapping>(raw).is_err());
    }
}

#[test]
fn complete_asymmetric_declaration_roundtrips_without_fallback() {
    let raw = presentation();
    let declaration: TeletextLevel25PresentationDefaults =
        serde_json::from_value(raw.clone()).unwrap();
    assert_eq!(declaration.primary.get(), 0x55);
    assert_eq!(declaration.secondary.unwrap().get(), 0x44);
    assert_eq!(declaration.palette.clut2[0].get(), [1, 2, 3]);
    assert_eq!(declaration.palette.clut3[7].get(), [2, 8, 14]);
    assert_eq!(declaration.screen_colour.get(), 31);
    assert_eq!(declaration.row_colour.get(), 8);
    assert!(declaration.black_background_substitution);
    assert_eq!(declaration.panels.level25_widths(), (16, 0));
    assert_eq!(serde_json::to_value(declaration).unwrap(), raw);
}

#[test]
fn explicit_no_second_g0_is_nullable_and_never_a_missing_declaration() {
    let mut raw = presentation();
    raw["secondary"] = Value::Null;
    let mut declaration: TeletextLevel25PresentationDefaults =
        serde_json::from_value(raw.clone()).unwrap();
    declaration.secondary = TeletextPageCharsetDesignation::secondary_from_code(0x7f).unwrap();
    assert!(declaration.secondary.is_none());
    assert_eq!(serde_json::to_value(declaration).unwrap(), raw);
    raw["secondary"] = json!(127);
    assert!(serde_json::from_value::<TeletextLevel25PresentationDefaults>(raw).is_err());
}

#[test]
fn every_presentation_and_panel_field_is_required_and_shapes_are_closed() {
    let base = presentation();
    for field in base.as_object().unwrap().keys() {
        let mut raw = base.clone();
        raw.as_object_mut().unwrap().remove(field);
        assert!(
            serde_json::from_value::<TeletextLevel25PresentationDefaults>(raw).is_err(),
            "{field}"
        );
    }
    for field in base["panels"].as_object().unwrap().keys() {
        let mut raw = base.clone();
        raw["panels"].as_object_mut().unwrap().remove(field);
        assert!(
            serde_json::from_value::<TeletextLevel25PresentationDefaults>(raw).is_err(),
            "{field}"
        );
    }
    for (location, name) in [
        ("", "page_function"),
        ("panels", "resolved_width"),
        ("palette", "clut0"),
    ] {
        let mut raw = base.clone();
        let target = if location.is_empty() {
            &mut raw
        } else {
            &mut raw[location]
        };
        target
            .as_object_mut()
            .unwrap()
            .insert(name.into(), json!(0));
        assert!(serde_json::from_value::<TeletextLevel25PresentationDefaults>(raw).is_err());
    }
    let panel_array = json!([true, false, "level25_and35", 0]);
    assert!(serde_json::from_value::<TeletextSidePanels>(panel_array.clone()).is_err());
    let mut raw = base.clone();
    raw["panels"] = panel_array;
    assert!(serde_json::from_value::<TeletextLevel25PresentationDefaults>(raw).is_err());
    let mut raw = base.clone();
    raw["palette"] = json!([base["palette"]["clut2"], base["palette"]["clut3"]]);
    assert!(serde_json::from_value::<TeletextLevel25PresentationDefaults>(raw).is_err());
    let array = json!([
        85,
        null,
        base["palette"],
        31,
        8,
        true,
        "fg_clut2_bg_clut3",
        base["panels"]
    ]);
    assert!(serde_json::from_value::<TeletextLevel25PresentationDefaults>(array).is_err());
}

#[test]
fn nested_bad_values_do_not_bypass_existing_checked_types() {
    let base = presentation();
    for (pointer, value) in [
        ("/primary", json!(127)),
        ("/primary", json!(7)),
        ("/secondary", json!(128)),
        ("/secondary", json!(7)),
        ("/screen_colour", json!(32)),
        ("/row_colour", json!(-1)),
        ("/panels/width", json!(16)),
        ("/panels/left", json!(1)),
        ("/panels/status", json!("level25_only")),
        ("/black_background_substitution", json!(null)),
        ("/palette/clut2/0/0", json!(16)),
        ("/palette/clut3/7", json!([1, 2])),
        ("/palette/clut2", json!([[1, 2, 3]])),
    ] {
        let mut raw = base.clone();
        *raw.pointer_mut(pointer).unwrap() = value;
        assert!(
            serde_json::from_value::<TeletextLevel25PresentationDefaults>(raw).is_err(),
            "{pointer}"
        );
    }
}

#[test]
fn duplicate_fields_are_rejected_at_each_metadata_object() {
    let fixture = presentation();
    let raw = serde_json::to_string(&fixture).unwrap();
    let duplicate_palette = format!("\"clut3\":{},\"clut2\":", fixture["palette"]["clut3"]);
    let duplicates = [
        raw.replacen("\"primary\":85", "\"primary\":85,\"primary\":85", 1),
        raw.replacen("\"secondary\":68", "\"secondary\":null,\"secondary\":68", 1),
        raw.replacen("\"left\":true", "\"left\":false,\"left\":true", 1),
        raw.replacen("\"clut2\":", &duplicate_palette, 1),
    ];
    for duplicate in duplicates {
        assert_ne!(duplicate, raw);
        assert!(serde_json::from_str::<TeletextLevel25PresentationDefaults>(&duplicate).is_err());
    }
}
