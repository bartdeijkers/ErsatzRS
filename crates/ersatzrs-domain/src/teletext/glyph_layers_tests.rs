//! Original diagnostic glyph candidates, not a rendered or Level 1.5 result.

use super::*;

fn owner() -> TeletextPageAddress {
    TeletextPageAddress {
        page: TeletextPageNumber::new(200).unwrap(),
        subcode: TeletextSubcode::new(0x1234).unwrap(),
    }
}

fn position(row: u8, column: u8) -> TeletextEnhancementPosition {
    TeletextEnhancementPosition::new(row, column).unwrap()
}

fn base(owner: TeletextPageAddress) -> TeletextBaseDisplay {
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
                    background: TeletextColourReference::new(0).unwrap(),
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
                    let position = position(row, column);
                    let authored = (row == 1 && column == 8).then(|| {
                        TeletextBaseAuthoredGlyph::new(
                            position,
                            8,
                            TeletextStyle::default(),
                            TeletextBaseAuthoredContent::Text { character: 'é' },
                        )
                        .unwrap()
                    });
                    let code = if authored.is_some() { b'e' } else { b' ' };
                    let kind = if row == 0 && column < 8 {
                        TeletextBaseDisplayCellKind::HeaderOwned
                    } else if authored.is_some() {
                        TeletextBaseDisplayCellKind::Glyph
                    } else {
                        TeletextBaseDisplayCellKind::Blank
                    };
                    TeletextBaseDisplayCell::new(position, code, kind, authored, attributes)
                        .unwrap()
                })
                .collect();
            cells.try_into().unwrap()
        })
        .collect();
    // Convert a boxed slice in place: no 1000-cell temporary stack array.
    let rows: Box<[[TeletextBaseDisplayCell; 40]; 25]> =
        rows.into_boxed_slice().try_into().unwrap();
    TeletextBaseDisplay::new(
        owner,
        TeletextPageCharsetDesignation::new(1).unwrap(),
        TeletextNationalOption::new(1).unwrap(),
        None,
        rows,
    )
    .unwrap()
}

fn local(id: u16) -> TeletextObjectIdentity {
    TeletextObjectIdentity::local(owner(), TeletextObjectId::new(id))
}

fn resource(kind: TeletextResourceKind, page: u8, subpage: u8) -> TeletextResourceIdentity {
    TeletextResourceIdentity::new(
        kind,
        TeletextRawDataPageNumber::new(2, page).unwrap(),
        TeletextResourceSubpageIndex::new(subpage).unwrap(),
    )
    .unwrap()
}

fn at(column: u8, operation: TeletextColumnOperation) -> TeletextEnhancementCommand {
    TeletextEnhancementCommand::AtColumn { column, operation }
}

fn glyph(column: u8, code: u8) -> TeletextEnhancementCommand {
    at(
        column,
        TeletextColumnOperation::G0 {
            code: TeletextGlyphCode::new(code).unwrap(),
        },
    )
}

fn set(row: u8, column: u8) -> TeletextEnhancementCommand {
    TeletextEnhancementCommand::SetPosition {
        position: position(row, column),
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
    with_drcs: bool,
) -> TeletextSemanticService {
    let language = TeletextLanguage::new("nld".into()).unwrap();
    let drcs = if with_drcs {
        vec![TeletextDrcsSubtable::new(
            resource(TeletextResourceKind::Gdrcs, 0xb1, 9),
            vec![TeletextDrcsGlyphDefinition {
                slot: TeletextDrcsGlyphSlot::new(47).unwrap(),
                bitmap: TeletextDrcsBitmap::new([1; 10]).unwrap(),
            }],
        )
        .unwrap()]
    } else {
        vec![]
    };
    TeletextSemanticService::new(
        TeletextResolvedServiceDraft {
            base: TeletextPageSetDraft {
                language: language.clone(),
                initial_page: owner().page,
                pages: vec![TeletextPageDraft {
                    address: owner(),
                    kind: TeletextPageKind::FullPage,
                    language,
                    national_option: TeletextNationalOption::new(1).unwrap(),
                    rows: vec![TeletextRow {
                        row: 1,
                        runs: vec![TeletextRun {
                            column: 8,
                            style: TeletextStyle::default(),
                            content: TeletextRunContent::Text { text: "é".into() },
                        }],
                    }],
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
                        public: None,
                        global: None,
                    },
                    defaults,
                    drcs: TeletextDrcsPageLinks::new(
                        None,
                        with_drcs.then(|| TeletextRawDataPageNumber::new(2, 0xb1).unwrap()),
                    )
                    .unwrap(),
                },
            }],
            magazines: vec![],
            objects,
            drcs,
        },
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

fn spatial(service: &TeletextSemanticService) -> TeletextSpatialTrace {
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

fn layers(service: &TeletextSemanticService) -> TeletextGlyphLayers {
    build_teletext_glyph_layers(&base(owner()), &spatial(service), 128).unwrap()
}

#[test]
fn active_candidate_outranks_fallback_without_losing_original_unicode_intent() {
    let target = local(0xffff);
    let service = service(
        Some(vec![
            set(1, 8),
            invoke(target, TeletextObjectType::Active, None),
        ]),
        vec![definition(
            target,
            TeletextObjectType::Active,
            vec![glyph(0, b'A')],
        )],
        [None; 4],
        false,
    );
    let layers = layers(&service);
    let candidates = layers.candidates_at(1, 8);
    assert_eq!(candidates.len(), 2);
    let fallback = candidates[0];
    assert_eq!(fallback.priority(), TeletextGlyphPriority::Base);
    assert_eq!(fallback.operation(), None);
    assert_eq!(fallback.source(), None);
    assert_eq!(fallback.invocation(), None);
    let original = fallback.base().unwrap();
    assert_eq!(original.code(), b'e');
    assert_eq!(original.kind(), TeletextBaseDisplayCellKind::Glyph);
    assert_eq!(
        original.authored().unwrap().content(),
        TeletextBaseAuthoredContent::Text { character: 'é' }
    );
    assert_eq!(
        original.attributes().sources().foreground,
        TeletextBaseAttributeSource::RowDefault {
            row: 1,
            start_column: 0
        }
    );
    let winner = layers.highest_priority_candidate(1, 8).unwrap();
    assert_eq!(winner.priority(), TeletextGlyphPriority::Active);
    assert_eq!(
        winner.operation(),
        Some(TeletextColumnOperation::G0 {
            code: TeletextGlyphCode::new(b'A').unwrap()
        })
    );
    assert_eq!(
        winner.source(),
        Some(TeletextEnhancementSource {
            object: Some(target),
            command: 0
        })
    );
    assert_eq!(
        winner.root_source(),
        Some(TeletextSpatialRootSource::Local { command: 1 })
    );
    assert_eq!(
        winner.invocation().map(TeletextSpatialInvocationId::get),
        Some(0)
    );
}

#[test]
fn all_five_original_priority_tiers_remain_as_diagnostic_candidates() {
    let active = local(0xffff);
    let adaptive = local(0x0100);
    let passive = local(0);
    let service = service(
        Some(vec![
            set(1, 8),
            invoke(active, TeletextObjectType::Active, None),
            glyph(8, b'L'),
            invoke(adaptive, TeletextObjectType::Adaptive, None),
            invoke(passive, TeletextObjectType::Passive, None),
        ]),
        vec![
            definition(active, TeletextObjectType::Active, vec![glyph(0, b'A')]),
            definition(adaptive, TeletextObjectType::Adaptive, vec![glyph(0, b'D')]),
            definition(passive, TeletextObjectType::Passive, vec![glyph(0, b'P')]),
        ],
        [None; 4],
        false,
    );
    let layers = layers(&service);
    assert_eq!(
        layers
            .candidates_at(1, 8)
            .iter()
            .map(TeletextGlyphCandidate::priority)
            .collect::<Vec<_>>(),
        [
            TeletextGlyphPriority::Base,
            TeletextGlyphPriority::Active,
            TeletextGlyphPriority::Local,
            TeletextGlyphPriority::Adaptive,
            TeletextGlyphPriority::Passive
        ]
    );
    assert_eq!(
        layers.highest_priority_candidate(1, 8).unwrap().source(),
        Some(TeletextEnhancementSource {
            object: Some(passive),
            command: 0
        })
    );
    assert_eq!(
        layers.candidates_at(1, 8)[2].source(),
        Some(TeletextEnhancementSource {
            object: None,
            command: 2
        })
    );
}

#[test]
fn latest_repeated_passive_instance_wins_without_erasing_losers_or_call_origins() {
    let target = local(0xffff);
    let service = service(
        Some(vec![
            set(1, 6),
            invoke(target, TeletextObjectType::Passive, Some((0, 2))),
            set(1, 8),
            invoke(target, TeletextObjectType::Passive, None),
        ]),
        vec![definition(
            target,
            TeletextObjectType::Passive,
            vec![glyph(0, b'A')],
        )],
        [None; 4],
        false,
    );
    let original = spatial(&service);
    let layers = build_teletext_glyph_layers(&base(owner()), &original, 2).unwrap();
    let candidates = layers.candidates_at(1, 8);
    assert_eq!(candidates.len(), 3);
    assert_eq!(
        candidates[1]
            .invocation()
            .map(TeletextSpatialInvocationId::get),
        Some(0)
    );
    assert_eq!(
        candidates[2]
            .invocation()
            .map(TeletextSpatialInvocationId::get),
        Some(1)
    );
    assert_eq!(
        layers.highest_priority_candidate(1, 8),
        Some(&candidates[2])
    );
    assert_eq!(
        (
            original.invocations()[0].caller_position().row(),
            original.invocations()[0].caller_position().column()
        ),
        (1, 6)
    );
    assert_eq!(
        (
            original.invocations()[1].caller_position().row(),
            original.invocations()[1].caller_position().column()
        ),
        (1, 8)
    );
    for call in original.invocations() {
        assert_eq!((call.origin().row(), call.origin().column()), (1, 8));
        assert_eq!(call.target(), target);
    }
}

#[test]
fn attribute_charset_position_and_empty_invocation_commands_are_not_glyph_candidates() {
    let target = local(0xffff);
    let colour = TeletextColourReference::new(13).unwrap();
    let main = vec![
        TeletextEnhancementCommand::SetScreenColour { colour },
        TeletextEnhancementCommand::SetRowColour {
            row: 1,
            colour,
            following_rows: true,
        },
        set(1, 8),
        at(8, TeletextColumnOperation::Foreground { colour }),
        at(8, TeletextColumnOperation::Background { colour }),
        at(
            8,
            TeletextColumnOperation::DesignateCharset {
                designation: TeletextEnhancementCharsetDesignation::new(0x20).unwrap(),
            },
        ),
        at(
            8,
            TeletextColumnOperation::Attributes {
                double_height: true,
                double_width: true,
                boxed: true,
                conceal: true,
                invert: true,
                underline_or_separated_mosaics: true,
            },
        ),
        at(
            8,
            TeletextColumnOperation::Flash {
                mode: TeletextFlashMode::AdjacentClut,
                rate: TeletextFlashRate::Fast2HzPhase3,
            },
        ),
        invoke(target, TeletextObjectType::Passive, None),
    ];
    let service = service(
        Some(main),
        vec![definition(target, TeletextObjectType::Passive, vec![])],
        [None; 4],
        false,
    );
    let original = spatial(&service);
    assert_eq!(
        (original.events().len(), original.invocations().len()),
        (9, 1)
    );
    let layers = build_teletext_glyph_layers(&base(owner()), &original, 0).unwrap();
    assert_eq!(layers.candidates().len(), 992);
    assert!(layers
        .candidates()
        .iter()
        .all(|candidate| candidate.priority() == TeletextGlyphPriority::Base));
}

#[test]
fn exact_glyph_operations_retain_repertoire_compatibility_and_full_drcs_reference() {
    let reference = TeletextDrcsGlyphReference {
        table: TeletextDrcsTableScope::Global,
        subtable: TeletextDrcsSubtableIndex::new(9).unwrap(),
        slot: TeletextDrcsGlyphSlot::new(47).unwrap(),
    };
    let operations = [
        TeletextColumnOperation::G0 {
            code: TeletextGlyphCode::new(0x7f).unwrap(),
        },
        TeletextColumnOperation::G1 {
            mosaic: TeletextMosaic::new(0x21).unwrap(),
        },
        TeletextColumnOperation::G2 {
            code: TeletextGlyphCode::new(0x23).unwrap(),
        },
        TeletextColumnOperation::G3 {
            code: TeletextGlyphCode::new(0x40).unwrap(),
            compatibility: TeletextG3Compatibility::Level15,
        },
        TeletextColumnOperation::G3 {
            code: TeletextGlyphCode::new(0x41).unwrap(),
            compatibility: TeletextG3Compatibility::Level25,
        },
        TeletextColumnOperation::G0Diacritic {
            code: TeletextGlyphCode::new(b'e').unwrap(),
            diacritic: TeletextDiacriticIndex::new(2).unwrap(),
        },
        TeletextColumnOperation::Drcs { reference },
    ];
    let main = std::iter::once(set(1, 8))
        .chain(
            (8_u8..)
                .zip(operations)
                .map(|(column, operation)| at(column, operation)),
        )
        .collect();
    let service = service(Some(main), vec![], [None; 4], true);
    let layers = layers(&service);
    for (column, operation) in (8_u16..).zip(operations) {
        let candidates = layers.candidates_at(1, column);
        assert_eq!(candidates.len(), 2);
        let enhanced = candidates[1];
        assert_eq!(enhanced.operation(), Some(operation));
        assert_eq!(enhanced.priority(), TeletextGlyphPriority::Local);
        assert_eq!(enhanced.base(), None);
        assert_eq!(enhanced.position(), (1, column));
        assert_eq!(enhanced.invocation(), None);
    }
}

#[test]
fn sparse_default_roots_keep_original_global_and_public_resource_identity_and_types() {
    let global = TeletextObjectIdentity::global(
        resource(TeletextResourceKind::Gpop, 0xa2, 12),
        TeletextObjectId::new(0xffff),
    )
    .unwrap();
    let public = TeletextObjectIdentity::public(
        resource(TeletextResourceKind::Pop, 0xa0, 15),
        TeletextObjectId::new(0x0100),
    )
    .unwrap();
    let defaults = [
        Some(TeletextDefaultObjectSelection {
            target: global,
            object_type: TeletextObjectType::Active,
        }),
        None,
        None,
        Some(TeletextDefaultObjectSelection {
            target: public,
            object_type: TeletextObjectType::Passive,
        }),
    ];
    let service = service(
        None,
        vec![
            definition(
                global,
                TeletextObjectType::Active,
                vec![set(1, 8), glyph(8, b'A')],
            ),
            definition(
                public,
                TeletextObjectType::Passive,
                vec![set(1, 8), glyph(8, b'P')],
            ),
        ],
        defaults,
        false,
    );
    let layers = layers(&service);
    let candidates = layers.candidates_at(1, 8);
    assert_eq!(candidates.len(), 3);
    assert_eq!(candidates[1].priority(), TeletextGlyphPriority::Active);
    assert_eq!(
        candidates[1].source(),
        Some(TeletextEnhancementSource {
            object: Some(global),
            command: 1
        })
    );
    assert_eq!(
        candidates[1].root_source(),
        Some(TeletextSpatialRootSource::DefaultSlot { slot: 0 })
    );
    assert_eq!(candidates[2].priority(), TeletextGlyphPriority::Passive);
    assert_eq!(
        candidates[2].source(),
        Some(TeletextEnhancementSource {
            object: Some(public),
            command: 1
        })
    );
    assert_eq!(
        candidates[2].root_source(),
        Some(TeletextSpatialRootSource::DefaultSlot { slot: 3 })
    );
}

#[test]
fn exact_candidate_budget_rejects_the_second_instance_with_original_blame_without_mutation() {
    let target = local(0xffff);
    let service = service(
        Some(vec![
            set(1, 8),
            invoke(target, TeletextObjectType::Passive, None),
            invoke(target, TeletextObjectType::Passive, None),
        ]),
        vec![definition(
            target,
            TeletextObjectType::Passive,
            vec![glyph(0, b'A')],
        )],
        [None; 4],
        false,
    );
    let original = spatial(&service);
    let before_original = original.clone();
    let base = base(owner());
    let before_base = base.clone();
    let before_service = service.clone();
    let exact = build_teletext_glyph_layers(&base, &original, 2).unwrap();
    assert_eq!(exact.candidates().len(), 994);
    let error = build_teletext_glyph_layers(&base, &original, 1).unwrap_err();
    assert_eq!(error.owner, owner());
    assert_eq!(
        error.reason,
        TeletextGlyphLayersFailure::CandidateBudget { maximum: 1 }
    );
    assert_eq!(
        error.location,
        Some(TeletextEnhancementSource {
            object: Some(target),
            command: 0
        })
    );
    assert_eq!(
        error.root_source,
        Some(TeletextSpatialRootSource::Local { command: 2 })
    );
    assert_eq!(
        error.invocation.map(TeletextSpatialInvocationId::get),
        Some(1)
    );
    assert_eq!(base, before_base);
    assert_eq!(original, before_original);
    assert_eq!(service, before_service);
}

#[test]
fn wide_original_coordinate_is_retained_as_unaddressable_without_clipping() {
    let active = local(0xffff);
    let adaptive = local(0x0100);
    let passive = local(0);
    let service = service(
        Some(vec![
            set(24, 39),
            invoke(active, TeletextObjectType::Active, Some((23, 71))),
        ]),
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
                vec![set(24, 39), glyph(39, b'A')],
            ),
        ],
        [None; 4],
        false,
    );
    let layers = layers(&service);
    let candidates = layers.candidates_at(165, 369);
    assert_eq!(candidates.len(), 1);
    assert_eq!(candidates[0].position(), (165, 369));
    assert_eq!(candidates[0].priority(), TeletextGlyphPriority::Passive);
    assert!(!layers
        .slots()
        .iter()
        .find(|slot| slot.position() == (165, 369))
        .unwrap()
        .logical_addressable());
    assert!(layers.candidates_at(165, 113).is_empty());
}

#[test]
fn base_slots_exclude_only_broadcast_header_and_queries_do_not_invent_cells() {
    let service = service(Some(vec![]), vec![], [None; 4], false);
    let original = spatial(&service);
    let layers = build_teletext_glyph_layers(&base(owner()), &original, 0).unwrap();
    assert_eq!(layers.owner(), owner());
    assert_eq!(
        (layers.slots().len(), layers.candidates().len()),
        (992, 992)
    );
    for row in 0_u16..25 {
        for column in 0_u16..40 {
            let expected = usize::from(row != 0 || column >= 8);
            assert_eq!(layers.candidates_at(row, column).len(), expected);
        }
    }
    assert!(layers.candidates_at(0, 0).is_empty());
    assert!(layers.highest_priority_candidate(0, 7).is_none());
    assert!(layers.candidates_at(25, 0).is_empty());
    assert!(layers.highest_priority_candidate(1, 40).is_none());
    assert!(layers
        .slots()
        .iter()
        .all(TeletextGlyphSlot::logical_addressable));
}

#[test]
fn another_full_subcode_base_owner_cannot_be_combined_with_the_original_program() {
    let service = service(Some(vec![glyph(1, b'A')]), vec![], [None; 4], false);
    let original = spatial(&service);
    let mut other = owner();
    other.subcode = TeletextSubcode::new(0x1235).unwrap();
    let error = build_teletext_glyph_layers(&base(other), &original, 1).unwrap_err();
    assert_eq!(error.owner, other);
    assert_eq!(
        error.reason,
        TeletextGlyphLayersFailure::OwnerMismatch { original: owner() }
    );
    assert_eq!(error.location, None);
    assert_eq!(error.root_source, None);
    assert_eq!(error.invocation, None);
}

#[test]
fn compatible_and_enhanced_same_cell_writes_all_survive_in_original_event_order() {
    let operations = [
        TeletextColumnOperation::G2 {
            code: TeletextGlyphCode::new(0x23).unwrap(),
        },
        TeletextColumnOperation::G0 {
            code: TeletextGlyphCode::new(b'A').unwrap(),
        },
        TeletextColumnOperation::G1 {
            mosaic: TeletextMosaic::new(0x21).unwrap(),
        },
    ];
    let main = std::iter::once(set(1, 8))
        .chain(operations.into_iter().map(|operation| at(8, operation)))
        .collect();
    let layers = layers(&service(Some(main), vec![], [None; 4], false));
    let candidates = layers.candidates_at(1, 8);
    assert_eq!(candidates.len(), 4);
    for (ordinal, operation) in (1..).zip(operations) {
        assert_eq!(candidates[ordinal].operation(), Some(operation));
        assert_eq!(candidates[ordinal].priority(), TeletextGlyphPriority::Local);
        assert_eq!(candidates[ordinal].event_ordinal(), Some(ordinal));
        assert_eq!(
            candidates[ordinal].source(),
            Some(TeletextEnhancementSource {
                object: None,
                command: ordinal
            })
        );
    }
    // Diagnostic 2.5 rank only: selecting G1 here does not select a 1.5 glyph.
    assert_eq!(
        layers.highest_priority_candidate(1, 8),
        Some(&candidates[3])
    );
}

#[test]
fn header_panel_coordinate_is_retained_but_not_logically_addressable() {
    let target = local(0xffff);
    let service = service(
        Some(vec![
            set(0, 39),
            glyph(39, b'H'),
            invoke(target, TeletextObjectType::Passive, Some((0, 32))),
        ]),
        vec![definition(
            target,
            TeletextObjectType::Passive,
            vec![glyph(0, b'A'), set(1, 0), glyph(0, b'B')],
        )],
        [None; 4],
        false,
    );
    let layers = layers(&service);
    for (position, addressable) in [((0, 39), true), ((0, 71), false), ((1, 71), true)] {
        assert!(!layers.candidates_at(position.0, position.1).is_empty());
        let slot = layers
            .slots()
            .iter()
            .find(|slot| slot.position() == position)
            .unwrap();
        assert_eq!(slot.logical_addressable(), addressable);
    }
    assert_eq!(
        layers.candidates_at(0, 71)[0].priority(),
        TeletextGlyphPriority::Passive
    );
    assert_eq!(layers.candidates_at(0, 71)[0].position(), (0, 71));
}

#[test]
fn g1_alpha_candidate_keeps_original_priority_source_and_distinct_mosaic_operation() {
    let target = local(0xffff);
    let alpha = TeletextColumnOperation::G1Alpha {
        code: TeletextG1AlphaCode::new(0x41).unwrap(),
    };
    let mosaic = TeletextColumnOperation::G1 {
        mosaic: TeletextMosaic::new(0x21).unwrap(),
    };
    let service = service(
        Some(vec![
            set(1, 8),
            at(8, alpha),
            at(9, mosaic),
            invoke(target, TeletextObjectType::Passive, Some((0, 1))),
        ]),
        vec![definition(
            target,
            TeletextObjectType::Passive,
            vec![at(0, alpha)],
        )],
        [None; 4],
        false,
    );
    let before = service.clone();
    let layers = layers(&service);
    let local = layers.highest_priority_candidate(1, 8).unwrap();
    assert_eq!(local.operation(), Some(alpha));
    assert_eq!(local.priority(), TeletextGlyphPriority::Local);
    assert_eq!(
        local.source(),
        Some(TeletextEnhancementSource {
            object: None,
            command: 1
        })
    );
    assert_eq!(
        layers.highest_priority_candidate(1, 9).unwrap().operation(),
        Some(mosaic)
    );
    let passive = layers.highest_priority_candidate(1, 10).unwrap();
    assert_eq!(passive.operation(), Some(alpha));
    assert_eq!(passive.priority(), TeletextGlyphPriority::Passive);
    assert_eq!(
        passive.source(),
        Some(TeletextEnhancementSource {
            object: Some(target),
            command: 0
        })
    );
    assert_eq!(
        passive.root_source(),
        Some(TeletextSpatialRootSource::Local { command: 3 })
    );
    assert_eq!(passive.invocation().unwrap().get(), 0);
    assert_eq!(service, before);
}
