//! An edited active profile withdraws; its original Lua interval still expires.
use super::*;
use ersatzrs_app::teletext_channel_documents::ChannelTeletextDocumentError;
use ersatzrs_web::host::{
    teletext_channel_documents::ChannelTeletextDocumentEncodingError,
    teletext_designs::NativeTeletextDesignError,
    teletext_generations::NativeTeletextGenerationError,
};

type IntervalRow = (i64, String, String, String, i64, Option<String>);

async fn stored_intervals(fixture: &Fixture) -> Vec<IntervalRow> {
    sqlx::query_as(r#"SELECT "PlayoutId", "AddonId", "Start", "Finish", "State", "CandidateSnapshot" FROM "PlayoutTeletextInterval" ORDER BY "PlayoutId", "AddonId", "Start""#)
        .fetch_all(&fixture.state.pool).await.unwrap()
}

pub(super) async fn verify_interval(fixture: &Fixture, outcome: &Outcome, root: &Path) {
    let original: Vec<IntervalRow> = serde_json::from_slice(
        &tokio::fs::read(root.join("original-intervals.json"))
            .await
            .unwrap(),
    )
    .unwrap();
    assert_eq!(stored_intervals(fixture).await, original);
    // A historical clock does not bypass current profile eligibility. The
    // immutable persisted interval survives, but its old revision stays refused.
    assert!(matches!(
        SqlitePlayoutRepository(fixture.state.pool.clone())
            .published_teletext_intervals_at(3, outcome.interval.start)
            .await,
        Err(ersatzrs_core::repositories::RepositoryError::Conflict(_))
    ));
}

async fn reject_stale(
    trial: &Trial,
    scoped: &AcceptedEffectiveTeletextOutputCandidate,
    expired: bool,
) {
    let request = scoped.generation().request.clone();
    let result = accept_effective_original_teletext_generation(
        &trial.fixture.state.application,
        TeletextGenerationTarget {
            expected_design_head: request.revision,
            request,
            expected_generation_id: Some(scoped.generation().id),
        },
        designs::budget(),
        base_policy(),
        designs::deadline(),
    )
    .await;
    if expired {
        assert!(matches!(
            result,
            Err(NativeTeletextGenerationError::Design(
                NativeTeletextDesignError::TimedOut
            ))
        ));
    } else {
        let Err(NativeTeletextGenerationError::Design(NativeTeletextDesignError::Encoding(error))) =
            result
        else {
            panic!("edited scope must reject at the original selection fence")
        };
        assert!(matches!(
            *error,
            ChannelTeletextDocumentEncodingError::Preparation(
                ChannelTeletextDocumentError::SelectionChanged
            )
        ));
    }
    assert_eq!(
        trial
            .fixture
            .state
            .application
            .effective_teletext_accepted_generation(3)
            .await
            .unwrap(),
        Some(scoped.generation().clone())
    );
}

pub(super) async fn transition(trial: &Trial, root: &Path, origin: &live_tcp::Origin) -> Outcome {
    let initial = updates::wait_pair(origin, [0; 2], &trial.base[0], &trial.base[1]).await;
    updates::write_stage(root, "initial", &initial);
    let interval = publish_lua(trial).await;
    let original = stored_intervals(&trial.fixture).await;
    assert_eq!(original.len(), 1);
    write_new(
        root,
        "original-intervals.json",
        &serde_json::to_vec(&original).unwrap(),
    )
    .await;
    let request = request_scope(trial).await;
    let pending = hold_off(origin, &trial.base[0]).await;
    let after = updates::positions(origin);
    let scoped = accept_scope(trial, request).await;
    let first = pair_on(origin, after, trial, &scoped).await;
    updates::write_stage(root, "scoped", &first);
    let on_start = updates::positions(origin)[1];
    tokio::time::sleep(Duration::from_millis(1500)).await;
    pair_on(origin, updates::positions(origin), trial, &scoped).await;
    let on = (on_start, updates::positions(origin)[1]);

    let changed = saved(
        trial
            .fixture
            .state
            .application
            .save_teletext_profile(&super::super::super::profile_save(&trial.target, "night"))
            .await
            .unwrap(),
    );
    assert_eq!(
        changed.dependent_bindings_changed, 0,
        "profile belongs only to Lua"
    );
    assert_eq!(changed.profile.id, trial.target.id);
    assert_eq!(changed.profile.revision, trial.target.revision + 1);
    let invalidated = hold_off(origin, &trial.base[0]).await;
    reject_stale(trial, &scoped, false).await;

    let remaining = (interval.finish - chrono::Duration::milliseconds(700))
        .signed_duration_since(Utc::now())
        .to_std()
        .expect("profile edit and stale rejection leave original interval active");
    tokio::time::sleep(remaining).await;
    let after = updates::positions(origin);
    let views = origin.captures();
    let day = updates::pattern(&trial.base[0]);
    tokio::time::timeout(Duration::from_secs(5), async {
        tokio::join!(
            wait_observed_cycle(&views[0], after[0], &day),
            views[2].off(after[1])
        );
    })
    .await
    .expect("invalid profile cannot silently substitute Base before expiry");
    assert!(Utc::now() < interval.finish);
    let invalidated_end = updates::positions(origin)[1];
    write_new(
        root,
        "invalidated-plain.ts",
        &views[2].bytes()[invalidated.0..invalidated_end],
    )
    .await;

    tokio::time::sleep(
        interval
            .finish
            .signed_duration_since(Utc::now())
            .to_std()
            .unwrap(),
    )
    .await;
    let restored = updates::wait_pair(
        origin,
        updates::positions(origin),
        &trial.base[0],
        &trial.base[1],
    )
    .await;
    let repeated_after = updates::write_stage(root, "restored-own-base", &restored);
    let repeated = updates::wait_pair(origin, repeated_after, &trial.base[0], &trial.base[1]).await;
    updates::write_stage(root, "repeated-own-base", &repeated);
    reject_stale(trial, &scoped, true).await;
    disable_base(trial).await;
    let off = hold_off(origin, &trial.base[0]).await;
    write_new(root, "scope.json", &serde_json::to_vec_pretty(&json!({
        "interval":{"start":interval.start,"finish":interval.finish,"channel_id":3,"playout_id":95,
            "profile_id":trial.target.id,"profile_revision":trial.target.revision},
        "changed_profile_revision":changed.profile.revision,
        "pending_window":pending,"on_window":on,"invalidated_window":[invalidated.0,invalidated_end],
        "off_window":off,"requests":[1,0,1],"capacity":2,
        "stale_selection_rejected":true,"expired_publication_rejected":true
    })).unwrap()).await;
    Outcome {
        interval,
        scoped,
        pending,
        on,
        off,
    }
}
