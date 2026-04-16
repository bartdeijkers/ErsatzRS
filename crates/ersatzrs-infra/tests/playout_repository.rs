mod common;

use chrono::Duration;
use ersatzrs_core::repositories::{
    ChannelRepository, FfmpegProfileRepository, PlayoutRepository, ProgramScheduleRepository,
};
use ersatzrs_infra::repositories::make_repositories;
use sqlx::Executor;

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
