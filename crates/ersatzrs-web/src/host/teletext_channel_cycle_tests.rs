use super::*;
use ersatzrs_domain::{
    TeletextCustomPageWire, TeletextCustomPagesDocument, TeletextCustomPagesWire,
};

fn documents(seconds: Option<u16>, foreign_page: u16) -> Vec<TeletextProfileDocument> {
    let wire = TeletextCustomPagesWire {
        schema: ersatzrs_domain::TELETEXT_CUSTOM_PAGES_SCHEMA.to_owned(),
        generation: "original".to_owned(),
        pages: [1, 2]
            .into_iter()
            .map(|subcode| TeletextCustomPageWire {
                channel_links: Vec::new(),
                element_id: 7,
                element_revision: 3,
                page: 150,
                subcode,
                national_option: 0,
                cycle_seconds: seconds,
                rows: Vec::new(),
                dynamic_fields: Vec::new(),
            })
            .collect(),
    };
    let custom = TeletextCustomPagesDocument::from_wire(wire, "original").unwrap();
    let mut foreign = custom.draft().clone();
    foreign.pages.truncate(1);
    foreign.pages[0].address.page = ersatzrs_domain::TeletextPageNumber::new(foreign_page).unwrap();
    foreign.pages[0].address.subcode = ersatzrs_domain::TeletextSubcode::new(3).unwrap();
    foreign.initial_page = foreign.pages[0].address.page;
    vec![
        TeletextProfileDocument::CustomPages(Box::new(custom)),
        TeletextProfileDocument::Base(foreign),
    ]
}

#[test]
fn timed_custom_page_cannot_rotate_another_producers_distinct_subcode() {
    assert!(custom_subpage_cycles(&documents(Some(2), 150), 8).is_err());
    let cycles = custom_subpage_cycles(&documents(Some(2), 151), 8).unwrap();
    assert_eq!(cycles.len(), 1);
    assert_eq!((cycles[0].page().get(), cycles[0].seconds()), (150, 2));
}

#[test]
fn cycle_collection_retains_record_limit_and_old_untimed_behavior() {
    assert!(custom_subpage_cycles(&documents(Some(2), 151), 2).is_err());
    assert!(custom_subpage_cycles(&documents(None, 150), 8)
        .unwrap()
        .is_empty());
}
