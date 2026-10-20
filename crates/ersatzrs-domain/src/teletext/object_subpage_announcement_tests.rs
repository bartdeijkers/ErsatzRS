//! Explicit selected-page announcement, not implicit invocation repair.
use super::*;

fn limits() -> TeletextServiceBudget {
    TeletextServiceBudget {
        max_records: 64,
        max_objects: 64,
        max_structure_items: 1024,
        max_base_content_bytes: 1024,
        max_commands: 1024,
        max_collected_commands: 1024,
        max_collected_drcs_references: 64,
    }
}
fn resource(kind: TeletextResourceKind, s1: u8) -> TeletextResourceIdentity {
    identity(
        kind,
        if kind == TeletextResourceKind::Pop {
            0xa0
        } else {
            0xa2
        },
        s1,
    )
}
fn request(
    display: TeletextPageAddress,
    kind: TeletextResourceKind,
    s1: u8,
) -> TeletextObjectSubpageAnnouncement {
    TeletextObjectSubpageAnnouncement::new(display, resource(kind, s1)).unwrap()
}
fn explicit(mask: u16) -> TeletextX27Level25Links {
    let link = |target, mask, validity| {
        TeletextX27LinkDeclaration::new(target, validity, TeletextSubpageRequirementMask::new(mask))
            .unwrap()
    };
    TeletextX27Level25Links::new(
        link(
            TeletextX27LinkTarget::Page { page: raw(0xa2) },
            mask,
            TeletextX27LinkValidity::Level25And35,
        ),
        link(
            TeletextX27LinkTarget::Page { page: raw(0xa0) },
            mask,
            TeletextX27LinkValidity::Level25Only,
        ),
        link(unused(), u16::MAX, TeletextX27LinkValidity::Level35Only),
        link(unused(), u16::MAX, TeletextX27LinkValidity::Level25And35),
    )
    .unwrap()
}
fn fixture(explicit_links: bool, with_mot: bool, mask: u16) -> TeletextDeclaredService {
    fixture_links(explicit_links.then(|| explicit(mask)), with_mot)
}
fn fixture_links(
    links: Option<TeletextX27Level25Links>,
    with_mot: bool,
) -> TeletextDeclaredService {
    let mut input = draft();
    let mut second = input.base.pages[0].clone();
    second.address = owner(32);
    input.base.pages.push(second);
    input.pages[0].resources.objects.global = Some(raw(0xa2));
    input.pages[0].compositional_links = links;
    if !with_mot {
        input.pages[0].resources.defaults = [None; 4];
    }
    let mut second = input.pages[0].clone();
    second.address = owner(32);
    input.pages.push(second);
    let mut public = [empty_object(); 7];
    public[0] = object(
        TeletextX27LinkTarget::Page { page: raw(0xa0) },
        0,
        [
            active(0xa0),
            TeletextMotDefaultObject::NoObject { raw_number: 77 },
        ],
        1,
    );
    let global = object(
        TeletextX27LinkTarget::Page { page: raw(0xa2) },
        0,
        [TeletextMotDefaultObject::NoObject { raw_number: 19 }; 2],
        0,
    );
    let mots = if with_mot {
        vec![mot(public, global, true)]
    } else {
        vec![]
    };
    let companions = [
        resource(TeletextResourceKind::Pop, 15),
        resource(TeletextResourceKind::Gpop, 0),
        resource(TeletextResourceKind::Gpop, 15),
    ];
    let semantic =
        TeletextSemanticService::new_with_object_companions(input, companions.to_vec(), limits())
            .unwrap();
    TeletextDeclaredService::new(
        semantic,
        mots,
        vec![
            TeletextDisplayMotSelection {
                display: owner(0),
                mot: with_mot.then_some(identity(TeletextResourceKind::Mot, 0xfe, 9)),
            },
            TeletextDisplayMotSelection {
                display: owner(32),
                mot: with_mot.then_some(identity(TeletextResourceKind::Mot, 0xfe, 9)),
            },
        ],
        companions.to_vec(),
        budget(),
    )
    .unwrap()
}

#[test]
fn announcement_adds_both_role_extremes_and_multiple_owners_preserving_all_other_intent() {
    for with_mot in [false, true] {
        // Explicit zero masks are legal: add both S1 extremes without filling gaps.
        let original = fixture(true, with_mot, 0);
        let before = original.clone();
        let announcements = [
            request(owner(0), TeletextResourceKind::Pop, 0),
            request(owner(0), TeletextResourceKind::Pop, 15),
            request(owner(0), TeletextResourceKind::Gpop, 15),
            request(owner(32), TeletextResourceKind::Gpop, 0),
        ];
        let changed = original
            .clone()
            .with_object_subpage_announcements(&announcements, limits(), budget())
            .unwrap();
        assert_eq!(original, before);
        assert_eq!(changed.mots(), original.mots());
        assert_eq!(changed.selections(), original.selections());
        assert_eq!(changed.defaults(), original.defaults());
        assert_eq!(changed.resource_inventory(), original.resource_inventory());
        assert_eq!(changed.empty_object_pages(), original.empty_object_pages());
        assert_eq!(changed.semantic().base(), original.semantic().base());
        assert_eq!(changed.semantic().objects(), original.semantic().objects());
        assert_eq!(changed.semantic().drcs(), original.semantic().drcs());
        for display in [owner(0), owner(32)] {
            let mut restored = changed.semantic().page_enhancements()[&display].clone();
            let links = restored.compositional_links.unwrap();
            for role in [TeletextX27LinkRole::Gdrcs, TeletextX27LinkRole::Drcs] {
                assert_eq!(links.link(role), explicit(0).link(role));
            }
            for role in [TeletextX27LinkRole::Pop, TeletextX27LinkRole::Gpop] {
                assert_eq!(
                    links.link(role).validity(),
                    explicit(0).link(role).validity()
                );
                assert_eq!(links.link(role).target(), explicit(0).link(role).target());
            }
            restored.compositional_links =
                original.semantic().page_enhancements()[&display].compositional_links;
            assert_eq!(restored, original.semantic().page_enhancements()[&display]);
            assert_eq!(
                changed.semantic().effective_program(display),
                original.semantic().effective_program(display)
            );
            assert_eq!(
                changed.semantic().collected(display),
                original.semantic().collected(display)
            );
        }
        let first = changed.semantic().page_enhancements()[&owner(0)]
            .compositional_links
            .unwrap();
        assert_eq!(
            first
                .link(TeletextX27LinkRole::Pop)
                .required_subpages()
                .get(),
            0x8001
        );
        assert_eq!(
            first
                .link(TeletextX27LinkRole::Gpop)
                .required_subpages()
                .get(),
            0x8000
        );
        let second = changed.semantic().page_enhancements()[&owner(32)]
            .compositional_links
            .unwrap();
        assert_eq!(
            second
                .link(TeletextX27LinkRole::Gpop)
                .required_subpages()
                .get(),
            1
        );
        assert_eq!(
            second
                .link(TeletextX27LinkRole::Pop)
                .required_subpages()
                .get(),
            0
        );
    }
}

#[test]
fn announcement_materializes_only_authenticated_absent_x27_and_retains_mot_default_roots() {
    let original = fixture(false, true, 0);
    let before = original.clone();
    let changed = original
        .clone()
        .with_object_subpage_announcements(
            &[
                request(owner(0), TeletextResourceKind::Pop, 15),
                request(owner(0), TeletextResourceKind::Gpop, 15),
            ],
            limits(),
            budget(),
        )
        .unwrap();
    assert_eq!(original, before);
    let links = changed.semantic().page_enhancements()[&owner(0)]
        .compositional_links
        .unwrap();
    assert_eq!(
        links
            .link(TeletextX27LinkRole::Pop)
            .required_subpages()
            .get(),
        0x8001
    );
    assert_eq!(
        links
            .link(TeletextX27LinkRole::Gpop)
            .required_subpages()
            .get(),
        0x8001
    );
    for role in TeletextX27LinkRole::ALL {
        assert_eq!(
            links.link(role).validity(),
            TeletextX27LinkValidity::Level25Only
        );
    }
    assert_eq!(changed.mots(), original.mots());
    assert_eq!(changed.defaults(), original.defaults());
    assert_eq!(
        changed.resolutions()[0].mot().unwrap().default_sources(),
        original.resolutions()[0].mot().unwrap().default_sources()
    );
    assert_eq!(
        changed.semantic().page_enhancements()[&owner(32)],
        original.semantic().page_enhancements()[&owner(32)]
    );
    assert_eq!(
        changed.semantic().effective_presentation(owner(0)),
        original.semantic().effective_presentation(owner(0))
    );
    assert_eq!(
        changed.semantic().effective_program(owner(0)),
        original.semantic().effective_program(owner(0))
    );
}

#[test]
fn announcement_rejects_duplicate_present_foreign_missing_and_non_object_intents_atomically() {
    let original = fixture(true, true, 1);
    let before = original.clone();
    let add = request(owner(0), TeletextResourceKind::Pop, 15);
    let apply = |items: &[TeletextObjectSubpageAnnouncement]| {
        original
            .clone()
            .with_object_subpage_announcements(items, limits(), budget())
    };
    assert_eq!(
        apply(&[]),
        Err(TeletextObjectSubpageAnnouncementError::Empty)
    );
    assert!(matches!(
        apply(&[add, add]),
        Err(TeletextObjectSubpageAnnouncementError::Duplicate { .. })
    ));
    assert!(matches!(
        apply(&[request(owner(0), TeletextResourceKind::Pop, 0)]),
        Err(TeletextObjectSubpageAnnouncementError::AlreadyAnnounced { .. })
    ));
    assert!(matches!(
        apply(&[request(owner(1), TeletextResourceKind::Pop, 15)]),
        Err(TeletextObjectSubpageAnnouncementError::MissingDisplay { .. })
    ));
    assert!(matches!(
        apply(&[request(owner(0), TeletextResourceKind::Pop, 14)]),
        Err(TeletextObjectSubpageAnnouncementError::MissingResource { .. })
    ));
    let foreign = TeletextObjectResourceAddition::new(
        identity(TeletextResourceKind::Pop, 0xa1, 15),
        vec![],
        limits(),
    )
    .unwrap();
    let extra = original
        .clone()
        .with_object_resource_additions(&[foreign], limits(), budget())
        .unwrap();
    let invalid = TeletextObjectSubpageAnnouncement::new(
        owner(0),
        identity(TeletextResourceKind::Pop, 0xa1, 15),
    )
    .unwrap();
    assert!(matches!(
        extra.with_object_subpage_announcements(&[invalid], limits(), budget()),
        Err(TeletextObjectSubpageAnnouncementError::Owner { .. })
    ));
    for kind in [
        TeletextResourceKind::Mot,
        TeletextResourceKind::Drcs,
        TeletextResourceKind::Gdrcs,
    ] {
        let resource = identity(
            kind,
            if kind == TeletextResourceKind::Mot {
                0xfe
            } else {
                0xb1
            },
            0,
        );
        assert_eq!(
            TeletextObjectSubpageAnnouncement::new(owner(0), resource),
            Err(TeletextObjectSubpageAnnouncementError::Kind { resource })
        );
    }
    assert_eq!(original, before);
}

#[test]
fn announcement_fences_complete_namespace_and_reduced_service_declaration_limits() {
    let original = fixture(false, true, 0);
    let before = original.clone();
    let add = request(owner(0), TeletextResourceKind::Pop, 15);
    let exact = TeletextServiceBudget {
        max_records: original.semantic().base().pages.len() + original.resource_inventory().len(),
        ..limits()
    };
    assert!(original
        .clone()
        .with_object_subpage_announcements(&[add], exact, budget())
        .is_ok());
    for denied in [
        TeletextServiceBudget {
            max_records: exact.max_records - 1,
            ..exact
        },
        TeletextServiceBudget {
            max_structure_items: 0,
            ..exact
        },
        TeletextServiceBudget {
            max_commands: 0,
            ..exact
        },
        TeletextServiceBudget {
            max_objects: 0,
            ..exact
        },
        TeletextServiceBudget {
            max_collected_commands: 0,
            ..exact
        },
    ] {
        assert!(matches!(
            original
                .clone()
                .with_object_subpage_announcements(&[add], denied, budget()),
            Err(TeletextObjectSubpageAnnouncementError::Service(_))
        ));
    }
    for denied in [
        TeletextDeclaredServiceBudget {
            max_mots: 0,
            ..budget()
        },
        TeletextDeclaredServiceBudget {
            max_resources: original.resource_inventory().len() - 1,
            ..budget()
        },
        TeletextDeclaredServiceBudget {
            max_defaults: 0,
            ..budget()
        },
        TeletextDeclaredServiceBudget {
            max_displays: 1,
            ..budget()
        },
        TeletextDeclaredServiceBudget {
            max_objects: 0,
            ..budget()
        },
    ] {
        assert!(matches!(
            original
                .clone()
                .with_object_subpage_announcements(&[add], exact, denied),
            Err(TeletextObjectSubpageAnnouncementError::Declaration(_))
        ));
    }
    assert_eq!(original, before);
}

#[test]
fn announcement_does_not_upgrade_level35_roles_or_materialize_without_mot_authority() {
    let links = explicit(1);
    let pop = links.link(TeletextX27LinkRole::Pop);
    let links = TeletextX27Level25Links::new(
        links.link(TeletextX27LinkRole::Gpop),
        TeletextX27LinkDeclaration::new(
            pop.target(),
            TeletextX27LinkValidity::Level35Only,
            pop.required_subpages(),
        )
        .unwrap(),
        links.link(TeletextX27LinkRole::Gdrcs),
        links.link(TeletextX27LinkRole::Drcs),
    )
    .unwrap();
    let original = fixture_links(Some(links), true);
    let before = original.clone();
    assert_eq!(
        original.clone().with_object_subpage_announcements(
            &[request(owner(0), TeletextResourceKind::Pop, 15)],
            limits(),
            budget()
        ),
        Err(TeletextObjectSubpageAnnouncementError::Level35Only { display: owner(0) })
    );
    assert_eq!(original, before);
    for original in [fixture(true, false, 1), fixture(true, true, 1)] {
        assert!(
            matches!(original.resolutions()[0].materialized_level25_links(),Err(TeletextDrcsExtensionError::UnprovenOtherRoles {display}) if display==owner(0))
        );
    }
}

#[test]
fn announcement_preserves_physical_overlays_and_real_structure_limits() {
    let physical = TeletextPhysicalBaseOverlay::new(
        owner(0),
        vec![TeletextPhysicalBaseRow::new(5, [b'P'; 40]).unwrap()],
    )
    .unwrap();
    let original = fixture(false, true, 0)
        .with_physical_base_overlays(vec![physical], limits())
        .unwrap();
    let before = original.clone();
    let add = request(owner(0), TeletextResourceKind::Pop, 15);
    // Two base pages, two metadata owners, three companions, one overlay/row.
    let exact = TeletextServiceBudget {
        max_structure_items: 9,
        max_base_content_bytes: 40,
        ..limits()
    };
    let changed = original
        .clone()
        .with_object_subpage_announcements(&[add], exact, budget())
        .unwrap();
    assert_eq!(
        changed.semantic().physical_base_overlays(),
        original.semantic().physical_base_overlays()
    );
    for denied in [
        TeletextServiceBudget {
            max_structure_items: 8,
            ..exact
        },
        TeletextServiceBudget {
            max_base_content_bytes: 39,
            ..exact
        },
    ] {
        assert!(matches!(
            original
                .clone()
                .with_object_subpage_announcements(&[add], denied, budget()),
            Err(TeletextObjectSubpageAnnouncementError::Service(
                TeletextServiceError::Budget { .. }
            ))
        ));
    }
    assert_eq!(original, before);
}
