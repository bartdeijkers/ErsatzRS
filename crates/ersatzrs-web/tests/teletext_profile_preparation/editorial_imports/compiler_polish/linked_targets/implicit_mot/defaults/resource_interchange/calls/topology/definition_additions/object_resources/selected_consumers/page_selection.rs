//! Saved raw-owner intent and shared resource calls use one native concept boundary.
use super::*;
use ersatzrs_app::teletext_design_documents::{
    TeletextSavedObjectPageSelectionChange, TeletextSavedObjectSubpageAnnouncement,
};
use ersatzrs_domain::{
    TeletextObjectPageSelectionChange, TeletextObjectSubpageAnnouncement,
    TeletextResourceObjectProgramOverlay, TeletextSubpageRequirementMask,
    TeletextX27LinkDeclaration, TeletextX27LinkTarget, TeletextX27LinkValidity,
};
use ersatzrs_web::host::teletext_editorial_designs::{
    announce_teletext_editorial_object_subpages, select_teletext_editorial_object_pages,
    NativeTeletextObjectPagesSelectRequest, NativeTeletextObjectSubpagesAnnounceRequest,
};
#[path = "page_selection/oracle.rs"]
mod oracle;

async fn setup(
    first: &BankExpected,
    original: fn(&str, u16, u16) -> Value,
) -> (Fixture, Flow, SavedEditorialTeletextDesign) {
    let fixture = Fixture::new_persistent().await;
    http_routes::configure_authoring_policy(&fixture).await;
    let (fixture, flow) = Box::pin(setup_with_fixture_policy_and_writer(
        original,
        fixture,
        seed::encoding,
        authoring_metadata::write_compact,
    ))
    .await;
    let concept = add(&fixture, &flow.receipts[0], first.definition25.clone()).await;
    let exported = drcs_bitmap::reexport(&fixture, &concept).await;
    let current = receipt(&fixture, &exported).await;
    let concept = add_banks(
        &fixture,
        bank_request(
            &current,
            2,
            first.banks.clone(),
            vec![first.definition26.clone()],
        ),
    )
    .await;
    (fixture, flow, concept)
}
fn announcements() -> Vec<TeletextSavedObjectSubpageAnnouncement> {
    (124..127)
        .map(|page| {
            TeletextSavedObjectSubpageAnnouncement::new(
                0,
                TeletextObjectSubpageAnnouncement::new(
                    channel_documents::address(page, 32),
                    resources()[1],
                )
                .unwrap(),
            )
            .unwrap()
        })
        .collect()
}
async fn announce(
    fixture: &Fixture,
    concept: &SavedEditorialTeletextDesign,
) -> SavedEditorialTeletextDesign {
    let exported = drcs_bitmap::reexport(fixture, concept).await;
    let current = receipt(fixture, &exported).await;
    let result = announce_teletext_editorial_object_subpages(
        &fixture.state.application,
        NativeTeletextObjectSubpagesAnnounceRequest {
            name: "Explicit historical POP S1".into(),
            receipt_id: current.metadata.id,
            receipt_digest: current.metadata.digest.clone(),
            expected_design_head: concept.head().revision,
            announcements: announcements(),
        },
        policy(),
        designs::deadline(),
    )
    .await
    .unwrap();
    let TeletextDesignMutation::Saved(saved) = result else {
        panic!("announcement saves unpublished concept")
    };
    saved
}
fn selections(
    service: &TeletextDeclaredService,
    selected: bool,
) -> Vec<TeletextSavedObjectPageSelectionChange> {
    (124..127)
        .map(|page| {
            let display = channel_documents::address(page, 32);
            let metadata = &service.semantic().page_enhancements()[&display];
            let resolution = service
                .resolutions()
                .iter()
                .find(|value| value.display() == display)
                .unwrap();
            TeletextSavedObjectPageSelectionChange::new(
                0,
                TeletextObjectPageSelectionChange::new(
                    display,
                    TeletextX27LinkRole::Gpop,
                    metadata.compositional_links,
                    *resolution.role(TeletextX27LinkRole::Gpop),
                    TeletextX27LinkDeclaration::new(
                        TeletextX27LinkTarget::Page {
                            page: if selected {
                                resources()[0].page()
                            } else {
                                seed::target(20).resource().unwrap().page()
                            },
                        },
                        TeletextX27LinkValidity::Level25Only,
                        TeletextSubpageRequirementMask::new(1),
                    )
                    .unwrap(),
                )
                .unwrap(),
            )
            .unwrap()
        })
        .collect()
}
fn program_change(
    before: &TeletextDeclaredService,
    after: &TeletextDeclaredService,
) -> TeletextResourceObjectProgramOverlay {
    let body = |service: &TeletextDeclaredService| {
        service
            .semantic()
            .objects()
            .definition(seed::target(20))
            .unwrap()
            .program
            .clone()
    };
    let original = body(before);
    TeletextResourceObjectProgramOverlay::new_with_invocation_changes(
        seed::target(20),
        original.clone(),
        body(after),
        (0..original.commands().len()).map(Some).collect(),
        seed::encoding().composition.service,
    )
    .unwrap()
}
fn request(
    current: &TeletextDesignExportReceipt,
    concept: &SavedEditorialTeletextDesign,
    before: &BankExpected,
    after: &BankExpected,
    selected: bool,
) -> NativeTeletextObjectPagesSelectRequest {
    NativeTeletextObjectPagesSelectRequest {
        name: "Atomic selected raw GPOP and shared Passive call".into(),
        receipt_id: current.metadata.id,
        receipt_digest: current.metadata.digest.clone(),
        expected_design_head: concept.head().revision,
        selections: selections(&before.declared, selected),
        programs: vec![program_change(&before.declared, &after.declared)],
    }
}
async fn select(
    fixture: &Fixture,
    concept: &SavedEditorialTeletextDesign,
    before: &BankExpected,
    after: &BankExpected,
    selected: bool,
) -> SavedEditorialTeletextDesign {
    let exported = drcs_bitmap::reexport(fixture, concept).await;
    let current = receipt(fixture, &exported).await;
    let result = select_teletext_editorial_object_pages(
        &fixture.state.application,
        request(&current, concept, before, after, selected),
        policy(),
        designs::deadline(),
    )
    .await
    .unwrap();
    let TeletextDesignMutation::Saved(saved) = result else {
        panic!("atomic owner/call action saves unpublished concept")
    };
    saved
}
async fn check(
    fixture: &Fixture,
    flow: &Flow,
    concept: &SavedEditorialTeletextDesign,
    expected: &BankExpected,
    selected: bool,
) {
    let stored = drcs_bitmap::stored(fixture, concept).await;
    let decoded = decode_teletext_design_documents(&stored.document, designs::budget()).unwrap();
    let overlay = decoded.editorial_overlay().unwrap();
    assert_eq!(overlay.object_subpage_announcements(), announcements());
    assert_eq!(
        overlay.object_page_selections().len(),
        if selected { 3 } else { 0 }
    );
    assert_eq!(
        overlay.resource_object_program_overlays().len(),
        usize::from(selected)
    );
    Box::pin(assert_bank_revision_with_schema(
        fixture,
        flow,
        &stored,
        expected,
        if selected { 27 } else { 26 },
    ))
    .await;
}
fn scalar_edit(
    exported: &ExportedOriginalTeletextDesign,
    receipt: &TeletextDesignExportReceipt,
    head: i64,
    before: u8,
    after: u8,
) -> NativeTeletextEditorialImportRequest {
    let resource = exported
        .sidecar()
        .project
        .assets
        .iter()
        .find_map(|asset| match &asset.scope {
            TeletextExportAssetScope::Resource { identity, .. }
                if identity.kind == TeletextResourceKindWire::Gpop
                    && identity.page.magazine == 2
                    && identity.page.page == 0xe1
                    && identity.subpage == 0 =>
            {
                Some(asset.name.as_str())
            }
            _ => None,
        })
        .expect("retained GPOP e1/0 resource");
    let name = exported
        .sidecar()
        .project
        .assets
        .iter()
        .find_map(|asset| match &asset.scope {
            TeletextExportAssetScope::ObjectContextTtiV1 {
                resource_name,
                object_id: 32,
                ..
            } if resource_name == resource => Some(asset.name.as_str()),
            _ => None,
        })
        .expect("actual editable Passive32 context");
    let mut input = drcs_bitmap::from_receipt(receipt, head);
    let originals = input.files.clone();
    let sidecar = input.sidecar.clone();
    let file = input
        .files
        .iter_mut()
        .find(|file| file.name == name)
        .unwrap();
    let mut records = parse_teletext_tti(&file.bytes, policy().tti)
        .unwrap()
        .records()
        .to_vec();
    assert_eq!(word(&records, 7), 1 | (9 << 6) | (u32::from(before) << 11));
    for record in &mut records {
        if let TeletextTtiRecord::Packet {
            row: 26,
            designation: 0,
            words,
        } = record
        {
            words[7] = 1 | (9 << 6) | (u32::from(after) << 11);
        }
    }
    file.bytes = write_teletext_tti(
        &TeletextTtiDocument::checked_from_records(records, policy().tti).unwrap(),
        policy().tti,
    )
    .unwrap();
    assert_eq!(input.sidecar, sidecar);
    for (original, changed) in originals.iter().zip(&input.files) {
        assert_eq!(original.name, changed.name);
        if original.name != name {
            assert_eq!(
                original.bytes, changed.bytes,
                "all native resources and sibling contexts remain exact"
            );
        }
    }
    input
}
#[tokio::test]
async fn atomic_selected_owner_calls_preserve_history_fresh_tti_undo_preview_publication_and_cold_reopen(
) {
    let [before, announced, selected, edited] = Box::pin(oracle::all()).await;
    let (mut fixture, flow, concept) = Box::pin(setup(&before, oracle::original)).await;
    let retained = resource_bank_admin::Retained::capture(&fixture).await;
    Box::pin(assert_bank_concept(&fixture, &flow, &concept, &before)).await;
    let concept = Box::pin(announce(&fixture, &concept)).await;
    assert_eq!(concept.head().revision, 4);
    Box::pin(check(&fixture, &flow, &concept, &announced, false)).await;
    let concept = Box::pin(select(&fixture, &concept, &announced, &selected, true)).await;
    assert_eq!(concept.head().revision, 5);
    Box::pin(check(&fixture, &flow, &concept, &selected, true)).await;
    assert_original_pointers(&fixture, &flow).await;
    let exported = drcs_bitmap::reexport(&fixture, &concept).await;
    let current = receipt(&fixture, &exported).await;
    let unchanged = import_teletext_editorial_design(
        &fixture.state.application,
        drcs_bitmap::from_receipt(&current, 5),
        policy(),
        designs::deadline(),
    )
    .await;
    assert!(
        matches!(
            unchanged,
            Err(NativeTeletextEditorialImportError::NoChanges)
        ),
        "ordinary fresh TTI retains, rather than authors, selection intent: {unchanged:?}"
    );
    let concept = import(&fixture, scalar_edit(&exported, &current, 5, b'Q', b'R')).await;
    assert_eq!(concept.head().revision, 6);
    Box::pin(check(&fixture, &flow, &concept, &edited, true)).await;
    assert_original_pointers(&fixture, &flow).await;
    let exported = drcs_bitmap::reexport(&fixture, &concept).await;
    let current = receipt(&fixture, &exported).await;
    let concept = import(&fixture, scalar_edit(&exported, &current, 6, b'R', b'Q')).await;
    assert_eq!(concept.head().revision, 7);
    Box::pin(check(&fixture, &flow, &concept, &selected, true)).await;
    fixture = fixture.reopen().await;
    Box::pin(check(&fixture, &flow, &concept, &selected, true)).await;
    let concept = Box::pin(select(&fixture, &concept, &selected, &announced, false)).await;
    assert_eq!(concept.head().revision, 8);
    Box::pin(check(&fixture, &flow, &concept, &announced, false)).await;
    let concept = Box::pin(select(&fixture, &concept, &announced, &selected, true)).await;
    assert_eq!(concept.head().revision, 9);
    Box::pin(check(&fixture, &flow, &concept, &selected, true)).await;
    assert_original_pointers(&fixture, &flow).await;
    let accepted = publish_and_accept(&fixture, &flow, &concept).await;
    assert_bank_output(accepted.service(), &selected);
    fixture = fixture.reopen().await;
    Box::pin(check(&fixture, &flow, &concept, &selected, true)).await;
    assert_pointers(&fixture, &accepted, &flow.accepted[1]).await;
    retained.assert_unchanged(&fixture, &flow).await;
    assert!(fixture.playout.try_recv().is_err());
    fixture.state.pool.close().await;
}
#[tokio::test]
async fn selected_owner_and_calls_refuse_stale_source_bodies_shared_consumers_limits_and_deadlines_atomically(
) {
    let [before, announced, selected, _] = Box::pin(oracle::all()).await;
    let (mut fixture, flow, concept) = Box::pin(setup(&before, oracle::original)).await;
    let retained = resource_bank_admin::Retained::capture(&fixture).await;
    let concept = Box::pin(announce(&fixture, &concept)).await;
    let exported = drcs_bitmap::reexport(&fixture, &concept).await;
    let current = receipt(&fixture, &exported).await;
    for variant in 0..7 {
        let mut input = request(&current, &concept, &announced, &selected, true);
        let mut bounds = policy();
        let mut deadline = designs::deadline();
        match variant {
            0 => input.expected_design_head -= 1,
            1 => {
                input.selections[0] =
                    TeletextSavedObjectPageSelectionChange::new(1, input.selections[0].change())
                        .unwrap();
            }
            2 => input.programs = vec![program_change(&selected.declared, &announced.declared)],
            3 => input.selections.truncate(1),
            4 => {
                for saved in &mut input.selections {
                    let change = saved.change();
                    *saved = TeletextSavedObjectPageSelectionChange::new(
                        saved.document_index(),
                        TeletextObjectPageSelectionChange::new(
                            change.display(),
                            change.role(),
                            change.expected_links(),
                            change.expected_role(),
                            TeletextX27LinkDeclaration::new(
                                change.replacement().target(),
                                change.replacement().validity(),
                                TeletextSubpageRequirementMask::new(0),
                            )
                            .unwrap(),
                        )
                        .unwrap(),
                    )
                    .unwrap();
                }
            }
            5 => bounds.document.validation.service.max_structure_items = 0,
            6 => deadline = std::time::Instant::now(),
            _ => unreachable!(),
        }
        let result = select_teletext_editorial_object_pages(
            &fixture.state.application,
            input,
            bounds,
            deadline,
        )
        .await;
        if variant == 0 {
            assert!(
                matches!(
                    result,
                    Ok(TeletextDesignMutation::Rejected(
                        ersatzrs_core::repositories::TeletextDesignRejection::Conflict
                    ))
                ),
                "stale head must retain the existing typed conflict: {result:?}"
            );
        } else {
            assert!(
                result.is_err(),
                "selected-owner negative control {variant}: {result:?}"
            );
        }
        assert_head_only(&fixture, &concept).await;
        assert_original_pointers(&fixture, &flow).await;
        assert_eq!(
            qtm_carrier::stored_receipt(&fixture, current.metadata.id).await,
            current
        );
    }
    fixture = fixture.reopen().await;
    Box::pin(check(&fixture, &flow, &concept, &announced, false)).await;
    // The same valid request succeeds once malformed variants are removed.
    let concept = Box::pin(select(&fixture, &concept, &announced, &selected, true)).await;
    Box::pin(check(&fixture, &flow, &concept, &selected, true)).await;
    assert_original_pointers(&fixture, &flow).await;
    retained.assert_unchanged(&fixture, &flow).await;
    assert!(fixture.playout.try_recv().is_err());
    fixture.state.pool.close().await;
    Box::pin(refuse_generated_consumer(&before, &announced, &selected)).await;
}

async fn refuse_generated_consumer(
    before: &BankExpected,
    announced: &BankExpected,
    selected: &BankExpected,
) {
    // Only immutable authoring permissions change: the same nested P -> Q
    // consumer is now Generated, so a valid role/call request must be denied.
    let (mut fixture, flow, concept) = Box::pin(setup(before, oracle::generated_original)).await;
    let retained = resource_bank_admin::Retained::capture(&fixture).await;
    let concept = Box::pin(announce(&fixture, &concept)).await;
    let exported = drcs_bitmap::reexport(&fixture, &concept).await;
    let current = receipt(&fixture, &exported).await;
    let error = select_teletext_editorial_object_pages(
        &fixture.state.application,
        request(&current, &concept, announced, selected, true),
        policy(),
        designs::deadline(),
    )
    .await
    .unwrap_err();
    let NativeTeletextEditorialImportError::Effects(error) = error else {
        panic!("valid selection must reach protected shared-consumer permission: {error:?}")
    };
    assert_eq!(
        error.reason,
        TeletextEditorialEffectsFailure::ProtectedDisplay
    );
    assert_eq!(error.page, Some(channel_documents::address(125, 32)));
    let (row, column) = error.position.unwrap();
    assert_eq!(row, 5);
    // Effect diagnostics use visual coordinates: the original four-column
    // left panel maps protected Main columns11..12 to visual15..16.
    assert!((15..=16).contains(&column), "{error:?}");
    assert_head_only(&fixture, &concept).await;
    assert_original_pointers(&fixture, &flow).await;
    assert_eq!(
        qtm_carrier::stored_receipt(&fixture, current.metadata.id).await,
        current
    );
    fixture = fixture.reopen().await;
    Box::pin(check(&fixture, &flow, &concept, announced, false)).await;
    retained.assert_unchanged(&fixture, &flow).await;
    assert!(fixture.playout.try_recv().is_err());
    fixture.state.pool.close().await;
}
