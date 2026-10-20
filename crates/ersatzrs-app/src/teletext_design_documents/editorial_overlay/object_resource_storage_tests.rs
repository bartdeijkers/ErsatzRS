//! Complete bank/lifecycle storage and closed prior-role interpretation, not native replay.
use super::base_text_tests::{edit, native_edit, original};
use super::tests::{budget, bytes, provenance};
use super::*;
use crate::teletext_design_documents::{
    TeletextSavedResourceTransmission, TeletextSavedResourceUpdate,
};
use ersatzrs_domain::*;
use serde_json::{json, Value};

fn resource(global: bool, page: u8, subpage: u8) -> TeletextResourceIdentity {
    TeletextResourceIdentity::new(
        if global {
            TeletextResourceKind::Gpop
        } else {
            TeletextResourceKind::Pop
        },
        TeletextRawDataPageNumber::new(2, page).unwrap(),
        TeletextResourceSubpageIndex::new(subpage).unwrap(),
    )
    .unwrap()
}
fn object(resource: TeletextResourceIdentity, id: u16) -> TeletextObjectIdentity {
    if resource.kind() == TeletextResourceKind::Pop {
        TeletextObjectIdentity::public(resource, TeletextObjectId::new(id))
    } else {
        TeletextObjectIdentity::global(resource, TeletextObjectId::new(id))
    }
    .unwrap()
}
pub(super) fn additions() -> Vec<TeletextSavedObjectResourceAddition> {
    let public = resource(false, 0xa2, 0);
    let global = resource(true, 0xa3, 15);
    let definition = |bank, id, object_type, commands| TeletextEnhancementObjectDefinition {
        identity: object(bank, id),
        object_type,
        program: TeletextEnhancementProgram::new(commands).unwrap(),
    };
    [
        (
            global,
            vec![definition(
                global,
                65535,
                TeletextObjectType::Active,
                vec![TeletextEnhancementCommand::InvokeObject {
                    target: object(public, 65534),
                    object_type: TeletextObjectType::Passive,
                    offset: None,
                }],
            )],
        ),
        (
            public,
            vec![definition(
                public,
                65534,
                TeletextObjectType::Passive,
                vec![TeletextEnhancementCommand::AtColumn {
                    column: 1,
                    operation: TeletextColumnOperation::G0 {
                        code: TeletextGlyphCode::new(b'B').unwrap(),
                    },
                }],
            )],
        ),
        (resource(false, 0xa2, 15), vec![]),
    ]
    .into_iter()
    .map(|(resource, definitions)| {
        TeletextSavedObjectResourceAddition::new(
            0,
            TeletextObjectResourceAddition::new(resource, definitions, budget().validation.service)
                .unwrap(),
            TeletextSavedResourceTransmission {
                resource,
                counter: TeletextResourceUpdateCounter::new(7).unwrap(),
                update: TeletextSavedResourceUpdate::Changed,
                last_subpage: resource.logical_subpage().get() == 15,
            },
        )
        .unwrap()
    })
    .collect()
}
fn overlay(prior: u16) -> TeletextSavedEditorialOverlay {
    let previous = match prior {
        1 => TeletextSavedEditorialOverlay::new_with_object_resource_additions(
            provenance(),
            additions(),
        )
        .unwrap(),
        18 => TeletextSavedEditorialOverlay::new_with_resource_object_definition_additions(
            provenance(),
            vec![super::resource_object_addition_tests::addition(false, 15)],
        )
        .unwrap()
        .with_object_resource_additions(additions())
        .unwrap(),
        23 | 24 => TeletextSavedEditorialOverlay::new_with_base_text_edits(
            provenance(),
            vec![if prior == 23 { edit() } else { native_edit() }],
        )
        .unwrap()
        .with_object_resource_additions(additions())
        .unwrap(),
        _ => panic!("test prior role"),
    };
    assert_eq!(previous.schema_version(), 25);
    previous
}
fn saved(prior: u16) -> Vec<u8> {
    append_teletext_design_editorial_overlay(&original(true), &overlay(prior), budget())
        .unwrap()
        .into_bytes()
}

#[test]
fn v25_object_resources_keep_original_sources_lifecycle_and_prior_native_roles() {
    let source = original(true);
    let before = decode_teletext_design_documents(&source, budget()).unwrap();
    for prior in [1, 18, 23, 24] {
        let overlay = overlay(prior);
        let encoded =
            append_teletext_design_editorial_overlay(&source, &overlay, budget()).unwrap();
        let end = source
            .iter()
            .rposition(|v| !v.is_ascii_whitespace())
            .unwrap();
        assert_eq!(&encoded.bytes()[..end], &source[..end]);
        assert!(encoded.bytes().ends_with(&source[end..]));
        let value: Value = serde_json::from_slice(encoded.bytes()).unwrap();
        assert_eq!(value["editorial_overlay"]["prior_schema_version"], prior);
        let after = decode_teletext_design_documents(encoded.bytes(), budget()).unwrap();
        assert_eq!(after.editorial_overlay(), Some(&overlay));
        assert_eq!(after.selection(), before.selection());
        assert_eq!(
            after.resource_transmissions(),
            before.resource_transmissions()
        );
        assert_eq!(after.sources()[0].bytes(), before.sources()[0].bytes());
        assert_eq!(
            after.sources()[0].authoring(),
            before.sources()[0].authoring()
        );
        let TeletextProfileDocument::Authored(service) = &after.documents()[0] else {
            panic!("original authored document")
        };
        assert!(
            !service
                .resource_inventory()
                .contains(&additions()[0].addition().resource()),
            "stored originals must not become edited output"
        );
        let again = append_teletext_design_editorial_overlay(
            &source,
            after.editorial_overlay().unwrap(),
            budget(),
        )
        .unwrap();
        assert_eq!(again.bytes(), encoded.bytes());
    }
    // Adding later Base/native spelling must retain the enclosing bank role.
    for edit in [edit(), native_edit()] {
        let combined = overlay(1).with_base_text_edits(vec![edit]).unwrap();
        assert_eq!(combined.schema_version(), 25);
        let data = append_teletext_design_editorial_overlay(&source, &combined, budget()).unwrap();
        assert_eq!(
            decode_teletext_design_documents(data.bytes(), budget())
                .unwrap()
                .editorial_overlay(),
            Some(&combined)
        );
    }
}

#[test]
fn v25_object_resources_refuse_version_downgrade_null_unknown_and_ambiguous_source_data() {
    let original: Value = serde_json::from_slice(&saved(24)).unwrap();
    for version in (0..=24).chain([26, u16::MAX]) {
        let mut changed = original.clone();
        changed["editorial_overlay"]["schema_version"] = json!(version);
        assert!(decode_teletext_design_documents(&bytes(&changed), budget()).is_err());
    }
    for prior in [json!(0), json!(25), json!(u16::MAX), Value::Null] {
        let mut changed = original.clone();
        changed["editorial_overlay"]["prior_schema_version"] = prior;
        assert!(decode_teletext_design_documents(&bytes(&changed), budget()).is_err());
    }
    for key in ["prior_schema_version", "object_resource_additions"] {
        let mut changed = original.clone();
        changed["editorial_overlay"]
            .as_object_mut()
            .unwrap()
            .remove(key);
        assert!(decode_teletext_design_documents(&bytes(&changed), budget()).is_err());
    }
    let mut candidates = Vec::new();
    let mut changed = original.clone();
    changed["editorial_overlay"]["object_resource_additions"] = json!([]);
    candidates.push(changed);
    let mut changed = original.clone();
    changed["editorial_overlay"]["object_resource_additions"] = Value::Null;
    candidates.push(changed);
    for (pointer, value) in [
        (
            "/editorial_overlay/object_resource_additions/0/document_index",
            json!(1),
        ),
        (
            "/editorial_overlay/object_resource_additions/0/resource/logical_subpage",
            json!(0),
        ),
        (
            "/editorial_overlay/object_resource_additions/0/transmission/counter",
            json!(8),
        ),
        (
            "/editorial_overlay/object_resource_additions/0/transmission/update",
            json!("automatic"),
        ),
        (
            "/editorial_overlay/object_resource_additions/0/definitions/0/identity/object_id",
            Value::Null,
        ),
        (
            "/editorial_overlay/object_resource_additions/0/definitions/0",
            json!([]),
        ),
    ] {
        let mut changed = original.clone();
        *changed.pointer_mut(pointer).unwrap() = value;
        candidates.push(changed);
    }
    let mut changed = original.clone();
    changed["editorial_overlay"]["object_resource_additions"][0]["extra"] = json!(true);
    candidates.push(changed);
    for value in candidates {
        assert!(decode_teletext_design_documents(&bytes(&value), budget()).is_err());
    }
    let compact = serde_json::to_string(&original).unwrap();
    let duplicate = compact.replacen(
        "\"document_index\":0",
        "\"document_index\":0,\"document_index\":0",
        1,
    );
    assert_ne!(duplicate, compact);
    assert!(decode_teletext_design_documents(duplicate.as_bytes(), budget()).is_err());
    // Removing all new fields cannot reinterpret the bank-only envelope as V1.
    let mut empty: Value = serde_json::from_slice(&saved(1)).unwrap();
    let role = empty["editorial_overlay"].as_object_mut().unwrap();
    role.remove("object_resource_additions");
    role.remove("prior_schema_version");
    role.insert("schema_version".into(), json!(1));
    assert!(decode_teletext_design_documents(&bytes(&empty), budget()).is_err());
}

#[test]
fn v25_object_resources_bound_shared_original_new_lifecycle_and_old_new_programs() {
    let value: Value = serde_json::from_slice(&saved(18)).unwrap();
    let mut limits = budget();
    limits.max_resource_transmissions = 3;
    for first in [false, true] {
        let mut fields = value.as_object().unwrap().clone();
        let transmissions = fields.remove("resource_transmissions").unwrap();
        let rest = serde_json::to_string(&fields).unwrap();
        let text = if first {
            format!(
                "{{\"resource_transmissions\":{},{}",
                transmissions,
                &rest[1..]
            )
        } else {
            format!(
                "{},\"resource_transmissions\":{}}}",
                &rest[..rest.len() - 1],
                transmissions
            )
        };
        let error = decode_teletext_design_documents(text.as_bytes(), limits).unwrap_err();
        assert!(matches!(
            error.reason,
            TeletextDesignDocumentFailure::Budget {
                dimension: "resource transmissions",
                ..
            }
        ));
    }
    for (objects, commands, dimension) in [
        (2, 2048, "editorial object programs"),
        (64, 2, "editorial operands"),
    ] {
        let mut limits = budget();
        limits.validation.service.max_objects = objects;
        limits.validation.service.max_commands = commands;
        let error = decode_teletext_design_documents(&bytes(&value), limits).unwrap_err();
        assert!(
            matches!(error.reason, TeletextDesignDocumentFailure::Budget { dimension: actual, .. } if actual == dimension)
        );
    }
    let mut limits = budget();
    limits.max_serialized_bytes = saved(18).len() - 1;
    assert!(
        append_teletext_design_editorial_overlay(&original(true), &overlay(18), limits).is_err()
    );
}

#[test]
fn v25_object_resources_preserve_native_programs_beyond_editor_local_frame_capacity() {
    let mut records = additions();
    let original = &records[1];
    let mut definition = original.addition().definitions()[0].clone();
    let command = definition.program.commands()[0];
    // A native resource body is not an editor-only Local frame. This is storage
    // fidelity, not native packing, QTM capacity or independent decoding proof.
    definition.program = TeletextEnhancementProgram::new(vec![command; 229]).unwrap();
    records[1] = TeletextSavedObjectResourceAddition::new(
        original.document_index(),
        TeletextObjectResourceAddition::new(
            original.addition().resource(),
            vec![definition],
            budget().validation.service,
        )
        .unwrap(),
        original.transmission(),
    )
    .unwrap();
    let overlay = TeletextSavedEditorialOverlay::new_with_object_resource_additions(
        provenance(),
        records.clone(),
    )
    .unwrap();
    let encoded =
        append_teletext_design_editorial_overlay(&self::original(true), &overlay, budget())
            .unwrap();
    let after = decode_teletext_design_documents(encoded.bytes(), budget()).unwrap();
    let restored = after
        .editorial_overlay()
        .unwrap()
        .object_resource_additions();
    assert_eq!(restored, records);
    let commands = restored[1].addition().definitions()[0].program.commands();
    assert_eq!(commands.len(), 229);
    assert!(commands.iter().all(|value| value == &command));
}

#[test]
fn v25_object_resources_refuse_existing_banks_foreign_lifecycle_and_base_only_owners() {
    let source = original(true);
    let old = resource(false, 0xa0, 15);
    let record = |bank| TeletextSavedResourceTransmission {
        resource: bank,
        counter: TeletextResourceUpdateCounter::new(0).unwrap(),
        update: TeletextSavedResourceUpdate::Unchanged,
        last_subpage: false,
    };
    let added =
        TeletextObjectResourceAddition::new(old, vec![], budget().validation.service).unwrap();
    let duplicate = TeletextSavedObjectResourceAddition::new(0, added, record(old)).unwrap();
    let overlay = TeletextSavedEditorialOverlay::new_with_object_resource_additions(
        provenance(),
        vec![duplicate],
    )
    .unwrap();
    assert!(append_teletext_design_editorial_overlay(&source, &overlay, budget()).is_err());
    assert!(append_teletext_design_editorial_overlay(
        &original(false),
        &self::overlay(1),
        budget()
    )
    .is_err());
    let addition = additions().remove(0);
    assert!(
        TeletextSavedObjectResourceAddition::new(0, addition.addition().clone(), record(old))
            .is_err()
    );
    let doubled = vec![addition.clone(), addition];
    assert!(
        TeletextSavedEditorialOverlay::new_with_object_resource_additions(provenance(), doubled)
            .is_err()
    );
}

#[test]
fn v25_stores_coordinated_graph_without_validating_an_incomplete_prior_role() {
    let source = original(true);
    let decoded = decode_teletext_design_documents(&source, budget()).unwrap();
    let TeletextProfileDocument::Authored(service) = &decoded.documents()[0] else {
        panic!("authored source")
    };
    let banks = additions();
    let mut definition = super::resource_object_addition_tests::addition(false, 15)
        .definition()
        .clone();
    definition.object_type = TeletextObjectType::Adaptive;
    definition.program =
        TeletextEnhancementProgram::new(vec![TeletextEnhancementCommand::InvokeObject {
            target: banks[1].addition().definitions()[0].identity,
            object_type: TeletextObjectType::Passive,
            offset: None,
        }])
        .unwrap();
    let addition =
        TeletextResourceObjectDefinitionAddition::new(definition, budget().validation.service)
            .unwrap();
    assert!(service
        .as_ref()
        .clone()
        .with_resource_object_definition_additions(
            std::slice::from_ref(&addition),
            budget().validation.service
        )
        .is_err());
    let complete_banks = banks
        .iter()
        .map(|bank| bank.addition().clone())
        .collect::<Vec<_>>();
    service
        .as_ref()
        .clone()
        .with_object_resource_and_definition_additions(
            &complete_banks,
            std::slice::from_ref(&addition),
            budget().validation.service,
            budget().validation.declarations,
        )
        .unwrap();
    let prior = TeletextSavedEditorialOverlay::new_with_resource_object_definition_additions(
        provenance(),
        vec![addition],
    )
    .unwrap();
    assert!(
        append_teletext_design_editorial_overlay(&source, &prior, budget()).is_err(),
        "legacy standalone role still requires its complete original-bank graph"
    );
    let overlay = prior.with_object_resource_additions(banks).unwrap();
    let encoded = append_teletext_design_editorial_overlay(&source, &overlay, budget()).unwrap();
    let restored = decode_teletext_design_documents(encoded.bytes(), budget()).unwrap();
    assert_eq!(restored.editorial_overlay(), Some(&overlay));
    assert_eq!(restored.sources()[0].bytes(), decoded.sources()[0].bytes());
    assert_eq!(
        restored.resource_transmissions(),
        decoded.resource_transmissions()
    );
}
