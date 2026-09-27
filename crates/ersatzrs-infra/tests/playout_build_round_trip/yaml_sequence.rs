use super::*;
use ersatzrs_core::scheduling::SchedulingMediaItem;
use ersatzrs_domain::ChannelSubtitleMode;
use std::fmt::Write as _;

fn fixture() -> (String, SchedulingReferenceData) {
    let mut yaml = String::from("content:\n  - key: main\nsequence:\n  - key: shows\n    items:\n");
    for index in 0..8 {
        writeln!(
            yaml,
            "      - content: main\n        count: '1'\n        custom_title: Show-{index}"
        )
        .unwrap();
    }
    yaml.push_str("reset:\n  - shuffle_sequence: shows\nplayout:\n  - sequence: shows\n");
    let mut media = SchedulingMediaItem::new(1, Duration::minutes(10));
    media.preferred_audio_language_code = Some("nld".to_owned());
    media.preferred_subtitle_language_code = Some("eng".to_owned());
    media.subtitle_mode = Some(ChannelSubtitleMode::Forced);
    let mut reference = SchedulingReferenceData::default();
    reference
        .media_by_key
        .insert("main".to_owned(), vec![media]);
    (yaml, reference)
}

#[tokio::test]
async fn yaml_shuffled_sequence_continues_after_database_restart() {
    let directory = tempfile::tempdir().unwrap();
    let url = format!(
        "sqlite://{}",
        directory.path().join("sequence.db").display()
    );
    let pool = ersatzrs_infra::db::init(&url).await.unwrap();
    pool.execute(r#"INSERT INTO "Resolution" ("Id", "Name", "Width", "Height") VALUES (1, '1920x1080', 1920, 1080)"#)
        .await.unwrap();
    let repos = make_repositories(pool.clone());
    common::seed_profile_and_channel(&repos, 1).await;
    common::seed_media_graph(&pool).await;
    let (yaml, reference) = fixture();
    sqlx::query(r#"INSERT INTO "Playout" ("Id", "ChannelId", "ScheduleKind", "Seed", "ScheduleFile") VALUES (1, 1, 'Sequential', 42, ?)"#)
        .bind(&yaml).execute(&pool).await.unwrap();
    let playout = repos.playout.get_with_anchor(1).await.unwrap();
    let start = common::fixed_time();
    let finish = start + Duration::minutes(80);
    let first = PlayoutBuilder::default()
        .build(SchedulingRequest {
            playout: playout.clone(),
            start,
            finish: start + Duration::minutes(10),
            mode: PlayoutBuildMode::Reset,
            reference_data: reference.clone(),
        })
        .unwrap();
    assert_eq!(first.added_items.len(), 1);
    let state: serde_json::Value =
        serde_json::from_str(first.anchor.as_ref().unwrap().context.as_deref().unwrap()).unwrap();
    let order = state["sequence_orders"][""][0]["order"].as_array().unwrap();
    assert_eq!(order.len(), 8);

    // Replay the generated permutation without another random shuffle, so the
    // uninterrupted oracle and every split build start from the same choice.
    let mut oracle_definition = YamlPlayoutEngine::definition_from_str(&yaml).unwrap();
    let declared = oracle_definition.sequence[0].items.clone();
    oracle_definition.sequence[0].items = order
        .iter()
        .map(|index| declared[usize::try_from(index.as_u64().unwrap()).unwrap()].clone())
        .collect();
    oracle_definition.reset.clear();
    let oracle = YamlPlayoutEngine
        .schedule_definition(
            oracle_definition,
            SchedulingRequest {
                playout: playout.clone(),
                start,
                finish,
                mode: PlayoutBuildMode::Reset,
                reference_data: reference.clone(),
            },
        )
        .unwrap();
    assert_eq!(oracle.added_items.len(), 8);
    assert_eq!(first.added_items, oracle.added_items[..1]);
    repos
        .playout
        .apply_build_result(&first, &playout, Duration::zero())
        .await
        .unwrap();
    drop(repos);
    pool.close().await;

    let pool = ersatzrs_infra::db::init(&url).await.unwrap();
    let repos = make_repositories(pool.clone());
    let loaded = repos.playout.get_with_anchor(1).await.unwrap();
    assert_eq!(
        loaded.anchor.as_ref().unwrap().context,
        first.anchor.as_ref().unwrap().context
    );
    let mut restored = repos
        .scheduling
        .yaml_reference_data(1, &yaml, Duration::zero())
        .await
        .unwrap();
    assert!(!restored.existing_history.is_empty());
    restored.media_by_key = reference.media_by_key;
    let continued = PlayoutBuilder::default()
        .build(SchedulingRequest {
            playout: loaded.clone(),
            start: start + Duration::minutes(10),
            finish,
            mode: PlayoutBuildMode::Continue,
            reference_data: restored,
        })
        .unwrap();
    assert_eq!(continued.added_items, oracle.added_items[1..]);
    repos
        .playout
        .apply_build_result(&continued, &loaded, Duration::zero())
        .await
        .unwrap();
    let persisted = repos
        .playout
        .items_in_range(1, start, finish)
        .await
        .unwrap();
    assert_eq!(persisted.len(), 8);
    for (mut actual, expected) in persisted.into_iter().zip(oracle.added_items) {
        actual.id = expected.id;
        assert_eq!(actual, expected);
    }
    drop(repos);
    pool.close().await;
}
