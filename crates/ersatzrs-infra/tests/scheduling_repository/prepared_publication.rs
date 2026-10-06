//! Production scheduling transactions remain private until publication is ready.

use super::{scripted_continuation::snapshot, scripted_refresh::published_fixture};
use super::{scripted_sources::build, *};
use ersatzrs_core::repositories::PlayoutRepository;
use ersatzrs_core::scheduling::PlayoutBuildMode;
use ersatzrs_infra::repositories::SqlitePlayoutRepository;
use std::time::{Duration as StdDuration, Instant};
use tokio::sync::oneshot;

#[tokio::test]
async fn prepared_refresh_keeps_committed_state_until_commit_and_survives_reopen() {
    let root = tempfile::tempdir().unwrap();
    let url = format!("sqlite://{}", root.path().join("publication.db").display());
    let (pool, _, first) = published_fixture(&url, &root.path().join("schedule.lua")).await;
    let repository = SqlitePlayoutRepository(pool.clone());
    let playout = repository.get_with_anchor(903).await.unwrap();
    let start = first.added_items[0].start;
    let finish = start + Duration::hours(6);
    let result = build(
        &pool,
        playout.clone(),
        start + Duration::seconds(1),
        finish,
        PlayoutBuildMode::Refresh,
    )
    .await;
    assert!(result.clear_items);
    let before = snapshot(&pool, 903).await;
    let accepted = repository.items_in_range(903, start, finish).await.unwrap();
    let other = snapshot(&pool, 904).await;
    let mut pending = repository
        .prepare_build_result(&result, &playout, Duration::zero())
        .await
        .unwrap();
    let candidate = pending.items_in_range(start, finish).await.unwrap();
    assert!(!candidate.is_empty());
    assert!(candidate.iter().all(|item| item.id > 0));
    assert!(candidate.iter().all(|item| !item.watermark_ids.is_empty()));
    assert!(candidate
        .iter()
        .all(|item| !item.graphics_elements.is_empty()));
    assert!(candidate.iter().all(|item| {
        item.graphics_elements
            .iter()
            .all(|graphics| graphics.playout_item_id == item.id)
    }));
    assert!(candidate.iter().all(|item| {
        item.preferred_audio_language_code.as_deref() == Some("nld")
            && item.preferred_subtitle_language_code.as_deref() == Some("eng")
    }));
    assert_ne!(candidate, accepted);
    assert_eq!(snapshot(&pool, 903).await, before);
    assert_eq!(snapshot(&pool, 904).await, other);
    pending.commit().await.unwrap();
    let after = snapshot(&pool, 903).await;
    assert_eq!(
        repository.items_in_range(903, start, finish).await.unwrap(),
        candidate
    );
    assert_ne!(after["history"], before["history"]);
    assert_ne!(after["checkpoints"], before["checkpoints"]);
    assert_eq!(snapshot(&pool, 904).await, other);
    drop(repository);
    pool.close().await;
    let pool = ersatzrs_infra::db::init(&url).await.unwrap();
    assert_eq!(snapshot(&pool, 903).await, after);
    assert_eq!(snapshot(&pool, 904).await, other);
}

#[tokio::test]
async fn expired_or_cancelled_preparation_rolls_back_the_entire_scheduler_state() {
    let root = tempfile::tempdir().unwrap();
    let url = format!("sqlite://{}", root.path().join("rollback.db").display());
    let (pool, _, first) = published_fixture(&url, &root.path().join("schedule.lua")).await;
    let repository = SqlitePlayoutRepository(pool.clone());
    let playout = repository.get_with_anchor(903).await.unwrap();
    let start = first.added_items[0].start;
    let finish = start + Duration::hours(6);
    let mut result = build(
        &pool,
        playout.clone(),
        start + Duration::seconds(1),
        finish,
        PlayoutBuildMode::Refresh,
    )
    .await;
    let before = snapshot(&pool, 903).await;
    let other = snapshot(&pool, 904).await;
    let deadline = Instant::now() + StdDuration::from_secs(1);
    result.publication_deadline = Some(deadline);
    let mut pending = repository
        .prepare_build_result(&result, &playout, Duration::zero())
        .await
        .unwrap();
    assert!(!pending
        .items_in_range(start, finish)
        .await
        .unwrap()
        .is_empty());
    tokio::time::sleep_until(deadline.into()).await;
    assert!(pending
        .commit()
        .await
        .unwrap_err()
        .to_string()
        .contains("deadline"));
    // Acquiring a new writer proves that asynchronous rollback has completed.
    ersatzrs_infra::db::begin_write(&pool)
        .await
        .unwrap()
        .rollback()
        .await
        .unwrap();
    assert_eq!(snapshot(&pool, 903).await, before);
    result.publication_deadline = None;
    let (entered, ready) = oneshot::channel();
    let (db, candidate, source) = (repository.clone(), result.clone(), playout.clone());
    let task = tokio::spawn(async move {
        let mut pending = db
            .prepare_build_result(&candidate, &source, Duration::zero())
            .await
            .unwrap();
        entered
            .send(pending.items_in_range(start, finish).await.unwrap())
            .unwrap();
        std::future::pending::<()>().await;
        drop(pending);
    });
    assert!(!ready.await.unwrap().is_empty());
    assert_eq!(snapshot(&pool, 903).await, before);
    task.abort();
    assert!(task.await.unwrap_err().is_cancelled());
    // The same candidate can be prepared again after cancelled-owner cleanup.
    let retry = repository
        .prepare_build_result(&result, &playout, Duration::zero())
        .await
        .unwrap();
    assert_eq!(snapshot(&pool, 903).await, before);
    drop(retry);
    ersatzrs_infra::db::begin_write(&pool)
        .await
        .unwrap()
        .rollback()
        .await
        .unwrap();
    assert_eq!(snapshot(&pool, 903).await, before);
    assert_eq!(snapshot(&pool, 904).await, other);
    drop(repository);
    pool.close().await;
    let pool = ersatzrs_infra::db::init(&url).await.unwrap();
    assert_eq!(snapshot(&pool, 903).await, before);
    assert_eq!(snapshot(&pool, 904).await, other);
}
