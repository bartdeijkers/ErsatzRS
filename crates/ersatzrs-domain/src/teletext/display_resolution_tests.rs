//! Independent C.3 occupied-cell and original-source oracles.

use super::attribute_execution_tests::{
    at, definition, foreground, glyph, invoke, local, owner, service, set,
};
use super::base_footprints_tests::{attributes, empty_rows, finish, mixed_base, put};
use super::*;

struct Fixture {
    row_colours: [TeletextColourReference; 25],
    base: TeletextBaseDisplay,
    base_footprints: TeletextBaseFootprints,
    original: TeletextSpatialTrace,
    layers: TeletextGlyphLayers,
    attributes: TeletextAttributeExecution,
    intent: TeletextGlyphIntent,
    drcs: TeletextDrcsExecution,
    geometry: TeletextDisplayGeometry,
}

impl Fixture {
    fn new(base: TeletextBaseDisplay, service: &TeletextSemanticService) -> Self {
        let colours = [TeletextColourReference::BLACK; 25];
        let base_footprints = resolve_teletext_base_footprints(&base, &colours).unwrap();
        let original = resolve_teletext_spatial_trace(
            service,
            owner(),
            TeletextSpatialTraceBudget {
                max_events: 4096,
                max_invocations: 64,
            },
        )
        .unwrap();
        let layers = build_teletext_glyph_layers(&base, &original, 4096).unwrap();
        let attributes =
            resolve_teletext_attribute_execution(&base, &original, &layers, &colours, 4096)
                .unwrap();
        let charset = resolve_teletext_charset_execution(&base, &original, &layers, 4096).unwrap();
        let intent = resolve_teletext_glyph_intent(&charset, 8192).unwrap();
        let drcs = resolve_teletext_drcs_execution(
            service,
            &intent,
            TeletextDrcsExecutionBudget {
                max_candidates: 8192,
                max_references: 128,
            },
        )
        .unwrap();
        let geometry = resolve_teletext_display_geometry(
            &base,
            &original,
            &layers,
            TeletextDisplayGeometryBudget {
                max_candidates: 8192,
                max_events: 4096,
                max_invocations: 64,
            },
        )
        .unwrap();
        Self {
            row_colours: colours,
            base,
            base_footprints,
            original,
            layers,
            attributes,
            intent,
            drcs,
            geometry,
        }
    }

    fn inputs(&self) -> TeletextDisplayResolutionInputs<'_> {
        TeletextDisplayResolutionInputs {
            base: &self.base,
            base_footprints: &self.base_footprints,
            original: &self.original,
            layers: &self.layers,
            attributes: &self.attributes,
            intent: &self.intent,
            drcs: &self.drcs,
            geometry: &self.geometry,
            row_colours: &self.row_colours,
        }
    }
}

fn budget() -> TeletextDisplayResolutionBudget {
    TeletextDisplayResolutionBudget {
        max_candidates: 8192,
        max_commands: 4096,
        max_writers: 65536,
        max_origins: 8192,
        max_updates: 1_000_000,
    }
}

fn blank() -> TeletextBaseDisplay {
    finish(empty_rows(7, 0), None)
}

fn size(column: u8, height: bool, width: bool) -> TeletextEnhancementCommand {
    at(
        column,
        TeletextColumnOperation::Attributes {
            double_height: height,
            double_width: width,
            boxed: false,
            conceal: false,
            invert: false,
            underline_or_separated_mosaics: false,
        },
    )
}

fn background(column: u8, colour: u8) -> TeletextEnhancementCommand {
    at(
        column,
        TeletextColumnOperation::Background {
            colour: TeletextColourReference::new(colour).unwrap(),
        },
    )
}

fn resolve(fixture: &Fixture) -> TeletextDisplayResolution {
    resolve_teletext_display_resolution(fixture.inputs(), budget()).unwrap()
}

fn colour(result: &TeletextDisplayResolution, row: u8, column: u8) -> Option<u8> {
    result
        .cell(row, column)
        .unwrap()
        .attributes()
        .foreground()
        .value()
        .map(TeletextColourReference::get)
}

fn origin_at(result: &TeletextDisplayResolution, row: u8, column: u8) -> &TeletextDisplayOrigin {
    &result.origins()[result.cell(row, column).unwrap().origin().unwrap()]
}

#[test]
fn double_width_repeats_origin_attributes_and_defers_hidden_writer_without_replacing_its_source() {
    let service = service(
        vec![
            set(1, 8),
            foreground(8, 1),
            background(8, 4),
            size(8, false, true),
            glyph(8, b'A'),
            foreground(9, 4),
            size(10, false, false),
            glyph(10, b'B'),
        ],
        vec![],
    );
    let fixture = Fixture::new(blank(), &service);
    let original = fixture.original.clone();
    let result = resolve(&fixture);
    assert_eq!(
        origin_at(&result, 1, 8).effective_size(),
        Some(TeletextDisplaySize::DoubleWidth)
    );
    assert_eq!(
        result.cell(1, 8).unwrap().origin(),
        result.cell(1, 9).unwrap().origin()
    );
    for column in [8, 9] {
        let attributes = result.cell(1, column).unwrap().attributes();
        assert_eq!(attributes.foreground().value().unwrap().get(), 1);
        assert_eq!(attributes.background().value().unwrap().get(), 4);
        assert_eq!(attributes.double_width().value(), Some(true));
    }
    assert_eq!(colour(&result, 1, 10), Some(4));
    let writer = result
        .cell(1, 10)
        .unwrap()
        .attributes()
        .foreground()
        .writer()
        .unwrap();
    assert_eq!(writer.placed_position(), (1, 9));
    assert_eq!(writer.requested_start(), (1, 9));
    assert_eq!(writer.effective_start(), (1, 10));
    assert_eq!(fixture.original, original);
}

#[test]
fn later_visible_foreground_supersedes_deferred_hidden_foreground_at_first_free_cell() {
    let service = service(
        vec![
            set(1, 8),
            foreground(8, 1),
            size(8, false, true),
            glyph(8, b'A'),
            foreground(9, 4),
            foreground(10, 2),
            size(10, false, false),
            glyph(10, b'B'),
            foreground(11, 7),
        ],
        vec![],
    );
    let result = resolve(&Fixture::new(blank(), &service));
    // A literal complete-plane colour oracle: no whole-row overwrite can pass.
    for row in 0_u8..25 {
        for column in 0_u8..40 {
            if row == 0 && column < 8 {
                continue;
            }
            let expected = if row == 1 && (8..=9).contains(&column) {
                1
            } else if row == 1 && column == 10 {
                2
            } else {
                7
            };
            assert_eq!(
                colour(&result, row, column),
                Some(expected),
                "cell {row},{column}"
            );
        }
    }
    let writer = result
        .cell(1, 10)
        .unwrap()
        .attributes()
        .foreground()
        .writer()
        .unwrap();
    assert_eq!(writer.placed_position(), (1, 10));
    assert_eq!(writer.effective_start(), (1, 10));
}

#[test]
fn hidden_same_position_conflict_stays_pending_until_a_later_visible_writer_supersedes_it() {
    for visible in [false, true] {
        let mut commands = vec![
            set(1, 8),
            foreground(8, 1),
            size(8, false, true),
            glyph(8, b'A'),
            foreground(9, 4),
            foreground(9, 3),
        ];
        if visible {
            commands.push(foreground(10, 2));
        }
        commands.extend([size(10, false, false), glyph(10, b'B')]);
        let result = resolve(&Fixture::new(blank(), &service(commands, vec![])));
        let field = result.cell(1, 10).unwrap().attributes().foreground();
        if visible {
            assert_eq!(field.value().unwrap().get(), 2);
        } else {
            assert_eq!(field.value(), None);
            assert!(field.pending().is_some());
        }
        assert_eq!(colour(&result, 1, 9), Some(1));
    }
}

#[test]
fn double_size_conflict_reduces_only_width_and_keeps_unconflicted_height() {
    let commands = vec![
        set(1, 9),
        size(9, true, false),
        glyph(9, b'A'),
        size(10, false, false),
        set(2, 8),
        size(8, true, true),
        glyph(8, b'B'),
        size(9, false, false),
    ];
    let result = resolve(&Fixture::new(blank(), &service(commands, vec![])));
    let reduced = origin_at(&result, 2, 8);
    assert_eq!(
        reduced.requested_size(),
        Some(TeletextDisplaySize::DoubleSize)
    );
    assert_eq!(
        reduced.effective_size(),
        Some(TeletextDisplaySize::DoubleHeight)
    );
    assert_eq!(
        result.cell(2, 8).unwrap().origin(),
        result.cell(3, 8).unwrap().origin()
    );
    assert_eq!(
        result.cell(1, 9).unwrap().origin(),
        result.cell(2, 9).unwrap().origin()
    );
    assert_ne!(
        result.cell(3, 9).unwrap().origin(),
        result.cell(2, 8).unwrap().origin()
    );
}

#[test]
fn passive_normal_overlays_one_part_without_repainting_the_other_lower_fragments() {
    for height in [false, true] {
        let passive = local(0xffff);
        let mut commands = vec![
            set(1, 8),
            foreground(8, 1),
            background(8, 4),
            size(8, height, !height),
            glyph(8, b'L'),
        ];
        let (row, column) = if height { (2, 8) } else { (1, 9) };
        commands.extend([
            set(row, column),
            invoke(passive, TeletextObjectType::Passive, None),
        ]);
        let service = service(
            commands,
            vec![definition(
                passive,
                TeletextObjectType::Passive,
                vec![foreground(0, 2), glyph(0, b'P')],
            )],
        );
        let result = resolve(&Fixture::new(blank(), &service));
        assert_eq!(colour(&result, 1, 8), Some(1));
        assert_eq!(colour(&result, row, column), Some(2));
        assert_eq!(
            origin_at(&result, row, column).effective_size(),
            Some(TeletextDisplaySize::Normal)
        );
        assert_ne!(
            result.cell(1, 8).unwrap().origin(),
            result.cell(row, column).unwrap().origin()
        );
    }
}

#[test]
fn spacing_double_height_ignores_lower_coded_attributes_and_only_actual_glyph_repaints_background()
{
    let base = mixed_base();
    let service = service(
        vec![
            set(2, 20),
            foreground(20, 3),
            background(20, 2),
            glyph(21, b'E'),
        ],
        vec![],
    );
    let fixture = Fixture::new(base, &service);
    let result = resolve(&fixture);
    // Lower coded Green/Flash/DH and Z are ignored for this complete lower row.
    assert_eq!(fixture.base.cell(2, 39).unwrap().code(), b'Z');
    for column in [0, 1, 3, 10, 20, 22, 39] {
        assert_eq!(
            result
                .cell(2, column)
                .unwrap()
                .attributes()
                .background()
                .value()
                .unwrap()
                .get(),
            if column >= 2 { 4 } else { 0 }
        );
    }
    // A non-spacing background command alone does not repaint background-only20.
    assert_eq!(
        result
            .cell(2, 20)
            .unwrap()
            .attributes()
            .background()
            .value()
            .unwrap()
            .get(),
        4
    );
    let glyph_attributes = result.cell(2, 21).unwrap().attributes();
    assert_eq!(glyph_attributes.foreground().value().unwrap().get(), 3);
    assert_eq!(glyph_attributes.background().value().unwrap().get(), 2);
    assert_eq!(
        glyph_attributes.flash().value().unwrap().mode(),
        TeletextFlashMode::Steady
    );
    assert_eq!(glyph_attributes.double_height().value(), Some(false));
    assert_eq!(
        result.cell(1, 8).unwrap().origin(),
        result.cell(2, 8).unwrap().origin()
    );
    assert_ne!(
        result.cell(2, 21).unwrap().origin(),
        result.cell(1, 21).unwrap().origin()
    );
}

fn green_lower_base() -> TeletextBaseDisplay {
    let mut rows = empty_rows(7, 0);
    let control = TeletextBaseSpacingControl::AlphaColour {
        colour: TeletextColour::Green,
    };
    put(&mut rows, 2, 0, 2, Some(control), attributes(2, 7, 0));
    for column in 1..40 {
        let mut values = attributes(2, 7, 0).values();
        let mut sources = attributes(2, 7, 0).sources();
        values.raw_foreground = 2;
        values.foreground = TeletextColourReference::new(2).unwrap();
        let green = super::base_footprints_tests::source(2, 0, control);
        sources.foreground = green;
        sources.graphics = green;
        sources.conceal = green;
        put(
            &mut rows,
            2,
            column,
            if column == 20 { b'Z' } else { b' ' },
            None,
            TeletextBaseDisplayAttributes::new(values, sources).unwrap(),
        );
    }
    finish(rows, None)
}

#[test]
fn nonspacing_double_height_keeps_lower_coded_glyph_and_control_outside_its_bottom_part() {
    let service = service(
        vec![
            set(1, 8),
            foreground(8, 1),
            size(8, true, false),
            glyph(8, b'A'),
            size(9, false, false),
        ],
        vec![],
    );
    let fixture = Fixture::new(green_lower_base(), &service);
    let result = resolve(&fixture);
    assert_eq!(
        result.cell(1, 8).unwrap().origin(),
        result.cell(2, 8).unwrap().origin()
    );
    assert_eq!(colour(&result, 2, 8), Some(1));
    assert_eq!(colour(&result, 2, 20), Some(2));
    let origin = origin_at(&result, 2, 20);
    let candidate = fixture.layers.candidates()[origin.candidate_index()];
    assert_eq!(candidate.base().unwrap().code(), b'Z');
    assert_eq!(origin.effective_size(), Some(TeletextDisplaySize::Normal));
    assert_eq!(
        fixture.base_footprints.row(2).unwrap().suppressed_by(),
        None
    );
}

#[test]
fn adaptive_inherits_size_at_underlying_origin_and_explicit_normal_replaces_the_display_group() {
    for explicit in [false, true] {
        let adaptive = local(0x0100);
        let mut body = vec![foreground(0, 2)];
        if explicit {
            body.push(size(0, false, false));
        }
        body.push(glyph(0, b'A'));
        let service = service(
            vec![
                set(1, 8),
                size(8, false, true),
                glyph(8, b'L'),
                invoke(adaptive, TeletextObjectType::Adaptive, None),
            ],
            vec![definition(adaptive, TeletextObjectType::Adaptive, body)],
        );
        let result = resolve(&Fixture::new(blank(), &service));
        let origin = origin_at(&result, 1, 8);
        assert_eq!(
            origin.effective_size(),
            Some(if explicit {
                TeletextDisplaySize::Normal
            } else {
                TeletextDisplaySize::DoubleWidth
            })
        );
        assert_eq!(colour(&result, 1, 8), Some(2));
        assert_eq!(
            result.cell(1, 8).unwrap().origin() == result.cell(1, 9).unwrap().origin(),
            !explicit
        );
    }
}

#[test]
fn passive_private_size_and_all_origin_attributes_cover_each_part_and_start_fresh_per_instance() {
    let passive = local(0xffff);
    let service = service(
        vec![
            set(2, 8),
            invoke(passive, TeletextObjectType::Passive, None),
            set(4, 12),
            invoke(passive, TeletextObjectType::Passive, None),
        ],
        vec![definition(
            passive,
            TeletextObjectType::Passive,
            vec![
                glyph(0, b'W'),
                foreground(1, 1),
                background(1, 4),
                size(1, true, true),
                glyph(1, b'P'),
            ],
        )],
    );
    let fixture = Fixture::new(blank(), &service);
    let result = resolve(&fixture);
    assert_eq!(result.origins().len(), fixture.layers.candidates().len());
    for (index, origin) in result.origins().iter().enumerate() {
        assert_eq!(origin.candidate_index(), index);
    }
    for (row, column) in [(2, 8), (4, 12)] {
        assert_eq!(colour(&result, row, column), Some(7));
        let origin = result.cell(row, column + 1).unwrap().origin();
        for (part_row, part_column) in [
            (row, column + 1),
            (row, column + 2),
            (row + 1, column + 1),
            (row + 1, column + 2),
        ] {
            assert_eq!(result.cell(part_row, part_column).unwrap().origin(), origin);
            let attributes = result.cell(part_row, part_column).unwrap().attributes();
            assert_eq!(attributes.foreground().value().unwrap().get(), 1);
            assert_eq!(attributes.background().value().unwrap().get(), 4);
            assert_eq!(attributes.double_height().value(), Some(true));
            assert_eq!(attributes.double_width().value(), Some(true));
        }
    }
}

#[test]
fn side_panel_display_order_is_visual_left_main_right_and_never_raw_address_order() {
    use super::display_geometry_tests::{panels, presentation};
    let defaults = presentation(panels(true, true, 4, TeletextPanelStatus::Level25And35));
    let base = finish(empty_rows(7, 0), Some(defaults));
    let left = local(0xffff);
    let right = local(0x0100);
    let service = service(
        vec![
            set(1, 0),
            glyph(0, b'M'),
            invoke(right, TeletextObjectType::Passive, Some((0, 40))),
            invoke(left, TeletextObjectType::Passive, Some((0, 68))),
        ],
        vec![
            definition(
                right,
                TeletextObjectType::Passive,
                vec![foreground(0, 2), glyph(0, b'R')],
            ),
            definition(
                left,
                TeletextObjectType::Passive,
                vec![foreground(0, 1), glyph(0, b'L')],
            ),
        ],
    );
    let fixture = Fixture::new(base, &service);
    let result = resolve(&fixture);
    assert_eq!(result.width(), 56);
    for (visual, raw, code, expected_colour) in
        [(0, 68, b'L', 1), (4, 0, b'M', 7), (44, 40, b'R', 2)]
    {
        let origin = origin_at(&result, 1, visual);
        let candidate = fixture.layers.candidates()[origin.candidate_index()];
        assert_eq!(candidate.position(), (1, raw));
        assert!(
            matches!(candidate.operation(), Some(TeletextColumnOperation::G0 { code: value }) if value.get() == code)
        );
        assert_eq!(origin.visual_position(), Some((1, visual)));
        assert_eq!(colour(&result, 1, visual), Some(expected_colour));
    }
    assert!(result.cell(1, 56).is_none());
}

#[test]
fn adaptive_nonorigin_inherited_size_retains_located_uncertainty_instead_of_guessing_clipping() {
    let adaptive = local(0xffff);
    let service = service(
        vec![
            set(1, 8),
            foreground(8, 1),
            size(8, false, true),
            glyph(8, b'L'),
            set(1, 9),
            invoke(adaptive, TeletextObjectType::Adaptive, None),
        ],
        vec![definition(
            adaptive,
            TeletextObjectType::Adaptive,
            vec![foreground(0, 2), glyph(0, b'A')],
        )],
    );
    let fixture = Fixture::new(blank(), &service);
    let result = resolve(&fixture);
    let index = fixture
        .layers
        .candidates()
        .iter()
        .position(|candidate| {
            candidate.source()
                == Some(TeletextEnhancementSource {
                    object: Some(adaptive),
                    command: 1,
                })
        })
        .unwrap();
    let pending = result
        .pending()
        .iter()
        .find(|pending| pending.candidate_index() == Some(index))
        .unwrap();
    assert_eq!(
        pending.reason(),
        TeletextDisplayPendingReason::InheritedAdaptiveNonOrigin
    );
    assert_eq!(pending.position(), (1, 9));
    assert_eq!(
        pending.location(),
        Some(TeletextEnhancementSource {
            object: Some(adaptive),
            command: 1
        })
    );
    assert_eq!(
        pending.root_source(),
        Some(TeletextSpatialRootSource::Local { command: 5 })
    );
    assert_eq!(
        pending.invocation().map(TeletextSpatialInvocationId::get),
        Some(0)
    );
    let attributes = result.origin_attributes(index).unwrap();
    assert_eq!(attributes.foreground().value().unwrap().get(), 2);
    assert!(matches!(attributes.foreground().writer().unwrap().source(),
        TeletextAttributeSource::Enhancement { location, root_source, invocation: Some(instance), .. }
        if location == TeletextEnhancementSource { object: Some(adaptive), command: 0 }
            && root_source == TeletextSpatialRootSource::Local { command: 5 }
            && instance.get() == 0));
}

#[test]
fn illegal_height_and_partial_width_boundaries_are_located_pending_not_invented_reductions() {
    for (row, column, height, width) in [(23, 8, true, false), (1, 39, false, true)] {
        let service = service(
            vec![
                set(row, column),
                size(column, height, width),
                glyph(column, b'A'),
            ],
            vec![],
        );
        let fixture = Fixture::new(blank(), &service);
        let result = resolve(&fixture);
        let index = fixture
            .layers
            .candidates()
            .iter()
            .position(|candidate| {
                candidate.source()
                    == Some(TeletextEnhancementSource {
                        object: None,
                        command: 2,
                    })
            })
            .unwrap();
        let pending = result
            .pending()
            .iter()
            .find(|pending| pending.candidate_index() == Some(index))
            .unwrap();
        assert_eq!(
            pending.reason(),
            TeletextDisplayPendingReason::IllegalSizeBoundary
        );
        assert_eq!(pending.position(), (u16::from(row), u16::from(column)));
        assert_eq!(
            pending.location(),
            Some(TeletextEnhancementSource {
                object: None,
                command: 2
            })
        );
        assert_eq!(
            pending.root_source(),
            Some(TeletextSpatialRootSource::Local { command: 2 })
        );
        assert_eq!(pending.invocation(), None);
    }
}

#[test]
fn all_original_losing_candidates_keep_their_attribute_and_invocation_sources() {
    let first = local(0xffff);
    let second = local(0x0100);
    let service = service(
        vec![
            set(2, 8),
            invoke(first, TeletextObjectType::Passive, None),
            invoke(second, TeletextObjectType::Passive, None),
        ],
        vec![
            definition(
                first,
                TeletextObjectType::Passive,
                vec![foreground(0, 1), size(0, true, true), glyph(0, b'A')],
            ),
            definition(
                second,
                TeletextObjectType::Passive,
                vec![foreground(0, 4), glyph(0, b'B')],
            ),
        ],
    );
    let fixture = Fixture::new(blank(), &service);
    let result = resolve(&fixture);
    assert_eq!(result.origins().len(), fixture.layers.candidates().len());
    let first_index = fixture
        .layers
        .candidates()
        .iter()
        .position(|candidate| {
            candidate.source()
                == Some(TeletextEnhancementSource {
                    object: Some(first),
                    command: 2,
                })
        })
        .unwrap();
    let second_index = fixture
        .layers
        .candidates()
        .iter()
        .position(|candidate| {
            candidate.source()
                == Some(TeletextEnhancementSource {
                    object: Some(second),
                    command: 1,
                })
        })
        .unwrap();
    assert_eq!(result.origins()[first_index].candidate_index(), first_index);
    assert_eq!(
        result.origins()[second_index].candidate_index(),
        second_index
    );
    assert_eq!(result.cell(2, 8).unwrap().origin(), Some(second_index));
    let retained = result.origin_attributes(first_index).unwrap();
    assert_eq!(retained.foreground().value().unwrap().get(), 1);
    assert_eq!(retained.double_height().value(), Some(true));
    assert_eq!(retained.double_width().value(), Some(true));
    assert!(matches!(retained.foreground().writer().unwrap().source(),
        TeletextAttributeSource::Enhancement { location, invocation: Some(instance), .. }
        if location == TeletextEnhancementSource { object: Some(first), command: 0 } && instance.get() == 0));
    assert_eq!(result.cell(2, 9).unwrap().origin(), Some(first_index));
    assert_eq!(result.cell(3, 8).unwrap().origin(), Some(first_index));
    assert_eq!(result.cell(3, 9).unwrap().origin(), Some(first_index));
}

#[test]
fn candidate_origin_and_command_budgets_fail_before_foreign_projection_checks_with_original_blame()
{
    let actual = Fixture::new(blank(), &service(vec![set(1, 8), glyph(8, b'A')], vec![]));
    let foreign = Fixture::new(blank(), &service(vec![set(1, 8), glyph(8, b'B')], vec![]));
    let glyph_index = actual
        .layers
        .candidates()
        .iter()
        .position(|candidate| {
            candidate.source()
                == Some(TeletextEnhancementSource {
                    object: None,
                    command: 1,
                })
        })
        .unwrap();
    let original = actual.original.clone();
    let layers = actual.layers.clone();
    for which in 0..3 {
        let mut limited = budget();
        let expected = match which {
            0 => {
                limited.max_candidates = glyph_index;
                TeletextDisplayResolutionFailure::CandidateBudget {
                    count: 993,
                    maximum: glyph_index,
                }
            }
            1 => {
                limited.max_origins = glyph_index;
                TeletextDisplayResolutionFailure::OriginBudget {
                    count: 993,
                    maximum: glyph_index,
                }
            }
            _ => {
                limited.max_commands = 1;
                TeletextDisplayResolutionFailure::CommandBudget {
                    count: 2,
                    maximum: 1,
                }
            }
        };
        let mut inputs = actual.inputs();
        inputs.attributes = &foreign.attributes;
        let error = resolve_teletext_display_resolution(inputs, limited).unwrap_err();
        assert_eq!(error.reason, expected);
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
    }
    let mut exact = budget();
    exact.max_candidates = 993;
    exact.max_origins = 993;
    exact.max_commands = 2;
    assert!(resolve_teletext_display_resolution(actual.inputs(), exact).is_ok());
    assert_eq!(actual.original, original);
    assert_eq!(actual.layers, layers);
}

#[test]
fn writer_storage_and_update_work_are_bounded_without_mutating_or_returning_partial_planes() {
    let fixture = Fixture::new(blank(), &service(vec![], vec![]));
    let mut exact = budget();
    // Two independent default writers per coded row, not one per1000 cells.
    exact.max_writers = 50;
    exact.max_candidates = 992;
    exact.max_origins = 992;
    exact.max_commands = 0;
    let result = resolve_teletext_display_resolution(fixture.inputs(), exact).unwrap();
    assert_eq!(result.writers().len(), 50);
    assert_eq!(result.origins().len(), 992);
    let original = fixture.base.clone();
    let mut limited = exact;
    limited.max_writers = 49;
    assert_eq!(
        resolve_teletext_display_resolution(fixture.inputs(), limited)
            .unwrap_err()
            .reason,
        TeletextDisplayResolutionFailure::WriterBudget {
            count: 50,
            maximum: 49
        }
    );
    limited = exact;
    limited.max_updates = 0;
    assert_eq!(
        resolve_teletext_display_resolution(fixture.inputs(), limited)
            .unwrap_err()
            .reason,
        TeletextDisplayResolutionFailure::UpdateBudget { maximum: 0 }
    );
    assert_eq!(fixture.base, original);
}

#[test]
fn same_owner_changed_projection_and_changed_footprints_are_rejected_with_source_correspondence() {
    let actual = Fixture::new(blank(), &service(vec![set(1, 8), glyph(8, b'A')], vec![]));
    let foreign = Fixture::new(blank(), &service(vec![set(1, 8), glyph(8, b'B')], vec![]));
    let index = actual
        .layers
        .candidates()
        .iter()
        .position(|candidate| {
            candidate.source()
                == Some(TeletextEnhancementSource {
                    object: None,
                    command: 1,
                })
        })
        .unwrap();
    for component in 0..3 {
        let mut inputs = actual.inputs();
        match component {
            0 => inputs.attributes = &foreign.attributes,
            1 => inputs.intent = &foreign.intent,
            _ => inputs.geometry = &foreign.geometry,
        }
        let error = resolve_teletext_display_resolution(inputs, budget()).unwrap_err();
        assert_eq!(
            error.reason,
            TeletextDisplayResolutionFailure::CandidateMismatch { candidate: index }
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
    }
    let mut rows = empty_rows(7, 0);
    super::base_footprints_tests::height_segment(&mut rows, 1, 7, Some(10));
    let changed_base = finish(rows, None);
    let changed = resolve_teletext_base_footprints(&changed_base, &actual.row_colours).unwrap();
    let mut inputs = actual.inputs();
    inputs.base_footprints = &changed;
    assert_eq!(
        resolve_teletext_display_resolution(inputs, budget())
            .unwrap_err()
            .reason,
        TeletextDisplayResolutionFailure::BaseFootprintMismatch
    );
}

#[test]
fn exact_raw_subcode_owner_is_checked_before_any_display_is_returned() {
    let fixture = Fixture::new(blank(), &service(vec![], vec![]));
    let other = TeletextPageAddress {
        page: owner().page,
        subcode: TeletextSubcode::new(0x1235).unwrap(),
    };
    let changed_base = TeletextBaseDisplay::new(
        other,
        fixture.base.primary(),
        fixture.base.national_option(),
        fixture.base.presentation(),
        fixture
            .base
            .cells()
            .to_vec()
            .into_boxed_slice()
            .try_into()
            .unwrap(),
    )
    .unwrap();
    let mut inputs = fixture.inputs();
    inputs.base = &changed_base;
    let error = resolve_teletext_display_resolution(inputs, budget()).unwrap_err();
    assert_eq!(error.owner, other);
    assert_eq!(
        error.reason,
        TeletextDisplayResolutionFailure::OwnerMismatch { other: owner() }
    );
    assert_eq!(error.location, None);
    assert_eq!(error.root_source, None);
    assert_eq!(fixture.base.owner(), owner());
}

#[test]
fn equal_numeric_command_and_candidate_counts_do_not_erase_original_command_budget_blame() {
    let commands = vec![
        TeletextEnhancementCommand::SetScreenColour {
            colour: TeletextColourReference::BLACK,
        };
        992
    ];
    let fixture = Fixture::new(blank(), &service(commands, vec![]));
    assert_eq!(fixture.original.events().len(), 992);
    assert_eq!(fixture.layers.candidates().len(), 992);
    let mut limited = budget();
    limited.max_commands = 991;
    let error = resolve_teletext_display_resolution(fixture.inputs(), limited).unwrap_err();
    assert_eq!(
        error.reason,
        TeletextDisplayResolutionFailure::CommandBudget {
            count: 992,
            maximum: 991
        }
    );
    assert_eq!(
        error.location,
        Some(TeletextEnhancementSource {
            object: None,
            command: 991
        })
    );
    assert_eq!(
        error.root_source,
        Some(TeletextSpatialRootSource::Local { command: 991 })
    );
    assert_eq!(error.invocation, None);
}

#[test]
fn unplaced_passive_glyph_keeps_committed_private_attributes_and_original_source() {
    let passive = local(0xffff);
    for (row, column, offset, absolute) in
        [(0, 0, None, (0, 0)), (24, 39, Some((23, 71)), (47, 110))]
    {
        let service = service(
            vec![
                set(row, column),
                invoke(passive, TeletextObjectType::Passive, offset),
            ],
            vec![definition(
                passive,
                TeletextObjectType::Passive,
                vec![
                    foreground(0, 1),
                    background(0, 4),
                    size(0, true, true),
                    glyph(0, b'P'),
                ],
            )],
        );
        let fixture = Fixture::new(blank(), &service);
        let index = fixture
            .layers
            .candidates()
            .iter()
            .position(|candidate| {
                candidate.source()
                    == Some(TeletextEnhancementSource {
                        object: Some(passive),
                        command: 3,
                    })
            })
            .unwrap();
        let result = resolve(&fixture);
        let origin = &result.origins()[index];
        assert_eq!(origin.candidate_index(), index);
        assert_eq!(origin.visual_position(), None);
        let attributes = result.origin_attributes(index).unwrap();
        assert_eq!(attributes.foreground().value().unwrap().get(), 1);
        assert_eq!(attributes.background().value().unwrap().get(), 4);
        assert_eq!(attributes.double_height().value(), Some(true));
        assert_eq!(attributes.double_width().value(), Some(true));
        assert!(matches!(attributes.foreground().writer().unwrap().source(),
            TeletextAttributeSource::Enhancement { location, invocation: Some(instance), .. }
            if location == TeletextEnhancementSource { object: Some(passive), command: 0 }
                && instance.get() == 0));
        let pending = &result.pending()[origin.pending().unwrap()];
        assert_eq!(
            pending.reason(),
            TeletextDisplayPendingReason::Unaddressable
        );
        assert_eq!(pending.position(), absolute);
        assert_eq!(
            pending.location(),
            Some(TeletextEnhancementSource {
                object: Some(passive),
                command: 3
            })
        );
        assert_eq!(
            pending.root_source(),
            Some(TeletextSpatialRootSource::Local { command: 1 })
        );
        assert_eq!(
            pending.invocation().map(TeletextSpatialInvocationId::get),
            Some(0)
        );
    }
}

#[test]
fn repeated_point_within_one_passive_instance_retains_each_glyphs_committed_private_writer() {
    let passive = local(0xffff);
    let service = service(
        vec![
            set(2, 8),
            invoke(passive, TeletextObjectType::Passive, None),
        ],
        vec![definition(
            passive,
            TeletextObjectType::Passive,
            vec![
                foreground(0, 1),
                glyph(0, b'A'),
                foreground(0, 4),
                glyph(0, b'B'),
            ],
        )],
    );
    let fixture = Fixture::new(blank(), &service);
    let result = resolve(&fixture);
    let mut indices = Vec::new();
    for (command, foreground_command, expected) in [(1, 0, 1), (3, 2, 4)] {
        let index = fixture
            .layers
            .candidates()
            .iter()
            .position(|candidate| {
                candidate.source()
                    == Some(TeletextEnhancementSource {
                        object: Some(passive),
                        command,
                    })
            })
            .unwrap();
        indices.push(index);
        let origin = &result.origins()[index];
        assert_eq!(origin.visual_position(), Some((2, 8)));
        let attributes = result.origin_attributes(index).unwrap();
        assert_eq!(attributes.foreground().value().unwrap().get(), expected);
        assert!(matches!(attributes.foreground().writer().unwrap().source(),
            TeletextAttributeSource::Enhancement { location, invocation: Some(instance), .. }
            if location == TeletextEnhancementSource { object: Some(passive), command: foreground_command }
                && instance.get() == 0));
        assert_eq!(
            fixture.layers.candidates()[index]
                .invocation()
                .map(TeletextSpatialInvocationId::get),
            Some(0)
        );
    }
    assert_ne!(indices[0], indices[1]);
    assert_eq!(result.cell(2, 8).unwrap().origin(), Some(indices[1]));
    assert_eq!(colour(&result, 2, 8), Some(4));
    assert_eq!(fixture.layers.candidates()[indices[0]].position(), (2, 8));
    assert_eq!(fixture.layers.candidates()[indices[1]].position(), (2, 8));
}

#[test]
fn same_address_object_rules_distinguish_types_and_active_attribute_groups() {
    let first = local(0xffff);
    let second = local(0x0100);
    for case in 0..5 {
        let (first_type, second_type, forbidden) = match case {
            0 => (
                TeletextObjectType::Adaptive,
                TeletextObjectType::Adaptive,
                true,
            ),
            1 | 2 => (
                TeletextObjectType::Active,
                TeletextObjectType::Active,
                case == 1,
            ),
            3 => (
                TeletextObjectType::Passive,
                TeletextObjectType::Passive,
                false,
            ),
            _ => (
                TeletextObjectType::Active,
                TeletextObjectType::Adaptive,
                false,
            ),
        };
        let mut first_body = vec![];
        let mut second_body = vec![];
        if case == 1 || case == 2 || case == 4 {
            first_body.push(foreground(5, 1));
            second_body.push(if case == 2 {
                background(5, 4)
            } else {
                foreground(5, 2)
            });
        }
        first_body.push(glyph(5, b'A'));
        second_body.push(glyph(5, b'B'));
        let service = service(
            vec![
                set(1, 0),
                invoke(first, first_type, None),
                invoke(second, second_type, None),
            ],
            vec![
                definition(first, first_type, first_body),
                definition(second, second_type, second_body),
            ],
        );
        let fixture = Fixture::new(blank(), &service);
        let result = resolve_teletext_display_resolution(fixture.inputs(), budget());
        if forbidden {
            let error = result.unwrap_err();
            assert!(
                matches!(error.reason, TeletextDisplayResolutionFailure::ObjectOverlap { first, second }
                if first.get() == 0 && second.get() == 1)
            );
            assert_eq!(
                error.location,
                Some(TeletextEnhancementSource {
                    object: Some(second),
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
        } else {
            assert!(result.is_ok(), "permitted case {case}: {result:?}");
        }
    }
}

fn charset_address(column: u8) -> TeletextEnhancementCommand {
    at(
        column,
        TeletextColumnOperation::DesignateCharset {
            designation: TeletextEnhancementCharsetDesignation::new(0).unwrap(),
        },
    )
}

fn addressed_admission(
    first_type: TeletextObjectType,
    second_type: TeletextObjectType,
    first_body: Vec<TeletextEnhancementCommand>,
    second_body: Vec<TeletextEnhancementCommand>,
    offset: Option<(u8, u8)>,
) -> Result<(), TeletextDisplayResolutionError> {
    let first = local(0xffff);
    let second = local(0x0100);
    let service = service(
        vec![
            set(1, 0),
            invoke(first, first_type, offset),
            invoke(second, second_type, offset),
        ],
        vec![
            definition(first, first_type, first_body),
            definition(second, second_type, second_body),
        ],
    );
    let fixture = Fixture::new(blank(), &service);
    super::display_resolution_writers::validate_objects(fixture.inputs())
}

#[test]
fn charset_and_position_only_overlap_use_addressed_cells_without_glyph_or_panel_filtering() {
    for command in [
        charset_address(5),
        set(0, 5),
        TeletextEnhancementCommand::SetRowColour {
            row: 0,
            colour: TeletextColourReference::BLACK,
            following_rows: false,
        },
        TeletextEnhancementCommand::SetRowColour {
            row: 1,
            colour: TeletextColourReference::BLACK,
            following_rows: true,
        },
    ] {
        for offset in [None, Some((0, 40))] {
            let error = addressed_admission(
                TeletextObjectType::Adaptive,
                TeletextObjectType::Adaptive,
                vec![command],
                vec![command],
                offset,
            )
            .unwrap_err();
            assert!(
                matches!(error.reason, TeletextDisplayResolutionFailure::ObjectOverlap { first, second }
                if first.get() == 0 && second.get() == 1)
            );
            assert_eq!(
                error.location,
                Some(TeletextEnhancementSource {
                    object: Some(local(0x0100)),
                    command: 0
                })
            );
            for (first, second) in [
                (TeletextObjectType::Active, TeletextObjectType::Active),
                (TeletextObjectType::Passive, TeletextObjectType::Passive),
                (TeletextObjectType::Active, TeletextObjectType::Adaptive),
            ] {
                assert!(
                    addressed_admission(first, second, vec![command], vec![command], offset)
                        .is_ok()
                );
            }
        }
    }
}

#[test]
fn active_charset_overlap_uses_whole_instance_compound_attribute_type_masks() {
    for (first_change, second_change, forbidden) in [
        (foreground(6, 1), foreground(7, 2), true),
        (foreground(6, 1), background(7, 2), false),
        (size(6, true, false), size(7, false, true), true),
    ] {
        let result = addressed_admission(
            TeletextObjectType::Active,
            TeletextObjectType::Active,
            vec![charset_address(5), first_change],
            vec![charset_address(5), second_change],
            None,
        );
        if forbidden {
            let error = result.unwrap_err();
            assert!(matches!(
                error.reason,
                TeletextDisplayResolutionFailure::ObjectOverlap { .. }
            ));
            assert_eq!(
                error.location,
                Some(TeletextEnhancementSource {
                    object: Some(local(0x0100)),
                    command: 0
                })
            );
        } else {
            assert!(result.is_ok());
        }
    }
}

#[test]
fn charset_only_contained_interleaving_keeps_adaptive_group_rules_and_other_types_allowed() {
    for object_type in [
        TeletextObjectType::Active,
        TeletextObjectType::Adaptive,
        TeletextObjectType::Passive,
    ] {
        for changes in [false, true] {
            let mut first = vec![charset_address(5)];
            if changes {
                first.push(foreground(6, 1));
            }
            first.push(charset_address(15));
            let result = addressed_admission(
                object_type,
                object_type,
                first,
                vec![charset_address(10)],
                None,
            );
            if changes && object_type == TeletextObjectType::Adaptive {
                let error = result.unwrap_err();
                assert!(
                    matches!(error.reason, TeletextDisplayResolutionFailure::AdaptiveInterleaving { first, second }
                    if first.get() == 0 && second.get() == 1)
                );
                assert_eq!(
                    error.root_source,
                    Some(TeletextSpatialRootSource::Local { command: 2 })
                );
            } else {
                assert!(result.is_ok());
            }
        }
    }
}

#[test]
fn contained_adaptive_interleaving_is_forbidden_only_with_redefined_groups_on_the_same_row() {
    let first = local(0xffff);
    let second = local(0x0100);
    for (first_changes, second_changes, second_row, forbidden) in [
        (false, false, 1, false),
        (true, false, 1, true),
        (false, true, 1, true),
        (true, true, 1, true),
        (true, true, 2, false),
    ] {
        let mut first_body = vec![];
        let mut second_body = vec![];
        if first_changes {
            first_body.push(foreground(5, 1));
        }
        if second_changes {
            second_body.push(background(10, 4));
        }
        first_body.extend([glyph(5, b'A'), glyph(15, b'A')]);
        second_body.push(glyph(10, b'B'));
        let mut main = vec![set(1, 0), invoke(first, TeletextObjectType::Adaptive, None)];
        if second_row != 1 {
            main.push(set(second_row, 0));
        }
        let call = main.len();
        main.push(invoke(second, TeletextObjectType::Adaptive, None));
        let service = service(
            main,
            vec![
                definition(first, TeletextObjectType::Adaptive, first_body),
                definition(second, TeletextObjectType::Adaptive, second_body),
            ],
        );
        let fixture = Fixture::new(blank(), &service);
        let result = resolve_teletext_display_resolution(fixture.inputs(), budget());
        if forbidden {
            let error = result.unwrap_err();
            assert!(
                matches!(error.reason, TeletextDisplayResolutionFailure::AdaptiveInterleaving { first, second }
                if first.get() == 0 && second.get() == 1)
            );
            assert_eq!(
                error.invocation.map(TeletextSpatialInvocationId::get),
                Some(1)
            );
            assert_eq!(
                error.root_source,
                Some(TeletextSpatialRootSource::Local { command: call })
            );
        } else {
            assert!(
                result.is_ok(),
                "permitted row/changes {second_row}/{first_changes}/{second_changes}: {result:?}"
            );
        }
    }
}

#[test]
fn adaptive_attribute_only_size_rebuilds_underlying_glyph_occupied_parts_and_origin_attributes() {
    let adaptive = local(0xffff);
    let mut rows = empty_rows(7, 0);
    put(&mut rows, 1, 8, b'A', None, attributes(1, 7, 0));
    let service = service(
        vec![
            set(1, 8),
            invoke(adaptive, TeletextObjectType::Adaptive, None),
        ],
        vec![definition(
            adaptive,
            TeletextObjectType::Adaptive,
            vec![size(0, false, true)],
        )],
    );
    let fixture = Fixture::new(finish(rows, None), &service);
    let result = resolve(&fixture);
    let origin = origin_at(&result, 1, 8);
    assert_eq!(
        origin.effective_size(),
        Some(TeletextDisplaySize::DoubleWidth)
    );
    let candidate = fixture.layers.candidates()[origin.candidate_index()];
    assert_eq!(candidate.priority(), TeletextGlyphPriority::Base);
    assert_eq!(candidate.base().unwrap().code(), b'A');
    assert_eq!(
        result.cell(1, 8).unwrap().origin(),
        result.cell(1, 9).unwrap().origin()
    );
    assert_eq!(
        result.cell(1, 8).unwrap().fragment(),
        TeletextDisplayFragment::DoubleWidthLeft
    );
    assert_eq!(
        result.cell(1, 9).unwrap().fragment(),
        TeletextDisplayFragment::DoubleWidthRight
    );
    for column in [8, 9] {
        let attributes = result.cell(1, column).unwrap().attributes();
        assert_eq!(attributes.foreground().value().unwrap().get(), 7);
        assert_eq!(attributes.background().value().unwrap().get(), 0);
        assert_eq!(attributes.double_width().value(), Some(true));
        assert_eq!(attributes.double_height().value(), Some(false));
        assert_eq!(attributes.boxed().value(), Some(false));
        assert_eq!(attributes.conceal().value(), Some(false));
        assert_eq!(attributes.invert().value(), Some(false));
        assert_eq!(
            attributes.underline_or_separated_mosaics().value(),
            Some(false)
        );
        assert_eq!(
            attributes.flash().value().unwrap().mode(),
            TeletextFlashMode::Steady
        );
        assert!(
            matches!(attributes.double_width().writer().unwrap().source(),
            TeletextAttributeSource::Enhancement { location, invocation: Some(instance), .. }
            if location == TeletextEnhancementSource { object: Some(adaptive), command: 0 }
                && instance.get() == 0)
        );
    }
}

#[test]
fn adaptive_writer_obscured_by_lower_double_width_defers_only_within_its_own_extent() {
    for extends_to_free_cell in [false, true] {
        let adaptive = local(0xffff);
        let mut body = vec![foreground(0, 1), foreground(1, 4)];
        if extends_to_free_cell {
            body.push(glyph(2, b'B'));
        }
        let service = service(
            vec![
                set(1, 8),
                foreground(8, 1),
                size(8, false, true),
                glyph(8, b'A'),
                invoke(adaptive, TeletextObjectType::Adaptive, None),
                size(10, false, false),
            ],
            vec![definition(adaptive, TeletextObjectType::Adaptive, body)],
        );
        let fixture = Fixture::new(blank(), &service);
        let result = resolve(&fixture);
        assert_eq!(
            result.cell(1, 8).unwrap().origin(),
            result.cell(1, 9).unwrap().origin()
        );
        assert_eq!(colour(&result, 1, 8), Some(1));
        assert_eq!(colour(&result, 1, 9), Some(1));
        assert_eq!(
            colour(&result, 1, 10),
            Some(if extends_to_free_cell { 4 } else { 1 })
        );
        assert_eq!(colour(&result, 1, 11), Some(1));
        if extends_to_free_cell {
            let writer = result
                .cell(1, 10)
                .unwrap()
                .attributes()
                .foreground()
                .writer()
                .unwrap();
            assert_eq!(writer.placed_position(), (1, 9));
            assert_eq!(writer.requested_start(), (1, 9));
            assert_eq!(writer.effective_start(), (1, 10));
            assert!(
                matches!(writer.source(), TeletextAttributeSource::Enhancement { location, .. }
                if location == TeletextEnhancementSource { object: Some(adaptive), command: 1 })
            );
        }
    }
}

#[test]
fn adaptive_explicit_normal_glyph_overlays_lower_right_part_without_erasing_the_left_origin() {
    let adaptive = local(0xffff);
    let service = service(
        vec![
            set(1, 8),
            foreground(8, 1),
            background(8, 4),
            size(8, false, true),
            glyph(8, b'A'),
            set(1, 9),
            invoke(adaptive, TeletextObjectType::Adaptive, None),
            size(10, false, false),
        ],
        vec![definition(
            adaptive,
            TeletextObjectType::Adaptive,
            vec![foreground(0, 2), size(0, false, false), glyph(0, b'X')],
        )],
    );
    let fixture = Fixture::new(blank(), &service);
    let result = resolve(&fixture);
    let left = result.cell(1, 8).unwrap();
    assert_eq!(left.fragment(), TeletextDisplayFragment::DoubleWidthLeft);
    let left_origin = origin_at(&result, 1, 8);
    assert_eq!(
        left_origin.effective_size(),
        Some(TeletextDisplaySize::DoubleWidth)
    );
    assert_eq!(
        fixture.layers.candidates()[left_origin.candidate_index()].source(),
        Some(TeletextEnhancementSource {
            object: None,
            command: 4
        })
    );
    assert_eq!(left.attributes().foreground().value().unwrap().get(), 1);
    assert_eq!(left.attributes().background().value().unwrap().get(), 4);
    assert_eq!(left.attributes().double_width().value(), Some(true));
    let right = result.cell(1, 9).unwrap();
    assert_eq!(right.fragment(), TeletextDisplayFragment::Normal);
    assert_ne!(left.origin(), right.origin());
    let right_origin = origin_at(&result, 1, 9);
    assert_eq!(
        right_origin.effective_size(),
        Some(TeletextDisplaySize::Normal)
    );
    let candidate = fixture.layers.candidates()[right_origin.candidate_index()];
    assert_eq!(
        candidate.source(),
        Some(TeletextEnhancementSource {
            object: Some(adaptive),
            command: 2
        })
    );
    assert_eq!(
        candidate.root_source(),
        Some(TeletextSpatialRootSource::Local { command: 6 })
    );
    assert_eq!(
        candidate.invocation().map(TeletextSpatialInvocationId::get),
        Some(0)
    );
    assert_eq!(right.attributes().foreground().value().unwrap().get(), 2);
    assert_eq!(right.attributes().background().value().unwrap().get(), 4);
    assert_eq!(right.attributes().double_width().value(), Some(false));
    assert_eq!(right_origin.pending(), None);
}

#[test]
fn later_adaptive_lower_origin_repaint_preserves_an_earlier_higher_right_fragment() {
    let right_object = local(0xffff);
    let recolour_object = local(0x0100);
    let service = service(
        vec![
            set(1, 8),
            foreground(8, 1),
            background(8, 4),
            size(8, false, true),
            glyph(8, b'A'),
            invoke(right_object, TeletextObjectType::Adaptive, Some((0, 1))),
            invoke(recolour_object, TeletextObjectType::Adaptive, None),
            size(10, false, false),
        ],
        vec![
            definition(
                right_object,
                TeletextObjectType::Adaptive,
                vec![foreground(0, 3), size(0, false, false), glyph(0, b'X')],
            ),
            definition(
                recolour_object,
                TeletextObjectType::Adaptive,
                vec![foreground(0, 2)],
            ),
        ],
    );
    let fixture = Fixture::new(blank(), &service);
    let result = resolve(&fixture);
    let left = result.cell(1, 8).unwrap();
    assert_eq!(left.fragment(), TeletextDisplayFragment::DoubleWidthLeft);
    assert_eq!(left.attributes().foreground().value().unwrap().get(), 2);
    assert_eq!(left.attributes().background().value().unwrap().get(), 4);
    assert_eq!(
        fixture.layers.candidates()[origin_at(&result, 1, 8).candidate_index()].source(),
        Some(TeletextEnhancementSource {
            object: None,
            command: 4
        })
    );
    assert!(
        matches!(left.attributes().foreground().writer().unwrap().source(),
        TeletextAttributeSource::Enhancement { location, invocation: Some(instance), .. }
        if location == TeletextEnhancementSource { object: Some(recolour_object), command: 0 }
            && instance.get() == 1)
    );
    let right = result.cell(1, 9).unwrap();
    assert_eq!(right.fragment(), TeletextDisplayFragment::Normal);
    assert_eq!(right.attributes().foreground().value().unwrap().get(), 3);
    assert_eq!(right.attributes().background().value().unwrap().get(), 4);
    assert_eq!(right.attributes().double_width().value(), Some(false));
    let candidate = fixture.layers.candidates()[origin_at(&result, 1, 9).candidate_index()];
    assert_eq!(
        candidate.source(),
        Some(TeletextEnhancementSource {
            object: Some(right_object),
            command: 2
        })
    );
    assert_eq!(
        candidate.root_source(),
        Some(TeletextSpatialRootSource::Local { command: 5 })
    );
    assert_eq!(
        candidate.invocation().map(TeletextSpatialInvocationId::get),
        Some(0)
    );
    assert!(
        matches!(right.attributes().foreground().writer().unwrap().source(),
        TeletextAttributeSource::Enhancement { location, invocation: Some(instance), .. }
        if location == TeletextEnhancementSource { object: Some(right_object), command: 0 }
            && instance.get() == 0)
    );
    assert_ne!(left.origin(), right.origin());
}

fn tied_display_fixture(
    first: (bool, bool),
    active_flags: (bool, bool),
    third: Option<(bool, bool)>,
) -> Fixture {
    let active = local(0xffff);
    let mut main = vec![
        set(1, 8),
        size(8, first.0, first.1),
        invoke(active, TeletextObjectType::Active, None),
    ];
    if let Some(flags) = third {
        main.push(size(8, flags.0, flags.1));
    }
    main.extend([glyph(8, b'A'), size(10, false, false)]);
    let service = service(
        main,
        vec![definition(
            active,
            TeletextObjectType::Active,
            vec![size(0, active_flags.0, active_flags.1)],
        )],
    );
    Fixture::new(blank(), &service)
}

fn pending_writer_sources(
    result: &TeletextDisplayResolution,
    pending: &TeletextDisplayPending,
    seen: &mut Vec<usize>,
    sources: &mut Vec<TeletextAttributeSource>,
) {
    for writer in [pending.first_writer(), pending.second_writer()]
        .into_iter()
        .flatten()
    {
        let source = result.writers()[writer].source();
        if !sources.contains(&source) {
            sources.push(source);
        }
    }
    for index in [pending.previous(), pending.incoming_previous()]
        .into_iter()
        .flatten()
    {
        if !seen.contains(&index) {
            seen.push(index);
            pending_writer_sources(result, &result.pending()[index], seen, sources);
        }
    }
}

fn assert_display_contenders(result: &TeletextDisplayResolution, origin: usize, third: bool) {
    let attributes = result.origin_attributes(origin).unwrap();
    for field in [attributes.double_height(), attributes.double_width()] {
        assert_eq!(field.value(), None);
        assert_eq!(field.writer(), None);
        let pending = field.pending().unwrap();
        assert_eq!(
            pending.reason(),
            TeletextDisplayPendingReason::TransmissionOrder
        );
        let mut sources = vec![];
        pending_writer_sources(result, pending, &mut vec![], &mut sources);
        let mut commands = vec![];
        for source in sources {
            match source {
                TeletextAttributeSource::Enhancement {
                    location,
                    root_source,
                    invocation,
                    ..
                } => {
                    commands.push(location);
                    if location.object.is_some() {
                        assert_eq!(root_source, TeletextSpatialRootSource::Local { command: 2 });
                        assert_eq!(invocation.map(TeletextSpatialInvocationId::get), Some(0));
                    } else {
                        assert_eq!(invocation, None);
                    }
                }
                unexpected => {
                    panic!("default cannot compete with explicit display writers: {unexpected:?}")
                }
            }
        }
        assert!(commands.contains(&TeletextEnhancementSource {
            object: None,
            command: 1
        }));
        assert!(commands.contains(&TeletextEnhancementSource {
            object: Some(local(0xffff)),
            command: 0
        }));
        if third {
            assert!(commands.contains(&TeletextEnhancementSource {
                object: None,
                command: 3
            }));
        }
        assert_eq!(commands.len(), if third { 3 } else { 2 });
    }
}

#[test]
fn equal_display_size_contenders_keep_known_occupancy_and_unresolved_writer_order() {
    for (flags, expected) in [
        ((false, false), TeletextDisplaySize::Normal),
        ((false, true), TeletextDisplaySize::DoubleWidth),
        ((true, false), TeletextDisplaySize::DoubleHeight),
        ((true, true), TeletextDisplaySize::DoubleSize),
    ] {
        for third in [false, true] {
            let fixture = tied_display_fixture(flags, flags, third.then_some(flags));
            let result = resolve(&fixture);
            let origin = result.cell(1, 8).unwrap().origin().unwrap();
            assert_eq!(result.origins()[origin].requested_size(), Some(expected));
            assert_eq!(result.origins()[origin].effective_size(), Some(expected));
            assert_display_contenders(&result, origin, third);
            if flags.1 {
                assert_eq!(result.cell(1, 9).unwrap().origin(), Some(origin));
            }
            if flags.0 {
                assert_eq!(result.cell(2, 8).unwrap().origin(), Some(origin));
            }
            if flags.0 && flags.1 {
                assert_eq!(result.cell(2, 9).unwrap().origin(), Some(origin));
            }
            assert!(!result
                .pending()
                .iter()
                .any(|pending| pending.candidate_index() == Some(origin)
                    && pending.reason() == TeletextDisplayPendingReason::UnknownSize));
        }
    }
}

#[test]
fn any_disagreeing_predecessor_or_incoming_display_size_keeps_three_writer_size_unknown() {
    for (first, active, third) in [
        ((false, true), (false, false), (false, false)),
        ((true, false), (false, false), (false, false)),
        ((false, false), (false, true), (false, true)),
        ((true, true), (false, true), (false, true)),
    ] {
        let fixture = tied_display_fixture(first, active, Some(third));
        let result = resolve(&fixture);
        let index = fixture
            .layers
            .candidates()
            .iter()
            .position(|candidate| {
                candidate.source()
                    == Some(TeletextEnhancementSource {
                        object: None,
                        command: 4,
                    })
            })
            .unwrap();
        assert_eq!(result.origins()[index].requested_size(), None);
        assert_eq!(result.origins()[index].effective_size(), None);
        assert_eq!(
            result.pending()[result.origins()[index].pending().unwrap()].reason(),
            TeletextDisplayPendingReason::UnknownSize
        );
        assert_display_contenders(&result, index, true);
    }
}
