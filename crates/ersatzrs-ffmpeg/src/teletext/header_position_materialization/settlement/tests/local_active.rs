//! Addressable same-instance colour ties, not a cross-emitter ordering rule.
use super::*;
use crate::teletext::{
    authored_service_tests::{declared, transmissions},
    encode_teletext_authored_service, encode_teletext_page, project_encoded_teletext_display,
    EncodedTeletextAuthoredPage, TeletextEncodedDisplayRecord, TeletextEncodingRegion,
};

fn body(first: u8, last: u8) -> Vec<TeletextEnhancementCommand> {
    vec![
        fg(8, first),
        fg(8, last),
        TeletextEnhancementCommand::AtColumn {
            column: 10,
            operation: TeletextColumnOperation::G0 {
                code: TeletextGlyphCode::new(b'B').unwrap(),
            },
        },
    ]
}

pub(super) fn service(first: u8, last: u8) -> TeletextSemanticService {
    fixture_with_active_body(
        |passive, active| vec![active, position(9), passive],
        body(first, last),
    )
}

#[test]
fn complete_factory_restores_main_header_after_local_active_body_row() {
    let mut draft = fixture_draft_with_active_body(
        |passive, active| vec![active, position(8), passive],
        vec![
            TeletextEnhancementCommand::SetPosition {
                position: TeletextEnhancementPosition::new(1, 8).unwrap(),
            },
            fg(8, 3),
            TeletextEnhancementCommand::AtColumn {
                column: 8,
                operation: TeletextColumnOperation::G0 {
                    code: TeletextGlyphCode::new(b'B').unwrap(),
                },
            },
        ],
    );
    draft.base.pages[0].rows = [0, 1]
        .map(|row| TeletextRow {
            row,
            runs: vec![TeletextRun {
                column: 8,
                style: TeletextStyle::default(),
                content: TeletextRunContent::Text { text: "Z".into() },
            }],
        })
        .to_vec();
    let saved = declared(draft);
    let actual = encode_teletext_authored_service(saved.clone(), &transmissions(&saved), &limits())
        .expect("an Active body row must not replace its caller's restored Main AP");
    assert_eq!(actual.declared(), &saved);
    let EncodedTeletextAuthoredPage::CompiledDisplay(display) = &actual.pages()[0] else {
        panic!("complete authored output required");
    };
    assert!(!actual.pages()[0].data_units().is_empty());
    let normalized = display.normalized();
    let setters: Vec<_> = normalized
        .original()
        .program()
        .commands()
        .iter()
        .zip(normalized.original().provenance())
        .filter_map(|(command, source)| {
            matches!(source, TeletextMainCompositionSource::HeaderPosition { .. })
                .then_some(*command)
        })
        .collect();
    assert_eq!(setters, [fg(8, 7)]);
    for geometry in [
        normalized.original().display_geometry(),
        normalized.physical().replay().display_geometry(),
    ] {
        for (object_type, start, finish) in [
            (TeletextObjectType::Active, (0, 0), (1, 8)),
            (TeletextObjectType::Passive, (0, 8), (0, 8)),
        ] {
            let region = geometry
                .object_regions()
                .iter()
                .find(|region| region.instance().object_type() == object_type)
                .unwrap();
            assert_eq!(
                region.interval(),
                TeletextObjectAddressInterval::Known { start, finish }
            );
        }
        assert_eq!(geometry.object_open_count().maximum_known_open(), 2);
    }
    let fallback = encode_teletext_page(
        &saved.semantic().base().pages[0],
        TeletextEncodingRegion::WesternLatin,
    )
    .unwrap();
    let actual = project_encoded_teletext_display(display.page(), 128).unwrap();
    let baseline = project_encoded_teletext_display(&fallback, 128).unwrap();
    assert_eq!(actual.header, baseline.header);
    let rows = |records: Vec<TeletextEncodedDisplayRecord>| {
        records
            .into_iter()
            .filter(|record| !matches!(record, TeletextEncodedDisplayRecord::Triplets { .. }))
            .collect::<Vec<_>>()
    };
    assert_eq!(rows(actual.records), rows(baseline.records));
}

#[test]
fn complete_authored_factory_keeps_header_call_geometry_and_base_fallback() {
    for (first, last) in [(3, 4), (4, 3)] {
        let mut draft = fixture_draft_with_active_body(
            |passive, active| vec![active, position(9), passive],
            body(first, last),
        );
        draft.base.pages[0].rows.push(TeletextRow {
            row: 0,
            runs: vec![TeletextRun {
                column: 9,
                style: TeletextStyle::default(),
                content: TeletextRunContent::Text { text: "ZZ".into() },
            }],
        });
        let original = declared(draft);
        let saved = original.clone();
        let actual =
            encode_teletext_authored_service(original, &transmissions(&saved), &limits()).unwrap();
        assert_eq!(actual.declared(), &saved);
        let EncodedTeletextAuthoredPage::CompiledDisplay(display) = &actual.pages()[0] else {
            panic!("header call must retain complete authored output");
        };
        let normalized = display.normalized();
        let setters: Vec<_> = normalized
            .original()
            .program()
            .commands()
            .iter()
            .zip(normalized.original().provenance())
            .filter_map(|(command, source)| {
                matches!(source, TeletextMainCompositionSource::HeaderPosition { .. })
                    .then_some(*command)
            })
            .collect();
        assert_eq!(setters, [fg(9, last)]);
        for geometry in [
            normalized.original().display_geometry(),
            normalized.physical().replay().display_geometry(),
        ] {
            let active = geometry
                .object_regions()
                .iter()
                .find(|region| region.instance().object_type() == TeletextObjectType::Active)
                .unwrap();
            assert_eq!(
                active.interval(),
                TeletextObjectAddressInterval::Known {
                    start: (0, 0),
                    finish: (0, 10),
                }
            );
            let passive = geometry
                .object_regions()
                .iter()
                .find(|region| region.instance().object_type() == TeletextObjectType::Passive)
                .unwrap();
            assert_eq!(
                passive.interval(),
                TeletextObjectAddressInterval::Known {
                    start: (0, 9),
                    finish: (0, 9),
                }
            );
            assert_eq!(geometry.object_open_count().maximum_known_open(), 2);
        }
        let fallback = encode_teletext_page(
            &saved.semantic().base().pages[0],
            TeletextEncodingRegion::WesternLatin,
        )
        .unwrap();
        let actual = project_encoded_teletext_display(display.page(), 128).unwrap();
        let baseline = project_encoded_teletext_display(&fallback, 128).unwrap();
        assert_eq!(actual.header, baseline.header);
        let rows = |records: Vec<TeletextEncodedDisplayRecord>| {
            records
                .into_iter()
                .filter(|record| !matches!(record, TeletextEncodedDisplayRecord::Triplets { .. }))
                .collect::<Vec<_>>()
        };
        assert_eq!(rows(actual.records), rows(baseline.records));
    }
}

fn assert_initial_header_geometry(geometry: &TeletextDisplayGeometry) {
    let active = geometry
        .object_regions()
        .iter()
        .find(|region| region.instance().object_type() == TeletextObjectType::Active)
        .unwrap();
    assert_eq!(
        active.interval(),
        TeletextObjectAddressInterval::Known {
            start: (0, 8),
            finish: (0, 10),
        }
    );
    let passive = geometry
        .object_regions()
        .iter()
        .find(|region| region.instance().object_type() == TeletextObjectType::Passive)
        .unwrap();
    assert_eq!(
        passive.interval(),
        TeletextObjectAddressInterval::Known {
            start: (0, 9),
            finish: (0, 9),
        }
    );
    assert_eq!(geometry.object_open_count().maximum_known_open(), 2);
}

fn assert_initial_header_context(original: &TeletextMainComposition) {
    assert_initial_header_geometry(original.display_geometry());
    let active = original
        .display_geometry()
        .object_regions()
        .iter()
        .find(|region| region.instance().object_type() == TeletextObjectType::Active)
        .unwrap();
    let attributes = original.attribute_execution();
    assert_eq!(
        attributes
            .pending()
            .iter()
            .map(|pending| pending.position())
            .collect::<Vec<_>>(),
        (8..40).map(|column| (0, column)).collect::<Vec<_>>()
    );
    for pending in attributes.pending() {
        assert_eq!(
            pending.reason(),
            TeletextAttributePendingReason::TransmissionOrder
        );
        assert_eq!(
            pending.root_source(),
            TeletextSpatialRootSource::Local { command: 1 }
        );
        assert_eq!(pending.invocation(), Some(active.instance().id()));
        assert!(pending.previous().is_none() && pending.incoming_previous().is_none());
        for (source, ordinal) in [(pending.first_source(), 0), (pending.second_source(), 1)] {
            assert!(matches!(source, Some(TeletextAttributeSource::Enhancement {
                location: TeletextEnhancementSource { object: Some(target), command },
                invocation: Some(invocation), ..
            }) if target == active.instance().target() && command == ordinal && invocation == active.instance().id()));
        }
    }
}

#[test]
fn initial_header_active_call_settles_its_exact_relative_body_and_preserves_factory_fallback() {
    for (first, last) in [(3, 4), (4, 3)] {
        let saved = declared(initial_header_draft(first, last));
        let original = compose_teletext_main_program(
            saved.semantic(),
            saved.semantic().base().pages[0].address,
            limits().normalization.composition,
        )
        .unwrap();
        assert_initial_header_context(&original);
        let actual =
            encode_teletext_authored_service(saved.clone(), &transmissions(&saved), &limits())
                .expect("initial header8 must use exact same-Active transmitted order");
        assert_eq!(actual.declared(), &saved);
        let EncodedTeletextAuthoredPage::CompiledDisplay(display) = &actual.pages()[0] else {
            panic!("complete authored output required");
        };
        let normalized = display.normalized();
        for geometry in [
            normalized.original().display_geometry(),
            normalized.physical().replay().display_geometry(),
        ] {
            assert_initial_header_geometry(geometry);
        }
        let setters: Vec<_> = normalized
            .original()
            .program()
            .commands()
            .iter()
            .zip(normalized.original().provenance())
            .filter_map(|(command, source)| {
                matches!(source, TeletextMainCompositionSource::HeaderPosition { .. })
                    .then_some(*command)
            })
            .collect();
        assert_eq!(
            setters,
            [
                TeletextEnhancementCommand::AtColumn {
                    column: 8,
                    operation: TeletextColumnOperation::Background {
                        colour: TeletextColourReference::new(0).unwrap(),
                    },
                },
                fg(9, last)
            ]
        );
        let fallback = encode_teletext_page(
            &saved.semantic().base().pages[0],
            TeletextEncodingRegion::WesternLatin,
        )
        .unwrap();
        let actual = project_encoded_teletext_display(display.page(), 128).unwrap();
        let baseline = project_encoded_teletext_display(&fallback, 128).unwrap();
        assert_eq!(actual.header, baseline.header);
        let rows = |records: Vec<TeletextEncodedDisplayRecord>| {
            records
                .into_iter()
                .filter(|record| !matches!(record, TeletextEncodedDisplayRecord::Triplets { .. }))
                .collect::<Vec<_>>()
        };
        assert_eq!(rows(actual.records), rows(baseline.records));
    }
}

fn initial_header_draft(first: u8, last: u8) -> TeletextResolvedServiceDraft {
    let mut draft = fixture_draft_with_active_body(
        |passive, active| vec![position(8), active, position(9), passive],
        vec![
            fg(0, first),
            fg(0, last),
            TeletextEnhancementCommand::AtColumn {
                column: 2,
                operation: TeletextColumnOperation::G0 {
                    code: TeletextGlyphCode::new(b'B').unwrap(),
                },
            },
        ],
    );
    draft.base.pages[0].rows.push(TeletextRow {
        row: 0,
        runs: vec![TeletextRun {
            column: 8,
            style: TeletextStyle::default(),
            content: TeletextRunContent::Text { text: "ZZZ".into() },
        }],
    });
    draft
}

fn initial_composition(service: &TeletextSemanticService) -> TeletextMainComposition {
    compose_teletext_main_program(
        service,
        service.base().pages[0].address,
        limits().normalization.composition,
    )
    .unwrap()
}

#[test]
fn initial_header_background_candidate_requires_exact_original_call_and_unwritten_base_context() {
    use super::super::super::current_local_background;
    let service =
        TeletextSemanticService::new(initial_header_draft(3, 4), semantic_limits()).unwrap();
    let original = initial_composition(&service);
    assert_eq!(
        current_local_background(&original, 1, 8),
        Some(TeletextColourReference::new(0).unwrap())
    );
    for (before, column) in [(0, 8), (3, 9), (usize::MAX, 8), (1, 9)] {
        assert_eq!(current_local_background(&original, before, column), None);
    }
    for wrong_source in [true, false] {
        let mut provenance = original.provenance().to_vec();
        let mut commands = original.program().commands().to_vec();
        if wrong_source {
            provenance[1] = TeletextMainCompositionSource::HeaderPosition { command: 1 };
        } else {
            let TeletextEnhancementCommand::InvokeObject { offset, .. } = &mut commands[1] else {
                panic!("exact original call");
            };
            *offset = Some(TeletextObjectOffset::new(1, 0).unwrap());
        }
        let wrong = original
            .clone()
            .with_physical_main(
                &service,
                TeletextEnhancementProgram::new(commands).unwrap(),
                provenance,
                limits().normalization.composition,
                &mut TeletextCompileContext::new(None, None, None),
            )
            .unwrap();
        assert_eq!(current_local_background(&wrong, 1, 8), None);
    }
    let mut wrong_type = initial_header_draft(3, 4);
    wrong_type
        .objects
        .iter_mut()
        .find(|definition| definition.object_type == TeletextObjectType::Active)
        .unwrap()
        .object_type = TeletextObjectType::Adaptive;
    let mut commands = wrong_type.pages[0]
        .local_program
        .as_ref()
        .unwrap()
        .commands()
        .to_vec();
    let TeletextEnhancementCommand::InvokeObject { object_type, .. } = &mut commands[1] else {
        panic!("exact original call");
    };
    *object_type = TeletextObjectType::Adaptive;
    wrong_type.pages[0].local_program = Some(TeletextEnhancementProgram::new(commands).unwrap());
    let wrong_type = TeletextSemanticService::new(wrong_type, semantic_limits()).unwrap();
    assert_eq!(
        current_local_background(&initial_composition(&wrong_type), 1, 8),
        None
    );
    for backgrounds in [vec![0], vec![0, 1]] {
        let mut draft = initial_header_draft(3, 4);
        let active = draft
            .objects
            .iter_mut()
            .find(|definition| definition.object_type == TeletextObjectType::Active)
            .unwrap();
        let mut commands = active.program.commands().to_vec();
        commands.splice(
            2..2,
            backgrounds
                .into_iter()
                .map(|value| TeletextEnhancementCommand::AtColumn {
                    column: 0,
                    operation: TeletextColumnOperation::Background {
                        colour: TeletextColourReference::new(value).unwrap(),
                    },
                }),
        );
        active.program = TeletextEnhancementProgram::new(commands).unwrap();
        let service = TeletextSemanticService::new(draft, semantic_limits()).unwrap();
        assert_eq!(
            current_local_background(&initial_composition(&service), 1, 8),
            None
        );
    }
}

#[test]
fn provisional_local_words_require_exact_final_body_source_and_relative_mapping() {
    let service = service(3, 4);
    let budget = limits().normalization;
    let original = compose_teletext_main_program(
        &service,
        service.base().pages[0].address,
        budget.composition,
    )
    .unwrap();
    let allocations =
        allocate_teletext_resource_objects(service.objects(), &[], budget.max_resource_objects)
            .unwrap();
    let (candidate, witnesses) = prepare(
        original,
        &service,
        &allocations,
        budget.composition,
        budget.packing,
        &mut TeletextCompileContext::new(None, None, None),
    )
    .unwrap();
    assert_eq!(
        witnesses.len(),
        3,
        "only the already-called Active body grants evidence"
    );
    for (ordinal, witness) in witnesses.iter().enumerate() {
        assert_eq!(witness.position.word, ordinal);
        assert_eq!(witness.position.source.command, ordinal);
        assert_eq!(witness.position.invocation.unwrap().get(), 0);
        assert!(witness.local.is_some());
    }
    let packed = pack_composition_program(
        &service,
        &candidate,
        candidate.program(),
        &allocations,
        budget.packing,
    )
    .unwrap();
    verify(&candidate, &packed, &witnesses).unwrap();
    for corrupt in [0, 1, 2, 3] {
        let mut wrong = witnesses.clone();
        match corrupt {
            0 => wrong[0].position.word += 1,
            1 => wrong[0].bytes[0] ^= 1,
            2 => wrong[0].position.source.command += 1,
            _ => {
                wrong[0].position.invocation =
                    Some(candidate.spatial_trace().invocations()[1].id());
            }
        }
        assert_eq!(
            verify(&candidate, &packed, &wrong).unwrap_err().reason,
            TeletextMainCompositionFailure::SelectionState
        );
    }
    // An added payload after all three unchanged witnessed words cannot hide
    // behind their individually unchanged bytes or source/mapping entries.
    let mut changed_body = body(3, 4);
    changed_body.push(TeletextEnhancementCommand::AtColumn {
        column: 11,
        operation: TeletextColumnOperation::G0 {
            code: TeletextGlyphCode::new(b'C').unwrap(),
        },
    });
    let changed = fixture_with_active_body(
        |passive, active| vec![active, position(9), passive],
        changed_body,
    );
    let wrong = pack_composition_program(
        &changed,
        &candidate,
        candidate.program(),
        &allocations,
        budget.packing,
    )
    .unwrap();
    assert_eq!(
        verify(&candidate, &wrong, &witnesses).unwrap_err().reason,
        TeletextMainCompositionFailure::SelectionState
    );
}

#[test]
fn repeated_local_instances_keep_distinct_relative_order_evidence() {
    let service = fixture_with_active_body(
        |passive, active| {
            let mut second = active;
            if let TeletextEnhancementCommand::InvokeObject { offset, .. } = &mut second {
                *offset = Some(TeletextObjectOffset::new(1, 0).unwrap());
            }
            vec![active, second, position(9), passive]
        },
        body(3, 4),
    );
    let budget = limits().normalization;
    let original = compose_teletext_main_program(
        &service,
        service.base().pages[0].address,
        budget.composition,
    )
    .unwrap();
    let allocations =
        allocate_teletext_resource_objects(service.objects(), &[], budget.max_resource_objects)
            .unwrap();
    let (candidate, witnesses) = prepare(
        original,
        &service,
        &allocations,
        budget.composition,
        budget.packing,
        &mut TeletextCompileContext::new(None, None, None),
    )
    .unwrap();
    assert_eq!(witnesses.len(), 6);
    for id in [0, 1] {
        let words: Vec<_> = witnesses
            .iter()
            .filter(|w| w.position.invocation.unwrap().get() == id)
            .map(|w| w.position.word)
            .collect();
        assert_eq!(words, [0, 1, 2]);
    }
    assert_eq!(
        super::super::super::foreground(&candidate, 9),
        Some(TeletextColourReference::new(4).unwrap())
    );
    compile_teletext_normalized_main_program(&service, service.base().pages[0].address, budget)
        .unwrap();
}

#[test]
fn local_body_prefix_does_not_bypass_final_208_word_capacity() {
    for (count, fits) in [(195, true), (196, false)] {
        let service = fixture_with_active_body(
            |passive, active| {
                let mut main = vec![
                    TeletextEnhancementCommand::SetScreenColour {
                        colour: TeletextColourReference::new(0).unwrap(),
                    };
                    count
                ];
                main.extend([active, position(9), passive]);
                main
            },
            body(3, 4),
        );
        let result = compile_teletext_normalized_main_program(
            &service,
            service.base().pages[0].address,
            limits().normalization,
        );
        if fits {
            let normalized = result.unwrap();
            assert_eq!(normalized.encoded().packet_bodies().len(), 16);
            assert_eq!(normalized.encoded().main_triplet_count(), 199);
        } else {
            let error = result.unwrap_err();
            let crate::teletext::TeletextNormalizedMainProgramFailure::Original(error) =
                error.reason
            else {
                panic!("final original packing must enforce capacity");
            };
            let TeletextComposedMainEncodingError::Packing(error) = *error else {
                panic!("final packing must fail");
            };
            assert_eq!(
                error.reason,
                crate::teletext::TeletextLocalObjectProgramEncodingFailure::Capacity {
                    count: 209,
                    maximum: 208,
                }
            );
        }
    }
}

#[test]
fn main_header_settlement_does_not_pin_irrelevant_nested_local_pointer_bytes() {
    let owner = input().base.pages[0].address;
    let adaptive = TeletextObjectIdentity::local(owner, TeletextObjectId::new(65533));
    let empty = TeletextObjectIdentity::local(owner, TeletextObjectId::new(65532));
    let mut draft = fixture_draft_with_active_body(
        |passive, _| {
            vec![
                fg(8, 1),
                fg(8, 2),
                TeletextEnhancementCommand::InvokeObject {
                    target: adaptive,
                    object_type: TeletextObjectType::Adaptive,
                    offset: None,
                },
                position(9),
                passive,
            ]
        },
        vec![],
    );
    draft.objects.extend([
        TeletextEnhancementObjectDefinition {
            identity: adaptive,
            object_type: TeletextObjectType::Adaptive,
            program: TeletextEnhancementProgram::new(vec![
                TeletextEnhancementCommand::AtColumn {
                    column: 0,
                    operation: TeletextColumnOperation::G0 {
                        code: TeletextGlyphCode::new(b'D').unwrap(),
                    },
                },
                TeletextEnhancementCommand::InvokeObject {
                    target: empty,
                    object_type: TeletextObjectType::Passive,
                    offset: None,
                },
            ])
            .unwrap(),
        },
        TeletextEnhancementObjectDefinition {
            identity: empty,
            object_type: TeletextObjectType::Passive,
            program: TeletextEnhancementProgram::new(vec![]).unwrap(),
        },
    ]);
    let service = TeletextSemanticService::new(draft, semantic_limits()).unwrap();
    let saved = service.clone();
    let normalized =
        compile_teletext_normalized_main_program(&service, owner, limits().normalization).unwrap();
    let setters: Vec<_> = normalized
        .original()
        .program()
        .commands()
        .iter()
        .zip(normalized.original().provenance())
        .filter_map(|(command, source)| {
            matches!(source, TeletextMainCompositionSource::HeaderPosition { .. })
                .then_some(*command)
        })
        .collect();
    assert_eq!(setters, [fg(9, 2)]);
    assert_eq!(service, saved);
}
