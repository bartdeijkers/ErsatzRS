//! Physical projection pixels, distinct from original intent and native proof.

use super::*;
use crate::teletext::{
    allocate_teletext_resource_objects, compile_teletext_physical_display_program,
    render_teletext_composition, render_teletext_physical_display, TeletextRaster,
    TeletextRasterBudget, TeletextRasterError, TeletextRasterPixel,
};

const A: [u16; 10] = [0, 0x0f0, 0x198, 0x30c, 0x606, 0x7fe, 0x606, 0x606, 0, 0];
const POUND: [u16; 10] = [0, 0x0fc, 0x186, 0x0c0, 0x3f0, 0x0c0, 0x3e6, 0x33c, 0, 0];
const E: [u16; 10] = [0, 0, 0, 0x1fc, 0x306, 0x3fe, 0x300, 0x1fc, 0, 0];
const ACUTE_E: [u16; 10] = [0x060, 0x0c0, 0, 0x1fc, 0x306, 0x3fe, 0x300, 0x1fc, 0, 0];

fn options() -> TeletextColourPixelOptions {
    TeletextColourPixelOptions {
        reveal_concealed: false,
        flash_phase: TeletextFlashPhase::Original,
    }
}
fn pixels_budget() -> TeletextRasterBudget {
    TeletextRasterBudget {
        max_pixels: 216_000,
        max_diagnostics: 1800,
    }
}
fn limits() -> TeletextEnhancementCollectionBudget {
    TeletextEnhancementCollectionBudget {
        max_commands: 8192,
        max_drcs_references: 8,
    }
}
fn original_raster(composition: &TeletextMainComposition) -> TeletextRaster {
    render_teletext_composition(composition, options(), pixels_budget()).unwrap()
}
fn physical_raster(replay: &TeletextPhysicalDisplayReplay) -> TeletextRaster {
    render_teletext_physical_display(replay, options(), pixels_budget()).unwrap()
}
fn cell_rows(image: &TeletextRaster, row: usize, column: usize) -> [u16; 10] {
    std::array::from_fn(|y| {
        (0..12).fold(0, |bits, x| {
            let on = match image.pixel(column * 12 + x, row * 10 + y).unwrap() {
                TeletextRasterPixel::Colour(TeletextColourPixel::Rgb(rgb)) => match rgb.get() {
                    [15, 15, 15] => true,
                    [0, 0, 0] => false,
                    other => panic!("unexpected RGB {other:?}"),
                },
                other => panic!("unexpected pixel {other:?}"),
            };
            (bits << 1) | u16::from(on)
        })
    })
}
fn selected(display: &TeletextDisplayResolution, row: u8, column: u8) -> usize {
    display.origins()[display.cell(row, column).unwrap().origin().unwrap()].candidate_index()
}

#[test]
fn dropped_unicode_enhancements_render_transmitted_space_and_e_not_authored_replacements() {
    let service = Fixture::new(
        vec![text(1, 8, "£"), text(2, 8, "é"), text(4, 20, "A")],
        Some(vec![]),
    )
    .build();
    let composition = compose(&service);
    let retained = (service.clone(), composition.clone());
    let dropped = replay_teletext_physical_display(
        &service,
        &composition,
        &program(vec![]),
        service.objects(),
        limits(),
    )
    .unwrap();
    let physical = physical_raster(&dropped);
    let original = original_raster(&composition);
    assert_eq!(cell_rows(&physical, 1, 8), [0; 10]);
    assert_eq!(cell_rows(&physical, 2, 8), E);
    assert_eq!(cell_rows(&original, 1, 8), POUND);
    assert_eq!(cell_rows(&original, 2, 8), ACUTE_E);
    assert_eq!(cell_rows(&physical, 4, 20), A);
    assert_eq!(cell_rows(&original, 4, 20), A);
    assert!(physical.diagnostics().is_empty());
    assert_eq!(
        (physical.owner(), physical.width(), physical.height()),
        (owner(), 480, 250)
    );
    assert_eq!(
        physical.pixel(0, 0),
        Some(TeletextRasterPixel::ReceiverHeader)
    );
    assert_eq!((service, composition), retained);
}

#[test]
fn supplied_derived_glyphs_restore_unicode_without_modifying_base_or_inserting_twice() {
    let service = Fixture::new(
        vec![text(1, 8, "£"), text(2, 8, "é"), text(4, 20, "A")],
        Some(vec![]),
    )
    .build();
    let composition = compose(&service);
    let retained = composition.clone();
    let replay = replay_teletext_physical_display(
        &service,
        &composition,
        composition.program(),
        service.objects(),
        limits(),
    )
    .unwrap();
    let image = physical_raster(&replay);
    assert_eq!(cell_rows(&image, 1, 8), POUND);
    assert_eq!(cell_rows(&image, 2, 8), ACUTE_E);
    assert_eq!(cell_rows(&image, 4, 20), A);
    assert_eq!(image, original_raster(&composition));
    assert_eq!(composition, retained);
}

#[test]
fn current_physical_local_pound_differs_from_original_active_a_at_same_cell() {
    let target = local(0xffff);
    let mut fixture = Fixture::new(
        vec![text(1, 8, "£"), text(4, 20, "A")],
        Some(vec![
            position(1, 8),
            invoke(target, TeletextObjectType::Active),
        ]),
    );
    fixture
        .objects
        .push(definition(target, TeletextObjectType::Active, vec![g0(0)]));
    let service = fixture.build();
    let composition = compose(&service);
    let retained = (service.clone(), composition.clone());
    let replay = replay_teletext_physical_display(
        &service,
        &composition,
        composition.program(),
        service.objects(),
        limits(),
    )
    .unwrap();
    let original = original_raster(&composition);
    let physical = physical_raster(&replay);
    assert_eq!(cell_rows(&original, 1, 8), A);
    assert_eq!(cell_rows(&physical, 1, 8), POUND);
    assert_eq!(cell_rows(&original, 4, 20), cell_rows(&physical, 4, 20));
    assert_eq!(cell_rows(&physical, 4, 20), A);
    assert!(physical.diagnostics().is_empty());
    assert_eq!((service, composition), retained);
}

#[test]
fn final_packed_transmission_colours_replace_only_actual_pending_projection() {
    let service = Fixture::new(vec![text(1, 8, "A")], Some(vec![])).build();
    let composition = compose(&service);
    let retained = composition.clone();
    let actual = program(vec![
        position(1, 8),
        at(
            8,
            TeletextColumnOperation::Foreground {
                colour: TeletextColourReference::new(4).unwrap(),
            },
        ),
        at(
            8,
            TeletextColumnOperation::Foreground {
                colour: TeletextColourReference::new(13).unwrap(),
            },
        ),
        g0(8),
    ]);
    let pending = replay_teletext_physical_display(
        &service,
        &composition,
        &actual,
        service.objects(),
        limits(),
    )
    .unwrap();
    let allocations = allocate_teletext_resource_objects(service.objects(), &[], 4).unwrap();
    let packed = compile_teletext_physical_display_program(
        &service,
        &composition,
        &actual,
        service.objects(),
        &allocations,
        limits(),
        TeletextObjectCatalogBudget {
            max_objects: 4,
            max_commands: 64,
        },
    )
    .unwrap();
    let pending_image = physical_raster(&pending);
    assert!(matches!(
        pending_image.pixel(8 * 12 + 4, 11),
        Some(TeletextRasterPixel::Unresolved(_))
    ));
    let image = physical_raster(packed.replay());
    assert!(image.diagnostics().is_empty());
    // Literal A row1 has coverage at x4; the actual last physical writer is pink.
    assert!(matches!(
        image.pixel(8 * 12 + 4, 11),
        Some(TeletextRasterPixel::Colour(TeletextColourPixel::Rgb(rgb)))
            if rgb.get() == [7, 0, 7]
    ));
    assert!(matches!(
        original_raster(&composition).pixel(8 * 12 + 4, 11),
        Some(TeletextRasterPixel::Colour(TeletextColourPixel::Rgb(rgb)))
            if rgb.get() == [15, 15, 15]
    ));
    assert_eq!(composition, retained);
}

#[test]
fn shifted_drcs_and_added_carrier_pixels_follow_actual_candidate_indices_and_sources() {
    let bitmap = [0x800, 0x400, 0x200, 0x100, 0x80, 0x40, 0x20, 0x10, 0x8, 1];
    let service = drcs_fixture(bitmap).build();
    let composition = compose_drcs(&service);
    let retained = composition.clone();
    let reference = TeletextDrcsGlyphReference {
        table: TeletextDrcsTableScope::Normal,
        subtable: TeletextDrcsSubtableIndex::new(15).unwrap(),
        slot: TeletextDrcsGlyphSlot::new(47).unwrap(),
    };
    let carrier = local(0x0100);
    let mut definitions = service.objects().definitions().to_vec();
    definitions.push(definition(
        carrier,
        TeletextObjectType::Passive,
        vec![at(0, TeletextColumnOperation::Drcs { reference })],
    ));
    let catalog = TeletextEnhancementObjectCatalog::new(
        definitions,
        TeletextObjectCatalogBudget {
            max_objects: 16,
            max_commands: 2048,
        },
    )
    .unwrap();
    let mut commands = vec![TeletextEnhancementCommand::SetScreenColour {
        colour: TeletextColourReference::BLACK,
    }];
    commands.extend_from_slice(composition.program().commands());
    commands.extend([position(2, 8), invoke(carrier, TeletextObjectType::Passive)]);
    let actual = program(commands);
    let replay =
        replay_teletext_physical_display(&service, &composition, &actual, &catalog, limits())
            .unwrap();
    let image = physical_raster(&replay);
    // The original same-cell Global bitmap is [1;10]; the new carrier uses Normal.
    assert_eq!(cell_rows(&original_raster(&composition), 1, 8), [1; 10]);
    assert_eq!(cell_rows(&image, 1, 8), [1; 10]);
    assert_eq!(cell_rows(&image, 2, 8), bitmap);
    let selected = selected(replay.display_resolution(), 2, 8);
    let record = replay
        .drcs_execution()
        .records()
        .iter()
        .find(|record| record.candidate_index() == selected)
        .unwrap();
    assert_eq!(
        record.candidate().source(),
        Some(TeletextEnhancementSource {
            object: Some(carrier),
            command: 0
        })
    );
    assert_eq!(record.candidate().invocation().unwrap().get(), 2);
    assert_eq!(record.bitmap().get(), bitmap);
    assert!(!composition
        .drcs_execution()
        .records()
        .iter()
        .any(|record| record.candidate_index() == selected));
    assert_eq!(composition, retained);
}

#[test]
fn physical_diagnostics_keep_shifted_original_source_and_bound_whole_raster() {
    let target = local(0x0100);
    let mut fixture = Fixture::new(vec![text(4, 20, "A")], Some(vec![]));
    fixture.objects.push(definition(
        target,
        TeletextObjectType::Passive,
        vec![at(
            0,
            TeletextColumnOperation::G3 {
                code: code(0x6e),
                compatibility: TeletextG3Compatibility::Level25,
            },
        )],
    ));
    let service = fixture.build();
    let composition = compose(&service);
    let actual = program(vec![
        TeletextEnhancementCommand::SetScreenColour {
            colour: TeletextColourReference::BLACK,
        },
        position(1, 8),
        invoke(target, TeletextObjectType::Passive),
    ]);
    let replay = replay_teletext_physical_display(
        &service,
        &composition,
        &actual,
        service.objects(),
        limits(),
    )
    .unwrap();
    let retained = replay.clone();
    let image = physical_raster(&replay);
    let diagnostic = image
        .diagnostics()
        .iter()
        .find(|diagnostic| diagnostic.visual_position == (1, 8))
        .unwrap();
    assert_eq!(diagnostic.owner, owner());
    assert_eq!(diagnostic.original_position, Some((1, 8)));
    assert_eq!(
        diagnostic.location,
        Some(TeletextEnhancementSource {
            object: Some(target),
            command: 0
        })
    );
    assert_eq!(
        diagnostic.root_source,
        Some(TeletextSpatialRootSource::Local { command: 2 })
    );
    assert_eq!(diagnostic.invocation.unwrap().get(), 0);
    assert_eq!(
        diagnostic.candidate,
        Some(selected(replay.display_resolution(), 1, 8))
    );
    assert_eq!(cell_rows(&image, 4, 20), A);
    assert!(matches!(
        render_teletext_physical_display(
            &replay,
            options(),
            TeletextRasterBudget {
                max_pixels: 119_999,
                max_diagnostics: 0
            }
        ),
        Err(TeletextRasterError::PixelBudget {
            count: 120_000,
            maximum: 119_999
        })
    ));
    assert!(matches!(
        render_teletext_physical_display(
            &replay,
            options(),
            TeletextRasterBudget {
                max_pixels: 120_000,
                max_diagnostics: 0
            }
        ),
        Err(TeletextRasterError::DiagnosticBudget {
            count: 1,
            maximum: 0
        })
    ));
    let bounded = render_teletext_physical_display(
        &replay,
        options(),
        TeletextRasterBudget {
            max_pixels: 120_000,
            max_diagnostics: image.diagnostics().len(),
        },
    )
    .unwrap();
    assert_eq!(bounded, image);
    assert!(image.pixel(480, 0).is_none());
    assert!(image.pixel(0, 250).is_none());
    assert_eq!(replay, retained);
}
