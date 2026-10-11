//! Fixed original-coordinate oracles; no glyph priority or panel acceptance.

use super::*;

fn owner() -> TeletextPageAddress {
    TeletextPageAddress {
        page: TeletextPageNumber::new(200).unwrap(),
        subcode: TeletextSubcode::new(0x1234).unwrap(),
    }
}

fn local(id: u16) -> TeletextObjectIdentity {
    TeletextObjectIdentity::local(owner(), TeletextObjectId::new(id))
}

fn public(page: u8, id: u16) -> TeletextObjectIdentity {
    TeletextObjectIdentity::public(
        TeletextResourceIdentity::new(
            TeletextResourceKind::Pop,
            TeletextRawDataPageNumber::new(2, page).unwrap(),
            TeletextResourceSubpageIndex::new(0).unwrap(),
        )
        .unwrap(),
        TeletextObjectId::new(id),
    )
    .unwrap()
}

fn global(page: u8, id: u16) -> TeletextObjectIdentity {
    TeletextObjectIdentity::global(
        TeletextResourceIdentity::new(
            TeletextResourceKind::Gpop,
            TeletextRawDataPageNumber::new(2, page).unwrap(),
            TeletextResourceSubpageIndex::new(0).unwrap(),
        )
        .unwrap(),
        TeletextObjectId::new(id),
    )
    .unwrap()
}

fn position(row: u8, column: u8) -> TeletextEnhancementCommand {
    TeletextEnhancementCommand::SetPosition {
        position: TeletextEnhancementPosition::new(row, column).unwrap(),
    }
}

fn glyph(column: u8, code: u8) -> TeletextEnhancementCommand {
    TeletextEnhancementCommand::AtColumn {
        column,
        operation: TeletextColumnOperation::G0 {
            code: TeletextGlyphCode::new(code).unwrap(),
        },
    }
}

fn invoke(
    target: TeletextObjectIdentity,
    object_type: TeletextObjectType,
    offset: Option<(u8, u8)>,
) -> TeletextEnhancementCommand {
    TeletextEnhancementCommand::InvokeObject {
        target,
        object_type,
        offset: offset.map(|(row, column)| TeletextObjectOffset::new(row, column).unwrap()),
    }
}

fn definition(
    identity: TeletextObjectIdentity,
    object_type: TeletextObjectType,
    commands: Vec<TeletextEnhancementCommand>,
) -> TeletextEnhancementObjectDefinition {
    TeletextEnhancementObjectDefinition {
        identity,
        object_type,
        program: TeletextEnhancementProgram::new(commands).unwrap(),
    }
}

fn service(
    main: Option<Vec<TeletextEnhancementCommand>>,
    objects: Vec<TeletextEnhancementObjectDefinition>,
    defaults: [Option<TeletextDefaultObjectSelection>; 4],
    selected: Option<TeletextRawDataPageNumber>,
) -> TeletextSemanticService {
    let language = TeletextLanguage::new("nld".into()).unwrap();
    TeletextSemanticService::new(
        TeletextResolvedServiceDraft {
            base: TeletextPageSetDraft {
                initial_page: owner().page,
                language: language.clone(),
                pages: vec![TeletextPageDraft {
                    address: owner(),
                    kind: TeletextPageKind::FullPage,
                    language,
                    national_option: TeletextNationalOption::new(0).unwrap(),
                    rows: vec![],
                    navigation: vec![],
                }],
            },
            pages: vec![TeletextResolvedPageEnhancementDraft {
                address: owner(),
                presentation: None,
                compositional_links: None,
                local_program: main
                    .map(|commands| TeletextEnhancementProgram::new(commands).unwrap()),
                resources: TeletextResolvedPageResourceSelection {
                    objects: TeletextResolvedObjectPageLinks {
                        public: selected,
                        global: None,
                    },
                    drcs: TeletextDrcsPageLinks::unlinked(),
                    defaults,
                },
            }],
            magazines: vec![],
            objects,
            drcs: vec![],
        },
        TeletextServiceBudget {
            max_records: 32,
            max_objects: 16,
            max_structure_items: 4096,
            max_base_content_bytes: 1024,
            max_commands: 4096,
            max_collected_commands: 4096,
            max_collected_drcs_references: 0,
        },
    )
    .unwrap()
}

fn resolve(service: &TeletextSemanticService) -> TeletextSpatialTrace {
    resolve_teletext_spatial_trace(
        service,
        owner(),
        TeletextSpatialTraceBudget {
            max_events: 128,
            max_invocations: 16,
        },
    )
    .unwrap()
}

fn absolute(position: TeletextSpatialPosition) -> (u16, u16) {
    (position.row(), position.column())
}

#[test]
fn repeated_full_identity_uses_distinct_invocations_and_exact_offsets_without_moving_caller() {
    let target = local(0xffff);
    let service = service(
        Some(vec![
            position(2, 8),
            invoke(target, TeletextObjectType::Passive, Some((1, 2))),
            glyph(9, b'B'),
            position(5, 1),
            invoke(target, TeletextObjectType::Passive, Some((0, 4))),
        ]),
        vec![definition(
            target,
            TeletextObjectType::Passive,
            vec![position(1, 3), glyph(4, b'A')],
        )],
        [None; 4],
        None,
    );
    let trace = resolve(&service);
    assert_eq!(trace.owner(), owner());
    assert_eq!(
        trace.original_program_source(),
        &TeletextEffectiveProgramSource::Local
    );
    assert_eq!(trace.events().len(), 9);
    let calls = trace.invocations();
    assert_eq!(calls.len(), 2);
    assert_eq!((calls[0].id().get(), calls[1].id().get()), (0, 1));
    for call in calls {
        assert_eq!(call.target(), target);
        assert_eq!(call.parent(), None);
    }
    assert_eq!(absolute(calls[0].caller_position()), (2, 8));
    assert_eq!(absolute(calls[0].origin()), (3, 10));
    assert_eq!(absolute(calls[1].caller_position()), (5, 1));
    assert_eq!(absolute(calls[1].origin()), (5, 5));
    assert_eq!(
        calls[0].offset(),
        Some(TeletextObjectOffset::new(1, 2).unwrap())
    );
    let glyph_events: Vec<_> = trace
        .events()
        .iter()
        .filter(|event| matches!(event.command(), TeletextEnhancementCommand::AtColumn { .. }))
        .map(|event| {
            (
                absolute(event.absolute_after()),
                event.invocation().map(TeletextSpatialInvocationId::get),
            )
        })
        .collect();
    assert_eq!(
        glyph_events,
        [((4, 14), Some(0)), ((2, 9), None), ((6, 9), Some(1))]
    );
    assert_eq!(
        calls[0].source(),
        TeletextEnhancementSource {
            object: None,
            command: 1
        }
    );
    assert_eq!(
        calls[1].root_source(),
        TeletextSpatialRootSource::Local { command: 4 }
    );
    assert_eq!(trace.events()[1].called_invocation(), Some(calls[0].id()));
}

#[test]
fn nested_types_accumulate_origins_and_return_each_callers_relative_position() {
    let active = local(0xffff);
    let adaptive = local(0x0100);
    let passive = local(0);
    let service = service(
        Some(vec![
            position(2, 8),
            invoke(active, TeletextObjectType::Active, Some((1, 2))),
            glyph(9, b'D'),
        ]),
        vec![
            definition(
                active,
                TeletextObjectType::Active,
                vec![
                    position(1, 3),
                    invoke(adaptive, TeletextObjectType::Adaptive, Some((2, 4))),
                    glyph(4, b'C'),
                ],
            ),
            definition(
                adaptive,
                TeletextObjectType::Adaptive,
                vec![
                    position(1, 5),
                    invoke(passive, TeletextObjectType::Passive, Some((3, 6))),
                    glyph(6, b'B'),
                ],
            ),
            definition(
                passive,
                TeletextObjectType::Passive,
                vec![position(2, 7), glyph(8, b'A')],
            ),
        ],
        [None; 4],
        None,
    );
    let trace = resolve(&service);
    assert_eq!(trace.events().len(), 11);
    assert_eq!(
        trace
            .invocations()
            .iter()
            .map(|call| absolute(call.origin()))
            .collect::<Vec<_>>(),
        [(3, 10), (6, 17), (10, 28)]
    );
    assert_eq!(
        trace.invocations()[1].parent(),
        Some(trace.invocations()[0].id())
    );
    assert_eq!(
        trace.invocations()[2].parent(),
        Some(trace.invocations()[1].id())
    );
    let cells: Vec<_> = trace
        .events()
        .iter()
        .filter(|event| matches!(event.command(), TeletextEnhancementCommand::AtColumn { .. }))
        .map(|event| {
            (
                event.relative_before(),
                event.relative_after(),
                absolute(event.absolute_after()),
            )
        })
        .collect();
    assert_eq!(
        cells,
        [
            ((2, 7), (2, 8), (12, 36)),
            ((1, 5), (1, 6), (7, 23)),
            ((1, 3), (1, 4), (4, 14)),
            ((2, 8), (2, 9), (2, 9))
        ]
    );
    assert!(trace.events()[2..10]
        .iter()
        .all(|event| event.root_source() == TeletextSpatialRootSource::Local { command: 1 }));
    assert_eq!(
        trace.events()[7].source(),
        TeletextEnhancementSource {
            object: Some(passive),
            command: 1
        }
    );
}

#[test]
fn sparse_defaults_keep_original_resource_owner_and_sparse_root_slot() {
    let first = global(0xa2, 0xffff);
    let last = public(0xa0, 0);
    let selected = TeletextRawDataPageNumber::new(2, 0xa1).unwrap();
    let defaults = [
        Some(TeletextDefaultObjectSelection {
            target: first,
            object_type: TeletextObjectType::Passive,
        }),
        None,
        None,
        Some(TeletextDefaultObjectSelection {
            target: last,
            object_type: TeletextObjectType::Passive,
        }),
    ];
    let service = service(
        None,
        vec![
            definition(
                first,
                TeletextObjectType::Passive,
                vec![position(1, 2), glyph(3, b'A')],
            ),
            definition(last, TeletextObjectType::Passive, vec![glyph(4, b'B')]),
            definition(public(0xa1, 0xffff), TeletextObjectType::Passive, vec![]),
        ],
        defaults,
        Some(selected),
    );
    let trace = resolve(&service);
    assert_eq!(
        trace.original_program_source(),
        &TeletextEffectiveProgramSource::Defaults { slots: vec![0, 3] }
    );
    assert_eq!(
        trace
            .invocations()
            .iter()
            .map(|call| call.target())
            .collect::<Vec<_>>(),
        [first, last]
    );
    assert_eq!(
        trace.invocations()[0].root_source(),
        TeletextSpatialRootSource::DefaultSlot { slot: 0 }
    );
    assert_eq!(
        trace.invocations()[1].root_source(),
        TeletextSpatialRootSource::DefaultSlot { slot: 3 }
    );
    for call in trace.invocations() {
        assert_eq!(absolute(call.origin()), (0, 0));
        assert_eq!(call.offset(), None);
    }
    assert_eq!(
        trace.events()[2].root_source(),
        TeletextSpatialRootSource::DefaultSlot { slot: 0 }
    );
    assert_eq!(
        trace.events()[4].root_source(),
        TeletextSpatialRootSource::DefaultSlot { slot: 3 }
    );
    assert_eq!(absolute(trace.events()[4].absolute_after()), (0, 4));
}

#[test]
fn explicit_empty_main_suppresses_defaults_while_absence_and_base_only_stay_distinct() {
    let target = global(0xa2, 1);
    let defaults = [
        None,
        Some(TeletextDefaultObjectSelection {
            target,
            object_type: TeletextObjectType::Passive,
        }),
        None,
        None,
    ];
    let object = definition(target, TeletextObjectType::Passive, vec![glyph(3, b'A')]);
    let absent = resolve(&service(None, vec![object.clone()], defaults, None));
    assert_eq!(absent.events().len(), 2);
    assert_eq!(
        absent.original_program_source(),
        &TeletextEffectiveProgramSource::Defaults { slots: vec![1] }
    );
    let empty = resolve(&service(Some(vec![]), vec![object], defaults, None));
    assert_eq!(
        empty.original_program_source(),
        &TeletextEffectiveProgramSource::Local
    );
    assert!(empty.events().is_empty() && empty.invocations().is_empty());
    let base = resolve(&service(None, vec![], [None; 4], None));
    assert_eq!(
        base.original_program_source(),
        &TeletextEffectiveProgramSource::BaseOnly
    );
    assert!(base.events().is_empty());
}

#[test]
fn screen_colour_keeps_ap_and_row_zero_colour_selects_the_original_header_origin() {
    let screen = TeletextEnhancementCommand::SetScreenColour {
        colour: TeletextColourReference::new(13).unwrap(),
    };
    let row = TeletextEnhancementCommand::SetRowColour {
        row: 0,
        colour: TeletextColourReference::new(6).unwrap(),
        following_rows: true,
    };
    let main = vec![screen, row, glyph(8, b'A'), screen];
    let trace = resolve(&service(Some(main.clone()), vec![], [None; 4], None));
    assert_eq!(
        trace
            .events()
            .iter()
            .map(|event| event.relative_after())
            .collect::<Vec<_>>(),
        [(0, 0), (0, 0), (0, 8), (0, 8)]
    );
    assert_eq!(
        trace
            .events()
            .iter()
            .map(|event| event.command())
            .collect::<Vec<_>>(),
        main
    );
    assert_eq!(trace.events()[1].absolute_after().row(), 0);
    assert_eq!(
        trace.events()[3].absolute_before(),
        trace.events()[3].absolute_after()
    );
    for (command, event) in trace.events().iter().enumerate() {
        assert_eq!(
            event.source(),
            TeletextEnhancementSource {
                object: None,
                command
            }
        );
        assert_eq!(
            event.root_source(),
            TeletextSpatialRootSource::Local { command }
        );
    }
}

#[test]
fn original_main_backwards_move_is_located_and_does_not_mutate_service() {
    let service = service(
        Some(vec![position(2, 8), position(1, 9)]),
        vec![],
        [None; 4],
        None,
    );
    let before = service.clone();
    let error = resolve_teletext_spatial_trace(
        &service,
        owner(),
        TeletextSpatialTraceBudget {
            max_events: 16,
            max_invocations: 4,
        },
    )
    .unwrap_err();
    assert_eq!(
        error.location,
        Some(TeletextEnhancementSource {
            object: None,
            command: 1
        })
    );
    assert_eq!(
        error.root_source,
        Some(TeletextSpatialRootSource::Local { command: 1 })
    );
    assert_eq!(
        error.reason,
        TeletextSpatialTraceFailure::ActivePositionOrder {
            previous: (2, 8),
            requested: (1, 9)
        }
    );
    assert_eq!(service, before);
}

#[test]
fn all_object_types_reject_backwards_relative_ap_but_child_return_is_separate() {
    let target = local(0xffff);
    for object_type in [
        TeletextObjectType::Active,
        TeletextObjectType::Adaptive,
        TeletextObjectType::Passive,
    ] {
        let service = service(
            Some(vec![invoke(target, object_type, None)]),
            vec![definition(
                target,
                object_type,
                vec![position(2, 8), glyph(7, b'A')],
            )],
            [None; 4],
            None,
        );
        let before = service.clone();
        let error = resolve_teletext_spatial_trace(
            &service,
            owner(),
            TeletextSpatialTraceBudget {
                max_events: 16,
                max_invocations: 4,
            },
        )
        .unwrap_err();
        assert_eq!(
            error.location,
            Some(TeletextEnhancementSource {
                object: Some(target),
                command: 1
            })
        );
        assert_eq!(
            error.root_source,
            Some(TeletextSpatialRootSource::Local { command: 0 })
        );
        assert_eq!(
            error.invocation.map(TeletextSpatialInvocationId::get),
            Some(0)
        );
        assert_eq!(
            error.reason,
            TeletextSpatialTraceFailure::ActivePositionOrder {
                previous: (2, 8),
                requested: (2, 7)
            }
        );
        assert_eq!(service, before);
    }
}

#[test]
fn exact_event_and_invocation_budgets_charge_expanded_original_commands() {
    let target = local(0xffff);
    let service = service(
        Some(vec![
            position(2, 8),
            invoke(target, TeletextObjectType::Passive, None),
            glyph(9, b'B'),
        ]),
        vec![definition(
            target,
            TeletextObjectType::Passive,
            vec![position(1, 3), glyph(4, b'A')],
        )],
        [None; 4],
        None,
    );
    let before = service.clone();
    let exact = resolve_teletext_spatial_trace(
        &service,
        owner(),
        TeletextSpatialTraceBudget {
            max_events: 5,
            max_invocations: 1,
        },
    )
    .unwrap();
    assert_eq!((exact.events().len(), exact.invocations().len()), (5, 1));
    let error = resolve_teletext_spatial_trace(
        &service,
        owner(),
        TeletextSpatialTraceBudget {
            max_events: 4,
            max_invocations: 1,
        },
    )
    .unwrap_err();
    assert_eq!(
        error.reason,
        TeletextSpatialTraceFailure::EventBudget { maximum: 4 }
    );
    let error = resolve_teletext_spatial_trace(
        &service,
        owner(),
        TeletextSpatialTraceBudget {
            max_events: 5,
            max_invocations: 0,
        },
    )
    .unwrap_err();
    assert_eq!(
        error.reason,
        TeletextSpatialTraceFailure::InvocationBudget { maximum: 0 }
    );
    assert_eq!(service, before);
}

#[test]
fn repeated_body_work_is_charged_again_without_deduplicating_full_identity() {
    let target = local(0xffff);
    let service = service(
        Some(vec![
            invoke(target, TeletextObjectType::Passive, None),
            invoke(target, TeletextObjectType::Passive, None),
        ]),
        vec![definition(
            target,
            TeletextObjectType::Passive,
            vec![position(1, 3), glyph(4, b'A')],
        )],
        [None; 4],
        None,
    );
    let exact = resolve_teletext_spatial_trace(
        &service,
        owner(),
        TeletextSpatialTraceBudget {
            max_events: 6,
            max_invocations: 2,
        },
    )
    .unwrap();
    assert_eq!((exact.events().len(), exact.invocations().len()), (6, 2));
    let error = resolve_teletext_spatial_trace(
        &service,
        owner(),
        TeletextSpatialTraceBudget {
            max_events: 5,
            max_invocations: 2,
        },
    )
    .unwrap_err();
    assert_eq!(
        error.reason,
        TeletextSpatialTraceFailure::EventBudget { maximum: 5 }
    );
    assert_eq!(
        error.invocation.map(TeletextSpatialInvocationId::get),
        Some(1)
    );
    assert_eq!(
        error.location,
        Some(TeletextEnhancementSource {
            object: Some(target),
            command: 1
        })
    );
    assert_eq!(
        error.root_source,
        Some(TeletextSpatialRootSource::Local { command: 1 })
    );
    let error = resolve_teletext_spatial_trace(
        &service,
        owner(),
        TeletextSpatialTraceBudget {
            max_events: 6,
            max_invocations: 1,
        },
    )
    .unwrap_err();
    assert_eq!(
        error.reason,
        TeletextSpatialTraceFailure::InvocationBudget { maximum: 1 }
    );
    assert_eq!(error.invocation, None);
    assert_eq!(
        error.location,
        Some(TeletextEnhancementSource {
            object: None,
            command: 1
        })
    );
    assert_eq!(
        error.root_source,
        Some(TeletextSpatialRootSource::Local { command: 1 })
    );
}

#[test]
fn legal_relative_offsets_accumulate_beyond_u8_without_clamping_or_wrapping() {
    let active = local(0xffff);
    let adaptive = local(0x0100);
    let passive = local(0);
    let service = service(
        Some(vec![
            position(24, 39),
            invoke(active, TeletextObjectType::Active, Some((23, 71))),
        ]),
        vec![
            definition(
                active,
                TeletextObjectType::Active,
                vec![
                    position(24, 39),
                    invoke(adaptive, TeletextObjectType::Adaptive, Some((23, 71))),
                ],
            ),
            definition(
                adaptive,
                TeletextObjectType::Adaptive,
                vec![
                    position(24, 39),
                    invoke(passive, TeletextObjectType::Passive, Some((23, 71))),
                ],
            ),
            definition(
                passive,
                TeletextObjectType::Passive,
                vec![position(24, 39), glyph(39, b'A')],
            ),
        ],
        [None; 4],
        None,
    );
    let trace = resolve(&service);
    assert_eq!(
        trace
            .invocations()
            .iter()
            .map(|call| absolute(call.origin()))
            .collect::<Vec<_>>(),
        [(47, 110), (94, 220), (141, 330)]
    );
    assert_eq!(
        absolute(trace.events().last().unwrap().absolute_after()),
        (165, 369)
    );
    assert_eq!(trace.events().last().unwrap().relative_after(), (24, 39));
}

#[test]
fn exact_subcode_unknown_owner_is_rejected_without_returning_partial_events() {
    let service = service(Some(vec![glyph(1, b'A')]), vec![], [None; 4], None);
    let mut other = owner();
    other.subcode = TeletextSubcode::new(0x1235).unwrap();
    let error = resolve_teletext_spatial_trace(
        &service,
        other,
        TeletextSpatialTraceBudget {
            max_events: 16,
            max_invocations: 4,
        },
    )
    .unwrap_err();
    assert_eq!(error.owner, other);
    assert!(
        matches!(error.reason,TeletextSpatialTraceFailure::Service(reason) if matches!(*reason, TeletextServiceError::MissingPage {address} if address==other))
    );
}

fn replay_budget() -> TeletextSpatialTraceBudget {
    TeletextSpatialTraceBudget {
        max_events: 128,
        max_invocations: 16,
    }
}

#[test]
fn supplied_local_program_trace_equals_existing_service_trace_without_recomposing() {
    let target = local(0xffff);
    let service = service(
        Some(vec![
            position(2, 8),
            invoke(target, TeletextObjectType::Passive, None),
            glyph(9, b'B'),
        ]),
        vec![definition(
            target,
            TeletextObjectType::Passive,
            vec![position(1, 3), glyph(4, b'A')],
        )],
        [None; 4],
        None,
    );
    let (source, program) = service.effective_program(owner()).unwrap();
    assert_eq!(source, TeletextEffectiveProgramSource::Local);
    let replay =
        resolve_teletext_spatial_program(owner(), &program, service.objects(), replay_budget())
            .unwrap();
    assert_eq!(replay, resolve(&service));
}

#[test]
fn service_default_slots_remain_original_while_supplied_program_ordinals_are_local() {
    let first = global(0xa2, 0xfffe);
    let last = public(0xa0, 0xffff);
    let service = service(
        None,
        vec![
            definition(first, TeletextObjectType::Passive, vec![glyph(1, b'A')]),
            definition(last, TeletextObjectType::Passive, vec![glyph(2, b'B')]),
        ],
        [
            Some(TeletextDefaultObjectSelection {
                target: first,
                object_type: TeletextObjectType::Passive,
            }),
            None,
            None,
            Some(TeletextDefaultObjectSelection {
                target: last,
                object_type: TeletextObjectType::Passive,
            }),
        ],
        None,
    );
    let original = resolve(&service);
    let (_, program) = service.effective_program(owner()).unwrap();
    let replay =
        resolve_teletext_spatial_program(owner(), &program, service.objects(), replay_budget())
            .unwrap();
    assert_eq!(
        original.original_program_source(),
        &TeletextEffectiveProgramSource::Defaults { slots: vec![0, 3] }
    );
    assert_eq!(
        original.invocations()[1].root_source(),
        TeletextSpatialRootSource::DefaultSlot { slot: 3 }
    );
    assert_eq!(
        replay.original_program_source(),
        &TeletextEffectiveProgramSource::Local
    );
    assert_eq!(
        replay.invocations()[1].root_source(),
        TeletextSpatialRootSource::Local { command: 1 }
    );
    assert_eq!(replay.events()[3].source(), original.events()[3].source());
    assert_eq!(replay.invocations()[1].target(), last);
    assert_eq!(original, resolve(&service));
}

#[test]
fn augmented_main_and_new_carrier_catalog_keep_exact_ordinals_full_id_and_instance() {
    let active = local(7);
    let carrier = local(0xffff);
    let service = service(
        Some(vec![
            position(1, 8),
            invoke(active, TeletextObjectType::Active, None),
            glyph(9, b'B'),
        ]),
        vec![definition(
            active,
            TeletextObjectType::Active,
            vec![glyph(0, b'A')],
        )],
        [None; 4],
        None,
    );
    let mut definitions = service.objects().definitions().to_vec();
    definitions.push(definition(
        carrier,
        TeletextObjectType::Passive,
        vec![glyph(0, b'Z')],
    ));
    let catalog = TeletextEnhancementObjectCatalog::new(
        definitions,
        TeletextObjectCatalogBudget {
            max_objects: 2,
            max_commands: 2,
        },
    )
    .unwrap();
    let program = TeletextEnhancementProgram::new(vec![
        position(1, 8),
        invoke(active, TeletextObjectType::Active, None),
        TeletextEnhancementCommand::AtColumn {
            column: 8,
            operation: TeletextColumnOperation::G2 {
                code: TeletextGlyphCode::new(0x23).unwrap(),
            },
        },
        invoke(carrier, TeletextObjectType::Passive, None),
        glyph(9, b'B'),
    ])
    .unwrap();
    let collected = catalog
        .collect_display_references(
            owner(),
            &program,
            TeletextPageCharsetDesignation::new(0).unwrap().pair(),
            TeletextEnhancementCollectionBudget {
                max_commands: 7,
                max_drcs_references: 0,
            },
        )
        .unwrap();
    assert_eq!(collected.command_count(), 7);
    let replay =
        resolve_teletext_spatial_program(owner(), &program, &catalog, replay_budget()).unwrap();
    assert_eq!((replay.events().len(), replay.invocations().len()), (7, 2));
    assert_eq!(replay.events()[3].command(), program.commands()[2]);
    assert_eq!(
        replay.events()[3].source(),
        TeletextEnhancementSource {
            object: None,
            command: 2
        }
    );
    assert_eq!(
        replay.events()[5].source(),
        TeletextEnhancementSource {
            object: Some(carrier),
            command: 0
        }
    );
    assert_eq!(
        replay.events()[5].root_source(),
        TeletextSpatialRootSource::Local { command: 3 }
    );
    assert_eq!(replay.events()[5].invocation().unwrap().get(), 1);
    assert_eq!(absolute(replay.events()[5].absolute_after()), (1, 8));
    assert_eq!(replay.events()[6].source().command, 4);
    assert_eq!(service.objects().definitions().len(), 1);
}

#[test]
fn supplied_replay_exact_budget_failures_preserve_inputs_and_augmented_blame() {
    let carrier = local(0xffff);
    let catalog = TeletextEnhancementObjectCatalog::new(
        vec![definition(
            carrier,
            TeletextObjectType::Passive,
            vec![glyph(0, b'Z')],
        )],
        TeletextObjectCatalogBudget {
            max_objects: 1,
            max_commands: 1,
        },
    )
    .unwrap();
    let program = TeletextEnhancementProgram::new(vec![
        position(1, 8),
        glyph(8, b'A'),
        invoke(carrier, TeletextObjectType::Passive, None),
        glyph(9, b'B'),
    ])
    .unwrap();
    let before = (program.clone(), catalog.clone());
    let exact = resolve_teletext_spatial_program(
        owner(),
        &program,
        &catalog,
        TeletextSpatialTraceBudget {
            max_events: 5,
            max_invocations: 1,
        },
    )
    .unwrap();
    assert_eq!((exact.events().len(), exact.invocations().len()), (5, 1));
    let error = resolve_teletext_spatial_program(
        owner(),
        &program,
        &catalog,
        TeletextSpatialTraceBudget {
            max_events: 4,
            max_invocations: 1,
        },
    )
    .unwrap_err();
    assert_eq!(
        error.reason,
        TeletextSpatialTraceFailure::EventBudget { maximum: 4 }
    );
    assert_eq!(
        error.location,
        Some(TeletextEnhancementSource {
            object: None,
            command: 3
        })
    );
    assert_eq!(
        error.root_source,
        Some(TeletextSpatialRootSource::Local { command: 3 })
    );
    let error = resolve_teletext_spatial_program(
        owner(),
        &program,
        &catalog,
        TeletextSpatialTraceBudget {
            max_events: 5,
            max_invocations: 0,
        },
    )
    .unwrap_err();
    assert_eq!(
        error.reason,
        TeletextSpatialTraceFailure::InvocationBudget { maximum: 0 }
    );
    assert_eq!(
        error.location,
        Some(TeletextEnhancementSource {
            object: None,
            command: 2
        })
    );
    assert_eq!((program, catalog), before);
}
