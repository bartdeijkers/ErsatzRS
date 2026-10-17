//! V18 adds complete definitions without rewriting originals or weakening V1–17.
use super::resource_object_operand_tests::{identity, local_program, original};
use super::tests::{budget, bytes, provenance};
use super::*;
use ersatzrs_domain::{
    TeletextEnhancementObjectDefinition, TeletextEnhancementProgram, TeletextObjectIdentity,
    TeletextObjectType, TeletextResourceObjectDefinitionAddition,
};
use serde_json::{json, Value};

fn new_identity(global: bool, subpage: u8) -> TeletextObjectIdentity {
    let mut value = serde_json::to_value(identity(global, subpage)).unwrap();
    value["object_id"] = json!(65521);
    serde_json::from_value(value).unwrap()
}
fn glyph(code: u8) -> TeletextEnhancementCommand {
    serde_json::from_value(
        json!({"kind":"at_column","column":1,"operation":{"kind":"g0","code":code}}),
    )
    .unwrap()
}
fn addition(global: bool, subpage: u8) -> TeletextResourceObjectDefinitionAddition {
    TeletextResourceObjectDefinitionAddition::new(
        TeletextEnhancementObjectDefinition {
            identity: new_identity(global, subpage),
            object_type: TeletextObjectType::Passive,
            program: TeletextEnhancementProgram::new(vec![glyph(66)]).unwrap(),
        },
        budget().validation.service,
    )
    .unwrap()
}
fn concept() -> Value {
    let overlay = TeletextSavedEditorialOverlay::new_with_resource_object_definition_additions(
        provenance(),
        vec![addition(false, 15)],
    )
    .unwrap();
    serde_json::from_slice(
        append_teletext_design_editorial_overlay(&original(false), &overlay, budget())
            .unwrap()
            .bytes(),
    )
    .unwrap()
}
#[test]
fn v18_definition_additions_roundtrip_full_scopes_s1_and_exact_original_bytes() {
    let original = original(false);
    let additions = [false, true]
        .into_iter()
        .flat_map(|global| [0, 15].map(|subpage| addition(global, subpage)))
        .collect::<Vec<_>>();
    let overlay = TeletextSavedEditorialOverlay::new_with_resource_object_definition_additions(
        provenance(),
        additions.clone(),
    )
    .unwrap();
    assert_eq!(overlay.schema_version(), 18);
    let encoded = append_teletext_design_editorial_overlay(&original, &overlay, budget()).unwrap();
    let end = original
        .iter()
        .rposition(|byte| !byte.is_ascii_whitespace())
        .unwrap();
    assert_eq!(&encoded.bytes()[..end], &original[..end]);
    assert!(encoded.bytes().ends_with(&original[end..]));
    let restored = decode_teletext_design_documents(encoded.bytes(), budget()).unwrap();
    let saved = restored.editorial_overlay().unwrap();
    assert_eq!(saved, &overlay);
    assert_eq!(saved.resource_object_definition_additions(), additions);
    let decoded_original = decode_teletext_design_documents(&original, budget()).unwrap();
    assert_eq!(restored.sources().len(), decoded_original.sources().len());
    for (after, before) in restored.sources().iter().zip(decoded_original.sources()) {
        assert_eq!(after.bytes(), before.bytes());
        assert_eq!(after.authoring(), before.authoring());
    }
    let TeletextProfileDocument::Authored(service) = &restored.documents()[0] else {
        panic!("authored original")
    };
    for addition in &additions {
        assert!(
            service
                .semantic()
                .objects()
                .definition(addition.identity())
                .is_none(),
            "restored originals must not silently become edited output"
        );
    }
    let applied = service
        .as_ref()
        .clone()
        .with_resource_object_definition_additions(&additions, budget().validation.service)
        .unwrap();
    for addition in additions {
        assert_eq!(
            applied.semantic().objects().definition(addition.identity()),
            Some(addition.definition())
        );
    }
}
#[test]
fn v18_definition_additions_preserve_existing_local_and_resource_program_intents() {
    let before = TeletextEnhancementProgram::new(vec![glyph(65)]).unwrap();
    let after = TeletextEnhancementProgram::new(vec![
        glyph(65),
        TeletextEnhancementCommand::AtColumn {
            column: 2,
            operation: serde_json::from_value(json!({"kind":"g0","code":67})).unwrap(),
        },
    ])
    .unwrap();
    let change = TeletextResourceObjectProgramOverlay::new(
        identity(false, 15),
        before,
        after,
        vec![Some(0), None],
        budget().validation.service,
    )
    .unwrap();
    let body = TeletextSavedEditorialOverlay::new_with_resource_object_program_overlays(
        provenance(),
        vec![],
        vec![],
        vec![],
        vec![],
        vec![local_program()],
        (vec![], vec![change]),
    )
    .unwrap();
    let body_only =
        append_teletext_design_editorial_overlay(&original(true), &body, budget()).unwrap();
    let overlay = body
        .with_resource_object_definition_additions(vec![addition(false, 15)])
        .unwrap();
    let encoded =
        append_teletext_design_editorial_overlay(&original(true), &overlay, budget()).unwrap();
    let restored = decode_teletext_design_documents(encoded.bytes(), budget()).unwrap();
    assert_eq!(restored.editorial_overlay().unwrap(), &overlay);
    assert_eq!(overlay.local_object_glyph_program_overlays().len(), 1);
    assert_eq!(overlay.resource_object_program_overlays().len(), 1);
    // Each edit alone fits six commands; the original five plus a new
    // definition and the existing resource's expansion require seven together.
    let mut tight = budget();
    tight.validation.service.max_commands = 6;
    decode_teletext_design_documents(body_only.bytes(), tight).unwrap();
    let additions_only =
        TeletextSavedEditorialOverlay::new_with_resource_object_definition_additions(
            provenance(),
            vec![addition(false, 15)],
        )
        .unwrap();
    append_teletext_design_editorial_overlay(&original(true), &additions_only, tight).unwrap();
    let error = decode_teletext_design_documents(encoded.bytes(), tight).unwrap_err();
    assert!(
        matches!(
            error.reason,
            TeletextDesignDocumentFailure::Budget {
                dimension: "editorial result commands",
                count: 7,
                maximum: 6,
            }
        ),
        "combined original/addition/body result must be bounded: {error:?}"
    );
}
#[test]
fn v18_definition_addition_field_is_closed_and_cannot_be_smuggled_into_old_versions() {
    let valid = concept();
    for version in 1..18 {
        for field in [
            valid["editorial_overlay"]["resource_object_definition_additions"].clone(),
            json!([]),
            Value::Null,
        ] {
            let mut value = valid.clone();
            value["editorial_overlay"]["schema_version"] = json!(version);
            value["editorial_overlay"]["resource_object_definition_additions"] = field;
            assert!(decode_teletext_design_documents(&bytes(&value), budget()).is_err());
        }
    }
    for replacement in [Value::Null, json!([]), json!({}), json!([null])] {
        let mut value = valid.clone();
        value["editorial_overlay"]["resource_object_definition_additions"] = replacement;
        assert!(decode_teletext_design_documents(&bytes(&value), budget()).is_err());
    }
    let mut absent = valid.clone();
    absent["editorial_overlay"]
        .as_object_mut()
        .unwrap()
        .remove("resource_object_definition_additions");
    assert!(decode_teletext_design_documents(&bytes(&absent), budget()).is_err());
    for pointer in [
        "/editorial_overlay/resource_object_definition_additions/0",
        "/editorial_overlay/resource_object_definition_additions/0/identity",
        "/editorial_overlay/resource_object_definition_additions/0/identity/resource",
        "/editorial_overlay/resource_object_definition_additions/0/identity/resource/page",
        "/editorial_overlay/resource_object_definition_additions/0/program",
        "/editorial_overlay/resource_object_definition_additions/0/program/commands/0",
    ] {
        let mut value = valid.clone();
        value
            .pointer_mut(pointer)
            .unwrap()
            .as_object_mut()
            .unwrap()
            .insert("unexpected".into(), json!(true));
        assert!(
            decode_teletext_design_documents(&bytes(&value), budget()).is_err(),
            "closed {pointer}"
        );
    }
    let mut duplicate = valid;
    let item = duplicate["editorial_overlay"]["resource_object_definition_additions"][0].clone();
    duplicate["editorial_overlay"]["resource_object_definition_additions"]
        .as_array_mut()
        .unwrap()
        .push(item);
    assert!(decode_teletext_design_documents(&bytes(&duplicate), budget()).is_err());
}
#[test]
fn v18_definition_additions_reject_shadow_missing_resource_overlap_and_storage_limits() {
    let valid = concept();
    for (pointer,replacement) in [
        ("/editorial_overlay/resource_object_definition_additions/0/identity/object_id",json!(65520)),
        ("/editorial_overlay/resource_object_definition_additions/0/identity/resource/page/page",json!(162)),
        ("/editorial_overlay/resource_object_definition_additions/0/identity/resource/logical_subpage",json!(14)),
    ] {
        let mut value=valid.clone();*value.pointer_mut(pointer).unwrap()=replacement;
        assert!(decode_teletext_design_documents(&bytes(&value),budget()).is_err());
    }
    assert!(
        TeletextSavedEditorialOverlay::new_with_resource_object_definition_additions(
            provenance(),
            vec![]
        )
        .is_err()
    );
    let scalar = TeletextResourceObjectOperandOverlay::new(
        new_identity(false, 15),
        0,
        1,
        serde_json::from_value(json!({"kind":"g0","code":66})).unwrap(),
        serde_json::from_value(json!({"kind":"g0","code":67})).unwrap(),
    )
    .unwrap();
    let body = TeletextSavedEditorialOverlay::new_with_resource_object_operands(
        provenance(),
        vec![],
        vec![],
        vec![],
        vec![],
        vec![],
        vec![scalar],
    )
    .unwrap();
    assert!(
        body.with_resource_object_definition_additions(vec![addition(false, 15)])
            .is_err(),
        "new-object body edits belong in its complete addition"
    );
    let raw = bytes(&valid);
    let mut small = budget();
    small.max_serialized_bytes = raw.len() - 1;
    assert!(decode_teletext_design_documents(&raw, small).is_err());
    let mut inflated = valid;
    let item = inflated["editorial_overlay"]["resource_object_definition_additions"][0].clone();
    inflated["editorial_overlay"]["resource_object_definition_additions"] =
        json!(vec![item; budget().validation.service.max_objects + 1]);
    let error = decode_teletext_design_documents(&bytes(&inflated), budget()).unwrap_err();
    assert!(
        matches!(
            error.reason,
            TeletextDesignDocumentFailure::Budget {
                dimension: "editorial definition additions",
                ..
            }
        ),
        "bounded streaming preflight must reject before owned definition allocation: {error:?}"
    );
}
