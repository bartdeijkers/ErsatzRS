//! Real private HTTP song PNGs from the public troubleshooting preparation API.

use super::*;
use ersatzrs_core::repositories::{ChannelRepository as _, FfmpegProfileRepository as _};
use ersatzrs_domain::{Channel, FFmpegProfile, PlayoutItem, StreamingEngine};
use ersatzrs_infra::next_engine::{
    host::{NextHost, NextStart},
    protocol::Source,
    schedule,
    troubleshooting::TroubleshootingPreview,
};
use std::{path::PathBuf, sync::Arc, time::Duration};
use tokio::time::Instant;

struct Fixture {
    root: tempfile::TempDir,
    state: crate::host::state::AppState,
    ffmpeg: PathBuf,
    stop: tokio_util::sync::CancellationToken,
    server: tokio::task::JoinHandle<()>,
}

#[allow(clippy::disallowed_methods)] // Explicit package runtime gate.
fn ffmpeg() -> PathBuf {
    std::env::var_os("ERSATZRS_NEXT_TEST_FFMPEG")
        .map(PathBuf::from)
        .expect("verified FFmpeg runtime")
}

impl Fixture {
    async fn new() -> Self {
        let root = tempfile::tempdir().unwrap();
        let pool = sqlx::sqlite::SqlitePoolOptions::new()
            .max_connections(1)
            .connect("sqlite::memory:")
            .await
            .unwrap();
        sqlx::migrate!("../../migrations").run(&pool).await.unwrap();
        sqlx::raw_sql(r#"
            INSERT INTO "MediaSource" ("Id") VALUES (1);
            INSERT INTO "Library" ("Id","Name","MediaKind","MediaSourceId") VALUES (1,'Synthetic',1,1);
            INSERT INTO "LocalLibrary" ("Id") VALUES (1);
            INSERT INTO "LibraryPath" ("Id","LibraryId","Path") VALUES (1,1,'media');
            INSERT INTO "MediaItem" ("Id","LibraryPathId","State") VALUES (10,1,0);
            INSERT INTO "Song" ("Id") VALUES (10);
            INSERT INTO "SongMetadata" ("Id","SongId","Title","Artists","MetadataKind","DateAdded","DateUpdated") VALUES (20,10,'Synthetic Song','["Synthetic Artist"]',0,'2026-09-22','2026-09-22');
            INSERT INTO "MediaVersion" ("Id","DateAdded","DateUpdated","Duration","SongId","VideoScanKind","Width","Height") VALUES (100,'2026-09-22','2026-09-22','00:01:00',10,0,0,0);
            INSERT INTO "MediaStream" ("Id","Index","Codec","MediaStreamKind","Channels","Default","Forced","MediaVersionId") VALUES (1,0,'flac',2,2,1,0,100);
            INSERT INTO "Resolution" ("Id","Name","Width","Height") VALUES (1,'Fixture',320,180);
        "#).execute(&pool).await.unwrap();
        let clip = root.path().join("identity.flac");
        tokio::fs::write(
            &clip,
            b"synthetic identity; callback reads song metadata only",
        )
        .await
        .unwrap();
        sqlx::query(r#"INSERT INTO "MediaFile" ("Id","MediaVersionId","Path","PathHash") VALUES (1,100,?,'fixture')"#).bind(clip.to_str().unwrap()).execute(&pool).await.unwrap();
        let repos = ersatzrs_infra::repositories::make_repositories(pool.clone());
        repos
            .ffmpeg_profile
            .upsert(&FFmpegProfile {
                id: 1,
                resolution_id: 1,
                ..Default::default()
            })
            .await
            .unwrap();
        repos
            .channel
            .upsert(&Channel {
                id: 1,
                number: "1".into(),
                name: "Synthetic".into(),
                ffmpeg_profile_id: 1,
                streaming_engine: StreamingEngine::Next,
                ..Default::default()
            })
            .await
            .unwrap();
        let mut state = crate::host::state::AppState::for_tests(
            crate::host::state::IptvAccessTokenVerifier::disabled(),
        );
        state.pool = pool.clone();
        state.next_engine = Arc::new(NextHost::new(
            pool,
            ersatzrs_core::locking::EntityLocker::default(),
            root.path().join("next"),
        ));
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        state
            .next_engine
            .media
            .configure(listener.local_addr().unwrap())
            .unwrap();
        let router = crate::host::router::build_internal_app(state.clone());
        let stop = tokio_util::sync::CancellationToken::new();
        let shutdown = stop.clone();
        let server = tokio::spawn(async move {
            axum::serve(
                listener,
                router.into_make_service_with_connect_info::<std::net::SocketAddr>(),
            )
            .with_graceful_shutdown(shutdown.cancelled_owned())
            .await
            .unwrap();
        });
        Self {
            root,
            state,
            ffmpeg: ffmpeg(),
            stop,
            server,
        }
    }

    async fn preview(&self) -> (uuid::Uuid, Source) {
        let channel = ersatzrs_infra::repositories::make_repositories(self.state.pool.clone())
            .channel
            .get_with_profile(1)
            .await
            .unwrap();
        let mut profile =
            ersatzrs_infra::playback::load_channel_playback_profile(&self.state.pool, 1)
                .await
                .unwrap()
                .unwrap();
        // A saved preview profile deliberately differs from the persisted one.
        profile.resolution.width = 160;
        profile.resolution.height = 96;
        let start = chrono::Utc::now();
        let media = schedule::bind_media(
            &self.state.pool,
            PlayoutItem {
                media_item_id: 10,
                start,
                finish: start + chrono::TimeDelta::minutes(1),
                out_point: chrono::TimeDelta::minutes(1),
                ..Default::default()
            },
        )
        .await
        .unwrap();
        let (session, item) = self
            .state
            .next_engine
            .prepare_troubleshooting_session(
                TroubleshootingPreview {
                    media: &media,
                    channel: &channel,
                    profile: &profile,
                    subtitle_id: Some(0),
                    selection_time: None,
                    watermark_ids: &[],
                    graphics_ids: &[],
                    replacements: &[],
                    ffmpeg: &self.ffmpeg,
                    deadline: Instant::now() + Duration::from_secs(30),
                    diagnostics: None,
                },
                NextStart {
                    channel_id: 1,
                    number: "1".into(),
                    preview: true,
                    executable: "unused-next".into(),
                    output: self.root.path().join("output"),
                    ffmpeg: self.ffmpeg.clone(),
                    ffprobe: "unused-ffprobe".into(),
                    replacements: Vec::new(),
                    idle_timeout: None,
                    deadline: Instant::now() + Duration::from_secs(30),
                },
            )
            .await
            .unwrap();
        (session, item.tracks.unwrap().video.unwrap().source.unwrap())
    }

    async fn decoded(&self, bytes: &[u8]) {
        let path = self.root.path().join("served.png");
        tokio::fs::write(&path, bytes).await.unwrap();
        let output = tokio::time::timeout(
            Duration::from_secs(20),
            tokio::process::Command::new(&self.ffmpeg)
                .args(["-nostdin", "-v", "error", "-i"])
                .arg(path)
                .args([
                    "-frames:v",
                    "1",
                    "-pix_fmt",
                    "rgb24",
                    "-f",
                    "rawvideo",
                    "pipe:1",
                ])
                .kill_on_drop(true)
                .output(),
        )
        .await
        .unwrap()
        .unwrap();
        assert!(output.status.success());
        assert_eq!(output.stdout.len(), 160 * 96 * 3);
    }
}

impl Drop for Fixture {
    fn drop(&mut self) {
        self.stop.cancel();
        self.server.abort();
    }
}

async fn request(
    client: &reqwest::Client,
    source: &Source,
    session: Option<uuid::Uuid>,
) -> reqwest::Response {
    let Source::Http { uri, headers, .. } = source else {
        panic!("song callback source")
    };
    let mut map = HeaderMap::new();
    for entry in headers {
        let (name, value) = entry.split_once(": ").unwrap();
        map.insert(
            name.parse::<header::HeaderName>().unwrap(),
            value.parse().unwrap(),
        );
    }
    if let Some(session) = session {
        map.insert(SESSION_HEADER, session.to_string().parse().unwrap());
    }
    client.get(uri).headers(map).send().await.unwrap()
}

#[tokio::test]
#[ignore = "requires pinned FFmpeg; real private HTTP saved song preview and revocation"]
async fn song_http_uses_saved_preview_geometry_and_stable_choice_and_rejects_cross_session_and_revocation(
) {
    let fixture = Fixture::new().await;
    let client = reqwest::Client::builder()
        .no_proxy()
        .timeout(Duration::from_secs(20))
        .build()
        .unwrap();
    let (first, source) = fixture.preview().await;
    let (other, _) = fixture.preview().await;
    let response = request(&client, &source, None).await;
    assert_eq!(response.status(), StatusCode::OK);
    assert_eq!(response.headers()[header::CONTENT_TYPE], "image/png");
    assert_eq!(response.headers()[header::CACHE_CONTROL], "no-store");
    let bytes = response.bytes().await.unwrap();
    fixture.decoded(&bytes).await;
    sqlx::query(r#"UPDATE "Resolution" SET "Width"=640,"Height"=360 WHERE "Id"=1"#)
        .execute(&fixture.state.pool)
        .await
        .unwrap();
    let response = request(&client, &source, None).await;
    assert_eq!(response.status(), StatusCode::OK);
    assert_eq!(bytes, response.bytes().await.unwrap());
    assert_eq!(
        request(&client, &source, Some(other)).await.status(),
        StatusCode::GONE
    );
    fixture
        .state
        .next_engine
        .finish_troubleshooting_session(first)
        .await
        .unwrap();
    assert_eq!(
        request(&client, &source, None).await.status(),
        StatusCode::GONE
    );
    fixture
        .state
        .next_engine
        .finish_troubleshooting_session(other)
        .await
        .unwrap();
    let count: i64 = sqlx::query_scalar(r#"SELECT COUNT(*) FROM "PlayoutItem""#)
        .fetch_one(&fixture.state.pool)
        .await
        .unwrap();
    assert_eq!(count, 0, "preview must not persist a schedule");
    fixture.stop.cancel();
}
