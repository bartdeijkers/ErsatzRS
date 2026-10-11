//! Durable design identities and opaque immutable revision documents (ADR 0123).
//!
//! These storage read models carry no checked-page, invocation, export-receipt,
//! lifecycle or publication authority. Loading a revision does not validate its
//! authored pages or reactivate any deleted package, profile or channel.

use std::fmt;

use crate::EntityId;

/// Current durable original-document envelope version, separate from page schemas.
pub const TELETEXT_DESIGN_DOCUMENT_VERSION: u16 = 1;

/// Design storage names reuse the existing profile-name allocation convention.
/// This bounds stored names and places no limit on authored page content.
pub const MAX_TELETEXT_DESIGN_NAME_BYTES: usize = super::MAX_TELETEXT_PROFILE_NAME_BYTES;

/// Current named design head, including retained deletion tombstones.
#[derive(Clone, PartialEq, Eq)]
pub struct TeletextDesignHead {
    /// Repository-assigned durable design identity.
    pub id: EntityId,
    /// User-authored display name; excluded from diagnostic formatting.
    pub name: String,
    /// Latest immutable document revision; deletion does not create a revision.
    pub revision: i64,
    /// A deleted design remains readable but cannot receive new revisions.
    pub deleted: bool,
}

impl fmt::Debug for TeletextDesignHead {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("TeletextDesignHead")
            .field("id", &self.id)
            .field("name_bytes", &self.name.len())
            .field("revision", &self.revision)
            .field("deleted", &self.deleted)
            .finish_non_exhaustive()
    }
}

/// Exact immutable stored document and repository-owned integrity digest.
///
/// The version describes the design document envelope, independently of each
/// contained add-on wire schema. The repository verifies bounds, supported
/// envelope version and digest before returning this read model. Application
/// validation remains necessary before producing any checked authored model.
#[derive(Clone, PartialEq, Eq)]
pub struct TeletextDesignRevision {
    /// Owning durable design identity, independent of package/profile lifetime.
    pub design_id: EntityId,
    /// Monotonically increasing revision within this design.
    pub revision: i64,
    /// Version of the opaque, lossless original-document envelope.
    pub document_version: u16,
    /// Repository-computed lowercase SHA-256 digest of the exact document bytes.
    pub digest: String,
    /// Exact bounded document bytes; excluded from diagnostic formatting.
    pub document: Vec<u8>,
}

impl fmt::Debug for TeletextDesignRevision {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("TeletextDesignRevision")
            .field("design_id", &self.design_id)
            .field("revision", &self.revision)
            .field("document_version", &self.document_version)
            .field("document_bytes", &self.document.len())
            .finish_non_exhaustive()
    }
}

#[cfg(test)]
mod tests {
    use super::{TeletextDesignHead, TeletextDesignRevision};

    #[test]
    fn design_storage_diagnostics_exclude_authored_content() {
        let head = TeletextDesignHead {
            id: 7,
            name: "private design label".into(),
            revision: 2,
            deleted: true,
        };
        let revision = TeletextDesignRevision {
            design_id: 7,
            revision: 2,
            document_version: 1,
            digest: "unvalidated private digest value".into(),
            document: b"private authored response".to_vec(),
        };
        let diagnostic = format!("{head:?} {revision:?}");
        assert!(!diagnostic.contains("private"));
        assert!(!diagnostic.contains("response"));
        assert!(diagnostic.contains("design_id: 7"));
        assert!(diagnostic.contains("revision: 2"));
        assert!(diagnostic.contains("deleted: true"));
        assert!(diagnostic.contains("document_bytes: 25"));
    }
}
