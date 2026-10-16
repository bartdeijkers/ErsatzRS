use super::*;

// Table 36 Polish 5/B, retained as twelve-bit rows without a Unicode alias.
const POLISH_5B_MASK: [u16; 10] = [0, 0x3fc, 0x18, 0x30, 0x1f8, 0xc0, 0x180, 0x3fc, 0, 0];

fn polish_drcs_identity() -> TeletextResourceIdentity {
    TeletextResourceIdentity::new(
        TeletextResourceKind::Drcs,
        TeletextRawDataPageNumber::new(2, 0xa2).unwrap(),
        TeletextResourceSubpageIndex::new(3).unwrap(),
    )
    .unwrap()
}
fn polish_drcs_reference() -> TeletextDrcsGlyphReference {
    TeletextDrcsGlyphReference {
        table: TeletextDrcsTableScope::Normal,
        subtable: TeletextDrcsSubtableIndex::new(3).unwrap(),
        slot: TeletextDrcsGlyphSlot::new(5).unwrap(),
    }
}
fn polish_limits() -> TeletextNormalizedMainProgramBudget {
    let mut result = limits();
    result.replay.max_drcs_references = 16;
    result
}
fn enlarged_polish_base(
    authored_bank: Option<([u16; 10], bool)>,
    d5: bool,
) -> TeletextSemanticService {
    let mut fixture = enlarged_national_base();
    fixture.national = 0;
    fixture.primary = Some(0x08);
    fixture.rows = vec![text(1, 7, "A"), text(2, 8, "é"), text(3, 1, "B")];
    if d5 {
        // The Adaptive invocation owns this repaint's final attributes; a
        // preceding main attribute would be overwritten by its definition.
        let mut commands = fixture.objects[1].program.commands().to_vec();
        let TeletextEnhancementCommand::AtColumn {
            operation:
                TeletextColumnOperation::Attributes {
                    underline_or_separated_mosaics,
                    ..
                },
            ..
        } = &mut commands[0]
        else {
            unreachable!()
        };
        *underline_or_separated_mosaics = true;
        fixture.objects[1].program = program(commands);
    }
    if let Some((rows, linked)) = authored_bank {
        let identity = polish_drcs_identity();
        fixture.drcs = vec![TeletextDrcsSubtable::new(
            identity,
            vec![TeletextDrcsGlyphDefinition {
                slot: polish_drcs_reference().slot,
                bitmap: TeletextDrcsBitmap::new(rows).unwrap(),
            }],
        )
        .unwrap()];
        if linked {
            fixture.drcs_links = TeletextDrcsPageLinks::new(Some(identity.page()), None).unwrap();
        }
    }
    let service = fixture.build();
    // The supported physical editor input supplies literal Base 5/B; its
    // national identity comes from the original page designation.
    let mut cells = crate::teletext::layout::compile_for_composition(&service.base().pages[0])
        .unwrap()
        .rows[1];
    cells[7] = 0x5b;
    service
        .with_physical_base_overlays(
            vec![TeletextPhysicalBaseOverlay::new(
                owner(),
                vec![TeletextPhysicalBaseRow::new(1, cells).unwrap()],
            )
            .unwrap()],
            TeletextServiceBudget {
                max_records: 64,
                max_objects: 32,
                max_structure_items: 4096,
                max_base_content_bytes: 8192,
                max_commands: 8192,
                max_collected_commands: 16384,
                max_collected_drcs_references: 8192,
            },
        )
        .unwrap()
}
fn polish_fallback(service: &TeletextSemanticService) -> crate::teletext::EncodedTeletextPage {
    let page = &service.base().pages[0];
    let layout = crate::teletext::layout::compile_for_service_composition(service, page).unwrap();
    crate::teletext::packets::encode_compiled_page(
        page,
        &layout,
        &crate::teletext::packets::PagePacketMetadata {
            presentation: service.page_enhancements()[&owner()].presentation.as_ref(),
            compositional_links: None,
            enhancements: crate::teletext::packets::PageEnhancements::Emit,
        },
    )
    .unwrap()
}
fn assert_polish_original(original: &TeletextMainComposition) -> usize {
    assert_eq!(original.base_display().primary().get(), 0x08);
    let display = original.display_resolution();
    let origin = display.cell(1, 7).unwrap().origin().unwrap();
    assert_eq!(display.cell(1, 8).unwrap().origin(), Some(origin));
    assert!(origin >= original.glyph_layers().candidates().len());
    let candidate_index = display.origins()[origin].candidate_index();
    let candidate = original.glyph_layers().candidates()[candidate_index];
    assert_eq!(candidate.priority(), TeletextGlyphPriority::Base);
    assert_eq!(candidate.base().unwrap().code(), 0x5b);
    assert_eq!(candidate.source(), None);
    assert_eq!(candidate.root_source(), None);
    assert_eq!(candidate.invocation(), None);
    let intent = original.glyph_intent().records()[candidate_index];
    assert_eq!(intent.disposition(), TeletextGlyphDisposition::Assigned);
    assert!(
        matches!(intent.glyph(), Some(TeletextGlyphIdentity::G0 {glyph})
        if glyph.repertoire() == TeletextG0Repertoire::Latin && glyph.code() == 0x5b
            && glyph.national_subset() == Some(TeletextNationalSubset::Polish))
    );
    assert_eq!(original.base_replacement(1, 7), None);
    origin
}
fn assert_polish_conversion_pending(service: &TeletextSemanticService, d5: bool, spacing: bool) {
    let before = service.clone();
    let fallback = polish_fallback(service);
    let original = compose(service);
    let origin = assert_polish_original(&original);
    let attrs = original
        .display_resolution()
        .origin_attributes(origin)
        .unwrap();
    assert_eq!(attrs.underline_or_separated_mosaics().value(), Some(d5));
    assert_eq!(attrs.spacing_separated().value(), Some(spacing));
    let error =
        compile_teletext_normalized_main_program(service, owner(), polish_limits()).unwrap_err();
    let Failure::Assembly(assembly) = error.reason else {
        panic!("expected located carrier assembly rejection: {error:?}");
    };
    assert_eq!(assembly.owner, owner());
    assert_eq!(assembly.origin, Some(origin));
    let crate::teletext::TeletextAugmentedOriginProgramFailure::Carrier(carrier) = assembly.reason
    else {
        panic!("expected origin carrier rejection: {assembly:?}");
    };
    assert_eq!(carrier.origin, origin);
    assert_eq!(
        carrier.reason,
        crate::teletext::TeletextOriginCarrierFailure::NationalConversion
    );
    assert_eq!(polish_fallback(service), fallback);
    assert_eq!(service, &before);
}

#[test]
fn enlarged_overlap_polish_5b_without_authored_drcs_remains_located_pending() {
    // A resource-free physical recipe remains an explicit acceptance gap.
    assert_polish_conversion_pending(&enlarged_polish_base(None, false), false, false);
}

#[test]
fn enlarged_overlap_carries_polish_5b_via_linked_authored_drcs_without_changing_original_identity_or_fallback(
) {
    use crate::teletext::{
        allocate_teletext_resource_objects, compare_teletext_augmented_origin_glyphs,
        compare_teletext_physical_display, compile_teletext_augmented_origin_program,
        plan_teletext_base_compatibility_realization, plan_teletext_origin_carrier_placement,
        project_encoded_teletext_display, TeletextEncodedDisplayRecord,
        TeletextPhysicalComparisonField,
    };
    let service = enlarged_polish_base(Some((POLISH_5B_MASK, true)), false);
    let before = service.clone();
    let fallback = polish_fallback(&service);
    let projection = project_encoded_teletext_display(&fallback, 128).unwrap();
    assert!(projection.records.iter().any(|record| matches!(record,
        TeletextEncodedDisplayRecord::BaseRow {row:1,cells} if cells[7] == 0x5b)));
    let original = compose(&service);
    let origin = assert_polish_original(&original);
    assert!(original.drcs_execution().records().is_empty());
    let budget = polish_limits();
    let plan = plan_teletext_base_compatibility_realization(&original, budget.realization).unwrap();
    assert!(!plan.seeds().is_empty());
    assert!(plan
        .origins()
        .iter()
        .any(|candidate| candidate.origin_index == origin));
    // Successful normalization performs the witness-bound typed and whole-page
    // comparisons; the public detached physical comparator stays strict below.
    let normalized = compile_teletext_normalized_main_program(&service, owner(), budget).unwrap();
    assert_eq!(
        normalized.original().base_display(),
        original.base_display()
    );
    assert_polish_original(normalized.original());
    let actual = normalized.physical().replay();
    let actual_index = selected(actual.display_resolution(), 1, 7);
    assert_eq!(
        actual.glyph_intent().records()[actual_index].glyph(),
        Some(TeletextGlyphIdentity::Drcs {
            reference: polish_drcs_reference()
        })
    );
    let executed = actual
        .drcs_execution()
        .records()
        .iter()
        .find(|record| record.candidate_index() == actual_index)
        .unwrap();
    assert_eq!(executed.reference(), polish_drcs_reference());
    assert_eq!(executed.resource(), polish_drcs_identity());
    assert_eq!(
        executed.bitmap(),
        TeletextDrcsBitmap::new(POLISH_5B_MASK).unwrap()
    );
    let physical =
        compare_teletext_physical_display(&original, actual, budget.display_comparison).unwrap();
    assert_eq!(physical.compared_cells(), 1000);
    assert!(!physical.diagnostics().is_empty());
    assert!(
        physical
            .diagnostics()
            .iter()
            .all(|diagnostic| diagnostic.finding
                == TeletextPhysicalComparisonFinding::Difference(
                    TeletextPhysicalComparisonField::GlyphEffects
                )),
        "{:?}",
        physical.diagnostics()
    );
    assert!(physical
        .diagnostics()
        .iter()
        .any(|diagnostic| diagnostic.position == Some((1, 7))
            && diagnostic.original.origin == Some(origin)));
    let placement =
        plan_teletext_origin_carrier_placement(&plan, service.objects(), budget.placement).unwrap();
    let augmented =
        crate::teletext::augmented_origin_program::build_augmented_with_service_and_work_usage(
            &placement,
            &service,
            budget.assembly,
        )
        .0
        .unwrap();
    let allocations =
        allocate_teletext_resource_objects(augmented.catalog(), &[], budget.max_resource_objects)
            .unwrap();
    let compiled = compile_teletext_augmented_origin_program(
        &service,
        &augmented,
        &allocations,
        budget.replay,
        budget.packing,
        budget.ancestry,
    )
    .unwrap();
    let typed =
        compare_teletext_augmented_origin_glyphs(&compiled, budget.glyph_comparison).unwrap();
    assert_eq!(typed.compared_cells(), 1000);
    assert!(typed.diagnostics().is_empty(), "{:?}", typed.diagnostics());
    assert_eq!(polish_fallback(&service), fallback);
    assert_eq!(service, before);
}

#[test]
fn polish_5b_authored_drcs_bridge_rejects_unlinked_inexact_and_non_neutral_effects() {
    let mut changed = POLISH_5B_MASK;
    changed[1] ^= 1;
    for (mask, linked, d5) in [
        (POLISH_5B_MASK, false, false),
        (changed, true, false),
        (POLISH_5B_MASK, true, true),
    ] {
        let service = enlarged_polish_base(Some((mask, linked)), d5);
        assert_polish_conversion_pending(&service, d5, false);
    }
}

fn declared_polish_service(seed: &TeletextSemanticService) -> TeletextDeclaredService {
    use crate::teletext::authored_service_tests as authored;
    let mut metadata = seed
        .page_enhancements()
        .values()
        .cloned()
        .collect::<Vec<_>>();
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
            page: polish_drcs_identity().page(),
        },
        TeletextX27LinkValidity::Level25Only,
        TeletextSubpageRequirementMask::new(1 << 3),
    )
    .unwrap();
    metadata[0].compositional_links =
        Some(TeletextX27Level25Links::new(unused, unused, unused, drcs).unwrap());
    let semantic = TeletextSemanticService::new(
        TeletextResolvedServiceDraft {
            base: seed.base().clone(),
            pages: metadata,
            magazines: seed
                .magazine_presentations()
                .iter()
                .map(
                    |(&magazine, &presentation)| TeletextMagazinePresentationDraft {
                        magazine,
                        presentation,
                    },
                )
                .collect(),
            objects: seed.objects().definitions().to_vec(),
            drcs: seed.drcs().subtables().to_vec(),
        },
        authored::semantic_limits(),
    )
    .unwrap()
    .with_physical_base_overlays(
        seed.physical_base_overlays().values().cloned().collect(),
        authored::semantic_limits(),
    )
    .unwrap();
    TeletextDeclaredService::new(
        semantic,
        vec![],
        vec![TeletextDisplayMotSelection {
            display: owner(),
            mot: None,
        }],
        vec![],
        authored::declaration_limits(),
    )
    .unwrap()
}

#[test]
fn polish_5b_authored_service_preserves_existing_resource_inventory_lifecycle_and_fallback() {
    use crate::teletext::authored_service_tests as authored;
    use crate::teletext::{
        encode_teletext_authored_service, project_encoded_teletext_display,
        EncodedTeletextAuthoredPage, TeletextEncodedDisplayRecord,
    };
    let seed = enlarged_polish_base(Some((POLISH_5B_MASK, true)), false);
    let declared = declared_polish_service(&seed);
    let before = declared.clone();
    let tx = authored::transmissions(&declared);
    assert_eq!(tx.len(), 1);
    assert_eq!(tx[0].resource, polish_drcs_identity());
    let mut budget = authored::limits();
    budget.normalization = polish_limits();
    let encoded = encode_teletext_authored_service(declared, &tx, &budget).unwrap();
    assert_eq!(encoded.declared(), &before);
    assert_eq!(encoded.pages().len(), 2);
    assert_eq!(encoded.inventory().len(), 2);
    let EncodedTeletextAuthoredPage::CompiledDisplay(display) = &encoded.pages()[0] else {
        panic!("the resource bridge must run through the normalized display encoder");
    };
    assert_polish_original(display.normalized().original());
    let projection = project_encoded_teletext_display(display.page(), 128).unwrap();
    assert!(projection.records.iter().any(|record| matches!(record,
        TeletextEncodedDisplayRecord::BaseRow {row:1,cells} if cells[7]==0x5b)));
    assert_eq!(display.page().address(), owner());
    let header = encoded.pages()[1].resource_header().unwrap();
    assert_eq!(header.identity(), polish_drcs_identity());
    assert_eq!(header.physical_subcode(), 0x0313);
    let original_resource = crate::teletext::encode_teletext_drcs_page(
        &seed.drcs().subtables()[0],
        tx[0].counter,
        tx[0].update,
        crate::teletext::TeletextResourceTransmissionMode::Serial,
    )
    .unwrap();
    assert_eq!(
        encoded.pages()[1].data_units(),
        original_resource.data_units()
    );
    let original_rows = project_encoded_teletext_display(&polish_fallback(&seed), 128).unwrap();
    let base_rows = |records: Vec<TeletextEncodedDisplayRecord>| {
        records
            .into_iter()
            .filter(|record| matches!(record, TeletextEncodedDisplayRecord::BaseRow { .. }))
            .collect::<Vec<_>>()
    };
    assert_eq!(
        base_rows(projection.records),
        base_rows(original_rows.records)
    );

    assert_eq!(encoded.declared().semantic().drcs(), seed.drcs());
    assert_eq!(
        encoded.declared().semantic().physical_base_overlays(),
        seed.physical_base_overlays()
    );
}

#[test]
fn polish_5b_resource_lookup_and_new_reference_share_existing_bounds() {
    use crate::teletext::augmented_origin_program::build_augmented_with_service_and_work_usage;
    use crate::teletext::{
        plan_teletext_base_compatibility_realization, plan_teletext_origin_carrier_placement,
        TeletextAugmentedOriginProgramFailure,
    };
    let service = enlarged_polish_base(Some((POLISH_5B_MASK, true)), false);
    let before = service.clone();
    let original = compose(&service);
    let mut budget = polish_limits();
    let plan = plan_teletext_base_compatibility_realization(&original, budget.realization).unwrap();
    let placement =
        plan_teletext_origin_carrier_placement(&plan, service.objects(), budget.placement).unwrap();
    let (result, usage) =
        build_augmented_with_service_and_work_usage(&placement, &service, budget.assembly);
    result.unwrap();
    assert!(usage.work > 0);
    budget.assembly.max_work = usage.work - 1;
    let (result, denied) =
        build_augmented_with_service_and_work_usage(&placement, &service, budget.assembly);
    assert!(matches!(
        result.unwrap_err().reason,
        TeletextAugmentedOriginProgramFailure::WorkBudget { .. }
    ));
    assert!(denied.work <= budget.assembly.max_work);
    budget = polish_limits();
    budget.replay.max_drcs_references = 0;
    let error = compile_teletext_normalized_main_program(&service, owner(), budget).unwrap_err();
    // Original composition has no DRCS references; physical replay must charge
    // the newly derived reference rather than reuse the original empty count.
    assert!(matches!(error.reason, Failure::Compilation(_)), "{error:?}");
    assert_eq!(service, before);
}
