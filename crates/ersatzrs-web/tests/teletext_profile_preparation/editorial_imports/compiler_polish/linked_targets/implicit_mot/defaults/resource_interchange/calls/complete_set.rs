//! EE3: two existing editor actions share one complete retained resource graph.
use super::*;

#[path = "complete_set/admin.rs"]
mod admin;
#[path = "complete_set/lifecycle.rs"]
mod lifecycle;

fn retained(mut value: Value) -> Value {
    let objects = value["service"]["objects"].as_array_mut().unwrap();
    let mut dormant = objects
        .iter()
        .find(|object| object["identity"]["object_id"] == 21)
        .unwrap()
        .clone();
    dormant["identity"]["object_id"] = json!(23);
    objects.push(dormant);
    value
}

fn original_complete(theme: &str, page: u16, subcode: u16) -> Value {
    retained(original_response(theme, page, subcode))
}

fn combined(theme: &str, page: u16, subcode: u16) -> Value {
    let mut value = retained(translated_response(theme, page, subcode));
    let passive = value["service"]["objects"]
        .as_array_mut()
        .unwrap()
        .iter_mut()
        .find(|object| object["identity"]["object_id"] == 21)
        .unwrap();
    assert_eq!(passive["program"]["commands"][4]["operation"]["code"], 80);
    passive["program"]["commands"][4]["operation"]["code"] = json!(81);
    value
}

fn passive_name(flow: &Flow) -> &str {
    flow.exported[0]
        .sidecar()
        .project
        .assets
        .iter()
        .find(|asset| {
            matches!(
                asset.scope,
                TeletextExportAssetScope::ObjectContextTtiV1 { object_id: 21, .. }
            )
        })
        .unwrap()
        .name
        .as_str()
}

fn edit_word(
    file: &mut ersatzrs_app::teletext_export_project::TeletextExportFile,
    before: u32,
    after: u32,
) {
    let mut records = parse_teletext_tti(&file.bytes, policy().tti)
        .unwrap()
        .records()
        .to_vec();
    let words = records
        .iter_mut()
        .find_map(|record| match record {
            TeletextTtiRecord::Packet {
                row: 26,
                designation: 0,
                words,
            } => Some(words),
            _ => None,
        })
        .unwrap();
    assert_eq!(words[7], before);
    words[7] = after;
    file.bytes = write_teletext_tti(
        &TeletextTtiDocument::checked_from_records(records, policy().tti).unwrap(),
        policy().tti,
    )
    .unwrap();
}

fn complete_request(flow: &Flow) -> NativeTeletextEditorialImportRequest {
    let mut request = drcs_bitmap::from_receipt(&flow.receipts[0], 1);
    let call_name = companion(&flow.exported[0]);
    let glyph_name = passive_name(flow);
    assert_ne!(call_name, glyph_name);
    edit_word(
        request
            .files
            .iter_mut()
            .find(|file| file.name == call_name)
            .unwrap(),
        0x428,
        0xc28,
    );
    edit_word(
        request
            .files
            .iter_mut()
            .find(|file| file.name == glyph_name)
            .unwrap(),
        0x28241,
        0x28a41,
    );
    assert_eq!(
        request.sidecar,
        write_teletext_export_sidecar(flow.exported[0].sidecar(), policy().project).unwrap()
    );
    for original in flow.exported[0].files() {
        let changed = request
            .files
            .iter()
            .find(|file| file.name == original.name)
            .unwrap();
        assert_eq!(changed.name, original.name);
        assert_eq!(
            changed.bytes != original.bytes,
            original.name == call_name || original.name == glyph_name
        );
    }
    request
}

fn exact_service(actual: &EncodedTeletextAuthoredService, wanted: &EncodedTeletextAuthoredService) {
    assert_eq!(actual.declared(), wanted.declared());
    assert_eq!(actual.magazine_units(), wanted.magazine_units());
    assert_eq!(designs::units(actual), designs::units(wanted));
    for (actual, wanted) in actual.pages().iter().zip(wanted.pages()) {
        match (actual, wanted) {
            (
                EncodedTeletextAuthoredPage::Object(actual),
                EncodedTeletextAuthoredPage::Object(wanted),
            ) => {
                assert_eq!(actual, wanted);
            }
            (
                EncodedTeletextAuthoredPage::CompiledDisplay(actual),
                EncodedTeletextAuthoredPage::CompiledDisplay(wanted),
            ) => {
                assert_eq!(
                    actual.normalized().encoded().command_positions(),
                    wanted.normalized().encoded().command_positions()
                );
                assert_eq!(
                    actual.normalized().encoded().definition_positions(),
                    wanted.normalized().encoded().definition_positions()
                );
                assert_eq!(
                    actual.normalized().original().original_program_source(),
                    wanted.normalized().original().original_program_source()
                );
            }
            _ => {}
        }
    }
    for page in 124..127 {
        assert_eq!(
            external_editor_probe::preview_page(actual, page),
            external_editor_probe::preview_page(wanted, page)
        );
    }
}

fn catalogue_and_fallback(flow: &Flow, target: &Flow) {
    assert_eq!(
        flow.accepted[0]
            .service()
            .declared()
            .semantic()
            .objects()
            .definitions()
            .len(),
        4
    );
    let objects = flow.accepted[0].service().declared().semantic().objects();
    let dormant = objects.definition(seed::target(23)).unwrap();
    assert_eq!(
        Some(dormant),
        target.accepted[0]
            .service()
            .declared()
            .semantic()
            .objects()
            .definition(seed::target(23))
    );
    for definition in objects.definitions() {
        for command in definition.program.commands() {
            assert!(
                !matches!(command,TeletextEnhancementCommand::InvokeObject {target,..} if *target==dormant.identity)
            );
        }
    }
    for page in flow.accepted[0]
        .service()
        .declared()
        .semantic()
        .page_enhancements()
        .values()
    {
        assert_eq!(page.resources.defaults, seed::defaults());
    }
    assert_eq!(
        flow.accepted[0].service().declared().semantic().base(),
        target.accepted[0].service().declared().semantic().base()
    );
    for (original, wanted) in flow.accepted[0]
        .service()
        .pages()
        .iter()
        .zip(target.accepted[0].service().pages())
    {
        if matches!(original, EncodedTeletextAuthoredPage::CompiledDisplay(_)) {
            assert_eq!(
                original.data_units(),
                wanted.data_units(),
                "every complete display packet, including Base/fallback, stays exact"
            );
        }
    }
}

#[tokio::test]
async fn complete_resource_set_combines_two_context_documents_atomically() {
    let (target_fixture, target) = Box::pin(setup_with_fixture_policy_and_writer(
        combined,
        Fixture::new_persistent().await,
        seed::encoding,
        authoring_metadata::write_compact,
    ))
    .await;
    let (fixture, flow) = Box::pin(setup_with_fixture_policy_and_writer(
        original_complete,
        Fixture::new_persistent().await,
        seed::encoding,
        authoring_metadata::write_compact,
    ))
    .await;
    catalogue_and_fallback(&flow, &target);
    let retained = resource_bank_admin::Retained::capture(&fixture).await;
    let request = complete_request(&flow);
    let mut invalid = complete_request(&flow);
    invalid
        .files
        .iter_mut()
        .find(|file| file.name == passive_name(&flow))
        .unwrap()
        .bytes
        .push(0);
    assert!(matches!(
        import_teletext_editorial_design(
            &fixture.state.application,
            invalid,
            policy(),
            designs::deadline()
        )
        .await
        .unwrap_err(),
        NativeTeletextEditorialImportError::Tti(_)
    ));
    assert_eq!(
        fixture
            .state
            .application
            .teletext_design_head(flow.saved[0].head().id)
            .await
            .unwrap(),
        Some(flow.saved[0].head().clone())
    );
    assert_original_pointers(&fixture, &flow).await;
    retained.assert_unchanged(&fixture, &flow).await;
    let concept = import(&fixture, request).await;
    let restored = drcs_bitmap::restored(&fixture, &concept).await;
    exact_service(restored.encoded().unwrap(), target.accepted[0].service());
    assert_original_pointers(&fixture, &flow).await;
    retained.assert_unchanged(&fixture, &flow).await;
    Box::pin(lifecycle::finish(fixture, &flow, &concept, &target)).await;
    target_fixture.state.pool.close().await;
}
