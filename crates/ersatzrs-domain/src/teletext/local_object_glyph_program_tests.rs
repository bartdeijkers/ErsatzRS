//! Exact retained Local glyph structure; not editor, packet, raster or publication proof.
use super::*;

fn target() -> TeletextObjectIdentity {
    TeletextObjectIdentity::local(address(200, 0), TeletextObjectId::new(65520))
}
fn column(column: u8, operation: TeletextColumnOperation) -> TeletextEnhancementCommand {
    TeletextEnhancementCommand::AtColumn { column, operation }
}
fn g0(column_index: u8, code: u8) -> TeletextEnhancementCommand {
    column(
        column_index,
        TeletextColumnOperation::G0 {
            code: TeletextGlyphCode::new(code).unwrap(),
        },
    )
}
fn g1(column_index: u8, bits: u8) -> TeletextEnhancementCommand {
    column(
        column_index,
        TeletextColumnOperation::G1 {
            mosaic: TeletextMosaic::new(bits).unwrap(),
        },
    )
}
fn foreground(column_index: u8, colour: u8) -> TeletextEnhancementCommand {
    column(
        column_index,
        TeletextColumnOperation::Foreground {
            colour: TeletextColourReference::new(colour).unwrap(),
        },
    )
}
fn original_commands() -> Vec<TeletextEnhancementCommand> {
    vec![
        foreground(0, 1),
        g0(0, b'A'),
        g0(0, b'A'), // Identical commands need an explicit survivor identity.
        g1(2, 1),
        foreground(3, 2),
        g0(3, b'B'),
        column(
            4,
            TeletextColumnOperation::Drcs {
                reference: glyph_reference(0),
            },
        ),
        invoke(public(0xa0, 0, 1), TeletextObjectType::Passive),
    ]
}
fn replacement_commands() -> Vec<TeletextEnhancementCommand> {
    vec![
        foreground(0, 1),
        g0(0, b'Z'), // Inserted, not misidentified as the repeated original A.
        g0(1, b'A'), // Old command2 survives; old command1 is deleted.
        g1(2, 63),
        g1(2, 2),
        foreground(3, 2),
        g0(3, b'C'),
        column(
            4,
            TeletextColumnOperation::Drcs {
                reference: glyph_reference(0),
            },
        ),
        invoke(public(0xa0, 0, 1), TeletextObjectType::Passive),
    ]
}
fn retained() -> Vec<Option<usize>> {
    vec![
        Some(0),
        None,
        Some(2),
        Some(3),
        None,
        Some(4),
        Some(5),
        Some(6),
        Some(7),
    ]
}
fn overlay(identity: TeletextObjectIdentity) -> TeletextLocalObjectGlyphProgramOverlay {
    TeletextLocalObjectGlyphProgramOverlay::new(
        identity,
        program(original_commands()),
        program(replacement_commands()),
        retained(),
    )
    .unwrap()
}
fn input() -> TeletextResolvedServiceDraft {
    let mut input = draft(&[address(200, 0), address(200, 1)]);
    let mut defaults = presentation(0, 1);
    defaults.panels = TeletextSidePanels {
        left: true,
        right: true,
        status: TeletextPanelStatus::Level25And35,
        width: TeletextPanelWidth::new(4).unwrap(),
    };
    input.pages[0].presentation = Some(defaults);
    input.pages[0].local_program = Some(program(vec![
        TeletextEnhancementCommand::SetPosition {
            position: TeletextEnhancementPosition::new(1, 8).unwrap(),
        },
        TeletextEnhancementCommand::InvokeObject {
            target: target(),
            object_type: TeletextObjectType::Adaptive,
            offset: Some(TeletextObjectOffset::new(0, 32).unwrap()),
        },
        TeletextEnhancementCommand::InvokeObject {
            target: target(),
            object_type: TeletextObjectType::Adaptive,
            offset: Some(TeletextObjectOffset::new(0, 34).unwrap()),
        },
    ]));
    input.pages[0].resources.objects.public = Some(raw(0xa0));
    input.pages[0].resources.drcs = TeletextDrcsPageLinks::new(Some(raw(0xb0)), None).unwrap();
    input.objects = vec![
        definition(target(), TeletextObjectType::Adaptive, original_commands()),
        definition(public(0xa0, 0, 1), TeletextObjectType::Passive, vec![]),
        // An uninvoked original is still charged by the authored-command budget.
        definition(
            TeletextObjectIdentity::local(address(200, 0), TeletextObjectId::new(65519)),
            TeletextObjectType::Passive,
            vec![g0(0, b'U'); 32],
        ),
    ];
    input.drcs.push(table(0xb0, 0, 0x111));
    input.magazines.push(TeletextMagazinePresentationDraft {
        magazine: 3,
        presentation: presentation(0x20, 2),
    });
    input
}
fn service(input: TeletextResolvedServiceDraft) -> TeletextSemanticService {
    let mut original = TeletextSemanticService::new(input, budget()).unwrap();
    original.set_mot_fallback(address(200, 1), presentation(0, 3));
    original
        .with_physical_base_overlays(
            vec![TeletextPhysicalBaseOverlay::new(
                address(200, 0),
                vec![TeletextPhysicalBaseRow::new(6, [b'P'; 40]).unwrap()],
            )
            .unwrap()],
            budget(),
        )
        .unwrap()
}

#[test]
fn local_glyph_structure_explicit_survivors_preserve_context_and_rebuild_both_calls() {
    let before = input();
    let original = service(before.clone());
    let change = overlay(target());
    assert_eq!(change.identity(), target());
    assert_eq!(change.owner(), address(200, 0));
    assert_eq!(change.expected().commands(), original_commands());
    assert_eq!(change.replacement().commands(), replacement_commands());
    assert_eq!(change.retained_commands(), retained());
    assert_eq!(change.original_command(2), Some(2));
    assert_eq!(change.original_command(1), None);
    assert_eq!(change.replacement_command(1), None);
    assert_eq!(change.replacement_command(2), Some(2));
    let edited = original
        .clone()
        .with_local_object_glyph_program_overlays(&[change], budget())
        .unwrap();
    let mut expected = before;
    expected.objects[0].program = program(replacement_commands());
    assert_eq!(edited, service(expected));
    assert_ne!(edited, original);
    assert_eq!(edited.base(), original.base());
    assert_eq!(edited.drcs(), original.drcs());
    assert_eq!(edited.object_companions(), original.object_companions());
    assert_eq!(
        edited.effective_program(address(200, 0)),
        original.effective_program(address(200, 0))
    );
    assert_eq!(
        edited.effective_presentation(address(200, 1)),
        original.effective_presentation(address(200, 1))
    );
    let (_, before_collection) = original.collected(address(200, 0)).unwrap();
    let (_, after_collection) = edited.collected(address(200, 0)).unwrap();
    assert_eq!(before_collection.references().len(), 2);
    assert_eq!(
        after_collection.references(),
        before_collection.references()
    );
    assert_eq!(
        after_collection.command_count(),
        before_collection.command_count() + 2
    );
    assert_eq!(
        after_collection.invocation_count(TeletextObjectType::Passive),
        before_collection.invocation_count(TeletextObjectType::Passive)
    );
    let undo = TeletextLocalObjectGlyphProgramOverlay::new(
        target(),
        program(replacement_commands()),
        program(original_commands()),
        vec![
            Some(0),
            None,
            Some(2),
            Some(3),
            Some(5),
            Some(6),
            Some(7),
            Some(8),
        ],
    )
    .unwrap();
    assert_eq!(
        edited
            .with_local_object_glyph_program_overlays(&[undo], budget())
            .unwrap(),
        original
    );
}

#[test]
fn local_glyph_structure_rejects_nonlocal_noop_capacity_and_invalid_survivor_maps() {
    use TeletextLocalObjectGlyphProgramOverlayError as Error;
    for identity in [public(0xa0, 0, 1), global(0xa1, 1)] {
        assert_eq!(
            TeletextLocalObjectGlyphProgramOverlay::new(
                identity,
                program(original_commands()),
                program(replacement_commands()),
                retained(),
            ),
            Err(Error::Scope)
        );
    }
    assert_eq!(
        TeletextLocalObjectGlyphProgramOverlay::new(
            target(),
            program(original_commands()),
            program(original_commands()),
            (0..8).map(Some).collect(),
        ),
        Err(Error::Unchanged)
    );
    for (old, new, mapping) in [
        (vec![g0(0, b'A'); 209], vec![g0(0, b'B')], vec![Some(0)]),
        (vec![g0(0, b'A')], vec![g0(0, b'B'); 209], vec![None; 209]),
    ] {
        assert_eq!(
            TeletextLocalObjectGlyphProgramOverlay::new(
                target(),
                program(old),
                program(new),
                mapping,
            ),
            Err(Error::Capacity)
        );
    }
    assert!(TeletextLocalObjectGlyphProgramOverlay::new(
        target(),
        program(vec![g0(0, b'A'); 208]),
        program(vec![g0(0, b'B'); 208]),
        (0..208).map(Some).collect(),
    )
    .is_ok());
    assert_eq!(
        TeletextLocalObjectGlyphProgramOverlay::new(
            target(),
            program(vec![g0(0, b'A')]),
            program(vec![g0(0, b'B')]),
            vec![None; 209],
        ),
        Err(Error::Capacity)
    );
    for mapping in [
        vec![],
        vec![Some(0)],
        vec![Some(0), Some(99)],
        vec![Some(1), Some(0)],
        vec![Some(0), Some(0)],
    ] {
        assert_eq!(
            TeletextLocalObjectGlyphProgramOverlay::new(
                target(),
                program(vec![g0(0, b'A'); 2]),
                program(vec![g0(0, b'B'); 2]),
                mapping,
            ),
            Err(Error::Correspondence)
        );
    }
}

fn protected_anchors() -> Vec<TeletextEnhancementCommand> {
    vec![
        foreground(0, 1),
        column(
            0,
            TeletextColumnOperation::Background {
                colour: TeletextColourReference::new(2).unwrap(),
            },
        ),
        column(
            0,
            TeletextColumnOperation::Attributes {
                double_height: false,
                double_width: false,
                boxed: true,
                conceal: false,
                invert: false,
                underline_or_separated_mosaics: false,
            },
        ),
        charset(0x37),
        column(
            0,
            TeletextColumnOperation::Flash {
                mode: TeletextFlashMode::AdjacentClut,
                rate: TeletextFlashRate::Fast2HzPhase2,
            },
        ),
        column(
            0,
            TeletextColumnOperation::Drcs {
                reference: glyph_reference(0),
            },
        ),
        invoke(public(0xa0, 0, 1), TeletextObjectType::Passive),
        TeletextEnhancementCommand::SetPosition {
            position: TeletextEnhancementPosition::new(1, 0).unwrap(),
        },
        TeletextEnhancementCommand::SetScreenColour {
            colour: TeletextColourReference::new(1).unwrap(),
        },
        TeletextEnhancementCommand::SetRowColour {
            row: 1,
            colour: TeletextColourReference::new(2).unwrap(),
            following_rows: true,
        },
    ]
}
#[test]
fn local_glyph_structure_protects_all_original_anchors_and_context_segments() {
    use TeletextLocalObjectGlyphProgramOverlayError as Error;
    for anchor in protected_anchors() {
        let expected = program(vec![g0(0, b'A'), anchor, g0(1, b'A')]);
        for (replacement, mapping) in [
            (vec![g0(0, b'B'), g0(1, b'A')], vec![Some(0), Some(2)]),
            (
                vec![g0(0, b'B'), foreground(0, 31), g0(1, b'A')],
                vec![Some(0), Some(1), Some(2)],
            ),
            (
                vec![g0(0, b'B'), anchor, anchor, g0(1, b'A')],
                vec![Some(0), Some(1), None, Some(2)],
            ),
        ] {
            assert_eq!(
                TeletextLocalObjectGlyphProgramOverlay::new(
                    target(),
                    expected.clone(),
                    program(replacement),
                    mapping,
                ),
                Err(Error::ProtectedCommand)
            );
        }
        // A surviving glyph cannot move across an unchanged context anchor.
        assert_eq!(
            TeletextLocalObjectGlyphProgramOverlay::new(
                target(),
                expected,
                program(vec![g0(0, b'B'), anchor, g0(1, b'A')]),
                vec![Some(2), Some(1), Some(0)],
            ),
            Err(Error::ProtectedCommand)
        );
    }
    for (old, new) in [(g0(0, b'A'), g1(0, 1)), (g1(0, 1), g0(0, b'A'))] {
        assert_eq!(
            TeletextLocalObjectGlyphProgramOverlay::new(
                target(),
                program(vec![old]),
                program(vec![new]),
                vec![Some(0)],
            ),
            Err(Error::ProtectedCommand)
        );
    }
}

#[test]
fn local_glyph_structure_application_rejects_empty_duplicate_missing_and_stale_originals() {
    let original = service(input());
    assert_eq!(
        original
            .clone()
            .with_local_object_glyph_program_overlays(&[], budget()),
        Err(TeletextServiceError::EmptyLocalObjectGlyphProgramOverlays)
    );
    let mut stale = original_commands();
    stale[1] = g0(0, b'Z');
    let stale = TeletextLocalObjectGlyphProgramOverlay::new(
        target(),
        program(stale),
        program(replacement_commands()),
        retained(),
    )
    .unwrap();
    for (changes, identity, reason) in [
        (
            vec![overlay(target()), overlay(target())],
            target(),
            TeletextLocalObjectGlyphProgramFailure::Duplicate,
        ),
        (
            vec![overlay(TeletextObjectIdentity::local(
                address(200, 1),
                TeletextObjectId::new(65520),
            ))],
            TeletextObjectIdentity::local(address(200, 1), TeletextObjectId::new(65520)),
            TeletextLocalObjectGlyphProgramFailure::MissingObject,
        ),
        (
            vec![stale],
            target(),
            TeletextLocalObjectGlyphProgramFailure::ProgramMismatch,
        ),
    ] {
        assert_eq!(
            original
                .clone()
                .with_local_object_glyph_program_overlays(&changes, budget()),
            Err(TeletextServiceError::LocalObjectGlyphProgramOverlay { identity, reason })
        );
    }
}

#[test]
fn local_glyph_structure_reuses_existing_and_final_aggregate_work_budgets() {
    let original = service(input());
    let change = overlay(target());
    for dimension in [
        "record",
        "object",
        "structure item",
        "base content byte",
        "authored command",
        "collected command",
        "collected DRCS reference",
    ] {
        let mut limit = budget();
        match dimension {
            "record" => limit.max_records = 0,
            "object" => limit.max_objects = 0,
            "structure item" => limit.max_structure_items = 0,
            "base content byte" => limit.max_base_content_bytes = 0,
            "authored command" => limit.max_commands = 0,
            "collected command" => limit.max_collected_commands = 0,
            _ => limit.max_collected_drcs_references = 0,
        }
        assert!(
            matches!(original.clone().with_local_object_glyph_program_overlays(std::slice::from_ref(&change), limit), Err(TeletextServiceError::Budget { dimension: actual, .. }) if actual == dimension),
            "{dimension}"
        );
    }
    let mut authored = budget();
    authored.max_commands = 43; // 3 page + 8 target + 32 unrelated; raw overlay17 fits.
    assert!(TeletextSemanticService::new(input(), authored).is_ok());
    assert!(matches!(
        original
            .clone()
            .with_local_object_glyph_program_overlays(std::slice::from_ref(&change), authored),
        Err(TeletextServiceError::Budget {
            dimension: "authored command",
            maximum: 43
        })
    ));
    let mut collected = budget();
    collected.max_collected_commands = original
        .collected(address(200, 0))
        .unwrap()
        .1
        .command_count();
    assert!(TeletextSemanticService::new(input(), collected).is_ok());
    assert!(matches!(
        original.with_local_object_glyph_program_overlays(&[change], collected),
        Err(TeletextServiceError::Collection {
            program_source: TeletextEffectiveProgramSource::Local,
            error: TeletextEnhancementCollectionError {
                display,
                source_location: TeletextEnhancementSource { object: Some(identity), command: 7 },
                reason: TeletextEnhancementCollectionFailure::CommandBudget { maximum },
            },
        }) if display == address(200, 0) && identity == target()
            && maximum == collected.max_collected_commands
    ));
}

fn unrelated_mot() -> TeletextMotLevel25Declaration {
    let unused = TeletextX27LinkTarget::Unspecified {
        magazine: TeletextX27Magazine::new(3).unwrap(),
    };
    let object = TeletextMotObjectLink::new(
        unused,
        TeletextMotSubpageExtent::new(0).unwrap(),
        true,
        TeletextMotFallback::new(0).unwrap(),
        [TeletextMotDefaultObject::NoObject { raw_number: 77 }; 2],
    );
    let drcs = TeletextMotDrcsLink::new(unused, TeletextMotSubpageExtent::new(0).unwrap(), true);
    TeletextMotLevel25Declaration::new(
        TeletextResourceIdentity::new(
            TeletextResourceKind::Mot,
            TeletextRawDataPageNumber::new(3, 0xfe).unwrap(),
            TeletextResourceSubpageIndex::new(9).unwrap(),
        )
        .unwrap(),
        vec![],
        TeletextMotLevel25Links::new(
            object,
            [object; 7],
            drcs,
            [drcs; 7],
            TeletextMotEnhancementCounts::new(0, 0, 0),
        )
        .unwrap(),
    )
    .unwrap()
}

#[test]
fn local_glyph_structure_preserves_declared_mot_inventory_and_resolutions() {
    let mut declared_input = input();
    let unused = TeletextX27LinkDeclaration::new(
        TeletextX27LinkTarget::Unspecified {
            magazine: TeletextX27Magazine::new(2).unwrap(),
        },
        TeletextX27LinkValidity::Level25Only,
        TeletextSubpageRequirementMask::new(0xffff),
    )
    .unwrap();
    let selected = |page| {
        TeletextX27LinkDeclaration::new(
            TeletextX27LinkTarget::Page { page },
            TeletextX27LinkValidity::Level25Only,
            TeletextSubpageRequirementMask::new(1),
        )
        .unwrap()
    };
    // Authenticate both original selected resources with genuine X27/4 links.
    declared_input.pages[0].compositional_links = Some(
        TeletextX27Level25Links::new(unused, selected(raw(0xa0)), unused, selected(raw(0xb0)))
            .unwrap(),
    );
    let original = TeletextDeclaredService::new(
        service(declared_input),
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
    .unwrap();
    assert_eq!(original.mots().len(), 1);
    let edited = original
        .clone()
        .with_local_object_glyph_program_overlays(&[overlay(target())], budget())
        .unwrap();
    assert_ne!(edited.semantic(), original.semantic());
    assert_eq!(edited.mots(), original.mots());
    assert_eq!(edited.selections(), original.selections());
    assert_eq!(edited.empty_object_pages(), original.empty_object_pages());
    assert_eq!(edited.resolutions(), original.resolutions());
    assert_eq!(edited.resource_inventory(), original.resource_inventory());
    assert_eq!(edited.defaults(), original.defaults());
    assert_eq!(edited.semantic().base(), original.semantic().base());
    assert_eq!(edited.semantic().drcs(), original.semantic().drcs());
    assert_eq!(
        edited.semantic().physical_base_overlay(address(200, 0)),
        original.semantic().physical_base_overlay(address(200, 0))
    );
}

#[test]
fn local_glyph_structure_raw_programs_and_map_have_nonzero_preallocation_bounds() {
    // Removing the unrelated32 commands leaves original11/final9, both fitting11.
    // Only the supplied expected8 + replacement6 aggregate exceeds this limit.
    let mut smaller = input();
    smaller.objects.pop();
    let old = original_commands();
    let kept = [0, 2, 4, 5, 6, 7];
    let deletion = TeletextLocalObjectGlyphProgramOverlay::new(
        target(),
        program(old.clone()),
        program(kept.map(|index| old[index]).to_vec()),
        kept.map(Some).to_vec(),
    )
    .unwrap();
    let mut raw = budget();
    raw.max_commands = 11;
    let original = TeletextSemanticService::new(smaller.clone(), raw).unwrap();
    smaller.objects[0].program = deletion.replacement().clone();
    assert!(TeletextSemanticService::new(smaller, raw).is_ok());
    assert!(matches!(
        original.with_local_object_glyph_program_overlays(&[deletion], raw),
        Err(TeletextServiceError::Budget {
            dimension: "authored command",
            maximum: 11,
        })
    ));
    // No physical overlay here: original and final structure each need7 items.
    // The independent incoming correspondence map has9 entries and must refuse8.
    let mut map = budget();
    map.max_structure_items = 8;
    let original = TeletextSemanticService::new(input(), map).unwrap();
    let mut final_input = input();
    final_input.objects[0].program = program(replacement_commands());
    assert!(TeletextSemanticService::new(final_input, map).is_ok());
    assert!(matches!(
        original.with_local_object_glyph_program_overlays(&[overlay(target())], map),
        Err(TeletextServiceError::Budget {
            dimension: "structure item",
            maximum: 8,
        })
    ));
}

#[path = "local_object_all_glyph_program_tests.rs"]
mod all_glyphs;
