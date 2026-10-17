//! V17 call insertion/omission uses the same bounded exact-original wire shape.
use super::*;

fn topology(global: bool, subpage: u8, insert: bool) -> TeletextResourceObjectProgramOverlay {
    let calls = call_change(global, subpage);
    let expected = calls.expected().clone();
    let mut commands = expected.commands().to_vec();
    let mut retained: Vec<_> = (0..commands.len()).map(Some).collect();
    if insert {
        commands.push(invoke(global, 2));
        retained.push(None);
    } else {
        commands.pop();
        retained.pop();
    }
    TeletextResourceObjectProgramOverlay::new_with_invocation_structure(
        calls.identity(),
        expected,
        program(commands),
        retained,
        budget().validation.service,
    )
    .unwrap()
}
fn intent(programs: Vec<TeletextResourceObjectProgramOverlay>) -> TeletextSavedEditorialOverlay {
    TeletextSavedEditorialOverlay::new_with_resource_object_invocation_structure_overlays(
        provenance(),
        vec![],
        vec![],
        vec![],
        vec![],
        vec![],
        (vec![], programs),
    )
    .unwrap()
}
fn concept() -> Value {
    let change = topology(true, 15, true);
    serde_json::from_slice(
        append_teletext_design_editorial_overlay(
            &original_for(&change),
            &intent(vec![change]),
            budget(),
        )
        .unwrap()
        .bytes(),
    )
    .unwrap()
}

#[test]
fn v17_invocation_structure_roundtrips_insert_delete_both_scopes_s1_and_original_bytes() {
    for global in [false, true] {
        for subpage in [0, 15] {
            for insert in [false, true] {
                let change = topology(global, subpage, insert);
                let original = original_for(&change);
                let before = decode_teletext_design_documents(&original, budget()).unwrap();
                let overlay = intent(vec![change]);
                assert_eq!(overlay.schema_version(), 17);
                let encoded =
                    append_teletext_design_editorial_overlay(&original, &overlay, budget())
                        .unwrap();
                let after = decode_teletext_design_documents(encoded.bytes(), budget()).unwrap();
                assert_eq!(after.editorial_overlay(), Some(&overlay));
                assert!(after
                    .editorial_overlay()
                    .unwrap()
                    .resource_object_program_overlays()[0]
                    .has_invocation_structure_changes());
                assert!(!after
                    .editorial_overlay()
                    .unwrap()
                    .resource_object_program_overlays()[0]
                    .has_invocation_changes());
                assert_eq!(after.selection(), before.selection());
                assert_eq!(
                    after.resource_transmissions(),
                    before.resource_transmissions()
                );
                assert_eq!(after.edit_policy(), before.edit_policy());
                for (saved, root) in after.sources().iter().zip(before.sources()) {
                    assert_eq!(saved.bytes(), root.bytes());
                }
                assert_eq!(
                    append_teletext_design_editorial_overlay(
                        &original,
                        after.editorial_overlay().unwrap(),
                        budget()
                    )
                    .unwrap()
                    .bytes(),
                    encoded.bytes()
                );
            }
        }
    }
}

fn combined_original(changes: &[TeletextResourceObjectProgramOverlay]) -> Vec<u8> {
    let mut value: Value = serde_json::from_slice(&original_for(&changes[0])).unwrap();
    let mut source: Value =
        serde_json::from_str(value["documents"][0]["response"].as_str().unwrap()).unwrap();
    for change in changes.iter().skip(1) {
        let identity = change.identity();
        let scope =
            if identity.resource().unwrap().kind() == ersatzrs_domain::TeletextResourceKind::Pop {
                "public"
            } else {
                "global"
            };
        let object = source["service"]["objects"]
            .as_array_mut()
            .unwrap()
            .iter_mut()
            .find(|object| {
                object["identity"]["scope"] == scope
                    && object["identity"]["resource"]["subpage"]
                        == identity.resource().unwrap().logical_subpage().get()
            })
            .unwrap();
        if change
            .expected()
            .commands()
            .iter()
            .any(|command| matches!(command, TeletextEnhancementCommand::InvokeObject { .. }))
        {
            object["object_type"] = json!("active");
        }
        object["program"] = serde_json::to_value(change.expected()).unwrap();
        for command in object["program"]["commands"].as_array_mut().unwrap() {
            if let Some(resource) = command.pointer_mut("/target/resource") {
                let logical = resource
                    .as_object_mut()
                    .unwrap()
                    .remove("logical_subpage")
                    .unwrap();
                resource["subpage"] = logical;
            }
        }
    }
    value["documents"][0]["response"] = json!(serde_json::to_string_pretty(&source).unwrap());
    bytes(&value)
}

#[test]
fn v17_mixes_old_program_roles_but_never_relabels_them_or_downgrades_call_structure() {
    let inserted = topology(true, 15, true);
    let retained = call_change(false, 15);
    let glyphs = TeletextResourceObjectProgramOverlay::new(
        identity(true, 0),
        program(vec![glyph(1)]),
        program(vec![glyph(1), glyph(2)]),
        vec![Some(0), None],
        budget().validation.service,
    )
    .unwrap();
    let changes = vec![inserted.clone(), retained.clone(), glyphs.clone()];
    let original = combined_original(&changes);
    let overlay = intent(changes);
    let encoded = append_teletext_design_editorial_overlay(&original, &overlay, budget()).unwrap();
    assert_eq!(
        decode_teletext_design_documents(encoded.bytes(), budget())
            .unwrap()
            .editorial_overlay(),
        Some(&overlay)
    );
    for constructor in [
        TeletextSavedEditorialOverlay::new_with_resource_object_program_overlays,
        TeletextSavedEditorialOverlay::new_with_resource_object_invocation_overlays,
    ] {
        assert!(constructor(
            provenance(),
            vec![],
            vec![],
            vec![],
            vec![],
            vec![],
            (vec![], vec![inserted.clone(), retained.clone()])
        )
        .is_err());
    }
    for changes in [
        vec![],
        vec![retained.clone()],
        vec![glyphs.clone()],
        vec![retained.clone(), glyphs.clone()],
    ] {
        assert!(
            TeletextSavedEditorialOverlay::new_with_resource_object_invocation_structure_overlays(
                provenance(),
                vec![],
                vec![],
                vec![],
                vec![],
                vec![],
                (vec![], changes),
            )
            .is_err()
        );
    }
    let mut value: Value = serde_json::from_slice(encoded.bytes()).unwrap();
    for version in 1..=16 {
        value["editorial_overlay"]["schema_version"] = json!(version);
        rejects(&value);
    }
    for (version, changes) in [(15, vec![glyphs]), (16, vec![retained])] {
        let constructor = if version == 15 {
            TeletextSavedEditorialOverlay::new_with_resource_object_program_overlays
        } else {
            TeletextSavedEditorialOverlay::new_with_resource_object_invocation_overlays
        };
        let overlay = constructor(
            provenance(),
            vec![],
            vec![],
            vec![],
            vec![],
            vec![],
            (vec![], changes),
        )
        .unwrap();
        let old = append_teletext_design_editorial_overlay(&original, &overlay, budget()).unwrap();
        let restored = decode_teletext_design_documents(old.bytes(), budget()).unwrap();
        assert_eq!(
            restored.editorial_overlay().unwrap().schema_version(),
            version
        );
        assert_eq!(
            append_teletext_design_editorial_overlay(
                &original,
                restored.editorial_overlay().unwrap(),
                budget()
            )
            .unwrap()
            .bytes(),
            old.bytes()
        );
    }
}

#[test]
fn v17_invocation_structure_rejects_closed_fields_bad_maps_and_stale_originals() {
    let baseline = concept();
    for replacement in [Value::Null, json!([]), json!({})] {
        let mut value = baseline.clone();
        value["editorial_overlay"]["resource_object_program_overlays"] = replacement;
        rejects(&value);
    }
    let mut absent = baseline.clone();
    absent["editorial_overlay"]
        .as_object_mut()
        .unwrap()
        .remove("resource_object_program_overlays");
    rejects(&absent);
    for pointer in [
        "/editorial_overlay/resource_object_program_overlays/0",
        "/editorial_overlay/resource_object_program_overlays/0/identity",
        "/editorial_overlay/resource_object_program_overlays/0/replacement/commands/3",
        "/editorial_overlay/resource_object_program_overlays/0/replacement/commands/3/target",
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
    for (pointer,replacement) in [
        ("/editorial_overlay/resource_object_program_overlays/0/retained_commands",Value::Null),
        ("/editorial_overlay/resource_object_program_overlays/0/retained_commands",json!([0,1,2,3])),
        ("/editorial_overlay/resource_object_program_overlays/0/retained_commands",json!([0,1,2,2])),
        ("/editorial_overlay/resource_object_program_overlays/0/retained_commands",json!([0,1,2])),
        ("/editorial_overlay/resource_object_program_overlays/0/expected/commands/2/offset/column",json!(4)),
        ("/editorial_overlay/resource_object_program_overlays/0/identity/resource/logical_subpage",json!(1)),
    ] {let mut value=baseline.clone();*value.pointer_mut(pointer).unwrap()=replacement;rejects(&value);}
    let mut value = baseline;
    let change = value["editorial_overlay"]["resource_object_program_overlays"][0].clone();
    value["editorial_overlay"]["resource_object_program_overlays"]
        .as_array_mut()
        .unwrap()
        .push(change);
    rejects(&value);
}

#[test]
fn v17_invocation_structure_preserves_scalar_order_and_aggregate_preallocation_quotas() {
    let change = topology(true, 15, true);
    let original = original_for(&change);
    let mut expected = change.expected().commands().to_vec();
    expected[0] = foreground(4);
    let mut replacement = change.replacement().commands().to_vec();
    replacement[0] = foreground(4);
    let patched = TeletextResourceObjectProgramOverlay::new_with_invocation_structure(
        change.identity(),
        program(expected),
        program(replacement),
        change.retained_commands().to_vec(),
        budget().validation.service,
    )
    .unwrap();
    let operation =
        |colour| serde_json::from_value(json!({"kind":"foreground","colour":colour})).unwrap();
    let scalar = TeletextResourceObjectOperandOverlay::new(
        change.identity(),
        0,
        1,
        operation(1),
        operation(4),
    )
    .unwrap();
    let overlay =
        TeletextSavedEditorialOverlay::new_with_resource_object_invocation_structure_overlays(
            provenance(),
            vec![],
            vec![],
            vec![],
            vec![],
            vec![],
            (vec![scalar], vec![patched.clone()]),
        )
        .unwrap();
    let encoded = append_teletext_design_editorial_overlay(&original, &overlay, budget()).unwrap();
    assert_eq!(
        decode_teletext_design_documents(encoded.bytes(), budget())
            .unwrap()
            .editorial_overlay(),
        Some(&overlay)
    );
    assert!(
        append_teletext_design_editorial_overlay(&original, &intent(vec![patched]), budget())
            .is_err()
    );
    let other = TeletextResourceObjectProgramOverlay::new(
        identity(false, 15),
        program(vec![glyph(1)]),
        program(vec![glyph(1), glyph(2)]),
        vec![Some(0), None],
        budget().validation.service,
    )
    .unwrap();
    let bundle =
        append_teletext_design_editorial_overlay(&original, &intent(vec![change, other]), budget())
            .unwrap();
    for (commands, items, objects) in [(9, 2048, 64), (2048, 8, 64), (2048, 2048, 1)] {
        let mut limited = budget();
        limited.validation.service.max_commands = commands;
        limited.validation.service.max_structure_items = items;
        limited.validation.service.max_objects = objects;
        assert!(matches!(
            super::super::super::preflight_json(bundle.bytes(), limited)
                .unwrap_err()
                .reason,
            TeletextDesignDocumentFailure::Budget { .. }
        ));
    }
    let mut limited = budget();
    limited.max_serialized_bytes = bundle.bytes().len() - 1;
    assert!(decode_teletext_design_documents(bundle.bytes(), limited).is_err());
}
