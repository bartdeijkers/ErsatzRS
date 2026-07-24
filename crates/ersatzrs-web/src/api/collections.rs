use axum::{
    body::Bytes,
    extract::{Form, State},
    response::Redirect,
};
use serde::Deserialize;

use crate::{
    admin::server_fns::media_browser::{
        save_collection_with_state, save_multi_collection_with_state,
        save_smart_collection_with_state, CollectionForm, MultiCollectionForm,
        MultiCollectionItemForm,
    },
    host::state::AppState,
};

#[derive(Debug, Deserialize)]
pub struct CollectionSaveForm {
    id: i64,
    name: String,
}

#[derive(Debug, Deserialize)]
pub struct SmartCollectionSaveForm {
    id: i64,
    name: String,
    query: String,
}

#[derive(Debug, Deserialize)]
pub struct MultiCollectionDeleteForm {
    id: i64,
}

#[derive(Debug, Default, Deserialize)]
pub struct MultiCollectionSaveForm {
    id: i64,
    #[serde(default)]
    name: String,
    #[serde(default)]
    item_collection_id: Vec<String>,
    #[serde(default)]
    item_smart_collection_id: Vec<String>,
    #[serde(default)]
    item_name: Vec<String>,
    #[serde(default)]
    item_collection_type: Vec<String>,
    #[serde(default)]
    item_use_custom_playback_order: Vec<String>,
    #[serde(default)]
    item_schedule_as_group_index: Vec<String>,
    #[serde(default)]
    item_playback_order: Vec<String>,
}

pub async fn save_collection_form(
    State(state): State<AppState>,
    Form(input): Form<CollectionSaveForm>,
) -> Redirect {
    let id = input.id;
    let form = CollectionForm {
        id,
        name: input.name,
        query: String::new(),
    };

    match save_collection_with_state(&state, form).await {
        Ok(_) if id == 0 => Redirect::to("/media/collections"),
        Ok(saved_id) => Redirect::to(&format!("/media/collections/{saved_id}")),
        Err(error) => {
            tracing::error!(%error, collection_id = id, "failed to save collection from HTML form");
            let target = if id == 0 {
                "/media/collections/add?error=save".to_string()
            } else {
                format!("/media/collections/{id}/edit?error=save")
            };
            Redirect::to(&target)
        }
    }
}

pub async fn save_smart_collection_form(
    State(state): State<AppState>,
    Form(input): Form<SmartCollectionSaveForm>,
) -> Redirect {
    let id = input.id;
    let form = CollectionForm {
        id,
        name: input.name,
        query: input.query,
    };

    match save_smart_collection_with_state(&state, form).await {
        Ok(_) => Redirect::to("/media/smart-collections"),
        Err(error) => {
            tracing::error!(%error, smart_collection_id = id, "failed to save smart collection from HTML form");
            Redirect::to(&format!("/media/smart-collections/{id}/edit?error=save"))
        }
    }
}

pub async fn save_multi_collection_form(State(state): State<AppState>, body: Bytes) -> Redirect {
    let input = MultiCollectionSaveForm::from_urlencoded_bytes(&body);
    let id = input.id;
    let form = input.into_editor_form();

    match save_multi_collection_with_state(&state, form).await {
        Ok(_) => Redirect::to("/media/multi-collections"),
        Err(error) => {
            tracing::error!(%error, multi_collection_id = id, "failed to save multi collection from HTML form");
            let target = if id == 0 {
                "/media/multi-collections/add?error=save".to_string()
            } else {
                format!("/media/multi-collections/{id}/edit?error=save")
            };
            Redirect::to(&target)
        }
    }
}

pub async fn delete_multi_collection_form(
    State(state): State<AppState>,
    Form(input): Form<MultiCollectionDeleteForm>,
) -> Redirect {
    match state
        .application
        .delete_multi_collection(ersatzrs_app::commands::DeleteMultiCollection { id: input.id })
        .await
    {
        Ok(()) => Redirect::to("/media/multi-collections"),
        Err(error) => {
            tracing::error!(%error, multi_collection_id = input.id, "failed to delete multi collection from HTML form");
            Redirect::to("/media/multi-collections?error=delete")
        }
    }
}

impl MultiCollectionSaveForm {
    fn from_urlencoded_bytes(body: &[u8]) -> Self {
        let mut form = Self::default();
        let body = String::from_utf8_lossy(body);
        for pair in body.split('&').filter(|pair| !pair.is_empty()) {
            let (key, value) = pair.split_once('=').unwrap_or((pair, ""));
            let value = decode_form_value(value);
            match key {
                "id" => form.id = value.trim().parse().unwrap_or_default(),
                "name" => form.name = value,
                "item_collection_id" => form.item_collection_id.push(value),
                "item_smart_collection_id" => form.item_smart_collection_id.push(value),
                "item_name" => form.item_name.push(value),
                "item_collection_type" => form.item_collection_type.push(value),
                "item_use_custom_playback_order" => {
                    form.item_use_custom_playback_order.push(value);
                }
                "item_schedule_as_group_index" => form.item_schedule_as_group_index.push(value),
                "item_playback_order" => form.item_playback_order.push(value),
                _ => {}
            }
        }
        form
    }

    fn into_editor_form(self) -> MultiCollectionForm {
        let schedule_as_group_indexes = parse_index_set(&self.item_schedule_as_group_index);
        let item_len = self
            .item_collection_id
            .len()
            .max(self.item_smart_collection_id.len());
        let mut items = Vec::with_capacity(item_len);
        for pos in 0..item_len {
            let collection_id = parse_i64(self.item_collection_id.get(pos).cloned());
            let smart_collection_id = parse_i64(self.item_smart_collection_id.get(pos).cloned());
            if collection_id.is_none() && smart_collection_id.is_none() {
                continue;
            }
            items.push(MultiCollectionItemForm {
                collection_id,
                smart_collection_id,
                name: self.item_name.get(pos).cloned().unwrap_or_default(),
                collection_type: parse_i32(self.item_collection_type.get(pos).cloned())
                    .unwrap_or_default(),
                use_custom_playback_order: parse_bool(
                    self.item_use_custom_playback_order
                        .get(pos)
                        .map(String::as_str),
                ),
                schedule_as_group: schedule_as_group_indexes.contains(&pos),
                playback_order: parse_i32(self.item_playback_order.get(pos).cloned()).unwrap_or(1),
            });
        }

        MultiCollectionForm {
            id: self.id,
            name: self.name,
            items,
        }
    }
}

fn parse_index_set(values: &[String]) -> std::collections::HashSet<usize> {
    values
        .iter()
        .filter_map(|value| value.trim().parse::<usize>().ok())
        .collect()
}

fn parse_i32(value: Option<String>) -> Option<i32> {
    value.and_then(|value| value.trim().parse().ok())
}

fn parse_i64(value: Option<String>) -> Option<i64> {
    value
        .and_then(|value| value.trim().parse().ok())
        .filter(|value| *value != 0)
}

fn parse_bool(value: Option<&str>) -> bool {
    value.is_some_and(|value| matches!(value, "true" | "1" | "on"))
}

fn decode_form_value(value: &str) -> String {
    let value = value.replace('+', " ");
    urlencoding::decode(&value)
        .map(std::borrow::Cow::into_owned)
        .unwrap_or(value)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn multi_collection_save_parser_aligns_checkbox_indexes_with_items() {
        let input = MultiCollectionSaveForm::from_urlencoded_bytes(
            b"id=7&name=Prime&item_collection_id=11&item_smart_collection_id=&item_name=A&item_collection_type=0&item_use_custom_playback_order=true&item_playback_order=4&item_collection_id=&item_smart_collection_id=22&item_name=B&item_collection_type=5&item_use_custom_playback_order=false&item_schedule_as_group_index=1&item_playback_order=1",
        );

        let form = input.into_editor_form();

        assert_eq!(form.id, 7);
        assert_eq!(form.name, "Prime");
        assert_eq!(form.items.len(), 2);
        assert_eq!(form.items[0].collection_id, Some(11));
        assert_eq!(form.items[0].smart_collection_id, None);
        assert!(form.items[0].use_custom_playback_order);
        assert!(!form.items[0].schedule_as_group);
        assert_eq!(form.items[0].playback_order, 4);
        assert_eq!(form.items[1].collection_id, None);
        assert_eq!(form.items[1].smart_collection_id, Some(22));
        assert!(!form.items[1].use_custom_playback_order);
        assert!(form.items[1].schedule_as_group);
        assert_eq!(form.items[1].playback_order, 1);
    }
}
