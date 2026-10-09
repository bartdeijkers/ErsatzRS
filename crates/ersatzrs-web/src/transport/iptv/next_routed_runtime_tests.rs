//! Opt-in real router, installed producer and official Next lifecycle proof.
//! Unix-only because the disposable add-on uses a POSIX shell entrypoint.
use super::*;
use ersatzrs_core::hls::HlsStartupReadiness;
use ersatzrs_infra::next_engine::host::{NextHost, NextStart};
use std::{
    path::{Path, PathBuf},
    process::Stdio,
    sync::{
        atomic::{AtomicBool, Ordering},
        Arc,
    },
    time::Duration,
};
use tokio::{
    process::Command,
    sync::{mpsc, oneshot},
    time::{timeout, Instant},
};

#[path = "next_routed_concurrent_tests.rs"]
mod concurrent;
#[path = "next_routed_disconnect_tests.rs"]
mod disconnect;
#[path = "next_routed_fixture.rs"]
pub(crate) mod fixture;

#[allow(clippy::disallowed_methods)] // Explicit opt-in runtime locations only.
fn runtime_path(name: &str) -> PathBuf {
    std::env::var_os(name)
        .map(PathBuf::from)
        .expect("set Next runtime test variables")
}

async fn tool(mut command: Command) -> Vec<u8> {
    let errors = tempfile::NamedTempFile::new().unwrap();
    command
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(errors.reopen().unwrap());
    let mut child = command.kill_on_drop(true).spawn().unwrap();
    let mut stdout = child.stdout.take().unwrap().take(128 * 1024 + 1);
    let result = timeout(Duration::from_secs(45), async {
        let mut bytes = Vec::new();
        stdout.read_to_end(&mut bytes).await.unwrap();
        let status = child.wait().await.unwrap();
        (bytes, status)
    })
    .await;
    let Ok((bytes, status)) = result else {
        let _ = child.start_kill();
        let _ = timeout(Duration::from_secs(10), child.wait()).await;
        panic!("fixture process timed out");
    };
    assert!(std::fs::metadata(errors.path()).unwrap().len() < 128 * 1024);
    assert!(
        status.success(),
        "fixture process failed: {}",
        std::fs::read_to_string(errors.path()).unwrap()
    );
    assert!(bytes.len() <= 128 * 1024);
    bytes
}

#[tokio::test]
#[ignore = "requires official Next and FFmpeg; isolated real web route fixture"]
async fn real_next_routed_producer_auth_claim_and_cleanup() {
    let root = tempfile::tempdir().unwrap();
    let mut command = Command::new(std::env::current_exe().unwrap());
    command
        .args([
            "--exact",
            "transport::iptv::next_media::routed_runtime_tests::fixture_child_routed_producer",
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
        );
    let output = tool(command).await;
    assert!(String::from_utf8_lossy(&output).contains("1 passed"));
}

// This value deliberately has no Debug implementation: it includes a capability.
struct Captured {
    path: String,
    headers: HeaderMap,
}

struct Observation {
    first_prepared: AtomicBool,
    requests: mpsc::UnboundedSender<Captured>,
    release: tokio::sync::Notify,
}

async fn observe(
    State(observation): State<Arc<Observation>>,
    request: axum::extract::Request,
    next: axum::middleware::Next,
) -> Response {
    let path = request.uri().path();
    let prepared = path.starts_with("/ffmpeg/next-prepared/");
    let original = request.headers().get("x-fixture-probe").is_none();
    if original && (prepared || path.starts_with("/ffmpeg/next-item/")) {
        let _ = observation.requests.send(Captured {
            path: path.into(),
            headers: request.headers().clone(),
        });
        if prepared && !observation.first_prepared.swap(true, Ordering::SeqCst) {
            // Bounded even when the assertion task fails or is cancelled.
            let _ = timeout(Duration::from_secs(10), observation.release.notified()).await;
        }
    }
    next.run(request).await
}

fn state(pool: ersatzrs_infra::db::DbPool, root: &Path) -> AppState {
    use crate::host::state::{IptvAccessTokenVerifier, SystemStartup};
    use ersatzrs_infra::db::{DatabaseBackend, DatabaseSchemaStatus};
    let locker = ersatzrs_core::locking::EntityLocker::default();
    let (tx, _rx) = mpsc::unbounded_channel();
    let mut state = AppState::new(
        pool.clone(),
        DatabaseBackend::Sqlite,
        SystemStartup {
            database_ready: true,
            schema_status: DatabaseSchemaStatus::default(),
        },
        ersatzrs_ffmpeg::FfmpegRuntimeReport::for_tests(),
        uuid::Uuid::nil(),
        1,
        IptvAccessTokenVerifier::disabled(),
        tx,
        locker.clone(),
    );
    state.next_engine = Arc::new(NextHost::new(pool, locker, root.join("playout")));
    state
}

async fn probe(
    client: &reqwest::Client,
    base: &str,
    captured: &Captured,
    method: Method,
    headers: HeaderMap,
    path: Option<&str>,
) -> StatusCode {
    client
        .request(method, format!("{base}{}", path.unwrap_or(&captured.path)))
        .headers(headers)
        .header("x-fixture-probe", "1")
        .send()
        .await
        .unwrap()
        .status()
}

async fn rejection_checks(
    client: &reqwest::Client,
    base: &str,
    item: &Captured,
    prepared: &Captured,
) {
    assert_eq!(
        probe(client, base, item, Method::GET, HeaderMap::new(), None).await,
        StatusCode::UNAUTHORIZED
    );
    let mut stale = item.headers.clone();
    stale.insert(
        SESSION_HEADER,
        uuid::Uuid::new_v4().to_string().parse().unwrap(),
    );
    assert_eq!(
        probe(client, base, item, Method::GET, stale, None).await,
        StatusCode::SERVICE_UNAVAILABLE
    );
    assert_eq!(
        probe(client, base, prepared, Method::GET, HeaderMap::new(), None).await,
        StatusCode::UNAUTHORIZED
    );
    assert_eq!(
        probe(
            client,
            base,
            prepared,
            Method::HEAD,
            prepared.headers.clone(),
            None
        )
        .await,
        StatusCode::METHOD_NOT_ALLOWED
    );
    let mut range = prepared.headers.clone();
    range.insert(header::RANGE, "bytes=1-".parse().unwrap());
    assert_eq!(
        probe(client, base, prepared, Method::GET, range, None).await,
        StatusCode::RANGE_NOT_SATISFIABLE
    );
    let transfer = prepared.path.rsplit('/').next().unwrap();
    let wrong = format!("/ffmpeg/next-prepared/{}/{transfer}", uuid::Uuid::new_v4());
    assert_eq!(
        probe(
            client,
            base,
            prepared,
            Method::GET,
            prepared.headers.clone(),
            Some(&wrong)
        )
        .await,
        StatusCode::GONE
    );
}

async fn capture(state: &AppState, output: &Path, saved: &Path) {
    let handle = state.hls_sessions.try_get("1").await.unwrap();
    loop {
        match handle.startup_readiness() {
            HlsStartupReadiness::Ready => break,
            HlsStartupReadiness::Failed => {
                panic!("routed {:?} session failed", output.file_name());
            }
            HlsStartupReadiness::Legacy => panic!("routed session must use Next readiness"),
            HlsStartupReadiness::Pending { deadline } => {
                // Honor the admitted startup deadline, including the time
                // needed for Next's four complete real-time HLS segments.
                tokio::time::sleep_until(
                    (Instant::now() + Duration::from_millis(10)).min(deadline.into()),
                )
                .await;
            }
        }
    }
    let playlist = handle.trim_playlist().await.unwrap();
    let segment = playlist
        .lines()
        .find(|line| {
            !line.starts_with('#')
                && Path::new(line)
                    .extension()
                    .is_some_and(|ext| ext.eq_ignore_ascii_case("ts"))
        })
        .unwrap();
    tokio::fs::copy(output.join(segment), saved).await.unwrap();
}

async fn decode(ffmpeg: &Path, segment: &Path) {
    let mut video = Command::new(ffmpeg);
    video
        .args(["-v", "error", "-nostdin", "-i"])
        .arg(segment)
        .args([
            "-ss",
            "0.5",
            "-map",
            "0:v:0",
            "-frames:v",
            "1",
            "-vf",
            "scale=1:1",
            "-pix_fmt",
            "rgb24",
            "-f",
            "rawvideo",
            "pipe:1",
        ]);
    let pixels = tool(video).await;
    assert_eq!(pixels.len(), 3);
    assert!(
        pixels[2] > 180 && pixels[0] < 70 && pixels[1] < 70,
        "source-owned seek must decode blue"
    );
    let mut audio = Command::new(ffmpeg);
    audio
        .args(["-v", "error", "-nostdin", "-i"])
        .arg(segment)
        .args([
            "-map", "0:a:0", "-t", "1", "-ac", "1", "-ar", "8000", "-f", "s16le", "pipe:1",
        ]);
    let bytes = tool(audio).await;
    let samples: Vec<_> = bytes
        .chunks_exact(2)
        .map(|b| i16::from_le_bytes([b[0], b[1]]))
        .collect();
    assert!(samples.len() >= 7000);
    let crossings = samples.windows(2).filter(|p| p[0] <= 0 && p[1] > 0).count();
    let hz = f64::from(u32::try_from(crossings).unwrap()) * 8000.0
        / f64::from(u32::try_from(samples.len()).unwrap());
    assert!(
        (820.0..940.0).contains(&hz),
        "Dutch audio must survive the web route"
    );
}

#[tokio::test]
#[ignore = "isolated child of real_next_routed_producer_auth_claim_and_cleanup"]
async fn fixture_child_routed_producer() {
    let root = tempfile::tempdir().unwrap();
    let ffmpeg = runtime_path("ERSATZRS_NEXT_TEST_FFMPEG");
    let ffprobe = runtime_path("ERSATZRS_NEXT_TEST_FFPROBE");
    ersatzrs_ffmpeg::set_resolved_ffmpeg_tools(ffmpeg.to_str().unwrap(), ffprobe.to_str().unwrap());
    fixture::generate(&ffmpeg, root.path()).await;
    let pool = ersatzrs_infra::db::init(&format!(
        "sqlite:{}",
        root.path().join("fixture.db").display()
    ))
    .await
    .unwrap();
    fixture::seed(&pool, root.path()).await;
    let state = state(pool, root.path());
    run_routed(&state, root.path(), &ffmpeg, ffprobe).await;
}

async fn run_routed(state: &AppState, root: &Path, ffmpeg: &Path, ffprobe: PathBuf) {
    let (tx, rx) = mpsc::unbounded_channel();
    let observation = Arc::new(Observation {
        first_prepared: AtomicBool::new(false),
        requests: tx,
        release: tokio::sync::Notify::new(),
    });
    let (address, stop, server) = start_server(state, observation.clone()).await;
    let output = root.join("output");
    let task = state
        .next_engine
        .start(
            state.hls_sessions.clone(),
            NextStart {
                channel_id: 1,
                preview: false,
                number: "1".into(),
                executable: runtime_path("ERSATZRS_NEXT_TEST_BIN"),
                output: output.clone(),
                ffmpeg: ffmpeg.into(),
                ffprobe,
                replacements: Vec::new(),
                idle_timeout: None,
                deadline: Instant::now() + Duration::from_secs(30),
            },
        )
        .await
        .expect("routed session admission");
    // Assertions run in a task so cleanup still executes after a test failure.
    let saved = root.join("proof.ts");
    let checks = tokio::spawn(check_routes(
        state.clone(),
        output.clone(),
        saved.clone(),
        observation.clone(),
        rx,
        address,
    ));
    let result = checks.await;
    observation.release.notify_one();
    let producer_pid = tokio::fs::read_to_string(root.join("producer-pid"))
        .await
        .ok();
    let producer_pid = producer_pid
        .as_deref()
        .map(str::trim)
        .filter(|pid| !pid.is_empty() && pid.bytes().all(|b| b.is_ascii_digit()));
    let alive_before_stop = if let Some(pid) = producer_pid {
        producer_alive(pid).await
    } else {
        false
    };
    let stopped_session = state.hls_sessions.stop_channel("1").await;
    timeout(Duration::from_secs(15), task)
        .await
        .unwrap()
        .unwrap();
    let after_stop = if let Ok(prepared) = &result {
        Some(
            probe(
                &reqwest::Client::builder()
                    .timeout(Duration::from_secs(5))
                    .build()
                    .unwrap(),
                &format!("http://{address}"),
                prepared,
                Method::GET,
                prepared.headers.clone(),
                None,
            )
            .await,
        )
    } else {
        None
    };
    let _ = stop.send(());
    timeout(Duration::from_secs(10), server)
        .await
        .unwrap()
        .unwrap();
    result.expect("routed acceptance checks");
    assert_eq!(after_stop, Some(StatusCode::GONE));
    assert!(stopped_session);
    assert!(
        alive_before_stop,
        "producer must still be running when stop begins"
    );
    assert!(
        !producer_alive(producer_pid.unwrap()).await,
        "stop must reap the producer"
    );
    assert!(!output.exists());
    assert_eq!(
        tokio::fs::read_to_string(root.join("producer-starts"))
            .await
            .unwrap(),
        "play\n"
    );
    decode(ffmpeg, &saved).await;
}

async fn producer_alive(pid: &str) -> bool {
    timeout(
        Duration::from_secs(3),
        Command::new("kill")
            .args(["-0", pid])
            .stdin(Stdio::null())
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .kill_on_drop(true)
            .status(),
    )
    .await
    .unwrap()
    .unwrap()
    .success()
}

async fn check_routes(
    checks_state: AppState,
    checks_output: PathBuf,
    checks_saved: PathBuf,
    checks_observation: Arc<Observation>,
    mut rx: mpsc::UnboundedReceiver<Captured>,
    address: std::net::SocketAddr,
) -> Captured {
    let client = reqwest::Client::builder()
        .timeout(Duration::from_secs(5))
        .build()
        .unwrap();
    let base = format!("http://{address}");
    let item = timeout(Duration::from_secs(15), rx.recv())
        .await
        .unwrap()
        .unwrap();
    assert!(item.path.starts_with("/ffmpeg/next-item/"));
    let prepared = timeout(Duration::from_secs(15), rx.recv())
        .await
        .unwrap()
        .unwrap();
    assert!(prepared.path.starts_with("/ffmpeg/next-prepared/"));
    rejection_checks(&client, &base, &item, &prepared).await;
    checks_observation.release.notify_one();
    capture(&checks_state, &checks_output, &checks_saved).await;
    assert_eq!(
        probe(
            &client,
            &base,
            &prepared,
            Method::GET,
            prepared.headers.clone(),
            None
        )
        .await,
        StatusCode::GONE
    );
    prepared
}

async fn start_server(
    state: &AppState,
    observation: Arc<Observation>,
) -> (
    std::net::SocketAddr,
    oneshot::Sender<()>,
    tokio::task::JoinHandle<()>,
) {
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let address = listener.local_addr().unwrap();
    state.next_engine.media.configure(address).unwrap();
    let app = crate::host::router::build_internal_app(state.clone()).layer(
        axum::middleware::from_fn_with_state(observation.clone(), observe),
    );
    let (stop, stopped) = oneshot::channel();
    let server = tokio::spawn(async move {
        axum::serve(
            listener,
            app.into_make_service_with_connect_info::<std::net::SocketAddr>(),
        )
        .with_graceful_shutdown(async {
            let _ = stopped.await;
        })
        .await
        .unwrap();
    });
    (address, stop, server)
}
