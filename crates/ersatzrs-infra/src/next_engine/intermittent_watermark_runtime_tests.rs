//! Real-engine wall-clock timing with deterministic joined virtual starts.
use super::*;
use ersatzrs_core::repositories::ChannelRepository;

async fn frame_red(ffmpeg: &Path, segment: &Path) -> u8 {
    let mut decode = Command::new(ffmpeg);
    decode
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
            "crop=2:2:282:18",
            "-pix_fmt",
            "rgb24",
            "-f",
            "rawvideo",
            "pipe:1",
        ]);
    let bytes = tool(decode).await;
    assert_eq!(bytes.len(), 12);
    bytes[0]
}

async fn first_segment(output: &Path) -> PathBuf {
    timeout(Duration::from_secs(10), async {
        loop {
            if let Ok(playlist) = tokio::fs::read_to_string(output.join("live.m3u8")).await {
                if let Some(segment) = playlist.lines().find(|line| {
                    !line.starts_with('#')
                        && Path::new(line)
                            .extension()
                            .is_some_and(|ext| ext.eq_ignore_ascii_case("ts"))
                }) {
                    let path = output.join(segment);
                    if path.is_file() {
                        break path;
                    }
                }
            }
            tokio::time::sleep(Duration::from_millis(20)).await;
        }
    })
    .await
    .expect("real Next must publish a segment")
}

#[tokio::test]
#[allow(clippy::too_many_lines)] // One synthetic projection shared by three joined-clock probes.
#[ignore = "requires built Next engine and FFmpeg runtime; uses disposable synthetic media"]
async fn real_next_intermittent_watermark_joins_wall_cycle_and_preserves_fade_audio_subtitles() {
    use crate::next_engine::{config, protocol, publication, watermark};
    let binary = runtime_path("ERSATZRS_NEXT_TEST_BIN");
    let ffmpeg = runtime_path("ERSATZRS_NEXT_TEST_FFMPEG");
    let ffprobe = runtime_path("ERSATZRS_NEXT_TEST_FFPROBE");
    let root = tempfile::tempdir().unwrap();
    let clip = generate_text_clip(&ffmpeg, root.path()).await;
    let image = root.path().join("periodic-red.png");
    let mut generate = Command::new(&ffmpeg);
    generate
        .args([
            "-v",
            "error",
            "-nostdin",
            "-y",
            "-f",
            "lavfi",
            "-i",
            "color=c=red:s=40x20",
            "-frames:v",
            "1",
            "-threads",
            "1",
        ])
        .arg(&image);
    tool(generate).await;
    let profile = PlaybackProfile {
        profile: FFmpegProfile {
            video_format: FFmpegProfileVideoFormat::H264,
            audio_format: FFmpegProfileAudioFormat::Aac,
            video_bitrate: 1000,
            video_buffer_size: 2000,
            audio_sample_rate: 48,
            audio_channels: 1,
            ..Default::default()
        },
        resolution: Resolution {
            width: 320,
            height: 180,
            ..Default::default()
        },
    };
    let (pool, _) = prepare_schedule(&clip, false, &profile.profile).await;
    configure_text_tracks(&pool, false, false, false).await;
    seed_watermark(&pool, &image, false, false).await;
    sqlx::query(
        r#"UPDATE "ChannelWatermark" SET "Mode"=2, "FrequencyMinutes"=1, "DurationSeconds"=10"#,
    )
    .execute(&pool)
    .await
    .unwrap();
    let channel = crate::repositories::make_repositories(pool.clone())
        .channel
        .get_with_profile(1)
        .await
        .unwrap();
    let minute = chrono::DateTime::parse_from_rfc3339("2026-09-23T12:00:00Z")
        .unwrap()
        .with_timezone(&chrono::Utc);
    let mut item = selection::tests::item();
    item.id = 1;
    item.playout_id = 1;
    item.start = minute - chrono::Duration::seconds(10);
    item.finish = minute + chrono::Duration::seconds(40);
    item.in_point = chrono::Duration::seconds(5);
    item.out_point = chrono::Duration::seconds(55);
    let mut projected = selection::project_item(
        &pool,
        &channel,
        &item,
        100,
        protocol::Source::Local {
            path: clip.to_str().unwrap().to_owned(),
            in_point_ms: None,
            out_point_ms: None,
        },
        &profile.resolution,
    )
    .await
    .unwrap();
    let selected =
        crate::playback::select_playback_watermarks(&pool, &channel, 1, false, item.start)
            .await
            .unwrap();
    projected.watermark = watermark::project(&selected).unwrap();
    let playout = root.path().join("playout");
    publication::publish_playout(
        &playout,
        &protocol::Playout {
            version: protocol::PLAYOUT_VERSION.to_owned(),
            items: vec![projected],
        },
    )
    .unwrap();
    let mut config = config::channel_config(
        &profile,
        ersatzrs_domain::NextEngineTextSubtitleMode::Burn,
        &playout,
        &ffmpeg,
        &ffprobe,
    )
    .unwrap();
    for (second, expected) in [(1, "visible"), (11, "fade"), (20, "hidden")] {
        config.playout.virtual_start = Some(minute + chrono::Duration::seconds(second));
        let output = root.path().join(format!("output-{second}"));
        let mut process = NextProcess::start(
            &binary,
            "1",
            &output,
            &config,
            Instant::now() + Duration::from_secs(30),
        )
        .await
        .unwrap();
        let segment = first_segment(&output).await;
        let red = frame_red(&ffmpeg, &segment).await;
        // Read pixels before stopping, then confirm process-tree cleanup even if
        // a later assertion reveals an official-engine timing regression.
        assert_text_video_and_audio(&ffmpeg, &segment, true).await;
        process.stop().await.unwrap();
        match expected {
            "visible" => assert!(red > 210, "wall-second 1.5 must be visible, red={red}"),
            "fade" => assert!(
                (65..=190).contains(&red),
                "default one-second fade at wall-second 11.5, red={red}"
            ),
            _ => assert!(red < 25, "wall-second 20.5 must be hidden, red={red}"),
        }
    }
}
