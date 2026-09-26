use super::*;
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
            replacements: &[],
            ffmpeg: Path::new("unused-ffmpeg"),
            deadline,
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
