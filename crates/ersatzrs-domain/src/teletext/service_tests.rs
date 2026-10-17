//! Synthetic resolved-service integration; not packet, rendering or publication proof.

use super::*;

fn address(page: u16, subcode: u16) -> TeletextPageAddress {
    TeletextPageAddress {
        page: TeletextPageNumber::new(page).unwrap(),
        subcode: TeletextSubcode::new(subcode).unwrap(),
    }
}

fn raw(page: u8) -> TeletextRawDataPageNumber {
    TeletextRawDataPageNumber::new(2, page).unwrap()
}

fn resource(kind: TeletextResourceKind, page: u8, subpage: u8) -> TeletextResourceIdentity {
    TeletextResourceIdentity::new(
        kind,
        raw(page),
        TeletextResourceSubpageIndex::new(subpage).unwrap(),
    )
    .unwrap()
}

fn public(page: u8, subpage: u8, id: u16) -> TeletextObjectIdentity {
    TeletextObjectIdentity::public(
        resource(TeletextResourceKind::Pop, page, subpage),
        TeletextObjectId::new(id),
    )
    .unwrap()
}

fn global(page: u8, id: u16) -> TeletextObjectIdentity {
    TeletextObjectIdentity::global(
        resource(TeletextResourceKind::Gpop, page, 0),
        TeletextObjectId::new(id),
    )
    .unwrap()
}

fn program(commands: Vec<TeletextEnhancementCommand>) -> TeletextEnhancementProgram {
    TeletextEnhancementProgram::new(commands).unwrap()
}

fn invoke(
    target: TeletextObjectIdentity,
    object_type: TeletextObjectType,
) -> TeletextEnhancementCommand {
    TeletextEnhancementCommand::InvokeObject {
        target,
        object_type,
        offset: None,
    }
}

fn definition(
    identity: TeletextObjectIdentity,
    object_type: TeletextObjectType,
    commands: Vec<TeletextEnhancementCommand>,
) -> TeletextEnhancementObjectDefinition {
    TeletextEnhancementObjectDefinition {
        identity,
        object_type,
        program: program(commands),
    }
}

fn charset(code: u8) -> TeletextEnhancementCommand {
    TeletextEnhancementCommand::AtColumn {
        column: 0,
        operation: TeletextColumnOperation::DesignateCharset {
            designation: TeletextEnhancementCharsetDesignation::new(code).unwrap(),
        },
    }
}

fn glyph_reference(slot: u8) -> TeletextDrcsGlyphReference {
    TeletextDrcsGlyphReference {
        table: TeletextDrcsTableScope::Normal,
        subtable: TeletextDrcsSubtableIndex::new(0).unwrap(),
        slot: TeletextDrcsGlyphSlot::new(slot).unwrap(),
    }
}

fn drcs(slot: u8) -> TeletextEnhancementCommand {
    TeletextEnhancementCommand::AtColumn {
        column: 0,
        operation: TeletextColumnOperation::Drcs {
            reference: glyph_reference(slot),
        },
    }
}

fn table(page: u8, subpage: u8, bitmap: u16) -> TeletextDrcsSubtable {
    TeletextDrcsSubtable::new(
        resource(TeletextResourceKind::Drcs, page, subpage),
        vec![TeletextDrcsGlyphDefinition {
            slot: TeletextDrcsGlyphSlot::new(0).unwrap(),
            bitmap: TeletextDrcsBitmap::new([bitmap; 10]).unwrap(),
        }],
    )
    .unwrap()
}

fn presentation(primary: u8, marker: u8) -> TeletextLevel25PresentationDefaults {
    TeletextLevel25PresentationDefaults {
        primary: TeletextPageCharsetDesignation::new(primary).unwrap(),
        secondary: None,
        palette: TeletextLevel25Palette {
            clut2: [TeletextRgb::new([marker, 0, 0]).unwrap(); 8],
            clut3: [TeletextRgb::new([0, marker, 0]).unwrap(); 8],
        },
        screen_colour: TeletextColourReference::new(marker).unwrap(),
        row_colour: TeletextColourReference::new(0).unwrap(),
        black_background_substitution: false,
        remapping: TeletextColourRemapping::FgClut0BgClut0,
        panels: TeletextSidePanels {
            left: false,
            right: false,
            status: TeletextPanelStatus::Level25And35,
            width: TeletextPanelWidth::new(0).unwrap(),
        },
    }
}

fn enhancement(address: TeletextPageAddress) -> TeletextResolvedPageEnhancementDraft {
    TeletextResolvedPageEnhancementDraft {
        address,
        presentation: None,
        compositional_links: None,
        local_program: None,
        resources: TeletextResolvedPageResourceSelection {
            objects: TeletextResolvedObjectPageLinks {
                public: None,
                global: None,
            },
            drcs: TeletextDrcsPageLinks::new(None, None).unwrap(),
            defaults: [None; 4],
        },
    }
}

fn draft(addresses: &[TeletextPageAddress]) -> TeletextResolvedServiceDraft {
    let language = TeletextLanguage::new("nld".to_owned()).unwrap();
    TeletextResolvedServiceDraft {
        base: TeletextPageSetDraft {
            language: language.clone(),
            initial_page: addresses[0].page,
            pages: addresses
                .iter()
                .map(|address| TeletextPageDraft {
                    address: *address,
                    kind: TeletextPageKind::FullPage,
                    language: language.clone(),
                    national_option: TeletextNationalOption::new(0).unwrap(),
                    rows: vec![],
                    navigation: vec![],
                })
                .collect(),
        },
        pages: addresses.iter().copied().map(enhancement).collect(),
        magazines: vec![],
        objects: vec![],
        drcs: vec![],
    }
}

fn budget() -> TeletextServiceBudget {
    TeletextServiceBudget {
        max_records: 1024,
        max_objects: 512,
        max_structure_items: 8192,
        max_base_content_bytes: 4096,
        max_commands: 8192,
        max_collected_commands: 8192,
        max_collected_drcs_references: 4096,
    }
}

fn selection(
    target: TeletextObjectIdentity,
    object_type: TeletextObjectType,
) -> TeletextDefaultObjectSelection {
    TeletextDefaultObjectSelection {
        target,
        object_type,
    }
}

#[test]
fn presentation_precedence_is_exact_to_subcode_and_preserves_header_fallback() {
    let first = address(200, 1);
    let sibling = address(200, 2);
    let header = address(300, 0);
    let mut input = draft(&[first, sibling, header]);
    input.pages[0].presentation = Some(presentation(0x37, 1));
    input.magazines.push(TeletextMagazinePresentationDraft {
        magazine: 2,
        presentation: presentation(0x20, 2),
    });
    input.base.pages[2].national_option = TeletextNationalOption::new(6).unwrap();
    let original_base = input.base.clone();
    let service = TeletextSemanticService::new(input, budget()).unwrap();
    assert_eq!(service.base(), &original_base);
    assert!(
        matches!(service.effective_presentation(first), Some(TeletextEffectivePresentation::Extension { source: TeletextPresentationSource::Page, defaults }) if *defaults == presentation(0x37, 1))
    );
    assert!(
        matches!(service.effective_presentation(sibling), Some(TeletextEffectivePresentation::Extension { source: TeletextPresentationSource::Magazine, defaults }) if *defaults == presentation(0x20, 2))
    );
    assert_eq!(
        service.effective_presentation(header),
        Some(TeletextEffectivePresentation::Header {
            national_option: TeletextNationalOption::new(6).unwrap()
        })
    );
    assert_eq!(
        service
            .effective_presentation(header)
            .unwrap()
            .primary_designation()
            .unwrap()
            .get(),
        6
    );
    assert_eq!(service.effective_presentation(address(200, 3)), None);
    assert_eq!(
        service.collected(header).unwrap().0,
        TeletextEffectiveProgramSource::BaseOnly
    );
}

#[test]
fn reserved_header_seven_rejects_but_explicit_assigned_aliases_ending_seven_work() {
    let display = address(200, 0);
    let mut input = draft(&[display]);
    input.base.pages[0].national_option = TeletextNationalOption::new(7).unwrap();
    assert_eq!(
        TeletextSemanticService::new(input.clone(), budget()),
        Err(TeletextServiceError::Charset {
            display,
            error: TeletextCharsetValueError::PageDesignation { code: 7 }
        })
    );
    for primary in [0x37, 0x47, 0x57] {
        input.pages[0].presentation = Some(presentation(primary, 0));
        let service = TeletextSemanticService::new(input.clone(), budget()).unwrap();
        assert_eq!(
            service
                .effective_presentation(display)
                .unwrap()
                .primary_designation()
                .unwrap()
                .get(),
            primary
        );
    }
    input.pages[0].presentation = None;
    input.magazines.push(TeletextMagazinePresentationDraft {
        magazine: 2,
        presentation: presentation(0x37, 0),
    });
    assert!(TeletextSemanticService::new(input, budget()).is_ok());
}

#[test]
fn present_empty_local_program_suppresses_default_dependencies_and_display_budgets() {
    let display = address(200, 0);
    let target = global(0xB0, 1);
    let mut input = draft(&[display]);
    input.pages[0].local_program = Some(program(vec![]));
    input.pages[0].resources.objects.global = Some(raw(0xB0));
    input.pages[0].resources.defaults[0] = Some(selection(target, TeletextObjectType::Active));
    input.objects.push(definition(
        target,
        TeletextObjectType::Active,
        vec![charset(0x37), charset(0x20), drcs(0)],
    ));
    let limits = TeletextServiceBudget {
        max_collected_commands: 0,
        max_collected_drcs_references: 0,
        ..budget()
    };
    let service = TeletextSemanticService::new(input.clone(), limits).unwrap();
    let (source, refs) = service.collected(display).unwrap();
    assert_eq!(*source, TeletextEffectiveProgramSource::Local);
    assert_eq!(refs.command_count(), 0);
    assert!(refs.references().is_empty());
    assert!(refs.object_references().is_empty());
    assert_eq!(
        refs.charset_pairs().iter().copied().collect::<Vec<_>>(),
        vec![TeletextCharsetPair::LatinLatin]
    );
    input.pages[0].local_program = None;
    assert!(
        matches!(TeletextSemanticService::new(input, budget()), Err(TeletextServiceError::Collection { program_source: TeletextEffectiveProgramSource::Defaults { slots }, error: TeletextEnhancementCollectionError { reason: TeletextEnhancementCollectionFailure::CharsetPairs { .. }, .. } }) if slots == vec![0])
    );
}

#[test]
fn absent_local_program_applies_defaults_in_sparse_slot_order() {
    let display = address(200, 0);
    let targets = [
        global(0xB0, 1),
        global(0xB0, 2),
        public(0xB1, 0, 3),
        public(0xB1, 0, 4),
    ];
    let mut input = draft(&[display]);
    input.pages[0].resources.objects = TeletextResolvedObjectPageLinks {
        global: Some(raw(0xB0)),
        public: Some(raw(0xB1)),
    };
    for target in targets {
        input
            .objects
            .push(definition(target, TeletextObjectType::Passive, vec![]));
    }
    for slots in [vec![0, 1, 2, 3], vec![1, 3]] {
        input.pages[0].resources.defaults = [None; 4];
        for slot in &slots {
            input.pages[0].resources.defaults[*slot as usize] = Some(selection(
                targets[*slot as usize],
                TeletextObjectType::Passive,
            ));
        }
        let service = TeletextSemanticService::new(input.clone(), budget()).unwrap();
        let (source, refs) = service.collected(display).unwrap();
        assert_eq!(
            *source,
            TeletextEffectiveProgramSource::Defaults {
                slots: slots.clone()
            }
        );
        assert_eq!(refs.command_count(), slots.len());
        assert_eq!(
            refs.object_references()
                .iter()
                .map(|reference| reference.target)
                .collect::<Vec<_>>(),
            slots
                .iter()
                .map(|slot| targets[*slot as usize])
                .collect::<Vec<_>>()
        );
        assert_eq!(
            refs.object_references()
                .iter()
                .map(|reference| reference.source_location.command)
                .collect::<Vec<_>>(),
            (0..slots.len()).collect::<Vec<_>>()
        );
    }
}

#[test]
fn default_roots_keep_original_owners_with_different_or_absent_explicit_links() {
    let display = address(200, 0);
    let mot_public = public(0xa0, 1, 600);
    let mot_global = global(0xb0, 700);
    for with_explicit_links in [false, true] {
        let mut input = draft(&[display]);
        input.pages[0].resources.defaults[0] =
            Some(selection(mot_global, TeletextObjectType::Active));
        input.pages[0].resources.defaults[2] =
            Some(selection(mot_public, TeletextObjectType::Passive));
        input.objects = vec![
            definition(mot_global, TeletextObjectType::Active, vec![]),
            definition(mot_public, TeletextObjectType::Passive, vec![]),
        ];
        if with_explicit_links {
            input.pages[0].resources.objects.public = Some(raw(0xc0));
            input.pages[0].resources.objects.global = Some(raw(0xd0));
            input.objects.extend([
                definition(public(0xc0, 0, 600), TeletextObjectType::Passive, vec![]),
                definition(global(0xd0, 700), TeletextObjectType::Active, vec![]),
            ]);
        }
        let service = TeletextSemanticService::new(input, budget()).unwrap();
        let (source, refs) = service.collected(display).unwrap();
        assert_eq!(
            source,
            &TeletextEffectiveProgramSource::Defaults { slots: vec![0, 2] }
        );
        assert_eq!(refs.command_count(), 2);
        assert_eq!(
            refs.object_references()
                .iter()
                .map(|reference| reference.target)
                .collect::<Vec<_>>(),
            vec![mot_global, mot_public]
        );
    }
}

#[test]
fn independent_default_owner_does_not_exempt_local_or_nested_explicit_invocations() {
    let display = address(200, 0);
    let original = public(0xa0, 1, 600);
    let selected = public(0xb0, 0, 600);
    let mut input = draft(&[display]);
    input.pages[0].resources.objects.public = Some(raw(0xb0));
    input.objects = vec![
        definition(original, TeletextObjectType::Passive, vec![]),
        definition(selected, TeletextObjectType::Passive, vec![]),
    ];
    input.pages[0].local_program =
        Some(program(vec![invoke(original, TeletextObjectType::Passive)]));
    assert!(
        matches!(TeletextSemanticService::new(input.clone(), budget()), Err(TeletextServiceError::ObjectSelection { target, source_location: TeletextEnhancementSource { object: None, command: 0 }, .. }) if target == original)
    );

    let default_root = global(0xc0, 900);
    input.pages[0].local_program = None;
    input.pages[0].resources.defaults[0] =
        Some(selection(default_root, TeletextObjectType::Active));
    input.objects.push(definition(
        default_root,
        TeletextObjectType::Active,
        vec![invoke(original, TeletextObjectType::Passive)],
    ));
    assert!(
        matches!(TeletextSemanticService::new(input, budget()), Err(TeletextServiceError::ObjectSelection { target, source_location: TeletextEnhancementSource { object: Some(owner), command: 0 }, .. }) if target == original && owner == default_root)
    );
}

fn compositional_link(
    target: TeletextX27LinkTarget,
    mask: u16,
    validity: TeletextX27LinkValidity,
) -> TeletextX27LinkDeclaration {
    TeletextX27LinkDeclaration::new(target, validity, TeletextSubpageRequirementMask::new(mask))
        .unwrap()
}

fn compositional_links(
    role: TeletextX27LinkRole,
    declaration: TeletextX27LinkDeclaration,
) -> TeletextX27Level25Links {
    let null = compositional_link(
        TeletextX27LinkTarget::Unspecified {
            magazine: TeletextX27Magazine::new(8).unwrap(),
        },
        0xffff,
        TeletextX27LinkValidity::Level25Only,
    );
    let mut fields = [null; 4];
    fields[usize::from(role.wire_code())] = declaration;
    TeletextX27Level25Links::new(fields[0], fields[1], fields[2], fields[3]).unwrap()
}

#[test]
fn effective_resource_usage_requires_announced_s1_for_all_four_roles() {
    let display = address(200, 0x10);
    for role in TeletextX27LinkRole::ALL {
        for subpage in 0..16 {
            let mut input = draft(&[display]);
            let identity = resource(role.kind(), 0xa0, subpage);
            let command = match role {
                TeletextX27LinkRole::Pop | TeletextX27LinkRole::Gpop => {
                    let target = if role == TeletextX27LinkRole::Pop {
                        TeletextObjectIdentity::public(identity, TeletextObjectId::new(600))
                            .unwrap()
                    } else {
                        TeletextObjectIdentity::global(identity, TeletextObjectId::new(600))
                            .unwrap()
                    };
                    input
                        .objects
                        .push(definition(target, TeletextObjectType::Passive, vec![]));
                    if role == TeletextX27LinkRole::Pop {
                        input.pages[0].resources.objects.public = Some(raw(0xa0));
                    } else {
                        input.pages[0].resources.objects.global = Some(raw(0xa0));
                    }
                    invoke(target, TeletextObjectType::Passive)
                }
                TeletextX27LinkRole::Drcs | TeletextX27LinkRole::Gdrcs => {
                    input.drcs.push(
                        TeletextDrcsSubtable::new(
                            identity,
                            vec![TeletextDrcsGlyphDefinition {
                                slot: TeletextDrcsGlyphSlot::new(0).unwrap(),
                                bitmap: TeletextDrcsBitmap::new([0; 10]).unwrap(),
                            }],
                        )
                        .unwrap(),
                    );
                    let scope = if role == TeletextX27LinkRole::Drcs {
                        input.pages[0].resources.drcs =
                            TeletextDrcsPageLinks::new(Some(raw(0xa0)), None).unwrap();
                        TeletextDrcsTableScope::Normal
                    } else {
                        input.pages[0].resources.drcs =
                            TeletextDrcsPageLinks::new(None, Some(raw(0xa0))).unwrap();
                        TeletextDrcsTableScope::Global
                    };
                    TeletextEnhancementCommand::AtColumn {
                        column: 0,
                        operation: TeletextColumnOperation::Drcs {
                            reference: TeletextDrcsGlyphReference {
                                table: scope,
                                subtable: TeletextDrcsSubtableIndex::new(subpage).unwrap(),
                                slot: TeletextDrcsGlyphSlot::new(0).unwrap(),
                            },
                        },
                    }
                }
            };
            input.pages[0].local_program = Some(program(vec![command]));
            for validity in [
                TeletextX27LinkValidity::Level25Only,
                TeletextX27LinkValidity::Level25And35,
                TeletextX27LinkValidity::Level35Only,
            ] {
                for mask in [0, 1 << subpage] {
                    input.pages[0].compositional_links = Some(compositional_links(
                        role,
                        compositional_link(
                            TeletextX27LinkTarget::Page { page: raw(0xa0) },
                            mask,
                            validity,
                        ),
                    ));
                    let result = TeletextSemanticService::new(input.clone(), budget());
                    if mask == 0 && validity.applies_at_level25() {
                        assert!(
                            matches!(result, Err(TeletextServiceError::CompositionalUsage {
                            display: owner, role: actual_role, page, subpage: actual_s1,
                            source_location: TeletextEnhancementSource { object: None, command: 0 },
                        }) if owner == display && actual_role == role && page == raw(0xa0) && actual_s1.get() == subpage)
                        );
                    } else {
                        result.unwrap();
                    }
                }
            }
        }
    }
}

#[test]
fn default_root_mask_exception_does_not_exempt_nested_calls_or_glyphs() {
    let display = address(200, 1);
    let root = global(0xa0, 900);
    let mut input = draft(&[display]);
    input.pages[0].resources.defaults[0] = Some(selection(root, TeletextObjectType::Active));
    input.pages[0].resources.objects.global = Some(raw(0xa0));
    input.pages[0].compositional_links = Some(compositional_links(
        TeletextX27LinkRole::Gpop,
        compositional_link(
            TeletextX27LinkTarget::Page { page: raw(0xa0) },
            0,
            TeletextX27LinkValidity::Level25Only,
        ),
    ));
    input
        .objects
        .push(definition(root, TeletextObjectType::Active, vec![]));
    TeletextSemanticService::new(input.clone(), budget()).unwrap();
    let child = global(0xa0, 901);
    input
        .objects
        .push(definition(child, TeletextObjectType::Passive, vec![]));
    input.objects[0].program = program(vec![invoke(child, TeletextObjectType::Passive)]);
    assert!(
        matches!(TeletextSemanticService::new(input.clone(), budget()), Err(TeletextServiceError::CompositionalUsage {
        role: TeletextX27LinkRole::Gpop,
        source_location: TeletextEnhancementSource { object: Some(owner), command: 0 }, ..
    }) if owner == root)
    );

    input.objects[0].program = program(vec![drcs(0)]);
    input.pages[0].resources.drcs = TeletextDrcsPageLinks::new(Some(raw(0xb0)), None).unwrap();
    input.drcs.push(table(0xb0, 0, 0));
    input.pages[0].compositional_links = Some(compositional_links(
        TeletextX27LinkRole::Drcs,
        compositional_link(
            TeletextX27LinkTarget::Page { page: raw(0xb0) },
            0,
            TeletextX27LinkValidity::Level25Only,
        ),
    ));
    assert!(
        matches!(TeletextSemanticService::new(input, budget()), Err(TeletextServiceError::CompositionalUsage {
        role: TeletextX27LinkRole::Drcs,
        source_location: TeletextEnhancementSource { object: Some(owner), command: 0 }, ..
    }) if owner == root)
    );
}

#[test]
fn declaration_mask_does_not_invent_a_resolution_for_a_different_selected_owner() {
    let display = address(200, 0);
    let selected = public(0xa0, 0, 600);
    let mut input = draft(&[display]);
    input.pages[0].resources.objects.public = Some(raw(0xa0));
    input.pages[0].local_program =
        Some(program(vec![invoke(selected, TeletextObjectType::Passive)]));
    input
        .objects
        .push(definition(selected, TeletextObjectType::Passive, vec![]));
    input.pages[0].compositional_links = Some(compositional_links(
        TeletextX27LinkRole::Pop,
        compositional_link(
            TeletextX27LinkTarget::Page { page: raw(0xb0) },
            0,
            TeletextX27LinkValidity::Level25Only,
        ),
    ));
    // This semantic input already supplies selection. A future resolver must
    // prove why it differs; this validator cannot infer a missing MOT flag.
    TeletextSemanticService::new(input, budget()).unwrap();
}

#[test]
fn authored_x27_required_inventory_is_checked_even_without_effective_references() {
    let display = address(200, 0x10);
    for role in TeletextX27LinkRole::ALL {
        let mut input = draft(&[display]);
        let declared = compositional_links(
            role,
            compositional_link(
                TeletextX27LinkTarget::Page { page: raw(0xa0) },
                0x8001,
                TeletextX27LinkValidity::Level25Only,
            ),
        );
        input.pages[0].compositional_links = Some(declared);
        for subpage in [0, 15] {
            assert_eq!(
                TeletextSemanticService::new(input.clone(), budget()),
                Err(TeletextServiceError::CompositionalResource {
                    display,
                    role,
                    page: raw(0xa0),
                    subpage: TeletextResourceSubpageIndex::new(subpage).unwrap(),
                })
            );
            let identity = resource(role.kind(), 0xa0, subpage);
            match role {
                TeletextX27LinkRole::Gpop => input.objects.push(definition(
                    TeletextObjectIdentity::global(identity, TeletextObjectId::new(600)).unwrap(),
                    TeletextObjectType::Passive,
                    vec![],
                )),
                TeletextX27LinkRole::Pop => input.objects.push(definition(
                    TeletextObjectIdentity::public(identity, TeletextObjectId::new(600)).unwrap(),
                    TeletextObjectType::Passive,
                    vec![],
                )),
                TeletextX27LinkRole::Gdrcs | TeletextX27LinkRole::Drcs => input
                    .drcs
                    .push(TeletextDrcsSubtable::new(identity, vec![]).unwrap()),
            }
        }
        let service = TeletextSemanticService::new(input, budget()).unwrap();
        assert_eq!(
            service.page_enhancements()[&display].compositional_links,
            Some(declared)
        );
        assert_eq!(service.collected(display).unwrap().1.command_count(), 0);
    }
}

#[test]
fn null_zero_mask_and_level35_only_metadata_do_not_invent_required_inventory() {
    let display = address(200, 0);
    for role in TeletextX27LinkRole::ALL {
        for (mask, validity) in [
            (0, TeletextX27LinkValidity::Level25Only),
            (0xffff, TeletextX27LinkValidity::Level35Only),
        ] {
            let mut input = draft(&[display]);
            let declared = compositional_links(
                role,
                compositional_link(
                    TeletextX27LinkTarget::Page { page: raw(0xa0) },
                    mask,
                    validity,
                ),
            );
            input.pages[0].compositional_links = Some(declared);
            let service = TeletextSemanticService::new(input, budget()).unwrap();
            assert_eq!(
                service.page_enhancements()[&display].compositional_links,
                Some(declared)
            );
            assert!(service.objects().definitions().is_empty());
            assert!(service.drcs().subtables().is_empty());
        }
    }
}

#[test]
fn x27_required_kind_and_exact_subcode_metadata_cannot_be_hidden_by_other_records() {
    let display = address(200, 0);
    let sibling = address(200, 1);
    let mut input = draft(&[display, sibling]);
    input.pages[1].compositional_links = Some(compositional_links(
        TeletextX27LinkRole::Gdrcs,
        compositional_link(
            TeletextX27LinkTarget::Page { page: raw(0xa0) },
            1,
            TeletextX27LinkValidity::Level25And35,
        ),
    ));
    // The same raw page/S1 exists, but with normal DRCS purpose.
    input.drcs.push(table(0xa0, 0, 0));
    assert_eq!(
        TeletextSemanticService::new(input, budget()),
        Err(TeletextServiceError::CompositionalResource {
            display: sibling,
            role: TeletextX27LinkRole::Gdrcs,
            page: raw(0xa0),
            subpage: TeletextResourceSubpageIndex::ZERO,
        })
    );
}

#[test]
fn local_definitions_require_the_exact_base_owner_and_local_presence() {
    let display = address(200, 1);
    let sibling = address(200, 2);
    let identity = TeletextObjectIdentity::local(sibling, TeletextObjectId::new(1));
    let mut input = draft(&[display]);
    input
        .objects
        .push(definition(identity, TeletextObjectType::Passive, vec![]));
    assert_eq!(
        TeletextSemanticService::new(input.clone(), budget()),
        Err(TeletextServiceError::MissingLocalOwner { identity })
    );
    input
        .base
        .pages
        .push(draft(&[sibling]).base.pages.remove(0));
    input.pages.push(enhancement(sibling));
    assert_eq!(
        TeletextSemanticService::new(input.clone(), budget()),
        Err(TeletextServiceError::LocalPresence { identity })
    );
    input.pages[0].local_program = Some(program(vec![]));
    assert_eq!(
        TeletextSemanticService::new(input.clone(), budget()),
        Err(TeletextServiceError::LocalPresence { identity })
    );
    input.pages[1].local_program = Some(program(vec![]));
    assert!(TeletextSemanticService::new(input, budget()).is_ok());
}

fn nested_resources() -> (TeletextResolvedServiceDraft, [TeletextObjectIdentity; 3]) {
    let display = address(200, 0);
    let targets = [global(0xB0, 1), public(0xB1, 0, 2), public(0xB1, 0, 3)];
    let mut input = draft(&[display]);
    input.pages[0].local_program = Some(program(vec![
        invoke(targets[0], TeletextObjectType::Active),
        invoke(targets[0], TeletextObjectType::Active),
    ]));
    input.pages[0].resources.objects = TeletextResolvedObjectPageLinks {
        global: Some(raw(0xB0)),
        public: Some(raw(0xB1)),
    };
    input.pages[0].resources.drcs = TeletextDrcsPageLinks::new(Some(raw(0xA0)), None).unwrap();
    input.objects = vec![
        definition(
            targets[0],
            TeletextObjectType::Active,
            vec![invoke(targets[1], TeletextObjectType::Adaptive)],
        ),
        definition(
            targets[1],
            TeletextObjectType::Adaptive,
            vec![invoke(targets[2], TeletextObjectType::Passive)],
        ),
        definition(targets[2], TeletextObjectType::Passive, vec![drcs(0)]),
    ];
    input.drcs.push(table(0xA0, 0, 0x801));
    (input, targets)
}

#[test]
fn repeated_transitive_object_calls_preserve_preorder_sources_and_drcs_dependencies() {
    let display = address(200, 0);
    let (input, targets) = nested_resources();
    let service = TeletextSemanticService::new(input, budget()).unwrap();
    let (_, refs) = service.collected(display).unwrap();
    assert_eq!(refs.command_count(), 8);
    assert_eq!(refs.references(), &[glyph_reference(0), glyph_reference(0)]);
    assert_eq!(
        refs.sources(),
        &[TeletextEnhancementSource {
            object: Some(targets[2]),
            command: 0
        }; 2]
    );
    let expected = [
        (targets[0], None, 0),
        (targets[1], Some(targets[0]), 0),
        (targets[2], Some(targets[1]), 0),
        (targets[0], None, 1),
        (targets[1], Some(targets[0]), 0),
        (targets[2], Some(targets[1]), 0),
    ];
    assert_eq!(
        refs.object_references()
            .iter()
            .map(|reference| (
                reference.target,
                reference.source_location.object,
                reference.source_location.command
            ))
            .collect::<Vec<_>>(),
        expected
    );
    for kind in [
        TeletextObjectType::Active,
        TeletextObjectType::Adaptive,
        TeletextObjectType::Passive,
    ] {
        assert_eq!(refs.invocation_count(kind), 2);
    }
}

#[test]
fn nested_object_outside_selected_public_page_reports_its_parent_source() {
    let display = address(200, 0);
    let (mut input, targets) = nested_resources();
    let outsider = public(0xB2, 0, 3);
    input.objects[1].program = program(vec![invoke(outsider, TeletextObjectType::Passive)]);
    input.objects[2].identity = outsider;
    assert_eq!(
        TeletextSemanticService::new(input, budget()),
        Err(TeletextServiceError::ObjectSelection {
            display,
            source_location: TeletextEnhancementSource {
                object: Some(targets[1]),
                command: 0
            },
            target: outsider,
        })
    );
}

#[test]
fn transitive_missing_drcs_glyph_reports_object_source_without_mutating_input() {
    let display = address(200, 0);
    let (mut input, targets) = nested_resources();
    input.objects[2].program = program(vec![drcs(1)]);
    let original = input.clone();
    let error = TeletextSemanticService::new(input.clone(), budget()).unwrap_err();
    assert_eq!(input, original);
    assert!(
        matches!(error, TeletextServiceError::CollectedDrcs { program_source: TeletextEffectiveProgramSource::Local, error: TeletextCollectedDrcsError { source_location: Some(TeletextEnhancementSource { object: Some(identity), command: 0 }), error: TeletextDrcsResourceError::MissingGlyph { display: owner, reference: 0, slot, .. } } } if identity == targets[2] && owner == display && slot.get() == 1)
    );
}

#[test]
fn combined_resource_index_counts_tables_once_and_rejects_cross_kind_same_s1() {
    let mut input = draft(&[address(200, 0)]);
    input.objects = vec![
        definition(public(0xA0, 0, 1), TeletextObjectType::Passive, vec![]),
        definition(public(0xA0, 0, 2), TeletextObjectType::Passive, vec![]),
    ];
    let two = TeletextServiceBudget {
        max_records: 2,
        ..budget()
    };
    assert!(TeletextSemanticService::new(input.clone(), two).is_ok());
    assert_eq!(
        TeletextSemanticService::new(
            input.clone(),
            TeletextServiceBudget {
                max_records: 1,
                ..budget()
            }
        ),
        Err(TeletextServiceError::Budget {
            dimension: "record",
            maximum: 1
        })
    );
    input.drcs.push(table(0xA0, 0, 1));
    assert_eq!(
        TeletextSemanticService::new(input.clone(), budget()),
        Err(TeletextServiceError::ResourceCollision {
            page: raw(0xA0),
            subpage: TeletextResourceSubpageIndex::new(0).unwrap()
        })
    );
    input.drcs[0] = table(0xA0, 1, 1);
    assert!(TeletextSemanticService::new(
        input.clone(),
        TeletextServiceBudget {
            max_records: 3,
            ..budget()
        }
    )
    .is_ok());
    assert_eq!(
        TeletextSemanticService::new(input, two),
        Err(TeletextServiceError::Budget {
            dimension: "record",
            maximum: 2
        })
    );
}

#[test]
fn caller_input_budgets_charge_suppressed_authored_work_and_utf8_bytes_exactly() {
    let display = address(200, 0);
    let target = global(0xB0, 1);
    let mut input = draft(&[display]);
    input.base.pages[0].rows.push(TeletextRow {
        row: 1,
        runs: vec![TeletextRun {
            column: 0,
            style: TeletextStyle::default(),
            content: TeletextRunContent::Text {
                text: "é".to_owned(),
            },
        }],
    });
    input.pages[0].local_program = Some(program(vec![]));
    input.pages[0].resources.objects.global = Some(raw(0xB0));
    input.pages[0].resources.defaults[0] = Some(selection(target, TeletextObjectType::Passive));
    input.objects.push(definition(
        target,
        TeletextObjectType::Passive,
        vec![charset(0)],
    ));
    input.drcs.push(table(0xA0, 0, 1));
    // Base + metadata + table + row + run + glyph = six structural items.
    let exact = TeletextServiceBudget {
        max_records: 3,
        max_objects: 1,
        max_structure_items: 6,
        max_base_content_bytes: 2,
        max_commands: 2,
        max_collected_commands: 0,
        max_collected_drcs_references: 0,
    };
    assert!(TeletextSemanticService::new(input.clone(), exact).is_ok());
    for (dimension, limited, maximum) in [
        (
            "record",
            TeletextServiceBudget {
                max_records: 2,
                ..exact
            },
            2,
        ),
        (
            "object",
            TeletextServiceBudget {
                max_objects: 0,
                ..exact
            },
            0,
        ),
        (
            "structure item",
            TeletextServiceBudget {
                max_structure_items: 5,
                ..exact
            },
            5,
        ),
        (
            "base content byte",
            TeletextServiceBudget {
                max_base_content_bytes: 1,
                ..exact
            },
            1,
        ),
        (
            "authored command",
            TeletextServiceBudget {
                max_commands: 1,
                ..exact
            },
            1,
        ),
    ] {
        assert_eq!(
            TeletextSemanticService::new(input.clone(), limited),
            Err(TeletextServiceError::Budget { dimension, maximum })
        );
    }
}

#[test]
fn expanded_work_and_references_share_one_budget_across_two_base_pages() {
    let first = address(200, 0);
    let second = address(201, 0);
    let target = public(0xB0, 0, 1);
    let mut input = draft(&[first, second]);
    for page in &mut input.pages {
        page.local_program = Some(program(vec![invoke(target, TeletextObjectType::Passive)]));
        page.resources.objects.public = Some(raw(0xB0));
        page.resources.drcs = TeletextDrcsPageLinks::new(Some(raw(0xA0)), None).unwrap();
    }
    input.objects.push(definition(
        target,
        TeletextObjectType::Passive,
        vec![drcs(0)],
    ));
    input.drcs.push(table(0xA0, 0, 1));
    let exact = TeletextServiceBudget {
        max_collected_commands: 4,
        max_collected_drcs_references: 2,
        ..budget()
    };
    let service = TeletextSemanticService::new(input.clone(), exact).unwrap();
    for display in [first, second] {
        let refs = &service.collected(display).unwrap().1;
        assert_eq!(refs.command_count(), 2);
        assert_eq!(refs.invocation_count(TeletextObjectType::Passive), 1);
        assert_eq!(refs.references(), &[glyph_reference(0)]);
    }
    assert!(
        matches!(TeletextSemanticService::new(input.clone(), TeletextServiceBudget { max_collected_commands: 3, ..exact }), Err(TeletextServiceError::Collection { error: TeletextEnhancementCollectionError { display, source_location: TeletextEnhancementSource { object: Some(identity), command: 0 }, reason: TeletextEnhancementCollectionFailure::CommandBudget { maximum: 1 } }, .. }) if display == second && identity == target)
    );
    assert!(
        matches!(TeletextSemanticService::new(input, TeletextServiceBudget { max_collected_drcs_references: 1, ..exact }), Err(TeletextServiceError::Collection { error: TeletextEnhancementCollectionError { display, source_location: TeletextEnhancementSource { object: Some(identity), command: 0 }, reason: TeletextEnhancementCollectionFailure::ReferenceBudget { maximum: 0 } }, .. }) if display == second && identity == target)
    );
}

#[test]
fn suppressed_defaults_and_unused_declared_links_still_require_valid_targets() {
    let display = address(200, 0);
    let target = public(0xB0, 0, 1);
    let mut input = draft(&[display]);
    input.pages[0].local_program = Some(program(vec![]));
    input.pages[0].resources.objects.public = Some(raw(0xB0));
    assert_eq!(
        TeletextSemanticService::new(input.clone(), budget()),
        Err(TeletextServiceError::ObjectLink {
            display,
            kind: TeletextResourceKind::Pop,
            page: raw(0xB0)
        })
    );
    input
        .objects
        .push(definition(target, TeletextObjectType::Passive, vec![]));
    input.pages[0].resources.defaults[0] = Some(selection(target, TeletextObjectType::Passive));
    assert_eq!(
        TeletextSemanticService::new(input.clone(), budget()),
        Err(TeletextServiceError::DefaultScope {
            display,
            slot: 0,
            target
        })
    );
    input.pages[0].resources.defaults = [None; 4];
    input.pages[0].resources.defaults[2] = Some(selection(target, TeletextObjectType::Active));
    assert!(
        matches!(TeletextSemanticService::new(input.clone(), budget()), Err(TeletextServiceError::Collection { program_source: TeletextEffectiveProgramSource::Defaults { slots }, error: TeletextEnhancementCollectionError { reason: TeletextEnhancementCollectionFailure::Invocation(TeletextObjectInvocationError::Type { expected: TeletextObjectType::Active, actual: TeletextObjectType::Passive, .. }), .. } }) if slots == vec![2])
    );
    let missing = public(0xB0, 0, 99);
    input.pages[0].resources.defaults[2] = Some(selection(missing, TeletextObjectType::Passive));
    assert!(
        matches!(TeletextSemanticService::new(input.clone(), budget()), Err(TeletextServiceError::Collection { error: TeletextEnhancementCollectionError { reason: TeletextEnhancementCollectionFailure::Invocation(TeletextObjectInvocationError::Missing { target }), .. }, .. }) if target == missing)
    );
    input.pages[0].resources.defaults = [None; 4];
    input.pages[0].resources.drcs = TeletextDrcsPageLinks::new(Some(raw(0xA0)), None).unwrap();
    assert_eq!(
        TeletextSemanticService::new(input, budget()),
        Err(TeletextServiceError::Drcs(
            TeletextDrcsResourceError::MissingPage {
                display,
                table: TeletextDrcsTableScope::Normal,
                page: raw(0xA0)
            }
        ))
    );
}

#[test]
fn effective_page_primary_controls_transitive_charset_pair_constraint() {
    let display = address(200, 0);
    let target = public(0xB0, 0, 1);
    let mut input = draft(&[display]);
    input.pages[0].local_program = Some(program(vec![invoke(target, TeletextObjectType::Passive)]));
    input.pages[0].resources.objects.public = Some(raw(0xB0));
    input.objects.push(definition(
        target,
        TeletextObjectType::Passive,
        vec![charset(0x20), charset(0x21)],
    ));
    input.magazines.push(TeletextMagazinePresentationDraft {
        magazine: 2,
        presentation: presentation(0x20, 0),
    });
    let service = TeletextSemanticService::new(input.clone(), budget()).unwrap();
    assert_eq!(
        service.collected(display).unwrap().1.charset_pairs().len(),
        2
    );
    input.pages[0].presentation = Some(presentation(0x55, 0));
    assert!(
        matches!(TeletextSemanticService::new(input.clone(), budget()), Err(TeletextServiceError::Collection { error: TeletextEnhancementCollectionError { display: owner, source_location: TeletextEnhancementSource { object: Some(identity), command: 1 }, reason: TeletextEnhancementCollectionFailure::CharsetPairs { pair: TeletextCharsetPair::LatinLatin } }, .. }) if owner == display && identity == target)
    );
    input.pages[0].presentation = Some(presentation(0, 0));
    assert!(TeletextSemanticService::new(input, budget()).is_ok());
}

#[test]
fn independent_services_keep_same_identities_with_different_presentation_and_glyphs() {
    let display = address(200, 0);
    let (mut first, _) = nested_resources();
    first.pages[0].presentation = Some(presentation(0, 1));
    let mut second = first.clone();
    second.pages[0].presentation = Some(presentation(0x37, 2));
    second.drcs[0] = table(0xA0, 0, 0x402);
    let first = TeletextSemanticService::new(first, budget()).unwrap();
    let second = TeletextSemanticService::new(second, budget()).unwrap();
    assert_eq!(first.base(), second.base());
    assert_eq!(first.objects(), second.objects());
    assert_eq!(
        first.drcs().subtables()[0].identity(),
        second.drcs().subtables()[0].identity()
    );
    assert_eq!(
        first.drcs().subtables()[0].glyphs()[0].bitmap.get(),
        [0x801; 10]
    );
    assert_eq!(
        second.drcs().subtables()[0].glyphs()[0].bitmap.get(),
        [0x402; 10]
    );
    assert_eq!(
        first
            .effective_presentation(display)
            .unwrap()
            .primary_designation()
            .unwrap()
            .get(),
        0
    );
    assert_eq!(
        second
            .effective_presentation(display)
            .unwrap()
            .primary_designation()
            .unwrap()
            .get(),
        0x37
    );
    assert_ne!(
        first.page_enhancements()[&display].presentation,
        second.page_enhancements()[&display].presentation
    );
    assert_eq!(
        first.collected(display).unwrap().1.references(),
        second.collected(display).unwrap().1.references()
    );
}

#[test]
fn metadata_owners_duplicates_and_magazines_are_checked_without_partial_mutation() {
    let display = address(200, 0);
    let original = draft(&[display]);
    let mut input = original.clone();
    let unknown = address(200, 1);
    input.pages.push(enhancement(unknown));
    assert_eq!(
        TeletextSemanticService::new(input, budget()),
        Err(TeletextServiceError::MissingPage { address: unknown })
    );
    let mut input = original.clone();
    input.pages.push(input.pages[0].clone());
    assert_eq!(
        TeletextSemanticService::new(input, budget()),
        Err(TeletextServiceError::DuplicatePage { address: display })
    );
    for magazine in [0, 9] {
        let mut input = original.clone();
        input.magazines.push(TeletextMagazinePresentationDraft {
            magazine,
            presentation: presentation(0, 0),
        });
        assert_eq!(
            TeletextSemanticService::new(input, budget()),
            Err(TeletextServiceError::Magazine { magazine })
        );
    }
    let mut input = original.clone();
    input.magazines = vec![
        TeletextMagazinePresentationDraft {
            magazine: 2,
            presentation: presentation(0, 0)
        };
        2
    ];
    assert_eq!(
        TeletextSemanticService::new(input, budget()),
        Err(TeletextServiceError::Magazine { magazine: 2 })
    );
    let mut invalid_base = original.clone();
    invalid_base
        .base
        .pages
        .push(invalid_base.base.pages[0].clone());
    assert!(matches!(
        TeletextSemanticService::new(invalid_base, budget()),
        Err(TeletextServiceError::Base(_))
    ));
    assert_eq!(
        TeletextSemanticService::new(original.clone(), budget())
            .unwrap()
            .base(),
        &original.base
    );
}

#[test]
fn effective_program_borrows_original_nonempty_local_program_for_exact_owner() {
    let owner = address(200, 0x1234);
    let mut input = draft(&[owner]);
    input.pages[0].local_program = Some(program(vec![charset(0)]));
    let service = TeletextSemanticService::new(input, budget()).unwrap();
    let before = service.clone();
    let (source, effective) = service.effective_program(owner).unwrap();
    assert_eq!(source, TeletextEffectiveProgramSource::Local);
    let std::borrow::Cow::Borrowed(borrowed) = effective else {
        panic!("expected original local program borrow")
    };
    let stored = service.page_enhancements()[&owner]
        .local_program
        .as_ref()
        .unwrap();
    assert!(std::ptr::eq(borrowed, stored));
    assert_eq!(borrowed.commands(), &[charset(0)]);
    assert_eq!(service, before);
}

#[test]
fn effective_program_borrows_empty_local_override_and_suppresses_declared_defaults() {
    let owner = address(200, 0x1234);
    let target = global(0xb0, 65535);
    let mut input = draft(&[owner]);
    input.pages[0].local_program = Some(program(vec![]));
    input.pages[0].resources.defaults[1] = Some(selection(target, TeletextObjectType::Active));
    input.objects.push(definition(
        target,
        TeletextObjectType::Active,
        vec![charset(0)],
    ));
    let service = TeletextSemanticService::new(
        input,
        TeletextServiceBudget {
            max_collected_commands: 0,
            ..budget()
        },
    )
    .unwrap();
    let (source, effective) = service.effective_program(owner).unwrap();
    assert_eq!(source, TeletextEffectiveProgramSource::Local);
    let std::borrow::Cow::Borrowed(borrowed) = effective else {
        panic!("expected present empty local override borrow")
    };
    assert!(borrowed.commands().is_empty());
    assert!(std::ptr::eq(
        borrowed,
        service.page_enhancements()[&owner]
            .local_program
            .as_ref()
            .unwrap()
    ));
    assert_eq!(service.collected(owner).unwrap().0, source);
}

#[test]
fn effective_program_synthesizes_sparse_defaults_in_canonical_order_with_exact_targets() {
    let owner = address(200, 0x1234);
    let global_target = global(0xb0, 65535);
    let public_target = public(0xb1, 15, 0);
    let mut input = draft(&[owner]);
    // Insert slot3 first to prove physical/editor insertion order is irrelevant.
    input.pages[0].resources.defaults[3] =
        Some(selection(public_target, TeletextObjectType::Passive));
    input.pages[0].resources.defaults[1] =
        Some(selection(global_target, TeletextObjectType::Adaptive));
    input.objects = vec![
        definition(public_target, TeletextObjectType::Passive, vec![]),
        definition(global_target, TeletextObjectType::Adaptive, vec![]),
    ];
    let service = TeletextSemanticService::new(input, budget()).unwrap();
    let before = service.clone();
    let (source, effective) = service.effective_program(owner).unwrap();
    assert_eq!(
        source,
        TeletextEffectiveProgramSource::Defaults { slots: vec![1, 3] }
    );
    assert!(matches!(effective, std::borrow::Cow::Owned(_)));
    assert_eq!(
        effective.commands(),
        &[
            TeletextEnhancementCommand::InvokeObject {
                target: global_target,
                object_type: TeletextObjectType::Adaptive,
                offset: None,
            },
            TeletextEnhancementCommand::InvokeObject {
                target: public_target,
                object_type: TeletextObjectType::Passive,
                offset: None,
            },
        ]
    );
    assert_eq!(service.collected(owner).unwrap().0, source);
    assert_eq!(service, before);
}

#[test]
fn effective_program_is_owned_base_only_with_empty_or_absent_enhancement_metadata() {
    let owner = address(200, 0x1234);
    for omit_metadata in [false, true] {
        let mut input = draft(&[owner]);
        if omit_metadata {
            input.pages.clear();
        }
        let service = TeletextSemanticService::new(input, budget()).unwrap();
        let (source, effective) = service.effective_program(owner).unwrap();
        assert_eq!(source, TeletextEffectiveProgramSource::BaseOnly);
        assert!(matches!(effective, std::borrow::Cow::Owned(_)));
        assert!(effective.commands().is_empty());
        assert_eq!(service.collected(owner).unwrap().0, source);
    }
}

#[test]
fn effective_program_rejects_unknown_exact_page_and_subcode_instead_of_empty_fallback() {
    let owner = address(200, 0x1234);
    let mut input = draft(&[owner]);
    input.pages.clear();
    let service = TeletextSemanticService::new(input, budget()).unwrap();
    let before = service.clone();
    for unknown in [
        address(200, 0x1235),
        address(201, 0x1234),
        address(800, 0x1234),
    ] {
        assert_eq!(
            service.effective_program(unknown),
            Err(TeletextServiceError::MissingPage { address: unknown })
        );
    }
    assert_eq!(service, before);
}

#[path = "object_companion_service_tests.rs"]
mod object_companion_service_tests;

#[path = "local_program_operand_overlay_tests.rs"]
mod local_program_operand_overlay_tests;

#[path = "drcs_bitmap_overlay_tests.rs"]
mod drcs_bitmap_overlay_tests;

#[path = "local_object_operand_overlay_tests.rs"]
mod local_object_operand_overlay_tests;

#[path = "local_object_glyph_program_tests.rs"]
mod local_object_glyph_program_tests;

#[path = "resource_object_operand_tests.rs"]
mod resource_object_operand_tests;
