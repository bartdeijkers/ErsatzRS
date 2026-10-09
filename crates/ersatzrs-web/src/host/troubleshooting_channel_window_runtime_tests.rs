//! Row201: run the web's complete channel-window adapter against exact opt-in tools.
//! Descriptor assertions accompany, and never substitute for, decoded application output.
use super::*;
use futures_util::FutureExt as _;
use std::{panic::AssertUnwindSafe, path::Path, process::Stdio};
use tokio::{io::AsyncReadExt as _, time::timeout};

#[path = "troubleshooting_channel_window_decode.rs"]
mod decode;
#[path = "troubleshooting_channel_window_evidence.rs"]
mod evidence;
#[path = "troubleshooting_channel_window_fixture.rs"]
mod fixture;
#[path = "troubleshooting_channel_window_handoff.rs"]
mod handoff;
#[path = "troubleshooting_channel_window_origin.rs"]
mod origin;
#[path = "troubleshooting_channel_window_producer.rs"]
mod producer;
#[path = "troubleshooting_channel_window_servers.rs"]
mod servers;

#[derive(Clone, Copy, Debug)]
enum Caption {
    Generated,
    Pinned,
    Custom,
}

#[derive(Clone, Copy, Debug)]
enum Scenario {
    Timeline { normalize: bool },
    Empty,
    HttpLive,
    RtspLive,
    ProducerLive,
    ProducerFinite { caption: Caption },
}

impl Scenario {
    fn timeline(self) -> bool {
        matches!(self, Self::Timeline { .. } | Self::Empty)
    }
    fn normalize(self) -> bool {
        matches!(self, Self::Timeline { normalize: true })
    }
    fn live(self) -> bool {
        matches!(self, Self::HttpLive | Self::RtspLive | Self::ProducerLive)
    }
    fn producer(self) -> bool {
        matches!(self, Self::ProducerLive | Self::ProducerFinite { .. })
    }
    fn finite_producer(self) -> bool {
        matches!(self, Self::ProducerFinite { .. })
    }
    fn pinned(self) -> bool {
        matches!(
            self,
            Self::ProducerFinite {
                caption: Caption::Pinned
            }
        )
    }
    fn custom(self) -> bool {
        matches!(
            self,
            Self::ProducerFinite {
                caption: Caption::Custom
            }
        )
    }
}

#[tokio::test]
#[ignore = "requires exact published Next/FFmpeg; isolated full channel window"]
async fn next_channel_window_historical_mirror_gaps_canvas_remote_and_normalization() {
    isolated("fixture_child_row201_timeline").await;
}

#[tokio::test]
#[ignore = "requires exact published Next/FFmpeg; isolated whole-window fallback"]
async fn next_channel_window_empty_historical_window_decodes_configured_fallback() {
    isolated("fixture_child_row201_empty").await;
}

#[tokio::test]
#[ignore = "requires exact published Next/FFmpeg; direct Live source and captions"]
async fn next_channel_window_live_http_generated_credits_rebase_join_and_cleanup() {
    isolated("fixture_child_row201_http").await;
}

#[tokio::test]
#[ignore = "requires exact published Next/FFmpeg and pinned MediaMTX"]
async fn next_channel_window_live_rtsp_generated_credits_rebase_join_and_cleanup() {
    isolated("fixture_child_row201_rtsp").await;
}

#[tokio::test]
#[ignore = "requires exact published Next/FFmpeg; actual installed Live stdout"]
async fn next_channel_window_live_installed_producer_generated_credits_rebase_join_and_cleanup() {
    isolated("fixture_child_row201_live_producer").await;
}

#[tokio::test]
#[ignore = "requires exact published Next/FFmpeg; actual finite stdout and caption controls"]
async fn next_channel_window_finite_installed_producer_keeps_elapsed_credits_and_pinned_caption() {
    isolated("fixture_child_row201_finite_producer").await;
}

#[tokio::test]
#[ignore = "requires exact published Next/FFmpeg; real admitted Stop and cleanup"]
async fn next_channel_window_stop_revokes_callbacks_and_reaps_owned_runtime() {
    isolated("fixture_child_row201_stop").await;
}

#[tokio::test]
#[ignore = "child of full channel-window runtime test"]
async fn fixture_child_row201_timeline() {
    for normalize in [true, false] {
        execute(Scenario::Timeline { normalize }, false).await;
    }
}
#[tokio::test]
#[ignore = "child of whole-window fallback runtime test"]
async fn fixture_child_row201_empty() {
    execute(Scenario::Empty, false).await;
}
#[tokio::test]
#[ignore = "child of direct Live runtime test"]
async fn fixture_child_row201_http() {
    execute(Scenario::HttpLive, false).await;
}
#[tokio::test]
#[ignore = "child of direct RTSP runtime test"]
async fn fixture_child_row201_rtsp() {
    execute(Scenario::RtspLive, false).await;
}
#[tokio::test]
#[ignore = "child of installed Live runtime test"]
async fn fixture_child_row201_live_producer() {
    execute(Scenario::ProducerLive, false).await;
}
#[tokio::test]
#[ignore = "child of installed finite runtime test"]
async fn fixture_child_row201_finite_producer() {
    for caption in [Caption::Generated, Caption::Pinned, Caption::Custom] {
        execute(Scenario::ProducerFinite { caption }, false).await;
    }
}
#[tokio::test]
#[ignore = "child of actual admitted cancellation runtime test"]
async fn fixture_child_row201_stop() {
    execute(Scenario::ProducerLive, true).await;
}

async fn isolated(test: &str) {
    let root = tempfile::tempdir().unwrap();
    let mut command = Command::new(std::env::current_exe().unwrap());
    command
        .args([
            "--exact",
            &format!("host::troubleshooting::next::tests::channel_window_runtime::{test}"),
            "--ignored",
            "--nocapture",
        ])
        .env(
            ersatzrs_infra::file_system_layout::APP_DATA_DIR_ENV_VAR,
            root.path(),
        )
        .env(
            ersatzrs_infra::file_system_layout::APP_STATE_DIR_ENV_VAR,
            root.path().join("state"),
        )
        .env(
            "ETV_NEXT_FOLDER",
            runtime_path("ERSATZRS_NEXT_TEST_BIN").parent().unwrap(),
        );
    let log_directory = evidence::child_log_directory(test).await;
    match capture_managed(command, Duration::from_mins(10), log_directory.as_deref()).await {
        Ok(output) => assert!(String::from_utf8_lossy(&output).contains("1 passed")),
        Err(error) => {
            let path = root.keep();
            panic!(
                "row201 isolated child failed: {error}; synthetic state retained at {}",
                path.display()
            );
        }
    }
}

async fn execute(scenario: Scenario, stop: bool) {
    let fixture = Fixture::new().await;
    let result = AssertUnwindSafe(execute_fixture(&fixture, scenario, stop))
        .catch_unwind()
        .await;
    if let Err(panic) = result {
        let path = fixture.root.keep();
        eprintln!(
            "row201 failed synthetic fixture retained at {}",
            path.display()
        );
        std::panic::resume_unwind(panic);
    }
}

async fn execute_fixture(fixture: &Fixture, scenario: Scenario, stop: bool) {
    let origin = origin::Origin::start(fixture).await;
    let mut query = fixture::configure(fixture, scenario, &origin).await;
    if matches!(scenario, Scenario::Empty) {
        sqlx::query(r#"DELETE FROM "PlayoutItem""#)
            .execute(&fixture.state.pool)
            .await
            .unwrap();
        query.subtitle_id = None;
    }
    if stop {
        fixture::configure_stop(fixture).await;
    }
    let rtsp = prepare_rtsp(fixture, scenario).await;
    let before = evidence::database(fixture).await;
    let result = AssertUnwindSafe(async {
        if stop {
            handoff::cancel(fixture, &query).await?;
        } else {
            handoff::complete(fixture, &query, scenario).await?;
        }
        Ok::<_, Box<dyn std::error::Error + Send + Sync>>(())
    })
    .catch_unwind()
    .await;
    // All assertion/media decoding happens after the Next adapter's owned process cleanup.
    // External source fixtures are explicitly reaped even if the adapter or decoder failed.
    if let Some(rtsp) = rtsp {
        rtsp.stop().await;
    }
    let captured = origin.captured().await;
    let observation = AssertUnwindSafe(async {
        origin.assert_revoked().await;
        if scenario.producer() {
            producer::assert_stopped(fixture, scenario.live()).await;
        }
        assert_eq!(
            before,
            evidence::database(fixture).await,
            "preview mutated persisted application state"
        );
        evidence::callbacks(&captured, scenario, stop);
    })
    .catch_unwind()
    .await;
    origin.stop().await;
    let finished = if result.as_ref().is_ok_and(Result::is_ok) && observation.is_ok() {
        AssertUnwindSafe(decode_complete(fixture, scenario, stop, &captured, &before))
            .catch_unwind()
            .await
    } else {
        Ok(())
    };
    if result.is_err()
        || result.as_ref().is_ok_and(Result::is_err)
        || observation.is_err()
        || finished.is_err()
    {
        match result {
            Err(panic) => std::panic::resume_unwind(panic),
            Ok(Err(error)) => panic!("adapter failed: {error}"),
            Ok(Ok(())) => {}
        }
        if let Err(panic) = observation {
            std::panic::resume_unwind(panic);
        }
        if let Err(panic) = finished {
            std::panic::resume_unwind(panic);
        }
    }
}

async fn tool(command: Command) -> ApiResult<Vec<u8>> {
    capture_managed(command, Duration::from_mins(3), None).await
}

async fn capture_managed(
    mut command: Command,
    bound: Duration,
    log_directory: Option<&FsPath>,
) -> ApiResult<Vec<u8>> {
    command
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());
    let mut child = ManagedProcess::spawn(command)?;
    let (Some(stdout), Some(stderr)) = (child.take_stdout(), child.take_stderr()) else {
        timeout(Duration::from_secs(10), child.kill())
            .await
            .map_err(|_| "tool cleanup unconfirmed")??;
        return Err("missing synthetic tool pipes".into());
    };
    let mut stdout = stdout.take(32 * 1024 * 1024 + 1);
    let mut stderr = stderr.take(2 * 1024 * 1024 + 1);
    let mut output = Vec::new();
    let mut errors = Vec::new();
    let result = timeout(bound, async {
        tokio::join!(
            child.wait(),
            stdout.read_to_end(&mut output),
            stderr.read_to_end(&mut errors)
        )
    })
    .await;
    let status = if let Ok((status, read_output, read_error)) = result {
        match status {
            Ok(status) => {
                read_output?;
                read_error?;
                status
            }
            Err(error) => {
                timeout(Duration::from_secs(10), child.kill())
                    .await
                    .map_err(|_| "tool cleanup unconfirmed")??;
                evidence::child_logs(log_directory, &output, &errors).await?;
                return Err(error.into());
            }
        }
    } else {
        timeout(Duration::from_secs(10), child.kill())
            .await
            .map_err(|_| "tool cleanup unconfirmed")??;
        evidence::child_logs(log_directory, &output, &errors).await?;
        return Err("bounded synthetic tool timed out".into());
    };
    evidence::child_logs(log_directory, &output, &errors).await?;
    if output.len() > 32 * 1024 * 1024 || errors.len() > 2 * 1024 * 1024 {
        return Err("synthetic tool output exceeded its bound".into());
    }
    if !status.success() {
        return Err(format!(
            "synthetic tool failed ({:?}): {}",
            status.code(),
            String::from_utf8_lossy(&errors[..errors.len().min(4096)])
        )
        .into());
    }
    Ok(output)
}

async fn prepare_rtsp(fixture: &Fixture, scenario: Scenario) -> Option<servers::Rtsp> {
    if matches!(scenario, Scenario::RtspLive) {
        let server = servers::Rtsp::start(
            &runtime_path("ERSATZRS_NEXT_TEST_FFMPEG"),
            &fixture.root.path().join("fixture.mkv"),
            fixture.root.path(),
        )
        .await;
        sqlx::query(r#"UPDATE "RemoteStream" SET "Url"=? WHERE "Id"=10"#)
            .bind(&server.uri)
            .execute(&fixture.state.pool)
            .await
            .unwrap();
        Some(server)
    } else {
        None
    }
}

async fn decode_complete(
    fixture: &Fixture,
    scenario: Scenario,
    stop: bool,
    captured: &[origin::Captured],
    before: &evidence::DatabaseSnapshot,
) {
    if stop {
        evidence::export_stop(fixture, scenario, captured, before).await;
    } else {
        let output = fixture.paths().output;
        let decoded = decode::decode(&output.join("live.m3u8")).await;
        let cadence = if matches!(scenario, Scenario::Empty) {
            decoded.empty();
            None
        } else if scenario.timeline() {
            Some(decoded.timeline(scenario.normalize()))
        } else {
            decoded.credits(scenario);
            None
        };
        evidence::export(fixture, scenario, captured, &decoded, cadence, before).await;
    }
}
