//! Synthetic unpublished input: these tests establish no physical/client proof.
use super::*;
use serde_json::{json, Value};

const GENERATION: &str = "synthetic-authored-generation";

fn budgets() -> (d::TeletextServiceBudget, d::TeletextDeclaredServiceBudget) {
    (
        d::TeletextServiceBudget {
            max_records: 64,
            max_objects: 64,
            max_structure_items: 1024,
            max_base_content_bytes: 1024,
            max_commands: 1024,
            max_collected_commands: 1024,
            max_collected_drcs_references: 64,
        },
        d::TeletextDeclaredServiceBudget {
            max_mots: 8,
            max_displays: 64,
            max_resources: 64,
            max_defaults: 64,
            max_objects: 64,
        },
    )
}

fn identity(kind: &str, byte: u8, subpage: u8) -> Value {
    json!({"kind":kind,"page":{"magazine":2,"page":byte},"subpage":subpage})
}

fn owner() -> d::TeletextPageAddress {
    d::TeletextPageAddress {
        page: d::TeletextPageNumber::new(200).unwrap(),
        subcode: d::TeletextSubcode::new(16).unwrap(),
    }
}

fn presentation() -> Value {
    json!({"primary":8,"secondary":1,
        "palette":{"clut2":vec![[1,2,3];8],"clut3":vec![[4,5,6];8]},
        "screen_colour":16,"row_colour":23,"black_background_substitution":true,
        "remapping":"fg_clut2_bg_clut3",
        "panels":{"left":true,"right":true,"status":"level25_and35","width":4}})
}

fn document() -> Value {
    json!({"schema_version":2,"generation":GENERATION,"language":"nld","initial_page":200,
        "pages":[{"address":{"page":200,"subcode":16},"kind":"full_page","language":"nld",
            "national_option":5,"rows":[{"row":2,"runs":[{"column":1,
                "style":{"foreground":"white","background":"black","height":"normal",
                    "boxed":false,"flash":false,"conceal":false},
                "content":{"kind":"text","text":"Café"}}]}],"navigation":[]}],
        "page_enhancements":[{"address":{"page":200,"subcode":16},
            "presentation":null,"compositional_links":null,"local_program":null,
            "resources":{"objects":{"public":null,"global":null},
                "drcs":{"normal":null,"global":null},"defaults":[null,null,null,null]}}],
        "magazines":[],"objects":[],"drcs":[],"mots":[],
        "mot_selections":[{"display":{"page":200,"subcode":16},"mot":null}],
        "empty_object_pages":[]})
}

fn enhanced() -> Value {
    let mut value = document();
    let target = json!({"scope":"public","resource":identity("pop",0xa0,1),"object_id":65535});
    let unused = json!({"kind":"unspecified","magazine":2});
    let object_link = json!({"target":unused,"extent":0,"supersedable":true,"raw_fallback":0,
        "defaults":[{"kind":"no_object","raw_number":213},{"kind":"dormant","object_type":"passive","raw_number":234}]});
    let drcs_link = json!({"target":unused,"extent":0,"supersedable":true});
    let mut public = vec![object_link.clone(); 7];
    public[0] = json!({"target":{"kind":"page","page":{"magazine":2,"page":160}},
        "extent":1,"supersedable":true,"raw_fallback":9,
        "defaults":[{"kind":"object","target":target,"object_type":"passive"},
            {"kind":"no_object","raw_number":77}]});
    let mut normal = vec![drcs_link.clone(); 7];
    normal[0] = json!({"target":{"kind":"page","page":{"magazine":2,"page":192}},"extent":0,"supersedable":true});
    value["mots"] = json!([{"identity":identity("mot",0xfe,7),
        "associations":[{"page":0,"objects":{"global":false,"public":1},"drcs":{"global":false,"public":1}}],
        "links":{"global_object":object_link,"public_objects":public,
            "global_drcs":drcs_link,"normal_drcs":normal,
            "counts":{"magazine_mask":2,"object_pages":1,"drcs_pages":1}}}]);
    value["mot_selections"][0]["mot"] = identity("mot", 0xfe, 7);
    value["objects"] = json!([{"identity":target,"object_type":"passive","program":{"commands":[
        {"kind":"set_position","position":{"row":1,"column":8}},
        {"kind":"at_column","column":8,"operation":{"kind":"foreground","colour":18}},
        {"kind":"at_column","column":8,"operation":{"kind":"drcs","reference":{"table":"normal","subtable":0,"slot":47}}}
    ]}}]);
    value["drcs"] = json!([{"identity":identity("drcs",0xc0,0),
        "glyphs":[{"slot":47,"bitmap":[4095,1,2,4,8,16,32,64,128,2048]}]}]);
    value["empty_object_pages"] = json!([identity("pop", 0xa0, 0)]);
    value["page_enhancements"][0]["resources"] = json!({"objects":{"public":{"magazine":2,"page":160},"global":null},
        "drcs":{"normal":{"magazine":2,"page":192},"global":null},
        "defaults":[null,null,{"target":target,"object_type":"passive"},null]});
    value["magazines"] = json!([{"magazine":2,"presentation":presentation()}]);
    value
}

fn parse(value: &Value) -> Result<d::TeletextDeclaredService, TeletextAuthoredPagesError> {
    let (service, declared) = budgets();
    parse_declared_service(
        &serde_json::to_vec(value).unwrap(),
        GENERATION,
        service,
        declared,
    )
}

#[test]
fn complete_mot_pop_drcs_magazine_document_retains_original_declarations() {
    let result = parse(&enhanced()).unwrap();
    let service = result.semantic();
    let page = &service.base().pages[0];
    assert_eq!(page.address, owner());
    assert_eq!(page.language.as_str(), "nld");
    assert_eq!(page.national_option.get(), 5);
    assert_eq!(
        page.rows[0].runs[0].content,
        d::TeletextRunContent::Text {
            text: "Café".into()
        }
    );
    assert_eq!(result.mots()[0].identity().logical_subpage().get(), 7);
    assert_eq!(
        result.mots()[0]
            .links()
            .public_fallback(d::TeletextMotPublicLinkIndex::new(1).unwrap())
            .get(),
        9
    );
    assert_eq!(
        result.mots()[0].links().object_link(None).defaults()[1],
        d::TeletextMotDefaultObject::Dormant {
            object_type: d::TeletextObjectType::Passive,
            raw_number: 234
        }
    );
    let object = &service.objects().definitions()[0];
    assert_eq!(object.identity.object_id().get(), 65535);
    assert_eq!(
        object.identity.resource().unwrap().logical_subpage().get(),
        1
    );
    assert_eq!(object.program.commands().len(), 3);
    assert_eq!(service.drcs().subtables()[0].glyphs()[0].slot.get(), 47);
    assert_eq!(
        service.drcs().subtables()[0].glyphs()[0].bitmap.get(),
        [4095, 1, 2, 4, 8, 16, 32, 64, 128, 2048]
    );
    let presentation = service.magazine_presentations()[&2];
    assert_eq!(presentation.primary.get(), 8);
    assert_eq!(presentation.secondary.unwrap().get(), 1);
    assert_eq!(presentation.palette.clut2[0].get(), [1, 2, 3]);
    assert_eq!(presentation.panels.level25_widths(), (4, 12));
    assert_eq!(result.resource_inventory().len(), 4);
    assert_eq!(
        result.defaults()[0].selection(),
        service.page_enhancements()[&owner()].resources.defaults[2].unwrap()
    );
}

#[test]
fn null_and_present_empty_local_keep_defaults_but_change_activation() {
    let absent = parse(&enhanced()).unwrap();
    let mut value = enhanced();
    value["page_enhancements"][0]["local_program"] = json!({"commands":[]});
    let present = parse(&value).unwrap();
    assert_eq!(
        absent.resolutions()[0].defaults(),
        present.resolutions()[0].defaults()
    );
    let (source, program) = absent.semantic().effective_program(owner()).unwrap();
    assert_eq!(
        source,
        d::TeletextEffectiveProgramSource::Defaults { slots: vec![2] }
    );
    assert_eq!(program.commands().len(), 1);
    let (source, program) = present.semantic().effective_program(owner()).unwrap();
    assert_eq!(source, d::TeletextEffectiveProgramSource::Local);
    assert!(program.commands().is_empty());
    value["page_enhancements"][0]["resources"]["defaults"][2] = Value::Null;
    assert!(
        matches!(parse(&value), Err(TeletextAuthoredPagesError::Declarations(error))
        if error.reason == d::TeletextDeclaredServiceFailure::DefaultSelection { slot:2 })
    );
}

#[test]
fn page_magazine_and_mot_fallback_presence_are_not_promoted() {
    let mut value = enhanced();
    value["magazines"] = json!([]);
    let fallback = parse(&value).unwrap();
    assert!(matches!(
        fallback.semantic().effective_presentation(owner()).unwrap(),
        d::TeletextEffectivePresentation::MotFallback { .. }
    ));
    assert!(fallback.semantic().page_enhancements()[&owner()]
        .presentation
        .is_none());
    assert!(fallback.semantic().magazine_presentations().is_empty());
    value["magazines"] = json!([{"magazine":2,"presentation":presentation()}]);
    let magazine = parse(&value).unwrap();
    assert!(matches!(
        magazine.semantic().effective_presentation(owner()).unwrap(),
        d::TeletextEffectivePresentation::Extension {
            source: d::TeletextPresentationSource::Magazine,
            ..
        }
    ));
    value["page_enhancements"][0]["presentation"] = presentation();
    let page = parse(&value).unwrap();
    assert!(matches!(
        page.semantic().effective_presentation(owner()).unwrap(),
        d::TeletextEffectivePresentation::Extension {
            source: d::TeletextPresentationSource::Page,
            ..
        }
    ));
}

#[test]
fn mismatched_actual_links_or_magazine_selection_are_rejected() {
    let mut value = enhanced();
    value["page_enhancements"][0]["resources"]["objects"]["public"] = Value::Null;
    assert!(
        matches!(parse(&value), Err(TeletextAuthoredPagesError::Declarations(error))
        if error.reason == d::TeletextDeclaredServiceFailure::SelectedRole { role:d::TeletextX27LinkRole::Pop })
    );
    let mut value = enhanced();
    value["mot_selections"][0]["mot"] = Value::Null;
    assert!(
        matches!(parse(&value), Err(TeletextAuthoredPagesError::Declarations(error))
        if matches!(error.reason, d::TeletextDeclaredServiceFailure::MotSelection { .. }))
    );
    let mut value = enhanced();
    let mut second = value["mots"][0].clone();
    second["identity"]["subpage"] = json!(8);
    value["mots"].as_array_mut().unwrap().push(second);
    assert!(
        matches!(parse(&value), Err(TeletextAuthoredPagesError::Declarations(error))
        if matches!(error.reason, d::TeletextDeclaredServiceFailure::DuplicateMagazineMot { .. }))
    );
}

#[test]
fn ranges_are_located_and_never_masked_or_normalized() {
    for (path, invalid, section) in [
        (
            "/magazines/0/presentation/primary",
            json!(127),
            TeletextAuthoredSection::Magazines,
        ),
        (
            "/magazines/0/presentation/panels/width",
            json!(16),
            TeletextAuthoredSection::Magazines,
        ),
        (
            "/objects/0/program/commands/1/operation/colour",
            json!(32),
            TeletextAuthoredSection::Objects,
        ),
        (
            "/objects/0/program/commands/2/operation/reference/slot",
            json!(48),
            TeletextAuthoredSection::Objects,
        ),
        (
            "/drcs/0/glyphs/0/bitmap/3",
            json!(4096),
            TeletextAuthoredSection::Drcs,
        ),
        (
            "/objects/0/identity/resource/subpage",
            json!(16),
            TeletextAuthoredSection::Objects,
        ),
        (
            "/mots/0/links/public_objects/0/raw_fallback",
            json!(16),
            TeletextAuthoredSection::Mots,
        ),
    ] {
        let mut value = enhanced();
        *value.pointer_mut(path).unwrap() = invalid;
        let Err(TeletextAuthoredPagesError::Conversion { location, .. }) = parse(&value) else {
            panic!("expected located scalar failure")
        };
        assert_eq!(location.section, section);
        assert_eq!(location.record, 0);
        if path.contains("commands/1") {
            assert_eq!(location.command, Some(1));
        }
    }
}

#[test]
fn duplicate_missing_and_wrong_kind_resources_do_not_create_phantom_definitions() {
    let mut value = enhanced();
    value["empty_object_pages"] = json!([]);
    assert!(
        matches!(parse(&value), Err(TeletextAuthoredPagesError::Declarations(error))
        if error.reason == d::TeletextDeclaredServiceFailure::MissingResource)
    );
    let mut value = enhanced();
    value["drcs"] = json!([]);
    assert!(matches!(
        parse(&value),
        Err(TeletextAuthoredPagesError::Service(_))
    ));
    let mut value = enhanced();
    let duplicate = value["drcs"][0].clone();
    value["drcs"].as_array_mut().unwrap().push(duplicate);
    assert!(matches!(
        parse(&value),
        Err(TeletextAuthoredPagesError::Service(_))
    ));
    let mut value = enhanced();
    value["objects"][0]["identity"]["resource"]["kind"] = json!("gpop");
    assert!(matches!(
        parse(&value),
        Err(TeletextAuthoredPagesError::Conversion {
            reason: TeletextAuthoredValueError::Object(_),
            ..
        })
    ));
}

#[test]
fn raw_budgets_fence_conversion_even_for_invalid_scalar_input() {
    let mut value = enhanced();
    value["objects"][0]["program"]["commands"][1]["operation"]["colour"] = json!(32);
    let bytes = serde_json::to_vec(&value).unwrap();
    let (mut service, declared) = budgets();
    service.max_commands = 1;
    assert!(matches!(
        parse_declared_service(&bytes, GENERATION, service, declared),
        Err(TeletextAuthoredPagesError::Budget {
            location: TeletextAuthoredLocation {
                section: TeletextAuthoredSection::Objects,
                command: Some(0),
                ..
            },
            ..
        })
    ));
    let bytes = serde_json::to_vec(&document()).unwrap();
    let (mut service, declared) = budgets();
    service.max_base_content_bytes = 4; // Café is five UTF-8 bytes.
    assert!(matches!(
        parse_declared_service(&bytes, GENERATION, service, declared),
        Err(TeletextAuthoredPagesError::Budget {
            dimension: "base content byte",
            ..
        })
    ));
    service.max_base_content_bytes = 5;
    assert!(parse_declared_service(&bytes, GENERATION, service, declared).is_ok());
    let (mut service, declared) = budgets();
    service.max_records = 0;
    assert!(matches!(
        parse_declared_service(&bytes, GENERATION, service, declared),
        Err(TeletextAuthoredPagesError::Budget { .. })
    ));
    let (service, mut declared) = budgets();
    declared.max_displays = 0;
    assert!(matches!(
        parse_declared_service(&bytes, GENERATION, service, declared),
        Err(TeletextAuthoredPagesError::Budget { .. })
    ));
}

#[test]
fn strict_v2_errors_do_not_leak_authored_text_or_generation() {
    let mut value = document();
    value["schema_version"] = json!(1);
    assert!(matches!(
        parse(&value),
        Err(TeletextAuthoredPagesError::Contract(_))
    ));
    let (service, declared) = budgets();
    let error = parse_declared_service(
        &serde_json::to_vec(&document()).unwrap(),
        "different-generation",
        service,
        declared,
    )
    .unwrap_err();
    let diagnostic = format!("{error:?} {error}");
    assert!(!diagnostic.contains(GENERATION));
    assert!(!diagnostic.contains("different-generation"));
    assert!(!diagnostic.contains("Café"));
    let duplicate = br#"{"schema_version":2,"schema_version":2}"#;
    assert!(matches!(
        parse_declared_service(duplicate, GENERATION, service, declared),
        Err(TeletextAuthoredPagesError::Contract(_))
    ));
}

#[test]
fn every_original_command_family_preserves_exact_order_and_checked_values() {
    let operations = vec![
        json!({"kind":"foreground","colour":16}),
        json!({"kind":"background","colour":31}),
        json!({"kind":"designate_charset","designation":8}),
        json!({"kind":"g0","code":65}),
        json!({"kind":"g1","mosaic":63}),
        json!({"kind":"g1_alpha","code":64}),
        json!({"kind":"g2","code":127}),
        json!({"kind":"g3","code":32,"compatibility":"level15"}),
        json!({"kind":"g0_diacritic","code":42,"diacritic":0}),
        json!({"kind":"drcs","reference":{"table":"global","subtable":15,"slot":47}}),
        json!({"kind":"attributes","double_height":true,"double_width":true,"boxed":true,
            "conceal":true,"invert":true,"underline_or_separated_mosaics":true}),
        json!({"kind":"flash","mode":"adjacent_clut","rate":"fast2_hz_incremental"}),
    ];
    let mut commands = vec![
        json!({"kind":"set_position","position":{"row":1,"column":0}}),
        json!({"kind":"set_screen_colour","colour":23}),
        json!({"kind":"set_row_colour","row":24,"colour":20,"following_rows":true}),
    ];
    for operation in operations {
        commands.push(json!({"kind":"at_column","column":8,"operation":operation}));
    }
    commands.push(json!({"kind":"invoke_object","target":{"scope":"local",
        "owner":{"page":200,"subcode":16},"object_id":65535},
        "object_type":"adaptive","offset":{"row":1,"column":71}}));
    let original = json!({"commands":commands});
    let wire: wire::TeletextEnhancementProgramWire =
        serde_json::from_value(original.clone()).unwrap();
    let converted = program::convert(
        wire,
        TeletextAuthoredLocation::new(TeletextAuthoredSection::Pages, 0),
    )
    .unwrap();
    assert_eq!(serde_json::to_value(&converted).unwrap(), original);
}

#[test]
fn exact_authored_budget_and_declared_default_fences_precede_domain_indexes() {
    let bytes = serde_json::to_vec(&enhanced()).unwrap();
    let (mut service, mut declared) = budgets();
    service.max_commands = 4; // One default assertion and three original body commands.
    service.max_collected_commands = 4;
    assert!(parse_declared_service(&bytes, GENERATION, service, declared).is_ok());
    service.max_commands = 3;
    assert!(matches!(
        parse_declared_service(&bytes, GENERATION, service, declared),
        Err(TeletextAuthoredPagesError::Budget {
            location: TeletextAuthoredLocation {
                section: TeletextAuthoredSection::Objects,
                command: Some(2),
                ..
            },
            ..
        })
    ));
    service.max_commands = 4;
    declared.max_defaults = 0;
    assert!(matches!(
        parse_declared_service(&bytes, GENERATION, service, declared),
        Err(TeletextAuthoredPagesError::Budget {
            dimension: "declared default",
            ..
        })
    ));
    declared.max_defaults = 1;
    declared.max_mots = 0;
    assert!(matches!(
        parse_declared_service(&bytes, GENERATION, service, declared),
        Err(TeletextAuthoredPagesError::Budget { .. })
    ));
    declared.max_mots = 1;
    service.max_collected_commands = 0;
    assert!(matches!(
        parse_declared_service(&bytes, GENERATION, service, declared),
        Err(TeletextAuthoredPagesError::Service(_))
    ));
}
