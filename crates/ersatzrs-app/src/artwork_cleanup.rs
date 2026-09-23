//! Admission of bounded, asynchronous orphaned-artwork maintenance.
use async_trait::async_trait;

/// A cleanup limit applies independently to orphaned actors and artwork.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ArtworkCleanupRequest {
    /// Per-kind limit checked between cleanup batches; nonpositive means no-op.
    pub limit: i32,
}

/// Host-owned background admission; completion is deliberately asynchronous.
#[async_trait]
pub trait ArtworkCleanupQueue: Send + Sync {
    /// Admit work, waiting for queue capacity or returning an unavailable error.
    async fn enqueue(&self, request: ArtworkCleanupRequest) -> crate::Result<()>;
}

/// Queue the upstream maintenance operation, including non-positive no-op limits.
pub async fn request_artwork_cleanup(
    queue: &dyn ArtworkCleanupQueue,
    limit: i32,
) -> crate::Result<()> {
    queue.enqueue(ArtworkCleanupRequest { limit }).await
}
