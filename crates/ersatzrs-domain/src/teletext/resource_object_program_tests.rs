//! Reuse the genuine shared Public/Global/S1 service and declared fixture.
use super::*;

#[path = "resource_object_invocation_program_tests.rs"]
mod invocations;

fn change_program(global: bool, extra: usize) -> TeletextResourceObjectProgramOverlay {
    let source = input();
    let expected = source.objects[usize::from(global)].program.clone();
    let mut commands = expected.commands().to_vec();
    commands.splice(2..2, (0..extra).map(|_| at(0, glyph(b'B'))));
    let retained = (0..2)
        .map(Some)
        .chain(std::iter::repeat_n(None, extra))
        .chain((2..expected.commands().len()).map(Some))
        .collect();
    TeletextResourceObjectProgramOverlay::new(
        target(global, if global { 2 } else { 1 }),
        expected,
        program(commands),
        retained,
        budget(),
    )
    .unwrap()
}

#[test]
fn resource_programs_preserve_complete_context_and_both_shared_consumers_and_undo() {
    let before = input();
    let original = service(before.clone());
    let changes = [change_program(false, 2), change_program(true, 1)];
    let edited = original
        .clone()
        .with_resource_object_program_overlays(&changes, budget())
        .unwrap();
    let mut expected = before;
    for (definition, change) in expected.objects.iter_mut().zip(&changes) {
        definition.program = change.replacement().clone();
    }
    assert_eq!(edited, service(expected));
    assert_eq!(edited.base(), original.base());
    assert_eq!(edited.drcs(), original.drcs());
    assert_eq!(edited.object_companions(), original.object_companions());
    assert_eq!(
        edited.physical_base_overlays(),
        original.physical_base_overlays()
    );
    let mut inverse = Vec::new();
    for change in changes {
        assert_eq!(change.identity().owner(), None);
        assert_eq!(change.original_command(2), None);
        assert_eq!(
            change.replacement_command(2),
            Some(2 + change.replacement().commands().len() - change.expected().commands().len())
        );
        inverse.push(
            TeletextResourceObjectProgramOverlay::new(
                change.identity(),
                change.replacement().clone(),
                change.expected().clone(),
                (0..change.expected().commands().len())
                    .map(|ordinal| change.replacement_command(ordinal))
                    .collect(),
                budget(),
            )
            .unwrap(),
        );
    }
    for owner in [address(200, 0), address(200, 1)] {
        assert_eq!(
            edited.collected(owner).unwrap().1.references(),
            original.collected(owner).unwrap().1.references()
        );
        assert_eq!(
            edited.collected(owner).unwrap().1.command_count(),
            original.collected(owner).unwrap().1.command_count() + 3
        );
        assert_eq!(
            edited.collected(owner).unwrap().1.sources(),
            &[
                TeletextEnhancementSource {
                    object: Some(target(false, 1)),
                    command: 4
                },
                TeletextEnhancementSource {
                    object: Some(target(true, 2)),
                    command: 3
                },
            ]
        );
    }
    assert_eq!(
        edited
            .with_resource_object_program_overlays(&inverse, budget())
            .unwrap(),
        original
    );
}

#[test]
fn resource_program_constructor_bundles_all_glyphs_styles_and_exact_anchor_policy() {
    let values = [
        json!({"kind":"g0","code":65}),
        json!({"kind":"g1","mosaic":1}),
        json!({"kind":"g1_alpha","code":64}),
        json!({"kind":"g2","code":35}),
        json!({"kind":"g3","code":35,"compatibility":"level15"}),
        json!({"kind":"g3","code":35,"compatibility":"level25"}),
        json!({"kind":"g0_diacritic","code":65,"diacritic":1}),
        json!({"kind":"drcs","reference":{"table":"normal","subtable":15,"slot":47}}),
        json!({"kind":"drcs","reference":{"table":"global","subtable":15,"slot":47}}),
    ];
    let styles = [
        fg(2),
        op(json!({"kind":"background","colour":3})),
        op(
            json!({"kind":"attributes","double_height":true,"double_width":true,
            "boxed":true,"conceal":true,"invert":true,"underline_or_separated_mosaics":true}),
        ),
    ];
    for value in values {
        let glyph = op(value);
        let anchor = charset(0);
        let expected = program(vec![at(0, glyph), at(0, glyph), anchor]);
        for style in styles {
            let replacement = program(vec![at(0, glyph), at(1, glyph), at(1, style), anchor]);
            let change = TeletextResourceObjectProgramOverlay::new(
                target(false, 1),
                expected.clone(),
                replacement.clone(),
                vec![None, Some(1), None, Some(2)],
                budget(),
            )
            .unwrap();
            assert_eq!(change.retained_commands(), [None, Some(1), None, Some(2)]);
            assert_eq!(change.original_command(1), Some(1));
            assert_eq!(change.replacement_command(0), None);
            assert!(TeletextResourceObjectProgramOverlay::new(
                target(true, 2),
                expected.clone(),
                program(vec![anchor, at(0, glyph)]),
                vec![Some(2), Some(1)],
                budget()
            )
            .is_err());
            assert!(TeletextResourceObjectProgramOverlay::new(
                target(false, 1),
                expected.clone(),
                replacement,
                vec![Some(1), Some(0), None, Some(2)],
                budget()
            )
            .is_err());
        }
    }
    let source = program(vec![at(0, fg(1)), at(0, glyph(b'A'))]);
    assert!(TeletextResourceObjectProgramOverlay::new(
        target(false, 1),
        source.clone(),
        program(vec![at(1, fg(1)), at(0, glyph(b'A'))]),
        vec![Some(0), Some(1)],
        budget()
    )
    .is_err());
    assert!(TeletextResourceObjectProgramOverlay::new(
        target(false, 1),
        source,
        program(vec![at(0, glyph(b'A')), charset(55)]),
        vec![Some(1), None],
        budget()
    )
    .is_err());
}

#[test]
fn resource_programs_do_not_limit_logical_commands_to_native_words_or_local208() {
    let hidden = TeletextEnhancementCommand::SetPosition {
        position: TeletextEnhancementPosition::new(0, 0).unwrap(),
    };
    let mut commands = vec![hidden; 600];
    commands.push(at(0, glyph(b'A')));
    let expected = program(commands.clone());
    commands.push(at(0, glyph(b'B')));
    let replacement = program(commands);
    let retained = (0..601).map(Some).chain([None]).collect::<Vec<_>>();
    let change = TeletextResourceObjectProgramOverlay::new(
        target(false, 1),
        expected.clone(),
        replacement.clone(),
        retained.clone(),
        budget(),
    )
    .unwrap();
    assert_eq!(change.replacement().commands().len(), 602);
    let local = TeletextObjectIdentity::local(address(200, 0), TeletextObjectId::new(19));
    assert_eq!(
        TeletextResourceObjectProgramOverlay::new(
            local,
            expected.clone(),
            replacement.clone(),
            retained.clone(),
            budget()
        ),
        Err(TeletextResourceObjectProgramOverlayError::Scope)
    );
    assert_eq!(
        TeletextLocalObjectGlyphProgramOverlay::new_with_style_commands(
            local,
            expected.clone(),
            replacement.clone(),
            retained.clone()
        ),
        Err(TeletextLocalObjectGlyphProgramOverlayError::Capacity)
    );
    for (commands, items) in [(1202, 8192), (8192, 601)] {
        let mut limits = budget();
        limits.max_commands = commands;
        limits.max_structure_items = items;
        assert_eq!(
            TeletextResourceObjectProgramOverlay::new(
                target(false, 1),
                expected.clone(),
                replacement.clone(),
                retained.clone(),
                limits
            ),
            Err(TeletextResourceObjectProgramOverlayError::Budget)
        );
    }
    let mut source = input();
    source.objects[0].program = expected;
    let original = service(source);
    assert!(original
        .with_resource_object_program_overlays(&[change], budget())
        .is_ok());
}

#[test]
fn resource_programs_refuse_crossed_families_g3_compatibility_and_changed_anchors() {
    for (old, new) in [
        (glyph(b'A'), op(json!({"kind":"g2","code":65}))),
        (
            op(json!({"kind":"g3","code":35,"compatibility":"level15"})),
            op(json!({"kind":"g3","code":36,"compatibility":"level25"})),
        ),
    ] {
        assert!(matches!(
            TeletextResourceObjectProgramOverlay::new(
                target(false, 1),
                program(vec![at(0, old)]),
                program(vec![at(1, new)]),
                vec![Some(0)],
                budget()
            ),
            Err(TeletextResourceObjectProgramOverlayError::Correspondence(
                TeletextLocalObjectGlyphProgramOverlayError::ProtectedCommand
            ))
        ));
    }
    let anchors = [
        charset(0),
        invoke(target(true, 2), TeletextObjectType::Passive),
        TeletextEnhancementCommand::SetPosition {
            position: TeletextEnhancementPosition::new(0, 8).unwrap(),
        },
    ];
    for anchor in anchors {
        assert!(TeletextResourceObjectProgramOverlay::new(
            target(false, 1),
            program(vec![anchor, at(8, glyph(b'A'))]),
            program(vec![at(8, glyph(b'B'))]),
            vec![Some(1)],
            budget()
        )
        .is_err());
        assert!(TeletextResourceObjectProgramOverlay::new(
            target(false, 1),
            program(vec![at(8, glyph(b'A'))]),
            program(vec![anchor, at(8, glyph(b'B'))]),
            vec![None, Some(0)],
            budget()
        )
        .is_err());
    }
}

#[test]
fn resource_program_application_checks_missing_stale_duplicate_raw_and_final_budgets() {
    let original = service(input());
    let change = change_program(false, 6);
    let mut limit = budget();
    limit.max_commands = 15;
    assert!(TeletextSemanticService::new(input(), limit).is_ok());
    assert!(matches!(
        original
            .clone()
            .with_resource_object_program_overlays(std::slice::from_ref(&change), limit),
        Err(TeletextServiceError::Budget {
            dimension: "authored command",
            maximum: 15
        })
    ));
    let mut limit = budget();
    limit.max_structure_items = change.retained_commands().len() - 1;
    assert!(matches!(
        original
            .clone()
            .with_resource_object_program_overlays(std::slice::from_ref(&change), limit),
        Err(TeletextServiceError::Budget {
            dimension: "structure item",
            ..
        })
    ));
    assert!(matches!(
        original
            .clone()
            .with_resource_object_program_overlays(&[change.clone(), change.clone()], budget()),
        Err(TeletextServiceError::ResourceObjectProgramOverlay {
            reason: TeletextResourceObjectProgramFailure::Duplicate,
            ..
        })
    ));
    let missing = TeletextResourceObjectProgramOverlay::new(
        target(false, 0),
        change.expected().clone(),
        change.replacement().clone(),
        change.retained_commands().to_vec(),
        budget(),
    )
    .unwrap();
    assert!(matches!(
        original
            .clone()
            .with_resource_object_program_overlays(&[missing], budget()),
        Err(TeletextServiceError::ResourceObjectProgramOverlay {
            reason: TeletextResourceObjectProgramFailure::MissingObject,
            ..
        })
    ));
    let mut stale = input();
    stale.objects[0].program = program(vec![at(0, fg(3))]);
    assert!(matches!(
        service(stale).with_resource_object_program_overlays(&[change], budget()),
        Err(TeletextServiceError::ResourceObjectProgramOverlay {
            reason: TeletextResourceObjectProgramFailure::ProgramMismatch,
            ..
        })
    ));
    assert_eq!(
        original.with_resource_object_program_overlays(&[], budget()),
        Err(TeletextServiceError::EmptyResourceObjectProgramOverlays)
    );
}

#[test]
fn resource_program_reconstruction_rejects_missing_inserted_drcs_with_exact_source() {
    let original = service(input());
    let change = change_program(false, 1);
    let mut replacement = change.replacement().commands().to_vec();
    replacement[2] = at(
        0,
        op(json!({"kind":"drcs","reference":{"table":"normal","subtable":0,"slot":1}})),
    );
    let change = TeletextResourceObjectProgramOverlay::new(
        change.identity(),
        change.expected().clone(),
        program(replacement),
        change.retained_commands().to_vec(),
        budget(),
    )
    .unwrap();
    assert!(
        matches!(original.with_resource_object_program_overlays(&[change],budget()),
        Err(TeletextServiceError::CollectedDrcs {error:TeletextCollectedDrcsError {
            source_location:Some(TeletextEnhancementSource {object:Some(identity),command:2}),..
        },..}) if identity==target(false,1))
    );
}

#[test]
fn resource_program_declared_wrapper_preserves_authentication_and_full_resource_inventory() {
    let mut source = input();
    let link = |page, mask| {
        TeletextX27LinkDeclaration::new(
            TeletextX27LinkTarget::Page { page: raw(page) },
            TeletextX27LinkValidity::Level25Only,
            TeletextSubpageRequirementMask::new(mask),
        )
        .unwrap()
    };
    let absent = TeletextX27LinkDeclaration::new(
        TeletextX27LinkTarget::Unspecified {
            magazine: TeletextX27Magazine::new(2).unwrap(),
        },
        TeletextX27LinkValidity::Level25Only,
        TeletextSubpageRequirementMask::new(0xffff),
    )
    .unwrap();
    for page in &mut source.pages {
        page.compositional_links = Some(
            TeletextX27Level25Links::new(link(0xa1, 4), link(0xa0, 2), absent, link(0xb0, 1))
                .unwrap(),
        );
    }
    let mot = unused_mot();
    let original = TeletextDeclaredService::new(
        service(source),
        vec![mot.clone()],
        [address(200, 0), address(200, 1)]
            .map(|display| TeletextDisplayMotSelection {
                display,
                mot: Some(mot.identity()),
            })
            .to_vec(),
        vec![resource(TeletextResourceKind::Pop, 0xa2, 0)],
        TeletextDeclaredServiceBudget {
            max_mots: 8,
            max_displays: 8,
            max_resources: 64,
            max_defaults: 64,
            max_objects: 64,
        },
    )
    .unwrap();
    let edited = original
        .clone()
        .with_resource_object_program_overlays(
            &[change_program(false, 2), change_program(true, 1)],
            budget(),
        )
        .unwrap();
    assert_eq!(edited.mots(), original.mots());
    assert_eq!(edited.selections(), original.selections());
    assert_eq!(edited.resource_inventory(), original.resource_inventory());
    assert_eq!(edited.resolutions(), original.resolutions());
    assert_eq!(edited.defaults(), original.defaults());
    assert_eq!(edited.empty_object_pages(), original.empty_object_pages());
    assert_eq!(edited.semantic().base(), original.semantic().base());
    assert_eq!(
        edited.semantic().physical_base_overlays(),
        original.semantic().physical_base_overlays()
    );
}

#[test]
fn transient_resource_positions_keep_legacy_policies_and_retained_anchors_exact() {
    let position = |row, column| TeletextEnhancementCommand::SetPosition {
        position: TeletextEnhancementPosition::new(row, column).unwrap(),
    };
    let expected = program(vec![position(1, 0), at(8, glyph(b'P'))]);
    let replacement = program(vec![
        position(1, 0),
        at(8, glyph(b'P')),
        position(2, 0),
        at(4, glyph(b'Q')),
    ]);
    let retained = vec![Some(0), Some(1), None, None];
    let change = TeletextResourceObjectProgramOverlay::new_with_inserted_positions(
        target(false, 1),
        expected.clone(),
        replacement.clone(),
        retained.clone(),
        budget(),
    )
    .unwrap();
    assert!(change.has_inserted_positions());
    assert!(!change.has_invocation_changes());
    assert!(!change.has_invocation_structure_changes());
    for constructor in [
        TeletextResourceObjectProgramOverlay::new,
        TeletextResourceObjectProgramOverlay::new_with_invocation_changes,
        TeletextResourceObjectProgramOverlay::new_with_invocation_structure,
    ] {
        assert!(constructor(
            target(false, 1),
            expected.clone(),
            replacement.clone(),
            retained.clone(),
            budget()
        )
        .is_err());
    }
    for (replacement, retained) in [
        (
            program(vec![at(8, glyph(b'P')), position(2, 0)]),
            vec![Some(1), None],
        ),
        (
            program(vec![position(1, 1), at(8, glyph(b'P')), position(2, 0)]),
            vec![Some(0), Some(1), None],
        ),
    ] {
        assert!(
            TeletextResourceObjectProgramOverlay::new_with_inserted_positions(
                target(false, 1),
                expected.clone(),
                replacement,
                retained,
                budget(),
            )
            .is_err()
        );
    }
    assert_eq!(
        TeletextResourceObjectProgramOverlay::new_with_inserted_positions(
            target(false, 1),
            expected.clone(),
            program(vec![position(1, 0), at(8, glyph(b'Q'))]),
            vec![Some(0), Some(1)],
            budget(),
        ),
        Err(TeletextResourceObjectProgramOverlayError::MissingInsertedPosition)
    );
    let mut bounded = budget();
    bounded.max_commands = 5;
    assert_eq!(
        TeletextResourceObjectProgramOverlay::new_with_inserted_positions(
            target(false, 1),
            expected,
            replacement,
            retained,
            bounded,
        ),
        Err(TeletextResourceObjectProgramOverlayError::Budget)
    );
}

#[test]
fn original_resource_positions_have_distinct_intent_and_exact_anchor_correspondence() {
    let position = |row, column| TeletextEnhancementCommand::SetPosition {
        position: TeletextEnhancementPosition::new(row, column).unwrap(),
    };
    let expected = program(vec![position(1, 0), at(8, glyph(b'P')), position(3, 0)]);
    let replacement = program(vec![
        position(1, 0),
        at(8, glyph(b'P')),
        position(2, 0),
        at(6, glyph(b'R')),
        position(3, 0),
    ]);
    let map = vec![Some(0), Some(1), None, None, Some(2)];
    let original = TeletextResourceObjectProgramOverlay::new_with_original_positions(
        target(false, 15),
        expected.clone(),
        replacement.clone(),
        map.clone(),
        budget(),
    )
    .unwrap();
    let transient = TeletextResourceObjectProgramOverlay::new_with_inserted_positions(
        target(false, 15),
        expected.clone(),
        replacement.clone(),
        map.clone(),
        budget(),
    )
    .unwrap();
    assert!(original.has_inserted_positions());
    assert!(original.is_original_position_intent());
    assert!(!transient.is_original_position_intent());
    assert_ne!(original, transient);
    assert_eq!(original.retained_commands(), map);
    for (ordinal, row) in [(0, 1), (4, 3)] {
        let mut invalid = replacement.commands().to_vec();
        invalid[ordinal] = position(row, 1);
        assert!(
            TeletextResourceObjectProgramOverlay::new_with_original_positions(
                target(false, 15),
                expected.clone(),
                program(invalid),
                map.clone(),
                budget(),
            )
            .is_err()
        );
    }
    let mut row_zero = replacement.commands().to_vec();
    row_zero[2] = position(0, 0);
    assert!(
        TeletextResourceObjectProgramOverlay::new_with_original_positions(
            target(false, 15),
            expected.clone(),
            program(row_zero),
            map.clone(),
            budget(),
        )
        .is_err()
    );
    let mut bounded = budget();
    bounded.max_commands = 7;
    assert_eq!(
        TeletextResourceObjectProgramOverlay::new_with_original_positions(
            target(false, 15),
            expected,
            replacement,
            map,
            bounded,
        ),
        Err(TeletextResourceObjectProgramOverlayError::Budget)
    );
}

fn original_position_restoration_prior() -> TeletextResourceObjectProgramOverlay {
    let position = |row, column| TeletextEnhancementCommand::SetPosition {
        position: TeletextEnhancementPosition::new(row, column).unwrap(),
    };
    TeletextResourceObjectProgramOverlay::new_with_original_positions(
        target(true, 15),
        program(vec![
            position(0, 8),
            position(1, 0),
            at(8, glyph(b'P')),
            at(9, glyph(b'Q')),
            position(3, 0),
        ]),
        program(vec![
            position(0, 8),
            position(1, 0),
            at(8, glyph(b'P')),
            position(2, 0),
            at(6, glyph(b'R')),
            position(3, 0),
        ]),
        vec![Some(0), Some(1), Some(2), None, None, Some(4)],
        budget(),
    )
    .unwrap()
}

#[test]
fn original_position_restoration_derives_exact_inverse_and_retains_producer_anchors() {
    let prior = original_position_restoration_prior();
    let restored =
        TeletextResourceObjectProgramOverlay::restore_original_positions(&prior, budget()).unwrap();
    assert!(restored.is_position_restoration());
    assert!(!restored.is_original_position_intent());
    assert!(!restored.has_inserted_positions());
    assert_eq!(restored.identity(), prior.identity());
    assert_eq!(restored.expected(), prior.replacement());
    assert_eq!(restored.replacement(), prior.expected());
    assert_eq!(
        restored.retained_commands(),
        [Some(0), Some(1), Some(2), None, Some(5)]
    );
    for (ordinal, command) in restored.replacement().commands().iter().enumerate() {
        if matches!(command, TeletextEnhancementCommand::SetPosition { .. }) {
            let source = restored.retained_commands()[ordinal].unwrap();
            assert_eq!(restored.expected().commands()[source], *command);
        }
    }
}

#[test]
fn original_position_restoration_refuses_unqualified_priors_and_rechecks_fresh_bounds() {
    let prior = original_position_restoration_prior();
    let transient = TeletextResourceObjectProgramOverlay::new_with_inserted_positions(
        prior.identity(),
        prior.expected().clone(),
        prior.replacement().clone(),
        prior.retained_commands().to_vec(),
        budget(),
    )
    .unwrap();
    let ordinary = TeletextResourceObjectProgramOverlay::new(
        prior.identity(),
        program(vec![at(8, glyph(b'P'))]),
        program(vec![at(8, glyph(b'P')), at(9, glyph(b'Q'))]),
        vec![Some(0), None],
        budget(),
    )
    .unwrap();
    let restored =
        TeletextResourceObjectProgramOverlay::restore_original_positions(&prior, budget()).unwrap();
    for unqualified in [&transient, &ordinary, &restored] {
        assert_eq!(
            TeletextResourceObjectProgramOverlay::restore_original_positions(unqualified, budget()),
            Err(TeletextResourceObjectProgramOverlayError::OriginalPositionIntent)
        );
    }
    let mut exact = budget();
    exact.max_commands = 11;
    exact.max_structure_items = 5;
    assert!(
        TeletextResourceObjectProgramOverlay::restore_original_positions(&prior, exact).is_ok()
    );
    for bounded in [
        TeletextServiceBudget {
            max_commands: 10,
            ..exact
        },
        TeletextServiceBudget {
            max_structure_items: 4,
            ..exact
        },
        TeletextServiceBudget {
            max_objects: 0,
            ..exact
        },
    ] {
        assert_eq!(
            TeletextResourceObjectProgramOverlay::restore_original_positions(&prior, bounded),
            Err(TeletextResourceObjectProgramOverlayError::Budget)
        );
    }
}

fn recoloured_position_prior(global: bool) -> TeletextResourceObjectProgramOverlay {
    let prior = original_position_restoration_prior();
    let mut expected = prior.expected().commands().to_vec();
    let mut replacement = prior.replacement().commands().to_vec();
    expected[2] = at(8, fg(3));
    replacement[2] = at(8, fg(3));
    TeletextResourceObjectProgramOverlay::new_with_original_positions(
        target(global, 15),
        program(expected),
        program(replacement),
        prior.retained_commands().to_vec(),
        budget(),
    )
    .unwrap()
}

#[test]
fn original_position_restoration_to_root_combines_checked_scalar_and_inverse_lineage() {
    for global in [false, true] {
        let prior = recoloured_position_prior(global);
        let mut commands = prior.expected().commands().to_vec();
        commands[2] = at(8, fg(2));
        let root = program(commands);
        let restored = TeletextResourceObjectProgramOverlay::restore_original_positions_to(
            &prior,
            &root,
            budget(),
        )
        .unwrap();
        assert!(restored.is_position_restoration());
        assert_eq!(restored.expected(), prior.replacement());
        assert_eq!(restored.replacement(), &root);
        assert_eq!(restored.identity(), target(global, 15));
        assert_eq!(
            restored.retained_commands(),
            [Some(0), Some(1), Some(2), None, Some(5)]
        );
        let legacy =
            TeletextResourceObjectProgramOverlay::restore_original_positions(&prior, budget())
                .unwrap();
        assert_eq!(legacy.replacement(), prior.expected());
        assert_ne!(legacy, restored);
    }
}

#[test]
fn original_position_restoration_to_root_rejects_anchor_mode_column_and_budget_changes() {
    let prior = recoloured_position_prior(false);
    let expected = prior.expected().commands();
    for (ordinal, replacement) in [
        (
            1,
            TeletextEnhancementCommand::SetPosition {
                position: TeletextEnhancementPosition::new(2, 0).unwrap(),
            },
        ),
        (2, at(9, fg(2))),
        (2, at(8, op(json!({"kind":"background","colour":2})))),
        (2, at(8, glyph(b'P'))),
    ] {
        let mut commands = expected.to_vec();
        commands[ordinal] = replacement;
        assert_eq!(
            TeletextResourceObjectProgramOverlay::restore_original_positions_to(
                &prior,
                &program(commands),
                budget(),
            ),
            Err(TeletextResourceObjectProgramOverlayError::OriginalPositionIntent)
        );
    }
    assert_eq!(
        TeletextResourceObjectProgramOverlay::restore_original_positions_to(
            &prior,
            &program(expected[..4].to_vec()),
            budget(),
        ),
        Err(TeletextResourceObjectProgramOverlayError::OriginalPositionIntent)
    );
    let mut commands = expected.to_vec();
    commands[2] = at(8, fg(2));
    let root = program(commands);
    for bounded in [
        TeletextServiceBudget {
            max_commands: 10,
            ..budget()
        },
        TeletextServiceBudget {
            max_structure_items: 4,
            ..budget()
        },
        TeletextServiceBudget {
            max_objects: 0,
            ..budget()
        },
    ] {
        assert_eq!(
            TeletextResourceObjectProgramOverlay::restore_original_positions_to(
                &prior, &root, bounded,
            ),
            Err(TeletextResourceObjectProgramOverlayError::Budget)
        );
    }
}
