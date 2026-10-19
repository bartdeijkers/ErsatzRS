//! Exact spelling metadata cannot mutate logical content or gain Local identity.
use super::*;
fn spelling(sub: u16, column: u8, character: char) -> TeletextBaseTextSpelling {
    TeletextBaseTextSpelling::new(
        owner(sub),
        TeletextEnhancementPosition::new(1, column).unwrap(),
        character,
        TeletextGlyphCode::new(b' ').unwrap(),
        TeletextEnhancementCharsetDesignation::new(0).unwrap(),
        TeletextColumnOperation::G2 {
            code: TeletextGlyphCode::new(0x23).unwrap(),
        },
    )
    .unwrap()
}
#[test]
fn base_spelling_shape_is_closed_and_debug_does_not_expose_content() {
    let original = spelling(0, 4, '£');
    assert!(!format!("{original:?}").contains('£'));
    assert!(TeletextBaseTextSpelling::new(
        owner(0),
        TeletextEnhancementPosition::new(0, 7).unwrap(),
        'X',
        original.fallback(),
        original.charset(),
        original.operation()
    )
    .is_err());
    assert!(TeletextBaseTextSpelling::new(
        owner(0),
        original.position(),
        '\n',
        original.fallback(),
        original.charset(),
        original.operation()
    )
    .is_err());
    assert!(TeletextBaseTextSpelling::new(
        owner(0),
        original.position(),
        '£',
        original.fallback(),
        original.charset(),
        TeletextColumnOperation::Foreground {
            colour: TeletextColourReference::new(1).unwrap()
        }
    )
    .is_err());
}
#[test]
fn base_spellings_preserve_siblings_and_only_exact_text_cells_can_bind() {
    let original = input();
    let hint = spelling(0, 4, '£');
    let before = original.clone();
    let changed = original
        .with_base_text_spellings(vec![hint], limits())
        .unwrap();
    assert_eq!(changed.base(), before.base());
    assert_eq!(changed.objects(), before.objects());
    assert_eq!(changed.page_enhancements(), before.page_enhancements());
    assert_eq!(
        changed
            .base_text_spellings_for(owner(0))
            .copied()
            .collect::<Vec<_>>(),
        [hint]
    );
    assert_eq!(changed.base_text_spellings_for(owner(32)).count(), 0);
    for invalid in [
        vec![hint, hint],
        vec![spelling(64, 4, '£')],
        vec![spelling(0, 5, '£')],
        vec![spelling(0, 10, '£')],
        vec![spelling(0, 12, '£')],
    ] {
        assert!(before
            .clone()
            .with_base_text_spellings(invalid, limits())
            .is_err());
    }
    let rebuilt = changed
        .clone()
        .with_base_text_edits(&[change(32, 6, 'D', 'E')], limits())
        .unwrap();
    assert_eq!(rebuilt.base_text_spellings(), changed.base_text_spellings());
    let reset = changed
        .with_base_text_edits(&[change(0, 4, '£', '$')], limits())
        .unwrap();
    assert!(reset.base_text_spellings().is_empty());
}
#[test]
fn base_spelling_bounds_include_original_physical_rows_and_reduced_service_budgets() {
    let hint = spelling(0, 4, '£');
    let original = input();
    let original_commands: usize = original
        .page_enhancements()
        .values()
        .map(|page| {
            page.local_program
                .as_ref()
                .map_or(0, |program| program.commands().len())
                + page.resources.defaults.iter().flatten().count()
        })
        .sum::<usize>()
        + original
            .objects()
            .definitions()
            .iter()
            .map(|object| object.program.commands().len())
            .sum::<usize>();
    let mut aggregate = limits();
    aggregate.max_commands = original_commands;
    assert!(original
        .clone()
        .with_base_text_spellings(vec![hint], aggregate)
        .is_err());
    aggregate.max_commands += 1;
    assert!(original
        .with_base_text_spellings(vec![hint], aggregate)
        .is_ok());
    let mut budget = limits();
    budget.max_commands = 0;
    assert!(input()
        .with_base_text_spellings(vec![hint], budget)
        .is_err());
    let mut budget = limits();
    budget.max_records = 0;
    assert!(input()
        .with_base_text_spellings(vec![hint], budget)
        .is_err());
    // Ten UTF8 bytes and two mosaics are the unchanged original content.
    let mut budget = limits();
    budget.max_base_content_bytes = 17;
    assert!(input()
        .with_base_text_spellings(vec![hint], budget)
        .is_err());
    budget.max_base_content_bytes = 18;
    let typed = input()
        .with_base_text_spellings(vec![hint], budget)
        .unwrap();
    budget.max_base_content_bytes = 12;
    assert!(typed
        .clone()
        .with_base_text_spellings(vec![], budget)
        .unwrap()
        .base_text_spellings()
        .is_empty());
    let rows = vec![TeletextPhysicalBaseOverlay::new(
        owner(0),
        vec![TeletextPhysicalBaseRow::new(2, [b' '; 40]).unwrap()],
    )
    .unwrap()];
    budget.max_base_content_bytes = 52;
    assert!(typed
        .clone()
        .with_physical_base_overlays(rows.clone(), budget)
        .is_err());
    budget.max_base_content_bytes = 58;
    assert!(typed.with_physical_base_overlays(rows, budget).is_ok());
}

#[test]
fn base_edit_native_spelling_binds_and_rebuilds_exact_replacement_only() {
    let hint = TeletextBaseTextSpelling::new(
        owner(0),
        TeletextEnhancementPosition::new(1, 4).unwrap(),
        '$',
        TeletextGlyphCode::new(b' ').unwrap(),
        TeletextEnhancementCharsetDesignation::new(0).unwrap(),
        TeletextColumnOperation::G2 {
            code: TeletextGlyphCode::new(0x24).unwrap(),
        },
    )
    .unwrap();
    let edit = change(0, 4, '£', '$').with_native_spelling(hint).unwrap();
    for wrong in [
        change(32, 4, '£', '$'),
        change(0, 5, 'Ł', '$'),
        change(0, 4, '£', '¥'),
    ] {
        assert!(wrong.with_native_spelling(hint).is_err());
    }
    let original = input();
    let before = original.clone();
    let changed = original.with_base_text_edits(&[edit], limits()).unwrap();
    assert_eq!(
        changed
            .base_text_spellings_for(owner(0))
            .copied()
            .collect::<Vec<_>>(),
        [hint]
    );
    assert_eq!(
        changed.base().pages[0].rows[0].runs[0].content,
        TeletextRunContent::Text {
            text: "$ŁD".into()
        }
    );
    assert_eq!(
        before.base().pages[0].rows[0].runs[0].content,
        TeletextRunContent::Text {
            text: "£ŁD".into()
        }
    );
    assert!(before.base_text_spellings().is_empty());
    assert_eq!(changed.base().pages[1], before.base().pages[1]);
}
