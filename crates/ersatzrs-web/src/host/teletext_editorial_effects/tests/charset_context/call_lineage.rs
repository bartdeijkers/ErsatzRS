//! Changed callees cannot retain charset authority through equal raw trace IDs.
use super::super::super::{
    charset_context::same_selection,
    lineage::{CanonicalSource, ProtectedLineage},
};
use super::*;
use ersatzrs_ffmpeg::teletext::{
    encode_teletext_authored_service_before, TeletextServiceResourceTransmission,
};

mod topology;

fn resource() -> TeletextResourceIdentity {
    TeletextResourceIdentity::new(
        TeletextResourceKind::Pop,
        TeletextRawDataPageNumber::new(1, 0xa0).unwrap(),
        TeletextResourceSubpageIndex::new(15).unwrap(),
    )
    .unwrap()
}
fn target(id: u16) -> TeletextObjectIdentity {
    TeletextObjectIdentity::public(resource(), TeletextObjectId::new(id)).unwrap()
}
fn program(commands: Vec<TeletextEnhancementCommand>) -> TeletextEnhancementProgram {
    TeletextEnhancementProgram::new(commands).unwrap()
}
fn call(
    id: u16,
    kind: TeletextObjectType,
    offset: Option<TeletextObjectOffset>,
) -> TeletextEnhancementCommand {
    TeletextEnhancementCommand::InvokeObject {
        target: target(id),
        object_type: kind,
        offset,
    }
}
fn links() -> TeletextX27Level25Links {
    let absent = TeletextX27LinkDeclaration::new(
        TeletextX27LinkTarget::Unspecified {
            magazine: TeletextX27Magazine::new(1).unwrap(),
        },
        TeletextX27LinkValidity::Level25And35,
        TeletextSubpageRequirementMask::new(u16::MAX),
    )
    .unwrap();
    let public = TeletextX27LinkDeclaration::new(
        TeletextX27LinkTarget::Page {
            page: resource().page(),
        },
        TeletextX27LinkValidity::Level25And35,
        TeletextSubpageRequirementMask::new(1 << 15),
    )
    .unwrap();
    TeletextX27Level25Links::new(absent, public, absent, absent).unwrap()
}
fn object_definitions(
    offset: Option<TeletextObjectOffset>,
) -> Vec<TeletextEnhancementObjectDefinition> {
    [
        (
            21,
            TeletextObjectType::Active,
            vec![
                call(22, TeletextObjectType::Passive, offset),
                call(23, TeletextObjectType::Passive, None),
            ],
        ),
        (
            22,
            TeletextObjectType::Passive,
            vec![
                at(0, designation(0)),
                at(
                    0,
                    TeletextColumnOperation::G0 {
                        code: TeletextGlyphCode::new(b'A').unwrap(),
                    },
                ),
            ],
        ),
        (
            23,
            TeletextObjectType::Passive,
            vec![
                TeletextEnhancementCommand::SetPosition {
                    position: TeletextEnhancementPosition::new(0, 2).unwrap(),
                },
                at(2, designation(0)),
                at(
                    2,
                    TeletextColumnOperation::G0 {
                        code: TeletextGlyphCode::new(b'B').unwrap(),
                    },
                ),
            ],
        ),
    ]
    .into_iter()
    .map(
        |(id, object_type, commands)| TeletextEnhancementObjectDefinition {
            identity: target(id),
            object_type,
            program: program(commands),
        },
    )
    .collect()
}
fn native_service(offset: Option<TeletextObjectOffset>) -> EncodedTeletextAuthoredService {
    native_with_definitions(object_definitions(offset))
}
fn native_with_definitions(
    objects: Vec<TeletextEnhancementObjectDefinition>,
) -> EncodedTeletextAuthoredService {
    let policy = policy();
    let lifted = ersatzrs_ffmpeg::teletext::lift_teletext_v1_page_set(
        draft("ASCII"),
        TeletextEncodingRegion::WesternLatin,
        policy.validation.service,
    )
    .unwrap();
    let pages = lifted
        .page_enhancements()
        .values()
        .cloned()
        .map(|mut page| {
            page.compositional_links = Some(links());
            page.resources.objects.public = Some(resource().page());
            page.local_program = Some(program(vec![
                TeletextEnhancementCommand::SetPosition {
                    position: TeletextEnhancementPosition::new(1, 8).unwrap(),
                },
                call(21, TeletextObjectType::Active, None),
            ]));
            page
        })
        .collect();
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
    encode_teletext_authored_service_before(
        declared,
        &[TeletextServiceResourceTransmission {
            resource: resource(),
            counter: TeletextResourceUpdateCounter::new(1).unwrap(),
            update: ersatzrs_ffmpeg::teletext::TeletextResourceFullUpdate::Changed,
            last_subpage: true,
        }],
        &policy.encoding,
        deadline(),
    )
    .unwrap()
}

fn declaration(
    execution: &TeletextCharsetExecution,
    id: u16,
) -> (usize, &TeletextCharsetDeclaration) {
    execution
        .declarations()
        .iter()
        .enumerate()
        .find(|(_, declaration)| declaration.source().object == Some(target(id)))
        .unwrap()
}
fn selection(
    execution: &TeletextCharsetExecution,
    declaration: usize,
) -> TeletextExecutedCharsetSelection {
    execution
        .records()
        .iter()
        .filter_map(ersatzrs_domain::TeletextExecutedGlyphCharset::selection)
        .find(|selection| selection.declaration() == Some(declaration))
        .unwrap()
}
fn assert_equal_original_fields(a: &TeletextCharsetDeclaration, b: &TeletextCharsetDeclaration) {
    assert_eq!(a.designation(), b.designation());
    assert_eq!(a.position(), b.position());
    assert_eq!(a.source(), b.source());
    assert_eq!(a.root_source(), b.root_source());
    assert_eq!(a.invocation(), b.invocation());
}

#[test]
fn changed_resource_call_charset_writer_cannot_alias_original_through_equal_payload_and_raw_id() {
    let zero = Some(TeletextObjectOffset::new(0, 0).unwrap());
    for (old_offset, new_offset) in [(zero, None), (None, zero)] {
        let old = native_service(old_offset);
        let new = native_service(new_offset);
        let expected = old
            .declared()
            .semantic()
            .objects()
            .definition(target(21))
            .unwrap()
            .program
            .clone();
        let replacement = new
            .declared()
            .semantic()
            .objects()
            .definition(target(21))
            .unwrap()
            .program
            .clone();
        let changes = [
            TeletextResourceObjectProgramOverlay::new_with_invocation_changes(
                target(21),
                expected,
                replacement,
                vec![Some(0), Some(1)],
                policy().validation.service,
            )
            .unwrap(),
        ];
        let edits = TeletextEditorialEdits {
            base_text_edits: &[],
            rows: &[],
            operands: &[],
            object_operands: &[],
            resource_operands: &[],
            resource_programs: &changes,
            resource_additions: &[],
            object_programs: &[],
            main_programs: &[],
            bitmaps: &[],
        };
        let (a, b) = (composition(&old), composition(&new));
        let lineage = ProtectedLineage::new(a, b, edits, &policy().encoding, deadline()).unwrap();
        let (before, after) = (a.charset_execution(), b.charset_execution());
        let (a_index, a_declaration) = declaration(before, 22);
        let (b_index, b_declaration) = declaration(after, 22);
        assert_equal_original_fields(a_declaration, b_declaration);
        assert_eq!(
            lineage.source(b_declaration.source()),
            CanonicalSource::Existing(a_declaration.source())
        );
        assert_eq!(
            lineage.source_at_invocation(b_declaration.source(), b_declaration.invocation()),
            CanonicalSource::Inserted(b_declaration.source())
        );
        assert!(!same_selection(
            before,
            after,
            Some(selection(before, a_index)),
            Some(selection(after, b_index)),
            &lineage,
            deadline()
        )
        .unwrap());
        let (a_index, a_declaration) = declaration(before, 23);
        let (b_index, b_declaration) = declaration(after, 23);
        assert_equal_original_fields(a_declaration, b_declaration);
        assert_eq!(
            lineage.source_at_invocation(b_declaration.source(), b_declaration.invocation()),
            CanonicalSource::Existing(a_declaration.source())
        );
        assert!(same_selection(
            before,
            after,
            Some(selection(before, a_index)),
            Some(selection(after, b_index)),
            &lineage,
            deadline()
        )
        .unwrap());
    }
}
