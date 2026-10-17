//! Fragment authority stays on the original full object and command ordinals.
use super::*;

pub(super) fn large_service() -> EncodedTeletextAuthoredService {
    let mut commands = original_commands(2, b'A');
    commands.extend((0..440).map(|_| TeletextEnhancementCommand::AtColumn {
        column: 8,
        operation: glyph(b'A'),
    }));
    compile_commands(false, 2, b'A', TeletextObjectType::Passive, Some(commands))
}
pub(super) fn fragment_project(service: &EncodedTeletextAuthoredService) -> TeletextExportProject {
    let mut project = project(service, false);
    "qteletextmaker-1.0-carrier-v7".clone_into(&mut project.editor_profile);
    project.assets.pop();
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
    for plan in plan_fragments(page, definition, limits().1, deadline()).unwrap() {
        project.assets.push(TeletextExportAssetMapping {
            name: format!("object-{}.tti", plan.fragment_index),
            scope: TeletextExportAssetScope::ObjectTtiFragmentV1 {
                resource_name: "resource.bin".to_owned(),
                object_id: 65535,
                fragment_index: plan.fragment_index,
                fragment_count: plan.fragment_count,
                command_start: plan.command_start,
                command_end: plan.command_end,
                native_word_start: plan.native_word_start,
                native_word_end: plan.native_word_end,
                editor_page: 0x100,
                editor_ordinal: 0,
            },
            unit_count: 0,
            bytes: 1,
            sha256: "a".repeat(64),
        });
    }
    project
}
fn member(
    service: &EncodedTeletextAuthoredService,
    project: &TeletextExportProject,
    index: usize,
) -> ObjectEditorFrame {
    frame(
        &mapping(&project.assets[index + 1], project, service).unwrap(),
        limits(),
        deadline(),
    )
    .unwrap()
}

#[test]
fn resource_tti_fragment_first_middle_last_scalar_keeps_original_ordinals_and_reprojects() {
    let original = large_service();
    let project = fragment_project(&original);
    assert_eq!(project.assets.len(), 4);
    let mut all = Vec::new();
    for index in 0..3 {
        let frame = member(&original, &project, index);
        let position = frame
            .command_positions
            .iter()
            .find(|position| position.command >= 3)
            .unwrap();
        let before = write_teletext_tti(&frame.document, limits().0).unwrap();
        let edited = mutate(
            &frame,
            position.editor_word,
            (words(&frame)[position.editor_word] & 0x7ff) | (u32::from(b'B') << 11),
        );
        let changes = reconcile(
            &project.assets[index + 1],
            &project,
            &original,
            &before,
            &edited,
            limits(),
            deadline(),
        )
        .unwrap();
        assert_eq!(changes.len(), 1);
        assert_eq!(changes[0].command(), position.command);
        all.extend(changes);
    }
    authorize(&project, &original, &all, limits(), deadline()).unwrap();
    let mut commands = original
        .declared()
        .semantic()
        .objects()
        .definition(identity(false))
        .unwrap()
        .program
        .commands()
        .to_vec();
    for change in &all {
        commands[change.command()] = TeletextEnhancementCommand::AtColumn {
            column: change.column(),
            operation: change.replacement(),
        };
    }
    let candidate = compile_commands(false, 2, b'A', TeletextObjectType::Passive, Some(commands));
    for index in 0..3 {
        let frame = member(&candidate, &project, index);
        let bytes = write_teletext_tti(&frame.document, limits().0).unwrap();
        validate_applied(
            &project.assets[index + 1],
            &project,
            &candidate,
            &bytes,
            limits(),
            deadline(),
        )
        .unwrap();
        assert!(validate_applied(
            &project.assets[index + 1],
            &project,
            &original,
            &bytes,
            limits(),
            deadline()
        )
        .is_err());
    }
    let repeated = [all[0], all[0]];
    assert!(authorize(&project, &original, &repeated, limits(), deadline()).is_err());
}

#[test]
fn resource_tti_fragment_refuses_missing_duplicate_swapped_overlap_gap_and_old_profile() {
    let service = large_service();
    let original = fragment_project(&service);
    let change =
        TeletextResourceObjectOperandOverlay::new(identity(false), 3, 8, glyph(b'A'), glyph(b'B'))
            .unwrap();
    for variant in [
        "missing",
        "duplicate",
        "swap",
        "overlap",
        "gap",
        "count",
        "profile",
        "resource",
    ] {
        let mut project = original.clone();
        match variant {
            "missing" => {
                project.assets.pop();
            }
            "duplicate" => project.assets.push(project.assets[1].clone()),
            "swap" => {
                let first = project.assets[1].scope.clone();
                project.assets[1].scope = project.assets[2].scope.clone();
                project.assets[2].scope = first;
            }
            "profile" => "qteletextmaker-1.0-carrier-v6".clone_into(&mut project.editor_profile),
            _ => {
                let TeletextExportAssetScope::ObjectTtiFragmentV1 {
                    command_start,
                    command_end,
                    fragment_count,
                    resource_name,
                    ..
                } = &mut project.assets[2].scope
                else {
                    unreachable!()
                };
                match variant {
                    "overlap" => *command_start -= 1,
                    "gap" => *command_end -= 1,
                    "count" => *fragment_count += 1,
                    _ => "missing.bin".clone_into(resource_name),
                }
            }
        }
        assert!(
            authorize(&project, &service, &[change], limits(), deadline()).is_err(),
            "{variant}"
        );
    }
}

#[test]
fn resource_tti_fragment_rejects_wrapper_hidden_source_and_protected_description() {
    let service = large_service();
    let project = fragment_project(&service);
    let frame = member(&service, &project, 0);
    let baseline = write_teletext_tti(&frame.document, limits().0).unwrap();
    let protected = mutate(&frame, 0, words(&frame)[0] ^ 0x800);
    assert!(reconcile(
        &project.assets[1],
        &project,
        &service,
        &baseline,
        &protected,
        limits(),
        deadline()
    )
    .is_err());
    let hidden =
        TeletextResourceObjectOperandOverlay::new(identity(false), 0, 8, glyph(b'A'), glyph(b'B'))
            .unwrap();
    assert!(authorize(&project, &service, &[hidden], limits(), deadline()).is_err());
    let mut records = frame.document.records().to_vec();
    let description = records
        .iter_mut()
        .find(|record| matches!(record, TeletextTtiRecord::Description(_)))
        .unwrap();
    *description = TeletextTtiRecord::Description("complete object execution".to_owned());
    assert!(reconcile(
        &project.assets[1],
        &project,
        &service,
        &baseline,
        &bytes(records),
        limits(),
        deadline()
    )
    .is_err());
}

#[test]
fn resource_tti_fragment_hidden_header_and_wrapper_stay_protected_beside_g2_scalar() {
    let mut commands = original_commands(2, b'A');
    commands.insert(
        1,
        TeletextEnhancementCommand::AtColumn {
            column: 0,
            operation: TeletextColumnOperation::G2 {
                code: TeletextGlyphCode::new(b'A').unwrap(),
            },
        },
    );
    let service = compile_commands(false, 2, b'A', TeletextObjectType::Passive, Some(commands));
    let project = fragment_project(&service);
    assert_eq!(project.assets.len(), 2);
    let frame = member(&service, &project, 0);
    let original = service
        .pages()
        .iter()
        .find_map(|page| match page {
            EncodedTeletextAuthoredPage::Object(page) => Some(page),
            _ => None,
        })
        .unwrap();
    let native = original
        .command_positions()
        .iter()
        .find(|position| position.source().command == 1)
        .unwrap();
    // POP bodies have no display compatibility prefix; the original row-zero
    // position is hidden, while the preceding editor definition stays protected.
    assert_eq!(native.first_word(), native.semantic_word());
    assert!(!original
        .command_positions()
        .iter()
        .any(|position| position.source().command == 0));
    let semantic = scalar_word(&frame, 1);
    let baseline = write_teletext_tti(&frame.document, limits().0).unwrap();
    let original_words = words(&frame);
    let changed_prefix = mutate(&frame, semantic - 1, original_words[semantic - 1] ^ 0x800);
    assert!(reconcile(
        &project.assets[1],
        &project,
        &service,
        &baseline,
        &changed_prefix,
        limits(),
        deadline()
    )
    .is_err());
    let changed_scalar = mutate(
        &frame,
        semantic,
        (original_words[semantic] & 0x7ff) | (u32::from(b'B') << 11),
    );
    let changes = reconcile(
        &project.assets[1],
        &project,
        &service,
        &baseline,
        &changed_scalar,
        limits(),
        deadline(),
    )
    .unwrap();
    assert_eq!(changes.len(), 1);
    assert_eq!(changes[0].command(), 1);
    authorize(&project, &service, &changes, limits(), deadline()).unwrap();
}
