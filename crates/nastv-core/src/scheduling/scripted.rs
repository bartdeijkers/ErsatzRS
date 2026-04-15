use std::{cell::RefCell, fs, rc::Rc};

use chrono::{DateTime, Duration, Utc};
use mlua::Lua;
use nastv_domain::{EntityId, FillerKind, PlayoutAnchor};

use super::{
    history, lookup_media, parse_duration, parse_hhmm, playout_item, MediaEnumerator,
    PlayoutBuildMode, PlayoutSchedulingEngine, SchedulingError, SchedulingReferenceData,
    SchedulingRequest, SchedulingResult,
};

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
            request.reference_data,
        )));
        let lua = Lua::new();
        let globals = lua.globals();

        {
            let state = Rc::clone(&state);
            globals
                .set(
                    "schedule_count",
                    lua.create_function_mut(move |_, (content, count): (String, usize)| {
                        let mut state = state.borrow_mut();
                        for _ in 0..count {
                            state.add_next(&content, None);
                        }
                        Ok(())
                    })
                    .map_err(lua_error)?,
                )
                .map_err(lua_error)?;
        }

        {
            let state = Rc::clone(&state);
            globals
                .set(
                    "schedule_duration",
                    lua.create_function_mut(move |_, (content, duration): (String, String)| {
                        let target = {
                            let state_ref = state.borrow();
                            state_ref.current_time
                                + parse_duration(&duration).map_err(mlua::Error::external)?
                        };
                        let mut state = state.borrow_mut();
                        while state.current_time < target {
                            let Some(item_duration) = state.current_duration(&content) else {
                                break;
                            };
                            let duration = item_duration.min(target - state.current_time);
                            state.add_next(&content, Some(duration));
                        }
                        Ok(())
                    })
                    .map_err(lua_error)?,
                )
                .map_err(lua_error)?;
        }

        {
            let state = Rc::clone(&state);
            globals
                .set(
                    "wait_until",
                    lua.create_function_mut(move |_, value: String| {
                        let (hour, minute, second) =
                            parse_hhmm(&value).map_err(mlua::Error::external)?;
                        let mut state = state.borrow_mut();
                        let date = state.current_time.date_naive();
                        let target = date
                            .and_hms_opt(hour, minute, second)
                            .ok_or_else(|| mlua::Error::external("invalid time"))?;
                        state.current_time = DateTime::from_naive_utc_and_offset(target, Utc);
                        Ok(())
                    })
                    .map_err(lua_error)?,
                )
                .map_err(lua_error)?;
        }

        {
            let state = Rc::clone(&state);
            globals
                .set(
                    "set_watermark",
                    lua.create_function_mut(move |_, id: EntityId| {
                        state.borrow_mut().watermark_ids.push(id);
                        Ok(())
                    })
                    .map_err(lua_error)?,
                )
                .map_err(lua_error)?;
        }

        {
            let state = Rc::clone(&state);
            globals
                .set(
                    "clear_watermarks",
                    lua.create_function_mut(move |_, ()| {
                        state.borrow_mut().watermark_ids.clear();
                        Ok(())
                    })
                    .map_err(lua_error)?,
                )
                .map_err(lua_error)?;
        }

        {
            let state = Rc::clone(&state);
            globals
                .set(
                    "graphics_on",
                    lua.create_function_mut(move |_, id: EntityId| {
                        state.borrow_mut().graphics_element_ids.push(id);
                        Ok(())
                    })
                    .map_err(lua_error)?,
                )
                .map_err(lua_error)?;
        }

        {
            let state = Rc::clone(&state);
            globals
                .set(
                    "graphics_off",
                    lua.create_function_mut(move |_, id: Option<EntityId>| {
                        let mut state = state.borrow_mut();
                        if let Some(id) = id {
                            state.graphics_element_ids.retain(|current| *current != id);
                        } else {
                            state.graphics_element_ids.clear();
                        }
                        Ok(())
                    })
                    .map_err(lua_error)?,
                )
                .map_err(lua_error)?;
        }

        lua.load(script).exec().map_err(lua_error)?;

        let mut state = state.borrow().clone();
        let anchor_time = state
            .added_items
            .iter()
            .map(|item| item.finish)
            .max()
            .unwrap_or(state.current_time);
        Ok(SchedulingResult {
            clear_items: request.mode == PlayoutBuildMode::Reset,
            remove_before: Some(request.start),
            anchor: Some(PlayoutAnchor {
                next_start: anchor_time,
                ..PlayoutAnchor::default()
            }),
            added_items: std::mem::take(&mut state.added_items),
            added_history: std::mem::take(&mut state.added_history),
            ..SchedulingResult::default()
        })
    }
}

#[derive(Debug, Clone)]
struct ScriptState {
    playout_id: EntityId,
    current_time: DateTime<Utc>,
    finish: DateTime<Utc>,
    reference_data: SchedulingReferenceData,
    watermark_ids: Vec<EntityId>,
    graphics_element_ids: Vec<EntityId>,
    added_items: Vec<nastv_domain::PlayoutItem>,
    added_history: Vec<nastv_domain::PlayoutHistory>,
    enumerators: std::collections::BTreeMap<String, MediaEnumerator>,
}

impl ScriptState {
    fn new(
        playout_id: EntityId,
        current_time: DateTime<Utc>,
        finish: DateTime<Utc>,
        reference_data: SchedulingReferenceData,
    ) -> Self {
        Self {
            playout_id,
            current_time,
            finish,
            reference_data,
            watermark_ids: Vec::new(),
            graphics_element_ids: Vec::new(),
            added_items: Vec::new(),
            added_history: Vec::new(),
            enumerators: std::collections::BTreeMap::new(),
        }
    }

    fn enumerator(&mut self, content: &str) -> &mut MediaEnumerator {
        self.enumerators
            .entry(content.to_owned())
            .or_insert_with(|| lookup_media(&self.reference_data, content))
    }

    fn current_duration(&mut self, content: &str) -> Option<Duration> {
        self.enumerator(content).current().map(|item| item.duration)
    }

    fn add_next(&mut self, content: &str, duration: Option<Duration>) {
        if self.current_time >= self.finish {
            return;
        }
        let Some(media_item) = self.enumerator(content).current().cloned() else {
            return;
        };
        let duration = duration.unwrap_or(media_item.duration);
        let item = playout_item(
            self.playout_id,
            &media_item,
            self.current_time,
            duration,
            0,
            FillerKind::None,
            None,
            false,
            self.watermark_ids.clone(),
            self.graphics_element_ids.clone(),
            Some(format!(r#"{{"scriptContent":"{content}"}}"#)),
        );
        self.added_history
            .push(history(self.playout_id, content, &item, None));
        self.current_time = item.finish;
        self.added_items.push(item);
        self.enumerator(content).advance();
    }
}

#[allow(clippy::needless_pass_by_value)]
fn lua_error(error: mlua::Error) -> SchedulingError {
    SchedulingError::Script(error.to_string())
}

#[cfg(test)]
mod tests {
    use chrono::Duration;
    use nastv_domain::Playout;

    use super::*;
    use crate::scheduling::SchedulingMediaItem;

    #[test]
    fn scripted_lua_schedules_count_and_duration_snapshot() {
        let start = DateTime::parse_from_rfc3339("2026-04-15T12:00:00Z")
            .expect("valid time")
            .with_timezone(&Utc);
        let mut reference_data = SchedulingReferenceData::default();
        reference_data.media_by_key.insert(
            "main".to_owned(),
            vec![
                SchedulingMediaItem::new(1, Duration::minutes(10)),
                SchedulingMediaItem::new(2, Duration::minutes(20)),
            ],
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
}
