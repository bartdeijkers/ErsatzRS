use std::sync::Mutex;

use ersatzrs_domain::{
    TeletextInstallationSnapshot, TeletextOverrideState, TeletextPageContract,
    TeletextProfileCandidateSnapshot, TeletextProfileSetting, TeletextProfileValue,
    TeletextScriptedBaseSelection, MAX_TELETEXT_OVERRIDE_COMMANDS,
    MAX_TELETEXT_OVERRIDE_SNAPSHOT_BYTES,
};

use super::*;
use crate::repositories::{RepositoryError, Result, TeletextProfileMutation as Mutation};
use crate::scheduling::{SchedulingContentSource, SchedulingMediaItem};

const ADDON: &str = "org.example.teletext";

#[derive(Debug)]
struct Controls {
    enabled: bool,
    base_profile: EntityId,
    foreign_base: bool,
    foreign_candidate: bool,
    identity_bytes: usize,
    base_calls: Mutex<Vec<EntityId>>,
    candidate_calls: Mutex<Vec<(EntityId, EntityId)>>,
}

impl Default for Controls {
    fn default() -> Self {
        Self {
            enabled: true,
            base_profile: 11,
            foreign_base: false,
            foreign_candidate: false,
            identity_bytes: 0,
            base_calls: Mutex::new(Vec::new()),
            candidate_calls: Mutex::new(Vec::new()),
        }
    }
}

impl ScriptedContentResolver for Controls {
    fn media_items(&self, _: &SchedulingContentSource) -> Result<Vec<SchedulingMediaItem>> {
        Ok(vec![SchedulingMediaItem::new(1, Duration::minutes(3))])
    }
    fn playlist_items(
        &self,
        _: &SchedulingContentSource,
    ) -> Result<Option<Vec<SchedulingPlaylistItem>>> {
        Ok(None)
    }
    fn watermark_id(&self, _: &str) -> Result<Option<EntityId>> {
        Ok(None)
    }
    fn graphics_id(&self, _: &str) -> Result<Option<EntityId>> {
        Ok(None)
    }
    fn teletext_base(
        &self,
        channel: EntityId,
        addon: &str,
    ) -> Result<Mutation<TeletextScriptedBaseSelection>> {
        self.base_calls.lock().unwrap().push(channel);
        Ok(Mutation::Saved(TeletextScriptedBaseSelection {
            channel_id: if self.foreign_base { 777 } else { channel },
            addon_id: addon.to_owned(),
            profile_id: self.base_profile,
            enabled: self.enabled,
        }))
    }
    fn teletext_candidate(
        &self,
        channel: EntityId,
        addon: &str,
        profile: EntityId,
    ) -> Result<Mutation<TeletextProfileCandidateSnapshot>> {
        self.candidate_calls
            .lock()
            .unwrap()
            .push((channel, profile));
        if profile == 98 {
            return Err(RepositoryError::unavailable(
                "fixture",
                "private fixture detail",
            ));
        }
        if ![11, 12].contains(&profile) {
            return Ok(Mutation::Rejected(
                crate::repositories::TeletextProfileRejection::MissingProfile,
            ));
        }
        Ok(Mutation::Saved(TeletextProfileCandidateSnapshot {
            contract: TeletextPageContract::V3,
            channel_id: if self.foreign_candidate { 777 } else { channel },
            addon_id: addon.to_owned(),
            profile_id: profile,
            profile_revision: 1,
            installation: TeletextInstallationSnapshot {
                addon_id: addon.to_owned(),
                state_revision: "state".to_owned(),
                version: "1.0.0".to_owned(),
                package_sha256: "p".repeat(self.identity_bytes),
                content_sha256: "content".to_owned(),
                manifest_sha256: "manifest".to_owned(),
            },
            effective_settings: vec![TeletextProfileSetting {
                key: "THEME".to_owned(),
                value: TeletextProfileValue::Literal(
                    if profile == 11 { "day" } else { "night" }.to_owned(),
                ),
            }],
        }))
    }
}

fn request() -> SchedulingRequest {
    let mut request = continuation_tests::request();
    request.playout.channel_id = 7;
    request
}

fn run(script: &str, request: SchedulingRequest, controls: Arc<Controls>) -> SchedulingResult {
    ScriptedSchedulingEngine::schedule_script_resolved(script, request, Some(controls), None)
        .unwrap()
}

const CONTENT: &str = "add_collection('a', 'series')";

#[test]
fn lua_profile_off_intervals_use_raw_cursor_and_actual_item_finish() {
    let request = request();
    let controls = Arc::new(Controls::default());
    let result = run(&format!("{CONTENT}; assert(teletext_profile('{ADDON}',12).ok); add_count('a',1); assert(teletext_enabled('{ADDON}',false).ok); add_count('a',1)"), request.clone(), controls.clone());
    assert_eq!(result.teletext_intervals.len(), 2);
    let first = &result.teletext_intervals[0];
    assert_eq!(first.start, request.start);
    assert_eq!(first.finish, result.added_items[0].finish);
    assert!(
        matches!(&first.state, TeletextOverrideState::Enabled(candidate) if candidate.channel_id == 7 && candidate.profile_id == 12)
    );
    let last = &result.teletext_intervals[1];
    assert_eq!(last.start, first.finish);
    assert_eq!(last.finish, result.added_items[1].finish);
    assert!(last.finish < request.finish);
    assert!(matches!(
        &last.state,
        TeletextOverrideState::Off { channel_id: 7, .. }
    ));
    assert_eq!(*controls.base_calls.lock().unwrap(), [7, 7]);
    assert_eq!(*controls.candidate_calls.lock().unwrap(), [(7, 12)]);
}

#[test]
fn profile_preserves_off_and_enable_uses_latest_successful_selection() {
    let controls = Arc::new(Controls {
        enabled: false,
        ..Controls::default()
    });
    let result = run(&format!("{CONTENT}; assert(teletext_profile('{ADDON}',12).ok); add_count('a',1); assert(teletext_enabled('{ADDON}',true).ok); add_count('a',1)"), request(), controls.clone());
    assert!(matches!(
        &result.teletext_intervals[0].state,
        TeletextOverrideState::Off { .. }
    ));
    assert!(
        matches!(&result.teletext_intervals[1].state, TeletextOverrideState::Enabled(candidate) if candidate.profile_id == 12)
    );
    assert_eq!(
        *controls.candidate_calls.lock().unwrap(),
        [(7, 12), (7, 12)]
    );
}

#[test]
fn off_never_requires_a_healthy_profile_or_candidate_lookup() {
    let controls = Arc::new(Controls {
        base_profile: 99,
        ..Controls::default()
    });
    let result = run(
        &format!("{CONTENT}; assert(teletext_enabled('{ADDON}',false).ok); add_count('a',1)"),
        request(),
        controls.clone(),
    );
    assert!(matches!(
        &result.teletext_intervals[0].state,
        TeletextOverrideState::Off { .. }
    ));
    assert!(controls.candidate_calls.lock().unwrap().is_empty());
}

#[test]
fn handled_rejections_preserve_prior_selection_and_uncaught_error_discards_result() {
    let controls = Arc::new(Controls::default());
    let script = format!("{CONTENT}; assert(teletext_profile('{ADDON}',12).ok); add_count('a',1); local rejected=teletext_profile('{ADDON}',99); assert(not rejected.ok and rejected.code=='missing_profile'); local unavailable=teletext_profile('{ADDON}',98); assert(not unavailable.ok and unavailable.code=='unavailable'); assert(teletext_enabled('{ADDON}',true).ok); add_count('a',1)");
    let result = run(&script, request(), controls.clone());
    assert!(result.teletext_intervals.iter().all(|interval| matches!(&interval.state, TeletextOverrideState::Enabled(candidate) if candidate.profile_id == 12)));
    let failed = ScriptedSchedulingEngine::schedule_script_resolved(
        &format!("{script}; error('uncaught fixture failure')"),
        request(),
        Some(controls),
        None,
    );
    assert!(matches!(failed, Err(SchedulingError::Script(_))));
}

#[test]
fn same_cursor_latest_command_wins_and_no_intervals_escape_actual_finish() {
    let script = format!("{CONTENT}; assert(teletext_enabled('{ADDON}',false).ok); assert(teletext_profile('{ADDON}',12).ok); assert(teletext_enabled('{ADDON}',true).ok); add_count('a',1); assert(teletext_enabled('{ADDON}',false).ok)");
    let result = run(&script, request(), Arc::new(Controls::default()));
    assert_eq!(result.teletext_intervals.len(), 1);
    assert!(
        matches!(&result.teletext_intervals[0].state, TeletextOverrideState::Enabled(candidate) if candidate.profile_id == 12)
    );
    assert_eq!(
        result.teletext_intervals[0].finish,
        result.added_items[0].finish
    );
    let empty = run(
        &format!("assert(teletext_enabled('{ADDON}',false).ok); wait_until('00:01:00')"),
        request(),
        Arc::new(Controls::default()),
    );
    assert!(empty.teletext_intervals.is_empty());
}

#[test]
fn teletext_arguments_reject_coercion_foreign_channel_and_integer_overflow() {
    let controls = Arc::new(Controls::default());
    let script = format!("{CONTENT}; local function bad(result) assert(not result.ok and result.code=='invalid_argument') end; bad(teletext_profile('{ADDON}','11')); bad(teletext_profile('{ADDON}',11.0)); bad(teletext_profile('{ADDON}',1.5)); bad(teletext_profile('{ADDON}',9223372036854775808)); bad(teletext_profile('{ADDON}',0)); bad(teletext_profile(7,11)); bad(teletext_enabled('{ADDON}','false')); bad(teletext_enabled('{ADDON}',1)); bad(teletext_enabled('{ADDON}',nil)); bad(teletext_enabled('{ADDON}',false,777)); add_count('a',1)");
    let result = run(&script, request(), controls.clone());
    assert!(result.teletext_intervals.is_empty());
    assert!(controls.base_calls.lock().unwrap().is_empty());
    assert!(controls.candidate_calls.lock().unwrap().is_empty());
}

#[test]
fn foreign_adapter_observations_never_become_channel_authority() {
    for (foreign_base, foreign_candidate) in [(true, false), (false, true)] {
        let result = run(&format!("{CONTENT}; local r=teletext_profile('{ADDON}',12); assert(not r.ok and r.code=='invalid_response'); add_count('a',1)"), request(), Arc::new(Controls { foreign_base, foreign_candidate, ..Controls::default() }));
        assert!(result.teletext_intervals.is_empty());
    }
}

#[test]
fn teletext_command_and_all_identity_byte_budgets_preserve_accepted_state() {
    let controls = Arc::new(Controls::default());
    let result = run(&format!("{CONTENT}; for i=1,{MAX_TELETEXT_OVERRIDE_COMMANDS} do assert(teletext_enabled('{ADDON}',false).ok) end; local r=teletext_enabled('{ADDON}',true); assert(not r.ok and r.code=='command_limit'); add_count('a',1)"), request(), controls.clone());
    assert_eq!(
        controls.base_calls.lock().unwrap().len(),
        MAX_TELETEXT_OVERRIDE_COMMANDS
    );
    assert!(matches!(
        &result.teletext_intervals[0].state,
        TeletextOverrideState::Off { .. }
    ));
    let result = run(&format!("{CONTENT}; assert(teletext_enabled('{ADDON}',false).ok); local r=teletext_profile('{ADDON}',12); assert(not r.ok and r.code=='snapshot_limit'); add_count('a',1)"), request(), Arc::new(Controls { identity_bytes: MAX_TELETEXT_OVERRIDE_SNAPSHOT_BYTES, ..Controls::default() }));
    assert!(matches!(
        &result.teletext_intervals[0].state,
        TeletextOverrideState::Off { .. }
    ));
}

#[test]
fn fresh_continue_starts_from_current_base_not_prior_candidate_globals() {
    let original = request();
    let first = run(
        &format!("{CONTENT}; assert(teletext_profile('{ADDON}',12).ok); add_count('a',1)"),
        original.clone(),
        Arc::new(Controls::default()),
    );
    let mut continued = original;
    continued.mode = PlayoutBuildMode::Continue;
    continued.playout.anchor = first.anchor;
    continued.reference_data.existing_items = first.added_items;
    continued.reference_data.existing_history = first.added_history;
    let result = run(
        &format!("{CONTENT}; assert(teletext_enabled('{ADDON}',true).ok); add_count('a',1)"),
        continued,
        Arc::new(Controls::default()),
    );
    assert!(
        matches!(&result.teletext_intervals[0].state, TeletextOverrideState::Enabled(candidate) if candidate.profile_id == 11)
    );
}

#[test]
fn unsupported_controls_are_explicit_and_do_not_create_overrides() {
    let result = ScriptedSchedulingEngine.schedule_script(&format!("{CONTENT}; local r=teletext_enabled('{ADDON}',false); assert(not r.ok and r.code=='unsupported'); add_count('a',1)"), request()).unwrap();
    assert!(result.teletext_intervals.is_empty());
}

#[test]
fn aggregate_snapshot_budget_counts_replaced_same_cursor_candidates() {
    let result = run(
        &format!("{CONTENT}; assert(teletext_profile('{ADDON}',12).ok); local r=teletext_profile('{ADDON}',11); assert(not r.ok and r.code=='snapshot_limit'); add_count('a',1)"),
        request(),
        Arc::new(Controls { identity_bytes: MAX_TELETEXT_OVERRIDE_SNAPSHOT_BYTES / 2, ..Controls::default() }),
    );
    assert_eq!(result.teletext_intervals.len(), 1);
    assert!(
        matches!(&result.teletext_intervals[0].state, TeletextOverrideState::Enabled(candidate) if candidate.profile_id == 12)
    );
}

#[test]
fn two_output_channels_keep_distinct_profiles_and_themes() {
    let controls = Arc::new(Controls::default());
    for (channel, profile, theme) in [(7, 11, "day"), (8, 12, "night")] {
        let mut request = request();
        request.playout.channel_id = channel;
        let result = run(
            &format!(
                "{CONTENT}; assert(teletext_profile('{ADDON}',{profile}).ok); add_count('a',1)"
            ),
            request,
            controls.clone(),
        );
        let TeletextOverrideState::Enabled(candidate) = &result.teletext_intervals[0].state else {
            panic!("missing enabled candidate");
        };
        assert_eq!(candidate.channel_id, channel);
        assert_eq!(candidate.profile_id, profile);
        assert_eq!(
            candidate.effective_settings[0].value,
            TeletextProfileValue::Literal(theme.to_owned())
        );
    }
    assert_eq!(
        *controls.candidate_calls.lock().unwrap(),
        [(7, 11), (8, 12)]
    );
}

#[test]
fn refresh_and_reset_events_start_at_existing_preservation_seams() {
    let original = request();
    let first = run(
        &format!("{CONTENT}; add_count('a',1)"),
        original.clone(),
        Arc::new(Controls::default()),
    );
    let old_finish = first.added_items[0].finish;
    for mode in [PlayoutBuildMode::Refresh, PlayoutBuildMode::Reset] {
        let mut request = original.clone();
        request.mode = mode;
        request.playout.anchor = first.anchor.clone();
        request.reference_data.existing_items = first.added_items.clone();
        request.reference_data.existing_history = first.added_history.clone();
        request.reference_data.existing_scripted_checkpoints = first.scripted_checkpoints.clone();
        let result = run(
            &format!("{CONTENT}; assert(teletext_enabled('{ADDON}',false).ok); add_count('a',1)"),
            request,
            Arc::new(Controls::default()),
        );
        let expected_start = if mode == PlayoutBuildMode::Refresh {
            old_finish
        } else {
            original.start
        };
        assert_eq!(result.teletext_intervals[0].start, expected_start);
        assert_eq!(
            result.teletext_intervals[0].finish,
            result.added_items[0].finish
        );
        assert_eq!(
            result.teletext_intervals[0].start,
            result.added_items[0].start
        );
    }
}

#[test]
fn backward_command_rejection_cannot_replace_prior_candidate() {
    let result = run(
        &format!("{CONTENT}; assert(teletext_profile('{ADDON}',12).ok); add_count('a',1); assert(teletext_enabled('{ADDON}',false).ok); wait_until('00:01:00'); local r=teletext_enabled('{ADDON}',true); assert(not r.ok and r.code=='invalid_cursor')"),
        request(),
        Arc::new(Controls::default()),
    );
    assert_eq!(result.teletext_intervals.len(), 1);
    assert!(
        matches!(&result.teletext_intervals[0].state, TeletextOverrideState::Enabled(candidate) if candidate.profile_id == 12)
    );
}
