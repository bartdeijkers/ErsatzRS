//! Whole-service compiler ownership, distinct from the existing-bank-only primitive.

use super::polish_carrier::{
    assert_polish_original, enlarged_polish_base, polish_fallback, polish_limits, POLISH_5B_MASK,
};
use super::*;
use crate::teletext::authored_service_tests as authored;
use crate::teletext::{
    encode_teletext_authored_service, project_encoded_teletext_display,
    project_encoded_teletext_drcs, EncodedTeletextAuthoredPage, EncodedTeletextAuthoredService,
    TeletextAuthoredServiceEncodingBudget, TeletextEncodedDisplayRecord,
};

fn declared(seed: TeletextSemanticService) -> TeletextDeclaredService {
    let selections = seed
        .base()
        .pages
        .iter()
        .map(|page| TeletextDisplayMotSelection {
            display: page.address,
            mot: None,
        })
        .collect();
    TeletextDeclaredService::new(
        seed,
        vec![],
        selections,
        vec![],
        authored::declaration_limits(),
    )
    .unwrap()
}

fn service_budget() -> TeletextAuthoredServiceEncodingBudget {
    let mut budget = authored::limits();
    budget.normalization = polish_limits();
    budget
}

fn encode(input: &TeletextDeclaredService) -> EncodedTeletextAuthoredService {
    encode_teletext_authored_service(
        input.clone(),
        &authored::transmissions(input),
        &service_budget(),
    )
    .unwrap()
}

fn display(
    service: &EncodedTeletextAuthoredService,
) -> &crate::teletext::EncodedTeletextCompiledDisplayPage {
    let EncodedTeletextAuthoredPage::CompiledDisplay(display) = &service.pages()[0] else {
        panic!("overlapping physical Base must retain the full normalized display path");
    };
    display
}

fn rows(page: &crate::teletext::EncodedTeletextPage) -> Vec<TeletextEncodedDisplayRecord> {
    project_encoded_teletext_display(page, 128)
        .unwrap()
        .records
        .into_iter()
        .filter(|record| matches!(record, TeletextEncodedDisplayRecord::BaseRow { .. }))
        .collect()
}

#[test]
fn compiler_owned_polish_5b_resource_preserves_authenticated_original_and_fallback() {
    let seed = enlarged_polish_base(None, false);
    let fallback = polish_fallback(&seed);
    let input = declared(seed.clone());
    assert!(input.semantic().drcs().subtables().is_empty());
    assert!(input.semantic().page_enhancements()[&owner()]
        .compositional_links
        .is_none());
    assert!(input.resource_inventory().is_empty());
    let encoded = encode(&input);
    assert_eq!(encoded.declared(), &input);
    assert_eq!(encoded.compiler_drcs_resources().len(), 1);
    let resource = &encoded.compiler_drcs_resources()[0];
    let identity = resource.table().identity();
    assert_eq!(identity.kind(), TeletextResourceKind::Drcs);
    assert_eq!(identity.logical_subpage().get(), 0);
    assert_eq!(resource.consumers(), &[owner()]);
    assert_eq!(resource.table().glyphs().len(), 1);
    assert_eq!(resource.table().glyphs()[0].slot.get(), 0);
    assert_eq!(resource.table().glyphs()[0].bitmap.get(), POLISH_5B_MASK);
    let display = display(&encoded);
    assert_polish_original(display.normalized().original());
    let allocations = crate::teletext::allocate_teletext_resource_objects(
        seed.objects(),
        &[],
        polish_limits().max_resource_objects,
    )
    .unwrap();
    // The service factory compares a transmission-settled original. The plain
    // compose() helper is a preliminary model and is not an equivalent oracle.
    let original = crate::teletext::compile_teletext_composed_main_program(
        &seed,
        owner(),
        &allocations,
        polish_limits().composition,
        polish_limits().packing,
    )
    .unwrap();
    assert!(
        display.normalized().original() == original.composition(),
        "compiler resource derivation must preserve the independently settled authored original"
    );
    assert_eq!(rows(display.page()), rows(&fallback));
    let actual = display.normalized().physical().replay();
    let candidate = selected(actual.display_resolution(), 1, 7);
    let executed = actual
        .drcs_execution()
        .records()
        .iter()
        .find(|record| record.candidate_index() == candidate)
        .unwrap();
    assert_eq!(executed.resource(), identity);
    assert_eq!(executed.bitmap().get(), POLISH_5B_MASK);
    assert_eq!(encoded.pages().len(), 2);
    let physical = &encoded.pages()[1];
    let EncodedTeletextAuthoredPage::Resource(resource_page) = physical else {
        panic!("compiler DRCS must be a real resource page");
    };
    assert_eq!(
        resource_page.transmission_mode(),
        crate::teletext::TeletextResourceTransmissionMode::Serial
    );
    let header = physical.resource_header().unwrap();
    assert_eq!(header.identity(), identity);
    assert_eq!(header.update_counter().get(), 0);
    assert_eq!(
        header.transmission(),
        TeletextResourceTransmission::FullUnchangedFirst
    );
    let projection = project_encoded_teletext_drcs(physical, 32).unwrap();
    let cells = projection
        .records
        .iter()
        .find_map(|record| match record {
            TeletextEncodedDisplayRecord::BaseRow { row: 1, cells } => Some(cells),
            _ => None,
        })
        .unwrap();
    let bitmap = std::array::from_fn(|row| {
        (u16::from(cells[row * 2] & 0x3f) << 6) | u16::from(cells[row * 2 + 1] & 0x3f)
    });
    assert_eq!(bitmap, POLISH_5B_MASK);
    assert_eq!(&cells[20..], &[b' '; 20]);
    assert!(encoded.declared().semantic().drcs().subtables().is_empty());
    assert_eq!(seed, *encoded.declared().semantic());
}

fn compiled_units(service: &EncodedTeletextAuthoredService) -> Vec<Vec<[u8; 46]>> {
    service
        .pages()
        .iter()
        .map(|page| page.data_units().to_vec())
        .collect()
}

#[test]
fn compiler_resource_links_are_actual_x27_and_deterministic_with_bounded_lifecycle() {
    let input = declared(enlarged_polish_base(None, false));
    let encoded = encode(&input);
    let repeated = encode(&input);
    assert_eq!(compiled_units(&encoded), compiled_units(&repeated));
    assert_eq!(encoded.inventory(), repeated.inventory());
    let generated = &encoded.compiler_drcs_resources()[0];
    let identity = generated.table().identity();
    let link = generated
        .compositional_links(owner())
        .unwrap()
        .link(TeletextX27LinkRole::Drcs);
    assert_eq!(
        link.target(),
        TeletextX27LinkTarget::Page {
            page: identity.page()
        }
    );
    assert_eq!(link.required_subpages().get(), 1);
    let projection = project_encoded_teletext_display(display(&encoded).page(), 128).unwrap();
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
        .unwrap();
    // Independent table-15/16 extraction from actual X27/4, not a link encoder.
    assert_eq!(words[6] & 3, 3);
    assert_eq!((words[6] >> 6) & 15, u32::from(identity.page().page() & 15));
    assert_eq!(
        (words[6] >> 14) & 15,
        u32::from(identity.page().page() >> 4)
    );
    assert_eq!(
        (words[6] >> 11) & 7,
        u32::from(owner().page.wire_magazine() ^ identity.page().wire_magazine())
    );
    assert_eq!(words[7], 4);
    let with_lifecycle = encoded
        .with_resource_lifecycle_variants(Instant::now() + Duration::from_secs(10))
        .unwrap();
    let entries = with_lifecycle
        .resource_lifecycle_variants()
        .unwrap()
        .entries();
    assert_eq!(entries.len(), 1);
    assert_eq!(entries[0].resource(), identity);
    assert_eq!(entries[0].physical_page_index(), 1);
    assert_eq!(entries[0].headers().len(), 16);
    assert_eq!(with_lifecycle.declared(), &input);
    assert_eq!(compiled_units(&with_lifecycle), compiled_units(&repeated));
}

#[test]
fn compiler_owned_resource_counts_against_whole_service_page_and_unit_caps() {
    let input = declared(enlarged_polish_base(None, false));
    let successful = encode(&input);
    for (pages, units) in [
        (successful.pages().len() - 1, 1024),
        (32, successful.unit_count() - 1),
    ] {
        let mut budget = service_budget();
        budget.max_pages = pages;
        budget.max_units = units;
        let error = encode_teletext_authored_service(input.clone(), &[], &budget).unwrap_err();
        assert!(
            matches!(
                error.reason,
                crate::teletext::TeletextAuthoredServiceEncodingFailure::Budget { .. }
            ),
            "{error:?}"
        );
        assert!(input.semantic().drcs().subtables().is_empty());
        assert!(input.resource_inventory().is_empty());
    }
    assert_eq!(compiled_units(&encode(&input)), compiled_units(&successful));
}

fn rebuild(
    seed: &TeletextSemanticService,
    pages: Vec<TeletextResolvedPageEnhancementDraft>,
    drcs: Vec<TeletextDrcsSubtable>,
    objects: Vec<TeletextEnhancementObjectDefinition>,
) -> TeletextSemanticService {
    TeletextSemanticService::new(
        TeletextResolvedServiceDraft {
            base: seed.base().clone(),
            pages,
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
            objects,
            drcs,
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

fn occupied_seed(both: bool) -> TeletextSemanticService {
    let seed = enlarged_polish_base(None, false);
    let mut pages = seed
        .page_enhancements()
        .values()
        .cloned()
        .collect::<Vec<_>>();
    let normal = TeletextRawDataPageNumber::new(2, 0xa0).unwrap();
    let global = TeletextRawDataPageNumber::new(1, 0xa1).unwrap();
    let table = |kind, page| {
        TeletextDrcsSubtable::new(
            TeletextResourceIdentity::new(
                kind,
                page,
                TeletextResourceSubpageIndex::new(0).unwrap(),
            )
            .unwrap(),
            vec![TeletextDrcsGlyphDefinition {
                slot: TeletextDrcsGlyphSlot::new(0).unwrap(),
                bitmap: TeletextDrcsBitmap::new([0; 10]).unwrap(),
            }],
        )
        .unwrap()
    };
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
    pages[0].resources.drcs =
        TeletextDrcsPageLinks::new(Some(normal), both.then_some(global)).unwrap();
    pages[0].compositional_links = Some(
        TeletextX27Level25Links::new(
            unused,
            unused,
            if both { linked(global) } else { unused },
            linked(normal),
        )
        .unwrap(),
    );
    let mut tables = vec![table(TeletextResourceKind::Drcs, normal)];
    // An unlinked authored Global table still owns its raw physical address.
    // Choosing a different compiler Global bank proves namespace reservation.
    tables.push(table(TeletextResourceKind::Gdrcs, global));
    rebuild(&seed, pages, tables, seed.objects().definitions().to_vec())
}

#[test]
fn occupied_normal_role_uses_unused_global_without_aliasing_authored_resources() {
    let input = declared(occupied_seed(false));
    let encoded = encode(&input);
    assert_eq!(encoded.declared(), &input);
    assert_eq!(encoded.compiler_drcs_resources().len(), 1);
    let generated = &encoded.compiler_drcs_resources()[0];
    assert_eq!(
        generated.table().identity().kind(),
        TeletextResourceKind::Gdrcs
    );
    assert!(input
        .resource_inventory()
        .iter()
        .all(|identity| identity.page() != generated.table().identity().page()));
    let effective = generated.compositional_links(owner()).unwrap();
    let authored = input.semantic().page_enhancements()[&owner()]
        .compositional_links
        .unwrap();
    assert_eq!(
        effective.link(TeletextX27LinkRole::Drcs),
        authored.link(TeletextX27LinkRole::Drcs)
    );
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
    let generated_count = encoded
        .pages()
        .iter()
        .filter(|page| page.resource_header().is_some())
        .count();
    let with_lifecycle = encoded
        .with_resource_lifecycle_variants(Instant::now() + Duration::from_secs(10))
        .unwrap();
    assert_eq!(
        with_lifecycle
            .resource_lifecycle_variants()
            .unwrap()
            .entries()
            .len(),
        generated_count
    );
}

#[test]
fn occupied_normal_and_global_roles_leave_the_conversion_explicitly_rejected() {
    let input = declared(occupied_seed(true));
    let error = encode_teletext_authored_service(
        input.clone(),
        &authored::transmissions(&input),
        &service_budget(),
    )
    .unwrap_err();
    assert!(
        matches!(
            error.reason,
            crate::teletext::TeletextAuthoredServiceEncodingFailure::Compiled(_)
        ),
        "{error:?}"
    );
    assert_eq!(input.semantic().drcs().subtables().len(), 2);
    assert_eq!(input.resource_inventory().len(), 2);
}

#[test]
fn ordinary_nonoverlapping_polish_does_not_create_a_compiler_bank() {
    let seed = enlarged_polish_base(None, false);
    let mut pages = seed
        .page_enhancements()
        .values()
        .cloned()
        .collect::<Vec<_>>();
    pages[0].local_program = None;
    // No main/default invokes these Local definitions after this deliberate
    // fixture change. Clear them instead of bypassing LocalPresence validation.
    let input = declared(rebuild(&seed, pages, vec![], vec![]));
    let encoded = encode(&input);
    assert_eq!(encoded.declared(), &input);
    assert!(encoded.compiler_drcs_resources().is_empty());
    assert_eq!(encoded.pages().len(), 1);
    assert!(encoded
        .pages()
        .iter()
        .all(|page| page.resource_header().is_none()));
}

#[test]
fn expired_whole_service_deadline_cannot_acquire_a_compiler_owned_resource() {
    let input = declared(enlarged_polish_base(None, false));
    let original = input.clone();
    let error = crate::teletext::encode_teletext_authored_service_before(
        input.clone(),
        &[],
        &service_budget(),
        Instant::now().checked_sub(Duration::from_secs(1)).unwrap(),
    )
    .unwrap_err();
    assert!(matches!(
        error.reason,
        crate::teletext::TeletextAuthoredServiceEncodingFailure::TimedOut
    ));
    assert_eq!(input, original);
    assert!(input.resource_inventory().is_empty());
}

fn consumer_set(owners: &[TeletextPageAddress]) -> TeletextDeclaredService {
    let seed = enlarged_polish_base(None, false);
    let mut base = seed.base().clone();
    base.initial_page = owners[0].page;
    base.pages = owners
        .iter()
        .map(|&address| {
            let mut page = seed.base().pages[0].clone();
            page.address = address;
            page
        })
        .collect();
    let pages = owners
        .iter()
        .map(|&address| {
            let mut metadata = seed.page_enhancements()[&owner()].clone();
            metadata.address = address;
            metadata.local_program = metadata
                .local_program
                .as_ref()
                .map(|program| reowner(program, address));
            metadata
        })
        .collect();
    let objects = owners
        .iter()
        .flat_map(|&address| {
            seed.objects().definitions().iter().map(move |definition| {
                let mut definition = definition.clone();
                definition.identity =
                    TeletextObjectIdentity::local(address, definition.identity.object_id());
                definition.program = reowner(&definition.program, address);
                definition
            })
        })
        .collect();
    let semantic = TeletextSemanticService::new(
        TeletextResolvedServiceDraft {
            base,
            pages,
            magazines: vec![],
            objects,
            drcs: vec![],
        },
        authored::semantic_limits(),
    )
    .unwrap()
    .with_physical_base_overlays(
        owners
            .iter()
            .map(|&address| {
                TeletextPhysicalBaseOverlay::new(
                    address,
                    seed.physical_base_overlays()[&owner()].rows().to_vec(),
                )
                .unwrap()
            })
            .collect(),
        authored::semantic_limits(),
    )
    .unwrap();
    declared(semantic)
}

fn reowner(
    original: &TeletextEnhancementProgram,
    address: TeletextPageAddress,
) -> TeletextEnhancementProgram {
    let mut commands = original.commands().to_vec();
    for command in &mut commands {
        if let TeletextEnhancementCommand::InvokeObject { target, .. } = command {
            assert_eq!(target.owner(), Some(owner()));
            *target = TeletextObjectIdentity::local(address, target.object_id());
        }
    }
    program(commands)
}

#[test]
fn compiler_bank_identity_and_slot_survive_consumer_reordering_and_removal() {
    let second = TeletextPageAddress {
        page: TeletextPageNumber::new(201).unwrap(),
        subcode: owner().subcode,
    };
    let candidates = [
        encode(&consumer_set(&[owner(), second])),
        encode(&consumer_set(&[second, owner()])),
        encode(&consumer_set(&[second])),
    ];
    let table = candidates[0].compiler_drcs_resources()[0].table();
    assert_eq!(table.glyphs()[0].slot.get(), 0);
    for (index, candidate) in candidates.iter().enumerate() {
        assert_eq!(candidate.compiler_drcs_resources().len(), 1);
        let resource = &candidate.compiler_drcs_resources()[0];
        assert_eq!(resource.table(), table);
        let mut consumers = resource.consumers().to_vec();
        consumers.sort();
        assert_eq!(
            consumers,
            if index == 2 {
                vec![second]
            } else {
                vec![owner(), second]
            }
        );
        for consumer in consumers {
            assert!(resource.compositional_links(consumer).is_some());
        }
        let physical = candidate
            .pages()
            .iter()
            .find(|page| {
                page.resource_header()
                    .is_some_and(|header| header.identity() == table.identity())
            })
            .unwrap();
        let original = candidates[0]
            .pages()
            .iter()
            .find(|page| {
                page.resource_header()
                    .is_some_and(|header| header.identity() == table.identity())
            })
            .unwrap();
        assert_eq!(physical.data_units(), original.data_units());
    }
}

#[test]
fn compiler_resource_prepass_keeps_one_shared_assembly_quota_through_failure() {
    use crate::teletext::encode_teletext_authored_service_with_work_usage;
    let second = TeletextPageAddress {
        page: TeletextPageNumber::new(201).unwrap(),
        subcode: owner().subcode,
    };
    let input = consumer_set(&[owner(), second]);
    assert!(input.resource_inventory().is_empty());
    let mut budget = service_budget();
    budget.max_assembly_work = Some(0);
    let (result, used) =
        encode_teletext_authored_service_with_work_usage(input.clone(), &[], &budget, None);
    assert!(result.is_err());
    assert_eq!(used.assembly_work, Some(0));
    budget.max_assembly_work = Some(usize::MAX);
    let (result, admitted) =
        encode_teletext_authored_service_with_work_usage(input.clone(), &[], &budget, None);
    let expected = result.unwrap();
    let total = admitted.assembly_work.unwrap();
    assert!(total > 0);
    budget.max_assembly_work = Some(total);
    let (exact, exact_usage) =
        encode_teletext_authored_service_with_work_usage(input.clone(), &[], &budget, None);
    assert_eq!(compiled_units(&exact.unwrap()), compiled_units(&expected));
    assert_eq!(exact_usage.assembly_work, Some(total));
    budget.max_assembly_work = Some(total - 1);
    let (result, denied) =
        encode_teletext_authored_service_with_work_usage(input.clone(), &[], &budget, None);
    assert!(
        result.is_err(),
        "compiler preparation and later display work must not renew the shared quota"
    );
    let retained = denied.assembly_work.unwrap();
    assert!(
        retained > 0,
        "earlier admitted preparation/display work must not be refunded"
    );
    assert!(retained < total);
    assert_eq!(expected.declared(), &input);
}

#[test]
fn unselected_authored_x27_target_reserves_its_raw_address_without_a_resource_body() {
    let seed = enlarged_polish_base(None, false);
    let mut pages = seed
        .page_enhancements()
        .values()
        .cloned()
        .collect::<Vec<_>>();
    let named = TeletextRawDataPageNumber::new(1, 0xa0).unwrap();
    let unused = TeletextX27LinkDeclaration::new(
        TeletextX27LinkTarget::Unspecified {
            magazine: TeletextX27Magazine::new(2).unwrap(),
        },
        TeletextX27LinkValidity::Level25Only,
        TeletextSubpageRequirementMask::new(0xffff),
    )
    .unwrap();
    let unselected = TeletextX27LinkDeclaration::new(
        TeletextX27LinkTarget::Page { page: named },
        TeletextX27LinkValidity::Level35Only,
        TeletextSubpageRequirementMask::new(0),
    )
    .unwrap();
    pages[0].compositional_links =
        Some(TeletextX27Level25Links::new(unused, unused, unselected, unused).unwrap());
    let input = declared(rebuild(
        &seed,
        pages,
        vec![],
        seed.objects().definitions().to_vec(),
    ));
    assert!(input.resource_inventory().is_empty());
    let encoded = encode(&input);
    assert_eq!(encoded.declared(), &input);
    assert_eq!(encoded.compiler_drcs_resources().len(), 1);
    let resource = &encoded.compiler_drcs_resources()[0];
    assert_eq!(
        resource.table().identity().kind(),
        TeletextResourceKind::Drcs
    );
    assert_ne!(resource.table().identity().page(), named);
    assert_eq!(
        resource
            .compositional_links(owner())
            .unwrap()
            .link(TeletextX27LinkRole::Gdrcs),
        unselected
    );
}
