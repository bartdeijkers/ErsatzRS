use super::*;
use std::path::Path;

type Timeline = Vec<(i64, i64, String, String)>;
async fn timeline(pool: &DbPool) -> Timeline {
    sqlx::query_as(r#"SELECT "Id","MediaItemId","Start","Finish" FROM "PlayoutItem" ORDER BY "Id""#)
        .fetch_all(pool)
        .await
        .unwrap()
}
fn owner(pool: &DbPool, locker: &EntityLocker, root: &Path, id: i64) -> ChannelPublisher {
    ChannelPublisher::new_indexed(
        pool.clone(),
        locker.clone(),
        root,
        id,
        ProgrammeClock::named("UTC").unwrap(),
        crate::next_engine::indexed_runtime::IndexedReaderPermit::fixture(),
    )
}
async fn refresh(
    owner: &ChannelPublisher,
    start: DateTime<Utc>,
    future: DateTime<Utc>,
) -> Result<RefreshOutcome, RefreshError> {
    owner
        .refresh(start, start + Duration::minutes(2), |channel, items| {
            future_projection(channel, items, future)
        })
        .await
}

#[tokio::test]
async fn restarted_owner_recovers_accepted_sql_then_clones_preserve_noop_write_economy() {
    let root = tempfile::tempdir().unwrap();
    let (pool, locker, channel, start) = indexed_fixture(root.path()).await;
    let future = Utc::now() + Duration::hours(1);
    let original = owner(&pool, &locker, root.path(), channel.id);
    refresh(&original, start, future).await.unwrap();
    let folder = original.folder.join("indexed");
    let path = folder.join(INDEX_FILENAME);
    let accepted = std::fs::read(&path).unwrap();
    let rows = timeline(&pool).await;
    drop(original);
    let backup = folder.join(".next-rollback-Stranded");
    std::fs::write(&backup, &accepted).unwrap();
    std::fs::write(&path, b"{unreadable cache").unwrap();
    let restarted = owner(&pool, &locker, root.path(), channel.id);
    let shared = restarted.clone();
    let reader = File::open(folder.join(READER_LOCK_FILENAME)).unwrap();
    reader.try_lock_shared().unwrap();
    assert!(matches!(
        refresh(&restarted, start, future).await.unwrap(),
        RefreshOutcome::Busy
    ));
    assert_eq!(std::fs::read(&path).unwrap(), b"{unreadable cache");
    assert!(backup.exists());
    drop(reader);
    assert!(matches!(
        refresh(&restarted, start, future).await.unwrap(),
        RefreshOutcome::Published { items: 2 }
    ));
    assert_eq!(std::fs::read(&path).unwrap(), accepted);
    assert_eq!(timeline(&pool).await, rows);
    assert!(!backup.exists());
    File::options()
        .write(true)
        .open(&path)
        .unwrap()
        .set_times(std::fs::FileTimes::new().set_modified(
            std::time::SystemTime::UNIX_EPOCH + std::time::Duration::from_secs(1_600_000_000),
        ))
        .unwrap();
    let modified = path.metadata().unwrap().modified().unwrap();
    refresh(&shared, start, future).await.unwrap();
    assert_eq!(
        path.metadata().unwrap().modified().unwrap(),
        modified,
        "startup completion is shared across owner clones"
    );
}

#[tokio::test]
async fn a_private_sql_candidate_cannot_be_used_for_first_startup_recovery() {
    let root = tempfile::tempdir().unwrap();
    let (pool, locker, channel, start) = indexed_fixture(root.path()).await;
    let publisher = owner(&pool, &locker, root.path(), channel.id);
    let folder = publisher.folder.join("indexed");
    std::fs::create_dir_all(&folder).unwrap();
    let path = folder.join(INDEX_FILENAME);
    std::fs::write(&path, b"broken old cache").unwrap();
    let repository = crate::repositories::SqlitePlayoutRepository(pool.clone());
    let playout = repository.get_with_anchor(1).await.unwrap();
    let guard = PlayoutLockGuard::try_acquire(&locker, 1).unwrap();
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
        pending
            .items_in_range(start, start + Duration::minutes(2))
            .await
            .unwrap()
            .len(),
        1
    );
    let result = publisher
        .prepare_build(
            &mut pending,
            start,
            start + Duration::minutes(2),
            Instant::now() + std::time::Duration::from_secs(5),
            |_, _| async {
                panic!("private projection must not run before accepted-SQL startup recovery")
            },
        )
        .await;
    assert!(matches!(
        result,
        Err(RefreshError::Publication(
            publication::PublicationError::RecoveryRequired
        ))
    ));
    assert_eq!(std::fs::read(&path).unwrap(), b"broken old cache");
    drop(pending);
    drop(guard);
    assert!(matches!(
        refresh(&publisher, start, Utc::now() + Duration::hours(1))
            .await
            .unwrap(),
        RefreshOutcome::Published { items: 2 }
    ));
    assert_eq!(timeline(&pool).await.len(), 2);
}

#[tokio::test]
async fn reused_owner_recovers_lost_index_and_chunk_and_never_publishes_a_corrupt_private_candidate(
) {
    let root = tempfile::tempdir().unwrap();
    let (pool, locker, channel, start) = indexed_fixture(root.path()).await;
    let publisher = owner(&pool, &locker, root.path(), channel.id);
    let future = Utc::now() + Duration::hours(1);
    refresh(&publisher, start, future).await.unwrap();
    let folder = publisher.folder.join("indexed");
    let path = folder.join(INDEX_FILENAME);
    let accepted = std::fs::read(&path).unwrap();
    let rows = timeline(&pool).await;
    let index: PlayoutIndex = serde_json::from_slice(&accepted).unwrap();
    std::fs::remove_file(folder.join(&index.days[0].file)).unwrap();
    refresh(&publisher, start, future).await.unwrap();
    assert_eq!(std::fs::read(&path).unwrap(), accepted);
    assert!(folder.join(&index.days[0].file).exists());
    std::fs::remove_file(&path).unwrap();
    refresh(&publisher, start, future).await.unwrap();
    assert_eq!(std::fs::read(&path).unwrap(), accepted);
    std::fs::write(&path, b"corrupt derived index").unwrap();
    let repository = crate::repositories::SqlitePlayoutRepository(pool.clone());
    let playout = repository.get_with_anchor(1).await.unwrap();
    let guard = PlayoutLockGuard::try_acquire(&locker, 1).unwrap();
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
    let result = publisher
        .prepare_build(
            &mut pending,
            start,
            start + Duration::minutes(2),
            Instant::now() + std::time::Duration::from_secs(5),
            |channel, items| future_projection(channel, items, future),
        )
        .await;
    assert!(matches!(
        result,
        Err(RefreshError::Publication(
            publication::PublicationError::InvalidIndex
        ))
    ));
    assert_eq!(std::fs::read(&path).unwrap(), b"corrupt derived index");
    drop(pending);
    drop(guard);
    refresh(&publisher, start, future).await.unwrap();
    assert_eq!(std::fs::read(&path).unwrap(), accepted);
    assert_eq!(timeline(&pool).await, rows);
}

#[tokio::test]
async fn indexed_startup_recovery_does_not_run_for_native_channels() {
    let root = tempfile::tempdir().unwrap();
    let (pool, locker, mut channel, start) = indexed_fixture(root.path()).await;
    channel.streaming_engine = StreamingEngine::Legacy;
    make_repositories(pool.clone())
        .channel
        .upsert(&channel)
        .await
        .unwrap();
    let publisher = owner(&pool, &locker, root.path(), channel.id);
    let folder = publisher.folder.join("indexed");
    std::fs::create_dir_all(&folder).unwrap();
    let path = folder.join(INDEX_FILENAME);
    std::fs::write(&path, b"preserve").unwrap();
    assert!(matches!(
        refresh(&publisher, start, Utc::now()).await,
        Err(RefreshError::Retired)
    ));
    assert_eq!(std::fs::read(path).unwrap(), b"preserve");
    assert_eq!(
        make_repositories(pool)
            .channel
            .get_with_profile(channel.id)
            .await
            .unwrap()
            .streaming_engine,
        StreamingEngine::Legacy
    );
}

#[tokio::test]
async fn invalid_private_projection_does_not_invalidate_the_healthy_accepted_cache() {
    let root = tempfile::tempdir().unwrap();
    let (pool, locker, channel, start) = indexed_fixture(root.path()).await;
    let publisher = owner(&pool, &locker, root.path(), channel.id);
    let future = Utc::now() + Duration::hours(1);
    refresh(&publisher, start, future).await.unwrap();
    let path = publisher.folder.join("indexed").join(INDEX_FILENAME);
    let accepted = std::fs::read(&path).unwrap();
    let repository = crate::repositories::SqlitePlayoutRepository(pool.clone());
    let playout = repository.get_with_anchor(1).await.unwrap();
    let guard = PlayoutLockGuard::try_acquire(&locker, 1).unwrap();
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
    let result = publisher
        .prepare_build(
            &mut pending,
            start,
            start + Duration::minutes(2),
            Instant::now() + std::time::Duration::from_secs(5),
            |channel, items| async move {
                let mut projected = future_projection(channel, items, future).await?;
                projected[0].finish = projected[0].start;
                Ok(projected)
            },
        )
        .await;
    assert!(matches!(
        result,
        Err(RefreshError::Publication(
            publication::PublicationError::InvalidWindow
        ))
    ));
    assert_eq!(std::fs::read(&path).unwrap(), accepted);
    let retry = publisher
        .prepare_build(
            &mut pending,
            start,
            start + Duration::minutes(2),
            Instant::now() + std::time::Duration::from_secs(5),
            |channel, items| future_projection(channel, items, future),
        )
        .await
        .unwrap()
        .unwrap();
    drop(retry);
    drop(pending);
    drop(guard);
    assert_eq!(std::fs::read(path).unwrap(), accepted);
    assert_eq!(timeline(&pool).await.len(), 2);
}
