use super::*;
use ersatzrs_domain::{
    TeletextEnhancementObjectDefinition, TeletextObjectCatalogBudget, TeletextObjectId,
    TeletextObjectOffset, TeletextObjectType, TeletextResourceIdentity, TeletextResourceKind,
    TeletextResourceSubpageIndex,
};

fn budget() -> TeletextServiceBudget {
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

fn identity(kind: TeletextResourceKind, page: u8, id: u16) -> TeletextObjectIdentity {
    let resource = TeletextResourceIdentity::new(
        kind,
        TeletextRawDataPageNumber::new(2, page).unwrap(),
        TeletextResourceSubpageIndex::new(0).unwrap(),
    )
    .unwrap();
    match kind {
        TeletextResourceKind::Pop => {
            TeletextObjectIdentity::public(resource, TeletextObjectId::new(id)).unwrap()
        }
        TeletextResourceKind::Gpop => {
            TeletextObjectIdentity::global(resource, TeletextObjectId::new(id)).unwrap()
        }
        _ => unreachable!(),
    }
}

fn fixture() -> (TeletextEnhancementObjectCatalog, TeletextObjectCallChoice) {
    let caller = identity(TeletextResourceKind::Pop, 0xc0, 20);
    let old = identity(TeletextResourceKind::Gpop, 0xd0, 21);
    let target = identity(TeletextResourceKind::Gpop, 0xd1, 22);
    let mistyped = identity(TeletextResourceKind::Gpop, 0xd2, 23);
    let empty = || TeletextEnhancementProgram::new(Vec::new()).unwrap();
    let catalog = TeletextEnhancementObjectCatalog::new(
        vec![
            TeletextEnhancementObjectDefinition {
                identity: caller,
                object_type: TeletextObjectType::Active,
                program: TeletextEnhancementProgram::new(vec![
                    TeletextEnhancementCommand::SetPosition {
                        position: ersatzrs_domain::TeletextEnhancementPosition::new(2, 3).unwrap(),
                    },
                    TeletextEnhancementCommand::InvokeObject {
                        target: old,
                        object_type: TeletextObjectType::Passive,
                        offset: Some(TeletextObjectOffset::new(1, 2).unwrap()),
                    },
                    TeletextEnhancementCommand::InvokeObject {
                        target: old,
                        object_type: TeletextObjectType::Passive,
                        offset: None,
                    },
                ])
                .unwrap(),
            },
            TeletextEnhancementObjectDefinition {
                identity: old,
                object_type: TeletextObjectType::Passive,
                program: empty(),
            },
            TeletextEnhancementObjectDefinition {
                identity: target,
                object_type: TeletextObjectType::Passive,
                program: empty(),
            },
            TeletextEnhancementObjectDefinition {
                identity: mistyped,
                object_type: TeletextObjectType::Adaptive,
                program: empty(),
            },
        ],
        TeletextObjectCatalogBudget {
            max_objects: 64,
            max_commands: 1024,
        },
    )
    .unwrap();
    (
        catalog,
        TeletextObjectCallChoice {
            caller,
            command_index: 1,
            target,
        },
    )
}

fn deadline() -> Instant {
    Instant::now() + std::time::Duration::from_secs(30)
}

#[test]
fn chosen_calls_preserve_complete_original_type_offset_anchors_and_other_calls() {
    let (catalog, choice) = fixture();
    let before = catalog.definition(choice.caller).unwrap().program.clone();
    let programs = resolve_calls(&catalog, &[choice], budget(), deadline()).unwrap();
    assert_eq!(programs.len(), 1);
    let edit = &programs[0];
    assert_eq!(edit.expected(), &before);
    assert_eq!(edit.retained_commands(), &[Some(0), Some(1), Some(2)]);
    let mut expected = before.commands().to_vec();
    expected[1] = TeletextEnhancementCommand::InvokeObject {
        target: choice.target,
        object_type: TeletextObjectType::Passive,
        offset: Some(TeletextObjectOffset::new(1, 2).unwrap()),
    };
    assert_eq!(edit.replacement().commands(), expected);
    assert_eq!(catalog.definition(choice.caller).unwrap().program, before);

    let both = [
        choice,
        TeletextObjectCallChoice {
            command_index: 2,
            ..choice
        },
    ];
    let programs = resolve_calls(&catalog, &both, budget(), deadline()).unwrap();
    assert_eq!(programs.len(), 1);
    assert!(matches!(programs[0].replacement().commands()[2],
        TeletextEnhancementCommand::InvokeObject { target, object_type: TeletextObjectType::Passive, offset: None }
        if target == choice.target));
}

#[test]
fn chosen_call_refusals_and_unchanged_rows_do_not_clone_a_replacement() {
    let (catalog, choice) = fixture();
    for invalid_choice in [
        TeletextObjectCallChoice {
            command_index: 0,
            ..choice
        },
        TeletextObjectCallChoice {
            command_index: usize::MAX,
            ..choice
        },
        TeletextObjectCallChoice {
            caller: choice.target,
            ..choice
        },
        TeletextObjectCallChoice {
            target: identity(TeletextResourceKind::Gpop, 0xd2, 23),
            ..choice
        },
        TeletextObjectCallChoice {
            target: identity(TeletextResourceKind::Gpop, 0xd3, 99),
            ..choice
        },
    ] {
        assert!(matches!(
            resolve_calls(&catalog, &[invalid_choice], budget(), deadline()),
            Err(NativeTeletextEditorialImportError::Association)
        ));
    }
    let unchanged = TeletextObjectCallChoice {
        target: identity(TeletextResourceKind::Gpop, 0xd0, 21),
        ..choice
    };
    assert!(resolve_calls(&catalog, &[unchanged], budget(), deadline())
        .unwrap()
        .is_empty());
    let mut tiny = budget();
    tiny.max_commands = 5;
    assert!(matches!(
        resolve_calls(&catalog, &[choice], tiny, deadline()),
        Err(NativeTeletextEditorialImportError::Budget)
    ));
    assert!(matches!(
        resolve_calls(&catalog, &[choice], budget(), Instant::now()),
        Err(NativeTeletextEditorialImportError::TimedOut)
    ));
}

#[test]
fn raw_choice_preflight_rejects_duplicates_bounds_and_empty_actions() {
    let (_, choice) = fixture();
    assert!(matches!(
        preflight(&[], &[], budget(), deadline()),
        Err(NativeTeletextEditorialImportError::NoChanges)
    ));
    assert!(matches!(
        preflight(&[], &[choice, choice], budget(), deadline()),
        Err(NativeTeletextEditorialImportError::Association)
    ));
    let mut tiny = budget();
    tiny.max_structure_items = 0;
    assert!(matches!(
        preflight(&[], &[choice], tiny, deadline()),
        Err(NativeTeletextEditorialImportError::Budget)
    ));
    tiny = budget();
    tiny.max_objects = 0;
    assert!(matches!(
        preflight(&[], &[choice], tiny, deadline()),
        Err(NativeTeletextEditorialImportError::Budget)
    ));
    assert!(preflight(&[], &[choice], budget(), deadline()).is_ok());
    assert!(page_selections::preflight(&[], &[], budget(), deadline()).is_err());
    let page = TeletextObjectPageChoice {
        display: TeletextPageAddress {
            page: ersatzrs_domain::TeletextPageNumber::new(124).unwrap(),
            subcode: ersatzrs_domain::TeletextSubcode::new(32).unwrap(),
        },
        role: TeletextX27LinkRole::Pop,
        target: TeletextRawDataPageNumber::new(2, 0xc1).unwrap(),
        required_subpages: TeletextSubpageRequirementMask::new(0x4001),
    };
    assert!(preflight(&[page], &[], budget(), deadline()).is_ok());
    assert!(matches!(
        preflight(&[page, page], &[], budget(), deadline()),
        Err(NativeTeletextEditorialImportError::Association)
    ));
    // Raw choices bound all four admitted roles. Native source resolution and
    // closed saved versions separately authenticate selection/attachment authority.
    for role in TeletextX27LinkRole::ALL {
        let choice = TeletextObjectPageChoice { role, ..page };
        assert!(preflight(&[choice], &[], budget(), deadline()).is_ok());
        assert!(matches!(
            preflight(&[choice, choice], &[], budget(), deadline()),
            Err(NativeTeletextEditorialImportError::Association)
        ));
    }
}
