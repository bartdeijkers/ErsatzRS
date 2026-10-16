//! Effective-service derivation only; no wire, ownership or publication proof.
use super::*;
use crate::teletext::*;

fn address(page: u16) -> TeletextPageAddress {
    TeletextPageAddress {
        page: TeletextPageNumber::new(page).unwrap(),
        subcode: TeletextSubcode::new(0).unwrap(),
    }
}
fn identity(kind: TeletextResourceKind, page: u8, subpage: u8) -> TeletextResourceIdentity {
    TeletextResourceIdentity::new(
        kind,
        TeletextRawDataPageNumber::new(2, page).unwrap(),
        TeletextResourceSubpageIndex::new(subpage).unwrap(),
    )
    .unwrap()
}
fn table(kind: TeletextResourceKind, page: u8, subpage: u8) -> TeletextDrcsSubtable {
    TeletextDrcsSubtable::new(
        identity(kind, page, subpage),
        vec![TeletextDrcsGlyphDefinition {
            slot: TeletextDrcsGlyphSlot::new(3).unwrap(),
            bitmap: TeletextDrcsBitmap::new([0x801; 10]).unwrap(),
        }],
    )
    .unwrap()
}
fn unused() -> TeletextX27LinkDeclaration {
    TeletextX27LinkDeclaration::new(
        TeletextX27LinkTarget::Unspecified {
            magazine: TeletextX27Magazine::new(2).unwrap(),
        },
        TeletextX27LinkValidity::Level25Only,
        TeletextSubpageRequirementMask::new(u16::MAX),
    )
    .unwrap()
}
fn service_budget() -> TeletextServiceBudget {
    TeletextServiceBudget {
        max_records: 16,
        max_objects: 8,
        max_structure_items: 128,
        max_base_content_bytes: 1024,
        max_commands: 128,
        max_collected_commands: 256,
        max_collected_drcs_references: 32,
    }
}
fn extension_budget() -> TeletextDrcsExtensionBudget {
    TeletextDrcsExtensionBudget {
        max_records: 8,
        max_added_resources: 2,
        max_added_glyphs: 2,
        max_page_extensions: 2,
    }
}
fn draft(global: bool, metadata: bool) -> TeletextResolvedServiceDraft {
    let language = TeletextLanguage::new("eng".into()).unwrap();
    let pages = [200, 201].map(|number| TeletextPageDraft {
        address: address(number),
        kind: TeletextPageKind::FullPage,
        language: language.clone(),
        national_option: TeletextNationalOption::new(0).unwrap(),
        rows: vec![TeletextRow {
            row: 1,
            runs: vec![TeletextRun {
                column: 1,
                style: TeletextStyle::default(),
                content: TeletextRunContent::Text { text: "AB".into() },
            }],
        }],
        navigation: vec![],
    });
    TeletextResolvedServiceDraft {
        base: TeletextPageSetDraft {
            language,
            initial_page: address(200).page,
            pages: pages.into(),
        },
        pages: if metadata {
            [200, 201]
                .map(|number| {
                    let gdrcs = if global {
                        TeletextX27LinkDeclaration::new(
                            TeletextX27LinkTarget::Page {
                                page: identity(TeletextResourceKind::Gdrcs, 0xa1, 0).page(),
                            },
                            TeletextX27LinkValidity::Level25Only,
                            TeletextSubpageRequirementMask::new(1),
                        )
                        .unwrap()
                    } else {
                        unused()
                    };
                    TeletextResolvedPageEnhancementDraft {
                        address: address(number),
                        presentation: None,
                        compositional_links: Some(
                            TeletextX27Level25Links::new(unused(), unused(), gdrcs, unused())
                                .unwrap(),
                        ),
                        local_program: Some(
                            TeletextEnhancementProgram::new(vec![
                                TeletextEnhancementCommand::SetPosition {
                                    position: TeletextEnhancementPosition::new(1, 1).unwrap(),
                                },
                                TeletextEnhancementCommand::AtColumn {
                                    column: 1,
                                    operation: if global {
                                        TeletextColumnOperation::Drcs {
                                            reference: TeletextDrcsGlyphReference {
                                                table: TeletextDrcsTableScope::Global,
                                                subtable: TeletextDrcsSubtableIndex::new(0)
                                                    .unwrap(),
                                                slot: TeletextDrcsGlyphSlot::new(3).unwrap(),
                                            },
                                        }
                                    } else {
                                        TeletextColumnOperation::G0 {
                                            code: TeletextGlyphCode::new(b'A').unwrap(),
                                        }
                                    },
                                },
                            ])
                            .unwrap(),
                        ),
                        resources: TeletextResolvedPageResourceSelection {
                            objects: TeletextResolvedObjectPageLinks {
                                public: None,
                                global: None,
                            },
                            drcs: TeletextDrcsPageLinks::new(
                                None,
                                global
                                    .then(|| identity(TeletextResourceKind::Gdrcs, 0xa1, 0).page()),
                            )
                            .unwrap(),
                            defaults: [None; 4],
                        },
                    }
                })
                .into()
        } else {
            vec![]
        },
        magazines: vec![],
        objects: vec![],
        drcs: if global {
            vec![table(TeletextResourceKind::Gdrcs, 0xa1, 0)]
        } else {
            vec![]
        },
    }
}
fn original(global: bool, metadata: bool) -> TeletextSemanticService {
    TeletextSemanticService::new(draft(global, metadata), service_budget()).unwrap()
}
fn extension(
    scope: TeletextDrcsTableScope,
    resource: TeletextResourceIdentity,
) -> TeletextDrcsPageExtension {
    TeletextDrcsPageExtension {
        display: address(200),
        table: scope,
        resource,
    }
}

#[test]
fn effective_drcs_extension_preserves_original_fallbacks_rows_programs_and_collection() {
    let mut source = original(true, true);
    source.set_mot_fallback(
        address(200),
        resolve_teletext_mot_presentation_fallback(
            TeletextNationalOption::new(0).unwrap(),
            TeletextMotFallback::new(14).unwrap(),
        )
        .unwrap(),
    );
    let mut cells = [b' '; 40];
    cells[7] = 0x5b;
    let source = source
        .with_physical_base_overlays(
            vec![TeletextPhysicalBaseOverlay::new(
                address(200),
                vec![TeletextPhysicalBaseRow::new(1, cells).unwrap()],
            )
            .unwrap()],
            service_budget(),
        )
        .unwrap();
    let before = source.clone();
    let added = table(TeletextResourceKind::Drcs, 0xa2, 3);
    let derived = source
        .derive_with_drcs_resources(
            std::slice::from_ref(&added),
            &[extension(TeletextDrcsTableScope::Normal, added.identity())],
            extension_budget(),
        )
        .unwrap();
    assert_eq!(source, before);
    assert_eq!(derived.base, source.base);
    assert_eq!(derived.objects, source.objects);
    assert_eq!(derived.magazines, source.magazines);
    assert_eq!(derived.mot_fallbacks, source.mot_fallbacks);
    assert_eq!(
        derived.physical_base_overlays,
        source.physical_base_overlays
    );
    assert_eq!(derived.collected, source.collected);
    assert_eq!(derived.pages[&address(201)], source.pages[&address(201)]);
    let old = &source.pages[&address(200)];
    let new = &derived.pages[&address(200)];
    assert_eq!(new.local_program, old.local_program);
    assert_eq!(new.presentation, old.presentation);
    assert_eq!(new.resources.objects, old.resources.objects);
    assert_eq!(new.resources.defaults, old.resources.defaults);
    for role in [
        TeletextX27LinkRole::Gpop,
        TeletextX27LinkRole::Pop,
        TeletextX27LinkRole::Gdrcs,
    ] {
        assert_eq!(
            new.compositional_links.unwrap().link(role),
            old.compositional_links.unwrap().link(role)
        );
    }
    assert_eq!(
        new.resources.drcs.page(TeletextDrcsTableScope::Normal),
        Some(added.identity().page())
    );
    assert_eq!(
        new.compositional_links
            .unwrap()
            .link(TeletextX27LinkRole::Drcs)
            .required_subpages()
            .get(),
        8
    );
    assert_eq!(derived.drcs.subtables().last(), Some(&added));
    assert_eq!(
        derived
            .collected(address(200))
            .unwrap()
            .1
            .references()
            .len(),
        1
    );
}

#[test]
fn effective_drcs_extension_can_add_absent_metadata_and_exact_sparse_bank_masks() {
    let source = original(false, false);
    let before = source.clone();
    let tables = [
        table(TeletextResourceKind::Drcs, 0xa2, 0),
        table(TeletextResourceKind::Drcs, 0xa2, 3),
    ];
    let derived = source
        .derive_with_drcs_resources(
            &tables,
            &[extension(
                TeletextDrcsTableScope::Normal,
                tables[0].identity(),
            )],
            extension_budget(),
        )
        .unwrap();
    assert_eq!(source, before);
    assert_eq!(derived.collected, source.collected);
    let page = &derived.pages[&address(200)];
    assert_eq!(page.local_program, None);
    assert_eq!(page.presentation, None);
    assert_eq!(page.resources.defaults, [None; 4]);
    assert_eq!(
        page.compositional_links
            .unwrap()
            .link(TeletextX27LinkRole::Drcs)
            .required_subpages()
            .get(),
        9
    );
    assert_eq!(
        derived.effective_program(address(200)).unwrap(),
        source.effective_program(address(200)).unwrap()
    );
}

#[test]
fn effective_drcs_extension_bounds_precede_missing_owners_and_clone_work() {
    let source = original(false, true);
    let added = table(TeletextResourceKind::Drcs, 0xa2, 0);
    let mut target = extension(TeletextDrcsTableScope::Normal, added.identity());
    target.display = address(202);
    for budget in [
        TeletextDrcsExtensionBudget {
            max_added_resources: 0,
            ..extension_budget()
        },
        TeletextDrcsExtensionBudget {
            max_page_extensions: 0,
            ..extension_budget()
        },
        TeletextDrcsExtensionBudget {
            max_added_glyphs: 0,
            ..extension_budget()
        },
        TeletextDrcsExtensionBudget {
            max_records: 2,
            ..extension_budget()
        },
    ] {
        assert!(matches!(
            source.derive_with_drcs_resources(std::slice::from_ref(&added), &[target], budget),
            Err(TeletextDrcsExtensionError::Budget { .. })
        ));
    }
    assert!(source
        .derive_with_drcs_resources(
            std::slice::from_ref(&added),
            &[extension(TeletextDrcsTableScope::Normal, added.identity())],
            TeletextDrcsExtensionBudget {
                max_records: 3,
                ..extension_budget()
            }
        )
        .is_ok());
}

#[test]
fn effective_drcs_extension_rejects_collisions_repeated_owners_and_wrong_scopes() {
    let source = original(true, true);
    let before = source.clone();
    let added = table(TeletextResourceKind::Drcs, 0xa2, 0);
    let target = extension(TeletextDrcsTableScope::Normal, added.identity());
    assert!(matches!(
        source.derive_with_drcs_resources(
            &[added.clone(), added.clone()],
            &[target],
            extension_budget()
        ),
        Err(TeletextDrcsExtensionError::DuplicateResource { .. })
    ));
    assert!(matches!(
        source.derive_with_drcs_resources(
            std::slice::from_ref(&added),
            &[target, target],
            extension_budget()
        ),
        Err(TeletextDrcsExtensionError::DuplicatePage { .. })
    ));
    assert!(matches!(
        source.derive_with_drcs_resources(
            std::slice::from_ref(&added),
            &[TeletextDrcsPageExtension {
                display: address(202),
                ..target
            }],
            extension_budget()
        ),
        Err(TeletextDrcsExtensionError::MissingPage { .. })
    ));
    assert!(matches!(
        source.derive_with_drcs_resources(
            std::slice::from_ref(&added),
            &[TeletextDrcsPageExtension {
                table: TeletextDrcsTableScope::Global,
                ..target
            }],
            extension_budget()
        ),
        Err(TeletextDrcsExtensionError::Kind { .. })
    ));
    assert!(matches!(
        source.derive_with_drcs_resources(
            std::slice::from_ref(&added),
            &[TeletextDrcsPageExtension {
                resource: identity(TeletextResourceKind::Drcs, 0xa3, 0),
                ..target
            }],
            extension_budget()
        ),
        Err(TeletextDrcsExtensionError::MissingResource { .. })
    ));
    for collision in [
        table(TeletextResourceKind::Drcs, 0x00, 0),
        table(TeletextResourceKind::Drcs, 0xa1, 1),
    ] {
        assert!(matches!(
            source.derive_with_drcs_resources(
                std::slice::from_ref(&collision),
                &[extension(
                    TeletextDrcsTableScope::Normal,
                    collision.identity()
                )],
                extension_budget()
            ),
            Err(TeletextDrcsExtensionError::OccupiedResourcePage { .. })
        ));
    }
    let global = table(TeletextResourceKind::Gdrcs, 0xa2, 0);
    assert!(matches!(
        source.derive_with_drcs_resources(
            std::slice::from_ref(&global),
            &[extension(TeletextDrcsTableScope::Global, global.identity())],
            extension_budget()
        ),
        Err(TeletextDrcsExtensionError::OccupiedRole { .. })
    ));
    assert_eq!(source, before);
}

#[test]
fn effective_drcs_extension_preserves_explicit_other_roles_and_refuses_unproven_nulls() {
    let mut input = draft(true, true);
    // A valid unused authored X27 target is occupied even without a resolved
    // consumer; a compiler supplement cannot take ownership of that role.
    input.pages[0].resources.drcs = TeletextDrcsPageLinks::unlinked();
    input.pages[0].local_program = Some(TeletextEnhancementProgram::new(vec![]).unwrap());
    let source = TeletextSemanticService::new(input, service_budget()).unwrap();
    let global = table(TeletextResourceKind::Gdrcs, 0xa2, 0);
    assert!(matches!(
        source.derive_with_drcs_resources(
            std::slice::from_ref(&global),
            &[extension(TeletextDrcsTableScope::Global, global.identity())],
            extension_budget()
        ),
        Err(TeletextDrcsExtensionError::OccupiedRole { .. })
    ));
    let mut input = draft(true, true);
    input.pages[0].compositional_links = None;
    let source = TeletextSemanticService::new(input, service_budget()).unwrap();
    let normal = table(TeletextResourceKind::Drcs, 0xa2, 0);
    assert!(
        matches!(source.derive_with_drcs_resources(std::slice::from_ref(&normal),&[extension(TeletextDrcsTableScope::Normal,normal.identity())],extension_budget()),Err(TeletextDrcsExtensionError::UnprovenOtherRoles {display}) if display==address(200))
    );
}
