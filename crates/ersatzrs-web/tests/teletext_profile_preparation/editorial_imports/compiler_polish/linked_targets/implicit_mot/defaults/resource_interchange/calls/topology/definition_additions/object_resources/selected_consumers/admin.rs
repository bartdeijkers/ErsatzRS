//! Real admin routes for explicit bank creation and selected dependencies.
use super::*;
use std::collections::BTreeMap;
#[path = "admin/actual_admin.rs"]
mod actual_admin;
#[path = "admin/oracle.rs"]
mod oracle;
const ADD_BANK: &str = "/admin-api/teletext/design/add-resource-bank";
const ANNOUNCE: &str = "/admin-api/teletext/design/announce-object-subpages";
type Fields = Vec<(String, String)>;
fn fields(stored: &TeletextDesignRevision) -> Fields {
    [
        ("channel_id", "1".to_owned()),
        ("design_id", stored.design_id.to_string()),
        ("revision", stored.revision.to_string()),
        ("digest", stored.digest.clone()),
        ("expected_design_head", stored.revision.to_string()),
        ("name", "Explicit native bank and dependencies".to_owned()),
    ]
    .map(|(key, value)| (key.to_owned(), value))
    .to_vec()
}
fn bank_fields(stored: &TeletextDesignRevision, global: bool) -> Fields {
    let mut fields = fields(stored);
    fields.extend(
        [
            (
                "display",
                serde_json::to_string(&channel_documents::address(124, 32)).unwrap(),
            ),
            (
                "resource_kind",
                if global { "gpop" } else { "pop" }.to_owned(),
            ),
            (
                "resource_page",
                if global { "2D0" } else { "2C0" }.to_owned(),
            ),
            ("logical_subpage", "14".to_owned()),
            ("counter", "7".to_owned()),
            ("update", "changed".to_owned()),
            ("last_subpage", "true".to_owned()),
        ]
        .map(|(key, value)| (key.to_owned(), value)),
    );
    fields
}
fn definition_fields(stored: &TeletextDesignRevision, global: bool) -> Fields {
    let mut fields = fields(stored);
    fields.extend(
        [
            (
                "resource",
                serde_json::to_string(&oracle::resource(global)).unwrap(),
            ),
            ("object_type", "active".to_owned()),
            ("new_object_id", "32".to_owned()),
        ]
        .map(|(key, value)| (key.to_owned(), value)),
    );
    fields
}
fn announcement_fields(stored: &TeletextDesignRevision, global: bool) -> Fields {
    let mut fields = fields(stored);
    fields.push((
        "resource".to_owned(),
        serde_json::to_string(&oracle::resource(global)).unwrap(),
    ));
    fields.extend([124, 125, 126].map(|page| {
        (
            "display".to_owned(),
            serde_json::to_string(&channel_documents::address(page, 32)).unwrap(),
        )
    }));
    fields
}
async fn submit(fixture: &Fixture, route: &str, fields: &Fields) -> BTreeMap<String, String> {
    let channel = fields
        .iter()
        .find(|(key, _)| key == "channel_id")
        .unwrap()
        .1
        .parse()
        .unwrap();
    let response = http_routes::post(
        fixture,
        route,
        "application/x-www-form-urlencoded",
        serde_urlencoded::to_string(fields).unwrap().into_bytes(),
    )
    .await;
    http_routes::redirect(&response, channel)
}
async fn current(fixture: &Fixture, id: i64, revision: i64) -> TeletextDesignRevision {
    let app = &fixture.state.application;
    let head = app.teletext_design_head(id).await.unwrap().unwrap();
    assert_eq!(head.revision, revision);
    assert!(!head.deleted);
    assert!(app
        .teletext_design_revision(id, revision + 1, designs::budget().max_serialized_bytes)
        .await
        .unwrap()
        .is_none());
    app.teletext_design_revision(id, revision, designs::budget().max_serialized_bytes)
        .await
        .unwrap()
        .unwrap()
}
pub(super) async fn publish(
    fixture: &Fixture,
    flow: &Flow,
    stored: &TeletextDesignRevision,
) -> AcceptedTeletextOutputCandidate {
    use ersatzrs_core::repositories::TeletextPublicationMutation;
    let pending =
        ersatzrs_web::host::teletext_designs::request_original_teletext_design_publication(
            &fixture.state.application,
            stored.design_id,
            stored.revision,
            stored.digest.clone(),
            stored.revision,
            Some(flow.pending[0].id),
            designs::budget(),
            channel_documents::policy(),
            designs::deadline(),
        )
        .await
        .unwrap();
    let TeletextPublicationMutation::Requested(pending) = pending else {
        panic!("explicit publication must remain separate")
    };
    assert_eq!(
        fixture
            .state
            .application
            .accepted_teletext_generation(1)
            .await
            .unwrap(),
        Some(flow.accepted[0].generation().clone())
    );
    generations::accepted(
        accept_original_teletext_generation(
            &fixture.state.application,
            TeletextGenerationTarget {
                request: pending.request().clone(),
                expected_design_head: stored.revision,
                expected_generation_id: Some(flow.accepted[0].generation().id),
            },
            designs::budget(),
            channel_documents::policy(),
            designs::deadline(),
        )
        .await
        .unwrap(),
    )
}
#[tokio::test]
async fn http_explicit_bank_add_definition_and_multidisplay_announcement_are_atomic_and_recoverable(
) {
    let before = Box::pin(qualify_dormant()).await;
    let expected = Box::pin(oracle::qualify_all()).await;
    let (mut fixture, flow, concept) =
        Box::pin(retained_definitions::setup_retained(&before)).await;
    let retained = resource_bank_admin::Retained::capture(&fixture).await;
    let mut stored = drcs_bitmap::stored(&fixture, &concept).await;
    reject_controls(&fixture, &flow, &stored).await;
    for (index, expected_stage) in expected.iter().enumerate() {
        let global = index >= 3;
        let (route, input) = match index % 3 {
            0 => (ADD_BANK, bank_fields(&stored, global)),
            1 => (
                "/admin-api/teletext/design/add-object",
                definition_fields(&stored, global),
            ),
            _ => (ANNOUNCE, announcement_fields(&stored, global)),
        };
        let response = submit(&fixture, route, &input).await;
        assert_eq!(response["saved"], "draft");
        let revision = i64::try_from(index).unwrap() + 4;
        assert_eq!(response["revision"], revision.to_string());
        stored = current(&fixture, stored.design_id, revision).await;
        Box::pin(assert_bank_revision_with_schema(
            &fixture,
            &flow,
            &stored,
            expected_stage,
            if index >= 2 { 26 } else { 25 },
        ))
        .await;
        assert_original_pointers(&fixture, &flow).await;
    }
    fixture = fixture.reopen().await;
    Box::pin(assert_bank_revision_with_schema(
        &fixture,
        &flow,
        &stored,
        &expected[5],
        26,
    ))
    .await;
    let accepted = publish(&fixture, &flow, &stored).await;
    assert_bank_output(accepted.service(), &expected[5]);
    fixture = fixture.reopen().await;
    Box::pin(assert_bank_revision_with_schema(
        &fixture,
        &flow,
        &stored,
        &expected[5],
        26,
    ))
    .await;
    assert_pointers(&fixture, &accepted, &flow.accepted[1]).await;
    retained.assert_unchanged(&fixture, &flow).await;
    assert!(fixture.playout.try_recv().is_err());
    fixture.state.pool.close().await;
}
async fn reject_controls(fixture: &Fixture, flow: &Flow, stored: &TeletextDesignRevision) {
    for (route, mut input) in [
        (ADD_BANK, bank_fields(stored, false)),
        (ANNOUNCE, announcement_fields(stored, false)),
    ] {
        for variant in 0..if route == ANNOUNCE { 5 } else { 3 } {
            let mut changed = input.clone();
            match variant {
                0 => changed.push(("document_index".to_owned(), "0".to_owned())),
                1 => changed.push(("name".to_owned(), "duplicate".to_owned())),
                2 => {
                    "2".clone_into(
                        &mut changed
                            .iter_mut()
                            .find(|(key, _)| key == "expected_design_head")
                            .unwrap()
                            .1,
                    );
                }
                3 => changed.push((
                    "display".to_owned(),
                    serde_json::to_string(&channel_documents::address(124, 32)).unwrap(),
                )),
                4 => {
                    changed
                        .iter_mut()
                        .find(|(key, _)| key == "resource")
                        .unwrap()
                        .1 = serde_json::to_string(&resources()[0]).unwrap();
                }
                _ => unreachable!(),
            }
            let response = submit(fixture, route, &changed).await;
            assert_eq!(
                response["error"],
                if variant == 2 {
                    "publication-conflict"
                } else if variant >= 3 {
                    "subpage-announcement-invalid"
                } else {
                    "invalid-input"
                }
            );
            assert!(!response.contains_key("saved"));
            assert_eq!(
                current(fixture, stored.design_id, stored.revision).await,
                *stored
            );
            assert_original_pointers(fixture, flow).await;
        }
        "2".clone_into(
            &mut input
                .iter_mut()
                .find(|(key, _)| key == "channel_id")
                .unwrap()
                .1,
        );
        assert_eq!(
            submit(fixture, route, &input).await["error"],
            "foreign-channel"
        );
        assert_eq!(
            current(fixture, stored.design_id, stored.revision).await,
            *stored
        );
        assert_original_pointers(fixture, flow).await;
    }
}
