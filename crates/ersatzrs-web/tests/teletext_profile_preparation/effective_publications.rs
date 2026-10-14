//! Scoped pending intents with actual Lua/native originals, never accepted output.

use super::*;
use super::{
    channel_documents, designs, effective_channel_documents as effective,
    effective_designs as originals,
};
use ersatzrs_core::repositories::{
    TeletextPublicationMutation as M, TeletextPublicationRejection as R,
};
use ersatzrs_domain::TeletextPublicationRequest;
use ersatzrs_web::host::{
    teletext_effective_channel_documents::prepare_effective_channel_teletext_document_candidate,
    teletext_effective_designs::SavedEffectiveOriginalDesign,
    teletext_effective_publications::{
        request_effective_original_teletext_design_publication,
        NativeEffectiveTeletextPublicationError, RequestedEffectiveTeletextPublication,
        TeletextEffectivePublicationTarget,
    },
};

pub(super) fn target(
    value: &SavedEffectiveOriginalDesign,
    previous: Option<i64>,
) -> TeletextEffectivePublicationTarget {
    TeletextEffectivePublicationTarget {
        channel_id: value.candidate().selection().channel_id,
        design_id: value.head().id,
        revision: value.head().revision,
        expected_digest: value.digest().to_owned(),
        expected_design_head: value.head().revision,
        expected_request_id: previous,
    }
}
pub(super) async fn request(
    fixture: &Fixture,
    value: &SavedEffectiveOriginalDesign,
    previous: Option<i64>,
) -> TeletextPublicationRequest {
    match request_effective_original_teletext_design_publication(
        &fixture.state.application,
        target(value, previous),
        designs::budget(),
        channel_documents::policy(),
        designs::deadline(),
    )
    .await
    .unwrap()
    {
        M::Requested(value) => value.request().clone(),
        M::Rejected(reason) => panic!("unexpected scoped request rejection {reason:?}"),
    }
}
async fn attempt(
    fixture: &Fixture,
    target: TeletextEffectivePublicationTarget,
) -> Result<M<RequestedEffectiveTeletextPublication>, NativeEffectiveTeletextPublicationError> {
    request_effective_original_teletext_design_publication(
        &fixture.state.application,
        target,
        designs::budget(),
        channel_documents::policy(),
        designs::deadline(),
    )
    .await
}

#[tokio::test]
async fn scoped_pending_target_and_pointer_cas_are_exact_without_accepted_or_base_writes() {
    let (mut fixture, _, day, _) = originals::installed(true).await;
    effective::publish(&fixture, 1, Some(&day)).await;
    let value = originals::saved_design(
        originals::save(
            &fixture,
            effective::candidate(&fixture, 1, Some(ADDON))
                .await
                .unwrap(),
            None,
            designs::budget(),
        )
        .await
        .unwrap(),
    );
    let before = designs::config(&fixture).await;
    let calls = std::fs::read(fixture.root.path().join("authoring-calls")).unwrap();
    let pending = request(&fixture, &value, None).await;
    assert_eq!(
        (pending.channel_id, pending.design_id, pending.revision),
        (1, value.head().id, 1)
    );
    assert_eq!(pending.digest, value.digest());
    assert_eq!(
        fixture
            .state
            .application
            .effective_teletext_publication_request(1)
            .await
            .unwrap(),
        Some(pending.clone())
    );
    assert!(fixture
        .state
        .application
        .teletext_publication_request(1)
        .await
        .unwrap()
        .is_none());
    assert!(fixture
        .state
        .application
        .effective_teletext_accepted_generation(1)
        .await
        .unwrap()
        .is_none());
    assert!(fixture
        .state
        .application
        .accepted_teletext_generation(1)
        .await
        .unwrap()
        .is_none());
    assert!(matches!(
        attempt(&fixture, target(&value, None)).await.unwrap(),
        M::Rejected(R::RequestConflict)
    ));
    let mut foreign = target(&value, Some(pending.id));
    foreign.channel_id = 2;
    assert!(matches!(
        attempt(&fixture, foreign).await.unwrap(),
        M::Rejected(R::InvalidValues)
    ));
    let mut digest = target(&value, Some(pending.id));
    digest.expected_digest = "0".repeat(64);
    assert!(matches!(
        attempt(&fixture, digest).await.unwrap(),
        M::Rejected(R::DigestConflict)
    ));
    let mut head = target(&value, Some(pending.id));
    head.expected_design_head = 2;
    assert!(matches!(
        attempt(&fixture, head).await.unwrap(),
        M::Rejected(R::DesignConflict)
    ));
    assert_eq!(
        fixture
            .state
            .application
            .effective_teletext_publication_request(1)
            .await
            .unwrap(),
        Some(pending)
    );
    assert_eq!(
        std::fs::read(fixture.root.path().join("authoring-calls")).unwrap(),
        calls
    );
    assert_eq!(designs::config(&fixture).await, before);
    assert!(fixture.playout.try_recv().is_err());
}

#[tokio::test]
async fn scoped_request_rejects_base_only_v2_and_guide_inputs_without_fabricated_authority() {
    for guide_bearing in [false, true] {
        let (mut fixture, _, day, _) = originals::installed(true).await;
        if guide_bearing {
            effective::publish(&fixture, 1, Some(&day)).await;
        }
        let guide_channels = [TeletextGuideChannel {
            id: 1,
            number: "1",
            name: "Synthetic guide channel",
            programmes: &[],
        }];
        let context = guide(if guide_bearing { &guide_channels } else { &[] });
        let candidate = prepare_effective_channel_teletext_document_candidate(
            &fixture.state.application,
            1,
            Some(ADDON),
            &context,
            channel_documents::policy(),
        )
        .await
        .unwrap();
        let value = originals::saved_design(
            originals::save(&fixture, candidate, None, designs::budget())
                .await
                .unwrap(),
        );
        let calls = std::fs::read(fixture.root.path().join("authoring-calls")).unwrap();
        let error = attempt(&fixture, target(&value, None)).await.unwrap_err();
        if guide_bearing {
            assert!(matches!(
                error,
                NativeEffectiveTeletextPublicationError::GuideFreshnessRequired
            ));
        } else {
            assert!(matches!(
                error,
                NativeEffectiveTeletextPublicationError::Design(_)
            ));
        }
        assert!(fixture
            .state
            .application
            .effective_teletext_publication_request(1)
            .await
            .unwrap()
            .is_none());
        assert!(fixture
            .state
            .application
            .effective_teletext_accepted_generation(1)
            .await
            .unwrap()
            .is_none());
        assert_eq!(
            std::fs::read(fixture.root.path().join("authoring-calls")).unwrap(),
            calls
        );
        assert!(fixture.playout.try_recv().is_err());
    }
}

#[tokio::test]
async fn scoped_request_profile_drift_and_expired_deadline_never_invoke_or_write_pending() {
    let (mut fixture, _, day, _) = originals::installed(true).await;
    effective::publish(&fixture, 1, Some(&day)).await;
    let value = originals::saved_design(
        originals::save(
            &fixture,
            effective::candidate(&fixture, 1, Some(ADDON))
                .await
                .unwrap(),
            None,
            designs::budget(),
        )
        .await
        .unwrap(),
    );
    let calls = std::fs::read(fixture.root.path().join("authoring-calls")).unwrap();
    assert!(request_effective_original_teletext_design_publication(
        &fixture.state.application,
        target(&value, None),
        designs::budget(),
        channel_documents::policy(),
        std::time::Instant::now()
    )
    .await
    .is_err());
    sqlx::query(r#"UPDATE "TeletextProfile" SET "Revision"="Revision"+1 WHERE "Id"=?"#)
        .bind(day.id)
        .execute(&fixture.state.pool)
        .await
        .unwrap();
    assert!(attempt(&fixture, target(&value, None)).await.is_err());
    assert!(fixture
        .state
        .application
        .effective_teletext_publication_request(1)
        .await
        .unwrap()
        .is_none());
    assert!(fixture
        .state
        .application
        .effective_teletext_accepted_generation(1)
        .await
        .unwrap()
        .is_none());
    assert_eq!(
        std::fs::read(fixture.root.path().join("authoring-calls")).unwrap(),
        calls
    );
    assert!(fixture.playout.try_recv().is_err());
}
