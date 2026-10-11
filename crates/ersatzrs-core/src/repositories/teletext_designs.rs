//! Narrow immutable design storage port for ADR 0123.

use std::fmt;

use async_trait::async_trait;
use ersatzrs_domain::{EntityId, TeletextDesignHead, TeletextDesignRevision};

use super::Result;

/// Complete new document; `existing=None` creates a new durable design.
///
/// Existing writes require the exact current head revision. This opaque storage
/// request grants no checked-page, invocation or publication authority. Names
/// and documents are excluded from diagnostics, and save results do not copy
/// the full document back to the caller.
#[derive(Clone, PartialEq, Eq)]
pub struct TeletextDesignSave {
    /// User-authored name stored on the head.
    pub name: String,
    /// Design envelope version, independent of contained add-on wire schemas.
    pub document_version: u16,
    /// Exact original-document envelope bytes.
    pub document: Vec<u8>,
    /// Current design identity and expected head revision, or create.
    pub existing: Option<(EntityId, i64)>,
}

impl fmt::Debug for TeletextDesignSave {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("TeletextDesignSave")
            .field("name_bytes", &self.name.len())
            .field("document_version", &self.document_version)
            .field("document_bytes", &self.document.len())
            .field("existing", &self.existing)
            .finish_non_exhaustive()
    }
}

/// Static mutation rejection without authored or configuration content.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum TeletextDesignRejection {
    /// Submitted metadata or document does not satisfy storage bounds/version.
    InvalidValues,
    /// The expected durable design does not exist.
    MissingDesign,
    /// A tombstoned design cannot be edited or restored through this port.
    DeletedDesign,
    /// The current head does not match the expected revision.
    Conflict,
    /// The monotonic signed revision counter cannot advance.
    RevisionLimit,
}

/// A rejected mutation creates no immutable revision and changes no head.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum TeletextDesignMutation<T> {
    /// Atomically persisted result.
    Saved(T),
    /// Typed rejection with no partial write.
    Rejected(TeletextDesignRejection),
}

/// Atomic design head/CAS and immutable revision persistence.
///
/// Designs remain independently readable after package/profile/channel deletion;
/// this port has no cascading ownership, export receipts or active flag. Byte
/// and listing bounds are trusted caller policy, never document-controlled.
/// Repository failures must be redacted, and invalid/corrupt stored data must
/// return an error rather than masquerade as an absent record.
#[async_trait]
pub trait TeletextDesignRepository: Send + Sync {
    /// Read a head including a deletion tombstone; absent identity returns None.
    async fn head(&self, id: EntityId) -> Result<Option<TeletextDesignHead>>;

    /// Read exact immutable bytes, including revisions of tombstoned designs.
    ///
    /// Bound the stored length before allocating its body. Unsupported envelope
    /// versions, digest mismatch, corrupt metadata and oversized documents are
    /// repository errors; only an absent identity/revision returns None.
    async fn revision(
        &self,
        id: EntityId,
        revision: i64,
        max_document_bytes: usize,
    ) -> Result<Option<TeletextDesignRevision>>;

    /// Bounded ascending identity keyset, including tombstones.
    /// Validate count/storage arithmetic before allocating returned records.
    async fn heads(
        &self,
        after_id: Option<EntityId>,
        limit: usize,
    ) -> Result<Vec<TeletextDesignHead>>;

    /// Save exact bytes and their digest with the head in one write transaction.
    ///
    /// Creation starts at revision 1. An existing write requires a live exact
    /// expected head, advances once, and never changes any previous revision.
    /// Unsupported version/bounds reject without writes; counter overflow also
    /// rejects. Returns only the head, without another document allocation.
    async fn save(
        &self,
        save: &TeletextDesignSave,
        max_document_bytes: usize,
    ) -> Result<TeletextDesignMutation<TeletextDesignHead>>;

    /// Mark a live exact expected head deleted, retaining its revision/bytes.
    /// No new document revision is created and no dependent model is activated.
    async fn tombstone(
        &self,
        id: EntityId,
        expected_revision: i64,
    ) -> Result<TeletextDesignMutation<TeletextDesignHead>>;
}

#[cfg(test)]
mod tests {
    use super::{TeletextDesignMutation, TeletextDesignSave};

    #[test]
    fn save_request_and_saved_head_diagnostics_redact_authored_values() {
        let save = TeletextDesignSave {
            name: "private design label".into(),
            document_version: 1,
            document: b"private authored response".to_vec(),
            existing: Some((7, 2)),
        };
        let head = ersatzrs_domain::TeletextDesignHead {
            id: 7,
            name: save.name.clone(),
            revision: 3,
            deleted: false,
        };
        let saved = TeletextDesignMutation::Saved(head);
        let diagnostic = format!("{save:?} {saved:?}");
        assert!(!diagnostic.contains("private"));
        assert!(!diagnostic.contains("response"));
        assert!(diagnostic.contains("existing: Some((7, 2))"));
        assert!(diagnostic.contains("revision: 3"));
    }
}
