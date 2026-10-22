//! Genuine mapped-provider scan, Next Convert callback and public HLS delivery.
//! Candidate runtime qualification only; no production adoption or client claim.

use super::*;
use crate::transport::iptv::stream::encode_sql_datetime;
use axum::{body::Body, extract::State, middleware::Next, response::Response};
use ersatzrs_domain::{NextEngineTextSubtitleMode, StreamingEngine};
use ersatzrs_infra::next_engine::{
    host::NextHost,
    protocol::{PlayoutItem, Source, Track},
};
use sha2::{Digest as _, Sha256};
use std::{process::Stdio, sync::Arc};
use tokio::{io::AsyncReadExt as _, process::Command, sync::Mutex, time::timeout};

#[path = "next/policies.rs"]
mod policies;

#[path = "next/retained.rs"]
mod retained;

#[path = "next/retained_clock.rs"]
mod retained_clock;

type Callbacks = Arc<Mutex<Vec<PlayoutItem>>>;
type Segments = Vec<(String, Vec<u8>)>;
const SOURCE_SHA256: &str = "419c4d902c7d49d8e3c45097d935eac81d514fbd621883c7c3ac440b76d44a3c";

#[expect(
    clippy::disallowed_methods,
    reason = "Explicit opt-in retained synthetic fixture inputs"
)]
fn input(name: &str) -> PathBuf {
    PathBuf::from(std::env::var_os(name).expect("set retained mapped Next proof inputs"))
}

async fn tool(command: Command) -> Vec<u8> {
    tool_with_stderr(command, 128 * 1024).await.0
}

async fn tool_with_stderr(mut command: Command, stderr_limit: u64) -> (Vec<u8>, String) {
    assert!(stderr_limit <= 1024 * 1024);
    let errors = tempfile::NamedTempFile::new().unwrap();
    command
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(errors.reopen().unwrap());
    let mut child = command.kill_on_drop(true).spawn().unwrap();
    let mut stdout = child.stdout.take().unwrap().take(128 * 1024 + 1);
    let outcome = timeout(Duration::from_secs(45), async {
        let mut bytes = Vec::new();
        stdout.read_to_end(&mut bytes).await.unwrap();
        (bytes, child.wait().await.unwrap())
    })
    .await;
    let Ok((bytes, status)) = outcome else {
        let _ = child.start_kill();
        let _ = timeout(Duration::from_secs(10), child.wait()).await;
        panic!("mapped Next fixture process timed out");
    };
    assert!(bytes.len() <= 128 * 1024);
    assert!(std::fs::metadata(errors.path()).unwrap().len() <= stderr_limit);
    let stderr = std::fs::read_to_string(errors.path()).unwrap();
    assert!(status.success(), "{}", stderr);
    (bytes, stderr)
}

async fn record_callback(
    State(callbacks): State<Callbacks>,
    request: axum::extract::Request,
    next: Next,
) -> Response {
    let selected = request.uri().path().starts_with("/ffmpeg/next-item/");
    if selected {
        for header in [
            ersatzrs_infra::next_engine::media::ACCESS_HEADER,
            ersatzrs_infra::next_engine::media::SESSION_HEADER,
        ] {
            assert!(request.headers().contains_key(header));
        }
    }
    let response = next.run(request).await;
    if !selected || response.status() != axum::http::StatusCode::OK {
        return response;
    }
    let (parts, body) = response.into_parts();
    let bytes = axum::body::to_bytes(body, 512 * 1024).await.unwrap();
    callbacks
        .lock()
        .await
        .push(serde_json::from_slice(&bytes).unwrap());
    Response::from_parts(parts, Body::from(bytes))
}

async fn retained_source(ffmpeg: &str, library: &Path) -> PathBuf {
    retained_source_with_format(
        ffmpeg,
        library,
        ersatzrs_infra::ffprobe::SourceTeletextPacketFormat::Ts188,
    )
    .await
}

async fn retained_source_with_format(
    ffmpeg: &str,
    library: &Path,
    format: ersatzrs_infra::ffprobe::SourceTeletextPacketFormat,
) -> PathBuf {
    use std::fmt::Write as _;

    let original = input("ERSATZRS_NEXT_MAPPED_SOURCE");
    let mut bytes = tokio::fs::read(&original).await.unwrap();
    let mut digest = String::with_capacity(64);
    for byte in Sha256::digest(&bytes) {
        write!(digest, "{byte:02x}").unwrap();
    }
    assert_eq!(digest, SOURCE_SHA256);
    if format == ersatzrs_infra::ffprobe::SourceTeletextPacketFormat::M2ts192 {
        let wrapped = tokio::fs::read(input("ERSATZRS_NEXT_MAPPED_M2TS_SOURCE"))
            .await
            .unwrap();
        assert_eq!(bytes.len() % 188, 0);
        assert_eq!(wrapped.len(), bytes.len() / 188 * 192);
        for (original, frame) in bytes.chunks_exact(188).zip(wrapped.chunks_exact(192)) {
            assert_eq!(
                &frame[4..],
                original,
                "every original TS packet stays exact"
            );
        }
        bytes = wrapped;
    }
    tokio::fs::create_dir(library).await.unwrap();
    let source = library.join("movie.ts");
    tokio::fs::write(&source, bytes).await.unwrap();
    let expected = tokio::fs::read_to_string(input("ERSATZRS_NEXT_MAPPED_INDEPENDENT_889"))
        .await
        .unwrap();
    let mut decode = Command::new(ffmpeg);
    decode
        .args([
            "-v",
            "error",
            "-nostdin",
            "-txt_page:2",
            "889",
            "-txt_format:2",
            "text",
            "-fix_sub_duration",
            "-i",
        ])
        .arg(&source)
        .args(["-map", "0:2", "-c:s", "srt", "-f", "srt", "pipe:1"]);
    let decoded = tool(decode).await;
    assert_eq!(
        cue_blocks(std::str::from_utf8(&decoded).unwrap()),
        cue_blocks(&expected)
    );
    assert_eq!(cue_blocks(&expected).len(), 8);
    assert!(expected.contains("889 FINAL") && !expected.contains("ENGLISH PAGE 888"));
    tokio::fs::write(library.join("independent-889.srt"), expected)
        .await
        .unwrap();
    source
}

async fn next_schedule(state: &AppState, item: i64) -> chrono::DateTime<Utc> {
    let mut channel = state.repositories.channel.get_by_number("1").await.unwrap();
    channel.streaming_engine = StreamingEngine::Next;
    channel.streaming_mode = StreamingMode::HttpLiveStreamingSegmenter;
    channel.preferred_audio_language_code = Some("nld".into());
    channel.preferred_subtitle_language_code = Some("nld".into());
    channel.subtitle_mode = ChannelSubtitleMode::Any;
    channel.next_engine_text_subtitle_mode = NextEngineTextSubtitleMode::Convert;
    state.repositories.channel.upsert(&channel).await.unwrap();
    state
        .pool
        .execute(r#"UPDATE "Playout" SET "ScheduleKind"=1,"ScheduleFile"=NULL WHERE "Id"=1;"#)
        .await
        .unwrap();
    let start = Utc::now() - chrono::Duration::milliseconds(100);
    sqlx::query(r#"INSERT INTO "PlayoutItem" ("PlayoutId","MediaItemId","Start","Finish","InPoint","OutPoint",
        "DisableWatermarks","FillerKind","GuideGroup") VALUES (1,?,?,?,'00:00:02','00:00:09.200',0,0,0)"#)
        .bind(item).bind(encode_sql_datetime(start))
        .bind(encode_sql_datetime(start+chrono::Duration::milliseconds(7200)))
        .execute(&state.pool).await.unwrap();
    start
}

pub(super) async fn fetch(client: &reqwest::Client, url: &str, kind: &str) -> Vec<u8> {
    let response = client.get(url).send().await.unwrap();
    assert_eq!(
        response.status(),
        axum::http::StatusCode::OK,
        "public {url}"
    );
    assert!(response.headers()[axum::http::header::CONTENT_TYPE]
        .to_str()
        .unwrap()
        .starts_with(kind));
    let bytes = response.bytes().await.unwrap();
    assert!(!bytes.is_empty() && bytes.len() <= MAX_CAPTURE);
    bytes.to_vec()
}

pub(super) fn uris(playlist: &str) -> impl Iterator<Item = &str> {
    assert!(playlist.starts_with("#EXTM3U"));
    playlist
        .lines()
        .map(str::trim)
        .filter(|line| !line.is_empty() && !line.starts_with('#'))
}

fn program_uris(playlist: &str) -> impl Iterator<Item = &str> {
    assert!(playlist
        .lines()
        .any(|line| line == "#EXT-X-MEDIA-SEQUENCE:0"));
    let mut started = false;
    playlist
        .lines()
        .map(str::trim)
        .take_while(move |line| {
            if *line == "#EXT-X-DISCONTINUITY" && started {
                return false;
            }
            started |= !line.is_empty() && !line.starts_with('#');
            true
        })
        .filter(|line| !line.is_empty() && !line.starts_with('#'))
}

#[test]
fn program_capture_respects_the_advertised_fallback_discontinuity() {
    let playlist = "#EXTM3U\n#EXT-X-MEDIA-SEQUENCE:0\n#EXT-X-DISCONTINUITY\n\
        #EXTINF:4,\nlive000000.ts\n#EXTINF:3.08,\nlive000001.ts\n\
        #EXT-X-DISCONTINUITY\n#EXTINF:4,\nlive000002.ts\n";
    assert_eq!(
        program_uris(playlist).collect::<Vec<_>>(),
        ["live000000.ts", "live000001.ts"]
    );
}

async fn fetch_segments(
    client: &reqwest::Client,
    public: &str,
    playlist: &str,
    kind: &str,
    received: &mut Segments,
    root: &Path,
) {
    // The scheduled finite program is the first continuity group. Following
    // fallback is a separate advertised decoder interval, not source copy.
    for uri in program_uris(playlist) {
        if received.iter().any(|(seen, _)| seen == uri) {
            continue;
        }
        // Generated Next segment names are flat, relative and proof-owned.
        assert!(!uri.contains('/') && !uri.contains("..") && !uri.contains('?'));
        let bytes = fetch(client, &format!("{public}/iptv/session/1/{uri}"), kind).await;
        if kind == "video/mp2t" {
            assert!(bytes.len().is_multiple_of(188));
            assert!(bytes.chunks_exact(188).all(|packet| packet[0] == 0x47));
        }
        tokio::fs::write(root.join(uri), &bytes).await.unwrap();
        received.push((uri.into(), bytes));
        assert!(received.len() <= 16);
    }
}

fn final_media_received(media: &Segments, subtitles: &Segments) -> bool {
    let final_segments = subtitles
        .iter()
        .filter(|(_, bytes)| std::str::from_utf8(bytes).unwrap().contains("889 FINAL"))
        .collect::<Vec<_>>();
    !final_segments.is_empty()
        && final_segments.iter().all(|(uri, _)| {
            // Next's playlist_manager::write_playlists maps each media basename
            // directly to the VTT basename, from the same admitted segment deque.
            let base = uri.strip_suffix(".vtt").expect("Next VTT segment identity");
            media.iter().any(|(name, _)| name == &format!("{base}.ts"))
        })
}

async fn delivered(client: &reqwest::Client, public: &str, root: &Path) -> Vec<String> {
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
    assert!(master.contains("SUBTITLES=\"subs\""));
    assert!(master.contains("URI=\"/iptv/session/1/live_sub.m3u8\""));
    let media = uris(master).collect::<Vec<_>>();
    assert_eq!(media.len(), 1);
    assert_eq!(media[0], format!("{public}/iptv/session/1/hls.m3u8"));
    let mut segments = Segments::new();
    let mut subtitles = Segments::new();
    loop {
        let media = fetch(client, media[0], "application/vnd.apple.mpegurl").await;
        let captions = fetch(
            client,
            &format!("{public}/iptv/session/1/live_sub.m3u8"),
            "application/vnd.apple.mpegurl",
        )
        .await;
        for (name, bytes) in [("media.m3u8", &media), ("subtitles.m3u8", &captions)] {
            tokio::fs::write(root.join(name), bytes).await.unwrap();
        }
        fetch_segments(
            client,
            public,
            std::str::from_utf8(&media).unwrap(),
            "video/mp2t",
            &mut segments,
            root,
        )
        .await;
        fetch_segments(
            client,
            public,
            std::str::from_utf8(&captions).unwrap(),
            "text/vtt",
            &mut subtitles,
            root,
        )
        .await;
        let cues = subtitles
            .iter()
            .map(|(_, bytes)| String::from_utf8(bytes.clone()).unwrap())
            .collect::<Vec<_>>();
        if final_media_received(&segments, &subtitles) {
            let bytes = segments
                .into_iter()
                .flat_map(|(_, bytes)| bytes)
                .collect::<Vec<_>>();
            assert!(bytes.len() <= MAX_CAPTURE);
            tokio::fs::write(root.join("captured.ts"), bytes)
                .await
                .unwrap();
            return cues;
        }
        tokio::time::sleep(Duration::from_millis(20)).await;
    }
}

fn selected_tracks(item: &PlayoutItem, source: &Path) -> (PathBuf, u64, u64) {
    let tracks = item.tracks.as_ref().unwrap();
    let video = tracks.video.as_ref().unwrap();
    let audio = tracks.audio.as_ref().unwrap();
    let subtitle = tracks.subtitle.as_ref().unwrap();
    assert_eq!(video.stream_index, Some(0));
    assert_eq!(audio.stream_index, Some(3));
    assert_eq!(subtitle.stream_index, Some(0));
    let (seek, end) = timing(video);
    assert_eq!(timing(audio), (seek, end));
    assert_eq!(timing(subtitle), (seek, end));
    assert!(seek.unwrap() >= 2000);
    assert_eq!(end, Some(9200));
    for track in [video, audio] {
        let Source::Local { path, .. } = track.source.as_ref().unwrap() else {
            panic!("mapped finite source");
        };
        assert_eq!(Path::new(path), source);
        assert!(!path.contains(mapped_provider_fixture::PROVIDER_PATH));
    }
    let Source::Local { path, .. } = subtitle.source.as_ref().unwrap() else {
        panic!("owned caption sidecar");
    };
    assert_ne!(Path::new(path), source);
    (path.into(), seek.unwrap(), end.unwrap())
}

async fn check_delivery(
    state: AppState,
    callbacks: Callbacks,
    client: reqwest::Client,
    public: String,
    source: PathBuf,
    root: PathBuf,
    tools: [String; 3],
) {
    // The listener remains owned outside this task so a failed assertion still
    // reaches the explicit producer and HTTP cleanup boundary.
    let cues = timeout(Duration::from_secs(35), delivered(&client, &public, &root))
        .await
        .expect("bounded public Next caption delivery");
    let callbacks = callbacks.lock().await;
    let callback_count = callbacks.len();
    let callback = callbacks
        .iter()
        .find(|item| {
            item.tracks
                .as_ref()
                .is_some_and(|tracks| tracks.subtitle.is_some())
        })
        .expect("actual authenticated caption preparation callback")
        .clone();
    drop(callbacks);
    let (sidecar, seek, end) = selected_tracks(&callback, &source);
    let receipt = json!({"authenticated_callback":true,"callback_count":callback_count,
        "item_id":callback.id,"video_stream_index":0,"audio_stream_index":3,
        "subtitle_stream_index":0,"video_source":"mapped_physical_local",
        "subtitle_source":"owned_complete_local_sidecar","source_seek_ms":seek,"source_out_ms":end});
    tokio::fs::write(
        root.join("callback-receipt.json"),
        serde_json::to_vec_pretty(&receipt).unwrap(),
    )
    .await
    .unwrap();
    let text = tokio::fs::read_to_string(sidecar).await.unwrap();
    assert!(text.contains("889 FINAL") && !text.contains("ENGLISH PAGE 888"));
    assert!(cues.iter().all(|cue| !cue.contains("ENGLISH PAGE 888")));
    let mut decode = Command::new(&tools[0]);
    decode
        .args(["-v", "error", "-nostdin", "-xerror", "-i"])
        .arg(root.join("captured.ts"))
        .args(["-f", "null", "-"]);
    tool(decode).await;
    check(
        (Path::new(&tools[0]), Path::new(&tools[1])),
        &source,
        &root.join("captured.ts"),
        &cues,
        &text,
        (seek, end, true),
        &root,
    )
    .await;
    assert_eq!(
        state
            .application
            .operations()
            .entity_locker()
            .active_teletext_work(),
        0
    );
}

async fn stop_next(state: &AppState) {
    state.hls_sessions.stop_channel("1").await;
    timeout(Duration::from_secs(20), async {
        loop {
            if state.hls_sessions.active_channels().await.is_empty()
                && !crate::transport::iptv::hls::hls_working_dir("1").exists()
                && active_transcode_snapshot(DEFAULT_ACTIVE_TRANSCODE_LIMIT).active == 0
                && state
                    .application
                    .operations()
                    .entity_locker()
                    .active_teletext_work()
                    == 0
            {
                break;
            }
            tokio::time::sleep(Duration::from_millis(10)).await;
        }
    })
    .await
    .expect("Next HTTP session must reap producer, worker, source work and output");
}

async fn run_owner(owner: MediaSourceKind, tools: &[String; 3]) {
    let root = tempfile::tempdir().unwrap().keep();
    println!(
        "mapped Next ordinary HTTP evidence: {}, owner={owner:?}",
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
    next_schedule(&state, item).await;
    let callbacks = Callbacks::default();
    let internal = Server::start(build_internal_app(state.clone()).layer(
        axum::middleware::from_fn_with_state(callbacks.clone(), record_callback),
    ))
    .await;
    state.internal_http_addr.set(internal.address).unwrap();
    state.next_engine.media.configure(internal.address).unwrap();
    let public = Server::start(build_app(state.clone())).await;
    state.streaming_http_addr.set(public.address).unwrap();
    let assertion = tokio::spawn(check_delivery(
        state.clone(),
        callbacks,
        reqwest::Client::builder()
            .no_proxy()
            .timeout(Duration::from_secs(25))
            .build()
            .unwrap(),
        public.url(""),
        source,
        root,
        tools.clone(),
    ))
    .await;
    stop_next(&state).await;
    finish_caption_owner(&state, provider, owner, public, internal, unused).await;
    assertion.expect("mapped Next public delivery and exact source association must succeed");
}

#[test]
#[ignore = "requires candidate4 via ETV_NEXT_FOLDER, paired retained synthetic H264 source/889 SRT, FFmpeg9, loopback and fresh isolated app-data/TMPDIR"]
fn normal_next_mapped_provider_route_converts_exact_page889_and_preserves_copy_and_alternate_audio()
{
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
                Box::pin(run_owner(owner, &tools)).await;
                assert_eq!(
                    active_transcode_snapshot(DEFAULT_ACTIVE_TRANSCODE_LIMIT).active,
                    0
                );
            }
        }));
}

// The following exact packet/cue oracle is adapted from the existing infra
// source_teletext_runtime_tests::clock, applied to bytes actually fetched over
// public HTTP. Keep source-origin, one-tick map and exact clipping unchanged.
fn video_slices(bytes: &[u8]) -> Vec<&[u8]> {
    let starts: Vec<usize> = bytes
        .windows(3)
        .enumerate()
        .filter_map(|(index, bytes)| (bytes == [0, 0, 1]).then_some(index))
        .collect();
    let mut slices = Vec::new();
    for (index, start) in starts.iter().enumerate() {
        let mut end = starts.get(index + 1).copied().unwrap_or(bytes.len());
        while end > start + 3 && bytes[end - 1] == 0 {
            end -= 1;
        }
        let nal = &bytes[start + 3..end];
        if nal.first().is_some_and(|byte| matches!(byte & 31, 1 | 5)) {
            slices.push(nal);
        }
    }
    slices
}

fn timing(track: &Track) -> (Option<u64>, Option<u64>) {
    let Source::Local {
        in_point_ms,
        out_point_ms,
        ..
    } = track.source.as_ref().unwrap()
    else {
        panic!("finite local track")
    };
    (*in_point_ms, *out_point_ms)
}

async fn packets(ffprobe: &Path, path: &Path, stream: &str, video: bool) -> serde_json::Value {
    let mut probe = Command::new(ffprobe);
    probe
        .args([
            "-v",
            "error",
            "-select_streams",
            stream,
            "-show_packets",
            "-show_streams",
            "-show_format",
            "-show_data_hash",
            "sha256",
            "-show_entries",
            if video {
                "packet=pts,dts,duration,data_hash:packet_side_data=:stream=index,codec_name,time_base:format=start_time"
            } else {
                "packet=data_hash:packet_side_data=:stream=index,codec_name,time_base:format=start_time"
            },
            "-of",
            "compact=p=1:nk=0",
        ])
        .arg(path);
    let mut result = serde_json::json!({"packets":[],"streams":[],"format":{}});
    for line in std::str::from_utf8(&tool(probe).await)
        .unwrap()
        .lines()
        .filter(|line| !line.is_empty())
    {
        let mut fields = line.split('|');
        let kind = fields.next().unwrap();
        if kind == "program" {
            continue;
        }
        let mut row = serde_json::Map::new();
        for (name, value) in fields.filter_map(|field| field.split_once('=')) {
            let value = match name {
                "pts" | "dts" | "duration" | "index" => {
                    serde_json::json!(value.parse::<i64>().unwrap())
                }
                "data_hash" | "codec_name" | "time_base" | "start_time" => {
                    serde_json::json!(value)
                }
                _ => continue,
            };
            assert!(row.insert(name.into(), value).is_none());
        }
        match kind {
            "packet" => result["packets"].as_array_mut().unwrap().push(row.into()),
            "stream" => result["streams"].as_array_mut().unwrap().push(row.into()),
            "format" => result["format"] = row.into(),
            _ => panic!("unexpected probe section {kind}"),
        }
    }
    result
}

fn hashes(value: &serde_json::Value) -> BTreeSet<&str> {
    value["packets"]
        .as_array()
        .unwrap()
        .iter()
        .map(|packet| packet["data_hash"].as_str().unwrap())
        .collect()
}

async fn copied_audio(ffprobe: &Path, source: &Path, captured: &Path) {
    let dutch = packets(ffprobe, source, "a:1", false).await;
    let english = packets(ffprobe, source, "a:0", false).await;
    let actual = packets(ffprobe, captured, "a:0", false).await;
    assert_eq!(actual["streams"][0]["codec_name"], "aac");
    let expected = hashes(&dutch);
    let wrong = hashes(&english);
    let packets = actual["packets"].as_array().unwrap();
    assert!(packets.len() >= 40);
    assert!(packets
        .iter()
        .all(|packet| expected.contains(packet["data_hash"].as_str().unwrap())));
    assert!(
        packets
            .iter()
            .filter(|packet| !wrong.contains(packet["data_hash"].as_str().unwrap()))
            .count()
            >= 40,
        "actual AAC payloads must distinguish the selected alternate track"
    );
}

async fn copied_video(ffmpeg: &Path, source: &Path, actual: &Path) {
    let mut bodies = Vec::new();
    for path in [source, actual] {
        let output = tempfile::NamedTempFile::new().unwrap();
        let mut extract = Command::new(ffmpeg);
        extract
            .args(["-v", "error", "-nostdin", "-y", "-i"])
            .arg(path);
        extract
            .args([
                "-map",
                "0:v:0",
                "-t",
                "20",
                "-c",
                "copy",
                "-bsf:v",
                "h264_mp4toannexb",
                "-fs",
                "8388609",
                "-f",
                "h264",
            ])
            .arg(output.path());
        tool(extract).await;
        assert!(output.as_file().metadata().unwrap().len() <= 8 * 1024 * 1024);
        bodies.push(tokio::fs::read(output.path()).await.unwrap());
    }
    let original = video_slices(&bodies[0]);
    let actual = video_slices(&bodies[1]);
    assert!(actual.len() >= 20);
    assert!(actual.iter().all(|slice| original.contains(slice)));
}

fn video_shift(source: &serde_json::Value, actual: &serde_json::Value) -> i64 {
    for value in [source, actual] {
        assert_eq!(value["streams"][0]["time_base"], "1/90000");
    }
    let original = source["packets"].as_array().unwrap();
    let mut offsets = BTreeSet::new();
    let mut matched = 0;
    for packet in actual["packets"].as_array().unwrap() {
        let candidates = original
            .iter()
            .filter(|candidate| candidate["data_hash"] == packet["data_hash"])
            .collect::<Vec<_>>();
        if candidates.is_empty() {
            continue;
        }
        assert_eq!(candidates.len(), 1, "copied packet identity must be unique");
        let before = candidates[0];
        let pts = packet["pts"].as_i64().unwrap() - before["pts"].as_i64().unwrap();
        let dts = packet["dts"].as_i64().unwrap() - before["dts"].as_i64().unwrap();
        assert_eq!(
            pts, dts,
            "copied packet ordering must keep the exact source PTS/DTS relation"
        );
        offsets.insert(pts);
        matched += 1;
    }
    assert!(
        matched >= 20,
        "fixture must provide independently unique coded video packets"
    );
    assert_eq!(
        offsets.len(),
        1,
        "all unique copied packets must share one timestamp shift"
    );
    *offsets.first().unwrap()
}

fn milliseconds(text: &str) -> i64 {
    let fields = text
        .split([':', ',', '.'])
        .map(|value| value.parse::<i64>().unwrap())
        .collect::<Vec<_>>();
    assert_eq!(fields.len(), 4);
    fields[0] * 3_600_000 + fields[1] * 60_000 + fields[2] * 1000 + fields[3]
}

fn cue_blocks(body: &str) -> Vec<(i64, i64, String)> {
    body.split("\n\n")
        .filter_map(|block| {
            let mut lines = block.lines();
            let timing = lines.find(|line| line.contains(" --> "))?;
            let (start, end) = timing.split_once(" --> ").unwrap();
            Some((
                milliseconds(start),
                milliseconds(end),
                lines.collect::<Vec<_>>().join("\n"),
            ))
        })
        .collect()
}

fn vtt_cues(documents: &[String]) -> BTreeSet<(i64, i64, String)> {
    documents
        .iter()
        .flat_map(|document| {
            let map = document
                .lines()
                .find_map(|line| line.strip_prefix("X-TIMESTAMP-MAP=LOCAL:00:00:00.000,MPEGTS:"))
                .unwrap()
                .parse::<i64>()
                .unwrap();
            cue_blocks(document)
                .into_iter()
                .map(move |(start, end, text)| (map + start * 90, map + end * 90, text))
        })
        .collect()
}

fn format_ticks(text: &str) -> i64 {
    let (whole, fraction) = text.split_once('.').unwrap();
    assert_eq!(fraction.len(), 6, "pinned ffprobe plain microsecond writer");
    let scale = 1_000_000_i64;
    let numerator = whole.parse::<i64>().unwrap() * scale + fraction.parse::<i64>().unwrap();
    // ffprobe reports AV_TIME_BASE timestamps with six decimal places. The
    // pinned writer's closed one-microsecond enclosure must contain ONE exact
    // 90kHz tick, not a guessed/rounded tick or a widened cue comparison.
    let lower = ((numerator - 1) * 90_000 + scale - 1).div_euclid(scale);
    let upper = ((numerator + 1) * 90_000).div_euclid(scale);
    assert_eq!(
        lower, upper,
        "reported synthetic origin must identify one exact MPEG-TS tick"
    );
    lower
}

async fn check(
    tools: (&Path, &Path),
    source: &Path,
    captured: &Path,
    cues: &[String],
    sidecar: &str,
    interval: (u64, u64, bool),
    proof: &Path,
) {
    let (ffmpeg, ffprobe) = tools;
    copied_video(ffmpeg, source, captured).await;
    copied_audio(ffprobe, source, captured).await;
    let original = packets(ffprobe, source, "v:0", true).await;
    let actual = packets(ffprobe, captured, "v:0", true).await;
    let shift = video_shift(&original, &actual);
    let origin = format_ticks(original["format"]["start_time"].as_str().unwrap());
    assert_ne!(
        origin, 0,
        "source must discriminate full-format origin normalization"
    );
    let (seek, end, captioned) = interval;
    let independent =
        tokio::fs::read_to_string(source.parent().unwrap().join("independent-889.srt"))
            .await
            .unwrap();
    assert_eq!(
        cue_blocks(sidecar),
        cue_blocks(&independent),
        "complete cached text must equal independently decoded page889 timing and payload"
    );
    let first_output_pts = actual["packets"][0]["pts"].as_i64().unwrap();
    let landed_source_ticks = first_output_pts - shift - origin;
    assert!(first_output_pts >= 0 && landed_source_ticks >= 0);
    // Pinned Next0.2.2 render_subtitle_segment fixes the offset using the two
    // independently truncated millisecond coordinates, clamps below zero and
    // nudges the MPEGTS timestamp map by one tick. No timing tolerance is used.
    let offset_ms = first_output_pts / 90 - landed_source_ticks / 90;
    let mapped = |value: i64| 1 + (value + offset_ms).max(0) * 90;
    let join = i64::try_from(seek).unwrap();
    let finish = i64::try_from(end).unwrap();
    let expected = cue_blocks(&independent)
        .into_iter()
        .filter_map(|(start, end, text)| {
            let start = mapped(start.max(join));
            let end = mapped(end.min(finish));
            (end > start).then_some((start, end, text))
        })
        .collect::<BTreeSet<_>>();
    let actual_cues = vtt_cues(cues);
    let video_end = actual["packets"]
        .as_array()
        .unwrap()
        .iter()
        .map(|packet| {
            let duration = packet["duration"].as_i64().unwrap();
            assert!(
                duration > 0,
                "actual copied video packet must cover its frame interval"
            );
            packet["pts"]
                .as_i64()
                .unwrap()
                .checked_add(duration)
                .unwrap()
        })
        .max()
        .unwrap();
    let caption_end = actual_cues.iter().map(|(_, end, _)| *end).max().unwrap();
    assert!(
        video_end >= caption_end,
        "actual delivered videoPTS+frame duration must cover the last delivered caption end"
    );
    tokio::fs::write(proof.join("timing-and-payload.json"),serde_json::to_vec_pretty(&serde_json::json!({
        "original_format_start_ticks":origin,"video_copy_shift_ticks":shift,"source_seek_ms":seek,
        "source_out_ms":end,"expected":expected,"actual":actual_cues,
        "first_output_video_pts_ticks":first_output_pts,"landed_source_ticks":landed_source_ticks,
        "fixed_subtitle_offset_ms":offset_ms,"timestamp_map_nudge_ticks":1,
        "coded_video_pts_dts_association":true,"alternate_aac_payload_identity":true,
        "delivered_video_end_ticks":video_end,"delivered_caption_end_ticks":caption_end,
    })).unwrap()).await.unwrap();
    if captioned {
        for text in ["889 EARLY", "889 LATE", "889 FINAL"] {
            assert!(
                actual_cues.iter().any(|(_, _, body)| body.contains(text)),
                "missing actual {text}"
            );
        }
    } else {
        assert!(expected.is_empty());
    }
    assert_eq!(
        actual_cues, expected,
        "cue timeline must match copied source AV, exact seek and outpoint"
    );
}
