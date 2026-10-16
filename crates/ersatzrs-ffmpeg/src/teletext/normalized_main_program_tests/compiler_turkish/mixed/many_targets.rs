//! Three occupied Normal targets require twelve established glyph variants.

use super::{authored, consumers, program, rows, selected, service_budget};
use crate::teletext::{
    allocate_teletext_resource_objects, compile_teletext_composed_main_program,
    encode_teletext_authored_service, encode_teletext_drcs_page, project_teletext_comparison_glyph,
    EncodedTeletextAuthoredPage, EncodedTeletextAuthoredService, TeletextEncodedDisplayRecord,
    TeletextResourceTransmissionMode,
};
use ersatzrs_domain::teletext::{
    TeletextDeclaredService, TeletextDisplayMotSelection, TeletextDisplaySize, TeletextDrcsBitmap,
    TeletextDrcsGlyphDefinition, TeletextDrcsGlyphSlot, TeletextDrcsPageLinks,
    TeletextDrcsSubtable, TeletextDrcsTableScope, TeletextEnhancementCommand,
    TeletextEnhancementProgram, TeletextG0Repertoire, TeletextGlyphDisposition,
    TeletextGlyphIdentity, TeletextGlyphPriority, TeletextNationalSubset, TeletextObjectIdentity,
    TeletextPageAddress, TeletextPageNumber, TeletextPhysicalBaseOverlay,
    TeletextRawDataPageNumber, TeletextResolvedPageEnhancementDraft, TeletextResolvedServiceDraft,
    TeletextResourceIdentity, TeletextResourceKind, TeletextResourceSubpageIndex,
    TeletextSemanticService, TeletextSubpageRequirementMask, TeletextX27Level25Links,
    TeletextX27LinkDeclaration, TeletextX27LinkRole, TeletextX27LinkTarget,
    TeletextX27LinkValidity, TeletextX27Magazine,
};
use std::collections::BTreeMap;

fn normal_target(group: u8) -> TeletextRawDataPageNumber {
    TeletextRawDataPageNumber::new(2, [0xa0, 0xa2, 0xa3][usize::from(group)]).unwrap()
}

fn global_target() -> TeletextRawDataPageNumber {
    TeletextRawDataPageNumber::new(1, 0xa1).unwrap()
}

fn bank(kind: TeletextResourceKind, page: TeletextRawDataPageNumber) -> TeletextDrcsSubtable {
    TeletextDrcsSubtable::new(
        TeletextResourceIdentity::new(kind, page, TeletextResourceSubpageIndex::new(0).unwrap())
            .unwrap(),
        vec![TeletextDrcsGlyphDefinition {
            slot: TeletextDrcsGlyphSlot::new(0).unwrap(),
            bitmap: TeletextDrcsBitmap::new([0; 10]).unwrap(),
        }],
    )
    .unwrap()
}

fn address(from: TeletextPageAddress, group: u8) -> TeletextPageAddress {
    TeletextPageAddress {
        page: TeletextPageNumber::new(from.page.get() + 4 * u16::from(group)).unwrap(),
        subcode: from.subcode,
    }
}

fn rebase(
    original: &TeletextEnhancementProgram,
    from: TeletextPageAddress,
    to: TeletextPageAddress,
) -> TeletextEnhancementProgram {
    let mut commands = original.commands().to_vec();
    for command in &mut commands {
        if let TeletextEnhancementCommand::InvokeObject { target, .. } = command {
            assert_eq!(target.owner(), Some(from));
            *target = TeletextObjectIdentity::local(to, target.object_id());
        }
    }
    program(commands)
}

fn metadata(
    original: &TeletextResolvedPageEnhancementDraft,
    group: u8,
) -> TeletextResolvedPageEnhancementDraft {
    let mut page = original.clone();
    page.address = address(original.address, group);
    page.local_program = original
        .local_program
        .as_ref()
        .map(|commands| rebase(commands, original.address, page.address));
    page.resources.drcs =
        TeletextDrcsPageLinks::new(Some(normal_target(group)), Some(global_target())).unwrap();
    let unused = TeletextX27LinkDeclaration::new(
        TeletextX27LinkTarget::Unspecified {
            magazine: TeletextX27Magazine::new(2).unwrap(),
        },
        TeletextX27LinkValidity::Level25Only,
        TeletextSubpageRequirementMask::new(0xffff),
    )
    .unwrap();
    let linked = |page| {
        TeletextX27LinkDeclaration::new(
            TeletextX27LinkTarget::Page { page },
            TeletextX27LinkValidity::Level25Only,
            TeletextSubpageRequirementMask::new(1),
        )
        .unwrap()
    };
    page.compositional_links = Some(
        TeletextX27Level25Links::new(
            unused,
            unused,
            linked(global_target()),
            linked(normal_target(group)),
        )
        .unwrap(),
    );
    page
}

fn input() -> TeletextDeclaredService {
    let template = consumers(&[0, 1, 2, 3]);
    let seed = template.semantic();
    let mut base = seed.base().clone();
    base.pages = (0..3)
        .flat_map(|group| {
            seed.base().pages.iter().map(move |original| {
                let mut page = original.clone();
                page.address = address(original.address, group);
                page
            })
        })
        .collect();
    let pages = (0..3)
        .flat_map(|group| {
            seed.page_enhancements()
                .values()
                .map(move |page| metadata(page, group))
        })
        .collect();
    let objects = (0..3)
        .flat_map(|group| {
            seed.objects().definitions().iter().map(move |original| {
                let mut object = original.clone();
                let from = original.identity.owner().unwrap();
                let to = address(from, group);
                object.identity = TeletextObjectIdentity::local(to, original.identity.object_id());
                object.program = rebase(&original.program, from, to);
                object
            })
        })
        .collect();
    let overlays = (0..3)
        .flat_map(|group| {
            seed.physical_base_overlays().values().map(move |overlay| {
                TeletextPhysicalBaseOverlay::new(
                    address(overlay.owner(), group),
                    overlay.rows().to_vec(),
                )
                .unwrap()
            })
        })
        .collect();
    let mut drcs = (0..3)
        .map(|group| bank(TeletextResourceKind::Drcs, normal_target(group)))
        .collect::<Vec<_>>();
    drcs.push(bank(TeletextResourceKind::Gdrcs, global_target()));
    let semantic = TeletextSemanticService::new(
        TeletextResolvedServiceDraft {
            base,
            pages,
            objects,
            magazines: vec![],
            drcs,
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
    let mut limits = authored::declaration_limits();
    limits.max_displays = 12;
    TeletextDeclaredService::new(semantic, vec![], selections, vec![], limits).unwrap()
}

fn fallback_rows(
    service: &TeletextSemanticService,
    page: &ersatzrs_domain::teletext::TeletextPageDraft,
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

fn assert_links(service: &TeletextSemanticService, owner: TeletextPageAddress, group: u8) {
    let metadata = &service.page_enhancements()[&owner];
    for (scope, role, target) in [
        (
            TeletextDrcsTableScope::Normal,
            TeletextX27LinkRole::Drcs,
            normal_target(group),
        ),
        (
            TeletextDrcsTableScope::Global,
            TeletextX27LinkRole::Gdrcs,
            global_target(),
        ),
    ] {
        assert_eq!(metadata.resources.drcs.page(scope), Some(target));
        let link = metadata.compositional_links.unwrap().link(role);
        assert_eq!(link.target(), TeletextX27LinkTarget::Page { page: target });
        assert_eq!(link.validity(), TeletextX27LinkValidity::Level25Only);
        assert_eq!(link.required_subpages().get(), 1);
    }
}

fn assert_originals(
    input: &TeletextDeclaredService,
) -> BTreeMap<TeletextPageAddress, Vec<TeletextEncodedDisplayRecord>> {
    let service = input.semantic();
    assert_eq!(service.base().pages.len(), 12);
    assert_eq!(service.drcs().subtables().len(), 4);
    assert_eq!(input.resource_inventory().len(), 4);
    for table in service.drcs().subtables() {
        assert_eq!(table.identity().logical_subpage().get(), 0);
        assert_eq!(table.glyphs().len(), 1);
        assert_eq!(table.glyphs()[0].bitmap.get(), [0; 10]);
    }
    let budget = service_budget().normalization;
    let allocations =
        allocate_teletext_resource_objects(service.objects(), &[], budget.max_resource_objects)
            .unwrap();
    service.base().pages.iter().map(|page| {
        let variant = u8::try_from((page.address.page.get() - 200) % 4).unwrap();
        let group = u8::try_from((page.address.page.get() - 200) / 4).unwrap();
        assert_links(service, page.address, group);
        let composed = compile_teletext_composed_main_program(
            service, page.address, &allocations, budget.composition, budget.packing,
        ).unwrap();
        let original = composed.composition();
        let view = original.display_resolution();
        let origin = view.cell(1, 7).unwrap().origin().unwrap();
        assert_eq!(view.cell(1, 8).unwrap().origin(), Some(origin));
        assert_eq!(view.origins()[origin].effective_size(), Some(TeletextDisplaySize::DoubleWidth));
        let index = view.origins()[origin].candidate_index();
        assert_eq!(original.glyph_layers().candidates()[index].priority(), TeletextGlyphPriority::Base);
        let intent = original.glyph_intent().records()[index];
        assert_eq!(intent.disposition(), TeletextGlyphDisposition::Assigned);
        assert!(matches!(intent.glyph(), Some(TeletextGlyphIdentity::G0 {glyph})
            if glyph.repertoire() == TeletextG0Repertoire::Latin
                && glyph.national_subset() == Some(if variant < 2 {TeletextNationalSubset::Polish} else {TeletextNationalSubset::Turkish})
                && glyph.code() == if variant < 2 {0x5b} else {0x23}));
        assert_eq!(view.cell(1, 7).unwrap().attributes().underline_or_separated_mosaics().value(), Some(variant % 2 == 1));
        assert_eq!(project_teletext_comparison_glyph(original, view.cell(1, 7).unwrap()).unwrap().mask.unwrap().rows(), super::super::variants::expected_mask(variant));
        assert!(original.drcs_execution().records().is_empty());
        let fallback = fallback_rows(service, page);
        assert!(fallback.iter().any(|record| matches!(record,
            TeletextEncodedDisplayRecord::BaseRow {row:1,cells}
                if cells[7] == if variant < 2 {0x5b} else {0x23})));
        (page.address, fallback)
    }).collect()
}

fn assert_output(
    input: &TeletextDeclaredService,
    encoded: &EncodedTeletextAuthoredService,
    fallback: &BTreeMap<TeletextPageAddress, Vec<TeletextEncodedDisplayRecord>>,
) {
    assert_eq!(encoded.declared(), input);
    assert_eq!(encoded.compiler_drcs_resources().len(), 3);
    for group in 0..3 {
        let resource = encoded
            .compiler_drcs_resources()
            .iter()
            .find(|resource| resource.table().identity().page() == normal_target(group))
            .unwrap();
        let identity = resource.table().identity();
        assert_eq!(identity.kind(), TeletextResourceKind::Drcs);
        assert_eq!(identity.logical_subpage().get(), 1);
        assert_eq!(
            resource
                .table()
                .glyphs()
                .iter()
                .map(|glyph| glyph.slot.get())
                .collect::<Vec<_>>(),
            vec![0, 1, 2, 3]
        );
        assert_eq!(
            resource
                .consumers()
                .iter()
                .map(|owner| owner.page.get())
                .collect::<Vec<_>>(),
            (200 + u16::from(group) * 4..204 + u16::from(group) * 4).collect::<Vec<_>>()
        );
        for glyph in resource.table().glyphs() {
            assert_eq!(
                glyph.bitmap.get(),
                super::super::variants::expected_mask(glyph.slot.get())
            );
        }
    }
    for page in encoded.pages() {
        if let EncodedTeletextAuthoredPage::CompiledDisplay(display) = page {
            let owner = display.page().address();
            let variant = u8::try_from((owner.page.get() - 200) % 4).unwrap();
            let group = u8::try_from((owner.page.get() - 200) / 4).unwrap();
            assert_eq!(rows(display.page()), fallback[&owner]);
            let physical = display.normalized().physical().replay();
            let index = selected(physical.display_resolution(), 1, 7);
            let execution = physical
                .drcs_execution()
                .records()
                .iter()
                .find(|record| record.candidate_index() == index)
                .unwrap();
            assert_eq!(execution.reference().table, TeletextDrcsTableScope::Normal);
            assert_eq!(execution.reference().subtable.get(), 1);
            assert_eq!(execution.reference().slot.get(), variant);
            assert_eq!(execution.resource().page(), normal_target(group));
            assert_eq!(
                execution.bitmap().get(),
                super::super::variants::expected_mask(variant)
            );
        }
    }
    for transmission in authored::transmissions(input) {
        let table = input
            .semantic()
            .drcs()
            .subtables()
            .iter()
            .find(|table| table.identity() == transmission.resource)
            .unwrap();
        let expected = encode_teletext_drcs_page(
            table,
            transmission.counter,
            transmission.update,
            TeletextResourceTransmissionMode::Serial,
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
fn three_distinct_linked_targets_preserve_all_twelve_national_variants() {
    let input = input();
    let original = input.clone();
    let fallback = assert_originals(&input);
    let encoded = encode_teletext_authored_service(
        input.clone(),
        &authored::transmissions(&input),
        &service_budget(),
    );
    assert_eq!(input, original);
    let encoded = encoded.expect("three admitted linked targets require twelve established glyphs");
    assert_output(&input, &encoded, &fallback);
}

#[test]
fn three_linked_targets_keep_exact_complete_page_and_unit_boundaries() {
    let input = input();
    let original = input.clone();
    let fallback = assert_originals(&input);
    let tx = authored::transmissions(&input);
    let successful =
        encode_teletext_authored_service(input.clone(), &tx, &service_budget()).unwrap();
    assert_output(&input, &successful, &fallback);
    let mut budget = service_budget();
    budget.max_pages = successful.pages().len();
    budget.max_units = successful.unit_count();
    let exact = encode_teletext_authored_service(input.clone(), &tx, &budget).unwrap();
    assert_output(&input, &exact, &fallback);
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
        assert_eq!(input, original);
    }
    assert_eq!(input, original);
}
