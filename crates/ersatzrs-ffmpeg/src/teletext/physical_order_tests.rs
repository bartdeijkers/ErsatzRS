//! Literal physical C.2 evidence; this is not cross-object timing authority.

use super::*;
use crate::teletext::{allocate_teletext_resource_objects, compose_teletext_main_program};
use ersatzrs_domain::*;

fn owner() -> TeletextPageAddress {
    TeletextPageAddress {
        page: TeletextPageNumber::new(200).unwrap(),
        subcode: TeletextSubcode::new(0x1234).unwrap(),
    }
}
fn position(row: u8, column: u8) -> TeletextEnhancementCommand {
    TeletextEnhancementCommand::SetPosition {
        position: TeletextEnhancementPosition::new(row, column).unwrap(),
    }
}
fn foreground(column: u8, value: u8) -> TeletextEnhancementCommand {
    TeletextEnhancementCommand::AtColumn {
        column,
        operation: TeletextColumnOperation::Foreground {
            colour: TeletextColourReference::new(value).unwrap(),
        },
    }
}
fn glyph(column: u8) -> TeletextEnhancementCommand {
    TeletextEnhancementCommand::AtColumn {
        column,
        operation: TeletextColumnOperation::G0 {
            code: TeletextGlyphCode::new(b'A').unwrap(),
        },
    }
}
fn target() -> TeletextObjectIdentity {
    TeletextObjectIdentity::local(owner(), TeletextObjectId::new(0xffff))
}
fn invoke(kind: TeletextObjectType) -> TeletextEnhancementCommand {
    TeletextEnhancementCommand::InvokeObject {
        target: target(),
        object_type: kind,
        offset: None,
    }
}
fn service(
    main: Vec<TeletextEnhancementCommand>,
    object: Option<(TeletextObjectType, Vec<TeletextEnhancementCommand>)>,
) -> TeletextSemanticService {
    let mut page = crate::teletext::transport_tests::draft(200, 0x1234, false);
    page.rows.clear();
    let language = page.language.clone();
    let objects = object
        .into_iter()
        .map(
            |(object_type, commands)| TeletextEnhancementObjectDefinition {
                identity: target(),
                object_type,
                program: TeletextEnhancementProgram::new(commands).unwrap(),
            },
        )
        .collect();
    TeletextSemanticService::new(
        TeletextResolvedServiceDraft {
            base: TeletextPageSetDraft {
                language,
                initial_page: owner().page,
                pages: vec![page],
            },
            pages: vec![TeletextResolvedPageEnhancementDraft {
                address: owner(),
                presentation: None,
                compositional_links: None,
                local_program: Some(TeletextEnhancementProgram::new(main).unwrap()),
                resources: TeletextResolvedPageResourceSelection {
                    objects: TeletextResolvedObjectPageLinks {
                        public: None,
                        global: None,
                    },
                    drcs: TeletextDrcsPageLinks::unlinked(),
                    defaults: [None; 4],
                },
            }],
            magazines: vec![],
            objects,
            drcs: vec![],
        },
        TeletextServiceBudget {
            max_records: 32,
            max_objects: 4,
            max_structure_items: 4096,
            max_base_content_bytes: 1024,
            max_commands: 2048,
            max_collected_commands: 4096,
            max_collected_drcs_references: 0,
        },
    )
    .unwrap()
}
fn budget() -> TeletextMainCompositionBudget {
    TeletextMainCompositionBudget {
        max_input_commands: 1024,
        max_output_commands: 1024,
        collection: TeletextEnhancementCollectionBudget {
            max_commands: 2048,
            max_drcs_references: 0,
        },
    }
}
fn compile(service: &TeletextSemanticService) -> EncodedTeletextComposedMainProgram {
    let allocations = allocate_teletext_resource_objects(service.objects(), &[], 4).unwrap();
    compile_teletext_composed_main_program(
        service,
        owner(),
        &allocations,
        budget(),
        TeletextObjectCatalogBudget {
            max_objects: 4,
            max_commands: 1024,
        },
    )
    .unwrap()
}
// Independent standard Hamming data positions; assert parity before reading.
fn word(result: &EncodedTeletextComposedMainProgram, index: usize) -> u32 {
    let body = result.encoded().packet_bodies()[index / 13];
    let offset = 1 + (index % 13) * 3;
    let bits = u32::from_le_bytes([body[offset], body[offset + 1], body[offset + 2], 0]);
    assert_eq!(bits.count_ones() % 2, 1);
    for parity in [1_u32, 2, 4, 8, 16] {
        assert_eq!(
            (1_u32..24)
                .filter(|position| position & parity != 0)
                .map(|position| (bits >> (position - 1)) & 1)
                .sum::<u32>()
                % 2,
            1
        );
    }
    [
        3, 5, 6, 7, 9, 10, 11, 12, 13, 14, 15, 17, 18, 19, 20, 21, 22, 23,
    ]
    .into_iter()
    .enumerate()
    .fold(0, |value, (bit, position)| {
        value | (((bits >> (position - 1)) & 1) << bit)
    })
}
fn assert_pink(result: &EncodedTeletextComposedMainProgram, row: u8, column: u8) {
    let composition = result.composition();
    let field = composition
        .attribute_execution()
        .cell(row, column)
        .unwrap()
        .foreground();
    assert_eq!(
        field.value(),
        Some(TeletextColourReference::new(13).unwrap())
    );
    assert_eq!(field.pending(), None);
    let resolved = composition.display_resolution().cell(row, column).unwrap();
    assert_eq!(
        resolved.attributes().foreground().value(),
        Some(TeletextColourReference::new(13).unwrap())
    );
    assert!(resolved.attributes().foreground().pending().is_none());
    let pixel = composition
        .colour_composition()
        .pixel(
            composition.display_resolution(),
            row,
            column,
            true,
            TeletextColourPixelOptions {
                reveal_concealed: false,
                flash_phase: TeletextFlashPhase::Original,
            },
        )
        .unwrap();
    assert!(matches!(pixel, TeletextColourPixel::Rgb(rgb) if rgb.get() == [7,0,7]));
}
fn evidence_positions(
    composition: &TeletextMainComposition,
) -> Vec<TeletextAttributeTransmissionPosition> {
    composition
        .spatial_trace()
        .events()
        .iter()
        .enumerate()
        .map(|(ordinal, event)| TeletextAttributeTransmissionPosition {
            event_ordinal: ordinal,
            source: event.source(),
            invocation: event.invocation(),
            word: ordinal,
        })
        .collect()
}

#[test]
fn actual_local_packet_boundary_settles_r3_while_standalone_stays_pending() {
    let mut main = vec![
        TeletextEnhancementCommand::SetScreenColour {
            colour: TeletextColourReference::BLACK
        };
        11
    ];
    main.extend([
        position(1, 18),
        foreground(18, 4),
        foreground(18, 13),
        glyph(18),
    ]);
    let service = service(main, None);
    let original = service.clone();
    let standalone = compose_teletext_main_program(&service, owner(), budget()).unwrap();
    assert!(standalone
        .attribute_execution()
        .cell(1, 18)
        .unwrap()
        .foreground()
        .pending()
        .is_some());
    assert!(standalone
        .display_resolution()
        .cell(1, 18)
        .unwrap()
        .attributes()
        .foreground()
        .pending()
        .is_some());
    let result = compile(&service);
    assert_eq!(word(&result, 12), 0x2012); // blue: designation0/triplet12
    assert_eq!(word(&result, 13), 0x6812); // pink: designation1/triplet0
    let positions = result.encoded().command_positions();
    assert_eq!(positions[12].semantic_word(), 12);
    assert_eq!(positions[13].semantic_word(), 13);
    assert_pink(&result, 1, 18);
    assert_eq!(service, original);
}

#[test]
fn one_adaptive_body_uses_final_marker_and_exact_instance_order() {
    let service = service(
        vec![position(1, 0), invoke(TeletextObjectType::Adaptive)],
        Some((
            TeletextObjectType::Adaptive,
            vec![foreground(5, 4), foreground(5, 13), glyph(5)],
        )),
    );
    let standalone = compose_teletext_main_program(&service, owner(), budget()).unwrap();
    assert!(standalone
        .attribute_execution()
        .cell(1, 5)
        .unwrap()
        .foreground()
        .pending()
        .is_some());
    let result = compile(&service);
    assert_eq!(
        result.encoded().definition_positions()[0].absolute_index(),
        3
    );
    assert_eq!(word(&result, 3), 0x28 | (0x16 << 6) | (3 << 11));
    assert_eq!(word(&result, 4), 5 | (4 << 11));
    assert_eq!(word(&result, 5), 5 | (13 << 11));
    assert_pink(&result, 1, 5);
    let source = result
        .composition()
        .attribute_execution()
        .cell(1, 5)
        .unwrap()
        .foreground()
        .source()
        .unwrap();
    assert!(
        matches!(source, TeletextAttributeSource::Enhancement { location: TeletextEnhancementSource { object: Some(identity), command: 1 }, invocation: Some(id), .. }
        if identity == target() && id.get()==0)
    );
}

#[test]
fn compatibility_words_shift_evidence_without_changing_original_sources() {
    let main = vec![
        position(1, 8),
        invoke(TeletextObjectType::Passive),
        TeletextEnhancementCommand::AtColumn {
            column: 9,
            operation: TeletextColumnOperation::G2 {
                code: TeletextGlyphCode::new(0x23).unwrap(),
            },
        },
        foreground(10, 4),
        foreground(10, 13),
        glyph(10),
    ];
    let service = service(main, Some((TeletextObjectType::Passive, vec![])));
    let result = compile(&service);
    let positions = result.encoded().command_positions();
    assert_eq!(positions[2].first_word(), 2);
    assert_eq!(positions[2].semantic_word(), 3); // inserted compatibility row1
    assert_eq!(word(&result, 2), 0x29 | (4 << 6) | (9 << 11));
    assert_eq!(word(&result, 3), 9 | (0x0f << 6) | (0x23 << 11));
    assert_eq!(positions[3].source().command, 3);
    assert_eq!(positions[3].semantic_word(), 4);
    assert_eq!(positions[4].semantic_word(), 5);
    assert_eq!(word(&result, 4), 0x0a | (4 << 11));
    assert_eq!(word(&result, 5), 0x0a | (13 << 11));
    assert_pink(&result, 1, 10);
}

#[test]
fn older_cross_instance_unknown_is_not_erased_by_later_local_pair() {
    let service = service(
        vec![
            position(1, 18),
            foreground(18, 4),
            invoke(TeletextObjectType::Active),
            foreground(18, 2),
            glyph(18),
        ],
        Some((TeletextObjectType::Active, vec![foreground(0, 13)])),
    );
    let result = compile(&service);
    let attrs = result.composition().attribute_execution();
    let field = attrs.cell(1, 18).unwrap().foreground();
    assert_eq!(field.value(), None);
    let last = &attrs.pending()[field.pending().unwrap()];
    assert!(last.previous().is_some());
    assert!(result
        .composition()
        .display_resolution()
        .cell(1, 18)
        .unwrap()
        .attributes()
        .foreground()
        .pending()
        .is_some());
    let evidence = attrs.transmission_evidence().unwrap();
    let cross = &attrs.pending()[last.previous().unwrap()];
    assert_eq!(
        evidence.compare(
            cross.first_source().unwrap(),
            cross.second_source().unwrap()
        ),
        None
    );
}

#[test]
fn sparse_evidence_does_not_guess_missing_word_order() {
    let service = service(
        vec![
            position(1, 8),
            foreground(8, 4),
            foreground(8, 13),
            glyph(8),
        ],
        None,
    );
    let standalone = compose_teletext_main_program(&service, owner(), budget()).unwrap();
    let mut positions = evidence_positions(&standalone);
    positions.retain(|position| position.event_ordinal != 1);
    let evidence =
        TeletextAttributeTransmissionEvidence::new(standalone.spatial_trace(), &positions, 4)
            .unwrap();
    let attrs = resolve_teletext_attribute_execution_with_transmission(
        standalone.base_display(),
        standalone.spatial_trace(),
        standalone.glyph_layers(),
        standalone.colour_context().row_colours(),
        4,
        &evidence,
    )
    .unwrap();
    assert_eq!(attrs.cell(1, 8).unwrap().foreground().value(), None);
    assert!(attrs.cell(1, 8).unwrap().foreground().pending().is_some());
}

#[test]
fn evidence_is_bounded_unique_and_cannot_cross_changed_same_owner_trace() {
    let service_a = service(
        vec![
            position(1, 8),
            foreground(8, 4),
            foreground(8, 13),
            glyph(8),
        ],
        None,
    );
    let first = compose_teletext_main_program(&service_a, owner(), budget()).unwrap();
    let positions = evidence_positions(&first);
    assert_eq!(
        TeletextAttributeTransmissionEvidence::new(first.spatial_trace(), &positions, 3)
            .unwrap_err(),
        TeletextAttributeTransmissionError::Budget { maximum: 3 }
    );
    let mut duplicate = positions.clone();
    duplicate[2].word = duplicate[1].word;
    assert_eq!(
        TeletextAttributeTransmissionEvidence::new(first.spatial_trace(), &duplicate, 4)
            .unwrap_err(),
        TeletextAttributeTransmissionError::Correspondence { event: 2 }
    );
    duplicate = positions.clone();
    duplicate[1].source.command = 2;
    assert_eq!(
        TeletextAttributeTransmissionEvidence::new(first.spatial_trace(), &duplicate, 4)
            .unwrap_err(),
        TeletextAttributeTransmissionError::Correspondence { event: 1 }
    );
    let evidence =
        TeletextAttributeTransmissionEvidence::new(first.spatial_trace(), &positions, 4).unwrap();
    let service_b = service(
        vec![
            position(1, 8),
            foreground(8, 2),
            foreground(8, 13),
            glyph(8),
        ],
        None,
    );
    let second = compose_teletext_main_program(&service_b, owner(), budget()).unwrap();
    assert!(!evidence.matches(second.spatial_trace()));
    let error = resolve_teletext_attribute_execution_with_transmission(
        second.base_display(),
        second.spatial_trace(),
        second.glyph_layers(),
        second.colour_context().row_colours(),
        4,
        &evidence,
    )
    .unwrap_err();
    assert_eq!(
        error.reason,
        TeletextAttributeExecutionFailure::TransmissionMismatch
    );
    assert_eq!(evidence_positions(&first), positions);
}
