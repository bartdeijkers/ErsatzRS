//! Application command requests.

use ersatzrs_core::locking::PlayoutBuildAdmission;
use ersatzrs_core::repositories::{
    AddonMediaListItemReviewEdit, AddonMediaListPublicationBlockerKind, AddonMediaListRepository,
    AdminSettingsRepository, BlockAdminRow, BlockItemAdmin, BlockRepository, ChannelEditorForm,
    ChannelNumberUpdate, ChannelRepository, ChannelValidationError, ChannelValidationState,
    CollectionRepository, DecoAdminRecord, DecoBreakContentAdmin, DecoRepository,
    DecoTemplateItemAdmin, FfmpegProfileRepository, FillerRepository, LibraryRepository,
    ManagedLibraryOperationRepository, MediaItemRepository, MultiCollectionItem, PlaybackOverrides,
    PlaylistEditorAdmin, PlaylistItemInsert, PlayoutAlternateScheduleItemAdmin, PlayoutCreateAdmin,
    PlayoutRepository, PlayoutTemplateItemAdmin, ProgramScheduleRepository, RepositoryError,
    RerunCollectionAdmin, RerunCollectionReferenceKind, SavedSearchRepository, ScheduleAdminForm,
    ScheduleItemAdmin, SettingsMutation, TemplateItemAdmin, TemplateRepository,
    WatermarkRepository,
};
use ersatzrs_core::scheduling::{
    next_playout_seed, PlayoutBuildMode, PlayoutBuildRequest, SchedulingSmartShufflePreset,
};
use ersatzrs_domain::{
    BlockStopScheduling, ChannelWatermark, ChannelWatermarkImageSource, ChannelWatermarkMode,
    CollectionType, ContentLightLevel, DecoBreakPlacement, DecoMode, EntityId, FFmpegProfile,
    FFmpegProfileAudioFormat, FFmpegProfileBitDepth, FFmpegProfileTonemapAlgorithm,
    FFmpegProfileVideoFormat, FillerKind, FillerMode, FillerPreset, FilterMode,
    HardwareAccelerationKind, LibraryMediaKind, ManagedLibraryOperationKind,
    MasteringDisplayMetadata, MediaSourceKind, NormalizeLoudnessMode, OutputDynamicRange,
    PlaybackOrder, ScalingBehavior, VaapiDriver, WatermarkLocation, WatermarkSize,
};

use crate::operations::OperationCoordinator;
use crate::operations::OperationKey;
use crate::ports::SettingsRuntime;
use crate::ports::{ChannelRuntime, PlayoutBuildQueue, ScanWorkQueue};
use crate::ApplicationError;

/// Persist one settings value.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SetAdminSetting {
    pub key: &'static str,
    pub value: String,
}

/// Persist or clear one optional settings identity.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct SetOptionalAdminSetting {
    pub key: &'static str,
    pub value: Option<EntityId>,
}

/// Add a custom `FFmpeg` resolution.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct CreateCustomResolution {
    pub width: i32,
    pub height: i32,
}

/// Delete one custom `FFmpeg` resolution.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct DeleteCustomResolution {
    pub id: EntityId,
}

#[allow(clippy::struct_excessive_bools)]
#[derive(Clone, Debug, PartialEq)]
pub struct FfmpegProfileAdminForm {
    pub id: EntityId,
    pub name: String,
    pub thread_count: i32,
    pub transcode_video: bool,
    pub transcode_audio: bool,
    pub hardware_acceleration: i32,
    pub vaapi_display: String,
    pub vaapi_driver: i32,
    pub vaapi_device: String,
    pub qsv_extra_hardware_frames: i32,
    pub resolution_id: EntityId,
    pub scaling_behavior: i32,
    pub pad_mode: i32,
    pub video_format: i32,
    pub video_profile: String,
    pub video_preset: String,
    pub video_tune: String,
    pub allow_b_frames: bool,
    pub bit_depth: i32,
    pub video_bitrate: i32,
    pub video_buffer_size: i32,
    pub tonemap_algorithm: i32,
    /// ADR 0064 `OutputDynamicRange`: 0 = `SdrBt709`, 1 = `Hdr10Bt2020Pq`.
    pub output_dynamic_range: i32,
    /// ADR 0064 mastering-display override in `G(x,y)B(x,y)R(x,y)WP(x,y)L(max,min)`
    /// form. Blank means the documented default.
    pub hdr_mastering_display: String,
    /// ADR 0064 `MaxCLL` override in cd/m². `0` means the documented default.
    pub hdr_max_cll: i32,
    /// ADR 0064 `MaxFALL` override in cd/m². `0` means the documented default.
    pub hdr_max_fall: i32,
    pub audio_format: i32,
    pub audio_bitrate: i32,
    pub audio_buffer_size: i32,
    pub normalize_loudness_mode: i32,
    pub target_loudness: String,
    pub audio_channels: i32,
    pub audio_sample_rate: i32,
    pub normalize_framerate: bool,
    pub normalize_colors: bool,
    pub deinterlace_video: bool,
}

#[derive(Clone, Debug, PartialEq)]
pub struct SaveFfmpegProfile {
    pub form: FfmpegProfileAdminForm,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct DeleteFfmpegProfile {
    pub id: EntityId,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct CopyFfmpegProfile {
    pub id: EntityId,
    pub name: String,
}

#[allow(clippy::struct_excessive_bools)]
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct FillerPresetAdminForm {
    pub id: EntityId,
    pub name: String,
    pub filler_kind: i32,
    pub filler_mode: i32,
    pub duration_hours: i32,
    pub duration_minutes: i32,
    pub duration_seconds: i32,
    pub count: Option<i32>,
    pub pad_to_nearest_minute: Option<i32>,
    pub allow_watermarks: bool,
    pub collection_type: i32,
    pub collection_id: Option<EntityId>,
    pub media_item_id: Option<EntityId>,
    pub multi_collection_id: Option<EntityId>,
    pub smart_collection_id: Option<EntityId>,
    pub playlist_id: Option<EntityId>,
    pub expression: String,
    pub use_chapters_as_media_items: bool,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SaveFillerPreset {
    pub form: FillerPresetAdminForm,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct DeleteFillerPreset {
    pub id: EntityId,
}

#[derive(Clone, Debug, PartialEq)]
pub struct WatermarkAdminForm {
    pub id: EntityId,
    pub name: String,
    pub mode: i32,
    pub image_source: i32,
    pub image: Option<String>,
    pub original_content_type: Option<String>,
    pub location: i32,
    pub size: i32,
    pub width_percent: f64,
    pub horizontal_margin_percent: f64,
    pub vertical_margin_percent: f64,
    pub frequency_minutes: i32,
    pub duration_seconds: i32,
    pub opacity: i32,
    pub place_within_source_content: bool,
    pub opacity_expression: String,
    pub z_index: i32,
}

#[derive(Clone, Debug, PartialEq)]
pub struct SaveWatermark {
    pub form: WatermarkAdminForm,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct DeleteWatermark {
    pub id: EntityId,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct CopyWatermark {
    pub id: EntityId,
    pub name: String,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct CreateTemplateGroup {
    pub name: String,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct CreateTemplate {
    pub group_id: EntityId,
    pub name: String,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct CopyTemplate {
    pub template_id: EntityId,
    pub new_group_id: EntityId,
    pub name: String,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct DeleteTemplateGroup {
    pub id: EntityId,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct DeleteTemplate {
    pub id: EntityId,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct TemplateItemForm {
    pub block_id: EntityId,
    pub start_minutes: i32,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SaveTemplate {
    pub id: EntityId,
    pub group_id: EntityId,
    pub name: String,
    pub items: Vec<TemplateItemForm>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct CreateBlockGroup {
    pub name: String,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct CreateBlock {
    pub group_id: EntityId,
    pub name: String,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct CopyBlock {
    pub block_id: EntityId,
    pub new_group_id: EntityId,
    pub name: String,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct DeleteBlockGroup {
    pub id: EntityId,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct DeleteBlock {
    pub id: EntityId,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct BlockItemAdminForm {
    pub id: EntityId,
    pub index: i32,
    pub collection_type: i32,
    pub collection_id: Option<EntityId>,
    pub multi_collection_id: Option<EntityId>,
    pub smart_collection_id: Option<EntityId>,
    pub playlist_id: Option<EntityId>,
    pub media_item_id: Option<EntityId>,
    pub search_title: Option<String>,
    pub search_query: Option<String>,
    pub playback_order: i32,
    pub smart_shuffle: ersatzrs_core::scheduling::SchedulingSmartShufflePolicy,
    pub inherited_legacy_smart_shuffle: bool,
    pub include_in_program_guide: bool,
    pub disable_watermarks: bool,
    pub collection_name: String,
    pub watermark_ids: Vec<EntityId>,
    pub graphics_element_ids: Vec<EntityId>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SaveBlock {
    pub id: EntityId,
    pub group_id: EntityId,
    pub name: String,
    pub minutes: i32,
    pub stop_scheduling: i32,
    pub items: Vec<BlockItemAdminForm>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct CreateDecoGroup {
    pub name: String,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct CreateDeco {
    pub group_id: EntityId,
    pub name: String,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct DeleteDecoGroup {
    pub id: EntityId,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct DeleteDeco {
    pub id: EntityId,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct DecoBreakContentAdminForm {
    pub id: EntityId,
    pub collection_type: i32,
    pub collection_id: Option<EntityId>,
    pub media_item_id: Option<EntityId>,
    pub multi_collection_id: Option<EntityId>,
    pub smart_collection_id: Option<EntityId>,
    pub playlist_id: Option<EntityId>,
    pub collection_name: String,
    pub placement: i32,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct DecoAdminForm {
    pub id: EntityId,
    pub group_id: EntityId,
    pub group_name: String,
    pub name: String,
    pub watermark_mode: i32,
    pub watermark_ids: Vec<EntityId>,
    pub use_watermark_during_filler: bool,
    pub graphics_elements_mode: i32,
    pub graphics_element_ids: Vec<EntityId>,
    pub use_graphics_elements_during_filler: bool,
    pub break_content_mode: i32,
    pub break_content: Vec<DecoBreakContentAdminForm>,
    pub default_filler_mode: i32,
    pub default_filler_collection_type: i32,
    pub default_filler_collection_id: Option<EntityId>,
    pub default_filler_media_item_id: Option<EntityId>,
    pub default_filler_multi_collection_id: Option<EntityId>,
    pub default_filler_smart_collection_id: Option<EntityId>,
    pub default_filler_trim_to_fit: bool,
    pub dead_air_fallback_mode: i32,
    pub dead_air_fallback_collection_type: i32,
    pub dead_air_fallback_collection_id: Option<EntityId>,
    pub dead_air_fallback_media_item_id: Option<EntityId>,
    pub dead_air_fallback_multi_collection_id: Option<EntityId>,
    pub dead_air_fallback_smart_collection_id: Option<EntityId>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SaveDeco {
    pub form: DecoAdminForm,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct CreateDecoTemplateGroup {
    pub name: String,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct CreateDecoTemplate {
    pub group_id: EntityId,
    pub name: String,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct DeleteDecoTemplateGroup {
    pub id: EntityId,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct DeleteDecoTemplate {
    pub id: EntityId,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SaveDecoTemplate {
    pub id: EntityId,
    pub group_id: EntityId,
    pub name: String,
    pub items: Vec<DecoTemplateItemAdmin>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SaveCollection {
    pub id: EntityId,
    pub name: String,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct DeleteCollection {
    pub id: EntityId,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SaveSmartCollection {
    pub id: EntityId,
    pub name: String,
    pub query: String,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct DeleteSmartCollection {
    pub id: EntityId,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SaveSavedSearch {
    pub id: EntityId,
    pub name: String,
    pub query: String,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct DeleteSavedSearch {
    pub id: EntityId,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SaveMultiCollection {
    pub id: EntityId,
    pub name: String,
    pub items: Vec<MultiCollectionItem>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct DeleteMultiCollection {
    pub id: EntityId,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SaveRerunCollection {
    pub form: RerunCollectionAdmin,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct DeleteRerunCollection {
    pub id: EntityId,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct CreatePlaylistGroup {
    pub name: String,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct CreatePlaylist {
    pub playlist_group_id: EntityId,
    pub name: String,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct DeletePlaylistGroup {
    pub id: EntityId,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct DeletePlaylist {
    pub id: EntityId,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SavePlaylist {
    pub form: PlaylistEditorAdmin,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct AddCollectionItems {
    pub collection_id: EntityId,
    pub media_item_ids: Vec<EntityId>,
    pub playback: PlaybackOverrides,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct AddMediaItemToCollection {
    pub media_item_id: EntityId,
    pub collection_id: Option<EntityId>,
    pub new_name: Option<String>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct AddPlaylistItems {
    pub playlist_id: EntityId,
    pub items: Vec<PlaylistItemInsert>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct SetCollectionCustomOrder {
    pub collection_id: EntityId,
    pub enabled: bool,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct RemoveCollectionRows {
    pub collection_id: EntityId,
    pub row_ids: Vec<EntityId>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct UpdateCollectionCustomOrder {
    pub collection_id: EntityId,
    pub ordered_row_ids: Vec<EntityId>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SaveSchedule {
    pub form: ScheduleAdminForm,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct DeleteSchedule {
    pub id: EntityId,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct CopySchedule {
    pub id: EntityId,
    pub name: String,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ReplaceScheduleItems {
    pub schedule_id: EntityId,
    pub items: Vec<ScheduleItemAdmin>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct AppendScheduleMediaItem {
    pub schedule_id: EntityId,
    pub collection_type: i32,
    pub media_item_id: EntityId,
    pub pinned_stream_selection: Option<ersatzrs_domain::PinnedStreamSelection>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ChangeCollectionItem {
    pub collection_id: EntityId,
    pub media_item_id: EntityId,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SaveLoggingSettings {
    pub default_minimum_log_level: String,
    pub scanning_minimum_log_level: String,
    pub scheduling_minimum_log_level: String,
    pub searching_minimum_log_level: String,
    pub streaming_minimum_log_level: String,
    pub http_minimum_log_level: String,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct SaveHdhrSettings {
    pub tuner_count: i32,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct SaveScannerSettings {
    pub library_refresh_interval: i32,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct SaveAddonDetailWaitSettings {
    pub minimum_seconds: i32,
    pub maximum_seconds: i32,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct SavePlayoutSettings {
    pub days_to_build: i32,
    pub skip_missing_items: bool,
    pub scripted_schedule_timeout_seconds: i32,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SaveUiSettings {
    pub is_dark_mode: bool,
    pub language: String,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SaveXmltvSettings {
    pub days_to_build: i32,
    pub time_zone: String,
    pub block_behavior: String,
}

#[allow(clippy::struct_excessive_bools)]
#[derive(Clone, Debug, PartialEq)]
pub struct SaveFfmpegSettings {
    pub ffmpeg_path: String,
    pub ffprobe_path: String,
    pub default_ffmpeg_profile_id: EntityId,
    pub preferred_audio_language_code: String,
    pub preferred_subtitle_language_code: String,
    pub fallback_subtitle_language_code: String,
    pub use_embedded_subtitles: bool,
    pub extract_embedded_subtitles: bool,
    pub probe_for_interlaced_frames: bool,
    pub save_reports: bool,
    pub global_watermark_id: Option<EntityId>,
    pub global_fallback_filler_id: Option<EntityId>,
    pub hls_segmenter_idle_timeout: i32,
    pub work_ahead_segmenter_limit: i32,
    pub active_transcode_limit: i32,
    pub initial_segment_count: i32,
    pub hls_direct_output_format: i32,
    pub default_mpeg_ts_script: String,
}

/// Persist one already-validated settings value.
pub async fn set_admin_setting(
    command: SetAdminSetting,
    repository: &dyn AdminSettingsRepository,
) -> crate::Result<()> {
    repository
        .set_string(command.key, &command.value)
        .await
        .map_err(Into::into)
}

/// Persist or clear one optional setting.
pub async fn set_optional_admin_setting(
    command: SetOptionalAdminSetting,
    repository: &dyn AdminSettingsRepository,
) -> crate::Result<()> {
    match command.value {
        Some(value) => repository
            .set_string(command.key, &value.to_string())
            .await
            .map_err(Into::into),
        None => repository.delete_key(command.key).await.map_err(Into::into),
    }
}

/// Validate and create one custom resolution.
pub async fn create_custom_resolution(
    command: CreateCustomResolution,
    repository: &dyn AdminSettingsRepository,
) -> crate::Result<()> {
    if command.width <= 0 || command.height <= 0 {
        return Err(ApplicationError::Validation {
            field: "resolution",
            message: "Resolution width or height is invalid".to_owned(),
        });
    }
    repository
        .create_custom_resolution(command.width, command.height)
        .await
        .map_err(Into::into)
}

/// Validate and delete one custom resolution.
pub async fn delete_custom_resolution(
    command: DeleteCustomResolution,
    repository: &dyn AdminSettingsRepository,
) -> crate::Result<()> {
    validate_positive_id(command.id, "resolution_id")?;
    repository
        .delete_custom_resolution(command.id)
        .await
        .map_err(Into::into)
}

pub async fn save_ffmpeg_profile(
    mut command: SaveFfmpegProfile,
    repository: &dyn FfmpegProfileRepository,
) -> crate::Result<EntityId> {
    if command.form.id < 0 {
        return validation_error("profile_id", "must be zero or greater");
    }
    command.form.name = command.form.name.trim().to_owned();
    if command.form.name.is_empty() {
        return validation_error("name", "Profile name is required.");
    }
    if command.form.name.chars().count() > 50 {
        return validation_error("name", "Profile name must be 50 characters or fewer.");
    }
    if command.form.thread_count < 0 {
        return validation_error("thread_count", "Thread count must be 0 or greater.");
    }
    if repository
        .name_exists(command.form.id, &command.form.name)
        .await?
    {
        return validation_error(
            "name",
            format!(
                "An FFmpeg profile named \"{}\" already exists.",
                command.form.name
            ),
        );
    }
    if command.form.id > 0
        && repository
            .get_optional_admin(command.form.id)
            .await?
            .is_none()
    {
        return Err(ApplicationError::NotFound {
            kind: "ffmpeg profile",
            id: command.form.id,
        });
    }

    let profile = normalize_ffmpeg_profile(command.form, repository).await?;
    repository.save_admin(&profile).await.map_err(Into::into)
}

pub async fn delete_ffmpeg_profile(
    command: DeleteFfmpegProfile,
    repository: &dyn FfmpegProfileRepository,
) -> crate::Result<()> {
    validate_positive_id(command.id, "profile_id")?;
    repository.delete(command.id).await.map_err(Into::into)
}

pub async fn copy_ffmpeg_profile(
    command: CopyFfmpegProfile,
    repository: &dyn FfmpegProfileRepository,
) -> crate::Result<EntityId> {
    validate_positive_id(command.id, "profile_id")?;
    let name = command.name.trim();
    if name.is_empty() {
        return validation_error("name", "Profile name is required.");
    }
    if name.chars().count() > 50 {
        return validation_error("name", "Profile name must be 50 characters or fewer.");
    }
    if repository.name_exists(0, name).await? {
        return validation_error(
            "name",
            format!("An FFmpeg profile named \"{name}\" already exists."),
        );
    }
    repository
        .copy_admin(command.id, name)
        .await
        .map_err(Into::into)
}

#[allow(clippy::too_many_lines)]
async fn normalize_ffmpeg_profile(
    form: FfmpegProfileAdminForm,
    repository: &dyn FfmpegProfileRepository,
) -> crate::Result<FFmpegProfile> {
    let hardware_acceleration = if form.transcode_video {
        parse_hardware_acceleration(form.hardware_acceleration)?
    } else {
        HardwareAccelerationKind::None
    };
    let video_format = if form.transcode_video {
        parse_video_format(form.video_format)?
    } else {
        FFmpegProfileVideoFormat::Copy
    };
    let audio_format = if form.transcode_audio {
        parse_audio_format(form.audio_format)?
    } else {
        FFmpegProfileAudioFormat::Copy
    };
    let resolved_hardware = if form.transcode_video {
        form.hardware_acceleration
    } else {
        0
    };
    let resolved_video_format = if form.transcode_video {
        form.video_format
    } else {
        99
    };
    let requested_bit_depth = parse_bit_depth(form.bit_depth)?;
    validate_ffmpeg_profile_options(
        resolved_hardware,
        resolved_video_format,
        form.bit_depth,
        &form.video_profile,
    )?;
    // ADR 0064: the output dynamic range only describes a transcoded stream.
    // Video copy preserves the source elementary stream, so the selection is
    // inert and is normalized away rather than persisted as a promise the
    // pipeline cannot keep.
    let output_dynamic_range = if form.transcode_video {
        validate_output_dynamic_range(
            form.output_dynamic_range,
            resolved_video_format,
            form.bit_depth,
        )?;
        parse_output_dynamic_range(form.output_dynamic_range)?
    } else {
        OutputDynamicRange::SdrBt709
    };
    let (hdr_mastering_display, hdr_content_light) =
        normalized_hdr_metadata(output_dynamic_range, &form)?;
    let bit_depth = if video_format == FFmpegProfileVideoFormat::Mpeg2Video {
        FFmpegProfileBitDepth::EightBit
    } else {
        requested_bit_depth
    };
    let pad_mode = match hardware_acceleration {
        HardwareAccelerationKind::None => FilterMode::Software,
        HardwareAccelerationKind::Vaapi => parse_filter_mode(form.pad_mode)?,
        _ => FilterMode::HardwareIfPossible,
    };
    let resolution_id = if form.resolution_id > 0 {
        form.resolution_id
    } else {
        repository
            .list_resolutions()
            .await?
            .first()
            .map(|resolution| resolution.id)
            .ok_or(ApplicationError::InvalidData {
                kind: "ffmpeg profile",
                value: "No FFmpeg resolutions are available; database bootstrap is incomplete"
                    .to_owned(),
            })?
    };
    let video_profile = normalized_ffmpeg_video_profile(
        resolved_hardware,
        resolved_video_format,
        form.bit_depth,
        &form.video_profile,
    );
    let presets = ffmpeg_presets(resolved_hardware, resolved_video_format, form.bit_depth);
    let normalized_preset = normalize_legacy_nvenc_preset(&form.video_preset);
    let video_preset = presets
        .contains(&normalized_preset.as_str())
        .then_some(normalized_preset)
        .filter(|value| !value.is_empty());
    let tunes = ffmpeg_tunes(resolved_hardware, resolved_video_format);
    let legacy_tune = legacy_nvenc_tune(&form.video_preset);
    let requested_tune = if form.video_tune.trim().is_empty() {
        legacy_tune.unwrap_or_default()
    } else {
        form.video_tune.trim()
    };
    let video_tune = tunes
        .contains(&requested_tune)
        .then_some(requested_tune.to_owned())
        .filter(|value| !value.is_empty());
    let normalize_loudness_mode = parse_normalize_loudness(form.normalize_loudness_mode)?;
    let target_loudness = (normalize_loudness_mode == NormalizeLoudnessMode::LoudNorm)
        .then(|| form.target_loudness.trim().parse::<f64>().ok())
        .flatten();

    Ok(FFmpegProfile {
        id: form.id,
        name: form.name,
        thread_count: form.thread_count,
        transcode_audio: form.transcode_audio,
        transcode_video: form.transcode_video,
        hardware_acceleration,
        vaapi_display: normalize_optional_string(Some(form.vaapi_display)),
        vaapi_driver: parse_vaapi_driver(form.vaapi_driver)?,
        vaapi_device: normalize_optional_string(Some(form.vaapi_device)),
        qsv_extra_hardware_frames: (hardware_acceleration == HardwareAccelerationKind::Qsv)
            .then_some(form.qsv_extra_hardware_frames),
        resolution_id,
        scaling_behavior: parse_scaling_behavior(form.scaling_behavior)?,
        pad_mode,
        video_format,
        video_profile,
        video_preset,
        video_tune,
        allow_b_frames: form.allow_b_frames,
        bit_depth,
        video_bitrate: form.video_bitrate,
        video_buffer_size: form.video_buffer_size,
        tonemap_algorithm: parse_tonemap_algorithm(form.tonemap_algorithm)?,
        output_dynamic_range,
        hdr_mastering_display,
        hdr_content_light,
        audio_format,
        audio_bitrate: form.audio_bitrate,
        audio_buffer_size: form.audio_buffer_size,
        normalize_loudness_mode,
        target_loudness,
        audio_channels: form.audio_channels,
        audio_sample_rate: form.audio_sample_rate,
        normalize_framerate: form.normalize_framerate,
        normalize_colors: form.normalize_colors,
        deinterlace_video: Some(form.deinterlace_video),
    })
}

fn validate_ffmpeg_profile_options(
    hardware_acceleration: i32,
    video_format: i32,
    bit_depth: i32,
    video_profile: &str,
) -> crate::Result<()> {
    let error = match hardware_acceleration {
        1 if !matches!(video_format, 1..=4) => {
            Some("QSV supports formats (h264, hevc, mpeg2video)")
        }
        2 if !matches!(video_format, 1 | 2 | 4) => Some("NVENC supports formats (h264, hevc, av1)"),
        3 if !matches!(video_format, 1..=4) => {
            Some("VAAPI supports formats (h264, hevc, av1, mpeg2video)")
        }
        4 if !matches!(video_format, 1 | 2) => Some("VideoToolbox supports formats (h264, hevc)"),
        5 if !matches!(video_format, 1 | 2) => Some("Amf supports formats (h264, hevc)"),
        6 if !matches!(video_format, 1 | 2) => Some("V4L2 M2M supports formats (h264, hevc)"),
        7 if !matches!(video_format, 1 | 2) => Some("Rkmpp supports formats (h264, hevc)"),
        _ => None,
    };
    if let Some(error) = error {
        return validation_error("video_format", error);
    }
    if video_format == 3 && bit_depth == 1 {
        return validation_error("bit_depth", "Mpeg2Video does not support 10-bit content");
    }
    if video_format == 1 && bit_depth == 1 {
        let required = if hardware_acceleration == 2 {
            "high444p"
        } else {
            "high10"
        };
        if video_profile.trim() != required {
            return validation_error(
                "video_profile",
                format!("VideoProfile must be {required} with 10-bit h264"),
            );
        }
    }
    if video_format == 1 && bit_depth == 0 && video_profile.trim() == "high10" {
        return validation_error(
            "video_profile",
            "VideoProfile cannot be high10 with 8-bit h264",
        );
    }
    Ok(())
}

/// Resolve the profile's HDR10 static-metadata overrides (ADR 0064).
///
/// The overrides only mean something for an HDR10 output, so they are
/// normalized away otherwise rather than persisted as dormant state that
/// silently takes effect when the profile is later switched to HDR10.
fn normalized_hdr_metadata(
    output_dynamic_range: OutputDynamicRange,
    form: &FfmpegProfileAdminForm,
) -> crate::Result<(Option<MasteringDisplayMetadata>, Option<ContentLightLevel>)> {
    if !output_dynamic_range.is_hdr10() {
        return Ok((None, None));
    }
    Ok((
        parse_hdr_mastering_display(&form.hdr_mastering_display)?,
        parse_hdr_content_light(form.hdr_max_cll, form.hdr_max_fall)?,
    ))
}

/// Rejection message for an unparseable mastering-display override.
pub const HDR10_MASTERING_DISPLAY_ERROR: &str =
    "Mastering display must look like G(8500,39850)B(6550,2300)R(35400,14600)WP(15635,16450)L(10000000,1), with chromaticities up to 50000 and max luminance above min";
/// Rejection message for content light levels outside CTA-861.3.
pub const HDR10_CONTENT_LIGHT_ERROR: &str =
    "MaxCLL and MaxFALL must both be set (or both left empty), at most 65535, with MaxFALL no greater than MaxCLL";

/// Parse the operator's mastering-display override (ADR 0064).
///
/// Blank keeps the documented default. Anything else has to parse and be in
/// range, because an invalid value would make every HDR10 spawn fail rather
/// than degrade.
fn parse_hdr_mastering_display(value: &str) -> crate::Result<Option<MasteringDisplayMetadata>> {
    let trimmed = value.trim();
    if trimmed.is_empty() {
        return Ok(None);
    }
    match MasteringDisplayMetadata::parse(trimmed) {
        Some(parsed) => Ok(Some(parsed)),
        None => {
            validation_error("hdr_mastering_display", HDR10_MASTERING_DISPLAY_ERROR).map(|()| None)
        }
    }
}

/// Parse the operator's content-light override (ADR 0064).
///
/// Both halves are required together: `MaxFALL` alone is not a CTA-861.3 pair,
/// and a half-configured override would be silently completed from the default,
/// which is exactly the hidden value ADR 0064 forbids.
fn parse_hdr_content_light(
    max_cll: i32,
    max_fall: i32,
) -> crate::Result<Option<ContentLightLevel>> {
    if max_cll <= 0 && max_fall <= 0 {
        return Ok(None);
    }
    // CTA-861.3 does let 0 mean "not indicated", but in a form where empty is
    // also 0 the two are indistinguishable. Requiring both to be positive here
    // keeps "I left it blank" from being persisted as "I measured zero"; the
    // domain type stays faithful to the standard for values read back from the
    // database.
    let levels = positive_content_light_level(max_cll)
        .zip(positive_content_light_level(max_fall))
        .and_then(|(max_cll, max_fall)| ContentLightLevel::new(max_cll, max_fall));
    match levels {
        Some(levels) => Ok(Some(levels)),
        None => validation_error("hdr_content_light", HDR10_CONTENT_LIGHT_ERROR).map(|()| None),
    }
}

/// A content light level an operator actually entered, as opposed to a blank
/// field that the form reports as `0`.
const fn positive_content_light_level(level: i32) -> Option<u32> {
    if level > 0 {
        Some(level.unsigned_abs())
    } else {
        None
    }
}

/// Rejection message for an HDR10 profile on a codec without a metadata path.
pub const HDR10_CODEC_ERROR: &str =
    "HDR10 output requires hevc or av1; h264 and mpeg2video cannot carry HDR10 static metadata";
/// Rejection message for an 8-bit HDR10 profile.
pub const HDR10_BIT_DEPTH_ERROR: &str =
    "HDR10 output requires 10-bit; 8-bit cannot represent a PQ signal without banding";

/// Reject HDR10 output combinations the pipeline cannot honour (ADR 0064).
///
/// ADR 0064 forbids silently downgrading HDR10 to SDR, so an impossible
/// combination has to fail at save time rather than at play time. The two rules
/// are the ones that make HDR10 signalling meaningless if broken:
///
/// * the codec must have a mastering-display/content-light metadata path
///   `FFmpeg` actually emits — verified for HEVC and AV1;
/// * the output must be 10-bit, because PQ banding in 8-bit is not HDR10.
///
/// Hardware acceleration is intentionally *not* rejected here. Whether a given
/// device can run the HDR10 graph is a runtime capability question that ADR
/// 0014 answers through health and troubleshooting, not a property of the saved
/// profile.
fn validate_output_dynamic_range(
    output_dynamic_range: i32,
    video_format: i32,
    bit_depth: i32,
) -> crate::Result<()> {
    if output_dynamic_range != 1 {
        return Ok(());
    }
    // 2 = Hevc, 4 = Av1.
    if !matches!(video_format, 2 | 4) {
        return validation_error("output_dynamic_range", HDR10_CODEC_ERROR);
    }
    if bit_depth != 1 {
        return validation_error("output_dynamic_range", HDR10_BIT_DEPTH_ERROR);
    }
    Ok(())
}

fn normalized_ffmpeg_video_profile(
    hardware_acceleration: i32,
    video_format: i32,
    bit_depth: i32,
    requested: &str,
) -> Option<String> {
    if video_format != 1 {
        return None;
    }
    if hardware_acceleration == 2 && bit_depth == 1 {
        return Some("high444p".to_owned());
    }
    let requested = requested.trim();
    matches!(requested, "main" | "high" | "high10" | "high444p").then(|| requested.to_owned())
}

/// `libx264` / `libx265` preset ladder, fastest to slowest (ADR 0017 v5).
///
/// `placebo` is deliberately omitted: it is unusable in a realtime encoder at
/// any resolution and x264 documents it as not recommended.
pub const SOFTWARE_VIDEO_PRESETS: [&str; 9] = [
    "ultrafast",
    "superfast",
    "veryfast",
    "faster",
    "fast",
    "medium",
    "slow",
    "slower",
    "veryslow",
];

/// QSV target-usage ladder, fastest (TU7) to slowest (TU1) (ADR 0017 v5).
pub const QSV_VIDEO_PRESETS: [&str; 7] = [
    "veryfast", "faster", "fast", "medium", "slow", "slower", "veryslow",
];

/// NVENC performance presets (ADR 0063).
pub const NVENC_VIDEO_PRESETS: [&str; 7] = ["p1", "p2", "p3", "p4", "p5", "p6", "p7"];

/// Valid `VideoPreset` values for a profile.
///
/// Widens ErsatzTV's `AvailablePresets.ForAccelAndFormat`, which offered only
/// `veryfast` for software and QSV. That single value contradicted ADR 0017's
/// promise that an admin on a strong CPU can pick a slower preset, and pinned
/// QSV to its lowest-quality target usage. The values here are the domains
/// `FFmpeg` 9 actually accepts.
///
/// Must stay in sync with `presets_for_profile` in `ersatzrs-web`, which cannot
/// call this function because `ersatzrs-app` is excluded from the wasm32 build.
#[must_use]
pub fn ffmpeg_presets(hw: i32, format: i32, depth: i32) -> Vec<&'static str> {
    match (hw, format, depth) {
        (2, 1 | 2 | 4, _) => NVENC_VIDEO_PRESETS.to_vec(),
        // 10-bit H.264 always encodes through libx264, whatever the profile's
        // acceleration says.
        (_, 1, 1) | (0, 1 | 2, _) => SOFTWARE_VIDEO_PRESETS.to_vec(),
        (1, 1 | 2, _) => QSV_VIDEO_PRESETS.to_vec(),
        _ => Vec::new(),
    }
}

/// Valid NVENC `VideoTune` values for a profile.
///
/// `uhq` is codec-gated, not universal: `FFmpeg` 9's `hevc_nvenc` and `av1_nvenc`
/// advertise it, `h264_nvenc` does not (its tune range is 1-4).
///
/// Must stay in sync with `tunes_for_profile` in `ersatzrs-web`.
#[must_use]
pub fn ffmpeg_tunes(hw: i32, format: i32) -> Vec<&'static str> {
    match (hw, format) {
        (2, 1) => vec!["hq", "ll", "ull", "lossless"],
        (2, 2 | 4) => vec!["hq", "uhq", "ll", "ull", "lossless"],
        _ => Vec::new(),
    }
}

fn normalize_legacy_nvenc_preset(value: &str) -> String {
    match value.trim().to_ascii_lowercase().as_str() {
        "llhp" => "p1".to_owned(),
        "llhq" => "p7".to_owned(),
        _ => value.trim().to_owned(),
    }
}

fn legacy_nvenc_tune(value: &str) -> Option<&'static str> {
    matches!(value.trim().to_ascii_lowercase().as_str(), "llhp" | "llhq").then_some("ll")
}

macro_rules! parse_enum {
    ($name:ident, $type:ty, $field:literal, {$($value:literal => $variant:path),+ $(,)?}) => {
        fn $name(value: i32) -> crate::Result<$type> {
            match value {
                $($value => Ok($variant),)+
                _ => validation_error($field, format!("Invalid option: {value}")),
            }
        }
    };
}

parse_enum!(parse_hardware_acceleration, HardwareAccelerationKind, "hardware_acceleration", {
    0 => HardwareAccelerationKind::None, 1 => HardwareAccelerationKind::Qsv,
    2 => HardwareAccelerationKind::Nvenc, 3 => HardwareAccelerationKind::Vaapi,
    4 => HardwareAccelerationKind::VideoToolbox, 5 => HardwareAccelerationKind::Amf,
    6 => HardwareAccelerationKind::V4l2m2m, 7 => HardwareAccelerationKind::Rkmpp
});
parse_enum!(parse_video_format, FFmpegProfileVideoFormat, "video_format", {
    0 => FFmpegProfileVideoFormat::None, 1 => FFmpegProfileVideoFormat::H264,
    2 => FFmpegProfileVideoFormat::Hevc, 3 => FFmpegProfileVideoFormat::Mpeg2Video,
    4 => FFmpegProfileVideoFormat::Av1, 99 => FFmpegProfileVideoFormat::Copy
});
parse_enum!(parse_audio_format, FFmpegProfileAudioFormat, "audio_format", {
    0 => FFmpegProfileAudioFormat::None, 1 => FFmpegProfileAudioFormat::Aac,
    2 => FFmpegProfileAudioFormat::Ac3, 3 => FFmpegProfileAudioFormat::AacLatm,
    99 => FFmpegProfileAudioFormat::Copy
});
parse_enum!(parse_bit_depth, FFmpegProfileBitDepth, "bit_depth", {
    0 => FFmpegProfileBitDepth::EightBit, 1 => FFmpegProfileBitDepth::TenBit
});
parse_enum!(parse_filter_mode, FilterMode, "pad_mode", {
    0 => FilterMode::HardwareIfPossible, 1 => FilterMode::Software
});
parse_enum!(parse_scaling_behavior, ScalingBehavior, "scaling_behavior", {
    0 => ScalingBehavior::ScaleAndPad, 1 => ScalingBehavior::Stretch, 2 => ScalingBehavior::Crop
});
parse_enum!(parse_vaapi_driver, VaapiDriver, "vaapi_driver", {
    0 => VaapiDriver::Auto, 1 => VaapiDriver::IHD, 2 => VaapiDriver::I965,
    3 => VaapiDriver::RadeonSI, 4 => VaapiDriver::Nouveau
});
parse_enum!(parse_tonemap_algorithm, FFmpegProfileTonemapAlgorithm, "tonemap_algorithm", {
    0 => FFmpegProfileTonemapAlgorithm::Linear, 1 => FFmpegProfileTonemapAlgorithm::Clip,
    2 => FFmpegProfileTonemapAlgorithm::Gamma, 3 => FFmpegProfileTonemapAlgorithm::Reinhard,
    4 => FFmpegProfileTonemapAlgorithm::Mobius, 5 => FFmpegProfileTonemapAlgorithm::Hable
});
parse_enum!(parse_normalize_loudness, NormalizeLoudnessMode, "normalize_loudness_mode", {
    0 => NormalizeLoudnessMode::Off, 1 => NormalizeLoudnessMode::LoudNorm
});
parse_enum!(parse_output_dynamic_range, OutputDynamicRange, "output_dynamic_range", {
    0 => OutputDynamicRange::SdrBt709, 1 => OutputDynamicRange::Hdr10Bt2020Pq
});

pub async fn save_filler_preset(
    mut command: SaveFillerPreset,
    repository: &dyn FillerRepository,
) -> crate::Result<EntityId> {
    if command.form.id < 0 {
        return validation_error("filler_preset_id", "must be zero or greater");
    }
    normalize_filler_preset(&mut command.form);
    validate_filler_preset(&command.form)?;
    if repository
        .name_exists(command.form.id, &command.form.name)
        .await?
    {
        return validation_error("name", "Filler preset name must be unique");
    }
    if command.form.id > 0
        && repository
            .get_optional_admin(command.form.id)
            .await?
            .is_none()
    {
        return Err(ApplicationError::NotFound {
            kind: "filler preset",
            id: command.form.id,
        });
    }
    let preset = filler_preset_from_admin(command.form)?;
    repository.save_admin(&preset).await.map_err(Into::into)
}

pub async fn delete_filler_preset(
    command: DeleteFillerPreset,
    repository: &dyn FillerRepository,
) -> crate::Result<()> {
    validate_positive_id(command.id, "filler_preset_id")?;
    repository
        .delete_admin(command.id)
        .await
        .map_err(Into::into)
}

fn normalize_filler_preset(form: &mut FillerPresetAdminForm) {
    form.name = form.name.trim().to_owned();
    if matches!(form.filler_kind, 4 | 5) {
        form.filler_mode = 0;
    }
    if form.collection_type == 6 {
        form.filler_mode = 2;
    }
    if form.filler_kind != 2 {
        form.expression.clear();
    }
    if form.filler_kind == 5 {
        form.use_chapters_as_media_items = false;
    }
    if form.filler_mode != 1 {
        form.duration_hours = 0;
        form.duration_minutes = 0;
        form.duration_seconds = 0;
    }
    if !matches!(form.filler_mode, 2 | 4) {
        form.count = None;
    }
    if form.filler_mode != 3 {
        form.pad_to_nearest_minute = None;
    }
    if form.collection_type != 0 {
        form.collection_id = None;
    }
    if !matches!(form.collection_type, 1..=3) {
        form.media_item_id = None;
    }
    if form.collection_type != 4 {
        form.multi_collection_id = None;
    }
    if form.collection_type != 5 {
        form.smart_collection_id = None;
    }
    if form.collection_type != 6 {
        form.playlist_id = None;
    }
}

fn validate_filler_preset(form: &FillerPresetAdminForm) -> crate::Result<()> {
    if form.name.is_empty() {
        return validation_error("name", "Name must not be empty");
    }
    if form.name.chars().count() > 50 {
        return validation_error("name", "Name must be 50 characters or fewer");
    }
    if !matches!(form.filler_kind, 1..=5) {
        return validation_error("filler_kind", "Filler kind must not be None");
    }
    if !matches!(form.filler_kind, 4 | 5) && form.filler_mode == 0 {
        return validation_error("filler_mode", "Filler mode must not be None");
    }
    if form.filler_mode == 2 && form.count.is_none_or(|count| count < 0) {
        return validation_error("count", "Count must be greater than or equal to 0");
    }
    if form.filler_mode == 3 && form.pad_to_nearest_minute.is_none() {
        return validation_error(
            "pad_to_nearest_minute",
            "Pad To Nearest Minute must not be empty",
        );
    }
    match form.collection_type {
        0 if form.collection_id.is_none() => {
            validation_error("collection_id", "Collection must not be empty")
        }
        4 if form.multi_collection_id.is_none() => {
            validation_error("multi_collection_id", "Multi Collection must not be empty")
        }
        5 if form.smart_collection_id.is_none() => {
            validation_error("smart_collection_id", "Smart Collection must not be empty")
        }
        1..=3 if form.media_item_id.is_none() => {
            validation_error("media_item_id", "Media item must not be empty")
        }
        6 if form.playlist_id.is_none() => {
            validation_error("playlist_id", "Playlist must not be empty")
        }
        _ => Ok(()),
    }
}

fn filler_preset_from_admin(form: FillerPresetAdminForm) -> crate::Result<FillerPreset> {
    let duration = (form.filler_mode == 1).then(|| {
        chrono::Duration::seconds(i64::from(
            ((form.duration_hours.clamp(0, 24) * 3600)
                + (form.duration_minutes.clamp(0, 59) * 60)
                + form.duration_seconds.clamp(0, 59))
            .clamp(1, 24 * 3600),
        ))
    });
    Ok(FillerPreset {
        id: form.id,
        name: form.name,
        filler_kind: parse_filler_kind(form.filler_kind)?,
        filler_mode: parse_filler_mode(form.filler_mode)?,
        collection_type: parse_filler_collection_type(form.collection_type)?,
        collection_id: form.collection_id,
        media_item_id: form.media_item_id,
        multi_collection_id: form.multi_collection_id,
        smart_collection_id: form.smart_collection_id,
        playlist_id: form.playlist_id,
        duration,
        count: form.count,
        pad_to_nearest_minute: form.pad_to_nearest_minute,
        allow_watermarks: form.allow_watermarks,
        expression: normalize_optional_string(Some(form.expression)),
        use_chapters_as_media_items: form.use_chapters_as_media_items,
        playback_order: PlaybackOrder::default(),
    })
}

parse_enum!(parse_filler_kind, FillerKind, "filler_kind", {
    0 => FillerKind::None, 1 => FillerKind::PreRoll, 2 => FillerKind::MidRoll,
    3 => FillerKind::PostRoll, 4 => FillerKind::Tail, 5 => FillerKind::Fallback,
    6 => FillerKind::GuideMode, 7 => FillerKind::DecoDefault
});
parse_enum!(parse_filler_mode, FillerMode, "filler_mode", {
    0 => FillerMode::None, 1 => FillerMode::Duration, 2 => FillerMode::Count,
    3 => FillerMode::Pad, 4 => FillerMode::RandomCount
});
parse_enum!(parse_filler_collection_type, CollectionType, "collection_type", {
    0 => CollectionType::Collection, 1 => CollectionType::TelevisionShow,
    2 => CollectionType::TelevisionSeason, 3 => CollectionType::Artist,
    4 => CollectionType::MultiCollection, 5 => CollectionType::SmartCollection,
    6 => CollectionType::Playlist
});

pub async fn save_watermark(
    mut command: SaveWatermark,
    repository: &dyn WatermarkRepository,
) -> crate::Result<EntityId> {
    if command.form.id < 0 {
        return validation_error("watermark_id", "must be zero or greater");
    }
    command.form.name = command.form.name.trim().to_owned();
    validate_watermark(&command.form)?;
    if repository
        .name_exists(command.form.id, &command.form.name)
        .await?
    {
        return validation_error("name", "Watermark name must be unique");
    }
    if command.form.id > 0 && repository.get_admin(command.form.id).await?.is_none() {
        return Err(ApplicationError::NotFound {
            kind: "watermark",
            id: command.form.id,
        });
    }
    let watermark = watermark_from_admin(command.form)?;
    repository.save_admin(&watermark).await.map_err(Into::into)
}

pub async fn delete_watermark(
    command: DeleteWatermark,
    repository: &dyn WatermarkRepository,
) -> crate::Result<()> {
    validate_positive_id(command.id, "watermark_id")?;
    repository
        .delete_admin(command.id)
        .await
        .map_err(Into::into)
}

pub async fn copy_watermark(
    command: CopyWatermark,
    repository: &dyn WatermarkRepository,
) -> crate::Result<EntityId> {
    validate_positive_id(command.id, "watermark_id")?;
    let name = command.name.trim();
    if name.is_empty() {
        return validation_error("name", "Name is required");
    }
    if name.chars().count() > 50 {
        return validation_error("name", "Name must be 50 characters or fewer");
    }
    if repository.name_exists(0, name).await? {
        return validation_error("name", "Watermark name must be unique");
    }
    repository
        .copy_admin(command.id, name)
        .await
        .map_err(Into::into)
}

fn validate_watermark(form: &WatermarkAdminForm) -> crate::Result<()> {
    if form.name.is_empty() {
        return validation_error("name", "Name is required");
    }
    if form.name.chars().count() > 50 {
        return validation_error("name", "Name must be 50 characters or fewer");
    }
    if form.image_source == 0 && form.image.as_deref().unwrap_or_default().trim().is_empty() {
        return validation_error("image", "Watermark image is required");
    }
    if form.mode != 0 {
        if form.size == 0 && !(form.width_percent > 0.0 && form.width_percent <= 100.0) {
            return validation_error("width_percent", "Width must be between 0 and 100%");
        }
        if !(0.0..=50.0).contains(&form.horizontal_margin_percent) {
            return validation_error(
                "horizontal_margin_percent",
                "Horizontal margin must be between 0 and 50%",
            );
        }
        if !(0.0..=50.0).contains(&form.vertical_margin_percent) {
            return validation_error(
                "vertical_margin_percent",
                "Vertical margin must be between 0 and 50%",
            );
        }
        if form.duration_seconds <= 0
            || form.duration_seconds >= form.frequency_minutes.saturating_mul(60)
        {
            return validation_error(
                "duration_seconds",
                "Duration must be positive and less than frequency window",
            );
        }
        if !(1..=100).contains(&form.opacity) {
            return validation_error("opacity", "Opacity must be between 1 and 100");
        }
    }
    Ok(())
}

fn watermark_from_admin(form: WatermarkAdminForm) -> crate::Result<ChannelWatermark> {
    let custom_image = form.image_source == 0;
    Ok(ChannelWatermark {
        id: form.id,
        name: form.name,
        mode: parse_watermark_mode(form.mode)?,
        image_source: parse_watermark_image_source(form.image_source)?,
        image: custom_image
            .then(|| normalize_optional_string(form.image))
            .flatten(),
        original_content_type: custom_image
            .then(|| normalize_optional_string(form.original_content_type))
            .flatten(),
        location: parse_watermark_location(form.location)?,
        size: parse_watermark_size(form.size)?,
        width_percent: form.width_percent,
        horizontal_margin_percent: form.horizontal_margin_percent,
        vertical_margin_percent: form.vertical_margin_percent,
        frequency_minutes: form.frequency_minutes,
        duration_seconds: form.duration_seconds,
        opacity: form.opacity,
        place_within_source_content: form.place_within_source_content,
        opacity_expression: (form.mode == 3)
            .then(|| normalize_optional_string(Some(form.opacity_expression)))
            .flatten(),
        z_index: form.z_index,
    })
}

parse_enum!(parse_watermark_mode, ChannelWatermarkMode, "mode", {
    0 => ChannelWatermarkMode::None, 1 => ChannelWatermarkMode::Permanent,
    2 => ChannelWatermarkMode::Intermittent, 3 => ChannelWatermarkMode::OpacityExpression
});
parse_enum!(parse_watermark_image_source, ChannelWatermarkImageSource, "image_source", {
    0 => ChannelWatermarkImageSource::Custom, 1 => ChannelWatermarkImageSource::ChannelLogo,
    100 => ChannelWatermarkImageSource::Resource
});
parse_enum!(parse_watermark_location, WatermarkLocation, "location", {
    0 => WatermarkLocation::BottomRight, 1 => WatermarkLocation::BottomLeft,
    2 => WatermarkLocation::TopRight, 3 => WatermarkLocation::TopLeft,
    4 => WatermarkLocation::TopMiddle, 5 => WatermarkLocation::RightMiddle,
    6 => WatermarkLocation::BottomMiddle, 7 => WatermarkLocation::LeftMiddle,
    8 => WatermarkLocation::MiddleCenter
});
parse_enum!(parse_watermark_size, WatermarkSize, "size", {
    0 => WatermarkSize::Scaled, 1 => WatermarkSize::ActualSize
});

pub async fn create_template_group(
    command: CreateTemplateGroup,
    repository: &dyn TemplateRepository,
) -> crate::Result<EntityId> {
    let name = validate_template_group_name(&command.name)?;
    if repository.group_name_exists_admin(name).await? {
        return validation_error("name", "Template group name must be unique");
    }
    repository
        .create_group_admin(name)
        .await
        .map_err(Into::into)
}

pub async fn create_template(
    command: CreateTemplate,
    repository: &dyn TemplateRepository,
) -> crate::Result<EntityId> {
    validate_positive_id(command.group_id, "template_group_id")?;
    if !repository.group_exists_admin(command.group_id).await? {
        return Err(ApplicationError::NotFound {
            kind: "template group",
            id: command.group_id,
        });
    }
    let name = validate_template_name(&command.name)?;
    if repository
        .template_name_exists_admin(command.group_id, None, name)
        .await?
    {
        return validation_error(
            "name",
            format!("A template named \"{name}\" already exists in that template group"),
        );
    }
    repository
        .create_admin(command.group_id, name)
        .await
        .map_err(Into::into)
}

pub async fn copy_template(
    command: CopyTemplate,
    repository: &dyn TemplateRepository,
) -> crate::Result<EntityId> {
    validate_positive_id(command.template_id, "template_id")?;
    validate_positive_id(command.new_group_id, "template_group_id")?;
    if !repository.group_exists_admin(command.new_group_id).await? {
        return Err(ApplicationError::NotFound {
            kind: "template group",
            id: command.new_group_id,
        });
    }
    if !repository
        .template_exists_admin(command.template_id)
        .await?
    {
        return Err(ApplicationError::NotFound {
            kind: "template",
            id: command.template_id,
        });
    }
    let name = validate_template_name(&command.name)?;
    if repository
        .template_name_exists_admin(command.new_group_id, None, name)
        .await?
    {
        return validation_error(
            "name",
            format!("A template named \"{name}\" already exists in that template group"),
        );
    }
    repository
        .copy_admin(command.template_id, command.new_group_id, name)
        .await
        .map_err(Into::into)
}

pub async fn delete_template_group(
    command: DeleteTemplateGroup,
    repository: &dyn TemplateRepository,
) -> crate::Result<()> {
    validate_positive_id(command.id, "template_group_id")?;
    repository
        .delete_group_admin(command.id)
        .await
        .map_err(Into::into)
}

pub async fn delete_template(
    command: DeleteTemplate,
    repository: &dyn TemplateRepository,
) -> crate::Result<()> {
    validate_positive_id(command.id, "template_id")?;
    repository
        .delete_admin(command.id)
        .await
        .map_err(Into::into)
}

pub async fn save_template(
    mut command: SaveTemplate,
    repository: &dyn TemplateRepository,
) -> crate::Result<()> {
    validate_positive_id(command.id, "template_id")?;
    validate_positive_id(command.group_id, "template_group_id")?;
    if !repository.template_exists_admin(command.id).await? {
        return Err(ApplicationError::NotFound {
            kind: "template",
            id: command.id,
        });
    }
    if !repository.group_exists_admin(command.group_id).await? {
        return Err(ApplicationError::NotFound {
            kind: "template group",
            id: command.group_id,
        });
    }
    let name = validate_template_name(&command.name)?;
    if repository
        .template_name_exists_admin(command.group_id, Some(command.id), name)
        .await?
    {
        return validation_error(
            "name",
            format!("A template named \"{name}\" already exists in that template group"),
        );
    }

    let mut blocks = std::collections::BTreeMap::new();
    for group in repository.list_block_groups_admin().await? {
        for block in repository.list_blocks_admin(group.id).await? {
            blocks.insert(block.id, block);
        }
    }
    command.items.sort_by_key(|item| item.start_minutes);
    let mut items: Vec<TemplateItemAdmin> = Vec::with_capacity(command.items.len());
    for item in command.items {
        let block = blocks
            .get(&item.block_id)
            .ok_or(ApplicationError::NotFound {
                kind: "block",
                id: item.block_id,
            })?;
        let start = item.start_minutes.clamp(0, 24 * 60);
        let end = start.saturating_add(block.minutes.max(0));
        if end > 24 * 60 {
            return validation_error("items", "Template items cannot cross midnight.");
        }
        if let Some(previous) = items.last() {
            if start < previous.start_minutes + previous.duration_minutes {
                return validation_error(
                    "items",
                    format!(
                        "Block from {} to {} intersects block from {} to {}",
                        format_template_minutes(previous.start_minutes),
                        format_template_minutes(previous.start_minutes + previous.duration_minutes),
                        format_template_minutes(start),
                        format_template_minutes(end)
                    ),
                );
            }
        }
        items.push(TemplateItemAdmin {
            block_id: block.id,
            block_name: block.name.clone(),
            start_minutes: start,
            duration_minutes: block.minutes,
        });
    }
    repository
        .save_admin(command.id, name, &items)
        .await
        .map_err(Into::into)
}

fn validate_template_group_name(name: &str) -> crate::Result<&str> {
    let name = name.trim();
    if name.is_empty() {
        return validation_error("name", "Template group name is required");
    }
    if name.chars().count() > 50 {
        return validation_error("name", "Template group name must be 50 characters or fewer");
    }
    Ok(name)
}

fn validate_template_name(name: &str) -> crate::Result<&str> {
    let name = name.trim();
    if name.is_empty() {
        return validation_error("name", "Template name is required");
    }
    if name.chars().count() > 50 {
        return validation_error("name", format!("Template name \"{name}\" is invalid"));
    }
    Ok(name)
}

fn format_template_minutes(minutes: i32) -> String {
    let clamped = minutes.clamp(0, 24 * 60);
    format!("{:02}:{:02}:00", clamped / 60, clamped % 60)
}

pub async fn create_block_group(
    command: CreateBlockGroup,
    repository: &dyn BlockRepository,
) -> crate::Result<EntityId> {
    let name = validate_block_group_name(&command.name)?;
    if repository.group_name_exists_admin(name).await? {
        return validation_error("name", "Block group name must be unique");
    }
    repository
        .create_group_admin(name)
        .await
        .map_err(Into::into)
}

pub async fn create_block(
    command: CreateBlock,
    repository: &dyn BlockRepository,
) -> crate::Result<EntityId> {
    validate_positive_id(command.group_id, "block_group_id")?;
    if !repository.group_exists_admin(command.group_id).await? {
        return Err(ApplicationError::NotFound {
            kind: "block group",
            id: command.group_id,
        });
    }
    let name = validate_block_name(&command.name)?;
    validate_unique_block_name(repository, command.group_id, None, name).await?;
    repository
        .create_admin(command.group_id, name)
        .await
        .map_err(Into::into)
}

pub async fn copy_block(
    command: CopyBlock,
    repository: &dyn BlockRepository,
) -> crate::Result<EntityId> {
    validate_positive_id(command.block_id, "block_id")?;
    validate_positive_id(command.new_group_id, "block_group_id")?;
    if !repository.group_exists_admin(command.new_group_id).await? {
        return Err(ApplicationError::NotFound {
            kind: "block group",
            id: command.new_group_id,
        });
    }
    if !repository.block_exists_admin(command.block_id).await? {
        return Err(ApplicationError::NotFound {
            kind: "block",
            id: command.block_id,
        });
    }
    let name = validate_block_name(&command.name)?;
    validate_unique_block_name(repository, command.new_group_id, None, name).await?;
    repository
        .copy_admin(command.block_id, command.new_group_id, name)
        .await
        .map_err(Into::into)
}

pub async fn delete_block_group(
    command: DeleteBlockGroup,
    repository: &dyn BlockRepository,
) -> crate::Result<()> {
    validate_positive_id(command.id, "block_group_id")?;
    repository
        .delete_group_admin(command.id)
        .await
        .map_err(Into::into)
}

pub async fn delete_block(
    command: DeleteBlock,
    repository: &dyn BlockRepository,
) -> crate::Result<()> {
    validate_positive_id(command.id, "block_id")?;
    repository
        .delete_admin(command.id)
        .await
        .map_err(Into::into)
}

pub async fn save_block(command: SaveBlock, repository: &dyn BlockRepository) -> crate::Result<()> {
    validate_positive_id(command.id, "block_id")?;
    validate_positive_id(command.group_id, "block_group_id")?;
    if !repository.block_exists_admin(command.id).await? {
        return Err(ApplicationError::NotFound {
            kind: "block",
            id: command.id,
        });
    }
    if !repository.group_exists_admin(command.group_id).await? {
        return Err(ApplicationError::NotFound {
            kind: "block group",
            id: command.group_id,
        });
    }
    if command.minutes <= 0 || command.minutes % 5 != 0 || command.minutes > 24 * 60 {
        return validation_error(
            "minutes",
            "Block duration must be between 5 minutes and 24 hours",
        );
    }
    let name = validate_block_name(&command.name)?;
    validate_unique_block_name(repository, command.group_id, Some(command.id), name).await?;
    let stop_scheduling = parse_block_stop_scheduling(command.stop_scheduling)?;
    let mut items = Vec::with_capacity(command.items.len());
    for item in command.items {
        items.push(block_item_from_admin(item)?);
    }
    repository
        .save_admin(
            &BlockAdminRow {
                id: command.id,
                group_id: command.group_id,
                group_name: String::new(),
                name: name.to_owned(),
                minutes: command.minutes,
                stop_scheduling,
            },
            &items,
        )
        .await
        .map_err(Into::into)
}

fn block_item_from_admin(form: BlockItemAdminForm) -> crate::Result<BlockItemAdmin> {
    let collection_type = parse_block_collection_type(form.collection_type)?;
    match collection_type {
        CollectionType::Collection if form.collection_id.is_none() => {
            return validation_error("collection_id", "Collection is required.");
        }
        CollectionType::TelevisionShow
        | CollectionType::TelevisionSeason
        | CollectionType::Artist
        | CollectionType::Movie
        | CollectionType::Episode
        | CollectionType::MusicVideo
        | CollectionType::OtherVideo
        | CollectionType::Song
        | CollectionType::Image
        | CollectionType::RemoteStream
            if form.media_item_id.is_none() =>
        {
            return validation_error("media_item_id", "Media Item is required.");
        }
        CollectionType::MultiCollection if form.multi_collection_id.is_none() => {
            return validation_error("multi_collection_id", "Multi Collection is required.");
        }
        CollectionType::SmartCollection if form.smart_collection_id.is_none() => {
            return validation_error("smart_collection_id", "Smart Collection is required.");
        }
        CollectionType::Playlist => {
            return validation_error(
                "collection_type",
                "Playlist block items can be previewed but are not persisted by ErsatzTV's BlockItem schema",
            );
        }
        CollectionType::SearchQuery
            if form
                .search_query
                .as_deref()
                .unwrap_or_default()
                .trim()
                .is_empty() =>
        {
            return validation_error("search_query", "Search Query is required.");
        }
        CollectionType::RerunFirstRun
        | CollectionType::RerunRerun
        | CollectionType::FakeCollection
        | CollectionType::FakePlaylistItem => {
            return validation_error(
                "collection_type",
                format!(
                    "Collection type {} is not supported by the real block scheduler",
                    form.collection_type
                ),
            );
        }
        _ => {}
    }
    validate_schedule_smart_shuffle(&form.smart_shuffle)?;
    let smart_shuffle = if form.inherited_legacy_smart_shuffle
        && form.smart_shuffle.preset == SchedulingSmartShufflePreset::Off
    {
        None
    } else {
        Some(form.smart_shuffle)
    };
    Ok(BlockItemAdmin {
        id: form.id,
        index: form.index,
        collection_type,
        collection_id: form.collection_id,
        multi_collection_id: form.multi_collection_id,
        smart_collection_id: form.smart_collection_id,
        media_item_id: form.media_item_id,
        search_title: normalize_optional_string(form.search_title),
        search_query: normalize_optional_string(form.search_query),
        playback_order: parse_block_playback_order(form.playback_order)?,
        smart_shuffle,
        include_in_program_guide: form.include_in_program_guide,
        disable_watermarks: form.disable_watermarks,
        collection_name: form.collection_name,
        watermark_ids: form.watermark_ids,
        graphics_element_ids: form.graphics_element_ids,
    })
}

fn validate_block_group_name(name: &str) -> crate::Result<&str> {
    let name = name.trim();
    if name.is_empty() {
        return validation_error("name", "Block group name is required");
    }
    if name.chars().count() > 50 {
        return validation_error("name", format!("Block group name \"{name}\" is invalid"));
    }
    Ok(name)
}

fn validate_block_name(name: &str) -> crate::Result<&str> {
    let name = name.trim();
    if name.is_empty() {
        return validation_error("name", "Block name is required");
    }
    if name.chars().count() > 50 {
        return validation_error("name", format!("Block name \"{name}\" is invalid"));
    }
    Ok(name)
}

async fn validate_unique_block_name(
    repository: &dyn BlockRepository,
    group_id: EntityId,
    block_id: Option<EntityId>,
    name: &str,
) -> crate::Result<()> {
    if repository
        .block_name_exists_admin(group_id, block_id, name)
        .await?
    {
        return validation_error(
            "name",
            "A block named with that value already exists in that block group",
        );
    }
    Ok(())
}

parse_enum!(parse_block_stop_scheduling, BlockStopScheduling, "stop_scheduling", {
    0 => BlockStopScheduling::AfterDurationEnd,
    1 => BlockStopScheduling::BeforeDurationEnd
});

parse_enum!(parse_block_playback_order, PlaybackOrder, "playback_order", {
    0 => PlaybackOrder::None, 1 => PlaybackOrder::Chronological,
    2 => PlaybackOrder::Random, 3 => PlaybackOrder::Shuffle,
    4 => PlaybackOrder::ShuffleInOrder, 5 => PlaybackOrder::MultiEpisodeShuffle,
    6 => PlaybackOrder::SeasonEpisode, 7 => PlaybackOrder::RandomRotation,
    8 => PlaybackOrder::Marathon
});

parse_enum!(parse_block_collection_type, CollectionType, "collection_type", {
    0 => CollectionType::Collection, 1 => CollectionType::TelevisionShow,
    2 => CollectionType::TelevisionSeason, 3 => CollectionType::Artist,
    4 => CollectionType::MultiCollection, 5 => CollectionType::SmartCollection,
    6 => CollectionType::Playlist, 7 => CollectionType::RerunFirstRun,
    8 => CollectionType::RerunRerun, 9 => CollectionType::SearchQuery,
    10 => CollectionType::Movie, 20 => CollectionType::Episode,
    30 => CollectionType::MusicVideo, 40 => CollectionType::OtherVideo,
    50 => CollectionType::Song, 60 => CollectionType::Image,
    70 => CollectionType::RemoteStream, 100 => CollectionType::FakeCollection,
    101 => CollectionType::FakePlaylistItem
});

pub async fn create_deco_group(
    command: CreateDecoGroup,
    repository: &dyn DecoRepository,
) -> crate::Result<EntityId> {
    let name = validate_deco_group_name(&command.name)?;
    if repository.group_name_exists_admin(name).await? {
        return validation_error("name", "Deco group name must be unique");
    }
    repository
        .create_group_admin(name)
        .await
        .map_err(Into::into)
}

pub async fn create_deco(
    command: CreateDeco,
    repository: &dyn DecoRepository,
) -> crate::Result<EntityId> {
    validate_positive_id(command.group_id, "deco_group_id")?;
    if !repository.group_exists_admin(command.group_id).await? {
        return Err(ApplicationError::NotFound {
            kind: "deco group",
            id: command.group_id,
        });
    }
    let name = validate_deco_name(&command.name)?;
    validate_unique_deco_name(repository, command.group_id, None, name).await?;
    repository
        .create_admin(command.group_id, name)
        .await
        .map_err(Into::into)
}

pub async fn delete_deco_group(
    command: DeleteDecoGroup,
    repository: &dyn DecoRepository,
) -> crate::Result<()> {
    validate_positive_id(command.id, "deco_group_id")?;
    repository
        .delete_group_admin(command.id)
        .await
        .map_err(Into::into)
}

pub async fn delete_deco(
    command: DeleteDeco,
    repository: &dyn DecoRepository,
) -> crate::Result<()> {
    validate_positive_id(command.id, "deco_id")?;
    repository
        .delete_admin(command.id)
        .await
        .map_err(Into::into)
}

pub async fn save_deco(
    mut command: SaveDeco,
    repository: &dyn DecoRepository,
) -> crate::Result<()> {
    validate_positive_id(command.form.id, "deco_id")?;
    validate_positive_id(command.form.group_id, "deco_group_id")?;
    if !repository.deco_exists_admin(command.form.id).await? {
        return Err(ApplicationError::NotFound {
            kind: "deco",
            id: command.form.id,
        });
    }
    if !repository.group_exists_admin(command.form.group_id).await? {
        return Err(ApplicationError::NotFound {
            kind: "deco group",
            id: command.form.group_id,
        });
    }
    let name = validate_deco_name(&command.form.name)?.to_owned();
    validate_unique_deco_name(
        repository,
        command.form.group_id,
        Some(command.form.id),
        &name,
    )
    .await?;
    normalize_deco_admin_form(&mut command.form);
    let options = repository.editor_options_admin().await?;
    let record = deco_record_from_admin(command.form, name)?;
    validate_deco_references(&record, &options)?;
    validate_deco_break_placements(&record.break_content)?;
    repository.save_admin(&record).await.map_err(Into::into)
}

pub async fn create_deco_template_group(
    command: CreateDecoTemplateGroup,
    repository: &dyn DecoRepository,
) -> crate::Result<EntityId> {
    let name = validate_deco_template_group_name(&command.name)?;
    if repository.template_group_name_exists_admin(name).await? {
        return validation_error("name", "Deco template group name must be unique");
    }
    repository
        .create_template_group_admin(name)
        .await
        .map_err(Into::into)
}

pub async fn create_deco_template(
    command: CreateDecoTemplate,
    repository: &dyn DecoRepository,
) -> crate::Result<EntityId> {
    validate_positive_id(command.group_id, "deco_template_group_id")?;
    if !repository
        .template_group_exists_admin(command.group_id)
        .await?
    {
        return Err(ApplicationError::NotFound {
            kind: "deco template group",
            id: command.group_id,
        });
    }
    let name = validate_deco_template_name(&command.name)?;
    validate_unique_deco_template_name(repository, command.group_id, None, name).await?;
    repository
        .create_template_admin(command.group_id, name)
        .await
        .map_err(Into::into)
}

pub async fn delete_deco_template_group(
    command: DeleteDecoTemplateGroup,
    repository: &dyn DecoRepository,
) -> crate::Result<()> {
    validate_positive_id(command.id, "deco_template_group_id")?;
    repository
        .delete_template_group_admin(command.id)
        .await
        .map_err(Into::into)
}

pub async fn delete_deco_template(
    command: DeleteDecoTemplate,
    repository: &dyn DecoRepository,
) -> crate::Result<()> {
    validate_positive_id(command.id, "deco_template_id")?;
    repository
        .delete_template_admin(command.id)
        .await
        .map_err(Into::into)
}

pub async fn save_deco_template(
    mut command: SaveDecoTemplate,
    repository: &dyn DecoRepository,
) -> crate::Result<()> {
    validate_positive_id(command.id, "deco_template_id")?;
    validate_positive_id(command.group_id, "deco_template_group_id")?;
    if !repository.template_exists_admin(command.id).await? {
        return Err(ApplicationError::NotFound {
            kind: "deco template",
            id: command.id,
        });
    }
    if !repository
        .template_group_exists_admin(command.group_id)
        .await?
    {
        return Err(ApplicationError::NotFound {
            kind: "deco template group",
            id: command.group_id,
        });
    }
    let name = validate_deco_template_name(&command.name)?;
    validate_unique_deco_template_name(repository, command.group_id, Some(command.id), name)
        .await?;
    command.items.sort_by_key(|item| item.start_minutes);
    let mut ranges = Vec::with_capacity(command.items.len());
    for item in &mut command.items {
        if !repository.deco_exists_admin(item.deco_id).await? {
            return Err(ApplicationError::NotFound {
                kind: "deco",
                id: item.deco_id,
            });
        }
        item.start_minutes = item.start_minutes.clamp(0, 24 * 60);
        item.end_minutes = if item.end_minutes == 0 {
            24 * 60
        } else {
            item.end_minutes.clamp(0, 24 * 60)
        };
        if item.start_minutes >= item.end_minutes {
            return validation_error("items", "Deco template items cannot be empty.");
        }
        ranges.push((item.start_minutes, item.end_minutes));
    }
    for (index, (start, end)) in ranges.iter().enumerate() {
        for (other_start, other_end) in ranges.iter().skip(index + 1) {
            if *start < *other_end && *other_start < *end {
                return validation_error(
                    "items",
                    format!(
                        "Deco from {} to {} intersects deco from {} to {}",
                        format_template_minutes(*start),
                        format_template_minutes(*end),
                        format_template_minutes(*other_start),
                        format_template_minutes(*other_end)
                    ),
                );
            }
        }
    }
    repository
        .save_template_admin(command.id, name, &command.items)
        .await
        .map_err(Into::into)
}

fn deco_record_from_admin(form: DecoAdminForm, name: String) -> crate::Result<DecoAdminRecord> {
    Ok(DecoAdminRecord {
        id: form.id,
        group_id: form.group_id,
        group_name: form.group_name,
        name,
        watermark_mode: parse_deco_mode(form.watermark_mode, "watermark_mode")?,
        watermark_ids: form.watermark_ids,
        use_watermark_during_filler: form.use_watermark_during_filler,
        graphics_elements_mode: parse_deco_mode(
            form.graphics_elements_mode,
            "graphics_elements_mode",
        )?,
        graphics_element_ids: form.graphics_element_ids,
        use_graphics_elements_during_filler: form.use_graphics_elements_during_filler,
        break_content_mode: parse_deco_mode(form.break_content_mode, "break_content_mode")?,
        break_content: form
            .break_content
            .into_iter()
            .map(|item| {
                Ok(DecoBreakContentAdmin {
                    id: item.id,
                    collection_type: parse_deco_collection_type(item.collection_type)?,
                    collection_id: item.collection_id,
                    media_item_id: item.media_item_id,
                    multi_collection_id: item.multi_collection_id,
                    smart_collection_id: item.smart_collection_id,
                    playlist_id: item.playlist_id,
                    collection_name: item.collection_name,
                    placement: parse_deco_placement(item.placement)?,
                })
            })
            .collect::<crate::Result<Vec<_>>>()?,
        default_filler_mode: parse_deco_mode(form.default_filler_mode, "default_filler_mode")?,
        default_filler_collection_type: parse_deco_collection_type(
            form.default_filler_collection_type,
        )?,
        default_filler_collection_id: form.default_filler_collection_id,
        default_filler_media_item_id: form.default_filler_media_item_id,
        default_filler_multi_collection_id: form.default_filler_multi_collection_id,
        default_filler_smart_collection_id: form.default_filler_smart_collection_id,
        default_filler_trim_to_fit: form.default_filler_trim_to_fit,
        dead_air_fallback_mode: parse_deco_mode(
            form.dead_air_fallback_mode,
            "dead_air_fallback_mode",
        )?,
        dead_air_fallback_collection_type: parse_deco_collection_type(
            form.dead_air_fallback_collection_type,
        )?,
        dead_air_fallback_collection_id: form.dead_air_fallback_collection_id,
        dead_air_fallback_media_item_id: form.dead_air_fallback_media_item_id,
        dead_air_fallback_multi_collection_id: form.dead_air_fallback_multi_collection_id,
        dead_air_fallback_smart_collection_id: form.dead_air_fallback_smart_collection_id,
    })
}

fn validate_deco_references(
    deco: &DecoAdminRecord,
    options: &ersatzrs_core::repositories::DecoEditorOptionsAdmin,
) -> crate::Result<()> {
    if matches!(deco.watermark_mode, DecoMode::Override | DecoMode::Merge) {
        validate_deco_ids(
            &deco.watermark_ids,
            &options.watermarks,
            "watermark_ids",
            "Watermark",
        )?;
    }
    if matches!(
        deco.graphics_elements_mode,
        DecoMode::Override | DecoMode::Merge
    ) {
        validate_deco_ids(
            &deco.graphics_element_ids,
            &options.graphics_elements,
            "graphics_element_ids",
            "Graphics element",
        )?;
    }
    if deco.default_filler_mode == DecoMode::Override {
        validate_deco_collection_reference(
            deco.default_filler_collection_type,
            deco.default_filler_collection_id,
            deco.default_filler_media_item_id,
            deco.default_filler_multi_collection_id,
            deco.default_filler_smart_collection_id,
            None,
            "Default filler",
            options,
        )?;
    }
    if deco.dead_air_fallback_mode == DecoMode::Override {
        validate_deco_collection_reference(
            deco.dead_air_fallback_collection_type,
            deco.dead_air_fallback_collection_id,
            deco.dead_air_fallback_media_item_id,
            deco.dead_air_fallback_multi_collection_id,
            deco.dead_air_fallback_smart_collection_id,
            None,
            "Dead air fallback",
            options,
        )?;
    }
    if deco.break_content_mode == DecoMode::Override {
        for item in &deco.break_content {
            validate_deco_collection_reference(
                item.collection_type,
                item.collection_id,
                item.media_item_id,
                item.multi_collection_id,
                item.smart_collection_id,
                item.playlist_id,
                "Break content",
                options,
            )?;
        }
    }
    Ok(())
}

#[allow(clippy::too_many_arguments)]
fn validate_deco_collection_reference(
    collection_type: CollectionType,
    collection_id: Option<EntityId>,
    media_item_id: Option<EntityId>,
    multi_collection_id: Option<EntityId>,
    smart_collection_id: Option<EntityId>,
    playlist_id: Option<EntityId>,
    label: &str,
    options: &ersatzrs_core::repositories::DecoEditorOptionsAdmin,
) -> crate::Result<()> {
    let (id, valid) = match collection_type {
        CollectionType::Collection => (collection_id, &options.collections),
        CollectionType::TelevisionShow => (media_item_id, &options.shows),
        CollectionType::TelevisionSeason => (media_item_id, &options.seasons),
        CollectionType::Artist => (media_item_id, &options.artists),
        CollectionType::MultiCollection => (multi_collection_id, &options.multi_collections),
        CollectionType::SmartCollection => (smart_collection_id, &options.smart_collections),
        CollectionType::Playlist => {
            let Some(id) = playlist_id.filter(|id| *id > 0) else {
                return validation_error("reference", format!("{label} must have valid item"));
            };
            if options.playlists.iter().any(|item| item.id == id) {
                return Ok(());
            }
            return validation_error("reference", format!("{label} item does not exist."));
        }
        _ => {
            return validation_error(
                "collection_type",
                format!("{label} has unsupported collection type."),
            );
        }
    };
    let Some(id) = id.filter(|id| *id > 0) else {
        return validation_error("reference", format!("{label} must have valid item"));
    };
    if !valid.iter().any(|item| item.id == id) {
        return validation_error("reference", format!("{label} item does not exist."));
    }
    Ok(())
}

fn validate_deco_ids(
    ids: &[EntityId],
    options: &[ersatzrs_core::repositories::DecoOptionAdmin],
    field: &'static str,
    label: &str,
) -> crate::Result<()> {
    for id in ids.iter().copied().filter(|id| *id > 0) {
        if !options.iter().any(|option| option.id == id) {
            return validation_error(field, format!("{label} {id} does not exist."));
        }
    }
    Ok(())
}

fn validate_deco_break_placements(items: &[DecoBreakContentAdmin]) -> crate::Result<()> {
    for (placement, message) in [
        (
            DecoBreakPlacement::BlockStart,
            "Deco may only contain one [Block Start] break content",
        ),
        (
            DecoBreakPlacement::BetweenBlockItems,
            "Deco may only contain one [Between Block Items] break content",
        ),
        (
            DecoBreakPlacement::ChapterMarkers,
            "Deco may only contain one [At Chapter Markers] break content",
        ),
        (
            DecoBreakPlacement::BlockFinish,
            "Deco may only contain one [Block Finish] break content",
        ),
    ] {
        if items
            .iter()
            .filter(|item| item.placement == placement)
            .count()
            > 1
        {
            return validation_error("break_content", message);
        }
    }
    Ok(())
}

fn normalize_deco_admin_form(form: &mut DecoAdminForm) {
    form.watermark_ids.sort_unstable();
    form.watermark_ids.dedup();
    form.graphics_element_ids.sort_unstable();
    form.graphics_element_ids.dedup();
    if !matches!(form.watermark_mode, 2 | 3) {
        form.watermark_ids.clear();
        form.use_watermark_during_filler = false;
    } else if form.watermark_mode != 2 {
        form.use_watermark_during_filler = false;
    }
    if !matches!(form.graphics_elements_mode, 2 | 3) {
        form.graphics_element_ids.clear();
        form.use_graphics_elements_during_filler = false;
    } else if form.graphics_elements_mode != 2 {
        form.use_graphics_elements_during_filler = false;
    }
    if form.break_content_mode != 2 {
        form.break_content.clear();
    }
    normalize_deco_collection_fields(
        form.default_filler_mode,
        form.default_filler_collection_type,
        &mut form.default_filler_collection_id,
        &mut form.default_filler_media_item_id,
        &mut form.default_filler_multi_collection_id,
        &mut form.default_filler_smart_collection_id,
    );
    normalize_deco_collection_fields(
        form.dead_air_fallback_mode,
        form.dead_air_fallback_collection_type,
        &mut form.dead_air_fallback_collection_id,
        &mut form.dead_air_fallback_media_item_id,
        &mut form.dead_air_fallback_multi_collection_id,
        &mut form.dead_air_fallback_smart_collection_id,
    );
    for item in &mut form.break_content {
        normalize_deco_break_fields(item);
    }
}

fn normalize_deco_collection_fields(
    mode: i32,
    collection_type: i32,
    collection_id: &mut Option<EntityId>,
    media_item_id: &mut Option<EntityId>,
    multi_collection_id: &mut Option<EntityId>,
    smart_collection_id: &mut Option<EntityId>,
) {
    if mode != 2 {
        *collection_id = None;
        *media_item_id = None;
        *multi_collection_id = None;
        *smart_collection_id = None;
        return;
    }
    match collection_type {
        0 => {
            *media_item_id = None;
            *multi_collection_id = None;
            *smart_collection_id = None;
        }
        4 => {
            *collection_id = None;
            *media_item_id = None;
            *smart_collection_id = None;
        }
        5 => {
            *collection_id = None;
            *media_item_id = None;
            *multi_collection_id = None;
        }
        _ => {
            *collection_id = None;
            *multi_collection_id = None;
            *smart_collection_id = None;
        }
    }
}

fn normalize_deco_break_fields(item: &mut DecoBreakContentAdminForm) {
    match item.collection_type {
        0 => {
            item.media_item_id = None;
            item.multi_collection_id = None;
            item.smart_collection_id = None;
            item.playlist_id = None;
        }
        4 => {
            item.collection_id = None;
            item.media_item_id = None;
            item.smart_collection_id = None;
            item.playlist_id = None;
        }
        5 => {
            item.collection_id = None;
            item.media_item_id = None;
            item.multi_collection_id = None;
            item.playlist_id = None;
        }
        6 => {
            item.collection_id = None;
            item.media_item_id = None;
            item.multi_collection_id = None;
            item.smart_collection_id = None;
        }
        _ => {
            item.collection_id = None;
            item.multi_collection_id = None;
            item.smart_collection_id = None;
            item.playlist_id = None;
        }
    }
}

fn validate_deco_group_name(name: &str) -> crate::Result<&str> {
    validate_named_value(
        name,
        "Deco group",
        "Deco group name must be 50 characters or fewer",
    )
}

fn validate_deco_name(name: &str) -> crate::Result<&str> {
    validate_named_value(
        name,
        "Deco",
        &format!("Deco name \"{}\" is invalid", name.trim()),
    )
}

fn validate_deco_template_group_name(name: &str) -> crate::Result<&str> {
    validate_named_value(
        name,
        "Deco template group",
        "Deco template group name must be 50 characters or fewer",
    )
}

fn validate_deco_template_name(name: &str) -> crate::Result<&str> {
    validate_named_value(
        name,
        "Deco template",
        &format!("Deco template name \"{}\" is invalid", name.trim()),
    )
}

fn validate_named_value<'a>(
    name: &'a str,
    label: &str,
    max_message: &str,
) -> crate::Result<&'a str> {
    let name = name.trim();
    if name.is_empty() {
        return validation_error("name", format!("{label} name is required"));
    }
    if name.chars().count() > 50 {
        return validation_error("name", max_message);
    }
    Ok(name)
}

async fn validate_unique_deco_name(
    repository: &dyn DecoRepository,
    group_id: EntityId,
    deco_id: Option<EntityId>,
    name: &str,
) -> crate::Result<()> {
    if repository
        .deco_name_exists_admin(group_id, deco_id, name)
        .await?
    {
        return validation_error(
            "name",
            format!("A deco named \"{name}\" already exists in that deco group"),
        );
    }
    Ok(())
}

async fn validate_unique_deco_template_name(
    repository: &dyn DecoRepository,
    group_id: EntityId,
    template_id: Option<EntityId>,
    name: &str,
) -> crate::Result<()> {
    if repository
        .template_name_exists_admin(group_id, template_id, name)
        .await?
    {
        return validation_error(
            "name",
            format!("A deco template named \"{name}\" already exists in that deco template group"),
        );
    }
    Ok(())
}

fn parse_deco_mode(value: i32, field: &'static str) -> crate::Result<DecoMode> {
    match value {
        0 => Ok(DecoMode::Inherit),
        1 => Ok(DecoMode::Disable),
        2 => Ok(DecoMode::Override),
        3 => Ok(DecoMode::Merge),
        _ => validation_error(field, format!("unsupported value {value}")),
    }
}

fn parse_deco_collection_type(value: i32) -> crate::Result<CollectionType> {
    match value {
        0 => Ok(CollectionType::Collection),
        1 => Ok(CollectionType::TelevisionShow),
        2 => Ok(CollectionType::TelevisionSeason),
        3 => Ok(CollectionType::Artist),
        4 => Ok(CollectionType::MultiCollection),
        5 => Ok(CollectionType::SmartCollection),
        6 => Ok(CollectionType::Playlist),
        _ => validation_error("collection_type", format!("unsupported value {value}")),
    }
}

fn parse_deco_placement(value: i32) -> crate::Result<DecoBreakPlacement> {
    match value {
        0 => Ok(DecoBreakPlacement::BlockStart),
        1 => Ok(DecoBreakPlacement::BlockFinish),
        2 => Ok(DecoBreakPlacement::BetweenBlockItems),
        3 => Ok(DecoBreakPlacement::ChapterMarkers),
        _ => validation_error("placement", format!("unsupported value {value}")),
    }
}

pub async fn save_collection(
    command: SaveCollection,
    repository: &dyn CollectionRepository,
) -> crate::Result<EntityId> {
    if command.id < 0 {
        return validation_error("collection_id", "must be zero or greater");
    }
    let name = validate_collection_name(&command.name)?;
    if repository.admin_name_exists(command.id, name).await? {
        return validation_error("name", "Collection name must be unique");
    }
    repository
        .save_admin(command.id, name)
        .await
        .map_err(Into::into)
}

pub async fn add_media_item_to_collection(
    command: AddMediaItemToCollection,
    repository: &dyn CollectionRepository,
    queue: &dyn PlayoutBuildQueue,
    operations: &OperationCoordinator,
) -> crate::Result<EntityId> {
    validate_positive_id(command.media_item_id, "media_item_id")?;
    if !repository
        .media_items_exist(&[command.media_item_id])
        .await?
    {
        return validation_error("media_item_id", "media item does not exist");
    }

    if let Some(collection_id) = command.collection_id.filter(|id| *id > 0) {
        if repository.get_admin(collection_id).await?.is_none() {
            return validation_error("collection_id", "collection does not exist");
        }
        repository
            .add_default_item(collection_id, command.media_item_id)
            .await?;
        request_collection_playout_builds(collection_id, repository, queue, operations).await?;
        return Ok(collection_id);
    }

    let name = validate_collection_name(command.new_name.as_deref().unwrap_or_default())?;
    if repository.admin_name_exists(0, name).await? {
        return validation_error("name", "Collection name must be unique");
    }
    repository
        .create_with_default_item_admin(name, command.media_item_id)
        .await
        .map_err(|error| match error {
            RepositoryError::NotFound {
                kind: "media item", ..
            } => ApplicationError::Validation {
                field: "media_item_id",
                message: "media item does not exist".to_owned(),
            },
            other => other.into(),
        })
}

pub async fn delete_collection(
    command: DeleteCollection,
    repository: &dyn CollectionRepository,
) -> crate::Result<()> {
    validate_positive_id(command.id, "collection_id")?;
    repository
        .delete_admin(command.id)
        .await
        .map_err(Into::into)
}

pub async fn save_smart_collection(
    command: SaveSmartCollection,
    repository: &dyn CollectionRepository,
    queue: &dyn PlayoutBuildQueue,
    operations: &OperationCoordinator,
) -> crate::Result<EntityId> {
    if command.id < 0 {
        return validation_error("smart_collection_id", "must be zero or greater");
    }
    let name = command.name.trim();
    if name.is_empty() {
        return validation_error("name", "Smart collection name is required");
    }
    if name.chars().count() > 50 {
        return validation_error(
            "name",
            "Smart collection name must be 50 characters or less",
        );
    }
    if repository.smart_admin_name_exists(command.id, name).await? {
        return validation_error("name", "SmartCollection name must be unique");
    }
    let query = command.query.trim();
    if query.is_empty() {
        return validation_error("query", "Smart collection query is required");
    }
    ersatzrs_core::search::SearchExpression::parse_validated_user_query(query).map_err(
        |error| ApplicationError::Validation {
            field: "query",
            message: error.to_string(),
        },
    )?;
    let id = repository.save_smart_admin(command.id, name, query).await?;
    if command.id > 0 {
        for playout_id in repository.playout_ids_using_smart(command.id).await? {
            require_playout_build(
                RequestPlayoutBuild::new(playout_id, PlayoutBuildMode::Refresh),
                queue,
                operations,
            )?;
        }
    }
    Ok(id)
}

pub async fn delete_smart_collection(
    command: DeleteSmartCollection,
    repository: &dyn CollectionRepository,
) -> crate::Result<()> {
    validate_positive_id(command.id, "smart_collection_id")?;
    repository
        .delete_smart_admin(command.id)
        .await
        .map_err(Into::into)
}

pub async fn save_saved_search(
    command: SaveSavedSearch,
    repository: &dyn SavedSearchRepository,
) -> crate::Result<EntityId> {
    if command.id < 0 {
        return validation_error("saved_search_id", "must be zero or greater");
    }
    let name = command.name.trim();
    if name.is_empty() {
        return validation_error("name", "Saved search name is required");
    }
    if name.chars().count() > 80 {
        return validation_error("name", "Saved search name must be 80 characters or less");
    }
    if repository.name_exists(command.id, name).await? {
        return validation_error("name", "Saved search name must be unique");
    }
    let query = command.query.trim();
    ersatzrs_core::search::SearchExpression::parse_validated_user_query(query).map_err(
        |error| ApplicationError::Validation {
            field: "query",
            message: error.to_string(),
        },
    )?;
    repository
        .save_admin(command.id, name, query)
        .await
        .map_err(Into::into)
}

pub async fn delete_saved_search(
    command: DeleteSavedSearch,
    repository: &dyn SavedSearchRepository,
) -> crate::Result<()> {
    validate_positive_id(command.id, "saved_search_id")?;
    repository
        .delete_admin(command.id)
        .await
        .map_err(Into::into)
}

pub async fn save_multi_collection(
    command: SaveMultiCollection,
    repository: &dyn CollectionRepository,
    queue: &dyn PlayoutBuildQueue,
    operations: &OperationCoordinator,
) -> crate::Result<EntityId> {
    if command.id < 0 {
        return validation_error("multi_collection_id", "must be zero or greater");
    }
    let name = command.name.trim();
    if name.is_empty() {
        return validation_error("name", "Multi-collection name is required!");
    }
    if name.chars().count() > 50 {
        return validation_error(
            "name",
            "Multi-collection name cannot be longer than 50 characters.",
        );
    }
    if repository.multi_admin_name_exists(command.id, name).await? {
        return validation_error("name", "MultiCollection name must be unique");
    }
    validate_multi_collection_items(&command.items, repository).await?;

    let mut items = command.items;
    normalize_multi_collection_items(&mut items);
    let items_changed = if command.id == 0 {
        true
    } else {
        let Some(existing) = repository.get_multi_admin(command.id).await? else {
            return Err(ApplicationError::NotFound {
                kind: "multi collection",
                id: command.id,
            });
        };
        let mut existing_items = existing.items;
        normalize_multi_collection_items(&mut existing_items);
        existing_items != items
    };
    let id = repository
        .save_multi_admin(command.id, name, &items, items_changed)
        .await?;
    if command.id > 0 && items_changed {
        for playout_id in repository.playout_ids_using_multi(id).await? {
            require_playout_build(
                RequestPlayoutBuild::new(playout_id, PlayoutBuildMode::Refresh),
                queue,
                operations,
            )?;
        }
    }
    Ok(id)
}

pub async fn delete_multi_collection(
    command: DeleteMultiCollection,
    repository: &dyn CollectionRepository,
) -> crate::Result<()> {
    validate_positive_id(command.id, "multi_collection_id")?;
    repository
        .delete_multi_admin(command.id)
        .await
        .map_err(Into::into)
}

pub async fn save_rerun_collection(
    command: SaveRerunCollection,
    repository: &dyn CollectionRepository,
    queue: &dyn PlayoutBuildQueue,
    operations: &OperationCoordinator,
) -> crate::Result<EntityId> {
    let form = command.form;
    if form.id < 0 {
        return validation_error("rerun_collection_id", "must be zero or greater");
    }
    let existing = if form.id == 0 {
        None
    } else {
        Some(
            repository
                .get_rerun_admin(form.id)
                .await?
                .ok_or(ApplicationError::NotFound {
                    kind: "rerun collection",
                    id: form.id,
                })?,
        )
    };
    if form.name.trim().is_empty() {
        return validation_error("name", "[Name] is an empty string");
    }
    if form.name.chars().count() > 50 {
        return validation_error("name", "[Name] must not be longer than 50");
    }
    if repository
        .rerun_admin_name_exists(form.id, &form.name)
        .await?
    {
        return validation_error("name", "Rerun collection name must be unique");
    }
    validate_rerun_collection_target(&form, repository).await?;
    let changed = existing.as_ref().is_some_and(|existing| existing != &form);
    let id = repository.save_rerun_admin(&form).await?;
    if changed {
        for playout_id in repository.playout_ids_using_rerun(id).await? {
            require_playout_build(
                RequestPlayoutBuild::new(playout_id, PlayoutBuildMode::Refresh),
                queue,
                operations,
            )?;
        }
    }
    Ok(id)
}

pub async fn delete_rerun_collection(
    command: DeleteRerunCollection,
    repository: &dyn CollectionRepository,
) -> crate::Result<()> {
    validate_positive_id(command.id, "rerun_collection_id")?;
    repository
        .delete_rerun_admin(command.id)
        .await
        .map_err(Into::into)
}

pub async fn create_playlist_group(
    command: CreatePlaylistGroup,
    repository: &dyn CollectionRepository,
) -> crate::Result<ersatzrs_core::repositories::PlaylistGroupAdminRow> {
    let name = validate_playlist_group_name(&command.name)?;
    repository
        .create_playlist_group_admin(&name)
        .await
        .map_err(Into::into)
}

pub async fn create_playlist(
    command: CreatePlaylist,
    repository: &dyn CollectionRepository,
) -> crate::Result<ersatzrs_core::repositories::PlaylistAdminRow> {
    validate_positive_id(command.playlist_group_id, "playlist_group_id")?;
    let name = validate_playlist_name(&command.name)?;
    if repository
        .get_playlist_group_admin(command.playlist_group_id)
        .await?
        .is_none()
    {
        return validation_error("playlist_group_id", "[PlaylistGroupId] does not exist.");
    }
    if repository
        .playlist_name_exists(command.playlist_group_id, &name)
        .await?
    {
        return validation_error(
            "name",
            format!("A playlist named \"{name}\" already exists in that playlist group"),
        );
    }
    repository
        .create_playlist_admin(command.playlist_group_id, &name)
        .await
        .map_err(Into::into)
}

pub async fn delete_playlist_group(
    command: DeletePlaylistGroup,
    repository: &dyn CollectionRepository,
) -> crate::Result<()> {
    validate_positive_id(command.id, "playlist_group_id")?;
    let group = repository
        .get_playlist_group_admin(command.id)
        .await?
        .ok_or(ApplicationError::NotFound {
            kind: "playlist group",
            id: command.id,
        })?;
    if group.is_system {
        return validation_error("playlist_group_id", "Cannot delete system playlist group");
    }
    repository
        .delete_playlist_group_admin(command.id)
        .await
        .map_err(Into::into)
}

pub async fn delete_playlist(
    command: DeletePlaylist,
    repository: &dyn CollectionRepository,
) -> crate::Result<()> {
    validate_positive_id(command.id, "playlist_id")?;
    let playlist = repository
        .get_playlist_editor_admin(command.id)
        .await?
        .ok_or(ApplicationError::NotFound {
            kind: "playlist",
            id: command.id,
        })?;
    if playlist.is_system {
        return validation_error("playlist_id", "Cannot delete system (generated) playlist");
    }
    repository
        .delete_playlist_admin(command.id)
        .await
        .map_err(Into::into)
}

pub async fn save_playlist(
    command: SavePlaylist,
    repository: &dyn CollectionRepository,
) -> crate::Result<()> {
    let mut form = command.form;
    validate_positive_id(form.id, "playlist_id")?;
    let persisted =
        repository
            .get_playlist_editor_admin(form.id)
            .await?
            .ok_or(ApplicationError::NotFound {
                kind: "playlist",
                id: form.id,
            })?;
    if persisted.is_system {
        return validation_error("playlist_id", "Cannot save system (generated) playlist");
    }
    form.name = validate_playlist_name(&form.name)?;
    validate_playlist_items(&form)?;
    repository
        .save_playlist_admin(&form)
        .await
        .map_err(Into::into)
}

pub async fn add_collection_items(
    mut command: AddCollectionItems,
    repository: &dyn CollectionRepository,
    queue: &dyn PlayoutBuildQueue,
    operations: &OperationCoordinator,
) -> crate::Result<u64> {
    validate_positive_id(command.collection_id, "collection_id")?;
    if command.media_item_ids.is_empty() {
        return Ok(0);
    }
    if repository.get_admin(command.collection_id).await?.is_none() {
        return Err(ApplicationError::NotFound {
            kind: "collection",
            id: command.collection_id,
        });
    }
    validate_entity_ids(&command.media_item_ids, "media_item_id")?;
    if !repository
        .media_items_exist(&command.media_item_ids)
        .await?
    {
        return validation_error("media_item_id", "one or more media items do not exist");
    }
    command.playback = normalize_playback_overrides(command.playback);
    let added = repository
        .add_collection_items_admin(
            command.collection_id,
            &command.media_item_ids,
            &command.playback,
        )
        .await?;
    if added > 0 {
        for playout_id in repository.playout_ids_using(command.collection_id).await? {
            require_playout_build(
                RequestPlayoutBuild::new(playout_id, PlayoutBuildMode::Refresh),
                queue,
                operations,
            )?;
        }
    }
    Ok(added)
}

pub async fn add_playlist_items(
    mut command: AddPlaylistItems,
    repository: &dyn CollectionRepository,
) -> crate::Result<u64> {
    validate_positive_id(command.playlist_id, "playlist_id")?;
    if command.items.is_empty() {
        return Ok(0);
    }
    if repository
        .get_playlist_editor_admin(command.playlist_id)
        .await?
        .is_none()
    {
        return Err(ApplicationError::NotFound {
            kind: "playlist",
            id: command.playlist_id,
        });
    }
    let media_item_ids = command
        .items
        .iter()
        .map(|item| item.media_item_id)
        .collect::<Vec<_>>();
    validate_entity_ids(&media_item_ids, "media_item_id")?;
    if !repository.media_items_exist(&media_item_ids).await? {
        return validation_error("media_item_id", "one or more media items do not exist");
    }
    for item in &mut command.items {
        if !matches!(
            item.collection_type,
            1 | 2 | 3 | 10 | 20 | 30 | 40 | 50 | 60 | 70
        ) {
            return validation_error("collection_type", "collection type is invalid");
        }
        item.playback = normalize_playback_overrides(std::mem::take(&mut item.playback));
        if let Some(selection) = item.pinned_stream_selection {
            validate_positive_id(selection.media_version_id, "media_version_id")?;
            if selection.video_stream_index < 0
                || selection.audio_stream_index.is_some_and(|index| index < 0)
                || selection
                    .subtitle_stream_index
                    .is_some_and(|index| index < 0)
            {
                return validation_error("stream_index", "stream index is invalid");
            }
        }
    }
    repository
        .add_playlist_items_admin(command.playlist_id, &command.items)
        .await
        .map_err(Into::into)
}

pub async fn set_collection_custom_order(
    command: SetCollectionCustomOrder,
    repository: &dyn CollectionRepository,
    queue: &dyn PlayoutBuildQueue,
    operations: &OperationCoordinator,
) -> crate::Result<()> {
    validate_positive_id(command.collection_id, "collection_id")?;
    if repository
        .set_collection_custom_order_enabled(command.collection_id, command.enabled)
        .await?
    {
        refresh_collection_playouts(command.collection_id, repository, queue, operations).await?;
    }
    Ok(())
}

pub async fn remove_collection_rows(
    command: RemoveCollectionRows,
    repository: &dyn CollectionRepository,
    queue: &dyn PlayoutBuildQueue,
    operations: &OperationCoordinator,
) -> crate::Result<()> {
    validate_positive_id(command.collection_id, "collection_id")?;
    if command.row_ids.is_empty() {
        return Ok(());
    }
    validate_entity_ids(&command.row_ids, "collection_item_id")?;
    if repository
        .remove_collection_rows(command.collection_id, &command.row_ids)
        .await?
        > 0
    {
        refresh_collection_playouts(command.collection_id, repository, queue, operations).await?;
    }
    Ok(())
}

pub async fn update_collection_custom_order(
    command: UpdateCollectionCustomOrder,
    repository: &dyn CollectionRepository,
    queue: &dyn PlayoutBuildQueue,
    operations: &OperationCoordinator,
) -> crate::Result<()> {
    validate_positive_id(command.collection_id, "collection_id")?;
    if command.ordered_row_ids.is_empty() {
        return Ok(());
    }
    validate_entity_ids(&command.ordered_row_ids, "collection_item_id")?;
    let unique = command
        .ordered_row_ids
        .iter()
        .copied()
        .collect::<std::collections::BTreeSet<_>>();
    if unique.len() != command.ordered_row_ids.len() {
        return validation_error("collection_item_id", "collection item ids must be unique");
    }
    if repository
        .update_collection_custom_order(command.collection_id, &command.ordered_row_ids)
        .await?
        > 0
    {
        refresh_collection_playouts(command.collection_id, repository, queue, operations).await?;
    }
    Ok(())
}

fn normalize_playback_overrides(mut playback: PlaybackOverrides) -> PlaybackOverrides {
    playback.preferred_audio_language_code =
        normalize_optional_string(playback.preferred_audio_language_code);
    playback.preferred_audio_title = normalize_optional_string(playback.preferred_audio_title);
    playback.preferred_subtitle_language_code =
        normalize_optional_string(playback.preferred_subtitle_language_code);
    playback.subtitle_mode = playback.subtitle_mode.filter(|mode| matches!(mode, 1..=3));
    playback
}

fn normalize_optional_string(value: Option<String>) -> Option<String> {
    value
        .map(|value| value.trim().to_owned())
        .filter(|value| !value.is_empty())
}

fn validate_entity_ids(ids: &[EntityId], field: &'static str) -> crate::Result<()> {
    if ids.iter().any(|id| *id <= 0) {
        return validation_error(field, "must be greater than zero");
    }
    Ok(())
}

fn validate_collection_name(value: &str) -> crate::Result<&str> {
    let name = value.trim();
    if name.is_empty() {
        return validation_error("name", "Collection name is required");
    }
    if name.chars().count() > 50 {
        return validation_error("name", "Collection name must be 50 characters or less");
    }
    Ok(name)
}

async fn refresh_collection_playouts(
    collection_id: EntityId,
    repository: &dyn CollectionRepository,
    queue: &dyn PlayoutBuildQueue,
    operations: &OperationCoordinator,
) -> crate::Result<()> {
    for playout_id in repository.playout_ids_using(collection_id).await? {
        require_playout_build(
            RequestPlayoutBuild::new(playout_id, PlayoutBuildMode::Refresh),
            queue,
            operations,
        )?;
    }
    Ok(())
}

pub async fn save_schedule(
    mut command: SaveSchedule,
    repository: &dyn ProgramScheduleRepository,
    queue: &dyn PlayoutBuildQueue,
    operations: &OperationCoordinator,
) -> crate::Result<EntityId> {
    if command.form.id < 0 {
        return validation_error("schedule_id", "must be zero or greater");
    }
    command.form.name = validate_schedule_name(&command.form.name, "Schedule name is required!")?;
    if repository
        .admin_name_exists(command.form.id, &command.form.name)
        .await?
    {
        return validation_error("name", "Schedule name must be unique");
    }
    command.form.treat_collections_as_shows &= command.form.keep_multi_part_episodes_together;
    if !matches!(command.form.fixed_start_time_behavior, 0..=2) {
        return validation_error("fixed_start_time_behavior", "value is invalid");
    }
    let previous = if command.form.id == 0 {
        None
    } else {
        repository.get_admin(command.form.id).await?
    };
    if command.form.id != 0 && previous.is_none() {
        return Err(ApplicationError::NotFound {
            kind: "program schedule",
            id: command.form.id,
        });
    }
    let id = repository.save_admin(&command.form).await?;
    if previous
        .as_ref()
        .is_some_and(|previous| schedule_runtime_fields_changed(previous, &command.form))
    {
        refresh_schedule_playouts(id, repository, queue, operations).await?;
    }
    Ok(id)
}

pub async fn delete_schedule(
    command: DeleteSchedule,
    repository: &dyn ProgramScheduleRepository,
) -> crate::Result<()> {
    validate_positive_id(command.id, "schedule_id")?;
    if repository.get_admin(command.id).await?.is_none() {
        return Err(ApplicationError::NotFound {
            kind: "program schedule",
            id: command.id,
        });
    }
    repository
        .delete_admin(command.id)
        .await
        .map_err(Into::into)
}

pub async fn copy_schedule(
    command: CopySchedule,
    repository: &dyn ProgramScheduleRepository,
) -> crate::Result<EntityId> {
    validate_positive_id(command.id, "schedule_id")?;
    let name = validate_schedule_name(&command.name, "New Schedule Name is required.")?;
    if repository.admin_name_exists(0, &name).await? {
        return validation_error("name", "Schedule name must be unique");
    }
    if repository.get_admin(command.id).await?.is_none() {
        return Err(ApplicationError::NotFound {
            kind: "program schedule",
            id: command.id,
        });
    }
    repository
        .copy_admin(command.id, &name)
        .await
        .map_err(Into::into)
}

pub async fn replace_schedule_items(
    command: ReplaceScheduleItems,
    repository: &dyn ProgramScheduleRepository,
    queue: &dyn PlayoutBuildQueue,
    operations: &OperationCoordinator,
) -> crate::Result<()> {
    validate_positive_id(command.schedule_id, "schedule_id")?;
    if repository.get_admin(command.schedule_id).await?.is_none() {
        return Err(ApplicationError::NotFound {
            kind: "program schedule",
            id: command.schedule_id,
        });
    }

    let options = repository.get_item_options_admin().await?;
    let padded_filler_ids = options
        .fillers
        .into_iter()
        .filter(|filler| filler.pad_to_nearest_minute)
        .map(|filler| filler.id)
        .collect::<std::collections::BTreeSet<_>>();
    let mut items = command.items;
    for item in &mut items {
        normalize_schedule_item(item);
    }
    validate_schedule_items(&items, &padded_filler_ids)?;
    items.sort_by_key(|item| (item.index, item.id));
    repository
        .replace_items_admin(command.schedule_id, &items)
        .await?;
    refresh_schedule_playouts(command.schedule_id, repository, queue, operations).await
}

pub async fn append_schedule_media_item(
    command: AppendScheduleMediaItem,
    schedules: &dyn ProgramScheduleRepository,
    media_items: &dyn MediaItemRepository,
    queue: &dyn PlayoutBuildQueue,
    operations: &OperationCoordinator,
) -> crate::Result<()> {
    validate_positive_id(command.schedule_id, "schedule_id")?;
    validate_positive_id(command.media_item_id, "media_item_id")?;
    if !matches!(
        command.collection_type,
        1 | 2 | 3 | 10 | 20 | 30 | 40 | 50 | 60 | 70
    ) {
        return validation_error("collection_type", "collection type is invalid");
    }
    if schedules.get_admin(command.schedule_id).await?.is_none() {
        return Err(ApplicationError::NotFound {
            kind: "program schedule",
            id: command.schedule_id,
        });
    }
    media_items
        .get_with_version(command.media_item_id)
        .await
        .map_err(|error| match error {
            RepositoryError::NotFound { .. } => ApplicationError::NotFound {
                kind: "media item",
                id: command.media_item_id,
            },
            error => error.into(),
        })?;
    schedules
        .append_one_item_admin(
            command.schedule_id,
            command.collection_type,
            command.media_item_id,
            command.pinned_stream_selection,
        )
        .await?;
    refresh_schedule_playouts(command.schedule_id, schedules, queue, operations).await
}

fn validate_schedule_items(
    items: &[ScheduleItemAdmin],
    padded_filler_ids: &std::collections::BTreeSet<EntityId>,
) -> crate::Result<()> {
    let mut playback_by_key = std::collections::BTreeMap::new();
    for item in items {
        validate_schedule_item(item, padded_filler_ids)?;
        validate_schedule_smart_shuffle(&item.smart_shuffle)?;
        if let Some(key) = schedule_item_collection_key(item) {
            if let Some(existing) = playback_by_key.insert(key, item.playback_order) {
                if existing != item.playback_order {
                    return validation_error(
                        "playback_order",
                        "A collection must use the same playback order in every schedule item.",
                    );
                }
            }
        }
        if item.playback_order == 4 && item.fill_with_group_mode != 0 {
            return validation_error(
                "fill_with_group_mode",
                "Shuffle In Order cannot be used with Fill With Group Mode.",
            );
        }
    }
    Ok(())
}

#[allow(clippy::too_many_lines)]
fn validate_schedule_item(
    item: &ScheduleItemAdmin,
    padded_filler_ids: &std::collections::BTreeSet<EntityId>,
) -> crate::Result<()> {
    const COLLECTION: i32 = 0;
    const SHOW: i32 = 1;
    const SEASON: i32 = 2;
    const ARTIST: i32 = 3;
    const MULTI_COLLECTION: i32 = 4;
    const SMART_COLLECTION: i32 = 5;
    const PLAYLIST: i32 = 6;
    const RERUN_FIRST_RUN: i32 = 7;
    const RERUN_RERUN: i32 = 8;
    const SEARCH_QUERY: i32 = 9;
    const FIXED_START: i32 = 2;
    const PLAYOUT_MULTIPLE: i32 = 3;
    const PLAYOUT_DURATION: i32 = 4;
    const MULTIPLE_COUNT: i32 = 0;
    const PLAYLIST_ITEM_SIZE: i32 = 2;
    const TAIL_FILLER: i32 = 3;

    if item.start_type == FIXED_START && item.start_time.as_deref().unwrap_or_default().is_empty() {
        return validation_error("start_time", "Fixed schedule items require a start time.");
    }
    if item.collection_type == MULTI_COLLECTION && matches!(item.playback_order, 1 | 2 | 5 | 6 | 7)
    {
        return validation_error(
            "playback_order",
            "Multi Collection items can only use Shuffle or Shuffle In Order.",
        );
    }
    if item.multiple_mode == PLAYLIST_ITEM_SIZE && item.collection_type != PLAYLIST {
        return validation_error(
            "multiple_mode",
            "Playlist Item Size can only be used with Playlist schedule items.",
        );
    }
    if item.playout_mode == PLAYOUT_MULTIPLE
        && item.multiple_mode == MULTIPLE_COUNT
        && item.multiple_count.trim().is_empty()
    {
        return validation_error(
            "multiple_count",
            "Multiple playout mode with Count requires a count expression.",
        );
    }
    if item.playout_mode == PLAYOUT_DURATION {
        let minutes = item.duration_hours * 60 + item.duration_minutes;
        if !(1..=24 * 60).contains(&minutes) {
            return validation_error(
                "duration",
                "Duration playout mode requires a duration between 1 minute and 24 hours.",
            );
        }
        if item.discard_to_fill_attempts.unwrap_or(-1) < 0 {
            return validation_error(
                "discard_to_fill_attempts",
                "Duration playout mode requires discard-to-fill attempts.",
            );
        }
        if item.tail_mode == TAIL_FILLER && item.tail_filler_id.is_none() {
            return validation_error(
                "tail_filler_id",
                "Tail Mode Filler requires a tail filler preset.",
            );
        }
    }
    if item.tail_mode != TAIL_FILLER && item.tail_filler_id.is_some() {
        return validation_error(
            "tail_filler_id",
            "Tail filler can only be set when Tail Mode is Filler.",
        );
    }

    match item.collection_type {
        COLLECTION if item.collection_id.is_none() => {
            return validation_error("collection_id", "Collection is required.");
        }
        SHOW | SEASON | ARTIST if item.media_item_id.is_none() => {
            return validation_error("media_item_id", "Media item is required.");
        }
        MULTI_COLLECTION if item.multi_collection_id.is_none() => {
            return validation_error("multi_collection_id", "Multi Collection is required.");
        }
        SMART_COLLECTION if item.smart_collection_id.is_none() => {
            return validation_error("smart_collection_id", "Smart Collection is required.");
        }
        PLAYLIST if item.playlist_id.is_none() => {
            return validation_error("playlist_id", "Playlist is required.");
        }
        RERUN_FIRST_RUN | RERUN_RERUN if item.rerun_collection_id.is_none() => {
            return validation_error("rerun_collection_id", "Rerun Collection is required.");
        }
        SEARCH_QUERY
            if item
                .search_query
                .as_deref()
                .unwrap_or_default()
                .trim()
                .is_empty() =>
        {
            return validation_error("search_query", "Search Query is required.");
        }
        _ => {}
    }

    let padded_count = [
        item.pre_roll_filler_id,
        item.mid_roll_filler_id,
        item.post_roll_filler_id,
    ]
    .into_iter()
    .flatten()
    .filter(|id| padded_filler_ids.contains(id))
    .count();
    if padded_count > 1 {
        return validation_error(
            "filler",
            "Only one pre-, mid-, or post-roll filler can pad to the nearest minute.",
        );
    }
    if padded_count > 0 && item.fallback_filler_id.is_none() {
        return validation_error(
            "fallback_filler_id",
            "Padding filler requires a fallback filler.",
        );
    }
    Ok(())
}

fn validate_schedule_smart_shuffle(
    policy: &ersatzrs_core::scheduling::SchedulingSmartShufflePolicy,
) -> crate::Result<()> {
    if policy.preset != SchedulingSmartShufflePreset::Custom {
        return Ok(());
    }
    if policy.after_items.unwrap_or_default() == 0 && policy.after_duration_value().is_none() {
        return validation_error(
            "smart_shuffle",
            "Smart Shuffle requires videos-between or duration spacing.",
        );
    }
    if policy.rules.is_empty() {
        return validation_error(
            "smart_shuffle",
            "Smart Shuffle requires at least one similarity rule.",
        );
    }
    Ok(())
}

fn schedule_item_collection_key(item: &ScheduleItemAdmin) -> Option<String> {
    match item.collection_type {
        0 => item.collection_id.map(|id| format!("collection:{id}")),
        1 => item.media_item_id.map(|id| format!("show:{id}")),
        2 => item.media_item_id.map(|id| format!("season:{id}")),
        3 => item.media_item_id.map(|id| format!("artist:{id}")),
        4 => item
            .multi_collection_id
            .map(|id| format!("multi_collection:{id}")),
        5 => item
            .smart_collection_id
            .map(|id| format!("smart_collection:{id}")),
        6 => item.playlist_id.map(|id| format!("playlist:{id}")),
        7 | 8 => item
            .rerun_collection_id
            .map(|id| format!("rerun:{}:{id}", item.collection_type)),
        9 => item
            .search_query
            .as_deref()
            .map(str::trim)
            .filter(|value| !value.is_empty())
            .map(|value| format!("search:{value}")),
        _ => None,
    }
}

fn normalize_schedule_item(item: &mut ScheduleItemAdmin) {
    const COLLECTION: i32 = 0;
    const MULTI_COLLECTION: i32 = 4;
    const SMART_COLLECTION: i32 = 5;
    const PLAYLIST: i32 = 6;
    const RERUN_FIRST_RUN: i32 = 7;
    const RERUN_RERUN: i32 = 8;
    const PLAYBACK_NONE: i32 = 0;
    const PLAYBACK_CHRONOLOGICAL: i32 = 1;
    const PLAYBACK_SHUFFLE: i32 = 3;
    const PLAYBACK_SHUFFLE_IN_ORDER: i32 = 4;
    const PLAYBACK_MARATHON: i32 = 8;
    const MULTIPLE_COUNT: i32 = 0;
    const PLAYLIST_ITEM_SIZE: i32 = 2;
    const MULTI_EPISODE_GROUP_SIZE: i32 = 3;

    if item.collection_type != PLAYLIST && item.multiple_mode == PLAYLIST_ITEM_SIZE {
        item.multiple_mode = MULTIPLE_COUNT;
    }
    if matches!(
        item.collection_type,
        PLAYLIST | RERUN_FIRST_RUN | RERUN_RERUN
    ) {
        item.playback_order = PLAYBACK_NONE;
    }
    if item.collection_type == MULTI_COLLECTION {
        item.playback_order = PLAYBACK_SHUFFLE;
    }
    if item.smart_shuffle.is_active()
        && !matches!(
            item.collection_type,
            PLAYLIST | RERUN_FIRST_RUN | RERUN_RERUN
        )
    {
        item.playback_order = PLAYBACK_SHUFFLE;
    }
    if item.playback_order != PLAYBACK_CHRONOLOGICAL
        && item.multiple_mode == MULTI_EPISODE_GROUP_SIZE
    {
        item.multiple_mode = MULTIPLE_COUNT;
    }
    if item.playback_order != PLAYBACK_MARATHON {
        item.marathon_group_by = 0;
        item.marathon_shuffle_groups = false;
        item.marathon_shuffle_items = false;
        item.marathon_batch_size = None;
    }
    let can_fill_with_groups = matches!(item.playout_mode, 3 | 4)
        && item.playback_order != PLAYBACK_SHUFFLE_IN_ORDER
        && matches!(
            item.collection_type,
            COLLECTION | MULTI_COLLECTION | SMART_COLLECTION
        );
    if !can_fill_with_groups {
        item.fill_with_group_mode = 0;
    }
}

fn validate_schedule_name(value: &str, required_message: &str) -> crate::Result<String> {
    let name = value.trim();
    if name.is_empty() {
        return validation_error("name", required_message);
    }
    if name.chars().count() > 50 {
        return validation_error("name", "Schedule name must be 50 characters or less.");
    }
    Ok(name.to_owned())
}

fn schedule_runtime_fields_changed(
    previous: &ScheduleAdminForm,
    current: &ScheduleAdminForm,
) -> bool {
    previous.keep_multi_part_episodes_together != current.keep_multi_part_episodes_together
        || previous.treat_collections_as_shows != current.treat_collections_as_shows
        || previous.shuffle_schedule_items != current.shuffle_schedule_items
        || previous.random_start_point != current.random_start_point
        || previous.fixed_start_time_behavior != current.fixed_start_time_behavior
}

async fn refresh_schedule_playouts(
    schedule_id: EntityId,
    repository: &dyn ProgramScheduleRepository,
    queue: &dyn PlayoutBuildQueue,
    operations: &OperationCoordinator,
) -> crate::Result<()> {
    for playout_id in repository.playout_ids_using_admin(schedule_id).await? {
        require_playout_build(
            RequestPlayoutBuild::new(playout_id, PlayoutBuildMode::Refresh),
            queue,
            operations,
        )?;
    }
    Ok(())
}

fn validate_playlist_group_name(value: &str) -> crate::Result<String> {
    let name = value.trim();
    if name.is_empty() {
        return validation_error("name", "[Name] must not be empty.");
    }
    if name.chars().count() > 50 {
        return validation_error("name", "[Name] must not be longer than 50.");
    }
    Ok(name.to_owned())
}

fn validate_playlist_name(value: &str) -> crate::Result<String> {
    let name = value.trim();
    if name.is_empty() {
        return validation_error("name", "Playlist name is required");
    }
    if name.chars().count() > 50 {
        return validation_error("name", format!("Playlist name \"{name}\" is invalid"));
    }
    Ok(name.to_owned())
}

fn validate_playlist_items(form: &PlaylistEditorAdmin) -> crate::Result<()> {
    for item in &form.items {
        match item.collection_type {
            0 if item.collection_id.is_none() => {
                return validation_error(
                    "collection",
                    "[Collection] is required for collection type 'Collection'",
                );
            }
            1 | 2 | 3 | 10 | 20 | 30 | 40 | 50 | 60 | 70 if item.media_item_id.is_none() => {
                return validation_error(
                    "media_item",
                    format!(
                        "[MediaItem] is required for type '{}'",
                        playlist_collection_type_name(item.collection_type)
                    ),
                );
            }
            4 if item.multi_collection_id.is_none() => {
                return validation_error(
                    "multi_collection",
                    "[MultiCollection] is required for collection type 'MultiCollection'",
                );
            }
            5 if item.smart_collection_id.is_none() => {
                return validation_error(
                    "smart_collection",
                    "[SmartCollection] is required for collection type 'SmartCollection'",
                );
            }
            0 | 1 | 2 | 3 | 4 | 5 | 10 | 20 | 30 | 40 | 50 | 60 | 70 => {}
            _ => return validation_error("collection_type", "[CollectionType] is invalid"),
        }
    }
    Ok(())
}

fn playlist_collection_type_name(value: i32) -> &'static str {
    match value {
        0 => "Collection",
        1 => "TelevisionShow",
        2 => "TelevisionSeason",
        3 => "Artist",
        4 => "MultiCollection",
        5 => "SmartCollection",
        10 => "Movie",
        20 => "Episode",
        30 => "MusicVideo",
        40 => "OtherVideo",
        50 => "Song",
        60 => "Image",
        70 => "RemoteStream",
        _ => "Unknown",
    }
}

async fn validate_rerun_collection_target(
    form: &RerunCollectionAdmin,
    repository: &dyn CollectionRepository,
) -> crate::Result<()> {
    let reference = match form.collection_type {
        0 => form
            .collection_id
            .map(|id| (RerunCollectionReferenceKind::Collection, id)),
        1 => form
            .media_item_id
            .map(|id| (RerunCollectionReferenceKind::Show, id)),
        2 => form
            .media_item_id
            .map(|id| (RerunCollectionReferenceKind::Season, id)),
        3 => form
            .media_item_id
            .map(|id| (RerunCollectionReferenceKind::Artist, id)),
        4 => form
            .multi_collection_id
            .map(|id| (RerunCollectionReferenceKind::MultiCollection, id)),
        5 => form
            .smart_collection_id
            .map(|id| (RerunCollectionReferenceKind::SmartCollection, id)),
        6 => None,
        _ => return validation_error("collection_type", "Unsupported collection type."),
    };
    if let Some((kind, id)) = reference {
        validate_positive_id(id, "reference_id")?;
        if !repository.rerun_reference_exists(kind, id).await? {
            return validation_error(
                "reference_id",
                format!("Referenced item {id} does not exist."),
            );
        }
    }
    Ok(())
}

async fn validate_multi_collection_items(
    items: &[MultiCollectionItem],
    repository: &dyn CollectionRepository,
) -> crate::Result<()> {
    let mut collection_ids = std::collections::BTreeSet::new();
    let mut smart_collection_ids = std::collections::BTreeSet::new();
    for item in items {
        match (item.collection_id, item.smart_collection_id) {
            (Some(id), None) => {
                validate_positive_id(id, "collection_id")?;
                if !collection_ids.insert(id) {
                    return validation_error(
                        "items",
                        format!("Collection {id} is already present."),
                    );
                }
                if repository.get_admin(id).await?.is_none() {
                    return validation_error("items", format!("Collection {id} does not exist."));
                }
            }
            (None, Some(id)) => {
                validate_positive_id(id, "smart_collection_id")?;
                if !smart_collection_ids.insert(id) {
                    return validation_error(
                        "items",
                        format!("Smart collection {id} is already present."),
                    );
                }
                if repository.get_smart_admin(id).await?.is_none() {
                    return validation_error(
                        "items",
                        format!("Smart collection {id} does not exist."),
                    );
                }
            }
            _ => {
                return validation_error(
                    "items",
                    "Each multi-collection item must reference exactly one collection.",
                );
            }
        }
    }
    Ok(())
}

fn normalize_multi_collection_items(items: &mut [MultiCollectionItem]) {
    for item in items.iter_mut() {
        item.name.clear();
        item.use_custom_playback_order = false;
        item.collection_type = if item.smart_collection_id.is_some() {
            5
        } else {
            0
        };
        if item.playback_order == 0 {
            item.playback_order = 1;
        }
    }
    items.sort_by_key(|item| {
        (
            item.collection_id.unwrap_or_default(),
            item.smart_collection_id.unwrap_or_default(),
            item.schedule_as_group,
            item.playback_order,
        )
    });
}

pub async fn add_default_collection_item(
    command: ChangeCollectionItem,
    repository: &dyn CollectionRepository,
    queue: &dyn PlayoutBuildQueue,
    operations: &OperationCoordinator,
) -> crate::Result<()> {
    add_media_item_to_collection(
        AddMediaItemToCollection {
            media_item_id: command.media_item_id,
            collection_id: Some(command.collection_id),
            new_name: None,
        },
        repository,
        queue,
        operations,
    )
    .await
    .map(|_| ())
}

pub async fn remove_collection_item(
    command: ChangeCollectionItem,
    repository: &dyn CollectionRepository,
    queue: &dyn PlayoutBuildQueue,
    operations: &OperationCoordinator,
) -> crate::Result<()> {
    validate_positive_id(command.collection_id, "collection_id")?;
    validate_positive_id(command.media_item_id, "media_item_id")?;
    repository
        .remove_item(command.collection_id, command.media_item_id)
        .await?;
    request_collection_playout_builds(command.collection_id, repository, queue, operations).await
}

async fn request_collection_playout_builds(
    collection_id: EntityId,
    repository: &dyn CollectionRepository,
    queue: &dyn PlayoutBuildQueue,
    operations: &OperationCoordinator,
) -> crate::Result<()> {
    for playout_id in repository.playout_ids_using(collection_id).await? {
        require_playout_build(
            RequestPlayoutBuild::new(playout_id, PlayoutBuildMode::Refresh),
            queue,
            operations,
        )?;
    }
    Ok(())
}

/// Reload the logging runtime after its settings transaction.
pub async fn reload_settings_logging(runtime: &dyn SettingsRuntime) -> crate::Result<()> {
    runtime
        .reload_logging()
        .await
        .map_err(|error| ApplicationError::Unavailable {
            operation: "reload logging settings",
            message: error.to_string(),
        })
}

/// Validate, atomically persist, and apply logging settings.
pub async fn save_logging_settings(
    command: SaveLoggingSettings,
    repository: &dyn AdminSettingsRepository,
    runtime: &dyn SettingsRuntime,
) -> crate::Result<()> {
    let levels = [
        (
            ersatzrs_domain::CONFIG_KEY_MINIMUM_LOG_LEVEL,
            command.default_minimum_log_level,
        ),
        (
            ersatzrs_domain::CONFIG_KEY_MINIMUM_LOG_LEVEL_SCANNING,
            command.scanning_minimum_log_level,
        ),
        (
            ersatzrs_domain::CONFIG_KEY_MINIMUM_LOG_LEVEL_SCHEDULING,
            command.scheduling_minimum_log_level,
        ),
        (
            ersatzrs_domain::CONFIG_KEY_MINIMUM_LOG_LEVEL_SEARCHING,
            command.searching_minimum_log_level,
        ),
        (
            ersatzrs_domain::CONFIG_KEY_MINIMUM_LOG_LEVEL_STREAMING,
            command.streaming_minimum_log_level,
        ),
        (
            ersatzrs_domain::CONFIG_KEY_MINIMUM_LOG_LEVEL_HTTP,
            command.http_minimum_log_level,
        ),
    ];
    for (_, level) in &levels {
        validate_choice(
            "log_level",
            level,
            &["Debug", "Information", "Warning", "Error"],
        )?;
    }
    repository
        .apply_mutations(
            levels
                .into_iter()
                .map(|(key, value)| setting(key, value))
                .collect(),
        )
        .await?;
    reload_settings_logging(runtime).await
}

/// Validate, atomically persist, and apply HDHR settings.
pub async fn save_hdhr_settings(
    command: SaveHdhrSettings,
    repository: &dyn AdminSettingsRepository,
    runtime: &dyn SettingsRuntime,
) -> crate::Result<()> {
    if command.tuner_count <= 0 {
        return validation_error("tuner_count", "Tuner count must be greater than zero");
    }
    repository
        .apply_mutations(vec![setting(
            ersatzrs_domain::CONFIG_KEY_HDHR_TUNER_COUNT,
            command.tuner_count.to_string(),
        )])
        .await?;
    runtime.set_tuner_count(command.tuner_count);
    Ok(())
}

/// Validate and atomically persist scanner settings.
pub async fn save_scanner_settings(
    command: SaveScannerSettings,
    repository: &dyn AdminSettingsRepository,
) -> crate::Result<()> {
    if command.library_refresh_interval < 0 {
        return validation_error(
            "library_refresh_interval",
            "Library refresh interval must be 0 (do not refresh) or greater than zero",
        );
    }
    if command.library_refresh_interval >= 1_000_000 {
        return validation_error(
            "library_refresh_interval",
            "Library refresh interval must be less than 1,000,000. Use 0 to disable automatic refresh",
        );
    }
    repository
        .apply_mutations(vec![setting(
            ersatzrs_domain::CONFIG_KEY_LIBRARY_REFRESH_INTERVAL,
            command.library_refresh_interval.to_string(),
        )])
        .await
        .map_err(Into::into)
}

/// Validate and atomically persist the global add-on detail pacing window.
pub async fn save_addon_detail_wait_settings(
    command: SaveAddonDetailWaitSettings,
    repository: &dyn AdminSettingsRepository,
) -> crate::Result<()> {
    if command.minimum_seconds <= 0 {
        return validation_error(
            "minimum_seconds",
            "Minimum detail wait must be a positive whole number of seconds",
        );
    }
    if command.maximum_seconds <= 0 {
        return validation_error(
            "maximum_seconds",
            "Maximum detail wait must be a positive whole number of seconds",
        );
    }
    if command.minimum_seconds > command.maximum_seconds {
        return validation_error(
            "minimum_seconds",
            "Minimum detail wait must not exceed maximum detail wait",
        );
    }
    repository
        .apply_mutations(vec![
            setting(
                ersatzrs_domain::CONFIG_KEY_ADDON_DETAIL_WAIT_MINIMUM_SECONDS,
                command.minimum_seconds.to_string(),
            ),
            setting(
                ersatzrs_domain::CONFIG_KEY_ADDON_DETAIL_WAIT_MAXIMUM_SECONDS,
                command.maximum_seconds.to_string(),
            ),
        ])
        .await
        .map_err(Into::into)
}

/// Validate, atomically persist, and fan out playout-setting effects.
pub async fn save_playout_settings(
    command: SavePlayoutSettings,
    repository: &dyn AdminSettingsRepository,
    queue: &dyn PlayoutBuildQueue,
    operations: &OperationCoordinator,
) -> crate::Result<()> {
    if command.days_to_build <= 0 {
        return validation_error(
            "days_to_build",
            "Playout days to build must be greater than zero",
        );
    }
    if command.scripted_schedule_timeout_seconds < 10 {
        return validation_error(
            "scripted_schedule_timeout_seconds",
            "Scripted schedule timeout seconds must be at least 10",
        );
    }
    repository
        .apply_mutations(vec![
            setting(
                ersatzrs_domain::CONFIG_KEY_PLAYOUT_DAYS_TO_BUILD,
                command.days_to_build.to_string(),
            ),
            setting(
                ersatzrs_domain::CONFIG_KEY_PLAYOUT_SKIP_MISSING_ITEMS,
                bool_setting(command.skip_missing_items),
            ),
            setting(
                ersatzrs_domain::CONFIG_KEY_PLAYOUT_SCRIPTED_SCHEDULE_TIMEOUT_SECONDS,
                command.scripted_schedule_timeout_seconds.to_string(),
            ),
        ])
        .await?;
    request_settings_playout_builds(repository, queue, operations, PlayoutBuildMode::Continue).await
}

/// Atomically persist the transport-normalized UI settings.
pub async fn save_ui_settings(
    command: SaveUiSettings,
    repository: &dyn AdminSettingsRepository,
) -> crate::Result<()> {
    if command.language.trim().is_empty() {
        return validation_error("language", "Language is required");
    }
    repository
        .apply_mutations(vec![
            setting(
                ersatzrs_domain::CONFIG_KEY_PAGES_IS_DARK_MODE,
                bool_setting(command.is_dark_mode),
            ),
            setting(ersatzrs_domain::CONFIG_KEY_PAGES_LANGUAGE, command.language),
        ])
        .await
        .map_err(Into::into)
}

/// Apply the current user's Windows startup registration.
pub fn set_windows_startup(enabled: bool, runtime: &dyn SettingsRuntime) -> crate::Result<()> {
    runtime
        .set_windows_startup(enabled)
        .map_err(|error| ApplicationError::Unavailable {
            operation: "update Windows startup registration",
            message: error.to_string(),
        })
}

/// Validate, atomically persist, and fan out XMLTV-setting effects.
pub async fn save_xmltv_settings(
    command: SaveXmltvSettings,
    repository: &dyn AdminSettingsRepository,
    queue: &dyn PlayoutBuildQueue,
    operations: &OperationCoordinator,
) -> crate::Result<()> {
    if command.days_to_build <= 0 {
        return validation_error(
            "days_to_build",
            "XMLTV days to build must be greater than zero",
        );
    }
    let playout_days = match repository
        .get_optional_string(ersatzrs_domain::CONFIG_KEY_PLAYOUT_DAYS_TO_BUILD)
        .await?
    {
        None => 2,
        Some(value) => value
            .parse::<i32>()
            .map_err(|_| ApplicationError::InvalidData {
                kind: "admin setting",
                value: format!(
                    "{}={value}",
                    ersatzrs_domain::CONFIG_KEY_PLAYOUT_DAYS_TO_BUILD
                ),
            })?,
    };
    if playout_days < command.days_to_build {
        return validation_error(
            "days_to_build",
            format!(
                "XMLTV days to build ({}) cannot be greater than Playout days to build ({playout_days})",
                command.days_to_build
            ),
        );
    }
    validate_choice("time_zone", &command.time_zone, &["Local", "Utc"])?;
    validate_choice(
        "block_behavior",
        &command.block_behavior,
        &["SplitTimeEvenly", "UseActualTimes"],
    )?;
    repository
        .apply_mutations(vec![
            setting(
                ersatzrs_domain::CONFIG_KEY_XMLTV_TIME_ZONE,
                command.time_zone,
            ),
            setting(
                ersatzrs_domain::CONFIG_KEY_XMLTV_DAYS_TO_BUILD,
                command.days_to_build.to_string(),
            ),
            setting(
                ersatzrs_domain::CONFIG_KEY_XMLTV_BLOCK_BEHAVIOR,
                command.block_behavior,
            ),
        ])
        .await?;
    request_settings_playout_builds(repository, queue, operations, PlayoutBuildMode::Refresh).await
}

/// Validate, atomically persist, and apply `FFmpeg` settings and side effects.
#[allow(clippy::too_many_lines)]
pub async fn save_ffmpeg_settings(
    mut command: SaveFfmpegSettings,
    repository: &dyn AdminSettingsRepository,
    runtime: &dyn SettingsRuntime,
) -> crate::Result<()> {
    if command.hls_segmenter_idle_timeout < 30 {
        return validation_error(
            "hls_segmenter_idle_timeout",
            "HLS Segmenter idle timeout must be greater than or equal to 30",
        );
    }
    if command.work_ahead_segmenter_limit < 0 {
        return validation_error(
            "work_ahead_segmenter_limit",
            "Work-Ahead HLS Segmenter limit must be greater than or equal to 0",
        );
    }
    if command.active_transcode_limit < 1 {
        return validation_error(
            "active_transcode_limit",
            "Active HLS transcode limit must be greater than or equal to 1",
        );
    }
    if command.initial_segment_count < 1 {
        return validation_error(
            "initial_segment_count",
            "HLS Segmenter initial segment count must be greater than or equal to 1",
        );
    }

    let paths = runtime
        .validate_ffmpeg_tools(&command.ffmpeg_path, &command.ffprobe_path)
        .await
        .map_err(|error| ApplicationError::Unavailable {
            operation: "validate FFmpeg tools",
            message: error.to_string(),
        })?;

    if !command.use_embedded_subtitles {
        command.extract_embedded_subtitles = false;
    }

    let paths_remain_managed = repository
        .get_optional_string(ersatzrs_domain::CONFIG_KEY_FFMPEG_PATHS_MANAGED)
        .await?
        .is_some_and(|value| value.eq_ignore_ascii_case("true"))
        && repository
            .get_optional_string(ersatzrs_domain::CONFIG_KEY_FFMPEG_PATH)
            .await?
            .is_some_and(|value| same_configured_tool_path(&value, &command.ffmpeg_path))
        && repository
            .get_optional_string(ersatzrs_domain::CONFIG_KEY_FFPROBE_PATH)
            .await?
            .is_some_and(|value| same_configured_tool_path(&value, &command.ffprobe_path));

    let mut mutations = vec![
        setting(ersatzrs_domain::CONFIG_KEY_FFMPEG_PATH, command.ffmpeg_path),
        setting(
            ersatzrs_domain::CONFIG_KEY_FFPROBE_PATH,
            command.ffprobe_path,
        ),
        setting(
            ersatzrs_domain::CONFIG_KEY_FFMPEG_PATHS_MANAGED,
            paths_remain_managed.to_string(),
        ),
        setting(
            ersatzrs_domain::CONFIG_KEY_FFMPEG_DEFAULT_PROFILE_ID,
            command.default_ffmpeg_profile_id.to_string(),
        ),
        setting(
            ersatzrs_domain::CONFIG_KEY_FFMPEG_SAVE_REPORTS,
            bool_setting(command.save_reports),
        ),
        setting(
            ersatzrs_domain::CONFIG_KEY_FFMPEG_PREFERRED_LANGUAGE_CODE,
            command.preferred_audio_language_code,
        ),
        setting(
            ersatzrs_domain::CONFIG_KEY_FFMPEG_PREFERRED_SUBTITLE_LANGUAGE_CODE,
            command.preferred_subtitle_language_code,
        ),
        setting(
            ersatzrs_domain::CONFIG_KEY_FFMPEG_FALLBACK_SUBTITLE_LANGUAGE_CODE,
            command.fallback_subtitle_language_code,
        ),
        setting(
            ersatzrs_domain::CONFIG_KEY_FFMPEG_USE_EMBEDDED_SUBTITLES,
            bool_setting(command.use_embedded_subtitles),
        ),
        setting(
            ersatzrs_domain::CONFIG_KEY_FFMPEG_EXTRACT_EMBEDDED_SUBTITLES,
            bool_setting(command.extract_embedded_subtitles),
        ),
        setting(
            ersatzrs_domain::CONFIG_KEY_FFMPEG_PROBE_FOR_INTERLACED_FRAMES,
            bool_setting(command.probe_for_interlaced_frames),
        ),
        setting(
            ersatzrs_domain::CONFIG_KEY_FFMPEG_SEGMENTER_TIMEOUT,
            command.hls_segmenter_idle_timeout.to_string(),
        ),
        setting(
            ersatzrs_domain::CONFIG_KEY_FFMPEG_WORK_AHEAD_SEGMENTERS,
            command.work_ahead_segmenter_limit.to_string(),
        ),
        setting(
            ersatzrs_domain::CONFIG_KEY_FFMPEG_ACTIVE_TRANSCODE_LIMIT,
            command.active_transcode_limit.to_string(),
        ),
        setting(
            ersatzrs_domain::CONFIG_KEY_FFMPEG_INITIAL_SEGMENT_COUNT,
            command.initial_segment_count.to_string(),
        ),
        setting(
            ersatzrs_domain::CONFIG_KEY_FFMPEG_HLS_DIRECT_OUTPUT_FORMAT,
            command.hls_direct_output_format.to_string(),
        ),
        setting(
            ersatzrs_domain::CONFIG_KEY_FFMPEG_DEFAULT_MPEG_TS_SCRIPT,
            command.default_mpeg_ts_script,
        ),
    ];
    mutations.push(optional_setting(
        ersatzrs_domain::CONFIG_KEY_FFMPEG_GLOBAL_WATERMARK_ID,
        command.global_watermark_id,
    ));
    mutations.push(optional_setting(
        ersatzrs_domain::CONFIG_KEY_FFMPEG_GLOBAL_FALLBACK_FILLER_ID,
        command.global_fallback_filler_id,
    ));
    runtime
        .commit_ffmpeg_settings(&paths.ffmpeg, &paths.ffprobe, mutations)
        .await
        .map_err(|error| ApplicationError::Unavailable {
            operation: "commit FFmpeg settings",
            message: error.to_string(),
        })?;

    if command.save_reports {
        runtime.ensure_ffmpeg_report_directory();
    }
    if command.extract_embedded_subtitles {
        runtime.request_embedded_subtitle_extraction(repository.list_playout_ids().await?);
    }
    Ok(())
}

async fn request_settings_playout_builds(
    repository: &dyn AdminSettingsRepository,
    queue: &dyn PlayoutBuildQueue,
    operations: &OperationCoordinator,
    mode: PlayoutBuildMode,
) -> crate::Result<()> {
    for playout_id in repository.list_playout_ids().await? {
        require_playout_build(
            RequestPlayoutBuild::new(playout_id, mode),
            queue,
            operations,
        )?;
    }
    Ok(())
}

fn setting(key: impl Into<String>, value: impl Into<String>) -> SettingsMutation {
    SettingsMutation::Set {
        key: key.into(),
        value: value.into(),
    }
}

fn optional_setting(key: impl Into<String>, value: Option<EntityId>) -> SettingsMutation {
    let key = key.into();
    match value {
        Some(value) => setting(key, value.to_string()),
        None => SettingsMutation::Delete { key },
    }
}

fn bool_setting(value: bool) -> String {
    if value { "True" } else { "False" }.to_owned()
}

#[cfg(windows)]
fn same_configured_tool_path(left: &str, right: &str) -> bool {
    left.trim().eq_ignore_ascii_case(right.trim())
}

#[cfg(not(windows))]
fn same_configured_tool_path(left: &str, right: &str) -> bool {
    left.trim() == right.trim()
}

fn validate_choice(field: &'static str, value: &str, allowed: &[&str]) -> crate::Result<()> {
    if allowed.contains(&value) {
        Ok(())
    } else {
        validation_error(field, format!("Invalid option: {value}"))
    }
}

fn validation_error<T>(field: &'static str, message: impl Into<String>) -> crate::Result<T> {
    Err(ApplicationError::Validation {
        field,
        message: message.into(),
    })
}

/// Import or refresh one add-on media list from its public URL.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct AddAddonMediaList {
    /// Installed provider add-on identity.
    pub addon_id: String,
    /// Provider-specific public list or saved-search URL.
    pub source_url: String,
    /// Optional storage folder name for a first import; empty derives it from
    /// the list name.
    pub folder_name: String,
    /// Optional list-wide content classification override.
    pub content_kind_override: ersatzrs_domain::AddonMediaListContentKind,
    /// Optional YouTube-style chapter markers for a single-video source.
    pub chapter_input: String,
    /// Whether optional item details should load automatically after Publish.
    pub detail_import_policy: ersatzrs_domain::AddonMediaListDetailPolicy,
}

/// Re-run local matching for one persisted Add-on media list.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct MatchAddonMediaList {
    /// Persisted Add-on media list identity.
    pub id: EntityId,
}

/// Queue one durable, non-mutating source header/first-page probe.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct TestAddonMediaListSource {
    pub id: EntityId,
    /// Exact saved draft revision displayed with the Test source action.
    pub saved_revision: i32,
}

/// Publish one reviewed Add-on media list snapshot.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct PublishAddonMediaList {
    pub id: EntityId,
    /// Exact saved draft revision shown by the editor.
    pub saved_revision: i32,
}

/// Republish one retained complete revision as a new monotonic revision.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct RestoreAddonMediaListRevision {
    pub id: EntityId,
    pub source_published_revision: i32,
    pub expected_draft_revision: i32,
    pub expected_published_revision: i32,
}

/// Durable retained-revision restore admission result.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct RestoreAddonMediaListRevisionOutcome {
    pub operation_id: EntityId,
    pub generation: i32,
    pub saved_revision: i32,
    pub target_published_revision: i32,
    pub existing: bool,
}

/// Request a destructive reset followed by a fresh provider import.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ResetAddonMediaList {
    pub id: EntityId,
    /// Must match the list's current persisted name; only this input is trimmed.
    pub confirmed_name: String,
}

/// Durable reset admission result returned without owning execution.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ResetAddonMediaListOutcome {
    Queued {
        operation_id: EntityId,
        generation: i32,
        existing: bool,
    },
}

/// Request a complete, non-destructive source reconciliation.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ReconcileAddonMediaList {
    pub id: EntityId,
}

/// Durable reconcile admission result returned without owning execution.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ReconcileAddonMediaListOutcome {
    Queued {
        operation_id: EntityId,
        generation: i32,
        existing: bool,
    },
}

/// Confirm one first-observation absence as an explicit removal candidate.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ConfirmAddonMediaListRemoval {
    pub list_id: EntityId,
    pub item_id: EntityId,
    pub expected_generation: i32,
}

/// Durable publication admission result returned without owning execution.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PublishAddonMediaListOutcome {
    /// Required playback details were queued before the revision can publish.
    Preparing { pending_details: u64 },
    /// Required processed media was queued before publication takes its lease.
    PreparingMedia { pending_items: u64 },
    /// One durable generation is queued or already active.
    Queued {
        operation_id: EntityId,
        generation: i32,
        saved_revision: i32,
        existing: bool,
    },
}

/// Retry or restore one exact durable publication generation.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct RecoverAddonMediaListPublication {
    pub list_id: EntityId,
    pub job_id: EntityId,
    pub generation: i32,
    pub action: ersatzrs_domain::AddonMediaListRecoveryAction,
}

/// Include or exclude one provider item from publication.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct SetAddonMediaListItemIncluded {
    pub list_id: EntityId,
    pub item_id: EntityId,
    pub included: bool,
}

/// Set the primary/extra role of one source without changing list order.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct SetAddonMediaListItemRole {
    pub list_id: EntityId,
    pub item_id: EntityId,
    pub role: ersatzrs_domain::AddonMediaListItemRole,
}

/// Update user-editable Add-on media list settings.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct UpdateAddonMediaList {
    /// Persisted Add-on media list identity.
    pub id: EntityId,
    /// Fixed, adaptive, or disabled provider refresh policy.
    pub refresh_cadence: ersatzrs_domain::AddonMediaListRefreshCadence,
    /// Whether safe additions from scheduled refreshes may publish automatically.
    pub refresh_publication_policy: ersatzrs_domain::AddonMediaListRefreshPublicationPolicy,
    /// Maintain a generated playlist for matched rows.
    pub generate_playlist: bool,
    /// Force one descriptive content type for all entries, or retain
    /// provider inference with `Auto`.
    pub content_kind_override: ersatzrs_domain::AddonMediaListContentKind,
    /// Optional YouTube-style chapter markers for a single-video source.
    pub chapter_input: String,
    /// Reversible automatic/manual-only detail policy.
    pub detail_import_policy: ersatzrs_domain::AddonMediaListDetailPolicy,
}

/// Queue one item for detail enrichment with manual priority.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct RequestAddonMediaListItemDetails {
    pub list_id: EntityId,
    pub item_id: EntityId,
}

/// Pause, resume, retry, or cancel the newest overview generation.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ControlAddonMediaListImport {
    pub id: EntityId,
    pub action: ersatzrs_core::repositories::AddonMediaListImportAction,
}

/// Set the descriptive classification of one Add-on media list entry.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct SetAddonMediaListItemContentKind {
    pub item_id: EntityId,
    pub content_kind: ersatzrs_domain::AddonMediaListContentKind,
}

/// Set or restore the playback behavior of one Remote Stream entry.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct SetAddonMediaListItemLiveness {
    pub list_id: EntityId,
    pub item_id: EntityId,
    pub liveness: Option<ersatzrs_domain::RemoteStreamLiveness>,
}

/// Apply or restore playback behavior for selected or all Remote Stream entries.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ApplyAddonMediaListLiveness {
    pub id: EntityId,
    pub liveness: Option<ersatzrs_domain::RemoteStreamLiveness>,
    /// Empty means every top-level Remote Stream entry.
    pub item_ids: Vec<EntityId>,
}

/// Apply one classification to every entry of an Add-on media list.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ApplyAddonMediaListContentKind {
    pub id: EntityId,
    pub content_kind: ersatzrs_domain::AddonMediaListContentKind,
}

/// Apply exact quick-review snapshots atomically, including undo snapshots.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ApplyAddonMediaListItemReviewEdits {
    pub id: EntityId,
    pub edits: Vec<AddonMediaListItemReviewEdit>,
}

/// Assign consecutive episode numbers to included top-level list entries.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct AssignAddonMediaListEpisodeNumbers {
    pub id: EntityId,
    pub season: i32,
    pub starting_episode: i32,
    /// Maximum number of eligible entries to assign; `None` assigns all.
    pub item_count: Option<u32>,
    /// Preserve entries that already have a valid season and episode number.
    pub missing_only: bool,
}

/// Move one Add-on media list to a different add-on.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ChangeAddonMediaListAddon {
    /// Persisted Add-on media list identity.
    pub id: EntityId,
    /// Add-on that should process the list from now on.
    pub addon_id: String,
}

/// Delete one Add-on media list and any generated playlist.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct DeleteAddonMediaList {
    /// Persisted Add-on media list identity.
    pub id: EntityId,
}

/// Set or clear the exact local replacement for one add-on list item.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct SetAddonMediaListItemReplacement {
    pub item_id: EntityId,
    pub media_item_id: Option<EntityId>,
    pub media_version_id: Option<EntityId>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct RequestAddonMediaListPresentation {
    pub id: EntityId,
    pub presentation_kind: ersatzrs_domain::AddonMediaListPresentationKind,
    pub show_title: Option<String>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SaveAddonMediaListNfo {
    pub id: EntityId,
    pub list_id: Option<EntityId>,
    pub metadata: ersatzrs_core::repositories::AddonMediaListNfoMetadata,
    pub etag: String,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct UpdateAddonMediaListItemChapters {
    pub id: EntityId,
    pub chapter_input: String,
}

/// Import or refresh one Add-on media list.
pub async fn add_addon_media_list(
    command: AddAddonMediaList,
    repository: &dyn AddonMediaListRepository,
) -> crate::Result<EntityId> {
    if command.addon_id.trim().is_empty() {
        return validation_error("addon_id", "An add-on is required");
    }
    if command.source_url.trim().is_empty() {
        return validation_error("source_url", "A source URL is required");
    }
    let source_url =
        match ersatzrs_addon_contract::validate_operator_source_url(command.source_url.trim(), &[])
        {
            Ok(source_url) => source_url,
            Err(error) => {
                return validation_error(
                    "source_url",
                    ersatzrs_addon_contract::AddonOperationError::new(
                        ersatzrs_addon_contract::OPERATION_CODE_UNSUPPORTED_URL,
                        error.to_string(),
                    )
                    .to_string(),
                );
            }
        };
    let folder_name = command.folder_name.trim();
    ersatzrs_core::media_chapters::parse_chapter_input(&command.chapter_input, None).map_err(
        |error| ApplicationError::Validation {
            field: "chapter_input",
            message: error.to_string(),
        },
    )?;
    repository
        .add_or_refresh_with_policy(
            command.addon_id.trim(),
            &source_url,
            (!folder_name.is_empty()).then_some(folder_name),
            command.content_kind_override,
            command.chapter_input.trim(),
            command.detail_import_policy,
        )
        .await
        .map_err(Into::into)
}

/// Re-run local item matching for one Add-on media list.
pub async fn match_addon_media_list(
    command: MatchAddonMediaList,
    repository: &dyn AddonMediaListRepository,
) -> crate::Result<()> {
    validate_positive_id(command.id, "addon_media_list_id")?;
    repository.match_items(command.id).await.map_err(Into::into)
}

/// Validate and enqueue one exact saved-revision source probe.
pub async fn test_addon_media_list_source(
    command: TestAddonMediaListSource,
    repository: &dyn ersatzrs_core::repositories::AddonMediaListSourceProbeRepository,
) -> crate::Result<EntityId> {
    validate_positive_id(command.id, "addon_media_list_id")?;
    if command.saved_revision < 0 {
        return validation_error("saved_revision", "must not be negative");
    }
    repository
        .enqueue_source_probe(command.id, command.saved_revision)
        .await
        .map_err(Into::into)
}

/// Persist one exact reviewed snapshot for background publication.
pub async fn publish_addon_media_list(
    command: PublishAddonMediaList,
    repository: &dyn AddonMediaListRepository,
    managed_operations: &dyn ManagedLibraryOperationRepository,
) -> crate::Result<PublishAddonMediaListOutcome> {
    validate_positive_id(command.id, "addon_media_list_id")?;
    let readiness = crate::queries::addon_media_list_publication_readiness(
        command.id,
        repository,
        managed_operations,
    )
    .await?;
    if readiness.publication.as_ref().is_some_and(|publication| {
        matches!(
            publication.state,
            ersatzrs_domain::AddonMediaListPublicationState::Waiting
                | ersatzrs_domain::AddonMediaListPublicationState::Expired
                | ersatzrs_domain::AddonMediaListPublicationState::Failed
        )
    }) {
        return validation_error(
            "publication",
            "publication recovery is required before another revision can be published",
        );
    }
    if let Some(blocker) = readiness.blockers.first() {
        if !matches!(
            blocker.kind,
            AddonMediaListPublicationBlockerKind::PublicationInProgress
        ) {
            return validation_error(
                "publication",
                format!("publication is blocked by {:?}", blocker.kind),
            );
        }
    } else if readiness.saved_revision != command.saved_revision {
        return validation_error(
            "saved_revision",
            "the saved add-on media-list revision changed; reload before publishing",
        );
    }
    if readiness.blockers.is_empty() {
        let pending_details = repository.prepare_publication_details(command.id).await?;
        if pending_details > 0 {
            return Ok(PublishAddonMediaListOutcome::Preparing { pending_details });
        }
        let pending_items = repository
            .prepare_publication_media(command.id, command.saved_revision)
            .await?;
        if pending_items > 0 {
            return Ok(PublishAddonMediaListOutcome::PreparingMedia { pending_items });
        }
    }
    let form = repository
        .get_admin_form(command.id)
        .await?
        .ok_or(ApplicationError::NotFound {
            kind: "add-on media list",
            id: command.id,
        })?;
    let library_id =
        repository
            .managed_library_id(command.id)
            .await?
            .ok_or(ApplicationError::Unavailable {
                operation: "queue add-on media-list publication",
                message: "managed list storage is unavailable".to_owned(),
            })?;
    let queued = managed_operations
        .enqueue_publication(
            command.id,
            library_id,
            command.saved_revision,
            form.published_revision.saturating_add(1),
        )
        .await?;
    Ok(PublishAddonMediaListOutcome::Queued {
        operation_id: queued.operation.id,
        generation: queued.job.generation,
        saved_revision: queued.job.saved_revision,
        existing: queued.existing,
    })
}

/// Validate and queue a retained revision through the publication coordinator.
pub async fn restore_addon_media_list_revision(
    command: RestoreAddonMediaListRevision,
    repository: &dyn AddonMediaListRepository,
    managed_operations: &dyn ManagedLibraryOperationRepository,
) -> crate::Result<RestoreAddonMediaListRevisionOutcome> {
    validate_positive_id(command.id, "addon_media_list_id")?;
    if command.source_published_revision <= 0 {
        return validation_error("source_published_revision", "must be greater than zero");
    }
    if command.expected_draft_revision < 0 || command.expected_published_revision < 0 {
        return validation_error("expected_revision", "must not be negative");
    }
    let library_id =
        repository
            .managed_library_id(command.id)
            .await?
            .ok_or(ApplicationError::Unavailable {
                operation: "queue add-on media-list revision restore",
                message: "managed list storage is unavailable".to_owned(),
            })?;
    let queued = managed_operations
        .enqueue_revision_restore(
            command.id,
            library_id,
            command.source_published_revision,
            command.expected_draft_revision,
            command.expected_published_revision,
        )
        .await?;
    Ok(RestoreAddonMediaListRevisionOutcome {
        operation_id: queued.operation.id,
        generation: queued.job.generation,
        saved_revision: queued.job.saved_revision,
        target_published_revision: queued.job.target_published_revision,
        existing: queued.existing,
    })
}

/// Validate and durably admit one reset/reload generation.
pub async fn reset_addon_media_list(
    command: ResetAddonMediaList,
    managed_operations: &dyn ManagedLibraryOperationRepository,
) -> crate::Result<ResetAddonMediaListOutcome> {
    validate_positive_id(command.id, "addon_media_list_id")?;
    let confirmed_name = command.confirmed_name.trim();
    if confirmed_name.is_empty() {
        return validation_error("confirmed_name", "The current list name is required");
    }
    let enqueue = managed_operations
        .enqueue_reset_reload(command.id, confirmed_name)
        .await?;
    Ok(ResetAddonMediaListOutcome::Queued {
        operation_id: enqueue.operation.id,
        generation: enqueue.job.generation,
        existing: enqueue.existing,
    })
}

/// Validate and durably admit one full-source reconcile generation.
pub async fn reconcile_addon_media_list(
    command: ReconcileAddonMediaList,
    managed_operations: &dyn ManagedLibraryOperationRepository,
) -> crate::Result<ReconcileAddonMediaListOutcome> {
    validate_positive_id(command.id, "addon_media_list_id")?;
    let enqueue = managed_operations.enqueue_reconcile(command.id).await?;
    Ok(ReconcileAddonMediaListOutcome::Queued {
        operation_id: enqueue.operation.id,
        generation: enqueue.job.generation,
        existing: enqueue.existing,
    })
}

/// Confirm only the current tombstone after verifying that the item belongs to
/// the requested list. The repository generation fence prevents a stale UI
/// action from confirming a later reconciliation observation.
pub async fn confirm_addon_media_list_removal(
    command: ConfirmAddonMediaListRemoval,
    lists: &dyn AddonMediaListRepository,
    lifecycle: &dyn ersatzrs_core::repositories::AddonMediaListLifecycleRepository,
) -> crate::Result<()> {
    validate_positive_id(command.list_id, "addon_media_list_id")?;
    validate_positive_id(command.item_id, "addon_media_list_item_id")?;
    validate_positive_id(
        i64::from(command.expected_generation),
        "last_reconcile_generation",
    )?;
    if lists
        .get_item_detail(command.list_id, command.item_id)
        .await?
        .is_none()
    {
        return Err(crate::ApplicationError::NotFound {
            kind: "add-on media-list item",
            id: command.item_id,
        });
    }
    lifecycle
        .confirm_item_tombstone(
            command.item_id,
            command.expected_generation,
            chrono::Utc::now(),
        )
        .await?;
    Ok(())
}

/// Requeue an exact failed/expired publication generation for durable work.
pub async fn recover_addon_media_list_publication(
    command: RecoverAddonMediaListPublication,
    managed_operations: &dyn ManagedLibraryOperationRepository,
) -> crate::Result<ersatzrs_core::repositories::AddonMediaListPublicationStatus> {
    validate_positive_id(command.list_id, "addon_media_list_id")?;
    validate_positive_id(command.job_id, "publication_job_id")?;
    if command.generation <= 0 {
        return validation_error("generation", "must be greater than zero");
    }
    managed_operations
        .recover_publication(
            command.list_id,
            command.job_id,
            command.generation,
            command.action,
            chrono::Utc::now(),
        )
        .await
        .map_err(Into::into)
}

/// Include or exclude one item from publication.
pub async fn set_addon_media_list_item_included(
    command: SetAddonMediaListItemIncluded,
    repository: &dyn AddonMediaListRepository,
) -> crate::Result<()> {
    validate_positive_id(command.list_id, "addon_media_list_id")?;
    validate_positive_id(command.item_id, "addon_media_list_item_id")?;
    repository
        .set_item_included(command.list_id, command.item_id, command.included)
        .await
        .map_err(Into::into)
}

/// Persist one collection-aware item role for the next publication.
pub async fn set_addon_media_list_item_role(
    command: SetAddonMediaListItemRole,
    repository: &dyn AddonMediaListRepository,
) -> crate::Result<()> {
    validate_positive_id(command.list_id, "addon_media_list_id")?;
    validate_positive_id(command.item_id, "addon_media_list_item_id")?;
    repository
        .set_item_collection_role(command.list_id, command.item_id, command.role)
        .await
        .map_err(Into::into)
}

/// Persist Add-on media list settings.
pub async fn update_addon_media_list(
    command: UpdateAddonMediaList,
    repository: &dyn AddonMediaListRepository,
) -> crate::Result<()> {
    validate_positive_id(command.id, "addon_media_list_id")?;
    ersatzrs_core::media_chapters::parse_chapter_input(&command.chapter_input, None).map_err(
        |error| ApplicationError::Validation {
            field: "chapter_input",
            message: error.to_string(),
        },
    )?;
    repository
        .update_settings(
            command.id,
            command.refresh_cadence,
            command.refresh_publication_policy,
            command.generate_playlist,
            command.content_kind_override,
            command.chapter_input.trim(),
        )
        .await
        .map_err(ApplicationError::from)?;
    repository
        .set_detail_import_policy(command.id, command.detail_import_policy)
        .await
        .map_err(Into::into)
}

/// Queue one item detail request through the durable shared throttle.
pub async fn request_addon_media_list_item_details(
    command: RequestAddonMediaListItemDetails,
    repository: &dyn AddonMediaListRepository,
) -> crate::Result<()> {
    validate_positive_id(command.list_id, "addon_media_list_id")?;
    validate_positive_id(command.item_id, "addon_media_list_item_id")?;
    repository
        .request_item_details(command.list_id, command.item_id)
        .await
        .map_err(Into::into)
}

/// Apply one lifecycle action to the newest overview import generation.
pub async fn control_addon_media_list_import(
    command: ControlAddonMediaListImport,
    repository: &dyn AddonMediaListRepository,
) -> crate::Result<()> {
    validate_positive_id(command.id, "addon_media_list_id")?;
    repository
        .control_import(command.id, command.action)
        .await
        .map_err(Into::into)
}

/// Set the descriptive classification of one entry.
///
/// # Errors
///
/// Returns a validation error for a non-positive identity, or a repository
/// error when the entry does not exist.
pub async fn set_addon_media_list_item_content_kind(
    command: SetAddonMediaListItemContentKind,
    repository: &dyn AddonMediaListRepository,
) -> crate::Result<()> {
    validate_positive_id(command.item_id, "addon_media_list_item_id")?;
    repository
        .update_item_content_kind(command.item_id, command.content_kind)
        .await
        .map_err(Into::into)
}

/// Set or restore the playback behavior of one Remote Stream entry.
pub async fn set_addon_media_list_item_liveness(
    command: SetAddonMediaListItemLiveness,
    repository: &dyn AddonMediaListRepository,
) -> crate::Result<()> {
    validate_positive_id(command.list_id, "addon_media_list_id")?;
    validate_positive_id(command.item_id, "addon_media_list_item_id")?;
    repository
        .set_item_liveness_override(command.list_id, command.item_id, command.liveness)
        .await
        .map_err(Into::into)
}

/// Apply or restore playback behavior for selected or all Remote Stream entries.
pub async fn apply_addon_media_list_liveness(
    command: ApplyAddonMediaListLiveness,
    repository: &dyn AddonMediaListRepository,
) -> crate::Result<u64> {
    validate_positive_id(command.id, "addon_media_list_id")?;
    if command.item_ids.len() > 10_000 {
        return validation_error("item_ids", "must contain at most 10000 items");
    }
    for item_id in &command.item_ids {
        validate_positive_id(*item_id, "addon_media_list_item_id")?;
    }
    repository
        .apply_list_liveness_override(command.id, command.liveness, &command.item_ids)
        .await
        .map_err(Into::into)
}

/// Apply one classification to every entry of a list as a bulk default.
///
/// The list-wide value is stored as well, so entries enumerated later inherit
/// the same classification.
///
/// # Errors
///
/// Returns a validation error for a non-positive identity, or a repository
/// error when persistence fails.
pub async fn apply_addon_media_list_content_kind(
    command: ApplyAddonMediaListContentKind,
    repository: &dyn AddonMediaListRepository,
) -> crate::Result<u64> {
    validate_positive_id(command.id, "addon_media_list_id")?;
    repository
        .apply_list_content_kind(command.id, command.content_kind)
        .await
        .map_err(Into::into)
}

/// Apply a bounded set of exact operator-owned quick-review snapshots.
pub async fn apply_addon_media_list_item_review_edits(
    command: ApplyAddonMediaListItemReviewEdits,
    repository: &dyn AddonMediaListRepository,
) -> crate::Result<u64> {
    validate_positive_id(command.id, "addon_media_list_id")?;
    if command.edits.is_empty() {
        return validation_error("edits", "must contain at least one item");
    }
    if command.edits.len() > 10_000 {
        return validation_error("edits", "must contain at most 10000 items");
    }
    let mut item_ids = std::collections::BTreeSet::new();
    for edit in &command.edits {
        validate_positive_id(edit.item_id, "addon_media_list_item_id")?;
        if !item_ids.insert(edit.item_id) {
            return validation_error("edits", "contains a duplicate item identity");
        }
        if edit.season.is_some_and(|season| season < 0) {
            return validation_error("season", "must be zero or greater");
        }
        if edit.episode.is_some_and(|episode| episode < 1) {
            return validation_error("episode", "must be one or greater");
        }
        if edit.liveness_override == Some(ersatzrs_domain::RemoteStreamLiveness::Unknown) {
            return validation_error(
                "liveness_override",
                "use no override to restore the provider value",
            );
        }
    }
    repository
        .apply_item_review_edits(command.id, &command.edits)
        .await
        .map_err(Into::into)
}

/// Assign one season and consecutive episode numbers in original provider order.
///
/// # Errors
///
/// Returns a validation error for an invalid identity or numbering range, or a
/// repository error when the atomic update fails.
pub async fn assign_addon_media_list_episode_numbers(
    command: AssignAddonMediaListEpisodeNumbers,
    repository: &dyn AddonMediaListRepository,
) -> crate::Result<u64> {
    validate_positive_id(command.id, "addon_media_list_id")?;
    if command.season < 0 {
        return validation_error("season", "must be zero or greater");
    }
    if command.starting_episode < 1 {
        return validation_error("starting_episode", "must be one or greater");
    }
    if command.item_count == Some(0) {
        return validation_error("item_count", "must be one or greater when provided");
    }
    if let Some(item_count) = command.item_count {
        let Ok(last_offset) = i32::try_from(item_count - 1) else {
            return validation_error("item_count", "is too large");
        };
        if command.starting_episode.checked_add(last_offset).is_none() {
            return validation_error(
                "item_count",
                "episode numbering exceeds the supported range",
            );
        }
    }
    repository
        .assign_episode_numbers(
            command.id,
            command.season,
            command.starting_episode,
            command.item_count,
            command.missing_only,
        )
        .await
        .map_err(Into::into)
}

/// Move one Add-on media list to a different add-on and re-import it.
///
/// # Errors
///
/// Returns a validation error for an empty add-on, or a repository error when
/// the new add-on cannot enumerate the source.
pub async fn change_addon_media_list_addon(
    command: ChangeAddonMediaListAddon,
    repository: &dyn AddonMediaListRepository,
) -> crate::Result<()> {
    validate_positive_id(command.id, "addon_media_list_id")?;
    if command.addon_id.trim().is_empty() {
        return validation_error("addon_id", "An add-on is required");
    }
    repository
        .change_addon(command.id, command.addon_id.trim())
        .await
        .map_err(Into::into)
}

/// Delete one Add-on media list.
pub async fn delete_addon_media_list(
    command: DeleteAddonMediaList,
    repository: &dyn AddonMediaListRepository,
) -> crate::Result<()> {
    validate_positive_id(command.id, "addon_media_list_id")?;
    repository.delete(command.id).await.map_err(Into::into)
}

/// Persist one exact replacement mapping and rebuild the generated playlist.
pub async fn set_addon_media_list_item_replacement(
    command: SetAddonMediaListItemReplacement,
    repository: &dyn AddonMediaListRepository,
) -> crate::Result<()> {
    validate_positive_id(command.item_id, "addon_media_list_item_id")?;
    let replacement = match (command.media_item_id, command.media_version_id) {
        (None, None) => None,
        (Some(media_item_id), Some(media_version_id)) => {
            validate_positive_id(media_item_id, "media_item_id")?;
            validate_positive_id(media_version_id, "media_version_id")?;
            Some((media_item_id, media_version_id))
        }
        _ => {
            return validation_error(
                "replacement",
                "media item and media version must be supplied together",
            )
        }
    };
    repository
        .set_replacement(command.item_id, replacement)
        .await
        .map_err(Into::into)
}

pub async fn request_addon_media_list_presentation(
    command: RequestAddonMediaListPresentation,
    repository: &dyn AddonMediaListRepository,
) -> crate::Result<()> {
    validate_positive_id(command.id, "addon_media_list_id")?;
    repository
        .request_presentation(
            command.id,
            command.presentation_kind,
            command.show_title.as_deref(),
        )
        .await
        .map_err(Into::into)
}

pub async fn save_addon_media_list_show_nfo(
    command: SaveAddonMediaListNfo,
    repository: &dyn AddonMediaListRepository,
) -> crate::Result<()> {
    validate_positive_id(command.id, "addon_media_list_id")?;
    repository
        .save_show_nfo(command.id, &command.metadata, &command.etag)
        .await
        .map_err(Into::into)
}

pub async fn save_addon_media_list_item_nfo(
    command: SaveAddonMediaListNfo,
    repository: &dyn AddonMediaListRepository,
) -> crate::Result<()> {
    validate_positive_id(command.id, "addon_media_list_item_id")?;
    let list_id = command.list_id.ok_or(ApplicationError::Validation {
        field: "addon_media_list_id",
        message: "is required".to_owned(),
    })?;
    validate_positive_id(list_id, "addon_media_list_id")?;
    repository
        .save_item_nfo(list_id, command.id, &command.metadata, &command.etag)
        .await
        .map_err(Into::into)
}

pub async fn reset_addon_media_list_nfo(
    list_id: EntityId,
    item_id: Option<EntityId>,
    repository: &dyn AddonMediaListRepository,
) -> crate::Result<()> {
    validate_positive_id(list_id, "addon_media_list_id")?;
    if let Some(item_id) = item_id {
        validate_positive_id(item_id, "addon_media_list_item_id")?;
    }
    repository
        .reset_nfo(list_id, item_id)
        .await
        .map_err(Into::into)
}

pub async fn update_addon_media_list_item_chapters(
    command: UpdateAddonMediaListItemChapters,
    repository: &dyn AddonMediaListRepository,
) -> crate::Result<()> {
    validate_positive_id(command.id, "addon_media_list_item_id")?;
    ersatzrs_core::media_chapters::parse_chapter_input(&command.chapter_input, None).map_err(
        |error| ApplicationError::Validation {
            field: "chapter_input",
            message: error.to_string(),
        },
    )?;
    repository
        .update_item_chapters(command.id, command.chapter_input.trim())
        .await
        .map_err(Into::into)
}

fn validate_positive_id(id: EntityId, field: &'static str) -> crate::Result<()> {
    if id > 0 {
        Ok(())
    } else {
        Err(ApplicationError::Validation {
            field,
            message: "must be greater than zero".to_owned(),
        })
    }
}

/// Create or update one channel while preserving its transport-neutral form.
#[derive(Clone, Debug, PartialEq)]
pub struct SaveChannel {
    /// Normalized channel editor input.
    pub form: ChannelEditorForm,
}

impl SaveChannel {
    /// Create a channel-save command.
    #[must_use]
    pub fn new(form: ChannelEditorForm) -> Self {
        Self { form }
    }
}

/// Atomically replace channel numbers for the submitted channels.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SaveChannelNumbers {
    /// Submitted channel identities and target numbers.
    pub updates: Vec<ChannelNumberUpdate>,
}

impl SaveChannelNumbers {
    /// Create a channel-number command.
    #[must_use]
    pub fn new(updates: Vec<ChannelNumberUpdate>) -> Self {
        Self { updates }
    }
}

/// Delete one persisted channel.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct DeleteChannel {
    /// Persisted channel identity.
    pub channel_id: EntityId,
}

impl DeleteChannel {
    /// Create a channel-delete command.
    #[must_use]
    pub const fn new(channel_id: EntityId) -> Self {
        Self { channel_id }
    }
}

/// Save a channel, then admit its subtitle extraction side effect.
///
/// # Errors
///
/// Returns typed validation, conflict, decode, not-found, or storage failures.
pub async fn save_channel(
    command: SaveChannel,
    repository: &dyn ChannelRepository,
    runtime: &dyn ChannelRuntime,
) -> crate::Result<EntityId> {
    let validation_state = repository.validation_state().await?;
    let form = normalize_channel_form(command.form, &validation_state);
    reject_channel_validation(validate_channel_form(&form, &validation_state))?;
    let result = repository.save_editor_form(form).await?;

    if let Some(side_effects) = repository.save_side_effects(result.id).await? {
        if side_effects.subtitle_mode != 0 {
            let mut playout_ids = side_effects.playout_ids;
            playout_ids.sort_unstable();
            playout_ids.dedup();
            runtime.request_subtitle_extraction(playout_ids);
        }
    }

    Ok(result.id)
}

/// Persist channel-number changes as one atomic repository operation.
///
/// # Errors
///
/// Returns typed validation, decode, conflict, or storage failures.
pub async fn save_channel_numbers(
    command: SaveChannelNumbers,
    repository: &dyn ChannelRepository,
) -> crate::Result<Vec<EntityId>> {
    let validation_state = repository.validation_state().await?;
    let updates = normalize_channel_number_updates(command.updates);
    reject_channel_validation(validate_channel_number_updates(&updates, &validation_state))?;
    let result = repository.save_number_updates(updates).await?;
    if result.is_success() {
        Ok(result.changed_ids)
    } else {
        Err(ApplicationError::Validation {
            field: "channel_numbers",
            message: result
                .validation_errors
                .into_iter()
                .map(|error| format!("{}: {}", error.field, error.message))
                .collect::<Vec<_>>()
                .join("; "),
        })
    }
}

/// Delete one channel after validating its persisted identity.
///
/// # Errors
///
/// Returns not-found or storage failures from the channel port.
pub async fn delete_channel(
    command: DeleteChannel,
    repository: &dyn ChannelRepository,
) -> crate::Result<()> {
    if command.channel_id <= 0 {
        return Err(ApplicationError::Validation {
            field: "channel_id",
            message: "must be greater than zero".to_owned(),
        });
    }
    repository.delete(command.channel_id).await?;
    Ok(())
}

fn reject_channel_validation(errors: Vec<ChannelValidationError>) -> crate::Result<()> {
    if errors.is_empty() {
        return Ok(());
    }
    Err(ApplicationError::Validation {
        field: "channel",
        message: errors
            .into_iter()
            .map(|error| format!("{}: {}", error.field, error.message))
            .collect::<Vec<_>>()
            .join("; "),
    })
}

fn normalize_channel_form(
    mut form: ChannelEditorForm,
    state: &ChannelValidationState,
) -> ChannelEditorForm {
    form.name = form.name.trim().to_owned();
    form.number = form.number.trim().to_owned();
    form.group = form.group.trim().to_owned();
    form.categories = clean_optional(form.categories);
    form.preferred_audio_language_code = clean_optional(form.preferred_audio_language_code);
    form.preferred_audio_title = clean_optional(form.preferred_audio_title);
    form.preferred_subtitle_language_code = clean_optional(form.preferred_subtitle_language_code);
    form.stream_selector = clean_optional(form.stream_selector);

    if form.music_video_credits_mode == 1 {
        form.music_video_credits_template = clean_optional(form.music_video_credits_template);
    } else {
        form.music_video_credits_template = None;
    }

    if form.playout_source == 1 {
        form.playout_mode = 0;
    } else {
        form.mirror_source_channel_id = None;
        form.playout_offset = None;
    }

    if state
        .channels
        .iter()
        .any(|channel| channel.id == form.id && channel.playout_count > 0)
    {
        form.playout_source = 0;
        form.mirror_source_channel_id = None;
        form.playout_offset = None;
    }

    form
}

fn clean_optional(value: Option<String>) -> Option<String> {
    value.and_then(|inner| {
        let trimmed = inner.trim();
        (!trimmed.is_empty()).then(|| trimmed.to_owned())
    })
}

fn validate_channel_form(
    form: &ChannelEditorForm,
    state: &ChannelValidationState,
) -> Vec<ChannelValidationError> {
    let mut errors = Vec::new();

    if form.id > 0 && !state.channels.iter().any(|channel| channel.id == form.id) {
        errors.push(ChannelValidationError::new("id", "Channel does not exist."));
    }
    if !is_valid_channel_number(&form.number) {
        errors.push(ChannelValidationError::new(
            "number",
            "Invalid channel number; two decimals are allowed for subchannels",
        ));
    }
    if state
        .channels
        .iter()
        .any(|channel| channel.number == form.number && channel.id != form.id)
    {
        errors.push(ChannelValidationError::new(
            "number",
            "Channel number must be unique",
        ));
    }
    if form.name.is_empty() {
        errors.push(ChannelValidationError::new("name", "Name is required"));
    } else if form.name.chars().count() > 50 {
        errors.push(ChannelValidationError::new(
            "name",
            "Name must be 50 characters or fewer",
        ));
    }
    if form.group.is_empty() {
        errors.push(ChannelValidationError::new("group", "Group is required"));
    }
    if !state.ffmpeg_profile_ids.contains(&form.ffmpeg_profile_id) {
        errors.push(ChannelValidationError::new(
            "ffmpeg_profile_id",
            format!("FFmpegProfile {} does not exist.", form.ffmpeg_profile_id),
        ));
    }
    if let Some(id) = form
        .watermark_id
        .filter(|id| !state.watermark_ids.contains(id))
    {
        errors.push(ChannelValidationError::new(
            "watermark_id",
            format!("Watermark {id} does not exist."),
        ));
    }
    if let Some(id) = form
        .fallback_filler_id
        .filter(|id| !state.fallback_filler_ids.contains(id))
    {
        errors.push(ChannelValidationError::new(
            "fallback_filler_id",
            format!("Fallback filler {id} does not exist."),
        ));
    }
    if form
        .external_logo_url
        .as_deref()
        .is_some_and(|url| !is_external_logo_url(url))
    {
        errors.push(ChannelValidationError::new(
            "external_logo_url",
            "External logo url is invalid",
        ));
    }
    if !form.is_enabled && form.show_in_epg {
        errors.push(ChannelValidationError::new(
            "show_in_epg",
            "Disabled channels cannot be shown in EPG",
        ));
    }
    if form.playout_source == 1 {
        validate_channel_mirror_source(form, state, &mut errors);
    }

    errors
}

fn validate_channel_mirror_source(
    form: &ChannelEditorForm,
    state: &ChannelValidationState,
    errors: &mut Vec<ChannelValidationError>,
) {
    let source = form.mirror_source_channel_id.and_then(|source_id| {
        (source_id != form.id)
            .then(|| {
                state
                    .channels
                    .iter()
                    .find(|channel| channel.id == source_id)
            })
            .flatten()
    });
    match source {
        None => errors.push(ChannelValidationError::new(
            "mirror_source_channel_id",
            "Mirror source channel does not exist.",
        )),
        Some(source) if source.playout_source != 0 => {
            errors.push(ChannelValidationError::new(
                "mirror_source_channel_id",
                format!(
                    "Mirror source channel {} must use generated playout source",
                    source.name
                ),
            ));
        }
        Some(_) => {}
    }

    if form.playout_offset.is_some_and(|offset| {
        offset < chrono::Duration::hours(-12) || offset > chrono::Duration::hours(12)
    }) {
        errors.push(ChannelValidationError::new(
            "playout_offset",
            "Playout offset must not be greater than 12 hours",
        ));
    }
}

fn normalize_channel_number_updates(updates: Vec<ChannelNumberUpdate>) -> Vec<ChannelNumberUpdate> {
    updates
        .into_iter()
        .map(|update| ChannelNumberUpdate {
            id: update.id,
            number: update.number.trim().to_owned(),
        })
        .collect()
}

fn validate_channel_number_updates(
    updates: &[ChannelNumberUpdate],
    state: &ChannelValidationState,
) -> Vec<ChannelValidationError> {
    use std::collections::{HashMap, HashSet};

    let mut errors = Vec::new();
    let mut submitted_numbers = HashMap::<String, EntityId>::new();
    let submitted_ids = updates
        .iter()
        .map(|update| update.id)
        .collect::<HashSet<_>>();

    for update in updates {
        if update.id <= 0 || !state.channels.iter().any(|channel| channel.id == update.id) {
            errors.push(ChannelValidationError::new(
                format!("number_{}", update.id),
                "Channel does not exist.",
            ));
            continue;
        }
        if !is_valid_channel_number(&update.number) {
            errors.push(ChannelValidationError::new(
                format!("number_{}", update.id),
                "Invalid channel number; two decimals are allowed for subchannels",
            ));
            continue;
        }
        if submitted_numbers
            .insert(update.number.clone(), update.id)
            .is_some_and(|existing_id| existing_id != update.id)
            || state.channels.iter().any(|channel| {
                channel.id != update.id
                    && !submitted_ids.contains(&channel.id)
                    && channel.number == update.number
            })
        {
            errors.push(ChannelValidationError::new(
                format!("number_{}", update.id),
                "Channel number must be unique",
            ));
        }
    }

    errors
}

fn is_valid_channel_number(value: &str) -> bool {
    let mut parts = value.split('.');
    let Some(main) = parts.next() else {
        return false;
    };
    if main.is_empty() || !main.chars().all(|character| character.is_ascii_digit()) {
        return false;
    }
    parts.next().is_none_or(|subchannel| {
        !subchannel.is_empty()
            && subchannel.len() <= 2
            && subchannel
                .chars()
                .all(|character| character.is_ascii_digit())
            && parts.next().is_none()
    })
}

fn is_external_logo_url(value: &str) -> bool {
    let trimmed = value.trim();
    let rest = trimmed
        .strip_prefix("http://")
        .or_else(|| trimmed.strip_prefix("https://"));
    rest.is_some_and(|rest| {
        !rest.is_empty() && !rest.starts_with('/') && !rest.contains(char::is_whitespace)
    })
}

/// Request to begin one long-running operation under the shared coordinator.
///
/// Playout-build admission has its dedicated command below; scan admission
/// lands in milestone p7b-03. This command remains the generic exclusion
/// boundary for other application operations.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct BeginOperation {
    /// Stable operation identity to acquire.
    pub key: OperationKey,
}

impl BeginOperation {
    /// Create an operation-admission command.
    #[must_use]
    pub const fn new(key: OperationKey) -> Self {
        Self { key }
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct CreatePlayout {
    pub kind: String,
    pub channel_id: EntityId,
    pub program_schedule_id: Option<EntityId>,
    pub schedule_file: String,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct DeletePlayout {
    pub id: EntityId,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct SavePlayoutDailyReset {
    pub id: EntityId,
    pub minutes: Option<i32>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SavePlayoutScheduleFile {
    pub id: EntityId,
    pub schedule_kind: i32,
    pub schedule_file: String,
    pub rebuild_mode: Option<PlayoutBuildMode>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct SavePlayoutDefaultDeco {
    pub id: EntityId,
    pub deco_id: Option<EntityId>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SavePlayoutAlternateSchedules {
    pub id: EntityId,
    pub items: Vec<PlayoutAlternateScheduleItemAdmin>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SavePlayoutTemplates {
    pub id: EntityId,
    pub items: Vec<PlayoutTemplateItemAdmin>,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct PlayoutBuildBatchOutcome {
    pub accepted: u32,
    pub coalesced: u32,
}

pub async fn reset_all_playouts(
    repository: &dyn PlayoutRepository,
    queue: &dyn PlayoutBuildQueue,
    operations: &OperationCoordinator,
) -> crate::Result<PlayoutBuildBatchOutcome> {
    let mut outcome = PlayoutBuildBatchOutcome::default();
    for (id, kind) in repository.list_id_kinds_admin().await? {
        let mode = match kind {
            1 => PlayoutBuildMode::Refresh,
            2..=4 => PlayoutBuildMode::Reset,
            _ => continue,
        };
        match request_playout_build(RequestPlayoutBuild::new(id, mode), queue, operations) {
            RequestPlayoutBuildOutcome::Accepted => outcome.accepted += 1,
            RequestPlayoutBuildOutcome::AlreadyQueued
            | RequestPlayoutBuildOutcome::AlreadyBuilding => outcome.coalesced += 1,
            RequestPlayoutBuildOutcome::Rejected(_) => {}
            RequestPlayoutBuildOutcome::Unavailable { message } => {
                return Err(ApplicationError::Unavailable {
                    operation: "queue playout build",
                    message,
                });
            }
        }
    }
    Ok(outcome)
}

pub async fn create_playout(
    command: CreatePlayout,
    repository: &dyn PlayoutRepository,
    queue: &dyn PlayoutBuildQueue,
    operations: &OperationCoordinator,
) -> crate::Result<EntityId> {
    validate_positive_id(command.channel_id, "channel_id")?;
    let channels = repository.list_create_channels_admin().await?;
    let channel = channels
        .iter()
        .find(|channel| channel.id == command.channel_id)
        .ok_or(ApplicationError::NotFound {
            kind: "channel",
            id: command.channel_id,
        })?;
    if channel.playout_count > 0 {
        return validation_error("channel_id", "Channel already has one playout");
    }
    let schedule_kind = parse_playout_create_kind(&command.kind)?;
    let mut program_schedule_id = None;
    let mut schedule_file = String::new();
    match schedule_kind {
        1 => {
            let id = command.program_schedule_id.filter(|id| *id > 0).ok_or(
                ApplicationError::Validation {
                    field: "program_schedule_id",
                    message: "Program schedule does not exist".to_owned(),
                },
            )?;
            let schedules = repository.list_create_schedules_admin().await?;
            let schedule = schedules.iter().find(|schedule| schedule.id == id).ok_or(
                ApplicationError::NotFound {
                    kind: "program schedule",
                    id,
                },
            )?;
            if schedule.item_count == 0 {
                return validation_error("program_schedule_id", "Program schedule must have items");
            }
            program_schedule_id = Some(id);
        }
        2 => {}
        3 | 20 => {
            schedule_file = validate_playout_schedule_path(&command.schedule_file, schedule_kind)?;
        }
        4 => {
            schedule_file = command.schedule_file.trim().to_owned();
            let script = first_playout_command_token(&schedule_file);
            if script.is_empty() || std::fs::metadata(&script).is_err() {
                return validation_error(
                    "schedule_file",
                    playout_schedule_file_missing_message(schedule_kind),
                );
            }
        }
        _ => unreachable!("validated schedule kind"),
    }
    let id = repository
        .create_admin(&PlayoutCreateAdmin {
            channel_id: command.channel_id,
            schedule_kind,
            program_schedule_id,
            schedule_file,
            seed: next_playout_seed(),
        })
        .await?;
    let outcome = request_playout_build(
        RequestPlayoutBuild::new(id, PlayoutBuildMode::Reset),
        queue,
        operations,
    );
    finalize_initial_playout_build(id, outcome, repository.delete_admin(id)).await?;
    Ok(id)
}

pub async fn delete_playout(
    command: DeletePlayout,
    repository: &dyn PlayoutRepository,
) -> crate::Result<()> {
    validate_positive_id(command.id, "playout_id")?;
    repository
        .delete_admin(command.id)
        .await
        .map_err(Into::into)
}

pub async fn save_playout_daily_reset(
    command: SavePlayoutDailyReset,
    repository: &dyn PlayoutRepository,
) -> crate::Result<()> {
    validate_positive_id(command.id, "playout_id")?;
    if command
        .minutes
        .is_some_and(|minutes| !(0..24 * 60).contains(&minutes))
    {
        return validation_error("minutes", "must be between 0 and 1439");
    }
    repository
        .save_daily_reset_admin(command.id, command.minutes)
        .await
        .map_err(Into::into)
}

pub async fn save_playout_schedule_file(
    mut command: SavePlayoutScheduleFile,
    repository: &dyn PlayoutRepository,
    queue: &dyn PlayoutBuildQueue,
    operations: &OperationCoordinator,
) -> crate::Result<()> {
    validate_positive_id(command.id, "playout_id")?;
    match command.schedule_kind {
        3 | 20 => {}
        4 => {
            command.schedule_file = validate_scripted_playout_file(&command.schedule_file)?;
        }
        _ => {
            return validation_error("schedule_kind", "Unsupported playout kind");
        }
    }
    repository
        .save_schedule_file_admin(command.id, command.schedule_kind, &command.schedule_file)
        .await?;
    if let Some(mode) = command.rebuild_mode {
        require_playout_build(
            RequestPlayoutBuild::new(command.id, mode),
            queue,
            operations,
        )?;
    }
    Ok(())
}

pub async fn save_playout_default_deco(
    command: SavePlayoutDefaultDeco,
    repository: &dyn PlayoutRepository,
) -> crate::Result<()> {
    validate_positive_id(command.id, "playout_id")?;
    let deco_id = command.deco_id.filter(|id| *id > 0);
    if let Some(id) = deco_id {
        if !repository.deco_exists_admin(id).await? {
            return Err(ApplicationError::NotFound { kind: "deco", id });
        }
    }
    repository
        .save_default_deco_admin(command.id, deco_id)
        .await
        .map_err(Into::into)
}

pub async fn save_playout_alternate_schedules(
    mut command: SavePlayoutAlternateSchedules,
    repository: &dyn PlayoutRepository,
    queue: &dyn PlayoutBuildQueue,
    operations: &OperationCoordinator,
) -> crate::Result<()> {
    validate_positive_id(command.id, "playout_id")?;
    if command.items.is_empty() {
        return validation_error(
            "items",
            "Alternate schedules must include a default schedule.",
        );
    }
    let valid = repository.list_create_schedules_admin().await?;
    command.items.sort_by_key(|item| item.index);
    for (index, item) in command.items.iter_mut().enumerate() {
        if !valid
            .iter()
            .any(|schedule| schedule.id == item.program_schedule_id)
        {
            return Err(ApplicationError::NotFound {
                kind: "program schedule",
                id: item.program_schedule_id,
            });
        }
        item.index = i32::try_from(index + 1).unwrap_or(i32::MAX);
        normalize_playout_date_filter(&mut item.date_filter);
    }
    repository
        .save_alternate_schedules_admin(command.id, &command.items)
        .await?;
    if repository.has_items_admin(command.id).await? {
        require_playout_build(
            RequestPlayoutBuild::new(command.id, PlayoutBuildMode::Refresh),
            queue,
            operations,
        )?;
    }
    Ok(())
}

pub async fn save_playout_templates(
    mut command: SavePlayoutTemplates,
    repository: &dyn PlayoutRepository,
) -> crate::Result<()> {
    validate_positive_id(command.id, "playout_id")?;
    let editor =
        repository
            .get_templates_admin(command.id)
            .await?
            .ok_or(ApplicationError::NotFound {
                kind: "playout",
                id: command.id,
            })?;
    command.items.sort_by_key(|item| item.index);
    for (index, item) in command.items.iter_mut().enumerate() {
        if !editor
            .templates
            .iter()
            .any(|value| value.id == item.template_id)
        {
            return validation_error("template_id", "Template does not exist.");
        }
        if item
            .deco_template_id
            .is_some_and(|id| id > 0 && !editor.deco_templates.iter().any(|value| value.id == id))
        {
            return validation_error("deco_template_id", "Deco template does not exist.");
        }
        item.index = i32::try_from(index + 1).unwrap_or(i32::MAX);
        normalize_playout_date_filter(&mut item.date_filter);
    }
    repository
        .save_templates_admin(command.id, &command.items)
        .await
        .map_err(Into::into)
}

pub async fn erase_playout_items(
    command: DeletePlayout,
    repository: &dyn PlayoutRepository,
) -> crate::Result<()> {
    validate_positive_id(command.id, "playout_id")?;
    repository
        .erase_items_admin(command.id)
        .await
        .map_err(Into::into)
}

pub async fn erase_playout_items_and_history(
    command: DeletePlayout,
    repository: &dyn PlayoutRepository,
) -> crate::Result<()> {
    validate_positive_id(command.id, "playout_id")?;
    repository
        .erase_items_and_history_admin(command.id)
        .await
        .map_err(Into::into)
}

fn parse_playout_create_kind(kind: &str) -> crate::Result<i32> {
    match kind.trim().to_ascii_lowercase().as_str() {
        "" | "classic" => Ok(1),
        "block" => Ok(2),
        "sequential" => Ok(3),
        "scripted" => Ok(4),
        "externaljson" | "external-json" | "external_json" => Ok(20),
        _ => validation_error("kind", "Unsupported playout kind"),
    }
}

fn validate_playout_schedule_path(value: &str, kind: i32) -> crate::Result<String> {
    let value = value.trim();
    if value.is_empty() || std::fs::metadata(value).is_err() {
        return validation_error("schedule_file", playout_schedule_file_missing_message(kind));
    }
    Ok(value.to_owned())
}

fn playout_schedule_file_missing_message(kind: i32) -> &'static str {
    match kind {
        3 => "Sequential schedule does not exist!",
        4 => "Scripted schedule does not exist!",
        20 => "External Json File does not exist!",
        _ => "Schedule file does not exist",
    }
}

fn validate_scripted_playout_file(value: &str) -> crate::Result<String> {
    let value = value.trim();
    let script = first_playout_command_token(value);
    if script.is_empty() || std::fs::metadata(&script).is_err() {
        return validation_error("schedule_file", playout_schedule_file_missing_message(4));
    }
    Ok(value.to_owned())
}

fn first_playout_command_token(value: &str) -> String {
    let value = value.trim();
    if let Some(rest) = value.strip_prefix('"') {
        rest.split_once('"')
            .map_or_else(|| rest.to_owned(), |(token, _)| token.to_owned())
    } else {
        value
            .split_whitespace()
            .next()
            .unwrap_or_default()
            .to_owned()
    }
}

fn normalize_playout_date_filter(filter: &mut ersatzrs_core::repositories::PlayoutDateFilterAdmin) {
    normalize_i32_range(&mut filter.days_of_week, 0, 6);
    normalize_i32_range(&mut filter.days_of_month, 1, 31);
    normalize_i32_range(&mut filter.months_of_year, 1, 12);
    filter.start_month = filter.start_month.clamp(1, 12);
    filter.end_month = filter.end_month.clamp(1, 12);
    filter.start_day = filter.start_day.clamp(1, 31);
    filter.end_day = filter.end_day.clamp(1, 31);
}

fn normalize_i32_range(values: &mut Vec<i32>, minimum: i32, maximum: i32) {
    values.retain(|value| (minimum..=maximum).contains(value));
    values.sort_unstable();
    values.dedup();
}

/// Request admission for one scheduler-owned playout build.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct RequestPlayoutBuild {
    /// Persisted playout identity.
    pub playout_id: EntityId,
    /// Existing Refresh, Reset, or Continue build semantics.
    pub mode: PlayoutBuildMode,
}

impl RequestPlayoutBuild {
    /// Create a playout-build application command.
    #[must_use]
    pub const fn new(playout_id: EntityId, mode: PlayoutBuildMode) -> Self {
        Self { playout_id, mode }
    }
}

/// Validation reason for a rejected playout-build command.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum RequestPlayoutBuildRejection {
    /// Persisted entity identifiers must be positive.
    InvalidPlayoutId,
}

/// Typed application outcome for playout-build admission.
#[must_use = "playout build admission failures must be handled"]
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum RequestPlayoutBuildOutcome {
    /// Work was accepted by the FIFO scheduler queue.
    Accepted,
    /// Equivalent work is already waiting for the scheduler.
    AlreadyQueued,
    /// The scheduler is currently building this playout.
    AlreadyBuilding,
    /// The scheduler work queue is unavailable.
    Unavailable {
        /// Adapter diagnostic suitable for correlated logs or transport errors.
        message: String,
    },
    /// The command failed application validation.
    Rejected(RequestPlayoutBuildRejection),
}

/// Process-local identity for one accepted scan operation.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct ScanOperationId(pub u64);

/// Validated scanner work consumed by the sole native FIFO worker.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ScanRequest {
    /// Scan a complete library.
    Library {
        /// Provider resolved from persisted library state.
        provider: MediaSourceKind,
        /// Persisted library identity.
        library_id: EntityId,
        /// Whether unchanged provider items must be refreshed.
        deep_scan: bool,
    },
    /// Scan one show through its provider-specific targeted path.
    Show {
        /// Provider resolved from persisted library state.
        provider: MediaSourceKind,
        /// Persisted library identity.
        library_id: EntityId,
        /// Persisted show identity.
        show_id: EntityId,
        /// Whether unchanged provider items must be refreshed.
        deep_scan: bool,
    },
}

impl ScanRequest {
    /// Library exclusion identity shared by full and targeted scans.
    #[must_use]
    pub const fn library_id(self) -> EntityId {
        match self {
            Self::Library { library_id, .. } | Self::Show { library_id, .. } => library_id,
        }
    }
}

/// Queue token delivered after application admission.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ScanWorkRequest {
    /// Application operation identity.
    pub operation_id: ScanOperationId,
    /// Validated work to execute.
    pub request: ScanRequest,
}

/// Request a full library scan.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct RequestLibraryScan {
    /// Persisted library identity.
    pub library_id: EntityId,
    /// Whether unchanged provider items must be refreshed.
    pub deep_scan: bool,
}

/// Request a shallow periodic scan unless that library already has active work.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct RequestScheduledLibraryScan {
    /// Persisted library identity.
    pub library_id: EntityId,
}

impl RequestScheduledLibraryScan {
    /// Create a periodic library-scan command.
    #[must_use]
    pub const fn new(library_id: EntityId) -> Self {
        Self { library_id }
    }
}

impl RequestLibraryScan {
    /// Create a full-library scan command.
    #[must_use]
    pub const fn new(library_id: EntityId, deep_scan: bool) -> Self {
        Self {
            library_id,
            deep_scan,
        }
    }
}

/// Request one targeted show scan.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct RequestShowScan {
    /// Persisted library identity.
    pub library_id: EntityId,
    /// Persisted show identity.
    pub show_id: EntityId,
    /// Whether unchanged provider items must be refreshed.
    pub deep_scan: bool,
}

impl RequestShowScan {
    /// Create a targeted-show scan command.
    #[must_use]
    pub const fn new(library_id: EntityId, show_id: EntityId, deep_scan: bool) -> Self {
        Self {
            library_id,
            show_id,
            deep_scan,
        }
    }
}

/// Validation reason for a rejected scan command.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum RequestScanRejection {
    /// Persisted library identifiers must be positive.
    InvalidLibraryId,
    /// Persisted show identifiers must be positive.
    InvalidShowId,
    /// The requested library does not exist.
    LibraryNotFound,
    /// The requested show does not belong to the library.
    ShowNotFound,
    /// The persisted library subtype or media kind is invalid for the request.
    InvalidLibraryState,
    /// Provider item synchronization is disabled for the library.
    SynchronizationDisabled,
}

/// Typed application outcome for library/show scan admission.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum RequestScanOutcome {
    /// A distinct request was accepted by the FIFO scanner queue.
    Accepted {
        /// Stable identity for subsequent status queries.
        operation_id: ScanOperationId,
    },
    /// The scanner worker is unavailable.
    Unavailable {
        /// Adapter diagnostic suitable for correlated logs or transport errors.
        message: String,
    },
    /// The request failed application validation.
    Rejected(RequestScanRejection),
}

/// Typed application outcome for periodic scan admission.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum RequestScheduledScanOutcome {
    /// A distinct request was accepted by the FIFO scanner queue.
    Accepted {
        /// Stable identity for subsequent status queries.
        operation_id: ScanOperationId,
    },
    /// The library already has queued or running work, so this cadence was
    /// deliberately coalesced.
    AlreadyActive,
    /// The scanner worker is unavailable.
    Unavailable {
        /// Adapter diagnostic suitable for correlated logs.
        message: String,
    },
    /// Persisted state rejected the periodic request.
    Rejected(RequestScanRejection),
}

/// Terminal worker result after executing one accepted scan token.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum ExecuteScanOutcome {
    /// The scanner and any eligible post-scan work completed.
    Completed,
    /// The scanner or post-scan work failed and the registry retained detail.
    Failed {
        /// Storage-neutral adapter diagnostic.
        message: String,
    },
    /// The queue token did not correspond to queued application work.
    Missing,
}

/// Result of one claimed durable managed-library operation.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum ExecuteManagedLibraryOperationOutcome {
    Completed,
    Deferred,
    LeaseLost,
    Failed { message: String },
}

pub(crate) fn request_playout_build(
    command: RequestPlayoutBuild,
    queue: &dyn PlayoutBuildQueue,
    operations: &OperationCoordinator,
) -> RequestPlayoutBuildOutcome {
    if command.playout_id <= 0 {
        return RequestPlayoutBuildOutcome::Rejected(
            RequestPlayoutBuildRejection::InvalidPlayoutId,
        );
    }

    let request = PlayoutBuildRequest {
        playout_id: command.playout_id,
        mode: command.mode,
    };
    match operations
        .playout_activity()
        .admit(request, |request| queue.enqueue(request))
    {
        Ok(PlayoutBuildAdmission::Accepted) => RequestPlayoutBuildOutcome::Accepted,
        Ok(PlayoutBuildAdmission::FollowUpAccepted | PlayoutBuildAdmission::AlreadyBuilding) => {
            RequestPlayoutBuildOutcome::AlreadyBuilding
        }
        Ok(PlayoutBuildAdmission::AlreadyQueued) => RequestPlayoutBuildOutcome::AlreadyQueued,
        Err(error) => RequestPlayoutBuildOutcome::Unavailable {
            message: error.to_string(),
        },
    }
}

pub(crate) fn require_scan_playout_refreshes(
    report: crate::ScanRuntimeReport,
    queue: &dyn PlayoutBuildQueue,
    operations: &OperationCoordinator,
) -> crate::Result<()> {
    let mut refreshes = std::collections::BTreeMap::new();
    for refresh in report.playout_refreshes {
        refreshes
            .entry(refresh.playout_id)
            .and_modify(|reason: &mut crate::ScanPlayoutRefreshReason| {
                *reason = reason.merge(refresh.reason);
            })
            .or_insert(refresh.reason);
    }

    for (playout_id, reason) in refreshes {
        let outcome = request_playout_build(
            RequestPlayoutBuild::new(playout_id, PlayoutBuildMode::Refresh),
            queue,
            operations,
        );
        require_playout_build_outcome(outcome).map_err(|error| ApplicationError::Unavailable {
            operation: "queue scan-triggered playout refresh",
            message: format!("{}: {error}", reason.as_str()),
        })?;
        tracing::info!(
            playout_id,
            reason = reason.as_str(),
            "scan reconciliation requested a dependency-scoped playout refresh"
        );
    }
    Ok(())
}

fn require_playout_build(
    command: RequestPlayoutBuild,
    queue: &dyn PlayoutBuildQueue,
    operations: &OperationCoordinator,
) -> crate::Result<()> {
    require_playout_build_outcome(request_playout_build(command, queue, operations))
}

fn require_playout_build_outcome(outcome: RequestPlayoutBuildOutcome) -> crate::Result<()> {
    match outcome {
        RequestPlayoutBuildOutcome::Accepted
        | RequestPlayoutBuildOutcome::AlreadyQueued
        | RequestPlayoutBuildOutcome::AlreadyBuilding => Ok(()),
        RequestPlayoutBuildOutcome::Rejected(RequestPlayoutBuildRejection::InvalidPlayoutId) => {
            Err(ApplicationError::InvalidData {
                kind: "playout id",
                value: "must be greater than zero".to_owned(),
            })
        }
        RequestPlayoutBuildOutcome::Unavailable { message } => Err(ApplicationError::Unavailable {
            operation: "queue playout build",
            message,
        }),
    }
}

async fn finalize_initial_playout_build<F>(
    id: EntityId,
    outcome: RequestPlayoutBuildOutcome,
    delete_unbuilt_playout: F,
) -> crate::Result<()>
where
    F: std::future::Future<Output = ersatzrs_core::repositories::Result<()>>,
{
    let Err(admission_error) = require_playout_build_outcome(outcome) else {
        return Ok(());
    };
    match delete_unbuilt_playout.await {
        Ok(()) => Err(admission_error),
        Err(cleanup_error) => Err(ApplicationError::Unavailable {
            operation: "create playout",
            message: format!(
                "{admission_error}; unable to remove unbuilt playout {id}: {cleanup_error}"
            ),
        }),
    }
}

pub(crate) async fn request_library_scan(
    command: RequestLibraryScan,
    libraries: &dyn LibraryRepository,
    managed_operations: &dyn ManagedLibraryOperationRepository,
    queue: &dyn ScanWorkQueue,
    operations: &OperationCoordinator,
) -> RequestScanOutcome {
    if command.library_id <= 0 {
        return RequestScanOutcome::Rejected(RequestScanRejection::InvalidLibraryId);
    }

    let target = match libraries.get_scan_target(command.library_id).await {
        Ok(target) => target,
        Err(error) => return scan_repository_failure(error, RequestScanRejection::LibraryNotFound),
    };
    if !target.should_sync_items {
        return RequestScanOutcome::Rejected(RequestScanRejection::SynchronizationDisabled);
    }

    admit_scan(
        ScanRequest::Library {
            provider: target.provider,
            library_id: target.library_id,
            deep_scan: command.deep_scan,
        },
        ManagedLibraryOperationKind::ManualScan,
        false,
        managed_operations,
        queue,
        operations,
    )
    .await
}

pub(crate) async fn request_scheduled_library_scan(
    command: RequestScheduledLibraryScan,
    libraries: &dyn LibraryRepository,
    managed_operations: &dyn ManagedLibraryOperationRepository,
    queue: &dyn ScanWorkQueue,
    operations: &OperationCoordinator,
) -> RequestScheduledScanOutcome {
    if command.library_id <= 0 {
        return RequestScheduledScanOutcome::Rejected(RequestScanRejection::InvalidLibraryId);
    }

    let target = match libraries.get_scan_target(command.library_id).await {
        Ok(target) => target,
        Err(error) => {
            return scheduled_scan_repository_failure(error, RequestScanRejection::LibraryNotFound);
        }
    };
    if !target.should_sync_items {
        return RequestScheduledScanOutcome::Rejected(
            RequestScanRejection::SynchronizationDisabled,
        );
    }

    match admit_scan_persisted(
        ScanRequest::Library {
            provider: target.provider,
            library_id: target.library_id,
            deep_scan: false,
        },
        ManagedLibraryOperationKind::ScheduledScan,
        true,
        managed_operations,
        queue,
        operations,
    )
    .await
    {
        Ok(Some(operation_id)) => RequestScheduledScanOutcome::Accepted { operation_id },
        Ok(None) => RequestScheduledScanOutcome::AlreadyActive,
        Err(message) => RequestScheduledScanOutcome::Unavailable { message },
    }
}

pub(crate) async fn request_show_scan(
    command: RequestShowScan,
    libraries: &dyn LibraryRepository,
    managed_operations: &dyn ManagedLibraryOperationRepository,
    queue: &dyn ScanWorkQueue,
    operations: &OperationCoordinator,
) -> RequestScanOutcome {
    if command.library_id <= 0 {
        return RequestScanOutcome::Rejected(RequestScanRejection::InvalidLibraryId);
    }
    if command.show_id <= 0 {
        return RequestScanOutcome::Rejected(RequestScanRejection::InvalidShowId);
    }

    let target = match libraries.get_scan_target(command.library_id).await {
        Ok(target) => target,
        Err(error) => return scan_repository_failure(error, RequestScanRejection::LibraryNotFound),
    };
    if !target.should_sync_items {
        return RequestScanOutcome::Rejected(RequestScanRejection::SynchronizationDisabled);
    }
    if target.media_kind != LibraryMediaKind::Shows {
        return RequestScanOutcome::Rejected(RequestScanRejection::InvalidLibraryState);
    }
    match libraries
        .show_belongs_to_library(command.library_id, command.show_id)
        .await
    {
        Ok(true) => {}
        Ok(false) => return RequestScanOutcome::Rejected(RequestScanRejection::ShowNotFound),
        Err(error) => return scan_repository_failure(error, RequestScanRejection::ShowNotFound),
    }

    let request = if target.provider == MediaSourceKind::Local {
        ScanRequest::Library {
            provider: target.provider,
            library_id: target.library_id,
            deep_scan: command.deep_scan,
        }
    } else {
        ScanRequest::Show {
            provider: target.provider,
            library_id: target.library_id,
            show_id: command.show_id,
            deep_scan: command.deep_scan,
        }
    };
    admit_scan(
        request,
        ManagedLibraryOperationKind::ManualScan,
        false,
        managed_operations,
        queue,
        operations,
    )
    .await
}

async fn admit_scan(
    request: ScanRequest,
    kind: ManagedLibraryOperationKind,
    coalesce: bool,
    managed_operations: &dyn ManagedLibraryOperationRepository,
    queue: &dyn ScanWorkQueue,
    operations: &OperationCoordinator,
) -> RequestScanOutcome {
    match admit_scan_persisted(
        request,
        kind,
        coalesce,
        managed_operations,
        queue,
        operations,
    )
    .await
    {
        Ok(Some(operation_id)) => RequestScanOutcome::Accepted { operation_id },
        Ok(None) => RequestScanOutcome::Unavailable {
            message: "scan was coalesced unexpectedly".to_owned(),
        },
        Err(message) => RequestScanOutcome::Unavailable { message },
    }
}

async fn admit_scan_persisted(
    request: ScanRequest,
    kind: ManagedLibraryOperationKind,
    coalesce: bool,
    managed_operations: &dyn ManagedLibraryOperationRepository,
    queue: &dyn ScanWorkQueue,
    operations: &OperationCoordinator,
) -> std::result::Result<Option<ScanOperationId>, String> {
    let scan = match request {
        ScanRequest::Library {
            provider,
            deep_scan,
            ..
        } => ersatzrs_core::repositories::ManagedLibraryScanSpec {
            provider,
            show_id: None,
            deep_scan,
        },
        ScanRequest::Show {
            provider,
            show_id,
            deep_scan,
            ..
        } => ersatzrs_core::repositories::ManagedLibraryScanSpec {
            provider,
            show_id: Some(show_id),
            deep_scan,
        },
    };
    let operation = managed_operations
        .enqueue_scan(kind, request.library_id(), scan, coalesce)
        .await
        .map_err(|error| error.to_string())?;
    let Some(operation) = operation else {
        return Ok(None);
    };
    let operation_id = u64::try_from(operation.id)
        .map(ScanOperationId)
        .map_err(|error| error.to_string())?;
    operations
        .scan_activity()
        .observe_persisted(operation_id, request);
    let _ = queue.enqueue(ScanWorkRequest {
        operation_id,
        request,
    });
    Ok(Some(operation_id))
}

fn scan_repository_failure(
    error: RepositoryError,
    not_found: RequestScanRejection,
) -> RequestScanOutcome {
    match error {
        RepositoryError::NotFound { .. } => RequestScanOutcome::Rejected(not_found),
        RepositoryError::Decode { .. } | RepositoryError::Conflict(_) => {
            RequestScanOutcome::Rejected(RequestScanRejection::InvalidLibraryState)
        }
        RepositoryError::Unavailable { message, .. }
        | RepositoryError::UnavailableWithPublicationCause { message, .. } => {
            RequestScanOutcome::Unavailable { message }
        }
    }
}

fn scheduled_scan_repository_failure(
    error: RepositoryError,
    not_found: RequestScanRejection,
) -> RequestScheduledScanOutcome {
    match error {
        RepositoryError::NotFound { .. } => RequestScheduledScanOutcome::Rejected(not_found),
        RepositoryError::Decode { .. } | RepositoryError::Conflict(_) => {
            RequestScheduledScanOutcome::Rejected(RequestScanRejection::InvalidLibraryState)
        }
        RepositoryError::Unavailable { message, .. }
        | RepositoryError::UnavailableWithPublicationCause { message, .. } => {
            RequestScheduledScanOutcome::Unavailable { message }
        }
    }
}

#[cfg(test)]
mod tests {
    use std::collections::BTreeMap;
    use std::sync::{Arc, Mutex};

    use async_trait::async_trait;
    use ersatzrs_core::locking::{EntityLocker, PlayoutBuildActivity};
    use ersatzrs_core::repositories::{
        AddonMediaListAdminForm, AddonMediaListAdminRow, AddonMediaListItemAdminRow,
        AddonMediaListPublication, AddonMediaListPublicationEnqueue,
        AddonMediaListPublicationFailureCause, AddonMediaListPublicationHistory,
        AddonMediaListPublicationJob, AddonMediaListPublicationReadiness,
        AddonMediaListPublicationStatus, AddonMediaListReconcileEnqueue,
        AddonMediaListReplacementCandidate, AddonMediaListRepository, AdminSettingsRepository,
        ChannelEditorForm, ChannelNumberUpdate, ChannelValidationRow, ChannelValidationState,
        CollectionAdminRow, CollectionDetailAdmin, CollectionMemberAdmin, CollectionRepository,
        FfmpegProfileRepository, LibraryRepository, LibraryScanTarget, ManagedLibraryOperation,
        ManagedLibraryOperationClaim, ManagedLibraryOperationRepository, ManagedLibraryScanSpec,
        MediaItemSuggestionAdmin, MultiCollectionAdmin, MultiCollectionAdminRow,
        MultiCollectionEditorOptions, MultiCollectionItem, PlaybackOverrides, PlaylistAdminRow,
        PlaylistEditorAdmin, PlaylistEditorOptions, PlaylistGroupAdminRow, PlaylistItemAdmin,
        PlaylistItemInsert, RepositoryError, RerunCollectionAdmin, RerunCollectionAdminRow,
        RerunCollectionEditorOptions, RerunCollectionReferenceKind, ScheduleItemAdmin,
        SettingsMutation, SettingsOptionKind, SettingsSelectOption, SmartCollectionAdminRow,
    };
    use ersatzrs_core::scheduling::{PlayoutBuildMode, PlayoutBuildRequest};
    use ersatzrs_domain::{
        AddonMediaListLifecycleJobKind, AddonMediaListLifecyclePhase,
        AddonMediaListPublicationOutcome, AddonMediaListPublicationPhase,
        AddonMediaListRecoveryAction, Collection, CollectionType, EntityId, FFmpegProfile, Library,
        LibraryMediaKind, ManagedLibraryOperationKind, ManagedLibraryOperationState,
        MediaSourceKind, Playlist, Resolution, SmartCollection,
    };

    use super::{
        add_addon_media_list, add_default_collection_item, add_media_item_to_collection,
        assign_addon_media_list_episode_numbers, change_addon_media_list_addon,
        create_playlist_group, delete_playlist, finalize_initial_playout_build, legacy_nvenc_tune,
        normalize_channel_form, normalize_channel_number_updates, normalize_legacy_nvenc_preset,
        normalize_schedule_item, normalized_hdr_metadata, parse_hdr_content_light,
        parse_hdr_mastering_display, publish_addon_media_list, reconcile_addon_media_list,
        request_library_scan, request_playout_build, request_scheduled_library_scan,
        request_show_scan, save_addon_detail_wait_settings, save_collection, save_ffmpeg_profile,
        save_ffmpeg_settings, save_logging_settings, save_multi_collection, save_playlist,
        save_playout_settings, save_rerun_collection, save_smart_collection, set_windows_startup,
        validate_channel_form, validate_channel_number_updates, validate_output_dynamic_range,
        validate_schedule_items, validate_template_group_name, validate_template_name,
        validate_watermark, AddAddonMediaList, AddMediaItemToCollection,
        AssignAddonMediaListEpisodeNumbers, BlockItemAdminForm, ChangeAddonMediaListAddon,
        ChangeCollectionItem, CreatePlaylistGroup, DeletePlaylist, FfmpegProfileAdminForm,
        PublishAddonMediaList, PublishAddonMediaListOutcome, ReconcileAddonMediaList,
        ReconcileAddonMediaListOutcome, RequestLibraryScan, RequestPlayoutBuild,
        RequestPlayoutBuildOutcome, RequestPlayoutBuildRejection, RequestScanOutcome,
        RequestScanRejection, RequestScheduledLibraryScan, RequestScheduledScanOutcome,
        RequestShowScan, SaveAddonDetailWaitSettings, SaveCollection, SaveFfmpegProfile,
        SaveFfmpegSettings, SaveLoggingSettings, SaveMultiCollection, SavePlaylist,
        SavePlayoutSettings, SaveRerunCollection, SaveSmartCollection, ScanRequest,
        ScanWorkRequest, WatermarkAdminForm,
    };
    use crate::operations::OperationCoordinator;
    use crate::ports::{
        PlayoutBuildQueue, PlayoutBuildQueueError, ScanWorkQueue, ScanWorkQueueError,
        SettingsFfmpegPaths, SettingsRuntime, SettingsRuntimeError,
    };
    use ersatzrs_domain::ffmpeg::{ContentLightLevel, OutputDynamicRange};

    #[derive(Default)]
    struct RecordingSettingsRepository {
        values: Mutex<BTreeMap<String, String>>,
        batches: Mutex<Vec<Vec<SettingsMutation>>>,
        playout_ids: Vec<EntityId>,
    }

    type EpisodeNumberAssignment = (EntityId, i32, i32, Option<u32>, bool);

    #[derive(Default)]
    struct RecordingAddonMediaListRepository {
        added_source_urls: Mutex<Vec<String>>,
        changed_addons: Mutex<Vec<(EntityId, String)>>,
        item_content_kinds: Mutex<Vec<(EntityId, ersatzrs_domain::AddonMediaListContentKind)>>,
        episode_number_assignments: Mutex<Vec<EpisodeNumberAssignment>>,
        publication_events: Arc<Mutex<Vec<String>>>,
        pending_publication_details: u64,
        pending_publication_media: u64,
        fail_publication_commit: bool,
    }

    #[async_trait]
    impl AddonMediaListRepository for RecordingAddonMediaListRepository {
        async fn list_admin(
            &self,
            _page: u32,
            _page_size: u32,
            _sort_field: ersatzrs_core::repositories::AddonMediaListAdminSortField,
            _sort_direction: ersatzrs_core::repositories::AddonMediaListAdminSortDirection,
        ) -> ersatzrs_core::repositories::Result<(Vec<AddonMediaListAdminRow>, u64)> {
            Ok((Vec::new(), 0))
        }

        async fn get_admin_form(
            &self,
            id: EntityId,
        ) -> ersatzrs_core::repositories::Result<Option<AddonMediaListAdminForm>> {
            Ok(Some(AddonMediaListAdminForm {
                id,
                published_revision: 2,
                draft_revision: 5,
                ..AddonMediaListAdminForm::default()
            }))
        }

        async fn list_items(
            &self,
            _id: EntityId,
        ) -> ersatzrs_core::repositories::Result<Vec<AddonMediaListItemAdminRow>> {
            Ok(Vec::new())
        }

        async fn publication_readiness(
            &self,
            _id: EntityId,
        ) -> ersatzrs_core::repositories::Result<AddonMediaListPublicationReadiness> {
            Ok(AddonMediaListPublicationReadiness {
                required_playback_detail_count: self.pending_publication_details,
                required_prepared_media_count: self.pending_publication_media,
                saved_revision: 5,
                review_state: ersatzrs_domain::AddonMediaListReviewState::Ready,
                ..AddonMediaListPublicationReadiness::default()
            })
        }

        async fn search_replacement_candidates(
            &self,
            _query: &str,
            _limit: u32,
        ) -> ersatzrs_core::repositories::Result<Vec<AddonMediaListReplacementCandidate>> {
            Ok(Vec::new())
        }

        async fn set_replacement(
            &self,
            _item_id: EntityId,
            _replacement: Option<(EntityId, EntityId)>,
        ) -> ersatzrs_core::repositories::Result<()> {
            Ok(())
        }

        async fn add_or_refresh(
            &self,
            _addon_id: &str,
            source_url: &str,
            _requested_folder: Option<&str>,
            _content_kind_override: ersatzrs_domain::AddonMediaListContentKind,
            _chapter_input: &str,
        ) -> ersatzrs_core::repositories::Result<EntityId> {
            self.added_source_urls
                .lock()
                .expect("added source URLs")
                .push(source_url.to_owned());
            Ok(1)
        }

        async fn set_detail_import_policy(
            &self,
            _id: EntityId,
            _policy: ersatzrs_domain::AddonMediaListDetailPolicy,
        ) -> ersatzrs_core::repositories::Result<()> {
            Ok(())
        }

        async fn managed_library_id(
            &self,
            _id: EntityId,
        ) -> ersatzrs_core::repositories::Result<Option<EntityId>> {
            Ok(Some(41))
        }

        async fn match_items(&self, _id: EntityId) -> ersatzrs_core::repositories::Result<()> {
            Ok(())
        }

        async fn prepare_publication_details(
            &self,
            _id: EntityId,
        ) -> ersatzrs_core::repositories::Result<u64> {
            Ok(self.pending_publication_details)
        }

        async fn prepare_publication_media(
            &self,
            _id: EntityId,
            saved_revision: i32,
        ) -> ersatzrs_core::repositories::Result<u64> {
            assert_eq!(saved_revision, 5);
            Ok(self.pending_publication_media)
        }

        async fn stage_publication(
            &self,
            id: EntityId,
        ) -> ersatzrs_core::repositories::Result<AddonMediaListPublication> {
            self.publication_events
                .lock()
                .expect("publication events")
                .push(format!("stage:{id}"));
            Ok(AddonMediaListPublication {
                list_id: id,
                library_id: 41,
            })
        }

        async fn commit_staged_publication(
            &self,
            id: EntityId,
        ) -> ersatzrs_core::repositories::Result<()> {
            self.publication_events
                .lock()
                .expect("publication events")
                .push(format!("commit:{id}"));
            if self.fail_publication_commit {
                Err(RepositoryError::unavailable(
                    "commit test publication",
                    "injected commit failure",
                ))
            } else {
                Ok(())
            }
        }

        async fn restore_staged_publication(
            &self,
            id: EntityId,
            _message: &str,
        ) -> ersatzrs_core::repositories::Result<()> {
            self.publication_events
                .lock()
                .expect("publication events")
                .push(format!("restore:{id}"));
            Ok(())
        }

        async fn reconcile_restored_publication(
            &self,
            id: EntityId,
        ) -> ersatzrs_core::repositories::Result<()> {
            self.publication_events
                .lock()
                .expect("publication events")
                .push(format!("reconcile:{id}"));
            Ok(())
        }

        async fn list_items_page(
            &self,
            _id: EntityId,
            _availability: ersatzrs_domain::AddonMediaListItemAvailability,
            _page: u32,
            _page_size: Option<u32>,
        ) -> ersatzrs_core::repositories::Result<Vec<AddonMediaListItemAdminRow>> {
            Ok(Vec::new())
        }

        async fn item_counts(
            &self,
            _id: EntityId,
        ) -> ersatzrs_core::repositories::Result<
            ersatzrs_core::repositories::AddonMediaListItemCounts,
        > {
            Ok(ersatzrs_core::repositories::AddonMediaListItemCounts::default())
        }

        async fn update_item_content_kind(
            &self,
            item_id: EntityId,
            content_kind: ersatzrs_domain::AddonMediaListContentKind,
        ) -> ersatzrs_core::repositories::Result<()> {
            self.item_content_kinds
                .lock()
                .expect("item content kinds")
                .push((item_id, content_kind));
            Ok(())
        }

        async fn apply_list_content_kind(
            &self,
            _id: EntityId,
            _content_kind: ersatzrs_domain::AddonMediaListContentKind,
        ) -> ersatzrs_core::repositories::Result<u64> {
            Ok(0)
        }

        async fn assign_episode_numbers(
            &self,
            id: EntityId,
            season: i32,
            starting_episode: i32,
            item_count: Option<u32>,
            missing_only: bool,
        ) -> ersatzrs_core::repositories::Result<u64> {
            self.episode_number_assignments
                .lock()
                .expect("episode number assignments")
                .push((id, season, starting_episode, item_count, missing_only));
            Ok(item_count.map_or(3, u64::from))
        }

        async fn change_addon(
            &self,
            id: EntityId,
            addon_id: &str,
        ) -> ersatzrs_core::repositories::Result<()> {
            self.changed_addons
                .lock()
                .expect("changed add-ons")
                .push((id, addon_id.to_owned()));
            Ok(())
        }

        async fn update_settings(
            &self,
            _id: EntityId,
            _refresh_cadence: ersatzrs_domain::AddonMediaListRefreshCadence,
            _refresh_publication_policy: ersatzrs_domain::AddonMediaListRefreshPublicationPolicy,
            _generate_playlist: bool,
            _content_kind_override: ersatzrs_domain::AddonMediaListContentKind,
            _chapter_input: &str,
        ) -> ersatzrs_core::repositories::Result<()> {
            Ok(())
        }

        async fn delete(&self, _id: EntityId) -> ersatzrs_core::repositories::Result<()> {
            Ok(())
        }

        async fn get_page_size(&self) -> ersatzrs_core::repositories::Result<u32> {
            Ok(10)
        }

        async fn set_page_size(&self, _page_size: u32) -> ersatzrs_core::repositories::Result<()> {
            Ok(())
        }
    }

    #[derive(Default)]
    struct RecordingManagedOperations {
        next_id: Mutex<EntityId>,
        scans: Mutex<Vec<ManagedLibraryOperation>>,
        publication: Mutex<Option<AddonMediaListPublicationEnqueue>>,
        publication_calls: Mutex<Vec<(EntityId, EntityId, i32, i32)>>,
        reconcile: Mutex<Option<AddonMediaListReconcileEnqueue>>,
    }

    impl RecordingManagedOperations {
        fn next_id(&self) -> EntityId {
            let mut next = self.next_id.lock().expect("managed operation id");
            *next += 1;
            *next
        }
    }

    #[async_trait]
    impl ManagedLibraryOperationRepository for RecordingManagedOperations {
        async fn enqueue_scan(
            &self,
            kind: ManagedLibraryOperationKind,
            library_id: EntityId,
            scan: ManagedLibraryScanSpec,
            coalesce: bool,
        ) -> ersatzrs_core::repositories::Result<Option<ManagedLibraryOperation>> {
            let mut scans = self.scans.lock().expect("managed scans");
            if coalesce
                && scans.iter().any(|operation| {
                    operation.library_id == library_id
                        && matches!(
                            operation.state,
                            ManagedLibraryOperationState::Queued
                                | ManagedLibraryOperationState::Running
                                | ManagedLibraryOperationState::WaitingToRetry
                        )
                })
            {
                return Ok(None);
            }
            let operation = ManagedLibraryOperation {
                id: self.next_id(),
                library_id,
                parent_operation_id: None,
                owner_token: "owner".to_owned(),
                kind,
                state: ManagedLibraryOperationState::Queued,
                scan: Some(scan),
                attempt_count: 0,
                next_eligible_at: None,
            };
            scans.push(operation.clone());
            Ok(Some(operation))
        }

        async fn enqueue_publication(
            &self,
            list_id: EntityId,
            library_id: EntityId,
            saved_revision: i32,
            target_published_revision: i32,
        ) -> ersatzrs_core::repositories::Result<AddonMediaListPublicationEnqueue> {
            self.publication_calls
                .lock()
                .expect("publication calls")
                .push((
                    list_id,
                    library_id,
                    saved_revision,
                    target_published_revision,
                ));
            if let Some(existing) = self.publication.lock().expect("publication").clone() {
                return Ok(AddonMediaListPublicationEnqueue {
                    existing: true,
                    ..existing
                });
            }
            let operation_id = self.next_id();
            let operation = ManagedLibraryOperation {
                id: operation_id,
                library_id,
                parent_operation_id: None,
                owner_token: "publication-owner".to_owned(),
                kind: ManagedLibraryOperationKind::Publication,
                state: ManagedLibraryOperationState::Queued,
                scan: None,
                attempt_count: 0,
                next_eligible_at: None,
            };
            let queued = AddonMediaListPublicationEnqueue {
                job: AddonMediaListPublicationJob {
                    id: operation_id,
                    operation_id,
                    list_id,
                    generation: 1,
                    saved_revision,
                    target_published_revision,
                    source_published_revision: None,
                    restore_materialized_at: None,
                    phase: AddonMediaListPublicationPhase::AwaitingLease,
                    child_scan_operation_id: None,
                    original_failure_code: None,
                    original_failure_diagnostic: None,
                    recovery_diagnostic: None,
                    outcome: None,
                },
                operation,
                existing: false,
            };
            *self.publication.lock().expect("publication") = Some(queued.clone());
            Ok(queued)
        }

        async fn enqueue_reconcile(
            &self,
            list_id: EntityId,
        ) -> ersatzrs_core::repositories::Result<AddonMediaListReconcileEnqueue> {
            if let Some(existing) = self.reconcile.lock().expect("reconcile").clone() {
                return Ok(AddonMediaListReconcileEnqueue {
                    existing: true,
                    ..existing
                });
            }
            let operation_id = self.next_id();
            let operation = ManagedLibraryOperation {
                id: operation_id,
                library_id: 41,
                parent_operation_id: None,
                owner_token: "reconcile-owner".to_owned(),
                kind: ManagedLibraryOperationKind::Reconcile,
                state: ManagedLibraryOperationState::Queued,
                scan: None,
                attempt_count: 0,
                next_eligible_at: None,
            };
            let now = chrono::Utc::now();
            let queued = AddonMediaListReconcileEnqueue {
                job: ersatzrs_core::repositories::AddonMediaListLifecycleJob {
                    id: operation_id,
                    operation_id,
                    list_id,
                    generation: 1,
                    kind: AddonMediaListLifecycleJobKind::Reconcile,
                    phase: AddonMediaListLifecyclePhase::AwaitingLease,
                    saved_draft_revision: 5,
                    saved_published_revision: 2,
                    candidate_import_job_id: None,
                    created_at: now,
                    updated_at: now,
                    completed_at: None,
                },
                operation,
                existing: false,
            };
            *self.reconcile.lock().expect("reconcile") = Some(queued.clone());
            Ok(queued)
        }

        async fn operation(
            &self,
            _operation_id: EntityId,
        ) -> ersatzrs_core::repositories::Result<Option<ManagedLibraryOperation>> {
            unreachable!("operation lookup is not used by command tests")
        }

        async fn active_scan_operation(
            &self,
            library_id: EntityId,
        ) -> ersatzrs_core::repositories::Result<Option<ManagedLibraryOperation>> {
            Ok(self
                .scans
                .lock()
                .expect("managed scans")
                .iter()
                .find(|operation| {
                    operation.library_id == library_id
                        && operation.parent_operation_id.is_none()
                        && matches!(
                            operation.kind,
                            ManagedLibraryOperationKind::ManualScan
                                | ManagedLibraryOperationKind::ScheduledScan
                                | ManagedLibraryOperationKind::StartupReplayScan
                                | ManagedLibraryOperationKind::ProviderRefresh
                        )
                        && matches!(
                            operation.state,
                            ManagedLibraryOperationState::Queued
                                | ManagedLibraryOperationState::Running
                                | ManagedLibraryOperationState::WaitingToRetry
                        )
                })
                .cloned())
        }

        async fn claim_next(
            &self,
            _now: chrono::DateTime<chrono::Utc>,
        ) -> ersatzrs_core::repositories::Result<Option<ManagedLibraryOperationClaim>> {
            unreachable!("claiming is not used by command tests")
        }

        async fn claim_operation(
            &self,
            _operation_id: EntityId,
            _now: chrono::DateTime<chrono::Utc>,
        ) -> ersatzrs_core::repositories::Result<Option<ManagedLibraryOperationClaim>> {
            unreachable!("claiming is not used by command tests")
        }

        async fn heartbeat(
            &self,
            _claim: &ManagedLibraryOperationClaim,
            _now: chrono::DateTime<chrono::Utc>,
        ) -> ersatzrs_core::repositories::Result<bool> {
            unreachable!("heartbeat is not used by command tests")
        }

        async fn publication_job(
            &self,
            _operation_id: EntityId,
        ) -> ersatzrs_core::repositories::Result<Option<AddonMediaListPublicationJob>> {
            unreachable!("job loading is not used by command tests")
        }

        async fn publication_status(
            &self,
            _list_id: EntityId,
            _now: chrono::DateTime<chrono::Utc>,
        ) -> ersatzrs_core::repositories::Result<Option<AddonMediaListPublicationStatus>> {
            Ok(None)
        }

        async fn publication_history(
            &self,
            list_id: EntityId,
            _now: chrono::DateTime<chrono::Utc>,
        ) -> ersatzrs_core::repositories::Result<AddonMediaListPublicationHistory> {
            Ok(AddonMediaListPublicationHistory {
                list_id,
                jobs: Vec::new(),
            })
        }

        async fn prune_publication_history(
            &self,
            _now: chrono::DateTime<chrono::Utc>,
        ) -> ersatzrs_core::repositories::Result<u64> {
            Ok(0)
        }

        async fn recover_publication(
            &self,
            _list_id: EntityId,
            _job_id: EntityId,
            _generation: i32,
            _action: AddonMediaListRecoveryAction,
            _now: chrono::DateTime<chrono::Utc>,
        ) -> ersatzrs_core::repositories::Result<AddonMediaListPublicationStatus> {
            unreachable!("publication recovery is not used by existing command tests")
        }

        async fn transition_publication(
            &self,
            _claim: &ManagedLibraryOperationClaim,
            _expected: AddonMediaListPublicationPhase,
            _next: AddonMediaListPublicationPhase,
        ) -> ersatzrs_core::repositories::Result<bool> {
            unreachable!("transitions are not used by command tests")
        }

        async fn ensure_publication_child_scan(
            &self,
            _claim: &ManagedLibraryOperationClaim,
            _list_id: EntityId,
        ) -> ersatzrs_core::repositories::Result<ManagedLibraryOperation> {
            unreachable!("child scans are not used by command tests")
        }

        async fn ensure_publication_verification_scan(
            &self,
            _claim: &ManagedLibraryOperationClaim,
            _list_id: EntityId,
        ) -> ersatzrs_core::repositories::Result<ManagedLibraryOperation> {
            unreachable!("verification scans are not used by command tests")
        }

        async fn begin_publication_verification(
            &self,
            _claim: &ManagedLibraryOperationClaim,
            _expected: AddonMediaListPublicationPhase,
            _outcome: AddonMediaListPublicationOutcome,
        ) -> ersatzrs_core::repositories::Result<bool> {
            unreachable!("publication verification is not used by command tests")
        }

        async fn record_publication_failure(
            &self,
            _claim: &ManagedLibraryOperationClaim,
            _expected: AddonMediaListPublicationPhase,
            _code: &str,
            _diagnostic: &str,
            _cause: Option<AddonMediaListPublicationFailureCause>,
        ) -> ersatzrs_core::repositories::Result<bool> {
            unreachable!("failure recovery is not used by command tests")
        }

        async fn schedule_publication_recovery_retry(
            &self,
            _claim: &ManagedLibraryOperationClaim,
            _diagnostic: &str,
            _cause: Option<AddonMediaListPublicationFailureCause>,
            _next_eligible_at: chrono::DateTime<chrono::Utc>,
        ) -> ersatzrs_core::repositories::Result<bool> {
            unreachable!("retry is not used by command tests")
        }

        async fn fail_publication(
            &self,
            _claim: &ManagedLibraryOperationClaim,
            _code: &str,
            _diagnostic: &str,
            _cause: Option<AddonMediaListPublicationFailureCause>,
        ) -> ersatzrs_core::repositories::Result<bool> {
            unreachable!("publication failure is not used by command tests")
        }

        async fn complete_publication(
            &self,
            _claim: &ManagedLibraryOperationClaim,
            _outcome: AddonMediaListPublicationOutcome,
        ) -> ersatzrs_core::repositories::Result<bool> {
            unreachable!("completion is not used by command tests")
        }

        async fn complete_operation(
            &self,
            _claim: &ManagedLibraryOperationClaim,
        ) -> ersatzrs_core::repositories::Result<bool> {
            unreachable!("completion is not used by command tests")
        }

        async fn fail_operation(
            &self,
            _claim: &ManagedLibraryOperationClaim,
            _code: &str,
            _diagnostic: &str,
        ) -> ersatzrs_core::repositories::Result<bool> {
            unreachable!("failure is not used by command tests")
        }
    }

    #[async_trait]
    impl AdminSettingsRepository for RecordingSettingsRepository {
        async fn get_optional_string(
            &self,
            key: &str,
        ) -> ersatzrs_core::repositories::Result<Option<String>> {
            Ok(self
                .values
                .lock()
                .expect("settings values")
                .get(key)
                .cloned())
        }

        async fn set_string(
            &self,
            key: &str,
            value: &str,
        ) -> ersatzrs_core::repositories::Result<()> {
            self.values
                .lock()
                .expect("settings values")
                .insert(key.to_owned(), value.to_owned());
            Ok(())
        }

        async fn delete_key(&self, key: &str) -> ersatzrs_core::repositories::Result<()> {
            self.values.lock().expect("settings values").remove(key);
            Ok(())
        }

        async fn apply_mutations(
            &self,
            mutations: Vec<SettingsMutation>,
        ) -> ersatzrs_core::repositories::Result<()> {
            let mut values = self.values.lock().expect("settings values");
            for mutation in &mutations {
                match mutation {
                    SettingsMutation::Set { key, value } => {
                        values.insert(key.clone(), value.clone());
                    }
                    SettingsMutation::Delete { key } => {
                        values.remove(key);
                    }
                }
            }
            self.batches
                .lock()
                .expect("settings batches")
                .push(mutations);
            Ok(())
        }

        async fn list_options(
            &self,
            _kind: SettingsOptionKind,
        ) -> ersatzrs_core::repositories::Result<Vec<SettingsSelectOption>> {
            Ok(Vec::new())
        }

        async fn list_playout_ids(&self) -> ersatzrs_core::repositories::Result<Vec<EntityId>> {
            Ok(self.playout_ids.clone())
        }

        async fn create_custom_resolution(
            &self,
            _width: i32,
            _height: i32,
        ) -> ersatzrs_core::repositories::Result<()> {
            Ok(())
        }

        async fn delete_custom_resolution(
            &self,
            _id: EntityId,
        ) -> ersatzrs_core::repositories::Result<()> {
            Ok(())
        }
    }

    #[derive(Default)]
    struct RecordingSettingsRuntime {
        events: Mutex<Vec<String>>,
        committed_mutations: Mutex<Vec<SettingsMutation>>,
        fail_ffmpeg_commit: bool,
    }

    #[async_trait]
    impl SettingsRuntime for RecordingSettingsRuntime {
        fn windows_startup_status(&self) -> crate::WindowsStartupStatus {
            crate::WindowsStartupStatus::Disabled
        }

        fn set_windows_startup(&self, enabled: bool) -> Result<(), SettingsRuntimeError> {
            self.events
                .lock()
                .expect("settings runtime events")
                .push(format!("windows-startup:{enabled}"));
            Ok(())
        }

        async fn reload_logging(&self) -> Result<(), SettingsRuntimeError> {
            self.events
                .lock()
                .expect("settings runtime events")
                .push("reload_logging".to_owned());
            Ok(())
        }

        async fn validate_ffmpeg_tools(
            &self,
            ffmpeg_path: &str,
            ffprobe_path: &str,
        ) -> Result<SettingsFfmpegPaths, SettingsRuntimeError> {
            self.events
                .lock()
                .expect("settings runtime events")
                .push(format!("validate:{ffmpeg_path}:{ffprobe_path}"));
            Ok(SettingsFfmpegPaths {
                ffmpeg: "resolved-ffmpeg".to_owned(),
                ffprobe: "resolved-ffprobe".to_owned(),
            })
        }

        async fn commit_ffmpeg_settings(
            &self,
            ffmpeg_path: &str,
            ffprobe_path: &str,
            mutations: Vec<SettingsMutation>,
        ) -> Result<(), SettingsRuntimeError> {
            self.events
                .lock()
                .expect("settings runtime events")
                .push(format!("commit:{ffmpeg_path}:{ffprobe_path}"));
            if self.fail_ffmpeg_commit {
                return Err(SettingsRuntimeError::unavailable(
                    "simulated commit failure",
                ));
            }
            self.committed_mutations
                .lock()
                .expect("committed settings mutations")
                .extend(mutations);
            Ok(())
        }

        fn request_embedded_subtitle_extraction(&self, playout_ids: Vec<EntityId>) {
            self.events
                .lock()
                .expect("settings runtime events")
                .push(format!("extract:{playout_ids:?}"));
        }

        fn ensure_ffmpeg_report_directory(&self) {
            self.events
                .lock()
                .expect("settings runtime events")
                .push("ensure_reports".to_owned());
        }

        fn set_tuner_count(&self, tuner_count: i32) {
            self.events
                .lock()
                .expect("settings runtime events")
                .push(format!("tuners:{tuner_count}"));
        }
    }

    #[derive(Default)]
    struct RecordingCollectionRepository {
        duplicate_name: bool,
        collection_ids: std::collections::BTreeSet<EntityId>,
        smart_collection_ids: std::collections::BTreeSet<EntityId>,
        multi: Option<MultiCollectionAdmin>,
        rerun: Option<RerunCollectionAdmin>,
        playlist_group: Option<PlaylistGroupAdminRow>,
        playlist: Option<PlaylistEditorAdmin>,
        saved: Mutex<Vec<(EntityId, String)>>,
        multi_saves: Mutex<Vec<(bool, Vec<MultiCollectionItem>)>>,
        rerun_saves: Mutex<Vec<RerunCollectionAdmin>>,
        playlist_saves: Mutex<Vec<PlaylistEditorAdmin>>,
        added: Mutex<Vec<(EntityId, EntityId)>>,
        removed: Mutex<Vec<(EntityId, EntityId)>>,
        playout_ids: Vec<EntityId>,
        missing_media_items: bool,
    }

    #[async_trait]
    impl CollectionRepository for RecordingCollectionRepository {
        async fn get_with_items(
            &self,
            id: EntityId,
        ) -> ersatzrs_core::repositories::Result<Collection> {
            Err(RepositoryError::NotFound {
                kind: "collection",
                id,
            })
        }

        async fn list_for_media_item(
            &self,
            _item_id: EntityId,
        ) -> ersatzrs_core::repositories::Result<Vec<Collection>> {
            Ok(Vec::new())
        }

        async fn get_smart(
            &self,
            id: EntityId,
        ) -> ersatzrs_core::repositories::Result<SmartCollection> {
            Err(RepositoryError::NotFound {
                kind: "smart collection",
                id,
            })
        }

        async fn get_playlist_with_items(
            &self,
            id: EntityId,
        ) -> ersatzrs_core::repositories::Result<Playlist> {
            Err(RepositoryError::NotFound {
                kind: "playlist",
                id,
            })
        }

        async fn upsert(
            &self,
            _collection: &Collection,
        ) -> ersatzrs_core::repositories::Result<()> {
            Ok(())
        }

        async fn list_admin(
            &self,
            _search: &str,
            _page: u32,
            _page_size: u32,
        ) -> ersatzrs_core::repositories::Result<(Vec<CollectionAdminRow>, u64)> {
            Ok((Vec::new(), 0))
        }

        async fn get_admin(
            &self,
            id: EntityId,
        ) -> ersatzrs_core::repositories::Result<Option<CollectionAdminRow>> {
            Ok(self
                .collection_ids
                .contains(&id)
                .then(|| CollectionAdminRow {
                    id,
                    name: format!("Collection {id}"),
                }))
        }

        async fn admin_name_exists(
            &self,
            _id: EntityId,
            _name: &str,
        ) -> ersatzrs_core::repositories::Result<bool> {
            Ok(self.duplicate_name)
        }

        async fn save_admin(
            &self,
            id: EntityId,
            name: &str,
        ) -> ersatzrs_core::repositories::Result<EntityId> {
            self.saved
                .lock()
                .expect("saved collections")
                .push((id, name.to_owned()));
            Ok(if id == 0 { 11 } else { id })
        }

        async fn create_with_default_item_admin(
            &self,
            name: &str,
            media_item_id: EntityId,
        ) -> ersatzrs_core::repositories::Result<EntityId> {
            self.saved
                .lock()
                .expect("saved collections")
                .push((0, name.to_owned()));
            self.added
                .lock()
                .expect("added collection items")
                .push((11, media_item_id));
            Ok(11)
        }

        async fn delete_admin(&self, _id: EntityId) -> ersatzrs_core::repositories::Result<()> {
            Ok(())
        }

        async fn add_default_item(
            &self,
            collection_id: EntityId,
            media_item_id: EntityId,
        ) -> ersatzrs_core::repositories::Result<()> {
            self.added
                .lock()
                .expect("added collection items")
                .push((collection_id, media_item_id));
            Ok(())
        }

        async fn remove_item(
            &self,
            collection_id: EntityId,
            media_item_id: EntityId,
        ) -> ersatzrs_core::repositories::Result<()> {
            self.removed
                .lock()
                .expect("removed collection items")
                .push((collection_id, media_item_id));
            Ok(())
        }

        async fn playout_ids_using(
            &self,
            _collection_id: EntityId,
        ) -> ersatzrs_core::repositories::Result<Vec<EntityId>> {
            Ok(self.playout_ids.clone())
        }

        async fn list_smart_admin(
            &self,
            _search: &str,
            _page: u32,
            _page_size: u32,
        ) -> ersatzrs_core::repositories::Result<(Vec<SmartCollectionAdminRow>, u64)> {
            Ok((Vec::new(), 0))
        }

        async fn get_smart_admin(
            &self,
            id: EntityId,
        ) -> ersatzrs_core::repositories::Result<Option<SmartCollectionAdminRow>> {
            Ok(self
                .smart_collection_ids
                .contains(&id)
                .then(|| SmartCollectionAdminRow {
                    id,
                    name: format!("Smart {id}"),
                    query: "title contains movie".to_owned(),
                }))
        }

        async fn smart_admin_name_exists(
            &self,
            _id: EntityId,
            _name: &str,
        ) -> ersatzrs_core::repositories::Result<bool> {
            Ok(self.duplicate_name)
        }

        async fn save_smart_admin(
            &self,
            id: EntityId,
            name: &str,
            query: &str,
        ) -> ersatzrs_core::repositories::Result<EntityId> {
            self.saved
                .lock()
                .expect("saved collections")
                .push((id, format!("{name}:{query}")));
            Ok(if id == 0 { 12 } else { id })
        }

        async fn delete_smart_admin(
            &self,
            _id: EntityId,
        ) -> ersatzrs_core::repositories::Result<()> {
            Ok(())
        }

        async fn playout_ids_using_smart(
            &self,
            _id: EntityId,
        ) -> ersatzrs_core::repositories::Result<Vec<EntityId>> {
            Ok(self.playout_ids.clone())
        }

        async fn list_multi_admin(
            &self,
            _search: &str,
            _page: u32,
            _page_size: u32,
        ) -> ersatzrs_core::repositories::Result<(Vec<MultiCollectionAdminRow>, u64)> {
            Ok((Vec::new(), 0))
        }

        async fn get_multi_admin(
            &self,
            id: EntityId,
        ) -> ersatzrs_core::repositories::Result<Option<MultiCollectionAdmin>> {
            Ok(self.multi.clone().filter(|multi| multi.id == id))
        }

        async fn get_multi_editor_options(
            &self,
        ) -> ersatzrs_core::repositories::Result<MultiCollectionEditorOptions> {
            Ok(MultiCollectionEditorOptions {
                collections: Vec::new(),
                smart_collections: Vec::new(),
            })
        }

        async fn multi_admin_name_exists(
            &self,
            _id: EntityId,
            _name: &str,
        ) -> ersatzrs_core::repositories::Result<bool> {
            Ok(self.duplicate_name)
        }

        async fn save_multi_admin(
            &self,
            id: EntityId,
            name: &str,
            items: &[MultiCollectionItem],
            replace_items: bool,
        ) -> ersatzrs_core::repositories::Result<EntityId> {
            self.saved
                .lock()
                .expect("saved collections")
                .push((id, name.to_owned()));
            self.multi_saves
                .lock()
                .expect("multi collection saves")
                .push((replace_items, items.to_vec()));
            Ok(if id == 0 { 13 } else { id })
        }

        async fn delete_multi_admin(
            &self,
            _id: EntityId,
        ) -> ersatzrs_core::repositories::Result<()> {
            Ok(())
        }

        async fn playout_ids_using_multi(
            &self,
            _id: EntityId,
        ) -> ersatzrs_core::repositories::Result<Vec<EntityId>> {
            Ok(self.playout_ids.clone())
        }

        async fn list_rerun_admin(
            &self,
            _search: &str,
            _page: u32,
            _page_size: u32,
        ) -> ersatzrs_core::repositories::Result<(Vec<RerunCollectionAdminRow>, u64)> {
            Ok((Vec::new(), 0))
        }

        async fn get_rerun_admin(
            &self,
            id: EntityId,
        ) -> ersatzrs_core::repositories::Result<Option<RerunCollectionAdmin>> {
            Ok(self.rerun.clone().filter(|rerun| rerun.id == id))
        }

        async fn get_rerun_editor_options(
            &self,
        ) -> ersatzrs_core::repositories::Result<RerunCollectionEditorOptions> {
            Ok(RerunCollectionEditorOptions {
                collections: Vec::new(),
                multi_collections: Vec::new(),
                smart_collections: Vec::new(),
                shows: Vec::new(),
                seasons: Vec::new(),
                artists: Vec::new(),
            })
        }

        async fn rerun_admin_name_exists(
            &self,
            _id: EntityId,
            _name: &str,
        ) -> ersatzrs_core::repositories::Result<bool> {
            Ok(self.duplicate_name)
        }

        async fn rerun_reference_exists(
            &self,
            _kind: RerunCollectionReferenceKind,
            _id: EntityId,
        ) -> ersatzrs_core::repositories::Result<bool> {
            Ok(true)
        }

        async fn save_rerun_admin(
            &self,
            form: &RerunCollectionAdmin,
        ) -> ersatzrs_core::repositories::Result<EntityId> {
            self.rerun_saves
                .lock()
                .expect("rerun collection saves")
                .push(form.clone());
            Ok(if form.id == 0 { 14 } else { form.id })
        }

        async fn delete_rerun_admin(
            &self,
            _id: EntityId,
        ) -> ersatzrs_core::repositories::Result<()> {
            Ok(())
        }

        async fn playout_ids_using_rerun(
            &self,
            _id: EntityId,
        ) -> ersatzrs_core::repositories::Result<Vec<EntityId>> {
            Ok(self.playout_ids.clone())
        }

        async fn list_playlist_groups_admin(
            &self,
        ) -> ersatzrs_core::repositories::Result<Vec<PlaylistGroupAdminRow>> {
            Ok(self.playlist_group.clone().into_iter().collect())
        }

        async fn get_playlist_group_admin(
            &self,
            id: EntityId,
        ) -> ersatzrs_core::repositories::Result<Option<PlaylistGroupAdminRow>> {
            Ok(self
                .playlist_group
                .clone()
                .filter(|playlist_group| playlist_group.id == id))
        }

        async fn create_playlist_group_admin(
            &self,
            name: &str,
        ) -> ersatzrs_core::repositories::Result<PlaylistGroupAdminRow> {
            self.saved
                .lock()
                .expect("saved collections")
                .push((0, name.to_owned()));
            Ok(PlaylistGroupAdminRow {
                id: 15,
                name: name.to_owned(),
                is_system: false,
                playlists: Vec::new(),
            })
        }

        async fn playlist_name_exists(
            &self,
            _group_id: EntityId,
            _name: &str,
        ) -> ersatzrs_core::repositories::Result<bool> {
            Ok(self.duplicate_name)
        }

        async fn create_playlist_admin(
            &self,
            group_id: EntityId,
            name: &str,
        ) -> ersatzrs_core::repositories::Result<PlaylistAdminRow> {
            self.saved
                .lock()
                .expect("saved collections")
                .push((group_id, name.to_owned()));
            Ok(PlaylistAdminRow {
                id: 16,
                playlist_group_id: group_id,
                name: name.to_owned(),
                is_system: false,
            })
        }

        async fn delete_playlist_group_admin(
            &self,
            _id: EntityId,
        ) -> ersatzrs_core::repositories::Result<()> {
            Ok(())
        }

        async fn delete_playlist_admin(
            &self,
            _id: EntityId,
        ) -> ersatzrs_core::repositories::Result<()> {
            Ok(())
        }

        async fn get_playlist_editor_admin(
            &self,
            id: EntityId,
        ) -> ersatzrs_core::repositories::Result<Option<PlaylistEditorAdmin>> {
            Ok(self.playlist.clone().filter(|playlist| playlist.id == id))
        }

        async fn get_playlist_editor_options(
            &self,
        ) -> ersatzrs_core::repositories::Result<PlaylistEditorOptions> {
            Ok(PlaylistEditorOptions {
                collections: Vec::new(),
                multi_collections: Vec::new(),
                smart_collections: Vec::new(),
                shows: Vec::new(),
                seasons: Vec::new(),
                artists: Vec::new(),
                movies: Vec::new(),
                episodes: Vec::new(),
                music_videos: Vec::new(),
                other_videos: Vec::new(),
                songs: Vec::new(),
                images: Vec::new(),
                remote_streams: Vec::new(),
            })
        }

        async fn save_playlist_admin(
            &self,
            form: &PlaylistEditorAdmin,
        ) -> ersatzrs_core::repositories::Result<()> {
            self.playlist_saves
                .lock()
                .expect("playlist saves")
                .push(form.clone());
            Ok(())
        }

        async fn suggest_media_items_admin(
            &self,
            _prefix: &str,
        ) -> ersatzrs_core::repositories::Result<Vec<MediaItemSuggestionAdmin>> {
            Ok(Vec::new())
        }

        async fn media_items_exist(
            &self,
            _ids: &[EntityId],
        ) -> ersatzrs_core::repositories::Result<bool> {
            Ok(!self.missing_media_items)
        }

        async fn add_collection_items_admin(
            &self,
            _collection_id: EntityId,
            _media_item_ids: &[EntityId],
            _playback: &PlaybackOverrides,
        ) -> ersatzrs_core::repositories::Result<u64> {
            Ok(0)
        }

        async fn add_playlist_items_admin(
            &self,
            _playlist_id: EntityId,
            _items: &[PlaylistItemInsert],
        ) -> ersatzrs_core::repositories::Result<u64> {
            Ok(0)
        }

        async fn get_collection_detail_admin(
            &self,
            _id: EntityId,
        ) -> ersatzrs_core::repositories::Result<Option<CollectionDetailAdmin>> {
            Ok(None)
        }

        async fn set_collection_custom_order_enabled(
            &self,
            _id: EntityId,
            _enabled: bool,
        ) -> ersatzrs_core::repositories::Result<bool> {
            Ok(false)
        }

        async fn remove_collection_rows(
            &self,
            _collection_id: EntityId,
            _row_ids: &[EntityId],
        ) -> ersatzrs_core::repositories::Result<u64> {
            Ok(0)
        }

        async fn update_collection_custom_order(
            &self,
            _collection_id: EntityId,
            _ordered_row_ids: &[EntityId],
        ) -> ersatzrs_core::repositories::Result<u64> {
            Ok(0)
        }

        async fn list_collection_members_admin(
            &self,
            _collection_id: EntityId,
        ) -> ersatzrs_core::repositories::Result<Vec<CollectionMemberAdmin>> {
            Ok(Vec::new())
        }
    }

    #[derive(Default)]
    struct RecordingQueue {
        requests: Mutex<Vec<PlayoutBuildRequest>>,
        unavailable: bool,
    }

    impl RecordingQueue {
        fn unavailable() -> Self {
            Self {
                requests: Mutex::default(),
                unavailable: true,
            }
        }
    }

    impl PlayoutBuildQueue for RecordingQueue {
        fn enqueue(&self, request: PlayoutBuildRequest) -> Result<(), PlayoutBuildQueueError> {
            if self.unavailable {
                Err(PlayoutBuildQueueError::unavailable("worker closed"))
            } else {
                self.requests.lock().expect("request lock").push(request);
                Ok(())
            }
        }
    }

    enum OptionalFfmpegProfile {
        Missing,
        DecodeFailure,
        Unavailable,
    }

    struct FailingFfmpegRepository {
        optional: OptionalFfmpegProfile,
    }

    #[async_trait]
    impl FfmpegProfileRepository for FailingFfmpegRepository {
        async fn get(&self, _id: EntityId) -> ersatzrs_core::repositories::Result<FFmpegProfile> {
            Ok(FFmpegProfile::default())
        }

        async fn get_optional_admin(
            &self,
            _id: EntityId,
        ) -> ersatzrs_core::repositories::Result<Option<FFmpegProfile>> {
            match self.optional {
                OptionalFfmpegProfile::Missing => Ok(None),
                OptionalFfmpegProfile::DecodeFailure => Err(RepositoryError::Decode {
                    kind: "ffmpeg profile",
                    value: "invalid enum".to_owned(),
                }),
                OptionalFfmpegProfile::Unavailable => Err(RepositoryError::unavailable(
                    "load ffmpeg profile",
                    "database offline",
                )),
            }
        }

        async fn list(&self) -> ersatzrs_core::repositories::Result<Vec<FFmpegProfile>> {
            Ok(Vec::new())
        }

        async fn upsert(
            &self,
            _profile: &FFmpegProfile,
        ) -> ersatzrs_core::repositories::Result<()> {
            Ok(())
        }

        async fn delete(&self, _id: EntityId) -> ersatzrs_core::repositories::Result<()> {
            Ok(())
        }

        async fn list_resolutions(&self) -> ersatzrs_core::repositories::Result<Vec<Resolution>> {
            Ok(Vec::new())
        }

        async fn name_exists(
            &self,
            _id: EntityId,
            _name: &str,
        ) -> ersatzrs_core::repositories::Result<bool> {
            Ok(false)
        }

        async fn save_admin(
            &self,
            _profile: &FFmpegProfile,
        ) -> ersatzrs_core::repositories::Result<EntityId> {
            Ok(1)
        }

        async fn copy_admin(
            &self,
            _id: EntityId,
            _name: &str,
        ) -> ersatzrs_core::repositories::Result<EntityId> {
            Ok(2)
        }
    }

    #[derive(Default)]
    struct RecordingScanQueue {
        requests: Mutex<Vec<ScanWorkRequest>>,
        unavailable: bool,
    }

    impl RecordingScanQueue {
        fn unavailable() -> Self {
            Self {
                requests: Mutex::default(),
                unavailable: true,
            }
        }
    }

    impl ScanWorkQueue for RecordingScanQueue {
        fn enqueue(&self, request: ScanWorkRequest) -> Result<(), ScanWorkQueueError> {
            if self.unavailable {
                Err(ScanWorkQueueError::unavailable("scanner receiver closed"))
            } else {
                self.requests.lock().expect("scan requests").push(request);
                Ok(())
            }
        }
    }

    struct FakeLibraryRepository {
        targets: BTreeMap<EntityId, LibraryScanTarget>,
        show_exists: bool,
    }

    impl FakeLibraryRepository {
        fn with_targets(targets: impl IntoIterator<Item = LibraryScanTarget>) -> Self {
            Self {
                targets: targets
                    .into_iter()
                    .map(|target| (target.library_id, target))
                    .collect(),
                show_exists: true,
            }
        }
    }

    #[async_trait]
    impl LibraryRepository for FakeLibraryRepository {
        async fn get_with_paths(
            &self,
            id: EntityId,
        ) -> ersatzrs_core::repositories::Result<Library> {
            Err(RepositoryError::NotFound {
                kind: "library",
                id,
            })
        }

        async fn list_for_source(
            &self,
            _source_id: EntityId,
        ) -> ersatzrs_core::repositories::Result<Vec<Library>> {
            Ok(Vec::new())
        }

        async fn upsert(&self, _library: &Library) -> ersatzrs_core::repositories::Result<()> {
            Ok(())
        }

        async fn get_scan_target(
            &self,
            id: EntityId,
        ) -> ersatzrs_core::repositories::Result<LibraryScanTarget> {
            self.targets
                .get(&id)
                .copied()
                .ok_or(RepositoryError::NotFound {
                    kind: "library",
                    id,
                })
        }

        async fn show_belongs_to_library(
            &self,
            _library_id: EntityId,
            _show_id: EntityId,
        ) -> ersatzrs_core::repositories::Result<bool> {
            Ok(self.show_exists)
        }

        async fn media_state_revision_admin(
            &self,
        ) -> ersatzrs_core::repositories::Result<ersatzrs_core::repositories::MediaStateRevision>
        {
            Ok(ersatzrs_core::repositories::MediaStateRevision::default())
        }
    }

    fn scan_target(library_id: EntityId, provider: MediaSourceKind) -> LibraryScanTarget {
        LibraryScanTarget {
            library_id,
            provider,
            media_kind: LibraryMediaKind::Shows,
            should_sync_items: true,
        }
    }

    fn coordinator() -> OperationCoordinator {
        OperationCoordinator::new(EntityLocker::default(), PlayoutBuildActivity::default())
    }

    fn logging_settings(level: &str) -> SaveLoggingSettings {
        SaveLoggingSettings {
            default_minimum_log_level: level.to_owned(),
            scanning_minimum_log_level: level.to_owned(),
            scheduling_minimum_log_level: level.to_owned(),
            searching_minimum_log_level: level.to_owned(),
            streaming_minimum_log_level: level.to_owned(),
            http_minimum_log_level: level.to_owned(),
        }
    }

    fn ffmpeg_settings() -> SaveFfmpegSettings {
        SaveFfmpegSettings {
            ffmpeg_path: "ffmpeg-custom".to_owned(),
            ffprobe_path: "ffprobe-custom".to_owned(),
            default_ffmpeg_profile_id: 4,
            preferred_audio_language_code: "eng".to_owned(),
            preferred_subtitle_language_code: "nld".to_owned(),
            fallback_subtitle_language_code: "eng".to_owned(),
            use_embedded_subtitles: false,
            extract_embedded_subtitles: true,
            probe_for_interlaced_frames: true,
            save_reports: false,
            global_watermark_id: None,
            global_fallback_filler_id: None,
            hls_segmenter_idle_timeout: 60,
            work_ahead_segmenter_limit: 1,
            active_transcode_limit: 2,
            initial_segment_count: 1,
            hls_direct_output_format: 2,
            default_mpeg_ts_script: "default".to_owned(),
        }
    }

    #[tokio::test]
    async fn add_on_media_list_rejects_unsafe_urls_before_repository_work() {
        let repository = RecordingAddonMediaListRepository::default();
        for source_url in [
            "file:///tmp/list",
            "javascript:alert(1)",
            "example.test/list",
            "https://user:secret@example.test/list",
        ] {
            let error = add_addon_media_list(
                AddAddonMediaList {
                    addon_id: "org.ersatzrs.addon.fixture".to_owned(),
                    source_url: source_url.to_owned(),
                    folder_name: String::new(),
                    content_kind_override: ersatzrs_domain::AddonMediaListContentKind::Auto,
                    chapter_input: String::new(),
                    detail_import_policy: ersatzrs_domain::AddonMediaListDetailPolicy::Automatic,
                },
                &repository,
            )
            .await
            .expect_err("unsafe source URL");
            let message = error.to_string();
            assert!(
                message.contains("unsupported-url"),
                "{source_url}: {message}"
            );
            assert!(!message.contains("user:secret"));
        }
        assert!(repository
            .added_source_urls
            .lock()
            .expect("added source URLs")
            .is_empty());
    }

    #[tokio::test]
    async fn add_on_media_list_publication_persists_exact_revision_and_returns_queued() {
        let events = Arc::new(Mutex::new(Vec::new()));
        let repository = RecordingAddonMediaListRepository {
            publication_events: Arc::clone(&events),
            ..Default::default()
        };
        let managed = RecordingManagedOperations::default();

        let outcome = publish_addon_media_list(
            PublishAddonMediaList {
                id: 7,
                saved_revision: 5,
            },
            &repository,
            &managed,
        )
        .await
        .expect("publish");

        assert_eq!(
            outcome,
            PublishAddonMediaListOutcome::Queued {
                operation_id: 1,
                generation: 1,
                saved_revision: 5,
                existing: false,
            }
        );
        assert_eq!(
            *managed.publication_calls.lock().expect("publication calls"),
            [(7, 41, 5, 3)]
        );
        assert!(events.lock().expect("publication events").is_empty());
    }

    #[tokio::test]
    async fn add_on_media_list_reconcile_returns_the_coalesced_durable_generation() {
        let managed = RecordingManagedOperations::default();

        let first = reconcile_addon_media_list(ReconcileAddonMediaList { id: 7 }, &managed)
            .await
            .expect("queue reconcile");
        let duplicate = reconcile_addon_media_list(ReconcileAddonMediaList { id: 7 }, &managed)
            .await
            .expect("coalesce reconcile");

        assert_eq!(
            first,
            ReconcileAddonMediaListOutcome::Queued {
                operation_id: 1,
                generation: 1,
                existing: false,
            }
        );
        assert_eq!(
            duplicate,
            ReconcileAddonMediaListOutcome::Queued {
                operation_id: 1,
                generation: 1,
                existing: true,
            }
        );
    }

    #[tokio::test]
    async fn add_on_media_list_publication_defers_scanning_for_required_details() {
        let events = Arc::new(Mutex::new(Vec::new()));
        let repository = RecordingAddonMediaListRepository {
            publication_events: Arc::clone(&events),
            pending_publication_details: 3,
            ..Default::default()
        };
        let managed = RecordingManagedOperations::default();

        let outcome = publish_addon_media_list(
            PublishAddonMediaList {
                id: 7,
                saved_revision: 5,
            },
            &repository,
            &managed,
        )
        .await
        .expect("prepare playback details");

        assert_eq!(
            outcome,
            PublishAddonMediaListOutcome::Preparing { pending_details: 3 }
        );
        assert!(events.lock().expect("publication events").is_empty());
        assert!(managed
            .publication_calls
            .lock()
            .expect("publication calls")
            .is_empty());
    }

    #[tokio::test]
    async fn add_on_media_list_publication_prepares_media_before_taking_a_lease() {
        let repository = RecordingAddonMediaListRepository {
            pending_publication_media: 2,
            ..Default::default()
        };
        let managed = RecordingManagedOperations::default();
        assert_eq!(
            publish_addon_media_list(
                PublishAddonMediaList {
                    id: 7,
                    saved_revision: 5
                },
                &repository,
                &managed,
            )
            .await
            .expect("queue media preparation"),
            PublishAddonMediaListOutcome::PreparingMedia { pending_items: 2 }
        );
        assert!(managed.publication_calls.lock().unwrap().is_empty());
        assert!(repository.publication_events.lock().unwrap().is_empty());
    }

    #[tokio::test]
    async fn add_on_media_list_publication_rejects_a_stale_saved_revision() {
        let repository = RecordingAddonMediaListRepository::default();
        let managed = RecordingManagedOperations::default();
        let error = publish_addon_media_list(
            PublishAddonMediaList {
                id: 7,
                saved_revision: 4,
            },
            &repository,
            &managed,
        )
        .await
        .expect_err("stale revision");
        assert!(matches!(error, crate::ApplicationError::Validation { .. }));
        assert!(managed
            .publication_calls
            .lock()
            .expect("publication calls")
            .is_empty());
    }

    #[tokio::test]
    async fn add_on_media_list_forwards_the_validated_canonical_url() {
        let repository = RecordingAddonMediaListRepository::default();
        add_addon_media_list(
            AddAddonMediaList {
                addon_id: "org.ersatzrs.addon.fixture".to_owned(),
                source_url: r"https://allowed.example\@evil.example/list".to_owned(),
                folder_name: String::new(),
                content_kind_override: ersatzrs_domain::AddonMediaListContentKind::Auto,
                chapter_input: String::new(),
                detail_import_policy: ersatzrs_domain::AddonMediaListDetailPolicy::Automatic,
            },
            &repository,
        )
        .await
        .expect("canonical source URL import");

        assert_eq!(
            *repository
                .added_source_urls
                .lock()
                .expect("added source URLs"),
            ["https://allowed.example/@evil.example/list"]
        );
    }

    #[tokio::test]
    async fn add_on_media_list_rejects_invalid_chapters_before_repository_work() {
        for chapter_input in [
            "intro",
            "0:00",
            "0:61 Invalid",
            "0:00 First\n00:00 Duplicate",
            "2:00 Later\n1:59 Earlier",
        ] {
            let repository = RecordingAddonMediaListRepository::default();
            let error = add_addon_media_list(
                AddAddonMediaList {
                    addon_id: "org.ersatzrs.addon.fixture".to_owned(),
                    source_url: "https://example.test/episode".to_owned(),
                    folder_name: String::new(),
                    content_kind_override: ersatzrs_domain::AddonMediaListContentKind::Auto,
                    chapter_input: chapter_input.to_owned(),
                    detail_import_policy: ersatzrs_domain::AddonMediaListDetailPolicy::Automatic,
                },
                &repository,
            )
            .await
            .expect_err("invalid chapter input");
            assert!(error.to_string().contains("chapter"));
            assert!(repository
                .added_source_urls
                .lock()
                .expect("added source URLs")
                .is_empty());
        }
    }

    #[tokio::test]
    async fn changing_the_add_on_of_a_list_validates_before_touching_the_repository() {
        let repository = RecordingAddonMediaListRepository::default();
        for (id, addon_id) in [(0, "org.ersatzrs.addon.fixture"), (1, "  ")] {
            change_addon_media_list_addon(
                ChangeAddonMediaListAddon {
                    id,
                    addon_id: addon_id.to_owned(),
                },
                &repository,
            )
            .await
            .expect_err("invalid change request");
        }
        assert!(repository
            .changed_addons
            .lock()
            .expect("changed add-ons")
            .is_empty());

        change_addon_media_list_addon(
            ChangeAddonMediaListAddon {
                id: 7,
                addon_id: "  org.ersatzrs.addon.fixture  ".to_owned(),
            },
            &repository,
        )
        .await
        .expect("add-on change");
        assert_eq!(
            *repository.changed_addons.lock().expect("changed add-ons"),
            [(7, "org.ersatzrs.addon.fixture".to_owned())]
        );
    }

    #[tokio::test]
    async fn episode_number_assignment_validates_and_forwards_one_bounded_range() {
        let repository = RecordingAddonMediaListRepository::default();
        for command in [
            AssignAddonMediaListEpisodeNumbers {
                id: 0,
                season: 1,
                starting_episode: 1,
                item_count: None,
                missing_only: true,
            },
            AssignAddonMediaListEpisodeNumbers {
                id: 1,
                season: -1,
                starting_episode: 1,
                item_count: None,
                missing_only: true,
            },
            AssignAddonMediaListEpisodeNumbers {
                id: 1,
                season: 1,
                starting_episode: 0,
                item_count: None,
                missing_only: true,
            },
            AssignAddonMediaListEpisodeNumbers {
                id: 1,
                season: 1,
                starting_episode: 1,
                item_count: Some(0),
                missing_only: true,
            },
        ] {
            assign_addon_media_list_episode_numbers(command, &repository)
                .await
                .expect_err("invalid episode numbering request");
        }
        assert!(repository
            .episode_number_assignments
            .lock()
            .expect("episode number assignments")
            .is_empty());

        let assigned = assign_addon_media_list_episode_numbers(
            AssignAddonMediaListEpisodeNumbers {
                id: 7,
                season: 2,
                starting_episode: 4,
                item_count: Some(6),
                missing_only: true,
            },
            &repository,
        )
        .await
        .expect("valid episode numbering request");
        assert_eq!(assigned, 6);
        assert_eq!(
            *repository
                .episode_number_assignments
                .lock()
                .expect("episode number assignments"),
            [(7, 2, 4, Some(6), true)]
        );
    }

    // ── ADR 0064: HDR10 output validation ───────────────────────────────

    #[test]
    fn sdr_output_dynamic_range_is_always_valid() {
        for video_format in [0, 1, 2, 3, 4, 99] {
            for bit_depth in [0, 1] {
                assert!(
                    validate_output_dynamic_range(0, video_format, bit_depth).is_ok(),
                    "SDR must never be rejected ({video_format}, {bit_depth})"
                );
            }
        }
    }

    #[test]
    fn hdr10_requires_a_codec_that_can_carry_static_metadata() {
        // Hevc (2) and Av1 (4) are proven; h264 (1) and mpeg2video (3) are not.
        assert!(validate_output_dynamic_range(1, 2, 1).is_ok());
        assert!(validate_output_dynamic_range(1, 4, 1).is_ok());
        for rejected in [0, 1, 3, 99] {
            let error = validate_output_dynamic_range(1, rejected, 1)
                .expect_err("codec without an HDR10 metadata path must be rejected");
            assert!(
                format!("{error}").contains("hevc or av1"),
                "unexpected message for {rejected}: {error}"
            );
        }
    }

    #[test]
    fn hdr10_requires_ten_bit_output() {
        let error = validate_output_dynamic_range(1, 2, 0)
            .expect_err("8-bit HDR10 must be rejected, never silently downgraded");
        assert!(format!("{error}").contains("10-bit"), "{error}");
    }

    #[test]
    fn a_blank_hdr_override_selects_the_documented_default() {
        assert_eq!(
            parse_hdr_mastering_display("   ").expect("blank is not an error"),
            None,
            "blank must mean the documented default, not an empty override"
        );
        assert_eq!(
            parse_hdr_content_light(0, 0).expect("blank is not an error"),
            None
        );
    }

    #[test]
    fn an_unusable_hdr_override_is_rejected_at_save_time() {
        let error = parse_hdr_mastering_display("G(8500,39850)")
            .expect_err("a partial mastering display would fail every HDR10 spawn");
        assert!(format!("{error}").contains("Mastering display"), "{error}");

        // Half a CTA-861.3 pair would be silently completed from the default,
        // which is exactly the hidden value ADR 0064 forbids.
        let error = parse_hdr_content_light(1_000, 0)
            .expect_err("MaxCLL without MaxFALL must not be half-applied");
        assert!(format!("{error}").contains("MaxCLL"), "{error}");

        let error = parse_hdr_content_light(400, 1_000)
            .expect_err("MaxFALL above MaxCLL describes no real content");
        assert!(format!("{error}").contains("MaxFALL"), "{error}");
    }

    #[test]
    fn hdr_overrides_are_normalized_away_for_an_sdr_profile() {
        let mut form = existing_ffmpeg_profile_form();
        form.hdr_mastering_display = "this would never parse".to_owned();
        form.hdr_max_cll = 4_000;
        form.hdr_max_fall = 100;

        let (mastering_display, content_light) =
            normalized_hdr_metadata(OutputDynamicRange::SdrBt709, &form)
                .expect("an SDR profile must not be rejected for its dormant HDR fields");

        assert_eq!(mastering_display, None);
        assert_eq!(content_light, None);
    }

    #[test]
    fn an_hdr10_profile_keeps_the_operators_overrides() {
        let mut form = existing_ffmpeg_profile_form();
        form.hdr_mastering_display =
            "G(13250,34500)B(7500,3000)R(34000,16000)WP(15635,16450)L(10000000,50)".to_owned();
        form.hdr_max_cll = 4_000;
        form.hdr_max_fall = 100;

        let (mastering_display, content_light) =
            normalized_hdr_metadata(OutputDynamicRange::Hdr10Bt2020Pq, &form)
                .expect("a valid DCI-P3-style grade must be accepted");

        assert_eq!(
            mastering_display.expect("override kept").green_x,
            13_250,
            "the operator's own grade must survive, not be replaced by the default"
        );
        assert_eq!(
            content_light,
            ContentLightLevel::new(4_000, 100),
            "MaxCLL/MaxFALL must survive together"
        );
    }

    fn existing_ffmpeg_profile_form() -> FfmpegProfileAdminForm {
        FfmpegProfileAdminForm {
            id: 7,
            name: "Profile".to_owned(),
            thread_count: 0,
            transcode_video: false,
            transcode_audio: false,
            hardware_acceleration: 0,
            vaapi_display: String::new(),
            vaapi_driver: 0,
            vaapi_device: String::new(),
            qsv_extra_hardware_frames: 0,
            resolution_id: 1,
            scaling_behavior: 0,
            pad_mode: 0,
            video_format: 99,
            video_profile: String::new(),
            video_preset: String::new(),
            video_tune: String::new(),
            allow_b_frames: false,
            bit_depth: 8,
            video_bitrate: 0,
            video_buffer_size: 0,
            tonemap_algorithm: 0,
            output_dynamic_range: 0,
            hdr_mastering_display: String::new(),
            hdr_max_cll: 0,
            hdr_max_fall: 0,
            audio_format: 99,
            audio_bitrate: 0,
            audio_buffer_size: 0,
            normalize_loudness_mode: 0,
            target_loudness: String::new(),
            audio_channels: 2,
            audio_sample_rate: 48_000,
            normalize_framerate: false,
            normalize_colors: false,
            deinterlace_video: false,
        }
    }

    #[tokio::test]
    async fn invalid_logging_settings_do_not_touch_persistence_or_runtime() {
        let repository = RecordingSettingsRepository::default();
        let runtime = RecordingSettingsRuntime::default();

        let result =
            save_logging_settings(logging_settings("Verbose"), &repository, &runtime).await;

        assert!(matches!(
            result,
            Err(crate::ApplicationError::Validation { .. })
        ));
        assert!(repository
            .batches
            .lock()
            .expect("settings batches")
            .is_empty());
        assert!(runtime
            .events
            .lock()
            .expect("settings runtime events")
            .is_empty());
    }

    #[tokio::test]
    async fn logging_settings_use_one_atomic_batch_before_runtime_reload() {
        let repository = RecordingSettingsRepository::default();
        let runtime = RecordingSettingsRuntime::default();

        save_logging_settings(logging_settings("Warning"), &repository, &runtime)
            .await
            .expect("save logging settings");

        let batches = repository.batches.lock().expect("settings batches");
        assert_eq!(batches.len(), 1);
        assert_eq!(batches[0].len(), 6);
        assert_eq!(
            *runtime.events.lock().expect("settings runtime events"),
            ["reload_logging"]
        );
    }

    #[tokio::test]
    async fn addon_detail_wait_settings_validate_before_one_atomic_batch() {
        let repository = RecordingSettingsRepository::default();
        save_addon_detail_wait_settings(
            SaveAddonDetailWaitSettings {
                minimum_seconds: 7,
                maximum_seconds: 11,
            },
            &repository,
        )
        .await
        .expect("save add-on detail wait settings");

        assert_eq!(
            repository
                .values
                .lock()
                .expect("settings values")
                .get(ersatzrs_domain::CONFIG_KEY_ADDON_DETAIL_WAIT_MINIMUM_SECONDS)
                .map(String::as_str),
            Some("7")
        );
        assert_eq!(
            repository
                .values
                .lock()
                .expect("settings values")
                .get(ersatzrs_domain::CONFIG_KEY_ADDON_DETAIL_WAIT_MAXIMUM_SECONDS)
                .map(String::as_str),
            Some("11")
        );
        assert_eq!(
            repository.batches.lock().expect("settings batches").len(),
            1
        );
        assert_eq!(
            crate::queries::load_addon_detail_wait_settings(&repository)
                .await
                .expect("round-trip add-on detail wait settings"),
            crate::queries::AddonDetailWaitSettings {
                minimum_seconds: 7,
                maximum_seconds: 11,
            }
        );

        let invalid = save_addon_detail_wait_settings(
            SaveAddonDetailWaitSettings {
                minimum_seconds: 12,
                maximum_seconds: 11,
            },
            &repository,
        )
        .await;
        assert!(matches!(
            invalid,
            Err(crate::ApplicationError::Validation { .. })
        ));
        assert_eq!(
            repository.batches.lock().expect("settings batches").len(),
            1,
            "an invalid save must preserve the last valid range"
        );
    }

    #[test]
    fn windows_startup_uses_the_settings_runtime_port() {
        let runtime = RecordingSettingsRuntime::default();

        set_windows_startup(true, &runtime).expect("enable Windows startup");
        set_windows_startup(false, &runtime).expect("disable Windows startup");

        assert_eq!(
            *runtime.events.lock().expect("settings runtime events"),
            ["windows-startup:true", "windows-startup:false"]
        );
    }

    #[tokio::test]
    async fn automatic_settings_rebuild_reports_unavailable_playout_worker() {
        let repository = RecordingSettingsRepository {
            playout_ids: vec![5],
            ..RecordingSettingsRepository::default()
        };

        let error = save_playout_settings(
            SavePlayoutSettings {
                days_to_build: 2,
                skip_missing_items: false,
                scripted_schedule_timeout_seconds: 30,
            },
            &repository,
            &RecordingQueue::unavailable(),
            &coordinator(),
        )
        .await
        .unwrap_err();

        assert!(matches!(
            error,
            crate::ApplicationError::Unavailable {
                operation: "queue playout build",
                ..
            }
        ));
        assert_eq!(
            repository.batches.lock().expect("settings batches").len(),
            1,
            "settings are persisted before their rebuild side effect"
        );
    }

    #[tokio::test]
    async fn ffmpeg_settings_normalize_subtitle_extraction_and_optional_ids() {
        let repository = RecordingSettingsRepository {
            values: Mutex::new(BTreeMap::from([
                (
                    ersatzrs_domain::CONFIG_KEY_FFMPEG_PATH.to_owned(),
                    "managed-ffmpeg".to_owned(),
                ),
                (
                    ersatzrs_domain::CONFIG_KEY_FFPROBE_PATH.to_owned(),
                    "managed-ffprobe".to_owned(),
                ),
                (
                    ersatzrs_domain::CONFIG_KEY_FFMPEG_PATHS_MANAGED.to_owned(),
                    "true".to_owned(),
                ),
                (
                    ersatzrs_domain::CONFIG_KEY_FFMPEG_GLOBAL_WATERMARK_ID.to_owned(),
                    "12".to_owned(),
                ),
                (
                    ersatzrs_domain::CONFIG_KEY_FFMPEG_GLOBAL_FALLBACK_FILLER_ID.to_owned(),
                    "18".to_owned(),
                ),
            ])),
            batches: Mutex::default(),
            playout_ids: vec![2, 3],
        };
        let runtime = RecordingSettingsRuntime::default();

        save_ffmpeg_settings(ffmpeg_settings(), &repository, &runtime)
            .await
            .expect("save FFmpeg settings");

        let mutations = runtime
            .committed_mutations
            .lock()
            .expect("committed settings mutations");
        assert!(mutations.iter().any(|mutation| matches!(
            mutation,
            SettingsMutation::Set { key, value }
                if key == ersatzrs_domain::CONFIG_KEY_FFMPEG_EXTRACT_EMBEDDED_SUBTITLES
                    && value == "False"
        )));
        assert!(mutations.iter().any(|mutation| matches!(
            mutation,
            SettingsMutation::Set { key, value }
                if key == ersatzrs_domain::CONFIG_KEY_FFMPEG_PATHS_MANAGED && value == "false"
        )));
        for key in [
            ersatzrs_domain::CONFIG_KEY_FFMPEG_GLOBAL_WATERMARK_ID,
            ersatzrs_domain::CONFIG_KEY_FFMPEG_GLOBAL_FALLBACK_FILLER_ID,
        ] {
            assert!(mutations.iter().any(|mutation| matches!(
                mutation,
                SettingsMutation::Delete { key: deleted } if deleted == key
            )));
        }
        drop(mutations);
        assert!(repository
            .batches
            .lock()
            .expect("settings batches")
            .is_empty());
        assert_eq!(
            *runtime.events.lock().expect("settings runtime events"),
            [
                "validate:ffmpeg-custom:ffprobe-custom",
                "commit:resolved-ffmpeg:resolved-ffprobe"
            ]
        );
    }

    #[tokio::test]
    async fn ffmpeg_settings_keep_unchanged_managed_paths_updateable() {
        let repository = RecordingSettingsRepository {
            values: Mutex::new(BTreeMap::from([
                (
                    ersatzrs_domain::CONFIG_KEY_FFMPEG_PATH.to_owned(),
                    "ffmpeg-custom".to_owned(),
                ),
                (
                    ersatzrs_domain::CONFIG_KEY_FFPROBE_PATH.to_owned(),
                    "ffprobe-custom".to_owned(),
                ),
                (
                    ersatzrs_domain::CONFIG_KEY_FFMPEG_PATHS_MANAGED.to_owned(),
                    "True".to_owned(),
                ),
            ])),
            ..RecordingSettingsRepository::default()
        };

        let runtime = RecordingSettingsRuntime::default();
        save_ffmpeg_settings(ffmpeg_settings(), &repository, &runtime)
            .await
            .expect("save unchanged managed FFmpeg settings");

        assert!(runtime
            .committed_mutations
            .lock()
            .expect("committed settings mutations")
            .iter()
            .any(|mutation| matches!(
                mutation,
                SettingsMutation::Set { key, value }
                    if key == ersatzrs_domain::CONFIG_KEY_FFMPEG_PATHS_MANAGED
                        && value == "true"
            )));
    }

    #[tokio::test]
    async fn failed_ffmpeg_commit_keeps_persistence_and_side_effects_untouched() {
        let repository = RecordingSettingsRepository {
            playout_ids: vec![2, 3],
            ..RecordingSettingsRepository::default()
        };
        let runtime = RecordingSettingsRuntime {
            fail_ffmpeg_commit: true,
            ..RecordingSettingsRuntime::default()
        };
        let mut settings = ffmpeg_settings();
        settings.save_reports = true;
        settings.use_embedded_subtitles = true;
        settings.extract_embedded_subtitles = true;

        let error = save_ffmpeg_settings(settings, &repository, &runtime)
            .await
            .expect_err("commit failure");

        assert!(matches!(
            error,
            crate::ApplicationError::Unavailable {
                operation: "commit FFmpeg settings",
                ..
            }
        ));
        assert!(repository
            .batches
            .lock()
            .expect("settings batches")
            .is_empty());
        assert!(runtime
            .committed_mutations
            .lock()
            .expect("committed settings mutations")
            .is_empty());
        assert_eq!(
            *runtime.events.lock().expect("settings runtime events"),
            [
                "validate:ffmpeg-custom:ffprobe-custom",
                "commit:resolved-ffmpeg:resolved-ffprobe"
            ]
        );
    }

    #[test]
    fn legacy_nvenc_presets_split_into_ffmpeg_nine_preset_and_tune() {
        assert_eq!(normalize_legacy_nvenc_preset("llhp"), "p1");
        assert_eq!(legacy_nvenc_tune("llhp"), Some("ll"));
        assert_eq!(normalize_legacy_nvenc_preset("LLHQ"), "p7");
        assert_eq!(legacy_nvenc_tune("LLHQ"), Some("ll"));
    }

    #[tokio::test]
    async fn ffmpeg_profile_lookup_preserves_typed_repository_failures() {
        for (optional, expected_unavailable) in [
            (OptionalFfmpegProfile::DecodeFailure, false),
            (OptionalFfmpegProfile::Unavailable, true),
        ] {
            let error = save_ffmpeg_profile(
                SaveFfmpegProfile {
                    form: existing_ffmpeg_profile_form(),
                },
                &FailingFfmpegRepository { optional },
            )
            .await
            .unwrap_err();
            assert_eq!(
                matches!(error, crate::ApplicationError::Unavailable { .. }),
                expected_unavailable
            );
            assert!(matches!(
                error,
                crate::ApplicationError::InvalidData { .. }
                    | crate::ApplicationError::Unavailable { .. }
            ));
        }

        let error = save_ffmpeg_profile(
            SaveFfmpegProfile {
                form: existing_ffmpeg_profile_form(),
            },
            &FailingFfmpegRepository {
                optional: OptionalFfmpegProfile::Missing,
            },
        )
        .await
        .unwrap_err();
        assert!(matches!(
            error,
            crate::ApplicationError::NotFound {
                kind: "ffmpeg profile",
                id: 7
            }
        ));
    }

    #[tokio::test]
    async fn collection_save_validation_precedes_persistence() {
        let repository = RecordingCollectionRepository {
            duplicate_name: true,
            ..RecordingCollectionRepository::default()
        };

        let result = save_collection(
            SaveCollection {
                id: 0,
                name: "  Existing  ".to_owned(),
            },
            &repository,
        )
        .await;

        assert!(matches!(
            result,
            Err(crate::ApplicationError::Validation { .. })
        ));
        assert!(repository
            .saved
            .lock()
            .expect("saved collections")
            .is_empty());
    }

    #[tokio::test]
    async fn collection_media_action_validates_identities_before_persistence() {
        let missing_media = RecordingCollectionRepository {
            missing_media_items: true,
            ..RecordingCollectionRepository::default()
        };
        let error = add_media_item_to_collection(
            AddMediaItemToCollection {
                media_item_id: 99,
                collection_id: None,
                new_name: Some("Should not persist".to_owned()),
            },
            &missing_media,
            &RecordingQueue::default(),
            &coordinator(),
        )
        .await
        .unwrap_err();
        assert!(matches!(
            error,
            crate::ApplicationError::Validation {
                field: "media_item_id",
                ..
            }
        ));
        assert!(missing_media
            .saved
            .lock()
            .expect("saved collections")
            .is_empty());

        let stale_collection = RecordingCollectionRepository::default();
        let error = add_media_item_to_collection(
            AddMediaItemToCollection {
                media_item_id: 8,
                collection_id: Some(404),
                new_name: None,
            },
            &stale_collection,
            &RecordingQueue::default(),
            &coordinator(),
        )
        .await
        .unwrap_err();
        assert!(matches!(
            error,
            crate::ApplicationError::Validation {
                field: "collection_id",
                ..
            }
        ));
        assert!(stale_collection
            .added
            .lock()
            .expect("added collection items")
            .is_empty());
    }

    #[tokio::test]
    async fn collection_media_action_creates_collection_with_first_item_as_one_mutation() {
        let repository = RecordingCollectionRepository::default();

        let id = add_media_item_to_collection(
            AddMediaItemToCollection {
                media_item_id: 8,
                collection_id: None,
                new_name: Some("  New collection  ".to_owned()),
            },
            &repository,
            &RecordingQueue::default(),
            &coordinator(),
        )
        .await
        .expect("create collection with item");

        assert_eq!(id, 11);
        assert_eq!(
            *repository.saved.lock().expect("saved collections"),
            [(0, "New collection".to_owned())]
        );
        assert_eq!(
            *repository.added.lock().expect("added collection items"),
            [(11, 8)]
        );
    }

    #[tokio::test]
    async fn collection_item_add_refreshes_each_affected_playout() {
        let repository = RecordingCollectionRepository {
            collection_ids: std::collections::BTreeSet::from([3]),
            playout_ids: vec![4, 9],
            ..RecordingCollectionRepository::default()
        };
        let queue = RecordingQueue::default();
        let operations = coordinator();

        add_default_collection_item(
            ChangeCollectionItem {
                collection_id: 3,
                media_item_id: 8,
            },
            &repository,
            &queue,
            &operations,
        )
        .await
        .expect("add collection item");

        assert_eq!(
            *repository.added.lock().expect("added collection items"),
            [(3, 8)]
        );
        assert_eq!(
            *queue.requests.lock().expect("request lock"),
            [
                PlayoutBuildRequest {
                    playout_id: 4,
                    mode: PlayoutBuildMode::Refresh,
                },
                PlayoutBuildRequest {
                    playout_id: 9,
                    mode: PlayoutBuildMode::Refresh,
                }
            ]
        );
    }

    #[tokio::test]
    async fn collection_refresh_reports_unavailable_playout_worker() {
        let repository = RecordingCollectionRepository {
            collection_ids: std::collections::BTreeSet::from([3]),
            playout_ids: vec![4],
            ..RecordingCollectionRepository::default()
        };

        let error = add_default_collection_item(
            ChangeCollectionItem {
                collection_id: 3,
                media_item_id: 8,
            },
            &repository,
            &RecordingQueue::unavailable(),
            &coordinator(),
        )
        .await
        .unwrap_err();

        assert!(matches!(
            error,
            crate::ApplicationError::Unavailable {
                operation: "queue playout build",
                ..
            }
        ));
    }

    #[tokio::test]
    async fn smart_collection_update_refreshes_each_affected_playout() {
        let repository = RecordingCollectionRepository {
            playout_ids: vec![5, 6],
            ..RecordingCollectionRepository::default()
        };
        let queue = RecordingQueue::default();
        let operations = coordinator();

        let id = save_smart_collection(
            SaveSmartCollection {
                id: 7,
                name: "  Dynamic  ".to_owned(),
                query: "  title contains movie  ".to_owned(),
            },
            &repository,
            &queue,
            &operations,
        )
        .await
        .expect("save smart collection");

        assert_eq!(id, 7);
        assert_eq!(
            *repository.saved.lock().expect("saved collections"),
            [(7, "Dynamic:title contains movie".to_owned())]
        );
        assert_eq!(
            *queue.requests.lock().expect("request lock"),
            [
                PlayoutBuildRequest {
                    playout_id: 5,
                    mode: PlayoutBuildMode::Refresh,
                },
                PlayoutBuildRequest {
                    playout_id: 6,
                    mode: PlayoutBuildMode::Refresh,
                }
            ]
        );
    }

    #[tokio::test]
    async fn multi_collection_update_normalizes_items_and_refreshes_on_change() {
        let repository = RecordingCollectionRepository {
            collection_ids: std::collections::BTreeSet::from([2]),
            multi: Some(MultiCollectionAdmin {
                id: 7,
                name: "Mixed".to_owned(),
                items: Vec::new(),
            }),
            playout_ids: vec![12],
            ..RecordingCollectionRepository::default()
        };
        let queue = RecordingQueue::default();
        let operations = coordinator();

        let id = save_multi_collection(
            SaveMultiCollection {
                id: 7,
                name: "  Mixed  ".to_owned(),
                items: vec![MultiCollectionItem {
                    collection_id: Some(2),
                    smart_collection_id: None,
                    name: "Display only".to_owned(),
                    collection_type: 99,
                    use_custom_playback_order: true,
                    schedule_as_group: true,
                    playback_order: 0,
                }],
            },
            &repository,
            &queue,
            &operations,
        )
        .await
        .expect("save multi collection");

        assert_eq!(id, 7);
        let saves = repository
            .multi_saves
            .lock()
            .expect("multi collection saves");
        assert!(saves[0].0);
        assert_eq!(saves[0].1[0].name, "");
        assert_eq!(saves[0].1[0].collection_type, 0);
        assert_eq!(saves[0].1[0].playback_order, 1);
        assert!(!saves[0].1[0].use_custom_playback_order);
        assert_eq!(
            *queue.requests.lock().expect("request lock"),
            [PlayoutBuildRequest {
                playout_id: 12,
                mode: PlayoutBuildMode::Refresh,
            }]
        );
    }

    #[tokio::test]
    async fn rerun_collection_update_refreshes_only_when_persisted_form_changes() {
        let existing = RerunCollectionAdmin {
            id: 8,
            name: "Old".to_owned(),
            collection_type: 6,
            collection_id: None,
            media_item_id: None,
            multi_collection_id: None,
            smart_collection_id: None,
            first_run_playback_order: 1,
            rerun_playback_order: 1,
        };
        let repository = RecordingCollectionRepository {
            rerun: Some(existing.clone()),
            playout_ids: vec![15],
            ..RecordingCollectionRepository::default()
        };
        let queue = RecordingQueue::default();
        let operations = coordinator();
        let mut updated = existing;
        updated.name = "Updated".to_owned();

        let id = save_rerun_collection(
            SaveRerunCollection {
                form: updated.clone(),
            },
            &repository,
            &queue,
            &operations,
        )
        .await
        .expect("save rerun collection");

        assert_eq!(id, 8);
        assert_eq!(
            *repository
                .rerun_saves
                .lock()
                .expect("rerun collection saves"),
            [updated]
        );
        assert_eq!(
            *queue.requests.lock().expect("request lock"),
            [PlayoutBuildRequest {
                playout_id: 15,
                mode: PlayoutBuildMode::Refresh,
            }]
        );
    }

    #[tokio::test]
    async fn playlist_group_validation_precedes_persistence_and_normalizes_name() {
        let repository = RecordingCollectionRepository::default();

        let error = create_playlist_group(
            CreatePlaylistGroup {
                name: " ".to_owned(),
            },
            &repository,
        )
        .await
        .unwrap_err();
        assert!(matches!(
            error,
            crate::ApplicationError::Validation { field: "name", .. }
        ));
        assert!(repository
            .saved
            .lock()
            .expect("saved collections")
            .is_empty());

        let group = create_playlist_group(
            CreatePlaylistGroup {
                name: "  Favorites  ".to_owned(),
            },
            &repository,
        )
        .await
        .expect("create playlist group");
        assert_eq!(group.name, "Favorites");
    }

    #[tokio::test]
    async fn playlist_item_validation_precedes_transactional_save() {
        let persisted = PlaylistEditorAdmin {
            id: 9,
            playlist_group_id: 2,
            name: "Playlist".to_owned(),
            is_system: false,
            items: Vec::new(),
        };
        let repository = RecordingCollectionRepository {
            playlist: Some(persisted.clone()),
            ..RecordingCollectionRepository::default()
        };
        let mut invalid = persisted;
        invalid.items.push(PlaylistItemAdmin {
            id: 0,
            index: 0,
            collection_type: 10,
            collection_id: None,
            multi_collection_id: None,
            smart_collection_id: None,
            media_item_id: None,
            item_name: String::new(),
            playback_order: 1,
            count: None,
            play_all: false,
            include_in_program_guide: true,
            preferred_audio_language_code: None,
            preferred_audio_title: None,
            preferred_subtitle_language_code: None,
            subtitle_mode: None,
            pinned_stream_selection: None,
        });

        let error = save_playlist(SavePlaylist { form: invalid }, &repository)
            .await
            .unwrap_err();
        assert!(matches!(
            error,
            crate::ApplicationError::Validation {
                field: "media_item",
                ..
            }
        ));
        assert!(repository
            .playlist_saves
            .lock()
            .expect("playlist saves")
            .is_empty());
    }

    #[tokio::test]
    async fn system_playlist_delete_is_rejected_before_repository_mutation() {
        let repository = RecordingCollectionRepository {
            playlist: Some(PlaylistEditorAdmin {
                id: 9,
                playlist_group_id: 2,
                name: "Generated".to_owned(),
                is_system: true,
                items: Vec::new(),
            }),
            ..RecordingCollectionRepository::default()
        };

        let error = delete_playlist(DeletePlaylist { id: 9 }, &repository)
            .await
            .unwrap_err();
        assert!(matches!(
            error,
            crate::ApplicationError::Validation {
                field: "playlist_id",
                ..
            }
        ));
    }

    #[test]
    fn accepts_refresh_and_reset_requests_without_changing_mode() {
        let queue = RecordingQueue::default();
        let operations = coordinator();

        assert_eq!(
            request_playout_build(
                RequestPlayoutBuild::new(7, PlayoutBuildMode::Refresh),
                &queue,
                &operations,
            ),
            RequestPlayoutBuildOutcome::Accepted
        );
        operations.playout_activity().finish(7);
        assert_eq!(
            request_playout_build(
                RequestPlayoutBuild::new(7, PlayoutBuildMode::Reset),
                &queue,
                &operations,
            ),
            RequestPlayoutBuildOutcome::Accepted
        );

        assert_eq!(
            *queue.requests.lock().expect("request lock"),
            [
                PlayoutBuildRequest {
                    playout_id: 7,
                    mode: PlayoutBuildMode::Refresh,
                },
                PlayoutBuildRequest {
                    playout_id: 7,
                    mode: PlayoutBuildMode::Reset,
                },
            ]
        );
    }

    #[test]
    fn upgrades_queued_mode_and_retains_one_active_follow_up() {
        let queue = RecordingQueue::default();
        let operations = coordinator();
        let continue_command = RequestPlayoutBuild::new(8, PlayoutBuildMode::Continue);

        assert_eq!(
            request_playout_build(continue_command, &queue, &operations),
            RequestPlayoutBuildOutcome::Accepted
        );
        assert_eq!(
            request_playout_build(
                RequestPlayoutBuild::new(8, PlayoutBuildMode::Reset),
                &queue,
                &operations,
            ),
            RequestPlayoutBuildOutcome::AlreadyQueued
        );
        assert_eq!(queue.requests.lock().expect("request lock").len(), 1);

        let first_token = queue.requests.lock().expect("request lock")[0];
        let (first_request, first_guard) = operations.playout_activity().start_guard(first_token);
        assert_eq!(first_request.mode, PlayoutBuildMode::Reset);
        assert_eq!(
            request_playout_build(
                RequestPlayoutBuild::new(8, PlayoutBuildMode::Refresh),
                &queue,
                &operations,
            ),
            RequestPlayoutBuildOutcome::AlreadyBuilding
        );
        assert_eq!(
            request_playout_build(
                RequestPlayoutBuild::new(8, PlayoutBuildMode::Reset),
                &queue,
                &operations,
            ),
            RequestPlayoutBuildOutcome::AlreadyBuilding
        );
        assert_eq!(
            queue.requests.lock().expect("request lock").len(),
            2,
            "active mutations must share one follow-up token"
        );

        drop(first_guard);
        let follow_up_token = queue.requests.lock().expect("request lock")[1];
        let (follow_up, follow_up_guard) =
            operations.playout_activity().start_guard(follow_up_token);
        assert_eq!(follow_up.mode, PlayoutBuildMode::Reset);
        drop(follow_up_guard);
        assert!(operations.playout_activity().queued_playouts().is_empty());
        assert!(operations.playout_activity().building_playouts().is_empty());
    }

    #[test]
    fn preserves_fifo_admission_order_across_playouts() {
        let queue = RecordingQueue::default();
        let operations = coordinator();

        for playout_id in [30, 10, 20] {
            assert_eq!(
                request_playout_build(
                    RequestPlayoutBuild::new(playout_id, PlayoutBuildMode::Continue),
                    &queue,
                    &operations,
                ),
                RequestPlayoutBuildOutcome::Accepted
            );
        }

        let ids = queue
            .requests
            .lock()
            .expect("request lock")
            .iter()
            .map(|request| request.playout_id)
            .collect::<Vec<_>>();
        assert_eq!(ids, [30, 10, 20]);
    }

    #[test]
    fn unavailable_worker_releases_admission_for_retry() {
        let queue = RecordingQueue::unavailable();
        let operations = coordinator();
        let command = RequestPlayoutBuild::new(9, PlayoutBuildMode::Reset);

        assert_eq!(
            request_playout_build(command, &queue, &operations),
            RequestPlayoutBuildOutcome::Unavailable {
                message: "worker closed".to_owned(),
            }
        );
        assert!(operations.playout_activity().queued_playouts().is_empty());
        assert!(matches!(
            request_playout_build(command, &queue, &operations),
            RequestPlayoutBuildOutcome::Unavailable { .. }
        ));
    }

    #[tokio::test]
    async fn failed_initial_playout_admission_removes_unbuilt_playout() {
        let deleted = std::sync::atomic::AtomicBool::new(false);
        let result = finalize_initial_playout_build(
            12,
            RequestPlayoutBuildOutcome::Unavailable {
                message: "worker closed".to_owned(),
            },
            async {
                deleted.store(true, std::sync::atomic::Ordering::SeqCst);
                Ok(())
            },
        )
        .await;

        assert!(deleted.load(std::sync::atomic::Ordering::SeqCst));
        assert!(matches!(
            result,
            Err(crate::ApplicationError::Unavailable {
                operation: "queue playout build",
                ..
            })
        ));
    }

    #[tokio::test]
    async fn failed_initial_playout_cleanup_preserves_both_diagnostics() {
        let error = finalize_initial_playout_build(
            12,
            RequestPlayoutBuildOutcome::Unavailable {
                message: "worker closed".to_owned(),
            },
            async {
                Err(RepositoryError::unavailable(
                    "delete playout",
                    "database offline",
                ))
            },
        )
        .await
        .unwrap_err();

        assert!(matches!(
            error,
            crate::ApplicationError::Unavailable {
                operation: "create playout",
                ref message
            } if message.contains("worker closed") && message.contains("database offline")
        ));
    }

    #[test]
    fn rejects_invalid_identity_without_touching_queue_or_activity() {
        let queue = RecordingQueue::default();
        let operations = coordinator();

        assert_eq!(
            request_playout_build(
                RequestPlayoutBuild::new(0, PlayoutBuildMode::Reset),
                &queue,
                &operations,
            ),
            RequestPlayoutBuildOutcome::Rejected(RequestPlayoutBuildRejection::InvalidPlayoutId)
        );
        assert!(queue.requests.lock().expect("request lock").is_empty());
        assert!(operations.playout_activity().queued_playouts().is_empty());
    }

    #[tokio::test]
    async fn scan_admission_preserves_fifo_and_repeated_requests() {
        let repository = FakeLibraryRepository::with_targets([
            scan_target(2, MediaSourceKind::Plex),
            scan_target(1, MediaSourceKind::Local),
        ]);
        let managed = RecordingManagedOperations::default();
        let queue = RecordingScanQueue::default();
        let operations = coordinator();

        for library_id in [2, 1, 2] {
            assert!(matches!(
                request_library_scan(
                    RequestLibraryScan::new(library_id, false),
                    &repository,
                    &managed,
                    &queue,
                    &operations,
                )
                .await,
                RequestScanOutcome::Accepted { .. }
            ));
        }

        let ids = queue
            .requests
            .lock()
            .expect("scan requests")
            .iter()
            .map(|work| work.request.library_id())
            .collect::<Vec<_>>();
        assert_eq!(ids, [2, 1, 2]);
        assert_eq!(operations.scan_activity().active_library_ids(), [1, 2]);
    }

    #[tokio::test]
    async fn scheduled_scan_coalesces_active_work_without_dropping_manual_repeats() {
        let repository =
            FakeLibraryRepository::with_targets([scan_target(6, MediaSourceKind::Plex)]);
        let managed = RecordingManagedOperations::default();
        let queue = RecordingScanQueue::default();
        let operations = coordinator();

        assert!(matches!(
            request_library_scan(
                RequestLibraryScan::new(6, false),
                &repository,
                &managed,
                &queue,
                &operations,
            )
            .await,
            RequestScanOutcome::Accepted { .. }
        ));
        assert_eq!(
            request_scheduled_library_scan(
                RequestScheduledLibraryScan::new(6),
                &repository,
                &managed,
                &queue,
                &operations,
            )
            .await,
            RequestScheduledScanOutcome::AlreadyActive
        );
        assert_eq!(
            queue.requests.lock().expect("scan requests").len(),
            1,
            "periodic cadence must not append duplicate active work"
        );

        assert!(matches!(
            request_library_scan(
                RequestLibraryScan::new(6, true),
                &repository,
                &managed,
                &queue,
                &operations,
            )
            .await,
            RequestScanOutcome::Accepted { .. }
        ));
        assert_eq!(
            queue.requests.lock().expect("scan requests").len(),
            2,
            "manual repeated requests retain FIFO admission"
        );
    }

    #[tokio::test]
    async fn show_admission_uses_persisted_provider_and_local_fallback() {
        let repository = FakeLibraryRepository::with_targets([
            scan_target(3, MediaSourceKind::Local),
            scan_target(4, MediaSourceKind::Jellyfin),
        ]);
        let managed = RecordingManagedOperations::default();
        let queue = RecordingScanQueue::default();
        let operations = coordinator();

        assert!(matches!(
            request_show_scan(
                RequestShowScan::new(3, 30, true),
                &repository,
                &managed,
                &queue,
                &operations,
            )
            .await,
            RequestScanOutcome::Accepted { .. }
        ));
        assert!(matches!(
            request_show_scan(
                RequestShowScan::new(4, 40, false),
                &repository,
                &managed,
                &queue,
                &operations,
            )
            .await,
            RequestScanOutcome::Accepted { .. }
        ));

        let requests = queue.requests.lock().expect("scan requests");
        assert_eq!(
            requests[0].request,
            ScanRequest::Library {
                provider: MediaSourceKind::Local,
                library_id: 3,
                deep_scan: true,
            }
        );
        assert_eq!(
            requests[1].request,
            ScanRequest::Show {
                provider: MediaSourceKind::Jellyfin,
                library_id: 4,
                show_id: 40,
                deep_scan: false,
            }
        );
    }

    #[tokio::test]
    async fn scan_admission_validates_before_persistence_and_survives_wake_failure() {
        let mut disabled = scan_target(5, MediaSourceKind::Emby);
        disabled.should_sync_items = false;
        let repository = FakeLibraryRepository::with_targets([disabled]);
        let managed = RecordingManagedOperations::default();
        let queue = RecordingScanQueue::unavailable();
        let operations = coordinator();

        assert_eq!(
            request_library_scan(
                RequestLibraryScan::new(0, false),
                &repository,
                &managed,
                &queue,
                &operations,
            )
            .await,
            RequestScanOutcome::Rejected(RequestScanRejection::InvalidLibraryId)
        );
        assert_eq!(
            request_library_scan(
                RequestLibraryScan::new(5, false),
                &repository,
                &managed,
                &queue,
                &operations,
            )
            .await,
            RequestScanOutcome::Rejected(RequestScanRejection::SynchronizationDisabled)
        );

        let available_repository =
            FakeLibraryRepository::with_targets([scan_target(6, MediaSourceKind::Plex)]);
        assert!(matches!(
            request_library_scan(
                RequestLibraryScan::new(6, false),
                &available_repository,
                &managed,
                &queue,
                &operations,
            )
            .await,
            RequestScanOutcome::Accepted { .. }
        ));
        assert_eq!(operations.scan_activity().snapshots().len(), 1);
    }

    fn channel_validation_state() -> ChannelValidationState {
        ChannelValidationState {
            channels: vec![
                ChannelValidationRow {
                    id: 1,
                    number: "1".to_owned(),
                    name: "First".to_owned(),
                    playout_source: 0,
                    playout_count: 1,
                },
                ChannelValidationRow {
                    id: 2,
                    number: "2".to_owned(),
                    name: "Mirror".to_owned(),
                    playout_source: 1,
                    playout_count: 0,
                },
            ],
            ffmpeg_profile_ids: vec![10],
            watermark_ids: vec![20],
            fallback_filler_ids: vec![30],
        }
    }

    #[test]
    fn channel_validation_and_normalization_are_application_owned() {
        let state = channel_validation_state();
        let form = normalize_channel_form(
            ChannelEditorForm {
                id: 1,
                number: " 1 ".to_owned(),
                name: " Existing ".to_owned(),
                group: " Group ".to_owned(),
                ffmpeg_profile_id: 10,
                playout_source: 1,
                mirror_source_channel_id: Some(2),
                playout_offset: Some(chrono::Duration::hours(1)),
                is_enabled: true,
                show_in_epg: true,
                ..ChannelEditorForm::default()
            },
            &state,
        );

        assert_eq!(form.number, "1");
        assert_eq!(form.name, "Existing");
        assert_eq!(form.group, "Group");
        assert_eq!(form.playout_source, 0, "an existing playout wins");
        assert_eq!(form.mirror_source_channel_id, None);
        assert!(validate_channel_form(&form, &state).is_empty());

        let invalid = ChannelEditorForm {
            number: "1.234".to_owned(),
            name: String::new(),
            group: String::new(),
            ffmpeg_profile_id: 99,
            external_logo_url: Some("file:///tmp/logo.png".to_owned()),
            is_enabled: false,
            show_in_epg: true,
            ..ChannelEditorForm::default()
        };
        let fields = validate_channel_form(&invalid, &state)
            .into_iter()
            .map(|error| error.field)
            .collect::<Vec<_>>();
        assert_eq!(
            fields,
            [
                "number",
                "name",
                "group",
                "ffmpeg_profile_id",
                "external_logo_url",
                "show_in_epg",
            ]
        );
    }

    #[test]
    fn channel_number_validation_supports_atomic_swaps_and_rejects_conflicts() {
        let state = channel_validation_state();
        let swap = normalize_channel_number_updates(vec![
            ChannelNumberUpdate {
                id: 1,
                number: " 2 ".to_owned(),
            },
            ChannelNumberUpdate {
                id: 2,
                number: "1".to_owned(),
            },
        ]);
        assert!(validate_channel_number_updates(&swap, &state).is_empty());

        let conflict = [ChannelNumberUpdate {
            id: 1,
            number: "2".to_owned(),
        }];
        let errors = validate_channel_number_updates(&conflict, &state);
        assert_eq!(errors.len(), 1);
        assert_eq!(errors[0].field, "number_1");
    }

    #[test]
    fn addon_media_list_identity_validation_stays_in_the_application_layer() {
        let error = super::validate_positive_id(0, "addon_media_list_id").unwrap_err();
        assert!(matches!(
            error,
            crate::ApplicationError::Validation {
                field: "addon_media_list_id",
                ..
            }
        ));
        assert!(super::validate_positive_id(42, "addon_media_list_id").is_ok());
    }

    #[test]
    fn schedule_item_validation_and_dependency_normalization_are_application_owned() {
        let mut playlist = test_schedule_item();
        playlist.collection_type = 6;
        playlist.playlist_id = Some(12);
        playlist.multiple_mode = 2;
        playlist.playback_order = 3;
        normalize_schedule_item(&mut playlist);
        assert_eq!(playlist.playback_order, 0);

        let mut fixed = test_schedule_item();
        fixed.start_type = 2;
        fixed.start_time = None;
        let error =
            validate_schedule_items(&[fixed], &std::collections::BTreeSet::new()).unwrap_err();
        assert!(matches!(
            error,
            crate::ApplicationError::Validation {
                field: "start_time",
                ..
            }
        ));
    }

    #[test]
    fn watermark_validation_and_dependency_normalization_are_application_owned() {
        let form = WatermarkAdminForm {
            id: 0,
            name: "Station Bug".to_owned(),
            mode: 1,
            image_source: 0,
            image: Some("station.png".to_owned()),
            original_content_type: Some("image/png".to_owned()),
            location: 0,
            size: 0,
            width_percent: 15.0,
            horizontal_margin_percent: 5.0,
            vertical_margin_percent: 5.0,
            frequency_minutes: 15,
            duration_seconds: 15,
            opacity: 100,
            place_within_source_content: false,
            opacity_expression: String::new(),
            z_index: 0,
        };
        assert!(validate_watermark(&form).is_ok());

        let invalid = WatermarkAdminForm {
            name: String::new(),
            ..form.clone()
        };
        assert!(matches!(
            validate_watermark(&invalid),
            Err(crate::ApplicationError::Validation { field: "name", .. })
        ));

        let invalid = WatermarkAdminForm {
            image: None,
            ..form.clone()
        };
        assert!(matches!(
            validate_watermark(&invalid),
            Err(crate::ApplicationError::Validation { field: "image", .. })
        ));

        let invalid = WatermarkAdminForm {
            duration_seconds: 900,
            ..form
        };
        assert!(matches!(
            validate_watermark(&invalid),
            Err(crate::ApplicationError::Validation {
                field: "duration_seconds",
                ..
            })
        ));
    }

    #[test]
    fn template_name_validation_is_application_owned() {
        assert_eq!(
            validate_template_group_name("  weekday  ").unwrap(),
            "weekday"
        );
        assert_eq!(
            validate_template_name("  prime time  ").unwrap(),
            "prime time"
        );
        assert!(validate_template_group_name(" ").is_err());
        assert!(validate_template_name(" ").is_err());
        assert!(validate_template_name(&"x".repeat(51)).is_err());
    }

    #[test]
    fn block_item_validation_is_application_owned() {
        let valid = BlockItemAdminForm {
            id: 0,
            index: 0,
            collection_type: 9,
            collection_id: None,
            multi_collection_id: None,
            smart_collection_id: None,
            playlist_id: None,
            media_item_id: None,
            search_title: Some("News".to_owned()),
            search_query: Some("title:news".to_owned()),
            playback_order: 2,
            smart_shuffle: ersatzrs_core::scheduling::SchedulingSmartShufflePolicy::default(),
            inherited_legacy_smart_shuffle: true,
            include_in_program_guide: true,
            disable_watermarks: false,
            collection_name: "News".to_owned(),
            watermark_ids: Vec::new(),
            graphics_element_ids: Vec::new(),
        };
        let mapped = super::block_item_from_admin(valid.clone()).unwrap();
        assert_eq!(mapped.collection_type, CollectionType::SearchQuery);
        assert!(mapped.smart_shuffle.is_none());

        let playlist = BlockItemAdminForm {
            collection_type: 6,
            playlist_id: Some(1),
            ..valid.clone()
        };
        assert!(super::block_item_from_admin(playlist).is_err());

        let missing_query = BlockItemAdminForm {
            search_query: None,
            ..valid
        };
        assert!(matches!(
            super::block_item_from_admin(missing_query),
            Err(crate::ApplicationError::Validation {
                field: "search_query",
                ..
            })
        ));
    }

    #[test]
    fn deco_validation_and_dependency_normalization_are_application_owned() {
        assert_eq!(
            super::validate_deco_group_name("  channel  ").unwrap(),
            "channel"
        );
        assert_eq!(
            super::validate_deco_template_name("  weekday  ").unwrap(),
            "weekday"
        );
        assert!(super::validate_deco_name(" ").is_err());
        assert!(super::validate_deco_template_group_name(&"x".repeat(51)).is_err());

        let mut form = super::DecoAdminForm {
            id: 1,
            group_id: 1,
            group_name: String::new(),
            name: "deco".to_owned(),
            watermark_mode: 3,
            watermark_ids: vec![2, 1, 2],
            use_watermark_during_filler: true,
            graphics_elements_mode: 0,
            graphics_element_ids: vec![4],
            use_graphics_elements_during_filler: true,
            break_content_mode: 0,
            break_content: vec![super::DecoBreakContentAdminForm {
                id: 0,
                collection_type: 0,
                collection_id: Some(1),
                media_item_id: None,
                multi_collection_id: None,
                smart_collection_id: None,
                playlist_id: None,
                collection_name: String::new(),
                placement: 0,
            }],
            default_filler_mode: 0,
            default_filler_collection_type: 0,
            default_filler_collection_id: Some(1),
            default_filler_media_item_id: None,
            default_filler_multi_collection_id: None,
            default_filler_smart_collection_id: None,
            default_filler_trim_to_fit: false,
            dead_air_fallback_mode: 0,
            dead_air_fallback_collection_type: 0,
            dead_air_fallback_collection_id: None,
            dead_air_fallback_media_item_id: None,
            dead_air_fallback_multi_collection_id: None,
            dead_air_fallback_smart_collection_id: None,
        };
        super::normalize_deco_admin_form(&mut form);
        assert_eq!(form.watermark_ids, vec![1, 2]);
        assert!(!form.use_watermark_during_filler);
        assert!(form.graphics_element_ids.is_empty());
        assert!(form.break_content.is_empty());
        assert_eq!(form.default_filler_collection_id, None);

        let duplicate_placement = [
            ersatzrs_core::repositories::DecoBreakContentAdmin {
                id: 0,
                collection_type: CollectionType::Collection,
                collection_id: Some(1),
                media_item_id: None,
                multi_collection_id: None,
                smart_collection_id: None,
                playlist_id: None,
                collection_name: String::new(),
                placement: ersatzrs_domain::DecoBreakPlacement::BlockStart,
            },
            ersatzrs_core::repositories::DecoBreakContentAdmin {
                id: 0,
                collection_type: CollectionType::Collection,
                collection_id: Some(2),
                media_item_id: None,
                multi_collection_id: None,
                smart_collection_id: None,
                playlist_id: None,
                collection_name: String::new(),
                placement: ersatzrs_domain::DecoBreakPlacement::BlockStart,
            },
        ];
        assert!(super::validate_deco_break_placements(&duplicate_placement).is_err());
    }

    #[test]
    fn playout_kind_and_date_filter_validation_are_application_owned() {
        assert_eq!(super::parse_playout_create_kind("").unwrap(), 1);
        assert_eq!(super::parse_playout_create_kind("block").unwrap(), 2);
        assert_eq!(super::parse_playout_create_kind("sequential").unwrap(), 3);
        assert_eq!(super::parse_playout_create_kind("scripted").unwrap(), 4);
        assert_eq!(
            super::parse_playout_create_kind("externaljson").unwrap(),
            20
        );
        assert!(super::parse_playout_create_kind("unknown").is_err());

        let mut filter = ersatzrs_core::repositories::PlayoutDateFilterAdmin {
            limit_to_date_range: true,
            start_month: 0,
            start_day: 40,
            start_year: None,
            end_month: 13,
            end_day: 0,
            end_year: None,
            days_of_week: vec![6, 0, 6, 7],
            days_of_month: vec![31, 1, 0],
            months_of_year: vec![12, 1, 13],
        };
        super::normalize_playout_date_filter(&mut filter);
        assert_eq!(filter.start_month, 1);
        assert_eq!(filter.start_day, 31);
        assert_eq!(filter.end_month, 12);
        assert_eq!(filter.end_day, 1);
        assert_eq!(filter.days_of_week, vec![0, 6]);
        assert_eq!(filter.days_of_month, vec![1, 31]);
        assert_eq!(filter.months_of_year, vec![1, 12]);
    }

    fn test_schedule_item() -> ScheduleItemAdmin {
        ScheduleItemAdmin {
            id: 0,
            index: 0,
            start_type: 1,
            start_time: None,
            fixed_start_time_behavior: None,
            collection_type: 9,
            collection_id: None,
            media_item_id: None,
            multi_collection_id: None,
            smart_collection_id: None,
            rerun_collection_id: None,
            playlist_id: None,
            search_title: None,
            search_query: Some("genre:comedy".to_owned()),
            collection_name: String::new(),
            playback_order: 3,
            smart_shuffle: ersatzrs_core::scheduling::SchedulingSmartShufflePolicy::default(),
            inherited_legacy_smart_shuffle: false,
            marathon_group_by: 0,
            marathon_shuffle_groups: false,
            marathon_shuffle_items: false,
            marathon_batch_size: None,
            fill_with_group_mode: 0,
            playout_mode: 2,
            multiple_mode: 0,
            multiple_count: "1".to_owned(),
            duration_hours: 1,
            duration_minutes: 0,
            tail_mode: 0,
            discard_to_fill_attempts: Some(0),
            custom_title: String::new(),
            guide_mode: 0,
            pre_roll_filler_id: None,
            mid_roll_filler_id: None,
            post_roll_filler_id: None,
            tail_filler_id: None,
            fallback_filler_id: None,
            watermark_ids: Vec::new(),
            graphics_element_ids: Vec::new(),
            preferred_audio_language_code: None,
            preferred_audio_title: String::new(),
            preferred_subtitle_language_code: None,
            subtitle_mode: None,
            pinned_stream_selection: None,
        }
    }
}
