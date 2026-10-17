//! Structural Local styles retain every nonstyle execution anchor.
use super::*;

fn styles() -> Vec<TeletextEnhancementCommand> {
    vec![
        foreground(1, 2),
        column(
            1,
            TeletextColumnOperation::Background {
                colour: TeletextColourReference::new(3).unwrap(),
            },
        ),
        column(
            1,
            TeletextColumnOperation::Attributes {
                double_height: true,
                double_width: true,
                boxed: true,
                conceal: true,
                invert: true,
                underline_or_separated_mosaics: true,
            },
        ),
    ]
}

#[test]
fn local_style_insert_delete_keeps_legacy_factories_closed_and_original_ordinals() {
    for style in styles() {
        let original = program(vec![g0(0, b'A'), g1(2, 1)]);
        let replacement = program(vec![g0(0, b'A'), style, g1(2, 1)]);
        let map = vec![Some(0), None, Some(1)];
        let change = TeletextLocalObjectGlyphProgramOverlay::new_with_style_commands(
            target(),
            original.clone(),
            replacement.clone(),
            map.clone(),
        )
        .unwrap();
        assert!(!change.is_legacy_compatible());
        assert!(!change.is_all_glyphs_compatible());
        assert_eq!(change.original_command(2), Some(1));
        assert_eq!(change.replacement_command(1), Some(2));
        for factory in [
            TeletextLocalObjectGlyphProgramOverlay::new,
            TeletextLocalObjectGlyphProgramOverlay::new_with_all_glyphs,
        ] {
            assert_eq!(
                factory(target(), original.clone(), replacement.clone(), map.clone()),
                Err(TeletextLocalObjectGlyphProgramOverlayError::ProtectedCommand)
            );
        }
        let undo = TeletextLocalObjectGlyphProgramOverlay::new_with_style_commands(
            target(),
            replacement,
            original.clone(),
            vec![Some(0), Some(2)],
        )
        .unwrap();
        assert_eq!(undo.replacement(), &original);
    }
}

#[test]
fn local_style_retained_operands_and_columns_remain_exact_and_context_cannot_disappear() {
    use TeletextLocalObjectGlyphProgramOverlayError as Error;
    for style in styles() {
        let mut moved = style;
        let TeletextEnhancementCommand::AtColumn { column, .. } = &mut moved else {
            unreachable!()
        };
        *column = 2;
        assert_eq!(
            TeletextLocalObjectGlyphProgramOverlay::new_with_style_commands(
                target(),
                program(vec![style, g0(2, b'A')]),
                program(vec![moved, g0(3, b'B')]),
                vec![Some(0), Some(1)],
            ),
            Err(Error::ProtectedCommand)
        );
    }
    for anchor in protected_anchors() {
        if matches!(anchor, TeletextEnhancementCommand::AtColumn { operation, .. }
            if operation.glyph_family().is_some() || matches!(operation,
                TeletextColumnOperation::Foreground { .. } | TeletextColumnOperation::Background { .. }
                | TeletextColumnOperation::Attributes { .. }))
        {
            continue;
        }
        let original = program(vec![g0(0, b'A'), anchor, g0(2, b'B')]);
        for (replacement, map) in [
            (vec![g0(0, b'A'), g0(2, b'B')], vec![Some(0), Some(2)]),
            (
                vec![g0(0, b'A'), anchor, anchor, g0(2, b'B')],
                vec![Some(0), Some(1), None, Some(2)],
            ),
        ] {
            assert_eq!(
                TeletextLocalObjectGlyphProgramOverlay::new_with_style_commands(
                    target(),
                    original.clone(),
                    program(replacement),
                    map,
                ),
                Err(Error::ProtectedCommand)
            );
        }
    }
}

#[test]
fn local_style_preserves_glyph_family_and_command_capacity_checks() {
    use TeletextLocalObjectGlyphProgramOverlayError as Error;
    let old = program(vec![g0(0, b'A')]);
    assert_eq!(
        TeletextLocalObjectGlyphProgramOverlay::new_with_style_commands(
            target(),
            old.clone(),
            program(vec![g1(0, 1), foreground(1, 2)]),
            vec![Some(0), None],
        ),
        Err(Error::ProtectedCommand)
    );
    assert_eq!(
        TeletextLocalObjectGlyphProgramOverlay::new_with_style_commands(
            target(),
            old.clone(),
            program(vec![foreground(1, 2); 209]),
            vec![None; 209],
        ),
        Err(Error::Capacity)
    );
    let all_glyph = TeletextLocalObjectGlyphProgramOverlay::new_with_style_commands(
        target(),
        old,
        program(vec![g0(1, b'B'), g1(2, 1)]),
        vec![Some(0), None],
    )
    .unwrap();
    assert!(all_glyph.is_legacy_compatible());
    assert!(all_glyph.is_all_glyphs_compatible());
}
