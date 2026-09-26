use super::*;
use chrono::{Duration, TimeZone};
use ersatzrs_core::repositories::{ChannelRepository, FfmpegProfileRepository};
use ersatzrs_domain::{ChannelPlayoutSource, FFmpegProfile, PinnedStreamSelection};

pub(crate) async fn fixture() -> (DbPool, EntityLocker, Channel, DateTime<Utc>) {
    let pool = super::super::selection::tests::fixture_pool().await;
    fixture_with_pool(pool).await
}

pub(crate) async fn fixture_with_pool(
    pool: DbPool,
) -> (DbPool, EntityLocker, Channel, DateTime<Utc>) {
    sqlx::query(r#"INSERT INTO "Resolution" ("Id", "Name", "Width", "Height") VALUES (1, 'Fixture', 320, 180)"#)
        .execute(&pool).await.unwrap();
    let repos = crate::repositories::make_repositories(pool.clone());
    repos
        .ffmpeg_profile
        .upsert(&FFmpegProfile {
            id: 1,
            resolution_id: 1,
            ..Default::default()
        })
        .await
        .unwrap();
    let channel = Channel {
        id: 1,
        number: "1".to_owned(),
        name: "Fixture".to_owned(),
        ffmpeg_profile_id: 1,
        ..Default::default()
    };
    repos.channel.upsert(&channel).await.unwrap();
    sqlx::query(
        r#"INSERT INTO "Playout" ("Id", "ChannelId", "ScheduleKind", "Seed") VALUES (1, 1, 0, 1)"#,
    )
    .execute(&pool)
    .await
    .unwrap();
    sqlx::query(
        r#"INSERT INTO "MediaFile" ("Id", "MediaVersionId", "Path", "PathHash")
        VALUES (2, 100, 'media/second.mkv', 'second'), (1, 100, 'media/first.mkv', 'first')"#,
    )
    .execute(&pool)
    .await
    .unwrap();
    let start = Utc.with_ymd_and_hms(2026, 9, 22, 12, 0, 0).unwrap();
    for (id, seconds) in [(1, 0), (2, 60)] {
        repos
            .playout
            .upsert_item(&PlayoutItem {
                id,
                playout_id: 1,
                media_item_id: 10,
                start: start + Duration::seconds(seconds),
                finish: start + Duration::seconds(seconds + 60),
                guide_start: Some(start + Duration::seconds(seconds)),
                guide_finish: Some(start + Duration::seconds(seconds + 60)),
                ..Default::default()
            })
            .await
            .unwrap();
    }
    (pool, EntityLocker::default(), channel, start)
}

#[tokio::test]
async fn multiple_files_and_versions_do_not_duplicate_scheduled_items() {
    let (pool, locker, channel, start) = fixture().await;
    sqlx::query(r#"INSERT INTO "MediaVersion" ("Id", "DateAdded", "DateUpdated", "Duration", "MovieId", "Height", "Width", "VideoScanKind")
        VALUES (101, '2026-09-22 00:00:00', '2026-09-22 00:00:00', '00:01:00', 10, 180, 320, 0)"#)
        .execute(&pool).await.unwrap();
    sqlx::query(r#"INSERT INTO "MediaFile" ("Id", "MediaVersionId", "Path", "PathHash") VALUES (3, 101, 'media/alternate.mkv', 'alternate')"#)
        .execute(&pool).await.unwrap();
    let read = read_locked(
        &pool,
        &locker,
        &channel,
        start,
        start + Duration::minutes(2),
    )
    .await
    .unwrap()
    .unwrap();
    assert_eq!(
        read.items.iter().map(|m| m.item.id).collect::<Vec<_>>(),
        vec![1, 2]
    );
    assert!(read
        .items
        .iter()
        .all(|m| m.media_version_id == 100 && m.media_file_id == 1 && m.path == "media/first.mkv"));
    assert!(PlayoutLockGuard::try_acquire(&locker, 1).is_none());
    drop(read);
    let repo = SqlitePlayoutRepository(pool.clone());
    let mut item = repo
        .items_in_range(1, start, start + Duration::minutes(1))
        .await
        .unwrap()
        .remove(0);
    item.pinned_stream_selection = Some(PinnedStreamSelection {
        media_version_id: 101,
        video_stream_index: 0,
        audio_stream_index: None,
        subtitle_stream_index: None,
    });
    repo.upsert_item(&item).await.unwrap();
    let read = read_locked(
        &pool,
        &locker,
        &channel,
        start,
        start + Duration::minutes(1),
    )
    .await
    .unwrap()
    .unwrap();
    assert_eq!(read.items.len(), 1);
    assert_eq!(read.items[0].media_version_id, 101);
    assert_eq!(read.items[0].media_file_id, 3);
    assert_eq!(read.items[0].path, "media/alternate.mkv");
}

#[tokio::test]
async fn identical_paths_preserve_the_selected_file_identity() {
    let (pool, locker, channel, start) = fixture().await;
    // Provider file keys can differ even when their stored paths are identical.
    sqlx::query(r#"UPDATE "MediaFile" SET "Path" = 'media/shared.mkv'"#)
        .execute(&pool)
        .await
        .unwrap();
    for expected_id in [1, 2] {
        let read = read_locked(
            &pool,
            &locker,
            &channel,
            start,
            start + Duration::minutes(2),
        )
        .await
        .unwrap()
        .unwrap();
        assert_eq!(read.items.len(), 2);
        assert!(read.items.iter().all(|media| {
            media.media_file_id == expected_id
                && media.media_version_id == 100
                && media.path == "media/shared.mkv"
        }));
        drop(read);
        sqlx::query(r#"DELETE FROM "MediaFile" WHERE "Id" = ?"#)
            .bind(expected_id)
            .execute(&pool)
            .await
            .unwrap();
    }
}

#[tokio::test]
async fn mirror_range_and_guide_times_follow_the_requested_channel() {
    let (pool, locker, channel, start) = fixture().await;
    for offset in [Duration::hours(1), Duration::hours(-2)] {
        let mirror = Channel {
            id: 2,
            playout_source: ChannelPlayoutSource::Mirror,
            mirror_source_channel_id: Some(channel.id),
            playout_offset: Some(offset),
            ..channel.clone()
        };
        let target_start = start + offset + Duration::minutes(1);
        let read = read_locked(
            &pool,
            &locker,
            &mirror,
            target_start,
            target_start + Duration::minutes(1),
        )
        .await
        .unwrap()
        .unwrap();
        assert_eq!(read.context.source_channel_id, 1);
        assert_eq!(read.context.requested_channel_id, 2);
        assert_eq!(read.items.len(), 1);
        let item = &read.items[0].item;
        assert_eq!(item.id, 2);
        assert_eq!(item.start, target_start);
        assert_eq!(item.finish, target_start + Duration::minutes(1));
        assert_eq!(item.guide_start, Some(item.start));
        assert_eq!(item.guide_finish, Some(item.finish));
    }
}

#[tokio::test]
async fn busy_reads_release_partial_locks_and_empty_reads_remain_distinct() {
    let (pool, locker, channel, start) = fixture().await;
    sqlx::query(
        r#"INSERT INTO "Playout" ("Id", "ChannelId", "ScheduleKind", "Seed") VALUES (2, 1, 0, 1)"#,
    )
    .execute(&pool)
    .await
    .unwrap();
    let held = PlayoutLockGuard::try_acquire(&locker, 2).unwrap();
    assert!(read_locked(
        &pool,
        &locker,
        &channel,
        start,
        start + Duration::minutes(2)
    )
    .await
    .unwrap()
    .is_none());
    assert!(PlayoutLockGuard::try_acquire(&locker, 1).is_some());
    drop(held);
    let empty = read_locked(
        &pool,
        &locker,
        &channel,
        start + Duration::days(1),
        start + Duration::days(2),
    )
    .await
    .unwrap()
    .unwrap();
    assert!(empty.items.is_empty());
    assert!(PlayoutLockGuard::try_acquire(&locker, 2).is_none());
    drop(empty);
    assert!(PlayoutLockGuard::try_acquire(&locker, 2).is_some());
}

#[tokio::test]
async fn missing_files_fail_the_complete_read_and_release_locks() {
    let (pool, locker, channel, start) = fixture().await;
    sqlx::query(r#"DELETE FROM "MediaFile""#)
        .execute(&pool)
        .await
        .unwrap();
    assert!(read_locked(
        &pool,
        &locker,
        &channel,
        start,
        start + Duration::minutes(2)
    )
    .await
    .is_err());
    assert!(PlayoutLockGuard::try_acquire(&locker, 1).is_some());
    assert!(read_locked(&pool, &locker, &channel, start, start)
        .await
        .is_err());
}

#[tokio::test]
async fn media_binding_uses_owner_and_file_indexes() {
    let (pool, _, _, _) = fixture().await;
    let mut explain = sqlx::QueryBuilder::<crate::db::Db>::new("EXPLAIN QUERY PLAN ");
    explain.push(MEDIA_QUERY);
    let mut query = explain.build();
    for _ in 0..7 {
        query = query.bind(10_i64);
    }
    let rows = query
        .bind(Option::<i64>::None)
        .bind(Option::<i64>::None)
        .fetch_all(&pool)
        .await
        .unwrap();
    let details = rows
        .iter()
        .map(|row| row.get::<String, _>("detail"))
        .collect::<Vec<_>>()
        .join("\n");
    assert!(details.contains("MULTI-INDEX OR"), "{details}");
    assert!(details.contains("IX_MediaFile_MediaVersionId"), "{details}");
    assert!(!details.contains("SCAN mv"), "{details}");
}
