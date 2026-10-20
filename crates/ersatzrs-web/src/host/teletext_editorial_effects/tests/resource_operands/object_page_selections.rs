//! Complete selected-role and shared-consumer qualification with retained history.
use super::*;
use ersatzrs_app::teletext_design_documents::{
    TeletextSavedObjectPageSelectionChange, TeletextSavedObjectSubpageAnnouncement,
};
use ersatzrs_domain::{TeletextObjectPageSelectionChange, TeletextResourceObjectProgramOverlay};

pub(super) fn encode(declared: TeletextDeclaredService) -> EncodedTeletextAuthoredService {
    let transmissions = declared
        .resource_inventory()
        .iter()
        .map(|resource| TeletextServiceResourceTransmission {
            resource: *resource,
            counter: TeletextResourceUpdateCounter::new(1).unwrap(),
            update: TeletextResourceFullUpdate::Changed,
            last_subpage: true,
        })
        .collect::<Vec<_>>();
    encode_teletext_authored_service_before(
        declared,
        &transmissions,
        &policy().encoding,
        deadline(),
    )
    .unwrap()
}
fn selected_role(stable_global: bool) -> TeletextX27LinkRole {
    if stable_global {
        TeletextX27LinkRole::Pop
    } else {
        TeletextX27LinkRole::Gpop
    }
}
fn second_target(stable_global: bool) -> TeletextObjectIdentity {
    let resource = TeletextResourceIdentity::new(
        resource(!stable_global).kind(),
        TeletextRawDataPageNumber::new(1, 0xa4).unwrap(),
        TeletextResourceSubpageIndex::new(3).unwrap(),
    )
    .unwrap();
    if stable_global {
        TeletextObjectIdentity::public(resource, TeletextObjectId::new(2))
    } else {
        TeletextObjectIdentity::global(resource, TeletextObjectId::new(2))
    }
    .unwrap()
}
fn invocation(target: TeletextObjectIdentity) -> TeletextEnhancementProgram {
    TeletextEnhancementProgram::new(vec![TeletextEnhancementCommand::InvokeObject {
        target,
        object_type: TeletextObjectType::Passive,
        offset: None,
    }])
    .unwrap()
}
fn original(stable_global: bool) -> EncodedTeletextAuthoredService {
    let source = compile(Fixture {
        global: stable_global,
        ..Fixture::default()
    });
    let semantic = source.declared().semantic();
    let old_target = target(!stable_global, 1);
    let mut pages = semantic
        .page_enhancements()
        .values()
        .cloned()
        .collect::<Vec<_>>();
    for page in &mut pages {
        let commands = page
            .local_program
            .as_ref()
            .unwrap()
            .commands()
            .iter()
            .map(|command| match *command {
                TeletextEnhancementCommand::InvokeObject { target, offset, .. } => {
                    TeletextEnhancementCommand::InvokeObject {
                        target,
                        object_type: TeletextObjectType::Active,
                        offset,
                    }
                }
                command => command,
            })
            .collect();
        page.local_program = Some(TeletextEnhancementProgram::new(commands).unwrap());
        let mut roles =
            TeletextX27LinkRole::ALL.map(|role| page.compositional_links.unwrap().link(role));
        roles[usize::from(selected_role(stable_global).wire_code())] =
            TeletextX27LinkDeclaration::new(
                TeletextX27LinkTarget::Page {
                    page: resource(!stable_global).page(),
                },
                TeletextX27LinkValidity::Level25And35,
                TeletextSubpageRequirementMask::new(1 << 15),
            )
            .unwrap();
        page.compositional_links =
            Some(TeletextX27Level25Links::new(roles[0], roles[1], roles[2], roles[3]).unwrap());
        if stable_global {
            page.resources.objects.public = Some(resource(false).page());
        } else {
            page.resources.objects.global = Some(resource(true).page());
        }
    }
    let mut objects = semantic.objects().definitions().to_vec();
    let stable = objects
        .iter_mut()
        .find(|object| object.identity == target(stable_global, 65520))
        .unwrap();
    stable.object_type = TeletextObjectType::Active;
    stable.program = invocation(old_target);
    objects.push(TeletextEnhancementObjectDefinition {
        identity: second_target(stable_global),
        object_type: TeletextObjectType::Passive,
        program: TeletextEnhancementProgram::new(vec![at(0, glyph(b'Y'))]).unwrap(),
    });
    let semantic = TeletextSemanticService::new(
        TeletextResolvedServiceDraft {
            base: semantic.base().clone(),
            pages,
            magazines: vec![],
            objects,
            drcs: vec![],
        },
        policy().validation.service,
    )
    .unwrap();
    encode(
        TeletextDeclaredService::new(
            semantic,
            vec![],
            source.declared().selections().to_vec(),
            vec![],
            policy().validation.declarations,
        )
        .unwrap(),
    )
}
fn selections(
    before: &EncodedTeletextAuthoredService,
    stable_global: bool,
    reverse: bool,
) -> Vec<TeletextSavedObjectPageSelectionChange> {
    [123, 124]
        .into_iter()
        .map(|page| {
            let owner = address(page);
            let role = selected_role(stable_global);
            let resource = if reverse {
                resource(!stable_global)
            } else {
                second_target(stable_global).resource().unwrap()
            };
            TeletextSavedObjectPageSelectionChange::new(
                0,
                TeletextObjectPageSelectionChange::new(
                    owner,
                    role,
                    before.declared().semantic().page_enhancements()[&owner].compositional_links,
                    *before
                        .declared()
                        .resolutions()
                        .iter()
                        .find(|resolution| resolution.display() == owner)
                        .unwrap()
                        .role(role),
                    TeletextX27LinkDeclaration::new(
                        TeletextX27LinkTarget::Page {
                            page: resource.page(),
                        },
                        TeletextX27LinkValidity::Level25And35,
                        TeletextSubpageRequirementMask::new(1 << resource.logical_subpage().get()),
                    )
                    .unwrap(),
                )
                .unwrap(),
            )
            .unwrap()
        })
        .collect()
}
fn program(stable_global: bool, reverse: bool) -> TeletextResourceObjectProgramOverlay {
    let old = invocation(target(!stable_global, 1));
    let new = invocation(second_target(stable_global));
    let (old, new) = if reverse { (new, old) } else { (old, new) };
    TeletextResourceObjectProgramOverlay::new_with_invocation_changes(
        target(stable_global, 65520),
        old,
        new,
        vec![Some(0)],
        policy().validation.service,
    )
    .unwrap()
}
fn apply(
    before: &EncodedTeletextAuthoredService,
    selectors: &[TeletextSavedObjectPageSelectionChange],
    programs: &[TeletextResourceObjectProgramOverlay],
) -> EncodedTeletextAuthoredService {
    let changes = selectors
        .iter()
        .map(TeletextSavedObjectPageSelectionChange::change)
        .collect::<Vec<_>>();
    encode(
        before
            .declared()
            .clone()
            .with_object_page_selections_and_program_overlays(
                &changes,
                programs,
                policy().validation.service,
                policy().validation.declarations,
            )
            .unwrap(),
    )
}
fn edits_with<'a>(
    selectors: &'a [TeletextSavedObjectPageSelectionChange],
    programs: &'a [TeletextResourceObjectProgramOverlay],
) -> TeletextEditorialEdits<'a> {
    let mut edits = edits(&[]);
    edits.object_page_selections = selectors;
    edits.resource_programs = programs;
    edits
}
fn qualifies(
    before: &EncodedTeletextAuthoredService,
    after: &EncodedTeletextAuthoredService,
    incoming: TeletextEditorialEdits<'_>,
    cumulative: TeletextEditorialEdits<'_>,
    project: &TeletextExportProject,
) -> Result<()> {
    validate_teletext_editorial_transition_with_operands(
        before,
        after,
        project,
        incoming,
        cumulative,
        &policy().encoding,
        deadline(),
    )
}
#[test]
fn selected_roles_require_exact_programs_all_consumer_permissions_and_immutable_lifecycle() {
    for stable_global in [false, true] {
        let before = original(stable_global);
        let selectors = selections(&before, stable_global, false);
        let programs = [program(stable_global, false)];
        let after = apply(&before, &selectors, &programs);
        let incoming = edits_with(&selectors, &programs);
        qualifies(&before, &after, incoming, incoming, &permissions(true)).unwrap();
        for invalid in [
            edits_with(&[], &programs),
            edits_with(&selectors, &[]),
            edits(&[]),
        ] {
            assert!(qualifies(&before, &after, invalid, incoming, &permissions(true)).is_err());
        }
        let mut generated = permissions(true);
        generated.producers[0].authoring.as_mut().unwrap().pages[1].regions[0].kind =
            TeletextAuthoringRegionKindWire::Generated;
        for project in [permissions(false), generated] {
            assert_eq!(
                qualifies(&before, &after, incoming, incoming, &project)
                    .unwrap_err()
                    .reason,
                TeletextEditorialEffectsFailure::ProtectedDisplay
            );
        }
        let repeated = edits_with(&selectors, &programs);
        qualifies(&after, &after, edits(&[]), repeated, &permissions(true)).unwrap();
        assert!(qualifies(&after, &after, incoming, repeated, &permissions(true)).is_err());
        let reverse = selections(&after, stable_global, true);
        let reverse_programs = [program(stable_global, true)];
        let undone = apply(&after, &reverse, &reverse_programs);
        qualifies(
            &after,
            &undone,
            edits_with(&reverse, &reverse_programs),
            edits(&[]),
            &permissions(true),
        )
        .unwrap();
    }
}
#[test]
fn selection_preserves_historical_announcements_and_authenticates_final_full_roles() {
    for stable_global in [false, true] {
        let before = original(stable_global);
        // Existing old-page bit is a retained additive history, not final-page authority.
        let resource = TeletextResourceIdentity::new(
            resource(!stable_global).kind(),
            resource(!stable_global).page(),
            TeletextResourceSubpageIndex::new(2).unwrap(),
        )
        .unwrap();
        let declared = before
            .declared()
            .clone()
            .with_object_resource_additions(
                &[TeletextObjectResourceAddition::new(
                    resource,
                    vec![],
                    policy().validation.service,
                )
                .unwrap()],
                policy().validation.service,
                policy().validation.declarations,
            )
            .unwrap();
        let annotations = [123, 124].map(|page| {
            TeletextSavedObjectSubpageAnnouncement::new(
                0,
                TeletextObjectSubpageAnnouncement::new(address(page), resource).unwrap(),
            )
            .unwrap()
        });
        let announced = encode(
            declared
                .with_object_subpage_announcements(
                    &annotations
                        .iter()
                        .map(TeletextSavedObjectSubpageAnnouncement::announcement)
                        .collect::<Vec<_>>(),
                    policy().validation.service,
                    policy().validation.declarations,
                )
                .unwrap(),
        );
        let selectors = selections(&announced, stable_global, false);
        let programs = [program(stable_global, false)];
        let after = apply(&announced, &selectors, &programs);
        let incoming = edits_with(&selectors, &programs);
        let mut cumulative = incoming;
        cumulative.object_subpage_announcements = &annotations;
        qualifies(&announced, &after, incoming, cumulative, &permissions(true)).unwrap();
        qualifies(&after, &after, edits(&[]), cumulative, &permissions(true)).unwrap();
        let forged = selections(&before, stable_global, false);
        assert!(qualifies(
            &announced,
            &after,
            edits_with(&forged, &programs),
            cumulative,
            &permissions(true)
        )
        .is_err());
        // Immutable-root validation separately authenticates historical originals;
        // this current-to-current pass cannot invent an absent root declaration.
    }
}
