//! Complete authored output keeps the original shared model and Level1.5 fallback.
use super::*;
use crate::teletext::{
    authored_service_tests::{declared, transmissions},
    encode_teletext_authored_service, encode_teletext_page, project_encoded_teletext_display,
    EncodedTeletextAuthoredPage, TeletextEncodedDisplayRecord, TeletextEncodingRegion,
};

#[test]
fn same_page_main_active_order_reaches_complete_authored_shared_display_and_fallback() {
    let mut draft = fixture_draft_with_active_body(
        |passive, active| vec![fg(8, 1), active, position(9), passive],
        vec![fg(0, 3), fg(0, 4)],
    );
    draft.base.pages[0].rows.push(TeletextRow {
        row: 0,
        runs: vec![TeletextRun {
            column: 9,
            style: TeletextStyle::default(),
            content: TeletextRunContent::Text { text: "ZZ".into() },
        }],
    });
    let source = declared(draft);
    let unchanged = source.clone();
    let service =
        encode_teletext_authored_service(source, &transmissions(&unchanged), &limits()).unwrap();
    assert_eq!(service.declared(), &unchanged);
    let EncodedTeletextAuthoredPage::CompiledDisplay(page) = &service.pages()[0] else {
        panic!("the complete factory must retain a compiled shared display");
    };
    let original = page.normalized().original();
    let physical = page.normalized().physical().replay();
    for attributes in [
        original.attribute_execution(),
        physical.attribute_execution(),
    ] {
        assert_eq!(
            attributes.cell(0, 10).unwrap().foreground().value(),
            Some(TeletextColourReference::new(4).unwrap())
        );
        assert_eq!(attributes.cell(0, 10).unwrap().foreground().pending(), None);
    }
    for display in [original.display_resolution(), physical.display_resolution()] {
        assert_eq!(
            display
                .cell(0, 10)
                .unwrap()
                .attributes()
                .foreground()
                .value(),
            Some(TeletextColourReference::new(4).unwrap())
        );
    }
    let fallback = encode_teletext_page(
        &unchanged.semantic().base().pages[0],
        TeletextEncodingRegion::WesternLatin,
    )
    .unwrap();
    let actual = project_encoded_teletext_display(page.page(), 128).unwrap();
    let baseline = project_encoded_teletext_display(&fallback, 128).unwrap();
    assert_eq!(actual.header, baseline.header);
    let rows = |records: Vec<TeletextEncodedDisplayRecord>| {
        records
            .into_iter()
            .filter(|record| !matches!(record, TeletextEncodedDisplayRecord::Triplets { .. }))
            .collect::<Vec<_>>()
    };
    assert_eq!(rows(actual.records), rows(baseline.records));
    assert_eq!(unchanged, *service.declared());
}
