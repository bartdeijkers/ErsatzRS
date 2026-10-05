use ersatzrs_core::repositories::{BlockRepository, ProgramScheduleRepository};
use ersatzrs_domain::{Block, BlockItem, ProgramSchedule, ProgramScheduleItem};
use ersatzrs_infra::repositories::make_repositories;
use sqlx::{sqlite::SqlitePoolOptions, Executor as _};

#[tokio::test]
async fn graphics_attachment_variables_migration_preserves_legacy_ids_and_restart_values() {
    let pool = SqlitePoolOptions::new()
        .max_connections(1)
        .connect("sqlite::memory:")
        .await
        .unwrap();
    let migrations = sqlx::migrate!("../../migrations");
    sqlx::migrate::Migrator::with_migrations(
        migrations
            .iter()
            .filter(|migration| migration.version < 202_610_050_930)
            .cloned()
            .collect(),
    )
    .run(&pool)
    .await
    .unwrap();
    let repos = make_repositories(pool.clone());
    repos
        .program_schedule
        .upsert(&ProgramSchedule {
            id: 1,
            name: "Legacy schedule".into(),
            ..Default::default()
        })
        .await
        .unwrap();
    repos
        .program_schedule
        .upsert_item(&ProgramScheduleItem {
            id: 2,
            program_schedule_id: 1,
            ..Default::default()
        })
        .await
        .unwrap();
    pool.execute(r#"INSERT INTO "BlockGroup" ("Id", "Name") VALUES (1, 'Legacy group')"#)
        .await
        .unwrap();
    repos
        .block
        .upsert(&Block {
            id: 1,
            block_group_id: Some(1),
            name: "Legacy block".into(),
            ..Default::default()
        })
        .await
        .unwrap();
    repos
        .block
        .upsert_item(&BlockItem {
            id: 3,
            block_id: 1,
            ..Default::default()
        })
        .await
        .unwrap();
    pool.execute(r#"INSERT INTO "GraphicsElement" ("Id", "Kind", "Name") VALUES (10, 1, 'Existing');
        INSERT INTO "ProgramScheduleItemGraphicsElement" ("ProgramScheduleItemId", "GraphicsElementId") VALUES (2, 10);
        INSERT INTO "BlockItemGraphicsElement" ("BlockItemId", "GraphicsElementId") VALUES (3, 10);"#).await.unwrap();
    migrations.run(&pool).await.unwrap();
    let schedule = repos.program_schedule.get_item(2).await.unwrap();
    let block = repos.block.get_item(3).await.unwrap();
    assert_eq!(schedule.graphics_element_ids, [10]);
    assert_eq!(block.graphics_element_ids, [10]);
    assert!(schedule.graphics_elements.is_empty());
    assert!(block.graphics_elements.is_empty());
    pool.execute(
        r#"UPDATE "ProgramScheduleItemGraphicsElement" SET "Variables"='{"title":"Saved schedule"}';
        UPDATE "BlockItemGraphicsElement" SET "Variables"='{"title":"Saved block"}';"#,
    )
    .await
    .unwrap();
    migrations.run(&pool).await.unwrap();
    let restarted = make_repositories(pool.clone());
    assert_eq!(
        restarted
            .program_schedule
            .get_item(2)
            .await
            .unwrap()
            .graphics_elements[0]
            .variables
            .as_deref(),
        Some(r#"{"title":"Saved schedule"}"#)
    );
    assert_eq!(
        restarted.block.get_item(3).await.unwrap().graphics_elements[0]
            .variables
            .as_deref(),
        Some(r#"{"title":"Saved block"}"#)
    );
    assert_eq!(
        sqlx::query_scalar::<_, i64>("SELECT COUNT(*) FROM \"PlayoutItem\"")
            .fetch_one(&pool)
            .await
            .unwrap(),
        0
    );
}
