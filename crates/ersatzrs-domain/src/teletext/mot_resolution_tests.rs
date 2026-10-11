//! Declaration derivation only: no fetched resources, effective defaults or readiness.

use super::*;

fn page(magazine: u8, byte: u8) -> TeletextRawDataPageNumber {
    TeletextRawDataPageNumber::new(magazine, byte).unwrap()
}
fn target(magazine: u8, byte: u8) -> TeletextX27LinkTarget {
    TeletextX27LinkTarget::Page {
        page: page(magazine, byte),
    }
}
fn unused(magazine: u8) -> TeletextX27LinkTarget {
    TeletextX27LinkTarget::Unspecified {
        magazine: TeletextX27Magazine::new(magazine).unwrap(),
    }
}
fn display(number: u16, subcode: u16) -> TeletextPageAddress {
    TeletextPageAddress {
        page: TeletextPageNumber::new(number).unwrap(),
        subcode: TeletextSubcode::new(subcode).unwrap(),
    }
}
fn mot_identity(s1: u8) -> TeletextResourceIdentity {
    TeletextResourceIdentity::new(
        TeletextResourceKind::Mot,
        page(2, 0xfe),
        TeletextResourceSubpageIndex::new(s1).unwrap(),
    )
    .unwrap()
}
fn no_object(raw_number: u8) -> TeletextMotDefaultObject {
    TeletextMotDefaultObject::NoObject { raw_number }
}
fn default_object(
    kind: TeletextResourceKind,
    magazine: u8,
    byte: u8,
    s1: u8,
    id: u16,
) -> TeletextMotDefaultObject {
    let resource = TeletextResourceIdentity::new(
        kind,
        page(magazine, byte),
        TeletextResourceSubpageIndex::new(s1).unwrap(),
    )
    .unwrap();
    let target = match kind {
        TeletextResourceKind::Gpop => {
            TeletextObjectIdentity::global(resource, TeletextObjectId::new(id)).unwrap()
        }
        TeletextResourceKind::Pop => {
            TeletextObjectIdentity::public(resource, TeletextObjectId::new(id)).unwrap()
        }
        _ => panic!("default fixture must be an object resource"),
    };
    TeletextMotDefaultObject::Object {
        target,
        object_type: TeletextObjectType::Adaptive,
    }
}
fn object_link(
    target: TeletextX27LinkTarget,
    maximum: u8,
    supersedable: bool,
    fallback: u8,
    defaults: [TeletextMotDefaultObject; 2],
) -> TeletextMotObjectLink {
    TeletextMotObjectLink::new(
        target,
        TeletextMotSubpageExtent::new(maximum).unwrap(),
        supersedable,
        TeletextMotFallback::new(fallback).unwrap(),
        defaults,
    )
}
fn drcs_link(
    target: TeletextX27LinkTarget,
    maximum: u8,
    supersedable: bool,
) -> TeletextMotDrcsLink {
    TeletextMotDrcsLink::new(
        target,
        TeletextMotSubpageExtent::new(maximum).unwrap(),
        supersedable,
    )
}
fn index(value: u8) -> TeletextMotPublicLinkIndex {
    TeletextMotPublicLinkIndex::new(value).unwrap()
}

#[derive(Clone, Copy)]
struct Fixture {
    global_object: TeletextMotObjectLink,
    public_objects: [TeletextMotObjectLink; 7],
    global_drcs: TeletextMotDrcsLink,
    normal_drcs: [TeletextMotDrcsLink; 7],
    counts: TeletextMotEnhancementCounts,
}
impl Fixture {
    fn new(supersedable: bool) -> Self {
        let mut public_objects =
            [object_link(unused(8), 15, true, 15, [no_object(255), no_object(173)]); 7];
        public_objects[2] = object_link(
            target(3, 0xb0),
            4,
            supersedable,
            14,
            [
                default_object(TeletextResourceKind::Pop, 3, 0xb0, 4, 65535),
                no_object(201),
            ],
        );
        let mut normal_drcs = [drcs_link(unused(8), 15, true); 7];
        normal_drcs[6] = drcs_link(target(5, 0xd0), 6, supersedable);
        Self {
            global_object: object_link(
                target(8, 0xa0),
                1,
                supersedable,
                13,
                [
                    default_object(TeletextResourceKind::Gpop, 8, 0xa0, 1, 60000),
                    no_object(199),
                ],
            ),
            public_objects,
            global_drcs: drcs_link(target(4, 0xc0), 2, supersedable),
            normal_drcs,
            counts: TeletextMotEnhancementCounts::new(0x81, 255, 173),
        }
    }
    fn bank(self) -> TeletextMotLevel25Links {
        TeletextMotLevel25Links::new(
            self.global_object,
            self.public_objects,
            self.global_drcs,
            self.normal_drcs,
            self.counts,
        )
        .unwrap()
    }
    fn mot(self, associations: Vec<TeletextMotPageAssociation>) -> TeletextMotLevel25Declaration {
        TeletextMotLevel25Declaration::new(mot_identity(9), associations, self.bank()).unwrap()
    }
}
fn association(
    global_objects: bool,
    public_objects: Option<u8>,
    global_drcs: bool,
    normal_drcs: Option<u8>,
) -> TeletextMotPageAssociation {
    TeletextMotPageAssociation::new(
        0x23,
        TeletextMotAssociationSelection::new(global_objects, public_objects.map(index)),
        TeletextMotAssociationSelection::new(global_drcs, normal_drcs.map(index)),
    )
}
fn associated() -> TeletextMotPageAssociation {
    association(true, Some(3), true, Some(7))
}
fn mot_role(role: TeletextX27LinkRole) -> TeletextMotLinkRole {
    match role {
        TeletextX27LinkRole::Gpop => TeletextMotLinkRole::Gpop,
        TeletextX27LinkRole::Pop => TeletextMotLinkRole::Pop { index: index(3) },
        TeletextX27LinkRole::Gdrcs => TeletextMotLinkRole::Gdrcs,
        TeletextX27LinkRole::Drcs => TeletextMotLinkRole::Drcs { index: index(7) },
    }
}
fn original(role: TeletextX27LinkRole) -> (TeletextRawDataPageNumber, u8) {
    match role {
        TeletextX27LinkRole::Gpop => (page(8, 0xa0), 1),
        TeletextX27LinkRole::Pop => (page(3, 0xb0), 4),
        TeletextX27LinkRole::Gdrcs => (page(4, 0xc0), 2),
        TeletextX27LinkRole::Drcs => (page(5, 0xd0), 6),
    }
}
fn declaration(
    target: TeletextX27LinkTarget,
    validity: TeletextX27LinkValidity,
    mask: u16,
) -> TeletextX27LinkDeclaration {
    TeletextX27LinkDeclaration::new(target, validity, TeletextSubpageRequirementMask::new(mask))
        .unwrap()
}
fn x27_for(role: TeletextX27LinkRole, link: TeletextX27LinkDeclaration) -> TeletextX27Level25Links {
    let mut links = [declaration(unused(8), TeletextX27LinkValidity::Level35Only, 65535); 4];
    links[usize::from(role.wire_code())] = link;
    TeletextX27Level25Links::new(links[0], links[1], links[2], links[3]).unwrap()
}
fn candidate(role: TeletextX27LinkRole, supersedable: bool) -> TeletextMotResolutionCandidate {
    let (target, extent) = original(role);
    TeletextMotResolutionCandidate {
        target,
        extent: TeletextMotSubpageExtent::new(extent).unwrap(),
        supersedable,
        provenance: TeletextMotLinkProvenance::Mot {
            identity: mot_identity(9),
            role: mot_role(role),
        },
    }
}

#[test]
fn missing_x27_retains_all_four_associated_mot_roles_and_exact_owner() {
    let fixture = Fixture::new(false);
    let mot = fixture.mot(vec![associated()]);
    let exact_display = display(223, 0x123);
    let result = TeletextMotLevel25Resolution::resolve(&mot, exact_display, None).unwrap();
    assert_eq!(result.selected_mot(), mot_identity(9));
    assert_eq!(result.display(), exact_display);
    assert_eq!(result.association(), Some(associated()));
    assert_eq!(result.mot_links(), fixture.bank());
    assert_eq!(result.x27(), None);
    for role in TeletextX27LinkRole::ALL {
        let original = candidate(role, false);
        assert_eq!(
            *result.role(role),
            TeletextMotRoleResolution::Selected {
                target: original.target,
                requirements: TeletextMotResolutionRequirements::MotExtent(original.extent),
                provenance: original.provenance,
                original_mot: Some(original),
            }
        );
    }
    assert_eq!(*result.default_conflicts(), [None, None]);
}

#[test]
fn level35_only_never_supersedes_level25_but_authored_x27_is_retained() {
    for supersedable in [false, true] {
        let mot = Fixture::new(supersedable).mot(vec![associated()]);
        for role in TeletextX27LinkRole::ALL {
            let links = x27_for(
                role,
                declaration(
                    target(1, 0x20),
                    TeletextX27LinkValidity::Level35Only,
                    0x8000,
                ),
            );
            let result =
                TeletextMotLevel25Resolution::resolve(&mot, display(223, 7), Some(links)).unwrap();
            assert_eq!(result.x27(), Some(links));
            let original = candidate(role, supersedable);
            assert_eq!(
                *result.role(role),
                TeletextMotRoleResolution::Selected {
                    target: original.target,
                    requirements: TeletextMotResolutionRequirements::MotExtent(original.extent),
                    provenance: original.provenance,
                    original_mot: Some(original),
                }
            );
        }
    }
}

#[test]
fn supersedable_links_select_exact_x27_target_and_mask_without_retargeting_defaults() {
    let mot = Fixture::new(true).mot(vec![associated()]);
    for role in TeletextX27LinkRole::ALL {
        for validity in [
            TeletextX27LinkValidity::Level25Only,
            TeletextX27LinkValidity::Level25And35,
        ] {
            let exact_display = display(223, 0x123);
            let links = x27_for(role, declaration(target(1, 0x20), validity, 0x8001));
            let result =
                TeletextMotLevel25Resolution::resolve(&mot, exact_display, Some(links)).unwrap();
            assert_eq!(
                *result.role(role),
                TeletextMotRoleResolution::Selected {
                    target: page(1, 0x20),
                    requirements: TeletextMotResolutionRequirements::X27Mask(
                        TeletextSubpageRequirementMask::new(0x8001)
                    ),
                    provenance: TeletextMotLinkProvenance::X27 {
                        display: exact_display,
                        role
                    },
                    original_mot: Some(candidate(role, true)),
                }
            );
            let sources = result.default_sources();
            assert_eq!(sources[0].unwrap().link, mot.links().object_link(None));
            assert_eq!(
                sources[1].unwrap().link,
                mot.links().object_link(Some(index(3)))
            );
            assert_eq!(
                sources[1].unwrap().provenance,
                TeletextMotLinkProvenance::Mot {
                    identity: mot.identity(),
                    role: TeletextMotLinkRole::Pop { index: index(3) }
                }
            );
            assert_eq!(result.conditional_pop_fallback().unwrap().flags.get(), 14);
        }
    }
}

#[test]
fn non_supersedable_same_page_refines_exact_mask_not_extent_intersection() {
    let mot = Fixture::new(false).mot(vec![associated()]);
    for role in TeletextX27LinkRole::ALL {
        for mask in [0, 0x8000, 0x8001, 65535] {
            let (target_page, _) = original(role);
            let exact_display = display(223, 0x123);
            let result = TeletextMotLevel25Resolution::resolve(
                &mot,
                exact_display,
                Some(x27_for(
                    role,
                    declaration(
                        TeletextX27LinkTarget::Page { page: target_page },
                        TeletextX27LinkValidity::Level25Only,
                        mask,
                    ),
                )),
            )
            .unwrap();
            assert_eq!(
                *result.role(role),
                TeletextMotRoleResolution::Selected {
                    target: target_page,
                    requirements: TeletextMotResolutionRequirements::X27Mask(
                        TeletextSubpageRequirementMask::new(mask)
                    ),
                    provenance: TeletextMotLinkProvenance::MotPageWithX27Mask {
                        identity: mot.identity(),
                        role: mot_role(role),
                        display: exact_display
                    },
                    original_mot: Some(candidate(role, false)),
                }
            );
        }
    }
}

#[test]
fn non_supersedable_different_page_preserves_both_candidates_as_unresolved() {
    let mot = Fixture::new(false).mot(vec![associated()]);
    for role in TeletextX27LinkRole::ALL {
        let link = declaration(
            target(1, 0x20),
            TeletextX27LinkValidity::Level25And35,
            0x8000,
        );
        let result =
            TeletextMotLevel25Resolution::resolve(&mot, display(223, 7), Some(x27_for(role, link)))
                .unwrap();
        assert_eq!(
            *result.role(role),
            TeletextMotRoleResolution::Unresolved {
                reason: TeletextMotUnresolvedReason::NonSupersedablePageConflict,
                mot: candidate(role, false),
                x27: link,
            }
        );
    }
}

#[test]
fn explicit_unspecified_x27_does_not_erase_active_mot_even_if_supersedable() {
    for supersedable in [false, true] {
        let mot = Fixture::new(supersedable).mot(vec![associated()]);
        for role in TeletextX27LinkRole::ALL {
            let link = declaration(unused(7), TeletextX27LinkValidity::Level25Only, 65535);
            let result = TeletextMotLevel25Resolution::resolve(
                &mot,
                display(223, 7),
                Some(x27_for(role, link)),
            )
            .unwrap();
            assert_eq!(
                *result.role(role),
                TeletextMotRoleResolution::Unresolved {
                    reason: TeletextMotUnresolvedReason::UnspecifiedOverAssociatedMot,
                    mot: candidate(role, supersedable),
                    x27: link,
                }
            );
            assert_eq!(result.x27().unwrap().link(role).target().magazine(), 7);
        }
    }
}

#[test]
fn sparse_missing_association_is_no_selection_not_a_global_or_public_default() {
    let mot = Fixture::new(false).mot(vec![
        TeletextMotPageAssociation::new(0xab, associated().objects(), associated().drcs()),
        TeletextMotPageAssociation::new(0xff, associated().objects(), associated().drcs()),
    ]);
    let before = mot.clone();
    let result = TeletextMotLevel25Resolution::resolve(&mot, display(223, 0x123), None).unwrap();
    assert_eq!(result.association(), None);
    for role in TeletextX27LinkRole::ALL {
        assert_eq!(*result.role(role), TeletextMotRoleResolution::Unselected);
    }
    assert_eq!(
        result.default_sources()[0].unwrap().link,
        mot.links().object_link(None)
    );
    assert_eq!(result.default_sources()[1], None);
    assert_eq!(result.conditional_pop_fallback(), None);
    assert!(result.default_conflicts()[0].is_some());
    assert_eq!(result.mot_links(), mot.links());
    assert_eq!(mot, before);
    assert_eq!(mot.association(0xff), None);
}

#[test]
fn x27_selects_without_active_associated_mot_but_null_does_not_select() {
    for associations in [vec![], vec![association(false, None, false, None)]] {
        let mot = Fixture::new(false).mot(associations);
        for role in TeletextX27LinkRole::ALL {
            let exact_display = display(223, 0x123);
            let result = TeletextMotLevel25Resolution::resolve(
                &mot,
                exact_display,
                Some(x27_for(
                    role,
                    declaration(target(1, 0x20), TeletextX27LinkValidity::Level25Only, 0),
                )),
            )
            .unwrap();
            assert_eq!(
                *result.role(role),
                TeletextMotRoleResolution::Selected {
                    target: page(1, 0x20),
                    requirements: TeletextMotResolutionRequirements::X27Mask(
                        TeletextSubpageRequirementMask::new(0)
                    ),
                    provenance: TeletextMotLinkProvenance::X27 {
                        display: exact_display,
                        role
                    },
                    original_mot: None,
                }
            );
            let null = TeletextMotLevel25Resolution::resolve(
                &mot,
                exact_display,
                Some(x27_for(
                    role,
                    declaration(unused(7), TeletextX27LinkValidity::Level25Only, 65535),
                )),
            )
            .unwrap();
            assert_eq!(*null.role(role), TeletextMotRoleResolution::Unselected);
        }
    }
}

#[test]
fn cleared_global_flag_preserves_active_default_conflicts_even_when_x27_selects() {
    let mut fixture = Fixture::new(false);
    fixture.global_object = object_link(
        target(8, 0xa0),
        1,
        false,
        13,
        [
            default_object(TeletextResourceKind::Gpop, 8, 0xa0, 0, 1),
            default_object(TeletextResourceKind::Gpop, 8, 0xa0, 1, 60000),
        ],
    );
    let mot = fixture.mot(vec![association(false, Some(3), true, Some(7))]);
    let result = TeletextMotLevel25Resolution::resolve(
        &mot,
        display(223, 7),
        Some(x27_for(
            TeletextX27LinkRole::Gpop,
            declaration(target(1, 0x20), TeletextX27LinkValidity::Level25Only, 1),
        )),
    )
    .unwrap();
    assert!(matches!(
        result.role(TeletextX27LinkRole::Gpop),
        TeletextMotRoleResolution::Selected {
            original_mot: None,
            ..
        }
    ));
    for slot in 0_u8..2 {
        assert_eq!(
            result.default_conflicts()[usize::from(slot)],
            Some(TeletextMotGlobalDefaultConflict {
                slot,
                provenance: TeletextMotLinkProvenance::Mot {
                    identity: mot.identity(),
                    role: TeletextMotLinkRole::Gpop
                },
                declaration: fixture.global_object.defaults()[usize::from(slot)],
            })
        );
    }
    assert_eq!(
        result.default_sources()[0]
            .unwrap()
            .link
            .raw_fallback()
            .get(),
        13
    );
    assert_eq!(
        result.default_sources()[1].unwrap().link,
        fixture.public_objects[2]
    );
}

#[test]
fn ignored_and_dormant_global_selectors_do_not_create_default_conflicts() {
    let mut fixture = Fixture::new(false);
    fixture.global_object = object_link(
        unused(8),
        15,
        true,
        15,
        [
            TeletextMotDefaultObject::Dormant {
                object_type: TeletextObjectType::Passive,
                raw_number: 255,
            },
            no_object(173),
        ],
    );
    let mot = fixture.mot(vec![association(false, None, false, None)]);
    let result = TeletextMotLevel25Resolution::resolve(&mot, display(223, 7), None).unwrap();
    assert_eq!(*result.default_conflicts(), [None, None]);
    assert_eq!(
        result.default_sources()[0].unwrap().link,
        fixture.global_object
    );
    assert_eq!(result.mot_links(), fixture.bank());
}

#[test]
fn associated_unused_pop_preserves_original_dormant_slots_and_conditional_fallback() {
    let mut fixture = Fixture::new(false);
    fixture.public_objects[2] = object_link(
        unused(7),
        15,
        false,
        15,
        [
            TeletextMotDefaultObject::Dormant {
                object_type: TeletextObjectType::Active,
                raw_number: 255,
            },
            no_object(201),
        ],
    );
    let mot = fixture.mot(vec![associated()]);
    let result = TeletextMotLevel25Resolution::resolve(&mot, display(223, 7), None).unwrap();
    assert_eq!(
        *result.role(TeletextX27LinkRole::Pop),
        TeletextMotRoleResolution::Unselected
    );
    assert_eq!(
        result.default_sources()[1].unwrap().link,
        fixture.public_objects[2]
    );
    assert_eq!(
        result.conditional_pop_fallback(),
        Some(TeletextMotConditionalPopFallback {
            provenance: TeletextMotLinkProvenance::Mot {
                identity: mot.identity(),
                role: TeletextMotLinkRole::Pop { index: index(3) }
            },
            flags: TeletextMotFallback::new(15).unwrap(),
        })
    );
    let selected = TeletextMotLevel25Resolution::resolve(
        &mot,
        display(223, 7),
        Some(x27_for(
            TeletextX27LinkRole::Pop,
            declaration(
                target(1, 0x20),
                TeletextX27LinkValidity::Level25Only,
                0x8000,
            ),
        )),
    )
    .unwrap();
    assert!(matches!(
        selected.role(TeletextX27LinkRole::Pop),
        TeletextMotRoleResolution::Selected {
            original_mot: None,
            ..
        }
    ));
    assert_eq!(selected.default_sources(), result.default_sources());
    assert_eq!(
        selected.conditional_pop_fallback(),
        result.conditional_pop_fallback()
    );
}

#[test]
fn per_display_context_retains_exact_subcodes_and_explicit_selected_mot_s1() {
    let fixture = Fixture::new(true);
    let first = fixture.mot(vec![associated()]);
    let second =
        TeletextMotLevel25Declaration::new(mot_identity(15), vec![associated()], fixture.bank())
            .unwrap();
    let link = x27_for(
        TeletextX27LinkRole::Drcs,
        declaration(
            target(1, 0x20),
            TeletextX27LinkValidity::Level25Only,
            0x8000,
        ),
    );
    let a = TeletextMotLevel25Resolution::resolve(&first, display(223, 1), Some(link)).unwrap();
    let b =
        TeletextMotLevel25Resolution::resolve(&second, display(223, 0x123), Some(link)).unwrap();
    assert_ne!(a.display(), b.display());
    assert_ne!(a.selected_mot(), b.selected_mot());
    assert_eq!(a.selected_mot().logical_subpage().get(), 9);
    assert_eq!(b.selected_mot().logical_subpage().get(), 15);
    assert!(
        matches!(b.role(TeletextX27LinkRole::Drcs), TeletextMotRoleResolution::Selected { provenance: TeletextMotLinkProvenance::X27 { display: owner, .. }, original_mot: Some(TeletextMotResolutionCandidate { provenance: TeletextMotLinkProvenance::Mot { identity, .. }, .. }), .. } if *owner == display(223, 0x123) && identity.logical_subpage().get() == 15)
    );
    assert_eq!(a.display(), display(223, 1));
    assert_eq!(a.mot_links(), b.mot_links());
}

#[test]
fn mismatched_magazine_rejects_with_exact_owner_and_leaves_authored_context_unchanged() {
    let mot = Fixture::new(false).mot(vec![associated()]);
    let before = mot.clone();
    let requested = display(323, 0x123);
    assert_eq!(
        TeletextMotLevel25Resolution::resolve(&mot, requested, None),
        Err(TeletextMotResolutionError::MagazineOwner {
            display: requested,
            mot: mot.identity()
        })
    );
    assert_eq!(mot, before);
}
