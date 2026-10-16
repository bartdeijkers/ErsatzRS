//! Explicit linked-bank additions retain authored definitions and declarations.

use super::*;

fn target(scope: TeletextDrcsTableScope) -> (TeletextResourceKind, u8, TeletextX27LinkRole) {
    match scope {
        TeletextDrcsTableScope::Normal => {
            (TeletextResourceKind::Drcs, 0xa0, TeletextX27LinkRole::Drcs)
        }
        TeletextDrcsTableScope::Global => (
            TeletextResourceKind::Gdrcs,
            0xa1,
            TeletextX27LinkRole::Gdrcs,
        ),
    }
}

fn set_link(
    page: &mut TeletextResolvedPageEnhancementDraft,
    selected: TeletextX27LinkRole,
    link: TeletextX27LinkDeclaration,
) {
    let links = TeletextX27LinkRole::ALL.map(|role| {
        if role == selected {
            link
        } else {
            page.compositional_links.unwrap().link(role)
        }
    });
    page.compositional_links =
        Some(TeletextX27Level25Links::new(links[0], links[1], links[2], links[3]).unwrap());
}

fn occupied() -> TeletextResolvedServiceDraft {
    let mut input = draft(true, true);
    let normal = table(TeletextResourceKind::Drcs, 0xa0, 0);
    for page in &mut input.pages {
        page.resources.drcs = TeletextDrcsPageLinks::new(
            Some(normal.identity().page()),
            page.resources.drcs.page(TeletextDrcsTableScope::Global),
        )
        .unwrap();
        set_link(
            page,
            TeletextX27LinkRole::Drcs,
            TeletextX27LinkDeclaration::new(
                TeletextX27LinkTarget::Page {
                    page: normal.identity().page(),
                },
                TeletextX27LinkValidity::Level25Only,
                TeletextSubpageRequirementMask::new(1),
            )
            .unwrap(),
        );
    }
    input.drcs.push(normal);
    input
}

fn assert_addition(
    source: &TeletextSemanticService,
    derived: &TeletextSemanticService,
    added: &TeletextDrcsSubtable,
    selected: TeletextX27LinkRole,
) {
    assert_eq!(
        &derived.drcs.subtables()[..source.drcs.subtables().len()],
        source.drcs.subtables()
    );
    assert_eq!(derived.drcs.subtables().last(), Some(added));
    let old = &source.pages[&address(200)];
    let mut expected = old.clone();
    let link = old.compositional_links.unwrap().link(selected);
    set_link(
        &mut expected,
        selected,
        TeletextX27LinkDeclaration::new(
            link.target(),
            link.validity(),
            TeletextSubpageRequirementMask::new(9),
        )
        .unwrap(),
    );
    assert_eq!(derived.pages[&address(200)], expected);
    let mut unchanged = derived.clone();
    unchanged.pages.insert(address(200), old.clone());
    unchanged.drcs = source.drcs.clone();
    unchanged.original_structure_items = source.original_structure_items;
    assert_eq!(&unchanged, source);
    assert_eq!(
        derived.original_structure_items,
        source.original_structure_items + 2
    );
}

#[test]
fn effective_drcs_subpages_preserve_both_scope_links_validity_and_all_original_intent() {
    for scope in [
        TeletextDrcsTableScope::Normal,
        TeletextDrcsTableScope::Global,
    ] {
        for validity in [
            TeletextX27LinkValidity::Level25Only,
            TeletextX27LinkValidity::Level25And35,
        ] {
            let (kind, page, role) = target(scope);
            let mut input = occupied();
            let link = input.pages[0].compositional_links.unwrap().link(role);
            set_link(
                &mut input.pages[0],
                role,
                TeletextX27LinkDeclaration::new(link.target(), validity, link.required_subpages())
                    .unwrap(),
            );
            let source = TeletextSemanticService::new(input, service_budget()).unwrap();
            let before = source.clone();
            let added = table(kind, page, 3);
            let derived = source
                .derive_with_drcs_subpages(
                    std::slice::from_ref(&added),
                    &[extension(scope, added.identity())],
                    extension_budget(),
                )
                .unwrap();
            assert_eq!(source, before);
            assert_addition(&source, &derived, &added, role);
            assert!(matches!(
                source.derive_with_drcs_resources(
                    std::slice::from_ref(&added),
                    &[extension(scope, added.identity())],
                    extension_budget(),
                ),
                Err(TeletextDrcsExtensionError::OccupiedResourcePage { .. })
            ));
        }
    }
}

#[test]
fn effective_drcs_subpages_reuse_the_exact_fresh_page_derivation() {
    let source = original(false, false);
    let added = table(TeletextResourceKind::Drcs, 0xa2, 3);
    let pages = [extension(TeletextDrcsTableScope::Normal, added.identity())];
    assert_eq!(
        source
            .derive_with_drcs_subpages(std::slice::from_ref(&added), &pages, extension_budget(),)
            .unwrap(),
        source
            .derive_with_drcs_resources(&[added], &pages, extension_budget())
            .unwrap()
    );
}

#[test]
fn effective_drcs_subpages_keep_exact_and_minus_one_shared_bounds() {
    let source = TeletextSemanticService::new(occupied(), service_budget()).unwrap();
    let before = source.clone();
    let added = table(TeletextResourceKind::Drcs, 0xa0, 3);
    let pages = [extension(TeletextDrcsTableScope::Normal, added.identity())];
    let exact = TeletextDrcsExtensionBudget {
        max_records: 5,
        max_added_resources: 1,
        max_added_glyphs: 1,
        max_page_extensions: 1,
    };
    assert!(source
        .derive_with_drcs_subpages(std::slice::from_ref(&added), &pages, exact)
        .is_ok());
    for denied in [
        TeletextDrcsExtensionBudget {
            max_records: 4,
            ..exact
        },
        TeletextDrcsExtensionBudget {
            max_added_resources: 0,
            ..exact
        },
        TeletextDrcsExtensionBudget {
            max_added_glyphs: 0,
            ..exact
        },
        TeletextDrcsExtensionBudget {
            max_page_extensions: 0,
            ..exact
        },
    ] {
        assert!(matches!(
            source.derive_with_drcs_subpages(std::slice::from_ref(&added), &pages, denied,),
            Err(TeletextDrcsExtensionError::Budget { .. })
        ));
        assert_eq!(source, before);
    }
}

#[test]
fn effective_drcs_subpages_reject_missing_mismatched_or_level35_only_links() {
    for variant in 0..4 {
        let mut input = occupied();
        let page = &mut input.pages[0];
        match variant {
            0 => page.compositional_links = None,
            1 => {
                page.resources.drcs = TeletextDrcsPageLinks::new(
                    None,
                    page.resources.drcs.page(TeletextDrcsTableScope::Global),
                )
                .unwrap();
            }
            _ => set_link(
                page,
                TeletextX27LinkRole::Drcs,
                TeletextX27LinkDeclaration::new(
                    TeletextX27LinkTarget::Page {
                        page: identity(
                            TeletextResourceKind::Drcs,
                            if variant == 2 { 0xa2 } else { 0xa0 },
                            0,
                        )
                        .page(),
                    },
                    if variant == 3 {
                        TeletextX27LinkValidity::Level35Only
                    } else {
                        TeletextX27LinkValidity::Level25Only
                    },
                    TeletextSubpageRequirementMask::new(u16::from(variant != 2)),
                )
                .unwrap(),
            ),
        }
        let source = TeletextSemanticService::new(input, service_budget()).unwrap();
        let before = source.clone();
        let added = table(TeletextResourceKind::Drcs, 0xa0, 3);
        assert!(matches!(
            source.derive_with_drcs_subpages(
                std::slice::from_ref(&added),
                &[extension(TeletextDrcsTableScope::Normal, added.identity())],
                extension_budget(),
            ),
            Err(TeletextDrcsExtensionError::OccupiedRole { .. })
        ));
        assert_eq!(source, before);
    }
}

#[test]
fn effective_drcs_subpages_reject_original_or_added_collisions_and_wrong_scope() {
    let source = TeletextSemanticService::new(occupied(), service_budget()).unwrap();
    for kind in [TeletextResourceKind::Drcs, TeletextResourceKind::Gdrcs] {
        let added = table(kind, 0xa0, 0);
        assert!(matches!(
            source.derive_with_drcs_subpages(
                std::slice::from_ref(&added),
                &[extension(TeletextDrcsTableScope::Normal, added.identity())],
                extension_budget(),
            ),
            Err(TeletextDrcsExtensionError::DuplicateResource { .. })
        ));
    }
    let added = table(TeletextResourceKind::Drcs, 0xa0, 3);
    assert!(matches!(
        source.derive_with_drcs_subpages(
            std::slice::from_ref(&added),
            &[extension(TeletextDrcsTableScope::Global, added.identity())],
            extension_budget(),
        ),
        Err(TeletextDrcsExtensionError::Kind { .. })
    ));
    let duplicate = table(TeletextResourceKind::Gdrcs, 0xa0, 3);
    assert!(matches!(
        source.derive_with_drcs_subpages(
            &[added.clone(), duplicate],
            &[extension(TeletextDrcsTableScope::Normal, added.identity())],
            extension_budget(),
        ),
        Err(TeletextDrcsExtensionError::DuplicateResource { .. })
    ));
    let pages = [extension(TeletextDrcsTableScope::Normal, added.identity()); 2];
    assert!(matches!(
        source.derive_with_drcs_subpages(&[added], &pages, extension_budget()),
        Err(TeletextDrcsExtensionError::DuplicatePage { .. })
    ));
}

#[test]
fn effective_drcs_subpages_reject_foreign_owners_and_unproven_added_banks() {
    let source = TeletextSemanticService::new(occupied(), service_budget()).unwrap();
    let foreign = table(TeletextResourceKind::Drcs, 0x00, 3);
    assert!(matches!(
        source.derive_with_drcs_subpages(
            std::slice::from_ref(&foreign),
            &[extension(
                TeletextDrcsTableScope::Normal,
                foreign.identity()
            )],
            extension_budget(),
        ),
        Err(TeletextDrcsExtensionError::OccupiedResourcePage { .. })
    ));
    let existing = table(TeletextResourceKind::Drcs, 0xa0, 3);
    let fresh = table(TeletextResourceKind::Drcs, 0xa2, 0);
    assert!(matches!(
        source.derive_with_drcs_subpages(
            &[existing, fresh.clone()],
            &[extension(TeletextDrcsTableScope::Normal, fresh.identity())],
            extension_budget(),
        ),
        Err(TeletextDrcsExtensionError::OccupiedResourcePage { .. })
    ));
    let companion = identity(TeletextResourceKind::Pop, 0xa0, 5);
    let source = TeletextSemanticService::new_with_object_companions(
        occupied(),
        vec![companion],
        service_budget(),
    )
    .unwrap();
    let added = table(TeletextResourceKind::Drcs, 0xa0, 3);
    assert!(matches!(
        source.derive_with_drcs_subpages(
            std::slice::from_ref(&added),
            &[extension(TeletextDrcsTableScope::Normal, added.identity())],
            extension_budget(),
        ),
        Err(TeletextDrcsExtensionError::OccupiedResourcePage { .. })
    ));
}
