use super::super::{
    build_stream_command, serve_channel_stream_admitted_inner, Channel, ChannelSubtitleMode,
    FFmpegProfileAudioFormat, FFmpegProfileVideoFormat, FfmpegCapabilityReport,
    FfmpegRuntimeReport, OutputMetadata, PlaybackProfile, StatusCode,
};
use super::*;
use crate::transport::iptv::teletext_binding::{TeletextOutputAdmission, TeletextOutputRole};
use chrono::Timelike as _;
use ersatzrs_core::repositories::ChannelRepository as _;
use ersatzrs_core::repositories::FfmpegProfileRepository as _;
use ersatzrs_domain::StreamingMode;
use ersatzrs_ffmpeg::OutputFormat;
use ersatzrs_infra::db::DbPool;
use sqlx::Executor as _;

#[path = "stream_external_http_tests.rs"]
mod http;

async fn fixture(root: &std::path::Path) -> (AppState, Channel) {
    use crate::host::state::{IptvAccessTokenVerifier, SystemStartup};
    use ersatzrs_infra::db::{DatabaseBackend, DatabaseSchemaStatus};
    let pool = ersatzrs_infra::db::init(&format!("sqlite:{}", root.join("fixture.db").display()))
        .await
        .unwrap();
    pool.execute(r#"INSERT OR IGNORE INTO "Resolution" ("Id", "Name", "Width", "Height") VALUES (1, 'Fixture', 320, 180)"#).await.unwrap();
    let repos = ersatzrs_infra::repositories::make_repositories(pool.clone());
    repos
        .ffmpeg_profile
        .upsert(&ersatzrs_domain::FFmpegProfile {
            id: 1,
            name: "Fixture".into(),
            resolution_id: 1,
            transcode_video: true,
            transcode_audio: true,
            video_format: FFmpegProfileVideoFormat::H264,
            audio_format: FFmpegProfileAudioFormat::Aac,
            video_preset: Some("ultrafast".into()),
            video_bitrate: 1000,
            video_buffer_size: 2000,
            audio_bitrate: 128,
            audio_buffer_size: 256,
            audio_channels: 2,
            audio_sample_rate: 48,
            ..Default::default()
        })
        .await
        .unwrap();
    let channel = Channel {
        id: 1,
        number: "1".into(),
        name: "Fixture".into(),
        ffmpeg_profile_id: 1,
        is_enabled: true,
        streaming_mode: StreamingMode::TransportStream,
        subtitle_mode: ChannelSubtitleMode::Any,
        preferred_subtitle_language_code: Some("eng".into()),
        ..Default::default()
    };
    repos.channel.upsert(&channel).await.unwrap();
    pool.execute(r#"INSERT INTO "Playout" ("Id", "ChannelId", "ScheduleKind", "ScheduleFile", "Seed") VALUES (1, 1, 20, '', 0);
        INSERT INTO "MediaSource" ("Id") VALUES (10);
        INSERT INTO "PlexMediaSource" ("Id", "ServerName", "ClientIdentifier") VALUES (10, 'Fixture', 'fixture');
        INSERT INTO "Library" ("Id", "MediaSourceId", "MediaKind") VALUES (10, 10, 1);
        INSERT INTO "PlexLibrary" ("Id", "ShouldSyncItems") VALUES (10, 1);
        INSERT INTO "LibraryPath" ("Id", "LibraryId") VALUES (10, 10);
        INSERT OR REPLACE INTO "ConfigElement" ("Key", "Value") VALUES ('ffmpeg.use_embedded_subtitles', 'true'), ('ffmpeg.extract_embedded_subtitles', 'true');"#).await.unwrap();
    let (tx, _rx) = tokio::sync::mpsc::unbounded_channel();
    let state = AppState::new(
        pool,
        DatabaseBackend::Sqlite,
        SystemStartup {
            database_ready: true,
            schema_status: DatabaseSchemaStatus::default(),
        },
        FfmpegRuntimeReport::for_tests(),
        uuid::Uuid::nil(),
        1,
        IptvAccessTokenVerifier::disabled(),
        tx,
        ersatzrs_core::locking::EntityLocker::default(),
    );
    (state, channel)
}

async fn schedule(
    pool: &DbPool,
    root: &std::path::Path,
    start: DateTime<Utc>,
    programs: serde_json::Value,
) {
    let path = root.join("schedule.json");
    std::fs::write(
        &path,
        serde_json::to_vec(&serde_json::json!({
            "startTime": start.to_rfc3339(), "programs": programs,
        }))
        .unwrap(),
    )
    .unwrap();
    sqlx::query(r#"UPDATE "Playout" SET "ScheduleFile" = ? WHERE "Id" = 1"#)
        .bind(path.to_str().unwrap())
        .execute(pool)
        .await
        .unwrap();
}

#[tokio::test]
async fn external_native_loader_refuses_exhausted_or_invalid_schedule() {
    let root = tempfile::tempdir().unwrap();
    let (state, _) = fixture(root.path()).await;
    let now = Utc::now().with_nanosecond(0).unwrap();
    schedule(&state.pool, root.path(), now, serde_json::json!([])).await;
    assert!(load(
        &state,
        1,
        now,
        tokio::time::Instant::now() + Duration::from_secs(5)
    )
    .await
    .unwrap()
    .is_none());
    std::fs::write(
        root.path().join("schedule.json"),
        br#"{"startTime":"invalid", "programs":[]}"#,
    )
    .unwrap();
    assert!(load(
        &state,
        1,
        now,
        tokio::time::Instant::now() + Duration::from_secs(5)
    )
    .await
    .is_err());
}

#[tokio::test]
async fn external_native_missing_plex_preconditions_return_unavailable() {
    let root = tempfile::tempdir().unwrap();
    let (state, channel) = fixture(root.path()).await;
    for server in ["Unknown", "Fixture", "Fixture"] {
        schedule(&state.pool, root.path(), Utc::now(), serde_json::json!([{
            "serverKey": server, "duration": 60000, "plexFile": "/library/parts/1/file.mkv", "ratingKey": "1"
        }])).await;
        let response = Box::pin(serve_channel_stream_admitted_inner(
            &state,
            channel.clone(),
            OutputFormat::MpegTs,
            StreamingMode::TransportStream,
            tokio::time::Instant::now() + Duration::from_secs(5),
            None,
            TeletextOutputAdmission::plain(TeletextOutputRole::FinalDirectMpegTs),
        ))
        .await;
        assert_eq!(response.status(), StatusCode::SERVICE_UNAVAILABLE);
        if server == "Fixture" {
            sqlx::query(r#"INSERT OR IGNORE INTO "PlexConnection" ("Id", "PlexMediaSourceId", "IsActive", "Uri") VALUES (1, 10, 1, 'http://fixture.invalid')"#)
                .execute(&state.pool).await.unwrap();
        }
    }
}

#[tokio::test]
#[ignore = "requires verified FFmpeg/ffprobe and isolated application data directories"]
async fn external_native_stream_plays_without_library_rows() {
    let root = tempfile::tempdir().unwrap();
    let (state, channel) = fixture(root.path()).await;
    let clip = generate_clip(root.path()).await;
    let now = Utc::now().with_nanosecond(0).unwrap();
    schedule(&state.pool, root.path(), now, serde_json::json!([{
        "title": "Fixture", "serverKey": "Fixture", "file": clip.to_str().unwrap(), "duration": 8000
    }])).await;
    let position = now + chrono::Duration::seconds(1);
    let active = load(
        &state,
        1,
        position,
        tokio::time::Instant::now() + Duration::from_secs(10),
    )
    .await
    .unwrap()
    .unwrap();
    assert_eq!(
        active.content_seek_position(position),
        Duration::from_secs(1)
    );
    assert_eq!(active.program_duration(), Some(Duration::from_secs(8)));
    assert_eq!(
        active.remaining_duration(position),
        Some(Duration::from_secs(7))
    );
    let response = Box::pin(serve_channel_stream_admitted_inner(
        &state,
        channel,
        OutputFormat::MpegTs,
        StreamingMode::TransportStream,
        tokio::time::Instant::now() + Duration::from_secs(20),
        None,
        TeletextOutputAdmission::plain(TeletextOutputRole::FinalDirectMpegTs),
    ))
    .await;
    assert_eq!(response.status(), StatusCode::OK);
    let bytes = tokio::time::timeout(
        Duration::from_secs(20),
        axum::body::to_bytes(response.into_body(), 16 * 1024 * 1024),
    )
    .await
    .unwrap()
    .unwrap();
    assert!(
        bytes.len() >= 188 * 10,
        "native playback produced no transport stream"
    );
    let capture = root.path().join("capture.ts");
    std::fs::write(&capture, &bytes).unwrap();
    let white = assert_capture(&capture).await;
    if white <= 4 {
        let artifact = root.keep();
        panic!(
            "subtitle pixels missing ({white}); synthetic capture retained at {}",
            artifact.display()
        );
    }
    let counts: (i64, i64, i64) = sqlx::query_as(
        r#"SELECT
        (SELECT COUNT(*) FROM "MediaItem"),
        (SELECT COUNT(*) FROM "MediaVersion"),
        (SELECT COUNT(*) FROM "PlayoutItem")"#,
    )
    .fetch_one(&state.pool)
    .await
    .unwrap();
    assert_eq!(
        counts,
        (0, 0, 0),
        "external playback must not create library/timeline rows"
    );
}

#[test]
fn external_native_bitmap_sidecar_argv() {
    let selected = ersatzrs_infra::playback::transient::TransientPlaybackTracks::from_streams(&[
        ersatzrs_domain::MediaStream {
            index: 927,
            kind: ersatzrs_domain::MediaStreamKind::ExternalSubtitle,
            codec: Some("hdmv_pgs_subtitle".into()),
            file_name: Some("fixture/selected.sup".into()),
            ..Default::default()
        },
    ])
    .subtitles
    .remove(0);
    let input = subtitle_input(
        &selected,
        "fixture/video.mkv",
        OutputFormat::MpegTs,
        StreamingMode::TransportStream,
    )
    .unwrap();
    let profile = PlaybackProfile {
        profile: ersatzrs_domain::FFmpegProfile {
            video_format: FFmpegProfileVideoFormat::H264,
            audio_format: FFmpegProfileAudioFormat::Aac,
            ..Default::default()
        },
        resolution: ersatzrs_domain::Resolution {
            width: 640,
            height: 360,
            ..Default::default()
        },
    };
    let command = build_stream_command(
        "fixture/video.mkv",
        OutputFormat::MpegTs,
        Duration::ZERO,
        &profile,
        None,
        StreamingMode::TransportStream,
        OutputMetadata::default(),
        FfmpegCapabilityReport::default(),
        None,
        Vec::new(),
        None,
        Some(input),
        Vec::new(),
        Vec::new(),
    )
    .unwrap();
    insta::assert_snapshot!(command.arguments.join("\n"));
}

async fn assert_capture(capture: &std::path::Path) -> usize {
    let decoded = tokio::process::Command::new(ersatzrs_ffmpeg::ffmpeg_path())
        .args(["-nostdin", "-v", "error", "-i"])
        .arg(capture)
        .args(["-map", "0:v:0", "-map", "0:a:0", "-f", "null", "-"])
        .kill_on_drop(true)
        .output()
        .await
        .unwrap();
    assert!(
        decoded.status.success(),
        "native capture failed audio/video decode"
    );
    let frame = tokio::process::Command::new(ersatzrs_ffmpeg::ffmpeg_path())
        .args(["-nostdin", "-v", "error", "-i"])
        .arg(capture)
        .args([
            "-map",
            "0:v:0",
            "-frames:v",
            "1",
            "-vf",
            "scale=320:180",
            "-pix_fmt",
            "rgb24",
            "-f",
            "rawvideo",
            "pipe:1",
        ])
        .kill_on_drop(true)
        .output()
        .await
        .unwrap();
    assert!(frame.status.success());
    assert_eq!(frame.stdout.len(), 320 * 180 * 3);
    frame
        .stdout
        .chunks_exact(3)
        .filter(|pixel| pixel.iter().all(|value| *value > 170))
        .count()
}

async fn generate_clip(root: &std::path::Path) -> std::path::PathBuf {
    let subtitle = root.join("subtitle.srt");
    std::fs::write(
        &subtitle,
        "1\n00:00:00,000 --> 00:00:08,000\nEXTERNAL FIXTURE\n",
    )
    .unwrap();
    let clip = root.join("clip.mkv");
    let output = tokio::process::Command::new(ersatzrs_ffmpeg::ffmpeg_path())
        .args([
            "-nostdin",
            "-v",
            "error",
            "-f",
            "lavfi",
            "-i",
            "color=blue:size=320x180:rate=25",
            "-f",
            "lavfi",
            "-i",
            "sine=frequency=880:sample_rate=48000",
            "-i",
        ])
        .arg(subtitle)
        .args([
            "-map",
            "0:v",
            "-map",
            "1:a",
            "-map",
            "2:s",
            "-c:v",
            "ffv1",
            "-c:a",
            "pcm_s16le",
            "-c:s",
            "srt",
            "-metadata:s:a:0",
            "language=nld",
            "-metadata:s:s:0",
            "language=eng",
            "-t",
            "8",
        ])
        .arg(&clip)
        .kill_on_drop(true)
        .output()
        .await
        .unwrap();
    assert!(output.status.success(), "fixture generation failed");
    clip
}
