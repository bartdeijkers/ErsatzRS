//! Real Lua expiry on retained ordinary responses; receiver acceptance is separate.
use super::super::super::editorial_imports::{live_updates::live_tcp, wait_observed_cycle};
use super::super::super::scripted_controls;
use super::*;
use ersatzrs_core::repositories::PlayoutRepository as _;
use ersatzrs_domain::{
    ChannelTeletextBindingRead, TeletextOverrideInterval, TeletextOverrideState,
};
use ersatzrs_infra::repositories::SqlitePlayoutRepository;
use futures_util::FutureExt as _;
use std::{panic::AssertUnwindSafe, path::Path};

struct Outcome {
    interval: TeletextOverrideInterval,
    off: (usize, usize),
}

async fn write(root: &Path, name: &str, bytes: &[u8]) {
    let mut file = tokio::fs::OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(root.join(name))
        .await
        .unwrap();
    file.write_all(bytes).await.unwrap();
}

async fn publish_off(fixture: &Fixture) -> TeletextOverrideInterval {
    // Add the existing Scripted publication fixture only after both original
    // JSON-source responses are running. It cannot replace their selected source.
    scripted_controls::seed_publication_inputs(fixture).await;
    let start = ersatzrs_core::scheduling::truncate_to_persisted_precision(Utc::now());
    let script =
        format!("assert(teletext_enabled('{ADDON}', false).ok)\nadd_duration('clip', '00:00:08')");
    let result = scripted_controls::build_at(fixture, &script, 1, true, start)
        .await
        .unwrap();
    assert_eq!(result.teletext_intervals.len(), 1);
    let interval = result.teletext_intervals[0].clone();
    assert!(matches!(&interval.state,
        TeletextOverrideState::Off { channel_id: 1, addon_id } if addon_id == ADDON));
    assert_eq!(interval.start, start);
    assert_eq!(interval.finish - start, chrono::Duration::seconds(8));
    assert_eq!(result.added_items.len(), 1);
    assert_eq!(result.added_items[0].start, start);
    assert_eq!(result.added_items[0].finish, interval.finish);
    scripted_controls::publish_result(fixture, &result).await;
    let repository = SqlitePlayoutRepository(fixture.state.pool.clone());
    for when in [start, interval.finish - chrono::Duration::milliseconds(1)] {
        assert_eq!(
            repository
                .published_teletext_intervals_at(1, when)
                .await
                .unwrap(),
            std::slice::from_ref(&interval)
        );
    }
    assert!(repository
        .published_teletext_intervals_at(1, interval.finish)
        .await
        .unwrap()
        .is_empty());
    assert!(repository
        .published_teletext_intervals_at(2, start)
        .await
        .unwrap()
        .is_empty());
    interval
}

async fn transition(
    fixture: &Fixture,
    root: &Path,
    origin: &live_tcp::Origin,
    before: &[AcceptedTeletextOutputCandidate; 2],
) -> Outcome {
    let initial = updates::wait_pair(origin, [0; 2], &before[0], &before[1]).await;
    updates::write_stage(root, "initial", &initial);
    let interval = publish_off(fixture).await;
    let start = interval.start;
    let repository = SqlitePlayoutRepository(fixture.state.pool.clone());
    let views = origin.captures();
    let after = updates::positions(origin);
    let night = updates::pattern(&before[1]);
    let ((), night_cycle) = tokio::time::timeout(Duration::from_secs(5), async {
        tokio::join!(
            views[0].off(after[0]),
            wait_observed_cycle(&views[2], after[1], &night)
        )
    })
    .await
    .expect("Lua Off must withdraw only channel one");
    let off_start = updates::positions(origin)[0];
    write(
        root,
        "isolated-night-units.json",
        &serde_json::to_vec(
            &night_cycle
                .units
                .iter()
                .map(|u| u.to_vec())
                .collect::<Vec<_>>(),
        )
        .unwrap(),
    )
    .await;
    // An uncaught error after an On command must not publish partial intervals,
    // invoke producers, advance the existing finish, or change active output.
    let broken = format!(
        "assert(teletext_enabled('{ADDON}', true).ok)\nadd_duration('clip', '00:00:08')\nerror('synthetic scope rollback')"
    );
    assert!(
        scripted_controls::build_at(fixture, &broken, 1, true, start)
            .await
            .is_err()
    );
    assert_eq!(
        repository
            .published_teletext_intervals_at(1, start)
            .await
            .unwrap(),
        std::slice::from_ref(&interval)
    );
    let tail = interval.finish - chrono::Duration::milliseconds(500);
    let remaining = tail
        .signed_duration_since(Utc::now())
        .to_std()
        .expect("setup/error checks must leave a real Off interval");
    tokio::time::sleep(remaining).await;
    let after = updates::positions(origin);
    tokio::join!(
        views[0].off(after[0]),
        wait_observed_cycle(&views[2], after[1], &night)
    );
    assert!(
        Utc::now() < interval.finish,
        "Off observed before original expiry"
    );
    let off_end = updates::positions(origin)[0];
    let remaining = interval
        .finish
        .signed_duration_since(Utc::now())
        .to_std()
        .unwrap();
    tokio::time::sleep(remaining).await;
    let expired = Utc::now();
    assert!(expired >= interval.finish);
    assert!(repository
        .published_teletext_intervals_at(1, expired)
        .await
        .unwrap()
        .is_empty());
    let after = updates::positions(origin);
    let restored = updates::wait_pair(origin, after, &before[0], &before[1]).await;
    let repeated_after = updates::write_stage(root, "restored", &restored);
    let repeated = updates::wait_pair(origin, repeated_after, &before[0], &before[1]).await;
    updates::write_stage(root, "repeated", &repeated);
    tokio::time::sleep(Duration::from_secs(2)).await;
    updates::wait_pair(origin, updates::positions(origin), &before[0], &before[1]).await;
    assert_eq!(origin.requests_seen(), [true, false, true]);
    write(
        root,
        "scope.json",
        &serde_json::to_vec_pretty(&json!({
            "start": start, "finish": interval.finish, "observed_expired": expired,
            "off_window": [off_start, off_end], "same_responses": [1, 0, 1],
            "capacity": 2, "script_error_preserves_interval": true
        }))
        .unwrap(),
    )
    .await;
    Outcome {
        interval,
        off: (off_start, off_end),
    }
}

async fn verify(
    tools: &Tools,
    root: &Path,
    source: &[u8],
    origin: &live_tcp::Origin,
    outcome: &Outcome,
) {
    for (index, viewer) in [0, 2].into_iter().enumerate() {
        let mut bytes = origin.captures()[viewer].bytes();
        bytes.truncate(bytes.len() / 188 * 188);
        let gaps = live_tcp::qualified_source_clock_gap_count(&bytes);
        assert_eq!(
            gaps,
            usize::from(index == 0),
            "only withdrawn channel has an authored PTS gap"
        );
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
        if index == 0 {
            // Every source PCR is checked in the complete response above. Keep
            // all packets in this mid-response window for absent-PID/TS/AV.
            let absent = &bytes[outcome.off.0..outcome.off.1];
            verify_plain(tools, source, absent).await;
            write(root, "off-window.ts", absent).await;
        }
        write(root, &format!("viewer-{index}-clock.json"), &serde_json::to_vec(&json!({
            "qualified_authored_pts_gaps": gaps,
            "scope": "Full unchanged source/PMT-Off oracle; qualified gaps are not zero total TSDuck leaps"
        })).unwrap()).await;
    }
}

async fn assert_retained(
    mut fixture: Fixture,
    configuration: &[(Option<String>, Option<String>)],
    package: &str,
    bindings: &[Vec<ChannelTeletextBindingRead>; 2],
    before: &[AcceptedTeletextOutputCandidate; 2],
    interval: &TeletextOverrideInterval,
) {
    assert_eq!(designs::config(&fixture).await.as_slice(), configuration);
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
        b"day\nnight\n"
    );
    assert!(fixture.playout.try_recv().is_err());
    for statement in [
        r#"SELECT COUNT(*) FROM "TeletextEffectivePublicationRequestCurrent""#,
        r#"SELECT COUNT(*) FROM "TeletextEffectiveAcceptedGenerationCurrent""#,
    ] {
        let count: i64 = sqlx::query_scalar(statement)
            .fetch_one(&fixture.state.pool)
            .await
            .unwrap();
        assert_eq!(
            count, 0,
            "Lua cannot create or publish authored generations"
        );
    }
    fixture = fixture.reopen().await;
    for (index, channel) in [1, 2].into_iter().enumerate() {
        assert_eq!(
            fixture
                .state
                .application
                .channel_teletext_bindings(channel)
                .await
                .unwrap(),
            bindings[index]
        );
        assert_eq!(
            fixture
                .state
                .application
                .accepted_teletext_generation(channel)
                .await
                .unwrap(),
            Some(before[index].generation().clone())
        );
    }
    let repository = SqlitePlayoutRepository(fixture.state.pool.clone());
    assert_eq!(
        repository
            .published_teletext_intervals_at(1, interval.start)
            .await
            .unwrap(),
        std::slice::from_ref(interval)
    );
    assert!(repository
        .published_teletext_intervals_at(1, interval.finish)
        .await
        .unwrap()
        .is_empty());
    fixture.state.pool.close().await;
}

#[tokio::test]
#[ignore = "requires pinned FFmpeg9/TSDuck and fresh ERSATZRS_TELETEXT_LUA_EXPIRY_DIR; not suitable-client acceptance"]
async fn retained_responses_restore_base_after_real_lua_off_expiry_and_error() {
    let root = PathBuf::from(client_capture::selected("ERSATZRS_TELETEXT_LUA_EXPIRY_DIR"));
    tokio::fs::create_dir(&root).await.unwrap();
    live_tcp::terminal_trace(&root).unwrap();
    let tools = Tools::load().await;
    let (fixture, source) = installed_profiles_with(&tools, response, 60).await;
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
    let configuration = designs::config(&fixture).await;
    let package = calculate_addon_content_sha256(&fixture.root.path().join("package"))
        .await
        .unwrap();
    let bindings = [
        fixture
            .state
            .application
            .channel_teletext_bindings(1)
            .await
            .unwrap(),
        fixture
            .state
            .application
            .channel_teletext_bindings(2)
            .await
            .unwrap(),
    ];
    write(&root, "source.ts", &source).await;
    reset_source_start(&fixture).await;
    let mut origin = live_tcp::Origin::start(&fixture, &root).await.unwrap();
    let mut observers = [
        live_tcp::Observer::start_for(&origin, 1, "day"),
        live_tcp::Observer::start_for(&origin, 2, "night"),
    ];
    let outcome = AssertUnwindSafe(async {
        tokio::time::timeout(
            Duration::from_secs(30),
            transition(&fixture, &root, &origin, &before),
        )
        .await
        .expect("bounded actual Lua expiry trial")
    })
    .catch_unwind()
    .await;
    let stopped = AssertUnwindSafe(updates::close(&mut origin, &mut observers))
        .catch_unwind()
        .await;
    for (index, viewer) in [0, 2].into_iter().enumerate() {
        write(
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
    let outcome = match outcome {
        Ok(value) => value,
        Err(panic) => {
            fixture.state.pool.close().await;
            std::panic::resume_unwind(panic);
        }
    };
    assert_retained(
        fixture,
        &configuration,
        &package,
        &bindings,
        &before,
        &outcome.interval,
    )
    .await;
    verify(&tools, &root, &source, &origin, &outcome).await;
    write(&root, "success.json", &serde_json::to_vec_pretty(&json!({
        "scope": "Real Lua finite Off/error/expiry on two retained ordinary HTTP responses; no GUI receiver, full Lua/wholehost/platform/L15/9A acceptance",
        "requests": [1, 0, 1], "capacity": 2, "automatic_base_restoration": true,
        "complete_models_source_av_clock_transport_cleanup": true,
        "no_implicit_generation_or_addon_activation": true
    })).unwrap()).await;
}
