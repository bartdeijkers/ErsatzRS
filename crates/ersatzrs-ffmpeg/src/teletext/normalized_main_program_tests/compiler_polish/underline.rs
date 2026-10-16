//! Exact D5 response, fixed resource slots and counterfactual binding checks.

use super::*;
use crate::teletext::{
    compare_teletext_physical_display, render_teletext_composition,
    render_teletext_physical_display, TeletextComparisonGlyphEffects as Effects,
    TeletextRasterBudget,
};

fn mask(underlined: bool) -> [u16; 10] {
    let mut rows = POLISH_5B_MASK;
    if underlined {
        rows[9] = 0xfff;
    }
    rows
}

fn assert_response(
    display: &crate::teletext::EncodedTeletextCompiledDisplayPage,
    row: u8,
    underlined: bool,
) {
    let original = display.normalized().original();
    let actual = display.normalized().physical().replay();
    for column in [7, 8] {
        let first = original.display_resolution().cell(row, column).unwrap();
        let second = actual.display_resolution().cell(row, column).unwrap();
        assert_eq!(first.fragment(), second.fragment());
        let original_glyph =
            crate::teletext::project_teletext_comparison_glyph(original, first).unwrap();
        let actual_glyph =
            crate::teletext::raster::comparison_physical_glyph(actual, second).unwrap();
        assert_eq!(original_glyph.mask.unwrap().rows(), mask(underlined));
        assert_eq!(actual_glyph.mask, original_glyph.mask);
        assert_eq!(original_glyph.effects, Effects::Alphanumeric { underlined });
        assert_eq!(
            actual_glyph.effects,
            Effects::Undefined {
                d5: false,
                spacing_separated: false
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
        assert_eq!(
            first.underline_or_separated_mosaics().value(),
            Some(underlined)
        );
        assert_eq!(second.underline_or_separated_mosaics().value(), Some(false));
    }
    let original_index = original
        .display_resolution()
        .cell(row, 7)
        .unwrap()
        .origin()
        .unwrap();
    let origin = &original.display_resolution().origins()[original_index];
    assert_eq!(
        origin.effective_size(),
        Some(TeletextDisplaySize::DoubleWidth)
    );
    let candidate = original.glyph_layers().candidates()[origin.candidate_index()];
    assert_eq!(candidate.priority(), TeletextGlyphPriority::Base);
    assert_eq!(candidate.base().unwrap().code(), 0x5b);
    let actual_index = selected(actual.display_resolution(), row, 7);
    let record = actual
        .drcs_execution()
        .records()
        .iter()
        .find(|record| record.candidate_index() == actual_index)
        .unwrap();
    assert_eq!(record.reference().slot.get(), u8::from(underlined));
    assert_eq!(record.bitmap().get(), mask(underlined));
    let detached =
        compare_teletext_physical_display(original, actual, polish_limits().display_comparison)
            .unwrap();
    assert!(detached
        .diagnostics()
        .iter()
        .any(|diagnostic| diagnostic.position == Some((row, 7))));
}

fn assert_pixels(display: &crate::teletext::EncodedTeletextCompiledDisplayPage) {
    let budget = TeletextRasterBudget {
        max_pixels: 120_000,
        max_diagnostics: 4096,
    };
    for reveal_concealed in [false, true] {
        for flash_phase in [TeletextFlashPhase::Original, TeletextFlashPhase::Alternate] {
            let options = TeletextColourPixelOptions {
                reveal_concealed,
                flash_phase,
            };
            let original =
                render_teletext_composition(display.normalized().original(), options, budget)
                    .unwrap();
            let actual = render_teletext_physical_display(
                display.normalized().physical().replay(),
                options,
                budget,
            )
            .unwrap();
            assert!(
                original.diagnostics().is_empty(),
                "{:?}",
                original.diagnostics()
            );
            assert!(
                actual.diagnostics().is_empty(),
                "{:?}",
                actual.diagnostics()
            );
            assert_eq!(original.width(), actual.width());
            assert_eq!(original.pixels(), actual.pixels());
        }
    }
}

fn assert_ptu(encoded: &EncodedTeletextAuthoredService) {
    let generated = &encoded.compiler_drcs_resources()[0];
    let page = encoded
        .pages()
        .iter()
        .find(|page| {
            page.resource_header()
                .is_some_and(|header| header.identity() == generated.table().identity())
        })
        .unwrap();
    let projection = project_encoded_teletext_drcs(page, 32).unwrap();
    let cells = projection
        .records
        .iter()
        .find_map(|record| match record {
            TeletextEncodedDisplayRecord::BaseRow { row: 1, cells } => Some(cells),
            _ => None,
        })
        .unwrap();
    for glyph in generated.table().glyphs() {
        let offset = usize::from(glyph.slot.get()) * 20;
        let actual: [u16; 10] = std::array::from_fn(|row| {
            (u16::from(cells[offset + row * 2] & 0x3f) << 6)
                | u16::from(cells[offset + row * 2 + 1] & 0x3f)
        });
        assert_eq!(actual, mask(glyph.slot.get() == 1));
    }
    for slot in [0_u8, 1] {
        if !generated
            .table()
            .glyphs()
            .iter()
            .any(|glyph| glyph.slot.get() == slot)
        {
            let offset = usize::from(slot) * 20;
            assert_eq!(&cells[offset..offset + 20], &[b' '; 20]);
        }
    }
}

fn assert_original(
    input: &TeletextDeclaredService,
    display: &crate::teletext::EncodedTeletextCompiledDisplayPage,
) {
    let budget = polish_limits();
    let allocations = crate::teletext::allocate_teletext_resource_objects(
        input.semantic().objects(),
        &[],
        budget.max_resource_objects,
    )
    .unwrap();
    let original = crate::teletext::compile_teletext_composed_main_program(
        input.semantic(),
        display.page().address(),
        &allocations,
        budget.composition,
        budget.packing,
    )
    .unwrap();
    assert_eq!(display.normalized().original(), original.composition());
}

#[test]
fn polish_underline_mixed_origins_keep_both_fixed_slots_in_one_bank() {
    for first_underlined in [false, true] {
        let seed = super::super::polish_carrier::mixed_polish_base(first_underlined);
        let fallback = polish_fallback(&seed);
        let input = declared(seed);
        let encoded = encode(&input);
        assert_eq!(encoded.declared(), &input);
        assert!(input.resource_inventory().is_empty());
        assert_eq!(encoded.compiler_drcs_resources().len(), 1);
        let table = encoded.compiler_drcs_resources()[0].table();
        assert_eq!(table.glyphs().len(), 2);
        assert_eq!(table.glyphs()[0].slot.get(), 0);
        assert_eq!(table.glyphs()[1].slot.get(), 1);
        assert_eq!(rows(display(&encoded).page()), rows(&fallback));
        assert_response(display(&encoded), 1, first_underlined);
        assert_response(display(&encoded), 5, !first_underlined);
        assert_original(&input, display(&encoded));
        assert_ptu(&encoded);
        assert_pixels(display(&encoded));
    }
}

#[test]
fn polish_underline_consumer_order_and_variant_removal_preserve_binding_identity() {
    let second = TeletextPageAddress {
        page: TeletextPageNumber::new(201).unwrap(),
        subcode: owner().subcode,
    };
    let variants = [
        consumer_variants(&[owner(), second], &[second]),
        consumer_variants(&[second, owner()], &[second]),
        consumer_variants(&[second], &[second]),
        consumer_variants(&[owner()], &[]),
    ];
    let mut identity = None;
    for input in variants {
        let encoded = encode(&input);
        assert_eq!(encoded.declared(), &input);
        assert_eq!(encoded.compiler_drcs_resources().len(), 1);
        let table = encoded.compiler_drcs_resources()[0].table();
        assert_eq!(*identity.get_or_insert(table.identity()), table.identity());
        for glyph in table.glyphs() {
            assert_eq!(glyph.bitmap.get(), mask(glyph.slot.get() == 1));
        }
        for page in encoded.pages() {
            if let EncodedTeletextAuthoredPage::CompiledDisplay(display) = page {
                let underlined = display.page().address() == second;
                assert_response(display, 1, underlined);
                assert_original(&input, display);
                assert_pixels(display);
            }
        }
        assert_ptu(&encoded);
    }
}

fn styled_seed(case: u8) -> TeletextSemanticService {
    let seed = enlarged_polish_base(None, true);
    let mut objects = seed.objects().definitions().to_vec();
    let mut commands = objects[1].program.commands().to_vec();
    if let TeletextEnhancementCommand::AtColumn {
        operation:
            TeletextColumnOperation::Attributes {
                boxed,
                conceal,
                invert,
                ..
            },
        ..
    } = &mut commands[0]
    {
        *boxed = case == 1;
        *conceal = case == 2;
        *invert = case == 3;
    } else {
        unreachable!()
    }
    commands.splice(
        1..1,
        [
            at(
                0,
                TeletextColumnOperation::Foreground {
                    colour: TeletextColourReference::new(1).unwrap(),
                },
            ),
            at(
                0,
                TeletextColumnOperation::Background {
                    colour: TeletextColourReference::new(4).unwrap(),
                },
            ),
            at(
                0,
                TeletextColumnOperation::Flash {
                    mode: if case == 4 {
                        TeletextFlashMode::Normal
                    } else {
                        TeletextFlashMode::Steady
                    },
                    rate: TeletextFlashRate::Slow1Hz,
                },
            ),
        ],
    );
    objects[1].program = program(commands);
    rebuild(
        &seed,
        seed.page_enhancements().values().cloned().collect(),
        vec![],
        objects,
    )
}

#[test]
fn polish_underline_preserves_every_painted_attribute_and_temporal_phase() {
    for case in 0..5 {
        let input = declared(styled_seed(case));
        let encoded = encode(&input);
        assert_eq!(encoded.declared(), &input);
        let original = display(&encoded).normalized().original();
        let attrs = original
            .display_resolution()
            .cell(1, 7)
            .unwrap()
            .attributes();
        assert_eq!(
            attrs.foreground().value(),
            Some(TeletextColourReference::new(1).unwrap())
        );
        assert_eq!(
            attrs.background().value(),
            Some(TeletextColourReference::new(4).unwrap())
        );
        assert_eq!(attrs.boxed().value(), Some(case == 1));
        assert_eq!(attrs.conceal().value(), Some(case == 2));
        assert_eq!(attrs.invert().value(), Some(case == 3));
        assert_eq!(
            attrs.flash().value().unwrap().mode(),
            if case == 4 {
                TeletextFlashMode::Normal
            } else {
                TeletextFlashMode::Steady
            }
        );
        assert_response(display(&encoded), 1, true);
        assert_original(&input, display(&encoded));
        assert_pixels(display(&encoded));
        assert_ptu(&encoded);
    }
}

#[test]
fn polish_underline_mixed_global_bank_preserves_occupied_authored_normal_resource() {
    let mixed = super::super::polish_carrier::mixed_polish_base(false);
    let occupied = occupied_seed(false);
    let mut pages = mixed
        .page_enhancements()
        .values()
        .cloned()
        .collect::<Vec<_>>();
    pages[0].resources.drcs = occupied.page_enhancements()[&owner()].resources.drcs;
    pages[0].compositional_links = occupied.page_enhancements()[&owner()].compositional_links;
    let input = declared(rebuild(
        &mixed,
        pages,
        occupied.drcs().subtables().to_vec(),
        mixed.objects().definitions().to_vec(),
    ));
    let encoded = encode(&input);
    assert_eq!(encoded.declared(), &input);
    let generated = &encoded.compiler_drcs_resources()[0];
    assert_eq!(
        generated.table().identity().kind(),
        TeletextResourceKind::Gdrcs
    );
    assert_eq!(generated.table().glyphs().len(), 2);
    assert_response(display(&encoded), 1, false);
    assert_response(display(&encoded), 5, true);
    assert_original(&input, display(&encoded));
    assert_pixels(display(&encoded));
    assert_ptu(&encoded);
    for table in input.semantic().drcs().subtables() {
        let tx = authored::transmissions(&input)
            .into_iter()
            .find(|tx| tx.resource == table.identity())
            .unwrap();
        let expected = crate::teletext::encode_teletext_drcs_page(
            table,
            tx.counter,
            tx.update,
            crate::teletext::TeletextResourceTransmissionMode::Serial,
        )
        .unwrap();
        let actual = encoded
            .pages()
            .iter()
            .find(|page| {
                page.resource_header()
                    .is_some_and(|header| header.identity() == table.identity())
            })
            .unwrap();
        assert_eq!(actual.data_units(), expected.data_units());
    }
}

#[test]
fn polish_underline_mixed_demands_keep_exact_shared_work_and_output_boundaries() {
    use crate::teletext::encode_teletext_authored_service_with_work_usage;
    let input = declared(super::super::polish_carrier::mixed_polish_base(false));
    let mut budget = service_budget();
    budget.max_assembly_work = Some(usize::MAX);
    let (result, usage) =
        encode_teletext_authored_service_with_work_usage(input.clone(), &[], &budget, None);
    let successful = result.unwrap();
    let admitted = usage.assembly_work.unwrap();
    budget.max_assembly_work = Some(admitted);
    let (result, exact_usage) =
        encode_teletext_authored_service_with_work_usage(input.clone(), &[], &budget, None);
    assert_eq!(
        compiled_units(&result.unwrap()),
        compiled_units(&successful)
    );
    assert_eq!(exact_usage.assembly_work, Some(admitted));
    budget.max_assembly_work = Some(admitted - 1);
    let (denied, denied_usage) =
        encode_teletext_authored_service_with_work_usage(input.clone(), &[], &budget, None);
    assert!(denied.is_err());
    assert!(denied_usage.assembly_work.unwrap() < admitted);
    budget = service_budget();
    budget.max_pages = successful.pages().len();
    budget.max_units = successful
        .pages()
        .iter()
        .map(|page| page.data_units().len())
        .sum();
    assert!(encode_teletext_authored_service(input.clone(), &[], &budget).is_ok());
    budget.max_units -= 1;
    assert!(encode_teletext_authored_service(input.clone(), &[], &budget).is_err());
    assert_eq!(successful.declared(), &input);
}
