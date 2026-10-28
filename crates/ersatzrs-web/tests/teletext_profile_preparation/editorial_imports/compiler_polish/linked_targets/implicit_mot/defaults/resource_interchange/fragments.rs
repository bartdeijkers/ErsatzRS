//! Full-size original POP through bounded, command-aligned editor members.
use super::*;
use ersatzrs_app::teletext_export_project::{TeletextExportAssetMapping, TeletextExportProject};

pub(super) const FOREGROUND_COMMAND: usize = 352;

pub(super) fn response(theme: &str, page: u16, subcode: u16) -> Value {
    let mut value = super::response(theme, page, subcode);
    let passive = value["service"]["objects"]
        .as_array_mut()
        .unwrap()
        .iter_mut()
        .find(|object| object["identity"]["object_id"] == 21)
        .unwrap();
    let original = passive["program"]["commands"].as_array().unwrap();
    assert_eq!(original.len(), 5);
    let mut commands = vec![json!({"kind":"set_position","position":{"row":4,"column":1}}); 350];
    commands.extend_from_slice(original);
    assert_eq!(
        commands[FOREGROUND_COMMAND],
        json!({"kind":"at_column","column":1,"operation":{"kind":"foreground","colour":2}})
    );
    passive["program"]["commands"] = json!(commands);
    let bytes = serde_json::to_vec(&value).unwrap();
    assert!(
        bytes.len() <= encoding().input.max_response_bytes,
        "large original response {} exceeds explicit fixture budget",
        bytes.len()
    );
    value
}

fn complete_catalogue_response(theme: &str, page: u16, subcode: u16) -> Value {
    let mut value = response(theme, page, subcode);
    // Keep the original large fixture available to its other callers. Add a
    // small unused definition to this full-catalogue lifecycle, within the
    // same native bank's capacity, without changing any display consumer.
    let original = super::response(theme, page, subcode);
    let mut dormant = original["service"]["objects"]
        .as_array()
        .unwrap()
        .iter()
        .find(|object| object["identity"]["object_id"] == 21)
        .unwrap()
        .clone();
    dormant["identity"]["object_id"] = json!(23);
    value["service"]["objects"]
        .as_array_mut()
        .unwrap()
        .push(dormant);
    value
}

pub(super) fn encoding(
) -> ersatzrs_web::host::teletext_channel_documents::TeletextChannelDocumentEncodingPolicy {
    let mut policy = seed::encoding();
    policy.input.max_response_bytes = 65_536;
    policy
}

pub(super) fn selected_asset(project: &TeletextExportProject) -> &TeletextExportAssetMapping {
    assert_eq!(project.editor_profile, "qteletextmaker-1.0-carrier-v7");
    let resource = project.assets.iter().find(|asset| matches!(&asset.scope,
        TeletextExportAssetScope::Resource {identity, ..}
        if identity.kind == TeletextResourceKindWire::Pop && identity.page.magazine == 2 && identity.page.page == 0xc0 && identity.subpage == 0)).unwrap();
    let fragments: Vec<_> = project
        .assets
        .iter()
        .filter(|asset| {
            matches!(&asset.scope,
        TeletextExportAssetScope::ObjectTtiFragmentV1 {resource_name,object_id:21,..}
        if resource_name == &resource.name)
        })
        .collect();
    assert_eq!(fragments.len(), 2);
    let asset = fragments
        .into_iter()
        .find(|asset| {
            matches!(asset.scope,
        TeletextExportAssetScope::ObjectTtiFragmentV1 {command_start,command_end,..}
        if command_start <= 352 && 352 < command_end)
        })
        .unwrap();
    assert!(matches!(
        asset.scope,
        TeletextExportAssetScope::ObjectTtiFragmentV1 {
            fragment_index: 1,
            fragment_count: 2,
            editor_page: 256,
            editor_ordinal: 0,
            ..
        }
    ));
    asset
}

pub(super) fn request(
    receipt: &TeletextDesignExportReceipt,
    head: i64,
    from: u8,
    to: u8,
) -> NativeTeletextEditorialImportRequest {
    let body = parse_teletext_export_receipt_body(&receipt.receipt, policy().project).unwrap();
    let asset = selected_asset(&body.project);
    let mut request = drcs_bitmap::from_receipt(receipt, head);
    "Original full-size POP fragment foreground".clone_into(&mut request.name);
    let file = request
        .files
        .iter_mut()
        .find(|file| file.name == asset.name)
        .unwrap();
    file.bytes = edit_foreground_file(&file.bytes, from, to);
    request
}

async fn assert_unpublished(fixture: &Fixture, flow: &Flow) {
    assert_original_pointers(fixture, flow).await;
    assert_eq!(
        fixture
            .state
            .application
            .teletext_design_head(flow.saved[0].head().id)
            .await
            .unwrap()
            .unwrap(),
        *flow.saved[0].head()
    );
}

fn assert_changed_resource_reexport(
    flow: &Flow,
    exported: &ExportedOriginalTeletextDesign,
    service: &ersatzrs_ffmpeg::teletext::EncodedTeletextAuthoredService,
) {
    let mut changed = 0;
    for asset in &exported.sidecar().project.assets {
        let TeletextExportAssetScope::Resource {
            identity,
            physical_subcode,
        } = &asset.scope
        else {
            continue;
        };
        let page = service
            .pages()
            .iter()
            .find(|page| {
                page.resource_header().is_some_and(|header| {
                    let native = header.identity();
                    serde_json::to_value(native.kind()).unwrap()
                        == serde_json::to_value(identity.kind).unwrap()
                        && native.page().magazine() == identity.page.magazine
                        && native.page().page() == identity.page.page
                        && native.logical_subpage().get() == identity.subpage
                        && header.physical_subcode() == *physical_subcode
                })
            })
            .unwrap();
        let file = exported
            .files()
            .iter()
            .find(|file| file.name == asset.name)
            .unwrap();
        let canonical: Vec<u8> = page.data_units().iter().flatten().copied().collect();
        assert_eq!(
            file.bytes, canonical,
            "raw export must exactly match accepted native resource units"
        );
        let original_asset = flow.exported[0]
            .sidecar()
            .project
            .assets
            .iter()
            .find(|original| original.scope == asset.scope)
            .unwrap();
        let original = flow.exported[0]
            .files()
            .iter()
            .find(|file| file.name == original_asset.name)
            .unwrap();
        if page.resource_header().unwrap().identity() == seed::target(21).resource().unwrap() {
            assert_ne!(file.bytes, original.bytes);
            changed += 1;
        } else {
            assert_eq!(
                file.bytes, original.bytes,
                "every unedited resource, including MOT, remains exact"
            );
        }
    }
    assert_eq!(changed, 1);
}

async fn assert_missing_fragment_is_atomic(fixture: &Fixture, flow: &Flow) {
    let selected = selected_asset(&flow.exported[0].sidecar().project);
    let mut missing = request(&flow.receipts[0], 1, 2, 3);
    let other = flow.exported[0]
        .sidecar()
        .project
        .assets
        .iter()
        .find(|asset| {
            matches!(
                &asset.scope,
                TeletextExportAssetScope::ObjectTtiFragmentV1 {
                    object_id: 21,
                    fragment_index: 0,
                    ..
                }
            )
        })
        .unwrap();
    assert_ne!(other.name, selected.name);
    missing.files.retain(|file| file.name != other.name);
    assert!(import_teletext_editorial_design(
        &fixture.state.application,
        missing,
        policy(),
        designs::deadline()
    )
    .await
    .is_err());
    assert_unpublished(fixture, flow).await;
}

#[tokio::test]
async fn original_pop_fragments_import_publish_repeat_undo_reopen_preserves_all_consumers() {
    let (mut fixture, flow) = Box::pin(setup_with_fixture_policy_and_writer(
        complete_catalogue_response,
        Fixture::new_persistent().await,
        encoding,
        authoring_metadata::write_compact,
    ))
    .await;
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
    let retained = resource_bank_admin::Retained::capture(&fixture).await;
    assert_missing_fragment_is_atomic(&fixture, &flow).await;
    let concept = import(&fixture, request(&flow.receipts[0], 1, 2, 3)).await;
    assert_eq!(concept.head().revision, 2);
    let previews = Box::pin(assert_resource_concept_at(
        &fixture,
        &flow,
        &concept,
        3,
        FOREGROUND_COMMAND,
    ))
    .await;
    assert_original_pointers(&fixture, &flow).await;
    retained.assert_unchanged(&fixture, &flow).await;
    fixture = fixture.reopen().await;
    assert_eq!(
        Box::pin(assert_resource_concept_at(
            &fixture,
            &flow,
            &concept,
            3,
            FOREGROUND_COMMAND
        ))
        .await,
        previews
    );
    let day = publish_and_accept(&fixture, &flow, &concept).await;
    assert_eq!(
        (124..127)
            .map(|page| external_editor_probe::preview_page(day.service(), page))
            .collect::<Vec<_>>(),
        previews
    );
    let exported = drcs_bitmap::reexport(&fixture, &concept).await;
    assert_changed_resource_reexport(&flow, &exported, day.service());
    let latest = receipt(&fixture, &exported).await;
    let undone = import(&fixture, request(&latest, 2, 3, 2)).await;
    assert_eq!(undone.head().revision, 3);
    Box::pin(assert_resource_concept_at(
        &fixture,
        &flow,
        &undone,
        2,
        FOREGROUND_COMMAND,
    ))
    .await;
    let exported = drcs_bitmap::reexport(&fixture, &undone).await;
    assert_protected_reexport(&flow, &exported);
    let latest = receipt(&fixture, &exported).await;
    let repeated = import(&fixture, request(&latest, 3, 2, 3)).await;
    assert_eq!(repeated.head().revision, 4);
    fixture = fixture.reopen().await;
    assert_eq!(
        Box::pin(assert_resource_concept_at(
            &fixture,
            &flow,
            &repeated,
            3,
            FOREGROUND_COMMAND
        ))
        .await,
        previews
    );
    generations::assert_same_output(&recover(&fixture, 1).await, &day);
    generations::assert_same_output(&recover(&fixture, 2).await, &flow.accepted[1]);
    assert_pointers(&fixture, &day, &flow.accepted[1]).await;
    retained.assert_unchanged(&fixture, &flow).await;
    assert!(fixture.playout.try_recv().is_err());
    fixture.state.pool.close().await;
}

fn generated_response(theme: &str, page: u16, subcode: u16) -> Value {
    let mut value = response(theme, page, subcode);
    let page = value["authoring"]["pages"]
        .as_array_mut()
        .unwrap()
        .iter_mut()
        .find(|page| page["page"]["page"] == 125)
        .unwrap();
    page["regions"]
        .as_array_mut()
        .unwrap()
        .iter_mut()
        .find(|region| region["id"] == "guide-content")
        .unwrap()["kind"] = json!("generated");
    value
}

#[tokio::test]
async fn original_pop_fragment_shared_generated_consumer_is_rejected_without_publication() {
    let (mut fixture, flow) = Box::pin(setup_with_fixture_policy_and_writer(
        generated_response,
        Fixture::new_persistent().await,
        encoding,
        authoring_metadata::write_compact,
    ))
    .await;
    let retained = resource_bank_admin::Retained::capture(&fixture).await;
    let error = import_teletext_editorial_design(
        &fixture.state.application,
        request(&flow.receipts[0], 1, 2, 3),
        policy(),
        designs::deadline(),
    )
    .await
    .unwrap_err();
    let NativeTeletextEditorialImportError::Effects(error) = error else {
        panic!("shared generated consumer must remain protected")
    };
    assert_eq!(
        error.reason,
        TeletextEditorialEffectsFailure::ProtectedDisplay
    );
    assert_eq!(error.page, Some(channel_documents::address(125, 32)));
    assert_eq!(error.position, Some((4, 5)));
    assert_unpublished(&fixture, &flow).await;
    retained.assert_unchanged(&fixture, &flow).await;
    assert!(fixture.playout.try_recv().is_err());
    fixture.state.pool.close().await;
}
