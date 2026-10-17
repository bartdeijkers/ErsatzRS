//! Original semantic position anchors survive without inventing a wire word.
use super::*;

fn position(column: u8) -> TeletextEnhancementCommand {
    TeletextEnhancementCommand::SetPosition {
        position: TeletextEnhancementPosition::new(0, column).unwrap(),
    }
}
fn foreground(column: u8) -> TeletextEnhancementCommand {
    TeletextEnhancementCommand::AtColumn {
        column,
        operation: TeletextColumnOperation::Foreground {
            colour: TeletextColourReference::new(2).unwrap(),
        },
    }
}
fn assert_only_leading_anchor_has_no_word(
    service: &EncodedTeletextAuthoredService,
    frame: &ObjectEditorContextFrame,
    commands: usize,
) {
    let object = service
        .pages()
        .iter()
        .find_map(|page| match page {
            ersatzrs_ffmpeg::teletext::EncodedTeletextAuthoredPage::Object(page)
                if Some(page.header().identity()) == target(21).resource() =>
            {
                Some(page)
            }
            _ => None,
        })
        .unwrap();
    assert!(object
        .command_positions()
        .iter()
        .filter(|span| span.source().object == Some(target(21)))
        .map(|span| span.source().command)
        .eq(1..commands));
    let alias = frame.encoded.definition_positions()[0].target();
    assert!(frame
        .encoded
        .command_positions()
        .iter()
        .filter(|span| span.source().object == Some(alias))
        .map(|span| span.source().command)
        .eq(1..commands));
}

#[test]
fn context_original_zero_word_position_anchor_survives_glyph_and_call_structure() {
    let expected = program(vec![position(8), foreground(8), glyph(9, b'A'), invoked(0)]);
    let replacement = program(vec![
        position(8),
        foreground(8),
        glyph(9, b'A'),
        glyph(10, b'B'),
        invoked(0),
        invoked(1),
    ]);
    let original = native_fixture(expected.clone(), true);
    let candidate = native_fixture(replacement.clone(), true);
    let project = receipt(&original);
    let sealed = serde_json::to_vec(&project).unwrap();
    assert_eq!(project.editor_profile, "qteletextmaker-1.0-carrier-v8");
    let baseline =
        build_context(&original, target(21), limits().1, limits().0, deadline()).unwrap();
    let edited = build_context_with_root(
        &original,
        target(21),
        &replacement,
        limits().1,
        limits().0,
        deadline(),
    )
    .unwrap();
    assert_only_leading_anchor_has_no_word(&original, &baseline, expected.commands().len());
    assert_only_leading_anchor_has_no_word(&candidate, &edited, replacement.commands().len());
    assert_eq!(
        baseline.aliases,
        [target(21), target(22), target(23), target(24)]
    );
    assert_eq!(edited.aliases, baseline.aliases);
    let before = files(&original, &project, &baseline.document);
    let after = files(&original, &project, &edited.document);
    assert_eq!(before.len(), after.len());
    for (source, uploaded) in before.iter().zip(&after) {
        assert_eq!(source.name, uploaded.name);
        if source.name == "context.tti" {
            assert_ne!(source.bytes, uploaded.bytes);
        } else {
            assert_eq!(source.bytes, uploaded.bytes);
        }
    }
    let changes =
        reconcile_project(&project, &original, &before, &after, limits(), deadline()).unwrap();
    assert!(changes.operands.is_empty());
    let [change] = changes.programs.as_slice() else {
        panic!("the original position and calls belong to one complete resource program");
    };
    assert_eq!(change.identity(), target(21));
    assert_eq!(change.expected(), &expected);
    assert_eq!(change.replacement(), &replacement);
    // This is canonical correspondence against the authenticated original,
    // not recovery of GUI history. Exact retained FG8/A9 pin the leading
    // anchor before them; no uploaded call straddles that hidden boundary.
    assert_eq!(
        change.retained_commands(),
        [Some(0), Some(1), Some(2), None, Some(3), None]
    );
    assert!(change.has_invocation_structure_changes());
    validate_applied_project(
        &project,
        &original,
        &candidate,
        &after,
        limits(),
        deadline(),
    )
    .unwrap();
    assert_eq!(serde_json::to_vec(&project).unwrap(), sealed);
}

fn assert_missing_positions(
    service: &EncodedTeletextAuthoredService,
    frame: &ObjectEditorContextFrame,
    count: usize,
    missing: &[usize],
) {
    let object = service
        .pages()
        .iter()
        .find_map(|page| match page {
            ersatzrs_ffmpeg::teletext::EncodedTeletextAuthoredPage::Object(page)
                if Some(page.header().identity()) == target(21).resource() =>
            {
                Some(page)
            }
            _ => None,
        })
        .unwrap();
    let emitted = || (0..count).filter(|ordinal| !missing.contains(ordinal));
    assert!(object
        .command_positions()
        .iter()
        .filter(|span| span.source().object == Some(target(21)))
        .map(|span| span.source().command)
        .eq(emitted()));
    let alias = frame.encoded.definition_positions()[0].target();
    assert!(frame
        .encoded
        .command_positions()
        .iter()
        .filter(|span| span.source().object == Some(alias))
        .map(|span| span.source().command)
        .eq(emitted()));
}
fn assert_anchor_roundtrip(
    expected: &TeletextEnhancementProgram,
    replacement: &TeletextEnhancementProgram,
    missing: &[usize],
    retained: &[Option<usize>],
) {
    let original = native_fixture(expected.clone(), true);
    let candidate = native_fixture(replacement.clone(), true);
    let project = receipt(&original);
    let sealed = serde_json::to_vec(&project).unwrap();
    let baseline =
        build_context(&original, target(21), limits().1, limits().0, deadline()).unwrap();
    let edited = build_context_with_root(
        &original,
        target(21),
        replacement,
        limits().1,
        limits().0,
        deadline(),
    )
    .unwrap();
    assert_missing_positions(&original, &baseline, expected.commands().len(), missing);
    assert_missing_positions(&candidate, &edited, replacement.commands().len(), missing);
    assert_eq!(edited.aliases, baseline.aliases);
    let before = files(&original, &project, &baseline.document);
    let after = files(&original, &project, &edited.document);
    for (source, uploaded) in before.iter().zip(&after) {
        assert_eq!(source.name, uploaded.name);
        if source.name != "context.tti" {
            assert_eq!(source.bytes, uploaded.bytes);
        }
    }
    let changes =
        reconcile_project(&project, &original, &before, &after, limits(), deadline()).unwrap();
    assert!(changes.operands.is_empty());
    let [change] = changes.programs.as_slice() else {
        panic!("all original hidden anchors must survive in one resource program");
    };
    assert_eq!(change.identity(), target(21));
    assert_eq!(change.expected(), expected);
    assert_eq!(change.replacement(), replacement);
    assert_eq!(change.retained_commands(), retained);
    assert!(change.has_invocation_structure_changes());
    validate_applied_project(
        &project,
        &original,
        &candidate,
        &after,
        limits(),
        deadline(),
    )
    .unwrap();
    assert_eq!(serde_json::to_vec(&project).unwrap(), sealed);
}
fn multiple_anchors() -> (TeletextEnhancementProgram, TeletextEnhancementProgram) {
    let expected = program(vec![
        position(8),
        foreground(8),
        glyph(9, b'A'),
        position(10),
        foreground(10),
        glyph(11, b'B'),
        invoked(0),
    ]);
    let replacement = program(vec![
        position(8),
        foreground(8),
        glyph(9, b'A'),
        position(10),
        foreground(10),
        glyph(11, b'B'),
        glyph(12, b'C'),
        invoked(0),
        invoked(1),
    ]);
    (expected, replacement)
}

#[test]
fn context_leading_zero_and_multiple_forward_position_anchors_keep_exact_correspondence() {
    assert_anchor_roundtrip(
        &program(vec![position(0), foreground(0), glyph(1, b'A'), invoked(0)]),
        &program(vec![
            position(0),
            foreground(0),
            glyph(1, b'A'),
            glyph(2, b'B'),
            invoked(0),
            invoked(1),
        ]),
        &[0],
        &[Some(0), Some(1), Some(2), None, Some(3), None],
    );
    let (expected, replacement) = multiple_anchors();
    assert_anchor_roundtrip(
        &expected,
        &replacement,
        &[0, 3],
        &[
            Some(0),
            Some(1),
            Some(2),
            Some(3),
            Some(4),
            Some(5),
            None,
            Some(6),
            None,
        ],
    );
}

#[test]
fn context_call_adjacent_position_retains_known_parent_ap_and_original_call_owner() {
    // A9 has already set both physical and semantic AP9. The adjacent original
    // identity position is retained before its exact call, not moved across it.
    assert_anchor_roundtrip(
        &program(vec![foreground(8), glyph(9, b'A'), position(9), invoked(0)]),
        &program(vec![
            foreground(8),
            glyph(9, b'A'),
            position(9),
            invoked(0),
            glyph(10, b'B'),
            invoked(1),
        ]),
        &[2],
        &[Some(0), Some(1), Some(2), Some(3), None, None],
    );
}

#[test]
fn context_hidden_positions_still_charge_semantic_input_budget_and_original_deadline() {
    let (expected, replacement) = multiple_anchors();
    let original = native_fixture(expected.clone(), true);
    let _candidate = native_fixture(replacement.clone(), true);
    let project = receipt(&original);
    let baseline =
        build_context(&original, target(21), limits().1, limits().0, deadline()).unwrap();
    let edited = build_context_with_root(
        &original,
        target(21),
        &replacement,
        limits().1,
        limits().0,
        deadline(),
    )
    .unwrap();
    let before = files(&original, &project, &baseline.document);
    let after = files(&original, &project, &edited.document);
    let mut bounded = limits();
    bounded.2.max_commands = expected.commands().len() + replacement.commands().len() - 1;
    let semantic = original.declared().semantic();
    let authored = semantic
        .objects()
        .definitions()
        .iter()
        .map(|object| object.program.commands().len())
        .sum::<usize>()
        + semantic
            .page_enhancements()
            .values()
            .filter_map(|page| page.local_program.as_ref())
            .map(|program| program.commands().len())
            .sum::<usize>();
    assert!(
        authored <= bounded.2.max_commands,
        "the complete original fits this nonzero limit"
    );
    let error = reconcile_project(&project, &original, &before, &after, bounded, deadline())
        .err()
        .unwrap();
    assert_eq!(error.failure, Failure::Structure);
    let expired = Instant::now()
        .checked_sub(std::time::Duration::from_secs(1))
        .unwrap();
    let error = reconcile_project(&project, &original, &before, &after, limits(), expired)
        .err()
        .unwrap();
    assert_eq!(error.failure, Failure::TimedOut);
}

#[test]
fn context_position_anchor_reconstruction_does_not_make_callee_context_editable() {
    let (expected, replacement) = multiple_anchors();
    let original = native_fixture(expected, true);
    let candidate = native_fixture(replacement.clone(), true);
    let project = receipt(&original);
    let baseline =
        build_context(&original, target(21), limits().1, limits().0, deadline()).unwrap();
    let edited = build_context_with_root(
        &original,
        target(21),
        &replacement,
        limits().1,
        limits().0,
        deadline(),
    )
    .unwrap();
    let before = files(&original, &project, &baseline.document);
    let mut after = files(&original, &project, &edited.document);
    validate_applied_project(
        &project,
        &original,
        &candidate,
        &after,
        limits(),
        deadline(),
    )
    .unwrap();
    assert_eq!(edited.aliases[1], target(22));
    let alias = edited.encoded.definition_positions()[1].target();
    let word = edited
        .encoded
        .command_positions()
        .iter()
        .find(|span| span.source().object == Some(alias) && span.source().command == 0)
        .unwrap()
        .semantic_word();
    let original_word = edited
        .document
        .records()
        .iter()
        .find_map(|record| match record {
            TeletextTtiRecord::Packet {
                row: 26,
                designation,
                words,
            } if usize::from(*designation) == word / 13 => Some(words[word % 13]),
            _ => None,
        })
        .unwrap();
    assert_eq!((original_word >> 6) & 31, 9);
    assert_eq!(original_word >> 11, u32::from(b'P'));
    after
        .iter_mut()
        .find(|file| file.name == "context.tti")
        .unwrap()
        .bytes = mutate(&edited.document, word, original_word ^ (1 << 11));
    let error = reconcile_project(&project, &original, &before, &after, limits(), deadline())
        .err()
        .unwrap();
    assert_eq!(error.failure, Failure::Structure);
    for (source, uploaded) in before.iter().zip(&after) {
        if source.name != "context.tti" {
            assert_eq!(source, uploaded);
        }
    }
}
