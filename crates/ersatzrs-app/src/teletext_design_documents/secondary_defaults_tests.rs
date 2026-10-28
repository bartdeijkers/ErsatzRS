//! Storage/association proofs only; no current author or publication authority.
use super::*;
use ersatzrs_domain::{
    TeletextPageCharsetDesignation, TeletextSecondaryDefaultAgreement,
    TeletextSecondaryDefaultChoice,
};

fn agreement(secondary: Option<u8>) -> TeletextSecondaryDefaultAgreement {
    TeletextSecondaryDefaultAgreement::new(secondary.map_or(
        TeletextSecondaryDefaultChoice::NoSecondG0 {},
        |value| TeletextSecondaryDefaultChoice::Designated {
            designation: TeletextPageCharsetDesignation::new(value).unwrap(),
        },
    ))
}

#[test]
fn context_replacement_and_withdrawal_preserve_original_bytes_and_metadata() {
    let fixture = Fixture::with_authoring();
    let original = fixture.encoded();
    let mut current = original.bytes().to_vec();
    for secondary in [Some(1), Some(0x10)] {
        let intents = [binding_intent(&fixture, 1, secondary)];
        let saved =
            replace_teletext_design_secondary_default_agreements(&current, &intents, budget())
                .unwrap();
        let restored = decode_teletext_design_documents(saved.bytes(), budget()).unwrap();
        assert_eq!(restored.secondary_default_agreements(), intents);
        assert_eq!(restored.selection(), &fixture.selection);
        for (source, expected) in restored.sources().iter().zip(&fixture.sources) {
            assert_eq!(source.bytes(), expected.bytes());
            assert_eq!(source.authoring(), expected.authoring());
        }
        assert_eq!(
            replace_teletext_design_secondary_default_agreements(saved.bytes(), &intents, budget())
                .unwrap()
                .bytes(),
            saved.bytes()
        );
        current = saved.into_bytes();
    }
    let cleared =
        replace_teletext_design_secondary_default_agreements(&current, &[], budget()).unwrap();
    let restored = decode_teletext_design_documents(cleared.bytes(), budget()).unwrap();
    assert!(restored.secondary_default_agreements().is_empty());
    assert!(serde_json::from_slice::<Value>(cleared.bytes())
        .unwrap()
        .get("secondary_default_agreements")
        .is_none());
    for (source, expected) in restored.sources().iter().zip(&fixture.sources) {
        assert_eq!(source.bytes(), expected.bytes());
        assert_eq!(source.authoring(), expected.authoring());
    }
    assert_eq!(
        replace_teletext_design_secondary_default_agreements(original.bytes(), &[], budget())
            .unwrap()
            .bytes(),
        original.bytes()
    );
    assert!(replace_teletext_design_secondary_default_agreements(
        original.bytes(),
        &[TeletextSavedSecondaryDefaultAgreement::new(
            1,
            99,
            1,
            agreement(None)
        )],
        budget()
    )
    .is_err());
}

#[test]
fn context_only_receipt_has_its_own_closed_version_without_fake_body_edits() {
    let fixture = Fixture::mixed();
    let original = fixture.encoded();
    let provenance = TeletextSavedEditorialProvenance {
        export_receipt_id: 17,
        export_receipt_digest: "a".repeat(64),
        original_design_id: 9,
        original_revision: 3,
        original_digest: "b".repeat(64),
    };
    let overlay =
        TeletextSavedEditorialOverlay::new_secondary_default_context(provenance.clone()).unwrap();
    let saved =
        append_teletext_design_editorial_overlay(original.bytes(), &overlay, budget()).unwrap();
    let restored = decode_teletext_design_documents(saved.bytes(), budget()).unwrap();
    let actual = restored.editorial_overlay().unwrap();
    assert_eq!(actual.schema_version(), 32);
    assert_eq!(actual.provenance(), &provenance);
    assert!(actual.pages().is_empty());
    assert!(restored.secondary_default_agreements().is_empty());
    let value: Value = serde_json::from_slice(saved.bytes()).unwrap();
    for field in [
        "local_program_operands",
        "drcs_bitmap_overlays",
        "local_object_operands",
        "local_object_glyph_program_overlays",
        "main_local_invocation_program_overlays",
        "main_resource_invocations",
        "main_program_presence_overlays",
        "resource_object_operands",
        "resource_object_program_overlays",
        "resource_object_definition_additions",
        "object_resource_additions",
        "object_subpage_announcements",
        "object_page_selections",
        "base_text_edits",
    ] {
        let mut changed = value.clone();
        changed["editorial_overlay"][field] = json!([]);
        assert!(decode(&changed).is_err(), "accepted {field}");
    }
    for version in [1, 33, u16::MAX] {
        let mut changed = value.clone();
        changed["editorial_overlay"]["schema_version"] = json!(version);
        assert!(decode(&changed).is_err(), "accepted version {version}");
    }
}
fn binding_intent(
    fixture: &Fixture,
    producer: usize,
    secondary: Option<u8>,
) -> TeletextSavedSecondaryDefaultAgreement {
    let selected = &fixture.selection.bindings[producer];
    TeletextSavedSecondaryDefaultAgreement::new(
        producer,
        selected.profile_id,
        selected.profile_revision,
        agreement(secondary),
    )
}

#[test]
fn explicit_agreements_preserve_originals_profiles_and_unknown_contexts() {
    let fixture = Fixture::with_authoring();
    let original = fixture.encoded();
    assert!(decode_teletext_design_documents(original.bytes(), budget())
        .unwrap()
        .secondary_default_agreements()
        .is_empty());
    for secondary in [None, Some(1), Some(0x10)] {
        let intents = [
            binding_intent(&fixture, 0, secondary),
            binding_intent(&fixture, 1, secondary),
        ];
        let saved = append_teletext_design_secondary_default_agreements(
            original.bytes(),
            &intents,
            budget(),
        )
        .unwrap();
        let restored = decode_teletext_design_documents(saved.bytes(), budget()).unwrap();
        assert_eq!(restored.secondary_default_agreements(), intents);
        assert_eq!(restored.selection(), &fixture.selection);
        assert_eq!(
            restored.edit_policy(),
            TeletextSavedEditPolicy::OriginalOnly
        );
        assert!(restored.editorial_overlay().is_none());
        for (source, expected) in restored.sources().iter().zip(&fixture.sources) {
            assert_eq!(source.bytes(), expected.bytes());
            assert_eq!(source.authoring(), expected.authoring());
        }
        let end = original
            .bytes()
            .iter()
            .rposition(|b| !b.is_ascii_whitespace())
            .unwrap();
        assert!(saved.bytes().starts_with(&original.bytes()[..end]));
        assert!(saved.bytes().ends_with(&original.bytes()[end..]));
        assert_eq!(
            EncodedTeletextDesignDocuments::from_saved_bytes(saved.bytes().to_vec(), budget())
                .unwrap()
                .bytes(),
            saved.bytes()
        );
        assert!(append_teletext_design_secondary_default_agreements(
            saved.bytes(),
            &intents,
            budget()
        )
        .is_err());
        // Returning to the retained root clears intent without guessing defaults.
        assert_eq!(fixture.encoded().bytes(), original.bytes());
    }
}

#[test]
fn saved_agreements_reject_foreign_duplicate_unsorted_or_empty_binding_intent() {
    let fixture = Fixture::mixed();
    let original = fixture.encoded();
    let a = binding_intent(&fixture, 0, None);
    let b = binding_intent(&fixture, 1, Some(1));
    for invalid in [
        vec![],
        vec![a, a],
        vec![b, a],
        vec![TeletextSavedSecondaryDefaultAgreement::new(
            2,
            12,
            4,
            agreement(None),
        )],
        vec![TeletextSavedSecondaryDefaultAgreement::new(
            0,
            12,
            4,
            agreement(None),
        )],
        vec![TeletextSavedSecondaryDefaultAgreement::new(
            0,
            11,
            5,
            agreement(None),
        )],
    ] {
        assert!(append_teletext_design_secondary_default_agreements(
            original.bytes(),
            &invalid,
            budget()
        )
        .is_err());
    }
}

#[test]
fn companion_is_closed_versioned_required_and_never_an_implicit_default() {
    let fixture = Fixture::mixed();
    let saved = append_teletext_design_secondary_default_agreements(
        fixture.encoded().bytes(),
        &[binding_intent(&fixture, 0, None)],
        budget(),
    )
    .unwrap();
    let baseline: Value = serde_json::from_slice(saved.bytes()).unwrap();
    for case in 0..20 {
        let mut value = baseline.clone();
        let c = &mut value["secondary_default_agreements"];
        match case {
            0 => *c = Value::Null,
            1 => *c = json!([]),
            2 => c["schema_version"] = json!(2),
            3 => {
                c.as_object_mut().unwrap().remove("schema_version");
            }
            4 => c["unknown"] = json!(true),
            5 => c["bindings"] = json!([]),
            6 => c["bindings"] = Value::Null,
            7 => c["bindings"][0]["agreement"] = Value::Null,
            8 => {
                c["bindings"][0]
                    .as_object_mut()
                    .unwrap()
                    .remove("agreement");
            }
            9 => c["bindings"][0]["agreement"]["policy"] = json!("future-policy"),
            10 => c["bindings"][0]["agreement"]["choice"] = Value::Null,
            11 => c["bindings"][0]["agreement"]["choice"]["designation"] = json!(1),
            12 => {
                c["bindings"][0]["agreement"]["choice"] =
                    json!({"kind":"designated","designation":127});
            }
            13 => c["bindings"][0]["profile_id"] = json!(12),
            14 => c["bindings"][0]["profile_revision"] = json!(5),
            15 => c["bindings"][0]["producer"] = json!(2),
            16 => c["bindings"][0]["unexpected"] = json!(true),
            17 => {
                c["bindings"][0] = json!([0,11,4,{"policy":"ersatzrs-secondary-default-v1","choice":{"kind":"no_second_g0"}}]);
            }
            18 => *c = json!([1, []]),
            _ => {
                c["bindings"][0]
                    .as_object_mut()
                    .unwrap()
                    .remove("profile_revision");
            }
        }
        assert!(decode(&value).is_err(), "accepted case {case}");
    }
    let text = std::str::from_utf8(saved.bytes()).unwrap();
    for (needle, replacement) in [
        (
            "\"schema_version\":1,\"bindings\"",
            "\"schema_version\":1,\"schema_version\":1,\"bindings\"",
        ),
        ("\"producer\":0", "\"producer\":0,\"producer\":0"),
    ] {
        let changed = text.replace(needle, replacement);
        assert_ne!(changed, text);
        assert!(decode_teletext_design_documents(changed.as_bytes(), budget()).is_err());
    }
}

#[test]
fn agreement_storage_obeys_existing_serialized_and_preallocation_budgets() {
    let fixture = Fixture::mixed();
    let intents = [
        binding_intent(&fixture, 0, None),
        binding_intent(&fixture, 1, Some(1)),
    ];
    let original = fixture.encoded();
    let saved =
        append_teletext_design_secondary_default_agreements(original.bytes(), &intents, budget())
            .unwrap();
    let tight = TeletextDesignDocumentBudget {
        max_serialized_bytes: saved.bytes().len() - 1,
        ..budget()
    };
    assert!(
        append_teletext_design_secondary_default_agreements(original.bytes(), &intents, tight)
            .is_err()
    );
    let mut value: Value = serde_json::from_slice(saved.bytes()).unwrap();
    value["secondary_default_agreements"]["bindings"] = json!(vec![
            value["secondary_default_agreements"]["bindings"][0]
                .clone();
            9
        ]);
    assert!(matches!(
        decode(&value).unwrap_err().reason,
        TeletextDesignDocumentFailure::Budget {
            dimension: "secondary default bindings",
            count: 9,
            maximum: 8
        }
    ));
}
