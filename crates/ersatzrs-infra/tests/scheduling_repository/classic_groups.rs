//! Actual mixed-source grouping, cardinality and independent persisted source cursors.
use super::*;
use ersatzrs_core::{
    playout_builder::PlayoutBuilder,
    repositories::PlayoutRepository,
    scheduling::{PlayoutBuildMode, SchedulingReferenceData, SchedulingResult},
};
use ersatzrs_domain::FillWithGroupMode;

async fn database(url: &str, order: PlaybackOrder, fill: FillWithGroupMode) -> SqlitePool {
    let pool = ersatzrs_infra::db::init(url).await.unwrap();
    pool.execute(r#"INSERT INTO "Resolution" ("Id", "Name", "Width", "Height") VALUES (1, 'test', 1920, 1080)"#).await.unwrap();
    seed_scheduling_fixture(&pool).await;
    pool.execute(r#"INSERT INTO "Collection" ("Id", "Name", "UseCustomPlaybackOrder") VALUES (99, 'groups', 0);
        UPDATE "MediaVersion" SET "Duration"='00:10:00' WHERE "Id" IN (200,210);"#).await.unwrap();
    for id in [30, 31, 40, 41, 42, 43, 44, 50, 60] {
        common::seed_media_item(&pool, id, 1).await;
    }
    for id in [30, 31] {
        sqlx::query(r#"INSERT INTO "OtherVideo" ("Id") VALUES (?)"#)
            .bind(id)
            .execute(&pool)
            .await
            .unwrap();
        insert_version(&pool, id, 600_000, "OtherVideoId").await;
    }
    for id in 40..=44 {
        sqlx::query(r#"INSERT INTO "Song" ("Id") VALUES (?)"#)
            .bind(id)
            .execute(&pool)
            .await
            .unwrap();
        insert_version(&pool, id, 600_000, "SongId").await;
    }
    for (id, song, artists) in [
        (400, 40, r#"["Alpha","Alias"]"#),
        (401, 40, r#"["Ignored alternate"]"#),
        (410, 41, "[]"),
        (411, 41, r#"["Alpha"]"#),
        (420, 42, r#"["Beta"]"#),
        (430, 43, r#"["Beta"]"#),
    ] {
        sqlx::query(r#"INSERT INTO "SongMetadata" ("Id", "SongId", "MetadataKind", "AlbumArtists", "DateAdded", "DateUpdated") VALUES (?, ?, 0, ?, '2026-11-11', '2026-11-11')"#).bind(id).bind(song).bind(artists).execute(&pool).await.unwrap();
    }
    pool.execute(r#"INSERT INTO "Image" ("Id") VALUES (50);
        INSERT INTO "ImageMetadata" ("Id", "ImageId", "MetadataKind", "DateAdded", "DateUpdated", "DurationSeconds") VALUES (500,50,0,'2026-11-11','2026-11-11',600);
        INSERT INTO "RemoteStream" ("Id", "Duration", "Liveness", "Url") VALUES (60, '00:10:00', 1, 'https://example.test/finite');"#).await.unwrap();
    insert_version(&pool, 50, 600_000, "ImageId").await;
    for id in [20, 21, 30, 31, 40, 41, 42, 43, 44, 50, 60] {
        sqlx::query(
            r#"INSERT INTO "CollectionItem" ("CollectionId", "MediaItemId") VALUES (99, ?)"#,
        )
        .bind(id)
        .execute(&pool)
        .await
        .unwrap();
    }
    let repos = make_repositories(pool.clone());
    common::seed_profile_and_channel(&repos, 901).await;
    repos
        .program_schedule
        .upsert(&ProgramSchedule {
            id: 301,
            name: "groups".into(),
            ..ProgramSchedule::default()
        })
        .await
        .unwrap();
    repos
        .program_schedule
        .upsert_item(&ProgramScheduleItem {
            id: 401,
            program_schedule_id: 301,
            collection_type: CollectionType::Collection,
            collection_id: Some(99),
            playout_mode: PlayoutMode::Multiple,
            multiple_count: Some("1".into()),
            playback_order: order,
            fill_with_group_mode: fill,
            preferred_audio_language_code: Some("nld".into()),
            preferred_subtitle_language_code: Some("eng".into()),
            ..ProgramScheduleItem::default()
        })
        .await
        .unwrap();
    pool.execute(r#"INSERT INTO "Playout" ("Id", "ChannelId", "ProgramScheduleId", "ScheduleKind", "Seed") VALUES (901, 901, 301, 'Classic', 123)"#).await.unwrap();
    pool
}

async fn reference(pool: &SqlitePool, start: chrono::DateTime<Utc>) -> SchedulingReferenceData {
    make_repositories(pool.clone())
        .scheduling
        .classic_reference_data(901, start, Duration::zero())
        .await
        .unwrap()
}

async fn build(
    pool: &SqlitePool,
    start: chrono::DateTime<Utc>,
    finish: chrono::DateTime<Utc>,
    mode: PlayoutBuildMode,
) -> (Playout, SchedulingResult) {
    let repos = make_repositories(pool.clone());
    let playout = repos.playout.get_with_anchor(901).await.unwrap();
    let result = PlayoutBuilder::default()
        .build(SchedulingRequest {
            playout: playout.clone(),
            start,
            finish,
            mode,
            reference_data: reference(pool, start).await,
        })
        .unwrap();
    (playout, result)
}

#[tokio::test]
async fn classic_fill_groups_sql_membership_is_distinct_by_type_and_first_album_artist() {
    let pool = database(
        "sqlite::memory:",
        PlaybackOrder::Chronological,
        FillWithGroupMode::FillWithOrderedGroups,
    )
    .await;
    let start = Utc.with_ymd_and_hms(2026, 11, 11, 23, 10, 0).unwrap();
    let data = reference(&pool, start).await;
    let groups = &data.classic_fill_groups[&401];
    assert_eq!(groups.len(), 5);
    assert_eq!(
        groups
            .iter()
            .map(|g| g.media_items.iter().map(|m| m.id).collect::<Vec<_>>())
            .collect::<Vec<_>>(),
        [
            vec![40, 41],
            vec![42, 43],
            vec![44],
            vec![20, 21],
            vec![30, 31]
        ]
    );
    let keys = groups
        .iter()
        .map(|g| serde_json::to_string(&g.collection_key).unwrap())
        .collect::<BTreeSet<_>>();
    assert_eq!(keys.len(), 5);
    let (_, result) = build(
        &pool,
        start,
        start + Duration::minutes(100),
        PlayoutBuildMode::Continue,
    )
    .await;
    assert_eq!(
        result
            .added_items
            .iter()
            .map(|i| i.media_item_id)
            .collect::<Vec<_>>(),
        [40, 42, 44, 20, 30, 41, 43, 44, 21, 31]
    );
    assert!(result
        .added_items
        .iter()
        .all(
            |i| i.preferred_audio_language_code.as_deref() == Some("nld")
                && i.preferred_subtitle_language_code.as_deref() == Some("eng")
        ));
}

#[tokio::test]
async fn classic_fill_groups_sql_restart_continue_and_refresh_preserve_each_group() {
    for order in [
        PlaybackOrder::Chronological,
        PlaybackOrder::Shuffle,
        PlaybackOrder::Random,
        PlaybackOrder::RandomRotation,
    ] {
        for fill in [
            FillWithGroupMode::FillWithOrderedGroups,
            FillWithGroupMode::FillWithShuffledGroups,
        ] {
            for mode in [PlayoutBuildMode::Continue, PlayoutBuildMode::Refresh] {
                let dir = tempfile::tempdir().unwrap();
                let url = format!("sqlite://{}", dir.path().join("groups.db").display());
                let pool = database(&url, order, fill).await;
                let start = Utc.with_ymd_and_hms(2026, 11, 11, 23, 10, 0).unwrap();
                let seam = start + Duration::minutes(50);
                let finish = start + Duration::minutes(100);
                let (_, full) = build(&pool, start, finish, PlayoutBuildMode::Continue).await;
                let (playout, first) = build(&pool, start, seam, PlayoutBuildMode::Continue).await;
                make_repositories(pool.clone())
                    .playout
                    .apply_build_result(&first, &playout, Duration::zero())
                    .await
                    .unwrap();
                pool.close().await;
                let pool = ersatzrs_infra::db::init(&url).await.unwrap();
                let (playout, resumed) = build(&pool, seam, finish, mode).await;
                assert_eq!(
                    resumed.added_items,
                    full.added_items[first.added_items.len()..],
                    "{order:?}, {fill:?}, {mode:?}"
                );
                assert_eq!(resumed.anchor, full.anchor);
                make_repositories(pool.clone())
                    .playout
                    .apply_build_result(&resumed, &playout, Duration::zero())
                    .await
                    .unwrap();
                let stored = reference(&pool, seam).await;
                assert!(
                    ersatzrs_core::scheduling::audit_classic_runtime_state(&stored).is_complete()
                );
                assert!(ersatzrs_core::scheduling::audit_classic_refresh_checkpoint(
                    &stored, 901, seam
                )
                .is_complete());
                assert_eq!(
                    stored
                        .existing_program_schedule_anchors
                        .iter()
                        .filter(|a| a.anchor_date.is_none())
                        .count(),
                    6
                );
                assert_eq!(stored.existing_fill_group_indices.len(), 1);
                assert_eq!(stored.existing_items.len(), full.added_items.len());
            }
        }
    }
}

#[tokio::test]
async fn classic_fill_groups_sql_ungroupable_images_and_streams_use_original_collection() {
    for mode in [PlayoutBuildMode::Continue, PlayoutBuildMode::Refresh] {
        let directory = tempfile::tempdir().unwrap();
        let url = format!(
            "sqlite://{}",
            directory.path().join("ungroupable.db").display()
        );
        let pool = database(
            &url,
            PlaybackOrder::None,
            FillWithGroupMode::FillWithOrderedGroups,
        )
        .await;
        pool.execute(r#"DELETE FROM "CollectionItem" WHERE "CollectionId"=99 AND "MediaItemId" NOT IN (50,60)"#).await.unwrap();
        let start = Utc.with_ymd_and_hms(2026, 11, 11, 23, 40, 0).unwrap();
        let seam = start + Duration::minutes(20);
        let finish = start + Duration::minutes(40);
        let data = reference(&pool, start).await;
        assert!(data.classic_fill_groups[&401].is_empty());
        let (_, full) = build(&pool, start, finish, PlayoutBuildMode::Continue).await;
        assert_eq!(
            full.added_items
                .iter()
                .map(|i| i.media_item_id)
                .collect::<Vec<_>>(),
            [50, 60, 50, 60]
        );
        assert!(full.fill_group_indices.is_empty());
        let (playout, first) = build(&pool, start, seam, PlayoutBuildMode::Continue).await;
        make_repositories(pool.clone())
            .playout
            .apply_build_result(&first, &playout, Duration::zero())
            .await
            .unwrap();
        pool.close().await;
        let pool = ersatzrs_infra::db::init(&url).await.unwrap();
        let (playout, resumed) = build(&pool, seam, finish, mode).await;
        assert_eq!(resumed.added_items, full.added_items[2..]);
        assert!(resumed.fill_group_indices.is_empty());
        make_repositories(pool.clone())
            .playout
            .apply_build_result(&resumed, &playout, Duration::zero())
            .await
            .unwrap();
        let stored = reference(&pool, seam).await;
        assert!(ersatzrs_core::scheduling::audit_classic_runtime_state(&stored).is_complete());
        assert!(
            ersatzrs_core::scheduling::audit_classic_refresh_checkpoint(&stored, 901, seam)
                .is_complete()
        );
    }
}

#[tokio::test]
async fn classic_fill_groups_duplicate_sources_keep_item_policies_and_independent_anchors() {
    let directory = tempfile::tempdir().unwrap();
    let url = format!(
        "sqlite://{}",
        directory.path().join("duplicate-groups.db").display()
    );
    let pool = database(
        &url,
        PlaybackOrder::Chronological,
        FillWithGroupMode::FillWithOrderedGroups,
    )
    .await;
    let repos = make_repositories(pool.clone());
    let mut second = repos.program_schedule.get_item(401).await.unwrap();
    second.id = 402;
    second.sort_order = 1;
    second.preferred_audio_language_code = Some("deu".into());
    second.preferred_subtitle_language_code = Some("nld".into());
    repos.program_schedule.upsert_item(&second).await.unwrap();
    let start = Utc.with_ymd_and_hms(2026, 11, 11, 23, 10, 0).unwrap();
    let seam = start + Duration::minutes(50);
    let finish = start + Duration::minutes(150);
    let (_, full) = build(&pool, start, finish, PlayoutBuildMode::Continue).await;
    assert_eq!(
        full.added_items[..4]
            .iter()
            .map(|i| i.media_item_id)
            .collect::<Vec<_>>(),
        [40, 40, 42, 42]
    );
    assert_eq!(
        full.added_items[0].preferred_audio_language_code.as_deref(),
        Some("nld")
    );
    assert_eq!(
        full.added_items[1].preferred_audio_language_code.as_deref(),
        Some("deu")
    );
    let data = reference(&pool, start).await;
    let first_keys = data.classic_fill_groups[&401]
        .iter()
        .map(|g| serde_json::to_string(&g.collection_key).unwrap())
        .collect::<BTreeSet<_>>();
    let second_keys = data.classic_fill_groups[&402]
        .iter()
        .map(|g| serde_json::to_string(&g.collection_key).unwrap())
        .collect::<BTreeSet<_>>();
    assert!(first_keys.is_disjoint(&second_keys));
    let (playout, first) = build(&pool, start, seam, PlayoutBuildMode::Continue).await;
    repos
        .playout
        .apply_build_result(&first, &playout, Duration::zero())
        .await
        .unwrap();
    pool.close().await;
    let pool = ersatzrs_infra::db::init(&url).await.unwrap();
    let (playout, resumed) = build(&pool, seam, finish, PlayoutBuildMode::Continue).await;
    assert_eq!(
        resumed.added_items,
        full.added_items[first.added_items.len()..]
    );
    make_repositories(pool.clone())
        .playout
        .apply_build_result(&resumed, &playout, Duration::zero())
        .await
        .unwrap();
    let stored = reference(&pool, seam).await;
    assert_eq!(stored.existing_fill_group_indices.len(), 2);
    assert_eq!(
        stored
            .existing_program_schedule_anchors
            .iter()
            .filter(|a| a.anchor_date.is_none())
            .count(),
        11
    );
    assert!(ersatzrs_core::scheduling::audit_classic_runtime_state(&stored).is_complete());
}

#[tokio::test]
async fn classic_fill_groups_missing_dated_group_refuses_refresh_without_clearing_sql() {
    use ersatzrs_core::{locking::EntityLocker, scheduling::PlayoutBuildRequest};
    let pool = database(
        "sqlite::memory:",
        PlaybackOrder::Chronological,
        FillWithGroupMode::FillWithOrderedGroups,
    )
    .await;
    let start = Utc.with_ymd_and_hms(2026, 11, 11, 23, 10, 0).unwrap();
    let seam = start + Duration::minutes(50);
    let (playout, first) = build(&pool, start, seam, PlayoutBuildMode::Continue).await;
    make_repositories(pool.clone())
        .playout
        .apply_build_result(&first, &playout, Duration::zero())
        .await
        .unwrap();
    pool.execute(r#"DELETE FROM "PlayoutProgramScheduleAnchor" WHERE "PlayoutId"=901 AND "AnchorDate" IS NOT NULL AND "FakeCollectionKey" LIKE '%:other'"#).await.unwrap();
    let before = reference(&pool, seam).await;
    assert!(ersatzrs_core::scheduling::audit_classic_runtime_state(&before).is_complete());
    assert!(
        !ersatzrs_core::scheduling::audit_classic_refresh_checkpoint(&before, 901, seam)
            .is_complete()
    );
    let error = ersatzrs_infra::maintenance::run_requested_playout_build(
        &pool,
        &EntityLocker::new(),
        PlayoutBuildRequest::new(901, PlayoutBuildMode::Refresh),
        seam,
        None,
    )
    .await
    .unwrap_err();
    assert!(
        error.to_string().contains("checkpoint") || error.to_string().contains("anchors"),
        "{error}"
    );
    let after = reference(&pool, seam).await;
    assert_eq!(after.existing_items, before.existing_items);
    assert_eq!(after.existing_history, before.existing_history);
    assert_eq!(
        after.existing_program_schedule_anchors,
        before.existing_program_schedule_anchors
    );
    assert_eq!(
        after.existing_classic_checkpoints,
        before.existing_classic_checkpoints
    );
    assert_eq!(
        after.existing_fill_group_indices,
        before.existing_fill_group_indices
    );
}

#[tokio::test]
async fn classic_fill_groups_show_and_music_video_artist_keys_preserve_source_types() {
    let pool = database(
        "sqlite::memory:",
        PlaybackOrder::Chronological,
        FillWithGroupMode::FillWithOrderedGroups,
    )
    .await;
    for id in [70, 71, 80] {
        common::seed_media_item(&pool, id, 1).await;
    }
    pool.execute(r#"INSERT INTO "Artist" ("Id") VALUES (80);
        INSERT INTO "MusicVideo" ("Id", "ArtistId") VALUES (70,80),(71,80);
        INSERT INTO "CollectionItem" ("CollectionId", "MediaItemId") VALUES (99,12),(99,13),(99,70),(99,71);
        UPDATE "MediaVersion" SET "Duration"='00:10:00' WHERE "Id" IN (120,130);"#).await.unwrap();
    for id in [70, 71] {
        insert_version(&pool, id, 600_000, "MusicVideoId").await;
    }
    let start = Utc.with_ymd_and_hms(2026, 11, 11, 0, 0, 0).unwrap();
    let data = reference(&pool, start).await;
    let groups = &data.classic_fill_groups[&401];
    assert_eq!(groups.len(), 7);
    assert_eq!(
        (
            groups[0].collection_key.collection_type,
            groups[0].collection_key.media_item_id
        ),
        (CollectionType::TelevisionShow, Some(10))
    );
    assert_eq!(
        (
            groups[1].collection_key.collection_type,
            groups[1].collection_key.media_item_id
        ),
        (CollectionType::Artist, Some(80))
    );
    assert_ne!(groups[0].collection_key, groups[1].collection_key);
    let (_, result) = build(
        &pool,
        start,
        start + Duration::minutes(70),
        PlayoutBuildMode::Continue,
    )
    .await;
    assert_eq!(
        result
            .added_items
            .iter()
            .map(|i| i.media_item_id)
            .collect::<Vec<_>>(),
        [12, 70, 40, 42, 44, 20, 30]
    );
}
