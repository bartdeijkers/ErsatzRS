mod common;

use ersatzrs_core::repositories::{
    ChannelRepository, FfmpegProfileRepository, ProgramScheduleRepository,
};
use ersatzrs_infra::repositories::make_repositories;
use sqlx::Executor;

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
        "INSERT OR REPLACE INTO playouts (id, channel_id, program_schedule_id, schedule_kind, seed)
         VALUES (1, 1, 1, 'Classic', 0)",
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
