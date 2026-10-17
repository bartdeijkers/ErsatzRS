//! Existing resource call edits reuse complete closure and consumer validation.
use super::*;

#[path = "resource_object_invocation_structure_tests.rs"]
mod structure;

fn call(
    target: TeletextObjectIdentity,
    object_type: TeletextObjectType,
    column: u8,
) -> TeletextEnhancementCommand {
    TeletextEnhancementCommand::InvokeObject {
        target,
        object_type,
        offset: Some(TeletextObjectOffset::new(0, column).unwrap()),
    }
}

fn nested_input() -> TeletextResolvedServiceDraft {
    let mut source = input();
    source.objects[0].object_type = TeletextObjectType::Active;
    let mut commands = source.objects[0].program.commands().to_vec();
    commands.insert(0, call(target(true, 2), TeletextObjectType::Passive, 0));
    source.objects[0].program = program(commands);
    for page in &mut source.pages {
        page.local_program = Some(program(vec![
            invoke(target(false, 1), TeletextObjectType::Active),
            invoke(target(true, 2), TeletextObjectType::Passive),
        ]));
    }
    source.objects.push(definition(
        target(false, 3),
        TeletextObjectType::Passive,
        (0..20).map(|column| at(column, glyph(b'B'))).collect(),
    ));
    source.objects.push(definition(
        target(true, 4),
        TeletextObjectType::Adaptive,
        vec![at(0, glyph(b'C'))],
    ));
    source
}

fn change_call(
    source: &TeletextResolvedServiceDraft,
    replacement: TeletextEnhancementCommand,
) -> TeletextResourceObjectProgramOverlay {
    let expected = source.objects[0].program.clone();
    let mut commands = expected.commands().to_vec();
    commands[0] = replacement;
    TeletextResourceObjectProgramOverlay::new_with_invocation_changes(
        source.objects[0].identity,
        expected,
        program(commands),
        (0..source.objects[0].program.commands().len())
            .map(Some)
            .collect(),
        budget(),
    )
    .unwrap()
}

#[test]
fn explicit_resource_call_policy_accepts_offset_target_and_type_without_widening_legacy() {
    let source = nested_input();
    let original = service(source.clone());
    for replacement in [
        call(target(true, 2), TeletextObjectType::Passive, 1),
        invoke(target(true, 2), TeletextObjectType::Passive),
        call(target(false, 3), TeletextObjectType::Passive, 0),
        call(target(true, 4), TeletextObjectType::Adaptive, 0),
    ] {
        let change = change_call(&source, replacement);
        assert!(change.has_invocation_changes());
        assert_eq!(change.identity(), target(false, 1));
        assert_eq!(
            change.retained_commands(),
            [Some(0), Some(1), Some(2), Some(3)]
        );
        assert!(matches!(
            TeletextResourceObjectProgramOverlay::new(
                change.identity(),
                change.expected().clone(),
                change.replacement().clone(),
                change.retained_commands().to_vec(),
                budget()
            ),
            Err(TeletextResourceObjectProgramOverlayError::Correspondence(
                TeletextLocalObjectGlyphProgramOverlayError::ProtectedCommand
            ))
        ));
        let local = TeletextObjectIdentity::local(address(200, 0), TeletextObjectId::new(19));
        assert_eq!(
            TeletextLocalObjectGlyphProgramOverlay::new_with_style_commands(
                local,
                change.expected().clone(),
                change.replacement().clone(),
                change.retained_commands().to_vec()
            ),
            Err(TeletextLocalObjectGlyphProgramOverlayError::ProtectedCommand)
        );
        let edited = original
            .clone()
            .with_resource_object_program_overlays(std::slice::from_ref(&change), budget())
            .unwrap();
        let mut expected = source.clone();
        expected.objects[0].program = change.replacement().clone();
        assert_eq!(edited, service(expected));
        assert_eq!(edited.base(), original.base());
        assert_eq!(edited.drcs(), original.drcs());
        assert_eq!(edited.object_companions(), original.object_companions());
        assert_eq!(
            edited.physical_base_overlays(),
            original.physical_base_overlays()
        );
        let TeletextEnhancementCommand::InvokeObject {
            target: selected, ..
        } = replacement
        else {
            unreachable!()
        };
        for owner in [address(200, 0), address(200, 1)] {
            assert!(edited
                .collected(owner)
                .unwrap()
                .1
                .object_references()
                .contains(&TeletextCollectedObjectInvocation {
                    target: selected,
                    source_location: TeletextEnhancementSource {
                        object: Some(change.identity()),
                        command: 0,
                    },
                }));
        }
    }
}

#[test]
fn resource_call_policy_requires_actual_retained_call_change_and_preserves_all_other_anchors() {
    let before = call(target(true, 2), TeletextObjectType::Passive, 0);
    let after = call(target(true, 2), TeletextObjectType::Passive, 1);
    let expected = program(vec![before, charset(0), at(0, glyph(b'A'))]);
    let unchanged_call = program(vec![before, charset(0), at(0, glyph(b'B'))]);
    let retained = vec![Some(0), Some(1), Some(2)];
    let legacy = TeletextResourceObjectProgramOverlay::new(
        target(false, 1),
        expected.clone(),
        unchanged_call.clone(),
        retained.clone(),
        budget(),
    )
    .unwrap();
    assert!(!legacy.has_invocation_changes());
    assert_eq!(
        TeletextResourceObjectProgramOverlay::new_with_invocation_changes(
            target(false, 1),
            expected.clone(),
            unchanged_call,
            retained.clone(),
            budget()
        ),
        Err(TeletextResourceObjectProgramOverlayError::MissingInvocationChange)
    );
    let bundled = TeletextResourceObjectProgramOverlay::new_with_invocation_changes(
        target(false, 1),
        expected.clone(),
        program(vec![
            at(0, fg(2)),
            after,
            charset(0),
            at(1, glyph(b'B')),
            at(2, glyph(b'C')),
        ]),
        vec![None, Some(0), Some(1), Some(2), None],
        budget(),
    )
    .unwrap();
    assert!(bundled.has_invocation_changes());
    assert_eq!(bundled.replacement_command(0), Some(1));
    assert_eq!(
        TeletextResourceObjectProgramOverlay::new_with_invocation_changes(
            target(false, 1),
            expected.clone(),
            expected,
            retained,
            budget()
        ),
        Err(TeletextResourceObjectProgramOverlayError::Unchanged)
    );
}

#[test]
fn resource_call_policy_refuses_insert_delete_bad_maps_and_changed_protected_anchors() {
    let before = call(target(true, 2), TeletextObjectType::Passive, 0);
    let after = call(target(true, 2), TeletextObjectType::Passive, 1);
    let expected = program(vec![before, charset(0), at(0, glyph(b'A'))]);
    let retained = vec![Some(0), Some(1), Some(2)];
    for (commands, retained) in [
        (
            vec![after, before, charset(0), at(0, glyph(b'A'))],
            vec![Some(0), None, Some(1), Some(2)],
        ),
        (vec![charset(0), at(0, glyph(b'A'))], vec![Some(1), Some(2)]),
        (
            vec![after, charset(1), at(0, glyph(b'A'))],
            retained.clone(),
        ),
        (
            vec![after, charset(0), at(0, op(json!({"kind":"g2","code":65})))],
            retained.clone(),
        ),
        (
            vec![after, charset(0), at(0, glyph(b'A'))],
            vec![Some(0), Some(0), Some(2)],
        ),
    ] {
        assert!(
            TeletextResourceObjectProgramOverlay::new_with_invocation_changes(
                target(false, 1),
                expected.clone(),
                program(commands),
                retained,
                budget()
            )
            .is_err()
        );
    }
}

#[test]
fn resource_call_reconstruction_rejects_missing_wrong_type_and_self_cycle_edges() {
    let source = nested_input();
    let original = service(source.clone());
    for (replacement, reason) in [
        (
            call(target(false, 9), TeletextObjectType::Passive, 0),
            TeletextObjectInvocationError::Missing {
                target: target(false, 9),
            },
        ),
        (
            call(target(true, 2), TeletextObjectType::Adaptive, 0),
            TeletextObjectInvocationError::Type {
                target: target(true, 2),
                expected: TeletextObjectType::Adaptive,
                actual: TeletextObjectType::Passive,
            },
        ),
        // Permitted call types strictly descend, so a cycle necessarily has an
        // illegal edge; it cannot become a recursive collector expansion.
        (
            call(target(false, 1), TeletextObjectType::Active, 0),
            TeletextObjectInvocationError::TypeEdge {
                caller: TeletextObjectType::Active,
                target: target(false, 1),
                target_type: TeletextObjectType::Active,
            },
        ),
    ] {
        let change = change_call(&source, replacement);
        assert_eq!(
            original
                .clone()
                .with_resource_object_program_overlays(&[change], budget()),
            Err(TeletextServiceError::Objects(
                TeletextObjectCatalogError::Invocation {
                    owner: target(false, 1),
                    command: 0,
                    reason,
                }
            ))
        );
    }
}

#[test]
fn resource_call_reconstruction_rejects_local_targets_and_preserves_complete_consumer_budgets() {
    let mut source = nested_input();
    let local = TeletextObjectIdentity::local(address(200, 0), TeletextObjectId::new(29));
    source
        .objects
        .push(definition(local, TeletextObjectType::Passive, vec![]));
    let original = service(source.clone());
    let change = change_call(&source, call(local, TeletextObjectType::Passive, 0));
    assert_eq!(
        original
            .clone()
            .with_resource_object_program_overlays(&[change], budget()),
        Err(TeletextServiceError::Objects(
            TeletextObjectCatalogError::Invocation {
                owner: target(false, 1),
                command: 0,
                reason: TeletextObjectInvocationError::ResourceToLocal {
                    caller: target(false, 1),
                    target: local
                },
            }
        ))
    );
    let change = change_call(
        &source,
        call(target(false, 3), TeletextObjectType::Passive, 0),
    );
    let mut collected = budget();
    collected.max_collected_commands = [address(200, 0), address(200, 1)]
        .iter()
        .map(|owner| original.collected(*owner).unwrap().1.command_count())
        .sum();
    assert!(matches!(
        original.with_resource_object_program_overlays(std::slice::from_ref(&change), collected),
        Err(TeletextServiceError::Collection { .. })
    ));
    let commands = change.expected().commands().len() + change.replacement().commands().len();
    let items = change.retained_commands().len();
    for (max_commands, max_structure_items) in [(commands - 1, items), (commands, items - 1)] {
        let mut limits = budget();
        limits.max_commands = max_commands;
        limits.max_structure_items = max_structure_items;
        assert_eq!(
            TeletextResourceObjectProgramOverlay::new_with_invocation_changes(
                change.identity(),
                change.expected().clone(),
                change.replacement().clone(),
                change.retained_commands().to_vec(),
                limits
            ),
            Err(TeletextResourceObjectProgramOverlayError::Budget)
        );
    }
}
