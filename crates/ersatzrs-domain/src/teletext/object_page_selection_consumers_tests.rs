//! Existing two-subcode graph: direct Main and owner-bound Local call consumers.
use super::*;

fn local() -> TeletextObjectIdentity {
    TeletextObjectIdentity::local(owner(32), TeletextObjectId::new(65520))
}
fn child(changed: bool) -> TeletextObjectIdentity {
    TeletextObjectIdentity::public(
        identity(
            TeletextResourceKind::Pop,
            if changed { 0xa4 } else { 0xa0 },
            0,
        ),
        TeletextObjectId::new(10),
    )
    .unwrap()
}
fn calls(changed: bool) -> TeletextEnhancementProgram {
    TeletextEnhancementProgram::new(vec![
        program(child(changed)).commands()[0],
        program(if changed {
            global(0xa3, 15)
        } else {
            global(0xa2, 0)
        })
        .commands()[0],
    ])
    .unwrap()
}
fn consumer_input(changed: bool) -> TeletextResolvedServiceDraft {
    let mut source = input(true, false);
    for id in [child(false), child(true)] {
        source.objects.push(TeletextEnhancementObjectDefinition {
            identity: id,
            object_type: TeletextObjectType::Passive,
            program: TeletextEnhancementProgram::new(vec![]).unwrap(),
        });
    }
    source.objects.push(TeletextEnhancementObjectDefinition {
        identity: local(),
        object_type: TeletextObjectType::Active,
        program: calls(changed),
    });
    source.pages[0].local_program = Some(calls(changed));
    source.pages[1].local_program = Some(
        TeletextEnhancementProgram::new(vec![TeletextEnhancementCommand::InvokeObject {
            target: local(),
            object_type: TeletextObjectType::Active,
            offset: None,
        }])
        .unwrap(),
    );
    for page in &mut source.pages {
        page.resources.objects.public = Some(raw(if changed { 0xa4 } else { 0xa0 }));
        page.resources.objects.global = Some(raw(if changed { 0xa3 } else { 0xa2 }));
        page.compositional_links = Some(links(
            if changed { 0xa4 } else { 0xa0 },
            if changed { 0xa3 } else { 0xa2 },
            if changed { 0x8000 } else { 1 },
        ));
    }
    source
}
fn consumer_changes(original: &TeletextDeclaredService) -> Vec<TeletextObjectPageSelectionChange> {
    let mut intents = changes(original);
    for display in [owner(0), owner(32)] {
        intents.push(change(
            original,
            display,
            TeletextX27LinkRole::Pop,
            link(0xa4, 1),
        ));
    }
    intents
}
fn local_overlay() -> TeletextLocalObjectGlyphProgramOverlay {
    TeletextLocalObjectGlyphProgramOverlay::new_with_invocation_changes(
        local(),
        calls(false),
        calls(true),
        vec![Some(0), Some(1)],
    )
    .unwrap()
}

#[test]
fn main_local_consumers_baseline_exposes_sequential_selection_deadlock_and_closed_main_authority() {
    let original = bind_input(consumer_input(false));
    let before = original.clone();
    assert_eq!(
        original
            .clone()
            .with_local_object_glyph_program_overlays(&[local_overlay()], limits()),
        Err(TeletextServiceError::ObjectSelection {
            display: owner(32),
            source_location: TeletextEnhancementSource {
                object: Some(local()),
                command: 0
            },
            target: child(true),
        })
    );
    assert_eq!(
        original
            .clone()
            .with_object_page_selections_and_program_overlays(
                &consumer_changes(&original),
                &[],
                limits(),
                budget()
            ),
        Err(TeletextObjectPageSelectionError::Service(
            TeletextServiceError::ObjectSelection {
                display: owner(0),
                source_location: TeletextEnhancementSource {
                    object: None,
                    command: 0
                },
                target: child(false),
            }
        ))
    );
    assert_eq!(
        TeletextMainLocalInvocationProgramOverlay::new(
            owner(0),
            calls(false),
            calls(true),
            vec![Some(0), Some(1)]
        ),
        Err(TeletextMainLocalInvocationProgramOverlayError::ProtectedCommand)
    );
    assert_eq!(original, before);
}

fn main_overlays() -> Vec<TeletextMainResourceInvocationOverlay> {
    (0..2)
        .map(|command| {
            TeletextMainResourceInvocationOverlay::new(
                owner(0),
                command,
                calls(false).commands()[command],
                calls(true).commands()[command],
            )
            .unwrap()
        })
        .collect()
}
fn apply_consumers(
    original: TeletextDeclaredService,
    selectors: &[TeletextObjectPageSelectionChange],
    main: &[TeletextMainResourceInvocationOverlay],
    local: &[TeletextLocalObjectGlyphProgramOverlay],
    resources: &[TeletextResourceObjectProgramOverlay],
) -> Result<TeletextDeclaredService, TeletextObjectPageSelectionError> {
    original.with_object_page_selections_and_consumers(
        selectors,
        TeletextObjectPageSelectionConsumers {
            main,
            local,
            resources,
        },
        limits(),
        budget(),
    )
}

#[test]
fn main_local_resource_consumers_commit_one_exact_final_service_and_preserve_original_authorities()
{
    for include_resource in [false, true] {
        let original = bind_input(consumer_input(false));
        let before = original.clone();
        let selectors = consumer_changes(&original);
        let resources = if include_resource {
            vec![overlay()]
        } else {
            vec![]
        };
        let edited = apply_consumers(
            original.clone(),
            &selectors,
            &main_overlays(),
            &[local_overlay()],
            &resources,
        )
        .unwrap();
        let mut expected_input = consumer_input(true);
        if include_resource {
            expected_input.objects[0].program = program(global(0xa3, 15));
        }
        assert_eq!(edited, bind_input(expected_input));
        assert_eq!(original, before);
        assert_eq!(edited.defaults(), original.defaults());
        assert_eq!(edited.mots(), original.mots());
        assert_eq!(edited.selections(), original.selections());
        assert_eq!(edited.resource_inventory(), original.resource_inventory());
        assert_eq!(edited.empty_object_pages(), original.empty_object_pages());
        assert_eq!(edited.semantic().base(), original.semantic().base());
        assert_eq!(edited.semantic().drcs(), original.semantic().drcs());
        assert_eq!(
            edited.semantic().magazine_presentations(),
            original.semantic().magazine_presentations()
        );
        assert_eq!(
            edited.semantic().physical_base_overlays(),
            original.semantic().physical_base_overlays()
        );
        assert_eq!(
            edited.semantic().base_text_spellings(),
            original.semantic().base_text_spellings()
        );
        for (old, new) in original.resolutions().iter().zip(edited.resolutions()) {
            assert_eq!(old.defaults(), new.defaults());
            assert_eq!(
                old.conditional_pop_fallback(),
                new.conditional_pop_fallback()
            );
        }
        let refs = edited
            .semantic()
            .collected(owner(32))
            .unwrap()
            .1
            .object_references();
        assert_eq!(refs[0].target, local());
        assert_eq!(refs[1].target, child(true));
        assert_eq!(
            refs[1].source_location,
            TeletextEnhancementSource {
                object: Some(local()),
                command: 0
            }
        );
        let refs = edited
            .semantic()
            .collected(owner(0))
            .unwrap()
            .1
            .object_references();
        assert_eq!(refs[0].target, child(true));
        assert_eq!(
            refs[0].source_location,
            TeletextEnhancementSource {
                object: None,
                command: 0
            }
        );
    }
    check_cross_role_main_retargets();
}
fn check_cross_role_main_retargets() {
    let position = TeletextEnhancementCommand::SetPosition {
        position: TeletextEnhancementPosition::new(1, 8).unwrap(),
    };
    let mut source = consumer_input(false);
    let mut old_main = vec![position];
    old_main.extend_from_slice(calls(false).commands());
    source.pages[0].local_program = Some(TeletextEnhancementProgram::new(old_main).unwrap());
    let original = bind_input(source);
    let selectors = consumer_changes(&original);
    let mut replacement = vec![position];
    replacement.extend(calls(true).commands().iter().rev().copied());
    let reversed = TeletextEnhancementProgram::new(replacement).unwrap();
    let main: Vec<_> = (0..2)
        .map(|command| {
            TeletextMainResourceInvocationOverlay::new(
                owner(0),
                command + 1,
                calls(false).commands()[command],
                reversed.commands()[command + 1],
            )
            .unwrap()
        })
        .collect();
    let edited = apply_consumers(original, &selectors, &main, &[local_overlay()], &[]).unwrap();
    let mut expected = consumer_input(true);
    expected.pages[0].local_program = Some(reversed);
    assert_eq!(edited, bind_input(expected));
}

#[test]
fn main_resource_call_intent_keeps_type_offset_noncalls_and_historical_main_local_closed() {
    use TeletextMainResourceInvocationOverlay as Overlay;
    use TeletextMainResourceInvocationOverlayError as Error;
    let before = calls(false).commands()[0];
    let after = calls(true).commands()[0];
    let old = TeletextMainLocalInvocationProgramOverlay::new(
        owner(0),
        calls(false),
        calls(true),
        vec![Some(0), Some(1)],
    );
    assert_eq!(
        old,
        Err(TeletextMainLocalInvocationProgramOverlayError::ProtectedCommand)
    );
    assert_eq!(
        Overlay::new(owner(0), 0, before, before),
        Err(Error::Unchanged)
    );
    for altered in [
        TeletextEnhancementCommand::InvokeObject {
            target: child(true),
            object_type: TeletextObjectType::Active,
            offset: None,
        },
        TeletextEnhancementCommand::InvokeObject {
            target: child(true),
            object_type: TeletextObjectType::Passive,
            offset: Some(TeletextObjectOffset::new(0, 0).unwrap()),
        },
    ] {
        assert_eq!(
            Overlay::new(owner(0), 0, before, altered),
            Err(Error::ProtectedCall)
        );
    }
    let noncalls = [
        TeletextEnhancementCommand::SetPosition {
            position: TeletextEnhancementPosition::new(1, 8).unwrap(),
        },
        TeletextEnhancementCommand::AtColumn {
            column: 0,
            operation: TeletextColumnOperation::G0 {
                code: TeletextGlyphCode::new(b'A').unwrap(),
            },
        },
        TeletextEnhancementCommand::InvokeObject {
            target: local(),
            object_type: TeletextObjectType::Passive,
            offset: None,
        },
    ];
    for forbidden in noncalls {
        assert_eq!(
            Overlay::new(owner(0), 0, forbidden, after),
            Err(Error::ResourceCall)
        );
        assert_eq!(
            Overlay::new(owner(0), 0, before, forbidden),
            Err(Error::ResourceCall)
        );
    }
    let offset = Some(TeletextObjectOffset::new(0, 1).unwrap());
    let shifted = |target| TeletextEnhancementCommand::InvokeObject {
        target,
        object_type: TeletextObjectType::Passive,
        offset,
    };
    let intent = Overlay::new(
        owner(0),
        usize::MAX,
        shifted(child(false)),
        shifted(global(0xa3, 15)),
    )
    .unwrap();
    assert_eq!(intent.owner(), owner(0));
    assert_eq!(intent.command(), usize::MAX);
    assert_eq!(intent.expected(), shifted(child(false)));
    assert_eq!(intent.replacement(), shifted(global(0xa3, 15)));
}

#[test]
fn main_local_consumer_attachment_rejects_omissions_duplicates_stale_and_wrong_sources_atomically()
{
    let original = bind_input(consumer_input(false));
    let selectors = consumer_changes(&original);
    let main = main_overlays();
    let local_change = local_overlay();
    assert_eq!(
        apply_consumers(
            original.clone(),
            &selectors,
            &[],
            std::slice::from_ref(&local_change),
            &[]
        ),
        Err(TeletextObjectPageSelectionError::Service(
            TeletextServiceError::ObjectSelection {
                display: owner(0),
                source_location: TeletextEnhancementSource {
                    object: None,
                    command: 0
                },
                target: child(false)
            }
        ))
    );
    assert_eq!(
        apply_consumers(original.clone(), &selectors, &main, &[], &[]),
        Err(TeletextObjectPageSelectionError::Service(
            TeletextServiceError::ObjectSelection {
                display: owner(32),
                source_location: TeletextEnhancementSource {
                    object: Some(local()),
                    command: 0
                },
                target: child(false)
            }
        ))
    );
    check_main_attachment_failures(&original, &selectors);
    for locals in [
        vec![local_change.clone(), local_change],
        vec![
            TeletextLocalObjectGlyphProgramOverlay::new_with_invocation_changes(
                local(),
                calls(true),
                calls(false),
                vec![Some(0), Some(1)],
            )
            .unwrap(),
        ],
    ] {
        assert!(matches!(
            apply_consumers(original.clone(), &selectors, &main, &locals, &[]),
            Err(TeletextObjectPageSelectionError::Service(
                TeletextServiceError::LocalObjectGlyphProgramOverlay { .. }
            ))
        ));
    }
    let mut missing = consumer_input(false);
    missing.pages[0].local_program = None;
    let missing = bind_input(missing);
    let selectors = consumer_changes(&missing);
    assert_eq!(
        apply_consumers(missing, &selectors, &main, &[local_overlay()], &[]),
        Err(TeletextObjectPageSelectionError::Service(
            TeletextServiceError::MainResourceInvocationOverlay {
                owner: owner(0),
                command: 0,
                reason: TeletextMainResourceInvocationFailure::MissingProgram
            }
        ))
    );
}
fn check_main_attachment_failures(
    original: &TeletextDeclaredService,
    selectors: &[TeletextObjectPageSelectionChange],
) {
    use TeletextMainResourceInvocationFailure as Failure;
    let first = main_overlays()[0];
    let make = |owner, ordinal, expected, replacement| {
        TeletextMainResourceInvocationOverlay::new(owner, ordinal, expected, replacement).unwrap()
    };
    for (changes, owner, command, reason) in [
        (vec![first, first], first.owner(), 0, Failure::Duplicate),
        (
            vec![make(owner(1), 0, first.expected(), first.replacement())],
            owner(1),
            0,
            Failure::MissingProgram,
        ),
        (
            vec![make(
                owner(0),
                usize::MAX,
                first.expected(),
                first.replacement(),
            )],
            owner(0),
            usize::MAX,
            Failure::MissingCommand,
        ),
        (
            vec![make(owner(0), 1, first.expected(), first.replacement())],
            owner(0),
            1,
            Failure::OriginalMismatch,
        ),
        (
            vec![make(owner(0), 0, first.replacement(), first.expected())],
            owner(0),
            0,
            Failure::OriginalMismatch,
        ),
    ] {
        assert_eq!(
            apply_consumers(
                original.clone(),
                selectors,
                &changes,
                &[local_overlay()],
                &[]
            ),
            Err(TeletextObjectPageSelectionError::Service(
                TeletextServiceError::MainResourceInvocationOverlay {
                    owner,
                    command,
                    reason
                }
            ))
        );
    }
}

#[test]
fn main_local_consumer_only_actions_validate_existing_selected_banks_without_widening_old_entrypoint(
) {
    let alternate = TeletextObjectIdentity::public(
        identity(TeletextResourceKind::Pop, 0xa0, 0),
        TeletextObjectId::new(11),
    )
    .unwrap();
    for mode in 0..3 {
        let mut before = consumer_input(false);
        before.objects.push(TeletextEnhancementObjectDefinition {
            identity: alternate,
            object_type: TeletextObjectType::Passive,
            program: TeletextEnhancementProgram::new(vec![]).unwrap(),
        });
        let original = bind_input(before.clone());
        let call = program(alternate).commands()[0];
        let mut expected = before;
        let mut main = vec![];
        let mut local_changes = vec![];
        let mut resources = vec![];
        if mode == 0 {
            main.push(
                TeletextMainResourceInvocationOverlay::new(
                    owner(0),
                    0,
                    calls(false).commands()[0],
                    call,
                )
                .unwrap(),
            );
            expected.pages[0].local_program = Some(
                TeletextEnhancementProgram::new(vec![call, calls(false).commands()[1]]).unwrap(),
            );
        } else if mode == 1 {
            let replacement =
                TeletextEnhancementProgram::new(vec![call, calls(false).commands()[1]]).unwrap();
            local_changes.push(
                TeletextLocalObjectGlyphProgramOverlay::new_with_invocation_changes(
                    local(),
                    calls(false),
                    replacement.clone(),
                    vec![Some(0), Some(1)],
                )
                .unwrap(),
            );
            expected
                .objects
                .iter_mut()
                .find(|object| object.identity == local())
                .unwrap()
                .program = replacement;
        } else {
            resources.push(
                TeletextResourceObjectProgramOverlay::new_with_invocation_changes(
                    target(0xa0),
                    program(global(0xa2, 0)),
                    program(alternate),
                    vec![Some(0)],
                    limits(),
                )
                .unwrap(),
            );
            expected.objects[0].program = program(alternate);
        }
        let edited =
            apply_consumers(original.clone(), &[], &main, &local_changes, &resources).unwrap();
        assert_eq!(edited, bind_input(expected));
        assert_eq!(edited.resolutions(), original.resolutions());
        assert_eq!(
            original
                .clone()
                .with_object_page_selections_and_program_overlays(
                    &[],
                    &resources,
                    limits(),
                    budget()
                ),
            Err(TeletextObjectPageSelectionError::Empty)
        );
        assert_eq!(
            apply_consumers(original, &[], &[], &[], &[]),
            Err(TeletextObjectPageSelectionError::EmptyConsumers)
        );
    }
}

#[test]
fn main_local_consumers_revalidate_missing_and_wrong_type_final_targets_with_original_locations() {
    let mut source = consumer_input(false);
    source
        .objects
        .iter_mut()
        .find(|object| object.identity == child(true))
        .unwrap()
        .object_type = TeletextObjectType::Active;
    let original = bind_input(source);
    let selectors = consumer_changes(&original);
    // Catalogue closure authenticates the changed Local edge before Main collection.
    assert_eq!(
        apply_consumers(
            original,
            &selectors,
            &main_overlays(),
            &[local_overlay()],
            &[]
        ),
        Err(TeletextObjectPageSelectionError::Service(
            TeletextServiceError::Objects(TeletextObjectCatalogError::Invocation {
                owner: local(),
                command: 0,
                reason: TeletextObjectInvocationError::Type {
                    target: child(true),
                    expected: TeletextObjectType::Passive,
                    actual: TeletextObjectType::Active
                },
            })
        ))
    );
    let original = bind_input(consumer_input(false));
    let selectors = consumer_changes(&original);
    let missing = TeletextObjectIdentity::public(
        identity(TeletextResourceKind::Pop, 0xa4, 0),
        TeletextObjectId::new(12),
    )
    .unwrap();
    let mut main = main_overlays();
    main[0] = TeletextMainResourceInvocationOverlay::new(
        owner(0),
        0,
        calls(false).commands()[0],
        program(missing).commands()[0],
    )
    .unwrap();
    assert!(
        matches!(apply_consumers(original, &selectors, &main, &[local_overlay()], &[]),
        Err(TeletextObjectPageSelectionError::Service(TeletextServiceError::Collection {
            error: TeletextEnhancementCollectionError { display, source_location: TeletextEnhancementSource { object: None, command: 0 },
                reason: TeletextEnhancementCollectionFailure::Invocation(TeletextObjectInvocationError::Missing { target }) }, ..
        })) if display == owner(0) && target == missing)
    );
}

#[test]
fn main_local_consumer_budgets_cover_combined_raw_maps_and_final_spelling_before_clones() {
    let original = bind_input(consumer_input(false));
    let selectors = consumer_changes(&original);
    let main = main_overlays();
    let local = [local_overlay()];
    let resources = [overlay()];
    let run = |main: &[TeletextMainResourceInvocationOverlay],
               local: &[TeletextLocalObjectGlyphProgramOverlay],
               resources: &[TeletextResourceObjectProgramOverlay],
               budget| {
        original.clone().with_object_page_selections_and_consumers(
            &selectors,
            TeletextObjectPageSelectionConsumers {
                main,
                local,
                resources,
            },
            budget,
            super::budget(),
        )
    };
    // Four Main raw commands + four Local raw commands fit nine. Adding two
    // resource raw commands must reject even though every individual list fits.
    assert!(run(
        &main,
        &local,
        &[],
        TeletextServiceBudget {
            max_commands: 9,
            ..limits()
        }
    )
    .is_ok());
    assert!(matches!(
        run(
            &main,
            &local,
            &resources,
            TeletextServiceBudget {
                max_commands: 9,
                ..limits()
            }
        ),
        Err(TeletextObjectPageSelectionError::Service(
            TeletextServiceError::Budget {
                dimension: "authored command",
                maximum: 9
            }
        ))
    ));
    assert!(matches!(
        run(
            &main,
            &vec![local_overlay(); 4],
            &vec![overlay(); 3],
            TeletextServiceBudget {
                max_objects: 6,
                ..limits()
            }
        ),
        Err(TeletextObjectPageSelectionError::Service(
            TeletextServiceError::Budget {
                dimension: "object",
                maximum: 6
            }
        ))
    ));
    assert!(matches!(
        run(
            &vec![main[0]; 65],
            &local,
            &[],
            TeletextServiceBudget {
                max_structure_items: 64,
                ..limits()
            }
        ),
        Err(TeletextObjectPageSelectionError::Service(
            TeletextServiceError::Budget {
                dimension: "structure item",
                maximum: 64
            }
        ))
    ));
    check_consumer_final_spelling_budget(&original, &selectors, &main);
}
fn check_consumer_final_spelling_budget(
    original: &TeletextDeclaredService,
    selectors: &[TeletextObjectPageSelectionChange],
    main: &[TeletextMainResourceInvocationOverlay],
) {
    let run = |local: &[TeletextLocalObjectGlyphProgramOverlay], budget| {
        original.clone().with_object_page_selections_and_consumers(
            selectors,
            TeletextObjectPageSelectionConsumers {
                main,
                local,
                resources: &[],
            },
            budget,
            super::budget(),
        )
    };
    let mut replacement = calls(true).commands().to_vec();
    replacement.push(TeletextEnhancementCommand::AtColumn {
        column: 0,
        operation: TeletextColumnOperation::G0 {
            code: TeletextGlyphCode::new(b'B').unwrap(),
        },
    });
    let grown = [
        TeletextLocalObjectGlyphProgramOverlay::new_with_invocation_changes(
            self::local(),
            calls(false),
            TeletextEnhancementProgram::new(replacement).unwrap(),
            vec![Some(0), Some(1), None],
        )
        .unwrap(),
    ];
    // Final nine old commands + one inserted Local glyph includes the retained
    // Base spelling; raw inputs and the original service still fit nine.
    assert!(run(
        &grown,
        TeletextServiceBudget {
            max_commands: 10,
            ..limits()
        }
    )
    .is_ok());
    assert!(matches!(
        run(
            &grown,
            TeletextServiceBudget {
                max_commands: 9,
                ..limits()
            }
        ),
        Err(TeletextObjectPageSelectionError::Service(
            TeletextServiceError::Budget {
                dimension: "authored command",
                maximum: 9
            }
        ))
    ));
}
