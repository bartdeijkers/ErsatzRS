//! Shared POP/GPOP consumers and complete original-bound resource encoding.
use super::*;
use ersatzrs_ffmpeg::teletext::{
    encode_teletext_authored_service_before, TeletextServiceResourceTransmission,
};

#[derive(Clone, Copy)]
struct Fixture {
    global: bool,
    code: u8,
    colour: u8,
    counter: u8,
    last: bool,
    object_id: u16,
    other_code: u8,
}
impl Default for Fixture {
    fn default() -> Self {
        Self {
            global: false,
            code: b'A',
            colour: 1,
            counter: 1,
            last: true,
            object_id: 65520,
            other_code: b'Z',
        }
    }
}
fn resource(global: bool) -> TeletextResourceIdentity {
    TeletextResourceIdentity::new(
        if global {
            TeletextResourceKind::Gpop
        } else {
            TeletextResourceKind::Pop
        },
        TeletextRawDataPageNumber::new(1, if global { 0xa1 } else { 0xa0 }).unwrap(),
        TeletextResourceSubpageIndex::new(15).unwrap(),
    )
    .unwrap()
}
fn target(global: bool, id: u16) -> TeletextObjectIdentity {
    if global {
        TeletextObjectIdentity::global(resource(global), TeletextObjectId::new(id))
    } else {
        TeletextObjectIdentity::public(resource(global), TeletextObjectId::new(id))
    }
    .unwrap()
}
fn foreground(colour: u8) -> TeletextColumnOperation {
    TeletextColumnOperation::Foreground {
        colour: TeletextColourReference::new(colour).unwrap(),
    }
}
fn glyph(code: u8) -> TeletextColumnOperation {
    TeletextColumnOperation::G0 {
        code: TeletextGlyphCode::new(code).unwrap(),
    }
}
fn at(column: u8, operation: TeletextColumnOperation) -> TeletextEnhancementCommand {
    TeletextEnhancementCommand::AtColumn { column, operation }
}
fn links(global: bool) -> TeletextX27Level25Links {
    let absent = TeletextX27LinkDeclaration::new(
        TeletextX27LinkTarget::Unspecified {
            magazine: TeletextX27Magazine::new(1).unwrap(),
        },
        TeletextX27LinkValidity::Level25And35,
        TeletextSubpageRequirementMask::new(u16::MAX),
    )
    .unwrap();
    let selected = TeletextX27LinkDeclaration::new(
        TeletextX27LinkTarget::Page {
            page: resource(global).page(),
        },
        TeletextX27LinkValidity::Level25And35,
        TeletextSubpageRequirementMask::new(1 << 15),
    )
    .unwrap();
    TeletextX27Level25Links::new(
        if global { selected } else { absent },
        if global { absent } else { selected },
        absent,
        absent,
    )
    .unwrap()
}
#[path = "resource_operands/bank_sets.rs"]
mod bank_sets;
#[path = "resource_operands/implicit_mot_selection.rs"]
mod implicit_mot_selection;
#[path = "resource_operands/object_page_selections.rs"]
mod object_page_selections;
#[path = "resource_operands/programs.rs"]
mod programs;
#[path = "resource_operands/subpage_announcements.rs"]
mod subpage_announcements;

fn compile(fixture: Fixture) -> EncodedTeletextAuthoredService {
    compile_with_program(fixture, None, false)
}
fn compile_with_program(
    fixture: Fixture,
    program: Option<TeletextEnhancementProgram>,
    same_resource: bool,
) -> EncodedTeletextAuthoredService {
    let policy = policy();
    let lifted = ersatzrs_ffmpeg::teletext::lift_teletext_v1_page_set(
        draft("ASCII"),
        TeletextEncodingRegion::WesternLatin,
        policy.validation.service,
    )
    .unwrap();
    let mut pages: Vec<_> = lifted.page_enhancements().values().cloned().collect();
    for page in &mut pages {
        page.compositional_links = Some(links(fixture.global));
        if fixture.global {
            page.resources.objects.global = Some(resource(true).page());
        } else {
            page.resources.objects.public = Some(resource(false).page());
        }
        page.local_program = Some(
            TeletextEnhancementProgram::new(vec![
                TeletextEnhancementCommand::SetPosition {
                    position: TeletextEnhancementPosition::new(1, 8).unwrap(),
                },
                TeletextEnhancementCommand::InvokeObject {
                    target: target(fixture.global, fixture.object_id),
                    object_type: TeletextObjectType::Passive,
                    offset: None,
                },
            ])
            .unwrap(),
        );
    }
    let objects = vec![
        TeletextEnhancementObjectDefinition {
            identity: target(fixture.global, fixture.object_id),
            object_type: TeletextObjectType::Passive,
            program: program.unwrap_or_else(|| {
                TeletextEnhancementProgram::new(vec![
                    at(0, foreground(fixture.colour)),
                    at(0, glyph(fixture.code)),
                ])
                .unwrap()
            }),
        },
        TeletextEnhancementObjectDefinition {
            identity: if same_resource {
                target(fixture.global, 65521)
            } else {
                target(!fixture.global, 1)
            },
            object_type: TeletextObjectType::Passive,
            program: TeletextEnhancementProgram::new(vec![at(0, glyph(fixture.other_code))])
                .unwrap(),
        },
    ];
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
    let transmissions: Vec<_> = declared
        .resource_inventory()
        .iter()
        .map(|resource| TeletextServiceResourceTransmission {
            resource: *resource,
            counter: TeletextResourceUpdateCounter::new(fixture.counter).unwrap(),
            update: TeletextResourceFullUpdate::Changed,
            last_subpage: fixture.last,
        })
        .collect();
    encode_teletext_authored_service_before(declared, &transmissions, &policy.encoding, deadline())
        .unwrap()
}
fn changes(
    global: bool,
    before: (u8, u8),
    after: (u8, u8),
) -> Vec<TeletextResourceObjectOperandOverlay> {
    let mut changes = vec![];
    if before.0 != after.0 {
        changes.push(
            TeletextResourceObjectOperandOverlay::new(
                target(global, 65520),
                0,
                0,
                foreground(before.0),
                foreground(after.0),
            )
            .unwrap(),
        );
    }
    if before.1 != after.1 {
        changes.push(
            TeletextResourceObjectOperandOverlay::new(
                target(global, 65520),
                1,
                0,
                glyph(before.1),
                glyph(after.1),
            )
            .unwrap(),
        );
    }
    changes
}
fn edits(resource_operands: &[TeletextResourceObjectOperandOverlay]) -> TeletextEditorialEdits<'_> {
    TeletextEditorialEdits {
        base_text_edits: &[],
        rows: &[],
        operands: &[],
        object_operands: &[],
        resource_operands,
        resource_programs: &[],
        resource_additions: &[],
        object_resource_additions: &[],
        object_subpage_announcements: &[],
        object_page_selections: &[],
        object_programs: &[],
        main_programs: &[],
        bitmaps: &[],
    }
}
fn permissions(second: bool) -> TeletextExportProject {
    let mut project = project("zone", 1, 8, 1, 1);
    if second {
        let mut mapping = project.pages[0].clone();
        mapping.name = "p124.tti".into();
        mapping.page.page = 124;
        mapping.editor_page = 0x124;
        project.pages.push(mapping);
        let pages = &mut project.producers[0].authoring.as_mut().unwrap().pages;
        let mut page = pages[0].clone();
        page.page.page = 124;
        pages.push(page);
    }
    project
}
fn validate(
    before: &EncodedTeletextAuthoredService,
    after: &EncodedTeletextAuthoredService,
    incoming: &[TeletextResourceObjectOperandOverlay],
    cumulative: &[TeletextResourceObjectOperandOverlay],
) -> Result<()> {
    validate_teletext_editorial_transition_with_operands(
        before,
        after,
        &permissions(true),
        edits(incoming),
        edits(cumulative),
        &policy().encoding,
        deadline(),
    )
}

#[test]
fn resource_object_effects_require_every_shared_consumer_for_fg_and_g0_changes() {
    for global in [false, true] {
        for (colour, code) in [(4, b'A'), (1, b'B'), (4, b'B')] {
            let before = compile(Fixture {
                global,
                ..Fixture::default()
            });
            let after = compile(Fixture {
                global,
                code,
                colour,
                ..Fixture::default()
            });
            let changes = changes(global, (1, b'A'), (colour, code));
            validate(&before, &after, &changes, &changes).unwrap();
            let mut generated = permissions(true);
            generated.producers[0].authoring.as_mut().unwrap().pages[1].regions[0].kind =
                TeletextAuthoringRegionKindWire::Generated;
            for project in [permissions(false), generated] {
                let failure = validate_teletext_editorial_effects_with_operands(
                    &before,
                    &after,
                    &project,
                    edits(&changes),
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
}

#[test]
fn resource_object_effects_preserve_latest_and_root_cumulative_ordinals_and_exact_undo() {
    let root = compile(Fixture::default());
    let latest = compile(Fixture {
        code: b'B',
        ..Fixture::default()
    });
    let final_service = compile(Fixture {
        code: b'C',
        ..Fixture::default()
    });
    let incoming = changes(false, (1, b'B'), (1, b'C'));
    let cumulative = changes(false, (1, b'A'), (1, b'C'));
    validate(&latest, &final_service, &incoming, &cumulative).unwrap();
    validate(&root, &final_service, &cumulative, &cumulative).unwrap();
    validate(
        &final_service,
        &root,
        &changes(false, (1, b'C'), (1, b'A')),
        &[],
    )
    .unwrap();
    validate(&root, &root, &[], &[]).unwrap();
    for (incoming, cumulative) in [
        (changes(false, (1, b'A'), (1, b'C')), cumulative.clone()),
        (incoming.clone(), changes(false, (1, b'A'), (1, b'B'))),
        (vec![incoming[0], incoming[0]], cumulative.clone()),
        (incoming.clone(), vec![cumulative[0], cumulative[0]]),
    ] {
        assert_eq!(
            validate(&latest, &final_service, &incoming, &cumulative)
                .unwrap_err()
                .reason,
            TeletextEditorialEffectsFailure::Association
        );
    }
}

#[test]
fn resource_object_effects_reject_other_definitions_structure_and_absent_authority() {
    let before = compile(Fixture::default());
    let changes = changes(false, (1, b'A'), (1, b'B'));
    let after = compile(Fixture {
        code: b'B',
        ..Fixture::default()
    });
    assert_eq!(
        validate(&before, &after, &[], &[]).unwrap_err().reason,
        TeletextEditorialEffectsFailure::ProtectedStructure
    );
    let unrelated = compile(Fixture {
        code: b'B',
        other_code: b'Q',
        ..Fixture::default()
    });
    assert_eq!(
        validate(&before, &unrelated, &changes, &changes)
            .unwrap_err()
            .reason,
        TeletextEditorialEffectsFailure::ProtectedStructure
    );
    let retarget = compile(Fixture {
        code: b'B',
        object_id: 65521,
        ..Fixture::default()
    });
    assert!(validate(&before, &retarget, &changes, &changes).is_err());
    let header = compile(Fixture {
        code: b'B',
        counter: 2,
        ..Fixture::default()
    });
    assert_eq!(
        validate(&before, &header, &changes, &changes)
            .unwrap_err()
            .reason,
        TeletextEditorialEffectsFailure::ProtectedStructure
    );
}

fn object_page(
    service: &EncodedTeletextAuthoredService,
    global: bool,
) -> &EncodedTeletextAuthoredPage {
    service
        .pages()
        .iter()
        .find(|page| {
            page.resource_header()
                .is_some_and(|header| header.identity() == resource(global))
        })
        .unwrap()
}
#[test]
fn resource_object_effects_authenticate_complete_packet_and_provenance_against_canonical_original()
{
    let before = compile(Fixture::default());
    let after = compile(Fixture {
        code: b'B',
        ..Fixture::default()
    });
    let context = resource_objects::Context::new(
        &before,
        &after,
        &[],
        &[],
        &[],
        &policy().encoding,
        deadline(),
    )
    .unwrap();
    context
        .check(
            object_page(&before, false),
            object_page(&after, false),
            deadline(),
        )
        .unwrap();
    for forged in [
        Fixture {
            code: b'C',
            ..Fixture::default()
        },
        Fixture {
            code: b'B',
            last: false,
            ..Fixture::default()
        },
        Fixture {
            code: b'B',
            counter: 2,
            ..Fixture::default()
        },
        Fixture {
            code: b'B',
            object_id: 65521,
            ..Fixture::default()
        },
    ] {
        let forged = compile(forged);
        assert_eq!(
            context
                .check(
                    object_page(&before, false),
                    object_page(&forged, false),
                    deadline()
                )
                .unwrap_err()
                .reason,
            TeletextEditorialEffectsFailure::ProtectedStructure
        );
    }
    // Logical identity metadata is authenticated even when physical object
    // numbering makes the resulting pointer/body bytes exactly identical.
    let forged_original = compile(Fixture {
        object_id: 65521,
        ..Fixture::default()
    });
    assert_eq!(
        object_page(&before, false).data_units(),
        object_page(&forged_original, false).data_units()
    );
    assert_eq!(
        context
            .check(
                object_page(&forged_original, false),
                object_page(&after, false),
                deadline()
            )
            .unwrap_err()
            .reason,
        TeletextEditorialEffectsFailure::ProtectedStructure
    );
}

#[test]
fn resource_object_effects_enforce_raw_command_count_and_preallocation_limits() {
    let before = compile(Fixture::default());
    let after = compile(Fixture {
        code: b'B',
        colour: 4,
        ..Fixture::default()
    });
    let changes = changes(false, (1, b'A'), (4, b'B'));
    let mut budget = policy().encoding;
    budget.max_commands = 1;
    assert_eq!(
        validate_teletext_editorial_effects_with_operands(
            &before,
            &after,
            &permissions(true),
            edits(&changes),
            &budget,
            deadline()
        )
        .unwrap_err()
        .reason,
        TeletextEditorialEffectsFailure::Budget
    );
    budget = policy().encoding;
    budget.normalization.max_resource_objects = 1;
    assert_eq!(
        validate_teletext_editorial_effects_with_operands(
            &before,
            &after,
            &permissions(true),
            edits(&changes),
            &budget,
            deadline()
        )
        .unwrap_err()
        .reason,
        TeletextEditorialEffectsFailure::Budget
    );
    assert_eq!(
        validate_teletext_editorial_effects_with_operands(
            &before,
            &after,
            &permissions(true),
            edits(&changes),
            &policy().encoding,
            Instant::now()
        )
        .unwrap_err()
        .reason,
        TeletextEditorialEffectsFailure::TimedOut
    );
}
