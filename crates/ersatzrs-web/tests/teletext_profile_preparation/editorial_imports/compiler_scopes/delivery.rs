//! Reused genuine eight-save bytes under fresh native authority and ordinary TCP.
//! Full captured transport/model checks do not grant independent/client proof.

use super::super::super::{client_capture, ordinary_stream_flow};
use super::*;
use futures_util::FutureExt as _;
use std::panic::AssertUnwindSafe;

#[path = "delivery/policy.rs"]
mod runtime_policy;

fn fresh_sidecar(flow: &Flow) -> Vec<u8> {
    write_teletext_export_sidecar(flow.exported[0].sidecar(), policy().project).unwrap()
}

fn write_fresh_export(flow: &Flow, directory: &Path) -> Value {
    let original = directory.join("fresh-original-1");
    std::fs::create_dir(&original).unwrap();
    let mut files = Vec::new();
    for file in flow.exported[0].files() {
        live_client::write_new(&original.join(&file.name), &file.bytes).unwrap();
        files.push(json!({"name":file.name,"bytes":file.bytes.len(),"sha256":live_client::hash(&file.bytes)}));
    }
    let sidecar = fresh_sidecar(flow);
    live_client::write_new(&original.join("project.ersatzrs.json"), &sidecar).unwrap();
    json!({"directory":"fresh-original-1","files":files,
        "sidecar_file":"fresh-original-1/project.ersatzrs.json","sidecar_sha256":live_client::hash(&sidecar),
        "design_id":flow.saved[0].head().id,"revision":1,"original_digest":flow.saved[0].digest(),
        "receipt_id":flow.exported[0].metadata().id,"receipt_digest":flow.exported[0].metadata().digest})
}

fn assert_fresh_export(flow: &Flow, directory: &Path) {
    let original = directory.join("fresh-original-1");
    assert_eq!(std::fs::read_dir(&original).unwrap().count(), 11);
    for file in flow.exported[0].files() {
        assert_eq!(
            drcs_bitmap::read_retained_editor_file(&original.join(&file.name)),
            file.bytes
        );
    }
    assert_eq!(
        live_client::read_bounded(
            &original.join("project.ersatzrs.json"),
            policy().project.max_bytes
        )
        .unwrap(),
        fresh_sidecar(flow)
    );
}

fn preview_hash(image: &TeletextRaster) -> String {
    let pixels: Vec<u8> = image
        .pixels()
        .iter()
        .flat_map(|pixel| match pixel {
            TeletextRasterPixel::ReceiverHeader => [0, 0, 0, 0],
            TeletextRasterPixel::Colour(TeletextColourPixel::Rgb(rgb)) => {
                let [r, g, b] = rgb.get();
                [1, r, g, b]
            }
            TeletextRasterPixel::Colour(TeletextColourPixel::Video) => [2, 0, 0, 0],
            TeletextRasterPixel::Unresolved(_) => {
                panic!("all eight native previews must be fully resolved")
            }
        })
        .collect();
    live_client::hash(&pixels)
}

fn write_previews(
    directory: &Path,
    flow: &Flow,
    previews: &[TeletextRaster],
    accepted: &[AcceptedTeletextOutputCandidate; 2],
) {
    assert_eq!(oracle::previews(accepted[0].service()), previews);
    assert_eq!(
        oracle::previews(accepted[1].service()),
        oracle::previews(flow.accepted[1].service())
    );
    let pages: Vec<_>=(124..132).zip(previews).map(|(page,image)| json!({"page":page,"slot":slot(page),
        "scope":if page<128 {"normal"} else {"global"},"literal_code":code(page),"literal_mask":oracle::mask(page,true),
        "width":image.width(),"height":image.height(),"pixels_sha256":preview_hash(image),"concept_equals_published":true})).collect();
    polish_delivery::write_json(
        directory,
        "native-preview.json",
        &json!({"probe_version":1,
        "scope":"all eight complete shared concept/publication previews and fixed original/physical masks/PTUs; no client image claim",
        "day":pages,"both_generated_scopes_all_four_slots_verified":true,"night_original_unchanged":true,
        "night_generated_bank_count":0,"unrelated_authored_normal_bank_unchanged":true,
        "hash_encoding":"row-major four bytes per pixel: receiver=[0,0,0,0], RGB4=[1,r,g,b], video=[2,0,0,0]"}),
    );
}

async fn publish_retained(
    mut fixture: Fixture,
    flow: &Flow,
    retained: &resource_bank_admin::Retained,
    actual: &Path,
    directory: &Path,
) -> (Fixture, [AcceptedTeletextOutputCandidate; 2], Value) {
    let (request, reuse) = genuine::retained_request(flow, actual);
    retained.assert_unchanged(&fixture, flow).await;
    let concept = import(&fixture, request).await;
    assert_eq!(concept.head().id, flow.saved[0].head().id);
    assert_eq!(concept.head().revision, 2);
    assert_original_pointers(&fixture, flow).await;
    retained.assert_unchanged(&fixture, flow).await;
    let previews = Box::pin(assert_restored(&fixture, flow, &concept)).await;
    fixture = fixture.reopen().await;
    assert_original_pointers(&fixture, flow).await;
    retained.assert_unchanged(&fixture, flow).await;
    assert_eq!(
        Box::pin(assert_restored(&fixture, flow, &concept)).await,
        previews
    );
    let day = publish_and_accept(&fixture, flow, &concept).await;
    assert_concept(flow, day.service());
    let accepted = [day, recover(&fixture, 2).await];
    assert_eq!(accepted[0].selection(), flow.accepted[0].selection());
    compiler_polish::assert_pointers(&fixture, &accepted[0], &accepted[1]).await;
    write_previews(directory, flow, &previews, &accepted);
    retained.assert_unchanged(&fixture, flow).await;
    (fixture, accepted, reuse)
}

async fn assert_state(
    fixture: &Fixture,
    flow: &Flow,
    retained: &resource_bank_admin::Retained,
    accepted: &[AcceptedTeletextOutputCandidate; 2],
    inputs: (&Path, &[(String, String)]),
    media: (&Path, &[u8]),
) {
    retained.assert_unchanged(fixture, flow).await;
    compiler_polish::assert_pointers(fixture, &accepted[0], &accepted[1]).await;
    assert_concept(flow, recover(fixture, 1).await.service());
    assert_eq!(
        oracle::previews(recover(fixture, 2).await.service()),
        oracle::previews(flow.accepted[1].service())
    );
    assert_eq!(genuine::retained_input_hashes(flow, inputs.0), inputs.1);
    assert_fresh_export(flow, media.0);
    compiler_polish_delivery::assert_source(fixture, media.0, media.1);
}

async fn capture_channels(
    directory: &Path,
    origin: &live_client::LiveOrigin,
    tools: &drcs_bitmap_delivery::Tools,
    source: &[u8],
    accepted: &[AcceptedTeletextOutputCandidate; 2],
) -> Vec<Value> {
    let (day, night) = tokio::join!(
        drcs_bitmap_delivery::capture_tcp(origin.url(1)),
        drcs_bitmap_delivery::capture_tcp(origin.url(2))
    );
    let day =
        polish_delivery::verify_capture(directory, tools, source, 1, &day, &accepted[0]).await;
    let night =
        polish_delivery::verify_capture(directory, tools, source, 2, &night, &accepted[1]).await;
    vec![day, night]
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
#[ignore = "requires retained actual eight-page QTM saves, exact FFmpeg9/ffprobe/TSDuck and a new output child; uses fresh authority and real ordinary TCP; independent/client proof remains separate"]
async fn actual_qtm_eight_page_dual_scope_concept_serves_two_ordinary_channels() {
    let actual = PathBuf::from(client_capture::selected("ERSATZRS_QTM_DUAL_SCOPE_DIR"));
    let directory = PathBuf::from(client_capture::selected(
        "ERSATZRS_TELETEXT_DUAL_SCOPE_DELIVERY_DIR",
    ));
    std::fs::create_dir(&directory).expect("dual-scope ordinary output must be new");
    live_updates::live_tcp::terminal_trace(&directory).unwrap();
    let tools = drcs_bitmap_delivery::Tools::load().await;
    let (fixture, source) = drcs_bitmap_delivery::playback_fixture(&tools, &directory).await;
    runtime_policy::configure(&fixture).await;
    let (fixture, flow) = Box::pin(setup_with_fixture_policy_and_writer(
        scope_fixture::response,
        fixture,
        scope_fixture::encoding_policy,
        authoring_metadata::write_compact,
    ))
    .await;
    oracle::assert_originals(&flow);
    let retained = resource_bank_admin::Retained::capture(&fixture).await;
    let hashes = genuine::retained_input_hashes(&flow, &actual);
    let fresh = write_fresh_export(&flow, &directory);
    let (mut fixture, accepted, reuse) = Box::pin(publish_retained(
        fixture, &flow, &retained, &actual, &directory,
    ))
    .await;
    Box::pin(assert_state(
        &fixture,
        &flow,
        &retained,
        &accepted,
        (&actual, &hashes),
        (&directory, &source),
    ))
    .await;
    Box::pin(runtime_policy::preflight(&fixture, &accepted, &directory)).await;
    ordinary_stream_flow::reset_source_start(&fixture).await;
    let mut origin = live_client::LiveOrigin::start(&fixture).await.unwrap();
    let outcome=AssertUnwindSafe(async {
        let channels=capture_channels(&directory,&origin,&tools,&source,&accepted).await;
        Box::pin(assert_state(&fixture,&flow,&retained,&accepted,(&actual,&hashes),(&directory,&source))).await;
        polish_delivery::write_json(&directory,"stream-proof.json",&json!({"probe_version":1,
            "route":"/iptv/hls-direct/{channel}.ts","transport":"actual owned application loopback TCP host",
            "scope":"cold ordinary delivery of eight retained genuine QTM saves under fresh receipt/project authority; no new GUI trial",
            "source_file":"source.ts","source_sha256":live_client::hash(&source),"source_bytes":source.len(),
            "channels":channels,"fresh_export":fresh,"reuse_provenance":reuse,"retained_input_hashes":hashes,
            "display_pages":[124,125,126,127,128,129,130,131],"subcode":32,"generated_scopes":["normal","global"],"fixed_slots":[0,1,2,3],
            "night_generated_banks":0,"both_channels_keep_authored_normal_2a2_s1_3_slot5":true,
            "complete_native_preview_model_ptu_bindings_verified":true,"independent_decoding_proven":false,"actual_client_proven":false,"full_level25_proven":false,
            "units_scope":"exported chronological first at most512 units may end inside acquisition; full retained TS used for every transport/AV/model assertion"}));
    }).catch_unwind().await;
    let closed = origin.close().await;
    let cleaned = live_updates::live_tcp::wait_for_cleanup().await;
    if let Err(panic) = outcome {
        fixture.state.pool.close().await;
        std::panic::resume_unwind(panic);
    }
    closed.expect("owned dual-scope origin must close");
    cleaned.expect("both ordinary producers must release their capacity");
    fixture = fixture.reopen().await;
    Box::pin(assert_state(
        &fixture,
        &flow,
        &retained,
        &accepted,
        (&actual, &hashes),
        (&directory, &source),
    ))
    .await;
    assert!(fixture.playout.try_recv().is_err());
    fixture.state.pool.close().await;
    polish_delivery::write_json(
        &directory,
        "success.json",
        &json!({"probe_version":1,
        "retained_genuine_eight_page_native_import_preview_explicit_publication_and_ordinary_tcp_passed":true,
        "source_av_tsduck_complete_emitted_model_and_state_reopen_passed":true,"fresh_authority_only_uploaded":true,
        "retained_editor_inputs_and_fresh_export_unchanged":true,"owned_origin_closed":true,"capacity_released":true,
        "new_gui_operations_claimed":false,"independent_decoding_proven":false,"actual_client_proven":false,"full_level25_proven":false,
        "limitations":"Cold ordinary native transport/model proof. Exported512-unit prefix is limited; independent acquisition/decoding, suitable clients, headed admin/hydration, warm, panels, full-engine/platform acceptance remain separate."}),
    );
}
