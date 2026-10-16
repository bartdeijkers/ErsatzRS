//! Genuine Polish QTM upload through headed SSR forms on the ordinary TCP host.
//! Native barriers authenticate durable intent before explicit publication;
//! external browser images remain separate from decoder/client acceptance.

use super::super::{client_capture, ordinary_stream_flow};
use super::polish_delivery::{self, Retained};
use super::resource_bank_admin::{marker, BrowserComplete, BrowserConcept};
use super::*;
use ersatzrs_app::{
    teletext_design_documents::decode_teletext_design_documents,
    teletext_export_project::parse_teletext_export_receipt_body,
};
use ersatzrs_web::host::teletext_designs::RestoredOriginalTeletextDesign;
use futures_util::FutureExt as _;
use serde_json::{json, Value};
use std::{io, panic::AssertUnwindSafe, path::PathBuf};

fn browser_ready(origin: &live_client::LiveOrigin, flow: &Flow, actual: &Path) -> Value {
    let channels: Vec<_> = flow.accepted.iter().enumerate().map(|(index,candidate)| {
        let channel = index + 1;
        let binding = &candidate.selection().bindings[0];
        let saved = &flow.saved[index];
        json!({"channel_id":channel,"theme":if index==0 {"day"} else {"night"},
            "addon_id":binding.addon_id,"profile_id":binding.profile_id,
            "profile_revision":binding.profile_revision,"design_id":saved.head().id,
            "revision":1,"digest":saved.digest(),"original_digest":saved.digest(),
            "generation_id":candidate.generation().id,
            "admin_url":format!("{}/channels/{channel}/teletext",origin.base_url()),
            "original_preview_url":format!("{}/channels/{channel}/teletext?design_id={}&revision=1&page=124&subcode=0020",origin.base_url(),saved.head().id)})
    }).collect();
    let name = &flow.exported[0].sidecar().project.pages[0].name;
    assert_eq!(name, "page-124-0020.tti");
    let ready = json!({"probe_version":1,"base_url":origin.base_url(),"channels":channels,
        "editor":{"name":name,"original_path":actual.join("native-original-1").join(name),
            "edited_path":actual.join("editor-1/edited.tti"),
            "original_sha256":"a1cb39545a3ed73c13ff7cafc14b999b4a677ef39d1604b434aabee3453e9e3a",
            "edited_sha256":"962254bbb0427b5e9d1ff4d25f24d59fb5354203f61c3f1c4db2ae63b59f00fe"},
        "completion_file":"browser-complete.json","timeout_seconds":600,
        "markers":{"concept":"browser-concept-ready.json","publication":"publication-ready.json","complete":"browser-complete.json"}});
    assert!(serde_json::to_vec(&ready).unwrap().len() <= 64 * 1024);
    ready
}

async fn fresh_receipt(
    fixture: &Fixture,
    flow: &Flow,
    provenance: &ersatzrs_app::teletext_design_documents::TeletextSavedEditorialProvenance,
) -> TeletextDesignExportReceipt {
    assert_eq!(provenance.original_design_id, flow.saved[0].head().id);
    assert_eq!(provenance.original_revision, 1);
    assert_eq!(provenance.original_digest, flow.originals[0].digest);
    assert!(!flow
        .receipts
        .iter()
        .any(|old| old.metadata.id == provenance.export_receipt_id));
    let receipt = fixture
        .state
        .application
        .teletext_design_export_receipt(
            provenance.export_receipt_id,
            policy().project.max_bytes,
            designs::budget().max_serialized_bytes,
        )
        .await
        .unwrap()
        .unwrap();
    assert_eq!(receipt.metadata.digest, provenance.export_receipt_digest);
    assert_eq!(receipt.metadata.design_id, provenance.original_design_id);
    assert_eq!(receipt.metadata.revision, 1);
    assert_eq!(receipt.metadata.design_digest, flow.originals[0].digest);
    let body = parse_teletext_export_receipt_body(&receipt.receipt, policy().project).unwrap();
    assert_eq!(body.project, flow.exported[0].sidecar().project);
    assert_eq!(body.files, flow.exported[0].files());
    receipt
}

fn assert_row_overlay(flow: &Flow, document: &[u8]) {
    let documents = decode_teletext_design_documents(document, designs::budget()).unwrap();
    let overlay = documents.editorial_overlay().unwrap();
    assert!(overlay.local_program_operands().is_empty());
    assert!(overlay.drcs_bitmap_overlays().is_empty());
    assert_eq!(overlay.pages().len(), 1);
    let page = &overlay.pages()[0];
    assert_eq!(page.owner(), channel_documents::address(124, 32));
    assert_eq!(page.rows().len(), 1);
    assert_eq!(page.rows()[0].row(), 1);
    let original = flow.exported[0]
        .files()
        .iter()
        .find(|file| file.name == "page-124-0020.tti")
        .unwrap();
    let tti = parse_teletext_tti(&original.bytes, policy().tti).unwrap();
    let mut cells = *tti
        .records()
        .iter()
        .find_map(|record| match record {
            TeletextTtiRecord::BaseRow { row: 1, cells } => Some(cells),
            _ => None,
        })
        .unwrap();
    assert_eq!(cells[7], b'A');
    cells[7] = 0x5b;
    assert_eq!(*page.rows()[0].cells(), cells);
    let mut value: Value = serde_json::from_slice(document).unwrap();
    value.as_object_mut().unwrap().remove("editorial_overlay");
    assert_eq!(
        value,
        serde_json::from_slice::<Value>(&flow.originals[0].document).unwrap()
    );
}

async fn verified_concept(
    fixture: &Fixture,
    flow: &Flow,
    concept: &BrowserConcept,
) -> TeletextDesignExportReceipt {
    assert_eq!(concept.probe_version, 1);
    assert_eq!(concept.design_id, flow.saved[0].head().id);
    assert_eq!(concept.revision, 2);
    let revision = fixture
        .state
        .application
        .teletext_design_revision(concept.design_id, 2, designs::budget().max_serialized_bytes)
        .await
        .unwrap()
        .unwrap();
    assert_eq!(revision.digest, concept.digest);
    assert_row_overlay(flow, &revision.document);
    let documents =
        decode_teletext_design_documents(&revision.document, designs::budget()).unwrap();
    let receipt = fresh_receipt(
        fixture,
        flow,
        documents.editorial_overlay().unwrap().provenance(),
    )
    .await;
    let restored = restore_concept(fixture, concept).await;
    assert_polish_effect(flow, &restored);
    assert_original_pointers(fixture, flow).await;
    receipt
}

async fn restore_concept(
    fixture: &Fixture,
    concept: &BrowserConcept,
) -> RestoredOriginalTeletextDesign {
    reload_original_teletext_design_revision(
        &fixture.state.application,
        concept.design_id,
        concept.revision,
        designs::budget(),
        channel_documents::policy(),
        designs::deadline(),
    )
    .await
    .unwrap()
    .unwrap()
}

fn assert_polish_effect(flow: &Flow, restored: &RestoredOriginalTeletextDesign) {
    assert_eq!(restored.selection(), flow.saved[0].candidate().selection());
    assert_eq!(
        restored.transmissions(),
        flow.saved[0].candidate().transmissions()
    );
    let service = restored.encoded().unwrap();
    polish_carrier::assert_base(polish_carrier::composition(service), 0x5b);
    assert_eq!(
        service.pages().len(),
        flow.accepted[0].service().pages().len()
    );
    for (before, after) in flow.accepted[0]
        .service()
        .pages()
        .iter()
        .zip(service.pages())
    {
        if matches!(before, EncodedTeletextAuthoredPage::CompiledDisplay(page)
            if page.page().address() == channel_documents::address(124,32))
        {
            continue;
        }
        assert_eq!(before.data_units(), after.data_units());
    }
    assert_ne!(
        external_editor_probe::preview_page(service, 124),
        external_editor_probe::preview_page(flow.accepted[0].service(), 124)
    );
}

async fn published(
    fixture: &Fixture,
    flow: &Flow,
    concept: &BrowserConcept,
    complete: &BrowserComplete,
    directory: &Path,
) -> [AcceptedTeletextOutputCandidate; 2] {
    assert_eq!(complete.probe_version, 1);
    assert_eq!(complete.design_id, concept.design_id);
    assert_eq!(complete.revision, concept.revision);
    assert_eq!(complete.digest, concept.digest);
    let accepted = [recover(fixture, 1).await, recover(fixture, 2).await];
    let day = accepted[0].generation();
    assert_eq!(day.id, complete.generation_id);
    assert_ne!(day.id, flow.accepted[0].generation().id);
    assert_eq!(day.request.id, complete.request_id);
    assert_eq!(day.request.design_id, concept.design_id);
    assert_eq!(day.request.revision, 2);
    assert_eq!(day.request.digest, concept.digest);
    assert_eq!(
        fixture
            .state
            .application
            .teletext_publication_request(1)
            .await
            .unwrap(),
        Some(day.request.clone())
    );
    assert_eq!(accepted[0].selection(), flow.accepted[0].selection());
    generations::assert_same_output(&accepted[1], &flow.accepted[1]);
    assert_eq!(
        fixture
            .state
            .application
            .teletext_publication_request(2)
            .await
            .unwrap(),
        Some(flow.pending[1].clone())
    );
    let restored = restore_concept(fixture, concept).await;
    assert_polish_effect(flow, &restored);
    polish_carrier::assert_base(polish_carrier::composition(accepted[0].service()), 0x5b);
    polish_delivery::assert_published_previews(directory, restored.encoded().unwrap(), &accepted);
    accepted
}

async fn browser_publication(
    fixture: &mut Fixture,
    flow: &Flow,
    retained: &Retained,
    actual: &Path,
    directory: &Path,
) -> io::Result<(
    [AcceptedTeletextOutputCandidate; 2],
    TeletextDesignExportReceipt,
)> {
    tokio::time::timeout(Duration::from_mins(10), async {
        let concept: BrowserConcept = marker(directory, "browser-concept-ready.json").await?;
        let receipt = verified_concept(fixture, flow, &concept).await;
        retained
            .assert_unchanged(fixture, flow, &flow.accepted, actual)
            .await;
        polish_delivery::write_json(directory, "publication-ready.json", &concept);
        let complete: BrowserComplete = marker(directory, "browser-complete.json").await?;
        let accepted = published(fixture, flow, &concept, &complete, directory).await;
        retained
            .assert_unchanged(fixture, flow, &accepted, actual)
            .await;
        Ok((accepted, receipt))
    })
    .await
    .map_err(|_| io::Error::other("Polish browser trial exceeded 600 seconds"))?
}

async fn prepare(
    directory: &Path,
    tools: &drcs_bitmap_delivery::Tools,
) -> (Fixture, Flow, Vec<u8>, PathBuf) {
    let actual = std::fs::canonicalize(client_capture::selected(
        "ERSATZRS_QTM_POLISH_CALIBRATION_DIR",
    ))
    .unwrap();
    let (fixture, source) = drcs_bitmap_delivery::playback_fixture(tools, directory).await;
    ordinary_stream_flow::setting(
        &fixture,
        ersatzrs_domain::CONFIG_KEY_PAGES_IS_DARK_MODE,
        "False",
    )
    .await;
    http_routes::configure_authoring_policy(&fixture).await;
    let (fixture, flow) = Box::pin(setup_with_fixture_policy(
        polish_carrier::response,
        fixture,
        drcs_companion::dense_encoding_policy,
    ))
    .await;
    // Authenticate every fresh input and genuine saved output without importing.
    let request = polish_carrier::genuine_polish_request(&flow, &actual);
    assert_eq!(request.files.len(), flow.exported[0].files().len());
    (fixture, flow, source, actual)
}

async fn served_trial(
    fixture: &mut Fixture,
    flow: &Flow,
    retained: &Retained,
    actual: &Path,
    directory: &Path,
    origin: &live_client::LiveOrigin,
    media: (&drcs_bitmap_delivery::Tools, &[u8]),
) -> io::Result<(
    [AcceptedTeletextOutputCandidate; 2],
    TeletextDesignExportReceipt,
)> {
    let (accepted, receipt) =
        browser_publication(fixture, flow, retained, actual, directory).await?;
    ordinary_stream_flow::reset_source_start(fixture).await;
    let (day, night) = tokio::join!(
        drcs_bitmap_delivery::capture_tcp(origin.url(1)),
        drcs_bitmap_delivery::capture_tcp(origin.url(2))
    );
    let day =
        polish_delivery::verify_capture(directory, media.0, media.1, 1, &day, &accepted[0]).await;
    let night =
        polish_delivery::verify_capture(directory, media.0, media.1, 2, &night, &accepted[1]).await;
    retained
        .assert_unchanged(fixture, flow, &accepted, actual)
        .await;
    polish_delivery::write_json(
        directory,
        "stream-proof.json",
        &json!({"probe_version":1,
        "route":"/iptv/hls-direct/{channel}.ts","transport":"same ordinary TCP origin as real admin forms",
        "source_file":"source.ts","source_sha256":live_client::hash(media.1),"channels":[day,night],
        "native_browser_barriers_verified":true,"independent_decoding_proven":false,"actual_client_proven":false}),
    );
    Ok((accepted, receipt))
}

async fn assert_receipt(fixture: &Fixture, receipt: &TeletextDesignExportReceipt) {
    assert_eq!(
        fixture
            .state
            .application
            .teletext_design_export_receipt(
                receipt.metadata.id,
                policy().project.max_bytes,
                designs::budget().max_serialized_bytes,
            )
            .await
            .unwrap(),
        Some(receipt.clone())
    );
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
#[ignore = "requires retained actual QTM Polish edit, exact FFmpeg9/TSDuck and external headed localized SSR forms; no decoder/client claim"]
async fn actual_polish_browser_import_preview_publication_and_ordinary_streams() {
    let directory = PathBuf::from(client_capture::selected(
        "ERSATZRS_TELETEXT_POLISH_ADMIN_DIR",
    ));
    std::fs::create_dir(&directory).expect("Polish admin destination must be new");
    let tools = drcs_bitmap_delivery::Tools::load().await;
    let (mut fixture, flow, source, actual) = Box::pin(prepare(&directory, &tools)).await;
    let retained = Retained::capture(&fixture, &flow, &actual).await;
    let mut origin = live_client::LiveOrigin::start(&fixture).await.unwrap();
    let outcome = AssertUnwindSafe(async {
        polish_delivery::write_json(
            &directory,
            "browser-ready.json",
            &browser_ready(&origin, &flow, &actual),
        );
        Box::pin(served_trial(
            &mut fixture,
            &flow,
            &retained,
            &actual,
            &directory,
            &origin,
            (&tools, &source),
        ))
        .await
    })
    .catch_unwind()
    .await;
    let closed = origin.close().await;
    let cleaned = live_updates::live_tcp::wait_for_cleanup().await;
    let (accepted, receipt) = match outcome {
        Ok(Ok(value)) => value,
        Ok(Err(error)) => {
            fixture.state.pool.close().await;
            panic!("Polish admin trial failed: {error}");
        }
        Err(panic) => {
            fixture.state.pool.close().await;
            std::panic::resume_unwind(panic);
        }
    };
    closed.expect("owned Polish admin server must close");
    cleaned.expect("ordinary responses must release owned capacity");
    fixture = fixture.reopen().await;
    retained
        .assert_unchanged(&mut fixture, &flow, &accepted, &actual)
        .await;
    for (index, candidate) in accepted.iter().enumerate() {
        assert_eq!(
            fixture
                .state
                .application
                .teletext_publication_request(i64::try_from(index + 1).unwrap())
                .await
                .unwrap(),
            Some(candidate.generation().request.clone())
        );
    }
    assert_receipt(&fixture, &receipt).await;
    fixture.state.pool.close().await;
    polish_delivery::write_json(
        &directory,
        "success.json",
        &json!({"probe_version":1,
        "native_actual_browser_import_preview_request_publish_barriers_passed":true,
        "fresh_http_export_receipt_immutable_state_reopen_verified":true,
        "same_origin_ordinary_transport_source_av_complete_model_passed":true,
        "owned_server_closed":true,"capacity_released":true,
        "independent_decoding_proven":false,"actual_client_proven":false,"full_level25_proven":false,
        "limitations":"Inspect separately retained headed SSR/no-JS browser evidence; no hydration, independent decoder, suitable-client, warm or full-engine/platform acceptance claim"}),
    );
}
