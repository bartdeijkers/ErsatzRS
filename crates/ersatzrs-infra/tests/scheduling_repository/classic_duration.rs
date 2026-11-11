//! Row287 uses saved settings and generated, transactionally published checkpoints.
use super::*;
use ersatzrs_core::{
    playout_builder::PlayoutBuilder,
    repositories::PlayoutRepository,
    scheduling::{PlayoutBuildMode, SchedulingResult},
};
use ersatzrs_domain::TailMode;

async fn database(url: &str, attempts: i32) -> SqlitePool {
    let pool = ersatzrs_infra::db::init(url).await.unwrap();
    pool.execute(r#"INSERT INTO "Resolution" ("Id", "Name", "Width", "Height") VALUES (1, 'test', 1920, 1080)"#).await.unwrap();
    seed_scheduling_fixture(&pool).await;
    pool.execute(
        r#"INSERT INTO "CollectionItem" ("CollectionId", "MediaItemId") VALUES (1, 21);
        UPDATE "MediaVersion" SET "Duration"='00:40:00' WHERE "Id" IN (120, 130, 200);
        UPDATE "MediaVersion" SET "Duration"='00:15:00' WHERE "Id"=210;"#,
    )
    .await
    .unwrap();
    let repos = make_repositories(pool.clone());
    common::seed_profile_and_channel(&repos, 901).await;
    repos
        .program_schedule
        .upsert(&ProgramSchedule {
            id: 301,
            name: "duration".into(),
            ..ProgramSchedule::default()
        })
        .await
        .unwrap();
    for item in [
        ProgramScheduleItem {
            id: 401,
            program_schedule_id: 301,
            collection_type: CollectionType::Collection,
            collection_id: Some(1),
            playback_order: PlaybackOrder::Chronological,
            playout_mode: PlayoutMode::Duration,
            duration: Some(Duration::hours(1)),
            tail_mode: TailMode::Offline,
            discard_to_fill_attempts: attempts,
            preferred_audio_language_code: Some("nld".into()),
            preferred_subtitle_language_code: Some("eng".into()),
            ..ProgramScheduleItem::default()
        },
        ProgramScheduleItem {
            id: 402,
            program_schedule_id: 301,
            sort_order: 1,
            collection_type: CollectionType::Movie,
            media_item_id: Some(21),
            playout_mode: PlayoutMode::One,
            ..ProgramScheduleItem::default()
        },
    ] {
        repos.program_schedule.upsert_item(&item).await.unwrap();
    }
    pool.execute(r#"INSERT INTO "Playout" ("Id", "ChannelId", "ProgramScheduleId", "ScheduleKind", "Seed") VALUES (901, 901, 301, 'Classic', 123)"#).await.unwrap();
    pool
}

async fn build(
    pool: &SqlitePool,
    start: chrono::DateTime<Utc>,
    finish: chrono::DateTime<Utc>,
    mode: PlayoutBuildMode,
) -> (Playout, SchedulingResult) {
    let repos = make_repositories(pool.clone());
    let playout = repos.playout.get_with_anchor(901).await.unwrap();
    let reference_data = repos
        .scheduling
        .classic_reference_data(901, start, Duration::zero())
        .await
        .unwrap();
    assert!(reference_data.classic_schedule_items[0].discard_to_fill_attempts > 0);
    assert_eq!(
        reference_data.media_by_key
            [&classic_schedule_item_key(&reference_data.classic_schedule_items[0])]
            .iter()
            .map(|i| i.id)
            .collect::<Vec<_>>(),
        [12, 13, 20, 21]
    );
    let result = PlayoutBuilder::default()
        .build(SchedulingRequest {
            playout: playout.clone(),
            start,
            finish,
            mode,
            reference_data,
        })
        .unwrap();
    (playout, result)
}

#[tokio::test]
async fn classic_duration_discard_sqlite_restart_continue_and_refresh_match_uninterrupted() {
    for attempts in [1, 2] {
        for mode in [PlayoutBuildMode::Continue, PlayoutBuildMode::Refresh] {
            let directory = tempfile::tempdir().unwrap();
            let url = format!(
                "sqlite://{}",
                directory.path().join("duration.db").display()
            );
            let pool = database(&url, attempts).await;
            let start = Utc.with_ymd_and_hms(2026, 11, 11, 23, 20, 0).unwrap();
            let seam = start + Duration::minutes(40);
            let finish = start + Duration::minutes(75);
            let (_, full) = build(&pool, start, finish, PlayoutBuildMode::Continue).await;
            let expected_ids = if attempts == 1 {
                vec![20, 21]
            } else {
                vec![20, 21, 21]
            };
            assert_eq!(
                full.added_items
                    .iter()
                    .map(|i| i.media_item_id)
                    .collect::<Vec<_>>(),
                expected_ids
            );
            let (playout, first) = build(&pool, start, seam, PlayoutBuildMode::Continue).await;
            assert_eq!(
                first.anchor.as_ref().unwrap().duration_finish,
                Some(start + Duration::hours(1))
            );
            make_repositories(pool.clone())
                .playout
                .apply_build_result(&first, &playout, Duration::zero())
                .await
                .unwrap();
            pool.close().await;
            let pool = ersatzrs_infra::db::init(&url).await.unwrap();
            let (playout, resumed) = build(&pool, seam, finish, mode).await;
            assert_eq!(
                resumed.added_items,
                full.added_items[1..],
                "attempts={attempts}, mode={mode:?}"
            );
            assert_eq!(resumed.anchor, full.anchor);
            make_repositories(pool.clone())
                .playout
                .apply_build_result(&resumed, &playout, Duration::zero())
                .await
                .unwrap();
            let stored = make_repositories(pool.clone())
                .scheduling
                .classic_reference_data(901, seam, Duration::zero())
                .await
                .unwrap();
            assert!(ersatzrs_core::scheduling::audit_classic_runtime_state(&stored).is_complete());
            assert!(ersatzrs_core::scheduling::audit_classic_refresh_checkpoint(
                &stored, 901, seam
            )
            .is_complete());
            assert_eq!(
                stored
                    .existing_items
                    .iter()
                    .map(|i| (i.media_item_id, i.start, i.finish, i.guide_group))
                    .collect::<Vec<_>>(),
                full.added_items
                    .iter()
                    .map(|i| (i.media_item_id, i.start, i.finish, i.guide_group))
                    .collect::<Vec<_>>()
            );
            assert_eq!(
                stored.existing_items[0]
                    .preferred_audio_language_code
                    .as_deref(),
                Some("nld")
            );
            assert_eq!(
                stored.existing_items[0]
                    .preferred_subtitle_language_code
                    .as_deref(),
                Some("eng")
            );
        }
    }
}
