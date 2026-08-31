use axum::{
    body::Bytes,
    extract::{Form, State},
    response::Redirect,
};
use serde::Deserialize;

use crate::{
    admin::server_fns::media_browser::{
        playlist_editor_into_admin, PlaylistEditorForm, PlaylistItemForm,
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
    item_move: String,
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
    item_preferred_audio_language_code: Vec<String>,
    #[serde(default)]
    item_preferred_audio_title: Vec<String>,
    #[serde(default)]
    item_preferred_subtitle_language_code: Vec<String>,
    #[serde(default)]
    item_subtitle_mode: Vec<String>,
    #[serde(default)]
    item_media_version_id: Vec<String>,
    #[serde(default)]
    item_video_stream_index: Vec<String>,
    #[serde(default)]
    item_audio_stream_index: Vec<String>,
    #[serde(default)]
    item_subtitle_stream_index: Vec<String>,
    #[serde(default)]
    item_play_all_index: Vec<String>,
    #[serde(default)]
    item_include_in_program_guide_index: Vec<String>,
}

pub async fn create_playlist_group_form(
    State(state): State<AppState>,
    Form(input): Form<PlaylistGroupCreateForm>,
) -> Redirect {
    match state
        .application
        .create_playlist_group(ersatzrs_app::commands::CreatePlaylistGroup { name: input.name })
        .await
    {
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
    match state
        .application
        .create_playlist(ersatzrs_app::commands::CreatePlaylist {
            playlist_group_id,
            name: input.name,
        })
        .await
    {
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
    match state
        .application
        .delete_playlist_group(ersatzrs_app::commands::DeletePlaylistGroup { id: input.id })
        .await
    {
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
    match state
        .application
        .delete_playlist(ersatzrs_app::commands::DeletePlaylist { id: input.id })
        .await
    {
        Ok(()) => Redirect::to("/media/playlists"),
        Err(error) => {
            tracing::error!(%error, playlist_id = input.id, "failed to delete playlist from HTML form");
            Redirect::to("/media/playlists?error=delete-playlist")
        }
    }
}

pub async fn save_playlist_form(State(state): State<AppState>, body: Bytes) -> Redirect {
    let input = PlaylistSaveForm::from_urlencoded_bytes(&body);
    let id = input.id;
    let item_move = input.item_move.clone();
    let mut form = input.into_editor_form();
    if let Err(error) = apply_playlist_item_move(&mut form.items, &item_move) {
        tracing::warn!(%error, playlist_id = id, "rejected playlist item move from HTML form");
        return Redirect::to(&format!("/media/playlists/{id}?error=save"));
    }
    match state
        .application
        .save_playlist(ersatzrs_app::commands::SavePlaylist {
            form: playlist_editor_into_admin(form),
        })
        .await
    {
        Ok(()) => Redirect::to("/media/playlists"),
        Err(error) => {
            tracing::error!(%error, playlist_id = id, "failed to save playlist from HTML form");
            Redirect::to(&format!("/media/playlists/{id}?error=save"))
        }
    }
}

impl PlaylistSaveForm {
    fn from_urlencoded_bytes(body: &[u8]) -> Self {
        let mut form = Self::default();
        let body = String::from_utf8_lossy(body);
        for pair in body.split('&').filter(|pair| !pair.is_empty()) {
            let (key, value) = pair.split_once('=').unwrap_or((pair, ""));
            let value = decode_form_value(value);
            match key {
                "id" => form.id = value.trim().parse().unwrap_or_default(),
                "name" => form.name = value,
                "item_move" => form.item_move = value,
                "item_index" => form.item_index.push(value),
                "item_id" => form.item_id.push(value),
                "item_collection_type" => form.item_collection_type.push(value),
                "item_collection_id" => form.item_collection_id.push(value),
                "item_multi_collection_id" => form.item_multi_collection_id.push(value),
                "item_smart_collection_id" => form.item_smart_collection_id.push(value),
                "item_media_item_id" => form.item_media_item_id.push(value),
                "item_playback_order" => form.item_playback_order.push(value),
                "item_count" => form.item_count.push(value),
                "item_preferred_audio_language_code" => {
                    form.item_preferred_audio_language_code.push(value);
                }
                "item_preferred_audio_title" => form.item_preferred_audio_title.push(value),
                "item_preferred_subtitle_language_code" => {
                    form.item_preferred_subtitle_language_code.push(value);
                }
                "item_subtitle_mode" => form.item_subtitle_mode.push(value),
                "item_media_version_id" => form.item_media_version_id.push(value),
                "item_video_stream_index" => form.item_video_stream_index.push(value),
                "item_audio_stream_index" => form.item_audio_stream_index.push(value),
                "item_subtitle_stream_index" => form.item_subtitle_stream_index.push(value),
                "item_play_all_index" => form.item_play_all_index.push(value),
                "item_include_in_program_guide_index" => {
                    form.item_include_in_program_guide_index.push(value);
                }
                _ => {}
            }
        }
        form
    }

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
                preferred_audio_language_code: parse_string(
                    self.item_preferred_audio_language_code.get(pos).cloned(),
                ),
                preferred_audio_title: parse_string(
                    self.item_preferred_audio_title.get(pos).cloned(),
                ),
                preferred_subtitle_language_code: parse_string(
                    self.item_preferred_subtitle_language_code.get(pos).cloned(),
                ),
                subtitle_mode: parse_i32(self.item_subtitle_mode.get(pos).cloned()),
                pinned_stream_selection: parse_i64(self.item_media_version_id.get(pos).cloned())
                    .zip(parse_i32(self.item_video_stream_index.get(pos).cloned()))
                    .map(|(media_version_id, video_stream_index)| {
                        crate::admin::server_fns::media_browser::PinnedStreamSelectionForm {
                            media_version_id,
                            video_stream_index,
                            audio_stream_index: parse_i32(
                                self.item_audio_stream_index.get(pos).cloned(),
                            ),
                            subtitle_stream_index: parse_i32(
                                self.item_subtitle_stream_index.get(pos).cloned(),
                            ),
                        }
                    }),
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

fn apply_playlist_item_move(
    items: &mut [PlaylistItemForm],
    requested_move: &str,
) -> Result<(), &'static str> {
    if requested_move.is_empty() {
        return Ok(());
    }
    let (direction, index) = requested_move
        .split_once(':')
        .ok_or("playlist item move is malformed")?;
    let index = index
        .parse::<usize>()
        .map_err(|_| "playlist item move index is invalid")?;
    let target = match direction {
        "up" if index > 0 => index - 1,
        "down" if index + 1 < items.len() => index + 1,
        "up" | "down" => return Err("playlist item move is out of bounds"),
        _ => return Err("playlist item move direction is invalid"),
    };
    items.swap(index, target);
    for (index, item) in items.iter_mut().enumerate() {
        item.index = i32::try_from(index).unwrap_or(i32::MAX);
    }
    Ok(())
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

fn parse_string(value: Option<String>) -> Option<String> {
    value
        .map(|value| value.trim().to_owned())
        .filter(|value| !value.is_empty())
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
            item_preferred_audio_language_code: vec![String::new(), "eng".to_string()],
            item_preferred_audio_title: vec![String::new(), "Director Commentary".to_string()],
            item_preferred_subtitle_language_code: vec![String::new(), "eng".to_string()],
            item_subtitle_mode: vec![String::new(), "3".to_string()],
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
        assert_eq!(
            parsed.items[1].preferred_audio_title.as_deref(),
            Some("Director Commentary")
        );
        assert_eq!(
            parsed.items[1].preferred_subtitle_language_code.as_deref(),
            Some("eng")
        );
        assert_eq!(parsed.items[1].subtitle_mode, Some(3));
    }

    #[test]
    fn playlist_save_form_deserializes_single_item_payload() {
        let form = PlaylistSaveForm::from_urlencoded_bytes(
            b"id=3&name=firefly+and+star+trek+tng&item_index=0&item_id=0&item_collection_type=1&item_collection_id=&item_multi_collection_id=&item_smart_collection_id=&item_media_item_id=2124&item_playback_order=1&item_count=1&item_include_in_program_guide_index=0",
        );

        let parsed = form.into_editor_form();

        assert_eq!(parsed.id, 3);
        assert_eq!(parsed.name, "firefly and star trek tng");
        assert_eq!(parsed.items.len(), 1);
        assert_eq!(parsed.items[0].index, 0);
        assert_eq!(parsed.items[0].collection_type, 1);
        assert_eq!(parsed.items[0].media_item_id, Some(2124));
        assert_eq!(parsed.items[0].count, Some(1));
        assert!(!parsed.items[0].play_all);
        assert!(parsed.items[0].include_in_program_guide);
    }

    #[test]
    fn playlist_save_form_deserializes_repeated_item_payload() {
        let form = PlaylistSaveForm::from_urlencoded_bytes(
            b"id=3&name=firefly+and+star+trek+tng&item_index=0&item_id=0&item_collection_type=1&item_collection_id=&item_multi_collection_id=&item_smart_collection_id=&item_media_item_id=1197&item_playback_order=1&item_count=1&item_index=1&item_id=0&item_collection_type=1&item_collection_id=&item_multi_collection_id=&item_smart_collection_id=&item_media_item_id=2124&item_playback_order=1&item_count=1",
        );

        let parsed = form.into_editor_form();

        assert_eq!(parsed.items.len(), 2);
        assert_eq!(parsed.items[0].media_item_id, Some(1197));
        assert_eq!(parsed.items[1].media_item_id, Some(2124));
        assert_eq!(parsed.items[0].index, 0);
        assert_eq!(parsed.items[1].index, 1);
    }

    #[test]
    fn playlist_save_form_applies_no_js_move_before_persistence() {
        let form = PlaylistSaveForm::from_urlencoded_bytes(
            b"id=3&name=Saturday&item_move=down%3A0&item_index=0&item_id=11&item_collection_type=0&item_collection_id=5&item_playback_order=1&item_index=1&item_id=12&item_collection_type=5&item_smart_collection_id=7&item_playback_order=1",
        );
        let requested_move = form.item_move.clone();
        let mut parsed = form.into_editor_form();

        apply_playlist_item_move(&mut parsed.items, &requested_move).expect("valid down move");

        assert_eq!(parsed.items[0].id, 12);
        assert_eq!(parsed.items[0].index, 0);
        assert_eq!(parsed.items[1].id, 11);
        assert_eq!(parsed.items[1].index, 1);
    }

    #[test]
    fn playlist_save_form_rejects_out_of_bounds_move() {
        let mut items = vec![PlaylistItemForm::default()];

        let error = apply_playlist_item_move(&mut items, "up:0")
            .expect_err("first playlist item cannot move up");

        assert_eq!(error, "playlist item move is out of bounds");
    }
}
