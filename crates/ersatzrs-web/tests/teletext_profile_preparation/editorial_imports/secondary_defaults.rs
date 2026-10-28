//! Installed profiles -> retained receipt -> context concept -> reopen/undo.
//! Unix-only synthetic producer fixture; no external client or editor GUI claim.
use super::*;

#[path = "secondary_defaults/admin.rs"]
mod admin;
#[path = "secondary_defaults/http.rs"]
mod http;
use ersatzrs_app::teletext_design_documents::decode_teletext_design_documents;
use ersatzrs_core::repositories::TeletextDesignRejection;
use ersatzrs_domain::{
    TeletextPageCharsetDesignation, TeletextProducerSecondarySelection,
    TeletextSecondaryDefaultAgreement, TeletextSecondaryDefaultChoice,
};
use ersatzrs_web::host::teletext_editorial_designs::{
    select_teletext_editorial_secondary_default, NativeTeletextEditorialImportError,
    NativeTeletextSecondaryDefaultRequest,
};
use serde_json::{json, Value};

fn response(theme: &str, page: u16, subcode: u16) -> Value {
    let mut value = authoring_metadata::response(theme, page, subcode);
    value["service"]["magazines"] = json!([]);
    for page in value["service"]["page_enhancements"]
        .as_array_mut()
        .unwrap()
    {
        page["presentation"] = Value::Null;
    }
    value
}
fn protected_response(theme: &str, page: u16, subcode: u16) -> Value {
    let mut value = response(theme, page, subcode);
    for page in value["authoring"]["pages"].as_array_mut().unwrap() {
        page["regions"]
            .as_array_mut()
            .unwrap()
            .retain(|region| region["kind"] == "generated");
    }
    value
}
fn agreement(value: Option<u8>) -> TeletextSecondaryDefaultAgreement {
    TeletextSecondaryDefaultAgreement::new(value.map_or(
        TeletextSecondaryDefaultChoice::NoSecondG0 {},
        |value| TeletextSecondaryDefaultChoice::Designated {
            designation: TeletextPageCharsetDesignation::new(value).unwrap(),
        },
    ))
}
fn request(
    receipt: &TeletextDesignExportReceipt,
    agreement: Option<TeletextSecondaryDefaultAgreement>,
) -> NativeTeletextSecondaryDefaultRequest {
    NativeTeletextSecondaryDefaultRequest {
        name: "Explicit secondary-default concept".into(),
        receipt_id: receipt.metadata.id,
        receipt_digest: receipt.metadata.digest.clone(),
        expected_design_head: receipt.metadata.revision,
        producer: 0,
        agreement,
    }
}
async fn choose(
    fixture: &Fixture,
    receipt: &TeletextDesignExportReceipt,
    agreement: Option<TeletextSecondaryDefaultAgreement>,
) -> SavedEditorialTeletextDesign {
    let result = select_teletext_editorial_secondary_default(
        &fixture.state.application,
        request(receipt, agreement),
        policy(),
        designs::deadline(),
    )
    .await
    .unwrap();
    let TeletextDesignMutation::Saved(saved) = result else {
        panic!("current author choice must save: {result:?}")
    };
    saved
}
async fn current_receipt(
    fixture: &Fixture,
    saved: &SavedEditorialTeletextDesign,
) -> TeletextDesignExportReceipt {
    let exported = drcs_bitmap::reexport(fixture, saved).await;
    receipt(fixture, &exported).await
}
async fn inspect(
    fixture: &Fixture,
    flow: &Flow,
    saved: &SavedEditorialTeletextDesign,
    agreement: Option<TeletextSecondaryDefaultAgreement>,
) {
    let stored = fixture
        .state
        .application
        .teletext_design_revision(
            saved.head().id,
            saved.head().revision,
            designs::budget().max_serialized_bytes,
        )
        .await
        .unwrap()
        .unwrap();
    let decoded = decode_teletext_design_documents(&stored.document, designs::budget()).unwrap();
    assert_eq!(
        decoded.secondary_default_agreements().len(),
        usize::from(agreement.is_some())
    );
    let root =
        decode_teletext_design_documents(&flow.originals[0].document, designs::budget()).unwrap();
    assert_eq!(decoded.selection(), root.selection());
    for (actual, original) in decoded.sources().iter().zip(root.sources()) {
        assert_eq!(actual.bytes(), original.bytes());
        assert_eq!(actual.authoring(), original.authoring());
    }
    let restored = reload_original_teletext_design_revision(
        &fixture.state.application,
        saved.head().id,
        saved.head().revision,
        designs::budget(),
        channel_documents::policy(),
        designs::deadline(),
    )
    .await
    .unwrap()
    .unwrap();
    assert_eq!(
        restored.producers()[0].secondary_default_agreement(),
        agreement
    );
    for page in restored.producers()[0].pages() {
        assert_eq!(
            page.secondary_selection(),
            TeletextProducerSecondarySelection::DecoderDependent
        );
        let mut expected = *page.effective_defaults();
        let expected = agreement.map(|agreement| {
            expected.secondary = agreement.secondary();
            expected
        });
        assert_eq!(
            restored
                .encoded()
                .unwrap()
                .declared()
                .semantic()
                .page_enhancements()[&page.address()]
                .presentation,
            expected
        );
    }
    let exported = drcs_bitmap::reexport(fixture, saved).await;
    assert_eq!(
        exported.sidecar().project.producers[0].secondary_default_agreement,
        agreement
    );
    assert_original_pointers_with_width(fixture, flow, 40 * 12).await;
}

async fn body_request(
    fixture: &Fixture,
    saved: &SavedEditorialTeletextDesign,
) -> NativeTeletextEditorialImportRequest {
    let current = current_receipt(fixture, saved).await;
    drcs_bitmap::from_receipt(&current, saved.head().revision)
}

async fn first_choice(fixture: &Fixture, flow: &Flow) -> SavedEditorialTeletextDesign {
    let first = choose(fixture, &flow.receipts[0], Some(agreement(None))).await;
    assert_eq!(first.head().revision, 2);
    inspect(fixture, flow, &first, Some(agreement(None))).await;
    let mut historical = super::request(flow, 2, 1, b'Y');
    historical.expected_design_head = first.head().revision;
    assert!(
        matches!(
            import_teletext_editorial_design(
                &fixture.state.application,
                historical,
                policy(),
                designs::deadline()
            )
            .await,
            Err(NativeTeletextEditorialImportError::Association)
        ),
        "historical TTI cannot silently withdraw a newer native-only choice"
    );
    let current = current_receipt(fixture, &first).await;
    let same = select_teletext_editorial_secondary_default(
        &fixture.state.application,
        request(&current, Some(agreement(None))),
        policy(),
        designs::deadline(),
    )
    .await;
    assert!(matches!(
        same,
        Err(NativeTeletextEditorialImportError::NoChanges)
    ));
    // Uploaded companion metadata must echo the receipt, never grant a choice.
    let mut tampered = body_request(fixture, &first).await;
    let mut sidecar = drcs_bitmap::reexport(fixture, &first)
        .await
        .sidecar()
        .clone();
    sidecar.project.producers[0].secondary_default_agreement = Some(agreement(Some(0x10)));
    tampered.sidecar = write_teletext_export_sidecar(&sidecar, policy().project).unwrap();
    assert!(import_teletext_editorial_design(
        &fixture.state.application,
        tampered,
        policy(),
        designs::deadline()
    )
    .await
    .is_err());
    let unchanged = import_teletext_editorial_design(
        &fixture.state.application,
        body_request(fixture, &first).await,
        policy(),
        designs::deadline(),
    )
    .await;
    assert!(matches!(
        unchanged,
        Err(NativeTeletextEditorialImportError::NoChanges)
    ));
    first
}

async fn edit_change_and_reopen(
    mut fixture: Fixture,
    flow: &Flow,
    first: &SavedEditorialTeletextDesign,
) -> (Fixture, SavedEditorialTeletextDesign) {
    let mut body = body_request(&fixture, first).await;
    edit_cell(
        &mut body.files,
        &flow.exported[0].sidecar().project.pages[0].name,
        2,
        1,
        b'X',
    );
    let edited = import(&fixture, body).await;
    inspect(&fixture, flow, &edited, Some(agreement(None))).await;
    let current_edited = current_receipt(&fixture, &edited).await;
    let changed = choose(&fixture, &current_edited, Some(agreement(Some(0x10)))).await;
    inspect(&fixture, flow, &changed, Some(agreement(Some(0x10)))).await;
    fixture = fixture.reopen().await;
    inspect(&fixture, flow, &changed, Some(agreement(Some(0x10)))).await;
    let cleared = choose(&fixture, &current_receipt(&fixture, &changed).await, None).await;
    inspect(&fixture, flow, &cleared, None).await;
    for saved in [&changed, &cleared] {
        let stored = fixture
            .state
            .application
            .teletext_design_revision(
                saved.head().id,
                saved.head().revision,
                designs::budget().max_serialized_bytes,
            )
            .await
            .unwrap()
            .unwrap();
        let decoded =
            decode_teletext_design_documents(&stored.document, designs::budget()).unwrap();
        let row = &decoded.editorial_overlay().unwrap().pages()[0].rows()[0];
        assert_eq!(row.row(), 2);
        assert_eq!(row.cells()[1], b'X');
    }
    (fixture, cleared)
}

async fn exact_context_undo(
    fixture: &Fixture,
    flow: &Flow,
    first: &SavedEditorialTeletextDesign,
) -> SavedEditorialTeletextDesign {
    let restored_root = choose(fixture, &current_receipt(fixture, first).await, None).await;
    let stored = fixture
        .state
        .application
        .teletext_design_revision(
            restored_root.head().id,
            restored_root.head().revision,
            designs::budget().max_serialized_bytes,
        )
        .await
        .unwrap()
        .unwrap();
    assert_eq!(stored.document, flow.originals[0].document);
    assert_eq!(restored_root.digest(), flow.originals[0].digest);
    choose(
        fixture,
        &current_receipt(fixture, &restored_root).await,
        Some(agreement(None)),
    )
    .await
}

async fn publish_after_body_and_context_edits(
    fixture: &Fixture,
    flow: &Flow,
    cleared: &SavedEditorialTeletextDesign,
) {
    // The old receipt must not overwrite a newer current concept even when its
    // caller supplies the latest head.
    let mut stale = request(&flow.receipts[0], Some(agreement(None)));
    stale.expected_design_head = cleared.head().revision;
    let result = select_teletext_editorial_secondary_default(
        &fixture.state.application,
        stale,
        policy(),
        designs::deadline(),
    )
    .await
    .unwrap();
    assert!(matches!(
        result,
        TeletextDesignMutation::Rejected(TeletextDesignRejection::Conflict)
    ));
    assert_original_pointers_with_width(fixture, flow, 40 * 12).await;
    let final_choice = choose(
        fixture,
        &current_receipt(fixture, cleared).await,
        Some(agreement(None)),
    )
    .await;
    let accepted = publish_and_accept(fixture, flow, &final_choice).await;
    assert!(accepted
        .service()
        .declared()
        .semantic()
        .page_enhancements()
        .values()
        .all(|page| page
            .presentation
            .is_some_and(|defaults| defaults.secondary.is_none())));
    assert_eq!(
        fixture
            .state
            .application
            .accepted_teletext_generation(2)
            .await
            .unwrap(),
        Some(flow.accepted[1].generation().clone())
    );
    assert_eq!(designs::config(fixture).await, flow.config);
    assert_eq!(
        std::fs::read(fixture.root.path().join("authoring-calls")).unwrap(),
        flow.calls
    );
}

#[tokio::test]
async fn explicit_secondary_context_retains_receipts_body_edits_exact_root_and_publication_separation(
) {
    let (fixture, flow) = setup_with_response(response).await;
    let first = first_choice(&fixture, &flow).await;
    let first = exact_context_undo(&fixture, &flow, &first).await;
    let (fixture, cleared) = edit_change_and_reopen(fixture, &flow, &first).await;
    publish_after_body_and_context_edits(&fixture, &flow, &cleared).await;
}

#[tokio::test]
async fn context_authority_rejections_keep_head_publication_and_selection_unchanged() {
    let (fixture, flow) = setup_with_response(protected_response).await;
    for case in 0..5 {
        let mut input = request(&flow.receipts[0], Some(agreement(None)));
        let deadline = if case == 4 {
            std::time::Instant::now()
        } else {
            designs::deadline()
        };
        match case {
            1 => input.receipt_digest = "f".repeat(64),
            2 => input.producer = 99,
            3 => input.receipt_id = i64::MAX,
            _ => {}
        }
        assert!(
            select_teletext_editorial_secondary_default(
                &fixture.state.application,
                input,
                policy(),
                deadline
            )
            .await
            .is_err(),
            "accepted invalid case {case}"
        );
        assert_eq!(
            fixture
                .state
                .application
                .teletext_design_head(flow.saved[0].head().id)
                .await
                .unwrap(),
            Some(flow.saved[0].head().clone())
        );
        assert_original_pointers_with_width(&fixture, &flow, 40 * 12).await;
    }
}

#[tokio::test]
async fn effective_secondary_context_preserves_scripted_scope_through_concept_and_exact_undo() {
    use super::super::{
        effective_channel_documents as effective, effective_designs,
        effective_editorial_imports as scoped,
    };
    let (mut fixture, _, day, _) = effective_designs::installed(true).await;
    for theme in ["day", "night"] {
        authoring_metadata::write(&fixture, theme, &response(theme, 123, 16));
    }
    effective::publish(&fixture, 1, Some(&day)).await;
    let candidate = effective::candidate(&fixture, 1, Some(ADDON))
        .await
        .unwrap();
    let saved = effective_designs::saved_design(
        effective_designs::save(&fixture, candidate, None, designs::budget())
            .await
            .unwrap(),
    );
    let original = fixture
        .state
        .application
        .teletext_design_revision(saved.head().id, 1, designs::budget().max_serialized_bytes)
        .await
        .unwrap()
        .unwrap();
    let before =
        ersatzrs_app::teletext_design_documents::decode_teletext_effective_design_documents(
            &original.document,
            designs::budget(),
        )
        .unwrap();
    let exported = scoped::export(&fixture, &original).await;
    let retained = receipt(&fixture, &exported).await;
    let config = designs::config(&fixture).await;
    let calls = std::fs::read(fixture.root.path().join("authoring-calls")).unwrap();
    let chosen = choose(&fixture, &retained, Some(agreement(None))).await;
    assert_eq!(chosen.head().revision, 2);
    fixture = fixture.reopen().await;
    let restored =
        effective_designs::reload(&fixture, chosen.head().id, chosen.head().revision).await;
    assert_eq!(restored.selection(), before.selection());
    assert_eq!(restored.producer_bindings(), before.producer_bindings());
    assert_eq!(
        restored.producers()[0].secondary_default_agreement(),
        Some(agreement(None))
    );
    assert_eq!(restored.sources()[0].bytes(), before.sources()[0].bytes());
    let exported = drcs_bitmap::reexport(&fixture, &chosen).await;
    assert_eq!(
        exported.sidecar().project.producers[0].secondary_default_agreement,
        Some(agreement(None))
    );
    let retained = receipt(&fixture, &exported).await;
    let cleared = choose(&fixture, &retained, None).await;
    let stored = fixture
        .state
        .application
        .teletext_design_revision(
            cleared.head().id,
            cleared.head().revision,
            designs::budget().max_serialized_bytes,
        )
        .await
        .unwrap()
        .unwrap();
    assert_eq!(stored.document, original.document);
    assert_eq!(cleared.digest(), original.digest);
    assert_eq!(designs::config(&fixture).await, config);
    assert_eq!(
        std::fs::read(fixture.root.path().join("authoring-calls")).unwrap(),
        calls
    );
}
