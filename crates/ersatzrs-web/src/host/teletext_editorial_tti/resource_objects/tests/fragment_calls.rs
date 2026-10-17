//! Native V7 calls retain whole original resource identity beyond one editor page.
use super::super::{
    context::tests as native,
    project::{authorize_programs, reconcile_project, validate_applied_project},
};
use super::*;
use ersatzrs_app::teletext_export_project::TeletextExportFile;

fn program(duplicate: bool) -> TeletextEnhancementProgram {
    let mut commands = vec![TeletextEnhancementCommand::SetPosition {
        position: TeletextEnhancementPosition::new(1, 0).unwrap(),
    }];
    commands.extend((0..220).map(|_| TeletextEnhancementCommand::AtColumn {
        column: 0,
        operation: glyph(b'A'),
    }));
    let call = TeletextEnhancementCommand::InvokeObject {
        target: native::target(22),
        object_type: TeletextObjectType::Passive,
        offset: Some(TeletextObjectOffset::new(0, 0).unwrap()),
    };
    commands.push(call);
    if duplicate {
        commands.push(call);
    }
    TeletextEnhancementProgram::new(commands).unwrap()
}
fn project(service: &EncodedTeletextAuthoredService) -> TeletextExportProject {
    let mut project = native::receipt(service);
    project
        .assets
        .retain(|asset| matches!(asset.scope, TeletextExportAssetScope::Resource { .. }));
    "qteletextmaker-1.0-carrier-v7".clone_into(&mut project.editor_profile);
    for page in service.pages() {
        let EncodedTeletextAuthoredPage::Object(resource) = page else {
            continue;
        };
        let name = project
            .assets
            .iter()
            .find_map(|asset| match &asset.scope {
                TeletextExportAssetScope::Resource { identity, .. }
                    if identity.page.magazine == resource.header().identity().page().magazine()
                        && identity.page.page == resource.header().identity().page().page()
                        && identity.subpage
                            == resource.header().identity().logical_subpage().get() =>
                {
                    Some(asset.name.clone())
                }
                _ => None,
            })
            .unwrap();
        for position in resource.definition_positions() {
            let definition = service
                .declared()
                .semantic()
                .objects()
                .definition(position.target())
                .unwrap();
            for plan in plan_fragments(page, definition, native::limits().1, deadline()).unwrap() {
                project.assets.push(TeletextExportAssetMapping {
                    name: format!(
                        "{}-{}-{}.tti",
                        name,
                        position.target().object_id().get(),
                        plan.fragment_index
                    ),
                    scope: TeletextExportAssetScope::ObjectTtiFragmentV1 {
                        resource_name: name.clone(),
                        object_id: position.target().object_id().get(),
                        fragment_index: plan.fragment_index,
                        fragment_count: plan.fragment_count,
                        command_start: plan.command_start,
                        command_end: plan.command_end,
                        native_word_start: plan.native_word_start,
                        native_word_end: plan.native_word_end,
                        editor_page: 256,
                        editor_ordinal: 0,
                    },
                    unit_count: 0,
                    bytes: 1,
                    sha256: "a".repeat(64),
                });
            }
        }
    }
    project
}
fn files(
    service: &EncodedTeletextAuthoredService,
    project: &TeletextExportProject,
) -> Vec<TeletextExportFile> {
    let mut files = vec![TeletextExportFile {
        name: project.pages[0].name.clone(),
        bytes: vec![0],
    }];
    for asset in &project.assets {
        let bytes = match &asset.scope {
            TeletextExportAssetScope::Resource { identity, .. } => service
                .pages()
                .iter()
                .find(|page| {
                    page.resource_header()
                        .is_some_and(|header| header.identity().page().page() == identity.page.page)
                })
                .unwrap()
                .data_units()
                .iter()
                .flatten()
                .copied()
                .collect(),
            TeletextExportAssetScope::ObjectTtiFragmentV1 { fragment_index, .. } => {
                let mapped = mapping_resource(
                    asset,
                    project,
                    service,
                    mapping(asset, project, service)
                        .unwrap()
                        .page
                        .resource_header()
                        .unwrap(),
                )
                .unwrap();
                let plans = plan_fragments(
                    mapped.page,
                    mapped.definition,
                    native::limits().1,
                    deadline(),
                )
                .unwrap();
                let frame = build_fragment(
                    mapped.page,
                    mapped.definition,
                    &plans[usize::from(*fragment_index)],
                    native::limits().1,
                    native::limits().0,
                    deadline(),
                )
                .unwrap();
                write_teletext_tti(&frame.document, native::limits().0).unwrap()
            }
            _ => unreachable!(),
        };
        files.push(TeletextExportFile {
            name: asset.name.clone(),
            bytes,
        });
    }
    files
}

#[test]
fn fragment_calls_nonfirst_duplicate_retains_full_native_identity_and_original_program() {
    let expected = program(false);
    let replacement = program(true);
    let original = native::native_fixture(expected.clone(), false);
    let candidate = native::native_fixture(replacement.clone(), false);
    let project = project(&original);
    let before = files(&original, &project);
    // Candidate TTI is generated independently; immutable raw resource files in
    // the upload remain original even when the requested native output changes.
    let mut after = files(&candidate, &project);
    for asset in &project.assets {
        if matches!(asset.scope, TeletextExportAssetScope::Resource { .. }) {
            after
                .iter_mut()
                .find(|file| file.name == asset.name)
                .unwrap()
                .bytes
                .clone_from(
                    &before
                        .iter()
                        .find(|file| file.name == asset.name)
                        .unwrap()
                        .bytes,
                );
        }
    }
    let selected = project.assets.iter().find(|asset|matches!(asset.scope,
        TeletextExportAssetScope::ObjectTtiFragmentV1 {object_id:21,fragment_index:1,command_start,command_end,..}
        if command_start <= 221 && command_end > 221)).unwrap();
    for (a, b) in before.iter().zip(&after) {
        assert_eq!(a.name, b.name);
        if a.name != selected.name {
            assert_eq!(
                a.bytes, b.bytes,
                "unselected fragments and raw resources stay exact"
            );
        }
    }
    assert_ne!(
        before
            .iter()
            .find(|file| file.name == selected.name)
            .unwrap()
            .bytes,
        after
            .iter()
            .find(|file| file.name == selected.name)
            .unwrap()
            .bytes
    );
    eprintln!("independent native original222/candidate223 commands; original call221 in V7 nonfirst fragment1; all other uploads unchanged");
    let edits = reconcile_project(
        &project,
        &original,
        &before,
        &after,
        native::limits(),
        deadline(),
    )
    .unwrap();
    assert!(edits.operands.is_empty());
    let [change] = edits.programs.as_slice() else {
        panic!("one exact full-resource program")
    };
    assert_eq!(change.identity(), native::target(21));
    assert_eq!(change.expected(), &expected);
    assert_eq!(change.replacement(), &replacement);
    let mut retained = (0..221).map(Some).collect::<Vec<_>>();
    retained.extend([None, Some(221)]);
    assert_eq!(change.retained_commands(), retained);
    assert!(change.has_invocation_structure_changes());
    assert_eq!(
        original
            .declared()
            .clone()
            .with_resource_object_program_overlays(&edits.programs, native::limits().2)
            .unwrap(),
        *candidate.declared()
    );
    authorize_programs(
        &project,
        &original,
        &edits.operands,
        &edits.programs,
        native::limits(),
        deadline(),
    )
    .unwrap();
    validate_applied_project(
        &project,
        &original,
        &candidate,
        &after,
        native::limits(),
        deadline(),
    )
    .unwrap();
    assert_native_resource_changes(&original, &candidate);
}

fn assert_native_resource_changes(
    original: &EncodedTeletextAuthoredService,
    candidate: &EncodedTeletextAuthoredService,
) {
    for (old, new) in original.pages().iter().zip(candidate.pages()) {
        let root = old
            .resource_header()
            .is_some_and(|header| Some(header.identity()) == native::target(21).resource());
        if root {
            assert_ne!(old.data_units(), new.data_units());
        } else if old.resource_header().is_some() {
            assert_eq!(old.data_units(), new.data_units());
        }
    }
}

fn edited_files(
    original: &EncodedTeletextAuthoredService,
    candidate: &EncodedTeletextAuthoredService,
    project: &TeletextExportProject,
) -> (Vec<TeletextExportFile>, Vec<TeletextExportFile>) {
    let before = files(original, project);
    let mut after = files(candidate, project);
    for asset in &project.assets {
        if matches!(asset.scope, TeletextExportAssetScope::Resource { .. }) {
            let source = &before
                .iter()
                .find(|file| file.name == asset.name)
                .unwrap()
                .bytes;
            after
                .iter_mut()
                .find(|file| file.name == asset.name)
                .unwrap()
                .bytes
                .clone_from(source);
        }
    }
    (before, after)
}
fn check_program(
    original: &EncodedTeletextAuthoredService,
    candidate: &EncodedTeletextAuthoredService,
    project: &TeletextExportProject,
    files: (&[TeletextExportFile], &[TeletextExportFile]),
    retained: &[Option<usize>],
) {
    let edits = reconcile_project(
        project,
        original,
        files.0,
        files.1,
        native::limits(),
        deadline(),
    )
    .unwrap();
    assert!(edits.operands.is_empty());
    let [change] = edits.programs.as_slice() else {
        panic!("one full native resource program")
    };
    assert_eq!(change.identity(), native::target(21));
    assert_eq!(
        change.expected(),
        &original
            .declared()
            .semantic()
            .objects()
            .definition(native::target(21))
            .unwrap()
            .program
    );
    assert_eq!(
        change.replacement(),
        &candidate
            .declared()
            .semantic()
            .objects()
            .definition(native::target(21))
            .unwrap()
            .program
    );
    assert_eq!(change.retained_commands(), retained);
    assert!(change.has_invocation_structure_changes());
    assert_eq!(
        original
            .declared()
            .clone()
            .with_resource_object_program_overlays(&edits.programs, native::limits().2)
            .unwrap(),
        *candidate.declared()
    );
    authorize_programs(
        project,
        original,
        &edits.operands,
        &edits.programs,
        native::limits(),
        deadline(),
    )
    .unwrap();
    validate_applied_project(
        project,
        original,
        candidate,
        files.1,
        native::limits(),
        deadline(),
    )
    .unwrap();
    assert!(validate_applied_project(
        project,
        original,
        original,
        files.1,
        native::limits(),
        deadline()
    )
    .is_err());
}

#[test]
fn fragment_calls_delete_and_compound_glyph_style_growth_keep_native_targets_and_ordinals() {
    let original = native::native_fixture(program(false), false);
    let project = project(&original);
    let mut deleted = program(false).commands().to_vec();
    deleted.pop();
    let candidate =
        native::native_fixture(TeletextEnhancementProgram::new(deleted).unwrap(), false);
    let (before, after) = edited_files(&original, &candidate, &project);
    check_program(
        &original,
        &candidate,
        &project,
        (&before, &after),
        &(0..221).map(Some).collect::<Vec<_>>(),
    );

    let mut grown = program(true).commands().to_vec();
    grown.splice(
        221..221,
        [
            TeletextEnhancementCommand::AtColumn {
                column: 0,
                operation: foreground(2),
            },
            TeletextEnhancementCommand::AtColumn {
                column: 0,
                operation: glyph(b'B'),
            },
        ],
    );
    let candidate = native::native_fixture(TeletextEnhancementProgram::new(grown).unwrap(), false);
    let (before, after) = edited_files(&original, &candidate, &project);
    let mut retained = (0..221).map(Some).collect::<Vec<_>>();
    retained.extend([None, None, None, Some(221)]);
    check_program(
        &original,
        &candidate,
        &project,
        (&before, &after),
        &retained,
    );
}

#[test]
fn fragment_calls_known_original_selector_keeps_full_identity_despite_unused_resource_collision() {
    let original = native::native_fixture(program(false), true);
    let candidate = native::native_fixture(program(true), true);
    assert_ne!(native::target(22), native::target(24));
    let project = project(&original);
    let (before, after) = edited_files(&original, &candidate, &project);
    let mut retained = (0..221).map(Some).collect::<Vec<_>>();
    retained.extend([None, Some(221)]);
    check_program(
        &original,
        &candidate,
        &project,
        (&before, &after),
        &retained,
    );
    for command in &candidate
        .declared()
        .semantic()
        .objects()
        .definition(native::target(21))
        .unwrap()
        .program
        .commands()[221..]
    {
        assert!(
            matches!(command, TeletextEnhancementCommand::InvokeObject { target, .. } if *target==native::target(22))
        );
    }
    let untouched = native::target(24).resource().unwrap();
    let resource = |service: &EncodedTeletextAuthoredService| {
        service
            .pages()
            .iter()
            .find(|page| {
                page.resource_header()
                    .is_some_and(|header| header.identity() == untouched)
            })
            .unwrap()
            .data_units()
            .to_vec()
    };
    assert_eq!(resource(&original), resource(&candidate));
}

#[test]
fn fragment_calls_genuinely_unused_colliding_selector_cannot_invent_a_full_resource_identity() {
    let mut commands = program(false).commands().to_vec();
    commands[221] = TeletextEnhancementCommand::InvokeObject {
        target: native::target(23),
        object_type: TeletextObjectType::Adaptive,
        offset: Some(TeletextObjectOffset::new(0, 0).unwrap()),
    };
    let original = native::native_fixture(
        TeletextEnhancementProgram::new(commands.clone()).unwrap(),
        true,
    );
    commands.push(program(false).commands()[221]);
    // Calling selected POP22 is genuinely valid natively; the uploaded selector
    // alone cannot choose it over unused POP24 in a different full resource.
    let candidate =
        native::native_fixture(TeletextEnhancementProgram::new(commands).unwrap(), true);
    let project = project(&original);
    let (before, after) = edited_files(&original, &candidate, &project);
    assert!(reconcile_project(
        &project,
        &original,
        &before,
        &after,
        native::limits(),
        deadline()
    )
    .is_err());
}

fn tamper_added_call(files: &mut [TeletextExportFile], project: &TeletextExportProject, kind: u8) {
    let selected = project
        .assets
        .iter()
        .find(|asset| {
            matches!(
                asset.scope,
                TeletextExportAssetScope::ObjectTtiFragmentV1 {
                    object_id: 21,
                    fragment_index: 1,
                    ..
                }
            )
        })
        .unwrap();
    let file = files
        .iter_mut()
        .find(|file| file.name == selected.name)
        .unwrap();
    let parsed =
        ersatzrs_app::teletext_tti::parse_teletext_tti(&file.bytes, native::limits().0).unwrap();
    let mut records = parsed.records().to_vec();
    let mut words = records
        .iter()
        .flat_map(|record| match record {
            TeletextTtiRecord::Packet { row: 26, words, .. } => words.to_vec(),
            _ => vec![],
        })
        .collect::<Vec<_>>();
    let call = words
        .iter()
        .rposition(|word| word & 63 >= 40 && (word >> 6) & 31 == 19)
        .unwrap();
    assert_eq!(words[call - 1], 0x28 | (16 << 6));
    match kind {
        0 => words[call] = (words[call] & 0x7ff) | (127 << 11),
        1 => words[call] = (words[call] & !63) | 0x28,
        2 => words[call - 1] = 0x28 | (16 << 6) | (72 << 11),
        _ => unreachable!(),
    }
    for record in &mut records {
        if let TeletextTtiRecord::Packet {
            row: 26,
            designation,
            words: packet,
        } = record
        {
            packet.copy_from_slice(
                &words[usize::from(*designation) * 13..usize::from(*designation) * 13 + 13],
            );
        }
    }
    file.bytes = bytes(records);
}

#[test]
fn fragment_calls_unknown_local_and_reserved_origin_remain_refused_with_changed_count() {
    let original = native::native_fixture(program(false), false);
    let candidate = native::native_fixture(program(true), false);
    let project = project(&original);
    let (before, after) = edited_files(&original, &candidate, &project);
    for kind in 0..3 {
        let mut forged = after.clone();
        tamper_added_call(&mut forged, &project, kind);
        assert!(
            reconcile_project(
                &project,
                &original,
                &before,
                &forged,
                native::limits(),
                deadline()
            )
            .is_err(),
            "invalid raw call kind{kind}"
        );
    }
}

#[test]
fn fragment_calls_whole_original_command_budget_deadline_and_complete_membership_stay_required() {
    let original = native::native_fixture(program(false), false);
    let candidate = native::native_fixture(program(true), false);
    let project = project(&original);
    let (before, after) = edited_files(&original, &candidate, &project);
    let mut bounded = native::limits();
    bounded.2.max_commands = 221;
    assert!(reconcile_project(&project, &original, &before, &after, bounded, deadline()).is_err());
    let expired = Instant::now()
        .checked_sub(std::time::Duration::from_secs(1))
        .unwrap();
    assert_eq!(
        reconcile_project(
            &project,
            &original,
            &before,
            &after,
            native::limits(),
            expired
        )
        .err()
        .unwrap()
        .failure,
        Failure::TimedOut
    );
    let first = project
        .assets
        .iter()
        .find(|asset| {
            matches!(
                asset.scope,
                TeletextExportAssetScope::ObjectTtiFragmentV1 {
                    object_id: 21,
                    fragment_index: 0,
                    ..
                }
            )
        })
        .unwrap();
    let mut missing = after;
    missing.retain(|file| file.name != first.name);
    assert!(reconcile_project(
        &project,
        &original,
        &before,
        &missing,
        native::limits(),
        deadline()
    )
    .is_err());
}
