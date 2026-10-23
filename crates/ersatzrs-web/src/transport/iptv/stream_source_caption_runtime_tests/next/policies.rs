//! Finite Burn/Off policy proof for genuinely scanned mapped-provider sources.
//! Transcoded AV and rendered glyphs complement the retained exact Convert clock.

use super::*;
use std::sync::atomic::{AtomicUsize, Ordering};

#[derive(Clone, Default)]
struct CallbackObserver {
    successful: Callbacks,
    requests: Arc<AtomicUsize>,
}

async fn observe_callback(
    State(observer): State<CallbackObserver>,
    request: axum::extract::Request,
    next: Next,
) -> Response {
    if request.uri().path().starts_with("/ffmpeg/next-item/") {
        observer.requests.fetch_add(1, Ordering::SeqCst);
    }
    record_callback(State(observer.successful), request, next).await
}

#[derive(Clone, Copy, Debug)]
pub(super) enum Policy {
    Off,
    Burn,
}

impl Policy {
    fn burns(self) -> bool {
        matches!(self, Self::Burn)
    }
}

pub(super) async fn schedule(state: &AppState, item: i64, policy: Policy) {
    Box::pin(software_profile(state)).await;
    state
        .pool
        .execute(r#"UPDATE "Resolution" SET "Width"=320,"Height"=180 WHERE "Id"=1"#)
        .await
        .unwrap();
    next_schedule(state, item).await;
    let mut channel = state.repositories.channel.get_by_number("1").await.unwrap();
    channel.next_engine_text_subtitle_mode = NextEngineTextSubtitleMode::Burn;
    channel.subtitle_mode = if policy.burns() {
        ChannelSubtitleMode::Any
    } else {
        ChannelSubtitleMode::None
    };
    state.repositories.channel.upsert(&channel).await.unwrap();
}

fn following_discontinuity(playlist: &str) -> bool {
    let mut started = false;
    for line in playlist.lines().map(str::trim) {
        if line == "#EXT-X-DISCONTINUITY" && started {
            return true;
        }
        started |= !line.is_empty() && !line.starts_with('#');
    }
    false
}

pub(super) async fn capture(client: &reqwest::Client, public: &str, root: &Path) -> Vec<u8> {
    let master = fetch(
        client,
        &format!("{public}/iptv/channel/1.m3u8?mode=segmenter"),
        "application/vnd.apple.mpegurl",
    )
    .await;
    tokio::fs::write(root.join("master.m3u8"), &master)
        .await
        .unwrap();
    let master = std::str::from_utf8(&master).unwrap();
    assert!(!master.contains("SUBTITLES") && !master.contains("live_sub.m3u8"));
    let media = uris(master).collect::<Vec<_>>();
    assert_eq!(media, [format!("{public}/iptv/session/1/hls.m3u8")]);
    let mut segments = Segments::new();
    loop {
        let bytes = fetch(client, media[0], "application/vnd.apple.mpegurl").await;
        tokio::fs::write(root.join("media.m3u8"), &bytes)
            .await
            .unwrap();
        let playlist = std::str::from_utf8(&bytes).unwrap();
        fetch_segments(client, public, playlist, "video/mp2t", &mut segments, root).await;
        if following_discontinuity(playlist) {
            assert!(!segments.is_empty());
            assert_eq!(segments.len(), program_uris(playlist).count());
            let bytes = segments
                .into_iter()
                .flat_map(|(_, bytes)| bytes)
                .collect::<Vec<_>>();
            assert!(bytes.len() <= MAX_CAPTURE);
            tokio::fs::write(root.join("captured.ts"), &bytes)
                .await
                .unwrap();
            return bytes;
        }
        tokio::time::sleep(Duration::from_millis(20)).await;
    }
}

async fn published_off(root: &Path) -> PlayoutItem {
    let mut directory = tokio::fs::read_dir(root.join("playout/1")).await.unwrap();
    let entry = directory.next_entry().await.unwrap().unwrap();
    assert!(directory.next_entry().await.unwrap().is_none());
    let publication: Value =
        serde_json::from_slice(&tokio::fs::read(entry.path()).await.unwrap()).unwrap();
    let items = publication["items"]
        .as_array()
        .unwrap()
        .iter()
        .filter(|item| item["id"] == "1")
        .collect::<Vec<_>>();
    assert_eq!(items.len(), 1);
    serde_json::from_value(items[0].clone()).unwrap()
}

fn off_tracks(item: &PlayoutItem, source: &Path) -> (u64, u64) {
    assert!(
        item.source.is_none(),
        "Off must retain ordinary concrete publication"
    );
    let tracks = item.tracks.as_ref().unwrap();
    assert!(
        tracks.subtitle.is_none(),
        "Off must not publish caption preparation authority"
    );
    let video = tracks.video.as_ref().unwrap();
    let audio = tracks.audio.as_ref().unwrap();
    assert_eq!(video.stream_index, Some(0));
    assert_eq!(audio.stream_index, Some(3));
    let (seek, end) = timing(video);
    assert_eq!(timing(audio), (seek, end));
    assert!(seek.unwrap() >= 2000);
    assert_eq!(end, Some(9200));
    for track in [video, audio] {
        let Source::Local { path, .. } = track.source.as_ref().unwrap() else {
            panic!("actual mapped finite source");
        };
        assert_eq!(Path::new(path), source);
    }
    (seek.unwrap(), end.unwrap())
}

async fn policy_receipt(observer: &CallbackObserver, source: &Path, root: &Path, policy: Policy) {
    let ready = observer.successful.lock().await;
    let callback_count = ready.len();
    let request_count = observer.requests.load(Ordering::SeqCst);
    assert!(request_count >= callback_count);
    let item = if policy.burns() {
        ready
            .iter()
            .find(|item| {
                item.tracks
                    .as_ref()
                    .is_some_and(|tracks| tracks.subtitle.is_some())
            })
            .expect("actual authenticated Burn caption callback")
            .clone()
    } else {
        assert_eq!(
            request_count, 0,
            "Off must make no caption preparation requests"
        );
        assert!(
            ready.is_empty(),
            "Off must not request deferred caption preparation"
        );
        published_off(root).await
    };
    drop(ready);
    let (seek, end) = if policy.burns() {
        let (sidecar, seek, end) = selected_tracks(&item, source);
        let text = tokio::fs::read_to_string(sidecar).await.unwrap();
        let independent =
            tokio::fs::read_to_string(source.parent().unwrap().join("independent-889.srt"))
                .await
                .unwrap();
        assert_eq!(cue_blocks(&text), cue_blocks(&independent));
        assert!(text.contains("889 FINAL") && !text.contains("ENGLISH PAGE 888"));
        (seek, end)
    } else {
        assert!(
            !root.join("playout/burn-subtitles").exists(),
            "Off must create no caption cache"
        );
        off_tracks(&item, source)
    };
    let receipt = json!({"policy":format!("{policy:?}"),"item_id":item.id,"callback_count":callback_count,
        "caption_callback_requests":request_count,
        "authenticated_caption_callback":policy.burns(),"video_stream_index":0,"audio_stream_index":3,
        "subtitle_stream_index":policy.burns().then_some(0),"source_seek_ms":seek,"source_out_ms":end,
        "source_kind":"mapped_physical_local","caption_artifact_authority":policy.burns()});
    tokio::fs::write(
        root.join("policy-receipt.json"),
        serde_json::to_vec_pretty(&receipt).unwrap(),
    )
    .await
    .unwrap();
}

async fn picture(ffmpeg: &str, captured: &Path, policy: Policy) {
    // Existing Next text_subtitle_runtime_tests::assert_text_video_and_audio
    // oracle; the qualified source crop has no white pixels without captions.
    let mut command = Command::new(ffmpeg);
    command
        .args(["-v", "error", "-nostdin", "-i"])
        .arg(captured)
        .args([
            "-ss",
            "0.5",
            "-map",
            "0:v:0",
            "-frames:v",
            "1",
            "-vf",
            "crop=320:48:0:132",
            "-pix_fmt",
            "rgb24",
            "-f",
            "rawvideo",
            "pipe:1",
        ]);
    let pixels = tool(command).await;
    assert_eq!(pixels.len(), 320 * 48 * 3);
    let white = pixels
        .chunks_exact(3)
        .filter(|pixel| pixel.iter().all(|value| *value > 180))
        .count();
    let receipt = json!({"output_time_ms":500,"crop":"320:48:0:132",
        "rgb_white_threshold":180,"white_pixels":white,"burn":policy.burns()});
    tokio::fs::write(
        captured.with_extension("glyphs.json"),
        serde_json::to_vec_pretty(&receipt).unwrap(),
    )
    .await
    .unwrap();
    if policy.burns() {
        assert!(white > 20, "Burn must render selected subtitle glyphs");
    } else {
        assert_eq!(white, 0, "Off must leave video free of subtitle glyphs");
    }
}

pub(super) async fn output(tools: &[String; 3], root: &Path, bytes: &[u8], policy: Policy) {
    let captured = root.join("captured.ts");
    let mut decode = Command::new(&tools[0]);
    decode
        .args(["-v", "error", "-nostdin", "-xerror", "-i"])
        .arg(&captured)
        .args(["-f", "null", "-"]);
    tool(decode).await;
    let streams = stream_inventory(&tools[1], bytes).await;
    assert_eq!(
        streams.len(),
        2,
        "policy output must contain exactly the selected AV tracks"
    );
    let video = streams
        .iter()
        .find(|stream| stream["codec_type"] == "video")
        .unwrap();
    let audio = streams
        .iter()
        .find(|stream| stream["codec_type"] == "audio")
        .unwrap();
    assert_eq!(video["codec_name"], "h264");
    assert_eq!(audio["codec_name"], "aac");
    assert_eq!(audio["tags"]["language"], "nld");
    Box::pin(tone(&tools[0], bytes, 880)).await;
    picture(&tools[0], &captured, policy).await;
}

struct Delivery {
    observer: CallbackObserver,
    client: reqwest::Client,
    public: String,
    source: PathBuf,
    root: PathBuf,
    tools: [String; 3],
    policy: Policy,
}

impl Delivery {
    async fn check(self) {
        let bytes = timeout(
            Duration::from_secs(35),
            capture(&self.client, &self.public, &self.root),
        )
        .await
        .expect("complete first source programme must be delivered before the next discontinuity");
        policy_receipt(&self.observer, &self.source, &self.root, self.policy).await;
        Box::pin(output(&self.tools, &self.root, &bytes, self.policy)).await;
    }
}

async fn run(owner: MediaSourceKind, policy: Policy, tools: &[String; 3]) {
    let root = tempfile::tempdir().unwrap().keep();
    println!(
        "mapped Next policy HTTP evidence: {}, owner={owner:?}, policy={policy:?}",
        root.display()
    );
    let library = root.join("library");
    let source = retained_source(&tools[0], &library).await;
    let unused = Server::start(Router::new()).await;
    let mut state = application(&root, &unused).await;
    state.next_engine = Arc::new(NextHost::new(
        state.pool.clone(),
        state.application.operations().entity_locker().clone(),
        root.join("playout"),
    ));
    let (item, _, provider) = Box::pin(scan_caption_owner(
        &state, &library, &source, &tools[1], owner,
    ))
    .await;
    Box::pin(schedule(&state, item, policy)).await;
    assert_eq!(DEFAULT_ACTIVE_TRANSCODE_LIMIT, 2);
    assert_eq!(
        load_active_transcode_limit(&state.pool)
            .await
            .unwrap()
            .unwrap_or(DEFAULT_ACTIVE_TRANSCODE_LIMIT),
        2
    );
    assert!(!root.join("playout/burn-subtitles").exists());
    let observer = CallbackObserver::default();
    let internal = Server::start(build_internal_app(state.clone()).layer(
        axum::middleware::from_fn_with_state(observer.clone(), observe_callback),
    ))
    .await;
    state.internal_http_addr.set(internal.address).unwrap();
    state.next_engine.media.configure(internal.address).unwrap();
    let public = Server::start(build_app(state.clone())).await;
    state.streaming_http_addr.set(public.address).unwrap();
    let delivery = Delivery {
        observer,
        client: reqwest::Client::builder()
            .no_proxy()
            .timeout(Duration::from_secs(25))
            .build()
            .unwrap(),
        public: public.url(""),
        source,
        root,
        tools: tools.clone(),
        policy,
    };
    let assertion = tokio::spawn(Box::pin(delivery.check())).await;
    stop_next(&state).await;
    finish_caption_owner(&state, provider, owner, public, internal, unused).await;
    assertion.expect("mapped-provider Next Burn/Off must preserve selected AV and caption policy");
}

#[test]
#[ignore = "requires candidate4 via ETV_NEXT_FOLDER, retained H264 source/889 SRT, FFmpeg9, loopback and fresh isolated app-data/TMPDIR"]
fn normal_next_mapped_provider_route_burns_selected_page889_and_respects_off() {
    let _guard = active_transcode_test_guard_for_tests();
    reset_active_transcode_state_for_tests();
    tokio::runtime::Builder::new_multi_thread()
        .worker_threads(2)
        .enable_all()
        .build()
        .unwrap()
        .block_on(Box::pin(async {
            let tools = initialize_source_caption_runtime().await;
            for owner in [
                MediaSourceKind::Jellyfin,
                MediaSourceKind::Emby,
                MediaSourceKind::Plex,
            ] {
                for policy in [Policy::Off, Policy::Burn] {
                    Box::pin(run(owner, policy, &tools)).await;
                    assert_eq!(
                        active_transcode_snapshot(DEFAULT_ACTIVE_TRANSCODE_LIMIT).active,
                        0
                    );
                }
            }
        }));
}
