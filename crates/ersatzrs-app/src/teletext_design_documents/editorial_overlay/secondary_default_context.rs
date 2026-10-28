//! Receipt provenance for a context-only concept, without fabricated row edits.
use super::{
    invalid, StoredOverlay, TeletextDesignDocumentError, TeletextSavedEditorialOverlay,
    TeletextSavedEditorialProvenance,
};

const CONTEXT_VERSION: u16 = 32;

impl TeletextSavedEditorialOverlay {
    /// Retain the original export receipt when only secondary-default intent
    /// changes. Empty intent can explicitly return a previously bound original
    /// to decoder-dependent context; it never declares no second G0.
    /// Structural validity grants no edit or publication authority.
    /// # Errors
    /// Rejects malformed provenance under the existing receipt limits.
    pub fn new_secondary_default_context(
        provenance: TeletextSavedEditorialProvenance,
    ) -> Result<Self, TeletextDesignDocumentError> {
        let mut value = Self::new_parts(provenance, vec![], vec![], vec![], vec![])?;
        value.schema_version = CONTEXT_VERSION;
        Ok(value)
    }
}

impl StoredOverlay {
    pub(super) fn is_secondary_default_context(&self) -> bool {
        self.schema_version == CONTEXT_VERSION
    }

    pub(super) fn restore_secondary_default_context(
        self,
    ) -> Result<TeletextSavedEditorialOverlay, TeletextDesignDocumentError> {
        // Version32 is provenance only. Existing editorial bodies retain their
        // independently checked versions; extra fields cannot be smuggled here.
        if !self.pages.is_empty()
            || self.prior_schema_version.0.is_some()
            || self.local_program_operands.0.is_some()
            || self.drcs_bitmap_overlays.0.is_some()
            || self.local_object_operands.0.is_some()
            || self.local_object_glyph_program_overlays.0.is_some()
            || self.main_local_invocation_program_overlays.0.is_some()
            || self.main_resource_invocations.0.is_some()
            || self.main_program_presence_overlays.0.is_some()
            || self.resource_object_operands.0.is_some()
            || self.resource_object_program_overlays.0.is_some()
            || self.resource_object_definition_additions.0.is_some()
            || self.object_resource_additions.0.is_some()
            || self.object_subpage_announcements.0.is_some()
            || self.object_page_selections.0.is_some()
            || self.base_text_edits.0.is_some()
        {
            return Err(invalid());
        }
        TeletextSavedEditorialOverlay::new_secondary_default_context(
            TeletextSavedEditorialProvenance {
                export_receipt_id: self.export_receipt_id,
                export_receipt_digest: self.export_receipt_digest,
                original_design_id: self.original_design_id,
                original_revision: self.original_revision,
                original_digest: self.original_digest,
            },
        )
    }
}
