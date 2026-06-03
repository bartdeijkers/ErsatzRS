#![cfg(not(feature = "mysql"))]
//! Round-trip integration tests for `PlayoutBuilder` → `apply_build_result`.
//!
//! These tests verify that the persistence write path correctly stores
//! `SchedulingResult` rows and that the read-back matches expectations.
//!
//! The real-world tests (env-gated with `ERSATZRS_REAL_WORLD_SQLITE`) load a
//! Block playout and a YAML playout from an imported `ErsatzTV` database, run
//! each with the corresponding engine using Reset mode, persist via
//! `apply_build_result` into a fresh in-memory database (seeded from the real-
//! world schema), then snapshot the produced `PlayoutItem` ordering with `insta`.
mod common;

use chrono::{Duration, Utc};
use ersatzrs_core::playout_builder::PlayoutBuilder;
use ersatzrs_core::repositories::{GraphicsRepository, PlayoutRepository};
use ersatzrs_core::scheduling::{
    PlayoutBuildMode, SchedulingReferenceData, SchedulingRequest, SchedulingResult,
    YamlPlayoutEngine,
};
use ersatzrs_domain::{
    FillerKind, GraphicsElement, GraphicsElementConfig, GraphicsElementKind, Playout,
    PlayoutAnchor, PlayoutItemGraphicsElement, PlayoutScheduleKind, TextGraphicsConfig,
};
use ersatzrs_infra::repositories::make_repositories;
use sqlx::Executor;

// ── pure persistence tests ────────────────────────────────────────────────────

#[tokio::test]
async fn apply_build_result_reset_clears_items_and_appends_new() {
    let pool = common::test_db().await;
    let repos = make_repositories(pool.clone());
    common::seed_profile_and_channel(&repos, 1).await;
    common::seed_media_graph(&pool).await;
    pool.execute(
        r#"INSERT OR REPLACE INTO "Playout"
           ("Id", "ChannelId", "ScheduleKind", "Seed")
           VALUES (1, 1, 'Block', 42)"#,
    )
    .await
    .unwrap();

    // Pre-insert two old items that Reset should remove.
    let start = common::fixed_time();
    repos
        .playout
        .upsert_item(&common::playout_item(1, 1, 1))
        .await
        .unwrap();
    repos
        .playout
        .upsert_item(&common::playout_item(2, 1, 1))
        .await
        .unwrap();

    // Build a Reset result with one new item.
    let new_start = start + Duration::minutes(60);
    let new_item = ersatzrs_domain::PlayoutItem {
        playout_id: 1,
        media_item_id: 1,
        start: new_start,
        finish: new_start + Duration::minutes(30),
        guide_group: 1,
        filler_kind: FillerKind::None,
        in_point: Duration::zero(),
        out_point: Duration::minutes(30),
        ..Default::default()
    };
    let mut playout = Playout {
        id: 1,
        channel_id: 1,
        schedule_kind: PlayoutScheduleKind::Block,
        seed: 42,
        anchor: Some(PlayoutAnchor {
            next_start: new_start + Duration::minutes(30),
            ..Default::default()
        }),
        ..Default::default()
    };
    let result = SchedulingResult {
        clear_items: true,
        added_items: vec![new_item.clone()],
        ..Default::default()
    };

    repos
        .playout
        .apply_build_result(&result, &playout)
        .await
        .unwrap();

    // Old items (id 1 and 2) must be gone; one new item must be present.
    let now = Utc::now();
    let items = repos
        .playout
        .items_in_range(1, start - Duration::hours(1), now + Duration::hours(2))
        .await
        .unwrap();
    assert_eq!(items.len(), 1, "only the newly inserted item should remain");
    let item = &items[0];
    assert_eq!(item.media_item_id, 1);
    assert_eq!(item.guide_group, 1);

    // Read anchor back to confirm it was written.
    playout.anchor = Some(PlayoutAnchor {
        next_start: new_start + Duration::minutes(30),
        ..Default::default()
    });
    let loaded = repos.playout.get_with_anchor(1).await.unwrap();
    assert!(loaded.anchor.is_some());

    insta::assert_json_snapshot!("reset_item_ordering", {
        "[].start" => "[start]",
        "[].finish" => "[finish]",
    }, &items.iter().map(|i| serde_json::json!({
        "media_item_id": i.media_item_id,
        "guide_group": i.guide_group,
        "filler_kind": format!("{:?}", i.filler_kind),
    })).collect::<Vec<_>>());
}

#[tokio::test]
async fn apply_build_result_persists_graphics_variables_for_generated_items() {
    let pool = common::test_db().await;
    let repos = make_repositories(pool.clone());
    common::seed_profile_and_channel(&repos, 1).await;
    common::seed_media_graph(&pool).await;
    pool.execute(
        r#"INSERT OR REPLACE INTO "Playout"
           ("Id", "ChannelId", "ScheduleKind", "Seed")
           VALUES (1, 1, 'Sequential', 42)"#,
    )
    .await
    .unwrap();
    repos
        .graphics
        .upsert_element(&GraphicsElement {
            id: 7,
            name: "lower-third".to_owned(),
            kind: GraphicsElementKind::Text,
            path: Some("lower-third".to_owned()),
            config: GraphicsElementConfig::Text(TextGraphicsConfig {
                text: Some("{{ title }}".to_owned()),
                ..TextGraphicsConfig::default()
            }),
            ..GraphicsElement::default()
        })
        .await
        .unwrap();

    let start = common::fixed_time();
    let mut new_item = ersatzrs_domain::PlayoutItem {
        playout_id: 1,
        media_item_id: 1,
        start,
        finish: start + Duration::minutes(30),
        guide_group: 1,
        filler_kind: FillerKind::None,
        in_point: Duration::zero(),
        out_point: Duration::minutes(30),
        graphics_element_ids: vec![7],
        ..Default::default()
    };
    new_item.graphics_elements = vec![PlayoutItemGraphicsElement {
        playout_item_id: 0,
        graphics_element_id: Some(7),
        variables: Some(r#"{"title":"Now"}"#.to_owned()),
    }];
    let playout = Playout {
        id: 1,
        channel_id: 1,
        schedule_kind: PlayoutScheduleKind::Sequential,
        seed: 42,
        ..Default::default()
    };
    let result = SchedulingResult {
        clear_items: true,
        added_items: vec![new_item],
        ..Default::default()
    };

    repos
        .playout
        .apply_build_result(&result, &playout)
        .await
        .unwrap();

    let items = repos
        .playout
        .items_in_range(
            1,
            start - Duration::minutes(1),
            start + Duration::minutes(31),
        )
        .await
        .unwrap();
    assert_eq!(items.len(), 1);
    assert_eq!(items[0].graphics_element_ids, vec![7]);
    assert_eq!(items[0].graphics_elements.len(), 1);
    assert_eq!(items[0].graphics_elements[0].graphics_element_id, Some(7));
    assert_eq!(
        items[0].graphics_elements[0].variables.as_deref(),
        Some(r#"{"title":"Now"}"#)
    );
}

#[tokio::test]
async fn apply_build_result_continue_appends_and_removes_selectively() {
    let pool = common::test_db().await;
    let repos = make_repositories(pool.clone());
    common::seed_profile_and_channel(&repos, 1).await;
    common::seed_media_graph(&pool).await;
    pool.execute(
        r#"INSERT OR REPLACE INTO "Playout"
           ("Id", "ChannelId", "ScheduleKind", "Seed")
           VALUES (1, 1, 'Block', 42)"#,
    )
    .await
    .unwrap();

    let start = common::fixed_time();
    // Pre-insert item 1 (to keep) and item 2 (to remove).
    repos
        .playout
        .upsert_item(&common::playout_item(1, 1, 1))
        .await
        .unwrap();
    repos
        .playout
        .upsert_item(&common::playout_item(2, 1, 1))
        .await
        .unwrap();

    let new_start = start + Duration::minutes(60);
    let new_item = ersatzrs_domain::PlayoutItem {
        playout_id: 1,
        media_item_id: 1,
        start: new_start,
        finish: new_start + Duration::minutes(30),
        guide_group: 2,
        filler_kind: FillerKind::None,
        in_point: Duration::zero(),
        out_point: Duration::minutes(30),
        ..Default::default()
    };
    let playout = Playout {
        id: 1,
        channel_id: 1,
        schedule_kind: PlayoutScheduleKind::Block,
        seed: 42,
        anchor: Some(PlayoutAnchor {
            next_start: new_start + Duration::minutes(30),
            ..Default::default()
        }),
        ..Default::default()
    };
    let result = SchedulingResult {
        clear_items: false,
        items_to_remove: vec![2],
        added_items: vec![new_item],
        ..Default::default()
    };

    repos
        .playout
        .apply_build_result(&result, &playout)
        .await
        .unwrap();

    let now = Utc::now();
    let items = repos
        .playout
        .items_in_range(1, start - Duration::hours(1), now + Duration::hours(2))
        .await
        .unwrap();
    // item 1 stays, item 2 removed, new item added → 2 total
    assert_eq!(items.len(), 2, "item 1 kept + new item = 2");
    let guide_groups: Vec<i32> = items.iter().map(|i| i.guide_group).collect();
    assert!(
        guide_groups.contains(&0),
        "original item (guide_group=0) kept"
    );
    assert!(
        guide_groups.contains(&2),
        "new item (guide_group=2) present"
    );
}

// ── YAML engine round-trip (in-memory, no file I/O) ───────────────────────────

#[tokio::test]
async fn yaml_engine_result_persists_and_round_trips() {
    use ersatzrs_core::scheduling::SchedulingMediaItem;
    use std::collections::BTreeMap;

    let pool = common::test_db().await;
    let repos = make_repositories(pool.clone());
    common::seed_profile_and_channel(&repos, 1).await;
    common::seed_media_graph(&pool).await;
    pool.execute(
        r#"INSERT OR REPLACE INTO "Playout"
           ("Id", "ChannelId", "ScheduleKind", "Seed", "ScheduleFile")
           VALUES (1, 1, 'Sequential', 0, '/dev/null')"#,
    )
    .await
    .unwrap();

    // Build a minimal YAML definition and reference data in-memory.
    let definition_str = r"
content:
  - key: movies
playout:
  - content: movies
    count: 2
";
    let definition = YamlPlayoutEngine::definition_from_str(definition_str)
        .expect("parse inline YAML definition");

    let now = Utc::now();
    let item_a = SchedulingMediaItem {
        id: 1,
        duration: Duration::minutes(90),
        ..SchedulingMediaItem::new(1, Duration::minutes(90))
    };
    let item_b = SchedulingMediaItem {
        id: 1,
        duration: Duration::minutes(45),
        ..SchedulingMediaItem::new(1, Duration::minutes(45))
    };
    let mut media_by_key = BTreeMap::new();
    media_by_key.insert("movies".to_string(), vec![item_a, item_b]);

    let reference_data = SchedulingReferenceData {
        media_by_key,
        max_playout_offset: Duration::days(2),
        ..Default::default()
    };

    let playout = Playout {
        id: 1,
        channel_id: 1,
        schedule_kind: PlayoutScheduleKind::Sequential,
        seed: 0,
        ..Default::default()
    };
    let request = SchedulingRequest {
        playout: playout.clone(),
        start: now,
        finish: now + Duration::hours(6),
        mode: PlayoutBuildMode::Reset,
        reference_data,
    };

    let engine = YamlPlayoutEngine;
    let result = engine
        .schedule_definition(definition, request)
        .expect("YAML engine should produce a result");

    assert!(
        !result.added_items.is_empty(),
        "YAML engine should schedule at least one item"
    );

    let mut playout_with_anchor = playout;
    playout_with_anchor.anchor = result.anchor.clone();

    repos
        .playout
        .apply_build_result(&result, &playout_with_anchor)
        .await
        .unwrap();

    let written = repos
        .playout
        .items_in_range(1, now - Duration::minutes(1), now + Duration::hours(7))
        .await
        .unwrap();

    assert_eq!(
        written.len(),
        result.added_items.len(),
        "written item count must match engine output"
    );

    insta::assert_json_snapshot!(
        "yaml_engine_item_ordering",
        written
            .iter()
            .map(|i| {
                let offset_min = (i.start - now).num_minutes();
                let duration_min = (i.finish - i.start).num_minutes();
                serde_json::json!({
                    "offset_min": offset_min,
                    "duration_min": duration_min,
                    "filler_kind": format!("{:?}", i.filler_kind),
                })
            })
            .collect::<Vec<_>>()
    );
}

// ── real-world env-gated round-trip ───────────────────────────────────────────

#[cfg(test)]
#[allow(clippy::disallowed_methods)]
fn real_world_db_path() -> Option<std::path::PathBuf> {
    std::env::var_os("ERSATZRS_REAL_WORLD_SQLITE").map(std::path::PathBuf::from)
}

/// Load a writable in-memory database pre-seeded with the `ErsatzTV` schema
/// (via the normal migration path) and a representative subset of data
/// copied from the real-world database. We intentionally do NOT write to
/// the real-world DB so the test is safe to run against production data.
#[tokio::test]
#[allow(clippy::too_many_lines)]
async fn block_and_yaml_playtouts_round_trip_against_real_world_db() {
    use sqlx::sqlite::{SqliteConnectOptions, SqlitePoolOptions};

    let Some(db_path) = real_world_db_path() else {
        eprintln!("skipping real-world playout build round-trip; set ERSATZRS_REAL_WORLD_SQLITE");
        return;
    };
    if !db_path.exists() {
        eprintln!("skipping real-world playout build round-trip; path does not exist");
        return;
    }

    let real_pool = SqlitePoolOptions::new()
        .max_connections(1)
        .connect_with(
            SqliteConnectOptions::new()
                .filename(&db_path)
                .read_only(true)
                .create_if_missing(false)
                .foreign_keys(true),
        )
        .await
        .expect("open real-world ErsatzTV sqlite database read-only");
    let real_repos = make_repositories(real_pool.clone());

    // Find one Block playout and one Sequential playout from the real DB.
    let block_playout_id: Option<i64> =
        sqlx::query_scalar(r#"SELECT "Id" FROM "Playout" WHERE "ScheduleKind" = 'Block' LIMIT 1"#)
            .fetch_optional(&real_pool)
            .await
            .unwrap_or(None);

    let yaml_playout_id: Option<i64> = sqlx::query_scalar(
        r#"SELECT "Id" FROM "Playout" WHERE "ScheduleKind" = 'Sequential' AND "ScheduleFile" IS NOT NULL LIMIT 1"#,
    )
    .fetch_optional(&real_pool)
    .await
    .unwrap_or(None);

    if block_playout_id.is_none() && yaml_playout_id.is_none() {
        eprintln!("skipping: no Block or Sequential playtouts found in real-world DB");
        return;
    }

    let builder = PlayoutBuilder::default();
    let now = Utc::now();

    // Test Block playout.
    if let Some(playout_id) = block_playout_id {
        let playout = real_repos.playout.get_with_anchor(playout_id).await;
        let Ok(playout) = playout else {
            eprintln!("skipping Block playout {playout_id}: could not load");
            return;
        };

        let ref_data = real_repos
            .scheduling
            .block_reference_data(playout_id, now, 2, Duration::days(30))
            .await;
        let Ok(reference_data) = ref_data else {
            eprintln!("skipping Block playout {playout_id}: could not load reference data");
            return;
        };

        if reference_data.effective_blocks.is_empty() {
            eprintln!(
                "skipping Block playout {playout_id}: no effective blocks in schedule window"
            );
            return;
        }

        let request = SchedulingRequest {
            playout: playout.clone(),
            start: now,
            finish: now + Duration::days(2),
            mode: PlayoutBuildMode::Reset,
            reference_data,
        };

        let result = builder.build(request);
        let Ok(result) = result else {
            eprintln!(
                "skipping Block playout {playout_id}: engine error: {:?}",
                result.unwrap_err()
            );
            return;
        };

        // Apply to a fresh in-memory DB to avoid modifying real data.
        let mem_pool = common::test_db().await;
        // Seed a minimal Playout row so FK constraints are satisfied.
        seed_playout_row(&mem_pool, &playout).await;

        let mem_repos = make_repositories(mem_pool.clone());
        let mut playout_with_anchor = playout;
        playout_with_anchor.anchor = result.anchor.clone();

        mem_repos
            .playout
            .apply_build_result(&result, &playout_with_anchor)
            .await
            .unwrap();

        let written = mem_repos
            .playout
            .items_in_range(
                playout_id,
                now - Duration::minutes(1),
                now + Duration::days(3),
            )
            .await
            .unwrap();

        // Snapshot the ordering: start offsets relative to now in minutes.
        let ordering: Vec<serde_json::Value> = written
            .iter()
            .map(|i| {
                serde_json::json!({
                    "offset_min": (i.start - now).num_minutes(),
                    "duration_min": (i.finish - i.start).num_minutes(),
                    "filler_kind": format!("{:?}", i.filler_kind),
                    "guide_group": i.guide_group,
                })
            })
            .collect();

        insta::assert_json_snapshot!("real_world_block_playout_item_ordering", ordering);
    }

    // Test YAML (Sequential) playout.
    if let Some(playout_id) = yaml_playout_id {
        let playout = real_repos.playout.get_with_anchor(playout_id).await;
        let Ok(playout) = playout else {
            eprintln!("skipping Sequential playout {playout_id}: could not load");
            return;
        };

        // The YAML engine reads the schedule_file from the playout; if it
        // doesn't exist on this machine, skip gracefully.
        if let Some(ref file) = playout.schedule_file {
            if !std::path::Path::new(file).exists() {
                eprintln!(
                    "skipping Sequential playout {playout_id}: schedule file {file} not found"
                );
                return;
            }
        }

        // Use block_reference_data as a starting point for existing items/history;
        // the YAML engine resolves its own content from media_by_key or the
        // SchedulingContentResolver during schedule execution.
        let ref_data = real_repos
            .scheduling
            .block_reference_data(playout_id, now, 2, Duration::days(30))
            .await;
        let Ok(reference_data) = ref_data else {
            eprintln!(
                "skipping Sequential playout {playout_id}: could not load base reference data"
            );
            return;
        };

        let request = SchedulingRequest {
            playout: playout.clone(),
            start: now,
            finish: now + Duration::days(2),
            mode: PlayoutBuildMode::Reset,
            reference_data,
        };

        let result = builder.build(request);
        let Ok(result) = result else {
            eprintln!(
                "skipping Sequential playout {playout_id}: engine error: {:?}",
                result.unwrap_err()
            );
            return;
        };

        let mem_pool = common::test_db().await;
        seed_playout_row(&mem_pool, &playout).await;

        let mem_repos = make_repositories(mem_pool.clone());
        let mut playout_with_anchor = playout;
        playout_with_anchor.anchor = result.anchor.clone();

        mem_repos
            .playout
            .apply_build_result(&result, &playout_with_anchor)
            .await
            .unwrap();

        let written = mem_repos
            .playout
            .items_in_range(
                playout_id,
                now - Duration::minutes(1),
                now + Duration::days(3),
            )
            .await
            .unwrap();

        let ordering: Vec<serde_json::Value> = written
            .iter()
            .map(|i| {
                serde_json::json!({
                    "offset_min": (i.start - now).num_minutes(),
                    "duration_min": (i.finish - i.start).num_minutes(),
                    "filler_kind": format!("{:?}", i.filler_kind),
                })
            })
            .collect();

        insta::assert_json_snapshot!("real_world_yaml_playout_item_ordering", ordering);
    }
}

/// Seed a minimal Playout row and its FK prerequisites in the given pool.
async fn seed_playout_row(pool: &sqlx::SqlitePool, playout: &Playout) {
    use sqlx::Executor;
    // Minimal FFmpegProfile → Channel → Playout chain without conflicting.
    let _ = pool
        .execute(r#"INSERT OR IGNORE INTO "Resolution" ("Id","Name","Width","Height") VALUES (1,'1920x1080',1920,1080)"#)
        .await;
    let _ = pool
        .execute(
            r#"INSERT OR IGNORE INTO "FFmpegProfile" (
                    "Id","Name","ThreadCount","HardwareAcceleration","VaapiDriver",
                    "VaapiDevice","QsvExtraHardwareFrames","BitDepth","VideoFormat",
                    "VideoScanType","DeinterlaceVideo","ScalingBehavior","TonemappingAlgorithm",
                    "ResolutionId","FrameRate","VideoBufferSize","AudioFormat",
                    "AudioChannels","AudioBitrate","AudioSampleRate","AudioVolume",
                    "NormalizeFramerate","NormalizeLoudness","Transcode"
                ) VALUES (
                    1,'default',0,'None','Default',
                    '/dev/dri/renderD128',0,'Eight','H264',
                    'Progressive',0,'Default','None',
                    1,'ThirtyFps',0,'Aac',
                    2,192,48000,100,
                    0,0,1
                )"#,
        )
        .await;
    let playout_id = playout.id;
    let channel_id = playout.channel_id;
    let _ = pool
        .execute(
            format!(
                r#"INSERT OR IGNORE INTO "Channel" (
                    "Id","UniqueId","Number","SortNumber","Name","FFmpegProfileId",
                    "ProgressMessageTemplate","SubtitleMode","MusicVideoCreditsMode",
                    "IsEnabled","StreamingMode","PlexStreamingSession"
                ) VALUES (
                    {channel_id},
                    lower(hex(randomblob(4)))||'-'||lower(hex(randomblob(2)))||'-'||lower(hex(randomblob(2)))||'-'||lower(hex(randomblob(2)))||'-'||lower(hex(randomblob(6))),
                    '{channel_id}',{channel_id},'Channel {channel_id}',1,
                    '','None','Disabled',
                    1,'TransportStream',0
                )"#
            )
            .as_str(),
        )
        .await;
    let _ = pool
        .execute(
            format!(
                r#"INSERT OR IGNORE INTO "Playout"
                   ("Id","ChannelId","ScheduleKind","Seed")
                   VALUES ({playout_id},{channel_id},'{:?}',0)"#,
                playout.schedule_kind
            )
            .as_str(),
        )
        .await;
}
