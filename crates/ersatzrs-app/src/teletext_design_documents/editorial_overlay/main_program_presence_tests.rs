//! V29 preserves exact optional presence, closed legacy roles and immutable sources.
use super::resource_object_operand_tests::identity;
use super::tests::{address, budget, bytes, provenance};
use super::*;
use ersatzrs_domain::*;
use serde_json::{json, Value};

fn empty() -> TeletextEnhancementProgram {
    TeletextEnhancementProgram::new(vec![]).unwrap()
}
fn call(global: bool) -> TeletextEnhancementCommand {
    TeletextEnhancementCommand::InvokeObject {
        target: identity(global, 15),
        object_type: TeletextObjectType::Passive,
        offset: None,
    }
}
fn source(present: bool) -> Vec<u8> {
    let mut value: Value =
        serde_json::from_slice(&super::resource_object_operand_tests::original(false)).unwrap();
    let mut response: Value =
        serde_json::from_str(value["documents"][0]["response"].as_str().unwrap()).unwrap();
    let mut wire_call = serde_json::to_value(call(false)).unwrap();
    let resource = wire_call["target"]["resource"].as_object_mut().unwrap();
    let subpage = resource.remove("logical_subpage").unwrap();
    resource.insert("subpage".into(), subpage);
    let resources = json!({"objects":{"public":identity(false,15).resource().unwrap().page(),"global":identity(true,15).resource().unwrap().page()},"drcs":{"normal":null,"global":null},"defaults":[null,null,null,null]});
    let link = |global| json!({"target":{"kind":"page","page":identity(global,15).resource().unwrap().page()},"validity":"level25_only","required_subpages":32769});
    let absent = json!({"target":{"kind":"unspecified","magazine":2},"validity":"level25_only","required_subpages":65535});
    response["service"]["page_enhancements"] = json!([
        {"address":address(16),"presentation":null,"compositional_links":null,"local_program":if present {json!(empty())} else {Value::Null},
            "resources":{"objects":{"public":null,"global":null},"drcs":{"normal":null,"global":null},"defaults":[null,null,null,null]}},
        {"address":address(17),"presentation":null,"compositional_links":{"pop":link(false),"gpop":link(true),"drcs":absent,"gdrcs":absent},
            "local_program":{"commands":[wire_call]},"resources":resources}
    ]);
    value["documents"][0]["response"] = json!(serde_json::to_string_pretty(&response).unwrap());
    let data = bytes(&value);
    decode_teletext_design_documents(&data, budget())
        .expect("independent presence source must qualify");
    data
}
fn change(present: bool) -> TeletextMainProgramPresenceOverlay {
    TeletextMainProgramPresenceOverlay::new(
        address(16),
        present.then(empty),
        (!present).then(empty),
    )
    .unwrap()
}
fn pure(present: bool) -> TeletextSavedEditorialOverlay {
    TeletextSavedEditorialOverlay::new_with_main_program_presence_overlays(
        provenance(),
        vec![change(present)],
    )
    .unwrap()
}

#[test]
fn v29_main_program_presence_roundtrips_null_empty_and_all_original_source_bytes() {
    for present in [false, true] {
        let original = source(present);
        let before = decode_teletext_design_documents(&original, budget()).unwrap();
        let overlay = pure(present);
        let data = append_teletext_design_editorial_overlay(&original, &overlay, budget())
            .unwrap()
            .into_bytes();
        let value: Value = serde_json::from_slice(&data).unwrap();
        let record = &value["editorial_overlay"]["main_program_presence_overlays"][0];
        assert_eq!(value["editorial_overlay"]["schema_version"], 29);
        assert_eq!(
            record["expected"],
            if present { json!(empty()) } else { Value::Null }
        );
        assert_eq!(
            record["replacement"],
            if present { Value::Null } else { json!(empty()) }
        );
        let restored = decode_teletext_design_documents(&data, budget()).unwrap();
        assert_eq!(restored.editorial_overlay(), Some(&overlay));
        assert_eq!(restored.sources()[0].bytes(), before.sources()[0].bytes());
        assert_eq!(
            restored.sources()[0].authoring(),
            before.sources()[0].authoring()
        );
        assert_eq!(restored.selection(), before.selection());
        assert_eq!(
            restored.resource_transmissions(),
            before.resource_transmissions()
        );
        assert_eq!(restored.guide_source(), before.guide_source());
    }
}

#[test]
fn v29_main_program_presence_composes_latest_role_without_downgrading_either_order() {
    let invocation =
        TeletextMainResourceInvocationOverlay::new(address(17), 0, call(false), call(true))
            .unwrap();
    let first = pure(false)
        .with_main_resource_invocations(vec![invocation])
        .unwrap();
    let last = TeletextSavedEditorialOverlay::new_with_main_resource_invocations(
        provenance(),
        vec![invocation],
    )
    .unwrap()
    .with_main_program_presence_overlays(vec![change(false)])
    .unwrap();
    assert_eq!(first, last);
    let data = append_teletext_design_editorial_overlay(&source(false), &first, budget())
        .unwrap()
        .into_bytes();
    let restored = decode_teletext_design_documents(&data, budget()).unwrap();
    assert_eq!(restored.editorial_overlay(), Some(&first));
    assert_eq!(restored.editorial_overlay().unwrap().schema_version(), 29);
    assert_eq!(
        restored
            .editorial_overlay()
            .unwrap()
            .main_resource_invocations(),
        &[invocation]
    );
}

#[test]
fn v29_main_program_presence_rejects_legacy_fields_missing_null_bad_transition_and_source_mismatch()
{
    let original = source(false);
    let data = append_teletext_design_editorial_overlay(&original, &pure(false), budget())
        .unwrap()
        .into_bytes();
    let base: Value = serde_json::from_slice(&data).unwrap();
    for version in 1..29 {
        let mut bad = base.clone();
        bad["editorial_overlay"]["schema_version"] = json!(version);
        assert!(
            decode_teletext_design_documents(&bytes(&bad), budget()).is_err(),
            "closed V{version}"
        );
    }
    for field in [
        Value::Null,
        json!([]),
        json!([{}]),
        json!([{"owner":address(16),"expected":null,"replacement":null}]),
    ] {
        let mut bad = base.clone();
        bad["editorial_overlay"]["main_program_presence_overlays"] = field;
        assert!(decode_teletext_design_documents(&bytes(&bad), budget()).is_err());
    }
    for key in ["expected", "replacement"] {
        let mut bad = base.clone();
        bad["editorial_overlay"]["main_program_presence_overlays"][0]
            .as_object_mut()
            .unwrap()
            .remove(key);
        assert!(
            decode_teletext_design_documents(&bytes(&bad), budget()).is_err(),
            "required explicit {key}"
        );
    }
    let mut bad = base.clone();
    bad["editorial_overlay"]["main_program_presence_overlays"][0]["extra"] = json!(true);
    assert!(decode_teletext_design_documents(&bytes(&bad), budget()).is_err());
    assert!(append_teletext_design_editorial_overlay(&original, &pure(true), budget()).is_err());
    let duplicate = TeletextSavedEditorialOverlay::new_with_main_program_presence_overlays(
        provenance(),
        vec![change(false), change(false)],
    );
    assert!(duplicate.is_err());
    assert!(pure(false)
        .with_main_program_presence_overlays(vec![change(false)])
        .is_err());
    assert_invalid_targets(&original);
}

fn assert_invalid_targets(original: &[u8]) {
    let resource = identity(true, 15).resource().unwrap();
    let unknown = TeletextObjectIdentity::global(resource, TeletextObjectId::new(99)).unwrap();
    let foreign = TeletextObjectIdentity::local(address(17), TeletextObjectId::new(65520));
    for (target, object_type) in [
        (unknown, TeletextObjectType::Passive),
        (foreign, TeletextObjectType::Passive),
        (identity(true, 15), TeletextObjectType::Active),
    ] {
        let replacement =
            TeletextEnhancementProgram::new(vec![TeletextEnhancementCommand::InvokeObject {
                target,
                object_type,
                offset: None,
            }])
            .unwrap();
        let presence =
            TeletextMainProgramPresenceOverlay::new(address(16), None, Some(replacement)).unwrap();
        let overlay = TeletextSavedEditorialOverlay::new_with_main_program_presence_overlays(
            provenance(),
            vec![presence],
        )
        .unwrap();
        assert!(append_teletext_design_editorial_overlay(original, &overlay, budget()).is_err());
    }
}

#[test]
fn v29_main_program_presence_bounds_full_authored_bodies_without_a_208_word_alias() {
    let command = TeletextEnhancementCommand::AtColumn {
        column: 1,
        operation: TeletextColumnOperation::G0 {
            code: TeletextGlyphCode::new(b'A').unwrap(),
        },
    };
    let presence = TeletextMainProgramPresenceOverlay::new(
        address(16),
        None,
        Some(TeletextEnhancementProgram::new(vec![command; 300]).unwrap()),
    )
    .unwrap();
    let overlay = TeletextSavedEditorialOverlay::new_with_main_program_presence_overlays(
        provenance(),
        vec![presence],
    )
    .unwrap();
    let data = append_teletext_design_editorial_overlay(&source(false), &overlay, budget())
        .unwrap()
        .into_bytes();
    assert_eq!(
        decode_teletext_design_documents(&data, budget())
            .unwrap()
            .editorial_overlay(),
        Some(&overlay)
    );
    let mut small = budget();
    small.validation.service.max_commands = 299;
    assert!(decode_teletext_design_documents(&data, small).is_err());
    small = budget();
    small.validation.service.max_structure_items = 10;
    assert!(decode_teletext_design_documents(&data, small).is_err());
}
