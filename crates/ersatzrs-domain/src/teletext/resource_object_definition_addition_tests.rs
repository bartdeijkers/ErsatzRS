//! Existing resource catalogue additions, not import or publication authority.
use super::*;

#[path = "object_resource_addition_tests.rs"]
mod object_resource_addition_tests;

#[path = "resource_object_definition_removal_tests.rs"]
mod resource_object_definition_removal_tests;

fn identity(global: bool, page: u8, s1: u8, id: u16) -> TeletextObjectIdentity {
    let resource = resource(
        if global {
            TeletextResourceKind::Gpop
        } else {
            TeletextResourceKind::Pop
        },
        page,
        s1,
    );
    if global {
        TeletextObjectIdentity::global(resource, TeletextObjectId::new(id))
    } else {
        TeletextObjectIdentity::public(resource, TeletextObjectId::new(id))
    }
    .unwrap()
}
fn glyph(code: u8) -> TeletextEnhancementCommand {
    TeletextEnhancementCommand::AtColumn {
        column: 0,
        operation: TeletextColumnOperation::G0 {
            code: TeletextGlyphCode::new(code).unwrap(),
        },
    }
}
fn empty_resources() -> Vec<TeletextResourceIdentity> {
    vec![
        resource(TeletextResourceKind::Pop, 0xa2, 3),
        resource(TeletextResourceKind::Gpop, 0xa3, 4),
    ]
}
fn input() -> TeletextResolvedServiceDraft {
    let mut value = draft(&[address(200, 0), address(200, 1)]);
    let public = identity(false, 0xa0, 1, 19);
    let global = identity(true, 0xa1, 0, 19);
    for page in &mut value.pages {
        page.presentation = Some(presentation(0, 1));
        page.resources.objects = TeletextResolvedObjectPageLinks {
            public: Some(raw(0xa0)),
            global: Some(raw(0xa1)),
        };
        page.resources.defaults[0] = Some(selection(global, TeletextObjectType::Passive));
        page.resources.drcs = TeletextDrcsPageLinks::new(Some(raw(0xb0)), None).unwrap();
        page.local_program = Some(program(vec![
            invoke(public, TeletextObjectType::Passive),
            invoke(global, TeletextObjectType::Passive),
        ]));
    }
    value.objects = vec![
        definition(public, TeletextObjectType::Passive, vec![glyph(b'A')]),
        definition(global, TeletextObjectType::Passive, vec![glyph(b'P')]),
    ];
    value.drcs.push(table(0xb0, 0, 1));
    value
}
fn service(value: TeletextResolvedServiceDraft) -> TeletextSemanticService {
    TeletextSemanticService::new_with_object_companions(value, empty_resources(), budget())
        .unwrap()
        .with_physical_base_overlays(
            vec![TeletextPhysicalBaseOverlay::new(
                address(200, 1),
                vec![TeletextPhysicalBaseRow::new(5, [b'P'; 40]).unwrap()],
            )
            .unwrap()],
            budget(),
        )
        .unwrap()
}
fn addition(
    definition: TeletextEnhancementObjectDefinition,
) -> TeletextResourceObjectDefinitionAddition {
    TeletextResourceObjectDefinitionAddition::new(definition, budget()).unwrap()
}
fn additions() -> Vec<TeletextResourceObjectDefinitionAddition> {
    let passive = identity(false, 0xa0, 1, 25);
    let adaptive = identity(true, 0xa1, 0, 26);
    vec![
        addition(definition(
            passive,
            TeletextObjectType::Passive,
            vec![glyph(b'B')],
        )),
        addition(definition(
            adaptive,
            TeletextObjectType::Adaptive,
            vec![invoke(passive, TeletextObjectType::Passive)],
        )),
        addition(definition(
            identity(true, 0xa1, 0, 27),
            TeletextObjectType::Active,
            vec![invoke(adaptive, TeletextObjectType::Adaptive)],
        )),
        addition(definition(
            identity(false, 0xa2, 3, 28),
            TeletextObjectType::Passive,
            vec![glyph(b'Q')],
        )),
        addition(definition(
            identity(true, 0xa3, 4, 29),
            TeletextObjectType::Passive,
            vec![glyph(b'R')],
        )),
    ]
}

#[test]
fn resource_definition_addition_constructor_keeps_all_native_types_and_checked_scope_bounds() {
    for global in [false, true] {
        for object_type in [
            TeletextObjectType::Active,
            TeletextObjectType::Adaptive,
            TeletextObjectType::Passive,
        ] {
            let definition = definition(
                identity(global, if global { 0xa1 } else { 0xa0 }, 0, 25),
                object_type,
                vec![glyph(b'B')],
            );
            let changed = addition(definition.clone());
            assert_eq!(changed.definition(), &definition);
            assert_eq!(changed.identity(), definition.identity);
            assert!(changed.identity().owner().is_none());
        }
    }
    let local = definition(
        TeletextObjectIdentity::local(address(200, 0), TeletextObjectId::new(25)),
        TeletextObjectType::Passive,
        vec![glyph(b'B')],
    );
    assert_eq!(
        TeletextResourceObjectDefinitionAddition::new(local, budget()),
        Err(TeletextResourceObjectDefinitionAdditionError::Scope)
    );
    let value = additions().remove(0).definition().clone();
    for (dimension, limits) in [
        (
            "object",
            TeletextServiceBudget {
                max_objects: 0,
                ..budget()
            },
        ),
        (
            "structure item",
            TeletextServiceBudget {
                max_structure_items: 0,
                ..budget()
            },
        ),
        (
            "authored command",
            TeletextServiceBudget {
                max_commands: 0,
                ..budget()
            },
        ),
    ] {
        assert_eq!(
            TeletextResourceObjectDefinitionAddition::new(value.clone(), limits),
            Err(TeletextResourceObjectDefinitionAdditionError::Budget {
                dimension,
                maximum: 0
            })
        );
    }
    let empty = definition(
        identity(false, 0xa0, 1, 25),
        TeletextObjectType::Passive,
        vec![],
    );
    TeletextResourceObjectDefinitionAddition::new(
        empty,
        TeletextServiceBudget {
            max_commands: 0,
            ..budget()
        },
    )
    .unwrap();
}

#[test]
fn resource_definition_additions_preserve_old_definitions_consumers_and_fill_empty_resources() {
    let original_input = input();
    let original = service(original_input.clone());
    let snapshot = original.clone();
    let additions = additions();
    let edited = original
        .clone()
        .with_resource_object_definition_additions(&additions, budget())
        .unwrap();
    let mut expected = original_input;
    expected.objects.extend(
        additions
            .iter()
            .map(|addition| addition.definition().clone()),
    );
    let expected = TeletextSemanticService::new(expected, budget())
        .unwrap()
        .with_physical_base_overlays(
            original
                .physical_base_overlays()
                .values()
                .cloned()
                .collect(),
            budget(),
        )
        .unwrap();
    assert_eq!(edited, expected);
    assert_eq!(original, snapshot);
    assert!(edited.object_companions().is_empty());
    assert_eq!(
        &edited.objects().definitions()[..2],
        original.objects().definitions()
    );
    assert_eq!(edited.base(), original.base());
    assert_eq!(edited.drcs(), original.drcs());
    assert_eq!(
        edited.magazine_presentations(),
        original.magazine_presentations()
    );
    assert_eq!(
        edited.physical_base_overlays(),
        original.physical_base_overlays()
    );
    for page in original.page_enhancements().keys() {
        assert_eq!(
            edited.page_enhancements()[page],
            original.page_enhancements()[page]
        );
        assert_eq!(edited.collected(*page), original.collected(*page));
    }
    let partial = original
        .clone()
        .with_resource_object_definition_additions(&additions[3..4], budget())
        .unwrap();
    assert_eq!(partial.object_companions(), &empty_resources()[1..]);
    assert_eq!(
        &partial.objects().definitions()[..2],
        original.objects().definitions()
    );
    // Batch order need not be dependency order; the ordinary full catalog resolves it.
    let mut reversed = additions;
    reversed.reverse();
    let reversed = original
        .with_resource_object_definition_additions(&reversed, budget())
        .unwrap();
    assert_eq!(
        reversed.objects().definitions().len(),
        edited.objects().definitions().len()
    );
}

#[test]
fn resource_definition_additions_refuse_collisions_missing_owners_and_partial_batches() {
    let original = service(input());
    let snapshot = original.clone();
    assert_eq!(
        original
            .clone()
            .with_resource_object_definition_additions(&[], budget()),
        Err(TeletextResourceObjectDefinitionAdditionApplyError::Empty)
    );
    let added = additions().remove(0);
    assert_eq!(
        original
            .clone()
            .with_resource_object_definition_additions(&[added.clone(), added], budget()),
        Err(
            TeletextResourceObjectDefinitionAdditionApplyError::Duplicate {
                identity: identity(false, 0xa0, 1, 25)
            }
        )
    );
    let occupied = addition(original.objects().definitions()[0].clone());
    assert_eq!(
        original
            .clone()
            .with_resource_object_definition_additions(&[occupied], budget()),
        Err(
            TeletextResourceObjectDefinitionAdditionApplyError::Occupied {
                identity: identity(false, 0xa0, 1, 19)
            }
        )
    );
    for missing in [
        identity(false, 0xa0, 2, 25),
        identity(true, 0xa0, 1, 25),
        identity(false, 0xa4, 0, 25),
    ] {
        let mut batch = additions();
        batch.push(addition(definition(
            missing,
            TeletextObjectType::Passive,
            vec![glyph(b'B')],
        )));
        assert_eq!(
            original
                .clone()
                .with_resource_object_definition_additions(&batch, budget()),
            Err(
                TeletextResourceObjectDefinitionAdditionApplyError::MissingResource {
                    identity: missing.resource().unwrap()
                }
            )
        );
    }
    assert_eq!(original, snapshot);
}

#[test]
fn resource_definition_additions_validate_unused_graph_dependencies_types_and_cycles() {
    let original = service(input());
    let new = identity(false, 0xa0, 1, 25);
    for target in [identity(false, 0xa0, 1, 99), new] {
        let value = addition(definition(
            new,
            TeletextObjectType::Active,
            vec![invoke(target, TeletextObjectType::Active)],
        ));
        assert!(matches!(
            original
                .clone()
                .with_resource_object_definition_additions(&[value], budget()),
            Err(TeletextResourceObjectDefinitionAdditionApplyError::Service(
                TeletextServiceError::Objects(TeletextObjectCatalogError::Invocation { .. })
            ))
        ));
    }
    let value = addition(definition(
        new,
        TeletextObjectType::Active,
        vec![invoke(
            identity(false, 0xa0, 1, 19),
            TeletextObjectType::Adaptive,
        )],
    ));
    assert!(matches!(
        original
            .clone()
            .with_resource_object_definition_additions(&[value], budget()),
        Err(TeletextResourceObjectDefinitionAdditionApplyError::Service(
            TeletextServiceError::Objects(TeletextObjectCatalogError::Invocation {
                reason: TeletextObjectInvocationError::Type { .. },
                ..
            })
        ))
    ));
    let mut input = input();
    let local = TeletextObjectIdentity::local(address(200, 0), TeletextObjectId::new(50));
    input.objects.push(definition(
        local,
        TeletextObjectType::Passive,
        vec![glyph(b'L')],
    ));
    let original = service(input);
    let value = addition(definition(
        new,
        TeletextObjectType::Active,
        vec![invoke(local, TeletextObjectType::Passive)],
    ));
    assert!(matches!(
        original.with_resource_object_definition_additions(&[value], budget()),
        Err(TeletextResourceObjectDefinitionAdditionApplyError::Service(
            TeletextServiceError::Objects(TeletextObjectCatalogError::Invocation {
                reason: TeletextObjectInvocationError::ResourceToLocal { .. },
                ..
            })
        ))
    ));
}

#[test]
fn resource_definition_additions_preflight_aggregate_budgets_before_rebuilding() {
    let original = service(input());
    let snapshot = original.clone();
    let additions = additions();
    let exact = TeletextServiceBudget {
        max_objects: 7,
        max_commands: 13,
        max_records: 7,
        max_collected_commands: 8,
        max_base_content_bytes: 40,
        ..budget()
    };
    original
        .clone()
        .with_resource_object_definition_additions(&additions, exact)
        .unwrap();
    for limits in [
        TeletextServiceBudget {
            max_objects: 6,
            ..exact
        },
        TeletextServiceBudget {
            max_commands: 12,
            ..exact
        },
        TeletextServiceBudget {
            max_records: 6,
            ..exact
        },
        TeletextServiceBudget {
            max_collected_commands: 7,
            ..exact
        },
        TeletextServiceBudget {
            max_base_content_bytes: 39,
            ..exact
        },
    ] {
        assert!(matches!(
            original
                .clone()
                .with_resource_object_definition_additions(&additions, limits),
            Err(TeletextResourceObjectDefinitionAdditionApplyError::Service(
                TeletextServiceError::Budget { .. }
            ))
        ));
    }
    let maximum = TeletextServiceBudget {
        max_objects: usize::MAX,
        max_commands: usize::MAX,
        max_structure_items: usize::MAX,
        ..budget()
    };
    original
        .clone()
        .with_resource_object_definition_additions(&additions, maximum)
        .unwrap();
    assert_eq!(original, snapshot);
}

#[test]
fn resource_definition_additions_declared_wrapper_preserves_inventory_and_resolution() {
    let mut value = input();
    let unused = TeletextX27LinkDeclaration::new(
        TeletextX27LinkTarget::Unspecified {
            magazine: TeletextX27Magazine::new(2).unwrap(),
        },
        TeletextX27LinkValidity::Level25Only,
        TeletextSubpageRequirementMask::new(u16::MAX),
    )
    .unwrap();
    let link = |page, mask| {
        TeletextX27LinkDeclaration::new(
            TeletextX27LinkTarget::Page { page: raw(page) },
            TeletextX27LinkValidity::Level25Only,
            TeletextSubpageRequirementMask::new(mask),
        )
        .unwrap()
    };
    for page in &mut value.pages {
        page.resources.defaults = [None; 4];
        page.compositional_links = Some(
            TeletextX27Level25Links::new(link(0xa1, 1), link(0xa0, 2), unused, link(0xb0, 1))
                .unwrap(),
        );
    }
    let original = TeletextDeclaredService::new(
        service(value),
        vec![],
        [address(200, 0), address(200, 1)]
            .map(|display| TeletextDisplayMotSelection { display, mot: None })
            .to_vec(),
        empty_resources(),
        TeletextDeclaredServiceBudget {
            max_mots: 8,
            max_displays: 8,
            max_resources: 16,
            max_defaults: 16,
            max_objects: 16,
        },
    )
    .unwrap();
    let snapshot = original.clone();
    let edited = original
        .clone()
        .with_resource_object_definition_additions(&additions(), budget())
        .unwrap();
    assert_eq!(original, snapshot);
    assert!(edited.empty_object_pages().is_empty());
    assert_eq!(edited.resource_inventory(), original.resource_inventory());
    assert_eq!(edited.mots(), original.mots());
    assert_eq!(edited.defaults(), original.defaults());
    assert_eq!(edited.selections(), original.selections());
    assert_eq!(edited.resolutions(), original.resolutions());
    assert_eq!(
        &edited.semantic().objects().definitions()[..2],
        original.semantic().objects().definitions()
    );
    let removals: Vec<_> = additions()
        .iter()
        .map(TeletextResourceObjectDefinitionAddition::identity)
        .collect();
    let restored = edited
        .without_unused_resource_object_definitions(&removals, budget())
        .unwrap();
    assert_eq!(restored, original);
}

#[test]
fn resource_definition_additions_keep_contextual_drcs_unused_and_validate_every_invoked_reference()
{
    let owner = identity(false, 0xa0, 1, 19);
    let added_identity = identity(false, 0xa0, 1, 25);
    for (table, slot) in [
        (TeletextDrcsTableScope::Normal, 0),
        (TeletextDrcsTableScope::Normal, 1),
        (TeletextDrcsTableScope::Global, 0),
    ] {
        let mut original_input = input();
        original_input.objects[0].object_type = TeletextObjectType::Active;
        for page in &mut original_input.pages {
            page.local_program = Some(program(vec![
                invoke(owner, TeletextObjectType::Active),
                invoke(identity(true, 0xa1, 0, 19), TeletextObjectType::Passive),
            ]));
        }
        let original = service(original_input.clone());
        let new = definition(
            added_identity,
            TeletextObjectType::Passive,
            vec![TeletextEnhancementCommand::AtColumn {
                column: 0,
                operation: TeletextColumnOperation::Drcs {
                    reference: TeletextDrcsGlyphReference {
                        table,
                        subtable: TeletextDrcsSubtableIndex::new(0).unwrap(),
                        slot: TeletextDrcsGlyphSlot::new(slot).unwrap(),
                    },
                },
            }],
        );
        // Normal/Global references acquire a resource page from each consumer.
        // The unchanged legacy constructor therefore permits unused definitions
        // without inventing an owner or resolving a speculative future bank.
        original_input.objects.push(new.clone());
        let legacy_unused = service(original_input);
        let added_unused = original
            .clone()
            .with_resource_object_definition_additions(&[addition(new)], budget())
            .unwrap();
        assert_eq!(added_unused, legacy_unused);
        let snapshot = added_unused.clone();
        let expected = original
            .objects()
            .definition(owner)
            .unwrap()
            .program
            .clone();
        let mut replacement = expected.commands().to_vec();
        replacement.push(invoke(added_identity, TeletextObjectType::Passive));
        let change = TeletextResourceObjectProgramOverlay::new_with_invocation_structure(
            owner,
            expected,
            program(replacement),
            vec![Some(0), None],
            budget(),
        )
        .unwrap();
        let invoked = added_unused
            .clone()
            .with_resource_object_program_overlays(&[change], budget());
        let source = TeletextEnhancementSource {
            object: Some(added_identity),
            command: 0,
        };
        if table == TeletextDrcsTableScope::Normal && slot == 0 {
            let invoked = invoked.unwrap();
            for page in [address(200, 0), address(200, 1)] {
                assert_eq!(invoked.collected(page).unwrap().1.sources(), &[source]);
            }
        } else {
            let TeletextServiceError::CollectedDrcs { error, .. } = invoked.unwrap_err() else {
                panic!("invoking the new definition must use the ordinary DRCS collector")
            };
            assert_eq!(error.source_location, Some(source));
            let expected = if table == TeletextDrcsTableScope::Normal {
                TeletextDrcsResourceError::MissingGlyph {
                    display: address(200, 0),
                    reference: 0,
                    identity: resource(TeletextResourceKind::Drcs, 0xb0, 0),
                    slot: TeletextDrcsGlyphSlot::new(1).unwrap(),
                }
            } else {
                TeletextDrcsResourceError::MissingLink {
                    display: address(200, 0),
                    reference: 0,
                    table,
                }
            };
            assert_eq!(error.error, expected);
        }
        assert_eq!(
            added_unused, snapshot,
            "failed invocation cannot mutate the unused service"
        );
    }
}
