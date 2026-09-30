use std::{cell::RefCell, collections::BTreeMap, fs, rc::Rc};

use chrono::{DateTime, Datelike, Duration, NaiveTime, TimeZone, Timelike, Utc};
use ersatzrs_domain::{ChannelPlayoutMode, EntityId, FillerKind, PlaybackOrder, PlayoutAnchor};
use mlua::{Lua, Table};
use serde::{Deserialize, Serialize};
use std::sync::Arc;

mod command;
pub use command::ScriptedScheduleCommand;
mod resolver;
pub use resolver::ScriptedContentResolver;

use super::{
    history, history_details_for_media_item, history_key_for_content,
    history_key_for_marathon_content, parse_duration, parse_hhmm, playout_item,
    CurrentHistoryContext, MediaEnumerator, PlaylistEnumerator, PlayoutBuildMode,
    PlayoutSchedulingEngine, SchedulingCollectionKey, SchedulingContentGuid, SchedulingError,
    SchedulingPlaylistItem, SchedulingReferenceData, SchedulingRequest, SchedulingResult,
};

// ── public engine ─────────────────────────────────────────────────────────────

#[derive(Debug, Clone, Default)]
pub struct ScriptedSchedulingEngine;

impl PlayoutSchedulingEngine for ScriptedSchedulingEngine {
    fn schedule(&self, request: SchedulingRequest) -> Result<SchedulingResult, SchedulingError> {
        self.schedule_with_resolver(request, None)
    }
}

impl ScriptedSchedulingEngine {
    /// Run a script with optional storage-backed source resolution.
    ///
    /// # Errors
    /// Returns file, source resolution or Lua execution errors.
    pub fn schedule_with_resolver(
        &self,
        request: SchedulingRequest,
        resolver: Option<Arc<dyn ScriptedContentResolver>>,
    ) -> Result<SchedulingResult, SchedulingError> {
        let command = request
            .playout
            .schedule_file
            .clone()
            .ok_or(SchedulingError::MissingScheduleFile)?;
        let command =
            ScriptedScheduleCommand::parse(&command).ok_or(SchedulingError::MissingScheduleFile)?;
        let script = fs::read_to_string(&command.file)
            .map_err(|_| SchedulingError::ScheduleFileMissing(command.file.clone()))?;
        Self::schedule_script_resolved(&script, request, resolver, Some(&command))
    }
}

impl ScriptedSchedulingEngine {
    /// Build a timeline by executing a Lua scheduling script.
    ///
    /// # Errors
    ///
    /// Returns an error when Lua compilation or execution fails.
    pub fn schedule_script(
        &self,
        script: &str,
        request: SchedulingRequest,
    ) -> Result<SchedulingResult, SchedulingError> {
        Self::schedule_script_resolved(script, request, None, None)
    }

    fn schedule_script_resolved(
        script: &str,
        mut request: SchedulingRequest,
        resolver: Option<Arc<dyn ScriptedContentResolver>>,
        command: Option<&ScriptedScheduleCommand>,
    ) -> Result<SchedulingResult, SchedulingError> {
        let (item_policies, legacy_policy) = Self::smart_shuffle_policies_from_script(script)?;
        request
            .reference_data
            .smart_shuffle_policies_by_unit
            .extend(item_policies);
        if let Some(policy) = legacy_policy {
            request.reference_data.smart_shuffle_policy = policy;
        }
        let smart_shuffle_state = if request.mode == PlayoutBuildMode::Continue {
            request
                .playout
                .anchor
                .as_ref()
                .and_then(|anchor| anchor.context.as_deref())
                .and_then(|context| serde_json::from_str::<ScriptedAnchorContext>(context).ok())
                .map(|context| context.smart_shuffle)
                .unwrap_or_default()
        } else {
            super::SchedulingSmartShuffleState::default()
        };
        let state = Rc::new(RefCell::new(ScriptState::new(
            request.playout.id,
            request.start,
            request.finish,
            request.mode,
            request.reference_data,
            smart_shuffle_state,
        )));
        state.borrow_mut().resolver = resolver;
        state.borrow_mut().seed = request.playout.seed;
        let lua = Lua::new();
        if let Some(command) = command {
            let arguments = lua
                .create_sequence_from(command.arguments.iter().cloned())
                .map_err(lua_error)?;
            arguments.set(0, command.file.as_str()).map_err(lua_error)?;
            lua.globals().set("arg", arguments).map_err(lua_error)?;
        }
        register_all(&lua, &state).map_err(lua_error)?;
        run_lua_script(&lua, script).map_err(lua_error)?;

        let mut state = state.borrow().clone();
        let anchor_time = state
            .added_items
            .iter()
            .map(|item| item.finish)
            .max()
            .unwrap_or(state.current_time);
        let remove_before = (state.reference_data.channel_playout_mode
            != ChannelPlayoutMode::OnDemand)
            .then_some(request.start - Duration::hours(4));
        Ok(SchedulingResult {
            clear_items: matches!(
                state.mode,
                PlayoutBuildMode::Refresh | PlayoutBuildMode::Reset
            ),
            remove_before,
            anchor: Some(PlayoutAnchor {
                next_start: anchor_time,
                context: state.reference_data.has_active_smart_shuffle().then(|| {
                    serde_json::to_string(&ScriptedAnchorContext {
                        version: scripted_anchor_context_version(),
                        smart_shuffle: state.smart_shuffle_state.clone(),
                    })
                    .unwrap_or_default()
                }),
                ..PlayoutAnchor::default()
            }),
            added_items: std::mem::take(&mut state.added_items),
            added_history: std::mem::take(&mut state.added_history),
            history_to_remove: std::mem::take(&mut state.history_to_remove),
            warnings: std::mem::take(&mut state.warnings),
            ..SchedulingResult::default()
        })
    }

    /// Read native `set_smart_shuffle_policy(content_key, [[...json...]])`
    /// declarations plus the deprecated global fallback.
    ///
    /// # Errors
    ///
    /// Returns an error when a declaration is malformed or contains invalid JSON.
    pub fn smart_shuffle_policies_from_script(
        script: &str,
    ) -> Result<
        (
            BTreeMap<String, super::SchedulingSmartShufflePolicy>,
            Option<super::SchedulingSmartShufflePolicy>,
        ),
        SchedulingError,
    > {
        let mut item_policies = BTreeMap::new();
        let mut legacy_policy = None;
        let bytes = script.as_bytes();
        let mut cursor = 0;
        while cursor < bytes.len() {
            if bytes[cursor..].starts_with(b"--") {
                cursor = skip_lua_comment(script, cursor);
                continue;
            }
            if matches!(bytes[cursor], b'\'' | b'"') {
                cursor = skip_lua_quoted_string(script, cursor);
                continue;
            }
            if let Some((_, _, after)) = lua_long_bracket(script, cursor) {
                cursor = after;
                continue;
            }
            if !is_lua_identifier_start(bytes[cursor]) {
                cursor += 1;
                continue;
            }

            let identifier_start = cursor;
            cursor += 1;
            while cursor < bytes.len() && is_lua_identifier_continue(bytes[cursor]) {
                cursor += 1;
            }
            let identifier = &script[identifier_start..cursor];
            if !matches!(
                identifier,
                "set_smart_shuffle_policy" | "set_cooldown_policy"
            ) {
                continue;
            }
            let call_start = skip_ascii_whitespace(bytes, cursor);
            if bytes.get(call_start) != Some(&b'(') {
                continue;
            }
            let (content_key, json, after_call) =
                parse_smart_shuffle_declaration(script, identifier, call_start)?;
            let policy = serde_json::from_str(json)
                .map_err(|error| SchedulingError::Script(error.to_string()))?;
            if let Some(content_key) = content_key {
                item_policies.insert(content_key, policy);
            } else {
                legacy_policy = Some(policy);
            }
            cursor = after_call;
        }

        Ok((item_policies, legacy_policy))
    }
}

fn parse_smart_shuffle_declaration<'a>(
    script: &'a str,
    identifier: &str,
    call_start: usize,
) -> Result<(Option<String>, &'a str, usize), SchedulingError> {
    let bytes = script.as_bytes();
    let mut cursor = skip_ascii_whitespace(bytes, call_start + 1);
    let content_key = if identifier == "set_smart_shuffle_policy" {
        let (key, after_key) = parse_lua_quoted_string(script, cursor).ok_or_else(|| {
            SchedulingError::Script(
                "set_smart_shuffle_policy content key must be a quoted string".to_owned(),
            )
        })?;
        cursor = skip_ascii_whitespace(bytes, after_key);
        if bytes.get(cursor) != Some(&b',') {
            return Err(SchedulingError::Script(
                "set_smart_shuffle_policy requires a content key and JSON policy".to_owned(),
            ));
        }
        cursor = skip_ascii_whitespace(bytes, cursor + 1);
        Some(key)
    } else {
        None
    };

    let Some((json_start, json_end, after_json)) = lua_long_bracket(script, cursor) else {
        return Err(SchedulingError::Script(format!(
            "{identifier} JSON must use a terminated Lua long bracket"
        )));
    };
    cursor = skip_ascii_whitespace(bytes, after_json);
    if bytes.get(cursor) != Some(&b')') {
        return Err(SchedulingError::Script(format!(
            "{identifier} declaration must end after the JSON policy"
        )));
    }

    Ok((content_key, &script[json_start..json_end], cursor + 1))
}

fn parse_lua_quoted_string(script: &str, start: usize) -> Option<(String, usize)> {
    let bytes = script.as_bytes();
    let quote = *bytes.get(start)?;
    if !matches!(quote, b'\'' | b'"') {
        return None;
    }
    let mut value = String::new();
    let mut cursor = start + 1;
    while cursor < bytes.len() {
        match bytes[cursor] {
            current if current == quote => return Some((value, cursor + 1)),
            b'\\' => {
                cursor += 1;
                let escaped = *bytes.get(cursor)?;
                value.push(match escaped {
                    b'n' => '\n',
                    b'r' => '\r',
                    b't' => '\t',
                    b'\\' => '\\',
                    b'\'' => '\'',
                    b'"' => '"',
                    other => char::from(other),
                });
                cursor += 1;
            }
            _ => {
                let character = script[cursor..].chars().next()?;
                value.push(character);
                cursor += character.len_utf8();
            }
        }
    }
    None
}

fn skip_lua_quoted_string(script: &str, start: usize) -> usize {
    let bytes = script.as_bytes();
    let quote = bytes[start];
    let mut cursor = start + 1;
    while cursor < bytes.len() {
        match bytes[cursor] {
            b'\\' => cursor = (cursor + 2).min(bytes.len()),
            current if current == quote => return cursor + 1,
            _ => cursor += 1,
        }
    }
    bytes.len()
}

fn skip_lua_comment(script: &str, start: usize) -> usize {
    let bytes = script.as_bytes();
    let body_start = start + 2;
    if let Some((_, _, after)) = lua_long_bracket(script, body_start) {
        return after;
    }
    bytes[body_start..]
        .iter()
        .position(|byte| *byte == b'\n')
        .map_or(bytes.len(), |offset| body_start + offset + 1)
}

fn lua_long_bracket(script: &str, start: usize) -> Option<(usize, usize, usize)> {
    let bytes = script.as_bytes();
    if bytes.get(start) != Some(&b'[') {
        return None;
    }
    let mut opener_end = start + 1;
    while bytes.get(opener_end) == Some(&b'=') {
        opener_end += 1;
    }
    if bytes.get(opener_end) != Some(&b'[') {
        return None;
    }
    let equals_count = opener_end - start - 1;
    let content_start = opener_end + 1;
    let mut cursor = content_start;
    while cursor < bytes.len() {
        if bytes[cursor] == b']' {
            let equals_end = cursor + 1 + equals_count;
            if equals_end < bytes.len()
                && bytes[cursor + 1..equals_end]
                    .iter()
                    .all(|byte| *byte == b'=')
                && bytes[equals_end] == b']'
            {
                return Some((content_start, cursor, equals_end + 1));
            }
        }
        cursor += 1;
    }
    None
}

fn skip_ascii_whitespace(bytes: &[u8], mut cursor: usize) -> usize {
    while bytes.get(cursor).is_some_and(u8::is_ascii_whitespace) {
        cursor += 1;
    }
    cursor
}

fn is_lua_identifier_start(byte: u8) -> bool {
    byte.is_ascii_alphabetic() || byte == b'_'
}

fn is_lua_identifier_continue(byte: u8) -> bool {
    is_lua_identifier_start(byte) || byte.is_ascii_digit()
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
struct ScriptedAnchorContext {
    #[serde(default = "scripted_anchor_context_version")]
    version: u32,
    #[serde(default, alias = "cooldown")]
    smart_shuffle: super::SchedulingSmartShuffleState,
}

fn scripted_anchor_context_version() -> u32 {
    1
}

fn run_lua_script(lua: &Lua, script: &str) -> Result<(), mlua::Error> {
    lua.load(script).eval::<mlua::MultiValue>()?;
    Ok(())
}

// ── option structs ────────────────────────────────────────────────────────────

#[derive(Debug, Clone, Default)]
#[allow(clippy::struct_excessive_bools)]
struct AddOptions {
    fallback: Option<String>,
    trim: bool,
    discard_attempts: i32,
    stop_before_end: bool,
    offline_tail: bool,
    filler_kind: FillerKind,
    custom_title: Option<String>,
    disable_watermarks: bool,
    tomorrow: bool,
}

fn read_add_options(table: Option<&Table>) -> AddOptions {
    let mut opts = AddOptions::default();
    if let Some(t) = table {
        opts.fallback = t.get::<Option<String>>("fallback").ok().flatten();
        opts.trim = t
            .get::<Option<bool>>("trim")
            .ok()
            .flatten()
            .unwrap_or(false);
        opts.discard_attempts = t
            .get::<Option<i32>>("discard_attempts")
            .ok()
            .flatten()
            .unwrap_or(0);
        opts.stop_before_end = t
            .get::<Option<bool>>("stop_before_end")
            .ok()
            .flatten()
            .unwrap_or(false);
        opts.offline_tail = t
            .get::<Option<bool>>("offline_tail")
            .ok()
            .flatten()
            .unwrap_or(false);
        opts.custom_title = t.get::<Option<String>>("custom_title").ok().flatten();
        opts.disable_watermarks = t
            .get::<Option<bool>>("disable_watermarks")
            .ok()
            .flatten()
            .unwrap_or(false);
        opts.tomorrow = t
            .get::<Option<bool>>("tomorrow")
            .ok()
            .flatten()
            .unwrap_or(false);
        if let Ok(Some(s)) = t.get::<Option<String>>("filler_kind") {
            opts.filler_kind = parse_filler_kind(&s);
        }
    }
    opts
}

fn parse_filler_kind(s: &str) -> FillerKind {
    match s.to_lowercase().as_str() {
        "pre_roll" | "preroll" => FillerKind::PreRoll,
        "post_roll" | "postroll" => FillerKind::PostRoll,
        "mid_roll" | "midroll" => FillerKind::MidRoll,
        "tail" => FillerKind::Tail,
        "fallback" => FillerKind::Fallback,
        _ => FillerKind::None,
    }
}

fn parse_playback_order(s: Option<&str>) -> PlaybackOrder {
    match s {
        Some("chronological") => PlaybackOrder::Chronological,
        Some("shuffle") => PlaybackOrder::Shuffle,
        Some("random") => PlaybackOrder::Random,
        Some("season_episode" | "seasonepisode") => PlaybackOrder::SeasonEpisode,
        _ => PlaybackOrder::None,
    }
}

// ── Lua binding registration ──────────────────────────────────────────────────

mod api;

fn register_all(lua: &Lua, state: &Rc<RefCell<ScriptState>>) -> Result<(), mlua::Error> {
    api::policy::register(lua, state)?;
    api::content::register_legacy(lua, state)?;
    api::watermark::register_legacy(lua, state)?;
    api::graphics::register_legacy(lua, state)?;
    api::timing::register_wait(lua, state)?;
    api::content::register_add(lua, state)?;
    api::timing::register_padding(lua, state)?;
    api::skipping::register(lua, state)?;
    api::pre_roll::register(lua, state)?;
    api::guide::register(lua, state)?;
    api::state_queries::register(lua, state)?;
    api::watermark::register_named(lua, state)?;
    api::graphics::register_paths(lua, state)?;
    api::sources::register(lua, state)?;
    Ok(())
}

// ── script state ──────────────────────────────────────────────────────────────

#[derive(Debug, Clone)]
struct ScriptState {
    seed: i32,
    resolver: Option<Arc<dyn ScriptedContentResolver>>,
    warnings: Vec<String>,
    playout_id: EntityId,
    start: DateTime<Utc>,
    current_time: DateTime<Utc>,
    finish: DateTime<Utc>,
    mode: PlayoutBuildMode,
    reference_data: SchedulingReferenceData,
    watermark_ids: Vec<EntityId>,
    graphics_element_ids: Vec<EntityId>,
    graphics_element_variables: BTreeMap<EntityId, Option<String>>,
    added_items: Vec<ersatzrs_domain::PlayoutItem>,
    added_history: Vec<ersatzrs_domain::PlayoutHistory>,
    history_to_remove: Vec<EntityId>,
    enumerators: BTreeMap<String, MediaEnumerator>,
    playlist_enumerators: BTreeMap<String, PlaylistEnumerator>,
    playlist_history_keys: BTreeMap<String, String>,
    guide_group: i32,
    guide_group_locked: bool,
    guide_group_title: bool,
    custom_title: Option<String>,
    pre_roll_key: Option<String>,
    last_progress_time: Option<DateTime<Utc>>,
    no_progress_counter: u32,
    smart_shuffle_state: super::SchedulingSmartShuffleState,
}

impl ScriptState {
    fn new(
        playout_id: EntityId,
        start: DateTime<Utc>,
        finish: DateTime<Utc>,
        mode: PlayoutBuildMode,
        reference_data: SchedulingReferenceData,
        smart_shuffle_state: super::SchedulingSmartShuffleState,
    ) -> Self {
        let mut st = Self {
            seed: 0,
            resolver: None,
            warnings: Vec::new(),
            playout_id,
            start,
            current_time: start,
            finish,
            mode,
            reference_data,
            watermark_ids: Vec::new(),
            graphics_element_ids: Vec::new(),
            graphics_element_variables: BTreeMap::new(),
            added_items: Vec::new(),
            added_history: Vec::new(),
            history_to_remove: Vec::new(),
            enumerators: BTreeMap::new(),
            playlist_enumerators: BTreeMap::new(),
            playlist_history_keys: BTreeMap::new(),
            guide_group: 0,
            guide_group_locked: false,
            guide_group_title: false,
            custom_title: None,
            pre_roll_key: None,
            last_progress_time: None,
            no_progress_counter: 0,
            smart_shuffle_state,
        };
        if mode == PlayoutBuildMode::Reset {
            st.history_to_remove = st
                .reference_data
                .existing_history
                .iter()
                .filter(|h| h.when > start || h.finish.is_some_and(|f| f >= start))
                .map(|h| h.id)
                .collect();
        }
        st
    }

    fn current_media_and_context(
        &mut self,
        key: &str,
    ) -> Option<(super::SchedulingMediaItem, CurrentHistoryContext)> {
        let policy = self
            .reference_data
            .smart_shuffle_policy_for_unit(key)
            .clone();
        let smart_shuffle_state = self.smart_shuffle_state.clone();
        let tags_by_media_id = self.reference_data.smart_shuffle_tags_for_unit(key).clone();
        let scope_key = policy.scope_key(self.playout_id, None, Some(key));
        let scheduled_at = self.current_time;
        if self.playlist_enumerators.contains_key(key) {
            let playlist = self.playlist_enumerators.get_mut(key).unwrap();
            playlist.select_with_smart_shuffle(
                &policy,
                &smart_shuffle_state,
                &scope_key,
                scheduled_at,
                &tags_by_media_id,
            );
            let media_item = playlist.current().cloned()?;
            let ctx = playlist.current_history_context();
            Some((media_item, ctx))
        } else {
            // lazy-load from media_by_key, mirroring the old enumerator() behaviour
            let e = self.enumerator(key);
            e.select_with_smart_shuffle(
                &policy,
                &smart_shuffle_state,
                &scope_key,
                scheduled_at,
                &tags_by_media_id,
            );
            let media_item = e.current().cloned()?;
            let idx = e.current_index();
            Some((media_item, CurrentHistoryContext::Single { index: idx }))
        }
    }

    fn record_smart_shuffle_item(&mut self, key: &str, item: &ersatzrs_domain::PlayoutItem) {
        let playback_order = self
            .playlist_enumerators
            .get(key)
            .map(PlaylistEnumerator::current_playback_order)
            .or_else(|| {
                self.enumerators
                    .get(key)
                    .map(MediaEnumerator::playback_order)
            })
            .unwrap_or(PlaybackOrder::None);
        let policy = self
            .reference_data
            .smart_shuffle_policy_for_unit(key)
            .clone();
        let tags_by_media_id = self.reference_data.smart_shuffle_tags_for_unit(key);
        let scope_key = policy.scope_key(self.playout_id, None, Some(key));
        let unit = if let Some(playlist) = self.playlist_enumerators.get(key) {
            let Some(unit) = playlist.smart_shuffle_unit(tags_by_media_id) else {
                return;
            };
            unit
        } else {
            super::SchedulingSmartShuffleUnit::single(
                item.media_item_id,
                self.reference_data
                    .smart_shuffle_tags_for_media(key, item.media_item_id)
                    .clone(),
                item.finish - item.start,
            )
        };
        self.smart_shuffle_state.record_unit(
            &policy,
            self.playout_id,
            &scope_key,
            item.start,
            &unit,
            super::SchedulingSmartShuffleRecordContext::new(
                super::is_randomized_playback_order(playback_order),
                item.filler_kind != FillerKind::None,
            ),
        );
    }

    fn advance_key(&mut self, key: &str) {
        if let Some(playlist) = self.playlist_enumerators.get_mut(key) {
            playlist.advance();
        } else if let Some(e) = self.enumerators.get_mut(key) {
            e.advance();
        }
    }

    fn key_len(&mut self, key: &str) -> usize {
        if let Some(playlist) = self.playlist_enumerators.get(key) {
            playlist.len()
        } else {
            self.enumerator(key).len()
        }
    }

    fn active_graphics_elements(&self) -> Vec<ersatzrs_domain::PlayoutItemGraphicsElement> {
        self.graphics_element_ids
            .iter()
            .copied()
            .map(
                |graphics_element_id| ersatzrs_domain::PlayoutItemGraphicsElement {
                    playout_item_id: 0,
                    graphics_element_id: Some(graphics_element_id),
                    variables: self
                        .graphics_element_variables
                        .get(&graphics_element_id)
                        .cloned()
                        .flatten(),
                },
            )
            .collect()
    }

    fn history_key_for(&self, key: &str) -> String {
        if let Some(k) = self.playlist_history_keys.get(key) {
            k.clone()
        } else {
            let po = self
                .enumerators
                .get(key)
                .map_or(PlaybackOrder::None, |e| e.playback_order);
            history_key_for_content(key, po)
        }
    }

    fn push_history_ctx(
        &mut self,
        content_key: &str,
        item: &ersatzrs_domain::PlayoutItem,
        media_item: &super::SchedulingMediaItem,
        ctx: &CurrentHistoryContext,
        history_key: &str,
    ) {
        match ctx {
            CurrentHistoryContext::Single { index } => {
                self.added_history.push(history(
                    self.playout_id,
                    history_key,
                    item,
                    None,
                    *index,
                    history_details_for_media_item(media_item),
                    false,
                ));
            }
            CurrentHistoryContext::Playlist { .. } => {
                self.added_history
                    .extend(self.playlist_enumerators[content_key].history_for_item(
                        self.playout_id,
                        history_key,
                        item,
                    ));
            }
        }
    }

    fn collect_playlist_history(&self, history_key: &str) -> Vec<ersatzrs_domain::PlayoutHistory> {
        let cutoff = self.start;
        let Some(latest_when) = self
            .reference_data
            .existing_history
            .iter()
            .filter(|h| h.key == history_key && h.when < cutoff)
            .map(|h| h.when)
            .max()
        else {
            return Vec::new();
        };
        self.reference_data
            .existing_history
            .iter()
            .filter(|h| h.key == history_key && h.when == latest_when)
            .cloned()
            .collect()
    }

    fn peek_guide_group(&self) -> i32 {
        if self.guide_group_locked {
            self.guide_group
        } else {
            let next = self.guide_group + 1;
            if next > 1000 {
                1
            } else {
                next
            }
        }
    }

    fn advance_guide_group(&mut self) {
        if !self.guide_group_locked {
            self.guide_group += 1;
            if self.guide_group > 1000 {
                self.guide_group = 1;
            }
        }
    }

    fn lock_guide_group(&mut self, advance: bool, title: Option<String>) {
        if advance {
            self.advance_guide_group();
        }
        self.guide_group_locked = true;
        if let Some(t) = title {
            self.guide_group_title = true;
            self.custom_title = Some(t);
        }
    }

    fn unlock_guide_group(&mut self) {
        self.guide_group_locked = false;
        if self.guide_group_title {
            self.custom_title = None;
        }
        self.guide_group_title = false;
    }

    fn enumerator(&mut self, key: &str) -> &mut MediaEnumerator {
        self.enumerators.entry(key.to_owned()).or_insert_with(|| {
            let items = self
                .reference_data
                .media_by_key
                .get(key)
                .cloned()
                .unwrap_or_default();
            MediaEnumerator::new(key, items)
        })
    }

    fn apply_history(&self, key: &str, e: &mut MediaEnumerator, po: PlaybackOrder) {
        let history_key = history_key_for_content(key, po);
        let cutoff = self.start;
        let mut candidates: Vec<_> = self
            .reference_data
            .existing_history
            .iter()
            .filter(|h| h.key == history_key && h.when < cutoff)
            .collect();
        candidates.sort_by_key(|h| h.when);
        if let Some(h) = candidates.last() {
            let next_index = usize::try_from(h.index).unwrap_or(0).saturating_add(1);
            e.set_index(next_index);
            if po == PlaybackOrder::Chronological {
                e.move_after_history_details(&h.details);
            }
        }
    }

    fn add_item_simple(&mut self, key: &str, duration: Option<Duration>) {
        if self.current_time >= self.finish {
            return;
        }
        // Insert pre-roll before each content item (mirrors AddCountInternal).
        self.maybe_add_pre_roll(self.finish);
        if self.current_time >= self.finish {
            return;
        }
        let Some((media_item, history_ctx)) = self.current_media_and_context(key) else {
            return;
        };
        let duration = duration.unwrap_or(media_item.duration);
        let guide_group = self.peek_guide_group();
        let custom_title = self.custom_title.clone();
        let mut item = playout_item(
            self.playout_id,
            &media_item,
            self.current_time,
            duration,
            guide_group,
            FillerKind::None,
            custom_title,
            false,
            self.watermark_ids.clone(),
            self.graphics_element_ids.clone(),
            Some(format!(r#"{{"scriptContent":"{key}"}}"#)),
        );
        item.graphics_elements = self.active_graphics_elements();
        let history_key = self.history_key_for(key);
        self.push_history_ctx(key, &item, &media_item, &history_ctx, &history_key);
        self.record_smart_shuffle_item(key, &item);
        self.current_time = item.finish;
        self.added_items.push(item);
        self.advance_guide_group();
        self.advance_key(key);
    }

    fn add_item_with_opts(&mut self, key: &str, duration: Option<Duration>, opts: &AddOptions) {
        if self.current_time >= self.finish {
            return;
        }
        let Some((media_item, history_ctx)) = self.current_media_and_context(key) else {
            return;
        };
        let duration = duration.unwrap_or(media_item.duration);
        let guide_group = self.peek_guide_group();
        let watermarks = if opts.disable_watermarks {
            Vec::new()
        } else {
            self.watermark_ids.clone()
        };
        let custom_title = opts
            .custom_title
            .clone()
            .or_else(|| self.custom_title.clone());
        let mut item = playout_item(
            self.playout_id,
            &media_item,
            self.current_time,
            duration,
            guide_group,
            opts.filler_kind,
            custom_title,
            opts.disable_watermarks,
            watermarks,
            self.graphics_element_ids.clone(),
            Some(format!(r#"{{"scriptContent":"{key}"}}"#)),
        );
        item.graphics_elements = self.active_graphics_elements();
        let history_key = self.history_key_for(key);
        self.push_history_ctx(key, &item, &media_item, &history_ctx, &history_key);
        self.record_smart_shuffle_item(key, &item);
        self.current_time = item.finish;
        self.added_items.push(item);
        self.advance_guide_group();
        self.advance_key(key);
    }

    fn add_pre_roll_item(&mut self, key: &str, target: DateTime<Utc>) {
        if self.current_time >= target {
            return;
        }
        let Some(media_item) = self.enumerator(key).current().cloned() else {
            return;
        };
        let duration = media_item.duration.min(target - self.current_time);
        let guide_group = self.peek_guide_group();
        let item = playout_item(
            self.playout_id,
            &media_item,
            self.current_time,
            duration,
            guide_group,
            FillerKind::PreRoll,
            None,
            true,
            Vec::new(),
            Vec::new(),
            None,
        );
        self.record_smart_shuffle_item(key, &item);
        self.current_time = item.finish;
        self.added_items.push(item);
        self.advance_guide_group();
        self.enumerator(key).advance();
    }

    fn maybe_add_pre_roll(&mut self, target: DateTime<Utc>) {
        if let Some(key) = self.pre_roll_key.clone() {
            let count = self.enumerator(&key).len();
            for _ in 0..count {
                if self.current_time >= target {
                    break;
                }
                self.add_pre_roll_item(&key, target);
            }
        }
    }

    fn add_duration_internal(&mut self, key: &str, target: DateTime<Utc>, opts: &AddOptions) {
        let mut discard_attempts = opts.discard_attempts;
        let mut done = false;

        while !done && self.current_time < target {
            self.maybe_add_pre_roll(target);
            if self.current_time >= target {
                break;
            }
            let Some((media_item, _)) = self.current_media_and_context(key) else {
                break;
            };
            let item_duration = media_item.duration;
            let remaining = target - self.current_time;

            if remaining >= item_duration || !opts.stop_before_end {
                let use_duration = if opts.stop_before_end {
                    item_duration
                } else {
                    item_duration.min(remaining)
                };
                self.add_item_with_opts(key, Some(use_duration), opts);
            } else if discard_attempts > 0 {
                discard_attempts -= 1;
                self.advance_key(key);
            } else if opts.trim {
                self.add_item_with_opts(key, Some(remaining), opts);
                done = true;
            } else if let Some(ref fallback_key) = opts.fallback.clone() {
                let fb_opts = AddOptions {
                    filler_kind: FillerKind::Fallback,
                    ..AddOptions::default()
                };
                self.add_item_with_opts(fallback_key, Some(remaining), &fb_opts);
                done = true;
            } else {
                done = true;
            }
        }

        if opts.stop_before_end && opts.offline_tail {
            self.current_time = target;
        }
    }
}

// ── calendar helpers ──────────────────────────────────────────────────────────

fn next_n_minute_boundary(now: DateTime<Utc>, n: i64) -> DateTime<Utc> {
    if n <= 0 {
        return now;
    }
    let current_minute = i64::from(now.minute());
    let target_minute = ((current_minute + n - 1) / n) * n;
    let base = now - Duration::minutes(current_minute) + Duration::minutes(target_minute);
    let truncated = Utc
        .with_ymd_and_hms(
            base.year(),
            base.month(),
            base.day(),
            base.hour(),
            u32::try_from(target_minute % 60).unwrap_or(0),
            0,
        )
        .single()
        .unwrap_or(base);
    if truncated <= now {
        truncated + Duration::minutes(n)
    } else {
        truncated
    }
}

fn pad_until_target(now: DateTime<Utc>, target_time: NaiveTime, tomorrow: bool) -> DateTime<Utc> {
    let day = now.date_naive();
    let naive_target = day.and_time(target_time);
    let target_utc = Utc.from_utc_datetime(&naive_target);
    if now.time() > target_time {
        if tomorrow {
            target_utc + Duration::days(1)
        } else {
            target_utc
        }
    } else {
        target_utc
    }
}

#[allow(clippy::needless_pass_by_value)]
fn lua_error(error: mlua::Error) -> SchedulingError {
    SchedulingError::Script(error.to_string())
}

// ── tests ─────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use chrono::Duration;
    use ersatzrs_domain::Playout;

    use super::*;
    use crate::scheduling::{SchedulingMediaItem, SchedulingReferenceData};

    #[test]
    fn scripted_playlist_continue_preserves_every_child_position() {
        let script = r#"
add_collection("a", "A", "chronological")
add_collection("b", "B", "chronological")
add_collection("c", "C", "chronological")
create_playlist("pl", {a=1, b=1, c=1})
while not is_done() do add_count("pl", 1) end
"#;
        let mut reference = SchedulingReferenceData::default();
        for (name, base) in [("A", 10), ("B", 20), ("C", 30)] {
            reference.collections_by_name.insert(
                name.to_owned(),
                (0..4)
                    .map(|index| SchedulingMediaItem::new(base + index, Duration::minutes(1)))
                    .collect(),
            );
        }
        let request = SchedulingRequest {
            playout: Playout {
                id: 100,
                ..Playout::default()
            },
            start: base_start(),
            finish: base_start() + Duration::minutes(24),
            mode: PlayoutBuildMode::Reset,
            reference_data: reference,
        };
        let full = ScriptedSchedulingEngine
            .schedule_script(script, request.clone())
            .unwrap();
        for split in [1, 7, 11, 15] {
            let mut first_request = request.clone();
            first_request.finish = request.start + Duration::minutes(split);
            let first = ScriptedSchedulingEngine
                .schedule_script(script, first_request)
                .unwrap();
            let mut continued_request = request.clone();
            continued_request.mode = PlayoutBuildMode::Continue;
            continued_request.start = first.added_items.last().unwrap().finish;
            continued_request.playout.anchor = first.anchor;
            continued_request.reference_data.existing_history = first.added_history;
            let continued = ScriptedSchedulingEngine
                .schedule_script(script, continued_request)
                .unwrap();
            let actual = first
                .added_items
                .iter()
                .chain(&continued.added_items)
                .map(|item| (item.media_item_id, item.start, item.finish))
                .collect::<Vec<_>>();
            let expected = full
                .added_items
                .iter()
                .map(|item| (item.media_item_id, item.start, item.finish))
                .collect::<Vec<_>>();
            assert_eq!(actual, expected, "split after {split} items");
        }
    }

    macro_rules! assert_scripted_json_snapshot {
        ($($tokens:tt)*) => {
            insta::with_settings!({ snapshot_path => "../snapshots" }, {
                insta::assert_json_snapshot!($($tokens)*);
            });
        };
    }

    fn base_start() -> DateTime<Utc> {
        DateTime::parse_from_rfc3339("2026-04-15T12:00:00Z")
            .expect("valid time")
            .with_timezone(&Utc)
    }

    fn items_2(a: Duration, b: Duration) -> Vec<SchedulingMediaItem> {
        vec![
            SchedulingMediaItem::new(1, a),
            SchedulingMediaItem::new(2, b),
        ]
    }

    fn run(script: &str, reference_data: SchedulingReferenceData) -> SchedulingResult {
        ScriptedSchedulingEngine
            .schedule_script(
                script,
                SchedulingRequest {
                    playout: Playout {
                        id: 100,
                        ..Playout::default()
                    },
                    start: base_start(),
                    finish: base_start() + Duration::hours(2),
                    mode: PlayoutBuildMode::Reset,
                    reference_data,
                },
            )
            .expect("script succeeds")
    }

    #[test]
    fn lua_registry_exposes_exact_supported_function_set() {
        fn string_global_names(lua: &Lua) -> std::collections::BTreeSet<String> {
            lua.globals()
                .pairs::<String, mlua::Value>()
                .map(|pair| pair.map(|(name, _)| name))
                .collect::<Result<_, _>>()
                .expect("Lua globals are readable")
        }

        let lua = Lua::new();
        let before = string_global_names(&lua);
        let state = Rc::new(RefCell::new(ScriptState::new(
            100,
            base_start(),
            base_start() + Duration::hours(2),
            PlayoutBuildMode::Reset,
            SchedulingReferenceData::default(),
            crate::scheduling::SchedulingSmartShuffleState::default(),
        )));

        register_all(&lua, &state).expect("Lua functions register");

        let after = string_global_names(&lua);
        let added = after.difference(&before).cloned().collect::<Vec<_>>();
        let expected = api::REGISTERED_GLOBALS
            .iter()
            .map(ToString::to_string)
            .collect::<std::collections::BTreeSet<_>>();
        assert_eq!(
            added,
            expected.iter().cloned().collect::<Vec<_>>(),
            "registration must add every supported global and no others"
        );

        let globals = lua.globals();
        for name in api::REGISTERED_GLOBALS {
            assert!(
                matches!(
                    globals
                        .get::<mlua::Value>(name)
                        .expect("registered global is readable"),
                    mlua::Value::Function(_)
                ),
                "{name} must be registered as a Lua function"
            );
        }
    }

    fn snapshot_items(result: &SchedulingResult) -> serde_json::Value {
        serde_json::json!({
            "items": result.added_items.iter().map(|item| serde_json::json!({
                "media": item.media_item_id,
                "start": item.start.to_rfc3339(),
                "finish": item.finish.to_rfc3339(),
                "filler": format!("{:?}", item.filler_kind),
                "guide_group": item.guide_group,
                "watermarks": item.watermark_ids,
                "graphics": item.graphics_element_ids,
                "custom_title": item.custom_title,
                "disable_watermarks": item.disable_watermarks,
            })).collect::<Vec<_>>(),
        })
    }

    #[test]
    fn smart_shuffle_declarations_support_multiline_lua_long_brackets() {
        let script = r#"
-- set_smart_shuffle_policy("ignored", [[not json]])
local ignored = 'set_smart_shuffle_policy("also-ignored", [[not json]])'
set_smart_shuffle_policy(
    "main",
    [=[
        {
            "version": 1,
            "enabled": true,
            "preset": "custom",
            "scope": "playout",
            "mode": "soft",
            "after_items": 2,
            "include_filler": false,
            "rules": [{"id": "artist", "kind": "artist"}]
        }
    ]=]
)
set_cooldown_policy(
    [[{"version":1,"enabled":false}]]
)
"#;

        let (policies, legacy) =
            ScriptedSchedulingEngine::smart_shuffle_policies_from_script(script)
                .expect("multiline declarations parse");

        assert_eq!(policies.len(), 1);
        assert_eq!(policies["main"].after_items, Some(2));
        assert!(policies["main"].is_active());
        assert!(!legacy.expect("legacy fallback parsed").enabled);
    }

    #[test]
    fn malformed_smart_shuffle_declaration_is_rejected() {
        let error = ScriptedSchedulingEngine::smart_shuffle_policies_from_script(
            r#"set_smart_shuffle_policy("main", [[{"enabled":true})"#,
        )
        .expect_err("unterminated long bracket must fail");

        assert!(error.to_string().contains("terminated Lua long bracket"));
    }

    // ── regression: original test ─────────────────────────────────────────────

    #[test]
    fn scripted_trim_cutoff_depends_on_channel_playout_mode() {
        let start = base_start();
        for (channel_playout_mode, expected_cutoff) in [
            (
                ChannelPlayoutMode::Continuous,
                Some(start - Duration::hours(4)),
            ),
            (ChannelPlayoutMode::OnDemand, None),
        ] {
            let reference_data = SchedulingReferenceData {
                channel_playout_mode,
                ..SchedulingReferenceData::default()
            };
            let result = ScriptedSchedulingEngine
                .schedule_script(
                    "",
                    SchedulingRequest {
                        playout: Playout::default(),
                        start,
                        finish: start + Duration::hours(1),
                        mode: PlayoutBuildMode::Continue,
                        reference_data,
                    },
                )
                .expect("empty script builds");
            assert_eq!(result.remove_before, expected_cutoff);
        }
    }

    #[test]
    fn scripted_lua_schedules_count_and_duration_snapshot() {
        let start = base_start();
        let mut reference_data = SchedulingReferenceData::default();
        reference_data.media_by_key.insert(
            "main".to_owned(),
            items_2(Duration::minutes(10), Duration::minutes(20)),
        );

        let result = ScriptedSchedulingEngine
            .schedule_script(
                r#"
set_watermark(42)
graphics_on(77)
schedule_count("main", 1)
graphics_off(77)
schedule_duration("main", "00:05:00")
"#,
                SchedulingRequest {
                    playout: Playout {
                        id: 100,
                        ..Playout::default()
                    },
                    start,
                    finish: start + Duration::hours(1),
                    mode: PlayoutBuildMode::Reset,
                    reference_data,
                },
            )
            .expect("script builds");

        assert_scripted_json_snapshot!(serde_json::json!({
            "items": result.added_items.iter().map(|item| serde_json::json!({
                "media": item.media_item_id,
                "start": item.start.to_rfc3339(),
                "finish": item.finish.to_rfc3339(),
                "watermarks": item.watermark_ids,
                "graphics": item.graphics_element_ids,
            })).collect::<Vec<_>>(),
        }));
    }

    #[test]
    fn scripted_smart_shuffle_state_survives_continue_anchor() {
        let script = r#"
set_smart_shuffle_policy("main", [[{"version":1,"enabled":true,"preset":"custom","scope":"playout","mode":"soft","after_items":2,"include_filler":false,"rules":[{"id":"artist","kind":"artist"}]}]])
add_collection("main", "Main", "shuffle")
add_count("main", 1)
"#;
        let mut reference_data = SchedulingReferenceData::default();
        reference_data.collections_by_name.insert(
            "Main".to_owned(),
            vec![
                SchedulingMediaItem::new(1, Duration::minutes(10)),
                SchedulingMediaItem::new(2, Duration::minutes(10)),
                SchedulingMediaItem::new(3, Duration::minutes(10)),
            ],
        );
        reference_data.smart_shuffle_tags_by_media_id.insert(
            1,
            std::collections::BTreeSet::from(["artist:same".to_owned()]),
        );
        reference_data.smart_shuffle_tags_by_media_id.insert(
            2,
            std::collections::BTreeSet::from(["artist:other".to_owned()]),
        );
        reference_data.smart_shuffle_tags_by_media_id.insert(
            3,
            std::collections::BTreeSet::from(["artist:same".to_owned()]),
        );

        let first = ScriptedSchedulingEngine
            .schedule_script(
                script,
                SchedulingRequest {
                    playout: Playout {
                        id: 100,
                        ..Playout::default()
                    },
                    start: base_start(),
                    finish: base_start() + Duration::hours(1),
                    mode: PlayoutBuildMode::Reset,
                    reference_data: reference_data.clone(),
                },
            )
            .expect("initial script succeeds");
        assert_eq!(first.added_items[0].media_item_id, 1);
        let anchor = first.anchor.expect("cooldown anchor");
        assert!(anchor
            .context
            .as_deref()
            .is_some_and(|value| value.contains("artist:same")));

        let continued = ScriptedSchedulingEngine
            .schedule_script(
                script,
                SchedulingRequest {
                    playout: Playout {
                        id: 100,
                        anchor: Some(anchor.clone()),
                        ..Playout::default()
                    },
                    start: anchor.next_start,
                    finish: anchor.next_start + Duration::hours(1),
                    mode: PlayoutBuildMode::Continue,
                    reference_data,
                },
            )
            .expect("continued script succeeds");
        assert_eq!(continued.added_items[0].media_item_id, 2);
    }

    // ── add_all ───────────────────────────────────────────────────────────────

    #[test]
    fn scripted_add_all_schedules_every_item_snapshot() {
        let mut rd = SchedulingReferenceData::default();
        rd.media_by_key.insert(
            "movies".to_owned(),
            vec![
                SchedulingMediaItem::new(10, Duration::minutes(30)),
                SchedulingMediaItem::new(11, Duration::minutes(45)),
                SchedulingMediaItem::new(12, Duration::minutes(20)),
            ],
        );
        let result = run(r#"add_all("movies")"#, rd);
        assert_scripted_json_snapshot!(snapshot_items(&result));
    }

    // ── add_duration with trim ────────────────────────────────────────────────

    #[test]
    fn scripted_add_duration_trim_snapshot() {
        let mut rd = SchedulingReferenceData::default();
        rd.media_by_key.insert(
            "ep".to_owned(),
            vec![
                SchedulingMediaItem::new(1, Duration::minutes(22)),
                SchedulingMediaItem::new(2, Duration::minutes(22)),
                SchedulingMediaItem::new(3, Duration::minutes(22)),
            ],
        );
        // 45-minute window: items 1+2 fit, item 3 gets trimmed to 1 min
        let result = run(
            r#"add_duration("ep", "00:45:00", {trim=true, stop_before_end=true})"#,
            rd,
        );
        assert_scripted_json_snapshot!(snapshot_items(&result));
    }

    // ── add_duration stop_before_end ──────────────────────────────────────────

    #[test]
    fn scripted_add_duration_stop_before_end_snapshot() {
        let mut rd = SchedulingReferenceData::default();
        rd.media_by_key.insert(
            "ep".to_owned(),
            vec![
                SchedulingMediaItem::new(1, Duration::minutes(25)),
                SchedulingMediaItem::new(2, Duration::minutes(25)),
            ],
        );
        // 40-minute window: item 1 (25 min) fits, item 2 (25 min) would exceed
        let result = run(
            r#"add_duration("ep", "00:40:00", {stop_before_end=true})"#,
            rd,
        );
        assert_scripted_json_snapshot!(snapshot_items(&result));
    }

    // ── add_duration with fallback ────────────────────────────────────────────

    #[test]
    fn scripted_fallback_duration_history_and_guide_group_use_fallback_identity() {
        for gap_seconds in [90, 600] {
            for fallback_seconds in [30, 2700, 3600] {
                for locked in [false, true] {
                    let mut rd = SchedulingReferenceData::default();
                    rd.media_by_key.insert(
                        "main".into(),
                        vec![SchedulingMediaItem::new(1, Duration::minutes(30))],
                    );
                    let mut fallback =
                        SchedulingMediaItem::new(99, Duration::seconds(fallback_seconds));
                    fallback.preferred_audio_language_code = Some("ja".into());
                    fallback.preferred_subtitle_language_code = Some("nl".into());
                    rd.media_by_key.insert("fallback".into(), vec![fallback]);
                    let group = if locked { "lock_guide_group()\n" } else { "" };
                    let script = format!("{group}add_duration('main', '{gap_seconds}', {{stop_before_end=true, fallback='fallback'}})\nadd_count('main', 1)");
                    let result = run(&script, rd);
                    assert_eq!(result.added_items.len(), 2);
                    let item = &result.added_items[0];
                    let next = &result.added_items[1];
                    let gap = Duration::seconds(gap_seconds);
                    assert_eq!(item.media_item_id, 99);
                    assert_eq!(item.filler_kind, FillerKind::Fallback);
                    assert_eq!(item.start, base_start());
                    assert_eq!(item.finish, base_start() + gap);
                    assert_eq!(item.out_point - item.in_point, gap);
                    assert_eq!(next.start, item.finish);
                    assert_eq!(
                        next.media_item_id, 1,
                        "rejected main content is not consumed"
                    );
                    assert_eq!(item.guide_group == next.guide_group, locked);
                    assert_eq!(item.preferred_audio_language_code.as_deref(), Some("ja"));
                    assert_eq!(item.preferred_subtitle_language_code.as_deref(), Some("nl"));
                    assert_eq!(result.added_history.len(), 2);
                    for (history, scheduled) in result.added_history.iter().zip(&result.added_items)
                    {
                        let details: serde_json::Value =
                            serde_json::from_str(&history.details).unwrap();
                        assert_eq!(details["MediaItemId"], scheduled.media_item_id);
                        assert_eq!(history.when, scheduled.start);
                        assert_eq!(history.finish, Some(scheduled.finish));
                    }
                }
            }
        }
    }

    #[test]
    fn scripted_add_duration_fallback_snapshot() {
        let mut rd = SchedulingReferenceData::default();
        rd.media_by_key.insert(
            "main".to_owned(),
            vec![SchedulingMediaItem::new(1, Duration::minutes(40))],
        );
        rd.media_by_key.insert(
            "filler".to_owned(),
            vec![SchedulingMediaItem::new(99, Duration::minutes(5))],
        );
        // 30-minute window; main item (40 min) does not fit → fallback used
        let result = run(
            r#"add_duration("main", "00:30:00", {stop_before_end=true, fallback="filler"})"#,
            rd,
        );
        assert_scripted_json_snapshot!(snapshot_items(&result));
    }

    // ── pad_to_next ───────────────────────────────────────────────────────────

    #[test]
    fn scripted_pad_to_next_30_minute_snapshot() {
        let mut rd = SchedulingReferenceData::default();
        rd.media_by_key.insert(
            "bumper".to_owned(),
            vec![SchedulingMediaItem::new(50, Duration::minutes(5))],
        );
        // base_start()=12:00; next 30-min boundary=12:30; 6 × 5-min bumpers fit exactly
        let result = run(r#"pad_to_next("bumper", 30, {trim=true})"#, rd);
        assert_scripted_json_snapshot!(snapshot_items(&result));
    }

    // ── pad_until ─────────────────────────────────────────────────────────────

    #[test]
    fn scripted_pad_until_snapshot() {
        let mut rd = SchedulingReferenceData::default();
        rd.media_by_key.insert(
            "shorts".to_owned(),
            vec![
                SchedulingMediaItem::new(5, Duration::minutes(8)),
                SchedulingMediaItem::new(6, Duration::minutes(8)),
            ],
        );
        // 12:00 → 12:20: two 8-min items fit, 4 min of the third would be trimmed
        let result = run(r#"pad_until("shorts", "12:20", {trim=true})"#, rd);
        assert_scripted_json_snapshot!(snapshot_items(&result));
    }

    // ── skip ──────────────────────────────────────────────────────────────────

    #[test]
    fn scripted_skip_items_snapshot() {
        let mut rd = SchedulingReferenceData::default();
        rd.media_by_key.insert(
            "eps".to_owned(),
            vec![
                SchedulingMediaItem::new(1, Duration::minutes(22)),
                SchedulingMediaItem::new(2, Duration::minutes(22)),
                SchedulingMediaItem::new(3, Duration::minutes(22)),
            ],
        );
        let result = run("skip(\"eps\", 2)\nadd_count(\"eps\", 1)", rd);
        assert_scripted_json_snapshot!(snapshot_items(&result));
    }

    // ── skip_to ───────────────────────────────────────────────────────────────

    #[test]
    fn scripted_skip_to_season_episode_snapshot() {
        let mut rd = SchedulingReferenceData::default();
        let mut ep1 = SchedulingMediaItem::new(10, Duration::minutes(22));
        ep1.season = Some(1);
        ep1.episode = Some(1);
        let mut ep2 = SchedulingMediaItem::new(11, Duration::minutes(22));
        ep2.season = Some(1);
        ep2.episode = Some(2);
        let mut ep3 = SchedulingMediaItem::new(12, Duration::minutes(22));
        ep3.season = Some(2);
        ep3.episode = Some(1);
        rd.media_by_key
            .insert("show".to_owned(), vec![ep1, ep2, ep3]);
        let result = run("skip_to(\"show\", 2, 1)\nadd_count(\"show\", 1)", rd);
        assert_scripted_json_snapshot!(snapshot_items(&result));
    }

    // ── pre_roll ──────────────────────────────────────────────────────────────

    #[test]
    fn scripted_pre_roll_before_each_item_snapshot() {
        let mut rd = SchedulingReferenceData::default();
        rd.media_by_key.insert(
            "bumper".to_owned(),
            vec![SchedulingMediaItem::new(99, Duration::seconds(15))],
        );
        rd.media_by_key.insert(
            "main".to_owned(),
            items_2(Duration::minutes(10), Duration::minutes(10)),
        );
        let result = run(
            "pre_roll_on(\"bumper\")\nadd_count(\"main\", 2)\npre_roll_off()",
            rd,
        );
        assert_scripted_json_snapshot!(snapshot_items(&result));
    }

    // ── guide groups ──────────────────────────────────────────────────────────

    #[test]
    fn scripted_guide_group_lock_unlock_snapshot() {
        let mut rd = SchedulingReferenceData::default();
        rd.media_by_key.insert(
            "movies".to_owned(),
            vec![
                SchedulingMediaItem::new(1, Duration::minutes(90)),
                SchedulingMediaItem::new(2, Duration::minutes(90)),
            ],
        );
        let result = run(
            "lock_guide_group(true, \"Movie Night\")\nadd_count(\"movies\", 1)\nunlock_guide_group()\nadd_count(\"movies\", 1)",
            rd,
        );
        assert_scripted_json_snapshot!(snapshot_items(&result));
    }

    // ── is_done loop ──────────────────────────────────────────────────────────

    #[test]
    fn scripted_is_done_loop_snapshot() {
        let mut rd = SchedulingReferenceData::default();
        rd.media_by_key.insert(
            "ep".to_owned(),
            vec![
                SchedulingMediaItem::new(1, Duration::minutes(30)),
                SchedulingMediaItem::new(2, Duration::minutes(30)),
                SchedulingMediaItem::new(3, Duration::minutes(30)),
                SchedulingMediaItem::new(4, Duration::minutes(30)),
            ],
        );
        let result = run("while not is_done() do\n  add_count(\"ep\", 1)\nend", rd);
        assert_scripted_json_snapshot!(snapshot_items(&result));
    }

    // ── watermark by name ─────────────────────────────────────────────────────

    #[test]
    fn scripted_watermark_by_name_snapshot() {
        let mut rd = SchedulingReferenceData::default();
        rd.media_by_key.insert(
            "main".to_owned(),
            items_2(Duration::minutes(10), Duration::minutes(10)),
        );
        rd.watermark_ids_by_name
            .insert("Main Watermark".to_owned(), 55);
        rd.watermark_ids_by_name.insert("Secondary".to_owned(), 56);
        let result = run(
            "watermark_on(\"Main Watermark\")\nadd_count(\"main\", 1)\nwatermark_on(\"Secondary\")\nadd_count(\"main\", 1)\nwatermark_off(\"Main Watermark\")",
            rd,
        );
        assert_scripted_json_snapshot!(snapshot_items(&result));
    }

    // ── graphics by path ─────────────────────────────────────────────────────

    #[test]
    fn scripted_graphics_by_path_snapshot() {
        let mut rd = SchedulingReferenceData::default();
        rd.media_by_key.insert(
            "main".to_owned(),
            items_2(Duration::minutes(10), Duration::minutes(10)),
        );
        rd.graphics_ids_by_path
            .insert("lower-third.svg".to_owned(), 200);
        let result = run(
            "graphics_on_path(\"lower-third.svg\", '{\"title\":\"Now\"}')\nadd_count(\"main\", 1)\ngraphics_off_path(\"lower-third.svg\")\nadd_count(\"main\", 1)",
            rd,
        );
        assert_eq!(result.added_items[0].graphics_element_ids, vec![200]);
        assert_eq!(
            result.added_items[0].graphics_elements[0]
                .variables
                .as_deref(),
            Some(r#"{"title":"Now"}"#)
        );
        assert!(result.added_items[1].graphics_elements.is_empty());
        assert_scripted_json_snapshot!(snapshot_items(&result));
    }

    // ── add_collection registration ───────────────────────────────────────────

    #[test]
    fn scripted_add_collection_then_schedule_snapshot() {
        let mut rd = SchedulingReferenceData::default();
        rd.collections_by_name.insert(
            "Action Movies".to_owned(),
            vec![
                SchedulingMediaItem::new(20, Duration::minutes(100)),
                SchedulingMediaItem::new(21, Duration::minutes(95)),
            ],
        );
        let result = run(
            "add_collection(\"movies\", \"Action Movies\", \"chronological\")\nadd_count(\"movies\", 2)",
            rd,
        );
        assert_scripted_json_snapshot!(snapshot_items(&result));
    }

    // ── add_show registration ─────────────────────────────────────────────────

    #[test]
    fn scripted_add_show_then_schedule_snapshot() {
        let mut rd = SchedulingReferenceData::default();
        rd.media_by_show_guid.insert(
            "plex://show-abc".to_owned(),
            vec![
                SchedulingMediaItem::new(30, Duration::minutes(22)),
                SchedulingMediaItem::new(31, Duration::minutes(22)),
            ],
        );
        let result = run(
            "add_show(\"myshow\", {plex=\"show-abc\"}, \"chronological\")\nadd_count(\"myshow\", 2)",
            rd,
        );
        assert_scripted_json_snapshot!(snapshot_items(&result));
    }

    // ── history resume via add_collection ─────────────────────────────────────

    #[test]
    fn scripted_history_resume_on_collection_snapshot() {
        let mut rd = SchedulingReferenceData::default();
        rd.collections_by_name.insert(
            "Shorts".to_owned(),
            vec![
                SchedulingMediaItem::new(1, Duration::minutes(5)),
                SchedulingMediaItem::new(2, Duration::minutes(5)),
                SchedulingMediaItem::new(3, Duration::minutes(5)),
            ],
        );
        let history_key = history_key_for_content("shorts", PlaybackOrder::Chronological);
        let when = base_start() - Duration::hours(1);
        let finish_time = when + Duration::minutes(5);
        rd.existing_history.push(ersatzrs_domain::PlayoutHistory {
            id: 1,
            playout_id: 100,
            key: history_key,
            when,
            finish: Some(finish_time),
            index: 0,
            details: r#"{"MediaItemId":1}"#.to_owned(),
            ..ersatzrs_domain::PlayoutHistory::default()
        });
        let result = run(
            "add_collection(\"shorts\", \"Shorts\", \"chronological\")\nadd_count(\"shorts\", 2)",
            rd,
        );
        assert_scripted_json_snapshot!(snapshot_items(&result));
    }

    // ── rewind ────────────────────────────────────────────────────────────────

    #[test]
    fn scripted_rewind_resets_enumerator_snapshot() {
        let mut rd = SchedulingReferenceData::default();
        rd.media_by_key.insert(
            "ep".to_owned(),
            vec![
                SchedulingMediaItem::new(1, Duration::minutes(10)),
                SchedulingMediaItem::new(2, Duration::minutes(10)),
            ],
        );
        let result = run(
            "add_count(\"ep\", 2)\nrewind(\"ep\")\nadd_count(\"ep\", 1)",
            rd,
        );
        assert_scripted_json_snapshot!(snapshot_items(&result));
    }

    // ── add_marathon grouping ─────────────────────────────────────────────────

    #[test]
    fn scripted_add_marathon_by_show_snapshot() {
        // Two shows via different provider sources; show_id 1 under "plex",
        // show_id 2 under "tvdb".  add_marathon groups by show; add_count
        // cycles one item per group (round-robin): s1e1, s2e1, s1e2, s2e2.
        let mut rd = SchedulingReferenceData::default();
        let mut ep1 = SchedulingMediaItem::new(10, Duration::minutes(22));
        ep1.show_id = Some(1);
        ep1.season = Some(1);
        ep1.episode = Some(1);
        let mut ep2 = SchedulingMediaItem::new(11, Duration::minutes(22));
        ep2.show_id = Some(1);
        ep2.season = Some(1);
        ep2.episode = Some(2);
        let mut ep3 = SchedulingMediaItem::new(20, Duration::minutes(22));
        ep3.show_id = Some(2);
        ep3.season = Some(1);
        ep3.episode = Some(1);
        let mut ep4 = SchedulingMediaItem::new(21, Duration::minutes(22));
        ep4.show_id = Some(2);
        ep4.season = Some(1);
        ep4.episode = Some(2);
        rd.media_by_show_guid
            .insert("plex://show-1".to_owned(), vec![ep1, ep2]);
        rd.media_by_show_guid
            .insert("tvdb://show-2".to_owned(), vec![ep3, ep4]);
        let result = run(
            r#"
add_marathon("marathon", {plex="show-1", tvdb="show-2"}, {}, "show")
add_count("marathon", 4)
"#,
            rd,
        );
        assert_scripted_json_snapshot!(snapshot_items(&result));
    }

    #[test]
    fn scripted_add_marathon_empty_guids_snapshot() {
        // Edge case: marathon with no matching guids produces no items.
        let rd = SchedulingReferenceData::default();
        let result = run(
            r#"
add_marathon("marathon", {plex="nonexistent-show"}, {}, "show")
add_count("marathon", 3)
"#,
            rd,
        );
        assert_scripted_json_snapshot!(snapshot_items(&result));
    }

    // ── create_playlist inline assembly ──────────────────────────────────────

    #[test]
    fn scripted_create_playlist_from_enumerators_snapshot() {
        // Two enumerators registered via add_collection; create_playlist
        // assembles them into a single playlist with one slot from each per cycle.
        let mut rd = SchedulingReferenceData::default();
        rd.collections_by_name.insert(
            "Shorts".to_owned(),
            vec![
                SchedulingMediaItem::new(1, Duration::minutes(5)),
                SchedulingMediaItem::new(2, Duration::minutes(5)),
            ],
        );
        rd.collections_by_name.insert(
            "Bumpers".to_owned(),
            vec![SchedulingMediaItem::new(99, Duration::minutes(2))],
        );
        let result = run(
            r#"
add_collection("shorts", "Shorts", "chronological")
add_collection("bumpers", "Bumpers", "chronological")
create_playlist("pl", {shorts=1, bumpers=1})
add_count("pl", 4)
"#,
            rd,
        );
        assert_scripted_json_snapshot!(snapshot_items(&result));
    }

    #[test]
    fn scripted_create_playlist_missing_key_snapshot() {
        // Edge case: create_playlist references a key never registered;
        // the unknown key is silently skipped; valid keys still schedule.
        let mut rd = SchedulingReferenceData::default();
        rd.collections_by_name.insert(
            "Shorts".to_owned(),
            vec![SchedulingMediaItem::new(5, Duration::minutes(5))],
        );
        let result = run(
            r#"
add_collection("shorts", "Shorts", "chronological")
create_playlist("pl", {shorts=2, nonexistent=1})
add_count("pl", 2)
"#,
            rd,
        );
        assert_scripted_json_snapshot!(snapshot_items(&result));
    }

    // ── discard_attempts ──────────────────────────────────────────────────────

    #[test]
    fn scripted_discard_attempts_snapshot() {
        let mut rd = SchedulingReferenceData::default();
        rd.media_by_key.insert(
            "ep".to_owned(),
            vec![
                SchedulingMediaItem::new(1, Duration::minutes(40)),
                SchedulingMediaItem::new(2, Duration::minutes(15)),
                SchedulingMediaItem::new(3, Duration::minutes(15)),
            ],
        );
        // 30-min window; item 1 (40 min) discarded, item 2 (15 min) + item 3 (15 min) fit
        let result = run(
            r#"add_duration("ep", "00:30:00", {stop_before_end=true, discard_attempts=1})"#,
            rd,
        );
        assert_scripted_json_snapshot!(snapshot_items(&result));
    }
}
