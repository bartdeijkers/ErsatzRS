//! Actual composition-to-pixel checks; expected shapes are literal font data.

use super::*;
use crate::teletext::{
    render_teletext_composition, TeletextRaster, TeletextRasterBudget, TeletextRasterCellFailure,
    TeletextRasterError, TeletextRasterPixel,
};

const A: [u16; 10] = [0, 0x0f0, 0x198, 0x30c, 0x606, 0x7fe, 0x606, 0x606, 0, 0];
const WIDE_A: [u32; 10] = [
    0,
    0x0000_ff00,
    0x0003_c3c0,
    0x000f_00f0,
    0x003c_003c,
    0x003f_fffc,
    0x003c_003c,
    0x003c_003c,
    0,
    0,
];

fn options() -> TeletextColourPixelOptions {
    TeletextColourPixelOptions {
        reveal_concealed: false,
        flash_phase: TeletextFlashPhase::Original,
    }
}
fn raster(plan: &TeletextMainComposition) -> TeletextRaster {
    render_teletext_composition(
        plan,
        options(),
        TeletextRasterBudget {
            max_pixels: 216_000,
            max_diagnostics: 1800,
        },
    )
    .unwrap()
}
fn cell_rows(raster: &TeletextRaster, row: usize, column: usize, cells: usize) -> Vec<u32> {
    (0..10)
        .map(|y| {
            (0..12 * cells).fold(0, |bits, x| {
                let pixel = raster.pixel(column * 12 + x, row * 10 + y).unwrap();
                let on = match pixel {
                    TeletextRasterPixel::Colour(TeletextColourPixel::Rgb(rgb)) => match rgb.get() {
                        [15, 15, 15] => true,
                        [0, 0, 0] => false,
                        unexpected => panic!("unexpected colour {unexpected:?}"),
                    },
                    unexpected => panic!("unresolved pixel {unexpected:?}"),
                };
                (bits << 1) | u32::from(on)
            })
        })
        .collect()
}

#[test]
fn selected_base_and_local_g0_use_same_composition_and_literal_masks() {
    let service = Fixture::new(vec![text(1, 8, "A")], Some(vec![position(2, 8), g0(8)])).build();
    let before = service.clone();
    let plan = compose(&service);
    let result = raster(&plan);
    assert_eq!(cell_rows(&result, 1, 8, 1), A.map(u32::from));
    assert_eq!(cell_rows(&result, 2, 8, 1), A.map(u32::from));
    assert_eq!(result.owner(), owner());
    assert_eq!((result.width(), result.height()), (480, 250));
    assert_eq!(result.pixels().len(), 120_000);
    assert_eq!(result.diagnostics().len(), 8);
    assert!(result
        .diagnostics()
        .iter()
        .all(|diagnostic| diagnostic.visual_position.0 == 0
            && diagnostic.visual_position.1 < 8
            && matches!(
                diagnostic.reason,
                TeletextRasterCellFailure::MissingDisplayCell
            )));
    assert!(result.pixel(480, 0).is_none());
    assert!(result.pixel(0, 250).is_none());
    assert!(result.pixel(usize::MAX, usize::MAX).is_none());
    assert_eq!(service, before);
}

#[test]
fn authored_unicode_mask_is_lifted_at_base_priority_and_loses_to_active() {
    let identity = local(70);
    let mut fixture = Fixture::new(
        vec![text(1, 8, "é")],
        Some(vec![
            position(1, 8),
            invoke(identity, TeletextObjectType::Active),
        ]),
    );
    fixture.objects.push(definition(
        identity,
        TeletextObjectType::Active,
        vec![g0(0)],
    ));
    let service = fixture.build();
    let plan = compose(&service);
    assert_eq!(
        plan.glyph_layers().candidates_at(1, 8)[0].priority(),
        TeletextGlyphPriority::Base
    );
    let overridden = raster(&plan);
    assert_eq!(cell_rows(&overridden, 1, 8, 1), A.map(u32::from));
    let accented_base = compose(&Fixture::new(vec![text(1, 8, "é")], None).build());
    let accented = cell_rows(&raster(&accented_base), 1, 8, 1);
    let fallback = cell_rows(
        &raster(&compose(&Fixture::new(vec![text(1, 8, "e")], None).build())),
        1,
        8,
        1,
    );
    assert_ne!(accented, fallback);
    assert_eq!(accented[0], 0x060);
    assert_eq!(accented[1], 0x0c0);
    assert_eq!(accented[2..], fallback[2..]);
}

fn size(column: u8, wide: bool, high: bool) -> TeletextEnhancementCommand {
    at(
        column,
        TeletextColumnOperation::Attributes {
            double_width: wide,
            double_height: high,
            boxed: false,
            conceal: false,
            invert: false,
            underline_or_separated_mosaics: false,
        },
    )
}

#[test]
fn c3_width_height_and_four_double_size_fragments_rasterize_effective_occupancy() {
    for (wide, high) in [(true, false), (false, true), (true, true)] {
        let plan = compose(
            &Fixture::new(
                vec![],
                Some(vec![position(1, 8), size(8, wide, high), g0(8)]),
            )
            .build(),
        );
        let image = raster(&plan);
        let upper = cell_rows(&image, 1, 8, if wide { 2 } else { 1 });
        if high {
            let lower = cell_rows(&image, 2, 8, if wide { 2 } else { 1 });
            let joined: Vec<_> = upper.into_iter().chain(lower).collect();
            let expected: Vec<_> = if wide {
                WIDE_A.to_vec()
            } else {
                A.map(u32::from).to_vec()
            }
            .into_iter()
            .flat_map(|value| [value, value])
            .collect();
            assert_eq!(joined, expected);
        } else {
            assert_eq!(upper, WIDE_A);
        }
    }
}

#[test]
fn glyph_assignment_uncertainty_is_located_and_does_not_hide_known_cells() {
    let service = Fixture::new(
        vec![text(2, 8, "A")],
        Some(vec![
            position(1, 8),
            at(
                8,
                TeletextColumnOperation::G3 {
                    code: code(0x6e),
                    compatibility: TeletextG3Compatibility::Level25,
                },
            ),
        ]),
    )
    .build();
    let plan = compose(&service);
    let image = raster(&plan);
    let diagnostic = image
        .diagnostics()
        .iter()
        .find(|d| d.visual_position == (1, 8))
        .unwrap();
    assert_eq!(diagnostic.owner, owner());
    assert_eq!(diagnostic.original_position, Some((1, 8)));
    assert_eq!(
        diagnostic.location,
        Some(TeletextEnhancementSource {
            object: None,
            command: 1
        })
    );
    assert!(matches!(
        diagnostic.reason,
        TeletextRasterCellFailure::Display { .. }
            | TeletextRasterCellFailure::GlyphIntent {
                disposition: TeletextGlyphDisposition::ShouldNotTransmit,
                ..
            }
    ));
    let TeletextRasterPixel::Unresolved(index) = image.pixel(8 * 12, 10).unwrap() else {
        panic!("expected explicit pending pixel")
    };
    assert_eq!(&image.diagnostics()[index], diagnostic);
    assert_eq!(cell_rows(&image, 2, 8, 1), A.map(u32::from));
}

#[test]
fn caller_pixel_and_diagnostic_budgets_return_no_partial_raster() {
    let plan = compose(&Fixture::new(vec![text(1, 8, "A")], None).build());
    let before = plan.clone();
    assert!(matches!(
        render_teletext_composition(
            &plan,
            options(),
            TeletextRasterBudget {
                max_pixels: 119_999,
                max_diagnostics: 1800
            }
        ),
        Err(TeletextRasterError::PixelBudget {
            count: 120_000,
            maximum: 119_999
        })
    ));
    assert!(matches!(
        render_teletext_composition(
            &plan,
            options(),
            TeletextRasterBudget {
                max_pixels: 120_000,
                max_diagnostics: 7
            }
        ),
        Err(TeletextRasterError::DiagnosticBudget {
            count: 8,
            maximum: 7
        })
    ));
    assert_eq!(raster(&plan).diagnostics().len(), 8);
    assert_eq!(plan, before);
}

#[test]
fn exact_drcs_scope_bitmap_and_same_id_generations_cannot_bleed_into_each_other() {
    let pixels = [
        0x800, 0x400, 0x200, 0x100, 0x080, 0x040, 0x020, 0x010, 0x008, 0x001,
    ];
    let mut fixture = drcs_fixture(pixels);
    // Remove the two higher-priority global overlays to see the normal glyph.
    fixture.main.as_mut().unwrap().truncate(2);
    let first = compose_drcs(&fixture.clone().build());
    let mut changed = fixture;
    let resource = changed.drcs[0].identity();
    changed.drcs[0] = TeletextDrcsSubtable::new(
        resource,
        vec![TeletextDrcsGlyphDefinition {
            slot: TeletextDrcsGlyphSlot::new(47).unwrap(),
            bitmap: TeletextDrcsBitmap::new([0; 10]).unwrap(),
        }],
    )
    .unwrap();
    let second = compose_drcs(&changed.build());
    assert_eq!(cell_rows(&raster(&first), 1, 8, 1), pixels.map(u32::from));
    assert_eq!(cell_rows(&raster(&second), 1, 8, 1), [0; 10]);
    assert_eq!(cell_rows(&raster(&first), 1, 8, 1), pixels.map(u32::from));
    let globally_overlaid = compose_drcs(&drcs_fixture(pixels).build());
    assert_eq!(cell_rows(&raster(&globally_overlaid), 1, 8, 1), [1; 10]);
    let normal = &globally_overlaid.drcs_execution().records()[0];
    let global = &globally_overlaid.drcs_execution().records()[1];
    assert_eq!(
        resolve_teletext_glyph_mask(
            TeletextGlyphIdentity::Drcs {
                reference: normal.reference()
            },
            Some(global),
            false
        ),
        Err(TeletextGlyphMaskError::DrcsReferenceMismatch)
    );
}

#[test]
fn visual_panels_use_original_identity_and_selected_row_plane() {
    let target = local(91);
    let mut fixture = split_panel_fixture(vec![
        position(1, 4),
        g0(4),
        TeletextEnhancementCommand::InvokeObject {
            target,
            object_type: TeletextObjectType::Passive,
            offset: Some(TeletextObjectOffset::new(0, 36).unwrap()),
        },
        TeletextEnhancementCommand::InvokeObject {
            target,
            object_type: TeletextObjectType::Passive,
            offset: Some(TeletextObjectOffset::new(0, 64).unwrap()),
        },
    ]);
    fixture
        .objects
        .push(definition(target, TeletextObjectType::Passive, vec![g0(0)]));
    let plan = compose(&fixture.build());
    let image = raster(&plan);
    assert_eq!(image.width(), 56 * 12);
    assert_eq!(cell_rows(&image, 1, 0, 1), A.map(u32::from));
    assert_eq!(cell_rows(&image, 1, 8, 1), A.map(u32::from));
    assert_eq!(cell_rows(&image, 1, 44, 1), A.map(u32::from));
}

#[test]
fn distinct_selected_palettes_coexist_without_mutating_previous_raster() {
    let make = |colour: [u8; 3]| {
        let mut fixture = Fixture::new(
            vec![],
            Some(vec![
                position(1, 8),
                at(
                    8,
                    TeletextColumnOperation::Foreground {
                        colour: TeletextColourReference::new(16).unwrap(),
                    },
                ),
                g0(8),
            ]),
        );
        let mut presentation = crate::teletext::presentation::western_latin_v1_presentation(
            TeletextNationalOption::new(1).unwrap(),
        )
        .unwrap();
        presentation.palette.clut2[0] = TeletextRgb::new(colour).unwrap();
        fixture.presentation = Some(presentation);
        compose(&fixture.build())
    };
    let first = make([15, 2, 3]);
    let second = make([1, 14, 5]);
    let a = raster(&first);
    let b = raster(&second);
    let pixel_colour = |image: &TeletextRaster| match image.pixel(8 * 12 + 4, 10 + 1).unwrap() {
        TeletextRasterPixel::Colour(TeletextColourPixel::Rgb(rgb)) => rgb.get(),
        other => panic!("unexpected pixel {other:?}"),
    };
    assert_eq!(pixel_colour(&a), [15, 2, 3]);
    assert_eq!(pixel_colour(&b), [1, 14, 5]);
    assert_eq!(pixel_colour(&raster(&first)), [15, 2, 3]);
}

#[test]
fn d5_underlines_authored_composites_and_spaces_but_separates_mosaics() {
    let d5 = at(
        8,
        TeletextColumnOperation::Attributes {
            double_height: false,
            double_width: false,
            boxed: false,
            conceal: false,
            invert: false,
            underline_or_separated_mosaics: true,
        },
    );
    let plan =
        compose(&Fixture::new(vec![text(1, 8, "é")], Some(vec![position(1, 8), d5])).build());
    let image = raster(&plan);
    assert_eq!(cell_rows(&image, 1, 8, 1)[9], 0xfff);
    assert_eq!(
        cell_rows(&image, 1, 9, 1),
        [0, 0, 0, 0, 0, 0, 0, 0, 0, 0xfff]
    );
    let plan = compose(
        &Fixture::new(
            vec![],
            Some(vec![
                position(1, 8),
                d5,
                at(
                    8,
                    TeletextColumnOperation::G1 {
                        mosaic: TeletextMosaic::new(63).unwrap(),
                    },
                ),
            ]),
        )
        .build(),
    );
    assert_eq!(
        cell_rows(&raster(&plan), 1, 8, 1),
        [0, 0x79e, 0x79e, 0, 0x79e, 0x79e, 0x79e, 0, 0x79e, 0x79e]
    );
}
