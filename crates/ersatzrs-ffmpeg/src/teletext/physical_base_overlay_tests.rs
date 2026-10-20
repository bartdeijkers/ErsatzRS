//! Exact physical editor rows shared by preview and both native page encoders.

use super::*;
use ersatzrs_domain::*;

fn address(subcode: u16) -> TeletextPageAddress {
    TeletextPageAddress {
        page: TeletextPageNumber::new(200).unwrap(),
        subcode: TeletextSubcode::new(subcode).unwrap(),
    }
}
fn service_budget() -> TeletextServiceBudget {
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
fn service(value: &str) -> TeletextSemanticService {
    let pages = [0x1234, 0x1235].map(|subcode| TeletextPageDraft {
        address: address(subcode),
        kind: TeletextPageKind::FullPage,
        language: TeletextLanguage::new("nld".into()).unwrap(),
        national_option: TeletextNationalOption::new(1).unwrap(),
        rows: vec![TeletextRow {
            row: 1,
            runs: vec![TeletextRun {
                column: 8,
                style: TeletextStyle::default(),
                content: TeletextRunContent::Text { text: value.into() },
            }],
        }],
        navigation: vec![],
    });
    TeletextSemanticService::new(
        TeletextResolvedServiceDraft {
            base: TeletextPageSetDraft {
                language: pages[0].language.clone(),
                initial_page: address(0x1234).page,
                pages: pages.into(),
            },
            pages: vec![],
            magazines: vec![],
            objects: vec![],
            drcs: vec![],
        },
        service_budget(),
    )
    .unwrap()
}
fn overlay(
    service: TeletextSemanticService,
    owner: TeletextPageAddress,
    row: u8,
    cells: [u8; 40],
) -> TeletextSemanticService {
    service
        .with_physical_base_overlays(
            vec![TeletextPhysicalBaseOverlay::new(
                owner,
                vec![TeletextPhysicalBaseRow::new(row, cells).unwrap()],
            )
            .unwrap()],
            service_budget(),
        )
        .unwrap()
}
fn original_row(service: &TeletextSemanticService, row: usize) -> [u8; 40] {
    super::layout::compile_for_composition(&service.base().pages[0])
        .unwrap()
        .rows[row]
}
fn composition(
    service: &TeletextSemanticService,
    owner: TeletextPageAddress,
) -> TeletextMainComposition {
    compose_teletext_main_program(service, owner, limits().composition).unwrap()
}
fn native(service: &TeletextSemanticService, owner: TeletextPageAddress) -> EncodedTeletextPage {
    encode_teletext_native_default_page(service, owner, TeletextEncodingRegion::WesternLatin)
        .unwrap()
        .page()
        .clone()
}
fn compiled(
    service: &TeletextSemanticService,
    owner: TeletextPageAddress,
) -> EncodedTeletextCompiledDisplayPage {
    encode_teletext_normalized_display_page(
        service,
        owner,
        TeletextEncodingRegion::WesternLatin,
        limits(),
    )
    .unwrap()
}
fn base_row(page: &EncodedTeletextPage, row: u8) -> [u8; 40] {
    project_encoded_teletext_display(page, 128)
        .unwrap()
        .records
        .into_iter()
        .find_map(|record| match record {
            TeletextEncodedDisplayRecord::BaseRow { row: actual, cells } if actual == row => {
                Some(cells)
            }
            _ => None,
        })
        .unwrap()
}

#[test]
fn one_cell_edit_has_identical_native_compiled_and_preview_bytes() {
    let original = service("ASCII");
    let before = native(&original, address(0x1234));
    let mut cells = original_row(&original, 1);
    cells[8] = b'Z';
    let edited = overlay(original.clone(), address(0x1234), 1, cells);
    assert_eq!(base_row(&native(&edited, address(0x1234)), 1), cells);
    assert_eq!(
        base_row(compiled(&edited, address(0x1234)).page(), 1),
        cells
    );
    let display = composition(&edited, address(0x1234));
    assert_eq!(display.base_display().cell(1, 8).unwrap().code(), b'Z');
    assert!(display
        .base_display()
        .cell(1, 8)
        .unwrap()
        .authored()
        .is_none());
    let reference = composition(&service("ZSCII"), address(0x1234));
    let options = TeletextColourPixelOptions {
        reveal_concealed: false,
        flash_phase: TeletextFlashPhase::Original,
    };
    let budget = TeletextRasterBudget {
        max_pixels: 216_000,
        max_diagnostics: 1800,
    };
    let actual_pixels = render_teletext_composition(&display, options, budget).unwrap();
    let reference_pixels = render_teletext_composition(&reference, options, budget).unwrap();
    assert_eq!(actual_pixels.pixels(), reference_pixels.pixels());
    assert_eq!(native(&original, address(0x1234)), before);
    assert_eq!(edited.base(), original.base());
}

#[test]
fn overlay_is_exact_subcode_and_service_instance_scoped() {
    let original = service("ASCII");
    let other_subpage = native(&original, address(0x1235));
    let mut first = original_row(&original, 1);
    first[8] = b'R';
    let mut second = first;
    second[8] = b'B';
    let red = overlay(original.clone(), address(0x1234), 1, first);
    let blue = overlay(original.clone(), address(0x1234), 1, second);
    assert_eq!(base_row(&native(&red, address(0x1234)), 1)[8], b'R');
    assert_eq!(base_row(&native(&blue, address(0x1234)), 1)[8], b'B');
    assert_eq!(native(&red, address(0x1235)), other_subpage);
    assert_eq!(native(&blue, address(0x1235)), other_subpage);
    assert_eq!(
        composition(&red, address(0x1234))
            .base_display()
            .cell(1, 8)
            .unwrap()
            .code(),
        b'R'
    );
    assert_eq!(
        composition(&blue, address(0x1234))
            .base_display()
            .cell(1, 8)
            .unwrap()
            .code(),
        b'B'
    );
}

#[test]
fn raw_national_position_is_not_inverted_into_unicode() {
    let original = service("ASCII");
    let mut cells = original_row(&original, 1);
    cells[8] = 0x5b;
    let edited = overlay(original, address(0x1234), 1, cells);
    let display = composition(&edited, address(0x1234));
    let cell = display.base_display().cell(1, 8).unwrap();
    assert_eq!(cell.code(), 0x5b);
    assert!(cell.authored().is_none());
    assert_eq!(
        base_row(compiled(&edited, address(0x1234)).page(), 1)[8],
        0x5b
    );
}

#[test]
fn spacing_mode_change_removes_old_text_intent_for_unchanged_coded_cells() {
    let original = service("ASCII");
    let mut cells = original_row(&original, 1);
    cells[7] = 0x11;
    let edited = overlay(original, address(0x1234), 1, cells);
    let display = composition(&edited, address(0x1234));
    let cell = display.base_display().cell(1, 8).unwrap();
    assert_eq!(cell.code(), b'A');
    assert!(cell.authored().is_none());
    assert!(cell.attributes().graphics());
    assert_eq!(cell.attributes().raw_foreground(), 1);
    assert_eq!(
        base_row(compiled(&edited, address(0x1234)).page(), 1),
        cells
    );
}

#[test]
fn unchanged_generated_unicode_fallback_preserves_exact_x26_packets() {
    let original = service("é™");
    let before = compiled(&original, address(0x1234));
    let cells = original_row(&original, 1);
    let edited = overlay(original, address(0x1234), 1, cells);
    let after = compiled(&edited, address(0x1234));
    assert_eq!(before.page(), after.page());
    assert_eq!(before.magazine_units(), after.magazine_units());
    assert_eq!(
        before.normalized().original().program(),
        after.normalized().original().program()
    );
}

#[test]
fn changed_generated_fallback_is_rejected_with_exact_location() {
    let original = service("é™");
    let mut cells = original_row(&original, 1);
    cells[8] = b'X';
    let edited = overlay(original, address(0x1234), 1, cells);
    let error = super::layout::compile_for_service_composition(&edited, &edited.base().pages[0])
        .err()
        .unwrap();
    assert_eq!(
        error,
        TeletextEncodingError::PhysicalBaseGeneratedGlyph {
            address: address(0x1234),
            row: 1,
            column: 8,
        }
    );
    assert!(compose_teletext_main_program(&edited, address(0x1234), limits().composition).is_err());
    assert!(encode_teletext_normalized_display_page(
        &edited,
        address(0x1234),
        TeletextEncodingRegion::WesternLatin,
        limits()
    )
    .is_err());
}

#[test]
fn unsupported_spacing_fails_both_packet_encoders_and_preview_at_same_cell() {
    // ESC is now checked by the standalone Base-G0 toggle/reset regressions.
    for code in [0x00, 0x0e, 0x0f, 0x10, 0x1e, 0x1f] {
        let original = service("ASCII");
        let mut cells = original_row(&original, 1);
        cells[7] = code;
        let edited = overlay(original, address(0x1234), 1, cells);
        let expected = TeletextEncodingError::PhysicalBaseControl {
            address: address(0x1234),
            row: 1,
            column: 7,
            code,
        };
        assert_eq!(
            super::layout::compile_for_service_composition(&edited, &edited.base().pages[0])
                .err()
                .unwrap(),
            expected
        );
        assert!(encode_teletext_native_default_page(
            &edited,
            address(0x1234),
            TeletextEncodingRegion::WesternLatin
        )
        .is_err());
        assert!(encode_teletext_normalized_display_page(
            &edited,
            address(0x1234),
            TeletextEncodingRegion::WesternLatin,
            limits()
        )
        .is_err());
        assert!(
            compose_teletext_main_program(&edited, address(0x1234), limits().composition).is_err()
        );
    }
}

#[test]
fn raw_header_double_height_does_not_bypass_safety() {
    let original = service("ASCII");
    let mut header = [b' '; 40];
    header[8] = 0x0d;
    let edited = overlay(original, address(0x1234), 0, header);
    assert_eq!(
        super::layout::compile_for_service_composition(&edited, &edited.base().pages[0])
            .err()
            .unwrap(),
        TeletextEncodingError::HeaderHeight {
            address: address(0x1234)
        }
    );
}

#[test]
fn unboxed_raw_caption_text_is_rejected_but_double_start_box_remains_visible() {
    let base = service("ASCII");
    let mut draft = base.base().clone();
    for page in &mut draft.pages {
        page.kind = TeletextPageKind::Subtitle;
        page.rows[0].runs[0].style.boxed = true;
    }
    let original = TeletextSemanticService::new(
        TeletextResolvedServiceDraft {
            base: draft,
            pages: vec![],
            magazines: vec![],
            objects: vec![],
            drcs: vec![],
        },
        service_budget(),
    )
    .unwrap();
    let mut unboxed = original_row(&original, 1);
    assert_eq!(&unboxed[6..8], &[0x0b, 0x0b]);
    unboxed[6..8].fill(b' ');
    let rejected = overlay(original.clone(), address(0x1234), 1, unboxed);
    assert_eq!(
        super::layout::compile_for_service_composition(&rejected, &rejected.base().pages[0])
            .err()
            .unwrap(),
        TeletextEncodingError::HiddenContent {
            address: address(0x1234),
            row: 1,
            column: 8
        }
    );
    assert!(encode_teletext_native_default_page(
        &rejected,
        address(0x1234),
        TeletextEncodingRegion::WesternLatin
    )
    .is_err());
    let mut boxed = original_row(&original, 1);
    boxed[8] = b'Z';
    let accepted = overlay(original, address(0x1234), 1, boxed);
    assert_eq!(base_row(&native(&accepted, address(0x1234)), 1), boxed);
    assert!(composition(&accepted, address(0x1234))
        .base_display()
        .cell(1, 8)
        .unwrap()
        .attributes()
        .boxed());
}

fn assert_height_rejected(service: &TeletextSemanticService, row: u8) {
    let owner = address(0x1234);
    let expected = TeletextEncodingError::Structure(TeletextStructureError::DoubleHeight {
        address: owner,
        row,
    });
    assert_eq!(
        super::layout::compile_for_service_composition(service, &service.base().pages[0])
            .err()
            .unwrap(),
        expected
    );
    assert!(encode_teletext_native_default_page(
        service,
        owner,
        TeletextEncodingRegion::WesternLatin
    )
    .is_err());
    assert!(encode_teletext_normalized_display_page(
        service,
        owner,
        TeletextEncodingRegion::WesternLatin,
        limits()
    )
    .is_err());
    assert!(compose_teletext_main_program(service, owner, limits().composition).is_err());
}

fn with_lower_content(original: &TeletextSemanticService) -> TeletextSemanticService {
    let mut base = original.base().clone();
    base.pages[0].rows.push(TeletextRow {
        row: 2,
        runs: vec![TeletextRun {
            column: 8,
            style: TeletextStyle::default(),
            content: TeletextRunContent::Text {
                text: "LOWER".into(),
            },
        }],
    });
    TeletextSemanticService::new(
        TeletextResolvedServiceDraft {
            base,
            pages: vec![],
            magazines: vec![],
            objects: vec![],
            drcs: vec![],
        },
        service_budget(),
    )
    .unwrap()
}

#[test]
fn physical_double_height_last_rows_reject_in_every_consumer() {
    for row in [23, 24] {
        let mut cells = [b' '; 40];
        cells[7] = 0x0d;
        cells[8] = b'A';
        let edited = overlay(service("ASCII"), address(0x1234), row, cells);
        assert_height_rejected(&edited, row);
    }
}

#[test]
fn physical_double_height_cannot_hide_retained_authored_lower_row() {
    let original = with_lower_content(&service("ASCII"));
    let mut cells = original_row(&original, 1);
    cells[7] = 0x0d;
    let edited = overlay(original, address(0x1234), 1, cells);
    assert_height_rejected(&edited, 2);
}

#[test]
fn physical_double_height_with_blank_lower_row_encodes_and_previews() {
    let original = service("ASCII");
    let mut cells = original_row(&original, 1);
    cells[7] = 0x0d;
    let edited = overlay(original, address(0x1234), 1, cells);
    assert_eq!(base_row(&native(&edited, address(0x1234)), 1), cells);
    assert_eq!(
        base_row(compiled(&edited, address(0x1234)).page(), 1),
        cells
    );
    assert!(composition(&edited, address(0x1234))
        .base_display()
        .cell(1, 8)
        .unwrap()
        .attributes()
        .double_height());
}

#[test]
fn lower_row_clear_is_checked_after_all_rows_regardless_of_overlay_order() {
    let original = with_lower_content(&service("ASCII"));
    let mut upper = original_row(&original, 1);
    upper[7] = 0x0d;
    let lower = [b' '; 40];
    for order in [[1, 2], [2, 1]] {
        let rows = order
            .map(|row| {
                TeletextPhysicalBaseRow::new(row, if row == 1 { upper } else { lower }).unwrap()
            })
            .into();
        let edited = original
            .clone()
            .with_physical_base_overlays(
                vec![TeletextPhysicalBaseOverlay::new(address(0x1234), rows).unwrap()],
                service_budget(),
            )
            .unwrap();
        assert_eq!(base_row(&native(&edited, address(0x1234)), 2), lower);
        assert_eq!(
            base_row(compiled(&edited, address(0x1234)).page(), 2),
            lower
        );
        assert!(composition(&edited, address(0x1234))
            .base_display()
            .cell(1, 8)
            .unwrap()
            .attributes()
            .double_height());
    }
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
