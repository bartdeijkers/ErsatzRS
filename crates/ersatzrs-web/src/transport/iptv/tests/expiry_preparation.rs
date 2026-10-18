// Preparation is optional work, never a renewal or selection proposal.

use super::*;

#[tokio::test(start_paused = true)]
async fn blocked_optional_preparation_cannot_extend_original_scope_or_publish_early_base() {
    let mut initial = candidate(1, 9);
    let original_boundary = Instant::now() + Duration::from_secs(1);
    initial.original_transition = Some(original_boundary);
    let backend = Backend::new(Some(initial.clone()));
    *backend.transition.lock().unwrap() = Some(original_boundary);
    *backend.preparation_release.lock().unwrap() = Some(Arc::new(Notify::new()));
    let (updates, control, _) = teletext_output_control_channel();
    let mut observed = updates.subscribe();
    let owner = tokio::spawn(run_monitor(
        backend.clone(),
        9,
        TeletextAcceptedUpdatePolicy::new(Duration::from_secs(10), Duration::from_secs(20))
            .unwrap(),
        Some(initial),
        updates,
        || None,
    ));
    backend.preparation_entered.notified().await;
    assert!(
        observed.borrow().is_none(),
        "preparation is not publication"
    );
    backend.store.lock().unwrap().read_release = Some(Arc::new(Notify::new()));
    tokio::time::advance(Duration::from_secs(1)).await;
    observed.changed().await.unwrap();
    let update = observed.borrow().clone().unwrap();
    assert_eq!(update.sequence(), 1);
    assert!(format!("{update:?}").contains("replacement: false"));
    assert_eq!(backend.recoveries.load(Ordering::SeqCst), 0);
    assert_eq!(*backend.confirmations.lock().unwrap(), vec![1]);
    drop(observed);
    drop(control);
    owner.await.unwrap();
}

#[tokio::test(start_paused = true)]
async fn fresh_successor_after_original_expiry_uses_new_identity_without_extending_old_deadline() {
    let mut initial = candidate(1, 9);
    let boundary = Instant::now() + Duration::from_secs(1);
    initial.original_transition = Some(boundary);
    let backend = Backend::new(Some(initial.clone()));
    *backend.transition.lock().unwrap() = Some(boundary);
    *backend.preparation_release.lock().unwrap() = Some(Arc::new(Notify::new()));
    let (updates, control, _) = teletext_output_control_channel();
    let mut observed = updates.subscribe();
    let owner = tokio::spawn(run_monitor(
        backend.clone(),
        9,
        timing(),
        Some(initial),
        updates,
        || None,
    ));
    backend.preparation_entered.notified().await;
    assert!(observed.borrow().is_none());
    backend.store.lock().unwrap().accepted = Some(candidate(2, 9));
    tokio::time::advance(Duration::from_secs(1)).await;
    observed.changed().await.unwrap();
    // The bounded slot may coalesce withdrawal into the fresh replacement. The
    // native pump separately proves fail-closed rejection of that proposal.
    for _ in 0..10 {
        if observed.borrow().as_ref().unwrap().sequence() == 2 {
            break;
        }
        tokio::task::yield_now().await;
    }
    let update = observed.borrow().clone().unwrap();
    assert_eq!(update.sequence(), 2);
    assert!(format!("{update:?}").contains("replacement: true"));
    assert_eq!(*backend.confirmations.lock().unwrap(), vec![1, 2]);
    assert_eq!(backend.handover_checks.load(Ordering::SeqCst), 0);
    drop(observed);
    drop(control);
    owner.await.unwrap();
}

#[tokio::test(start_paused = true)]
async fn superseding_unacknowledged_successors_keep_expiry_withdrawal_until_exact_applied() {
    use crate::transport::iptv::teletext_binding::TeletextOutputTransitionRejection;

    for finish_with_withdrawal in [false, true] {
        let backend = Backend::new(Some(candidate(2, 9)));
        let (updates, _control, _) = teletext_output_control_channel();
        let mut state = MonitorState::new(Some(candidate(1, 9)));
        state.cached = None;
        state.expiry_replacement = true;
        assert!(state.withdraw(&updates));
        assert!(reconcile(&backend, &mut state, &updates, 9, deadline()).await);
        assert_eq!(state.sequence, 2);
        assert!(
            state.expiry_replacement,
            "submission is not physical retirement"
        );

        // A slow sink may still be emitting the expired original when another
        // accepted generation coalesces both earlier withdrawal and successor.
        backend.store.lock().unwrap().accepted = Some(candidate(3, 9));
        assert!(reconcile(&backend, &mut state, &updates, 9, deadline()).await);
        assert_eq!(state.sequence, 4);
        for status in [
            (4, TeletextOutputTransitionPhase::Waiting),
            (2, TeletextOutputTransitionPhase::Applied),
        ] {
            assert!(state.observe_rejection(Some(status), &updates, 9));
            assert!(state.expiry_replacement);
            assert!(!state.handover_confirmed);
        }
        assert!(state.observe_rejection(
            Some((
                4,
                TeletextOutputTransitionPhase::Rejected(
                    TeletextOutputTransitionRejection::ResourceUpdate,
                ),
            )),
            &updates,
            9,
        ));
        assert_eq!(state.sequence, 5);
        assert!(state.expiry_replacement);

        let applied = if finish_with_withdrawal {
            5
        } else {
            backend.store.lock().unwrap().accepted = Some(candidate(4, 9));
            assert!(reconcile(&backend, &mut state, &updates, 9, deadline()).await);
            assert_eq!(state.sequence, 6);
            assert!(state.expiry_replacement);
            6
        };
        assert!(state.observe_rejection(
            Some((applied - 1, TeletextOutputTransitionPhase::Applied)),
            &updates,
            9,
        ));
        assert!(
            state.expiry_replacement,
            "obsolete Applied is not this proposal"
        );
        assert!(state.observe_rejection(
            Some((applied, TeletextOutputTransitionPhase::Applied)),
            &updates,
            9,
        ));
        assert!(!state.expiry_replacement);
        assert_eq!(state.handover_confirmed, !finish_with_withdrawal);
    }
}
