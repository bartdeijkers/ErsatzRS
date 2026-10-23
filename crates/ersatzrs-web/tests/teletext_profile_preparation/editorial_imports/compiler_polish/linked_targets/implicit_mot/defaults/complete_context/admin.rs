//! A2 joins the closed complete-context action to the real multipart Router.
use super::*;
use crate::editorial_imports::http_routes as http;

fn stale_restore(flow: &Flow, target: &Flow) -> NativeTeletextEditorialImportRequest {
    let mut request = checks::request(flow, target);
    let file = request
        .files
        .iter_mut()
        .find(|file| file.name == "page-124-0020.tti")
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
    assert_eq!(words[11], 113_160);
    words[11] = 178_696;
    file.bytes = write_teletext_tti(
        &TeletextTtiDocument::checked_from_records(records, policy().tti).unwrap(),
        policy().tti,
    )
    .unwrap();
    request
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn admin_complete_context_multipart_preserves_located_refusal_and_exact_concept() {
    let (target_fixture, target) = Box::pin(setup_with_fixture_policy_and_writer(
        edited,
        Fixture::new_persistent().await,
        seed::encoding,
        authoring_metadata::write_compact,
    ))
    .await;
    let fixture = Fixture::new_persistent().await;
    http::configure_authoring_policy(&fixture).await;
    let (fixture, flow) = Box::pin(setup_with_fixture_policy_and_writer(
        root,
        fixture,
        seed::encoding,
        authoring_metadata::write_compact,
    ))
    .await;
    let retained = resource_bank_admin::Retained::capture(&fixture).await;
    let response = http::import_response(&fixture, &stale_restore(&flow, &target), 1).await;
    let query = http::redirect(&response, 1);
    for (key, value) in [
        ("error", "editorial-protected"),
        ("error_page", "124"),
        ("error_subcode", "0020"),
        ("error_row", "26"),
        ("error_designation", "0"),
        ("error_column", "11"),
    ] {
        assert_eq!(
            query.get(key).map(String::as_str),
            Some(value),
            "{key}: {query:?}"
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
    assert_original_pointers(&fixture, &flow).await;
    retained.assert_unchanged(&fixture, &flow).await;
    let response = http::import_response(&fixture, &checks::request(&flow, &target), 1).await;
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
    let wanted = target.accepted[0].service();
    let actual = restored.encoded().unwrap();
    assert_eq!(actual.declared(), wanted.declared());
    assert_eq!(designs::units(actual), designs::units(wanted));
    assert_eq!(actual.magazine_units(), wanted.magazine_units());
    assert_eq!(preview(actual), preview(wanted));
    assert_original_pointers(&fixture, &flow).await;
    retained.assert_unchanged(&fixture, &flow).await;
    fixture.state.pool.close().await;
    target_fixture.state.pool.close().await;
}
