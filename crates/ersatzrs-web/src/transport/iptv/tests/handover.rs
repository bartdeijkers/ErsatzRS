// Exact current-selection permission bridges one bounded recovery, never scope.

use super::*;

fn checked(id: i64, context: u8) -> Candidate {
    let mut value = candidate(id, 9);
    value.identity.artifact_version = TELETEXT_LIFECYCLE_GENERATION_ARTIFACT_VERSION;
    value.service = Arc::new(
        crate::transport::iptv::teletext_binding::authored_tests::authored_service_with_counter(3)
            .with_resource_lifecycle_variants(std::time::Instant::now() + Duration::from_secs(30))
            .unwrap(),
    );
    value.selection_context = context;
    value
}

fn backend_with_successor(
    initial: Candidate,
    successor: Candidate,
) -> (
    Backend,
    MonitorState<Candidate, u64>,
    watch::Sender<Option<TeletextOutputUpdate>>,
    TeletextOutputControl,
) {
    let backend = Backend::new(Some(successor));
    backend.store.lock().unwrap().handover = Ok(true);
    let (updates, control, _) = teletext_output_control_channel();
    (backend, MonitorState::new(Some(initial)), updates, control)
}

fn assert_withdrawal(updates: &watch::Sender<Option<TeletextOutputUpdate>>, sequence: u64) {
    let current = updates.borrow();
    let current = current.as_ref().unwrap();
    assert_eq!(current.sequence(), sequence);
    // The action is intentionally private to the pump; its redacted Debug is
    // the same existing public observation used by ordinary adapter tests.
    assert!(format!("{current:?}").contains("replacement: false"));
}

#[tokio::test(start_paused = true)]
async fn same_selection_checked_content_replacement_emits_one_replace_without_withdrawal() {
    let (backend, mut state, updates, _control) =
        backend_with_successor(checked(1, 1), checked(2, 1));
    assert!(reconcile(&backend, &mut state, &updates, 9, deadline()).await);
    assert!(state.active);
    assert_eq!(state.cached.as_ref().unwrap().identity.id, 2);
    assert_eq!(updates.borrow().as_ref().unwrap().sequence(), 1);
    assert!(format!("{:?}", updates.borrow().as_ref().unwrap()).contains("replacement: true"));
    assert_eq!(backend.handover_checks.load(Ordering::SeqCst), 1);
    assert_eq!(*backend.confirmations.lock().unwrap(), vec![2]);
    assert!(
        !state.handover_confirmed,
        "a proposed replacement is not a sink acknowledgement"
    );
    assert!(state.observe_rejection(
        Some((1, TeletextOutputTransitionPhase::Applied)),
        &updates,
        9
    ));
    assert!(state.handover_confirmed);
    backend.store.lock().unwrap().accepted = Some(checked(3, 1));
    assert!(reconcile(&backend, &mut state, &updates, 9, deadline()).await);
    assert_eq!(updates.borrow().as_ref().unwrap().sequence(), 2);
}

fn checked_guide(context: u64) -> Candidate {
    let mut candidate = checked(1, 1);
    candidate.identity.artifact_version = TELETEXT_GUIDE_GENERATION_ARTIFACT_VERSION;
    candidate.guide_context = Some(context);
    candidate
}

#[tokio::test(start_paused = true)]
async fn same_manual_root_new_guide_snapshot_uses_one_confirmed_atomic_replacement() {
    let initial = checked_guide(10);
    let root = initial.identity.clone();
    let mut successor = checked_guide(11);
    successor.service = Arc::new(
        crate::transport::iptv::teletext_binding::authored_tests::authored_service_with_counter(4)
            .with_resource_lifecycle_variants(std::time::Instant::now() + Duration::from_secs(30))
            .unwrap(),
    );
    let (backend, mut state, updates, _control) = backend_with_successor(initial, successor);
    assert!(reconcile(&backend, &mut state, &updates, 9, deadline()).await);
    assert_eq!(state.cached.as_ref().unwrap().identity, root);
    assert_eq!(state.cached.as_ref().unwrap().guide_context, Some(11));
    assert_eq!(state.proposal_context, Some(11));
    assert_eq!(updates.borrow().as_ref().unwrap().sequence(), 1);
    assert!(format!("{:?}", updates.borrow().as_ref().unwrap()).contains("replacement: true"));
    assert_eq!(backend.handover_checks.load(Ordering::SeqCst), 1);
    assert_eq!(*backend.confirmations.lock().unwrap(), vec![root.id]);
    assert!(
        !state.handover_confirmed,
        "proposal is not an applied acknowledgement"
    );
}

#[tokio::test(start_paused = true)]
async fn guide_snapshot_handover_keeps_selection_off_expiry_and_final_current_failure_fences() {
    for mode in 0..5 {
        let mut initial = checked_guide(10);
        let mut successor = checked_guide(11);
        if mode == 0 {
            successor.selection_context = 2;
        }
        if mode == 1 {
            successor.service_present = false;
        }
        if mode == 4 {
            initial.original_transition = Some(Instant::now());
        }
        let (backend, mut state, updates, _control) = backend_with_successor(initial, successor);
        match mode {
            2 => backend.store.lock().unwrap().eligible = false,
            3 => backend.store.lock().unwrap().recovery = RecoveryScenario::Unavailable,
            _ => {}
        }
        assert!(reconcile(&backend, &mut state, &updates, 9, deadline()).await);
        if mode == 0 {
            // A different selection must first withdraw, then independently
            // validate/reannounce the new profile through normal recovery.
            assert_eq!(updates.borrow().as_ref().unwrap().sequence(), 2);
        } else if mode == 1 {
            assert!(!state.active);
            // Preserve the separate identified Off receipt after withdrawal.
            assert_withdrawal(&updates, 2);
        } else {
            assert!(!state.active);
            assert_withdrawal(&updates, 1);
        }
    }
}

#[tokio::test(start_paused = true)]
async fn changed_selection_and_legacy_artifacts_withdraw_before_blocked_recovery() {
    for mode in 0..7 {
        let mut initial = if mode == 1 {
            candidate(1, 9)
        } else {
            checked(1, 1)
        };
        let mut successor = if mode == 1 {
            candidate(2, 9)
        } else {
            checked(2, if mode == 0 { 2 } else { 1 })
        };
        match mode {
            4 => {
                successor.identity.artifact_version =
                    TELETEXT_EFFECTIVE_LIFECYCLE_GENERATION_ARTIFACT_VERSION;
            }
            5 => initial.service = candidate(1, 9).service,
            6 => successor.identity.artifact_version = 1,
            _ => {}
        }
        let (backend, mut state, updates, _control) = backend_with_successor(initial, successor);
        if mode == 2 {
            backend.store.lock().unwrap().handover = Ok(false);
        }
        if mode == 3 {
            backend.store.lock().unwrap().handover = Err(ReadFailure);
        }
        let release = Arc::new(Notify::new());
        backend.store.lock().unwrap().recovery_release = Some(Arc::clone(&release));
        let work_backend = backend.clone();
        let work_updates = updates.clone();
        let task = tokio::spawn(async move {
            reconcile(&work_backend, &mut state, &work_updates, 9, deadline()).await;
            state
        });
        backend.recovery_entered.notified().await;
        assert_withdrawal(&updates, 1);
        if matches!(mode, 1 | 4..=6) {
            assert_eq!(backend.handover_checks.load(Ordering::SeqCst), 0);
        }
        release.notify_one();
        task.await.unwrap();
    }
}

#[tokio::test(start_paused = true)]
async fn authorized_successor_cannot_be_replaced_by_a_later_recovered_identity() {
    let (backend, mut state, updates, _control) =
        backend_with_successor(checked(1, 1), checked(2, 1));
    backend.store.lock().unwrap().after_handover = Some(checked(3, 1));
    assert!(reconcile(&backend, &mut state, &updates, 9, deadline()).await);
    assert_withdrawal(&updates, 1);
    assert!(!state.active);
    assert_eq!(state.cached.as_ref().unwrap().identity.id, 1);
    assert!(backend.confirmations.lock().unwrap().is_empty());
}

#[tokio::test(start_paused = true)]
async fn authorized_recovery_absence_error_or_failed_final_confirmation_withdraws() {
    for failure in 0..3 {
        let (backend, mut state, updates, _control) =
            backend_with_successor(checked(1, 1), checked(2, 1));
        {
            let mut store = backend.store.lock().unwrap();
            match failure {
                0 => store.recovery = RecoveryScenario::Absent,
                1 => store.recovery = RecoveryScenario::Unavailable,
                _ => store.eligible = false,
            }
        }
        assert!(reconcile(&backend, &mut state, &updates, 9, deadline()).await);
        assert_withdrawal(&updates, 1);
        assert!(!state.active);
        assert_eq!(state.cached.as_ref().unwrap().identity.id, 1);
    }
}

#[tokio::test(start_paused = true)]
async fn original_expiry_and_existing_operation_deadline_bound_authorized_recovery() {
    for original_first in [false, true] {
        let mut initial = checked(1, 1);
        initial.original_transition =
            Some(Instant::now() + Duration::from_secs(if original_first { 1 } else { 5 }));
        let (backend, mut state, updates, _control) =
            backend_with_successor(initial, checked(2, 1));
        backend.store.lock().unwrap().recovery_release = Some(Arc::new(Notify::new()));
        let operation = Instant::now() + Duration::from_secs(if original_first { 5 } else { 1 });
        let work_backend = backend.clone();
        let work_updates = updates.clone();
        let task = tokio::spawn(async move {
            reconcile(&work_backend, &mut state, &work_updates, 9, operation).await;
            state
        });
        backend.recovery_entered.notified().await;
        assert!(
            updates.borrow().is_none(),
            "authorized in-budget handover need not withdraw preemptively"
        );
        tokio::time::advance(Duration::from_secs(1)).await;
        let state = task.await.unwrap();
        assert_withdrawal(&updates, 1);
        assert!(!state.active);
        assert_eq!(backend.recovering.load(Ordering::SeqCst), 0);
    }
}

#[tokio::test(start_paused = true)]
async fn withdrawn_expired_cache_does_not_block_recovery_under_the_new_boundary() {
    for future_boundary in [false, true] {
        let mut initial = checked(1, 1);
        initial.original_transition = Some(Instant::now() + Duration::from_secs(1));
        let (backend, mut state, updates, _control) =
            backend_with_successor(initial, checked(2, 1));
        backend.store.lock().unwrap().recovery_release = Some(Arc::new(Notify::new()));
        let work_backend = backend.clone();
        let work_updates = updates.clone();
        let task = tokio::spawn(async move {
            reconcile(&work_backend, &mut state, &work_updates, 9, deadline()).await;
            state
        });
        backend.recovery_entered.notified().await;
        // The current read has advanced the backend observation to B's future
        // boundary (or unscoped B). A still expires at its original boundary.
        *backend.transition.lock().unwrap() =
            future_boundary.then(|| Instant::now() + Duration::from_secs(10));
        tokio::time::advance(Duration::from_secs(1)).await;
        let mut state = task.await.unwrap();
        assert_withdrawal(&updates, 1);
        assert!(!state.active);
        assert_eq!(state.cached.as_ref().unwrap().identity.id, 1);

        backend.store.lock().unwrap().recovery_release = None;
        assert!(reconcile(&backend, &mut state, &updates, 9, deadline()).await);
        assert!(state.active);
        assert_eq!(state.cached.as_ref().unwrap().identity.id, 2);
        assert_eq!(updates.borrow().as_ref().unwrap().sequence(), 2);
        assert_eq!(backend.recoveries.load(Ordering::SeqCst), 2);
    }
}

#[tokio::test(start_paused = true)]
async fn off_and_receiver_close_never_retain_a_checked_handover() {
    let (backend, mut state, updates, control) =
        backend_with_successor(checked(1, 1), checked(2, 1));
    backend.store.lock().unwrap().accepted = None;
    assert!(reconcile(&backend, &mut state, &updates, 9, deadline()).await);
    assert_withdrawal(&updates, 1);
    assert_eq!(backend.handover_checks.load(Ordering::SeqCst), 0);
    assert_eq!(backend.recoveries.load(Ordering::SeqCst), 0);
    drop(control);

    let (backend, mut state, updates, control) =
        backend_with_successor(checked(1, 1), checked(2, 1));
    backend.store.lock().unwrap().recovery_release = Some(Arc::new(Notify::new()));
    let work_backend = backend.clone();
    let work_updates = updates.clone();
    let task = tokio::spawn(async move {
        let result = reconcile(&work_backend, &mut state, &work_updates, 9, deadline()).await;
        (result, state)
    });
    backend.recovery_entered.notified().await;
    drop(control);
    let (result, state) = task.await.unwrap();
    assert!(!result);
    assert!(!state.active);
    assert_eq!(backend.recovering.load(Ordering::SeqCst), 0);
}

#[tokio::test(start_paused = true)]
async fn waiting_cached_context_and_obsolete_applied_cannot_authorize_actual_old_output() {
    for obsolete_applied in [false, true] {
        let (backend, mut state, updates, _control) =
            backend_with_successor(checked(1, 1), checked(2, 2));
        assert!(reconcile(&backend, &mut state, &updates, 9, deadline()).await);
        assert_eq!(updates.borrow().as_ref().unwrap().sequence(), 2); // withdraw A, propose B
                                                                      // Retire the completed B recovery signal before waiting for blocked C.
                                                                      // Otherwise its stored permit can satisfy C's barrier before C runs.
        backend.recovery_entered.notified().await;
        assert!(!state.handover_confirmed);
        assert!(state.observe_rejection(
            Some((2, TeletextOutputTransitionPhase::Waiting)),
            &updates,
            9
        ));
        assert!(!state.handover_confirmed);
        if obsolete_applied {
            assert!(state.observe_rejection(
                Some((1, TeletextOutputTransitionPhase::Applied)),
                &updates,
                9
            ));
            assert!(!state.handover_confirmed);
        }
        backend.store.lock().unwrap().accepted = Some(checked(3, 2));
        let release = Arc::new(Notify::new());
        backend.store.lock().unwrap().recovery_release = Some(Arc::clone(&release));
        let work_backend = backend.clone();
        let work_updates = updates.clone();
        let task = tokio::spawn(async move {
            reconcile(&work_backend, &mut state, &work_updates, 9, deadline()).await;
            state
        });
        backend.recovery_entered.notified().await;
        // B/Y is only Waiting: A/X may still be actual. C/Y cannot borrow
        // B's cached context to keep A running during its recovery.
        assert_withdrawal(&updates, 3);
        assert_eq!(backend.handover_checks.load(Ordering::SeqCst), 1);
        release.notify_one();
        task.await.unwrap();
    }
}

#[tokio::test(start_paused = true)]
async fn rejected_checked_replacement_withdraws_once_and_obsolete_applied_stays_inert() {
    use crate::transport::iptv::teletext_binding::TeletextOutputTransitionRejection;
    let (backend, mut state, updates, _control) =
        backend_with_successor(checked(1, 1), checked(2, 1));
    assert!(reconcile(&backend, &mut state, &updates, 9, deadline()).await);
    assert!(state.observe_rejection(
        Some((
            1,
            TeletextOutputTransitionPhase::Rejected(
                TeletextOutputTransitionRejection::CacheReconciliation
            )
        )),
        &updates,
        9
    ));
    assert_withdrawal(&updates, 2);
    assert!(!state.handover_confirmed);
    assert!(state.observe_rejection(
        Some((1, TeletextOutputTransitionPhase::Applied)),
        &updates,
        9
    ));
    assert!(!state.handover_confirmed);
    assert!(reconcile(&backend, &mut state, &updates, 9, deadline()).await);
    assert_eq!(updates.borrow().as_ref().unwrap().sequence(), 2);
    assert_eq!(backend.recoveries.load(Ordering::SeqCst), 1);
}
