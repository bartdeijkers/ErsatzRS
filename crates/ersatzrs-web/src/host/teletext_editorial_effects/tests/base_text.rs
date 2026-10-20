//! Base text intent passes the same complete protected-region checks.
use super::*;

fn edit() -> TeletextBaseTextEdit {
    TeletextBaseTextEdit::new(
        address(123),
        TeletextEnhancementPosition::new(1, 8).unwrap(),
        '£',
        '¥',
    )
    .unwrap()
}

#[test]
fn generated_base_text_respects_free_zone_generated_and_undeclared_boundaries() {
    let original = encode(draft("£"), vec![]);
    let mut changed = draft("£");
    changed.pages[0].rows[0].runs[0].content = TeletextRunContent::Text { text: "¥".into() };
    let edited = encode(changed, vec![]);
    let base_edits = [edit()];
    let check = |kind, column, width, intents: &[TeletextBaseTextEdit]| {
        validate_teletext_editorial_effects_with_operands(
            service(&original),
            service(&edited),
            &project(kind, 1, column, width, 1),
            TeletextEditorialEdits {
                base_text_edits: intents,
                rows: &[],
                operands: &[],
                object_operands: &[],
                resource_operands: &[],
                resource_programs: &[],
                resource_additions: &[],
                object_resource_additions: &[],
                object_subpage_announcements: &[],
                object_page_selections: &[],
                object_programs: &[],
                main_programs: &[],
                bitmaps: &[],
            },
            &policy().encoding,
            deadline(),
        )
    };
    for kind in ["free", "zone"] {
        check(kind, 8, 1, &base_edits).unwrap();
    }
    for (kind, column, width) in [("generated", 8, 1), ("free", 9, 1), ("zone", 9, 1)] {
        let rejected = check(kind, column, width, &base_edits).unwrap_err();
        assert_eq!(rejected.page, Some(address(123)));
        assert_eq!(rejected.position, Some((1, 8)));
        // Latin £ has a physical G0 byte; ¥ uses G2 with a space fallback.
        // The existing byte guard rejects before semantic/display comparison.
        assert_eq!(
            rejected.reason,
            TeletextEditorialEffectsFailure::ProtectedByte
        );
    }
    assert_eq!(
        check("free", 8, 1, &[]).unwrap_err().reason,
        TeletextEditorialEffectsFailure::ProtectedStructure
    );
    let forged = [TeletextBaseTextEdit::new(
        address(123),
        TeletextEnhancementPosition::new(1, 8).unwrap(),
        'Z',
        '¥',
    )
    .unwrap()];
    assert_eq!(
        check("free", 8, 1, &forged).unwrap_err().reason,
        TeletextEditorialEffectsFailure::Association
    );
}

#[test]
fn generated_base_native_spelling_delta_refuses_unclaimed_or_unrelated_changes() {
    let original = encode(draft("£"), vec![]);
    let before = service(&original).declared().semantic();
    let plain = TeletextBaseTextEdit::new(
        address(123),
        TeletextEnhancementPosition::new(1, 8).unwrap(),
        '£',
        '$',
    )
    .unwrap();
    let hint = TeletextBaseTextSpelling::new(
        plain.owner(),
        plain.position(),
        '$',
        TeletextGlyphCode::new(b' ').unwrap(),
        TeletextEnhancementCharsetDesignation::new(0).unwrap(),
        TeletextColumnOperation::G2 {
            code: TeletextGlyphCode::new(0x24).unwrap(),
        },
    )
    .unwrap();
    let incoming = plain.with_native_spelling(hint).unwrap();
    let after = before
        .clone()
        .with_base_text_edits(&[incoming], policy().validation.service)
        .unwrap();
    super::super::base_text::check_spellings(before, &after, &[incoming], deadline()).unwrap();
    for missing in [vec![], vec![plain]] {
        assert_eq!(
            super::super::base_text::check_spellings(before, &after, &missing, deadline())
                .unwrap_err()
                .reason,
            TeletextEditorialEffectsFailure::ProtectedStructure
        );
    }
    let canonical = before
        .clone()
        .with_base_text_edits(&[plain], policy().validation.service)
        .unwrap();
    assert!(
        super::super::base_text::check_spellings(before, &canonical, &[incoming], deadline())
            .is_err()
    );
    assert!(super::super::base_text::check_spellings(&after, &canonical, &[], deadline()).is_err());
    let next = TeletextBaseTextEdit::new(plain.owner(), plain.position(), '$', '¥').unwrap();
    let cleared = after
        .clone()
        .with_base_text_edits(&[next], policy().validation.service)
        .unwrap();
    super::super::base_text::check_spellings(&after, &cleared, &[next], deadline()).unwrap();
}
