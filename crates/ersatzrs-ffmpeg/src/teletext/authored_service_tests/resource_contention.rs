//! A.7 instance priority is distinct from C.2 native word correspondence.
use super::*;

#[derive(Clone, Copy, Debug)]
enum Case {
    EmptyRepeat,
    PassiveRepeat,
    NestedPassiveRepeat,
    PassivePair,
    Mixed,
    Complementary,
    ActiveConflict,
    AdaptiveConflict,
}

fn at(column: u8, operation: TeletextColumnOperation) -> TeletextEnhancementCommand {
    TeletextEnhancementCommand::AtColumn { column, operation }
}
fn position(row: u8, column: u8) -> TeletextEnhancementCommand {
    TeletextEnhancementCommand::SetPosition {
        position: TeletextEnhancementPosition::new(row, column).unwrap(),
    }
}
fn body(global: bool, background: bool, empty: bool) -> TeletextEnhancementProgram {
    let column = if empty { 5 } else { 8 };
    let colour = |value| {
        let colour = TeletextColourReference::new(value).unwrap();
        at(
            column,
            if background {
                TeletextColumnOperation::Background { colour }
            } else {
                TeletextColumnOperation::Foreground { colour }
            },
        )
    };
    let mut commands = vec![
        position(0, 0),
        position(1, column),
        colour(if global { 2 } else { 4 }),
        colour(if global { 3 } else { 13 }),
    ];
    if !empty {
        commands.push(at(
            column,
            TeletextColumnOperation::G0 {
                code: TeletextGlyphCode::new(if global { b'G' } else { b'P' }).unwrap(),
            },
        ));
    }
    TeletextEnhancementProgram::new(commands).unwrap()
}
fn types(case: Case) -> [TeletextObjectType; 2] {
    use TeletextObjectType::{Active, Adaptive, Passive};
    match case {
        Case::Mixed => [Active, Passive],
        Case::Complementary | Case::ActiveConflict => [Active, Active],
        Case::AdaptiveConflict => [Adaptive, Adaptive],
        _ => [Passive, Passive],
    }
}
fn fixture(case: Case, reverse: bool) -> (TeletextDeclaredService, [TeletextObjectIdentity; 2]) {
    let empty = matches!(case, Case::EmptyRepeat);
    let public = TeletextObjectIdentity::public(
        resource(TeletextResourceKind::Pop, 0xa0, if empty { 5 } else { 0 }),
        TeletextObjectId::new(if empty { u16::MAX } else { 21 }),
    )
    .unwrap();
    let global = TeletextObjectIdentity::global(
        resource(TeletextResourceKind::Gpop, 0xb0, 0),
        TeletextObjectId::new(22),
    )
    .unwrap();
    let mut draft = input();
    let kinds = types(case);
    for (index, target) in [public, global].into_iter().enumerate() {
        draft.objects.push(TeletextEnhancementObjectDefinition {
            identity: target,
            object_type: kinds[index],
            program: body(
                index == 1,
                matches!(case, Case::Complementary) && index == 1,
                empty,
            ),
        });
    }
    let call = |index: usize| TeletextEnhancementCommand::InvokeObject {
        target: [public, global][index],
        object_type: kinds[index],
        offset: None,
    };
    let calls = if matches!(case, Case::NestedPassiveRepeat) {
        let parent = TeletextObjectIdentity::local(owner(0), TeletextObjectId::new(65534));
        draft.objects.push(TeletextEnhancementObjectDefinition {
            identity: parent,
            object_type: TeletextObjectType::Active,
            program: TeletextEnhancementProgram::new(vec![call(0), call(0)]).unwrap(),
        });
        vec![TeletextEnhancementCommand::InvokeObject {
            target: parent,
            object_type: TeletextObjectType::Active,
            offset: None,
        }]
    } else if matches!(case, Case::EmptyRepeat | Case::PassiveRepeat) {
        vec![call(0), call(0)]
    } else if reverse {
        vec![call(1), call(0)]
    } else {
        vec![call(0), call(1)]
    };
    draft.pages[0].local_program = Some(TeletextEnhancementProgram::new(calls).unwrap());
    draft.pages[0].resources.objects.global = Some(global.resource().unwrap().page());
    let link = |page, mask| {
        TeletextX27LinkDeclaration::new(
            page,
            TeletextX27LinkValidity::Level25Only,
            TeletextSubpageRequirementMask::new(mask),
        )
        .unwrap()
    };
    let unused = link(
        TeletextX27LinkTarget::Unspecified {
            magazine: TeletextX27Magazine::new(2).unwrap(),
        },
        u16::MAX,
    );
    draft.pages[0].compositional_links = Some(
        TeletextX27Level25Links::new(
            link(
                TeletextX27LinkTarget::Page {
                    page: global.resource().unwrap().page(),
                },
                1,
            ),
            link(
                TeletextX27LinkTarget::Page {
                    page: public.resource().unwrap().page(),
                },
                if empty { 1 << 5 } else { 1 },
            ),
            unused,
            unused,
        )
        .unwrap(),
    );
    if empty {
        draft.base.pages[0].rows[0].runs.push(TeletextRun {
            column: 5,
            style: TeletextStyle::default(),
            content: TeletextRunContent::Text { text: "Z".into() },
        });
    }
    (declared(draft), [public, global])
}
fn expected(case: Case, reverse: bool) -> (Option<usize>, u8, u8, usize) {
    match case {
        Case::EmptyRepeat => (None, 7, 0, 1),
        Case::PassiveRepeat => (Some(0), 13, 0, 1),
        Case::NestedPassiveRepeat => (Some(0), 13, 0, 2),
        Case::PassivePair => (
            Some(usize::from(!reverse)),
            if reverse { 13 } else { 3 },
            0,
            1,
        ),
        Case::Mixed => (Some(1), 3, 0, usize::from(!reverse)),
        Case::Complementary => (Some(usize::from(!reverse)), 13, 3, 1),
        _ => unreachable!(),
    }
}
fn check(case: Case, reverse: bool) -> Vec<[u8; 46]> {
    let (source, targets) = fixture(case, reverse);
    let unchanged = source.clone();
    let encoded =
        encode_teletext_authored_service(source, &transmissions(&unchanged), &limits()).unwrap();
    assert_eq!(encoded.declared(), &unchanged);
    let EncodedTeletextAuthoredPage::CompiledDisplay(page) = &encoded.pages()[0] else {
        panic!("compiled display required")
    };
    let (winner, fg, bg, invocation) = expected(case, reverse);
    let column = if matches!(case, Case::EmptyRepeat) {
        5
    } else {
        8
    };
    for resolution in [
        page.normalized().original().display_resolution(),
        page.normalized().physical().replay().display_resolution(),
    ] {
        let cell = resolution.cell(1, column).unwrap();
        assert_eq!(
            cell.attributes().foreground().value(),
            Some(TeletextColourReference::new(fg).unwrap()),
            "{case:?}/{reverse}"
        );
        assert_eq!(
            cell.attributes().background().value(),
            Some(TeletextColourReference::new(bg).unwrap()),
            "{case:?}/{reverse}"
        );
    }
    let original = page.normalized().original();
    let resolution = original.display_resolution();
    let origin = resolution.cell(1, column).unwrap().origin().unwrap();
    let candidate =
        &original.glyph_layers().candidates()[resolution.origins()[origin].candidate_index()];
    assert_eq!(
        candidate.source().and_then(|s| s.object),
        winner.map(|i| targets[i])
    );
    if winner.is_some() {
        assert_eq!(
            candidate.invocation().map(TeletextSpatialInvocationId::get),
            Some(invocation)
        );
        assert_eq!(
            candidate.root_source(),
            Some(TeletextSpatialRootSource::Local {
                command: if matches!(case, Case::NestedPassiveRepeat) {
                    0
                } else {
                    invocation
                }
            })
        );
    }
    let baseline = encode_teletext_page(
        &unchanged.semantic().base().pages[0],
        TeletextEncodingRegion::WesternLatin,
    )
    .unwrap();
    let actual = project_encoded_teletext_display(page.page(), 128).unwrap();
    let baseline = project_encoded_teletext_display(&baseline, 128).unwrap();
    assert_eq!(actual.header, baseline.header);
    let rows = |records: Vec<TeletextEncodedDisplayRecord>| {
        records
            .into_iter()
            .filter(|r| !matches!(r, TeletextEncodedDisplayRecord::Triplets { .. }))
            .collect::<Vec<_>>()
    };
    assert_eq!(rows(actual.records), rows(baseline.records));
    let resources = encoded
        .pages()
        .iter()
        .filter(|page| page.resource_header().is_some())
        .flat_map(EncodedTeletextAuthoredPage::data_units)
        .copied()
        .collect();
    if let Some(directory) = crate::locator::first_non_empty_env(&[
        "ERSATZRS_TELETEXT_TEST_RESOURCE_CONTENTION_FIXTURE_DIR",
    ]) {
        let mut carousel = TeletextCarousel::new_authored(encoded).unwrap();
        let cycles = [
            crate::teletext::carousel::authored_service_carousel_tests::cycle(&mut carousel),
            crate::teletext::carousel::authored_service_carousel_tests::cycle(&mut carousel),
        ];
        let units = cycles
            .iter()
            .flatten()
            .flat_map(TeletextCarouselBatch::data_units)
            .map(|u| u.to_vec())
            .collect::<Vec<_>>();
        std::fs::write(
            std::path::Path::new(&directory).join(format!("{case:?}-{reverse}.json")),
            serde_json::to_vec(&serde_json::json!({"page":512,"subcode":0,"units":units})).unwrap(),
        )
        .unwrap();
    }
    resources
}

#[test]
fn repeated_resource_attributes_require_no_cross_instance_clock_without_a_painted_cell() {
    let _ = check(Case::EmptyRepeat, false);
    assert_eq!(
        check(Case::PassiveRepeat, false),
        check(Case::NestedPassiveRepeat, false)
    );
}
#[test]
fn cross_resource_passive_overlap_uses_latest_invocation_not_page_storage_order() {
    assert_eq!(
        check(Case::PassivePair, false),
        check(Case::PassivePair, true)
    );
}
#[test]
fn cross_resource_mixed_types_preserve_passive_priority_in_both_call_orders() {
    assert_eq!(check(Case::Mixed, false), check(Case::Mixed, true));
}
#[test]
fn cross_resource_complementary_active_attributes_combine_without_a_time_tie() {
    assert_eq!(
        check(Case::Complementary, false),
        check(Case::Complementary, true)
    );
}
#[test]
fn cross_resource_forbidden_active_and_adaptive_overlaps_keep_original_refusal() {
    for case in [Case::ActiveConflict, Case::AdaptiveConflict] {
        let (source, _) = fixture(case, false);
        let error = compose_teletext_main_program(
            source.semantic(),
            owner(0),
            limits().normalization.composition,
        )
        .unwrap_err();
        let TeletextMainCompositionFailure::DisplayResolution(error) = error.reason else {
            panic!("A7 located overlap required")
        };
        assert!(matches!(
            error.reason,
            TeletextDisplayResolutionFailure::ObjectOverlap { .. }
        ));
        assert_eq!(
            error.invocation.map(TeletextSpatialInvocationId::get),
            Some(1)
        );
        assert_eq!(
            error.root_source,
            Some(TeletextSpatialRootSource::Local { command: 1 })
        );
        let saved = source.clone();
        let error = encode_teletext_authored_service(source, &transmissions(&saved), &limits())
            .unwrap_err();
        assert_eq!(error.display, Some(owner(0)));
        assert!(format!("{error:?}").contains("ObjectOverlap"));
    }
}
