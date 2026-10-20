//! Mirror output owns its live Lua scope, Base recovery and Off independently.
use super::super::super::super::super::{
    editorial_imports::{live_updates::live_tcp, wait_observed_cycle, ObservedCycle},
    effective_designs as originals, effective_publications as pending, scripted_controls,
};
use super::super::super::updates;
use super::*;
use ersatzrs_core::repositories::PlayoutRepository as _;
use ersatzrs_domain::{
    ChannelTeletextBindingRead, TeletextEffectiveBindingSnapshot, TeletextOverrideInterval,
    TeletextOverrideState,
};
use ersatzrs_infra::repositories::SqlitePlayoutRepository;
use ersatzrs_web::host::{
    teletext_effective_channel_documents::prepare_effective_channel_teletext_document_candidate,
    teletext_effective_generations::{
        accept_effective_original_teletext_generation, AcceptedEffectiveTeletextOutputCandidate,
    },
    teletext_effective_publications::request_effective_original_teletext_design_publication,
};
use futures_util::FutureExt as _;
use std::panic::AssertUnwindSafe;

struct Trial {
    fixture: Fixture,
    source: Vec<u8>,
    base: [AcceptedTeletextOutputCandidate; 2],
    mirror: ChannelTeletextBinding,
    target: TeletextProfile,
    unchanged: [Vec<ChannelTeletextBindingRead>; 2],
    configuration: Vec<(Option<String>, Option<String>)>,
    package: String,
}

struct Outcome {
    interval: TeletextOverrideInterval,
    scoped: AcceptedEffectiveTeletextOutputCandidate,
    pending: (usize, usize),
    on: (usize, usize),
    off: (usize, usize),
}

async fn setup(tools: &Tools) -> Trial {
    let (fixture, source) = installed_profiles_with(tools, response, 60).await;
    install_mirror(&fixture).await;
    let profiles = fixture
        .state
        .application
        .list_teletext_profiles(ADDON)
        .await
        .unwrap();
    let target = profiles
        .iter()
        .find(|p| p.profile.name == "day")
        .unwrap()
        .profile
        .clone();
    let night = &profiles
        .iter()
        .find(|p| p.profile.name == "night")
        .unwrap()
        .profile;
    let mirror = fixture.bind(3, night, None).await;
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
    let base = [
        accept_base(&fixture, 1).await,
        accept_base(&fixture, 3).await,
    ];
    assert_ne!(
        designs::units(base[0].service()),
        designs::units(base[1].service())
    );
    let unchanged = [
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
    let configuration = designs::config(&fixture).await;
    let package = calculate_addon_content_sha256(&fixture.root.path().join("package"))
        .await
        .unwrap();
    Trial {
        fixture,
        source,
        base,
        mirror,
        target,
        unchanged,
        configuration,
        package,
    }
}

async fn publish_lua(trial: &Trial) -> TeletextOverrideInterval {
    let fixture = &trial.fixture;
    scripted_controls::seed_publication_inputs(fixture).await;
    let file = fixture.root.path().join("controls-3.lua");
    sqlx::query(r#"INSERT INTO "Playout" ("Id","ChannelId","ScheduleKind","ScheduleFile","Seed") VALUES (95,3,4,?,0)"#)
        .bind(file.to_str().unwrap()).execute(&fixture.state.pool).await.unwrap();
    let start = ersatzrs_core::scheduling::truncate_to_persisted_precision(Utc::now());
    let script = format!("assert(teletext_profile('{ADDON}', {}).ok)\nassert(teletext_enabled('{ADDON}', true).ok)\nadd_duration('clip', '00:00:12')", trial.target.id);
    let result = scripted_controls::build_at(fixture, &script, 3, true, start)
        .await
        .unwrap();
    assert_eq!(result.teletext_intervals.len(), 1);
    let interval = result.teletext_intervals[0].clone();
    assert_eq!(interval.start, start);
    assert_eq!(interval.finish - start, chrono::Duration::seconds(12));
    assert!(
        matches!(&interval.state, TeletextOverrideState::Enabled(value)
        if value.channel_id == 3 && value.profile_id == trial.target.id && value.profile_revision == trial.target.revision)
    );
    scripted_controls::publish_result(fixture, &result).await;
    let repository = SqlitePlayoutRepository(fixture.state.pool.clone());
    for when in [start, interval.finish - chrono::Duration::milliseconds(1)] {
        assert_eq!(
            repository
                .published_teletext_intervals_at(3, when)
                .await
                .unwrap(),
            std::slice::from_ref(&interval)
        );
        assert!(repository
            .published_teletext_intervals_at(1, when)
            .await
            .unwrap()
            .is_empty());
    }
    assert!(repository
        .published_teletext_intervals_at(3, interval.finish)
        .await
        .unwrap()
        .is_empty());
    interval
}

async fn request_scope(trial: &Trial) -> ersatzrs_domain::TeletextPublicationRequest {
    let fixture = &trial.fixture;
    // Producer input is outside the immutable package. Its unique label makes
    // inherited source-Day and old mirror-Night observably wrong alternatives.
    let mut changed = response("day");
    changed["pages"][0]["rows"][0]["runs"][0]["content"]["text"] = json!("SCOPED MIRROR");
    tokio::fs::write(
        fixture.root.path().join("ordinary-response-day.json"),
        serde_json::to_vec(&changed).unwrap(),
    )
    .await
    .unwrap();
    let candidate = prepare_effective_channel_teletext_document_candidate(
        &fixture.state.application,
        3,
        Some(ADDON),
        &guide(&[]),
        base_policy(),
    )
    .await
    .unwrap();
    let saved = originals::saved_design(
        originals::save(fixture, candidate, None, designs::budget())
            .await
            .unwrap(),
    );
    let requested = request_effective_original_teletext_design_publication(
        &fixture.state.application,
        pending::target(&saved, None),
        designs::budget(),
        base_policy(),
        designs::deadline(),
    )
    .await
    .unwrap();
    let TeletextPublicationMutation::Requested(requested) = requested else {
        panic!("explicit scoped pending request")
    };
    assert_eq!(requested.request().channel_id, 3);
    assert!(fixture
        .state
        .application
        .effective_teletext_accepted_generation(3)
        .await
        .unwrap()
        .is_none());
    requested.request().clone()
}

async fn accept_scope(
    trial: &Trial,
    request: ersatzrs_domain::TeletextPublicationRequest,
) -> AcceptedEffectiveTeletextOutputCandidate {
    let value = accept_effective_original_teletext_generation(
        &trial.fixture.state.application,
        TeletextGenerationTarget {
            expected_design_head: request.revision,
            request,
            expected_generation_id: None,
        },
        designs::budget(),
        base_policy(),
        designs::deadline(),
    )
    .await
    .unwrap();
    let TeletextGenerationMutation::Accepted(value) = value else {
        panic!("explicit scoped acceptance")
    };
    assert_eq!(value.selection().channel_id, 3);
    let TeletextEffectiveBindingSnapshot::Scripted(scope) = &value.selection().bindings[0] else {
        panic!("scoped ownership, not source or Base")
    };
    assert_eq!(scope.playout_id, 95);
    assert!(
        matches!(&scope.interval.state, TeletextOverrideState::Enabled(target)
        if target.channel_id == 3 && target.profile_id == trial.target.id)
    );
    let service = value.service().unwrap();
    assert!(service.magazine_units().is_empty());
    assert!(service.declared().resource_inventory().is_empty());
    assert!(service.compiler_drcs_resources().is_empty());
    assert_eq!(service.pages().len(), 3);
    assert_ne!(
        designs::units(service),
        designs::units(trial.base[0].service())
    );
    assert_ne!(
        designs::units(service),
        designs::units(trial.base[1].service())
    );
    value
}

async fn pair_on(
    origin: &live_tcp::Origin,
    after: [usize; 2],
    trial: &Trial,
    scoped: &AcceptedEffectiveTeletextOutputCandidate,
) -> [ObservedCycle; 2] {
    let views = origin.captures();
    let day = updates::pattern(&trial.base[0]);
    let mut carousel =
        TeletextCarousel::new_authored_shared(Arc::clone(scoped.service().unwrap())).unwrap();
    let first =
        super::super::super::super::super::editorial_imports::observed_cycle_units(&mut carousel);
    let second =
        super::super::super::super::super::editorial_imports::observed_cycle_units(&mut carousel);
    let pattern = first
        .into_iter()
        .zip(second)
        .map(|(a, b)| vec![a, b])
        .collect::<Vec<_>>();
    let (source, mirror) = tokio::time::timeout(Duration::from_secs(5), async {
        tokio::join!(
            wait_observed_cycle(&views[0], after[0], &day),
            wait_observed_cycle(&views[2], after[1], &pattern)
        )
    })
    .await
    .expect("isolated source and exact scoped mirror cycles");
    assert_eq!(origin.requests_seen(), [true, false, true]);
    [source, mirror]
}

async fn hold_off(
    origin: &live_tcp::Origin,
    source: &AcceptedTeletextOutputCandidate,
) -> (usize, usize) {
    let views = origin.captures();
    let pattern = updates::pattern(source);
    let after = updates::positions(origin);
    tokio::join!(
        wait_observed_cycle(&views[0], after[0], &pattern),
        views[2].off(after[1])
    );
    let start = updates::positions(origin)[1];
    tokio::time::sleep(Duration::from_millis(1500)).await;
    let after = updates::positions(origin);
    tokio::join!(
        wait_observed_cycle(&views[0], after[0], &pattern),
        views[2].off(after[1])
    );
    (start, updates::positions(origin)[1])
}

async fn disable_base(trial: &Trial) {
    let disabled = trial
        .fixture
        .state
        .application
        .disable_channel_teletext_binding(3, ADDON, trial.mirror.revision)
        .await
        .unwrap();
    let disabled = saved(disabled);
    assert_eq!(disabled.channel_id, 3);
    assert!(!disabled.enabled);
    assert_eq!(disabled.revision, trial.mirror.revision + 1);
}

async fn transition(
    trial: &Trial,
    root: &Path,
    origin: &live_tcp::Origin,
    disable_during_scope: bool,
) -> Outcome {
    let initial = updates::wait_pair(origin, [0; 2], &trial.base[0], &trial.base[1]).await;
    updates::write_stage(root, "initial", &initial);
    let interval = publish_lua(trial).await;
    let request = request_scope(trial).await;
    let pending = hold_off(origin, &trial.base[0]).await;
    let after = updates::positions(origin);
    let scoped = accept_scope(trial, request).await;
    let first = pair_on(origin, after, trial, &scoped).await;
    updates::write_stage(root, "scoped", &first);
    if disable_during_scope {
        disable_base(trial).await;
        let retained = pair_on(origin, updates::positions(origin), trial, &scoped).await;
        updates::write_stage(root, "admin-off-scoped-on", &retained);
    }
    let start = updates::positions(origin)[1];
    let remaining = (interval.finish - chrono::Duration::milliseconds(700))
        .signed_duration_since(Utc::now())
        .to_std()
        .expect("preparation/publication must leave a real scoped display interval");
    tokio::time::sleep(remaining).await;
    let late = pair_on(origin, updates::positions(origin), trial, &scoped).await;
    updates::write_stage(root, "late-scoped", &late);
    assert!(
        Utc::now() < interval.finish,
        "scoped display before original expiry"
    );
    let on = (start, updates::positions(origin)[1]);
    tokio::time::sleep(
        interval
            .finish
            .signed_duration_since(Utc::now())
            .to_std()
            .unwrap(),
    )
    .await;
    if !disable_during_scope {
        let restored = updates::wait_pair(
            origin,
            updates::positions(origin),
            &trial.base[0],
            &trial.base[1],
        )
        .await;
        let repeated_after = updates::write_stage(root, "restored-own-base", &restored);
        let repeated =
            updates::wait_pair(origin, repeated_after, &trial.base[0], &trial.base[1]).await;
        updates::write_stage(root, "repeated-own-base", &repeated);
        disable_base(trial).await;
    }
    let off = hold_off(origin, &trial.base[0]).await;
    write_new(
        root,
        "scope.json",
        &serde_json::to_vec_pretty(&json!({
        "interval": {"start":interval.start,"finish":interval.finish,"playout_id":95,
            "channel_id":3,"profile_id":trial.target.id,"profile_revision":trial.target.revision},
        "pending_window": pending, "on_window": on, "off_window": off,
        "base_disabled_during_scope": disable_during_scope,
            "source_channel":1, "mirror_output":3, "requests":[1,0,1], "capacity":2
        }))
        .unwrap(),
    )
    .await;
    Outcome {
        interval,
        scoped,
        pending,
        on,
        off,
    }
}

async fn verify_wire(
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
        if index == 0 {
            assert_eq!(gaps, 0, "source channel clock never changes");
        }
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
        write_new(root, &format!("viewer-{index}-clock.json"), &serde_json::to_vec(&json!({
            "qualified_authored_pts_gaps":gaps, "scope":"Full original raw clock/PMT-Off oracle, not zero total TSDuck leaps"
        })).unwrap()).await;
        if index == 1 {
            for (name, (start, end)) in [("pending", outcome.pending), ("off", outcome.off)] {
                let plain = &bytes[start..end];
                verify_plain(tools, source, plain).await;
                write_new(root, &format!("{name}-plain.ts"), plain).await;
            }
            let scoped = &bytes[outcome.on.0..outcome.on.1];
            write_new(root, "scoped-only.ts", scoped).await;
            let decoded = client_capture::decode_page_text(&tools.ffmpeg, scoped, "200").await;
            assert!(
                decoded.contains("SCOPED MIRROR"),
                "independently decode the selected mirror-only label"
            );
            write_new(root, "scoped-independent.srt", decoded.as_bytes()).await;
        }
    }
}

async fn verify_state(mut trial: Trial, outcome: &Outcome) -> Vec<u8> {
    let fixture = &mut trial.fixture;
    assert_eq!(designs::config(fixture).await, trial.configuration);
    assert_eq!(
        calculate_addon_content_sha256(&fixture.root.path().join("package"))
            .await
            .unwrap(),
        trial.package
    );
    assert_eq!(
        tokio::fs::read(fixture.root.path().join("ordinary-calls"))
            .await
            .unwrap(),
        b"day\nnight\nday\n"
    );
    assert!(fixture.playout.try_recv().is_err());
    trial.fixture = trial.fixture.reopen().await;
    let fixture = &trial.fixture;
    for (index, channel) in [1, 2].into_iter().enumerate() {
        assert_eq!(
            fixture
                .state
                .application
                .channel_teletext_bindings(channel)
                .await
                .unwrap(),
            trial.unchanged[index]
        );
    }
    for (index, channel) in [1, 3].into_iter().enumerate() {
        assert_eq!(
            fixture
                .state
                .application
                .accepted_teletext_generation(channel)
                .await
                .unwrap(),
            Some(trial.base[index].generation().clone())
        );
    }
    assert_eq!(
        fixture
            .state
            .application
            .effective_teletext_accepted_generation(3)
            .await
            .unwrap(),
        Some(outcome.scoped.generation().clone())
    );
    let bindings = fixture
        .state
        .application
        .channel_teletext_bindings(3)
        .await
        .unwrap();
    assert_eq!(bindings.len(), 1);
    assert!(!bindings[0].binding.enabled);
    assert_eq!(bindings[0].binding.profile_id, trial.mirror.profile_id);
    assert_eq!(bindings[0].binding.revision, trial.mirror.revision + 1);
    let repository = SqlitePlayoutRepository(fixture.state.pool.clone());
    assert_eq!(
        repository
            .published_teletext_intervals_at(3, outcome.interval.start)
            .await
            .unwrap(),
        std::slice::from_ref(&outcome.interval)
    );
    assert!(repository
        .published_teletext_intervals_at(3, outcome.interval.finish)
        .await
        .unwrap()
        .is_empty());
    fixture.state.pool.close().await;
    trial.source
}

async fn run_trial(root_setting: &str, disable_during_scope: bool) {
    let root = PathBuf::from(client_capture::selected(root_setting));
    tokio::fs::create_dir(&root).await.unwrap();
    live_tcp::terminal_trace(&root).unwrap();
    let tools = Tools::load().await;
    let trial = setup(&tools).await;
    write_new(&root, "source.ts", &trial.source).await;
    reset_source_start(&trial.fixture).await;
    let mut origin = live_tcp::Origin::start(&trial.fixture, &root)
        .await
        .unwrap();
    let mut observers = [
        live_tcp::Observer::start_for(&origin, 1, "day"),
        live_tcp::Observer::start_for(&origin, 3, "night"),
    ];
    let outcome = AssertUnwindSafe(async {
        tokio::time::timeout(
            Duration::from_secs(30),
            transition(&trial, &root, &origin, disable_during_scope),
        )
        .await
        .expect("bounded mirror scoped On/expiry/Off trial")
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
        trial.fixture.state.pool.close().await;
        std::panic::resume_unwind(panic);
    }
    let outcome = match outcome {
        Ok(value) => value,
        Err(panic) => {
            trial.fixture.state.pool.close().await;
            std::panic::resume_unwind(panic);
        }
    };
    let source = verify_state(trial, &outcome).await;
    verify_wire(&tools, &root, &source, &origin, &outcome).await;
    write_new(&root, "success.json", &serde_json::to_vec_pretty(&json!({
        "scope":"Actual source1/mirror3 retained HTTP responses/default2; own Lua scope/pending/publication/Base expiry/Off, no GUI client or full C/platform/L15/9A acceptance",
        "requests":[1,0,1], "capacity":2, "explicit_scoped_publication":true,
        "no_source_teletext_inheritance":true, "full_model_clock_av_tsduck_cleanup":true,
        "automatic_own_base_recovery":!disable_during_scope,
        "base_disabled_during_scope":disable_during_scope,
        "expiry_to_current_base_off":disable_during_scope,
        "no_implicit_producer_replay":true
    })).unwrap()).await;
}

#[tokio::test]
#[ignore = "requires pinned FFmpeg9/TSDuck and fresh ERSATZRS_TELETEXT_MIRROR_LUA_DIR; no suitable GUI client claim"]
async fn retained_mirror_owns_scoped_profile_base_expiry_and_off_without_source_inheritance() {
    run_trial("ERSATZRS_TELETEXT_MIRROR_LUA_DIR", false).await;
}

#[tokio::test]
#[ignore = "requires pinned FFmpeg9/TSDuck and fresh ERSATZRS_TELETEXT_LUA_BASE_OFF_DIR; no suitable GUI client claim"]
async fn retained_lua_on_overrides_admin_base_off_until_original_expiry() {
    run_trial("ERSATZRS_TELETEXT_LUA_BASE_OFF_DIR", true).await;
}
