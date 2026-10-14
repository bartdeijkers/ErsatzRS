use super::*;
use crate::{
    next_engine::{host::NextStart, publication::PLAYOUT_FILENAME, schedule, selection},
    repositories::SqlitePlayoutRepository,
};
use ersatzrs_core::{
    locking::PlayoutLockGuard,
    repositories::{ChannelRepository, PlayoutRepository},
    scheduling::SchedulingResult,
};
use ersatzrs_domain::{ChannelPlayoutSource, StreamingEngine};

pub(crate) async fn fixture(root: &std::path::Path) -> (Arc<NextHost>, DateTime<Utc>) {
    let pool = sqlx::sqlite::SqlitePoolOptions::new()
        .max_connections(4)
        .connect_with(
            sqlx::sqlite::SqliteConnectOptions::new()
                .filename(root.join("owners.db"))
                .create_if_missing(true)
                .journal_mode(sqlx::sqlite::SqliteJournalMode::Wal),
        )
        .await
        .unwrap();
    let pool = selection::tests::seed_fixture_pool(pool).await;
    let (pool, locker, mut channel, start) = schedule::tests::fixture_with_pool(pool).await;
    sqlx::query(r#"INSERT INTO "LocalLibrary" ("Id") VALUES (1)"#)
        .execute(&pool)
        .await
        .unwrap();
    sqlx::query(r#"UPDATE "PlayoutItem" SET "OutPoint" = '00:01:00'"#)
        .execute(&pool)
        .await
        .unwrap();
    let repos = crate::repositories::make_repositories(pool.clone());
    channel.streaming_engine = StreamingEngine::Next;
    repos.channel.upsert(&channel).await.unwrap();
    repos
        .channel
        .upsert(&Channel {
            id: 2,
            number: "2".into(),
            playout_source: ChannelPlayoutSource::Mirror,
            mirror_source_channel_id: Some(1),
            playout_offset: Some(chrono::Duration::seconds(-5)),
            ..channel
        })
        .await
        .unwrap();
    let host = Arc::new(NextHost::new(pool, locker, root.to_owned()));
    host.media
        .configure("127.0.0.1:8409".parse().unwrap())
        .unwrap();
    for (id, preview) in [(1, false), (1, true), (2, true), (99, false)] {
        let request = NextStart {
            channel_id: id,
            preview,
            number: id.to_string(),
            executable: "fixture-next".into(),
            output: root.join(format!("output-{id}-{preview}")),
            ffmpeg: "fixture-ffmpeg".into(),
            ffprobe: "fixture-ffprobe".into(),
            replacements: vec![],
            idle_timeout: None,
            deadline: tokio::time::Instant::now() + std::time::Duration::from_secs(30),
        };
        super::super::retain_dynamic_context(
            &mut *host.dynamic_contexts.lock().await,
            &request,
            start,
        );
    }
    (host, start)
}

#[tokio::test]
async fn active_preparation_coalesces_main_preview_and_includes_leading_mirror() {
    let root = tempfile::tempdir().unwrap();
    let (host, start) = fixture(root.path()).await;
    let repository = SqlitePlayoutRepository(host.pool.clone());
    let playout = repository.get_with_anchor(1).await.unwrap();
    let _guard = PlayoutLockGuard::try_acquire(&host.locker, 1).unwrap();
    let item = ersatzrs_domain::PlayoutItem {
        id: 0,
        playout_id: 1,
        start,
        finish: start + chrono::Duration::seconds(60),
        in_point: chrono::Duration::zero(),
        out_point: chrono::Duration::seconds(60),
        ..selection::tests::item()
    };
    let result = SchedulingResult {
        clear_items: true,
        added_items: vec![item],
        ..Default::default()
    };
    for publish in [false, true] {
        let mut pending = repository
            .prepare_build_result(&result, &playout, chrono::Duration::zero())
            .await
            .unwrap();
        let prepared = host
            .prepare_active_build(
                &mut pending,
                start - chrono::Duration::seconds(5)..start + chrono::Duration::seconds(60),
                Instant::now() + std::time::Duration::from_secs(3),
            )
            .await
            .unwrap()
            .unwrap();
        assert_eq!(
            prepared
                .iter()
                .map(|owner| owner.channel_id)
                .collect::<Vec<_>>(),
            [1, 2]
        );
        assert!(!root.path().join("1").join(PLAYOUT_FILENAME).exists());
        assert!(!root.path().join("2").join(PLAYOUT_FILENAME).exists());
        if publish {
            pending.commit().await.unwrap();
            for owner in prepared {
                owner.publish().await.unwrap();
            }
            for (id, offset) in [(1, 0), (2, -5)] {
                let wire: protocol::Playout = serde_json::from_slice(
                    &std::fs::read(root.path().join(id.to_string()).join(PLAYOUT_FILENAME))
                        .unwrap(),
                )
                .unwrap();
                let item = wire.items.last().unwrap();
                assert_eq!(item.start, start + chrono::Duration::seconds(offset));
                assert_eq!(item.finish, start + chrono::Duration::seconds(60 + offset));
            }
        } else {
            drop(prepared);
            drop(pending);
            crate::db::begin_write(&host.pool)
                .await
                .unwrap()
                .rollback()
                .await
                .unwrap();
            assert_eq!(std::fs::read_dir(root.path().join("1")).unwrap().count(), 0);
            assert_eq!(std::fs::read_dir(root.path().join("2")).unwrap().count(), 0);
        }
        assert!(!root.path().join("99").exists());
    }
}

#[tokio::test]
async fn conflicting_shared_owner_configuration_refuses_before_staging() {
    let root = tempfile::tempdir().unwrap();
    let (host, start) = fixture(root.path()).await;
    host.dynamic_contexts
        .lock()
        .await
        .values_mut()
        .find(|context| context.request.channel_id == 1 && context.request.preview)
        .unwrap()
        .request
        .replacements
        .push(PathReplacement {
            source_path: "media".into(),
            replacement_path: "other".into(),
            source_platform: ersatzrs_core::path_replacement::PathPlatform::Unix,
        });
    let repository = SqlitePlayoutRepository(host.pool.clone());
    let playout = repository.get_with_anchor(1).await.unwrap();
    let _guard = PlayoutLockGuard::try_acquire(&host.locker, 1).unwrap();
    let mut pending = repository
        .prepare_build_result(
            &SchedulingResult::default(),
            &playout,
            chrono::Duration::zero(),
        )
        .await
        .unwrap();
    assert!(matches!(
        host.prepare_active_build(
            &mut pending,
            start..start + chrono::Duration::minutes(2),
            Instant::now() + std::time::Duration::from_secs(3)
        )
        .await,
        Err(RefreshError::Projection)
    ));
    assert!(!root.path().join("1").exists());
    assert!(!root.path().join("2").exists());
}

#[tokio::test]
async fn later_owner_timeout_discards_earlier_preparation_without_publishing() {
    let root = tempfile::tempdir().unwrap();
    let (host, start) = fixture(root.path()).await;
    let repository = SqlitePlayoutRepository(host.pool.clone());
    let playout = repository.get_with_anchor(1).await.unwrap();
    let _guard = PlayoutLockGuard::try_acquire(&host.locker, 1).unwrap();
    let mut pending = repository
        .prepare_build_result(
            &SchedulingResult::default(),
            &playout,
            chrono::Duration::zero(),
        )
        .await
        .unwrap();
    let window = start - chrono::Duration::seconds(5)..start + chrono::Duration::minutes(2);
    let held = host
        .prepare_build(
            &mut pending,
            2,
            window.clone(),
            &[],
            Instant::now() + std::time::Duration::from_secs(5),
        )
        .await
        .unwrap()
        .unwrap();
    let outcome = host
        .prepare_active_build(
            &mut pending,
            window.clone(),
            Instant::now() + std::time::Duration::from_millis(200),
        )
        .await;
    assert!(matches!(
        outcome,
        Err(RefreshError::Publication(
            crate::next_engine::publication::PublicationError::Expired
        ))
    ));
    assert!(!root.path().join("1").join(PLAYOUT_FILENAME).exists());
    assert!(!root.path().join("2").join(PLAYOUT_FILENAME).exists());
    // A timed-out blocking flush retains admission until its private file is
    // discarded. Reacquire that admission before asserting cleanup, rather
    // than racing the still-owned worker on a loaded machine.
    let drained = host
        .prepare_build(
            &mut pending,
            1,
            window,
            &[],
            Instant::now() + std::time::Duration::from_secs(5),
        )
        .await
        .unwrap()
        .unwrap();
    drop(drained);
    assert_eq!(std::fs::read_dir(root.path().join("1")).unwrap().count(), 0);
    assert!(!root.path().join("1").join(PLAYOUT_FILENAME).exists());
    drop(held);
    drop(pending);
    crate::db::begin_write(&host.pool)
        .await
        .unwrap()
        .rollback()
        .await
        .unwrap();
    assert_eq!(std::fs::read_dir(root.path().join("2")).unwrap().count(), 0);
}

#[tokio::test]
async fn requested_worker_rolls_back_failed_preparation_and_publishes_a_fresh_retry() {
    use crate::maintenance::{run_requested_playout_build_with_execution, PlayoutExecution};
    use ersatzrs_core::scheduling::{PlayoutBuildMode, PlayoutBuildRequest};
    let root = tempfile::tempdir().unwrap();
    let (host, start) = fixture(root.path()).await;
    let script = root.path().join("worker.lua");
    std::fs::write(
        &script,
        "add_collection('a', 'fixture', 'chronological') add_count('a', 4)",
    )
    .unwrap();
    sqlx::query(r#"UPDATE "Playout" SET "ScheduleKind" = 4, "ScheduleFile" = ? WHERE "Id" = 1"#)
        .bind(script.to_str().unwrap())
        .execute(&host.pool)
        .await
        .unwrap();
    sqlx::raw_sql(r#"INSERT INTO "Collection" ("Id", "Name", "UseCustomPlaybackOrder") VALUES (1, 'fixture', 0);
        INSERT INTO "CollectionItem" ("CollectionId", "MediaItemId") VALUES (1, 10);"#)
        .execute(&host.pool).await.unwrap();
    let repository = SqlitePlayoutRepository(host.pool.clone());
    let before = repository
        .items_in_range(1, start, start + chrono::Duration::hours(1))
        .await
        .unwrap();
    let old_playout = repository.get_with_anchor(1).await.unwrap();
    host.dynamic_contexts
        .lock()
        .await
        .values_mut()
        .find(|context| context.request.channel_id == 1 && context.request.preview)
        .unwrap()
        .request
        .replacements
        .push(PathReplacement {
            source_path: "media".into(),
            replacement_path: "different".into(),
            source_platform: ersatzrs_core::path_replacement::PathPlatform::Unix,
        });
    let request = PlayoutBuildRequest::new(1, PlayoutBuildMode::Reset);
    let sessions = unpolled_sessions(root.path()).await;
    let execution = PlayoutExecution {
        next: Some(&host),
        hls_sessions: Some(&sessions),
        ..Default::default()
    };
    let failure = run_requested_playout_build_with_execution(
        &host.pool,
        &host.locker,
        request,
        start,
        execution,
    )
    .await
    .unwrap_err();
    assert!(failure.to_string().contains("publication"));
    assert_eq!(
        repository
            .items_in_range(1, start, start + chrono::Duration::hours(1))
            .await
            .unwrap(),
        before
    );
    let mut failed_playout = repository.get_with_anchor(1).await.unwrap();
    assert!(!failed_playout.build_status.take().unwrap().success);
    assert_eq!(failed_playout, old_playout);
    assert!(!root.path().join("1").join(PLAYOUT_FILENAME).exists());
    assert!(!root.path().join("2").join(PLAYOUT_FILENAME).exists());
    for context in host.dynamic_contexts.lock().await.values_mut() {
        context.request.replacements.clear();
    }
    assert!(tokio::time::timeout(
        std::time::Duration::from_secs(3),
        run_requested_playout_build_with_execution(
            &host.pool,
            &host.locker,
            request,
            start,
            execution
        )
    )
    .await
    .expect("prepared publishers do not await a redundant session refresh")
    .unwrap());
    let committed = repository
        .items_in_range(1, start, start + chrono::Duration::hours(1))
        .await
        .unwrap();
    assert_eq!(committed.len(), 4);
    assert_published_candidates(root.path(), &committed);
    let success: bool =
        sqlx::query_scalar(r#"SELECT "Success" FROM "PlayoutBuildStatus" WHERE "PlayoutId" = 1"#)
            .fetch_one(&host.pool)
            .await
            .unwrap();
    assert!(success);
}

async fn unpolled_sessions(root: &std::path::Path) -> crate::hls::InMemoryHlsSessionRegistry {
    use ersatzrs_core::hls::HlsSessionRegistry;
    let sessions = crate::hls::InMemoryHlsSessionRegistry::new();
    for number in ["1", "2"] {
        sessions
            .insert(Arc::new(crate::next_engine::session::NextSession::new(
                number.into(),
                root.join(number),
                tokio::time::Instant::now() + std::time::Duration::from_secs(30),
            )))
            .await;
    }
    sessions
}

fn assert_published_candidates(root: &std::path::Path, committed: &[ersatzrs_domain::PlayoutItem]) {
    for (id, offset) in [(1, 0), (2, -5)] {
        let wire: protocol::Playout = serde_json::from_slice(
            &std::fs::read(root.join(id.to_string()).join(PLAYOUT_FILENAME)).unwrap(),
        )
        .unwrap();
        assert_eq!(wire.items.len(), committed.len());
        for (published, item) in wire.items.iter().zip(committed) {
            assert_eq!(published.id, item.id.to_string());
            assert_eq!(
                published.start,
                item.start + chrono::Duration::seconds(offset)
            );
            assert_eq!(
                published.finish,
                item.finish + chrono::Duration::seconds(offset)
            );
        }
    }
}
