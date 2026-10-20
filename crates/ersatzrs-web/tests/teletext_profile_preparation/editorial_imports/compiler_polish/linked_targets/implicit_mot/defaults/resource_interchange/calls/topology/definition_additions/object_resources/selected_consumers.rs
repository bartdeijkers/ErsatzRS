//! A real editor alias cannot authorize an unannounced selected-bank S1.
use super::*;
#[path = "selected_consumers/admin.rs"]
mod admin;
#[path = "selected_consumers/announcements.rs"]
mod announcements;
#[path = "selected_consumers/page_selection.rs"]
mod page_selection;
use ersatzrs_domain::{
    TeletextEnhancementSource, TeletextProducerCompositionFailure, TeletextServiceError,
    TeletextX27LinkRole,
};
use ersatzrs_web::host::{
    teletext_channel_documents::ChannelTeletextDocumentEncodingError,
    teletext_designs::NativeTeletextDesignError,
};

fn resources() -> [TeletextResourceIdentity; 3] {
    let mut resources = bank_resources();
    resources[1] = TeletextResourceIdentity::new(
        TeletextResourceKind::Pop,
        seed::target(21).resource().unwrap().page(),
        TeletextResourceSubpageIndex::new(15).unwrap(),
    )
    .unwrap();
    [resources[0], resources[1], resources[2]]
}
fn target() -> TeletextObjectIdentity {
    TeletextObjectIdentity::public(resources()[1], TeletextObjectId::new(30)).unwrap()
}
fn dormant(theme: &str, page: u16, subcode: u16) -> Value {
    let mut value = complete_response(theme, page, subcode);
    let old = wire_target(new_target(1));
    let new = wire_target(target());
    for object in value["service"]["objects"].as_array_mut().unwrap() {
        if object["identity"] == old {
            object["identity"] = new.clone();
            object["program"]["commands"][4]["operation"]["code"] = json!(81);
        }
        for command in object["program"]["commands"].as_array_mut().unwrap() {
            if command["target"] == old {
                command["target"] = new.clone();
            }
        }
    }
    value
}
fn encoding() -> TeletextChannelDocumentEncodingPolicy {
    let mut encoding = seed::encoding();
    encoding
        .transmissions
        .extend(resources().into_iter().map(lifecycle));
    encoding
}
async fn qualify_dormant() -> BankExpected {
    let (fixture, flow) = Box::pin(setup_with_fixture_policy_and_writer(
        dormant,
        Fixture::new_persistent().await,
        encoding,
        authoring_metadata::write_compact,
    ))
    .await;
    let expected = bank_expected(flow.accepted[0].service(), &resources());
    fixture.state.pool.close().await;
    expected
}
fn unannounced_call(
    exported: &ExportedOriginalTeletextDesign,
    receipt: &TeletextDesignExportReceipt,
    head: i64,
) -> NativeTeletextEditorialImportRequest {
    let mut request = drcs_bitmap::from_receipt(receipt, head);
    let originals = request.files.clone();
    let name = companion(exported);
    let file = request
        .files
        .iter_mut()
        .find(|file| file.name == name)
        .unwrap();
    let mut records = parse_teletext_tti(&file.bytes, policy().tti)
        .unwrap()
        .records()
        .to_vec();
    let markers = records
        .iter()
        .filter_map(|record| match record {
            TeletextTtiRecord::Packet { row: 26, words, .. } => Some(words.as_slice()),
            _ => None,
        })
        .flatten()
        .copied()
        .filter(|word| word & 63 >= 40 && (21..=23).contains(&((word >> 6) & 31)))
        .collect::<Vec<_>>();
    // The exact fresh catalogue has Active20 first, then Adaptive22, Passive21,
    // new Global31, new Public30, old Add25 and old Add26. A checked definition
    // marker supplies the real Local editor alias; no arbitrary native pointer.
    assert_eq!(markers.len(), 7);
    assert_eq!((markers[4] >> 6) & 31, 23);
    let replacement = markers[4] - (4 << 6);
    assert_ne!(word(&records, 8), replacement);
    for record in &mut records {
        if let TeletextTtiRecord::Packet {
            row: 26,
            designation: 0,
            words,
        } = record
        {
            words[8] = replacement;
        }
    }
    file.bytes = write_teletext_tti(
        &TeletextTtiDocument::checked_from_records(records, policy().tti).unwrap(),
        policy().tti,
    )
    .unwrap();
    for (before, after) in originals.iter().zip(&request.files) {
        assert_eq!(before.name, after.name);
        if before.name != name {
            assert_eq!(before.bytes, after.bytes);
        }
    }
    request
}
#[tokio::test]
async fn selected_new_bank_unannounced_s1_refuses_without_revision_publication_or_recovery_damage()
{
    let before = Box::pin(qualify_dormant()).await;
    let (mut fixture, flow, concept) =
        Box::pin(retained_definitions::setup_retained(&before)).await;
    let retained = resource_bank_admin::Retained::capture(&fixture).await;
    Box::pin(assert_bank_concept(&fixture, &flow, &concept, &before)).await;
    let exported = drcs_bitmap::reexport(&fixture, &concept).await;
    let current = receipt(&fixture, &exported).await;
    let result = import_teletext_editorial_design(
        &fixture.state.application,
        unannounced_call(&exported, &current, 3),
        policy(),
        designs::deadline(),
    )
    .await;
    assert_unannounced_error(result.unwrap_err(), 124);
    assert_head_only(&fixture, &concept).await;
    assert_original_pointers(&fixture, &flow).await;
    assert_eq!(
        qtm_carrier::stored_receipt(&fixture, current.metadata.id).await,
        current
    );
    fixture = fixture.reopen().await;
    Box::pin(assert_bank_concept(&fixture, &flow, &concept, &before)).await;
    assert_original_pointers(&fixture, &flow).await;
    retained.assert_unchanged(&fixture, &flow).await;
    assert!(fixture.playout.try_recv().is_err());
    fixture.state.pool.close().await;
}

fn assert_unannounced_error(error: NativeTeletextEditorialImportError, display: u16) {
    let NativeTeletextEditorialImportError::Design(error) = error else {
        panic!("a valid sealed editor alias must reach native declaration validation")
    };
    let NativeTeletextDesignError::Encoding(error) = *error else {
        panic!("retarget must fail the complete native design encoding")
    };
    let ChannelTeletextDocumentEncodingError::Composition(error) = *error else {
        panic!("existing resource use must fail composition before physical encoding")
    };
    let TeletextProducerCompositionFailure::Semantic(error) = error.reason else {
        panic!("implicit MOT use must produce a located semantic failure")
    };
    assert_eq!(
        *error,
        TeletextServiceError::CompositionalUsage {
            display: channel_documents::address(display, 32),
            role: TeletextX27LinkRole::Pop,
            page: resources()[1].page(),
            subpage: resources()[1].logical_subpage(),
            source_location: TeletextEnhancementSource {
                object: Some(seed::target(20)),
                command: 4
            },
        }
    );
}
