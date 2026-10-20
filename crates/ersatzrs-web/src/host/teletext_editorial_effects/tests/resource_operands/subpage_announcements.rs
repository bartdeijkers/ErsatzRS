//! Real same-page retained banks, exact intent, immutable roles and packets.
use super::*;
use ersatzrs_app::teletext_design_documents::TeletextSavedObjectSubpageAnnouncement;

fn retained(global: bool) -> TeletextResourceIdentity {
    retained_index(global, 2)
}
fn retained_index(global: bool, index: u8) -> TeletextResourceIdentity {
    TeletextResourceIdentity::new(
        resource(global).kind(),
        resource(global).page(),
        TeletextResourceSubpageIndex::new(index).unwrap(),
    )
    .unwrap()
}
fn encode(declared: TeletextDeclaredService) -> EncodedTeletextAuthoredService {
    let transmissions: Vec<_> = declared
        .resource_inventory()
        .iter()
        .map(|resource| TeletextServiceResourceTransmission {
            resource: *resource,
            counter: TeletextResourceUpdateCounter::new(1).unwrap(),
            update: TeletextResourceFullUpdate::Changed,
            last_subpage: true,
        })
        .collect();
    encode_teletext_authored_service_before(
        declared,
        &transmissions,
        &policy().encoding,
        deadline(),
    )
    .unwrap()
}
fn original(global: bool, extra: bool) -> EncodedTeletextAuthoredService {
    let source = compile(Fixture {
        global,
        ..Fixture::default()
    });
    let banks: Vec<_> = [2, 4]
        .into_iter()
        .take(if extra { 2 } else { 1 })
        .map(|index| {
            let resource = retained_index(global, index);
            let definitions = if extra {
                vec![TeletextEnhancementObjectDefinition {
                    identity: TeletextObjectIdentity::public(
                        resource,
                        TeletextObjectId::new(65522),
                    )
                    .unwrap(),
                    object_type: TeletextObjectType::Passive,
                    program: TeletextEnhancementProgram::new(vec![at(0, glyph(b'Y'))]).unwrap(),
                }]
            } else {
                vec![]
            };
            TeletextObjectResourceAddition::new(resource, definitions, policy().validation.service)
                .unwrap()
        })
        .collect();
    encode(
        source
            .declared()
            .clone()
            .with_object_resource_additions(
                &banks,
                policy().validation.service,
                policy().validation.declarations,
            )
            .unwrap(),
    )
}
fn intent(global: bool) -> TeletextSavedObjectSubpageAnnouncement {
    TeletextSavedObjectSubpageAnnouncement::new(
        0,
        TeletextObjectSubpageAnnouncement::new(address(123), retained(global)).unwrap(),
    )
    .unwrap()
}
fn announced(
    before: &EncodedTeletextAuthoredService,
    intents: &[TeletextSavedObjectSubpageAnnouncement],
) -> EncodedTeletextAuthoredService {
    let intents: Vec<_> = intents
        .iter()
        .map(TeletextSavedObjectSubpageAnnouncement::announcement)
        .collect();
    encode(
        before
            .declared()
            .clone()
            .with_object_subpage_announcements(
                &intents,
                policy().validation.service,
                policy().validation.declarations,
            )
            .unwrap(),
    )
}
fn changes(intents: &[TeletextSavedObjectSubpageAnnouncement]) -> TeletextEditorialEdits<'_> {
    let mut edits = edits(&[]);
    edits.object_subpage_announcements = intents;
    edits
}
fn validate(
    before: &EncodedTeletextAuthoredService,
    after: &EncodedTeletextAuthoredService,
    incoming: &[TeletextSavedObjectSubpageAnnouncement],
    cumulative: &[TeletextSavedObjectSubpageAnnouncement],
) -> Result<()> {
    validate_teletext_editorial_transition_with_operands(
        before,
        after,
        &permissions(true),
        changes(incoming),
        changes(cumulative),
        &policy().encoding,
        deadline(),
    )
}
fn mutate_links(
    before: &EncodedTeletextAuthoredService,
    change: impl FnOnce(TeletextX27Level25Links) -> TeletextX27Level25Links,
) -> EncodedTeletextAuthoredService {
    let source = before.declared();
    let semantic = source.semantic();
    let mut pages: Vec<_> = semantic.page_enhancements().values().cloned().collect();
    let page = pages
        .iter_mut()
        .find(|page| page.address == address(123))
        .unwrap();
    page.compositional_links = Some(change(page.compositional_links.unwrap()));
    let semantic = TeletextSemanticService::new(
        TeletextResolvedServiceDraft {
            base: semantic.base().clone(),
            pages,
            magazines: semantic
                .magazine_presentations()
                .iter()
                .map(
                    |(&magazine, &presentation)| TeletextMagazinePresentationDraft {
                        magazine,
                        presentation,
                    },
                )
                .collect(),
            objects: semantic.objects().definitions().to_vec(),
            drcs: semantic.drcs().subtables().to_vec(),
        },
        policy().validation.service,
    )
    .unwrap();
    encode(
        TeletextDeclaredService::new(
            semantic,
            source.mots().to_vec(),
            source.selections().to_vec(),
            source.empty_object_pages().to_vec(),
            policy().validation.declarations,
        )
        .unwrap(),
    )
}
fn replace(
    links: TeletextX27Level25Links,
    role: TeletextX27LinkRole,
    mask: u16,
    validity: TeletextX27LinkValidity,
) -> TeletextX27Level25Links {
    let mut roles = TeletextX27LinkRole::ALL.map(|role| links.link(role));
    let old = links.link(role);
    roles[usize::from(role.wire_code())] = TeletextX27LinkDeclaration::new(
        old.target(),
        validity,
        TeletextSubpageRequirementMask::new(mask),
    )
    .unwrap();
    TeletextX27Level25Links::new(roles[0], roles[1], roles[2], roles[3]).unwrap()
}
#[test]
fn explicit_retained_pop_and_gpop_announcement_preserves_other_pages_and_packets() {
    for global in [false, true] {
        let before = original(global, false);
        let intents = [intent(global)];
        let after = announced(&before, &intents);
        validate(&before, &after, &intents, &intents).unwrap();
        assert_eq!(
            before.pages()[1].data_units(),
            after.pages()[1].data_units()
        );
        assert_eq!(before.magazine_units(), after.magazine_units());
        assert_eq!(
            before.declared().semantic().base(),
            after.declared().semantic().base()
        );
        assert!(validate(&before, &after, &[], &intents).is_err());
        assert!(validate(&before, &after, &intents, &[]).is_err());
        assert!(validate(&before, &after, &[intents[0], intents[0]], &intents).is_err());
        assert!(validate(&before, &after, &intents, &[intents[0], intents[0]]).is_err());
        assert!(validate(&after, &after, &intents, &intents).is_err());
        // Existing cumulative intent does not grant a fresh implicit change.
        validate(&after, &after, &[], &intents).unwrap();
    }
}
#[test]
fn announcement_rejects_extra_role_bits_changed_validity_and_absent_resource() {
    let before = original(false, true);
    let intents = [intent(false)];
    let exact = announced(&before, &intents);
    for changed in [
        mutate_links(&exact, |links| {
            replace(
                links,
                TeletextX27LinkRole::Pop,
                (1 << 15) | (1 << 2) | (1 << 4),
                TeletextX27LinkValidity::Level25And35,
            )
        }),
        mutate_links(&exact, |links| {
            replace(
                links,
                TeletextX27LinkRole::Pop,
                (1 << 15) | (1 << 2),
                TeletextX27LinkValidity::Level25Only,
            )
        }),
        mutate_links(&exact, |links| {
            replace(
                links,
                TeletextX27LinkRole::Gdrcs,
                u16::MAX,
                TeletextX27LinkValidity::Level25Only,
            )
        }),
    ] {
        assert!(validate(&before, &changed, &intents, &intents).is_err());
    }
    let absent = TeletextResourceIdentity::new(
        TeletextResourceKind::Pop,
        resource(false).page(),
        TeletextResourceSubpageIndex::new(3).unwrap(),
    )
    .unwrap();
    let unknown = [TeletextSavedObjectSubpageAnnouncement::new(
        0,
        TeletextObjectSubpageAnnouncement::new(address(123), absent).unwrap(),
    )
    .unwrap()];
    assert!(validate(&before, &exact, &unknown, &unknown).is_err());
    let wrong_document =
        [TeletextSavedObjectSubpageAnnouncement::new(1, intents[0].announcement()).unwrap()];
    assert!(validate(&before, &exact, &wrong_document, &intents).is_err());
}
