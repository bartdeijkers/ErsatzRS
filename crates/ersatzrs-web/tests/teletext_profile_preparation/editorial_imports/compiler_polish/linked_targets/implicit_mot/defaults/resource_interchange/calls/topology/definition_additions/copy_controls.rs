//! The production Copy form saves a native concept without publishing it.
use super::*;
use ersatzrs_domain::{TeletextObjectId, TeletextObjectIdentity};
use std::collections::BTreeMap;

const COPY: &str = "/admin-api/teletext/design/copy-object";

fn fields(
    flow: &Flow,
    channel: i64,
    source: TeletextObjectIdentity,
    new_id: u16,
) -> BTreeMap<String, String> {
    BTreeMap::from([
        ("channel_id".into(), channel.to_string()),
        ("design_id".into(), flow.saved[0].head().id.to_string()),
        ("revision".into(), "1".into()),
        ("digest".into(), flow.saved[0].digest().to_owned()),
        ("expected_design_head".into(), "1".into()),
        ("name".into(), "HTTP copied resource definition".into()),
        (
            "source_object".into(),
            serde_json::to_string(&source).unwrap(),
        ),
        ("new_object_id".into(), new_id.to_string()),
    ])
}
async fn submit(fixture: &Fixture, fields: &BTreeMap<String, String>) -> BTreeMap<String, String> {
    let response = http_routes::post(
        fixture,
        COPY,
        "application/x-www-form-urlencoded",
        serde_urlencoded::to_string(fields).unwrap().into_bytes(),
    )
    .await;
    http_routes::redirect(&response, fields["channel_id"].parse().unwrap())
}

async fn assert_copied(
    fixture: &Fixture,
    flow: &Flow,
    expected: &Expected,
) -> TeletextDesignRevision {
    let id = flow.saved[0].head().id;
    let head = fixture
        .state
        .application
        .teletext_design_head(id)
        .await
        .unwrap()
        .unwrap();
    assert_eq!(head.revision, 2);
    assert!(!head.deleted);
    assert!(fixture
        .state
        .application
        .teletext_design_revision(id, 3, designs::budget().max_serialized_bytes)
        .await
        .unwrap()
        .is_none());
    let stored = fixture
        .state
        .application
        .teletext_design_revision(id, 2, designs::budget().max_serialized_bytes)
        .await
        .unwrap()
        .unwrap();
    let document = decode_teletext_design_documents(&stored.document, designs::budget()).unwrap();
    let overlay = document.editorial_overlay().unwrap();
    assert_eq!(overlay.schema_version(), 18);
    let [addition] = overlay.resource_object_definition_additions() else {
        panic!("HTTP Copy must persist one complete native addition")
    };
    assert_eq!(addition.definition(), &expected.definition);
    assert!(
        overlay.pages().is_empty()
            && overlay.local_program_operands().is_empty()
            && overlay.local_object_operands().is_empty()
            && overlay.local_object_glyph_program_overlays().is_empty()
            && overlay.resource_object_operands().is_empty()
            && overlay.resource_object_program_overlays().is_empty()
            && overlay.drcs_bitmap_overlays().is_empty()
    );
    let restored = reload_original_teletext_design_revision(
        &fixture.state.application,
        id,
        2,
        designs::budget(),
        channel_documents::policy(),
        designs::deadline(),
    )
    .await
    .unwrap()
    .unwrap();
    let original = flow.saved[0].candidate();
    assert_eq!(restored.selection(), original.selection());
    assert_eq!(restored.metadata(), original.metadata());
    assert_eq!(restored.transmissions(), original.transmissions());
    assert_eq!(restored.sources().len(), original.sources().len());
    for (after, before) in restored.sources().iter().zip(original.sources()) {
        assert_eq!(after.bytes(), before.bytes());
        assert_eq!(after.authoring(), before.authoring());
    }
    assert_eq!(
        restored.producers()[0].original(),
        original.producers()[0].original()
    );
    assert_output(restored.encoded().unwrap(), expected);
    assert_original_pointers(fixture, flow).await;
    stored
}

async fn assert_exported(fixture: &Fixture, stored: &TeletextDesignRevision, expected: &Expected) {
    let result = export_original_teletext_design(
        &fixture.state.application,
        NativeTeletextExportRequest {
            design_id: stored.design_id,
            revision: stored.revision,
            design_digest: stored.digest.clone(),
            expected_design_head: 2,
            pages: None,
        },
        policy(),
        designs::deadline(),
    )
    .await
    .unwrap();
    let TeletextDesignMutation::Saved(exported) = result else {
        panic!("HTTP copied concept must re-export through the existing native flow")
    };
    assert_eq!(exported.metadata().revision, 2);
    assert_eq!(
        exported.sidecar().project.editor_profile,
        "qteletextmaker-1.0-carrier-v8"
    );
    let records = file_records(&exported, passive_name(&exported));
    assert_eq!(word(&records, 7), expected.glyph_word);
}

async fn reject_original_controls(fixture: &Fixture, flow: &Flow) {
    let local = TeletextObjectIdentity::local(
        channel_documents::address(124, 32),
        TeletextObjectId::new(21),
    );
    for (source, new_id) in [
        (seed::target(21), 22),
        (seed::target(21), 21),
        (local, 25),
        (seed::target(99), 25),
    ] {
        let query = submit(fixture, &fields(flow, 1, source, new_id)).await;
        assert_eq!(query["error"], "object-copy-invalid");
        assert!(!query.contains_key("saved"));
        assert_unchanged(fixture, flow).await;
    }
    let query = submit(fixture, &fields(flow, 2, seed::target(21), 25)).await;
    assert_eq!(query["error"], "foreign-channel");
    assert_unchanged(fixture, flow).await;
}

#[tokio::test]
async fn http_resource_definition_copy_preserves_native_concept_and_rejects_invalid_controls() {
    let dormant = Box::pin(qualify(State::Dormant)).await;
    let fixture = Fixture::new_persistent().await;
    http_routes::configure_authoring_policy(&fixture).await;
    let (mut fixture, flow) = Box::pin(setup_with_fixture_policy_and_writer(
        super::super::response,
        fixture,
        seed::encoding,
        authoring_metadata::write_compact,
    ))
    .await;
    let retained = resource_bank_admin::Retained::capture(&fixture).await;
    Box::pin(reject_original_controls(&fixture, &flow)).await;
    let submitted = fields(&flow, 1, seed::target(21), 25);
    let query = submit(&fixture, &submitted).await;
    assert_eq!(query["saved"], "draft");
    assert_eq!(query["design_id"], flow.saved[0].head().id.to_string());
    assert_eq!(query["revision"], "2");
    assert_eq!(query["page"], "124");
    assert_eq!(query["subcode"], "0020");
    let stored = Box::pin(assert_copied(&fixture, &flow, &dormant)).await;
    Box::pin(assert_exported(&fixture, &stored, &dormant)).await;
    retained.assert_unchanged(&fixture, &flow).await;
    let stale = submit(&fixture, &submitted).await;
    assert_eq!(stale["error"], "publication-conflict");
    Box::pin(assert_copied(&fixture, &flow, &dormant)).await;
    retained.assert_unchanged(&fixture, &flow).await;
    assert!(fixture.playout.try_recv().is_err());
    fixture = fixture.reopen().await;
    Box::pin(assert_copied(&fixture, &flow, &dormant)).await;
    Box::pin(assert_exported(&fixture, &stored, &dormant)).await;
    retained.assert_unchanged(&fixture, &flow).await;
    assert!(fixture.playout.try_recv().is_err());
    fixture.state.pool.close().await;
}
