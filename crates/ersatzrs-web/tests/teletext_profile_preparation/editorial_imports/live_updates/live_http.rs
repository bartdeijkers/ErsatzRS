//! Explicit opt-in ordinary Router responses backed by the real `FFmpeg` source.
//! Three already-open bodies observe updates; no new request replaces a viewer.
//! This is native application/transport evidence, not headed-client acceptance.

use super::super::super::{client_capture, ordinary_stream_flow};
use super::*;
use axum::{body::Body, http::Request};
use ersatzrs_app::commands::SetAdminSetting;
use ersatzrs_web::host::{router::build_app, teletext_work::MAX_CONCURRENT_SETTING};
use futures_util::{FutureExt as _, StreamExt as _};
use serde_json::{json, Value};
use std::panic::AssertUnwindSafe;
use tokio::io::AsyncWriteExt as _;
use tokio_util::sync::CancellationToken;
use tower::ServiceExt as _;

struct HttpPumps {
    stop: CancellationToken,
    tasks: Vec<JoinHandle<()>>,
}

impl HttpPumps {
    fn new() -> Self {
        Self {
            stop: CancellationToken::new(),
            tasks: Vec::new(),
        }
    }

    async fn start(&mut self, fixture: &Fixture, channel: i64, mut capture: Capture) {
        let response = build_app(fixture.state.clone())
            .oneshot(
                Request::builder()
                    .uri(format!("/iptv/hls-direct/{channel}.ts"))
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(response.status(), axum::http::StatusCode::OK);
        let stop = self.stop.clone();
        self.tasks.push(tokio::spawn(async move {
            let mut body = response.into_body().into_data_stream();
            loop {
                let next = tokio::select! { biased;
                    () = stop.cancelled() => break,
                    next = body.next() => next,
                };
                let Some(chunk) = next else {
                    break;
                };
                capture
                    .write_all(&chunk.expect("ordinary updated source failed"))
                    .await
                    .unwrap();
            }
            // Dropping this original body invokes the production stream guard;
            // no replacement HTTP request or synthetic terminal state is used.
        }));
    }

    async fn close(&mut self) {
        self.stop.cancel();
        for task in &mut self.tasks {
            tokio::time::timeout(Duration::from_secs(5), task)
                .await
                .expect("ordinary response observer did not stop")
                .unwrap();
        }
    }
}

impl Drop for HttpPumps {
    fn drop(&mut self) {
        self.stop.cancel();
        for task in &self.tasks {
            task.abort();
        }
    }
}

async fn setting(fixture: &Fixture, key: &'static str, value: String) {
    fixture
        .state
        .application
        .set_admin_setting(SetAdminSetting { key, value })
        .await
        .unwrap();
}

async fn configured_source(ffmpeg: &str) -> (Fixture, Vec<u8>) {
    let fixture = Fixture::new_persistent().await;
    let source =
        client_capture::generate_source_with_duration(ffmpeg, fixture.root.path(), 60).await;
    client_capture::configure_source_for_channels(&fixture, &source, &[1, 2]).await;
    let bytes = tokio::fs::read(&source).await.unwrap();
    assert!(!bytes.is_empty() && bytes.len() <= MAX_CAPTURE && bytes.len().is_multiple_of(188));
    let mut policy: Value = serde_json::from_str(include_str!(
        "../../../fixtures/teletext-output-policy-v1.json"
    ))
    .unwrap();
    policy["version"] = json!(2);
    policy["live_updates"] = json!({"poll_interval_ms":25,"recovery_timeout_ms":2000});
    setting(
        &fixture,
        "teletext.output.policy",
        serde_json::to_string(&policy).unwrap(),
    )
    .await;
    // These three actual response owners require three existing capacity slots;
    // admission slates are intentionally outside this source-update scenario.
    setting(
        &fixture,
        ersatzrs_domain::CONFIG_KEY_FFMPEG_ACTIVE_TRANSCODE_LIMIT,
        "3".to_owned(),
    )
    .await;
    // Three response monitors plus the explicit publication operator require
    // four bounded native work owners; this does not change product defaults.
    setting(&fixture, MAX_CONCURRENT_SETTING, "4".to_owned()).await;
    (fixture, bytes)
}

async fn wait_for_owned_cleanup() {
    tokio::time::timeout(Duration::from_secs(5), async {
        while ersatzrs_infra::transcode_capacity::active_transcode_snapshot(3).active != 0 {
            tokio::time::sleep(Duration::from_millis(20)).await;
        }
    })
    .await
    .expect("ordinary response producers did not release their owned capacity");
}

async fn verify_native(tools: (&str, &str, &str), source: &[u8], views: &[Capture; 3]) {
    let (ffmpeg, ffprobe, tsanalyze) = tools;
    for (index, capture) in views.iter().enumerate() {
        let mut bytes = capture.bytes();
        // ReaderStream chunks are not TS packet boundaries. Keep every complete
        // received packet for the independent probes, without repairing bytes.
        bytes.truncate(bytes.len() / 188 * 188);
        assert!(bytes.len() >= 188 * 1000);
        client_capture::verify_transport(tsanalyze, ffprobe, &bytes).await;
        client_capture::verify_av(ffmpeg, source, &bytes).await;
        let text = client_capture::decode_page_text(ffmpeg, &bytes, "124").await;
        assert!(
            text.contains("AZ"),
            "actual decoder lost original enhanced source glyphs"
        );
        assert_eq!(
            text.contains('Q'),
            index != 2,
            "actual decoder mixed channel-specific concept edits"
        );
    }
}

#[tokio::test]
#[ignore = "requires explicitly selected exact FFmpeg/ffprobe9 and TSDuck; actual ordinary three-viewer update proof is a separate native gate"]
async fn three_open_ordinary_responses_observe_on_replace_off_reenable_and_missing_profile() {
    let ffmpeg = client_capture::selected("ERSATZRS_TELETEXT_TEST_FFMPEG");
    let ffprobe = client_capture::selected("FFPROBE_PATH");
    let tsanalyze = client_capture::selected("ERSATZRS_TELETEXT_TEST_TSANALYZE");
    client_capture::check_tools(&ffmpeg, &ffprobe).await;
    let (fixture, source) = configured_source(&ffmpeg).await;
    let (mut fixture, flow) =
        Box::pin(setup_with_fixture(authoring_metadata::response, fixture)).await;
    let off = disable(&fixture, 1).await;
    fixture = fixture.reopen().await;
    ordinary_stream_flow::reset_source_start(&fixture).await;
    let views = [Capture::new(), Capture::new(), Capture::new()];
    let mut pumps = HttpPumps::new();
    let outcome = AssertUnwindSafe(async {
        for (index, capture) in views.iter().enumerate() {
            pumps
                .start(&fixture, if index == 2 { 2 } else { 1 }, capture.clone())
                .await;
        }
        scenario(&fixture, &flow, &views, off).await;
    })
    .catch_unwind()
    .await;
    let stopped = AssertUnwindSafe(async {
        pumps.close().await;
        wait_for_owned_cleanup().await;
    })
    .catch_unwind()
    .await;
    fixture.state.pool.close().await;
    if let Err(panic) = outcome {
        std::panic::resume_unwind(panic);
    }
    if let Err(panic) = stopped {
        std::panic::resume_unwind(panic);
    }
    assert!(fixture.playout.try_recv().is_err());
    verify_native((&ffmpeg, &ffprobe, &tsanalyze), &source, &views).await;
}
