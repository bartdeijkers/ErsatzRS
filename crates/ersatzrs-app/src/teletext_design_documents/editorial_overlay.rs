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
    EntityId, TeletextColumnOperation, TeletextDrcsBitmap, TeletextDrcsBitmapOverlay,
    TeletextDrcsGlyphSlot, TeletextEnhancementCommand, TeletextLocalObjectOperandOverlay,
    TeletextLocalProgramOperandOverlay, TeletextPageAddress, TeletextPhysicalBaseOverlay,
    TeletextPhysicalBaseRow, TeletextResourceIdentity,
};
use serde::{
    de::{SeqAccess, Visitor},
    ser::SerializeSeq,
    Deserialize, Deserializer, Serialize, Serializer,
};
use std::{collections::BTreeSet, fmt, io::Write};

const OVERLAY_VERSION: u16 = 1;
const OPERAND_OVERLAY_VERSION: u16 = 2;
const DRCS_OVERLAY_VERSION: u16 = 3;
const GLYPH_OVERLAY_VERSION: u16 = 4;
const OBJECT_ATTRIBUTE_OVERLAY_VERSION: u16 = 5;
const EXTENDED_GLYPH_OVERLAY_VERSION: u16 = 6;

#[path = "editorial_overlay/object_operands.rs"]
pub(super) mod object_operands;
use object_operands::{BorrowedObjectOperands, OptionalStoredObjectOperands};

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

/// Immutable rows, local operands and DRCS bitmaps belonging to a captured original.
/// This is not a checked native rendering or authenticated editor capability.
#[derive(Clone, PartialEq, Eq)]
pub struct TeletextSavedEditorialOverlay {
    provenance: TeletextSavedEditorialProvenance,
    pages: Vec<TeletextPhysicalBaseOverlay>,
    local_program_operands: Vec<TeletextLocalProgramOperandOverlay>,
    drcs_bitmap_overlays: Vec<TeletextDrcsBitmapOverlay>,
    local_object_operands: Vec<TeletextLocalObjectOperandOverlay>,
    glyph_schema: bool,
    object_attribute_schema: bool,
    extended_glyph_schema: bool,
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
        if pages.is_empty() {
            return Err(invalid());
        }
        Self::new_parts(provenance, pages, Vec::new(), Vec::new(), Vec::new())
    }

    /// Capture version 2 intent, including operand-only concepts with no rows.
    ///
    /// Every operand must match a unique original authored document when this is
    /// attached or restored. Character sizing and command structure are immutable;
    /// all affected protected display effects still require host validation.
    /// # Errors
    /// Rejects invalid references, empty operand input, glyph intent and repeated targets.
    /// Page-local G0/G1 intent requires the explicit version4 constructor.
    pub fn new_with_local_program_operands(
        provenance: TeletextSavedEditorialProvenance,
        pages: Vec<TeletextPhysicalBaseOverlay>,
        local_program_operands: Vec<TeletextLocalProgramOperandOverlay>,
    ) -> Result<Self, TeletextDesignDocumentError> {
        if local_program_operands.is_empty()
            || local_program_operands
                .iter()
                .any(|operand| object_operands::glyph(operand.expected()))
        {
            return Err(invalid());
        }
        Self::new_parts(
            provenance,
            pages,
            local_program_operands,
            Vec::new(),
            Vec::new(),
        )
    }

    /// Capture version 3 intent for exact-original DRCS glyph replacements.
    ///
    /// Resource identity, logical S1 and defined slot remain immutable. Restoring
    /// authenticates original bitmaps; permissions and protected shared effects
    /// still require native validation before saving or publishing.
    /// # Errors
    /// Rejects invalid references, empty bitmap input, glyph intent and repeated targets.
    /// Page-local G0/G1 intent requires the explicit version4 constructor.
    pub fn new_with_drcs_bitmap_overlays(
        provenance: TeletextSavedEditorialProvenance,
        pages: Vec<TeletextPhysicalBaseOverlay>,
        local_program_operands: Vec<TeletextLocalProgramOperandOverlay>,
        drcs_bitmap_overlays: Vec<TeletextDrcsBitmapOverlay>,
    ) -> Result<Self, TeletextDesignDocumentError> {
        if drcs_bitmap_overlays.is_empty()
            || local_program_operands
                .iter()
                .any(|operand| object_operands::glyph(operand.expected()))
        {
            return Err(invalid());
        }
        Self::new_parts(
            provenance,
            pages,
            local_program_operands,
            drcs_bitmap_overlays,
            Vec::new(),
        )
    }

    /// Capture version 4 glyph intent in existing page-local or local-object commands.
    ///
    /// An empty object vector is permitted only when page-local operands include
    /// a G0/G1 glyph change. Rows, legacy attributes and bitmaps may accompany the
    /// new glyph intent, without granting edit or publication authority.
    /// # Errors
    /// Rejects invalid references, repeated exact targets, or input containing
    /// neither a local-object operand nor a page-local glyph operand. Every
    /// local-object operand must remain in the same G0 or G1 family; object
    /// attributes require the explicit version5 constructor.
    pub fn new_with_local_object_operands(
        provenance: TeletextSavedEditorialProvenance,
        pages: Vec<TeletextPhysicalBaseOverlay>,
        local_program_operands: Vec<TeletextLocalProgramOperandOverlay>,
        local_object_operands: Vec<TeletextLocalObjectOperandOverlay>,
        drcs_bitmap_overlays: Vec<TeletextDrcsBitmapOverlay>,
    ) -> Result<Self, TeletextDesignDocumentError> {
        if local_object_operands
            .iter()
            .any(|operand| !same_glyph_family(operand.expected(), operand.replacement()))
            || (local_object_operands.is_empty()
                && !local_program_operands
                    .iter()
                    .any(|operand| object_operands::glyph(operand.expected())))
        {
            return Err(invalid());
        }
        Self::new_parts(
            provenance,
            pages,
            local_program_operands,
            drcs_bitmap_overlays,
            local_object_operands,
        )
    }

    /// Capture version 5 existing local-object foreground/background/attribute intent.
    ///
    /// At least one local-object attribute must change within its original family.
    /// Character size, original command identity and ordering remain immutable.
    /// Glyph, page-local, row and bitmap intent may accompany that attribute.
    /// This private concept version is independent of the editor carrier version
    /// and grants no edit permission or publication authority.
    /// # Errors
    /// Rejects missing object attribute intent, unsupported families, invalid
    /// references or repeated exact original targets.
    pub fn new_with_local_object_attributes(
        provenance: TeletextSavedEditorialProvenance,
        pages: Vec<TeletextPhysicalBaseOverlay>,
        local_program_operands: Vec<TeletextLocalProgramOperandOverlay>,
        local_object_operands: Vec<TeletextLocalObjectOperandOverlay>,
        drcs_bitmap_overlays: Vec<TeletextDrcsBitmapOverlay>,
    ) -> Result<Self, TeletextDesignDocumentError> {
        if !local_object_operands
            .iter()
            .any(|operand| same_attribute_family(operand.expected(), operand.replacement()))
        {
            return Err(invalid());
        }
        Self::new_parts(
            provenance,
            pages,
            local_program_operands,
            drcs_bitmap_overlays,
            local_object_operands,
        )
    }

    /// Capture version 6 G2 or G0-with-diacritic operands in original local commands.
    ///
    /// At least one page-local or local-object operand must change within one of
    /// these two families. G0-with-diacritic may change its code and/or mark index;
    /// charset, source identity, ordering, invocation and character size stay fixed.
    /// Existing glyph/attribute, row and bitmap intents may accompany the change.
    /// This grants no authoring-region permission or publication authority.
    /// # Errors
    /// Rejects missing extended-glyph intent, unsupported or crossed families,
    /// invalid provenance and repeated exact original targets.
    pub fn new_with_extended_glyph_operands(
        provenance: TeletextSavedEditorialProvenance,
        pages: Vec<TeletextPhysicalBaseOverlay>,
        local_program_operands: Vec<TeletextLocalProgramOperandOverlay>,
        local_object_operands: Vec<TeletextLocalObjectOperandOverlay>,
        drcs_bitmap_overlays: Vec<TeletextDrcsBitmapOverlay>,
    ) -> Result<Self, TeletextDesignDocumentError> {
        if !local_program_operands
            .iter()
            .any(|operand| same_extended_glyph_family(operand.expected(), operand.replacement()))
            && !local_object_operands.iter().any(|operand| {
                same_extended_glyph_family(operand.expected(), operand.replacement())
            })
        {
            return Err(invalid());
        }
        Self::new_parts_for_families(
            provenance,
            pages,
            local_program_operands,
            drcs_bitmap_overlays,
            local_object_operands,
            true,
        )
    }

    fn new_parts(
        provenance: TeletextSavedEditorialProvenance,
        pages: Vec<TeletextPhysicalBaseOverlay>,
        local_program_operands: Vec<TeletextLocalProgramOperandOverlay>,
        drcs_bitmap_overlays: Vec<TeletextDrcsBitmapOverlay>,
        local_object_operands: Vec<TeletextLocalObjectOperandOverlay>,
    ) -> Result<Self, TeletextDesignDocumentError> {
        Self::new_parts_for_families(
            provenance,
            pages,
            local_program_operands,
            drcs_bitmap_overlays,
            local_object_operands,
            false,
        )
    }

    fn new_parts_for_families(
        provenance: TeletextSavedEditorialProvenance,
        pages: Vec<TeletextPhysicalBaseOverlay>,
        local_program_operands: Vec<TeletextLocalProgramOperandOverlay>,
        drcs_bitmap_overlays: Vec<TeletextDrcsBitmapOverlay>,
        local_object_operands: Vec<TeletextLocalObjectOperandOverlay>,
        allow_extended: bool,
    ) -> Result<Self, TeletextDesignDocumentError> {
        validate_provenance(&provenance)?;
        let supported = |expected, replacement| {
            supported_family(expected, replacement)
                || (allow_extended && same_extended_glyph_family(expected, replacement))
        };
        if local_program_operands
            .iter()
            .any(|operand| !supported(operand.expected(), operand.replacement()))
            || local_object_operands
                .iter()
                .any(|operand| !supported(operand.expected(), operand.replacement()))
        {
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
        let mut targets = BTreeSet::new();
        for operand in &local_program_operands {
            if !targets.insert((operand.owner(), operand.command())) {
                return Err(invalid());
            }
        }
        let mut bitmap_targets = BTreeSet::new();
        for bitmap in &drcs_bitmap_overlays {
            if !bitmap_targets.insert((bitmap.identity(), bitmap.slot())) {
                return Err(invalid());
            }
        }
        object_operands::distinct(&local_object_operands)?;
        let extended_glyph_schema = local_program_operands
            .iter()
            .any(|operand| same_extended_glyph_family(operand.expected(), operand.replacement()))
            || local_object_operands.iter().any(|operand| {
                same_extended_glyph_family(operand.expected(), operand.replacement())
            });
        let object_attribute_schema = local_object_operands
            .iter()
            .any(|operand| same_attribute_family(operand.expected(), operand.replacement()));
        let glyph_schema = !local_object_operands.is_empty()
            || local_program_operands
                .iter()
                .any(|operand| object_operands::glyph(operand.expected()));
        Ok(Self {
            provenance,
            pages,
            local_program_operands,
            drcs_bitmap_overlays,
            local_object_operands,
            glyph_schema,
            object_attribute_schema,
            extended_glyph_schema,
        })
    }
    #[must_use]
    pub const fn schema_version(&self) -> u16 {
        if self.extended_glyph_schema {
            EXTENDED_GLYPH_OVERLAY_VERSION
        } else if self.object_attribute_schema {
            OBJECT_ATTRIBUTE_OVERLAY_VERSION
        } else if self.glyph_schema {
            GLYPH_OVERLAY_VERSION
        } else if !self.drcs_bitmap_overlays.is_empty() {
            DRCS_OVERLAY_VERSION
        } else if self.local_program_operands.is_empty() {
            OVERLAY_VERSION
        } else {
            OPERAND_OVERLAY_VERSION
        }
    }
    #[must_use]
    pub const fn provenance(&self) -> &TeletextSavedEditorialProvenance {
        &self.provenance
    }
    #[must_use]
    pub fn pages(&self) -> &[TeletextPhysicalBaseOverlay] {
        &self.pages
    }
    /// Exact-original local command changes; no decoded effective-program rewrite.
    #[must_use]
    pub fn local_program_operands(&self) -> &[TeletextLocalProgramOperandOverlay] {
        &self.local_program_operands
    }
    /// Exact-original local-object operands; no edit/publication authority.
    #[must_use]
    pub fn local_object_operands(&self) -> &[TeletextLocalObjectOperandOverlay] {
        &self.local_object_operands
    }
    /// Exact-original resource bitmap changes; no edit/publication authority.
    #[must_use]
    pub fn drcs_bitmap_overlays(&self) -> &[TeletextDrcsBitmapOverlay] {
        &self.drcs_bitmap_overlays
    }
}
impl fmt::Debug for TeletextSavedEditorialOverlay {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("TeletextSavedEditorialOverlay")
            .field("schema_version", &self.schema_version())
            .field("provenance", &self.provenance)
            .field("pages", &self.pages.len())
            .field("local_program_operands", &self.local_program_operands.len())
            .field("drcs_bitmap_overlays", &self.drcs_bitmap_overlays.len())
            .field("local_object_operands", &self.local_object_operands.len())
            .finish_non_exhaustive()
    }
}
// Explicit private-codec whitelists keep future domain extensions versioned.
fn same_glyph_family(
    expected: TeletextColumnOperation,
    replacement: TeletextColumnOperation,
) -> bool {
    matches!(
        (expected, replacement),
        (
            TeletextColumnOperation::G0 { .. },
            TeletextColumnOperation::G0 { .. }
        ) | (
            TeletextColumnOperation::G1 { .. },
            TeletextColumnOperation::G1 { .. }
        )
    )
}
fn same_attribute_family(
    expected: TeletextColumnOperation,
    replacement: TeletextColumnOperation,
) -> bool {
    match (expected, replacement) {
        (
            TeletextColumnOperation::Foreground { .. },
            TeletextColumnOperation::Foreground { .. },
        )
        | (
            TeletextColumnOperation::Background { .. },
            TeletextColumnOperation::Background { .. },
        ) => true,
        (
            TeletextColumnOperation::Attributes {
                double_height: old_height,
                double_width: old_width,
                ..
            },
            TeletextColumnOperation::Attributes {
                double_height: new_height,
                double_width: new_width,
                ..
            },
        ) => old_height == new_height && old_width == new_width,
        _ => false,
    }
}
fn supported_family(
    expected: TeletextColumnOperation,
    replacement: TeletextColumnOperation,
) -> bool {
    same_glyph_family(expected, replacement) || same_attribute_family(expected, replacement)
}
fn same_extended_glyph_family(
    expected: TeletextColumnOperation,
    replacement: TeletextColumnOperation,
) -> bool {
    matches!(
        (expected, replacement),
        (
            TeletextColumnOperation::G2 { .. },
            TeletextColumnOperation::G2 { .. }
        ) | (
            TeletextColumnOperation::G0Diacritic { .. },
            TeletextColumnOperation::G0Diacritic { .. }
        )
    )
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
    ($name:ident { $($(#[$attribute:meta])* $field:ident: $ty:ty),* $(,)? }) => {
        #[derive(Serialize)]
        pub(super) struct $name { $($(#[$attribute])* $field: $ty),* }
        impl<'de> Deserialize<'de> for $name {
            fn deserialize<D: Deserializer<'de>>(d: D) -> Result<Self, D::Error> {
                #[derive(Deserialize)] #[serde(deny_unknown_fields)]
                struct Fields { $($(#[$attribute])* $field: $ty),* }
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
    #[serde(default, skip_serializing_if = "OptionalStoredOperands::is_absent")]
    local_program_operands: OptionalStoredOperands,
    #[serde(default, skip_serializing_if = "OptionalStoredBitmaps::is_absent")]
    drcs_bitmap_overlays: OptionalStoredBitmaps,
    #[serde(default, skip_serializing_if = "OptionalStoredObjectOperands::is_absent")]
    local_object_operands: OptionalStoredObjectOperands,
});
closed_map!(StoredPage { owner: super::Entry, rows: Vec<StoredRow> });
closed_map!(StoredRow {
    row: u8,
    cells: Cells
});
closed_map!(StoredOperand {
    owner: super::Entry,
    command: usize,
    column: u8,
    expected: StoredOperation,
    replacement: StoredOperation,
});

#[derive(Default, Serialize)]
struct OptionalStoredOperands(Option<Vec<StoredOperand>>);
impl OptionalStoredOperands {
    fn is_absent(&self) -> bool {
        self.0.is_none()
    }
}
impl<'de> Deserialize<'de> for OptionalStoredOperands {
    fn deserialize<D: Deserializer<'de>>(d: D) -> Result<Self, D::Error> {
        // Omission alone selects absence; present null is never accepted.
        Vec::<StoredOperand>::deserialize(d).map(|value| Self(Some(value)))
    }
}

closed_map!(StoredBitmap {
    identity: TeletextResourceIdentity,
    slot: TeletextDrcsGlyphSlot,
    expected: TeletextDrcsBitmap,
    replacement: TeletextDrcsBitmap,
});
#[derive(Default, Serialize)]
struct OptionalStoredBitmaps(Option<Vec<StoredBitmap>>);
impl OptionalStoredBitmaps {
    fn is_absent(&self) -> bool {
        self.0.is_none()
    }
}
impl<'de> Deserialize<'de> for OptionalStoredBitmaps {
    fn deserialize<D: Deserializer<'de>>(d: D) -> Result<Self, D::Error> {
        Vec::<StoredBitmap>::deserialize(d).map(|value| Self(Some(value)))
    }
}

#[derive(Serialize)]
struct StoredOperation(TeletextColumnOperation);
impl<'de> Deserialize<'de> for StoredOperation {
    fn deserialize<D: Deserializer<'de>>(d: D) -> Result<Self, D::Error> {
        map_only(d).map(Self)
    }
}

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
    fn check_version(&self) -> Result<(), TeletextDesignDocumentError> {
        let operands = &self.local_program_operands.0;
        let bitmaps = &self.drcs_bitmap_overlays.0;
        let objects = &self.local_object_operands.0;
        match (self.schema_version, operands, bitmaps, objects) {
            (OVERLAY_VERSION, None, None, None) => {}
            (OPERAND_OVERLAY_VERSION, Some(operands), None, None) if !operands.is_empty() => {}
            (DRCS_OVERLAY_VERSION, operands, Some(bitmaps), None)
                if !bitmaps.is_empty() && operands.as_ref().is_none_or(|v| !v.is_empty()) => {}
            (EXTENDED_GLYPH_OVERLAY_VERSION, operands, bitmaps, objects)
                if (operands.is_some() || objects.is_some())
                    && operands.as_ref().is_none_or(|v| !v.is_empty())
                    && objects.as_ref().is_none_or(|v| !v.is_empty())
                    && bitmaps.as_ref().is_none_or(|v| !v.is_empty()) => {}
            (OBJECT_ATTRIBUTE_OVERLAY_VERSION, operands, bitmaps, Some(objects))
                if !objects.is_empty()
                    && operands.as_ref().is_none_or(|v| !v.is_empty())
                    && bitmaps.as_ref().is_none_or(|v| !v.is_empty()) => {}
            (GLYPH_OVERLAY_VERSION, operands, bitmaps, objects)
                if operands.as_ref().is_none_or(|v| !v.is_empty())
                    && bitmaps.as_ref().is_none_or(|v| !v.is_empty())
                    && objects.as_ref().is_none_or(|v| !v.is_empty()) => {}
            _ => return Err(invalid()),
        }
        Ok(())
    }

    pub(super) fn restore(
        self,
        documents: &[TeletextProfileDocument],
        budget: TeletextDesignDocumentBudget,
    ) -> Result<TeletextSavedEditorialOverlay, TeletextDesignDocumentError> {
        self.check_version()?;
        let operands = self.local_program_operands.0;
        let bitmaps = self.drcs_bitmap_overlays.0;
        let objects = self.local_object_operands.0;
        let provenance = TeletextSavedEditorialProvenance {
            export_receipt_id: self.export_receipt_id,
            export_receipt_digest: self.export_receipt_digest,
            original_design_id: self.original_design_id,
            original_revision: self.original_revision,
            original_digest: self.original_digest,
        };
        validate_provenance(&provenance)?;
        // Parent streaming preflight already bounded owner/row/operand vectors.
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
        let operands = operands
            .unwrap_or_default()
            .into_iter()
            .map(|operand| {
                TeletextLocalProgramOperandOverlay::new(
                    TeletextPageAddress {
                        page: operand.owner.page,
                        subcode: operand.owner.subcode,
                    },
                    operand.command,
                    operand.column,
                    operand.expected.0,
                    operand.replacement.0,
                )
                .map_err(|_| invalid())
            })
            .collect::<Result<Vec<_>, _>>()?;
        let bitmaps = bitmaps
            .unwrap_or_default()
            .into_iter()
            .map(|bitmap| {
                TeletextDrcsBitmapOverlay::new(
                    bitmap.identity,
                    bitmap.slot,
                    bitmap.expected,
                    bitmap.replacement,
                )
                .map_err(|_| invalid())
            })
            .collect::<Result<Vec<_>, _>>()?;
        let objects = object_operands::restore(objects.unwrap_or_default())?;
        let value = match self.schema_version {
            EXTENDED_GLYPH_OVERLAY_VERSION => {
                TeletextSavedEditorialOverlay::new_with_extended_glyph_operands(
                    provenance, pages, operands, objects, bitmaps,
                )?
            }
            OBJECT_ATTRIBUTE_OVERLAY_VERSION => {
                TeletextSavedEditorialOverlay::new_with_local_object_attributes(
                    provenance, pages, operands, objects, bitmaps,
                )?
            }
            GLYPH_OVERLAY_VERSION => TeletextSavedEditorialOverlay::new_with_local_object_operands(
                provenance, pages, operands, objects, bitmaps,
            )?,
            DRCS_OVERLAY_VERSION => TeletextSavedEditorialOverlay::new_with_drcs_bitmap_overlays(
                provenance, pages, operands, bitmaps,
            )?,
            OPERAND_OVERLAY_VERSION => {
                TeletextSavedEditorialOverlay::new_with_local_program_operands(
                    provenance, pages, operands,
                )?
            }
            _ => TeletextSavedEditorialOverlay::new(provenance, pages)?,
        };
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
        unique_original_owner(page.owner(), documents)?;
    }
    limit(
        overlay
            .local_program_operands
            .len()
            .checked_add(overlay.local_object_operands.len())
            .ok_or_else(|| failure(TeletextDesignDocumentFailure::Storage))?,
        budget.validation.service.max_commands,
        "editorial operands",
    )?;
    items = items
        .checked_add(overlay.local_program_operands.len())
        .and_then(|count| count.checked_add(overlay.local_object_operands.len()))
        .ok_or_else(|| failure(TeletextDesignDocumentFailure::Storage))?;
    limit(
        items,
        budget.validation.service.max_structure_items,
        "editorial structure items",
    )?;
    items = items
        .checked_add(overlay.drcs_bitmap_overlays.len())
        .ok_or_else(|| failure(TeletextDesignDocumentFailure::Storage))?;
    limit(
        items,
        budget.validation.service.max_structure_items,
        "editorial structure items",
    )?;
    validate_bitmap_association(&overlay.drcs_bitmap_overlays, documents)?;
    object_operands::validate_association(&overlay.local_object_operands, documents)?;
    for operand in &overlay.local_program_operands {
        let TeletextProfileDocument::Authored(service) =
            unique_original_owner(operand.owner(), documents)?
        else {
            return Err(invalid());
        };
        let program = service
            .semantic()
            .page_enhancements()
            .get(&operand.owner())
            .and_then(|page| page.local_program.as_ref())
            .ok_or_else(invalid)?;
        if program.commands().get(operand.command())
            != Some(&TeletextEnhancementCommand::AtColumn {
                column: operand.column(),
                operation: operand.expected(),
            })
        {
            return Err(invalid());
        }
    }
    Ok(())
}

fn validate_bitmap_association(
    bitmaps: &[TeletextDrcsBitmapOverlay],
    documents: &[TeletextProfileDocument],
) -> Result<(), TeletextDesignDocumentError> {
    for bitmap in bitmaps {
        let mut original = None;
        for document in documents {
            let TeletextProfileDocument::Authored(service) = document else {
                continue;
            };
            for table in service
                .semantic()
                .drcs()
                .subtables()
                .iter()
                .filter(|table| table.identity() == bitmap.identity())
            {
                if original.is_some_and(|previous| previous != table) {
                    return Err(invalid());
                }
                original = Some(table);
            }
        }
        let table = original.ok_or_else(invalid)?;
        if !table
            .glyphs()
            .iter()
            .any(|glyph| glyph.slot == bitmap.slot() && glyph.bitmap == bitmap.expected())
        {
            return Err(invalid());
        }
    }
    Ok(())
}

fn unique_original_owner(
    owner: TeletextPageAddress,
    documents: &[TeletextProfileDocument],
) -> Result<&TeletextProfileDocument, TeletextDesignDocumentError> {
    let mut owners = documents.iter().filter(|document| {
        let base = match document {
            TeletextProfileDocument::Base(base) => base,
            TeletextProfileDocument::Authored(service) => service.semantic().base(),
        };
        base.pages.iter().any(|page| page.address == owner)
    });
    let document = owners.next().ok_or_else(invalid)?;
    // Ambiguous ownership is rejected, including a competing base-only producer.
    if owners.next().is_some() {
        return Err(invalid());
    }
    Ok(document)
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
    #[serde(skip_serializing_if = "Option::is_none")]
    local_program_operands: Option<BorrowedOperands<'a>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    drcs_bitmap_overlays: Option<BorrowedBitmaps<'a>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    local_object_operands: Option<BorrowedObjectOperands<'a>>,
}
struct BorrowedBitmaps<'a>(&'a [TeletextDrcsBitmapOverlay]);
impl Serialize for BorrowedBitmaps<'_> {
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        let mut sequence = serializer.serialize_seq(Some(self.0.len()))?;
        for bitmap in self.0 {
            sequence.serialize_element(&StoredBitmap {
                identity: bitmap.identity(),
                slot: bitmap.slot(),
                expected: bitmap.expected(),
                replacement: bitmap.replacement(),
            })?;
        }
        sequence.end()
    }
}
struct BorrowedOperands<'a>(&'a [TeletextLocalProgramOperandOverlay]);
impl Serialize for BorrowedOperands<'_> {
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        #[derive(Serialize)]
        struct BorrowedOperand {
            owner: super::Entry,
            command: usize,
            column: u8,
            expected: TeletextColumnOperation,
            replacement: TeletextColumnOperation,
        }
        let mut sequence = serializer.serialize_seq(Some(self.0.len()))?;
        for operand in self.0 {
            sequence.serialize_element(&BorrowedOperand {
                owner: super::Entry {
                    page: operand.owner().page,
                    subcode: operand.owner().subcode,
                },
                command: operand.command(),
                column: operand.column(),
                expected: operand.expected(),
                replacement: operand.replacement(),
            })?;
        }
        sequence.end()
    }
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

/// Append concept rows, local operands and bitmaps to exact immutable originals.
/// Existing strings/JSON prefix and `OriginalOnly` policy remain intact. This checks
/// syntax, bounds and exact original owner/command/resource association; authenticate receipt
/// provenance, enforce edit regions and native preview before any save/publication.
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
            schema_version: overlay.schema_version(),
            export_receipt_id: value.export_receipt_id,
            export_receipt_digest: &value.export_receipt_digest,
            original_design_id: value.original_design_id,
            original_revision: value.original_revision,
            original_digest: &value.original_digest,
            pages: BorrowedPages(&overlay.pages),
            local_program_operands: (!overlay.local_program_operands.is_empty())
                .then_some(BorrowedOperands(&overlay.local_program_operands)),
            drcs_bitmap_overlays: (!overlay.drcs_bitmap_overlays.is_empty())
                .then_some(BorrowedBitmaps(&overlay.drcs_bitmap_overlays)),
            local_object_operands: (!overlay.local_object_operands.is_empty())
                .then_some(BorrowedObjectOperands(&overlay.local_object_operands)),
        },
    )?;
    writer
        .write_all(&original[end..])
        .map_err(serde_json::Error::io)
}

#[cfg(test)]
#[path = "editorial_overlay/tests.rs"]
mod tests;

#[cfg(test)]
#[path = "editorial_overlay/drcs_tests.rs"]
mod drcs_tests;

#[cfg(test)]
#[path = "editorial_overlay/glyph_tests.rs"]
mod glyph_tests;

#[cfg(test)]
#[path = "editorial_overlay/object_attribute_tests.rs"]
mod object_attribute_tests;

#[cfg(test)]
#[path = "editorial_overlay/extended_glyph_tests.rs"]
mod extended_glyph_tests;
