//! Main absence changes preserve exact defaults and never inherit root authority.
use super::*;
use ersatzrs_domain::TeletextMainProgramPresenceOverlay;

fn bank() -> TeletextResourceIdentity {
    TeletextResourceIdentity::new(
        TeletextResourceKind::Gpop,
        TeletextRawDataPageNumber::new(1, 0xa0).unwrap(),
        TeletextResourceSubpageIndex::new(0).unwrap(),
    )
    .unwrap()
}
fn target() -> TeletextObjectIdentity {
    TeletextObjectIdentity::global(bank(), TeletextObjectId::new(1)).unwrap()
}
fn default_call() -> TeletextEnhancementCommand {
    TeletextEnhancementCommand::InvokeObject {
        target: target(),
        object_type: TeletextObjectType::Passive,
        offset: None,
    }
}
fn program(code: u8) -> TeletextEnhancementProgram {
    TeletextEnhancementProgram::new(vec![
        TeletextEnhancementCommand::SetPosition {
            position: TeletextEnhancementPosition::new(2, 8).unwrap(),
        },
        TeletextEnhancementCommand::AtColumn {
            column: 8,
            operation: TeletextColumnOperation::G0 {
                code: TeletextGlyphCode::new(code).unwrap(),
            },
        },
    ])
    .unwrap()
}
fn mot() -> TeletextMotLevel25Declaration {
    let absent = TeletextX27LinkTarget::Unspecified {
        magazine: TeletextX27Magazine::new(1).unwrap(),
    };
    let unused = TeletextMotObjectLink::new(
        absent,
        TeletextMotSubpageExtent::new(0).unwrap(),
        false,
        TeletextMotFallback::new(0).unwrap(),
        [TeletextMotDefaultObject::NoObject { raw_number: 0 }; 2],
    );
    let global = TeletextMotObjectLink::new(
        TeletextX27LinkTarget::Page {
            page: bank().page(),
        },
        TeletextMotSubpageExtent::new(0).unwrap(),
        false,
        TeletextMotFallback::new(0).unwrap(),
        [
            TeletextMotDefaultObject::Object {
                target: target(),
                object_type: TeletextObjectType::Passive,
            },
            TeletextMotDefaultObject::NoObject { raw_number: 0 },
        ],
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
                    TeletextMotAssociationSelection::new(true, None),
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
fn encode(declared: TeletextDeclaredService) -> EncodedTeletextAuthoredService {
    let transmissions = declared
        .resource_inventory()
        .iter()
        .map(
            |resource| ersatzrs_ffmpeg::teletext::TeletextServiceResourceTransmission {
                resource: *resource,
                counter: TeletextResourceUpdateCounter::new(1).unwrap(),
                update: TeletextResourceFullUpdate::Changed,
                last_subpage: true,
            },
        )
        .collect::<Vec<_>>();
    ersatzrs_ffmpeg::teletext::encode_teletext_authored_service_before(
        declared,
        &transmissions,
        &policy().encoding,
        deadline(),
    )
    .unwrap()
}
fn original() -> EncodedTeletextAuthoredService {
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
        page.local_program = None;
        page.resources.objects.global = Some(bank().page());
        page.resources.defaults = [
            Some(TeletextDefaultObjectSelection {
                target: target(),
                object_type: TeletextObjectType::Passive,
            }),
            None,
            None,
            None,
        ];
    }
    let semantic = TeletextSemanticService::new(
        TeletextResolvedServiceDraft {
            base: lifted.base().clone(),
            pages,
            magazines: vec![],
            objects: vec![TeletextEnhancementObjectDefinition {
                identity: target(),
                object_type: TeletextObjectType::Passive,
                program: program(b'A'),
            }],
            drcs: vec![],
        },
        policy().validation.service,
    )
    .unwrap();
    let mot = mot();
    encode(
        TeletextDeclaredService::new(
            semantic,
            vec![mot.clone()],
            [123, 124]
                .map(|page| TeletextDisplayMotSelection {
                    display: address(page),
                    mot: Some(mot.identity()),
                })
                .into(),
            vec![],
            policy().validation.declarations,
        )
        .unwrap(),
    )
}
fn apply(
    before: &EncodedTeletextAuthoredService,
    changes: &[TeletextMainProgramPresenceOverlay],
) -> EncodedTeletextAuthoredService {
    encode(
        before
            .declared()
            .clone()
            .with_main_program_presence_overlays(
                changes,
                policy().validation.service,
                policy().validation.declarations,
            )
            .unwrap(),
    )
}
fn edits(presence: &[TeletextMainProgramPresenceOverlay]) -> TeletextEditorialEdits<'_> {
    TeletextEditorialEdits {
        main_presence: presence,
        ..Default::default()
    }
}
fn permissions(editable: bool) -> TeletextExportProject {
    let mut value = project("zone", 1, 0, 40, 24);
    if !editable {
        value.producers[0].authoring.as_mut().unwrap().pages[0].regions[0].kind =
            TeletextAuthoringRegionKindWire::Generated;
    }
    value
}
fn qualifies(
    before: &EncodedTeletextAuthoredService,
    after: &EncodedTeletextAuthoredService,
    incoming: &[TeletextMainProgramPresenceOverlay],
    cumulative: &[TeletextMainProgramPresenceOverlay],
    editable: bool,
) -> Result<()> {
    validate_teletext_editorial_transition_with_operands(
        before,
        after,
        &permissions(editable),
        edits(incoming),
        edits(cumulative),
        &policy().encoding,
        deadline(),
    )
}
#[test]
fn main_presence_creation_empty_suppression_removal_and_repeat_have_exact_optional_bodies() {
    let before = original();
    for body in [
        program(b'Q'),
        TeletextEnhancementProgram::new(vec![]).unwrap(),
    ] {
        let creation =
            [
                TeletextMainProgramPresenceOverlay::new(address(123), None, Some(body.clone()))
                    .unwrap(),
            ];
        let after = apply(&before, &creation);
        qualifies(&before, &after, &creation, &creation, true).unwrap();
        assert_eq!(before.declared().mots(), after.declared().mots());
        assert_eq!(before.declared().defaults(), after.declared().defaults());
        assert_eq!(
            before.declared().resolutions(),
            after.declared().resolutions()
        );
        assert_eq!(
            before.pages()[1].data_units(),
            after.pages()[1].data_units()
        );
        assert!(qualifies(&before, &after, &[], &creation, true).is_err());
        assert!(qualifies(
            &before,
            &after,
            &[creation[0].clone(), creation[0].clone()],
            &creation,
            true
        )
        .is_err());
        assert!(qualifies(
            &before,
            &after,
            &creation,
            &[creation[0].clone(), creation[0].clone()],
            true
        )
        .is_err());
        let wrong =
            [
                TeletextMainProgramPresenceOverlay::new(address(123), None, Some(program(b'Z')))
                    .unwrap(),
            ];
        assert!(qualifies(&before, &after, &creation, &wrong, true).is_err());
        qualifies(&after, &after, &[], &creation, true).unwrap();
        assert!(qualifies(&after, &after, &creation, &creation, true).is_err());
        let removal =
            [TeletextMainProgramPresenceOverlay::new(address(123), Some(body), None).unwrap()];
        let restored = apply(&after, &removal);
        qualifies(&after, &restored, &removal, &[], true).unwrap();
        assert!(
            restored.declared().semantic().page_enhancements()[&address(123)]
                .local_program
                .is_none()
        );
        assert!(
            matches!(display(&restored.pages()[0]).unwrap().1.spatial_trace().original_program_source(),TeletextEffectiveProgramSource::Defaults {slots} if slots==&[0])
        );
        assert_eq!(
            restored.pages()[0].data_units(),
            before.pages()[0].data_units()
        );
    }
}
#[test]
fn equal_pixel_main_creation_and_default_reactivation_are_new_roots_in_protected_regions() {
    let before = original();
    let body = TeletextEnhancementProgram::new(vec![default_call()]).unwrap();
    let creation =
        [
            TeletextMainProgramPresenceOverlay::new(address(123), None, Some(body.clone()))
                .unwrap(),
        ];
    let after = apply(&before, &creation);
    let removal =
        [TeletextMainProgramPresenceOverlay::new(address(123), Some(body), None).unwrap()];
    for (old, new, incoming) in [(&before, &after, &creation), (&after, &before, &removal)] {
        let a = display(&old.pages()[0]).unwrap().1;
        let b = display(&new.pages()[0]).unwrap().1;
        let raster = |composition| {
            render_teletext_composition(
                composition,
                TeletextColourPixelOptions {
                    reveal_concealed: false,
                    flash_phase: TeletextFlashPhase::Original,
                },
                TeletextRasterBudget {
                    max_pixels: 168_000,
                    max_diagnostics: 1400,
                },
            )
            .unwrap()
        };
        assert_eq!(raster(a), raster(b));
        let lineage =
            lineage::ProtectedLineage::new(a, b, edits(incoming), &policy().encoding, deadline())
                .unwrap();
        for call in b.spatial_trace().invocations() {
            assert!(matches!(
                lineage.source_at_invocation(call.source(), Some(call.id())),
                lineage::CanonicalSource::Inserted(_)
            ));
            assert_eq!(lineage.root_source(call.root_source()), None);
        }
        assert_eq!(
            qualifies(
                old,
                new,
                incoming,
                if incoming[0].replacement().is_some() {
                    incoming
                } else {
                    &[]
                },
                false
            )
            .unwrap_err()
            .reason,
            TeletextEditorialEffectsFailure::ProtectedDisplay
        );
    }
}
#[test]
fn main_presence_does_not_consume_unapplied_scalar_or_unbounded_body_intents() {
    let before = original();
    let creation =
        [
            TeletextMainProgramPresenceOverlay::new(address(123), None, Some(program(b'Q')))
                .unwrap(),
        ];
    let after = apply(&before, &creation);
    let removal =
        [
            TeletextMainProgramPresenceOverlay::new(address(123), Some(program(b'Q')), None)
                .unwrap(),
        ];
    let scalar = [TeletextLocalProgramOperandOverlay::new(
        address(123),
        1,
        8,
        TeletextColumnOperation::G0 {
            code: TeletextGlyphCode::new(b'Q').unwrap(),
        },
        TeletextColumnOperation::G0 {
            code: TeletextGlyphCode::new(b'R').unwrap(),
        },
    )
    .unwrap()];
    let mut invalid = edits(&removal);
    invalid.operands = &scalar;
    assert!(validate_teletext_editorial_transition_with_operands(
        &after,
        &before,
        &permissions(true),
        invalid,
        edits(&[]),
        &policy().encoding,
        deadline()
    )
    .is_err());
    let mut budget = policy().encoding;
    budget.max_commands = 1;
    assert_eq!(
        validate_teletext_editorial_transition_with_operands(
            &before,
            &after,
            &permissions(true),
            edits(&creation),
            edits(&creation),
            &budget,
            deadline()
        )
        .unwrap_err()
        .reason,
        TeletextEditorialEffectsFailure::Budget
    );
    assert_eq!(
        validate_teletext_editorial_transition_with_operands(
            &before,
            &after,
            &permissions(true),
            edits(&creation),
            edits(&creation),
            &policy().encoding,
            Instant::now()
        )
        .unwrap_err()
        .reason,
        TeletextEditorialEffectsFailure::TimedOut
    );
}
