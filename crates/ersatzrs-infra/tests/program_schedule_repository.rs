mod common;

use chrono::Duration;
use ersatzrs_core::repositories::{
    ChannelRepository, FfmpegProfileRepository, PlayoutRepository, ProgramScheduleRepository,
    ScheduleAdminForm, ScheduleItemAdmin,
};
use ersatzrs_core::scheduling::{SchedulingSmartShufflePolicy, SchedulingSmartShufflePreset};
use ersatzrs_domain::{
    CollectionType, FillWithGroupMode, GuideMode, MarathonGroupBy, MultipleMode, PlaybackOrder,
    PlayoutMode, ProgramScheduleAlternate, ProgramScheduleItem, TailMode,
};
use ersatzrs_infra::repositories::make_repositories;
use sqlx::{Executor, Row};

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
        r#"INSERT OR REPLACE INTO "Playout" ("Id", "ChannelId", "ProgramScheduleId", "ScheduleKind", "Seed")
         VALUES (1, 1, 1, 0, 0)"#,
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

#[tokio::test]
async fn program_schedule_repository_round_trips_typed_items_and_overlays() {
    let pool = common::test_db().await;
    let repos = make_repositories(pool.clone());
    repos
        .program_schedule
        .upsert(&common::schedule(1, Vec::new()))
        .await
        .unwrap();
    seed_overlay_targets(&pool).await;
    seed_filler_targets(&pool).await;

    let item = ProgramScheduleItem {
        id: 30,
        program_schedule_id: 1,
        collection_type: CollectionType::SearchQuery,
        search_query: Some("genre:comedy".to_owned()),
        playout_mode: PlayoutMode::Multiple,
        playback_order: PlaybackOrder::Random,
        multiple_mode: MultipleMode::CollectionSize,
        multiple_count: Some("3".to_owned()),
        guide_mode: GuideMode::Filler,
        fill_with_group_mode: FillWithGroupMode::FillWithOrderedGroups,
        marathon_group_by: MarathonGroupBy::Show,
        marathon_shuffle_groups: true,
        marathon_batch_size: Some(2),
        marathon_shuffle_items: true,
        custom_title: Some("Comedy".to_owned()),
        sort_order: 7,
        watermark_ids: vec![501],
        graphics_element_ids: vec![601],
        graphics_elements: vec![ersatzrs_domain::ProgramScheduleItemGraphicsElement {
            program_schedule_item_id: 30,
            graphics_element_id: 601,
            variables: Some(r#"{"label":"Schedule label"}"#.to_owned()),
        }],
        pre_roll_filler_id: Some(701),
        mid_roll_filler_id: Some(702),
        post_roll_filler_id: Some(703),
        tail_filler_id: Some(704),
        fallback_filler_id: Some(705),
        ..ProgramScheduleItem::default()
    };
    repos.program_schedule.upsert_item(&item).await.unwrap();

    let loaded = repos.program_schedule.get_item(30).await.unwrap();
    assert_eq!(loaded.playout_mode, PlayoutMode::Multiple);
    assert_eq!(loaded.multiple_mode, MultipleMode::CollectionSize);
    assert!(loaded.marathon_shuffle_groups);
    assert_eq!(loaded.marathon_batch_size, Some(2));
    assert_eq!(loaded.multiple_count, Some("3".to_owned()));
    assert_eq!(loaded.search_query, Some("genre:comedy".to_owned()));
    assert_eq!(loaded.watermark_ids, vec![501]);
    assert_eq!(loaded.graphics_element_ids, vec![601]);
    assert_eq!(
        loaded.graphics_elements[0].variables.as_deref(),
        Some(r#"{"label":"Schedule label"}"#)
    );
    assert_eq!(loaded.pre_roll_filler_id, Some(701));
    assert_eq!(loaded.mid_roll_filler_id, Some(702));
    assert_eq!(loaded.post_roll_filler_id, Some(703));
    assert_eq!(loaded.tail_filler_id, Some(704));
    assert_eq!(loaded.fallback_filler_id, Some(705));
    let mut updated = loaded;
    updated.marathon_shuffle_groups = false;
    updated.marathon_batch_size = Some(3);
    repos.program_schedule.upsert_item(&updated).await.unwrap();
    let updated = repos.program_schedule.get_item(30).await.unwrap();
    assert!(!updated.marathon_shuffle_groups);
    assert_eq!(updated.marathon_batch_size, Some(3));

    let duration = ProgramScheduleItem {
        id: 31,
        program_schedule_id: 1,
        playout_mode: PlayoutMode::Duration,
        duration: Some(Duration::minutes(45)),
        tail_mode: TailMode::Filler,
        sort_order: 8,
        ..ProgramScheduleItem::default()
    };
    repos.program_schedule.upsert_item(&duration).await.unwrap();

    let loaded_duration = repos.program_schedule.get_item(31).await.unwrap();
    assert_eq!(loaded_duration.playout_mode, PlayoutMode::Duration);
    assert_eq!(loaded_duration.duration, Some(Duration::minutes(45)));
    assert_eq!(loaded_duration.tail_mode, TailMode::Filler);

    let items = repos.program_schedule.list_items(1).await.unwrap();
    assert_eq!(
        items.iter().map(|item| item.id).collect::<Vec<_>>(),
        vec![30, 31]
    );
}

#[tokio::test]
async fn program_schedule_repository_round_trips_alternates() {
    let pool = common::test_db().await;
    let repos = make_repositories(pool.clone());

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
        r#"INSERT INTO "Playout" ("Id", "ChannelId", "ProgramScheduleId", "ScheduleKind", "Seed")
           VALUES (10, 1, 1, 1, 0)"#,
    )
    .await
    .unwrap();

    repos
        .program_schedule
        .upsert_alternate(&ProgramScheduleAlternate {
            id: 40,
            program_schedule_id: 1,
            playout_id: Some(10),
            sort_order: 2,
            days_of_week: vec!["Monday".to_owned(), "Friday".to_owned()],
            days_of_month: vec![1, 15],
            months_of_year: vec![5, 12],
            limit_to_date_range: true,
            start_month: 5,
            start_day: 1,
            start_year: Some(2026),
            end_month: 12,
            end_day: 31,
            end_year: Some(2026),
        })
        .await
        .unwrap();

    let alternates = repos.program_schedule.list_alternates(1).await.unwrap();
    assert_eq!(alternates.len(), 1);
    assert_eq!(alternates[0].days_of_week, vec!["Monday", "Friday"]);
    assert_eq!(alternates[0].days_of_month, vec![1, 15]);
    assert_eq!(alternates[0].months_of_year, vec![5, 12]);
    assert!(alternates[0].limit_to_date_range);

    // Row145: rejection must roll back the DELETE performed inside the repository.
    assert!(repos
        .playout
        .save_alternate_schedules_admin(10, &[])
        .await
        .is_err());
    assert_eq!(
        repos.program_schedule.list_alternates(1).await.unwrap(),
        alternates
    );
    let default_schedule: i64 =
        sqlx::query_scalar(r#"SELECT "ProgramScheduleId" FROM "Playout" WHERE "Id" = 10"#)
            .fetch_one(&pool)
            .await
            .unwrap();
    assert_eq!(default_schedule, 1);

    let row = sqlx::query(
        r#"SELECT "DaysOfWeek", "DaysOfMonth", "MonthsOfYear"
           FROM "ProgramScheduleAlternate"
           WHERE "Id" = 40"#,
    )
    .fetch_one(&pool)
    .await
    .unwrap();
    assert_eq!(
        row.get::<Option<String>, _>("DaysOfWeek"),
        Some(r#"["Monday","Friday"]"#.to_owned())
    );
    assert_eq!(
        row.get::<Option<String>, _>("DaysOfMonth"),
        Some("1,15".to_owned())
    );
    assert_eq!(
        row.get::<Option<String>, _>("MonthsOfYear"),
        Some("5,12".to_owned())
    );
}

#[tokio::test]
async fn program_schedule_repository_preserves_existing_item_target_fields() {
    let pool = common::test_db().await;
    let repos = make_repositories(pool.clone());

    repos
        .program_schedule
        .upsert(&common::schedule(1, Vec::new()))
        .await
        .unwrap();
    pool.execute(
        r#"INSERT INTO "ProgramScheduleItem" (
            "Id", "ProgramScheduleId", "CollectionType", "PlaybackOrder",
            "FillWithGroupMode", "GuideMode", "MarathonGroupBy",
            "MarathonShuffleGroups", "MarathonShuffleItems", "Index",
            "SearchQuery", "CustomTitle"
         ) VALUES (30, 1, 9, 2, 0, 0, 0, 0, 0, 5, 'genre:comedy', 'Comedy block')"#,
    )
    .await
    .unwrap();

    repos
        .program_schedule
        .upsert(&common::schedule(1, vec![30]))
        .await
        .unwrap();

    let row = sqlx::query(
        r#"SELECT "CollectionType", "PlaybackOrder", "Index", "SearchQuery", "CustomTitle"
           FROM "ProgramScheduleItem"
           WHERE "Id" = 30"#,
    )
    .fetch_one(&pool)
    .await
    .unwrap();

    assert_eq!(row.get::<i64, _>("CollectionType"), 9);
    assert_eq!(row.get::<i64, _>("PlaybackOrder"), 2);
    assert_eq!(row.get::<i64, _>("Index"), 0);
    assert_eq!(
        row.get::<Option<String>, _>("SearchQuery"),
        Some("genre:comedy".to_owned())
    );
    assert_eq!(
        row.get::<Option<String>, _>("CustomTitle"),
        Some("Comedy block".to_owned())
    );
}

#[tokio::test]
async fn program_schedule_repository_replaces_and_copies_complete_admin_items() {
    let pool = common::test_db().await;
    let repos = make_repositories(pool.clone());
    let schedule_id = repos
        .program_schedule
        .save_admin(&ScheduleAdminForm {
            name: "Prime Time".to_owned(),
            ..ScheduleAdminForm::default()
        })
        .await
        .unwrap();
    let policy = SchedulingSmartShufflePolicy::for_preset(SchedulingSmartShufflePreset::Balanced);
    seed_overlay_targets(&pool).await;
    let mut item = admin_search_item(policy.clone());
    item.graphics_element_ids = vec![601];
    item.graphics_element_variables = [(601, r#"{"label":"Copied label"}"#.to_owned())].into();

    repos
        .program_schedule
        .replace_items_admin(schedule_id, &[item])
        .await
        .unwrap();
    let loaded = repos
        .program_schedule
        .list_items_admin(schedule_id)
        .await
        .unwrap();
    assert_eq!(loaded.len(), 1);
    assert_eq!(loaded[0].search_query.as_deref(), Some("genre:comedy"));
    assert_eq!(loaded[0].duration_hours, 1);
    assert_eq!(loaded[0].duration_minutes, 15);
    assert_eq!(loaded[0].watermark_ids, Vec::<i64>::new());
    assert_eq!(loaded[0].smart_shuffle, policy);
    assert_eq!(
        loaded[0].graphics_element_variables[&601],
        r#"{"label":"Copied label"}"#
    );
    assert_eq!(
        loaded[0].pinned_stream_selection,
        Some(ersatzrs_domain::PinnedStreamSelection {
            media_version_id: 901,
            video_stream_index: 0,
            audio_stream_index: Some(1),
            subtitle_stream_index: Some(2),
        })
    );

    let copy_id = repos
        .program_schedule
        .copy_admin(schedule_id, "Prime Time Copy")
        .await
        .unwrap();
    let copied = repos
        .program_schedule
        .list_items_admin(copy_id)
        .await
        .unwrap();
    assert_eq!(copied.len(), 1);
    assert_eq!(copied[0].search_query, loaded[0].search_query);
    assert_eq!(copied[0].smart_shuffle, loaded[0].smart_shuffle);
    assert_eq!(
        copied[0].graphics_element_variables,
        loaded[0].graphics_element_variables
    );
    assert_eq!(
        copied[0].pinned_stream_selection,
        loaded[0].pinned_stream_selection
    );
}

#[tokio::test]
async fn program_schedule_admin_reads_report_closed_pool_errors() {
    let pool = common::test_db().await;
    let repos = make_repositories(pool.clone());
    pool.close().await;

    assert!(repos.program_schedule.list_admin("", 0, 10).await.is_err());
    assert!(repos.program_schedule.list_items_admin(1).await.is_err());
    assert!(repos
        .program_schedule
        .get_item_options_admin()
        .await
        .is_err());
}

fn admin_search_item(smart_shuffle: SchedulingSmartShufflePolicy) -> ScheduleItemAdmin {
    ScheduleItemAdmin {
        id: 0,
        index: 0,
        start_type: 1,
        start_time: None,
        fixed_start_time_behavior: None,
        collection_type: 9,
        collection_id: None,
        media_item_id: None,
        multi_collection_id: None,
        smart_collection_id: None,
        rerun_collection_id: None,
        playlist_id: None,
        search_title: Some("Comedy".to_owned()),
        search_query: Some("genre:comedy".to_owned()),
        collection_name: "Comedy".to_owned(),
        playback_order: 3,
        smart_shuffle,
        inherited_legacy_smart_shuffle: false,
        marathon_group_by: 0,
        marathon_shuffle_groups: false,
        marathon_shuffle_items: false,
        marathon_batch_size: None,
        fill_with_group_mode: 0,
        playout_mode: 4,
        multiple_mode: 0,
        multiple_count: "1".to_owned(),
        duration_hours: 1,
        duration_minutes: 15,
        tail_mode: 0,
        discard_to_fill_attempts: Some(3),
        custom_title: "Comedy Hour".to_owned(),
        guide_mode: 0,
        pre_roll_filler_id: None,
        mid_roll_filler_id: None,
        post_roll_filler_id: None,
        tail_filler_id: None,
        fallback_filler_id: None,
        watermark_ids: Vec::new(),
        graphics_element_ids: Vec::new(),
        graphics_element_variables: std::collections::BTreeMap::default(),
        preferred_audio_language_code: Some("eng".to_owned()),
        preferred_audio_title: String::new(),
        preferred_subtitle_language_code: None,
        subtitle_mode: None,
        pinned_stream_selection: Some(ersatzrs_domain::PinnedStreamSelection {
            media_version_id: 901,
            video_stream_index: 0,
            audio_stream_index: Some(1),
            subtitle_stream_index: Some(2),
        }),
    }
}

async fn seed_overlay_targets(pool: &sqlx::SqlitePool) {
    pool.execute(
        r#"INSERT INTO "ChannelWatermark" (
            "Id", "DurationSeconds", "FrequencyMinutes", "HorizontalMarginPercent",
            "ImageSource", "Location", "Mode", "Name", "Opacity",
            "PlaceWithinSourceContent", "Size", "VerticalMarginPercent",
            "WidthPercent", "ZIndex"
        ) VALUES (501, 30, 10, 0.0, 0, 0, 1, 'wm', 100, 0, 0, 0.0, 10.0, 0)"#,
    )
    .await
    .unwrap();
    pool.execute(r#"INSERT INTO "GraphicsElement" ("Id", "Kind", "Name") VALUES (601, 1, 'ge')"#)
        .await
        .unwrap();
}

async fn seed_filler_targets(pool: &sqlx::SqlitePool) {
    for (id, kind) in [
        (701, "PreRoll"),
        (702, "MidRoll"),
        (703, "PostRoll"),
        (704, "Tail"),
        (705, "Fallback"),
    ] {
        sqlx::query(
            r#"INSERT INTO "FillerPreset" (
                "Id", "Name", "FillerKind", "FillerMode", "CollectionType",
                "AllowWatermarks", "UseChaptersAsMediaItems"
            ) VALUES (?, ?, ?, 'Count', 'Collection', 1, 0)"#,
        )
        .bind(id)
        .bind(format!("filler {id}"))
        .bind(kind)
        .execute(pool)
        .await
        .unwrap();
    }
}
