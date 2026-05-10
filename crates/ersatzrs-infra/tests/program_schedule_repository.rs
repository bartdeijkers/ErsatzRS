#![cfg(not(feature = "mysql"))]
mod common;

use chrono::Duration;
use ersatzrs_core::repositories::{
    ChannelRepository, FfmpegProfileRepository, ProgramScheduleRepository,
};
use ersatzrs_domain::{
    CollectionType, FillWithGroupMode, GuideMode, MarathonGroupBy, MultipleMode, PlaybackOrder,
    PlayoutMode, ProgramScheduleAlternate, ProgramScheduleItem, TailMode,
};
use ersatzrs_infra::repositories::make_repositories;
use sqlx::{Executor, Row};

#[tokio::test]
async fn program_schedule_repository_loads_items_and_lists_for_playout() {
    let pool = common::test_db().await;
    let repos = make_repositories(pool.clone());

    repos
        .program_schedule
        .upsert(&common::schedule(1, vec![30]))
        .await
        .unwrap();
    repos
        .ffmpeg_profile
        .upsert(&common::profile(1, "default"))
        .await
        .unwrap();
    repos
        .channel
        .upsert(&common::channel(1, "1", true))
        .await
        .unwrap();
    pool.execute(
        r#"INSERT OR REPLACE INTO "Playout" ("Id", "ChannelId", "ProgramScheduleId", "ScheduleKind", "Seed")
         VALUES (1, 1, 1, 0, 0)"#,
    )
    .await
    .unwrap();

    assert_eq!(
        repos
            .program_schedule
            .get_with_items(1)
            .await
            .unwrap()
            .item_ids,
        vec![30]
    );
    let listed = repos.program_schedule.list_for_playout(1).await.unwrap();
    assert_eq!(listed[0].id, 1);
    assert!(listed[0].item_ids.is_empty());
    assert!(listed[0].playout_ids.is_empty());
}

#[tokio::test]
async fn program_schedule_repository_round_trips_typed_items_and_overlays() {
    let pool = common::test_db().await;
    let repos = make_repositories(pool.clone());
    repos
        .program_schedule
        .upsert(&common::schedule(1, Vec::new()))
        .await
        .unwrap();
    seed_overlay_targets(&pool).await;

    let item = ProgramScheduleItem {
        id: 30,
        program_schedule_id: 1,
        collection_type: CollectionType::SearchQuery,
        search_query: Some("genre:comedy".to_owned()),
        playout_mode: PlayoutMode::Multiple,
        playback_order: PlaybackOrder::Random,
        multiple_mode: MultipleMode::CollectionSize,
        multiple_count: Some("3".to_owned()),
        guide_mode: GuideMode::Filler,
        fill_with_group_mode: FillWithGroupMode::FillWithOrderedGroups,
        marathon_group_by: MarathonGroupBy::Show,
        marathon_shuffle_items: true,
        custom_title: Some("Comedy".to_owned()),
        sort_order: 7,
        watermark_ids: vec![501],
        graphics_element_ids: vec![601],
        ..ProgramScheduleItem::default()
    };
    repos.program_schedule.upsert_item(&item).await.unwrap();

    let loaded = repos.program_schedule.get_item(30).await.unwrap();
    assert_eq!(loaded.playout_mode, PlayoutMode::Multiple);
    assert_eq!(loaded.multiple_mode, MultipleMode::CollectionSize);
    assert_eq!(loaded.multiple_count, Some("3".to_owned()));
    assert_eq!(loaded.search_query, Some("genre:comedy".to_owned()));
    assert_eq!(loaded.watermark_ids, vec![501]);
    assert_eq!(loaded.graphics_element_ids, vec![601]);

    let duration = ProgramScheduleItem {
        id: 31,
        program_schedule_id: 1,
        playout_mode: PlayoutMode::Duration,
        duration: Some(Duration::minutes(45)),
        tail_mode: TailMode::Filler,
        sort_order: 8,
        ..ProgramScheduleItem::default()
    };
    repos.program_schedule.upsert_item(&duration).await.unwrap();

    let loaded_duration = repos.program_schedule.get_item(31).await.unwrap();
    assert_eq!(loaded_duration.playout_mode, PlayoutMode::Duration);
    assert_eq!(loaded_duration.duration, Some(Duration::minutes(45)));
    assert_eq!(loaded_duration.tail_mode, TailMode::Filler);

    let items = repos.program_schedule.list_items(1).await.unwrap();
    assert_eq!(
        items.iter().map(|item| item.id).collect::<Vec<_>>(),
        vec![30, 31]
    );
}

#[tokio::test]
async fn program_schedule_repository_round_trips_alternates() {
    let pool = common::test_db().await;
    let repos = make_repositories(pool.clone());

    repos
        .ffmpeg_profile
        .upsert(&common::profile(1, "default"))
        .await
        .unwrap();
    repos
        .channel
        .upsert(&common::channel(1, "1", true))
        .await
        .unwrap();
    repos
        .program_schedule
        .upsert(&common::schedule(1, Vec::new()))
        .await
        .unwrap();
    pool.execute(
        r#"INSERT INTO "Playout" ("Id", "ChannelId", "ProgramScheduleId", "ScheduleKind", "Seed")
           VALUES (10, 1, 1, 1, 0)"#,
    )
    .await
    .unwrap();

    repos
        .program_schedule
        .upsert_alternate(&ProgramScheduleAlternate {
            id: 40,
            program_schedule_id: 1,
            playout_id: Some(10),
            sort_order: 2,
            days_of_week: vec!["Monday".to_owned(), "Friday".to_owned()],
            days_of_month: vec![1, 15],
            months_of_year: vec![5, 12],
            limit_to_date_range: true,
            start_month: 5,
            start_day: 1,
            start_year: Some(2026),
            end_month: 12,
            end_day: 31,
            end_year: Some(2026),
        })
        .await
        .unwrap();

    let alternates = repos.program_schedule.list_alternates(1).await.unwrap();
    assert_eq!(alternates.len(), 1);
    assert_eq!(alternates[0].days_of_week, vec!["Monday", "Friday"]);
    assert_eq!(alternates[0].days_of_month, vec![1, 15]);
    assert_eq!(alternates[0].months_of_year, vec![5, 12]);
    assert!(alternates[0].limit_to_date_range);

    let row = sqlx::query(
        r#"SELECT "DaysOfWeek", "DaysOfMonth", "MonthsOfYear"
           FROM "ProgramScheduleAlternate"
           WHERE "Id" = 40"#,
    )
    .fetch_one(&pool)
    .await
    .unwrap();
    assert_eq!(
        row.get::<Option<String>, _>("DaysOfWeek"),
        Some(r#"["Monday","Friday"]"#.to_owned())
    );
    assert_eq!(
        row.get::<Option<String>, _>("DaysOfMonth"),
        Some("1,15".to_owned())
    );
    assert_eq!(
        row.get::<Option<String>, _>("MonthsOfYear"),
        Some("5,12".to_owned())
    );
}

#[tokio::test]
async fn program_schedule_repository_preserves_existing_item_target_fields() {
    let pool = common::test_db().await;
    let repos = make_repositories(pool.clone());

    repos
        .program_schedule
        .upsert(&common::schedule(1, Vec::new()))
        .await
        .unwrap();
    pool.execute(
        r#"INSERT INTO "ProgramScheduleItem" (
            "Id", "ProgramScheduleId", "CollectionType", "PlaybackOrder",
            "FillWithGroupMode", "GuideMode", "MarathonGroupBy",
            "MarathonShuffleGroups", "MarathonShuffleItems", "Index",
            "SearchQuery", "CustomTitle"
         ) VALUES (30, 1, 9, 2, 0, 0, 0, 0, 0, 5, 'genre:comedy', 'Comedy block')"#,
    )
    .await
    .unwrap();

    repos
        .program_schedule
        .upsert(&common::schedule(1, vec![30]))
        .await
        .unwrap();

    let row = sqlx::query(
        r#"SELECT "CollectionType", "PlaybackOrder", "Index", "SearchQuery", "CustomTitle"
           FROM "ProgramScheduleItem"
           WHERE "Id" = 30"#,
    )
    .fetch_one(&pool)
    .await
    .unwrap();

    assert_eq!(row.get::<i64, _>("CollectionType"), 9);
    assert_eq!(row.get::<i64, _>("PlaybackOrder"), 2);
    assert_eq!(row.get::<i64, _>("Index"), 0);
    assert_eq!(
        row.get::<Option<String>, _>("SearchQuery"),
        Some("genre:comedy".to_owned())
    );
    assert_eq!(
        row.get::<Option<String>, _>("CustomTitle"),
        Some("Comedy block".to_owned())
    );
}

async fn seed_overlay_targets(pool: &sqlx::SqlitePool) {
    pool.execute(
        r#"INSERT INTO "ChannelWatermark" (
            "Id", "DurationSeconds", "FrequencyMinutes", "HorizontalMarginPercent",
            "ImageSource", "Location", "Mode", "Name", "Opacity",
            "PlaceWithinSourceContent", "Size", "VerticalMarginPercent",
            "WidthPercent", "ZIndex"
        ) VALUES (501, 30, 10, 0.0, 0, 0, 1, 'wm', 100, 0, 0, 0.0, 10.0, 0)"#,
    )
    .await
    .unwrap();
    pool.execute(r#"INSERT INTO "GraphicsElement" ("Id", "Kind", "Name") VALUES (601, 1, 'ge')"#)
        .await
        .unwrap();
}
