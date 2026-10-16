//! A valid raw Base spacing latch is independent of assigned G0 glyph paint.

use super::*;

fn spacing(seed: TeletextSemanticService, neighbor: bool) -> TeletextSemanticService {
    let original_rows = seed.physical_base_overlay(owner()).unwrap().rows();
    assert_eq!(original_rows.len(), 1);
    assert_eq!(original_rows[0].row(), 1);
    let mut cells = *original_rows[0].cells();
    assert_eq!(cells[6], b' ');
    assert_eq!(cells[7], 0x5b);
    cells[6] = 0x1a;
    if neighbor {
        cells[2..6].copy_from_slice(&[0x17, 0x1a, 0x7f, 0x07]);
    }
    seed.with_physical_base_overlays(
        vec![TeletextPhysicalBaseOverlay::new(
            owner(),
            vec![TeletextPhysicalBaseRow::new(1, cells).unwrap()],
        )
        .unwrap()],
        authored::semantic_limits(),
    )
    .unwrap()
}

fn assert_detached_strict(
    original: &crate::teletext::TeletextMainComposition,
    physical: &crate::teletext::TeletextPhysicalDisplayReplay,
) {
    let detached = crate::teletext::compare_teletext_physical_display(
        original,
        physical,
        polish_limits().display_comparison,
    )
    .unwrap();
    for column in [7, 8] {
        assert!(detached.diagnostics().iter().any(|diagnostic| {
            diagnostic.position == Some((1, column))
                && diagnostic.finding
                    == TeletextPhysicalComparisonFinding::Difference(
                        crate::teletext::TeletextPhysicalComparisonField::GlyphEffects,
                    )
        }));
    }
}

#[test]
fn compiler_owned_polish_spacing_preserves_settled_original_and_fallback() {
    let seed = spacing(enlarged_polish_base(None, true), false);
    let fallback = polish_fallback(&seed);
    let budget = polish_limits();
    let allocations = crate::teletext::allocate_teletext_resource_objects(
        seed.objects(),
        &[],
        budget.max_resource_objects,
    )
    .unwrap();
    let composed = crate::teletext::compile_teletext_composed_main_program(
        &seed,
        owner(),
        &allocations,
        budget.composition,
        budget.packing,
    )
    .unwrap();
    let original = composed.composition();
    let origin = assert_polish_original(original);
    assert_eq!(
        original.display_resolution().origins()[origin].effective_size(),
        Some(TeletextDisplaySize::DoubleWidth)
    );
    // Table 26 notes 11/12 do not give spacing mosaic separation a G0 effect.
    // Table 29 D5 still underlines G0; this existing font policy fills row9.
    let underlined = [0, 0x3fc, 0x18, 0x30, 0x1f8, 0xc0, 0x180, 0x3fc, 0, 0xfff];
    for column in [7, 8] {
        let cell = original.display_resolution().cell(1, column).unwrap();
        assert_eq!(cell.origin(), Some(origin));
        assert_eq!(cell.attributes().spacing_separated().value(), Some(true));
        assert_eq!(
            cell.attributes().underline_or_separated_mosaics().value(),
            Some(true)
        );
        let glyph = crate::teletext::project_teletext_comparison_glyph(original, cell).unwrap();
        assert_eq!(glyph.mask.unwrap().rows(), underlined);
        assert_eq!(
            glyph.effects,
            crate::teletext::TeletextComparisonGlyphEffects::Alphanumeric { underlined: true }
        );
    }
    let input = declared(seed.clone());
    let encoded = encode(&input);
    assert_eq!(encoded.declared(), &input);
    assert_eq!(display(&encoded).normalized().original(), original);
    assert_eq!(rows(display(&encoded).page()), rows(&fallback));
    assert_eq!(encoded.compiler_drcs_resources().len(), 1);
    let glyphs = encoded.compiler_drcs_resources()[0].table().glyphs();
    assert_eq!(glyphs.len(), 1);
    assert_eq!(glyphs[0].slot.get(), 1);
    assert_eq!(glyphs[0].bitmap.get(), underlined);
    let physical = display(&encoded).normalized().physical().replay();
    for column in [7, 8] {
        let attrs = physical
            .display_resolution()
            .cell(1, column)
            .unwrap()
            .attributes();
        assert_eq!(attrs.underline_or_separated_mosaics().value(), Some(false));
        assert_eq!(attrs.spacing_separated().value(), Some(false));
    }
    assert_detached_strict(original, physical);
    assert_eq!(encoded.declared().semantic(), &seed);
}

#[test]
fn polish_spacing_reuses_exact_authored_underline_bank_without_mutating_fallback() {
    let underlined = [0, 0x3fc, 0x18, 0x30, 0x1f8, 0xc0, 0x180, 0x3fc, 0, 0xfff];
    let seed = spacing(enlarged_polish_base(Some((underlined, true)), true), false);
    let before = seed.clone();
    let fallback = polish_fallback(&seed);
    assert!(rows(&fallback).iter().any(|record| matches!(record,
        TeletextEncodedDisplayRecord::BaseRow {row:1,cells} if cells[6]==0x1a && cells[7]==0x5b)));
    let normalized =
        compile_teletext_normalized_main_program(&seed, owner(), polish_limits()).unwrap();
    let original = normalized.original();
    let origin = assert_polish_original(original);
    assert_eq!(
        original
            .display_resolution()
            .origin_attributes(origin)
            .unwrap()
            .spacing_separated()
            .value(),
        Some(true)
    );
    let physical = normalized.physical().replay();
    let index = selected(physical.display_resolution(), 1, 7);
    let record = physical
        .drcs_execution()
        .records()
        .iter()
        .find(|record| record.candidate_index() == index)
        .unwrap();
    let table = &seed.drcs().subtables()[0];
    assert_eq!(record.resource(), table.identity());
    assert_eq!(record.reference().table, TeletextDrcsTableScope::Normal);
    assert_eq!(
        record.reference().subtable.get(),
        table.identity().logical_subpage().get()
    );
    assert_eq!(record.reference().slot, table.glyphs()[0].slot);
    assert_eq!(record.bitmap().get(), underlined);
    assert_detached_strict(original, physical);
    assert_eq!(seed, before);
    assert_eq!(polish_fallback(&seed), fallback);
}

#[test]
fn polish_spacing_carrier_preserves_neighboring_base_g1_separation() {
    let seed = spacing(enlarged_polish_base(None, true), true);
    let fallback = polish_fallback(&seed);
    let input = declared(seed);
    let encoded = encode(&input);
    assert_eq!(encoded.declared(), &input);
    assert_eq!(rows(display(&encoded).page()), rows(&fallback));
    let original = display(&encoded).normalized().original();
    let physical = display(&encoded).normalized().physical().replay();
    let cell = original.display_resolution().cell(1, 4).unwrap();
    assert_eq!(cell.attributes().spacing_separated().value(), Some(true));
    assert_eq!(
        cell.attributes().underline_or_separated_mosaics().value(),
        Some(false)
    );
    let index = selected(original.display_resolution(), 1, 4);
    let identity = TeletextGlyphIdentity::G1Mosaic {
        mosaic: TeletextMosaic::new(63).unwrap(),
    };
    assert_eq!(
        original.glyph_intent().records()[index].glyph(),
        Some(identity)
    );
    let expected = resolve_teletext_glyph_mask(identity, None, true).unwrap();
    assert_ne!(
        expected,
        resolve_teletext_glyph_mask(identity, None, false).unwrap()
    );
    let actual = crate::teletext::raster::comparison_physical_glyph(
        physical,
        physical.display_resolution().cell(1, 4).unwrap(),
    )
    .unwrap();
    assert_eq!(
        actual.effects,
        crate::teletext::TeletextComparisonGlyphEffects::Mosaic { separated: true }
    );
    assert_eq!(actual.mask, Some(expected));
    let options = TeletextColourPixelOptions {
        reveal_concealed: false,
        flash_phase: TeletextFlashPhase::Original,
    };
    let budget = crate::teletext::TeletextRasterBudget {
        max_pixels: 120_000,
        max_diagnostics: 4096,
    };
    let raster =
        crate::teletext::render_teletext_physical_display(physical, options, budget).unwrap();
    for (y, mask) in expected.rows().into_iter().enumerate() {
        for x in 0..12 {
            let rgb = if mask & (1 << (11 - x)) == 0 {
                [0, 0, 0]
            } else {
                [15, 15, 15]
            };
            assert!(
                matches!(raster.pixel(4*12+x,10+y), Some(crate::teletext::TeletextRasterPixel::Colour(TeletextColourPixel::Rgb(value))) if value.get()==rgb)
            );
        }
    }
    assert_detached_strict(original, physical);
}
