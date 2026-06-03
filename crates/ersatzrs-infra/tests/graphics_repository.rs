#![cfg(not(feature = "mysql"))]
mod common;

use ersatzrs_core::repositories::{
    ChannelRepository, FfmpegProfileRepository, GraphicsRepository, PlayoutRepository,
    ProgramScheduleRepository,
};
use ersatzrs_domain::{
    GraphicsElement, GraphicsElementConfig, GraphicsElementKind, ImageGraphicsConfig,
    MotionEndBehavior, MotionGraphicsConfig, SubtitleGraphicsConfig, TextAlignment,
    TextGraphicsConfig, WatermarkLocation,
};
use ersatzrs_infra::graphics_import::upsert_graphics_element_from_yaml;
use ersatzrs_infra::repositories::make_repositories;
use sqlx::{Executor, Row};

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

#[tokio::test]
async fn graphics_repository_uses_kind_default_when_config_is_null() {
    let pool = common::test_db().await;
    let repos = make_repositories(pool.clone());

    pool.execute(
        r#"INSERT INTO "GraphicsElement" ("Id", "Name", "Kind", "Path", "Config")
         VALUES (20, 'subtitle', 2, '/tmp/subtitle.yml', NULL)"#,
    )
    .await
    .unwrap();

    let element = repos.graphics.get_element(20).await.unwrap();
    assert_eq!(element.kind, GraphicsElementKind::Subtitle);
    assert_eq!(
        element.config,
        GraphicsElementConfig::Subtitle(SubtitleGraphicsConfig::default())
    );
}

#[tokio::test]
async fn graphics_repository_rejects_config_kind_mismatch() {
    let pool = common::test_db().await;
    let repos = make_repositories(pool.clone());
    let config =
        serde_json::to_string(&GraphicsElementConfig::Image(ImageGraphicsConfig::default()))
            .unwrap();

    sqlx::query(
        r#"INSERT INTO "GraphicsElement" ("Id", "Name", "Kind", "Path", "Config")
         VALUES (21, 'bad', 1, '/tmp/text.yml', ?)"#,
    )
    .bind(config)
    .execute(&pool)
    .await
    .unwrap();

    let error = repos.graphics.get_element(21).await.unwrap_err();
    assert!(error
        .to_string()
        .contains("config kind Image does not match row kind Text"));
}

#[tokio::test]
async fn graphics_yaml_import_honors_ersatztv_aliases_and_upserts() {
    let pool = common::test_db().await;
    let repos = make_repositories(pool.clone());
    let yaml = r"
name: Promo bug
video_path: /media/bug.mov
start_seconds: 2.5
hold_seconds: 4
end_behavior: Hold
location: TopLeft
horizontal_margin_percent: 3
vertical_margin_percent: 4
z_index: 9
epg_entries: 2
scale: true
scale_width_percent: 15
";

    let imported = upsert_graphics_element_from_yaml(
        &repos.graphics,
        22,
        GraphicsElementKind::Motion,
        "/templates/graphics-elements/motion/promo.yml",
        yaml,
    )
    .await
    .unwrap();
    assert_eq!(imported.name, "Promo bug");

    let element = repos.graphics.get_element(22).await.unwrap();
    assert_eq!(element.kind, GraphicsElementKind::Motion);
    assert_eq!(
        element.path.as_deref(),
        Some("/templates/graphics-elements/motion/promo.yml")
    );
    assert_eq!(element.z_index, 9);
    assert_eq!(element.location, WatermarkLocation::TopLeft);
    assert!(element.scale);
    assert_eq!(element.scale_width_percent, Some(15.0));
    assert_eq!(element.epg_entries, 2);
    let GraphicsElementConfig::Motion(MotionGraphicsConfig {
        video_path,
        start_seconds,
        hold_seconds,
        end_behavior,
    }) = element.config
    else {
        panic!("expected motion config");
    };
    assert_eq!(video_path.as_deref(), Some("/media/bug.mov"));
    assert_eq!(start_seconds, Some(2.5));
    assert_eq!(hold_seconds, Some(4.0));
    assert_eq!(end_behavior, MotionEndBehavior::Hold);

    let row = sqlx::query(r#"SELECT "Config" FROM "GraphicsElement" WHERE "Id" = 22"#)
        .fetch_one(&pool)
        .await
        .unwrap();
    let config: String = row.try_get("Config").unwrap();
    assert!(config.contains(r#""kind":"motion""#));

    let loop_yaml = yaml.replace("end_behavior: Hold", "end_behavior: loop");
    let loop_imported = upsert_graphics_element_from_yaml(
        &repos.graphics,
        23,
        GraphicsElementKind::Motion,
        "/templates/graphics-elements/motion/loop.yml",
        &loop_yaml,
    )
    .await
    .unwrap();
    let GraphicsElementConfig::Motion(loop_config) = loop_imported.config else {
        panic!("expected motion config");
    };
    assert_eq!(loop_config.end_behavior, MotionEndBehavior::Loop);
}
