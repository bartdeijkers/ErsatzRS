//! Reconstruct derived Base text without mutating saved producer documents.

use super::{
    TeletextMagazinePresentationDraft, TeletextResolvedServiceDraft, TeletextRunContent,
    TeletextSemanticService, TeletextServiceBudget, TeletextServiceError,
};
use crate::teletext::{TeletextBaseTextEdit, TeletextBaseTextEditError};
use std::collections::BTreeMap;

impl TeletextSemanticService {
    /// Replace exact Unicode Text cells and rebuild the complete semantic service.
    /// All other rows, styles, navigation, resources, defaults, Local commands and
    /// physical row overlays are retained. Saved producer strings are owned by
    /// the caller's immutable original, not rewritten by this derived value.
    ///
    /// This establishes no compiler correspondence, edit permission, character
    /// representation, concept eligibility or publication authority.
    /// # Errors
    /// Rejects empty/duplicate/unbound edits, mismatched originals and exceeded
    /// input, final content, service or transitive collector budgets atomically.
    pub fn with_base_text_edits(
        self,
        edits: &[TeletextBaseTextEdit],
        budget: TeletextServiceBudget,
    ) -> Result<Self, TeletextServiceError> {
        self.check_local_operand_overlay_budgets(edits.len(), budget)?;
        if edits.is_empty() {
            return Err(TeletextServiceError::EmptyBaseTextEdits);
        }
        let mut indexed = index_edits(edits, self.original_base_content_bytes, budget)?;
        let edited_positions: std::collections::BTreeSet<_> = indexed.keys().copied().collect();
        let mut base = self.base;
        for page in &mut base.pages {
            for row in &mut page.rows {
                for run in &mut row.runs {
                    let TeletextRunContent::Text { text } = &mut run.content else {
                        continue;
                    };
                    if text.is_empty() {
                        continue;
                    }
                    let last = usize::from(run.column) + text.chars().count();
                    let Some(last) = last
                        .checked_sub(1)
                        .and_then(|column| u8::try_from(column).ok())
                    else {
                        continue;
                    };
                    if indexed
                        .range((page.address, row.row, run.column)..=(page.address, row.row, last))
                        .next()
                        .is_none()
                    {
                        continue;
                    }
                    let mut replacement = String::with_capacity(text.len());
                    for (column, character) in (run.column..).zip(text.chars()) {
                        if let Some(edit) = indexed.remove(&(page.address, row.row, column)) {
                            if edit.expected() != character {
                                return Err(base_text_failure(
                                    edit,
                                    TeletextBaseTextEditError::Expected,
                                ));
                            }
                            replacement.push(edit.replacement());
                        } else {
                            replacement.push(character);
                        }
                    }
                    *text = replacement;
                }
            }
        }
        if let Some(edit) = indexed.values().next() {
            return Err(base_text_failure(edit, TeletextBaseTextEditError::Target));
        }
        let draft = TeletextResolvedServiceDraft {
            base,
            pages: self.pages.into_values().collect(),
            magazines: self
                .magazines
                .into_iter()
                .map(
                    |(magazine, presentation)| TeletextMagazinePresentationDraft {
                        magazine,
                        presentation,
                    },
                )
                .collect(),
            objects: self.objects.definitions().to_vec(),
            drcs: self.drcs.subtables().to_vec(),
        };
        Self::new_with_resolved_mot_fallbacks(
            draft,
            self.object_companions,
            self.mot_fallbacks,
            budget,
        )?
        .with_physical_base_overlays(self.physical_base_overlays.into_values().collect(), budget)?
        .with_base_text_spellings(
            self.base_text_spellings
                .into_values()
                .filter(|spelling| {
                    !edited_positions.contains(&(
                        spelling.owner(),
                        spelling.position().row(),
                        spelling.position().column(),
                    ))
                })
                .chain(edits.iter().filter_map(|edit| edit.native_spelling()))
                .collect(),
            budget,
        )
    }
}

fn base_text_failure(
    edit: &TeletextBaseTextEdit,
    reason: TeletextBaseTextEditError,
) -> TeletextServiceError {
    TeletextServiceError::BaseTextEdit {
        owner: edit.owner(),
        position: edit.position(),
        reason,
    }
}

fn index_edits(
    edits: &[TeletextBaseTextEdit],
    original_content_bytes: usize,
    budget: TeletextServiceBudget,
) -> Result<
    BTreeMap<(super::TeletextPageAddress, u8, u8), &TeletextBaseTextEdit>,
    TeletextServiceError,
> {
    let mut indexed = BTreeMap::new();
    let mut content_bytes = original_content_bytes;
    let content_budget = || TeletextServiceError::Budget {
        dimension: "base content bytes",
        maximum: budget.max_base_content_bytes,
    };
    for edit in edits {
        if indexed
            .insert(
                (
                    edit.owner(),
                    edit.position().row(),
                    edit.position().column(),
                ),
                edit,
            )
            .is_some()
        {
            return Err(base_text_failure(
                edit,
                TeletextBaseTextEditError::Duplicate,
            ));
        }
        content_bytes = content_bytes
            .checked_sub(edit.expected().len_utf8())
            .and_then(|value| value.checked_add(edit.replacement().len_utf8()))
            .ok_or_else(content_budget)?;
    }
    if content_bytes > budget.max_base_content_bytes {
        return Err(content_budget());
    }
    Ok(indexed)
}
