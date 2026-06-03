#![cfg(not(feature = "mysql"))]
mod common;

use ersatzrs_core::repositories::{
    ChannelRepository, FfmpegProfileRepository, GraphicsRepository, PlayoutRepository,
    ProgramScheduleRepository,
};
use ersatzrs_domain::{
    GraphicsElement, GraphicsElementConfig, GraphicsElementKind, TextAlignment, TextGraphicsConfig,
};
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
        r#"INSERT OR REPLACE INTO "Playout" ("Id", "ChannelId", "ProgramScheduleId", "ScheduleKind", "Seed")
         VALUES (1, 1, 1, 0, 0)"#,
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
            kind: GraphicsElementKind::Text,
            z_index: 7,
            opacity_percent: Some(80),
            horizontal_margin_percent: Some(10.0),
            vertical_margin_percent: Some(20.0),
            scale_width_percent: Some(30.0),
            config: GraphicsElementConfig::Text(TextGraphicsConfig {
                text: Some("Now".to_owned()),
                align: TextAlignment::Center,
                width_percent: Some(30.0),
                ..TextGraphicsConfig::default()
            }),
            ..GraphicsElement::default()
        })
        .await
        .unwrap();
    pool.execute(
        r#"INSERT INTO "PlayoutItemGraphicsElement" ("PlayoutItemId", "GraphicsElementId", "Variables")
         VALUES (1, 2, '{}')"#,
    )
    .await
    .unwrap();

    let element = repos.graphics.get_element(2).await.unwrap();
    assert_eq!(element.name, "lower-third");
    assert_eq!(element.kind, GraphicsElementKind::Text);
    assert_eq!(element.z_index, 7);
    assert_eq!(element.opacity_percent, Some(80));
    assert_eq!(element.horizontal_margin_percent, Some(10.0));
    assert_eq!(element.vertical_margin_percent, Some(20.0));
    assert_eq!(element.scale_width_percent, Some(30.0));
    let GraphicsElementConfig::Text(config) = element.config else {
        panic!("expected text config");
    };
    assert_eq!(config.text.as_deref(), Some("Now"));
    assert_eq!(config.align, TextAlignment::Center);
    assert_eq!(config.width_percent, Some(30.0));
    assert_eq!(repos.graphics.list_for_channel(1).await.unwrap()[0].id, 2);
}
