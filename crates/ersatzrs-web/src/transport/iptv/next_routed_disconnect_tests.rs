//! A real HTTP client disconnect must cancel its owned installed producer.
use super::*;

#[tokio::test]
#[ignore = "requires official Next and FFmpeg; isolated routed disconnect fixture"]
async fn real_next_routed_body_disconnect_reaps_producer() {
    let root = tempfile::tempdir().unwrap();
    let mut command = Command::new(std::env::current_exe().unwrap());
    command.args(["--exact", "transport::iptv::next_media::routed_runtime_tests::disconnect::fixture_child_disconnect", "--ignored", "--nocapture"])
        .env(ersatzrs_infra::file_system_layout::APP_DATA_DIR_ENV_VAR, root.path())
        .env(ersatzrs_infra::file_system_layout::APP_STATE_DIR_ENV_VAR, root.path().join("state"));
    assert!(String::from_utf8_lossy(&tool(command).await).contains("1 passed"));
}

#[tokio::test]
#[ignore = "isolated child of real_next_routed_body_disconnect_reaps_producer"]
async fn fixture_child_disconnect() {
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
    run(state(pool, root.path()), root.path(), ffmpeg, ffprobe).await;
}

async fn run(state: AppState, root: &Path, ffmpeg: PathBuf, ffprobe: PathBuf) {
    let (tx, mut rx) = mpsc::unbounded_channel();
    let observation = Arc::new(Observation {
        first_prepared: AtomicBool::new(false),
        requests: tx,
        release: tokio::sync::Notify::new(),
    });
    let (address, stop, server) = start_server(&state, observation.clone()).await;
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
                ffmpeg,
                ffprobe,
                replacements: Vec::new(),
                idle_timeout: None,
                deadline: Instant::now() + Duration::from_secs(30),
            },
        )
        .await
        .unwrap();
    let checks_root = root.to_owned();
    let checks = tokio::spawn(async move {
        let prepared = timeout(Duration::from_secs(15), async {
            loop {
                let request = rx.recv().await.unwrap();
                if request.path.starts_with("/ffmpeg/next-prepared/") {
                    break request;
                }
            }
        })
        .await
        .unwrap();
        disconnect(address, prepared, &checks_root).await;
    });
    let result = checks.await;
    // Release Next only after the HTTP disconnect has independently reaped the
    // producer. Session stop must not be the cause of that assertion passing.
    observation.release.notify_one();
    state.hls_sessions.stop_channel("1").await;
    timeout(Duration::from_secs(15), task)
        .await
        .unwrap()
        .unwrap();
    let _ = stop.send(());
    timeout(Duration::from_secs(10), server)
        .await
        .unwrap()
        .unwrap();
    result.expect("HTTP disconnect cleanup");
    assert!(!output.exists());
    assert_eq!(
        ersatzrs_infra::transcode_capacity::active_transcode_snapshot(1).active,
        0
    );
    assert_eq!(
        tokio::fs::read_to_string(root.join("producer-starts"))
            .await
            .unwrap(),
        "play\n"
    );
}

async fn disconnect(address: std::net::SocketAddr, prepared: Captured, root: &Path) {
    let pid = tokio::fs::read_to_string(root.join("producer-pid"))
        .await
        .unwrap();
    let pid = pid.trim();
    assert!(producer_alive(pid).await);
    let client = reqwest::Client::builder()
        .timeout(Duration::from_secs(5))
        .build()
        .unwrap();
    let mut response = client
        .get(format!("http://{address}{}", prepared.path))
        .headers(prepared.headers.clone())
        .header("x-fixture-probe", "1")
        .header(header::CONNECTION, "close")
        .send()
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::OK);
    assert!(!response.chunk().await.unwrap().unwrap().is_empty());
    assert!(producer_alive(pid).await);
    drop(response);
    drop(client);
    timeout(Duration::from_secs(5), async {
        while producer_alive(pid).await {
            tokio::time::sleep(Duration::from_millis(10)).await;
        }
    })
    .await
    .expect("HTTP body disconnect must reap producer before session stop");
    let client = reqwest::Client::builder()
        .timeout(Duration::from_secs(5))
        .build()
        .unwrap();
    assert_eq!(
        probe(
            &client,
            &format!("http://{address}"),
            &prepared,
            Method::GET,
            prepared.headers.clone(),
            None
        )
        .await,
        StatusCode::GONE
    );
}
