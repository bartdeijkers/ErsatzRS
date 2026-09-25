mod common;

use chrono::Duration;
use ersatzrs_core::repositories::{
    ChannelRepository, FfmpegProfileRepository, PlayoutCreateAdmin, PlayoutRepository,
    ProgramScheduleRepository,
};
use ersatzrs_infra::repositories::make_repositories;
use sqlx::{Executor, Row};

#[tokio::test]
async fn playout_repository_loads_anchor_status_and_filters_item_ranges() {
    let pool = common::test_db().await;
    let repos = make_repositories(pool.clone());
    common::seed_media_graph(&pool).await;
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
    pool.execute(r#"INSERT INTO "TemplateGroup" ("Id", "Name") VALUES (1, 'default-group')"#)
        .await
        .unwrap();
    pool.execute(
        r#"INSERT INTO "Template" ("Id", "TemplateGroupId", "Name", "DateUpdated")
         VALUES (10, 1, 'weekday', '1000')"#,
    )
    .await
    .unwrap();
    pool.execute(r#"INSERT INTO "DecoTemplateGroup" ("Id", "Name") VALUES (1, 'deco-group')"#)
        .await
        .unwrap();
    pool.execute(
        r#"INSERT INTO "DecoTemplate" ("Id", "DecoTemplateGroupId", "Name", "DateUpdated")
         VALUES (11, 1, 'morning-window', '0')"#,
    )
    .await
    .unwrap();
    pool.execute(
        r#"INSERT OR REPLACE INTO "Playout" ("Id", "ChannelId", "ProgramScheduleId", "ScheduleKind", "Seed")
         VALUES (1, 1, 1, 'Classic', 0)"#,
    )
    .await
    .unwrap();
    pool.execute(
        r#"INSERT INTO "PlayoutTemplate" (
            "PlayoutId", "TemplateId", "DateUpdated", "DecoTemplateId", "Index",
            "DaysOfWeek", "DaysOfMonth", "MonthsOfYear",
            "LimitToDateRange", "StartMonth", "StartDay", "EndMonth", "EndDay"
         ) VALUES (
            1, 10, '1234', 11, 2, '[1,2,3]', '[1,15]', '[4]', 1, 4, 1, 4, 30
         )"#,
    )
    .await
    .unwrap();

    let start = common::fixed_time();
    repos
        .playout
        .upsert_item(&common::playout_item(1, 1, 1))
        .await
        .unwrap();
    repos
        .playout
        .upsert_build_status(&common::build_status(1))
        .await
        .unwrap();

    let in_range = repos
        .playout
        .items_in_range(
            1,
            start + Duration::minutes(5),
            start + Duration::minutes(10),
        )
        .await
        .unwrap();
    assert_eq!(in_range[0].id, 1);

    let loaded = repos.playout.get_with_anchor(1).await.unwrap();
    assert_eq!(loaded.item_ids, vec![1]);
    assert_eq!(loaded.templates.len(), 1);
    assert_eq!(loaded.templates[0].updated_ticks, 1234);
    assert_eq!(loaded.templates[0].deco_template_id, Some(11));
    assert_eq!(loaded.templates[0].days_of_week, vec![1, 2, 3]);
    assert_eq!(loaded.build_status.unwrap().playout_id, 1);
}

#[tokio::test]
async fn playout_repository_replaces_runtime_state_tables() {
    let pool = common::test_db().await;
    let repos = make_repositories(pool.clone());
    seed_runtime_state_fixture(&pool, &repos).await;

    let mut playout = common::playout(1, 1, 1);
    playout.anchor = Some(common::playout_anchor());
    playout.templates = vec![common::playout_template(1, 10, 11)];

    repos
        .playout
        .replace_runtime_state(
            &playout,
            &[common::playout_gap(21, 1)],
            &[common::playout_history(31, 1)],
            &[common::program_schedule_anchor(41, 1)],
            &[common::fill_group_index(51, 1, 30)],
        )
        .await
        .unwrap();

    let loaded = repos.playout.get_with_anchor(1).await.unwrap();
    assert_eq!(loaded.gap_ids, vec![21]);
    assert_eq!(loaded.playout_history_ids, vec![31]);
    assert_eq!(loaded.program_schedule_anchor_ids, vec![41]);
    assert_eq!(loaded.fill_group_index_ids, vec![51]);
    assert_eq!(loaded.templates, vec![common::playout_template(1, 10, 11)]);
    assert_eq!(loaded.anchor, Some(common::playout_anchor()));
    assert_eq!(loaded.on_demand_checkpoint, playout.on_demand_checkpoint);

    assert_runtime_state_rows(&pool).await;
}

#[tokio::test]
async fn playout_runtime_state_ids_are_global_and_cannot_replace_another_playout() {
    let pool = common::test_db().await;
    let repos = make_repositories(pool.clone());
    seed_runtime_state_fixture(&pool, &repos).await;
    seed_second_runtime_state_fixture(&pool, &repos).await;
    let first = common::playout(1, 1, 1);
    let second = common::playout(2, 2, 2);

    repos
        .playout
        .replace_runtime_state(
            &first,
            &[],
            &[],
            &[common::program_schedule_anchor(0, 1)],
            &[common::fill_group_index(0, 1, 30)],
        )
        .await
        .unwrap();
    repos
        .playout
        .replace_runtime_state(
            &second,
            &[],
            &[],
            &[common::program_schedule_anchor(0, 2)],
            &[common::fill_group_index(0, 2, 31)],
        )
        .await
        .unwrap();

    let first_anchor_id = runtime_state_id(&pool, "PlayoutProgramScheduleAnchor", 1).await;
    let second_anchor_id = runtime_state_id(&pool, "PlayoutProgramScheduleAnchor", 2).await;
    let first_fill_id = runtime_state_id(&pool, "PlayoutScheduleItemFillGroupIndex", 1).await;
    let second_fill_id = runtime_state_id(&pool, "PlayoutScheduleItemFillGroupIndex", 2).await;
    assert_ne!(first_anchor_id, second_anchor_id);
    assert_ne!(first_fill_id, second_fill_id);

    let anchor_collision = repos
        .playout
        .replace_runtime_state(
            &first,
            &[],
            &[],
            &[common::program_schedule_anchor(second_anchor_id, 1)],
            &[common::fill_group_index(first_fill_id, 1, 30)],
        )
        .await;
    assert!(anchor_collision.is_err());
    assert_eq!(
        runtime_state_id(&pool, "PlayoutProgramScheduleAnchor", 1).await,
        first_anchor_id,
        "a rejected anchor collision must roll back the caller's state"
    );
    assert_eq!(
        runtime_state_id(&pool, "PlayoutProgramScheduleAnchor", 2).await,
        second_anchor_id,
        "a rejected anchor collision must preserve the owning playout"
    );

    let fill_collision = repos
        .playout
        .replace_runtime_state(
            &first,
            &[],
            &[],
            &[common::program_schedule_anchor(first_anchor_id, 1)],
            &[common::fill_group_index(second_fill_id, 1, 30)],
        )
        .await;
    assert!(fill_collision.is_err());
    assert_eq!(
        runtime_state_id(&pool, "PlayoutScheduleItemFillGroupIndex", 1).await,
        first_fill_id,
        "a rejected fill-group collision must roll back the caller's state"
    );
    assert_eq!(
        runtime_state_id(&pool, "PlayoutScheduleItemFillGroupIndex", 2).await,
        second_fill_id,
        "a rejected fill-group collision must preserve the owning playout"
    );

    let wrong_owner = repos
        .playout
        .replace_runtime_state(
            &first,
            &[],
            &[],
            &[common::program_schedule_anchor(0, 2)],
            &[common::fill_group_index(first_fill_id, 1, 30)],
        )
        .await;
    assert!(wrong_owner.is_err());
    assert_eq!(
        runtime_state_id(&pool, "PlayoutProgramScheduleAnchor", 1).await,
        first_anchor_id,
        "cross-playout state must be rejected before deleting existing rows"
    );
}

async fn seed_second_runtime_state_fixture(
    pool: &sqlx::SqlitePool,
    repos: &ersatzrs_infra::repositories::Repositories,
) {
    repos
        .channel
        .upsert(&common::channel(2, "2", true))
        .await
        .unwrap();
    repos
        .program_schedule
        .upsert(&common::schedule(2, vec![31]))
        .await
        .unwrap();
    pool.execute(
        r#"INSERT INTO "Playout" ("Id", "ChannelId", "ProgramScheduleId", "ScheduleKind", "Seed")
           VALUES (2, 2, 2, 0, 100)"#,
    )
    .await
    .unwrap();
}

#[tokio::test]
async fn playout_admin_repository_lists_creates_edits_and_erases_playouts() {
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

    let channels = repos.playout.list_create_channels_admin().await.unwrap();
    assert_eq!(channels.len(), 1);
    assert_eq!(channels[0].playout_count, 0);

    let id = repos
        .playout
        .create_admin(&PlayoutCreateAdmin {
            channel_id: 1,
            schedule_kind: 3,
            program_schedule_id: None,
            schedule_file: "one: two".to_owned(),
            seed: 42,
        })
        .await
        .unwrap();
    let sequential = repos
        .playout
        .get_sequential_admin(id)
        .await
        .unwrap()
        .unwrap();
    assert_eq!(sequential.yaml_content, "one: two");

    repos
        .playout
        .save_schedule_file_admin(id, 3, "three: four")
        .await
        .unwrap();
    assert_eq!(
        repos
            .playout
            .get_sequential_admin(id)
            .await
            .unwrap()
            .unwrap()
            .yaml_content,
        "three: four"
    );

    let rows = repos.playout.list_admin("", 0, 10).await.unwrap();
    assert_eq!(rows.len(), 1);
    assert_eq!(rows[0].schedule_kind, 3);

    repos
        .playout
        .erase_items_and_history_admin(id)
        .await
        .unwrap();
    repos.playout.delete_admin(id).await.unwrap();
    assert!(repos.playout.delete_admin(id).await.is_err());
}

#[tokio::test]
async fn playout_admin_repository_propagates_closed_pool_failures() {
    let pool = common::test_db().await;
    let repos = make_repositories(pool.clone());
    pool.close().await;

    assert!(repos.playout.list_admin("", 0, 10).await.is_err());
    assert!(repos.playout.count_admin().await.is_err());
    assert!(repos.playout.list_create_channels_admin().await.is_err());
}

async fn seed_runtime_state_fixture(
    pool: &sqlx::SqlitePool,
    repos: &ersatzrs_infra::repositories::Repositories,
) {
    common::seed_collection_target(pool).await;
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
        .upsert(&common::schedule(1, vec![30]))
        .await
        .unwrap();
    seed_runtime_state_templates(pool).await;
    pool.execute(
        r#"INSERT OR REPLACE INTO "Playout" ("Id", "ChannelId", "ProgramScheduleId", "ScheduleKind", "Seed")
         VALUES (1, 1, 1, 0, 99)"#,
    )
    .await
    .unwrap();
}

async fn seed_runtime_state_templates(pool: &sqlx::SqlitePool) {
    pool.execute(r#"INSERT INTO "TemplateGroup" ("Id", "Name") VALUES (1, 'default-group')"#)
        .await
        .unwrap();
    pool.execute(
        r#"INSERT INTO "Template" ("Id", "TemplateGroupId", "Name", "DateUpdated")
         VALUES (10, 1, 'weekday', '1000')"#,
    )
    .await
    .unwrap();
    pool.execute(r#"INSERT INTO "DecoTemplateGroup" ("Id", "Name") VALUES (1, 'deco-group')"#)
        .await
        .unwrap();
    pool.execute(
        r#"INSERT INTO "DecoTemplate" ("Id", "DecoTemplateGroupId", "Name", "DateUpdated")
         VALUES (11, 1, 'morning-window', '0')"#,
    )
    .await
    .unwrap();
}

async fn assert_runtime_state_rows(pool: &sqlx::SqlitePool) {
    assert_history_row(pool).await;
    assert_anchor_state_row(pool).await;
    assert_program_anchor_state_row(pool).await;
    assert_fill_group_state_row(pool).await;
}

async fn assert_history_row(pool: &sqlx::SqlitePool) {
    let history_row = sqlx::query(
        r#"SELECT "PlaybackOrder", "ChildKey", "IsCurrentChild"
           FROM "PlayoutHistory"
           WHERE "Id" = 31"#,
    )
    .fetch_one(pool)
    .await
    .unwrap();
    assert_eq!(history_row.try_get::<i32, _>("PlaybackOrder").unwrap(), 1);
    assert_eq!(
        history_row
            .try_get::<Option<String>, _>("ChildKey")
            .unwrap(),
        Some("child-key".to_owned())
    );
    assert!(history_row.try_get::<bool, _>("IsCurrentChild").unwrap());
}

async fn assert_anchor_state_row(pool: &sqlx::SqlitePool) {
    let anchor_state = sqlx::query(
        r#"SELECT "Seed", "Index", "Started" FROM "ScheduleItemsEnumeratorState"
           WHERE "PlayoutAnchorPlayoutId" = 1"#,
    )
    .fetch_one(pool)
    .await
    .unwrap();
    assert_eq!(anchor_state.try_get::<i32, _>("Seed").unwrap(), 77);
    assert_eq!(anchor_state.try_get::<i32, _>("Index").unwrap(), 5);
    assert!(anchor_state.try_get::<bool, _>("Started").unwrap());
}

async fn assert_program_anchor_state_row(pool: &sqlx::SqlitePool) {
    let program_anchor_state = sqlx::query(
        r#"SELECT "Seed", "Index", "Started" FROM "CollectionEnumeratorState"
           WHERE "PlayoutProgramScheduleAnchorId" = 41"#,
    )
    .fetch_one(pool)
    .await
    .unwrap();
    assert_eq!(program_anchor_state.try_get::<i32, _>("Seed").unwrap(), 33);
    assert_eq!(program_anchor_state.try_get::<i32, _>("Index").unwrap(), 7);
    assert!(program_anchor_state.try_get::<bool, _>("Started").unwrap());
}

async fn assert_fill_group_state_row(pool: &sqlx::SqlitePool) {
    let fill_group_state = sqlx::query(
        r#"SELECT "Seed", "Index", "Started" FROM "FillGroupEnumeratorState"
           WHERE "PlayoutScheduleItemFillGroupIndexId" = 51"#,
    )
    .fetch_one(pool)
    .await
    .unwrap();
    assert_eq!(fill_group_state.try_get::<i32, _>("Seed").unwrap(), 55);
    assert_eq!(fill_group_state.try_get::<i32, _>("Index").unwrap(), 11);
    assert!(fill_group_state.try_get::<bool, _>("Started").unwrap());
}

async fn runtime_state_id(pool: &sqlx::SqlitePool, table: &str, playout_id: i64) -> i64 {
    match table {
        "PlayoutProgramScheduleAnchor" => sqlx::query_scalar(
            r#"SELECT "Id" FROM "PlayoutProgramScheduleAnchor" WHERE "PlayoutId" = ?"#,
        )
        .bind(playout_id)
        .fetch_one(pool)
        .await
        .unwrap(),
        "PlayoutScheduleItemFillGroupIndex" => sqlx::query_scalar(
            r#"SELECT "Id" FROM "PlayoutScheduleItemFillGroupIndex" WHERE "PlayoutId" = ?"#,
        )
        .bind(playout_id)
        .fetch_one(pool)
        .await
        .unwrap(),
        _ => panic!("unsupported runtime-state table"),
    }
}
