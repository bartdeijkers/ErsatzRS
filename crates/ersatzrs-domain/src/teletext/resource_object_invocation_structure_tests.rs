//! Resource call insertion/omission is additive to the retained-call contract.
use super::*;

fn topology(
    source: &TeletextResolvedServiceDraft,
    insert: bool,
) -> TeletextResourceObjectProgramOverlay {
    let expected = source.objects[0].program.clone();
    let mut commands = expected.commands().to_vec();
    let mut retained: Vec<_> = (0..commands.len()).map(Some).collect();
    if insert {
        commands.insert(1, call(target(true, 2), TeletextObjectType::Passive, 1));
        retained.insert(1, None);
    } else {
        commands.remove(0);
        retained.remove(0);
    }
    TeletextResourceObjectProgramOverlay::new_with_invocation_structure(
        source.objects[0].identity,
        expected,
        program(commands),
        retained,
        budget(),
    )
    .unwrap()
}

#[test]
fn resource_invocation_structure_accepts_insert_delete_and_rebuilds_all_consumers() {
    let source = nested_input();
    let original = service(source.clone());
    for insert in [false, true] {
        let change = topology(&source, insert);
        assert!(change.has_invocation_structure_changes());
        assert!(!change.has_invocation_changes());
        let edited = original
            .clone()
            .with_resource_object_program_overlays(std::slice::from_ref(&change), budget())
            .unwrap();
        let mut expected = source.clone();
        expected.objects[0].program = change.replacement().clone();
        assert_eq!(edited, service(expected));
        assert_eq!(edited.base(), original.base());
        assert_eq!(edited.drcs(), original.drcs());
        assert_eq!(
            edited.physical_base_overlays(),
            original.physical_base_overlays()
        );
        for owner in [address(200, 0), address(200, 1)] {
            let old = original
                .collected(owner)
                .unwrap()
                .1
                .invocation_count(TeletextObjectType::Passive);
            assert_eq!(
                edited
                    .collected(owner)
                    .unwrap()
                    .1
                    .invocation_count(TeletextObjectType::Passive),
                if insert { old + 1 } else { old - 1 }
            );
        }
        let inverse_map = (0..change.expected().commands().len())
            .map(|ordinal| change.replacement_command(ordinal))
            .collect();
        let inverse = TeletextResourceObjectProgramOverlay::new_with_invocation_structure(
            change.identity(),
            change.replacement().clone(),
            change.expected().clone(),
            inverse_map,
            budget(),
        )
        .unwrap();
        assert_eq!(
            edited
                .with_resource_object_program_overlays(&[inverse], budget())
                .unwrap(),
            original
        );
    }
}

#[test]
fn resource_invocation_structure_preserves_old_policies_scope_and_s1() {
    let source = nested_input();
    for global in [false, true] {
        for s1 in [0, 15] {
            for insert in [false, true] {
                let sample = topology(&source, insert);
                let id = target(global, s1);
                let change = TeletextResourceObjectProgramOverlay::new_with_invocation_structure(
                    id,
                    sample.expected().clone(),
                    sample.replacement().clone(),
                    sample.retained_commands().to_vec(),
                    budget(),
                )
                .unwrap();
                assert_eq!(change.identity(), id);
                assert!(change.has_invocation_structure_changes());
                for constructor in [
                    TeletextResourceObjectProgramOverlay::new,
                    TeletextResourceObjectProgramOverlay::new_with_invocation_changes,
                ] {
                    assert!(constructor(
                        id,
                        change.expected().clone(),
                        change.replacement().clone(),
                        change.retained_commands().to_vec(),
                        budget()
                    )
                    .is_err());
                }
                assert!(
                    TeletextResourceObjectProgramOverlay::new_with_invocation_structure(
                        TeletextObjectIdentity::local(address(200, 0), TeletextObjectId::new(1)),
                        change.expected().clone(),
                        change.replacement().clone(),
                        change.retained_commands().to_vec(),
                        budget(),
                    )
                    .is_err()
                );
                assert!(
                    TeletextLocalObjectGlyphProgramOverlay::new_with_style_commands(
                        TeletextObjectIdentity::local(address(200, 0), TeletextObjectId::new(1)),
                        change.expected().clone(),
                        change.replacement().clone(),
                        change.retained_commands().to_vec(),
                    )
                    .is_err()
                );
            }
        }
    }
}

#[test]
fn resource_invocation_structure_mixes_retained_calls_and_styles_without_aliasing_v16() {
    let source = nested_input();
    let change = topology(&source, true);
    let mut commands = change.replacement().commands().to_vec();
    commands[0] = call(target(true, 2), TeletextObjectType::Passive, 2);
    commands.push(at(2, fg(3)));
    let mut retained = change.retained_commands().to_vec();
    retained.push(None);
    let mixed = TeletextResourceObjectProgramOverlay::new_with_invocation_structure(
        change.identity(),
        change.expected().clone(),
        program(commands),
        retained,
        budget(),
    )
    .unwrap();
    assert!(mixed.has_invocation_structure_changes());
    assert!(mixed.has_invocation_changes());
    service(source.clone())
        .with_resource_object_program_overlays(&[mixed], budget())
        .unwrap();
    let retained = change_call(
        &source,
        call(target(true, 2), TeletextObjectType::Passive, 1),
    );
    assert!(!retained.has_invocation_structure_changes());
    assert_eq!(
        TeletextResourceObjectProgramOverlay::new_with_invocation_structure(
            retained.identity(),
            retained.expected().clone(),
            retained.replacement().clone(),
            retained.retained_commands().to_vec(),
            budget(),
        ),
        Err(TeletextResourceObjectProgramOverlayError::MissingInvocationStructure)
    );
}

#[test]
fn resource_invocation_structure_refuses_bad_maps_and_every_other_anchor_change() {
    let call = call(target(true, 2), TeletextObjectType::Passive, 0);
    let expected = program(vec![call, charset(0), at(0, glyph(b'A'))]);
    for (commands, retained) in [
        (vec![charset(1), at(0, glyph(b'A'))], vec![Some(1), Some(2)]),
        (vec![at(0, glyph(b'A'))], vec![Some(2)]),
        (
            vec![charset(0), at(0, op(json!({"kind":"g2","code":65})))],
            vec![Some(1), Some(2)],
        ),
        (vec![charset(0), at(0, glyph(b'A'))], vec![Some(1), Some(1)]),
        (vec![charset(0), at(0, glyph(b'A'))], vec![Some(2), Some(1)]),
        (vec![charset(0), at(0, glyph(b'A'))], vec![Some(1), Some(3)]),
        (vec![charset(0), at(0, glyph(b'A'))], vec![Some(1)]),
    ] {
        assert!(
            TeletextResourceObjectProgramOverlay::new_with_invocation_structure(
                target(false, 1),
                expected.clone(),
                program(commands),
                retained,
                budget(),
            )
            .is_err()
        );
    }
}

#[test]
fn resource_invocation_structure_keeps_graph_link_and_collection_validation() {
    let source = nested_input();
    let original = service(source.clone());
    for command in [
        call(target(false, 9), TeletextObjectType::Passive, 0),
        call(target(true, 2), TeletextObjectType::Adaptive, 0),
        call(source.objects[0].identity, TeletextObjectType::Active, 0),
    ] {
        let change = topology(&source, true);
        let mut commands = change.replacement().commands().to_vec();
        commands[1] = command;
        let change = TeletextResourceObjectProgramOverlay::new_with_invocation_structure(
            change.identity(),
            change.expected().clone(),
            program(commands),
            change.retained_commands().to_vec(),
            budget(),
        )
        .unwrap();
        assert!(matches!(
            original
                .clone()
                .with_resource_object_program_overlays(&[change], budget()),
            Err(TeletextServiceError::Objects(_))
        ));
    }
    let mut selected = source.clone();
    let other = TeletextObjectIdentity::public(
        resource(TeletextResourceKind::Pop, 0xa2, 1),
        TeletextObjectId::new(99),
    )
    .unwrap();
    selected
        .objects
        .push(definition(other, TeletextObjectType::Passive, vec![]));
    let selected = service(selected);
    let change = topology(&source, true);
    let mut commands = change.replacement().commands().to_vec();
    commands[1] = invoke(other, TeletextObjectType::Passive);
    let change = TeletextResourceObjectProgramOverlay::new_with_invocation_structure(
        change.identity(),
        change.expected().clone(),
        program(commands),
        change.retained_commands().to_vec(),
        budget(),
    )
    .unwrap();
    assert!(
        matches!(selected.with_resource_object_program_overlays(&[change],budget()),Err(TeletextServiceError::ObjectSelection {target,..}) if target==other)
    );
    let change = topology(&source, true);
    let mut limits = budget();
    limits.max_collected_commands = [address(200, 0), address(200, 1)]
        .into_iter()
        .map(|owner| original.collected(owner).unwrap().1.command_count())
        .sum();
    assert!(matches!(
        original.with_resource_object_program_overlays(&[change], limits),
        Err(TeletextServiceError::Collection { .. })
    ));
}

#[test]
fn resource_invocation_structure_raw_budgets_precede_correspondence() {
    let change = topology(&nested_input(), true);
    let commands = change.expected().commands().len() + change.replacement().commands().len();
    let items = change.retained_commands().len();
    for (objects, commands, items) in [
        (0, commands, items),
        (1, commands - 1, items),
        (1, commands, items - 1),
    ] {
        let mut limits = budget();
        limits.max_objects = objects;
        limits.max_commands = commands;
        limits.max_structure_items = items;
        assert_eq!(
            TeletextResourceObjectProgramOverlay::new_with_invocation_structure(
                change.identity(),
                change.expected().clone(),
                change.replacement().clone(),
                vec![Some(usize::MAX); change.retained_commands().len()],
                limits,
            ),
            Err(TeletextResourceObjectProgramOverlayError::Budget)
        );
    }
}
