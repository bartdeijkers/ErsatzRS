//! V14 authenticates resource scalars without changing earlier private roles.
use super::tests::{address, budget, bytes, original_with_context, provenance};
use super::{
    append_teletext_design_editorial_overlay, decode_teletext_design_documents,
    TeletextSavedEditorialOverlay,
};
use ersatzrs_domain::{
    TeletextColumnOperation, TeletextEnhancementCommand, TeletextEnhancementProgram,
    TeletextLocalObjectGlyphProgramOverlay, TeletextObjectId, TeletextObjectIdentity,
    TeletextRawDataPageNumber, TeletextResourceIdentity, TeletextResourceKind,
    TeletextResourceObjectOperandOverlay, TeletextResourceSubpageIndex,
};
use serde_json::{json, Value};

pub(super) fn identity(global: bool, subpage: u8) -> TeletextObjectIdentity {
    let resource = TeletextResourceIdentity::new(
        if global {
            TeletextResourceKind::Gpop
        } else {
            TeletextResourceKind::Pop
        },
        TeletextRawDataPageNumber::new(2, if global { 0xa1 } else { 0xa0 }).unwrap(),
        TeletextResourceSubpageIndex::new(subpage).unwrap(),
    )
    .unwrap();
    if global {
        TeletextObjectIdentity::global(resource, TeletextObjectId::new(65520))
    } else {
        TeletextObjectIdentity::public(resource, TeletextObjectId::new(65520))
    }
    .unwrap()
}
fn operation(code: u8) -> TeletextColumnOperation {
    serde_json::from_value(json!({"kind":"g0","code":code})).unwrap()
}
fn operand(global: bool, subpage: u8) -> TeletextResourceObjectOperandOverlay {
    TeletextResourceObjectOperandOverlay::new(
        identity(global, subpage),
        0,
        1,
        operation(65),
        operation(66),
    )
    .unwrap()
}
pub(super) fn local_program() -> TeletextLocalObjectGlyphProgramOverlay {
    let program = |column| {
        TeletextEnhancementProgram::new(vec![TeletextEnhancementCommand::AtColumn {
            column,
            operation: operation(67),
        }])
        .unwrap()
    };
    TeletextLocalObjectGlyphProgramOverlay::new(
        TeletextObjectIdentity::local(address(16), TeletextObjectId::new(65520)),
        program(1),
        program(2),
        vec![Some(0)],
    )
    .unwrap()
}
pub(super) fn original(mixed: bool) -> Vec<u8> {
    let mut value: Value = serde_json::from_slice(&original_with_context()).unwrap();
    let mut response: Value =
        serde_json::from_str(value["documents"][0]["response"].as_str().unwrap()).unwrap();
    let mut objects = Vec::new();
    for global in [false, true] {
        for subpage in [0, 15] {
            let id = identity(global, subpage);
            let mut wire_id = serde_json::to_value(id).unwrap();
            let resource = wire_id["resource"].as_object_mut().unwrap();
            let logical = resource.remove("logical_subpage").unwrap();
            resource.insert("subpage".into(), logical);
            objects.push(
                json!({"identity":wire_id,"object_type":"passive","program":{"commands":[
                    {"kind":"at_column","column":1,"operation":{"kind":"g0","code":65}}
                ]}}),
            );
            // The original fixture already declares POP 2A0/S1=15.
            if global || subpage != 15 {
                value["resource_transmissions"].as_array_mut().unwrap().push(json!({
                    "resource":id.resource().unwrap(),"counter":5,"update":"changed","last_subpage":true
                }));
            }
        }
    }
    response["service"]["empty_object_pages"] = json!([]);
    if mixed {
        let local = local_program();
        objects.push(
            json!({"identity":local.identity(),"object_type":"passive","program":local.expected()}),
        );
        // A Local definition requires its exact display owner's local-program
        // presence even when that program has no commands (EN 300 706 ownership).
        response["service"]["page_enhancements"] = json!([{
            "address":{"page":200,"subcode":16},"presentation":null,"compositional_links":null,
            "local_program":{"commands":[]},
            "resources":{"objects":{"public":null,"global":null},"drcs":{"normal":null,"global":null},"defaults":[null,null,null,null]}
        }]);
    }
    response["service"]["objects"] = json!(objects);
    value["documents"][0]["response"] = json!(serde_json::to_string_pretty(&response).unwrap());
    bytes(&value)
}
fn intent(
    operands: Vec<TeletextResourceObjectOperandOverlay>,
    mixed: bool,
) -> TeletextSavedEditorialOverlay {
    TeletextSavedEditorialOverlay::new_with_resource_object_operands(
        provenance(),
        vec![],
        vec![],
        vec![],
        vec![],
        if mixed { vec![local_program()] } else { vec![] },
        operands,
    )
    .unwrap()
}
fn concept() -> Value {
    serde_json::from_slice(
        append_teletext_design_editorial_overlay(
            &original(false),
            &intent(vec![operand(false, 15)], false),
            budget(),
        )
        .unwrap()
        .bytes(),
    )
    .unwrap()
}
fn rejects(value: &Value) {
    assert!(decode_teletext_design_documents(&bytes(value), budget()).is_err());
}

#[test]
fn v14_resource_scalars_preserve_both_scopes_full_s1_and_original_context() {
    for mixed in [false, true] {
        let original = original(mixed);
        let before = decode_teletext_design_documents(&original, budget()).unwrap();
        let operands = [false, true]
            .into_iter()
            .flat_map(|global| [0, 15].map(|subpage| operand(global, subpage)))
            .collect();
        let overlay = intent(operands, mixed);
        assert_eq!(overlay.schema_version(), 14);
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
        assert_eq!(
            restored
                .editorial_overlay()
                .unwrap()
                .resource_object_operands()
                .len(),
            4
        );
        assert_eq!(restored.selection(), before.selection());
        assert_eq!(restored.guide_source(), before.guide_source());
        assert_eq!(
            restored.resource_transmissions(),
            before.resource_transmissions()
        );
        assert_eq!(restored.edit_policy(), before.edit_policy());
        for (saved, root) in restored.sources().iter().zip(before.sources()) {
            assert_eq!(saved.bytes(), root.bytes());
        }
        for contract in [
            ersatzrs_domain::TeletextPageContract::V1,
            ersatzrs_domain::TeletextPageContract::V2,
            ersatzrs_domain::TeletextPageContract::V3,
        ] {
            assert_eq!(
                restored
                    .guide_inputs()
                    .and_then(|input| input.request(contract)),
                before
                    .guide_inputs()
                    .and_then(|input| input.request(contract))
            );
        }
        let debug = format!("{overlay:?}");
        assert!(!debug.contains(&"d".repeat(64)));
        assert!(!debug.contains("expected"));
    }
}
#[test]
fn v14_requires_real_resource_intent_and_cannot_downgrade_to_old_roles() {
    assert!(
        TeletextSavedEditorialOverlay::new_with_resource_object_operands(
            provenance(),
            vec![],
            vec![],
            vec![],
            vec![],
            vec![],
            vec![]
        )
        .is_err()
    );
    assert!(
        TeletextSavedEditorialOverlay::new_with_resource_object_operands(
            provenance(),
            vec![],
            vec![],
            vec![],
            vec![],
            vec![],
            vec![operand(false, 15), operand(false, 15)]
        )
        .is_err()
    );
    let baseline = concept();
    for version in 1..=13 {
        let mut value = baseline.clone();
        value["editorial_overlay"]["schema_version"] = json!(version);
        rejects(&value);
    }
    for replacement in [Value::Null, json!([]), json!({})] {
        let mut value = baseline.clone();
        value["editorial_overlay"]["resource_object_operands"] = replacement;
        rejects(&value);
    }
    let mut absent = baseline;
    absent["editorial_overlay"]
        .as_object_mut()
        .unwrap()
        .remove("resource_object_operands");
    rejects(&absent);
}
#[test]
fn v14_resource_identity_and_operands_are_closed_maps_at_every_level() {
    let baseline = concept();
    for pointer in [
        "/editorial_overlay/resource_object_operands/0",
        "/editorial_overlay/resource_object_operands/0/identity",
        "/editorial_overlay/resource_object_operands/0/identity/resource",
        "/editorial_overlay/resource_object_operands/0/identity/resource/page",
        "/editorial_overlay/resource_object_operands/0/expected",
        "/editorial_overlay/resource_object_operands/0/replacement",
    ] {
        for replacement in [Value::Null, json!([])] {
            let mut value = baseline.clone();
            *value.pointer_mut(pointer).unwrap() = replacement;
            rejects(&value);
        }
        let mut value = baseline.clone();
        value
            .pointer_mut(pointer)
            .unwrap()
            .as_object_mut()
            .unwrap()
            .insert("unknown".into(), json!(1));
        rejects(&value);
    }
    for (pointer, replacement) in [
        (
            "/editorial_overlay/resource_object_operands/0/identity/scope",
            json!("local"),
        ),
        (
            "/editorial_overlay/resource_object_operands/0/identity/resource/kind",
            json!("gpop"),
        ),
        (
            "/editorial_overlay/resource_object_operands/0/identity/resource/logical_subpage",
            json!(16),
        ),
        (
            "/editorial_overlay/resource_object_operands/0/identity/resource/page/magazine",
            json!(0),
        ),
        (
            "/editorial_overlay/resource_object_operands/0/identity/object_id",
            json!(65536),
        ),
        (
            "/editorial_overlay/resource_object_operands/0/command",
            json!(-1),
        ),
        (
            "/editorial_overlay/resource_object_operands/0/column",
            json!(72),
        ),
        (
            "/editorial_overlay/resource_object_operands/0/expected/code",
            json!(256),
        ),
        (
            "/editorial_overlay/resource_object_operands/0/replacement",
            json!({"kind":"foreground","colour":1}),
        ),
    ] {
        let mut value = baseline.clone();
        *value.pointer_mut(pointer).unwrap() = replacement;
        rejects(&value);
    }
}
#[test]
fn v14_authenticates_original_resource_full_identity_ordinal_column_and_operand() {
    let baseline = concept();
    for (pointer, replacement) in [
        (
            "/editorial_overlay/resource_object_operands/0/identity/resource/logical_subpage",
            json!(1),
        ),
        (
            "/editorial_overlay/resource_object_operands/0/identity/object_id",
            json!(65519),
        ),
        (
            "/editorial_overlay/resource_object_operands/0/identity/resource/page/page",
            json!(162),
        ),
        (
            "/editorial_overlay/resource_object_operands/0/command",
            json!(1),
        ),
        (
            "/editorial_overlay/resource_object_operands/0/column",
            json!(2),
        ),
        (
            "/editorial_overlay/resource_object_operands/0/expected/code",
            json!(67),
        ),
    ] {
        let mut value = baseline.clone();
        *value.pointer_mut(pointer).unwrap() = replacement;
        rejects(&value);
    }
    let mut duplicate = baseline;
    let operand = duplicate["editorial_overlay"]["resource_object_operands"][0].clone();
    duplicate["editorial_overlay"]["resource_object_operands"]
        .as_array_mut()
        .unwrap()
        .push(operand);
    rejects(&duplicate);
}
#[test]
fn v14_preflight_and_attachment_share_existing_command_item_and_byte_budgets() {
    let original = original(true);
    let overlay = intent(
        [false, true]
            .into_iter()
            .flat_map(|global| [0, 15].map(|subpage| operand(global, subpage)))
            .collect(),
        true,
    );
    let encoded = append_teletext_design_editorial_overlay(&original, &overlay, budget()).unwrap();
    let mut limited = budget();
    limited.max_serialized_bytes = encoded.bytes().len() - 1;
    assert!(decode_teletext_design_documents(encoded.bytes(), limited).is_err());
    limited = budget();
    limited.validation.service.max_commands = 5;
    assert!(
        decode_teletext_design_documents(&original, limited).is_ok(),
        "original command budget must pass before editorial intent is counted"
    );
    assert!(decode_teletext_design_documents(encoded.bytes(), limited).is_err());
    assert!(append_teletext_design_editorial_overlay(&original, &overlay, limited).is_err());
    limited = budget();
    limited.validation.service.max_structure_items = 3;
    assert!(decode_teletext_design_documents(encoded.bytes(), limited).is_err());
    assert!(append_teletext_design_editorial_overlay(&original, &overlay, limited).is_err());
}
#[test]
fn v14_field_stays_absent_from_existing_row_overlay_serialization() {
    let original = original_with_context();
    let overlay = super::tests::overlay();
    let encoded = append_teletext_design_editorial_overlay(&original, &overlay, budget()).unwrap();
    let value: Value = serde_json::from_slice(encoded.bytes()).unwrap();
    assert_eq!(value["editorial_overlay"]["schema_version"], json!(1));
    assert!(value["editorial_overlay"]
        .get("resource_object_operands")
        .is_none());
    assert!(decode_teletext_design_documents(encoded.bytes(), budget())
        .unwrap()
        .editorial_overlay()
        .unwrap()
        .resource_object_operands()
        .is_empty());
}

#[test]
fn v14_combines_resource_intent_with_every_existing_local_program_policy() {
    let original = original(true);
    let before = local_program().expected().clone();
    for operation in [
        operation(68),
        serde_json::from_value(json!({"kind":"g2","code":35})).unwrap(),
        serde_json::from_value(json!({"kind":"foreground","colour":2})).unwrap(),
    ] {
        let mut commands = before.commands().to_vec();
        commands.push(TeletextEnhancementCommand::AtColumn {
            column: 2,
            operation,
        });
        let after = TeletextEnhancementProgram::new(commands).unwrap();
        let program = TeletextLocalObjectGlyphProgramOverlay::new_with_style_commands(
            local_program().identity(),
            before.clone(),
            after,
            vec![Some(0), None],
        )
        .unwrap();
        let overlay = TeletextSavedEditorialOverlay::new_with_resource_object_operands(
            provenance(),
            vec![],
            vec![],
            vec![],
            vec![],
            vec![program],
            vec![operand(false, 15)],
        )
        .unwrap();
        let encoded =
            append_teletext_design_editorial_overlay(&original, &overlay, budget()).unwrap();
        assert_eq!(
            decode_teletext_design_documents(encoded.bytes(), budget())
                .unwrap()
                .editorial_overlay(),
            Some(&overlay)
        );
    }
}
