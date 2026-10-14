//! Opt-in cold headed-client trial against actual ordinary ErsatzRS URLs.
//! This owns disposable source/server state, never a finite capture replay.
//! External headed evidence and warm transitions remain separate acceptance.

use super::super::{client_capture, ordinary_stream_flow};
use super::*;
use axum::{
    body::Body,
    extract::{Request, State},
    middleware::{self, Next},
    response::Response,
};
use futures_util::{FutureExt as _, StreamExt as _};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use sha2::Digest as _;
use std::{
    io,
    net::SocketAddr,
    panic::AssertUnwindSafe,
    path::{Path, PathBuf},
};
use tokio::{net::TcpListener, task::JoinHandle};
use tokio_util::sync::CancellationToken;

const DIRECTORY_ENV: &str = "ERSATZRS_TELETEXT_LIVE_CLIENT_DIR";
const SOURCE_MAXIMUM: usize = 48 * 1024 * 1024;
const COMPLETION_MAXIMUM: usize = 4096;
const CLIENT_TIMEOUT: Duration = Duration::from_mins(3);

#[derive(Debug, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
struct ClientComplete {
    probe_version: u8,
    client_name: String,
    client_version: String,
    decoder_version: String,
    proof_suffix: String,
}

impl ClientComplete {
    fn validate(&self) -> io::Result<()> {
        if self.probe_version != 1
            || self.client_name != "VLC"
            || self.client_version != "3.0.20"
            || self.decoder_version != "0.2.42"
            || self.proof_suffix != "live-ordinary"
        {
            return Err(io::Error::other(
                "unexpected live-client completion identity",
            ));
        }
        Ok(())
    }
}

fn hash(bytes: &[u8]) -> String {
    use std::fmt::Write as _;
    let mut value = String::with_capacity(64);
    for byte in sha2::Sha256::digest(bytes) {
        write!(value, "{byte:02x}").unwrap();
    }
    value
}

fn read_bounded(path: &Path, maximum: usize) -> io::Result<Vec<u8>> {
    use std::io::Read as _;
    let metadata = std::fs::symlink_metadata(path)?;
    if !metadata.file_type().is_file() || metadata.len() > u64::try_from(maximum).unwrap() {
        return Err(io::Error::other(
            "live-client input is not a bounded regular file",
        ));
    }
    let mut bytes = Vec::new();
    std::fs::File::open(path)?
        .take(u64::try_from(maximum).unwrap() + 1)
        .read_to_end(&mut bytes)?;
    if bytes.len() > maximum {
        return Err(io::Error::other(
            "live-client input exceeded its read bound",
        ));
    }
    Ok(bytes)
}

fn write_new(path: &Path, bytes: &[u8]) -> io::Result<()> {
    use std::io::Write as _;
    std::fs::OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(path)?
        .write_all(bytes)
}

async fn wait_for_client(directory: &Path) -> io::Result<ClientComplete> {
    tokio::time::timeout(CLIENT_TIMEOUT, async {
        loop {
            let path = directory.join("client-complete.json");
            if tokio::fs::try_exists(&path).await? {
                let marker: ClientComplete =
                    serde_json::from_slice(&read_bounded(&path, COMPLETION_MAXIMUM)?).map_err(
                        |_| io::Error::other("invalid closed live-client completion marker"),
                    )?;
                marker.validate()?;
                return Ok(marker);
            }
            tokio::time::sleep(Duration::from_millis(100)).await;
        }
    })
    .await
    .map_err(|_| io::Error::other("live-client trial did not finish within 180 seconds"))?
}

// This layer only releases test-owned response bodies during server teardown.
// The live route, byte stream, rate, generation lookup and process owner remain
// the production ordinary path. Dropping its Body signals existing child cleanup.
async fn cancel_responses(
    State(stop): State<CancellationToken>,
    request: Request,
    next: Next,
) -> Response {
    let streaming = request.uri().path().starts_with("/iptv/hls-direct/");
    let response = next.run(request).await;
    if !streaming {
        return response;
    }
    let (parts, body) = response.into_parts();
    Response::from_parts(
        parts,
        Body::from_stream(body.into_data_stream().take_until(stop.cancelled_owned())),
    )
}

struct LiveOrigin {
    base: String,
    stop: CancellationToken,
    task: JoinHandle<io::Result<()>>,
}

impl LiveOrigin {
    async fn start(fixture: &Fixture) -> io::Result<Self> {
        let listener = TcpListener::bind("127.0.0.1:0").await?;
        let address = listener.local_addr()?;
        let stop = CancellationToken::new();
        let app = ersatzrs_web::host::router::build_app(fixture.state.clone()).layer(
            middleware::from_fn_with_state(stop.clone(), cancel_responses),
        );
        let shutdown = stop.clone();
        let task = tokio::spawn(async move {
            axum::serve(
                listener,
                app.into_make_service_with_connect_info::<SocketAddr>(),
            )
            .with_graceful_shutdown(shutdown.cancelled_owned())
            .await
        });
        Ok(Self {
            base: format!("http://{address}"),
            stop,
            task,
        })
    }

    async fn close(&mut self) -> io::Result<()> {
        self.stop.cancel();
        match tokio::time::timeout(Duration::from_secs(5), &mut self.task).await {
            Ok(Ok(result)) => result,
            Ok(Err(_)) => Err(io::Error::other("owned live-client server task failed")),
            Err(_) => {
                self.task.abort();
                let _ = (&mut self.task).await;
                Err(io::Error::other(
                    "owned live-client server shutdown exceeded its bound",
                ))
            }
        }
    }
}

impl Drop for LiveOrigin {
    fn drop(&mut self) {
        self.stop.cancel();
        self.task.abort();
    }
}

fn ready(
    origin: &LiveOrigin,
    accepted: &[AcceptedTeletextOutputCandidate; 2],
    source: &[u8],
) -> Value {
    let channels = accepted.iter().enumerate().map(|(index, candidate)| {
        let channel = index + 1;
        let selection = &candidate.selection().bindings[0];
        json!({"channel_id":channel,
            "url":format!("{}/iptv/hls-direct/{channel}.ts", origin.base),
            "addon_id":selection.addon_id,"profile_id":selection.profile_id,
            "profile_revision":selection.profile_revision,"theme":if channel==1 {"day"} else {"night"},
            "theme_rgb4":if channel==1 {[15,0,0]} else {[0,0,15]},
            "generation_id":candidate.generation().id,"request_id":candidate.generation().request.id,
            "design_revision":candidate.generation().request.revision,
            "expected_pages":[124,125],"subcode":32,"expected_navigation":[124,125,124],
            "edited_q_page":if channel==1 {Some(124)} else {None},
            "edited_q_cell":{"row":2,"column":8},"drcs_mode":"monochrome_dense_s1_0_1"})
    }).collect::<Vec<_>>();
    json!({"probe_version":1,"scope":"cold actual ordinary HTTP URLs; no capture replay or warm transition proof",
        "source":{"file":"source.ts","sha256":hash(source),"bytes":source.len(),"duration_seconds":60},
        "channels":channels,"active_transcode_limit":4,
        "completion_file":"client-complete.json","completion_timeout_seconds":180,
        "expected_completion":{"probe_version":1,"client_name":"VLC","client_version":"3.0.20",
            "decoder_version":"0.2.42","proof_suffix":"live-ordinary"},
        "client_actions":"Open both URLs in owned headed clients; navigate 124 to 125 to 124 on each; finish and close clients before writing completion.",
        "limitations":"No full Level 2.5, warm generation/Off, panel or platform acceptance is implied."})
}

async fn assert_unchanged(
    fixture: &mut Fixture,
    flow: &Flow,
    accepted: &[AcceptedTeletextOutputCandidate; 2],
    package_hash: &str,
) {
    for (index, expected) in accepted.iter().enumerate() {
        let channel = i64::try_from(index + 1).unwrap();
        generations::assert_same_output(&recover(fixture, channel).await, expected);
        assert_eq!(
            fixture
                .state
                .application
                .accepted_teletext_generation(channel)
                .await
                .unwrap(),
            Some(expected.generation().clone())
        );
        assert_eq!(
            fixture
                .state
                .application
                .teletext_publication_request(channel)
                .await
                .unwrap(),
            Some(expected.generation().request.clone())
        );
    }
    assert_retained_storage(fixture, flow).await;
    assert_eq!(
        calculate_addon_content_sha256(&fixture.root.path().join("package"))
            .await
            .unwrap(),
        package_hash
    );
    assert!(fixture.playout.try_recv().is_err());
}

#[tokio::test]
#[ignore = "requires exact FFmpeg/ffprobe9, a create-new live-client proof directory and separately operated pinned headed VLC/libzvbi clients on the actual ordinary URLs"]
async fn published_indexed_originals_serve_live_ordinary_urls_for_two_headed_clients() {
    let directory = PathBuf::from(client_capture::selected(DIRECTORY_ENV));
    std::fs::create_dir(&directory).expect("live-client proof destination must not already exist");
    let ffmpeg = client_capture::selected("ERSATZRS_TELETEXT_TEST_FFMPEG");
    let ffprobe = client_capture::selected("FFPROBE_PATH");
    client_capture::check_tools(&ffmpeg, &ffprobe).await;
    let fixture = Fixture::new_persistent().await;
    // Two headed viewers plus two concurrent transport probes need four real
    // output owners. Configure only this disposable fixture before its captured
    // baseline; capacity-wait slates are a separate playback safety case.
    fixture
        .state
        .application
        .set_admin_setting(ersatzrs_app::commands::SetAdminSetting {
            key: ersatzrs_domain::CONFIG_KEY_FFMPEG_ACTIVE_TRANSCODE_LIMIT,
            value: "4".to_owned(),
        })
        .await
        .unwrap();
    let source_path =
        client_capture::generate_source_with_duration(&ffmpeg, fixture.root.path(), 60).await;
    client_capture::configure_source_for_channels(&fixture, &source_path, &[1, 2]).await;
    ordinary_stream_flow::enable_output_policy(&fixture).await;
    let source = read_bounded(&source_path, SOURCE_MAXIMUM).unwrap();
    assert!(!source.is_empty());
    assert_eq!(source.len() % 188, 0);
    write_new(&directory.join("source.ts"), &source).unwrap();
    let (mut fixture, flow) = Box::pin(setup_with_fixture_policy(
        drcs_companion::dense_response,
        fixture,
        drcs_companion::dense_encoding_policy,
    ))
    .await;
    let edited = import(&fixture, request(&flow, 2, 8, b'Q')).await;
    assert_original_pointers(&fixture, &flow).await;
    let preview = assert_concept_preview(&fixture, &flow, &edited, 2).await;
    let day = publish_and_accept(&fixture, &flow, &edited).await;
    assert_eq!(generations::preview(day.service()).raster(), &preview);
    fixture = fixture.reopen().await;
    assert_eq!(
        ersatzrs_infra::transcode_capacity::load_active_transcode_limit(&fixture.state.pool)
            .await
            .unwrap(),
        Some(4)
    );
    let accepted = [recover(&fixture, 1).await, recover(&fixture, 2).await];
    generations::assert_same_output(&accepted[0], &day);
    generations::assert_same_output(&accepted[1], &flow.accepted[1]);
    assert_retained_storage(&fixture, &flow).await;
    let package_hash = calculate_addon_content_sha256(&fixture.root.path().join("package"))
        .await
        .unwrap();
    ordinary_stream_flow::reset_source_start(&fixture).await;
    let mut origin = LiveOrigin::start(&fixture).await.unwrap();
    // Catch assertion failures as well as marker errors so every post-bind path
    // shuts down the owned server/response bodies before closing the pool.
    let outcome = AssertUnwindSafe(async {
        write_new(
            &directory.join("ready.json"),
            &serde_json::to_vec_pretty(&ready(&origin, &accepted, &source)).unwrap(),
        )?;
        let complete = wait_for_client(&directory).await?;
        assert_unchanged(&mut fixture, &flow, &accepted, &package_hash).await;
        Ok::<_, io::Error>(complete)
    })
    .catch_unwind()
    .await;
    let closed = origin.close().await;
    fixture.state.pool.close().await;
    let complete = match outcome {
        Ok(result) => result.expect("live-client trial did not complete"),
        Err(panic) => std::panic::resume_unwind(panic),
    };
    closed.expect("owned live-client server did not close cleanly");
    write_new(&directory.join("success.json"), &serde_json::to_vec_pretty(&json!({
        "probe_version":1,"scope":"published/reopened two-channel originals hosted at actual ordinary live URLs",
        "completion":complete,"ordinary_route":"/iptv/hls-direct/{channel}.ts",
        "source_sha256":hash(&source),"source_bytes":source.len(),
        "native_generations_resources_originals_receipts_packages_unchanged":true,
        "owned_server_closed":true,
        "headed_client_evidence":"External completion marker; root-owned display/navigation evidence must be attached separately.",
        "limitations":"Cold live-URL trial only; full engine, warm transitions and platform acceptance remain open."
    })).unwrap()).unwrap();
}
