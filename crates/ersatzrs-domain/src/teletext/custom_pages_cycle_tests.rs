use super::*;

fn page(number: u16, subcode: u16, seconds: Option<u16>) -> TeletextCustomPageWire {
    TeletextCustomPageWire {
        channel_links: Vec::new(),
        element_id: i64::from(number),
        element_revision: 4,
        page: number,
        subcode,
        national_option: 0,
        cycle_seconds: seconds,
        rows: Vec::new(),
        dynamic_fields: Vec::new(),
    }
}

fn source() -> TeletextCustomPagesWire {
    TeletextCustomPagesWire {
        schema: TELETEXT_CUSTOM_PAGES_SCHEMA.to_owned(),
        generation: "original".to_owned(),
        pages: vec![
            page(150, 1, Some(2)),
            page(150, 2, Some(2)),
            page(151, 1, Some(3)),
            page(151, 2, Some(3)),
        ],
    }
}

#[test]
fn published_cycle_roundtrip_retains_each_logical_pages_own_setting() {
    let original = source();
    let bytes = serde_json::to_vec(&original).unwrap();
    let reloaded = serde_json::from_slice(&bytes).unwrap();
    assert_eq!(original, reloaded);
    let document = TeletextCustomPagesDocument::from_wire(reloaded, "original").unwrap();
    assert_eq!(document.draft().pages.len(), 4);
    assert_eq!(
        document
            .subpage_cycles()
            .iter()
            .map(|cycle| (cycle.page().get(), cycle.seconds()))
            .collect::<Vec<_>>(),
        [(150, 2), (151, 3)]
    );
}

#[test]
fn old_frozen_original_does_not_invent_timing_metadata() {
    let mut original = source();
    for page in &mut original.pages {
        page.cycle_seconds = None;
    }
    let bytes = serde_json::to_vec(&original).unwrap();
    assert!(!String::from_utf8(bytes.clone())
        .unwrap()
        .contains("cycle_seconds"));
    let reloaded = serde_json::from_slice(&bytes).unwrap();
    let document = TeletextCustomPagesDocument::from_wire(reloaded, "original").unwrap();
    assert!(document.subpage_cycles().is_empty());
    assert_eq!(document.draft().pages.len(), 4);
}

#[test]
fn invalid_cycle_seconds_refuse_the_complete_source() {
    for seconds in [0, 601, u16::MAX] {
        let mut original = source();
        original.pages[0].cycle_seconds = Some(seconds);
        assert_eq!(
            TeletextCustomPagesDocument::from_wire(original, "original"),
            Err(TeletextCustomPagesError::Cycle { page: 150 })
        );
    }
}

#[test]
fn mixed_or_different_subpage_settings_cannot_define_one_logical_cycle() {
    for seconds in [None, Some(3)] {
        let mut original = source();
        original.pages[1].cycle_seconds = seconds;
        assert_eq!(
            TeletextCustomPagesDocument::from_wire(original, "original"),
            Err(TeletextCustomPagesError::Cycle { page: 150 })
        );
    }
    let mut original = source();
    original.pages[0].cycle_seconds = None;
    assert_eq!(
        TeletextCustomPagesDocument::from_wire(original, "original"),
        Err(TeletextCustomPagesError::Cycle { page: 150 })
    );
}

#[test]
fn existing_minimum_and_maximum_cycle_seconds_remain_exact() {
    let mut original = source();
    original.pages.truncate(2);
    for seconds in [1, 600] {
        for page in &mut original.pages {
            page.cycle_seconds = Some(seconds);
        }
        let document =
            TeletextCustomPagesDocument::from_wire(original.clone(), "original").unwrap();
        assert_eq!(document.subpage_cycles()[0].seconds(), seconds);
    }
}
