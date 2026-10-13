use super::*;
use serde_json::{json, Value};

const GENERATION: &str = "synthetic-v3-generation";

fn document() -> Value {
    json!({
        "schema_version":3,
        "service": {
            "schema_version":2, "generation":GENERATION, "language":"eng",
            "initial_page":200,
            "pages":[{"address":{"page":200,"subcode":7},"kind":"full_page",
                "language":"eng","national_option":0,"rows":[],"navigation":[]}],
            "page_enhancements":[],"magazines":[],"objects":[],"drcs":[],
            "mots":[],"mot_selections":[],"empty_object_pages":[]
        },
        "authoring": {
            "schema_version":1,
            "template":{"id":"guide-template","revision":"r2.1"},
            "theme":{"id":"night","overrides":[
                {"key":"accent","value":{"kind":"colour","red":15,"green":8,"blue":0}},
                {"key":"grid","value":{"kind":"layout","id":"compact"}},
                {"key":"spacing","value":{"kind":"number","value":-2}},
                {"key":"heading","value":{"kind":"boolean","value":true}}
            ]},
            "pages":[{"page":{"page":200,"subcode":7},"regions":[
                {"id":"header","kind":"free","area":"main","row":0,"column":8,"width":32,"height":1},
                {"id":"headline","kind":"zone","area":"main","row":1,"column":0,"width":40,"height":1},
                {"id":"guide","kind":"generated","area":"main","row":2,"column":0,"width":40,"height":23},
                {"id":"left","kind":"zone","area":"left_panel","row":0,"column":0,"width":16,"height":25},
                {"id":"right","kind":"free","area":"right_panel","row":0,"column":0,"width":16,"height":25}
            ],"dynamic_fields":[{"id":"programme-title","region":"guide","binding":"guide.title"}]}]
        }
    })
}

fn parse(value: &Value) -> Result<TeletextPagesV3Response, TeletextPagesV3Error> {
    TeletextPagesV3Response::from_json(&serde_json::to_vec(value).unwrap(), GENERATION)
}

fn metadata_kind(value: &Value) -> TeletextAuthoringMetadataErrorKind {
    match parse(value).unwrap_err() {
        TeletextPagesV3Error::Authoring(error) => error.kind,
        other => panic!("expected metadata rejection, got {other:?}"),
    }
}

#[test]
fn exact_metadata_roundtrip_retains_sidepanels_bindings_and_typed_theme() {
    let value = document();
    let response = parse(&value).unwrap();
    assert_eq!(serde_json::to_value(&response).unwrap(), value);
    assert_eq!(
        parse(&serde_json::to_value(&response).unwrap()).unwrap(),
        response
    );
    assert_eq!(
        response.authoring.pages[0].regions[3].area,
        TeletextAuthoringRegionAreaWire::LeftPanel
    );
    assert_eq!(
        response.authoring.pages[0].regions[4].area,
        TeletextAuthoringRegionAreaWire::RightPanel
    );
    let mut sparse = value;
    sparse["authoring"]["pages"][0]["regions"] = json!([]);
    sparse["authoring"]["pages"][0]["dynamic_fields"] = json!([]);
    assert!(parse(&sparse).is_ok(), "undeclared cells remain protected");
}

#[test]
fn envelope_requires_explicit_version_exact_generation_and_existing_v2_service() {
    let value = document();
    let bytes = serde_json::to_vec(&value).unwrap();
    assert!(matches!(
        TeletextPagesV2Response::from_json(&bytes, GENERATION),
        Err(TeletextPagesV2Error::Contract(
            TeletextPagesError::InvalidJson
        ))
    ));
    assert_eq!(
        TeletextPagesV3Response::from_json(&bytes, "different"),
        Err(TeletextPagesV3Error::Service(
            TeletextPagesV2Error::Contract(TeletextPagesError::GenerationMismatch)
        ))
    );
    assert_eq!(
        TeletextPagesV3Response::from_json(&bytes, ""),
        Err(TeletextPagesV3Error::Service(
            TeletextPagesV2Error::Contract(TeletextPagesError::InvalidGeneration)
        ))
    );
    for version in [0, 1, 2, 4] {
        let mut invalid = value.clone();
        invalid["schema_version"] = json!(version);
        assert_eq!(
            parse(&invalid),
            Err(TeletextPagesV3Error::Contract(
                TeletextPagesError::UnsupportedSchema
            ))
        );
    }
    let mut invalid = value;
    invalid["service"]["schema_version"] = json!(3);
    assert_eq!(
        parse(&invalid),
        Err(TeletextPagesV3Error::Service(
            TeletextPagesV2Error::Contract(TeletextPagesError::UnsupportedSchema)
        ))
    );
}

#[test]
fn authoring_objects_tagged_values_addresses_and_string_enums_are_closed_maps() {
    let original = document();
    for pointer in [
        "",
        "/authoring",
        "/authoring/template",
        "/authoring/theme",
        "/authoring/theme/overrides/0",
        "/authoring/theme/overrides/0/value",
        "/authoring/pages/0",
        "/authoring/pages/0/page",
        "/authoring/pages/0/regions/0",
        "/authoring/pages/0/dynamic_fields/0",
    ] {
        let mut unknown = original.clone();
        unknown
            .pointer_mut(pointer)
            .unwrap()
            .as_object_mut()
            .unwrap()
            .insert("unknown".into(), json!(true));
        assert_eq!(
            parse(&unknown),
            Err(TeletextPagesV3Error::Contract(
                TeletextPagesError::InvalidJson
            )),
            "{pointer}"
        );
        let mut array = original.clone();
        *array.pointer_mut(pointer).unwrap() = json!([]);
        assert_eq!(
            parse(&array),
            Err(TeletextPagesV3Error::Contract(
                TeletextPagesError::InvalidJson
            )),
            "{pointer}"
        );
    }
    for pointer in [
        "/authoring/pages/0/regions/0/kind",
        "/authoring/pages/0/regions/0/area",
    ] {
        let mut map = original.clone();
        let old = map.pointer(pointer).unwrap().as_str().unwrap().to_owned();
        *map.pointer_mut(pointer).unwrap() = json!({old:null});
        assert!(parse(&map).is_err());
    }
    let bytes = serde_json::to_string(&original).unwrap();
    for (from, to) in [
        (
            "\"schema_version\":3",
            "\"schema_version\":3,\"schema_version\":3",
        ),
        (
            "\"binding\":\"guide.title\"",
            "\"binding\":\"guide.title\",\"binding\":\"guide.title\"",
        ),
        ("\"red\":15", "\"red\":15,\"red\":15"),
    ] {
        let duplicated = bytes.replace(from, to);
        assert_ne!(duplicated, bytes);
        assert_eq!(
            TeletextPagesV3Response::from_json(duplicated.as_bytes(), GENERATION),
            Err(TeletextPagesV3Error::Contract(
                TeletextPagesError::DuplicateKey
            ))
        );
    }
}

#[test]
fn symbolic_ids_colours_unique_overrides_and_metadata_schema_are_validated() {
    use TeletextAuthoringMetadataErrorKind as K;
    let original = document();
    for pointer in [
        "/authoring/template/id",
        "/authoring/template/revision",
        "/authoring/theme/id",
        "/authoring/theme/overrides/0/key",
        "/authoring/theme/overrides/1/value/id",
        "/authoring/pages/0/regions/0/id",
        "/authoring/pages/0/dynamic_fields/0/id",
        "/authoring/pages/0/dynamic_fields/0/region",
        "/authoring/pages/0/dynamic_fields/0/binding",
    ] {
        for invalid in [
            String::new(),
            "../private/path".to_owned(),
            "unicode-é".to_owned(),
            "x\n".to_owned(),
            "x".repeat(129),
        ] {
            let mut value = original.clone();
            *value.pointer_mut(pointer).unwrap() = json!(invalid);
            assert_eq!(metadata_kind(&value), K::Identifier, "{pointer}");
        }
    }
    for component in ["red", "green", "blue"] {
        let mut invalid = original.clone();
        invalid["authoring"]["theme"]["overrides"][0]["value"][component] = json!(16);
        assert_eq!(metadata_kind(&invalid), K::Colour);
    }
    let mut duplicate = original.clone();
    duplicate["authoring"]["theme"]["overrides"][1]["key"] = json!("accent");
    assert_eq!(metadata_kind(&duplicate), K::DuplicateIdentifier);
    let mut wrong_version = original;
    wrong_version["authoring"]["schema_version"] = json!(2);
    assert_eq!(metadata_kind(&wrong_version), K::UnsupportedSchema);
}

#[test]
fn exact_page_subpage_coverage_and_unique_original_addresses_are_required() {
    use TeletextAuthoringMetadataErrorKind as K;
    for pointer in [
        "/authoring/pages/0/page/page",
        "/authoring/pages/0/page/subcode",
    ] {
        let mut invalid = document();
        *invalid.pointer_mut(pointer).unwrap() = json!(8);
        assert_eq!(metadata_kind(&invalid), K::PageCoverage);
    }
    let mut missing = document();
    missing["authoring"]["pages"] = json!([]);
    assert_eq!(metadata_kind(&missing), K::PageCoverage);
    let mut duplicate = document();
    let original = duplicate["service"]["pages"][0].clone();
    let metadata = duplicate["authoring"]["pages"][0].clone();
    duplicate["service"]["pages"]
        .as_array_mut()
        .unwrap()
        .push(original);
    duplicate["authoring"]["pages"]
        .as_array_mut()
        .unwrap()
        .push(metadata);
    assert_eq!(metadata_kind(&duplicate), K::PageCoverage);
    duplicate["service"]["pages"][1]["address"]["subcode"] = json!(8);
    assert_eq!(metadata_kind(&duplicate), K::PageCoverage);
}

#[test]
fn rectangles_are_nonempty_nonoverlapping_and_protect_receiver_owned_header() {
    use TeletextAuthoringMetadataErrorKind as K;
    for (field, value) in [
        ("width", 0),
        ("height", 0),
        ("row", 25),
        ("column", 40),
        ("width", 255),
        ("height", 255),
    ] {
        let mut invalid = document();
        invalid["authoring"]["pages"][0]["regions"][1][field] = json!(value);
        assert_eq!(metadata_kind(&invalid), K::Rectangle);
    }
    for kind in ["free", "zone"] {
        let mut invalid = document();
        invalid["authoring"]["pages"][0]["regions"][0]["kind"] = json!(kind);
        invalid["authoring"]["pages"][0]["regions"][0]["column"] = json!(7);
        assert_eq!(metadata_kind(&invalid), K::Header);
    }
    let mut generated_header = document();
    generated_header["authoring"]["pages"][0]["regions"][0]["kind"] = json!("generated");
    generated_header["authoring"]["pages"][0]["regions"][0]["column"] = json!(0);
    assert!(parse(&generated_header).is_ok());
    for panel in [3usize, 4] {
        let mut invalid = document();
        invalid["authoring"]["pages"][0]["regions"][panel]["column"] = json!(1);
        assert_eq!(metadata_kind(&invalid), K::Rectangle);
    }
    let mut overlap = document();
    overlap["authoring"]["pages"][0]["regions"][1]["height"] = json!(2);
    assert_eq!(metadata_kind(&overlap), K::Overlap);
    let mut duplicate = document();
    duplicate["authoring"]["pages"][0]["regions"][1]["id"] = json!("header");
    assert_eq!(metadata_kind(&duplicate), K::DuplicateIdentifier);
}

#[test]
fn dynamic_fields_are_unique_and_reference_only_generated_regions() {
    use TeletextAuthoringMetadataErrorKind as K;
    for reference in ["missing", "headline", "header"] {
        let mut invalid = document();
        invalid["authoring"]["pages"][0]["dynamic_fields"][0]["region"] = json!(reference);
        assert_eq!(metadata_kind(&invalid), K::DynamicBinding);
    }
    let mut invalid = document();
    let field = invalid["authoring"]["pages"][0]["dynamic_fields"][0].clone();
    invalid["authoring"]["pages"][0]["dynamic_fields"]
        .as_array_mut()
        .unwrap()
        .push(field);
    assert_eq!(metadata_kind(&invalid), K::DuplicateIdentifier);
}

#[test]
fn metadata_raw_collections_and_aggregate_totals_are_bounded_without_truncation() {
    use TeletextAuthoringMetadataErrorKind as K;
    let original = document();
    for pointer in [
        "/authoring/theme/overrides",
        "/authoring/pages/0/regions",
        "/authoring/pages/0/dynamic_fields",
    ] {
        let mut invalid = original.clone();
        let item = invalid.pointer(pointer).unwrap()[0].clone();
        *invalid.pointer_mut(pointer).unwrap() = json!(vec![item; 257]);
        assert_eq!(metadata_kind(&invalid), K::RecordCount, "{pointer}");
    }
    let mut total = parse(&original).unwrap();
    let original_page = total.service.pages[0].clone();
    let metadata_page = total.authoring.pages[0].clone();
    total.service.pages.clear();
    total.authoring.pages.clear();
    for ordinal in 0..17u16 {
        let mut page = original_page.clone();
        page.address.subcode = ordinal;
        let mut metadata = metadata_page.clone();
        metadata.page = page.address;
        metadata.regions = (0..256u16)
            .map(|cell| TeletextAuthoringRegionWire {
                id: format!("cell-{cell}"),
                kind: TeletextAuthoringRegionKindWire::Generated,
                area: TeletextAuthoringRegionAreaWire::Main,
                row: u8::try_from(1 + cell / 40).unwrap(),
                column: u8::try_from(cell % 40).unwrap(),
                width: 1,
                height: 1,
            })
            .collect();
        metadata.dynamic_fields.clear();
        total.service.pages.push(page);
        total.authoring.pages.push(metadata);
    }
    assert_eq!(
        total
            .authoring
            .validate_against(&total.service.pages)
            .unwrap_err()
            .kind,
        K::RecordCount
    );
    for page in &mut total.authoring.pages {
        page.regions.truncate(1);
        page.dynamic_fields = (0..256)
            .map(|field| TeletextAuthoringDynamicFieldWire {
                id: format!("field-{field}"),
                region: "cell-0".into(),
                binding: "guide.title".into(),
            })
            .collect();
    }
    assert_eq!(
        total
            .authoring
            .validate_against(&total.service.pages)
            .unwrap_err()
            .kind,
        K::RecordCount
    );
    total.service.pages.pop();
    total.authoring.pages.pop();
    assert!(total
        .authoring
        .validate_against(&total.service.pages)
        .is_ok());
}

#[test]
fn raw_envelope_bounds_and_all_metadata_debug_diagnostics_are_content_free() {
    assert_eq!(
        TeletextPagesV3Response::from_json(
            &vec![b' '; MAX_TELETEXT_PAGES_OUTPUT_BYTES + 1],
            GENERATION
        ),
        Err(TeletextPagesV3Error::Contract(
            TeletextPagesError::OutputTooLarge
        ))
    );
    assert_eq!(
        TeletextPagesV3Response::from_json(&[0xff], GENERATION),
        Err(TeletextPagesV3Error::Contract(
            TeletextPagesError::InvalidUtf8
        ))
    );
    let response = parse(&document()).unwrap();
    let metadata = &response.authoring;
    let debug = format!(
        "{response:?} {metadata:?} {:?} {:?} {:?} {:?} {:?} {:?} {:?}",
        metadata.template,
        metadata.theme,
        metadata.theme.overrides[0],
        metadata.theme.overrides[0].value,
        metadata.pages[0],
        metadata.pages[0].regions[0],
        metadata.pages[0].dynamic_fields[0]
    );
    for secret in [
        GENERATION,
        "guide-template",
        "night",
        "accent",
        "programme-title",
        "guide.title",
        "r2.1",
    ] {
        assert!(
            !debug.contains(secret),
            "debug leaked an arbitrary identifier"
        );
    }
    let mut invalid = document();
    invalid["authoring"]["pages"][0]["regions"][1]["id"] = json!("private/path");
    let error = parse(&invalid).unwrap_err();
    assert!(!format!("{error:?} {error}").contains("private/path"));
    assert!(matches!(
        error,
        TeletextPagesV3Error::Authoring(TeletextAuthoringMetadataError {
            page: Some(0),
            record: Some(1),
            kind: TeletextAuthoringMetadataErrorKind::Identifier
        })
    ));
}
