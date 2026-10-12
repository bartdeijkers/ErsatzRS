use super::*;
use crate::{
    db::DbPool,
    repositories::make_repositories,
    xmltv_read::{GuideExpansionLimits, GuideReadLimits},
};
use chrono::TimeZone;
use ersatzrs_core::repositories::{ChannelRepository, FfmpegProfileRepository, PlayoutRepository};
use ersatzrs_domain::{Channel, FFmpegProfile, FillerKind, PlayoutItem};
use sqlx::Executor;

fn now() -> DateTime<Utc> {
    Utc.with_ymd_and_hms(2026, 4, 15, 10, 0, 0).unwrap()
}
fn budget() -> GuideReadBudget {
    GuideReadBudget::new_with_expansion(
        GuideReadLimits {
            max_records: 1000,
            max_source_bytes: 1024 * 1024,
        },
        GuideExpansionLimits {
            max_nodes: 100_000,
            max_bytes: 16 * 1024 * 1024,
        },
    )
}

async fn fixture() -> (tempfile::TempDir, DbPool) {
    let root = tempfile::tempdir().unwrap();
    let pool = crate::db::init(&format!(
        "sqlite:{}",
        root.path().join("guide.db").display()
    ))
    .await
    .unwrap();
    pool.execute(r#"INSERT OR IGNORE INTO "Resolution" ("Id","Name","Width","Height") VALUES (1,'Synthetic',320,180)"#).await.unwrap();
    let repos = make_repositories(pool.clone());
    repos
        .ffmpeg_profile
        .upsert(&FFmpegProfile {
            id: 1,
            name: "Synthetic".into(),
            resolution_id: 1,
            ..FFmpegProfile::default()
        })
        .await
        .unwrap();
    for id in [1, 2] {
        repos
            .channel
            .upsert(&Channel {
                id,
                unique_id: uuid::Uuid::from_u128(u128::try_from(id).unwrap()),
                number: id.to_string(),
                sort_number: f64::from(i32::try_from(id).unwrap()),
                name: format!("Synthetic {id}"),
                ffmpeg_profile_id: 1,
                playout_source: if id == 2 {
                    ChannelPlayoutSource::Mirror
                } else {
                    ChannelPlayoutSource::Generated
                },
                mirror_source_channel_id: (id == 2).then_some(1),
                playout_offset: (id == 2).then_some(Duration::hours(1)),
                ..Channel::default()
            })
            .await
            .unwrap();
    }
    for sql in [
        r#"INSERT INTO "MediaSource" ("Id") VALUES (10)"#,
        r#"INSERT INTO "LocalMediaSource" ("Id") VALUES (10)"#,
        r#"INSERT INTO "Library" ("Id","MediaKind","MediaSourceId","Name") VALUES (10,1,10,'Synthetic')"#,
        r#"INSERT INTO "LocalLibrary" ("Id") VALUES (10)"#,
        r#"INSERT INTO "LibraryPath" ("Id","LibraryId","Path") VALUES (10,10,'media')"#,
        r#"INSERT INTO "MediaItem" ("Id","LibraryPathId") VALUES (20,10)"#,
        r#"INSERT INTO "Movie" ("Id") VALUES (20)"#,
        r#"INSERT INTO "MovieMetadata" ("Id","MovieId","DateAdded","DateUpdated","MetadataKind","Title","Plot") VALUES (30,20,'2026-01-01','2026-01-01',0,'Original é','Plot')"#,
        r#"INSERT INTO "Playout" ("Id","ChannelId","ScheduleKind","Seed") VALUES (1,1,'Block',0)"#,
        r#"INSERT INTO "ConfigElement" ("Key","Value") VALUES ('pages.language','nld') ON CONFLICT("Key") DO UPDATE SET "Value"=excluded."Value""#,
        r#"INSERT INTO "ConfigElement" ("Key","Value") VALUES ('xmltv.block_behavior','SplitTimeEvenly') ON CONFLICT("Key") DO UPDATE SET "Value"=excluded."Value""#,
    ] {
        pool.execute(sql).await.unwrap();
    }
    (root, pool)
}

async fn segments(pool: &DbPool) {
    let repos = make_repositories(pool.clone());
    for (id, start, finish, filler, input, output, group) in [
        (31, -1200, -900, FillerKind::None, 0, 300, 7),
        (32, -900, -890, FillerKind::MidRoll, 0, 10, 7),
        (33, -890, 60, FillerKind::None, 300, 1250, 7),
        (34, 120, 180, FillerKind::None, 0, 60, 8),
    ] {
        let full_start = if group == 7 {
            now() - Duration::hours(20)
        } else {
            now() + Duration::hours(2)
        };
        let full_finish = if group == 7 {
            now() + Duration::hours(1)
        } else {
            now() + Duration::hours(3)
        };
        repos
            .playout
            .upsert_item(&PlayoutItem {
                id,
                playout_id: 1,
                media_item_id: 20,
                start: now() + Duration::minutes(start),
                finish: now() + Duration::minutes(finish),
                guide_start: Some(full_start),
                guide_finish: Some(full_finish),
                guide_group: group,
                filler_kind: filler,
                in_point: Duration::minutes(input),
                out_point: Duration::minutes(output),
                ..PlayoutItem::default()
            })
            .await
            .unwrap();
    }
}

async fn capture(
    pool: &DbPool,
    ids: &[i64],
    start: DateTime<Utc>,
    finish: DateTime<Utc>,
    limits: &mut GuideReadBudget,
) -> Result<GuideSnapshot, GuideSnapshotError> {
    // Read-only transaction pins guide source/configuration and all metadata reads.
    let mut transaction = pool.begin().await.unwrap();
    capture_guide_snapshot(&mut transaction, ids, start, finish, limits).await
}

async fn context(pool: &DbPool, ids: &[i64], limits: &mut GuideReadBudget) -> GuideSnapshot {
    capture(pool, ids, now(), now() + Duration::hours(4), limits)
        .await
        .unwrap()
}

#[tokio::test]
async fn context_comparison_ignores_usage_and_preserves_order_membership_and_window() {
    let (_root, pool) = fixture().await;
    segments(&pool).await;
    let mut limits = budget();
    let first = context(&pool, &[1, 2], &mut limits).await;
    let second = context(&pool, &[1, 2], &mut limits).await;
    assert_ne!(first.source_usage(), second.source_usage());
    assert_ne!(first.expansion_usage(), second.expansion_usage());
    assert_eq!(
        first.compare_context(&second),
        GuideSnapshotComparison::Unchanged
    );
    let reversed = context(&pool, &[2, 1], &mut budget()).await;
    assert_eq!(
        first.compare_context(&reversed),
        GuideSnapshotComparison::ChannelChanged(1)
    );
    let subset = context(&pool, &[1], &mut budget()).await;
    assert_eq!(
        first.compare_context(&subset),
        GuideSnapshotComparison::ChannelChanged(2)
    );
    assert_eq!(
        subset.compare_context(&first),
        GuideSnapshotComparison::ChannelChanged(2)
    );
    let shifted = capture(
        &pool,
        &[1, 2],
        now() + Duration::seconds(1),
        now() + Duration::hours(4),
        &mut budget(),
    )
    .await
    .unwrap();
    assert_eq!(
        first.compare_context(&shifted),
        GuideSnapshotComparison::GlobalChanged
    );
    assert_eq!(
        format!("{:?}", first.compare_context(&reversed)),
        "ChannelChanged(1)"
    );
}

#[tokio::test]
async fn context_comparison_locates_actual_channel_content_segments_and_source_changes() {
    let (_root, pool) = fixture().await;
    segments(&pool).await;
    let mut previous = context(&pool, &[1, 2], &mut budget()).await;
    for (sql, id) in [
        (
            r#"UPDATE "MovieMetadata" SET "Plot"='Changed plot' WHERE "Id"=30"#,
            1,
        ),
        (
            r#"UPDATE "PlayoutItem" SET "OutPoint"='06:00:00' WHERE "Id"=33"#,
            1,
        ),
        (
            r#"UPDATE "Channel" SET "Name"='Changed name' WHERE "Id"=1"#,
            1,
        ),
        (r#"UPDATE "Channel" SET "Number"='3' WHERE "Id"=1"#, 1),
        (
            r#"UPDATE "Channel" SET "PlayoutOffset"='02:00:00' WHERE "Id"=2"#,
            2,
        ),
        (
            r#"UPDATE "Channel" SET "MirrorSourceChannelId"=2 WHERE "Id"=2"#,
            2,
        ),
        (r#"UPDATE "Channel" SET "PlayoutSource"=0 WHERE "Id"=2"#, 2),
    ] {
        pool.execute(sql).await.unwrap();
        let next = context(&pool, &[1, 2], &mut budget()).await;
        assert_eq!(
            previous.compare_context(&next),
            GuideSnapshotComparison::ChannelChanged(id)
        );
        previous = next;
    }
}

#[tokio::test]
async fn context_comparison_retains_raw_configuration_and_accepted_revision_identity() {
    let (root, pool) = fixture().await;
    external_fixture(
        &pool,
        &root.path().join("accepted.json"),
        r#"{"startTime":"2026-04-15T10:00:00Z","programs":[{"title":"First","duration":60000}]}"#,
    )
    .await;
    let original = context(&pool, &[1], &mut budget()).await;
    pool.execute(r#"UPDATE "PlayoutExternalJsonSnapshot" SET "Revision"=8 WHERE "PlayoutId"=1"#)
        .await
        .unwrap();
    let revised = context(&pool, &[1], &mut budget()).await;
    assert_eq!(
        original.channel(1).unwrap().occurrences(),
        revised.channel(1).unwrap().occurrences()
    );
    assert_eq!(
        original.compare_context(&revised),
        GuideSnapshotComparison::ChannelChanged(1)
    );
    pool.execute(r#"UPDATE "ConfigElement" SET "Value"='splittimeevenly' WHERE "Key"='xmltv.block_behavior'"#).await.unwrap();
    let raw_changed = context(&pool, &[1], &mut budget()).await;
    assert_eq!(revised.block_behavior(), raw_changed.block_behavior());
    assert_eq!(
        revised.compare_context(&raw_changed),
        GuideSnapshotComparison::GlobalChanged
    );
    pool.execute(r#"UPDATE "ConfigElement" SET "Value"='deu' WHERE "Key"='pages.language'"#)
        .await
        .unwrap();
    let language_changed = context(&pool, &[1], &mut budget()).await;
    assert_eq!(
        raw_changed.compare_context(&language_changed),
        GuideSnapshotComparison::GlobalChanged
    );
}

#[tokio::test]
async fn path_associations_retain_distinct_configured_and_accepted_paths_for_mirrors() {
    let (root, pool) = fixture().await;
    external_fixture(
        &pool,
        &root.path().join("accepted-original.json"),
        r#"{"startTime":"2026-04-15T10:00:00Z","programs":[{"title":"First","duration":60000}]}"#,
    )
    .await;
    let original = context(&pool, &[1, 2], &mut budget()).await;
    let first = &original.channel(1).unwrap().path_associations()[0];
    let mirror = &original.channel(2).unwrap().path_associations()[0];
    assert_eq!(first, mirror);
    assert_eq!(first.playout_id(), 1);
    assert_eq!(
        first.configured_schedule_file(),
        first.accepted_schedule_file()
    );
    assert!(!format!("{first:?}").contains("accepted-original"));
    pool.execute(r#"UPDATE "Playout" SET "ScheduleFile"='configured-only.json' WHERE "Id"=1"#)
        .await
        .unwrap();
    let configured = context(&pool, &[1, 2], &mut budget()).await;
    assert_eq!(
        original.compare_context(&configured),
        GuideSnapshotComparison::ChannelChanged(1)
    );
    for id in [1, 2] {
        assert_eq!(
            original.channel(id).unwrap().sources(),
            configured.channel(id).unwrap().sources()
        );
        assert_eq!(
            original.channel(id).unwrap().occurrences(),
            configured.channel(id).unwrap().occurrences()
        );
        let path = &configured.channel(id).unwrap().path_associations()[0];
        assert_eq!(
            path.configured_schedule_file(),
            Some("configured-only.json")
        );
        assert_eq!(
            path.accepted_schedule_file(),
            first.accepted_schedule_file()
        );
    }
    pool.execute(r#"UPDATE "PlayoutExternalJsonSnapshot" SET "ScheduleFile"='accepted-only.json' WHERE "PlayoutId"=1"#).await.unwrap();
    let accepted = context(&pool, &[1, 2], &mut budget()).await;
    assert_eq!(
        configured.compare_context(&accepted),
        GuideSnapshotComparison::ChannelChanged(1)
    );
    for id in [1, 2] {
        assert_eq!(
            configured.channel(id).unwrap().sources(),
            accepted.channel(id).unwrap().sources()
        );
        assert_eq!(
            configured.channel(id).unwrap().occurrences(),
            accepted.channel(id).unwrap().occurrences()
        );
        let path = &accepted.channel(id).unwrap().path_associations()[0];
        assert_eq!(
            path.configured_schedule_file(),
            Some("configured-only.json")
        );
        assert_eq!(path.accepted_schedule_file(), Some("accepted-only.json"));
        assert!(!format!("{path:?}").contains("only.json"));
    }
}

#[tokio::test]
async fn path_associations_preserve_absent_and_empty_configured_sql_paths() {
    let (_root, pool) = fixture().await;
    segments(&pool).await;
    let absent = context(&pool, &[1], &mut budget()).await;
    let path = &absent.channel(1).unwrap().path_associations()[0];
    assert_eq!(path.configured_schedule_file(), None);
    assert_eq!(path.accepted_schedule_file(), None);
    pool.execute(r#"UPDATE "Playout" SET "ScheduleFile"='' WHERE "Id"=1"#)
        .await
        .unwrap();
    let empty = context(&pool, &[1], &mut budget()).await;
    let path = &empty.channel(1).unwrap().path_associations()[0];
    assert_eq!(path.configured_schedule_file(), Some(""));
    assert_eq!(path.accepted_schedule_file(), None);
    assert_eq!(
        absent.channel(1).unwrap().occurrences(),
        empty.channel(1).unwrap().occurrences()
    );
    assert_eq!(
        absent.compare_context(&empty),
        GuideSnapshotComparison::ChannelChanged(1)
    );
}

#[tokio::test]
async fn full_split_sources_mirrors_and_same_media_repeats_survive_before_half_open_window() {
    let (_root, pool) = fixture().await;
    segments(&pool).await;
    let snapshot = capture(
        &pool,
        &[1, 2],
        now(),
        now() + Duration::minutes(10),
        &mut budget(),
    )
    .await
    .unwrap();
    assert_eq!(snapshot.page_language(), Some("nld"));
    assert_eq!(snapshot.raw_block_behavior(), Some("SplitTimeEvenly"));
    for (id, offset) in [(1, Duration::zero()), (2, Duration::hours(1))] {
        let channel = snapshot.channel(id).unwrap();
        assert_eq!(channel.occurrences().len(), 1);
        assert_eq!(channel.source_channel_id(), 1);
        let first = &channel.occurrences()[0];
        assert_eq!(first.first_item_id, 31);
        assert_eq!(first.programme.start, now() - Duration::hours(20) + offset);
        assert_eq!(first.programme.stop, now() + Duration::hours(1) + offset);
        assert_eq!(first.programme.title, "Original é");
        assert_eq!(first.segments.len(), 2);
        assert_eq!(first.segments[1].in_point, Duration::hours(5));
        assert_eq!(first.segments[1].out_point, Duration::minutes(1250));
        assert_eq!(
            first.segments[1].start,
            now() - Duration::minutes(890) + offset
        );
        assert_eq!(
            channel.sources(),
            [GuideSnapshotSource::Sql {
                playout_id: 1,
                schedule_kind: PlayoutScheduleKind::Block,
                raw_rows: 4
            }]
        );
    }
    let full = capture(
        &pool,
        &[1],
        now() - Duration::days(1),
        now() + Duration::hours(4),
        &mut budget(),
    )
    .await
    .unwrap();
    let legacy = crate::xmltv::load_channel_programmes(
        &pool,
        1,
        now() - Duration::days(1),
        now() + Duration::hours(4),
    )
    .await
    .unwrap();
    assert_eq!(full.channel(1).unwrap().occurrences(), legacy);
    assert_eq!(legacy.len(), 2);
    assert_eq!(legacy[1].first_item_id, 34);
    assert_eq!(legacy[1].segments.len(), 1);
    let gap = capture(
        &pool,
        &[1],
        now() + Duration::hours(1),
        now() + Duration::hours(2),
        &mut budget(),
    )
    .await
    .unwrap();
    assert!(gap.channel(1).unwrap().occurrences().is_empty());
    assert!(!format!("{snapshot:?}").contains("Original"));
}

async fn update_pinned_sources(pool: &DbPool) {
    for sql in [
        r#"UPDATE "MovieMetadata" SET "Title"='Changed' WHERE "MovieId"=20"#,
        r#"UPDATE "Channel" SET "Name"='Changed',"Number"='5' WHERE "Id"=1"#,
        r#"UPDATE "ConfigElement" SET "Value"='deu' WHERE "Key"='pages.language'"#,
        r#"UPDATE "ConfigElement" SET "Value"='UseActualTimes' WHERE "Key"='xmltv.block_behavior'"#,
        r#"UPDATE "PlayoutItem" SET "CustomTitle"='Changed' WHERE "Id"=31"#,
    ] {
        pool.execute(sql).await.unwrap();
    }
}

#[tokio::test]
async fn whole_transaction_pins_rows_metadata_channel_and_raw_configuration() {
    let (_root, pool) = fixture().await;
    segments(&pool).await;
    // Read-only transaction pins guide source/configuration and all metadata reads.
    let mut transaction = pool.begin().await.unwrap();
    let first = capture_guide_snapshot(
        &mut transaction,
        &[1],
        now(),
        now() + Duration::hours(4),
        &mut budget(),
    )
    .await
    .unwrap();
    update_pinned_sources(&pool).await;
    let pinned = capture_guide_snapshot(
        &mut transaction,
        &[1],
        now(),
        now() + Duration::hours(4),
        &mut budget(),
    )
    .await
    .unwrap();
    assert_eq!(
        pinned.channel(1).unwrap().occurrences(),
        first.channel(1).unwrap().occurrences()
    );
    assert_eq!(pinned.channel(1).unwrap().name(), "Synthetic 1");
    assert_eq!(pinned.channel(1).unwrap().number(), "1");
    assert_eq!(pinned.page_language(), Some("nld"));
    assert_eq!(pinned.raw_block_behavior(), Some("SplitTimeEvenly"));
    transaction.rollback().await.unwrap();
    let changed = capture(
        &pool,
        &[1],
        now(),
        now() + Duration::hours(4),
        &mut budget(),
    )
    .await
    .unwrap();
    assert_eq!(changed.page_language(), Some("deu"));
    assert_eq!(changed.channel(1).unwrap().number(), "5");
    assert_eq!(
        changed.channel(1).unwrap().occurrences()[0].programme.title,
        "Changed"
    );
}

async fn external_fixture(pool: &DbPool, path: &std::path::Path, document: &str) {
    sqlx::query(r#"UPDATE "Playout" SET "ScheduleKind"=20,"ScheduleFile"=? WHERE "Id"=1"#)
        .bind(path.to_str().unwrap())
        .execute(pool)
        .await
        .unwrap();
    sqlx::query(r#"INSERT INTO "PlayoutExternalJsonSnapshot" ("PlayoutId","Revision","SchemaVersion","ScheduleFile","Document","PublishedAt") VALUES (1,7,1,?,?,'2026-04-15T10:00:00Z')"#)
        .bind(path.to_str().unwrap()).bind(document).execute(pool).await.unwrap();
}

#[tokio::test]
async fn external_capture_uses_exact_revision_counts_all_programs_and_never_reads_live_file() {
    let (root, pool) = fixture().await;
    let path = root.path().join("live.json");
    std::fs::write(&path, "{broken").unwrap();
    sqlx::query(r#"UPDATE "Playout" SET "ScheduleKind"=20,"ScheduleFile"=? WHERE "Id"=1"#)
        .bind(path.to_str().unwrap())
        .execute(&pool)
        .await
        .unwrap();
    let missing = capture(
        &pool,
        &[1],
        now(),
        now() + Duration::hours(2),
        &mut budget(),
    )
    .await
    .unwrap_err();
    assert_eq!(missing.reason, GuideSnapshotFailure::MissingAccepted);
    let document=serde_json::json!({"startTime":now().to_rfc3339(),"guideMinimumDurationSeconds":30,
        "programs":[{"title":"First","duration":60_000},{"title":"Short","duration":1000},{"type":"episode","showTitle":"Series","title":"Episode","duration":60_000}]}).to_string();
    external_fixture(&pool, &path, &document).await;
    let result = capture(
        &pool,
        &[1, 2],
        now(),
        now() + Duration::hours(2),
        &mut budget(),
    )
    .await
    .unwrap();
    for id in [1, 2] {
        let channel = result.channel(id).unwrap();
        assert_eq!(channel.occurrences().len(), 2);
        assert_eq!(
            channel.sources(),
            [GuideSnapshotSource::Accepted {
                playout_id: 1,
                revision: 7,
                raw_programs: 3
            }]
        );
        assert_eq!(channel.occurrences()[1].first_item_id, 2);
        assert_eq!(channel.occurrences()[1].programme.title, "Series");
    }
    pool.execute(r#"UPDATE "PlayoutExternalJsonSnapshot" SET "Document"='{broken'"#)
        .await
        .unwrap();
    let corrupt = capture(
        &pool,
        &[1],
        now(),
        now() + Duration::hours(2),
        &mut budget(),
    )
    .await
    .unwrap_err();
    assert!(matches!(corrupt.reason, GuideSnapshotFailure::Accepted(_)));
    assert!(!corrupt.to_string().contains("broken"));
}

#[tokio::test]
async fn accepted_parse_is_admitted_before_allocating_invalid_semantic_programs() {
    let (root, pool) = fixture().await;
    external_fixture(
        &pool,
        &root.path().join("accepted.json"),
        r#"{"startTime":"2026-04-15T10:00:00Z","programs":[{"duration":0}]}"#,
    )
    .await;
    let mut limited = GuideReadBudget::new_with_expansion(
        GuideReadLimits {
            max_records: 100,
            max_source_bytes: 100_000,
        },
        GuideExpansionLimits {
            // Configuration normalization, channel, receipt and path association.
            max_nodes: 4,
            max_bytes: usize::MAX,
        },
    );
    let blocked = capture(&pool, &[1], now(), now() + Duration::hours(1), &mut limited)
        .await
        .unwrap_err();
    assert_eq!(blocked.reason, GuideSnapshotFailure::Admission);
    assert_eq!(blocked.channel_id, Some(1));
    assert_eq!(blocked.playout_id, Some(1));
    assert_eq!(limited.usage().records, 6);
    let invalid = capture(
        &pool,
        &[1],
        now(),
        now() + Duration::hours(1),
        &mut budget(),
    )
    .await
    .unwrap_err();
    assert!(matches!(invalid.reason, GuideSnapshotFailure::Accepted(_)));
}

#[tokio::test]
async fn snapshot_requires_expansion_and_aggregate_clones_are_rejected_without_partial_result() {
    let (_root, pool) = fixture().await;
    segments(&pool).await;
    let mut raw = GuideReadBudget::new(GuideReadLimits {
        max_records: 1000,
        max_source_bytes: 1024 * 1024,
    });
    assert_eq!(
        capture(&pool, &[1], now(), now() + Duration::hours(4), &mut raw)
            .await
            .unwrap_err()
            .reason,
        GuideSnapshotFailure::MissingExpansion
    );
    assert_eq!(raw.usage().records, 0);
    let mut all = budget();
    let first = capture(&pool, &[1], now(), now() + Duration::hours(4), &mut all)
        .await
        .unwrap();
    let used = first.expansion_usage();
    let mut one = GuideReadBudget::new_with_expansion(
        GuideReadLimits {
            max_records: 1000,
            max_source_bytes: 1024 * 1024,
        },
        GuideExpansionLimits {
            max_nodes: used.nodes,
            max_bytes: used.bytes,
        },
    );
    assert!(
        capture(&pool, &[1], now(), now() + Duration::hours(4), &mut one)
            .await
            .is_ok()
    );
    assert!(
        capture(&pool, &[2], now(), now() + Duration::hours(4), &mut one)
            .await
            .is_err()
    );
    assert_eq!(
        capture(
            &pool,
            &[1, 1],
            now(),
            now() + Duration::hours(1),
            &mut budget()
        )
        .await
        .unwrap_err()
        .reason,
        GuideSnapshotFailure::DuplicateChannel
    );
    assert_eq!(
        capture(
            &pool,
            &[99],
            now(),
            now() + Duration::hours(1),
            &mut budget()
        )
        .await
        .unwrap_err()
        .reason,
        GuideSnapshotFailure::MissingChannel
    );
}

#[tokio::test]
async fn invalid_large_timestamps_fail_with_static_coordinates_before_legacy_error_formatting() {
    let (_root, pool) = fixture().await;
    segments(&pool).await;
    let reference = capture(
        &pool,
        &[1],
        now(),
        now() + Duration::hours(4),
        &mut budget(),
    )
    .await
    .unwrap();
    for column in ["Start", "Finish"] {
        let invalid = "private-invalid-date".repeat(16_384);
        let sql = format!(r#"UPDATE "PlayoutItem" SET "{column}"=? WHERE "Id"=31"#);
        sqlx::query(crate::db::assert_sql_safe(sql))
            .bind(&invalid)
            .execute(&pool)
            .await
            .unwrap();
        let mut limited = GuideReadBudget::new_with_expansion(
            GuideReadLimits {
                max_records: 1000,
                max_source_bytes: 1024 * 1024,
            },
            GuideExpansionLimits {
                max_nodes: reference.expansion_usage().nodes,
                max_bytes: reference.expansion_usage().bytes,
            },
        );
        let error = capture(&pool, &[1], now(), now() + Duration::hours(4), &mut limited)
            .await
            .unwrap_err();
        assert_eq!(error.reason, GuideSnapshotFailure::InvalidValue);
        assert_eq!((error.channel_id, error.playout_id), (Some(1), Some(1)));
        assert!(!error.to_string().contains("private-invalid"));
        pool.execute(r#"UPDATE "PlayoutItem" SET "Start"='2026-04-14 14:00:00',"Finish"='2026-04-14 19:00:00' WHERE "Id"=31"#).await.unwrap();
    }
}

#[tokio::test]
async fn named_text_mirror_cannot_disagree_with_numeric_source_selection() {
    let (_root, pool) = fixture().await;
    segments(&pool).await;
    let valid = capture(
        &pool,
        &[2],
        now(),
        now() + Duration::hours(1),
        &mut budget(),
    )
    .await
    .unwrap();
    assert_eq!(valid.channel(2).unwrap().source_channel_id(), 1);
    assert_eq!(valid.channel(2).unwrap().occurrences().len(), 1);
    pool.execute(r#"UPDATE "Channel" SET "PlayoutSource"='Mirror' WHERE "Id"=2"#)
        .await
        .unwrap();
    let scalar: String =
        sqlx::query_scalar(r#"SELECT typeof("PlayoutSource") FROM "Channel" WHERE "Id"=2"#)
            .fetch_one(&pool)
            .await
            .unwrap();
    assert_eq!(scalar, "text");
    let interpreted:i64=sqlx::query_scalar(r#"SELECT CASE WHEN "PlayoutSource"=1 THEN "MirrorSourceChannelId" ELSE "Id" END FROM "Channel" WHERE "Id"=2"#)
        .fetch_one(&pool).await.unwrap();
    assert_eq!(interpreted, 2);
    let mut limits = budget();
    let rejected = capture(&pool, &[2], now(), now() + Duration::hours(1), &mut limits)
        .await
        .unwrap_err();
    assert_eq!(
        rejected,
        GuideSnapshotError {
            channel_id: Some(2),
            playout_id: None,
            reason: GuideSnapshotFailure::InvalidValue
        }
    );
    assert_eq!(
        limits.usage().records,
        3,
        "rejected before querying the wrong source playout"
    );
    assert!(!rejected.to_string().contains("Mirror"));
}
