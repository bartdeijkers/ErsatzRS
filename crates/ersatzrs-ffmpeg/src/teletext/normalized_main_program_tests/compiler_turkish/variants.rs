//! Original effects, exact generated PTUs and literal detached comparison.

use super::*;

pub(super) fn expected_mask(slot: u8) -> [u16; 10] {
    let mut mask = match slot {
        0 | 1 => super::super::polish_carrier::POLISH_5B_MASK,
        2 | 3 => TURKISH_23_MASK,
        _ => panic!("only the four established national/effect slots are expected"),
    };
    if slot % 2 == 1 {
        mask[9] = 0xfff;
    }
    mask
}

pub(super) fn assert_ptu(encoded: &EncodedTeletextAuthoredService) {
    let generated = &encoded.compiler_drcs_resources()[0];
    let page = encoded
        .pages()
        .iter()
        .find(|page| {
            page.resource_header()
                .is_some_and(|header| header.identity() == generated.table().identity())
        })
        .unwrap();
    let header = page.resource_header().unwrap();
    assert_eq!(header.update_counter().get(), 0);
    assert_eq!(
        header.transmission(),
        TeletextResourceTransmission::FullUnchangedFirst
    );
    let projection = crate::teletext::project_encoded_teletext_drcs(page, 32).unwrap();
    for slot in 0_u8..4 {
        let present = generated
            .table()
            .glyphs()
            .iter()
            .any(|glyph| glyph.slot.get() == slot);
        let cells = projection.records.iter().find_map(|record| match record {
            TeletextEncodedDisplayRecord::BaseRow { row, cells } if *row == slot / 2 + 1 => {
                Some(cells)
            }
            _ => None,
        });
        let Some(cells) = cells else {
            assert!(!present, "a defined slot requires its actual PTU row");
            continue;
        };
        let offset = usize::from(slot % 2) * 20;
        if present {
            let actual = std::array::from_fn(|row| {
                (u16::from(cells[offset + row * 2] & 0x3f) << 6)
                    | u16::from(cells[offset + row * 2 + 1] & 0x3f)
            });
            assert_eq!(actual, expected_mask(slot));
        } else {
            assert_eq!(&cells[offset..offset + 20], &[b' '; 20]);
        }
    }
}

fn assert_response(
    display: &crate::teletext::EncodedTeletextCompiledDisplayPage,
    underlined: bool,
) {
    let original = display.normalized().original();
    let physical = display.normalized().physical().replay();
    for column in [7, 8] {
        let first = original.display_resolution().cell(1, column).unwrap();
        let second = physical.display_resolution().cell(1, column).unwrap();
        assert_eq!(first.fragment(), second.fragment());
        let glyph = crate::teletext::raster::comparison_physical_glyph(physical, second).unwrap();
        assert_eq!(glyph.mask.unwrap().rows(), turkish_mask(underlined));
        assert_eq!(
            glyph.effects,
            crate::teletext::TeletextComparisonGlyphEffects::Undefined {
                d5: false,
                spacing_separated: false,
            }
        );
        let first = first.attributes();
        let second = second.attributes();
        assert_eq!(first.foreground().value(), second.foreground().value());
        assert_eq!(first.background().value(), second.background().value());
        assert_eq!(first.flash().value(), second.flash().value());
        assert_eq!(first.boxed().value(), second.boxed().value());
        assert_eq!(first.conceal().value(), second.conceal().value());
        assert_eq!(first.invert().value(), second.invert().value());
        assert_eq!(second.underline_or_separated_mosaics().value(), Some(false));
        assert_eq!(second.spacing_separated().value(), Some(false));
    }
    let detached = crate::teletext::compare_teletext_physical_display(
        original,
        physical,
        service_budget().normalization.display_comparison,
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

fn assert_pixels(display: &crate::teletext::EncodedTeletextCompiledDisplayPage) {
    let budget = crate::teletext::TeletextRasterBudget {
        max_pixels: 120_000,
        max_diagnostics: 4096,
    };
    let options = TeletextColourPixelOptions {
        reveal_concealed: false,
        flash_phase: TeletextFlashPhase::Original,
    };
    let first = crate::teletext::render_teletext_composition(
        display.normalized().original(),
        options,
        budget,
    )
    .unwrap();
    let second = crate::teletext::render_teletext_physical_display(
        display.normalized().physical().replay(),
        options,
        budget,
    )
    .unwrap();
    assert!(first.diagnostics().is_empty());
    assert!(second.diagnostics().is_empty());
    assert_eq!(first.width(), second.width());
    assert_eq!(first.pixels(), second.pixels());
}

#[test]
fn turkish_plain_underline_spacing_and_primary_alias_keep_exact_slots_and_ptus() {
    for primary in [0x16, 0x36] {
        // Adaptive D5=0 clears the original raw spacing latch. Its known
        // true original state is therefore exercised with the D5=1 repaint.
        for (underlined, spacing) in [(false, false), (true, false), (true, true)] {
            let seed = turkish_base(primary, underlined, spacing);
            let fallback = super::super::polish_carrier::polish_fallback(&seed);
            let input = declared(seed);
            let encoded = encode(&input);
            assert_eq!(encoded.declared(), &input);
            assert!(input.resource_inventory().is_empty());
            let display = display(&encoded);
            assert_original_case(
                display.normalized().original(),
                primary,
                underlined,
                spacing,
            );
            assert_eq!(rows(display.page()), rows(&fallback));
            let resource = &encoded.compiler_drcs_resources()[0];
            assert_eq!(encoded.compiler_drcs_resources().len(), 1);
            assert_eq!(resource.consumers(), &[owner()]);
            assert_eq!(
                resource.table().identity().kind(),
                TeletextResourceKind::Drcs
            );
            assert_eq!(resource.table().identity().logical_subpage().get(), 0);
            assert_eq!(resource.table().glyphs().len(), 1);
            let slot = 2 + u8::from(underlined);
            assert_eq!(resource.table().glyphs()[0].slot.get(), slot);
            assert_eq!(
                resource.table().glyphs()[0].bitmap.get(),
                turkish_mask(underlined)
            );
            let actual = display.normalized().physical().replay();
            let index = selected(actual.display_resolution(), 1, 7);
            let execution = actual
                .drcs_execution()
                .records()
                .iter()
                .find(|record| record.candidate_index() == index)
                .unwrap();
            assert_eq!(execution.reference().table, TeletextDrcsTableScope::Normal);
            assert_eq!(execution.reference().subtable.get(), 0);
            assert_eq!(execution.reference().slot.get(), slot);
            assert_eq!(execution.resource(), resource.table().identity());
            assert_eq!(execution.bitmap().get(), turkish_mask(underlined));
            assert_response(display, underlined);
            assert_ptu(&encoded);
            assert_pixels(display);
        }
    }
}

#[test]
fn turkish_exact_authored_bank_reuses_original_resource_and_literal_fallback() {
    for underlined in [false, true] {
        let input = declared(super::binding::authored_seed(
            turkish_mask(underlined),
            true,
            underlined,
        ));
        let fallback = super::super::polish_carrier::polish_fallback(input.semantic());
        let encoded = encode(&input);
        assert_eq!(encoded.declared(), &input);
        assert!(encoded.compiler_drcs_resources().is_empty());
        let display = display(&encoded);
        assert_original_case(display.normalized().original(), 0x16, underlined, false);
        let actual = display.normalized().physical().replay();
        let index = selected(actual.display_resolution(), 1, 7);
        let record = actual
            .drcs_execution()
            .records()
            .iter()
            .find(|record| record.candidate_index() == index)
            .unwrap();
        let table = &input.semantic().drcs().subtables()[0];
        assert_eq!(record.resource(), table.identity());
        assert_eq!(record.reference().subtable.get(), 3);
        assert_eq!(record.reference().slot.get(), 5);
        assert_eq!(record.bitmap().get(), turkish_mask(underlined));
        assert_eq!(rows(display.page()), rows(&fallback));
        assert_response(display, underlined);
        assert_pixels(display);
        let tx = authored::transmissions(&input);
        let expected = crate::teletext::encode_teletext_drcs_page(
            table,
            tx[0].counter,
            tx[0].update,
            crate::teletext::TeletextResourceTransmissionMode::Serial,
        )
        .unwrap();
        let page = encoded
            .pages()
            .iter()
            .find(|page| {
                page.resource_header()
                    .is_some_and(|header| header.identity() == table.identity())
            })
            .unwrap();
        assert_eq!(page.data_units(), expected.data_units());
    }
}
