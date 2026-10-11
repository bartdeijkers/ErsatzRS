//! Original DRCS resource/pixel oracles; no decoder, placement or readiness claim.

use super::*;

const NORMAL: [u16; 10] = [
    0x800, 0x401, 0x202, 0x104, 0x088, 0x050, 0x020, 0x010, 0x008, 0x001,
];
const GLOBAL: [u16; 10] = [
    0x003, 0x006, 0x00c, 0x018, 0x030, 0x060, 0x0c0, 0x180, 0x300, 0xc00,
];

fn owner() -> TeletextPageAddress {
    TeletextPageAddress {
        page: TeletextPageNumber::new(200).unwrap(),
        subcode: TeletextSubcode::new(0x1234).unwrap(),
    }
}
fn identity(table: TeletextDrcsTableScope) -> TeletextResourceIdentity {
    let (kind, magazine, page, subpage) = match table {
        TeletextDrcsTableScope::Normal => (TeletextResourceKind::Drcs, 2, 0xb3, 3),
        TeletextDrcsTableScope::Global => (TeletextResourceKind::Gdrcs, 8, 0xa4, 9),
    };
    TeletextResourceIdentity::new(
        kind,
        TeletextRawDataPageNumber::new(magazine, page).unwrap(),
        TeletextResourceSubpageIndex::new(subpage).unwrap(),
    )
    .unwrap()
}
fn reference(table: TeletextDrcsTableScope, slot: u8) -> TeletextDrcsGlyphReference {
    TeletextDrcsGlyphReference {
        table,
        subtable: TeletextDrcsSubtableIndex::new(if table == TeletextDrcsTableScope::Normal {
            3
        } else {
            9
        })
        .unwrap(),
        slot: TeletextDrcsGlyphSlot::new(slot).unwrap(),
    }
}
fn drcs(column: u8, table: TeletextDrcsTableScope, slot: u8) -> TeletextEnhancementCommand {
    TeletextEnhancementCommand::AtColumn {
        column,
        operation: TeletextColumnOperation::Drcs {
            reference: reference(table, slot),
        },
    }
}
fn set(row: u8, column: u8) -> TeletextEnhancementCommand {
    TeletextEnhancementCommand::SetPosition {
        position: TeletextEnhancementPosition::new(row, column).unwrap(),
    }
}
fn local(id: u16) -> TeletextObjectIdentity {
    TeletextObjectIdentity::local(owner(), TeletextObjectId::new(id))
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
fn tables(normal: [u16; 10]) -> Vec<TeletextDrcsSubtable> {
    vec![
        TeletextDrcsSubtable::new(
            identity(TeletextDrcsTableScope::Normal),
            vec![
                TeletextDrcsGlyphDefinition {
                    slot: TeletextDrcsGlyphSlot::new(7).unwrap(),
                    bitmap: TeletextDrcsBitmap::new(normal).unwrap(),
                },
                TeletextDrcsGlyphDefinition {
                    slot: TeletextDrcsGlyphSlot::new(8).unwrap(),
                    bitmap: TeletextDrcsBitmap::new([0; 10]).unwrap(),
                },
            ],
        )
        .unwrap(),
        TeletextDrcsSubtable::new(
            identity(TeletextDrcsTableScope::Global),
            vec![TeletextDrcsGlyphDefinition {
                slot: TeletextDrcsGlyphSlot::new(47).unwrap(),
                bitmap: TeletextDrcsBitmap::new(GLOBAL).unwrap(),
            }],
        )
        .unwrap(),
    ]
}
fn links() -> TeletextDrcsPageLinks {
    TeletextDrcsPageLinks::new(
        Some(identity(TeletextDrcsTableScope::Normal).page()),
        Some(identity(TeletextDrcsTableScope::Global).page()),
    )
    .unwrap()
}
fn draft(
    commands: Vec<TeletextEnhancementCommand>,
    objects: Vec<TeletextEnhancementObjectDefinition>,
    drcs: Vec<TeletextDrcsSubtable>,
) -> TeletextResolvedServiceDraft {
    let language = TeletextLanguage::new("nld".into()).unwrap();
    TeletextResolvedServiceDraft {
        base: TeletextPageSetDraft {
            language: language.clone(),
            initial_page: owner().page,
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
            local_program: Some(TeletextEnhancementProgram::new(commands).unwrap()),
            resources: TeletextResolvedPageResourceSelection {
                objects: TeletextResolvedObjectPageLinks {
                    public: None,
                    global: None,
                },
                defaults: [None; 4],
                drcs: links(),
            },
        }],
        magazines: vec![],
        objects,
        drcs,
    }
}
fn service(draft: TeletextResolvedServiceDraft) -> TeletextSemanticService {
    TeletextSemanticService::new(
        draft,
        TeletextServiceBudget {
            max_records: 32,
            max_objects: 16,
            max_structure_items: 4096,
            max_base_content_bytes: 1024,
            max_commands: 4096,
            max_collected_commands: 4096,
            max_collected_drcs_references: 128,
        },
    )
    .unwrap()
}
fn base() -> TeletextBaseDisplay {
    let rows: Vec<[TeletextBaseDisplayCell; 40]> = (0..25)
        .map(|row| {
            let source = TeletextBaseAttributeSource::RowDefault {
                row,
                start_column: if row == 0 { 8 } else { 0 },
            };
            let attributes = TeletextBaseDisplayAttributes::new(
                TeletextBaseAttributeValues {
                    raw_foreground: 7,
                    raw_background: 0,
                    foreground: TeletextColourReference::new(7).unwrap(),
                    background: TeletextColourReference::BLACK,
                    graphics: false,
                    separated: false,
                    double_height: false,
                    boxed: false,
                    flash: false,
                    conceal: false,
                },
                TeletextBaseAttributeSources {
                    foreground: source,
                    background: source,
                    graphics: source,
                    separated: source,
                    height: source,
                    boxed: source,
                    flash: source,
                    conceal: source,
                },
            )
            .unwrap();
            let cells: Vec<_> = (0..40)
                .map(|column| {
                    TeletextBaseDisplayCell::new(
                        TeletextEnhancementPosition::new(row, column).unwrap(),
                        b' ',
                        if row == 0 && column < 8 {
                            TeletextBaseDisplayCellKind::HeaderOwned
                        } else {
                            TeletextBaseDisplayCellKind::Blank
                        },
                        None,
                        attributes,
                    )
                    .unwrap()
                })
                .collect();
            cells.try_into().unwrap()
        })
        .collect();
    TeletextBaseDisplay::new(
        owner(),
        TeletextPageCharsetDesignation::new(0).unwrap(),
        TeletextNationalOption::new(0).unwrap(),
        None,
        rows.into_boxed_slice().try_into().unwrap(),
    )
    .unwrap()
}
fn intent(service: &TeletextSemanticService) -> TeletextGlyphIntent {
    let original = resolve_teletext_spatial_trace(
        service,
        owner(),
        TeletextSpatialTraceBudget {
            max_events: 128,
            max_invocations: 16,
        },
    )
    .unwrap();
    let base = base();
    let layers = build_teletext_glyph_layers(&base, &original, 128).unwrap();
    let charset = resolve_teletext_charset_execution(&base, &original, &layers, 128).unwrap();
    resolve_teletext_glyph_intent(&charset, charset.records().len()).unwrap()
}
fn resolve(
    service: &TeletextSemanticService,
    intent: &TeletextGlyphIntent,
) -> TeletextDrcsExecution {
    resolve_teletext_drcs_execution(
        service,
        intent,
        TeletextDrcsExecutionBudget {
            max_candidates: intent.records().len(),
            max_references: 128,
        },
    )
    .unwrap()
}

#[test]
fn normal_and_global_use_distinct_linked_subtables_and_exact_asymmetric_pixels() {
    let service = service(draft(
        vec![
            set(1, 8),
            drcs(8, TeletextDrcsTableScope::Normal, 7),
            drcs(9, TeletextDrcsTableScope::Global, 47),
        ],
        vec![],
        tables(NORMAL),
    ));
    let intent = intent(&service);
    let result = resolve(&service, &intent);
    assert_eq!(result.owner(), owner());
    assert_eq!(result.records().len(), 2);
    for (record, table, slot, pixels, position, command) in [
        (
            &result.records()[0],
            TeletextDrcsTableScope::Normal,
            7,
            NORMAL,
            (1, 8),
            1,
        ),
        (
            &result.records()[1],
            TeletextDrcsTableScope::Global,
            47,
            GLOBAL,
            (1, 9),
            2,
        ),
    ] {
        assert_eq!(record.reference(), reference(table, slot));
        assert_eq!(record.resource(), identity(table));
        assert_eq!(record.bitmap().get(), pixels);
        assert_eq!(record.candidate().position(), position);
        assert_eq!(
            record.candidate().source(),
            Some(TeletextEnhancementSource {
                object: None,
                command
            })
        );
        assert_eq!(
            record.candidate(),
            intent.records()[record.candidate_index()]
                .context()
                .candidate()
        );
    }
}

#[test]
fn repeated_nested_objects_retain_full_identity_root_source_and_distinct_instances() {
    let active = local(0xffff);
    let adaptive = local(0x0100);
    let passive = local(0);
    let service = service(draft(
        vec![
            set(1, 8),
            invoke(active, TeletextObjectType::Active, None),
            set(2, 8),
            invoke(active, TeletextObjectType::Active, None),
        ],
        vec![
            definition(
                active,
                TeletextObjectType::Active,
                vec![
                    set(1, 0),
                    invoke(adaptive, TeletextObjectType::Adaptive, None),
                ],
            ),
            definition(
                adaptive,
                TeletextObjectType::Adaptive,
                vec![
                    set(1, 0),
                    invoke(passive, TeletextObjectType::Passive, None),
                ],
            ),
            definition(
                passive,
                TeletextObjectType::Passive,
                vec![drcs(0, TeletextDrcsTableScope::Normal, 7)],
            ),
        ],
        tables(NORMAL),
    ));
    let intent = intent(&service);
    let result = resolve(&service, &intent);
    assert_eq!(result.records().len(), 2);
    for (record, row, instance, root) in [
        (&result.records()[0], 3, 2, 1),
        (&result.records()[1], 4, 5, 3),
    ] {
        assert_eq!(record.candidate().position(), (row, 8));
        assert_eq!(
            record.candidate().source(),
            Some(TeletextEnhancementSource {
                object: Some(passive),
                command: 0
            })
        );
        assert_eq!(
            record.candidate().root_source(),
            Some(TeletextSpatialRootSource::Local { command: root })
        );
        assert_eq!(record.candidate().invocation().unwrap().get(), instance);
        assert_eq!(record.bitmap().get(), NORMAL);
    }
    let error = resolve_teletext_drcs_execution(
        &service,
        &intent,
        TeletextDrcsExecutionBudget {
            max_candidates: intent.records().len(),
            max_references: 1,
        },
    )
    .unwrap_err();
    assert_eq!(
        error.location,
        Some(TeletextEnhancementSource {
            object: Some(passive),
            command: 0
        })
    );
    assert_eq!(
        error.root_source,
        Some(TeletextSpatialRootSource::Local { command: 3 })
    );
    assert_eq!(error.invocation.unwrap().get(), 5);
    assert_eq!(error.position, Some((4, 8)));
}

#[test]
fn each_fresh_service_resolves_current_pixels_without_cache_by_resource_identity() {
    let first = service(draft(
        vec![set(1, 8), drcs(8, TeletextDrcsTableScope::Normal, 7)],
        vec![],
        tables(NORMAL),
    ));
    let changed = [
        0x001, 0x002, 0x004, 0x008, 0x010, 0x020, 0x040, 0x080, 0x100, 0x800,
    ];
    let second = service(draft(
        vec![set(1, 8), drcs(8, TeletextDrcsTableScope::Normal, 7)],
        vec![],
        tables(changed),
    ));
    let first_intent = intent(&first);
    let second_intent = intent(&second);
    let one = resolve(&first, &first_intent);
    let two = resolve(&second, &second_intent);
    assert_eq!(one.records()[0].resource(), two.records()[0].resource());
    assert_eq!(one.records()[0].reference(), two.records()[0].reference());
    assert_eq!(one.records()[0].bitmap().get(), NORMAL);
    assert_eq!(two.records()[0].bitmap().get(), changed);
    assert_eq!(
        resolve(&first, &first_intent).records()[0].bitmap().get(),
        NORMAL
    );
}

#[test]
fn defined_zero_bitmap_is_a_glyph_while_missing_slot_is_constructor_error() {
    let valid = service(draft(
        vec![set(1, 8), drcs(8, TeletextDrcsTableScope::Normal, 8)],
        vec![],
        tables(NORMAL),
    ));
    let result = resolve(&valid, &intent(&valid));
    assert_eq!(result.records()[0].bitmap().get(), [0; 10]);
    assert_eq!(result.records()[0].reference().slot.get(), 8);
    let missing = draft(
        vec![set(1, 8), drcs(8, TeletextDrcsTableScope::Normal, 9)],
        vec![],
        tables(NORMAL),
    );
    let error = TeletextSemanticService::new(
        missing,
        TeletextServiceBudget {
            max_records: 32,
            max_objects: 16,
            max_structure_items: 4096,
            max_base_content_bytes: 1024,
            max_commands: 4096,
            max_collected_commands: 4096,
            max_collected_drcs_references: 128,
        },
    )
    .unwrap_err();
    assert!(matches!(error, TeletextServiceError::CollectedDrcs {
            program_source: TeletextEffectiveProgramSource::Local,
            error: TeletextCollectedDrcsError {
                source_location: Some(TeletextEnhancementSource { object: None, command: 1 }),
                error: TeletextDrcsResourceError::MissingGlyph { display, identity: resource, slot, reference: 0 },
            },
        } if display == owner() && resource == identity(TeletextDrcsTableScope::Normal) && slot.get() == 9));
}

#[test]
fn losing_drcs_candidate_and_wide_nested_origin_remain_original_not_clamped() {
    let passive = local(0xffff);
    let same_cell = service(draft(
        vec![
            set(3, 8),
            drcs(8, TeletextDrcsTableScope::Normal, 7),
            invoke(passive, TeletextObjectType::Passive, None),
        ],
        vec![definition(
            passive,
            TeletextObjectType::Passive,
            vec![drcs(0, TeletextDrcsTableScope::Global, 47)],
        )],
        tables(NORMAL),
    ));
    let result = resolve(&same_cell, &intent(&same_cell));
    assert_eq!(result.records().len(), 2);
    assert_eq!(result.records()[0].candidate().position(), (3, 8));
    assert_eq!(result.records()[1].candidate().position(), (3, 8));
    assert_eq!(
        result.records()[0].candidate().priority(),
        TeletextGlyphPriority::Local
    );
    assert_eq!(
        result.records()[1].candidate().priority(),
        TeletextGlyphPriority::Passive
    );
    assert_eq!(result.records()[0].bitmap().get(), NORMAL);
    assert_eq!(result.records()[1].bitmap().get(), GLOBAL);

    let active = local(0x0100);
    let adaptive = local(0);
    let wide = service(draft(
        vec![
            set(24, 39),
            invoke(active, TeletextObjectType::Active, Some((23, 71))),
        ],
        vec![
            definition(
                active,
                TeletextObjectType::Active,
                vec![
                    set(24, 39),
                    invoke(adaptive, TeletextObjectType::Adaptive, Some((23, 71))),
                ],
            ),
            definition(
                adaptive,
                TeletextObjectType::Adaptive,
                vec![
                    set(24, 39),
                    invoke(passive, TeletextObjectType::Passive, Some((23, 71))),
                ],
            ),
            definition(
                passive,
                TeletextObjectType::Passive,
                vec![set(24, 0), drcs(39, TeletextDrcsTableScope::Normal, 7)],
            ),
        ],
        tables(NORMAL),
    ));
    let result = resolve(&wide, &intent(&wide));
    assert_eq!(result.records().len(), 1);
    assert_eq!(result.records()[0].candidate().position(), (165, 369));
    assert_eq!(
        result.records()[0].candidate().source(),
        Some(TeletextEnhancementSource {
            object: Some(passive),
            command: 1
        })
    );
    assert_eq!(
        result.records()[0].candidate().invocation().unwrap().get(),
        2
    );
    assert_eq!(result.records()[0].bitmap().get(), NORMAL);
}

#[test]
fn both_budgets_include_original_roles_and_locate_first_rejected_reference() {
    let service = service(draft(
        vec![
            set(1, 8),
            drcs(8, TeletextDrcsTableScope::Normal, 7),
            drcs(9, TeletextDrcsTableScope::Global, 47),
        ],
        vec![],
        tables(NORMAL),
    ));
    let original = intent(&service);
    let before = original.clone();
    let service_before = service.clone();
    let count = original.records().len();
    assert_eq!(count, 994);
    let exact = resolve_teletext_drcs_execution(
        &service,
        &original,
        TeletextDrcsExecutionBudget {
            max_candidates: count,
            max_references: 2,
        },
    )
    .unwrap();
    assert_eq!(exact.records().len(), 2);
    let candidates = resolve_teletext_drcs_execution(
        &service,
        &original,
        TeletextDrcsExecutionBudget {
            max_candidates: count - 1,
            max_references: 2,
        },
    )
    .unwrap_err();
    assert_eq!(
        candidates.reason,
        TeletextDrcsExecutionFailure::CandidateBudget {
            count,
            maximum: count - 1
        }
    );
    assert_eq!(candidates.position, Some((24, 39)));
    assert_eq!(candidates.location, None);
    let references = resolve_teletext_drcs_execution(
        &service,
        &original,
        TeletextDrcsExecutionBudget {
            max_candidates: count,
            max_references: 1,
        },
    )
    .unwrap_err();
    assert_eq!(
        references.reason,
        TeletextDrcsExecutionFailure::ReferenceBudget {
            count: 2,
            maximum: 1
        }
    );
    assert_eq!(references.position, Some((1, 9)));
    assert_eq!(
        references.location,
        Some(TeletextEnhancementSource {
            object: None,
            command: 2
        })
    );
    assert_eq!(
        references.root_source,
        Some(TeletextSpatialRootSource::Local { command: 2 })
    );
    let ordinal = exact.records()[0].candidate_index();
    let first = resolve_teletext_drcs_execution(
        &service,
        &original,
        TeletextDrcsExecutionBudget {
            max_candidates: ordinal,
            max_references: 2,
        },
    )
    .unwrap_err();
    assert_eq!(first.position, Some((1, 8)));
    assert_eq!(
        first.location,
        Some(TeletextEnhancementSource {
            object: None,
            command: 1
        })
    );
    assert_eq!(original, before);
    assert_eq!(service, service_before);
}

#[test]
fn changed_same_owner_program_cannot_borrow_another_displays_link_or_blank_fallback() {
    let original = service(draft(
        vec![set(1, 8), drcs(8, TeletextDrcsTableScope::Normal, 7)],
        vec![],
        tables(NORMAL),
    ));
    let input = intent(&original);
    let mut foreign = draft(vec![], vec![], tables(NORMAL));
    foreign.pages[0].resources.drcs = TeletextDrcsPageLinks::unlinked();
    let other = TeletextPageAddress {
        page: TeletextPageNumber::new(201).unwrap(),
        subcode: TeletextSubcode::new(0x2321).unwrap(),
    };
    let mut page = foreign.base.pages[0].clone();
    page.address = other;
    foreign.base.pages.push(page);
    let mut enhancement = foreign.pages[0].clone();
    enhancement.address = other;
    enhancement.resources.drcs = links();
    foreign.pages.push(enhancement);
    let foreign = service(foreign);
    let error = resolve_teletext_drcs_execution(
        &foreign,
        &input,
        TeletextDrcsExecutionBudget {
            max_candidates: input.records().len(),
            max_references: 1,
        },
    )
    .unwrap_err();
    assert_eq!(
        error.reason,
        TeletextDrcsExecutionFailure::CandidateMismatch
    );
    assert_eq!(error.owner, owner());
    assert_eq!(error.position, Some((1, 8)));
    assert_eq!(
        error.location,
        Some(TeletextEnhancementSource {
            object: None,
            command: 1
        })
    );
}

#[test]
fn exact_owner_subcode_is_required_and_no_drcs_input_returns_empty_sparse_records() {
    let original = service(draft(
        vec![set(1, 8), drcs(8, TeletextDrcsTableScope::Normal, 7)],
        vec![],
        tables(NORMAL),
    ));
    let input = intent(&original);
    let mut different = draft(vec![], vec![], tables(NORMAL));
    different.base.pages[0].address.subcode = TeletextSubcode::new(0x1235).unwrap();
    different.pages[0].address = different.base.pages[0].address;
    let error = resolve_teletext_drcs_execution(
        &service(different),
        &input,
        TeletextDrcsExecutionBudget {
            max_candidates: input.records().len(),
            max_references: 1,
        },
    )
    .unwrap_err();
    assert_eq!(error.owner, owner());
    assert_eq!(error.reason, TeletextDrcsExecutionFailure::MissingPage);
    let empty = service(draft(vec![], vec![], tables(NORMAL)));
    let input = intent(&empty);
    assert_eq!(input.records().len(), 992);
    let result = resolve_teletext_drcs_execution(
        &empty,
        &input,
        TeletextDrcsExecutionBudget {
            max_candidates: 992,
            max_references: 0,
        },
    )
    .unwrap();
    assert!(result.records().is_empty());
}

fn replay_catalog(commands: Vec<TeletextEnhancementCommand>) -> TeletextEnhancementObjectCatalog {
    TeletextEnhancementObjectCatalog::new(
        vec![definition(
            local(0xffff),
            TeletextObjectType::Passive,
            commands,
        )],
        TeletextObjectCatalogBudget {
            max_objects: 1,
            max_commands: 128,
        },
    )
    .unwrap()
}

fn replay_intent(
    program: &TeletextEnhancementProgram,
    catalog: &TeletextEnhancementObjectCatalog,
) -> TeletextGlyphIntent {
    let spatial = resolve_teletext_spatial_program(
        owner(),
        program,
        catalog,
        TeletextSpatialTraceBudget {
            max_events: 128,
            max_invocations: 16,
        },
    )
    .unwrap();
    let base = base();
    let layers = build_teletext_glyph_layers(&base, &spatial, 128).unwrap();
    let charset = resolve_teletext_charset_execution(&base, &spatial, &layers, 128).unwrap();
    resolve_teletext_glyph_intent(&charset, charset.records().len()).unwrap()
}

fn replay_collection_budget() -> TeletextEnhancementCollectionBudget {
    TeletextEnhancementCollectionBudget {
        max_commands: 128,
        max_drcs_references: 128,
    }
}

fn replay(
    service: &TeletextSemanticService,
    program: &TeletextEnhancementProgram,
    catalog: &TeletextEnhancementObjectCatalog,
    intent: &TeletextGlyphIntent,
) -> Result<TeletextDrcsExecution, TeletextDrcsProgramExecutionError> {
    resolve_teletext_drcs_program(
        service,
        program,
        catalog,
        intent,
        replay_collection_budget(),
        TeletextDrcsExecutionBudget {
            max_candidates: intent.records().len(),
            max_references: 128,
        },
    )
}

#[test]
fn supplied_program_drcs_equals_original_execution_without_changing_old_result() {
    let service = service(draft(
        vec![
            set(1, 8),
            drcs(8, TeletextDrcsTableScope::Normal, 7),
            drcs(9, TeletextDrcsTableScope::Global, 47),
        ],
        vec![],
        tables(NORMAL),
    ));
    let input = intent(&service);
    let (_, program) = service.effective_program(owner()).unwrap();
    let original = resolve(&service, &input);
    assert_eq!(
        replay(&service, &program, service.objects(), &input).unwrap(),
        original
    );
    assert_eq!(resolve(&service, &input), original);
}

#[test]
fn newly_supplied_carrier_drcs_resolves_both_original_service_bitmaps_and_losing_reference() {
    let service = service(draft(vec![], vec![], tables(NORMAL)));
    let before = service.clone();
    let catalog = replay_catalog(vec![
        drcs(0, TeletextDrcsTableScope::Normal, 7),
        drcs(0, TeletextDrcsTableScope::Global, 47),
    ]);
    let program = TeletextEnhancementProgram::new(vec![
        set(1, 8),
        invoke(local(0xffff), TeletextObjectType::Passive, None),
    ])
    .unwrap();
    let input = replay_intent(&program, &catalog);
    assert!(service.objects().definition(local(0xffff)).is_none());
    assert!(service
        .collected(owner())
        .unwrap()
        .1
        .object_references()
        .is_empty());
    let result = replay(&service, &program, &catalog, &input).unwrap();
    assert_eq!(result.records().len(), 2);
    for (index, record) in result.records().iter().enumerate() {
        assert_eq!(
            record.candidate().source(),
            Some(TeletextEnhancementSource {
                object: Some(local(0xffff)),
                command: index
            })
        );
        assert_eq!(
            record.candidate().root_source(),
            Some(TeletextSpatialRootSource::Local { command: 1 })
        );
        assert_eq!(record.candidate().position(), (1, 8));
        assert_eq!(record.candidate().invocation().unwrap().get(), 0);
    }
    assert_eq!(
        result.records()[0].resource(),
        identity(TeletextDrcsTableScope::Normal)
    );
    assert_eq!(result.records()[0].bitmap().get(), NORMAL);
    assert_eq!(
        result.records()[1].resource(),
        identity(TeletextDrcsTableScope::Global)
    );
    assert_eq!(result.records()[1].bitmap().get(), GLOBAL);
    assert_eq!(
        resolve_teletext_drcs_execution(
            &service,
            &input,
            TeletextDrcsExecutionBudget {
                max_candidates: input.records().len(),
                max_references: 2
            }
        )
        .unwrap_err()
        .reason,
        TeletextDrcsExecutionFailure::CandidateMismatch
    );
    assert_eq!(service, before);
}

#[test]
fn shifted_main_drcs_ordinal_uses_supplied_program_and_rejects_stale_original_ordinal() {
    let service = service(draft(
        vec![set(1, 8), drcs(8, TeletextDrcsTableScope::Normal, 7)],
        vec![],
        tables(NORMAL),
    ));
    let original = intent(&service);
    let program = TeletextEnhancementProgram::new(vec![
        set(1, 8),
        TeletextEnhancementCommand::AtColumn {
            column: 8,
            operation: TeletextColumnOperation::G2 {
                code: TeletextGlyphCode::new(0x23).unwrap(),
            },
        },
        drcs(8, TeletextDrcsTableScope::Normal, 7),
    ])
    .unwrap();
    let input = replay_intent(&program, service.objects());
    let result = replay(&service, &program, service.objects(), &input).unwrap();
    assert_eq!(
        result.records()[0].candidate().source(),
        Some(TeletextEnhancementSource {
            object: None,
            command: 2
        })
    );
    assert_eq!(result.records()[0].bitmap().get(), NORMAL);
    let error = replay(&service, &program, service.objects(), &original).unwrap_err();
    assert_eq!(
        error.location(),
        Some(TeletextEnhancementSource {
            object: None,
            command: 1
        })
    );
    assert!(
        matches!(error,TeletextDrcsProgramExecutionError::Execution(error)
        if error.reason == TeletextDrcsExecutionFailure::CandidateMismatch)
    );
}

#[test]
fn stale_carrier_body_missing_target_and_foreign_exact_owner_are_rejected_with_source() {
    let original_service = service(draft(vec![], vec![], tables(NORMAL)));
    let program = TeletextEnhancementProgram::new(vec![
        set(1, 8),
        invoke(local(0xffff), TeletextObjectType::Passive, None),
    ])
    .unwrap();
    let catalog = replay_catalog(vec![drcs(0, TeletextDrcsTableScope::Normal, 7)]);
    let input = replay_intent(&program, &catalog);
    let stale = replay_catalog(vec![drcs(0, TeletextDrcsTableScope::Normal, 8)]);
    let error = replay(&original_service, &program, &stale, &input).unwrap_err();
    assert_eq!(error.owner(), owner());
    assert_eq!(
        error.location(),
        Some(TeletextEnhancementSource {
            object: Some(local(0xffff)),
            command: 0
        })
    );
    assert!(
        matches!(error,TeletextDrcsProgramExecutionError::Execution(error)
        if error.reason == TeletextDrcsExecutionFailure::CandidateMismatch
        && error.root_source == Some(TeletextSpatialRootSource::Local {command:1})
        && error.invocation.unwrap().get() == 0)
    );
    let missing = TeletextEnhancementObjectCatalog::new(
        vec![],
        TeletextObjectCatalogBudget {
            max_objects: 0,
            max_commands: 0,
        },
    )
    .unwrap();
    let error = replay(&original_service, &program, &missing, &input).unwrap_err();
    assert_eq!(
        error.location(),
        Some(TeletextEnhancementSource {
            object: None,
            command: 1
        })
    );
    assert!(
        matches!(error,TeletextDrcsProgramExecutionError::Collection(error)
        if error.reason == TeletextEnhancementCollectionFailure::Invocation(TeletextObjectInvocationError::Missing {target:local(0xffff)}))
    );
    let mut foreign = draft(vec![], vec![], tables(NORMAL));
    foreign.base.pages[0].address.subcode = TeletextSubcode::new(0x1235).unwrap();
    foreign.pages[0].address = foreign.base.pages[0].address;
    let error = replay(&service(foreign), &program, &catalog, &input).unwrap_err();
    assert!(
        matches!(error,TeletextDrcsProgramExecutionError::Execution(error)
        if error.reason == TeletextDrcsExecutionFailure::MissingPage)
    );
}

#[test]
fn entire_supplied_program_dependencies_are_validated_even_if_intent_has_no_drcs_record() {
    let service = service(draft(vec![], vec![], tables(NORMAL)));
    let input = intent(&service);
    let program = TeletextEnhancementProgram::new(vec![
        set(1, 8),
        drcs(8, TeletextDrcsTableScope::Normal, 9),
    ])
    .unwrap();
    let error = replay(&service, &program, service.objects(), &input).unwrap_err();
    assert_eq!(error.owner(), owner());
    assert_eq!(
        error.location(),
        Some(TeletextEnhancementSource {
            object: None,
            command: 1
        })
    );
    assert!(
        matches!(error,TeletextDrcsProgramExecutionError::Dependencies {error,..}
        if matches!(error.error,TeletextDrcsResourceError::MissingGlyph {slot,..} if slot.get() == 9))
    );
}

#[test]
fn replay_raw_budgets_precede_collection_and_preserve_all_inputs() {
    let service = service(draft(vec![], vec![], tables(NORMAL)));
    let program = TeletextEnhancementProgram::new(vec![
        set(1, 8),
        invoke(local(0xffff), TeletextObjectType::Passive, None),
    ])
    .unwrap();
    let catalog = replay_catalog(vec![
        drcs(0, TeletextDrcsTableScope::Normal, 7),
        drcs(0, TeletextDrcsTableScope::Global, 47),
    ]);
    let input = replay_intent(&program, &catalog);
    let before = (
        service.clone(),
        program.clone(),
        catalog.clone(),
        input.clone(),
    );
    let count = input.records().len();
    assert_eq!(count, 994);
    let empty_budget = TeletextEnhancementCollectionBudget {
        max_commands: 0,
        max_drcs_references: 0,
    };
    let error = resolve_teletext_drcs_program(
        &service,
        &program,
        &catalog,
        &input,
        empty_budget,
        TeletextDrcsExecutionBudget {
            max_candidates: count - 1,
            max_references: 2,
        },
    )
    .unwrap_err();
    assert!(
        matches!(error,TeletextDrcsProgramExecutionError::Execution(error)
        if error.reason == TeletextDrcsExecutionFailure::CandidateBudget {count,maximum:993})
    );
    let error = resolve_teletext_drcs_program(
        &service,
        &program,
        &catalog,
        &input,
        empty_budget,
        TeletextDrcsExecutionBudget {
            max_candidates: count,
            max_references: 1,
        },
    )
    .unwrap_err();
    assert!(
        matches!(error,TeletextDrcsProgramExecutionError::Execution(error)
        if error.reason == TeletextDrcsExecutionFailure::ReferenceBudget {count:2,maximum:1}
        && error.location == Some(TeletextEnhancementSource {object:Some(local(0xffff)),command:1}))
    );
    assert_eq!((service, program, catalog, input), before);
}

#[test]
fn replay_exact_collection_budget_and_one_short_failure_preserve_original_body_blame() {
    let service = service(draft(vec![], vec![], tables(NORMAL)));
    let program = TeletextEnhancementProgram::new(vec![
        set(1, 8),
        invoke(local(0xffff), TeletextObjectType::Passive, None),
    ])
    .unwrap();
    let catalog = replay_catalog(vec![
        drcs(0, TeletextDrcsTableScope::Normal, 7),
        drcs(0, TeletextDrcsTableScope::Global, 47),
    ]);
    let input = replay_intent(&program, &catalog);
    let before = (
        service.clone(),
        program.clone(),
        catalog.clone(),
        input.clone(),
    );
    let count = input.records().len();
    let exact = resolve_teletext_drcs_program(
        &service,
        &program,
        &catalog,
        &input,
        TeletextEnhancementCollectionBudget {
            max_commands: 4,
            max_drcs_references: 2,
        },
        TeletextDrcsExecutionBudget {
            max_candidates: count,
            max_references: 2,
        },
    )
    .unwrap();
    assert_eq!(exact.records().len(), 2);
    let error = resolve_teletext_drcs_program(
        &service,
        &program,
        &catalog,
        &input,
        TeletextEnhancementCollectionBudget {
            max_commands: 3,
            max_drcs_references: 2,
        },
        TeletextDrcsExecutionBudget {
            max_candidates: count,
            max_references: 2,
        },
    )
    .unwrap_err();
    assert_eq!(
        error.location(),
        Some(TeletextEnhancementSource {
            object: Some(local(0xffff)),
            command: 1
        })
    );
    assert!(
        matches!(error,TeletextDrcsProgramExecutionError::Collection(error)
        if error.reason == TeletextEnhancementCollectionFailure::CommandBudget {maximum:3})
    );
    assert_eq!((service, program, catalog, input), before);
}
