//! New bank admission retains the complete original MOT/default correspondence.
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
fn additions() -> Vec<TeletextObjectResourceAddition> {
    let public = identity(TeletextResourceKind::Pop, 0xa1, 15);
    let global = identity(TeletextResourceKind::Gpop, 0xa2, 0);
    let passive = TeletextObjectIdentity::public(public, TeletextObjectId::new(65534)).unwrap();
    let global_id = TeletextObjectIdentity::global(global, TeletextObjectId::new(65533)).unwrap();
    vec![
        TeletextObjectResourceAddition::new(
            global,
            vec![TeletextEnhancementObjectDefinition {
                identity: global_id,
                object_type: TeletextObjectType::Active,
                program: TeletextEnhancementProgram::new(vec![
                    TeletextEnhancementCommand::InvokeObject {
                        target: passive,
                        object_type: TeletextObjectType::Passive,
                        offset: None,
                    },
                ])
                .unwrap(),
            }],
            limits(),
        )
        .unwrap(),
        TeletextObjectResourceAddition::new(
            public,
            vec![TeletextEnhancementObjectDefinition {
                identity: passive,
                object_type: TeletextObjectType::Passive,
                program: TeletextEnhancementProgram::new(vec![]).unwrap(),
            }],
            limits(),
        )
        .unwrap(),
        TeletextObjectResourceAddition::new(
            identity(TeletextResourceKind::Pop, 0xa1, 0),
            vec![],
            limits(),
        )
        .unwrap(),
    ]
}

#[test]
fn object_resource_additions_rebuild_inventory_without_rebinding_mot_defaults_or_fallback() {
    let original = bind(draft(), declaration(0, 1), vec![]).unwrap();
    let before = original.clone();
    let additions = additions();
    let changed = original
        .clone()
        .with_object_resource_additions(&additions, limits(), budget())
        .unwrap();
    assert_eq!(original, before);
    let mut expected_inventory = original.resource_inventory().to_vec();
    expected_inventory.extend(
        additions
            .iter()
            .map(TeletextObjectResourceAddition::resource),
    );
    expected_inventory.sort_by_key(|resource| (resource.page(), resource.logical_subpage()));
    assert_eq!(changed.resource_inventory(), expected_inventory);
    assert_eq!(changed.mots(), original.mots());
    assert_eq!(changed.selections(), original.selections());
    assert_eq!(changed.resolutions(), original.resolutions());
    assert_eq!(changed.defaults(), original.defaults());
    assert!(!original.defaults().is_empty());
    assert_eq!(
        changed.semantic().effective_presentation(owner(0)),
        original.semantic().effective_presentation(owner(0))
    );
    assert_eq!(
        changed.semantic().effective_program(owner(0)),
        original.semantic().effective_program(owner(0))
    );
    assert_eq!(
        changed.semantic().collected(owner(0)),
        original.semantic().collected(owner(0))
    );
    assert_eq!(changed.empty_object_pages(), [additions[2].resource()]);
    assert_eq!(
        changed.semantic().object_companions(),
        changed.empty_object_pages()
    );
    assert_eq!(changed.semantic().base(), original.semantic().base());
    // Re-applying the same creation cannot overwrite its new owner.
    assert!(matches!(
        changed.with_object_resource_additions(&additions, limits(), budget()),
        Err(TeletextObjectResourceAdditionApplyError::Occupied { .. })
    ));
}

#[test]
fn object_resource_additions_preflight_complete_declaration_limits_and_preserve_reserved_purposes()
{
    let original = bind(draft(), declaration(0, 1), vec![]).unwrap();
    let additions = additions();
    let exact = TeletextDeclaredServiceBudget {
        max_resources: 5,
        max_objects: 3,
        ..budget()
    };
    original
        .clone()
        .with_object_resource_additions(&additions, limits(), exact)
        .unwrap();
    for (category, count, maximum, lowered) in [
        (
            "resource",
            5,
            4,
            TeletextDeclaredServiceBudget {
                max_resources: 4,
                ..exact
            },
        ),
        (
            "object",
            3,
            2,
            TeletextDeclaredServiceBudget {
                max_objects: 2,
                ..exact
            },
        ),
    ] {
        let error = original
            .clone()
            .with_object_resource_additions(&additions, limits(), lowered)
            .unwrap_err();
        assert_eq!(
            error,
            TeletextObjectResourceAdditionApplyError::Declaration(TeletextDeclaredServiceError {
                display: None,
                mot: None,
                resource: None,
                reason: TeletextDeclaredServiceFailure::Budget {
                    category,
                    count,
                    maximum
                }
            })
        );
    }
    for lowered in [
        TeletextDeclaredServiceBudget {
            max_mots: 0,
            ..exact
        },
        TeletextDeclaredServiceBudget {
            max_displays: 0,
            ..exact
        },
        TeletextDeclaredServiceBudget {
            max_defaults: 0,
            ..exact
        },
    ] {
        assert!(matches!(
            original
                .clone()
                .with_object_resource_additions(&additions, limits(), lowered),
            Err(TeletextObjectResourceAdditionApplyError::Declaration(_))
        ));
    }
    let occupied = TeletextObjectResourceAddition::new(
        identity(TeletextResourceKind::Gpop, 0xa0, 0),
        vec![],
        limits(),
    )
    .unwrap();
    assert_eq!(
        original.with_object_resource_additions(
            std::slice::from_ref(&occupied),
            limits(),
            budget()
        ),
        Err(TeletextObjectResourceAdditionApplyError::Occupied {
            resource: occupied.resource()
        })
    );
    // A bank cannot acquire a MOT/MIP raw owner, even before service admission.
    for kind in [TeletextResourceKind::Pop, TeletextResourceKind::Gpop] {
        for byte in [0xfe, 0xfd] {
            assert!(matches!(
                TeletextResourceIdentity::new(
                    kind,
                    raw(byte),
                    TeletextResourceSubpageIndex::new(9).unwrap()
                ),
                Err(TeletextResourceIdentityError::ReservedPurpose { .. })
            ));
        }
    }
}

#[test]
fn coordinated_bank_batch_rebuilds_complete_declarations_and_combined_object_limits() {
    let original = bind(draft(), declaration(0, 1), vec![]).unwrap();
    let new_banks = additions();
    let defined = TeletextResourceObjectDefinitionAddition::new(
        TeletextEnhancementObjectDefinition {
            identity: TeletextObjectIdentity::public(
                identity(TeletextResourceKind::Pop, 0xa0, 0),
                TeletextObjectId::new(31),
            )
            .unwrap(),
            object_type: TeletextObjectType::Passive,
            program: TeletextEnhancementProgram::new(vec![]).unwrap(),
        },
        limits(),
    )
    .unwrap();
    let exact = TeletextDeclaredServiceBudget {
        max_resources: 5,
        max_objects: 4,
        ..budget()
    };
    let changed = original
        .clone()
        .with_object_resource_and_definition_additions(
            &new_banks,
            std::slice::from_ref(&defined),
            limits(),
            exact,
        )
        .unwrap();
    assert_eq!(changed.semantic().objects().definitions().len(), 4);
    assert_eq!(changed.resource_inventory().len(), 5);
    assert_eq!(changed.mots(), original.mots());
    assert_eq!(changed.selections(), original.selections());
    assert_eq!(changed.defaults(), original.defaults());
    assert_eq!(changed.resolutions(), original.resolutions());
    assert_eq!(changed.empty_object_pages(), [new_banks[2].resource()]);
    assert_eq!(changed.semantic().base(), original.semantic().base());
    for lowered in [
        TeletextDeclaredServiceBudget {
            max_objects: 3,
            ..exact
        },
        TeletextDeclaredServiceBudget {
            max_resources: 4,
            ..exact
        },
    ] {
        assert!(matches!(
            original
                .clone()
                .with_object_resource_and_definition_additions(
                    &new_banks,
                    std::slice::from_ref(&defined),
                    limits(),
                    lowered
                ),
            Err(TeletextObjectResourceAdditionApplyError::Declaration(
                TeletextDeclaredServiceError {
                    reason: TeletextDeclaredServiceFailure::Budget { .. },
                    ..
                }
            ))
        ));
    }
}
