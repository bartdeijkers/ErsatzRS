//! Typed Flash/charset intentions and reconstruction, not timing or client proof.
use super::*;

fn flash(mode: TeletextFlashMode, rate: TeletextFlashRate) -> TeletextColumnOperation {
    TeletextColumnOperation::Flash { mode, rate }
}
fn charset(code: u8) -> TeletextColumnOperation {
    TeletextColumnOperation::DesignateCharset {
        designation: TeletextEnhancementCharsetDesignation::new(code).unwrap(),
    }
}
fn normal() -> TeletextColumnOperation {
    flash(TeletextFlashMode::Normal, TeletextFlashRate::Slow1Hz)
}
fn inverted() -> TeletextColumnOperation {
    flash(
        TeletextFlashMode::Inverted,
        TeletextFlashRate::Fast2HzPhase3,
    )
}
fn column(column: u8, operation: TeletextColumnOperation) -> TeletextEnhancementCommand {
    TeletextEnhancementCommand::AtColumn { column, operation }
}
fn page_change(
    command: usize,
    column: u8,
    expected: TeletextColumnOperation,
    replacement: TeletextColumnOperation,
) -> TeletextLocalProgramOperandOverlay {
    TeletextLocalProgramOperandOverlay::new(address(200, 0), command, column, expected, replacement)
        .unwrap()
}
fn assert_pair(expected: TeletextColumnOperation, replacement: TeletextColumnOperation) {
    let owner = address(200, 0);
    let identity = target(owner);
    let page =
        TeletextLocalProgramOperandOverlay::new(owner, usize::MAX, 39, expected, replacement);
    let object =
        TeletextLocalObjectOperandOverlay::new(identity, usize::MAX, 39, expected, replacement);
    if expected == replacement {
        assert_eq!(
            page,
            Err(TeletextLocalProgramOperandOverlayError::Unchanged)
        );
        assert_eq!(
            object,
            Err(TeletextLocalObjectOperandOverlayError::Operand(
                TeletextLocalProgramOperandOverlayError::Unchanged
            ))
        );
    } else {
        let page = page.unwrap();
        let object = object.unwrap();
        assert_eq!(
            (page.owner(), page.command(), page.column()),
            (owner, usize::MAX, 39)
        );
        assert_eq!(
            (object.identity(), object.command(), object.column()),
            (identity, usize::MAX, 39)
        );
        assert_eq!(
            (page.expected(), page.replacement()),
            (expected, replacement)
        );
        assert_eq!(
            (object.expected(), object.replacement()),
            (expected, replacement)
        );
        assert_eq!(
            serde_json::from_value::<TeletextColumnOperation>(
                serde_json::to_value(replacement).unwrap()
            )
            .unwrap(),
            replacement
        );
    }
}
#[test]
fn flash_intents_cover_every_typed_mode_rate_transition_and_noop() {
    let modes = [
        TeletextFlashMode::Steady,
        TeletextFlashMode::Normal,
        TeletextFlashMode::Inverted,
        TeletextFlashMode::AdjacentClut,
    ];
    let rates = [
        TeletextFlashRate::Slow1Hz,
        TeletextFlashRate::Fast2HzPhase1,
        TeletextFlashRate::Fast2HzPhase2,
        TeletextFlashRate::Fast2HzPhase3,
        TeletextFlashRate::Fast2HzIncremental,
        TeletextFlashRate::Fast2HzDecremental,
    ];
    let states: Vec<_> = modes
        .into_iter()
        .flat_map(|mode| rates.map(|rate| flash(mode, rate)))
        .collect();
    assert_eq!(states.len(), 24);
    for &before in &states {
        for &after in &states {
            assert_pair(before, after);
        }
    }
}
#[test]
fn charset_intents_preserve_all_assigned_raw_aliases_and_reject_undefined_or_missing_data() {
    for code in 0..=u8::MAX {
        let assigned = matches!(
            code,
            0x00..=0x26 | 0x36 | 0x37 | 0x40 | 0x44 | 0x47 | 0x55 | 0x57
        );
        let parsed = serde_json::from_value::<TeletextColumnOperation>(
            serde_json::json!({"kind":"designate_charset","designation":code}),
        );
        assert_eq!(parsed.is_ok(), assigned, "Table34 rawcode{code}");
        if assigned {
            let expected = charset(u8::from(code == 0));
            let replacement = parsed.unwrap();
            assert_pair(expected, replacement);
            assert_pair(replacement, replacement);
        }
    }
    for value in [
        serde_json::json!({"kind":"designate_charset"}),
        serde_json::json!({"kind":"designate_charset","designation":null}),
        serde_json::json!({"kind":"flash","mode":"normal"}),
        serde_json::json!({"kind":"flash","rate":"slow1_hz"}),
        serde_json::json!({"kind":"flash","mode":"future","rate":"slow1_hz"}),
        serde_json::json!({"kind":"flash","mode":"normal","rate":"future"}),
    ] {
        assert!(serde_json::from_value::<TeletextColumnOperation>(value).is_err());
    }
}
#[test]
fn flash_charset_intents_keep_family_column_and_local_identity_boundaries() {
    for (before, after) in [
        (normal(), charset(0x37)),
        (charset(0), normal()),
        (normal(), foreground(2)),
        (charset(0), g0(b'A')),
    ] {
        assert_eq!(
            TeletextLocalProgramOperandOverlay::new(address(200, 0), 0, 0, before, after),
            Err(TeletextLocalProgramOperandOverlayError::Family)
        );
        assert_eq!(
            TeletextLocalObjectOperandOverlay::new(target(address(200, 0)), 0, 0, before, after),
            Err(TeletextLocalObjectOperandOverlayError::Operand(
                TeletextLocalProgramOperandOverlayError::Family
            ))
        );
    }
    for (before, after) in [(normal(), inverted()), (charset(0), charset(0x37))] {
        assert_eq!(
            TeletextLocalProgramOperandOverlay::new(address(200, 0), 0, 40, before, after),
            Err(TeletextLocalProgramOperandOverlayError::Column { column: 40 })
        );
        for external in [public(0xa0, 0, 65520), global(0xa1, 65520)] {
            assert_eq!(
                TeletextLocalObjectOperandOverlay::new(external, 0, 0, before, after),
                Err(TeletextLocalObjectOperandOverlayError::Scope)
            );
        }
    }
}
fn context_input() -> TeletextResolvedServiceDraft {
    let mut input = super::extended_operand_tests::extended_input();
    let mut page = input.pages[0]
        .local_program
        .as_ref()
        .unwrap()
        .commands()
        .to_vec();
    page.push(column(10, normal()));
    input.pages[0].local_program = Some(program(page));
    let mut object = input.objects[0].program.commands().to_vec();
    object.push(column(2, normal()));
    input.objects[0].program = program(object);
    input
}
#[test]
fn flash_charset_edits_rebuild_full_service_with_original_sources_and_context_qualification() {
    let input = context_input();
    let original = service(input.clone());
    let identity = target(address(200, 0));
    let pages = [
        page_change(1, 8, charset(0x24), charset(0x37)),
        page_change(5, 10, normal(), inverted()),
    ];
    let objects = [
        change(identity, 2, 0, charset(0x10), charset(0x36)),
        change(identity, 6, 2, normal(), inverted()),
    ];
    let edited = original
        .clone()
        .with_local_program_operand_overlays(&pages, budget())
        .unwrap()
        .with_local_object_operand_overlays(&objects, budget())
        .unwrap();
    let reverse = original
        .clone()
        .with_local_object_operand_overlays(&objects, budget())
        .unwrap()
        .with_local_program_operand_overlays(&pages, budget())
        .unwrap();
    assert_eq!(edited, reverse);
    let mut expected = input;
    let mut commands = expected.pages[0]
        .local_program
        .as_ref()
        .unwrap()
        .commands()
        .to_vec();
    commands[1] = column(8, charset(0x37));
    commands[5] = column(10, inverted());
    expected.pages[0].local_program = Some(program(commands));
    let mut commands = expected.objects[0].program.commands().to_vec();
    commands[2] = column(0, charset(0x36));
    commands[6] = column(2, inverted());
    expected.objects[0].program = program(commands);
    assert_eq!(edited, service(expected));
    assert_ne!(edited, original);
    assert_eq!(edited.drcs(), original.drcs());
    assert_eq!(
        edited.effective_program(address(200, 1)).unwrap(),
        original.effective_program(address(200, 1)).unwrap()
    );
}
#[test]
fn charset_edit_still_rejects_a_third_transitive_pair_without_changing_original() {
    let original = service(context_input());
    let before = original.clone();
    let identity = target(address(200, 0));
    let changed = change(identity, 2, 0, charset(0x10), charset(0x37));
    assert!(
        matches!(original.clone().with_local_object_operand_overlays(&[changed],budget()),
        Err(TeletextServiceError::Collection {error:TeletextEnhancementCollectionError {
            source_location:TeletextEnhancementSource {object:Some(owner),command:2},
            reason:TeletextEnhancementCollectionFailure::CharsetPairs {pair:TeletextCharsetPair::GreekGreek},..},..}) if owner==identity)
    );
    assert_eq!(original, before);
}
#[test]
fn flash_charset_edits_reject_missing_or_stale_original_commands() {
    let original = service(context_input());
    let identity = target(address(200, 0));
    let missing = TeletextLocalProgramOperandOverlay::new(
        address(200, 1),
        1,
        8,
        charset(0x24),
        charset(0x37),
    )
    .unwrap();
    assert_eq!(
        original
            .clone()
            .with_local_program_operand_overlays(&[missing], budget()),
        Err(TeletextServiceError::LocalProgramOperandOverlay {
            address: address(200, 1),
            command: 1,
            reason: TeletextLocalProgramOperandFailure::MissingLocalProgram
        })
    );
    let missing = change(target(address(200, 1)), 2, 0, charset(0x10), charset(0x36));
    assert_eq!(
        original
            .clone()
            .with_local_object_operand_overlays(&[missing], budget()),
        Err(TeletextServiceError::LocalObjectOperandOverlay {
            identity: missing.identity(),
            command: 2,
            reason: TeletextLocalObjectOperandFailure::MissingObject
        })
    );
    let page = page_change(5, 10, normal(), inverted());
    let edited = original
        .clone()
        .with_local_program_operand_overlays(&[page], budget())
        .unwrap();
    assert_eq!(
        edited.with_local_program_operand_overlays(&[page], budget()),
        Err(TeletextServiceError::LocalProgramOperandOverlay {
            address: page.owner(),
            command: 5,
            reason: TeletextLocalProgramOperandFailure::CommandMismatch
        })
    );
    let object = change(identity, 6, 2, normal(), inverted());
    let edited = original
        .with_local_object_operand_overlays(&[object], budget())
        .unwrap();
    assert_eq!(
        edited.with_local_object_operand_overlays(&[object], budget()),
        Err(TeletextServiceError::LocalObjectOperandOverlay {
            identity,
            command: 6,
            reason: TeletextLocalObjectOperandFailure::CommandMismatch
        })
    );
}
