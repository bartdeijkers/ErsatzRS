//! Synthetic semantic programs, without object resolution or transport claims.

use serde_json::{json, Value};

use super::{
    TeletextColourReference, TeletextColumnOperation, TeletextEnhancementCommand,
    TeletextEnhancementPosition, TeletextEnhancementProgram, TeletextEnhancementProgramError,
    TeletextG1AlphaCode,
};

fn operation_fixtures() -> Vec<Value> {
    vec![
        json!({"kind": "foreground", "colour": 31}),
        json!({"kind": "background", "colour": 8}),
        json!({"kind": "designate_charset", "designation": 55}),
        json!({"kind": "g0", "code": 32}),
        json!({"kind": "g1", "mosaic": 37}),
        json!({"kind": "g2", "code": 127}),
        json!({"kind": "g3", "code": 81, "compatibility": "level15"}),
        json!({"kind": "g0_diacritic", "code": 65, "diacritic": 15}),
        json!({"kind": "drcs", "reference": {"table": "normal", "subtable": 15, "slot": 47}}),
        json!({"kind": "attributes", "double_height": true, "double_width": false,
            "boxed": true, "conceal": false, "invert": true,
            "underline_or_separated_mosaics": false}),
        json!({"kind": "flash", "mode": "adjacent_clut", "rate": "fast2_hz_decremental"}),
    ]
}

fn operation_program(operation: &Value) -> Value {
    json!({"commands": [{"kind": "at_column", "column": 7, "operation": operation}]})
}

#[test]
fn rust_construction_preserves_order_repetition_and_relative_origin() {
    let foreground = TeletextEnhancementCommand::AtColumn {
        column: 3,
        operation: TeletextColumnOperation::Foreground {
            colour: TeletextColourReference::new(31).unwrap(),
        },
    };
    let commands = vec![
        TeletextEnhancementCommand::SetScreenColour {
            colour: TeletextColourReference::new(8).unwrap(),
        },
        TeletextEnhancementCommand::SetPosition {
            position: TeletextEnhancementPosition::new(0, 0).unwrap(),
        },
        foreground,
        TeletextEnhancementCommand::SetRowColour {
            row: 24,
            colour: TeletextColourReference::new(15).unwrap(),
            following_rows: true,
        },
        foreground,
    ];
    let program = TeletextEnhancementProgram::new(commands.clone()).unwrap();
    assert_eq!(program.commands(), commands);
}

#[test]
fn all_column_operations_roundtrip_without_flattening_or_defaulting() {
    for fixture in operation_fixtures() {
        let raw = operation_program(&fixture);
        let program: TeletextEnhancementProgram = serde_json::from_value(raw.clone()).unwrap();
        assert_eq!(program.commands().len(), 1);
        assert_eq!(serde_json::to_value(program).unwrap(), raw);
    }
    let raw = json!({"commands": [
        {"kind": "set_position", "position": {"row": 0, "column": 0}},
        {"kind": "set_screen_colour", "colour": 0},
        {"kind": "set_row_colour", "row": 24, "colour": 31, "following_rows": false},
        {"kind": "invoke_object", "target": {"scope": "local",
            "owner": {"page": 200, "subcode": 16}, "object_id": 65535},
            "object_type": "passive", "offset": {"row": 23, "column": 71}}
    ]});
    let program: TeletextEnhancementProgram = serde_json::from_value(raw.clone()).unwrap();
    assert_eq!(program.commands().len(), 4);
    assert_eq!(serde_json::to_value(program).unwrap(), raw);
}

#[test]
fn constructor_reports_first_invalid_command_with_exact_ordinal_and_coordinate() {
    let valid = TeletextEnhancementCommand::SetPosition {
        position: TeletextEnhancementPosition::new(0, 0).unwrap(),
    };
    for row in 25..=255 {
        let invalid = TeletextEnhancementCommand::SetRowColour {
            row,
            colour: TeletextColourReference::new(0).unwrap(),
            following_rows: false,
        };
        assert_eq!(
            TeletextEnhancementProgram::new(vec![valid, invalid]),
            Err(TeletextEnhancementProgramError::Row { ordinal: 1, row })
        );
    }
    for column in 40..=255 {
        let invalid = TeletextEnhancementCommand::AtColumn {
            column,
            operation: TeletextColumnOperation::Background {
                colour: TeletextColourReference::new(0).unwrap(),
            },
        };
        assert_eq!(
            TeletextEnhancementProgram::new(vec![valid, valid, invalid]),
            Err(TeletextEnhancementProgramError::Column { ordinal: 2, column })
        );
    }
}

#[test]
fn program_json_rechecks_raw_row_and_column_fields() {
    for row in [0, 24] {
        let raw = json!({"commands": [{"kind": "set_row_colour", "row": row,
            "colour": 0, "following_rows": true}]});
        assert!(serde_json::from_value::<TeletextEnhancementProgram>(raw).is_ok());
    }
    for row in [25, 128, 255] {
        let raw = json!({"commands": [{"kind": "set_row_colour", "row": row,
            "colour": 0, "following_rows": true}]});
        let error = serde_json::from_value::<TeletextEnhancementProgram>(raw)
            .unwrap_err()
            .to_string();
        assert!(error.contains("command 0"));
        assert!(error.contains(&format!("row {row}")));
    }
    for column in [0, 39, 40, 128, 255] {
        let raw = json!({"commands": [{"kind": "at_column", "column": column,
            "operation": {"kind": "foreground", "colour": 0}}]});
        assert_eq!(
            serde_json::from_value::<TeletextEnhancementProgram>(raw).is_ok(),
            column <= 39
        );
    }
}

#[test]
fn nested_scalar_checks_cannot_be_bypassed_by_the_program_parser() {
    for operation in [
        json!({"kind": "foreground", "colour": 32}),
        json!({"kind": "background", "colour": 255}),
        json!({"kind": "designate_charset", "designation": 39}),
        json!({"kind": "g0", "code": 31}),
        json!({"kind": "g1", "mosaic": 64}),
        json!({"kind": "g2", "code": 128}),
        json!({"kind": "g3", "code": 255, "compatibility": "level25"}),
        json!({"kind": "g0_diacritic", "code": 65, "diacritic": 16}),
        json!({"kind": "drcs", "reference": {"table": "normal", "subtable": 16, "slot": 0}}),
        json!({"kind": "drcs", "reference": {"table": "global", "subtable": 0, "slot": 48}}),
    ] {
        assert!(
            serde_json::from_value::<TeletextEnhancementProgram>(operation_program(&operation))
                .is_err()
        );
    }
    for position in [
        json!({"row": 25, "column": 0}),
        json!({"row": 0, "column": 40}),
        json!([0, 0]),
    ] {
        let raw = json!({"commands": [{"kind": "set_position", "position": position}]});
        assert!(serde_json::from_value::<TeletextEnhancementProgram>(raw).is_err());
    }
}

#[test]
fn closed_program_and_nested_tags_reject_missing_extra_duplicate_and_raw_commands() {
    for raw in [
        "null",
        "[]",
        "{}",
        "[[]]",
        r#"{"commands":null}"#,
        r#"{"commands":[],"triplets":[]}"#,
        r#"{"commands":[],"commands":[]}"#,
        r#"{"commands":[{"kind":"termination_marker"}]}"#,
        r#"{"commands":[{"kind":"set_screen_colour","colour":0,"raw":1}]}"#,
        r#"{"commands":[{"kind":"set_row_colour","row":0,"colour":0}]}"#,
        r#"{"commands":[{"kind":"set_row_colour","row":0,"colour":0,"following_rows":null}]}"#,
        r#"{"commands":[{"kind":"at_column","column":0}]}"#,
        r#"{"commands":[{"kind":"at_column","column":-1,"operation":{"kind":"g0","code":32}}]}"#,
        r#"{"commands":[{"kind":"at_column","column":0.5,"operation":{"kind":"g0","code":32}}]}"#,
        r#"{"commands":[{"kind":"at_column","column":0,"operation":{"kind":"g0","code":32,"code":33}}]}"#,
        r#"{"commands":[{"kind":"at_column","column":0,"operation":{"kind":"g0","kind":"g2","code":32}}]}"#,
        r#"{"commands":[{"kind":"at_column","column":0,"operation":{"kind":"font_style","bold":true}}]}"#,
    ] {
        assert!(
            serde_json::from_str::<TeletextEnhancementProgram>(raw).is_err(),
            "{raw}"
        );
    }
}

fn invocation_fixture() -> Value {
    json!({"commands": [{"kind": "invoke_object", "target": {
        "scope": "public", "resource": {"kind": "pop",
            "page": {"magazine": 2, "page": 160}, "logical_subpage": 15},
        "object_id": 0}, "object_type": "adaptive", "offset": null}]})
}

#[test]
fn invocation_preserves_explicit_absence_and_rejects_invalid_target_or_offset() {
    let raw = invocation_fixture();
    let program: TeletextEnhancementProgram = serde_json::from_value(raw.clone()).unwrap();
    assert_eq!(serde_json::to_value(program).unwrap(), raw);
    let mut missing = raw.clone();
    missing["commands"][0]
        .as_object_mut()
        .unwrap()
        .remove("offset");
    assert!(serde_json::from_value::<TeletextEnhancementProgram>(missing).is_err());
    for offset in [
        json!({"row": 24, "column": 0}),
        json!({"row": 0, "column": 72}),
        json!([0, 0]),
    ] {
        let mut invalid = raw.clone();
        invalid["commands"][0]["offset"] = offset;
        assert!(serde_json::from_value::<TeletextEnhancementProgram>(invalid).is_err());
    }
    let mut wrong_kind = raw;
    wrong_kind["commands"][0]["target"]["resource"]["kind"] = json!("gpop");
    assert!(serde_json::from_value::<TeletextEnhancementProgram>(wrong_kind).is_err());
}

#[test]
fn all_twenty_four_assigned_flash_choices_preserve_distinct_mode_and_rate() {
    // Independently enumerated EN 300 706 table 29 choices; reserved rates 6/7 absent.
    for mode in ["steady", "normal", "inverted", "adjacent_clut"] {
        for rate in [
            "slow1_hz",
            "fast2_hz_phase1",
            "fast2_hz_phase2",
            "fast2_hz_phase3",
            "fast2_hz_incremental",
            "fast2_hz_decremental",
        ] {
            let raw = operation_program(&json!({"kind": "flash", "mode": mode, "rate": rate}));
            let program: TeletextEnhancementProgram = serde_json::from_value(raw.clone()).unwrap();
            assert_eq!(serde_json::to_value(program).unwrap(), raw);
        }
    }
}

#[test]
fn flash_rejects_reserved_choices_and_missing_explicit_semantics() {
    for operation in [
        json!({"kind": "flash", "mode": "normal", "rate": "reserved6"}),
        json!({"kind": "flash", "mode": "normal", "rate": "fast2_hz_phase4"}),
        json!({"kind": "flash", "mode": "normal", "rate": 6}),
        json!({"kind": "flash", "mode": "unknown", "rate": "slow1_hz"}),
        json!({"kind": "flash", "mode": "normal"}),
        json!({"kind": "flash", "rate": "slow1_hz"}),
    ] {
        assert!(
            serde_json::from_value::<TeletextEnhancementProgram>(operation_program(&operation))
                .is_err()
        );
    }
}

#[test]
fn g3_compatibility_is_explicit_and_preserved_without_decoder_assurance() {
    for compatibility in ["level15", "level25"] {
        let raw =
            operation_program(&json!({"kind": "g3", "code": 81, "compatibility": compatibility}));
        let program: TeletextEnhancementProgram = serde_json::from_value(raw.clone()).unwrap();
        assert_eq!(serde_json::to_value(program).unwrap(), raw);
    }
    for operation in [
        json!({"kind": "g3", "code": 81}),
        json!({"kind": "g3", "code": 81, "compatibility": "level35"}),
    ] {
        assert!(
            serde_json::from_value::<TeletextEnhancementProgram>(operation_program(&operation))
                .is_err()
        );
    }
}

#[test]
fn every_display_attribute_is_required_and_independently_preserved() {
    let attributes = json!({"kind": "attributes", "double_height": false,
        "double_width": false, "boxed": false, "conceal": false, "invert": false,
        "underline_or_separated_mosaics": false});
    for field in [
        "double_height",
        "double_width",
        "boxed",
        "conceal",
        "invert",
        "underline_or_separated_mosaics",
    ] {
        let mut enabled = attributes.clone();
        enabled[field] = json!(true);
        let raw = operation_program(&enabled);
        let program: TeletextEnhancementProgram = serde_json::from_value(raw.clone()).unwrap();
        assert_eq!(serde_json::to_value(program).unwrap(), raw);
        let mut missing = attributes.clone();
        missing.as_object_mut().unwrap().remove(field);
        assert!(
            serde_json::from_value::<TeletextEnhancementProgram>(operation_program(&missing))
                .is_err()
        );
    }
}

#[test]
fn empty_and_large_programs_leave_resource_budget_to_the_caller() {
    let empty: TeletextEnhancementProgram = serde_json::from_str(r#"{"commands":[]}"#).unwrap();
    assert!(empty.commands().is_empty());
    let command = TeletextEnhancementCommand::SetPosition {
        position: TeletextEnhancementPosition::new(0, 0).unwrap(),
    };
    let program = TeletextEnhancementProgram::new(vec![command; 2048]).unwrap();
    assert_eq!(program.commands().len(), 2048);
}

#[test]
fn g1_alpha_program_preserves_distinct_closed_operation_and_checked_code() {
    let raw = operation_program(&json!({"kind": "g1_alpha", "code": 95}));
    let program: TeletextEnhancementProgram = serde_json::from_value(raw.clone()).unwrap();
    assert_eq!(
        program.commands(),
        &[TeletextEnhancementCommand::AtColumn {
            column: 7,
            operation: TeletextColumnOperation::G1Alpha {
                code: TeletextG1AlphaCode::new(0x5f).unwrap()
            },
        }]
    );
    assert_eq!(serde_json::to_value(program).unwrap(), raw);
    for invalid in [
        json!({"kind": "g1_alpha", "code": 63}),
        json!({"kind": "g1_alpha", "code": 96}),
        json!({"kind": "g1_alpha", "code": 192}),
        json!({"kind": "g1_alpha", "code": [64]}),
        json!({"kind": "g1_alpha", "code": 64, "mosaic": 0}),
        json!({"kind": "g1_alpha", "mosaic": 0}),
    ] {
        assert!(
            serde_json::from_value::<TeletextEnhancementProgram>(operation_program(&invalid))
                .is_err()
        );
    }
}
