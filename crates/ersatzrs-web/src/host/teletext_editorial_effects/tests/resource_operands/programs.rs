//! Canonical pointer/extent changes and shared protected structural lineage.
use super::*;
use ersatzrs_domain::TeletextResourceObjectProgramOverlay;

fn checked(
    global: bool,
    original: &EncodedTeletextAuthoredService,
    replacement: Vec<TeletextEnhancementCommand>,
    retained: Vec<Option<usize>>,
) -> TeletextResourceObjectProgramOverlay {
    TeletextResourceObjectProgramOverlay::new(
        target(global, 65520),
        original
            .declared()
            .semantic()
            .objects()
            .definition(target(global, 65520))
            .unwrap()
            .program
            .clone(),
        TeletextEnhancementProgram::new(replacement).unwrap(),
        retained,
        policy().composition.service,
    )
    .unwrap()
}
fn program_edits(programs: &[TeletextResourceObjectProgramOverlay]) -> TeletextEditorialEdits<'_> {
    let mut result = edits(&[]);
    result.resource_programs = programs;
    result
}
#[test]
fn resource_program_effects_preserve_shared_lineage_while_canonically_relocating_following_definition(
) {
    for global in [false, true] {
        let fixture = Fixture {
            global,
            ..Fixture::default()
        };
        let before = compile_with_program(fixture, None, true);
        let mut replacement = vec![at(0, foreground(1))];
        replacement.extend(vec![at(0, glyph(b'A')); 21]);
        let mut retained = vec![Some(0)];
        retained.extend(vec![None; 20]);
        retained.push(Some(1));
        let change = checked(global, &before, replacement.clone(), retained);
        let after = compile_with_program(
            fixture,
            Some(TeletextEnhancementProgram::new(replacement.clone()).unwrap()),
            true,
        );
        let object = |service: &EncodedTeletextAuthoredService| {
            service
                .pages()
                .iter()
                .find_map(|page| match page {
                    EncodedTeletextAuthoredPage::Object(page)
                        if page.header().identity() == resource(global) =>
                    {
                        Some(page.clone())
                    }
                    _ => None,
                })
                .unwrap()
        };
        let (old, new) = (object(&before), object(&after));
        assert_ne!(old.header().last_packet(), new.header().last_packet());
        let following = target(global, 65521);
        let position = |page: &ersatzrs_ffmpeg::teletext::EncodedTeletextObjectPage| {
            page.definition_positions()
                .iter()
                .find(|p| p.target() == following)
                .unwrap()
                .absolute_index()
        };
        assert_ne!(position(&old), position(&new));
        let changes = [change];
        validate_teletext_editorial_effects_with_operands(
            &before,
            &after,
            &permissions(true),
            program_edits(&changes),
            &policy().encoding,
            deadline(),
        )
        .unwrap();
        assert!(validate_teletext_editorial_effects_with_operands(
            &before,
            &after,
            &permissions(true),
            edits(&[]),
            &policy().encoding,
            deadline()
        )
        .is_err());
        let wrong_lifecycle = compile_with_program(
            Fixture {
                counter: 2,
                ..fixture
            },
            Some(TeletextEnhancementProgram::new(replacement).unwrap()),
            true,
        );
        assert_eq!(
            validate_teletext_editorial_effects_with_operands(
                &before,
                &wrong_lifecycle,
                &permissions(true),
                program_edits(&changes),
                &policy().encoding,
                deadline()
            )
            .unwrap_err()
            .reason,
            TeletextEditorialEffectsFailure::ProtectedStructure
        );
    }
}
#[test]
fn resource_program_deletion_requires_every_shared_consumer_permission() {
    for global in [false, true] {
        let fixture = Fixture {
            global,
            ..Fixture::default()
        };
        let before = compile(fixture);
        let replacement = vec![at(0, foreground(1))];
        let changes = [checked(global, &before, replacement.clone(), vec![Some(0)])];
        let after = compile_with_program(
            fixture,
            Some(TeletextEnhancementProgram::new(replacement).unwrap()),
            false,
        );
        validate_teletext_editorial_effects_with_operands(
            &before,
            &after,
            &permissions(true),
            program_edits(&changes),
            &policy().encoding,
            deadline(),
        )
        .unwrap();
        let mut generated = permissions(true);
        generated.producers[0].authoring.as_mut().unwrap().pages[1].regions[0].kind =
            TeletextAuthoringRegionKindWire::Generated;
        for project in [permissions(false), generated] {
            let failure = validate_teletext_editorial_effects_with_operands(
                &before,
                &after,
                &project,
                program_edits(&changes),
                &policy().encoding,
                deadline(),
            )
            .unwrap_err();
            assert_eq!(
                failure.reason,
                TeletextEditorialEffectsFailure::ProtectedDisplay
            );
            assert_eq!(failure.page, Some(address(124)));
            assert_eq!(failure.position, Some((1, 8)));
        }
    }
}
