use crate::teletext::*;

fn owner(subcode: u16) -> TeletextPageAddress {
    TeletextPageAddress {
        page: TeletextPageNumber::new(200).unwrap(),
        subcode: TeletextSubcode::new(subcode).unwrap(),
    }
}
fn limits() -> TeletextServiceBudget {
    TeletextServiceBudget {
        max_records: 8,
        max_objects: 8,
        max_structure_items: 128,
        max_base_content_bytes: 256,
        max_commands: 128,
        max_collected_commands: 128,
        max_collected_drcs_references: 128,
    }
}
fn input() -> TeletextSemanticService {
    let language = TeletextLanguage::new("eng".into()).unwrap();
    let pages = [owner(0), owner(32)]
        .into_iter()
        .map(|address| TeletextPageDraft {
            address,
            kind: TeletextPageKind::FullPage,
            language: language.clone(),
            national_option: TeletextNationalOption::new(0).unwrap(),
            rows: vec![TeletextRow {
                row: 1,
                runs: vec![
                    TeletextRun {
                        column: 4,
                        style: TeletextStyle {
                            conceal: true,
                            flash: true,
                            ..TeletextStyle::default()
                        },
                        content: TeletextRunContent::Text {
                            text: "£ŁD".into()
                        },
                    },
                    TeletextRun {
                        column: 12,
                        style: TeletextStyle::default(),
                        content: TeletextRunContent::Mosaics {
                            cells: vec![TeletextMosaic::new(63).unwrap()],
                            separated: true,
                        },
                    },
                ],
            }],
            navigation: vec![],
        })
        .collect();
    TeletextSemanticService::new(
        TeletextResolvedServiceDraft {
            base: TeletextPageSetDraft {
                language,
                initial_page: owner(0).page,
                pages,
            },
            pages: vec![],
            magazines: vec![],
            objects: vec![],
            drcs: vec![],
        },
        limits(),
    )
    .unwrap()
}
fn change(subcode: u16, column: u8, expected: char, replacement: char) -> TeletextBaseTextEdit {
    TeletextBaseTextEdit::new(
        owner(subcode),
        TeletextEnhancementPosition::new(1, column).unwrap(),
        expected,
        replacement,
    )
    .unwrap()
}

#[test]
fn base_text_edits_preserve_other_subpage_styles_mosaics_and_original_value() {
    let original = input();
    let physical = TeletextPhysicalBaseOverlay::new(
        owner(0),
        vec![TeletextPhysicalBaseRow::new(2, [b'A'; 40]).unwrap()],
    )
    .unwrap();
    let original = original
        .with_physical_base_overlays(vec![physical], limits())
        .unwrap();
    let before = original.clone();
    let edits = [change(32, 4, '£', '¥'), change(32, 5, 'Ł', 'Ø')];
    let derived = original.with_base_text_edits(&edits, limits()).unwrap();
    let mut expected = before.base().clone();
    expected.pages[1].rows[0].runs[0].content = TeletextRunContent::Text {
        text: "¥ØD".into()
    };
    assert_eq!(derived.base(), &expected);
    assert_eq!(derived.page_enhancements(), before.page_enhancements());
    assert_eq!(derived.objects(), before.objects());
    assert_eq!(derived.drcs(), before.drcs());
    assert_eq!(
        derived.physical_base_overlays(),
        before.physical_base_overlays()
    );
    assert_eq!(
        before.base().pages[1].rows[0].runs[0].content,
        TeletextRunContent::Text {
            text: "£ŁD".into()
        }
    );
    assert!(!format!("{:?}", edits[0]).contains('£'));
    assert!(!format!("{:?}", edits[0]).contains('¥'));
}

#[test]
fn base_text_edits_refuse_wrong_original_duplicate_gap_mosaic_and_missing_subpage() {
    for (edits, expected) in [
        (
            vec![change(32, 4, 'X', 'Y')],
            TeletextBaseTextEditError::Expected,
        ),
        (
            vec![change(32, 4, '£', '¥'); 2],
            TeletextBaseTextEditError::Duplicate,
        ),
        (
            vec![change(32, 10, 'X', 'Y')],
            TeletextBaseTextEditError::Target,
        ),
        (
            vec![change(32, 12, 'X', 'Y')],
            TeletextBaseTextEditError::Target,
        ),
        (
            vec![change(64, 4, '£', '¥')],
            TeletextBaseTextEditError::Target,
        ),
        // A successful earlier cell must not allow a partially returned service.
        (
            vec![change(32, 4, '£', '¥'), change(32, 5, 'X', 'Y')],
            TeletextBaseTextEditError::Expected,
        ),
    ] {
        assert!(matches!(input().with_base_text_edits(&edits, limits()),
            Err(TeletextServiceError::BaseTextEdit { reason, .. }) if reason == expected));
    }
    assert!(matches!(
        input().with_base_text_edits(&[], limits()),
        Err(TeletextServiceError::EmptyBaseTextEdits)
    ));
}

#[test]
fn base_text_edits_recheck_raw_input_and_final_utf8_byte_budgets() {
    let mut budget = limits();
    budget.max_commands = 0;
    assert!(matches!(
        input().with_base_text_edits(&[change(32, 4, '£', '¥')], budget),
        Err(TeletextServiceError::Budget { .. })
    ));
    // Original Text runs contain ten UTF-8 bytes plus two mosaic cells.
    budget = limits();
    budget.max_base_content_bytes = 12;
    assert!(matches!(
        input().with_base_text_edits(&[change(32, 6, 'D', '€')], budget),
        Err(TeletextServiceError::Budget { .. })
    ));
    budget.max_base_content_bytes = 14;
    assert!(input()
        .with_base_text_edits(&[change(32, 6, 'D', '€')], budget)
        .is_ok());
}

#[test]
fn base_text_edit_shape_rejects_header_controls_and_unchanged_scalars() {
    for (position, expected, replacement, error) in [
        ((0, 7), 'A', 'B', TeletextBaseTextEditError::Header),
        ((1, 4), 'A', '\n', TeletextBaseTextEditError::Scalar),
        ((1, 4), '\0', 'B', TeletextBaseTextEditError::Scalar),
        ((1, 4), '£', '£', TeletextBaseTextEditError::Unchanged),
    ] {
        assert_eq!(
            TeletextBaseTextEdit::new(
                owner(0),
                TeletextEnhancementPosition::new(position.0, position.1).unwrap(),
                expected,
                replacement
            )
            .unwrap_err(),
            error
        );
    }
}
