//! Original charset authority is stricter than equivalent painted pixels.
use super::*;

mod call_lineage;

fn designation(code: u8) -> TeletextColumnOperation {
    TeletextColumnOperation::DesignateCharset {
        designation: TeletextEnhancementCharsetDesignation::new(code).unwrap(),
    }
}
fn at(column: u8, operation: TeletextColumnOperation) -> TeletextEnhancementCommand {
    TeletextEnhancementCommand::AtColumn { column, operation }
}
fn declared(code: u8, glyph: u8, pending: bool) -> TeletextDeclaredService {
    let policy = policy();
    let lifted = ersatzrs_ffmpeg::teletext::lift_teletext_v1_page_set(
        draft("ASCII"),
        TeletextEncodingRegion::WesternLatin,
        policy.validation.service,
    )
    .unwrap();
    let identity = TeletextObjectIdentity::local(address(123), TeletextObjectId::new(10));
    let pages = lifted
        .page_enhancements()
        .values()
        .cloned()
        .map(|mut page| {
            let mut commands = vec![
                TeletextEnhancementCommand::SetPosition {
                    position: TeletextEnhancementPosition::new(1, 8).unwrap(),
                },
                at(
                    8,
                    designation(if page.address == address(123) {
                        code
                    } else {
                        0
                    }),
                ),
            ];
            if pending && page.address == address(123) {
                commands.push(TeletextEnhancementCommand::InvokeObject {
                    target: identity,
                    object_type: TeletextObjectType::Active,
                    offset: None,
                });
            }
            commands.push(at(
                9,
                TeletextColumnOperation::G0 {
                    code: TeletextGlyphCode::new(glyph).unwrap(),
                },
            ));
            page.local_program = Some(TeletextEnhancementProgram::new(commands).unwrap());
            page
        })
        .collect();
    let objects = if pending {
        vec![TeletextEnhancementObjectDefinition {
            identity,
            object_type: TeletextObjectType::Active,
            program: TeletextEnhancementProgram::new(vec![at(0, designation(0))]).unwrap(),
        }]
    } else {
        vec![]
    };
    let semantic = TeletextSemanticService::new(
        TeletextResolvedServiceDraft {
            base: lifted.base().clone(),
            pages,
            magazines: vec![],
            objects,
            drcs: vec![],
        },
        policy.validation.service,
    )
    .unwrap();
    TeletextDeclaredService::new(
        semantic,
        vec![],
        [123, 124]
            .map(|page| TeletextDisplayMotSelection {
                display: address(page),
                mot: None,
            })
            .into(),
        vec![],
        policy.validation.declarations,
    )
    .unwrap()
}
fn encoded(code: u8, glyph: u8) -> EncodedTeletextAuthoredService {
    ersatzrs_ffmpeg::teletext::encode_teletext_authored_service_before(
        declared(code, glyph, false),
        &[],
        &policy().encoding,
        deadline(),
    )
    .unwrap()
}
fn composition(service: &EncodedTeletextAuthoredService) -> &TeletextMainComposition {
    display(&service.pages()[0]).unwrap().1
}
fn intent_at(composition: &TeletextMainComposition) -> &TeletextExecutedGlyphIntent {
    let cell = composition.display_resolution().cell(1, 9).unwrap();
    let origin = &composition.display_resolution().origins()[cell.origin().unwrap()];
    &composition.glyph_intent().records()[origin.candidate_index()]
}
fn assert_old_fence_equal(old: &TeletextMainComposition, new: &TeletextMainComposition) {
    let a = old.display_resolution().cell(1, 9).unwrap();
    let b = new.display_resolution().cell(1, 9).unwrap();
    assert_eq!(a.fragment(), b.fragment());
    assert!(same_attributes(a, b, &lineage::ProtectedLineage::default()));
    assert_eq!(
        origin_signature(old, a, None),
        origin_signature(new, b, None)
    );
    assert_eq!(
        project_teletext_comparison_glyph(old, a).unwrap(),
        project_teletext_comparison_glyph(new, b).unwrap()
    );
}
fn check(
    old: &EncodedTeletextAuthoredService,
    new: &EncodedTeletextAuthoredService,
    code: u8,
    width: u8,
) -> Result<()> {
    let changes = [TeletextLocalProgramOperandOverlay::new(
        address(123),
        1,
        8,
        designation(0),
        designation(code),
    )
    .unwrap()];
    validate_teletext_editorial_effects_with_operands(
        old,
        new,
        &project("zone", 1, 8, width, 1),
        TeletextEditorialEdits {
            rows: &[],
            operands: &changes,
            object_operands: &[],
            resource_operands: &[],
            resource_programs: &[],
            resource_additions: &[],
            object_programs: &[],
            main_programs: &[],
            bitmaps: &[],
        },
        &policy().encoding,
        deadline(),
    )
}
#[test]
fn protected_raw_charset_alias_cannot_hide_behind_identical_glyph_pixels() {
    let old = encoded(0, b'A');
    let new = encoded(1, b'A');
    let (a, b) = (composition(&old), composition(&new));
    assert_old_fence_equal(a, b);
    assert_eq!(intent_at(a).glyph(), intent_at(b).glyph());
    assert_ne!(
        intent_at(a).context().selection().unwrap().value(),
        intent_at(b).context().selection().unwrap().value()
    );
    let failure = check(&old, &new, 1, 1).unwrap_err();
    assert_eq!(
        failure.reason,
        TeletextEditorialEffectsFailure::ProtectedDisplay
    );
    assert_eq!(failure.position, Some((1, 9)));
    check(&old, &new, 1, 2).unwrap();
}
#[test]
fn protected_cross_repertoire_blank_retains_assigned_identity_not_only_zero_mask() {
    let old = encoded(0, b' ');
    let new = encoded(0x37, b' ');
    let (a, b) = (composition(&old), composition(&new));
    assert_old_fence_equal(a, b);
    assert_eq!(
        intent_at(a).disposition(),
        TeletextGlyphDisposition::Assigned
    );
    assert_eq!(
        intent_at(b).disposition(),
        TeletextGlyphDisposition::Assigned
    );
    assert_ne!(intent_at(a).glyph(), intent_at(b).glyph());
    for composition in [a, b] {
        let cell = composition.display_resolution().cell(1, 9).unwrap();
        assert_eq!(
            project_teletext_comparison_glyph(composition, cell)
                .unwrap()
                .mask
                .unwrap()
                .rows(),
            [0; 10]
        );
    }
    let failure = check(&old, &new, 0x37, 1).unwrap_err();
    assert_eq!(
        failure.reason,
        TeletextEditorialEffectsFailure::ProtectedDisplay
    );
    assert_eq!(failure.position, Some((1, 9)));
    check(&old, &new, 0x37, 2).unwrap();
}
fn pending_execution(code: u8) -> TeletextCharsetExecution {
    let service = declared(code, b'A', true);
    let baseline = encoded(0, b'A');
    let base = composition(&baseline).base_display();
    let spatial = resolve_teletext_spatial_trace(
        service.semantic(),
        address(123),
        TeletextSpatialTraceBudget {
            max_events: 128,
            max_invocations: 16,
        },
    )
    .unwrap();
    let layers = build_teletext_glyph_layers(base, &spatial, 128).unwrap();
    resolve_teletext_charset_execution(base, &spatial, &layers, 128).unwrap()
}
fn pending_choice(execution: &TeletextCharsetExecution) -> TeletextExecutedCharsetSelection {
    execution
        .records()
        .iter()
        .find_map(|record| {
            record
                .selection()
                .filter(|selection| selection.pending().is_some())
        })
        .unwrap()
}
#[test]
fn unchanged_pending_consensus_is_preserved_but_changed_raw_contender_is_protected() {
    let old = pending_execution(0);
    let new = pending_execution(1);
    let a = pending_choice(&old);
    let b = pending_choice(&new);
    assert!(a.value().is_none());
    assert!(b.value().is_none());
    let same = super::super::charset_context::same_selection;
    let lineage = super::super::lineage::ProtectedLineage::default();
    assert!(same(&old, &old, Some(a), Some(a), &lineage, deadline()).unwrap());
    assert!(!same(&old, &new, Some(a), Some(b), &lineage, deadline()).unwrap());
    assert_eq!(
        same(&old, &old, Some(a), Some(a), &lineage, Instant::now())
            .unwrap_err()
            .reason,
        TeletextEditorialEffectsFailure::TimedOut
    );
}
