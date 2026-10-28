//! Same guarded native-form route as other editorial concept controls.
use super::*;
use std::collections::BTreeMap;

const POST: &str = "/admin-api/teletext/design/choose-secondary-default";

fn fields(flow: &Flow, revision: i64, digest: &str) -> BTreeMap<String, String> {
    BTreeMap::from([
        ("channel_id".into(), "1".into()),
        ("design_id".into(), flow.originals[0].design_id.to_string()),
        ("revision".into(), revision.to_string()),
        ("digest".into(), digest.to_owned()),
        ("expected_design_head".into(), revision.to_string()),
        ("name".into(), "Explicit HTTP secondary agreement".into()),
        ("producer".into(), "0".into()),
        ("choice".into(), "no_second".into()),
        ("designation".into(), String::new()),
    ])
}

async fn submit(fixture: &Fixture, fields: &BTreeMap<String, String>) -> axum::response::Response {
    http_routes::post(
        fixture,
        POST,
        "application/x-www-form-urlencoded",
        serde_urlencoded::to_string(fields).unwrap().into_bytes(),
    )
    .await
}

#[tokio::test]
async fn secondary_default_http_current_authority_roundtrip_and_atomic_rejections() {
    let fixture = Fixture::new_persistent().await;
    http_routes::configure_authoring_policy(&fixture).await;
    let (fixture, flow) = setup_with_fixture(response, fixture).await;
    let original = &flow.originals[0];
    let valid = fields(&flow, 1, &original.digest);
    for (key, value) in [
        ("choice", "unknown"),
        ("producer", "99"),
        ("channel_id", "2"),
        ("expected_design_head", "2"),
        ("digest", "foreign"),
        ("allow_protected", "true"),
    ] {
        let mut invalid = valid.clone();
        invalid.insert(key.into(), value.into());
        let response = submit(&fixture, &invalid).await;
        assert!(
            http_routes::redirect(&response, invalid["channel_id"].parse().unwrap())
                .contains_key("error")
        );
        assert_eq!(
            fixture
                .state
                .application
                .teletext_design_head(original.design_id)
                .await
                .unwrap()
                .unwrap()
                .revision,
            1
        );
        assert_original_pointers_with_width(&fixture, &flow, 40 * 12).await;
    }
    for code in ["7F", "07", "100", "GG", ""] {
        let mut invalid = valid.clone();
        invalid.insert("choice".into(), "designated".into());
        invalid.insert("designation".into(), code.into());
        let response = submit(&fixture, &invalid).await;
        assert_eq!(
            http_routes::redirect(&response, 1)["error"],
            "secondary-default-invalid"
        );
    }
    let mut digest = original.digest.clone();
    for (offset, (choice, code, expected)) in [
        ("no_second", "", Some(agreement(None))),
        ("designated", "37", Some(agreement(Some(0x37)))),
        ("original", "37", None),
    ]
    .into_iter()
    .enumerate()
    {
        let revision = i64::try_from(offset).unwrap() + 1;
        let mut input = fields(&flow, revision, &digest);
        input.insert("choice".into(), choice.into());
        input.insert("designation".into(), code.into());
        let response = submit(&fixture, &input).await;
        let query = http_routes::redirect(&response, 1);
        assert_eq!(
            query.get("saved").map(String::as_str),
            Some("draft"),
            "{query:?}"
        );
        assert_eq!(query["revision"], (revision + 1).to_string());
        let stored = fixture
            .state
            .application
            .teletext_design_revision(
                original.design_id,
                revision + 1,
                designs::budget().max_serialized_bytes,
            )
            .await
            .unwrap()
            .unwrap();
        let decoded =
            decode_teletext_design_documents(&stored.document, designs::budget()).unwrap();
        assert_eq!(
            decoded
                .secondary_default_agreements()
                .first()
                .map(|v| v.agreement()),
            expected
        );
        if expected.is_none() {
            assert_eq!(stored.document, original.document);
            assert_eq!(stored.digest, original.digest);
        }
        digest = stored.digest;
        assert_original_pointers_with_width(&fixture, &flow, 40 * 12).await;
    }
    assert_eq!(designs::config(&fixture).await, flow.config);
    assert_eq!(
        std::fs::read(fixture.root.path().join("authoring-calls")).unwrap(),
        flow.calls
    );
}
