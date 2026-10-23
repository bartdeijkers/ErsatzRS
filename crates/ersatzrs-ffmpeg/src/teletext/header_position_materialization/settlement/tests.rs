//! Original Main ordering, multi-call progress and rejection boundaries.

use super::*;
use crate::teletext::{
    allocate_teletext_resource_objects,
    authored_service_tests::{input, limits, semantic_limits},
    compile_teletext_normalized_main_program, compose_teletext_main_program,
};
use ersatzrs_domain::*;

fn fg(column: u8, value: u8) -> TeletextEnhancementCommand {
    TeletextEnhancementCommand::AtColumn {
        column,
        operation: TeletextColumnOperation::Foreground {
            colour: TeletextColourReference::new(value).unwrap(),
        },
    }
}

fn position(column: u8) -> TeletextEnhancementCommand {
    TeletextEnhancementCommand::SetPosition {
        position: TeletextEnhancementPosition::new(0, column).unwrap(),
    }
}

fn fixture(
    build: impl FnOnce(
        TeletextEnhancementCommand,
        TeletextEnhancementCommand,
    ) -> Vec<TeletextEnhancementCommand>,
) -> TeletextSemanticService {
    fixture_with_active_body(build, vec![fg(0, 3), fg(0, 4)])
}

fn fixture_with_active_body(
    build: impl FnOnce(
        TeletextEnhancementCommand,
        TeletextEnhancementCommand,
    ) -> Vec<TeletextEnhancementCommand>,
    active_body: Vec<TeletextEnhancementCommand>,
) -> TeletextSemanticService {
    TeletextSemanticService::new(
        fixture_draft_with_active_body(build, active_body),
        semantic_limits(),
    )
    .unwrap()
}

fn fixture_draft_with_active_body(
    build: impl FnOnce(
        TeletextEnhancementCommand,
        TeletextEnhancementCommand,
    ) -> Vec<TeletextEnhancementCommand>,
    active_body: Vec<TeletextEnhancementCommand>,
) -> TeletextResolvedServiceDraft {
    let mut draft = input();
    let owner = draft.base.pages[0].address;
    let mut call = |id, object_type, body| {
        let target = TeletextObjectIdentity::local(owner, TeletextObjectId::new(id));
        draft.objects.push(TeletextEnhancementObjectDefinition {
            identity: target,
            object_type,
            program: TeletextEnhancementProgram::new(body).unwrap(),
        });
        TeletextEnhancementCommand::InvokeObject {
            target,
            object_type,
            offset: None,
        }
    };
    let passive = call(
        65535,
        TeletextObjectType::Passive,
        vec![
            fg(0, 2),
            TeletextEnhancementCommand::AtColumn {
                column: 0,
                operation: TeletextColumnOperation::G0 {
                    code: TeletextGlyphCode::new(b'A').unwrap(),
                },
            },
        ],
    );
    let active = call(65534, TeletextObjectType::Active, active_body);
    draft.pages[0].local_program =
        Some(TeletextEnhancementProgram::new(build(passive, active)).unwrap());
    draft
}

#[test]
fn multiple_header_calls_settle_with_known_and_unsettled_prefixes() {
    for first_known in [false, true] {
        let service = fixture(|passive, _| {
            let mut main = if first_known {
                vec![]
            } else {
                vec![fg(8, 1), fg(8, 2)]
            };
            main.extend([
                position(9),
                passive,
                fg(11, 3),
                fg(11, 4),
                position(12),
                passive,
            ]);
            main
        });
        let saved = service.clone();
        let normalized = compile_teletext_normalized_main_program(
            &service,
            service.base().pages[0].address,
            limits().normalization,
        )
        .unwrap();
        let original = normalized.original();
        let setters: Vec<_> = original
            .program()
            .commands()
            .iter()
            .zip(original.provenance())
            .filter_map(|(command, source)| {
                matches!(source, TeletextMainCompositionSource::HeaderPosition { .. })
                    .then_some(*command)
            })
            .collect();
        assert_eq!(setters, [fg(9, if first_known { 7 } else { 2 }), fg(12, 4)]);
        assert_eq!(service, saved);
    }
}

#[test]
fn prefix_only_witnesses_retained_main_and_checks_final_words() {
    let service = fixture(|passive, _| vec![fg(8, 1), fg(8, 2), position(9), passive]);
    let budget = limits().normalization;
    let owner = service.base().pages[0].address;
    let original = compose_teletext_main_program(&service, owner, budget.composition).unwrap();
    let allocations =
        allocate_teletext_resource_objects(service.objects(), &[], budget.max_resource_objects)
            .unwrap();
    let (candidate, witnesses) = prepare(
        original,
        &service,
        &allocations,
        budget.composition,
        (budget.packing, budget.assembly.max_work),
        &mut TeletextCompileContext::new(None, None, None),
    )
    .unwrap();
    assert_eq!(witnesses.len(), 2);
    assert!(witnesses
        .iter()
        .all(|w| w.position.source.object.is_none() && w.position.invocation.is_none()));
    let packed = pack_composition_program(
        &service,
        &candidate,
        candidate.program(),
        &allocations,
        budget.packing,
    )
    .unwrap();
    // Appended definitions really are present, but never partial witnesses.
    assert_eq!(packed.definition_positions().len(), 2);
    assert!(packed
        .command_positions()
        .iter()
        .any(|p| p.source().object.is_some()));
    verify(&candidate, &packed, &witnesses).unwrap();
    let mut wrong = witnesses.clone();
    wrong[0].position.word += 1;
    assert_eq!(
        verify(&candidate, &packed, &wrong).unwrap_err().reason,
        TeletextMainCompositionFailure::SelectionState
    );
    let mut wrong = witnesses;
    wrong[0].bytes[0] ^= 1;
    assert_eq!(
        verify(&candidate, &packed, &wrong).unwrap_err().reason,
        TeletextMainCompositionFailure::SelectionState
    );
}

#[test]
fn local_active_prefix_settles_its_own_header_foreground_before_a_later_call() {
    let service = local_active::service(3, 4);
    let saved = service.clone();
    let original = compose_teletext_main_program(
        &service,
        service.base().pages[0].address,
        limits().normalization.composition,
    )
    .unwrap();
    let active = original
        .display_geometry()
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
    let attributes = original.attribute_execution();
    let foreground = attributes.before_adaptive_cell(0, 9).unwrap().foreground();
    assert_eq!(foreground.value(), None);
    let pending = attributes.pending()[foreground.pending().unwrap()];
    assert_eq!(
        pending.reason(),
        TeletextAttributePendingReason::TransmissionOrder
    );
    assert!(pending.previous().is_none());
    assert!(pending.incoming_previous().is_none());
    for (source, ordinal) in [(pending.first_source(), 0), (pending.second_source(), 1)] {
        assert!(matches!(source, Some(TeletextAttributeSource::Enhancement {
            location: TeletextEnhancementSource { object: Some(target), command },
            invocation: Some(invocation), ..
        }) if target == active.instance().target() && command == ordinal && invocation == active.instance().id()));
    }
    let normalized = compile_teletext_normalized_main_program(
        &service,
        service.base().pages[0].address,
        limits().normalization,
    )
    .unwrap();
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
    assert_eq!(setters, [fg(9, 4)]);
    assert_eq!(service, saved);
}

mod local_active;

#[test]
fn same_page_main_and_unique_active_native_words_settle_header_before_later_call() {
    let service = fixture(|passive, active| vec![fg(8, 1), active, position(9), passive]);
    let saved = service.clone();
    let budget = limits().normalization;
    let owner = service.base().pages[0].address;
    let original = compose_teletext_main_program(&service, owner, budget.composition).unwrap();
    assert_eq!(super::super::foreground(&original, 9), None);
    let normalized = compile_teletext_normalized_main_program(&service, owner, budget).unwrap();
    assert_eq!(
        super::super::foreground(normalized.original(), 9),
        Some(TeletextColourReference::new(4).unwrap())
    );
    let setters = normalized
        .original()
        .program()
        .commands()
        .iter()
        .zip(normalized.original().provenance())
        .filter_map(|(command, source)| {
            matches!(source, TeletextMainCompositionSource::HeaderPosition { .. })
                .then_some(*command)
        })
        .collect::<Vec<_>>();
    assert_eq!(setters, [fg(9, 4)]);
    let native = normalized.encoded();
    let main = native
        .command_positions()
        .iter()
        .find(|position| {
            position.original_main_source()
                == Some(TeletextMainCompositionSource::OriginalPage { command: 0 })
        })
        .unwrap();
    let target = TeletextObjectIdentity::local(owner, TeletextObjectId::new(65534));
    let body = [0, 1].map(|command| {
        native
            .command_positions()
            .iter()
            .find(|position| {
                position.source().object == Some(target)
                    && position.original_object_source()
                        == Some(TeletextMainCompositionSource::Object {
                            identity: target,
                            command,
                        })
            })
            .unwrap()
    });
    assert!(main.semantic_word() < body[0].semantic_word());
    assert!(body[0].semantic_word() < body[1].semantic_word());
    let actual = normalized
        .physical()
        .replay()
        .attribute_execution()
        .before_adaptive_cell(0, 9)
        .unwrap()
        .foreground();
    assert_eq!(
        actual.value(),
        Some(TeletextColourReference::new(4).unwrap())
    );
    assert_eq!(service, saved);
}

#[test]
fn prefix_settlement_consumes_existing_display_quota() {
    let service = fixture(|passive, _| vec![fg(8, 1), fg(8, 2), position(9), passive]);
    let saved = service.clone();
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
    let error = prepare(
        original,
        &service,
        &allocations,
        budget.composition,
        (budget.packing, budget.assembly.max_work),
        &mut TeletextCompileContext::new(None, None, Some(0)),
    )
    .unwrap_err();
    let TeletextComposedMainEncodingError::Composition(error) = error else {
        panic!("settlement must consume the existing display quota")
    };
    assert!(matches!(
        error.reason,
        TeletextMainCompositionFailure::DisplayResolution(TeletextDisplayResolutionError {
            reason: TeletextDisplayResolutionFailure::UpdateBudget { maximum: 0 },
            ..
        })
    ));
    assert_eq!(service, saved);
}

#[test]
fn c2_prefix_keeps_final_208_word_capacity_authoritative() {
    for (count, fits) in [(195, true), (196, false)] {
        let service = fixture(|passive, _| {
            let mut commands = vec![
                TeletextEnhancementCommand::SetScreenColour {
                    colour: TeletextColourReference::new(0).unwrap(),
                };
                count
            ];
            commands.extend([fg(8, 1), fg(8, 2), position(9), passive]);
            commands
        });
        let saved = service.clone();
        let result = compile_teletext_normalized_main_program(
            &service,
            service.base().pages[0].address,
            limits().normalization,
        );
        if fits {
            let normalized = result.unwrap();
            assert_eq!(normalized.encoded().packet_bodies().len(), 16);
            assert_eq!(normalized.encoded().main_triplet_count(), 200);
        } else {
            let Err(error) = result else {
                panic!("209 real words must not yield a normalized candidate")
            };
            let crate::teletext::TeletextNormalizedMainProgramFailure::Original(error) =
                error.reason
            else {
                panic!("capacity is a final original-packing failure")
            };
            let TeletextComposedMainEncodingError::Packing(error) = *error else {
                panic!("packing failure required")
            };
            assert_eq!(
                error.reason,
                crate::teletext::TeletextLocalObjectProgramEncodingFailure::Capacity {
                    count: 209,
                    maximum: 208
                }
            );
        }
        assert_eq!(service, saved);
    }
}

mod resource_order;
mod same_page;
