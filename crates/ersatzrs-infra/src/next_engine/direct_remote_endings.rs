//! Keep RTSP handoff and padded-audio EOF proof after retiring ADR 0116's patch.
use super::*;
use crate::next_engine::protocol as wire;

#[tokio::test]
#[ignore = "requires Next, FFmpeg 9 and pinned MediaMTX; decoded live handoff and padded EOF"]
async fn real_next_rtsp_handoff_preserves_tracks_and_stops_padded_audio_at_video_eof() {
    let ffmpeg = runtime_path("ERSATZRS_NEXT_TEST_FFMPEG");
    let root = tempfile::tempdir().unwrap();
    let live = root.path().join("live.mkv");
    generate_clip(&ffmpeg, &live).await;
    let finite = root.path().join("finite.mkv");
    let mut generate = Command::new(&ffmpeg);
    generate
        .args([
            "-v",
            "error",
            "-nostdin",
            "-f",
            "lavfi",
            "-i",
            "color=red:s=320x180:r=25:d=6",
            "-f",
            "lavfi",
            "-i",
            "sine=frequency=440:sample_rate=48000:duration=2",
            "-c:v",
            "libx264",
            "-preset",
            "ultrafast",
            "-threads",
            "2",
            "-g",
            "25",
            "-c:a",
            "aac",
        ])
        .arg(&finite);
    tool(generate).await;
    let rtsp = servers::Rtsp::start(&ffmpeg, &live, root.path()).await;
    let result = run(&ffmpeg, &rtsp.uri, &finite, root.path()).await;
    rtsp.stop().await;
    assert!(
        result.is_ok(),
        "RTSP handoff/EOF: {result:?}; retained at {}",
        root.keep().display()
    );
}

async fn run(ffmpeg: &Path, uri: &str, finite: &Path, root: &Path) -> Result<(), String> {
    let playout = root.join("playout");
    tokio::fs::create_dir(&playout).await.unwrap();
    let start = chrono::Utc::now();
    let handoff = start + chrono::Duration::seconds(12);
    let finish = handoff + chrono::Duration::seconds(6);
    let schedule = serde_json::json!({
        "version": wire::PLAYOUT_VERSION,
        "items": [
            {"id":"rtsp","start":start,"finish":handoff,
             "source":{"source_type":"rtsp","uri":uri},
             "tracks":{"audio":{"stream_index":2}}},
            {"id":"finite","start":handoff,"finish":finish,
             "source":{"source_type":"local","path":finite},
             "tracks":{"audio":{"stream_index":1}}}
        ]
    });
    tokio::fs::write(
        playout.join("00010101T000000Z_99991231T235959Z.json"),
        serde_json::to_vec(&schedule).unwrap(),
    )
    .await
    .unwrap();
    let mut config = crate::next_engine::config::channel_config(
        &PlaybackProfile {
            profile: text_profile(),
            resolution: Resolution {
                width: 320,
                height: 180,
                ..Default::default()
            },
        },
        ersatzrs_domain::NextEngineTextSubtitleMode::Burn,
        &playout,
        ffmpeg,
        &runtime_path("ERSATZRS_NEXT_TEST_FFPROBE"),
    )
    .unwrap();
    config.playout.virtual_start = Some(start);
    let output = root.join("output");
    let mut process = NextProcess::start_troubleshooting(
        &runtime_path("ERSATZRS_NEXT_TEST_BIN"),
        "1",
        &output,
        &config,
        None,
        Instant::now() + Duration::from_secs(30),
    )
    .await
    .map_err(|error| error.to_string())?;
    let status = timeout(Duration::from_mins(1), process.wait()).await;
    process.stop().await.map_err(|error| error.to_string())?;
    let status = status
        .map_err(|_| "padded audio did not terminate")?
        .map_err(|error| error.to_string())?;
    if !status.success() {
        return Err(format!("Next failed: {}", process.diagnostics()));
    }
    let playlist = output.join("live.m3u8");
    let body = tokio::fs::read_to_string(&playlist)
        .await
        .map_err(|error| error.to_string())?;
    check_playlist(&body);
    check_decoded_handoff(ffmpeg, &playlist).await;
    Ok(())
}

fn check_playlist(body: &str) {
    assert!(
        body.contains("#EXT-X-ENDLIST"),
        "finite playout must complete"
    );
    let duration: f64 = body
        .lines()
        .filter_map(|line| line.strip_prefix("#EXTINF:"))
        .map(|value| value.trim_end_matches(',').parse::<f64>().unwrap())
        .sum();
    assert!(
        (17.5..18.5).contains(&duration),
        "handoff or EOF changed duration: {duration}"
    );
    let mut previous = None;
    for timestamp in body
        .lines()
        .filter_map(|line| line.strip_prefix("#EXT-X-PROGRAM-DATE-TIME:"))
    {
        let current = chrono::DateTime::parse_from_str(timestamp, "%Y-%m-%dT%H:%M:%S%.f%z")
            .expect("Next programme timestamp");
        assert!(previous.is_none_or(|previous| current > previous));
        previous = Some(current);
    }
}

async fn check_decoded_handoff(ffmpeg: &Path, playlist: &Path) {
    for (second, red, audible) in [(1, false, true), (13, true, true), (17, true, false)] {
        let mut decode = Command::new(ffmpeg);
        decode
            .args(["-v", "error", "-nostdin", "-i"])
            .arg(playlist)
            .args([
                "-ss",
                &second.to_string(),
                "-map",
                "0:v:0",
                "-frames:v",
                "1",
                "-vf",
                "scale=1:1",
                "-pix_fmt",
                "rgb24",
                "-f",
                "rawvideo",
                "pipe:1",
            ]);
        let rgb = tool(decode).await;
        assert_eq!(rgb.len(), 3);
        assert!(if red {
            rgb[0] > 180 && rgb[2] < 70
        } else {
            rgb[2] > 180 && rgb[0] < 70
        });
        let mut decode = Command::new(ffmpeg);
        decode
            .args(["-v", "error", "-nostdin", "-i"])
            .arg(playlist)
            .args([
                "-ss",
                &second.to_string(),
                "-map",
                "0:a:0",
                "-t",
                "0.5",
                "-ac",
                "1",
                "-ar",
                "8000",
                "-f",
                "s16le",
                "pipe:1",
            ]);
        let bytes = tool(decode).await;
        let samples: Vec<i16> = bytes
            .chunks_exact(2)
            .map(|pair| i16::from_le_bytes([pair[0], pair[1]]))
            .collect();
        assert!(samples.len() >= 3000);
        if audible {
            let crossings = samples
                .windows(2)
                .filter(|pair| pair[0] <= 0 && pair[1] > 0)
                .count();
            #[allow(clippy::cast_precision_loss)] // Bounded half-second synthetic audio sample.
            let hz = crossings as f64 * 8000.0 / samples.len() as f64;
            let expected = if red { 440.0 } else { 880.0 };
            assert!((hz - expected).abs() < 60.0, "wrong handoff audio: {hz}");
        } else {
            assert!(
                samples.iter().all(|sample| sample.unsigned_abs() <= 1),
                "short source audio must be padded through video EOF"
            );
        }
    }
}
