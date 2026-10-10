use super::*;

#[tokio::test]
async fn closed_scope_rejects_registration_without_an_owner() {
    let scope = LocalProbeCleanupScope::new();
    scope.close();
    assert!(matches!(scope.register(), Err(AdmissionError::Closed)));
    assert_eq!(scope.cleanup_status(), Ok(true));
    assert_eq!(scope.wait_cleanup().await, Ok(()));
}

#[tokio::test]
async fn owner_drop_records_sticky_uncertainty_and_fences_later_probes() {
    let scope = LocalProbeCleanupScope::new();
    drop(scope.register().unwrap());
    assert!(matches!(scope.register(), Err(AdmissionError::Unconfirmed)));
    assert_eq!(
        scope.wait_cleanup().await,
        Err(LocalProbeCleanupError::CleanupUnconfirmed)
    );
}

#[tokio::test]
async fn confirmed_probe_failure_does_not_make_cleanup_uncertain() {
    let scope = LocalProbeCleanupScope::new();
    let owner = scope.register().unwrap();
    assert_eq!(scope.cleanup_status(), Ok(false));
    owner.complete(true);
    assert_eq!(scope.cleanup_status(), Ok(true));
    scope.register().unwrap().complete(true);
}

#[tokio::test]
async fn explicit_uncertain_completion_closes_and_cancels_other_owners() {
    let scope = LocalProbeCleanupScope::new();
    let uncertain = scope.register().unwrap();
    let remaining = scope.register().unwrap();
    uncertain.complete(false);
    remaining.cancelled().await;
    assert_eq!(
        scope.cleanup_status(),
        Err(LocalProbeCleanupError::CleanupUnconfirmed)
    );
    remaining.complete(true);
    assert_eq!(
        scope.wait_cleanup().await,
        Err(LocalProbeCleanupError::CleanupUnconfirmed)
    );
}

#[tokio::test(start_paused = true)]
async fn an_open_scope_does_not_add_a_total_probe_timeout() {
    let scope = LocalProbeCleanupScope::new();
    let owner = scope.register().unwrap();
    let wait = scope.wait_cleanup();
    tokio::pin!(wait);
    tokio::select! {
        result = &mut wait => panic!("live open owner cannot finish: {result:?}"),
        () = tokio::time::sleep(Duration::from_mins(2)) => {},
    }
    owner.complete(true);
    assert_eq!(wait.await, Ok(()));
}

#[tokio::test(start_paused = true)]
async fn closed_drain_timeout_does_not_drop_a_live_owner_or_forget_uncertainty() {
    let scope = LocalProbeCleanupScope::new();
    let owner = scope.register().unwrap();
    scope.close();
    assert_eq!(
        scope.wait_cleanup().await,
        Err(LocalProbeCleanupError::CleanupUnconfirmed)
    );
    assert_eq!(scope.state().active, 1);
    owner.complete(true);
    assert_eq!(scope.state().active, 0);
    assert_eq!(
        scope.cleanup_status(),
        Err(LocalProbeCleanupError::CleanupUnconfirmed)
    );
}

#[tokio::test(start_paused = true)]
async fn repeated_close_does_not_extend_the_original_drain_budget() {
    let scope = LocalProbeCleanupScope::new();
    let owner = scope.register().unwrap();
    scope.close();
    let closed_at = scope.state().closed_at.unwrap();
    tokio::time::advance(Duration::from_secs(4)).await;
    scope.close();
    assert_eq!(scope.state().closed_at, Some(closed_at));
    assert_eq!(
        scope.wait_cleanup().await,
        Err(LocalProbeCleanupError::CleanupUnconfirmed)
    );
    assert_eq!(Instant::now(), closed_at + CLOSED_DRAIN_TIMEOUT);
    owner.complete(true);
}

#[tokio::test]
async fn cancellation_and_completion_before_waiter_registration_are_level_triggered() {
    let scope = LocalProbeCleanupScope::new();
    let owner = scope.register().unwrap();
    scope.close();
    owner.cancelled().await;
    owner.complete(true);
    assert_eq!(scope.wait_cleanup().await, Ok(()));
}

#[tokio::test]
async fn completion_after_a_waiter_starts_does_not_lose_the_wake() {
    let scope = LocalProbeCleanupScope::new();
    let owner = scope.register().unwrap();
    let waiting_scope = scope.clone();
    let waiter = tokio::spawn(async move { waiting_scope.wait_cleanup().await });
    tokio::task::yield_now().await;
    scope.close();
    owner.complete(true);
    assert_eq!(waiter.await.unwrap(), Ok(()));
}

#[tokio::test]
async fn registrations_are_bounded_without_poisoning_a_confirmed_scope() {
    let scope = LocalProbeCleanupScope::new();
    let owners: Vec<_> = (0..MAX_ACTIVE_OWNERS)
        .map(|_| scope.register().unwrap())
        .collect();
    assert!(matches!(scope.register(), Err(AdmissionError::Capacity)));
    for owner in owners {
        owner.complete(true);
    }
    assert_eq!(scope.wait_cleanup().await, Ok(()));
}

#[tokio::test]
async fn spawned_tasks_require_explicit_scope_propagation() {
    let scope = LocalProbeCleanupScope::new();
    scope
        .scope(async {
            assert!(LocalProbeCleanupScope::current().is_some());
            assert!(
                tokio::spawn(async { LocalProbeCleanupScope::current().is_none() })
                    .await
                    .unwrap()
            );
            let child_scope = scope.clone();
            tokio::spawn(async move {
                child_scope
                    .scope(async {
                        register_current().unwrap().unwrap().complete(true);
                    })
                    .await;
            })
            .await
            .unwrap();
        })
        .await;
    assert_eq!(scope.wait_cleanup().await, Ok(()));
}

#[tokio::test]
async fn nesting_uses_the_innermost_scope_and_restores_the_outer_scope() {
    let outer = LocalProbeCleanupScope::new();
    let inner = LocalProbeCleanupScope::new();
    outer
        .scope(async {
            let outer_owner = register_current().unwrap().unwrap();
            inner
                .scope(async {
                    let inner_owner = register_current().unwrap().unwrap();
                    assert_eq!(outer.state().active, 1);
                    assert_eq!(inner.state().active, 1);
                    inner_owner.complete(true);
                })
                .await;
            assert!(Arc::ptr_eq(
                &LocalProbeCleanupScope::current().unwrap().inner,
                &outer.inner
            ));
            outer
                .scope(async {
                    register_current().unwrap().unwrap().complete(true);
                })
                .await;
            outer_owner.complete(true);
        })
        .await;
    assert!(LocalProbeCleanupScope::current().is_none());
    assert_eq!(outer.wait_cleanup().await, Ok(()));
    assert_eq!(inner.wait_cleanup().await, Ok(()));
}
