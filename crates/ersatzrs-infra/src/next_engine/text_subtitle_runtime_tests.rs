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

async fn configure_text_tracks(
    pool: &crate::db::DbPool,
    folder: &Path,
    embedded: bool,
    convert: bool,
    off: bool,
) {
    use ersatzrs_core::repositories::PlayoutRepository;
    for sql in [
        r#"DELETE FROM "Subtitle""#,
        r#"DELETE FROM "MediaStream" WHERE "MediaStreamKind" = 3"#,
        r#"INSERT INTO "MediaStream" ("Index", "Codec", "MediaStreamKind", "Language", "Channels", "Title", "Default", "Forced", "MediaVersionId")
            VALUES (3,'subrip',3,'nld',0,'Dutch',1,0,100)"#,
    ] {
        sqlx::query(sql).execute(pool).await.unwrap();
    }
    let extracted = embedded && !convert;
    let path = if extracted {
        Some(folder.join("fixture.nl.srt").to_string_lossy().into_owned())
    } else if embedded {
        None
    } else {
        Some("fixture.nl.srt".into())
    };
    sqlx::query(r#"INSERT INTO "Subtitle" ("MovieMetadataId", "SubtitleKind", "Title", "StreamIndex", "Codec", "Default", "Forced", "SDH", "Language", "IsExtracted", "Path", "DateAdded", "DateUpdated")
        VALUES (20,?,'Dutch',?,'subrip',1,0,0,'nld',?,?,'2026-09-23','2026-09-23')"#)
        .bind(i32::from(!embedded)).bind(if embedded {3} else {0})
        .bind(i32::from(extracted)).bind(path).execute(pool).await.unwrap();
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

#[tokio::test]
#[ignore = "requires built Next engine and FFmpeg runtime; uses disposable synthetic media"]
async fn real_next_text_subtitles_burn_convert_seek_and_preserve_audio_and_off() {
    let binary = runtime_path("ERSATZRS_NEXT_TEST_BIN");
    let ffmpeg = runtime_path("ERSATZRS_NEXT_TEST_FFMPEG");
    let ffprobe = runtime_path("ERSATZRS_NEXT_TEST_FFPROBE");
    let root = tempfile::tempdir().unwrap();
    let clip = generate_text_clip(&ffmpeg, root.path()).await;
    let profile = FFmpegProfile {
        video_format: FFmpegProfileVideoFormat::H264,
        audio_format: FFmpegProfileAudioFormat::Aac,
        video_bitrate: 1000,
        video_buffer_size: 2000,
        audio_sample_rate: 48,
        audio_channels: 1,
        ..Default::default()
    };
    for (embedded, convert, off) in [
        (true, false, false),
        (true, true, false),
        (false, false, false),
        (false, true, false),
        (true, true, true),
    ] {
        let (pool, locker) = prepare_schedule(&clip, false, &profile).await;
        configure_text_tracks(&pool, root.path(), embedded, convert, off).await;
        let host = std::sync::Arc::new(host::NextHost::new(
            pool,
            locker,
            root.path().join("playout"),
        ));
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
        assert!(!output.exists());
    }
}
