//! Independent physical words and original-source diagnostics for composition.
//! These selected lowering checks do not certify final object/display semantics.

use ersatzrs_domain::*;

use super::composed_main::*;
use super::main_composition::{
    TeletextBaseCellCommandKind, TeletextMainCompositionBudget, TeletextMainCompositionFailure,
    TeletextMainCompositionSource,
};
use super::{
    allocate_teletext_resource_objects, compile_teletext_local_program_with_objects,
    TeletextLocalObjectProgramEncodingFailure as PackingFailure,
    TeletextLocalProgramEncodingFailure as LoweringFailure, TeletextResourceObjectAllocations,
};

fn owner() -> TeletextPageAddress {
    TeletextPageAddress {
        page: TeletextPageNumber::new(200).unwrap(),
        subcode: TeletextSubcode::new(0x1234).unwrap(),
    }
}

fn position(row: u8, column: u8) -> TeletextEnhancementCommand {
    TeletextEnhancementCommand::SetPosition {
        position: TeletextEnhancementPosition::new(row, column).unwrap(),
    }
}

fn screen(value: u8) -> TeletextEnhancementCommand {
    TeletextEnhancementCommand::SetScreenColour {
        colour: TeletextColourReference::new(value).unwrap(),
    }
}

fn local() -> TeletextObjectIdentity {
    TeletextObjectIdentity::local(owner(), TeletextObjectId::new(0xffff))
}

fn public(page: u8, id: u16) -> TeletextObjectIdentity {
    TeletextObjectIdentity::public(
        TeletextResourceIdentity::new(
            TeletextResourceKind::Pop,
            TeletextRawDataPageNumber::new(2, page).unwrap(),
            TeletextResourceSubpageIndex::new(0).unwrap(),
        )
        .unwrap(),
        TeletextObjectId::new(id),
    )
    .unwrap()
}

fn invoke(target: TeletextObjectIdentity) -> TeletextEnhancementCommand {
    TeletextEnhancementCommand::InvokeObject {
        target,
        object_type: TeletextObjectType::Passive,
        offset: None,
    }
}

fn definition(identity: TeletextObjectIdentity) -> TeletextEnhancementObjectDefinition {
    TeletextEnhancementObjectDefinition {
        identity,
        object_type: TeletextObjectType::Passive,
        program: TeletextEnhancementProgram::new(vec![]).unwrap(),
    }
}

fn row(number: u8, column: u8) -> TeletextRow {
    TeletextRow {
        row: number,
        runs: vec![TeletextRun {
            column,
            style: TeletextStyle::default(),
            content: TeletextRunContent::Text { text: "£".into() },
        }],
    }
}

fn composition_budget() -> TeletextMainCompositionBudget {
    TeletextMainCompositionBudget {
        max_input_commands: 1024,
        max_output_commands: 1024,
        collection: TeletextEnhancementCollectionBudget {
            max_commands: 2048,
            max_drcs_references: 0,
        },
    }
}

fn packing_budget() -> TeletextObjectCatalogBudget {
    TeletextObjectCatalogBudget {
        max_objects: 4,
        max_commands: 1024,
    }
}

struct Fixture {
    rows: Vec<TeletextRow>,
    main: Option<Vec<TeletextEnhancementCommand>>,
    objects: Vec<TeletextEnhancementObjectDefinition>,
    selected: Option<TeletextRawDataPageNumber>,
    defaults: [Option<TeletextDefaultObjectSelection>; 4],
    presentation: Option<TeletextLevel25PresentationDefaults>,
}

impl Fixture {
    fn new(main: Vec<TeletextEnhancementCommand>) -> Self {
        Self {
            rows: vec![],
            main: Some(main),
            objects: vec![],
            selected: None,
            defaults: [None; 4],
            presentation: None,
        }
    }

    fn build(self) -> TeletextSemanticService {
        let mut base = super::transport_tests::draft(200, 0x1234, false);
        base.national_option = TeletextNationalOption::new(1).unwrap();
        base.rows = self.rows;
        TeletextSemanticService::new(
            TeletextResolvedServiceDraft {
                base: TeletextPageSetDraft {
                    language: base.language.clone(),
                    initial_page: owner().page,
                    pages: vec![base],
                },
                pages: vec![TeletextResolvedPageEnhancementDraft {
                    address: owner(),
                    presentation: self.presentation,
                    compositional_links: None,
                    local_program: self
                        .main
                        .map(|commands| TeletextEnhancementProgram::new(commands).unwrap()),
                    resources: TeletextResolvedPageResourceSelection {
                        objects: TeletextResolvedObjectPageLinks {
                            public: self.selected,
                            global: None,
                        },
                        drcs: TeletextDrcsPageLinks::unlinked(),
                        defaults: self.defaults,
                    },
                }],
                magazines: vec![],
                objects: self.objects,
                drcs: vec![],
            },
            TeletextServiceBudget {
                max_records: 32,
                max_objects: 4,
                max_structure_items: 4096,
                max_base_content_bytes: 1024,
                max_commands: 2048,
                max_collected_commands: 4096,
                max_collected_drcs_references: 0,
            },
        )
        .unwrap()
    }
}

fn allocations(service: &TeletextSemanticService) -> TeletextResourceObjectAllocations {
    allocate_teletext_resource_objects(service.objects(), &[], 4).unwrap()
}

fn compile(
    service: &TeletextSemanticService,
) -> Result<EncodedTeletextComposedMainProgram, TeletextComposedMainEncodingError> {
    compile_teletext_composed_main_program(
        service,
        owner(),
        &allocations(service),
        composition_budget(),
        packing_budget(),
    )
}

// Independent published Hamming data positions/parity, not encoder roundtrips.
fn words(bodies: &[[u8; 40]]) -> Vec<(u32, u32, u32)> {
    const HAMMING: [u8; 16] = [
        0x15, 0x02, 0x49, 0x5e, 0x64, 0x73, 0x38, 0x2f, 0xd0, 0xc7, 0x8c, 0x9b, 0xa1, 0xb6, 0xfd,
        0xea,
    ];
    bodies
        .iter()
        .enumerate()
        .flat_map(|(designation, body)| {
            assert_eq!(body[0], HAMMING[designation]);
            body[1..].chunks_exact(3).map(|bytes| {
                let raw = u32::from_le_bytes([bytes[0], bytes[1], bytes[2], 0]);
                assert_eq!(raw.count_ones() % 2, 1);
                for parity in [1_u32, 2, 4, 8, 16] {
                    let ones: u32 = (1_u32..24)
                        .filter(|position| position & parity != 0)
                        .map(|position| (raw >> (position - 1)) & 1)
                        .sum();
                    assert_eq!(ones % 2, 1);
                }
                let value = [
                    3, 5, 6, 7, 9, 10, 11, 12, 13, 14, 15, 17, 18, 19, 20, 21, 22, 23,
                ]
                .into_iter()
                .enumerate()
                .fold(0, |data, (bit, position)| {
                    data | (((raw >> (position - 1)) & 1) << bit)
                });
                (value & 63, (value >> 6) & 31, value >> 11)
            })
        })
        .collect()
}

fn invocation_fixture(padding: usize) -> TeletextSemanticService {
    invocation_at_row(padding, 2)
}

fn invocation_at_row(padding: usize, target_row: u8) -> TeletextSemanticService {
    let mut main = vec![screen(0); padding];
    main.extend([position(target_row, 8), invoke(local())]);
    let mut fixture = Fixture::new(main);
    fixture.rows = vec![row(target_row, 9)];
    fixture.objects = vec![definition(local())];
    fixture.build()
}

#[test]
fn high_local_pointer_sizing_and_emission_agree_for_rows_one_and_twenty_four() {
    // Marker104 is designation8/slot0 => N80. Its invocation address41
    // differs from placeholder N0/address40, exposing both opposite row cases.
    for (target_row, selector_address) in [(1, 41), (24, 40)] {
        let result = compile(&invocation_at_row(99, target_row)).unwrap();
        let encoded = result.encoded();
        let marker = encoded.definition_positions()[0];
        assert_eq!(encoded.main_triplet_count(), 103);
        assert_eq!(marker.absolute_index(), 104);
        assert_eq!(marker.object_number(), 0x80);
        let decoded = words(encoded.packet_bodies());
        assert_eq!(decoded[100], (41, 19, 0));
        assert_eq!(decoded[101], (selector_address, 4, 9));
        assert_eq!(decoded[104], (41, 23, 0));
        let number = ((decoded[100].0 & 1) << 7) | decoded[100].2;
        let absolute = (number >> 4) * 13 + (number & 15);
        assert_eq!(absolute, 104);
        assert_eq!(decoded[usize::try_from(absolute).unwrap()], (41, 23, 0));
    }
}

#[test]
fn high_pointer_compatibility_restoration_preserves_exact_capacity_in_both_row_cases() {
    for (target_row, selector_address) in [(1, 41), (24, 40)] {
        let exact = compile(&invocation_at_row(201, target_row)).unwrap();
        let encoded = exact.encoded();
        assert_eq!(encoded.packet_bodies().len(), 16);
        assert_eq!(encoded.main_triplet_count(), 205);
        assert_eq!(encoded.definition_positions()[0].absolute_index(), 206);
        assert_eq!(encoded.definition_positions()[0].object_number(), 0xfb);
        let decoded = words(encoded.packet_bodies());
        assert_eq!(decoded[202], (41, 19, 123));
        assert_eq!(decoded[203], (selector_address, 4, 9));
        assert_eq!(decoded[206], (41, 23, 123));
        assert_eq!(decoded[207], (63, 31, 5));
        let error = compile(&invocation_at_row(202, target_row)).unwrap_err();
        assert!(matches!(
            error,
            TeletextComposedMainEncodingError::Packing(TeletextComposedMainPackingError {
                reason: PackingFailure::Capacity {
                    count: 209,
                    maximum: 208
                },
                ..
            })
        ));
    }
}

#[test]
fn invocation_row_disturbance_restores_body_row_and_relocates_exact_local_pointer() {
    let result = compile(&invocation_fixture(0)).unwrap();
    let encoded = result.encoded();
    assert_eq!(encoded.main_triplet_count(), 4);
    assert_eq!(encoded.definition_positions()[0].target(), local());
    assert_eq!(encoded.definition_positions()[0].absolute_index(), 5);
    assert_eq!(encoded.definition_positions()[0].object_number(), 5);
    assert_eq!(
        &words(encoded.packet_bodies())[..7],
        &[
            (42, 4, 8),
            (40, 19, 5),
            (42, 4, 9),
            (9, 15, 0x23),
            (63, 31, 6),
            (40, 23, 5),
            (63, 31, 5)
        ]
    );
}

#[test]
fn extra_restoration_moves_definition_from_slot_twelve_to_next_designation() {
    for (padding, marker, number) in [(7, 12, 12), (8, 13, 16)] {
        let result = compile(&invocation_fixture(padding)).unwrap();
        let encoded = result.encoded();
        let position = encoded.definition_positions()[0];
        assert_eq!(position.absolute_index(), marker);
        assert_eq!(position.object_number(), number);
        let actual = words(encoded.packet_bodies());
        assert_eq!(actual[padding + 1], (40, 19, u32::from(number)));
        assert_eq!(actual[padding + 2], (42, 4, 9));
        assert_eq!(actual[usize::from(marker)], (40, 23, u32::from(number)));
    }
}

#[test]
fn final_capacity_counts_restoration_before_any_packet_bodies_are_returned() {
    let exact = compile(&invocation_fixture(201)).unwrap();
    assert_eq!(exact.encoded().packet_bodies().len(), 16);
    assert_eq!(exact.encoded().main_triplet_count(), 205);
    assert_eq!(
        exact.encoded().definition_positions()[0].absolute_index(),
        206
    );
    assert_eq!(words(exact.encoded().packet_bodies())[207], (63, 31, 5));
    let service = invocation_fixture(202);
    let before = service.clone();
    let error = compile(&service).unwrap_err();
    assert!(matches!(
        error,
        TeletextComposedMainEncodingError::Packing(TeletextComposedMainPackingError {
            reason: PackingFailure::Capacity {
                count: 209,
                maximum: 208
            },
            ..
        })
    ));
    assert_eq!(service, before);
    // Without the new compatibility selector, the identical semantic program
    // would fit. This is the additional physical word, not an input-work bound.
    let composition = super::main_composition::compose_teletext_main_program(
        &service,
        owner(),
        composition_budget(),
    )
    .unwrap();
    let legacy = compile_teletext_local_program_with_objects(
        owner(),
        composition.program(),
        service.objects(),
        &allocations(&service),
        TeletextResolvedObjectPageLinks {
            public: None,
            global: None,
        },
        packing_budget(),
    )
    .unwrap();
    assert_eq!(legacy.packet_bodies().len(), 16);
}

#[test]
fn explicit_header_position_error_retains_original_page_ordinal() {
    let service = Fixture::new(vec![position(0, 8)]).build();
    let before = service.clone();
    let error = compile(&service).unwrap_err();
    assert!(matches!(
        error,
        TeletextComposedMainEncodingError::Packing(TeletextComposedMainPackingError {
            location: Some(TeletextMainCompositionSource::OriginalPage { command: 0 }),
            reason: PackingFailure::Lowering(LoweringFailure::NeedsRowZeroLowering { .. }),
            ..
        })
    ));
    assert_eq!(service, before);
}

#[test]
fn header_attribute_then_call_maps_late_restoration_failure_to_derived_cell() {
    let mut fixture = Fixture::new(vec![
        TeletextEnhancementCommand::AtColumn {
            column: 8,
            operation: TeletextColumnOperation::Foreground {
                colour: TeletextColourReference::new(3).unwrap(),
            },
        },
        invoke(local()),
    ]);
    fixture.rows = vec![row(0, 9)];
    fixture.objects = vec![definition(local())];
    let service = fixture.build();
    let before = service.clone();
    let error = compile(&service).unwrap_err();
    assert!(matches!(error,TeletextComposedMainEncodingError::Packing(
        TeletextComposedMainPackingError {
            location:Some(TeletextMainCompositionSource::BaseCell {position,kind:TeletextBaseCellCommandKind::Glyph}),
            reason:PackingFailure::Lowering(LoweringFailure::NeedsCompatibilityRowLowering {position:(0,9)}),..
        }
    ) if position==TeletextEnhancementPosition::new(0,9).unwrap()));
    assert_eq!(service, before);
}

#[test]
fn initial_header_selector_uses_original_nonblack_row_default() {
    let mut fixture = Fixture::new(vec![]);
    fixture.rows = vec![row(0, 8)];
    let mut defaults =
        super::presentation::western_latin_v1_presentation(TeletextNationalOption::new(1).unwrap())
            .unwrap();
    defaults.screen_colour = TeletextColourReference::new(13).unwrap();
    defaults.row_colour = TeletextColourReference::new(6).unwrap();
    fixture.presentation = Some(defaults);
    let result = compile(&fixture.build()).unwrap();
    assert_eq!(
        result.composition().colour_context().row_colour(0),
        Some(TeletextColourReference::new(6).unwrap())
    );
    assert_eq!(
        &words(result.encoded().packet_bodies())[..3],
        &[(63, 7, 6), (8, 15, 0x23), (63, 31, 7)]
    );
}

#[test]
fn same_service_catalogue_and_links_are_used_without_accepting_foreign_allocations() {
    let target = public(0xa0, 0xffff);
    let mut fixture = Fixture::new(vec![position(1, 8), invoke(target)]);
    fixture.objects = vec![definition(target)];
    fixture.selected = Some(target.resource().unwrap().page());
    let service = fixture.build();
    let result = compile(&service).unwrap();
    assert_eq!(
        &words(result.encoded().packet_bodies())[..3],
        &[(41, 4, 8), (48, 19, 0), (63, 31, 7)]
    );
    let foreign = TeletextEnhancementObjectCatalog::new(
        vec![definition(public(0xa0, 0x0100))],
        packing_budget(),
    )
    .unwrap();
    let foreign = allocate_teletext_resource_objects(&foreign, &[], 4).unwrap();
    let before = service.clone();
    let error = compile_teletext_composed_main_program(
        &service,
        owner(),
        &foreign,
        composition_budget(),
        packing_budget(),
    )
    .unwrap_err();
    assert!(matches!(error,TeletextComposedMainEncodingError::Packing(
        TeletextComposedMainPackingError {
            location:Some(TeletextMainCompositionSource::OriginalPage {command:1}),
            reason:PackingFailure::Lowering(LoweringFailure::MissingObjectAllocation {target:actual,..}),..
        }
    ) if actual==target));
    assert_eq!(service, before);
}

#[test]
fn mot_default_keeps_original_owner_and_sparse_slot_when_selected_page_differs() {
    let target = public(0xa0, 0xffff);
    let selected = TeletextRawDataPageNumber::new(2, 0xa1).unwrap();
    let mut fixture = Fixture::new(vec![]);
    fixture.main = None;
    // Both real pages exist; this is a deliberate selected-link/MOT-owner
    // difference, not a missing resource that the semantic service rejects.
    fixture.objects = vec![definition(target), definition(public(0xa1, 0x0100))];
    fixture.selected = Some(selected);
    fixture.defaults[2] = Some(TeletextDefaultObjectSelection {
        target,
        object_type: TeletextObjectType::Passive,
    });
    let service = fixture.build();
    let before = service.clone();
    let error = compile(&service).unwrap_err();
    assert!(matches!(error,TeletextComposedMainEncodingError::Packing(
        TeletextComposedMainPackingError {
            location:Some(TeletextMainCompositionSource::DefaultSlot {slot:2}),
            reason:PackingFailure::Lowering(LoweringFailure::ObjectSelection {target:actual,selected:Some(page)}),..
        }
    ) if actual==target && page==selected));
    assert_eq!(service, before);
}

#[test]
fn composition_errors_remain_distinct_from_packing_and_preserve_original_input() {
    let mut fixture = Fixture::new(vec![]);
    fixture.rows = vec![row(1, 8)];
    let service = fixture.build();
    let before = service.clone();
    let mut budget = composition_budget();
    budget.max_input_commands = 0;
    let error = compile_teletext_composed_main_program(
        &service,
        owner(),
        &allocations(&service),
        budget,
        packing_budget(),
    )
    .unwrap_err();
    assert!(matches!(
        error,
        TeletextComposedMainEncodingError::Composition(
            super::main_composition::TeletextMainCompositionError {
                reason: TeletextMainCompositionFailure::InputBudget { maximum: 0 },
                ..
            }
        )
    ));
    assert_eq!(service, before);
}
