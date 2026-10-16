//! A fresh Normal bank must retain a Global role inherited only from MOT.

use super::*;

fn fresh_identity() -> TeletextResourceIdentity {
    // resource_namespace.rs scans unreserved non-BCD even pages from1A0.
    // Original owners are200,2FE,2A0,1A1; none reserves this first candidate.
    TeletextResourceIdentity::new(
        TeletextResourceKind::Drcs,
        TeletextRawDataPageNumber::new(1, 0xa0).unwrap(),
        TeletextResourceSubpageIndex::ZERO,
    )
    .unwrap()
}

fn global_only_input() -> TeletextDeclaredService {
    let original = input();
    let seed = original.semantic();
    let mut pages = seed
        .page_enhancements()
        .values()
        .cloned()
        .collect::<Vec<_>>();
    pages[0].resources.drcs = TeletextDrcsPageLinks::new(
        None,
        pages[0].resources.drcs.page(TeletextDrcsTableScope::Global),
    )
    .unwrap();
    let semantic = rebuild(
        seed,
        pages,
        seed.drcs().subtables().to_vec(),
        seed.objects().definitions().to_vec(),
    );
    let mot = TeletextMotLevel25Declaration::new(
        original.mots()[0].identity(),
        vec![TeletextMotPageAssociation::new(
            owner().page.wire_page(),
            original.mots()[0].associations()[0].objects(),
            TeletextMotAssociationSelection::new(true, None),
        )],
        original.mots()[0].links(),
    )
    .unwrap();
    TeletextDeclaredService::new(
        semantic,
        vec![mot],
        original.selections().to_vec(),
        vec![],
        authored::declaration_limits(),
    )
    .unwrap()
}

fn assert_global_only_original(input: &TeletextDeclaredService) {
    let metadata = &input.semantic().page_enhancements()[&owner()];
    assert!(metadata.compositional_links.is_none());
    assert_eq!(
        metadata.resources.drcs.page(TeletextDrcsTableScope::Normal),
        None
    );
    assert_eq!(
        metadata.resources.drcs.page(TeletextDrcsTableScope::Global),
        Some(TeletextRawDataPageNumber::new(1, 0xa1).unwrap())
    );
    assert_eq!(metadata.resources.defaults, [None; 4]);
    assert!(input.defaults().is_empty());
    assert_eq!(input.resource_inventory().len(), 3);
    assert_eq!(input.semantic().drcs().subtables().len(), 2);
    assert!(!input
        .resource_inventory()
        .iter()
        .any(|identity| identity.page() == fresh_identity().page()));
    assert!(input.semantic().drcs().subtables().iter().all(|table| table
        .identity()
        .logical_subpage()
        .get()
        == 0
        && table.glyphs().len() == 1
        && table.glyphs()[0].bitmap.get() == [0; 10]));
    let resolved = &input.resolutions()[0];
    assert_eq!(resolved.mot().unwrap().selected_mot(), mot_identity());
    assert_eq!(resolved.defaults(), &[None; 4]);
    for role in [
        TeletextX27LinkRole::Pop,
        TeletextX27LinkRole::Gpop,
        TeletextX27LinkRole::Drcs,
    ] {
        assert_eq!(*resolved.role(role), TeletextMotRoleResolution::Unselected);
    }
    let TeletextMotRoleResolution::Selected {
        target,
        requirements,
        original_mot,
        ..
    } = *resolved.role(TeletextX27LinkRole::Gdrcs)
    else {
        panic!("the Global role must be inherited from the selected MOT");
    };
    assert_eq!(target, TeletextRawDataPageNumber::new(1, 0xa1).unwrap());
    assert_eq!(
        requirements,
        TeletextMotResolutionRequirements::MotExtent(TeletextMotSubpageExtent::new(0).unwrap())
    );
    let candidate = original_mot.unwrap();
    assert_eq!(candidate.target, target);
    assert!(!candidate.supersedable);
    assert!(
        matches!(candidate.provenance,TeletextMotLinkProvenance::Mot {identity,role:TeletextMotLinkRole::Gdrcs} if identity == mot_identity())
    );
}

fn assert_fresh_links(input: &TeletextDeclaredService, links: TeletextX27Level25Links) {
    let original = input.resolutions()[0].mot().unwrap();
    let actual =
        TeletextMotLevel25Resolution::resolve(&input.mots()[0], owner(), Some(links)).unwrap();
    assert_eq!(actual.selected_mot(), original.selected_mot());
    assert_eq!(actual.association(), original.association());
    assert_eq!(actual.mot_links(), original.mot_links());
    assert_eq!(actual.default_sources(), original.default_sources());
    assert_eq!(actual.default_conflicts(), original.default_conflicts());
    for role in [TeletextX27LinkRole::Pop, TeletextX27LinkRole::Gpop] {
        assert_eq!(*actual.role(role), *original.role(role));
        assert_eq!(*actual.role(role), TeletextMotRoleResolution::Unselected);
    }
    for role in [TeletextX27LinkRole::Drcs, TeletextX27LinkRole::Gdrcs] {
        assert_eq!(
            links.link(role).validity(),
            TeletextX27LinkValidity::Level25Only
        );
        assert_eq!(links.link(role).required_subpages().get(), 1);
    }
    let TeletextMotRoleResolution::Selected {
        target,
        requirements,
        original_mot,
        provenance,
    } = *actual.role(TeletextX27LinkRole::Drcs)
    else {
        panic!("the fresh Normal role must be explicitly materialized");
    };
    assert_eq!(target, fresh_identity().page());
    assert_eq!(
        requirements,
        TeletextMotResolutionRequirements::X27Mask(TeletextSubpageRequirementMask::new(1))
    );
    assert_eq!(original_mot, None);
    assert_eq!(
        provenance,
        TeletextMotLinkProvenance::X27 {
            display: owner(),
            role: TeletextX27LinkRole::Drcs
        }
    );
    let TeletextMotRoleResolution::Selected {
        target,
        requirements,
        original_mot,
        ..
    } = *actual.role(TeletextX27LinkRole::Gdrcs)
    else {
        panic!("the inherited Global role must survive fresh Normal allocation");
    };
    let TeletextMotRoleResolution::Selected {
        target: old,
        original_mot: retained,
        ..
    } = *original.role(TeletextX27LinkRole::Gdrcs)
    else {
        unreachable!()
    };
    assert_eq!(target, old);
    assert_eq!(original_mot, retained);
    assert_eq!(
        requirements,
        TeletextMotResolutionRequirements::X27Mask(TeletextSubpageRequirementMask::new(1))
    );
    assert_eq!(
        links.link(TeletextX27LinkRole::Gdrcs).target(),
        TeletextX27LinkTarget::Page { page: old }
    );
}

fn assert_fresh_output(input: &TeletextDeclaredService, encoded: &EncodedTeletextAuthoredService) {
    assert_eq!(encoded.declared(), input);
    assert_eq!(encoded.compiler_drcs_resources().len(), 1);
    let generated = &encoded.compiler_drcs_resources()[0];
    assert_eq!(generated.table().identity(), fresh_identity());
    assert_eq!(generated.consumers(), &[owner()]);
    assert_eq!(generated.table().glyphs().len(), 1);
    assert_eq!(generated.table().glyphs()[0].slot.get(), 0);
    assert_eq!(generated.table().glyphs()[0].bitmap.get(), POLISH_5B_MASK);
    let links = *generated.compositional_links(owner()).unwrap();
    assert_fresh_links(input, links);
    let display = display(encoded);
    assert_native_x27(display, links);
    let replay = display.normalized().physical().replay();
    for column in [7, 8] {
        let index = selected(replay.display_resolution(), 1, column);
        let actual = replay
            .drcs_execution()
            .records()
            .iter()
            .find(|record| record.candidate_index() == index)
            .unwrap();
        assert_eq!(actual.resource(), fresh_identity());
        assert_eq!(actual.reference().table, TeletextDrcsTableScope::Normal);
        assert_eq!(actual.reference().subtable.get(), 0);
        assert_eq!(actual.reference().slot.get(), 0);
        assert_eq!(actual.bitmap().get(), POLISH_5B_MASK);
    }
    let resource = encoded
        .pages()
        .iter()
        .find(|page| {
            page.resource_header()
                .is_some_and(|header| header.identity() == fresh_identity())
        })
        .unwrap();
    let projection = project_encoded_teletext_drcs(resource, 32).unwrap();
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
fn fresh_normal_bank_preserves_global_role_inherited_from_selected_mot() {
    let input = global_only_input();
    let before = input.clone();
    assert_global_only_original(&input);
    let wires = original_wires(&input);
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
        assert_eq!(
            cell.attributes().underline_or_separated_mosaics().value(),
            Some(false)
        );
        assert_eq!(cell.attributes().spacing_separated().value(), Some(false));
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
    let encoded = result
        .expect("a fresh Normal bank must materialize and preserve the inherited Global MOT role");
    assert_fresh_output(&input, &encoded);
    assert_eq!(display(&encoded).normalized().original(), original);
    assert_eq!(rows(display(&encoded).page()), fallback);
    for (identity, bytes) in wires {
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
