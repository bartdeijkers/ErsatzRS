//! Exact scoped acceptance/recovery; actual route/client proof is a separate gate.

use super::*;
use super::{
    authoring_metadata, channel_documents, designs, effective_channel_documents as effective,
    effective_designs as originals, effective_publications as pending, generations,
    scripted_controls,
};
use ersatzrs_core::repositories::{
    TeletextGenerationMutation as M, TeletextGenerationRejection as R,
    TELETEXT_EFFECTIVE_GENERATION_ARTIFACT_VERSION,
};
use ersatzrs_domain::TeletextPublicationRequest;
use ersatzrs_web::host::{
    teletext_effective_generations::{
        accept_effective_original_teletext_generation,
        reload_accepted_effective_teletext_generation, AcceptedEffectiveTeletextOutputCandidate,
    },
    teletext_effective_publications::request_effective_original_teletext_design_publication,
    teletext_generations::{NativeTeletextGenerationError, TeletextGenerationTarget},
};

pub(super) async fn accept(
    fixture: &Fixture,
    request: &TeletextPublicationRequest,
    head: i64,
    previous: Option<i64>,
) -> Result<M<AcceptedEffectiveTeletextOutputCandidate>, NativeTeletextGenerationError> {
    accept_effective_original_teletext_generation(
        &fixture.state.application,
        TeletextGenerationTarget {
            request: request.clone(),
            expected_design_head: head,
            expected_generation_id: previous,
        },
        designs::budget(),
        channel_documents::policy(),
        designs::deadline(),
    )
    .await
}
pub(super) fn accepted(
    value: M<AcceptedEffectiveTeletextOutputCandidate>,
) -> AcceptedEffectiveTeletextOutputCandidate {
    match value {
        M::Accepted(value) => value,
        M::Rejected(reason) => panic!("unexpected scoped acceptance rejection {reason:?}"),
    }
}
pub(super) async fn recover(
    fixture: &Fixture,
    channel: i64,
) -> Result<Option<AcceptedEffectiveTeletextOutputCandidate>, NativeTeletextGenerationError> {
    reload_accepted_effective_teletext_generation(
        &fixture.state.application,
        channel,
        designs::budget(),
        channel_documents::policy(),
        designs::deadline(),
    )
    .await
}
fn same_output(
    actual: &AcceptedEffectiveTeletextOutputCandidate,
    expected: &AcceptedEffectiveTeletextOutputCandidate,
) {
    assert_eq!(actual.generation(), expected.generation());
    assert_eq!(actual.selection(), expected.selection());
    assert_eq!(actual.service().is_some(), expected.service().is_some());
    if let (Some(actual), Some(expected)) = (actual.service(), expected.service()) {
        assert_eq!(designs::units(actual), designs::units(expected));
        assert_eq!(actual.magazine_units(), expected.magazine_units());
        assert_eq!(
            originals::pixels(actual, 123, 16),
            originals::pixels(expected, 123, 16)
        );
    }
}

#[tokio::test]
async fn actual_lua_two_themes_accept_reopen_and_recover_exact_scoped_output_without_replay() {
    let (mut fixture, base, day, night) = originals::installed(true).await;
    effective::publish(&fixture, 1, Some(&day)).await;
    effective::publish(&fixture, 2, Some(&night)).await;
    let first = originals::saved_design(
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
    let second = originals::saved_design(
        originals::save(
            &fixture,
            effective::candidate(&fixture, 2, Some(ADDON))
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
    let p1 = pending::request(&fixture, &first, None).await;
    let p2 = pending::request(&fixture, &second, None).await;
    assert!(recover(&fixture, 1).await.unwrap().is_none());
    let a = accepted(accept(&fixture, &p1, 1, None).await.unwrap());
    let b = accepted(accept(&fixture, &p2, 1, None).await.unwrap());
    assert_eq!(
        a.generation().artifact_version,
        TELETEXT_EFFECTIVE_GENERATION_ARTIFACT_VERSION
    );
    assert_ne!(
        a.generation().artifact_digest,
        b.generation().artifact_digest
    );
    assert_ne!(
        originals::pixels(a.service().unwrap(), 123, 16),
        originals::pixels(b.service().unwrap(), 123, 16)
    );
    assert_eq!(a.selection(), first.candidate().selection());
    assert_eq!(b.selection(), second.candidate().selection());
    // Shadowed operator base is independent of both selected Scripted targets.
    sqlx::query(r#"UPDATE "TeletextProfile" SET "IsDeleted"=1 WHERE "Id"=?"#)
        .bind(base.id)
        .execute(&fixture.state.pool)
        .await
        .unwrap();
    sqlx::query(r#"UPDATE "ChannelTeletextBinding" SET "Enabled"=0,"Revision"="Revision"+1"#)
        .execute(&fixture.state.pool)
        .await
        .unwrap();
    fixture = fixture.reopen().await;
    same_output(&recover(&fixture, 1).await.unwrap().unwrap(), &a);
    same_output(&recover(&fixture, 2).await.unwrap().unwrap(), &b);
    assert_eq!(
        fixture
            .state
            .application
            .effective_teletext_publication_request(1)
            .await
            .unwrap(),
        Some(p1)
    );
    assert_eq!(
        fixture
            .state
            .application
            .effective_teletext_publication_request(2)
            .await
            .unwrap(),
        Some(p2)
    );
    assert!(fixture
        .state
        .application
        .accepted_teletext_generation(1)
        .await
        .unwrap()
        .is_none());
    assert!(fixture
        .state
        .application
        .accepted_teletext_generation(2)
        .await
        .unwrap()
        .is_none());
    assert_eq!(
        std::fs::read(fixture.root.path().join("authoring-calls")).unwrap(),
        calls
    );
    assert_eq!(designs::config(&fixture).await, before);
    assert!(fixture.playout.try_recv().is_err());
}

#[tokio::test]
async fn accepted_scoped_history_is_independent_of_new_pending_and_pointer_cas() {
    let (mut fixture, _, day, _) = originals::installed(true).await;
    effective::publish(&fixture, 1, Some(&day)).await;
    let first = originals::saved_design(
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
    let p1 = pending::request(&fixture, &first, None).await;
    assert!(matches!(
        accept(&fixture, &p1, 1, Some(999)).await.unwrap(),
        M::Rejected(R::GenerationConflict)
    ));
    let old = accepted(accept(&fixture, &p1, 1, None).await.unwrap());
    assert!(matches!(
        accept(&fixture, &p1, 1, Some(old.generation().id))
            .await
            .unwrap(),
        M::Rejected(R::AlreadyAccepted)
    ));
    let second = originals::saved_design(
        originals::save(
            &fixture,
            effective::candidate(&fixture, 1, Some(ADDON))
                .await
                .unwrap(),
            Some((first.head().id, 1)),
            designs::budget(),
        )
        .await
        .unwrap(),
    );
    let p2 = pending::request(&fixture, &second, Some(p1.id)).await;
    let calls = std::fs::read(fixture.root.path().join("authoring-calls")).unwrap();
    same_output(&recover(&fixture, 1).await.unwrap().unwrap(), &old);
    assert_eq!(
        fixture
            .state
            .application
            .effective_teletext_publication_request(1)
            .await
            .unwrap(),
        Some(p2.clone())
    );
    assert!(matches!(
        accept(&fixture, &p2, 2, None).await.unwrap(),
        M::Rejected(R::GenerationConflict)
    ));
    same_output(&recover(&fixture, 1).await.unwrap().unwrap(), &old);
    let current = accepted(
        accept(&fixture, &p2, 2, Some(old.generation().id))
            .await
            .unwrap(),
    );
    assert_ne!(current.generation().id, old.generation().id);
    same_output(&recover(&fixture, 1).await.unwrap().unwrap(), &current);
    assert_eq!(
        std::fs::read(fixture.root.path().join("authoring-calls")).unwrap(),
        calls
    );
    assert!(fixture.playout.try_recv().is_err());
}

#[tokio::test]
async fn actual_lua_off_accepts_absence_and_preserves_separate_existing_base_history() {
    let (mut fixture, _, _, _) = originals::installed(true).await;
    let base = designs::save(
        &fixture,
        authoring_metadata::candidate(&fixture, 1).await,
        None,
    )
    .await;
    let base_pending = generations::request(&fixture, &base, None).await;
    let base_accepted =
        generations::accepted(generations::accept(&fixture, &base_pending, 1, None).await);
    let interval = effective::publish(&fixture, 1, None).await;
    let value = originals::saved_design(
        originals::save(
            &fixture,
            effective::candidate(&fixture, 1, None).await.unwrap(),
            None,
            designs::budget(),
        )
        .await
        .unwrap(),
    );
    let calls = std::fs::read(fixture.root.path().join("authoring-calls")).unwrap();
    let p = pending::request(&fixture, &value, None).await;
    let selected = accepted(accept(&fixture, &p, 1, None).await.unwrap());
    assert!(selected.service().is_none());
    assert_eq!(selected.selection().valid_until, Some(interval.finish));
    assert!(!selected.selection().bindings[0].enabled());
    same_output(&recover(&fixture, 1).await.unwrap().unwrap(), &selected);
    assert_eq!(
        fixture
            .state
            .application
            .accepted_teletext_generation(1)
            .await
            .unwrap(),
        Some(base_accepted.generation().clone())
    );
    assert_eq!(
        fixture
            .state
            .application
            .teletext_publication_request(1)
            .await
            .unwrap(),
        Some(base_pending)
    );
    assert_eq!(selected.generation().artifact_version, 2);
    assert_ne!(
        selected.generation().artifact_digest,
        base_accepted.generation().artifact_digest
    );
    assert_eq!(
        std::fs::read(fixture.root.path().join("authoring-calls")).unwrap(),
        calls
    );
    assert!(fixture.playout.try_recv().is_err());
}

#[tokio::test]
async fn scoped_original_expiry_cannot_be_revived_by_acceptance_or_recovery() {
    let (mut fixture, _, day, _) = originals::installed(true).await;
    let start = ersatzrs_core::scheduling::truncate_to_persisted_precision(
        Utc::now() - chrono::Duration::seconds(55),
    );
    let result=scripted_controls::build_at(&fixture,&format!("assert(teletext_profile('{ADDON}', {}).ok)\nassert(teletext_enabled('{ADDON}',true).ok)\nadd_count('clip',1)",day.id),1,true,start).await.unwrap();
    let finish = result.teletext_intervals[0].finish;
    scripted_controls::publish_result(&fixture, &result).await;
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
    let p = pending::request(&fixture, &value, None).await;
    let old = accepted(accept(&fixture, &p, 1, None).await.unwrap());
    let calls = std::fs::read(fixture.root.path().join("authoring-calls")).unwrap();
    tokio::time::sleep(
        (finish - Utc::now()).to_std().unwrap_or_default() + Duration::from_millis(1),
    )
    .await;
    assert!(Utc::now() >= finish);
    assert!(accept(&fixture, &p, 1, Some(old.generation().id))
        .await
        .is_err());
    assert!(recover(&fixture, 1).await.is_err());
    assert!(request_effective_original_teletext_design_publication(
        &fixture.state.application,
        pending::target(&value, Some(p.id)),
        designs::budget(),
        channel_documents::policy(),
        designs::deadline()
    )
    .await
    .is_err());
    assert_eq!(
        fixture
            .state
            .application
            .effective_teletext_accepted_generation(1)
            .await
            .unwrap(),
        Some(old.generation().clone())
    );
    let historical = originals::reload(&fixture, value.head().id, 1).await;
    assert_eq!(historical.selection(), old.selection());
    assert_eq!(
        std::fs::read(fixture.root.path().join("authoring-calls")).unwrap(),
        calls
    );
    assert!(fixture.playout.try_recv().is_err());
}

#[tokio::test]
async fn scoped_artifact_corruption_deadline_and_profile_drift_never_consume_pending_or_base() {
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
    let p = pending::request(&fixture, &value, None).await;
    let old = accepted(accept(&fixture, &p, 1, None).await.unwrap());
    let calls = std::fs::read(fixture.root.path().join("authoring-calls")).unwrap();
    assert!(reload_accepted_effective_teletext_generation(
        &fixture.state.application,
        1,
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
    assert!(recover(&fixture, 1).await.is_err());
    assert_eq!(
        fixture
            .state
            .application
            .effective_teletext_accepted_generation(1)
            .await
            .unwrap(),
        Some(old.generation().clone())
    );
    sqlx::query(r#"UPDATE "TeletextProfile" SET "Revision"="Revision"-1 WHERE "Id"=?"#)
        .bind(day.id)
        .execute(&fixture.state.pool)
        .await
        .unwrap();
    sqlx::query(r#"DROP TRIGGER "TR_TeletextAcceptedGeneration_ImmutableUpdate""#)
        .execute(&fixture.state.pool)
        .await
        .unwrap();
    sqlx::query(r#"UPDATE "TeletextAcceptedGeneration" SET "ArtifactDigest"=? WHERE "Id"=?"#)
        .bind("0".repeat(64))
        .bind(old.generation().id)
        .execute(&fixture.state.pool)
        .await
        .unwrap();
    assert!(matches!(
        recover(&fixture, 1).await,
        Err(NativeTeletextGenerationError::ArtifactMismatch)
    ));
    assert_eq!(
        fixture
            .state
            .application
            .effective_teletext_publication_request(1)
            .await
            .unwrap(),
        Some(p)
    );
    assert!(fixture
        .state
        .application
        .accepted_teletext_generation(1)
        .await
        .unwrap()
        .is_none());
    assert_eq!(
        std::fs::read(fixture.root.path().join("authoring-calls")).unwrap(),
        calls
    );
    assert!(fixture.playout.try_recv().is_err());
}
