//! Eight exact national/effect slots share a stable compiler-owned bank.

use super::*;
use std::collections::BTreeMap;

fn code(slot: u8) -> u8 {
    match slot {
        0 | 1 => 0x5b,
        2 | 3 => 0x23,
        4 | 5 => 0x5d,
        6 | 7 => 0x7d,
        _ => panic!("only the eight established variants are expected"),
    }
}

fn primary(slot: u8) -> u8 {
    match slot {
        0 | 1 => 0x08,
        2 | 3 => 0x16,
        4..=7 => 0x1f,
        _ => panic!("only the eight established variants are expected"),
    }
}

fn mask(slot: u8) -> [u16; 10] {
    let mut result = match slot {
        0 | 1 => [0, 0x3fc, 0x18, 0x30, 0x1f8, 0xc0, 0x180, 0x3fc, 0, 0],
        2 | 3 => [0, 0, 0x3f0, 0xc0, 0xd8, 0xd8, 0xd8, 0xde, 0, 0],
        4 | 5 => CAPITAL,
        6 | 7 => SMALL,
        _ => panic!("only the eight fixed national masks are expected"),
    };
    if slot % 2 == 1 {
        result[9] = 0xfff;
    }
    result
}

fn reowner(
    original: &TeletextEnhancementProgram,
    to: TeletextPageAddress,
) -> TeletextEnhancementProgram {
    let mut commands = original.commands().to_vec();
    for command in &mut commands {
        if let TeletextEnhancementCommand::InvokeObject { target, .. } = command {
            assert_eq!(target.owner(), Some(owner()));
            *target = TeletextObjectIdentity::local(to, target.object_id());
        }
    }
    program(commands)
}

fn input(slots: &[u8]) -> TeletextDeclaredService {
    // The shared seed supplies raw rows and repaint geometry only. Each final
    // owner receives its exact original primary before semantic validation.
    let seeds = slots
        .iter()
        .map(|&slot| {
            let to = TeletextPageAddress {
                page: TeletextPageNumber::new(200 + u16::from(slot)).unwrap(),
                subcode: owner().subcode,
            };
            (slot, to, romanian_base(code(slot), slot % 2 == 1))
        })
        .collect::<Vec<_>>();
    let mut base = seeds[0].2.base().clone();
    base.initial_page = seeds[0].1.page;
    base.pages = seeds
        .iter()
        .map(|(_, to, seed)| {
            let mut page = seed.base().pages[0].clone();
            page.address = *to;
            page
        })
        .collect();
    let pages = seeds
        .iter()
        .map(|(slot, to, seed)| {
            let mut page = seed.page_enhancements()[&owner()].clone();
            page.address = *to;
            page.presentation.as_mut().unwrap().primary =
                TeletextPageCharsetDesignation::new(primary(*slot)).unwrap();
            page.local_program = page
                .local_program
                .as_ref()
                .map(|commands| reowner(commands, *to));
            page
        })
        .collect();
    let objects = seeds
        .iter()
        .flat_map(|(_, to, seed)| {
            seed.objects().definitions().iter().map(move |original| {
                let mut object = original.clone();
                object.identity = TeletextObjectIdentity::local(*to, original.identity.object_id());
                object.program = reowner(&original.program, *to);
                object
            })
        })
        .collect();
    let overlays = seeds
        .iter()
        .map(|(_, to, seed)| {
            TeletextPhysicalBaseOverlay::new(
                *to,
                seed.physical_base_overlays()[&owner()].rows().to_vec(),
            )
            .unwrap()
        })
        .collect();
    let semantic = TeletextSemanticService::new(
        TeletextResolvedServiceDraft {
            base,
            pages,
            objects,
            magazines: vec![],
            drcs: vec![],
        },
        authored::semantic_limits(),
    )
    .unwrap()
    .with_physical_base_overlays(overlays, authored::semantic_limits())
    .unwrap();
    let selections = semantic
        .base()
        .pages
        .iter()
        .map(|page| TeletextDisplayMotSelection {
            display: page.address,
            mot: None,
        })
        .collect();
    TeletextDeclaredService::new(
        semantic,
        vec![],
        selections,
        vec![],
        authored::declaration_limits(),
    )
    .unwrap()
}

fn fallback(
    service: &TeletextSemanticService,
    page: &TeletextPageDraft,
) -> Vec<TeletextEncodedDisplayRecord> {
    let layout = crate::teletext::layout::compile_for_service_composition(service, page).unwrap();
    let metadata = &service.page_enhancements()[&page.address];
    let encoded = crate::teletext::packets::encode_compiled_page(
        page,
        &layout,
        &crate::teletext::packets::PagePacketMetadata {
            presentation: metadata.presentation.as_ref(),
            compositional_links: metadata.compositional_links.as_ref(),
            enhancements: crate::teletext::packets::PageEnhancements::Emit,
        },
    )
    .unwrap();
    rows(&encoded)
}

fn assert_original(original: &TeletextMainComposition, slot: u8) {
    let subset = match slot {
        0 | 1 => TeletextNationalSubset::Polish,
        2 | 3 => TeletextNationalSubset::Turkish,
        _ => TeletextNationalSubset::Romanian,
    };
    assert_eq!(original.base_display().primary().get(), primary(slot));
    let view = original.display_resolution();
    let origin = view.cell(1, 7).unwrap().origin().unwrap();
    assert_eq!(view.cell(1, 8).unwrap().origin(), Some(origin));
    assert_eq!(
        view.origins()[origin].effective_size(),
        Some(TeletextDisplaySize::DoubleWidth)
    );
    let index = view.origins()[origin].candidate_index();
    assert_eq!(
        original.glyph_layers().candidates()[index].priority(),
        TeletextGlyphPriority::Base
    );
    let intent = original.glyph_intent().records()[index];
    assert_eq!(intent.disposition(), TeletextGlyphDisposition::Assigned);
    assert!(
        matches!(intent.glyph(),Some(TeletextGlyphIdentity::G0 {glyph})
        if glyph.repertoire() == TeletextG0Repertoire::Latin && glyph.code() == code(slot) && glyph.national_subset() == Some(subset))
    );
    assert!(original.drcs_execution().records().is_empty());
    assert_eq!(original.base_replacement(1, 7), None);
    for column in [7, 8] {
        let cell = view.cell(1, column).unwrap();
        assert_eq!(
            cell.attributes().underline_or_separated_mosaics().value(),
            Some(slot % 2 == 1)
        );
        assert_eq!(cell.attributes().spacing_separated().value(), Some(false));
        assert_eq!(
            crate::teletext::project_teletext_comparison_glyph(original, cell)
                .unwrap()
                .mask
                .unwrap()
                .rows(),
            mask(slot)
        );
    }
}

fn originals(
    input: &TeletextDeclaredService,
) -> BTreeMap<TeletextPageAddress, (TeletextMainComposition, Vec<TeletextEncodedDisplayRecord>)> {
    assert!(input.resource_inventory().is_empty());
    let service = input.semantic();
    let budget = service_budget().normalization;
    let allocations = crate::teletext::allocate_teletext_resource_objects(
        service.objects(),
        &[],
        budget.max_resource_objects,
    )
    .unwrap();
    service
        .base()
        .pages
        .iter()
        .map(|page| {
            let slot = u8::try_from(page.address.page.get() - 200).unwrap();
            let composed = crate::teletext::compile_teletext_composed_main_program(
                service,
                page.address,
                &allocations,
                budget.composition,
                budget.packing,
            )
            .unwrap();
            assert_original(composed.composition(), slot);
            let fallback = fallback(service, page);
            assert!(fallback.iter().any(|record| matches!(record,
                TeletextEncodedDisplayRecord::BaseRow {row:1,cells}
                    if cells[7] == code(slot))));
            (page.address, (composed.composition().clone(), fallback))
        })
        .collect()
}

fn assert_ptus(
    encoded: &EncodedTeletextAuthoredService,
    identity: TeletextResourceIdentity,
    slots: &[u8],
) {
    let page = encoded
        .pages()
        .iter()
        .find(|page| {
            page.resource_header()
                .is_some_and(|header| header.identity() == identity)
        })
        .unwrap();
    let projection = crate::teletext::project_encoded_teletext_drcs(page, 32).unwrap();
    for &slot in slots {
        let cells = projection
            .records
            .iter()
            .find_map(|record| match record {
                TeletextEncodedDisplayRecord::BaseRow { row, cells } if *row == slot / 2 + 1 => {
                    Some(cells)
                }
                _ => None,
            })
            .unwrap();
        let offset = usize::from(slot % 2) * 20;
        let actual = std::array::from_fn(|row| {
            (u16::from(cells[offset + row * 2] & 63) << 6)
                | u16::from(cells[offset + row * 2 + 1] & 63)
        });
        assert_eq!(actual, mask(slot));
    }
}

fn assert_output(
    encoded: &EncodedTeletextAuthoredService,
    original: &BTreeMap<
        TeletextPageAddress,
        (TeletextMainComposition, Vec<TeletextEncodedDisplayRecord>),
    >,
) {
    for page in encoded.pages() {
        let EncodedTeletextAuthoredPage::CompiledDisplay(display) = page else {
            continue;
        };
        let address = display.page().address();
        let slot = u8::try_from(address.page.get() - 200).unwrap();
        assert_eq!(display.normalized().original(), &original[&address].0);
        assert_eq!(rows(display.page()), original[&address].1);
        let physical = display.normalized().physical().replay();
        let resource = encoded.compiler_drcs_resources()[0].table().identity();
        for column in [7, 8] {
            let cell = physical.display_resolution().cell(1, column).unwrap();
            let index =
                physical.display_resolution().origins()[cell.origin().unwrap()].candidate_index();
            let executed = physical
                .drcs_execution()
                .records()
                .iter()
                .find(|record| record.candidate_index() == index)
                .unwrap();
            assert_eq!(executed.reference().table, TeletextDrcsTableScope::Normal);
            assert_eq!(executed.reference().subtable.get(), 0);
            assert_eq!(executed.reference().slot.get(), slot);
            assert_eq!(executed.resource(), resource);
            assert_eq!(executed.bitmap().get(), mask(slot));
            assert_eq!(
                cell.attributes().underline_or_separated_mosaics().value(),
                Some(false)
            );
            assert_eq!(cell.attributes().spacing_separated().value(), Some(false));
        }
        assert_raster(display);
    }
}

#[test]
fn eight_national_slots_share_one_bank_across_order_and_consumer_removal() {
    let mut identity = None;
    for slots in [
        vec![0, 1, 2, 3, 4, 5, 6, 7],
        vec![7, 6, 5, 4, 3, 2, 1, 0],
        vec![0, 2, 4, 6],
        vec![1, 3, 5, 7],
        vec![7],
    ] {
        let input = input(&slots);
        let before = input.clone();
        let original = originals(&input);
        let encoded = encode_teletext_authored_service(
            input.clone(),
            &authored::transmissions(&input),
            &service_budget(),
        )
        .unwrap();
        assert_eq!(input, before);
        assert_eq!(encoded.declared(), &input);
        assert_eq!(encoded.compiler_drcs_resources().len(), 1);
        let resource = &encoded.compiler_drcs_resources()[0];
        assert_eq!(
            *identity.get_or_insert(resource.table().identity()),
            resource.table().identity()
        );
        assert_eq!(
            resource.table().identity().kind(),
            TeletextResourceKind::Drcs
        );
        assert_eq!(resource.table().identity().logical_subpage().get(), 0);
        let mut expected = slots.clone();
        expected.sort_unstable();
        assert_eq!(
            resource
                .table()
                .glyphs()
                .iter()
                .map(|glyph| glyph.slot.get())
                .collect::<Vec<_>>(),
            expected
        );
        assert_eq!(
            resource
                .consumers()
                .iter()
                .map(|owner| u8::try_from(owner.page.get() - 200).unwrap())
                .collect::<Vec<_>>(),
            slots
        );
        for glyph in resource.table().glyphs() {
            assert_eq!(glyph.bitmap.get(), mask(glyph.slot.get()));
        }
        assert_output(&encoded, &original);
        assert_ptus(&encoded, resource.table().identity(), &slots);
    }
}

#[test]
fn eight_national_slots_keep_exact_complete_page_and_unit_boundaries() {
    let input = input(&[0, 1, 2, 3, 4, 5, 6, 7]);
    let before = input.clone();
    let original = originals(&input);
    let tx = authored::transmissions(&input);
    let successful =
        encode_teletext_authored_service(input.clone(), &tx, &service_budget()).unwrap();
    assert_output(&successful, &original);
    let mut budget = service_budget();
    budget.max_pages = successful.pages().len();
    budget.max_units = successful.unit_count();
    let exact = encode_teletext_authored_service(input.clone(), &tx, &budget).unwrap();
    assert_eq!(exact.declared(), &input);
    assert_eq!(
        exact
            .pages()
            .iter()
            .map(EncodedTeletextAuthoredPage::data_units)
            .collect::<Vec<_>>(),
        successful
            .pages()
            .iter()
            .map(EncodedTeletextAuthoredPage::data_units)
            .collect::<Vec<_>>()
    );
    for pages in [true, false] {
        let mut denied = budget;
        if pages {
            denied.max_pages -= 1;
        } else {
            denied.max_units -= 1;
        }
        let error = encode_teletext_authored_service(input.clone(), &tx, &denied).unwrap_err();
        let crate::teletext::TeletextAuthoredServiceEncodingFailure::Budget {
            category,
            count,
            maximum,
        } = error.reason
        else {
            panic!("expected complete caller output budget rejection: {error:?}");
        };
        assert_eq!(category, if pages { "pages" } else { "units" });
        assert_eq!(
            maximum + 1,
            if pages {
                budget.max_pages
            } else {
                budget.max_units
            }
        );
        assert!(count > maximum);
        assert_eq!(input, before);
    }
    assert_eq!(input, before);
}
