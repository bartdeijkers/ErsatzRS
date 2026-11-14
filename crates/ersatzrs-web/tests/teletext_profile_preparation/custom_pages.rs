//! Host-owned custom pages (ADR 0120 amendment of 2026-11-07): linked teletext
//! elements reach Check, the stored revision and the encoder as exact Level 1
//! rows, and blocked pages stop Check with a per-page reason.

use super::{authoring::setting, channel_documents, designs, *};
use ersatzrs_app::teletext_elements::broadcast::CustomPageIssueKind;
use ersatzrs_web::host::{
    teletext_authoring::{
        observe_teletext_authoring_selection, prepare_static_teletext_design,
        TeletextAuthoringPrepare, TeletextAuthoringScope, TeletextAuthoringStatus,
    },
    teletext_designs::reload_original_teletext_design_revision,
    teletext_work::MAX_CONCURRENT_SETTING,
};
use serde_json::json;
use std::time::Instant;

const CHANNEL: i64 = 1;

/// One page with a double-height title (row 2 left empty below it), a
/// held-mosaic row and a bound field.
fn element_tti(marker: &str) -> Vec<u8> {
    let mut file = Vec::new();
    file.extend_from_slice(b"DE,Welkom\r\nPN,15000\r\nSC,0000\r\nPS,8000\r\n");
    file.extend_from_slice(b"OL,1,\x1bD\x1b]\x1bMWELKOM\r\n");
    file.extend_from_slice(b"OL,3,\x1bW\x1b^\x7f\x7f\x7f\x1bI\x1bY\x70\x70\r\n");
    file.extend_from_slice(format!("OL,4,\x1bFKanaal: \x1bC{marker}\r\n").as_bytes());
    file
}

async fn fixture() -> Fixture {
    let fixture = Fixture::new_persistent().await;
    setting(
        &fixture,
        "teletext.authoring.policy",
        &super::authoring::policy().to_string(),
    )
    .await;
    setting(&fixture, "pages.language", "en-us").await;
    setting(&fixture, MAX_CONCURRENT_SETTING, "1").await;
    fixture
}

async fn link(fixture: &Fixture, marker: &str, page: Option<u16>) -> i64 {
    let application = &fixture.state.application;
    let upload = application
        .upload_teletext_element(
            &format!("welkom-{}.tti", marker.len()),
            &element_tti(marker),
        )
        .await
        .unwrap();
    application
        .save_channel_teletext_element(CHANNEL, upload.id, page, true)
        .await
        .unwrap();
    upload.id
}

fn request(digest: String) -> TeletextAuthoringPrepare {
    TeletextAuthoringPrepare {
        scope: TeletextAuthoringScope::Base,
        channel_id: CHANNEL,
        selection_digest: digest,
        name: "Custom pages".to_owned(),
        input_json: json!({
            "input_version":1,"channel_id":CHANNEL,
            "entry_addon_id":ersatzrs_domain::TELETEXT_CUSTOM_PAGES_PRODUCER_ID,
            "entry":{"page":150,"subcode":0},"v1_region":"western_latin",
            "resource_transmissions":[]})
        .to_string(),
    }
}

async fn observe(fixture: &Fixture) -> String {
    observe_teletext_authoring_selection(&fixture.state.application, CHANNEL, designs::deadline())
        .await
        .unwrap()
}

/// Seven-bit row contents of every encoded packet (EN 300 472 data units
/// carry each packet bit-reversed after a four-byte unit header).
fn packet_rows(
    encoded: &ersatzrs_ffmpeg::teletext::EncodedTeletextAuthoredService,
) -> Vec<[u8; 40]> {
    designs::units(encoded)
        .into_iter()
        .flatten()
        .map(|unit| std::array::from_fn(|index| unit[6 + index].reverse_bits() & 0x7f))
        .collect()
}

#[tokio::test]
async fn linked_element_reaches_check_storage_and_encoder_as_exact_rows() {
    let fixture = fixture().await;
    let element = link(&fixture, "%KANAAL%%%%%%%", None).await;
    let selection = fixture
        .state
        .application
        .observe_channel_teletext_document_selection(CHANNEL)
        .await
        .unwrap();
    assert_eq!(
        selection.bindings,
        vec![ersatzrs_domain::teletext_custom_pages_selection(CHANNEL)]
    );
    let expected = fixture
        .state
        .application
        .preview_teletext_element(element, Some(CHANNEL), chrono::Utc::now())
        .await
        .unwrap()
        .page
        .subpages[0]
        .rows;
    assert!(expected[4].starts_with(b"\x06Kanaal: \x03Synthetic 1"));

    let observed = observe(&fixture).await;
    let saved = prepare_static_teletext_design(
        &fixture.state.application,
        request(observed.clone()),
        designs::deadline(),
    )
    .await
    .unwrap();
    let preview = saved.preview.unwrap();
    assert_eq!(preview.pages(), &[channel_documents::address(150, 0)]);
    assert!(preview.raster().unwrap().diagnostics().is_empty());
    let identity = saved.identity;
    drop(preview);

    // Editing the element changes neither the selection nor the stored revision.
    let stored = fixture
        .state
        .application
        .get_teletext_element(element)
        .await
        .unwrap()
        .unwrap();
    fixture
        .state
        .application
        .save_teletext_element_details(element, stored.revision, "Renamed", 0x150, 10)
        .await
        .unwrap();
    assert_eq!(observe(&fixture).await, observed);

    let historical = reload_original_teletext_design_revision(
        &fixture.state.application,
        identity.design_id,
        identity.revision,
        designs::budget(),
        channel_documents::policy(),
        designs::deadline(),
    )
    .await
    .unwrap()
    .unwrap();
    let rows = packet_rows(historical.encoded().unwrap());
    for row in [1, 3, 4] {
        assert!(
            rows.contains(&expected[row]),
            "row {row} is not carried byte for byte"
        );
    }
    let source: serde_json::Value =
        serde_json::from_slice(historical.sources()[0].bytes()).unwrap();
    assert_eq!(source["pages"][0]["element_id"], element);
    assert_eq!(source["pages"][0]["element_revision"], stored.revision);
}

#[tokio::test]
async fn stored_dynamic_field_recipe_ignores_later_element_binding_edits() {
    let fixture = fixture().await;
    let element = link(&fixture, "%NOW%%%%%%%%%%", None).await;
    let saved = prepare_static_teletext_design(
        &fixture.state.application,
        request(observe(&fixture).await),
        designs::deadline(),
    )
    .await
    .unwrap();
    let identity = saved.identity;
    drop(saved.preview);
    let stored = fixture
        .state
        .application
        .get_teletext_element(element)
        .await
        .unwrap()
        .unwrap();
    let mut bindings = stored.bindings.clone();
    bindings[0].source = ersatzrs_domain::TeletextElementBindingSource::Time;
    fixture
        .state
        .application
        .save_teletext_element_bindings(element, stored.revision, bindings)
        .await
        .unwrap();
    let historical = reload_original_teletext_design_revision(
        &fixture.state.application,
        identity.design_id,
        identity.revision,
        designs::budget(),
        channel_documents::policy(),
        designs::deadline(),
    )
    .await
    .unwrap()
    .unwrap();
    let source: ersatzrs_domain::TeletextCustomPagesWire =
        serde_json::from_slice(historical.sources()[0].bytes()).unwrap();
    assert_eq!(source.pages[0].element_revision, stored.revision);
    assert_eq!(source.pages[0].dynamic_fields, stored.bindings);
    let updated = fixture
        .state
        .application
        .plan_teletext_custom_pages(CHANNEL, chrono::Utc::now())
        .await
        .unwrap();
    assert_eq!(
        updated.pages[0].dynamic_fields[0].source,
        ersatzrs_domain::TeletextElementBindingSource::Time
    );
    let refreshed = ersatzrs_app::teletext_elements::refresh::refresh_published_custom_pages(
        &source,
        ersatzrs_app::teletext_elements::refresh::CustomPageRefreshTriggers {
            programme: true,
            ..Default::default()
        },
        Some(&ersatzrs_app::teletext_elements::fields::FieldValues {
            now_title: "Published recipe".to_owned(),
            time: "23:45".to_owned(),
            ..Default::default()
        }),
    )
    .unwrap();
    let cells = refreshed.document.pages[0]
        .rows
        .iter()
        .find(|row| row.row == 4)
        .unwrap()
        .decode()
        .unwrap();
    assert!(cells.starts_with(b"\x06Kanaal: \x03Published reci"));
    assert_eq!(refreshed.changed, [(150, 0)]);
}

async fn saved_source(fixture: &Fixture) -> ersatzrs_domain::TeletextCustomPagesWire {
    let saved = prepare_static_teletext_design(
        &fixture.state.application,
        request(observe(fixture).await),
        designs::deadline(),
    )
    .await
    .unwrap();
    let identity = saved.identity;
    drop(saved);
    let original = reload_original_teletext_design_revision(
        &fixture.state.application,
        identity.design_id,
        identity.revision,
        designs::budget(),
        channel_documents::policy(),
        designs::deadline(),
    )
    .await
    .unwrap()
    .unwrap();
    serde_json::from_slice(original.sources()[0].bytes()).unwrap()
}

async fn reproduce(
    fixture: &Fixture,
    selection: &ersatzrs_domain::TeletextChannelSelectionSnapshot,
    source: &ersatzrs_domain::TeletextCustomPagesWire,
    budget: ersatzrs_app::teletext_channel_documents::TeletextChannelDocumentInputBudget,
    deadline: Instant,
) -> Result<
    ersatzrs_app::teletext_channel_documents::BoundChannelTeletextDocumentInput,
    ersatzrs_app::teletext_channel_documents::ChannelTeletextDocumentError,
> {
    let guard = ersatzrs_web::host::teletext_work::begin(&fixture.state.application, None)
        .await
        .unwrap();
    fixture
        .state
        .application
        .invoke_channel_teletext_documents_for_refresh(
            CHANNEL,
            Some(ersatzrs_domain::TELETEXT_CUSTOM_PAGES_PRODUCER_ID),
            &guide(&[]),
            budget,
            ersatzrs_app::teletext_channel_documents::TeletextChannelDocumentRefresh {
                selection,
                custom_pages: source,
                deadline,
                hold: guard.hold_work(),
            },
        )
        .await
}

#[tokio::test]
async fn guide_reproduction_keeps_saved_custom_rows_after_edit_delete_and_restart() {
    let mut fixture = fixture().await;
    let element = link(&fixture, "%NOW%%%%%%%%%%", None).await;
    let source = saved_source(&fixture).await;
    let selection = fixture
        .state
        .application
        .observe_channel_teletext_document_selection(CHANNEL)
        .await
        .unwrap();
    let stored = fixture
        .state
        .application
        .get_teletext_element(element)
        .await
        .unwrap()
        .unwrap();
    let mut bindings = stored.bindings.clone();
    bindings[0].source = ersatzrs_domain::TeletextElementBindingSource::Time;
    fixture
        .state
        .application
        .save_teletext_element_bindings(element, stored.revision, bindings)
        .await
        .unwrap();
    // A new linked page retains the host producer's identity. Neither it nor
    // later bindings may replace the saved original during automatic refresh.
    link(&fixture, "%UNKNOWN%%%%%%", Some(0x151)).await;
    fixture
        .state
        .application
        .delete_teletext_element(element)
        .await
        .unwrap();
    fixture = fixture.reopen().await;
    assert_eq!(
        fixture
            .state
            .application
            .observe_channel_teletext_document_selection(CHANNEL)
            .await
            .unwrap(),
        selection
    );
    let latest = fixture
        .state
        .application
        .plan_teletext_custom_pages(CHANNEL, Utc::now())
        .await
        .unwrap();
    assert!(latest.pages.is_empty());
    assert_eq!(latest.issues.len(), 1);
    assert_eq!(latest.issues[0].page, "151");
    assert_eq!(latest.issues[0].kind, CustomPageIssueKind::UnsetField);
    let policy = channel_documents::policy();
    let refreshed = reproduce(
        &fixture,
        &selection,
        &source,
        policy.input,
        designs::deadline(),
    )
    .await
    .unwrap()
    .validate(policy.validation)
    .unwrap()
    .into_parts_with_requests();
    let actual: ersatzrs_domain::TeletextCustomPagesWire =
        serde_json::from_slice(refreshed.sources[0].bytes()).unwrap();
    assert_ne!(source.generation, actual.generation);
    assert_eq!(actual.generation, GENERATION);
    assert_eq!(actual.pages, source.pages);
    assert_eq!(
        actual.pages[0].dynamic_fields[0].source,
        ersatzrs_domain::TeletextElementBindingSource::NowTitle
    );
    assert_eq!(actual.pages[0].element_revision, stored.revision);
    assert_eq!(refreshed.selection, selection);
    assert!(fixture.playout.try_recv().is_err());
}

#[tokio::test]
async fn published_custom_reproduction_preserves_selection_deadline_and_response_fences() {
    use ersatzrs_app::teletext_channel_documents::ChannelTeletextDocumentError as Error;
    let fixture = fixture().await;
    link(&fixture, "%NOW%%%%%%%%%%", None).await;
    let source = saved_source(&fixture).await;
    let selection = fixture
        .state
        .application
        .observe_channel_teletext_document_selection(CHANNEL)
        .await
        .unwrap();
    let budget = channel_documents::policy().input;
    let mut stale = selection.clone();
    stale.bindings[0].binding_revision += 1;
    assert!(matches!(
        reproduce(&fixture, &stale, &source, budget, designs::deadline()).await,
        Err(Error::SelectionChanged)
    ));
    assert!(matches!(
        reproduce(
            &fixture,
            &selection,
            &source,
            budget,
            Instant::now().checked_sub(Duration::from_secs(1)).unwrap()
        )
        .await,
        Err(Error::TimedOut)
    ));
    assert!(matches!(
        reproduce(
            &fixture,
            &selection,
            &source,
            ersatzrs_app::teletext_channel_documents::TeletextChannelDocumentInputBudget {
                max_response_bytes: 1,
                ..budget
            },
            designs::deadline()
        )
        .await,
        Err(Error::AggregateResponseBudget { .. })
    ));
    let mut invalid = source.clone();
    invalid.pages[0].rows[0].cells = "ff".repeat(40);
    assert!(matches!(
        reproduce(&fixture, &selection, &invalid, budget, designs::deadline()).await,
        Err(Error::Runtime { .. })
    ));
    // Refusal does not poison the immutable source or renew the failed deadline.
    assert!(
        reproduce(&fixture, &selection, &source, budget, designs::deadline())
            .await
            .is_ok()
    );
}

#[tokio::test]
async fn blocked_pages_stop_check_with_a_reason_per_page() {
    let fixture = fixture().await;
    let application = &fixture.state.application;
    link(&fixture, "%UNKNOWN%%%", None).await;
    let plan = application
        .plan_teletext_custom_pages(CHANNEL, chrono::Utc::now())
        .await
        .unwrap();
    assert_eq!(plan.issues.len(), 1);
    assert_eq!(plan.issues[0].kind, CustomPageIssueKind::UnsetField);
    assert_eq!(plan.issues[0].page, "150");
    let error = prepare_static_teletext_design(
        application,
        request(observe(&fixture).await),
        designs::deadline(),
    )
    .await
    .err()
    .unwrap();
    assert_eq!(error.status, TeletextAuthoringStatus::CustomPagesBlocked);

    // Linking refuses a number already in use, but editing an element's own
    // page number afterwards can still make two links meet. That is a
    // conflict for both pages, never precedence.
    let second = application
        .upload_teletext_element("second.tti", &element_tti("%KANAAL%%%"))
        .await
        .unwrap();
    let error = application
        .save_channel_teletext_element(CHANNEL, second.id, None, true)
        .await
        .err()
        .unwrap();
    assert_eq!(
        error,
        ersatzrs_app::teletext_elements::TeletextElementError::PageInUse
    );
    let second_id = second.id;
    let rename = |page| async move {
        let stored = application
            .get_teletext_element(second_id)
            .await
            .unwrap()
            .unwrap();
        application
            .save_teletext_element_details(second_id, stored.revision, "Second", page, 10)
            .await
            .unwrap();
    };
    rename(0x151).await;
    application
        .save_channel_teletext_element(CHANNEL, second.id, None, true)
        .await
        .unwrap();
    rename(0x150).await;
    let plan = application
        .plan_teletext_custom_pages(CHANNEL, chrono::Utc::now())
        .await
        .unwrap();
    assert!(plan
        .issues
        .iter()
        .all(|issue| issue.kind == CustomPageIssueKind::DuplicatePage));
    assert_eq!(plan.issues.len(), 2);
    assert!(plan.pages.is_empty());
}

#[tokio::test]
async fn switching_every_page_off_removes_the_host_source() {
    let fixture = fixture().await;
    let application = &fixture.state.application;
    let element = link(&fixture, "%KANAAL%%%", None).await;
    application
        .save_channel_teletext_element(CHANNEL, element, None, false)
        .await
        .unwrap();
    let selection = application
        .observe_channel_teletext_document_selection(CHANNEL)
        .await
        .unwrap();
    assert!(selection.bindings.is_empty());
}

#[tokio::test]
async fn text_hidden_under_a_double_height_line_blocks_the_page() {
    let fixture = fixture().await;
    let application = &fixture.state.application;
    let mut file = b"DE,Hidden\r\nPN,16000\r\nSC,0000\r\nPS,8000\r\n".to_vec();
    file.extend_from_slice(b"OL,1,\x1bMGROOT\r\nOL,2,verborgen\r\n");
    let upload = application
        .upload_teletext_element("hidden.tti", &file)
        .await
        .unwrap();
    application
        .save_channel_teletext_element(CHANNEL, upload.id, None, true)
        .await
        .unwrap();
    let plan = application
        .plan_teletext_custom_pages(CHANNEL, chrono::Utc::now())
        .await
        .unwrap();
    assert_eq!(plan.issues.len(), 1);
    assert_eq!(plan.issues[0].kind, CustomPageIssueKind::DoubleHeight);
    assert_eq!(plan.issues[0].page, "160");
}
