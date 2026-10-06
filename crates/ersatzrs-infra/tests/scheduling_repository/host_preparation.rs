//! The full host projector must work while new item IDs are still private.

use super::{
    next_preparation::{bind_fixture_files, project},
    scripted_refresh::published_fixture,
    scripted_sources::build,
    *,
};
use ersatzrs_core::{
    locking::{EntityLocker, PlayoutLockGuard},
    repositories::{ChannelRepository, PlayoutRepository},
    scheduling::PlayoutBuildMode,
};
use ersatzrs_domain::StreamingEngine;
use ersatzrs_infra::{
    next_engine::{coordinator::ChannelPublisher, host::NextHost, protocol, publication},
    repositories::SqlitePlayoutRepository,
};
use std::{sync::Arc, time::Instant};

#[tokio::test]
async fn full_host_preparation_preserves_selected_tracks_and_canvas_before_commit() {
    let root = tempfile::tempdir().unwrap();
    let url = format!("sqlite://{}", root.path().join("host.db").display());
    let (pool, _, first) = published_fixture(&url, &root.path().join("schedule.lua")).await;
    bind_fixture_files(&pool).await;
    seed_tracks(&pool).await;
    let repos = make_repositories(pool.clone());
    let mut channel = repos.channel.get_with_profile(903).await.unwrap();
    channel.streaming_engine = StreamingEngine::Next;
    repos.channel.upsert(&channel).await.unwrap();
    let repository = SqlitePlayoutRepository(pool.clone());
    let playout = repository.get_with_anchor(903).await.unwrap();
    let start = first.added_items[0].start;
    let finish = start + Duration::hours(6);
    let result = build(
        &pool,
        playout.clone(),
        start + Duration::seconds(1),
        finish,
        PlayoutBuildMode::Refresh,
    )
    .await;
    let locker = EntityLocker::new();
    ChannelPublisher::new(pool.clone(), locker.clone(), root.path(), 903)
        .refresh(start, finish, project)
        .await
        .unwrap();
    let path = root.path().join("903").join(publication::PLAYOUT_FILENAME);
    let previous = std::fs::read(&path).unwrap();
    let host = Arc::new(NextHost::new(
        pool.clone(),
        locker.clone(),
        root.path().into(),
    ));
    host.media
        .configure("127.0.0.1:8409".parse().unwrap())
        .unwrap();
    let _guard = PlayoutLockGuard::try_acquire(&locker, 903).unwrap();
    for commit in [false, true] {
        let mut pending = repository
            .prepare_build_result(&result, &playout, Duration::zero())
            .await
            .unwrap();
        let candidate = pending.items_in_range(start, finish).await.unwrap();
        let prepared = host
            .prepare_build(
                &mut pending,
                903,
                start..finish,
                &[],
                Instant::now() + std::time::Duration::from_secs(10),
            )
            .await
            .unwrap()
            .unwrap();
        assert_eq!(std::fs::read(&path).unwrap(), previous);
        if commit {
            pending.commit().await.unwrap();
            prepared.publish().await.unwrap();
            let published: protocol::Playout =
                serde_json::from_slice(&std::fs::read(&path).unwrap()).unwrap();
            assert_eq!(published.items.len(), candidate.len());
            for (wire, item) in published.items.iter().zip(candidate) {
                assert_eq!(
                    (&wire.id, wire.start, wire.finish),
                    (&item.id.to_string(), item.start, item.finish)
                );
                let tracks = wire.tracks.as_ref().unwrap();
                assert_eq!(tracks.audio.as_ref().unwrap().stream_index, Some(2));
                assert_eq!(tracks.subtitle.as_ref().unwrap().stream_index, Some(3));
                assert_eq!(wire.graphics.len(), 1);
                assert!(matches!(
                    wire.graphics[0].kind,
                    Some(protocol::GraphicsLayerKind::Canvas)
                ));
            }
        } else {
            drop(prepared);
            drop(pending);
            ersatzrs_infra::db::begin_write(&pool)
                .await
                .unwrap()
                .rollback()
                .await
                .unwrap();
            assert_eq!(std::fs::read(&path).unwrap(), previous);
        }
        assert_eq!(
            std::fs::read_dir(root.path().join("903")).unwrap().count(),
            1
        );
    }
}

async fn seed_tracks(pool: &SqlitePool) {
    pool.execute(r#"
        UPDATE "MediaVersion" SET "VideoScanKind" = 0;
        UPDATE "ChannelWatermark" SET "ImageSource" = 1;
        INSERT INTO "Artwork" ("Id", "ArtworkKind", "ChannelId", "Path", "DateAdded", "DateUpdated")
        VALUES (950, 2, 903, 'https://example.test/logo.png', '2026-04-15T00:00:00Z', '2026-04-15T00:00:00Z');
        DELETE FROM "MediaStream";
        INSERT INTO "MediaStream" ("Index", "Codec", "MediaStreamKind", "Language", "Channels", "Default", "Forced", "MediaVersionId")
        SELECT 0, 'h264', 1, NULL, 0, 1, 0, "Id" FROM "MediaVersion"
        UNION ALL SELECT 1, 'aac', 2, 'eng', 2, 1, 0, "Id" FROM "MediaVersion"
        UNION ALL SELECT 2, 'aac', 2, 'nld', 2, 0, 0, "Id" FROM "MediaVersion"
        UNION ALL SELECT 3, 'hdmv_pgs_subtitle', 3, 'eng', 0, 0, 0, "Id" FROM "MediaVersion";
        DELETE FROM "Subtitle";
        INSERT INTO "Subtitle" ("MovieMetadataId", "EpisodeMetadataId", "SubtitleKind", "StreamIndex", "Codec", "Default", "Forced", "SDH", "Language", "IsExtracted", "DateAdded", "DateUpdated")
        SELECT "Id", NULL, 0, 3, 'hdmv_pgs_subtitle', 0, 0, 0, 'eng', 0, '2026-04-15T00:00:00Z', '2026-04-15T00:00:00Z' FROM "MovieMetadata"
        UNION ALL SELECT NULL, "Id", 0, 3, 'hdmv_pgs_subtitle', 0, 0, 0, 'eng', 0, '2026-04-15T00:00:00Z', '2026-04-15T00:00:00Z' FROM "EpisodeMetadata";
    "#).await.unwrap();
}
