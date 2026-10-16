//! Retained actual VLC decoder/navigation over the original ordinary TCP URLs.
//! Root image review is a separate gate after all paced producers have stopped.

use super::*;
use serde::Deserialize;
use std::io;
use tokio::time::{timeout_at, Instant};

const DIRECTORY_ENV: &str = "ERSATZRS_TELETEXT_DRCS_WARM_HEADED_DIR";
const LIVE_SECONDS: u64 = 55;
const REVIEW_SECONDS: u64 = 180;
const NAVIGATION: [Option<u16>; 3] = [Some(124), Some(125), Some(124)];

fn channel(candidate: &AcceptedTeletextOutputCandidate, theme: &str, url: &str) -> Value {
    let generation = candidate.generation();
    let binding = &candidate.selection().bindings[0];
    json!({"channel_id":binding.channel_id,"theme":theme,"url":url,
        "addon_id":binding.addon_id,"profile_id":binding.profile_id,"profile_revision":binding.profile_revision,
        "theme_rgb4":if theme=="day" {[15,0,0]} else {[0,0,15]},
        "design_id":generation.request.design_id,"design_revision":generation.request.revision,
        "design_digest":generation.request.digest,"generation_id":generation.id,"request_id":generation.request.id})
}

fn ready(origin: &live_tcp::Origin, flow: &Flow, source: &[u8]) -> Value {
    json!({"probe_version":3,"protocol":"warm-drcs-live-ordinary-v1",
        "completion_timeout_seconds":LIVE_SECONDS,"source":{"file":"source.ts","sha256":live_tcp::hash(source),
            "bytes":source.len(),"duration_seconds":60},
        "channels":[channel(&flow.accepted[0],"day",&origin.url(1,"day")),
            channel(&flow.accepted[1],"night",&origin.url(2,"night"))],
        "editor_original":{"design_id":flow.originals[0].design_id,"revision":flow.originals[0].revision,
            "digest":flow.originals[0].digest,"receipt_id":flow.receipts[0].metadata.id,
            "receipt_digest":flow.receipts[0].metadata.digest},
        "resource":{"kind":"drcs","magazine":1,"page":192,"logical_subpage":0,"slot":0,
            "original_first_bitmap_row":2049,"edited_first_bitmap_row":2051},
        "expected_pages":[124,125],"subcode":32,"expected_navigation":[124,125,124],
        "stages":["original_bitmap","edited_bitmap"],
        "stage_files":"stage-N-ready.json and stage-N-captured.json, N from 1 through 2",
        "client_stop_file":"clients-stopped.json",
        "limitations":"MAIN DRCS pixel and actual page navigation only; no side-panel, numeric-entry, full Level2.5, EOF or platform acceptance. Stage markers certify no rendering."})
}

struct Protocol<'a> {
    directory: &'a Path,
    deadline: Instant,
    ready_sha256: String,
    clients: Option<[u32; 2]>,
    stages: Vec<Value>,
    stage_hashes: Vec<String>,
}
impl Protocol<'_> {
    async fn stage(
        &mut self,
        number: u8,
        current: &AcceptedTeletextOutputCandidate,
        flow: &Flow,
        origin: &live_tcp::Origin,
    ) -> io::Result<()> {
        let value = json!({"probe_version":3,"stage":number,
            "state":if number==1 {"original_bitmap"} else {"edited_bitmap"},
            "scope":"Exact native wire cycle observed; actual headed image inspection remains pending",
            "initial_ready_sha256":self.ready_sha256,
            "day":channel(current,"day",&origin.url(1,"day")),
            "night":channel(&flow.accepted[1],"night",&origin.url(2,"night")),
            "day_first_bitmap_row":if number==1 {2049} else {2051},"night_first_bitmap_row":2049,
            "day_s2":if number==1 {1} else {2},"night_s2":1,
            "expected_pages":[124,125],"subcode":32,"expected_navigation":[124,125,124],
            "wire_offsets":origin.captures().each_ref().map(Capture::position)});
        let bytes = serde_json::to_vec_pretty(&value).unwrap();
        let hash = live_tcp::hash(&bytes);
        live_tcp::write_new(
            &self.directory.join(format!("stage-{number}-ready.json")),
            &bytes,
        )?;
        let raw = live_tcp::wait_file(
            self.directory,
            &format!("stage-{number}-captured.json"),
            self.deadline,
        )
        .await?;
        let marker: live_tcp::StageCaptured = serde_json::from_slice(&raw)
            .map_err(|_| io::Error::other("invalid closed DRCS stage acknowledgement"))?;
        self.clients = Some(live_tcp::stage_client_identities(
            &marker,
            3,
            number,
            &hash,
            self.clients,
        )?);
        live_tcp::validate_images_for(&marker, [&NAVIGATION, &NAVIGATION])?;
        self.stage_hashes.push(hash.clone());
        self.stages.push(json!({"stage":number,"ready_sha256":hash,"capture":marker,"visual_match_certified":false}));
        Ok(())
    }
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct StoppedClients {
    probe_version: u8,
    ready_sha256: String,
    clients: [StoppedClient; 2],
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct StoppedClient {
    viewer: String,
    pid: u32,
    exit_code: i32,
    decoder_identity: String,
    decoder_starts: u8,
    decoder_stops_before_shutdown: u8,
}
async fn stopped_clients(protocol: &Protocol<'_>) -> io::Result<Value> {
    let raw = live_tcp::wait_file(
        protocol.directory,
        "clients-stopped.json",
        protocol.deadline,
    )
    .await?;
    let stopped: StoppedClients = serde_json::from_slice(&raw)
        .map_err(|_| io::Error::other("invalid closed DRCS owned-client stop receipt"))?;
    if stopped.probe_version != 3 || stopped.ready_sha256 != protocol.ready_sha256 {
        return Err(io::Error::other(
            "DRCS stop receipt changed native readiness",
        ));
    }
    let identities = protocol
        .clients
        .ok_or_else(|| io::Error::other("DRCS client stages were not acknowledged"))?;
    for (index, client) in stopped.clients.iter().enumerate() {
        if client.viewer != ["day", "night"][index]
            || client.pid != identities[index]
            || !matches!(client.exit_code, 0 | -15)
            || client.decoder_starts != 1
            || client.decoder_stops_before_shutdown != 0
            || client.decoder_identity.is_empty()
            || client.decoder_identity.len() > 32
            || !client
                .decoder_identity
                .bytes()
                .all(|b| b.is_ascii_hexdigit())
        {
            return Err(io::Error::other(
                "DRCS client/decoder was replaced or did not stop cleanly",
            ));
        }
    }
    serde_json::from_slice(&raw).map_err(|_| io::Error::other("invalid DRCS stop receipt"))
}

async fn transitions(
    fixture: &Fixture,
    flow: &Flow,
    origin: &live_tcp::Origin,
    actual: &Path,
    protocol: &mut Protocol<'_>,
) -> io::Result<AcceptedTeletextOutputCandidate> {
    live_tcp::wait_attached(origin, protocol.deadline).await?;
    let before = original_cycles(origin, flow, [0; 3]).await;
    for (index, observed) in before.iter().enumerate() {
        write_units(
            protocol.directory,
            &format!("viewer-{index}-original-units.json"),
            observed,
        );
    }
    protocol.stage(1, &flow.accepted[0], flow, origin).await?;
    let (_concept, pending) = pending_actual_edit(fixture, flow, actual).await;
    let held = original_cycles(
        origin,
        flow,
        origin.captures().each_ref().map(Capture::position),
    )
    .await;
    let offsets = origin.captures().each_ref().map(Capture::position);
    let edited = generations::accepted(
        accept_original_teletext_generation(
            &fixture.state.application,
            TeletextGenerationTarget {
                request: pending,
                expected_design_head: 2,
                expected_generation_id: Some(flow.accepted[0].generation().id),
            },
            designs::budget(),
            channel_documents::policy(),
            designs::deadline(),
        )
        .await
        .unwrap(),
    );
    let day = pattern(&edited, 2);
    let night = pattern(&flow.accepted[1], 1);
    let views = origin.captures();
    let (a, b, c) = tokio::join!(
        wait_cycle(&views[0], offsets[0], &day),
        wait_cycle(&views[1], offsets[1], &day),
        wait_cycle(&views[2], offsets[2], &night)
    );
    let updated = [a, b, c];
    let (a, b, c) = tokio::join!(
        wait_cycle(&views[0], updated[0].end, &day),
        wait_cycle(&views[1], updated[1].end, &day),
        wait_cycle(&views[2], updated[2].end, &night)
    );
    let repeated = [a, b, c];
    for index in 0..3 {
        write_units(
            protocol.directory,
            &format!("viewer-{index}-updated-units.json"),
            &updated[index],
        );
        write_units(
            protocol.directory,
            &format!("viewer-{index}-repeat-units.json"),
            &repeated[index],
        );
    }
    live_tcp::write_new(&protocol.directory.join("wire-offsets.json"),&serde_json::to_vec_pretty(&json!({
        "original_complete_pes_end":before.each_ref().map(|c|c.end),"pending_complete_pes_end":held.each_ref().map(|c|c.end),
        "pre_accept_offsets":offsets,"updated_complete_pes_end":updated.each_ref().map(|c|c.end),
        "repeat_complete_pes_end":repeated.each_ref().map(|c|c.end),"day_s2":2,"night_s2":1
    })).unwrap())?;
    protocol.stage(2, &edited, flow, origin).await?;
    Ok(edited)
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct RootComplete {
    probe_version: u8,
    client_name: String,
    client_version: String,
    decoder_version: String,
    proof_suffix: String,
    ready_sha256: String,
    stage_ready_sha256: [String; 2],
}
async fn root_review(protocol: &Protocol<'_>) -> io::Result<Value> {
    let expected = json!({"probe_version":3,"client_name":"VLC","client_version":"3.0.20",
        "decoder_version":"0.2.42","proof_suffix":"warm-drcs-live-ordinary",
        "ready_sha256":protocol.ready_sha256,"stage_ready_sha256":protocol.stage_hashes});
    live_tcp::write_new(&protocol.directory.join("await-root-review.json"),&serde_json::to_vec_pretty(&json!({
        "probe_version":3,"owned_streams_closed":true,"client_stop_receipt_validated":true,
        "visual_match_certified":false,"expected_completion":expected,
        "next":"Inspect all12 original/edited day/night124→125→124 images before writing client-complete.json"
    })).unwrap())?;
    let raw = live_tcp::wait_file(
        protocol.directory,
        "client-complete.json",
        Instant::now() + Duration::from_secs(REVIEW_SECONDS),
    )
    .await?;
    let complete: RootComplete = serde_json::from_slice(&raw)
        .map_err(|_| io::Error::other("invalid closed root DRCS visual completion"))?;
    let actual = json!({"probe_version":complete.probe_version,"client_name":complete.client_name,
        "client_version":complete.client_version,"decoder_version":complete.decoder_version,
        "proof_suffix":complete.proof_suffix,"ready_sha256":complete.ready_sha256,"stage_ready_sha256":complete.stage_ready_sha256});
    if actual != expected {
        return Err(io::Error::other(
            "root visual completion differs from exact DRCS trial",
        ));
    }
    Ok(actual)
}

async fn close_observer(
    origin: &mut live_tcp::Origin,
    observer: &mut live_tcp::Observer,
    directory: &Path,
) -> io::Result<()> {
    let receiver_closed = observer.close().await;
    let closed = origin.close().await;
    let cleaned = live_tcp::wait_for_cleanup().await;
    let receipt = json!({"probe_version":3,
        "observer_closed":receiver_closed.is_ok(),"origin_closed":closed.is_ok(),"capacity_released":cleaned.is_ok(),
        "observer_error":receiver_closed.as_ref().err().map(std::string::ToString::to_string),
        "origin_error":closed.as_ref().err().map(std::string::ToString::to_string),
        "cleanup_error":cleaned.as_ref().err().map(std::string::ToString::to_string)});
    let recorded = live_tcp::write_new(
        &directory.join("cleanup.json"),
        &serde_json::to_vec_pretty(&receipt).unwrap(),
    );
    receiver_closed?;
    closed?;
    cleaned?;
    recorded
}

#[tokio::test]
#[ignore = "requires exact QTM files/FFmpeg9/TSDuck, create-new headed directory, separately owned VLC stage captures and root visual approval"]
async fn headed_same_ordinary_urls_keep_decoder_across_actual_qtm_drcs_edit_and_navigation() {
    let directory = PathBuf::from(client_capture::selected(DIRECTORY_ENV));
    let actual = PathBuf::from(client_capture::selected(
        "ERSATZRS_QTM_DRCS_CALIBRATION_DIR",
    ));
    std::fs::create_dir(&directory).unwrap();
    live_tcp::terminal_trace(&directory).unwrap();
    let tools = Tools::load().await;
    let (mut fixture, flow, source) = Box::pin(source_fixture(&tools, &directory)).await;
    let package_hash = calculate_addon_content_sha256(&fixture.root.path().join("package"))
        .await
        .unwrap();
    let mut origin = live_tcp::Origin::start(&fixture, &directory).await.unwrap();
    let mut observer = live_tcp::Observer::start(&origin);
    let ready = serde_json::to_vec_pretty(&ready(&origin, &flow, &source)).unwrap();
    let mut protocol = Protocol {
        directory: &directory,
        deadline: Instant::now() + Duration::from_secs(LIVE_SECONDS),
        ready_sha256: live_tcp::hash(&ready),
        clients: None,
        stages: Vec::new(),
        stage_hashes: Vec::new(),
    };
    let outcome = AssertUnwindSafe(async {
        live_tcp::write_new(&directory.join("ready.json"), &ready)?;
        let edited = timeout_at(
            protocol.deadline,
            transitions(&fixture, &flow, &origin, &actual, &mut protocol),
        )
        .await
        .map_err(|_| io::Error::other("headed DRCS live stages exceeded55seconds"))??;
        let stopped = stopped_clients(&protocol).await?;
        Ok::<_, io::Error>((edited, stopped))
    })
    .catch_unwind()
    .await;
    let closed = close_observer(&mut origin, &mut observer, &directory).await;
    let captures = live_tcp::save_captures(&directory, &origin);
    let (edited, stopped) = match outcome {
        Ok(Ok(value)) => value,
        Ok(Err(error)) => {
            fixture.state.pool.close().await;
            panic!("headed DRCS trial failed: {error}");
        }
        Err(panic) => {
            fixture.state.pool.close().await;
            std::panic::resume_unwind(panic)
        }
    };
    closed.unwrap();
    captures.unwrap();
    assert!(origin.single_requests());
    drcs_bitmap::assert_only_glyph_pixels(flow.accepted[0].service(), edited.service());
    generations::assert_same_output(&recover(&fixture, 1).await, &edited);
    generations::assert_same_output(&recover(&fixture, 2).await, &flow.accepted[1]);
    assert_retained_storage(&fixture, &flow).await;
    assert_eq!(
        calculate_addon_content_sha256(&fixture.root.path().join("package"))
            .await
            .unwrap(),
        package_hash
    );
    assert!(fixture.playout.try_recv().is_err());
    fixture.state.pool.close().await;
    let receipts = verify(&directory, &tools, &source, &origin).await;
    let complete = root_review(&protocol).await.unwrap();
    live_tcp::write_new(&directory.join("success.json"),&serde_json::to_vec_pretty(&json!({
        "probe_version":3,"protocol":"warm-drcs-live-ordinary-v1","source_requests_per_viewer":1,
        "source_resets_before_start":1,"source_resets_after_start":0,"same_client_and_decoder_receipts":stopped,
        "source_sha256":live_tcp::hash(&source),"package_sha256":package_hash,
        "stages":protocol.stages,"native_receipts":receipts,"root_completion":complete,
        "owned_streams_closed_before_visual_review":true,"channel2_and_originals_receipts_package_unchanged":true,
        "limitations":"Representative MAIN asymmetric DRCS glyph/cache/navigation only; no full authoredLevel2.5, panels, numeric entry, EOF or platform acceptance"
    })).unwrap()).unwrap();
}
