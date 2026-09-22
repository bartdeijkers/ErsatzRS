//! Opt-in real-engine proof on disposable, generated media. Required variables:
//! `ERSATZRS_NEXT_TEST_BIN`, `ERSATZRS_NEXT_TEST_FFMPEG`, `ERSATZRS_NEXT_TEST_FFPROBE`.

use super::*;
use ersatzrs_core::hls::{HlsAdmissionStatus, HlsSessionRegistry, HlsStartupReadiness};
use ersatzrs_domain::{
    FFmpegProfile, FFmpegProfileAudioFormat, FFmpegProfileVideoFormat, Resolution,
};
use tokio::io::AsyncReadExt;

use crate::next_engine::{host, schedule, selection};
use crate::playback::PlaybackProfile;

#[allow(clippy::disallowed_methods)] // Explicit opt-in test runtime locations, never production configuration.
fn runtime_path(name: &str) -> PathBuf {
    std::env::var_os(name)
        .map(PathBuf::from)
        .expect("set all documented Next runtime test variables")
}

async fn tool(mut command: Command) -> Vec<u8> {
    const LIMIT: u64 = 128 * 1024;
    command
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::null());
    let mut child = spawn_managed_addon_process(command).unwrap();
    let mut stdout = child.stdout().take().unwrap().take(LIMIT + 1);
    timeout(Duration::from_secs(30), async {
        let mut bytes = Vec::new();
        stdout.read_to_end(&mut bytes).await.unwrap();
        assert!(
            u64::try_from(bytes.len()).unwrap() <= LIMIT,
            "fixture tool exceeded output bound"
        );
        assert!(child.wait().await.unwrap().success(), "fixture tool failed");
        bytes
    })
    .await
    .expect("fixture tool timed out")
}

async fn generate_clip(ffmpeg: &Path, clip: &Path) {
    let mut command = Command::new(ffmpeg);
    command
        .args([
            "-hide_banner",
            "-v",
            "error",
            "-nostdin",
            "-y",
            "-f",
            "lavfi",
            "-i",
            "color=c=blue:s=320x180:r=25",
            "-f",
            "lavfi",
            "-i",
            "sine=frequency=440:sample_rate=48000",
            "-f",
            "lavfi",
            "-i",
            "sine=frequency=880:sample_rate=48000",
            "-map",
            "0:v",
            "-map",
            "1:a",
            "-map",
            "2:a",
            "-t",
            "60",
            "-c:v",
            "libx264",
            "-preset",
            "ultrafast",
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
        .arg(clip);
    tool(command).await;
}

async fn prepare_schedule(
    clip: &Path,
    muted: bool,
    profile: &FFmpegProfile,
) -> (crate::db::DbPool, ersatzrs_core::locking::EntityLocker) {
    use ersatzrs_core::repositories::{
        ChannelRepository, FfmpegProfileRepository, PlayoutRepository,
    };
    use ersatzrs_domain::{ChannelSubtitleMode, PinnedStreamSelection};
    let (pool, locker, mut channel, _) = schedule::tests::fixture().await;
    sqlx::query(r#"INSERT INTO "LocalLibrary" ("Id") VALUES (1)"#)
        .execute(&pool)
        .await
        .unwrap();
    sqlx::query(r#"INSERT INTO "ConfigElement" ("Key", "Value") VALUES (?, '1')"#)
        .bind(ersatzrs_domain::CONFIG_KEY_FFMPEG_ACTIVE_TRANSCODE_LIMIT)
        .execute(&pool)
        .await
        .unwrap();
    let mut stored_profile = profile.clone();
    stored_profile.id = 1;
    stored_profile.resolution_id = 1;
    crate::repositories::make_repositories(pool.clone())
        .ffmpeg_profile
        .upsert(&stored_profile)
        .await
        .unwrap();
    channel.streaming_engine = ersatzrs_domain::StreamingEngine::Next;

    sqlx::query(r#"UPDATE "MediaFile" SET "Path" = ? WHERE "Id" = 1"#)
        .bind(clip.to_str().unwrap())
        .execute(&pool)
        .await
        .unwrap();
    sqlx::query(r#"DELETE FROM "PlayoutItem""#)
        .execute(&pool)
        .await
        .unwrap();
    let mut item = selection::tests::item();
    item.id = 1;
    item.playout_id = 1;
    item.in_point = chrono::Duration::zero();
    item.out_point = chrono::Duration::seconds(60);
    if muted {
        item.pinned_stream_selection = Some(PinnedStreamSelection {
            media_version_id: 100,
            video_stream_index: 0,
            audio_stream_index: None,
            subtitle_stream_index: None,
        });
    }
    channel.preferred_audio_language_code = Some("nld".to_owned());
    channel.preferred_subtitle_language_code = Some("nld".to_owned());
    channel.subtitle_mode = ChannelSubtitleMode::Default;
    crate::repositories::make_repositories(pool.clone())
        .playout
        .upsert_item(&item)
        .await
        .unwrap();
    crate::repositories::make_repositories(pool.clone())
        .channel
        .upsert(&channel)
        .await
        .unwrap();
    (pool, locker)
}

async fn assert_audio_and_subtitle(ffmpeg: &Path, ffprobe: &Path, segment: &Path, muted: bool) {
    let mut probe = Command::new(ffprobe);
    probe
        .args(["-v", "error", "-show_streams", "-of", "json"])
        .arg(segment);
    let info: serde_json::Value = serde_json::from_slice(&tool(probe).await).unwrap();
    let streams = info["streams"].as_array().unwrap();
    assert!(streams.iter().any(|stream| stream["codec_name"] == "h264"));
    assert!(streams
        .iter()
        .any(|stream| stream["codec_name"] == "aac" && stream["sample_rate"] == "48000"));
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
    assert!(
        samples.len() >= 7000,
        "need approximately one second of decoded audio"
    );
    let crossings = samples
        .windows(2)
        .filter(|pair| pair[0] <= 0 && pair[1] > 0)
        .count();
    let frequency = f64::from(u32::try_from(crossings).unwrap()) * 8000.0
        / f64::from(u32::try_from(samples.len()).unwrap());
    if muted {
        assert!(
            samples.iter().all(|sample| sample.unsigned_abs() <= 10),
            "pinned silence must not restore a source audio track"
        );
    } else {
        assert!(
            (820.0..940.0).contains(&frequency),
            "expected selected 880 Hz alternate audio, found {frequency}"
        );
    }
    let mut video = Command::new(ffmpeg);
    video
        .args(["-v", "error", "-nostdin", "-i"])
        .arg(segment)
        .args([
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
        .filter(|p| p.iter().all(|c| *c > 180))
        .count();
    if muted {
        assert_eq!(
            white, 0,
            "explicit subtitles-off must leave the blue fixture free of glyphs"
        );
    } else {
        assert!(
            white > 20,
            "subtitle must produce white glyphs over the otherwise solid blue fixture"
        );
    }
}

#[tokio::test]
#[ignore = "requires the real Next executable and FFmpeg 9; see module environment contract"]
async fn real_next_hls_preserves_selected_audio_and_burns_external_subtitles() {
    let binary = runtime_path("ERSATZRS_NEXT_TEST_BIN");
    let ffmpeg = runtime_path("ERSATZRS_NEXT_TEST_FFMPEG");
    let ffprobe = runtime_path("ERSATZRS_NEXT_TEST_FFPROBE");
    let root = tempfile::tempdir().unwrap();
    let clip = root.path().join("fixture.mkv");
    let subtitle = root.path().join("fixture.nl.srt");
    let playout = root.path().join("playout");
    let output = root.path().join("output");
    generate_clip(&ffmpeg, &clip).await;
    std::fs::write(
        &subtitle,
        "1\n00:00:00,000 --> 00:01:00,000\nNext subtitle proof\n",
    )
    .unwrap();
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
    for muted in [false, true] {
        let (pool, locker) = prepare_schedule(&clip, muted, &profile.profile).await;
        let host = std::sync::Arc::new(host::NextHost::new(pool, locker, playout.clone()));
        let request = host::NextStart {
            channel_id: 1,
            number: "1".into(),
            executable: binary.clone(),
            output: output.clone(),
            ffmpeg: ffmpeg.clone(),
            ffprobe: ffprobe.clone(),
            replacements: Vec::new(),
            idle_timeout: None,
            deadline: Instant::now() + Duration::from_secs(30),
        };
        assert_next_session(&host, request, muted).await;
    }
}

async fn assert_next_session(
    host: &std::sync::Arc<host::NextHost>,
    request: host::NextStart,
    muted: bool,
) {
    use crate::transcode_capacity::{
        active_transcode_snapshot, try_reserve_active_transcode, ActiveTranscodeSlot,
    };
    let registry = std::sync::Arc::new(crate::hls::InMemoryHlsSessionRegistry::new());
    let blocker =
        try_reserve_active_transcode(1, ActiveTranscodeSlot::new_next_hls("other")).unwrap();
    let task = host.start(registry.clone(), request.clone()).await.unwrap();
    let handle = registry.try_get("1").await.unwrap();
    timeout(Duration::from_secs(30), async {
        while handle.admission_status() != HlsAdmissionStatus::AtCapacity {
            assert!(!task.is_finished(), "Next preflight failed");
            tokio::time::sleep(Duration::from_millis(10)).await;
        }
    })
    .await
    .unwrap();
    assert!(
        !request.output.exists(),
        "capacity must be acquired before process startup"
    );
    drop(blocker);
    timeout(Duration::from_secs(30), async {
        loop {
            match handle.startup_readiness() {
                HlsStartupReadiness::Ready => break,
                HlsStartupReadiness::Failed => panic!("Next session startup failed"),
                _ => tokio::time::sleep(Duration::from_millis(10)).await,
            }
        }
    })
    .await
    .unwrap();
    registry.try_get("1").await.unwrap().touch();
    let playlist = handle.trim_playlist().await.unwrap();
    assert!(playlist.starts_with("#EXTM3U"));
    let segment = playlist
        .lines()
        .find(|line| {
            !line.starts_with('#')
                && Path::new(line)
                    .extension()
                    .is_some_and(|extension| extension.eq_ignore_ascii_case("ts"))
        })
        .expect("Next must publish an MPEG-TS segment");
    assert_eq!(active_transcode_snapshot(1).active, 1);
    assert!(host
        .start(registry.clone(), request.clone())
        .await
        .is_none());
    assert_audio_and_subtitle(
        &request.ffmpeg,
        &request.ffprobe,
        &request.output.join(segment),
        muted,
    )
    .await;
    assert!(registry.stop_channel("1").await);
    task.await.unwrap();
    assert_eq!(active_transcode_snapshot(1).active, 0);
    assert!(!request.output.exists());
    assert!(registry.try_get("1").await.is_none());
    assert!(handle.trim_playlist().await.is_none());
}
