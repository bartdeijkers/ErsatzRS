//! V19 preserves original row intent without serializing transient Add authority.
use super::resource_object_addition_tests::addition;
use super::resource_object_operand_tests::identity;
use super::resource_object_program_tests::original_with_program;
use super::tests::{budget, bytes, provenance};
use super::{
    append_teletext_design_editorial_overlay, decode_teletext_design_documents,
    TeletextSavedEditorialOverlay,
};
use ersatzrs_domain::{
    TeletextEnhancementCommand, TeletextEnhancementPosition, TeletextEnhancementProgram,
    TeletextResourceObjectProgramOverlay,
};
use serde_json::{json, Value};

fn position(row: u8) -> TeletextEnhancementCommand {
    TeletextEnhancementCommand::SetPosition {
        position: TeletextEnhancementPosition::new(row, 0).unwrap(),
    }
}
fn glyph(column: u8, code: u8) -> TeletextEnhancementCommand {
    serde_json::from_value(
        json!({"kind":"at_column","column":column,"operation":{"kind":"g0","code":code}}),
    )
    .unwrap()
}
fn change(global: bool, s1: u8, transient: bool) -> TeletextResourceObjectProgramOverlay {
    let expected =
        TeletextEnhancementProgram::new(vec![position(1), glyph(8, 80), position(3), glyph(4, 81)])
            .unwrap();
    let replacement = TeletextEnhancementProgram::new(vec![
        position(1),
        glyph(8, 80),
        position(2),
        glyph(6, 82),
        position(3),
        glyph(4, 81),
    ])
    .unwrap();
    let constructor = if transient {
        TeletextResourceObjectProgramOverlay::new_with_inserted_positions
    } else {
        TeletextResourceObjectProgramOverlay::new_with_original_positions
    };
    constructor(
        identity(global, s1),
        expected,
        replacement,
        vec![Some(0), Some(1), None, None, Some(2), Some(3)],
        budget().validation.service,
    )
    .unwrap()
}
fn intent(change: TeletextResourceObjectProgramOverlay) -> TeletextSavedEditorialOverlay {
    TeletextSavedEditorialOverlay::new_with_resource_object_position_overlays(
        provenance(),
        vec![],
        vec![],
        vec![],
        vec![],
        vec![],
        (vec![], vec![change]),
    )
    .unwrap()
}
fn concept() -> Value {
    let change = change(false, 15, false);
    let original = original_with_program(change.identity(), change.expected(), false);
    serde_json::from_slice(
        append_teletext_design_editorial_overlay(&original, &intent(change), budget())
            .unwrap()
            .bytes(),
    )
    .unwrap()
}

#[test]
fn v19_original_positions_roundtrip_full_scope_s1_map_and_exact_source_bytes() {
    for global in [false, true] {
        for s1 in [0, 15] {
            let change = change(global, s1, false);
            let original = original_with_program(change.identity(), change.expected(), false);
            let overlay = intent(change.clone());
            assert_eq!(overlay.schema_version(), 19);
            let encoded =
                append_teletext_design_editorial_overlay(&original, &overlay, budget()).unwrap();
            let end = original
                .iter()
                .rposition(|byte| !byte.is_ascii_whitespace())
                .unwrap();
            assert_eq!(&encoded.bytes()[..end], &original[..end]);
            assert!(encoded.bytes().ends_with(&original[end..]));
            let restored = decode_teletext_design_documents(encoded.bytes(), budget()).unwrap();
            assert_eq!(restored.editorial_overlay(), Some(&overlay));
            let programs = restored
                .editorial_overlay()
                .unwrap()
                .resource_object_program_overlays();
            assert_eq!(programs, [change]);
            assert!(programs[0].is_original_position_intent());
            let before = decode_teletext_design_documents(&original, budget()).unwrap();
            for (after, before) in restored.sources().iter().zip(before.sources()) {
                assert_eq!(after.bytes(), before.bytes());
                assert_eq!(after.authoring(), before.authoring());
            }
        }
    }
}

#[test]
fn v19_original_positions_compose_additions_without_downgrading_or_overlap() {
    let change = change(false, 15, false);
    let original = original_with_program(change.identity(), change.expected(), false);
    let overlay = intent(change)
        .with_resource_object_definition_additions(vec![addition(false, 15)])
        .unwrap();
    assert_eq!(overlay.schema_version(), 19);
    let encoded = append_teletext_design_editorial_overlay(&original, &overlay, budget()).unwrap();
    assert_eq!(
        decode_teletext_design_documents(encoded.bytes(), budget())
            .unwrap()
            .editorial_overlay(),
        Some(&overlay)
    );
    assert!(overlay
        .with_resource_object_definition_additions(vec![addition(true, 0)])
        .is_err());
}

#[test]
fn v19_refuses_transient_add_positions_and_missing_original_position_intent() {
    let transient = change(false, 15, true);
    assert!(!transient.is_original_position_intent());
    assert!(
        TeletextSavedEditorialOverlay::new_with_resource_object_position_overlays(
            provenance(),
            vec![],
            vec![],
            vec![],
            vec![],
            vec![],
            (vec![], vec![transient])
        )
        .is_err()
    );
    let ordinary = TeletextResourceObjectProgramOverlay::new(
        identity(false, 15),
        TeletextEnhancementProgram::new(vec![glyph(1, 65)]).unwrap(),
        TeletextEnhancementProgram::new(vec![glyph(1, 65), glyph(2, 66)]).unwrap(),
        vec![Some(0), None],
        budget().validation.service,
    )
    .unwrap();
    assert!(
        TeletextSavedEditorialOverlay::new_with_resource_object_position_overlays(
            provenance(),
            vec![],
            vec![],
            vec![],
            vec![],
            vec![],
            (vec![], vec![ordinary])
        )
        .is_err()
    );
    for constructor in [
        TeletextSavedEditorialOverlay::new_with_resource_object_program_overlays,
        TeletextSavedEditorialOverlay::new_with_resource_object_invocation_overlays,
        TeletextSavedEditorialOverlay::new_with_resource_object_invocation_structure_overlays,
    ] {
        assert!(constructor(
            provenance(),
            vec![],
            vec![],
            vec![],
            vec![],
            vec![],
            (vec![], vec![change(false, 15, false)])
        )
        .is_err());
    }
    let overlay = TeletextSavedEditorialOverlay::new_with_resource_object_definition_additions(
        provenance(),
        vec![addition(false, 15)],
    )
    .unwrap();
    assert_eq!(overlay.schema_version(), 18);
}

#[test]
fn v19_storage_rejects_old_roles_tampered_map_identity_anchors_and_null_additions() {
    let value = concept();
    for version in 15..=18 {
        let mut changed = value.clone();
        changed["editorial_overlay"]["schema_version"] = json!(version);
        assert!(decode_teletext_design_documents(&bytes(&changed), budget()).is_err());
    }
    let path = &["editorial_overlay", "resource_object_program_overlays"];
    for mutation in 0..6 {
        let mut changed = value.clone();
        let program = &mut changed[path[0]][path[1]][0];
        match mutation {
            0 => program["retained_commands"][0] = Value::Null,
            1 => program["retained_commands"][4] = json!(0),
            2 => program["expected"]["commands"][0]["position"]["column"] = json!(1),
            3 => program["replacement"]["commands"][0]["position"]["column"] = json!(1),
            4 => program["identity"]["resource"]["logical_subpage"] = json!(0),
            _ => program["replacement"]["commands"][2]["position"]["row"] = json!(0),
        }
        assert!(decode_teletext_design_documents(&bytes(&changed), budget()).is_err());
    }
    for additions in [Value::Null, json!([])] {
        let mut changed = value.clone();
        changed["editorial_overlay"]["resource_object_definition_additions"] = additions;
        assert!(decode_teletext_design_documents(&bytes(&changed), budget()).is_err());
    }
}

#[test]
fn v19_association_and_complete_budget_remain_checked() {
    let change = change(false, 15, false);
    let original = original_with_program(change.identity(), change.expected(), false);
    let overlay = intent(change.clone());
    let wrong_original = original_with_program(
        change.identity(),
        &TeletextEnhancementProgram::new(vec![glyph(8, 80)]).unwrap(),
        false,
    );
    assert!(append_teletext_design_editorial_overlay(&wrong_original, &overlay, budget()).is_err());
    let mut bounded = budget();
    bounded.validation.service.max_commands = 5;
    assert!(append_teletext_design_editorial_overlay(&original, &overlay, bounded).is_err());
    assert!(
        TeletextSavedEditorialOverlay::new_with_resource_object_position_overlays(
            provenance(),
            vec![],
            vec![],
            vec![],
            vec![],
            vec![],
            (vec![], vec![change.clone(), change])
        )
        .is_err()
    );
}

#[test]
fn position_restoration_never_enters_saved_program_roles_even_without_inserted_ap() {
    let prior = change(false, 15, false);
    let restoration = TeletextResourceObjectProgramOverlay::restore_original_positions(
        &prior,
        budget().validation.service,
    )
    .unwrap();
    assert!(restoration.is_position_restoration());
    assert!(!restoration.has_inserted_positions());
    assert!(super::resource_object_programs::distinct_targets(
        std::slice::from_ref(&restoration),
        &[]
    )
    .is_err());
    for constructor in [
        TeletextSavedEditorialOverlay::new_with_resource_object_program_overlays,
        TeletextSavedEditorialOverlay::new_with_resource_object_invocation_overlays,
        TeletextSavedEditorialOverlay::new_with_resource_object_invocation_structure_overlays,
        TeletextSavedEditorialOverlay::new_with_resource_object_position_overlays,
    ] {
        assert!(constructor(
            provenance(),
            vec![],
            vec![],
            vec![],
            vec![],
            vec![],
            (vec![], vec![restoration.clone()])
        )
        .is_err());
    }
    assert!(
        TeletextSavedEditorialOverlay::new_with_resource_object_position_overlays(
            provenance(),
            vec![],
            vec![],
            vec![],
            vec![],
            vec![],
            (vec![], vec![change(true, 15, false), restoration]),
        )
        .is_err()
    );
}

#[test]
fn position_restoration_cannot_be_relabelled_by_a_stored_version_or_uploaded_map() {
    let prior = change(false, 15, false);
    let restoration = TeletextResourceObjectProgramOverlay::restore_original_positions(
        &prior,
        budget().validation.service,
    )
    .unwrap();
    let mut value = concept();
    let stored = &mut value["editorial_overlay"]["resource_object_program_overlays"][0];
    stored["expected"] = serde_json::to_value(restoration.expected()).unwrap();
    stored["replacement"] = serde_json::to_value(restoration.replacement()).unwrap();
    stored["retained_commands"] = serde_json::to_value(restoration.retained_commands()).unwrap();
    for version in 15..=19 {
        value["editorial_overlay"]["schema_version"] = json!(version);
        assert!(decode_teletext_design_documents(&bytes(&value), budget()).is_err());
    }
}
