//! Implicit MOT originals remain materializable after a selected X27 is present.
use super::*;
use ersatzrs_app::teletext_design_documents::TeletextSavedObjectPageSelectionChange;
use ersatzrs_domain::TeletextObjectPageSelectionChange;

fn bank(page: u8) -> TeletextResourceIdentity {
    TeletextResourceIdentity::new(
        TeletextResourceKind::Gpop,
        TeletextRawDataPageNumber::new(1, page).unwrap(),
        TeletextResourceSubpageIndex::new(0).unwrap(),
    )
    .unwrap()
}
fn declaration(global_associated: bool) -> TeletextMotLevel25Declaration {
    let absent = TeletextX27LinkTarget::Unspecified {
        magazine: TeletextX27Magazine::new(1).unwrap(),
    };
    let unused = TeletextMotObjectLink::new(
        absent,
        TeletextMotSubpageExtent::new(0).unwrap(),
        true,
        TeletextMotFallback::new(0).unwrap(),
        [TeletextMotDefaultObject::NoObject { raw_number: 0 }; 2],
    );
    let global = TeletextMotObjectLink::new(
        TeletextX27LinkTarget::Page {
            page: bank(0xa0).page(),
        },
        TeletextMotSubpageExtent::new(0).unwrap(),
        true,
        TeletextMotFallback::new(0).unwrap(),
        [TeletextMotDefaultObject::NoObject { raw_number: 0 }; 2],
    );
    let drcs = TeletextMotDrcsLink::new(absent, TeletextMotSubpageExtent::new(0).unwrap(), false);
    TeletextMotLevel25Declaration::new(
        TeletextResourceIdentity::new(
            TeletextResourceKind::Mot,
            TeletextRawDataPageNumber::new(1, 0xfe).unwrap(),
            TeletextResourceSubpageIndex::new(0).unwrap(),
        )
        .unwrap(),
        [0x23, 0x24]
            .map(|page| {
                TeletextMotPageAssociation::new(
                    page,
                    TeletextMotAssociationSelection::new(global_associated, None),
                    TeletextMotAssociationSelection::new(false, None),
                )
            })
            .into(),
        TeletextMotLevel25Links::new(
            global,
            [unused; 7],
            drcs,
            [drcs; 7],
            TeletextMotEnhancementCounts::new(1, 1, 0),
        )
        .unwrap(),
    )
    .unwrap()
}
fn check_original_mot(mot: &TeletextMotLevel25Declaration) -> TeletextMotResolutionCandidate {
    let original_candidate = TeletextMotResolutionCandidate {
        target: bank(0xa0).page(),
        extent: TeletextMotSubpageExtent::new(0).unwrap(),
        supersedable: true,
        provenance: TeletextMotLinkProvenance::Mot {
            identity: mot.identity(),
            role: TeletextMotLinkRole::Gpop,
        },
    };
    for page in [123, 124] {
        assert_eq!(
            *TeletextMotLevel25Resolution::resolve(&declaration(false), address(page), None)
                .unwrap()
                .role(TeletextX27LinkRole::Gpop),
            TeletextMotRoleResolution::Unselected,
        );
        assert_eq!(
            *TeletextMotLevel25Resolution::resolve(mot, address(page), None)
                .unwrap()
                .role(TeletextX27LinkRole::Gpop),
            TeletextMotRoleResolution::Selected {
                target: bank(0xa0).page(),
                requirements: TeletextMotResolutionRequirements::MotExtent(
                    original_candidate.extent
                ),
                provenance: original_candidate.provenance,
                original_mot: Some(original_candidate),
            },
        );
    }
    original_candidate
}
fn fixture() -> (
    EncodedTeletextAuthoredService,
    TeletextMotResolutionCandidate,
) {
    let lifted = ersatzrs_ffmpeg::teletext::lift_teletext_v1_page_set(
        draft("ASCII"),
        TeletextEncodingRegion::WesternLatin,
        policy().validation.service,
    )
    .unwrap();
    let mut pages = lifted
        .page_enhancements()
        .values()
        .cloned()
        .collect::<Vec<_>>();
    for page in &mut pages {
        page.compositional_links = None;
        page.resources.objects.global = Some(bank(0xa0).page());
    }
    let banks = vec![bank(0xa0), bank(0xa4)];
    let semantic = TeletextSemanticService::new_with_object_companions(
        TeletextResolvedServiceDraft {
            base: lifted.base().clone(),
            pages,
            magazines: vec![],
            objects: vec![],
            drcs: vec![],
        },
        banks.clone(),
        policy().validation.service,
    )
    .unwrap();
    let mot = declaration(true);
    let original_candidate = check_original_mot(&mot);
    let before = object_page_selections::encode(
        TeletextDeclaredService::new(
            semantic,
            vec![mot.clone()],
            [123, 124]
                .map(|page| TeletextDisplayMotSelection {
                    display: address(page),
                    mot: Some(mot.identity()),
                })
                .into(),
            banks,
            policy().validation.declarations,
        )
        .unwrap(),
    );
    (before, original_candidate)
}
fn selectors(
    before: &EncodedTeletextAuthoredService,
) -> [TeletextSavedObjectPageSelectionChange; 2] {
    [123, 124].map(|page| {
        let owner = address(page);
        TeletextSavedObjectPageSelectionChange::new(
            0,
            TeletextObjectPageSelectionChange::new(
                owner,
                TeletextX27LinkRole::Gpop,
                None,
                *before
                    .declared()
                    .resolutions()
                    .iter()
                    .find(|resolution| resolution.display() == owner)
                    .unwrap()
                    .role(TeletextX27LinkRole::Gpop),
                TeletextX27LinkDeclaration::new(
                    TeletextX27LinkTarget::Page {
                        page: bank(0xa4).page(),
                    },
                    TeletextX27LinkValidity::Level25Only,
                    TeletextSubpageRequirementMask::new(1),
                )
                .unwrap(),
            )
            .unwrap(),
        )
        .unwrap()
    })
}
fn check_selected_mot(
    after: &EncodedTeletextAuthoredService,
    original_candidate: TeletextMotResolutionCandidate,
) {
    for page in [123, 124] {
        assert_eq!(
            *after
                .declared()
                .resolutions()
                .iter()
                .find(|value| value.display() == address(page))
                .unwrap()
                .role(TeletextX27LinkRole::Gpop),
            TeletextMotRoleResolution::Selected {
                target: bank(0xa4).page(),
                requirements: TeletextMotResolutionRequirements::X27Mask(
                    TeletextSubpageRequirementMask::new(1)
                ),
                provenance: TeletextMotLinkProvenance::X27 {
                    display: address(page),
                    role: TeletextX27LinkRole::Gpop
                },
                original_mot: Some(original_candidate),
            },
        );
    }
}
#[test]
fn selected_page_effects_recover_original_implicit_mot_roles_on_repeat() {
    let (before, original_candidate) = fixture();
    let selectors = selectors(&before);
    let after = object_page_selections::encode(
        before
            .declared()
            .clone()
            .with_object_page_selections_and_program_overlays(
                &selectors
                    .iter()
                    .map(TeletextSavedObjectPageSelectionChange::change)
                    .collect::<Vec<_>>(),
                &[],
                policy().validation.service,
                policy().validation.declarations,
            )
            .unwrap(),
    );
    check_selected_mot(&after, original_candidate);
    let mut changed = edits(&[]);
    changed.object_page_selections = &selectors;
    for (old, incoming) in [(&before, changed), (&after, edits(&[]))] {
        validate_teletext_editorial_transition_with_operands(
            old,
            &after,
            &permissions(true),
            incoming,
            changed,
            &policy().encoding,
            deadline(),
        )
        .unwrap();
    }
}
