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

#[tokio::test]
#[ignore = "requires the real Next executable and FFmpeg 9; see module environment contract"]
async fn real_next_troubleshooting_finishes_with_reports_and_selected_tracks() {
    let binary = runtime_path("ERSATZRS_NEXT_TEST_BIN");
    let ffmpeg = runtime_path("ERSATZRS_NEXT_TEST_FFMPEG");
    let ffprobe = runtime_path("ERSATZRS_NEXT_TEST_FFPROBE");
    let root = tempfile::tempdir().unwrap();
    let clip = root.path().join("fixture.mkv");
    let subtitle = root.path().join("fixture.nl.srt");
    let playout = root.path().join("playout");
    let output = root.path().join("output");
    generate_clip(&ffmpeg, &clip).await;
    tokio::fs::write(
        &subtitle,
        "1\n00:00:05,000 --> 00:00:15,000\nNext subtitle proof\n",
    )
    .await
    .unwrap();
    tokio::fs::create_dir(&playout).await.unwrap();
    let start = chrono::DateTime::from_timestamp(chrono::Utc::now().timestamp(), 0).unwrap();
    let finish = start + chrono::Duration::seconds(10);
    let item = super::super::protocol::Playout {
        version: super::super::protocol::PLAYOUT_VERSION.to_owned(),
        items: vec![
            Box::pin(prepare_troubleshooting_fixture(
                &clip, &subtitle, &ffmpeg, start, finish,
            ))
            .await,
        ],
    };
    tokio::fs::write(
        playout.join(format!(
            "{}_{}.json",
            start.timestamp_millis(),
            finish.timestamp_millis()
        )),
        serde_json::to_vec(&item).unwrap(),
    )
    .await
    .unwrap();
    let mut config = super::tests::config();
    config.playout.folder = playout.to_str().unwrap().to_owned();
    config.playout.virtual_start = Some(start);
    config.ffmpeg.ffmpeg_path = ffmpeg.to_str().unwrap().to_owned();
    config.ffmpeg.ffprobe_path = ffprobe.to_str().unwrap().to_owned();
    config.ffmpeg.reports_folder = Some(output.to_str().unwrap().to_owned());
    config.normalization.audio.sample_rate_hz = Some(48_000);
    tokio::fs::write(
        root.path().join("config.json"),
        serde_json::to_vec(&config).unwrap(),
    )
    .await
    .unwrap();
    let mut process = NextProcess::start_troubleshooting(
        &binary,
        "1",
        &output,
        &config,
        None,
        Instant::now() + Duration::from_secs(30),
    )
    .await
    .unwrap();
    let status = timeout(Duration::from_mins(1), process.wait())
        .await
        .unwrap()
        .unwrap();
    if !status.success() {
        let retained = root.keep();
        panic!(
            "finite troubleshooting failed with {status}; synthetic fixture retained at {}",
            retained.display()
        );
    }
    let playlist = tokio::fs::read_to_string(output.join("ffmpeg.m3u8"))
        .await
        .unwrap();
    assert!(playlist.contains("#EXT-X-ENDLIST"));
    assert_audio_and_subtitle(&ffmpeg, &ffprobe, &output.join("ffmpeg.m3u8"), false, "aac").await;
    assert_troubleshooting_dossier(&output);
}

fn assert_troubleshooting_dossier(output: &Path) {
    let mut reports = 0;
    for entry in std::fs::read_dir(output).unwrap() {
        let entry = entry.unwrap();
        if entry.file_type().unwrap().is_dir() {
            for report in std::fs::read_dir(entry.path()).unwrap() {
                if report
                    .unwrap()
                    .file_name()
                    .to_string_lossy()
                    .contains("ffreport")
                {
                    reports += 1;
                }
            }
        }
    }
    assert!(
        reports > 0,
        "successful troubleshooting must retain its report dossier"
    );
}

async fn prepare_troubleshooting_fixture(
    clip: &Path,
    subtitle: &Path,
    ffmpeg: &Path,
    start: chrono::DateTime<chrono::Utc>,
    finish: chrono::DateTime<chrono::Utc>,
) -> super::super::protocol::PlayoutItem {
    use ersatzrs_core::repositories::ChannelRepository as _;
    let (pool, locker) = prepare_schedule(clip, false, &FFmpegProfile::default()).await;
    sqlx::query(r#"UPDATE "Subtitle" SET "Path"=? WHERE "Id"=2"#)
        .bind(subtitle.to_str().unwrap())
        .execute(&pool)
        .await
        .unwrap();
    let channel = crate::repositories::make_repositories(pool.clone())
        .channel
        .get_with_profile(1)
        .await
        .unwrap();
    let profile = crate::playback::load_playback_profile(&pool, 1)
        .await
        .unwrap()
        .unwrap();
    let media = schedule::bind_media(
        &pool,
        ersatzrs_domain::PlayoutItem {
            start,
            finish,
            in_point: chrono::Duration::seconds(5),
            out_point: chrono::Duration::seconds(15),
            ..selection::tests::item()
        },
    )
    .await
    .unwrap();
    let host = host::NextHost::new(pool, locker, clip.parent().unwrap().join("preview-host"));
    host.prepare_troubleshooting(super::super::troubleshooting::TroubleshootingPreview {
        media: &media,
        channel: &channel,
        profile: &profile,
        subtitle_id: Some(2),
        selection_time: None,
        watermark_ids: &[],
        graphics_ids: &[],
        replacements: &[],
        ffmpeg,
        deadline: Instant::now() + Duration::from_secs(30),
        diagnostics: None,
    })
    .await
    .unwrap()
}

async fn prepare_schedule(
    clip: &Path,
    muted: bool,
    profile: &FFmpegProfile,
) -> (crate::db::DbPool, ersatzrs_core::locking::EntityLocker) {
    let pool = selection::tests::fixture_pool().await;
    prepare_schedule_with_pool(pool, clip, muted, profile).await
}

async fn prepare_schedule_with_pool(
    pool: crate::db::DbPool,
    clip: &Path,
    muted: bool,
    profile: &FFmpegProfile,
) -> (crate::db::DbPool, ersatzrs_core::locking::EntityLocker) {
    use ersatzrs_core::repositories::{
        ChannelRepository, FfmpegProfileRepository, PlayoutRepository,
    };
    use ersatzrs_domain::{ChannelSubtitleMode, PinnedStreamSelection};
    let (pool, locker, mut channel, _) = schedule::tests::fixture_with_pool(pool).await;
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

async fn assert_audio_and_subtitle(
    ffmpeg: &Path,
    ffprobe: &Path,
    segment: &Path,
    muted: bool,
    codec: &str,
) {
    let mut probe = Command::new(ffprobe);
    probe
        .args(["-v", "error", "-show_streams", "-of", "json"])
        .arg(segment);
    let info: serde_json::Value = serde_json::from_slice(&tool(probe).await).unwrap();
    let streams = info["streams"].as_array().unwrap();
    assert!(streams.iter().any(|stream| stream["codec_name"] == "h264"));
    assert!(
        streams
            .iter()
            .any(|stream| stream["codec_name"] == codec && stream["sample_rate"] == "48000"),
        "unexpected synthetic output codecs: {:?}",
        streams
            .iter()
            .map(|s| (&s["codec_name"], &s["sample_rate"]))
            .collect::<Vec<_>>()
    );
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
async fn real_next_copy_profiles_preserve_audio_and_burn_subtitle_fallback() {
    let binary = runtime_path("ERSATZRS_NEXT_TEST_BIN");
    let ffmpeg = runtime_path("ERSATZRS_NEXT_TEST_FFMPEG");
    let ffprobe = runtime_path("ERSATZRS_NEXT_TEST_FFPROBE");
    let root = tempfile::tempdir().unwrap();
    let clip = root.path().join("fixture.mkv");
    generate_clip(&ffmpeg, &clip).await;
    tokio::fs::write(
        root.path().join("fixture.nl.srt"),
        "1\n00:00:00,000 --> 00:01:00,000\nNext copy fallback proof\n",
    )
    .await
    .unwrap();
    for (audio_copy, video_copy) in [(true, true), (true, false), (false, true)] {
        let profile = FFmpegProfile {
            video_format: if video_copy {
                FFmpegProfileVideoFormat::Copy
            } else {
                FFmpegProfileVideoFormat::H264
            },
            audio_format: if audio_copy {
                FFmpegProfileAudioFormat::Copy
            } else {
                FFmpegProfileAudioFormat::Aac
            },
            video_bitrate: 1000,
            video_buffer_size: 2000,
            audio_sample_rate: 48,
            audio_channels: 1,
            ..Default::default()
        };
        let (pool, locker) = prepare_schedule(&clip, false, &profile).await;
        let host = std::sync::Arc::new(host::NextHost::new(
            pool,
            locker,
            root.path().join("playout"),
        ));
        assert_next_session(
            &host,
            host::NextStart {
                channel_id: 1,
                preview: false,
                number: "1".into(),
                executable: binary.clone(),
                output: root.path().join("output"),
                ffmpeg: ffmpeg.clone(),
                ffprobe: ffprobe.clone(),
                replacements: Vec::new(),
                idle_timeout: None,
                deadline: Instant::now() + Duration::from_secs(30),
            },
            false,
        )
        .await;
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
            preview: false,
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
    assert_media_server_sessions(
        &binary,
        &ffmpeg,
        &ffprobe,
        &clip,
        &subtitle,
        &profile.profile,
        root.path(),
    )
    .await;
    let mut ac3_profile = profile.profile;
    ac3_profile.audio_format = FFmpegProfileAudioFormat::Ac3;
    let (pool, locker) = prepare_schedule(&clip, false, &ac3_profile).await;
    sqlx::query(r#"UPDATE "ConfigElement" SET "Value"='2' WHERE "Key"=?"#)
        .bind(ersatzrs_domain::CONFIG_KEY_FFMPEG_ACTIVE_TRANSCODE_LIMIT)
        .execute(&pool)
        .await
        .unwrap();
    let host = std::sync::Arc::new(host::NextHost::new(pool.clone(), locker, playout));
    assert_preview_sessions(
        &host,
        host::NextStart {
            channel_id: 1,
            preview: false,
            number: "1".into(),
            executable: binary,
            output,
            ffmpeg,
            ffprobe,
            replacements: Vec::new(),
            idle_timeout: None,
            deadline: Instant::now() + Duration::from_secs(30),
        },
    )
    .await;
    assert_eq!(
        crate::playback::load_channel_playback_profile(&pool, 1)
            .await
            .unwrap()
            .unwrap()
            .profile
            .audio_format,
        FFmpegProfileAudioFormat::Ac3
    );
}

#[allow(clippy::too_many_arguments)]
async fn assert_media_server_sessions(
    binary: &Path,
    ffmpeg: &Path,
    ffprobe: &Path,
    clip: &Path,
    subtitle: &Path,
    profile: &FFmpegProfile,
    root: &Path,
) {
    use crate::next_engine::source;
    use ersatzrs_core::repositories::RemoteMediaSourceKind;
    let server = fixture_media_server().await;
    let bytes = std::sync::Arc::new(tokio::fs::read(clip).await.unwrap());
    for kind in RemoteMediaSourceKind::ALL {
        eprintln!("checking Next {} HTTP playback", kind.provider_name());
        let (pool, locker) = prepare_schedule(clip, false, profile).await;
        sqlx::query(r#"DELETE FROM "LocalLibrary""#)
            .execute(&pool)
            .await
            .unwrap();
        source::tests::seed_provider(&pool, kind).await;
        // An inaccessible server path forces HTTP. All provider sidecars
        // retain provider identities and fetch authenticated text.
        sqlx::query(r#"UPDATE "MediaFile" SET "Path" = 'server-only/fixture.mkv' WHERE "Id" = 1"#)
            .execute(&pool)
            .await
            .unwrap();
        sqlx::query(r#"UPDATE "Subtitle" SET "Path" = ? WHERE "Id" = 2"#)
            .bind(if kind == RemoteMediaSourceKind::Plex {
                "/library/streams/123"
            } else {
                "fixture-source"
            })
            .execute(&pool)
            .await
            .unwrap();
        let (path, header, value) = match kind {
            RemoteMediaSourceKind::Plex => {
                ("/library/parts/11/file.mkv", "X-Plex-Token", "<token>")
            }
            RemoteMediaSourceKind::Jellyfin => (
                "/Videos/fixture-item/stream",
                "Authorization",
                "MediaBrowser Token=\"<api-key>\"",
            ),
            RemoteMediaSourceKind::Emby => {
                ("/Videos/fixture-item/stream", "X-Emby-Token", "<api-key>")
            }
        };
        let mock = server
            .mock_async(|when, then| {
                when.method(httpmock::Method::GET)
                    .path(path)
                    .header(header, value);
                let bytes = std::sync::Arc::clone(&bytes);
                then.respond_with(move |request| fixture_range_response(request, &bytes));
            })
            .await;
        let subtitle_mock = fixture_provider_subtitle(&server, kind, subtitle, header, value).await;
        let _wrong_plex_server = fixture_provider_connection(&pool, kind, &server).await;
        let host = std::sync::Arc::new(host::NextHost::new(
            pool.clone(),
            locker,
            root.join("remote-playout"),
        ));
        let _relay_server = text_subtitle_runtime_tests::callback_listener(host.clone()).await;
        assert_next_session(
            &host,
            host::NextStart {
                channel_id: 1,
                preview: false,
                number: "1".into(),
                executable: binary.to_owned(),
                output: root.join("remote-output"),
                ffmpeg: ffmpeg.to_owned(),
                ffprobe: ffprobe.to_owned(),
                replacements: Vec::new(),
                idle_timeout: None,
                deadline: Instant::now() + Duration::from_secs(30),
            },
            false,
        )
        .await;
        assert!(
            mock.calls_async().await > 0,
            "Next must fetch authenticated media"
        );
        if let Some(subtitle_mock) = subtitle_mock {
            assert!(
                subtitle_mock.calls_async().await > 0,
                "Next must fetch the authenticated provider subtitle"
            );
            subtitle_mock.delete_async().await;
        }
        mock.delete_async().await;
    }
}

async fn fixture_provider_subtitle<'a>(
    server: &'a httpmock::MockServer,
    kind: ersatzrs_core::repositories::RemoteMediaSourceKind,
    subtitle: &Path,
    header: &str,
    value: &str,
) -> Option<httpmock::Mock<'a>> {
    use ersatzrs_core::repositories::RemoteMediaSourceKind;
    let subtitle_path = match kind {
        RemoteMediaSourceKind::Plex => Some("/library/streams/123"),
        RemoteMediaSourceKind::Jellyfin => {
            Some("/Videos/fixture-item/fixture-item/Subtitles/9/9/Stream.srt")
        }
        RemoteMediaSourceKind::Emby => {
            Some("/Videos/fixture-item/fixture-source/Subtitles/9/Stream.srt")
        }
    };
    if let Some(path) = subtitle_path {
        let text = tokio::fs::read(subtitle).await.unwrap();
        Some(
            server
                .mock_async(|when, then| {
                    when.method(httpmock::Method::GET)
                        .path(path)
                        .header(header, value);
                    then.status(200).body(text);
                })
                .await,
        )
    } else {
        None
    }
}

async fn fixture_provider_connection(
    pool: &crate::db::DbPool,
    kind: ersatzrs_core::repositories::RemoteMediaSourceKind,
    server: &httpmock::MockServer,
) -> Option<httpmock::MockServer> {
    use ersatzrs_core::repositories::RemoteMediaSourceKind;
    if kind == RemoteMediaSourceKind::Plex {
        let wrong = httpmock::MockServer::start_async().await;
        wrong
            .mock_async(|when, then| {
                when.method(httpmock::Method::GET).path("/");
                then.status(200)
                    .body(r#"<MediaContainer machineIdentifier="wrong-server" />"#);
            })
            .await;
        sqlx::query(r#"UPDATE "PlexConnection" SET "Uri"=? WHERE "Id"=1"#)
            .bind(wrong.base_url())
            .execute(pool)
            .await
            .unwrap();
        sqlx::query(r#"INSERT INTO "PlexConnection" ("Id", "PlexMediaSourceId", "IsActive", "Uri") VALUES (2,1,0,?)"#)
            .bind(server.base_url()).execute(pool).await.unwrap();
        return Some(wrong);
    }
    let update = match kind {
        RemoteMediaSourceKind::Jellyfin => r#"UPDATE "JellyfinConnection" SET "Address"=?"#,
        RemoteMediaSourceKind::Emby => r#"UPDATE "EmbyConnection" SET "Address"=?"#,
        RemoteMediaSourceKind::Plex => unreachable!("handled above"),
    };
    sqlx::query(update)
        .bind(server.base_url())
        .execute(pool)
        .await
        .unwrap();
    sqlx::query(r#"UPDATE "ConfigElement" SET "Key"=? WHERE "Key" LIKE '%.api_key.%'"#)
        .bind(format!(
            "{}.api_key.{}",
            kind.provider_name().to_lowercase(),
            server.base_url()
        ))
        .execute(pool)
        .await
        .unwrap();
    None
}

async fn fixture_media_server() -> httpmock::MockServer {
    let server = httpmock::MockServer::start_async().await;
    server
        .mock_async(|when, then| {
            when.method(httpmock::Method::GET)
                .path("/")
                .header("X-Plex-Token", "<token>");
            then.status(200)
                .body(r#"<MediaContainer machineIdentifier="fixture-server" />"#);
        })
        .await;
    server
}

async fn fixture_media_relay(
    host: std::sync::Arc<host::NextHost>,
    pool: crate::db::DbPool,
) -> httpmock::MockServer {
    let relay_server = httpmock::MockServer::start_async().await;
    host.media.configure(*relay_server.address()).unwrap();
    let relay_host = host.clone();
    let runtime = tokio::runtime::Handle::current();
    relay_server
        .mock_async(|when, then| {
            when.method(httpmock::Method::GET)
                .path("/ffmpeg/next-media/10/100/1");
            then.respond_with(move |request| {
                use crate::next_engine::media::ACCESS_HEADER;
                let headers = request.headers();
                if !headers
                    .get(ACCESS_HEADER)
                    .and_then(|v| v.to_str().ok())
                    .is_some_and(|v| relay_host.media.authorized(v))
                {
                    return httpmock::HttpMockResponse::builder().status(401).build();
                }
                let range = headers.get("range").map(|v| v.to_str().unwrap().to_owned());
                let expected = headers
                    .get(crate::next_engine::media::SOURCE_BINDING_HEADER)
                    .map(|value| value.to_str().unwrap().to_owned());
                let host = relay_host.clone();
                let pool = pool.clone();
                let runtime = runtime.clone();
                // httpmock responders are synchronous. A separate fixture thread
                // drives the real async media-open without blocking its runtime.
                std::thread::spawn(move || {
                    runtime.block_on(async move {
                        let response = host
                            .media
                            .open_bound(&pool, (10, 100, 1), range.as_deref(), expected.as_deref())
                            .await
                            .unwrap();
                        let mut builder = httpmock::HttpMockResponse::builder()
                            .status(response.status().as_u16());
                        for name in ["content-type", "content-range", "accept-ranges"] {
                            if let Some(value) = response.headers().get(name) {
                                builder = builder.header(name, value.to_str().unwrap());
                            }
                        }
                        builder
                            .body(response.bytes().await.unwrap().to_vec())
                            .build()
                    })
                })
                .join()
                .unwrap()
            });
        })
        .await;
    relay_server
}

fn fixture_range_response(
    request: &httpmock::HttpMockRequest,
    bytes: &[u8],
) -> httpmock::HttpMockResponse {
    let response = httpmock::HttpMockResponse::builder()
        .header("Content-Type", "video/x-matroska")
        .header("Accept-Ranges", "bytes");
    let headers = request.headers();
    let Some(range) = headers.get("range") else {
        return response.status(200).body(bytes.to_vec()).build();
    };
    let range = range.to_str().unwrap().strip_prefix("bytes=").unwrap();
    let (start, end) = range.split_once('-').unwrap();
    let start: usize = start.parse().unwrap();
    let end = if end.is_empty() {
        bytes.len() - 1
    } else {
        end.parse::<usize>().unwrap().min(bytes.len() - 1)
    };
    if start > end {
        return response
            .status(416)
            .header("Content-Range", format!("bytes */{}", bytes.len()))
            .build();
    }
    response
        .status(206)
        .header(
            "Content-Range",
            format!("bytes {start}-{end}/{}", bytes.len()),
        )
        .body(bytes[start..=end].to_vec())
        .build()
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
        "aac",
    )
    .await;
    assert!(registry.stop_channel("1").await);
    task.await.unwrap();
    assert_eq!(active_transcode_snapshot(1).active, 0);
    assert!(!request.output.exists());
    assert!(registry.try_get("1").await.is_none());
    assert!(handle.trim_playlist().await.is_none());
}

#[tokio::test]
#[ignore = "requires qualified indexed Next and FFmpeg9; simultaneous normal/browser preview"]
async fn real_next_indexed_normal_and_browser_preview_preserve_tracks_and_owned_shutdown() {
    let root = tempfile::tempdir().unwrap();
    let ffmpeg = runtime_path("ERSATZRS_NEXT_TEST_FFMPEG");
    let clip = root.path().join("fixture.mkv");
    generate_clip(&ffmpeg, &clip).await;
    std::fs::write(
        root.path().join("fixture.nl.srt"),
        "1\n00:00:00,000 --> 00:01:00,000\nNext subtitle proof\n",
    )
    .unwrap();
    let profile = FFmpegProfile {
        video_format: FFmpegProfileVideoFormat::H264,
        audio_format: FFmpegProfileAudioFormat::Ac3,
        audio_sample_rate: 48,
        audio_channels: 1,
        ..Default::default()
    };
    let (pool, locker) = prepare_schedule(&clip, false, &profile).await;
    sqlx::query(r#"UPDATE "ConfigElement" SET "Value"='2' WHERE "Key"=?"#)
        .bind(ersatzrs_domain::CONFIG_KEY_FFMPEG_ACTIVE_TRANSCODE_LIMIT)
        .execute(&pool)
        .await
        .unwrap();
    let binary = runtime_path("ERSATZRS_NEXT_TEST_BIN");
    let permit = crate::next_engine::indexed_runtime::IndexedReaderPermit::qualify(&binary, None)
        .await
        .unwrap();
    let host = std::sync::Arc::new(host::NextHost::new_indexed(
        pool.clone(),
        locker,
        root.path().join("playout"),
        ersatzrs_core::programme::ProgrammeClock::named("UTC").unwrap(),
        permit,
    ));
    assert_preview_sessions(
        &host,
        host::NextStart {
            channel_id: 1,
            preview: false,
            number: "1".into(),
            executable: binary,
            output: root.path().join("output"),
            ffmpeg,
            ffprobe: runtime_path("ERSATZRS_NEXT_TEST_FFPROBE"),
            replacements: Vec::new(),
            idle_timeout: None,
            deadline: Instant::now() + Duration::from_secs(30),
        },
    )
    .await;
    assert_eq!(
        crate::playback::load_channel_playback_profile(&pool, 1)
            .await
            .unwrap()
            .unwrap()
            .profile
            .audio_format,
        FFmpegProfileAudioFormat::Ac3
    );
    pool.close().await;
}

async fn assert_preview_sessions(host: &std::sync::Arc<host::NextHost>, request: host::NextStart) {
    use crate::hls::InMemoryHlsSessionRegistry;
    use crate::transcode_capacity::active_transcode_snapshot;
    let previews = std::sync::Arc::new(InMemoryHlsSessionRegistry::new());
    let normal = std::sync::Arc::new(InMemoryHlsSessionRegistry::with_playout_updates_to(
        previews.clone(),
    ));
    let normal_task = host.start(normal.clone(), request.clone()).await.unwrap();
    let mut preview_request = request.clone();
    preview_request.preview = true;
    preview_request.output = request.output.with_file_name("preview-output");
    let preview_task = host
        .start(previews.clone(), preview_request.clone())
        .await
        .unwrap();
    for (registry, config, codec) in [
        (&normal, &request, "ac3"),
        (&previews, &preview_request, "aac"),
    ] {
        let handle = registry.try_get("1").await.unwrap();
        timeout(Duration::from_secs(30), async {
            loop {
                match handle.startup_readiness() {
                    HlsStartupReadiness::Ready => break,
                    HlsStartupReadiness::Failed => {
                        panic!("Next simultaneous session startup failed")
                    }
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
        assert_audio_and_subtitle(
            &config.ffmpeg,
            &config.ffprobe,
            &config.output.join(segment),
            false,
            codec,
        )
        .await;
    }
    assert_eq!(active_transcode_snapshot(2).active, 2);
    assert!(previews.stop_channel("1").await);
    preview_task.await.unwrap();
    assert_eq!(active_transcode_snapshot(2).active, 1);
    assert!(!preview_request.output.exists());
    assert!(normal
        .try_get("1")
        .await
        .unwrap()
        .trim_playlist()
        .await
        .is_some());
    assert!(normal.stop_channel("1").await);
    normal_task.await.unwrap();
    assert_eq!(active_transcode_snapshot(2).active, 0);
    assert!(!request.output.exists());
}

#[tokio::test]
#[ignore = "requires real Next and FFmpeg 9; uses an isolated child application-data directory"]
async fn real_next_javascript_selects_nondefault_audio() {
    let root = tempfile::tempdir().unwrap();
    let mut command = Command::new(std::env::current_exe().unwrap());
    command
        .args([
            "--exact",
            "next_engine::process::runtime_tests::fixture_child_javascript_audio",
            "--ignored",
            "--nocapture",
        ])
        .env(crate::file_system_layout::APP_DATA_DIR_ENV_VAR, root.path())
        .env(
            crate::file_system_layout::APP_STATE_DIR_ENV_VAR,
            root.path().join("state"),
        );
    // Check the harness actually selected the child; an incorrect name otherwise
    // silently succeeds with zero tests.
    let output = tool(command).await;
    let output = String::from_utf8(output).unwrap();
    assert!(
        output.contains("1 passed"),
        "child fixture did not run: {output}"
    );
}

#[tokio::test]
#[ignore = "child of real_next_javascript_selects_nondefault_audio; never run directly"]
async fn fixture_child_javascript_audio() {
    let binary = runtime_path("ERSATZRS_NEXT_TEST_BIN");
    let ffmpeg = runtime_path("ERSATZRS_NEXT_TEST_FFMPEG");
    let ffprobe = runtime_path("ERSATZRS_NEXT_TEST_FFPROBE");
    let data = runtime_path(crate::file_system_layout::APP_DATA_DIR_ENV_VAR);
    assert_eq!(crate::file_system_layout::app_data_folder(), data);
    let scripts = data.join("scripts/audio-stream-selector");
    tokio::fs::create_dir_all(&scripts).await.unwrap();
    tokio::fs::write(scripts.join("movie.js"), "function selectMovieAudioStreamIndex(number,name,title,guids,languages,audio) { return audio.find(stream => stream.language === 'nld').index; }").await.unwrap();
    selection::tests::assert_javascript_precedence().await;
    let root = tempfile::tempdir().unwrap();
    let clip = root.path().join("fixture.mkv");
    generate_clip(&ffmpeg, &clip).await;
    tokio::fs::write(
        root.path().join("fixture.nl.srt"),
        "1\n00:00:00,000 --> 00:01:00,000\nNext subtitle proof\n",
    )
    .await
    .unwrap();
    let profile = FFmpegProfile {
        video_format: FFmpegProfileVideoFormat::H264,
        audio_format: FFmpegProfileAudioFormat::Aac,
        video_bitrate: 1000,
        video_buffer_size: 2000,
        audio_sample_rate: 48,
        audio_channels: 1,
        ..Default::default()
    };
    let (pool, locker) = prepare_schedule(&clip, false, &profile).await;
    // Built-in selection would choose the default 440 Hz English stream. Only
    // JavaScript can cause the shared decoder assertion to observe 880 Hz Dutch.
    sqlx::query(r#"UPDATE "Channel" SET "PreferredAudioLanguageCode" = 'eng'"#)
        .execute(&pool)
        .await
        .unwrap();
    let host = std::sync::Arc::new(host::NextHost::new(
        pool,
        locker,
        root.path().join("playout"),
    ));
    assert_next_session(
        &host,
        host::NextStart {
            channel_id: 1,
            preview: false,
            number: "1".into(),
            executable: binary,
            output: root.path().join("output"),
            ffmpeg,
            ffprobe,
            replacements: Vec::new(),
            idle_timeout: None,
            deadline: Instant::now() + Duration::from_secs(30),
        },
        false,
    )
    .await;
}

#[path = "image_subtitle_runtime_tests.rs"]
mod image_subtitle_runtime_tests;

#[path = "late_pgs_runtime_tests.rs"]
mod late_pgs_runtime_tests;

#[path = "hardware_fallback_runtime_tests.rs"]
mod hardware_fallback_runtime_tests;

#[path = "jellyfin_audio_runtime_tests.rs"]
mod jellyfin_audio_runtime_tests;

#[path = "subtitle_nul_runtime_tests.rs"]
mod subtitle_nul_runtime_tests;

#[path = "../../../../tests/support/late_pgs.rs"]
mod late_pgs;

#[path = "text_subtitle_runtime_tests.rs"]
mod text_subtitle_runtime_tests;

#[path = "scaling_runtime_tests.rs"]
mod scaling_runtime_tests;

#[path = "reports_runtime_tests.rs"]
mod reports_runtime_tests;

#[path = "scheduled_image_tests.rs"]
mod scheduled_image_tests;

#[path = "song_runtime_tests.rs"]
mod song_runtime_tests;

#[path = "direct_remote_servers.rs"]
mod direct_remote_servers;

#[tokio::test]
#[ignore = "requires the real Next executable and FFmpeg 9; see module environment contract"]
async fn real_next_config_overlays_override_dimensions_and_preserve_selected_tracks() {
    let binary = runtime_path("ERSATZRS_NEXT_TEST_BIN");
    let ffmpeg = runtime_path("ERSATZRS_NEXT_TEST_FFMPEG");
    let ffprobe = runtime_path("ERSATZRS_NEXT_TEST_FFPROBE");
    let root = tempfile::tempdir().unwrap();
    let clip = root.path().join("fixture.mkv");
    generate_clip(&ffmpeg, &clip).await;
    std::fs::write(
        root.path().join("fixture.nl.srt"),
        "1\n00:00:00,000 --> 00:01:00,000\nNext subtitle proof\n",
    )
    .unwrap();
    let overlays = root.path().join("overlay files");
    std::fs::create_dir(&overlays).unwrap();
    std::fs::write(
        overlays.join("default.json"),
        br#"{"version":"https://ersatztv.org/channel/version/0.1.0","normalization":{"video":{"width":352}}}"#,
    )
    .unwrap();
    let profile = FFmpegProfile {
        video_format: FFmpegProfileVideoFormat::H264,
        audio_format: FFmpegProfileAudioFormat::Aac,
        video_bitrate: 1000,
        video_buffer_size: 2000,
        audio_sample_rate: 48,
        audio_channels: 1,
        ..Default::default()
    };
    let (pool, locker) = prepare_schedule(&clip, false, &profile).await;
    let host = std::sync::Arc::new(
        host::NextHost::new(pool, locker, root.path().join("playout"))
            .with_config_overlays(overlays.clone()),
    );
    let request = host::NextStart {
        channel_id: 1,
        preview: false,
        number: "1".into(),
        executable: binary.clone(),
        output: root.path().join("output"),
        ffmpeg: ffmpeg.clone(),
        ffprobe: ffprobe.clone(),
        replacements: Vec::new(),
        idle_timeout: None,
        deadline: Instant::now() + Duration::from_secs(30),
    };
    for expected_width in [352, 384] {
        if expected_width == 384 {
            std::fs::write(
                overlays.join("1.json"),
                br#"{"version":"https://ersatztv.org/channel/version/0.1.0","normalization":{"video":{"width":384}}}"#,
            )
            .unwrap();
        }
        let mut request = request.clone();
        request.deadline = Instant::now() + Duration::from_secs(30);
        assert_overlay_session(&host, request, expected_width).await;
    }

    // The engine owns parsing/merging; malformed JSON must fail before readiness
    // and the adapter must return only after its managed child has been reaped.
    let invalid_output = root.path().join("invalid-output");
    let config = super::super::config::channel_config(
        &PlaybackProfile {
            profile,
            resolution: Resolution {
                width: 320,
                height: 180,
                ..Default::default()
            },
        },
        ersatzrs_domain::NextEngineTextSubtitleMode::Burn,
        &root.path().join("playout/1"),
        &ffmpeg,
        &ffprobe,
    )
    .unwrap();
    let mut valid = NextProcess::start_with_overlays(
        &binary,
        "1",
        &invalid_output,
        &config,
        Some(&overlays),
        Instant::now() + Duration::from_secs(30),
    )
    .await
    .unwrap();
    valid.stop().await.unwrap();
    std::fs::write(overlays.join("1.json"), b"{ malformed overlay").unwrap();
    let result = NextProcess::start_with_overlays(
        &binary,
        "1",
        &invalid_output,
        &config,
        Some(&overlays),
        Instant::now() + Duration::from_secs(10),
    )
    .await;
    assert!(matches!(result, Err(ProcessError::ExitedBeforeReady(Some(code))) if code != 0));
    assert!(!invalid_output.join(".ready").exists());
}

async fn assert_overlay_session(
    host: &std::sync::Arc<host::NextHost>,
    request: host::NextStart,
    expected_width: usize,
) {
    let registry = std::sync::Arc::new(crate::hls::InMemoryHlsSessionRegistry::new());
    let task = host.start(registry.clone(), request.clone()).await.unwrap();
    let handle = registry.try_get("1").await.unwrap();
    timeout(Duration::from_secs(30), async {
        loop {
            match handle.startup_readiness() {
                HlsStartupReadiness::Ready => break,
                HlsStartupReadiness::Failed => panic!("Next overlay session startup failed"),
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
        .expect("Next must publish an MPEG-TS segment");
    let segment = request.output.join(segment);
    let mut probe = Command::new(&request.ffprobe);
    probe
        .args([
            "-v",
            "error",
            "-select_streams",
            "v:0",
            "-show_streams",
            "-of",
            "json",
        ])
        .arg(&segment);
    let info: serde_json::Value = serde_json::from_slice(&tool(probe).await).unwrap();
    assert_eq!(info["streams"][0]["width"], expected_width);
    assert_eq!(info["streams"][0]["height"], 180);
    let mut decode = Command::new(&request.ffmpeg);
    decode
        .args(["-v", "error", "-nostdin", "-i"])
        .arg(&segment)
        .args([
            "-map",
            "0:v:0",
            "-frames:v",
            "1",
            "-pix_fmt",
            "gray",
            "-f",
            "rawvideo",
            "pipe:1",
        ]);
    assert_eq!(
        tool(decode).await.len(),
        expected_width * 180,
        "decoded overlay dimensions"
    );
    // Height remains 180, so the existing crop still covers the subtitle's
    // bottom-center glyphs at either width. Reuse the exact audio/glyph gates.
    assert_audio_and_subtitle(&request.ffmpeg, &request.ffprobe, &segment, false, "aac").await;
    assert!(registry.stop_channel("1").await);
    task.await.unwrap();
    assert!(!request.output.exists());
    assert!(registry.try_get("1").await.is_none());
}

#[path = "retained_nut_runtime_tests.rs"]
mod retained_nut_runtime_tests;

#[path = "producer_prefix_runtime_tests.rs"]
mod producer_prefix_runtime_tests;

#[path = "fallback_error_runtime_tests.rs"]
mod fallback_error_runtime_tests;

#[path = "rotation_runtime_tests.rs"]
mod rotation_runtime_tests;

#[path = "copy_runtime_tests.rs"]
mod copy_runtime_tests;

#[path = "output_frame_rate_runtime_tests.rs"]
mod output_frame_rate_runtime_tests;

#[path = "playout_refresh_runtime_tests.rs"]
mod playout_refresh_runtime_tests;
