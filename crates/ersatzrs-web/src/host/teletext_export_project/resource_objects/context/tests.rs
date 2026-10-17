use super::*;
use ersatzrs_app::teletext_tti::TeletextTtiRecord;
use ersatzrs_domain::{
    TeletextColumnOperation, TeletextEnhancementPosition, TeletextGlyphCode, TeletextObjectOffset,
    TeletextRawDataPageNumber, TeletextResourceIdentity, TeletextResourceKind,
    TeletextResourceSubpageIndex,
};

fn deadline() -> Instant {
    Instant::now() + std::time::Duration::from_secs(30)
}
fn tti() -> TeletextTtiBudget {
    TeletextTtiBudget {
        max_bytes: 65536,
        max_records: 64,
    }
}
fn identity(global: bool, subpage: u8) -> TeletextObjectIdentity {
    let resource = TeletextResourceIdentity::new(
        if global {
            TeletextResourceKind::Gpop
        } else {
            TeletextResourceKind::Pop
        },
        TeletextRawDataPageNumber::new(2, if global { 0xd0 } else { 0xc0 }).unwrap(),
        TeletextResourceSubpageIndex::new(subpage).unwrap(),
    )
    .unwrap();
    if global {
        TeletextObjectIdentity::global(resource, TeletextObjectId::new(50000))
    } else {
        TeletextObjectIdentity::public(resource, TeletextObjectId::new(50000))
    }
    .unwrap()
}
fn position(row: u8, column: u8) -> TeletextEnhancementCommand {
    TeletextEnhancementCommand::SetPosition {
        position: TeletextEnhancementPosition::new(row, column).unwrap(),
    }
}
fn glyph() -> TeletextEnhancementCommand {
    TeletextEnhancementCommand::AtColumn {
        column: 1,
        operation: TeletextColumnOperation::G0 {
            code: TeletextGlyphCode::new(b'P').unwrap(),
        },
    }
}
fn invoke(
    target: TeletextObjectIdentity,
    kind: TeletextObjectType,
    offset: Option<(u8, u8)>,
) -> TeletextEnhancementCommand {
    TeletextEnhancementCommand::InvokeObject {
        target,
        object_type: kind,
        offset: offset.map(|(row, column)| TeletextObjectOffset::new(row, column).unwrap()),
    }
}
fn definition(
    id: TeletextObjectIdentity,
    kind: TeletextObjectType,
    commands: Vec<TeletextEnhancementCommand>,
) -> TeletextEnhancementObjectDefinition {
    TeletextEnhancementObjectDefinition {
        identity: id,
        object_type: kind,
        program: TeletextEnhancementProgram::new(commands).unwrap(),
    }
}
fn limits() -> TeletextObjectCatalogBudget {
    TeletextObjectCatalogBudget {
        max_objects: 512,
        max_commands: 4096,
    }
}
fn catalog(
    definitions: Vec<TeletextEnhancementObjectDefinition>,
) -> TeletextEnhancementObjectCatalog {
    TeletextEnhancementObjectCatalog::new(definitions, limits()).unwrap()
}
fn nested() -> TeletextEnhancementObjectCatalog {
    let passive = identity(false, 0);
    let adaptive = identity(true, 15);
    let active = identity(false, 15);
    // Native authoring order deliberately differs from selected-root-first order.
    catalog(vec![
        definition(
            passive,
            TeletextObjectType::Passive,
            vec![position(4, 1), glyph()],
        ),
        definition(
            adaptive,
            TeletextObjectType::Adaptive,
            vec![
                position(1, 2),
                invoke(passive, TeletextObjectType::Passive, Some((0, 4))),
            ],
        ),
        definition(
            active,
            TeletextObjectType::Active,
            vec![
                position(1, 10),
                invoke(adaptive, TeletextObjectType::Adaptive, Some((0, 1))),
                invoke(passive, TeletextObjectType::Passive, None),
            ],
        ),
        definition(
            identity(true, 0),
            TeletextObjectType::Adaptive,
            vec![glyph()],
        ),
        definition(
            alias(owner().unwrap(), 50000).unwrap(),
            TeletextObjectType::Passive,
            vec![],
        ),
    ])
}

#[test]
fn contextual_frame_retains_full_catalog_ids_order_types_offsets_and_provenance() {
    let catalog = nested();
    let originals = catalog.clone();
    let target = identity(false, 15);
    let frame = build_catalog_context(&catalog, target, 16, tti(), deadline()).unwrap();
    assert_eq!(frame.identity, target);
    assert_eq!(frame.object_type, TeletextObjectType::Active);
    assert_eq!(
        frame.aliases,
        [
            target,
            identity(false, 0),
            identity(true, 15),
            identity(true, 0)
        ]
    );
    assert_eq!(catalog, originals);
    let projected = project_catalog(
        &catalog,
        &frame.aliases,
        owner().unwrap(),
        limits(),
        None,
        deadline(),
    )
    .unwrap();
    assert_eq!(projected.definitions().len(), 4);
    for (ordinal, original) in frame.aliases.iter().enumerate() {
        let native = catalog.definition(*original).unwrap();
        let local = &projected.definitions()[ordinal];
        assert_eq!(local.identity, alias(owner().unwrap(), ordinal).unwrap());
        assert_eq!(local.object_type, native.object_type);
        let physical = &frame.encoded.definition_positions()[ordinal];
        assert_eq!(physical.target(), local.identity);
        assert_eq!(physical.object_type(), local.object_type);
        for (command, (before, after)) in native
            .program
            .commands()
            .iter()
            .zip(local.program.commands())
            .enumerate()
        {
            match (*before, *after) {
                (
                    TeletextEnhancementCommand::InvokeObject {
                        target: before,
                        object_type: a,
                        offset: b,
                    },
                    TeletextEnhancementCommand::InvokeObject {
                        target: after,
                        object_type: c,
                        offset: d,
                    },
                ) => {
                    let target_ordinal = frame.aliases.iter().position(|id| *id == before).unwrap();
                    assert_eq!(after, alias(owner().unwrap(), target_ordinal).unwrap());
                    assert_eq!((a, b), (c, d));
                }
                _ => assert_eq!(before, after),
            }
            assert!(frame
                .encoded
                .command_positions()
                .iter()
                .any(|position| position.source().object == Some(local.identity)
                    && position.source().command == command));
        }
    }
    assert!(matches!(
        frame.document.records()[0],
        TeletextTtiRecord::PageNumber {
            page: 0x100,
            editor_ordinal: 0
        }
    ));
    let main: Vec<_> = frame
        .encoded
        .command_positions()
        .iter()
        .filter(|p| p.source().object.is_none())
        .collect();
    assert_eq!(main.len(), 1);
    assert_eq!(main[0].source().command, 0);
}

#[test]
fn contextual_selected_roots_cover_each_type_and_every_original_resource_target() {
    let catalog = nested();
    for (target, kind, count) in [
        (identity(false, 15), TeletextObjectType::Active, 4),
        (identity(true, 15), TeletextObjectType::Adaptive, 4),
        (identity(false, 0), TeletextObjectType::Passive, 4),
    ] {
        let frame = build_catalog_context(&catalog, target, 16, tti(), deadline()).unwrap();
        assert_eq!(frame.object_type, kind);
        assert_eq!(frame.aliases[0], target);
        assert_eq!(frame.aliases.len(), count);
        assert_eq!(frame.encoded.definition_positions().len(), count);
    }
}

#[test]
fn contextual_frame_accepts_exact_208_words_and_rejects_overflow_without_truncation() {
    let target = identity(false, 15);
    let build = |count| {
        catalog(vec![definition(
            target,
            TeletextObjectType::Passive,
            vec![glyph(); count],
        )])
    };
    // One root invocation, main terminator, definition marker and final terminator.
    let exact = build(204);
    let frame = build_catalog_context(&exact, target, 16, tti(), deadline()).unwrap();
    assert_eq!(frame.encoded.packet_bodies().len(), 16);
    assert_eq!(frame.encoded.command_positions().len(), 205);
    assert_eq!(frame.encoded.definition_positions()[0].absolute_index(), 2);
    assert_eq!(frame.document.records().len(), 22);
    let large = build(205);
    assert!(matches!(
        build_catalog_context(&large, target, 16, tti(), deadline()),
        Err(Error::Budget)
    ));
    assert_eq!(
        large.definition(target).unwrap().program.commands().len(),
        205
    );
    assert!(matches!(
        build_catalog_context(&exact, target, 15, tti(), deadline()),
        Err(Error::Budget)
    ));
    let mut small = tti();
    small.max_records = 21;
    assert!(matches!(
        build_catalog_context(&exact, target, 16, small, deadline()),
        Err(Error::Budget)
    ));
}

#[test]
fn contextual_missing_owner_deadline_and_zero_limits_fail_closed() {
    let catalog = nested();
    assert!(matches!(
        build_catalog_context(&catalog, identity(true, 1), 16, tti(), deadline()),
        Err(Error::Association)
    ));
    assert!(matches!(
        build_catalog_context(&catalog, identity(false, 15), 0, tti(), deadline()),
        Err(Error::Budget)
    ));
    assert!(matches!(
        build_catalog_context(&catalog, identity(false, 15), 16, tti(), Instant::now()),
        Err(Error::TimedOut)
    ));
    let mut small = tti();
    small.max_records = 6;
    assert!(matches!(
        build_catalog_context(&catalog, identity(false, 15), 16, small, deadline()),
        Err(Error::Budget)
    ));
}

#[test]
fn contextual_typed_catalog_boundary_refuses_duplicates_missing_types_and_cycles() {
    let a = identity(false, 15);
    let b = identity(true, 15);
    let empty = definition(a, TeletextObjectType::Active, vec![]);
    assert!(TeletextEnhancementObjectCatalog::new(vec![empty.clone(), empty], limits()).is_err());
    for definitions in [
        vec![definition(
            a,
            TeletextObjectType::Active,
            vec![invoke(b, TeletextObjectType::Passive, None)],
        )],
        vec![
            definition(
                a,
                TeletextObjectType::Active,
                vec![invoke(b, TeletextObjectType::Adaptive, None)],
            ),
            definition(b, TeletextObjectType::Passive, vec![]),
        ],
        vec![
            definition(
                a,
                TeletextObjectType::Active,
                vec![invoke(b, TeletextObjectType::Adaptive, None)],
            ),
            definition(
                b,
                TeletextObjectType::Adaptive,
                vec![invoke(a, TeletextObjectType::Active, None)],
            ),
        ],
    ] {
        assert!(TeletextEnhancementObjectCatalog::new(definitions, limits()).is_err());
    }
}

#[test]
fn contextual_root_replacement_preserves_frozen_aliases_and_readonly_callees() {
    let catalog = nested();
    let target = identity(false, 15);
    let baseline = build_catalog_context(&catalog, target, 16, tti(), deadline()).unwrap();
    let replacement = TeletextEnhancementProgram::new(vec![invoke(
        identity(false, 0),
        TeletextObjectType::Passive,
        Some((0, 1)),
    )])
    .unwrap();
    let changed = build_catalog_context_with_root(
        &catalog,
        target,
        Some(&replacement),
        16,
        tti(),
        deadline(),
    )
    .unwrap();
    assert_eq!(changed.aliases, baseline.aliases);
    assert_eq!(changed.encoded.definition_positions().len(), 4);
    let before = project_catalog(
        &catalog,
        &baseline.aliases,
        owner().unwrap(),
        limits(),
        None,
        deadline(),
    )
    .unwrap();
    let after = project_catalog(
        &catalog,
        &changed.aliases,
        owner().unwrap(),
        limits(),
        Some(&replacement),
        deadline(),
    )
    .unwrap();
    assert_eq!(&before.definitions()[1..], &after.definitions()[1..]);
    assert_eq!(
        after.definitions()[0].program.commands(),
        [invoke(
            alias(owner().unwrap(), 1).unwrap(),
            TeletextObjectType::Passive,
            Some((0, 1))
        ),]
    );
    assert_ne!(
        changed.encoded.definition_positions()[1].absolute_index(),
        baseline.encoded.definition_positions()[1].absolute_index()
    );
    let retargeted = TeletextEnhancementProgram::new(vec![invoke(
        identity(true, 0),
        TeletextObjectType::Adaptive,
        Some((0, 1)),
    )])
    .unwrap();
    let retargeted_frame =
        build_catalog_context_with_root(&catalog, target, Some(&retargeted), 16, tti(), deadline())
            .unwrap();
    assert_eq!(retargeted_frame.aliases, baseline.aliases);
    let projected = project_catalog(
        &catalog,
        &baseline.aliases,
        owner().unwrap(),
        limits(),
        Some(&retargeted),
        deadline(),
    )
    .unwrap();
    assert_eq!(
        projected.definitions()[0].program.commands(),
        [invoke(
            alias(owner().unwrap(), 3).unwrap(),
            TeletextObjectType::Adaptive,
            Some((0, 1))
        ),]
    );
    for command in [
        invoke(identity(true, 1), TeletextObjectType::Passive, None),
        invoke(identity(false, 0), TeletextObjectType::Adaptive, None),
        invoke(target, TeletextObjectType::Active, None),
    ] {
        let replacement = TeletextEnhancementProgram::new(vec![command]).unwrap();
        assert!(matches!(
            build_catalog_context_with_root(
                &catalog,
                target,
                Some(&replacement),
                16,
                tti(),
                deadline()
            ),
            Err(Error::Association)
        ));
    }
}

#[test]
fn contextual_physical_capacity_does_not_cap_invisible_authored_commands() {
    let target = identity(false, 15);
    let mut commands = vec![position(0, 1); 512];
    commands.push(glyph());
    let catalog = catalog(vec![definition(
        target,
        TeletextObjectType::Passive,
        commands,
    )]);
    let frame = build_catalog_context(&catalog, target, 16, tti(), deadline()).unwrap();
    assert_eq!(
        catalog.definition(target).unwrap().program.commands().len(),
        513
    );
    assert_eq!(frame.encoded.packet_bodies().len(), 1);
    assert_eq!(frame.encoded.command_positions().len(), 2);
}

#[test]
fn contextual_full_resource_inventory_never_drops_unreachable_definitions_to_fit() {
    let resource = identity(false, 15).resource().unwrap();
    let targets: Vec<_> = (0..209)
        .map(|id| TeletextObjectIdentity::public(resource, TeletextObjectId::new(id)).unwrap())
        .collect();
    let catalog = catalog(
        targets
            .iter()
            .map(|target| definition(*target, TeletextObjectType::Passive, vec![]))
            .collect(),
    );
    assert!(matches!(
        build_catalog_context(&catalog, targets[0], 16, tti(), deadline()),
        Err(Error::Budget)
    ));
    assert_eq!(catalog.definitions().len(), 209);
}
