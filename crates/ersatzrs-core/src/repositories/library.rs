use async_trait::async_trait;
use ersatzrs_domain::{EntityId, Library, LibraryMediaKind, MediaSourceKind};

use super::Result;

/// Persisted library state required to admit scan work.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct LibraryScanTarget {
    /// Persisted library identity.
    pub library_id: EntityId,
    /// Provider resolved from the persisted library subtype.
    pub provider: MediaSourceKind,
    /// Persisted media kind used to validate targeted show scans.
    pub media_kind: LibraryMediaKind,
    /// Whether provider item synchronization is enabled for this library.
    pub should_sync_items: bool,
}

#[async_trait]
pub trait LibraryRepository: Send + Sync {
    async fn get_with_paths(&self, id: EntityId) -> Result<Library>;
    async fn list_for_source(&self, source_id: EntityId) -> Result<Vec<Library>>;
    async fn upsert(&self, library: &Library) -> Result<()>;

    /// Load authoritative persisted state used by scan admission.
    async fn get_scan_target(&self, id: EntityId) -> Result<LibraryScanTarget>;

    /// Return whether `show_id` belongs to `library_id`.
    async fn show_belongs_to_library(
        &self,
        library_id: EntityId,
        show_id: EntityId,
    ) -> Result<bool>;
}
