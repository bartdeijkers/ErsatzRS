//! Synthetic PGS proof: English draws a left rectangle, Dutch a right rectangle.
//! Packet fields follow `FFmpeg`'s `libavformat/supdec.c` and
//! `libavcodec/pgssubdec.c` (presentation, palette, object and display segments).
//! No downloaded media, fonts, subtitle encoder or copyrighted fixture is required.

use super::*;

fn segment(output: &mut Vec<u8>, second: u32, kind: u8, payload: &[u8]) {
    output.extend_from_slice(b"PG");
    for _ in 0..2 {
        output.extend_from_slice(&(second * 90_000).to_be_bytes());
    }
    output.push(kind);
    output.extend_from_slice(&u16::try_from(payload.len()).unwrap().to_be_bytes());
    output.extend_from_slice(payload);
}

fn bitmap_subtitles(x: u16) -> Vec<u8> {
    let mut output = Vec::new();
    // Independent epochs each second keep the fixture seekable at any live join.
    for second in 0_u16..60 {
        let mut presentation = Vec::new();
        presentation.extend_from_slice(&320_u16.to_be_bytes());
        presentation.extend_from_slice(&180_u16.to_be_bytes());
        presentation.push(0x30); // 25 fps.
        presentation.extend_from_slice(&second.to_be_bytes());
        presentation.extend_from_slice(&[0x80, 0, 0, 1, 0, 0, 0, 0]);
        presentation.extend_from_slice(&x.to_be_bytes());
        presentation.extend_from_slice(&140_u16.to_be_bytes());
        segment(&mut output, second.into(), 0x16, &presentation);
        let mut window = vec![1, 0];
        for value in [x, 140, 60, 24] {
            window.extend_from_slice(&value.to_be_bytes());
        }
        segment(&mut output, second.into(), 0x17, &window);
        // Palette 0: transparent background and opaque neutral white (YCbCr).
        segment(
            &mut output,
            second.into(),
            0x14,
            &[0, 0, 0, 16, 128, 128, 0, 1, 235, 128, 128, 255],
        );
        // Object 0, complete sequence, 124-byte data including dimensions.
        let mut object = vec![0, 0, 0, 0xc0, 0, 0, 124, 0, 60, 0, 24];
        for _ in 0..24 {
            object.extend_from_slice(&[0, 0xbc, 1, 0, 0]); // 60 white pixels, end of row.
        }
        segment(&mut output, second.into(), 0x15, &object);
        segment(&mut output, second.into(), 0x80, &[]);
    }
    output
}

pub(super) async fn generate_bitmap_clip(ffmpeg: &Path, folder: &Path) -> PathBuf {
    let source = folder.join("source.mkv");
    generate_clip(ffmpeg, &source).await;
    let english = folder.join("english.sup");
    let dutch = folder.join("dutch.sup");
    std::fs::write(&english, bitmap_subtitles(30)).unwrap();
    std::fs::write(&dutch, bitmap_subtitles(230)).unwrap();
    let clip = folder.join("bitmap.mkv");
    let mut mux = Command::new(ffmpeg);
    mux.args(["-v", "error", "-nostdin", "-y", "-i"])
        .arg(source)
        .arg("-i")
        .arg(english)
        .arg("-i")
        .arg(dutch)
        .args([
            "-map",
            "0",
            "-map",
            "1:0",
            "-map",
            "2:0",
            "-c",
            "copy",
            "-metadata:s:s:0",
            "language=eng",
            "-metadata:s:s:0",
            "title=English",
            "-metadata:s:s:1",
            "language=nld",
            "-metadata:s:s:1",
            "title=Dutch",
            "-disposition:s:0",
            "default",
            "-disposition:s:1",
            "0",
        ])
        .arg(&clip);
    tool(mux).await;
    clip
}

async fn configure_bitmap_tracks(pool: &crate::db::DbPool, subtitles_off: bool) {
    for sql in [
        r#"DELETE FROM "Subtitle""#,
        r#"DELETE FROM "MediaStream" WHERE "MediaStreamKind"=3"#,
        r#"INSERT INTO "MediaStream" ("Id", "Index", "Codec", "MediaStreamKind", "Language", "Channels", "Title", "Default", "Forced", "MediaVersionId") VALUES
            (4,3,'hdmv_pgs_subtitle',3,'eng',0,'English',1,0,100),
            (5,4,'hdmv_pgs_subtitle',3,'nld',0,'Dutch',0,0,100)"#,
        r#"INSERT INTO "Subtitle" ("Id", "MovieMetadataId", "SubtitleKind", "Title", "StreamIndex", "Codec", "Default", "Forced", "SDH", "Language", "IsExtracted", "Path", "DateAdded", "DateUpdated") VALUES
            (1,20,0,'English',3,'hdmv_pgs_subtitle',1,0,0,'eng',0,NULL,'2026-09-22 00:00:00','2026-09-22 00:00:00'),
            (2,20,0,'Dutch',4,'hdmv_pgs_subtitle',0,0,0,'nld',0,NULL,'2026-09-22 00:00:00','2026-09-22 00:00:00')"#,
    ] {
        sqlx::query(sql).execute(pool).await.unwrap();
    }
    if subtitles_off {
        use ersatzrs_core::repositories::PlayoutRepository;
        let mut item = selection::tests::item();
        item.id = 1;
        item.playout_id = 1;
        item.in_point = chrono::Duration::zero();
        item.out_point = chrono::Duration::seconds(60);
        item.pinned_stream_selection = Some(ersatzrs_domain::PinnedStreamSelection {
            media_version_id: 100,
            video_stream_index: 0,
            audio_stream_index: Some(2),
            subtitle_stream_index: None,
        });
        crate::repositories::make_repositories(pool.clone())
            .playout
            .upsert_item(&item)
            .await
            .unwrap();
    }
}

pub(super) async fn assert_bitmap_output(ffmpeg: &Path, segment: &Path, subtitles_off: bool) {
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
    let mut left = 0;
    let mut right = 0;
    for (index, pixel) in pixels.chunks_exact(3).enumerate() {
        if pixel.iter().all(|channel| *channel > 180) {
            if index % 320 < 160 {
                left += 1;
            } else {
                right += 1;
            }
        }
    }
    assert_eq!(
        left, 0,
        "default English image must not override preferred Dutch"
    );
    if subtitles_off {
        assert_eq!(
            right, 0,
            "explicit subtitles-off must hide the embedded bitmap"
        );
    } else {
        assert!(
            right > 1000,
            "preferred Dutch bitmap must appear in decoded output"
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
    let samples: Vec<i16> = pcm
        .chunks_exact(2)
        .map(|b| i16::from_le_bytes([b[0], b[1]]))
        .collect();
    assert!(samples.len() >= 7000);
    let crossings = samples
        .windows(2)
        .filter(|pair| pair[0] <= 0 && pair[1] > 0)
        .count();
    let frequency = f64::from(u32::try_from(crossings).unwrap()) * 8000.0
        / f64::from(u32::try_from(samples.len()).unwrap());
    assert!(
        (820.0..940.0).contains(&frequency),
        "Dutch audio must survive both subtitle modes: {frequency}"
    );
}

#[tokio::test]
#[ignore = "requires built Next engine and FFmpeg runtime; uses disposable synthetic media"]
async fn real_next_hls_burns_preferred_embedded_bitmap_and_respects_subtitles_off() {
    let binary = runtime_path("ERSATZRS_NEXT_TEST_BIN");
    let ffmpeg = runtime_path("ERSATZRS_NEXT_TEST_FFMPEG");
    let ffprobe = runtime_path("ERSATZRS_NEXT_TEST_FFPROBE");
    let root = tempfile::tempdir().unwrap();
    let clip = generate_bitmap_clip(&ffmpeg, root.path()).await;
    let profile = FFmpegProfile {
        video_format: FFmpegProfileVideoFormat::H264,
        audio_format: FFmpegProfileAudioFormat::Aac,
        video_bitrate: 1000,
        video_buffer_size: 2000,
        audio_sample_rate: 48,
        audio_channels: 1,
        ..Default::default()
    };
    for (convert, subtitles_off) in [(false, false), (true, false), (false, true), (true, true)] {
        let (pool, locker) = prepare_schedule(&clip, false, &profile).await;
        configure_bitmap_tracks(&pool, subtitles_off).await;
        sqlx::query(r#"UPDATE "Channel" SET "NextEngineTextSubtitleMode" = ? WHERE "Id" = 1"#)
            .bind(i32::from(convert))
            .execute(&pool)
            .await
            .unwrap();
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
                    HlsStartupReadiness::Failed => panic!("bitmap Next session startup failed"),
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
                        .is_some_and(|extension| extension.eq_ignore_ascii_case("ts"))
            })
            .unwrap();
        assert_bitmap_output(&ffmpeg, &output.join(segment), subtitles_off).await;
        assert!(registry.stop_channel("1").await);
        task.await.unwrap();
        assert!(!output.exists());
    }
}
