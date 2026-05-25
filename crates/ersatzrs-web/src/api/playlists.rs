use axum::{
    extract::{Form, State},
    response::Redirect,
};
use serde::Deserialize;

use crate::{
    admin::server_fns::media_browser::{
        create_playlist_group_with_pool, create_playlist_with_pool,
        delete_playlist_group_with_pool, delete_playlist_with_pool, save_playlist_with_pool,
        PlaylistEditorForm, PlaylistItemForm,
    },
    host::state::AppState,
};

#[derive(Debug, Deserialize)]
pub struct PlaylistGroupCreateForm {
    name: String,
}

#[derive(Debug, Deserialize)]
pub struct PlaylistCreateForm {
    playlist_group_id: String,
    name: String,
}

#[derive(Debug, Deserialize)]
pub struct PlaylistDeleteForm {
    id: i64,
}

#[derive(Debug, Default, Deserialize)]
pub struct PlaylistSaveForm {
    id: i64,
    #[serde(default)]
    name: String,
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
    item_playback_order: Vec<String>,
    #[serde(default)]
    item_count: Vec<String>,
    #[serde(default)]
    item_play_all_index: Vec<String>,
    #[serde(default)]
    item_include_in_program_guide_index: Vec<String>,
}

pub async fn create_playlist_group_form(
    State(state): State<AppState>,
    Form(input): Form<PlaylistGroupCreateForm>,
) -> Redirect {
    match create_playlist_group_with_pool(&state.pool, input.name).await {
        Ok(_) => Redirect::to("/media/playlists"),
        Err(error) => {
            tracing::error!(%error, "failed to create playlist group from HTML form");
            Redirect::to("/media/playlists?error=add-group")
        }
    }
}

pub async fn create_playlist_form(
    State(state): State<AppState>,
    Form(input): Form<PlaylistCreateForm>,
) -> Redirect {
    let playlist_group_id = parse_i64(Some(input.playlist_group_id)).unwrap_or_default();
    match create_playlist_with_pool(&state.pool, playlist_group_id, input.name).await {
        Ok(_) => Redirect::to("/media/playlists"),
        Err(error) => {
            tracing::error!(%error, playlist_group_id, "failed to create playlist from HTML form");
            Redirect::to("/media/playlists?error=add-playlist")
        }
    }
}

pub async fn delete_playlist_group_form(
    State(state): State<AppState>,
    Form(input): Form<PlaylistDeleteForm>,
) -> Redirect {
    match delete_playlist_group_with_pool(&state.pool, input.id).await {
        Ok(()) => Redirect::to("/media/playlists"),
        Err(error) => {
            tracing::error!(%error, playlist_group_id = input.id, "failed to delete playlist group from HTML form");
            Redirect::to("/media/playlists?error=delete-group")
        }
    }
}

pub async fn delete_playlist_form(
    State(state): State<AppState>,
    Form(input): Form<PlaylistDeleteForm>,
) -> Redirect {
    match delete_playlist_with_pool(&state.pool, input.id).await {
        Ok(()) => Redirect::to("/media/playlists"),
        Err(error) => {
            tracing::error!(%error, playlist_id = input.id, "failed to delete playlist from HTML form");
            Redirect::to("/media/playlists?error=delete-playlist")
        }
    }
}

pub async fn save_playlist_form(
    State(state): State<AppState>,
    Form(input): Form<PlaylistSaveForm>,
) -> Redirect {
    let id = input.id;
    let form = input.into_editor_form();
    match save_playlist_with_pool(&state.pool, form).await {
        Ok(()) => Redirect::to("/media/playlists"),
        Err(error) => {
            tracing::error!(%error, playlist_id = id, "failed to save playlist from HTML form");
            Redirect::to(&format!("/media/playlists/{id}?error=save"))
        }
    }
}

impl PlaylistSaveForm {
    fn into_editor_form(self) -> PlaylistEditorForm {
        let play_all_indexes = parse_index_set(&self.item_play_all_index);
        let epg_indexes = parse_index_set(&self.item_include_in_program_guide_index);
        let mut items = Vec::with_capacity(self.item_index.len());
        for pos in 0..self.item_index.len() {
            let form_index = parse_i32(self.item_index.get(pos).cloned()).unwrap_or_default();
            items.push(PlaylistItemForm {
                id: parse_i64(self.item_id.get(pos).cloned()).unwrap_or_default(),
                index: i32::try_from(pos).unwrap_or(form_index),
                collection_type: parse_i32(self.item_collection_type.get(pos).cloned())
                    .unwrap_or_default(),
                collection_id: parse_i64(self.item_collection_id.get(pos).cloned()),
                multi_collection_id: parse_i64(self.item_multi_collection_id.get(pos).cloned()),
                smart_collection_id: parse_i64(self.item_smart_collection_id.get(pos).cloned()),
                media_item_id: parse_i64(self.item_media_item_id.get(pos).cloned()),
                item_name: String::new(),
                playback_order: parse_i32(self.item_playback_order.get(pos).cloned()).unwrap_or(1),
                count: parse_i32(self.item_count.get(pos).cloned()),
                play_all: play_all_indexes.contains(&form_index),
                include_in_program_guide: epg_indexes.contains(&form_index),
            });
        }

        PlaylistEditorForm {
            id: self.id,
            playlist_group_id: 0,
            name: self.name,
            is_system: false,
            items,
        }
    }
}

fn parse_index_set(values: &[String]) -> std::collections::HashSet<i32> {
    values
        .iter()
        .filter_map(|value| value.trim().parse::<i32>().ok())
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn playlist_save_parser_aligns_checkbox_indexes_with_items() {
        let form = PlaylistSaveForm {
            id: 7,
            name: "Saturday".to_string(),
            item_index: vec!["0".to_string(), "1".to_string()],
            item_id: vec!["11".to_string(), "12".to_string()],
            item_collection_type: vec!["0".to_string(), "10".to_string()],
            item_collection_id: vec!["5".to_string(), String::new()],
            item_media_item_id: vec![String::new(), "42".to_string()],
            item_playback_order: vec!["1".to_string(), "2".to_string()],
            item_count: vec!["3".to_string(), String::new()],
            item_play_all_index: vec!["1".to_string()],
            item_include_in_program_guide_index: vec!["0".to_string(), "1".to_string()],
            ..PlaylistSaveForm::default()
        };

        let parsed = form.into_editor_form();

        assert_eq!(parsed.id, 7);
        assert_eq!(parsed.items.len(), 2);
        assert_eq!(parsed.items[0].collection_id, Some(5));
        assert_eq!(parsed.items[0].count, Some(3));
        assert!(!parsed.items[0].play_all);
        assert!(parsed.items[0].include_in_program_guide);
        assert_eq!(parsed.items[1].media_item_id, Some(42));
        assert_eq!(parsed.items[1].count, None);
        assert!(parsed.items[1].play_all);
        assert!(parsed.items[1].include_in_program_guide);
    }
}
