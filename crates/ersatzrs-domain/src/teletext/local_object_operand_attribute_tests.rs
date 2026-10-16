//! Same-family local object operands; no host permission or rendering claim.
use super::*;

#[test]
fn local_object_colours_and_attributes_preserve_sized_originals_and_complete_service() {
    let owner = address(200, 0);
    let identity = target(owner);
    for (double_height, double_width) in
        [(false, false), (true, false), (false, true), (true, true)]
    {
        let input = attribute_input(double_height, double_width);
        let original = service(input.clone());
        let changes = [
            change(identity, 0, 0, foreground(18), foreground(31)),
            change(identity, 1, 0, background(0), background(16)),
            change(
                identity,
                5,
                0,
                attributes(double_height, double_width, 0),
                attributes(double_height, double_width, 15),
            ),
        ];
        let edited = original
            .clone()
            .with_local_object_operand_overlays(&changes, budget())
            .unwrap();
        let mut expected = input;
        let mut commands = expected.objects[0].program.commands().to_vec();
        commands[0] = TeletextEnhancementCommand::AtColumn {
            column: 0,
            operation: foreground(31),
        };
        commands[1] = TeletextEnhancementCommand::AtColumn {
            column: 0,
            operation: background(16),
        };
        commands[5] = TeletextEnhancementCommand::AtColumn {
            column: 0,
            operation: attributes(double_height, double_width, 15),
        };
        expected.objects[0].program = program(commands);
        // Includes original MOT fallback, physical rows, page/magazine defaults,
        // object identity/type/order, unrelated POP/DRCS and base/nav inputs.
        assert_eq!(edited, service(expected));
        assert_ne!(edited, original);
        assert_eq!(edited.collected(owner), original.collected(owner));
        assert_eq!(
            edited.effective_program(address(200, 1)).unwrap(),
            original.effective_program(address(200, 1)).unwrap()
        );
    }
}

#[test]
fn local_object_attributes_delegate_all_boolean_and_original_size_combinations() {
    let identity = target(address(200, 0));
    for (double_height, double_width) in
        [(false, false), (true, false), (false, true), (true, true)]
    {
        for mask in 1..=15 {
            let expected = attributes(double_height, double_width, 0);
            let replacement = attributes(double_height, double_width, mask);
            let overlay = change(identity, 5, 39, expected, replacement);
            assert_eq!(
                (
                    overlay.identity(),
                    overlay.owner(),
                    overlay.command(),
                    overlay.column()
                ),
                (identity, address(200, 0), 5, 39)
            );
            assert_eq!(overlay.expected(), expected);
            assert_eq!(overlay.replacement(), replacement);
            let cleared = change(identity, 5, 39, replacement, expected);
            assert_eq!(cleared.expected(), replacement);
            assert_eq!(cleared.replacement(), expected);
        }
        for (new_height, new_width) in [(false, false), (true, false), (false, true), (true, true)]
        {
            if (double_height, double_width) == (new_height, new_width) {
                continue;
            }
            assert_eq!(
                TeletextLocalObjectOperandOverlay::new(
                    identity,
                    5,
                    0,
                    attributes(double_height, double_width, 0),
                    attributes(new_height, new_width, 15)
                ),
                Err(TeletextLocalObjectOperandOverlayError::Operand(
                    TeletextLocalProgramOperandOverlayError::Geometry
                ))
            );
        }
    }
}

#[test]
fn local_object_new_operand_families_reject_cross_family_noop_and_invalid_columns() {
    let identity = target(address(200, 0));
    let pairs = [
        (foreground(18), foreground(31)),
        (background(0), background(16)),
        (attributes(false, false, 0), attributes(false, false, 15)),
    ];
    for (expected, replacement) in pairs {
        assert!(
            TeletextLocalObjectOperandOverlay::new(identity, 0, 0, expected, replacement).is_ok()
        );
        assert_eq!(
            TeletextLocalObjectOperandOverlay::new(identity, 0, u8::MAX, expected, replacement),
            Err(TeletextLocalObjectOperandOverlayError::Operand(
                TeletextLocalProgramOperandOverlayError::Column { column: u8::MAX }
            ))
        );
        assert_eq!(
            TeletextLocalObjectOperandOverlay::new(identity, 0, 0, expected, expected),
            Err(TeletextLocalObjectOperandOverlayError::Operand(
                TeletextLocalProgramOperandOverlayError::Unchanged
            ))
        );
        for external in [public(0xa0, 0, 65520), global(0xa1, 65520)] {
            assert_eq!(
                TeletextLocalObjectOperandOverlay::new(external, 0, 0, expected, replacement),
                Err(TeletextLocalObjectOperandOverlayError::Scope)
            );
        }
    }
    for (expected, replacement) in [
        (foreground(18), background(16)),
        (background(0), attributes(false, false, 15)),
        (attributes(false, false, 0), g0(b'B')),
        (g1(0), foreground(31)),
    ] {
        assert_eq!(
            TeletextLocalObjectOperandOverlay::new(identity, 0, 0, expected, replacement),
            Err(TeletextLocalObjectOperandOverlayError::Operand(
                TeletextLocalProgramOperandOverlayError::Family
            ))
        );
    }
}

#[test]
fn local_object_colour_and_attribute_commands_remain_exact_original_and_unique() {
    let identity = target(address(200, 0));
    let original = service(attribute_input(false, false));
    for valid in [
        change(identity, 0, 0, foreground(18), foreground(31)),
        change(identity, 1, 0, background(0), background(16)),
        change(
            identity,
            5,
            0,
            attributes(false, false, 0),
            attributes(false, false, 15),
        ),
    ] {
        assert_eq!(
            original
                .clone()
                .with_local_object_operand_overlays(&[valid, valid], budget()),
            Err(TeletextServiceError::LocalObjectOperandOverlay {
                identity,
                command: valid.command(),
                reason: TeletextLocalObjectOperandFailure::Duplicate
            })
        );
        let edited = original
            .clone()
            .with_local_object_operand_overlays(&[valid], budget())
            .unwrap();
        assert_eq!(
            edited.with_local_object_operand_overlays(&[valid], budget()),
            Err(TeletextServiceError::LocalObjectOperandOverlay {
                identity,
                command: valid.command(),
                reason: TeletextLocalObjectOperandFailure::CommandMismatch
            })
        );
    }
    for invalid in [
        change(identity, 0, 0, foreground(17), foreground(31)),
        change(identity, 1, 1, background(0), background(16)),
        change(
            identity,
            5,
            0,
            attributes(false, false, 1),
            attributes(false, false, 15),
        ),
        change(identity, 2, 0, foreground(18), foreground(31)),
    ] {
        assert_eq!(
            original
                .clone()
                .with_local_object_operand_overlays(&[invalid], budget()),
            Err(TeletextServiceError::LocalObjectOperandOverlay {
                identity,
                command: invalid.command(),
                reason: TeletextLocalObjectOperandFailure::CommandMismatch
            })
        );
    }
}
