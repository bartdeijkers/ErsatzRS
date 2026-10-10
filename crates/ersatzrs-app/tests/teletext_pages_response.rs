use ersatzrs_addon_contract::TeletextPagesError as WireError;
use ersatzrs_app::teletext_pages::{parse_page_set, TeletextPagesError};
use ersatzrs_domain::{TeletextNavigationRole, TeletextRunContent};

// Independent original wire fixture: generic authored content, no NOS page map
// or provider data. Required style/identity fields and nullable links are explicit.
const RESPONSE: &str = r#"{
  "schema_version": 1,
  "generation": "synthetic-response-1",
  "language": "nld",
  "initial_page": 200,
  "pages": [{
    "address": {"page": 200, "subcode": 0},
    "kind": "full_page",
    "language": "nld",
    "national_option": 0,
    "rows": [{
      "row": 1,
      "runs": [{
        "column": 0,
        "style": {
          "foreground": "white", "background": "black", "height": "normal",
          "boxed": false, "flash": false, "conceal": false
        },
        "content": {"kind": "text", "text": "TV <guide> & caf\u00e9"}
      }]
    }, {
      "row": 2,
      "runs": [{
        "column": 0,
        "style": {
          "foreground": "green", "background": "black", "height": "normal",
          "boxed": false, "flash": false, "conceal": false
        },
        "content": {"kind": "mosaics", "cells": [1, 63, 0], "separated": true}
      }]
    }],
    "navigation": [{
      "role": "index", "target": {"page": 200, "subcode": null}
    }]
  }]
}"#;

#[test]
fn public_host_boundary_preserves_literal_unicode_mosaics_and_nullable_navigation() {
    let draft = parse_page_set(RESPONSE.as_bytes(), "synthetic-response-1").unwrap();
    assert_eq!(draft.language.as_str(), "nld");
    assert_eq!(draft.initial_page.get(), 200);
    let page = &draft.pages[0];
    assert_eq!(page.address.page.get(), 200);
    assert_eq!(page.address.subcode.get(), 0);
    assert_eq!(page.national_option.get(), 0);
    assert_eq!(
        page.rows[0].runs[0].content,
        TeletextRunContent::Text {
            text: "TV <guide> & café".to_owned(),
        }
    );
    let TeletextRunContent::Mosaics { cells, separated } = &page.rows[1].runs[0].content else {
        panic!("typed mosaic content must survive host conversion");
    };
    assert!(*separated);
    assert_eq!(
        cells.iter().map(|cell| cell.blocks()).collect::<Vec<_>>(),
        [1, 63, 0]
    );
    assert_eq!(page.navigation[0].role, TeletextNavigationRole::Index);
    assert_eq!(page.navigation[0].target.page.get(), 200);
    assert_eq!(page.navigation[0].target.subcode, None);
    draft.validate_structure().unwrap();
    // Structure grants no encoded charset, published generation or runtime output.
}

#[test]
fn nested_duplicate_and_missing_nullable_key_fail_before_host_defaults_or_conversion() {
    let duplicate = RESPONSE.replace("\"subcode\": null", "\"subcode\": null, \"subcode\": null");
    assert!(matches!(
        parse_page_set(duplicate.as_bytes(), "synthetic-response-1"),
        Err(TeletextPagesError::Contract(WireError::DuplicateKey))
    ));
    let missing = RESPONSE.replace("\"page\": 200, \"subcode\": null", "\"page\": 200");
    assert!(matches!(
        parse_page_set(missing.as_bytes(), "synthetic-response-1"),
        Err(TeletextPagesError::Contract(WireError::InvalidJson))
    ));
}
