//! Exact per-scope execution and existing shared caller-budget boundaries.

use super::super::super::variants::expected_mask;
use super::*;

fn assert_links(
    input: &TeletextDeclaredService,
    resource: &crate::teletext::TeletextCompilerDrcsResource,
    owner: TeletextPageAddress,
    scope: TeletextDrcsTableScope,
) {
    let actual = resource.compositional_links(owner).unwrap();
    let role = match scope {
        TeletextDrcsTableScope::Normal => TeletextX27LinkRole::Drcs,
        TeletextDrcsTableScope::Global => TeletextX27LinkRole::Gdrcs,
    };
    assert_eq!(
        actual.link(role).target(),
        TeletextX27LinkTarget::Page {
            page: resource.table().identity().page(),
        }
    );
    assert_eq!(actual.link(role).required_subpages().get(), 1);
    if scope == TeletextDrcsTableScope::Global {
        let original = &input.semantic().page_enhancements()[&owner];
        assert_eq!(
            original.resources.drcs.page(TeletextDrcsTableScope::Normal),
            Some(authored_bank().identity().page())
        );
        for role in [
            TeletextX27LinkRole::Pop,
            TeletextX27LinkRole::Gpop,
            TeletextX27LinkRole::Drcs,
        ] {
            assert_eq!(
                actual.link(role),
                original.compositional_links.unwrap().link(role)
            );
        }
    }
}

fn assert_raster(display: &crate::teletext::EncodedTeletextCompiledDisplayPage) {
    let budget = crate::teletext::TeletextRasterBudget {
        max_pixels: 120_000,
        max_diagnostics: 4096,
    };
    let options = TeletextColourPixelOptions {
        reveal_concealed: false,
        flash_phase: TeletextFlashPhase::Original,
    };
    let original = crate::teletext::render_teletext_composition(
        display.normalized().original(),
        options,
        budget,
    )
    .unwrap();
    let actual = crate::teletext::render_teletext_physical_display(
        display.normalized().physical().replay(),
        options,
        budget,
    )
    .unwrap();
    assert!(original.diagnostics().is_empty());
    assert!(actual.diagnostics().is_empty());
    assert_eq!(original.width(), actual.width());
    assert_eq!(original.height(), actual.height());
    assert_eq!(original.pixels(), actual.pixels());
}

fn assert_binding(
    display: &crate::teletext::EncodedTeletextCompiledDisplayPage,
    resource: &crate::teletext::TeletextCompilerDrcsResource,
    scope: TeletextDrcsTableScope,
    slot: u8,
) {
    let original = display.normalized().original();
    let actual = display.normalized().physical().replay();
    for column in [7, 8] {
        let first = original.display_resolution().cell(1, column).unwrap();
        let second = actual.display_resolution().cell(1, column).unwrap();
        assert_eq!(first.fragment(), second.fragment());
        assert_eq!(
            first.attributes().underline_or_separated_mosaics().value(),
            Some(slot % 2 == 1)
        );
        assert_eq!(
            second.attributes().underline_or_separated_mosaics().value(),
            Some(false)
        );
        assert_eq!(second.attributes().spacing_separated().value(), Some(false));
        let expected = crate::teletext::project_teletext_comparison_glyph(original, first).unwrap();
        assert_eq!(expected.mask.unwrap().rows(), expected_mask(slot));
        let index = selected(actual.display_resolution(), 1, column);
        let executed = actual
            .drcs_execution()
            .records()
            .iter()
            .find(|record| record.candidate_index() == index)
            .unwrap();
        assert_eq!(executed.reference().table, scope);
        assert_eq!(executed.reference().subtable.get(), 0);
        assert_eq!(executed.reference().slot.get(), slot);
        assert_eq!(executed.resource(), resource.table().identity());
        assert_eq!(executed.bitmap().get(), expected_mask(slot));
        assert_eq!(
            actual.glyph_intent().records()[index].glyph(),
            Some(TeletextGlyphIdentity::Drcs {
                reference: executed.reference(),
            })
        );
    }
}

pub(super) fn assert_displays(
    input: &TeletextDeclaredService,
    encoded: &EncodedTeletextAuthoredService,
) {
    let budget = service_budget().normalization;
    let allocations = crate::teletext::allocate_teletext_resource_objects(
        input.semantic().objects(),
        &[],
        budget.max_resource_objects,
    )
    .unwrap();
    let mut owners = Vec::new();
    for page in encoded.pages() {
        let EncodedTeletextAuthoredPage::CompiledDisplay(display) = page else {
            continue;
        };
        let owner = display.page().address();
        let number = owner.page.get();
        let slot = u8::try_from((number - 200) % 4).unwrap();
        let (scope, kind) = if number < 204 {
            (TeletextDrcsTableScope::Normal, TeletextResourceKind::Drcs)
        } else {
            (TeletextDrcsTableScope::Global, TeletextResourceKind::Gdrcs)
        };
        let resource = encoded
            .compiler_drcs_resources()
            .iter()
            .find(|resource| resource.table().identity().kind() == kind)
            .unwrap();
        assert!(resource.consumers().contains(&owner));
        assert_eq!(resource.table().identity().logical_subpage().get(), 0);
        let composed = crate::teletext::compile_teletext_composed_main_program(
            input.semantic(),
            owner,
            &allocations,
            budget.composition,
            budget.packing,
        )
        .unwrap();
        assert_eq!(display.normalized().original(), composed.composition());
        assert_links(input, resource, owner, scope);
        assert_binding(display, resource, scope, slot);
        assert_raster(display);
        owners.push(number);
    }
    owners.sort_unstable();
    assert_eq!(owners, (200..208).collect::<Vec<_>>());
    assert_generated_ptus(encoded);
}

fn assert_generated_ptus(encoded: &EncodedTeletextAuthoredService) {
    for resource in encoded.compiler_drcs_resources() {
        let page = encoded
            .pages()
            .iter()
            .find(|page| {
                page.resource_header()
                    .is_some_and(|header| header.identity() == resource.table().identity())
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
            let cells = projection
                .records
                .iter()
                .find_map(|record| match record {
                    TeletextEncodedDisplayRecord::BaseRow { row, cells }
                        if *row == slot / 2 + 1 =>
                    {
                        Some(cells)
                    }
                    _ => None,
                })
                .expect("each generated slot must have its transmitted PTU row");
            let offset = usize::from(slot % 2) * 20;
            let actual = std::array::from_fn(|row| {
                (u16::from(cells[offset + row * 2] & 0x3f) << 6)
                    | u16::from(cells[offset + row * 2 + 1] & 0x3f)
            });
            assert_eq!(actual, expected_mask(slot));
        }
    }
}

fn units(service: &EncodedTeletextAuthoredService) -> Vec<Vec<[u8; 46]>> {
    service
        .pages()
        .iter()
        .map(|page| page.data_units().to_vec())
        .collect()
}

fn assert_output_bounds(
    input: &TeletextDeclaredService,
    successful: &EncodedTeletextAuthoredService,
) {
    let original = input.clone();
    let tx = authored::transmissions(input);
    let mut budget = service_budget();
    budget.max_pages = successful.pages().len();
    budget.max_units = successful.unit_count();
    let exact = encode_teletext_authored_service(input.clone(), &tx, &budget).unwrap();
    assert_eq!(exact.declared(), input);
    assert_eq!(units(&exact), units(successful));
    for pages in [true, false] {
        let mut denied = budget;
        if pages {
            denied.max_pages -= 1;
        } else {
            denied.max_units -= 1;
        }
        let error = encode_teletext_authored_service(input.clone(), &tx, &denied).unwrap_err();
        let TeletextAuthoredServiceEncodingFailure::Budget {
            category,
            count,
            maximum,
        } = error.reason
        else {
            panic!("expected caller output budget rejection: {error:?}");
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
        assert_eq!(input, &original);
    }
}

#[test]
fn both_scope_national_variants_preserve_shared_work_and_output_boundaries() {
    let input = both_scopes();
    let original = input.clone();
    let tx = authored::transmissions(&input);
    let mut budget = service_budget();
    budget.max_assembly_work = Some(usize::MAX);
    let (result, usage) = crate::teletext::encode_teletext_authored_service_with_work_usage(
        input.clone(),
        &tx,
        &budget,
        None,
    );
    let successful = result.unwrap();
    let admitted = usage.assembly_work.unwrap();
    assert!(admitted > 0);
    assert_eq!(successful.declared(), &input);
    budget.max_assembly_work = Some(admitted);
    let (result, exact_usage) = crate::teletext::encode_teletext_authored_service_with_work_usage(
        input.clone(),
        &tx,
        &budget,
        None,
    );
    let exact = result.unwrap();
    assert_eq!(exact.declared(), &input);
    assert_eq!(units(&exact), units(&successful));
    assert_eq!(exact_usage.assembly_work, Some(admitted));
    budget.max_assembly_work = Some(admitted - 1);
    let (denied, denied_usage) = crate::teletext::encode_teletext_authored_service_with_work_usage(
        input.clone(),
        &tx,
        &budget,
        None,
    );
    assert!(denied.is_err());
    assert!(denied_usage.assembly_work.unwrap() < admitted);
    assert_eq!(input, original);
    assert_output_bounds(&input, &successful);
    assert_eq!(input, original);
}
