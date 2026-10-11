//! Pending originals transfer into output ownership without runtime promotion.

use super::channel_documents::{self, candidate};
use super::designs::{budget, config, deadline, persistent_pair, save, units};
use super::documents::{self, DocumentFixture};
use super::*;
use ersatzrs_core::repositories::TeletextPublicationMutation;
use ersatzrs_web::host::teletext_designs::request_original_teletext_design_publication;
use std::sync::Arc;

fn assert_private_debug(debug: &str, digest: &str) {
    for private in [digest, GENERATION, "night", "THEME", "ZZ", "response"] {
        assert!(
            !debug.contains(private),
            "output diagnostics retain private data"
        );
    }
}

#[tokio::test]
async fn pending_request_moves_complete_resource_bearing_output_without_reinvocation_or_activation()
{
    let (mut fixture, _) = persistent_pair(DocumentFixture::Companion).await;
    let mut policy = channel_documents::policy();
    policy
        .transmissions
        .push(documents::companion_transmission());
    let prepared = candidate(&fixture, policy).await.unwrap();
    let saved = save(&fixture, prepared, None).await;
    let config_before = config(&fixture).await;
    let bindings = fixture
        .state
        .application
        .channel_teletext_bindings(1)
        .await
        .unwrap();
    let requested = request_original_teletext_design_publication(
        &fixture.state.application,
        saved.head().id,
        saved.head().revision,
        saved.digest().to_owned(),
        saved.head().revision,
        None,
        budget(),
        channel_documents::policy(),
        deadline(),
    )
    .await
    .unwrap();
    let TeletextPublicationMutation::Requested(requested) = requested else {
        panic!("unchanged complete originals must yield a pending request");
    };
    let expected_request = requested.request().clone();
    let expected_selection = requested.design().selection().clone();
    let original = requested.design().encoded().unwrap();
    assert_eq!(original.pages().len(), 3);
    assert_eq!(original.declared().resource_inventory().len(), 1);
    let expected_units = units(original);
    let page_allocations = original
        .pages()
        .iter()
        .map(|page| page.data_units().as_ptr())
        .collect::<Vec<_>>();
    let inventory_allocation = original.inventory().as_ptr();
    let magazine_allocation = original.magazine_units().as_ptr();
    let output = requested.into_output_candidate().unwrap();
    assert_eq!(output.request(), &expected_request);
    assert_eq!(output.selection(), &expected_selection);
    assert_eq!(units(output.service()), expected_units);
    assert_eq!(output.service().inventory().as_ptr(), inventory_allocation);
    assert_eq!(
        output.service().magazine_units().as_ptr(),
        magazine_allocation
    );
    assert_eq!(
        output
            .service()
            .pages()
            .iter()
            .map(|page| page.data_units().as_ptr())
            .collect::<Vec<_>>(),
        page_allocations
    );
    let shared = Arc::clone(output.service());
    assert!(Arc::ptr_eq(output.service(), &shared));
    assert_private_debug(&format!("{output:?}"), saved.digest());
    assert_eq!(
        fixture
            .state
            .application
            .teletext_publication_request(1)
            .await
            .unwrap(),
        Some(expected_request)
    );
    assert!(fixture
        .state
        .application
        .teletext_publication_request(2)
        .await
        .unwrap()
        .is_none());
    assert_eq!(config(&fixture).await, config_before);
    assert_eq!(
        fixture
            .state
            .application
            .channel_teletext_bindings(1)
            .await
            .unwrap(),
        bindings
    );
    channel_documents::assert_requests(&fixture, 1, 2);
    assert!(fixture.playout.try_recv().is_err());
    fixture.state.pool.close().await;
}
