use super::*;
use crate::next_engine::{
    fallback::{tests::fixture, tests::resolve, Gap},
    publication::indexed::{PlayoutIndex, INDEX_FILENAME},
};
use chrono::TimeDelta;
use ersatzrs_core::programme::ProgrammeClock;
use std::{future::Future, path::Path};

async fn visible(path: &Path) {
    tokio::time::timeout(Duration::from_secs(5), async {
        while !path.exists() {
            tokio::time::sleep(Duration::from_millis(10)).await;
        }
    })
    .await
    .expect("the owned worker reaches atomic replacement");
}

async fn pending<T>(future: std::pin::Pin<&mut impl Future<Output = T>>) {
    let mut future = future;
    std::future::poll_fn(|cx| {
        assert!(future.as_mut().poll(cx).is_pending());
        std::task::Poll::Ready(())
    })
    .await;
}

async fn indexed(host: &NextHost) -> ChannelPublisher {
    let publisher = ChannelPublisher::new_indexed(
        host.pool.clone(),
        host.locker.clone(),
        &host.root,
        1,
        ProgrammeClock::named("UTC").unwrap(),
        crate::next_engine::indexed_runtime::IndexedReaderPermit::fixture(),
    );
    host.publishers.lock().await.insert(1, publisher.clone());
    publisher
}

#[tokio::test]
async fn visible_recovery_keeps_exact_callbacks_after_cleanup_failure_or_awaiter_cancellation() {
    for cancel in [false, true] {
        let (host, normal, start, _root) = fixture().await;
        let publisher = indexed(&host).await;
        let folder = host.root.join("1/indexed");
        std::fs::create_dir_all(&folder).unwrap();
        let backup = folder.join(".next-rollback-Stranded");
        std::fs::write(&backup, b"untrusted old generation").unwrap();
        let gap = Gap {
            start: start + TimeDelta::seconds(60),
            finish: start + TimeDelta::seconds(180),
        };
        let (normal_gaps, preview, unrelated) = {
            let mut contexts = host.dynamic_contexts.lock().await;
            let request = contexts.get(&normal).unwrap().request.clone();
            let normal_gaps = contexts.get(&normal).unwrap().gaps.clone();
            let preview = retain_dynamic_context(
                &mut contexts,
                &NextStart {
                    preview: true,
                    ..request.clone()
                },
                start,
            );
            let unrelated = retain_dynamic_context(
                &mut contexts,
                &NextStart {
                    channel_id: 2,
                    number: "2".into(),
                    ..request
                },
                start,
            );
            (normal_gaps, preview, unrelated)
        };
        let mut held = normal_gaps.clone().lock_owned().await;
        held.clear();
        let refreshing = host.clone();
        let writing = publisher.clone();
        let mut task = Some(tokio::spawn(async move {
            refreshing
                .refresh(
                    &writing,
                    1,
                    &[],
                    std::time::Instant::now() + Duration::from_secs(5),
                )
                .await
        }));
        visible(&folder.join(INDEX_FILENAME)).await;
        // Replacement is visible, but the commit hook cannot yet register the
        // normal reader. Inject cleanup refusal strictly after that commit.
        std::fs::remove_file(&backup).unwrap();
        std::fs::create_dir(&backup).unwrap();
        let callback = resolve(&host, normal, start, 90);
        tokio::pin!(callback);
        pending(callback.as_mut()).await;
        if cancel {
            let task = task.take().unwrap();
            task.abort();
            assert!(task.await.unwrap_err().is_cancelled());
        }
        drop(held);
        if let Some(task) = task {
            assert!(task.await.unwrap().is_err());
        }
        let selected = tokio::time::timeout(Duration::from_secs(5), callback)
            .await
            .unwrap()
            .unwrap();
        assert_eq!(
            serde_json::to_value(selected).unwrap()["tracks"]["audio"]["stream_index"],
            2
        );
        let _drained = publisher.callback_admission().await;
        let index: PlayoutIndex =
            serde_json::from_slice(&std::fs::read(folder.join(INDEX_FILENAME)).unwrap()).unwrap();
        assert!(index
            .days
            .iter()
            .all(|day| folder.join(&day.file).is_file()));
        let contexts = host.dynamic_contexts.lock().await;
        for instance in [normal, preview] {
            assert!(contexts
                .get(&instance)
                .unwrap()
                .gaps
                .lock()
                .await
                .contains(&gap));
        }
        assert!(contexts
            .get(&unrelated)
            .unwrap()
            .gaps
            .lock()
            .await
            .is_empty());
    }
}

#[tokio::test]
async fn refused_recovery_does_not_admit_unpublished_gaps() {
    let (host, normal, start, _root) = fixture().await;
    let publisher = indexed(&host).await;
    let folder = host.root.join("1/indexed");
    std::fs::create_dir_all(folder.join(".next-rollback-Unsafe")).unwrap();
    host.dynamic_contexts
        .lock()
        .await
        .get(&normal)
        .unwrap()
        .gaps
        .lock()
        .await
        .clear();
    assert!(host
        .refresh(
            &publisher,
            1,
            &[],
            std::time::Instant::now() + Duration::from_secs(5)
        )
        .await
        .is_err());
    assert!(!folder.join(INDEX_FILENAME).exists());
    assert!(resolve(&host, normal, start, 90).await.is_err());
    assert!(host
        .dynamic_contexts
        .lock()
        .await
        .get(&normal)
        .unwrap()
        .gaps
        .lock()
        .await
        .is_empty());
}

#[tokio::test]
async fn waiting_callback_rechecks_revocation_and_unknown_channel_does_not_create_owner() {
    let (host, normal, start, _root) = fixture().await;
    let publisher = host.publisher(1).await;
    let held = publisher.callback_admission().await;
    let callback = resolve(&host, normal, start, 90);
    tokio::pin!(callback);
    pending(callback.as_mut()).await;
    host.dynamic_contexts.lock().await.remove(&normal);
    // An unrelated channel remains independent of the held publication.
    assert!(host.callback_admission(99).await.is_none());
    assert_eq!(host.publishers.lock().await.len(), 1);
    drop(held);
    assert!(callback.await.is_err());
}

#[tokio::test]
async fn callback_publication_wait_uses_the_existing_thirty_second_budget() {
    let (host, normal, start, _root) = fixture().await;
    let publisher = host.publisher(1).await;
    let _held = publisher.callback_admission().await;
    tokio::time::pause();
    let callback = resolve(&host, normal, start, 90);
    tokio::pin!(callback);
    pending(callback.as_mut()).await;
    tokio::time::advance(Duration::from_secs(29)).await;
    pending(callback.as_mut()).await;
    tokio::time::advance(Duration::from_secs(1)).await;
    assert!(callback.await.is_err());
}

#[tokio::test]
async fn prepared_host_publication_registers_before_releasing_owned_worker_on_cancel() {
    use ersatzrs_core::{repositories::PlayoutRepository, scheduling::SchedulingResult};
    let root = tempfile::tempdir().unwrap();
    let (host, start) = super::publication::tests::fixture(root.path()).await;
    let repository = crate::repositories::SqlitePlayoutRepository(host.pool.clone());
    let playout = repository.get_with_anchor(1).await.unwrap();
    let _guard = ersatzrs_core::locking::PlayoutLockGuard::try_acquire(&host.locker, 1).unwrap();
    let mut sql = repository
        .prepare_build_result(&SchedulingResult::default(), &playout, TimeDelta::zero())
        .await
        .unwrap();
    let file = host
        .prepare_build(
            &mut sql,
            1,
            start - TimeDelta::seconds(5)..start + TimeDelta::seconds(120),
            &[],
            std::time::Instant::now() + Duration::from_secs(5),
        )
        .await
        .unwrap()
        .unwrap();
    sql.commit().await.unwrap();
    let gaps = host
        .dynamic_contexts
        .lock()
        .await
        .values()
        .find(|context| context.request.channel_id == 1 && !context.request.preview)
        .unwrap()
        .gaps
        .clone();
    let held = gaps.clone().lock_owned().await;
    let task = tokio::spawn(file.publish());
    visible(
        &host
            .root
            .join("1")
            .join(crate::next_engine::publication::PLAYOUT_FILENAME),
    )
    .await;
    task.abort();
    assert!(task.await.unwrap_err().is_cancelled());
    let publisher = host.publisher(1).await;
    let drained = publisher.callback_admission();
    tokio::pin!(drained);
    pending(drained.as_mut()).await;
    drop(held);
    let _admission = tokio::time::timeout(Duration::from_secs(5), drained)
        .await
        .unwrap();
    assert!(gaps.lock().await.contains(&Gap {
        start: start - TimeDelta::seconds(5),
        finish: start
    }));
}

#[tokio::test]
async fn indexed_host_refuses_unqualified_request_before_output_or_publication() {
    let (host, _, _, root) = fixture().await;
    let publisher = indexed(&host).await;
    assert!(publisher
        .authorize_reader(Path::new("another-runtime"))
        .await
        .is_err());
    let registry = Arc::new(crate::hls::InMemoryHlsSessionRegistry::default());
    let context_count = host.dynamic_contexts.lock().await.len();
    let mut request = super::tests::request(false);
    request.output = root.path().join("must-not-be-created");
    let output = request.output.clone();
    let handle = host.start(registry.clone(), request).await.unwrap();
    tokio::time::timeout(Duration::from_secs(5), handle)
        .await
        .unwrap()
        .unwrap();
    assert!(!output.exists());
    assert!(!host.root.join("1/indexed/index.json").exists());
    assert!(registry.try_get("1").await.is_none());
    assert_eq!(host.dynamic_contexts.lock().await.len(), context_count);
}
