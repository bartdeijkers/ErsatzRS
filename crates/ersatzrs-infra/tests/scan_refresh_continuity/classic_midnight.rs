//! Classic midnight publication and restart safety through real `SQLite` builds.

use super::*;
use chrono::TimeZone as _;

fn start() -> DateTime<Utc> {
    Utc.with_ymd_and_hms(2026, 4, 15, 23, 30, 0).unwrap()
}

async fn seed(pool: &SqlitePool, filler: bool) {
    seed_acceptance_fixture(pool).await;
    pool.execute(
        r#"UPDATE "MediaVersion" SET "Duration"='02:00:00' WHERE "MovieId" IN (101, 102);
        INSERT INTO "PlaylistItem" ("Id", "Index", "PlaylistId", "CollectionType", "MediaItemId",
            "IncludeInProgramGuide", "PlaybackOrder", "PlayAll")
        VALUES (102, 1, 11, 'Movie', 102, 1, 'Chronological', 1);"#,
    )
    .await
    .unwrap();
    if filler {
        seed_continuity_filler_chapters(pool).await;
    }
}

#[tokio::test]
async fn classic_publication_rejects_overlapping_candidate_before_any_state_changes() {
    let directory = tempfile::tempdir().unwrap();
    let pool = open_database(&directory.path().join("candidate.sqlite"), true).await;
    seed(&pool, false).await;
    let start = start();
    build_filler_window(
        &pool,
        PlayoutBuildMode::Reset,
        start,
        start + Duration::days(1),
        true,
    )
    .await;
    let before = persisted_classic_rows(&pool, RELATED_PLAYOUT_ID).await;
    let mut candidate = build_filler_window(
        &pool,
        PlayoutBuildMode::Reset,
        start,
        start + Duration::days(1),
        false,
    )
    .await;
    assert!(candidate.added_items[0].start < start + Duration::minutes(30));
    assert!(candidate.added_items[0].finish > start + Duration::minutes(30));
    candidate.added_items[1].start = start + Duration::minutes(30);
    let repos = make_repositories(pool.clone());
    let playout = repos
        .playout
        .get_with_anchor(RELATED_PLAYOUT_ID)
        .await
        .unwrap();
    let error = repos
        .playout
        .apply_build_result(&candidate, &playout, Duration::zero())
        .await
        .expect_err("reject a second film starting at midnight before the first ends");
    assert!(error.to_string().contains("Classic timeline"), "{error}");
    assert!(
        before == persisted_classic_rows(&pool, RELATED_PLAYOUT_ID).await,
        "rejected publication must retain rows, guide fields, history, anchors and checkpoints"
    );
    for defect in ["zero", "anchor", "owner", "retention", "append"] {
        let mut unsafe_candidate = candidate.clone();
        unsafe_candidate.added_items[1].start = unsafe_candidate.added_items[0].finish;
        match defect {
            "zero" => {
                unsafe_candidate.added_items[0].finish = unsafe_candidate.added_items[0].start;
            }
            "anchor" => unsafe_candidate.anchor.as_mut().unwrap().next_start = start,
            "owner" => unsafe_candidate.added_items[0].playout_id = UNRELATED_PLAYOUT_ID,
            "append" => unsafe_candidate.clear_items = false,
            _ => unsafe_candidate.retain_through = Some(start + Duration::hours(1)),
        }
        assert!(
            repos
                .playout
                .apply_build_result(&unsafe_candidate, &playout, Duration::zero())
                .await
                .is_err(),
            "{defect}"
        );
        assert!(
            before == persisted_classic_rows(&pool, RELATED_PLAYOUT_ID).await,
            "{defect} mutated state"
        );
    }
}

#[tokio::test]
async fn classic_midnight_continue_after_process_restart_matches_uninterrupted_programmes() {
    for filler in [false, true] {
        for order in ["Chronological", "Shuffle", "Random", "RandomRotation"] {
            let directory = tempfile::tempdir().unwrap();
            let path = directory.path().join("restart.sqlite");
            let pool = open_database(&path, true).await;
            seed(&pool, filler).await;
            pool.execute(r#"INSERT INTO "Collection" ("Id", "Name", "UseCustomPlaybackOrder")
                VALUES (93, 'Midnight films', 0);
                INSERT INTO "CollectionItem" ("CollectionId", "MediaItemId") VALUES (93, 101), (93, 102);"#)
                .await.unwrap();
            sqlx::query(
                r#"UPDATE "ProgramScheduleItem" SET "CollectionType"='Collection',
                "CollectionId"=93, "PlaylistId"=NULL, "PlaybackOrder"=? WHERE "Id"=401"#,
            )
            .bind(order)
            .execute(&pool)
            .await
            .unwrap();
            let start = start();
            let finish = start + Duration::hours(26);
            let oracle =
                build_filler_window(&pool, PlayoutBuildMode::Reset, start, finish, false).await;
            let first = build_filler_window(
                &pool,
                PlayoutBuildMode::Reset,
                start,
                start + Duration::minutes(30),
                true,
            )
            .await;
            assert!(first.anchor.as_ref().unwrap().next_start >= start + Duration::hours(2));
            assert!(first
                .added_items
                .iter()
                .any(|item| item.start < start + Duration::minutes(30)
                    && item.finish > start + Duration::minutes(30)));
            let prefix = make_repositories(pool.clone())
                .playout
                .items_in_range(RELATED_PLAYOUT_ID, start, finish)
                .await
                .unwrap();
            pool.close().await;
            continue_in_new_process(&path).await;
            let reopened = open_database(&path, false).await;
            let repos = make_repositories(reopened.clone());
            let mut actual = repos
                .playout
                .items_in_range(
                    RELATED_PLAYOUT_ID,
                    start,
                    oracle.added_items.last().unwrap().finish,
                )
                .await
                .unwrap();
            assert_eq!(&actual[..prefix.len()], prefix, "retained rows changed");
            for item in &mut actual {
                item.id = 0;
            }
            assert_eq!(actual, oracle.added_items, "{order}, filler={filler}");
            assert!(actual
                .windows(2)
                .all(|pair| pair[0].finish <= pair[1].start));
            let anchor = repos
                .playout
                .get_with_anchor(RELATED_PLAYOUT_ID)
                .await
                .unwrap()
                .anchor
                .unwrap();
            assert_eq!(
                Some(anchor),
                oracle.anchor,
                "complete continuation state differs"
            );
            if filler {
                assert!(actual
                    .iter()
                    .any(|item| item.filler_kind != ersatzrs_domain::FillerKind::None));
            }
            reopened.close().await;
        }
    }
}

async fn continue_in_new_process(path: &Path) {
    let output = tokio::time::timeout(
        std::time::Duration::from_secs(30),
        tokio::process::Command::new(std::env::current_exe().unwrap())
            .kill_on_drop(true)
            .args([
                "--exact",
                "classic_midnight::continue_in_fresh_process",
                "--ignored",
                "--nocapture",
            ])
            .env("ERSATZRS_CLASSIC_RESTART_DB", path)
            .output(),
    )
    .await
    .expect("bounded fresh-process continuation")
    .unwrap();
    assert!(
        output.status.success(),
        "fresh process failed: {}{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
}

#[tokio::test]
#[ignore = "invoked by the midnight restart test in a fresh process"]
#[allow(clippy::disallowed_methods)] // Parent supplies only a disposable test database path.
async fn continue_in_fresh_process() {
    let path = std::env::var_os("ERSATZRS_CLASSIC_RESTART_DB").expect("parent database");
    let pool = open_database(Path::new(&path), false).await;
    // The request clock is still inside the retained film; Continue must use
    // the persisted end, including any post-roll, not this wall-clock cursor.
    build_filler_window(
        &pool,
        PlayoutBuildMode::Continue,
        start() + Duration::minutes(31),
        start() + Duration::hours(26),
        true,
    )
    .await;
    pool.close().await;
}

#[tokio::test]
async fn classic_future_overlap_repair_preserves_current_programme_and_rotation() {
    let directory = tempfile::tempdir().unwrap();
    let pool = open_database(&directory.path().join("repair.sqlite"), true).await;
    seed(&pool, true).await;
    let start = start();
    let finish = start + Duration::days(1);
    let oracle = build_filler_window(&pool, PlayoutBuildMode::Reset, start, finish, true).await;
    let now = start + Duration::minutes(5);
    let repos = make_repositories(pool.clone());
    let original = repos
        .playout
        .items_in_range(RELATED_PLAYOUT_ID, start, finish)
        .await
        .unwrap();
    let history_before = classic_history(&repos, now).await;
    let films: Vec<_> = original
        .iter()
        .filter(|item| item.filler_kind == ersatzrs_domain::FillerKind::None)
        .collect();
    let broken = films[2];
    sqlx::query(r#"UPDATE "PlayoutItem" SET "Start"=? WHERE "Id"=?"#)
        .bind((films[1].start + Duration::minutes(30)).to_rfc3339())
        .bind(broken.id)
        .execute(&pool)
        .await
        .unwrap();
    let playout = repos
        .playout
        .get_with_anchor(RELATED_PLAYOUT_ID)
        .await
        .unwrap();
    let request = preserving_request(&repos, now, finish, PlayoutBuildMode::Refresh).await;
    let repaired = PlayoutBuilder::default()
        .build(request)
        .expect("a verified current boundary can replace an invalid future");
    let seam = repaired.retain_through.unwrap();
    repos
        .playout
        .apply_build_result(&repaired, &playout, Duration::zero())
        .await
        .unwrap();
    let actual = repos
        .playout
        .items_in_range(
            RELATED_PLAYOUT_ID,
            start,
            oracle.added_items.last().unwrap().finish,
        )
        .await
        .unwrap();
    assert_eq!(
        actual
            .iter()
            .filter(|item| item.finish <= seam)
            .collect::<Vec<_>>(),
        original
            .iter()
            .filter(|item| item.finish <= seam)
            .collect::<Vec<_>>()
    );
    let without_ids = actual
        .into_iter()
        .map(|mut item| {
            item.id = 0;
            item
        })
        .collect::<Vec<_>>();
    assert_eq!(
        without_ids, oracle.added_items,
        "repair must reproduce guide, settings and rotation"
    );
    assert_eq!(
        repos
            .playout
            .get_with_anchor(RELATED_PLAYOUT_ID)
            .await
            .unwrap()
            .anchor,
        oracle.anchor
    );
    let history_after = classic_history(&repos, now).await;
    assert_eq!(
        history_after
            .iter()
            .filter(|entry| entry.when < seam)
            .collect::<Vec<_>>(),
        history_before
            .iter()
            .filter(|entry| entry.when < seam)
            .collect::<Vec<_>>(),
        "repair must retain consumed history and its identities"
    );
}

#[tokio::test]
async fn classic_ambiguous_current_programme_refuses_repair_without_mutation() {
    let directory = tempfile::tempdir().unwrap();
    let pool = open_database(&directory.path().join("unsafe.sqlite"), true).await;
    seed(&pool, false).await;
    let start = start();
    let finish = start + Duration::days(1);
    build_filler_window(&pool, PlayoutBuildMode::Reset, start, finish, true).await;
    let repos = make_repositories(pool.clone());
    let items = repos
        .playout
        .items_in_range(RELATED_PLAYOUT_ID, start, finish)
        .await
        .unwrap();
    sqlx::query(r#"UPDATE "PlayoutItem" SET "Start"=? WHERE "Id"=?"#)
        .bind((start + Duration::minutes(1)).to_rfc3339())
        .bind(items[1].id)
        .execute(&pool)
        .await
        .unwrap();
    let before = persisted_classic_rows(&pool, RELATED_PLAYOUT_ID).await;
    let now = start + Duration::minutes(5);
    let playout = repos
        .playout
        .get_with_anchor(RELATED_PLAYOUT_ID)
        .await
        .unwrap();
    let mut reference = repos
        .scheduling
        .classic_reference_data(RELATED_PLAYOUT_ID, now, Duration::zero())
        .await
        .unwrap();
    reference.preservation = ersatzrs_core::scheduling::PlayoutPreservation::KeepCurrent;
    for mode in [PlayoutBuildMode::Refresh, PlayoutBuildMode::Reset] {
        assert!(
            PlayoutBuilder::default()
                .build(SchedulingRequest {
                    playout: playout.clone(),
                    start: now,
                    finish,
                    mode,
                    reference_data: reference.clone(),
                })
                .is_err(),
            "ambiguous current {mode:?} must refuse repair"
        );
        assert!(before == persisted_classic_rows(&pool, RELATED_PLAYOUT_ID).await);
    }
}

#[tokio::test]
async fn classic_health_reports_future_overlap_and_recovers_after_valid_rebuild() {
    use ersatzrs_core::health::HealthCheckStatus;
    let directory = tempfile::tempdir().unwrap();
    let pool = open_database(&directory.path().join("health.sqlite"), true).await;
    seed(&pool, false).await;
    let start = Utc::now()
        .date_naive()
        .and_hms_opt(23, 30, 0)
        .unwrap()
        .and_utc();
    let finish = start + Duration::days(1);
    build_filler_window(&pool, PlayoutBuildMode::Reset, start, finish, true).await;
    let health = ersatzrs_infra::health::make_health_check_service(
        pool.clone(),
        ersatzrs_ffmpeg::FfmpegRuntimeReport::for_tests(),
        ersatzrs_infra::db::DatabaseSchemaStatus::default(),
    );
    let status = || async {
        health
            .perform_health_checks()
            .await
            .into_iter()
            .find(|result| result.title == "Classic Playout State")
            .unwrap()
            .status
    };
    assert_eq!(status().await, HealthCheckStatus::Pass);
    let items = make_repositories(pool.clone())
        .playout
        .items_in_range(RELATED_PLAYOUT_ID, start, finish)
        .await
        .unwrap();
    sqlx::query(r#"UPDATE "PlayoutItem" SET "Start"=? WHERE "Id"=?"#)
        .bind((start + Duration::minutes(30)).to_rfc3339())
        .bind(items[1].id)
        .execute(&pool)
        .await
        .unwrap();
    assert_eq!(status().await, HealthCheckStatus::Fail);
    build_filler_window(&pool, PlayoutBuildMode::Reset, start, finish, true).await;
    assert_eq!(status().await, HealthCheckStatus::Pass);
}

async fn preserving_request(
    repos: &Repositories,
    now: DateTime<Utc>,
    finish: DateTime<Utc>,
    mode: PlayoutBuildMode,
) -> SchedulingRequest {
    let playout = repos
        .playout
        .get_with_anchor(RELATED_PLAYOUT_ID)
        .await
        .unwrap();
    let mut reference_data = repos
        .scheduling
        .classic_reference_data(RELATED_PLAYOUT_ID, now, Duration::zero())
        .await
        .unwrap();
    reference_data.preservation = ersatzrs_core::scheduling::PlayoutPreservation::KeepCurrent;
    SchedulingRequest {
        playout,
        start: now,
        finish,
        mode,
        reference_data,
    }
}

async fn classic_history(
    repos: &Repositories,
    now: DateTime<Utc>,
) -> Vec<ersatzrs_domain::PlayoutHistory> {
    repos
        .scheduling
        .classic_reference_data(RELATED_PLAYOUT_ID, now, Duration::zero())
        .await
        .unwrap()
        .existing_history
}
