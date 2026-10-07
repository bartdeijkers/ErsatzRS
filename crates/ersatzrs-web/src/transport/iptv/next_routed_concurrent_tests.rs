//! Separate normal/preview producers through the real authenticated router.
use super::*;
use ersatzrs_infra::transcode_capacity::active_transcode_snapshot;

#[tokio::test]
#[ignore = "requires official Next and FFmpeg; isolated concurrent routed producers"]
async fn real_next_routed_normal_preview_have_independent_producer_lifetimes() {
    let root = tempfile::tempdir().unwrap();
    let mut command = Command::new(std::env::current_exe().unwrap());
    command.args(["--exact", "transport::iptv::next_media::routed_runtime_tests::concurrent::fixture_child_concurrent_producers", "--ignored", "--nocapture"])
        .env(ersatzrs_infra::file_system_layout::APP_DATA_DIR_ENV_VAR, root.path())
        .env(ersatzrs_infra::file_system_layout::APP_STATE_DIR_ENV_VAR, root.path().join("state"));
    assert!(String::from_utf8_lossy(&tool(command).await).contains("1 passed"));
}

#[tokio::test]
#[ignore = "isolated child of real_next_routed_normal_preview_have_independent_producer_lifetimes"]
async fn fixture_child_concurrent_producers() {
    let root = tempfile::tempdir().unwrap();
    let ffmpeg = runtime_path("ERSATZRS_NEXT_TEST_FFMPEG");
    let ffprobe = runtime_path("ERSATZRS_NEXT_TEST_FFPROBE");
    ersatzrs_ffmpeg::set_resolved_ffmpeg_tools(ffmpeg.to_str().unwrap(), ffprobe.to_str().unwrap());
    fixture::generate(&ffmpeg, root.path()).await;
    // A constant blue source isolates lifecycle assertions from the different
    // wall-clock joins. Source-owned seek has its own routed regression.
    let mut blue = Command::new(&ffmpeg);
    blue.args(["-v", "error", "-nostdin", "-i"])
        .arg(root.path().join("source.mkv"))
        .args([
            "-map",
            "0",
            "-vf",
            "drawbox=color=blue:t=fill",
            "-c:v",
            "libx264",
            "-preset",
            "ultrafast",
            "-g",
            "25",
            "-threads",
            "2",
            "-c:a",
            "copy",
        ])
        .arg(root.path().join("blue.mkv"));
    tool(blue).await;
    tokio::fs::rename(root.path().join("blue.mkv"), root.path().join("source.mkv"))
        .await
        .unwrap();
    let pool = ersatzrs_infra::db::init(&format!(
        "sqlite:{}",
        root.path().join("fixture.db").display()
    ))
    .await
    .unwrap();
    fixture::seed(&pool, root.path()).await;
    sqlx::query(r#"INSERT INTO "ConfigElement" ("Key","Value") VALUES (?, '2') ON CONFLICT("Key") DO UPDATE SET "Value"='2'"#)
        .bind(ersatzrs_domain::CONFIG_KEY_FFMPEG_ACTIVE_TRANSCODE_LIMIT).execute(&pool).await.unwrap();
    run(
        state(pool, root.path()),
        root.path().to_owned(),
        ffmpeg,
        ffprobe,
    )
    .await;
}

async fn run(state: AppState, root: PathBuf, ffmpeg: PathBuf, ffprobe: PathBuf) {
    let (tx, mut rx) = mpsc::unbounded_channel();
    let observation = Arc::new(Observation {
        first_prepared: AtomicBool::new(true),
        requests: tx,
        release: tokio::sync::Notify::new(),
    });
    let (address, stop, server) = start_server(&state, observation).await;
    let normal = root.join("normal");
    let preview = root.join("preview");
    let request = NextStart {
        channel_id: 1,
        preview: false,
        number: "1".into(),
        executable: runtime_path("ERSATZRS_NEXT_TEST_BIN"),
        output: normal.clone(),
        ffmpeg: ffmpeg.clone(),
        ffprobe,
        replacements: Vec::new(),
        idle_timeout: None,
        deadline: Instant::now() + Duration::from_secs(30),
    };
    let normal_task = state
        .next_engine
        .start(state.hls_sessions.clone(), request.clone())
        .await
        .unwrap();
    let normal_transfer = prepared_request(&mut rx).await;
    let normal_pid = tokio::fs::read_to_string(root.join("producer-pid"))
        .await
        .unwrap();
    let normal_pid = normal_pid.trim().to_owned();
    let mut preview_request = request;
    preview_request.preview = true;
    preview_request.output.clone_from(&preview);
    preview_request.deadline = Instant::now() + Duration::from_secs(30);
    let preview_task = state
        .next_engine
        .start(state.preview_hls_sessions.clone(), preview_request)
        .await
        .unwrap();
    let preview_transfer = prepared_request(&mut rx).await;
    let preview_pid = tokio::fs::read_to_string(root.join("producer-pid"))
        .await
        .unwrap();
    let preview_pid = preview_pid.trim().to_owned();
    let checks = tokio::spawn(verify_independence(
        state.clone(),
        root.clone(),
        address,
        normal_transfer,
        preview_transfer,
        (normal_pid, preview_pid),
        preview_task,
    ));
    let result = checks.await;
    state.preview_hls_sessions.stop_channel("1").await;
    state.hls_sessions.stop_channel("1").await;
    timeout(Duration::from_secs(15), normal_task)
        .await
        .unwrap()
        .unwrap();
    let _ = stop.send(());
    timeout(Duration::from_secs(10), server)
        .await
        .unwrap()
        .unwrap();
    let (normal_pid, preview_pid) = result.expect("independent routed producer lifetimes");
    assert!(!producer_alive(&normal_pid).await && !producer_alive(&preview_pid).await);
    assert_eq!(active_transcode_snapshot(2).active, 0);
    assert!(!normal.exists() && !preview.exists());
    assert_eq!(
        tokio::fs::read_to_string(root.join("producer-starts"))
            .await
            .unwrap(),
        "play\nplay\n"
    );
    decode(&ffmpeg, &root.join("normal.ts")).await;
    decode(&ffmpeg, &root.join("preview.ts")).await;
}

async fn verify_independence(
    state: AppState,
    root: PathBuf,
    address: std::net::SocketAddr,
    normal_transfer: Captured,
    preview_transfer: Captured,
    (normal_pid, preview_pid): (String, String),
    preview_task: tokio::task::JoinHandle<()>,
) -> (String, String) {
    let normal = root.join("normal");
    let preview = root.join("preview");
    assert_ne!(normal_pid, preview_pid);
    assert_ne!(normal_transfer.path, preview_transfer.path);
    assert_eq!(active_transcode_snapshot(2).active, 2);
    capture(&state, &normal, &root.join("normal.ts")).await;
    let mut preview_state = state.clone();
    preview_state.hls_sessions = state.preview_hls_sessions.clone();
    capture(&preview_state, &preview, &root.join("preview.ts")).await;
    assert!(producer_alive(&normal_pid).await && producer_alive(&preview_pid).await);
    assert!(state.preview_hls_sessions.stop_channel("1").await);
    timeout(Duration::from_secs(15), preview_task)
        .await
        .unwrap()
        .unwrap();
    assert!(!producer_alive(&preview_pid).await);
    assert!(producer_alive(&normal_pid).await);
    assert_eq!(active_transcode_snapshot(2).active, 1);
    assert!(!preview.exists());
    assert!(normal.exists());
    assert!(state
        .hls_sessions
        .try_get("1")
        .await
        .unwrap()
        .trim_playlist()
        .await
        .is_some());
    let client = reqwest::Client::builder()
        .timeout(Duration::from_secs(5))
        .build()
        .unwrap();
    assert_eq!(
        probe(
            &client,
            &format!("http://{address}"),
            &preview_transfer,
            Method::GET,
            preview_transfer.headers.clone(),
            None
        )
        .await,
        StatusCode::GONE
    );
    (normal_pid, preview_pid)
}

async fn prepared_request(rx: &mut mpsc::UnboundedReceiver<Captured>) -> Captured {
    timeout(Duration::from_secs(15), async {
        loop {
            let captured = rx.recv().await.unwrap();
            if captured.path.starts_with("/ffmpeg/next-prepared/") {
                break captured;
            }
        }
    })
    .await
    .unwrap()
}
