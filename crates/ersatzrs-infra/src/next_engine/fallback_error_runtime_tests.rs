//! Official Next burns its emergency error card independently of subtitle mode.
use super::*;
use crate::next_engine::protocol as wire;

#[tokio::test]
#[ignore = "requires official Next and checksum-verified FFmpeg; synthetic fallback error video"]
async fn real_next_fallback_errors_burn_in_both_modes_and_honor_overlay_off() {
    for mode in [wire::SubtitleMode::Burn, wire::SubtitleMode::Convert] {
        for enabled in [true, false] {
            for failed_media in [false, true] {
                error_card_case(mode, enabled, failed_media).await;
            }
        }
    }
}

async fn error_card_case(mode: wire::SubtitleMode, enabled: bool, failed_media: bool) {
    let binary = runtime_path("ERSATZRS_NEXT_TEST_BIN");
    let ffmpeg = runtime_path("ERSATZRS_NEXT_TEST_FFMPEG");
    let ffprobe = runtime_path("ERSATZRS_NEXT_TEST_FFPROBE");
    let root = tempfile::tempdir().unwrap();
    let playout = root.path().join("playout");
    let output = root.path().join("output");
    let overlays = root.path().join("overlays");
    tokio::fs::create_dir(&playout).await.unwrap();
    tokio::fs::create_dir(&overlays).await.unwrap();
    let now = chrono::Utc::now();
    let begin = now - chrono::Duration::seconds(5);
    let finish = now + chrono::Duration::minutes(2);
    let items = if failed_media {
        vec![serde_json::from_value(serde_json::json!({
            "id": "synthetic-failed-item", "start": begin, "finish": finish,
            "source": { "source_type": "local", "path": root.path().join("missing-fixture.mkv") }
        }))
        .unwrap()]
    } else {
        Vec::new()
    };
    tokio::fs::write(
        playout.join(format!(
            "{}_{}.json",
            begin.timestamp_millis(),
            finish.timestamp_millis()
        )),
        serde_json::to_vec(&wire::Playout {
            version: wire::PLAYOUT_VERSION.into(),
            items,
        })
        .unwrap(),
    )
    .await
    .unwrap();
    let mut config = super::super::tests::config();
    config.playout.folder = playout.to_str().unwrap().into();
    config.ffmpeg.ffmpeg_path = ffmpeg.to_str().unwrap().into();
    config.ffmpeg.ffprobe_path = ffprobe.to_str().unwrap().into();
    config.normalization.subtitle.mode = mode;
    config.normalization.audio.sample_rate_hz = Some(48_000);
    if !enabled {
        tokio::fs::write(
            overlays.join("default.json"),
            br#"{"fallback":{"show_error":false}}"#,
        )
        .await
        .unwrap();
    }
    let mut process = NextProcess::start_with_overlays(
        &binary,
        "1",
        &output,
        &config,
        Some(&overlays),
        Instant::now() + Duration::from_secs(30),
    )
    .await
    .unwrap();
    let observation = observe_pixels(&ffmpeg, &output).await;
    process.stop().await.unwrap();
    let pixels = observation.expect("complete fallback segment must be produced");
    assert_eq!(pixels.len(), 320 * 60 * 3);
    let white = pixels
        .chunks_exact(3)
        .filter(|pixel| pixel.iter().all(|value| *value > 180))
        .count();
    if enabled {
        assert!(
            white > 20,
            "fallback error must be burned, including Convert mode"
        );
        let card = tokio::fs::read_to_string(output.join("fallback.ass"))
            .await
            .unwrap();
        if failed_media {
            assert!(
                card.contains("playout item synthetic-failed-item failed to transcode"),
                "{card}"
            );
        } else {
            assert!(card.contains("no playout item is scheduled"));
        }
    } else {
        assert_eq!(
            white, 0,
            "explicit overlay off retains plain black fallback"
        );
        assert!(!output.join("fallback.ass").exists());
    }
}

async fn observe_pixels(
    ffmpeg: &Path,
    output: &Path,
) -> Result<Vec<u8>, tokio::time::error::Elapsed> {
    timeout(Duration::from_secs(30), async {
        loop {
            if let Ok(playlist) = tokio::fs::read_to_string(output.join("ffmpeg.m3u8")).await {
                let mut duration = 0.0;
                for line in playlist.lines() {
                    if let Some(value) = line.strip_prefix("#EXTINF:") {
                        duration = value.trim_end_matches(',').parse::<f64>().unwrap();
                    } else if !line.starts_with('#') && !line.trim().is_empty() && duration > 1.0 {
                        let segment = output.join(line);
                        if segment.exists() {
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
                                    "crop=320:60:0:120",
                                    "-pix_fmt",
                                    "rgb24",
                                    "-f",
                                    "rawvideo",
                                    "pipe:1",
                                ]);
                            return tool(decode).await;
                        }
                    }
                }
            }
            tokio::time::sleep(Duration::from_millis(50)).await;
        }
    })
    .await
}
