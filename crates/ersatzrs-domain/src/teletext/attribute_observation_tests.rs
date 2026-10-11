//! Original candidate attributes for later C.3 occupancy, not footprint acceptance.

use super::attribute_execution_tests::{
    at, base, definition, foreground, glyph, invoke, local, owner, service, set,
};
use super::*;

struct Fixture {
    base: TeletextBaseDisplay,
    original: TeletextSpatialTrace,
    layers: TeletextGlyphLayers,
}

fn fixture(service: &TeletextSemanticService) -> Fixture {
    let base = base();
    let original = resolve_teletext_spatial_trace(
        service,
        owner(),
        TeletextSpatialTraceBudget {
            max_events: 4096,
            max_invocations: 32,
        },
    )
    .unwrap();
    let layers = build_teletext_glyph_layers(&base, &original, 4096).unwrap();
    Fixture {
        base,
        original,
        layers,
    }
}

fn execute(
    fixture: &Fixture,
    maximum: usize,
) -> Result<TeletextAttributeExecution, TeletextAttributeExecutionError> {
    resolve_teletext_attribute_execution(
        &fixture.base,
        &fixture.original,
        &fixture.layers,
        &[TeletextColourReference::BLACK; 25],
        maximum,
    )
}

fn display(column: u8, enabled: bool) -> TeletextEnhancementCommand {
    at(
        column,
        TeletextColumnOperation::Attributes {
            double_height: enabled,
            double_width: enabled,
            boxed: enabled,
            conceal: enabled,
            invert: enabled,
            underline_or_separated_mosaics: enabled,
        },
    )
}

fn private_program(colour: u8, enabled: bool, code: u8) -> Vec<TeletextEnhancementCommand> {
    vec![
        foreground(0, colour),
        at(
            0,
            TeletextColumnOperation::Background {
                colour: TeletextColourReference::new(if enabled { 4 } else { 0 }).unwrap(),
            },
        ),
        at(
            0,
            TeletextColumnOperation::Flash {
                mode: if enabled {
                    TeletextFlashMode::Inverted
                } else {
                    TeletextFlashMode::Steady
                },
                rate: if enabled {
                    TeletextFlashRate::Fast2HzPhase3
                } else {
                    TeletextFlashRate::Slow1Hz
                },
            },
        ),
        display(0, enabled),
        glyph(0, code),
    ]
}

fn record_for(
    execution: &TeletextAttributeExecution,
    identity: TeletextObjectIdentity,
    command: usize,
    instance: usize,
) -> &TeletextExecutedGlyphAttributes {
    execution
        .records()
        .iter()
        .find(|record| {
            record.candidate().source()
                == Some(TeletextEnhancementSource {
                    object: Some(identity),
                    command,
                })
                && record
                    .candidate()
                    .invocation()
                    .map(TeletextSpatialInvocationId::get)
                    == Some(instance)
        })
        .unwrap()
}

fn foreground_index(attributes: &TeletextExecutedAttributes) -> u8 {
    attributes.foreground().value().unwrap().get()
}

#[test]
fn losing_passive_retains_all_private_fields_while_global_cell_keeps_later_candidate() {
    let first = local(0xffff);
    let last = local(0x0100);
    let service = service(
        vec![
            set(2, 8),
            invoke(first, TeletextObjectType::Passive, None),
            invoke(last, TeletextObjectType::Passive, None),
        ],
        vec![
            definition(
                first,
                TeletextObjectType::Passive,
                private_program(1, true, b'A'),
            ),
            definition(
                last,
                TeletextObjectType::Passive,
                private_program(4, false, b'B'),
            ),
        ],
    );
    let inputs = fixture(&service);
    let original = inputs.original.clone();
    let layers = inputs.layers.clone();
    let execution = execute(&inputs, 13).unwrap();
    assert_eq!(inputs.original.events().len(), 13);
    let losing = record_for(&execution, first, 4, 0);
    let winner = record_for(&execution, last, 4, 1);
    let attributes = losing.attributes().unwrap();
    assert_eq!(foreground_index(&attributes), 1);
    assert_eq!(attributes.background().value().unwrap().get(), 4);
    assert_eq!(
        attributes.flash().value().unwrap().mode(),
        TeletextFlashMode::Inverted
    );
    assert_eq!(
        attributes.flash().value().unwrap().rate(),
        TeletextFlashRate::Fast2HzPhase3
    );
    for flag in [
        attributes.double_height(),
        attributes.double_width(),
        attributes.boxed(),
        attributes.conceal(),
        attributes.invert(),
        attributes.underline_or_separated_mosaics(),
    ] {
        assert_eq!(flag.value(), Some(true));
    }
    assert_eq!(losing.reason(), None);
    assert_eq!(losing.candidate().position(), (2, 8));
    assert_eq!(
        losing.candidate().root_source(),
        Some(TeletextSpatialRootSource::Local { command: 1 })
    );
    assert_eq!(winner.attributes().unwrap(), *execution.cell(2, 8).unwrap());
    assert_eq!(foreground_index(&winner.attributes().unwrap()), 4);
    assert_eq!(
        winner.attributes().unwrap().double_width().value(),
        Some(false)
    );
    assert_eq!(inputs.original, original);
    assert_eq!(inputs.layers, layers);
}

#[test]
fn shared_local_adaptive_and_private_passive_observations_keep_their_original_stages() {
    let adaptive = local(0x0100);
    let passive = local(0xffff);
    let service = service(
        vec![
            set(2, 8),
            foreground(8, 2),
            glyph(8, b'L'),
            invoke(adaptive, TeletextObjectType::Adaptive, None),
            invoke(passive, TeletextObjectType::Passive, None),
        ],
        vec![
            definition(
                adaptive,
                TeletextObjectType::Adaptive,
                vec![foreground(0, 3), glyph(0, b'A')],
            ),
            definition(
                passive,
                TeletextObjectType::Passive,
                private_program(4, false, b'P'),
            ),
        ],
    );
    let execution = execute(&fixture(&service), 128).unwrap();
    let at_cell: Vec<_> = execution
        .records()
        .iter()
        .filter(|record| record.candidate().position() == (2, 8))
        .collect();
    assert_eq!(at_cell.len(), 4);
    for (record, priority, colour) in [
        (at_cell[0], TeletextGlyphPriority::Base, 3),
        (at_cell[1], TeletextGlyphPriority::Local, 3),
        (at_cell[2], TeletextGlyphPriority::Adaptive, 3),
        (at_cell[3], TeletextGlyphPriority::Passive, 4),
    ] {
        assert_eq!(record.candidate().priority(), priority);
        assert_eq!(foreground_index(&record.attributes().unwrap()), colour);
    }
    assert_eq!(
        foreground_index(execution.before_adaptive_cell(2, 8).unwrap()),
        2
    );
    assert_eq!(
        foreground_index(execution.before_passive_cell(2, 8).unwrap()),
        3
    );
    assert_eq!(foreground_index(execution.cell(2, 8).unwrap()), 4);
}

#[test]
fn repeated_nested_passive_instances_start_fresh_and_keep_private_state_across_rows() {
    let active = local(0xffff);
    let adaptive = local(0x0100);
    let passive = local(0);
    let service = service(
        vec![
            set(2, 8),
            invoke(active, TeletextObjectType::Active, None),
            set(5, 8),
            invoke(active, TeletextObjectType::Active, None),
        ],
        vec![
            definition(
                active,
                TeletextObjectType::Active,
                vec![invoke(adaptive, TeletextObjectType::Adaptive, None)],
            ),
            definition(
                adaptive,
                TeletextObjectType::Adaptive,
                vec![invoke(passive, TeletextObjectType::Passive, None)],
            ),
            definition(
                passive,
                TeletextObjectType::Passive,
                vec![
                    glyph(0, b'A'),
                    foreground(1, 1),
                    display(1, true),
                    glyph(1, b'B'),
                    set(1, 0),
                    glyph(0, b'C'),
                ],
            ),
        ],
    );
    let execution = execute(&fixture(&service), 128).unwrap();
    for (instance, row, root) in [(2, 2, 1), (5, 5, 3)] {
        let initial = record_for(&execution, passive, 0, instance);
        assert_eq!(foreground_index(&initial.attributes().unwrap()), 7);
        assert_eq!(
            initial.attributes().unwrap().double_width().value(),
            Some(false)
        );
        assert!(
            matches!(initial.attributes().unwrap().foreground().source(), Some(TeletextAttributeSource::PassiveDefault { invocation }) if invocation.get() == instance)
        );
        for (command, position) in [(3, (row, 9)), (5, (row + 1, 8))] {
            let retained = record_for(&execution, passive, command, instance);
            assert_eq!(retained.candidate().position(), position);
            assert_eq!(foreground_index(&retained.attributes().unwrap()), 1);
            assert_eq!(
                retained.attributes().unwrap().double_width().value(),
                Some(true)
            );
            assert_eq!(
                retained.candidate().root_source(),
                Some(TeletextSpatialRootSource::Local { command: root })
            );
        }
    }
}

#[test]
fn wide_and_header_passive_glyphs_retain_private_attributes_without_inventing_shared_cells() {
    let active = local(0xffff);
    let adaptive = local(0x0100);
    let passive = local(0);
    let service = service(
        vec![
            invoke(passive, TeletextObjectType::Passive, None),
            set(24, 39),
            invoke(active, TeletextObjectType::Active, Some((23, 71))),
        ],
        vec![
            definition(
                active,
                TeletextObjectType::Active,
                vec![
                    glyph(0, b'W'),
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
                vec![foreground(0, 1), glyph(0, b'P')],
            ),
        ],
    );
    let execution = execute(&fixture(&service), 128).unwrap();
    let header = record_for(&execution, passive, 1, 0);
    assert_eq!(header.candidate().position(), (0, 0));
    assert_eq!(foreground_index(&header.attributes().unwrap()), 1);
    assert!(header.reason().is_some());
    let wide_shared = record_for(&execution, active, 0, 1);
    assert_eq!(wide_shared.candidate().position(), (47, 110));
    assert_eq!(wide_shared.attributes(), None);
    assert!(wide_shared.reason().is_some());
    let wide_private = record_for(&execution, passive, 1, 3);
    assert_eq!(wide_private.candidate().position(), (141, 330));
    assert_eq!(foreground_index(&wide_private.attributes().unwrap()), 1);
    assert!(wide_private.reason().is_some());
    assert_eq!(execution.cell(0, 0), None);
}

fn base_with_unicode(original: &TeletextBaseDisplay) -> TeletextBaseDisplay {
    let mut rows = original.cells().to_vec();
    let position = TeletextEnhancementPosition::new(1, 8).unwrap();
    rows[1][8] = TeletextBaseDisplayCell::new(
        position,
        b'e',
        TeletextBaseDisplayCellKind::Glyph,
        Some(
            TeletextBaseAuthoredGlyph::new(
                position,
                8,
                TeletextStyle::default(),
                TeletextBaseAuthoredContent::Text { character: 'é' },
            )
            .unwrap(),
        ),
        rows[1][8].attributes(),
    )
    .unwrap();
    TeletextBaseDisplay::new(
        owner(),
        original.primary(),
        original.national_option(),
        None,
        rows.into_boxed_slice().try_into().unwrap(),
    )
    .unwrap()
}

#[test]
fn candidate_indexes_keep_original_base_unicode_control_roles_and_c2_pending_handles() {
    let service = service(
        vec![
            set(1, 18),
            foreground(18, 4),
            foreground(18, 13),
            glyph(18, b'A'),
        ],
        vec![],
    );
    let mut inputs = fixture(&service);
    inputs.base = base_with_unicode(&inputs.base);
    inputs.layers = build_teletext_glyph_layers(&inputs.base, &inputs.original, 128).unwrap();
    let execution = execute(&inputs, 4).unwrap();
    assert_eq!(execution.records().len(), 993);
    for (index, (record, candidate)) in execution
        .records()
        .iter()
        .zip(inputs.layers.candidates())
        .enumerate()
    {
        assert_eq!(record.candidate_index(), index);
        assert_eq!(record.candidate(), candidate);
    }
    let unicode = execution
        .records()
        .iter()
        .find(|record| record.candidate().position() == (1, 8))
        .unwrap()
        .candidate()
        .base()
        .unwrap();
    assert_eq!(unicode.code(), b'e');
    assert_eq!(
        unicode.authored().unwrap().content(),
        TeletextBaseAuthoredContent::Text { character: 'é' }
    );
    let control = execution
        .records()
        .iter()
        .find(|record| record.candidate().position() == (1, 17))
        .unwrap()
        .candidate()
        .base()
        .unwrap();
    assert!(matches!(
        control.kind(),
        TeletextBaseDisplayCellKind::SpacingControl {
            control: TeletextBaseSpacingControl::AlphaColour {
                colour: TeletextColour::Red
            },
            timing: TeletextSpacingTiming::SetAfter
        }
    ));
    for record in execution
        .records()
        .iter()
        .filter(|record| record.candidate().position() == (1, 18))
    {
        let field = record.attributes().unwrap().foreground();
        assert_eq!(field.value(), None);
        let pending = &execution.pending()[field.pending().unwrap()];
        assert_eq!(
            pending.reason(),
            TeletextAttributePendingReason::TransmissionOrder
        );
        assert_eq!(
            pending.location(),
            TeletextEnhancementSource {
                object: None,
                command: 2
            }
        );
        assert!(matches!(
            pending.first_source(),
            Some(TeletextAttributeSource::Enhancement {
                location: TeletextEnhancementSource {
                    object: None,
                    command: 1
                },
                ..
            })
        ));
        assert!(matches!(
            pending.second_source(),
            Some(TeletextAttributeSource::Enhancement {
                location: TeletextEnhancementSource {
                    object: None,
                    command: 2
                },
                ..
            })
        ));
    }
}

#[test]
fn exact_command_budget_rejects_original_private_glyph_without_mutating_inputs() {
    let target = local(0xffff);
    let service = service(
        vec![set(2, 8), invoke(target, TeletextObjectType::Passive, None)],
        vec![definition(
            target,
            TeletextObjectType::Passive,
            private_program(1, true, b'A'),
        )],
    );
    let inputs = fixture(&service);
    let original_base = inputs.base.clone();
    let original_trace = inputs.original.clone();
    let original_layers = inputs.layers.clone();
    assert_eq!(execute(&inputs, 7).unwrap().records().len(), 993);
    let error = execute(&inputs, 6).unwrap_err();
    assert_eq!(
        error.reason,
        TeletextAttributeExecutionFailure::CommandBudget { maximum: 6 }
    );
    assert_eq!(
        error.location,
        Some(TeletextEnhancementSource {
            object: Some(target),
            command: 4
        })
    );
    assert_eq!(
        error.root_source,
        Some(TeletextSpatialRootSource::Local { command: 1 })
    );
    assert_eq!(error.invocation.unwrap().get(), 0);
    assert_eq!(inputs.base, original_base);
    assert_eq!(inputs.original, original_trace);
    assert_eq!(inputs.layers, original_layers);
}

#[test]
fn different_same_owner_projection_rejects_exact_source_and_unknown_subcode_rejects_owner() {
    let original_service = service(vec![set(2, 8), glyph(8, b'A')], vec![]);
    let other_service = service(vec![set(2, 8), glyph(8, b'B')], vec![]);
    let inputs = fixture(&original_service);
    let other = fixture(&other_service);
    let original_layers = inputs.layers.clone();
    let error = resolve_teletext_attribute_execution(
        &inputs.base,
        &inputs.original,
        &other.layers,
        &[TeletextColourReference::BLACK; 25],
        2,
    )
    .unwrap_err();
    assert_eq!(
        error.reason,
        TeletextAttributeExecutionFailure::CandidateMismatch { candidate: 81 }
    );
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
    assert_eq!(error.invocation, None);
    assert_eq!(inputs.layers, original_layers);
    let other_owner = TeletextPageAddress {
        page: owner().page,
        subcode: TeletextSubcode::new(0x1235).unwrap(),
    };
    let foreign_base = TeletextBaseDisplay::new(
        other_owner,
        inputs.base.primary(),
        inputs.base.national_option(),
        None,
        inputs
            .base
            .cells()
            .to_vec()
            .into_boxed_slice()
            .try_into()
            .unwrap(),
    )
    .unwrap();
    let error = resolve_teletext_attribute_execution(
        &foreign_base,
        &inputs.original,
        &inputs.layers,
        &[TeletextColourReference::BLACK; 25],
        2,
    )
    .unwrap_err();
    assert_eq!(error.owner, other_owner);
    assert_eq!(
        error.reason,
        TeletextAttributeExecutionFailure::OwnerMismatch { other: owner() }
    );
}

#[test]
fn all_candidate_bound_precedes_correspondence_for_foreign_same_owner_projection() {
    let empty = fixture(&service(vec![], vec![]));
    let populated = fixture(&service(
        vec![set(2, 8), glyph(8, b'A'), glyph(9, b'B')],
        vec![],
    ));
    assert_eq!(execute(&empty, 0).unwrap().records().len(), 992);
    let error = resolve_teletext_attribute_execution(
        &empty.base,
        &empty.original,
        &populated.layers,
        &[TeletextColourReference::BLACK; 25],
        0,
    )
    .unwrap_err();
    assert_eq!(
        error.reason,
        TeletextAttributeExecutionFailure::CandidateBudget {
            count: 994,
            maximum: 992
        }
    );
    assert_eq!(empty.original.events().len(), 0);
    assert_eq!(populated.layers.candidates().len(), 994);
}
