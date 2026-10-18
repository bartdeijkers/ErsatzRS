//! Receipt mappings distinguish original-position intent from transient Add bodies.
use super::*;
use ersatzrs_domain::{
    TeletextEnhancementPosition, TeletextResourceObjectProgramOverlay as Program,
};

#[test]
fn original_position_authorization_requires_explicit_role_and_full_source_identity() {
    for global in [false, true] {
        for object_type in [
            TeletextObjectType::Passive,
            TeletextObjectType::Active,
            TeletextObjectType::Adaptive,
        ] {
            let original = compile(global, 2, b'A', object_type);
            let project = project(&original, global);
            let expected = original
                .declared()
                .semantic()
                .objects()
                .definition(identity(global))
                .unwrap()
                .program
                .clone();
            let mut commands = expected.commands().to_vec();
            commands.extend([
                TeletextEnhancementCommand::SetPosition {
                    position: TeletextEnhancementPosition::new(2, 0).unwrap(),
                },
                TeletextEnhancementCommand::AtColumn {
                    column: 10,
                    operation: glyph(b'B'),
                },
            ]);
            let replacement = TeletextEnhancementProgram::new(commands).unwrap();
            let mut retained = (0..expected.commands().len()).map(Some).collect::<Vec<_>>();
            retained.extend([None, None]);
            for allowed in [false, true] {
                let constructor = if allowed {
                    Program::new_with_original_positions
                } else {
                    Program::new_with_inserted_positions
                };
                let intent = constructor(
                    identity(global),
                    expected.clone(),
                    replacement.clone(),
                    retained.clone(),
                    bounds().2,
                )
                .unwrap();
                assert_eq!(
                    authorize_programs(&project, &original, &[], &[intent], bounds(), deadline())
                        .is_ok(),
                    allowed
                );
            }
            let wrong = Program::new_with_original_positions(
                identity(!global),
                expected,
                replacement,
                retained,
                bounds().2,
            )
            .unwrap();
            assert!(
                authorize_programs(&project, &original, &[], &[wrong], bounds(), deadline())
                    .is_err()
            );
        }
    }
}
