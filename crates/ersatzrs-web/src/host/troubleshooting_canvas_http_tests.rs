//! Unix-only process observation uses a disposable POSIX wrapper around the pinned `FFmpeg`.
//! It records only this fixture's PID/cwd; rendering still runs the published executable.
use super::*;
use axum::http::{HeaderMap, HeaderName, StatusCode};
use ersatzrs_infra::next_engine::protocol::Source;
use std::os::unix::fs::PermissionsExt as _;
use std::process::Stdio;

#[tokio::test]
#[ignore = "requires pinned FFmpeg; real private HTTP canvas lifecycle"]
async fn canvas_http_decodes_alpha_and_reaps_on_disconnect_and_session_stop() {
    let fixture = Fixture::new().await;
    let ffmpeg = runtime_path("ERSATZRS_NEXT_TEST_FFMPEG");
    let ffprobe = runtime_path("ERSATZRS_NEXT_TEST_FFPROBE");
    ersatzrs_ffmpeg::set_resolved_ffmpeg_tools(ffmpeg.to_str().unwrap(), ffprobe.to_str().unwrap());
    let wrapper = wrapper(fixture.root.path(), &ffmpeg).await;
    {
        let mut runtime = fixture.state.ffmpeg_runtime.write().await;
        runtime.ffmpeg.path = wrapper.to_str().unwrap().into();
        runtime.ffprobe.path = ffprobe.to_str().unwrap().into();
    }
    let mut query = Fixture::query();
    query.seek_seconds = Some(0);
    let origin = remote_runtime::Origin::start(&fixture, remote_runtime::Mode::Local).await;
    canvas_runtime::install(&fixture, &mut query).await;
    let paths = fixture.paths();
    let mut session = None;
    let (manifest, _, _, _) = prepare(
        &fixture.state,
        &query,
        &paths,
        Instant::now() + Duration::from_secs(30),
        &mut session,
    )
    .await
    .unwrap();
    let mut files = tokio::fs::read_dir(manifest.path()).await.unwrap();
    let path = files.next_entry().await.unwrap().unwrap().path();
    let playout: protocol::Playout =
        serde_json::from_slice(&tokio::fs::read(path).await.unwrap()).unwrap();
    let source = &playout.items[0].graphics[0].source;
    let client = new_client();
    let response = request(&client, source, "2500", "1000").await;
    let bytes = response.bytes().await.unwrap();
    assert!(bytes.len() < 4 * 1024 * 1024);
    let first = producer(fixture.root.path()).await;
    reaped(&first).await;
    let nut = fixture.root.path().join("canvas.nut");
    tokio::fs::write(&nut, bytes).await.unwrap();
    assert_alpha(&ffmpeg, &nut).await;

    // A 30-second producer cannot finish naturally within the cleanup deadline.
    // Observe disconnect cleanup before revoking the session.
    let mut response = request(&client, source, "0", "30000").await;
    assert!(!response.chunk().await.unwrap().unwrap().is_empty());
    let disconnected = producer(fixture.root.path()).await;
    assert!(disconnected.directory.exists());
    assert!(alive(&disconnected.pid).await);
    drop(response);
    drop(client);
    reaped(&disconnected).await;

    // Keep the HTTP body open while session shutdown owns cancellation/reaping.
    let client = new_client();
    let mut response = request(&client, source, "0", "30000").await;
    assert!(!response.chunk().await.unwrap().unwrap().is_empty());
    let stopped = producer(fixture.root.path()).await;
    assert!(alive(&stopped.pid).await);
    fixture
        .state
        .next_engine
        .finish_troubleshooting_session(session.unwrap())
        .await
        .unwrap();
    reaped(&stopped).await;
    drop(response);
    origin.verify_revoked_and_stop().await;
}

fn new_client() -> reqwest::Client {
    reqwest::Client::builder()
        .timeout(Duration::from_secs(10))
        .build()
        .unwrap()
}

async fn request(
    client: &reqwest::Client,
    source: &Source,
    offset: &str,
    duration: &str,
) -> reqwest::Response {
    let Source::Http { uri, headers, .. } = source else {
        panic!("canvas HTTP source")
    };
    let mut map = HeaderMap::new();
    for header in headers {
        let (name, value) = header.split_once(": ").unwrap();
        map.insert(name.parse::<HeaderName>().unwrap(), value.parse().unwrap());
    }
    let response = client
        .get(uri)
        .headers(map)
        .header("x-etv-offset-ms", offset)
        .header("x-etv-duration-ms", duration)
        .header("x-etv-frame-rate", "4")
        .header("connection", "close")
        .send()
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::OK);
    response
}

async fn wrapper(root: &FsPath, ffmpeg: &FsPath) -> PathBuf {
    let quoted = |path: &FsPath| format!("'{}'", path.to_str().unwrap().replace('\'', "'\\''"));
    let script = format!(
        "#!/bin/sh\nset -eu\nprintf '%s\\n%s\\n' \"$$\" \"$PWD\" > {}\nexec {} \"$@\"\n",
        quoted(&root.join("canvas-process")),
        quoted(ffmpeg)
    );
    let path = root.join("ffmpeg-fixture.sh");
    tokio::fs::write(&path, script).await.unwrap();
    tokio::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o755))
        .await
        .unwrap();
    path
}

struct Producer {
    pid: String,
    directory: PathBuf,
}

async fn producer(root: &FsPath) -> Producer {
    let log = tokio::fs::read_to_string(root.join("canvas-process"))
        .await
        .unwrap();
    let mut lines = log.lines();
    let pid = lines.next().unwrap().to_owned();
    assert!(pid.parse::<u32>().unwrap() > 0);
    let directory = PathBuf::from(lines.next().unwrap());
    assert!(directory
        .file_name()
        .unwrap()
        .to_str()
        .unwrap()
        .starts_with("ersatzrs-canvas-"));
    Producer { pid, directory }
}

async fn alive(pid: &str) -> bool {
    Command::new("kill")
        .args(["-0", pid])
        .stderr(Stdio::null())
        .status()
        .await
        .unwrap()
        .success()
}

async fn reaped(producer: &Producer) {
    tokio::time::timeout(Duration::from_secs(5), async {
        while producer.directory.exists() || alive(&producer.pid).await {
            tokio::time::sleep(Duration::from_millis(20)).await;
        }
    })
    .await
    .expect("canvas process and generated artifacts must be removed");
}

async fn assert_alpha(ffmpeg: &FsPath, nut: &FsPath) {
    let mut command = Command::new(ffmpeg);
    command
        .args(["-v", "error", "-nostdin", "-i"])
        .arg(nut)
        .args(["-pix_fmt", "bgra", "-f", "rawvideo", "pipe:1"]);
    let decoded = output(command).await;
    assert_eq!(decoded.len(), 320 * 180 * 4 * 4);
    for frame in decoded.chunks_exact(320 * 180 * 4) {
        let pixel = |x: usize, y: usize| &frame[(y * 320 + x) * 4..(y * 320 + x) * 4 + 4];
        assert_eq!(pixel(80, 24)[3], 0, "background must be transparent");
        assert_eq!(pixel(16, 16), [0, 0, 255, 255], "script alpha");
        assert_eq!(pixel(32, 32), [0, 255, 0, 255], "image alpha and order");
        assert!(pixel(136, 8)[3] > 240, "motion alpha");
        assert!(pixel(228, 16)[3] > 240, "ASS alpha");
        for (left, right) in [(8, 50), (128, 210)] {
            let visible = (60..90)
                .flat_map(|y| (left..right).map(move |x| pixel(x, y)))
                .filter(|pixel| pixel[3] > 200)
                .count();
            assert!(visible > 10, "text backend alpha");
        }
    }
}
