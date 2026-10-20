//! Explicit announcement joins new banks to protected, ordinary concept editing.
use super::*;
use ersatzrs_app::teletext_design_documents::TeletextSavedObjectSubpageAnnouncement;
use ersatzrs_domain::TeletextObjectSubpageAnnouncement;
use ersatzrs_web::host::teletext_editorial_designs::{
    announce_teletext_editorial_object_subpages, NativeTeletextObjectSubpagesAnnounceRequest,
};
fn authored(theme: &str, page: u16, subcode: u16, first_only: bool, called: bool) -> Value {
    let mut value = dormant(theme, page, subcode);
    for metadata in value["service"]["page_enhancements"]
        .as_array_mut()
        .unwrap()
    {
        if first_only && metadata["address"]["page"] != 124 {
            continue;
        }
        assert!(metadata["compositional_links"].is_null());
        let link = |page: &Value, mask: u16| {
            assert!(!page.is_null());
            json!({"target":{"kind":"page","page":page},"validity":"level25_only","required_subpages":mask})
        };
        // Independent complete-source oracle: all four original selected owners
        // have MOT extent0. Only the POP requirement gains the explicit S1=15.
        metadata["compositional_links"] = json!({
            "gpop":link(&metadata["resources"]["objects"]["global"],1),
            "pop":link(&metadata["resources"]["objects"]["public"],0x8001),
            "gdrcs":link(&metadata["resources"]["drcs"]["global"],1),
            "drcs":link(&metadata["resources"]["drcs"]["normal"],1),
        });
    }
    if called {
        let object = value["service"]["objects"]
            .as_array_mut()
            .unwrap()
            .iter_mut()
            .find(|object| object["identity"]["object_id"] == 20)
            .unwrap();
        object["program"]["commands"][4]["target"] = wire_target(target());
    }
    value
}
fn first(theme: &str, page: u16, subcode: u16) -> Value {
    authored(theme, page, subcode, true, false)
}
fn all(theme: &str, page: u16, subcode: u16) -> Value {
    authored(theme, page, subcode, false, false)
}
fn used(theme: &str, page: u16, subcode: u16) -> Value {
    authored(theme, page, subcode, false, true)
}
async fn qualified(factory: fn(&str, u16, u16) -> Value) -> BankExpected {
    let (fixture, flow) = Box::pin(setup_with_fixture_policy_and_writer(
        factory,
        Fixture::new_persistent().await,
        encoding,
        authoring_metadata::write_compact,
    ))
    .await;
    let expected = bank_expected(flow.accepted[0].service(), &resources());
    fixture.state.pool.close().await;
    expected
}
fn announcements(pages: &[u16]) -> Vec<TeletextSavedObjectSubpageAnnouncement> {
    pages
        .iter()
        .map(|page| {
            TeletextSavedObjectSubpageAnnouncement::new(
                0,
                TeletextObjectSubpageAnnouncement::new(
                    channel_documents::address(*page, 32),
                    resources()[1],
                )
                .unwrap(),
            )
            .unwrap()
        })
        .collect()
}
fn request(
    receipt: &TeletextDesignExportReceipt,
    head: i64,
    pages: &[u16],
) -> NativeTeletextObjectSubpagesAnnounceRequest {
    NativeTeletextObjectSubpagesAnnounceRequest {
        name: "Explicit selected object subpage".into(),
        receipt_id: receipt.metadata.id,
        receipt_digest: receipt.metadata.digest.clone(),
        expected_design_head: head,
        announcements: announcements(pages),
    }
}
async fn announce(
    fixture: &Fixture,
    concept: &SavedEditorialTeletextDesign,
    pages: &[u16],
) -> SavedEditorialTeletextDesign {
    let exported = drcs_bitmap::reexport(fixture, concept).await;
    let current = receipt(fixture, &exported).await;
    let result = announce_teletext_editorial_object_subpages(
        &fixture.state.application,
        request(&current, concept.head().revision, pages),
        policy(),
        designs::deadline(),
    )
    .await
    .unwrap();
    let TeletextDesignMutation::Saved(saved) = result else {
        panic!("explicit checked announcement must save one unpublished revision")
    };
    saved
}
async fn check(
    fixture: &Fixture,
    flow: &Flow,
    concept: &SavedEditorialTeletextDesign,
    expected: &BankExpected,
    pages: &[u16],
) {
    let stored = drcs_bitmap::stored(fixture, concept).await;
    let decoded = decode_teletext_design_documents(&stored.document, designs::budget()).unwrap();
    assert_eq!(
        decoded
            .editorial_overlay()
            .unwrap()
            .object_subpage_announcements(),
        announcements(pages)
    );
    Box::pin(assert_bank_revision_with_schema(
        fixture, flow, &stored, expected, 26,
    ))
    .await;
}
#[tokio::test]
async fn explicit_subpage_announcement_editor_use_preview_publication_and_cold_reopen() {
    let before = Box::pin(qualify_dormant()).await;
    let one = Box::pin(qualified(first)).await;
    let announced = Box::pin(qualified(all)).await;
    let called = Box::pin(qualified(used)).await;
    assert_eq!(
        before.rasters, announced.rasters,
        "announcement alone changes no display content"
    );
    assert_ne!(
        announced.rasters, called.rasters,
        "the new-bank Q must actually be used"
    );
    assert_eq!(
        before.declared.semantic().base(),
        called.declared.semantic().base(),
        "exact logical Level 1/1.5 fallback"
    );
    let (mut fixture, flow, concept) =
        Box::pin(retained_definitions::setup_retained(&before)).await;
    let retained = resource_bank_admin::Retained::capture(&fixture).await;
    let concept = Box::pin(announce(&fixture, &concept, &[124])).await;
    assert_eq!(concept.head().revision, 4);
    Box::pin(check(&fixture, &flow, &concept, &one, &[124])).await;
    let exported = drcs_bitmap::reexport(&fixture, &concept).await;
    let current = receipt(&fixture, &exported).await;
    let result = import_teletext_editorial_design(
        &fixture.state.application,
        unannounced_call(&exported, &current, 4),
        policy(),
        designs::deadline(),
    )
    .await;
    assert_unannounced_error(result.unwrap_err(), 125);
    assert_head_only(&fixture, &concept).await;
    assert_original_pointers(&fixture, &flow).await;
    let concept = Box::pin(announce(&fixture, &concept, &[125, 126])).await;
    assert_eq!(concept.head().revision, 5);
    fixture = fixture.reopen().await;
    Box::pin(check(
        &fixture,
        &flow,
        &concept,
        &announced,
        &[124, 125, 126],
    ))
    .await;
    let exported = drcs_bitmap::reexport(&fixture, &concept).await;
    let current = receipt(&fixture, &exported).await;
    let concept = import(&fixture, unannounced_call(&exported, &current, 5)).await;
    assert_eq!(concept.head().revision, 6);
    Box::pin(check(&fixture, &flow, &concept, &called, &[124, 125, 126])).await;
    assert_original_pointers(&fixture, &flow).await;
    let exported = drcs_bitmap::reexport(&fixture, &concept).await;
    let current = receipt(&fixture, &exported).await;
    let unchanged = import_teletext_editorial_design(
        &fixture.state.application,
        drcs_bitmap::from_receipt(&current, 6),
        policy(),
        designs::deadline(),
    )
    .await;
    assert!(matches!(
        unchanged,
        Err(NativeTeletextEditorialImportError::NoChanges)
    ));
    assert_head_only(&fixture, &concept).await;
    let accepted = publish_and_accept(&fixture, &flow, &concept).await;
    assert_bank_output(accepted.service(), &called);
    fixture = fixture.reopen().await;
    Box::pin(check(&fixture, &flow, &concept, &called, &[124, 125, 126])).await;
    assert_pointers(&fixture, &accepted, &flow.accepted[1]).await;
    retained.assert_unchanged(&fixture, &flow).await;
    assert!(fixture.playout.try_recv().is_err());
    fixture.state.pool.close().await;
}

#[tokio::test]
async fn explicit_subpage_announcement_source_owner_and_bounds_refuse_atomically() {
    let before = Box::pin(qualify_dormant()).await;
    let (mut fixture, flow, concept) =
        Box::pin(retained_definitions::setup_retained(&before)).await;
    let retained = resource_bank_admin::Retained::capture(&fixture).await;
    let exported = drcs_bitmap::reexport(&fixture, &concept).await;
    let current = receipt(&fixture, &exported).await;
    for variant in 0..6 {
        let mut input = request(&current, 3, &[124]);
        let mut limits = policy();
        match variant {
            0 => input.announcements.clear(),
            1 => input.announcements.extend(announcements(&[124])),
            2 => {
                input.announcements[0] = TeletextSavedObjectSubpageAnnouncement::new(
                    1,
                    input.announcements[0].announcement(),
                )
                .unwrap();
            }
            3 => {
                input.announcements[0] = TeletextSavedObjectSubpageAnnouncement::new(
                    0,
                    TeletextObjectSubpageAnnouncement::new(
                        channel_documents::address(124, 32),
                        bank_resources()[0],
                    )
                    .unwrap(),
                )
                .unwrap();
            }
            4 => {
                input.announcements[0] = TeletextSavedObjectSubpageAnnouncement::new(
                    0,
                    TeletextObjectSubpageAnnouncement::new(
                        channel_documents::address(124, 32),
                        bank_resources()[3],
                    )
                    .unwrap(),
                )
                .unwrap();
            }
            5 => limits.document.validation.service.max_structure_items = 0,
            _ => unreachable!(),
        }
        let result = announce_teletext_editorial_object_subpages(
            &fixture.state.application,
            input,
            limits,
            designs::deadline(),
        )
        .await;
        assert!(
            result.is_err(),
            "explicit announcement control {variant} must refuse: {result:?}"
        );
        assert_head_only(&fixture, &concept).await;
        assert_original_pointers(&fixture, &flow).await;
        assert_eq!(
            qtm_carrier::stored_receipt(&fixture, current.metadata.id).await,
            current
        );
    }
    fixture = fixture.reopen().await;
    Box::pin(assert_bank_concept(&fixture, &flow, &concept, &before)).await;
    retained.assert_unchanged(&fixture, &flow).await;
    assert!(fixture.playout.try_recv().is_err());
    fixture.state.pool.close().await;
}
