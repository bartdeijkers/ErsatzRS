//! Opt-in genuine browser transfer/publication to the same ordinary TCP host.
//! Marker files synchronize owned clients; durable/native/transport assertions
//! and separately inspected screenshots supply the actual evidence.

use super::super::{client_capture, ordinary_stream_flow};
use super::*;
use ersatzrs_app::{
    teletext_design_documents::decode_teletext_design_documents,
    teletext_export_project::parse_teletext_export_receipt_body,
};
use ersatzrs_domain::{ChannelTeletextBindingRead, TeletextProfileRead};
use ersatzrs_ffmpeg::teletext::EncodedTeletextAuthoredService;
use futures_util::FutureExt as _;
use serde::{de::DeserializeOwned, Deserialize, Serialize};
use serde_json::{json, Value};
use std::{io, panic::AssertUnwindSafe, path::PathBuf};

const ORIGINAL_HASH: &str = "d1e9a9d49a28be14adb1e727c11ab0cfe908bbd0df83ca49c48ef4e170a0a870";
const EDITED_HASH: &str = "6b4ac2e70bab6d73b6bdbb52bf9590887c7094582c14bbbca87433225aaebda6";
const SPARSE_ORIGINAL_HASH: &str =
    "0e563bc16e0a5d3e339f0a035b2d802d3463e873cbba2a1c7d4d99f302a0afc7";
const SPARSE_EDITED_HASH: &str = "65e4580a5cd766648cd9499ba1bc2f034c95e62c9243a16edcf0311a2dacc0d9";
const BROWSER_TIMEOUT: Duration = Duration::from_mins(10);

#[derive(Clone, Copy, PartialEq, Eq)]
enum BankCase {
    Dense,
    Sparse,
    ExplicitPage,
}

impl BankCase {
    fn sparse(self) -> bool {
        self == Self::Sparse
    }

    fn assert_presentation(self, service: &EncodedTeletextAuthoredService) {
        if matches!(self, Self::ExplicitPage) {
            drcs_bank::assert_explicit_page_presentation(service);
        }
    }
}

fn actual_hashes(sparse: bool) -> (&'static str, &'static str) {
    if sparse {
        (SPARSE_ORIGINAL_HASH, SPARSE_EDITED_HASH)
    } else {
        (ORIGINAL_HASH, EDITED_HASH)
    }
}

#[derive(Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub(super) struct BrowserConcept {
    pub(super) probe_version: u8,
    pub(super) design_id: i64,
    pub(super) revision: i64,
    pub(super) digest: String,
}

#[derive(Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub(super) struct BrowserComplete {
    pub(super) probe_version: u8,
    pub(super) design_id: i64,
    pub(super) revision: i64,
    pub(super) digest: String,
    pub(super) generation_id: i64,
    pub(super) request_id: i64,
}

struct Retained {
    package: String,
    profiles: Vec<TeletextProfileRead>,
    bindings: [Vec<ChannelTeletextBindingRead>; 2],
}

impl Retained {
    async fn capture(fixture: &Fixture) -> Self {
        Self {
            package: calculate_addon_content_sha256(&fixture.root.path().join("package"))
                .await
                .unwrap(),
            profiles: fixture
                .state
                .application
                .list_teletext_profiles(ADDON)
                .await
                .unwrap(),
            bindings: [
                fixture
                    .state
                    .application
                    .channel_teletext_bindings(1)
                    .await
                    .unwrap(),
                fixture
                    .state
                    .application
                    .channel_teletext_bindings(2)
                    .await
                    .unwrap(),
            ],
        }
    }

    async fn assert_unchanged(&self, fixture: &Fixture, flow: &Flow) {
        assert_retained_storage(fixture, flow).await;
        assert_eq!(
            calculate_addon_content_sha256(&fixture.root.path().join("package"))
                .await
                .unwrap(),
            self.package
        );
        assert_eq!(
            fixture
                .state
                .application
                .list_teletext_profiles(ADDON)
                .await
                .unwrap(),
            self.profiles
        );
        for (index, expected) in self.bindings.iter().enumerate() {
            assert_eq!(
                fixture
                    .state
                    .application
                    .channel_teletext_bindings(i64::try_from(index + 1).unwrap())
                    .await
                    .unwrap(),
                *expected
            );
        }
        generations::assert_same_output(&recover(fixture, 2).await, &flow.accepted[1]);
    }
}

fn write_json(directory: &Path, filename: &str, value: &impl Serialize) -> io::Result<()> {
    live_client::write_new(
        &directory.join(filename),
        &serde_json::to_vec_pretty(value).unwrap(),
    )
}

pub(super) async fn marker<T: DeserializeOwned>(directory: &Path, filename: &str) -> io::Result<T> {
    loop {
        let path = directory.join(filename);
        if tokio::fs::try_exists(&path).await? {
            return serde_json::from_slice(&live_client::read_bounded(&path, 4096)?)
                .map_err(|_| io::Error::other("invalid closed browser marker"));
        }
        tokio::time::sleep(Duration::from_millis(100)).await;
    }
}

fn browser_ready(
    origin: &live_client::LiveOrigin,
    flow: &Flow,
    actual: &Path,
    sparse: bool,
) -> Value {
    let (original_hash, edited_hash) = actual_hashes(sparse);
    let channels: Vec<_> = flow.accepted.iter().enumerate().map(|(index, candidate)| {
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
    json!({"probe_version":1,"base_url":origin.base_url(),"channels":channels,
        "bank":{"name":drcs_companion::indexed_companion(flow,0).0.name,"sparse":sparse,
            "original_path":actual.join("native-input.tti"),"edited_path":actual.join("edited.tti"),
            "original_sha256":original_hash,"edited_sha256":edited_hash},
        "completion_file":"browser-complete.json","timeout_seconds":600,
        "markers":{"concept":"browser-concept-ready.json","publication":"publication-ready.json","complete":"browser-complete.json"}})
}

fn authenticate_actual(flow: &Flow, actual: &Path, sparse: bool) {
    let (original_hash, edited_hash) = actual_hashes(sparse);
    let source = drcs_bitmap::read_retained_editor_file(&actual.join("native-input.tti"));
    let edited = drcs_bitmap::read_retained_editor_file(&actual.join("edited.tti"));
    assert_eq!(drcs_bitmap::hash(&source), original_hash);
    assert_eq!(drcs_bitmap::hash(&edited), edited_hash);
    let bank = &drcs_companion::indexed_companion(flow, 0).0.name;
    assert_eq!(
        flow.exported[0]
            .files()
            .iter()
            .find(|file| &file.name == bank)
            .unwrap()
            .bytes,
        source
    );
}

async fn verified_concept(
    fixture: &Fixture,
    flow: &Flow,
    marker: &BrowserConcept,
    case: BankCase,
) -> TeletextDesignExportReceipt {
    let sparse = case.sparse();
    assert_eq!(marker.probe_version, 1);
    assert_eq!(marker.design_id, flow.saved[0].head().id);
    assert_eq!(marker.revision, 2);
    let revision = fixture
        .state
        .application
        .teletext_design_revision(
            marker.design_id,
            marker.revision,
            designs::budget().max_serialized_bytes,
        )
        .await
        .unwrap()
        .unwrap();
    assert_eq!(revision.digest, marker.digest);
    let documents =
        decode_teletext_design_documents(&revision.document, designs::budget()).unwrap();
    let overlay = documents.editorial_overlay().unwrap();
    assert_eq!(overlay.drcs_bitmap_overlays().len(), 1);
    assert!(overlay.pages().is_empty());
    assert!(overlay.local_program_operands().is_empty());
    let bitmap = overlay.drcs_bitmap_overlays()[0];
    assert_eq!(
        bitmap.identity().kind(),
        ersatzrs_domain::TeletextResourceKind::Drcs
    );
    assert_eq!(bitmap.identity().page().magazine(), 1);
    assert_eq!(bitmap.identity().page().page(), 0xc0);
    assert_eq!(bitmap.identity().logical_subpage().get(), u8::from(sparse));
    assert_eq!(bitmap.slot().get(), 0);
    assert_eq!(bitmap.expected().get()[0], 0x801);
    assert_eq!(bitmap.replacement().get()[0], 0x803);
    let provenance = overlay.provenance();
    assert_eq!(provenance.original_design_id, marker.design_id);
    assert_eq!(provenance.original_revision, 1);
    assert_eq!(provenance.original_digest, flow.originals[0].digest);
    assert!(!flow
        .receipts
        .iter()
        .any(|receipt| receipt.metadata.id == provenance.export_receipt_id));
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
    assert_eq!(receipt.metadata.design_id, marker.design_id);
    assert_eq!(receipt.metadata.revision, 1);
    assert_eq!(receipt.metadata.design_digest, flow.originals[0].digest);
    let body = parse_teletext_export_receipt_body(&receipt.receipt, policy().project).unwrap();
    assert_eq!(body.project, flow.exported[0].sidecar().project);
    assert_eq!(body.files, flow.exported[0].files());
    let mut value: Value = serde_json::from_slice(&revision.document).unwrap();
    value.as_object_mut().unwrap().remove("editorial_overlay");
    assert_eq!(
        value,
        serde_json::from_slice::<Value>(&flow.originals[0].document).unwrap()
    );
    let restored = reload_original_teletext_design_revision(
        &fixture.state.application,
        marker.design_id,
        marker.revision,
        designs::budget(),
        channel_documents::policy(),
        designs::deadline(),
    )
    .await
    .unwrap()
    .unwrap();
    drcs_bank::assert_native_edit(
        flow.accepted[0].service(),
        restored.encoded().unwrap(),
        sparse,
    );
    case.assert_presentation(restored.encoded().unwrap());
    assert_original_pointers(fixture, flow).await;
    receipt
}

async fn browser_publication(
    fixture: &Fixture,
    flow: &Flow,
    retained: &Retained,
    directory: &Path,
    case: BankCase,
) -> io::Result<(
    [AcceptedTeletextOutputCandidate; 2],
    TeletextDesignExportReceipt,
)> {
    tokio::time::timeout(BROWSER_TIMEOUT, async {
        let concept: BrowserConcept = marker(directory, "browser-concept-ready.json").await?;
        let receipt = verified_concept(fixture, flow, &concept, case).await;
        retained.assert_unchanged(fixture, flow).await;
        write_json(directory, "publication-ready.json", &concept)?;
        let complete: BrowserComplete = marker(directory, "browser-complete.json").await?;
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
        assert_eq!(accepted[0].selection(), flow.accepted[0].selection());
        assert_eq!(
            fixture
                .state
                .application
                .teletext_publication_request(1)
                .await
                .unwrap(),
            Some(day.request.clone())
        );
        drcs_bank::assert_native_edit(
            flow.accepted[0].service(),
            accepted[0].service(),
            case.sparse(),
        );
        for candidate in &accepted {
            case.assert_presentation(candidate.service());
        }
        retained.assert_unchanged(fixture, flow).await;
        Ok((accepted, receipt))
    })
    .await
    .map_err(|_| io::Error::other("browser resource-bank trial exceeded 600 seconds"))?
}

fn client_ready(
    origin: &live_client::LiveOrigin,
    accepted: &[AcceptedTeletextOutputCandidate; 2],
    source: &[u8],
    case: BankCase,
) -> Value {
    let sparse = case.sparse();
    let channels: Vec<_> = accepted.iter().enumerate().map(|(index,candidate)| {
        let channel = index+1;
        let binding = &candidate.selection().bindings[0];
        json!({"channel_id":channel,"theme":if index==0 {"day"} else {"night"},
            "url":origin.url(i64::try_from(channel).unwrap()),"addon_id":binding.addon_id,
            "profile_id":binding.profile_id,"profile_revision":binding.profile_revision,
            "generation_id":candidate.generation().id,"design_revision":candidate.generation().request.revision,
            "expected_pages":[124,125],"subcode":32,"expected_navigation":[124,125,124],
            "edited_q_page":null,
            "drcs_mode":if sparse {"monochrome_sparse_s1_1_x27"} else {"monochrome_dense_s1_0_1"},
            "bitmap_first_row_by_page":{"124":if index==0 {0x803} else {0x801},
                "125":if sparse && index==0 {0x803} else {0x801}}})
    }).collect();
    json!({"probe_version":1,"scope":"Actual browser resource-bank import and publication on these same ordinary URLs",
    "channels":channels,"source":{"file":"source.ts","sha256":live_client::hash(source),"bytes":source.len(),"duration_seconds":60},
    "active_transcode_limit":4,"completion_file":"client-complete.json","completion_timeout_seconds":180,
    "expected_completion":{"probe_version":1,"client_name":"VLC","client_version":"3.0.20","decoder_version":"0.2.42","proof_suffix":"live-ordinary"},
    "client_actions":if sparse {
        "Open both ordinary URLs with the qualified test-corrected decoder; navigate124 to125 to124. Main columns8/10 use mirrored bank1 on both pages. Both day pages gain bitmap pixel x10 row0; both night pages remain original. Inspect both themes and close owned clients before completion."
    } else {
        "Open both ordinary URLs; navigate124 to125 to124. Main columns8/10 use bank0 on124 and bank1 on125. Only day124 gains bitmap pixel x10 row0; inspect both themes and close owned clients before completion."
    },
    "limitations":match case {
        BankCase::Sparse => "Cold sparse-bank MAIN client proof with a test-corrected decoder only; panels, warm transitions, full Level2.5 and platform acceptance remain open.",
        BankCase::Dense => "Cold dense-bank client proof only; sparse decoding, warm transitions, full Level2.5 and platform acceptance remain open.",
        BankCase::ExplicitPage => "Cold dense-bank explicit page-X28/no-M29 client proof only; source reset prevents a warm handover claim. Acknowledged M29-to-page-X28 ordinary handover, full Level2.5 and platform acceptance remain open."
    }})
}

async fn served_trial(
    fixture: &Fixture,
    flow: &Flow,
    retained: &Retained,
    origin: &live_client::LiveOrigin,
    directory: &Path,
    media: (&drcs_bitmap_delivery::Tools, &[u8]),
    case: BankCase,
) -> io::Result<(
    [AcceptedTeletextOutputCandidate; 2],
    TeletextDesignExportReceipt,
    Value,
)> {
    let (tools, source) = media;
    let (accepted, receipt) = browser_publication(fixture, flow, retained, directory, case).await?;
    ordinary_stream_flow::reset_source_start(fixture).await;
    let (day, night) = tokio::join!(
        drcs_bitmap_delivery::capture_tcp(origin.url(1)),
        drcs_bitmap_delivery::capture_tcp(origin.url(2))
    );
    let mut channels = Vec::with_capacity(2);
    for (index, bytes) in [&day, &night].into_iter().enumerate() {
        let channel = i64::try_from(index + 1).unwrap();
        let report = if case.sparse() {
            drcs_bitmap_delivery::verify_sparse_edited_capture(
                directory,
                tools,
                source,
                channel,
                bytes,
                &accepted[index],
            )
            .await
        } else {
            drcs_bitmap_delivery::verify_capture_mode(
                directory,
                tools,
                source,
                channel,
                bytes,
                &accepted[index],
                "indexed-edited",
            )
            .await
        };
        channels.push(report);
    }
    write_json(
        directory,
        "stream-proof.json",
        &json!({"probe_version":1,"route":"/iptv/hls-direct/{channel}.ts",
        "transport":"same application loopback TCP host as admin","channels":channels,"source_sha256":live_client::hash(source)}),
    )?;
    ordinary_stream_flow::reset_source_start(fixture).await;
    write_json(
        directory,
        "ready.json",
        &client_ready(origin, &accepted, source, case),
    )?;
    let completion = live_client::wait_for_client(directory).await?;
    retained.assert_unchanged(fixture, flow).await;
    Ok((accepted, receipt, serde_json::to_value(completion).unwrap()))
}

/// Set `ERSATZRS_TELETEXT_RESOURCE_BANK_DIR` to a new proof directory and
/// `ERSATZRS_QTM_DRCS_BANK_CALIBRATION_DIR` to retained actual-2. Operate the real
/// headed admin and pinned client against the published readiness manifests.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
#[ignore = "requires retained actual QTM dense bank, exact FFmpeg9/TSDuck/libzvbi, headed browser and VLC on owned ordinary URLs"]
async fn actual_browser_bank_import_preview_publication_and_ordinary_clients() {
    Box::pin(run_trial(BankCase::Dense)).await;
}

/// Reuses retained actual-2 and the same headed forms/ordinary URLs with native
/// explicit page X28 and no M29. Source reset makes this a cold client proof;
/// acknowledged warm M29-to-page-X28 ordinary handover remains separate.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
#[ignore = "requires retained actual QTM dense bank, exact FFmpeg9/TSDuck/libzvbi, headed browser and VLC on owned ordinary URLs"]
async fn actual_explicit_page_browser_bank_import_preview_publication_and_ordinary_clients() {
    Box::pin(run_trial(BankCase::ExplicitPage)).await;
}

/// Set `ERSATZRS_QTM_DRCS_SPARSE_CALIBRATION_DIR` to retained actual-padded-2.
/// The same real admin forms publish S1=1; the separately qualified corrected
/// decoder must consume these ordinary URLs without changing their packets.
// Intentional Linux-x64 diagnostic: candidate qualification uses /proc and a
// receipted shared library. Other supported platforms retain separate acceptance.
#[cfg(all(target_os = "linux", target_arch = "x86_64"))]
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
#[ignore = "requires retained actual padded QTM sparse bank, exact FFmpeg9/TSDuck, qualified test decoder, headed browser and VLC on owned ordinary URLs"]
async fn actual_sparse_browser_bank_import_preview_publication_and_ordinary_clients() {
    Box::pin(run_trial(BankCase::Sparse)).await;
}

async fn prepare_trial(
    case: BankCase,
    tools: &drcs_bitmap_delivery::Tools,
    directory: &Path,
) -> (Fixture, Flow, Vec<u8>, PathBuf) {
    let sparse = case.sparse();
    let actual = std::fs::canonicalize(client_capture::selected(if sparse {
        "ERSATZRS_QTM_DRCS_SPARSE_CALIBRATION_DIR"
    } else {
        "ERSATZRS_QTM_DRCS_BANK_CALIBRATION_DIR"
    }))
    .unwrap();
    let (fixture, source) = drcs_bitmap_delivery::playback_fixture(tools, directory).await;
    // Match the existing admin setting form's canonical boolean serialization
    // before capturing the baseline restored by the browser's locale trials.
    ordinary_stream_flow::setting(
        &fixture,
        ersatzrs_domain::CONFIG_KEY_PAGES_IS_DARK_MODE,
        "False",
    )
    .await;
    http_routes::configure_authoring_policy(&fixture).await;
    let (fixture, flow) = if sparse {
        Box::pin(setup_with_fixture_policy(
            drcs_bank::sparse_response,
            fixture,
            drcs_companion::sparse_encoding_policy,
        ))
        .await
    } else if matches!(case, BankCase::ExplicitPage) {
        Box::pin(setup_with_fixture_policy(
            drcs_bank::explicit_page_response,
            fixture,
            drcs_companion::dense_encoding_policy,
        ))
        .await
    } else {
        Box::pin(setup_with_fixture_policy(
            drcs_bank::dense_response,
            fixture,
            drcs_companion::dense_encoding_policy,
        ))
        .await
    };
    authenticate_actual(&flow, &actual, sparse);
    for candidate in &flow.accepted {
        case.assert_presentation(candidate.service());
    }
    (fixture, flow, source, actual)
}

async fn run_trial(case: BankCase) {
    let sparse = case.sparse();
    let directory = PathBuf::from(client_capture::selected(
        "ERSATZRS_TELETEXT_RESOURCE_BANK_DIR",
    ));
    std::fs::create_dir(&directory).expect("resource-bank destination must be new");
    let tools = drcs_bitmap_delivery::Tools::load().await;
    let (fixture, flow, source, actual) = Box::pin(prepare_trial(case, &tools, &directory)).await;
    let retained = Retained::capture(&fixture).await;
    let mut origin = live_client::LiveOrigin::start(&fixture).await.unwrap();
    let outcome = AssertUnwindSafe(async {
        write_json(
            &directory,
            "browser-ready.json",
            &browser_ready(&origin, &flow, &actual, sparse),
        )?;
        Box::pin(served_trial(
            &fixture,
            &flow,
            &retained,
            &origin,
            &directory,
            (&tools, &source),
            case,
        ))
        .await
    })
    .catch_unwind()
    .await;
    let closed = origin.close().await;
    let (accepted, receipt, completion) = match outcome {
        Ok(result) => match result {
            Ok(result) => result,
            Err(error) => {
                fixture.state.pool.close().await;
                panic!("resource-bank trial failed: {error}");
            }
        },
        Err(panic) => {
            fixture.state.pool.close().await;
            std::panic::resume_unwind(panic);
        }
    };
    closed.expect("owned resource-bank server failed to close");
    let fixture = fixture.reopen().await;
    for (index, expected) in accepted.iter().enumerate() {
        let recovered = recover(&fixture, i64::try_from(index + 1).unwrap()).await;
        generations::assert_same_output(&recovered, expected);
        case.assert_presentation(recovered.service());
    }
    retained.assert_unchanged(&fixture, &flow).await;
    assert_eq!(
        fixture
            .state
            .application
            .teletext_design_export_receipt(
                receipt.metadata.id,
                policy().project.max_bytes,
                designs::budget().max_serialized_bytes
            )
            .await
            .unwrap(),
        Some(receipt)
    );
    fixture.state.pool.close().await;
    write_json(&directory,"success.json",&json!({"probe_version":1,"completion":completion,
        "genuine_browser_export_import_preview_request_publish":true,
        "fresh_http_receipt_and_exact_bitmap_concept_verified_before_publication":true,
        "same_host_ordinary_transport_source_av_independent_decoding_passed":true,
        "channel_profile_package_original_receipt_config_isolation_and_reopen_verified":true,
        "owned_server_closed":true,"limitations":match case {
            BankCase::Sparse => "Attach inspected browser/client images. Sparse cold MAIN flow with a test-corrected decoder only; panels, warm transitions and complete engine/platform acceptance remain open.",
            BankCase::Dense => "Attach inspected browser/client images. Dense cold flow only; sparse decoding and complete engine/platform acceptance remain open.",
            BankCase::ExplicitPage => "Attach inspected browser/client images. Dense explicit page-X28/no-M29 cold flow only; source reset prevents a warm handover claim. Acknowledged M29-to-page-X28 ordinary handover and complete engine/platform acceptance remain open."
        }})).unwrap();
}
