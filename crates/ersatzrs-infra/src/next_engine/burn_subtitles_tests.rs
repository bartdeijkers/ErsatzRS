use super::*;

fn local() -> Source {
    Source::Local {
        path: "synthetic/source.mkv".into(),
        in_point_ms: Some(12_000),
        out_point_ms: Some(60_000),

        probe_hint: None,
    }
}

fn strings(args: Vec<OsString>) -> Vec<String> {
    args.into_iter()
        .map(|arg| arg.into_string().unwrap())
        .collect()
}

#[test]
fn selected_track_preserves_timeline_and_ass_style_with_explicit_output_format() {
    for (codec, format) in [
        ("ass", "ass"),
        ("SSA", "ass"),
        ("subrip", "srt"),
        ("mov_text", "srt"),
        ("webvtt", "srt"),
    ] {
        let args =
            strings(arguments(&local(), 4, codec, Path::new("synthetic/pending.tmp")).unwrap());
        assert!(args.iter().any(|arg| arg == "-copyts"));
        assert!(!args
            .iter()
            .any(|arg| matches!(arg.as_str(), "-ss" | "-t" | "-to" | "-start_at_zero")));
        assert!(args.windows(2).any(|pair| pair == ["-map", "0:4"]));
        assert!(args.windows(2).any(|pair| pair == ["-c:s", format]));
        assert!(args.windows(2).any(|pair| pair == ["-f", format]));
        assert_eq!(args.last().unwrap(), "synthetic/pending.tmp");
    }
}

#[test]
fn http_input_preserves_transport_options_before_exact_input() {
    let source = Source::Http {
        probe_hint: None,
        is_live: None,
        uri: "http://127.0.0.1:8409/synthetic".into(),
        headers: vec!["X-Fixture: synthetic".into(), "X-Other: value".into()],
        user_agent: Some("synthetic-agent".into()),
        keep_alive: None,
        reconnect: Some(true),
        reconnect_delay_max: Some(2),
        timeout_us: Some(500_000),
        in_point_ms: Some(4_000),
        out_point_ms: Some(8_000),
    };
    let args = strings(arguments(&source, 3, "srt", Path::new("pending")).unwrap());
    let input = args.iter().position(|arg| arg == "-i").unwrap();
    for pair in [
        ["-headers", "X-Fixture: synthetic\r\nX-Other: value\r\n"],
        ["-user_agent", "synthetic-agent"],
        ["-timeout", "500000"],
        ["-reconnect_delay_max", "2"],
        ["-reconnect", "1"],
    ] {
        assert!(args[..input].windows(2).any(|actual| actual == pair));
    }
    assert_eq!(args[input + 1], "http://127.0.0.1:8409/synthetic");
}

#[tokio::test]
async fn expired_deadline_does_not_launch_or_create_output() {
    let root = tempfile::tempdir().unwrap();
    let output = root.path().join("pending");
    assert_eq!(
        extract(
            &local(),
            4,
            "ass",
            Path::new("missing-fixture-tool"),
            &output,
            Instant::now()
        )
        .await,
        Err(BurnError::Deadline)
    );
    assert!(!output.exists());
}

#[tokio::test]
async fn cancelled_extraction_never_launches_or_creates_output() {
    let root = tempfile::tempdir().unwrap();
    let output = root.path().join("pending");
    for explicit in [false, true] {
        let (sender, receiver) = tokio::sync::oneshot::channel();
        if explicit {
            sender.send(()).unwrap();
        } else {
            drop(sender);
        }
        assert_eq!(
            extract_cancellable(
                &local(),
                4,
                "ass",
                Path::new("missing-fixture-tool"),
                &output,
                Instant::now() + Duration::from_mins(1),
                receiver,
            )
            .await,
            Err(BurnError::Cancelled)
        );
        assert!(!output.exists());
    }
}

#[tokio::test]
async fn output_bounds_reject_empty_and_oversized_completed_files() {
    let root = tempfile::tempdir().unwrap();
    let path = root.path().join("pending");
    assert_eq!(output_size(&path, false).await, Ok(()));
    assert_eq!(output_size(&path, true).await, Err(BurnError::Output));
    let file = std::fs::File::create(&path).unwrap();
    assert_eq!(output_size(&path, true).await, Err(BurnError::Output));
    file.set_len(MAX_OUTPUT_BYTES + 1).unwrap();
    assert_eq!(output_size(&path, false).await, Err(BurnError::OutputLimit));
    assert_eq!(output_size(&path, true).await, Err(BurnError::OutputLimit));
}

#[test]
fn extraction_argv_snapshot() {
    let args = strings(arguments(&local(), 4, "ass", Path::new("synthetic/pending.tmp")).unwrap());
    insta::assert_snapshot!(args.join("\n"), @r"
    -nostdin
    -hide_banner
    -loglevel
    error
    -y
    -copyts
    -i
    synthetic/source.mkv
    -map
    0:4
    -c:s
    ass
    -f
    ass
    -fs
    33554433
    synthetic/pending.tmp
    ");
}

/// A real child opens HTTP and then stalls before receiving any media bytes.
/// This is portable across the six native targets and requires no shell fixture.
#[tokio::test]
#[ignore = "requires the documented Next runtime FFmpeg executable"]
#[allow(clippy::disallowed_methods)] // Explicit opt-in fixture location, not production configuration.
async fn real_ffmpeg_extraction_deadline_closes_http_and_never_commits_cache() {
    use super::super::burn_cache::{BurnCache, CacheError};
    use tokio::io::AsyncReadExt as _;

    let ffmpeg = std::env::var_os("ERSATZRS_NEXT_TEST_FFMPEG")
        .map(std::path::PathBuf::from)
        .expect("set ERSATZRS_NEXT_TEST_FFMPEG for the real extraction regression");
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let address = listener.local_addr().unwrap();
    let source = Source::Http {
        probe_hint: None,
        is_live: None,
        uri: format!("http://{address}/synthetic-stalled-media"),
        headers: Vec::new(),
        user_agent: None,
        keep_alive: None,
        reconnect: Some(false),
        reconnect_delay_max: None,
        // Much longer than the caller deadline, so socket timeout cannot provide
        // the process-cleanup evidence this regression is intended to exercise.
        timeout_us: Some(60_000_000),
        in_point_ms: None,
        out_point_ms: None,
    };
    let root = tempfile::tempdir().unwrap();
    let cache = BurnCache::new(root.path().to_owned());
    let key = "1".repeat(64);
    let deadline = Instant::now() + Duration::from_secs(5);
    let prepare = cache.prepare(
        &key,
        &source,
        0,
        "subrip",
        &ffmpeg,
        chrono::Utc::now() + chrono::Duration::minutes(1),
        deadline,
        || async { true },
    );
    let stalled_server = async {
        let (mut socket, _) = listener.accept().await.unwrap();
        let mut request = Vec::new();
        let mut buffer = [0_u8; 512];
        while !request.windows(4).any(|bytes| bytes == b"\r\n\r\n") {
            let read = socket.read(&mut buffer).await.unwrap();
            assert_ne!(
                read, 0,
                "child must send its probe request before deadline cleanup"
            );
            request.extend_from_slice(&buffer[..read]);
            assert!(
                request.len() <= 8192,
                "synthetic probe request exceeds fixture bound"
            );
        }
        assert!(request.starts_with(b"GET /synthetic-stalled-media "));
        // Do not return a response: only terminating the managed FFmpeg child
        // should close this connection. A surviving child leaves this read pending.
        match socket.read(&mut buffer).await {
            Ok(0) => {}
            Err(error)
                if matches!(
                    error.kind(),
                    std::io::ErrorKind::ConnectionReset
                        | std::io::ErrorKind::ConnectionAborted
                        | std::io::ErrorKind::BrokenPipe
                ) => {}
            _ => panic!("terminated child must close its stalled HTTP connection"),
        }
    };
    let (result, ()) = tokio::time::timeout(Duration::from_secs(12), async {
        tokio::join!(prepare, stalled_server)
    })
    .await
    .expect("deadline must terminate the child and close its probe connection");
    assert_eq!(result, Err(CacheError::Extraction(BurnError::Deadline)));
    assert_eq!(
        std::fs::read_dir(root.path()).unwrap().count(),
        0,
        "failed extraction must leave no artifact, completion record or temporary file"
    );
}
