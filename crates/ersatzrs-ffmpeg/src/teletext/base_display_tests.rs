//! Actual fallback bytes, checked spacing provenance and retained authored intent.
//! These traces do not resolve objects, render Unicode or suppress lower rows.

use ersatzrs_domain::*;

use super::base_display::*;
use super::main_composition::{compose_teletext_main_program, TeletextMainCompositionBudget};
use super::{layout, resolve_teletext_colour_context, TeletextEncodingError};

fn position(row: u8, column: u8) -> TeletextEnhancementPosition {
    TeletextEnhancementPosition::new(row, column).unwrap()
}

fn run(column: u8, style: TeletextStyle, text: &str) -> TeletextRun {
    TeletextRun {
        column,
        style,
        content: TeletextRunContent::Text { text: text.into() },
    }
}

fn row(row: u8, runs: Vec<TeletextRun>) -> TeletextRow {
    TeletextRow { row, runs }
}

fn service(
    rows: Vec<TeletextRow>,
    presentation: Option<TeletextLevel25PresentationDefaults>,
    main: Vec<TeletextEnhancementCommand>,
) -> TeletextSemanticService {
    let mut page = super::transport_tests::draft(200, 0x1234, false);
    page.national_option = TeletextNationalOption::new(1).unwrap();
    page.rows = rows;
    let address = page.address;
    TeletextSemanticService::new(
        TeletextResolvedServiceDraft {
            base: TeletextPageSetDraft {
                language: page.language.clone(),
                initial_page: address.page,
                pages: vec![page],
            },
            pages: vec![TeletextResolvedPageEnhancementDraft {
                address,
                presentation,
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
            objects: vec![],
            drcs: vec![],
        },
        TeletextServiceBudget {
            max_records: 16,
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

fn trace(service: &TeletextSemanticService) -> TeletextBaseDisplay {
    let page = &service.base().pages[0];
    let layout = layout::compile_for_composition(page).unwrap();
    let colours = resolve_teletext_colour_context(service, page.address, 2048).unwrap();
    trace_teletext_base_display(
        page,
        &layout,
        service.effective_presentation(page.address).unwrap(),
        &colours,
    )
    .unwrap()
}

fn cell(display: &TeletextBaseDisplay, row: u8, column: u8) -> TeletextBaseDisplayCell {
    *display.cell(row, column).unwrap()
}

fn spacing(
    row: u8,
    column: u8,
    control: TeletextBaseSpacingControl,
) -> TeletextBaseAttributeSource {
    TeletextBaseAttributeSource::Spacing {
        position: position(row, column),
        control,
        timing: control.timing(),
    }
}

fn control_service() -> TeletextSemanticService {
    service(
        vec![
            row(
                1,
                vec![run(
                    8,
                    TeletextStyle {
                        foreground: TeletextColour::Red,
                        background: TeletextColour::Blue,
                        boxed: true,
                        flash: true,
                        conceal: true,
                        ..TeletextStyle::default()
                    },
                    "A",
                )],
            ),
            row(2, vec![run(8, TeletextStyle::default(), "A")]),
        ],
        None,
        vec![],
    )
}

#[test]
fn literal_spacing_cells_apply_set_at_and_set_after_with_exact_sources() {
    let display = trace(&control_service());
    let codes: Vec<_> = (1..=8)
        .map(|column| cell(&display, 1, column).code())
        .collect();
    assert_eq!(codes, [0x04, 0x1d, 0x01, 0x0b, 0x0b, 0x08, 0x18, b'A']);
    assert_eq!(cell(&display, 1, 1).attributes().raw_foreground(), 7);
    assert_eq!(cell(&display, 1, 2).attributes().raw_foreground(), 4);
    assert_eq!(cell(&display, 1, 2).attributes().raw_background(), 4);
    assert_eq!(cell(&display, 1, 3).attributes().raw_foreground(), 4);
    assert_eq!(cell(&display, 1, 4).attributes().raw_foreground(), 1);
    assert!(!cell(&display, 1, 4).attributes().boxed());
    assert!(cell(&display, 1, 5).attributes().boxed());
    assert!(!cell(&display, 1, 6).attributes().flash());
    assert!(cell(&display, 1, 7).attributes().flash());
    assert!(cell(&display, 1, 7).attributes().conceal());
    let sources = cell(&display, 1, 8).attributes().sources();
    assert_eq!(
        sources.foreground,
        spacing(
            1,
            3,
            TeletextBaseSpacingControl::AlphaColour {
                colour: TeletextColour::Red
            }
        )
    );
    assert_eq!(
        sources.background,
        spacing(1, 2, TeletextBaseSpacingControl::NewBackground)
    );
    assert_eq!(
        sources.boxed,
        spacing(1, 4, TeletextBaseSpacingControl::StartBox)
    );
    assert_eq!(
        sources.flash,
        spacing(1, 6, TeletextBaseSpacingControl::Flash)
    );
    assert_eq!(
        sources.conceal,
        spacing(1, 7, TeletextBaseSpacingControl::Conceal)
    );
    assert_eq!(sources.foreground.effective_start_column(), 4);
    assert_eq!(sources.background.effective_start_column(), 2);
    assert!(cell(&display, 1, 7).authored().is_none());
}

#[test]
fn fixed_geometry_header_ownership_and_each_row_reset_are_explicit() {
    let display = trace(&control_service());
    assert_eq!(display.cells().len(), 25);
    for row in 0..25 {
        for column in 0..40 {
            assert_eq!(
                cell(&display, row, column).position(),
                position(row, column)
            );
        }
    }
    for column in 0..8 {
        assert_eq!(
            cell(&display, 0, column).kind(),
            TeletextBaseDisplayCellKind::HeaderOwned
        );
        assert_eq!(cell(&display, 0, column).code(), b' ');
        assert!(cell(&display, 0, column).authored().is_none());
    }
    assert_eq!(
        cell(&display, 0, 8).attributes().sources().foreground,
        TeletextBaseAttributeSource::RowDefault {
            row: 0,
            start_column: 8
        }
    );
    let reset = cell(&display, 2, 8).attributes();
    assert_eq!((reset.raw_foreground(), reset.raw_background()), (7, 0));
    assert!(!reset.boxed() && !reset.flash() && !reset.conceal() && !reset.graphics());
    assert_eq!(
        reset.sources().foreground,
        TeletextBaseAttributeSource::RowDefault {
            row: 2,
            start_column: 0
        }
    );
    assert!(display.cell(25, 0).is_none());
    assert!(display.cell(0, 40).is_none());
    assert_eq!(
        spacing(1, 39, TeletextBaseSpacingControl::Flash).effective_start_column(),
        40
    );
}

#[test]
fn new_background_maps_raw_foreground_through_the_background_clut() {
    let mut presentation =
        super::presentation::western_latin_v1_presentation(TeletextNationalOption::new(1).unwrap())
            .unwrap();
    presentation.remapping = TeletextColourRemapping::FgClut0BgClut2;
    let display = trace(&service(
        vec![row(
            1,
            vec![run(
                8,
                TeletextStyle {
                    foreground: TeletextColour::Red,
                    background: TeletextColour::Red,
                    ..TeletextStyle::default()
                },
                "A",
            )],
        )],
        Some(presentation),
        vec![],
    ));
    assert_eq!(cell(&display, 1, 6).code(), 0x01);
    assert_eq!(cell(&display, 1, 7).code(), 0x1d);
    let before = cell(&display, 1, 6).attributes();
    assert_eq!(
        (before.foreground().get(), before.background().get()),
        (7, 16)
    );
    let after = cell(&display, 1, 7).attributes();
    assert_eq!((after.raw_foreground(), after.raw_background()), (1, 1));
    assert_eq!(
        (after.foreground().get(), after.background().get()),
        (1, 17)
    );
    assert_eq!(display.presentation(), Some(presentation));
}

#[test]
fn implicit_black_substitution_uses_original_row_colour_but_new_background_does_not() {
    let mut presentation =
        super::presentation::western_latin_v1_presentation(TeletextNationalOption::new(1).unwrap())
            .unwrap();
    presentation.black_background_substitution = true;
    presentation.screen_colour = TeletextColourReference::new(13).unwrap();
    let service = service(
        vec![row(
            1,
            vec![
                run(8, TeletextStyle::default(), "A"),
                run(
                    12,
                    TeletextStyle {
                        background: TeletextColour::Red,
                        ..TeletextStyle::default()
                    },
                    "B",
                ),
            ],
        )],
        Some(presentation),
        vec![TeletextEnhancementCommand::SetRowColour {
            row: 1,
            colour: TeletextColourReference::new(6).unwrap(),
            following_rows: false,
        }],
    );
    let display = trace(&service);
    assert_eq!(cell(&display, 1, 8).attributes().background().get(), 6);
    assert_eq!(cell(&display, 1, 9).code(), 0x01);
    assert_eq!(cell(&display, 1, 10).code(), 0x1d);
    assert_eq!(cell(&display, 1, 10).attributes().background().get(), 1);
    assert_eq!(cell(&display, 1, 12).attributes().background().get(), 1);
    assert_eq!(cell(&display, 2, 0).attributes().background().get(), 0);
}

#[test]
fn subsequent_alpha_colour_clears_conceal_only_after_its_own_spacing_cell() {
    let display = trace(&service(
        vec![row(
            1,
            vec![
                run(
                    8,
                    TeletextStyle {
                        conceal: true,
                        ..TeletextStyle::default()
                    },
                    "A",
                ),
                run(
                    11,
                    TeletextStyle {
                        foreground: TeletextColour::Red,
                        ..TeletextStyle::default()
                    },
                    "B",
                ),
            ],
        )],
        None,
        vec![],
    ));
    assert_eq!(cell(&display, 1, 7).code(), 0x18);
    assert!(cell(&display, 1, 7).attributes().conceal());
    assert_eq!(cell(&display, 1, 10).code(), 0x01);
    assert!(cell(&display, 1, 10).attributes().conceal());
    assert!(!cell(&display, 1, 11).attributes().conceal());
    assert_eq!(
        cell(&display, 1, 11).attributes().sources().conceal,
        spacing(
            1,
            10,
            TeletextBaseSpacingControl::AlphaColour {
                colour: TeletextColour::Red
            }
        )
    );
}

#[test]
fn monochrome_mosaic_bits_separation_and_text_switch_retain_distinct_intent() {
    let mosaics = [
        TeletextMosaic::new(1).unwrap(),
        TeletextMosaic::new(32).unwrap(),
    ];
    let style = TeletextStyle {
        foreground: TeletextColour::Cyan,
        ..TeletextStyle::default()
    };
    let display = trace(&service(
        vec![row(
            3,
            vec![
                TeletextRun {
                    column: 6,
                    style,
                    content: TeletextRunContent::Mosaics {
                        cells: mosaics.to_vec(),
                        separated: true,
                    },
                },
                run(10, TeletextStyle::default(), "A"),
            ],
        )],
        None,
        vec![],
    ));
    assert_eq!(cell(&display, 3, 4).code(), 0x16);
    assert_eq!(cell(&display, 3, 5).code(), 0x1a);
    for (column, code, mosaic) in [(6, 0x21, mosaics[0]), (7, 0x60, mosaics[1])] {
        let glyph = cell(&display, 3, column);
        assert_eq!(glyph.code(), code);
        assert!(glyph.attributes().graphics() && glyph.attributes().separated());
        let authored = glyph.authored().unwrap();
        assert_eq!(
            authored.content(),
            TeletextBaseAuthoredContent::Mosaic {
                mosaic,
                separated: true
            }
        );
        assert_eq!(authored.run_column(), 6);
        assert_eq!(authored.style(), style);
    }
    assert_eq!(
        cell(&display, 3, 8).kind(),
        TeletextBaseDisplayCellKind::Blank
    );
    assert!(cell(&display, 3, 8).attributes().graphics());
    assert_eq!(cell(&display, 3, 9).code(), 0x07);
    assert!(cell(&display, 3, 9).attributes().graphics());
    assert!(!cell(&display, 3, 10).attributes().graphics());
}

#[test]
fn double_height_origins_and_normal_height_set_at_do_not_suppress_other_rows() {
    let display = trace(&service(
        vec![row(
            1,
            vec![
                run(
                    8,
                    TeletextStyle {
                        height: TeletextHeight::Double,
                        ..TeletextStyle::default()
                    },
                    "A",
                ),
                run(11, TeletextStyle::default(), "B"),
            ],
        )],
        None,
        vec![],
    ));
    assert_eq!(cell(&display, 1, 7).code(), 0x0d);
    assert_eq!(cell(&display, 1, 7).double_height_origin(), None);
    assert_eq!(
        cell(&display, 1, 8).double_height_origin(),
        Some(position(1, 8))
    );
    assert_eq!(
        cell(&display, 1, 8).attributes().sources().height,
        spacing(1, 7, TeletextBaseSpacingControl::DoubleHeight)
    );
    assert_eq!(cell(&display, 1, 10).code(), 0x0c);
    assert!(!cell(&display, 1, 10).attributes().double_height());
    assert_eq!(
        cell(&display, 1, 10).attributes().sources().height,
        spacing(1, 10, TeletextBaseSpacingControl::NormalHeight)
    );
    // Existing page validation reserves the authored lower row. The trace
    // retains its actual blank coded cells without performing display suppression.
    assert_eq!(cell(&display, 2, 8).code(), b' ');
    assert_eq!(
        cell(&display, 2, 8).kind(),
        TeletextBaseDisplayCellKind::Blank
    );
    assert_eq!(
        cell(&display, 2, 8).attributes().sources().height,
        TeletextBaseAttributeSource::RowDefault {
            row: 2,
            start_column: 0
        }
    );
    assert_eq!(cell(&display, 2, 8).double_height_origin(), None);
}

#[test]
fn colour_context_from_another_exact_subcode_cannot_label_a_base_trace() {
    let service = service(vec![], None, vec![]);
    let original = &service.base().pages[0];
    let colours = resolve_teletext_colour_context(&service, original.address, 32).unwrap();
    let mut page = original.clone();
    page.address.subcode = TeletextSubcode::new(0x1235).unwrap();
    let layout = layout::compile_for_composition(&page).unwrap();
    let error = trace_teletext_base_display(
        &page,
        &layout,
        service.effective_presentation(original.address).unwrap(),
        &colours,
    )
    .unwrap_err();
    assert_eq!(error.owner, page.address);
    assert_eq!(error.position, None);
    assert_eq!(
        error.reason,
        TeletextBaseDisplayFailure::ContextOwner {
            actual: original.address
        }
    );
}

#[test]
fn a_layout_glyph_that_disagrees_with_authored_intent_has_a_located_error() {
    let service = service(
        vec![row(1, vec![run(8, TeletextStyle::default(), "A")])],
        None,
        vec![],
    );
    let page = &service.base().pages[0];
    let mut layout = layout::compile_for_composition(page).unwrap();
    layout.rows[1][8] = b'B';
    let colours = resolve_teletext_colour_context(&service, page.address, 32).unwrap();
    let error = trace_teletext_base_display(
        page,
        &layout,
        service.effective_presentation(page.address).unwrap(),
        &colours,
    )
    .unwrap_err();
    assert_eq!(error.owner, page.address);
    assert_eq!(error.position, Some(position(1, 8)));
    assert_eq!(
        error.reason,
        TeletextBaseDisplayFailure::FallbackMismatch {
            expected: b'A',
            actual: b'B'
        }
    );
}

#[test]
fn original_unicode_is_not_inferred_from_fallback_codes_or_enhancement_priority() {
    let display = trace(&service(
        vec![row(1, vec![run(8, TeletextStyle::default(), "£éA")])],
        None,
        vec![],
    ));
    assert_eq!(display.primary().get(), 1);
    assert_eq!(display.national_option().get(), 1);
    for (column, fallback, character) in [(8, b' ', '£'), (9, b'e', 'é'), (10, b'A', 'A')] {
        let glyph = cell(&display, 1, column);
        assert_eq!(glyph.code(), fallback);
        assert_eq!(glyph.kind(), TeletextBaseDisplayCellKind::Glyph);
        let authored = glyph.authored().unwrap();
        assert_eq!(
            authored.content(),
            TeletextBaseAuthoredContent::Text { character }
        );
        assert_eq!(authored.position(), position(1, column));
        assert_eq!(authored.run_column(), 8);
    }
}

#[test]
fn composition_retains_the_same_trace_when_authored_compatible_glyph_suppresses_derivation() {
    let main = vec![
        TeletextEnhancementCommand::SetPosition {
            position: position(1, 9),
        },
        TeletextEnhancementCommand::AtColumn {
            column: 9,
            operation: TeletextColumnOperation::G2 {
                code: TeletextGlyphCode::new(0x25).unwrap(),
            },
        },
    ];
    let service = service(
        vec![row(1, vec![run(9, TeletextStyle::default(), "é")])],
        None,
        main.clone(),
    );
    let before = service.clone();
    let expected = trace(&service);
    let composed = compose_teletext_main_program(
        &service,
        service.base().pages[0].address,
        TeletextMainCompositionBudget {
            max_input_commands: 32,
            max_output_commands: 32,
            collection: TeletextEnhancementCollectionBudget {
                max_commands: 32,
                max_drcs_references: 0,
            },
        },
    )
    .unwrap();
    assert_eq!(composed.base_display(), &expected);
    assert_eq!(composed.program().commands(), main);
    assert_eq!(cell(composed.base_display(), 1, 9).code(), b'e');
    assert_eq!(
        cell(composed.base_display(), 1, 9)
            .authored()
            .unwrap()
            .content(),
        TeletextBaseAuthoredContent::Text { character: 'é' }
    );
    assert_eq!(service, before);
}

#[test]
fn reserved_legacy_national_bits_fail_before_a_base_trace_can_be_obtained() {
    let mut page = super::transport_tests::draft(200, 0, false);
    page.national_option = TeletextNationalOption::new(7).unwrap();
    assert_eq!(
        layout::compile_for_composition(&page).err(),
        Some(TeletextEncodingError::NationalOption {
            address: page.address
        })
    );
}

fn compose_base_fragments(service: &TeletextSemanticService) -> super::TeletextMainComposition {
    compose_teletext_main_program(
        service,
        service.base().pages[0].address,
        TeletextMainCompositionBudget {
            max_input_commands: 128,
            max_output_commands: 128,
            collection: TeletextEnhancementCollectionBudget {
                max_commands: 128,
                max_drcs_references: 0,
            },
        },
    )
    .unwrap()
}

#[test]
fn composition_retains_exact_base_fragment_origins_and_separate_mapped_row_defaults() {
    let mut presentation =
        super::presentation::western_latin_v1_presentation(TeletextNationalOption::new(1).unwrap())
            .unwrap();
    presentation.remapping = TeletextColourRemapping::FgClut2BgClut1;
    presentation.black_background_substitution = true;
    presentation.row_colour = TeletextColourReference::new(18).unwrap();
    let service = service(
        vec![row(
            1,
            vec![run(
                8,
                TeletextStyle {
                    foreground: TeletextColour::Red,
                    background: TeletextColour::Blue,
                    height: TeletextHeight::Double,
                    ..TeletextStyle::default()
                },
                "A",
            )],
        )],
        Some(presentation),
        vec![],
    );
    let original = service.clone();
    let composed = compose_base_fragments(&service);
    let footprints = composed.base_footprints();
    assert_eq!(footprints.owner(), service.base().pages[0].address);
    assert_eq!(footprints.cells().len(), 1000);
    assert_eq!(
        footprints.row(2).unwrap().suppressed_by(),
        Some(position(1, 7))
    );
    for (row, column, kind, origin) in [
        (1, 7, TeletextBaseFragmentKind::Normal, position(1, 7)),
        (
            1,
            8,
            TeletextBaseFragmentKind::DoubleHeightTop,
            position(1, 8),
        ),
        (
            2,
            8,
            TeletextBaseFragmentKind::DoubleHeightBottom,
            position(1, 8),
        ),
        (
            2,
            6,
            TeletextBaseFragmentKind::BackgroundOnly,
            position(1, 6),
        ),
    ] {
        let fragment = footprints.cell(row, column).unwrap();
        assert_eq!(fragment.coded_position(), position(row, column));
        assert_eq!(fragment.kind(), kind);
        assert_eq!(fragment.origin(), Some(origin));
        assert_eq!(fragment.pending(), None);
    }
    let origin = cell(composed.base_display(), 1, 8);
    assert_eq!(origin.code(), b'A');
    assert_eq!(origin.attributes().values().foreground.get(), 17);
    assert_eq!(origin.attributes().values().background.get(), 12);
    // The lower coded cell remains its own blank/reset trace, not a cloned A.
    assert_eq!(cell(composed.base_display(), 2, 8).code(), b' ');
    let baseline = footprints.row(2).unwrap().baseline();
    assert_eq!(baseline.values().foreground.get(), 23);
    assert_eq!(baseline.values().background.get(), 18);
    assert!(!baseline.values().double_height);
    assert_eq!(
        baseline.sources().background,
        TeletextBaseAttributeSource::RowDefault {
            row: 2,
            start_column: 0,
        }
    );
    assert_eq!(service, original);
}

#[test]
fn composition_keeps_nonspacing_height_separate_from_spacing_lower_row_suppression() {
    let service = service(
        vec![
            row(1, vec![run(8, TeletextStyle::default(), "A")]),
            row(
                2,
                vec![run(
                    4,
                    TeletextStyle {
                        foreground: TeletextColour::Red,
                        ..TeletextStyle::default()
                    },
                    "B",
                )],
            ),
        ],
        None,
        vec![
            TeletextEnhancementCommand::SetPosition {
                position: position(1, 8),
            },
            TeletextEnhancementCommand::AtColumn {
                column: 8,
                operation: TeletextColumnOperation::Attributes {
                    double_height: true,
                    double_width: false,
                    boxed: false,
                    conceal: false,
                    invert: false,
                    underline_or_separated_mosaics: false,
                },
            },
        ],
    );
    let before = service.clone();
    let composed = compose_base_fragments(&service);
    let footprints = composed.base_footprints();
    assert_eq!(
        footprints.row(1).unwrap().first_double_height_control(),
        None
    );
    assert_eq!(footprints.row(2).unwrap().suppressed_by(), None);
    assert_eq!(
        footprints.cell(1, 8).unwrap().kind(),
        TeletextBaseFragmentKind::Normal
    );
    assert_eq!(
        footprints.cell(2, 4).unwrap().kind(),
        TeletextBaseFragmentKind::Normal
    );
    assert_eq!(cell(composed.base_display(), 2, 4).code(), b'B');
    assert_eq!(
        cell(composed.base_display(), 2, 4)
            .attributes()
            .values()
            .foreground
            .get(),
        1
    );
    assert_eq!(
        composed
            .attribute_execution()
            .before_adaptive_cell(1, 8)
            .unwrap()
            .double_height()
            .value(),
        Some(true)
    );
    assert_eq!(service, before);
}

fn reconstruct(
    display: &TeletextBaseDisplay,
    cells: Box<[[TeletextBaseDisplayCell; 40]; 25]>,
) -> Result<TeletextBaseDisplay, TeletextBaseDisplayValueError> {
    TeletextBaseDisplay::new(
        display.owner(),
        display.primary(),
        display.national_option(),
        display.presentation(),
        cells,
    )
}

fn with_sources(
    cell: TeletextBaseDisplayCell,
    sources: TeletextBaseAttributeSources,
) -> TeletextBaseDisplayCell {
    TeletextBaseDisplayCell::new(
        cell.position(),
        cell.code(),
        cell.kind(),
        cell.authored(),
        TeletextBaseDisplayAttributes::new(cell.attributes().values(), sources).unwrap(),
    )
    .unwrap()
}

#[test]
fn forged_earlier_foreground_source_must_point_to_a_real_matching_control_cell() {
    let display = trace(&service(
        vec![row(
            1,
            vec![
                run(1, TeletextStyle::default(), "A"),
                run(8, TeletextStyle::default(), "B"),
            ],
        )],
        None,
        vec![],
    ));
    let mut cells = Box::new(*display.cells());
    let mut sources = cells[1][8].attributes().sources();
    sources.foreground = spacing(
        1,
        1,
        TeletextBaseSpacingControl::AlphaColour {
            colour: TeletextColour::Red,
        },
    );
    cells[1][8] = with_sources(cells[1][8], sources);
    assert_eq!(
        reconstruct(&display, cells).unwrap_err(),
        TeletextBaseDisplayValueError::AttributeOrigin
    );
}

#[test]
fn an_unconfirmed_single_start_box_cannot_be_used_as_attribute_provenance() {
    let display = trace(&control_service());
    let mut cells = Box::new(*display.cells());
    let original = cells[1][5];
    cells[1][5] = TeletextBaseDisplayCell::new(
        original.position(),
        b' ',
        TeletextBaseDisplayCellKind::Blank,
        None,
        original.attributes(),
    )
    .unwrap();
    assert_eq!(
        reconstruct(&display, cells).unwrap_err(),
        TeletextBaseDisplayValueError::AttributeOrigin
    );
}
