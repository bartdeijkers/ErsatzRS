//! Decode the published Next package, proving rotation hints are applied once.
use super::*;
use crate::next_engine::protocol as wire;

#[tokio::test]
#[ignore = "requires official Next and verified FFmpeg; synthetic rotation pixels and audio"]
async fn real_next_rotation_hints_preserve_orientation_geometry_and_audio() {
    let binary = runtime_path("ERSATZRS_NEXT_TEST_BIN");
    let ffmpeg = runtime_path("ERSATZRS_NEXT_TEST_FFMPEG");
    let ffprobe = runtime_path("ERSATZRS_NEXT_TEST_FFPROBE");
    let root = tempfile::tempdir().unwrap();
    let clip = root.path().join("two-colors.mkv");
    generate_clip(&ffmpeg, &clip).await;
    for rotation in [0, 90, 180, 270] {
        let case = root.path().join(rotation.to_string());
        let playout = case.join("playout");
        let output = case.join("output");
        tokio::fs::create_dir_all(&playout).await.unwrap();
        let now = chrono::Utc::now();
        let start = now - chrono::Duration::seconds(2);
        let finish = now + chrono::Duration::seconds(40);
        let item = serde_json::from_value(serde_json::json!({
            "id": "synthetic-rotation", "start": start, "finish": finish,
            "source": {"source_type": "local", "path": clip, "probe_hint": {
                "video": [{"codec":"h264", "width":320, "height":180,
                    "pix_fmt":"yuv420p", "stream_index":0, "frame_rate":"25/1",
                    "sample_aspect_ratio":"1:1", "display_aspect_ratio":"16:9",
                    "rotation": rotation}],
                "audio": [{"codec":"aac", "stream_index":1, "channels":1}],
                "subtitle": [], "format_name":"matroska,webm", "duration_ms":60000
            }}
        }))
        .unwrap();
        let schedule = wire::Playout {
            version: wire::PLAYOUT_VERSION.into(),
            items: vec![item],
        };
        tokio::fs::write(
            playout.join(format!(
                "{}_{}.json",
                start.timestamp_millis(),
                finish.timestamp_millis()
            )),
            serde_json::to_vec(&schedule).unwrap(),
        )
        .await
        .unwrap();
        let mut config = super::super::tests::config();
        config.playout.folder = playout.to_str().unwrap().into();
        config.ffmpeg.ffmpeg_path = ffmpeg.to_str().unwrap().into();
        config.ffmpeg.ffprobe_path = ffprobe.to_str().unwrap().into();
        config.normalization.audio.sample_rate_hz = Some(48_000);
        let mut process = NextProcess::start(
            &binary,
            "1",
            &output,
            &config,
            Instant::now() + Duration::from_secs(30),
        )
        .await
        .unwrap();
        let observed = segment_pixels(&ffmpeg, &output).await;
        process.stop().await.unwrap();
        let (segment, pixels) = observed.expect("Next must publish a decodable rotation segment");
        assert_eq!(pixels.len(), 64 * 36 * 3);
        let pixel = |x: usize, y: usize| &pixels[(y * 64 + x) * 3..(y * 64 + x) * 3 + 3];
        let red = |p: &[u8]| p[0] > 150 && p[1] < 70 && p[2] < 70;
        let blue = |p: &[u8]| p[2] > 150 && p[0] < 70 && p[1] < 70;
        match rotation {
            0 => assert!(red(pixel(8, 18)) && blue(pixel(56, 18))),
            180 => assert!(blue(pixel(8, 18)) && red(pixel(56, 18))),
            90 => assert!(blue(pixel(32, 8)) && red(pixel(32, 28))),
            270 => assert!(red(pixel(32, 8)) && blue(pixel(32, 28))),
            _ => unreachable!(),
        }
        if matches!(rotation, 90 | 270) {
            assert!(pixel(8, 18).iter().all(|value| *value < 30));
            assert!(pixel(56, 18).iter().all(|value| *value < 30));
        }
        retained_nut_runtime_tests::assert_selected_audio(&ffmpeg, &segment).await;
    }
}

async fn segment_pixels(
    ffmpeg: &Path,
    output: &Path,
) -> Result<(PathBuf, Vec<u8>), tokio::time::error::Elapsed> {
    timeout(Duration::from_secs(30), async {
        loop {
            if let Ok(playlist) = tokio::fs::read_to_string(output.join("ffmpeg.m3u8")).await {
                let mut duration = 0.0;
                for line in playlist.lines() {
                    if let Some(value) = line.strip_prefix("#EXTINF:") {
                        duration = value.trim_end_matches(',').parse::<f64>().unwrap();
                    } else if !line.starts_with('#') && !line.is_empty() && duration > 1.0 {
                        let segment = output.join(line);
                        if segment.is_file() {
                            let mut decode = Command::new(ffmpeg);
                            decode
                                .args(["-v", "error", "-nostdin", "-i"])
                                .arg(&segment)
                                .args([
                                    "-ss",
                                    "0.5",
                                    "-map",
                                    "0:v:0",
                                    "-frames:v",
                                    "1",
                                    "-vf",
                                    "scale=64:36",
                                    "-pix_fmt",
                                    "rgb24",
                                    "-f",
                                    "rawvideo",
                                    "pipe:1",
                                ]);
                            return (segment, tool(decode).await);
                        }
                    }
                }
            }
            tokio::time::sleep(Duration::from_millis(50)).await;
        }
    })
    .await
}

async fn generate_clip(ffmpeg: &Path, clip: &Path) {
    let mut generate = Command::new(ffmpeg);
    generate
        .args([
            "-v",
            "error",
            "-nostdin",
            "-y",
            "-f",
            "lavfi",
            "-i",
            "color=c=red:s=320x180:r=25,drawbox=x=160:y=0:w=160:h=180:color=blue:t=fill",
            "-f",
            "lavfi",
            "-i",
            "sine=frequency=880:sample_rate=48000",
            "-t",
            "60",
            "-c:v",
            "libx264",
            "-preset",
            "ultrafast",
            "-threads",
            "2",
            "-pix_fmt",
            "yuv420p",
            "-c:a",
            "aac",
            "-ac",
            "1",
        ])
        .arg(clip);
    tool(generate).await;
}
