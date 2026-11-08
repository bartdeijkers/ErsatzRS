use super::*;
use crate::next_engine::publication::indexed::{
    PlayoutIndex, INDEX_FILENAME, READER_LOCK_FILENAME,
};
use ersatzrs_core::{programme::ProgrammeClock, scheduling::SchedulingResult};
use std::{fs::File, time::Instant};

#[path = "indexed_startup_tests.rs"]
mod startup;

async fn indexed_fixture(root: &Path) -> (DbPool, EntityLocker, Channel, DateTime<Utc>) {
    let pool = sqlx::sqlite::SqlitePoolOptions::new()
        .max_connections(4)
        .connect_with(
            sqlx::sqlite::SqliteConnectOptions::new()
                .filename(root.join("indexed.db"))
                .create_if_missing(true)
                .journal_mode(sqlx::sqlite::SqliteJournalMode::Wal),
        )
        .await
        .unwrap();
    let pool = crate::next_engine::selection::tests::seed_fixture_pool(pool).await;
    let (pool, locker, mut channel, start) = schedule::tests::fixture_with_pool(pool).await;
    channel.streaming_engine = StreamingEngine::Next;
    make_repositories(pool.clone())
        .channel
        .upsert(&channel)
        .await
        .unwrap();
    (pool, locker, channel, start)
}

async fn future_projection(
    channel: Channel,
    items: Vec<schedule::ScheduledMedia>,
    future: DateTime<Utc>,
) -> ersatzrs_core::repositories::Result<Vec<protocol::PlayoutItem>> {
    // Reuse the persisted fixture's source/track selection, moving only its
    // frozen September timeline into this test's future projection window.
    let mut projected = project(channel, items).await?;
    if let Some(first) = projected.first() {
        let delta = future - first.start;
        for item in &mut projected {
            item.start += delta;
            item.finish += delta;
        }
    }
    Ok(projected)
}

#[tokio::test]
async fn indexed_refresh_uses_the_real_coordinator_and_retains_native_retirement_guard() {
    let root = tempfile::tempdir().unwrap();
    let (pool, locker, mut channel, start) = indexed_fixture(root.path()).await;
    let publisher = ChannelPublisher::new_indexed(
        pool.clone(),
        locker,
        root.path(),
        channel.id,
        ProgrammeClock::named("UTC").unwrap(),
        crate::next_engine::indexed_runtime::IndexedReaderPermit::fixture(),
    );
    let future = Utc::now() + Duration::hours(1);
    let finish = start + Duration::minutes(2);
    let refresh = || {
        publisher.refresh(start, finish, |channel, items| {
            future_projection(channel, items, future)
        })
    };
    assert!(matches!(
        refresh().await.unwrap(),
        RefreshOutcome::Published { items: 2 }
    ));
    let folder = publisher.folder.join("indexed");
    let index: PlayoutIndex =
        serde_json::from_slice(&std::fs::read(folder.join(INDEX_FILENAME)).unwrap()).unwrap();
    assert!(!index.days.is_empty());
    assert!(!publisher
        .folder
        .join(publication::PLAYOUT_FILENAME)
        .exists());
    let reader = File::open(folder.join(READER_LOCK_FILENAME)).unwrap();
    reader.try_lock_shared().unwrap();
    assert!(matches!(refresh().await.unwrap(), RefreshOutcome::Busy));
    drop(reader);
    assert!(matches!(
        refresh().await.unwrap(),
        RefreshOutcome::Published { items: 2 }
    ));
    channel.streaming_engine = StreamingEngine::Legacy;
    make_repositories(pool)
        .channel
        .upsert(&channel)
        .await
        .unwrap();
    assert!(matches!(refresh().await, Err(RefreshError::Retired)));
    assert_eq!(
        serde_json::to_value(&index).unwrap(),
        serde_json::from_slice::<serde_json::Value>(
            &std::fs::read(folder.join(INDEX_FILENAME)).unwrap()
        )
        .unwrap()
    );
}

#[tokio::test]
async fn indexed_candidate_compensates_when_the_real_private_sql_commit_is_refused() {
    let root = tempfile::tempdir().unwrap();
    let (pool, locker, channel, start) = indexed_fixture(root.path()).await;
    let publisher = ChannelPublisher::new_indexed(
        pool.clone(),
        locker.clone(),
        root.path(),
        channel.id,
        ProgrammeClock::named("UTC").unwrap(),
        crate::next_engine::indexed_runtime::IndexedReaderPermit::fixture(),
    );
    let future = Utc::now() + Duration::hours(1);
    let finish = start + Duration::minutes(2);
    publisher
        .refresh(start, finish, |channel, items| {
            future_projection(channel, items, future)
        })
        .await
        .unwrap();
    let path = publisher.folder.join("indexed").join(INDEX_FILENAME);
    let accepted = std::fs::read(&path).unwrap();
    let repository = crate::repositories::SqlitePlayoutRepository(pool.clone());
    let playout = repository.get_with_anchor(1).await.unwrap();
    let _guard = PlayoutLockGuard::try_acquire(&locker, 1).unwrap();
    let mut pending = repository
        .prepare_build_result(
            &SchedulingResult {
                items_to_remove: vec![1],
                ..Default::default()
            },
            &playout,
            Duration::zero(),
        )
        .await
        .unwrap();
    assert_eq!(
        pending.items_in_range(start, finish).await.unwrap().len(),
        1
    );
    let staged = publisher
        .prepare_build(
            &mut pending,
            start,
            finish,
            Instant::now() + std::time::Duration::from_secs(5),
            |channel, items| future_projection(channel, items, future),
        )
        .await
        .unwrap()
        .unwrap();
    let applied = staged.publish_reversible().unwrap();
    assert_ne!(std::fs::read(&path).unwrap(), accepted);
    pending.restrict_deadline(Instant::now());
    assert!(
        pending.commit().await.is_err(),
        "real SQL finalizer refuses the expired candidate"
    );
    drop(applied);
    assert_eq!(std::fs::read(path).unwrap(), accepted);
    let count: i64 =
        sqlx::query_scalar(r#"SELECT COUNT(*) FROM "PlayoutItem" WHERE "PlayoutId"=1"#)
            .fetch_one(&pool)
            .await
            .unwrap();
    assert_eq!(count, 2, "the entire accepted SQL timeline remains intact");
}
