use super::*;

use serde_json::{json, Value};

const RESPONSE: &str = r#"{
    "schema_version": 1,
    "generation": "fixture-generation",
    "language": "eng",
    "initial_page": 200,
    "pages": [{
        "address": {"page": 200, "subcode": 0},
        "kind": "full_page",
        "language": "eng",
        "national_option": 0,
        "rows": [{"row": 1, "runs": [{
            "column": 2,
            "style": {"foreground": "white", "background": "black", "height": "normal",
                      "boxed": false, "flash": false, "conceal": false},
            "content": {"kind": "text", "text": "Literal café"}
        }, {
            "column": 20,
            "style": {"foreground": "cyan", "background": "blue", "height": "double",
                      "boxed": true, "flash": true, "conceal": true},
            "content": {"kind": "mosaics", "cells": [0, 63], "separated": true}
        }]}],
        "navigation": [{"role": "index", "target": {"page": 200, "subcode": null}}]
    }]
}"#;
const GENERATION: &str = "fixture-generation";

fn document() -> Value {
    serde_json::from_str(RESPONSE).unwrap()
}

fn parse(value: &Value) -> Result<TeletextPagesResponse, TeletextPagesError> {
    TeletextPagesResponse::from_json(&serde_json::to_vec(value).unwrap(), GENERATION)
}

fn object_paths() -> [&'static str; 11] {
    [
        "",
        "/pages/0",
        "/pages/0/address",
        "/pages/0/rows/0",
        "/pages/0/rows/0/runs/0",
        "/pages/0/rows/0/runs/0/style",
        "/pages/0/rows/0/runs/0/content",
        "/pages/0/rows/0/runs/1/content",
        "/pages/0/navigation/0",
        "/pages/0/navigation/0/target",
        "/pages/0/rows/0/runs/1/style",
    ]
}

#[test]
fn explicit_wire_response_roundtrips_typed_text_mosaics_styles_and_navigation() {
    let response = TeletextPagesResponse::from_json(RESPONSE.as_bytes(), GENERATION).unwrap();
    assert_eq!(response.schema_version, 1);
    assert_eq!(response.generation, GENERATION);
    assert_eq!(response.language, "eng");
    assert_eq!(response.initial_page, 200);
    let page = &response.pages[0];
    assert_eq!(
        page.address,
        TeletextPageAddressWire {
            page: 200,
            subcode: 0
        }
    );
    assert_eq!(page.kind, TeletextPageKindWire::FullPage);
    assert_eq!(
        page.rows[0].runs[0].content,
        TeletextRunContentWire::Text {
            text: "Literal café".to_owned()
        }
    );
    assert_eq!(
        page.rows[0].runs[1].content,
        TeletextRunContentWire::Mosaics {
            cells: vec![0, 63],
            separated: true
        }
    );
    assert_eq!(
        page.rows[0].runs[1].style.foreground,
        TeletextColourWire::Cyan
    );
    assert_eq!(
        page.rows[0].runs[1].style.height,
        TeletextHeightWire::Double
    );
    assert!(page.rows[0].runs[1].style.boxed);
    assert_eq!(page.navigation[0].target.subcode, None);
    assert_eq!(
        TeletextPagesResponse::from_json(&serde_json::to_vec(&response).unwrap(), GENERATION)
            .unwrap(),
        response
    );
}

#[test]
fn every_wire_object_field_is_required_and_unknown_fields_are_rejected() {
    let original = document();
    for path in object_paths() {
        let fields = original.pointer(path).unwrap().as_object().unwrap().keys();
        for field in fields {
            let mut missing = original.clone();
            missing
                .pointer_mut(path)
                .unwrap()
                .as_object_mut()
                .unwrap()
                .remove(field);
            assert_eq!(
                parse(&missing),
                Err(TeletextPagesError::InvalidJson),
                "missing {path}/{field}"
            );
        }
        let mut unknown = original.clone();
        unknown
            .pointer_mut(path)
            .unwrap()
            .as_object_mut()
            .unwrap()
            .insert("unexpected".to_owned(), json!(true));
        assert_eq!(
            parse(&unknown),
            Err(TeletextPagesError::InvalidJson),
            "unknown field at {path}"
        );
    }
}

#[test]
fn nullable_navigation_subcode_requires_presence_and_preserves_explicit_values() {
    for subcode in [Value::Null, json!(0), json!(0x1234), json!(u16::MAX)] {
        let mut value = document();
        value["pages"][0]["navigation"][0]["target"]["subcode"] = subcode.clone();
        let response = parse(&value).unwrap();
        let expected = if subcode.is_null() {
            None
        } else {
            Some(u16::try_from(subcode.as_u64().unwrap()).unwrap())
        };
        assert_eq!(response.pages[0].navigation[0].target.subcode, expected);
    }
    let mut value = document();
    value["pages"][0]["navigation"][0]["target"]
        .as_object_mut()
        .unwrap()
        .remove("subcode");
    assert_eq!(parse(&value), Err(TeletextPagesError::InvalidJson));
    for subcode in [json!("0"), json!(-1), json!(65_536), json!({})] {
        value["pages"][0]["navigation"][0]["target"]["subcode"] = subcode;
        assert_eq!(parse(&value), Err(TeletextPagesError::InvalidJson));
    }
}

#[test]
fn duplicates_are_rejected_before_any_tagged_or_nested_object_can_overwrite_them() {
    for (field, replacement) in [
        (
            "\"schema_version\": 1",
            "\"schema_version\": 1, \"schema_version\": 1",
        ),
        (
            "\"generation\": \"fixture-generation\"",
            "\"generation\": \"fixture-generation\", \"generation\": \"other\"",
        ),
        ("\"page\": 200", "\"page\": 200, \"page\": 201"),
        (
            "\"kind\": \"text\"",
            "\"kind\": \"text\", \"kind\": \"mosaics\"",
        ),
        (
            "\"kind\": \"text\"",
            "\"kind\": \"text\", \"\\u006b\\u0069\\u006e\\u0064\": \"text\"",
        ),
        (
            "\"text\": \"Literal café\"",
            "\"text\": \"first\", \"text\": \"second\"",
        ),
        ("\"cells\": [0, 63]", "\"cells\": [0], \"cells\": [63]"),
        ("\"boxed\": false", "\"boxed\": false, \"boxed\": true"),
        ("\"subcode\": null", "\"subcode\": null, \"subcode\": 0"),
    ] {
        assert!(RESPONSE.contains(field));
        let bytes = RESPONSE.replacen(field, replacement, 1);
        assert_eq!(
            TeletextPagesResponse::from_json(bytes.as_bytes(), GENERATION),
            Err(TeletextPagesError::DuplicateKey)
        );
    }
    for nested in [r#"{"":0,"":1}"#, r#"{"outer":[{"inner":{"x":0,"x":1}}]}"#] {
        let bytes = RESPONSE.replacen('{', &format!("{{\"unexpected\":{nested},"), 1);
        assert_eq!(
            TeletextPagesResponse::from_json(bytes.as_bytes(), GENERATION),
            Err(TeletextPagesError::DuplicateKey)
        );
    }
}

#[test]
fn the_same_key_in_distinct_objects_is_not_a_duplicate() {
    let mut value = document();
    let repeated_page = value["pages"][0].clone();
    value["pages"].as_array_mut().unwrap().push(repeated_page);
    assert_eq!(parse(&value).unwrap().pages.len(), 2);
    // Duplicate page identity is deliberately still a host structural check.
}

#[test]
fn generation_is_opaque_nonempty_byte_bounded_and_echoed_exactly() {
    for generation in [" ".to_owned(), "Mixed Case \n".to_owned(), "é".repeat(512)] {
        let mut value = document();
        value["generation"] = json!(generation);
        let bytes = serde_json::to_vec(&value).unwrap();
        assert_eq!(
            TeletextPagesResponse::from_json(&bytes, &generation)
                .unwrap()
                .generation,
            generation
        );
    }
    for generation in [String::new(), "a".repeat(1025), "é".repeat(513)] {
        let mut value = document();
        value["generation"] = json!(generation);
        let bytes = serde_json::to_vec(&value).unwrap();
        assert_eq!(
            TeletextPagesResponse::from_json(&bytes, &generation),
            Err(TeletextPagesError::InvalidGeneration)
        );
    }
    for expected in [
        "Fixture-generation",
        "fixture-generation ",
        "fixture-generatioN",
    ] {
        assert_eq!(
            TeletextPagesResponse::from_json(RESPONSE.as_bytes(), expected),
            Err(TeletextPagesError::GenerationMismatch)
        );
    }
    for expected in [String::new(), "a".repeat(1025)] {
        assert_eq!(
            TeletextPagesResponse::from_json(RESPONSE.as_bytes(), &expected),
            Err(TeletextPagesError::InvalidGeneration)
        );
    }
    let mut value = document();
    value["generation"] = json!("é");
    assert_eq!(
        TeletextPagesResponse::from_json(&serde_json::to_vec(&value).unwrap(), "e\u{301}"),
        Err(TeletextPagesError::GenerationMismatch)
    );
}

#[test]
fn schema_version_requires_explicit_supported_integer_and_static_errors() {
    for version in [0, 2, u32::MAX] {
        let mut value = document();
        value["schema_version"] = json!(version);
        assert_eq!(parse(&value), Err(TeletextPagesError::UnsupportedSchema));
    }
    for version in [Value::Null, json!("1"), json!(-1), json!(1.5)] {
        let mut value = document();
        value["schema_version"] = version;
        assert_eq!(parse(&value), Err(TeletextPagesError::InvalidJson));
    }
    let error =
        TeletextPagesResponse::from_json(br#"{"private-payload":"private-value"}"#, GENERATION)
            .unwrap_err();
    assert_eq!(error, TeletextPagesError::InvalidJson);
    assert!(!error.to_string().contains("private"));
    assert!(!format!("{error:?}").contains("private"));
}

#[test]
fn raw_cap_counts_whitespace_and_is_checked_before_utf8_or_json() {
    let mut bytes = RESPONSE.as_bytes().to_vec();
    bytes.resize(MAX_TELETEXT_PAGES_OUTPUT_BYTES, b' ');
    assert!(TeletextPagesResponse::from_json(&bytes, GENERATION).is_ok());
    bytes.push(b' ');
    assert_eq!(
        TeletextPagesResponse::from_json(&bytes, GENERATION),
        Err(TeletextPagesError::OutputTooLarge)
    );
    bytes.fill(0xff);
    assert_eq!(
        TeletextPagesResponse::from_json(&bytes, GENERATION),
        Err(TeletextPagesError::OutputTooLarge)
    );
    assert_eq!(
        TeletextPagesResponse::from_json(&[0xff], GENERATION),
        Err(TeletextPagesError::InvalidUtf8)
    );
}

#[test]
fn page_record_count_is_bounded_without_claiming_unique_or_valid_pages() {
    for count in [
        0,
        1,
        MAX_TELETEXT_PAGES_RECORDS,
        MAX_TELETEXT_PAGES_RECORDS + 1,
    ] {
        let mut value = document();
        let page = value["pages"][0].clone();
        value["pages"] = json!(vec![page; count]);
        let parsed = parse(&value);
        if (1..=MAX_TELETEXT_PAGES_RECORDS).contains(&count) {
            assert_eq!(parsed.unwrap().pages.len(), count);
        } else {
            assert_eq!(parsed, Err(TeletextPagesError::PageCount));
        }
    }
}

#[test]
fn trailing_json_ndjson_invalid_syntax_and_excessive_nesting_are_rejected() {
    for tail in ["{}", "\n{}\n", "null", "junk"] {
        let bytes = format!("{RESPONSE}{tail}");
        assert_eq!(
            TeletextPagesResponse::from_json(bytes.as_bytes(), GENERATION),
            Err(TeletextPagesError::InvalidJson)
        );
    }
    for bytes in [b"".as_slice(), b"{", b"[1,]", b"{\"x\":NaN}"] {
        assert_eq!(
            TeletextPagesResponse::from_json(bytes, GENERATION),
            Err(TeletextPagesError::InvalidJson)
        );
    }
    let deep = format!("{}0{}", "[".repeat(256), "]".repeat(256));
    assert_eq!(
        TeletextPagesResponse::from_json(deep.as_bytes(), GENERATION),
        Err(TeletextPagesError::InvalidJson)
    );
}

#[test]
fn strings_are_literal_and_domain_layout_or_value_checks_are_not_fabricated() {
    let literal = "<script>alert('x')</script> drawvg: rotate(90) %{metadata:title} café\n";
    let mut value = document();
    value["pages"][0]["rows"][0]["runs"][0]["content"]["text"] = json!(literal);
    value["pages"][0]["address"]["page"] = json!(0);
    value["pages"][0]["address"]["subcode"] = json!(u16::MAX);
    value["pages"][0]["language"] = json!("en-US");
    value["pages"][0]["national_option"] = json!(255);
    value["pages"][0]["rows"][0]["row"] = json!(255);
    value["pages"][0]["rows"][0]["runs"][0]["column"] = json!(255);
    value["pages"][0]["rows"][0]["runs"][1]["content"]["cells"] = json!([255]);
    let response = parse(&value).unwrap();
    assert_eq!(
        response.pages[0].rows[0].runs[0].content,
        TeletextRunContentWire::Text {
            text: literal.to_owned()
        }
    );
    assert_eq!(response.pages[0].national_option, 255);
    assert_eq!(response.pages[0].address.subcode, u16::MAX);
    assert_eq!(
        response.pages[0].rows[0].runs[1].content,
        TeletextRunContentWire::Mosaics {
            cells: vec![255],
            separated: true
        }
    );
}

#[test]
fn typed_enums_and_scalar_widths_reject_unknown_or_malformed_wire_values() {
    for path in [
        "/pages/0/kind",
        "/pages/0/rows/0/runs/0/style/foreground",
        "/pages/0/rows/0/runs/0/style/background",
        "/pages/0/rows/0/runs/0/style/height",
        "/pages/0/rows/0/runs/0/content/kind",
        "/pages/0/navigation/0/role",
    ] {
        let mut value = document();
        *value.pointer_mut(path).unwrap() = json!("unknown");
        assert_eq!(parse(&value), Err(TeletextPagesError::InvalidJson));
    }
    for path in [
        "/initial_page",
        "/pages/0/address/page",
        "/pages/0/address/subcode",
    ] {
        let mut value = document();
        *value.pointer_mut(path).unwrap() = json!(65_536);
        assert_eq!(parse(&value), Err(TeletextPagesError::InvalidJson));
    }
    let mut value = document();
    value["pages"][0]["rows"][0]["runs"][0]["style"]["boxed"] = Value::Null;
    assert_eq!(parse(&value), Err(TeletextPagesError::InvalidJson));
}
