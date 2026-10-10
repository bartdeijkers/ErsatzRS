use super::*;

fn address(page: u16, subcode: u16) -> TeletextPageAddress {
    TeletextPageAddress {
        page: TeletextPageNumber::new(page).unwrap(),
        subcode: TeletextSubcode::new(subcode).unwrap(),
    }
}

fn page(page: u16, subcode: u16) -> TeletextPageDraft {
    TeletextPageDraft {
        address: address(page, subcode),
        kind: TeletextPageKind::FullPage,
        language: TeletextLanguage::new("nld".into()).unwrap(),
        national_option: TeletextNationalOption::default(),
        rows: Vec::new(),
        navigation: Vec::new(),
    }
}

fn draft(pages: Vec<TeletextPageDraft>) -> TeletextPageSetDraft {
    TeletextPageSetDraft {
        language: TeletextLanguage::new("nld".into()).unwrap(),
        initial_page: TeletextPageNumber::new(200).unwrap(),
        pages,
    }
}

fn text(row: u8, column: u8, value: &str) -> TeletextRow {
    TeletextRow {
        row,
        runs: vec![TeletextRun {
            column,
            style: TeletextStyle::default(),
            content: TeletextRunContent::Text { text: value.into() },
        }],
    }
}

#[test]
fn display_addresses_encode_decimal_digits_and_magazine_eight() {
    for number in 100..=899 {
        let value = TeletextPageNumber::new(number).unwrap();
        let encoded = u16::from(value.wire_page());
        assert_eq!(encoded >> 4, number / 10 % 10);
        assert_eq!(encoded & 15, number % 10);
        assert_eq!(u16::from(value.wire_magazine()), number / 100 % 8);
        assert_eq!(
            serde_json::from_str::<TeletextPageNumber>(&number.to_string()).unwrap(),
            value
        );
    }
    for number in [0_u16, 99, 900, u16::MAX] {
        assert!(TeletextPageNumber::new(number).is_err());
        assert!(serde_json::from_str::<TeletextPageNumber>(&number.to_string()).is_err());
    }
}

#[test]
fn subcodes_retain_raw_identity_and_reject_reserved_header_bits() {
    for raw in 0..=u16::MAX {
        match TeletextSubcode::new(raw) {
            Ok(value) => {
                assert_eq!(raw & 0xc080, 0);
                assert_eq!(value.get(), raw);
            }
            Err(_) => assert_ne!(raw & 0xc080, 0),
        }
    }
    assert_ne!(
        TeletextSubcode::new(10).unwrap(),
        TeletextSubcode::new(0x10).unwrap()
    );
    assert_eq!(TeletextSubcode::new(0x3f7f).unwrap().get(), 0x3f7f);
    assert!(serde_json::from_str::<TeletextSubcode>("128").is_err());
}

#[test]
fn serialized_scalars_cannot_bypass_constructor_validation() {
    for code in ["ENG", "en-US", "en", "éé", " eng"] {
        let json = serde_json::to_string(code).unwrap();
        assert!(serde_json::from_str::<TeletextLanguage>(&json).is_err());
    }
    for code in ["eng", "nld", "und"] {
        let language = TeletextLanguage::new(code.into()).unwrap();
        assert_eq!(language.as_str(), code);
    }
    assert_eq!(
        serde_json::from_str::<TeletextMosaic>("63")
            .unwrap()
            .blocks(),
        63
    );
    assert!(serde_json::from_str::<TeletextMosaic>("64").is_err());
    assert_eq!(
        serde_json::from_str::<TeletextNationalOption>("7")
            .unwrap()
            .get(),
        7
    );
    assert!(serde_json::from_str::<TeletextNationalOption>("8").is_err());
}

#[test]
fn page_links_resolve_actual_subpages_without_inventing_subcode_zero() {
    let mut first = page(200, 1);
    first.navigation.push(TeletextNavigationLink {
        role: TeletextNavigationRole::Red,
        target: TeletextNavigationTarget {
            page: first.address.page,
            subcode: None,
        },
    });
    let mut set = draft(vec![first, page(200, 0x10)]);
    assert_eq!(set.validate_structure(), Ok(()));
    set.pages[0].navigation[0].target.subcode = Some(TeletextSubcode::default());
    assert!(matches!(
        set.validate_structure(),
        Err(TeletextStructureError::MissingTarget { .. })
    ));
    set.pages[0].navigation[0].target.subcode = Some(TeletextSubcode::new(0x10).unwrap());
    assert_eq!(set.validate_structure(), Ok(()));
}

#[test]
fn duplicate_addresses_and_missing_initial_pages_fail_generation_validation() {
    let set = draft(vec![page(200, 1), page(200, 1)]);
    assert!(matches!(
        set.validate_structure(),
        Err(TeletextStructureError::DuplicatePage { .. })
    ));
    assert_eq!(
        draft(vec![page(201, 0)]).validate_structure(),
        Err(TeletextStructureError::MissingInitialPage)
    );
    assert_eq!(
        draft(Vec::new()).validate_structure(),
        Err(TeletextStructureError::RecordCount)
    );
    assert_eq!(
        draft(vec![page(200, 0); 1025]).validate_structure(),
        Err(TeletextStructureError::RecordCount)
    );
}

#[test]
fn header_ownership_and_row_bounds_are_validated_without_truncation() {
    let mut value = page(200, 0);
    value.rows = vec![text(0, 8, &"H".repeat(32)), text(1, 0, &"é".repeat(40))];
    assert_eq!(value.validate_structure(), Ok(()));
    value.rows[0].runs[0].column = 7;
    assert!(matches!(
        value.validate_structure(),
        Err(TeletextStructureError::Run { row: 0, .. })
    ));
    value.rows[0].runs[0].column = 8;
    value.rows[1].runs[0].column = 1;
    assert!(matches!(
        value.validate_structure(),
        Err(TeletextStructureError::Run { row: 1, .. })
    ));
    value.rows[1].row = 26;
    assert!(matches!(
        value.validate_structure(),
        Err(TeletextStructureError::Row { row: 26, .. })
    ));
}

#[test]
fn overlaps_and_repeated_rows_are_rejected() {
    let mut value = page(200, 0);
    value.rows = vec![text(1, 0, "ABC")];
    value.rows[0].runs.push(text(1, 2, "D").runs.remove(0));
    assert!(matches!(
        value.validate_structure(),
        Err(TeletextStructureError::Overlap { .. })
    ));
    value.rows[0].runs.pop();
    value.rows.push(text(1, 10, "E"));
    assert!(matches!(
        value.validate_structure(),
        Err(TeletextStructureError::Row { .. })
    ));
}

#[test]
fn double_height_cannot_hide_authored_lower_row_content() {
    let mut value = page(200, 0);
    value.rows = vec![text(1, 0, "A"), text(2, 20, "B")];
    value.rows[0].runs[0].style.height = TeletextHeight::Double;
    assert!(matches!(
        value.validate_structure(),
        Err(TeletextStructureError::DoubleHeight { row: 2, .. })
    ));
    value.rows[1].runs.clear();
    assert_eq!(value.validate_structure(), Ok(()));
    for row in [23, 24, 25] {
        value.rows[0].row = row;
        assert!(matches!(
            value.validate_structure(),
            Err(TeletextStructureError::DoubleHeight { .. })
        ));
    }
}

#[test]
fn literal_unicode_is_preserved_and_controls_are_rejected() {
    let mut value = page(200, 0);
    // Structural validity does not promise a DVB charset can encode this text.
    value.rows = vec![text(1, 0, "é e\u{301} 📺 <script>")];
    let json = serde_json::to_string(&value).unwrap();
    let decoded: TeletextPageDraft = serde_json::from_str(&json).unwrap();
    assert_eq!(decoded, value);
    assert_eq!(decoded.validate_structure(), Ok(()));
    for invalid in ["", "a\nb", "a\u{1b}b", "\u{7f}"] {
        value.rows[0] = text(1, 0, invalid);
        assert!(matches!(
            value.validate_structure(),
            Err(TeletextStructureError::Run { .. })
        ));
    }
}

#[test]
fn mosaics_have_logical_extents_and_spacing_controls_remain_encoder_owned() {
    let mut value = page(200, 0);
    let mut row = text(1, 0, &"A".repeat(40));
    row.runs[0].style.foreground = TeletextColour::Cyan;
    // There is no room for the colour control: a later encoder must reject it.
    // This API deliberately checks only structural display intent.
    value.rows = vec![row];
    assert_eq!(value.validate_structure(), Ok(()));
    value.rows[0].runs[0].content = TeletextRunContent::Mosaics {
        cells: vec![TeletextMosaic::new(63).unwrap(); 40],
        separated: true,
    };
    assert_eq!(value.validate_structure(), Ok(()));
    value.rows[0].runs[0].column = 1;
    assert!(matches!(
        value.validate_structure(),
        Err(TeletextStructureError::Run { .. })
    ));
}

#[test]
fn navigation_roles_are_unique_and_transport_fields_are_not_accepted() {
    let mut value = page(200, 0);
    let link = TeletextNavigationLink {
        role: TeletextNavigationRole::Index,
        target: TeletextNavigationTarget {
            page: value.address.page,
            subcode: None,
        },
    };
    value.navigation = vec![link.clone(), link];
    assert!(matches!(
        value.validate_structure(),
        Err(TeletextStructureError::Navigation { .. })
    ));
    let mut json = serde_json::to_value(page(200, 0)).unwrap();
    json["transport_bytes"] = serde_json::json!([27, 1]);
    assert!(serde_json::from_value::<TeletextPageDraft>(json).is_err());
}
