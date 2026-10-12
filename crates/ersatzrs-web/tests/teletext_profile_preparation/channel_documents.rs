//! Real installed producers to one cold channel candidate, without publication.
//! The parent deliberately limits these POSIX producer fixtures to Unix.

use super::documents::{self, DocumentFixture};
use super::*;
use ersatzrs_app::teletext_channel_documents::{
    ChannelTeletextDocumentError, TeletextChannelDocumentInputBudget,
};
use ersatzrs_domain::teletext::*;
use ersatzrs_ffmpeg::teletext::*;
use ersatzrs_web::host::teletext_channel_documents::{
    prepare_channel_teletext_document_candidate, ChannelTeletextDocumentEncodingError as Error,
    EncodedChannelTeletextDocumentCandidate, TeletextChannelDocumentEncodingPolicy,
};
use serde_json::{json, Value};

const CHANNEL_SCRIPT: &str = r#"case "$1" in
check) printf '%s' '{"status":"ready","code":"ready","message":"ok"}'; exit 0;;
pages) ;;
*) exit 9;;
esac
cat > "../channel-input-LABEL.json"
printf '%s:%s\n' LABEL "$ERSATZRS_ADDON_CAPABILITY" >> ../channel-calls
cat ../channel-response-LABEL.json
"#;

pub(super) fn policy() -> TeletextChannelDocumentEncodingPolicy {
    let original = documents::policy();
    TeletextChannelDocumentEncodingPolicy {
        input: TeletextChannelDocumentInputBudget {
            max_response_bytes: 32_768,
        },
        validation: original.validation,
        v1_region: original.v1_region,
        composition: TeletextProducerCompositionBudget {
            max_producers: 8,
            max_input_items: 16_384,
            max_input_content_bytes: 65_536,
            max_input_commands: 16_384,
            max_storage_bytes: 16 * 1024 * 1024,
            max_work: 1_000_000,
            service: original.validation.service,
            declarations: original.validation.declarations,
        },
        encoding: original.encoding,
        transmissions: Vec::new(),
        entry: None,
    }
}

pub(super) fn address(page: u16, subcode: u16) -> TeletextPageAddress {
    TeletextPageAddress {
        page: TeletextPageNumber::new(page).unwrap(),
        subcode: TeletextSubcode::new(subcode).unwrap(),
    }
}

pub(super) fn response(
    kind: DocumentFixture,
    theme: &str,
    page: u16,
    subcode: u16,
    language: &str,
) -> Value {
    let mut value = documents::document(kind, theme);
    let owner = json!({"page":page,"subcode":subcode});
    value["language"] = json!(language);
    value["initial_page"] = json!(page);
    value["pages"][0]["address"] = owner.clone();
    value["pages"][0]["language"] = json!(language);
    value["pages"][0]["navigation"][0]["target"]["page"] = json!(page);
    if !matches!(kind, DocumentFixture::Base) {
        value["page_enhancements"][0]["address"] = owner.clone();
        value["mot_selections"][0]["display"] = owner;
    }
    value
}

pub(super) fn write_response(fixture: &Fixture, label: &str, value: &Value) {
    std::fs::write(
        fixture
            .root
            .path()
            .join(format!("channel-response-{label}.json")),
        serde_json::to_vec(value).unwrap(),
    )
    .unwrap();
}

pub(super) async fn install_contract(fixture: &Fixture, addon: &str, label: &str, schema: u32) {
    let package = fixture.root.path().join(if addon == ADDON {
        "package"
    } else {
        "secondary"
    });
    std::fs::write(
        package.join("fixture.sh"),
        CHANNEL_SCRIPT.replace("LABEL", label),
    )
    .unwrap();
    let mut current = manifest();
    addon.clone_into(&mut current.id);
    current.capabilities[1].id = format!("teletext.pages.v{schema}");
    current.validate().unwrap();
    let hash = calculate_addon_content_sha256(&package).await.unwrap();
    sqlx::query(
        r#"UPDATE "AddonInstallation" SET "ContentSha256"=?, "ManifestJson"=? WHERE "AddonId"=?"#,
    )
    .bind(hash)
    .bind(serde_json::to_string(&current).unwrap())
    .bind(addon)
    .execute(&fixture.state.pool)
    .await
    .unwrap();
}

pub(super) async fn configure_pair(fixture: &Fixture, first: &Value, second: &Value) {
    install_secondary(fixture).await;
    for (addon, label, response) in [(ADDON, "first", first), (SECONDARY, "second", second)] {
        install_contract(
            fixture,
            addon,
            label,
            response["schema_version"]
                .as_u64()
                .unwrap()
                .try_into()
                .unwrap(),
        )
        .await;
        write_response(fixture, label, response);
        let profile = fixture.profile_for(addon, label, "day", "good").await;
        fixture.bind(1, &profile, None).await;
    }
}

pub(super) async fn candidate(
    fixture: &Fixture,
    policy: TeletextChannelDocumentEncodingPolicy,
) -> Result<EncodedChannelTeletextDocumentCandidate, Error> {
    prepare_channel_teletext_document_candidate(
        &fixture.state.application,
        1,
        Some(SECONDARY),
        &guide(&[]),
        policy,
    )
    .await
}

pub(super) fn assert_requests(fixture: &Fixture, first: u32, second: u32) {
    for (label, schema) in [("first", first), ("second", second)] {
        assert_eq!(
            std::fs::read(
                fixture
                    .root
                    .path()
                    .join(format!("channel-input-{label}.json"))
            )
            .unwrap(),
            ersatzrs_app::teletext_guide::build_request_for_schema(&guide(&[]), schema).unwrap(),
        );
    }
    assert_eq!(
        std::fs::read(fixture.root.path().join("channel-calls")).unwrap(),
        format!("first:teletext.pages.v{first}\nsecond:teletext.pages.v{second}\n").as_bytes(),
    );
}

fn display_units(page: &EncodedTeletextAuthoredPage) -> &[[u8; 46]] {
    match page {
        EncodedTeletextAuthoredPage::NativeDisplay(display) => {
            display.encoded().page().data_units()
        }
        EncodedTeletextAuthoredPage::CompiledDisplay(display) => display.page().data_units(),
        _ => panic!("expected complete display, not resource"),
    }
}

fn assert_original_sources(result: &EncodedChannelTeletextDocumentCandidate, values: &[&Value]) {
    assert_eq!(result.sources().len(), values.len());
    for (index, value) in values.iter().enumerate() {
        let source = &result.sources()[index];
        assert_eq!(source.bytes(), serde_json::to_vec(value).unwrap());
        assert_eq!(source.generation(), GENERATION);
        assert_eq!(
            source.contract(),
            result.selection().bindings[index].contract
        );
        assert!(!format!("{source:?}").contains(GENERATION));
        assert!(!format!("{source:?}").contains("ZZ"));
    }
}

#[tokio::test]
async fn mixed_contracts_retain_order_originals_exact_entry_and_complete_native_pages() {
    let mut fixture = Fixture::new().await;
    let first = response(DocumentFixture::Base, "day", 123, 16, "nld");
    let second = response(DocumentFixture::Authored, "night", 124, 32, "fra");
    configure_pair(&fixture, &first, &second).await;
    let config_before: Vec<(Option<String>, Option<String>)> =
        sqlx::query_as("SELECT \"Key\", \"Value\" FROM \"ConfigElement\" ORDER BY \"Key\"")
            .fetch_all(&fixture.state.pool)
            .await
            .unwrap();
    let selected_before = fixture
        .state
        .application
        .channel_teletext_bindings(1)
        .await
        .unwrap();
    let result = candidate(&fixture, policy()).await.unwrap();
    assert_requests(&fixture, 1, 2);
    assert_eq!(result.generation(), GENERATION);
    assert_eq!(result.entry_addon_id(), Some(SECONDARY));
    assert_eq!(
        result
            .selection()
            .bindings
            .iter()
            .map(|s| s.addon_id.as_str())
            .collect::<Vec<_>>(),
        [ADDON, SECONDARY]
    );
    assert_eq!(result.metadata().unwrap().entry, address(124, 32));
    assert_eq!(result.metadata().unwrap().language.as_str(), "fra");
    assert_eq!(result.producers().len(), 2);
    assert_eq!(result.v1_region(), Some(policy().v1_region));
    assert_original_sources(&result, &[&first, &second]);
    let original_base = ersatzrs_app::teletext_pages::parse_page_set(
        &serde_json::to_vec(&first).unwrap(),
        GENERATION,
    )
    .unwrap();
    assert_eq!(
        result.producers()[0].original().semantic().base(),
        &original_base
    );
    let original = result.producers()[1].original().semantic();
    assert!(original.page_enhancements()[&address(124, 32)]
        .presentation
        .is_none());
    assert!(original.magazine_presentations().contains_key(&1));
    for (index, provenance) in result.provenance().iter().enumerate() {
        assert_eq!(provenance.producer_ordinal(), index);
        assert_eq!(provenance.page_ordinal(), 0);
        assert_eq!(
            provenance.address(),
            [address(123, 16), address(124, 32)][index]
        );
    }
    let encoded = result.encoded().unwrap();
    assert_eq!(encoded.pages().len(), 2);
    assert_eq!(
        encoded.declared().semantic().base().pages[0]
            .language
            .as_str(),
        "nld"
    );
    assert_eq!(
        encoded.declared().semantic().base().pages[1]
            .language
            .as_str(),
        "fra"
    );
    assert_eq!(encoded.declared().semantic().base().initial_page.get(), 124);
    assert_eq!(result.descriptor().unwrap().bytes(), b"\x56\x05fra\x09\x24");
    assert!(documents::packet_numbers(display_units(&encoded.pages()[1])).contains(&26));
    let debug = format!("{result:?}");
    assert!(!debug.contains(GENERATION));
    assert!(!debug.contains("ZZ"));
    let config_after: Vec<(Option<String>, Option<String>)> =
        sqlx::query_as("SELECT \"Key\", \"Value\" FROM \"ConfigElement\" ORDER BY \"Key\"")
            .fetch_all(&fixture.state.pool)
            .await
            .unwrap();
    assert_eq!(config_after, config_before);
    assert_eq!(
        fixture
            .state
            .application
            .channel_teletext_bindings(1)
            .await
            .unwrap(),
        selected_before
    );
    assert!(fixture.playout.try_recv().is_err());
    fixture.state.pool.close().await;
}

#[tokio::test]
async fn different_full_magazine_themes_become_page_packets_without_changing_originals() {
    let mut fixture = Fixture::new().await;
    let first = response(DocumentFixture::Authored, "day", 123, 16, "nld");
    let second = response(DocumentFixture::Authored, "night", 124, 32, "fra");
    configure_pair(&fixture, &first, &second).await;
    let result = candidate(&fixture, policy()).await.unwrap();
    assert!(result.v1_region().is_none());
    assert_original_sources(&result, &[&first, &second]);
    let encoded = result.encoded().unwrap();
    assert!(encoded.magazine_units().is_empty());
    for (index, rgb) in [[15, 0, 0], [0, 0, 15]].into_iter().enumerate() {
        let producer = &result.producers()[index];
        let original = producer.original().semantic();
        let owner = original.base().pages[0].address;
        assert!(original.page_enhancements()[&owner].presentation.is_none());
        assert_eq!(
            producer.pages()[0].source(),
            TeletextProducerPresentationSource::Magazine { magazine: 1 }
        );
        assert_eq!(
            result.provenance()[index].derivation(),
            TeletextProducerPageDerivation::MagazineToPage { magazine: 1 }
        );
        let composed = encoded.declared().semantic().page_enhancements()[&owner]
            .presentation
            .unwrap();
        assert_eq!(composed, original.magazine_presentations()[&1]);
        assert!(documents::packet_numbers(display_units(&encoded.pages()[index])).contains(&28));
        let EncodedTeletextAuthoredPage::CompiledDisplay(display) = &encoded.pages()[index] else {
            panic!("actual nonempty Local needs compiled display")
        };
        let raster = render_teletext_composition(
            display.normalized().original(),
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
        assert!((144..156).any(|x| (10..20).any(|y| matches!(raster.pixel(x,y),
            Some(TeletextRasterPixel::Colour(TeletextColourPixel::Rgb(colour))) if colour.get()==rgb))));
    }
    assert_requests(&fixture, 2, 2);
    assert!(fixture.playout.try_recv().is_err());
    fixture.state.pool.close().await;
}

fn add_second_subcode(value: &mut Value, subcode: u16) {
    let mut page = value["pages"][0].clone();
    page["address"]["subcode"] = json!(subcode);
    value["pages"].as_array_mut().unwrap().push(page);
    let mut enhancement = value["page_enhancements"][0].clone();
    enhancement["address"]["subcode"] = json!(subcode);
    value["page_enhancements"]
        .as_array_mut()
        .unwrap()
        .push(enhancement);
    let mut selection = value["mot_selections"][0].clone();
    selection["display"]["subcode"] = json!(subcode);
    value["mot_selections"]
        .as_array_mut()
        .unwrap()
        .push(selection);
}

#[tokio::test]
async fn ambiguous_entry_requires_exact_original_subcode_and_cannot_select_foreign_page() {
    let fixture = Fixture::new().await;
    let first = response(DocumentFixture::Base, "day", 123, 16, "nld");
    let mut second = response(DocumentFixture::Authored, "night", 124, 32, "fra");
    add_second_subcode(&mut second, 48);
    configure_pair(&fixture, &first, &second).await;
    assert!(matches!(
        candidate(&fixture, policy()).await,
        Err(Error::EntryRequired { binding: 1 })
    ));
    for entry in [address(124, 0), address(123, 16), address(124, 64)] {
        let mut explicit = policy();
        explicit.entry = Some(entry);
        assert!(
            matches!(candidate(&fixture,explicit).await,Err(Error::EntryAddress {binding:1,entry:actual}) if actual==entry)
        );
    }
    let mut explicit = policy();
    explicit.entry = Some(address(124, 48));
    let result = candidate(&fixture, explicit).await.unwrap();
    assert_eq!(result.metadata().unwrap().entry, address(124, 48));
    assert_eq!(result.metadata().unwrap().language.as_str(), "fra");
    assert_eq!(
        result.producers()[1]
            .pages()
            .iter()
            .map(TeletextProducerPagePresentation::address)
            .collect::<Vec<_>>(),
        [address(124, 32), address(124, 48)]
    );
    assert_eq!(result.encoded().unwrap().pages().len(), 3);
    fixture.state.pool.close().await;
}

#[tokio::test]
async fn off_invokes_nobody_and_has_no_composed_or_native_partial_result() {
    let mut fixture = Fixture::new().await;
    configure_pair(
        &fixture,
        &response(DocumentFixture::Base, "day", 123, 16, "nld"),
        &response(DocumentFixture::Authored, "night", 124, 32, "fra"),
    )
    .await;
    for selected in fixture
        .state
        .application
        .channel_teletext_bindings(1)
        .await
        .unwrap()
    {
        saved(
            fixture
                .state
                .application
                .disable_channel_teletext_binding(
                    1,
                    &selected.binding.addon_id,
                    selected.binding.revision,
                )
                .await
                .unwrap(),
        );
    }
    let mut unused = policy();
    unused.composition.max_producers = 0;
    unused.encoding.max_units = 0;
    unused.entry = Some(address(124, 0));
    let result = candidate(&fixture, unused).await.unwrap();
    assert!(result.selection().bindings.is_empty());
    assert!(result.entry_addon_id().is_none());
    assert!(result.producers().is_empty());
    assert!(result.sources().is_empty());
    assert!(result.v1_region().is_none());
    assert!(result.provenance().is_empty());
    assert!(result.metadata().is_none());
    assert!(result.encoded().is_none());
    assert!(result.descriptor().is_none());
    assert!(!fixture.root.path().join("channel-calls").exists());
    assert!(fixture.playout.try_recv().is_err());
    fixture.state.pool.close().await;
}

#[tokio::test]
async fn resource_lifecycle_and_composition_encoding_budgets_fail_as_whole_candidates() {
    let mut fixture = Fixture::new().await;
    configure_pair(
        &fixture,
        &response(DocumentFixture::Base, "day", 124, 32, "fra"),
        &response(DocumentFixture::Companion, "day", 123, 16, "nld"),
    )
    .await;
    assert!(
        matches!(candidate(&fixture,policy()).await,Err(Error::Encoding(error)) if matches!(error.reason,TeletextAuthoredServiceEncodingFailure::MissingTransmission))
    );
    let mut complete = policy();
    complete
        .transmissions
        .push(documents::companion_transmission());
    let result = candidate(&fixture, complete).await.unwrap();
    assert_eq!(result.encoded().unwrap().pages().len(), 3);
    assert_eq!(
        result.producers()[1].pages()[0].local_program_presence(),
        TeletextProducerLocalProgramPresence::Absent
    );
    assert_eq!(
        result.producers()[1].pages()[0].secondary_selection(),
        TeletextProducerSecondarySelection::DecoderDependent
    );
    assert!(matches!(
        result.encoded().unwrap().pages()[2],
        EncodedTeletextAuthoredPage::Object(_)
    ));
    let mut limited = policy();
    limited.composition.max_input_items = 0;
    assert!(
        matches!(candidate(&fixture,limited).await,Err(Error::Composition(error)) if matches!(error.reason,TeletextProducerCompositionFailure::Budget{..}))
    );
    let mut limited = policy();
    limited.encoding.max_units = 0;
    limited
        .transmissions
        .push(documents::companion_transmission());
    assert!(matches!(
        candidate(&fixture, limited).await,
        Err(Error::Encoding(_))
    ));
    assert!(fixture.playout.try_recv().is_err());
    fixture.state.pool.close().await;
}

#[tokio::test]
async fn raw_budget_and_wrong_second_contract_do_not_return_a_first_producer_candidate() {
    let mut fixture = Fixture::new().await;
    let first = response(DocumentFixture::Base, "day", 123, 16, "nld");
    let mut second = response(DocumentFixture::Authored, "night", 124, 32, "fra");
    configure_pair(&fixture, &first, &second).await;
    let mut limited = policy();
    limited.input.max_response_bytes = serde_json::to_vec(&first).unwrap().len();
    assert!(matches!(
        candidate(&fixture, limited).await,
        Err(Error::Preparation(
            ChannelTeletextDocumentError::AggregateResponseBudget { binding: 1, .. }
        ))
    ));
    second["schema_version"] = json!(1);
    write_response(&fixture, "second", &second);
    assert!(matches!(
        candidate(&fixture, policy()).await,
        Err(Error::Preparation(ChannelTeletextDocumentError::Document {
            binding: 1,
            ..
        }))
    ));
    let calls = std::fs::read_to_string(fixture.root.path().join("channel-calls")).unwrap();
    assert_eq!(calls,"first:teletext.pages.v1\nsecond:teletext.pages.v2\nfirst:teletext.pages.v1\nsecond:teletext.pages.v2\n");
    assert!(fixture.playout.try_recv().is_err());
    fixture.state.pool.close().await;
}

#[tokio::test]
async fn retained_native_candidate_uses_whole_channel_freshness_for_member_profile_and_settings_changes(
) {
    for change in ["membership", "profile", "settings"] {
        let mut fixture = Fixture::new().await;
        configure_pair(
            &fixture,
            &response(DocumentFixture::Base, "day", 123, 16, "nld"),
            &response(DocumentFixture::Authored, "night", 124, 32, "fra"),
        )
        .await;
        let result = candidate(&fixture, policy()).await.unwrap();
        let second = &result.selection().bindings[1];
        match change {
            "membership" => {
                saved(
                    fixture
                        .state
                        .application
                        .disable_channel_teletext_binding(1, SECONDARY, second.binding_revision)
                        .await
                        .unwrap(),
                );
            }
            "profile" => {
                let replacement = fixture
                    .profile_for(SECONDARY, "Replacement", "night", "good")
                    .await;
                fixture
                    .bind(1, &replacement, Some(second.binding_revision))
                    .await;
            }
            "settings" => {
                sqlx::query(r#"UPDATE "TeletextProfileSetting" SET "Value"='night' WHERE "ProfileId"=? AND "Key"='THEME'"#)
                    .bind(second.profile_id).execute(&fixture.state.pool).await.unwrap();
            }
            _ => unreachable!(),
        }
        let rejected = fixture
            .state
            .application
            .recheck_channel_teletext_document_selection(result.selection(), result.deadline())
            .await;
        assert!(
            matches!(
                rejected,
                Err(ChannelTeletextDocumentError::SelectionChanged
                    | ChannelTeletextDocumentError::SelectionRejected(_))
            ),
            "{change}: {rejected:?}"
        );
        // The rejected freshness check never rewrites the already-owned diagnostic candidate.
        assert_eq!(
            result.producers()[1].original().semantic().base().pages[0].address,
            address(124, 32)
        );
        assert!(fixture.playout.try_recv().is_err());
        fixture.state.pool.close().await;
    }
}
