//! Synthetic closure/collection fixtures; not wire, spatial or publication proof.

use std::collections::BTreeSet;

use serde_json::json;

use super::enhancement_objects::TeletextEnhancementCollectionUsage;
use super::*;

fn display(page: u16, subcode: u16) -> TeletextPageAddress {
    TeletextPageAddress {
        page: TeletextPageNumber::new(page).unwrap(),
        subcode: TeletextSubcode::new(subcode).unwrap(),
    }
}

fn local(owner: TeletextPageAddress, id: u16) -> TeletextObjectIdentity {
    TeletextObjectIdentity::local(owner, TeletextObjectId::new(id))
}

fn raw(page: u8) -> TeletextRawDataPageNumber {
    TeletextRawDataPageNumber::new(2, page).unwrap()
}

fn resource(kind: TeletextResourceKind, page: u8) -> TeletextResourceIdentity {
    TeletextResourceIdentity::new(
        kind,
        raw(page),
        TeletextResourceSubpageIndex::new(0).unwrap(),
    )
    .unwrap()
}

fn program(commands: Vec<TeletextEnhancementCommand>) -> TeletextEnhancementProgram {
    TeletextEnhancementProgram::new(commands).unwrap()
}

fn definition(
    identity: TeletextObjectIdentity,
    object_type: TeletextObjectType,
    commands: Vec<TeletextEnhancementCommand>,
) -> TeletextEnhancementObjectDefinition {
    TeletextEnhancementObjectDefinition {
        identity,
        object_type,
        program: program(commands),
    }
}

fn invoke(
    target: TeletextObjectIdentity,
    object_type: TeletextObjectType,
) -> TeletextEnhancementCommand {
    TeletextEnhancementCommand::InvokeObject {
        target,
        object_type,
        offset: None,
    }
}

fn glyph_reference(slot: u8) -> TeletextDrcsGlyphReference {
    TeletextDrcsGlyphReference {
        table: TeletextDrcsTableScope::Normal,
        subtable: TeletextDrcsSubtableIndex::new(0).unwrap(),
        slot: TeletextDrcsGlyphSlot::new(slot).unwrap(),
    }
}

fn drcs(slot: u8) -> TeletextEnhancementCommand {
    TeletextEnhancementCommand::AtColumn {
        column: 0,
        operation: TeletextColumnOperation::Drcs {
            reference: glyph_reference(slot),
        },
    }
}

fn charset(code: u8) -> TeletextEnhancementCommand {
    TeletextEnhancementCommand::AtColumn {
        column: 0,
        operation: TeletextColumnOperation::DesignateCharset {
            designation: TeletextEnhancementCharsetDesignation::new(code).unwrap(),
        },
    }
}

fn catalog(
    definitions: Vec<TeletextEnhancementObjectDefinition>,
) -> TeletextEnhancementObjectCatalog {
    TeletextEnhancementObjectCatalog::new(
        definitions,
        TeletextObjectCatalogBudget {
            max_objects: 512,
            max_commands: 4096,
        },
    )
    .unwrap()
}

fn budget(commands: usize, references: usize) -> TeletextEnhancementCollectionBudget {
    TeletextEnhancementCollectionBudget {
        max_commands: commands,
        max_drcs_references: references,
    }
}

fn source(object: Option<TeletextObjectIdentity>, command: usize) -> TeletextEnhancementSource {
    TeletextEnhancementSource { object, command }
}

fn drcs_catalog(slots: impl IntoIterator<Item = u8>) -> TeletextDrcsCatalog {
    TeletextDrcsCatalog::new(vec![TeletextDrcsSubtable::new(
        resource(TeletextResourceKind::Drcs, 0xa0),
        slots
            .into_iter()
            .map(|slot| TeletextDrcsGlyphDefinition {
                slot: TeletextDrcsGlyphSlot::new(slot).unwrap(),
                bitmap: TeletextDrcsBitmap::new([0x801; 10]).unwrap(),
            })
            .collect(),
    )
    .unwrap()])
    .unwrap()
}

fn links() -> TeletextDrcsPageLinks {
    TeletextDrcsPageLinks::new(Some(raw(0xa0)), None).unwrap()
}

#[test]
fn local_active_adaptive_passive_closure_collects_ordered_transitive_references() {
    let page = display(200, 0x1234);
    let active = local(page, 7);
    let adaptive = local(page, 2);
    let passive = local(page, 9);
    let definitions = vec![
        definition(passive, TeletextObjectType::Passive, vec![drcs(1)]),
        definition(
            active,
            TeletextObjectType::Active,
            vec![drcs(0), invoke(adaptive, TeletextObjectType::Adaptive)],
        ),
        definition(
            adaptive,
            TeletextObjectType::Adaptive,
            vec![charset(0x37), invoke(passive, TeletextObjectType::Passive)],
        ),
    ];
    let catalog = catalog(definitions.clone());
    assert_eq!(catalog.definitions(), definitions);
    let collected = catalog
        .collect_display_references(
            page,
            &program(vec![invoke(active, TeletextObjectType::Active)]),
            TeletextCharsetPair::LatinLatin,
            budget(6, 2),
        )
        .unwrap();
    assert_eq!(
        collected.references(),
        [glyph_reference(0), glyph_reference(1)]
    );
    assert_eq!(
        collected.sources(),
        [source(Some(active), 0), source(Some(passive), 0)]
    );
    assert_eq!(collected.command_count(), 6);
    for kind in [
        TeletextObjectType::Active,
        TeletextObjectType::Adaptive,
        TeletextObjectType::Passive,
    ] {
        assert_eq!(collected.invocation_count(kind), 1);
    }
    assert_eq!(
        collected.charset_pairs(),
        &BTreeSet::from([
            TeletextCharsetPair::LatinLatin,
            TeletextCharsetPair::GreekGreek
        ])
    );
    collected
        .validate_drcs(&drcs_catalog([0, 1]), links())
        .unwrap();
}

#[test]
fn unused_definitions_still_reject_missing_and_mistyped_targets_at_authored_ordinal() {
    let page = display(200, 1);
    let owner = local(page, 0);
    let target = local(page, 1);
    let owner_definition = definition(
        owner,
        TeletextObjectType::Active,
        vec![charset(0), invoke(target, TeletextObjectType::Adaptive)],
    );
    let limits = TeletextObjectCatalogBudget {
        max_objects: 2,
        max_commands: 2,
    };
    assert_eq!(
        TeletextEnhancementObjectCatalog::new(vec![owner_definition.clone()], limits),
        Err(TeletextObjectCatalogError::Invocation {
            owner,
            command: 1,
            reason: TeletextObjectInvocationError::Missing { target }
        })
    );
    assert_eq!(
        TeletextEnhancementObjectCatalog::new(
            vec![
                owner_definition,
                definition(target, TeletextObjectType::Passive, vec![])
            ],
            limits
        ),
        Err(TeletextObjectCatalogError::Invocation {
            owner,
            command: 1,
            reason: TeletextObjectInvocationError::Type {
                target,
                expected: TeletextObjectType::Adaptive,
                actual: TeletextObjectType::Passive
            }
        })
    );
}

#[test]
fn closure_rejects_every_non_descending_type_edge_including_self_invocation() {
    let page = display(200, 1);
    let owner = local(page, 0);
    let target = local(page, 1);
    let kinds = [
        TeletextObjectType::Active,
        TeletextObjectType::Adaptive,
        TeletextObjectType::Passive,
    ];
    for caller in kinds {
        for target_type in kinds {
            if matches!(
                (caller, target_type),
                (
                    TeletextObjectType::Active,
                    TeletextObjectType::Adaptive | TeletextObjectType::Passive
                ) | (TeletextObjectType::Adaptive, TeletextObjectType::Passive)
            ) {
                continue;
            }
            assert_eq!(
                TeletextEnhancementObjectCatalog::new(
                    vec![
                        definition(owner, caller, vec![invoke(target, target_type)]),
                        definition(target, target_type, vec![]),
                    ],
                    TeletextObjectCatalogBudget {
                        max_objects: 2,
                        max_commands: 1
                    }
                ),
                Err(TeletextObjectCatalogError::Invocation {
                    owner,
                    command: 0,
                    reason: TeletextObjectInvocationError::TypeEdge {
                        caller,
                        target,
                        target_type
                    }
                })
            );
        }
    }
    assert!(matches!(
        TeletextEnhancementObjectCatalog::new(
            vec![definition(
                owner,
                TeletextObjectType::Active,
                vec![invoke(owner, TeletextObjectType::Active)]
            )],
            TeletextObjectCatalogBudget {
                max_objects: 1,
                max_commands: 1
            }
        ),
        Err(TeletextObjectCatalogError::Invocation {
            reason: TeletextObjectInvocationError::TypeEdge { .. },
            ..
        })
    ));
}

#[test]
fn unused_resource_definitions_cannot_invoke_local_objects() {
    let page = display(200, 1);
    let target = local(page, 0);
    for caller in [
        TeletextObjectIdentity::public(
            resource(TeletextResourceKind::Pop, 0xb0),
            TeletextObjectId::new(1),
        )
        .unwrap(),
        TeletextObjectIdentity::global(
            resource(TeletextResourceKind::Gpop, 0xb1),
            TeletextObjectId::new(1),
        )
        .unwrap(),
    ] {
        assert_eq!(
            TeletextEnhancementObjectCatalog::new(
                vec![
                    definition(
                        caller,
                        TeletextObjectType::Active,
                        vec![invoke(target, TeletextObjectType::Passive)]
                    ),
                    definition(target, TeletextObjectType::Passive, vec![]),
                ],
                TeletextObjectCatalogBudget {
                    max_objects: 2,
                    max_commands: 1
                }
            ),
            Err(TeletextObjectCatalogError::Invocation {
                owner: caller,
                command: 0,
                reason: TeletextObjectInvocationError::ResourceToLocal { caller, target }
            })
        );
    }
}

#[test]
fn local_owner_checks_exact_page_and_raw_subcode_in_unused_edges_and_root_invocations() {
    let page = display(200, 0x0101);
    let owner = local(page, 0);
    for other_page in [display(201, 0x0101), display(200, 0x0001)] {
        let target = local(other_page, 1);
        assert_eq!(
            TeletextEnhancementObjectCatalog::new(
                vec![
                    definition(
                        owner,
                        TeletextObjectType::Active,
                        vec![invoke(target, TeletextObjectType::Passive)]
                    ),
                    definition(target, TeletextObjectType::Passive, vec![]),
                ],
                TeletextObjectCatalogBudget {
                    max_objects: 2,
                    max_commands: 1
                }
            ),
            Err(TeletextObjectCatalogError::Invocation {
                owner,
                command: 0,
                reason: TeletextObjectInvocationError::LocalOwner {
                    target,
                    display: page
                }
            })
        );
        let catalog = catalog(vec![definition(
            target,
            TeletextObjectType::Passive,
            vec![],
        )]);
        assert_eq!(
            catalog.collect_display_references(
                page,
                &program(vec![invoke(target, TeletextObjectType::Passive)]),
                TeletextCharsetPair::LatinLatin,
                budget(1, 0)
            ),
            Err(TeletextEnhancementCollectionError {
                display: page,
                source_location: source(None, 0),
                reason: TeletextEnhancementCollectionFailure::Invocation(
                    TeletextObjectInvocationError::LocalOwner {
                        target,
                        display: page
                    }
                )
            })
        );
    }
}

#[test]
fn root_missing_and_wrong_type_invocations_report_root_source() {
    let page = display(200, 1);
    let target = local(page, 0);
    let root = program(vec![
        charset(0),
        invoke(target, TeletextObjectType::Adaptive),
    ]);
    for (catalog, reason) in [
        (
            catalog(vec![]),
            TeletextObjectInvocationError::Missing { target },
        ),
        (
            catalog(vec![definition(
                target,
                TeletextObjectType::Passive,
                vec![],
            )]),
            TeletextObjectInvocationError::Type {
                target,
                expected: TeletextObjectType::Adaptive,
                actual: TeletextObjectType::Passive,
            },
        ),
    ] {
        assert_eq!(
            catalog.collect_display_references(
                page,
                &root,
                TeletextCharsetPair::LatinLatin,
                budget(2, 0)
            ),
            Err(TeletextEnhancementCollectionError {
                display: page,
                source_location: source(None, 1),
                reason: TeletextEnhancementCollectionFailure::Invocation(reason)
            })
        );
    }
}

#[test]
fn catalog_enforces_caller_object_and_aggregate_command_budgets_before_duplicates() {
    let identity = local(display(200, 1), 0);
    let first = definition(identity, TeletextObjectType::Passive, vec![drcs(0)]);
    let duplicate = vec![first.clone(), first.clone()];
    assert_eq!(
        TeletextEnhancementObjectCatalog::new(
            duplicate.clone(),
            TeletextObjectCatalogBudget {
                max_objects: 1,
                max_commands: 2
            }
        ),
        Err(TeletextObjectCatalogError::ObjectBudget {
            count: 2,
            maximum: 1
        })
    );
    assert_eq!(
        TeletextEnhancementObjectCatalog::new(
            duplicate.clone(),
            TeletextObjectCatalogBudget {
                max_objects: 2,
                max_commands: 1
            }
        ),
        Err(TeletextObjectCatalogError::CommandBudget { maximum: 1 })
    );
    assert_eq!(
        TeletextEnhancementObjectCatalog::new(
            duplicate,
            TeletextObjectCatalogBudget {
                max_objects: 2,
                max_commands: 2
            }
        ),
        Err(TeletextObjectCatalogError::Duplicate { identity })
    );
    let other = definition(
        local(display(200, 1), 1),
        TeletextObjectType::Passive,
        vec![drcs(1)],
    );
    assert!(TeletextEnhancementObjectCatalog::new(
        vec![first, other],
        TeletextObjectCatalogBudget {
            max_objects: 2,
            max_commands: 2
        }
    )
    .is_ok());
    assert!(TeletextEnhancementObjectCatalog::new(
        vec![],
        TeletextObjectCatalogBudget {
            max_objects: 0,
            max_commands: 0
        }
    )
    .is_ok());
}

#[test]
fn collection_state_is_fresh_for_each_display_and_after_rejection() {
    let target = TeletextObjectIdentity::public(
        resource(TeletextResourceKind::Pop, 0xb0),
        TeletextObjectId::new(0),
    )
    .unwrap();
    let catalog = catalog(vec![definition(
        target,
        TeletextObjectType::Passive,
        vec![charset(0x37), drcs(0)],
    )]);
    let root = program(vec![invoke(target, TeletextObjectType::Passive)]);
    for page in [display(200, 1), display(201, 0x0101)] {
        assert!(catalog
            .collect_display_references(page, &root, TeletextCharsetPair::LatinLatin, budget(2, 1))
            .is_err());
        let result = catalog
            .collect_display_references(page, &root, TeletextCharsetPair::LatinLatin, budget(3, 1))
            .unwrap();
        assert_eq!(result.command_count(), 3);
        assert_eq!(result.invocation_count(TeletextObjectType::Passive), 1);
        assert_eq!(result.references(), [glyph_reference(0)]);
        result.validate_drcs(&drcs_catalog([0]), links()).unwrap();
    }
}

#[test]
fn each_object_type_accepts_127_repeated_invocations_and_rejects_128() {
    let page = display(200, 1);
    let target = local(page, 0);
    for object_type in [
        TeletextObjectType::Active,
        TeletextObjectType::Adaptive,
        TeletextObjectType::Passive,
    ] {
        let catalog = catalog(vec![definition(target, object_type, vec![])]);
        let accepted = catalog
            .collect_display_references(
                page,
                &program(vec![invoke(target, object_type); 127]),
                TeletextCharsetPair::LatinLatin,
                budget(128, 0),
            )
            .unwrap();
        assert_eq!(accepted.invocation_count(object_type), 127);
        assert_eq!(accepted.command_count(), 127);
        assert_eq!(
            catalog.collect_display_references(
                page,
                &program(vec![invoke(target, object_type); 128]),
                TeletextCharsetPair::LatinLatin,
                budget(128, 0)
            ),
            Err(TeletextEnhancementCollectionError {
                display: page,
                source_location: source(None, 127),
                reason: TeletextEnhancementCollectionFailure::InvocationCount { object_type }
            })
        );
    }
}

#[test]
fn nested_repeated_invocations_consume_type_counters_independently() {
    let page = display(200, 1);
    let active = local(page, 0);
    let adaptive = local(page, 1);
    let passive = local(page, 2);
    let chain = catalog(vec![
        definition(
            active,
            TeletextObjectType::Active,
            vec![invoke(adaptive, TeletextObjectType::Adaptive)],
        ),
        definition(
            adaptive,
            TeletextObjectType::Adaptive,
            vec![invoke(passive, TeletextObjectType::Passive)],
        ),
        definition(passive, TeletextObjectType::Passive, vec![]),
    ]);
    let accepted = chain
        .collect_display_references(
            page,
            &program(vec![invoke(active, TeletextObjectType::Active); 127]),
            TeletextCharsetPair::LatinLatin,
            budget(381, 0),
        )
        .unwrap();
    assert_eq!(accepted.command_count(), 381);
    for kind in [
        TeletextObjectType::Active,
        TeletextObjectType::Adaptive,
        TeletextObjectType::Passive,
    ] {
        assert_eq!(accepted.invocation_count(kind), 127);
    }
    let doubled = catalog(vec![
        definition(
            active,
            TeletextObjectType::Active,
            vec![invoke(adaptive, TeletextObjectType::Adaptive); 2],
        ),
        definition(adaptive, TeletextObjectType::Adaptive, vec![]),
    ]);
    let accepted = doubled
        .collect_display_references(
            page,
            &program(vec![invoke(active, TeletextObjectType::Active); 63]),
            TeletextCharsetPair::LatinLatin,
            budget(192, 0),
        )
        .unwrap();
    assert_eq!(accepted.invocation_count(TeletextObjectType::Active), 63);
    assert_eq!(accepted.invocation_count(TeletextObjectType::Adaptive), 126);
    assert_eq!(
        doubled.collect_display_references(
            page,
            &program(vec![invoke(active, TeletextObjectType::Active); 64]),
            TeletextCharsetPair::LatinLatin,
            budget(192, 0)
        ),
        Err(TeletextEnhancementCollectionError {
            display: page,
            source_location: source(Some(active), 1),
            reason: TeletextEnhancementCollectionFailure::InvocationCount {
                object_type: TeletextObjectType::Adaptive
            }
        })
    );
}

#[test]
fn traversal_and_reference_budgets_count_repeated_work_at_exact_boundaries() {
    let page = display(200, 1);
    let passive = local(page, 0);
    let catalog = catalog(vec![definition(
        passive,
        TeletextObjectType::Passive,
        vec![drcs(0), drcs(0)],
    )]);
    let root = program(vec![invoke(passive, TeletextObjectType::Passive); 127]);
    let result = catalog
        .collect_display_references(
            page,
            &root,
            TeletextCharsetPair::LatinLatin,
            budget(381, 254),
        )
        .unwrap();
    assert_eq!(result.command_count(), 381);
    assert_eq!(result.references().len(), 254);
    // Repeated references consume caller output/work budgets, but only one distinct DRCS glyph.
    result.validate_drcs(&drcs_catalog([0]), links()).unwrap();
    for (limits, reason) in [
        (
            budget(380, 254),
            TeletextEnhancementCollectionFailure::CommandBudget { maximum: 380 },
        ),
        (
            budget(381, 253),
            TeletextEnhancementCollectionFailure::ReferenceBudget { maximum: 253 },
        ),
    ] {
        assert_eq!(
            catalog.collect_display_references(
                page,
                &root,
                TeletextCharsetPair::LatinLatin,
                limits
            ),
            Err(TeletextEnhancementCollectionError {
                display: page,
                source_location: source(Some(passive), 1),
                reason
            })
        );
    }
    assert_eq!(
        catalog.collect_display_references(
            page,
            &root,
            TeletextCharsetPair::LatinLatin,
            budget(0, 0)
        ),
        Err(TeletextEnhancementCollectionError {
            display: page,
            source_location: source(None, 0),
            reason: TeletextEnhancementCollectionFailure::CommandBudget { maximum: 0 }
        })
    );
    assert_eq!(
        catalog.collect_display_references(
            page,
            &root,
            TeletextCharsetPair::LatinLatin,
            budget(381, 0)
        ),
        Err(TeletextEnhancementCollectionError {
            display: page,
            source_location: source(Some(passive), 0),
            reason: TeletextEnhancementCollectionFailure::ReferenceBudget { maximum: 0 }
        })
    );
    let empty = catalog
        .collect_display_references(
            page,
            &program(vec![]),
            TeletextCharsetPair::LatinLatin,
            budget(0, 0),
        )
        .unwrap();
    assert_eq!(empty.command_count(), 0);
    assert!(empty.references().is_empty());
}

#[test]
fn a_third_transitive_charset_pair_is_rejected_but_numeric_aliases_are_not_third_pairs() {
    let page = display(200, 1);
    let active = local(page, 0);
    let passive = local(page, 1);
    let root = program(vec![invoke(active, TeletextObjectType::Active)]);
    for leaf_code in [0x00, 0x07, 0x10, 0x36] {
        let catalog = catalog(vec![
            definition(
                active,
                TeletextObjectType::Active,
                vec![charset(0x37), invoke(passive, TeletextObjectType::Passive)],
            ),
            definition(
                passive,
                TeletextObjectType::Passive,
                vec![charset(leaf_code)],
            ),
        ]);
        assert_eq!(
            catalog
                .collect_display_references(
                    page,
                    &root,
                    TeletextCharsetPair::LatinLatin,
                    budget(4, 0)
                )
                .unwrap()
                .charset_pairs(),
            &BTreeSet::from([
                TeletextCharsetPair::LatinLatin,
                TeletextCharsetPair::GreekGreek
            ])
        );
    }
    let catalog = catalog(vec![
        definition(
            active,
            TeletextObjectType::Active,
            vec![charset(0x37), invoke(passive, TeletextObjectType::Passive)],
        ),
        definition(passive, TeletextObjectType::Passive, vec![charset(0x24)]),
    ]);
    assert_eq!(
        catalog.collect_display_references(
            page,
            &root,
            TeletextCharsetPair::LatinLatin,
            budget(4, 0)
        ),
        Err(TeletextEnhancementCollectionError {
            display: page,
            source_location: source(Some(passive), 0),
            reason: TeletextEnhancementCollectionFailure::CharsetPairs {
                pair: TeletextCharsetPair::Cyrillic2Cyrillic
            }
        })
    );
}

#[test]
fn transitive_drcs_distinct_glyph_limit_maps_back_to_the_object_command() {
    let page = display(200, 1);
    let passive = local(page, 0);
    let resources = drcs_catalog(0..25);
    let mut root_commands: Vec<_> = (0..12).map(drcs).collect();
    root_commands.push(invoke(passive, TeletextObjectType::Passive));
    let root = program(root_commands);
    for end in [24, 25] {
        let catalog = catalog(vec![definition(
            passive,
            TeletextObjectType::Passive,
            (12..end).map(drcs).collect(),
        )]);
        let result = catalog
            .collect_display_references(
                page,
                &root,
                TeletextCharsetPair::LatinLatin,
                budget(26, 25),
            )
            .unwrap();
        if end == 24 {
            result.validate_drcs(&resources, links()).unwrap();
        } else {
            assert_eq!(
                result.validate_drcs(&resources, links()),
                Err(TeletextCollectedDrcsError {
                    source_location: Some(source(Some(passive), 12)),
                    error: TeletextDrcsResourceError::DisplayGlyphs {
                        display: page,
                        reference: 24
                    },
                })
            );
        }
    }
}

#[test]
fn missing_drcs_glyph_maps_to_object_source_and_link_only_failure_has_no_source() {
    let page = display(200, 1);
    let passive = local(page, 0);
    let catalog = catalog(vec![definition(
        passive,
        TeletextObjectType::Passive,
        vec![charset(0), drcs(3)],
    )]);
    let result = catalog
        .collect_display_references(
            page,
            &program(vec![invoke(passive, TeletextObjectType::Passive)]),
            TeletextCharsetPair::LatinLatin,
            budget(3, 1),
        )
        .unwrap();
    assert_eq!(
        result.validate_drcs(&drcs_catalog([0]), links()),
        Err(TeletextCollectedDrcsError {
            source_location: Some(source(Some(passive), 1)),
            error: TeletextDrcsResourceError::MissingGlyph {
                display: page,
                reference: 0,
                identity: resource(TeletextResourceKind::Drcs, 0xa0),
                slot: TeletextDrcsGlyphSlot::new(3).unwrap()
            },
        })
    );
    assert_eq!(
        result.validate_drcs(&TeletextDrcsCatalog::new(vec![]).unwrap(), links()),
        Err(TeletextCollectedDrcsError {
            source_location: None,
            error: TeletextDrcsResourceError::MissingPage {
                display: page,
                table: TeletextDrcsTableScope::Normal,
                page: raw(0xa0)
            },
        })
    );
}

#[test]
fn nested_definition_json_cannot_bypass_program_values_or_object_identity_checks() {
    let page = display(200, 0x0101);
    let identity = local(page, 0);
    let target = TeletextObjectIdentity::public(
        resource(TeletextResourceKind::Pop, 0xb0),
        TeletextObjectId::new(1),
    )
    .unwrap();
    let value = definition(
        identity,
        TeletextObjectType::Active,
        vec![
            charset(0x07),
            drcs(0),
            invoke(target, TeletextObjectType::Passive),
        ],
    );
    let fixture = serde_json::to_value(&value).unwrap();
    assert_eq!(
        serde_json::from_value::<TeletextEnhancementObjectDefinition>(fixture.clone()).unwrap(),
        value
    );
    for (pointer, invalid) in [
        ("/identity/owner/page", json!(99)),
        ("/identity/owner/subcode", json!(0x0080)),
        ("/program/commands/0/column", json!(40)),
        ("/program/commands/0/operation/designation", json!(0x27)),
        ("/program/commands/1/operation/reference/slot", json!(48)),
        (
            "/program/commands/1/operation/reference/subtable",
            json!(16),
        ),
        ("/program/commands/2/target/resource/kind", json!("drcs")),
        (
            "/program/commands/2/target/resource/logical_subpage",
            json!(16),
        ),
        ("/program/commands/2/target/resource/page/page", json!(255)),
        (
            "/program/commands/2/offset",
            json!({"row": 24, "column": 0}),
        ),
        ("/program", json!([[]])),
        ("/object_type", json!("unknown")),
    ] {
        let mut changed = fixture.clone();
        *changed.pointer_mut(pointer).unwrap() = invalid;
        assert!(
            serde_json::from_value::<TeletextEnhancementObjectDefinition>(changed).is_err(),
            "{pointer}"
        );
    }
    let mut changed = fixture;
    changed["max_commands"] = json!(usize::MAX);
    assert!(serde_json::from_value::<TeletextEnhancementObjectDefinition>(changed).is_err());
}

#[test]
fn collection_usage_repeated_calls_exact_and_one_short_keep_original_model_and_blame() {
    let owner = display(200, 0x1234);
    let target = local(owner, 42);
    let unused = local(owner, 43);
    let objects = catalog(vec![
        definition(target, TeletextObjectType::Active, vec![drcs(1), drcs(2)]),
        definition(unused, TeletextObjectType::Passive, vec![drcs(3)]),
    ]);
    let main = program(vec![
        invoke(target, TeletextObjectType::Active),
        invoke(target, TeletextObjectType::Active),
    ]);
    let saved = (objects.clone(), main.clone());
    let ordinary = objects
        .collect_display_references(owner, &main, TeletextCharsetPair::LatinLatin, budget(6, 4))
        .unwrap();
    let (result, usage) = objects.collect_display_references_with_usage(
        owner,
        &main,
        TeletextCharsetPair::LatinLatin,
        budget(6, 4),
    );
    assert_eq!(result.unwrap(), ordinary);
    assert_eq!(
        usage,
        TeletextEnhancementCollectionUsage {
            commands: 6,
            drcs_references: 4
        }
    );
    for (limits, expected_usage, expected_reason) in [
        (
            budget(5, 4),
            TeletextEnhancementCollectionUsage {
                commands: 5,
                drcs_references: 3,
            },
            TeletextEnhancementCollectionFailure::CommandBudget { maximum: 5 },
        ),
        (
            budget(6, 3),
            TeletextEnhancementCollectionUsage {
                commands: 6,
                drcs_references: 3,
            },
            TeletextEnhancementCollectionFailure::ReferenceBudget { maximum: 3 },
        ),
    ] {
        let ordinary = objects
            .collect_display_references(owner, &main, TeletextCharsetPair::LatinLatin, limits)
            .unwrap_err();
        let (result, usage) = objects.collect_display_references_with_usage(
            owner,
            &main,
            TeletextCharsetPair::LatinLatin,
            limits,
        );
        let error = result.unwrap_err();
        assert_eq!(error, ordinary);
        assert_eq!(error.source_location, source(Some(target), 1));
        assert_eq!(error.reason, expected_reason);
        assert_eq!(usage, expected_usage);
    }
    assert_eq!((objects, main), saved);
}

#[test]
fn collection_usage_semantic_failure_admits_command_but_denied_command_does_not() {
    let owner = display(200, 0);
    let target = local(owner, 42);
    let objects = catalog(vec![]);
    let main = program(vec![invoke(target, TeletextObjectType::Active)]);
    let (result, usage) = objects.collect_display_references_with_usage(
        owner,
        &main,
        TeletextCharsetPair::LatinLatin,
        budget(1, 0),
    );
    let error = result.unwrap_err();
    assert_eq!(error.source_location, source(None, 0));
    assert_eq!(
        error.reason,
        TeletextEnhancementCollectionFailure::Invocation(TeletextObjectInvocationError::Missing {
            target
        })
    );
    assert_eq!(
        usage,
        TeletextEnhancementCollectionUsage {
            commands: 1,
            drcs_references: 0
        }
    );
    let (result, usage) = objects.collect_display_references_with_usage(
        owner,
        &main,
        TeletextCharsetPair::LatinLatin,
        budget(0, 0),
    );
    assert!(matches!(
        result.unwrap_err().reason,
        TeletextEnhancementCollectionFailure::CommandBudget { maximum: 0 }
    ));
    assert_eq!(usage, TeletextEnhancementCollectionUsage::default());
    let empty = program(vec![]);
    let (result, usage) = objects.collect_display_references_with_usage(
        owner,
        &empty,
        TeletextCharsetPair::LatinLatin,
        budget(0, 0),
    );
    assert_eq!(result.unwrap().command_count(), 0);
    assert_eq!(usage, TeletextEnhancementCollectionUsage::default());
}
