//! The real host finalizer retains admission and owners after uncertain cleanup.

use super::*;
use crate::{hls::InMemoryHlsSessionRegistry, remote_stream::RemoteStreamError};
use std::sync::Weak;

struct Fixture {
    _root: tempfile::TempDir,
    host: NextHost,
    registry: InMemoryHlsSessionRegistry,
    request: NextStart,
    session: Arc<NextSession>,
    instance: uuid::Uuid,
    reservation: Option<ActiveTranscodeReservation>,
    transfers: Weak<super::super::super::session_transfers::SessionTransfers>,
    capacity_key: String,
}

impl Fixture {
    async fn new() -> Self {
        let pool = sqlx::sqlite::SqlitePoolOptions::new()
            .connect_lazy("sqlite::memory:")
            .unwrap();
        // A closed pool distinguishes revoked callbacks from downstream SQL.
        pool.close().await;
        let root = tempfile::tempdir().unwrap();
        let host = NextHost::new(pool, EntityLocker::default(), root.path().to_owned());
        let mut request = request(false);
        request.output = root.path().join("owned-output");
        tokio::fs::create_dir(&request.output).await.unwrap();
        tokio::fs::write(request.output.join("sentinel"), b"owned")
            .await
            .unwrap();
        let session = Arc::new(NextSession::new(
            request.number.clone(),
            request.output.clone(),
            request.deadline,
        ));
        let registry = InMemoryHlsSessionRegistry::new();
        assert!(registry.try_insert(session.clone()).await);
        let instance = retain_dynamic_context(
            &mut *host.dynamic_contexts.lock().await,
            &request,
            Utc::now(),
        );
        let transfers = Arc::downgrade(&host.dynamic_contexts.lock().await[&instance].transfers);
        let capacity_key = format!("next-cleanup-{}", uuid::Uuid::new_v4());
        let reservation = try_reserve_active_transcode(
            i32::MAX,
            ActiveTranscodeSlot::new_next_hls_with_capacity_key(&request.number, &capacity_key),
        );
        assert!(reservation.is_some());
        Self {
            _root: root,
            host,
            registry,
            request,
            session,
            instance,
            reservation,
            transfers,
            capacity_key,
        }
    }

    fn reacquire(&self) -> Option<ActiveTranscodeReservation> {
        try_reserve_active_transcode(
            i32::MAX,
            ActiveTranscodeSlot::new_next_hls_with_capacity_key(
                &self.request.number,
                &self.capacity_key,
            ),
        )
    }

    async fn checkpoint_before_release(&self) {
        assert!(!self
            .host
            .dynamic_contexts
            .lock()
            .await
            .contains_key(&self.instance));
        assert!(
            self.transfers.upgrade().is_some(),
            "removed context must remain owned"
        );
        assert_eq!(
            tokio::fs::read(self.request.output.join("sentinel"))
                .await
                .unwrap(),
            b"owned"
        );
        let registered = self.registry.try_get(&self.request.number).await.unwrap();
        let expected = self.session.clone() as Arc<dyn HlsSessionHandle>;
        assert!(Arc::ptr_eq(&registered, &expected));
        assert!(
            self.reacquire().is_none(),
            "checkpoint must settle before admission is released"
        );
        let now = Utc::now();
        let error = self
            .host
            .resolve_dynamic(
                1,
                1,
                "fixture",
                None,
                "1",
                self.instance,
                now,
                now + chrono::Duration::seconds(10),
            )
            .await
            .err()
            .unwrap();
        assert_eq!(
            error.to_string(),
            "Next dynamic item is unavailable: session is unavailable"
        );
    }

    async fn finish(&mut self, outcome: Result<SessionExit, SessionError>, unconfirmed: bool) {
        let completion = SessionCompletion {
            request: self.request.clone(),
            session: self.session.clone(),
            instance: self.instance,
            outcome,
            reservation: self.reservation.take(),
        };
        self.host
            .finalize_session(&self.registry, completion, self.checkpoint_before_release())
            .await;
        assert_eq!(self.request.output.exists(), unconfirmed);
        assert_eq!(
            self.registry.try_get(&self.request.number).await.is_some(),
            unconfirmed
        );
        assert_eq!(self.transfers.upgrade().is_some(), unconfirmed);
        assert_eq!(self.reacquire().is_none(), unconfirmed);
        let quarantine = self.host.cleanup_quarantine.lock().await;
        assert_eq!(quarantine.len(), usize::from(unconfirmed));
        if unconfirmed {
            assert_eq!(
                tokio::fs::read(self.request.output.join("sentinel"))
                    .await
                    .unwrap(),
                b"owned"
            );
        }
    }

    async fn producer_cleanup(&self, error: RemoteStreamError) {
        use super::super::super::session_transfers::TransferBinding;
        let transfers = self.transfers.upgrade().unwrap();
        let now = Utc::now();
        let result = transfers
            .prepare(
                TransferBinding {
                    media_item_id: 1,
                    playout_item_id: 1,
                    media_version_id: 1,
                    media_file_id: 1,
                    publication: "synthetic".into(),
                    start: now,
                    finish: now + chrono::Duration::seconds(10),
                },
                Instant::now() + Duration::from_secs(5),
                Instant::now() + Duration::from_secs(10),
                move |_| async move { Err(error) },
            )
            .await;
        assert!(result.is_err());
    }
}

#[test]
fn next_cleanup_finalization_retains_context_output_registry_and_capacity_only_when_unconfirmed() {
    let _guard = crate::transcode_capacity::active_transcode_test_guard_for_tests();
    tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .unwrap()
        .block_on(async {
            for error in [
                ProcessError::CleanupTimeout,
                ProcessError::Io {
                    operation: "stop",
                    kind: std::io::ErrorKind::PermissionDenied,
                },
                ProcessError::Io {
                    operation: "wait",
                    kind: std::io::ErrorKind::Other,
                },
                ProcessError::ExitedBeforeReady(Some(1)),
                ProcessError::StartupTimeout,
            ] {
                let unconfirmed = matches!(
                    &error,
                    ProcessError::CleanupTimeout
                        | ProcessError::Io {
                            operation: "stop",
                            ..
                        }
                );
                let mut fixture = Fixture::new().await;
                fixture
                    .finish(Err(SessionError::Process(error)), unconfirmed)
                    .await;
                let capacity_key = fixture.capacity_key.clone();
                let transfers = fixture.transfers.clone();
                drop(fixture);
                assert!(transfers.upgrade().is_none());
                assert!(try_reserve_active_transcode(
                    i32::MAX,
                    ActiveTranscodeSlot::new_next_hls_with_capacity_key("1", &capacity_key)
                )
                .is_some());
            }
        });
}

#[test]
fn host_finalization_retains_complete_context_for_unconfirmed_prepared_producer_only() {
    let _guard = crate::transcode_capacity::active_transcode_test_guard_for_tests();
    tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .unwrap()
        .block_on(async {
            for (error, unconfirmed) in [
                (RemoteStreamError::PreparedTerminationUnconfirmed, true),
                (RemoteStreamError::PreparedProducerFailed, false),
            ] {
                let mut fixture = Fixture::new().await;
                fixture.producer_cleanup(error).await;
                fixture
                    .finish(Ok(SessionExit::Cancelled), unconfirmed)
                    .await;
            }
        });
}
