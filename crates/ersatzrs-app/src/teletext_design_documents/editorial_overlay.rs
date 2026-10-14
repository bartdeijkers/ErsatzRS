//! Bounded editorial concept storage, independent of native receipt authentication.
//!
//! Original producer strings, requests and source context remain unchanged. This
//! typed subdocument grants no edit permission, save eligibility or publication
//! authority; the host must authenticate its exact immutable export/original pair.

use super::{
    decode_teletext_design_documents, failure, limit, map_only, BoundedWriter,
    EncodedTeletextDesignDocuments, TeletextDesignDocumentBudget, TeletextDesignDocumentError,
    TeletextDesignDocumentFailure,
};
use crate::teletext_profile_documents::TeletextProfileDocument;
use ersatzrs_domain::{
    EntityId, TeletextPageAddress, TeletextPhysicalBaseOverlay, TeletextPhysicalBaseRow,
};
use serde::{
    de::{SeqAccess, Visitor},
    ser::SerializeSeq,
    Deserialize, Deserializer, Serialize, Serializer,
};
use std::{fmt, io::Write};

const OVERLAY_VERSION: u16 = 1;

/// Structural association claims. Only the native host and repository can
/// authenticate these references against exact immutable stored bytes.
#[derive(Clone, PartialEq, Eq)]
pub struct TeletextSavedEditorialProvenance {
    pub export_receipt_id: EntityId,
    pub export_receipt_digest: String,
    pub original_design_id: EntityId,
    pub original_revision: i64,
    pub original_digest: String,
}
impl fmt::Debug for TeletextSavedEditorialProvenance {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("TeletextSavedEditorialProvenance")
            .field("export_receipt_id", &self.export_receipt_id)
            .field("original_design_id", &self.original_design_id)
            .field("original_revision", &self.original_revision)
            .finish_non_exhaustive()
    }
}

/// Immutable physical rows belonging to a captured original, not a checked
/// native rendering or authenticated editor/import capability.
#[derive(Clone, PartialEq, Eq)]
pub struct TeletextSavedEditorialOverlay {
    provenance: TeletextSavedEditorialProvenance,
    pages: Vec<TeletextPhysicalBaseOverlay>,
}
impl TeletextSavedEditorialOverlay {
    /// Check reference shape and distinct exact owners. Original-page membership
    /// and host budgets are checked when attaching/restoring the subdocument.
    /// # Errors
    /// Rejects invalid IDs/digests, empty rows or duplicate page/subcode owners.
    pub fn new(
        provenance: TeletextSavedEditorialProvenance,
        pages: Vec<TeletextPhysicalBaseOverlay>,
    ) -> Result<Self, TeletextDesignDocumentError> {
        validate_provenance(&provenance)?;
        if pages.is_empty() {
            return Err(invalid());
        }
        for (ordinal, page) in pages.iter().enumerate() {
            if pages[..ordinal]
                .iter()
                .any(|previous| previous.owner() == page.owner())
            {
                return Err(invalid());
            }
        }
        Ok(Self { provenance, pages })
    }
    #[must_use]
    pub const fn schema_version(&self) -> u16 {
        OVERLAY_VERSION
    }
    #[must_use]
    pub const fn provenance(&self) -> &TeletextSavedEditorialProvenance {
        &self.provenance
    }
    #[must_use]
    pub fn pages(&self) -> &[TeletextPhysicalBaseOverlay] {
        &self.pages
    }
}
impl fmt::Debug for TeletextSavedEditorialOverlay {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("TeletextSavedEditorialOverlay")
            .field("schema_version", &OVERLAY_VERSION)
            .field("provenance", &self.provenance)
            .field("pages", &self.pages.len())
            .finish_non_exhaustive()
    }
}
fn invalid() -> TeletextDesignDocumentError {
    failure(TeletextDesignDocumentFailure::EditorialOverlay)
}
fn digest(value: &str) -> bool {
    value.len() == 64
        && value
            .bytes()
            .all(|byte| byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte))
}
fn validate_provenance(
    value: &TeletextSavedEditorialProvenance,
) -> Result<(), TeletextDesignDocumentError> {
    if value.export_receipt_id <= 0
        || value.original_design_id <= 0
        || value.original_revision <= 0
        || !digest(&value.export_receipt_digest)
        || !digest(&value.original_digest)
    {
        return Err(invalid());
    }
    Ok(())
}

#[derive(Default)]
pub(super) struct OptionalEditorialOverlay(pub Option<StoredOverlay>);
impl<'de> Deserialize<'de> for OptionalEditorialOverlay {
    fn deserialize<D: Deserializer<'de>>(d: D) -> Result<Self, D::Error> {
        // Only omission selects Default. A present null/sequence is rejected.
        StoredOverlay::deserialize(d).map(|value| Self(Some(value)))
    }
}
impl Serialize for OptionalEditorialOverlay {
    fn serialize<S: Serializer>(&self, s: S) -> Result<S::Ok, S::Error> {
        self.0.serialize(s)
    }
}

macro_rules! closed_map {
    ($name:ident { $($field:ident: $ty:ty),* $(,)? }) => {
        #[derive(Serialize)]
        pub(super) struct $name { $($field: $ty),* }
        impl<'de> Deserialize<'de> for $name {
            fn deserialize<D: Deserializer<'de>>(d: D) -> Result<Self, D::Error> {
                #[derive(Deserialize)] #[serde(deny_unknown_fields)]
                struct Fields { $($field: $ty),* }
                let fields: Fields = map_only(d)?;
                Ok(Self { $($field: fields.$field),* })
            }
        }
    };
}
closed_map!(StoredOverlay {
    schema_version: u16,
    export_receipt_id: EntityId,
    export_receipt_digest: String,
    original_design_id: EntityId,
    original_revision: i64,
    original_digest: String,
    pages: Vec<StoredPage>,
});
closed_map!(StoredPage { owner: super::Entry, rows: Vec<StoredRow> });
closed_map!(StoredRow {
    row: u8,
    cells: Cells
});

struct Cells([u8; 40]);
impl Serialize for Cells {
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        serialize_cells(&self.0, serializer)
    }
}
fn serialize_cells<S: Serializer>(cells: &[u8; 40], serializer: S) -> Result<S::Ok, S::Error> {
    let mut sequence = serializer.serialize_seq(Some(40))?;
    for cell in cells {
        sequence.serialize_element(cell)?;
    }
    sequence.end()
}
impl<'de> Deserialize<'de> for Cells {
    fn deserialize<D: Deserializer<'de>>(d: D) -> Result<Self, D::Error> {
        struct FixedCells;
        impl<'de> Visitor<'de> for FixedCells {
            type Value = Cells;
            fn expecting(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
                f.write_str("exactly forty seven-bit physical cells")
            }
            fn visit_seq<A: SeqAccess<'de>>(self, mut seq: A) -> Result<Cells, A::Error> {
                let mut cells = [0_u8; 40];
                for cell in &mut cells {
                    *cell = seq
                        .next_element()?
                        .ok_or_else(|| serde::de::Error::custom("short physical row"))?;
                }
                if seq.next_element::<serde::de::IgnoredAny>()?.is_some() {
                    return Err(serde::de::Error::custom("long physical row"));
                }
                Ok(Cells(cells))
            }
        }
        d.deserialize_seq(FixedCells)
    }
}

impl StoredOverlay {
    pub(super) fn restore(
        self,
        documents: &[TeletextProfileDocument],
        budget: TeletextDesignDocumentBudget,
    ) -> Result<TeletextSavedEditorialOverlay, TeletextDesignDocumentError> {
        if self.schema_version != OVERLAY_VERSION {
            return Err(invalid());
        }
        let provenance = TeletextSavedEditorialProvenance {
            export_receipt_id: self.export_receipt_id,
            export_receipt_digest: self.export_receipt_digest,
            original_design_id: self.original_design_id,
            original_revision: self.original_revision,
            original_digest: self.original_digest,
        };
        validate_provenance(&provenance)?;
        // Parent streaming preflight already bounded owner/row vectors.
        let pages = self
            .pages
            .into_iter()
            .map(|page| {
                let rows = page
                    .rows
                    .into_iter()
                    .map(|row| {
                        TeletextPhysicalBaseRow::new(row.row, row.cells.0).map_err(|_| invalid())
                    })
                    .collect::<Result<Vec<_>, _>>()?;
                TeletextPhysicalBaseOverlay::new(
                    TeletextPageAddress {
                        page: page.owner.page,
                        subcode: page.owner.subcode,
                    },
                    rows,
                )
                .map_err(|_| invalid())
            })
            .collect::<Result<Vec<_>, _>>()?;
        let value = TeletextSavedEditorialOverlay::new(provenance, pages)?;
        validate_association(&value, documents, budget)?;
        Ok(value)
    }
}

pub(super) fn validate_association(
    overlay: &TeletextSavedEditorialOverlay,
    documents: &[TeletextProfileDocument],
    budget: TeletextDesignDocumentBudget,
) -> Result<(), TeletextDesignDocumentError> {
    limit(
        overlay.pages.len(),
        budget.validation.service.max_records,
        "editorial pages",
    )?;
    let mut items = overlay.pages.len();
    let mut bytes = 0_usize;
    for page in &overlay.pages {
        items = items
            .checked_add(page.rows().len())
            .ok_or_else(|| failure(TeletextDesignDocumentFailure::Storage))?;
        bytes = bytes
            .checked_add(
                page.rows()
                    .len()
                    .checked_mul(40)
                    .ok_or_else(|| failure(TeletextDesignDocumentFailure::Storage))?,
            )
            .ok_or_else(|| failure(TeletextDesignDocumentFailure::Storage))?;
        limit(
            items,
            budget.validation.service.max_structure_items,
            "editorial structure items",
        )?;
        limit(
            bytes,
            budget.validation.service.max_base_content_bytes,
            "editorial content bytes",
        )?;
        let mut owners = documents
            .iter()
            .flat_map(|document| match document {
                TeletextProfileDocument::Base(base) => base.pages.iter(),
                TeletextProfileDocument::Authored(service) => {
                    service.semantic().base().pages.iter()
                }
            })
            .filter(|candidate| candidate.address == page.owner());
        // Ambiguous ownership is rejected, not rebound to the first producer.
        if owners.next().is_none() || owners.next().is_some() {
            return Err(invalid());
        }
    }
    Ok(())
}

#[derive(Serialize)]
struct BorrowedOverlay<'a> {
    schema_version: u16,
    export_receipt_id: EntityId,
    export_receipt_digest: &'a str,
    original_design_id: EntityId,
    original_revision: i64,
    original_digest: &'a str,
    pages: BorrowedPages<'a>,
}
struct BorrowedPages<'a>(&'a [TeletextPhysicalBaseOverlay]);
impl Serialize for BorrowedPages<'_> {
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        let mut sequence = serializer.serialize_seq(Some(self.0.len()))?;
        for page in self.0 {
            sequence.serialize_element(&BorrowedPage {
                owner: super::Entry {
                    page: page.owner().page,
                    subcode: page.owner().subcode,
                },
                rows: BorrowedRows(page.rows()),
            })?;
        }
        sequence.end()
    }
}
#[derive(Serialize)]
struct BorrowedPage<'a> {
    owner: super::Entry,
    rows: BorrowedRows<'a>,
}
struct BorrowedRows<'a>(&'a [TeletextPhysicalBaseRow]);
impl Serialize for BorrowedRows<'_> {
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        let mut sequence = serializer.serialize_seq(Some(self.0.len()))?;
        for row in self.0 {
            sequence.serialize_element(&BorrowedRow(row))?;
        }
        sequence.end()
    }
}
struct BorrowedRow<'a>(&'a TeletextPhysicalBaseRow);
impl Serialize for BorrowedRow<'_> {
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        #[derive(Serialize)]
        struct Fields<'a> {
            row: u8,
            cells: BorrowedCells<'a>,
        }
        Fields {
            row: self.0.row(),
            cells: BorrowedCells(self.0.cells()),
        }
        .serialize(serializer)
    }
}
struct BorrowedCells<'a>(&'a [u8; 40]);
impl Serialize for BorrowedCells<'_> {
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        serialize_cells(self.0, serializer)
    }
}

/// Append concept rows to the exact immutable original storage blob. Existing
/// strings/JSON prefix and `OriginalOnly` policy remain intact. This checks only
/// syntax, bounds and original owner membership; authenticate receipt provenance,
/// enforce edit regions and native preview before any repository save/publication.
/// # Errors
/// Rejects invalid originals, existing overlays, missing/ambiguous owners and
/// limits. No stored revision or running broadcast is modified.
pub fn append_teletext_design_editorial_overlay(
    original_bytes: &[u8],
    overlay: &TeletextSavedEditorialOverlay,
    budget: TeletextDesignDocumentBudget,
) -> Result<EncodedTeletextDesignDocuments, TeletextDesignDocumentError> {
    let original = decode_teletext_design_documents(original_bytes, budget)?;
    if original.editorial_overlay().is_some() {
        return Err(invalid());
    }
    validate_association(overlay, original.documents(), budget)?;
    drop(original);
    let bytes = append_overlay_bytes(original_bytes, overlay, budget)?;
    decode_teletext_design_documents(&bytes, budget)?;
    Ok(EncodedTeletextDesignDocuments { bytes })
}

// Callers first decode the exact role and validate original owner association,
// then repeat their own role decoder over this bounded result. This helper only
// appends bytes; it grants neither role selection nor provenance authentication.
pub(super) fn append_overlay_bytes(
    original_bytes: &[u8],
    overlay: &TeletextSavedEditorialOverlay,
    budget: TeletextDesignDocumentBudget,
) -> Result<Vec<u8>, TeletextDesignDocumentError> {
    let end = original_bytes
        .iter()
        .rposition(|byte| !byte.is_ascii_whitespace())
        .filter(|index| original_bytes[*index] == b'}')
        .ok_or_else(invalid)?;
    let mut writer = BoundedWriter {
        bytes: Vec::new(),
        maximum: budget.max_serialized_bytes,
        exceeded: false,
    };
    write_appended(&mut writer, original_bytes, end, overlay).map_err(|_| {
        if writer.exceeded {
            failure(TeletextDesignDocumentFailure::Budget {
                dimension: "serialized",
                count: budget.max_serialized_bytes.saturating_add(1),
                maximum: budget.max_serialized_bytes,
            })
        } else {
            invalid()
        }
    })?;
    Ok(writer.bytes)
}
fn write_appended(
    writer: &mut BoundedWriter,
    original: &[u8],
    end: usize,
    overlay: &TeletextSavedEditorialOverlay,
) -> Result<(), serde_json::Error> {
    writer
        .write_all(&original[..end])
        .map_err(serde_json::Error::io)?;
    writer
        .write_all(b",\"editorial_overlay\":")
        .map_err(serde_json::Error::io)?;
    let value = &overlay.provenance;
    serde_json::to_writer(
        &mut *writer,
        &BorrowedOverlay {
            schema_version: OVERLAY_VERSION,
            export_receipt_id: value.export_receipt_id,
            export_receipt_digest: &value.export_receipt_digest,
            original_design_id: value.original_design_id,
            original_revision: value.original_revision,
            original_digest: &value.original_digest,
            pages: BorrowedPages(&overlay.pages),
        },
    )?;
    writer
        .write_all(&original[end..])
        .map_err(serde_json::Error::io)
}

#[cfg(test)]
#[path = "editorial_overlay/tests.rs"]
mod tests;
