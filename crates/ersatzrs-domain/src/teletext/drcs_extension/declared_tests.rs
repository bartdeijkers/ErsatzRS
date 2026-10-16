//! Checked MOT authority is additive; semantic-only guesses remain rejected.

use crate::teletext::*;

fn display() -> TeletextPageAddress {
    TeletextPageAddress {
        page: TeletextPageNumber::new(200).unwrap(),
        subcode: TeletextSubcode::new(0x1234).unwrap(),
    }
}

fn identity(kind: TeletextResourceKind, byte: u8, s1: u8) -> TeletextResourceIdentity {
    TeletextResourceIdentity::new(
        kind,
        TeletextRawDataPageNumber::new(2, byte).unwrap(),
        TeletextResourceSubpageIndex::new(s1).unwrap(),
    )
    .unwrap()
}

fn table(kind: TeletextResourceKind, byte: u8, s1: u8) -> TeletextDrcsSubtable {
    TeletextDrcsSubtable::new(
        identity(kind, byte, s1),
        vec![TeletextDrcsGlyphDefinition {
            slot: TeletextDrcsGlyphSlot::new(3).unwrap(),
            bitmap: TeletextDrcsBitmap::new([0x801; 10]).unwrap(),
        }],
    )
    .unwrap()
}

fn unused() -> TeletextX27LinkTarget {
    TeletextX27LinkTarget::Unspecified {
        magazine: TeletextX27Magazine::new(2).unwrap(),
    }
}

fn object_target() -> TeletextObjectIdentity {
    TeletextObjectIdentity::public(
        identity(TeletextResourceKind::Pop, 0xc0, 0),
        TeletextObjectId::new(0),
    )
    .unwrap()
}

fn explicit_links(occupied: bool, global_extent: u8) -> TeletextX27Level25Links {
    let link = |target, mask| {
        TeletextX27LinkDeclaration::new(
            target,
            TeletextX27LinkValidity::Level25And35,
            TeletextSubpageRequirementMask::new(mask),
        )
        .unwrap()
    };
    TeletextX27Level25Links::new(
        link(unused(), u16::MAX),
        link(
            TeletextX27LinkTarget::Page {
                page: identity(TeletextResourceKind::Pop, 0xc0, 0).page(),
            },
            1,
        ),
        link(
            TeletextX27LinkTarget::Page {
                page: identity(TeletextResourceKind::Gdrcs, 0xa1, 0).page(),
            },
            TeletextMotSubpageExtent::new(global_extent)
                .unwrap()
                .required_mask()
                .get(),
        ),
        link(
            if occupied {
                TeletextX27LinkTarget::Page {
                    page: identity(TeletextResourceKind::Drcs, 0xa0, 0).page(),
                }
            } else {
                unused()
            },
            if occupied { 1 } else { u16::MAX },
        ),
    )
    .unwrap()
}

fn page_metadata(
    occupied: bool,
    explicit: bool,
    global_extent: u8,
) -> TeletextResolvedPageEnhancementDraft {
    TeletextResolvedPageEnhancementDraft {
        address: display(),
        presentation: None,
        compositional_links: explicit.then(|| explicit_links(occupied, global_extent)),
        local_program: None,
        resources: TeletextResolvedPageResourceSelection {
            objects: TeletextResolvedObjectPageLinks {
                public: Some(identity(TeletextResourceKind::Pop, 0xc0, 0).page()),
                global: None,
            },
            drcs: TeletextDrcsPageLinks::new(
                occupied.then(|| identity(TeletextResourceKind::Drcs, 0xa0, 0).page()),
                Some(identity(TeletextResourceKind::Gdrcs, 0xa1, 0).page()),
            )
            .unwrap(),
            defaults: [
                None,
                None,
                Some(TeletextDefaultObjectSelection {
                    target: object_target(),
                    object_type: TeletextObjectType::Passive,
                }),
                None,
            ],
        },
    }
}

fn semantic(occupied: bool, explicit: bool, global_extent: u8) -> TeletextSemanticService {
    let language = TeletextLanguage::new("eng".into()).unwrap();
    let mut drcs = (0..=global_extent)
        .map(|s1| table(TeletextResourceKind::Gdrcs, 0xa1, s1))
        .collect::<Vec<_>>();
    if occupied {
        drcs.push(table(TeletextResourceKind::Drcs, 0xa0, 0));
    }
    TeletextSemanticService::new(
        TeletextResolvedServiceDraft {
            base: TeletextPageSetDraft {
                language: language.clone(),
                initial_page: display().page,
                pages: vec![TeletextPageDraft {
                    address: display(),
                    kind: TeletextPageKind::FullPage,
                    language,
                    national_option: TeletextNationalOption::new(0).unwrap(),
                    rows: vec![],
                    navigation: vec![],
                }],
            },
            pages: vec![page_metadata(occupied, explicit, global_extent)],
            magazines: vec![],
            objects: vec![TeletextEnhancementObjectDefinition {
                identity: object_target(),
                object_type: TeletextObjectType::Passive,
                program: TeletextEnhancementProgram::new(vec![]).unwrap(),
            }],
            drcs,
        },
        TeletextServiceBudget {
            max_records: 32,
            max_objects: 8,
            max_structure_items: 256,
            max_base_content_bytes: 1024,
            max_commands: 128,
            max_collected_commands: 256,
            max_collected_drcs_references: 32,
        },
    )
    .unwrap()
}

fn mot(occupied: bool, global_extent: u8) -> TeletextMotLevel25Declaration {
    let no_object = TeletextMotDefaultObject::NoObject { raw_number: 17 };
    let object = |target, defaults| {
        TeletextMotObjectLink::new(
            target,
            TeletextMotSubpageExtent::new(0).unwrap(),
            true,
            TeletextMotFallback::new(0).unwrap(),
            defaults,
        )
    };
    let mut public = [object(unused(), [no_object; 2]); 7];
    public[0] = object(
        TeletextX27LinkTarget::Page {
            page: identity(TeletextResourceKind::Pop, 0xc0, 0).page(),
        },
        [
            TeletextMotDefaultObject::Object {
                target: object_target(),
                object_type: TeletextObjectType::Passive,
            },
            no_object,
        ],
    );
    let drcs_link = |target, extent| {
        TeletextMotDrcsLink::new(target, TeletextMotSubpageExtent::new(extent).unwrap(), true)
    };
    let mut normal = [drcs_link(unused(), 0); 7];
    if occupied {
        normal[0] = drcs_link(
            TeletextX27LinkTarget::Page {
                page: identity(TeletextResourceKind::Drcs, 0xa0, 0).page(),
            },
            0,
        );
    }
    TeletextMotLevel25Declaration::new(
        identity(TeletextResourceKind::Mot, 0xfe, 0),
        vec![TeletextMotPageAssociation::new(
            0,
            TeletextMotAssociationSelection::new(
                false,
                Some(TeletextMotPublicLinkIndex::new(1).unwrap()),
            ),
            TeletextMotAssociationSelection::new(
                true,
                occupied.then(|| TeletextMotPublicLinkIndex::new(1).unwrap()),
            ),
        )],
        TeletextMotLevel25Links::new(
            object(unused(), [no_object; 2]),
            public,
            drcs_link(
                TeletextX27LinkTarget::Page {
                    page: identity(TeletextResourceKind::Gdrcs, 0xa1, 0).page(),
                },
                global_extent,
            ),
            normal,
            TeletextMotEnhancementCounts::new(0, 0, 0),
        )
        .unwrap(),
    )
    .unwrap()
}

fn fixture(occupied: bool, explicit: bool, global_extent: u8) -> TeletextDeclaredService {
    TeletextDeclaredService::new(
        semantic(occupied, explicit, global_extent),
        vec![mot(occupied, global_extent)],
        vec![TeletextDisplayMotSelection {
            display: display(),
            mot: Some(identity(TeletextResourceKind::Mot, 0xfe, 0)),
        }],
        vec![],
        TeletextDeclaredServiceBudget {
            max_mots: 1,
            max_displays: 1,
            max_resources: 32,
            max_defaults: 32,
            max_objects: 8,
        },
    )
    .unwrap()
}

fn limits() -> TeletextDrcsExtensionBudget {
    TeletextDrcsExtensionBudget {
        max_records: 32,
        max_added_resources: 1,
        max_added_glyphs: 1,
        max_page_extensions: 1,
    }
}

fn extension(resource: TeletextResourceIdentity) -> [TeletextDrcsPageExtension; 1] {
    [TeletextDrcsPageExtension {
        display: display(),
        table: TeletextDrcsTableScope::Normal,
        resource,
    }]
}

fn assert_page_and_defaults_preserved(
    original: &TeletextDeclaredService,
    derived: &TeletextSemanticService,
    links: TeletextX27Level25Links,
    added: TeletextResourceIdentity,
) {
    let mut expected = original.semantic().pages[&display()].clone();
    expected.compositional_links = Some(links);
    if expected
        .resources
        .drcs
        .page(TeletextDrcsTableScope::Normal)
        .is_none()
    {
        expected.resources.drcs = TeletextDrcsPageLinks::new(
            Some(added.page()),
            expected.resources.drcs.page(TeletextDrcsTableScope::Global),
        )
        .unwrap();
    }
    assert_eq!(derived.pages[&display()], expected);
    let resolved =
        TeletextMotLevel25Resolution::resolve(&original.mots()[0], display(), Some(links)).unwrap();
    assert_eq!(
        resolved.default_sources(),
        original.resolutions()[0].mot().unwrap().default_sources()
    );
}

#[test]
fn checked_mot_materializes_exact_roles_for_linked_and_fresh_drcs_without_changing_original() {
    for occupied in [true, false] {
        for global_extent in [0, 15] {
            let original = fixture(occupied, false, global_extent);
            let before = original.clone();
            let added = table(
                TeletextResourceKind::Drcs,
                if occupied { 0xa0 } else { 0xa2 },
                1,
            );
            let derived = original
                .derive_with_drcs_subpages(
                    std::slice::from_ref(&added),
                    &extension(added.identity()),
                    limits(),
                )
                .unwrap();
            assert_eq!(original, before);
            assert!(matches!(
                original.semantic().derive_with_drcs_subpages(
                    std::slice::from_ref(&added),
                    &extension(added.identity()),
                    limits()
                ),
                Err(TeletextDrcsExtensionError::OccupiedRole { .. }
                    | TeletextDrcsExtensionError::UnprovenOtherRoles { .. })
            ));
            let links = derived.pages[&display()].compositional_links.unwrap();
            assert_eq!(
                links.link(TeletextX27LinkRole::Pop).target(),
                TeletextX27LinkTarget::Page {
                    page: identity(TeletextResourceKind::Pop, 0xc0, 0).page()
                }
            );
            assert_eq!(
                links
                    .link(TeletextX27LinkRole::Pop)
                    .required_subpages()
                    .get(),
                1
            );
            assert_eq!(
                links
                    .link(TeletextX27LinkRole::Gdrcs)
                    .required_subpages()
                    .get(),
                if global_extent == 15 { u16::MAX } else { 1 }
            );
            assert_eq!(
                links
                    .link(TeletextX27LinkRole::Drcs)
                    .required_subpages()
                    .get(),
                if occupied { 3 } else { 2 }
            );
            assert_eq!(links.link(TeletextX27LinkRole::Gpop).target(), unused());
            assert_eq!(
                links
                    .link(TeletextX27LinkRole::Gpop)
                    .required_subpages()
                    .get(),
                u16::MAX
            );
            for role in TeletextX27LinkRole::ALL {
                assert_eq!(
                    links.link(role).validity(),
                    TeletextX27LinkValidity::Level25Only
                );
            }
            assert_page_and_defaults_preserved(&original, &derived, links, added.identity());
            let mut restored = derived;
            restored
                .pages
                .insert(display(), original.semantic().pages[&display()].clone());
            restored.drcs = original.semantic().drcs.clone();
            restored.original_structure_items = original.semantic().original_structure_items;
            assert_eq!(&restored, original.semantic());
        }
    }
}

#[test]
fn checked_mot_extension_preserves_explicit_semantic_derivation_exactly() {
    let original = fixture(true, true, 0);
    let added = table(TeletextResourceKind::Drcs, 0xa0, 1);
    assert_eq!(
        original
            .derive_with_drcs_subpages(
                std::slice::from_ref(&added),
                &extension(added.identity()),
                limits()
            )
            .unwrap(),
        original
            .semantic()
            .derive_with_drcs_subpages(
                std::slice::from_ref(&added),
                &extension(added.identity()),
                limits()
            )
            .unwrap()
    );
}

#[test]
fn checked_mot_extension_fences_complete_namespace_and_raw_counts_without_mutation() {
    let original = fixture(true, false, 0);
    let before = original.clone();
    let mot_page = original.mots()[0].identity().page();
    assert_eq!(
        TeletextResourceIdentity::new(
            TeletextResourceKind::Drcs,
            mot_page,
            TeletextResourceSubpageIndex::new(1).unwrap()
        ),
        Err(TeletextResourceIdentityError::ReservedPurpose {
            kind: TeletextResourceKind::Drcs,
            page: mot_page
        })
    );
    assert_eq!(original, before);
    let collision = table(TeletextResourceKind::Drcs, 0xc0, 1);
    assert!(
        matches!(original.derive_with_drcs_subpages(std::slice::from_ref(&collision),&extension(collision.identity()),limits()),Err(TeletextDrcsExtensionError::OccupiedResourcePage {resource}) if resource==collision.identity())
    );
    assert_eq!(original, before);
    let duplicate = table(TeletextResourceKind::Drcs, 0xa0, 0);
    assert!(matches!(
        original.derive_with_drcs_subpages(
            std::slice::from_ref(&duplicate),
            &extension(duplicate.identity()),
            limits()
        ),
        Err(TeletextDrcsExtensionError::DuplicateResource { .. })
    ));
    let added = table(TeletextResourceKind::Drcs, 0xa0, 1);
    let count = original.semantic().base().pages.len() + original.resource_inventory().len() + 1;
    let exact = TeletextDrcsExtensionBudget {
        max_records: count,
        ..limits()
    };
    assert!(original
        .derive_with_drcs_subpages(
            std::slice::from_ref(&added),
            &extension(added.identity()),
            exact
        )
        .is_ok());
    for denied in [
        TeletextDrcsExtensionBudget {
            max_records: count - 1,
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
            original.derive_with_drcs_subpages(
                std::slice::from_ref(&added),
                &extension(added.identity()),
                denied
            ),
            Err(TeletextDrcsExtensionError::Budget { .. })
        ));
        assert_eq!(original, before);
    }
}
