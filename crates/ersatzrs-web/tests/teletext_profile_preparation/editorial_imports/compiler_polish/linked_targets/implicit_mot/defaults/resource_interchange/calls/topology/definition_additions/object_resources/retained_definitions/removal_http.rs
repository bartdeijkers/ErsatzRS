//! The real POST route authenticates current owner/receipt, not observed choices.
use super::*;
use ersatzrs_web::host::teletext_authoring::{
    preview_original_teletext_design, TeletextAuthoringPreviewRequest,
};
use std::collections::BTreeMap;

const REMOVE: &str = "/admin-api/teletext/design/remove-unused-object";

fn fields(concept: &SavedEditorialTeletextDesign) -> BTreeMap<String, String> {
    BTreeMap::from([
        ("channel_id".into(), "1".into()),
        ("design_id".into(), concept.head().id.to_string()),
        ("revision".into(), concept.head().revision.to_string()),
        ("digest".into(), concept.digest().to_owned()),
        (
            "expected_design_head".into(),
            concept.head().revision.to_string(),
        ),
        ("name".into(), "Remove unused HTTP copy".into()),
        (
            "object".into(),
            serde_json::to_string(&copied_identity()).unwrap(),
        ),
    ])
}
async fn submit(fixture: &Fixture, fields: &BTreeMap<String, String>) -> BTreeMap<String, String> {
    let response = http_routes::post(
        fixture,
        REMOVE,
        "application/x-www-form-urlencoded",
        serde_urlencoded::to_string(fields).unwrap().into_bytes(),
    )
    .await;
    http_routes::redirect(&response, fields["channel_id"].parse().unwrap())
}

async fn assert_invalid(
    fixture: &Fixture,
    flow: &Flow,
    first: &BankExpected,
    copied: &SavedEditorialTeletextDesign,
    retained: &resource_bank_admin::Retained,
) {
    let valid = fields(copied);
    for variant in 0..6 {
        let mut invalid = valid.clone();
        match variant {
            0 => {
                invalid.insert(
                    "object".into(),
                    serde_json::to_string(
                        &first.declared.semantic().objects().definitions()[0].identity,
                    )
                    .unwrap(),
                );
            }
            1 => {
                invalid.insert(
                    "object".into(),
                    serde_json::to_string(&first.definition26.identity).unwrap(),
                );
            }
            2 => {
                invalid.insert("expected_design_head".into(), "1".into());
            }
            3 => {
                invalid.insert("channel_id".into(), "2".into());
            }
            4 => {
                invalid.insert("allow_delete".into(), "true".into());
            }
            _ => {
                invalid.insert("object".into(), "[]".into());
            }
        }
        let query = submit(fixture, &invalid).await;
        assert!(
            query.contains_key("error"),
            "invalid removal {variant} saved a draft"
        );
        assert_eq!(
            fixture
                .state
                .application
                .teletext_design_head(copied.head().id)
                .await
                .unwrap()
                .unwrap(),
            *copied.head(),
        );
        retained.assert_unchanged(fixture, flow).await;
        assert_original_pointers(fixture, flow).await;
    }
}

#[tokio::test]
async fn unused_copy_http_removal_observes_eligible_ids_and_preserves_native_draft_boundaries() {
    let first = Box::pin(qualify_banks(false, false)).await;
    let (mut fixture, flow, original) = Box::pin(setup_retained(&first)).await;
    let copied = Box::pin(copy_retained(&fixture, &original)).await;
    let retained = resource_bank_admin::Retained::capture(&fixture).await;
    let owner = first.declared.semantic().base().pages[0].address;
    let preview = preview_original_teletext_design(
        &fixture.state.application,
        TeletextAuthoringPreviewRequest {
            channel_id: 1,
            design_id: copied.head().id,
            revision: copied.head().revision,
            owner: Some(owner),
        },
        designs::deadline(),
    )
    .await
    .unwrap();
    assert!(preview
        .removable_resource_objects()
        .contains(&copied_identity()));
    assert!(preview
        .removable_resource_objects()
        .contains(&first.definition25.identity));
    assert!(!preview
        .removable_resource_objects()
        .contains(&first.definition26.identity));
    assert!(!preview
        .removable_resource_objects()
        .contains(&first.declared.semantic().objects().definitions()[0].identity,));
    drop(preview);
    Box::pin(assert_invalid(&fixture, &flow, &first, &copied, &retained)).await;
    let valid = fields(&copied);
    let query = submit(&fixture, &valid).await;
    assert_eq!(query["saved"], "draft");
    let revision: i64 = query["revision"].parse().unwrap();
    assert_eq!(revision, copied.head().revision + 1);
    let stored = fixture
        .state
        .application
        .teletext_design_revision(
            copied.head().id,
            revision,
            designs::budget().max_serialized_bytes,
        )
        .await
        .unwrap()
        .unwrap();
    assert_eq!(stored.digest, original.digest());
    assert_original_pointers(&fixture, &flow).await;
    retained.assert_unchanged(&fixture, &flow).await;
    fixture = fixture.reopen().await;
    let preview = preview_original_teletext_design(
        &fixture.state.application,
        TeletextAuthoringPreviewRequest {
            channel_id: 1,
            design_id: copied.head().id,
            revision,
            owner: Some(owner),
        },
        designs::deadline(),
    )
    .await
    .unwrap();
    assert!(!preview
        .removable_resource_objects()
        .contains(&copied_identity()));
    drop(preview);
    assert!(fixture.playout.try_recv().is_err());
    fixture.state.pool.close().await;
}
