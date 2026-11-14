//! Coordinator lifecycle proof without producers or actual guide mutations.
use super::*;
use ersatzrs_core::locking::EntityLocker;

#[tokio::test(start_paused = true)]
async fn cancelled_guide_work_drains_its_specific_hold_and_quarantines_unconfirmed_cleanup() {
    let locker = EntityLocker::default();
    let guard = TeletextWorkGuard::try_acquire(&locker, 1).unwrap();
    let unrelated = TeletextWorkGuard::try_acquire(&locker, 2).unwrap();
    let hold = guard.hold_work();
    let (release, wait) = tokio::sync::oneshot::channel::<()>();
    let worker = tokio::spawn(async move {
        let _hold = hold;
        let _ = wait.await;
    });
    assert!(!guard.is_settled());
    release.send(()).unwrap();
    worker.await.unwrap();
    assert!(
        settle(
            &guard,
            Instant::now() + Duration::from_secs(30),
            Duration::from_millis(25)
        )
        .await
    );
    // Another operation's active lease cannot hide settlement of this owner.
    assert_eq!(locker.active_teletext_work(), 2);
    drop(guard);
    drop(unrelated);
    assert_eq!(locker.active_teletext_work(), 0);
    let guard = TeletextWorkGuard::try_acquire(&locker, 1).unwrap();
    let hold = guard.hold_work();
    assert!(!settle(&guard, finalization_deadline(), Duration::from_millis(25)).await);
    assert!(!guard.is_settled());
    drop(hold);
    drop(guard);
    assert_eq!(locker.active_teletext_work(), 1);
    assert!(TeletextWorkGuard::try_acquire(&locker, 1).is_none());
}

#[tokio::test(start_paused = true)]
async fn expired_production_work_can_settle_without_releasing_capacity_before_actual_cleanup() {
    let locker = EntityLocker::default();
    let guard = TeletextWorkGuard::try_acquire(&locker, 1).unwrap();
    let hold = guard.hold_work();
    let expired = Instant::now().checked_sub(Duration::from_secs(1)).unwrap();
    assert!(wall_deadline(Utc::now(), expired).is_none());
    let deadline = finalization_deadline();
    assert!(deadline > Instant::now());
    let worker_locker = locker.clone();
    let worker = tokio::spawn(async move {
        tokio::time::sleep(Duration::from_millis(100)).await;
        // Expiring the production operation does not release its actual worker.
        assert!(TeletextWorkGuard::try_acquire(&worker_locker, 1).is_none());
        drop(hold);
    });
    assert!(!guard.is_settled());
    assert!(TeletextWorkGuard::try_acquire(&locker, 1).is_none());
    assert!(settle(&guard, deadline, Duration::from_millis(25)).await);
    worker.await.unwrap();
    assert!(guard.is_settled());
    // Final bookkeeping still owns the admitted guard even after worker exit.
    assert!(TeletextWorkGuard::try_acquire(&locker, 1).is_none());
    drop(guard);
    assert_eq!(locker.active_teletext_work(), 0);
    let next = TeletextWorkGuard::try_acquire(&locker, 1).unwrap();
    drop(next);
    assert_eq!(locker.active_teletext_work(), 0);
    // The terminal allowance never renews the expired production/source clock.
    assert!(wall_deadline(Utc::now(), expired).is_none());
}

#[test]
fn lease_clock_uses_only_the_original_remaining_operation_allowance() {
    let now = Utc::now();
    let deadline = Instant::now() + Duration::from_secs(30);
    let first = wall_deadline(now, deadline).unwrap();
    assert!(first <= now + chrono::Duration::seconds(30));
    let expired = Instant::now().checked_sub(Duration::from_secs(1)).unwrap();
    assert!(wall_deadline(now, expired).is_none());
}

fn base_selection(revision: i64) -> TeletextChannelSelectionSnapshot {
    use ersatzrs_domain::{
        TeletextChannelSelectionSnapshot, TeletextInstallationSnapshot, TeletextPageContract,
        TeletextSelectionSnapshot,
    };
    TeletextChannelSelectionSnapshot {
        channel_id: 1,
        bindings: vec![TeletextSelectionSnapshot {
            contract: TeletextPageContract::V3,
            channel_id: 1,
            addon_id: "org.ersatzrs.synthetic-refresh".to_owned(),
            binding_revision: 1,
            profile_id: 1,
            profile_revision: revision,
            installation: TeletextInstallationSnapshot {
                addon_id: "org.ersatzrs.synthetic-refresh".to_owned(),
                state_revision: "fixture".to_owned(),
                version: "0.1.1".to_owned(),
                package_sha256: "a".repeat(64),
                content_sha256: "b".repeat(64),
                manifest_sha256: "c".repeat(64),
            },
            effective_settings: vec![],
        }],
    }
}

#[test]
fn published_host_membership_does_not_revoke_the_guide_worker() {
    let original = base_selection(1);
    let mut with_host = original.clone();
    with_host
        .bindings
        .insert(0, ersatzrs_domain::teletext_custom_pages_selection(1));
    // Deleting the final latest link must preserve the published recipe.
    assert_eq!(
        base_configuration(&with_host, &original),
        Configuration::Current
    );
    // Adding a first link cannot publish its host membership implicitly.
    assert_eq!(
        base_configuration(&original, &with_host),
        Configuration::Current
    );
    assert_eq!(
        base_configuration(&with_host, &base_selection(2)),
        Configuration::Changed
    );
    let mut disabled = with_host.clone();
    disabled.bindings.pop();
    assert_eq!(
        base_configuration(&with_host, &disabled),
        Configuration::Disabled
    );
    let mut added = original.clone();
    let mut extra = added.bindings[0].clone();
    extra.addon_id.push_str("-second");
    added.bindings.push(extra);
    assert_eq!(
        base_configuration(&with_host, &added),
        Configuration::NeedsReview
    );
}

#[test]
fn a_profile_change_refreshes_once_and_runtime_identity_cache_eviction_preserves_comparison() {
    let original = base_selection(1);
    let changed = base_selection(2);
    assert_eq!(
        base_configuration(&original, &changed),
        Configuration::Changed
    );
    let key = |seed_id, runtime_id| ConfigurationIdentity {
        seed_id,
        runtime_id,
        digest: "d".repeat(64),
    };
    let mut cache = ConfigurationCache::new();
    cache.set_maximum(128);
    cache.insert(
        key(1, None),
        TeletextGuideRefreshSelection::Base(original.clone()),
        64,
    );
    cache.insert(
        key(1, Some(10)),
        TeletextGuideRefreshSelection::Base(changed.clone()),
        64,
    );
    assert!(cache.get(&key(1, None)).is_none());
    let TeletextGuideRefreshSelection::Base(baseline) = cache.get(&key(1, Some(10))).unwrap()
    else {
        panic!("wrong role");
    };
    assert_eq!(
        base_configuration(&baseline, &changed),
        Configuration::Current
    );
    for id in [2, 3] {
        cache.insert(
            key(id, Some(10)),
            TeletextGuideRefreshSelection::Base(changed.clone()),
            64,
        );
    }
    assert!(cache.get(&key(1, Some(10))).is_none());
    assert_eq!(cache.charged, 128);
    // A miss reloads the same immutable runtime baseline, never the old seed.
    cache.insert(
        key(1, Some(10)),
        TeletextGuideRefreshSelection::Base(changed.clone()),
        64,
    );
    let TeletextGuideRefreshSelection::Base(reloaded) = cache.get(&key(1, Some(10))).unwrap()
    else {
        panic!("wrong role");
    };
    assert_eq!(
        base_configuration(&reloaded, &changed),
        Configuration::Current
    );
    cache.set_maximum(32);
    assert_eq!(cache.charged, 0);
    assert!(cache.values.is_empty());
    cache.set_maximum(1024);
    for id in 1..=257 {
        cache.insert(
            key(id, None),
            TeletextGuideRefreshSelection::Base(changed.clone()),
            1,
        );
    }
    assert_eq!(cache.values.len(), MAX_TELETEXT_GUIDE_REFRESH_BATCH);
    assert!(cache.get(&key(257, None)).is_some());
}
