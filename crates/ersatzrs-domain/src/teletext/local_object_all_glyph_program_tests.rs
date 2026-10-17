//! Complete typed glyph structure; no packet, font, editor or publication proof.
use super::*;

fn reference(table: TeletextDrcsTableScope, subtable: u8, slot: u8) -> TeletextColumnOperation {
    TeletextColumnOperation::Drcs {
        reference: TeletextDrcsGlyphReference {
            table,
            subtable: TeletextDrcsSubtableIndex::new(subtable).unwrap(),
            slot: TeletextDrcsGlyphSlot::new(slot).unwrap(),
        },
    }
}
fn pairs() -> Vec<(TeletextColumnOperation, TeletextColumnOperation)> {
    let code = |code| TeletextGlyphCode::new(code).unwrap();
    vec![
        (
            TeletextColumnOperation::G0 { code: code(b'A') },
            TeletextColumnOperation::G0 { code: code(b'B') },
        ),
        (
            TeletextColumnOperation::G1 {
                mosaic: TeletextMosaic::new(1).unwrap(),
            },
            TeletextColumnOperation::G1 {
                mosaic: TeletextMosaic::new(63).unwrap(),
            },
        ),
        (
            TeletextColumnOperation::G1Alpha {
                code: TeletextG1AlphaCode::new(0x40).unwrap(),
            },
            TeletextColumnOperation::G1Alpha {
                code: TeletextG1AlphaCode::new(0x5f).unwrap(),
            },
        ),
        (
            TeletextColumnOperation::G2 { code: code(0x23) },
            TeletextColumnOperation::G2 { code: code(0x42) },
        ),
        (
            TeletextColumnOperation::G0Diacritic {
                code: code(b'A'),
                diacritic: TeletextDiacriticIndex::new(9).unwrap(),
            },
            TeletextColumnOperation::G0Diacritic {
                code: code(b'B'),
                diacritic: TeletextDiacriticIndex::new(12).unwrap(),
            },
        ),
        (
            TeletextColumnOperation::G3 {
                code: code(0x2f),
                compatibility: TeletextG3Compatibility::Level15,
            },
            TeletextColumnOperation::G3 {
                code: code(0x5f),
                compatibility: TeletextG3Compatibility::Level15,
            },
        ),
        (
            TeletextColumnOperation::G3 {
                code: code(0x2f),
                compatibility: TeletextG3Compatibility::Level25,
            },
            TeletextColumnOperation::G3 {
                code: code(0x5f),
                compatibility: TeletextG3Compatibility::Level25,
            },
        ),
        (
            reference(TeletextDrcsTableScope::Normal, 0, 0),
            reference(TeletextDrcsTableScope::Global, 15, 47),
        ),
    ]
}
fn zero() -> TeletextEnhancementCommand {
    TeletextEnhancementCommand::SetPosition {
        position: TeletextEnhancementPosition::new(0, 0).unwrap(),
    }
}
fn before(operation: TeletextColumnOperation) -> TeletextEnhancementProgram {
    program(vec![
        foreground(0, 1),
        column(0, operation),
        zero(),
        column(0, operation),
        foreground(3, 2),
    ])
}
fn after(old: TeletextColumnOperation, new: TeletextColumnOperation) -> TeletextEnhancementProgram {
    program(vec![
        foreground(0, 1),
        column(0, old),
        zero(),
        column(1, new),
        column(2, old),
        foreground(3, 2),
    ])
}
fn change(
    old: TeletextColumnOperation,
    new: TeletextColumnOperation,
) -> TeletextLocalObjectGlyphProgramOverlay {
    TeletextLocalObjectGlyphProgramOverlay::new_with_all_glyphs(
        target(),
        before(old),
        after(old, new),
        vec![Some(0), None, Some(2), Some(3), None, Some(4)],
    )
    .unwrap()
}

#[test]
fn local_all_glyph_classifier_keeps_every_family_and_g3_compatibility_distinct() {
    use TeletextColumnGlyphFamily as Family;
    for ((before, after), expected) in pairs().into_iter().zip([
        Family::G0,
        Family::G1,
        Family::G1Alpha,
        Family::G2,
        Family::G0Diacritic,
        Family::G3 {
            compatibility: TeletextG3Compatibility::Level15,
        },
        Family::G3 {
            compatibility: TeletextG3Compatibility::Level25,
        },
        Family::Drcs,
    ]) {
        assert_eq!(before.glyph_family(), Some(expected));
        assert_eq!(after.glyph_family(), Some(expected));
    }
    for command in protected_anchors() {
        if let TeletextEnhancementCommand::AtColumn { operation, .. } = command {
            assert_eq!(
                operation.glyph_family().is_some(),
                matches!(operation, TeletextColumnOperation::Drcs { .. })
            );
        }
    }
    assert!(TeletextGlyphCode::new(0x1f).is_err());
    assert!(TeletextGlyphCode::new(0x80).is_err());
    assert!(TeletextG1AlphaCode::new(0x3f).is_err());
    assert!(TeletextG1AlphaCode::new(0x60).is_err());
    assert!(TeletextDiacriticIndex::new(16).is_err());
    assert!(TeletextDrcsSubtableIndex::new(16).is_err());
    assert!(TeletextDrcsGlyphSlot::new(48).is_err());
}

#[test]
fn local_all_glyph_insert_delete_reposition_and_code_change_keep_hidden_context_anchor() {
    for (old, new) in pairs() {
        let overlay = change(old, new);
        assert_eq!(overlay.expected(), &before(old));
        assert_eq!(overlay.replacement(), &after(old, new));
        assert_eq!(overlay.original_command(1), None);
        assert_eq!(overlay.original_command(2), Some(2));
        assert_eq!(overlay.replacement_command(1), None);
        assert_eq!(overlay.original_command(3), Some(3));
        let legacy = matches!(
            old.glyph_family(),
            Some(TeletextColumnGlyphFamily::G0 | TeletextColumnGlyphFamily::G1)
        );
        assert_eq!(overlay.is_legacy_compatible(), legacy);
        assert_eq!(
            TeletextLocalObjectGlyphProgramOverlay::new(
                target(),
                before(old),
                after(old, new),
                overlay.retained_commands().to_vec()
            )
            .is_ok(),
            legacy
        );
        let undo = TeletextLocalObjectGlyphProgramOverlay::new_with_all_glyphs(
            target(),
            after(old, new),
            before(old),
            vec![Some(0), None, Some(2), Some(3), Some(5)],
        )
        .unwrap();
        assert_eq!(undo.replacement(), overlay.expected());
        // The same typed family may be deleted entirely, never its AP00 anchor.
        let deletion = TeletextLocalObjectGlyphProgramOverlay::new_with_all_glyphs(
            target(),
            before(old),
            program(vec![foreground(0, 1), zero(), foreground(3, 2)]),
            vec![Some(0), Some(2), Some(4)],
        )
        .unwrap();
        assert_eq!(deletion.replacement().commands().len(), 3);
    }
}

#[test]
fn local_all_glyphs_keep_legacy_closure_and_reject_mapped_family_or_g3_mode_changes() {
    use TeletextLocalObjectGlyphProgramOverlayError as Error;
    let all = pairs();
    for (index, (old, _)) in all.iter().enumerate() {
        for (other, (new, _)) in all.iter().enumerate() {
            if index == other {
                continue;
            }
            assert_eq!(
                TeletextLocalObjectGlyphProgramOverlay::new_with_all_glyphs(
                    target(),
                    program(vec![column(0, *old)]),
                    program(vec![column(1, *new)]),
                    vec![Some(0)]
                ),
                Err(Error::ProtectedCommand)
            );
        }
    }
    let extended = all[3].0;
    let old = program(vec![column(0, extended), g0(1, b'A')]);
    let new = program(vec![column(0, extended), g0(2, b'B')]);
    let value = TeletextLocalObjectGlyphProgramOverlay::new_with_all_glyphs(
        target(),
        old.clone(),
        new.clone(),
        vec![Some(0), Some(1)],
    )
    .unwrap();
    assert!(
        value.is_legacy_compatible(),
        "an unchanged G2 anchor must not force a new stored version"
    );
    assert!(TeletextLocalObjectGlyphProgramOverlay::new(
        target(),
        old,
        new,
        vec![Some(0), Some(1)]
    )
    .is_ok());
    for (old, new) in &all[2..] {
        assert_eq!(
            TeletextLocalObjectGlyphProgramOverlay::new(
                target(),
                before(*old),
                after(*old, *new),
                vec![Some(0), None, Some(2), Some(3), None, Some(4)]
            ),
            Err(Error::ProtectedCommand)
        );
    }
}

fn input_for(operation: TeletextColumnOperation) -> TeletextResolvedServiceDraft {
    let mut input = input();
    let mut commands = before(operation).commands().to_vec();
    commands.extend([
        column(4, reference(TeletextDrcsTableScope::Normal, 0, 0)),
        invoke(public(0xa0, 0, 1), TeletextObjectType::Passive),
    ]);
    input.objects[0].program = program(commands);
    input.pages[0].resources.drcs =
        TeletextDrcsPageLinks::new(Some(raw(0xb0)), Some(raw(0xb1))).unwrap();
    input.drcs.push(
        TeletextDrcsSubtable::new(
            resource(TeletextResourceKind::Gdrcs, 0xb1, 15),
            vec![TeletextDrcsGlyphDefinition {
                slot: TeletextDrcsGlyphSlot::new(47).unwrap(),
                bitmap: TeletextDrcsBitmap::new([0x801; 10]).unwrap(),
            }],
        )
        .unwrap(),
    );
    input
}
fn service_change(
    old: TeletextColumnOperation,
    new: TeletextColumnOperation,
) -> TeletextLocalObjectGlyphProgramOverlay {
    let expected = input_for(old).objects[0].program.clone();
    let mut commands = after(old, new).commands().to_vec();
    commands.extend_from_slice(&expected.commands()[5..]);
    TeletextLocalObjectGlyphProgramOverlay::new_with_all_glyphs(
        target(),
        expected,
        program(commands),
        vec![
            Some(0),
            None,
            Some(2),
            Some(3),
            None,
            Some(4),
            Some(5),
            Some(6),
        ],
    )
    .unwrap()
}
fn declared(mut input: TeletextResolvedServiceDraft) -> TeletextDeclaredService {
    let link = |target, mask| {
        TeletextX27LinkDeclaration::new(
            target,
            TeletextX27LinkValidity::Level25Only,
            TeletextSubpageRequirementMask::new(mask),
        )
        .unwrap()
    };
    let unused = link(
        TeletextX27LinkTarget::Unspecified {
            magazine: TeletextX27Magazine::new(2).unwrap(),
        },
        0xffff,
    );
    let selected = |page, mask| link(TeletextX27LinkTarget::Page { page: raw(page) }, mask);
    // Exact API role order is GPOP, POP, GDRCS, DRCS.
    input.pages[0].compositional_links = Some(
        TeletextX27Level25Links::new(
            unused,
            selected(0xa0, 1),
            selected(0xb1, 0x8000),
            selected(0xb0, 1),
        )
        .unwrap(),
    );
    TeletextDeclaredService::new(
        service(input),
        vec![unrelated_mot()],
        [address(200, 0), address(200, 1)]
            .map(|display| TeletextDisplayMotSelection { display, mot: None })
            .to_vec(),
        vec![],
        TeletextDeclaredServiceBudget {
            max_mots: 8,
            max_displays: 8,
            max_resources: 64,
            max_defaults: 64,
            max_objects: 64,
        },
    )
    .unwrap()
}

#[test]
fn local_all_glyph_service_reuses_full_collector_and_preserves_all_original_declarations() {
    for (old, new) in pairs() {
        let input = input_for(old);
        let original = declared(input.clone());
        let overlay = service_change(old, new);
        let edited = original
            .clone()
            .with_local_object_glyph_program_overlays(std::slice::from_ref(&overlay), budget())
            .unwrap();
        let mut expected = input;
        expected.objects[0].program = overlay.replacement().clone();
        assert_eq!(edited, declared(expected));
        assert_eq!(edited.mots(), original.mots());
        assert_eq!(edited.selections(), original.selections());
        assert_eq!(edited.resource_inventory(), original.resource_inventory());
        assert_eq!(edited.resolutions(), original.resolutions());
        assert_eq!(edited.defaults(), original.defaults());
        assert_eq!(edited.semantic().base(), original.semantic().base());
        assert_eq!(edited.semantic().drcs(), original.semantic().drcs());
        assert_eq!(
            edited.semantic().physical_base_overlay(address(200, 0)),
            original.semantic().physical_base_overlay(address(200, 0))
        );
        let old_count = original
            .semantic()
            .collected(address(200, 0))
            .unwrap()
            .1
            .command_count();
        let new_count = edited
            .semantic()
            .collected(address(200, 0))
            .unwrap()
            .1
            .command_count();
        assert_eq!(
            new_count,
            old_count + 2,
            "both actual Local invocations must be recollected"
        );
        assert!(matches!(
            edited.with_local_object_glyph_program_overlays(&[overlay], budget()),
            Err(TeletextServiceError::LocalObjectGlyphProgramOverlay {
                reason: TeletextLocalObjectGlyphProgramFailure::ProgramMismatch,
                ..
            })
        ));
    }
}

#[test]
fn local_all_glyph_drcs_references_resolve_original_scope_subtable_and_slot_with_locations() {
    let old = reference(TeletextDrcsTableScope::Normal, 0, 0);
    let original = service(input_for(old));
    for (new, reason) in [
        (
            reference(TeletextDrcsTableScope::Global, 14, 47),
            "subtable",
        ),
        (reference(TeletextDrcsTableScope::Global, 15, 46), "glyph"),
    ] {
        let error = original
            .clone()
            .with_local_object_glyph_program_overlays(&[service_change(old, new)], budget())
            .unwrap_err();
        let TeletextServiceError::CollectedDrcs { error, .. } = error else {
            panic!("original resource qualification must reject")
        };
        assert_eq!(
            error.source_location,
            Some(TeletextEnhancementSource {
                object: Some(target()),
                command: 3
            })
        );
        match (reason, error.error) {
            (
                "subtable",
                TeletextDrcsResourceError::MissingSubtable {
                    display, identity, ..
                },
            ) => {
                assert_eq!(display, address(200, 0));
                assert_eq!(identity, resource(TeletextResourceKind::Gdrcs, 0xb1, 14));
            }
            (
                "glyph",
                TeletextDrcsResourceError::MissingGlyph {
                    display,
                    identity,
                    slot,
                    ..
                },
            ) => {
                assert_eq!(display, address(200, 0));
                assert_eq!(identity, resource(TeletextResourceKind::Gdrcs, 0xb1, 15));
                assert_eq!(slot.get(), 46);
            }
            _ => panic!("exact missing original subtable/glyph diagnostic required"),
        }
    }
    assert_eq!(original.drcs().subtables().len(), 2);
}

#[test]
fn local_all_glyph_overlays_preserve_raw_final_and_expanded_work_limits() {
    let old = reference(TeletextDrcsTableScope::Normal, 0, 0);
    let new = reference(TeletextDrcsTableScope::Global, 15, 47);
    let input = input_for(old);
    let original = service(input.clone());
    let overlay = service_change(old, new);
    let mut authored = budget();
    authored.max_commands = 42; // Original3+7+32, output43, raw15.
    assert!(TeletextSemanticService::new(input.clone(), authored).is_ok());
    assert_eq!(
        original
            .clone()
            .with_local_object_glyph_program_overlays(std::slice::from_ref(&overlay), authored),
        Err(TeletextServiceError::Budget {
            dimension: "authored command",
            maximum: 42
        })
    );
    let mut collected = budget();
    collected.max_collected_commands = original
        .collected(address(200, 0))
        .unwrap()
        .1
        .command_count();
    assert!(TeletextSemanticService::new(input.clone(), collected).is_ok());
    assert!(matches!(
        original
            .clone()
            .with_local_object_glyph_program_overlays(std::slice::from_ref(&overlay), collected),
        Err(TeletextServiceError::Collection {
            error: TeletextEnhancementCollectionError {
                reason: TeletextEnhancementCollectionFailure::CommandBudget { .. },
                ..
            },
            ..
        })
    ));
    let mut references = budget();
    references.max_collected_drcs_references = 6;
    assert!(TeletextSemanticService::new(input, references).is_ok());
    assert!(matches!(
        original.with_local_object_glyph_program_overlays(&[overlay], references),
        Err(TeletextServiceError::Collection {
            error: TeletextEnhancementCollectionError {
                reason: TeletextEnhancementCollectionFailure::ReferenceBudget { .. },
                ..
            },
            ..
        })
    ));
}

#[test]
fn local_all_glyphs_preserve_nonglyph_anchors_and_shared_208_input_bounds() {
    use TeletextLocalObjectGlyphProgramOverlayError as Error;
    let (old, new) = pairs()[3]; // G2, not covered by the strict G0/G1 entrypoint.
    for anchor in protected_anchors().into_iter().chain([zero()]) {
        if matches!(anchor,TeletextEnhancementCommand::AtColumn { operation,.. } if operation.glyph_family().is_some())
        {
            continue; // DRCS is a glyph reference; it is not a context anchor.
        }
        let expected = program(vec![column(0, old), anchor, column(1, old)]);
        for (replacement, mapping) in [
            (vec![column(0, new), column(1, old)], vec![Some(0), Some(2)]),
            (
                vec![column(0, new), anchor, anchor, column(1, old)],
                vec![Some(0), Some(1), None, Some(2)],
            ),
        ] {
            assert_eq!(
                TeletextLocalObjectGlyphProgramOverlay::new_with_all_glyphs(
                    target(),
                    expected.clone(),
                    program(replacement),
                    mapping
                ),
                Err(Error::ProtectedCommand)
            );
        }
    }
    for (expected, replacement, mapping) in [
        (
            vec![column(0, old); 209],
            vec![column(0, new)],
            vec![Some(0)],
        ),
        (
            vec![column(0, old)],
            vec![column(0, new); 209],
            vec![None; 209],
        ),
        (vec![column(0, old)], vec![column(0, new)], vec![None; 209]),
    ] {
        assert_eq!(
            TeletextLocalObjectGlyphProgramOverlay::new_with_all_glyphs(
                target(),
                program(expected),
                program(replacement),
                mapping
            ),
            Err(Error::Capacity)
        );
    }
    let boundary = TeletextLocalObjectGlyphProgramOverlay::new_with_all_glyphs(
        target(),
        program(vec![column(0, old); 208]),
        program(vec![column(0, new); 208]),
        (0..208).map(Some).collect(),
    )
    .unwrap();
    assert!(!boundary.is_legacy_compatible());
}

#[test]
fn local_all_glyph_drcs_insertion_cannot_create_a_missing_original_link() {
    let old = reference(TeletextDrcsTableScope::Normal, 0, 0);
    let mut input = input_for(old);
    input.pages[0].resources.drcs = TeletextDrcsPageLinks::new(Some(raw(0xb0)), None).unwrap();
    let original = service(input);
    let error = original
        .with_local_object_glyph_program_overlays(
            &[service_change(
                old,
                reference(TeletextDrcsTableScope::Global, 15, 47),
            )],
            budget(),
        )
        .unwrap_err();
    assert!(matches!(error,TeletextServiceError::CollectedDrcs {
        program_source: TeletextEffectiveProgramSource::Local,
        error: TeletextCollectedDrcsError { source_location:Some(source),
            error: TeletextDrcsResourceError::MissingLink { display,table:TeletextDrcsTableScope::Global,.. } },
    } if display==address(200,0) && source==TeletextEnhancementSource {object:Some(target()),command:3}));
}
