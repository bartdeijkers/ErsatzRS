//! Whole-factory empty-call bound; no endpoint is fabricated for an empty body.
use super::*;

fn fixture(count: usize) -> (TeletextDeclaredService, TeletextObjectIdentity) {
    fixture_with_reset(count, None)
}

fn fixture_with_reset(
    count: usize,
    reset_before: Option<bool>,
) -> (TeletextDeclaredService, TeletextObjectIdentity) {
    let identity = TeletextObjectIdentity::local(owner(0), TeletextObjectId::new(65534));
    let mut draft = input();
    draft.objects.push(TeletextEnhancementObjectDefinition {
        identity,
        object_type: TeletextObjectType::Passive,
        program: TeletextEnhancementProgram::new(vec![]).unwrap(),
    });
    let mut commands = vec![
        TeletextEnhancementCommand::InvokeObject {
            target: identity,
            object_type: TeletextObjectType::Passive,
            offset: None,
        };
        count
    ];
    if let Some(before) = reset_before {
        commands.insert(
            if before { 0 } else { count },
            TeletextEnhancementCommand::SetScreenColour {
                colour: TeletextColourReference::BLACK,
            },
        );
    }
    draft.pages[0].local_program = Some(TeletextEnhancementProgram::new(commands).unwrap());
    (declared(draft), identity)
}

#[test]
fn whole_factory_retains_fifteen_empty_calls_without_inventing_intervals() {
    let (source, identity) = fixture(15);
    let result =
        encode_teletext_authored_service(source.clone(), &transmissions(&source), &limits())
            .expect("fifteen total instances cannot exceed fifteen simultaneous objects");
    assert_eq!(result.declared(), &source);
    let EncodedTeletextAuthoredPage::CompiledDisplay(page) = &result.pages()[0] else {
        panic!("the original explicit calls must retain their full X26 programme");
    };
    let normalized = page.normalized();
    for (trace, geometry) in [
        (
            normalized.original().spatial_trace(),
            normalized.original().display_geometry(),
        ),
        (
            normalized.physical().replay().spatial_trace(),
            normalized.physical().replay().display_geometry(),
        ),
    ] {
        assert_eq!(trace.invocations().len(), 15);
        assert_eq!(geometry.object_open_count().known_intervals(), 0);
        assert_eq!(geometry.object_open_count().pending_intervals(), 15);
        assert_eq!(
            geometry.successive_open_count().maximum_known_successive(),
            15
        );
        assert!(geometry.successive_open_count().pending().is_empty());
        for (index, call) in trace.invocations().iter().enumerate() {
            assert_eq!(call.target(), identity);
            assert_eq!(call.object_type(), TeletextObjectType::Passive);
            assert_eq!(call.parent(), None);
            assert_eq!(
                call.root_source(),
                TeletextSpatialRootSource::Local { command: index }
            );
            assert_eq!(
                geometry.object_regions()[index].interval(),
                TeletextObjectAddressInterval::Pending {
                    reason: TeletextObjectIntervalPending::NoDirectAddress,
                    endpoint: None,
                }
            );
        }
    }
    let baseline = encode_teletext_page(
        &source.semantic().base().pages[0],
        TeletextEncodingRegion::WesternLatin,
    )
    .unwrap();
    assert_eq!(
        page.page()
            .data_units()
            .iter()
            .filter(|u| packet(u) <= 25)
            .collect::<Vec<_>>(),
        baseline
            .data_units()
            .iter()
            .filter(|u| packet(u) <= 25)
            .collect::<Vec<_>>()
    );
    let projection = project_encoded_teletext_display(page.page(), 128).unwrap();
    let calls = projection
        .records
        .iter()
        .filter_map(|r| match r {
            TeletextEncodedDisplayRecord::Triplets { row: 26, words, .. } => Some(words),
            _ => None,
        })
        .flatten()
        .filter(|&&word| word & 0x3f >= 40 && (word >> 6) & 0x1f == 0x13)
        .count();
    assert_eq!(
        calls, 15,
        "Hamming-decoded native words must retain every empty invocation"
    );
    assert!(source
        .semantic()
        .objects()
        .definition(identity)
        .unwrap()
        .program
        .commands()
        .is_empty());
}

#[test]
fn whole_factory_sixteenth_empty_call_retains_original_successive_open_rejection() {
    let (source, _) = fixture(16);
    assert_successive_refusal(&source, 15);
}

#[test]
fn whole_factory_plane_colour_cannot_hide_sixteen_same_source_opens() {
    for before in [false, true] {
        let (source, _) = fixture_with_reset(16, Some(before));
        assert_successive_refusal(&source, if before { 16 } else { 15 });
    }
}

pub(super) fn original_successive_refusal(
    source: &TeletextDeclaredService,
) -> TeletextDisplayGeometryError {
    let original = source.clone();
    let result =
        encode_teletext_authored_service(source.clone(), &transmissions(source), &limits());
    if let (Ok(encoded), Some(directory)) = (
        &result,
        crate::locator::first_non_empty_env(&[
            "ERSATZRS_TELETEXT_TEST_SUCCESSIVE_PREFIX_FIXTURE_DIR",
        ]),
    ) {
        let mut units = encoded.pages()[0].data_units().to_vec();
        units.push(super::super::packets::terminator(2));
        std::fs::write(std::path::Path::new(&directory).join("unexpected.json"),
            serde_json::to_vec(&serde_json::json!({"page":512,"subcode":0,"units":units.iter().map(|u|u.to_vec()).collect::<Vec<_>>()})).unwrap()).unwrap();
    }
    assert!(
        result.is_err(),
        "A8(3): complete factory admitted16 successive same-source opens"
    );
    let error = result.unwrap_err();
    let Failure::Compiled(error) = error.reason else {
        panic!("explicit Main must retain the compiled-display failure")
    };
    let TeletextCompiledDisplayPageEncodingFailure::Normalization(error) = error.reason else {
        panic!("the original source must fail during normalization")
    };
    let error = match error.reason {
        TeletextNormalizedMainProgramFailure::Original(error) => error,
        other => panic!("original admission was not retained: {other:?}"),
    };
    let TeletextComposedMainEncodingError::Composition(error) = *error else {
        panic!("the original complete composition must retain its located refusal")
    };
    let TeletextMainCompositionFailure::DisplayGeometry(error) = error.reason else {
        panic!("original geometry must retain A8(3)")
    };
    assert_eq!(
        error.reason,
        TeletextDisplayGeometryFailure::TooManySuccessiveObjectOpens {
            count: 16,
            maximum: 15
        }
    );
    assert_eq!(source, &original);
    error
}

pub(super) fn assert_successive_refusal(
    source: &TeletextDeclaredService,
    command: usize,
) -> TeletextDisplayGeometryError {
    let error = original_successive_refusal(source);
    assert_eq!(
        error.location,
        Some(TeletextEnhancementSource {
            object: None,
            command
        })
    );
    assert_eq!(
        error.root_source,
        Some(TeletextSpatialRootSource::Local { command })
    );
    assert_eq!(error.invocation.unwrap().get(), 15);
    error
}
