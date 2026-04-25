#![cfg(not(feature = "mysql"))]
mod common;

use ersatzrs_core::repositories::{
    ChannelRepository, FfmpegProfileRepository, ProgramScheduleRepository,
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
