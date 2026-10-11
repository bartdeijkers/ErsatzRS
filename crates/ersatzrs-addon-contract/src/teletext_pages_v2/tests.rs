//! Wire regressions for enhanced documents; fixtures grant no publication authority.

use super::*;
use serde_json::{json, Value};

const GENERATION: &str = "synthetic-v2-generation";

fn document() -> Value {
    json!({
        "schema_version": 2, "generation": GENERATION, "language": "eng",
        "initial_page": 200,
        "pages": [{"address": {"page":200,"subcode":0},"kind":"full_page",
            "language":"eng","national_option":0,"rows":[],"navigation":[]}],
        "page_enhancements": [{"address":{"page":200,"subcode":0},
            "presentation":null,"compositional_links":null,"local_program":null,
            "resources":{"objects":{"public":null,"global":null},
                "drcs":{"normal":null,"global":null},"defaults":[null,null,null,null]}}],
        "magazines": [], "objects": [], "drcs": [], "mots": [],
        "mot_selections": [{"display":{"page":200,"subcode":0},"mot":null}],
        "empty_object_pages": []
    })
}

fn parse(value: &Value) -> Result<TeletextPagesV2Response, TeletextPagesV2Error> {
    TeletextPagesV2Response::from_json(&serde_json::to_vec(value).unwrap(), GENERATION)
}

// Deliberately raw declarations: wire parsing is not semantic admission.
fn enhanced_document() -> Value {
    let mut value = document();
    let raw_page = json!({"magazine":2,"page":192});
    let resource = json!({"kind":"pop","page":raw_page,"subpage":15});
    let local = json!({"scope":"local","owner":{"page":200,"subcode":7},"object_id":65535});
    let public = json!({"scope":"public","resource":resource,"object_id":500});
    let global = json!({"scope":"global","resource":{
        "kind":"gpop","page":{"magazine":8,"page":160},"subpage":3},"object_id":500});
    let operations = vec![
        json!({"kind":"foreground","colour":16}),
        json!({"kind":"background","colour":31}),
        json!({"kind":"designate_charset","designation":8}),
        json!({"kind":"g0","code":65}),
        json!({"kind":"g1","mosaic":63}),
        json!({"kind":"g1_alpha","code":64}),
        json!({"kind":"g2","code":127}),
        json!({"kind":"g3","code":32,"compatibility":"level15"}),
        json!({"kind":"g0_diacritic","code":101,"diacritic":2}),
        json!({"kind":"drcs","reference":{"table":"normal","subtable":15,"slot":47}}),
        json!({"kind":"attributes","double_height":true,"double_width":true,
            "boxed":true,"conceal":true,"invert":true,"underline_or_separated_mosaics":true}),
        json!({"kind":"flash","mode":"adjacent_clut","rate":"fast2_hz_incremental"}),
    ];
    let mut commands = vec![
        json!({"kind":"set_position","position":{"row":0,"column":0}}),
        json!({"kind":"set_screen_colour","colour":23}),
        json!({"kind":"set_row_colour","row":24,"colour":20,"following_rows":true}),
    ];
    for operation in operations {
        commands.push(json!({"kind":"at_column","column":2,"operation":operation}));
    }
    commands
        .push(json!({"kind":"invoke_object","target":public,"object_type":"active","offset":null}));
    commands.push(
        json!({"kind":"invoke_object","target":global,"object_type":"passive",
        "offset":{"row":1,"column":71}}),
    );
    value["page_enhancements"][0]["local_program"] = json!({"commands":commands});
    let presentation = json!({"primary":8,"secondary":null,
        "palette":{"clut2":vec![[15,0,5];8],"clut3":vec![[3,3,3];8]},
        "screen_colour":16,"row_colour":23,"black_background_substitution":true,
        "remapping":"fg_clut2_bg_clut3",
        "panels":{"left":true,"right":true,"status":"level25_and35","width":8}});
    value["page_enhancements"][0]["presentation"] = presentation.clone();
    value["magazines"] = json!([{"magazine":2,"presentation":presentation}]);
    let target = json!({"kind":"unspecified","magazine":8});
    let link = json!({"target":target,"validity":"level25_and35","required_subpages":65535});
    value["page_enhancements"][0]["compositional_links"] = json!({
        "gpop":link,"pop":link,"gdrcs":link,"drcs":link});
    value["objects"] = json!([{"identity":local,"object_type":"adaptive","program":{"commands":[]}},
        {"identity":public,"object_type":"active","program":{"commands":[]}},
        {"identity":global,"object_type":"passive","program":{"commands":[]}}]);
    value["drcs"] = json!([{"identity":{"kind":"drcs","page":{"magazine":2,"page":208},"subpage":15},
        "glyphs":[{"slot":47,"bitmap":[4095,1,2,4,8,16,32,64,128,2048]}]}]);
    let object_link = json!({"target":target,"extent":15,"supersedable":true,"raw_fallback":15,
        "defaults":[{"kind":"no_object","raw_number":255},
            {"kind":"dormant","object_type":"passive","raw_number":234}]});
    let drcs_link = json!({"target":target,"extent":15,"supersedable":true});
    let mot = json!({"kind":"mot","page":{"magazine":2,"page":254},"subpage":7});
    value["mots"] = json!([{"identity":mot,
        "associations":[{"page":255,"objects":{"global":true,"public":7},"drcs":{"global":false,"public":null}}],
        "links":{"global_object":object_link,"public_objects":vec![object_link;7],
            "global_drcs":drcs_link,"normal_drcs":vec![drcs_link;7],
            "counts":{"magazine_mask":255,"object_pages":255,"drcs_pages":255}}}]);
    value["mot_selections"][0]["mot"] = mot;
    value["empty_object_pages"] = json!([resource]);
    value
}

#[test]
fn enhanced_wire_preserves_all_program_variants_aliases_and_original_declarations() {
    let original = enhanced_document();
    let parsed = parse(&original).unwrap();
    assert_eq!(serde_json::to_value(&parsed).unwrap(), original);
    assert_eq!(
        parsed.page_enhancements[0]
            .presentation
            .as_ref()
            .unwrap()
            .primary,
        8
    );
    assert_eq!(parsed.mots[0].identity.subpage, 7);
    assert_eq!(parsed.drcs[0].identity.subpage, 15);
    assert_eq!(parsed.drcs[0].glyphs[0].bitmap[0], 4095);
    assert_eq!(parsed.mots[0].associations[0].page, 255);
    assert_eq!(parsed.mots[0].links.public_objects[6].raw_fallback, 15);
    assert_eq!(
        parsed.mots[0].links.global_object.defaults[1],
        TeletextMotDefaultObjectWire::Dormant {
            object_type: TeletextObjectTypeWire::Passive,
            raw_number: 234
        }
    );
}

fn enhanced_object_paths(value: &Value, prefix: &str, paths: &mut Vec<String>) {
    if prefix.starts_with("/pages") {
        return;
    } // Exact legacy shape remains unchanged.
    match value {
        Value::Object(fields) => {
            paths.push(prefix.into());
            for (key, nested) in fields {
                enhanced_object_paths(nested, &format!("{prefix}/{key}"), paths);
            }
        }
        Value::Array(items) => {
            for (ordinal, nested) in items.iter().enumerate() {
                enhanced_object_paths(nested, &format!("{prefix}/{ordinal}"), paths);
            }
        }
        _ => {}
    }
}

#[test]
fn every_enhanced_object_is_closed_map_only_with_no_implicit_fields() {
    let original = enhanced_document();
    let mut paths = Vec::new();
    enhanced_object_paths(&original, "", &mut paths);
    assert!(paths.len() > 100);
    for path in paths {
        for field in original.pointer(&path).unwrap().as_object().unwrap().keys() {
            let mut missing = original.clone();
            missing
                .pointer_mut(&path)
                .unwrap()
                .as_object_mut()
                .unwrap()
                .remove(field);
            assert_eq!(
                parse(&missing),
                Err(TeletextPagesError::InvalidJson.into()),
                "missing {path}/{field}"
            );
        }
        let mut unknown = original.clone();
        unknown
            .pointer_mut(&path)
            .unwrap()
            .as_object_mut()
            .unwrap()
            .insert("unexpected".into(), json!(1));
        assert_eq!(
            parse(&unknown),
            Err(TeletextPagesError::InvalidJson.into()),
            "unknown {path}"
        );
        let mut array = original.clone();
        let fields = original
            .pointer(&path)
            .unwrap()
            .as_object()
            .unwrap()
            .values()
            .cloned()
            .collect();
        *array.pointer_mut(&path).unwrap() = Value::Array(fields);
        assert_eq!(
            parse(&array),
            Err(TeletextPagesError::InvalidJson.into()),
            "array {path}"
        );
    }
}

#[test]
fn enhanced_wire_rejects_tag_first_positional_variants() {
    for (path, positional) in [
        (
            "/page_enhancements/0/local_program/commands/3",
            json!(["at_column", 2, ["g0", 65]]),
        ),
        (
            "/page_enhancements/0/local_program/commands/3/operation",
            json!(["foreground", 16]),
        ),
        (
            "/objects/0/identity",
            json!(["local",{"page":200,"subcode":7},65535]),
        ),
        (
            "/page_enhancements/0/compositional_links/pop/target",
            json!(["unspecified", 8]),
        ),
        (
            "/mots/0/links/global_object/defaults/0",
            json!(["no_object", 255]),
        ),
    ] {
        let mut value = enhanced_document();
        *value.pointer_mut(path).unwrap() = positional;
        assert_eq!(
            parse(&value),
            Err(TeletextPagesError::InvalidJson.into()),
            "{path}"
        );
    }
}

#[test]
fn enhanced_scalar_enum_fields_require_assigned_strings() {
    for (path, name) in [
        ("/objects/0/object_type", "adaptive"),
        ("/drcs/0/identity/kind", "drcs"),
        (
            "/page_enhancements/0/presentation/remapping",
            "fg_clut2_bg_clut3",
        ),
        (
            "/page_enhancements/0/presentation/panels/status",
            "level25_and35",
        ),
        (
            "/page_enhancements/0/compositional_links/pop/validity",
            "level25_and35",
        ),
        (
            "/page_enhancements/0/local_program/commands/10/operation/compatibility",
            "level15",
        ),
        (
            "/page_enhancements/0/local_program/commands/12/operation/reference/table",
            "normal",
        ),
        (
            "/page_enhancements/0/local_program/commands/14/operation/mode",
            "adjacent_clut",
        ),
        (
            "/page_enhancements/0/local_program/commands/14/operation/rate",
            "fast2_hz_incremental",
        ),
    ] {
        let original = enhanced_document();
        assert_eq!(original.pointer(path).unwrap(), &json!(name));
        for invalid in [
            json!({name:null}),
            json!("unknown_value"),
            json!([name]),
            json!(null),
            json!(0),
        ] {
            let mut value = original.clone();
            *value.pointer_mut(path).unwrap() = invalid;
            assert_eq!(
                parse(&value),
                Err(TeletextPagesError::InvalidJson.into()),
                "{path}"
            );
        }
    }
}

#[test]
fn enhanced_wire_accepts_1024_distinct_fallback_pages_with_their_metadata() {
    let original = document();
    let mut value = original.clone();
    let mut pages = Vec::new();
    let mut metadata = Vec::new();
    let mut selections = Vec::new();
    for ordinal in 0..MAX_TELETEXT_PAGES_RECORDS {
        let address = json!({"page":200,"subcode":u16::try_from(ordinal).unwrap()});
        let mut page = original["pages"][0].clone();
        page["address"] = address.clone();
        pages.push(page);
        let mut entry = original["page_enhancements"][0].clone();
        entry["address"] = address.clone();
        metadata.push(entry);
        let mut selection = original["mot_selections"][0].clone();
        selection["display"] = address;
        selections.push(selection);
    }
    value["pages"] = Value::Array(pages);
    value["page_enhancements"] = Value::Array(metadata);
    value["mot_selections"] = Value::Array(selections);
    let parsed = parse(&value).unwrap();
    assert_eq!(parsed.pages.len(), 1024);
    assert_eq!(parsed.page_enhancements.len(), 1024);
    assert_eq!(parsed.mot_selections.len(), 1024);
    for field in [
        "page_enhancements",
        "magazines",
        "objects",
        "drcs",
        "mots",
        "mot_selections",
        "empty_object_pages",
    ] {
        let mut overflow = enhanced_document();
        let record = overflow[field][0].clone();
        overflow[field] = Value::Array(vec![record; MAX_TELETEXT_PAGES_RECORDS + 1]);
        assert_eq!(
            parse(&overflow),
            Err(TeletextPagesV2Error::RecordCount),
            "{field}"
        );
    }
}

#[test]
fn enhanced_null_and_present_empty_program_are_distinct_and_roundtrip() {
    let absent = parse(&document()).unwrap();
    let mut present = document();
    present["page_enhancements"][0]["local_program"] = json!({"commands":[]});
    let present = parse(&present).unwrap();
    assert_ne!(absent, present);
    assert!(absent.page_enhancements[0].local_program.is_none());
    assert!(present.page_enhancements[0]
        .local_program
        .as_ref()
        .unwrap()
        .commands
        .is_empty());
    for draft in [absent, present] {
        assert_eq!(
            parse(&serde_json::to_value(&draft).unwrap()).unwrap(),
            draft
        );
    }
}

#[test]
fn enhanced_wire_requires_all_fields_including_explicit_nulls() {
    for path in [
        "",
        "/page_enhancements/0",
        "/page_enhancements/0/resources",
        "/page_enhancements/0/resources/objects",
        "/page_enhancements/0/resources/drcs",
        "/mot_selections/0",
    ] {
        let original = document();
        for field in original.pointer(path).unwrap().as_object().unwrap().keys() {
            let mut missing = original.clone();
            missing
                .pointer_mut(path)
                .unwrap()
                .as_object_mut()
                .unwrap()
                .remove(field);
            assert_eq!(
                parse(&missing),
                Err(TeletextPagesError::InvalidJson.into()),
                "{path}/{field}"
            );
        }
        let mut unknown = original.clone();
        unknown
            .pointer_mut(path)
            .unwrap()
            .as_object_mut()
            .unwrap()
            .insert("unexpected".into(), json!(true));
        assert_eq!(
            parse(&unknown),
            Err(TeletextPagesError::InvalidJson.into()),
            "{path}"
        );
        let mut array = original.clone();
        let fields = array
            .pointer(path)
            .unwrap()
            .as_object()
            .unwrap()
            .values()
            .cloned()
            .collect();
        *array.pointer_mut(path).unwrap() = Value::Array(fields);
        assert_eq!(
            parse(&array),
            Err(TeletextPagesError::InvalidJson.into()),
            "array {path}"
        );
    }
}

#[test]
fn enhanced_wire_rejects_duplicate_keys_before_constructing_tagged_commands() {
    let mut value = document();
    value["page_enhancements"][0]["local_program"] = json!({"commands":[{
        "kind":"at_column","column":2,"operation":{"kind":"g0","code":65}}]});
    let encoded = serde_json::to_string(&value).unwrap();
    let repeated = encoded.replace("\"code\":65", "\"code\":65,\"code\":66");
    assert_ne!(encoded, repeated);
    assert_eq!(
        TeletextPagesV2Response::from_json(repeated.as_bytes(), GENERATION),
        Err(TeletextPagesError::DuplicateKey.into())
    );
}

#[test]
fn enhanced_wire_rejects_wrong_version_correlation_and_authority_fields() {
    for version in [0, 1, 3] {
        let mut value = document();
        value["schema_version"] = json!(version);
        assert_eq!(
            parse(&value),
            Err(TeletextPagesError::UnsupportedSchema.into())
        );
    }
    let bytes = serde_json::to_vec(&document()).unwrap();
    assert_eq!(
        TeletextPagesV2Response::from_json(&bytes, "other"),
        Err(TeletextPagesError::GenerationMismatch.into())
    );
    assert_eq!(
        TeletextPagesV2Response::from_json(&bytes, ""),
        Err(TeletextPagesError::InvalidGeneration.into())
    );
    for field in [
        "publication_id",
        "ready_generation",
        "budget",
        "update_counter",
        "last_packet",
    ] {
        let mut value = document();
        value[field] = json!(1);
        assert_eq!(parse(&value), Err(TeletextPagesError::InvalidJson.into()));
    }
}

#[test]
fn enhanced_wire_enforces_raw_and_collection_original_record_limits() {
    assert_eq!(
        TeletextPagesV2Response::from_json(
            &vec![b' '; MAX_TELETEXT_PAGES_OUTPUT_BYTES + 1],
            GENERATION
        ),
        Err(TeletextPagesError::OutputTooLarge.into())
    );
    assert_eq!(
        TeletextPagesV2Response::from_json(&[255], GENERATION),
        Err(TeletextPagesError::InvalidUtf8.into())
    );
    let mut value = document();
    value["pages"] = json!([]);
    assert_eq!(parse(&value), Err(TeletextPagesError::PageCount.into()));
    let mut value = document();
    let record = value["mot_selections"][0].clone();
    // Repetition is bounded in the original collection, not deduplicated.
    value["mot_selections"] = Value::Array(vec![record.clone(); MAX_TELETEXT_PAGES_RECORDS]);
    parse(&value).unwrap();
    value["mot_selections"].as_array_mut().unwrap().push(record);
    assert_eq!(parse(&value), Err(TeletextPagesV2Error::RecordCount));
}

#[test]
fn enhanced_wire_retains_unchanged_v1_fallback_and_v1_parser_rejects_v2() {
    let value = document();
    assert_eq!(
        crate::TeletextPagesResponse::from_json(&serde_json::to_vec(&value).unwrap(), GENERATION),
        Err(TeletextPagesError::InvalidJson)
    );
    let enhanced = parse(&value).unwrap();
    let v1 = crate::TeletextPagesResponse {
        schema_version: 1,
        generation: GENERATION.into(),
        language: enhanced.language,
        initial_page: enhanced.initial_page,
        pages: enhanced.pages,
    };
    assert_eq!(
        crate::TeletextPagesResponse::from_json(&serde_json::to_vec(&v1).unwrap(), GENERATION)
            .unwrap(),
        v1
    );
}
