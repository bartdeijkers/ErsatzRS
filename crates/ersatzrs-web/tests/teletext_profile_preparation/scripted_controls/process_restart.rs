//! A fresh OS process observes published Lua scopes without replaying Lua.
//! Native application/storage proof only; no server startup or stream claim.

use super::super::{client_capture, saved};
use super::{
    assert_no_publication_or_invocation, build, continue_after_reopen::Configuration,
    publish_result, seed_publication_inputs, Fixture, ADDON,
};
use chrono::{DateTime, Duration, TimeZone as _, Utc};
use ersatzrs_core::{
    repositories::PlayoutRepository as _,
    scheduling::{
        PlayoutBuildMode, SchedulingMediaItem, SchedulingRequest, ScriptedTeletextControl,
    },
};
use ersatzrs_domain::{
    TeletextEffectiveBindingSnapshot as Binding, TeletextInstallationSnapshot,
    TeletextOverrideState, TeletextProfileCandidateSnapshot, TeletextProfileSetting,
    TeletextProfileValue,
};
use ersatzrs_infra::repositories::{SqlitePlayoutRepository, SqliteSchedulingContentRepository};
use ersatzrs_web::host::state::AppState;
use serde_json::{json, Value};
use std::{collections::BTreeMap, path::Path, sync::Arc};

const ROOT: &str = "ERSATZRS_TELETEXT_RESTART_FIXTURE";
const CHILD: &str = "scripted_controls::process_restart::observe_and_continue_in_fresh_process";

fn settings(values: &[TeletextProfileSetting]) -> Value {
    json!(values
        .iter()
        .map(|setting| {
            let TeletextProfileValue::Literal(value) = &setting.value else {
                panic!("this synthetic fixture contains no secret references")
            };
            json!([setting.key, value])
        })
        .collect::<Vec<_>>())
}

fn installation(value: &TeletextInstallationSnapshot) -> Value {
    json!([
        value.addon_id,
        value.state_revision,
        value.version,
        value.package_sha256,
        value.content_sha256,
        value.manifest_sha256
    ])
}

fn candidate(value: &TeletextProfileCandidateSnapshot) -> Value {
    json!({
        "channel":value.channel_id,"addon":value.addon_id,
        "profile":value.profile_id,"revision":value.profile_revision,
        "contract":value.contract.schema_version(),
        "installation":installation(&value.installation),
        "settings":settings(&value.effective_settings)
    })
}

fn start() -> DateTime<Utc> {
    Utc.with_ymd_and_hms(2026, 10, 14, 12, 0, 0).unwrap()
}

async fn observations(state: &AppState) -> Value {
    let mut result = Vec::new();
    for channel in [1, 2] {
        // Half-open boundaries plus genuine interior observations. These are
        // supplied application clocks, not a claim to advance the host clock.
        for millis in [
            -1, 0, 30_000, 59_999, 60_000, 90_000, 119_999, 120_000, 120_001,
        ] {
            let selected = saved(
                state
                    .application
                    .observe_effective_channel_teletext_selection(
                        channel,
                        start() + Duration::milliseconds(millis),
                    )
                    .await
                    .unwrap(),
            );
            assert_eq!(selected.channel_id, channel);
            assert_eq!(selected.bindings.len(), 1);
            let expected_boundary = if millis < 0 {
                Some(start())
            } else if millis < 60_000 {
                Some(start() + Duration::minutes(1))
            } else if millis < 120_000 {
                Some(start() + Duration::minutes(2))
            } else {
                None
            };
            assert_eq!(selected.valid_until, expected_boundary);
            let binding = match &selected.bindings[0] {
                Binding::Base(base) => {
                    assert!(!(0..120_000).contains(&millis));
                    json!({"origin":"base","binding_revision":base.binding_revision,
                    "candidate":candidate(&TeletextProfileCandidateSnapshot {
                        channel_id:base.channel_id, addon_id:base.addon_id.clone(),
                        profile_id:base.profile_id, profile_revision:base.profile_revision,
                        contract:base.contract, installation:base.installation.clone(),
                        effective_settings:base.effective_settings.clone()
                    })})
                }
                Binding::Scripted(published) => {
                    assert_eq!(published.playout_id, 92 + channel);
                    let (offset, value) = match &published.interval.state {
                        TeletextOverrideState::Enabled(value) => {
                            assert!((0..60_000).contains(&millis));
                            let expected = if channel == 1 { "night" } else { "day" };
                            assert!(value
                                .effective_settings
                                .iter()
                                .any(|setting| setting.key == "THEME"
                                    && setting.value
                                        == TeletextProfileValue::Literal(expected.to_owned())));
                            (0, candidate(value))
                        }
                        TeletextOverrideState::Off {
                            channel_id,
                            addon_id,
                        } => {
                            assert!((60_000..120_000).contains(&millis));
                            assert_eq!((*channel_id, addon_id.as_str()), (channel, ADDON));
                            (1, Value::Null)
                        }
                    };
                    assert_eq!(
                        published.interval.start,
                        start() + Duration::minutes(offset)
                    );
                    assert_eq!(
                        published.interval.finish,
                        start() + Duration::minutes(offset + 1)
                    );
                    json!({"origin":"scripted","playout":published.playout_id,
                        "start":published.interval.start,"finish":published.interval.finish,
                        "candidate":value})
                }
            };
            result.push(json!({"channel":channel,"millis":millis,
                "valid_until":selected.valid_until,"binding":binding}));
        }
    }
    json!(result)
}

#[tokio::test]
async fn lua_scope_observations_survive_actual_process_restart_without_replay() {
    let mut fixture = Fixture::new_persistent().await;
    let day = fixture.profile("Restart Day", "day", "good").await;
    let night = fixture.profile("Restart Night", "night", "good").await;
    fixture.bind(1, &day, None).await;
    fixture.bind(2, &night, None).await;
    seed_publication_inputs(&fixture).await;
    for (channel, target) in [(1, night.id), (2, day.id)] {
        let script = format!(
            "if build_mode() == 'reset' then\n\
             assert(teletext_profile('{ADDON}', {target}).ok) end\n\
             assert(teletext_enabled('{ADDON}', true).ok)\nadd_count('clip',1)\n\
             assert(teletext_enabled('{ADDON}', false).ok)\nadd_count('clip',1)"
        );
        let result = build(&fixture, &script, channel, true).await.unwrap();
        assert_eq!(result.added_items.len(), 2);
        assert_eq!(result.teletext_intervals.len(), 2);
        publish_result(&fixture, &result).await;
    }
    let configuration = Configuration::read(&fixture).await;
    let before = observations(&fixture.state).await;
    for (index, profile) in [(0, day.id), (9, night.id)] {
        for offset in [0, 7, 8] {
            assert_eq!(
                before[index + offset]["binding"]["candidate"]["profile"],
                profile
            );
        }
    }
    std::fs::write(
        fixture.root.path().join("expected.json"),
        serde_json::to_vec(&before).unwrap(),
    )
    .unwrap();
    assert_no_publication_or_invocation(&mut fixture).await;
    // No parent application/pool/cache remains available to the child.
    fixture.state.pool.close().await;
    let root = fixture.root;
    drop(fixture.state);
    drop(fixture.playout);
    let output = tokio::time::timeout(
        std::time::Duration::from_secs(45),
        tokio::process::Command::new(std::env::current_exe().unwrap())
            .kill_on_drop(true)
            .args(["--exact", CHILD, "--ignored", "--nocapture"])
            .env(ROOT, root.path())
            .output(),
    )
    .await
    .unwrap()
    .unwrap();
    assert!(
        output.status.success(),
        "{}{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
    let child: Value =
        serde_json::from_slice(&std::fs::read(root.path().join("observed.json")).unwrap()).unwrap();
    assert_ne!(child["pid"], std::process::id());
    assert_eq!(before, child["observations"]);
    println!("process-restart: parent={} child={} exact_observations=18; fresh Continue=Day/Off; no publication/replay", std::process::id(), child["pid"]);
    let pool = Fixture::open_persistent_pool(root.path()).await;
    let (state, playout) = Fixture::state_from_pool(pool);
    let mut fixture = Fixture {
        root,
        state,
        playout,
    };
    assert_eq!(Configuration::read(&fixture).await, configuration);
    assert_eq!(observations(&fixture.state).await, before);
    assert_no_publication_or_invocation(&mut fixture).await;
}

#[tokio::test]
#[ignore = "owned by lua_scope_observations process-restart regression"]
async fn observe_and_continue_in_fresh_process() {
    let root = client_capture::selected(ROOT);
    let root = Path::new(&root);
    let pool = Fixture::open_persistent_pool(root).await;
    let (state, mut notification) = Fixture::state_from_pool(pool);
    let observed = observations(&state).await;
    let expected: Value =
        serde_json::from_slice(&std::fs::read(root.join("expected.json")).unwrap()).unwrap();
    assert_eq!(observed, expected);
    let repository = SqlitePlayoutRepository(state.pool.clone());
    let playout = repository.get_with_anchor(93).await.unwrap();
    let mut reference = SqliteSchedulingContentRepository::new(state.pool.clone())
        .scripted_reference_data(93, BTreeMap::new(), None)
        .await
        .unwrap();
    assert_eq!(reference.existing_items.len(), 2);
    assert_eq!(reference.existing_scripted_checkpoints.len(), 2);
    let boundary = start() + Duration::minutes(2);
    assert_eq!(playout.anchor.as_ref().unwrap().next_start, boundary);
    assert_eq!(
        reference
            .existing_scripted_checkpoints
            .last()
            .unwrap()
            .checkpoint_date,
        boundary
    );
    reference.media_by_key.insert(
        "clip".into(),
        vec![SchedulingMediaItem::new(71, Duration::minutes(1))],
    );
    let controls: Arc<dyn ScriptedTeletextControl> = state.application.clone();
    let builder = ersatzrs_infra::scripted_scheduling::playout_builder_with_teletext(
        state.pool.clone(),
        tokio::runtime::Handle::current(),
        None,
        Some(controls),
    );
    let request = SchedulingRequest {
        playout,
        start: start(),
        finish: boundary + Duration::minutes(2),
        mode: PlayoutBuildMode::Continue,
        reference_data: reference,
    };
    let result = tokio::task::spawn_blocking(move || builder.build(request))
        .await
        .unwrap()
        .unwrap();
    assert_eq!(result.added_items.len(), 2);
    assert_eq!(result.added_items[0].start, boundary);
    assert_eq!(result.teletext_intervals.len(), 2);
    let TeletextOverrideState::Enabled(fresh) = &result.teletext_intervals[0].state else {
        panic!("Continue must start from enabled admin base")
    };
    let base = saved(state.application.teletext_base(1, ADDON).await.unwrap());
    assert_eq!(fresh.profile_id, base.profile_id);
    assert_eq!(
        *fresh,
        saved(
            state
                .application
                .teletext_candidate(1, ADDON, base.profile_id)
                .await
                .unwrap()
        )
    );
    assert!(fresh
        .effective_settings
        .iter()
        .any(|setting| setting.key == "THEME"
            && setting.value == TeletextProfileValue::Literal("day".into())));
    for (index, (item, interval)) in result
        .added_items
        .iter()
        .zip(&result.teletext_intervals)
        .enumerate()
    {
        let offset = i64::try_from(index).unwrap();
        assert_eq!(item.start, boundary + Duration::minutes(offset));
        assert_eq!(item.finish, boundary + Duration::minutes(offset + 1));
        assert_eq!((interval.start, interval.finish), (item.start, item.finish));
    }
    assert!(matches!(&result.teletext_intervals[1].state,
        TeletextOverrideState::Off {channel_id:1, addon_id} if addon_id == ADDON));
    // An uncommitted continuation cannot change the persisted observations.
    assert_eq!(observations(&state).await, expected);
    assert!(notification.try_recv().is_err());
    for theme in ["day", "night"] {
        assert!(!root.join(format!("calls-{theme}")).exists());
    }
    assert_eq!(std::fs::read_dir(root.join("package")).unwrap().count(), 1);
    write_observations(root, &observed);
    state.pool.close().await;
}

fn write_observations(root: &Path, observed: &Value) {
    let mut file = std::fs::OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(root.join("observed.json"))
        .unwrap();
    serde_json::to_writer(
        &mut file,
        &json!({"pid":std::process::id(),
        "observations":observed}),
    )
    .unwrap();
}
