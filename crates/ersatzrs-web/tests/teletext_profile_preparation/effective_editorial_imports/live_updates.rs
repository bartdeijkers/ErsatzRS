//! Actual scoped Q -> cumulative QR -> Lua Off on three original TCP responses.
//! Wire barriers and capture markers never certify client pixels or navigation.

use super::super::{
    authoring::AuthoringFixture,
    channel_documents, client_capture, designs,
    editorial_imports::live_updates::{live_tcp as shared, Capture},
    repeated_editorial_imports,
};
use super::live_client::{self as cold, Flow};
use super::{export, import, original};
use ersatzrs_app::teletext_design_documents::decode_teletext_effective_design_documents;
use ersatzrs_domain::TeletextDesignRevision;
use ersatzrs_web::host::{
    teletext_authoring::{
        observe_effective_teletext_authoring_selection, prepare_static_teletext_design,
        TeletextAuthoringIdentity, TeletextAuthoringScope,
    },
    teletext_design_exports::ExportedOriginalTeletextDesign,
    teletext_effective_generations::AcceptedEffectiveTeletextOutputCandidate,
};
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

const DIRECTORY_ENV: &str = "ERSATZRS_TELETEXT_SCOPED_WARM_CLIENT_DIR";
const MAX_SOURCE: usize = 48 * 1024 * 1024;

fn identity(current: &AcceptedEffectiveTeletextOutputCandidate) -> TeletextAuthoringIdentity {
    let request = &current.generation().request;
    TeletextAuthoringIdentity {
        channel_id: request.channel_id,
        design_id: request.design_id,
        revision: request.revision,
    }
}

async fn cumulative_qr(flow: &Flow) -> AcceptedEffectiveTeletextOutputCandidate {
    let fixture = &flow.fixture.fixture;
    let q = original(fixture, identity(&flow.accepted[0])).await;
    assert_eq!(q.revision, 2);
    let exported = export(fixture, &q).await;
    let concept = import(
        fixture,
        repeated_editorial_imports::request(
            &exported,
            2,
            &[(channel_documents::address(200, 0), 3, b'R')],
        ),
    )
    .await;
    let identity = TeletextAuthoringIdentity {
        channel_id: 1,
        design_id: concept.head().id,
        revision: concept.head().revision,
    };
    assert_eq!(identity.revision, 3);
    // Import/preview leave the existing output selected until explicit publication.
    assert_eq!(
        fixture
            .state
            .application
            .effective_teletext_accepted_generation(1)
            .await
            .unwrap()
            .as_ref(),
        Some(flow.accepted[0].generation())
    );
    cold::publish(&flow.fixture, identity).await;
    let current = cold::recover(fixture, 1).await;
    assert_eq!(current.selection(), flow.accepted[0].selection());
    current
}

async fn lua_off(
    flow: &Flow,
) -> (
    AcceptedEffectiveTeletextOutputCandidate,
    TeletextDesignRevision,
) {
    cold::publish_lua_off(&flow.fixture).await;
    let application = &flow.fixture.fixture.state.application;
    let observed =
        observe_effective_teletext_authoring_selection(application, 1, designs::deadline())
            .await
            .unwrap();
    assert!(observed.is_absent);
    let mut request = AuthoringFixture::request(1, observed.selection_digest);
    request.scope = TeletextAuthoringScope::Effective;
    request.input_json.clear();
    let saved = prepare_static_teletext_design(application, request, designs::deadline())
        .await
        .unwrap();
    let preview = saved.preview.unwrap();
    assert!(preview.is_absent() && preview.raster().is_none());
    drop(preview);
    cold::publish_with_owner(&flow.fixture, saved.identity, None).await;
    let current = cold::recover(&flow.fixture.fixture, 1).await;
    assert!(current.service().is_none());
    let original = original(&flow.fixture.fixture, saved.identity).await;
    assert_eq!(original.revision, 1);
    assert_ne!(original.design_id, flow.roots[0].design_id);
    (current, original)
}

fn facts(
    current: &AcceptedEffectiveTeletextOutputCandidate,
    root: &TeletextDesignRevision,
    receipt: Option<&ExportedOriginalTeletextDesign>,
    visible: bool,
    qr: bool,
) -> Value {
    let request = &current.generation().request;
    let root_scope =
        decode_teletext_effective_design_documents(&root.document, designs::budget()).unwrap();
    assert_eq!(root_scope.selection(), current.selection());
    let expiry = root_scope.selection().valid_until.unwrap().to_rfc3339();
    json!({"channel_id":request.channel_id,"scoped_role":2,"design_id":request.design_id,
        "design_revision":request.revision,"design_digest":request.digest,"generation_id":current.generation().id,"request_id":request.id,
        "valid_until":current.selection().valid_until.unwrap().to_rfc3339(),
        "original_design_id":root.design_id,"original_revision":root.revision,"original_digest":root.digest,
        "root_receipt_id":receipt.map(|value|value.metadata().id),"root_receipt_digest":receipt.map(|value|value.metadata().digest.clone()),
        "original_valid_until":expiry,"teletext_visible":visible,"edited_q_present":visible&&request.channel_id==1,
        "edited_r_present":qr&&request.channel_id==1,"theme_rgb4":if request.channel_id==1 {[15,0,0]} else {[0,0,15]}})
}

struct Protocol<'a> {
    directory: &'a Path,
    deadline: Instant,
    clients: Option<[u32; 2]>,
    receipts: Vec<Value>,
    stage_prefixes: Vec<[usize; 3]>,
}

impl Protocol<'_> {
    async fn stage(
        &mut self,
        index: u8,
        state: &str,
        day: Value,
        night: Value,
        views: &[Capture; 3],
    ) -> io::Result<()> {
        let offsets = views.each_ref().map(Capture::position);
        let ready = json!({"probe_version":2,"stage":index,"state":state,
            "scope":"Physical wire observed; external capture marker is not visual certification",
            "day":day,"night":night,"expected_pages":[200,201],"subcode":0,"wire_offsets":offsets});
        let bytes = serde_json::to_vec_pretty(&ready).unwrap();
        let digest = shared::hash(&bytes);
        shared::write_new(
            &self.directory.join(format!("stage-{index}-ready.json")),
            &bytes,
        )?;
        let raw = shared::wait_file(
            self.directory,
            &format!("stage-{index}-captured.json"),
            self.deadline,
        )
        .await?;
        let marker: shared::StageCaptured = serde_json::from_slice(&raw)
            .map_err(|_| io::Error::other("invalid closed scoped-stage marker"))?;
        if marker.probe_version != 2
            || marker.stage != index
            || marker.ready_sha256 != digest
            || marker.clients.len() != 2
        {
            return Err(io::Error::other(
                "scoped stage does not bind exact readiness",
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
                return Err(io::Error::other(
                    "scoped stage requires unique owned viewers",
                ));
            }
            pids[slot] = matches[0].pid;
        }
        if pids[0] == pids[1] || self.clients.is_some_and(|previous| previous != pids) {
            return Err(io::Error::other("scoped stage replaced a client"));
        }
        let navigation = [Some(200), Some(201), Some(200)];
        let single = [Some(200)];
        let off = [None];
        shared::validate_images_for(
            &marker,
            match index {
                1 => [&navigation[..], &navigation[..]],
                2 => [&single[..], &single[..]],
                3 => [&off[..], &single[..]],
                _ => return Err(io::Error::other("unknown scoped stage")),
            },
        )?;
        self.clients = Some(pids);
        self.receipts.push(json!({"stage":index,"ready_sha256":digest,"capture":marker,"visual_match_certified":false}));
        // Include the actual navigation/capture interval, before the next
        // publication. Readiness alone may precede the next page-closing header.
        self.stage_prefixes
            .push(views.each_ref().map(Capture::position));
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
    let marker: Complete = serde_json::from_slice(&raw)
        .map_err(|_| io::Error::other("invalid closed scoped warm completion"))?;
    if marker.probe_version != 2
        || marker.client_name != "VLC"
        || marker.client_version != "3.0.20"
        || marker.decoder_version != "0.2.42"
        || marker.proof_suffix != "warm-scoped-live-ordinary"
    {
        return Err(io::Error::other(
            "unexpected scoped warm completion identity",
        ));
    }
    Ok(marker)
}

fn ready(origin: &shared::Origin, flow: &Flow, source: &[u8]) -> Value {
    let mut channels = [cold::channel_facts(flow, 0), cold::channel_facts(flow, 1)];
    for (index, channel) in channels.iter_mut().enumerate() {
        channel["url"] = json!(origin.url(
            u8::try_from(index + 1).unwrap(),
            if index == 0 { "day" } else { "night" }
        ));
        channel["edited_r_page"] = Value::Null;
    }
    json!({"probe_version":2,"protocol":"warm-scoped-live-ordinary-v2","completion_timeout_seconds":180,
        "source":{"file":"source.ts","sha256":shared::hash(source),"bytes":source.len(),"duration_seconds":60},"channels":channels,
        "expected_pages":[200,201],"subcode":0,"stages":["q","cumulative_qr","lua_off"],
        "stage_files":"stage-N-ready.json and stage-N-captured.json, N from 1 through 3",
        "final_completion":{"probe_version":2,"client_name":"VLC","client_version":"3.0.20","decoder_version":"0.2.42","proof_suffix":"warm-scoped-live-ordinary"},
        "limitations":"Representative resource-free scoped warm flow; stage markers certify no pixels; original cold roots/expiry stay immutable; complete Level 2.5 and platform acceptance remain open."})
}

async fn immutable_state(flow: &Flow) -> Vec<Vec<String>> {
    let fixture = &flow.fixture.fixture;
    let mut state = Vec::new();
    for sql in [
        r#"SELECT json_array("Key","Value") FROM "ConfigElement" ORDER BY "Key""#,
        r#"SELECT json_array("AddonId","Enabled","StateRevision","PackageSha256","ContentSha256","ManifestJson") FROM "AddonInstallation" ORDER BY "AddonId""#,
        r#"SELECT json_array("Id","AddonId","Name","Revision","IsDeleted","InstallationStateRevision","InstallationContentSha256") FROM "TeletextProfile" ORDER BY "Id""#,
        r#"SELECT json_array("ProfileId","Key","Kind","Value","Reference") FROM "TeletextProfileSetting" ORDER BY "ProfileId","Key""#,
        r#"SELECT json_array("ChannelId","AddonId","ProfileId","Enabled","Revision") FROM "ChannelTeletextBinding" ORDER BY "ChannelId","AddonId""#,
        r#"SELECT json_array("ChannelId","GenerationId") FROM "TeletextAcceptedGenerationCurrent" ORDER BY "ChannelId""#,
        r#"SELECT json_array("ChannelId","RequestId") FROM "TeletextPublicationRequestCurrent" ORDER BY "ChannelId""#,
        r#"SELECT json_array("ChannelId","GenerationId") FROM "TeletextEffectiveAcceptedGenerationCurrent" WHERE "ChannelId"=2"#,
        r#"SELECT json_array("ChannelId","RequestId") FROM "TeletextEffectivePublicationRequestCurrent" WHERE "ChannelId"=2"#,
        r#"SELECT json_array("PlayoutId","AddonId","Start","Finish","State","CandidateSnapshot") FROM "PlayoutTeletextInterval" WHERE "PlayoutId"=94 ORDER BY "AddonId","Start""#,
        r#"SELECT json_array("Id","PlayoutId","MediaItemId","Start","Finish") FROM "PlayoutItem" WHERE "PlayoutId"=94 ORDER BY "Id""#,
        r#"SELECT json_array("PlayoutId","CheckpointDate","Context") FROM "PlayoutScriptedCheckpoint" WHERE "PlayoutId"=94 ORDER BY "CheckpointDate""#,
    ] {
        state.push(
            sqlx::query_scalar(sql)
                .fetch_all(&fixture.state.pool)
                .await
                .unwrap(),
        );
    }
    for root in &flow.roots {
        state.push(sqlx::query_scalar(r#"SELECT json_array("DesignId","Revision","DocumentVersion","Digest",hex("Document")) FROM "TeletextDesignRevision" WHERE "DesignId"=? AND "Revision"=1"#).bind(root.design_id).fetch_all(&fixture.state.pool).await.unwrap());
    }
    for exported in &flow.root_exports {
        state.push(sqlx::query_scalar(r#"SELECT json_array("Id","DesignId","Revision","Digest",hex("Receipt")) FROM "TeletextDesignExportReceipt" WHERE "Id"=?"#).bind(exported.metadata().id).fetch_all(&fixture.state.pool).await.unwrap());
    }
    state
}

async fn transitions(
    flow: &Flow,
    origin: &shared::Origin,
    protocol: &mut Protocol<'_>,
) -> io::Result<(
    AcceptedEffectiveTeletextOutputCandidate,
    TeletextDesignRevision,
)> {
    shared::wait_attached(origin, protocol.deadline).await?;
    let views = origin.captures();
    for view in &views[..2] {
        view.service_encoded(0, flow.accepted[0].service().unwrap())
            .await;
    }
    views[2]
        .service_encoded(0, flow.accepted[1].service().unwrap())
        .await;
    let night = || {
        facts(
            &flow.accepted[1],
            &flow.roots[1],
            Some(&flow.root_exports[1]),
            true,
            false,
        )
    };
    protocol
        .stage(
            1,
            "q",
            facts(
                &flow.accepted[0],
                &flow.roots[0],
                Some(&flow.root_exports[0]),
                true,
                false,
            ),
            night(),
            views,
        )
        .await?;
    let offsets = views.each_ref().map(Capture::position);
    let qr = cumulative_qr(flow).await;
    for (index, view) in views.iter().enumerate() {
        view.service_encoded(
            offsets[index],
            if index == 2 {
                flow.accepted[1].service().unwrap()
            } else {
                qr.service().unwrap()
            },
        )
        .await;
    }
    protocol
        .stage(
            2,
            "cumulative_qr",
            facts(&qr, &flow.roots[0], Some(&flow.root_exports[0]), true, true),
            night(),
            views,
        )
        .await?;
    let qr_original = original(&flow.fixture.fixture, identity(&qr)).await;
    let offsets = views.each_ref().map(Capture::position);
    let (off, off_original) = lua_off(flow).await;
    for (index, view) in views[..2].iter().enumerate() {
        view.off(offsets[index]).await;
    }
    views[2]
        .service_encoded(offsets[2], flow.accepted[1].service().unwrap())
        .await;
    protocol
        .stage(
            3,
            "lua_off",
            facts(&off, &off_original, None, false, false),
            night(),
            views,
        )
        .await?;
    // The genuine scheduling reset never changes the retained QR source scope.
    assert_eq!(
        original(&flow.fixture.fixture, identity(&qr)).await,
        qr_original
    );
    Ok((off, off_original))
}

async fn verify_stage_text(
    directory: &Path,
    ffmpeg: &str,
    views: &[Capture; 3],
    prefixes: &[[usize; 3]],
) {
    for (stage, positions) in prefixes.iter().take(2).enumerate() {
        for (index, view) in views.iter().enumerate() {
            let bytes = view.bytes();
            for page in ["200", "201"] {
                let text = client_capture::decode_page_text_with_discovery(
                    ffmpeg,
                    &bytes[..positions[index]],
                    page,
                    true,
                )
                .await;
                assert!(text.contains("AZ"));
                assert_eq!(text.contains('Q'), index != 2 && page == "200");
                assert_eq!(
                    text.contains('R'),
                    stage == 1 && index != 2 && page == "200"
                );
                shared::write_new(
                    &directory.join(format!(
                        "stage-{}-{}-decoded-{page}.srt",
                        stage + 1,
                        ["day", "observer", "night"][index]
                    )),
                    text.as_bytes(),
                )
                .unwrap();
            }
        }
    }
}

#[tokio::test]
async fn warm_scoped_native_transitions_preserve_cumulative_root_and_publish_real_absence() {
    let flow = cold::complete_with_zone(cold::fixture().await, false).await;
    let before = immutable_state(&flow).await;
    let calls = flow.fixture.calls();
    assert_eq!(flow.accepted[0].generation().request.revision, 2);
    let qr = cumulative_qr(&flow).await;
    let qr_original = original(&flow.fixture.fixture, identity(&qr)).await;
    let scope =
        decode_teletext_effective_design_documents(&qr_original.document, designs::budget())
            .unwrap();
    assert_eq!(scope.selection(), flow.accepted[0].selection());
    assert_eq!(
        scope.editorial_overlay().unwrap().pages()[0].rows().len(),
        2
    );
    let (off, off_original) = lua_off(&flow).await;
    assert_ne!(off.generation().id, qr.generation().id);
    assert!(off.service().is_none());
    assert_eq!(
        original(&flow.fixture.fixture, identity(&qr)).await,
        qr_original
    );
    assert_eq!(immutable_state(&flow).await, before);
    assert_eq!(flow.fixture.calls(), calls);
    cold::same_output(&cold::recover(&flow.fixture.fixture, 1).await, &off);
    cold::same_output(
        &cold::recover(&flow.fixture.fixture, 2).await,
        &flow.accepted[1],
    );
    assert_eq!(
        facts(&off, &off_original, None, false, false)["root_receipt_id"],
        Value::Null
    );
    flow.fixture.fixture.state.pool.close().await;
}

async fn source_flow(ffmpeg: &str, directory: &Path) -> (Flow, PathBuf, Vec<u8>) {
    let fixture = cold::fixture().await;
    let source_path =
        client_capture::generate_source_with_duration(ffmpeg, fixture.fixture.root.path(), 60)
            .await;
    cold::configure_source(&fixture.fixture, &source_path).await;
    let source = shared::read_bounded(&source_path, MAX_SOURCE).unwrap();
    shared::write_new(&directory.join("source.ts"), &source).unwrap();
    let flow = cold::complete_with_zone(fixture, false).await;
    (flow, source_path, source)
}

#[tokio::test]
#[ignore = "Requires exact FFmpeg9/TSDuck, two owned headed clients and three bound stage markers plus root-only completion"]
async fn warm_scoped_cumulative_tcp_views_wait_for_headed_q_qr_and_lua_off() {
    let directory = PathBuf::from(client_capture::selected(DIRECTORY_ENV));
    std::fs::create_dir(&directory).unwrap();
    shared::terminal_trace(&directory).unwrap();
    let ffmpeg = client_capture::selected("ERSATZRS_TELETEXT_TEST_FFMPEG");
    let ffprobe = client_capture::selected("FFPROBE_PATH");
    let tsanalyze = client_capture::selected("ERSATZRS_TELETEXT_TEST_TSANALYZE");
    client_capture::check_tools(&ffmpeg, &ffprobe).await;
    let (flow, source_path, source) = Box::pin(source_flow(&ffmpeg, &directory)).await;
    let baseline = immutable_state(&flow).await;
    let calls = flow.fixture.calls();
    let package = ersatzrs_infra::addons::calculate_addon_content_sha256(
        &flow.fixture.fixture.root.path().join("package"),
    )
    .await
    .unwrap();
    let mut origin = shared::Origin::start(&flow.fixture.fixture, &directory)
        .await
        .unwrap();
    let mut observer = shared::Observer::start(&origin);
    let deadline = Instant::now() + Duration::from_mins(3);
    let mut protocol = Protocol {
        directory: &directory,
        deadline,
        clients: None,
        receipts: Vec::new(),
        stage_prefixes: Vec::new(),
    };
    let outcome = AssertUnwindSafe(async {
        shared::write_new(
            &directory.join("ready.json"),
            &serde_json::to_vec_pretty(&ready(&origin, &flow, &source)).unwrap(),
        )?;
        let current = timeout_at(deadline, transitions(&flow, &origin, &mut protocol))
            .await
            .map_err(|_| io::Error::other("scoped transitions exceeded original deadline"))??;
        let complete = completion(&protocol).await?;
        assert_eq!(immutable_state(&flow).await, baseline);
        assert_eq!(flow.fixture.calls(), calls);
        assert_eq!(
            ersatzrs_infra::addons::calculate_addon_content_sha256(
                &flow.fixture.fixture.root.path().join("package")
            )
            .await
            .unwrap(),
            package
        );
        assert_eq!(shared::read_bounded(&source_path, MAX_SOURCE)?, source);
        Ok::<_, io::Error>((current, complete))
    })
    .catch_unwind()
    .await;
    let observer_closed = observer.close().await;
    let origin_closed = origin.close().await;
    let cleanup = shared::wait_for_cleanup().await;
    let captures = shared::save_captures(&directory, &origin);
    let ((off, off_original), complete) = match outcome {
        Ok(Ok(result)) => result,
        Ok(Err(error)) => {
            flow.fixture.fixture.state.pool.close().await;
            panic!("scoped warm trial failed: {error}")
        }
        Err(panic) => {
            flow.fixture.fixture.state.pool.close().await;
            std::panic::resume_unwind(panic)
        }
    };
    let recovered_off = cold::recover(&flow.fixture.fixture, 1).await;
    let recovered_night = cold::recover(&flow.fixture.fixture, 2).await;
    flow.fixture.fixture.state.pool.close().await;
    observer_closed.unwrap();
    origin_closed.unwrap();
    cleanup.unwrap();
    captures.unwrap();
    cold::same_output(&recovered_off, &off);
    cold::same_output(&recovered_night, &flow.accepted[1]);
    // Decode/raster comparisons run after paced producers and owned bodies stop.
    verify_stage_text(
        &directory,
        &ffmpeg,
        origin.captures(),
        &protocol.stage_prefixes,
    )
    .await;
    shared::verify_native_pages(
        &directory,
        (&ffmpeg, &ffprobe, &tsanalyze),
        &source,
        origin.captures(),
        &["200", "201"],
        true,
    )
    .await;
    shared::write_new(&directory.join("success.json"),&serde_json::to_vec_pretty(&json!({"probe_version":2,"source_sha256":shared::hash(&source),"stages":protocol.receipts,"completion":complete,
        "scope":"Three original ordinary TCP responses across actual scoped Q, cumulative QR and Lua Off",
        "final_absent_generation":off.generation().id,"final_absent_original":off_original.design_id,"source_requests_per_viewer":1,"owned_server_and_observer_closed":true,
        "original_scopes_roots_packages_profiles_settings_and_other_channel_unchanged":true,
        "visual_evidence":"Root-owned client frames/navigation must be inspected independently; markers certify no visual match.","limitations":"Representative resource-free warm flow; full engine, resource-cache retirement, panels and platform acceptance remain open."})).unwrap()).unwrap();
}
