use crate::teletext::*;

fn address(subcode: u16) -> TeletextPageAddress {
    TeletextPageAddress {
        page: TeletextPageNumber::new(200).unwrap(),
        subcode: TeletextSubcode::new(subcode).unwrap(),
    }
}

fn budget() -> TeletextServiceBudget {
    TeletextServiceBudget {
        max_records: 16,
        max_objects: 16,
        max_structure_items: 128,
        max_base_content_bytes: 256,
        max_commands: 128,
        max_collected_commands: 128,
        max_collected_drcs_references: 128,
    }
}

fn service() -> TeletextSemanticService {
    let language = TeletextLanguage::new("eng".into()).unwrap();
    let pages = [address(0), address(1)]
        .into_iter()
        .map(|owner| TeletextPageDraft {
            address: owner,
            kind: TeletextPageKind::FullPage,
            language: language.clone(),
            national_option: TeletextNationalOption::new(0).unwrap(),
            rows: vec![TeletextRow {
                row: 1,
                runs: vec![TeletextRun {
                    column: 0,
                    style: TeletextStyle::default(),
                    content: TeletextRunContent::Text {
                        text: "base".into(),
                    },
                }],
            }],
            navigation: vec![],
        })
        .collect();
    TeletextSemanticService::new(
        TeletextResolvedServiceDraft {
            base: TeletextPageSetDraft {
                language,
                initial_page: address(0).page,
                pages,
            },
            pages: vec![],
            magazines: vec![],
            objects: vec![],
            drcs: vec![],
        },
        budget(),
    )
    .unwrap()
}

fn overlay(owner: TeletextPageAddress, row: u8, cell: u8) -> TeletextPhysicalBaseOverlay {
    TeletextPhysicalBaseOverlay::new(
        owner,
        vec![TeletextPhysicalBaseRow::new(row, [cell; 40]).unwrap()],
    )
    .unwrap()
}

#[test]
fn physical_row_checks_header_and_seven_bit_positions_without_revealing_content() {
    assert_eq!(
        TeletextPhysicalBaseRow::new(25, [b' '; 40]).unwrap_err(),
        TeletextPhysicalBaseOverlayError::Row { row: 25 }
    );
    let mut cells = [b' '; 40];
    cells[39] = 0x80;
    assert_eq!(
        TeletextPhysicalBaseRow::new(24, cells).unwrap_err(),
        TeletextPhysicalBaseOverlayError::Cell {
            row: 24,
            column: 39
        }
    );
    cells[39] = 0x00;
    assert_eq!(
        TeletextPhysicalBaseRow::new(24, cells).unwrap().cells()[39],
        0
    );
    cells[7] = b'X';
    assert_eq!(
        TeletextPhysicalBaseRow::new(0, cells).unwrap_err(),
        TeletextPhysicalBaseOverlayError::Header { column: 7 }
    );
    cells[7] = b' ';
    cells[8] = b'X';
    let header = TeletextPhysicalBaseRow::new(0, cells).unwrap();
    assert_eq!(header.cells()[8], b'X');
    assert!(!format!("{header:?}").contains("cells"));
}

#[test]
fn physical_overlay_requires_unique_bounded_rows_and_retains_input_order() {
    assert_eq!(
        TeletextPhysicalBaseOverlay::new(address(0), vec![]).unwrap_err(),
        TeletextPhysicalBaseOverlayError::Shape
    );
    let row = TeletextPhysicalBaseRow::new(1, [b'X'; 40]).unwrap();
    assert_eq!(
        TeletextPhysicalBaseOverlay::new(address(0), vec![row.clone(); 26]).unwrap_err(),
        TeletextPhysicalBaseOverlayError::Shape
    );
    assert_eq!(
        TeletextPhysicalBaseOverlay::new(address(0), vec![row.clone(), row]).unwrap_err(),
        TeletextPhysicalBaseOverlayError::DuplicateRow { row: 1 }
    );
    let all = (0..=24)
        .rev()
        .map(|row| TeletextPhysicalBaseRow::new(row, [b' '; 40]).unwrap())
        .collect();
    let result = TeletextPhysicalBaseOverlay::new(address(1), all).unwrap();
    assert_eq!(result.owner(), address(1));
    assert_eq!(result.rows().first().unwrap().row(), 24);
    assert_eq!(result.rows().last().unwrap().row(), 0);
    assert!(!format!("{result:?}").contains("cells"));
}

#[test]
fn physical_overlay_exact_subcode_owners_replacement_and_removal_preserve_originals() {
    let original = service();
    let changed = original
        .clone()
        .with_physical_base_overlays(
            vec![overlay(address(0), 1, b'A'), overlay(address(1), 1, b'B')],
            budget(),
        )
        .unwrap();
    assert_eq!(changed.base(), original.base());
    assert_eq!(changed.page_enhancements(), original.page_enhancements());
    assert_eq!(
        changed.collected(address(0)),
        original.collected(address(0))
    );
    assert_eq!(
        changed.physical_base_overlay(address(0)).unwrap().rows()[0].cells()[0],
        b'A'
    );
    assert_eq!(
        changed.physical_base_overlay(address(1)).unwrap().rows()[0].cells()[0],
        b'B'
    );
    assert!(changed.physical_base_overlay(address(2)).is_none());
    let replaced = changed
        .with_physical_base_overlays(vec![overlay(address(1), 24, b'C')], budget())
        .unwrap();
    assert!(replaced.physical_base_overlay(address(0)).is_none());
    assert_eq!(replaced.physical_base_overlays().len(), 1);
    assert_eq!(
        replaced
            .with_physical_base_overlays(vec![], budget())
            .unwrap(),
        original
    );
}

#[test]
fn physical_overlay_rejects_missing_or_duplicate_exact_owner() {
    for (overlays, owner, duplicate) in [
        (vec![overlay(address(2), 1, b'A')], address(2), false),
        (
            vec![overlay(address(0), 1, b'A'), overlay(address(0), 2, b'B')],
            address(0),
            true,
        ),
    ] {
        assert_eq!(
            service()
                .with_physical_base_overlays(overlays, budget())
                .unwrap_err(),
            TeletextServiceError::PhysicalBaseOverlayOwner {
                address: owner,
                duplicate
            }
        );
    }
}

#[test]
fn physical_overlay_shares_original_content_and_structure_budget_without_double_charging_replacement(
) {
    // Original: 2 base records + 2 rows + 2 runs, and 8 text bytes.
    // One overlay contributes its owner record, one row and 40 raw bytes.
    let original = service();
    let mut exact = budget();
    exact.max_structure_items = 8;
    exact.max_base_content_bytes = 48;
    let changed = original
        .clone()
        .with_physical_base_overlays(vec![overlay(address(0), 1, b'A')], exact)
        .unwrap();
    changed
        .clone()
        .with_physical_base_overlays(vec![overlay(address(1), 1, b'B')], exact)
        .unwrap();
    for dimension in ["structure item", "base content byte"] {
        let mut too_small = exact;
        if dimension == "structure item" {
            too_small.max_structure_items -= 1;
        } else {
            too_small.max_base_content_bytes -= 1;
        }
        assert!(matches!(
            original.clone().with_physical_base_overlays(vec![overlay(address(0), 1, b'A')], too_small),
            Err(TeletextServiceError::Budget { dimension: actual, .. }) if actual == dimension
        ));
    }
    // Removing rows still requires the original footprint, even under a new policy.
    let mut too_small = budget();
    too_small.max_base_content_bytes = 7;
    assert!(matches!(
        changed.with_physical_base_overlays(vec![], too_small),
        Err(TeletextServiceError::Budget {
            dimension: "base content byte",
            ..
        })
    ));
}
