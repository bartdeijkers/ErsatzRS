//! Linked-subpage capacity is physical identity capacity, never authored mutation.

use super::*;

fn filled(normal_count: u8, global_count: u8) -> TeletextSemanticService {
    let seed = occupied_seed(true);
    let tables = seed
        .drcs()
        .subtables()
        .iter()
        .flat_map(|table| {
            let count = if table.identity().kind() == TeletextResourceKind::Drcs {
                normal_count
            } else {
                global_count
            };
            (0..count).map(move |subpage| {
                let old = table.identity();
                TeletextDrcsSubtable::new(
                    TeletextResourceIdentity::new(
                        old.kind(),
                        old.page(),
                        TeletextResourceSubpageIndex::new(subpage).unwrap(),
                    )
                    .unwrap(),
                    table.glyphs().to_vec(),
                )
                .unwrap()
            })
        })
        .collect();
    rebuild(
        &seed,
        seed.page_enhancements().values().cloned().collect(),
        tables,
        seed.objects().definitions().to_vec(),
    )
}

fn larger_declaration(seed: TeletextSemanticService) -> TeletextDeclaredService {
    let mut limits = authored::declaration_limits();
    limits.max_resources = 32;
    TeletextDeclaredService::new(
        seed,
        vec![],
        vec![TeletextDisplayMotSelection {
            display: owner(),
            mot: None,
        }],
        vec![],
        limits,
    )
    .unwrap()
}

pub(super) fn exhausted() -> (
    TeletextDeclaredService,
    TeletextAuthoredServiceEncodingBudget,
) {
    let input = larger_declaration(filled(16, 16));
    let mut budget = service_budget();
    budget.max_pages = 33;
    (input, budget)
}

#[test]
fn full_normal_page_uses_free_global_subpage_without_renumbering() {
    let seed = filled(16, 1);
    let input = larger_declaration(seed.clone());
    let encoded = encode(&input);
    let generated = &encoded.compiler_drcs_resources()[0];
    let identity = generated.table().identity();
    assert_eq!(identity.kind(), TeletextResourceKind::Gdrcs);
    assert_eq!(identity.logical_subpage().get(), 1);
    assert_eq!(
        identity.page(),
        seed.page_enhancements()[&owner()]
            .resources
            .drcs
            .page(TeletextDrcsTableScope::Global)
            .unwrap()
    );
    assert_eq!(generated.table().glyphs()[0].bitmap.get(), POLISH_5B_MASK);
    let original = seed.page_enhancements()[&owner()]
        .compositional_links
        .unwrap();
    let links = generated.compositional_links(owner()).unwrap();
    assert_eq!(
        links.link(TeletextX27LinkRole::Drcs),
        original.link(TeletextX27LinkRole::Drcs)
    );
    assert_eq!(
        links
            .link(TeletextX27LinkRole::Gdrcs)
            .required_subpages()
            .get(),
        3
    );
    assert_eq!(encoded.declared(), &input);
    assert_eq!(
        rows(display(&encoded).page()),
        rows(&polish_fallback(&seed))
    );
    assert_eq!(encoded.pages().len(), 19);
}

#[test]
fn occupied_link_supplement_obeys_exact_page_and_unit_limits() {
    let input = declared(occupied_seed(true));
    let encoded = encode(&input);
    let mut exact = service_budget();
    exact.max_pages = encoded.pages().len();
    exact.max_units = encoded.unit_count();
    let tx = authored::transmissions(&input);
    assert_eq!(
        compiled_units(&encode_teletext_authored_service(input.clone(), &tx, &exact).unwrap()),
        compiled_units(&encoded)
    );
    for page_limit in [true, false] {
        let mut small = exact;
        if page_limit {
            small.max_pages -= 1;
        } else {
            small.max_units -= 1;
        }
        assert!(encode_teletext_authored_service(input.clone(), &tx, &small).is_err());
    }
    assert_eq!(input.semantic().drcs().subtables().len(), 2);
}

#[test]
fn linked_compiler_subpage_preserves_full_raster_lifecycle_and_carousel() {
    use crate::teletext::{self, TeletextCarousel, TeletextCarouselBatchKind};
    let input = declared(occupied_seed(true));
    let encoded = encode(&input);
    let d = display(&encoded);
    let options = TeletextColourPixelOptions {
        reveal_concealed: false,
        flash_phase: TeletextFlashPhase::Original,
    };
    let budget = teletext::TeletextRasterBudget {
        max_pixels: 120_000,
        max_diagnostics: 4096,
    };
    let before =
        teletext::render_teletext_composition(d.normalized().original(), options, budget).unwrap();
    let after = teletext::render_teletext_physical_display(
        d.normalized().physical().replay(),
        options,
        budget,
    )
    .unwrap();
    assert!(before.diagnostics().is_empty() && after.diagnostics().is_empty());
    assert_eq!(
        (before.width(), before.height()),
        (after.width(), after.height())
    );
    assert_eq!(before.pixels(), after.pixels());
    let encoded = std::sync::Arc::new(
        encoded
            .with_resource_lifecycle_variants(Instant::now() + Duration::from_secs(10))
            .unwrap(),
    );
    assert_eq!(
        encoded
            .resource_lifecycle_variants()
            .unwrap()
            .entries()
            .len(),
        3
    );
    let mut carousel =
        TeletextCarousel::new_authored_shared(std::sync::Arc::clone(&encoded)).unwrap();
    // All resource kinds have equal acquisition rank; keep sealed encoded order.
    for index in [1, 2, 3, 0] {
        let units = encoded.pages()[index].data_units();
        let header = carousel.next_batch().unwrap();
        assert_eq!(header.kind(), TeletextCarouselBatchKind::Header);
        assert_eq!(header.data_units(), &units[..1]);
        for chunk in units[1..].chunks(31) {
            let body = carousel.next_batch().unwrap();
            assert_eq!(body.kind(), TeletextCarouselBatchKind::Page);
            assert_eq!(body.data_units(), chunk);
        }
    }
    assert_eq!(
        carousel.next_batch().unwrap().kind(),
        TeletextCarouselBatchKind::Terminator
    );
    assert!(carousel.is_cycle_boundary());
    assert_eq!(encoded.declared(), &input);
}

#[test]
fn linked_compiler_subpage_cannot_exceed_two_actually_invoked_subtables() {
    let seed = occupied_seed(true);
    let mut pages: Vec<_> = seed.page_enhancements().values().cloned().collect();
    let mut commands = pages[0].local_program.as_ref().unwrap().commands().to_vec();
    commands.push(TeletextEnhancementCommand::SetPosition {
        position: TeletextEnhancementPosition::new(4, 1).unwrap(),
    });
    for (column, table) in [
        (1, TeletextDrcsTableScope::Normal),
        (2, TeletextDrcsTableScope::Global),
    ] {
        commands.push(TeletextEnhancementCommand::AtColumn {
            column,
            operation: TeletextColumnOperation::Drcs {
                reference: TeletextDrcsGlyphReference {
                    table,
                    subtable: TeletextDrcsSubtableIndex::new(0).unwrap(),
                    slot: TeletextDrcsGlyphSlot::new(0).unwrap(),
                },
            },
        });
    }
    pages[0].local_program = Some(TeletextEnhancementProgram::new(commands).unwrap());
    // Constructing the original proves its two authored references are valid.
    let mut original_budget = authored::semantic_limits();
    original_budget.max_collected_drcs_references = 2;
    assert!(seed.magazine_presentations().is_empty());
    let source = TeletextSemanticService::new(
        TeletextResolvedServiceDraft {
            base: seed.base().clone(),
            pages,
            magazines: vec![],
            objects: seed.objects().definitions().to_vec(),
            drcs: seed.drcs().subtables().to_vec(),
        },
        original_budget,
    )
    .unwrap()
    .with_physical_base_overlays(
        seed.physical_base_overlays().values().cloned().collect(),
        original_budget,
    )
    .unwrap();
    let input = declared(source.clone());
    let mut encoding_budget = service_budget();
    encoding_budget
        .normalization
        .composition
        .collection
        .max_drcs_references = 16;
    let error = encode_teletext_authored_service(
        input.clone(),
        &authored::transmissions(&input),
        &encoding_budget,
    )
    .unwrap_err();
    assert_eq!(error.display, Some(owner()));
    assert!(
        format!("{error:?}").contains("DisplaySubtables"),
        "{error:?}"
    );
    assert_eq!(input.semantic(), &source);
    assert_eq!(input.resource_inventory().len(), 2);
}
