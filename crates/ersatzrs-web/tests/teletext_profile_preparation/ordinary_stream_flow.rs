//! Actual ordinary Router HTTP to accepted synthetic services and source AV.
//! Opt-in native tools prove this scoped flow, not full engine/client acceptance.

use super::{channel_documents, client_capture, designs, documents::DocumentFixture, *};
use axum::{body::Body, http::Request};
use ersatzrs_app::commands::SetAdminSetting;
use ersatzrs_core::repositories::{TeletextGenerationMutation, TeletextPublicationMutation};
use ersatzrs_domain::teletext::{TeletextColourPixelOptions, TeletextFlashPhase};
use ersatzrs_ffmpeg::teletext::{
    source_assembly::{SuppliedTeletextAssembler, TeletextAssemblyEvent},
    EncodedTeletextAuthoredService, TeletextCarousel, TeletextCarouselBatchKind,
};
use ersatzrs_web::host::{
    router::build_app,
    teletext_channel_documents::prepare_channel_teletext_document_candidate,
    teletext_designs::request_original_teletext_design_publication,
    teletext_generations::{
        accept_original_teletext_generation, reload_accepted_teletext_generation,
        AcceptedTeletextOutputCandidate, TeletextGenerationTarget,
    },
    teletext_preview::project_teletext_preview,
    teletext_work::{begin, MAX_CONCURRENT_SETTING},
};
use serde_json::{json, Value};
use std::{collections::BTreeSet, path::PathBuf, sync::Arc};
use tokio::io::AsyncWriteExt as _;
use tower::ServiceExt as _;

const POLICY_KEY: &str = "teletext.output.policy";
const POLICY: &str = include_str!("../fixtures/teletext-output-policy-v1.json");

#[path = "ordinary_stream_flow/standalone_base.rs"]
mod standalone_base;
const PROFILE_SCRIPT: &str = r#"test "$1" = pages || exit 9
 test "$ERSATZRS_ADDON_CAPABILITY" = teletext.pages.v2 || exit 10
 theme="$ERSATZRS_ADDON_SETTING_THEME"
 case "$theme" in day|night) ;; *) exit 11;; esac
 cat > "../ordinary-input-$theme.json"
 printf '%s\n' "$theme" >> ../ordinary-calls
 cat "../ordinary-response-$theme.json"
"#;

pub(super) struct Tools {
    pub(super) ffmpeg: String,
    pub(super) ffprobe: String,
    pub(super) tsanalyze: String,
}

impl Tools {
    pub(super) async fn load() -> Self {
        let tools = Self {
            ffmpeg: client_capture::selected("ERSATZRS_TELETEXT_TEST_FFMPEG"),
            ffprobe: client_capture::selected("FFPROBE_PATH"),
            tsanalyze: client_capture::selected("ERSATZRS_TELETEXT_TEST_TSANALYZE"),
        };
        client_capture::check_tools(&tools.ffmpeg, &tools.ffprobe).await;
        tools
    }
}

pub(super) async fn setting(fixture: &Fixture, key: &'static str, value: &str) {
    fixture
        .state
        .application
        .set_admin_setting(SetAdminSetting {
            key,
            value: value.to_owned(),
        })
        .await
        .unwrap();
}

/// Configure the existing ordinary-route policy after sibling fixture source
/// setup. Capturing two isolated channels uses the same bounded work admission.
pub(super) async fn enable_output_policy(fixture: &Fixture) {
    setting(fixture, POLICY_KEY, POLICY).await;
    setting(fixture, MAX_CONCURRENT_SETTING, "2").await;
}

fn themed_pages(theme: &str) -> Value {
    let mut document = channel_documents::response(DocumentFixture::Authored, theme, 200, 0, "nld");
    document["magazines"][0]["magazine"] = json!(2);
    document["magazines"][0]["presentation"]["panels"] = json!({
        "left":false,"right":false,"status":"level25_and35","width":0
    });
    let mut second = document.clone();
    let owner = json!({"page":201,"subcode":0});
    second["pages"][0]["address"] = owner.clone();
    second["page_enhancements"][0]["address"] = owner.clone();
    second["mot_selections"][0]["display"] = owner;
    document["pages"][0]["navigation"][0]["target"]["page"] = json!(201);
    second["pages"][0]["navigation"][0]["target"]["page"] = json!(200);
    for field in ["pages", "page_enhancements", "mot_selections"] {
        document[field]
            .as_array_mut()
            .unwrap()
            .push(second[field][0].take());
    }
    document
}

pub(super) async fn installed_profiles(tools: &Tools) -> (Fixture, Vec<u8>) {
    installed_profiles_with(tools, themed_pages, 12).await
}

async fn installed_profiles_with(
    tools: &Tools,
    response: fn(&str) -> Value,
    seconds: u8,
) -> (Fixture, Vec<u8>) {
    let fixture = Fixture::new_persistent().await;
    channel_documents::install_contract(&fixture, ADDON, "first", 2).await;
    tokio::fs::write(
        fixture.root.path().join("package/fixture.sh"),
        PROFILE_SCRIPT,
    )
    .await
    .unwrap();
    let hash = calculate_addon_content_sha256(&fixture.root.path().join("package"))
        .await
        .unwrap();
    sqlx::query(
        r#"UPDATE "AddonInstallation" SET "ContentSha256"=?,"PackageSha256"=? WHERE "AddonId"=?"#,
    )
    .bind(hash)
    .bind("a".repeat(64))
    .bind(ADDON)
    .execute(&fixture.state.pool)
    .await
    .unwrap();
    for (channel, theme) in [(1, "day"), (2, "night")] {
        let document = response(theme);
        serde_json::from_value::<ersatzrs_addon_contract::TeletextPagesV2Response>(
            document.clone(),
        )
        .unwrap();
        tokio::fs::write(
            fixture
                .root
                .path()
                .join(format!("ordinary-response-{theme}.json")),
            serde_json::to_vec(&document).unwrap(),
        )
        .await
        .unwrap();
        let profile = fixture.profile(theme, theme, "good").await;
        fixture.bind(channel, &profile, None).await;
    }
    let source =
        client_capture::generate_source_with_duration(&tools.ffmpeg, fixture.root.path(), seconds)
            .await;
    client_capture::configure_source_for_channels(&fixture, &source, &[1, 2]).await;
    // The generated clip remains finite; the schedule remains selectable
    // through these sequential fallback trials without becoming a new source.
    let schedule = fixture.root.path().join("synthetic-client-schedule.json");
    let mut body: Value =
        serde_json::from_slice(&tokio::fs::read(&schedule).await.unwrap()).unwrap();
    body["programs"][0]["duration"] = json!(600_000);
    tokio::fs::write(schedule, serde_json::to_vec(&body).unwrap())
        .await
        .unwrap();
    setting(&fixture, MAX_CONCURRENT_SETTING, "2").await;
    let bytes = tokio::fs::read(source).await.unwrap();
    (fixture, bytes)
}

async fn pending(fixture: &Fixture, channel: i64) -> ersatzrs_domain::TeletextPublicationRequest {
    let candidate = prepare_channel_teletext_document_candidate(
        &fixture.state.application,
        channel,
        Some(ADDON),
        &guide(&[]),
        channel_documents::policy(),
    )
    .await
    .unwrap();
    let saved = designs::save(fixture, candidate, None).await;
    let result = request_original_teletext_design_publication(
        &fixture.state.application,
        saved.head().id,
        saved.head().revision,
        saved.digest().to_owned(),
        saved.head().revision,
        None,
        designs::budget(),
        channel_documents::policy(),
        designs::deadline(),
    )
    .await
    .unwrap();
    let TeletextPublicationMutation::Requested(requested) = result else {
        panic!("synthetic original pages must create pending intent");
    };
    assert_eq!(requested.request().channel_id, channel);
    let original = saved.candidate().encoded().unwrap();
    let replay = requested.design().encoded().unwrap();
    assert_eq!(designs::units(original), designs::units(replay));
    assert_eq!(original.magazine_units(), replay.magazine_units());
    for page in [200, 201] {
        assert_eq!(raster(original, page), raster(replay, page));
    }
    requested.request().clone()
}

pub(super) async fn accept(fixture: &Fixture, channel: i64) -> AcceptedTeletextOutputCandidate {
    let request = pending(fixture, channel).await;
    let result = accept_original_teletext_generation(
        &fixture.state.application,
        TeletextGenerationTarget {
            expected_design_head: request.revision,
            request,
            expected_generation_id: None,
        },
        designs::budget(),
        channel_documents::policy(),
        designs::deadline(),
    )
    .await
    .unwrap();
    let TeletextGenerationMutation::Accepted(accepted) = result else {
        panic!("reviewed synthetic originals must explicitly accept");
    };
    accepted
}

async fn reload(fixture: &Fixture, channel: i64) -> AcceptedTeletextOutputCandidate {
    reload_accepted_teletext_generation(
        &fixture.state.application,
        channel,
        designs::budget(),
        channel_documents::policy(),
        designs::deadline(),
    )
    .await
    .unwrap()
    .unwrap()
}

pub(super) fn raster(
    service: &EncodedTeletextAuthoredService,
    page: u16,
) -> ersatzrs_ffmpeg::teletext::TeletextRaster {
    let preview = project_teletext_preview(
        service,
        channel_documents::address(page, 0),
        TeletextColourPixelOptions {
            reveal_concealed: false,
            flash_phase: TeletextFlashPhase::Original,
        },
        client_capture::preview_budget(),
    )
    .unwrap();
    assert!(preview.raster().diagnostics().is_empty());
    assert_eq!(preview.raster().width(), 40 * 12);
    assert_eq!(
        preview.navigation()[0].target.page.get(),
        if page == 200 { 201 } else { 200 }
    );
    preview.raster().clone()
}

pub(super) async fn reset_source_start(fixture: &Fixture) {
    let path = fixture.root.path().join("synthetic-client-schedule.json");
    let mut body: Value = serde_json::from_slice(&tokio::fs::read(&path).await.unwrap()).unwrap();
    body["startTime"] = json!(Utc::now().to_rfc3339());
    tokio::fs::write(path, serde_json::to_vec(&body).unwrap())
        .await
        .unwrap();
}

pub(super) async fn capture(fixture: &Fixture, channel: i64) -> Vec<u8> {
    let response = build_app(fixture.state.clone())
        .oneshot(
            Request::builder()
                .uri(format!("/iptv/hls-direct/{channel}.ts"))
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    client_capture::capture_response(response).await
}

fn emitted_units(bytes: &[u8]) -> BTreeSet<[u8; 46]> {
    let mut assembler = SuppliedTeletextAssembler::new(512).unwrap();
    let mut observed = BTreeSet::new();
    for packet in bytes.chunks_exact(188) {
        if let TeletextAssemblyEvent::Complete(pes) =
            assembler.push(packet.try_into().unwrap()).unwrap()
        {
            observed.extend(pes.units().filter_map(|unit| unit.teletext_unit().copied()));
        }
    }
    assembler.finish().unwrap();
    observed
}

pub(super) fn assert_emitted_model(bytes: &[u8], service: &Arc<EncodedTeletextAuthoredService>) {
    let mut expected: BTreeSet<_> = service
        .pages()
        .iter()
        .flat_map(|page| page.data_units().iter())
        .chain(service.magazine_units().iter())
        .copied()
        .collect();
    // The immutable pages retain their first-cycle C4 erase headers. Actual
    // transmission also carries repeat headers and the reserved mFF terminator.
    let mut carousel = TeletextCarousel::new_authored_shared(Arc::clone(service)).unwrap();
    for _ in 0..2 {
        loop {
            let batch = carousel.next_batch().unwrap();
            expected.extend(batch.data_units().iter().copied());
            if batch.kind() == TeletextCarouselBatchKind::Terminator {
                break;
            }
        }
    }
    let actual = emitted_units(bytes);
    let unexpected: Vec<_> = actual
        .difference(&expected)
        .map(|unit| format!("{unit:02x?}"))
        .collect();
    let missing: Vec<_> = expected
        .difference(&actual)
        .map(|unit| format!("{unit:02x?}"))
        .collect();
    assert!(
        actual == expected,
        "generation units differ: unexpected {unexpected:?}; missing {missing:?}"
    );
}

pub(super) async fn verify_authored(
    tools: &Tools,
    source: &[u8],
    bytes: &[u8],
    service: &Arc<EncodedTeletextAuthoredService>,
) {
    client_capture::verify_transport(&tools.tsanalyze, &tools.ffprobe, bytes).await;
    client_capture::verify_av(&tools.ffmpeg, source, bytes).await;
    assert_emitted_model(bytes, service);
    for page in ["200", "201"] {
        client_capture::decode_page(&tools.ffmpeg, bytes, page).await;
    }
}

pub(super) async fn verify_plain(tools: &Tools, source: &[u8], bytes: &[u8]) {
    assert!(bytes
        .chunks_exact(188)
        .all(|packet| ((u16::from(packet[1] & 0x1f) << 8) | u16::from(packet[2])) != 512));
    let report: Value = serde_json::from_slice(
        &client_capture::probe(&tools.tsanalyze, &["--json"], bytes, 262_144).await,
    )
    .unwrap();
    assert_eq!(
        report["ts"]["packets"]["total"].as_u64().unwrap(),
        u64::try_from(bytes.len() / 188).unwrap()
    );
    for field in ["invalid-syncs", "transport-errors"] {
        assert_eq!(report["ts"]["packets"][field], 0);
    }
    assert_eq!(report["ts"]["pids"]["unreferenced"], 0);
    for pid in report["pids"].as_array().unwrap() {
        for field in ["discontinuities", "pts-leap", "pcr-leap"] {
            assert_eq!(pid["packets"][field], 0);
        }
    }
    client_capture::verify_av(&tools.ffmpeg, source, bytes).await;
}

async fn export_capture(channel: i64, bytes: &[u8]) {
    let path = PathBuf::from(format!(
        "{}.channel-{channel}.ts",
        client_capture::selected("ERSATZRS_TELETEXT_CLIENT_CAPTURE")
    ));
    let mut file = tokio::fs::OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(path)
        .await
        .unwrap();
    file.write_all(bytes).await.unwrap();
    file.sync_all().await.unwrap();
}

/// Validate captures made through the real served host and native admin flow.
/// The independent expected fixture supplies the same redistributable model;
/// this test does not generate or replace the supplied HTTP capture bytes.
#[tokio::test]
#[ignore = "requires actual served-host captures plus pinned FFmpeg/ffprobe9 and TSDuck"]
async fn served_admin_publication_captures_preserve_exact_model_and_source_av() {
    let tools = Tools::load().await;
    let prefix = client_capture::selected("ERSATZRS_TELETEXT_SERVED_CAPTURE");
    let (fixture, _) = installed_profiles(&tools).await;
    let source = tokio::fs::read(format!("{prefix}.source.ts"))
        .await
        .unwrap();
    let accepted = [accept(&fixture, 1).await, accept(&fixture, 2).await];
    for (index, suffix) in [
        (0, "day"),
        (1, "night"),
        (0, "recovered"),
        (1, "other-channel"),
    ] {
        let delivered = tokio::fs::read(format!("{prefix}.{suffix}.ts"))
            .await
            .unwrap();
        verify_authored(&tools, &source, &delivered, accepted[index].service()).await;
    }
    for suffix in ["pending", "invalid-policy", "unavailable-profile"] {
        let delivered = tokio::fs::read(format!("{prefix}.{suffix}.ts"))
            .await
            .unwrap();
        verify_plain(&tools, &source, &delivered).await;
    }
}

/// Validate independently captured streams on both sides of a real host restart.
/// Reuse the same expected authored models without generating capture bytes or
/// treating transport and decoder checks as full client or engine acceptance.
#[tokio::test]
#[ignore = "requires actual served-host restart captures plus pinned FFmpeg/ffprobe9 and TSDuck"]
async fn served_admin_restart_captures_preserve_exact_model_and_source_av() {
    let tools = Tools::load().await;
    let prefix = client_capture::selected("ERSATZRS_TELETEXT_RESTART_CAPTURE");
    let (fixture, _) = installed_profiles(&tools).await;
    let source = tokio::fs::read(format!("{prefix}.source.ts"))
        .await
        .unwrap();
    let accepted = [accept(&fixture, 1).await, accept(&fixture, 2).await];
    for (index, suffix) in [
        (0, "before.day"),
        (1, "before.night"),
        (0, "after.day"),
        (1, "after.night"),
    ] {
        let delivered = tokio::fs::read(format!("{prefix}.{suffix}.ts"))
            .await
            .unwrap();
        verify_authored(&tools, &source, &delivered, accepted[index].service()).await;
    }
}

#[tokio::test]
#[ignore = "requires pinned FFmpeg/ffprobe9,TSDuck and explicit create-new capture prefix; headed client trial remains separate"]
async fn ordinary_http_two_channels_share_one_addon_with_isolated_accepted_themes() {
    let tools = Tools::load().await;
    let (mut fixture, source) = installed_profiles(&tools).await;
    let originals = [accept(&fixture, 1).await, accept(&fixture, 2).await];
    assert_ne!(originals[0].generation().id, originals[1].generation().id);
    assert_eq!(originals[0].selection().bindings[0].addon_id, ADDON);
    assert_eq!(originals[1].selection().bindings[0].addon_id, ADDON);
    assert_ne!(
        originals[0].selection().bindings[0].profile_id,
        originals[1].selection().bindings[0].profile_id
    );
    assert_ne!(
        originals[0].service().magazine_units(),
        originals[1].service().magazine_units()
    );
    assert_ne!(
        raster(originals[0].service(), 200),
        raster(originals[1].service(), 200)
    );
    setting(&fixture, POLICY_KEY, POLICY).await;
    let before = designs::config(&fixture).await;
    let calls = tokio::fs::read(fixture.root.path().join("ordinary-calls"))
        .await
        .unwrap();
    assert_eq!(calls, b"day\nnight\n");
    fixture = fixture.reopen().await;
    for (channel, original) in [1, 2].into_iter().zip(&originals) {
        let restored = reload(&fixture, channel).await;
        assert_eq!(restored.generation(), original.generation());
        assert_eq!(
            designs::units(restored.service()),
            designs::units(original.service())
        );
        assert_eq!(
            raster(restored.service(), 200),
            raster(original.service(), 200)
        );
    }
    reset_source_start(&fixture).await;
    let (first, second) = tokio::join!(capture(&fixture, 1), capture(&fixture, 2));
    verify_authored(&tools, &source, &first, originals[0].service()).await;
    verify_authored(&tools, &source, &second, originals[1].service()).await;
    assert_eq!(designs::config(&fixture).await, before);
    assert_eq!(
        tokio::fs::read(fixture.root.path().join("ordinary-calls"))
            .await
            .unwrap(),
        calls
    );
    assert!(fixture.playout.try_recv().is_err());
    export_capture(1, &first).await;
    export_capture(2, &second).await;
    fixture.state.pool.close().await;
}

#[tokio::test]
#[ignore = "requires pinned FFmpeg/ffprobe9 and TSDuck; scoped ordinary HTTP fallback proof"]
async fn ordinary_http_missing_policy_and_pending_only_preserve_source_av() {
    let tools = Tools::load().await;
    let (mut fixture, source) = installed_profiles(&tools).await;
    let original = accept(&fixture, 1).await;
    let before = designs::config(&fixture).await;
    let calls = tokio::fs::read(fixture.root.path().join("ordinary-calls"))
        .await
        .unwrap();
    reset_source_start(&fixture).await;
    verify_plain(&tools, &source, &capture(&fixture, 1).await).await;
    assert_eq!(designs::config(&fixture).await, before);
    assert_eq!(
        reload(&fixture, 1).await.generation(),
        original.generation()
    );
    let request = pending(&fixture, 2).await;
    setting(&fixture, POLICY_KEY, POLICY).await;
    let before = designs::config(&fixture).await;
    let pending_calls = tokio::fs::read(fixture.root.path().join("ordinary-calls"))
        .await
        .unwrap();
    assert_ne!(pending_calls, calls);
    reset_source_start(&fixture).await;
    verify_plain(&tools, &source, &capture(&fixture, 2).await).await;
    assert!(fixture
        .state
        .application
        .accepted_teletext_generation(2)
        .await
        .unwrap()
        .is_none());
    assert_eq!(
        fixture
            .state
            .application
            .teletext_publication_request(2)
            .await
            .unwrap(),
        Some(request)
    );
    assert_eq!(designs::config(&fixture).await, before);
    assert_eq!(
        tokio::fs::read(fixture.root.path().join("ordinary-calls"))
            .await
            .unwrap(),
        pending_calls
    );
    assert!(fixture.playout.try_recv().is_err());
    fixture.state.pool.close().await;
}

#[tokio::test]
#[ignore = "requires pinned FFmpeg/ffprobe9 and TSDuck; scoped ordinary HTTP fallback proof"]
async fn ordinary_http_invalid_exhausted_and_busy_optional_work_preserve_source_av() {
    let tools = Tools::load().await;
    let (mut fixture, source) = installed_profiles(&tools).await;
    let original = accept(&fixture, 1).await;
    let calls = tokio::fs::read(fixture.root.path().join("ordinary-calls"))
        .await
        .unwrap();
    let mut exhausted: Value = serde_json::from_str(POLICY).unwrap();
    exhausted["encoding"]["max_display_updates"] = json!(1);
    let mut timed: Value = serde_json::from_str(POLICY).unwrap();
    timed["recovery_timeout_ms"] = json!(1);
    for policy in [
        "{".to_owned(),
        serde_json::to_string(&exhausted).unwrap(),
        serde_json::to_string(&timed).unwrap(),
    ] {
        setting(&fixture, POLICY_KEY, &policy).await;
        let before = designs::config(&fixture).await;
        reset_source_start(&fixture).await;
        verify_plain(&tools, &source, &capture(&fixture, 1).await).await;
        assert_eq!(designs::config(&fixture).await, before);
        assert_eq!(
            fixture
                .state
                .application
                .accepted_teletext_generation(1)
                .await
                .unwrap(),
            Some(original.generation().clone())
        );
    }
    setting(&fixture, POLICY_KEY, POLICY).await;
    let before = designs::config(&fixture).await;
    let first = begin(&fixture.state.application, None).await.unwrap();
    let second = begin(&fixture.state.application, None).await.unwrap();
    reset_source_start(&fixture).await;
    verify_plain(&tools, &source, &capture(&fixture, 1).await).await;
    drop(first);
    drop(second);
    let restored = reload(&fixture, 1).await;
    assert_eq!(restored.generation(), original.generation());
    reset_source_start(&fixture).await;
    let bytes = capture(&fixture, 1).await;
    verify_authored(&tools, &source, &bytes, original.service()).await;
    let disabled = fixture
        .state
        .application
        .disable_channel_teletext_binding(
            1,
            ADDON,
            original.selection().bindings[0].binding_revision,
        )
        .await
        .unwrap();
    assert!(matches!(disabled, TeletextProfileMutation::Saved(_)));
    reset_source_start(&fixture).await;
    verify_plain(&tools, &source, &capture(&fixture, 1).await).await;
    assert_eq!(
        fixture
            .state
            .application
            .accepted_teletext_generation(1)
            .await
            .unwrap(),
        Some(original.generation().clone())
    );
    assert_eq!(designs::config(&fixture).await, before);
    assert_eq!(
        tokio::fs::read(fixture.root.path().join("ordinary-calls"))
            .await
            .unwrap(),
        calls
    );
    assert!(fixture.playout.try_recv().is_err());
    fixture.state.pool.close().await;
}
