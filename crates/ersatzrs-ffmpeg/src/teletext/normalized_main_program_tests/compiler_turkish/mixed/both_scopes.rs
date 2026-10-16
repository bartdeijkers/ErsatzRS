//! Four established variants are required in each of two separate DRCS scopes.

use super::*;
use std::collections::BTreeMap;

fn global_owner(original: TeletextPageAddress) -> TeletextPageAddress {
    TeletextPageAddress {
        page: TeletextPageNumber::new(original.page.get() + 4).unwrap(),
        subcode: original.subcode,
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

fn authored_bank() -> TeletextDrcsSubtable {
    TeletextDrcsSubtable::new(
        TeletextResourceIdentity::new(
            TeletextResourceKind::Drcs,
            TeletextRawDataPageNumber::new(2, 0xa2).unwrap(),
            TeletextResourceSubpageIndex::new(3).unwrap(),
        )
        .unwrap(),
        vec![TeletextDrcsGlyphDefinition {
            slot: TeletextDrcsGlyphSlot::new(5).unwrap(),
            bitmap: TeletextDrcsBitmap::new([0; 10]).unwrap(),
        }],
    )
    .unwrap()
}

fn global_pages(seed: &TeletextSemanticService) -> Vec<TeletextResolvedPageEnhancementDraft> {
    let resource = authored_bank().identity();
    let unused = TeletextX27LinkDeclaration::new(
        TeletextX27LinkTarget::Unspecified {
            magazine: TeletextX27Magazine::new(2).unwrap(),
        },
        TeletextX27LinkValidity::Level25Only,
        TeletextSubpageRequirementMask::new(0xffff),
    )
    .unwrap();
    let normal = TeletextX27LinkDeclaration::new(
        TeletextX27LinkTarget::Page {
            page: resource.page(),
        },
        TeletextX27LinkValidity::Level25Only,
        TeletextSubpageRequirementMask::new(1 << 3),
    )
    .unwrap();
    seed.page_enhancements()
        .values()
        .map(|original| {
            let mut page = original.clone();
            page.address = global_owner(original.address);
            page.local_program = original
                .local_program
                .as_ref()
                .map(|program| rebase(program, original.address, page.address));
            page.resources.drcs = TeletextDrcsPageLinks::new(Some(resource.page()), None).unwrap();
            page.compositional_links =
                Some(TeletextX27Level25Links::new(unused, unused, unused, normal).unwrap());
            page
        })
        .collect()
}

fn both_scopes() -> TeletextDeclaredService {
    let normal = consumers(&[0, 1, 2, 3]);
    let seed = normal.semantic();
    let mut base = seed.base().clone();
    base.pages.extend(seed.base().pages.iter().map(|original| {
        let mut page = original.clone();
        page.address = global_owner(original.address);
        page
    }));
    let mut pages = seed
        .page_enhancements()
        .values()
        .cloned()
        .collect::<Vec<_>>();
    pages.extend(global_pages(seed));
    let mut objects = seed.objects().definitions().to_vec();
    objects.extend(seed.objects().definitions().iter().map(|original| {
        let mut object = original.clone();
        let from = original.identity.owner().unwrap();
        let to = global_owner(from);
        object.identity = TeletextObjectIdentity::local(to, original.identity.object_id());
        object.program = rebase(&original.program, from, to);
        object
    }));
    let mut overlays = seed
        .physical_base_overlays()
        .values()
        .cloned()
        .collect::<Vec<_>>();
    overlays.extend(seed.physical_base_overlays().values().map(|overlay| {
        TeletextPhysicalBaseOverlay::new(global_owner(overlay.owner()), overlay.rows().to_vec())
            .unwrap()
    }));
    declared(
        TeletextSemanticService::new(
            TeletextResolvedServiceDraft {
                base,
                pages,
                magazines: vec![],
                objects,
                drcs: vec![authored_bank()],
            },
            authored::semantic_limits(),
        )
        .unwrap()
        .with_physical_base_overlays(overlays, authored::semantic_limits())
        .unwrap(),
    )
}

fn fallback_rows(
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

fn assert_originals(
    input: &TeletextDeclaredService,
) -> BTreeMap<TeletextPageAddress, Vec<TeletextEncodedDisplayRecord>> {
    let service = input.semantic();
    assert_eq!(service.base().pages.len(), 8);
    assert_eq!(input.resource_inventory(), &[authored_bank().identity()]);
    assert_eq!(service.drcs().subtables(), &[authored_bank()]);
    for slot in 0..4 {
        assert_ne!(super::super::variants::expected_mask(slot), [0; 10]);
    }
    let budget = service_budget().normalization;
    let allocations = crate::teletext::allocate_teletext_resource_objects(
        service.objects(),
        &[],
        budget.max_resource_objects,
    )
    .unwrap();
    service.base().pages.iter().map(|page| {
        let number = page.address.page.get();
        let slot = u8::try_from((number-200)%4).unwrap();
        let metadata = &service.page_enhancements()[&page.address];
        assert_eq!(metadata.resources.drcs.page(TeletextDrcsTableScope::Global), None);
        assert_eq!(metadata.resources.drcs.page(TeletextDrcsTableScope::Normal), if number<204 {None} else {Some(authored_bank().identity().page())});
        if number>=204 {
            let link = metadata.compositional_links.unwrap().link(TeletextX27LinkRole::Drcs);
            assert_eq!(link.target(), TeletextX27LinkTarget::Page {page:authored_bank().identity().page()});
            assert_eq!(link.required_subpages().get(),1<<3);
        } else {
            assert!(metadata.compositional_links.is_none());
        }
        let composed = crate::teletext::compile_teletext_composed_main_program(
            service, page.address, &allocations, budget.composition, budget.packing,
        ).unwrap();
        let original = composed.composition();
        let display = original.display_resolution();
        let origin = display.cell(1,7).unwrap().origin().unwrap();
        assert_eq!(display.cell(1,8).unwrap().origin(),Some(origin));
        assert_eq!(display.origins()[origin].effective_size(),Some(TeletextDisplaySize::DoubleWidth));
        let index = display.origins()[origin].candidate_index();
        let candidate = original.glyph_layers().candidates()[index];
        assert_eq!(candidate.priority(),TeletextGlyphPriority::Base);
        assert_eq!(candidate.base().unwrap().code(),if slot<2 {0x5b} else {0x23});
        let intent = original.glyph_intent().records()[index];
        assert_eq!(intent.disposition(),TeletextGlyphDisposition::Assigned);
        assert!(matches!(intent.glyph(),Some(TeletextGlyphIdentity::G0 {glyph})
            if glyph.repertoire()==TeletextG0Repertoire::Latin
                && glyph.national_subset()==Some(if slot<2 {TeletextNationalSubset::Polish} else {TeletextNationalSubset::Turkish})
                && glyph.code()==if slot<2 {0x5b} else {0x23}));
        let cell = display.cell(1,7).unwrap();
        assert_eq!(cell.attributes().underline_or_separated_mosaics().value(),Some(slot%2==1));
        assert_eq!(cell.attributes().spacing_separated().value(),Some(false));
        assert_eq!(crate::teletext::project_teletext_comparison_glyph(original,cell).unwrap().mask.unwrap().rows(),
            super::super::variants::expected_mask(slot));
        let plan = crate::teletext::plan_teletext_base_compatibility_realization(original,budget.realization).unwrap();
        assert!(plan.origins().iter().any(|candidate|candidate.origin_index==origin));
        let fallback = fallback_rows(service,page);
        assert!(fallback.iter().any(|record| matches!(record,
            TeletextEncodedDisplayRecord::BaseRow {row:1,cells}
                if cells[7]==if slot<2 {0x5b} else {0x23})));
        (page.address,fallback)
    }).collect()
}

fn assert_output(
    input: &TeletextDeclaredService,
    encoded: &EncodedTeletextAuthoredService,
    fallback: &BTreeMap<TeletextPageAddress, Vec<TeletextEncodedDisplayRecord>>,
) {
    assert_eq!(encoded.declared(), input);
    assert_eq!(encoded.compiler_drcs_resources().len(), 2);
    for resource in encoded.compiler_drcs_resources() {
        assert_ne!(
            resource.table().identity().page(),
            authored_bank().identity().page()
        );
        assert_eq!(
            resource
                .table()
                .glyphs()
                .iter()
                .map(|glyph| glyph.slot.get())
                .collect::<Vec<_>>(),
            vec![0, 1, 2, 3]
        );
        for glyph in resource.table().glyphs() {
            assert_eq!(
                glyph.bitmap.get(),
                super::super::variants::expected_mask(glyph.slot.get())
            );
        }
        let first = match resource.table().identity().kind() {
            TeletextResourceKind::Drcs => 200,
            TeletextResourceKind::Gdrcs => 204,
            _ => panic!("only the two DRCS scope banks may be generated"),
        };
        assert_eq!(
            resource
                .consumers()
                .iter()
                .map(|address| address.page.get())
                .collect::<Vec<_>>(),
            (first..first + 4).collect::<Vec<_>>()
        );
    }
    for page in encoded.pages() {
        if let EncodedTeletextAuthoredPage::CompiledDisplay(display) = page {
            assert_eq!(rows(display.page()), fallback[&display.page().address()]);
        }
    }
    let tx = authored::transmissions(input);
    let expected = crate::teletext::encode_teletext_drcs_page(
        &authored_bank(),
        tx[0].counter,
        tx[0].update,
        crate::teletext::TeletextResourceTransmissionMode::Serial,
    )
    .unwrap();
    let actual = encoded
        .pages()
        .iter()
        .find(|page| {
            page.resource_header()
                .is_some_and(|header| header.identity() == authored_bank().identity())
        })
        .unwrap();
    assert_eq!(actual.data_units(), expected.data_units());
}

#[test]
fn all_four_national_variants_in_both_compiler_scopes_preserve_authored_service() {
    let input = both_scopes();
    let original = input.clone();
    let fallback = assert_originals(&input);
    for page in 200..204 {
        let address = TeletextPageAddress {
            page: TeletextPageNumber::new(page).unwrap(),
            subcode: owner().subcode,
        };
        assert_eq!(fallback[&address], fallback[&global_owner(address)]);
    }
    let result = encode_teletext_authored_service(
        input.clone(),
        &authored::transmissions(&input),
        &service_budget(),
    );
    assert_eq!(input, original);
    let encoded = result.unwrap_or_else(|error| {
        assert!(
            matches!(&error.reason, TeletextAuthoredServiceEncodingFailure::CompilerDrcs(reason)
            if matches!(reason.as_ref(),TeletextDrcsExtensionError::Budget {
                category:"added glyphs", count:8, maximum:4,
            })),
            "{error:?}"
        );
        panic!("both valid scope banks require eight established glyph variants: {error}");
    });
    assert_output(&input, &encoded, &fallback);
    proof::assert_displays(&input, &encoded);
}

#[path = "both_scopes/proof.rs"]
mod proof;
