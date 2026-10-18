//! Headed natural-expiry barriers on the original finite ordinary responses.
//! Markers synchronize captures; root-owned image inspection certifies rendering.

use super::super::super::{
    client_capture, designs,
    editorial_imports::live_updates::{live_tcp as shared, Capture},
};
use super::{base_wire, bases, cold, on_for, recover_base, retained, Base, Scoped};
use chrono::Utc;
use ersatzrs_app::teletext_design_documents::decode_teletext_effective_design_documents;
use ersatzrs_core::repositories::{
    TELETEXT_EFFECTIVE_GENERATION_ARTIFACT_VERSION,
    TELETEXT_EFFECTIVE_LIFECYCLE_GENERATION_ARTIFACT_VERSION, TELETEXT_GENERATION_ARTIFACT_VERSION,
    TELETEXT_LIFECYCLE_GENERATION_ARTIFACT_VERSION,
};
use ersatzrs_domain::{TeletextAcceptedGeneration, TeletextEffectiveBindingSnapshot};
use futures_util::FutureExt as _;
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use std::{
    io,
    panic::AssertUnwindSafe,
    path::{Path, PathBuf},
    time::Duration,
};
use tokio::time::{timeout_at, Instant};

const DIRECTORY_ENV: &str = "ERSATZRS_TELETEXT_EXPIRY_CLIENT_DIR";
const MAX_SOURCE: usize = 48 * 1024 * 1024;

#[derive(Clone, Copy, PartialEq, Eq)]
enum ScopeMode {
    On,
    Off,
}

impl ScopeMode {
    fn version(self) -> u8 {
        match self {
            Self::On => 1,
            Self::Off => 2,
        }
    }
    fn suffix(self) -> &'static str {
        match self {
            Self::On => "natural-expiry-live-ordinary",
            Self::Off => "natural-off-expiry-live-ordinary",
        }
    }
}

fn current(generation: &TeletextAcceptedGeneration, version: u16, q: bool) -> Value {
    // Document role and compiled resource-lifecycle framing are distinct.
    assert!(matches!(
        (version, generation.artifact_version),
        (
            1,
            TELETEXT_GENERATION_ARTIFACT_VERSION | TELETEXT_LIFECYCLE_GENERATION_ARTIFACT_VERSION
        ) | (
            2,
            TELETEXT_EFFECTIVE_GENERATION_ARTIFACT_VERSION
                | TELETEXT_EFFECTIVE_LIFECYCLE_GENERATION_ARTIFACT_VERSION
        )
    ));
    let request = &generation.request;
    json!({"channel_id":request.channel_id,"generation_id":generation.id,"request_id":request.id,
        "design_id":request.design_id,"design_revision":request.revision,"design_digest":request.digest,
        "document_version":version,"teletext_visible":true,"edited_q_present":q,
        "theme_rgb4":if request.channel_id==1 {[15,0,0]} else {[0,0,15]}})
}

fn scoped_record(scoped: &Scoped) -> Value {
    let original =
        decode_teletext_effective_design_documents(&scoped.original.document, designs::budget())
            .unwrap();
    let provenance = original.editorial_overlay().unwrap().provenance();
    let TeletextEffectiveBindingSnapshot::Scripted(published) =
        &scoped.accepted.selection().bindings[0]
    else {
        panic!("genuine Scripted origin required")
    };
    assert_eq!(published.interval.finish, scoped.finish);
    json!({"design_id":scoped.original.design_id,"revision":scoped.original.revision,"digest":scoped.original.digest,
        "generation_id":scoped.accepted.generation().id,"request_id":scoped.accepted.generation().request.id,
        "original_design_id":provenance.original_design_id,"original_revision":provenance.original_revision,"original_digest":provenance.original_digest,
        "root_receipt_id":provenance.export_receipt_id,"root_receipt_digest":provenance.export_receipt_digest,
        "start":published.interval.start.to_rfc3339(),"finish":scoped.finish.to_rfc3339()})
}

fn ready(base: &Base, origin: &shared::Origin, source: &[u8]) -> Value {
    let channels=base.accepted.iter().enumerate().map(|(index,candidate)| {
        let binding=&candidate.selection().bindings[0];
        let request=&candidate.generation().request;
        let theme=if index==0 {"day"} else {"night"};
        json!({"channel_id":request.channel_id,"theme":theme,"url":origin.url(u8::try_from(index+1).unwrap(),theme),
            "addon_id":binding.addon_id,"profile_id":binding.profile_id,"profile_revision":binding.profile_revision,
            "base_generation_id":candidate.generation().id,"base_request_id":request.id,"base_design_id":request.design_id,
            "base_design_revision":request.revision,"base_digest":request.digest,"theme_rgb4":if index==0 {[15,0,0]} else {[0,0,15]}})
    }).collect::<Vec<_>>();
    json!({"probe_version":1,"protocol":"natural-expiry-live-ordinary-v1","completion_timeout_seconds":180,
        "source":{"file":"source.ts","bytes":source.len(),"sha256":shared::hash(source),"duration_seconds":60},"channels":channels,
        "expected_pages":[200,201],"subcode":0,"stages":["base","scoped_q","expired_base"],
        "stage_files":"stage-N-ready.json and stage-N-captured.json, N from 1 through 3",
        "final_completion":{"probe_version":1,"client_name":"VLC","client_version":"3.0.20","decoder_version":"0.2.42","proof_suffix":"natural-expiry-live-ordinary"},
        "limitations":"Stage captures certify no visual match; immediate UTC clearing, resource retirement, complete Level 2.5 and platform acceptance remain separate."})
}

fn ready_for(base: &Base, origin: &shared::Origin, source: &[u8], mode: ScopeMode) -> Value {
    let mut value = ready(base, origin, source);
    if mode == ScopeMode::Off {
        value["probe_version"] = json!(mode.version());
        value["protocol"] = json!("natural-off-expiry-live-ordinary-v1");
        value["stages"] = json!(["base", "scoped_off", "expired_base"]);
        value["final_completion"]["probe_version"] = json!(mode.version());
        value["final_completion"]["proof_suffix"] = json!(mode.suffix());
    }
    value
}

fn absent(generation: &TeletextAcceptedGeneration) -> Value {
    let mut value = current(generation, 2, false);
    value["teletext_visible"] = json!(false);
    value["theme_rgb4"] = Value::Null;
    value["owner"] = Value::Null;
    value["producer_count"] = json!(0);
    value
}

struct Protocol<'a> {
    mode: ScopeMode,
    directory: &'a Path,
    deadline: Instant,
    clients: Option<[u32; 2]>,
    receipts: Vec<Value>,
}

impl Protocol<'_> {
    async fn stage(
        &mut self,
        index: u8,
        day: Value,
        night: Value,
        scoped: Value,
        views: &[Capture; 3],
        limit: Instant,
    ) -> io::Result<()> {
        let state = match index {
            1 => "base",
            2 if self.mode == ScopeMode::Off => "scoped_off",
            2 => "scoped_q",
            3 => "expired_base",
            _ => return Err(io::Error::other("unknown expiry stage")),
        };
        let ready = json!({"probe_version":self.mode.version(),"stage":index,"state":state,"scope":"Wire-observed readiness; capture markers certify no rendered match",
            "snapshot_utc":Utc::now().to_rfc3339(),"day":day,"night":night,"scoped":scoped,
            "expected_pages":[200,201],"subcode":0,"wire_offsets":views.each_ref().map(Capture::position)});
        let bytes = serde_json::to_vec_pretty(&ready).unwrap();
        let digest = shared::hash(&bytes);
        shared::write_new(
            &self.directory.join(format!("stage-{index}-ready.json")),
            &bytes,
        )?;
        let raw = shared::wait_file(
            self.directory,
            &format!("stage-{index}-captured.json"),
            self.deadline.min(limit),
        )
        .await?;
        let marker: shared::StageCaptured = serde_json::from_slice(&raw)
            .map_err(|_| io::Error::other("invalid closed expiry stage marker"))?;
        if marker.probe_version != self.mode.version()
            || marker.stage != index
            || marker.ready_sha256 != digest
            || marker.clients.len() != 2
        {
            return Err(io::Error::other(
                "expiry marker does not bind exact readiness",
            ));
        }
        let mut pids = [0; 2];
        for (slot, viewer) in ["day", "night"].iter().enumerate() {
            let matches: Vec<_> = marker
                .clients
                .iter()
                .filter(|client| client.viewer == *viewer)
                .collect();
            if matches.len() != 1 || matches[0].pid == 0 {
                return Err(io::Error::other("expiry marker lacks unique owned viewers"));
            }
            pids[slot] = matches[0].pid;
        }
        if pids[0] == pids[1] || self.clients.is_some_and(|prior| prior != pids) {
            return Err(io::Error::other("expiry stage replaced a client"));
        }
        let initial = [Some(200)];
        let inactive = [None];
        let restored = [None, Some(200), Some(201), Some(200)];
        let expected = if index == 3 {
            &restored[..]
        } else {
            &initial[..]
        };
        let day = if index == 2 && self.mode == ScopeMode::Off {
            &inactive[..]
        } else {
            expected
        };
        shared::validate_images_for(&marker, [day, expected])?;
        self.clients = Some(pids);
        let mut receipt = json!({"stage":index,"ready_sha256":digest,"capture":marker,"visual_match_certified":false});
        if self.mode == ScopeMode::Off && index == 2 {
            receipt["off_window"] = json!({
                "start_offsets":ready["wire_offsets"],"start_utc":ready["snapshot_utc"],
                "end_offsets":views.each_ref().map(Capture::position),"end_utc":Utc::now().to_rfc3339(),
                "scope":"Original-response window after wire absence and before scoped capture completion; replay must independently check no authored PES/descriptor on day and observer"});
        }
        self.receipts.push(receipt);
        Ok(())
    }
}

#[derive(Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
struct Complete {
    probe_version: u8,
    client_name: String,
    client_version: String,
    decoder_version: String,
    proof_suffix: String,
}

async fn completion(protocol: &Protocol<'_>) -> io::Result<Complete> {
    let raw = shared::wait_file(
        protocol.directory,
        "client-complete.json",
        protocol.deadline,
    )
    .await?;
    let value: Complete = serde_json::from_slice(&raw)
        .map_err(|_| io::Error::other("invalid closed expiry completion"))?;
    if value.probe_version != protocol.mode.version()
        || value.client_name != "VLC"
        || value.client_version != "3.0.20"
        || value.decoder_version != "0.2.42"
        || value.proof_suffix != protocol.mode.suffix()
    {
        return Err(io::Error::other("unexpected expiry client completion"));
    }
    Ok(value)
}

struct Observation {
    scoped: Scoped,
    utc_offsets: [usize; 3],
    utc_observed: String,
    restored_offsets: [usize; 3],
    restored_utc: String,
    off_window: Option<Value>,
}

async fn transitions(
    base: &Base,
    origin: &shared::Origin,
    protocol: &mut Protocol<'_>,
) -> io::Result<Observation> {
    shared::wait_attached(origin, protocol.deadline).await?;
    let views = origin.captures();
    base_wire(views, [0; 3], base).await;
    let night = || current(base.accepted[1].generation(), 1, false);
    protocol
        .stage(
            1,
            current(base.accepted[0].generation(), 1, false),
            night(),
            Value::Null,
            views,
            protocol.deadline,
        )
        .await?;
    let positions = views.each_ref().map(Capture::position);
    let scoped = match protocol.mode {
        ScopeMode::On => on_for(base, 35).await,
        ScopeMode::Off => super::off::prepare(base).await,
    };
    for (index, view) in views[..2].iter().enumerate() {
        match scoped.accepted.service() {
            Some(service) => view.service_encoded(positions[index], service).await,
            None => view.off(positions[index]).await,
        }
    }
    views[2]
        .service_encoded(positions[2], base.accepted[1].service())
        .await;
    let (scope_record, day) = match protocol.mode {
        ScopeMode::On => (
            scoped_record(&scoped),
            current(scoped.accepted.generation(), 2, true),
        ),
        ScopeMode::Off => (
            super::off::record(&scoped),
            absent(scoped.accepted.generation()),
        ),
    };
    let before = retained(&base.fixture.fixture).await;
    let calls = base.fixture.calls();
    let until = (scoped.finish - Utc::now())
        .to_std()
        .map_err(|_| io::Error::other("scope expired before headed scoped capture"))?;
    protocol
        .stage(
            2,
            day,
            night(),
            scope_record.clone(),
            views,
            Instant::now() + until,
        )
        .await?;
    let off_window = if protocol.mode == ScopeMode::Off {
        assert!(Utc::now() < scoped.finish);
        Some(protocol.receipts.last().unwrap()["off_window"].clone())
    } else {
        None
    };
    tokio::time::sleep((scoped.finish - Utc::now()).to_std().unwrap_or_default()).await;
    assert!(Utc::now() >= scoped.finish);
    let utc_observed = Utc::now().to_rfc3339();
    let utc_offsets = views.each_ref().map(Capture::position);
    base_wire(views, utc_offsets, base).await;
    let first = views.each_ref().map(Capture::position);
    base_wire(views, first, base).await;
    let restored_offsets = views.each_ref().map(Capture::position);
    let restored_utc = Utc::now().to_rfc3339();
    protocol
        .stage(
            3,
            current(base.accepted[0].generation(), 1, false),
            night(),
            scope_record,
            views,
            protocol.deadline,
        )
        .await?;
    assert_eq!(retained(&base.fixture.fixture).await, before);
    assert_eq!(base.fixture.calls(), calls);
    assert!(origin.single_requests());
    Ok(Observation {
        scoped,
        utc_offsets,
        utc_observed,
        restored_offsets,
        restored_utc,
        off_window,
    })
}

async fn source_base(ffmpeg: &str, directory: &Path) -> (Base, PathBuf, Vec<u8>) {
    let fixture = cold::fixture().await;
    let source_path =
        client_capture::generate_source_with_duration(ffmpeg, fixture.fixture.root.path(), 60)
            .await;
    cold::configure_source(&fixture.fixture, &source_path).await;
    let source = shared::read_bounded(&source_path, MAX_SOURCE).unwrap();
    shared::write_new(&directory.join("source.ts"), &source).unwrap();
    (bases(fixture).await, source_path, source)
}

async fn verify_restored(
    directory: &Path,
    ffmpeg: &str,
    views: &[Capture; 3],
    observation: &Observation,
) {
    for (index, view) in views.iter().enumerate() {
        let bytes = view.bytes();
        let fragment = &bytes[observation.restored_offsets[index]..bytes.len() / 188 * 188];
        assert!(fragment.len() >= 188 * 1000);
        shared::write_new(
            &directory.join(format!(
                "{}-observed-base.ts",
                ["day", "observer", "night"][index]
            )),
            fragment,
        )
        .unwrap();
        for page in ["200", "201"] {
            let text =
                client_capture::decode_page_text_with_discovery(ffmpeg, fragment, page, true).await;
            assert!(text.contains("AZ") && !text.contains('Q') && !text.contains('R'));
            shared::write_new(
                &directory.join(format!(
                    "{}-observed-base-{page}.srt",
                    ["day", "observer", "night"][index]
                )),
                text.as_bytes(),
            )
            .unwrap();
        }
    }
}

async fn native_checks(base: &Base, observation: &Observation) {
    for (index, expected) in base.accepted.iter().enumerate() {
        let actual = recover_base(&base.fixture.fixture, i64::try_from(index + 1).unwrap()).await;
        assert_eq!(actual.generation(), expected.generation());
        assert_eq!(actual.selection(), expected.selection());
        assert_eq!(
            designs::units(actual.service()),
            designs::units(expected.service())
        );
    }
    // Expiry retains historical accepted metadata; fresh scoped recovery remains
    // forbidden. The same original UTC deadline is neither renewed nor edited.
    assert_eq!(
        base.fixture
            .fixture
            .state
            .application
            .effective_teletext_accepted_generation(1)
            .await
            .unwrap()
            .as_ref(),
        Some(observation.scoped.accepted.generation())
    );
}

#[tokio::test]
#[ignore = "Requires pinned FFmpeg9/TSDuck, owned headed clients, three capture barriers and root-only completion; source remains genuine60s"]
async fn original_headed_clients_observe_natural_expiry_and_navigate_restored_base() {
    run(ScopeMode::On).await;
}

#[tokio::test]
#[ignore = "Requires pinned FFmpeg9/TSDuck and owned headed Off-expiry clients; captures/native state only, automatic display requires root visual qualification"]
async fn original_headed_clients_capture_natural_off_expiry_and_navigate_restored_base() {
    run(ScopeMode::Off).await;
}

async fn run(mode: ScopeMode) {
    let directory = PathBuf::from(client_capture::selected(DIRECTORY_ENV));
    std::fs::create_dir(&directory).unwrap();
    shared::terminal_trace(&directory).unwrap();
    let ffmpeg = client_capture::selected("ERSATZRS_TELETEXT_TEST_FFMPEG");
    let ffprobe = client_capture::selected("FFPROBE_PATH");
    let tsanalyze = client_capture::selected("ERSATZRS_TELETEXT_TEST_TSANALYZE");
    client_capture::check_tools(&ffmpeg, &ffprobe).await;
    let (base, source_path, source) = Box::pin(source_base(&ffmpeg, &directory)).await;
    let package = ersatzrs_infra::addons::calculate_addon_content_sha256(
        &base.fixture.fixture.root.path().join("package"),
    )
    .await
    .unwrap();
    let deadline = Instant::now() + Duration::from_mins(3);
    let mut origin = shared::Origin::start(&base.fixture.fixture, &directory)
        .await
        .unwrap();
    let mut observer = shared::Observer::start(&origin);
    let mut protocol = Protocol {
        mode,
        directory: &directory,
        deadline,
        clients: None,
        receipts: Vec::new(),
    };
    let outcome = AssertUnwindSafe(async {
        shared::write_new(
            &directory.join("ready.json"),
            &serde_json::to_vec_pretty(&ready_for(&base, &origin, &source, mode)).unwrap(),
        )?;
        let observation = timeout_at(
            deadline,
            Box::pin(transitions(&base, &origin, &mut protocol)),
        )
        .await
        .map_err(|_| io::Error::other("headed expiry exceeded original deadline"))??;
        let complete = completion(&protocol).await?;
        assert_eq!(
            ersatzrs_infra::addons::calculate_addon_content_sha256(
                &base.fixture.fixture.root.path().join("package")
            )
            .await
            .unwrap(),
            package
        );
        assert_eq!(shared::read_bounded(&source_path, MAX_SOURCE)?, source);
        Ok::<_, io::Error>((observation, complete))
    })
    .catch_unwind()
    .await;
    let observer_closed = observer.close().await;
    let origin_closed = origin.close().await;
    let cleanup = shared::wait_for_cleanup().await;
    let captures = shared::save_captures(&directory, &origin);
    let (observation, complete) = match outcome {
        Ok(Ok(value)) => value,
        Ok(Err(error)) => {
            base.fixture.fixture.state.pool.close().await;
            panic!("headed natural-expiry trial failed: {error}")
        }
        Err(panic) => {
            base.fixture.fixture.state.pool.close().await;
            std::panic::resume_unwind(panic)
        }
    };
    native_checks(&base, &observation).await;
    base.fixture.fixture.state.pool.close().await;
    observer_closed.unwrap();
    origin_closed.unwrap();
    cleanup.unwrap();
    captures.unwrap();
    if mode == ScopeMode::Off {
        write_off_observation(&directory, &protocol, &observation, &complete, &source);
    }
    verify_restored(&directory, &ffmpeg, origin.captures(), &observation).await;
    verify_capture(
        mode,
        &directory,
        (&ffmpeg, &ffprobe, &tsanalyze),
        &source,
        origin.captures(),
    )
    .await;
    let success = json!({"probe_version":1,"scope":"Headed same-response natural expiry barriers; root image/navigation inspection remains separate",
        "source_sha256":shared::hash(&source),"stages":protocol.receipts,"completion":complete,
        "original_valid_until":observation.scoped.finish.to_rfc3339(),"raw_utc_offsets":observation.utc_offsets,"observed_utc":observation.utc_observed,
        "observed_base_offsets":observation.restored_offsets,"observed_base_utc":observation.restored_utc,
        "observed_base_suffix_is_owner_ack_proof":false,"same_urls_and_client_pids":true,"source_requests_per_viewer":1,"owned_actors_closed":true,
        "original_scope_generations_profiles_settings_packages_and_calls_unchanged_across_expiry":true,
        "limitations":"No immediate UTC clearing or new owner-ACK qualification is claimed; prior qualified native engine evidence remains separate; full authored Level 2.5 and platform acceptance remain open."});
    write_success(&directory, mode, success, observation.off_window);
}

async fn verify_capture(
    mode: ScopeMode,
    directory: &Path,
    tools: (&str, &str, &str),
    source: &[u8],
    views: &[Capture; 3],
) {
    match mode {
        ScopeMode::On => {
            shared::verify_native_pages(directory, tools, source, views, &["200", "201"], false)
                .await;
        }
        ScopeMode::Off => {
            shared::verify_original_native_pages(directory, tools, source, views, &["200", "201"])
                .await;
        }
    }
}

fn write_off_observation(
    directory: &Path,
    protocol: &Protocol<'_>,
    observation: &Observation,
    complete: &Complete,
    source: &[u8],
) {
    let value = json!({"probe_version":2,
        "scope":"Off-expiry state/ownership/capture checkpoint before independent external replay; not trial or visual success",
        "source_sha256":shared::hash(source),"stages":protocol.receipts,"completion":complete,
        "original_valid_until":observation.scoped.finish.to_rfc3339(),"raw_utc_offsets":observation.utc_offsets,"observed_utc":observation.utc_observed,
        "observed_base_offsets":observation.restored_offsets,"observed_base_utc":observation.restored_utc,"off_window":observation.off_window,
        "same_urls_and_client_pids":true,"source_requests_per_viewer":1,"owned_actors_closed":true,
        "original_scope_generations_profiles_settings_packages_and_calls_unchanged_across_expiry":true,
        "independent_external_verification_complete":false,"automatic_display_accepted":false});
    shared::write_new(
        &directory.join("off-observation.json"),
        &serde_json::to_vec_pretty(&value).unwrap(),
    )
    .unwrap();
}

fn write_success(directory: &Path, mode: ScopeMode, mut success: Value, off_window: Option<Value>) {
    if mode == ScopeMode::Off {
        success["probe_version"] = json!(mode.version());
        success["scope"] = json!("Off-expiry capture/native transport/state completion only; automatic display remains unqualified");
        success["automatic_display_accepted"] = json!(false);
        success["off_window"] = off_window.unwrap();
    }
    shared::write_new(
        &directory.join("success.json"),
        &serde_json::to_vec_pretty(&success).unwrap(),
    )
    .unwrap();
}
