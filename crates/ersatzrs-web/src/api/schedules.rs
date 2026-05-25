use axum::{
    extract::{Form, State},
    response::Redirect,
};
use serde::Deserialize;

use crate::{
    admin::server_fns::schedules::{
        copy_schedule_with_pool, delete_schedule_with_pool, replace_schedule_items_with_state,
        save_schedule_with_state, ScheduleForm, ScheduleItemForm, COLLECTION_TYPE_COLLECTION,
        FILL_WITH_GROUP_MODE_NONE, GUIDE_MODE_NORMAL, MARATHON_GROUP_BY_NONE, MULTIPLE_MODE_COUNT,
        PLAYBACK_ORDER_SHUFFLE, PLAYOUT_MODE_ONE, START_TYPE_DYNAMIC, TAIL_MODE_NONE,
    },
    host::state::AppState,
};

#[allow(clippy::struct_excessive_bools)]
#[derive(Debug, Deserialize)]
pub struct ScheduleSaveForm {
    id: i64,
    name: String,
    #[serde(default)]
    keep_multi_part_episodes_together: bool,
    #[serde(default)]
    treat_collections_as_shows: bool,
    #[serde(default)]
    shuffle_schedule_items: bool,
    #[serde(default)]
    random_start_point: bool,
    fixed_start_time_behavior: i32,
}

#[derive(Debug, Deserialize)]
pub struct ScheduleDeleteForm {
    id: i64,
}

#[derive(Debug, Deserialize)]
pub struct ScheduleCopyForm {
    id: i64,
    name: String,
}

#[derive(Debug, Default, Deserialize)]
pub struct ScheduleItemsSaveForm {
    schedule_id: i64,
    #[serde(default)]
    action: String,
    #[serde(default)]
    action_index: String,
    #[serde(default)]
    item_index: Vec<String>,
    #[serde(default)]
    item_id: Vec<String>,
    #[serde(default)]
    item_start_type: Vec<String>,
    #[serde(default)]
    item_start_time: Vec<String>,
    #[serde(default)]
    item_fixed_start_time_behavior: Vec<String>,
    #[serde(default)]
    item_collection_type: Vec<String>,
    #[serde(default)]
    item_collection_id: Vec<String>,
    #[serde(default)]
    item_media_item_id: Vec<String>,
    #[serde(default)]
    item_multi_collection_id: Vec<String>,
    #[serde(default)]
    item_smart_collection_id: Vec<String>,
    #[serde(default)]
    item_rerun_collection_id: Vec<String>,
    #[serde(default)]
    item_playlist_id: Vec<String>,
    #[serde(default)]
    item_search_title: Vec<String>,
    #[serde(default)]
    item_search_query: Vec<String>,
    #[serde(default)]
    item_collection_name: Vec<String>,
    #[serde(default)]
    item_playback_order: Vec<String>,
    #[serde(default)]
    item_marathon_group_by: Vec<String>,
    #[serde(default)]
    item_marathon_shuffle_groups_index: Vec<String>,
    #[serde(default)]
    item_marathon_shuffle_items_index: Vec<String>,
    #[serde(default)]
    item_marathon_batch_size: Vec<String>,
    #[serde(default)]
    item_fill_with_group_mode: Vec<String>,
    #[serde(default)]
    item_playout_mode: Vec<String>,
    #[serde(default)]
    item_multiple_mode: Vec<String>,
    #[serde(default)]
    item_multiple_count: Vec<String>,
    #[serde(default)]
    item_duration_hours: Vec<String>,
    #[serde(default)]
    item_duration_minutes: Vec<String>,
    #[serde(default)]
    item_tail_mode: Vec<String>,
    #[serde(default)]
    item_discard_to_fill_attempts: Vec<String>,
    #[serde(default)]
    item_custom_title: Vec<String>,
    #[serde(default)]
    item_guide_mode: Vec<String>,
    #[serde(default)]
    item_pre_roll_filler_id: Vec<String>,
    #[serde(default)]
    item_mid_roll_filler_id: Vec<String>,
    #[serde(default)]
    item_post_roll_filler_id: Vec<String>,
    #[serde(default)]
    item_tail_filler_id: Vec<String>,
    #[serde(default)]
    item_fallback_filler_id: Vec<String>,
    #[serde(default)]
    item_watermark_ids: Vec<String>,
    #[serde(default)]
    item_graphics_element_ids: Vec<String>,
    #[serde(default)]
    item_preferred_audio_language_code: Vec<String>,
    #[serde(default)]
    item_preferred_audio_title: Vec<String>,
    #[serde(default)]
    item_preferred_subtitle_language_code: Vec<String>,
    #[serde(default)]
    item_subtitle_mode: Vec<String>,
    #[serde(default)]
    new_collection_type: String,
    #[serde(default)]
    new_collection_id: String,
    #[serde(default)]
    new_playback_order: String,
    #[serde(default)]
    new_playout_mode: String,
}

pub async fn save_schedule_form(
    State(state): State<AppState>,
    Form(input): Form<ScheduleSaveForm>,
) -> Redirect {
    let id = input.id;
    let form = ScheduleForm {
        id,
        name: input.name,
        keep_multi_part_episodes_together: input.keep_multi_part_episodes_together,
        treat_collections_as_shows: input.treat_collections_as_shows,
        shuffle_schedule_items: input.shuffle_schedule_items,
        random_start_point: input.random_start_point,
        fixed_start_time_behavior: input.fixed_start_time_behavior,
    };

    match save_schedule_with_state(&state, form).await {
        Ok(saved_id) if id == 0 => Redirect::to(&format!("/schedules/{saved_id}/items")),
        Ok(_) => Redirect::to("/schedules"),
        Err(error) => {
            tracing::error!(%error, schedule_id = id, "failed to save schedule from HTML form");
            let target = if id == 0 {
                "/schedules/add?error=save".to_string()
            } else {
                format!("/schedules/{id}?error=save")
            };
            Redirect::to(&target)
        }
    }
}

pub async fn delete_schedule_form(
    State(state): State<AppState>,
    Form(input): Form<ScheduleDeleteForm>,
) -> Redirect {
    match delete_schedule_with_pool(&state.pool, input.id).await {
        Ok(()) => Redirect::to("/schedules"),
        Err(error) => {
            tracing::error!(%error, schedule_id = input.id, "failed to delete schedule from HTML form");
            Redirect::to("/schedules?error=delete")
        }
    }
}

pub async fn copy_schedule_form(
    State(state): State<AppState>,
    Form(input): Form<ScheduleCopyForm>,
) -> Redirect {
    match copy_schedule_with_pool(&state.pool, input.id, &input.name).await {
        Ok(id) => Redirect::to(&format!("/schedules/{id}/items")),
        Err(error) => {
            tracing::error!(%error, schedule_id = input.id, "failed to copy schedule from HTML form");
            Redirect::to("/schedules?error=copy")
        }
    }
}

pub async fn save_schedule_items_form(
    State(state): State<AppState>,
    Form(input): Form<ScheduleItemsSaveForm>,
) -> Redirect {
    let schedule_id = input.schedule_id;
    let mut items = input.to_items();
    let (action, embedded_index) = parse_action(&input.action);
    let action_index = embedded_index.or_else(|| input_action_index(&items, &input.action_index));

    match action.as_str() {
        "copy" => {
            if let Some(index) =
                action_index.and_then(|index| items.get(index).cloned().map(|item| (index, item)))
            {
                let (index, mut copy) = index;
                copy.id = 0;
                items.insert(index + 1, copy);
            }
        }
        "move-up" => {
            if let Some(index) = action_index.filter(|index| *index > 0) {
                items.swap(index, index - 1);
            }
        }
        "move-down" => {
            if let Some(index) = action_index.filter(|index| *index + 1 < items.len()) {
                items.swap(index, index + 1);
            }
        }
        "delete" => {
            if let Some(index) = action_index.filter(|index| *index < items.len()) {
                items.remove(index);
            }
        }
        "add" => items.push(input.new_item()),
        _ => {}
    }

    normalize_indexes(&mut items);
    match replace_schedule_items_with_state(&state, schedule_id, items).await {
        Ok(()) => Redirect::to(&format!("/schedules/{schedule_id}/items")),
        Err(error) => {
            tracing::error!(%error, schedule_id, "failed to save schedule items from HTML form");
            Redirect::to(&format!("/schedules/{schedule_id}/items?error=save-items"))
        }
    }
}

impl ScheduleItemsSaveForm {
    fn to_items(&self) -> Vec<ScheduleItemForm> {
        let marathon_shuffle_groups = parse_index_set(&self.item_marathon_shuffle_groups_index);
        let marathon_shuffle_items = parse_index_set(&self.item_marathon_shuffle_items_index);
        let mut items = Vec::with_capacity(self.item_index.len());
        for pos in 0..self.item_index.len() {
            let form_index = parse_i32(self.item_index.get(pos)).unwrap_or_default();
            items.push(ScheduleItemForm {
                id: parse_i64(self.item_id.get(pos)).unwrap_or_default(),
                index: form_index,
                start_type: parse_i32(self.item_start_type.get(pos)).unwrap_or(START_TYPE_DYNAMIC),
                start_time: parse_optional_string(self.item_start_time.get(pos)),
                fixed_start_time_behavior: parse_i32(self.item_fixed_start_time_behavior.get(pos)),
                collection_type: parse_i32(self.item_collection_type.get(pos))
                    .unwrap_or(COLLECTION_TYPE_COLLECTION),
                collection_id: parse_i64(self.item_collection_id.get(pos)),
                media_item_id: parse_i64(self.item_media_item_id.get(pos)),
                multi_collection_id: parse_i64(self.item_multi_collection_id.get(pos)),
                smart_collection_id: parse_i64(self.item_smart_collection_id.get(pos)),
                rerun_collection_id: parse_i64(self.item_rerun_collection_id.get(pos)),
                playlist_id: parse_i64(self.item_playlist_id.get(pos)),
                search_title: parse_optional_string(self.item_search_title.get(pos)),
                search_query: parse_optional_string(self.item_search_query.get(pos)),
                collection_name: self
                    .item_collection_name
                    .get(pos)
                    .cloned()
                    .unwrap_or_default(),
                playback_order: parse_i32(self.item_playback_order.get(pos))
                    .unwrap_or(PLAYBACK_ORDER_SHUFFLE),
                marathon_group_by: parse_i32(self.item_marathon_group_by.get(pos))
                    .unwrap_or(MARATHON_GROUP_BY_NONE),
                marathon_shuffle_groups: marathon_shuffle_groups.contains(&form_index),
                marathon_shuffle_items: marathon_shuffle_items.contains(&form_index),
                marathon_batch_size: parse_i32(self.item_marathon_batch_size.get(pos)),
                fill_with_group_mode: parse_i32(self.item_fill_with_group_mode.get(pos))
                    .unwrap_or(FILL_WITH_GROUP_MODE_NONE),
                playout_mode: parse_i32(self.item_playout_mode.get(pos))
                    .unwrap_or(PLAYOUT_MODE_ONE),
                multiple_mode: parse_i32(self.item_multiple_mode.get(pos))
                    .unwrap_or(MULTIPLE_MODE_COUNT),
                multiple_count: self
                    .item_multiple_count
                    .get(pos)
                    .cloned()
                    .filter(|value| !value.trim().is_empty())
                    .unwrap_or_else(|| "1".to_string()),
                duration_hours: parse_i32(self.item_duration_hours.get(pos)).unwrap_or(1),
                duration_minutes: parse_i32(self.item_duration_minutes.get(pos)).unwrap_or(0),
                tail_mode: parse_i32(self.item_tail_mode.get(pos)).unwrap_or(TAIL_MODE_NONE),
                discard_to_fill_attempts: parse_i32(self.item_discard_to_fill_attempts.get(pos)),
                custom_title: self.item_custom_title.get(pos).cloned().unwrap_or_default(),
                guide_mode: parse_i32(self.item_guide_mode.get(pos)).unwrap_or(GUIDE_MODE_NORMAL),
                pre_roll_filler_id: parse_i64(self.item_pre_roll_filler_id.get(pos)),
                mid_roll_filler_id: parse_i64(self.item_mid_roll_filler_id.get(pos)),
                post_roll_filler_id: parse_i64(self.item_post_roll_filler_id.get(pos)),
                tail_filler_id: parse_i64(self.item_tail_filler_id.get(pos)),
                fallback_filler_id: parse_i64(self.item_fallback_filler_id.get(pos)),
                watermark_ids: parse_id_list(self.item_watermark_ids.get(pos)),
                graphics_element_ids: parse_id_list(self.item_graphics_element_ids.get(pos)),
                preferred_audio_language_code: parse_optional_string(
                    self.item_preferred_audio_language_code.get(pos),
                ),
                preferred_audio_title: self
                    .item_preferred_audio_title
                    .get(pos)
                    .cloned()
                    .unwrap_or_default(),
                preferred_subtitle_language_code: parse_optional_string(
                    self.item_preferred_subtitle_language_code.get(pos),
                ),
                subtitle_mode: parse_i32(self.item_subtitle_mode.get(pos)),
            });
        }
        items
    }

    fn new_item(&self) -> ScheduleItemForm {
        ScheduleItemForm {
            collection_type: parse_i32(Some(&self.new_collection_type))
                .unwrap_or(COLLECTION_TYPE_COLLECTION),
            collection_id: parse_i64(Some(&self.new_collection_id)),
            playback_order: parse_i32(Some(&self.new_playback_order))
                .unwrap_or(PLAYBACK_ORDER_SHUFFLE),
            playout_mode: parse_i32(Some(&self.new_playout_mode)).unwrap_or(PLAYOUT_MODE_ONE),
            ..ScheduleItemForm::default()
        }
    }
}

fn input_action_index(items: &[ScheduleItemForm], value: &str) -> Option<usize> {
    let requested = value.trim().parse::<i32>().ok()?;
    items
        .iter()
        .position(|item| item.index == requested)
        .or_else(|| {
            usize::try_from(requested)
                .ok()
                .filter(|index| *index < items.len())
        })
}

fn parse_action(value: &str) -> (String, Option<usize>) {
    let Some((action, index)) = value.split_once(':') else {
        return (value.to_string(), None);
    };
    (action.to_string(), index.parse::<usize>().ok())
}

fn normalize_indexes(items: &mut [ScheduleItemForm]) {
    for (index, item) in items.iter_mut().enumerate() {
        item.index = i32::try_from(index).unwrap_or(i32::MAX);
    }
}

fn parse_index_set(values: &[String]) -> std::collections::HashSet<i32> {
    values
        .iter()
        .filter_map(|value| value.trim().parse::<i32>().ok())
        .collect()
}

fn parse_i32(value: Option<&String>) -> Option<i32> {
    value.and_then(|value| value.trim().parse().ok())
}

fn parse_i64(value: Option<&String>) -> Option<i64> {
    value
        .and_then(|value| value.trim().parse().ok())
        .filter(|value| *value != 0)
}

fn parse_optional_string(value: Option<&String>) -> Option<String> {
    value
        .map(|value| value.trim().to_string())
        .filter(|value| !value.is_empty())
}

fn parse_id_list(value: Option<&String>) -> Vec<i64> {
    value
        .map(String::as_str)
        .unwrap_or_default()
        .split(',')
        .filter_map(|value| value.trim().parse::<i64>().ok())
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn schedule_items_parser_aligns_indexed_fields_and_checkbox_indexes() {
        let form = ScheduleItemsSaveForm {
            schedule_id: 9,
            item_index: vec!["0".to_string(), "1".to_string()],
            item_id: vec!["11".to_string(), "12".to_string()],
            item_start_type: vec!["1".to_string(), "2".to_string()],
            item_start_time: vec![String::new(), "08:30:00".to_string()],
            item_fixed_start_time_behavior: vec![String::new(), "1".to_string()],
            item_collection_type: vec!["0".to_string(), "9".to_string()],
            item_collection_id: vec!["5".to_string(), String::new()],
            item_search_title: vec![String::new(), "Morning".to_string()],
            item_search_query: vec![String::new(), "library_id:1".to_string()],
            item_collection_name: vec!["Movies".to_string(), "Morning".to_string()],
            item_playback_order: vec!["3".to_string(), "1".to_string()],
            item_marathon_group_by: vec!["0".to_string(), "0".to_string()],
            item_marathon_shuffle_groups_index: vec!["1".to_string()],
            item_marathon_shuffle_items_index: vec!["0".to_string()],
            item_fill_with_group_mode: vec!["0".to_string(), "0".to_string()],
            item_playout_mode: vec!["2".to_string(), "3".to_string()],
            item_multiple_mode: vec!["0".to_string(), "0".to_string()],
            item_multiple_count: vec!["1".to_string(), "count + 1".to_string()],
            item_duration_hours: vec!["1".to_string(), "2".to_string()],
            item_duration_minutes: vec!["0".to_string(), "15".to_string()],
            item_tail_mode: vec!["0".to_string(), "0".to_string()],
            item_discard_to_fill_attempts: vec!["0".to_string(), "3".to_string()],
            item_custom_title: vec![String::new(), "Custom".to_string()],
            item_guide_mode: vec!["0".to_string(), "1".to_string()],
            item_watermark_ids: vec!["1,2".to_string(), String::new()],
            item_graphics_element_ids: vec![String::new(), "7".to_string()],
            item_subtitle_mode: vec![String::new(), "3".to_string()],
            ..ScheduleItemsSaveForm::default()
        };

        let parsed = form.to_items();

        assert_eq!(parsed.len(), 2);
        assert_eq!(parsed[0].collection_id, Some(5));
        assert_eq!(parsed[0].watermark_ids, vec![1, 2]);
        assert!(parsed[0].marathon_shuffle_items);
        assert!(!parsed[0].marathon_shuffle_groups);
        assert_eq!(parsed[1].start_time.as_deref(), Some("08:30:00"));
        assert_eq!(parsed[1].search_query.as_deref(), Some("library_id:1"));
        assert_eq!(parsed[1].multiple_count, "count + 1");
        assert_eq!(parsed[1].graphics_element_ids, vec![7]);
        assert_eq!(parsed[1].subtitle_mode, Some(3));
        assert!(parsed[1].marathon_shuffle_groups);
    }

    #[test]
    fn schedule_items_parser_builds_add_item_from_new_fields() {
        let form = ScheduleItemsSaveForm {
            new_collection_type: "0".to_string(),
            new_collection_id: "44".to_string(),
            new_playback_order: "4".to_string(),
            new_playout_mode: "3".to_string(),
            ..ScheduleItemsSaveForm::default()
        };

        let item = form.new_item();

        assert_eq!(item.collection_type, COLLECTION_TYPE_COLLECTION);
        assert_eq!(item.collection_id, Some(44));
        assert_eq!(item.playback_order, 4);
        assert_eq!(item.playout_mode, 3);
    }

    #[test]
    fn schedule_items_action_supports_embedded_row_index() {
        assert_eq!(parse_action("copy:2"), ("copy".to_string(), Some(2)));
        assert_eq!(parse_action("save"), ("save".to_string(), None));
    }
}
