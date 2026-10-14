use super::*;
use ersatzrs_core::scheduling::SchedulingMediaItem;

fn fixture() -> (String, SchedulingReferenceData) {
    let yaml = r"
content:
  - key: main
  - key: ident
sequence:
  - key: break
    items:
      - content: ident
        count: '1'
        disable_watermarks: true
playout:
  - post_roll: true
    sequence: break
  - mid_roll: true
    sequence: break
    expression: 'true'
  - graphics_on: lower-third
    variables:
      title: Next
  - content: main
    count: '1'
  - content: main
    count: '1'
  - content: main
    count: '1'
"
    .to_owned();
    let mut reference = SchedulingReferenceData::default();
    let mut main = SchedulingMediaItem::new(1, Duration::minutes(10));
    main.preferred_audio_language_code = Some("nld".to_owned());
    main.preferred_subtitle_language_code = Some("eng".to_owned());
    reference.media_by_key.insert("main".to_owned(), vec![main]);
    reference.media_by_key.insert(
        "ident".to_owned(),
        vec![SchedulingMediaItem::new(2, Duration::minutes(1))],
    );
    reference
        .graphics_ids_by_path
        .insert("lower-third".to_owned(), 7);
    (yaml, reference)
}

async fn seed(pool: &sqlx::SqlitePool, yaml: &str) {
    pool.execute(r#"INSERT OR IGNORE INTO "Resolution" ("Id", "Name", "Width", "Height") VALUES (1, '1920x1080', 1920, 1080)"#).await.unwrap();
    let repos = make_repositories(pool.clone());
    common::seed_profile_and_channel(&repos, 1).await;
    common::seed_media_graph(pool).await;
    common::seed_media_item(pool, 2, 1).await;
    repos
        .graphics
        .upsert_element(&GraphicsElement {
            id: 7,
            name: "lower-third".to_owned(),
            path: Some("lower-third".to_owned()),
            kind: GraphicsElementKind::Text,
            config: GraphicsElementConfig::Text(TextGraphicsConfig::default()),
            ..GraphicsElement::default()
        })
        .await
        .unwrap();
    sqlx::query(r#"INSERT INTO "Playout" ("Id", "ChannelId", "ScheduleKind", "Seed", "ScheduleFile") VALUES (1, 1, 'Sequential', 42, ?)"#)
        .bind(yaml).execute(pool).await.unwrap();
}

#[tokio::test]
async fn yaml_ambient_state_survives_repeated_database_restarts() {
    let (yaml, reference) = fixture();
    restart_roundtrip(yaml, reference, 3).await;
}

#[tokio::test]
async fn yaml_nested_filler_survives_repeated_database_restarts() {
    let (yaml, mut reference) = fixture();
    let yaml = yaml.replace(
        "  - key: break\n    items:\n      - content: ident",
        "  - key: break\n    items:\n      - sequence: nested\n        repeat: 2\n        custom_title: Up Next\n  - key: nested\n    items:\n      - content: ident",
    );
    reference.media_by_key.insert(
        "ident".to_owned(),
        [2, 3, 4]
            .map(|id| SchedulingMediaItem::new(id, Duration::minutes(1)))
            .to_vec(),
    );
    restart_roundtrip(yaml, reference, 5).await;
}

async fn restart_roundtrip(yaml: String, reference: SchedulingReferenceData, unit_length: usize) {
    let directory = tempfile::tempdir().unwrap();
    let url = format!("sqlite://{}", directory.path().join("ambient.db").display());
    let pool = ersatzrs_infra::db::init(&url).await.unwrap();
    seed(&pool, &yaml).await;
    for id in [3, 4] {
        common::seed_media_item(&pool, id, 1).await;
    }
    let repos = make_repositories(pool.clone());
    let playout = repos.playout.get_with_anchor(1).await.unwrap();
    let start = common::fixed_time()
        .with_hour(23)
        .unwrap()
        .with_minute(55)
        .unwrap();
    let oracle = PlayoutBuilder::default()
        .build(SchedulingRequest {
            playout,
            start,
            finish: start + Duration::minutes(35),
            mode: PlayoutBuildMode::Reset,
            reference_data: reference.clone(),
        })
        .unwrap();
    assert_eq!(oracle.added_items.len(), 3 * unit_length);
    drop(repos);
    pool.close().await;
    for build in 0..3 {
        let pool = ersatzrs_infra::db::init(&url).await.unwrap();
        let repos = make_repositories(pool.clone());
        let loaded = repos.playout.get_with_anchor(1).await.unwrap();
        let mut restored = repos
            .scheduling
            .yaml_reference_data(1, &yaml, Duration::zero())
            .await
            .unwrap();
        restored.media_by_key = reference.media_by_key.clone();
        let at = loaded
            .anchor
            .as_ref()
            .map_or(start, |anchor| anchor.next_start);
        let result = PlayoutBuilder::default()
            .build(SchedulingRequest {
                playout: loaded.clone(),
                start: at,
                finish: at + Duration::minutes(1),
                mode: if build == 0 {
                    PlayoutBuildMode::Reset
                } else {
                    PlayoutBuildMode::Continue
                },
                reference_data: restored,
            })
            .unwrap();
        assert_eq!(result.clear_items, build == 0);
        assert_eq!(
            result.added_items,
            oracle.added_items[build * unit_length..(build + 1) * unit_length]
        );
        let mid_end = 1 + (unit_length - 1) / 2;
        assert!(result.added_items[1..mid_end]
            .iter()
            .all(|item| item.filler_kind == FillerKind::MidRoll));
        assert!(result.added_items[mid_end..]
            .iter()
            .all(|item| item.filler_kind == FillerKind::PostRoll));
        if unit_length == 5 {
            assert!(result.added_items[1..]
                .iter()
                .all(|item| item.custom_title.as_deref() == Some("Up Next")));
        }
        assert!(result.added_items[1..]
            .iter()
            .all(|item| item.disable_watermarks));
        repos
            .playout
            .apply_build_result(&result, &loaded, Duration::zero())
            .await
            .unwrap();
        let persisted = repos
            .playout
            .items_in_range(1, at, result.anchor.as_ref().unwrap().next_start)
            .await
            .unwrap();
        assert_eq!(persisted.len(), unit_length);
        assert_persisted_items(&persisted, &result.added_items);
        drop(repos);
        pool.close().await;
    }
}

#[tokio::test]
async fn yaml_ambient_graphics_lookup_covers_imports_reset_and_named_lists() {
    let pool = common::test_db().await;
    seed(&pool, "{}").await;
    let repos = make_repositories(pool.clone());
    for (id, path) in [(8, "named-off"), (9, "imported")] {
        repos
            .graphics
            .upsert_element(&GraphicsElement {
                id,
                name: path.to_owned(),
                path: Some(path.to_owned()),
                kind: GraphicsElementKind::Text,
                config: GraphicsElementConfig::Text(TextGraphicsConfig::default()),
                ..GraphicsElement::default()
            })
            .await
            .unwrap();
    }
    let directory = tempfile::tempdir().unwrap();
    std::fs::write(
        directory.path().join("import.yaml"),
        "sequence:\n  - key: overlay\n    items:\n      - graphics_on: imported\n",
    )
    .unwrap();
    let yaml = "import: [import.yaml]\nreset:\n  - graphics_on: lower-third\nplayout:\n  - graphics_on: missing\nschedules:\n  - name: Holiday\n    start_date: 12-25\n    end_date: 12-25\n    playout:\n      - graphics_off: named-off\n";
    let schedule = directory.path().join("schedule.yaml");
    std::fs::write(&schedule, yaml).unwrap();
    let loaded = repos
        .scheduling
        .yaml_reference_data(1, schedule.to_str().unwrap(), Duration::zero())
        .await
        .unwrap();
    assert_eq!(
        loaded.graphics_ids_by_path,
        std::collections::BTreeMap::from([
            ("lower-third".to_owned(), 7),
            ("named-off".to_owned(), 8),
            ("imported".to_owned(), 9)
        ])
    );
}

fn assert_persisted_items(
    actual: &[ersatzrs_domain::PlayoutItem],
    expected: &[ersatzrs_domain::PlayoutItem],
) {
    assert_eq!(actual.len(), expected.len());
    for (actual, expected) in actual.iter().zip(expected) {
        let mut actual = actual.clone();
        actual.id = expected.id;
        for graphic in &mut actual.graphics_elements {
            graphic.playout_item_id = expected.id;
        }
        assert_eq!(&actual, expected);
    }
}
