//! A2: the admin uploads the complete fitting resource set as one concept.
use super::*;
use crate::editorial_imports::http_routes as http;

async fn refused(fixture: &Fixture, flow: &Flow) {
    let mut invalid = complete_request(flow);
    edit_word(
        invalid
            .files
            .iter_mut()
            .find(|file| file.name == passive_name(flow))
            .unwrap(),
        0x28a41,
        0xfa41,
    );
    let response = http::import_response(fixture, &invalid, 1).await;
    let query = http::redirect(&response, 1);
    assert_eq!(
        query.get("error").map(String::as_str),
        Some("editorial-protected"),
        "{query:?}"
    );
    let resource: ersatzrs_domain::TeletextResourceIdentity = serde_json::from_str(
        query
            .get("error_resource")
            .expect("resource error must retain its native resource identity"),
    )
    .unwrap();
    assert_eq!(Some(resource), seed::target(21).resource());
    for (key, value) in [
        ("error_row", "26"),
        ("error_designation", "0"),
        ("error_column", "7"),
    ] {
        assert_eq!(query.get(key).map(String::as_str), Some(value), "{query:?}");
    }
    let html = http::rendered(fixture, response.headers()["location"].to_str().unwrap()).await;
    for literal in [
        "data-testid=\"teletext-error-location\"",
        "pop 2C0 / 0",
        "26, 7",
    ] {
        assert!(
            html.contains(literal),
            "missing rendered native error {literal}"
        );
    }
    assert_eq!(
        fixture
            .state
            .application
            .teletext_design_head(flow.saved[0].head().id)
            .await
            .unwrap(),
        Some(flow.saved[0].head().clone())
    );
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn admin_complete_resource_set_multipart_is_atomic_owned_and_revision_fenced() {
    let (target_fixture, target) = Box::pin(setup_with_fixture_policy_and_writer(
        combined,
        Fixture::new_persistent().await,
        seed::encoding,
        authoring_metadata::write_compact,
    ))
    .await;
    let fixture = Fixture::new_persistent().await;
    http::configure_authoring_policy(&fixture).await;
    let (fixture, flow) = Box::pin(setup_with_fixture_policy_and_writer(
        original_complete,
        fixture,
        seed::encoding,
        authoring_metadata::write_compact,
    ))
    .await;
    let retained = resource_bank_admin::Retained::capture(&fixture).await;
    let request = complete_request(&flow);
    let foreign = http::import_response(&fixture, &request, 2).await;
    assert_eq!(http::redirect(&foreign, 2)["error"], "foreign-channel");
    refused(&fixture, &flow).await;
    assert_original_pointers(&fixture, &flow).await;
    retained.assert_unchanged(&fixture, &flow).await;
    let response = http::import_response(&fixture, &request, 1).await;
    let query = http::redirect(&response, 1);
    for (key, value) in [
        ("saved", "draft"),
        ("revision", "2"),
        ("page", "124"),
        ("subcode", "0020"),
    ] {
        assert_eq!(
            query.get(key).map(String::as_str),
            Some(value),
            "{key}: {query:?}"
        );
    }
    let html = http::rendered(&fixture, response.headers()["location"].to_str().unwrap()).await;
    for literal in [
        "data-testid=\"teletext-preview-model\"",
        "data-page=\"124\"",
        "data-subcode=\"0020\"",
        "data-testid=\"teletext-editor-import-form\"",
    ] {
        assert!(
            html.contains(literal),
            "missing current concept preview {literal}"
        );
    }
    let restored = reload_original_teletext_design_revision(
        &fixture.state.application,
        flow.saved[0].head().id,
        2,
        designs::budget(),
        seed::encoding(),
        designs::deadline(),
    )
    .await
    .unwrap()
    .unwrap();
    exact_service(restored.encoded().unwrap(), target.accepted[0].service());
    let stale = http::import_response(&fixture, &request, 1).await;
    assert_eq!(http::redirect(&stale, 1)["error"], "conflict");
    assert_eq!(
        fixture
            .state
            .application
            .teletext_design_head(flow.saved[0].head().id)
            .await
            .unwrap()
            .unwrap()
            .revision,
        2
    );
    assert_original_pointers(&fixture, &flow).await;
    retained.assert_unchanged(&fixture, &flow).await;
    fixture.state.pool.close().await;
    target_fixture.state.pool.close().await;
}
