//! New native bank sets, without host/editor/persistence/publication authority.
use super::*;

fn bank(
    global: bool,
    page: u8,
    s1: u8,
    definitions: Vec<TeletextEnhancementObjectDefinition>,
) -> TeletextObjectResourceAddition {
    TeletextObjectResourceAddition::new(
        resource(
            if global {
                TeletextResourceKind::Gpop
            } else {
                TeletextResourceKind::Pop
            },
            page,
            s1,
        ),
        definitions,
        budget(),
    )
    .unwrap()
}

fn banks() -> Vec<TeletextObjectResourceAddition> {
    let passive = identity(false, 0xa4, 2, 30);
    let adaptive = identity(true, 0xa5, 15, 31);
    vec![
        // Forward dependency: the Global bank precedes the Public bank it uses.
        bank(
            true,
            0xa5,
            15,
            vec![
                definition(
                    adaptive,
                    TeletextObjectType::Adaptive,
                    vec![invoke(passive, TeletextObjectType::Passive)],
                ),
                definition(
                    identity(true, 0xa5, 15, 32),
                    TeletextObjectType::Active,
                    vec![invoke(adaptive, TeletextObjectType::Adaptive)],
                ),
            ],
        ),
        bank(
            false,
            0xa4,
            2,
            vec![definition(
                passive,
                TeletextObjectType::Passive,
                vec![glyph(b'B')],
            )],
        ),
        bank(false, 0xa4, 15, vec![]),
        bank(true, 0xa6, 0, vec![]),
    ]
}

fn assert_constructor_preserves_native_types() {
    for global in [false, true] {
        for s1 in [0, 15] {
            for object_type in [
                TeletextObjectType::Active,
                TeletextObjectType::Adaptive,
                TeletextObjectType::Passive,
            ] {
                let definition = definition(
                    identity(global, 0xa4, s1, 65535),
                    object_type,
                    vec![glyph(b'B')],
                );
                let addition = bank(global, 0xa4, s1, vec![definition.clone()]);
                assert_eq!(addition.definitions(), [definition]);
                assert_eq!(addition.resource().logical_subpage().get(), s1);
            }
        }
    }
}

#[test]
fn object_resource_addition_constructor_preserves_types_and_exact_owner_without_aliases() {
    assert_constructor_preserves_native_types();
    let target = resource(TeletextResourceKind::Pop, 0xa4, 2);
    for foreign in [
        identity(false, 0xa4, 3, 1),
        identity(true, 0xa4, 2, 1),
        identity(false, 0xa5, 2, 1),
        TeletextObjectIdentity::local(address(200, 0), TeletextObjectId::new(1)),
    ] {
        assert_eq!(
            TeletextObjectResourceAddition::new(
                target,
                vec![definition(foreign, TeletextObjectType::Passive, vec![])],
                budget()
            ),
            Err(TeletextObjectResourceAdditionError::Owner {
                resource: target,
                identity: foreign
            })
        );
    }
    let invalid = resource(TeletextResourceKind::Drcs, 0xa4, 2);
    assert_eq!(
        TeletextObjectResourceAddition::new(invalid, vec![], budget()),
        Err(TeletextObjectResourceAdditionError::Kind { resource: invalid })
    );
    let defined = definition(
        identity(false, 0xa4, 2, 1),
        TeletextObjectType::Passive,
        vec![glyph(b'B')],
    );
    assert_eq!(
        TeletextObjectResourceAddition::new(
            target,
            vec![defined.clone(), defined.clone()],
            budget()
        ),
        Err(TeletextObjectResourceAdditionError::DuplicateDefinition {
            identity: defined.identity
        })
    );
    for (dimension, limits, definitions) in [
        (
            "record",
            TeletextServiceBudget {
                max_records: 0,
                ..budget()
            },
            vec![],
        ),
        (
            "structure item",
            TeletextServiceBudget {
                max_structure_items: 0,
                ..budget()
            },
            vec![],
        ),
        (
            "object",
            TeletextServiceBudget {
                max_objects: 0,
                ..budget()
            },
            vec![defined.clone()],
        ),
        (
            "authored command",
            TeletextServiceBudget {
                max_commands: 0,
                ..budget()
            },
            vec![defined],
        ),
    ] {
        assert_eq!(
            TeletextObjectResourceAddition::new(target, definitions, limits),
            Err(TeletextObjectResourceAdditionError::Budget {
                dimension,
                maximum: 0
            })
        );
    }
    TeletextObjectResourceAddition::new(
        target,
        vec![],
        TeletextServiceBudget {
            max_objects: 0,
            max_commands: 0,
            ..budget()
        },
    )
    .unwrap();
}

#[test]
fn object_resource_additions_are_atomic_complete_union_with_original_display_and_dependencies() {
    let original = service(input());
    let snapshot = original.clone();
    let additions = banks();
    let edited = original
        .clone()
        .with_object_resource_additions(&additions, budget())
        .unwrap();
    let mut expected = input();
    expected.objects.extend(
        additions
            .iter()
            .flat_map(|a| a.definitions().iter().cloned()),
    );
    let mut companions = empty_resources();
    companions.extend(
        additions
            .iter()
            .filter(|a| a.definitions().is_empty())
            .map(TeletextObjectResourceAddition::resource),
    );
    let expected =
        TeletextSemanticService::new_with_object_companions(expected, companions, budget())
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
    assert_eq!(edited.base(), original.base());
    for owner in [address(200, 0), address(200, 1)] {
        assert_eq!(
            edited.effective_program(owner),
            original.effective_program(owner)
        );
        assert_eq!(edited.collected(owner), original.collected(owner));
    }
    // Resource creation does not loosen ordinary definition Add's ownership check.
    let definition = addition(additions[1].definitions()[0].clone());
    assert!(matches!(
        original.with_resource_object_definition_additions(&[definition], budget()),
        Err(TeletextResourceObjectDefinitionAdditionApplyError::MissingResource { .. })
    ));
    let mut reversed = additions;
    reversed.reverse();
    snapshot
        .with_object_resource_additions(&reversed, budget())
        .unwrap();
}

#[test]
fn object_resource_additions_refuse_occupied_purposes_s1_duplicates_and_bad_graphs() {
    let original = service(input());
    assert_eq!(
        original
            .clone()
            .with_object_resource_additions(&[], budget()),
        Err(TeletextObjectResourceAdditionApplyError::Empty)
    );
    for occupied in [
        bank(false, 0xa0, 1, vec![]),
        bank(true, 0xa0, 1, vec![]),
        bank(false, 0xa2, 3, vec![]),
        bank(false, 0xb0, 0, vec![]),
    ] {
        let resource = occupied.resource();
        assert_eq!(
            original
                .clone()
                .with_object_resource_additions(&[banks().remove(0), occupied], budget()),
            Err(TeletextObjectResourceAdditionApplyError::Occupied { resource })
        );
    }
    let duplicate = bank(false, 0xa4, 2, vec![]);
    for second in [duplicate.clone(), bank(true, 0xa4, 2, vec![])] {
        assert!(matches!(
            original
                .clone()
                .with_object_resource_additions(&[duplicate.clone(), second], budget()),
            Err(TeletextObjectResourceAdditionApplyError::Occupied { .. })
        ));
    }
    // Distinct S1 on an already used raw page remains a distinct valid owner.
    original
        .clone()
        .with_object_resource_additions(&[bank(false, 0xa0, 15, vec![])], budget())
        .unwrap();
    let new = identity(false, 0xa4, 2, 30);
    for target in [identity(false, 0xa4, 2, 99), new] {
        let bad = bank(
            false,
            0xa4,
            2,
            vec![definition(
                new,
                TeletextObjectType::Active,
                vec![invoke(target, TeletextObjectType::Active)],
            )],
        );
        assert!(matches!(
            original
                .clone()
                .with_object_resource_additions(&[bad], budget()),
            Err(TeletextObjectResourceAdditionApplyError::Service(
                TeletextServiceError::Objects(TeletextObjectCatalogError::Invocation { .. })
            ))
        ));
    }
    let wrong_type = bank(
        false,
        0xa4,
        2,
        vec![definition(
            new,
            TeletextObjectType::Active,
            vec![invoke(
                identity(false, 0xa0, 1, 19),
                TeletextObjectType::Adaptive,
            )],
        )],
    );
    assert!(matches!(
        original.with_object_resource_additions(&[wrong_type], budget()),
        Err(TeletextObjectResourceAdditionApplyError::Service(
            TeletextServiceError::Objects(TeletextObjectCatalogError::Invocation {
                reason: TeletextObjectInvocationError::Type { .. },
                ..
            })
        ))
    ));
}

#[test]
fn object_resource_additions_bound_combined_native_counts_and_reduced_caller_limits() {
    let original = service(input());
    let additions = banks();
    let exact = TeletextServiceBudget {
        max_objects: 5,
        max_commands: 11,
        max_records: 11,
        max_collected_commands: 8,
        max_base_content_bytes: 40,
        ..budget()
    };
    original
        .clone()
        .with_object_resource_additions(&additions, exact)
        .unwrap();
    for limits in [
        TeletextServiceBudget {
            max_objects: 4,
            ..exact
        },
        TeletextServiceBudget {
            max_commands: 10,
            ..exact
        },
        TeletextServiceBudget {
            max_records: 10,
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
        TeletextServiceBudget {
            max_structure_items: 0,
            ..exact
        },
    ] {
        assert!(matches!(
            original
                .clone()
                .with_object_resource_additions(&additions, limits),
            Err(TeletextObjectResourceAdditionApplyError::Service(
                TeletextServiceError::Budget { .. }
            ))
        ));
    }
    original
        .with_object_resource_additions(
            &additions,
            TeletextServiceBudget {
                max_records: usize::MAX,
                max_objects: usize::MAX,
                max_commands: usize::MAX,
                max_structure_items: usize::MAX,
                ..budget()
            },
        )
        .unwrap();
}

fn service_with_spelling() -> TeletextSemanticService {
    let mut draft = input();
    draft.base.pages[0].rows.push(TeletextRow {
        row: 1,
        runs: vec![TeletextRun {
            column: 4,
            style: TeletextStyle::default(),
            content: TeletextRunContent::Text { text: "£".into() },
        }],
    });
    let spelling = TeletextBaseTextSpelling::new(
        address(200, 0),
        TeletextEnhancementPosition::new(1, 4).unwrap(),
        '£',
        TeletextGlyphCode::new(b' ').unwrap(),
        TeletextEnhancementCharsetDesignation::new(0).unwrap(),
        TeletextColumnOperation::G2 {
            code: TeletextGlyphCode::new(0x23).unwrap(),
        },
    )
    .unwrap();
    service(draft)
        .with_base_text_spellings(vec![spelling], budget())
        .unwrap()
}

#[test]
fn object_resource_additions_preserve_spellings_and_preflight_their_combined_storage() {
    let original = service_with_spelling();
    let new = identity(false, 0xa4, 2, 30);
    let addition = bank(
        false,
        0xa4,
        2,
        vec![definition(
            new,
            TeletextObjectType::Active,
            vec![glyph(b'B')],
        )],
    );
    let exact = TeletextServiceBudget {
        max_commands: 10,
        max_base_content_bytes: 48,
        ..budget()
    };
    let changed = original
        .clone()
        .with_object_resource_additions(&[addition], exact)
        .unwrap();
    assert_eq!(
        changed.base_text_spellings(),
        original.base_text_spellings()
    );
    assert_eq!(
        changed.physical_base_overlays(),
        original.physical_base_overlays()
    );
    assert_eq!(changed.base(), original.base());
    // The cycle would fail catalogue reconstruction. The aggregate spelling+
    // program limit must reject first, before that reconstruction is attempted.
    let cyclic = bank(
        false,
        0xa4,
        2,
        vec![definition(
            new,
            TeletextObjectType::Active,
            vec![invoke(new, TeletextObjectType::Active)],
        )],
    );
    assert_eq!(
        original.clone().with_object_resource_additions(
            &[cyclic],
            TeletextServiceBudget {
                max_commands: 9,
                ..exact
            }
        ),
        Err(TeletextObjectResourceAdditionApplyError::Service(
            TeletextServiceError::Budget {
                dimension: "authored command",
                maximum: 9
            }
        ))
    );
    let empty = bank(false, 0xa4, 2, vec![]);
    original
        .clone()
        .with_object_resource_additions(
            std::slice::from_ref(&empty),
            TeletextServiceBudget {
                max_structure_items: 14,
                ..budget()
            },
        )
        .unwrap();
    assert_eq!(
        original.with_object_resource_additions(
            &[empty],
            TeletextServiceBudget {
                max_structure_items: 13,
                ..budget()
            }
        ),
        Err(TeletextObjectResourceAdditionApplyError::Service(
            TeletextServiceError::Budget {
                dimension: "structure item",
                maximum: 13
            }
        ))
    );
}
