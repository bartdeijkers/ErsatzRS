use super::*;
use chrono::Utc;
use ersatzrs_core::repositories::PlayoutRepository as _;
use std::sync::Arc;

struct Fixture {
    root: tempfile::TempDir,
    host: Arc<NextHost>,
    channel: Channel,
    media: ScheduledMedia,
    profile: PlaybackProfile,
}

impl Fixture {
    async fn new() -> Self {
        let (pool, locker, mut channel, _) =
            super::super::remote_projection::tests::fixture().await;
        let media =
            super::super::schedule::bind_media(&pool, super::super::selection::tests::item())
                .await
                .unwrap();
        let profile = playback::load_playback_profile(&pool, 1)
            .await
            .unwrap()
            .unwrap();
        sqlx::query(r#"DELETE FROM "PlayoutItem""#)
            .execute(&pool)
            .await
            .unwrap();
        channel.id = 0;
        channel.number = ".troubleshooting".into();
        channel.preferred_audio_language_code = Some("nld".into());
        let root = tempfile::tempdir().unwrap();
        let host = Arc::new(NextHost::new(pool, locker, root.path().join("next")));
        host.media
            .configure("127.0.0.1:18419".parse().unwrap())
            .unwrap();
        Self {
            root,
            host,
            channel,
            media,
            profile,
        }
    }

    async fn publish(&self) -> (uuid::Uuid, String) {
        let deadline = Instant::now() + std::time::Duration::from_secs(10);
        let request = TroubleshootingPreview {
            channel: &self.channel,
            media: &self.media,
            profile: &self.profile,
            subtitle_id: Some(0),
            selection_time: None,
            watermark_ids: &[],
            graphics_ids: &[],
            replacements: &[],
            ffmpeg: Path::new("unused-ffmpeg"),
            deadline,
            diagnostics: None,
        };
        let start = super::super::host::NextStart {
            channel_id: 0,
            preview: true,
            number: self.channel.number.clone(),
            executable: "unused-next".into(),
            output: self.root.path().join("output"),
            ffmpeg: "unused-ffmpeg".into(),
            ffprobe: "unused-ffprobe".into(),
            replacements: vec![],
            idle_timeout: None,
            deadline,
        };
        let (instance, item) = self
            .host
            .prepare_troubleshooting_session(request, start)
            .await
            .unwrap();
        assert!(item.tracks.is_none());
        let Some(protocol::Source::Dynamic { uri, .. }) = item.source else {
            panic!("remote preview must use its authenticated callback");
        };
        let segments: Vec<_> = uri.split('/').collect();
        assert_eq!(segments.last(), Some(&"2"));
        (instance, segments[segments.len() - 2].to_owned())
    }

    async fn resolve(&self, instance: uuid::Uuid, digest: &str) -> Result<protocol::PlayoutItem> {
        self.host
            .resolve_dynamic(
                0,
                self.media.item.id,
                digest,
                Some(2),
                &self.channel.number,
                instance,
                self.media.item.start,
                self.media.item.finish,
            )
            .await
    }
}

#[tokio::test]
async fn troubleshooting_remote_callback_needs_no_persisted_channel_or_playout() {
    let fixture = Fixture::new().await;
    let (instance, digest) = fixture.publish().await;
    let item = fixture.resolve(instance, &digest).await.unwrap();
    let tracks = item.tracks.unwrap();
    assert_eq!(tracks.audio.unwrap().stream_index, Some(2));
    assert!(tracks.subtitle.is_none());
    assert!(matches!(
        tracks.video.unwrap().source,
        Some(protocol::Source::Http {
            in_point_ms: Some(1000),
            ..
        })
    ));
    let count: i64 = sqlx::query_scalar(r#"SELECT COUNT(*) FROM "PlayoutItem""#)
        .fetch_one(&fixture.host.pool)
        .await
        .unwrap();
    assert_eq!(count, 0);
    fixture
        .host
        .finish_troubleshooting_session(instance)
        .await
        .unwrap();
    assert!(fixture.resolve(instance, &digest).await.is_err());
}

#[tokio::test]
async fn troubleshooting_remote_callback_rejects_changed_source_and_foreign_session() {
    let fixture = Fixture::new().await;
    let (instance, digest) = fixture.publish().await;
    assert!(fixture
        .resolve(uuid::Uuid::new_v4(), &digest)
        .await
        .is_err());
    sqlx::query(r#"UPDATE "RemoteStream" SET "Url"='https://example.test/replaced' WHERE "Id"=10"#)
        .execute(&fixture.host.pool)
        .await
        .unwrap();
    assert!(fixture.resolve(instance, &digest).await.is_err());
    fixture
        .host
        .finish_troubleshooting_session(instance)
        .await
        .unwrap();
}

async fn prepare_remote_window(
    fixture: &Fixture,
    occurrences: &[TroubleshootingChannelOccurrence],
    at: chrono::DateTime<Utc>,
    finish: chrono::DateTime<Utc>,
) -> (uuid::Uuid, protocol::Playout) {
    let deadline = Instant::now() + std::time::Duration::from_secs(10);
    let start = super::super::host::NextStart {
        channel_id: fixture.channel.id,
        preview: true,
        number: fixture.channel.number.clone(),
        executable: "unused-next".into(),
        output: fixture.root.path().join("output"),
        ffmpeg: "unused-ffmpeg".into(),
        ffprobe: "unused-ffprobe".into(),
        replacements: vec![],
        idle_timeout: None,
        deadline,
    };
    fixture
        .host
        .prepare_troubleshooting_window(
            TroubleshootingChannelWindow {
                channel: &fixture.channel,
                profile: &fixture.profile,
                occurrences,
                start: at,
                finish,
                replacements: &[],
                ffmpeg: Path::new("unused-ffmpeg"),
                deadline,
                diagnostics: None,
            },
            start,
        )
        .await
        .unwrap()
}

#[tokio::test]
async fn troubleshooting_window_remote_second_member_preserves_identity_and_revocation() {
    let fixture = Fixture::new().await;
    let at =
        chrono::DateTime::from_timestamp(chrono::Utc::now().timestamp() - 2 * 86_400, 0).unwrap();
    let mut occurrences = Vec::new();
    for (id, offset, length) in [(41, 0, 10), (42, 10, 20)] {
        let mut media = fixture.media.clone();
        media.item.id = id;
        media.item.playout_id = 1;
        media.item.start = at + chrono::TimeDelta::seconds(offset);
        media.item.finish = media.item.start + chrono::TimeDelta::seconds(length);
        media.item.in_point = chrono::TimeDelta::seconds(1);
        media.item.out_point = chrono::TimeDelta::seconds(1 + length);
        crate::repositories::make_repositories(fixture.host.pool.clone())
            .playout
            .upsert_item(&media.item)
            .await
            .unwrap();
        occurrences.push(TroubleshootingChannelOccurrence {
            media,
            channel: fixture.channel.clone(),
            subtitle_id: Some(0),
            credits: None,
        });
    }
    let finish = at + chrono::TimeDelta::seconds(30);
    let before = super::tests::persisted_window_state(&fixture.host.pool, at, finish).await;
    let (instance, playout) = prepare_remote_window(&fixture, &occurrences, at, finish).await;
    assert_eq!(
        before,
        super::tests::persisted_window_state(&fixture.host.pool, at, finish).await,
        "remote window preparation must preserve scheduled rows and their exact state"
    );
    let Some(protocol::Source::Dynamic { uri, .. }) = &playout.items[1].source else {
        panic!("remote callback");
    };
    let segments: Vec<_> = uri.split('/').collect();
    let digest = segments[segments.len() - 2];
    let member = &occurrences[1].media;
    let resolved = fixture
        .host
        .resolve_dynamic(
            fixture.channel.id,
            member.item.id,
            digest,
            Some(2),
            &fixture.channel.number,
            instance,
            member.item.start,
            member.item.finish,
        )
        .await
        .unwrap();
    assert_eq!(resolved.id, "42");
    assert_eq!(
        resolved
            .tracks
            .as_ref()
            .unwrap()
            .audio
            .as_ref()
            .unwrap()
            .stream_index,
        Some(2)
    );
    assert!(fixture
        .host
        .resolve_dynamic(
            fixture.channel.id,
            41,
            digest,
            Some(2),
            &fixture.channel.number,
            instance,
            member.item.start,
            member.item.finish
        )
        .await
        .is_err());
    fixture
        .host
        .finish_troubleshooting_session(instance)
        .await
        .unwrap();
    assert!(fixture
        .host
        .resolve_dynamic(
            fixture.channel.id,
            member.item.id,
            digest,
            Some(2),
            &fixture.channel.number,
            instance,
            member.item.start,
            member.item.finish
        )
        .await
        .is_err());
}

async fn live_credit_window(fixture: &mut Fixture, uri: &str) -> (uuid::Uuid, String) {
    for sql in [
        r#"INSERT INTO "MediaItem" ("Id","LibraryPathId","State") VALUES (31,1,0)"#,
        r#"INSERT INTO "Artist" ("Id") VALUES (31)"#,
        r#"INSERT INTO "ArtistMetadata" ("Id","ArtistId","Title","MetadataKind","DateAdded","DateUpdated") VALUES (32,31,'Synthetic artist',0,'2026-09-30','2026-09-30')"#,
        r#"INSERT INTO "MusicVideo" ("Id","ArtistId") VALUES (10,31)"#,
        r#"INSERT INTO "MusicVideoMetadata" ("Id","MusicVideoId","Title","MetadataKind","DateAdded","DateUpdated") VALUES (33,10,'Synthetic credits',0,'2026-09-30','2026-09-30')"#,
        r#"UPDATE "MediaVersion" SET "MusicVideoId"=10 WHERE "Id"=100"#,
    ] {
        sqlx::query(sql).execute(&fixture.host.pool).await.unwrap();
    }
    sqlx::query(r#"UPDATE "RemoteStream" SET "Url"=?,"Liveness"=2,"IsLive"=1 WHERE "Id"=10"#)
        .bind(uri)
        .execute(&fixture.host.pool)
        .await
        .unwrap();
    fixture.channel.subtitle_mode = ersatzrs_domain::ChannelSubtitleMode::Default;
    fixture.channel.music_video_credits_mode =
        ersatzrs_domain::ChannelMusicVideoCreditsMode::GenerateSubtitles;
    let at = Utc::now() - chrono::Duration::days(2);
    fixture.media.item.playout_id = 1;
    fixture.media.item.start = at;
    fixture.media.item.finish = at + chrono::Duration::seconds(30);
    fixture.media.item.in_point = chrono::TimeDelta::zero();
    fixture.media.item.out_point = chrono::TimeDelta::seconds(30);
    fixture.media.item.pinned_stream_selection = None;
    crate::repositories::make_repositories(fixture.host.pool.clone())
        .playout
        .upsert_item(&fixture.media.item)
        .await
        .unwrap();
    let before =
        super::tests::persisted_window_state(&fixture.host.pool, at, fixture.media.item.finish)
            .await;
    let path = fixture.root.path().join("owned-credits.ass");
    tokio::fs::write(&path, "[Events]\nFormat: Layer, Start, End, Style, Text\nDialogue: 0,0:00:00.00,0:00:01.00,Default,EXPIRED\nDialogue: 0,0:00:02.00,0:00:05.00,Default,ACTIVE\n").await.unwrap();
    let occurrences = [TroubleshootingChannelOccurrence {
        media: fixture.media.clone(),
        channel: fixture.channel.clone(),
        subtitle_id: None,
        credits: Some(protocol::Track {
            stream_index: Some(0),
            source: Some(protocol::Source::Local {
                path: path.to_str().unwrap().into(),
                in_point_ms: Some(0),
                out_point_ms: None,
                probe_hint: None,
            }),
        }),
    }];
    let deadline = Instant::now() + std::time::Duration::from_secs(10);
    let start = super::super::host::NextStart {
        channel_id: fixture.channel.id,
        preview: true,
        number: fixture.channel.number.clone(),
        executable: "unused-next".into(),
        output: fixture.root.path().join("output"),
        ffmpeg: "unused-ffmpeg".into(),
        ffprobe: "unused-ffprobe".into(),
        replacements: vec![],
        idle_timeout: None,
        deadline,
    };
    let (instance, playout) = fixture
        .host
        .prepare_troubleshooting_window(
            TroubleshootingChannelWindow {
                channel: &fixture.channel,
                profile: &fixture.profile,
                occurrences: &occurrences,
                start: at,
                finish: fixture.media.item.finish,
                replacements: &[],
                ffmpeg: Path::new("unused-ffmpeg"),
                deadline,
                diagnostics: None,
            },
            start,
        )
        .await
        .unwrap();
    assert_eq!(
        before,
        super::tests::persisted_window_state(&fixture.host.pool, at, fixture.media.item.finish)
            .await,
        "live credit window preparation must preserve scheduled rows and their exact state"
    );
    let Some(protocol::Source::Dynamic { uri, .. }) = &playout.items[0].source else {
        panic!("dynamic live credits");
    };
    let parts: Vec<_> = uri.split('/').collect();
    (instance, parts[parts.len() - 2].to_owned())
}

#[tokio::test]
async fn troubleshooting_live_http_and_rtsp_credits_rebase_join_without_changing_publication() {
    for uri in ["https://example.test/live", "rtsp://example.test/live"] {
        let mut fixture = Fixture::new().await;
        let (instance, digest) = live_credit_window(&mut fixture, uri).await;
        let joined = fixture.media.item.start + chrono::Duration::seconds(2);
        let resolved = fixture
            .host
            .resolve_dynamic(
                fixture.channel.id,
                fixture.media.item.id,
                &digest,
                Some(2),
                &fixture.channel.number,
                instance,
                joined,
                fixture.media.item.finish,
            )
            .await
            .unwrap();
        assert!(super::super::live_subtitles::is_live(&resolved));
        let tracks = resolved.tracks.unwrap();
        assert_eq!(tracks.audio.unwrap().stream_index, Some(2));
        let Some(protocol::Source::Local {
            path,
            in_point_ms: Some(0),
            ..
        }) = tracks.subtitle.unwrap().source
        else {
            panic!("rebased generated credits");
        };
        let text = tokio::fs::read_to_string(path).await.unwrap();
        assert!(!text.contains("EXPIRED"));
        assert!(text.contains("Dialogue: 0,0:00:00.00,0:00:03.00,Default,ACTIVE"));
        // A second callback retains the immutable original digest, without rebasing twice.
        fixture
            .host
            .resolve_dynamic(
                fixture.channel.id,
                fixture.media.item.id,
                &digest,
                Some(2),
                &fixture.channel.number,
                instance,
                joined,
                fixture.media.item.finish,
            )
            .await
            .unwrap();
        fixture
            .host
            .finish_troubleshooting_session(instance)
            .await
            .unwrap();
        assert!(fixture
            .host
            .resolve_dynamic(
                fixture.channel.id,
                fixture.media.item.id,
                &digest,
                Some(2),
                &fixture.channel.number,
                instance,
                joined,
                fixture.media.item.finish
            )
            .await
            .is_err());
    }
}
