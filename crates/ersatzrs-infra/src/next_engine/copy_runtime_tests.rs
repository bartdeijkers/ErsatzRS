//! Prove copying with compressed payload identity, not codec names alone.
use super::*;

async fn capture(root: &Path, clip: &Path, codecs: (&str, &str), watermark: bool) -> PathBuf {
    let profile = FFmpegProfile {
        video_format: FFmpegProfileVideoFormat::Copy,
        audio_format: FFmpegProfileAudioFormat::Copy,
        video_bitrate: 1000,
        video_buffer_size: 2000,
        audio_sample_rate: 32,
        audio_channels: 2,
        ..Default::default()
    };
    let (pool, locker) = prepare_schedule(clip, false, &profile).await;
    // The real scanner persists these codecs. The shared fixture starts as
    // H.264/AAC, so converted media must update the metadata used by probe hints.
    for (kind, codec) in [(1, codecs.0), (2, codecs.1)] {
        sqlx::query(r#"UPDATE "MediaStream" SET "Codec"=? WHERE "MediaStreamKind"=?"#)
            .bind(codec)
            .bind(kind)
            .execute(&pool)
            .await
            .unwrap();
    }
    // generate_clip creates two mono tracks, not the shared fixture's stereo.
    sqlx::query(r#"UPDATE "MediaStream" SET "Channels"=1 WHERE "MediaStreamKind"=2"#)
        .execute(&pool)
        .await
        .unwrap();
    sqlx::query(r#"UPDATE "Channel" SET "SubtitleMode"=0"#)
        .execute(&pool)
        .await
        .unwrap();
    sqlx::query(r#"UPDATE "Resolution" SET "Width"=640,"Height"=360 WHERE "Id"=1"#)
        .execute(&pool)
        .await
        .unwrap();
    if watermark {
        let image = root.join("logo.ppm");
        let mut pixels = b"P6\n40 20\n255\n".to_vec();
        pixels.extend([255, 0, 0].repeat(40 * 20));
        tokio::fs::write(&image, pixels).await.unwrap();
        sqlx::query(r#"INSERT INTO "ChannelWatermark" ("Id","Name","Mode","ImageSource","Location","Size","WidthPercent","HorizontalMarginPercent","VerticalMarginPercent","FrequencyMinutes","DurationSeconds","Opacity","PlaceWithinSourceContent","ZIndex") VALUES (1,'Copy fallback fixture',1,1,2,1,20,5,5,0,0,100,0,0)"#)
            .execute(&pool).await.unwrap();
        sqlx::query(r#"INSERT INTO "Artwork" ("ArtworkKind","ChannelId","Path","DateAdded","DateUpdated") VALUES (2,1,?,'2026-10-06','2026-10-06')"#)
            .bind(image.to_str().unwrap()).execute(&pool).await.unwrap();
        sqlx::query(r#"UPDATE "Channel" SET "WatermarkId"=1 WHERE "Id"=1"#)
            .execute(&pool)
            .await
            .unwrap();
    }
    let host = std::sync::Arc::new(host::NextHost::new(pool, locker, root.join("playout")));
    let registry = std::sync::Arc::new(crate::hls::InMemoryHlsSessionRegistry::new());
    let output = root.join("output");
    let task = host
        .start(
            registry.clone(),
            host::NextStart {
                channel_id: 1,
                preview: false,
                number: "1".into(),
                executable: runtime_path("ERSATZRS_NEXT_TEST_BIN"),
                output: output.clone(),
                ffmpeg: runtime_path("ERSATZRS_NEXT_TEST_FFMPEG"),
                ffprobe: runtime_path("ERSATZRS_NEXT_TEST_FFPROBE"),
                replacements: Vec::new(),
                idle_timeout: None,
                deadline: Instant::now() + Duration::from_secs(30),
            },
        )
        .await
        .unwrap();
    let saved = root.join("captured.ts");
    let captured = timeout(Duration::from_secs(30), async {
        let handle = registry.try_get("1").await.unwrap();
        loop {
            if handle.startup_readiness() == HlsStartupReadiness::Failed {
                return Err(std::io::Error::other("Next startup failed"));
            }
            if let Some(playlist) = handle.trim_playlist().await {
                if let Some(segment) = playlist.lines().find(|line| {
                    !line.starts_with('#')
                        && Path::new(line)
                            .extension()
                            .is_some_and(|extension| extension.eq_ignore_ascii_case("ts"))
                }) {
                    tokio::fs::copy(output.join(segment), &saved).await?;
                    return Ok(());
                }
            }
            tokio::time::sleep(Duration::from_millis(10)).await;
        }
    })
    .await;
    registry.stop_channel("1").await;
    task.await.unwrap();
    captured.expect("copy capture timed out").unwrap();
    saved
}

async fn elementary(ffmpeg: &Path, path: &Path, stream: &str, format: &str) -> Vec<u8> {
    let mut command = Command::new(ffmpeg);
    command.args(["-v", "error", "-nostdin", "-i"]).arg(path);
    command.args(["-map", stream, "-t", "10", "-c", "copy", "-f", format]);
    if format == "h264" {
        command.args(["-bsf:v", "h264_mp4toannexb"]);
    }
    command.arg("pipe:1");
    tool(command).await
}

fn aac_frames(bytes: &[u8]) -> Vec<&[u8]> {
    let mut frames = Vec::new();
    let mut offset = 0;
    while offset < bytes.len() {
        let header = &bytes[offset..];
        assert!(header.len() >= 7 && header[0] == 0xff && header[1] & 0xf6 == 0xf0);
        let length = (usize::from(header[3] & 3) << 11)
            | (usize::from(header[4]) << 3)
            | usize::from(header[5] >> 5);
        let header_length = if header[1] & 1 == 0 { 9 } else { 7 };
        assert!(length > header_length && length <= header.len());
        frames.push(&header[header_length..length]);
        offset += length;
    }
    frames
}

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

#[tokio::test]
#[ignore = "requires real Next and FFmpeg 9; compares compressed selected-stream payloads"]
async fn real_next_copies_selected_payloads_without_applying_fallback_normalization() {
    let ffmpeg = runtime_path("ERSATZRS_NEXT_TEST_FFMPEG");
    let ffprobe = runtime_path("ERSATZRS_NEXT_TEST_FFPROBE");
    let root = tempfile::tempdir().unwrap();
    let clip = root.path().join("fixture.mkv");
    generate_clip(&ffmpeg, &clip).await;
    let saved = capture(root.path(), &clip, ("h264", "aac"), false).await;
    let mut probe = Command::new(ffprobe);
    probe
        .args(["-v", "error", "-show_streams", "-of", "json"])
        .arg(&saved);
    let info: serde_json::Value = serde_json::from_slice(&tool(probe).await).unwrap();
    let streams = info["streams"].as_array().unwrap();
    assert!(streams
        .iter()
        .any(|s| s["codec_name"] == "h264" && s["width"] == 320 && s["height"] == 180));
    assert!(streams
        .iter()
        .any(|s| s["codec_name"] == "aac" && s["sample_rate"] == "48000" && s["channels"] == 1));
    let source_audio = elementary(&ffmpeg, &clip, "0:a:1", "adts").await;
    let copied_audio = elementary(&ffmpeg, &saved, "0:a:0", "adts").await;
    let source_frames = aac_frames(&source_audio);
    let copied_frames = aac_frames(&copied_audio);
    assert!(copied_frames.len() >= 40, "need a useful copy interval");
    assert!(
        copied_frames
            .iter()
            .all(|frame| source_frames.contains(frame)),
        "AAC payloads must match the selected alternate track exactly"
    );
    let source_video = elementary(&ffmpeg, &clip, "0:v:0", "h264").await;
    let copied_video = elementary(&ffmpeg, &saved, "0:v:0", "h264").await;
    let source_slices = video_slices(&source_video);
    let copied_slices = video_slices(&copied_video);
    assert!(copied_slices.len() >= 20);
    assert!(
        copied_slices
            .iter()
            .all(|slice| source_slices.contains(slice)),
        "H.264 coded slices must be copied unchanged"
    );
    let mut decode = Command::new(ffmpeg);
    decode
        .args(["-v", "error", "-xerror", "-i"])
        .arg(saved)
        .args(["-f", "null", "-"]);
    tool(decode).await;
}

#[tokio::test]
#[ignore = "requires real Next and FFmpeg 9; decodes fallback from non-copyable source codecs"]
async fn real_next_copy_profile_transcodes_unsupported_codecs_with_profile_settings() {
    let ffmpeg = runtime_path("ERSATZRS_NEXT_TEST_FFMPEG");
    let ffprobe = runtime_path("ERSATZRS_NEXT_TEST_FFPROBE");
    let root = tempfile::tempdir().unwrap();
    let source = root.path().join("source.mkv");
    let clip = root.path().join("fixture.mkv");
    generate_clip(&ffmpeg, &source).await;
    let mut convert = Command::new(&ffmpeg);
    convert
        .args(["-v", "error", "-nostdin", "-i"])
        .arg(source)
        .args(["-map", "0", "-c:v", "ffv1", "-c:a", "pcm_s16le"])
        .arg(&clip);
    tool(convert).await;
    let saved = capture(root.path(), &clip, ("ffv1", "pcm_s16le"), false).await;
    let mut probe = Command::new(ffprobe);
    probe
        .args(["-v", "error", "-show_streams", "-of", "json"])
        .arg(&saved);
    let info: serde_json::Value = serde_json::from_slice(&tool(probe).await).unwrap();
    let streams = info["streams"].as_array().unwrap();
    assert!(streams
        .iter()
        .any(|s| s["codec_name"] == "h264" && s["width"] == 640 && s["height"] == 360));
    assert!(streams
        .iter()
        .any(|s| s["codec_name"] == "aac" && s["sample_rate"] == "32000" && s["channels"] == 2));
    let mut audio = Command::new(&ffmpeg);
    audio
        .args(["-v", "error", "-nostdin", "-i"])
        .arg(&saved)
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
        "fallback lost selected alternate audio: {frequency}; retained synthetic fixture: {}",
        root.keep().display()
    );
    let mut decode = Command::new(ffmpeg);
    decode
        .args(["-v", "error", "-xerror", "-i"])
        .arg(saved)
        .args(["-f", "null", "-"]);
    tool(decode).await;
}

#[tokio::test]
#[ignore = "requires real Next and FFmpeg 9; graphics force video fallback while audio remains copied"]
async fn real_next_copy_profile_burns_graphics_without_transcoding_selected_audio() {
    let ffmpeg = runtime_path("ERSATZRS_NEXT_TEST_FFMPEG");
    let ffprobe = runtime_path("ERSATZRS_NEXT_TEST_FFPROBE");
    let root = tempfile::tempdir().unwrap();
    let clip = root.path().join("fixture.mkv");
    generate_clip(&ffmpeg, &clip).await;
    let saved = capture(root.path(), &clip, ("h264", "aac"), true).await;
    let mut probe = Command::new(ffprobe);
    probe
        .args(["-v", "error", "-show_streams", "-of", "json"])
        .arg(&saved);
    let info: serde_json::Value = serde_json::from_slice(&tool(probe).await).unwrap();
    assert!(info["streams"]
        .as_array()
        .unwrap()
        .iter()
        .any(|s| s["width"] == 640 && s["height"] == 360));
    let mut decode = Command::new(&ffmpeg);
    decode
        .args(["-v", "error", "-xerror", "-i"])
        .arg(&saved)
        .args([
            "-map",
            "0:v:0",
            "-frames:v",
            "1",
            "-vf",
            "scale=160:90",
            "-pix_fmt",
            "rgb24",
            "-f",
            "rawvideo",
            "pipe:1",
        ]);
    let pixels = tool(decode).await;
    assert_eq!(pixels.len(), 160 * 90 * 3);
    let red = pixels
        .chunks_exact(3)
        .filter(|p| p[0] > 180 && p[1] < 50 && p[2] < 50)
        .count();
    assert!(red > 20, "watermark must be visible on transcoded video");
    let source_audio = elementary(&ffmpeg, &clip, "0:a:1", "adts").await;
    let output_audio = elementary(&ffmpeg, &saved, "0:a:0", "adts").await;
    let source_frames = aac_frames(&source_audio);
    let output_frames = aac_frames(&output_audio);
    assert!(output_frames.len() >= 40);
    assert!(
        output_frames
            .iter()
            .all(|frame| source_frames.contains(frame)),
        "graphics must not re-encode the selected AAC stream"
    );
}

#[tokio::test]
#[ignore = "requires published Next and FFmpeg 9; decodes persisted AVI with selected alternate audio"]
async fn real_next_avi_hint_preserves_picture_and_selected_audio() {
    let ffmpeg = runtime_path("ERSATZRS_NEXT_TEST_FFMPEG");
    let root = tempfile::tempdir().unwrap();
    let original = root.path().join("source.mkv");
    let avi = root.path().join("fixture.AVI");
    generate_clip(&ffmpeg, &original).await;
    let mut remux = Command::new(&ffmpeg);
    remux
        .args(["-v", "error", "-nostdin", "-i"])
        .arg(&original)
        .args(["-map", "0", "-c", "copy", "-bsf:v", "h264_mp4toannexb"])
        .arg(&avi);
    tool(remux).await;
    let saved = capture(root.path(), &avi, ("h264", "aac"), false).await;
    let mut probe = Command::new(runtime_path("ERSATZRS_NEXT_TEST_FFPROBE"));
    probe
        .args(["-v", "error", "-show_streams", "-of", "json"])
        .arg(&saved);
    let info: serde_json::Value = serde_json::from_slice(&tool(probe).await).unwrap();
    let streams = info["streams"].as_array().unwrap();
    // Next's ContainerWithoutPts blocker must use the configured fallback.
    assert!(streams
        .iter()
        .any(|s| s["width"] == 640 && s["height"] == 360));
    assert!(streams
        .iter()
        .any(|s| s["codec_name"] == "aac" && s["sample_rate"] == "32000" && s["channels"] == 2));
    let mut audio = Command::new(&ffmpeg);
    audio
        .args(["-v", "error", "-nostdin", "-i"])
        .arg(&saved)
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
    let frequency = f64::from(u32::try_from(crossings).unwrap()) * 8000.0
        / f64::from(u32::try_from(samples.len()).unwrap());
    assert!(
        (860.0..900.0).contains(&frequency),
        "selected Dutch audio: {frequency} Hz"
    );
    let mut decode = Command::new(&ffmpeg);
    decode
        .args(["-v", "error", "-xerror", "-i"])
        .arg(&saved)
        .args([
            "-map",
            "0:v:0",
            "-frames:v",
            "1",
            "-vf",
            "scale=32:18",
            "-pix_fmt",
            "rgb24",
            "-f",
            "rawvideo",
            "pipe:1",
        ]);
    let pixels = tool(decode).await;
    assert_eq!(pixels.len(), 32 * 18 * 3);
    assert!(pixels
        .chunks_exact(3)
        .all(|pixel| pixel[2] > 200 && pixel[0] < 30 && pixel[1] < 30));
}
