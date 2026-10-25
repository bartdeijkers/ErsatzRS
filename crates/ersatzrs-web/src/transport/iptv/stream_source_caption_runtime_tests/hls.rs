//! Actual public native HLS source-caption delivery, without a MPEG-TS wrapper.

use super::*;

async fn capture(public: std::net::SocketAddr, root: &Path) -> Vec<u8> {
    capture_segments(public, root, 1).await.remove(0)
}

pub(super) async fn capture_segments(
    public: std::net::SocketAddr,
    root: &Path,
    count: usize,
) -> Vec<Vec<u8>> {
    let client = reqwest::Client::builder()
        .timeout(Duration::from_secs(20))
        .build()
        .unwrap();
    let master_url = format!("http://{public}/iptv/channel/1.m3u8");
    let master = next::fetch(&client, &master_url, "application/vnd.apple.mpegurl").await;
    tokio::fs::write(root.join("master.m3u8"), &master)
        .await
        .unwrap();
    let master = String::from_utf8(master).unwrap();
    let variants = next::uris(&master).collect::<Vec<_>>();
    assert_eq!(variants.len(), 1);
    let media_url = reqwest::Url::parse(&master_url)
        .unwrap()
        .join(variants[0])
        .unwrap();
    assert_eq!(media_url.path(), "/iptv/session/1/hls.m3u8");
    let playlist = tokio::time::timeout(Duration::from_secs(20), async {
        loop {
            let playlist =
                next::fetch(&client, media_url.as_str(), "application/vnd.apple.mpegurl").await;
            let playlist = String::from_utf8(playlist).unwrap();
            if next::uris(&playlist).count() >= count {
                break playlist;
            }
            tokio::time::sleep(Duration::from_millis(50)).await;
        }
    })
    .await
    .expect("required complete native HLS segments must arrive");
    tokio::fs::write(root.join("hls.m3u8"), &playlist)
        .await
        .unwrap();
    assert!(playlist
        .lines()
        .any(|line| line == "#EXT-X-MEDIA-SEQUENCE:0"));
    let mut captured = Vec::new();
    for (index, uri) in next::uris(&playlist).take(count).enumerate() {
        let url = media_url.join(uri).unwrap();
        assert_eq!(url.origin(), media_url.origin());
        let bytes = next::fetch(&client, url.as_str(), "video/mp2t").await;
        assert!(bytes.len().is_multiple_of(188));
        assert!(bytes.chunks_exact(188).all(|packet| packet[0] == 0x47));
        tokio::fs::write(root.join(format!("captured-{index}.ts")), &bytes)
            .await
            .unwrap();
        if index == 0 {
            tokio::fs::write(root.join("captured.ts"), &bytes)
                .await
                .unwrap();
        }
        captured.push(bytes);
    }
    assert_eq!(captured.len(), count);
    captured
}

pub(super) async fn verify(tools: &[String; 3], bytes: &[u8], expected: &[Vec<u8>; 3], burn: bool) {
    let streams = stream_inventory(&tools[1], bytes).await;
    assert_eq!(streams.len(), 2, "native HLS must contain the programme AV");
    let video = streams.iter().find(|s| s["codec_type"] == "video").unwrap();
    assert_eq!(video["codec_name"], "h264");
    let audio = streams.iter().find(|s| s["codec_type"] == "audio").unwrap();
    assert_eq!(audio["codec_name"], "aac");
    assert_eq!(audio["tags"]["language"], "nld");
    tone(&tools[0], bytes, 660).await;
    let actual = pixels(&tools[0], bytes, None).await;
    if burn {
        compare_burn(&actual, &expected[0], &expected[1], &expected[2]);
    } else {
        assert!(
            actual.iter().all(|value| *value <= 20),
            "Off retains plain programme"
        );
    }
}

async fn run() {
    let tools = initialize_source_caption_runtime().await;
    let root = tempfile::tempdir().unwrap().keep();
    println!(
        "public native HLS source-caption evidence: {}",
        root.display()
    );
    let library = root.join("library");
    let source = local_source(&tools[0], &library).await;
    let expected = calibration(&tools, &source, &root).await;
    let unused_provider = Server::start(Router::new()).await;
    let state = application(&root, &unused_provider).await;
    let (item, _) = scan_source(&state, &library, &tools[1]).await;
    software_profile(&state).await;
    assert_eq!(
        state
            .repositories
            .channel
            .get_by_number("1")
            .await
            .unwrap()
            .streaming_engine,
        ersatzrs_domain::StreamingEngine::Legacy
    );
    let internal = Server::start(build_internal_app(state.clone())).await;
    state.internal_http_addr.set(internal.address).unwrap();
    state.next_engine.media.configure(internal.address).unwrap();
    let public = Server::start(build_app(state.clone())).await;
    state.streaming_http_addr.set(public.address).unwrap();
    for (label, streaming_mode, subtitle_mode) in [
        (
            "segmenter-off",
            StreamingMode::HttpLiveStreamingSegmenter,
            ChannelSubtitleMode::None,
        ),
        (
            "segmenter-burn",
            StreamingMode::HttpLiveStreamingSegmenter,
            ChannelSubtitleMode::Any,
        ),
        (
            "direct-burn",
            StreamingMode::HttpLiveStreamingDirect,
            ChannelSubtitleMode::Any,
        ),
    ] {
        let case = root.join(label);
        tokio::fs::create_dir(&case).await.unwrap();
        start_item(&state, item, subtitle_mode, "nld", streaming_mode).await;
        let address = public.address;
        let response = tokio::spawn(async move {
            tokio::time::timeout(Duration::from_secs(30), capture(address, &case)).await
        })
        .await;
        stop_native_hls(&state).await;
        let bytes = response
            .expect("native HLS capture assertion")
            .expect("bounded native HLS response");
        verify(
            &tools,
            &bytes,
            &expected,
            subtitle_mode != ChannelSubtitleMode::None,
        )
        .await;
        println!(
            "public native HLS verified: {label}, Dutch660Hz, exact888, packets={}",
            bytes.len() / 188
        );
    }
    finish_caption_owner(
        &state,
        None,
        MediaSourceKind::Local,
        public,
        internal,
        unused_provider,
    )
    .await;
}

#[test]
#[ignore = "requires pinned FFmpeg/ffprobe9+libzvbi/libx264, TSDuck, loopback and fresh isolated app-data; genuine Local scan and public native HLS"]
fn normal_public_native_hls_preserves_source_captions_off_and_alternate_audio() {
    let _guard = active_transcode_test_guard_for_tests();
    reset_active_transcode_state_for_tests();
    tokio::runtime::Builder::new_multi_thread()
        .worker_threads(2)
        .enable_all()
        .build()
        .unwrap()
        .block_on(Box::pin(run()));
    assert_eq!(
        active_transcode_snapshot(DEFAULT_ACTIVE_TRANSCODE_LIMIT).active,
        0
    );
}
