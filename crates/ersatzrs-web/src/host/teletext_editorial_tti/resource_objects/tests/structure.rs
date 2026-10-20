//! Whole original identity, genuine target compilation and partition-independent replay.
use super::super::project::{authorize_programs, reconcile_project, validate_applied_project};
use super::*;
use ersatzrs_app::teletext_export_project::TeletextExportFile;

#[path = "structure/extent.rs"]
mod extent;

#[path = "structure/positions.rs"]
mod positions;

#[path = "structure/added_body_clear.rs"]
mod added_body_clear;

fn bounds() -> (TeletextTtiBudget, usize, TeletextServiceBudget) {
    (
        limits().0,
        limits().1,
        TeletextServiceBudget {
            max_records: 16,
            max_objects: 16,
            max_structure_items: 4096,
            max_base_content_bytes: 8192,
            max_commands: 8192,
            max_collected_commands: 16384,
            max_collected_drcs_references: 16,
        },
    )
}
fn view(service: &EncodedTeletextAuthoredService, fragments: bool) -> ObjectEditorFrame {
    let page = service
        .pages()
        .iter()
        .find(|page| page.resource_header().is_some())
        .unwrap();
    if fragments {
        let definition = service
            .declared()
            .semantic()
            .objects()
            .definition(identity(false))
            .unwrap();
        let plans = plan_fragments(page, definition, limits().1, deadline()).unwrap();
        assert_eq!(plans.len(), 1);
        build_fragment(
            page,
            definition,
            &plans[0],
            limits().1,
            limits().0,
            deadline(),
        )
        .unwrap()
    } else {
        build_frame(page, identity(false), limits().1, limits().0, deadline()).unwrap()
    }
}
fn project_for(service: &EncodedTeletextAuthoredService, fragments: bool) -> TeletextExportProject {
    if fragments {
        super::fragments::fragment_project(service)
    } else {
        project(service, false)
    }
}
fn files(project: &TeletextExportProject, frame: &ObjectEditorFrame) -> Vec<TeletextExportFile> {
    let mut files = vec![TeletextExportFile {
        name: project.pages[0].name.clone(),
        bytes: vec![0],
    }];
    for asset in &project.assets {
        files.push(TeletextExportFile {
            name: asset.name.clone(),
            bytes: if matches!(
                asset.scope,
                TeletextExportAssetScope::ObjectTtiV1 { .. }
                    | TeletextExportAssetScope::ObjectTtiFragmentV1 { .. }
            ) {
                write_teletext_tti(&frame.document, limits().0).unwrap()
            } else {
                vec![0]
            },
        });
    }
    files
}

#[test]
fn resource_project_glyph_insert_delete_and_style_growth_use_whole_original_without_local_aliases()
{
    let original = compile(false, 2, b'A', TeletextObjectType::Passive);
    for fragments in [false, true] {
        let project = project_for(&original, fragments);
        let before = files(&project, &view(&original, fragments));
        for stage in ["insert", "delete", "style"] {
            let mut commands = original_commands(2, b'A');
            match stage {
                "insert" => commands.push(TeletextEnhancementCommand::AtColumn {
                    column: 8,
                    operation: glyph(b'B'),
                }),
                "delete" => {
                    commands.pop();
                }
                _ => commands.insert(
                    3,
                    TeletextEnhancementCommand::AtColumn {
                        column: 8,
                        operation: foreground(4),
                    },
                ),
            }
            let target =
                compile_commands(false, 2, b'A', TeletextObjectType::Passive, Some(commands));
            let after = files(&project, &view(&target, fragments));
            let edits =
                reconcile_project(&project, &original, &before, &after, bounds(), deadline())
                    .unwrap();
            assert!(edits.operands.is_empty());
            assert_eq!(edits.programs.len(), 1);
            let program = &edits.programs[0];
            assert_eq!(program.identity(), identity(false));
            assert!(program.identity().owner().is_none());
            assert_eq!(
                program.expected(),
                &original
                    .declared()
                    .semantic()
                    .objects()
                    .definition(identity(false))
                    .unwrap()
                    .program
            );
            assert_eq!(
                program.replacement(),
                &target
                    .declared()
                    .semantic()
                    .objects()
                    .definition(identity(false))
                    .unwrap()
                    .program
            );
            authorize_programs(
                &project,
                &original,
                &edits.operands,
                &edits.programs,
                bounds(),
                deadline(),
            )
            .unwrap();
            validate_applied_project(&project, &original, &target, &after, bounds(), deadline())
                .unwrap();
            assert!(validate_applied_project(
                &project,
                &original,
                &original,
                &after,
                bounds(),
                deadline()
            )
            .is_err());
        }
    }
}

#[test]
fn resource_project_keeps_scalar_api_and_rejects_wrapper_metadata_or_missing_member() {
    let original = compile(false, 2, b'A', TeletextObjectType::Passive);
    let project = project_for(&original, false);
    let before = files(&project, &view(&original, false));
    let target = compile(false, 4, b'A', TeletextObjectType::Passive);
    let after = files(&project, &view(&target, false));
    let edits =
        reconcile_project(&project, &original, &before, &after, bounds(), deadline()).unwrap();
    assert_eq!(edits.operands.len(), 1);
    assert!(edits.programs.is_empty());
    validate_applied_project(&project, &original, &target, &after, bounds(), deadline()).unwrap();
    let mut missing = after.clone();
    missing.pop();
    assert!(
        reconcile_project(&project, &original, &before, &missing, bounds(), deadline()).is_err()
    );
    let mut forged = after.clone();
    let frame = view(&target, false);
    forged[2].bytes = mutate(&frame, 0, words(&frame)[0] ^ 0x800);
    assert!(
        reconcile_project(&project, &original, &before, &forged, bounds(), deadline()).is_err()
    );
    let mut bounded = bounds();
    bounded.2.max_commands = 2;
    assert!(reconcile_project(&project, &original, &before, &after, bounded, deadline()).is_err());
}

#[test]
fn resource_project_preserves_six_hundred_hidden_anchors_without_a_logical_physical_cap() {
    let mut commands = vec![
        TeletextEnhancementCommand::SetPosition {
            position: TeletextEnhancementPosition::new(0, 0).unwrap(),
        };
        600
    ];
    commands.extend_from_slice(&original_commands(2, b'A')[1..]);
    let original = compile_commands(
        false,
        2,
        b'A',
        TeletextObjectType::Passive,
        Some(commands.clone()),
    );
    commands.push(TeletextEnhancementCommand::AtColumn {
        column: 8,
        operation: glyph(b'B'),
    });
    let target = compile_commands(false, 2, b'A', TeletextObjectType::Passive, Some(commands));
    for fragments in [false, true] {
        let project = project_for(&original, fragments);
        let before = files(&project, &view(&original, fragments));
        let after = files(&project, &view(&target, fragments));
        let edits =
            reconcile_project(&project, &original, &before, &after, bounds(), deadline()).unwrap();
        assert_eq!(edits.programs.len(), 1);
        let program = &edits.programs[0];
        assert_eq!(program.expected().commands().len(), 603);
        assert_eq!(program.replacement().commands().len(), 604);
        assert_eq!(
            &program.retained_commands()[..600],
            &(0..600).map(Some).collect::<Vec<_>>()
        );
        authorize_programs(
            &project,
            &original,
            &edits.operands,
            &edits.programs,
            bounds(),
            deadline(),
        )
        .unwrap();
        validate_applied_project(&project, &original, &target, &after, bounds(), deadline())
            .unwrap();
    }
}

fn group_files(
    service: &EncodedTeletextAuthoredService,
    project: &TeletextExportProject,
) -> Vec<TeletextExportFile> {
    let page = service
        .pages()
        .iter()
        .find(|page| page.resource_header().is_some())
        .unwrap();
    let definition = service
        .declared()
        .semantic()
        .objects()
        .definition(identity(false))
        .unwrap();
    let plans = plan_fragments(page, definition, limits().1, deadline()).unwrap();
    let mut files = vec![TeletextExportFile {
        name: project.pages[0].name.clone(),
        bytes: vec![0],
    }];
    for asset in &project.assets {
        let bytes =
            if let TeletextExportAssetScope::ObjectTtiFragmentV1 { fragment_index, .. } =
                asset.scope
            {
                let frame = build_fragment(
                    page,
                    definition,
                    &plans[usize::from(fragment_index)],
                    limits().1,
                    limits().0,
                    deadline(),
                )
                .unwrap();
                write_teletext_tti(&frame.document, limits().0).unwrap()
            } else {
                vec![0]
            };
        files.push(TeletextExportFile {
            name: asset.name.clone(),
            bytes,
        });
    }
    files
}

#[test]
fn resource_project_rebuilds_across_original_fragment_boundary_and_validates_uploaded_partitions() {
    let original = super::fragments::large_service();
    let project = super::fragments::fragment_project(&original);
    let TeletextExportAssetScope::ObjectTtiFragmentV1 { command_end, .. } = project.assets[1].scope
    else {
        unreachable!()
    };
    let mut commands = original
        .declared()
        .semantic()
        .objects()
        .definition(identity(false))
        .unwrap()
        .program
        .commands()
        .to_vec();
    commands.insert(
        usize::try_from(command_end).unwrap() - 1,
        TeletextEnhancementCommand::AtColumn {
            column: 8,
            operation: glyph(b'B'),
        },
    );
    let target = compile_commands(false, 2, b'A', TeletextObjectType::Passive, Some(commands));
    let before = group_files(&original, &project);
    let after = group_files(&target, &project);
    let edits =
        reconcile_project(&project, &original, &before, &after, bounds(), deadline()).unwrap();
    assert!(edits.operands.is_empty());
    assert_eq!(edits.programs.len(), 1);
    assert_eq!(
        edits.programs[0].replacement(),
        &target
            .declared()
            .semantic()
            .objects()
            .definition(identity(false))
            .unwrap()
            .program
    );
    authorize_programs(
        &project,
        &original,
        &edits.operands,
        &edits.programs,
        bounds(),
        deadline(),
    )
    .unwrap();
    validate_applied_project(&project, &original, &target, &after, bounds(), deadline()).unwrap();
    assert!(
        validate_applied_project(&project, &original, &original, &after, bounds(), deadline())
            .is_err()
    );
    let mut missing = after;
    missing.remove(2);
    assert!(
        reconcile_project(&project, &original, &before, &missing, bounds(), deadline()).is_err()
    );
}
