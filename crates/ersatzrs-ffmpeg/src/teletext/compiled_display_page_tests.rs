//! Checked X26, actual metadata and same-service resource reservations.

use super::compiled_display_page::*;
use super::*;
use ersatzrs_domain::*;
use std::time::{Duration, Instant};

#[test]
fn expired_compiled_display_deadline_returns_no_units_before_normalization() {
    let service = service(draft());
    let before = service.clone();
    let mut budget = limits();
    budget.composition.max_input_commands = 0;
    let expired = Instant::now().checked_sub(Duration::from_secs(1)).unwrap();
    let error = encode_teletext_compiled_display_page_before(
        &service,
        owner(),
        TeletextEncodingRegion::WesternLatin,
        &[],
        &budget,
        None,
        Some(expired),
    )
    .unwrap_err();
    assert_eq!(error.owner, owner());
    assert!(matches!(
        error.reason,
        TeletextCompiledDisplayPageEncodingFailure::TimedOut
    ));
    assert_eq!(service, before);
}

#[test]
fn future_compiled_display_deadline_preserves_packets_magazines_and_models() {
    let service = service(draft());
    let before = service.clone();
    let expected = encode(&service);
    let deadline = Instant::now() + Duration::from_mins(1);
    let actual = encode_teletext_compiled_display_page_before(
        &service,
        owner(),
        TeletextEncodingRegion::WesternLatin,
        &[],
        &limits(),
        None,
        Some(deadline),
    )
    .unwrap();
    assert_eq!(actual.page(), expected.page());
    assert_eq!(actual.magazine_units(), expected.magazine_units());
    assert_eq!(
        actual.normalized().original(),
        expected.normalized().original()
    );
    assert_eq!(
        actual.normalized().physical(),
        expected.normalized().physical()
    );
    assert_eq!(service, before);
}

#[test]
fn future_compiled_display_deadline_retains_existing_normalization_failure() {
    let service = service(draft());
    let mut budget = limits();
    budget.composition.max_input_commands = 0;
    let deadline = Instant::now() + Duration::from_mins(1);
    let error = encode_teletext_compiled_display_page_before(
        &service,
        owner(),
        TeletextEncodingRegion::WesternLatin,
        &[],
        &budget,
        None,
        Some(deadline),
    )
    .unwrap_err();
    assert_eq!(error.owner, owner());
    assert!(matches!(
        error.reason,
        TeletextCompiledDisplayPageEncodingFailure::Normalization(_)
    ));
}

fn owner() -> TeletextPageAddress {
    TeletextPageAddress {
        page: TeletextPageNumber::new(200).unwrap(),
        subcode: TeletextSubcode::new(0x1234).unwrap(),
    }
}
fn row(number: u8, column: u8, text: &str) -> TeletextRow {
    TeletextRow {
        row: number,
        runs: vec![TeletextRun {
            column,
            style: TeletextStyle::default(),
            content: TeletextRunContent::Text { text: text.into() },
        }],
    }
}
fn position(row: u8, column: u8) -> TeletextEnhancementCommand {
    TeletextEnhancementCommand::SetPosition {
        position: TeletextEnhancementPosition::new(row, column).unwrap(),
    }
}
fn program(commands: Vec<TeletextEnhancementCommand>) -> TeletextEnhancementProgram {
    TeletextEnhancementProgram::new(commands).unwrap()
}
fn draft() -> TeletextResolvedServiceDraft {
    let language = TeletextLanguage::new("nld".into()).unwrap();
    TeletextResolvedServiceDraft {
        base: TeletextPageSetDraft {
            language: language.clone(),
            initial_page: owner().page,
            pages: vec![TeletextPageDraft {
                address: owner(),
                kind: TeletextPageKind::FullPage,
                language,
                national_option: TeletextNationalOption::new(1).unwrap(),
                rows: vec![row(1, 8, "£Z"), row(2, 1, "B")],
                navigation: vec![],
            }],
        },
        pages: vec![TeletextResolvedPageEnhancementDraft {
            address: owner(),
            presentation: None,
            compositional_links: None,
            local_program: Some(program(vec![])),
            resources: TeletextResolvedPageResourceSelection {
                objects: TeletextResolvedObjectPageLinks {
                    public: None,
                    global: None,
                },
                drcs: TeletextDrcsPageLinks::unlinked(),
                defaults: [None; 4],
            },
        }],
        magazines: vec![],
        objects: vec![],
        drcs: vec![],
    }
}
fn service(input: TeletextResolvedServiceDraft) -> TeletextSemanticService {
    TeletextSemanticService::new(
        input,
        TeletextServiceBudget {
            max_records: 64,
            max_objects: 32,
            max_structure_items: 4096,
            max_base_content_bytes: 8192,
            max_commands: 8192,
            max_collected_commands: 16384,
            max_collected_drcs_references: 0,
        },
    )
    .unwrap()
}
fn limits() -> TeletextNormalizedMainProgramBudget {
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
fn encode(service: &TeletextSemanticService) -> EncodedTeletextCompiledDisplayPage {
    encode_teletext_normalized_display_page(
        service,
        owner(),
        TeletextEncodingRegion::WesternLatin,
        limits(),
    )
    .unwrap()
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
fn packets(page: &EncodedTeletextPage) -> Vec<u8> {
    page.data_units().iter().map(packet).collect()
}
fn declaration(primary: u8) -> TeletextLevel25PresentationDefaults {
    let mut result =
        super::presentation::western_latin_v1_presentation(TeletextNationalOption::new(1).unwrap())
            .unwrap();
    result.primary = TeletextPageCharsetDesignation::new(primary).unwrap();
    result
}

#[test]
fn actual_x26_once_preserves_original_header_and_fallback_without_invented_x28() {
    let service = service(draft());
    let before = service.clone();
    let compiled = encode(&service);
    assert_eq!(
        packets(compiled.page()),
        [vec![0, 26], (1..25).collect()].concat()
    );
    assert!(compiled.magazine_units().is_empty());
    assert_eq!(compiled.normalized().owner(), owner());
    assert_eq!(compiled.page().address(), owner());
    assert_eq!(compiled.page().kind(), TeletextPageKind::FullPage);
    assert_eq!(compiled.page().language(), &service.base().language);
    let header = body(&compiled.page().data_units()[0]);
    assert_eq!(
        header[..8].iter().copied().map(nibble).collect::<Vec<_>>(),
        [0, 0, 4, 11, 2, 1, 0, 9]
    );
    let fallback = encode_teletext_page(
        &service.base().pages[0],
        TeletextEncodingRegion::WesternLatin,
    )
    .unwrap();
    let expected: Vec<_> = fallback
        .data_units()
        .iter()
        .filter(|unit| packet(unit) < 25)
        .copied()
        .collect();
    let retained: Vec<_> = compiled
        .page()
        .data_units()
        .iter()
        .filter(|unit| packet(unit) < 25)
        .copied()
        .collect();
    assert_eq!(retained, expected);
    assert_eq!(body(&compiled.page().data_units()[2])[8] & 0x7f, b' ');
    assert_eq!(
        body(&compiled.page().data_units()[1]),
        compiled.normalized().encoded().packet_bodies()[0]
    );
    assert!(matches!(
        service.effective_presentation(owner()),
        Some(TeletextEffectivePresentation::Header { national_option }) if national_option.get() == 1
    ));
    assert_eq!(service, before);
}

fn unused_links() -> TeletextX27Level25Links {
    let unused = TeletextX27LinkDeclaration::new(
        TeletextX27LinkTarget::Unspecified {
            magazine: TeletextX27Magazine::new(8).unwrap(),
        },
        TeletextX27LinkValidity::Level35Only,
        TeletextSubpageRequirementMask::new(0xffff),
    )
    .unwrap();
    TeletextX27Level25Links::new(unused, unused, unused, unused).unwrap()
}

#[test]
fn actual_editorial_compositional_page_and_magazine_metadata_keep_order_and_presence() {
    let mut input = draft();
    input.base.pages[0].navigation.push(TeletextNavigationLink {
        role: TeletextNavigationRole::Index,
        target: TeletextNavigationTarget {
            page: owner().page,
            subcode: None,
        },
    });
    let mut page = declaration(1);
    page.palette.clut2[0] = TeletextRgb::new([1, 2, 3]).unwrap();
    input.pages[0].presentation = Some(page);
    input.pages[0].compositional_links = Some(unused_links());
    let magazine = declaration(0);
    input.magazines.push(TeletextMagazinePresentationDraft {
        magazine: 2,
        presentation: magazine,
    });
    let service = service(input);
    let compiled = encode(&service);
    assert_eq!(
        packets(compiled.page()),
        [vec![0, 27, 27, 28, 26], (1..25).collect()].concat()
    );
    assert_eq!(body(&compiled.page().data_units()[1])[0], 0x15);
    assert_eq!(body(&compiled.page().data_units()[2])[0], 0x64);
    assert_eq!(
        body(&compiled.page().data_units()[3]),
        super::presentation::encode_presentation(&page)
    );
    assert_eq!(compiled.magazine_units().len(), 1);
    assert_eq!(packet(&compiled.magazine_units()[0]), 29);
    assert_eq!(
        body(&compiled.magazine_units()[0]),
        super::presentation::encode_presentation(&magazine)
    );
    assert!(matches!(service.effective_presentation(owner()),
        Some(TeletextEffectivePresentation::Extension { source: TeletextPresentationSource::Page, defaults }) if defaults == &page));
}

#[test]
fn magazine_only_remains_separate_from_compiled_page() {
    let mut input = draft();
    input.magazines.push(TeletextMagazinePresentationDraft {
        magazine: 2,
        presentation: declaration(1),
    });
    let compiled = encode(&service(input));
    assert!(!packets(compiled.page()).contains(&28));
    assert_eq!(compiled.magazine_units().len(), 1);
    assert_eq!(packet(&compiled.magazine_units()[0]), 29);
}

fn resource(id: u16) -> TeletextObjectIdentity {
    TeletextObjectIdentity::public(
        TeletextResourceIdentity::new(
            TeletextResourceKind::Pop,
            TeletextRawDataPageNumber::new(2, 0xa0).unwrap(),
            TeletextResourceSubpageIndex::new(0).unwrap(),
        )
        .unwrap(),
        TeletextObjectId::new(id),
    )
    .unwrap()
}
fn resource_fixture() -> TeletextSemanticService {
    let mut input = draft();
    input.base.pages[0].rows = vec![row(2, 8, "B")];
    input.pages[0].local_program = Some(program(vec![
        position(1, 8),
        TeletextEnhancementCommand::InvokeObject {
            target: resource(10),
            object_type: TeletextObjectType::Passive,
            offset: None,
        },
    ]));
    input.pages[0].resources.objects.public =
        resource(10).resource().map(TeletextResourceIdentity::page);
    input.objects = [10, 20]
        .into_iter()
        .map(|id| TeletextEnhancementObjectDefinition {
            identity: resource(id),
            object_type: TeletextObjectType::Passive,
            program: program(vec![
                position(1, 0),
                TeletextEnhancementCommand::AtColumn {
                    column: 0,
                    operation: TeletextColumnOperation::G0 {
                        code: TeletextGlyphCode::new(b'A').unwrap(),
                    },
                },
            ]),
        })
        .collect();
    service(input)
}
fn word(body: &[u8; 40], index: usize) -> u32 {
    let start = 1 + 3 * index;
    let raw = u32::from_le_bytes([body[start], body[start + 1], body[start + 2], 0]);
    [
        3, 5, 6, 7, 9, 10, 11, 12, 13, 14, 15, 17, 18, 19, 20, 21, 22, 23,
    ]
    .into_iter()
    .enumerate()
    .fold(0, |value, (bit, position)| {
        value | (((raw >> (position - 1)) & 1) << bit)
    })
}

#[test]
fn shared_mot_reservations_change_actual_pointer_without_rebinding_semantic_target() {
    let service = resource_fixture();
    let before = service.clone();
    let defaults = [TeletextDefaultObjectSelection {
        target: resource(20),
        object_type: TeletextObjectType::Passive,
    }];
    let compiled = encode_teletext_compiled_display_page(
        &service,
        owner(),
        TeletextEncodingRegion::WesternLatin,
        &defaults,
        &limits(),
        None,
    )
    .unwrap();
    let allocation = allocate_teletext_resource_objects(service.objects(), &defaults, 64).unwrap();
    assert_eq!(
        allocation
            .lookup(resource(20), TeletextObjectType::Passive)
            .unwrap()
            .object_number(),
        0
    );
    assert_eq!(
        allocation
            .lookup(resource(10), TeletextObjectType::Passive)
            .unwrap()
            .object_number(),
        16
    );
    let actual = compiled.normalized().encoded();
    let call = actual
        .command_positions()
        .iter()
        .find(|entry| {
            entry.source()
                == TeletextEnhancementSource {
                    object: None,
                    command: 1,
                }
        })
        .unwrap();
    let actual_word = word(
        &actual.packet_bodies()[call.semantic_word() / 13],
        call.semantic_word() % 13,
    );
    assert_eq!(
        (actual_word & 63, (actual_word >> 6) & 31, actual_word >> 11),
        (0x30, 0x13, 16)
    );
    let ordinary = compile_teletext_normalized_main_program(&service, owner(), limits()).unwrap();
    let original_call = ordinary
        .encoded()
        .command_positions()
        .iter()
        .find(|entry| entry.source() == call.source())
        .unwrap();
    let original_word = word(
        &ordinary.encoded().packet_bodies()[original_call.semantic_word() / 13],
        original_call.semantic_word() % 13,
    );
    assert_eq!(original_word >> 11, 0);
    assert_eq!(service, before);
}

#[test]
fn exact_owner_and_late_comparison_cutoff_return_no_owned_page_or_mutation() {
    let service = service(draft());
    let before = service.clone();
    let mut unknown = owner();
    unknown.subcode = TeletextSubcode::new(0x1235).unwrap();
    let missing = encode_teletext_compiled_display_page(
        &service,
        unknown,
        TeletextEncodingRegion::WesternLatin,
        &[],
        &limits(),
        None,
    )
    .unwrap_err();
    assert_eq!(missing.owner, unknown);
    assert!(matches!(
        missing.reason,
        TeletextCompiledDisplayPageEncodingFailure::MissingPage
    ));
    let mut bounded = limits();
    bounded.display_comparison.max_cells = 0;
    let error = encode_teletext_compiled_display_page(
        &service,
        owner(),
        TeletextEncodingRegion::WesternLatin,
        &[],
        &bounded,
        None,
    )
    .unwrap_err();
    assert!(
        matches!(error.reason, TeletextCompiledDisplayPageEncodingFailure::Normalization(reason)
        if matches!(reason.reason, TeletextNormalizedMainProgramFailure::DisplayComparison(_)))
    );
    assert_eq!(service, before);
}

#[test]
fn defaults_raw_bound_precedes_packing_without_partial_units() {
    let service = resource_fixture();
    let before = service.clone();
    let selected = TeletextDefaultObjectSelection {
        target: resource(20),
        object_type: TeletextObjectType::Passive,
    };
    let mut bounded = limits();
    bounded.max_resource_objects = 2;
    // Repeated slots still consume raw validation work; deduplication must not
    // erase the trusted input bound before the original allocation pass.
    let error = encode_teletext_compiled_display_page(
        &service,
        owner(),
        TeletextEncodingRegion::WesternLatin,
        &[selected, selected, selected],
        &bounded,
        None,
    )
    .unwrap_err();
    assert!(
        matches!(error.reason, TeletextCompiledDisplayPageEncodingFailure::Normalization(reason)
        if matches!(&reason.reason, TeletextNormalizedMainProgramFailure::Allocation(error)
            if **error == TeletextObjectAllocationError::DefaultBudget { count: 3, maximum: 2 }))
    );
    assert_eq!(service, before);
}

#[test]
fn compiled_packet_count_is_bounded_before_returning_any_page_units() {
    let input = draft();
    let page = &input.base.pages[0];
    let layout = super::layout::compile_for_composition(page).unwrap();
    let bodies = [[0_u8; 40]; 17];
    let error = super::packets::encode_compiled_page(
        page,
        &layout,
        &super::packets::PagePacketMetadata {
            presentation: None,
            compositional_links: None,
            enhancements: super::packets::PageEnhancements::Compiled(&bodies),
        },
    )
    .unwrap_err();
    assert_eq!(
        error,
        TeletextEncodingError::Enhancements { address: owner() }
    );
}

// Intentional opt-in Linux x64 decoder calibration; portable packet/model tests
// above remain available on all supported platforms. This is not client proof.
#[cfg(all(target_os = "linux", target_arch = "x86_64"))]
#[tokio::test]
#[ignore = "Requires Python 3 and the unchanged pinned teletextimager checkout"]
async fn production_display_units_decode_active_override_and_original_fallback() {
    let mut input = draft();
    let identity = TeletextObjectIdentity::local(owner(), TeletextObjectId::new(0xffff));
    input.pages[0].local_program = Some(program(vec![
        position(1, 8),
        TeletextEnhancementCommand::InvokeObject {
            target: identity,
            object_type: TeletextObjectType::Active,
            offset: None,
        },
    ]));
    input.objects.push(TeletextEnhancementObjectDefinition {
        identity,
        object_type: TeletextObjectType::Active,
        program: program(vec![TeletextEnhancementCommand::AtColumn {
            column: 0,
            operation: TeletextColumnOperation::G0 {
                code: TeletextGlyphCode::new(b'A').unwrap(),
            },
        }]),
    });
    let service = service(input);
    let compiled = encode(&service);
    let numbers = packets(compiled.page());
    assert_eq!(numbers[0], 0);
    assert!(numbers.contains(&26));
    assert!(!numbers.contains(&28));
    assert_eq!(
        numbers
            .iter()
            .copied()
            .filter(|&number| number < 25)
            .collect::<Vec<_>>(),
        (0..25).collect::<Vec<_>>()
    );
    // Feed the complete production result unchanged, then the ordinary cycle
    // terminator. No legacy packet replacement or synthetic page metadata.
    let mut units = compiled.page().data_units().to_vec();
    units.push(super::packets::terminator(2));
    let request = serde_json::to_vec(&serde_json::json!({
        "expected_version": "0.2.0",
        "cases": [{
            "name": "compiled_display_active_override", "page": 0x200, "subcode": 0x1234,
            "units": units.iter().map(|unit| unit.to_vec()).collect::<Vec<_>>(),
            "cells": [
                {"row": 1, "column": 8, "l15": [0x23, 7, 0, 7, 0], "l25": [b'A', 0, 0, 7, 0]},
                {"row": 1, "column": 9, "l15": [b'Z', 15, 0, 7, 0], "l25": [b'Z', 15, 0, 7, 0]},
                {"row": 2, "column": 1, "l15": [b'B', 15, 0, 7, 0], "l25": [b'B', 15, 0, 7, 0]}
            ]
        }]
    }))
    .unwrap();
    if let Some(path) = crate::locator::first_non_empty_env(&[
        "ERSATZRS_TELETEXT_TEST_COMPILED_DISPLAY_IMAGER_FIXTURE",
    ]) {
        std::fs::write(path, &request).unwrap();
    }
    let script =
        test_manifest_dir!().join("../../scripts/e2e/teletext-main-composition-decoder.py");
    let python = crate::locator::first_non_empty_env(&["ERSATZRS_TELETEXT_TEST_PYTHON"])
        .unwrap_or_else(|| "python3".into());
    let output =
        crate::teletext_probe::run_probe(&python, &[script.to_str().unwrap()], &request, 16 * 1024)
            .await
            .unwrap();
    let report: serde_json::Value = serde_json::from_slice(&output).unwrap();
    assert_eq!(
        report,
        serde_json::json!({"decoder": "teletextimager", "decoder_version": "0.2.0",
            "decoder_commit": "2b0125b778c45ffdfa2df0b196870c093361f317",
            "cases_passed": 1, "levels": ["1.5", "2.5"]})
    );
}
