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

#[tokio::test]
async fn authored_graphics_all_kinds_roundtrip_yaml_and_sql_without_losing_shared_values() {
    use ersatzrs_domain::{ScriptGraphicsConfig, TextFit};
    let pool = common::test_db().await;
    let repos = make_repositories(pool);
    let configs = [
        GraphicsElementConfig::Image(ImageGraphicsConfig {
            image: Some("graphics/logo.png".into()),
        }),
        GraphicsElementConfig::Text(TextGraphicsConfig {
            text: Some("Long Épisode title".into()),
            fit: TextFit::Wrap,
            width_percent: Some(70.0),
            ..Default::default()
        }),
        GraphicsElementConfig::Subtitle(SubtitleGraphicsConfig {
            template: Some("graphics/title.ass".into()),
        }),
        GraphicsElementConfig::Motion(MotionGraphicsConfig {
            video_path: Some("graphics/motion.nut".into()),
            start_seconds: Some(1.25),
            hold_seconds: Some(2.5),
            end_behavior: MotionEndBehavior::Hold,
        }),
        GraphicsElementConfig::Script(ScriptGraphicsConfig {
            command: Some("return 'setcolor red\\nrect 0 0 20 20\\nfill'".into()),
            args: vec!["one".into()],
            format: Some("Raw".into()),
            ..Default::default()
        }),
    ];
    for config in configs {
        let element = GraphicsElement {
            name: format!("Author {:?}", config.kind()),
            kind: config.kind(),
            config,
            location: WatermarkLocation::TopLeft,
            location_x: Some(32.5),
            location_y: Some(40.0),
            opacity_percent: Some(72),
            opacity_expression: Some("between(t,1,3)".into()),
            z_index: 4,
            epg_entries: 12,
            horizontal_margin_percent: Some(2.0),
            vertical_margin_percent: Some(3.0),
            scale: true,
            scale_width_percent: Some(37.5),
            place_within_source_content: true,
            ..Default::default()
        };
        let id = repos.graphics.create_element(&element).await.unwrap();
        let saved = repos.graphics.get_element(id).await.unwrap();
        let yaml = repos.graphics.export_yaml(&saved).await.unwrap();
        let mut parsed = repos.graphics.parse_yaml(saved.kind, &yaml).await.unwrap();
        parsed.id = id;
        parsed.path = saved.path.clone();
        assert_eq!(parsed, saved);
        parsed.name.push_str(" edited");
        repos.graphics.upsert_element(&parsed).await.unwrap();
        assert_eq!(repos.graphics.get_element(id).await.unwrap(), parsed);
    }
    assert_eq!(repos.graphics.list_elements().await.unwrap().len(), 5);
    for yaml in [
        "name: Typo\ntext: Title\ndrawtext:\n  wrong_font_file: font.ttf",
        "name: Typo\ntext: Title\nstyles:\n  - font_szie: 32",
    ] {
        assert!(repos
            .graphics
            .parse_yaml(GraphicsElementKind::Text, yaml)
            .await
            .is_err());
    }
    for yaml in [
        "name: Typo\nimage: logo.png\nunknown: true",
        "name: Duplicate\nimage: logo.png\nz_index: 1\nzIndex: 2",
        "[]",
    ] {
        assert!(repos
            .graphics
            .parse_yaml(GraphicsElementKind::Image, yaml)
            .await
            .is_err());
    }
}

#[tokio::test]
async fn authored_graphics_validation_rejects_missing_inputs_before_any_publication() {
    use ersatzrs_core::repositories::GraphicsRenderPreviewRequest;
    use ersatzrs_domain::{ScriptGraphicsConfig, TextGraphicsRenderer};
    let pool = common::test_db().await;
    let repos = make_repositories(pool);
    let mut request = GraphicsRenderPreviewRequest {
        element: GraphicsElement {
            name: "Validate draft".into(),
            kind: GraphicsElementKind::Text,
            config: GraphicsElementConfig::Text(TextGraphicsConfig {
                text: Some("{{ Unknown }}".into()),
                ..Default::default()
            }),
            ..Default::default()
        },
        channel_id: None,
        render_time: common::fixed_time(),
        seek_seconds: 0.0,
        width: 320,
        height: 180,
        variables: None,
    };
    assert!(repos
        .graphics
        .validate_authoring(&request)
        .await
        .unwrap_err()
        .to_string()
        .contains("missing-variable"));
    request.variables = Some(r#"{"Unknown":"Actual override"}"#.into());
    repos.graphics.validate_authoring(&request).await.unwrap();
    if let GraphicsElementConfig::Text(config) = &mut request.element.config {
        config.renderer = TextGraphicsRenderer::Drawtext;
        config.drawtext.font_file = Some("missing/font.ttf".into());
    }
    assert!(repos
        .graphics
        .validate_authoring(&request)
        .await
        .unwrap_err()
        .to_string()
        .contains("missing-font"));
    request.element.kind = GraphicsElementKind::Image;
    request.element.config = GraphicsElementConfig::Image(ImageGraphicsConfig {
        image: Some("missing/image.png".into()),
    });
    assert!(repos
        .graphics
        .validate_authoring(&request)
        .await
        .unwrap_err()
        .to_string()
        .contains("missing-media"));
    request.element.kind = GraphicsElementKind::Script;
    request.element.config = GraphicsElementConfig::Script(ScriptGraphicsConfig {
        command: Some("while true do end".into()),
        ..Default::default()
    });
    assert!(repos.graphics.validate_authoring(&request).await.is_err());
    assert!(repos.graphics.list_elements().await.unwrap().is_empty());
}
