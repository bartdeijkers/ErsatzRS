mod common;

use ersatzrs_core::repositories::{
    ChannelRepository, FfmpegProfileRepository, GraphicsRepository, PlayoutRepository,
    ProgramScheduleRepository,
};
use ersatzrs_domain::GraphicsElement;
use ersatzrs_infra::repositories::make_repositories;
use sqlx::Executor;

#[tokio::test]
async fn graphics_repository_round_trips_elements_and_lists_for_channel() {
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
    pool.execute(
        "INSERT OR REPLACE INTO playouts (id, channel_id, program_schedule_id, schedule_kind, seed)
         VALUES (1, 1, 1, 'Classic', 0)",
    )
    .await
    .unwrap();
    repos
        .playout
        .upsert_item(&common::playout_item(1, 1, 1))
        .await
        .unwrap();

    repos
        .graphics
        .upsert_element(&GraphicsElement {
            id: 2,
            name: "lower-third".to_owned(),
            x: 10,
            y: 20,
            width: 300,
            height: 80,
            text: Some("Now".to_owned()),
            ..GraphicsElement::default()
        })
        .await
        .unwrap();
    pool.execute(
        "INSERT INTO playout_item_graphics_elements (playout_item_id, graphics_element_id, variables)
         VALUES (1, 2, '{}')",
    )
    .await
    .unwrap();

    assert_eq!(
        repos.graphics.get_element(2).await.unwrap().name,
        "lower-third"
    );
    assert_eq!(repos.graphics.list_for_channel(1).await.unwrap()[0].id, 2);
}
