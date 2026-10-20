//! Atomic selected-owner changes retain original foreign MOT default roots.
use super::*;

fn child(global: bool, selected: bool) -> TeletextObjectIdentity {
    let kind = if global {
        TeletextResourceKind::Gpop
    } else {
        TeletextResourceKind::Pop
    };
    let page = if global {
        if selected {
            0xb1
        } else {
            0xb0
        }
    } else if selected {
        0xa1
    } else {
        0xa0
    };
    let resource = resource(kind, page, 0);
    if global {
        TeletextObjectIdentity::global(resource, TeletextObjectId::new(10)).unwrap()
    } else {
        TeletextObjectIdentity::public(resource, TeletextObjectId::new(10)).unwrap()
    }
}
fn call(target: TeletextObjectIdentity) -> TeletextEnhancementCommand {
    TeletextEnhancementCommand::InvokeObject {
        target,
        object_type: TeletextObjectType::Passive,
        offset: None,
    }
}
fn links(selected: bool) -> TeletextX27Level25Links {
    let unused = TeletextX27LinkDeclaration::new(
        TeletextX27LinkTarget::Unspecified {
            magazine: TeletextX27Magazine::new(2).unwrap(),
        },
        TeletextX27LinkValidity::Level25Only,
        TeletextSubpageRequirementMask::new(u16::MAX),
    )
    .unwrap();
    let link = |global| {
        TeletextX27LinkDeclaration::new(
            TeletextX27LinkTarget::Page {
                page: child(global, selected).resource().unwrap().page(),
            },
            TeletextX27LinkValidity::Level25Only,
            TeletextSubpageRequirementMask::new(1),
        )
        .unwrap()
    };
    TeletextX27Level25Links::new(link(true), link(false), unused, unused).unwrap()
}
fn positioned(
    identity: TeletextObjectIdentity,
    code: u8,
    column: u8,
) -> TeletextEnhancementObjectDefinition {
    TeletextEnhancementObjectDefinition {
        identity,
        object_type: TeletextObjectType::Passive,
        program: TeletextEnhancementProgram::new(vec![
            TeletextEnhancementCommand::SetPosition {
                position: TeletextEnhancementPosition::new(1, column).unwrap(),
            },
            TeletextEnhancementCommand::AtColumn {
                column,
                operation: TeletextColumnOperation::G0 {
                    code: TeletextGlyphCode::new(code).unwrap(),
                },
            },
        ])
        .unwrap(),
    }
}
fn source(selected: bool, compatibility: bool) -> TeletextResolvedServiceDraft {
    let mut draft = input();
    draft.objects[0] = positioned(child(false, false), b'B', 10);
    let root = &mut draft.objects[1];
    root.object_type = TeletextObjectType::Active;
    let mut commands = root.program.commands().to_vec();
    commands.extend([call(child(false, selected)), call(child(true, selected))]);
    root.program = TeletextEnhancementProgram::new(commands).unwrap();
    draft.objects.extend([
        positioned(child(false, true), b'P', 10),
        positioned(child(true, false), b'G', 12),
        positioned(child(true, true), b'H', 12),
    ]);
    for page in &mut draft.pages {
        page.resources.defaults[2].as_mut().unwrap().object_type = TeletextObjectType::Active;
        page.resources.objects.public = Some(child(false, selected).resource().unwrap().page());
        page.resources.objects.global = Some(child(true, selected).resource().unwrap().page());
        page.compositional_links = Some(links(selected));
    }
    if compatibility {
        draft.base.pages[1].rows.push(TeletextRow {
            row: 2,
            runs: vec![TeletextRun {
                column: 1,
                style: TeletextStyle::default(),
                content: TeletextRunContent::Text { text: "£Z".into() },
            }],
        });
    }
    draft
}
fn declared_selection(draft: TeletextResolvedServiceDraft) -> TeletextDeclaredService {
    let companions = vec![resource(TeletextResourceKind::Pop, 0xa0, 1)];
    let semantic = TeletextSemanticService::new_with_object_companions(
        draft,
        companions.clone(),
        semantic_limits(),
    )
    .unwrap();
    TeletextDeclaredService::new(
        semantic,
        vec![mot_with_type(TeletextObjectType::Active)],
        [0, 1]
            .map(|subcode| TeletextDisplayMotSelection {
                display: owner(subcode),
                mot: Some(resource(TeletextResourceKind::Mot, 0xfe, 9)),
            })
            .to_vec(),
        companions,
        declaration_limits(),
    )
    .unwrap()
}
fn composition(page: &EncodedTeletextAuthoredPage) -> &TeletextMainComposition {
    match page {
        EncodedTeletextAuthoredPage::NativeDisplay(page) => page.composition(),
        EncodedTeletextAuthoredPage::CompiledDisplay(page) => page.normalized().original(),
        _ => panic!("display expected"),
    }
}
fn assert_cells(page: &TeletextMainComposition) {
    assert_eq!(
        page.original_program_source(),
        &TeletextEffectiveProgramSource::Defaults { slots: vec![2] }
    );
    let calls = page.spatial_trace().invocations();
    assert_eq!(
        calls.iter().map(|call| call.target()).collect::<Vec<_>>(),
        [target(20), child(false, true), child(true, true)]
    );
    assert_eq!(
        calls[0].root_source(),
        TeletextSpatialRootSource::DefaultSlot { slot: 2 }
    );
    // Nested positions are relative to the retained caller AP (1,8).
    // Keep that original geometry; the selected bank does not reset the origin.
    for invocation in &calls[1..] {
        assert_eq!(invocation.origin().row(), 1);
        assert_eq!(invocation.origin().column(), 8);
    }
    for (row, column, code) in [(1, 8, b'A'), (2, 18, b'P'), (2, 20, b'H')] {
        let display = page.display_resolution();
        let index = display.origins()[display.cell(row, column).unwrap().origin().unwrap()]
            .candidate_index();
        assert!(
            matches!(page.glyph_intent().records()[index].glyph(),Some(TeletextGlyphIdentity::G0 {glyph}) if glyph.repertoire()==TeletextG0Repertoire::Latin && glyph.code()==code),
            "row={row} column={column} expected={code} actual={:?} calls={:?}",
            page.glyph_intent().records()[index].glyph(),
            calls,
        );
    }
}
fn assert_encoded_preservation(
    actual: &EncodedTeletextAuthoredService,
    oracle: &EncodedTeletextAuthoredService,
    before: &EncodedTeletextAuthoredService,
) {
    assert_eq!(actual.inventory(), before.inventory());
    assert_eq!(actual.unit_count(), oracle.unit_count());
    assert_eq!(actual.magazine_units(), before.magazine_units());
    for ((page, want), old) in actual
        .pages()
        .iter()
        .zip(oracle.pages())
        .zip(before.pages())
    {
        assert_eq!(page.inventory_record(), want.inventory_record());
        assert_eq!(page.data_units(), want.data_units());
        if let Some(header) = page.resource_header() {
            if header.identity() != target(20).resource().unwrap() {
                assert_eq!(page.data_units(), old.data_units());
            }
        } else {
            assert_eq!(
                page.data_units()
                    .iter()
                    .filter(|u| packet(u) <= 25)
                    .collect::<Vec<_>>(),
                old.data_units()
                    .iter()
                    .filter(|u| packet(u) <= 25)
                    .collect::<Vec<_>>()
            );
        }
    }
}

#[test]
fn atomic_object_page_selection_encodes_complete_original_defaults_and_unchanged_base() {
    for compatibility in [false, true] {
        let original = declared_selection(source(false, compatibility));
        let expected = declared_selection(source(true, compatibility));
        let new_program = expected
            .semantic()
            .objects()
            .definition(target(20))
            .unwrap()
            .program
            .clone();
        let old_program = original
            .semantic()
            .objects()
            .definition(target(20))
            .unwrap()
            .program
            .clone();
        let change = TeletextResourceObjectProgramOverlay::new_with_invocation_changes(
            target(20),
            old_program,
            new_program.clone(),
            (0..new_program.commands().len()).map(Some).collect(),
            semantic_limits(),
        )
        .unwrap();
        // New calls under old selection are invalid: there is no sequential edit.
        assert!(original
            .clone()
            .with_resource_object_program_overlays(std::slice::from_ref(&change), semantic_limits())
            .is_err());
        let intents = original
            .resolutions()
            .iter()
            .flat_map(|resolution| {
                [TeletextX27LinkRole::Pop, TeletextX27LinkRole::Gpop].map(|role| {
                    TeletextObjectPageSelectionChange::new(
                        resolution.display(),
                        role,
                        Some(links(false)),
                        *resolution.role(role),
                        links(true).link(role),
                    )
                    .unwrap()
                })
            })
            .collect::<Vec<_>>();
        let changed = original
            .clone()
            .with_object_page_selections_and_program_overlays(
                &intents,
                &[change],
                semantic_limits(),
                declaration_limits(),
            )
            .unwrap();
        assert_eq!(changed, expected);
        assert_eq!(changed.defaults(), original.defaults());
        assert_eq!(changed.mots(), original.mots());
        assert_eq!(changed.selections(), original.selections());
        let tx = transmissions(&original);
        assert_eq!(tx, transmissions(&changed));
        let before = encode_teletext_authored_service(original.clone(), &tx, &limits()).unwrap();
        let actual = encode_teletext_authored_service(changed.clone(), &tx, &limits()).unwrap();
        let oracle = encode_teletext_authored_service(expected, &tx, &limits()).unwrap();
        assert_eq!(actual.declared(), &changed);
        assert_encoded_preservation(&actual, &oracle, &before);
        assert_cells(composition(&actual.pages()[1]));
        let resolution = changed
            .resolutions()
            .iter()
            .find(|r| r.display() == owner(1))
            .unwrap();
        assert_eq!(resolution.defaults()[2].unwrap().target, target(20));
        assert!(
            matches!(resolution.role(TeletextX27LinkRole::Pop),TeletextMotRoleResolution::Selected {original_mot:Some(mot),..} if mot.target == target(20).resource().unwrap().page() && mot.supersedable)
        );
    }
}
