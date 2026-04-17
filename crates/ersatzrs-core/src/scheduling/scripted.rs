use std::{cell::RefCell, collections::BTreeMap, fs, rc::Rc};

use chrono::{DateTime, Datelike, Duration, NaiveTime, TimeZone, Timelike, Utc};
use ersatzrs_domain::{EntityId, FillerKind, PlaybackOrder, PlayoutAnchor};
use mlua::{Lua, Table};

use super::{
    history, history_details_for_media_item, history_key_for_content,
    history_key_for_marathon_content, parse_duration, parse_hhmm, playout_item, shuffle_with_seed,
    CurrentHistoryContext, MediaEnumerator, PlaylistEnumerator, PlayoutBuildMode,
    PlayoutSchedulingEngine, SchedulingCollectionKey, SchedulingContentGuid, SchedulingError,
    SchedulingPlaylistItem, SchedulingReferenceData, SchedulingRequest, SchedulingResult,
};

// ── public engine ─────────────────────────────────────────────────────────────

#[derive(Debug, Clone, Default)]
pub struct ScriptedSchedulingEngine;

impl PlayoutSchedulingEngine for ScriptedSchedulingEngine {
    fn schedule(&self, request: SchedulingRequest) -> Result<SchedulingResult, SchedulingError> {
        let script_file = request
            .playout
            .schedule_file
            .clone()
            .ok_or(SchedulingError::MissingScheduleFile)?;
        let script = fs::read_to_string(&script_file)
            .map_err(|_| SchedulingError::ScheduleFileMissing(script_file.clone()))?;
        self.schedule_script(&script, request)
    }
}

impl ScriptedSchedulingEngine {
    /// Build a timeline by executing a Lua scheduling script.
    ///
    /// # Errors
    ///
    /// Returns an error when Lua compilation or execution fails.
    #[allow(clippy::too_many_lines, clippy::needless_pass_by_value)]
    pub fn schedule_script(
        &self,
        script: &str,
        request: SchedulingRequest,
    ) -> Result<SchedulingResult, SchedulingError> {
        let state = Rc::new(RefCell::new(ScriptState::new(
            request.playout.id,
            request.start,
            request.finish,
            request.mode,
            request.reference_data,
        )));
        let lua = Lua::new();
        register_all(&lua, &state).map_err(lua_error)?;
        run_lua_script(&lua, script).map_err(lua_error)?;

        let mut state = state.borrow().clone();
        let anchor_time = state
            .added_items
            .iter()
            .map(|item| item.finish)
            .max()
            .unwrap_or(state.current_time);
        Ok(SchedulingResult {
            clear_items: state.mode == PlayoutBuildMode::Reset,
            remove_before: Some(request.start),
            anchor: Some(PlayoutAnchor {
                next_start: anchor_time,
                ..PlayoutAnchor::default()
            }),
            added_items: std::mem::take(&mut state.added_items),
            added_history: std::mem::take(&mut state.added_history),
            history_to_remove: std::mem::take(&mut state.history_to_remove),
            ..SchedulingResult::default()
        })
    }
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

#[allow(clippy::too_many_lines)]
fn register_all(lua: &Lua, state: &Rc<RefCell<ScriptState>>) -> Result<(), mlua::Error> {
    let globals = lua.globals();

    // schedule_count(content, count)  [legacy alias]
    {
        let s = Rc::clone(state);
        globals.set(
            "schedule_count",
            lua.create_function_mut(move |_, (content, count): (String, usize)| {
                let mut st = s.borrow_mut();
                for _ in 0..count {
                    st.add_item_simple(&content, None);
                }
                Ok(())
            })?,
        )?;
    }

    // schedule_duration(content, duration)  [legacy alias]
    {
        let s = Rc::clone(state);
        globals.set(
            "schedule_duration",
            lua.create_function_mut(move |_, (content, duration): (String, String)| {
                let target = {
                    let st = s.borrow();
                    st.current_time + parse_duration(&duration).map_err(mlua::Error::external)?
                };
                let opts = AddOptions::default();
                s.borrow_mut()
                    .add_duration_internal(&content, target, &opts);
                Ok(())
            })?,
        )?;
    }

    // set_watermark(id)  [legacy: by numeric ID]
    {
        let s = Rc::clone(state);
        globals.set(
            "set_watermark",
            lua.create_function_mut(move |_, id: EntityId| {
                s.borrow_mut().watermark_ids.push(id);
                Ok(())
            })?,
        )?;
    }

    // clear_watermarks()  [legacy]
    {
        let s = Rc::clone(state);
        globals.set(
            "clear_watermarks",
            lua.create_function_mut(move |_, ()| {
                s.borrow_mut().watermark_ids.clear();
                Ok(())
            })?,
        )?;
    }

    // graphics_on(id)  [legacy: by numeric ID]
    {
        let s = Rc::clone(state);
        globals.set(
            "graphics_on",
            lua.create_function_mut(move |_, id: EntityId| {
                let mut st = s.borrow_mut();
                if !st.graphics_element_ids.contains(&id) {
                    st.graphics_element_ids.push(id);
                }
                Ok(())
            })?,
        )?;
    }

    // graphics_off(id?)  [legacy]
    {
        let s = Rc::clone(state);
        globals.set(
            "graphics_off",
            lua.create_function_mut(move |_, id: Option<EntityId>| {
                let mut st = s.borrow_mut();
                if let Some(id) = id {
                    st.graphics_element_ids.retain(|c| *c != id);
                } else {
                    st.graphics_element_ids.clear();
                }
                Ok(())
            })?,
        )?;
    }

    // wait_until(hh:mm[:ss])
    {
        let s = Rc::clone(state);
        globals.set(
            "wait_until",
            lua.create_function_mut(move |_, value: String| {
                let (hour, minute, second) = parse_hhmm(&value).map_err(mlua::Error::external)?;
                let mut st = s.borrow_mut();
                let date = st.current_time.date_naive();
                let target = date
                    .and_hms_opt(hour, minute, second)
                    .ok_or_else(|| mlua::Error::external("invalid time"))?;
                st.current_time = DateTime::from_naive_utc_and_offset(target, Utc);
                Ok(())
            })?,
        )?;
    }

    // add_count(content, count)
    {
        let s = Rc::clone(state);
        globals.set(
            "add_count",
            lua.create_function_mut(move |_, (content, count): (String, usize)| {
                let mut st = s.borrow_mut();
                for _ in 0..count {
                    st.add_item_simple(&content, None);
                }
                Ok(())
            })?,
        )?;
    }

    // add_all(content, opts?)
    {
        let s = Rc::clone(state);
        globals.set(
            "add_all",
            lua.create_function_mut(move |_, (content, opts): (String, Option<Table>)| {
                let count = s.borrow_mut().key_len(&content);
                let opts = read_add_options(opts.as_ref());
                for _ in 0..count {
                    s.borrow_mut().add_item_with_opts(&content, None, &opts);
                }
                Ok(())
            })?,
        )?;
    }

    // add_duration(content, duration, opts?)
    {
        let s = Rc::clone(state);
        globals.set(
            "add_duration",
            lua.create_function_mut(
                move |_, (content, duration, opts): (String, String, Option<Table>)| {
                    let opts = read_add_options(opts.as_ref());
                    let target = {
                        let st = s.borrow();
                        st.current_time
                            + parse_duration(&duration).map_err(mlua::Error::external)?
                    };
                    s.borrow_mut()
                        .add_duration_internal(&content, target, &opts);
                    Ok(())
                },
            )?,
        )?;
    }

    // pad_to_next(content, minutes, opts?)
    {
        let s = Rc::clone(state);
        globals.set(
            "pad_to_next",
            lua.create_function_mut(
                move |_, (content, minutes, opts): (String, i64, Option<Table>)| {
                    let opts = read_add_options(opts.as_ref());
                    let target = {
                        let st = s.borrow();
                        next_n_minute_boundary(st.current_time, minutes)
                    };
                    s.borrow_mut()
                        .add_duration_internal(&content, target, &opts);
                    Ok(())
                },
            )?,
        )?;
    }

    // pad_until(content, time_str, opts?)  — opts may contain {tomorrow=true}
    {
        let s = Rc::clone(state);
        globals.set(
            "pad_until",
            lua.create_function_mut(
                move |_, (content, time_str, opts): (String, String, Option<Table>)| {
                    let opts = read_add_options(opts.as_ref());
                    let target = {
                        let st = s.borrow();
                        let (hour, minute, second) =
                            parse_hhmm(&time_str).map_err(mlua::Error::external)?;
                        let nt = NaiveTime::from_hms_opt(hour, minute, second)
                            .ok_or_else(|| mlua::Error::external("invalid time"))?;
                        pad_until_target(st.current_time, nt, opts.tomorrow)
                    };
                    s.borrow_mut()
                        .add_duration_internal(&content, target, &opts);
                    Ok(())
                },
            )?,
        )?;
    }

    // skip(content, count)
    {
        let s = Rc::clone(state);
        globals.set(
            "skip",
            lua.create_function_mut(move |_, (content, count): (String, usize)| {
                let mut st = s.borrow_mut();
                let e = st.enumerator(&content);
                for _ in 0..count {
                    e.advance();
                }
                Ok(())
            })?,
        )?;
    }

    // skip_to(content, season, episode)
    {
        let s = Rc::clone(state);
        globals.set(
            "skip_to",
            lua.create_function_mut(move |_, (content, season, episode): (String, i32, i32)| {
                let mut st = s.borrow_mut();
                let len = st.enumerator(&content).len();
                for _ in 0..len {
                    let matches = st.enumerator(&content).current().is_some_and(|item| {
                        item.season == Some(season) && item.episode == Some(episode)
                    });
                    if matches {
                        break;
                    }
                    st.enumerator(&content).advance();
                }
                Ok(())
            })?,
        )?;
    }

    // rewind(content)
    {
        let s = Rc::clone(state);
        globals.set(
            "rewind",
            lua.create_function_mut(move |_, content: String| {
                s.borrow_mut().enumerator(&content).set_index(0);
                Ok(())
            })?,
        )?;
    }

    // pre_roll_on(content_key)
    {
        let s = Rc::clone(state);
        globals.set(
            "pre_roll_on",
            lua.create_function_mut(move |_, key: String| {
                s.borrow_mut().pre_roll_key = Some(key);
                Ok(())
            })?,
        )?;
    }

    // pre_roll_off()
    {
        let s = Rc::clone(state);
        globals.set(
            "pre_roll_off",
            lua.create_function_mut(move |_, ()| {
                s.borrow_mut().pre_roll_key = None;
                Ok(())
            })?,
        )?;
    }

    // lock_guide_group(advance?, title?)
    {
        let s = Rc::clone(state);
        globals.set(
            "lock_guide_group",
            lua.create_function_mut(move |_, (advance, title): (Option<bool>, Option<String>)| {
                s.borrow_mut()
                    .lock_guide_group(advance.unwrap_or(true), title);
                Ok(())
            })?,
        )?;
    }

    // unlock_guide_group()
    {
        let s = Rc::clone(state);
        globals.set(
            "unlock_guide_group",
            lua.create_function_mut(move |_, ()| {
                s.borrow_mut().unlock_guide_group();
                Ok(())
            })?,
        )?;
    }

    // is_done()  →  bool
    {
        let s = Rc::clone(state);
        globals.set(
            "is_done",
            lua.create_function_mut(move |_, ()| {
                let mut st = s.borrow_mut();
                let current = st.current_time;
                if st.last_progress_time == Some(current) {
                    st.no_progress_counter += 1;
                    if st.no_progress_counter >= 20 {
                        return Err(mlua::Error::external(
                            "is_done() called 20 times without time advancing",
                        ));
                    }
                } else {
                    st.last_progress_time = Some(current);
                    st.no_progress_counter = 0;
                }
                Ok(current >= st.finish)
            })?,
        )?;
    }

    // current_time()  →  RFC-3339 string
    {
        let s = Rc::clone(state);
        globals.set(
            "current_time",
            lua.create_function(move |_, ()| Ok(s.borrow().current_time.to_rfc3339()))?,
        )?;
    }

    // finish_time()  →  RFC-3339 string
    {
        let s = Rc::clone(state);
        globals.set(
            "finish_time",
            lua.create_function(move |_, ()| Ok(s.borrow().finish.to_rfc3339()))?,
        )?;
    }

    // watermark_on(name)
    {
        let s = Rc::clone(state);
        globals.set(
            "watermark_on",
            lua.create_function_mut(move |_, name: String| {
                let mut st = s.borrow_mut();
                if let Some(&id) = st.reference_data.watermark_ids_by_name.get(&name) {
                    if !st.watermark_ids.contains(&id) {
                        st.watermark_ids.push(id);
                    }
                }
                Ok(())
            })?,
        )?;
    }

    // watermark_off(name?)
    {
        let s = Rc::clone(state);
        globals.set(
            "watermark_off",
            lua.create_function_mut(move |_, name: Option<String>| {
                let mut st = s.borrow_mut();
                match name {
                    Some(n) => {
                        if let Some(&id) = st.reference_data.watermark_ids_by_name.get(&n) {
                            st.watermark_ids.retain(|c| *c != id);
                        }
                    }
                    None => st.watermark_ids.clear(),
                }
                Ok(())
            })?,
        )?;
    }

    // graphics_on_path(path, variables_json?)
    {
        let s = Rc::clone(state);
        globals.set(
            "graphics_on_path",
            lua.create_function_mut(move |_, (path, _vars): (String, Option<String>)| {
                let mut st = s.borrow_mut();
                if let Some(&id) = st.reference_data.graphics_ids_by_path.get(&path) {
                    if !st.graphics_element_ids.contains(&id) {
                        st.graphics_element_ids.push(id);
                    }
                }
                Ok(())
            })?,
        )?;
    }

    // graphics_off_path(path?)
    {
        let s = Rc::clone(state);
        globals.set(
            "graphics_off_path",
            lua.create_function_mut(move |_, path: Option<String>| {
                let mut st = s.borrow_mut();
                match path {
                    Some(p) => {
                        if let Some(&id) = st.reference_data.graphics_ids_by_path.get(&p) {
                            st.graphics_element_ids.retain(|c| *c != id);
                        }
                    }
                    None => st.graphics_element_ids.clear(),
                }
                Ok(())
            })?,
        )?;
    }

    // add_collection(key, name, order?)
    {
        let s = Rc::clone(state);
        globals.set(
            "add_collection",
            lua.create_function_mut(
                move |_, (key, name, order): (String, String, Option<String>)| {
                    let po = parse_playback_order(order.as_deref());
                    let mut st = s.borrow_mut();
                    if st.enumerators.contains_key(&key) {
                        return Ok(());
                    }
                    let items = st
                        .reference_data
                        .collections_by_name
                        .get(&name)
                        .cloned()
                        .unwrap_or_default();
                    let mut e = MediaEnumerator::new(&key, items).with_playback_order(po);
                    st.apply_history(&key, &mut e, po);
                    st.enumerators.insert(key, e);
                    Ok(())
                },
            )?,
        )?;
    }

    // add_multi_collection(key, name, order?)
    {
        let s = Rc::clone(state);
        globals.set(
            "add_multi_collection",
            lua.create_function_mut(
                move |_, (key, name, order): (String, String, Option<String>)| {
                    let po = parse_playback_order(order.as_deref());
                    let mut st = s.borrow_mut();
                    if st.enumerators.contains_key(&key) {
                        return Ok(());
                    }
                    let items = st
                        .reference_data
                        .multi_collections_by_name
                        .get(&name)
                        .cloned()
                        .unwrap_or_default();
                    let mut e = MediaEnumerator::new(&key, items).with_playback_order(po);
                    st.apply_history(&key, &mut e, po);
                    st.enumerators.insert(key, e);
                    Ok(())
                },
            )?,
        )?;
    }

    // add_smart_collection(key, name, order?)
    {
        let s = Rc::clone(state);
        globals.set(
            "add_smart_collection",
            lua.create_function_mut(
                move |_, (key, name, order): (String, String, Option<String>)| {
                    let po = parse_playback_order(order.as_deref());
                    let mut st = s.borrow_mut();
                    if st.enumerators.contains_key(&key) {
                        return Ok(());
                    }
                    let items = st
                        .reference_data
                        .smart_collections_by_name
                        .get(&name)
                        .cloned()
                        .unwrap_or_default();
                    let mut e = MediaEnumerator::new(&key, items).with_playback_order(po);
                    st.apply_history(&key, &mut e, po);
                    st.enumerators.insert(key, e);
                    Ok(())
                },
            )?,
        )?;
    }

    // add_search(key, query, order?)
    {
        let s = Rc::clone(state);
        globals.set(
            "add_search",
            lua.create_function_mut(
                move |_, (key, query, order): (String, String, Option<String>)| {
                    let po = parse_playback_order(order.as_deref());
                    let mut st = s.borrow_mut();
                    if st.enumerators.contains_key(&key) {
                        return Ok(());
                    }
                    let items = st
                        .reference_data
                        .searches_by_query
                        .get(&query)
                        .cloned()
                        .unwrap_or_default();
                    let mut e = MediaEnumerator::new(&key, items).with_playback_order(po);
                    st.apply_history(&key, &mut e, po);
                    st.enumerators.insert(key, e);
                    Ok(())
                },
            )?,
        )?;
    }

    // add_show(key, guids_table, order?)
    {
        let s = Rc::clone(state);
        globals.set(
            "add_show",
            lua.create_function_mut(
                move |_, (key, guids_table, order): (String, Table, Option<String>)| {
                    let po = parse_playback_order(order.as_deref());
                    let mut st = s.borrow_mut();
                    if st.enumerators.contains_key(&key) {
                        return Ok(());
                    }
                    let mut items = Vec::new();
                    let mut seen = std::collections::BTreeSet::new();
                    for pair in guids_table.pairs::<String, String>() {
                        let (source, value) = pair?;
                        let guid_key = format!("{source}://{value}");
                        if let Some(show_items) =
                            st.reference_data.media_by_show_guid.get(&guid_key)
                        {
                            for item in show_items {
                                if seen.insert(item.id) {
                                    items.push(item.clone());
                                }
                            }
                        }
                    }
                    let mut e = MediaEnumerator::new(&key, items).with_playback_order(po);
                    st.apply_history(&key, &mut e, po);
                    st.enumerators.insert(key, e);
                    Ok(())
                },
            )?,
        )?;
    }

    // add_playlist(key, name, group?)
    {
        let s = Rc::clone(state);
        globals.set(
            "add_playlist",
            lua.create_function_mut(
                move |_, (key, name, group): (String, String, Option<String>)| {
                    let mut st = s.borrow_mut();
                    if st.enumerators.contains_key(&key) {
                        return Ok(());
                    }
                    let items = st
                        .reference_data
                        .playlists_by_group_and_name
                        .get(&(group.clone(), name.clone()))
                        .or_else(|| {
                            st.reference_data
                                .playlists_by_group_and_name
                                .get(&(None, name.clone()))
                        })
                        .cloned()
                        .unwrap_or_default();
                    let mut e =
                        MediaEnumerator::new(&key, items).with_playback_order(PlaybackOrder::None);
                    st.apply_history(&key, &mut e, PlaybackOrder::None);
                    st.enumerators.insert(key, e);
                    Ok(())
                },
            )?,
        )?;
    }

    // add_key(key)  — register a pre-loaded direct-key from reference_data.media_by_key
    {
        let s = Rc::clone(state);
        globals.set(
            "add_key",
            lua.create_function_mut(move |_, key: String| {
                let mut st = s.borrow_mut();
                if st.enumerators.contains_key(&key) {
                    return Ok(());
                }
                let items = st
                    .reference_data
                    .media_by_key
                    .get(&key)
                    .cloned()
                    .unwrap_or_default();
                st.enumerators
                    .insert(key.clone(), MediaEnumerator::new(&key, items));
                Ok(())
            })?,
        )?;
    }

    // add_marathon(key, guids_table, searches_table, group_by, opts?)
    // opts = {shuffle_groups=false, item_order="season_episode", play_all=false}
    {
        let s = Rc::clone(state);
        globals.set(
            "add_marathon",
            lua.create_function_mut(
                move |_,
                      (key, guids_table, searches_table, group_by, opts): (
                    String,
                    Table,
                    Table,
                    String,
                    Option<Table>,
                )| {
                    let mut st = s.borrow_mut();
                    if st.enumerators.contains_key(&key)
                        || st.playlist_enumerators.contains_key(&key)
                    {
                        return Ok(());
                    }

                    let shuffle_groups = opts
                        .as_ref()
                        .and_then(|t| t.get::<Option<bool>>("shuffle_groups").ok().flatten())
                        .unwrap_or(false);
                    let play_all = opts
                        .as_ref()
                        .and_then(|t| t.get::<Option<bool>>("play_all").ok().flatten())
                        .unwrap_or(false);
                    let item_order = parse_playback_order(
                        opts.as_ref()
                            .and_then(|t| t.get::<Option<String>>("item_order").ok().flatten())
                            .as_deref(),
                    );

                    let mut guids: Vec<SchedulingContentGuid> = Vec::new();
                    for pair in guids_table.pairs::<String, String>() {
                        let (source, value) = pair?;
                        guids.push(SchedulingContentGuid { source, value });
                    }
                    guids.sort_by(|left, right| {
                        left.source
                            .cmp(&right.source)
                            .then(left.value.cmp(&right.value))
                    });

                    let mut searches: Vec<String> = Vec::new();
                    for v in searches_table.sequence_values::<String>() {
                        searches.push(v?);
                    }

                    let mut items = st.reference_data.marathon_playlist_items(
                        None, &guids, &searches, &group_by, item_order, play_all,
                    );

                    if shuffle_groups {
                        let seed = st
                            .playout_id
                            .unsigned_abs()
                            .wrapping_add(st.enumerators.len() as u64);
                        shuffle_with_seed(&mut items, seed);
                    }

                    let history_key = history_key_for_marathon_content(&key, item_order, &group_by);
                    let relevant_history = st.collect_playlist_history(&history_key);

                    let mut playlist = PlaylistEnumerator::new(items);
                    if !relevant_history.is_empty() {
                        playlist.apply_history(&relevant_history);
                    }

                    st.playlist_history_keys.insert(key.clone(), history_key);
                    st.playlist_enumerators.insert(key, playlist);
                    Ok(())
                },
            )?,
        )?;
    }

    // create_playlist(key, items_table)
    // items_table = {enumerator_key = count, ...}
    // Assembles an inline playlist from already-registered enumerators.
    {
        let s = Rc::clone(state);
        globals.set(
            "create_playlist",
            lua.create_function_mut(move |_, (key, items_table): (String, Table)| {
                let mut st = s.borrow_mut();
                if st.enumerators.contains_key(&key) || st.playlist_enumerators.contains_key(&key) {
                    return Ok(());
                }

                let mut playlist_items: Vec<SchedulingPlaylistItem> = Vec::new();
                let mut playlist_index: i64 = 0;
                let mut entries = Vec::new();
                for pair in items_table.pairs::<String, usize>() {
                    let (child_key, count) = pair?;
                    entries.push((child_key, count));
                }
                entries.sort_by(|left, right| left.0.cmp(&right.0));
                for (child_key, count) in entries {
                    let (playback_order, media_items) =
                        if let Some(e) = st.enumerators.get(&child_key) {
                            (e.playback_order, e.items.clone())
                        } else {
                            continue;
                        };
                    for _ in 0..count {
                        playlist_items.push(SchedulingPlaylistItem {
                            collection_key: SchedulingCollectionKey::fake_playlist_item(
                                playlist_index,
                            ),
                            playback_order,
                            play_all: false,
                            count: None,
                            media_items: media_items.clone(),
                        });
                        playlist_index += 1;
                    }
                }

                let history_key = history_key_for_content(&key, PlaybackOrder::None);
                let relevant_history = st.collect_playlist_history(&history_key);

                let mut playlist = PlaylistEnumerator::new(playlist_items);
                if !relevant_history.is_empty() {
                    playlist.apply_history(&relevant_history);
                }

                st.playlist_history_keys.insert(key.clone(), history_key);
                st.playlist_enumerators.insert(key, playlist);
                Ok(())
            })?,
        )?;
    }

    Ok(())
}

// ── script state ──────────────────────────────────────────────────────────────

#[derive(Debug, Clone)]
struct ScriptState {
    playout_id: EntityId,
    start: DateTime<Utc>,
    current_time: DateTime<Utc>,
    finish: DateTime<Utc>,
    mode: PlayoutBuildMode,
    reference_data: SchedulingReferenceData,
    watermark_ids: Vec<EntityId>,
    graphics_element_ids: Vec<EntityId>,
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
}

impl ScriptState {
    fn new(
        playout_id: EntityId,
        start: DateTime<Utc>,
        finish: DateTime<Utc>,
        mode: PlayoutBuildMode,
        reference_data: SchedulingReferenceData,
    ) -> Self {
        let mut st = Self {
            playout_id,
            start,
            current_time: start,
            finish,
            mode,
            reference_data,
            watermark_ids: Vec::new(),
            graphics_element_ids: Vec::new(),
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
        if self.playlist_enumerators.contains_key(key) {
            let playlist = self.playlist_enumerators.get(key).unwrap();
            let media_item = playlist.current().cloned()?;
            let ctx = playlist.current_history_context();
            Some((media_item, ctx))
        } else {
            // lazy-load from media_by_key, mirroring the old enumerator() behaviour
            let e = self.enumerator(key);
            let media_item = e.current().cloned()?;
            let idx = e.current_index();
            Some((media_item, CurrentHistoryContext::Single { index: idx }))
        }
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

    fn history_key_for(&self, key: &str) -> String {
        if let Some(k) = self.playlist_history_keys.get(key) {
            k.clone()
        } else {
            let po = self
                .enumerators
                .get(key)
                .map(|e| e.playback_order)
                .unwrap_or(PlaybackOrder::None);
            history_key_for_content(key, po)
        }
    }

    fn push_history_ctx(
        &mut self,
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
            CurrentHistoryContext::Playlist {
                index,
                child_key,
                child_index,
                child_details,
            } => {
                self.added_history.push(history(
                    self.playout_id,
                    history_key,
                    item,
                    None,
                    *index,
                    history_details_for_media_item(media_item),
                    false,
                ));
                self.added_history.push(history(
                    self.playout_id,
                    history_key,
                    item,
                    Some(child_key.clone()),
                    *child_index,
                    child_details.clone(),
                    true,
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
        let item = playout_item(
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
        let history_key = self.history_key_for(key);
        self.push_history_ctx(&item, &media_item, &history_ctx, &history_key);
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
        let item = playout_item(
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
        let history_key = self.history_key_for(key);
        self.push_history_ctx(&item, &media_item, &history_ctx, &history_key);
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

    // ── regression: original test ─────────────────────────────────────────────

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

        insta::assert_json_snapshot!(serde_json::json!({
            "items": result.added_items.iter().map(|item| serde_json::json!({
                "media": item.media_item_id,
                "start": item.start.to_rfc3339(),
                "finish": item.finish.to_rfc3339(),
                "watermarks": item.watermark_ids,
                "graphics": item.graphics_element_ids,
            })).collect::<Vec<_>>(),
        }));
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
        insta::assert_json_snapshot!(snapshot_items(&result));
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
        insta::assert_json_snapshot!(snapshot_items(&result));
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
        insta::assert_json_snapshot!(snapshot_items(&result));
    }

    // ── add_duration with fallback ────────────────────────────────────────────

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
        insta::assert_json_snapshot!(snapshot_items(&result));
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
        insta::assert_json_snapshot!(snapshot_items(&result));
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
        insta::assert_json_snapshot!(snapshot_items(&result));
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
        insta::assert_json_snapshot!(snapshot_items(&result));
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
        insta::assert_json_snapshot!(snapshot_items(&result));
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
        insta::assert_json_snapshot!(snapshot_items(&result));
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
        insta::assert_json_snapshot!(snapshot_items(&result));
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
        insta::assert_json_snapshot!(snapshot_items(&result));
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
        insta::assert_json_snapshot!(snapshot_items(&result));
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
            "graphics_on_path(\"lower-third.svg\")\nadd_count(\"main\", 1)\ngraphics_off_path(\"lower-third.svg\")\nadd_count(\"main\", 1)",
            rd,
        );
        insta::assert_json_snapshot!(snapshot_items(&result));
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
        insta::assert_json_snapshot!(snapshot_items(&result));
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
        insta::assert_json_snapshot!(snapshot_items(&result));
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
        insta::assert_json_snapshot!(snapshot_items(&result));
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
        insta::assert_json_snapshot!(snapshot_items(&result));
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
        insta::assert_json_snapshot!(snapshot_items(&result));
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
        insta::assert_json_snapshot!(snapshot_items(&result));
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
        insta::assert_json_snapshot!(snapshot_items(&result));
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
        insta::assert_json_snapshot!(snapshot_items(&result));
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
        insta::assert_json_snapshot!(snapshot_items(&result));
    }
}
