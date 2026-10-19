//! Closed V23 Base Text intent, retaining all previously supported sibling roles.
use super::{
    invalid, limit, map_only, StoredOverlay, TeletextDesignDocumentBudget,
    TeletextDesignDocumentError, TeletextProfileDocument, TeletextSavedEditorialOverlay,
    TeletextSavedEditorialProvenance,
};
use ersatzrs_domain::{
    TeletextBaseTextEdit, TeletextEnhancementPosition, TeletextPageAddress, TeletextRunContent,
};
use serde::{ser::SerializeSeq, Deserialize, Deserializer, Serialize, Serializer};
use std::collections::BTreeSet;

#[derive(Serialize)]
pub(in super::super) struct StoredBaseTextEdit {
    owner: super::super::Entry,
    row: u8,
    column: u8,
    expected: char,
    replacement: char,
}
impl<'de> Deserialize<'de> for StoredBaseTextEdit {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        #[derive(Deserialize)]
        #[serde(deny_unknown_fields)]
        struct Fields {
            owner: super::super::Entry,
            row: u8,
            column: u8,
            expected: char,
            replacement: char,
        }
        let value: Fields = map_only(deserializer)?;
        Ok(Self {
            owner: value.owner,
            row: value.row,
            column: value.column,
            expected: value.expected,
            replacement: value.replacement,
        })
    }
}
#[derive(Default, Serialize)]
pub(super) struct OptionalStoredBaseTextEdits(pub(super) Option<Vec<StoredBaseTextEdit>>);
impl OptionalStoredBaseTextEdits {
    pub(super) fn is_absent(&self) -> bool {
        self.0.is_none()
    }
}
impl<'de> Deserialize<'de> for OptionalStoredBaseTextEdits {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        Vec::<StoredBaseTextEdit>::deserialize(deserializer).map(|values| Self(Some(values)))
    }
}
pub(super) struct BorrowedBaseTextEdits<'a>(pub(super) &'a [TeletextBaseTextEdit]);
impl Serialize for BorrowedBaseTextEdits<'_> {
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        let mut sequence = serializer.serialize_seq(Some(self.0.len()))?;
        for edit in self.0 {
            sequence.serialize_element(&StoredBaseTextEdit {
                owner: super::super::Entry {
                    page: edit.owner().page,
                    subcode: edit.owner().subcode,
                },
                row: edit.position().row(),
                column: edit.position().column(),
                expected: edit.expected(),
                replacement: edit.replacement(),
            })?;
        }
        sequence.end()
    }
}

impl TeletextSavedEditorialOverlay {
    /// Capture V23 exact original Unicode Base text edits independently of Local.
    /// Saved original producer responses are unchanged. This grants no receipt,
    /// region permission, native equivalence, concept or publication authority.
    /// # Errors
    /// Rejects invalid provenance, empty batches or repeated exact cell targets.
    pub fn new_with_base_text_edits(
        provenance: TeletextSavedEditorialProvenance,
        edits: Vec<TeletextBaseTextEdit>,
    ) -> Result<Self, TeletextDesignDocumentError> {
        Self::new_parts(provenance, vec![], vec![], vec![], vec![])?.with_base_text_edits(edits)
    }
    /// Compose Base Text edits with already checked sibling editorial roles.
    /// Exact original scalar/owner association is checked when attaching/restoring.
    /// # Errors
    /// Rejects empty batches and repeated targets, including prior Base edits.
    pub fn with_base_text_edits(
        mut self,
        edits: Vec<TeletextBaseTextEdit>,
    ) -> Result<Self, TeletextDesignDocumentError> {
        if edits.is_empty() {
            return Err(invalid());
        }
        let mut targets = BTreeSet::new();
        for edit in self.base_text_edits.iter().chain(&edits) {
            if !targets.insert((edit.owner(), edit.position())) {
                return Err(invalid());
            }
        }
        self.base_text_edits.extend(edits);
        self.schema_version = super::BASE_TEXT_EDIT_VERSION;
        Ok(self)
    }
    /// Private-original intent; not raw X26 words or upload edit permission.
    #[must_use]
    pub fn base_text_edits(&self) -> &[TeletextBaseTextEdit] {
        &self.base_text_edits
    }
}

pub(super) fn check_version(value: &StoredOverlay) -> Result<(), TeletextDesignDocumentError> {
    if value.schema_version != super::BASE_TEXT_EDIT_VERSION {
        return if value.base_text_edits.is_absent() {
            Ok(())
        } else {
            Err(invalid())
        };
    }
    if value.base_text_edits.0.as_ref().is_none_or(Vec::is_empty)
        || [
            value
                .local_program_operands
                .0
                .as_ref()
                .is_some_and(Vec::is_empty),
            value
                .local_object_operands
                .0
                .as_ref()
                .is_some_and(Vec::is_empty),
            value
                .drcs_bitmap_overlays
                .0
                .as_ref()
                .is_some_and(Vec::is_empty),
            value
                .local_object_glyph_program_overlays
                .0
                .as_ref()
                .is_some_and(Vec::is_empty),
            value
                .main_local_invocation_program_overlays
                .0
                .as_ref()
                .is_some_and(Vec::is_empty),
            value
                .resource_object_operands
                .0
                .as_ref()
                .is_some_and(Vec::is_empty),
            value
                .resource_object_program_overlays
                .0
                .as_ref()
                .is_some_and(Vec::is_empty),
            value
                .resource_object_definition_additions
                .0
                .as_ref()
                .is_some_and(Vec::is_empty),
        ]
        .into_iter()
        .any(|empty| empty)
    {
        return Err(invalid());
    }
    Ok(())
}
pub(super) fn restore(
    mut stored: StoredOverlay,
    documents: &[TeletextProfileDocument],
    budget: TeletextDesignDocumentBudget,
) -> Result<TeletextSavedEditorialOverlay, TeletextDesignDocumentError> {
    let edits = stored
        .base_text_edits
        .0
        .take()
        .ok_or_else(invalid)?
        .into_iter()
        .map(|edit| {
            TeletextBaseTextEdit::new(
                TeletextPageAddress {
                    page: edit.owner.page,
                    subcode: edit.owner.subcode,
                },
                TeletextEnhancementPosition::new(edit.row, edit.column).map_err(|_| invalid())?,
                edit.expected,
                edit.replacement,
            )
            .map_err(|_| invalid())
        })
        .collect::<Result<Vec<_>, _>>()?;
    let siblings = if stored.main_local_invocation_program_overlays.is_absent() {
        super::local_object_invocation_programs::restore_siblings(stored, documents, budget)?
    } else {
        super::main_local_invocation_programs::restore(stored, documents, budget)?
    };
    let value = siblings.with_base_text_edits(edits)?;
    super::validate_association(&value, documents, budget)?;
    Ok(value)
}

pub(super) fn validate_association(
    overlay: &TeletextSavedEditorialOverlay,
    documents: &[TeletextProfileDocument],
    budget: TeletextDesignDocumentBudget,
) -> Result<(), TeletextDesignDocumentError> {
    limit(
        overlay.base_text_edits.len(),
        budget.validation.service.max_commands,
        "editorial operands",
    )?;
    let mut targets = BTreeSet::new();
    for edit in &overlay.base_text_edits {
        if !targets.insert((edit.owner(), edit.position())) {
            return Err(invalid());
        }
        let document = super::unique_original_owner(edit.owner(), documents)?;
        let base = match document {
            TeletextProfileDocument::Base(base) => base,
            TeletextProfileDocument::Authored(service) => service.semantic().base(),
        };
        let character = base
            .pages
            .iter()
            .find(|page| page.address == edit.owner())
            .and_then(|page| {
                page.rows
                    .iter()
                    .find(|row| row.row == edit.position().row())
            })
            .and_then(|row| {
                row.runs.iter().find_map(|run| match &run.content {
                    TeletextRunContent::Text { text } => edit
                        .position()
                        .column()
                        .checked_sub(run.column)
                        .and_then(|offset| text.chars().nth(usize::from(offset))),
                    TeletextRunContent::Mosaics { .. } => None,
                })
            });
        if character != Some(edit.expected()) {
            return Err(invalid());
        }
    }
    Ok(())
}

pub(super) fn content_bytes(
    overlay: &TeletextSavedEditorialOverlay,
    budget: TeletextDesignDocumentBudget,
) -> Result<usize, TeletextDesignDocumentError> {
    let bytes = overlay
        .base_text_edits
        .iter()
        .try_fold(0_usize, |sum, edit| {
            sum.checked_add(edit.expected().len_utf8())
                .and_then(|sum| sum.checked_add(edit.replacement().len_utf8()))
        })
        .ok_or_else(invalid)?;
    limit(
        bytes,
        budget.validation.service.max_base_content_bytes,
        "editorial content bytes",
    )?;
    Ok(bytes)
}
