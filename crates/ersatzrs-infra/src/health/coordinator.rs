//! Shared health-run ownership, matching upstream `HealthCheckService` (#3006).

use std::panic::AssertUnwindSafe;
use std::sync::Arc;
use std::time::Duration;

use ersatzrs_core::health::{HealthCheckError, HealthCheckResult};
use ersatzrs_ffmpeg::FfmpegRuntimeReport;
use futures_util::{future::BoxFuture, FutureExt};
use tokio::sync::{watch, Mutex, RwLock};
use tokio::time::Instant;

use super::{make_health_check_service, PersistedHealthCheckRun, SqlHealthCheckStore};
use crate::db::{DatabaseSchemaStatus, DbPool};

const CACHE_DURATION: Duration = Duration::from_mins(5);
const CHECK_TIMEOUT: Duration = Duration::from_mins(15);
type CheckRunner = Arc<dyn Fn() -> BoxFuture<'static, Vec<HealthCheckResult>> + Send + Sync>;
type RunResult = Result<PersistedHealthCheckRun, HealthRunError>;
type RunReceiver = watch::Receiver<Option<RunResult>>;

/// Failure of a shared health run, distinct from a failed individual check.
#[derive(Clone, Debug, thiserror::Error)]
pub enum HealthRunError {
    /// The run exceeded its check deadline; no partial results were published.
    #[error("health checks exceeded their execution deadline")]
    TimedOut,
    /// The host is shutting down, or its run task stopped unexpectedly.
    #[error("health check service stopped")]
    Stopped,
    /// A check panicked; the service remains available for another run.
    #[error("health check task failed")]
    TaskFailed,
    /// Completed results could not be persisted.
    #[error("health check publication failed: {0}")]
    Publication(#[source] Arc<HealthCheckError>),
}

#[derive(Default)]
struct RunState {
    cached: Option<(Instant, PersistedHealthCheckRun)>,
    in_flight: Option<RunReceiver>,
}

struct Inner {
    state: Mutex<RunState>,
    runner: CheckRunner,
    store: SqlHealthCheckStore,
    shutdown: watch::Sender<bool>,
    cache_duration: Duration,
    check_timeout: Duration,
}

/// One host-owned run shared by admin reads, refreshes and scheduled checks.
///
/// Dropping a caller only stops that caller's wait. Checks retain their own
/// lifetime and deadline; only complete, persisted results enter the cache.
#[derive(Clone)]
pub struct HealthCheckCoordinator {
    inner: Arc<Inner>,
}

impl HealthCheckCoordinator {
    /// Create the coordinator shared by this host's health consumers.
    #[must_use]
    pub fn new(
        pool: DbPool,
        runtime: Arc<RwLock<FfmpegRuntimeReport>>,
        schema_status: DatabaseSchemaStatus,
    ) -> Self {
        let check_pool = pool.clone();
        let runner: CheckRunner = Arc::new(move || {
            let pool = check_pool.clone();
            let runtime = runtime.clone();
            let schema = schema_status.clone();
            Box::pin(async move {
                let report = runtime.read().await.clone();
                make_health_check_service(pool, report, schema)
                    .perform_health_checks()
                    .await
            })
        });
        Self::from_runner(pool, runner, CACHE_DURATION, CHECK_TIMEOUT)
    }

    fn from_runner(
        pool: DbPool,
        runner: CheckRunner,
        cache_duration: Duration,
        check_timeout: Duration,
    ) -> Self {
        Self {
            inner: Arc::new(Inner {
                state: Mutex::new(RunState::default()),
                runner,
                store: SqlHealthCheckStore::new(pool),
                shutdown: watch::channel(false).0,
                cache_duration,
                check_timeout,
            }),
        }
    }

    /// Read cached results, or bypass the completed cache for an explicit refresh.
    /// Concurrent refreshes join the same run rather than starting more checks.
    ///
    /// # Errors
    /// Returns an error on deadline, shutdown, task failure or failed publication.
    pub async fn run(&self, refresh: bool) -> RunResult {
        let mut receiver = {
            let mut state = self.inner.state.lock().await;
            if *self.inner.shutdown.borrow() {
                return Err(HealthRunError::Stopped);
            }
            if !refresh {
                if let Some((expires, cached)) = &state.cached {
                    if Instant::now() < *expires {
                        return Ok(cached.clone());
                    }
                }
            }
            if let Some(receiver) = &state.in_flight {
                receiver.clone()
            } else {
                let (sender, receiver) = watch::channel(None);
                state.in_flight = Some(receiver.clone());
                let inner = self.inner.clone();
                // Intentionally service-owned: the state retains the result channel,
                // and shutdown/deadline bounds this task even when all callers leave.
                tokio::spawn(async move { inner.execute(sender).await });
                receiver
            }
        };
        wait_for_run(&mut receiver).await
    }

    /// Cancel the owned run and wait until its check future has been dropped.
    pub async fn shutdown(&self) {
        self.inner.shutdown.send_replace(true);
        let running = self.inner.state.lock().await.in_flight.clone();
        if let Some(mut receiver) = running {
            let _ = wait_for_run(&mut receiver).await;
        }
    }
}

impl Inner {
    async fn execute(self: Arc<Self>, sender: watch::Sender<Option<RunResult>>) {
        let mut shutdown = self.shutdown.subscribe();
        let result = AssertUnwindSafe(async {
            tokio::select! {
                biased;
                _ = shutdown.wait_for(|stopped| *stopped) => Err(HealthRunError::Stopped),
                result = self.check_and_publish() => result,
            }
        })
        .catch_unwind()
        .await
        .unwrap_or(Err(HealthRunError::TaskFailed));
        if let Err(error) = &result {
            tracing::warn!(%error, "health run did not publish new results");
        }
        let mut state = self.state.lock().await;
        if let Ok(run) = &result {
            state.cached = Some((Instant::now() + self.cache_duration, run.clone()));
        }
        state.in_flight = None;
        sender.send_replace(Some(result));
    }

    async fn check_and_publish(&self) -> RunResult {
        let results = tokio::time::timeout(self.check_timeout, (self.runner)())
            .await
            .map_err(|_| HealthRunError::TimedOut)?;
        // Publication only sees a complete check set. SQL connection/statement
        // deadlines still apply, and the store commits results and summary together.
        let summary = self
            .store
            .save_results(&results)
            .await
            .map_err(|error| HealthRunError::Publication(Arc::new(error)))?;
        Ok(PersistedHealthCheckRun { results, summary })
    }
}

async fn wait_for_run(receiver: &mut RunReceiver) -> RunResult {
    receiver
        .wait_for(Option::is_some)
        .await
        .map_err(|_| HealthRunError::Stopped)?
        .clone()
        .ok_or(HealthRunError::Stopped)?
}

#[cfg(test)]
mod tests {
    use std::sync::atomic::{AtomicUsize, Ordering};

    use ersatzrs_core::health::HealthCheckStatus;
    use tokio::sync::{Notify, Semaphore};

    use super::*;

    async fn pool() -> DbPool {
        let pool = sqlx::sqlite::SqlitePoolOptions::new()
            .max_connections(1)
            .connect("sqlite::memory:")
            .await
            .unwrap();
        sqlx::query(r#"CREATE TABLE "ConfigElement" ("Key" TEXT PRIMARY KEY, "Value" TEXT)"#)
            .execute(&pool)
            .await
            .unwrap();
        pool
    }

    fn rows(number: usize) -> Vec<HealthCheckResult> {
        vec![HealthCheckResult::new(
            format!("Run {number}"),
            HealthCheckStatus::Warning,
            "warning",
            "warning",
        )]
    }

    fn counting_runner(calls: &Arc<AtomicUsize>) -> CheckRunner {
        let calls = calls.clone();
        Arc::new(move || {
            let calls = calls.clone();
            Box::pin(async move { rows(calls.fetch_add(1, Ordering::SeqCst) + 1) })
        })
    }

    #[tokio::test]
    async fn concurrent_refreshes_survive_a_cancelled_waiter_and_publish_once() {
        let pool = pool().await;
        let calls = Arc::new(AtomicUsize::new(0));
        let started = Arc::new(Notify::new());
        let release = Arc::new(Semaphore::new(0));
        let runner: CheckRunner = {
            let (calls, started, release) = (calls.clone(), started.clone(), release.clone());
            Arc::new(move || {
                let (calls, started, release) = (calls.clone(), started.clone(), release.clone());
                Box::pin(async move {
                    let number = calls.fetch_add(1, Ordering::SeqCst) + 1;
                    started.notify_one();
                    release.acquire().await.unwrap().forget();
                    rows(number)
                })
            })
        };
        let coordinator = HealthCheckCoordinator::from_runner(
            pool.clone(),
            runner,
            CACHE_DURATION,
            CHECK_TIMEOUT,
        );
        let mut first = Box::pin(coordinator.run(false));
        assert!(futures_util::poll!(first.as_mut()).is_pending());
        started.notified().await;
        let mut second = Box::pin(coordinator.run(true));
        assert!(futures_util::poll!(second.as_mut()).is_pending());
        drop(first);
        release.add_permits(1);
        let result = second.await.unwrap();
        assert_eq!(result.results, rows(1));
        assert_eq!(calls.load(Ordering::SeqCst), 1);
        assert_eq!(coordinator.run(false).await.unwrap(), result);
        let store = SqlHealthCheckStore::new(pool);
        assert_eq!(store.load_results().await.unwrap(), result.results);
        assert_eq!(store.load_summary().await.unwrap(), result.summary);
    }

    #[tokio::test]
    async fn leaving_the_last_page_does_not_cancel_publication() {
        let pool = pool().await;
        let release = Arc::new(Semaphore::new(0));
        let started = Arc::new(Notify::new());
        let runner: CheckRunner = {
            let (release, started) = (release.clone(), started.clone());
            Arc::new(move || {
                let (release, started) = (release.clone(), started.clone());
                Box::pin(async move {
                    started.notify_one();
                    release.acquire().await.unwrap().forget();
                    rows(1)
                })
            })
        };
        let coordinator = HealthCheckCoordinator::from_runner(
            pool.clone(),
            runner,
            CACHE_DURATION,
            CHECK_TIMEOUT,
        );
        let mut page = Box::pin(coordinator.run(false));
        assert!(futures_util::poll!(page.as_mut()).is_pending());
        started.notified().await;
        drop(page);
        release.add_permits(1);
        // No waiting reader remains to keep the check alive or publish it.
        let store = SqlHealthCheckStore::new(pool);
        tokio::time::timeout(Duration::from_secs(2), async {
            while store.load_results().await.unwrap() != rows(1) {
                tokio::task::yield_now().await;
            }
        })
        .await
        .expect("service-owned run publishes after its last reader leaves");
        assert_eq!(coordinator.run(false).await.unwrap().results, rows(1));
    }

    #[tokio::test]
    async fn completed_cache_expires_and_explicit_refresh_bypasses_it() {
        let calls = Arc::new(AtomicUsize::new(0));
        let coordinator = HealthCheckCoordinator::from_runner(
            pool().await,
            counting_runner(&calls),
            CACHE_DURATION,
            CHECK_TIMEOUT,
        );
        assert_eq!(coordinator.run(false).await.unwrap().results, rows(1));
        assert_eq!(coordinator.run(false).await.unwrap().results, rows(1));
        assert_eq!(coordinator.run(true).await.unwrap().results, rows(2));
        // Move only the completed cache deadline; no real five-minute wait or
        // paused SQL pool timer is needed to exercise the expired-read path.
        coordinator
            .inner
            .state
            .lock()
            .await
            .cached
            .as_mut()
            .unwrap()
            .0 = Instant::now();
        assert_eq!(coordinator.run(false).await.unwrap().results, rows(3));
        assert_eq!(calls.load(Ordering::SeqCst), 3);
    }

    #[tokio::test]
    async fn publication_failure_preserves_complete_previous_results_and_recovers() {
        let pool = pool().await;
        let calls = Arc::new(AtomicUsize::new(0));
        let coordinator = HealthCheckCoordinator::from_runner(
            pool.clone(),
            counting_runner(&calls),
            CACHE_DURATION,
            CHECK_TIMEOUT,
        );
        let before = coordinator.run(false).await.unwrap();
        sqlx::query(
            r#"CREATE TRIGGER "RejectHealthSummary" BEFORE INSERT ON "ConfigElement"
            WHEN NEW."Key" = 'health.summary' BEGIN SELECT RAISE(ABORT, 'injected failure'); END"#,
        )
        .execute(&pool)
        .await
        .unwrap();
        assert!(matches!(
            coordinator.run(true).await,
            Err(HealthRunError::Publication(_))
        ));
        let store = SqlHealthCheckStore::new(pool.clone());
        assert_eq!(store.load_results().await.unwrap(), before.results);
        assert_eq!(store.load_summary().await.unwrap(), before.summary);
        assert_eq!(coordinator.run(false).await.unwrap(), before);
        sqlx::query(r#"DROP TRIGGER "RejectHealthSummary""#)
            .execute(&pool)
            .await
            .unwrap();
        assert_eq!(coordinator.run(true).await.unwrap().results, rows(3));
    }

    struct Dropped(Arc<AtomicUsize>);
    impl Drop for Dropped {
        fn drop(&mut self) {
            self.0.fetch_add(1, Ordering::SeqCst);
        }
    }

    #[tokio::test]
    async fn timed_out_checks_are_dropped_without_replacing_the_previous_snapshot() {
        let pool = pool().await;
        let calls = Arc::new(AtomicUsize::new(0));
        let dropped = Arc::new(AtomicUsize::new(0));
        let runner: CheckRunner = {
            let (calls, dropped) = (calls.clone(), dropped.clone());
            Arc::new(move || {
                let (calls, dropped) = (calls.clone(), dropped.clone());
                Box::pin(async move {
                    let number = calls.fetch_add(1, Ordering::SeqCst) + 1;
                    if number == 2 {
                        let _guard = Dropped(dropped);
                        std::future::pending::<()>().await;
                    }
                    rows(number)
                })
            })
        };
        let coordinator = HealthCheckCoordinator::from_runner(
            pool.clone(),
            runner,
            CACHE_DURATION,
            Duration::from_millis(20),
        );
        let before = coordinator.run(false).await.unwrap();
        assert!(matches!(
            coordinator.run(true).await,
            Err(HealthRunError::TimedOut)
        ));
        assert_eq!(dropped.load(Ordering::SeqCst), 1);
        assert_eq!(coordinator.run(false).await.unwrap(), before);
        assert_eq!(
            SqlHealthCheckStore::new(pool).load_results().await.unwrap(),
            before.results
        );
        assert_eq!(coordinator.run(true).await.unwrap().results, rows(3));
    }

    #[tokio::test]
    async fn panicked_check_does_not_poison_the_next_run() {
        let calls = Arc::new(AtomicUsize::new(0));
        let runner: CheckRunner = Arc::new(move || {
            let calls = calls.clone();
            Box::pin(async move {
                let number = calls.fetch_add(1, Ordering::SeqCst) + 1;
                assert_ne!(number, 1, "injected check panic");
                rows(number)
            })
        });
        let coordinator = HealthCheckCoordinator::from_runner(
            pool().await,
            runner,
            CACHE_DURATION,
            CHECK_TIMEOUT,
        );
        assert!(matches!(
            coordinator.run(false).await,
            Err(HealthRunError::TaskFailed)
        ));
        assert_eq!(coordinator.run(false).await.unwrap().results, rows(2));
    }

    #[tokio::test]
    async fn host_shutdown_drops_owned_checks_and_rejects_new_runs() {
        let dropped = Arc::new(AtomicUsize::new(0));
        let started = Arc::new(Notify::new());
        let runner: CheckRunner = {
            let (dropped, started) = (dropped.clone(), started.clone());
            Arc::new(move || {
                let (dropped, started) = (dropped.clone(), started.clone());
                Box::pin(async move {
                    let _guard = Dropped(dropped);
                    started.notify_one();
                    std::future::pending().await
                })
            })
        };
        let pool = pool().await;
        let coordinator = HealthCheckCoordinator::from_runner(
            pool.clone(),
            runner,
            CACHE_DURATION,
            CHECK_TIMEOUT,
        );
        let mut first = Box::pin(coordinator.run(false));
        assert!(futures_util::poll!(first.as_mut()).is_pending());
        started.notified().await;
        coordinator.shutdown().await;
        assert!(matches!(first.await, Err(HealthRunError::Stopped)));
        assert!(matches!(
            coordinator.run(true).await,
            Err(HealthRunError::Stopped)
        ));
        assert_eq!(dropped.load(Ordering::SeqCst), 1);
        assert!(SqlHealthCheckStore::new(pool)
            .load_results()
            .await
            .unwrap()
            .is_empty());
    }
}
