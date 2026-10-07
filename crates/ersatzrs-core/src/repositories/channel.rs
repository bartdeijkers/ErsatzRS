use async_trait::async_trait;
use chrono::{DateTime, Duration, Utc};
use ersatzrs_domain::{Channel, EntityId};
use serde::{Deserialize, Serialize};

use super::Result;

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct ChannelAdminListRow {
    pub id: EntityId,
    pub number: String,
    pub name: String,
    pub preferred_audio_language_code: Option<String>,
    pub is_enabled: bool,
    pub show_in_epg: bool,
    pub streaming_mode: i32,
    pub ffmpeg_profile_name: String,
    pub playout_count: i64,
    pub logo_path: Option<String>,
}

/// Storage-neutral channel and programme projection for the admin previewer.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct ChannelWatchState {
    pub id: EntityId,
    pub number: String,
    pub name: String,
    pub logo_path: Option<String>,
    pub ffmpeg_profile_name: String,
    pub streaming_mode: i32,
    pub is_enabled: bool,
    pub playout_id: Option<EntityId>,
    pub programmes: Vec<ChannelWatchProgramme>,
}

/// One EPG-visible channel loaded as part of the aggregate admin guide query.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct EpgPreviewChannelState {
    pub id: EntityId,
    pub number: String,
    pub sort_number: f64,
    pub name: String,
    pub logo_path: Option<String>,
    pub ffmpeg_profile_name: String,
    pub streaming_mode: i32,
    pub playout_id: Option<EntityId>,
    pub programmes: Vec<ChannelWatchProgramme>,
}

/// One programme or explicit schedule gap shown by the channel previewer.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct ChannelWatchProgramme {
    pub id: EntityId,
    pub title: String,
    pub start: DateTime<Utc>,
    pub finish: DateTime<Utc>,
    pub filler_kind: i32,
    pub is_gap: bool,
}

#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct ChannelSaveSideEffects {
    pub subtitle_mode: i32,
    pub playout_ids: Vec<EntityId>,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct ChannelValidationRow {
    pub id: EntityId,
    pub number: String,
    pub name: String,
    pub playout_source: i32,
    pub playout_count: i64,
}

#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct ChannelValidationState {
    pub channels: Vec<ChannelValidationRow>,
    pub ffmpeg_profile_ids: Vec<EntityId>,
    pub watermark_ids: Vec<EntityId>,
    pub fallback_filler_ids: Vec<EntityId>,
}

#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct ChannelLogoForm {
    pub path: Option<String>,
    pub content_type: Option<String>,
}

#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct ChannelEditorForm {
    pub id: EntityId,
    pub number: String,
    pub name: String,
    pub group: String,
    pub categories: Option<String>,
    pub ffmpeg_profile_id: EntityId,
    pub slug_seconds: Option<f64>,
    pub logo: ChannelLogoForm,
    pub external_logo_url: Option<String>,
    pub stream_selector_mode: i32,
    pub stream_selector: Option<String>,
    pub preferred_audio_language_code: Option<String>,
    pub preferred_audio_title: Option<String>,
    pub playout_source: i32,
    pub playout_mode: i32,
    pub mirror_source_channel_id: Option<EntityId>,
    pub playout_offset: Option<Duration>,
    #[serde(default)]
    pub streaming_engine: i32,
    #[serde(default)]
    pub next_engine_text_subtitle_mode: i32,
    pub streaming_mode: i32,
    pub watermark_id: Option<EntityId>,
    pub fallback_filler_id: Option<EntityId>,
    pub preferred_subtitle_language_code: Option<String>,
    pub subtitle_mode: i32,
    pub music_video_credits_mode: i32,
    pub music_video_credits_template: Option<String>,
    pub song_video_mode: i32,
    pub transcode_mode: i32,
    pub idle_behavior: i32,
    pub is_enabled: bool,
    pub show_in_epg: bool,
}

impl ChannelEditorForm {
    #[must_use]
    pub fn new_defaults(ffmpeg_profile_id: EntityId, number: String) -> Self {
        Self {
            number,
            name: "New Channel".to_owned(),
            group: "ErsatzRS".to_owned(),
            ffmpeg_profile_id,
            streaming_engine: 1,
            streaming_mode: 5,
            is_enabled: true,
            show_in_epg: true,
            ..Self::default()
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct ChannelSelectOption {
    pub id: EntityId,
    pub value: Option<String>,
    pub label: String,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct ChannelMirrorOption {
    pub id: EntityId,
    pub number: String,
    pub name: String,
    pub playout_source: i32,
    pub playout_count: i64,
}

#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct ChannelEditorOptions {
    pub ffmpeg_profiles: Vec<ChannelSelectOption>,
    pub watermarks: Vec<ChannelSelectOption>,
    pub fallback_fillers: Vec<ChannelSelectOption>,
    pub mirror_channels: Vec<ChannelMirrorOption>,
    pub language_codes: Vec<ChannelSelectOption>,
    pub stream_selectors: Vec<String>,
    pub music_video_credit_templates: Vec<String>,
    pub default_ffmpeg_profile_id: EntityId,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct ChannelEditorState {
    pub form: ChannelEditorForm,
    pub options: ChannelEditorOptions,
    pub playout_count: i64,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct ChannelSaveResult {
    pub id: EntityId,
    pub validation_errors: Vec<ChannelValidationError>,
}

impl ChannelSaveResult {
    #[must_use]
    pub fn success(id: EntityId) -> Self {
        Self {
            id,
            validation_errors: Vec::new(),
        }
    }

    #[must_use]
    pub fn validation(errors: Vec<ChannelValidationError>) -> Self {
        Self {
            id: 0,
            validation_errors: errors,
        }
    }

    #[must_use]
    pub fn is_success(&self) -> bool {
        self.validation_errors.is_empty()
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct ChannelNumberRow {
    pub id: EntityId,
    pub number: String,
    pub name: String,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct ChannelNumberUpdate {
    pub id: EntityId,
    pub number: String,
}

#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct ChannelNumberSaveResult {
    pub validation_errors: Vec<ChannelValidationError>,
    pub changed_ids: Vec<EntityId>,
}

impl ChannelNumberSaveResult {
    #[must_use]
    pub fn success(changed_ids: Vec<EntityId>) -> Self {
        Self {
            validation_errors: Vec::new(),
            changed_ids,
        }
    }

    #[must_use]
    pub fn validation(errors: Vec<ChannelValidationError>) -> Self {
        Self {
            validation_errors: errors,
            changed_ids: Vec::new(),
        }
    }

    #[must_use]
    pub fn is_success(&self) -> bool {
        self.validation_errors.is_empty()
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct ChannelValidationError {
    pub field: String,
    pub message: String,
}

impl ChannelValidationError {
    #[must_use]
    pub fn new(field: impl Into<String>, message: impl Into<String>) -> Self {
        Self {
            field: field.into(),
            message: message.into(),
        }
    }
}

#[async_trait]
pub trait ChannelRepository: Send + Sync {
    async fn get_with_profile(&self, id: EntityId) -> Result<Channel>;
    async fn get_by_number(&self, number: &str) -> Result<Channel>;
    async fn list_enabled(&self) -> Result<Vec<Channel>>;
    async fn upsert(&self, channel: &Channel) -> Result<()>;
    async fn delete(&self, id: EntityId) -> Result<()>;
    async fn get_editor_state(&self, id: Option<EntityId>) -> Result<Option<ChannelEditorState>>;
    async fn save_editor_form(&self, form: ChannelEditorForm) -> Result<ChannelSaveResult>;
    async fn list_number_rows(&self) -> Result<Vec<ChannelNumberRow>>;
    async fn save_number_updates(
        &self,
        updates: Vec<ChannelNumberUpdate>,
    ) -> Result<ChannelNumberSaveResult>;
    async fn list_admin_rows(
        &self,
        show_disabled: bool,
        page: u32,
        page_size: u32,
    ) -> Result<Vec<ChannelAdminListRow>>;
    async fn count_admin_rows(&self, show_disabled: bool) -> Result<u64>;
    /// Load a channel and its programmes overlapping the requested UTC window.
    async fn get_watch_state(
        &self,
        id: EntityId,
        now: DateTime<Utc>,
        start: DateTime<Utc>,
        finish: DateTime<Utc>,
    ) -> Result<Option<ChannelWatchState>>;
    /// Load all enabled, EPG-visible channels and their selected playout rows
    /// for one bounded UTC window using batched storage reads.
    async fn list_epg_preview_states(
        &self,
        now: DateTime<Utc>,
        start: DateTime<Utc>,
        finish: DateTime<Utc>,
    ) -> Result<Vec<EpgPreviewChannelState>>;
    async fn list_ffmpeg_profile_options(&self) -> Result<Vec<ChannelSelectOption>>;
    async fn list_filler_preset_options(&self) -> Result<Vec<ChannelSelectOption>>;
    async fn validation_state(&self) -> Result<ChannelValidationState>;
    async fn save_side_effects(&self, id: EntityId) -> Result<Option<ChannelSaveSideEffects>>;
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn new_channel_defaults_next_without_reinterpreting_legacy_serialized_forms() {
        let form = ChannelEditorForm::new_defaults(7, "101".to_owned());
        assert_eq!(form.streaming_engine, 1);
        assert_eq!(form.streaming_mode, 5);
        assert_eq!(form.ffmpeg_profile_id, 7);
        let mut old = serde_json::to_value(&form).unwrap();
        old.as_object_mut().unwrap().remove("streaming_engine");
        let restored: ChannelEditorForm = serde_json::from_value(old).unwrap();
        assert_eq!(restored.streaming_engine, 0);
        assert_eq!(ChannelEditorForm::default().streaming_engine, 0);
    }
}
