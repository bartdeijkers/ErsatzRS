//! Exact latest-scalar CAS and trusted root undo; no serialized inverse edits.
use super::{check_deadline, NativeTeletextEditorialImportError, Result};
use ersatzrs_domain::{TeletextBaseTextEdit, TeletextServiceBudget};
use std::{
    collections::{BTreeMap, BTreeSet},
    time::Instant,
};

pub(super) fn merge(
    previous: &[TeletextBaseTextEdit],
    incoming: &[TeletextBaseTextEdit],
    budget: TeletextServiceBudget,
    deadline: Instant,
) -> Result<Vec<TeletextBaseTextEdit>> {
    let maximum = budget.max_commands.min(budget.max_structure_items);
    if previous.len() > maximum || incoming.len() > maximum {
        return Err(NativeTeletextEditorialImportError::Budget);
    }
    let mut result = BTreeMap::new();
    for edit in previous {
        check_deadline(deadline)?;
        if result
            .insert((edit.owner(), edit.position()), *edit)
            .is_some()
        {
            return Err(NativeTeletextEditorialImportError::Association);
        }
    }
    let mut seen = BTreeSet::new();
    for edit in incoming {
        check_deadline(deadline)?;
        let key = (edit.owner(), edit.position());
        if !seen.insert(key) {
            return Err(NativeTeletextEditorialImportError::Association);
        }
        if let Some(before) = result.remove(&key) {
            if before.replacement() != edit.expected() {
                return Err(NativeTeletextEditorialImportError::Association);
            }
            if before.expected() != edit.replacement() {
                let mut rebased = TeletextBaseTextEdit::new(
                    edit.owner(),
                    edit.position(),
                    before.expected(),
                    edit.replacement(),
                )
                .map_err(|_| NativeTeletextEditorialImportError::Association)?;
                if let Some(spelling) = edit.native_spelling() {
                    rebased = rebased
                        .with_native_spelling(spelling)
                        .map_err(|_| NativeTeletextEditorialImportError::Association)?;
                }
                result.insert(key, rebased);
            }
        } else {
            if result.len() == maximum {
                return Err(NativeTeletextEditorialImportError::Budget);
            }
            result.insert(key, *edit);
        }
    }
    Ok(result.into_values().collect())
}

#[cfg(test)]
mod tests {
    use super::*;
    use ersatzrs_domain::{
        TeletextEnhancementPosition, TeletextPageAddress, TeletextPageNumber, TeletextSubcode,
    };
    use std::time::Duration;
    pub(super) fn edit(expected: char, replacement: char) -> TeletextBaseTextEdit {
        TeletextBaseTextEdit::new(
            TeletextPageAddress {
                page: TeletextPageNumber::new(124).unwrap(),
                subcode: TeletextSubcode::new(32).unwrap(),
            },
            TeletextEnhancementPosition::new(3, 8).unwrap(),
            expected,
            replacement,
        )
        .unwrap()
    }
    pub(super) fn budget() -> TeletextServiceBudget {
        TeletextServiceBudget {
            max_records: 32,
            max_objects: 16,
            max_structure_items: 4096,
            max_base_content_bytes: 1024,
            max_commands: 128,
            max_collected_commands: 128,
            max_collected_drcs_references: 24,
        }
    }
    fn end() -> Instant {
        Instant::now() + Duration::from_secs(30)
    }
    #[test]
    fn base_text_repeat_uses_latest_scalar_and_true_undo_removes_root_intent() {
        let root = [edit('£', '¥')];
        let next = merge(&root, &[edit('¥', '§')], budget(), end()).unwrap();
        assert_eq!(next, [edit('£', '§')]);
        assert!(merge(&next, &[edit('§', '£')], budget(), end())
            .unwrap()
            .is_empty());
        assert!(matches!(
            merge(&root, &[edit('£', '§')], budget(), end()),
            Err(NativeTeletextEditorialImportError::Association)
        ));
        assert!(matches!(
            merge(&root, &[edit('¥', '§'), edit('¥', '¤')], budget(), end()),
            Err(NativeTeletextEditorialImportError::Association)
        ));
    }
    #[test]
    fn base_text_repeat_preserves_deadline_duplicate_and_aggregate_bounds() {
        let mut bounded = budget();
        bounded.max_commands = 0;
        assert!(matches!(
            merge(&[], &[edit('£', '¥')], bounded, end()),
            Err(NativeTeletextEditorialImportError::Budget)
        ));
        assert!(matches!(
            merge(&[], &[edit('£', '¥')], budget(), Instant::now()),
            Err(NativeTeletextEditorialImportError::TimedOut)
        ));
        assert!(matches!(
            merge(&[edit('£', '¥'), edit('£', '¥')], &[], budget(), end()),
            Err(NativeTeletextEditorialImportError::Association)
        ));
    }
}

#[cfg(test)]
mod native_spelling_tests {
    use super::*;
    use ersatzrs_domain::{
        TeletextBaseTextSpelling, TeletextColumnOperation, TeletextEnhancementCharsetDesignation,
        TeletextGlyphCode,
    };
    #[test]
    fn base_text_repeat_retains_native_spelling_and_root_identity_then_clears_on_canonical_edit() {
        let plain = super::tests::edit('¥', '$');
        let hint = TeletextBaseTextSpelling::new(
            plain.owner(),
            plain.position(),
            '$',
            TeletextGlyphCode::new(b' ').unwrap(),
            TeletextEnhancementCharsetDesignation::new(0x20).unwrap(),
            TeletextColumnOperation::G2 {
                code: TeletextGlyphCode::new(0x24).unwrap(),
            },
        )
        .unwrap();
        let native = plain.with_native_spelling(hint).unwrap();
        let end = Instant::now() + std::time::Duration::from_secs(30);
        let merged = merge(
            &[super::tests::edit('£', '¥')],
            &[native],
            super::tests::budget(),
            end,
        )
        .unwrap();
        assert_eq!(
            merged,
            [super::tests::edit('£', '$')
                .with_native_spelling(hint)
                .unwrap()]
        );
        let canonical = merge(
            &merged,
            &[super::tests::edit('$', '§')],
            super::tests::budget(),
            end,
        )
        .unwrap();
        assert_eq!(canonical, [super::tests::edit('£', '§')]);
        assert!(merge(
            &merged,
            &[super::tests::edit('$', '£')],
            super::tests::budget(),
            end
        )
        .unwrap()
        .is_empty());
    }
}
