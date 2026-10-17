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
    TeletextDrcsGlyphSlot, TeletextEnhancementCommand, TeletextLocalObjectGlyphProgramOverlay,
    TeletextLocalObjectOperandOverlay, TeletextLocalProgramOperandOverlay, TeletextPageAddress,
    TeletextPhysicalBaseOverlay, TeletextPhysicalBaseRow, TeletextResourceIdentity,
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
const REMAINING_GLYPH_OVERLAY_VERSION: u16 = 7;
const DRCS_REFERENCE_OVERLAY_VERSION: u16 = 8;
const FLASH_CHARSET_OVERLAY_VERSION: u16 = 9;
const DRCS_SELECTION_OVERLAY_VERSION: u16 = 10;
const LOCAL_GLYPH_PROGRAM_OVERLAY_VERSION: u16 = 11;
const ALL_LOCAL_GLYPH_PROGRAM_OVERLAY_VERSION: u16 = 12;
const LOCAL_STYLE_PROGRAM_OVERLAY_VERSION: u16 = 13;

#[path = "editorial_overlay/object_operands.rs"]
pub(super) mod object_operands;
use object_operands::{BorrowedObjectOperands, OptionalStoredObjectOperands};

#[path = "editorial_overlay/local_object_glyph_programs.rs"]
pub(super) mod local_object_glyph_programs;
use local_object_glyph_programs::{
    BorrowedGlyphPrograms, OptionalStoredGlyphPrograms, ProgramPolicy,
};

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
    local_object_glyph_program_overlays: Vec<TeletextLocalObjectGlyphProgramOverlay>,
    schema_version: u16,
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
            OperandFamilies::Extended,
        )
    }

    /// Capture version 7 G1-alphabetic or G3 operands in original local commands.
    ///
    /// At least one page-local or local-object operand must change within one of
    /// these families. G3 retains its original Level 1.5/2.5 compatibility mode.
    /// Known version1–6 row, glyph, attribute and bitmap intent may accompany the
    /// change. This grants no authoring-region permission or publication authority.
    /// # Errors
    /// Rejects missing G1-alphabetic/G3 intent, unsupported or crossed families,
    /// changed G3 compatibility, invalid provenance and repeated original targets.
    pub fn new_with_remaining_glyph_operands(
        provenance: TeletextSavedEditorialProvenance,
        pages: Vec<TeletextPhysicalBaseOverlay>,
        local_program_operands: Vec<TeletextLocalProgramOperandOverlay>,
        local_object_operands: Vec<TeletextLocalObjectOperandOverlay>,
        drcs_bitmap_overlays: Vec<TeletextDrcsBitmapOverlay>,
    ) -> Result<Self, TeletextDesignDocumentError> {
        if !local_program_operands
            .iter()
            .any(|operand| same_remaining_glyph_family(operand.expected(), operand.replacement()))
            && !local_object_operands.iter().any(|operand| {
                same_remaining_glyph_family(operand.expected(), operand.replacement())
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
            OperandFamilies::Remaining,
        )
    }

    /// Capture version 8 slot changes in existing page-local or local-object DRCS references.
    ///
    /// At least one reference must change its slot while retaining the original
    /// Normal/Global table and logical subtable. Known version1–7 intents may
    /// accompany the change. Catalog resolution and all protected display effects
    /// still require native validation; this grants no publication authority.
    /// # Errors
    /// Rejects missing DRCS slot intent, changed table/subtable, unsupported or
    /// crossed families, invalid provenance and repeated original targets.
    pub fn new_with_drcs_reference_slots(
        provenance: TeletextSavedEditorialProvenance,
        pages: Vec<TeletextPhysicalBaseOverlay>,
        local_program_operands: Vec<TeletextLocalProgramOperandOverlay>,
        local_object_operands: Vec<TeletextLocalObjectOperandOverlay>,
        drcs_bitmap_overlays: Vec<TeletextDrcsBitmapOverlay>,
    ) -> Result<Self, TeletextDesignDocumentError> {
        if !local_program_operands
            .iter()
            .any(|operand| same_drcs_reference_slots(operand.expected(), operand.replacement()))
            && !local_object_operands
                .iter()
                .any(|operand| same_drcs_reference_slots(operand.expected(), operand.replacement()))
        {
            return Err(invalid());
        }
        Self::new_parts_for_families(
            provenance,
            pages,
            local_program_operands,
            drcs_bitmap_overlays,
            local_object_operands,
            OperandFamilies::DrcsReferences,
        )
    }

    /// Capture version 9 Flash or `DesignateCharset` edits in original local commands.
    ///
    /// At least one page-local or local-object operand must change within one of
    /// these two typed families. Known version1–8 intents may accompany the edit.
    /// Native reconstruction still qualifies all repertoire/context and protected
    /// display effects; this grants no permission or publication authority.
    /// # Errors
    /// Rejects missing Flash/`DesignateCharset` intent, crossed or unsupported
    /// families, invalid provenance and repeated exact original targets.
    pub fn new_with_flash_charset_operands(
        provenance: TeletextSavedEditorialProvenance,
        pages: Vec<TeletextPhysicalBaseOverlay>,
        local_program_operands: Vec<TeletextLocalProgramOperandOverlay>,
        local_object_operands: Vec<TeletextLocalObjectOperandOverlay>,
        drcs_bitmap_overlays: Vec<TeletextDrcsBitmapOverlay>,
    ) -> Result<Self, TeletextDesignDocumentError> {
        if !local_program_operands
            .iter()
            .any(|operand| same_flash_charset_family(operand.expected(), operand.replacement()))
            && !local_object_operands
                .iter()
                .any(|operand| same_flash_charset_family(operand.expected(), operand.replacement()))
        {
            return Err(invalid());
        }
        Self::new_parts_for_families(
            provenance,
            pages,
            local_program_operands,
            drcs_bitmap_overlays,
            local_object_operands,
            OperandFamilies::FlashCharset,
        )
    }

    /// Capture version 10 DRCS table/subtable selection changes in original commands.
    ///
    /// At least one page-local or local-object reference must change its checked
    /// Normal/Global scope or logical subtable. Slot changes and known version1–9
    /// intents may accompany it. Resource declarations/catalogs stay original;
    /// native reconstruction still resolves every target and enforces usage caps.
    /// This grants no edit permission or publication authority.
    /// # Errors
    /// Rejects missing selection change, unsupported/crossed families, invalid
    /// provenance and repeated exact original targets.
    pub fn new_with_drcs_reference_selections(
        provenance: TeletextSavedEditorialProvenance,
        pages: Vec<TeletextPhysicalBaseOverlay>,
        local_program_operands: Vec<TeletextLocalProgramOperandOverlay>,
        local_object_operands: Vec<TeletextLocalObjectOperandOverlay>,
        drcs_bitmap_overlays: Vec<TeletextDrcsBitmapOverlay>,
    ) -> Result<Self, TeletextDesignDocumentError> {
        if !local_program_operands
            .iter()
            .any(|operand| changed_drcs_selection(operand.expected(), operand.replacement()))
            && !local_object_operands
                .iter()
                .any(|operand| changed_drcs_selection(operand.expected(), operand.replacement()))
        {
            return Err(invalid());
        }
        Self::new_parts_for_families(
            provenance,
            pages,
            local_program_operands,
            drcs_bitmap_overlays,
            local_object_operands,
            OperandFamilies::DrcsSelections,
        )
    }

    /// Capture version 11 bounded G0/G1 structure changes in original Local programs.
    ///
    /// A length, retained-command map or glyph-column change is required. Known
    /// version1–10 intents may accompany it. Same-object G0/G1 scalar changes must
    /// be folded into the structural replacement; nonglyph scalar changes retain
    /// original ordinals and are applied before matching the expected program.
    /// Restoring binds every intent to the immutable original native definition.
    /// This grants no region permission, save eligibility or publication authority.
    /// # Errors
    /// Rejects missing structural intent, repeated definitions, scalar G0/G1
    /// overlap, unsupported scalar families or invalid provenance.
    pub fn new_with_local_object_glyph_programs(
        provenance: TeletextSavedEditorialProvenance,
        pages: Vec<TeletextPhysicalBaseOverlay>,
        local_program_operands: Vec<TeletextLocalProgramOperandOverlay>,
        local_object_operands: Vec<TeletextLocalObjectOperandOverlay>,
        drcs_bitmap_overlays: Vec<TeletextDrcsBitmapOverlay>,
        programs: Vec<TeletextLocalObjectGlyphProgramOverlay>,
    ) -> Result<Self, TeletextDesignDocumentError> {
        local_object_glyph_programs::distinct(
            &programs,
            &local_object_operands,
            ProgramPolicy::Legacy,
        )?;
        let mut value = Self::new_parts_for_families(
            provenance,
            pages,
            local_program_operands,
            drcs_bitmap_overlays,
            local_object_operands,
            OperandFamilies::DrcsSelections,
        )?;
        value.local_object_glyph_program_overlays = programs;
        value.schema_version = LOCAL_GLYPH_PROGRAM_OVERLAY_VERSION;
        Ok(value)
    }

    /// Capture version 12 bounded structure changes using additional Local glyph families.
    ///
    /// At least one program must require the wider glyph policy and at least one
    /// must change length, correspondence or a glyph column. All same-object
    /// scalar glyph changes must be folded into program replacements. Nonglyph
    /// scalar changes still patch immutable original ordinals first. Known prior
    /// intents may accompany the change; native qualification and captured edit
    /// permissions remain required before saving or explicitly publishing.
    /// # Errors
    /// Rejects legacy-only or nonstructural aliases, repeated definitions, scalar
    /// glyph overlap, unsupported scalar families and invalid provenance.
    pub fn new_with_all_local_object_glyph_programs(
        provenance: TeletextSavedEditorialProvenance,
        pages: Vec<TeletextPhysicalBaseOverlay>,
        local_program_operands: Vec<TeletextLocalProgramOperandOverlay>,
        local_object_operands: Vec<TeletextLocalObjectOperandOverlay>,
        drcs_bitmap_overlays: Vec<TeletextDrcsBitmapOverlay>,
        programs: Vec<TeletextLocalObjectGlyphProgramOverlay>,
    ) -> Result<Self, TeletextDesignDocumentError> {
        local_object_glyph_programs::distinct(
            &programs,
            &local_object_operands,
            ProgramPolicy::AllGlyphs,
        )?;
        let mut value = Self::new_parts_for_families(
            provenance,
            pages,
            local_program_operands,
            drcs_bitmap_overlays,
            local_object_operands,
            OperandFamilies::DrcsSelections,
        )?;
        value.local_object_glyph_program_overlays = programs;
        value.schema_version = ALL_LOCAL_GLYPH_PROGRAM_OVERLAY_VERSION;
        Ok(value)
    }

    /// Capture version13 inserted or deleted Local foreground/background/attributes.
    ///
    /// At least one program must require structural style support. All checked
    /// glyph families may accompany it; retained style commands stay exact after
    /// independently authenticated scalar patches to original ordinals. Same-object
    /// scalar glyph edits must be folded into the program. Immutable originals,
    /// bounded restoration and native shared-consumer qualification still apply.
    /// This grants no edit permission or publication authority.
    /// # Errors
    /// Rejects older-version aliases, repeated definitions, scalar glyph overlap,
    /// unsupported scalar families and invalid provenance.
    pub fn new_with_local_object_style_programs(
        provenance: TeletextSavedEditorialProvenance,
        pages: Vec<TeletextPhysicalBaseOverlay>,
        local_program_operands: Vec<TeletextLocalProgramOperandOverlay>,
        local_object_operands: Vec<TeletextLocalObjectOperandOverlay>,
        drcs_bitmap_overlays: Vec<TeletextDrcsBitmapOverlay>,
        programs: Vec<TeletextLocalObjectGlyphProgramOverlay>,
    ) -> Result<Self, TeletextDesignDocumentError> {
        local_object_glyph_programs::distinct(
            &programs,
            &local_object_operands,
            ProgramPolicy::StyleCommands,
        )?;
        let mut value = Self::new_parts_for_families(
            provenance,
            pages,
            local_program_operands,
            drcs_bitmap_overlays,
            local_object_operands,
            OperandFamilies::DrcsSelections,
        )?;
        value.local_object_glyph_program_overlays = programs;
        value.schema_version = LOCAL_STYLE_PROGRAM_OVERLAY_VERSION;
        Ok(value)
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
            OperandFamilies::Legacy,
        )
    }

    fn new_parts_for_families(
        provenance: TeletextSavedEditorialProvenance,
        pages: Vec<TeletextPhysicalBaseOverlay>,
        local_program_operands: Vec<TeletextLocalProgramOperandOverlay>,
        drcs_bitmap_overlays: Vec<TeletextDrcsBitmapOverlay>,
        local_object_operands: Vec<TeletextLocalObjectOperandOverlay>,
        families: OperandFamilies,
    ) -> Result<Self, TeletextDesignDocumentError> {
        validate_provenance(&provenance)?;
        let supported = |expected, replacement| families.supports(expected, replacement);
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
        let schema_version = inferred_schema_version(
            &local_program_operands,
            &local_object_operands,
            &drcs_bitmap_overlays,
        );
        Ok(Self {
            provenance,
            pages,
            local_program_operands,
            drcs_bitmap_overlays,
            local_object_operands,
            local_object_glyph_program_overlays: Vec::new(),
            schema_version,
        })
    }
    #[must_use]
    pub const fn schema_version(&self) -> u16 {
        self.schema_version
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
    /// Exact-original Local glyph program changes and retained-command correspondence.
    #[must_use]
    pub fn local_object_glyph_program_overlays(&self) -> &[TeletextLocalObjectGlyphProgramOverlay] {
        &self.local_object_glyph_program_overlays
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
            .field(
                "local_object_glyph_program_overlays",
                &self.local_object_glyph_program_overlays.len(),
            )
            .finish_non_exhaustive()
    }
}
// Explicit private-codec whitelists keep future domain extensions versioned.
#[derive(Clone, Copy)]
enum OperandFamilies {
    Legacy,
    Extended,
    Remaining,
    DrcsReferences,
    FlashCharset,
    DrcsSelections,
}
impl OperandFamilies {
    fn supports(
        self,
        expected: TeletextColumnOperation,
        replacement: TeletextColumnOperation,
    ) -> bool {
        supported_family(expected, replacement)
            || (!matches!(self, Self::Legacy) && same_extended_glyph_family(expected, replacement))
            || (matches!(
                self,
                Self::Remaining | Self::DrcsReferences | Self::FlashCharset | Self::DrcsSelections
            ) && same_remaining_glyph_family(expected, replacement))
            || (matches!(
                self,
                Self::DrcsReferences | Self::FlashCharset | Self::DrcsSelections
            ) && same_drcs_reference_slots(expected, replacement))
            || (matches!(self, Self::FlashCharset | Self::DrcsSelections)
                && same_flash_charset_family(expected, replacement))
            || (matches!(self, Self::DrcsSelections)
                && changed_drcs_selection(expected, replacement))
    }
}
fn inferred_schema_version(
    operands: &[TeletextLocalProgramOperandOverlay],
    objects: &[TeletextLocalObjectOperandOverlay],
    bitmaps: &[TeletextDrcsBitmapOverlay],
) -> u16 {
    let any_family = |family: fn(TeletextColumnOperation, TeletextColumnOperation) -> bool| {
        operands
            .iter()
            .any(|operand| family(operand.expected(), operand.replacement()))
            || objects
                .iter()
                .any(|operand| family(operand.expected(), operand.replacement()))
    };
    if any_family(changed_drcs_selection) {
        DRCS_SELECTION_OVERLAY_VERSION
    } else if any_family(same_flash_charset_family) {
        FLASH_CHARSET_OVERLAY_VERSION
    } else if any_family(same_drcs_reference_slots) {
        DRCS_REFERENCE_OVERLAY_VERSION
    } else if any_family(same_remaining_glyph_family) {
        REMAINING_GLYPH_OVERLAY_VERSION
    } else if any_family(same_extended_glyph_family) {
        EXTENDED_GLYPH_OVERLAY_VERSION
    } else if objects
        .iter()
        .any(|operand| same_attribute_family(operand.expected(), operand.replacement()))
    {
        OBJECT_ATTRIBUTE_OVERLAY_VERSION
    } else if !objects.is_empty()
        || operands
            .iter()
            .any(|operand| object_operands::glyph(operand.expected()))
    {
        GLYPH_OVERLAY_VERSION
    } else if !bitmaps.is_empty() {
        DRCS_OVERLAY_VERSION
    } else if operands.is_empty() {
        OVERLAY_VERSION
    } else {
        OPERAND_OVERLAY_VERSION
    }
}
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
fn same_remaining_glyph_family(
    expected: TeletextColumnOperation,
    replacement: TeletextColumnOperation,
) -> bool {
    match (expected, replacement) {
        (TeletextColumnOperation::G1Alpha { .. }, TeletextColumnOperation::G1Alpha { .. }) => true,
        (
            TeletextColumnOperation::G3 {
                compatibility: original,
                ..
            },
            TeletextColumnOperation::G3 {
                compatibility: changed,
                ..
            },
        ) => original == changed,
        _ => false,
    }
}
fn same_drcs_reference_slots(
    expected: TeletextColumnOperation,
    replacement: TeletextColumnOperation,
) -> bool {
    match (expected, replacement) {
        (
            TeletextColumnOperation::Drcs {
                reference: original,
            },
            TeletextColumnOperation::Drcs { reference: changed },
        ) => {
            original.table == changed.table
                && original.subtable == changed.subtable
                && original.slot != changed.slot
        }
        _ => false,
    }
}
fn same_flash_charset_family(
    expected: TeletextColumnOperation,
    replacement: TeletextColumnOperation,
) -> bool {
    matches!(
        (expected, replacement),
        (
            TeletextColumnOperation::Flash { .. },
            TeletextColumnOperation::Flash { .. }
        ) | (
            TeletextColumnOperation::DesignateCharset { .. },
            TeletextColumnOperation::DesignateCharset { .. }
        )
    )
}
fn changed_drcs_selection(
    expected: TeletextColumnOperation,
    replacement: TeletextColumnOperation,
) -> bool {
    match (expected, replacement) {
        (
            TeletextColumnOperation::Drcs {
                reference: original,
            },
            TeletextColumnOperation::Drcs { reference: changed },
        ) => original.table != changed.table || original.subtable != changed.subtable,
        _ => false,
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
    #[serde(default, skip_serializing_if = "OptionalStoredGlyphPrograms::is_absent")]
    local_object_glyph_program_overlays: OptionalStoredGlyphPrograms,
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

fn restore_page(
    page: StoredPage,
) -> Result<TeletextPhysicalBaseOverlay, TeletextDesignDocumentError> {
    let rows = page
        .rows
        .into_iter()
        .map(|row| TeletextPhysicalBaseRow::new(row.row, row.cells.0).map_err(|_| invalid()))
        .collect::<Result<Vec<_>, _>>()?;
    TeletextPhysicalBaseOverlay::new(
        TeletextPageAddress {
            page: page.owner.page,
            subcode: page.owner.subcode,
        },
        rows,
    )
    .map_err(|_| invalid())
}

fn restore_operand(
    operand: &StoredOperand,
) -> Result<TeletextLocalProgramOperandOverlay, TeletextDesignDocumentError> {
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
}

fn restore_bitmap(
    bitmap: &StoredBitmap,
) -> Result<TeletextDrcsBitmapOverlay, TeletextDesignDocumentError> {
    TeletextDrcsBitmapOverlay::new(
        bitmap.identity,
        bitmap.slot,
        bitmap.expected,
        bitmap.replacement,
    )
    .map_err(|_| invalid())
}

impl StoredOverlay {
    fn check_version(&self) -> Result<(), TeletextDesignDocumentError> {
        let operands = &self.local_program_operands.0;
        let bitmaps = &self.drcs_bitmap_overlays.0;
        let objects = &self.local_object_operands.0;
        let programs = &self.local_object_glyph_program_overlays.0;
        if matches!(
            self.schema_version,
            LOCAL_GLYPH_PROGRAM_OVERLAY_VERSION
                | ALL_LOCAL_GLYPH_PROGRAM_OVERLAY_VERSION
                | LOCAL_STYLE_PROGRAM_OVERLAY_VERSION
        ) {
            if programs.as_ref().is_none_or(Vec::is_empty)
                || operands.as_ref().is_some_and(Vec::is_empty)
                || objects.as_ref().is_some_and(Vec::is_empty)
                || bitmaps.as_ref().is_some_and(Vec::is_empty)
            {
                return Err(invalid());
            }
            return Ok(());
        }
        if programs.is_some() {
            return Err(invalid());
        }
        match (self.schema_version, operands, bitmaps, objects) {
            (OVERLAY_VERSION, None, None, None) => {}
            (OPERAND_OVERLAY_VERSION, Some(operands), None, None) if !operands.is_empty() => {}
            (DRCS_OVERLAY_VERSION, operands, Some(bitmaps), None)
                if !bitmaps.is_empty() && operands.as_ref().is_none_or(|v| !v.is_empty()) => {}
            (
                EXTENDED_GLYPH_OVERLAY_VERSION
                | REMAINING_GLYPH_OVERLAY_VERSION
                | DRCS_REFERENCE_OVERLAY_VERSION
                | FLASH_CHARSET_OVERLAY_VERSION
                | DRCS_SELECTION_OVERLAY_VERSION,
                operands,
                bitmaps,
                objects,
            ) if (operands.is_some() || objects.is_some())
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

    fn program_policy(&self) -> ProgramPolicy {
        match self.schema_version {
            LOCAL_STYLE_PROGRAM_OVERLAY_VERSION => ProgramPolicy::StyleCommands,
            ALL_LOCAL_GLYPH_PROGRAM_OVERLAY_VERSION => ProgramPolicy::AllGlyphs,
            _ => ProgramPolicy::Legacy,
        }
    }

    pub(super) fn restore(
        self,
        documents: &[TeletextProfileDocument],
        budget: TeletextDesignDocumentBudget,
    ) -> Result<TeletextSavedEditorialOverlay, TeletextDesignDocumentError> {
        self.check_version()?;
        let policy = self.program_policy();
        let operands = self.local_program_operands.0;
        let bitmaps = self.drcs_bitmap_overlays.0;
        let objects = self.local_object_operands.0;
        let programs = self.local_object_glyph_program_overlays.0;
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
            .map(restore_page)
            .collect::<Result<Vec<_>, _>>()?;
        let operands = operands
            .unwrap_or_default()
            .iter()
            .map(restore_operand)
            .collect::<Result<Vec<_>, _>>()?;
        let bitmaps = bitmaps
            .unwrap_or_default()
            .iter()
            .map(restore_bitmap)
            .collect::<Result<Vec<_>, _>>()?;
        let objects = object_operands::restore(objects.unwrap_or_default())?;
        let value = match self.schema_version {
            LOCAL_GLYPH_PROGRAM_OVERLAY_VERSION
            | ALL_LOCAL_GLYPH_PROGRAM_OVERLAY_VERSION
            | LOCAL_STYLE_PROGRAM_OVERLAY_VERSION => {
                let constructor = match policy {
                    ProgramPolicy::StyleCommands => {
                        TeletextSavedEditorialOverlay::new_with_local_object_style_programs
                    }
                    ProgramPolicy::AllGlyphs => {
                        TeletextSavedEditorialOverlay::new_with_all_local_object_glyph_programs
                    }
                    ProgramPolicy::Legacy => {
                        TeletextSavedEditorialOverlay::new_with_local_object_glyph_programs
                    }
                };
                constructor(
                    provenance,
                    pages,
                    operands,
                    objects,
                    bitmaps,
                    local_object_glyph_programs::restore(programs.unwrap_or_default(), policy)?,
                )?
            }
            DRCS_SELECTION_OVERLAY_VERSION => {
                TeletextSavedEditorialOverlay::new_with_drcs_reference_selections(
                    provenance, pages, operands, objects, bitmaps,
                )?
            }
            FLASH_CHARSET_OVERLAY_VERSION => {
                TeletextSavedEditorialOverlay::new_with_flash_charset_operands(
                    provenance, pages, operands, objects, bitmaps,
                )?
            }
            DRCS_REFERENCE_OVERLAY_VERSION => {
                TeletextSavedEditorialOverlay::new_with_drcs_reference_slots(
                    provenance, pages, operands, objects, bitmaps,
                )?
            }
            REMAINING_GLYPH_OVERLAY_VERSION => {
                TeletextSavedEditorialOverlay::new_with_remaining_glyph_operands(
                    provenance, pages, operands, objects, bitmaps,
                )?
            }
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
    local_object_glyph_programs::validate_budgets(overlay, items, budget)?;
    local_object_glyph_programs::validate_association(
        &overlay.local_object_glyph_program_overlays,
        &overlay.local_object_operands,
        documents,
    )?;
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
    #[serde(skip_serializing_if = "Option::is_none")]
    local_object_glyph_program_overlays: Option<BorrowedGlyphPrograms<'a>>,
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
            local_object_glyph_program_overlays: (!overlay
                .local_object_glyph_program_overlays
                .is_empty())
            .then_some(BorrowedGlyphPrograms(
                &overlay.local_object_glyph_program_overlays,
            )),
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

#[cfg(test)]
#[path = "editorial_overlay/remaining_glyph_tests.rs"]
mod remaining_glyph_tests;

#[cfg(test)]
#[path = "editorial_overlay/drcs_reference_tests.rs"]
mod drcs_reference_tests;

#[cfg(test)]
#[path = "editorial_overlay/flash_charset_tests.rs"]
mod flash_charset_tests;

#[cfg(test)]
#[path = "editorial_overlay/drcs_selection_tests.rs"]
mod drcs_selection_tests;

#[cfg(test)]
#[path = "editorial_overlay/local_object_glyph_program_tests.rs"]
mod local_object_glyph_program_tests;

#[cfg(test)]
#[path = "editorial_overlay/all_local_object_glyph_program_tests.rs"]
mod all_local_object_glyph_program_tests;

#[cfg(test)]
#[path = "editorial_overlay/local_object_style_program_tests.rs"]
mod local_object_style_program_tests;
