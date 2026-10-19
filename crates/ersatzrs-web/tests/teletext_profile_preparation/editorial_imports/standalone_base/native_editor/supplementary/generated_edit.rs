//! Generated G2 words must not impersonate original page-local commands.
use super::*;
use ersatzrs_web::host::teletext_editorial_designs::{
    NativeTeletextEditorialImportError, NativeTeletextEditorialTtiFailure,
};

fn request(flow: &Flow, context: Context) -> NativeTeletextEditorialImportRequest {
    let mut request = drcs_bitmap::from_receipt(&flow.receipts[0], 1);
    let file = request
        .files
        .iter_mut()
        .find(|file| file.name == "page-124-0020.tti")
        .unwrap();
    let mut records = parse_teletext_tti(&file.bytes, policy().tti)
        .unwrap()
        .records()
        .to_vec();
    let mut targets = Vec::new();
    for record in &mut records {
        if let TeletextTtiRecord::Packet {
            row: 26,
            designation,
            words,
        } = record
        {
            for (slot, word) in words.iter_mut().enumerate() {
                if (*word >> 6) & 31 == 15 && *word >> 11 == 0x23 {
                    targets.push((usize::from(*designation) * 13 + slot, *word));
                    *word =
                        (*word & 0x7ff) | (if context.primary == 0x37 { 0x24 } else { 0x25 } << 11);
                }
            }
        }
    }
    assert_eq!(
        targets.len(),
        1,
        "exact original generated £ word: {targets:?}"
    );
    println!(
        "native generated G2 context{:02x} physical target {targets:?}",
        context.primary
    );
    file.bytes = write_teletext_tti(
        &TeletextTtiDocument::checked_from_records(records, policy().tti).unwrap(),
        policy().tti,
    )
    .unwrap();
    request
}

async fn assert_unchanged(fixture: &Fixture, flow: &Flow) {
    assert_original_pointers_with_width(fixture, flow, 480).await;
    for original in &flow.saved {
        assert_eq!(
            fixture
                .state
                .application
                .teletext_design_head(original.head().id)
                .await
                .unwrap()
                .unwrap()
                .revision,
            1
        );
    }
    assert!(fixture.playout.is_empty());
}

// This protects explicit, atomic refusal while generated Base scalar editing
// remains an open L15-B product gap. It does not accept the missing edit flow.
#[tokio::test]
async fn generated_native_g2_word_edit_rejects_without_concept_or_channel_damage() {
    for context in CONTEXTS_G2 {
        let (mut fixture, flow) = Box::pin(setup_with_fixture_policy_and_writer(
            context.response,
            Fixture::new_persistent().await,
            channel_documents::policy,
            authoring_metadata::write_compact,
        ))
        .await;
        let error = import_teletext_editorial_design(
            &fixture.state.application,
            request(&flow, context),
            policy(),
            designs::deadline(),
        )
        .await
        .unwrap_err();
        let NativeTeletextEditorialImportError::Tti(error) = error else {
            panic!("generated Base G2 must not impersonate authored Local intent: {error:?}");
        };
        assert_eq!(error.failure, NativeTeletextEditorialTtiFailure::Packet);
        assert_eq!(error.page, 124);
        assert_eq!(error.subcode, 32);
        assert_eq!(error.row, Some(26));
        assert_eq!(error.designation, Some(0));
        assert_eq!(error.column, Some(1));
        assert_unchanged(&fixture, &flow).await;
        fixture = fixture.reopen().await;
        assert_unchanged(&fixture, &flow).await;
        fixture.state.pool.close().await;
    }
}
