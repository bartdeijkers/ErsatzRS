//! Positioned Annex A.8(3) oracles, separate from interval overlap.

use super::display_geometry_tests::{definition, fixture, glyph, invoke, local, set, Fixture};
use super::*;

fn result(fixture: &Fixture) -> Result<TeletextDisplayGeometry, TeletextDisplayGeometryError> {
    resolve_teletext_display_geometry(
        &fixture.base,
        &fixture.spatial,
        &fixture.glyphs,
        TeletextDisplayGeometryBudget {
            max_candidates: 2048,
            max_events: 128,
            max_invocations: 32,
        },
    )
}

fn empty_calls(count: usize) -> Fixture {
    let target = local(0);
    fixture(
        None,
        vec![invoke(target, TeletextObjectType::Passive, None); count],
        vec![definition(target, TeletextObjectType::Passive, vec![])],
    )
}

#[test]
fn fifteen_empty_calls_are_known_successive_despite_pending_intervals() {
    let geometry = result(&empty_calls(15)).unwrap();
    assert_eq!(geometry.object_open_count().known_intervals(), 0);
    assert_eq!(geometry.object_open_count().pending_intervals(), 15);
    assert_eq!(
        geometry.successive_open_count().maximum_known_successive(),
        15
    );
    assert!(geometry.successive_open_count().pending().is_empty());
}

#[test]
fn sixteenth_empty_call_rejects_with_original_source_and_preserves_inputs() {
    let fixture = empty_calls(16);
    let original = (
        fixture.base.clone(),
        fixture.spatial.clone(),
        fixture.glyphs.clone(),
    );
    let failure = result(&fixture).unwrap_err();
    assert_eq!(
        failure.reason,
        TeletextDisplayGeometryFailure::TooManySuccessiveObjectOpens {
            count: 16,
            maximum: 15
        }
    );
    assert_eq!(failure.position, Some((0, 0)));
    assert_eq!(
        failure.location,
        Some(TeletextEnhancementSource {
            object: None,
            command: 15
        })
    );
    assert_eq!(
        failure.root_source,
        Some(TeletextSpatialRootSource::Local { command: 15 })
    );
    assert_eq!(failure.invocation.unwrap().get(), 15);
    assert_eq!((fixture.base, fixture.spatial, fixture.glyphs), original);
}

#[test]
fn same_stream_character_or_attribute_splits_same_position_call_runs() {
    let target = local(0);
    let writers = [
        glyph(8),
        TeletextEnhancementCommand::AtColumn {
            column: 8,
            operation: TeletextColumnOperation::Foreground {
                colour: TeletextColourReference::WHITE,
            },
        },
    ];
    for writer in writers {
        let mut commands = vec![set(1, 8)];
        commands.extend([invoke(target, TeletextObjectType::Passive, None); 15]);
        commands.push(writer);
        commands.extend([invoke(target, TeletextObjectType::Passive, None); 15]);
        let geometry = result(&fixture(
            None,
            commands,
            vec![definition(target, TeletextObjectType::Passive, vec![])],
        ))
        .unwrap();
        assert_eq!(
            geometry.successive_open_count().maximum_known_successive(),
            15
        );
        assert!(geometry.successive_open_count().pending().is_empty());
    }
}

#[test]
fn mixed_types_targets_and_offsets_share_one_call_limit() {
    let kinds = [
        TeletextObjectType::Active,
        TeletextObjectType::Adaptive,
        TeletextObjectType::Passive,
    ];
    let definitions = kinds
        .iter()
        .enumerate()
        .map(|(index, &kind)| definition(local(u16::try_from(index).unwrap()), kind, vec![]))
        .collect();
    let commands = (0..16)
        .map(|index| {
            invoke(
                local(u16::try_from(index % 3).unwrap()),
                kinds[index % 3],
                Some((1, u8::try_from(index).unwrap())),
            )
        })
        .collect();
    let failure = result(&fixture(None, commands, definitions)).unwrap_err();
    assert!(matches!(
        failure.reason,
        TeletextDisplayGeometryFailure::TooManySuccessiveObjectOpens { count: 16, .. }
    ));
    // Offset affects the fresh child origin, never the caller AP diagnostic.
    assert_eq!(failure.position, Some((0, 0)));
    assert_eq!(failure.invocation.unwrap().get(), 15);
}

#[test]
fn a_strictly_later_write_cannot_reset_sixteen_earlier_openings() {
    let target = local(0);
    let mut commands = vec![invoke(target, TeletextObjectType::Passive, None); 16];
    commands.extend([set(1, 8), glyph(8)]);
    let failure = result(&fixture(
        None,
        commands,
        vec![definition(target, TeletextObjectType::Passive, vec![])],
    ))
    .unwrap_err();
    assert!(matches!(
        failure.reason,
        TeletextDisplayGeometryFailure::TooManySuccessiveObjectOpens { count: 16, .. }
    ));
    assert_eq!(failure.position, Some((0, 0)));
    assert_eq!(failure.location.unwrap().command, 15);
}

#[test]
fn same_emitter_potential_reset_does_not_hide_a_definite_sixteenth_opening() {
    let target = local(0);
    for writer in [
        TeletextEnhancementCommand::SetScreenColour {
            colour: TeletextColourReference::BLACK,
        },
        TeletextEnhancementCommand::SetRowColour {
            row: 0,
            colour: TeletextColourReference::BLACK,
            following_rows: false,
        },
        TeletextEnhancementCommand::AtColumn {
            column: 0,
            operation: TeletextColumnOperation::Foreground {
                colour: TeletextColourReference::WHITE,
            },
        },
    ] {
        for before in [false, true] {
            let mut commands = vec![invoke(target, TeletextObjectType::Passive, None); 16];
            commands.insert(if before { 0 } else { 16 }, writer);
            let input = fixture(
                None,
                commands,
                vec![definition(target, TeletextObjectType::Passive, vec![])],
            );
            let original = (
                input.base.clone(),
                input.spatial.clone(),
                input.glyphs.clone(),
            );
            let observed = result(&input);
            assert!(
                observed.is_err(),
                "A8(3): same-emitter reset hid16 definite opens; before={before}"
            );
            let failure = observed.unwrap_err();
            assert_eq!(
                failure.reason,
                TeletextDisplayGeometryFailure::TooManySuccessiveObjectOpens {
                    count: 16,
                    maximum: 15
                }
            );
            assert_eq!(
                failure.location,
                Some(TeletextEnhancementSource {
                    object: None,
                    command: if before { 16 } else { 15 }
                })
            );
            assert_eq!(failure.position, Some((0, 0)));
            assert_eq!(failure.invocation.unwrap().get(), 15);
            assert_eq!((input.base, input.spatial, input.glyphs), original);
        }
    }
}

#[test]
fn plane_colour_writes_remain_potential_resets_without_an_invented_cell() {
    let target = local(0);
    for writer in [
        TeletextEnhancementCommand::SetScreenColour {
            colour: TeletextColourReference::BLACK,
        },
        TeletextEnhancementCommand::SetRowColour {
            row: 2,
            colour: TeletextColourReference::WHITE,
            following_rows: true,
        },
    ] {
        let mut commands = vec![set(1, 8)];
        commands.extend([invoke(target, TeletextObjectType::Passive, None); 15]);
        commands.push(writer);
        commands.extend([invoke(target, TeletextObjectType::Passive, None); 15]);
        let geometry = result(&fixture(
            None,
            commands,
            vec![definition(target, TeletextObjectType::Passive, vec![])],
        ))
        .unwrap();
        let count = geometry.successive_open_count();
        // The two definite runs of fifteen remain provable without assuming
        // that this potential reset actually writes any addressed cell.
        assert_eq!(count.maximum_known_successive(), 15);
        assert_eq!(count.pending().len(), 1);
        assert_eq!(count.pending()[0].location().command, 16);
        assert_eq!(
            count.pending()[0].reason(),
            TeletextSuccessiveOpenPendingReason::UnpositionedPotentialReset
        );
    }
}

#[test]
fn child_write_cannot_hide_the_sixteenth_main_open_that_invokes_it() {
    let empty = local(0);
    let writer = local(1);
    let mut commands = vec![set(1, 8)];
    commands.extend([invoke(empty, TeletextObjectType::Passive, None); 15]);
    commands.push(invoke(writer, TeletextObjectType::Active, None));
    let input = fixture(
        None,
        commands,
        vec![
            definition(empty, TeletextObjectType::Passive, vec![]),
            definition(writer, TeletextObjectType::Active, vec![glyph(0)]),
        ],
    );
    let original = (
        input.base.clone(),
        input.spatial.clone(),
        input.glyphs.clone(),
    );
    let observed = result(&input);
    assert!(
        observed.is_err(),
        "A8(3): a child reset hid its own sixteenth Main opening"
    );
    let error = observed.unwrap_err();
    assert_eq!(
        error.reason,
        TeletextDisplayGeometryFailure::TooManySuccessiveObjectOpens {
            count: 16,
            maximum: 15
        }
    );
    assert_eq!(error.position, Some((1, 8)));
    assert_eq!(
        error.location,
        Some(TeletextEnhancementSource {
            object: None,
            command: 16
        })
    );
    assert_eq!(
        error.root_source,
        Some(TeletextSpatialRootSource::Local { command: 16 })
    );
    assert_eq!(error.invocation.unwrap().get(), 15);
    assert_eq!((input.base, input.spatial, input.glyphs), original);
}

#[test]
fn earlier_child_writer_keeps_mixed_order_pending_instead_of_forcing_a_run() {
    let empty = local(0);
    let writer = local(1);
    let mut commands = vec![set(1, 8), invoke(writer, TeletextObjectType::Active, None)];
    commands.extend([invoke(empty, TeletextObjectType::Passive, None); 15]);
    let input = fixture(
        None,
        commands,
        vec![
            definition(empty, TeletextObjectType::Passive, vec![]),
            definition(writer, TeletextObjectType::Active, vec![glyph(0)]),
        ],
    );
    let geometry = result(&input).unwrap();
    let count = geometry.successive_open_count();
    assert_eq!(count.maximum_known_successive(), 1);
    assert_eq!(count.pending().len(), 17);
    assert!(count.pending().iter().all(|p| p.reason()
        == TeletextSuccessiveOpenPendingReason::CrossEmitterOrder
        && p.position() == (1, 8)));
    let reset = &count.pending()[1];
    assert_eq!(reset.location().object, Some(writer));
    assert_eq!(reset.location().command, 0);
    assert_eq!(
        reset.root_source(),
        TeletextSpatialRootSource::Local { command: 1 }
    );
    assert_eq!(reset.invocation().unwrap().get(), 0);
}

#[test]
fn position_and_charset_setup_do_not_reset_successive_calls() {
    let target = local(0);
    let setup = TeletextEnhancementCommand::AtColumn {
        column: 8,
        operation: TeletextColumnOperation::DesignateCharset {
            designation: TeletextEnhancementCharsetDesignation::new(0).unwrap(),
        },
    };
    for setup in [set(1, 8), setup] {
        let mut commands = vec![invoke(target, TeletextObjectType::Passive, None); 15];
        commands.extend([setup, invoke(target, TeletextObjectType::Passive, None)]);
        let failure = result(&fixture(
            None,
            commands,
            vec![definition(target, TeletextObjectType::Passive, vec![])],
        ))
        .unwrap_err();
        assert!(matches!(
            failure.reason,
            TeletextDisplayGeometryFailure::TooManySuccessiveObjectOpens { count: 16, .. }
        ));
        assert_eq!(failure.location.unwrap().command, 16);
    }
}

#[test]
fn positioned_child_writes_between_calls_preserve_valid_successive_admission() {
    let parent = local(0);
    let child = local(1);
    // Distinct one-row child intervals keep the simultaneous-open maximum below
    // sixteen. Recursive preorder nevertheless contains a glyph after every call.
    let mut body = Vec::new();
    for row in 1..=16 {
        body.extend([
            set(row, 0),
            invoke(child, TeletextObjectType::Passive, Some((0, 8))),
        ]);
    }
    let fixture = fixture(
        None,
        vec![invoke(parent, TeletextObjectType::Active, None)],
        vec![
            definition(parent, TeletextObjectType::Active, body),
            definition(child, TeletextObjectType::Passive, vec![glyph(0)]),
        ],
    );
    let geometry = result(&fixture).unwrap();
    assert_eq!(geometry.object_open_count().maximum_known_open(), 2);
    assert_eq!(
        geometry.successive_open_count().maximum_known_successive(),
        2
    );
    assert!(geometry.successive_open_count().pending().is_empty());
}

#[test]
fn out_of_plane_potential_write_is_pending_rather_than_an_assumed_absence() {
    let parent = local(0);
    let child = local(1);
    let mut body = vec![invoke(child, TeletextObjectType::Passive, None); 15];
    body.extend([set(24, 0), glyph(0)]);
    body.extend([invoke(child, TeletextObjectType::Passive, None); 15]);
    let geometry = result(&fixture(
        None,
        vec![invoke(parent, TeletextObjectType::Active, Some((1, 0)))],
        vec![
            definition(parent, TeletextObjectType::Active, body),
            definition(child, TeletextObjectType::Passive, vec![]),
        ],
    ))
    .unwrap();
    let count = geometry.successive_open_count();
    assert_eq!(count.maximum_known_successive(), 0);
    assert_eq!(count.pending().len(), 16);
    assert_eq!(
        count
            .pending()
            .iter()
            .filter(|witness| {
                witness.reason() == TeletextSuccessiveOpenPendingReason::UnpositionedOpening
                    && witness.position() == (25, 0)
            })
            .count(),
        15
    );
    let pending = count
        .pending()
        .iter()
        .find(|witness| {
            witness.reason() == TeletextSuccessiveOpenPendingReason::UnpositionedPotentialReset
        })
        .unwrap();
    assert_eq!(pending.position(), (25, 0));
    assert_eq!(pending.location().object, Some(parent));
    assert_eq!(pending.location().command, 16);
    assert_eq!(pending.invocation().unwrap().get(), 0);
    assert_eq!(
        pending.reason(),
        TeletextSuccessiveOpenPendingReason::UnpositionedPotentialReset
    );
}
