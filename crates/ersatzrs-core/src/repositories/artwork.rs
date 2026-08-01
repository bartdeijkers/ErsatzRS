use async_trait::async_trait;

use super::Result;

/// Read access to persisted `Artwork` rows, kept minimal and separate from
/// [`super::AdminMediaRepository`] because it is also used by the
/// unauthenticated public `/artwork/*` serving routes, not just admin pages.
#[async_trait]
pub trait ArtworkRepository: Send + Sync {
    /// Whether `path` exactly matches a persisted `Artwork.Path` value.
    ///
    /// This is the trust boundary for serving a local absolute filesystem
    /// path back over an unauthenticated route: only paths the scanner
    /// already registered as artwork may ever be served.
    async fn is_known_path(&self, path: &str) -> Result<bool>;
}
