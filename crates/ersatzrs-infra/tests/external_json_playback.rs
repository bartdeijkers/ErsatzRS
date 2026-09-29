mod common;

use chrono::Duration;
use ersatzrs_core::repositories::{ChannelRepository as _, FfmpegProfileRepository as _};
use ersatzrs_domain::MediaStreamKind;
use ersatzrs_infra::graphics_template::{
    graphics_template_variables, GraphicsMedia, GraphicsTemplateContext,
};
use ersatzrs_infra::{
    db::DbPool,
    external_json::{
        playback::{
            self, ExternalMediaSource, ExternalPlaybackError, ExternalPlaybackItem,
            ExternalPlayoutSelection,
        },
        ExternalJsonProgram,
    },
    ffprobe::FfprobeExtractor,
    repositories::make_repositories,
};
use httpmock::prelude::*;
use serde_json::json;
use sqlx::Executor as _;
use tokio::time::Instant;

fn graphics_context(media: GraphicsMedia) -> GraphicsTemplateContext {
    GraphicsTemplateContext {
        channel_id: 1,
        channel_number: "1".into(),
        media,
        media_path: "fixture.mkv".into(),
        channel_start_time: common::fixed_time(),
        content_start_time: common::fixed_time(),
        stream_seek_seconds: 0.0,
        duration_seconds: 120.0,
        frame_width: 64,
        frame_height: 64,
        scaled_frame_width: 64,
        scaled_frame_height: 64,
        r_frame_rate: Some("25/1".into()),
        frame_rate: Some(25.0),
    }
}

async fn fixture() -> DbPool {
    let pool = common::test_db().await;
    let repos = make_repositories(pool.clone());
    repos
        .ffmpeg_profile
        .upsert(&common::profile(1, "fixture"))
        .await
        .unwrap();
    repos
        .channel
        .upsert(&common::channel(1, "1", true))
        .await
        .unwrap();
    pool
}

async fn server(pool: &DbPool) {
    pool.execute(r#"INSERT INTO "MediaSource" ("Id") VALUES (10);
        INSERT INTO "PlexMediaSource" ("Id", "ServerName", "ClientIdentifier") VALUES (10, 'Fixture Plex', 'fixture-plex');"#)
        .await.unwrap();
}

async fn library(pool: &DbPool) {
    pool.execute(
        r#"INSERT INTO "Library" ("Id", "MediaSourceId", "MediaKind") VALUES (10, 10, 1);
        INSERT INTO "PlexLibrary" ("Id", "ShouldSyncItems") VALUES (10, 1);
        INSERT INTO "LibraryPath" ("Id", "LibraryId") VALUES (10, 10);"#,
    )
    .await
    .unwrap();
}

async fn connection(pool: &DbPool, uri: &str) {
    sqlx::query(r#"INSERT INTO "PlexConnection" ("Id", "PlexMediaSourceId", "IsActive", "Uri") VALUES (10, 10, 1, ?)"#)
        .bind(uri).execute(pool).await.unwrap();
}

async fn synthetic_token(pool: &DbPool) {
    sqlx::query(r#"INSERT INTO "ConfigElement" ("Key", "Value") VALUES ('plex.server_auth_token.fixture-plex', ?)"#)
        .bind("synthetic-external-fixture").execute(pool).await.unwrap();
}

fn occurrence() -> ExternalPlaybackItem {
    ExternalPlaybackItem {
        playout_id: 1,
        start: common::fixed_time(),
        finish: common::fixed_time() + Duration::minutes(2),
        program: ExternalJsonProgram {
            title: Some("Fixture programme".into()),
            server_key: Some("Fixture Plex".into()),
            rating_key: Some("42".into()),
            plex_file: Some("/library/parts/50/file.mkv".into()),
            duration: 120_000,
            ..ExternalJsonProgram::default()
        },
    }
}

async fn resolve(
    pool: &DbPool,
    item: ExternalPlaybackItem,
) -> Result<playback::ResolvedExternalMedia, ExternalPlaybackError> {
    let (_sender, cancellation) = tokio::sync::oneshot::channel();
    playback::resolve(
        pool,
        item,
        &FfprobeExtractor::new("unused-ffprobe"),
        Instant::now() + std::time::Duration::from_secs(5),
        cancellation,
    )
    .await
}

#[tokio::test]
async fn external_json_selection_distinguishes_exhaustion_and_projects_mirrors() {
    let pool = fixture().await;
    let now = common::fixed_time();
    assert!(matches!(
        playback::select(&pool, 1, now).await.unwrap(),
        ExternalPlayoutSelection::NotConfigured
    ));
    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().join("schedule.json");
    std::fs::write(
        &path,
        serde_json::to_vec(&json!({"startTime":now.to_rfc3339(),
        "programs":[{"title":"First","duration":60_000},{"title":"Second","duration":60_000}]}))
        .unwrap(),
    )
    .unwrap();
    sqlx::query(r#"INSERT INTO "Playout" ("Id", "ChannelId", "ScheduleKind", "ScheduleFile", "Seed") VALUES (1, 1, 20, ?, 1)"#)
        .bind(path.to_str().unwrap()).execute(&pool).await.unwrap();
    let mut mirror = common::channel(2, "2", true);
    mirror.playout_source = ersatzrs_domain::ChannelPlayoutSource::Mirror;
    mirror.mirror_source_channel_id = Some(1);
    mirror.playout_offset = Some(Duration::hours(1));
    make_repositories(pool.clone())
        .channel
        .upsert(&mirror)
        .await
        .unwrap();
    for (channel, offset) in [(1, Duration::zero()), (2, Duration::hours(1))] {
        let ExternalPlayoutSelection::Item(item) =
            playback::select(&pool, channel, now + offset + Duration::minutes(1))
                .await
                .unwrap()
        else {
            panic!("programme expected")
        };
        assert_eq!(item.start, now + offset + Duration::minutes(1));
        assert_eq!(item.finish, now + offset + Duration::minutes(2));
        assert_eq!(item.program.title.as_deref(), Some("Second"));
        assert!(matches!(
            playback::select(&pool, channel, now + offset + Duration::minutes(2))
                .await
                .unwrap(),
            ExternalPlayoutSelection::Unavailable
        ));
    }
    std::fs::write(&path, br#"{"startTime":null,"programs":[]}"#).unwrap();
    assert!(matches!(
        playback::select(&pool, 1, now).await,
        Err(ExternalPlaybackError::Schedule(
            ersatzrs_infra::external_json::ExternalJsonError::StartTime
        ))
    ));
}

#[tokio::test]
async fn external_json_graphics_keep_occurrence_metadata_and_read_only_the_selected_guide() {
    let pool = fixture().await;
    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().join("schedule.json");
    let start = common::fixed_time();
    let schedule = |title: &str| {
        json!({"startTime":start.to_rfc3339(), "programs":[
            {"type":"episode", "showTitle":"Fixture show", "title":title, "duration":120_000}
        ]})
    };
    std::fs::write(&path, serde_json::to_vec(&schedule("First")).unwrap()).unwrap();
    sqlx::query(r#"INSERT INTO "Playout" ("Id", "ChannelId", "ScheduleKind", "ScheduleFile", "Seed") VALUES (1, 1, 20, ?, 1)"#)
        .bind(path.to_str().unwrap()).execute(&pool).await.unwrap();
    let repos = make_repositories(pool.clone());
    let mut mirror = common::channel(2, "2", true);
    mirror.playout_source = ersatzrs_domain::ChannelPlayoutSource::Mirror;
    mirror.mirror_source_channel_id = Some(1);
    mirror.playout_offset = Some(Duration::hours(1));
    repos.channel.upsert(&mirror).await.unwrap();
    repos
        .channel
        .upsert(&common::channel(3, "3", true))
        .await
        .unwrap();
    let invalid = directory.path().join("invalid.json");
    std::fs::write(&invalid, b"not JSON").unwrap();
    sqlx::query(r#"INSERT INTO "Playout" ("Id", "ChannelId", "ScheduleKind", "ScheduleFile", "Seed") VALUES (3, 3, 20, ?, 1)"#)
        .bind(invalid.to_str().unwrap()).execute(&pool).await.unwrap();
    let mut context = graphics_context(GraphicsMedia::Transient(
        json!({"MediaItem_Title":"Selected occurrence"})
            .as_object()
            .unwrap()
            .clone(),
    ));
    for (channel, offset) in [(1, Duration::zero()), (2, Duration::hours(1))] {
        context.channel_id = channel;
        context.content_start_time = start + offset;
        let variables = graphics_template_variables(&pool, &context, 1, None)
            .await
            .unwrap();
        assert_eq!(variables["MediaItem_Title"], "Selected occurrence");
        assert_eq!(variables["Epg"][0]["Title"], "Fixture show");
        assert_eq!(variables["Epg"][0]["SubTitle"], "First");
        assert_eq!(variables["Epg"][0]["Start"], (start + offset).to_rfc3339());
        assert_eq!(
            variables["Epg"][0]["Stop"],
            (start + offset + Duration::minutes(2)).to_rfc3339()
        );
    }
    std::fs::write(&path, serde_json::to_vec(&schedule("Updated")).unwrap()).unwrap();
    let variables =
        graphics_template_variables(&pool, &context, 1, Some(r#"{"Custom":"Overlay"}"#))
            .await
            .unwrap();
    assert_eq!(variables["MediaItem_Title"], "Selected occurrence");
    assert_eq!(variables["Custom"], "Overlay");
    assert_eq!(variables["Epg"][0]["SubTitle"], "Updated");
    std::fs::write(&path, b"not JSON").unwrap();
    assert!(graphics_template_variables(&pool, &context, 1, None)
        .await
        .is_err());
    let count: i64 = sqlx::query_scalar(r#"SELECT COUNT(*) FROM "MediaItem""#)
        .fetch_one(&pool)
        .await
        .unwrap();
    assert_eq!(count, 0);
}

#[tokio::test]
async fn external_json_plex_preconditions_have_distinct_redacted_errors() {
    let pool = fixture().await;
    assert_eq!(
        resolve(&pool, occurrence()).await.err().unwrap(),
        ExternalPlaybackError::MissingServer
    );
    server(&pool).await;
    assert_eq!(
        resolve(&pool, occurrence()).await.err().unwrap(),
        ExternalPlaybackError::MissingLibrary
    );
    library(&pool).await;
    assert_eq!(
        resolve(&pool, occurrence()).await.err().unwrap(),
        ExternalPlaybackError::MissingConnection
    );
    connection(&pool, "http://127.0.0.1:1").await;
    assert_eq!(
        resolve(&pool, occurrence()).await.err().unwrap(),
        ExternalPlaybackError::MissingToken
    );
    synthetic_token(&pool).await;
    for key in [
        "https://example.invalid/library/parts/1",
        "/library/parts/../../identity",
        "/library/parts/1\r\nX-Injected: true",
    ] {
        let mut item = occurrence();
        item.program.plex_file = Some(key.into());
        let error = resolve(&pool, item).await.err().unwrap();
        assert_eq!(error, ExternalPlaybackError::InvalidPart);
        assert!(!error.to_string().contains(key));
    }
    let (sender, cancellation) = tokio::sync::oneshot::channel();
    sender.send(()).unwrap();
    assert!(matches!(
        playback::resolve(
            &pool,
            occurrence(),
            &FfprobeExtractor::new("unused"),
            Instant::now() + std::time::Duration::from_secs(5),
            cancellation
        )
        .await,
        Err(ExternalPlaybackError::Cancelled)
    ));
}

#[tokio::test]
async fn external_json_plex_metadata_preserves_streams_without_library_writes() {
    let pool = fixture().await;
    server(&pool).await;
    library(&pool).await;
    let mock_server = MockServer::start_async().await;
    connection(&pool, &mock_server.base_url()).await;
    synthetic_token(&pool).await;
    // Official PMS JSON schema uses capitalized Media, Part and Stream arrays.
    let response = mock_server.mock_async(|when, then| {
        when.method(GET).path("/library/metadata/42").header("X-Plex-Token", "synthetic-external-fixture");
        then.status(200).json_body(json!({"MediaContainer":{"Metadata":[{
            "ratingKey":"42","title":"Plex programme","Media":[{"id":1,"duration":120_000,"width":64,"height":64,
                "Part":[{"id":50,"file":"fixture.mkv","key":"/library/parts/50/file.mkv","Stream":[
                    {"index":0,"streamType":1,"codec":"h264","frameRate":"25","width":64,"height":64},
                    {"index":1,"streamType":2,"codec":"aac","channels":2,"languageCode":"nld","default":true},
                    {"index":2,"streamType":3,"codec":"srt","languageCode":"eng","forced":true}
                ]}]}]
        }]}}));
    }).await;
    let media = resolve(&pool, occurrence()).await.unwrap();
    response.assert_async().await;
    assert!(matches!(
        media.source,
        ExternalMediaSource::Plex {
            media_source_id: 10,
            ..
        }
    ));
    assert_eq!(media.probe.version.duration, Duration::minutes(2));
    assert_eq!(media.probe.streams.len(), 3);
    assert!(media
        .probe
        .streams
        .iter()
        .any(|stream| stream.kind == MediaStreamKind::Audio
            && stream.language.as_deref() == Some("nld")));
    assert!(media
        .probe
        .streams
        .iter()
        .any(|stream| stream.kind == MediaStreamKind::Subtitle
            && stream.forced
            && stream.language.as_deref() == Some("eng")));
    let selected = media
        .tracks()
        .select_subtitle(
            &pool,
            &["en".into()],
            ersatzrs_domain::ChannelSubtitleMode::Forced,
            ersatzrs_infra::playback::transient::TrackSelectionPolicy {
                mode: ersatzrs_domain::StreamingMode::HttpLiveStreamingSegmenter,
                engine: ersatzrs_infra::playback::SubtitleSelectionEngine::Next,
                media_server: true,
                prepare_embedded_text: false,
            },
        )
        .await
        .unwrap()
        .unwrap();
    assert_eq!(selected.stream_index, 2);
    assert!(selected.forced);
    assert!(matches!(
        media.audio_metadata(),
        ersatzrs_infra::audio_script::transient::AudioMetadata::Movie {
            title: Some("Plex programme"),
            ..
        }
    ));
    let variables =
        graphics_template_variables(&pool, &graphics_context(media.graphics_media()), 0, None)
            .await
            .unwrap();
    assert_eq!(variables["MediaItem_Title"], "Plex programme");
    assert_eq!(variables["MediaItem_Resolution"]["Width"], 64);
    assert_eq!(variables["MediaItem_Duration"]["TotalSeconds"], 120.0);
    assert_eq!(media.plex_metadata.unwrap().title, "Plex programme");
    for query in [
        r#"SELECT COUNT(*) FROM "MediaItem""#,
        r#"SELECT COUNT(*) FROM "MediaVersion""#,
        r#"SELECT COUNT(*) FROM "PlayoutItem""#,
    ] {
        let count: i64 = sqlx::query_scalar(query).fetch_one(&pool).await.unwrap();
        assert_eq!(count, 0);
    }
}

#[tokio::test]
async fn external_json_relay_binds_parts_and_reads_current_credentials() {
    use ersatzrs_infra::next_engine::media::MediaRelay;

    let pool = fixture().await;
    server(&pool).await;
    let upstream = MockServer::start_async().await;
    connection(&pool, &upstream.base_url()).await;
    synthetic_token(&pool).await;
    let part = "/library/parts/50/file.mkv";
    let response = upstream
        .mock_async(|when, then| {
            when.method(GET)
                .path(part)
                .header("X-Plex-Token", "synthetic-external-fixture")
                .header("Range", "bytes=2-5");
            then.status(206)
                .header("Content-Range", "bytes 2-5/10")
                .body("2345");
        })
        .await;
    let relay = MediaRelay::new();
    relay.configure("127.0.0.1:8409".parse().unwrap()).unwrap();
    let expires = chrono::Utc::now() + Duration::hours(2);
    let url = url::Url::parse(&relay.external_source_url(10, part, expires).unwrap()).unwrap();
    let ticket = url
        .query_pairs()
        .find(|(key, _)| key == "ticket")
        .unwrap()
        .1;
    assert!(relay
        .open_external(
            &pool,
            10,
            expires.timestamp(),
            "/library/parts/51/file.mkv",
            &ticket,
            None
        )
        .await
        .is_err());
    response.assert_calls_async(0).await;
    let opened = relay
        .open_external(
            &pool,
            10,
            expires.timestamp(),
            part,
            &ticket,
            Some("bytes=2-5"),
        )
        .await
        .unwrap();
    assert_eq!(opened.status().as_u16(), 206);
    assert_eq!(opened.headers()["Content-Range"], "bytes 2-5/10");
    assert_eq!(opened.text().await.unwrap(), "2345");
    response.assert_async().await;

    // A previously minted ticket must use the current credential on every open.
    sqlx::query(r#"UPDATE "ConfigElement" SET "Value" = 'synthetic-rotated-fixture' WHERE "Key" = 'plex.server_auth_token.fixture-plex'"#)
        .execute(&pool).await.unwrap();
    let rotated = upstream
        .mock_async(|when, then| {
            when.method(GET)
                .path(part)
                .header("X-Plex-Token", "synthetic-rotated-fixture");
            then.status(200).body("current");
        })
        .await;
    assert_eq!(
        relay
            .open_external(&pool, 10, expires.timestamp(), part, &ticket, None)
            .await
            .unwrap()
            .text()
            .await
            .unwrap(),
        "current"
    );
    rotated.assert_async().await;
}

#[tokio::test]
#[ignore = "requires the verified FFmpeg runtime through ERSATZRS_FFPROBE"]
async fn external_json_local_probe_prefers_mapped_file_without_plex_credentials() {
    let probe_path = std::path::PathBuf::from(ersatzrs_ffmpeg::ffprobe_path());
    let ffmpeg = probe_path.with_file_name(format!("ffmpeg{}", std::env::consts::EXE_SUFFIX));
    let directory = tempfile::tempdir().unwrap();
    let clip = create_probe_fixture(&ffmpeg, directory.path()).await;
    let pool = fixture().await;
    server(&pool).await;
    library(&pool).await;
    sqlx::query(r#"INSERT INTO "PlexPathReplacement" ("PlexMediaSourceId", "PlexPath", "LocalPath") VALUES (10, '/provider/external', ?)"#)
        .bind(directory.path().to_str().unwrap()).execute(&pool).await.unwrap();
    let mut item = occurrence();
    item.program.file = Some("/provider/external/clip.mkv".into());
    item.program.show_title = Some("Fixture show".into());
    item.program.season = 2;
    item.program.episode = 3;
    let (_sender, cancellation) = tokio::sync::oneshot::channel();
    let media = playback::resolve(
        &pool,
        item,
        &FfprobeExtractor::new(probe_path.to_string_lossy()),
        Instant::now() + std::time::Duration::from_secs(15),
        cancellation,
    )
    .await
    .unwrap();
    assert!(matches!(
        media.audio_metadata(),
        ersatzrs_infra::audio_script::transient::AudioMetadata::Episode {
            show_title: Some("Fixture show"),
            season: 2,
            episode: 3,
            guids: [],
            show_guids: [],
        }
    ));
    let variables =
        graphics_template_variables(&pool, &graphics_context(media.graphics_media()), 0, None)
            .await
            .unwrap();
    assert_eq!(variables["MediaItem_Title"], "Fixture programme");
    assert_eq!(variables["MediaItem_ShowTitle"], "Fixture show");
    let ExternalMediaSource::Local(path) = media.source else {
        panic!("local file should win")
    };
    assert_eq!(std::path::Path::new(&path), clip);
    assert!(media.plex_metadata.is_none());
    assert!(media
        .probe
        .streams
        .iter()
        .any(|stream| stream.kind == MediaStreamKind::Audio
            && stream.language.as_deref() == Some("nld")));
    assert!(media
        .probe
        .streams
        .iter()
        .any(|stream| stream.kind == MediaStreamKind::Subtitle
            && stream.language.as_deref() == Some("eng")));
    let video = ersatzrs_infra::playback::source_video_stream_from_probe(&media.probe)
        .unwrap()
        .unwrap();
    assert_eq!(video.index, 0);
    assert_eq!(
        ersatzrs_infra::playback::source_audio_streams_from_probe(&media.probe)
            .unwrap()
            .len(),
        1
    );
    let selected = ersatzrs_infra::playback::transient::TransientPlaybackTracks::from_streams(
        &media.probe.streams,
    )
    .subtitles
    .into_iter()
    .find(|track| track.language.as_deref() == Some("eng"))
    .unwrap();
    let artifact = ersatzrs_infra::external_json::subtitles::PreparedSubtitle::extract(
        ersatzrs_infra::next_engine::protocol::Source::Local {
            path,
            probe_hint: None,
            in_point_ms: None,
            out_point_ms: None,
        },
        selected,
        ffmpeg,
        Instant::now() + std::time::Duration::from_secs(15),
    )
    .await
    .unwrap();
    let prepared = artifact.subtitle();
    assert!(prepared.is_extracted);
    assert_eq!(prepared.language.as_deref(), Some("eng"));
    let output = std::path::PathBuf::from(prepared.path.as_ref().unwrap());
    let text = std::fs::read_to_string(&output).unwrap();
    assert!(text.contains("Fixture subtitle"));
    assert!(text.contains("00:00:00,000 --> 00:00:01,000"));
    let temporary_directory = output.parent().unwrap().to_owned();
    drop(artifact);
    assert!(
        !temporary_directory.exists(),
        "the stream owner releases its temporary subtitle"
    );
}

#[tokio::test]
async fn external_json_plex_metadata_rejects_redirects_oversize_and_deadline_expiry() {
    let pool = fixture().await;
    server(&pool).await;
    library(&pool).await;
    let mock_server = MockServer::start_async().await;
    connection(&pool, &mock_server.base_url()).await;
    synthetic_token(&pool).await;
    let destination = mock_server
        .mock_async(|when, then| {
            when.method(GET).path("/unexpected-target");
            then.status(200);
        })
        .await;
    let redirect = mock_server
        .mock_async(|when, then| {
            when.method(GET).path("/library/metadata/42");
            then.status(302)
                .header("Location", mock_server.url("/unexpected-target"));
        })
        .await;
    assert_eq!(
        resolve(&pool, occurrence()).await.err().unwrap(),
        ExternalPlaybackError::Metadata
    );
    redirect.assert_async().await;
    destination.assert_calls_async(0).await;

    let oversized = mock_server
        .mock_async(|when, then| {
            when.method(GET).path("/library/metadata/43");
            then.status(200).header("Content-Length", "8388609");
        })
        .await;
    let mut item = occurrence();
    item.program.rating_key = Some("43".into());
    assert_eq!(
        resolve(&pool, item).await.err().unwrap(),
        ExternalPlaybackError::Metadata
    );
    oversized.assert_async().await;

    let slow = mock_server
        .mock_async(|when, then| {
            when.method(GET).path("/library/metadata/44");
            then.status(200).delay(std::time::Duration::from_secs(2));
        })
        .await;
    let mut item = occurrence();
    item.program.rating_key = Some("44".into());
    let (_sender, cancellation) = tokio::sync::oneshot::channel();
    let result = playback::resolve(
        &pool,
        item,
        &FfprobeExtractor::new("unused"),
        Instant::now() + std::time::Duration::from_millis(500),
        cancellation,
    )
    .await;
    assert_eq!(result.err().unwrap(), ExternalPlaybackError::Timeout);
    slow.assert_async().await;
}

async fn create_probe_fixture(
    ffmpeg: &std::path::Path,
    directory: &std::path::Path,
) -> std::path::PathBuf {
    let subtitle = directory.join("subtitle.srt");
    std::fs::write(
        &subtitle,
        "1\n00:00:00,000 --> 00:00:01,000\nFixture subtitle\n",
    )
    .unwrap();
    let clip = directory.join("clip.mkv");
    let output = tokio::process::Command::new(ffmpeg)
        .args([
            "-hide_banner",
            "-loglevel",
            "error",
            "-f",
            "lavfi",
            "-i",
            "color=size=64x64:rate=25:duration=1",
            "-f",
            "lavfi",
            "-i",
            "sine=frequency=880:duration=1",
            "-i",
        ])
        .arg(&subtitle)
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
            "1",
            "-y",
        ])
        .arg(&clip)
        .output()
        .await
        .unwrap();
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    clip
}

#[tokio::test]
#[ignore = "requires the verified FFmpeg runtime and loopback sockets"]
async fn external_json_cancelled_subtitle_request_reaps_stalled_ffmpeg() {
    use ersatzrs_infra::{
        external_json::subtitles::PreparedSubtitle, next_engine::protocol::Source,
        playback::transient::TransientPlaybackTracks,
    };
    use tokio::io::AsyncReadExt as _;

    let ffmpeg = std::path::PathBuf::from(ersatzrs_ffmpeg::ffprobe_path())
        .with_file_name(format!("ffmpeg{}", std::env::consts::EXE_SUFFIX));
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let source = Source::Http {
        uri: format!(
            "http://{}/synthetic-stalled-subtitle",
            listener.local_addr().unwrap()
        ),
        probe_hint: None,
        is_live: None,
        headers: Vec::new(),
        user_agent: None,
        keep_alive: None,
        reconnect: Some(false),
        reconnect_delay_max: None,
        timeout_us: Some(60_000_000),
        in_point_ms: None,
        out_point_ms: None,
    };
    let subtitle = TransientPlaybackTracks::from_streams(&[ersatzrs_domain::MediaStream {
        index: 2,
        kind: MediaStreamKind::Subtitle,
        codec: Some("subrip".into()),
        ..Default::default()
    }])
    .subtitles
    .remove(0);
    let request = tokio::spawn(PreparedSubtitle::extract(
        source,
        subtitle,
        ffmpeg,
        Instant::now() + std::time::Duration::from_mins(1),
    ));
    tokio::time::timeout(std::time::Duration::from_secs(12), async {
        let (mut socket, _) = listener.accept().await.unwrap();
        let mut bytes = Vec::new();
        let mut buffer = [0; 512];
        while !bytes.windows(4).any(|part| part == b"\r\n\r\n") {
            let read = socket.read(&mut buffer).await.unwrap();
            assert_ne!(read, 0);
            bytes.extend_from_slice(&buffer[..read]);
            assert!(bytes.len() <= 8192);
        }
        assert!(bytes.starts_with(b"GET /synthetic-stalled-subtitle "));
        request.abort();
        assert!(matches!(request.await, Err(error) if error.is_cancelled()));
        // Both FFmpeg's transport timeout and the worker deadline are 60 seconds.
        // Only cancellation should close this socket inside the 12-second bound.
        match socket.read(&mut buffer).await {
            Ok(0) => {}
            Err(error)
                if matches!(
                    error.kind(),
                    std::io::ErrorKind::ConnectionReset
                        | std::io::ErrorKind::ConnectionAborted
                        | std::io::ErrorKind::BrokenPipe
                ) => {}
            _ => panic!("cancelled preparation left its FFmpeg connection open"),
        }
    })
    .await
    .expect("request cancellation must terminate the extraction child");
}

#[tokio::test]
async fn external_json_plex_sidecar_uses_current_credentials_and_rejects_redirects() {
    use ersatzrs_infra::{
        external_json::subtitles::PreparedSubtitle, playback::transient::TransientPlaybackTracks,
    };
    let pool = fixture().await;
    server(&pool).await;
    let upstream = MockServer::start_async().await;
    connection(&pool, &upstream.base_url()).await;
    synthetic_token(&pool).await;
    let subtitle = TransientPlaybackTracks::from_streams(&[ersatzrs_domain::MediaStream {
        index: 27,
        kind: MediaStreamKind::ExternalSubtitle,
        codec: Some("subrip".into()),
        file_name: Some("/library/streams/27".into()),
        language: Some("nld".into()),
        ..Default::default()
    }])
    .subtitles
    .remove(0);
    let response = upstream
        .mock_async(|when, then| {
            when.method(GET)
                .path("/library/streams/27")
                .header("X-Plex-Token", "synthetic-external-fixture");
            then.status(200)
                .body("1\n00:00:00,000 --> 00:00:01,000\nFixture sidecar\n");
        })
        .await;
    let deadline = || Instant::now() + std::time::Duration::from_secs(5);
    let artifact = PreparedSubtitle::fetch_plex(&pool, 10, subtitle.clone(), deadline())
        .await
        .unwrap();
    let path = std::path::PathBuf::from(artifact.subtitle().path.as_ref().unwrap());
    assert!(std::fs::read_to_string(&path)
        .unwrap()
        .contains("Fixture sidecar"));
    assert_eq!(artifact.subtitle().stream_index, 27);
    assert_eq!(artifact.subtitle().language.as_deref(), Some("nld"));
    drop(artifact);
    assert!(!path.parent().unwrap().exists());
    response.assert_async().await;

    sqlx::query(r#"UPDATE "ConfigElement" SET "Value" = 'synthetic-rotated-fixture' WHERE "Key" = 'plex.server_auth_token.fixture-plex'"#)
        .execute(&pool).await.unwrap();
    let redirect = upstream
        .mock_async(|when, then| {
            when.method(GET)
                .path("/library/streams/27")
                .header("X-Plex-Token", "synthetic-rotated-fixture");
            then.status(302).header("Location", "/unexpected-sidecar");
        })
        .await;
    let destination = upstream
        .mock_async(|when, then| {
            when.method(GET).path("/unexpected-sidecar");
            then.status(200).body("unexpected");
        })
        .await;
    assert!(
        PreparedSubtitle::fetch_plex(&pool, 10, subtitle.clone(), deadline())
            .await
            .is_err()
    );
    redirect.assert_async().await;
    destination.assert_calls_async(0).await;
    let mut oversized = subtitle;
    oversized.path = Some("/library/streams/28".into());
    let response = upstream
        .mock_async(|when, then| {
            when.method(GET).path("/library/streams/28");
            then.status(200).header("Content-Length", "33554433");
        })
        .await;
    assert!(
        PreparedSubtitle::fetch_plex(&pool, 10, oversized, deadline())
            .await
            .is_err()
    );
    response.assert_async().await;
}
