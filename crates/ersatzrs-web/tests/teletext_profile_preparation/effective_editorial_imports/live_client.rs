//! Opt-in cumulative scoped concepts at genuine ordinary live URLs.
//! External display/navigation receipts remain separate from native fixture success.

use super::super::{
    authoring::AuthoringFixture,
    authoring_metadata, channel_documents, client_capture, designs,
    editorial_imports::live_client::{hash, read_bounded, LiveOrigin},
    effective_authoring, effective_generations, ordinary_stream_flow, repeated_editorial_imports,
    scripted_controls, Fixture, ADDON,
};
use super::{export, import, original, stable_state, v3_fixture};
use chrono::{Timelike as _, Utc};
use ersatzrs_app::teletext_design_documents::decode_teletext_effective_design_documents;
use ersatzrs_core::repositories::ChannelRepository as _;
use ersatzrs_core::scheduling::{
    PlayoutBuildMode, SchedulingMediaItem, SchedulingReferenceData, SchedulingRequest,
    ScriptedTeletextControl,
};
use ersatzrs_domain::{
    ChannelSubtitleMode, Playout, PlayoutScheduleKind, StreamingMode, TeletextDesignRevision,
    TeletextEffectiveBindingSnapshot, TeletextOverrideState,
};
use ersatzrs_web::host::{
    teletext_authoring::{
        observe_effective_teletext_authoring_selection, prepare_static_teletext_design,
        preview_original_teletext_design, publish_static_teletext_generation,
        request_static_teletext_publication, TeletextAuthoringIdentity,
        TeletextAuthoringPreviewRequest, TeletextAuthoringPublishTarget, TeletextAuthoringScope,
    },
    teletext_design_exports::ExportedOriginalTeletextDesign,
    teletext_effective_generations::AcceptedEffectiveTeletextOutputCandidate,
};
use futures_util::FutureExt as _;
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use std::{
    collections::BTreeMap,
    io,
    panic::AssertUnwindSafe,
    path::{Path, PathBuf},
    sync::Arc,
    time::Duration,
};

const DIRECTORY_ENV: &str = "ERSATZRS_TELETEXT_SCOPED_LIVE_CLIENT_DIR";
const SOURCE_MAXIMUM: usize = 48 * 1024 * 1024;

struct Flow {
    fixture: AuthoringFixture,
    roots: [TeletextDesignRevision; 2],
    root_exports: [ExportedOriginalTeletextDesign; 2],
    accepted: [AcceptedEffectiveTeletextOutputCandidate; 2],
}

fn two_pages(theme: &str) -> Value {
    let mut first = authoring_metadata::response(theme, 200, 0);
    first["service"]["magazines"][0]["magazine"] = json!(2);
    first["service"]["magazines"][0]["presentation"]["panels"] = json!({
        "left":false,"right":false,"status":"level25_and35","width":0
    });
    let mut second = first.clone();
    let owner = json!({"page":201,"subcode":0});
    second["service"]["pages"][0]["address"] = owner.clone();
    second["service"]["page_enhancements"][0]["address"] = owner.clone();
    second["service"]["mot_selections"][0]["display"] = owner.clone();
    second["authoring"]["pages"][0]["page"] = owner;
    first["service"]["pages"][0]["navigation"][0]["target"]["page"] = json!(201);
    second["service"]["pages"][0]["navigation"][0]["target"]["page"] = json!(200);
    for field in ["pages", "page_enhancements", "mot_selections"] {
        first["service"][field]
            .as_array_mut()
            .unwrap()
            .push(second["service"][field][0].take());
    }
    first["authoring"]["pages"]
        .as_array_mut()
        .unwrap()
        .push(second["authoring"]["pages"][0].take());
    first
}

async fn fixture() -> AuthoringFixture {
    let fixture = v3_fixture().await;
    // All settings and synthetic response changes precede scoped capture.
    ordinary_stream_flow::setting(&fixture.fixture, "teletext.work.max_concurrent", "4").await;
    ordinary_stream_flow::setting(
        &fixture.fixture,
        ersatzrs_domain::CONFIG_KEY_FFMPEG_ACTIVE_TRANSCODE_LIMIT,
        "4",
    )
    .await;
    for theme in ["day", "night"] {
        authoring_metadata::write(&fixture.fixture, theme, &two_pages(theme));
    }
    fixture
}

async fn publish_lua(fixture: &AuthoringFixture, channel: i64, index: usize) {
    let file = fixture
        .fixture
        .root
        .path()
        .join(format!("controls-{channel}.lua"));
    std::fs::write(&file, format!("assert(teletext_profile('{ADDON}',{}).ok)\nassert(teletext_enabled('{ADDON}',true).ok)\nadd_count('clip',4)", fixture.profiles[index].id)).unwrap();
    let start = (Utc::now() - chrono::Duration::seconds(1))
        .with_nanosecond(0)
        .unwrap();
    let request = SchedulingRequest {
        playout: Playout {
            id: 92 + channel,
            channel_id: channel,
            schedule_kind: PlayoutScheduleKind::Scripted,
            schedule_file: Some(file.to_str().unwrap().to_owned()),
            ..Default::default()
        },
        start,
        finish: start + chrono::Duration::minutes(5),
        mode: PlayoutBuildMode::Reset,
        reference_data: SchedulingReferenceData {
            media_by_key: BTreeMap::from([(
                "clip".to_owned(),
                vec![SchedulingMediaItem::new(71, chrono::Duration::seconds(60))],
            )]),
            ..Default::default()
        },
    };
    let controls: Arc<dyn ScriptedTeletextControl> = fixture.fixture.state.application.clone();
    let builder = ersatzrs_infra::scripted_scheduling::playout_builder_with_teletext(
        fixture.fixture.state.pool.clone(),
        tokio::runtime::Handle::current(),
        None,
        Some(controls),
    );
    let result = tokio::task::spawn_blocking(move || builder.build(request))
        .await
        .unwrap()
        .unwrap();
    assert_eq!(result.added_items.len(), 4);
    assert_eq!(result.teletext_intervals.len(), 1);
    assert_eq!(
        result.teletext_intervals[0].finish - result.teletext_intervals[0].start,
        chrono::Duration::minutes(4)
    );
    assert!(result
        .added_items
        .iter()
        .all(|item| item.playout_id == 92 + channel
            && item.media_item_id == 71
            && item.finish - item.start == chrono::Duration::seconds(60)));
    scripted_controls::publish_result(&fixture.fixture, &result).await;
}

async fn prepare(fixture: &AuthoringFixture, channel: i64) -> TeletextAuthoringIdentity {
    let observed = observe_effective_teletext_authoring_selection(
        &fixture.fixture.state.application,
        channel,
        designs::deadline(),
    )
    .await
    .unwrap();
    let mut request = AuthoringFixture::request(channel, observed.selection_digest);
    request.scope = TeletextAuthoringScope::Effective;
    let mut input: Value = serde_json::from_str(&request.input_json).unwrap();
    input["entry"] = json!({"page":200,"subcode":0});
    request.input_json = serde_json::to_string(&input).unwrap();
    let saved = prepare_static_teletext_design(
        &fixture.fixture.state.application,
        request,
        designs::deadline(),
    )
    .await
    .unwrap();
    let preview = saved.preview.unwrap();
    assert_eq!(preview.scope(), TeletextAuthoringScope::Effective);
    assert!(preview.raster().is_some());
    saved.identity
}

async fn publish(fixture: &AuthoringFixture, identity: TeletextAuthoringIdentity) {
    let request = || TeletextAuthoringPreviewRequest {
        channel_id: identity.channel_id,
        design_id: identity.design_id,
        revision: identity.revision,
        owner: Some(channel_documents::address(200, 0)),
    };
    let preview = preview_original_teletext_design(
        &fixture.fixture.state.application,
        request(),
        designs::deadline(),
    )
    .await
    .unwrap();
    let observed = preview.publication().unwrap().clone();
    drop(preview);
    let prior = fixture
        .fixture
        .state
        .application
        .effective_teletext_accepted_generation(identity.channel_id)
        .await
        .unwrap();
    request_static_teletext_publication(
        &fixture.fixture.state.application,
        effective_authoring::target(identity, &observed),
        designs::deadline(),
    )
    .await
    .unwrap();
    assert_eq!(
        fixture
            .fixture
            .state
            .application
            .effective_teletext_accepted_generation(identity.channel_id)
            .await
            .unwrap(),
        prior
    );
    let preview = preview_original_teletext_design(
        &fixture.fixture.state.application,
        request(),
        designs::deadline(),
    )
    .await
    .unwrap();
    let observed = preview.publication().unwrap().clone();
    drop(preview);
    publish_static_teletext_generation(
        &fixture.fixture.state.application,
        TeletextAuthoringPublishTarget {
            publication: effective_authoring::target(identity, &observed),
            expected_generation_id: observed.current_accepted_id,
        },
        designs::deadline(),
    )
    .await
    .unwrap();
}

async fn complete(mut fixture: AuthoringFixture) -> Flow {
    for (index, channel) in [1, 2].into_iter().enumerate() {
        publish_lua(&fixture, channel, index).await;
    }
    let first = prepare(&fixture, 1).await;
    let second = prepare(&fixture, 2).await;
    let roots = [
        original(&fixture.fixture, first).await,
        original(&fixture.fixture, second).await,
    ];
    let root_exports = [
        export(&fixture.fixture, &roots[0]).await,
        export(&fixture.fixture, &roots[1]).await,
    ];
    let calls = fixture.calls();
    let owner = channel_documents::address(200, 0);
    let q = import(
        &fixture.fixture,
        repeated_editorial_imports::request(&root_exports[0], 1, &[(owner, 2, b'Q')]),
    )
    .await;
    let q_identity = TeletextAuthoringIdentity {
        channel_id: 1,
        design_id: q.head().id,
        revision: q.head().revision,
    };
    let q_revision = original(&fixture.fixture, q_identity).await;
    let q_export = export(&fixture.fixture, &q_revision).await;
    let r = import(
        &fixture.fixture,
        repeated_editorial_imports::request(&q_export, 2, &[(owner, 3, b'R')]),
    )
    .await;
    let identity = TeletextAuthoringIdentity {
        channel_id: 1,
        design_id: r.head().id,
        revision: r.head().revision,
    };
    assert_eq!(identity.revision, 3);
    let current = original(&fixture.fixture, identity).await;
    let restored =
        decode_teletext_effective_design_documents(&current.document, designs::budget()).unwrap();
    let root =
        decode_teletext_effective_design_documents(&roots[0].document, designs::budget()).unwrap();
    assert_eq!(restored.selection(), root.selection());
    let overlay = restored.editorial_overlay().unwrap();
    assert_eq!(
        overlay.provenance().export_receipt_id,
        root_exports[0].metadata().id
    );
    assert_eq!(overlay.provenance().original_digest, roots[0].digest);
    assert_eq!(overlay.pages().len(), 1);
    assert_eq!(overlay.pages()[0].rows().len(), 2);
    publish(&fixture, identity).await;
    publish(&fixture, second).await;
    assert_eq!(fixture.calls(), calls);
    let before = [
        recover(&fixture.fixture, 1).await,
        recover(&fixture.fixture, 2).await,
    ];
    fixture.fixture = fixture.fixture.reopen().await;
    let accepted = [
        recover(&fixture.fixture, 1).await,
        recover(&fixture.fixture, 2).await,
    ];
    for (actual, prior) in accepted.iter().zip(&before) {
        same_output(actual, prior);
    }
    assert_eq!(fixture.calls(), calls);
    Flow {
        fixture,
        roots,
        root_exports,
        accepted,
    }
}

async fn recover(fixture: &Fixture, channel: i64) -> AcceptedEffectiveTeletextOutputCandidate {
    effective_generations::recover(fixture, channel)
        .await
        .unwrap()
        .unwrap()
}

fn same_output(
    actual: &AcceptedEffectiveTeletextOutputCandidate,
    expected: &AcceptedEffectiveTeletextOutputCandidate,
) {
    assert_eq!(actual.generation(), expected.generation());
    assert_eq!(actual.selection(), expected.selection());
    assert_eq!(
        designs::units(actual.service().unwrap()),
        designs::units(expected.service().unwrap())
    );
    assert_eq!(
        actual.service().unwrap().magazine_units(),
        expected.service().unwrap().magazine_units()
    );
}

fn channel_facts(flow: &Flow, index: usize) -> Value {
    let accepted = &flow.accepted[index];
    let TeletextEffectiveBindingSnapshot::Scripted(published) = &accepted.selection().bindings[0]
    else {
        panic!("real Scripted origin required")
    };
    let TeletextOverrideState::Enabled(profile) = &published.interval.state else {
        panic!("real enabled profile required")
    };
    let root = &flow.roots[index];
    let scope =
        decode_teletext_effective_design_documents(&root.document, designs::budget()).unwrap();
    assert_eq!(scope.selection(), accepted.selection());
    assert_eq!(published.playout_id, 93 + i64::try_from(index).unwrap());
    let original_valid_until = scope.selection().valid_until.unwrap();
    assert!(original_valid_until > Utc::now() + chrono::Duration::seconds(180));
    let request = &accepted.generation().request;
    json!({"channel_id":request.channel_id,"theme":if index==0 {"day"} else {"night"},
      "addon_id":profile.addon_id,"profile_id":profile.profile_id,"profile_revision":profile.profile_revision,
      "theme_rgb4":if index==0 {[15,0,0]} else {[0,0,15]},"generation_id":accepted.generation().id,"request_id":request.id,
      "design_id":request.design_id,"design_revision":request.revision,"scoped_role":2,
      "original_design_id":root.design_id,"original_revision":root.revision,"original_digest":root.digest,
      "root_receipt_id":flow.root_exports[index].metadata().id,"root_receipt_digest":flow.root_exports[index].metadata().digest,
      "original_valid_until":original_valid_until.to_rfc3339(),"expected_pages":[200,201],"subcode":0,"expected_navigation":[200,201,200],
      "edited_q_page":if index==0 {Some(200)} else {None},"edited_r_page":if index==0 {Some(200)} else {None},
      "edited_q_cell":{"row":2,"column":8},"edited_r_cell":{"row":3,"column":8}})
}

async fn configure_source(fixture: &Fixture, source: &Path) {
    client_capture::configure_source_for_channels(fixture, source, &[]).await;
    let repositories = ersatzrs_infra::repositories::make_repositories(fixture.state.pool.clone());
    for channel in [1, 2] {
        let mut value = repositories
            .channel
            .get_with_profile(channel)
            .await
            .unwrap();
        value.streaming_mode = StreamingMode::TransportStream;
        value.preferred_audio_language_code = Some("eng".to_owned());
        value.subtitle_mode = ChannelSubtitleMode::None;
        repositories.channel.upsert(&value).await.unwrap();
        let changed = sqlx::query(r#"UPDATE "Playout" SET "ScheduleKind"=?,"ScheduleFile"=? WHERE "Id"=? AND "ChannelId"=?"#)
            .bind(PlayoutScheduleKind::Scripted as i32).bind(fixture.root.path().join(format!("controls-{channel}.lua")).to_str().unwrap()).bind(92+channel).bind(channel).execute(&fixture.state.pool).await.unwrap();
        assert_eq!(changed.rows_affected(), 1);
    }
    for statement in [
        r#"INSERT INTO "LocalMediaSource" ("Id") VALUES (71)"#,
        r#"UPDATE "Library" SET "MediaKind"=1,"Name"='Synthetic local movie' WHERE "Id"=71"#,
        r#"INSERT INTO "LocalLibrary" ("Id") VALUES (71)"#,
        r#"UPDATE "MediaItem" SET "State"=0 WHERE "Id"=71"#,
        r#"INSERT INTO "Movie" ("Id") VALUES (71)"#,
        r#"INSERT INTO "MovieMetadata" ("DateAdded","DateUpdated","MetadataKind","MovieId","Title") VALUES ('2026-01-01','2026-01-01',0,71,'Synthetic red video and tone')"#,
        r#"INSERT INTO "MediaVersion" ("Id","DateAdded","DateUpdated","Duration","Height","MovieId","Name","RFrameRate","VideoScanKind","Width") VALUES (71,'2026-01-01','2026-01-01','00:01:00',90,71,'Synthetic','25/1',1,160)"#,
        r#"INSERT INTO "MediaStream" ("Index","Codec","MediaStreamKind","Channels","Default","Forced","MediaVersionId","PixelFormat") VALUES (0,'mpeg2video',1,0,1,0,71,'yuv420p')"#,
        r#"INSERT INTO "MediaStream" ("Index","Codec","MediaStreamKind","Language","Channels","Default","Forced","MediaVersionId") VALUES (1,'aac',2,'eng',1,1,0,71)"#,
    ] {
        sqlx::query(statement)
            .execute(&fixture.state.pool)
            .await
            .unwrap();
    }
    sqlx::query(r#"UPDATE "LibraryPath" SET "Path"=? WHERE "Id"=71"#)
        .bind(fixture.root.path().to_str().unwrap())
        .execute(&fixture.state.pool)
        .await
        .unwrap();
    sqlx::query(r#"INSERT INTO "MediaFile" ("Id","MediaVersionId","Path","PathHash") VALUES (71,71,?,'synthetic-source')"#).bind(source.to_str().unwrap()).execute(&fixture.state.pool).await.unwrap();
    let mut policy: Value = serde_json::from_str(include_str!(
        "../../fixtures/teletext-output-policy-v1.json"
    ))
    .unwrap();
    policy["version"] = json!(2);
    policy["live_updates"] = json!({"poll_interval_ms":25,"recovery_timeout_ms":2000});
    ordinary_stream_flow::setting(
        fixture,
        "teletext.output.policy",
        &serde_json::to_string(&policy).unwrap(),
    )
    .await;
    assert_eq!(
        sqlx::query_scalar::<_, i64>(r#"SELECT COUNT(*) FROM "Playout""#)
            .fetch_one(&fixture.state.pool)
            .await
            .unwrap(),
        2
    );
}

fn write_new(path: &Path, bytes: &[u8]) -> io::Result<()> {
    use std::io::Write as _;
    let temporary = path.with_extension("closed-part");
    let mut file = std::fs::OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(&temporary)?;
    file.write_all(bytes)?;
    drop(file);
    let result = std::fs::hard_link(&temporary, path);
    std::fs::remove_file(&temporary)?;
    result
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

async fn wait_for_client(directory: &Path) -> io::Result<Complete> {
    tokio::time::timeout(Duration::from_mins(3), async {
        loop {
            let path = directory.join("client-complete.json");
            if tokio::fs::try_exists(&path).await? {
                let value: Complete = serde_json::from_slice(&read_bounded(&path, 4096)?)
                    .map_err(|_| io::Error::other("invalid closed scoped-client completion"))?;
                if value.probe_version != 2
                    || value.client_name != "VLC"
                    || value.client_version != "3.0.20"
                    || value.decoder_version != "0.2.42"
                    || value.proof_suffix != "cumulative-effective-live-ordinary"
                {
                    return Err(io::Error::other(
                        "unexpected scoped-client completion identity",
                    ));
                }
                return Ok(value);
            }
            tokio::time::sleep(Duration::from_millis(100)).await;
        }
    })
    .await
    .map_err(|_| io::Error::other("scoped live-client trial exceeded 180 seconds"))?
}

async fn retained_state(fixture: &Fixture) -> Vec<Vec<String>> {
    let mut state = stable_state(fixture).await;
    for statement in [
        r#"SELECT json_array("Key","Value") FROM "ConfigElement" ORDER BY "Key""#,
        r#"SELECT json_array("DesignId","Revision","DocumentVersion","Digest",hex("Document")) FROM "TeletextDesignRevision" ORDER BY "DesignId","Revision""#,
        r#"SELECT json_array("Id","Name","Revision","IsDeleted") FROM "TeletextDesign" ORDER BY "Id""#,
        r#"SELECT json_array("Id","DesignId","Revision","DesignDigest","ExpectedDesignHead","ReceiptVersion","Digest",hex("Receipt")) FROM "TeletextDesignExportReceipt" ORDER BY "Id""#,
    ] {
        state.push(
            sqlx::query_scalar(statement)
                .fetch_all(&fixture.state.pool)
                .await
                .unwrap(),
        );
    }
    state
}

#[tokio::test]
async fn cumulative_scoped_live_client_metadata_preserves_real_role_scope_and_two_edits() {
    let flow = complete(fixture().await).await;
    let day = channel_facts(&flow, 0);
    let night = channel_facts(&flow, 1);
    assert_eq!(day["design_revision"], 3);
    assert_eq!(night["design_revision"], 1);
    assert_eq!(
        day["original_valid_until"],
        flow.root_exports[0]
            .sidecar()
            .project
            .effective_scope
            .as_ref()
            .unwrap()
            .valid_until
            .as_ref()
            .unwrap()
            .as_str()
    );
    assert_eq!(day["expected_navigation"], json!([200, 201, 200]));
    assert_ne!(day["profile_id"], night["profile_id"]);
    assert_eq!(day["addon_id"], night["addon_id"]);
    assert_eq!(night["edited_r_page"], Value::Null);
    flow.fixture.fixture.state.pool.close().await;
}

#[tokio::test]
#[ignore = "Requires pinned FFmpeg 9, two owned headed clients and external root-owned completion"]
async fn cumulative_scoped_originals_serve_live_ordinary_urls() {
    let directory = PathBuf::from(client_capture::selected(DIRECTORY_ENV));
    std::fs::create_dir(&directory).unwrap();
    let fixture = fixture().await;
    let ffmpeg = client_capture::selected("ERSATZRS_TELETEXT_TEST_FFMPEG");
    let ffprobe = client_capture::selected("FFPROBE_PATH");
    client_capture::check_tools(&ffmpeg, &ffprobe).await;
    let source_path =
        client_capture::generate_source_with_duration(&ffmpeg, fixture.fixture.root.path(), 60)
            .await;
    configure_source(&fixture.fixture, &source_path).await;
    let source = read_bounded(&source_path, SOURCE_MAXIMUM).unwrap();
    assert!(!source.is_empty() && source.len().is_multiple_of(188));
    write_new(&directory.join("source.ts"), &source).unwrap();
    let flow = complete(fixture).await;
    assert_eq!(
        ersatzrs_infra::transcode_capacity::load_active_transcode_limit(
            &flow.fixture.fixture.state.pool
        )
        .await
        .unwrap(),
        Some(4)
    );
    let baseline = retained_state(&flow.fixture.fixture).await;
    let calls = flow.fixture.calls();
    let package = ersatzrs_infra::addons::calculate_addon_content_sha256(
        &flow.fixture.fixture.root.path().join("package"),
    )
    .await
    .unwrap();
    let mut origin = LiveOrigin::start(&flow.fixture.fixture).await.unwrap();
    let outcome=AssertUnwindSafe(async {
        let mut channels=[channel_facts(&flow,0),channel_facts(&flow,1)];
        for (index,channel) in channels.iter_mut().enumerate() {channel["url"]=json!(origin.url(i64::try_from(index+1).unwrap()));}
        let ready=json!({"probe_version":2,"scope":"Cumulative effective-role concepts published/reopened at cold actual ordinary URLs; no capture replay",
          "source":{"file":"source.ts","sha256":hash(&source),"bytes":source.len(),"duration_seconds":60},"channels":channels,"active_transcode_limit":4,
          "completion_file":"client-complete.json","completion_timeout_seconds":180,
          "expected_completion":{"probe_version":2,"client_name":"VLC","client_version":"3.0.20","decoder_version":"0.2.42","proof_suffix":"cumulative-effective-live-ordinary"},
          "client_actions":"Open both actual URLs in owned headed clients, navigate 200 to 201 to 200 on each, verify day-only Q and R, then close clients before root-owned completion.",
          "limitations":"Representative cold scoped flow only; full Level 2.5, warm expiry/restart and platform acceptance remain open."});
        write_new(&directory.join("ready.json"),&serde_json::to_vec_pretty(&ready).unwrap())?;
        let marker=wait_for_client(&directory).await?;
        for (index,expected) in flow.accepted.iter().enumerate() {same_output(&recover(&flow.fixture.fixture,i64::try_from(index+1).unwrap()).await,expected);}
        assert_eq!(retained_state(&flow.fixture.fixture).await,baseline);
        assert_eq!(flow.fixture.calls(),calls);
        assert_eq!(ersatzrs_infra::addons::calculate_addon_content_sha256(&flow.fixture.fixture.root.path().join("package")).await.unwrap(),package);
        assert_eq!(read_bounded(&source_path,SOURCE_MAXIMUM)?,source);
        Ok::<_,io::Error>(marker)
    }).catch_unwind().await;
    let closed = origin.close().await;
    flow.fixture.fixture.state.pool.close().await;
    let marker = match outcome {
        Ok(result) => result.expect("scoped live-client trial failed"),
        Err(panic) => std::panic::resume_unwind(panic),
    };
    closed.expect("owned scoped live-client origin did not close");
    write_new(&directory.join("success.json"),&serde_json::to_vec_pretty(&json!({"probe_version":2,"completion":marker,"source_sha256":hash(&source),"source_bytes":source.len(),"roles_origins_expiry_roots_generations_packages_settings_and_producer_calls_unchanged":true,"owned_server_closed":true,"headed_evidence":"Root-owned external display/navigation receipts required; marker alone is not full engine acceptance."})).unwrap()).unwrap();
}
