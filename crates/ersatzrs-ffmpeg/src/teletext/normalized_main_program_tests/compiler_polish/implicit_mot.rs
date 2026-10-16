//! An authenticated MOT selection may occupy both DRCS roles without page X27.

use super::*;

fn mot_identity() -> TeletextResourceIdentity {
    TeletextResourceIdentity::new(
        TeletextResourceKind::Mot,
        TeletextRawDataPageNumber::new(2, 0xfe).unwrap(),
        TeletextResourceSubpageIndex::new(9).unwrap(),
    )
    .unwrap()
}

fn unused_target() -> TeletextX27LinkTarget {
    TeletextX27LinkTarget::Unspecified {
        magazine: TeletextX27Magazine::new(2).unwrap(),
    }
}

fn declaration(seed: &TeletextSemanticService) -> TeletextMotLevel25Declaration {
    // These unused object fields follow authored_service_tests::empty_link.
    // Ignored raw default numbers and counts are retained declaration data,
    // not assertions of fetched availability or effective default objects.
    let objects = TeletextMotObjectLink::new(
        unused_target(),
        TeletextMotSubpageExtent::new(0).unwrap(),
        true,
        TeletextMotFallback::new(0).unwrap(),
        [TeletextMotDefaultObject::NoObject { raw_number: 77 }; 2],
    );
    let unused = TeletextMotDrcsLink::new(
        unused_target(),
        TeletextMotSubpageExtent::new(0).unwrap(),
        true,
    );
    let linked = |scope| {
        TeletextMotDrcsLink::new(
            TeletextX27LinkTarget::Page {
                page: seed.page_enhancements()[&owner()]
                    .resources
                    .drcs
                    .page(scope)
                    .unwrap(),
            },
            TeletextMotSubpageExtent::new(0).unwrap(),
            false,
        )
    };
    let mut normal = [unused; 7];
    normal[0] = linked(TeletextDrcsTableScope::Normal);
    TeletextMotLevel25Declaration::new(
        mot_identity(),
        vec![TeletextMotPageAssociation::new(
            owner().page.wire_page(),
            TeletextMotAssociationSelection::new(false, None),
            TeletextMotAssociationSelection::new(
                true,
                Some(TeletextMotPublicLinkIndex::new(1).unwrap()),
            ),
        )],
        TeletextMotLevel25Links::new(
            objects,
            [objects; 7],
            linked(TeletextDrcsTableScope::Global),
            normal,
            TeletextMotEnhancementCounts::new(0, 0, 0),
        )
        .unwrap(),
    )
    .unwrap()
}

fn input() -> TeletextDeclaredService {
    let seed = occupied_seed(true);
    let mut pages = seed
        .page_enhancements()
        .values()
        .cloned()
        .collect::<Vec<_>>();
    pages[0].compositional_links = None;
    let semantic = rebuild(
        &seed,
        pages,
        seed.drcs().subtables().to_vec(),
        seed.objects().definitions().to_vec(),
    );
    let mot = declaration(&semantic);
    TeletextDeclaredService::new(
        semantic,
        vec![mot],
        vec![TeletextDisplayMotSelection {
            display: owner(),
            mot: Some(mot_identity()),
        }],
        vec![],
        authored::declaration_limits(),
    )
    .unwrap()
}

fn assert_original_declarations(input: &TeletextDeclaredService) {
    let service = input.semantic();
    let metadata = &service.page_enhancements()[&owner()];
    assert!(metadata.compositional_links.is_none());
    assert_eq!(metadata.resources.defaults, [None; 4]);
    assert!(input.defaults().is_empty());
    assert_eq!(input.mots(), &[declaration(service)]);
    assert_eq!(
        input.selections(),
        [TeletextDisplayMotSelection {
            display: owner(),
            mot: Some(mot_identity())
        }]
    );
    assert_eq!(input.resource_inventory().len(), 3);
    assert_eq!(service.drcs().subtables().len(), 2);
    assert!(service.drcs().subtables().iter().all(|table| table
        .identity()
        .logical_subpage()
        .get()
        == 0
        && table.glyphs().len() == 1
        && table.glyphs()[0].bitmap.get() == [0; 10]));
    let resolved = &input.resolutions()[0];
    assert_eq!(resolved.display(), owner());
    assert_eq!(resolved.mot().unwrap().selected_mot(), mot_identity());
    assert_eq!(resolved.defaults(), &[None; 4]);
    for role in [TeletextX27LinkRole::Gpop, TeletextX27LinkRole::Pop] {
        assert_eq!(*resolved.role(role), TeletextMotRoleResolution::Unselected);
    }
    for (scope, role) in [
        (TeletextDrcsTableScope::Normal, TeletextX27LinkRole::Drcs),
        (TeletextDrcsTableScope::Global, TeletextX27LinkRole::Gdrcs),
    ] {
        let target = metadata.resources.drcs.page(scope).unwrap();
        let TeletextMotRoleResolution::Selected {
            target: actual,
            requirements,
            original_mot,
            ..
        } = *resolved.role(role)
        else {
            panic!("both roles must be independently resolved by the selected MOT");
        };
        assert_eq!(actual, target);
        assert_eq!(
            requirements,
            TeletextMotResolutionRequirements::MotExtent(TeletextMotSubpageExtent::new(0).unwrap())
        );
        let original = original_mot.unwrap();
        assert_eq!(original.target, target);
        assert!(!original.supersedable);
        assert!(
            matches!(original.provenance, TeletextMotLinkProvenance::Mot {identity,..} if identity == mot_identity())
        );
        assert!(!input
            .resource_inventory()
            .iter()
            .any(|identity| identity.page() == target && identity.logical_subpage().get() == 1));
    }
}

fn original_wires(
    input: &TeletextDeclaredService,
) -> Vec<(TeletextResourceIdentity, Vec<[u8; 46]>)> {
    let tx = authored::transmissions(input);
    assert_eq!(tx.len(), 3);
    tx.into_iter()
        .map(|tx| {
            let encoded = if tx.resource == mot_identity() {
                crate::teletext::encode_teletext_mot_page(
                    &input.mots()[0],
                    &[[None; 2]; 8],
                    tx.counter,
                    tx.update,
                    crate::teletext::TeletextResourceTransmissionMode::Serial,
                )
                .unwrap()
            } else {
                let table = input
                    .semantic()
                    .drcs()
                    .subtables()
                    .iter()
                    .find(|table| table.identity() == tx.resource)
                    .unwrap();
                crate::teletext::encode_teletext_drcs_page(
                    table,
                    tx.counter,
                    tx.update,
                    crate::teletext::TeletextResourceTransmissionMode::Serial,
                )
                .unwrap()
            };
            (tx.resource, encoded.data_units().to_vec())
        })
        .collect()
}

fn assert_effective_links(input: &TeletextDeclaredService, links: TeletextX27Level25Links) {
    let original = &input.resolutions()[0];
    let resolved =
        TeletextMotLevel25Resolution::resolve(&input.mots()[0], owner(), Some(links)).unwrap();
    assert_eq!(resolved.selected_mot(), mot_identity());
    assert_eq!(
        resolved.association(),
        original.mot().unwrap().association()
    );
    assert_eq!(resolved.mot_links(), original.mot().unwrap().mot_links());
    assert_eq!(
        resolved.default_sources(),
        original.mot().unwrap().default_sources()
    );
    assert_eq!(resolved.default_conflicts(), &[None, None]);
    for role in TeletextX27LinkRole::ALL {
        match (*original.role(role), *resolved.role(role)) {
            (TeletextMotRoleResolution::Unselected, TeletextMotRoleResolution::Unselected) => {}
            (
                TeletextMotRoleResolution::Selected {
                    target,
                    original_mot,
                    ..
                },
                TeletextMotRoleResolution::Selected {
                    target: actual,
                    requirements,
                    original_mot: retained,
                    ..
                },
            ) => {
                assert_eq!(actual, target);
                assert_eq!(retained, original_mot);
                assert_eq!(
                    links.link(role).target(),
                    TeletextX27LinkTarget::Page { page: target }
                );
                assert_eq!(
                    links.link(role).validity(),
                    TeletextX27LinkValidity::Level25Only
                );
                let mask = if role == TeletextX27LinkRole::Drcs {
                    3
                } else {
                    1
                };
                assert_eq!(
                    requirements,
                    TeletextMotResolutionRequirements::X27Mask(
                        TeletextSubpageRequirementMask::new(mask)
                    )
                );
                assert_eq!(links.link(role).required_subpages().get(), mask);
            }
            state => panic!("materialized roles must preserve the selected MOT meaning: {state:?}"),
        }
    }
}

fn assert_raster(display: &crate::teletext::EncodedTeletextCompiledDisplayPage) {
    let options = TeletextColourPixelOptions {
        reveal_concealed: false,
        flash_phase: TeletextFlashPhase::Original,
    };
    let budget = crate::teletext::TeletextRasterBudget {
        max_pixels: 120_000,
        max_diagnostics: 4096,
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
    assert_eq!(
        (original.width(), original.height()),
        (actual.width(), actual.height())
    );
    assert_eq!(original.pixels(), actual.pixels());
}

fn assert_native_x27(
    display: &crate::teletext::EncodedTeletextCompiledDisplayPage,
    links: TeletextX27Level25Links,
) {
    let projection = project_encoded_teletext_display(display.page(), 128).unwrap();
    let words = projection
        .records
        .iter()
        .find_map(|record| match record {
            TeletextEncodedDisplayRecord::Triplets {
                row: 27,
                designation: 4,
                words,
            } => Some(words),
            _ => None,
        })
        .expect("the effective page must actually transmit its materialized X27/4");
    let mut actual = [0; 40];
    actual[0] = crate::teletext::packets::hamming_8_4(4);
    for (target, word) in actual[1..].chunks_exact_mut(3).zip(words) {
        target.copy_from_slice(&crate::teletext::packets::hamming_24_18(*word));
    }
    assert_eq!(
        actual,
        crate::teletext::encode_teletext_compositional_links(owner(), &links)
    );
}

fn assert_generated(input: &TeletextDeclaredService, encoded: &EncodedTeletextAuthoredService) {
    assert_eq!(encoded.declared(), input);
    assert_eq!(encoded.compiler_drcs_resources().len(), 1);
    let generated = &encoded.compiler_drcs_resources()[0];
    let identity = generated.table().identity();
    assert_eq!(identity.kind(), TeletextResourceKind::Drcs);
    assert_eq!(
        identity.page(),
        TeletextRawDataPageNumber::new(2, 0xa0).unwrap()
    );
    assert_eq!(identity.logical_subpage().get(), 1);
    assert_eq!(generated.consumers(), &[owner()]);
    assert_eq!(generated.table().glyphs().len(), 1);
    assert_eq!(generated.table().glyphs()[0].slot.get(), 0);
    assert_eq!(generated.table().glyphs()[0].bitmap.get(), POLISH_5B_MASK);
    let links = *generated.compositional_links(owner()).unwrap();
    assert_effective_links(input, links);
    let display = display(encoded);
    assert_native_x27(display, links);
    assert_polish_original(display.normalized().original());
    let replay = display.normalized().physical().replay();
    for column in [7, 8] {
        let candidate = selected(replay.display_resolution(), 1, column);
        let executed = replay
            .drcs_execution()
            .records()
            .iter()
            .find(|record| record.candidate_index() == candidate)
            .unwrap();
        assert_eq!(executed.resource(), identity);
        assert_eq!(executed.reference().table, TeletextDrcsTableScope::Normal);
        assert_eq!(executed.reference().subtable.get(), 1);
        assert_eq!(executed.reference().slot.get(), 0);
        assert_eq!(executed.bitmap().get(), POLISH_5B_MASK);
    }
    let page = encoded
        .pages()
        .iter()
        .find(|page| {
            page.resource_header()
                .is_some_and(|header| header.identity() == identity)
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
    let actual = std::array::from_fn(|row| {
        (u16::from(cells[row * 2] & 63) << 6) | u16::from(cells[row * 2 + 1] & 63)
    });
    assert_eq!(actual, POLISH_5B_MASK);
    assert_raster(display);
}

#[test]
fn selected_mot_without_x27_preserves_authored_roles_and_uses_free_linked_subpage() {
    let input = input();
    let before = input.clone();
    assert_original_declarations(&input);
    let original_wires = original_wires(&input);
    let fallback = rows(&polish_fallback(input.semantic()));
    assert!(fallback.iter().any(|record| matches!(record,TeletextEncodedDisplayRecord::BaseRow {row:1,cells} if cells[7] == 0x5b)));
    let budget = service_budget().normalization;
    let allocations = crate::teletext::allocate_teletext_resource_objects(
        input.semantic().objects(),
        &[],
        budget.max_resource_objects,
    )
    .unwrap();
    let composed = crate::teletext::compile_teletext_composed_main_program(
        input.semantic(),
        owner(),
        &allocations,
        budget.composition,
        budget.packing,
    )
    .unwrap();
    let original = composed.composition();
    let origin = assert_polish_original(original);
    for column in [7, 8] {
        let cell = original.display_resolution().cell(1, column).unwrap();
        assert_eq!(cell.attributes().spacing_separated().value(), Some(false));
        assert_eq!(
            cell.attributes().underline_or_separated_mosaics().value(),
            Some(false)
        );
        assert_eq!(
            crate::teletext::project_teletext_comparison_glyph(original, cell)
                .unwrap()
                .mask
                .unwrap()
                .rows(),
            POLISH_5B_MASK
        );
    }
    let plan =
        crate::teletext::plan_teletext_base_compatibility_realization(original, budget.realization)
            .unwrap();
    assert!(plan
        .origins()
        .iter()
        .any(|candidate| candidate.origin_index == origin));
    let result = encode_teletext_authored_service(
        input.clone(),
        &authored::transmissions(&input),
        &service_budget(),
    );
    assert_eq!(input, before);
    let encoded = result.expect("valid selected MOT roles require an exact free linked S1 carrier without rewriting the authored MOT");
    assert_generated(&input, &encoded);
    assert_eq!(display(&encoded).normalized().original(), original);
    assert_eq!(rows(display(&encoded).page()), fallback);
    for (identity, bytes) in original_wires {
        let page = encoded
            .pages()
            .iter()
            .find(|page| {
                page.resource_header()
                    .is_some_and(|header| header.identity() == identity)
            })
            .unwrap();
        assert_eq!(page.data_units(), bytes);
    }
    assert_eq!(input, before);
}

#[path = "implicit_mot/fresh.rs"]
mod fresh;

#[path = "implicit_mot/passive_default.rs"]
mod passive_default;
