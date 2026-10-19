//! Bounded warm headed-client protocol over three real ordinary TCP responses.
//! Stage capture markers synchronize a trial; they never certify visual output.
//! No capture replay, replacement viewer or production infrastructure is added.

use super::super::super::{client_capture, ordinary_stream_flow};
use super::super::{external_editor_probe, Flow};
use super::*;
use axum::{
    body::Body,
    extract::{Request, State},
    http::StatusCode,
    middleware::{self, Next},
    response::{IntoResponse as _, Response},
};
use ersatzrs_app::commands::SetAdminSetting;
use ersatzrs_web::host::{router::build_app, teletext_work::MAX_CONCURRENT_SETTING};
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
use tokio::{net::TcpListener, time::timeout_at};
use tokio_util::sync::CancellationToken;
use tracing::Instrument as _;

const DIRECTORY_ENV: &str = "ERSATZRS_TELETEXT_WARM_CLIENT_DIR";
const MAX_MARKER: usize = 16 * 1024;
const TRIAL_TIMEOUT: Duration = Duration::from_mins(3);
const MAX_TERMINAL_LOG: usize = 512 * 1024;
const MAX_PROVENANCE_LOG: usize = 4 * 1024 * 1024;

#[derive(Clone)]
struct TerminalLog(Arc<Mutex<(std::fs::File, usize, usize)>>);

impl io::Write for TerminalLog {
    fn write(&mut self, bytes: &[u8]) -> io::Result<usize> {
        let mut state = self.0.lock().unwrap();
        let count = bytes.len().min(state.2 - state.1);
        state.0.write_all(&bytes[..count])?;
        state.1 += count;
        // The diagnostic cap must never change the observed stream lifetime.
        Ok(bytes.len())
    }

    fn flush(&mut self) -> io::Result<()> {
        self.0.lock().unwrap().0.flush()
    }
}

pub(in super::super::super) fn terminal_trace(directory: &Path) -> io::Result<()> {
    terminal_trace_with_provenance(directory, false)
}

pub(in super::super::super) fn terminal_trace_with_provenance(
    directory: &Path,
    provenance: bool,
) -> io::Result<()> {
    use tracing_subscriber::{layer::SubscriberExt as _, util::SubscriberInitExt as _};
    let file = std::fs::OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(directory.join("stream-terminal.log"))?;
    // Three packet-window observers exceed the warning-only cap before expiry.
    // Keep the opt-in proof bounded, and let its verifier reject truncation.
    let maximum = if provenance {
        MAX_PROVENANCE_LOG
    } else {
        MAX_TERMINAL_LOG
    };
    let writer = TerminalLog(Arc::new(Mutex::new((file, 0, maximum))));
    // This ignored gate runs alone in its owned nextest process. Its subscriber
    // must cover the real producer tasks, without initializing host logging or
    // changing the application's subscriber/filter configuration.
    let mut targets = tracing_subscriber::filter::Targets::new()
        .with_target(
            "ersatzrs_web::transport::iptv::stream",
            tracing::Level::WARN,
        )
        .with_target(
            "ersatzrs_web::transport::iptv::teletext_output",
            tracing::Level::WARN,
        );
    if provenance {
        targets = targets.with_target(
            "ersatzrs_web::transport::iptv::teletext_provenance",
            tracing::Level::DEBUG,
        );
    }
    tracing_subscriber::registry()
        .with(targets)
        .with(
            tracing_subscriber::fmt::layer()
                .with_ansi(false)
                .with_timer(tracing_subscriber::fmt::time::uptime())
                .with_writer(move || writer.clone()),
        )
        .try_init()
        .map_err(|_| io::Error::other("warm isolated terminal subscriber could not be installed"))
}

pub(in super::super::super) fn hash(bytes: &[u8]) -> String {
    use std::fmt::Write as _;
    let mut value = String::with_capacity(64);
    for byte in sha2::Sha256::digest(bytes) {
        write!(value, "{byte:02x}").unwrap();
    }
    value
}

pub(in super::super::super) fn write_new(path: &Path, bytes: &[u8]) -> io::Result<()> {
    use std::io::Write as _;
    let directory = path
        .parent()
        .ok_or_else(|| io::Error::other("warm proof output requires a parent directory"))?;
    let mut temporary = tempfile::Builder::new()
        .prefix(".warm-proof-")
        .tempfile_in(directory)?;
    temporary.write_all(bytes)?;
    // Close the complete file before making its create-new public name visible.
    // A same-directory hard link refuses replacement of any existing marker.
    let temporary = temporary.into_temp_path();
    std::fs::hard_link(&temporary, path)?;
    temporary.close()
}

pub(in super::super::super) fn read_bounded(path: &Path, maximum: usize) -> io::Result<Vec<u8>> {
    use std::io::Read as _;
    let metadata = std::fs::symlink_metadata(path)?;
    if !metadata.file_type().is_file() || metadata.len() > u64::try_from(maximum).unwrap() {
        return Err(io::Error::other(
            "warm proof input is not a bounded regular file",
        ));
    }
    let mut bytes = Vec::new();
    std::fs::File::open(path)?
        .take(u64::try_from(maximum).unwrap() + 1)
        .read_to_end(&mut bytes)?;
    if bytes.len() > maximum {
        return Err(io::Error::other("warm proof input exceeds its read bound"));
    }
    Ok(bytes)
}

pub(in super::super::super) async fn wait_file(
    directory: &Path,
    name: &str,
    deadline: Instant,
) -> io::Result<Vec<u8>> {
    timeout_at(deadline, async {
        let path = directory.join(name);
        loop {
            if tokio::fs::try_exists(&path).await? {
                return read_bounded(&path, MAX_MARKER);
            }
            tokio::time::sleep(Duration::from_millis(50)).await;
        }
    })
    .await
    .map_err(|_| io::Error::other("warm trial exceeded its single overall deadline"))?
}

#[derive(Clone)]
struct Tee {
    captures: [Capture; 3],
    seen: Arc<Mutex<[bool; 3]>>,
    stop: CancellationToken,
    started: Instant,
    directory: PathBuf,
}

fn body_diagnostic(error: &axum::Error, started: Instant) -> Value {
    use std::error::Error as _;
    let label = match error.to_string().as_str() {
        "teletext transport failed" => "teletext transport failed",
        "encoded output was cancelled" => "encoded output was cancelled",
        "encoded output producer failed" => "encoded output producer failed",
        "encoded output cleanup is unconfirmed" => "encoded output cleanup is unconfirmed",
        "encoded output supervisor ended" => "encoded output supervisor ended",
        _ => "other body error",
    };
    let mut nested = error.source();
    let mut io_kind = None;
    for _ in 0..8 {
        let Some(cause) = nested else { break };
        if let Some(error) = cause.downcast_ref::<io::Error>() {
            io_kind = Some(format!("{:?}", error.kind()));
        }
        nested = cause.source();
    }
    json!({"probe_version":1,"elapsed_ms":started.elapsed().as_millis().max(1),
        "terminal_label":label,"io_kind":io_kind})
}

fn record_client_error(directory: &Path, started: Instant, error: &reqwest::Error) {
    let diagnostic = json!({"probe_version":1,"elapsed_ms":started.elapsed().as_millis().max(1),
        "is_timeout":error.is_timeout(),"is_body":error.is_body(),"is_decode":error.is_decode(),
        "is_request":error.is_request(),"is_connect":error.is_connect()});
    if write_new(
        &directory.join("observer-client-error.json"),
        &serde_json::to_vec_pretty(&diagnostic).unwrap(),
    )
    .is_err()
    {
        eprintln!("warm observer client diagnostic could not be published");
    }
}

fn viewer_index(request: &Request) -> Option<usize> {
    if request.method() != axum::http::Method::GET {
        return None;
    }
    match (request.uri().path(), request.uri().query()) {
        ("/iptv/hls-direct/1.ts", Some("teletext_test_viewer=day")) => Some(0),
        ("/iptv/hls-direct/1.ts", Some("teletext_test_viewer=observer")) => Some(1),
        ("/iptv/hls-direct/2.ts", Some("teletext_test_viewer=night")) => Some(2),
        _ => None,
    }
}

async fn tee_response(State(state): State<Tee>, request: Request, next: Next) -> Response {
    let Some(index) = viewer_index(&request) else {
        return StatusCode::NOT_FOUND.into_response();
    };
    {
        let mut seen = state.seen.lock().unwrap();
        if seen[index] {
            return StatusCode::CONFLICT.into_response();
        }
        seen[index] = true;
    }
    // The ordinary intent is created inline here; its private trace ID bridges
    // the later producer task without injecting a binding or changing headers.
    let response = next
        .run(request)
        .instrument(tracing::debug_span!(
            target: "ersatzrs_web::transport::iptv::teletext_provenance",
            "teletext_native_trial_request",
            viewer_index = index,
            request_trace = %uuid::Uuid::new_v4(),
        ))
        .await;
    if response.status() != StatusCode::OK {
        return response;
    }
    let (parts, body) = response.into_parts();
    let capture = state.captures[index].clone();
    let directory = state.directory;
    let started = state.started;
    let stream = body
        .into_data_stream()
        .take_until(state.stop.cancelled_owned())
        .map(move |result| {
            if let Ok(bytes) = &result {
                capture.wire.lock().unwrap().append(bytes);
                capture.changed.notify_one();
            } else {
                let diagnostic = body_diagnostic(result.as_ref().unwrap_err(), started);
                let filename = if index == 1 {
                    "observer-body-error.json".to_owned()
                } else {
                    format!("viewer-{index}-body-error.json")
                };
                if write_new(
                    &directory.join(filename),
                    &serde_json::to_vec_pretty(&diagnostic).unwrap(),
                )
                .is_err()
                {
                    eprintln!("warm observer body diagnostic could not be published");
                }
            }
            result
        });
    Response::from_parts(parts, Body::from_stream(stream))
}

pub(in super::super::super) struct Origin {
    base: String,
    state: Tee,
    task: JoinHandle<io::Result<()>>,
}

impl Origin {
    pub(in super::super::super) async fn start(
        fixture: &Fixture,
        directory: &Path,
    ) -> io::Result<Self> {
        let listener = TcpListener::bind("127.0.0.1:0").await?;
        let address = listener.local_addr()?;
        let state = Tee {
            captures: [Capture::new(), Capture::new(), Capture::new()],
            seen: Arc::new(Mutex::new([false; 3])),
            stop: CancellationToken::new(),
            started: Instant::now(),
            directory: directory.to_owned(),
        };
        let app = build_app(fixture.state.clone())
            .layer(middleware::from_fn_with_state(state.clone(), tee_response));
        let shutdown = state.stop.clone();
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
            state,
            task,
        })
    }
    pub(in super::super::super) fn url(&self, channel: u8, viewer: &str) -> String {
        format!(
            "{}/iptv/hls-direct/{channel}.ts?teletext_test_viewer={viewer}",
            self.base
        )
    }
    pub(in super::super::super) async fn close(&mut self) -> io::Result<()> {
        self.state.stop.cancel();
        match tokio::time::timeout(Duration::from_secs(5), &mut self.task).await {
            Ok(Ok(result)) => result,
            Ok(Err(_)) => Err(io::Error::other("warm origin task failed")),
            Err(_) => {
                self.task.abort();
                let _ = (&mut self.task).await;
                Err(io::Error::other("warm origin shutdown exceeded its bound"))
            }
        }
    }
    pub(in super::super::super) fn captures(&self) -> &[Capture; 3] {
        &self.state.captures
    }
    pub(in super::super::super) fn single_requests(&self) -> bool {
        *self.state.seen.lock().unwrap() == [true; 3]
    }
    pub(in super::super::super) fn requests_seen(&self) -> [bool; 3] {
        *self.state.seen.lock().unwrap()
    }
}

impl Drop for Origin {
    fn drop(&mut self) {
        self.state.stop.cancel();
        self.task.abort();
    }
}

pub(in super::super::super) struct Observer {
    stop: CancellationToken,
    task: JoinHandle<io::Result<()>>,
}

impl Observer {
    pub(in super::super::super) fn start(origin: &Origin) -> Self {
        Self::start_for(origin, 1, "observer")
    }
    pub(in super::super::super) fn start_for(origin: &Origin, channel: u8, viewer: &str) -> Self {
        let stop = CancellationToken::new();
        let cancel = stop.clone();
        let url = origin.url(channel, viewer);
        let directory = origin.state.directory.clone();
        let started = origin.state.started;
        let task = tokio::spawn(async move {
            let client = reqwest::Client::builder()
                .connect_timeout(Duration::from_secs(5))
                .build()
                .map_err(|_| io::Error::other("warm observer client setup failed"))?;
            let response = client.get(url).send().await.map_err(|error| {
                record_client_error(&directory, started, &error);
                io::Error::other("warm observer request failed")
            })?;
            if response.status() != reqwest::StatusCode::OK {
                return Err(io::Error::other("warm observer request was not admitted"));
            }
            let mut body = response.bytes_stream();
            loop {
                let next =
                    tokio::select! { biased; ()=cancel.cancelled()=>break,next=body.next()=>next, };
                let Some(bytes) = next else {
                    break;
                };
                bytes.map_err(|error| {
                    record_client_error(&directory, started, &error);
                    io::Error::other("warm observer stream failed")
                })?;
            }
            Ok(())
        });
        Self { stop, task }
    }
    pub(in super::super::super) async fn close(&mut self) -> io::Result<()> {
        self.stop.cancel();
        match tokio::time::timeout(Duration::from_secs(5), &mut self.task).await {
            Ok(Ok(result)) => result,
            Ok(Err(_)) => Err(io::Error::other("warm observer task failed")),
            Err(_) => {
                self.task.abort();
                let _ = (&mut self.task).await;
                Err(io::Error::other(
                    "warm observer shutdown exceeded its bound",
                ))
            }
        }
    }
}

impl Drop for Observer {
    fn drop(&mut self) {
        self.stop.cancel();
        self.task.abort();
    }
}

#[derive(Clone, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub(in super::super::super) struct ClientIdentity {
    pub(in super::super::super) viewer: String,
    pub(in super::super::super) pid: u32,
}
#[derive(Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub(in super::super::super) struct Screenshot {
    pub(in super::super::super) viewer: String,
    pub(in super::super::super) page: Option<u16>,
    pub(in super::super::super) file: String,
    pub(in super::super::super) sha256: String,
}
#[derive(Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub(in super::super::super) struct StageCaptured {
    pub(in super::super::super) probe_version: u8,
    pub(in super::super::super) stage: u8,
    pub(in super::super::super) ready_sha256: String,
    pub(in super::super::super) clients: Vec<ClientIdentity>,
    pub(in super::super::super) screenshots: Vec<Screenshot>,
}

fn valid_digest(value: &str) -> bool {
    value.len() == 64
        && value
            .bytes()
            .all(|c| c.is_ascii_digit() || (b'a'..=b'f').contains(&c))
}

fn validate_images(marker: &StageCaptured) -> io::Result<()> {
    let navigation = [Some(124), Some(125), Some(124)];
    let single = [Some(124)];
    let off = [None];
    let day = if marker.stage == 2 {
        &navigation[..]
    } else if matches!(marker.stage, 1 | 4) {
        &off[..]
    } else {
        &single[..]
    };
    let night = if marker.stage == 2 {
        &navigation[..]
    } else {
        &single[..]
    };
    validate_images_for(marker, [day, night])
}

pub(in super::super::super) fn validate_images_for(
    marker: &StageCaptured,
    expected: [&[Option<u16>]; 2],
) -> io::Result<()> {
    if marker.screenshots.len() > 12 {
        return Err(io::Error::other("warm stage image count exceeds bound"));
    }
    let mut names = std::collections::BTreeSet::new();
    for image in &marker.screenshots {
        let name = &image.file;
        if name.is_empty()
            || name.len() > 128
            || Path::new(name).extension() != Some(std::ffi::OsStr::new("png"))
            || name.contains(['/', '\\'])
            || !name
                .bytes()
                .all(|c| c.is_ascii_alphanumeric() || matches!(c, b'-' | b'_' | b'.'))
            || !names.insert(name)
            || !valid_digest(&image.sha256)
        {
            return Err(io::Error::other("warm stage image label/digest is invalid"));
        }
    }
    for (index, viewer) in ["day", "night"].into_iter().enumerate() {
        let pages: Vec<_> = marker
            .screenshots
            .iter()
            .filter(|image| image.viewer == viewer)
            .map(|image| image.page)
            .collect();
        if pages != expected[index] {
            return Err(io::Error::other(
                "warm stage requested-page sequence is incomplete",
            ));
        }
    }
    if marker
        .screenshots
        .iter()
        .any(|image| !matches!(image.viewer.as_str(), "day" | "night"))
    {
        return Err(io::Error::other("warm stage contains an unknown viewer"));
    }
    Ok(())
}

/// Authenticate an exact bounded stage acknowledgement and retained client PIDs.
/// Image sequences remain the caller's explicit protocol contract.
pub(in super::super::super) fn stage_client_identities(
    marker: &StageCaptured,
    version: u8,
    index: u8,
    ready_sha256: &str,
    previous: Option<[u32; 2]>,
) -> io::Result<[u32; 2]> {
    if marker.probe_version != version
        || marker.stage != index
        || marker.ready_sha256 != ready_sha256
        || marker.clients.len() != 2
    {
        return Err(io::Error::other(
            "warm stage capture does not bind this exact readiness",
        ));
    }
    let mut identities = [0; 2];
    for (slot, viewer) in ["day", "night"].iter().enumerate() {
        let matches: Vec<_> = marker
            .clients
            .iter()
            .filter(|client| client.viewer == *viewer)
            .collect();
        if matches.len() != 1 || matches[0].pid == 0 {
            return Err(io::Error::other(
                "warm stage lacks a unique owned client identity",
            ));
        }
        identities[slot] = matches[0].pid;
    }
    if identities[0] == identities[1] || previous.is_some_and(|previous| previous != identities) {
        return Err(io::Error::other("warm stage replaced a client process"));
    }
    Ok(identities)
}

struct Protocol<'a> {
    directory: &'a Path,
    deadline: Instant,
    clients: Option<[u32; 2]>,
    receipts: Vec<Value>,
}

impl Protocol<'_> {
    async fn stage(
        &mut self,
        stage: (u8, &str, bool, bool),
        current: &AcceptedTeletextOutputCandidate,
        flow: &Flow,
        views: &[Capture; 3],
    ) -> io::Result<()> {
        let (index, status, visible, q) = stage;
        let ready = json!({"probe_version":1,"stage":index,"state":status,
            "scope":"wire-observed stage, awaiting owned headed screenshots; no visual certification",
            "day":{"channel_id":1,"generation_id":current.generation().id,"teletext_visible":visible,"edited_q_present":q,"theme_rgb4":[15,0,0]},
            "night":{"channel_id":2,"generation_id":flow.accepted[1].generation().id,"teletext_visible":true,"edited_q_present":false,"theme_rgb4":[0,0,15]},
            "expected_pages":[124,125],"subcode":32,"wire_offsets":views.iter().map(Capture::position).collect::<Vec<_>>()});
        let bytes = serde_json::to_vec_pretty(&ready).unwrap();
        let ready_sha256 = hash(&bytes);
        write_new(
            &self.directory.join(format!("stage-{index}-ready.json")),
            &bytes,
        )?;
        let raw = wait_file(
            self.directory,
            &format!("stage-{index}-captured.json"),
            self.deadline,
        )
        .await?;
        let marker: StageCaptured = serde_json::from_slice(&raw)
            .map_err(|_| io::Error::other("invalid closed warm-stage capture marker"))?;
        let identities = stage_client_identities(&marker, 1, index, &ready_sha256, self.clients)?;
        validate_images(&marker)?;
        self.clients = Some(identities);
        self.receipts.push(
            json!({"stage":index,"ready_sha256":ready_sha256,"capture":marker,
            "visual_match_certified":false}),
        );
        Ok(())
    }
}

#[derive(Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
struct ClientComplete {
    probe_version: u8,
    client_name: String,
    client_version: String,
    decoder_version: String,
    proof_suffix: String,
}

async fn completion(protocol: &Protocol<'_>) -> io::Result<ClientComplete> {
    let raw = wait_file(
        protocol.directory,
        "client-complete.json",
        protocol.deadline,
    )
    .await?;
    let value: ClientComplete = serde_json::from_slice(&raw)
        .map_err(|_| io::Error::other("invalid closed warm-client completion marker"))?;
    if value.probe_version != 1
        || value.client_name != "VLC"
        || value.client_version != "3.0.20"
        || value.decoder_version != "0.2.42"
        || value.proof_suffix != "warm-live-ordinary"
    {
        return Err(io::Error::other(
            "unexpected warm-client completion identity",
        ));
    }
    Ok(value)
}

async fn setting(fixture: &Fixture, key: &'static str, value: String) {
    fixture
        .state
        .application
        .set_admin_setting(SetAdminSetting { key, value })
        .await
        .unwrap();
}

async fn fixture(ffmpeg: &str, directory: &Path) -> (Fixture, Flow, Vec<u8>, i64) {
    let fixture = Fixture::new_persistent().await;
    let source_path =
        client_capture::generate_source_with_duration(ffmpeg, fixture.root.path(), 60).await;
    client_capture::configure_source_for_channels(&fixture, &source_path, &[1, 2]).await;
    let source = read_bounded(&source_path, MAX_CAPTURE).unwrap();
    assert!(!source.is_empty() && source.len().is_multiple_of(188));
    write_new(&directory.join("source.ts"), &source).unwrap();
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
    setting(
        &fixture,
        ersatzrs_domain::CONFIG_KEY_FFMPEG_ACTIVE_TRANSCODE_LIMIT,
        "3".to_owned(),
    )
    .await;
    setting(&fixture, MAX_CONCURRENT_SETTING, "4".to_owned()).await;
    let (fixture, flow) = Box::pin(setup_with_fixture_policy(
        external_editor_probe::editor_response,
        fixture,
        channel_documents::policy,
    ))
    .await;
    let off = disable(&fixture, 1).await;
    let fixture = fixture.reopen().await;
    generations::assert_same_output(&recover(&fixture, 2).await, &flow.accepted[1]);
    (fixture, flow, source, off)
}

fn initial_ready(origin: &Origin, flow: &Flow, source: &[u8]) -> Value {
    json!({"probe_version":1,"protocol":"warm-live-ordinary-v1","completion_timeout_seconds":180,
        "source":{"file":"source.ts","sha256":hash(source),"bytes":source.len(),"duration_seconds":60},
        "channels":[{"channel_id":1,"theme":"day","url":origin.url(1,"day"),"initially_off":true,"theme_rgb4":[15,0,0],
            "profile_id":flow.saved[0].candidate().selection().bindings[0].profile_id},
            {"channel_id":2,"theme":"night","url":origin.url(2,"night"),"initially_off":false,"theme_rgb4":[0,0,15],
            "profile_id":flow.saved[1].candidate().selection().bindings[0].profile_id}],
        "expected_pages":[124,125],"subcode":32,"stages":["initial_off","on","replacement","off","reenabled"],
        "stage_files":"stage-N-ready.json and stage-N-captured.json, N from 1 through 5",
        "final_completion":{"probe_version":1,"client_name":"VLC","client_version":"3.0.20","decoder_version":"0.2.42","proof_suffix":"warm-live-ordinary"},
        "limitations":"Stage markers certify neither pixels nor navigation; root-owned visual receipts remain required. Resource retirement, panels and full engine acceptance remain open."})
}

async fn transitions(
    fixture: &Fixture,
    flow: &Flow,
    off: i64,
    origin: &Origin,
    protocol: &mut Protocol<'_>,
) -> io::Result<Current> {
    wait_attached(origin, protocol.deadline).await?;
    let views = &origin.state.captures;
    for view in &views[..2] {
        view.off(0).await;
    }
    views[2].service(0, &flow.accepted[1]).await;
    protocol
        .stage(
            (1, "initial_off", false, false),
            &flow.accepted[0],
            flow,
            views,
        )
        .await?;
    let revision = enable(fixture, flow, off).await;
    let current = fresh_acceptance(
        fixture,
        views,
        &flow.pending[0],
        flow.accepted[0].generation().id,
    )
    .await;
    views[2]
        .service(views[2].position(), &flow.accepted[1])
        .await;
    protocol
        .stage((2, "on", true, false), &current.accepted, flow, views)
        .await?;
    let current = replace_concept(fixture, views, &current).await;
    views[2]
        .service(views[2].position(), &flow.accepted[1])
        .await;
    protocol
        .stage(
            (3, "replacement", true, true),
            &current.accepted,
            flow,
            views,
        )
        .await?;
    let revision = disable(fixture, revision).await;
    for view in &views[..2] {
        view.off(view.position()).await;
    }
    views[2]
        .service(views[2].position(), &flow.accepted[1])
        .await;
    protocol
        .stage((4, "off", false, false), &current.accepted, flow, views)
        .await?;
    enable(fixture, flow, revision).await;
    let current = fresh_acceptance(
        fixture,
        views,
        &current.request,
        current.accepted.generation().id,
    )
    .await;
    views[2]
        .service(views[2].position(), &flow.accepted[1])
        .await;
    protocol
        .stage(
            (5, "reenabled", true, false),
            &current.accepted,
            flow,
            views,
        )
        .await?;
    assert_eq!(*origin.state.seen.lock().unwrap(), [true; 3]);
    assert_eq!(designs::config(fixture).await, flow.config);
    for index in 0..2 {
        assert_eq!(
            original(fixture, &flow.saved[index]).await,
            flow.originals[index]
        );
        assert_eq!(
            receipt(fixture, &flow.exported[index]).await,
            flow.receipts[index]
        );
    }
    Ok(current)
}

pub(in super::super::super) async fn wait_attached(
    origin: &Origin,
    deadline: Instant,
) -> io::Result<()> {
    // Client process launch is external orchestration, not a new source-clock
    // deadline. Start the existing five-second physical predicates only after
    // all three distinct real responses have actually yielded complete packets.
    timeout_at(deadline, async {
        loop {
            let seen = *origin.state.seen.lock().unwrap() == [true; 3];
            if seen
                && origin
                    .state
                    .captures
                    .iter()
                    .all(|capture| capture.position() >= 200 * 188)
            {
                return;
            }
            tokio::time::sleep(Duration::from_millis(50)).await;
        }
    })
    .await
    .map_err(|_| io::Error::other("warm viewers did not attach within overall deadline"))
}

pub(in super::super::super) async fn wait_for_cleanup() -> io::Result<()> {
    tokio::time::timeout(Duration::from_secs(5), async {
        while ersatzrs_infra::transcode_capacity::active_transcode_snapshot(3).active != 0 {
            tokio::time::sleep(Duration::from_millis(20)).await;
        }
    })
    .await
    .map_err(|_| io::Error::other("warm response producers did not release owned capacity"))
}

pub(in super::super::super) fn save_captures(directory: &Path, origin: &Origin) -> io::Result<()> {
    for (index, name) in ["day", "observer", "night"].iter().enumerate() {
        write_new(
            &directory.join(format!("{name}-ordinary.ts")),
            &origin.state.captures[index].bytes(),
        )?;
    }
    Ok(())
}

/// Reuse the complete original synthetic PCR/PES/PMT clock oracle for an
/// uninterrupted On-to-On resource replacement, where no gap is expected.
pub(in super::super::super) fn assert_uninterrupted_source_clock(bytes: &[u8]) {
    assert_eq!(qualified_source_clock_gap_count(bytes), 0);
}

/// Reuse the unchanged PMT-Off/source-clock oracle for a retained response.
/// A nonzero result is a qualified analyzer gap, never a zero-gap transport claim.
pub(in super::super::super) fn qualified_source_clock_gap_count(bytes: &[u8]) -> usize {
    super::live_tcp_clock::verify_clock_gaps(bytes).len()
}

pub(in super::super::super) fn assert_plain_source_clock(bytes: &[u8]) {
    super::live_tcp_clock::verify_plain_source_clock(bytes);
}

async fn verify_native(
    directory: &Path,
    tools: (&str, &str, &str),
    source: &[u8],
    views: &[Capture; 3],
) {
    verify_native_pages(directory, tools, source, views, &["124"], false).await;
}

pub(in super::super::super) async fn verify_native_pages(
    directory: &Path,
    tools: (&str, &str, &str),
    source: &[u8],
    views: &[Capture; 3],
    pages: &[&str],
    zone_edit: bool,
) {
    verify_native_pages_with_content(
        directory,
        tools,
        source,
        views,
        pages,
        PageContent::edited(zone_edit),
    )
    .await;
}

#[derive(Clone, Copy)]
enum PageContent {
    FreeEdit,
    FreeAndZoneEdit,
    Original,
}

impl PageContent {
    fn edited(zone_edit: bool) -> Self {
        if zone_edit {
            Self::FreeAndZoneEdit
        } else {
            Self::FreeEdit
        }
    }
}

/// Original Base text has no editorial Q or R on any viewer or page.
pub(in super::super::super) async fn verify_original_native_pages(
    directory: &Path,
    tools: (&str, &str, &str),
    source: &[u8],
    views: &[Capture; 3],
    pages: &[&str],
) {
    verify_native_pages_with_content(
        directory,
        tools,
        source,
        views,
        pages,
        PageContent::Original,
    )
    .await;
}

async fn verify_native_pages_with_content(
    directory: &Path,
    tools: (&str, &str, &str),
    source: &[u8],
    views: &[Capture; 3],
    pages: &[&str],
    content: PageContent,
) {
    let (ffmpeg, ffprobe, tsanalyze) = tools;
    for (index, capture) in views.iter().enumerate() {
        let name = ["day", "observer", "night"][index];
        let mut bytes = capture.bytes();
        let original_length = bytes.len();
        let original_digest = hash(&bytes);
        // Preserve every received complete packet for independent probes. The
        // uncorrected capture, including a possible partial tail, stays on disk.
        bytes.truncate(bytes.len() / 188 * 188);
        assert!(bytes.len() >= 188 * 1000);
        // Qualify every analyzer-sized gap using full raw PCR/PES/PMT evidence,
        // before supplying its exact count to the unchanged transport assertions.
        let gaps = super::live_tcp_clock::verify_clock_gaps(&bytes);
        let provenance = gaps.iter().map(|gap| json!({
            "before_packet":gap.before_packet,"after_packet":gap.after_packet,
            "before_pts":gap.before_pts,"after_pts":gap.after_pts,
            "withdrawal_packet":gap.withdrawal_packet,"withdrawal_version":gap.withdrawal_version,
            "announcement_packet":gap.announcement_packet,"announcement_version":gap.announcement_version,
        })).collect::<Vec<_>>();
        write_new(&directory.join(format!("{name}-clock-gaps.json")),
            &serde_json::to_vec_pretty(&json!({"probe_version":1,
                "scope":"independently qualified raw PMT-Off clock gaps, not zero analyzer leaps or complete live acceptance",
                "capture_sha256":original_digest,"capture_bytes":original_length,
                "complete_packet_bytes":bytes.len(),"partial_tail_bytes":original_length-bytes.len(),
                "qualified_authored_pts_gaps":gaps.len(),"provenance":provenance})).unwrap()).unwrap();
        let report_path = directory.join(format!("{name}-tsduck.json"));
        client_capture::verify_transport_with_qualified_authored_pts_gaps(
            tsanalyze,
            ffprobe,
            &bytes,
            gaps.len(),
            Some(&report_path),
            true,
        )
        .await;
        // At most60s mono48kHz s16 PCM; the cold12s helper keeps its2MiB cap.
        client_capture::verify_av_with_audio_bound(ffmpeg, source, &bytes, 6 * 1024 * 1024).await;
        for (ordinal, page) in pages.iter().enumerate() {
            let text =
                client_capture::decode_page_text_with_discovery(ffmpeg, &bytes, page, true).await;
            assert!(
                text.contains("AZ"),
                "actual decoder lost original enhanced source glyphs"
            );
            assert_eq!(
                text.contains('Q'),
                !matches!(content, PageContent::Original) && index != 2 && ordinal == 0,
                "actual decoder mixed channel-specific concept edits"
            );
            if !matches!(content, PageContent::FreeEdit) {
                assert_eq!(
                    text.contains('R'),
                    matches!(content, PageContent::FreeAndZoneEdit) && index != 2 && ordinal == 0,
                    "actual decoder mixed cumulative zone edits"
                );
            }
            write_new(
                &directory.join(format!("{name}-decoded-{page}.srt")),
                text.as_bytes(),
            )
            .unwrap();
        }
    }
}

#[tokio::test]
#[ignore = "requires explicit bounded synthetic warm-capture proof directory, exact FFmpeg/ffprobe9 and TSDuck; offline verification never proves original HTTP/body cleanup or headed-client acceptance"]
async fn offline_warm_raw_captures_verify_clock_transport_av_and_authored_text() {
    let directory = PathBuf::from(client_capture::selected(
        "ERSATZRS_TELETEXT_WARM_OFFLINE_DIR",
    ));
    let ffmpeg = client_capture::selected("ERSATZRS_TELETEXT_TEST_FFMPEG");
    let ffprobe = client_capture::selected("FFPROBE_PATH");
    let tsanalyze = client_capture::selected("ERSATZRS_TELETEXT_TEST_TSANALYZE");
    client_capture::check_tools(&ffmpeg, &ffprobe).await;
    verify_saved_native_pages(
        &directory,
        &directory.join("offline-native-proof"),
        (&ffmpeg, &ffprobe, &tsanalyze),
        &["124"],
        false,
    )
    .await;
}

/// Recheck retained raw synthetic captures without constructing new responses.
/// This deliberately grants no success to the original live trial or actors.
pub(in super::super::super) async fn verify_saved_native_pages(
    directory: &Path,
    output: &Path,
    tools: (&str, &str, &str),
    pages: &[&str],
    zone_edit: bool,
) {
    verify_saved_native_pages_with_content(
        directory,
        output,
        tools,
        pages,
        PageContent::edited(zone_edit),
    )
    .await;
}

/// Offline original-only replay never substitutes for live actors or rendering.
pub(in super::super::super) async fn verify_saved_original_native_pages(
    directory: &Path,
    output: &Path,
    tools: (&str, &str, &str),
    pages: &[&str],
) {
    verify_saved_native_pages_with_content(directory, output, tools, pages, PageContent::Original)
        .await;
}

async fn verify_saved_native_pages_with_content(
    directory: &Path,
    output: &Path,
    tools: (&str, &str, &str),
    pages: &[&str],
    content: PageContent,
) {
    let source = read_bounded(&directory.join("source.ts"), MAX_CAPTURE).unwrap();
    assert!(!source.is_empty() && source.len().is_multiple_of(188));
    let names = ["day", "observer", "night"];
    let raw: [Vec<u8>; 3] = std::array::from_fn(|index| {
        read_bounded(
            &directory.join(format!("{}-ordinary.ts", names[index])),
            MAX_CAPTURE,
        )
        .unwrap()
    });
    let views = std::array::from_fn(|index| {
        let capture = Capture::new();
        // These are the existing unmodified captures, not new response/viewer
        // lifetimes. Only the verifier's complete-packet copy can be truncated.
        capture.wire.lock().unwrap().bytes.clone_from(&raw[index]);
        capture
    });
    std::fs::create_dir(output).expect("offline proof destination must not already exist");
    verify_native_pages_with_content(output, tools, &source, &views, pages, content).await;
    assert_eq!(
        read_bounded(&directory.join("source.ts"), MAX_CAPTURE).unwrap(),
        source
    );
    for (index, name) in names.iter().enumerate() {
        assert_eq!(
            read_bounded(&directory.join(format!("{name}-ordinary.ts")), MAX_CAPTURE).unwrap(),
            raw[index]
        );
    }
    let mut report = json!({
        "probe_version":1,"scope":"offline independent clock, exact TSDuck counts, source AV and decoded page text for retained raw synthetic captures",
        "pages":pages,"zone_edit_checked":matches!(content, PageContent::FreeAndZoneEdit),
        "source_sha256":hash(&source),"inputs":names.iter().enumerate().map(|(index,name)|json!({
            "file":format!("{name}-ordinary.ts"),"bytes":raw[index].len(),"sha256":hash(&raw[index])
        })).collect::<Vec<_>>(),"original_inputs_unchanged":true,
        "original_trial_success_claimed":false,"original_http_body_cleanup_proven":false,
        "headed_client_acceptance_claimed":false,"full_engine_acceptance_claimed":false,
        "limitations":"Offline success does not repair or certify an earlier failed native/HTTP trial, client rendering/navigation or producer cleanup."
    });
    if matches!(content, PageContent::Original) {
        report["editorial_content"] = json!("original_only");
        report["forbidden_editorial_glyphs"] = json!(["Q", "R"]);
    }
    write_new(
        &output.join("offline-verification.json"),
        &serde_json::to_vec_pretty(&report).unwrap(),
    )
    .unwrap();
}

#[tokio::test]
#[ignore = "requires exact FFmpeg/ffprobe9 and TSDuck, a create-new warm proof directory and separately controlled owned headed VLC clients with five bound stage capture markers and final root inspection"]
async fn warm_ordinary_tcp_views_wait_for_headed_off_on_replacement_off_and_reenable() {
    let directory = PathBuf::from(client_capture::selected(DIRECTORY_ENV));
    std::fs::create_dir(&directory).expect("warm proof destination must not already exist");
    terminal_trace(&directory).unwrap();
    let ffmpeg = client_capture::selected("ERSATZRS_TELETEXT_TEST_FFMPEG");
    let ffprobe = client_capture::selected("FFPROBE_PATH");
    let tsanalyze = client_capture::selected("ERSATZRS_TELETEXT_TEST_TSANALYZE");
    client_capture::check_tools(&ffmpeg, &ffprobe).await;
    let (mut fixture, flow, source, off) = fixture(&ffmpeg, &directory).await;
    let package_hash = calculate_addon_content_sha256(&fixture.root.path().join("package"))
        .await
        .unwrap();
    ordinary_stream_flow::reset_source_start(&fixture).await;
    let mut origin = Origin::start(&fixture, &directory).await.unwrap();
    let mut observer = Observer::start(&origin);
    let deadline = Instant::now() + TRIAL_TIMEOUT;
    let mut protocol = Protocol {
        directory: &directory,
        deadline,
        clients: None,
        receipts: Vec::new(),
    };
    let outcome = AssertUnwindSafe(async {
        write_new(
            &directory.join("ready.json"),
            &serde_json::to_vec_pretty(&initial_ready(&origin, &flow, &source)).unwrap(),
        )?;
        let current = timeout_at(
            deadline,
            transitions(&fixture, &flow, off, &origin, &mut protocol),
        )
        .await
        .map_err(|_| io::Error::other("warm transitions exceeded overall deadline"))??;
        let complete = completion(&protocol).await?;
        assert_eq!(
            calculate_addon_content_sha256(&fixture.root.path().join("package"))
                .await
                .unwrap(),
            package_hash
        );
        assert!(fixture.playout.try_recv().is_err());
        Ok::<_, io::Error>((current, complete))
    })
    .catch_unwind()
    .await;
    let observer_closed = observer.close().await;
    let origin_closed = origin.close().await;
    let cleanup = wait_for_cleanup().await;
    let captures = save_captures(&directory, &origin);
    let (current, complete) = match outcome {
        Ok(Ok(result)) => result,
        Ok(Err(error)) => {
            fixture.state.pool.close().await;
            panic!("warm trial did not complete: {error}");
        }
        Err(panic) => {
            fixture.state.pool.close().await;
            std::panic::resume_unwind(panic);
        }
    };
    // Pure raster comparisons are intentionally synchronous. Run them only
    // after the three paced producers have stopped; on this current-thread
    // test runtime they must not starve an unrelated response's100ms deadline.
    let recovered_day = recover(&fixture, 1).await;
    let recovered_night = recover(&fixture, 2).await;
    fixture.state.pool.close().await;
    observer_closed.unwrap();
    origin_closed.unwrap();
    cleanup.unwrap();
    captures.unwrap();
    generations::assert_same_output(&recovered_day, &current.accepted);
    generations::assert_same_output(&recovered_night, &flow.accepted[1]);
    verify_native(
        &directory,
        (&ffmpeg, &ffprobe, &tsanalyze),
        &source,
        &origin.state.captures,
    )
    .await;
    write_new(&directory.join("success.json"),&serde_json::to_vec_pretty(&json!({
        "probe_version":1,"scope":"three original ordinary TCP responses across five wire-observed stages",
        "source_sha256":hash(&source),"stages":protocol.receipts,"completion":complete,
        "final_generation_id":current.accepted.generation().id,"source_requests_per_viewer":1,
        "owned_server_and_observer_closed":true,"originals_receipts_packages_and_other_channel_unchanged":true,
        "visual_evidence":"Root-owned screenshots and actual rendered navigation must be inspected independently; capture markers are not visual acknowledgements.",
        "limitations":"Resource-free representative warm flow; no full engine, resource-cache, panel or platform acceptance."
    })).unwrap()).unwrap();
}
