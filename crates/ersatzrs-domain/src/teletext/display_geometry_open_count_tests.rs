//! Inclusive raw Annex A.8 counts, independent of visual/object admission.

use super::display_geometry_tests::{
    definition, fixture, glyph, invoke, local, panels, presentation, resolve, set, Fixture,
};
use super::*;

fn mixed(count: usize, pending: bool) -> Fixture {
    let active = local(0xffff);
    let adaptive = local(0x0100);
    let passive = local(0);
    let mut objects = vec![
        definition(active, TeletextObjectType::Active, vec![glyph(0)]),
        definition(adaptive, TeletextObjectType::Adaptive, vec![glyph(0)]),
        definition(passive, TeletextObjectType::Passive, vec![glyph(0)]),
    ];
    let mut commands: Vec<_> = (0..count)
        .map(|ordinal| {
            let (target, object_type) = match ordinal {
                0..=4 => (active, TeletextObjectType::Active),
                5..=9 => (adaptive, TeletextObjectType::Adaptive),
                _ => (passive, TeletextObjectType::Passive),
            };
            // Caller stays0,0; distinct offset glyphs avoid Adaptive same-cell overlap.
            invoke(
                target,
                object_type,
                Some((1, u8::try_from(ordinal).unwrap())),
            )
        })
        .collect();
    if pending {
        let empty = local(1);
        objects.push(definition(empty, TeletextObjectType::Passive, vec![]));
        commands.insert(15, invoke(empty, TeletextObjectType::Passive, None));
    }
    fixture(None, commands, objects)
}
fn failure(fixture: &Fixture) -> TeletextDisplayGeometryError {
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
    .unwrap_err()
}
fn assert_sixteenth(
    error: &TeletextDisplayGeometryError,
    command: usize,
    instance: usize,
    position: (u16, u16),
) {
    assert_eq!(
        error.reason,
        TeletextDisplayGeometryFailure::TooManyOpenObjects {
            count: 16,
            maximum: 15
        }
    );
    assert_eq!(error.position, Some(position));
    assert_eq!(
        error.location,
        Some(TeletextEnhancementSource {
            object: None,
            command
        })
    );
    assert_eq!(
        error.root_source,
        Some(TeletextSpatialRootSource::Local { command })
    );
    assert_eq!(error.invocation.unwrap().get(), instance);
}

#[test]
fn all_three_types_share_one_fifteen_object_limit_with_distinct_written_cells() {
    let fixture = mixed(15, false);
    let geometry = resolve(&fixture);
    let count = geometry.object_open_count();
    assert_eq!(
        (
            count.known_intervals(),
            count.pending_intervals(),
            count.maximum_known_open()
        ),
        (15, 0, 15)
    );
    assert_eq!(geometry.object_regions().len(), 15);
    for (ordinal, region) in geometry.object_regions().iter().enumerate() {
        assert_eq!(region.instance().id().get(), ordinal);
        assert_eq!(
            region.interval(),
            TeletextObjectAddressInterval::Known {
                start: (0, 0),
                finish: (1, u16::try_from(ordinal).unwrap())
            }
        );
        assert_eq!(
            region.instance().target(),
            local(match ordinal {
                0..=4 => 0xffff,
                5..=9 => 0x0100,
                _ => 0,
            })
        );
    }
}

#[test]
fn sixteenth_mixed_instance_rejects_original_call_before_offset_and_preserves_inputs() {
    let fixture = mixed(16, false);
    let before = (
        fixture.base.clone(),
        fixture.spatial.clone(),
        fixture.glyphs.clone(),
    );
    let error = failure(&fixture);
    assert_sixteenth(&error, 15, 15, (0, 0));
    assert_eq!((fixture.base, fixture.spatial, fixture.glyphs), before);
}

fn touching(next_column: u8) -> Fixture {
    let target = local(0xffff);
    let mut commands: Vec<_> = (0..15)
        .map(|_| invoke(target, TeletextObjectType::Passive, Some((1, 8))))
        .collect();
    commands.push(set(1, next_column));
    commands.push(invoke(target, TeletextObjectType::Passive, Some((1, 0))));
    fixture(
        None,
        commands,
        vec![definition(
            target,
            TeletextObjectType::Passive,
            vec![glyph(0)],
        )],
    )
}

#[test]
fn shared_endpoint_is_inclusive_but_next_raw_cell_is_no_longer_overlapping() {
    assert_sixteenth(&failure(&touching(8)), 16, 15, (1, 8));
    let geometry = resolve(&touching(9));
    let count = geometry.object_open_count();
    assert_eq!(
        (
            count.known_intervals(),
            count.pending_intervals(),
            count.maximum_known_open()
        ),
        (16, 0, 15)
    );
    assert_eq!(
        geometry.object_regions()[15].interval(),
        TeletextObjectAddressInterval::Known {
            start: (1, 9),
            finish: (2, 9)
        }
    );
}

#[test]
fn twenty_sequential_calls_of_same_full_id_have_maximum_one_known_open() {
    let target = local(0xffff);
    let commands = (0..20)
        .flat_map(|index| {
            [
                set(1, index * 2),
                invoke(target, TeletextObjectType::Active, None),
            ]
        })
        .collect();
    let geometry = resolve(&fixture(
        None,
        commands,
        vec![definition(
            target,
            TeletextObjectType::Active,
            vec![glyph(1)],
        )],
    ));
    let count = geometry.object_open_count();
    assert_eq!(
        (
            count.known_intervals(),
            count.pending_intervals(),
            count.maximum_known_open()
        ),
        (20, 0, 1)
    );
    for (ordinal, region) in geometry.object_regions().iter().enumerate() {
        let column = u16::try_from(ordinal).unwrap() * 2;
        assert_eq!(region.instance().target(), target);
        assert_eq!(region.instance().id().get(), ordinal);
        assert_eq!(
            region.interval(),
            TeletextObjectAddressInterval::Known {
                start: (1, column),
                finish: (1, column + 1)
            }
        );
    }
}

#[test]
fn disabled_panels_do_not_filter_known_raw_intervals_out_of_open_count() {
    let target = local(0xffff);
    let raw = panels(false, false, 13, TeletextPanelStatus::Level25And35);
    let make = |count| {
        fixture(
            Some(presentation(raw)),
            (0..count)
                .map(|index| invoke(target, TeletextObjectType::Passive, Some((1, 40 + index))))
                .collect(),
            vec![definition(
                target,
                TeletextObjectType::Passive,
                vec![glyph(0)],
            )],
        )
    };
    let geometry = resolve(&make(15));
    assert_eq!(geometry.object_open_count().known_intervals(), 15);
    assert_eq!(geometry.object_open_count().maximum_known_open(), 15);
    assert_eq!(geometry.total_width(), 40);
    for region in geometry.object_regions() {
        assert_eq!(
            region.area_pending(),
            Some(TeletextObjectAreaPending::UnplacedAddress)
        );
        assert_eq!(region.written_area(), None);
    }
    assert_sixteenth(&failure(&make(16)), 15, 15, (0, 0));
}

#[test]
fn address_only_endpoints_count_without_definite_written_glyphs() {
    let target = local(0xffff);
    let make = |count| {
        fixture(
            None,
            (0..count)
                .map(|_| invoke(target, TeletextObjectType::Passive, Some((1, 8))))
                .collect(),
            vec![definition(
                target,
                TeletextObjectType::Passive,
                vec![set(0, 0)],
            )],
        )
    };
    let geometry = resolve(&make(15));
    assert_eq!(geometry.object_open_count().maximum_known_open(), 15);
    assert_eq!(geometry.object_open_count().known_intervals(), 15);
    for region in geometry.object_regions() {
        assert_eq!(region.written_area(), None);
        assert_eq!(
            region.area_pending(),
            Some(TeletextObjectAreaPending::NoDefiniteWrites)
        );
        assert!(geometry.direct_writes(region.instance().id()).is_empty());
    }
    assert_sixteenth(&failure(&make(16)), 15, 15, (0, 0));
}

#[test]
fn pending_interval_is_retained_and_cannot_hide_sixteen_known_intervals() {
    let geometry = resolve(&mixed(15, true));
    let count = geometry.object_open_count();
    assert_eq!(
        (
            count.known_intervals(),
            count.pending_intervals(),
            count.maximum_known_open()
        ),
        (15, 1, 15)
    );
    assert_eq!(
        geometry.object_regions()[15].interval(),
        TeletextObjectAddressInterval::Pending {
            reason: TeletextObjectIntervalPending::NoDirectAddress,
            endpoint: None
        }
    );
    assert_sixteenth(&failure(&mixed(16, true)), 16, 16, (0, 0));
}

#[test]
fn parent_interval_uses_last_own_address_and_summary_retains_outside_pending() {
    let parent = local(0xffff);
    let child = local(0x0100);
    let outside = local(0);
    let input = fixture(
        None,
        vec![
            set(1, 8),
            invoke(parent, TeletextObjectType::Active, None),
            invoke(outside, TeletextObjectType::Passive, Some((23, 71))),
        ],
        vec![
            definition(
                parent,
                TeletextObjectType::Active,
                vec![
                    glyph(0),
                    invoke(child, TeletextObjectType::Adaptive, Some((1, 0))),
                    glyph(1),
                ],
            ),
            definition(child, TeletextObjectType::Adaptive, vec![glyph(0)]),
            definition(
                outside,
                TeletextObjectType::Passive,
                vec![set(24, 0), glyph(0)],
            ),
        ],
    );
    let geometry = resolve(&input);
    let count = geometry.object_open_count();
    assert_eq!(
        (
            count.known_intervals(),
            count.pending_intervals(),
            count.maximum_known_open()
        ),
        (2, 1, 2)
    );
    assert_eq!(
        geometry.object_regions()[0].interval(),
        TeletextObjectAddressInterval::Known {
            start: (1, 8),
            finish: (1, 9)
        }
    );
    assert_eq!(
        geometry.object_regions()[1].interval(),
        TeletextObjectAddressInterval::Known {
            start: (1, 8),
            finish: (2, 8)
        }
    );
    assert_eq!(
        geometry.object_regions()[2].interval(),
        TeletextObjectAddressInterval::Pending {
            reason: TeletextObjectIntervalPending::OutsideDisplay,
            endpoint: Some((48, 79))
        }
    );
    let empty = resolve(&fixture(None, vec![], vec![]));
    let count = empty.object_open_count();
    assert_eq!(
        (
            count.known_intervals(),
            count.pending_intervals(),
            count.maximum_known_open()
        ),
        (0, 0, 0)
    );
}
