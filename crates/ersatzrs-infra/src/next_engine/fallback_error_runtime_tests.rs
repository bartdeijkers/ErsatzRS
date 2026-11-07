//! Official Next burns its emergency error card independently of subtitle mode.
use super::*;
use crate::next_engine::protocol as wire;

#[tokio::test]
#[ignore = "requires official Next and checksum-verified FFmpeg; synthetic fallback error video"]
async fn real_next_fallback_errors_burn_in_both_modes_and_honor_overlay_off() {
    for mode in [wire::SubtitleMode::Burn, wire::SubtitleMode::Convert] {
        for enabled in [true, false] {
            for failed_media in [false, true] {
                error_card_case(mode, enabled, failed_media, None).await;
            }
        }
    }
}

#[tokio::test]
#[ignore = "requires qualified Next/FFmpeg; decoded custom/default background and error-overlay policy"]
async fn real_next_error_backgrounds_preserve_default_custom_and_overlay_off() {
    for (custom, failed_media, enabled) in [
        (false, false, true),
        (true, true, true),
        (false, true, false),
        (true, false, false),
    ] {
        error_card_case(
            wire::SubtitleMode::Burn,
            enabled,
            failed_media,
            Some(custom),
        )
        .await;
    }
}

async fn error_card_case(
    mode: wire::SubtitleMode,
    enabled: bool,
    failed_media: bool,
    background: Option<bool>,
) {
    let binary = runtime_path("ERSATZRS_NEXT_TEST_BIN");
    let ffmpeg = runtime_path("ERSATZRS_NEXT_TEST_FFMPEG");
    let ffprobe = runtime_path("ERSATZRS_NEXT_TEST_FFPROBE");
    let root = tempfile::tempdir().unwrap();
    let playout = root.path().join("playout");
    let output = root.path().join("output");
    let overlays = root.path().join("overlays");
    tokio::fs::create_dir(&playout).await.unwrap();
    tokio::fs::create_dir(&overlays).await.unwrap();
    write_error_playout(root.path(), &playout, failed_media).await;
    let mut config = super::super::tests::config();
    config.playout.folder = playout.to_str().unwrap().into();
    config.ffmpeg.ffmpeg_path = ffmpeg.to_str().unwrap().into();
    config.ffmpeg.ffprobe_path = ffprobe.to_str().unwrap().into();
    config.normalization.subtitle.mode = mode;
    config.normalization.audio.sample_rate_hz = Some(48_000);
    let background_path = case_background(&ffmpeg, root.path(), background).await;
    if let Some(path) = &background_path {
        config.fallback.as_mut().unwrap().background_image =
            Some(path.to_str().unwrap().to_owned());
    }
    if !enabled {
        tokio::fs::write(
            overlays.join("default.json"),
            br#"{"version":"https://ersatztv.org/channel/version/0.1.0","fallback":{"show_error":false}}"#,
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
    let observation = observe_pixels(&ffmpeg, &output, false).await;
    let top = if background.is_some() {
        Some(observe_pixels(&ffmpeg, &output, true).await)
    } else {
        None
    };
    process.stop().await.unwrap();
    let pixels = observation.expect("complete fallback segment must be produced");
    assert_eq!(pixels.len(), 320 * 60 * 3);
    let expected = if let Some(path) = &background_path {
        let expected_top = background_pixels(&ffmpeg, path, true).await;
        let actual = top.unwrap().unwrap();
        assert_eq!(actual.len(), expected_top.len());
        let error = actual
            .iter()
            .zip(&expected_top)
            .map(|(a, b)| f64::from(a.abs_diff(*b)))
            .sum::<f64>()
            / f64::from(u32::try_from(actual.len()).unwrap());
        assert!(
            error < 8.0,
            "background must match independent image decode: mean error {error}"
        );
        Some(background_pixels(&ffmpeg, path, false).await)
    } else {
        None
    };
    let white = card_pixels(&pixels, expected.as_deref());
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

async fn write_error_playout(root: &Path, playout: &Path, failed_media: bool) {
    let now = chrono::Utc::now();
    let begin = now - chrono::Duration::seconds(5);
    let finish = now + chrono::Duration::minutes(2);
    let items = if failed_media {
        vec![serde_json::from_value(serde_json::json!({
            "id": "synthetic-failed-item", "start": begin, "finish": finish,
            "source": { "source_type": "local", "path": root.join("missing-fixture.mkv") }
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
}

async fn case_background(ffmpeg: &Path, root: &Path, background: Option<bool>) -> Option<PathBuf> {
    if let Some(custom) = background {
        let resources = root.join("resources");
        tokio::fs::create_dir(&resources).await.unwrap();
        tokio::fs::write(
            resources.join("_background.png"),
            include_bytes!("../../../../assets/_background.png"),
        )
        .await
        .unwrap();
        if custom {
            let mut image = Command::new(ffmpeg);
            image
                .args([
                    "-v",
                    "error",
                    "-nostdin",
                    "-f",
                    "lavfi",
                    "-i",
                    "color=c=red:s=320x180",
                    "-frames:v",
                    "1",
                    "-update",
                    "1",
                ])
                .arg(resources.join("background.png"));
            tool(image).await;
        }
        let selected = crate::next_engine::fallback::background_image_in(&resources).await;
        Some(selected)
    } else {
        None
    }
}

async fn background_pixels(ffmpeg: &Path, image: &Path, top: bool) -> Vec<u8> {
    let mut reference = Command::new(ffmpeg);
    reference
        .args(["-v", "error", "-nostdin", "-i"])
        .arg(image)
        .args([
            "-vf",
            if top {
                "scale=320:180:flags=bicubic,crop=320:60:0:0"
            } else {
                "scale=320:180:flags=bicubic,crop=320:60:0:120"
            },
            "-frames:v",
            "1",
            "-pix_fmt",
            "rgb24",
            "-f",
            "rawvideo",
            "pipe:1",
        ]);
    tool(reference).await
}

fn card_pixels(pixels: &[u8], background: Option<&[u8]>) -> usize {
    pixels
        .chunks_exact(3)
        .enumerate()
        .filter(|(index, pixel)| {
            pixel.iter().all(|value| *value > 180)
                && background.is_none_or(|reference| {
                    reference[index * 3..index * 3 + 3]
                        .iter()
                        .any(|value| *value < 150)
                })
        })
        .count()
}

async fn observe_pixels(
    ffmpeg: &Path,
    output: &Path,
    top: bool,
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
                                    if top {
                                        "crop=320:60:0:0"
                                    } else {
                                        "crop=320:60:0:120"
                                    },
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
