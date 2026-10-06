use async_trait::async_trait;
use ersatzrs_app::artwork_cleanup::{ArtworkCleanupQueue, ArtworkCleanupRequest};
use ersatzrs_infra::artwork_cache_maintenance::ArtworkCacheMaintenance;
use tokio::sync::mpsc;
use tokio_util::sync::CancellationToken;

/// Bounded admission with backpressure, rather than dropping maintenance work.
pub struct TokioArtworkCleanupQueue {
    sender: mpsc::Sender<ArtworkCleanupRequest>,
}

impl TokioArtworkCleanupQueue {
    pub fn channel() -> (Self, mpsc::Receiver<ArtworkCleanupRequest>) {
        let (sender, receiver) = mpsc::channel(16);
        (Self { sender }, receiver)
    }
}

#[async_trait]
impl ArtworkCleanupQueue for TokioArtworkCleanupQueue {
    async fn enqueue(&self, request: ArtworkCleanupRequest) -> ersatzrs_app::Result<()> {
        self.sender
            .send(request)
            .await
            .map_err(|_| ersatzrs_app::ApplicationError::Unavailable {
                operation: "queue artwork cleanup",
                message: "artwork cleanup worker is closed".to_owned(),
            })
    }
}

pub async fn run_artwork_cleanup_worker(
    maintenance: ArtworkCacheMaintenance,
    mut receiver: mpsc::Receiver<ArtworkCleanupRequest>,
    shutdown: CancellationToken,
) {
    loop {
        let request = tokio::select! {
            biased;
            () = shutdown.cancelled() => break,
            request = receiver.recv() => match request {
                Some(request) => request,
                None => break,
            },
        };
        tokio::select! {
            biased;
            () = shutdown.cancelled() => break,
            result = maintenance.run(request.limit) => {
                match result {
                    Ok(report) => tracing::debug!(
                        ?report,
                        "completed manual orphaned artwork cleanup"
                    ),
                    Err(error) => tracing::warn!(%error, "manual orphaned artwork cleanup failed"),
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::Duration;

    #[tokio::test]
    async fn artwork_cleanup_admission_backpressures_and_cancelled_wait_does_not_enqueue() {
        let (queue, mut receiver) = TokioArtworkCleanupQueue::channel();
        for limit in 0..16 {
            queue
                .enqueue(ArtworkCleanupRequest { limit })
                .await
                .unwrap();
        }
        {
            let pending = queue.enqueue(ArtworkCleanupRequest { limit: 999 });
            tokio::pin!(pending);
            assert!(futures_util::poll!(&mut pending).is_pending());
        }
        for limit in 0..16 {
            assert_eq!(receiver.recv().await.unwrap().limit, limit);
        }
        assert!(receiver.try_recv().is_err());
        drop(receiver);
        assert!(queue
            .enqueue(ArtworkCleanupRequest { limit: 1 })
            .await
            .is_err());
    }

    #[tokio::test]
    async fn artwork_cleanup_shutdown_interrupts_work_waiting_for_database() {
        let pool = sqlx::sqlite::SqlitePoolOptions::new()
            .max_connections(1)
            .connect("sqlite::memory:")
            .await
            .unwrap();
        let _held_connection = pool.acquire().await.unwrap();
        let (queue, receiver) = TokioArtworkCleanupQueue::channel();
        queue
            .enqueue(ArtworkCleanupRequest { limit: 1 })
            .await
            .unwrap();
        let shutdown = CancellationToken::new();
        let root = tempfile::tempdir().unwrap();
        let maintenance = ArtworkCacheMaintenance::new(pool.clone(), root.path().to_owned());
        let worker = run_artwork_cleanup_worker(maintenance, receiver, shutdown.clone());
        tokio::pin!(worker);
        assert!(futures_util::poll!(&mut worker).is_pending());
        shutdown.cancel();
        tokio::time::timeout(Duration::from_secs(1), worker)
            .await
            .unwrap();
        assert!(queue
            .enqueue(ArtworkCleanupRequest { limit: 1 })
            .await
            .is_err());
    }

    #[tokio::test]
    async fn artwork_cleanup_shutdown_closes_admission_without_touching_database() {
        let (queue, receiver) = TokioArtworkCleanupQueue::channel();
        queue
            .enqueue(ArtworkCleanupRequest { limit: 1 })
            .await
            .unwrap();
        let shutdown = CancellationToken::new();
        shutdown.cancel();
        let pool = ersatzrs_infra::db::lazy_sqlite_pool("sqlite::memory:").unwrap();
        let root = tempfile::tempdir().unwrap();
        let maintenance = ArtworkCacheMaintenance::new(pool, root.path().to_owned());
        tokio::time::timeout(
            Duration::from_secs(1),
            run_artwork_cleanup_worker(maintenance, receiver, shutdown),
        )
        .await
        .unwrap();
        assert!(queue
            .enqueue(ArtworkCleanupRequest { limit: 1 })
            .await
            .is_err());
    }
}
