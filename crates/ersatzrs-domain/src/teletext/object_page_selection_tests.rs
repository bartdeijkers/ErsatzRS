//! One final complete draft closes the sequential raw-owner/consumer deadlock.
use super::*;

fn limits() -> TeletextServiceBudget {
    TeletextServiceBudget {
        max_records: 64,
        max_objects: 64,
        max_structure_items: 1024,
        max_base_content_bytes: 1024,
        max_commands: 1024,
        max_collected_commands: 1024,
        max_collected_drcs_references: 64,
    }
}
fn global(byte: u8, s1: u8) -> TeletextObjectIdentity {
    TeletextObjectIdentity::global(
        identity(TeletextResourceKind::Gpop, byte, s1),
        TeletextObjectId::new(19),
    )
    .unwrap()
}
fn program(target: TeletextObjectIdentity) -> TeletextEnhancementProgram {
    TeletextEnhancementProgram::new(vec![TeletextEnhancementCommand::InvokeObject {
        target,
        object_type: TeletextObjectType::Passive,
        offset: None,
    }])
    .unwrap()
}
fn link(byte: u8, mask: u16) -> TeletextX27LinkDeclaration {
    TeletextX27LinkDeclaration::new(
        TeletextX27LinkTarget::Page { page: raw(byte) },
        TeletextX27LinkValidity::Level25Only,
        TeletextSubpageRequirementMask::new(mask),
    )
    .unwrap()
}
fn links(public: u8, global: u8, mask: u16) -> TeletextX27Level25Links {
    let unused = TeletextX27LinkDeclaration::new(
        unused(),
        TeletextX27LinkValidity::Level25And35,
        TeletextSubpageRequirementMask::new(u16::MAX),
    )
    .unwrap();
    TeletextX27Level25Links::new(link(global, mask), link(public, 1), unused, unused).unwrap()
}
fn input(explicit: bool, changed: bool) -> TeletextResolvedServiceDraft {
    let mut input = draft();
    input.base.pages[0].rows.push(TeletextRow {
        row: 2,
        runs: vec![TeletextRun {
            column: 4,
            style: TeletextStyle::default(),
            content: TeletextRunContent::Text { text: "£".into() },
        }],
    });
    let mut second = input.base.pages[0].clone();
    second.address = owner(32);
    input.base.pages.push(second);
    input.objects[0].object_type = TeletextObjectType::Active;
    input.objects[0].program = program(if changed {
        global(0xa3, 15)
    } else {
        global(0xa2, 0)
    });
    for id in [global(0xa2, 0), global(0xa3, 15)] {
        input.objects.push(TeletextEnhancementObjectDefinition {
            identity: id,
            object_type: TeletextObjectType::Passive,
            program: TeletextEnhancementProgram::new(vec![]).unwrap(),
        });
    }
    input.pages[0].resources.defaults[2]
        .as_mut()
        .unwrap()
        .object_type = TeletextObjectType::Active;
    input.pages[0].resources.objects.global = Some(raw(if changed { 0xa3 } else { 0xa2 }));
    input.pages[0].compositional_links = (explicit || changed).then(|| {
        links(
            0xa0,
            if changed { 0xa3 } else { 0xa2 },
            if changed { 0x8000 } else { 1 },
        )
    });
    let mut second = input.pages[0].clone();
    second.address = owner(32);
    input.pages.push(second);
    input
}
fn bind_input(input: TeletextResolvedServiceDraft) -> TeletextDeclaredService {
    bind_input_supersedable(input, true)
}
fn bind_input_supersedable(
    input: TeletextResolvedServiceDraft,
    supersedable: bool,
) -> TeletextDeclaredService {
    bind_input_mode(input, supersedable, true)
}
fn bind_input_mode(
    input: TeletextResolvedServiceDraft,
    supersedable: bool,
    with_mot: bool,
) -> TeletextDeclaredService {
    let mut public = [empty_object(); 7];
    public[0] = object(
        TeletextX27LinkTarget::Page { page: raw(0xa0) },
        0,
        [
            TeletextMotDefaultObject::Object {
                target: target(0xa0),
                object_type: TeletextObjectType::Active,
            },
            TeletextMotDefaultObject::NoObject { raw_number: 77 },
        ],
        1,
    );
    let mot = mot(
        public,
        TeletextMotObjectLink::new(
            TeletextX27LinkTarget::Page { page: raw(0xa2) },
            TeletextMotSubpageExtent::new(0).unwrap(),
            supersedable,
            TeletextMotFallback::new(0).unwrap(),
            [TeletextMotDefaultObject::NoObject { raw_number: 19 }; 2],
        ),
        true,
    );
    let empty = vec![identity(TeletextResourceKind::Pop, 0xa1, 0)];
    let semantic =
        TeletextSemanticService::new_with_object_companions(input, empty.clone(), limits())
            .unwrap()
            .with_physical_base_overlays(
                vec![TeletextPhysicalBaseOverlay::new(
                    owner(32),
                    vec![TeletextPhysicalBaseRow::new(5, [b'P'; 40]).unwrap()],
                )
                .unwrap()],
                limits(),
            )
            .unwrap()
            .with_base_text_spellings(
                vec![TeletextBaseTextSpelling::new(
                    owner(0),
                    TeletextEnhancementPosition::new(2, 4).unwrap(),
                    '£',
                    TeletextGlyphCode::new(b' ').unwrap(),
                    TeletextEnhancementCharsetDesignation::new(0).unwrap(),
                    TeletextColumnOperation::G2 {
                        code: TeletextGlyphCode::new(0x23).unwrap(),
                    },
                )
                .unwrap()],
                limits(),
            )
            .unwrap();
    TeletextDeclaredService::new(
        semantic,
        if with_mot { vec![mot] } else { vec![] },
        [owner(0), owner(32)]
            .map(|display| TeletextDisplayMotSelection {
                display,
                mot: with_mot.then_some(identity(TeletextResourceKind::Mot, 0xfe, 9)),
            })
            .to_vec(),
        empty,
        budget(),
    )
    .unwrap()
}
fn bind_input_without_mot(changed: bool) -> TeletextDeclaredService {
    let mut source = input(true, changed);
    for page in &mut source.pages {
        page.resources.defaults = [None; 4];
        page.local_program = Some(
            TeletextEnhancementProgram::new(vec![TeletextEnhancementCommand::InvokeObject {
                target: target(0xa0),
                object_type: TeletextObjectType::Active,
                offset: None,
            }])
            .unwrap(),
        );
    }
    bind_input_mode(source, true, false)
}
fn change(
    original: &TeletextDeclaredService,
    display: TeletextPageAddress,
    role: TeletextX27LinkRole,
    replacement: TeletextX27LinkDeclaration,
) -> TeletextObjectPageSelectionChange {
    TeletextObjectPageSelectionChange::new(
        display,
        role,
        original.semantic().page_enhancements()[&display].compositional_links,
        *original
            .resolutions()
            .iter()
            .find(|resolution| resolution.display() == display)
            .unwrap()
            .role(role),
        replacement,
    )
    .unwrap()
}
fn changes(original: &TeletextDeclaredService) -> Vec<TeletextObjectPageSelectionChange> {
    [owner(0), owner(32)]
        .map(|display| {
            change(
                original,
                display,
                TeletextX27LinkRole::Gpop,
                link(0xa3, 0x8000),
            )
        })
        .to_vec()
}
fn overlay() -> TeletextResourceObjectProgramOverlay {
    TeletextResourceObjectProgramOverlay::new_with_invocation_changes(
        target(0xa0),
        program(global(0xa2, 0)),
        program(global(0xa3, 15)),
        vec![Some(0)],
        limits(),
    )
    .unwrap()
}

#[test]
fn atomic_object_page_selection_solves_sequential_deadlock_and_preserves_complete_originals() {
    for explicit in [false, true] {
        let original = bind_input(input(explicit, false));
        let before = original.clone();
        let changes = changes(&original);
        assert!(original
            .clone()
            .with_object_page_selections_and_program_overlays(&changes, &[], limits(), budget())
            .is_err());
        assert!(original
            .clone()
            .with_resource_object_program_overlays(&[overlay()], limits())
            .is_err());
        let edited = original
            .clone()
            .with_object_page_selections_and_program_overlays(
                &changes,
                &[overlay()],
                limits(),
                budget(),
            )
            .unwrap();
        // Independent final fixture, not a reconstruction from the returned fields.
        let mut expected_input = input(explicit, true);
        if !explicit {
            // Materialization preserves the exact inherited four-role masks/validity.
            let absent = TeletextX27LinkDeclaration::new(
                unused(),
                TeletextX27LinkValidity::Level25Only,
                TeletextSubpageRequirementMask::new(u16::MAX),
            )
            .unwrap();
            let expected =
                TeletextX27Level25Links::new(link(0xa3, 0x8000), link(0xa0, 1), absent, absent)
                    .unwrap();
            for page in &mut expected_input.pages {
                page.compositional_links = Some(expected);
            }
        }
        assert_eq!(edited, bind_input(expected_input));
        assert_eq!(original, before);
        assert_eq!(edited.defaults(), original.defaults());
        assert_eq!(edited.mots(), original.mots());
        assert_eq!(edited.selections(), original.selections());
        assert_eq!(edited.resource_inventory(), original.resource_inventory());
        assert_eq!(edited.empty_object_pages(), original.empty_object_pages());
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
            assert!(matches!(
                new.role(TeletextX27LinkRole::Gpop),
                TeletextMotRoleResolution::Selected {
                    original_mot: Some(_),
                    ..
                }
            ));
        }
    }
    let original = bind_input_without_mot(false);
    let intents = changes(&original);
    let edited = original
        .clone()
        .with_object_page_selections_and_program_overlays(
            &intents,
            &[overlay()],
            limits(),
            budget(),
        )
        .unwrap();
    assert_eq!(edited, bind_input_without_mot(true));
    assert_eq!(edited.empty_object_pages(), original.empty_object_pages());
    assert!(edited.mots().is_empty());
    assert!(edited.defaults().is_empty());
    for resolution in edited.resolutions() {
        assert!(matches!(
            resolution.role(TeletextX27LinkRole::Gpop),
            TeletextMotRoleResolution::Selected {
                original_mot: None,
                provenance: TeletextMotLinkProvenance::X27 { .. },
                ..
            }
        ));
    }
}

#[test]
fn atomic_object_page_selection_bundles_pop_gpop_without_rebinding_mot_defaults() {
    let original = bind_input(input(true, false));
    let mut changes = changes(&original);
    for display in [owner(0), owner(32)] {
        changes.push(change(
            &original,
            display,
            TeletextX27LinkRole::Pop,
            link(0xa1, 1),
        ));
    }
    let edited = original
        .with_object_page_selections_and_program_overlays(
            &changes,
            &[overlay()],
            limits(),
            budget(),
        )
        .unwrap();
    let mut expected = input(true, true);
    for page in &mut expected.pages {
        page.resources.objects.public = Some(raw(0xa1));
        page.compositional_links = Some(links(0xa1, 0xa3, 0x8000));
    }
    assert_eq!(edited, bind_input(expected));
    assert_eq!(edited.defaults()[0].selection().target, target(0xa0));
}

#[test]
fn atomic_object_page_selection_rejects_omitted_consumers_and_unannounced_used_s1() {
    let original = bind_input(input(true, false));
    let changes = changes(&original);
    assert!(original
        .clone()
        .with_object_page_selections_and_program_overlays(
            &changes[..1],
            &[overlay()],
            limits(),
            budget()
        )
        .is_err());
    let wrong_mask: Vec<_> = [owner(0), owner(32)]
        .map(|display| change(&original, display, TeletextX27LinkRole::Gpop, link(0xa3, 0)))
        .to_vec();
    let rejected = original.with_object_page_selections_and_program_overlays(
        &wrong_mask,
        &[overlay()],
        limits(),
        budget(),
    );
    assert_eq!(
        rejected,
        Err(TeletextObjectPageSelectionError::Service(
            TeletextServiceError::CompositionalUsage {
                display: owner(0),
                role: TeletextX27LinkRole::Gpop,
                page: raw(0xa3),
                subpage: TeletextResourceSubpageIndex::new(15).unwrap(),
                source_location: TeletextEnhancementSource {
                    object: Some(target(0xa0)),
                    command: 0
                },
            }
        ))
    );
}

#[test]
fn atomic_object_page_selection_rejects_duplicate_stale_missing_and_noop_intent() {
    use TeletextObjectPageSelectionError as Error;
    let original = bind_input(input(true, false));
    let changes = changes(&original);
    let first = changes[0];
    let apply = |changes: &[TeletextObjectPageSelectionChange],
                 overlays: &[TeletextResourceObjectProgramOverlay]| {
        original
            .clone()
            .with_object_page_selections_and_program_overlays(changes, overlays, limits(), budget())
    };
    assert_eq!(apply(&[], &[overlay()]), Err(Error::Empty));
    assert!(matches!(
        apply(&[first, first], &[overlay()]),
        Err(Error::Duplicate { .. })
    ));
    let stale = TeletextObjectPageSelectionChange::new(
        first.display(),
        first.role(),
        Some(links(0xa0, 0xa2, 3)),
        first.expected_role(),
        first.replacement(),
    )
    .unwrap();
    assert!(matches!(
        apply(&[stale], &[overlay()]),
        Err(Error::OriginalMismatch { .. })
    ));
    let stale_role = match first.expected_role() {
        TeletextMotRoleResolution::Selected {
            requirements,
            provenance,
            original_mot,
            ..
        } => TeletextMotRoleResolution::Selected {
            target: raw(0xab),
            requirements,
            provenance,
            original_mot,
        },
        _ => panic!("selected fixture"),
    };
    let stale = TeletextObjectPageSelectionChange::new(
        first.display(),
        first.role(),
        first.expected_links(),
        stale_role,
        first.replacement(),
    )
    .unwrap();
    assert!(matches!(
        apply(&[stale], &[overlay()]),
        Err(Error::OriginalMismatch { .. })
    ));
    let missing = TeletextObjectPageSelectionChange::new(
        owner(1),
        first.role(),
        first.expected_links(),
        first.expected_role(),
        first.replacement(),
    )
    .unwrap();
    assert!(matches!(
        apply(&[missing], &[overlay()]),
        Err(Error::MissingDisplay { .. })
    ));
    let missing = change(
        &original,
        owner(0),
        TeletextX27LinkRole::Gpop,
        link(0xab, 1),
    );
    assert!(matches!(
        apply(&[missing], &[overlay()]),
        Err(Error::MissingResource { .. })
    ));
    check_invalid_selection_constructor_intent(first);
    check_stale_resource_program_intent(&original, &changes);
}

fn check_stale_resource_program_intent(
    original: &TeletextDeclaredService,
    changes: &[TeletextObjectPageSelectionChange],
) {
    use TeletextObjectPageSelectionError as Error;
    let apply = |overlays: &[TeletextResourceObjectProgramOverlay]| {
        original
            .clone()
            .with_object_page_selections_and_program_overlays(changes, overlays, limits(), budget())
    };
    let forward = overlay();
    // The existing overlay constructor checks correspondence; a reversed valid edit is stale here.
    let stale_program = TeletextResourceObjectProgramOverlay::new_with_invocation_changes(
        forward.identity(),
        forward.replacement().clone(),
        forward.expected().clone(),
        vec![Some(0)],
        limits(),
    )
    .unwrap();
    assert!(matches!(
        apply(&[stale_program]),
        Err(Error::Service(
            TeletextServiceError::ResourceObjectProgramOverlay {
                reason: TeletextResourceObjectProgramFailure::ProgramMismatch,
                ..
            }
        ))
    ));
    assert!(matches!(
        apply(&[overlay(), overlay()]),
        Err(Error::Service(
            TeletextServiceError::ResourceObjectProgramOverlay {
                reason: TeletextResourceObjectProgramFailure::Duplicate,
                ..
            }
        ))
    ));
}

fn check_invalid_selection_constructor_intent(first: TeletextObjectPageSelectionChange) {
    assert!(TeletextObjectPageSelectionChange::new(
        first.display(),
        first.role(),
        first.expected_links(),
        first.expected_role(),
        link(0xa2, 1)
    )
    .is_err());
    for role in [TeletextX27LinkRole::Drcs, TeletextX27LinkRole::Gdrcs] {
        assert!(TeletextObjectPageSelectionChange::new(
            first.display(),
            role,
            first.expected_links(),
            first.expected_role(),
            first.replacement()
        )
        .is_err());
    }
    let denied = TeletextX27LinkDeclaration::new(
        TeletextX27LinkTarget::Page { page: raw(0xa3) },
        TeletextX27LinkValidity::Level35Only,
        TeletextSubpageRequirementMask::new(0x8000),
    )
    .unwrap();
    assert!(TeletextObjectPageSelectionChange::new(
        first.display(),
        first.role(),
        first.expected_links(),
        first.expected_role(),
        denied
    )
    .is_err());
    assert!(TeletextObjectPageSelectionChange::new(
        first.display(),
        first.role(),
        first.expected_links(),
        TeletextMotRoleResolution::Unselected,
        first.replacement()
    )
    .is_err());
}

#[test]
fn atomic_object_page_selection_preserves_applicability_and_non_supersedable_mot_authority() {
    for explicit in [false, true] {
        let original = bind_input_supersedable(input(explicit, false), false);
        let changes = changes(&original);
        assert!(matches!(
            original.with_object_page_selections_and_program_overlays(
                &changes,
                &[overlay()],
                limits(),
                budget()
            ),
            Err(TeletextObjectPageSelectionError::Unresolved { .. })
        ));
    }
    let original = bind_input(input(true, false));
    let original_change = changes(&original)[0];
    let alternate = TeletextX27LinkDeclaration::new(
        TeletextX27LinkTarget::Page { page: raw(0xa3) },
        TeletextX27LinkValidity::Level25And35,
        TeletextSubpageRequirementMask::new(0x8000),
    )
    .unwrap();
    assert!(matches!(
        TeletextObjectPageSelectionChange::new(
            original_change.display(),
            original_change.role(),
            original_change.expected_links(),
            original_change.expected_role(),
            alternate
        ),
        Err(TeletextObjectPageSelectionError::InvalidChange)
    ));
    let original = bind_input(input(false, false));
    let resolution = *original.resolutions()[0].role(TeletextX27LinkRole::Gpop);
    let changed = TeletextObjectPageSelectionChange::new(
        owner(0),
        TeletextX27LinkRole::Gpop,
        None,
        resolution,
        alternate,
    )
    .unwrap();
    assert!(matches!(
        original.with_object_page_selections_and_program_overlays(
            &[changed],
            &[overlay()],
            limits(),
            budget()
        ),
        Err(TeletextObjectPageSelectionError::InvalidChange)
    ));
    let source = links(0xa0, 0xa2, 1);
    let level35 = TeletextX27LinkDeclaration::new(
        TeletextX27LinkTarget::Page { page: raw(0xa2) },
        TeletextX27LinkValidity::Level35Only,
        TeletextSubpageRequirementMask::new(1),
    )
    .unwrap();
    let expected = TeletextX27Level25Links::new(
        level35,
        source.link(TeletextX27LinkRole::Pop),
        source.link(TeletextX27LinkRole::Gdrcs),
        source.link(TeletextX27LinkRole::Drcs),
    )
    .unwrap();
    assert!(matches!(
        TeletextObjectPageSelectionChange::new(
            owner(0),
            TeletextX27LinkRole::Gpop,
            Some(expected),
            resolution,
            link(0xa3, 0x8000)
        ),
        Err(TeletextObjectPageSelectionError::InvalidChange)
    ));
}

#[test]
fn atomic_object_page_selection_reuses_complete_catalogue_type_and_missing_object_checks() {
    let original = bind_input(input(true, false));
    let changes = changes(&original);
    for (target_id, object_type) in [
        (global(0xa3, 15), TeletextObjectType::Adaptive),
        (global(0xab, 15), TeletextObjectType::Passive),
    ] {
        let replacement =
            TeletextEnhancementProgram::new(vec![TeletextEnhancementCommand::InvokeObject {
                target: target_id,
                object_type,
                offset: None,
            }])
            .unwrap();
        let overlay = TeletextResourceObjectProgramOverlay::new_with_invocation_changes(
            target(0xa0),
            program(global(0xa2, 0)),
            replacement,
            vec![Some(0)],
            limits(),
        )
        .unwrap();
        assert!(matches!(
            original
                .clone()
                .with_object_page_selections_and_program_overlays(
                    &changes,
                    &[overlay],
                    limits(),
                    budget()
                ),
            Err(TeletextObjectPageSelectionError::Service(_))
        ));
    }
    let absent = TeletextResourceObjectProgramOverlay::new_with_invocation_changes(
        target(0xab),
        program(global(0xa2, 0)),
        program(global(0xa3, 15)),
        vec![Some(0)],
        limits(),
    )
    .unwrap();
    assert!(matches!(
        original.with_object_page_selections_and_program_overlays(
            &changes,
            &[absent],
            limits(),
            budget()
        ),
        Err(TeletextObjectPageSelectionError::Service(
            TeletextServiceError::ResourceObjectProgramOverlay {
                reason: TeletextResourceObjectProgramFailure::MissingObject,
                ..
            }
        ))
    ));
}

#[test]
fn atomic_object_page_selection_fences_raw_existing_final_and_declared_budgets() {
    let original = bind_input(input(true, false));
    let changes = changes(&original);
    // Repeated input hits trusted bounds before duplicate maps or graph clones.
    for (raw_changes, raw_overlays) in [
        (vec![changes[0]; 1025], vec![overlay()]),
        (changes.clone(), vec![overlay(); 1025]),
    ] {
        assert!(matches!(
            original
                .clone()
                .with_object_page_selections_and_program_overlays(
                    &raw_changes,
                    &raw_overlays,
                    limits(),
                    budget()
                ),
            Err(TeletextObjectPageSelectionError::Service(
                TeletextServiceError::Budget { .. }
            ))
        ));
    }
    for denied in [
        TeletextServiceBudget {
            max_records: 0,
            ..limits()
        },
        TeletextServiceBudget {
            max_objects: 0,
            ..limits()
        },
        TeletextServiceBudget {
            max_base_content_bytes: 0,
            ..limits()
        },
        TeletextServiceBudget {
            max_commands: 0,
            ..limits()
        },
        TeletextServiceBudget {
            max_collected_commands: 0,
            ..limits()
        },
        TeletextServiceBudget {
            max_structure_items: 0,
            ..limits()
        },
    ] {
        assert!(matches!(
            original
                .clone()
                .with_object_page_selections_and_program_overlays(
                    &changes,
                    &[overlay()],
                    denied,
                    budget()
                ),
            Err(TeletextObjectPageSelectionError::Service(
                TeletextServiceError::Budget { .. }
            ))
        ));
    }
    for denied in [
        TeletextDeclaredServiceBudget {
            max_mots: 0,
            ..budget()
        },
        TeletextDeclaredServiceBudget {
            max_displays: 0,
            ..budget()
        },
        TeletextDeclaredServiceBudget {
            max_resources: 0,
            ..budget()
        },
        TeletextDeclaredServiceBudget {
            max_defaults: 0,
            ..budget()
        },
        TeletextDeclaredServiceBudget {
            max_objects: 0,
            ..budget()
        },
    ] {
        assert!(matches!(
            original
                .clone()
                .with_object_page_selections_and_program_overlays(
                    &changes,
                    &[overlay()],
                    limits(),
                    denied
                ),
            Err(TeletextObjectPageSelectionError::Declaration(_))
        ));
    }
    check_final_selection_spelling_budget(original, &changes);
}

fn check_final_selection_spelling_budget(
    original: TeletextDeclaredService,
    changes: &[TeletextObjectPageSelectionChange],
) {
    let mut replacement = program(global(0xa3, 15)).commands().to_vec();
    replacement.extend((0..3).map(|column| TeletextEnhancementCommand::AtColumn {
        column,
        operation: TeletextColumnOperation::G0 {
            code: TeletextGlyphCode::new(b'B').unwrap(),
        },
    }));
    let overlay = TeletextResourceObjectProgramOverlay::new_with_invocation_changes(
        target(0xa0),
        program(global(0xa2, 0)),
        TeletextEnhancementProgram::new(replacement).unwrap(),
        vec![Some(0), None, None, None],
        limits(),
    )
    .unwrap();
    // Two default roots + four final POP commands + retained Base spelling = seven.
    let exact = TeletextServiceBudget {
        max_commands: 7,
        ..limits()
    };
    assert!(original
        .clone()
        .with_object_page_selections_and_program_overlays(
            changes,
            std::slice::from_ref(&overlay),
            exact,
            budget()
        )
        .is_ok());
    assert!(matches!(
        original.with_object_page_selections_and_program_overlays(
            changes,
            &[overlay],
            TeletextServiceBudget {
                max_commands: 6,
                ..exact
            },
            budget()
        ),
        Err(TeletextObjectPageSelectionError::Service(
            TeletextServiceError::Budget {
                dimension: "authored command",
                maximum: 6
            }
        ))
    ));
}
