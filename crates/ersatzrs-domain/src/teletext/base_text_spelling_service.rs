//! Bounded explicit Base Text glyph spelling; character encoding stays compiler-owned.
use super::{charge, TeletextSemanticService, TeletextServiceBudget, TeletextServiceError};
use crate::teletext::{
    TeletextBaseTextSpelling, TeletextBaseTextSpellingError, TeletextEnhancementPosition,
    TeletextPageAddress, TeletextRunContent,
};
use std::collections::BTreeMap;

type Spellings =
    BTreeMap<(TeletextPageAddress, TeletextEnhancementPosition), TeletextBaseTextSpelling>;

impl TeletextSemanticService {
    /// Replace explicit native Text spellings while retaining all semantic and
    /// physical siblings. Empty input clears the set. There is no decoding,
    /// editor permission, publication authority or automatic package action here.
    /// # Errors
    /// Rejects raw counts, aggregate content/structure budgets, duplicate targets
    /// and cells that do not have the exact declared logical Text scalar.
    pub fn with_base_text_spellings(
        mut self,
        spellings: Vec<TeletextBaseTextSpelling>,
        budget: TeletextServiceBudget,
    ) -> Result<Self, TeletextServiceError> {
        self.base_text_spellings.clear();
        self.check_existing_service_budgets(budget)?;
        self.check_base_spelling_budgets(spellings.iter(), budget)?;
        let mut pending = BTreeMap::new();
        for spelling in spellings {
            let key = (
                spelling.owner(),
                spelling.position().row(),
                spelling.position().column(),
            );
            if pending.insert(key, spelling).is_some() {
                return Err(failure(spelling, TeletextBaseTextSpellingError::Duplicate));
            }
        }
        let mut indexed = Spellings::new();
        for page in &self.base.pages {
            for row in &page.rows {
                for run in &row.runs {
                    let TeletextRunContent::Text { text } = &run.content else {
                        continue;
                    };
                    for (column, character) in (run.column..).zip(text.chars()) {
                        if let Some(spelling) = pending.remove(&(page.address, row.row, column)) {
                            if spelling.character() != character {
                                return Err(failure(
                                    spelling,
                                    TeletextBaseTextSpellingError::Target,
                                ));
                            }
                            indexed.insert((spelling.owner(), spelling.position()), spelling);
                        }
                    }
                }
            }
        }
        if let Some(spelling) = pending.into_values().next() {
            return Err(failure(spelling, TeletextBaseTextSpellingError::Target));
        }
        self.base_text_spellings = indexed;
        Ok(self)
    }
    #[must_use]
    pub fn base_text_spellings(&self) -> &Spellings {
        &self.base_text_spellings
    }

    /// Borrow exactly one owner without scanning other pages' spelling records.
    pub fn base_text_spellings_for(
        &self,
        owner: TeletextPageAddress,
    ) -> impl Iterator<Item = &TeletextBaseTextSpelling> {
        self.base_text_spellings
            .range(
                (owner, TeletextEnhancementPosition::FIRST)
                    ..=(owner, TeletextEnhancementPosition::LAST),
            )
            .map(|(_, value)| value)
    }

    pub(super) fn check_base_spelling_budgets<'a>(
        &self,
        spellings: impl ExactSizeIterator<Item = &'a TeletextBaseTextSpelling>,
        budget: TeletextServiceBudget,
    ) -> Result<(), TeletextServiceError> {
        // A spelling is bounded command input as well as owned typed storage.
        let mut commands = budget.max_commands;
        for page in self.pages.values() {
            charge(
                &mut commands,
                page.local_program
                    .as_ref()
                    .map_or(0, |program| program.commands().len()),
                "authored command",
                budget.max_commands,
            )?;
            charge(
                &mut commands,
                page.resources.defaults.iter().flatten().count(),
                "authored command",
                budget.max_commands,
            )?;
        }
        for object in self.objects.definitions() {
            charge(
                &mut commands,
                object.program.commands().len(),
                "authored command",
                budget.max_commands,
            )?;
        }
        charge(
            &mut commands,
            spellings.len(),
            "authored command",
            budget.max_commands,
        )?;
        let mut items = budget.max_structure_items;
        let mut bytes = budget.max_base_content_bytes;
        charge(
            &mut items,
            self.original_structure_items,
            "structure item",
            budget.max_structure_items,
        )?;
        charge(
            &mut bytes,
            self.original_base_content_bytes,
            "base content byte",
            budget.max_base_content_bytes,
        )?;
        charge(
            &mut items,
            self.physical_base_overlays.len(),
            "structure item",
            budget.max_structure_items,
        )?;
        for overlay in self.physical_base_overlays.values() {
            charge(
                &mut items,
                overlay.rows().len(),
                "structure item",
                budget.max_structure_items,
            )?;
            for _ in overlay.rows() {
                charge(
                    &mut bytes,
                    40,
                    "base content byte",
                    budget.max_base_content_bytes,
                )?;
            }
        }
        charge(
            &mut items,
            spellings.len(),
            "structure item",
            budget.max_structure_items,
        )?;
        for spelling in spellings {
            // Character plus the four retained native numeric fields.
            charge(
                &mut bytes,
                spelling.character().len_utf8() + 4,
                "base content byte",
                budget.max_base_content_bytes,
            )?;
        }
        Ok(())
    }
}
fn failure(
    spelling: TeletextBaseTextSpelling,
    reason: TeletextBaseTextSpellingError,
) -> TeletextServiceError {
    TeletextServiceError::BaseTextSpelling {
        owner: spelling.owner(),
        position: spelling.position(),
        reason,
    }
}
