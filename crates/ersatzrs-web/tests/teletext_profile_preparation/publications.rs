//! Real native publication requests are durable intent, not active output.
//! The parent's Unix-only installed producer fixtures remain synthetic.

use super::channel_documents::{self, candidate};
use super::designs::{budget, config, deadline, persistent_pair, save, units};
use super::documents::{self, DocumentFixture};
use super::*;
use ersatzrs_core::repositories::{TeletextPublicationMutation, TeletextPublicationRejection};
use ersatzrs_web::host::teletext_designs::{
    request_original_teletext_design_publication, NativeTeletextDesignError,
    NativeTeletextPublicationError, RequestedOriginalTeletextPublication,
    SavedOriginalTeletextDesign,
};
use std::time::Instant;

async fn request(
    fixture: &Fixture,
    design: &SavedOriginalTeletextDesign,
    expected_head: i64,
    previous: Option<i64>,
) -> Result<
    TeletextPublicationMutation<RequestedOriginalTeletextPublication>,
    NativeTeletextPublicationError,
> {
    request_original_teletext_design_publication(
        &fixture.state.application,
        design.head().id,
        design.head().revision,
        design.digest().to_owned(),
        expected_head,
        previous,
        budget(),
        channel_documents::policy(),
        deadline(),
    )
    .await
}

fn requested(
    result: TeletextPublicationMutation<RequestedOriginalTeletextPublication>,
) -> RequestedOriginalTeletextPublication {
    match result {
        TeletextPublicationMutation::Requested(value) => value,
        TeletextPublicationMutation::Rejected(reason) => {
            panic!("unexpected request rejection: {reason:?}")
        }
    }
}

async fn requests(fixture: &Fixture) -> i64 {
    sqlx::query_scalar(r#"SELECT COUNT(*) FROM "TeletextPublicationRequest""#)
        .fetch_one(&fixture.state.pool)
        .await
        .unwrap()
}

async fn current(fixture: &Fixture) -> Option<i64> {
    sqlx::query_scalar(
        r#"SELECT "RequestId" FROM "TeletextPublicationRequestCurrent" WHERE "ChannelId"=1"#,
    )
    .fetch_optional(&fixture.state.pool)
    .await
    .unwrap()
}

fn calls(fixture: &Fixture) -> Vec<u8> {
    std::fs::read(fixture.root.path().join("channel-calls")).unwrap()
}

fn assert_request_privacy(request: &RequestedOriginalTeletextPublication, digest: &str) {
    let debug = format!("{request:?}");
    for private in [digest, GENERATION, "night", "THEME", "ZZ", "response"] {
        assert!(
            !debug.contains(private),
            "request diagnostics retain private data"
        );
    }
}

#[tokio::test]
async fn resource_bearing_request_reopens_as_pending_without_activation_or_reinvocation() {
    let (mut fixture, originals) = persistent_pair(DocumentFixture::Companion).await;
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
    let queued = requested(request(&fixture, &saved, 1, None).await.unwrap());
    assert_eq!(queued.request().channel_id, 1);
    assert_eq!(queued.request().design_id, saved.head().id);
    assert_eq!(queued.request().revision, 1);
    assert_eq!(queued.request().digest, saved.digest());
    assert_request_privacy(&queued, saved.digest());
    let request_id = queued.request().id;
    let expected_request = queued.request().clone();
    assert_eq!(
        units(queued.design().encoded().unwrap()),
        units(saved.candidate().encoded().unwrap())
    );
    assert_eq!(
        queued.design().transmissions(),
        &[documents::companion_transmission()]
    );
    for (index, source) in queued.design().sources().iter().enumerate() {
        assert_eq!(source.bytes(), originals[index]);
    }
    assert_eq!(requests(&fixture).await, 1);
    assert_eq!(current(&fixture).await, Some(request_id));
    channel_documents::assert_requests(&fixture, 1, 2);
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
    assert!(fixture.playout.try_recv().is_err());
    drop(queued);
    fixture = fixture.reopen().await;
    assert_eq!(requests(&fixture).await, 1);
    assert_eq!(current(&fixture).await, Some(request_id));
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
    let restored = ersatzrs_web::host::teletext_designs::reload_original_teletext_design_revision(
        &fixture.state.application,
        saved.head().id,
        1,
        budget(),
        channel_documents::policy(),
        deadline(),
    )
    .await
    .unwrap()
    .unwrap();
    assert_eq!(
        units(restored.encoded().unwrap()),
        units(saved.candidate().encoded().unwrap())
    );
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

#[tokio::test]
async fn changed_saved_profile_and_tombstoned_design_create_no_request() {
    let (mut fixture, _) = persistent_pair(DocumentFixture::Authored).await;
    let prepared = candidate(&fixture, channel_documents::policy())
        .await
        .unwrap();
    let saved_design = save(&fixture, prepared, None).await;
    let original = &saved_design.candidate().selection().bindings[0];
    let changed = saved(
        fixture
            .state
            .application
            .save_teletext_profile(&TeletextProfileSave {
                addon_id: original.addon_id.clone(),
                name: "Changed after original capture".to_owned(),
                settings: vec![TeletextProfileSetting {
                    key: "THEME".to_owned(),
                    value: TeletextProfileValue::Literal("day".to_owned()),
                }],
                expected_installation: original.installation.clone(),
                existing: Some((original.profile_id, original.profile_revision)),
            })
            .await
            .unwrap(),
    );
    assert_eq!(changed.profile.revision, original.profile_revision + 1);
    let calls_before = calls(&fixture);
    assert!(matches!(
        request(&fixture, &saved_design, 1, None).await.unwrap(),
        TeletextPublicationMutation::Rejected(TeletextPublicationRejection::SelectionChanged)
    ));
    assert_eq!(requests(&fixture).await, 0);
    assert_eq!(current(&fixture).await, None);
    assert_eq!(calls(&fixture), calls_before);
    let tombstone = fixture
        .state
        .application
        .tombstone_teletext_design(saved_design.head().id, 1)
        .await
        .unwrap();
    assert!(
        matches!(tombstone, ersatzrs_core::repositories::TeletextDesignMutation::Saved(ref head) if head.deleted)
    );
    assert!(matches!(
        request(&fixture, &saved_design, 1, None).await.unwrap(),
        TeletextPublicationMutation::Rejected(TeletextPublicationRejection::DeletedDesign)
    ));
    assert_eq!(requests(&fixture).await, 0);
    assert_eq!(current(&fixture).await, None);
    assert_eq!(calls(&fixture), calls_before);
    assert!(fixture.playout.try_recv().is_err());
    fixture.state.pool.close().await;
}

#[tokio::test]
async fn historical_target_requires_fresh_head_and_pending_pointer_cas() {
    let (mut fixture, _) = persistent_pair(DocumentFixture::Authored).await;
    let prepared = candidate(&fixture, channel_documents::policy())
        .await
        .unwrap();
    let first = save(&fixture, prepared, None).await;
    let prepared = candidate(&fixture, channel_documents::policy())
        .await
        .unwrap();
    let second = save(&fixture, prepared, Some((first.head().id, 1))).await;
    assert_eq!(second.head().revision, 2);
    let calls_before = calls(&fixture);
    let config_before = config(&fixture).await;
    assert!(matches!(
        request(&fixture, &first, 1, None).await.unwrap(),
        TeletextPublicationMutation::Rejected(TeletextPublicationRejection::DesignConflict)
    ));
    assert_eq!(requests(&fixture).await, 0);
    let historical = requested(request(&fixture, &first, 2, None).await.unwrap());
    assert_eq!(historical.request().revision, 1);
    assert!(matches!(
        request(&fixture, &second, 2, None).await.unwrap(),
        TeletextPublicationMutation::Rejected(TeletextPublicationRejection::RequestConflict)
    ));
    assert_eq!(current(&fixture).await, Some(historical.request().id));
    let latest = requested(
        request(&fixture, &second, 2, Some(historical.request().id))
            .await
            .unwrap(),
    );
    assert_eq!(latest.request().revision, 2);
    assert_ne!(latest.request().id, historical.request().id);
    assert!(matches!(
        request(&fixture, &first, 2, Some(historical.request().id))
            .await
            .unwrap(),
        TeletextPublicationMutation::Rejected(TeletextPublicationRejection::RequestConflict)
    ));
    assert_eq!(requests(&fixture).await, 2);
    assert_eq!(current(&fixture).await, Some(latest.request().id));
    let stored: Vec<(i64, i64)> = sqlx::query_as(
        r#"SELECT "Id", "Revision" FROM "TeletextPublicationRequest" ORDER BY "Id""#,
    )
    .fetch_all(&fixture.state.pool)
    .await
    .unwrap();
    assert_eq!(
        stored,
        [(historical.request().id, 1), (latest.request().id, 2)]
    );
    assert_eq!(
        fixture
            .state
            .application
            .teletext_design_head(first.head().id)
            .await
            .unwrap()
            .unwrap()
            .revision,
        2
    );
    assert_eq!(calls(&fixture), calls_before);
    assert_eq!(config(&fixture).await, config_before);
    assert!(fixture.playout.try_recv().is_err());
    fixture.state.pool.close().await;
}

#[tokio::test]
async fn deadline_native_validation_digest_and_missing_revision_fail_without_pending_intent() {
    let (mut fixture, _) = persistent_pair(DocumentFixture::Authored).await;
    let prepared = candidate(&fixture, channel_documents::policy())
        .await
        .unwrap();
    let saved = save(&fixture, prepared, None).await;
    let calls_before = calls(&fixture);
    let config_before = config(&fixture).await;
    assert!(matches!(
        request_original_teletext_design_publication(
            &fixture.state.application,
            saved.head().id,
            1,
            saved.digest().to_owned(),
            1,
            None,
            budget(),
            channel_documents::policy(),
            Instant::now()
                .checked_sub(Duration::from_millis(1))
                .expect("test clock permits a past deadline"),
        )
        .await,
        Err(NativeTeletextPublicationError::Design(
            NativeTeletextDesignError::TimedOut
        ))
    ));
    let mut no_packets = channel_documents::policy();
    no_packets.encoding.max_units = 0;
    assert!(matches!(
        request_original_teletext_design_publication(
            &fixture.state.application,
            saved.head().id,
            1,
            saved.digest().to_owned(),
            1,
            None,
            budget(),
            no_packets,
            deadline(),
        )
        .await,
        Err(NativeTeletextPublicationError::Design(
            NativeTeletextDesignError::Encoding(_)
        ))
    ));
    assert!(matches!(
        request_original_teletext_design_publication(
            &fixture.state.application,
            saved.head().id,
            1,
            "0".repeat(64),
            1,
            None,
            budget(),
            channel_documents::policy(),
            deadline(),
        )
        .await
        .unwrap(),
        TeletextPublicationMutation::Rejected(TeletextPublicationRejection::DigestConflict)
    ));
    assert!(matches!(
        request_original_teletext_design_publication(
            &fixture.state.application,
            saved.head().id,
            99,
            saved.digest().to_owned(),
            1,
            None,
            budget(),
            channel_documents::policy(),
            deadline(),
        )
        .await
        .unwrap(),
        TeletextPublicationMutation::Rejected(TeletextPublicationRejection::MissingRevision)
    ));
    assert_eq!(requests(&fixture).await, 0);
    assert_eq!(current(&fixture).await, None);
    assert_eq!(calls(&fixture), calls_before);
    assert_eq!(config(&fixture).await, config_before);
    assert!(fixture.playout.try_recv().is_err());
    fixture.state.pool.close().await;
}
