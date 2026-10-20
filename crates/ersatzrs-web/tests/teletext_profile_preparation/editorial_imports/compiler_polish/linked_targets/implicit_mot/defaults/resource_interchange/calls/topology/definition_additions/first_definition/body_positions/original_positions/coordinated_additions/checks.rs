//! Independent complete producer oracles and exact saved-role checks.
use super::*;
use ersatzrs_domain::TeletextDeclaredService;

pub(super) struct Expected {
    declared: TeletextDeclaredService,
    pub records: [Vec<TeletextTtiRecord>; 2],
    units: Vec<Vec<[u8; 46]>>,
    raster: TeletextRaster,
}

fn response(theme: &str, page: u16, subcode: u16, stage: u8) -> Value {
    let root = original_outer_rows(theme, page, subcode);
    let mut response = with_definitions(theme, page, subcode, 2);
    response["service"]["objects"][0]["program"] = root["service"]["objects"][0]["program"].clone();
    if stage != 0 {
        response["service"]["objects"][0]["program"]["commands"].as_array_mut().unwrap().splice(2..2, [
            json!({"kind":"set_position","position":{"row":2,"column":0}}),
            json!({"kind":"at_column","column":6,"operation":{"kind":"g0","code":if stage==1 {82} else {83}}}),
        ]);
        response["service"]["objects"][1]["program"]["commands"] = json!([
            {"kind":"set_position","position":{"row":1,"column":0}},
            {"kind":"at_column","column":8,"operation":{"kind":"g0","code":if stage==1 {80} else {81}}},
        ]);
    }
    validate_response(&response);
    response
}
pub(super) fn added(theme: &str, page: u16, subcode: u16) -> Value {
    response(theme, page, subcode, 0)
}
pub(super) fn edited(theme: &str, page: u16, subcode: u16) -> Value {
    response(theme, page, subcode, 1)
}
pub(super) fn repeated(theme: &str, page: u16, subcode: u16) -> Value {
    response(theme, page, subcode, 2)
}

pub(super) async fn qualify(response: fn(&str, u16, u16) -> Value) -> Expected {
    let (fixture, flow) = Box::pin(setup_with_fixture_policy_and_writer(
        response,
        Fixture::new_persistent().await,
        encoding,
        authoring_metadata::write_compact,
    ))
    .await;
    let service = flow.accepted[0].service();
    let expected = Expected {
        declared: service.declared().clone(),
        records: std::array::from_fn(|index| {
            file_records(
                &flow.exported[0],
                support::name(&flow.exported[0], identity(index)),
            )
        }),
        units: service
            .pages()
            .iter()
            .map(|page| page.data_units().to_vec())
            .collect(),
        raster: generations::preview(service).raster().clone(),
    };
    fixture.state.pool.close().await;
    expected
}

fn roles(stored: &TeletextDesignRevision, expected: &Expected, positions: bool) {
    let document = decode_teletext_design_documents(&stored.document, designs::budget()).unwrap();
    let overlay = document.editorial_overlay().unwrap();
    assert_eq!(overlay.schema_version(), if positions { 19 } else { 18 });
    let [addition] = overlay.resource_object_definition_additions() else {
        panic!("only actual admin-added26")
    };
    assert_eq!(addition.definition().identity, identity(1));
    assert_eq!(
        addition.definition().object_type,
        TeletextObjectType::Active
    );
    assert_eq!(
        Some(addition.definition()),
        expected
            .declared
            .semantic()
            .objects()
            .definition(identity(1))
    );
    assert!(overlay.resource_object_operands().is_empty());
    let programs = overlay.resource_object_program_overlays();
    if positions {
        let [program] = programs else {
            panic!("only producer-original25 has original program ancestry")
        };
        assert_eq!(program.identity(), identity(0));
        assert!(program.is_original_position_intent());
        assert_eq!(
            program.retained_commands(),
            [Some(0), Some(1), None, None, Some(2), Some(3)]
        );
    } else {
        assert!(programs.is_empty());
        assert!(addition.definition().program.commands().is_empty());
    }
    assert!(
        overlay.pages().is_empty()
            && overlay.local_program_operands().is_empty()
            && overlay.local_object_operands().is_empty()
            && overlay.local_object_glyph_program_overlays().is_empty()
            && overlay.drcs_bitmap_overlays().is_empty()
    );
}

pub(super) async fn revision(
    fixture: &Fixture,
    flow: &Flow,
    stored: &TeletextDesignRevision,
    expected: &Expected,
    positions: bool,
) {
    roles(stored, expected, positions);
    let restored = reload_original_teletext_design_revision(
        &fixture.state.application,
        stored.design_id,
        stored.revision,
        designs::budget(),
        encoding(),
        designs::deadline(),
    )
    .await
    .unwrap()
    .unwrap();
    let original = flow.saved[0].candidate();
    assert_eq!(restored.selection(), original.selection());
    assert_eq!(restored.metadata(), original.metadata());
    assert_eq!(restored.transmissions(), original.transmissions());
    assert_eq!(restored.sources().len(), original.sources().len());
    for (after, before) in restored.sources().iter().zip(original.sources()) {
        assert_eq!(after.bytes(), before.bytes());
        assert_eq!(after.authoring(), before.authoring());
    }
    assert_eq!(
        restored.producers()[0].original(),
        original.producers()[0].original()
    );
    let service = restored.encoded().unwrap();
    assert_eq!(service.declared(), &expected.declared);
    assert_eq!(service.declared().resource_inventory(), resources());
    assert_eq!(
        service.declared().semantic().objects().definitions().len(),
        2
    );
    assert_eq!(
        service
            .pages()
            .iter()
            .map(|page| page.data_units().to_vec())
            .collect::<Vec<_>>(),
        expected.units
    );
    assert_eq!(generations::preview(service).raster(), &expected.raster);
    assert_eq!(
        generations::preview(service).raster(),
        generations::preview(flow.accepted[0].service()).raster(),
        "both changed bodies are uncalled"
    );
    let preview = preview_original_teletext_design(
        &fixture.state.application,
        TeletextAuthoringPreviewRequest {
            channel_id: 1,
            design_id: stored.design_id,
            revision: stored.revision,
            owner: Some(channel_documents::address(124, 32)),
        },
        designs::deadline(),
    )
    .await
    .unwrap();
    assert_eq!(preview.object_resources(), resources());
    // A preview owns the existing single-work lease until it is dropped.
    // Sequential export must not overlap that completed user operation.
    drop(preview);
    drop(restored);
    let exported = export_stored(fixture, stored).await;
    for index in 0..2 {
        assert_eq!(
            file_records(&exported, support::name(&exported, identity(index))),
            expected.records[index]
        );
    }
    assert_retained_storage(fixture, flow).await;
}

pub(super) async fn head_unchanged(fixture: &Fixture, saved: &SavedEditorialTeletextDesign) {
    assert_eq!(
        fixture
            .state
            .application
            .teletext_design_head(saved.head().id)
            .await
            .unwrap(),
        Some(saved.head().clone())
    );
    assert!(fixture
        .state
        .application
        .teletext_design_revision(
            saved.head().id,
            saved.head().revision + 1,
            designs::budget().max_serialized_bytes
        )
        .await
        .unwrap()
        .is_none());
}

pub(super) async fn published(
    fixture: &Fixture,
    day: &AcceptedTeletextOutputCandidate,
    night: &AcceptedTeletextOutputCandidate,
) {
    for (channel, expected) in [(1, day), (2, night)] {
        assert_eq!(
            fixture
                .state
                .application
                .teletext_publication_request(channel)
                .await
                .unwrap(),
            Some(expected.generation().request.clone())
        );
        assert_eq!(
            fixture
                .state
                .application
                .accepted_teletext_generation(channel)
                .await
                .unwrap(),
            Some(expected.generation().clone())
        );
        generations::assert_same_output_width(&recover(fixture, channel).await, expected, 40 * 12);
    }
}

pub(super) async fn publish_after(
    fixture: &Fixture,
    saved: &SavedEditorialTeletextDesign,
    previous: &AcceptedTeletextOutputCandidate,
) -> AcceptedTeletextOutputCandidate {
    use ersatzrs_core::repositories::TeletextPublicationMutation;

    let pending =
        ersatzrs_web::host::teletext_designs::request_original_teletext_design_publication(
            &fixture.state.application,
            saved.head().id,
            saved.head().revision,
            saved.digest().to_owned(),
            saved.head().revision,
            Some(previous.generation().request.id),
            designs::budget(),
            channel_documents::policy(),
            designs::deadline(),
        )
        .await
        .unwrap();
    let TeletextPublicationMutation::Requested(pending) = pending else {
        panic!(
            "restored concept must request publication against the current accepted predecessor"
        );
    };
    assert_eq!(
        fixture
            .state
            .application
            .accepted_teletext_generation(1)
            .await
            .unwrap(),
        Some(previous.generation().clone()),
        "requesting revision5 must leave accepted revision4 unchanged"
    );
    generations::assert_same_output_width(&recover(fixture, 1).await, previous, 40 * 12);
    generations::accepted(
        accept_original_teletext_generation(
            &fixture.state.application,
            TeletextGenerationTarget {
                request: pending.request().clone(),
                expected_design_head: saved.head().revision,
                expected_generation_id: Some(previous.generation().id),
            },
            designs::budget(),
            channel_documents::policy(),
            designs::deadline(),
        )
        .await
        .unwrap(),
    )
}
