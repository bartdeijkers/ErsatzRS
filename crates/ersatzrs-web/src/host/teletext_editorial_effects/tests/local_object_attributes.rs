//! Permissions cover every cell of a retained sized object, not its command column.
use super::*;

fn identity(owner: TeletextPageAddress) -> TeletextObjectIdentity {
    TeletextObjectIdentity::local(owner, TeletextObjectId::new(65520))
}
fn attributes(invert: bool) -> TeletextColumnOperation {
    TeletextColumnOperation::Attributes {
        double_height: true,
        double_width: true,
        boxed: false,
        conceal: false,
        invert,
        underline_or_separated_mosaics: false,
    }
}
fn original() -> EncodedTeletextAuthoredService {
    let policy = policy();
    let lifted = ersatzrs_ffmpeg::teletext::lift_teletext_v1_page_set(
        draft("ASCII"),
        TeletextEncodingRegion::WesternLatin,
        policy.validation.service,
    )
    .unwrap();
    let mut pages: Vec<_> = lifted.page_enhancements().values().cloned().collect();
    let mut objects = Vec::new();
    for page in &mut pages {
        let target = identity(page.address);
        page.local_program = Some(
            TeletextEnhancementProgram::new(vec![
                TeletextEnhancementCommand::SetPosition {
                    position: TeletextEnhancementPosition::new(1, 8).unwrap(),
                },
                TeletextEnhancementCommand::InvokeObject {
                    target,
                    object_type: TeletextObjectType::Passive,
                    offset: None,
                },
            ])
            .unwrap(),
        );
        objects.push(TeletextEnhancementObjectDefinition {
            identity: target,
            object_type: TeletextObjectType::Passive,
            program: TeletextEnhancementProgram::new(vec![
                TeletextEnhancementCommand::AtColumn {
                    column: 0,
                    operation: attributes(false),
                },
                TeletextEnhancementCommand::AtColumn {
                    column: 0,
                    operation: TeletextColumnOperation::Foreground {
                        colour: TeletextColourReference::new(1).unwrap(),
                    },
                },
                TeletextEnhancementCommand::AtColumn {
                    column: 0,
                    operation: TeletextColumnOperation::G0 {
                        code: TeletextGlyphCode::new(b'A').unwrap(),
                    },
                },
            ])
            .unwrap(),
        });
    }
    let semantic = TeletextSemanticService::new(
        TeletextResolvedServiceDraft {
            base: lifted.base().clone(),
            pages,
            magazines: vec![],
            objects,
            drcs: vec![],
        },
        policy.validation.service,
    )
    .unwrap();
    let declared = TeletextDeclaredService::new(
        semantic,
        vec![],
        [123, 124]
            .map(|page| TeletextDisplayMotSelection {
                display: address(page),
                mot: None,
            })
            .into(),
        vec![],
        policy.validation.declarations,
    )
    .unwrap();
    ersatzrs_ffmpeg::teletext::encode_teletext_authored_service_before(
        declared,
        &[],
        &policy.encoding,
        deadline(),
    )
    .unwrap()
}

#[test]
fn retained_sized_object_attributes_require_the_whole_original_lower_and_right_footprint() {
    let before = original();
    let changes = [TeletextLocalObjectOperandOverlay::new(
        identity(address(123)),
        0,
        0,
        attributes(false),
        attributes(true),
    )
    .unwrap()];
    let after = ersatzrs_ffmpeg::teletext::encode_teletext_authored_service_before(
        before
            .declared()
            .clone()
            .with_local_object_operand_overlays(&changes, policy().validation.service)
            .unwrap(),
        &[],
        &policy().encoding,
        deadline(),
    )
    .unwrap();
    assert_ne!(
        before.pages()[0].data_units(),
        after.pages()[0].data_units()
    );
    assert_eq!(
        before.pages()[1].data_units(),
        after.pages()[1].data_units()
    );
    let edits = TeletextEditorialEdits {
        rows: &[],
        operands: &[],
        object_operands: &changes,
        resource_operands: &[],
        resource_programs: &[],
        resource_additions: &[],
        object_programs: &[],
        bitmaps: &[],
    };
    for kind in ["free", "zone"] {
        validate_teletext_editorial_effects_with_operands(
            &before,
            &after,
            &project(kind, 1, 8, 2, 2),
            edits,
            &policy().encoding,
            deadline(),
        )
        .unwrap();
    }
    for (width, height, protected) in [(2, 1, (2, 8)), (1, 2, (1, 9))] {
        let error = validate_teletext_editorial_effects_with_operands(
            &before,
            &after,
            &project("zone", 1, 8, width, height),
            edits,
            &policy().encoding,
            deadline(),
        )
        .unwrap_err();
        assert_eq!(
            error.reason,
            TeletextEditorialEffectsFailure::ProtectedDisplay
        );
        assert_eq!(error.page, Some(address(123)));
        assert_eq!(error.position, Some(protected));
    }
}
