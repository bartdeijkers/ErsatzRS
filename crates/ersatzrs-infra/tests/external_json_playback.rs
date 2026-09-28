mod common;

use chrono::Duration;
use ersatzrs_core::repositories::{ChannelRepository as _, FfmpegProfileRepository as _};
use ersatzrs_domain::MediaStreamKind;
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
