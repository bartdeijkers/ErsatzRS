//! Retained opaque export receipts for immutable design revisions (ADR 0123).
//!
//! Receipt storage preserves exact bytes and provenance, not edit-policy,
//! package, native-confirmation or publication authority.

use std::fmt;

use crate::EntityId;

/// Current opaque export-receipt envelope version, independent of TTI syntax.
pub const TELETEXT_DESIGN_EXPORT_RECEIPT_VERSION: u16 = 1;

/// Repository-owned immutable receipt identity and checked storage provenance.
#[derive(Clone, PartialEq, Eq)]
pub struct TeletextDesignExportMetadata {
    /// Durable repository-assigned export identity.
    pub id: EntityId,
    /// Owning design identity, independent of channel/profile/package lifetime.
    pub design_id: EntityId,
    /// Exact exported immutable revision, possibly older than the live head.
    pub revision: i64,
    /// Verified SHA-256 of the original immutable design document.
    pub design_digest: String,
    /// Exact live design head observed when this receipt was persisted.
    pub expected_design_head: i64,
    /// Supported opaque receipt envelope version.
    pub receipt_version: u16,
    /// Repository-computed SHA-256 of the exact opaque receipt bytes.
    pub digest: String,
}

impl fmt::Debug for TeletextDesignExportMetadata {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("TeletextDesignExportMetadata")
            .field("id", &self.id)
            .field("design_id", &self.design_id)
            .field("revision", &self.revision)
            .field("expected_design_head", &self.expected_design_head)
            .field("receipt_version", &self.receipt_version)
            .finish_non_exhaustive()
    }
}

/// Exact bounded receipt bytes returned only by an explicit storage read.
///
/// Both this receipt's digest and its original revision's stored integrity are
/// checked by the repository. Semantic receipt/edit-policy validation remains
/// an application responsibility; this read model grants no editing authority.
#[derive(Clone, PartialEq, Eq)]
pub struct TeletextDesignExportReceipt {
    /// Immutable metadata, including both integrity digests.
    pub metadata: TeletextDesignExportMetadata,
    /// Exact opaque envelope bytes, excluded from diagnostics.
    pub receipt: Vec<u8>,
}

impl fmt::Debug for TeletextDesignExportReceipt {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("TeletextDesignExportReceipt")
            .field("metadata", &self.metadata)
            .field("receipt_bytes", &self.receipt.len())
            .finish_non_exhaustive()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn export_receipt_diagnostics_exclude_bytes_and_digest_values() {
        let receipt = TeletextDesignExportReceipt {
            metadata: TeletextDesignExportMetadata {
                id: 3,
                design_id: 7,
                revision: 2,
                design_digest: "private original digest".into(),
                expected_design_head: 4,
                receipt_version: 1,
                digest: "private receipt digest".into(),
            },
            receipt: b"private receipt content".to_vec(),
        };
        let diagnostic = format!("{receipt:?}");
        assert!(!diagnostic.contains("private"));
        assert!(diagnostic.contains("expected_design_head: 4"));
        assert!(diagnostic.contains("receipt_bytes: 23"));
    }
}
