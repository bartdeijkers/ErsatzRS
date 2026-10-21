//! First attachment is distinct from replacing an already selected raw bank.
use super::*;

fn attachment_display() -> TeletextPageAddress {
    TeletextPageAddress {
        page: TeletextPageNumber::new(201).unwrap(),
        subcode: TeletextSubcode::new(0).unwrap(),
    }
}

fn unused_links() -> TeletextX27Level25Links {
    let declaration = TeletextX27LinkDeclaration::new(
        unused(),
        TeletextX27LinkValidity::Level25Only,
        TeletextSubpageRequirementMask::new(u16::MAX),
    )
    .unwrap();
    TeletextX27Level25Links::new(declaration, declaration, declaration, declaration).unwrap()
}

fn attachment_input(
    explicit: bool,
    attached: bool,
    consuming: bool,
) -> TeletextResolvedServiceDraft {
    let mut source = input(true, false);
    for (kind, page, bitmap) in [
        (TeletextResourceKind::Drcs, 0xb0, 0x801),
        (TeletextResourceKind::Gdrcs, 0xb1, 0x803),
    ] {
        source.drcs.push(
            TeletextDrcsSubtable::new(
                identity(kind, page, 0),
                vec![TeletextDrcsGlyphDefinition {
                    slot: TeletextDrcsGlyphSlot::new(0).unwrap(),
                    bitmap: TeletextDrcsBitmap::new([bitmap; 10]).unwrap(),
                }],
            )
            .unwrap(),
        );
    }
    let mut base = source.base.pages[0].clone();
    base.address = attachment_display();
    source.base.pages.push(base);
    let mut commands = vec![TeletextEnhancementCommand::InvokeObject {
        target: target(0xa0),
        object_type: TeletextObjectType::Active,
        offset: None,
    }];
    for (column, table) in [
        (4, TeletextDrcsTableScope::Normal),
        (5, TeletextDrcsTableScope::Global),
    ] {
        commands.push(TeletextEnhancementCommand::AtColumn {
            column,
            operation: TeletextColumnOperation::Drcs {
                reference: TeletextDrcsGlyphReference {
                    table,
                    subtable: TeletextDrcsSubtableIndex::new(0).unwrap(),
                    slot: TeletextDrcsGlyphSlot::new(0).unwrap(),
                },
            },
        });
    }
    source.pages.push(TeletextResolvedPageEnhancementDraft {
        address: attachment_display(),
        presentation: None,
        compositional_links: if attached {
            Some(
                TeletextX27Level25Links::new(
                    link(0xa2, 1),
                    link(0xa0, 1),
                    link(0xb1, 1),
                    link(0xb0, 1),
                )
                .unwrap(),
            )
        } else {
            explicit.then(unused_links)
        },
        local_program: consuming.then(|| TeletextEnhancementProgram::new(commands).unwrap()),
        resources: TeletextResolvedPageResourceSelection {
            objects: TeletextResolvedObjectPageLinks {
                public: attached.then_some(raw(0xa0)),
                global: attached.then_some(raw(0xa2)),
            },
            drcs: TeletextDrcsPageLinks::new(
                attached.then_some(raw(0xb0)),
                attached.then_some(raw(0xb1)),
            )
            .unwrap(),
            defaults: [None; 4],
        },
    });
    source
}

fn attachment_service(explicit: bool, attached: bool, consuming: bool) -> TeletextDeclaredService {
    let template = bind_input(input(true, false));
    let semantic = TeletextSemanticService::new_with_object_companions(
        attachment_input(explicit, attached, consuming),
        template.empty_object_pages().to_vec(),
        limits(),
    )
    .unwrap()
    .with_physical_base_overlays(
        template
            .semantic()
            .physical_base_overlays()
            .values()
            .cloned()
            .collect(),
        limits(),
    )
    .unwrap()
    .with_base_text_spellings(
        template
            .semantic()
            .base_text_spellings()
            .values()
            .copied()
            .collect(),
        limits(),
    )
    .unwrap();
    let mut selections = template.selections().to_vec();
    selections.push(TeletextDisplayMotSelection {
        display: attachment_display(),
        mot: Some(template.mots()[0].identity()),
    });
    TeletextDeclaredService::new(
        semantic,
        template.mots().to_vec(),
        selections,
        template.empty_object_pages().to_vec(),
        budget(),
    )
    .unwrap()
}

#[test]
fn object_page_attachment_qualifies_retained_four_role_banks_without_retarget_authority() {
    for explicit in [false, true] {
        // Both generations pass the complete semantic and declaration factories.
        // Neither fixture is obtained by mutating a trusted returned service.
        let original = attachment_service(explicit, false, false);
        let final_service = attachment_service(explicit, true, false);
        let consuming_service = attachment_service(explicit, true, true);
        let before = original.clone();
        let resolution = |service: &TeletextDeclaredService| {
            *service
                .resolutions()
                .iter()
                .find(|resolution| resolution.display() == attachment_display())
                .unwrap()
        };
        let old_resolution = resolution(&original);
        let final_resolution = resolution(&final_service);
        for (role, page) in [
            (TeletextX27LinkRole::Pop, 0xa0),
            (TeletextX27LinkRole::Gpop, 0xa2),
            (TeletextX27LinkRole::Drcs, 0xb0),
            (TeletextX27LinkRole::Gdrcs, 0xb1),
        ] {
            assert_eq!(
                *old_resolution.role(role),
                TeletextMotRoleResolution::Unselected
            );
            assert_eq!(
                *final_resolution.role(role),
                TeletextMotRoleResolution::Selected {
                    target: raw(page),
                    requirements: TeletextMotResolutionRequirements::X27Mask(
                        TeletextSubpageRequirementMask::new(1),
                    ),
                    provenance: TeletextMotLinkProvenance::X27 {
                        display: attachment_display(),
                        role,
                    },
                    original_mot: None,
                }
            );
            assert_eq!(
                TeletextObjectPageSelectionChange::new(
                    attachment_display(),
                    role,
                    original.semantic().page_enhancements()[&attachment_display()]
                        .compositional_links,
                    *old_resolution.role(role),
                    link(page, 1),
                ),
                Err(TeletextObjectPageSelectionError::InvalidChange)
            );
        }
        assert_eq!(original, before);
        assert_preserved_attachment_context(&original, &final_service);
        assert!(original
            .semantic()
            .collected(attachment_display())
            .unwrap()
            .1
            .references()
            .is_empty());
        assert!(final_service
            .semantic()
            .collected(attachment_display())
            .unwrap()
            .1
            .references()
            .is_empty());
        assert_eq!(
            consuming_service
                .semantic()
                .collected(attachment_display())
                .unwrap()
                .1
                .references()
                .len(),
            2
        );
        assert_associated_mot_is_not_unselected(&original);
    }
}

fn attachment_changes(
    original: &TeletextDeclaredService,
) -> Vec<TeletextObjectPageSelectionChange> {
    [
        (TeletextX27LinkRole::Pop, 0xa0),
        (TeletextX27LinkRole::Gpop, 0xa2),
        (TeletextX27LinkRole::Drcs, 0xb0),
        (TeletextX27LinkRole::Gdrcs, 0xb1),
    ]
    .map(|(role, page)| {
        TeletextObjectPageSelectionChange::new_attachment(
            attachment_display(),
            role,
            original.semantic().page_enhancements()[&attachment_display()].compositional_links,
            link(page, 1),
        )
        .unwrap()
    })
    .to_vec()
}

#[test]
fn object_page_attachment_applies_four_retained_banks_and_rejects_stale_missing_or_bounded_inputs()
{
    use TeletextObjectPageSelectionError as Error;
    for explicit in [false, true] {
        let original = attachment_service(explicit, false, false);
        let changes = attachment_changes(&original);
        let apply = |changes: &[TeletextObjectPageSelectionChange], service_budget| {
            original
                .clone()
                .with_object_page_selections_and_program_overlays(
                    changes,
                    &[],
                    service_budget,
                    budget(),
                )
        };
        assert_eq!(
            apply(&changes, limits()).unwrap(),
            attachment_service(explicit, true, false)
        );
        assert!(changes
            .iter()
            .all(|change| !change.is_unselected_restoration()
                && change.restored_unselected_links().is_none()));
        assert!(matches!(
            apply(&[changes[0], changes[0]], limits()),
            Err(Error::Duplicate { .. })
        ));
        let stale = TeletextObjectPageSelectionChange::new_attachment(
            attachment_display(),
            TeletextX27LinkRole::Pop,
            if explicit { None } else { Some(unused_links()) },
            link(0xa0, 1),
        )
        .unwrap();
        assert!(matches!(
            apply(&[stale], limits()),
            Err(Error::OriginalMismatch { .. })
        ));
        for (role, page) in [
            (TeletextX27LinkRole::Pop, 0xab),
            (TeletextX27LinkRole::Drcs, 0xb1),
        ] {
            let missing = TeletextObjectPageSelectionChange::new_attachment(
                attachment_display(),
                role,
                changes[0].expected_links(),
                link(page, 1),
            )
            .unwrap();
            assert!(matches!(
                apply(&[missing], limits()),
                Err(Error::MissingResource { .. })
            ));
        }
        let missing = TeletextObjectPageSelectionChange::new_attachment(
            owner(1),
            TeletextX27LinkRole::Pop,
            None,
            link(0xa0, 1),
        )
        .unwrap();
        assert!(matches!(
            apply(&[missing], limits()),
            Err(Error::MissingDisplay { .. })
        ));
        assert!(matches!(
            apply(
                &changes,
                TeletextServiceBudget {
                    max_structure_items: 0,
                    ..limits()
                }
            ),
            Err(Error::Service(TeletextServiceError::Budget { .. }))
        ));
        // A selected original is not made unselected by using the new constructor.
        let selected_original = TeletextObjectPageSelectionChange::new_attachment(
            owner(0),
            TeletextX27LinkRole::Pop,
            Some(unused_links()),
            link(0xa1, 1),
        )
        .unwrap();
        assert!(matches!(
            apply(&[selected_original], limits()),
            Err(Error::OriginalMismatch { .. })
        ));
    }
}

#[test]
fn object_page_attachment_restoration_is_exact_transient_undo_not_rebuild_authority() {
    use TeletextObjectPageSelectionError as Error;
    for explicit in [false, true] {
        let original = attachment_service(explicit, false, false);
        let changes = attachment_changes(&original);
        let selected = attachment_service(explicit, true, false);
        let metadata = &selected.semantic().page_enhancements()[&attachment_display()];
        let resolution = selected
            .resolutions()
            .iter()
            .find(|r| r.display() == attachment_display())
            .unwrap();
        for attachment in changes {
            let role = attachment.role();
            let selected_role = *resolution.role(role);
            let restoration = TeletextObjectPageSelectionChange::new_unselected_restoration(
                attachment,
                metadata.compositional_links,
                selected_role,
            )
            .unwrap();
            assert!(restoration.is_unselected_restoration());
            assert_eq!(
                restoration.restored_unselected_links(),
                Some(attachment.expected_links())
            );
            assert_eq!(restoration.expected_links(), metadata.compositional_links);
            assert_eq!(restoration.expected_role(), selected_role);
            assert_eq!(restoration.replacement(), unused_links().link(role));
            assert_eq!(
                selected
                    .clone()
                    .with_object_page_selections_and_program_overlays(
                        &[restoration],
                        &[],
                        limits(),
                        budget(),
                    ),
                Err(Error::InvalidChange)
            );
            assert_eq!(
                selected.clone().with_object_page_selections_and_consumers(
                    &[restoration],
                    TeletextObjectPageSelectionConsumers::default(),
                    limits(),
                    budget(),
                ),
                Err(Error::InvalidChange)
            );
            assert_restoration_rejections(
                attachment,
                metadata.compositional_links,
                selected_role,
                restoration,
            );
        }
    }
    let selected = bind_input(input(true, false));
    assert_eq!(
        TeletextObjectPageSelectionChange::new_unselected_restoration(
            changes(&selected)[0],
            selected.semantic().page_enhancements()[&owner(0)].compositional_links,
            *selected.resolutions()[0].role(TeletextX27LinkRole::Gpop),
        ),
        Err(Error::InvalidChange)
    );
}

#[test]
fn object_page_attachment_constructor_keeps_explicit_validity_and_unused_original_guards() {
    use TeletextObjectPageSelectionError as Error;
    let alternate = TeletextX27LinkDeclaration::new(
        TeletextX27LinkTarget::Page { page: raw(0xa0) },
        TeletextX27LinkValidity::Level25And35,
        TeletextSubpageRequirementMask::new(1),
    )
    .unwrap();
    let level35 = TeletextX27LinkDeclaration::new(
        TeletextX27LinkTarget::Page { page: raw(0xa0) },
        TeletextX27LinkValidity::Level35Only,
        TeletextSubpageRequirementMask::new(1),
    )
    .unwrap();
    for (original, replacement) in [
        (None, alternate),
        (None, level35),
        (None, unused_links().link(TeletextX27LinkRole::Pop)),
        (Some(unused_links()), alternate),
        (Some(links(0xa0, 0xa2, 1)), link(0xa1, 1)),
    ] {
        assert_eq!(
            TeletextObjectPageSelectionChange::new_attachment(
                attachment_display(),
                TeletextX27LinkRole::Pop,
                original,
                replacement,
            ),
            Err(Error::InvalidChange)
        );
    }
    // Explicit unused declarations preserve their original mask, validity and
    // magazine; only an absent root synthesizes the canonical unused spelling.
    let unused = TeletextX27LinkDeclaration::new(
        TeletextX27LinkTarget::Unspecified {
            magazine: TeletextX27Magazine::new(3).unwrap(),
        },
        TeletextX27LinkValidity::Level25And35,
        TeletextSubpageRequirementMask::new(u16::MAX),
    )
    .unwrap();
    let root = TeletextX27Level25Links::new(unused, unused, unused, unused).unwrap();
    let attachment = TeletextObjectPageSelectionChange::new_attachment(
        attachment_display(),
        TeletextX27LinkRole::Pop,
        Some(root),
        alternate,
    )
    .unwrap();
    let current = TeletextX27Level25Links::new(unused, alternate, unused, unused).unwrap();
    let selected = TeletextMotRoleResolution::Selected {
        target: raw(0xa0),
        requirements: TeletextMotResolutionRequirements::X27Mask(alternate.required_subpages()),
        provenance: TeletextMotLinkProvenance::X27 {
            display: attachment_display(),
            role: TeletextX27LinkRole::Pop,
        },
        original_mot: None,
    };
    let restoration = TeletextObjectPageSelectionChange::new_unselected_restoration(
        attachment,
        Some(current),
        selected,
    )
    .unwrap();
    assert_eq!(restoration.replacement(), unused);
    assert_eq!(restoration.restored_unselected_links(), Some(Some(root)));
}

fn assert_preserved_attachment_context(
    original: &TeletextDeclaredService,
    final_service: &TeletextDeclaredService,
) {
    assert_eq!(original.mots(), final_service.mots());
    assert_eq!(original.defaults(), final_service.defaults());
    assert_eq!(original.selections(), final_service.selections());
    assert_eq!(
        original.resource_inventory(),
        final_service.resource_inventory()
    );
    assert_eq!(
        original.empty_object_pages(),
        final_service.empty_object_pages()
    );
    assert_eq!(original.semantic().base(), final_service.semantic().base());
    assert_eq!(
        original.semantic().objects(),
        final_service.semantic().objects()
    );
    assert_eq!(original.semantic().drcs(), final_service.semantic().drcs());
    assert_eq!(
        original.semantic().physical_base_overlays(),
        final_service.semantic().physical_base_overlays()
    );
    assert_eq!(
        original.semantic().base_text_spellings(),
        final_service.semantic().base_text_spellings()
    );
    for display in [owner(0), owner(32)] {
        assert_eq!(
            original.semantic().page_enhancements()[&display],
            final_service.semantic().page_enhancements()[&display]
        );
        assert_eq!(
            original.semantic().collected(display),
            final_service.semantic().collected(display)
        );
    }
}

fn assert_associated_mot_is_not_unselected(original: &TeletextDeclaredService) {
    // Unspecified X27 over a live MOT candidate is unresolved, not an
    // alternative spelling of the independently unselected attachment case.
    let unresolved =
        TeletextMotLevel25Resolution::resolve(&original.mots()[0], owner(0), Some(unused_links()))
            .unwrap();
    assert!(matches!(
        unresolved.role(TeletextX27LinkRole::Pop),
        TeletextMotRoleResolution::Unresolved {
            reason: TeletextMotUnresolvedReason::UnspecifiedOverAssociatedMot,
            ..
        }
    ));
    assert_eq!(
        TeletextObjectPageSelectionChange::new(
            owner(0),
            TeletextX27LinkRole::Pop,
            Some(unused_links()),
            *unresolved.role(TeletextX27LinkRole::Pop),
            link(0xa0, 1),
        ),
        Err(TeletextObjectPageSelectionError::InvalidChange)
    );
}

fn assert_restoration_rejections(
    attachment: TeletextObjectPageSelectionChange,
    current_links: Option<TeletextX27Level25Links>,
    selected_role: TeletextMotRoleResolution,
    restoration: TeletextObjectPageSelectionChange,
) {
    use TeletextObjectPageSelectionError as Error;
    let role = attachment.role();
    for links in [None, Some(unused_links())] {
        assert_eq!(
            TeletextObjectPageSelectionChange::new_unselected_restoration(
                attachment,
                links,
                selected_role,
            ),
            Err(Error::InvalidChange)
        );
    }
    assert_eq!(
        TeletextObjectPageSelectionChange::new_unselected_restoration(
            restoration,
            current_links,
            selected_role,
        ),
        Err(Error::InvalidChange)
    );
    assert_eq!(
        TeletextObjectPageSelectionChange::new_unselected_restoration(
            attachment,
            current_links,
            TeletextMotRoleResolution::Unselected,
        ),
        Err(Error::InvalidChange)
    );
    let TeletextMotRoleResolution::Selected {
        target,
        requirements,
        provenance,
        ..
    } = selected_role
    else {
        panic!("selected fixture")
    };
    for stale_role in [
        TeletextMotRoleResolution::Selected {
            target: raw(0xab),
            requirements,
            provenance,
            original_mot: None,
        },
        TeletextMotRoleResolution::Selected {
            target,
            requirements: TeletextMotResolutionRequirements::X27Mask(
                TeletextSubpageRequirementMask::new(0),
            ),
            provenance,
            original_mot: None,
        },
        TeletextMotRoleResolution::Selected {
            target,
            requirements,
            provenance: TeletextMotLinkProvenance::X27 {
                display: owner(0),
                role,
            },
            original_mot: None,
        },
    ] {
        assert_eq!(
            TeletextObjectPageSelectionChange::new_unselected_restoration(
                attachment,
                current_links,
                stale_role,
            ),
            Err(Error::InvalidChange)
        );
    }
}

fn no_mot_display() -> TeletextPageAddress {
    TeletextPageAddress {
        page: TeletextPageNumber::new(301).unwrap(),
        subcode: TeletextSubcode::new(0).unwrap(),
    }
}
fn without_attachment_mot(explicit: bool, attached: bool) -> TeletextDeclaredService {
    let template = bind_input(input(true, false));
    let mut draft = attachment_input(explicit, attached, false);
    draft.base.pages.last_mut().unwrap().address = no_mot_display();
    draft.pages.last_mut().unwrap().address = no_mot_display();
    let semantic = TeletextSemanticService::new_with_object_companions(
        draft,
        template.empty_object_pages().to_vec(),
        limits(),
    )
    .unwrap()
    .with_physical_base_overlays(
        template
            .semantic()
            .physical_base_overlays()
            .values()
            .cloned()
            .collect(),
        limits(),
    )
    .unwrap()
    .with_base_text_spellings(
        template
            .semantic()
            .base_text_spellings()
            .values()
            .copied()
            .collect(),
        limits(),
    )
    .unwrap();
    let mut selections = template.selections().to_vec();
    selections.push(TeletextDisplayMotSelection {
        display: no_mot_display(),
        mot: None,
    });
    TeletextDeclaredService::new(
        semantic,
        template.mots().to_vec(),
        selections,
        template.empty_object_pages().to_vec(),
        budget(),
    )
    .unwrap()
}
#[test]
fn object_page_attachment_without_mot_uses_only_exact_unselected_role_materialization() {
    let original = without_attachment_mot(false, false);
    let resolution = original
        .resolutions()
        .iter()
        .find(|r| r.display() == no_mot_display())
        .unwrap();
    assert!(resolution.mot().is_none());
    assert!(resolution.materialized_level25_links().is_err());
    let unused = resolution.materialized_level25_attachment_links().unwrap();
    assert!(TeletextX27LinkRole::ALL.iter().all(|role| matches!(
        unused.link(*role).target(),
        TeletextX27LinkTarget::Unspecified { .. }
    )));
    let changes = [
        (TeletextX27LinkRole::Gpop, 0xa2),
        (TeletextX27LinkRole::Pop, 0xa0),
        (TeletextX27LinkRole::Gdrcs, 0xb1),
        (TeletextX27LinkRole::Drcs, 0xb0),
    ]
    .map(|(role, page)| {
        TeletextObjectPageSelectionChange::new_attachment(
            no_mot_display(),
            role,
            None,
            link(page, 1),
        )
        .unwrap()
    });
    let selected = original
        .clone()
        .with_object_page_selections_and_program_overlays(&changes, &[], limits(), budget())
        .unwrap();
    assert_eq!(selected, without_attachment_mot(false, true));
    let final_resolution = selected
        .resolutions()
        .iter()
        .find(|r| r.display() == no_mot_display())
        .unwrap();
    assert!(final_resolution.mot().is_none());
    assert!(final_resolution
        .materialized_level25_attachment_links()
        .is_err());
}
