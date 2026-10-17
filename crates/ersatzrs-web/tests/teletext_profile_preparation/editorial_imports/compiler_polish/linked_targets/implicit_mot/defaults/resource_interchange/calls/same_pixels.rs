//! Equal pixels do not give a changed invocation its original writer authority.
use super::*;

fn implicit_origin_response(theme: &str, page: u16, subcode: u16) -> Value {
    let mut value = original_response(theme, page, subcode);
    let active = value["service"]["objects"]
        .as_array_mut()
        .unwrap()
        .iter_mut()
        .find(|o| o["identity"]["object_id"] == 20)
        .unwrap();
    assert_eq!(
        active["program"]["commands"][4]["offset"],
        json!({"row":0,"column":0})
    );
    active["program"]["commands"][4]["offset"] = Value::Null;
    value
}

fn generated_response(theme: &str, page: u16, subcode: u16) -> Value {
    let mut value = original_response(theme, page, subcode);
    let protected = value["authoring"]["pages"]
        .as_array_mut()
        .unwrap()
        .iter_mut()
        .find(|p| p["page"]["page"] == 125)
        .unwrap();
    let region = protected["regions"]
        .as_array_mut()
        .unwrap()
        .iter_mut()
        .find(|r| r["id"] == "nested-call-zone")
        .unwrap();
    region["kind"] = json!("generated");
    value
}

async fn qualified_target_without_origin() -> (Vec<TeletextTtiRecord>, Vec<TeletextRaster>) {
    let (fixture, flow) = Box::pin(setup_with_fixture_policy_and_writer(
        implicit_origin_response,
        Fixture::new_persistent().await,
        seed::encoding,
        authoring_metadata::write_compact,
    ))
    .await;
    let service = flow.accepted[0].service();
    let definition = service
        .declared()
        .semantic()
        .objects()
        .definition(seed::target(20))
        .unwrap();
    assert_eq!(definition.program.commands().len(), 5);
    assert_eq!(
        definition.program.commands()[4],
        TeletextEnhancementCommand::InvokeObject {
            target: seed::target(21),
            object_type: TeletextObjectType::Passive,
            offset: None,
        }
    );
    let file = flow.exported[0]
        .files()
        .iter()
        .find(|f| f.name == companion(&flow.exported[0]))
        .unwrap();
    let records = parse_teletext_tti(&file.bytes, policy().tti)
        .unwrap()
        .records()
        .to_vec();
    let pixels = (124..127)
        .map(|p| external_editor_probe::preview_page(service, p))
        .collect();
    fixture.state.pool.close().await;
    (records, pixels)
}

fn without_origin_request(
    flow: &Flow,
    target: &[TeletextTtiRecord],
) -> NativeTeletextEditorialImportRequest {
    let mut request = drcs_bitmap::from_receipt(&flow.receipts[0], 1);
    let originals = request.files.clone();
    let sidecar = request.sidecar.clone();
    let name = companion(&flow.exported[0]);
    let file = request.files.iter_mut().find(|f| f.name == name).unwrap();
    let mut records = parse_teletext_tti(&file.bytes, policy().tti)
        .unwrap()
        .records()
        .to_vec();
    let words = records
        .iter_mut()
        .find_map(|r| match r {
            TeletextTtiRecord::Packet {
                row: 26,
                designation: 0,
                words,
            } => Some(words),
            _ => None,
        })
        .unwrap();
    assert_eq!(
        words[7],
        0x28 | (16 << 6),
        "original explicit zero OriginModifier"
    );
    assert_eq!(
        (words[8] >> 6) & 31,
        19,
        "original Passive invocation remains exact"
    );
    if flow.exported[0].sidecar().project.editor_profile == "qteletextmaker-1.0-carrier-v8" {
        // The separately compiled target also relocates every protected Local
        // definition/pointer after removing the origin word. Native context
        // reconciliation authenticates these derived moves before effects.
        let target_word = target
            .iter()
            .find_map(|r| match r {
                TeletextTtiRecord::Packet {
                    row: 26,
                    designation: 0,
                    words,
                } => Some(words[7]),
                _ => None,
            })
            .unwrap();
        assert_eq!(
            (target_word >> 6) & 31,
            19,
            "the original Passive call follows directly"
        );
        records = target.to_vec();
    } else {
        let final_termination = 63 | (31 << 6) | (5 << 11);
        assert_eq!(words[12], final_termination);
        let mut shortened = words.to_vec();
        assert_eq!(shortened.remove(7), 0x28 | (16 << 6));
        shortened.push(final_termination);
        *words = shortened.try_into().unwrap();
        assert_eq!(records,target,"only omission of the original explicit zero offset matches the independently compiled target");
    }
    file.bytes = write_teletext_tti(
        &TeletextTtiDocument::checked_from_records(records, policy().tti).unwrap(),
        policy().tti,
    )
    .unwrap();
    assert_eq!(request.sidecar, sidecar);
    for (before, after) in originals.iter().zip(&request.files) {
        assert_eq!(before.name, after.name);
        if before.name != name {
            assert_eq!(before.bytes, after.bytes);
        }
    }
    request
}

#[tokio::test]
async fn equal_pixel_resource_call_change_cannot_retain_generated_writer_authority() {
    let (target, pixels) = Box::pin(qualified_target_without_origin()).await;
    let (mut fixture, flow) = Box::pin(setup_with_fixture_policy_and_writer(
        generated_response,
        Fixture::new_persistent().await,
        seed::encoding,
        authoring_metadata::write_compact,
    ))
    .await;
    let retained = resource_bank_admin::Retained::capture(&fixture).await;
    assert_eq!(
        translated_pixels(flow.accepted[0].service(), 0),
        pixels,
        "Some(offset0) and None have identical complete rendered output on all three displays"
    );
    let error = import_teletext_editorial_design(
        &fixture.state.application,
        without_origin_request(&flow, &target),
        policy(),
        designs::deadline(),
    )
    .await
    .unwrap_err();
    let NativeTeletextEditorialImportError::Effects(error) = error else {
        panic!("same pixels cannot bypass changed invocation lineage: {error:?}");
    };
    assert_eq!(
        error.reason,
        TeletextEditorialEffectsFailure::ProtectedDisplay
    );
    assert_eq!(error.page, Some(channel_documents::address(125, 32)));
    assert_eq!(error.position, Some((5, 15)));
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
    assert!(fixture
        .state
        .application
        .teletext_design_revision(
            flow.saved[0].head().id,
            2,
            designs::budget().max_serialized_bytes,
        )
        .await
        .unwrap()
        .is_none());
    assert_original_pointers(&fixture, &flow).await;
    retained.assert_unchanged(&fixture, &flow).await;
    assert!(fixture.playout.try_recv().is_err());
    fixture.state.pool.close().await;
}
