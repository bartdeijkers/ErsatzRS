//! Real Next text burn/convert proof with generated media and seeked SRT cues.
use super::*;

const CUES: &str = "1\n00:00:00,000 --> 00:00:05,000\nBEFORE SEEK\n\n2\n00:00:05,000 --> 00:00:55,000\nNext text conversion proof\n\n3\n00:00:55,000 --> 00:01:00,000\nAFTER SEEK\n";

async fn generate_text_clip(ffmpeg: &Path, folder: &Path) -> PathBuf {
    let source = folder.join("source.mkv");
    generate_clip(ffmpeg, &source).await;
    let subtitle = folder.join("fixture.nl.srt");
    std::fs::write(&subtitle, CUES).unwrap();
    let clip = folder.join("text.mkv");
    let mut mux = Command::new(ffmpeg);
    mux.args(["-v", "error", "-nostdin", "-y", "-i"])
        .arg(source)
        .arg("-i")
        .arg(subtitle)
        .args([
            "-map",
            "0",
            "-map",
            "1:0",
            "-c",
            "copy",
            "-metadata:s:s:0",
            "language=nld",
            "-metadata:s:s:0",
            "title=Dutch",
            "-disposition:s:0",
            "default",
        ])
        .arg(&clip);
    tool(mux).await;
    clip
}

async fn configure_text_tracks(pool: &crate::db::DbPool, embedded: bool, convert: bool, off: bool) {
    use ersatzrs_core::repositories::PlayoutRepository;
    for sql in [
        r#"DELETE FROM "Subtitle""#,
        r#"DELETE FROM "MediaStream" WHERE "MediaStreamKind" = 3"#,
        r#"INSERT INTO "MediaStream" ("Index", "Codec", "MediaStreamKind", "Language", "Channels", "Title", "Default", "Forced", "MediaVersionId")
            VALUES (3,'subrip',3,'nld',0,'Dutch',1,0,100)"#,
    ] {
        sqlx::query(sql).execute(pool).await.unwrap();
    }
    // Both modes start with unextracted embedded metadata. Burn must use the
    // production demand-time callback and extract the selected original stream.
    let path = (!embedded).then_some("fixture.nl.srt");
    sqlx::query(r#"INSERT INTO "Subtitle" ("MovieMetadataId", "SubtitleKind", "Title", "StreamIndex", "Codec", "Default", "Forced", "SDH", "Language", "IsExtracted", "Path", "DateAdded", "DateUpdated")
        VALUES (20,?,'Dutch',?,'subrip',1,0,0,'nld',?,?,'2026-09-23','2026-09-23')"#)
        .bind(i32::from(!embedded)).bind(if embedded {3} else {0})
        .bind(0).bind(path).execute(pool).await.unwrap();
    sqlx::query(r#"UPDATE "Channel" SET "NextEngineTextSubtitleMode" = ? WHERE "Id" = 1"#)
        .bind(i32::from(convert))
        .execute(pool)
        .await
        .unwrap();
    let mut item = selection::tests::item();
    item.id = 1;
    item.playout_id = 1;
    item.in_point = chrono::Duration::seconds(5);
    item.out_point = chrono::Duration::seconds(55);
    item.finish = item.start + chrono::Duration::seconds(50);
    if off {
        item.pinned_stream_selection = Some(ersatzrs_domain::PinnedStreamSelection {
            media_version_id: 100,
            video_stream_index: 0,
            audio_stream_index: Some(2),
            subtitle_stream_index: None,
        });
    }
    crate::repositories::make_repositories(pool.clone())
        .playout
        .upsert_item(&item)
        .await
        .unwrap();
}

async fn assert_text_video_and_audio(ffmpeg: &Path, segment: &Path, burned: bool) {
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
            "crop=320:48:0:132",
            "-pix_fmt",
            "rgb24",
            "-f",
            "rawvideo",
            "pipe:1",
        ]);
    let pixels = tool(video).await;
    assert_eq!(pixels.len(), 320 * 48 * 3);
    let white = pixels
        .chunks_exact(3)
        .filter(|pixel| pixel.iter().all(|c| *c > 180))
        .count();
    if burned {
        assert!(white > 20, "Burn must render subtitle glyphs");
    } else {
        assert_eq!(
            white, 0,
            "Convert/off must leave video free of subtitle glyphs"
        );
    }
    let mut audio = Command::new(ffmpeg);
    audio
        .args(["-v", "error", "-nostdin", "-i"])
        .arg(segment)
        .args([
            "-map", "0:a:0", "-t", "1", "-ac", "1", "-ar", "8000", "-f", "s16le", "pipe:1",
        ]);
    let pcm = tool(audio).await;
    let samples: Vec<_> = pcm
        .chunks_exact(2)
        .map(|b| i16::from_le_bytes([b[0], b[1]]))
        .collect();
    assert!(samples.len() >= 7000);
    let crossings = samples.windows(2).filter(|p| p[0] <= 0 && p[1] > 0).count();
    let hz = f64::from(u32::try_from(crossings).unwrap()) * 8000.0
        / f64::from(u32::try_from(samples.len()).unwrap());
    assert!(
        (820.0..940.0).contains(&hz),
        "selected Dutch audio must survive: {hz}"
    );
}

fn vtt_seconds(text: &str) -> f64 {
    let fields: Vec<f64> = text.split(':').map(|part| part.parse().unwrap()).collect();
    assert_eq!(fields.len(), 3);
    fields[0] * 3600.0 + fields[1] * 60.0 + fields[2]
}

fn assert_text_vtt(body: &str, converted: bool, segment_duration: f64) {
    assert!(body.starts_with("WEBVTT\n"));
    let map = body
        .lines()
        .find_map(|line| line.strip_prefix("X-TIMESTAMP-MAP=LOCAL:00:00:00.000,MPEGTS:"))
        .expect("VTT timestamp map");
    assert!(map.parse::<u64>().unwrap() < 8_589_934_592);
    assert!(!body.contains("BEFORE SEEK"));
    assert!(!body.contains("AFTER SEEK"));
    if converted {
        assert!(body.contains("Next text conversion proof"));
        let cue = body
            .lines()
            .find_map(|line| line.split_once(" --> "))
            .expect("converted timed cue");
        assert_eq!(
            cue.0, "00:00:00.000",
            "active seeked cue starts at segment origin"
        );
        let end = vtt_seconds(cue.1);
        assert!(
            end > 0.0 && (end - segment_duration).abs() < 0.002,
            "active cue must clip at segment end: {end} vs {segment_duration}"
        );
    } else {
        assert!(
            !body.contains(" --> "),
            "Burn/off must not publish text cues"
        );
    }
}

/// Keep only transport framing in the fixture; authentication, SQL selection,
/// extraction, cache admission and callback timing use the production host.
struct CallbackListener(tokio::task::JoinHandle<()>);

impl CallbackListener {
    async fn stop(&mut self) {
        self.0.abort();
        assert!((&mut self.0)
            .await
            .as_ref()
            .is_err_and(tokio::task::JoinError::is_cancelled));
    }
}

impl Drop for CallbackListener {
    fn drop(&mut self) {
        self.0.abort();
    }
}

async fn callback_listener(host: std::sync::Arc<host::NextHost>) -> CallbackListener {
    use tokio::io::{AsyncReadExt as _, AsyncWriteExt as _};
    let listener = tokio::net::TcpListener::bind((std::net::Ipv4Addr::LOCALHOST, 0))
        .await
        .unwrap();
    host.media
        .configure(listener.local_addr().unwrap())
        .unwrap();
    CallbackListener(tokio::spawn(async move {
        loop {
            let (mut socket, _) = listener.accept().await.unwrap();
            timeout(Duration::from_secs(35), async {
                let mut bytes = Vec::new();
                while !bytes.ends_with(b"\r\n\r\n") {
                    assert!(bytes.len() < 16 * 1024, "callback headers exceed fixture limit");
                    bytes.push(socket.read_u8().await.unwrap());
                }
                let request = std::str::from_utf8(&bytes).unwrap();
                let mut lines = request.split("\r\n");
                let target = lines.next().unwrap().strip_prefix("GET ").unwrap()
                    .strip_suffix(" HTTP/1.1").unwrap();
                let headers: Vec<_> = lines.filter_map(|line| line.split_once(':')).collect();
                let header = |name: &str| headers.iter()
                    .find(|(key, _)| key.eq_ignore_ascii_case(name))
                    .map(|(_, value)| value.trim());
                let authorized = header(crate::next_engine::media::ACCESS_HEADER)
                    .is_some_and(|value| host.media.authorized(value));
                if authorized && target.starts_with("/ffmpeg/next-prepared/") {
                    let parts: Vec<_> = target.trim_start_matches("/ffmpeg/next-prepared/").split('/').collect();
                    assert_eq!(parts.len(), 2);
                    let mut reader = host.claim_prepared(parts[0].parse().unwrap(),parts[1].parse().unwrap()).await.unwrap();
                    socket.write_all(b"HTTP/1.1 200 OK\r\nContent-Type: application/octet-stream\r\nConnection: close\r\n\r\n").await.unwrap();
                    let _ = tokio::io::copy(&mut reader,&mut socket).await;
                    let _ = socket.shutdown().await;
                    return;
                }
                let (status, body) = if authorized {
                    let parts: Vec<_> = target.strip_prefix("/ffmpeg/next-item/").unwrap()
                        .split('/').collect();
                    assert_eq!(parts.len(), 4);
                    let now = chrono::DateTime::parse_from_rfc3339(header("x-etv-now").unwrap())
                        .unwrap().with_timezone(&chrono::Utc);
                    let until = chrono::DateTime::parse_from_rfc3339(header("x-etv-until").unwrap())
                        .unwrap().with_timezone(&chrono::Utc);
                    match host.resolve_dynamic(parts[0].parse().unwrap(), parts[1].parse().unwrap(),
                        parts[2], if parts[3] == "off" { None } else { Some(parts[3].parse().unwrap()) }, header("x-etv-channel").unwrap(),
                        uuid::Uuid::parse_str(header(crate::next_engine::media::SESSION_HEADER)
                            .expect("official Next must expand the session header")).unwrap(), now, until).await {
                        Ok(item) => ("200 OK", serde_json::to_vec(&item).unwrap()),
                        Err(error) => panic!("production callback failed: {error}"),
                    }
                } else {
                    ("401 Unauthorized", Vec::new())
                };
                let response = format!("HTTP/1.1 {status}\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n", body.len());
                socket.write_all(response.as_bytes()).await.unwrap();
                socket.write_all(&body).await.unwrap();
                socket.shutdown().await.unwrap();
            }).await.unwrap();
        }
    }))
}

#[tokio::test]
#[ignore = "requires built Next engine and FFmpeg runtime; uses disposable synthetic media"]
async fn real_next_text_subtitles_burn_convert_seek_and_preserve_audio_and_off() {
    text_runtime_cases(false).await;
}

#[tokio::test]
#[ignore = "requires official Next and FFmpeg; isolated app data under the temporary directory"]
async fn real_next_prepared_artifact_callbacks_preserve_text_audio_and_seek() {
    prepared_artifact_runtime::run_isolated().await;
}

#[tokio::test]
#[ignore = "child of real_next_prepared_artifact_callbacks_preserve_text_audio_and_seek"]
async fn fixture_child_prepared_artifacts() {
    text_runtime_cases(true).await;
}

fn text_profile() -> FFmpegProfile {
    FFmpegProfile {
        video_format: FFmpegProfileVideoFormat::H264,
        audio_format: FFmpegProfileAudioFormat::Aac,
        video_bitrate: 1000,
        video_buffer_size: 2000,
        audio_sample_rate: 48,
        audio_channels: 1,
        ..Default::default()
    }
}

async fn text_runtime_cases(prepared_artifact: bool) {
    let binary = runtime_path("ERSATZRS_NEXT_TEST_BIN");
    let ffmpeg = runtime_path("ERSATZRS_NEXT_TEST_FFMPEG");
    let ffprobe = runtime_path("ERSATZRS_NEXT_TEST_FFPROBE");
    let root = if prepared_artifact {
        prepared_artifact_runtime::temporary_root().await
    } else {
        tempfile::tempdir().unwrap()
    };
    let clip = generate_text_clip(&ffmpeg, root.path()).await;
    let profile = text_profile();
    for (embedded, convert, off) in [
        (true, false, false),
        (true, true, false),
        (false, false, false),
        (false, true, false),
        (true, true, true),
    ] {
        let (pool, locker) = prepare_schedule(&clip, false, &profile).await;
        configure_text_tracks(&pool, embedded, convert, off).await;
        if prepared_artifact {
            prepared_artifact_runtime::publish_fixture(&pool, &clip).await;
        }
        let host = std::sync::Arc::new(host::NextHost::new(
            pool,
            locker,
            root.path().join("playout"),
        ));
        let mut callback = callback_listener(host.clone()).await;
        let output = root.path().join("output");
        let registry = std::sync::Arc::new(crate::hls::InMemoryHlsSessionRegistry::new());
        let task = host
            .start(
                registry.clone(),
                host::NextStart {
                    channel_id: 1,
                    preview: false,
                    number: "1".into(),
                    executable: binary.clone(),
                    output: output.clone(),
                    ffmpeg: ffmpeg.clone(),
                    ffprobe: ffprobe.clone(),
                    replacements: Vec::new(),
                    idle_timeout: None,
                    deadline: Instant::now() + Duration::from_secs(30),
                },
            )
            .await
            .unwrap();
        let handle = registry.try_get("1").await.unwrap();
        timeout(Duration::from_secs(30), async {
            loop {
                match handle.startup_readiness() {
                    HlsStartupReadiness::Ready => break,
                    HlsStartupReadiness::Failed => panic!(
                        "text session failed: embedded={embedded} convert={convert} off={off}"
                    ),
                    _ => tokio::time::sleep(Duration::from_millis(10)).await,
                }
            }
        })
        .await
        .unwrap();
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
        let segment_duration: f64 = playlist
            .lines()
            .find_map(|line| line.strip_prefix("#EXTINF:"))
            .unwrap()
            .trim_end_matches(',')
            .parse()
            .unwrap();
        let vtt_path = output.join(segment).with_extension("vtt");
        let body = timeout(Duration::from_secs(5), async {
            loop {
                if let Ok(body) = tokio::fs::read_to_string(&vtt_path).await {
                    break body;
                }
                tokio::time::sleep(Duration::from_millis(10)).await;
            }
        })
        .await
        .unwrap();
        assert_text_vtt(&body, convert && !off, segment_duration);
        assert_text_video_and_audio(&ffmpeg, &output.join(segment), !convert && !off).await;
        assert!(registry.stop_channel("1").await);
        task.await.unwrap();
        callback.stop().await;
        assert!(!output.exists());
    }
}

#[path = "watermark_runtime_tests.rs"]
mod watermark_runtime_tests;

#[path = "provider_text_runtime_tests.rs"]
mod provider_text_runtime_tests;

#[path = "prepared_artifact_runtime_tests.rs"]
mod prepared_artifact_runtime;

// POSIX-shell fixture add-on; native production code remains platform-neutral.
#[cfg(unix)]
#[path = "retained_callback_runtime_tests.rs"]
mod retained_callback_runtime;

// Installed fixture add-on uses a POSIX shell; production remains portable.
#[cfg(unix)]
#[path = "ordinary_callback_runtime_tests.rs"]
mod ordinary_callback_runtime;

#[path = "direct_remote_runtime_tests.rs"]
mod direct_remote_runtime;
