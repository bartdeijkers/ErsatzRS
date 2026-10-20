//! V28 stores original Main resource retargets without widening earlier roles.
use super::resource_object_operand_tests::{identity, local_program};
use super::tests::{address, budget, bytes, provenance};
use super::*;
use ersatzrs_domain::*;
use serde_json::{json, Value};

fn call(global: bool, subpage: u8) -> TeletextEnhancementCommand {
    TeletextEnhancementCommand::InvokeObject {
        target: identity(global, subpage),
        object_type: TeletextObjectType::Passive,
        offset: Some(TeletextObjectOffset::new(0, 1).unwrap()),
    }
}
fn foreground(colour: u8) -> TeletextEnhancementCommand {
    TeletextEnhancementCommand::AtColumn {
        column: 0,
        operation: TeletextColumnOperation::Foreground {
            colour: TeletextColourReference::new(colour).unwrap(),
        },
    }
}
fn original() -> Vec<u8> {
    let source = super::resource_object_operand_tests::original(true);
    let mut value: Value = serde_json::from_slice(&source).unwrap();
    let mut response: Value =
        serde_json::from_str(value["documents"][0]["response"].as_str().unwrap()).unwrap();
    let page = &mut response["service"]["page_enhancements"][0];
    page["local_program"] = json!({"commands":[foreground(1),call(false,15)]});
    // Producer V3 uses `subpage`; private domain/storage identities deliberately
    // retain `logical_subpage`, as in the reused resource fixture above.
    let resource = page["local_program"]["commands"][1]["target"]["resource"]
        .as_object_mut()
        .unwrap();
    let subpage = resource.remove("logical_subpage").unwrap();
    resource.insert("subpage".into(), subpage);
    page["resources"]["objects"] = json!({"public":identity(false,15).resource().unwrap().page(),"global":identity(true,15).resource().unwrap().page()});
    let selected = |global| json!({"target":{"kind":"page","page":identity(global,15).resource().unwrap().page()},"validity":"level25_only","required_subpages":32769});
    let absent = json!({"target":{"kind":"unspecified","magazine":2},"validity":"level25_only","required_subpages":65535});
    page["compositional_links"] =
        json!({"pop":selected(false),"gpop":selected(true),"drcs":absent,"gdrcs":absent});
    crate::teletext_pages_v3::parse_authored_service_with_metadata(
        &serde_json::to_vec(&response).unwrap(),
        response["service"]["generation"].as_str().unwrap(),
        budget().validation.service,
        budget().validation.declarations,
    )
    .expect("the independent Main resource-call original must be a valid declared service");
    value["documents"][0]["response"] = json!(serde_json::to_string_pretty(&response).unwrap());
    bytes(&value)
}
fn change() -> TeletextMainResourceInvocationOverlay {
    TeletextMainResourceInvocationOverlay::new(address(16), 1, call(false, 15), call(true, 15))
        .unwrap()
}
fn pure() -> TeletextSavedEditorialOverlay {
    TeletextSavedEditorialOverlay::new_with_main_resource_invocations(provenance(), vec![change()])
        .unwrap()
}
fn saved(overlay: &TeletextSavedEditorialOverlay) -> Vec<u8> {
    append_teletext_design_editorial_overlay(&original(), overlay, budget())
        .unwrap()
        .into_bytes()
}
fn main_local() -> TeletextMainLocalInvocationProgramOverlay {
    let expected = TeletextEnhancementProgram::new(vec![foreground(1), call(false, 15)]).unwrap();
    let mut replacement = expected.commands().to_vec();
    replacement.push(TeletextEnhancementCommand::InvokeObject {
        target: local_program().identity(),
        object_type: TeletextObjectType::Passive,
        offset: None,
    });
    TeletextMainLocalInvocationProgramOverlay::new(
        address(16),
        expected,
        TeletextEnhancementProgram::new(replacement).unwrap(),
        vec![Some(0), Some(1), None],
    )
    .unwrap()
}
fn mixed() -> TeletextSavedEditorialOverlay {
    let expected = TeletextEnhancementProgram::new(vec![TeletextEnhancementCommand::AtColumn {
        column: 1,
        operation: TeletextColumnOperation::G0 {
            code: TeletextGlyphCode::new(b'A').unwrap(),
        },
    }])
    .unwrap();
    let replacement = TeletextEnhancementProgram::new(vec![TeletextEnhancementCommand::AtColumn {
        column: 2,
        operation: TeletextColumnOperation::G0 {
            code: TeletextGlyphCode::new(b'A').unwrap(),
        },
    }])
    .unwrap();
    let resource = TeletextResourceObjectProgramOverlay::new(
        identity(true, 15),
        expected,
        replacement,
        vec![Some(0)],
        budget().validation.service,
    )
    .unwrap();
    TeletextSavedEditorialOverlay::new_with_resource_object_program_overlays(
        provenance(),
        vec![],
        vec![],
        vec![],
        vec![],
        vec![local_program()],
        (vec![], vec![resource]),
    )
    .unwrap()
    .with_main_local_invocation_programs(vec![main_local()])
    .unwrap()
    .with_main_resource_invocations(vec![change()])
    .unwrap()
}

#[test]
fn v28_main_resource_invocations_preserve_originals_and_closed_prior_roles() {
    let source = original();
    let before = decode_teletext_design_documents(&source, budget()).unwrap();
    for overlay in [pure(), mixed()] {
        assert_eq!(overlay.schema_version(), 28);
        let data = saved(&overlay);
        let value: Value = serde_json::from_slice(&data).unwrap();
        assert_eq!(value["editorial_overlay"]["schema_version"], 28);
        assert_eq!(
            value["editorial_overlay"]["main_resource_invocations"][0]["command"],
            1
        );
        assert_eq!(
            value["editorial_overlay"]["main_resource_invocations"][0]["expected"],
            json!(call(false, 15))
        );
        assert_eq!(
            value["editorial_overlay"]["main_resource_invocations"][0]["replacement"],
            json!(call(true, 15))
        );
        let after = decode_teletext_design_documents(&data, budget()).unwrap();
        assert_eq!(after.editorial_overlay(), Some(&overlay));
        assert_eq!(
            after
                .editorial_overlay()
                .unwrap()
                .main_resource_invocations(),
            &[change()]
        );
        assert_eq!(after.sources()[0].bytes(), before.sources()[0].bytes());
        assert_eq!(
            after.sources()[0].authoring(),
            before.sources()[0].authoring()
        );
        assert_eq!(after.selection(), before.selection());
        assert_eq!(
            after.resource_transmissions(),
            before.resource_transmissions()
        );
        assert_eq!(after.guide_source(), before.guide_source());
        let end = source
            .iter()
            .rposition(|byte| !byte.is_ascii_whitespace())
            .unwrap();
        assert_eq!(&data[..end], &source[..end]);
        assert_eq!(saved(after.editorial_overlay().unwrap()), data);
    }
}

#[test]
fn v28_main_resource_invocations_compose_selection_banks_and_existing_local_resource_intents() {
    let additions = super::object_resource_storage_tests::additions();
    let target = additions[1].addition().definitions()[0].identity;
    let replacement = TeletextEnhancementCommand::InvokeObject {
        target,
        object_type: TeletextObjectType::Passive,
        offset: Some(TeletextObjectOffset::new(0, 1).unwrap()),
    };
    let change =
        TeletextMainResourceInvocationOverlay::new(address(16), 1, call(false, 15), replacement)
            .unwrap();
    let root = decode_teletext_design_documents(&original(), budget()).unwrap();
    let TeletextProfileDocument::Authored(service) = &root.documents()[0] else {
        panic!()
    };
    let links = service
        .semantic()
        .page_enhancements()
        .get(&address(16))
        .unwrap()
        .compositional_links;
    let selection = TeletextObjectPageSelectionChange::new(
        address(16),
        TeletextX27LinkRole::Pop,
        links,
        TeletextMotRoleResolution::Selected {
            target: identity(false, 15).resource().unwrap().page(),
            requirements: TeletextMotResolutionRequirements::X27Mask(
                TeletextSubpageRequirementMask::new(32769),
            ),
            provenance: TeletextMotLinkProvenance::X27 {
                display: address(16),
                role: TeletextX27LinkRole::Pop,
            },
            original_mot: None,
        },
        TeletextX27LinkDeclaration::new(
            TeletextX27LinkTarget::Page {
                page: target.resource().unwrap().page(),
            },
            TeletextX27LinkValidity::Level25Only,
            TeletextSubpageRequirementMask::new(32769),
        )
        .unwrap(),
    )
    .unwrap();
    let mut overlay = mixed();
    overlay.main_resource_invocations = vec![change];
    let overlay = overlay
        .with_object_resource_additions(additions)
        .unwrap()
        .with_object_page_selections(vec![TeletextSavedObjectPageSelectionChange::new(
            0, selection,
        )
        .unwrap()])
        .unwrap();
    let data = saved(&overlay);
    let value: Value = serde_json::from_slice(&data).unwrap();
    assert_eq!(value["editorial_overlay"]["schema_version"], 28);
    assert_eq!(value["editorial_overlay"]["prior_schema_version"], 22);
    let restored = decode_teletext_design_documents(&data, budget()).unwrap();
    assert_eq!(restored.editorial_overlay(), Some(&overlay));
    assert_eq!(saved(restored.editorial_overlay().unwrap()), data);
}

#[test]
fn v28_main_resource_invocations_cannot_enter_old_versions_or_recursive_envelopes() {
    let value: Value = serde_json::from_slice(&saved(&pure())).unwrap();
    for version in 1..=27 {
        for field in [
            value["editorial_overlay"]["main_resource_invocations"].clone(),
            json!([]),
            Value::Null,
        ] {
            let mut bad = value.clone();
            bad["editorial_overlay"]["schema_version"] = json!(version);
            bad["editorial_overlay"]["main_resource_invocations"] = field;
            assert!(
                decode_teletext_design_documents(&bytes(&bad), budget()).is_err(),
                "old role{version}"
            );
        }
    }
    for prior in [0, 25, 26, 27, 28, 65535] {
        let mut bad = value.clone();
        bad["editorial_overlay"]["prior_schema_version"] = json!(prior);
        assert!(decode_teletext_design_documents(&bytes(&bad), budget()).is_err());
    }
    let mut prior22: Value = serde_json::from_slice(&saved(&mixed())).unwrap();
    prior22["editorial_overlay"]["schema_version"] = json!(22);
    prior22["editorial_overlay"]
        .as_object_mut()
        .unwrap()
        .remove("prior_schema_version");
    prior22["editorial_overlay"]
        .as_object_mut()
        .unwrap()
        .remove("main_resource_invocations");
    assert!(decode_teletext_design_documents(&bytes(&prior22), budget()).is_ok());
    prior22["editorial_overlay"]["main_local_invocation_program_overlays"][0]["replacement"]
        ["commands"][1] = json!(call(true, 15));
    assert!(
        decode_teletext_design_documents(&bytes(&prior22), budget()).is_err(),
        "V22 resource anchors remain exact"
    );
}

#[test]
fn v28_main_resource_invocations_reject_empty_null_unknown_and_invalid_calls() {
    let value: Value = serde_json::from_slice(&saved(&pure())).unwrap();
    for field in [
        json!([]),
        Value::Null,
        json!({}),
        json!([null]),
        json!([[]]),
    ] {
        let mut bad = value.clone();
        bad["editorial_overlay"]["main_resource_invocations"] = field;
        assert!(decode_teletext_design_documents(&bytes(&bad), budget()).is_err());
    }
    for (field, replacement) in [
        ("extra", json!(1)),
        ("command", json!(usize::MAX)),
        ("expected", json!(call(true, 15))),
        ("replacement", json!(call(false, 15))),
    ] {
        let mut bad = value.clone();
        bad["editorial_overlay"]["main_resource_invocations"][0][field] = replacement;
        assert!(decode_teletext_design_documents(&bytes(&bad), budget()).is_err());
    }
    for replacement in [
        json!(foreground(1)),
        json!({"kind":"invoke_object","target":local_program().identity(),"object_type":"passive","offset":{"row":0,"column":1}}),
        json!({"kind":"invoke_object","target":identity(true,15),"object_type":"active","offset":{"row":0,"column":1}}),
        json!({"kind":"invoke_object","target":identity(true,15),"object_type":"passive","offset":null}),
    ] {
        let mut bad = value.clone();
        bad["editorial_overlay"]["main_resource_invocations"][0]["replacement"] = replacement;
        assert!(decode_teletext_design_documents(&bytes(&bad), budget()).is_err());
    }
    assert!(
        TeletextSavedEditorialOverlay::new_with_main_resource_invocations(provenance(), vec![])
            .is_err()
    );
    assert!(
        TeletextSavedEditorialOverlay::new_with_main_resource_invocations(
            provenance(),
            vec![change(), change()]
        )
        .is_err()
    );
    assert!(pure()
        .with_main_resource_invocations(vec![change()])
        .is_err());
}

#[test]
fn v28_main_resource_invocations_require_unique_original_owner_and_full_target() {
    let source = original();
    let root = decode_teletext_design_documents(&source, budget()).unwrap();
    let TeletextProfileDocument::Authored(service) = &root.documents()[0] else {
        panic!()
    };
    let duplicate = vec![
        TeletextProfileDocument::Authored(service.clone()),
        TeletextProfileDocument::Authored(service.clone()),
    ];
    assert!(main_resource_invocations::validate_association(&pure(), &duplicate).is_err());
    for change in [
        TeletextMainResourceInvocationOverlay::new(address(17), 1, call(false, 15), call(true, 15))
            .unwrap(),
        TeletextMainResourceInvocationOverlay::new(address(16), 1, call(false, 15), call(true, 14))
            .unwrap(),
    ] {
        let overlay = TeletextSavedEditorialOverlay::new_with_main_resource_invocations(
            provenance(),
            vec![change],
        )
        .unwrap();
        assert!(append_teletext_design_editorial_overlay(&source, &overlay, budget()).is_err());
    }
}

#[test]
fn v28_main_resource_invocations_share_lexical_command_and_structure_limits() {
    let operand = TeletextLocalProgramOperandOverlay::new(
        address(16),
        0,
        0,
        TeletextColumnOperation::Foreground {
            colour: TeletextColourReference::new(1).unwrap(),
        },
        TeletextColumnOperation::Foreground {
            colour: TeletextColourReference::new(2).unwrap(),
        },
    )
    .unwrap();
    let overlay = TeletextSavedEditorialOverlay::new_with_local_program_operands(
        provenance(),
        vec![],
        vec![operand],
    )
    .unwrap()
    .with_main_resource_invocations(vec![change()])
    .unwrap();
    let data = saved(&overlay);
    for commands in [true, false] {
        let mut small = budget();
        if commands {
            small.validation.service.max_commands = 1;
        } else {
            small.validation.service.max_structure_items = 1;
        }
        let error = decode_teletext_design_documents(&data, small).unwrap_err();
        assert!(
            matches!(error.reason,TeletextDesignDocumentFailure::Budget { dimension,count:2,maximum:1 }
            if dimension==if commands { "editorial operands" } else { "editorial structure items" })
        );
    }
    let mut value: Value = serde_json::from_slice(&saved(&pure())).unwrap();
    let item = value["editorial_overlay"]["main_resource_invocations"][0].clone();
    value["editorial_overlay"]["main_resource_invocations"] =
        json!([item.clone(), item.clone(), item.clone(), item]);
    let mut small = budget();
    small.validation.service.max_commands = 3;
    assert!(matches!(
        decode_teletext_design_documents(&bytes(&value), small)
            .unwrap_err()
            .reason,
        TeletextDesignDocumentFailure::Budget {
            dimension: "editorial operands",
            count: 4,
            maximum: 3
        }
    ));
}
