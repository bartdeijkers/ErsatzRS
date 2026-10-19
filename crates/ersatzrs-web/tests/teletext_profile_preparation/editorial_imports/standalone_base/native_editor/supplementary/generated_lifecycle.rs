//! Fresh receipt authority, repeated Base text editing and trusted root undo.
use super::*;
use ersatzrs_core::repositories::TeletextDesignRejection;

fn edit(
    receipt: &TeletextDesignExportReceipt,
    head: i64,
    from: u8,
    to: u8,
) -> NativeTeletextEditorialImportRequest {
    let mut request = drcs_bitmap::from_receipt(receipt, head);
    let file = request
        .files
        .iter_mut()
        .find(|f| f.name == "page-124-0020.tti")
        .unwrap();
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
    assert_eq!(words[1], 8 | (15 << 6) | (u32::from(from) << 11));
    words[1] = 8 | (15 << 6) | (u32::from(to) << 11);
    file.bytes = write_teletext_tti(
        &TeletextTtiDocument::checked_from_records(records, policy().tti).unwrap(),
        policy().tti,
    )
    .unwrap();
    request
}

async fn rejected(fixture: &Fixture, request: NativeTeletextEditorialImportRequest) {
    let result = import_teletext_editorial_design(
        &fixture.state.application,
        request,
        policy(),
        designs::deadline(),
    )
    .await;
    assert!(matches!(result,
        Err(ersatzrs_web::host::teletext_editorial_designs::NativeTeletextEditorialImportError::Association)
        | Ok(TeletextDesignMutation::Rejected(TeletextDesignRejection::Conflict))
    ), "exact receipt/CAS rejection: {result:?}");
}

async fn assert_root_undo(fixture: &Fixture, flow: &Flow, undo: &SavedEditorialTeletextDesign) {
    let restored = drcs_bitmap::restored(fixture, undo).await;
    let actual = restored.encoded().unwrap();
    let original = flow.accepted[0].service();
    assert_eq!(designs::units(actual), designs::units(original));
    assert_eq!(actual.magazine_units(), original.magazine_units());
    for page in [124, 125] {
        assert_eq!(
            external_editor_probe::preview_page(actual, page),
            external_editor_probe::preview_page(original, page)
        );
    }
}

#[tokio::test]
async fn generated_base_g2_fresh_receipt_repeat_root_undo_and_stale_refusal() {
    for context in [CONTEXTS_G2[0], CONTEXTS_G2[3]] {
        let (mut fixture, flow) = Box::pin(setup_with_fixture_policy_and_writer(
            context.response,
            Fixture::new_persistent().await,
            channel_documents::policy,
            authoring_metadata::write_compact,
        ))
        .await;
        let first = import(&fixture, edit(&flow.receipts[0], 1, 35, 37)).await;
        assert_eq!(first.head().revision, 2);
        rejected(&fixture, edit(&flow.receipts[0], 1, 35, 36)).await;
        let exported = drcs_bitmap::reexport(&fixture, &first).await;
        let current = receipt(&fixture, &exported).await;
        let mut forged = edit(&current, 2, 37, 39);
        forged.receipt_digest = "f".repeat(64);
        rejected(&fixture, forged).await;
        let second = import(&fixture, edit(&current, 2, 37, 39)).await;
        assert_eq!(second.head().revision, 3);
        fixture = fixture.reopen().await;
        let exported = drcs_bitmap::reexport(&fixture, &second).await;
        let current = receipt(&fixture, &exported).await;
        let undo = import(&fixture, edit(&current, 3, 39, 35)).await;
        assert_eq!(undo.head().revision, 4);
        assert_root_undo(&fixture, &flow, &undo).await;
        let root_export = drcs_bitmap::reexport(&fixture, &undo).await;
        national_editor::assert_files(root_export.files(), flow.exported[0].files());
        assert_original_pointers_with_width(&fixture, &flow, 480).await;
        assert_retained_storage(&fixture, &flow).await;
        fixture = fixture.reopen().await;
        assert_root_undo(&fixture, &flow, &undo).await;
        assert_original_pointers_with_width(&fixture, &flow, 480).await;
        assert_retained_storage(&fixture, &flow).await;
        fixture.state.pool.close().await;
    }
}

#[tokio::test]
async fn lossless_generated_g2_repeat_reopen_canonicalization_and_exact_root_undo() {
    let context = CONTEXTS_G2[0];
    let (mut fixture, flow) = Box::pin(setup_with_fixture_policy_and_writer(
        context.response,
        Fixture::new_persistent().await,
        channel_documents::policy,
        authoring_metadata::write_compact,
    ))
    .await;
    let first = import(&fixture, edit(&flow.receipts[0], 1, 35, 36)).await;
    assert_eq!(first.head().revision, 2);
    rejected(&fixture, edit(&flow.receipts[0], 1, 35, 37)).await;
    fixture = fixture.reopen().await;
    let restored = drcs_bitmap::restored(&fixture, &first).await;
    let semantic = restored.encoded().unwrap().declared().semantic();
    assert_eq!(semantic.base_text_spellings().len(), 1);
    assert_eq!(
        semantic
            .base_text_spellings()
            .values()
            .next()
            .unwrap()
            .character(),
        '$'
    );
    let exported = drcs_bitmap::reexport(&fixture, &first).await;
    let current = receipt(&fixture, &exported).await;
    let mut forged = edit(&current, 2, 36, 37);
    forged.receipt_digest = "f".repeat(64);
    rejected(&fixture, forged).await;
    let second = import(&fixture, edit(&current, 2, 36, 37)).await;
    assert_eq!(second.head().revision, 3);
    let restored = drcs_bitmap::restored(&fixture, &second).await;
    assert!(restored
        .encoded()
        .unwrap()
        .declared()
        .semantic()
        .base_text_spellings()
        .is_empty());
    fixture = fixture.reopen().await;
    let exported = drcs_bitmap::reexport(&fixture, &second).await;
    let current = receipt(&fixture, &exported).await;
    let undo = import(&fixture, edit(&current, 3, 37, 35)).await;
    assert_eq!(undo.head().revision, 4);
    assert_root_undo(&fixture, &flow, &undo).await;
    fixture = fixture.reopen().await;
    assert_root_undo(&fixture, &flow, &undo).await;
    assert_original_pointers_with_width(&fixture, &flow, 480).await;
    assert_retained_storage(&fixture, &flow).await;
    fixture.state.pool.close().await;
}
