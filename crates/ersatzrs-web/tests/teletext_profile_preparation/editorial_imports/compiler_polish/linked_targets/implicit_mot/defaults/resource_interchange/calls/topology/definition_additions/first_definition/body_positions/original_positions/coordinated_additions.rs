//! Original-position edits and genuine Add bodies keep separate source authority.
use super::*;
use ersatzrs_core::repositories::TeletextDesignRejection;
use ersatzrs_web::host::teletext_editorial_designs::{
    NativeTeletextEditorialImportError, NativeTeletextEditorialTtiFailure,
};

#[path = "coordinated_additions/checks.rs"]
mod checks;

fn request(
    exported: &ExportedOriginalTeletextDesign,
    receipt: &TeletextDesignExportReceipt,
    expected: &checks::Expected,
) -> NativeTeletextEditorialImportRequest {
    let mut request = drcs_bitmap::from_receipt(receipt, receipt.metadata.revision);
    let originals = request.files.clone();
    for (index, records) in expected.records.iter().enumerate() {
        let name = support::name(exported, identity(index));
        request
            .files
            .iter_mut()
            .find(|file| file.name == name)
            .unwrap()
            .bytes = write_teletext_tti(
            &TeletextTtiDocument::checked_from_records(records.clone(), policy().tti).unwrap(),
            policy().tti,
        )
        .unwrap();
    }
    for file in originals
        .iter()
        .filter(|file| (0..2).all(|index| file.name != support::name(exported, identity(index))))
    {
        assert_eq!(
            request.files.iter().find(|value| value.name == file.name),
            Some(file)
        );
    }
    assert_eq!(
        request.sidecar,
        drcs_bitmap::from_receipt(receipt, receipt.metadata.revision).sidecar
    );
    request
}

fn invalid_added_position(
    exported: &ExportedOriginalTeletextDesign,
    request: &mut NativeTeletextEditorialImportRequest,
) -> (u8, usize) {
    let name = support::name(exported, identity(1));
    let file = request
        .files
        .iter_mut()
        .find(|file| file.name == name)
        .unwrap();
    let mut records = parse_teletext_tti(&file.bytes, policy().tti)
        .unwrap()
        .records()
        .to_vec();
    let mut changed = 0;
    let mut location = None;
    for record in &mut records {
        if let TeletextTtiRecord::Packet {
            row: 26,
            designation,
            words,
            ..
        } = record
        {
            for (slot, value) in words.iter_mut().enumerate() {
                if *value == POSITION {
                    *value |= 40 << 11;
                    changed += 1;
                    location = Some((*designation, slot));
                }
            }
        }
    }
    assert_eq!(changed, 1, "change only the new body's actual AP1 column");
    file.bytes = write_teletext_tti(
        &TeletextTtiDocument::checked_from_records(records, policy().tti).unwrap(),
        policy().tti,
    )
    .unwrap();
    location.unwrap()
}

async fn reject_atomic_and_forged(
    fixture: &Fixture,
    flow: &Flow,
    before: &Retained,
    saved: &SavedEditorialTeletextDesign,
    exported: &ExportedOriginalTeletextDesign,
    receipt: &TeletextDesignExportReceipt,
    expected: &checks::Expected,
) {
    let mut invalid = request(exported, receipt, expected);
    let (designation, slot) = invalid_added_position(exported, &mut invalid);
    let mut forged = request(exported, receipt, expected);
    forged.receipt_digest = "0".repeat(64);
    for (request, malformed) in [(invalid, true), (forged, false)] {
        let result = import_teletext_editorial_design(
            &fixture.state.application,
            request,
            policy(),
            designs::deadline(),
        )
        .await;
        let Err(error) = result else {
            panic!("invalid input must reject without a mutation")
        };
        if malformed {
            let NativeTeletextEditorialImportError::Tti(error) = error else {
                panic!("malformed added position must retain TTI validation: {error:?}");
            };
            assert_eq!(error.failure, NativeTeletextEditorialTtiFailure::Structure);
            assert_eq!(
                (error.row, error.designation, error.column),
                (Some(26), Some(designation), Some(slot)),
                "resource-group reconstruction retains the actual rejected X/26 word"
            );
            assert_eq!(error.resource, Some(resources()[0]));
        } else {
            assert!(
                matches!(error, NativeTeletextEditorialImportError::Association),
                "{error:?}"
            );
        }
        checks::head_unchanged(fixture, saved).await;
        unchanged(fixture, flow, before).await;
    }
}

async fn undo_and_publish(
    mut fixture: Fixture,
    flow: &Flow,
    before: &Retained,
    repeated: &SavedEditorialTeletextDesign,
    baseline: (
        &TeletextDesignRevision,
        &ExportedOriginalTeletextDesign,
        &checks::Expected,
    ),
) {
    let (baseline, baseline_export, expected) = baseline;
    let accepted = publish_and_accept(&fixture, flow, repeated).await;
    fixture = fixture.reopen().await;
    checks::published(&fixture, &accepted, &flow.accepted[1]).await;
    let exported = drcs_bitmap::reexport(&fixture, repeated).await;
    let latest = receipt(&fixture, &exported).await;
    assert_eq!(latest.metadata.revision, 4);
    let mut undo = drcs_bitmap::from_receipt(&latest, 4);
    for index in 0..2 {
        let old_name = support::name(baseline_export, identity(index));
        let old = baseline_export
            .files()
            .iter()
            .find(|file| file.name == old_name)
            .unwrap();
        let current_name = support::name(&exported, identity(index));
        undo.files
            .iter_mut()
            .find(|file| file.name == current_name)
            .unwrap()
            .bytes
            .clone_from(&old.bytes);
    }
    let undone = import(&fixture, undo).await;
    assert_eq!(undone.head().revision, 5);
    let stored = drcs_bitmap::stored(&fixture, &undone).await;
    checks::revision(&fixture, flow, &stored, expected, false).await;
    assert_eq!(
        stored.document, baseline.document,
        "undo retains the deliberately added empty26"
    );
    checks::published(&fixture, &accepted, &flow.accepted[1]).await;
    let restored_export = export_stored(&fixture, &stored).await;
    for index in 0..2 {
        assert_eq!(
            file_records(
                &restored_export,
                support::name(&restored_export, identity(index))
            ),
            file_records(
                baseline_export,
                support::name(baseline_export, identity(index))
            ),
        );
    }
    let accepted = checks::publish_after(&fixture, &undone, &accepted).await;
    fixture = fixture.reopen().await;
    checks::revision(&fixture, flow, &stored, expected, false).await;
    checks::published(&fixture, &accepted, &flow.accepted[1]).await;
    assert_retained_storage(&fixture, flow).await;
    assert_eq!(retained(&fixture).await, *before);
    assert!(fixture.playout.is_empty());
    fixture.state.pool.close().await;
}

#[tokio::test]
async fn original_and_admin_added_positions_joint_repeat_undo_publish_and_reopen() {
    let baseline_expected = Box::pin(checks::qualify(checks::added)).await;
    let edited = Box::pin(checks::qualify(checks::edited)).await;
    let repeated_expected = Box::pin(checks::qualify(checks::repeated)).await;
    let fixture = Fixture::new_persistent().await;
    http_routes::configure_authoring_policy(&fixture).await;
    let (mut fixture, flow) = Box::pin(setup_with_fixture_policy_and_writer(
        original_outer_rows,
        fixture,
        encoding,
        authoring_metadata::write_compact,
    ))
    .await;
    let before = retained(&fixture).await;
    assert_eq!(
        submit(&fixture, &fields(&flow.originals[0], 1)).await["saved"],
        "draft"
    );
    let baseline = stored(&fixture, &flow, 1).await;
    assert_eq!(baseline.revision, 2);
    checks::revision(&fixture, &flow, &baseline, &baseline_expected, false).await;
    let baseline_export = export_stored(&fixture, &baseline).await;
    let baseline_receipt = receipt(&fixture, &baseline_export).await;
    let saved = import(
        &fixture,
        request(&baseline_export, &baseline_receipt, &edited),
    )
    .await;
    assert_eq!(saved.head().revision, 3);
    let current = drcs_bitmap::stored(&fixture, &saved).await;
    checks::revision(&fixture, &flow, &current, &edited, true).await;
    unchanged(&fixture, &flow, &before).await;
    let exported = export_stored(&fixture, &current).await;
    let latest = receipt(&fixture, &exported).await;
    reject_atomic_and_forged(
        &fixture,
        &flow,
        &before,
        &saved,
        &exported,
        &latest,
        &repeated_expected,
    )
    .await;
    let stale = request(&exported, &latest, &repeated_expected);
    let repeated = import(&fixture, request(&exported, &latest, &repeated_expected)).await;
    assert_eq!(repeated.head().revision, 4);
    assert!(matches!(
        import_teletext_editorial_design(
            &fixture.state.application,
            stale,
            policy(),
            designs::deadline(),
        )
        .await,
        Ok(TeletextDesignMutation::Rejected(
            TeletextDesignRejection::Conflict
        ))
    ));
    checks::head_unchanged(&fixture, &repeated).await;
    fixture = fixture.reopen().await;
    let current = drcs_bitmap::stored(&fixture, &repeated).await;
    checks::revision(&fixture, &flow, &current, &repeated_expected, true).await;
    unchanged(&fixture, &flow, &before).await;
    undo_and_publish(
        fixture,
        &flow,
        &before,
        &repeated,
        (&baseline, &baseline_export, &baseline_expected),
    )
    .await;
}
