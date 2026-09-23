//! Real HTTP framing and `FFmpeg` EOF proof for the production prepared response.
use super::*;
use std::{
    path::Path,
    process::Stdio,
    sync::{
        atomic::{AtomicUsize, Ordering},
        Arc,
    },
    time::Duration,
};
use tokio::process::Command;

fn apply_prepared_http_options(command: &mut Command, address: std::net::SocketAddr) {
    use ersatzrs_infra::next_engine::protocol::{ProbeHint, Source};
    let relay = MediaRelay::new();
    relay.configure(address).unwrap();
    let hint = ProbeHint {
        video: Vec::new(),
        audio: Vec::new(),
        subtitle: Vec::new(),
        format_name: Some("mpegts".into()),
        duration_ms: None,
    };
    let Source::Http {
        keep_alive,
        reconnect,
        ..
    } = relay
        .prepared_source(uuid::Uuid::new_v4(), uuid::Uuid::new_v4(), hint, false)
        .unwrap()
    else {
        panic!("prepared source must use HTTP");
    };
    assert_eq!(reconnect, Some(false));
    // Official Next f2a981b ffpipeline::input maps keep_alive to this option.
    if keep_alive == Some(true) {
        command.args(["-multiple_requests", "1"]);
    }
}

async fn run(mut command: Command, root: &Path, name: &str) -> (std::process::ExitStatus, String) {
    let stderr_path = root.join(format!("{name}.stderr"));
    command
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::from(std::fs::File::create(&stderr_path).unwrap()));
    let mut child = command.kill_on_drop(true).spawn().unwrap();
    let result = tokio::time::timeout(Duration::from_secs(20), child.wait()).await;
    let status = if let Ok(status) = result {
        status.unwrap()
    } else {
        let _ = child.start_kill();
        tokio::time::timeout(Duration::from_secs(5), child.wait())
            .await
            .unwrap()
            .unwrap();
        panic!("synthetic FFmpeg operation timed out");
    };
    assert!(std::fs::metadata(&stderr_path).unwrap().len() < 64 * 1024);
    (status, std::fs::read_to_string(stderr_path).unwrap())
}

#[tokio::test]
#[ignore = "requires FFmpeg 9 path ERSATZRS_NEXT_TEST_FFMPEG and loopback sockets"]
async fn real_prepared_http_response_reaches_clean_ffmpeg_eof() {
    #[allow(clippy::disallowed_methods)] // Explicit opt-in test runtime, not host configuration.
    let ffmpeg = std::env::var_os("ERSATZRS_NEXT_TEST_FFMPEG").expect("set FFmpeg 9 path");
    let root = tempfile::tempdir().unwrap();
    let source = root.path().join("source.ts");
    let mut generate = Command::new(&ffmpeg);
    generate
        .args([
            "-v",
            "error",
            "-nostdin",
            "-f",
            "lavfi",
            "-i",
            "color=blue:s=160x90:r=20:d=2",
            "-c:v",
            "libx264",
            "-preset",
            "ultrafast",
            "-threads",
            "1",
            "-f",
            "mpegts",
        ])
        .arg(&source);
    let (status, error) = run(generate, root.path(), "generate").await;
    assert!(status.success(), "fixture generation failed: {error}");
    let bytes = Arc::new(tokio::fs::read(source).await.unwrap());
    let requests = Arc::new(AtomicUsize::new(0));
    let counter = requests.clone();
    let app = axum::Router::new().route(
        "/prepared",
        axum::routing::get(move |method: Method, headers: HeaderMap| {
            let bytes = bytes.clone();
            let counter = counter.clone();
            async move {
                if let Err(status) = prepared_request(&method, &headers) {
                    return status.into_response();
                }
                if counter.fetch_add(1, Ordering::SeqCst) != 0 {
                    return StatusCode::GONE.into_response();
                }
                prepared_response(std::io::Cursor::new((*bytes).clone()))
            }
        }),
    );
    let listener = tokio::net::TcpListener::bind((std::net::Ipv4Addr::LOCALHOST, 0))
        .await
        .unwrap();
    let address = listener.local_addr().unwrap();
    let (stop, stopped) = tokio::sync::oneshot::channel();
    let server = tokio::spawn(async move {
        axum::serve(listener, app)
            .with_graceful_shutdown(async {
                let _ = stopped.await;
            })
            .await
            .unwrap();
    });
    let decoded = root.path().join("frames.rgb");
    let mut decode = Command::new(ffmpeg);
    apply_prepared_http_options(&mut decode, address);
    decode
        .args(["-v", "error", "-nostdin", "-i"])
        .arg(format!("http://{address}/prepared"))
        .args([
            "-map",
            "0:v:0",
            "-vf",
            "scale=1:1",
            "-pix_fmt",
            "rgb24",
            "-f",
            "rawvideo",
        ])
        .arg(&decoded);
    let result = run(decode, root.path(), "decode").await;
    let _ = stop.send(());
    tokio::time::timeout(Duration::from_secs(5), server)
        .await
        .unwrap()
        .unwrap();
    assert!(result.0.success(), "decoder failed: {}", result.1);
    assert!(
        result.1.is_empty(),
        "production HTTP response must reach clean EOF: {}",
        result.1
    );
    assert_eq!(requests.load(Ordering::SeqCst), 1);
    let pixels = tokio::fs::read(decoded).await.unwrap();
    assert_eq!(pixels.len(), 40 * 3, "all 40 frames must reach the decoder");
    assert!(pixels
        .chunks_exact(3)
        .all(|rgb| rgb[0] < 40 && rgb[1] < 40 && rgb[2] > 180));
}
