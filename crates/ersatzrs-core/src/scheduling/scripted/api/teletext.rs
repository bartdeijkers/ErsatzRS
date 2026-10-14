//! Candidate-only controls. Each returns {ok=true} or {ok=false, code=static}.

use ersatzrs_domain::{
    EntityId, TeletextOverrideInterval, TeletextOverrideState, MAX_TELETEXT_OVERRIDE_COMMANDS,
    MAX_TELETEXT_OVERRIDE_SNAPSHOT_BYTES,
};
use mlua::{MultiValue, Value};

use crate::repositories::{TeletextProfileMutation, TeletextProfileRejection};

use super::super::{BTreeMap, DateTime, Lua, Rc, RefCell, ScriptState, Utc};

const MAX_ADDON_ID_BYTES: usize = 256;

#[derive(Clone, Copy)]
enum CommandKind {
    Profile,
    Enabled,
}

#[derive(Debug, Clone)]
struct Selection {
    profile_id: EntityId,
    enabled: bool,
    last_at: DateTime<Utc>,
}

#[derive(Debug, Clone, Default)]
pub(in crate::scheduling::scripted) struct CandidateCommands {
    attempts: usize,
    snapshot_bytes: usize,
    selections: BTreeMap<String, Selection>,
    events: BTreeMap<String, BTreeMap<DateTime<Utc>, TeletextOverrideState>>,
}

impl CandidateCommands {
    pub(in crate::scheduling::scripted) fn intervals(
        &self,
        actual_finish: DateTime<Utc>,
    ) -> Vec<TeletextOverrideInterval> {
        let mut intervals = Vec::new();
        for events in self.events.values() {
            let mut events = events.range(..actual_finish).peekable();
            while let Some((start, state)) = events.next() {
                let finish = events.peek().map_or(actual_finish, |(at, _)| **at);
                intervals.push(TeletextOverrideInterval {
                    start: *start,
                    finish,
                    state: state.clone(),
                });
            }
        }
        intervals.sort_by(|a, b| {
            a.start
                .cmp(&b.start)
                .then_with(|| a.state.addon_id().cmp(b.state.addon_id()))
        });
        intervals
    }
}

pub(in crate::scheduling::scripted) fn register(
    lua: &Lua,
    state: &Rc<RefCell<ScriptState>>,
) -> mlua::Result<()> {
    for (name, kind) in [
        ("teletext_profile", CommandKind::Profile),
        ("teletext_enabled", CommandKind::Enabled),
    ] {
        let state = Rc::clone(state);
        lua.globals().set(
            name,
            lua.create_function_mut(move |lua, args: MultiValue| {
                let result = state.borrow_mut().teletext_command(&args, kind);
                let status = lua.create_table()?;
                status.set("ok", result.is_ok())?;
                if let Err(code) = result {
                    status.set("code", code)?;
                }
                Ok(status)
            })?,
        )?;
    }
    Ok(())
}

impl ScriptState {
    fn teletext_command(
        &mut self,
        args: &MultiValue,
        kind: CommandKind,
    ) -> Result<(), &'static str> {
        self.teletext.attempts = self.teletext.attempts.saturating_add(1);
        if self.teletext.attempts > MAX_TELETEXT_OVERRIDE_COMMANDS {
            return Err("command_limit");
        }
        if args.len() != 2 {
            return Err("invalid_argument");
        }
        let Some(Value::String(addon)) = args.front() else {
            return Err("invalid_argument");
        };
        let addon = addon.to_str().map_err(|_| "invalid_argument")?;
        if addon.is_empty() || addon.len() > MAX_ADDON_ID_BYTES || addon.contains('\0') {
            return Err("invalid_argument");
        }
        let addon: &str = addon.as_ref();
        let requested_profile = match (kind, args.get(1)) {
            (CommandKind::Profile, Some(Value::Integer(id))) if *id > 0 => Some(*id),
            (CommandKind::Enabled, Some(Value::Boolean(_))) => None,
            _ => return Err("invalid_argument"),
        };
        if self.channel_id <= 0 || self.current_time < self.start {
            return Err("invalid_cursor");
        }
        let previous = self.teletext.selections.get(addon);
        if previous.is_some_and(|selection| self.current_time < selection.last_at) {
            return Err("invalid_cursor");
        }
        let resolver = self.resolver.as_ref().ok_or("unsupported")?;
        let base = accepted(resolver.teletext_base(self.channel_id, addon))?;
        if base.channel_id != self.channel_id || base.addon_id != addon {
            return Err("invalid_response");
        }
        let profile_id = requested_profile
            .or_else(|| previous.map(|selection| selection.profile_id))
            .unwrap_or(base.profile_id);
        let enabled = if let Some(Value::Boolean(enabled)) = args.get(1) {
            *enabled
        } else {
            previous.map_or(base.enabled, |selection| selection.enabled)
        };
        let candidate = if requested_profile.is_some() || enabled {
            let candidate =
                accepted(resolver.teletext_candidate(self.channel_id, addon, profile_id))?;
            if candidate.channel_id != self.channel_id
                || candidate.addon_id != addon
                || candidate.profile_id != profile_id
                || candidate.installation.addon_id != addon
            {
                return Err("invalid_response");
            }
            Some(TeletextOverrideState::Enabled(candidate))
        } else {
            None
        };
        let off = TeletextOverrideState::Off {
            channel_id: self.channel_id,
            addon_id: addon.to_owned(),
        };
        let bytes = candidate.as_ref().unwrap_or(&off).snapshot_bytes();
        let aggregate = self.teletext.snapshot_bytes.saturating_add(bytes);
        if aggregate > MAX_TELETEXT_OVERRIDE_SNAPSHOT_BYTES {
            return Err("snapshot_limit");
        }
        let state = if enabled {
            candidate.ok_or("invalid_response")?
        } else {
            off
        };
        self.teletext.snapshot_bytes = aggregate;
        self.teletext.selections.insert(
            addon.to_owned(),
            Selection {
                profile_id,
                enabled,
                last_at: self.current_time,
            },
        );
        self.teletext
            .events
            .entry(addon.to_owned())
            .or_default()
            .insert(self.current_time, state);
        Ok(())
    }
}

fn accepted<T>(
    result: crate::repositories::Result<TeletextProfileMutation<T>>,
) -> Result<T, &'static str> {
    match result {
        Ok(TeletextProfileMutation::Saved(value)) => Ok(value),
        Ok(TeletextProfileMutation::Rejected(reason)) => Err(rejection_code(reason)),
        Err(_) => Err("unavailable"),
    }
}

const fn rejection_code(reason: TeletextProfileRejection) -> &'static str {
    match reason {
        TeletextProfileRejection::MissingChannel => "missing_channel",
        TeletextProfileRejection::MissingBinding => "missing_binding",
        TeletextProfileRejection::DisabledBinding => "disabled_binding",
        TeletextProfileRejection::BindingLimit => "binding_limit",
        TeletextProfileRejection::MissingProfile => "missing_profile",
        TeletextProfileRejection::WrongAddon => "wrong_addon",
        TeletextProfileRejection::DeletedProfile => "deleted_profile",
        TeletextProfileRejection::InstallationUnavailable => "installation_unavailable",
        TeletextProfileRejection::InvalidSettings => "invalid_settings",
        TeletextProfileRejection::InvalidValues => "invalid_values",
        TeletextProfileRejection::Conflict => "conflict",
        TeletextProfileRejection::NameConflict => "name_conflict",
        TeletextProfileRejection::RevisionLimit => "revision_limit",
    }
}
