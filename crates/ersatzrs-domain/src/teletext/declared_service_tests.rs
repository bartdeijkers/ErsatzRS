//! Actual declaration correspondence, not encoding or client acceptance.
use super::*;

#[path = "object_page_selection_tests.rs"]
mod object_page_selection_tests;

#[path = "object_resource_addition_declared_tests.rs"]
mod object_resource_addition_declared_tests;

#[path = "object_subpage_announcement_tests.rs"]
mod object_subpage_announcement_tests;

#[test]
fn drcs_bitmap_overlay_preserves_authenticated_mot_selections_defaults_and_inventory() {
    let resource = identity(TeletextResourceKind::Drcs, 0xc0, 1);
    let slot = TeletextDrcsGlyphSlot::new(0).unwrap();
    let old = TeletextDrcsBitmap::new([0x801; 10]).unwrap();
    let new = TeletextDrcsBitmap::new([0x803; 10]).unwrap();
    let mut input = draft();
    input.drcs.push(
        TeletextDrcsSubtable::new(
            resource,
            vec![TeletextDrcsGlyphDefinition { slot, bitmap: old }],
        )
        .unwrap(),
    );
    let original = bind(input.clone(), declaration(0, 1), vec![]).unwrap();
    let before = original.clone();
    let change = TeletextDrcsBitmapOverlay::new(resource, slot, old, new).unwrap();
    let limits = TeletextServiceBudget {
        max_records: 64,
        max_objects: 64,
        max_structure_items: 1024,
        max_base_content_bytes: 1024,
        max_commands: 1024,
        max_collected_commands: 1024,
        max_collected_drcs_references: 64,
    };
    let changed = original
        .clone()
        .with_drcs_bitmap_overlays(&[change], limits)
        .unwrap();
    input.drcs[0] = TeletextDrcsSubtable::new(
        resource,
        vec![TeletextDrcsGlyphDefinition { slot, bitmap: new }],
    )
    .unwrap();
    let expected = bind(input, declaration(0, 1), vec![]).unwrap();
    assert_eq!(changed, expected);
    assert_eq!(original, before);
    assert_ne!(changed.semantic(), original.semantic());
    assert!(!changed.defaults().is_empty());
    assert!(!changed.resolutions().is_empty());
    assert_eq!(changed.mots(), original.mots());
    assert_eq!(changed.resource_inventory(), original.resource_inventory());
}

fn owner(subcode: u16) -> TeletextPageAddress {
    TeletextPageAddress {
        page: TeletextPageNumber::new(200).unwrap(),
        subcode: TeletextSubcode::new(subcode).unwrap(),
    }
}
fn raw(byte: u8) -> TeletextRawDataPageNumber {
    TeletextRawDataPageNumber::new(2, byte).unwrap()
}
fn identity(kind: TeletextResourceKind, byte: u8, s1: u8) -> TeletextResourceIdentity {
    TeletextResourceIdentity::new(
        kind,
        raw(byte),
        TeletextResourceSubpageIndex::new(s1).unwrap(),
    )
    .unwrap()
}
fn target(byte: u8) -> TeletextObjectIdentity {
    TeletextObjectIdentity::public(
        identity(TeletextResourceKind::Pop, byte, 0),
        TeletextObjectId::new(0xffff),
    )
    .unwrap()
}
fn definition(byte: u8) -> TeletextEnhancementObjectDefinition {
    TeletextEnhancementObjectDefinition {
        identity: target(byte),
        object_type: TeletextObjectType::Passive,
        program: TeletextEnhancementProgram::new(vec![]).unwrap(),
    }
}
fn unused() -> TeletextX27LinkTarget {
    TeletextX27LinkTarget::Unspecified {
        magazine: TeletextX27Magazine::new(2).unwrap(),
    }
}
fn empty_object() -> TeletextMotObjectLink {
    object(
        unused(),
        0,
        [TeletextMotDefaultObject::NoObject { raw_number: 213 }; 2],
        0,
    )
}
fn object(
    page: TeletextX27LinkTarget,
    extent: u8,
    defaults: [TeletextMotDefaultObject; 2],
    flags: u8,
) -> TeletextMotObjectLink {
    TeletextMotObjectLink::new(
        page,
        TeletextMotSubpageExtent::new(extent).unwrap(),
        true,
        TeletextMotFallback::new(flags).unwrap(),
        defaults,
    )
}
fn active(byte: u8) -> TeletextMotDefaultObject {
    TeletextMotDefaultObject::Object {
        target: target(byte),
        object_type: TeletextObjectType::Passive,
    }
}
fn mot(
    public: [TeletextMotObjectLink; 7],
    global: TeletextMotObjectLink,
    global_association: bool,
) -> TeletextMotLevel25Declaration {
    let drcs = TeletextMotDrcsLink::new(unused(), TeletextMotSubpageExtent::new(0).unwrap(), true);
    TeletextMotLevel25Declaration::new(
        identity(TeletextResourceKind::Mot, 0xfe, 9),
        vec![TeletextMotPageAssociation::new(
            0,
            TeletextMotAssociationSelection::new(
                global_association,
                Some(TeletextMotPublicLinkIndex::new(1).unwrap()),
            ),
            TeletextMotAssociationSelection::new(false, None),
        )],
        TeletextMotLevel25Links::new(
            global,
            public,
            drcs,
            [drcs; 7],
            TeletextMotEnhancementCounts::new(0, 0, 0),
        )
        .unwrap(),
    )
    .unwrap()
}
fn declaration(extent: u8, flags: u8) -> TeletextMotLevel25Declaration {
    let mut public = [empty_object(); 7];
    public[0] = object(
        TeletextX27LinkTarget::Page { page: raw(0xa0) },
        extent,
        [
            active(0xa0),
            TeletextMotDefaultObject::NoObject { raw_number: 77 },
        ],
        flags,
    );
    mot(public, empty_object(), false)
}
fn draft() -> TeletextResolvedServiceDraft {
    let language = TeletextLanguage::new("nld".to_owned()).unwrap();
    let selection = TeletextDefaultObjectSelection {
        target: target(0xa0),
        object_type: TeletextObjectType::Passive,
    };
    TeletextResolvedServiceDraft {
        base: TeletextPageSetDraft {
            language: language.clone(),
            initial_page: owner(0).page,
            pages: vec![TeletextPageDraft {
                address: owner(0),
                kind: TeletextPageKind::FullPage,
                language,
                national_option: TeletextNationalOption::new(0).unwrap(),
                rows: vec![],
                navigation: vec![],
            }],
        },
        pages: vec![TeletextResolvedPageEnhancementDraft {
            address: owner(0),
            presentation: None,
            compositional_links: None,
            local_program: None,
            resources: TeletextResolvedPageResourceSelection {
                objects: TeletextResolvedObjectPageLinks {
                    public: Some(raw(0xa0)),
                    global: None,
                },
                drcs: TeletextDrcsPageLinks::unlinked(),
                defaults: [None, None, Some(selection), None],
            },
        }],
        magazines: vec![],
        objects: vec![definition(0xa0)],
        drcs: vec![],
    }
}
fn service(draft: TeletextResolvedServiceDraft) -> TeletextSemanticService {
    TeletextSemanticService::new(
        draft,
        TeletextServiceBudget {
            max_records: 64,
            max_objects: 64,
            max_structure_items: 1024,
            max_base_content_bytes: 1024,
            max_commands: 1024,
            max_collected_commands: 1024,
            max_collected_drcs_references: 64,
        },
    )
    .unwrap()
}
fn budget() -> TeletextDeclaredServiceBudget {
    TeletextDeclaredServiceBudget {
        max_mots: 8,
        max_displays: 8,
        max_resources: 64,
        max_defaults: 64,
        max_objects: 64,
    }
}
fn selection() -> Vec<TeletextDisplayMotSelection> {
    vec![TeletextDisplayMotSelection {
        display: owner(0),
        mot: Some(identity(TeletextResourceKind::Mot, 0xfe, 9)),
    }]
}
fn bind(
    input: TeletextResolvedServiceDraft,
    mot: TeletextMotLevel25Declaration,
    empty: Vec<TeletextResourceIdentity>,
) -> Result<TeletextDeclaredService, TeletextDeclaredServiceError> {
    // Invalid request companions are left for the wrapper's precise diagnostics;
    // the semantic constructor receives only legitimate empty owner records.
    let valid = empty
        .iter()
        .copied()
        .filter(|resource| {
            matches!(
                resource.kind(),
                TeletextResourceKind::Pop | TeletextResourceKind::Gpop
            ) && !input
                .objects
                .iter()
                .any(|object| object.identity.resource() == Some(*resource))
        })
        .collect();
    let semantic = TeletextSemanticService::new_with_object_companions(
        input,
        valid,
        TeletextServiceBudget {
            max_records: 64,
            max_objects: 64,
            max_structure_items: 1024,
            max_base_content_bytes: 1024,
            max_commands: 1024,
            max_collected_commands: 1024,
            max_collected_drcs_references: 64,
        },
    )
    .unwrap();
    TeletextDeclaredService::new(semantic, vec![mot], selection(), empty, budget())
}

fn links(byte: u8) -> TeletextX27Level25Links {
    let null = TeletextX27LinkDeclaration::new(
        unused(),
        TeletextX27LinkValidity::Level25Only,
        TeletextSubpageRequirementMask::new(0xffff),
    )
    .unwrap();
    let pop = TeletextX27LinkDeclaration::new(
        TeletextX27LinkTarget::Page { page: raw(byte) },
        TeletextX27LinkValidity::Level25Only,
        TeletextSubpageRequirementMask::new(1),
    )
    .unwrap();
    TeletextX27Level25Links::new(null, pop, null, null).unwrap()
}

#[test]
fn authentic_foreign_default_retains_mot_owner_slot_and_superseding_x27() {
    let mut input = draft();
    input.pages[0].compositional_links = Some(links(0xa1));
    input.pages[0].resources.objects.public = Some(raw(0xa1));
    input.objects.push(definition(0xa1));
    let original = service(input.clone());
    let checked = bind(input, declaration(0, 0), vec![]).unwrap();
    assert_eq!(checked.mots()[0].identity().logical_subpage().get(), 9);
    assert_eq!(checked.defaults()[0].selection().target, target(0xa0));
    assert_eq!(checked.defaults()[0].slot(), 0);
    assert_eq!(
        checked.resolutions()[0].defaults()[2].unwrap().target,
        target(0xa0)
    );
    assert!(
        matches!(checked.resolutions()[0].role(TeletextX27LinkRole::Pop), TeletextMotRoleResolution::Selected { target: page, original_mot: Some(original), .. } if *page == raw(0xa1) && original.target == raw(0xa0))
    );
    assert_eq!(checked.semantic().base(), original.base());
    assert_eq!(checked.semantic().objects(), original.objects());
    assert_eq!(
        checked.semantic().collected(owner(0)),
        original.collected(owner(0))
    );
    assert!(checked
        .resource_inventory()
        .contains(&identity(TeletextResourceKind::Mot, 0xfe, 9)));
}

#[test]
fn local_empty_suppresses_execution_but_cannot_hide_false_declared_defaults() {
    let mut input = draft();
    input.pages[0].local_program = Some(TeletextEnhancementProgram::new(vec![]).unwrap());
    let checked = bind(input.clone(), declaration(0, 0), vec![]).unwrap();
    assert_eq!(
        checked.semantic().effective_program(owner(0)).unwrap().0,
        TeletextEffectiveProgramSource::Local
    );
    assert_eq!(checked.defaults().len(), 1);
    input.pages[0].resources.defaults[2] = None;
    let error = bind(input, declaration(0, 0), vec![]).unwrap_err();
    assert_eq!(error.display, Some(owner(0)));
    assert_eq!(
        error.reason,
        TeletextDeclaredServiceFailure::DefaultSelection { slot: 2 }
    );
}

#[test]
fn explicit_empty_companion_fills_contiguous_extent_without_fake_definition() {
    let companion = identity(TeletextResourceKind::Pop, 0xa0, 1);
    let error = bind(draft(), declaration(1, 0), vec![]).unwrap_err();
    assert_eq!(error.resource, Some(companion));
    assert_eq!(
        error.reason,
        TeletextDeclaredServiceFailure::MissingResource
    );
    let checked = bind(draft(), declaration(1, 0), vec![companion]).unwrap();
    assert_eq!(checked.empty_object_pages(), &[companion]);
    assert_eq!(checked.resource_inventory().len(), 3);
    assert_eq!(checked.semantic().objects().definitions().len(), 1);
    assert_eq!(
        bind(
            draft(),
            declaration(0, 0),
            vec![identity(TeletextResourceKind::Pop, 0xa0, 0)]
        )
        .unwrap_err()
        .reason,
        TeletextDeclaredServiceFailure::CompanionOccupied
    );
    assert_eq!(
        bind(
            draft(),
            declaration(0, 0),
            vec![identity(TeletextResourceKind::Drcs, 0xb0, 0)]
        )
        .unwrap_err()
        .reason,
        TeletextDeclaredServiceFailure::CompanionKind
    );
}

#[test]
fn every_unassociated_bank_default_is_reserved_and_requires_real_definition() {
    let mut public = [empty_object(); 7];
    public[0] = declaration(0, 0)
        .links()
        .object_link(Some(TeletextMotPublicLinkIndex::new(1).unwrap()));
    public[1] = object(
        TeletextX27LinkTarget::Page { page: raw(0xb0) },
        0,
        [
            active(0xb0),
            TeletextMotDefaultObject::NoObject { raw_number: 0 },
        ],
        0,
    );
    let declared = mot(public, empty_object(), false);
    let missing = bind(
        draft(),
        declared.clone(),
        vec![identity(TeletextResourceKind::Pop, 0xb0, 0)],
    )
    .unwrap_err();
    assert_eq!(
        missing.reason,
        TeletextDeclaredServiceFailure::DefaultDefinition {
            role: TeletextMotLinkRole::Pop {
                index: TeletextMotPublicLinkIndex::new(2).unwrap()
            },
            slot: 0
        }
    );
    let mut input = draft();
    input.objects.push(definition(0xb0));
    let checked = bind(input, declared, vec![]).unwrap();
    assert_eq!(checked.defaults().len(), 2);
    assert_eq!(checked.defaults()[1].selection().target, target(0xb0));
}

#[test]
fn no_mot_selection_authenticates_x27_instead_of_manufacturing_defaults() {
    let mut input = draft();
    input.pages[0].compositional_links = Some(links(0xa0));
    input.pages[0].resources.defaults = [None; 4];
    let checked = TeletextDeclaredService::new(
        service(input.clone()),
        vec![],
        vec![TeletextDisplayMotSelection {
            display: owner(0),
            mot: None,
        }],
        vec![],
        budget(),
    )
    .unwrap();
    assert!(checked.resolutions()[0].mot().is_none());
    assert_eq!(*checked.resolutions()[0].defaults(), [None; 4]);
    input.pages[0].compositional_links = None;
    let error = TeletextDeclaredService::new(
        service(input),
        vec![],
        vec![TeletextDisplayMotSelection {
            display: owner(0),
            mot: None,
        }],
        vec![],
        budget(),
    )
    .unwrap_err();
    assert_eq!(
        error.reason,
        TeletextDeclaredServiceFailure::SelectedRole {
            role: TeletextX27LinkRole::Pop
        }
    );
}

#[test]
fn exact_selection_and_mot_identity_guards_preserve_full_subcode() {
    let original = service(draft());
    let check = |mots, selections| {
        TeletextDeclaredService::new(original.clone(), mots, selections, vec![], budget())
            .unwrap_err()
    };
    assert_eq!(
        check(vec![declaration(0, 0)], vec![]).reason,
        TeletextDeclaredServiceFailure::MissingSelection
    );
    assert_eq!(
        check(vec![declaration(0, 0)], vec![selection()[0]; 2]).reason,
        TeletextDeclaredServiceFailure::DuplicateSelection
    );
    let mut wrong = selection()[0];
    wrong.display = owner(1);
    assert_eq!(
        check(vec![declaration(0, 0)], vec![wrong]).reason,
        TeletextDeclaredServiceFailure::UnknownDisplay
    );
    assert_eq!(
        check(vec![], selection()).reason,
        TeletextDeclaredServiceFailure::MissingMot
    );
    assert_eq!(
        check(vec![declaration(0, 0); 2], selection()).reason,
        TeletextDeclaredServiceFailure::DuplicateMot
    );
    assert_eq!(original, service(draft()));
}

#[test]
fn raw_and_unique_budgets_are_distinct_and_zero_is_meaningful() {
    let original = service(draft());
    for (limited, category) in [
        (
            TeletextDeclaredServiceBudget {
                max_mots: 0,
                ..budget()
            },
            "MOT",
        ),
        (
            TeletextDeclaredServiceBudget {
                max_displays: 0,
                ..budget()
            },
            "display",
        ),
        (
            TeletextDeclaredServiceBudget {
                max_objects: 0,
                ..budget()
            },
            "object",
        ),
        (
            TeletextDeclaredServiceBudget {
                max_defaults: 0,
                ..budget()
            },
            "default",
        ),
        (
            TeletextDeclaredServiceBudget {
                max_resources: 1,
                ..budget()
            },
            "resource",
        ),
    ] {
        let error = TeletextDeclaredService::new(
            original.clone(),
            vec![declaration(0, 0)],
            selection(),
            vec![],
            limited,
        )
        .unwrap_err();
        assert!(
            matches!(error.reason, TeletextDeclaredServiceFailure::Budget { category: actual, .. } if actual == category)
        );
    }
    let exact = TeletextDeclaredServiceBudget {
        max_mots: 1,
        max_displays: 1,
        max_resources: 2,
        max_defaults: 1,
        max_objects: 1,
    };
    assert!(TeletextDeclaredService::new(
        original,
        vec![declaration(0, 0)],
        selection(),
        vec![],
        exact
    )
    .is_ok());
}

#[test]
fn null_over_associated_mot_and_non_supersedable_conflict_remain_located() {
    let mut input = draft();
    let null = TeletextX27LinkDeclaration::new(
        unused(),
        TeletextX27LinkValidity::Level25Only,
        TeletextSubpageRequirementMask::new(0xffff),
    )
    .unwrap();
    input.pages[0].compositional_links =
        Some(TeletextX27Level25Links::new(null, null, null, null).unwrap());
    let error = bind(input, declaration(0, 0), vec![]).unwrap_err();
    assert_eq!(error.display, Some(owner(0)));
    assert_eq!(
        error.reason,
        TeletextDeclaredServiceFailure::Unresolved {
            role: TeletextX27LinkRole::Pop,
            reason: TeletextMotUnresolvedReason::UnspecifiedOverAssociatedMot
        }
    );
    let original = declaration(0, 0)
        .links()
        .object_link(Some(TeletextMotPublicLinkIndex::new(1).unwrap()));
    let mut public = [empty_object(); 7];
    public[0] = TeletextMotObjectLink::new(
        original.target(),
        original.extent(),
        false,
        original.raw_fallback(),
        original.defaults(),
    );
    let mut input = draft();
    input.pages[0].compositional_links = Some(links(0xa1));
    input.pages[0].resources.objects.public = Some(raw(0xa1));
    input.objects.push(definition(0xa1));
    assert_eq!(
        bind(input, mot(public, empty_object(), false), vec![])
            .unwrap_err()
            .reason,
        TeletextDeclaredServiceFailure::Unresolved {
            role: TeletextX27LinkRole::Pop,
            reason: TeletextMotUnresolvedReason::NonSupersedablePageConflict
        }
    );
}

#[test]
fn advertised_unassociated_extent_requires_every_exact_subtable_kind() {
    let mut public = [empty_object(); 7];
    public[0] = declaration(0, 0)
        .links()
        .object_link(Some(TeletextMotPublicLinkIndex::new(1).unwrap()));
    public[6] = object(
        TeletextX27LinkTarget::Page { page: raw(0xb0) },
        1,
        [TeletextMotDefaultObject::NoObject { raw_number: 0 }; 2],
        0,
    );
    let declared = mot(public, empty_object(), false);
    let zero = identity(TeletextResourceKind::Pop, 0xb0, 0);
    let one = identity(TeletextResourceKind::Pop, 0xb0, 1);
    let missing = bind(draft(), declared.clone(), vec![zero]).unwrap_err();
    assert_eq!(missing.resource, Some(one));
    assert_eq!(missing.mot, Some(declared.identity()));
    let checked = bind(draft(), declared, vec![zero, one]).unwrap();
    assert_eq!(checked.resource_inventory().len(), 4);
}

#[test]
fn original_pop_flags_apply_without_promoting_x28_and_x26_does_not_disable_them() {
    let mut input = draft();
    input.pages[0].local_program = Some(TeletextEnhancementProgram::new(vec![]).unwrap());
    let original = service(input.clone());
    let checked = bind(input, declaration(0, 0b1110), vec![]).unwrap();
    assert!(checked.semantic().page_enhancements()[&owner(0)]
        .presentation
        .is_none());
    assert_eq!(
        checked.resolutions()[0]
            .conditional_pop_fallback()
            .unwrap()
            .flags
            .get(),
        14
    );
    let TeletextEffectivePresentation::MotFallback { defaults, .. } =
        checked.semantic().effective_presentation(owner(0)).unwrap()
    else {
        panic!("expected original POP fallback, not authored X28");
    };
    assert_eq!(defaults.panels.level25_widths(), (8, 8));
    assert!(defaults.black_background_substitution);
    assert_eq!(checked.semantic().base(), original.base());
    assert_eq!(
        checked.semantic().effective_program(owner(0)).unwrap().0,
        TeletextEffectiveProgramSource::Local
    );
}

#[test]
fn unused_pop_nontrivial_presentation_is_unresolved_unless_real_x28_overrides() {
    let mut input = draft();
    input.pages[0].resources.objects.public = None;
    input.pages[0].resources.defaults = [None; 4];
    let mut public = [empty_object(); 7];
    public[0] = object(
        unused(),
        0,
        [TeletextMotDefaultObject::NoObject { raw_number: 77 }; 2],
        2,
    );
    let declared = mot(public, empty_object(), false);
    let error = bind(input.clone(), declared.clone(), vec![]).unwrap_err();
    assert_eq!(
        error.reason,
        TeletextDeclaredServiceFailure::UnresolvedPresentation
    );
    assert_eq!(error.display, Some(owner(0)));
    input.pages[0].presentation = Some(
        resolve_teletext_mot_presentation_fallback(
            TeletextNationalOption::new(0).unwrap(),
            TeletextMotFallback::new(1).unwrap(),
        )
        .unwrap(),
    );
    let checked = bind(input, declared, vec![]).unwrap();
    assert!(matches!(
        checked.semantic().effective_presentation(owner(0)).unwrap(),
        TeletextEffectivePresentation::Extension {
            source: TeletextPresentationSource::Page,
            ..
        }
    ));
}

#[test]
fn defaults_cleared_global_association_and_foreign_magazine_are_not_authenticated() {
    let gpop = identity(TeletextResourceKind::Gpop, 0xb0, 0);
    let global_target = TeletextObjectIdentity::global(gpop, TeletextObjectId::new(123)).unwrap();
    let global = object(
        TeletextX27LinkTarget::Page { page: raw(0xb0) },
        0,
        [
            TeletextMotDefaultObject::Object {
                target: global_target,
                object_type: TeletextObjectType::Passive,
            },
            TeletextMotDefaultObject::NoObject { raw_number: 0 },
        ],
        0,
    );
    let mut public = [empty_object(); 7];
    public[0] = declaration(0, 0)
        .links()
        .object_link(Some(TeletextMotPublicLinkIndex::new(1).unwrap()));
    let mut input = draft();
    input.objects.push(TeletextEnhancementObjectDefinition {
        identity: global_target,
        object_type: TeletextObjectType::Passive,
        program: TeletextEnhancementProgram::new(vec![]).unwrap(),
    });
    assert_eq!(
        bind(input, mot(public, global, false), vec![])
            .unwrap_err()
            .reason,
        TeletextDeclaredServiceFailure::GlobalDefaultConflict { slot: 0 }
    );
    let declared = declaration(0, 0);
    let foreign_identity = TeletextResourceIdentity::new(
        TeletextResourceKind::Mot,
        TeletextRawDataPageNumber::new(3, 0xfe).unwrap(),
        TeletextResourceSubpageIndex::new(9).unwrap(),
    )
    .unwrap();
    let foreign = TeletextMotLevel25Declaration::new(
        foreign_identity,
        declared.associations().to_vec(),
        declared.links(),
    )
    .unwrap();
    let error = TeletextDeclaredService::new(
        service(draft()),
        vec![foreign],
        vec![TeletextDisplayMotSelection {
            display: owner(0),
            mot: Some(foreign_identity),
        }],
        vec![],
        budget(),
    )
    .unwrap_err();
    assert!(matches!(
        error.reason,
        TeletextDeclaredServiceFailure::Resolution(
            TeletextMotResolutionError::MagazineOwner { .. }
        )
    ));
    assert_eq!(error.mot, Some(foreign_identity));
}

#[test]
fn fully_empty_selected_pop_is_an_actual_companion_not_a_phantom_object() {
    let mut input = draft();
    input.objects.clear();
    input.pages[0].resources.defaults = [None; 4];
    input.pages[0].compositional_links = Some(links(0xa0));
    let mut public = [empty_object(); 7];
    public[0] = object(
        TeletextX27LinkTarget::Page { page: raw(0xa0) },
        0,
        [TeletextMotDefaultObject::NoObject { raw_number: 173 }; 2],
        0,
    );
    let resource = identity(TeletextResourceKind::Pop, 0xa0, 0);
    let checked = bind(input, mot(public, empty_object(), false), vec![resource]).unwrap();
    assert!(checked.semantic().objects().definitions().is_empty());
    assert_eq!(checked.semantic().object_companions(), &[resource]);
    assert_eq!(checked.empty_object_pages(), &[resource]);
    assert!(checked.defaults().is_empty());
}

#[test]
fn detached_companion_list_cannot_claim_another_semantic_inventory() {
    let original = service(draft());
    let companion = identity(TeletextResourceKind::Pop, 0xa0, 1);
    let error = TeletextDeclaredService::new(
        original,
        vec![declaration(1, 0)],
        selection(),
        vec![companion],
        budget(),
    )
    .unwrap_err();
    assert_eq!(
        error.reason,
        TeletextDeclaredServiceFailure::CompanionSelection
    );
}

#[test]
fn reauthentication_replaces_old_fallback_instead_of_retaining_stale_flags() {
    let first = bind(draft(), declaration(0, 14), vec![]).unwrap();
    let revised = TeletextDeclaredService::new(
        first.semantic().clone(),
        vec![declaration(0, 1)],
        selection(),
        vec![],
        budget(),
    )
    .unwrap();
    let TeletextEffectivePresentation::MotFallback { defaults, .. } =
        revised.semantic().effective_presentation(owner(0)).unwrap()
    else {
        panic!("expected newly derived fallback");
    };
    assert_eq!(defaults.panels.level25_widths(), (0, 0));
    assert!(!defaults.black_background_substitution);
    let TeletextEffectivePresentation::MotFallback { defaults, .. } =
        first.semantic().effective_presentation(owner(0)).unwrap()
    else {
        panic!("expected untouched first fallback");
    };
    assert_eq!(defaults.panels.level25_widths(), (8, 8));
    assert!(defaults.black_background_substitution);
    // With no active default slots, the same selected POP is independently
    // authenticated by X27 after the original MOT selection is removed.
    let mut input = draft();
    input.pages[0].resources.defaults = [None; 4];
    input.pages[0].compositional_links = Some(links(0xa0));
    let mut public = [empty_object(); 7];
    public[0] = object(
        TeletextX27LinkTarget::Page { page: raw(0xa0) },
        0,
        [TeletextMotDefaultObject::NoObject { raw_number: 0 }; 2],
        14,
    );
    let old = bind(input, mot(public, empty_object(), false), vec![]).unwrap();
    let without_mot = TeletextDeclaredService::new(
        old.semantic().clone(),
        vec![],
        vec![TeletextDisplayMotSelection {
            display: owner(0),
            mot: None,
        }],
        vec![],
        budget(),
    )
    .unwrap();
    assert!(matches!(
        without_mot
            .semantic()
            .effective_presentation(owner(0))
            .unwrap(),
        TeletextEffectivePresentation::Header { .. }
    ));
}

#[test]
fn host_absence_cannot_disable_the_actual_magazine_mot() {
    let declared = declaration(0, 0);
    let error = TeletextDeclaredService::new(
        service(draft()),
        vec![declared.clone()],
        vec![TeletextDisplayMotSelection {
            display: owner(0),
            mot: None,
        }],
        vec![],
        budget(),
    )
    .unwrap_err();
    assert_eq!(error.display, Some(owner(0)));
    assert_eq!(
        error.reason,
        TeletextDeclaredServiceFailure::MotSelection {
            expected: Some(declared.identity())
        }
    );
}

#[test]
fn two_logical_mot_owners_in_one_magazine_are_not_wire_selectable() {
    let first = declaration(0, 0);
    let second_identity = identity(TeletextResourceKind::Mot, 0xfe, 10);
    let second = TeletextMotLevel25Declaration::new(
        second_identity,
        first.associations().to_vec(),
        first.links(),
    )
    .unwrap();
    let error = TeletextDeclaredService::new(
        service(draft()),
        vec![first.clone(), second],
        selection(),
        vec![],
        budget(),
    )
    .unwrap_err();
    assert_eq!(error.mot, Some(second_identity));
    assert_eq!(
        error.reason,
        TeletextDeclaredServiceFailure::DuplicateMagazineMot {
            first: first.identity()
        }
    );
}

#[test]
fn exact_display_subcodes_share_the_actual_magazine_mot_context() {
    let mut input = draft();
    let mut page = input.base.pages[0].clone();
    page.address = owner(1);
    input.base.pages.push(page);
    let mut metadata = input.pages[0].clone();
    metadata.address = owner(1);
    input.pages.push(metadata);
    let original = service(input);
    let second = TeletextDisplayMotSelection {
        display: owner(1),
        mot: None,
    };
    let error = TeletextDeclaredService::new(
        original.clone(),
        vec![declaration(0, 0)],
        vec![selection()[0], second],
        vec![],
        budget(),
    )
    .unwrap_err();
    assert_eq!(error.display, Some(owner(1)));
    assert_eq!(
        error.reason,
        TeletextDeclaredServiceFailure::MotSelection {
            expected: selection()[0].mot
        }
    );
    let checked = TeletextDeclaredService::new(
        original,
        vec![declaration(0, 0)],
        vec![
            selection()[0],
            TeletextDisplayMotSelection {
                display: owner(1),
                mot: selection()[0].mot,
            },
        ],
        vec![],
        budget(),
    )
    .unwrap();
    assert_eq!(checked.resolutions().len(), 2);
    assert_eq!(checked.resolutions()[1].display(), owner(1));
    assert_eq!(
        checked.resolutions()[0].mot().unwrap().selected_mot(),
        checked.resolutions()[1].mot().unwrap().selected_mot()
    );
}

#[test]
fn separate_magazines_keep_independent_single_mots_and_nonzero_s1_provenance() {
    let first = declaration(0, 0);
    let second_identity = TeletextResourceIdentity::new(
        TeletextResourceKind::Mot,
        TeletextRawDataPageNumber::new(3, 0xfe).unwrap(),
        TeletextResourceSubpageIndex::new(7).unwrap(),
    )
    .unwrap();
    let second = TeletextMotLevel25Declaration::new(
        second_identity,
        first.associations().to_vec(),
        first.links(),
    )
    .unwrap();
    let mut input = draft();
    let second_owner = TeletextPageAddress {
        page: TeletextPageNumber::new(300).unwrap(),
        subcode: owner(0).subcode,
    };
    let mut page = input.base.pages[0].clone();
    page.address = second_owner;
    input.base.pages.push(page);
    let mut metadata = input.pages[0].clone();
    metadata.address = second_owner;
    input.pages.push(metadata);
    let checked = TeletextDeclaredService::new(
        service(input),
        vec![first.clone(), second],
        vec![
            selection()[0],
            TeletextDisplayMotSelection {
                display: second_owner,
                mot: Some(second_identity),
            },
        ],
        vec![],
        budget(),
    )
    .unwrap();
    assert_eq!(
        checked.resolutions()[0].mot().unwrap().selected_mot(),
        first.identity()
    );
    assert_eq!(
        checked.resolutions()[1].mot().unwrap().selected_mot(),
        second_identity
    );
    assert_eq!(checked.resource_inventory().len(), 3);
}

#[test]
fn local_object_overlay_preserves_declared_mot_inventory_defaults_and_original_selections() {
    let mut input = draft();
    let identity = TeletextObjectIdentity::local(owner(0), TeletextObjectId::new(65520));
    let old = TeletextColumnOperation::G0 {
        code: TeletextGlyphCode::new(b'A').unwrap(),
    };
    let new = TeletextColumnOperation::G0 {
        code: TeletextGlyphCode::new(b'B').unwrap(),
    };
    input.pages[0].local_program = Some(
        TeletextEnhancementProgram::new(vec![
            TeletextEnhancementCommand::SetPosition {
                position: TeletextEnhancementPosition::new(1, 8).unwrap(),
            },
            TeletextEnhancementCommand::InvokeObject {
                target: identity,
                object_type: TeletextObjectType::Passive,
                offset: None,
            },
        ])
        .unwrap(),
    );
    input.objects.push(TeletextEnhancementObjectDefinition {
        identity,
        object_type: TeletextObjectType::Passive,
        program: TeletextEnhancementProgram::new(vec![TeletextEnhancementCommand::AtColumn {
            column: 0,
            operation: old,
        }])
        .unwrap(),
    });
    let original = bind(input.clone(), declaration(0, 1), vec![]).unwrap();
    let overlay = TeletextLocalObjectOperandOverlay::new(identity, 0, 0, old, new).unwrap();
    let changed = original
        .clone()
        .with_local_object_operand_overlays(
            &[overlay],
            TeletextServiceBudget {
                max_records: 64,
                max_objects: 64,
                max_structure_items: 1024,
                max_base_content_bytes: 1024,
                max_commands: 1024,
                max_collected_commands: 1024,
                max_collected_drcs_references: 64,
            },
        )
        .unwrap();
    input.objects.last_mut().unwrap().program =
        TeletextEnhancementProgram::new(vec![TeletextEnhancementCommand::AtColumn {
            column: 0,
            operation: new,
        }])
        .unwrap();
    assert_eq!(changed, bind(input, declaration(0, 1), vec![]).unwrap());
    assert_eq!(changed.mots(), original.mots());
    assert_eq!(changed.resource_inventory(), original.resource_inventory());
    assert_eq!(changed.resolutions(), original.resolutions());
    assert_eq!(changed.defaults(), original.defaults());
    assert_ne!(changed.semantic(), original.semantic());
}

#[path = "declared_service_usage_tests.rs"]
mod usage_tests;
