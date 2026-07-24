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

/// Cheap persisted revision used to invalidate admin media views.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct MediaStateRevision {
    pub latest_library_scan: Option<String>,
    pub latest_path_scan: Option<String>,
    pub unavailable_count: u64,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct AdminLibraryRow {
    pub id: EntityId,
    pub name: String,
    pub kind: String,
    pub media_kind: i64,
    pub source_name: Option<String>,
    pub media_source_id: EntityId,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct AdminExternalCollectionRow {
    pub media_source_id: EntityId,
    pub kind: String,
    pub server_name: Option<String>,
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

    /// Load the persisted media-state revision without exposing storage details.
    async fn media_state_revision_admin(&self) -> Result<MediaStateRevision>;

    async fn list_admin(&self) -> Result<Vec<AdminLibraryRow>> {
        Err(super::RepositoryError::unavailable(
            "list admin libraries",
            "repository adapter does not implement admin library reads",
        ))
    }

    async fn list_external_collections_admin(&self) -> Result<Vec<AdminExternalCollectionRow>> {
        Err(super::RepositoryError::unavailable(
            "list external collections",
            "repository adapter does not implement external collection reads",
        ))
    }
}
