use async_trait::async_trait;
use ersatzrs_domain::{ArtworkKind, EntityId};

use super::Result;

/// Persisted identity and server-owned location of one artwork image.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ArtworkReference {
    pub id: EntityId,
    pub kind: ArtworkKind,
    pub path: String,
}

/// Read access to persisted `Artwork` rows, kept minimal and separate from
/// [`super::AdminMediaRepository`] because it is also used by the
/// unauthenticated public `/artwork/*` serving routes, not just admin pages.
#[async_trait]
pub trait ArtworkRepository: Send + Sync {
    /// Resolve an image by persisted identity. HTTP paths never choose the
    /// provider request path or its credentials.
    async fn find_by_id(&self, id: EntityId) -> Result<Option<ArtworkReference>>;

    /// Whether `path` exactly matches a persisted `Artwork.Path` value.
    ///
    /// This is the trust boundary for serving a local absolute filesystem
    /// path back over an unauthenticated route: only paths the scanner
    /// already registered as artwork may ever be served.
    async fn is_known_path(&self, path: &str) -> Result<bool>;
}
