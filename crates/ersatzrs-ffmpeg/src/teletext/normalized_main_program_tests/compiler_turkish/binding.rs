//! Existing-bank permission and actual-execution counterfactuals.

use super::*;

pub(super) fn authored_seed(
    mask: [u16; 10],
    linked: bool,
    underlined: bool,
) -> TeletextSemanticService {
    let seed = turkish_base(0x16, underlined, false);
    let identity = TeletextResourceIdentity::new(
        TeletextResourceKind::Drcs,
        TeletextRawDataPageNumber::new(2, 0xa2).unwrap(),
        TeletextResourceSubpageIndex::new(3).unwrap(),
    )
    .unwrap();
    let mut pages = seed
        .page_enhancements()
        .values()
        .cloned()
        .collect::<Vec<_>>();
    if linked {
        pages[0].resources.drcs = TeletextDrcsPageLinks::new(Some(identity.page()), None).unwrap();
        let unused = TeletextX27LinkDeclaration::new(
            TeletextX27LinkTarget::Unspecified {
                magazine: TeletextX27Magazine::new(2).unwrap(),
            },
            TeletextX27LinkValidity::Level25Only,
            TeletextSubpageRequirementMask::new(0xffff),
        )
        .unwrap();
        let drcs = TeletextX27LinkDeclaration::new(
            TeletextX27LinkTarget::Page {
                page: identity.page(),
            },
            TeletextX27LinkValidity::Level25Only,
            TeletextSubpageRequirementMask::new(1 << 3),
        )
        .unwrap();
        pages[0].compositional_links =
            Some(TeletextX27Level25Links::new(unused, unused, unused, drcs).unwrap());
    }
    TeletextSemanticService::new(
        TeletextResolvedServiceDraft {
            base: seed.base().clone(),
            pages,
            magazines: vec![],
            objects: seed.objects().definitions().to_vec(),
            drcs: vec![TeletextDrcsSubtable::new(
                identity,
                vec![TeletextDrcsGlyphDefinition {
                    slot: TeletextDrcsGlyphSlot::new(5).unwrap(),
                    bitmap: TeletextDrcsBitmap::new(mask).unwrap(),
                }],
            )
            .unwrap()],
        },
        authored::semantic_limits(),
    )
    .unwrap()
    .with_physical_base_overlays(
        seed.physical_base_overlays().values().cloned().collect(),
        authored::semantic_limits(),
    )
    .unwrap()
}

#[test]
fn turkish_existing_bank_bridge_rejects_unlinked_one_pixel_and_missing_underline() {
    let mut changed = TURKISH_23_MASK;
    changed[4] ^= 1;
    let mut partial = turkish_mask(true);
    partial[9] = 0xffe;
    for (mask, linked, underlined) in [
        (TURKISH_23_MASK, false, false),
        (changed, true, false),
        (TURKISH_23_MASK, true, true),
        (partial, true, true),
    ] {
        let seed = authored_seed(mask, linked, underlined);
        let before = seed.clone();
        let fallback = super::super::polish_carrier::polish_fallback(&seed);
        let original = compose(&seed);
        let origin = assert_original_case(&original, 0x16, underlined, false);
        let error = compile_teletext_normalized_main_program(
            &seed,
            owner(),
            service_budget().normalization,
        )
        .unwrap_err();
        let Failure::Assembly(assembly) = error.reason else {
            panic!("expected existing-bank-only assembly rejection");
        };
        assert_eq!(assembly.origin, Some(origin));
        let crate::teletext::TeletextAugmentedOriginProgramFailure::Carrier(carrier) =
            assembly.reason
        else {
            panic!("expected located carrier rejection");
        };
        assert_eq!(carrier.origin, origin);
        assert_eq!(
            carrier.reason,
            crate::teletext::TeletextOriginCarrierFailure::NationalConversion
        );
        assert_eq!(seed, before);
        assert_eq!(
            super::super::polish_carrier::polish_fallback(&seed),
            fallback
        );
    }
}

#[test]
fn turkish_typed_and_physical_witnesses_reject_substituted_execution_bitmap() {
    let seed = authored_seed(turkish_mask(true), true, true);
    let original = compose(&seed);
    let budget = service_budget().normalization;
    let plan = crate::teletext::plan_teletext_base_compatibility_realization(
        &original,
        budget.realization,
    )
    .unwrap();
    let placement = crate::teletext::plan_teletext_origin_carrier_placement(
        &plan,
        seed.objects(),
        budget.placement,
    )
    .unwrap();
    let augmented =
        crate::teletext::augmented_origin_program::build_augmented_with_service_and_work_usage(
            &placement,
            &seed,
            budget.assembly,
        )
        .0
        .unwrap();
    let allocations = crate::teletext::allocate_teletext_resource_objects(
        augmented.catalog(),
        &[],
        budget.max_resource_objects,
    )
    .unwrap();
    let mut changed = turkish_mask(true);
    changed[4] ^= 1;
    for (mask, valid) in [
        (turkish_mask(true), true),
        (TURKISH_23_MASK, false),
        (changed, false),
    ] {
        // Preserve producer-owned binding and invocation while changing only
        // the actual bank supplied to physical execution.
        let substituted = authored_seed(mask, true, true);
        let compiled = crate::teletext::compile_teletext_augmented_origin_program(
            &substituted,
            &augmented,
            &allocations,
            budget.replay,
            budget.packing,
            budget.ancestry,
        )
        .unwrap();
        let typed = crate::teletext::compare_teletext_augmented_origin_glyphs(
            &compiled,
            budget.glyph_comparison,
        )
        .unwrap();
        assert_eq!(typed.diagnostics().is_empty(), valid);
        let physical = crate::teletext::physical_comparison::compare_augmented_physical_with_usage(
            &compiled,
            budget.display_comparison,
        )
        .0
        .unwrap();
        assert_eq!(
            physical
                .diagnostics()
                .iter()
                .any(|diagnostic| diagnostic.position == Some((1, 7))),
            !valid
        );
    }
}
