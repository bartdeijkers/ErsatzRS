//! Actual installed-package preparation and native compilation, without publication.
//! This inherits the parent's intentional Unix-only POSIX producer fixture.

use super::*;
use ersatzrs_app::teletext_profile_documents::{
    BoundTeletextDocumentError, TeletextProfileDocumentBudget,
};
use ersatzrs_domain::teletext::*;
use ersatzrs_ffmpeg::teletext::*;
use ersatzrs_web::host::teletext_documents::{
    prepare_bound_teletext_document_candidate, EncodedBoundTeletextDocumentCandidate,
    TeletextDocumentEncodingError as Error, TeletextDocumentEncodingPolicy,
};
use serde_json::{json, Value};

const DOCUMENT_SCRIPT: &str = r#"case "$1" in
check) printf '%s' '{"status":"ready","code":"ready","message":"ok"}'; exit 0;;
pages) ;;
*) exit 9;;
esac
theme="$ERSATZRS_ADDON_SETTING_THEME"
case "$theme" in day|night) ;; *) exit 11;; esac
cat > "../document-input-$theme.json"
printf '%s\n' "$ERSATZRS_ADDON_CAPABILITY" >> "../document-calls-$theme"
cat "../document-response-$theme.json"
"#;

#[derive(Clone, Copy)]
pub(super) enum DocumentFixture {
    Base,
    Authored,
    Companion,
}

pub(super) fn validation_budget() -> TeletextProfileDocumentBudget {
    TeletextProfileDocumentBudget {
        service: TeletextServiceBudget {
            max_records: 16,
            max_objects: 16,
            max_structure_items: 4096,
            max_base_content_bytes: 8192,
            max_commands: 8192,
            max_collected_commands: 16384,
            max_collected_drcs_references: 16,
        },
        declarations: TeletextDeclaredServiceBudget {
            max_mots: 4,
            max_displays: 8,
            max_resources: 16,
            max_defaults: 16,
            max_objects: 16,
        },
    }
}

pub(super) fn policy() -> TeletextDocumentEncodingPolicy {
    let catalog = TeletextObjectCatalogBudget {
        max_objects: 64,
        max_commands: 8192,
    };
    let collection = TeletextEnhancementCollectionBudget {
        max_commands: 8192,
        max_drcs_references: 16,
    };
    let normalization = TeletextNormalizedMainProgramBudget {
        composition: TeletextMainCompositionBudget {
            max_input_commands: 4096,
            max_output_commands: 4096,
            collection,
        },
        realization: TeletextBaseCompatibilityRealizationBudget {
            max_commands: 4096,
            max_candidates: 4096,
            max_origins: 8192,
            max_seeds: 64,
            max_edges: 4096,
            max_updates: 100_000,
        },
        placement: TeletextOriginCarrierPlacementBudget {
            max_commands: 4096,
            max_origins: 8192,
            max_edges: 4096,
            catalog,
            spatial: TeletextSpatialTraceBudget {
                max_events: 8192,
                max_invocations: 512,
            },
            max_work: 100_000,
        },
        assembly: TeletextAugmentedOriginProgramBudget {
            max_input_commands: 4096,
            max_output_commands: 4096,
            max_carriers: 64,
            max_identity_checks: 100_000,
            max_work: 100_000,
            recipe: TeletextOriginCarrierBudget {
                max_candidates: 4096,
                max_origins: 8192,
                catalog,
                max_id_checks: 65_600,
                max_output_commands: 6,
            },
        },
        packing: catalog,
        replay: collection,
        ancestry: TeletextAugmentedInvocationBudget {
            max_original_invocations: 256,
            max_actual_invocations: 512,
            max_carriers: 64,
            max_work: 100_000,
        },
        glyph_comparison: TeletextAugmentedGlyphComparisonBudget {
            max_records: 100_000,
            max_cells: 1800,
            max_diagnostics: 4096,
            max_work: 1_000_000,
        },
        display_comparison: TeletextPhysicalComparisonBudget {
            max_records: 100_000,
            max_cells: 1800,
            max_diagnostics: 32_000,
            max_updates: 1_000_000,
        },
        max_resource_objects: 64,
    };
    TeletextDocumentEncodingPolicy {
        validation: validation_budget(),
        v1_region: TeletextEncodingRegion::WesternLatin,
        encoding: TeletextAuthoredServiceEncodingBudget {
            max_pages: 16,
            max_units: 1024,
            max_commands: 8192,
            normalization,
        },
        transmissions: Vec::new(),
    }
}

pub(super) fn document(kind: DocumentFixture, theme: &str) -> Value {
    let mut value = json!({"schema_version":1,"generation":GENERATION,
        "language":"nld","initial_page":123,"pages":[{
            "address":{"page":123,"subcode":16},"kind":"full_page","language":"nld",
            "national_option":0,"rows":[{"row":1,"runs":[{"column":8,
                "style":{"foreground":"white","background":"black","height":"normal",
                    "boxed":false,"flash":false,"conceal":false},
                "content":{"kind":"text","text":"ZZ"}}]}],
            "navigation":[{"role":"index","target":{"page":123,"subcode":null}}]}]});
    if matches!(kind, DocumentFixture::Base) {
        return value;
    }
    value["schema_version"] = json!(2);
    value["pages"][0]["national_option"] = json!(1);
    let rgb = if theme == "day" {
        [15, 0, 0]
    } else {
        [0, 0, 15]
    };
    let presentation = json!({"primary":1,"secondary":null,
        "palette":{"clut2":vec![rgb;8],"clut3":vec![[3,3,3];8]},
        "screen_colour":0,"row_colour":0,"black_background_substitution":false,
        "remapping":"fg_clut0_bg_clut0",
        "panels":{"left":true,"right":true,"status":"level25_and35","width":4}});
    value["page_enhancements"] = json!([{"address":{"page":123,"subcode":16},
        "presentation":null,"compositional_links":null,"local_program":{"commands":[
            {"kind":"set_position","position":{"row":1,"column":8}},
            {"kind":"at_column","column":8,"operation":{"kind":"foreground","colour":18}},
            {"kind":"at_column","column":8,"operation":{"kind":"g0","code":65}}]},
        "resources":{"objects":{"public":null,"global":null},
            "drcs":{"normal":null,"global":null},"defaults":[null,null,null,null]}}]);
    value["magazines"] = json!([{"magazine":1,"presentation":presentation}]);
    for field in ["objects", "drcs", "mots", "empty_object_pages"] {
        value[field] = json!([]);
    }
    value["mot_selections"] = json!([{"display":{"page":123,"subcode":16},"mot":null}]);
    if matches!(kind, DocumentFixture::Companion) {
        value["magazines"] = json!([]);
        value["page_enhancements"][0]["local_program"] = Value::Null;
        let unused = json!({"target":{"kind":"unspecified","magazine":1},
            "validity":"level25_only","required_subpages":65535});
        value["page_enhancements"][0]["compositional_links"] = json!({
            "gpop":unused,"pop":{"target":{"kind":"page","page":{"magazine":1,"page":160}},
                "validity":"level25_only","required_subpages":1},"gdrcs":unused,"drcs":unused});
        value["page_enhancements"][0]["resources"]["objects"]["public"] =
            json!({"magazine":1,"page":160});
        value["empty_object_pages"] = json!([{"kind":"pop",
            "page":{"magazine":1,"page":160},"subpage":0}]);
    }
    value
}

pub(super) async fn configure(fixture: &Fixture, kind: DocumentFixture) {
    let mut current = manifest();
    let capability = if matches!(kind, DocumentFixture::Base) {
        "teletext.pages.v1"
    } else {
        "teletext.pages.v2"
    };
    current.capabilities[1].id = capability.into();
    current.validate().unwrap();
    let package = fixture.root.path().join("package");
    std::fs::write(package.join("fixture.sh"), DOCUMENT_SCRIPT).unwrap();
    let hash = calculate_addon_content_sha256(&package).await.unwrap();
    sqlx::query(
        r#"UPDATE "AddonInstallation" SET "ContentSha256"=?, "ManifestJson"=? WHERE "AddonId"=?"#,
    )
    .bind(hash)
    .bind(serde_json::to_string(&current).unwrap())
    .bind(ADDON)
    .execute(&fixture.state.pool)
    .await
    .unwrap();
    for theme in ["day", "night"] {
        write_response(fixture, theme, &document(kind, theme));
    }
}

fn write_response(fixture: &Fixture, theme: &str, value: &Value) {
    std::fs::write(
        fixture
            .root
            .path()
            .join(format!("document-response-{theme}.json")),
        serde_json::to_vec(value).unwrap(),
    )
    .unwrap();
}

async fn candidate(
    fixture: &Fixture,
    channel: i64,
    policy: TeletextDocumentEncodingPolicy,
) -> Result<EncodedBoundTeletextDocumentCandidate, Error> {
    prepare_bound_teletext_document_candidate(
        &fixture.state.application,
        channel,
        ADDON,
        1,
        &guide(&[]),
        policy,
    )
    .await
}

pub(super) fn packet_numbers(units: &[[u8; 46]]) -> Vec<u8> {
    units
        .iter()
        .map(|unit| match observe_dvb_teletext_header(unit).unwrap() {
            TeletextHeaderObservation::Header(header) => header.packet_address.packet_number,
            TeletextHeaderObservation::NotHeader(address) => address.packet_number,
            _ => panic!("expected only actual authored page packets"),
        })
        .collect()
}

fn assert_compiled(candidate: &EncodedBoundTeletextDocumentCandidate, rgb: [u8; 3]) {
    assert_eq!(candidate.generation(), GENERATION);
    assert_eq!(candidate.selection().contract, TeletextPageContract::V2);
    let encoded = candidate.encoded();
    let original = encoded.declared().semantic();
    let presentations = candidate.producer_presentations();
    assert_eq!(presentations.len(), 1);
    let presentation = &presentations[0];
    assert_eq!(presentation.address(), original.base().pages[0].address);
    assert_eq!(
        presentation.source(),
        TeletextProducerPresentationSource::Magazine { magazine: 1 }
    );
    assert_eq!(
        presentation.effective_defaults(),
        &original.magazine_presentations()[&1]
    );
    assert_eq!(
        presentation.secondary_selection(),
        TeletextProducerSecondarySelection::Declared(
            original.magazine_presentations()[&1].secondary
        )
    );
    assert_eq!(
        presentation.local_program_presence(),
        TeletextProducerLocalProgramPresence::PresentNonEmpty
    );
    assert_eq!(encoded.pages().len(), 1);
    assert_eq!(packet_numbers(encoded.magazine_units()), [29]);
    let EncodedTeletextAuthoredPage::CompiledDisplay(page) = &encoded.pages()[0] else {
        panic!("present local program requires complete compiled display")
    };
    let TeletextHeaderObservation::Header(header) =
        observe_dvb_teletext_header(&page.page().data_units()[0]).unwrap()
    else {
        panic!("expected actual display header");
    };
    assert_eq!(header.display_page().unwrap().get(), 123);
    assert_eq!(header.subcode, 16);
    let packets = packet_numbers(page.page().data_units());
    assert!(packets.contains(&26));
    assert!(!packets.contains(&28)); // Magazine declaration is not a synthetic page override.
    let row = page
        .page()
        .data_units()
        .iter()
        .find(|unit| {
            matches!(observe_dvb_teletext_header(unit),
            Ok(TeletextHeaderObservation::NotHeader(address)) if address.packet_number == 1)
        })
        .unwrap();
    assert_eq!(
        [row[14].reverse_bits() & 0x7f, row[15].reverse_bits() & 0x7f],
        *b"ZZ"
    );
    let preview = page.normalized().original();
    let cell = preview.display_resolution().cell(1, 12).unwrap();
    let origin = &preview.display_resolution().origins()[cell.origin().unwrap()];
    let glyph = &preview.glyph_intent().records()[origin.candidate_index()];
    assert_eq!(
        glyph.context().candidate().operation(),
        Some(TeletextColumnOperation::G0 {
            code: TeletextGlyphCode::new(65).unwrap(),
        })
    );
    let raster = render_teletext_composition(
        preview,
        TeletextColourPixelOptions {
            reveal_concealed: false,
            flash_phase: TeletextFlashPhase::Original,
        },
        TeletextRasterBudget {
            max_pixels: 168_000,
            max_diagnostics: 1400,
        },
    )
    .unwrap();
    assert!(raster.diagnostics().is_empty());
    assert_eq!(raster.width(), 56 * 12);
    assert!((144..156).any(|x| (10..20).any(|y|
        matches!(raster.pixel(x,y), Some(TeletextRasterPixel::Colour(TeletextColourPixel::Rgb(colour)))
            if colour.get()==rgb))));
    assert_eq!(candidate.descriptor().bytes(), b"\x56\x05nld\x09\x23");
    let debug = format!("{candidate:?}");
    assert!(!debug.contains(GENERATION));
    assert!(!debug.contains("ZZ"));
}

#[tokio::test]
async fn two_actual_v2_profiles_compile_isolated_complete_candidates_without_writes() {
    let mut fixture = Fixture::new().await;
    configure(&fixture, DocumentFixture::Authored).await;
    let day = fixture.profile("Day document", "day", "good").await;
    let night = fixture.profile("Night document", "night", "good").await;
    let first = fixture.bind(1, &day, None).await;
    let second = fixture.bind(2, &night, None).await;
    let config_before: Vec<(Option<String>, Option<String>)> =
        sqlx::query_as("SELECT \"Key\", \"Value\" FROM \"ConfigElement\" ORDER BY \"Key\"")
            .fetch_all(&fixture.state.pool)
            .await
            .unwrap();
    let before = fixture
        .state
        .application
        .teletext_installation_snapshot(ADDON)
        .await
        .unwrap();
    let (day, night) = tokio::join!(
        candidate(&fixture, 1, policy()),
        candidate(&fixture, 2, policy())
    );
    for (result, theme, channel, profile, rgb) in [
        (day.unwrap(), "day", 1, first.profile_id, [15, 0, 0]),
        (night.unwrap(), "night", 2, second.profile_id, [0, 0, 15]),
    ] {
        assert_eq!(result.selection().channel_id, channel);
        assert_eq!(result.selection().profile_id, profile);
        assert_compiled(&result, rgb);
        let input = std::fs::read(
            fixture
                .root
                .path()
                .join(format!("document-input-{theme}.json")),
        )
        .unwrap();
        assert_eq!(
            input,
            ersatzrs_app::teletext_guide::build_request_for_schema(&guide(&[]), 2).unwrap()
        );
        assert_eq!(
            std::fs::read(fixture.root.path().join(format!("document-calls-{theme}"))).unwrap(),
            b"teletext.pages.v2\n"
        );
    }
    assert_eq!(
        fixture
            .state
            .application
            .teletext_installation_snapshot(ADDON)
            .await
            .unwrap(),
        before
    );
    assert_eq!(
        fixture
            .state
            .application
            .channel_teletext_bindings(1)
            .await
            .unwrap()[0]
            .binding,
        first
    );
    assert_eq!(
        fixture
            .state
            .application
            .channel_teletext_bindings(2)
            .await
            .unwrap()[0]
            .binding,
        second
    );
    let config_after: Vec<(Option<String>, Option<String>)> =
        sqlx::query_as("SELECT \"Key\", \"Value\" FROM \"ConfigElement\" ORDER BY \"Key\"")
            .fetch_all(&fixture.state.pool)
            .await
            .unwrap();
    assert_eq!(config_after, config_before);
    assert!(fixture.playout.try_recv().is_err());
    fixture.state.pool.close().await;
}

#[tokio::test]
async fn wrong_contract_malformed_and_budget_rejections_do_not_return_partial_candidates() {
    let mut fixture = Fixture::new().await;
    configure(&fixture, DocumentFixture::Authored).await;
    let profile = fixture.profile("Strict document", "day", "good").await;
    let binding = fixture.bind(1, &profile, None).await;
    let mut wrong = document(DocumentFixture::Authored, "day");
    wrong["schema_version"] = json!(1);
    write_response(&fixture, "day", &wrong);
    assert!(matches!(
        candidate(&fixture, 1, policy()).await,
        Err(Error::Document(
            BoundTeletextDocumentError::AuthoredResponse(_)
        ))
    ));
    std::fs::write(
        fixture.root.path().join("document-response-day.json"),
        b"{malformed",
    )
    .unwrap();
    assert!(matches!(
        candidate(&fixture, 1, policy()).await,
        Err(Error::Document(
            BoundTeletextDocumentError::AuthoredResponse(_)
        ))
    ));
    write_response(&fixture, "day", &document(DocumentFixture::Authored, "day"));
    let mut limited = policy();
    limited.validation.service.max_commands = 2;
    assert!(matches!(
        candidate(&fixture, 1, limited).await,
        Err(Error::Document(
            BoundTeletextDocumentError::AuthoredResponse(_)
        ))
    ));
    let mut limited = policy();
    limited.encoding.max_units = 0;
    assert!(matches!(
        candidate(&fixture, 1, limited).await,
        Err(Error::Encoding(_))
    ));
    assert_eq!(
        fixture
            .state
            .application
            .channel_teletext_bindings(1)
            .await
            .unwrap()[0]
            .binding,
        binding
    );
    assert_eq!(
        std::fs::read(fixture.root.path().join("document-calls-day")).unwrap(),
        b"teletext.pages.v2\nteletext.pages.v2\nteletext.pages.v2\nteletext.pages.v2\n"
    );
    assert!(fixture.playout.try_recv().is_err());
    fixture.state.pool.close().await;
}

pub(super) fn companion_transmission() -> TeletextServiceResourceTransmission {
    TeletextServiceResourceTransmission {
        resource: TeletextResourceIdentity::new(
            TeletextResourceKind::Pop,
            TeletextRawDataPageNumber::new(1, 0xa0).unwrap(),
            TeletextResourceSubpageIndex::new(0).unwrap(),
        )
        .unwrap(),
        counter: TeletextResourceUpdateCounter::new(3).unwrap(),
        update: TeletextResourceFullUpdate::Changed,
        last_subpage: true,
    }
}

#[tokio::test]
async fn actual_empty_resource_requires_explicit_lifecycle_and_never_invents_missing_body() {
    let mut fixture = Fixture::new().await;
    configure(&fixture, DocumentFixture::Companion).await;
    let profile = fixture.profile("Companion document", "day", "good").await;
    fixture.bind(1, &profile, None).await;
    let rejected = candidate(&fixture, 1, policy()).await;
    assert!(
        matches!(&rejected,
        Err(Error::Encoding(error)) if matches!(error.reason,
            TeletextAuthoredServiceEncodingFailure::MissingTransmission)),
        "{rejected:?}"
    );
    let mut complete = policy();
    complete.transmissions.push(companion_transmission());
    let result = candidate(&fixture, 1, complete).await.unwrap();
    let presentation = &result.producer_presentations()[0];
    assert_eq!(
        presentation.source(),
        TeletextProducerPresentationSource::Header {
            national_option: TeletextNationalOption::new(1).unwrap()
        }
    );
    assert_eq!(
        presentation.secondary_selection(),
        TeletextProducerSecondarySelection::DecoderDependent
    );
    assert_eq!(
        presentation.local_program_presence(),
        TeletextProducerLocalProgramPresence::Absent
    );
    let EncodedTeletextAuthoredPage::NativeDisplay(display) = &result.encoded().pages()[0] else {
        panic!("snapshot must not synthesize a local program or presentation packet");
    };
    assert!(!packet_numbers(display.encoded().page().data_units()).contains(&28));
    assert_eq!(result.encoded().pages().len(), 2);
    let EncodedTeletextAuthoredPage::Object(resource) = &result.encoded().pages()[1] else {
        panic!("empty original POP companion must have its real object-page body")
    };
    assert_eq!(
        resource.header().identity(),
        companion_transmission().resource
    );
    assert_eq!(resource.header().update_counter().get(), 3);
    let mut missing = document(DocumentFixture::Companion, "day");
    missing["empty_object_pages"] = json!([]);
    write_response(&fixture, "day", &missing);
    let mut complete = policy();
    complete.transmissions.push(companion_transmission());
    assert!(matches!(
        candidate(&fixture, 1, complete).await,
        Err(Error::Document(
            BoundTeletextDocumentError::AuthoredResponse(_)
        ))
    ));
    assert!(fixture.playout.try_recv().is_err());
    fixture.state.pool.close().await;
}

#[tokio::test]
async fn actual_v1_document_is_lifted_losslessly_with_explicit_isolation() {
    let mut fixture = Fixture::new().await;
    configure(&fixture, DocumentFixture::Base).await;
    let profile = fixture.profile("Base document", "day", "good").await;
    fixture.bind(1, &profile, None).await;
    let mut base_policy = policy();
    base_policy.validation.declarations.max_mots = 0;
    base_policy.validation.declarations.max_resources = 0;
    base_policy.validation.declarations.max_defaults = 0;
    base_policy.validation.declarations.max_objects = 0;
    let result = candidate(&fixture, 1, base_policy).await.unwrap();
    assert_eq!(result.selection().contract, TeletextPageContract::V1);
    let source = document(DocumentFixture::Base, "day");
    let expected = ersatzrs_app::teletext_pages::parse_page_set(
        &serde_json::to_vec(&source).unwrap(),
        GENERATION,
    )
    .unwrap();
    let service = result.encoded().declared().semantic();
    assert_eq!(service.base(), &expected);
    let metadata = &service.page_enhancements()[&expected.pages[0].address];
    let presentation = &result.producer_presentations()[0];
    assert_eq!(presentation.address(), expected.pages[0].address);
    assert_eq!(
        presentation.source(),
        TeletextProducerPresentationSource::Page
    );
    assert_eq!(
        presentation.effective_defaults(),
        metadata.presentation.as_ref().unwrap()
    );
    assert_eq!(
        presentation.secondary_selection(),
        TeletextProducerSecondarySelection::Declared(metadata.presentation.unwrap().secondary)
    );
    assert_eq!(
        presentation.local_program_presence(),
        TeletextProducerLocalProgramPresence::PresentEmpty
    );
    assert!(metadata.presentation.is_some());
    assert!(metadata
        .local_program
        .as_ref()
        .unwrap()
        .commands()
        .is_empty());
    assert!(service.magazine_presentations().is_empty());
    assert!(result.encoded().declared().resource_inventory().is_empty());
    let mut limited = policy();
    limited.validation.declarations.max_displays = 0;
    assert!(matches!(
        candidate(&fixture, 1, limited).await,
        Err(Error::DeclarationBudget {
            count: 1,
            maximum: 0
        })
    ));
    assert!(fixture.playout.try_recv().is_err());
    fixture.state.pool.close().await;
}
