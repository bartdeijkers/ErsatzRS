use axum::{
    extract::{Form, State},
    response::Redirect,
};
use serde::Deserialize;

use crate::{
    admin::server_fns::blocks::{
        collection_type, playback_order, save_block_with_pool, stop_scheduling, BlockItemForm,
        BlockWithItemsForm,
    },
    host::state::AppState,
};

#[derive(Debug, Default, Deserialize)]
pub struct BlockItemsSaveForm {
    block_id: i64,
    group_id: i64,
    #[serde(default)]
    name: String,
    #[serde(default)]
    duration_hours: String,
    #[serde(default)]
    duration_minutes: String,
    #[serde(default)]
    stop_scheduling: String,
    #[serde(default)]
    action: String,
    #[serde(default)]
    item_index: Vec<String>,
    #[serde(default)]
    item_id: Vec<String>,
    #[serde(default)]
    item_collection_type: Vec<String>,
    #[serde(default)]
    item_collection_id: Vec<String>,
    #[serde(default)]
    item_multi_collection_id: Vec<String>,
    #[serde(default)]
    item_smart_collection_id: Vec<String>,
    #[serde(default)]
    item_media_item_id: Vec<String>,
    #[serde(default)]
    item_search_title: Vec<String>,
    #[serde(default)]
    item_search_query: Vec<String>,
    #[serde(default)]
    item_collection_name: Vec<String>,
    #[serde(default)]
    item_playback_order: Vec<String>,
    #[serde(default)]
    item_include_in_program_guide_index: Vec<String>,
    #[serde(default)]
    item_disable_watermarks_index: Vec<String>,
    #[serde(default)]
    item_watermark_ids: Vec<String>,
    #[serde(default)]
    item_graphics_element_ids: Vec<String>,
    #[serde(default)]
    new_collection_id: String,
}

pub async fn save_block_items_form(
    State(state): State<AppState>,
    Form(input): Form<BlockItemsSaveForm>,
) -> Redirect {
    let block_id = input.block_id;
    let mut items = input.to_items();
    let (action, action_index) = parse_action(&input.action);

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
    let form = BlockWithItemsForm {
        id: block_id,
        group_id: input.group_id,
        name: input.name,
        minutes: parse_i32(Some(&input.duration_hours)).unwrap_or_default() * 60
            + parse_i32(Some(&input.duration_minutes)).unwrap_or_default(),
        stop_scheduling: parse_i32(Some(&input.stop_scheduling))
            .unwrap_or(stop_scheduling::AFTER_DURATION_END),
        items,
    };

    match save_block_with_pool(&state.pool, form).await {
        Ok(()) if action == "save" || action.is_empty() => Redirect::to("/blocks"),
        Ok(()) => Redirect::to(&format!("/blocks/{block_id}")),
        Err(error) => {
            tracing::error!(%error, block_id, "failed to save block from HTML form");
            Redirect::to(&format!("/blocks/{block_id}?error=save"))
        }
    }
}

impl BlockItemsSaveForm {
    fn to_items(&self) -> Vec<BlockItemForm> {
        let epg_indexes = parse_index_set(&self.item_include_in_program_guide_index);
        let disable_watermark_indexes = parse_index_set(&self.item_disable_watermarks_index);
        let mut items = Vec::with_capacity(self.item_index.len());
        for pos in 0..self.item_index.len() {
            let form_index = parse_i32(self.item_index.get(pos)).unwrap_or_default();
            items.push(BlockItemForm {
                id: parse_i64(self.item_id.get(pos)).unwrap_or_default(),
                index: form_index,
                collection_type: parse_i32(self.item_collection_type.get(pos))
                    .unwrap_or(collection_type::COLLECTION),
                collection_id: parse_i64(self.item_collection_id.get(pos)),
                multi_collection_id: parse_i64(self.item_multi_collection_id.get(pos)),
                smart_collection_id: parse_i64(self.item_smart_collection_id.get(pos)),
                media_item_id: parse_i64(self.item_media_item_id.get(pos)),
                search_title: parse_optional_string(self.item_search_title.get(pos)),
                search_query: parse_optional_string(self.item_search_query.get(pos)),
                playback_order: parse_i32(self.item_playback_order.get(pos))
                    .unwrap_or(playback_order::CHRONOLOGICAL),
                include_in_program_guide: epg_indexes.contains(&form_index),
                disable_watermarks: disable_watermark_indexes.contains(&form_index),
                collection_name: self
                    .item_collection_name
                    .get(pos)
                    .cloned()
                    .unwrap_or_default(),
                watermark_ids: parse_id_list(self.item_watermark_ids.get(pos)),
                graphics_element_ids: parse_id_list(self.item_graphics_element_ids.get(pos)),
            });
        }
        items
    }

    fn new_item(&self) -> BlockItemForm {
        BlockItemForm {
            collection_type: collection_type::COLLECTION,
            collection_id: parse_i64(Some(&self.new_collection_id)),
            playback_order: playback_order::CHRONOLOGICAL,
            include_in_program_guide: true,
            ..BlockItemForm::default()
        }
    }
}

fn normalize_indexes(items: &mut [BlockItemForm]) {
    for (index, item) in items.iter_mut().enumerate() {
        item.index = i32::try_from(index).unwrap_or(i32::MAX);
    }
}

fn parse_action(value: &str) -> (String, Option<usize>) {
    let Some((action, index)) = value.split_once(':') else {
        return (value.to_string(), None);
    };
    (action.to_string(), index.parse::<usize>().ok())
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
        .map(|value| value.trim())
        .filter(|value| !value.is_empty())
        .map(ToOwned::to_owned)
}

fn parse_id_list(value: Option<&String>) -> Vec<i64> {
    value
        .map(String::as_str)
        .unwrap_or_default()
        .split(',')
        .filter_map(|value| value.trim().parse::<i64>().ok())
        .filter(|value| *value != 0)
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn block_items_parser_aligns_indexed_checkboxes_and_ids() {
        let form = BlockItemsSaveForm {
            block_id: 7,
            group_id: 2,
            name: "Prime".to_string(),
            item_index: vec!["0".to_string(), "2".to_string()],
            item_id: vec!["10".to_string(), "11".to_string()],
            item_collection_type: vec!["0".to_string(), "4".to_string()],
            item_collection_id: vec!["3".to_string(), String::new()],
            item_multi_collection_id: vec![String::new(), "9".to_string()],
            item_playback_order: vec!["1".to_string(), "3".to_string()],
            item_include_in_program_guide_index: vec!["2".to_string()],
            item_disable_watermarks_index: vec!["0".to_string()],
            item_watermark_ids: vec!["1,2".to_string(), String::new()],
            item_graphics_element_ids: vec![String::new(), "4".to_string()],
            ..BlockItemsSaveForm::default()
        };

        let items = form.to_items();

        assert_eq!(items.len(), 2);
        assert_eq!(items[0].collection_id, Some(3));
        assert!(!items[0].include_in_program_guide);
        assert!(items[0].disable_watermarks);
        assert_eq!(items[0].watermark_ids, vec![1, 2]);
        assert_eq!(items[1].multi_collection_id, Some(9));
        assert!(items[1].include_in_program_guide);
        assert!(!items[1].disable_watermarks);
        assert_eq!(items[1].graphics_element_ids, vec![4]);
    }

    #[test]
    fn block_items_add_action_builds_default_collection_item() {
        let form = BlockItemsSaveForm {
            new_collection_id: "42".to_string(),
            ..BlockItemsSaveForm::default()
        };

        let item = form.new_item();

        assert_eq!(item.collection_type, collection_type::COLLECTION);
        assert_eq!(item.collection_id, Some(42));
        assert!(item.include_in_program_guide);
    }
}
