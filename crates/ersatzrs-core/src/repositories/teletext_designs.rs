//! Narrow immutable design storage port for ADR 0123.

use std::fmt;

use async_trait::async_trait;
use ersatzrs_domain::{
    EntityId, TeletextDesignExportMetadata, TeletextDesignExportReceipt, TeletextDesignHead,
    TeletextDesignRevision, TELETEXT_DESIGN_EXPORT_RECEIPT_VERSION,
};

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

/// Exact opaque export envelope bound to an immutable revision and live head.
///
/// Storage checks provenance and computes the receipt digest. The application
/// must separately validate semantic mappings, edit policy and native output;
/// bytes supplied here never grant editing, package or publication authority.
#[derive(Clone, PartialEq, Eq)]
pub struct TeletextDesignExportSave {
    /// Existing durable design identity.
    pub design_id: EntityId,
    /// Deliberately selected immutable revision, not necessarily the current head.
    pub revision: i64,
    /// Exact original document digest expected by the caller.
    pub design_digest: String,
    /// Independently observed current live head for compare-and-swap.
    pub expected_design_head: i64,
    /// Opaque export-receipt envelope version.
    pub receipt_version: u16,
    /// Exact bounded receipt bytes, excluded from diagnostics.
    pub receipt: Vec<u8>,
}

impl TeletextDesignExportSave {
    /// Check storage shape and trusted byte bounds without reads or native work.
    ///
    /// This preflight grants no semantic, editing or publication authority. Both
    /// limits must be positive and representable by `SQLite` and Rust allocation
    /// lengths, including on supported 32-bit targets.
    #[must_use]
    pub fn is_valid(&self, max_receipt_bytes: usize, max_document_bytes: usize) -> bool {
        self.design_id > 0
            && self.revision > 0
            && self.expected_design_head >= self.revision
            && self.design_digest.len() == 64
            && self
                .design_digest
                .bytes()
                .all(|byte| byte.is_ascii_digit() || matches!(byte, b'a'..=b'f'))
            && self.receipt_version == TELETEXT_DESIGN_EXPORT_RECEIPT_VERSION
            && !self.receipt.is_empty()
            && self.receipt.len() <= max_receipt_bytes
            && [max_receipt_bytes, max_document_bytes]
                .into_iter()
                .all(|limit| {
                    limit > 0 && isize::try_from(limit).is_ok() && i64::try_from(limit).is_ok()
                })
    }
}

impl fmt::Debug for TeletextDesignExportSave {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("TeletextDesignExportSave")
            .field("design_id", &self.design_id)
            .field("revision", &self.revision)
            .field("expected_design_head", &self.expected_design_head)
            .field("receipt_version", &self.receipt_version)
            .field("receipt_bytes", &self.receipt.len())
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
    /// A monotonic signed storage counter cannot advance.
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
/// this port has no cascading ownership or active flag. Byte
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

    /// Persist an immutable opaque receipt without changing the design head.
    /// One immediate transaction checks a live exact head, the selected original
    /// revision's bounded bytes/integrity and its exact digest. Historical
    /// revision selection is allowed; missing targets reject with `MissingDesign`,
    /// stale head or digest with `Conflict`. Returns metadata, never receipt bytes.
    async fn create_export(
        &self,
        save: &TeletextDesignExportSave,
        max_receipt_bytes: usize,
        max_document_bytes: usize,
    ) -> Result<TeletextDesignMutation<TeletextDesignExportMetadata>>;

    /// Read retained exact receipt bytes, including after a design tombstone.
    /// Bound stored types/lengths before allocating, then verify receipt version
    /// and digest plus the owning original revision's version, bytes and digest.
    /// Only an absent receipt identity returns None; corruption is an error.
    async fn export(
        &self,
        id: EntityId,
        max_receipt_bytes: usize,
        max_document_bytes: usize,
    ) -> Result<Option<TeletextDesignExportReceipt>>;
}

#[cfg(test)]
mod tests {
    use super::{TeletextDesignExportSave, TeletextDesignMutation, TeletextDesignSave};

    #[test]
    fn export_preflight_checks_exact_storage_shape_without_exposing_bytes() {
        let valid = TeletextDesignExportSave {
            design_id: 7,
            revision: 2,
            design_digest: "a".repeat(64),
            expected_design_head: 4,
            receipt_version: 1,
            receipt: b"private export payload".to_vec(),
        };
        assert!(valid.is_valid(valid.receipt.len(), 1));
        for (receipt, document) in [
            (0, 1),
            (valid.receipt.len() - 1, 1),
            (1024, 0),
            (usize::MAX, 1),
            (1024, usize::MAX),
        ] {
            assert!(!valid.is_valid(receipt, document));
        }
        let mut invalid = vec![valid.clone(); 7];
        invalid[0].design_id = 0;
        invalid[1].revision = 0;
        invalid[2].expected_design_head = 1;
        invalid[3].design_digest = "A".repeat(64);
        invalid[4].design_digest = "a".repeat(63);
        invalid[5].receipt_version = 2;
        invalid[6].receipt.clear();
        for input in invalid {
            assert!(!input.is_valid(1024, 1024));
        }
        let diagnostic = format!("{valid:?}");
        assert!(!diagnostic.contains("private"));
        assert!(!diagnostic.contains(&valid.design_digest));
        assert!(diagnostic.contains("expected_design_head: 4"));
    }

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
