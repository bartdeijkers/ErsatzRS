use super::scripted_continuation::snapshot;
use super::scripted_sources::{build, database};
use super::*;
use ersatzrs_core::locking::EntityLocker;
use ersatzrs_core::repositories::PlayoutRepository;
use ersatzrs_core::scheduling::{
    PlayoutBuildMode, PlayoutBuildRequest, SchedulingReferenceData, SchedulingResult,
};
use ersatzrs_domain::ScanPlayoutRefreshReason;
use ersatzrs_infra::maintenance::{
    run_requested_playout_build_cancellable, PlayoutBuildCancellation,
};
use ersatzrs_infra::scan_refresh;
use std::sync::{
    atomic::{AtomicUsize, Ordering},
    Arc, Mutex,
};

async fn changed_source(pool: &SqlitePool) {
    changed_source_for(pool, 903).await;
}

async fn changed_source_for(pool: &SqlitePool, playout_id: i64) {
    let mut tx = ersatzrs_infra::db::begin_write(pool).await.unwrap();
    scan_refresh::persist_unowned_refresh_intents(
        &mut tx,
        ScanPlayoutRefreshReason::RemoteStreamMembershipAdded,
        &[playout_id],
    )
    .await
    .unwrap();
    tx.commit().await.unwrap();
}

pub(super) async fn published_fixture(
    url: &str,
    path: &std::path::Path,
) -> (SqlitePool, Playout, SchedulingResult) {
    std::fs::write(path, "add_playlist('pl', 'nightly', 'prime')\nwatermark_on('bug')\ngraphics_on_path('lower-third.svg')\nwhile not is_done() do add_count('pl', 1) end").unwrap();
    let pool = database(url, path).await;
    let repos = make_repositories(pool.clone());
    let playout = repos.playout.get_with_anchor(903).await.unwrap();
    let start = common::fixed_time();
    let result = build(
        &pool,
        playout.clone(),
        start,
        start + Duration::hours(4),
        PlayoutBuildMode::Reset,
    )
    .await;
    repos
        .playout
        .apply_build_result(&result, &playout, Duration::zero())
        .await
        .unwrap();
    common::seed_profile_and_channel(&repos, 904).await;
    pool.execute(r#"INSERT INTO "Playout" ("Id", "ChannelId", "ScheduleKind", "Seed") VALUES (904, 904, 0, 99)"#).await.unwrap();
    let mut other_item = result.added_items[0].clone();
    other_item.id = 900_000;
    other_item.playout_id = 904;
    repos.playout.upsert_item(&other_item).await.unwrap();
    (pool, playout, result)
}

fn reset_future_window(
    data: &SchedulingReferenceData,
    boundary: chrono::DateTime<Utc>,
) -> Vec<ersatzrs_domain::PlayoutItem> {
    data.existing_items
        .iter()
        .filter(|item| item.start >= boundary && item.finish <= boundary + Duration::hours(2))
        .cloned()
        .map(|mut item| {
            item.id = 0;
            for graphics in &mut item.graphics_elements {
                graphics.playout_item_id = 0;
            }
            item
        })
        .collect()
}

#[tokio::test]
async fn scripted_preserved_reset_keeps_programme_and_restarts_playlist_after_sqlite_restart() {
    use ersatzrs_core::scheduling::PlayoutPreservation;
    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().join("schedule.lua");
    let url = format!("sqlite://{}", directory.path().join("reset.db").display());
    let (pool, _, generated) = published_fixture(&url, &path).await;
    let now = generated.added_items[1].start + Duration::seconds(1);
    let boundary = generated.added_items[1].finish;
    let repos = make_repositories(pool.clone());
    let before = repos
        .scheduling
        .scripted_reference_data(903, BTreeMap::new(), None)
        .await
        .unwrap();
    let other = snapshot(&pool, 904).await;
    let reset = PlayoutBuildRequest::new(903, PlayoutBuildMode::Reset)
        .with_preservation(PlayoutPreservation::KeepCurrent);
    run_requested_playout_build_cancellable(
        &pool,
        &EntityLocker::default(),
        reset,
        now,
        None,
        None,
    )
    .await
    .unwrap();
    let after = repos
        .scheduling
        .scripted_reference_data(903, BTreeMap::new(), None)
        .await
        .unwrap();
    let prefix = |data: &SchedulingReferenceData| {
        data.existing_items
            .iter()
            .filter(|item| item.finish <= boundary)
            .cloned()
            .collect::<Vec<_>>()
    };
    let history = |data: &SchedulingReferenceData| {
        data.existing_history
            .iter()
            .filter(|entry| entry.when < boundary)
            .cloned()
            .collect::<Vec<_>>()
    };
    assert_eq!(prefix(&before), prefix(&after));
    assert_eq!(history(&before), history(&after));
    let first = after
        .existing_items
        .iter()
        .find(|item| item.start == boundary)
        .unwrap();
    assert_eq!(
        first.media_item_id, generated.added_items[0].media_item_id,
        "Reset starts the playlist at its first item after the retained programme"
    );
    let expected = reset_future_window(&after, boundary);
    drop(repos);
    pool.close().await;

    let pool = ersatzrs_infra::db::init(&url).await.unwrap();
    let repos = make_repositories(pool.clone());
    let refresh = PlayoutBuildRequest::new(903, PlayoutBuildMode::Refresh)
        .with_preservation(PlayoutPreservation::KeepCurrent);
    for clock in [now + Duration::seconds(1), boundary + Duration::seconds(1)] {
        run_requested_playout_build_cancellable(
            &pool,
            &EntityLocker::default(),
            refresh,
            clock,
            None,
            None,
        )
        .await
        .unwrap();
        let refreshed = repos
            .scheduling
            .scripted_reference_data(903, BTreeMap::new(), None)
            .await
            .unwrap();
        assert_eq!(prefix(&before), prefix(&refreshed));
        assert_eq!(history(&before), history(&refreshed));
        assert_eq!(reset_future_window(&refreshed, boundary), expected);
        assert_eq!(snapshot(&pool, 904).await, other);
    }
    assert_eq!(
        scripted_health_status(&pool).await,
        ersatzrs_core::health::HealthCheckStatus::Pass
    );
}

#[tokio::test]
async fn scripted_requested_preservation_choice_reaches_publication() {
    use ersatzrs_core::scheduling::PlayoutPreservation;
    for policy in [
        PlayoutPreservation::KeepCurrent,
        PlayoutPreservation::ReplaceCurrent,
    ] {
        let directory = tempfile::tempdir().unwrap();
        let path = directory.path().join("schedule.lua");
        let (pool, playout, generated) = published_fixture("sqlite::memory:", &path).await;
        let now = generated.added_items[1].start + Duration::seconds(1);
        let repos = make_repositories(pool.clone());
        let current = repos
            .playout
            .items_in_range(903, now, now + Duration::milliseconds(1))
            .await
            .unwrap()
            .remove(0);
        let other = snapshot(&pool, 904).await;
        let request =
            PlayoutBuildRequest::new(903, PlayoutBuildMode::Refresh).with_preservation(policy);
        assert!(run_requested_playout_build_cancellable(
            &pool,
            &EntityLocker::default(),
            request,
            now,
            None,
            None,
        )
        .await
        .unwrap());
        let after = repos
            .playout
            .items_in_range(903, now, now + Duration::milliseconds(1))
            .await
            .unwrap()
            .remove(0);
        if policy == PlayoutPreservation::KeepCurrent {
            assert_eq!(
                after, current,
                "preservation includes row identity and attachments"
            );
        } else {
            assert_eq!(after.start, now);
            assert_ne!(after.id, current.id);
            assert_eq!(
                after.media_item_id, current.media_item_id,
                "Refresh restores the active series position rather than resetting it"
            );
        }
        assert_eq!(
            repos.playout.get_with_anchor(903).await.unwrap().seed,
            playout.seed
        );
        assert_eq!(snapshot(&pool, 904).await, other);
    }
}

#[tokio::test]
async fn scripted_publication_fences_source_revisions_across_restart_and_intent_recreation() {
    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().join("schedule.lua");
    let url = format!("sqlite://{}", directory.path().join("refresh.db").display());
    let (pool, mut playout, candidate) = published_fixture(&url, &path).await;
    let before = snapshot(&pool, 903).await;
    let other_before = snapshot(&pool, 904).await;
    // A rejected replacement must not even publish its new Reset seed.
    playout.seed += 1;
    let absent = scan_refresh::publication_guard(&pool, 903).await.unwrap();
    changed_source(&pool).await;
    let first = scan_refresh::publication_guard(&pool, 903).await.unwrap();
    changed_source(&pool).await;
    let second = scan_refresh::publication_guard(&pool, 903).await.unwrap();
    scan_refresh::complete_refresh(&pool, 903, 2).await.unwrap();
    changed_source(&pool).await;
    // Recreated intent has generation 1 again, but a distinct lifecycle.
    sqlx::query(r#"UPDATE "ScanPlayoutRefreshIntent" SET "CreatedAt" = ? WHERE "PlayoutId" = 903"#)
        .bind("2030-01-01T00:00:00Z")
        .execute(&pool)
        .await
        .unwrap();
    pool.close().await;
    let pool = ersatzrs_infra::db::init(&url).await.unwrap();
    let repos = make_repositories(pool.clone());
    let current = scan_refresh::publication_guard(&pool, 903).await.unwrap();
    let mut wrong_owner = current.clone();
    wrong_owner.playout_id = 904;
    for stale in [absent, first, second, wrong_owner] {
        let mut rejected = candidate.clone();
        rejected.publication_guard = Some(stale);
        let error = repos
            .playout
            .apply_build_result(&rejected, &playout, Duration::zero())
            .await
            .unwrap_err();
        assert!(error.to_string().contains("superseded"), "{error}");
        assert_eq!(snapshot(&pool, 903).await, before);
        assert_eq!(snapshot(&pool, 904).await, other_before);
        assert_eq!(
            scan_refresh::publication_guard(&pool, 903).await.unwrap(),
            current
        );
    }
    // Claim/state changes do not invalidate the source generation itself.
    scan_refresh::mark_refresh_building(&pool, 903)
        .await
        .unwrap();
    // Independent work for another channel does not invalidate this candidate.
    changed_source_for(&pool, 904).await;
    let mut accepted = candidate;
    accepted.publication_guard = Some(current);
    repos
        .playout
        .apply_build_result(&accepted, &playout, Duration::zero())
        .await
        .unwrap();
    assert_ne!(snapshot(&pool, 903).await, before);
    assert_eq!(snapshot(&pool, 904).await, other_before);
}

#[tokio::test]
async fn scripted_requested_build_rejects_a_scan_arriving_during_generation_then_retries() {
    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().join("schedule.lua");
    let (pool, _, _) = published_fixture("sqlite::memory:", &path).await;
    let before = snapshot(&pool, 903).await;
    let other_before = snapshot(&pool, 904).await;
    changed_source(&pool).await;
    let (started_tx, started_rx) = tokio::sync::oneshot::channel();
    let (release_tx, release_rx) = std::sync::mpsc::channel();
    let started = Mutex::new(Some(started_tx));
    let release = Mutex::new(release_rx);
    let calls = AtomicUsize::new(0);
    let cancelled: PlayoutBuildCancellation = Arc::new(move || {
        // First check admits the request; second runs in the generation worker,
        // after the source guard and all reference data have been loaded.
        if calls.fetch_add(1, Ordering::SeqCst) == 1 {
            started.lock().unwrap().take().unwrap().send(()).unwrap();
            return release
                .lock()
                .unwrap()
                .recv_timeout(std::time::Duration::from_secs(10))
                .is_err();
        }
        false
    });
    let worker_pool = pool.clone();
    let request = PlayoutBuildRequest::new(903, PlayoutBuildMode::Reset);
    let worker = tokio::spawn(async move {
        run_requested_playout_build_cancellable(
            &worker_pool,
            &EntityLocker::default(),
            request,
            common::fixed_time(),
            None,
            Some(&cancelled),
        )
        .await
    });
    tokio::time::timeout(std::time::Duration::from_secs(10), started_rx)
        .await
        .unwrap()
        .unwrap();
    changed_source(&pool).await;
    let latest = scan_refresh::publication_guard(&pool, 903).await.unwrap();
    release_tx.send(()).unwrap();
    let error = worker.await.unwrap().unwrap_err();
    assert!(error.to_string().contains("superseded"), "{error}");
    let repos = make_repositories(pool.clone());
    assert!(
        !repos
            .playout
            .get_with_anchor(903)
            .await
            .unwrap()
            .build_status
            .unwrap()
            .success
    );
    assert_eq!(snapshot(&pool, 903).await, before);
    assert_eq!(snapshot(&pool, 904).await, other_before);
    assert_eq!(
        scan_refresh::publication_guard(&pool, 903).await.unwrap(),
        latest
    );
    assert_eq!(
        scan_refresh::pending_refreshes(&pool).await.unwrap().len(),
        1
    );
    assert!(run_requested_playout_build_cancellable(
        &pool,
        &EntityLocker::default(),
        request,
        common::fixed_time(),
        None,
        None
    )
    .await
    .unwrap());
    assert!(scan_refresh::pending_refreshes(&pool)
        .await
        .unwrap()
        .is_empty());
    assert!(
        repos
            .playout
            .get_with_anchor(903)
            .await
            .unwrap()
            .build_status
            .unwrap()
            .success
    );
    assert_eq!(snapshot(&pool, 904).await, other_before);
}

#[tokio::test]
async fn scripted_refresh_restores_published_boundary_after_sqlite_restart() {
    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().join("schedule.lua");
    let url = format!(
        "sqlite://{}",
        directory.path().join("boundary.db").display()
    );
    let (pool, _, _) = published_fixture(&url, &path).await;
    let before = snapshot(&pool, 903).await;
    let other_before = snapshot(&pool, 904).await;
    pool.close().await;
    let pool = ersatzrs_infra::db::init(&url).await.unwrap();
    let repos = make_repositories(pool.clone());
    let playout = repos.playout.get_with_anchor(903).await.unwrap();
    let original = repos
        .scheduling
        .scripted_reference_data(903, BTreeMap::new(), None)
        .await
        .unwrap();
    assert_eq!(
        original.existing_scripted_checkpoints.len(),
        original.existing_items.len()
    );
    let first = &original.existing_items[0];
    let result = build(
        &pool,
        playout.clone(),
        first.start + Duration::milliseconds(1),
        common::fixed_time() + Duration::hours(4),
        PlayoutBuildMode::Refresh,
    )
    .await;
    let boundary = result.retain_through.unwrap();
    assert!(boundary >= first.finish);
    assert_eq!(result.added_items[0].start, boundary);
    repos
        .playout
        .apply_build_result(&result, &playout, Duration::zero())
        .await
        .unwrap();
    let refreshed = repos
        .scheduling
        .scripted_reference_data(903, BTreeMap::new(), None)
        .await
        .unwrap();
    let retained = |reference: &SchedulingReferenceData| {
        reference
            .existing_items
            .iter()
            .filter(|item| item.finish <= boundary)
            .cloned()
            .collect::<Vec<_>>()
    };
    assert_eq!(retained(&original), retained(&refreshed));
    let retained_history = |reference: &SchedulingReferenceData| {
        reference
            .existing_history
            .iter()
            .filter(|history| history.when < boundary)
            .cloned()
            .collect::<Vec<_>>()
    };
    assert_eq!(retained_history(&original), retained_history(&refreshed));
    let canonical_items = |reference: &SchedulingReferenceData| {
        reference
            .existing_items
            .iter()
            .cloned()
            .map(|mut item| {
                item.id = 0;
                for graphics in &mut item.graphics_elements {
                    graphics.playout_item_id = 0;
                }
                item
            })
            .collect::<Vec<_>>()
    };
    assert_eq!(canonical_items(&original), canonical_items(&refreshed));
    assert_eq!(snapshot(&pool, 904).await, other_before);
    // A legacy/missing checkpoint refuses a real requested Refresh before writes.
    pool.execute(r#"DELETE FROM "PlayoutScriptedCheckpoint" WHERE "PlayoutId" = 903"#)
        .await
        .unwrap();
    let legacy = snapshot(&pool, 903).await;
    let request = PlayoutBuildRequest::new(903, PlayoutBuildMode::Refresh);
    let error = run_requested_playout_build_cancellable(
        &pool,
        &EntityLocker::default(),
        request,
        first.start + Duration::milliseconds(1),
        None,
        None,
    )
    .await
    .unwrap_err();
    assert!(error.to_string().contains("checkpoint"), "{error}");
    assert_eq!(snapshot(&pool, 903).await, legacy);
    assert_eq!(snapshot(&pool, 904).await, other_before);
    assert_ne!(legacy, before); // only the deliberate checkpoint deletion/replacement differs
}

pub(super) async fn scripted_health_status(
    pool: &SqlitePool,
) -> ersatzrs_core::health::HealthCheckStatus {
    let health = ersatzrs_infra::health::HealthCheckCoordinator::new(
        pool.clone(),
        Arc::new(tokio::sync::RwLock::new(
            ersatzrs_ffmpeg::FfmpegRuntimeReport::for_tests(),
        )),
        ersatzrs_infra::db::DatabaseSchemaStatus::default(),
    );
    health
        .run(true)
        .await
        .unwrap()
        .results
        .into_iter()
        .find(|result| result.title == "Scripted Playout State")
        .unwrap()
        .status
}

#[tokio::test]
async fn scripted_checkpoint_health_and_publication_reject_corrupt_restore_state() {
    use ersatzrs_core::health::HealthCheckStatus::{Fail, Pass};
    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().join("schedule.lua");
    let (pool, playout, result) = published_fixture("sqlite::memory:", &path).await;
    assert_eq!(scripted_health_status(&pool).await, Pass);
    let before = snapshot(&pool, 903).await;
    let repos = make_repositories(pool.clone());
    for pointer in [
        "/version",
        "/sources/pl/Playlist/state/ChildIndex",
        "/sources/pl/Playlist/state/Children/0/CurrentIndex",
    ] {
        let mut candidate = result.clone();
        let mut context: serde_json::Value =
            serde_json::from_str(&candidate.scripted_checkpoints[0].context).unwrap();
        *context
            .pointer_mut(pointer)
            .expect("checkpoint field exists") = 999.into();
        candidate.scripted_checkpoints[0].context = context.to_string();
        assert!(repos
            .playout
            .apply_build_result(&candidate, &playout, Duration::zero())
            .await
            .is_err());
        assert_eq!(snapshot(&pool, 903).await, before);
    }
    let mut incomplete = result.clone();
    incomplete.scripted_checkpoints.clear();
    assert!(repos
        .playout
        .apply_build_result(&incomplete, &playout, Duration::zero())
        .await
        .is_err());
    assert_eq!(snapshot(&pool, 903).await, before);
    sqlx::query(
        r#"UPDATE "PlayoutScriptedCheckpoint" SET "Context" = '{}' WHERE "PlayoutId" = 903"#,
    )
    .execute(&pool)
    .await
    .unwrap();
    assert_eq!(scripted_health_status(&pool).await, Fail);
    repos
        .playout
        .apply_build_result(&result, &playout, Duration::zero())
        .await
        .unwrap();
    assert_eq!(scripted_health_status(&pool).await, Pass);
    pool.execute(r#"DELETE FROM "PlayoutScriptedCheckpoint" WHERE "PlayoutId" = 903"#)
        .await
        .unwrap();
    assert_eq!(scripted_health_status(&pool).await, Fail);
}
