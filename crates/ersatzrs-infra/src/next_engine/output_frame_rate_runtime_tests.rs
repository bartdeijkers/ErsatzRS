//! Real host cadence, copy eligibility and selected tracks across rate changes.
//! Uses the parent's explicit published-runtime environment contract.

use super::*;
use ersatzrs_core::repositories::{ChannelRepository, FfmpegProfileRepository, PlayoutRepository};
use ersatzrs_domain::{ChannelPlayoutSource, PinnedStreamSelection, PlayoutItem};
use futures_util::FutureExt as _;

const SOURCE_SECONDS: [u32; 2] = [90, 60];
// Covers a permitted45s startup plus four4s readiness segments and a useful
// complete measurement window. Source media outlasts this real schedule slot.
const FIRST_PROGRAMME_SECONDS: i64 = 75;
const STARTUP_SECONDS: u64 = 45;
const CAPTURE_SECONDS: u64 = 120;

#[derive(Clone, Copy)]
enum Case {
    Integer,
    Disabled,
    RationalMirror,
}

impl Case {
    fn name(self) -> &'static str {
        match self {
            Self::Integer => "integer-copy",
            Self::Disabled => "disabled-copy",
            Self::RationalMirror => "rational-mirror-burn",
        }
    }

    fn rates(self) -> [&'static str; 2] {
        match self {
            Self::Integer | Self::Disabled => ["25/1", "30/1"],
            Self::RationalMirror => ["24000/1001", "30000/1001"],
        }
    }

    fn target(self) -> Option<&'static str> {
        match self {
            Self::Integer => Some("25/1"),
            Self::Disabled => None,
            Self::RationalMirror => Some("24000/1001"),
        }
    }
}

#[tokio::test]
#[ignore = "requires exact published Next and FFmpeg; actual integer mixed-rate copy/handoff"]
async fn real_next_output_frame_rate_integer_preserves_copy_dutch_tracks_and_handoff() {
    run_case(Case::Integer).await;
}

#[tokio::test]
#[ignore = "requires exact published Next and FFmpeg; actual disabled mixed-rate passthrough"]
async fn real_next_output_frame_rate_disabled_preserves_source_cadence_and_payloads() {
    run_case(Case::Disabled).await;
}

#[tokio::test]
#[ignore = "requires exact published Next and FFmpeg; requested rational mirror/Burn/handoff"]
async fn real_next_output_frame_rate_requested_mirror_normalizes_rational_burn_and_audio() {
    run_case(Case::RationalMirror).await;
}

// A fixed fixture command, not production subprocess infrastructure. Stop/reap
// failures retain the caller's fixture root; an assertion cannot skip cleanup.
async fn measured_tool(mut command: Command) -> Vec<u8> {
    const LIMIT: u64 = 4 * 1024 * 1024;
    command
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::null());
    let mut child = spawn_managed_addon_process(command).unwrap();
    let mut stdout = child.stdout().take().unwrap().take(LIMIT + 1);
    let result = timeout(Duration::from_secs(30), async {
        let mut bytes = Vec::new();
        stdout.read_to_end(&mut bytes).await?;
        if u64::try_from(bytes.len()).unwrap() > LIMIT {
            return Err(std::io::Error::other("fixture output exceeded bound"));
        }
        let status = child.wait().await?;
        Ok::<_, std::io::Error>((status, bytes))
    })
    .await;
    if let Ok(Ok((status, bytes))) = result {
        // wait() already confirmed that the managed tree is empty, including
        // a nonzero exit. Do not retry termination of a reaped process.
        assert!(status.success(), "fixture media command failed: {status}");
        return bytes;
    }
    let cleanup = timeout(Duration::from_secs(5), child.kill()).await;
    assert!(
        matches!(cleanup, Ok(Ok(()))),
        "fixture tool cleanup unconfirmed; retain artifacts"
    );
    panic!("bounded fixture media command failed or timed out");
}

async fn generate(ffmpeg: &Path, path: &Path, rate: &str, color: &str, seconds: u32) {
    let mut command = Command::new(ffmpeg);
    command
        .args(["-v", "error", "-nostdin", "-y", "-f", "lavfi", "-i"])
        .arg(format!("color=c={color}:s=320x180:r={rate}"))
        .args([
            "-f",
            "lavfi",
            "-i",
            "sine=frequency=440:sample_rate=48000",
            "-f",
            "lavfi",
            "-i",
            "sine=frequency=880:sample_rate=48000",
            "-f",
            "lavfi",
            "-i",
        ])
        .arg(format!("testsrc2=size=64x36:rate={rate}"))
        .args([
            "-filter_complex_threads",
            "1",
            "-filter_complex",
            "[0:v][3:v]overlay=0:0[v]",
            "-map",
            "[v]",
            "-map",
            "1:a",
            "-map",
            "2:a",
            "-c:v",
            "libx264",
            "-preset",
            "ultrafast",
            "-pix_fmt",
            "yuv420p",
            "-g",
            "24",
            "-threads",
            "2",
            "-c:a",
            "aac",
            "-b:a",
            "96k",
            "-metadata:s:a:0",
            "language=eng",
            "-metadata:s:a:1",
            "language=nld",
        ])
        .arg("-t")
        .arg(seconds.to_string())
        .arg(path);
    measured_tool(command).await;
    std::fs::write(
        path.with_extension("nl.srt"),
        format!(
            "1\n00:00:00,000 --> 00:{:02}:{:02},000\nNederlandse ondertiteling\n",
            seconds / 60,
            seconds % 60
        ),
    )
    .unwrap();
}

fn profile(case: Case) -> FFmpegProfile {
    FFmpegProfile {
        video_format: if matches!(case, Case::RationalMirror) {
            FFmpegProfileVideoFormat::H264
        } else {
            FFmpegProfileVideoFormat::Copy
        },
        audio_format: FFmpegProfileAudioFormat::Copy,
        normalize_framerate: !matches!(case, Case::Disabled),
        video_bitrate: 1000,
        video_buffer_size: 2000,
        audio_sample_rate: 48,
        audio_channels: 1,
        ..Default::default()
    }
}

async fn install_sources(pool: &crate::db::DbPool, clips: &[PathBuf; 2], rates: &[String; 2]) {
    for sql in [
        r#"DELETE FROM "MediaFile" WHERE "Id"=2"#,
        r#"DELETE FROM "MediaStream" WHERE "MediaStreamKind"=3"#,
        r#"DELETE FROM "Subtitle" WHERE "Id"<>2"#,
        r#"UPDATE "MediaStream" SET "Channels"=1 WHERE "MediaStreamKind"=2"#,
        r#"UPDATE "MediaVersion" SET "Duration"='00:01:30' WHERE "Id"=100"#,
        r#"UPDATE "MediaStream" SET "PixelFormat"='yuv420p' WHERE "MediaStreamKind"=1"#,
        r#"INSERT INTO "MediaItem" ("Id","LibraryPathId","State") VALUES (11,1,0)"#,
        r#"INSERT INTO "Movie" ("Id") VALUES (11)"#,
        r#"INSERT INTO "MovieMetadata" ("Id","MovieId","Title","MetadataKind","DateAdded","DateUpdated") VALUES (21,11,'Red source',0,'2026-10-07','2026-10-07')"#,
        r#"INSERT INTO "MediaVersion" ("Id","MovieId","Height","Width","VideoScanKind","Duration","DateAdded","DateUpdated") VALUES (101,11,180,320,0,'00:01:00','2026-10-07','2026-10-07')"#,
        r#"INSERT INTO "MediaStream" ("Index","Codec","MediaStreamKind","Language","Channels","Default","Forced","MediaVersionId","PixelFormat") SELECT "Index","Codec","MediaStreamKind","Language","Channels","Default","Forced",101,"PixelFormat" FROM "MediaStream" WHERE "MediaVersionId"=100"#,
        r#"INSERT INTO "Subtitle" ("Id","MovieMetadataId","SubtitleKind","Title","StreamIndex","Codec","Default","Forced","SDH","Language","IsExtracted","DateAdded","DateUpdated") VALUES (4,21,1,'Dutch',9,'subrip',1,0,0,'nld',0,'2026-10-07','2026-10-07')"#,
    ] {
        sqlx::query(sql).execute(pool).await.unwrap();
    }
    for (index, version) in [100_i64, 101].into_iter().enumerate() {
        sqlx::query(r#"UPDATE "MediaVersion" SET "RFrameRate"=? WHERE "Id"=?"#)
            .bind(&rates[index])
            .bind(version)
            .execute(pool)
            .await
            .unwrap();
        sqlx::query(r#"UPDATE "Subtitle" SET "Path"=? WHERE "Id"=?"#)
            .bind(clips[index].with_extension("nl.srt").to_str().unwrap())
            .bind(if index == 0 { 2 } else { 4 })
            .execute(pool)
            .await
            .unwrap();
    }
    sqlx::query(r#"INSERT INTO "MediaFile" ("Id","MediaVersionId","Path","PathHash") VALUES (3,101,?,'red-source')"#)
        .bind(clips[1].to_str().unwrap()).execute(pool).await.unwrap();
}

async fn install_schedule(
    pool: &crate::db::DbPool,
) -> (chrono::DateTime<chrono::Utc>, chrono::DateTime<chrono::Utc>) {
    let repos = crate::repositories::make_repositories(pool.clone());
    sqlx::query(r#"DELETE FROM "PlayoutItem""#)
        .execute(pool)
        .await
        .unwrap();
    let start = chrono::Utc::now() - chrono::Duration::seconds(1);
    let red_seconds = i64::from(SOURCE_SECONDS[1]);
    let finish = start + chrono::Duration::seconds(FIRST_PROGRAMME_SECONDS + red_seconds);
    for (id, media, version, offset, length) in [
        (1, 10, 100, 0, FIRST_PROGRAMME_SECONDS),
        (2, 11, 101, FIRST_PROGRAMME_SECONDS, red_seconds),
    ] {
        repos
            .playout
            .upsert_item(&PlayoutItem {
                id,
                playout_id: 1,
                media_item_id: media,
                start: start + chrono::Duration::seconds(offset),
                finish: start + chrono::Duration::seconds(offset + length),
                in_point: chrono::Duration::zero(),
                out_point: chrono::Duration::seconds(length),
                pinned_stream_selection: Some(PinnedStreamSelection {
                    media_version_id: version,
                    video_stream_index: 0,
                    audio_stream_index: Some(2),
                    subtitle_stream_index: Some(9),
                }),
                ..Default::default()
            })
            .await
            .unwrap();
    }
    (start, finish)
}

async fn requested_channel(pool: &crate::db::DbPool, case: Case) -> (i64, String) {
    let repos = crate::repositories::make_repositories(pool.clone());
    sqlx::query(r#"UPDATE "Channel" SET "NextEngineTextSubtitleMode"=? WHERE "Id"=1"#)
        .bind(i32::from(!matches!(case, Case::RationalMirror)))
        .execute(pool)
        .await
        .unwrap();
    if !matches!(case, Case::RationalMirror) {
        return (1, "1".into());
    }
    let mut source = repos.channel.get_with_profile(1).await.unwrap();
    let mut requested = profile(case);
    requested.id = 2;
    requested.resolution_id = 1;
    repos.ffmpeg_profile.upsert(&requested).await.unwrap();
    sqlx::query(r#"INSERT INTO "Resolution" ("Id","Name","Width","Height") VALUES (2,'Source only',640,360)"#)
        .execute(pool).await.unwrap();
    sqlx::query(
        r#"UPDATE "FFmpegProfile" SET "NormalizeFramerate"=0,"ResolutionId"=2 WHERE "Id"=1"#,
    )
    .execute(pool)
    .await
    .unwrap();
    source.id = 2;
    source.number = "2".into();
    source.ffmpeg_profile_id = 2;
    source.playout_source = ChannelPlayoutSource::Mirror;
    source.mirror_source_channel_id = Some(1);
    repos.channel.upsert(&source).await.unwrap();
    assert_eq!(
        sqlx::query_scalar::<_, i64>(r#"SELECT COUNT(*) FROM "Playout" WHERE "ChannelId"=2"#)
            .fetch_one(pool)
            .await
            .unwrap(),
        0
    );
    (2, "2".into())
}

#[derive(Debug, PartialEq)]
struct PersistedState {
    items: Vec<PlayoutItem>,
    channels: Vec<ersatzrs_domain::Channel>,
    profiles: Vec<PlaybackProfile>,
}

async fn persisted_state(
    pool: &crate::db::DbPool,
    requested: i64,
    start: chrono::DateTime<chrono::Utc>,
    finish: chrono::DateTime<chrono::Utc>,
) -> PersistedState {
    let repos = crate::repositories::make_repositories(pool.clone());
    let items = repos
        .playout
        .items_in_range(1, start, finish)
        .await
        .unwrap();
    let mut channels = Vec::new();
    let mut profiles = Vec::new();
    for id in 1..=requested {
        channels.push(repos.channel.get_with_profile(id).await.unwrap());
        profiles.push(
            crate::playback::load_channel_playback_profile(pool, id)
                .await
                .unwrap()
                .unwrap(),
        );
    }
    PersistedState {
        items,
        channels,
        profiles,
    }
}

async fn scanned_sources(
    case: Case,
    ffmpeg: &Path,
    ffprobe: &Path,
    clips: &[PathBuf; 2],
    root: &Path,
) -> [String; 2] {
    let expected = case.rates();
    let extractor = crate::ffprobe::FfprobeExtractor::new(ffprobe.to_str().unwrap());
    let mut rates = Vec::new();
    for (index, color) in ["blue", "red"].into_iter().enumerate() {
        generate(
            ffmpeg,
            &clips[index],
            expected[index],
            color,
            SOURCE_SECONDS[index],
        )
        .await;
        let metadata = extractor.extract(&clips[index]).await.unwrap();
        assert!(
            (metadata.version.duration
                - chrono::Duration::seconds(i64::from(SOURCE_SECONDS[index])))
            .num_milliseconds()
            .abs()
                < 50,
            "real source selected-content duration must cover its schedule slot"
        );
        let rate = metadata.version.r_frame_rate.unwrap();
        assert_eq!(rate, expected[index], "persist the actual scanned rate");
        assert!(metadata
            .streams
            .iter()
            .any(|s| s.language.as_deref() == Some("eng")));
        assert!(metadata
            .streams
            .iter()
            .any(|s| s.language.as_deref() == Some("nld")));
        rates.push(rate);
    }
    let rates: [String; 2] = rates.try_into().unwrap();
    std::fs::write(
        root.join("source-rates.json"),
        serde_json::to_vec_pretty(&rates).unwrap(),
    )
    .unwrap();
    rates
}

fn assert_configuration(
    case: Case,
    configuration: &crate::next_engine::protocol::ChannelConfig,
    root: &Path,
) {
    assert_eq!(
        configuration.version.as_deref(),
        Some("https://ersatztv.org/channel/version/0.1.2")
    );
    assert_eq!(
        configuration.normalization.video.frame_rate.as_deref(),
        case.target()
    );
    let wire = serde_json::to_value(configuration).unwrap();
    std::fs::write(
        root.join("host-channel.json"),
        serde_json::to_vec_pretty(&wire).unwrap(),
    )
    .unwrap();
    assert_eq!(configuration.normalization.video.width, 320);
    assert_eq!(configuration.normalization.video.height, 180);
    assert!(matches!(
        configuration.normalization.video.format,
        crate::next_engine::protocol::VideoFormat::H264
    ));
    assert!(matches!(
        configuration.normalization.audio.mode,
        crate::next_engine::protocol::StreamMode::Copy
    ));
    assert!(matches!(
        (case, configuration.normalization.video.mode),
        (
            Case::RationalMirror,
            crate::next_engine::protocol::StreamMode::Transcode
        ) | (
            Case::Integer | Case::Disabled,
            crate::next_engine::protocol::StreamMode::Copy
        )
    ));
}

async fn run_case(case: Case) {
    let binary = runtime_path("ERSATZRS_NEXT_TEST_BIN");
    let ffmpeg = runtime_path("ERSATZRS_NEXT_TEST_FFMPEG");
    let ffprobe = runtime_path("ERSATZRS_NEXT_TEST_FFPROBE");
    // Always retain until all assertions and confirmed owner cleanup succeed.
    let root = tempfile::tempdir().unwrap().keep();
    println!(
        "frame-rate synthetic {} artifacts: {}",
        case.name(),
        root.display()
    );
    let clips = [root.join("blue.mkv"), root.join("red.mkv")];
    let rates = scanned_sources(case, &ffmpeg, &ffprobe, &clips, &root).await;
    let (pool, locker) = prepare_schedule(&clips[0], false, &profile(case)).await;
    install_sources(&pool, &clips, &rates).await;
    let (channel_id, number) = requested_channel(&pool, case).await;
    let (start, finish) = install_schedule(&pool).await;
    let before = persisted_state(&pool, channel_id, start, finish).await;
    std::fs::write(
        root.join("persisted-selection.json"),
        serde_json::to_vec_pretty(&before.items).unwrap(),
    )
    .unwrap();
    let host = std::sync::Arc::new(host::NextHost::new(
        pool.clone(),
        locker,
        root.join("playout"),
    ));
    let request = host::NextStart {
        channel_id,
        preview: false,
        number: number.clone(),
        executable: binary,
        output: root.join("output"),
        ffmpeg: ffmpeg.clone(),
        ffprobe: ffprobe.clone(),
        replacements: Vec::new(),
        idle_timeout: None,
        deadline: Instant::now() + Duration::from_secs(STARTUP_SECONDS),
    };
    let registry = std::sync::Arc::new(crate::hls::InMemoryHlsSessionRegistry::new());
    let configuration = host.configuration(&request).await.unwrap();
    assert_configuration(case, &configuration, &root);
    let mut callback = text_subtitle_runtime_tests::callback_listener(host.clone()).await;
    let Some(task) = host.start(registry.clone(), request.clone()).await else {
        timeout(Duration::from_secs(5), callback.stop())
            .await
            .unwrap();
        panic!("new fixture session was not admitted");
    };
    let observation = std::panic::AssertUnwindSafe(async {
        collect_and_assert(case, &clips, &rates, &root, &request, &registry).await;
    })
    .catch_unwind()
    .await;
    registry.stop_channel(&number).await;
    let reaped = timeout(Duration::from_secs(10), task).await;
    let listener_stopped = timeout(Duration::from_secs(5), callback.stop()).await;
    assert!(
        matches!(reaped, Ok(Ok(()))),
        "Next owner reaping unconfirmed; fixture retained"
    );
    assert!(
        listener_stopped.is_ok(),
        "callback listener cleanup unconfirmed"
    );
    assert!(!request.output.exists(), "owned output survives Stop");
    assert!(
        registry.try_get(&number).await.is_none(),
        "owned registry survives Stop"
    );
    assert_eq!(
        crate::transcode_capacity::active_transcode_snapshot(1).active,
        0
    );
    if let Err(panic) = observation {
        std::panic::resume_unwind(panic);
    }
    assert_eq!(persisted_state(&pool, channel_id, start, finish).await, before,
        "observing output must preserve source schedule, pins and source/requested profile settings");
    pool.close().await;
    std::fs::write(
        root.join("cleanup.json"),
        br#"{"stop_reaped":true,"output_absent":true,"registry_absent":true,"capacity":0}"#,
    )
    .unwrap();
    export_proof(case, &root);
    std::fs::remove_dir_all(root).unwrap();
}

fn export_proof(case: Case, root: &Path) {
    #[allow(
        clippy::disallowed_methods,
        reason = "Explicit opt-in synthetic runtime proof export"
    )]
    let destination = std::env::var_os("ERSATZRS_ROW199_PROOF_DIR");
    if let Some(destination) = destination {
        let destination = PathBuf::from(destination).join(case.name());
        std::fs::create_dir_all(&destination).unwrap();
        for entry in std::fs::read_dir(root).unwrap() {
            let entry = entry.unwrap();
            if entry.file_type().unwrap().is_file() {
                std::fs::copy(entry.path(), destination.join(entry.file_name())).unwrap();
            }
        }
    }
}

async fn programme_color(ffmpeg: &Path, segment: &Path) -> Option<usize> {
    let mut command = Command::new(ffmpeg);
    command
        .args(["-v", "error", "-nostdin", "-i"])
        .arg(segment)
        .args([
            "-map",
            "0:v:0",
            "-vf",
            "format=rgb24,crop=1:1:160:90",
            "-f",
            "rawvideo",
            "pipe:1",
        ]);
    // Decode every frame of this finite segment. Reject short startup tails
    // and handoff fragments before selecting a cadence/copy measurement.
    let pixels = measured_tool(command).await;
    std::fs::write(segment.with_extension("colors.rgb"), &pixels).unwrap();
    assert_eq!(pixels.len() % 3, 0);
    if pixels.len() / 3 < 50 {
        return None;
    }
    let index = color_index(pixels[..3].try_into().unwrap())?;
    pixels
        .chunks_exact(3)
        .all(|pixel| color_index(pixel.try_into().unwrap()) == Some(index))
        .then_some(index)
}

fn color_index(pixel: [u8; 3]) -> Option<usize> {
    if pixel[2] > 180 && pixel[0] < 40 && pixel[1] < 40 {
        Some(0)
    } else if pixel[0] > 180 && pixel[1] < 40 && pixel[2] < 40 {
        Some(1)
    } else {
        None
    }
}

async fn collect_and_assert(
    case: Case,
    clips: &[PathBuf; 2],
    rates: &[String; 2],
    root: &Path,
    request: &host::NextStart,
    registry: &std::sync::Arc<crate::hls::InMemoryHlsSessionRegistry>,
) {
    let captured = capture_programmes(case, root, request, registry).await;
    for (index, segment) in captured.iter().enumerate() {
        let expected_rate = case.target().unwrap_or(&rates[index]);
        let source = matches!(case, Case::Disabled).then_some(clips[index].as_path());
        assert_cadence(&request.ffprobe, segment, expected_rate, source).await;
        assert_selected_audio(&request.ffmpeg, segment).await;
        assert_caption_pixels(
            &request.ffmpeg,
            segment,
            matches!(case, Case::RationalMirror),
        )
        .await;
        if !matches!(case, Case::RationalMirror) {
            if index == 0 || matches!(case, Case::Disabled) {
                assert_video_payload(&request.ffmpeg, &clips[index], segment, true).await;
            } else {
                assert_video_payload(&request.ffmpeg, &clips[index], segment, false).await;
            }
        }
    }
}

async fn capture_programmes(
    case: Case,
    root: &Path,
    request: &host::NextStart,
    registry: &std::sync::Arc<crate::hls::InMemoryHlsSessionRegistry>,
) -> [PathBuf; 2] {
    let handle = registry.try_get(&request.number).await.unwrap();
    // A deadline check must not drop a running decoder future: measured_tool
    // owns its timeout and confirms Stop/reap before returning or panicking.
    let capture_deadline = Instant::now() + Duration::from_secs(CAPTURE_SECONDS);
    let captured = {
        let mut seen = std::collections::HashSet::new();
        let mut saved = [None, None];
        loop {
            assert!(
                Instant::now() < capture_deadline,
                "both identified programme windows must be captured before deadline"
            );
            assert_ne!(
                handle.startup_readiness(),
                HlsStartupReadiness::Failed,
                "Next cadence startup failed"
            );
            handle.touch();
            if let Some(playlist) = handle.trim_playlist().await {
                for name in playlist.lines().filter(|line| {
                    !line.starts_with('#')
                        && Path::new(line)
                            .extension()
                            .is_some_and(|ext| ext.eq_ignore_ascii_case("ts"))
                }) {
                    if seen.contains(name) {
                        continue;
                    }
                    let original = request.output.join(name);
                    let staged = root.join(format!("observed-{}.ts", seen.len()));
                    if tokio::fs::copy(&original, &staged).await.is_err() {
                        continue;
                    }
                    let identified = programme_color(&request.ffmpeg, &staged).await;
                    seen.insert(name.to_owned());
                    let Some(index) = identified else {
                        continue;
                    };
                    if saved[index].is_some() {
                        continue;
                    }
                    let saved_segment = root.join(if index == 0 { "blue.ts" } else { "red.ts" });
                    tokio::fs::rename(
                        staged.with_extension("colors.rgb"),
                        saved_segment.with_extension("colors.rgb"),
                    )
                    .await
                    .unwrap();
                    tokio::fs::rename(&staged, &saved_segment).await.unwrap();
                    std::fs::write(
                        root.join(if index == 0 {
                            "blue-original.m3u8"
                        } else {
                            "red-original.m3u8"
                        }),
                        &playlist,
                    )
                    .unwrap();
                    if !matches!(case, Case::RationalMirror) {
                        let vtt = tokio::fs::read_to_string(original.with_extension("vtt"))
                            .await
                            .expect("selected Convert WebVTT segment");
                        std::fs::write(saved_segment.with_extension("vtt"), &vtt).unwrap();
                        assert_converted_vtt(&vtt, &saved_segment, &request.ffprobe).await;
                    }
                    saved[index] = Some(saved_segment);
                }
            }
            if saved.iter().all(Option::is_some) {
                break saved.map(Option::unwrap);
            }
            tokio::time::sleep(Duration::from_millis(100)).await;
        }
    };
    captured
}

async fn assert_converted_vtt(body: &str, segment: &Path, ffprobe: &Path) {
    assert!(body.starts_with("WEBVTT\n"));
    assert!(
        body.contains("Nederlandse ondertiteling"),
        "selected Dutch cue missing"
    );
    let clock = body
        .lines()
        .find_map(|line| line.strip_prefix("X-TIMESTAMP-MAP=LOCAL:00:00:00.000,MPEGTS:"))
        .expect("actual WebVTT timestamp map");
    let clock = clock.parse::<u64>().unwrap();
    assert!(clock < 8_589_934_592);
    let (start, finish) = body
        .lines()
        .find_map(|line| line.split_once(" --> "))
        .expect("timed Dutch cue");
    let seconds = |clock: &str| {
        let parts: Vec<f64> = clock.split(':').map(|v| v.parse().unwrap()).collect();
        assert_eq!(parts.len(), 3);
        parts[0] * 3600.0 + parts[1] * 60.0 + parts[2]
    };
    let start = seconds(start);
    let finish = seconds(finish.split_whitespace().next().unwrap());
    assert!(finish - start > 10.0, "retain the full programme cue");
    let report = cadence_report(ffprobe, segment, &segment.with_extension("vtt-clock.json")).await;
    let times = presentation_times(&report);
    let anchor = f64::from(u32::try_from(clock / 90_000).unwrap())
        + f64::from(u32::try_from(clock % 90_000).unwrap()) / 90_000.0;
    for observed in [times[0] + 0.5, times[0] + 1.5] {
        assert!(
            anchor + start <= observed && observed < anchor + finish,
            "mapped caption must cover decoded observation {observed}: {}..{}",
            anchor + start,
            anchor + finish
        );
    }
}

async fn cadence_report(ffprobe: &Path, media: &Path, evidence: &Path) -> serde_json::Value {
    let mut command = Command::new(ffprobe);
    command
        .args([
            "-v",
            "error",
            "-select_streams",
            "v:0",
            "-show_frames",
            "-show_streams",
            "-show_entries",
            "frame=best_effort_timestamp_time:stream=r_frame_rate,width,height,time_base",
            "-of",
            "json",
        ])
        .arg(media);
    let raw = measured_tool(command).await;
    std::fs::write(evidence, &raw).unwrap();
    serde_json::from_slice(&raw).unwrap()
}

fn presentation_times(report: &serde_json::Value) -> Vec<f64> {
    let mut timestamps: Vec<f64> = report["frames"]
        .as_array()
        .unwrap()
        .iter()
        .map(|frame| {
            frame["best_effort_timestamp_time"]
                .as_str()
                .unwrap()
                .parse()
                .unwrap()
        })
        .collect();
    timestamps.sort_by(f64::total_cmp);
    assert!(
        timestamps.len() >= 30,
        "need consecutive decoded presentation frames"
    );
    timestamps
}

fn clock_tick(report: &serde_json::Value) -> f64 {
    let (numerator, denominator) = report["streams"][0]["time_base"]
        .as_str()
        .unwrap()
        .split_once('/')
        .unwrap();
    let tick = numerator.parse::<f64>().unwrap() / denominator.parse::<f64>().unwrap();
    assert!(
        tick.is_finite() && tick > 0.0,
        "positive finite media clock"
    );
    tick
}

fn assert_quantized_cadence(timestamps: &[f64], step: f64, precision: f64) {
    assert!(precision.is_finite() && precision > 0.0 && precision < step);
    for pair in timestamps.windows(2) {
        assert!(
            ((pair[1] - pair[0]) - step).abs() <= precision,
            "passthrough frame step exceeds measured clock precision"
        );
    }
    for (index, timestamp) in timestamps.iter().enumerate() {
        let phase = timestamp - timestamps[0];
        let expected_phase = f64::from(u32::try_from(index).unwrap()) * step;
        assert!(
            (phase - expected_phase).abs() <= precision,
            "passthrough cumulative phase exceeds measured clock precision at frame {index}"
        );
    }
}

async fn assert_cadence(ffprobe: &Path, segment: &Path, expected: &str, source: Option<&Path>) {
    let report = cadence_report(ffprobe, segment, &segment.with_extension("cadence.json")).await;
    assert_eq!(report["streams"][0]["width"], 320);
    assert_eq!(report["streams"][0]["height"], 180);
    let timestamps = presentation_times(&report);
    let (n, d) = expected.split_once('/').unwrap_or((expected, "1"));
    let step = d.parse::<f64>().unwrap() / n.parse::<f64>().unwrap();
    let stream_rate = report["streams"][0]["r_frame_rate"].as_str().unwrap();
    let (stream_n, stream_d) = stream_rate.split_once('/').unwrap();
    assert!(
        (stream_d.parse::<f64>().unwrap() / stream_n.parse::<f64>().unwrap() - step).abs()
            < 0.000_001,
        "reported rate disagrees with decoded cadence"
    );
    if let Some(source) = source {
        let original = cadence_report(
            ffprobe,
            source,
            &segment.with_extension("source-cadence.json"),
        )
        .await;
        let source_tick = clock_tick(&original);
        let output_tick = clock_tick(&report);
        // Copy preserves the original timestamp lattice. Its clock tick and
        // the output remux tick bound quantization, not a different frame rate.
        assert_quantized_cadence(&presentation_times(&original), step, source_tick);
        assert_quantized_cadence(&timestamps, step, source_tick + output_tick);
        println!("passthrough clock evidence: source {source_tick:.9}s, output {output_tick:.9}s");
    } else {
        for pair in timestamps.windows(2) {
            assert!(
                ((pair[1] - pair[0]) - step).abs() < 0.00015,
                "decoded presentation cadence differs from {expected}: {}",
                pair[1] - pair[0]
            );
        }
    }
    let duration = timestamps.last().unwrap() - timestamps[0] + step;
    let frame_count = f64::from(u32::try_from(timestamps.len()).unwrap());
    assert!(
        (duration - frame_count * step).abs() < 0.001,
        "frame count/presentation duration mismatch"
    );
    println!(
        "frame-rate evidence {}: {expected}, {} decoded frames, {duration:.6}s",
        segment.file_name().unwrap().to_string_lossy(),
        timestamps.len()
    );
}

#[test]
fn output_frame_rate_passthrough_clock_rejects_cumulative_drift_and_missing_frames() {
    let step = 1.0 / 30.0;
    let source_tick = 1.0 / 1000.0;
    let precision = source_tick + 1.0 / 90_000.0;
    let quantized: Vec<f64> = (0..120)
        .map(|index| (f64::from(index) * step / source_tick).round() * source_tick)
        .collect();
    assert_quantized_cadence(&quantized, step, precision);

    // Every 29.5fps step fits a 1ms tolerance, but its accumulating phase does
    // not describe copied 30fps content. A per-pair-only oracle misses this.
    let drift: Vec<f64> = (0..120).map(|index| f64::from(index) / 29.5).collect();
    assert!(drift
        .windows(2)
        .all(|pair| ((pair[1] - pair[0]) - step).abs() < source_tick));
    assert!(
        std::panic::catch_unwind(|| assert_quantized_cadence(&drift, step, precision)).is_err()
    );

    let mut missing = quantized.clone();
    missing.remove(60);
    assert!(
        std::panic::catch_unwind(|| assert_quantized_cadence(&missing, step, precision)).is_err()
    );
    let mut duplicate = quantized;
    duplicate.insert(60, duplicate[60]);
    assert!(
        std::panic::catch_unwind(|| assert_quantized_cadence(&duplicate, step, precision)).is_err()
    );
}

async fn assert_selected_audio(ffmpeg: &Path, segment: &Path) {
    let mut command = Command::new(ffmpeg);
    command
        .args(["-v", "error", "-nostdin", "-i"])
        .arg(segment)
        .args([
            "-map", "0:a:0", "-t", "1", "-ac", "1", "-ar", "8000", "-f", "s16le", "pipe:1",
        ]);
    let pcm = measured_tool(command).await;
    std::fs::write(segment.with_extension("pcm"), &pcm).unwrap();
    let samples: Vec<i16> = pcm
        .chunks_exact(2)
        .map(|s| i16::from_le_bytes([s[0], s[1]]))
        .collect();
    assert!(samples.len() >= 7000);
    assert!(
        samples.iter().any(|value| value.unsigned_abs() > 100),
        "selected source audio became silence"
    );
    let crossings = samples.windows(2).filter(|p| p[0] <= 0 && p[1] > 0).count();
    let hz = f64::from(u32::try_from(crossings).unwrap()) * 8000.0
        / f64::from(u32::try_from(samples.len()).unwrap());
    assert!(
        (820.0..940.0).contains(&hz),
        "Dutch880 audio lost across handoff: {hz}"
    );
}

async fn assert_caption_pixels(ffmpeg: &Path, segment: &Path, burn: bool) {
    let mut command = Command::new(ffmpeg);
    command
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
    let pixels = measured_tool(command).await;
    assert_eq!(pixels.len(), 320 * 48 * 3);
    std::fs::write(segment.with_extension("rgb"), &pixels).unwrap();
    let white = pixels
        .chunks_exact(3)
        .filter(|p| p.iter().all(|v| *v > 180))
        .count();
    if burn {
        assert!(white > 20, "chosen Dutch caption missing from Burn video");
    } else {
        assert_eq!(white, 0, "Convert must preserve clean copy-eligible video");
    }
}

async fn assert_video_payload(ffmpeg: &Path, source: &Path, segment: &Path, copied: bool) {
    let original = elementary_video(ffmpeg, source).await;
    let encoded = elementary_video(ffmpeg, segment).await;
    let source_slices = copy_runtime_tests::video_slices(&original);
    let output_slices = copy_runtime_tests::video_slices(&encoded);
    assert!(output_slices.len() >= 20);
    let changed = output_slices
        .iter()
        .filter(|slice| !source_slices.contains(slice))
        .count();
    if copied {
        assert_eq!(
            changed, 0,
            "matching coded H.264 slices must be copied unchanged"
        );
    } else {
        assert!(
            changed * 100 > output_slices.len() * 80,
            "mismatched frame rate must use video transcode"
        );
    }
    std::fs::write(segment.with_extension("source.h264"), &original).unwrap();
    std::fs::write(segment.with_extension("payload.json"), serde_json::to_vec_pretty(
        &serde_json::json!({"expected_copy":copied,"output_slices":output_slices.len(),"changed_slices":changed})
    ).unwrap()).unwrap();
    std::fs::write(segment.with_extension("h264"), encoded).unwrap();
}

// Same AnnexB extraction contract as copy_runtime_tests::elementary, but this
// finite generated source needs its complete timeline for payload membership.
async fn elementary_video(ffmpeg: &Path, path: &Path) -> Vec<u8> {
    let mut command = Command::new(ffmpeg);
    command.args(["-v", "error", "-nostdin", "-i"]).arg(path);
    command.args([
        "-map",
        "0:v:0",
        "-c",
        "copy",
        "-f",
        "h264",
        "-bsf:v",
        "h264_mp4toannexb",
        "pipe:1",
    ]);
    measured_tool(command).await
}
