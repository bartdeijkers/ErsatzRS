//! Existing observers/clock/transport oracles; no suitable-receiver acceptance.
use super::super::super::super::editorial_imports::{live_updates::live_tcp, wait_observed_cycle};
use super::super::updates;
use super::*;
use futures_util::FutureExt as _;
use std::panic::AssertUnwindSafe;

type Window = [(usize, usize); 2];

async fn stable_off(origin: &live_tcp::Origin) -> Window {
    let views = origin.captures();
    let after = updates::positions(origin);
    tokio::join!(views[0].off(after[0]), views[2].off(after[1]));
    let start = updates::positions(origin);
    // Keep a real silent interval and enough source audio for the unchanged AV
    // oracle. Waiting is on the existing responses, not another HTTP request.
    tokio::time::sleep(Duration::from_millis(1500)).await;
    tokio::join!(views[0].off(start[0]), views[2].off(start[1]));
    let end = updates::positions(origin);
    std::array::from_fn(|index| (start[index], end[index]))
}

struct ResultState {
    accepted: [AcceptedTeletextOutputCandidate; 2],
    invalidated: Window,
    pending: Window,
}

async fn transition(
    fixture: &Fixture,
    root: &Path,
    origin: &live_tcp::Origin,
    shared: &TeletextProfile,
    before: &[AcceptedTeletextOutputCandidate; 2],
) -> ResultState {
    let initial = updates::wait_pair(origin, [0; 2], &before[0], &before[1]).await;
    updates::write_stage(root, "shared-day", &initial);
    let calls = tokio::fs::read(fixture.root.path().join("ordinary-calls"))
        .await
        .unwrap();
    let secondary = tokio::fs::read(fixture.root.path().join("secondary-calls-day"))
        .await
        .unwrap();
    let changed = update_shared(fixture, shared).await;
    assert_pointers(fixture, before).await;
    for channel in [1, 2] {
        assert!(reload_base(fixture, channel).await.is_none());
    }
    let invalidated = stable_off(origin).await;
    assert_eq!(
        tokio::fs::read(fixture.root.path().join("ordinary-calls"))
            .await
            .unwrap(),
        calls
    );
    assert_eq!(
        tokio::fs::read(fixture.root.path().join("secondary-calls-day"))
            .await
            .unwrap(),
        secondary
    );
    // A valid pending request cannot restore emission before explicit Publish.
    let first = request_base(fixture, 1, Some(before[0].generation().request.id)).await;
    let second = request_base(fixture, 2, Some(before[1].generation().request.id)).await;
    for (channel, previous) in [1, 2].into_iter().zip(before) {
        assert_eq!(
            fixture
                .state
                .application
                .accepted_teletext_generation(channel)
                .await
                .unwrap(),
            Some(previous.generation().clone())
        );
        assert!(reload_base(fixture, channel).await.is_none());
    }
    let pending = stable_off(origin).await;
    let after = updates::positions(origin);
    let first = publish_base(fixture, first, Some(before[0].generation().id)).await;
    let pattern = updates::pattern(&first);
    let views = origin.captures();
    let (cycle, ()) = tokio::time::timeout(Duration::from_secs(5), async {
        tokio::join!(
            wait_observed_cycle(&views[0], after[0], &pattern),
            views[2].off(after[1])
        )
    })
    .await
    .expect("channel one publication must not activate channel two");
    write_new(
        root,
        "first-only-units.json",
        &serde_json::to_vec(&cycle.units.iter().map(|u| u.to_vec()).collect::<Vec<_>>()).unwrap(),
    )
    .await;
    assert!(reload_base(fixture, 2).await.is_none());
    let after = updates::positions(origin);
    let second = publish_base(fixture, second, Some(before[1].generation().id)).await;
    let accepted = [first, second];
    assert_multi_binding(&accepted, &changed.profile);
    let paired = updates::wait_pair(origin, after, &accepted[0], &accepted[1]).await;
    let repeated = updates::write_stage(root, "republished-night", &paired);
    let paired = updates::wait_pair(origin, repeated, &accepted[0], &accepted[1]).await;
    updates::write_stage(root, "repeated-night", &paired);
    tokio::time::sleep(Duration::from_secs(2)).await;
    updates::wait_pair(
        origin,
        updates::positions(origin),
        &accepted[0],
        &accepted[1],
    )
    .await;
    assert_eq!(origin.requests_seen(), [true, false, true]);
    write_new(
        root,
        "wire-offsets.json",
        &serde_json::to_vec_pretty(&json!({
            "invalidated": invalidated,
            "pending": pending,
            "requests": [1, 0, 1],
            "capacity": 2
        }))
        .unwrap(),
    )
    .await;
    ResultState {
        accepted,
        invalidated,
        pending,
    }
}

async fn verify(
    tools: &Tools,
    root: &Path,
    source: &[u8],
    origin: &live_tcp::Origin,
    result: &ResultState,
) {
    for (index, viewer) in [0, 2].into_iter().enumerate() {
        let mut bytes = origin.captures()[viewer].bytes();
        bytes.truncate(bytes.len() / 188 * 188);
        let gaps = live_tcp::qualified_source_clock_gap_count(&bytes);
        client_capture::verify_transport_with_qualified_authored_pts_gaps(
            &tools.tsanalyze,
            &tools.ffprobe,
            &bytes,
            gaps,
            Some(&root.join(format!("viewer-{index}-tsduck.json"))),
            true,
        )
        .await;
        client_capture::verify_av_with_audio_bound(&tools.ffmpeg, source, &bytes, 6 * 1024 * 1024)
            .await;
        for (stage, windows) in [
            ("invalidated", result.invalidated),
            ("pending", result.pending),
        ] {
            let (start, end) = windows[index];
            let absent = &bytes[start..end];
            // The full-response oracle above checks every source PCR, including
            // these intervals. A mid-response window can start with PCR before
            // its first PAT/PMT, so it is not a fresh source-clock epoch. Keep
            // every window packet for strict absent-PID, TSDuck and AV checks.
            verify_plain(tools, source, absent).await;
            write_new(root, &format!("{stage}-{index}-plain.ts"), absent).await;
        }
        write_new(root,&format!("viewer-{index}-clock.json"),&serde_json::to_vec(&json!({"qualified_authored_pts_gaps":gaps,"scope":"Unchanged raw source/PMT-Off clock oracle; qualified gaps are not zero TSDuck leaps"})).unwrap()).await;
    }
}

#[tokio::test]
#[ignore = "requires pinned FFmpeg9/TSDuck and fresh ERSATZRS_TELETEXT_ACTIVE_SHARED_DIR; not a headed receiver proof"]
async fn active_two_http_responses_withdraw_shared_profile_and_recover_only_after_publication() {
    let root = PathBuf::from(client_capture::selected(
        "ERSATZRS_TELETEXT_ACTIVE_SHARED_DIR",
    ));
    tokio::fs::create_dir(&root).await.unwrap();
    live_tcp::terminal_trace(&root).unwrap();
    let tools = Tools::load().await;
    let (mut fixture, source) = installed_profiles_with(&tools, response, 60).await;
    let (shared, _) = prepare_shared(&fixture).await;
    let mut policy: Value = serde_json::from_str(POLICY).unwrap();
    policy["version"] = json!(2);
    policy["live_updates"] = json!({"poll_interval_ms":25,"recovery_timeout_ms":2000});
    setting(&fixture, POLICY_KEY, &policy.to_string()).await;
    assert_eq!(
        ersatzrs_infra::transcode_capacity::load_active_transcode_limit(&fixture.state.pool)
            .await
            .unwrap()
            .unwrap_or(ersatzrs_infra::transcode_capacity::DEFAULT_ACTIVE_TRANSCODE_LIMIT),
        2
    );
    let before = [
        accept_base(&fixture, 1).await,
        accept_base(&fixture, 2).await,
    ];
    assert_multi_binding(&before, &shared);
    let configuration = designs::config(&fixture).await;
    let package = calculate_addon_content_sha256(&fixture.root.path().join("package"))
        .await
        .unwrap();
    write_new(&root, "source.ts", &source).await;
    reset_source_start(&fixture).await;
    let mut origin = live_tcp::Origin::start(&fixture, &root).await.unwrap();
    let mut observers = [
        live_tcp::Observer::start_for(&origin, 1, "day"),
        live_tcp::Observer::start_for(&origin, 2, "night"),
    ];
    let outcome = AssertUnwindSafe(async {
        tokio::time::timeout(
            Duration::from_secs(30),
            transition(&fixture, &root, &origin, &shared, &before),
        )
        .await
        .expect("bounded shared-profile live transition")
    })
    .catch_unwind()
    .await;
    let stopped = AssertUnwindSafe(updates::close(&mut origin, &mut observers))
        .catch_unwind()
        .await;
    for (index, viewer) in [0, 2].into_iter().enumerate() {
        write_new(
            &root,
            &format!("raw-viewer-{index}.ts"),
            &origin.captures()[viewer].bytes(),
        )
        .await;
    }
    if let Err(panic) = stopped {
        fixture.state.pool.close().await;
        std::panic::resume_unwind(panic);
    }
    let result = match outcome {
        Ok(result) => result,
        Err(panic) => {
            fixture.state.pool.close().await;
            std::panic::resume_unwind(panic);
        }
    };
    assert_eq!(designs::config(&fixture).await, configuration);
    assert_eq!(
        calculate_addon_content_sha256(&fixture.root.path().join("package"))
            .await
            .unwrap(),
        package
    );
    assert_eq!(
        tokio::fs::read(fixture.root.path().join("ordinary-calls"))
            .await
            .unwrap(),
        b"day\nday\nnight\nnight\n"
    );
    assert_eq!(
        tokio::fs::read(fixture.root.path().join("secondary-calls-day"))
            .await
            .unwrap(),
        b"1\n1\n"
    );
    assert!(fixture.playout.try_recv().is_err());
    fixture = fixture.reopen().await;
    for (channel, expected) in [1, 2].into_iter().zip(&result.accepted) {
        assert_eq!(
            reload_base(&fixture, channel).await.unwrap().generation(),
            expected.generation()
        );
    }
    fixture.state.pool.close().await;
    verify(&tools, &root, &source, &origin, &result).await;
    write_new(&root,"success.json",&serde_json::to_vec_pretty(&json!({"scope":"Two retained ordinary HTTP responses, shared profile invalidation/pending/isolated explicit publication; no suitable client, Lua/fullhost/platform/fullC or L15/9A acceptance","requests":[1,0,1],"capacity":2,"full_model_cycles":true,"configuration_packages_retained":true,"no_automatic_addon_replay":true,"source_av_tsduck_clock_cleanup":true})).unwrap()).await;
}
