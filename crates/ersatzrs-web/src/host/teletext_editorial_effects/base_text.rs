//! Exact scalar delta, followed by the existing full protected-effect checks.
use super::{check_deadline, error, Result, TeletextEditorialEffectsFailure as Failure};
use ersatzrs_domain::{
    TeletextBaseTextEdit, TeletextPageSetDraft, TeletextRunContent, TeletextSemanticService,
};
use std::{collections::BTreeMap, time::Instant};

pub(super) fn check(
    before: &TeletextPageSetDraft,
    after: &TeletextPageSetDraft,
    edits: &[TeletextBaseTextEdit],
    deadline: Instant,
) -> Result<()> {
    let structure = || error(None, None, Failure::ProtectedStructure);
    if before.language != after.language
        || before.initial_page != after.initial_page
        || before.pages.len() != after.pages.len()
    {
        return Err(structure());
    }
    let mut targets = BTreeMap::new();
    for edit in edits {
        check_deadline(deadline)?;
        if targets
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
            return Err(error(
                Some(edit.owner()),
                Some((edit.position().row(), edit.position().column())),
                Failure::Association,
            ));
        }
    }
    for (old, new) in before.pages.iter().zip(&after.pages) {
        check_deadline(deadline)?;
        if old.address != new.address
            || old.kind != new.kind
            || old.language != new.language
            || old.national_option != new.national_option
            || old.navigation != new.navigation
            || old.rows.len() != new.rows.len()
        {
            return Err(structure());
        }
        for (row, new_row) in old.rows.iter().zip(&new.rows) {
            if row.row != new_row.row || row.runs.len() != new_row.runs.len() {
                return Err(structure());
            }
            for (run, new_run) in row.runs.iter().zip(&new_row.runs) {
                if run.column != new_run.column || run.style != new_run.style {
                    return Err(structure());
                }
                match (&run.content, &new_run.content) {
                    (
                        TeletextRunContent::Text { text },
                        TeletextRunContent::Text { text: next },
                    ) => {
                        if text.chars().count() != next.chars().count() {
                            return Err(structure());
                        }
                        for ((column, original), replacement) in
                            (run.column..).zip(text.chars()).zip(next.chars())
                        {
                            check_deadline(deadline)?;
                            if let Some(edit) = targets.remove(&(old.address, row.row, column)) {
                                if original != edit.expected() || replacement != edit.replacement()
                                {
                                    return Err(error(
                                        Some(old.address),
                                        Some((row.row, column)),
                                        Failure::Association,
                                    ));
                                }
                            } else if original != replacement {
                                return Err(structure());
                            }
                        }
                    }
                    (original, replacement) if original == replacement => {}
                    _ => return Err(structure()),
                }
            }
        }
    }
    if !targets.is_empty() {
        return Err(error(None, None, Failure::Association));
    }
    Ok(())
}

/// Match the complete spelling delta without cloning immutable native records.
pub(super) fn check_spellings(
    before: &TeletextSemanticService,
    after: &TeletextSemanticService,
    edits: &[TeletextBaseTextEdit],
    deadline: Instant,
) -> Result<()> {
    let failure = || error(None, None, Failure::ProtectedStructure);
    let mut targets = BTreeMap::new();
    for edit in edits {
        check_deadline(deadline)?;
        if targets
            .insert((edit.owner(), edit.position()), edit)
            .is_some()
        {
            return Err(failure());
        }
    }
    let mut expected = 0_usize;
    for (key, spelling) in before.base_text_spellings() {
        check_deadline(deadline)?;
        if !targets.contains_key(key) {
            if after.base_text_spellings().get(key) != Some(spelling) {
                return Err(failure());
            }
            expected += 1;
        }
    }
    for (key, edit) in targets {
        check_deadline(deadline)?;
        if after.base_text_spellings().get(&key).copied() != edit.native_spelling() {
            return Err(failure());
        }
        expected += usize::from(edit.native_spelling().is_some());
    }
    if after.base_text_spellings().len() != expected {
        return Err(failure());
    }
    Ok(())
}
