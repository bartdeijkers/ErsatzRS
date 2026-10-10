use serde_json::{json, Value};

use super::*;

fn style() -> Value {
    json!({
        "foreground": "yellow", "background": "blue", "height": "normal",
        "boxed": true, "flash": true, "conceal": false
    })
}

fn page(number: u16, subcode: u16) -> Value {
    json!({
        "address": { "page": number, "subcode": subcode },
        "kind": "full_page", "language": "nld", "national_option": 5,
        "rows": [], "navigation": []
    })
}

fn response(pages: Vec<Value>) -> Value {
    let mut value = json!({
        "schema_version": 1, "generation": "generation-1",
        "language": "nld", "initial_page": 200
    });
    value["pages"] = Value::Array(pages);
    value
}

fn convert(value: &Value) -> Result<TeletextPageSetDraft, TeletextPagesError> {
    parse_page_set(&serde_json::to_vec(value).unwrap(), "generation-1")
}

#[test]
fn teletext_pages_preserve_literal_unicode_mosaics_and_styles() {
    let mut first = page(200, 0x10);
    first["kind"] = json!("newsflash");
    first["rows"] = json!([
        { "row": 1, "runs": [{ "column": 2, "style": style(),
          "content": { "kind": "text", "text": "Café <b>雪</b>" } }] },
        { "row": 2, "runs": [{ "column": 3, "style": style(),
          "content": { "kind": "mosaics", "cells": [0, 1, 32, 63],
                       "separated": true } }] }
    ]);
    let draft = convert(&response(vec![first])).unwrap();
    assert_eq!(draft.language.as_str(), "nld");
    assert_eq!(draft.initial_page.get(), 200);
    let first = &draft.pages[0];
    assert_eq!(first.address.subcode.get(), 0x10);
    assert_eq!(first.kind, TeletextPageKind::Newsflash);
    assert_eq!(first.language.as_str(), "nld");
    assert_eq!(first.national_option.get(), 5);
    let run = &first.rows[0].runs[0];
    assert_eq!(first.rows[0].row, 1);
    assert_eq!(run.column, 2);
    assert_eq!(
        run.style,
        TeletextStyle {
            foreground: TeletextColour::Yellow,
            background: TeletextColour::Blue,
            height: TeletextHeight::Normal,
            boxed: true,
            flash: true,
            conceal: false
        }
    );
    assert_eq!(
        run.content,
        TeletextRunContent::Text {
            text: "Café <b>雪</b>".to_owned()
        }
    );
    assert_eq!(
        first.rows[1].runs[0].content,
        TeletextRunContent::Mosaics {
            cells: [0, 1, 32, 63]
                .into_iter()
                .map(|value| TeletextMosaic::new(value).unwrap())
                .collect(),
            separated: true
        }
    );
}

#[test]
fn teletext_pages_navigation_keeps_null_and_exact_actual_subcodes() {
    let mut first = page(200, 1);
    first["navigation"] = json!([
        { "role": "index", "target": { "page": 200, "subcode": null } },
        { "role": "red", "target": { "page": 200, "subcode": 16 } }
    ]);
    let mut value = response(vec![first, page(200, 0x10)]);
    let draft = convert(&value).unwrap();
    assert_eq!(
        draft.pages[0].navigation[0].role,
        TeletextNavigationRole::Index
    );
    assert_eq!(draft.pages[0].navigation[0].target.subcode, None);
    assert_eq!(
        draft.pages[0].navigation[1].role,
        TeletextNavigationRole::Red
    );
    assert_eq!(
        draft.pages[0].navigation[1].target.subcode.unwrap().get(),
        0x10
    );
    value["pages"][0]["navigation"][1]["target"]["subcode"] = json!(0);
    assert!(matches!(
        convert(&value),
        Err(TeletextPagesError::Structure(
            TeletextStructureError::MissingTarget { .. }
        ))
    ));
}

#[test]
fn teletext_pages_reject_invalid_values_without_normalization() {
    for (field, invalid) in [("language", json!("NLD")), ("national_option", json!(8))] {
        let mut value = response(vec![page(200, 0)]);
        value["pages"][0][field] = invalid;
        assert!(matches!(
            convert(&value),
            Err(TeletextPagesError::DomainValue(_))
        ));
    }
    for (field, invalid) in [("page", json!(99)), ("subcode", json!(128))] {
        let mut value = response(vec![page(200, 0)]);
        value["pages"][0]["address"][field] = invalid;
        assert!(matches!(
            convert(&value),
            Err(TeletextPagesError::DomainValue(_))
        ));
    }
    let mut value = response(vec![page(200, 0)]);
    value["pages"][0]["rows"] = json!([{ "row": 1, "runs": [
        { "column": 0, "style": style(), "content": {
            "kind": "mosaics", "cells": [64], "separated": false
        } }
    ] }]);
    assert!(matches!(
        convert(&value),
        Err(TeletextPagesError::DomainValue(TeletextValueError::Mosaic))
    ));
}

#[test]
fn teletext_pages_validate_rows_overlap_and_whole_set_references() {
    let mut invalid_row = response(vec![page(200, 0)]);
    invalid_row["pages"][0]["rows"] = json!([{ "row": 25, "runs": [] }]);
    assert!(matches!(
        convert(&invalid_row),
        Err(TeletextPagesError::Structure(
            TeletextStructureError::Row { .. }
        ))
    ));
    let mut overlap = response(vec![page(200, 0)]);
    overlap["pages"][0]["rows"] = json!([{ "row": 1, "runs": [
        { "column": 0, "style": style(), "content": { "kind": "text", "text": "ab" } },
        { "column": 1, "style": style(), "content": { "kind": "text", "text": "c" } }
    ] }]);
    assert!(matches!(
        convert(&overlap),
        Err(TeletextPagesError::Structure(
            TeletextStructureError::Overlap { .. }
        ))
    ));
    assert!(matches!(
        convert(&response(vec![page(200, 1), page(200, 1)])),
        Err(TeletextPagesError::Structure(
            TeletextStructureError::DuplicatePage { .. }
        ))
    ));
    assert!(matches!(
        convert(&response(vec![page(201, 0)])),
        Err(TeletextPagesError::Structure(
            TeletextStructureError::MissingInitialPage
        ))
    ));
}

#[test]
fn teletext_pages_reject_late_invalid_page_and_redact_literal_text() {
    let mut value = response(vec![page(200, 0), page(201, 1)]);
    value["pages"][1]["rows"] = json!([{ "row": 1, "runs": [
        { "column": 0, "style": style(), "content": {
            "kind": "text", "text": "private-programme\n"
        } }
    ] }]);
    let error = convert(&value).unwrap_err();
    assert!(matches!(
        error,
        TeletextPagesError::Structure(TeletextStructureError::Run { .. })
    ));
    assert!(!error.to_string().contains("private-programme"));
    assert!(!format!("{error:?}").contains("private-programme"));
}

#[test]
fn teletext_pages_raw_entry_rejects_wrong_echo_and_missing_explicit_fields() {
    let mut value = response(vec![page(200, 0)]);
    value["generation"] = json!("another-generation");
    assert!(matches!(
        convert(&value),
        Err(TeletextPagesError::Contract(_))
    ));
    let mut value = response(vec![page(200, 0)]);
    value["pages"][0]
        .as_object_mut()
        .unwrap()
        .remove("national_option");
    assert!(matches!(
        convert(&value),
        Err(TeletextPagesError::Contract(_))
    ));
}
