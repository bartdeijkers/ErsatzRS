//! Accepted context agreements reach the unchanged ordinary output provider.
//! The existing two-channel source/clock/transport fixture supplies no receiver proof.
use super::*;
use ersatzrs_domain::{
    TeletextPageCharsetDesignation, TeletextProducerPresentationSource,
    TeletextProducerSecondarySelection, TeletextSecondaryDefaultAgreement,
    TeletextSecondaryDefaultChoice,
};
use ersatzrs_web::host::teletext_editorial_designs::{
    select_teletext_editorial_secondary_default, NativeTeletextSecondaryDefaultRequest,
};

const DIRECTORY_ENV: &str = "ERSATZRS_TELETEXT_SECONDARY_HANDOVER_DIR";

fn contribution(theme: &str, page: u16, subcode: u16) -> Value {
    let mut value = authoring_metadata::response(theme, page, subcode);
    // ESC affects the rest of its row. Declare its entire intended footprint,
    // rather than allowing changed protected cells outside a smaller zone.
    for page in value["authoring"]["pages"].as_array_mut().unwrap() {
        page["regions"][0]["width"] = json!(39);
    }
    value
}

fn header_contribution() -> Value {
    let mut value = contribution("day", 124, 32);
    value["service"]["magazines"] = json!([]);
    for page in value["service"]["page_enhancements"]
        .as_array_mut()
        .unwrap()
    {
        page["presentation"] = Value::Null;
    }
    value
}

fn agreement(code: Option<u8>) -> TeletextSecondaryDefaultAgreement {
    TeletextSecondaryDefaultAgreement::new(code.map_or(
        TeletextSecondaryDefaultChoice::NoSecondG0 {},
        |code| TeletextSecondaryDefaultChoice::Designated {
            designation: TeletextPageCharsetDesignation::new(code).unwrap(),
        },
    ))
}

async fn prepare_body(fixture: &Fixture) -> SavedEditorialTeletextDesign {
    authoring_metadata::write(fixture, "day", &header_contribution());
    let candidate = prepare_with_policy(fixture, 1, channel_documents::policy()).await;
    let saved = designs::save(fixture, candidate, None).await;
    let exported = export_original_teletext_design(
        &fixture.state.application,
        NativeTeletextExportRequest {
            design_id: saved.head().id,
            revision: 1,
            design_digest: saved.digest().to_owned(),
            expected_design_head: 1,
            pages: None,
        },
        policy(),
        designs::deadline(),
    )
    .await
    .unwrap();
    let TeletextDesignMutation::Saved(exported) = exported else {
        panic!("the new immutable Header original must export")
    };
    // Resolve only explicitly approved uncertainty before authoring ESC; an
    // unbound original must continue rejecting DecoderDependentSecondary.
    let bound = select_teletext_editorial_secondary_default(
        &fixture.state.application,
        NativeTeletextSecondaryDefaultRequest {
            name: "Choose before ESC authoring".into(),
            receipt_id: exported.metadata().id,
            receipt_digest: exported.metadata().digest.clone(),
            expected_design_head: 1,
            producer: 0,
            agreement: Some(agreement(None)),
        },
        policy(),
        designs::deadline(),
    )
    .await
    .unwrap();
    let TeletextDesignMutation::Saved(bound) = bound else {
        panic!("explicit context must precede the ESC edit")
    };
    let exported = drcs_bitmap::reexport(fixture, &bound).await;
    let mut input = drcs_bitmap::from_receipt(&receipt(fixture, &exported).await, 2);
    let file = exported.sidecar().project.pages[0].name.clone();
    edit_cell(&mut input.files, &file, 2, 1, 0x1b);
    edit_cell(&mut input.files, &file, 2, 2, 0x23);
    let body = import(fixture, input).await;
    assert_eq!(body.head().revision, 3);
    body
}

async fn choose(
    fixture: &Fixture,
    prior: &SavedEditorialTeletextDesign,
    choice: TeletextSecondaryDefaultAgreement,
) -> SavedEditorialTeletextDesign {
    let exported = drcs_bitmap::reexport(fixture, prior).await;
    let result = select_teletext_editorial_secondary_default(
        &fixture.state.application,
        NativeTeletextSecondaryDefaultRequest {
            name: "Explicit ordinary-stream context".into(),
            receipt_id: exported.metadata().id,
            receipt_digest: exported.metadata().digest.clone(),
            expected_design_head: prior.head().revision,
            producer: 0,
            agreement: Some(choice),
        },
        policy(),
        designs::deadline(),
    )
    .await
    .unwrap();
    let TeletextDesignMutation::Saved(saved) = result else {
        panic!("the exact current context choice must save")
    };
    saved
}

async fn assert_context(
    fixture: &Fixture,
    candidate: &AcceptedTeletextOutputCandidate,
    choice: TeletextSecondaryDefaultAgreement,
) {
    let service = candidate.service();
    assert!(service.magazine_units().is_empty());
    let request = &candidate.generation().request;
    let restored = reload_original_teletext_design_revision(
        &fixture.state.application,
        request.design_id,
        request.revision,
        designs::budget(),
        channel_documents::policy(),
        designs::deadline(),
    )
    .await
    .unwrap()
    .unwrap();
    let producer = &restored.producers()[0];
    assert_eq!(producer.secondary_default_agreement(), Some(choice));
    assert!(producer
        .original()
        .semantic()
        .magazine_presentations()
        .is_empty());
    for page in producer.pages() {
        assert!(matches!(
            page.source(),
            TeletextProducerPresentationSource::Header { .. }
        ));
        assert_eq!(
            page.secondary_selection(),
            TeletextProducerSecondarySelection::DecoderDependent
        );
        let mut expected = *page.effective_defaults();
        expected.secondary = choice.secondary();
        assert_eq!(
            service.declared().semantic().page_enhancements()[&page.address()].presentation,
            Some(expected)
        );
        assert_eq!(expected.panels.level25_widths(), (0, 0));
        let overlay = &service.declared().semantic().physical_base_overlays()[&page.address()];
        let row = overlay.rows().iter().find(|row| row.row() == 2).unwrap();
        assert_eq!(&row.cells()[1..3], &[0x1b, 0x23]);
    }
    assert_secondary_intent(service, choice);
}

fn assert_secondary_intent(
    service: &EncodedTeletextAuthoredService,
    choice: TeletextSecondaryDefaultAgreement,
) {
    use ersatzrs_domain::{TeletextGlyphIdentity, TeletextNationalSubset};
    let expected = if choice.secondary().is_none() {
        TeletextNationalSubset::German
    } else {
        TeletextNationalSubset::English
    };
    let mut observed = false;
    for page in service.pages() {
        let main = match page {
            EncodedTeletextAuthoredPage::CompiledDisplay(page) => page.normalized().original(),
            EncodedTeletextAuthoredPage::NativeDisplay(page) => page.composition(),
            _ => continue,
        };
        for intent in main.glyph_intent().records() {
            let Some(base) = intent.context().candidate().base() else {
                continue;
            };
            if (base.position().row(), base.position().column()) == (2, 2) {
                let TeletextGlyphIdentity::G0 { glyph } = intent.glyph().unwrap() else {
                    panic!("the explicit ESC witness must retain G0 intent")
                };
                assert_eq!(glyph.national_subset(), Some(expected));
                observed = true;
            }
        }
    }
    assert!(observed);
}

async fn request_publication(
    fixture: &Fixture,
    concept: &SavedEditorialTeletextDesign,
    prior: &AcceptedTeletextOutputCandidate,
) -> TeletextPublicationRequest {
    let request =
        ersatzrs_web::host::teletext_designs::request_original_teletext_design_publication(
            &fixture.state.application,
            concept.head().id,
            concept.head().revision,
            concept.digest().to_owned(),
            concept.head().revision,
            Some(prior.generation().request.id),
            designs::budget(),
            channel_documents::policy(),
            designs::deadline(),
        )
        .await
        .unwrap();
    let TeletextPublicationMutation::Requested(request) = request else {
        panic!("validated agreement must request publication separately")
    };
    assert_eq!(
        fixture
            .state
            .application
            .accepted_teletext_generation(1)
            .await
            .unwrap(),
        Some(prior.generation().clone())
    );
    request.request().clone()
}

async fn accept(
    fixture: &Fixture,
    concept: &SavedEditorialTeletextDesign,
    pending: TeletextPublicationRequest,
    prior: &AcceptedTeletextOutputCandidate,
) -> AcceptedTeletextOutputCandidate {
    generations::accepted(
        accept_original_teletext_generation(
            &fixture.state.application,
            TeletextGenerationTarget {
                request: pending,
                expected_design_head: concept.head().revision,
                expected_generation_id: Some(prior.generation().id),
            },
            designs::budget(),
            channel_documents::policy(),
            designs::deadline(),
        )
        .await
        .unwrap(),
    )
}

async fn transition(
    fixture: &Fixture,
    flow: &Flow,
    origin: &live_tcp::Origin,
    directory: &Path,
) -> (AcceptedTeletextOutputCandidate, usize) {
    let mut current_pattern = pattern(&flow.accepted[0], 0);
    let night = pattern(&flow.accepted[1], 0);
    let before = pair(origin, [0; 2], &current_pattern, &night).await;
    for (index, cycle) in before.iter().enumerate() {
        write_units(
            directory,
            &format!("channel-{}-original-units.json", index + 1),
            cycle,
        );
    }
    assert!(!flow.accepted[0].service().magazine_units().is_empty());
    let mut concept = prepare_body(fixture).await;
    let mut accepted = None;
    let mut first_end = None;
    for (label, choice) in [
        ("no-second", agreement(None)),
        ("alias-10", agreement(Some(0x10))),
    ] {
        if choice.secondary().is_some() {
            concept = choose(fixture, &concept, choice).await;
        }
        let prior = accepted.as_ref().unwrap_or(&flow.accepted[0]);
        // Concept creation cannot change the original running generation.
        let positions = [
            origin.captures()[0].position(),
            origin.captures()[2].position(),
        ];
        pair(origin, positions, &current_pattern, &night).await;
        let pending = request_publication(fixture, &concept, prior).await;
        // A pending request must still deliver the old complete cycle, not
        // merely leave the durable accepted pointer unchanged.
        pair(origin, positions, &current_pattern, &night).await;
        let next = accept(fixture, &concept, pending, prior).await;
        assert_context(fixture, &next, choice).await;
        let next_pattern = pattern(&next, 0);
        let changed = pair(origin, positions, &next_pattern, &night).await;
        let repeated = pair(
            origin,
            [changed[0].end, changed[1].end],
            &next_pattern,
            &night,
        )
        .await;
        for (index, cycle) in changed.iter().enumerate() {
            write_units(
                directory,
                &format!("channel-{}-{label}-units.json", index + 1),
                cycle,
            );
            write_units(
                directory,
                &format!("channel-{}-{label}-repeat-units.json", index + 1),
                &repeated[index],
            );
        }
        first_end.get_or_insert(changed[0].end);
        current_pattern = next_pattern;
        accepted = Some(next);
    }
    tokio::time::sleep(Duration::from_secs(2)).await;
    (accepted.unwrap(), first_end.unwrap())
}

fn assert_no_magazine_tail(capture: &Capture, after: usize) {
    let mut cursor = after;
    let mut observed = false;
    loop {
        let units = capture.ordered_units_after(cursor, MAX_UNITS);
        observed |= !units.is_empty();
        assert!(units.iter().all(|(_, unit)| !matches!(
            observe_dvb_teletext_header(unit).unwrap(),
            TeletextHeaderObservation::NotHeader(address) if address.packet_number == 29
        )));
        if units.len() < MAX_UNITS {
            break;
        }
        let last = units.last().unwrap().0;
        let complete = units
            .iter()
            .rev()
            .find(|(offset, _)| *offset < last)
            .unwrap()
            .0;
        assert!(complete > cursor);
        cursor = complete;
    }
    assert!(observed);
}

#[tokio::test]
async fn secondary_agreements_keep_original_unknown_context_and_esc_intent_through_publication() {
    let (fixture, flow) = setup_with_response(contribution).await;
    let retained = Retained::capture(&fixture).await;
    let mut concept = prepare_body(&fixture).await;
    let mut accepted = None;
    for choice in [agreement(None), agreement(Some(0x10))] {
        if choice.secondary().is_some() {
            concept = choose(&fixture, &concept, choice).await;
        }
        let prior = accepted.as_ref().unwrap_or(&flow.accepted[0]);
        let pending = request_publication(&fixture, &concept, prior).await;
        let next = accept(&fixture, &concept, pending, prior).await;
        assert_context(&fixture, &next, choice).await;
        retained.assert_unchanged(&fixture, &flow).await;
        accepted = Some(next);
    }
    assert_eq!(concept.head().revision, 4);
}

#[tokio::test]
#[ignore = "requires pinned FFmpeg9/TSDuck and a create-new context handover directory"]
async fn ordinary_secondary_agreements_replace_magazine_effects_on_original_responses() {
    let directory = PathBuf::from(client_capture::selected(DIRECTORY_ENV));
    std::fs::create_dir(&directory).unwrap();
    live_tcp::terminal_trace(&directory).unwrap();
    let tools = Tools::load().await;
    let (mut fixture, flow, source) = Box::pin(super::super::super::source_fixture_with_capacity(
        &tools,
        &directory,
        contribution,
        channel_documents::policy,
        authoring_metadata::write,
        2,
    ))
    .await;
    assert_eq!(
        ersatzrs_infra::transcode_capacity::load_active_transcode_limit(&fixture.state.pool)
            .await
            .unwrap(),
        Some(2)
    );
    let retained = Retained::capture(&fixture).await;
    let mut origin = live_tcp::Origin::start(&fixture, &directory).await.unwrap();
    let mut observers = [
        live_tcp::Observer::start_for(&origin, 1, "day"),
        live_tcp::Observer::start_for(&origin, 2, "night"),
    ];
    let outcome = AssertUnwindSafe(async {
        await_two_channels(&origin).await;
        tokio::time::timeout(
            Duration::from_secs(55),
            transition(&fixture, &flow, &origin, &directory),
        )
        .await
        .expect("existing55s handover bound")
    })
    .catch_unwind()
    .await;
    let stopped = AssertUnwindSafe(async {
        for observer in &mut observers {
            observer.close().await.unwrap();
        }
        origin.close().await.unwrap();
        live_tcp::wait_for_cleanup().await.unwrap();
    })
    .catch_unwind()
    .await;
    live_tcp::save_captures(&directory, &origin).unwrap();
    if let Err(panic) = stopped {
        fixture.state.pool.close().await;
        std::panic::resume_unwind(panic);
    }
    let (accepted, after) = match outcome {
        Ok(value) => value,
        Err(panic) => {
            fixture.state.pool.close().await;
            std::panic::resume_unwind(panic)
        }
    };
    assert_eq!(origin.requests_seen(), [true, false, true]);
    assert_no_magazine_tail(&origin.captures()[0], after);
    retained.assert_unchanged(&fixture, &flow).await;
    generations::assert_same_output_width(&recover(&fixture, 1).await, &accepted, 40 * 12);
    fixture = fixture.reopen().await;
    retained.assert_unchanged(&fixture, &flow).await;
    generations::assert_same_output_width(&recover(&fixture, 1).await, &accepted, 40 * 12);
    assert_context(&fixture, &accepted, agreement(Some(0x10))).await;
    assert!(fixture.playout.try_recv().is_err());
    fixture.state.pool.close().await;
    let reports = verify_captures(&directory, &tools, &source, &origin).await;
    live_tcp::write_new(&directory.join("secondary-handover.json"), &serde_json::to_vec_pretty(&json!({
        "schema_version":1,"capacity":2,"requests_per_channel":1,"source_resets_after_start":0,
        "choices":["no_second_g0","designation_0x10"],"concept_revisions":[3,4],
        "source_sha256":live_tcp::hash(&source),"channels":reports,
        "original_sources_profiles_bindings_receipts_night_preserved":true,
        "same_response_complete_cycles_and_no_m29_tail":true,"explicit_publication":true,
        "durable_reopen":true,"owned_cleanup":true,
        "limitations":"Native Linux ordinary-stream context/AV/transport component only; no independent warm receiver, basic receiver agreement, external editor, platform or complete9A acceptance."
    })).unwrap()).unwrap();
}
