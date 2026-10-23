//! Atomic same-service displays, resources, lifecycle and actual header inventory.

use super::authored_service::TeletextAuthoredServiceEncodingFailure as Failure;
use super::authored_service::*;
use super::*;
use ersatzrs_domain::*;

fn owner(subcode: u16) -> TeletextPageAddress {
    TeletextPageAddress {
        page: TeletextPageNumber::new(200).unwrap(),
        subcode: TeletextSubcode::new(subcode).unwrap(),
    }
}
fn resource(kind: TeletextResourceKind, byte: u8, subpage: u8) -> TeletextResourceIdentity {
    TeletextResourceIdentity::new(
        kind,
        TeletextRawDataPageNumber::new(2, byte).unwrap(),
        TeletextResourceSubpageIndex::new(subpage).unwrap(),
    )
    .unwrap()
}
fn target(id: u16) -> TeletextObjectIdentity {
    TeletextObjectIdentity::public(
        resource(TeletextResourceKind::Pop, 0xa0, 0),
        TeletextObjectId::new(id),
    )
    .unwrap()
}
fn rows(text: &str) -> Vec<TeletextRow> {
    vec![TeletextRow {
        row: 1,
        runs: vec![TeletextRun {
            column: 8,
            style: TeletextStyle::default(),
            content: TeletextRunContent::Text { text: text.into() },
        }],
    }]
}
fn object(id: u16, code: u8) -> TeletextEnhancementObjectDefinition {
    TeletextEnhancementObjectDefinition {
        identity: target(id),
        object_type: TeletextObjectType::Passive,
        program: TeletextEnhancementProgram::new(vec![
            TeletextEnhancementCommand::SetPosition {
                position: TeletextEnhancementPosition::new(1, 8).unwrap(),
            },
            TeletextEnhancementCommand::AtColumn {
                column: 8,
                operation: TeletextColumnOperation::G0 {
                    code: TeletextGlyphCode::new(code).unwrap(),
                },
            },
        ])
        .unwrap(),
    }
}
fn empty_link() -> TeletextMotObjectLink {
    TeletextMotObjectLink::new(
        TeletextX27LinkTarget::Unspecified {
            magazine: TeletextX27Magazine::new(2).unwrap(),
        },
        TeletextMotSubpageExtent::new(0).unwrap(),
        true,
        TeletextMotFallback::new(0).unwrap(),
        [TeletextMotDefaultObject::NoObject { raw_number: 77 }; 2],
    )
}
fn mot() -> TeletextMotLevel25Declaration {
    mot_with_type(TeletextObjectType::Passive)
}
fn mot_with_type(object_type: TeletextObjectType) -> TeletextMotLevel25Declaration {
    let mut public = [empty_link(); 7];
    public[0] = TeletextMotObjectLink::new(
        TeletextX27LinkTarget::Page {
            page: resource(TeletextResourceKind::Pop, 0xa0, 0).page(),
        },
        TeletextMotSubpageExtent::new(1).unwrap(),
        true,
        TeletextMotFallback::new(0).unwrap(),
        [
            TeletextMotDefaultObject::Object {
                target: target(20),
                object_type,
            },
            TeletextMotDefaultObject::NoObject { raw_number: 77 },
        ],
    );
    let drcs = TeletextMotDrcsLink::new(
        TeletextX27LinkTarget::Unspecified {
            magazine: TeletextX27Magazine::new(2).unwrap(),
        },
        TeletextMotSubpageExtent::new(0).unwrap(),
        true,
    );
    TeletextMotLevel25Declaration::new(
        resource(TeletextResourceKind::Mot, 0xfe, 9),
        vec![TeletextMotPageAssociation::new(
            0,
            TeletextMotAssociationSelection::new(
                false,
                Some(TeletextMotPublicLinkIndex::new(1).unwrap()),
            ),
            TeletextMotAssociationSelection::new(false, None),
        )],
        TeletextMotLevel25Links::new(
            empty_link(),
            public,
            drcs,
            [drcs; 7],
            TeletextMotEnhancementCounts::new(0, 0, 0),
        )
        .unwrap(),
    )
    .unwrap()
}
pub(super) fn input() -> TeletextResolvedServiceDraft {
    let language = TeletextLanguage::new("nld".into()).unwrap();
    let mut declaration =
        super::presentation::western_latin_v1_presentation(TeletextNationalOption::new(1).unwrap())
            .unwrap();
    declaration.palette.clut2[0] = TeletextRgb::new([1, 2, 3]).unwrap();
    let base = [0, 1]
        .map(|subcode| TeletextPageDraft {
            address: owner(subcode),
            kind: TeletextPageKind::FullPage,
            language: language.clone(),
            national_option: TeletextNationalOption::new(1).unwrap(),
            rows: rows("ZZ"),
            navigation: vec![],
        })
        .to_vec();
    let pages = [0, 1]
        .map(|subcode| TeletextResolvedPageEnhancementDraft {
            address: owner(subcode),
            presentation: None,
            compositional_links: None,
            local_program: (subcode == 0).then(|| TeletextEnhancementProgram::new(vec![]).unwrap()),
            resources: TeletextResolvedPageResourceSelection {
                objects: TeletextResolvedObjectPageLinks {
                    public: Some(resource(TeletextResourceKind::Pop, 0xa0, 0).page()),
                    global: None,
                },
                drcs: TeletextDrcsPageLinks::unlinked(),
                defaults: [
                    None,
                    None,
                    Some(TeletextDefaultObjectSelection {
                        target: target(20),
                        object_type: TeletextObjectType::Passive,
                    }),
                    None,
                ],
            },
        })
        .to_vec();
    TeletextResolvedServiceDraft {
        base: TeletextPageSetDraft {
            language,
            initial_page: owner(0).page,
            pages: base,
        },
        pages,
        magazines: vec![TeletextMagazinePresentationDraft {
            magazine: 2,
            presentation: declaration,
        }],
        // Authored order differs from the active MOT reservation order.
        objects: vec![object(10, b'B'), object(20, b'A')],
        drcs: vec![TeletextDrcsSubtable::new(
            resource(TeletextResourceKind::Drcs, 0xc0, 0),
            vec![],
        )
        .unwrap()],
    }
}
pub(super) fn declared(input: TeletextResolvedServiceDraft) -> TeletextDeclaredService {
    let companions = vec![resource(TeletextResourceKind::Pop, 0xa0, 1)];
    let service = TeletextSemanticService::new_with_object_companions(
        input,
        companions.clone(),
        semantic_limits(),
    )
    .unwrap();
    TeletextDeclaredService::new(
        service,
        vec![mot()],
        [0, 1]
            .map(|subcode| TeletextDisplayMotSelection {
                display: owner(subcode),
                mot: Some(resource(TeletextResourceKind::Mot, 0xfe, 9)),
            })
            .to_vec(),
        companions,
        declaration_limits(),
    )
    .unwrap()
}
pub(super) fn transmissions(
    declared: &TeletextDeclaredService,
) -> Vec<TeletextServiceResourceTransmission> {
    declared
        .resource_inventory()
        .iter()
        .map(|&resource| TeletextServiceResourceTransmission {
            resource,
            counter: TeletextResourceUpdateCounter::new(1).unwrap(),
            update: TeletextResourceFullUpdate::Changed,
            last_subpage: true,
        })
        .collect()
}

const HAM: [u8; 16] = [
    0x15, 0x02, 0x49, 0x5e, 0x64, 0x73, 0x38, 0x2f, 0xd0, 0xc7, 0x8c, 0x9b, 0xa1, 0xb6, 0xfd, 0xea,
];
fn nibble(byte: u8) -> u8 {
    u8::try_from(HAM.iter().position(|&value| value == byte).unwrap()).unwrap()
}
fn packet(unit: &[u8; 46]) -> u8 {
    (nibble(unit[4].reverse_bits()) | (nibble(unit[5].reverse_bits()) << 4)) >> 3
}
fn body(unit: &[u8; 46]) -> [u8; 40] {
    std::array::from_fn(|index| unit[index + 6].reverse_bits())
}

#[test]
fn complete_candidate_keeps_native_and_present_empty_x26_paths_and_one_magazine_unit() {
    let original = declared(input());
    let expected = original.clone();
    let tx = transmissions(&original);
    let result = encode_teletext_authored_service(original, &tx, &limits()).unwrap();
    assert_eq!(result.declared(), &expected);
    assert_eq!(result.pages().len(), 6);
    assert_eq!(result.inventory().len(), 6);
    assert_eq!(result.magazine_units().len(), 1);
    assert_eq!(packet(&result.magazine_units()[0]), 29);
    assert_eq!(
        result.unit_count(),
        1 + result
            .pages()
            .iter()
            .map(|page| page.data_units().len())
            .sum::<usize>()
    );
    let EncodedTeletextAuthoredPage::CompiledDisplay(first) = &result.pages()[0] else {
        panic!("present empty Local must stay on X26 path");
    };
    assert_eq!(first.page().address(), owner(0));
    assert!(first
        .page()
        .data_units()
        .iter()
        .any(|unit| packet(unit) == 26));
    assert!(!first
        .page()
        .data_units()
        .iter()
        .any(|unit| packet(unit) == 28));
    let EncodedTeletextAuthoredPage::NativeDisplay(second) = &result.pages()[1] else {
        panic!("absent Local with no derived glyph must stay native");
    };
    assert_eq!(second.encoded().page().address(), owner(1));
    assert_eq!(second.composition().owner(), owner(1));
    assert!(!second
        .encoded()
        .page()
        .data_units()
        .iter()
        .any(|unit| packet(unit) == 26));
    assert_eq!(second.encoded().defaults()[2].unwrap().target, target(20));
    let display = second.composition().display_resolution();
    let selected = display.cell(1, 8).unwrap().origin().unwrap();
    let candidate = display.origins()[selected].candidate_index();
    assert_eq!(
        second.composition().glyph_layers().candidates()[candidate].source(),
        Some(TeletextEnhancementSource {
            object: Some(target(20)),
            command: 1
        })
    );
    for (page, record) in result.pages().iter().zip(result.inventory()) {
        assert_eq!(page.inventory_record(), *record);
        if let Some(header) = page.resource_header() {
            assert_eq!(header.update_counter().get(), 1);
            assert_eq!(nibble(body(&page.data_units()[0])[7]) & 1, 1);
        }
    }
}

#[test]
fn actual_object_numbers_reserve_mot_defaults_and_encode_required_empty_companion() {
    let original = declared(input());
    let tx = transmissions(&original);
    let result = encode_teletext_authored_service(original, &tx, &limits()).unwrap();
    let full = result
        .pages()
        .iter()
        .find_map(|page| match page {
            EncodedTeletextAuthoredPage::Object(page)
                if page.header().identity() == resource(TeletextResourceKind::Pop, 0xa0, 0) =>
            {
                Some(page)
            }
            _ => None,
        })
        .unwrap();
    assert_eq!(full.definition_positions().len(), 2);
    assert_eq!(full.definition_positions()[0].target(), target(20));
    assert_eq!(full.definition_positions()[0].object_number(), 0);
    assert_eq!(full.definition_positions()[1].target(), target(10));
    assert_eq!(full.definition_positions()[1].object_number(), 16);
    let empty = result
        .pages()
        .iter()
        .find_map(|page| match page {
            EncodedTeletextAuthoredPage::Object(page)
                if page.header().identity() == resource(TeletextResourceKind::Pop, 0xa0, 1) =>
            {
                Some(page)
            }
            _ => None,
        })
        .unwrap();
    assert!(empty.definition_positions().is_empty());
    assert_eq!(
        empty.data_units().iter().map(packet).collect::<Vec<_>>(),
        [0, 28, 1, 2, 3]
    );
    assert_eq!(empty.header().last_packet().get(), 3);
    assert_eq!(empty.header().physical_subcode(), 0x0311);
    let mot = result
        .pages()
        .iter()
        .find(|page| {
            page.resource_header()
                .is_some_and(|header| header.identity().kind() == TeletextResourceKind::Mot)
        })
        .unwrap();
    assert_eq!(mot.data_units().len(), 19);
    assert_eq!(mot.resource_header().unwrap().last_packet().get(), 21);
    assert_eq!(mot.resource_header().unwrap().physical_subcode(), 0x1519);
    let drcs = result
        .pages()
        .iter()
        .find(|page| {
            page.resource_header()
                .is_some_and(|header| header.identity().kind() == TeletextResourceKind::Drcs)
        })
        .unwrap();
    assert_eq!(
        drcs.data_units().iter().map(packet).collect::<Vec<_>>(),
        [0, 28, 1]
    );
    assert_eq!(drcs.resource_header().unwrap().last_packet().get(), 1);
    assert_eq!(drcs.resource_header().unwrap().physical_subcode(), 0x0110);
}

#[test]
fn lifecycle_missing_duplicate_and_unexpected_are_rejected_before_encoding() {
    let original = declared(input());
    let before = original.clone();
    let tx = transmissions(&original);
    let missing =
        encode_teletext_authored_service(original.clone(), &tx[1..], &limits()).unwrap_err();
    assert_eq!(missing.resource, Some(tx[0].resource));
    assert!(matches!(missing.reason, Failure::MissingTransmission));
    let duplicate =
        encode_teletext_authored_service(original.clone(), &[tx[0], tx[0]], &limits()).unwrap_err();
    assert_eq!(duplicate.resource, Some(tx[0].resource));
    assert!(matches!(
        duplicate.reason,
        Failure::DuplicateTransmission {
            first: 0,
            second: 1
        }
    ));
    let extra = TeletextServiceResourceTransmission {
        resource: resource(TeletextResourceKind::Pop, 0xa4, 0),
        ..tx[0]
    };
    let unexpected =
        encode_teletext_authored_service(original.clone(), &[extra], &limits()).unwrap_err();
    assert_eq!(unexpected.resource, Some(extra.resource));
    assert!(matches!(
        unexpected.reason,
        Failure::UnexpectedTransmission { ordinal: 0 }
    ));
    assert_eq!(original, before);
}

#[test]
fn raw_page_and_command_bounds_precede_lifecycle_and_late_unit_cutoff_is_atomic() {
    let original = declared(input());
    let before = original.clone();
    let mut bound = limits();
    bound.max_pages = 5;
    let error = encode_teletext_authored_service(original.clone(), &[], &bound).unwrap_err();
    assert!(matches!(
        error.reason,
        Failure::Budget {
            category: "pages",
            count: 6,
            maximum: 5
        }
    ));
    bound = limits();
    bound.max_commands = 3;
    let error = encode_teletext_authored_service(original.clone(), &[], &bound).unwrap_err();
    assert!(matches!(
        error.reason,
        Failure::Budget {
            category: "commands",
            count: 4,
            maximum: 3
        }
    ));
    let tx = transmissions(&original);
    let result = encode_teletext_authored_service(original.clone(), &tx, &limits()).unwrap();
    bound = limits();
    bound.max_units = result.unit_count() - 1;
    let error = encode_teletext_authored_service(original.clone(), &tx, &bound).unwrap_err();
    assert!(matches!(
        error.reason,
        Failure::Budget {
            category: "units",
            ..
        }
    ));
    assert_eq!(
        error.resource,
        result
            .pages()
            .last()
            .unwrap()
            .resource_header()
            .map(TeletextResourceHeaderMetadata::identity)
    );
    assert_eq!(original, before);
}

#[test]
fn known_magazine_unit_bound_precedes_lifecycle_index_allocation() {
    let original = declared(input());
    let before = original.clone();
    let mut bounded = limits();
    bounded.max_units = 0;
    let error = encode_teletext_authored_service(original.clone(), &[], &bounded).unwrap_err();
    assert_eq!(error.display, None);
    assert_eq!(error.resource, None);
    assert!(matches!(
        error.reason,
        Failure::Budget {
            category: "units",
            count: 1,
            maximum: 0,
        }
    ));
    assert_eq!(original, before);
}

#[test]
fn derived_base_requires_normalization_and_late_failure_does_not_fallback_to_native() {
    let mut input = input();
    input.pages[1].local_program = None;
    input.base.pages[1].rows = rows("£Z");
    let original = declared(input);
    let before = original.clone();
    let tx = transmissions(&original);
    let result = encode_teletext_authored_service(original.clone(), &tx, &limits()).unwrap();
    assert!(matches!(
        result.pages()[1],
        EncodedTeletextAuthoredPage::CompiledDisplay(_)
    ));
    // The first display is legal empty-X26. The second requires derived Base
    // replacement and therefore cannot silently switch to the native encoder.
    let mut bounded = limits();
    bounded.normalization.composition.max_input_commands = 1;
    let error = encode_teletext_authored_service(original.clone(), &tx, &bounded).unwrap_err();
    assert_eq!(error.display, Some(owner(1)));
    assert!(matches!(error.reason, Failure::Compiled(_)));
    assert_eq!(original, before);
}

#[test]
fn final_actual_header_collision_discards_the_whole_candidate() {
    let mut draft = input();
    draft.base.pages.truncate(1);
    draft.pages.truncate(1);
    let address = owner(0x0320);
    draft.base.pages[0].address = address;
    draft.pages[0].address = address;
    let resource = resource(TeletextResourceKind::Pop, 0x00, 0);
    let target = TeletextObjectIdentity::public(resource, TeletextObjectId::new(10)).unwrap();
    draft.objects.truncate(1);
    draft.objects[0].identity = target;
    draft.drcs.clear();
    draft.pages[0].resources.defaults = [None; 4];
    draft.pages[0].resources.objects.public = Some(resource.page());
    let unused = TeletextX27LinkDeclaration::new(
        TeletextX27LinkTarget::Unspecified {
            magazine: TeletextX27Magazine::new(2).unwrap(),
        },
        TeletextX27LinkValidity::Level35Only,
        TeletextSubpageRequirementMask::new(0xffff),
    )
    .unwrap();
    let pop = TeletextX27LinkDeclaration::new(
        TeletextX27LinkTarget::Page {
            page: resource.page(),
        },
        TeletextX27LinkValidity::Level25Only,
        TeletextSubpageRequirementMask::new(1),
    )
    .unwrap();
    draft.pages[0].compositional_links =
        Some(TeletextX27Level25Links::new(unused, pop, unused, unused).unwrap());
    let semantic = TeletextSemanticService::new(draft, semantic_limits()).unwrap();
    let original = TeletextDeclaredService::new(
        semantic,
        vec![],
        vec![TeletextDisplayMotSelection {
            display: address,
            mot: None,
        }],
        vec![],
        declaration_limits(),
    )
    .unwrap();
    let before = original.clone();
    let tx = [TeletextServiceResourceTransmission {
        resource,
        counter: TeletextResourceUpdateCounter::new(2).unwrap(),
        update: TeletextResourceFullUpdate::Changed,
        last_subpage: true,
    }];
    // The POP has actual final row3/counter2/S1=0: exact physical 0x0320,
    // colliding with the authored display despite distinct logical page roles.
    let error = encode_teletext_authored_service(original.clone(), &tx, &limits()).unwrap_err();
    assert!(matches!(error.reason, Failure::Inventory(error)
    if *error == TeletextPhysicalPageInventoryError::PhysicalCollision {
        first: 0, second: 1, magazine: 2, page: 0x00, subcode: 0x0320,
    }));
    assert_eq!(original, before);
}

pub(super) fn semantic_limits() -> TeletextServiceBudget {
    TeletextServiceBudget {
        max_records: 64,
        max_objects: 32,
        max_structure_items: 4096,
        max_base_content_bytes: 8192,
        max_commands: 8192,
        max_collected_commands: 16384,
        max_collected_drcs_references: 0,
    }
}
pub(super) fn declaration_limits() -> TeletextDeclaredServiceBudget {
    TeletextDeclaredServiceBudget {
        max_mots: 8,
        max_displays: 8,
        max_resources: 16,
        max_defaults: 64,
        max_objects: 32,
    }
}

fn normalization_limits() -> TeletextNormalizedMainProgramBudget {
    let packing = TeletextObjectCatalogBudget {
        max_objects: 64,
        max_commands: 8192,
    };
    let collection = TeletextEnhancementCollectionBudget {
        max_commands: 8192,
        max_drcs_references: 0,
    };
    TeletextNormalizedMainProgramBudget {
        composition: TeletextMainCompositionBudget {
            max_input_commands: 4096,
            max_output_commands: 4096,
            collection,
        },
        realization: TeletextBaseCompatibilityRealizationBudget {
            max_commands: 4096,
            max_candidates: 4096,
            max_origins: 8192,
            max_seeds: 64,
            max_edges: 4096,
            max_updates: 100_000,
        },
        placement: TeletextOriginCarrierPlacementBudget {
            max_commands: 4096,
            max_origins: 8192,
            max_edges: 4096,
            catalog: packing,
            spatial: TeletextSpatialTraceBudget {
                max_events: 8192,
                max_invocations: 512,
            },
            max_work: 100_000,
        },
        assembly: TeletextAugmentedOriginProgramBudget {
            max_input_commands: 4096,
            max_output_commands: 4096,
            max_carriers: 64,
            max_identity_checks: 100_000,
            max_work: 100_000,
            recipe: TeletextOriginCarrierBudget {
                max_candidates: 4096,
                max_origins: 8192,
                catalog: packing,
                max_id_checks: 65_600,
                max_output_commands: 6,
            },
        },
        packing,
        replay: collection,
        ancestry: TeletextAugmentedInvocationBudget {
            max_original_invocations: 256,
            max_actual_invocations: 512,
            max_carriers: 64,
            max_work: 100_000,
        },
        glyph_comparison: TeletextAugmentedGlyphComparisonBudget {
            max_records: 100_000,
            max_cells: 1800,
            max_diagnostics: 4096,
            max_work: 1_000_000,
        },
        display_comparison: TeletextPhysicalComparisonBudget {
            max_records: 100_000,
            max_cells: 1800,
            max_diagnostics: 32_000,
            max_updates: 1_000_000,
        },
        max_resource_objects: 64,
    }
}

pub(super) fn limits() -> TeletextAuthoredServiceEncodingBudget {
    TeletextAuthoredServiceEncodingBudget {
        max_pages: 32,
        max_units: 1024,
        max_commands: 8192,
        max_display_updates: None,
        max_glyph_comparison_work: None,
        max_display_comparison_updates: None,
        max_realization_updates: None,
        max_placement_work: None,
        max_assembly_work: None,
        max_ancestry_work: None,
        max_collected_commands: None,
        max_collected_drcs_references: None,
        max_spatial_event_visits: None,
        max_spatial_invocation_visits: None,
        max_identity_checks: None,
        max_attribute_cell_updates: None,
        normalization: normalization_limits(),
    }
}

pub(super) fn candidate() -> EncodedTeletextAuthoredService {
    let declared = declared(input());
    let tx = transmissions(&declared);
    encode_teletext_authored_service(declared, &tx, &limits()).unwrap()
}

#[path = "authored_service_transmission_tests.rs"]
mod transmission_tests;

#[path = "authored_service_charset_tests.rs"]
mod charset_tests;

#[path = "authored_service_deadline_tests.rs"]
mod deadline_tests;

#[path = "authored_service_update_quota_tests.rs"]
mod update_quota_tests;

#[path = "authored_service_comparison_quota_tests.rs"]
mod comparison_quota_tests;

#[path = "authored_service_planning_quota_tests.rs"]
mod planning_quota_tests;

#[path = "authored_service_traversal_quota_tests.rs"]
mod traversal_quota_tests;

#[path = "authored_service_attribute_quota_tests.rs"]
mod attribute_quota_tests;

#[path = "authored_service_tests/foreign_defaults.rs"]
mod foreign_defaults;

#[path = "authored_service_tests/standalone_base.rs"]
mod standalone_base;

#[path = "authored_service_tests/standalone_national.rs"]
mod standalone_national;

#[path = "authored_service_tests/object_page_selection.rs"]
mod object_page_selection;

#[path = "authored_service_tests/resource_contention.rs"]
mod resource_contention;

#[path = "authored_service_tests/empty_admission.rs"]
mod empty_admission;

#[path = "authored_service_tests/successive_ancestry.rs"]
mod successive_ancestry;
