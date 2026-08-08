//! Application query requests and read models.

use std::collections::BTreeMap;

use chrono::{DateTime, Duration, NaiveTime, Utc};
use ersatzrs_core::repositories::{
    AdminSearchKind, AdminSearchRepository, AdminSearchSnapshot, AdminSettingsRepository,
    BlockAdminRow, BlockEditorOptionAdmin, BlockEditorOptionsAdmin, BlockGroupAdminRow,
    BlockRepository, ChannelAdminListRow, ChannelEditorState, ChannelNumberRow, ChannelRepository,
    ChannelSelectOption, CollectionAdminRow, CollectionDetailAdmin, CollectionMemberAdmin,
    CollectionRepository, DecoAdminRecord, DecoAdminRow, DecoEditorOptionsAdmin, DecoGroupAdminRow,
    DecoRepository, DecoTemplateAdminRow, DecoTemplateGroupAdminRow, DecoTemplateItemAdmin,
    EpgPreviewChannelState, FfmpegProfileRepository, FillerRepository, LibraryRepository,
    MediaItemSuggestionAdmin, MultiCollectionAdmin, MultiCollectionAdminRow,
    MultiCollectionEditorOptions, PlaylistEditorAdmin, PlaylistEditorOptions,
    PlaylistGroupAdminRow, PlayoutAlternateSchedulesAdmin, PlayoutKindEditorAdmin,
    PlayoutListAdminRow, PlayoutRepository, PlayoutTemplatesAdmin, ProgramScheduleRepository,
    RerunCollectionAdmin, RerunCollectionAdminRow, RerunCollectionEditorOptions, ScheduleAdminForm,
    ScheduleAdminRow, ScheduleItemAdmin, ScheduleItemOptions, SequentialPlayoutAdmin,
    SettingsOptionKind, SettingsSelectOption, SmartCollectionAdminRow, TemplateAdminRow,
    TemplateBlockAdmin, TemplateBlockGroupAdmin, TemplateGroupAdminRow, TemplateItemAdmin,
    TemplateRepository, TraktListAdminForm, TraktListAdminRow, TraktRepository,
    WatermarkRepository,
};
use ersatzrs_core::scheduling::{
    BlockDefinition, BlockTemplate, BlockTemplateItem, BlockTemplateSchedule, EffectiveBlock,
    EffectiveBlockItem, PlayoutBuildMode, SchedulingCollectionKey, SchedulingPlaylistItem,
    SchedulingRequest, SchedulingSmartShufflePolicy,
};
use ersatzrs_domain::{
    BlockStopScheduling, ChannelWatermark, ChannelWatermarkImageSource, ChannelWatermarkMode,
    CollectionType, EntityId, FixedStartTimeBehavior, PlaybackOrder, Playout, PlayoutMode,
    PlayoutScheduleKind, ProgramScheduleItem, StartType, WatermarkLocation, WatermarkSize,
};

use crate::commands::{
    BlockItemAdminForm, DecoAdminForm, DecoBreakContentAdminForm, FfmpegProfileAdminForm,
    FillerPresetAdminForm, ScanOperationId, ScanRequest, WatermarkAdminForm,
};
use crate::operations::OperationKey;
use crate::ports::BlockPreviewBuilder;
use crate::ports::ChannelRuntime;
use crate::{ApplicationError, Result};

/// Storage-neutral season-detail projection assembled for the admin UI.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SeasonDetailReadModel {
    pub season: ersatzrs_core::repositories::SeasonDetail,
    pub episode_count: i64,
    pub episodes: Vec<ersatzrs_core::repositories::EpisodeCard>,
    pub extras: Vec<ersatzrs_core::repositories::AdminMediaCard>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PlaybackTroubleshootingReadModel {
    pub data: ersatzrs_core::repositories::PlaybackTroubleshootingAdmin,
    pub stream_selectors: Vec<String>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct TroubleshootingReportReadModel {
    pub ffmpeg_settings: FfmpegSettings,
    pub channels: Vec<ersatzrs_core::repositories::TroubleshootingChannelSummaryAdmin>,
    pub ffmpeg_profiles: Vec<ersatzrs_core::repositories::TroubleshootingFfmpegProfileSummaryAdmin>,
}

/// Load a complete season-detail projection.
pub async fn get_season_detail(
    id: EntityId,
    television: &dyn ersatzrs_core::repositories::TelevisionRepository,
    admin_media: &dyn ersatzrs_core::repositories::AdminMediaRepository,
) -> Result<Option<SeasonDetailReadModel>> {
    let Some(season) = television.get_season(id).await? else {
        return Ok(None);
    };
    let episode_count = television.get_episode_count(id).await?;
    let episodes = television.get_paged_episodes(id, 1, 100).await?;
    let extras = admin_media.list_extras_admin(id).await?;

    Ok(Some(SeasonDetailReadModel {
        season,
        episode_count,
        episodes,
        extras,
    }))
}

#[allow(clippy::struct_excessive_bools)]
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct FfmpegSettings {
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

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct FfmpegSettingsReadModel {
    pub form: FfmpegSettings,
    pub ffmpeg_profiles: Vec<SettingsSelectOption>,
    pub watermarks: Vec<SettingsSelectOption>,
    pub fallback_fillers: Vec<SettingsSelectOption>,
    pub custom_resolutions: Vec<SettingsSelectOption>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct LoggingSettings {
    pub default_minimum_log_level: String,
    pub scanning_minimum_log_level: String,
    pub scheduling_minimum_log_level: String,
    pub searching_minimum_log_level: String,
    pub streaming_minimum_log_level: String,
    pub http_minimum_log_level: String,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct HdhrSettings {
    pub uuid: String,
    pub tuner_count: i32,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ScannerSettings {
    pub library_refresh_interval: i32,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct PlayoutSettings {
    pub days_to_build: i32,
    pub skip_missing_items: bool,
    pub scripted_schedule_timeout_seconds: i32,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct UiSettings {
    pub is_dark_mode: bool,
    pub language: String,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct XmltvSettings {
    pub days_to_build: i32,
    pub time_zone: String,
    pub block_behavior: String,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ListCollections {
    pub search: String,
    pub page: u32,
    pub page_size: u32,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct FfmpegProfileListRead {
    pub id: EntityId,
    pub name: String,
    pub resolution: String,
    pub video_format: i32,
    pub audio_format: i32,
    pub hardware_acceleration: i32,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct FillerPresetListRead {
    pub id: EntityId,
    pub name: String,
    pub filler_kind: i32,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct FillerSelectRead {
    pub id: EntityId,
    pub name: String,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct FillerPlaylistRead {
    pub id: EntityId,
    pub name: String,
    pub playlist_group_id: EntityId,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct FillerEditorOptionsRead {
    pub collections: Vec<FillerSelectRead>,
    pub multi_collections: Vec<FillerSelectRead>,
    pub smart_collections: Vec<FillerSelectRead>,
    pub television_shows: Vec<FillerSelectRead>,
    pub television_seasons: Vec<FillerSelectRead>,
    pub artists: Vec<FillerSelectRead>,
    pub playlist_groups: Vec<FillerSelectRead>,
    pub playlists: Vec<FillerPlaylistRead>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct WatermarkListRead {
    pub id: EntityId,
    pub name: String,
    pub mode: i32,
    pub location: i32,
    pub image: Option<String>,
    pub image_source: i32,
}

/// One deterministic neighbor in the watermark editor comparison sequence.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct WatermarkNavigationRead {
    pub id: EntityId,
    pub name: String,
}

/// Watermark editor form plus its immediate neighbors.
#[derive(Clone, Debug, PartialEq)]
pub struct WatermarkEditorRead {
    pub form: WatermarkAdminForm,
    pub previous: Option<WatermarkNavigationRead>,
    pub next: Option<WatermarkNavigationRead>,
}

pub async fn list_ffmpeg_profiles(
    page: u32,
    page_size: u32,
    repository: &dyn FfmpegProfileRepository,
) -> Result<Vec<FfmpegProfileListRead>> {
    let resolutions = repository
        .list_resolutions()
        .await?
        .into_iter()
        .map(|resolution| (resolution.id, resolution.name))
        .collect::<BTreeMap<_, _>>();
    let offset = usize::try_from(page)
        .unwrap_or(usize::MAX)
        .saturating_mul(usize::try_from(page_size).unwrap_or(usize::MAX));
    Ok(repository
        .list()
        .await?
        .into_iter()
        .skip(offset)
        .take(usize::try_from(page_size).unwrap_or(usize::MAX))
        .map(|profile| FfmpegProfileListRead {
            id: profile.id,
            resolution: resolutions
                .get(&profile.resolution_id)
                .cloned()
                .unwrap_or_default(),
            name: profile.name,
            video_format: video_format_value(profile.video_format),
            audio_format: audio_format_value(profile.audio_format),
            hardware_acceleration: hardware_acceleration_value(profile.hardware_acceleration),
        })
        .collect())
}

pub async fn count_ffmpeg_profiles(repository: &dyn FfmpegProfileRepository) -> Result<u64> {
    u64::try_from(repository.list().await?.len()).map_err(|_| ApplicationError::InvalidData {
        kind: "ffmpeg profiles",
        value: "row count exceeds u64".to_owned(),
    })
}

pub async fn list_ffmpeg_resolutions(
    repository: &dyn FfmpegProfileRepository,
) -> Result<Vec<(EntityId, String)>> {
    Ok(repository
        .list_resolutions()
        .await?
        .into_iter()
        .map(|resolution| (resolution.id, resolution.name))
        .collect())
}

pub async fn get_ffmpeg_profile(
    id: EntityId,
    repository: &dyn FfmpegProfileRepository,
) -> Result<Option<FfmpegProfileAdminForm>> {
    if id <= 0 {
        return Err(ApplicationError::Validation {
            field: "profile_id",
            message: "must be greater than zero".to_owned(),
        });
    }
    Ok(repository
        .get_optional_admin(id)
        .await?
        .map(ffmpeg_profile_admin_form))
}

fn ffmpeg_profile_admin_form(profile: ersatzrs_domain::FFmpegProfile) -> FfmpegProfileAdminForm {
    let normalize_loudness_mode = normalize_loudness_value(profile.normalize_loudness_mode);
    FfmpegProfileAdminForm {
        id: profile.id,
        name: profile.name,
        thread_count: profile.thread_count,
        transcode_video: profile.transcode_video,
        transcode_audio: profile.transcode_audio,
        hardware_acceleration: hardware_acceleration_value(profile.hardware_acceleration),
        vaapi_display: profile.vaapi_display.unwrap_or_default(),
        vaapi_driver: vaapi_driver_value(profile.vaapi_driver),
        vaapi_device: profile.vaapi_device.unwrap_or_default(),
        qsv_extra_hardware_frames: profile.qsv_extra_hardware_frames.unwrap_or(64),
        resolution_id: profile.resolution_id,
        scaling_behavior: scaling_behavior_value(profile.scaling_behavior),
        pad_mode: filter_mode_value(profile.pad_mode),
        video_format: video_format_value(profile.video_format),
        video_profile: profile.video_profile.unwrap_or_default(),
        video_preset: profile.video_preset.unwrap_or_default(),
        video_tune: profile.video_tune.unwrap_or_default(),
        allow_b_frames: profile.allow_b_frames,
        bit_depth: bit_depth_value(profile.bit_depth),
        video_bitrate: profile.video_bitrate,
        video_buffer_size: profile.video_buffer_size,
        tonemap_algorithm: tonemap_value(profile.tonemap_algorithm),
        output_dynamic_range: output_dynamic_range_value(profile.output_dynamic_range),
        // Blank and zero mean "use the documented default", which is what the
        // editor shows as a placeholder rather than pre-filling (ADR 0064).
        hdr_mastering_display: profile
            .hdr_mastering_display
            .map(ersatzrs_domain::ffmpeg::MasteringDisplayMetadata::to_ffmpeg_value)
            .unwrap_or_default(),
        hdr_max_cll: profile
            .hdr_content_light
            .map_or(0, |light| i32::try_from(light.max_cll).unwrap_or(0)),
        hdr_max_fall: profile
            .hdr_content_light
            .map_or(0, |light| i32::try_from(light.max_fall).unwrap_or(0)),
        audio_format: audio_format_value(profile.audio_format),
        audio_bitrate: profile.audio_bitrate,
        audio_buffer_size: profile.audio_buffer_size,
        normalize_loudness_mode,
        target_loudness: profile.target_loudness.map_or_else(
            || {
                if normalize_loudness_mode == 1 {
                    "-16".to_owned()
                } else {
                    String::new()
                }
            },
            |value| value.to_string(),
        ),
        audio_channels: profile.audio_channels,
        audio_sample_rate: profile.audio_sample_rate,
        normalize_framerate: profile.normalize_framerate,
        normalize_colors: profile.normalize_colors,
        deinterlace_video: profile.deinterlace_video.unwrap_or(true),
    }
}

fn hardware_acceleration_value(value: ersatzrs_domain::HardwareAccelerationKind) -> i32 {
    match value {
        ersatzrs_domain::HardwareAccelerationKind::None => 0,
        ersatzrs_domain::HardwareAccelerationKind::Qsv => 1,
        ersatzrs_domain::HardwareAccelerationKind::Nvenc => 2,
        ersatzrs_domain::HardwareAccelerationKind::Vaapi => 3,
        ersatzrs_domain::HardwareAccelerationKind::VideoToolbox => 4,
        ersatzrs_domain::HardwareAccelerationKind::Amf => 5,
        ersatzrs_domain::HardwareAccelerationKind::V4l2m2m => 6,
        ersatzrs_domain::HardwareAccelerationKind::Rkmpp => 7,
    }
}

fn video_format_value(value: ersatzrs_domain::FFmpegProfileVideoFormat) -> i32 {
    match value {
        ersatzrs_domain::FFmpegProfileVideoFormat::None => 0,
        ersatzrs_domain::FFmpegProfileVideoFormat::H264 => 1,
        ersatzrs_domain::FFmpegProfileVideoFormat::Hevc => 2,
        ersatzrs_domain::FFmpegProfileVideoFormat::Mpeg2Video => 3,
        ersatzrs_domain::FFmpegProfileVideoFormat::Av1 => 4,
        ersatzrs_domain::FFmpegProfileVideoFormat::Copy => 99,
    }
}

fn audio_format_value(value: ersatzrs_domain::FFmpegProfileAudioFormat) -> i32 {
    match value {
        ersatzrs_domain::FFmpegProfileAudioFormat::None => 0,
        ersatzrs_domain::FFmpegProfileAudioFormat::Aac => 1,
        ersatzrs_domain::FFmpegProfileAudioFormat::Ac3 => 2,
        ersatzrs_domain::FFmpegProfileAudioFormat::AacLatm => 3,
        ersatzrs_domain::FFmpegProfileAudioFormat::Copy => 99,
    }
}

fn bit_depth_value(value: ersatzrs_domain::FFmpegProfileBitDepth) -> i32 {
    match value {
        ersatzrs_domain::FFmpegProfileBitDepth::EightBit => 0,
        ersatzrs_domain::FFmpegProfileBitDepth::TenBit => 1,
    }
}

fn tonemap_value(value: ersatzrs_domain::FFmpegProfileTonemapAlgorithm) -> i32 {
    match value {
        ersatzrs_domain::FFmpegProfileTonemapAlgorithm::Linear => 0,
        ersatzrs_domain::FFmpegProfileTonemapAlgorithm::Clip => 1,
        ersatzrs_domain::FFmpegProfileTonemapAlgorithm::Gamma => 2,
        ersatzrs_domain::FFmpegProfileTonemapAlgorithm::Reinhard => 3,
        ersatzrs_domain::FFmpegProfileTonemapAlgorithm::Mobius => 4,
        ersatzrs_domain::FFmpegProfileTonemapAlgorithm::Hable => 5,
    }
}

const fn output_dynamic_range_value(value: ersatzrs_domain::OutputDynamicRange) -> i32 {
    match value {
        ersatzrs_domain::OutputDynamicRange::SdrBt709 => 0,
        ersatzrs_domain::OutputDynamicRange::Hdr10Bt2020Pq => 1,
    }
}

fn normalize_loudness_value(value: ersatzrs_domain::NormalizeLoudnessMode) -> i32 {
    match value {
        ersatzrs_domain::NormalizeLoudnessMode::Off => 0,
        ersatzrs_domain::NormalizeLoudnessMode::LoudNorm => 1,
    }
}

fn vaapi_driver_value(value: ersatzrs_domain::VaapiDriver) -> i32 {
    match value {
        ersatzrs_domain::VaapiDriver::Auto => 0,
        ersatzrs_domain::VaapiDriver::IHD => 1,
        ersatzrs_domain::VaapiDriver::I965 => 2,
        ersatzrs_domain::VaapiDriver::RadeonSI => 3,
        ersatzrs_domain::VaapiDriver::Nouveau => 4,
    }
}

fn scaling_behavior_value(value: ersatzrs_domain::ScalingBehavior) -> i32 {
    match value {
        ersatzrs_domain::ScalingBehavior::ScaleAndPad => 0,
        ersatzrs_domain::ScalingBehavior::Stretch => 1,
        ersatzrs_domain::ScalingBehavior::Crop => 2,
    }
}

fn filter_mode_value(value: ersatzrs_domain::FilterMode) -> i32 {
    match value {
        ersatzrs_domain::FilterMode::HardwareIfPossible => 0,
        ersatzrs_domain::FilterMode::Software => 1,
    }
}

const FILLER_PRESETS_PAGE_SIZE_KEY: &str = "pages.filler_presets.page_size";

pub async fn get_filler_presets_page_size(settings: &dyn AdminSettingsRepository) -> Result<u32> {
    positive_page_size(FILLER_PRESETS_PAGE_SIZE_KEY, settings).await
}

pub async fn list_filler_presets(
    page: u32,
    page_size: u32,
    repository: &dyn FillerRepository,
    settings: &dyn AdminSettingsRepository,
) -> Result<(Vec<FillerPresetListRead>, u64)> {
    settings
        .set_string(FILLER_PRESETS_PAGE_SIZE_KEY, &page_size.max(1).to_string())
        .await?;
    let presets = repository.list().await?;
    let total = u64::try_from(presets.len()).map_err(|_| ApplicationError::InvalidData {
        kind: "filler presets",
        value: "row count exceeds u64".to_owned(),
    })?;
    let offset = usize::try_from(page)
        .unwrap_or(usize::MAX)
        .saturating_mul(usize::try_from(page_size).unwrap_or(usize::MAX));
    Ok((
        presets
            .into_iter()
            .skip(offset)
            .take(usize::try_from(page_size).unwrap_or(usize::MAX))
            .map(|preset| FillerPresetListRead {
                id: preset.id,
                name: preset.name,
                filler_kind: filler_kind_value(preset.filler_kind),
            })
            .collect(),
        total,
    ))
}

pub async fn get_filler_preset(
    id: EntityId,
    repository: &dyn FillerRepository,
) -> Result<Option<FillerPresetAdminForm>> {
    if id <= 0 {
        return Err(ApplicationError::Validation {
            field: "filler_preset_id",
            message: "must be greater than zero".to_owned(),
        });
    }
    Ok(repository.get_optional_admin(id).await?.map(|preset| {
        let seconds = preset
            .duration
            .and_then(|duration| i32::try_from(duration.num_seconds()).ok())
            .unwrap_or_default();
        FillerPresetAdminForm {
            id: preset.id,
            name: preset.name,
            filler_kind: filler_kind_value(preset.filler_kind),
            filler_mode: filler_mode_value(preset.filler_mode),
            duration_hours: seconds / 3600,
            duration_minutes: (seconds % 3600) / 60,
            duration_seconds: seconds % 60,
            count: preset.count,
            pad_to_nearest_minute: preset.pad_to_nearest_minute,
            allow_watermarks: preset.allow_watermarks,
            collection_type: collection_type_value(preset.collection_type),
            collection_id: preset.collection_id,
            media_item_id: preset.media_item_id,
            multi_collection_id: preset.multi_collection_id,
            smart_collection_id: preset.smart_collection_id,
            playlist_id: preset.playlist_id,
            expression: preset.expression.unwrap_or_default(),
            use_chapters_as_media_items: preset.use_chapters_as_media_items,
        }
    }))
}

pub async fn list_watermarks(
    page: u32,
    page_size: u32,
    repository: &dyn WatermarkRepository,
) -> Result<Vec<WatermarkListRead>> {
    let offset = usize::try_from(page.saturating_sub(1))
        .unwrap_or(usize::MAX)
        .saturating_mul(usize::try_from(page_size).unwrap_or(usize::MAX));
    Ok(repository
        .list_admin()
        .await?
        .into_iter()
        .skip(offset)
        .take(usize::try_from(page_size).unwrap_or(usize::MAX))
        .map(|watermark| WatermarkListRead {
            id: watermark.id,
            name: watermark.name,
            mode: watermark_mode_value(watermark.mode),
            location: watermark_location_value(watermark.location),
            image: watermark.image,
            image_source: watermark_image_source_value(watermark.image_source),
        })
        .collect())
}

pub async fn count_watermarks(repository: &dyn WatermarkRepository) -> Result<u64> {
    u64::try_from(repository.list_admin().await?.len()).map_err(|_| ApplicationError::InvalidData {
        kind: "watermarks",
        value: "row count exceeds u64".to_owned(),
    })
}

pub async fn get_watermark(
    id: EntityId,
    repository: &dyn WatermarkRepository,
) -> Result<Option<WatermarkAdminForm>> {
    if id <= 0 {
        return Err(ApplicationError::Validation {
            field: "watermark_id",
            message: "must be greater than zero".to_owned(),
        });
    }
    Ok(repository.get_admin(id).await?.map(watermark_admin_form))
}

pub async fn get_watermark_editor(
    id: EntityId,
    repository: &dyn WatermarkRepository,
) -> Result<Option<WatermarkEditorRead>> {
    if id <= 0 {
        return Err(ApplicationError::Validation {
            field: "watermark_id",
            message: "must be greater than zero".to_owned(),
        });
    }

    let watermarks = repository.list_admin().await?;
    let Some(index) = watermarks.iter().position(|watermark| watermark.id == id) else {
        return Ok(None);
    };
    let form = watermark_admin_form(watermarks[index].clone());
    let previous = index.checked_sub(1).map(|previous_index| {
        let watermark = &watermarks[previous_index];
        WatermarkNavigationRead {
            id: watermark.id,
            name: watermark.name.clone(),
        }
    });
    let next = watermarks
        .get(index + 1)
        .map(|watermark| WatermarkNavigationRead {
            id: watermark.id,
            name: watermark.name.clone(),
        });

    Ok(Some(WatermarkEditorRead {
        form,
        previous,
        next,
    }))
}

pub async fn list_template_groups(
    repository: &dyn TemplateRepository,
) -> Result<Vec<TemplateGroupAdminRow>> {
    repository.list_groups_admin().await.map_err(Into::into)
}

pub async fn list_templates(
    search: &str,
    repository: &dyn TemplateRepository,
) -> Result<Vec<TemplateAdminRow>> {
    repository.list_admin(search).await.map_err(Into::into)
}

pub async fn get_template(
    id: EntityId,
    repository: &dyn TemplateRepository,
) -> Result<Option<TemplateAdminRow>> {
    if id <= 0 {
        return Err(ApplicationError::Validation {
            field: "template_id",
            message: "must be greater than zero".to_owned(),
        });
    }
    repository.get_admin(id).await.map_err(Into::into)
}

pub async fn get_template_items(
    template_id: EntityId,
    repository: &dyn TemplateRepository,
) -> Result<Vec<TemplateItemAdmin>> {
    if template_id <= 0 {
        return Err(ApplicationError::Validation {
            field: "template_id",
            message: "must be greater than zero".to_owned(),
        });
    }
    repository
        .list_items_admin(template_id)
        .await
        .map_err(Into::into)
}

pub async fn list_template_block_groups(
    repository: &dyn TemplateRepository,
) -> Result<Vec<TemplateBlockGroupAdmin>> {
    repository
        .list_block_groups_admin()
        .await
        .map_err(Into::into)
}

pub async fn list_template_blocks(
    group_id: EntityId,
    repository: &dyn TemplateRepository,
) -> Result<Vec<TemplateBlockAdmin>> {
    if group_id <= 0 {
        return Err(ApplicationError::Validation {
            field: "block_group_id",
            message: "must be greater than zero".to_owned(),
        });
    }
    repository
        .list_blocks_admin(group_id)
        .await
        .map_err(Into::into)
}

pub async fn list_block_groups(
    repository: &dyn BlockRepository,
) -> Result<Vec<BlockGroupAdminRow>> {
    repository.list_groups_admin().await.map_err(Into::into)
}

pub async fn list_blocks(
    search: &str,
    repository: &dyn BlockRepository,
) -> Result<Vec<BlockAdminRow>> {
    repository.list_admin(search).await.map_err(Into::into)
}

pub async fn get_block(
    id: EntityId,
    repository: &dyn BlockRepository,
) -> Result<Option<BlockAdminRow>> {
    if id <= 0 {
        return Err(ApplicationError::Validation {
            field: "block_id",
            message: "must be greater than zero".to_owned(),
        });
    }
    repository.get_admin(id).await.map_err(Into::into)
}

pub async fn get_block_items(
    block_id: EntityId,
    repository: &dyn BlockRepository,
) -> Result<Vec<BlockItemAdminForm>> {
    if block_id <= 0 {
        return Err(ApplicationError::Validation {
            field: "block_id",
            message: "must be greater than zero".to_owned(),
        });
    }
    Ok(repository
        .list_items_admin(block_id)
        .await?
        .into_iter()
        .map(|item| BlockItemAdminForm {
            id: item.id,
            index: item.index,
            collection_type: collection_type_value(item.collection_type),
            collection_id: item.collection_id,
            multi_collection_id: item.multi_collection_id,
            smart_collection_id: item.smart_collection_id,
            playlist_id: None,
            media_item_id: item.media_item_id,
            search_title: item.search_title,
            search_query: item.search_query,
            playback_order: playback_order_value(item.playback_order),
            inherited_legacy_smart_shuffle: item.smart_shuffle.is_none(),
            smart_shuffle: item.smart_shuffle.unwrap_or_default(),
            include_in_program_guide: item.include_in_program_guide,
            disable_watermarks: item.disable_watermarks,
            collection_name: item.collection_name,
            watermark_ids: item.watermark_ids,
            graphics_element_ids: item.graphics_element_ids,
        })
        .collect())
}

pub async fn search_block_options(
    kind: BlockOptionKind,
    prefix: &str,
    repository: &dyn BlockRepository,
) -> Result<Vec<BlockEditorOptionAdmin>> {
    let prefix = prefix.trim();
    if prefix.is_empty() {
        return Ok(Vec::new());
    }
    match kind {
        BlockOptionKind::Collection => repository.search_collections_admin(prefix).await,
        BlockOptionKind::SmartCollection => repository.search_smart_collections_admin(prefix).await,
        BlockOptionKind::MultiCollection => repository.search_multi_collections_admin(prefix).await,
        BlockOptionKind::Show => repository.search_shows_admin(prefix).await,
        BlockOptionKind::Season => repository.search_seasons_admin(prefix).await,
    }
    .map_err(Into::into)
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum BlockOptionKind {
    Collection,
    SmartCollection,
    MultiCollection,
    Show,
    Season,
}

pub async fn load_block_editor_options(
    repository: &dyn BlockRepository,
) -> Result<BlockEditorOptionsAdmin> {
    repository.editor_options_admin().await.map_err(Into::into)
}

pub async fn list_deco_groups(repository: &dyn DecoRepository) -> Result<Vec<DecoGroupAdminRow>> {
    repository.list_groups_admin().await.map_err(Into::into)
}

pub async fn list_decos(
    search: &str,
    repository: &dyn DecoRepository,
) -> Result<Vec<DecoAdminRow>> {
    repository.list_admin(search).await.map_err(Into::into)
}

pub async fn get_deco(
    id: EntityId,
    repository: &dyn DecoRepository,
) -> Result<Option<DecoAdminForm>> {
    if id <= 0 {
        return Err(ApplicationError::Validation {
            field: "deco_id",
            message: "must be greater than zero".to_owned(),
        });
    }
    Ok(repository.get_admin(id).await?.map(deco_admin_form))
}

pub async fn load_deco_editor_options(
    repository: &dyn DecoRepository,
) -> Result<DecoEditorOptionsAdmin> {
    repository.editor_options_admin().await.map_err(Into::into)
}

pub async fn list_deco_template_groups(
    repository: &dyn DecoRepository,
) -> Result<Vec<DecoTemplateGroupAdminRow>> {
    repository
        .list_template_groups_admin()
        .await
        .map_err(Into::into)
}

pub async fn list_deco_templates(
    search: &str,
    repository: &dyn DecoRepository,
) -> Result<Vec<DecoTemplateAdminRow>> {
    repository
        .list_templates_admin(search)
        .await
        .map_err(Into::into)
}

pub async fn get_deco_template(
    id: EntityId,
    repository: &dyn DecoRepository,
) -> Result<Option<DecoTemplateAdminRow>> {
    if id <= 0 {
        return Err(ApplicationError::Validation {
            field: "deco_template_id",
            message: "must be greater than zero".to_owned(),
        });
    }
    repository.get_template_admin(id).await.map_err(Into::into)
}

pub async fn get_deco_template_items(
    id: EntityId,
    repository: &dyn DecoRepository,
) -> Result<Vec<DecoTemplateItemAdmin>> {
    if id <= 0 {
        return Err(ApplicationError::Validation {
            field: "deco_template_id",
            message: "must be greater than zero".to_owned(),
        });
    }
    repository
        .list_template_items_admin(id)
        .await
        .map_err(Into::into)
}

fn deco_admin_form(deco: DecoAdminRecord) -> DecoAdminForm {
    DecoAdminForm {
        id: deco.id,
        group_id: deco.group_id,
        group_name: deco.group_name,
        name: deco.name,
        watermark_mode: deco_mode_value(deco.watermark_mode),
        watermark_ids: deco.watermark_ids,
        use_watermark_during_filler: deco.use_watermark_during_filler,
        graphics_elements_mode: deco_mode_value(deco.graphics_elements_mode),
        graphics_element_ids: deco.graphics_element_ids,
        use_graphics_elements_during_filler: deco.use_graphics_elements_during_filler,
        break_content_mode: deco_mode_value(deco.break_content_mode),
        break_content: deco
            .break_content
            .into_iter()
            .map(|item| DecoBreakContentAdminForm {
                id: item.id,
                collection_type: collection_type_value(item.collection_type),
                collection_id: item.collection_id,
                media_item_id: item.media_item_id,
                multi_collection_id: item.multi_collection_id,
                smart_collection_id: item.smart_collection_id,
                playlist_id: item.playlist_id,
                collection_name: item.collection_name,
                placement: deco_placement_value(item.placement),
            })
            .collect(),
        default_filler_mode: deco_mode_value(deco.default_filler_mode),
        default_filler_collection_type: collection_type_value(deco.default_filler_collection_type),
        default_filler_collection_id: deco.default_filler_collection_id,
        default_filler_media_item_id: deco.default_filler_media_item_id,
        default_filler_multi_collection_id: deco.default_filler_multi_collection_id,
        default_filler_smart_collection_id: deco.default_filler_smart_collection_id,
        default_filler_trim_to_fit: deco.default_filler_trim_to_fit,
        dead_air_fallback_mode: deco_mode_value(deco.dead_air_fallback_mode),
        dead_air_fallback_collection_type: collection_type_value(
            deco.dead_air_fallback_collection_type,
        ),
        dead_air_fallback_collection_id: deco.dead_air_fallback_collection_id,
        dead_air_fallback_media_item_id: deco.dead_air_fallback_media_item_id,
        dead_air_fallback_multi_collection_id: deco.dead_air_fallback_multi_collection_id,
        dead_air_fallback_smart_collection_id: deco.dead_air_fallback_smart_collection_id,
    }
}

fn deco_mode_value(value: ersatzrs_domain::DecoMode) -> i32 {
    match value {
        ersatzrs_domain::DecoMode::Inherit => 0,
        ersatzrs_domain::DecoMode::Disable => 1,
        ersatzrs_domain::DecoMode::Override => 2,
        ersatzrs_domain::DecoMode::Merge => 3,
    }
}

fn deco_placement_value(value: ersatzrs_domain::DecoBreakPlacement) -> i32 {
    match value {
        ersatzrs_domain::DecoBreakPlacement::BlockStart => 0,
        ersatzrs_domain::DecoBreakPlacement::BlockFinish => 1,
        ersatzrs_domain::DecoBreakPlacement::BetweenBlockItems => 2,
        ersatzrs_domain::DecoBreakPlacement::ChapterMarkers => 3,
    }
}

fn watermark_admin_form(watermark: ChannelWatermark) -> WatermarkAdminForm {
    WatermarkAdminForm {
        id: watermark.id,
        name: watermark.name,
        mode: watermark_mode_value(watermark.mode),
        image_source: watermark_image_source_value(watermark.image_source),
        image: watermark.image,
        original_content_type: watermark.original_content_type,
        location: watermark_location_value(watermark.location),
        size: watermark_size_value(watermark.size),
        width_percent: watermark.width_percent,
        horizontal_margin_percent: watermark.horizontal_margin_percent,
        vertical_margin_percent: watermark.vertical_margin_percent,
        frequency_minutes: watermark.frequency_minutes,
        duration_seconds: watermark.duration_seconds,
        opacity: watermark.opacity,
        place_within_source_content: watermark.place_within_source_content,
        opacity_expression: watermark.opacity_expression.unwrap_or_default(),
        z_index: watermark.z_index,
    }
}

fn watermark_mode_value(value: ChannelWatermarkMode) -> i32 {
    match value {
        ChannelWatermarkMode::None => 0,
        ChannelWatermarkMode::Permanent => 1,
        ChannelWatermarkMode::Intermittent => 2,
        ChannelWatermarkMode::OpacityExpression => 3,
    }
}

fn watermark_image_source_value(value: ChannelWatermarkImageSource) -> i32 {
    match value {
        ChannelWatermarkImageSource::Custom => 0,
        ChannelWatermarkImageSource::ChannelLogo => 1,
        ChannelWatermarkImageSource::Resource => 100,
    }
}

fn watermark_location_value(value: WatermarkLocation) -> i32 {
    match value {
        WatermarkLocation::BottomRight => 0,
        WatermarkLocation::BottomLeft => 1,
        WatermarkLocation::TopRight => 2,
        WatermarkLocation::TopLeft => 3,
        WatermarkLocation::TopMiddle => 4,
        WatermarkLocation::RightMiddle => 5,
        WatermarkLocation::BottomMiddle => 6,
        WatermarkLocation::LeftMiddle => 7,
        WatermarkLocation::MiddleCenter => 8,
    }
}

fn watermark_size_value(value: WatermarkSize) -> i32 {
    match value {
        WatermarkSize::Scaled => 0,
        WatermarkSize::ActualSize => 1,
    }
}

pub async fn get_filler_editor_options(
    repository: &dyn CollectionRepository,
) -> Result<FillerEditorOptionsRead> {
    let options = repository.get_rerun_editor_options().await?;
    let groups = repository.list_playlist_groups_admin().await?;
    Ok(FillerEditorOptionsRead {
        collections: options
            .collections
            .into_iter()
            .map(|row| FillerSelectRead {
                id: row.id,
                name: row.name,
            })
            .collect(),
        multi_collections: options
            .multi_collections
            .into_iter()
            .map(|row| FillerSelectRead {
                id: row.id,
                name: row.name,
            })
            .collect(),
        smart_collections: options
            .smart_collections
            .into_iter()
            .map(|row| FillerSelectRead {
                id: row.id,
                name: row.name,
            })
            .collect(),
        television_shows: options
            .shows
            .into_iter()
            .map(|row| FillerSelectRead {
                id: row.id,
                name: row.name,
            })
            .collect(),
        television_seasons: options
            .seasons
            .into_iter()
            .map(|row| FillerSelectRead {
                id: row.id,
                name: row.name,
            })
            .collect(),
        artists: options
            .artists
            .into_iter()
            .map(|row| FillerSelectRead {
                id: row.id,
                name: row.name,
            })
            .collect(),
        playlist_groups: groups
            .iter()
            .map(|row| FillerSelectRead {
                id: row.id,
                name: row.name.clone(),
            })
            .collect(),
        playlists: groups
            .into_iter()
            .flat_map(|group| {
                group
                    .playlists
                    .into_iter()
                    .map(move |playlist| FillerPlaylistRead {
                        id: playlist.id,
                        name: playlist.name,
                        playlist_group_id: group.id,
                    })
            })
            .collect(),
    })
}

fn filler_kind_value(value: ersatzrs_domain::FillerKind) -> i32 {
    match value {
        ersatzrs_domain::FillerKind::None => 0,
        ersatzrs_domain::FillerKind::PreRoll => 1,
        ersatzrs_domain::FillerKind::MidRoll => 2,
        ersatzrs_domain::FillerKind::PostRoll => 3,
        ersatzrs_domain::FillerKind::Tail => 4,
        ersatzrs_domain::FillerKind::Fallback => 5,
        ersatzrs_domain::FillerKind::GuideMode => 6,
        ersatzrs_domain::FillerKind::DecoDefault => 7,
    }
}

fn filler_mode_value(value: ersatzrs_domain::FillerMode) -> i32 {
    match value {
        ersatzrs_domain::FillerMode::None => 0,
        ersatzrs_domain::FillerMode::Duration => 1,
        ersatzrs_domain::FillerMode::Count => 2,
        ersatzrs_domain::FillerMode::Pad => 3,
        ersatzrs_domain::FillerMode::RandomCount => 4,
    }
}

fn collection_type_value(value: ersatzrs_domain::CollectionType) -> i32 {
    match value {
        ersatzrs_domain::CollectionType::Collection => 0,
        ersatzrs_domain::CollectionType::TelevisionShow => 1,
        ersatzrs_domain::CollectionType::TelevisionSeason => 2,
        ersatzrs_domain::CollectionType::Artist => 3,
        ersatzrs_domain::CollectionType::MultiCollection => 4,
        ersatzrs_domain::CollectionType::SmartCollection => 5,
        ersatzrs_domain::CollectionType::Playlist => 6,
        ersatzrs_domain::CollectionType::RerunFirstRun => 7,
        ersatzrs_domain::CollectionType::RerunRerun => 8,
        ersatzrs_domain::CollectionType::SearchQuery => 9,
        ersatzrs_domain::CollectionType::Movie => 10,
        ersatzrs_domain::CollectionType::Episode => 20,
        ersatzrs_domain::CollectionType::MusicVideo => 30,
        ersatzrs_domain::CollectionType::OtherVideo => 40,
        ersatzrs_domain::CollectionType::Song => 50,
        ersatzrs_domain::CollectionType::Image => 60,
        ersatzrs_domain::CollectionType::RemoteStream => 70,
        ersatzrs_domain::CollectionType::FakeCollection => 100,
        ersatzrs_domain::CollectionType::FakePlaylistItem => 101,
    }
}

fn playback_order_value(value: PlaybackOrder) -> i32 {
    match value {
        PlaybackOrder::None => 0,
        PlaybackOrder::Chronological => 1,
        PlaybackOrder::Random => 2,
        PlaybackOrder::Shuffle => 3,
        PlaybackOrder::ShuffleInOrder => 4,
        PlaybackOrder::MultiEpisodeShuffle => 5,
        PlaybackOrder::SeasonEpisode => 6,
        PlaybackOrder::RandomRotation => 7,
        PlaybackOrder::Marathon => 8,
    }
}

pub async fn list_collections(
    query: ListCollections,
    repository: &dyn CollectionRepository,
) -> Result<(Vec<CollectionAdminRow>, u64)> {
    repository
        .list_admin(&query.search, query.page, query.page_size)
        .await
        .map_err(Into::into)
}

pub async fn get_collection(
    id: EntityId,
    repository: &dyn CollectionRepository,
) -> Result<Option<CollectionAdminRow>> {
    if id <= 0 {
        return Err(ApplicationError::Validation {
            field: "collection_id",
            message: "must be greater than zero".to_owned(),
        });
    }
    repository.get_admin(id).await.map_err(Into::into)
}

pub async fn list_all_collections(
    repository: &dyn CollectionRepository,
) -> Result<Vec<CollectionAdminRow>> {
    let (rows, _) = repository.list_admin("", 0, u32::MAX).await?;
    Ok(rows)
}

pub async fn list_smart_collections(
    query: ListCollections,
    repository: &dyn CollectionRepository,
) -> Result<(Vec<SmartCollectionAdminRow>, u64)> {
    repository
        .list_smart_admin(&query.search, query.page, query.page_size)
        .await
        .map_err(Into::into)
}

pub async fn get_smart_collection(
    id: EntityId,
    repository: &dyn CollectionRepository,
) -> Result<Option<SmartCollectionAdminRow>> {
    if id <= 0 {
        return Err(ApplicationError::Validation {
            field: "smart_collection_id",
            message: "must be greater than zero".to_owned(),
        });
    }
    repository.get_smart_admin(id).await.map_err(Into::into)
}

const MULTI_COLLECTIONS_PAGE_SIZE_KEY: &str = "pages.multi_collections.page_size";

pub async fn get_multi_collections_page_size(
    settings: &dyn AdminSettingsRepository,
) -> Result<u32> {
    let value = get_i32_setting(MULTI_COLLECTIONS_PAGE_SIZE_KEY, 10, settings).await?;
    Ok(u32::try_from(value)
        .ok()
        .filter(|value| *value > 0)
        .unwrap_or(10))
}

pub async fn list_multi_collections(
    query: ListCollections,
    repository: &dyn CollectionRepository,
    settings: &dyn AdminSettingsRepository,
) -> Result<(Vec<MultiCollectionAdminRow>, u64)> {
    settings
        .set_string(
            MULTI_COLLECTIONS_PAGE_SIZE_KEY,
            &query.page_size.to_string(),
        )
        .await?;
    repository
        .list_multi_admin(&query.search, query.page, query.page_size)
        .await
        .map_err(Into::into)
}

pub async fn get_multi_collection(
    id: EntityId,
    repository: &dyn CollectionRepository,
) -> Result<Option<MultiCollectionAdmin>> {
    if id <= 0 {
        return Err(ApplicationError::Validation {
            field: "multi_collection_id",
            message: "must be greater than zero".to_owned(),
        });
    }
    repository.get_multi_admin(id).await.map_err(Into::into)
}

pub async fn get_multi_collection_editor_options(
    repository: &dyn CollectionRepository,
) -> Result<MultiCollectionEditorOptions> {
    repository
        .get_multi_editor_options()
        .await
        .map_err(Into::into)
}

const RERUN_COLLECTIONS_PAGE_SIZE_KEY: &str = "pages.rerun_collections.page_size";

pub async fn get_rerun_collections_page_size(
    settings: &dyn AdminSettingsRepository,
) -> Result<u32> {
    let value = get_i32_setting(RERUN_COLLECTIONS_PAGE_SIZE_KEY, 10, settings).await?;
    Ok(u32::try_from(value)
        .ok()
        .filter(|value| *value > 0)
        .unwrap_or(10))
}

pub async fn list_rerun_collections(
    query: ListCollections,
    repository: &dyn CollectionRepository,
    settings: &dyn AdminSettingsRepository,
) -> Result<(Vec<RerunCollectionAdminRow>, u64)> {
    settings
        .set_string(
            RERUN_COLLECTIONS_PAGE_SIZE_KEY,
            &query.page_size.to_string(),
        )
        .await?;
    repository
        .list_rerun_admin(&query.search, query.page, query.page_size)
        .await
        .map_err(Into::into)
}

pub async fn get_rerun_collection(
    id: EntityId,
    repository: &dyn CollectionRepository,
) -> Result<Option<RerunCollectionAdmin>> {
    if id <= 0 {
        return Err(ApplicationError::Validation {
            field: "rerun_collection_id",
            message: "must be greater than zero".to_owned(),
        });
    }
    repository.get_rerun_admin(id).await.map_err(Into::into)
}

pub async fn get_rerun_collection_editor_options(
    repository: &dyn CollectionRepository,
) -> Result<RerunCollectionEditorOptions> {
    repository
        .get_rerun_editor_options()
        .await
        .map_err(Into::into)
}

pub async fn list_playlist_groups(
    repository: &dyn CollectionRepository,
) -> Result<Vec<PlaylistGroupAdminRow>> {
    repository
        .list_playlist_groups_admin()
        .await
        .map_err(Into::into)
}

pub async fn get_playlist_editor(
    id: EntityId,
    repository: &dyn CollectionRepository,
) -> Result<Option<PlaylistEditorAdmin>> {
    if id <= 0 {
        return Err(ApplicationError::Validation {
            field: "playlist_id",
            message: "must be greater than zero".to_owned(),
        });
    }
    repository
        .get_playlist_editor_admin(id)
        .await
        .map_err(Into::into)
}

pub async fn get_playlist_editor_options(
    repository: &dyn CollectionRepository,
) -> Result<PlaylistEditorOptions> {
    repository
        .get_playlist_editor_options()
        .await
        .map_err(Into::into)
}

pub async fn suggest_media_items(
    prefix: &str,
    repository: &dyn CollectionRepository,
) -> Result<Vec<MediaItemSuggestionAdmin>> {
    let prefix = prefix.trim();
    if prefix.is_empty() {
        return Ok(Vec::new());
    }
    repository
        .suggest_media_items_admin(prefix)
        .await
        .map_err(Into::into)
}

pub async fn get_collection_detail(
    id: EntityId,
    repository: &dyn CollectionRepository,
) -> Result<Option<CollectionDetailAdmin>> {
    if id <= 0 {
        return Err(ApplicationError::Validation {
            field: "collection_id",
            message: "must be greater than zero".to_owned(),
        });
    }
    repository
        .get_collection_detail_admin(id)
        .await
        .map_err(Into::into)
}

pub async fn list_collection_members(
    id: EntityId,
    repository: &dyn CollectionRepository,
) -> Result<Vec<CollectionMemberAdmin>> {
    if id <= 0 {
        return Err(ApplicationError::Validation {
            field: "collection_id",
            message: "must be greater than zero".to_owned(),
        });
    }
    repository
        .list_collection_members_admin(id)
        .await
        .map_err(Into::into)
}

const SCHEDULES_PAGE_SIZE_KEY: &str = "pages.schedules.page_size";
const SCHEDULES_DETAIL_PAGE_SIZE_KEY: &str = "pages.schedules.detail_page_size";

pub async fn get_schedules_page_sizes(
    settings: &dyn AdminSettingsRepository,
) -> Result<(u32, u32)> {
    Ok((
        positive_page_size(SCHEDULES_PAGE_SIZE_KEY, settings).await?,
        positive_page_size(SCHEDULES_DETAIL_PAGE_SIZE_KEY, settings).await?,
    ))
}

pub async fn save_schedules_detail_page_size(
    page_size: u32,
    settings: &dyn AdminSettingsRepository,
) -> Result<()> {
    settings
        .set_string(
            SCHEDULES_DETAIL_PAGE_SIZE_KEY,
            &page_size.max(1).to_string(),
        )
        .await
        .map_err(Into::into)
}

pub async fn list_schedules(
    query: ListCollections,
    repository: &dyn ProgramScheduleRepository,
    settings: &dyn AdminSettingsRepository,
) -> Result<(Vec<ScheduleAdminRow>, u64)> {
    settings
        .set_string(SCHEDULES_PAGE_SIZE_KEY, &query.page_size.max(1).to_string())
        .await?;
    repository
        .list_admin(&query.search, query.page, query.page_size.max(1))
        .await
        .map_err(Into::into)
}

pub async fn get_schedule(
    id: EntityId,
    repository: &dyn ProgramScheduleRepository,
) -> Result<Option<ScheduleAdminForm>> {
    if id < 0 {
        return Err(ApplicationError::Validation {
            field: "schedule_id",
            message: "must be zero or greater".to_owned(),
        });
    }
    if id == 0 {
        return Ok(Some(ScheduleAdminForm::default()));
    }
    repository.get_admin(id).await.map_err(Into::into)
}

pub async fn list_schedule_items(
    id: EntityId,
    repository: &dyn ProgramScheduleRepository,
) -> Result<Vec<ScheduleItemAdmin>> {
    if id <= 0 {
        return Err(ApplicationError::Validation {
            field: "schedule_id",
            message: "must be greater than zero".to_owned(),
        });
    }
    repository.list_items_admin(id).await.map_err(Into::into)
}

pub async fn get_schedule_item_options(
    repository: &dyn ProgramScheduleRepository,
) -> Result<ScheduleItemOptions> {
    repository
        .get_item_options_admin()
        .await
        .map_err(Into::into)
}

async fn positive_page_size(key: &str, settings: &dyn AdminSettingsRepository) -> Result<u32> {
    let value = get_i32_setting(key, 10, settings).await?;
    Ok(u32::try_from(value)
        .ok()
        .filter(|value| *value > 0)
        .unwrap_or(10))
}

/// Load the complete persisted `FFmpeg` settings editor read model.
#[allow(clippy::too_many_lines)]
pub async fn load_ffmpeg_settings(
    repository: &dyn AdminSettingsRepository,
) -> Result<FfmpegSettingsReadModel> {
    Ok(FfmpegSettingsReadModel {
        form: FfmpegSettings {
            ffmpeg_path: get_string_setting(
                ersatzrs_domain::CONFIG_KEY_FFMPEG_PATH,
                "",
                repository,
            )
            .await?,
            ffprobe_path: get_string_setting(
                ersatzrs_domain::CONFIG_KEY_FFPROBE_PATH,
                "",
                repository,
            )
            .await?,
            default_ffmpeg_profile_id: EntityId::from(
                get_i32_setting(
                    ersatzrs_domain::CONFIG_KEY_FFMPEG_DEFAULT_PROFILE_ID,
                    0,
                    repository,
                )
                .await?,
            ),
            preferred_audio_language_code: get_string_setting(
                ersatzrs_domain::CONFIG_KEY_FFMPEG_PREFERRED_LANGUAGE_CODE,
                "eng",
                repository,
            )
            .await?,
            preferred_subtitle_language_code: get_string_setting(
                ersatzrs_domain::CONFIG_KEY_FFMPEG_PREFERRED_SUBTITLE_LANGUAGE_CODE,
                "",
                repository,
            )
            .await?,
            fallback_subtitle_language_code: get_string_setting(
                ersatzrs_domain::CONFIG_KEY_FFMPEG_FALLBACK_SUBTITLE_LANGUAGE_CODE,
                "",
                repository,
            )
            .await?,
            use_embedded_subtitles: get_bool_setting(
                ersatzrs_domain::CONFIG_KEY_FFMPEG_USE_EMBEDDED_SUBTITLES,
                true,
                repository,
            )
            .await?,
            extract_embedded_subtitles: get_bool_setting(
                ersatzrs_domain::CONFIG_KEY_FFMPEG_EXTRACT_EMBEDDED_SUBTITLES,
                false,
                repository,
            )
            .await?,
            probe_for_interlaced_frames: get_bool_setting(
                ersatzrs_domain::CONFIG_KEY_FFMPEG_PROBE_FOR_INTERLACED_FRAMES,
                false,
                repository,
            )
            .await?,
            save_reports: get_bool_setting(
                ersatzrs_domain::CONFIG_KEY_FFMPEG_SAVE_REPORTS,
                false,
                repository,
            )
            .await?,
            global_watermark_id: get_optional_id_setting(
                ersatzrs_domain::CONFIG_KEY_FFMPEG_GLOBAL_WATERMARK_ID,
                repository,
            )
            .await?,
            global_fallback_filler_id: get_optional_id_setting(
                ersatzrs_domain::CONFIG_KEY_FFMPEG_GLOBAL_FALLBACK_FILLER_ID,
                repository,
            )
            .await?,
            hls_segmenter_idle_timeout: get_i32_setting(
                ersatzrs_domain::CONFIG_KEY_FFMPEG_SEGMENTER_TIMEOUT,
                60,
                repository,
            )
            .await?,
            work_ahead_segmenter_limit: get_i32_setting(
                ersatzrs_domain::CONFIG_KEY_FFMPEG_WORK_AHEAD_SEGMENTERS,
                1,
                repository,
            )
            .await?,
            active_transcode_limit: get_i32_setting(
                ersatzrs_domain::CONFIG_KEY_FFMPEG_ACTIVE_TRANSCODE_LIMIT,
                2,
                repository,
            )
            .await?,
            initial_segment_count: get_i32_setting(
                ersatzrs_domain::CONFIG_KEY_FFMPEG_INITIAL_SEGMENT_COUNT,
                1,
                repository,
            )
            .await?,
            hls_direct_output_format: get_i32_setting(
                ersatzrs_domain::CONFIG_KEY_FFMPEG_HLS_DIRECT_OUTPUT_FORMAT,
                2,
                repository,
            )
            .await?,
            default_mpeg_ts_script: get_string_setting(
                ersatzrs_domain::CONFIG_KEY_FFMPEG_DEFAULT_MPEG_TS_SCRIPT,
                "default",
                repository,
            )
            .await?,
        },
        ffmpeg_profiles: repository
            .list_options(SettingsOptionKind::FfmpegProfile)
            .await?,
        watermarks: repository
            .list_options(SettingsOptionKind::Watermark)
            .await?,
        fallback_fillers: repository
            .list_options(SettingsOptionKind::FallbackFiller)
            .await?,
        custom_resolutions: repository
            .list_options(SettingsOptionKind::CustomResolution)
            .await?,
    })
}

/// Load logging settings as one application read model.
pub async fn load_logging_settings(
    repository: &dyn AdminSettingsRepository,
) -> Result<LoggingSettings> {
    Ok(LoggingSettings {
        default_minimum_log_level: get_string_setting(
            ersatzrs_domain::CONFIG_KEY_MINIMUM_LOG_LEVEL,
            "Information",
            repository,
        )
        .await?,
        scanning_minimum_log_level: get_string_setting(
            ersatzrs_domain::CONFIG_KEY_MINIMUM_LOG_LEVEL_SCANNING,
            "Information",
            repository,
        )
        .await?,
        scheduling_minimum_log_level: get_string_setting(
            ersatzrs_domain::CONFIG_KEY_MINIMUM_LOG_LEVEL_SCHEDULING,
            "Information",
            repository,
        )
        .await?,
        searching_minimum_log_level: get_string_setting(
            ersatzrs_domain::CONFIG_KEY_MINIMUM_LOG_LEVEL_SEARCHING,
            "Information",
            repository,
        )
        .await?,
        streaming_minimum_log_level: get_string_setting(
            ersatzrs_domain::CONFIG_KEY_MINIMUM_LOG_LEVEL_STREAMING,
            "Information",
            repository,
        )
        .await?,
        http_minimum_log_level: get_string_setting(
            ersatzrs_domain::CONFIG_KEY_MINIMUM_LOG_LEVEL_HTTP,
            "Information",
            repository,
        )
        .await?,
    })
}

/// Load HDHR settings, creating and persisting the stable UUID when absent.
pub async fn load_hdhr_settings(repository: &dyn AdminSettingsRepository) -> Result<HdhrSettings> {
    let uuid = match get_optional_setting(ersatzrs_domain::CONFIG_KEY_HDHR_UUID, repository).await?
    {
        Some(value) if uuid::Uuid::parse_str(&value).is_ok() => value,
        _ => {
            let value = uuid::Uuid::new_v4().to_string();
            repository
                .set_string(ersatzrs_domain::CONFIG_KEY_HDHR_UUID, &value)
                .await?;
            value
        }
    };
    Ok(HdhrSettings {
        uuid,
        tuner_count: get_i32_setting(ersatzrs_domain::CONFIG_KEY_HDHR_TUNER_COUNT, 2, repository)
            .await?,
    })
}

pub async fn load_scanner_settings(
    repository: &dyn AdminSettingsRepository,
) -> Result<ScannerSettings> {
    Ok(ScannerSettings {
        library_refresh_interval: get_i32_setting(
            ersatzrs_domain::CONFIG_KEY_LIBRARY_REFRESH_INTERVAL,
            0,
            repository,
        )
        .await?,
    })
}

pub async fn load_playout_settings(
    repository: &dyn AdminSettingsRepository,
) -> Result<PlayoutSettings> {
    Ok(PlayoutSettings {
        days_to_build: get_i32_setting(
            ersatzrs_domain::CONFIG_KEY_PLAYOUT_DAYS_TO_BUILD,
            2,
            repository,
        )
        .await?,
        skip_missing_items: get_bool_setting(
            ersatzrs_domain::CONFIG_KEY_PLAYOUT_SKIP_MISSING_ITEMS,
            false,
            repository,
        )
        .await?,
        scripted_schedule_timeout_seconds: get_i32_setting(
            ersatzrs_domain::CONFIG_KEY_PLAYOUT_SCRIPTED_SCHEDULE_TIMEOUT_SECONDS,
            30,
            repository,
        )
        .await?,
    })
}

pub async fn load_ui_settings(repository: &dyn AdminSettingsRepository) -> Result<UiSettings> {
    Ok(UiSettings {
        is_dark_mode: get_bool_setting(
            ersatzrs_domain::CONFIG_KEY_PAGES_IS_DARK_MODE,
            true,
            repository,
        )
        .await?,
        language: get_string_setting(
            ersatzrs_domain::CONFIG_KEY_PAGES_LANGUAGE,
            "en-us",
            repository,
        )
        .await?,
    })
}

pub async fn load_xmltv_settings(
    repository: &dyn AdminSettingsRepository,
) -> Result<XmltvSettings> {
    Ok(XmltvSettings {
        days_to_build: get_i32_setting(
            ersatzrs_domain::CONFIG_KEY_XMLTV_DAYS_TO_BUILD,
            2,
            repository,
        )
        .await?,
        time_zone: get_string_setting(
            ersatzrs_domain::CONFIG_KEY_XMLTV_TIME_ZONE,
            "Local",
            repository,
        )
        .await?,
        block_behavior: get_string_setting(
            ersatzrs_domain::CONFIG_KEY_XMLTV_BLOCK_BEHAVIOR,
            "SplitTimeEvenly",
            repository,
        )
        .await?,
    })
}

/// Load an optional settings value without exposing storage.
pub async fn get_optional_setting(
    key: &str,
    repository: &dyn AdminSettingsRepository,
) -> Result<Option<String>> {
    repository
        .get_optional_string(key)
        .await
        .map_err(Into::into)
}

/// Load a settings string, defaulting only when the key is absent.
pub async fn get_string_setting(
    key: &str,
    default: &str,
    repository: &dyn AdminSettingsRepository,
) -> Result<String> {
    Ok(get_optional_setting(key, repository)
        .await?
        .unwrap_or_else(|| default.to_owned()))
}

/// Load a strictly decoded integer setting.
pub async fn get_i32_setting(
    key: &str,
    default: i32,
    repository: &dyn AdminSettingsRepository,
) -> Result<i32> {
    match get_optional_setting(key, repository).await? {
        None => Ok(default),
        Some(value) => value
            .parse::<i32>()
            .map_err(|_| ApplicationError::InvalidData {
                kind: "admin setting",
                value: format!("{key}={value}"),
            }),
    }
}

/// Load a strictly decoded optional entity id.
pub async fn get_optional_id_setting(
    key: &str,
    repository: &dyn AdminSettingsRepository,
) -> Result<Option<EntityId>> {
    match get_optional_setting(key, repository).await? {
        None => Ok(None),
        Some(value) if value.trim().is_empty() => Ok(None),
        Some(value) => {
            value
                .parse::<EntityId>()
                .map(Some)
                .map_err(|_| ApplicationError::InvalidData {
                    kind: "admin setting",
                    value: format!("{key}={value}"),
                })
        }
    }
}

/// Load a strictly decoded boolean setting.
pub async fn get_bool_setting(
    key: &str,
    default: bool,
    repository: &dyn AdminSettingsRepository,
) -> Result<bool> {
    match get_optional_setting(key, repository).await? {
        None => Ok(default),
        Some(value) => match value.trim().to_ascii_lowercase().as_str() {
            "true" | "1" => Ok(true),
            "false" | "0" => Ok(false),
            _ => Err(ApplicationError::InvalidData {
                kind: "admin setting",
                value: format!("{key}={value}"),
            }),
        },
    }
}

/// Load typed select options used by settings editors.
pub async fn list_settings_options(
    kind: SettingsOptionKind,
    repository: &dyn AdminSettingsRepository,
) -> Result<Vec<SettingsSelectOption>> {
    repository.list_options(kind).await.map_err(Into::into)
}

/// Load all playout ids used by settings side effects.
pub async fn list_settings_playout_ids(
    repository: &dyn AdminSettingsRepository,
) -> Result<Vec<EntityId>> {
    repository.list_playout_ids().await.map_err(Into::into)
}

/// Page through persisted Trakt lists.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ListTraktLists {
    /// Zero-based page index.
    pub page: u32,
    /// Page size persisted for the next visit.
    pub page_size: u32,
}

/// List Trakt rows and preserve the existing page-size preference.
pub async fn list_trakt_lists(
    query: ListTraktLists,
    repository: &dyn TraktRepository,
) -> Result<(Vec<TraktListAdminRow>, u64)> {
    let page_size = query.page_size.max(1);
    repository.set_page_size(page_size).await?;
    repository
        .list_admin(query.page, page_size)
        .await
        .map_err(Into::into)
}

/// Load one Trakt list editor form.
pub async fn get_trakt_list(
    id: EntityId,
    repository: &dyn TraktRepository,
) -> Result<Option<TraktListAdminForm>> {
    if id <= 0 {
        return Err(ApplicationError::Validation {
            field: "trakt_list_id",
            message: "must be greater than zero".to_owned(),
        });
    }
    repository.get_admin_form(id).await.map_err(Into::into)
}

/// Load the persisted Trakt list page size.
pub async fn get_trakt_page_size(repository: &dyn TraktRepository) -> Result<u32> {
    repository.get_page_size().await.map_err(Into::into)
}

/// Page through persisted channel list rows.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ListChannels {
    /// Include disabled channels when true.
    pub show_disabled: bool,
    /// Zero-based page index.
    pub page: u32,
    /// Requested page size.
    pub page_size: u32,
}

/// Load the complete channel editor state.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct GetChannelEditor {
    /// Positive id for edit, or zero for create defaults.
    pub channel_id: EntityId,
}

/// Load the channel-watch projection for a bounded admin guide window.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct GetChannelWatch {
    /// Channel identifier from the admin route.
    pub channel_id: EntityId,
    /// UTC instant used to select current and upcoming programming.
    pub now: chrono::DateTime<chrono::Utc>,
    /// Guide horizon in minutes.
    pub horizon_minutes: i64,
    /// Historical context before `now`, in minutes.
    pub history_minutes: i64,
}

/// Aggregate query for the global EPG preview.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct GetEpgPreview {
    /// UTC instant that anchors the two-hour history and 24-hour future window.
    pub now: chrono::DateTime<chrono::Utc>,
}

/// Storage-neutral aggregate projection for the global admin EPG.
#[derive(Clone, Debug, PartialEq)]
pub struct EpgPreviewData {
    pub generated_at: chrono::DateTime<chrono::Utc>,
    pub window_start: chrono::DateTime<chrono::Utc>,
    pub window_finish: chrono::DateTime<chrono::Utc>,
    pub channels: Vec<EpgPreviewChannel>,
}

/// One channel lane in [`EpgPreviewData`].
#[derive(Clone, Debug, PartialEq)]
pub struct EpgPreviewChannel {
    pub state: EpgPreviewChannelState,
    pub is_active: bool,
}

/// List channel rows without transport or storage dependencies.
pub async fn list_channels(
    query: ListChannels,
    repository: &dyn ChannelRepository,
) -> Result<Vec<ChannelAdminListRow>> {
    repository
        .list_admin_rows(query.show_disabled, query.page, query.page_size.max(1))
        .await
        .map_err(Into::into)
}

/// Count channels using the same disabled-row predicate as [`list_channels`].
pub async fn count_channels(
    show_disabled: bool,
    repository: &dyn ChannelRepository,
) -> Result<u64> {
    repository
        .count_admin_rows(show_disabled)
        .await
        .map_err(Into::into)
}

/// Load rows used by the atomic channel-number editor.
pub async fn list_channel_numbers(
    repository: &dyn ChannelRepository,
) -> Result<Vec<ChannelNumberRow>> {
    repository.list_number_rows().await.map_err(Into::into)
}

/// Load a create/edit channel aggregate and its select options.
pub async fn get_channel_editor(
    query: GetChannelEditor,
    repository: &dyn ChannelRepository,
) -> Result<Option<ChannelEditorState>> {
    repository
        .get_editor_state((query.channel_id > 0).then_some(query.channel_id))
        .await
        .map_err(Into::into)
}

/// Load channel identity and programmes for the preview modal and watch route.
pub async fn get_channel_watch(
    query: GetChannelWatch,
    repository: &dyn ChannelRepository,
) -> Result<Option<ersatzrs_core::repositories::ChannelWatchState>> {
    if query.channel_id <= 0 {
        return Err(ApplicationError::Validation {
            field: "channel_id",
            message: "must be greater than zero".to_owned(),
        });
    }
    if !(1..=1_440).contains(&query.horizon_minutes) {
        return Err(ApplicationError::Validation {
            field: "horizon_minutes",
            message: "must be between 1 and 1440".to_owned(),
        });
    }
    if !(0..=120).contains(&query.history_minutes) {
        return Err(ApplicationError::Validation {
            field: "history_minutes",
            message: "must be between 0 and 120".to_owned(),
        });
    }

    repository
        .get_watch_state(
            query.channel_id,
            query.now,
            query.now - chrono::Duration::minutes(query.history_minutes),
            query.now + chrono::Duration::minutes(query.horizon_minutes),
        )
        .await
        .map_err(Into::into)
}

/// Load the complete 26-hour EPG preview and enrich lanes with active-session
/// state without exposing runtime or storage adapters to the web transport.
pub async fn get_epg_preview(
    query: GetEpgPreview,
    repository: &dyn ChannelRepository,
    runtime: &dyn ChannelRuntime,
) -> Result<EpgPreviewData> {
    let window_start = query.now - chrono::Duration::hours(2);
    let window_finish = query.now + chrono::Duration::hours(24);
    let active = runtime
        .active_channel_numbers()
        .await
        .map_err(|error| ApplicationError::Unavailable {
            operation: "list active channel sessions",
            message: error.to_string(),
        })?
        .into_iter()
        .collect::<std::collections::HashSet<_>>();
    let channels = repository
        .list_epg_preview_states(query.now, window_start, window_finish)
        .await?
        .into_iter()
        .map(|state| EpgPreviewChannel {
            is_active: active.contains(&state.number),
            state,
        })
        .collect();

    Ok(EpgPreviewData {
        generated_at: query.now,
        window_start,
        window_finish,
        channels,
    })
}

#[cfg(test)]
mod epg_preview_query_tests {
    use async_trait::async_trait;
    use chrono::{TimeZone as _, Utc};
    use ersatzrs_core::repositories::{
        ChannelAdminListRow, ChannelEditorForm, ChannelEditorState, ChannelNumberRow,
        ChannelNumberSaveResult, ChannelNumberUpdate, ChannelRepository, ChannelSaveResult,
        ChannelSaveSideEffects, ChannelSelectOption, ChannelValidationState, ChannelWatchState,
        EpgPreviewChannelState, RepositoryError, Result as RepositoryResult,
    };
    use ersatzrs_domain::{Channel, EntityId};

    use super::{
        get_channel_watch, get_epg_preview, ApplicationError, GetChannelWatch, GetEpgPreview,
    };
    use crate::ports::{ChannelRuntime, ChannelRuntimeError};

    struct EpgRepository {
        fail: bool,
        channels: Vec<EpgPreviewChannelState>,
    }

    #[async_trait]
    impl ChannelRepository for EpgRepository {
        async fn get_with_profile(&self, _id: EntityId) -> RepositoryResult<Channel> {
            unimplemented!()
        }
        async fn get_by_number(&self, _number: &str) -> RepositoryResult<Channel> {
            unimplemented!()
        }
        async fn list_enabled(&self) -> RepositoryResult<Vec<Channel>> {
            unimplemented!()
        }
        async fn upsert(&self, _channel: &Channel) -> RepositoryResult<()> {
            unimplemented!()
        }
        async fn delete(&self, _id: EntityId) -> RepositoryResult<()> {
            unimplemented!()
        }
        async fn get_editor_state(
            &self,
            _id: Option<EntityId>,
        ) -> RepositoryResult<Option<ChannelEditorState>> {
            unimplemented!()
        }
        async fn save_editor_form(
            &self,
            _form: ChannelEditorForm,
        ) -> RepositoryResult<ChannelSaveResult> {
            unimplemented!()
        }
        async fn list_number_rows(&self) -> RepositoryResult<Vec<ChannelNumberRow>> {
            unimplemented!()
        }
        async fn save_number_updates(
            &self,
            _updates: Vec<ChannelNumberUpdate>,
        ) -> RepositoryResult<ChannelNumberSaveResult> {
            unimplemented!()
        }
        async fn list_admin_rows(
            &self,
            _show_disabled: bool,
            _page: u32,
            _page_size: u32,
        ) -> RepositoryResult<Vec<ChannelAdminListRow>> {
            unimplemented!()
        }
        async fn count_admin_rows(&self, _show_disabled: bool) -> RepositoryResult<u64> {
            unimplemented!()
        }
        async fn get_watch_state(
            &self,
            _id: EntityId,
            _now: chrono::DateTime<Utc>,
            _start: chrono::DateTime<Utc>,
            _finish: chrono::DateTime<Utc>,
        ) -> RepositoryResult<Option<ChannelWatchState>> {
            unimplemented!()
        }
        async fn list_epg_preview_states(
            &self,
            _now: chrono::DateTime<Utc>,
            _start: chrono::DateTime<Utc>,
            _finish: chrono::DateTime<Utc>,
        ) -> RepositoryResult<Vec<EpgPreviewChannelState>> {
            if self.fail {
                Err(RepositoryError::unavailable(
                    "load aggregate EPG preview",
                    "injected failure",
                ))
            } else {
                Ok(self.channels.clone())
            }
        }
        async fn list_ffmpeg_profile_options(&self) -> RepositoryResult<Vec<ChannelSelectOption>> {
            unimplemented!()
        }
        async fn list_filler_preset_options(&self) -> RepositoryResult<Vec<ChannelSelectOption>> {
            unimplemented!()
        }
        async fn validation_state(&self) -> RepositoryResult<ChannelValidationState> {
            unimplemented!()
        }
        async fn save_side_effects(
            &self,
            _id: EntityId,
        ) -> RepositoryResult<Option<ChannelSaveSideEffects>> {
            unimplemented!()
        }
    }

    struct Runtime {
        active: Vec<String>,
        fail: bool,
    }

    #[async_trait]
    impl ChannelRuntime for Runtime {
        async fn active_channel_numbers(&self) -> Result<Vec<String>, ChannelRuntimeError> {
            if self.fail {
                Err(ChannelRuntimeError::unavailable("injected failure"))
            } else {
                Ok(self.active.clone())
            }
        }

        async fn stop_channel_session(&self, _number: &str) -> Result<bool, ChannelRuntimeError> {
            unimplemented!()
        }

        fn request_subtitle_extraction(&self, _playout_ids: Vec<EntityId>) {
            unimplemented!()
        }
    }

    fn channel(number: &str) -> EpgPreviewChannelState {
        EpgPreviewChannelState {
            id: 1,
            number: number.to_owned(),
            sort_number: 1.0,
            name: "Channel".to_owned(),
            logo_path: None,
            ffmpeg_profile_name: "Profile".to_owned(),
            streaming_mode: 5,
            playout_id: None,
            programmes: Vec::new(),
        }
    }

    #[tokio::test]
    async fn aggregate_epg_uses_full_window_and_active_runtime_state() {
        let now = Utc.with_ymd_and_hms(2026, 7, 26, 12, 0, 0).unwrap();
        let data = get_epg_preview(
            GetEpgPreview { now },
            &EpgRepository {
                fail: false,
                channels: vec![channel("10")],
            },
            &Runtime {
                active: vec!["10".to_owned()],
                fail: false,
            },
        )
        .await
        .expect("aggregate EPG");

        assert_eq!(data.window_start, now - chrono::Duration::hours(2));
        assert_eq!(data.window_finish, now + chrono::Duration::hours(24));
        assert!(data.channels[0].is_active);
    }

    #[tokio::test]
    async fn aggregate_epg_preserves_typed_repository_and_runtime_failures() {
        let now = Utc.with_ymd_and_hms(2026, 7, 26, 12, 0, 0).unwrap();
        let repository_error = get_epg_preview(
            GetEpgPreview { now },
            &EpgRepository {
                fail: true,
                channels: Vec::new(),
            },
            &Runtime {
                active: Vec::new(),
                fail: false,
            },
        )
        .await
        .expect_err("typed repository failure");
        assert!(matches!(
            repository_error,
            ApplicationError::Unavailable {
                operation: "load aggregate EPG preview",
                ..
            }
        ));

        let runtime_error = get_epg_preview(
            GetEpgPreview { now },
            &EpgRepository {
                fail: false,
                channels: Vec::new(),
            },
            &Runtime {
                active: Vec::new(),
                fail: true,
            },
        )
        .await
        .expect_err("typed runtime failure");
        assert!(matches!(
            runtime_error,
            ApplicationError::Unavailable {
                operation: "list active channel sessions",
                ..
            }
        ));
    }

    #[tokio::test]
    async fn channel_watch_rejects_invalid_history_before_storage() {
        let error = get_channel_watch(
            GetChannelWatch {
                channel_id: 1,
                now: Utc::now(),
                horizon_minutes: 1_440,
                history_minutes: 121,
            },
            &EpgRepository {
                fail: false,
                channels: Vec::new(),
            },
        )
        .await
        .expect_err("invalid history");
        assert!(matches!(
            error,
            ApplicationError::Validation {
                field: "history_minutes",
                ..
            }
        ));
    }
}

/// Load FFmpeg-profile channel select options.
pub async fn list_channel_ffmpeg_profiles(
    repository: &dyn ChannelRepository,
) -> Result<Vec<ChannelSelectOption>> {
    repository
        .list_ffmpeg_profile_options()
        .await
        .map_err(Into::into)
}

/// Load filler-preset channel select options.
pub async fn list_channel_filler_presets(
    repository: &dyn ChannelRepository,
) -> Result<Vec<ChannelSelectOption>> {
    repository
        .list_filler_preset_options()
        .await
        .map_err(Into::into)
}

/// Query active process-local channel sessions.
pub async fn list_active_channel_numbers(runtime: &dyn ChannelRuntime) -> Result<Vec<String>> {
    runtime
        .active_channel_numbers()
        .await
        .map_err(|error| ApplicationError::Unavailable {
            operation: "list active channel sessions",
            message: error.to_string(),
        })
}

/// Stop one process-local channel session.
pub async fn stop_channel_session(number: &str, runtime: &dyn ChannelRuntime) -> Result<bool> {
    let number = number.trim();
    if number.is_empty() {
        return Err(ApplicationError::Validation {
            field: "number",
            message: "must not be empty".to_owned(),
        });
    }
    runtime
        .stop_channel_session(number)
        .await
        .map_err(|error| ApplicationError::Unavailable {
            operation: "stop channel session",
            message: error.to_string(),
        })
}

/// One unsaved block item adapted from a transport draft.
#[derive(Clone, Debug, PartialEq)]
pub struct PreviewBlockItem {
    /// Draft order shown in the editor.
    pub index: i32,
    /// Real scheduler collection/reference identity.
    pub collection_key: SchedulingCollectionKey,
    /// Real scheduler playback order.
    pub playback_order: PlaybackOrder,
    /// Whether generated rows remain normal guide content.
    pub include_in_program_guide: bool,
    /// Whether generated rows suppress channel/global watermarks.
    pub disable_watermarks: bool,
    /// Watermarks attached to this unsaved item.
    pub watermark_ids: Vec<EntityId>,
    /// Graphics elements attached to this unsaved item.
    pub graphics_element_ids: Vec<EntityId>,
    /// Unsaved Smart Shuffle attachment for this item.
    pub smart_shuffle_policy: SchedulingSmartShufflePolicy,
}

/// Query the real block scheduler for an unsaved editor draft.
#[derive(Clone, Debug, PartialEq)]
pub struct PreviewBlockPlayout {
    /// Unsaved block name.
    pub name: String,
    /// Unsaved block duration in minutes.
    pub minutes: i32,
    /// Unsaved block stop behavior.
    pub stop_scheduling: BlockStopScheduling,
    /// Unsaved ordered block items.
    pub items: Vec<PreviewBlockItem>,
}

/// Enriched row returned from the real block builder.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PreviewBlockPlayoutRow {
    /// Real media identity selected by the scheduler.
    pub media_item_id: EntityId,
    /// Actual generated start instant.
    pub start: DateTime<Utc>,
    /// Actual generated finish instant.
    pub finish: DateTime<Utc>,
    /// Title hydrated from real media/reference data.
    pub title: String,
    /// Actual generated row duration.
    pub duration: Duration,
}

/// One unsaved playlist item adapted from the admin editor.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PreviewPlaylistItem {
    pub collection_type: i32,
    pub collection_id: Option<EntityId>,
    pub multi_collection_id: Option<EntityId>,
    pub smart_collection_id: Option<EntityId>,
    pub media_item_id: Option<EntityId>,
    pub playback_order: PlaybackOrder,
    pub play_all: bool,
    pub count: Option<usize>,
    pub include_in_program_guide: bool,
    pub preferred_audio_language_code: Option<String>,
    pub preferred_audio_title: Option<String>,
    pub preferred_subtitle_language_code: Option<String>,
    pub subtitle_mode: Option<ersatzrs_domain::ChannelSubtitleMode>,
}

/// Query the real classic scheduler for an unsaved playlist draft.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PreviewPlaylistPlayout {
    pub playlist_id: EntityId,
    pub items: Vec<PreviewPlaylistItem>,
}

/// Enriched row returned from the real playlist preview.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PreviewPlaylistPlayoutRow {
    pub start: Duration,
    pub finish: Duration,
    pub title: String,
    pub duration: Duration,
}

/// Execute an unsaved block preview through the real scheduling builder.
///
/// This mirrors
/// `$ERSATZTV_SRC/ErsatzTV.Application/Scheduling/Commands/PreviewBlockPlayoutHandler.cs`:
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PlayoutDetailReadModel {
    pub title: String,
    pub start: String,
    pub finish: String,
    pub duration: String,
    pub scheduling_context: Option<String>,
    pub filler_kind: i32,
}

pub async fn list_playouts(
    query: &str,
    page: u32,
    page_size: u32,
    repository: &dyn PlayoutRepository,
) -> Result<Vec<PlayoutListAdminRow>> {
    let limit = i64::from(page_size.max(1));
    let offset = i64::from(page).saturating_mul(limit);
    repository
        .list_admin(query, offset, limit)
        .await
        .map_err(Into::into)
}

pub async fn count_playouts(repository: &dyn PlayoutRepository) -> Result<u64> {
    repository.count_admin().await.map_err(Into::into)
}

pub async fn count_playout_warnings(repository: &dyn PlayoutRepository) -> Result<u32> {
    repository
        .count_failed_builds_admin()
        .await
        .map_err(Into::into)
}

pub async fn media_state_revision(repository: &dyn LibraryRepository) -> Result<String> {
    let revision = repository.media_state_revision_admin().await?;
    Ok(format!(
        "{}|{}|{}",
        revision.latest_library_scan.unwrap_or_default(),
        revision.latest_path_scan.unwrap_or_default(),
        revision.unavailable_count
    ))
}

pub async fn admin_search_snapshot(
    query: &str,
    repository: &dyn AdminSearchRepository,
) -> Result<AdminSearchSnapshot> {
    let query = query.trim();
    if query.is_empty() {
        return Ok(AdminSearchSnapshot::default());
    }
    let documents = repository.search_admin_documents(query).await?;
    let mut enrichment = std::collections::HashMap::new();
    for (kind, media_kind) in [
        (AdminSearchKind::Movie, "movie"),
        (AdminSearchKind::Show, "show"),
        (AdminSearchKind::Season, "season"),
        (AdminSearchKind::Episode, "episode"),
        (AdminSearchKind::Artist, "artist"),
        (AdminSearchKind::MusicVideo, "music_video"),
        (AdminSearchKind::OtherVideo, "other_video"),
        (AdminSearchKind::Song, "song"),
        (AdminSearchKind::Image, "image"),
        (AdminSearchKind::RemoteStream, "remote_stream"),
    ] {
        let ids = documents
            .iter()
            .filter(|document| document.media_kind == media_kind)
            .map(|document| document.media_item_id)
            .collect::<Vec<_>>();
        enrichment.insert(kind, repository.enrich_admin_search(kind, &ids).await?);
    }
    Ok(AdminSearchSnapshot {
        documents,
        enrichment,
    })
}

pub async fn admin_search_documents(
    query: &str,
    repository: &dyn AdminSearchRepository,
) -> Result<Vec<ersatzrs_core::search::SearchDocument>> {
    let query = query.trim();
    if query.is_empty() {
        return Ok(Vec::new());
    }
    repository
        .search_admin_documents(query)
        .await
        .map_err(Into::into)
}

#[cfg(test)]
mod admin_search_query_tests {
    use super::{admin_search_snapshot, ApplicationError};
    use async_trait::async_trait;
    use ersatzrs_core::repositories::{
        AdminSearchEnrichment, AdminSearchKind, AdminSearchRepository, RepositoryError,
    };
    use ersatzrs_core::search::SearchDocument;
    use ersatzrs_domain::EntityId;

    struct SearchRepository {
        fail: bool,
    }

    #[async_trait]
    impl AdminSearchRepository for SearchRepository {
        async fn search_admin_documents(
            &self,
            _query: &str,
        ) -> ersatzrs_core::repositories::Result<Vec<SearchDocument>> {
            if self.fail {
                Err(RepositoryError::unavailable(
                    "search admin media",
                    "injected failure",
                ))
            } else {
                Ok(Vec::new())
            }
        }

        async fn enrich_admin_search(
            &self,
            _kind: AdminSearchKind,
            _ids: &[EntityId],
        ) -> ersatzrs_core::repositories::Result<AdminSearchEnrichment> {
            Ok(AdminSearchEnrichment::default())
        }
    }

    #[tokio::test]
    async fn valid_empty_search_is_distinct_from_typed_repository_failure() {
        let empty = admin_search_snapshot("nothing", &SearchRepository { fail: false })
            .await
            .expect("valid empty search");
        assert!(empty.documents.is_empty());

        let error = admin_search_snapshot("anything", &SearchRepository { fail: true })
            .await
            .expect_err("repository failure remains typed");
        assert!(matches!(
            error,
            ApplicationError::Unavailable {
                operation: "search admin media",
                ..
            }
        ));
    }
}

pub async fn list_playout_detail_items(
    playout_id: EntityId,
    show_filler: bool,
    page: u32,
    page_size: u32,
    repository: &dyn PlayoutRepository,
) -> Result<Vec<PlayoutDetailReadModel>> {
    if playout_id <= 0 {
        return Err(ApplicationError::Validation {
            field: "playout_id",
            message: "must be greater than zero".to_owned(),
        });
    }
    let limit = i64::from(page_size.max(1));
    let offset = i64::from(page).saturating_mul(limit);
    repository
        .list_detail_admin(playout_id, show_filler, Utc::now(), offset, limit)
        .await?
        .into_iter()
        .map(|item| {
            Ok(PlayoutDetailReadModel {
                title: item.title,
                start: format_playout_local(&item.start)?,
                finish: format_playout_local(&item.finish)?,
                duration: display_playout_duration(&item.start, &item.finish)?,
                scheduling_context: item.scheduling_context,
                filler_kind: item.filler_kind,
            })
        })
        .collect()
}

pub async fn count_playout_detail_items(
    playout_id: EntityId,
    show_filler: bool,
    repository: &dyn PlayoutRepository,
) -> Result<u64> {
    if playout_id <= 0 {
        return Err(ApplicationError::Validation {
            field: "playout_id",
            message: "must be greater than zero".to_owned(),
        });
    }
    repository
        .count_detail_admin(playout_id, show_filler, Utc::now())
        .await
        .map_err(Into::into)
}

pub async fn get_playout_kind_editor(
    id: EntityId,
    schedule_kind: i32,
    repository: &dyn PlayoutRepository,
) -> Result<Option<PlayoutKindEditorAdmin>> {
    if id <= 0 {
        return Err(ApplicationError::Validation {
            field: "playout_id",
            message: "must be greater than zero".to_owned(),
        });
    }
    if !matches!(schedule_kind, 1 | 2 | 4) {
        return Ok(None);
    }
    repository
        .get_kind_editor_admin(id, schedule_kind)
        .await
        .map_err(Into::into)
}

pub async fn get_sequential_playout(
    id: EntityId,
    repository: &dyn PlayoutRepository,
) -> Result<Option<SequentialPlayoutAdmin>> {
    if id <= 0 {
        return Err(ApplicationError::Validation {
            field: "playout_id",
            message: "must be greater than zero".to_owned(),
        });
    }
    repository
        .get_sequential_admin(id)
        .await
        .map_err(Into::into)
}

pub async fn get_playout_alternate_schedules(
    id: EntityId,
    repository: &dyn PlayoutRepository,
) -> Result<Option<PlayoutAlternateSchedulesAdmin>> {
    if id <= 0 {
        return Err(ApplicationError::Validation {
            field: "playout_id",
            message: "must be greater than zero".to_owned(),
        });
    }
    repository
        .get_alternate_schedules_admin(id)
        .await
        .map_err(Into::into)
}

pub async fn get_playout_templates(
    id: EntityId,
    repository: &dyn PlayoutRepository,
) -> Result<Option<PlayoutTemplatesAdmin>> {
    if id <= 0 {
        return Err(ApplicationError::Validation {
            field: "playout_id",
            message: "must be greater than zero".to_owned(),
        });
    }
    repository.get_templates_admin(id).await.map_err(Into::into)
}

pub async fn process_scheduling_context(
    serialized: &str,
    repository: &dyn PlayoutRepository,
) -> Result<String> {
    repository
        .enrich_scheduling_context_admin(serialized)
        .await
        .map_err(Into::into)
}

fn parse_playout_utc(value: &str) -> Result<DateTime<Utc>> {
    DateTime::parse_from_rfc3339(value)
        .map(|value| value.with_timezone(&Utc))
        .or_else(|_| {
            chrono::NaiveDateTime::parse_from_str(value, "%Y-%m-%d %H:%M:%S%.f")
                .map(|value| value.and_utc())
        })
        .map_err(|error| ApplicationError::InvalidData {
            kind: "playout timestamp",
            value: format!("{value}: {error}"),
        })
}

fn format_playout_local(value: &str) -> Result<String> {
    use chrono::TimeZone as _;
    let utc = parse_playout_utc(value)?;
    let local = chrono::Local
        .timestamp_opt(utc.timestamp(), utc.timestamp_subsec_nanos())
        .single()
        .ok_or_else(|| ApplicationError::InvalidData {
            kind: "local playout timestamp",
            value: value.to_owned(),
        })?;
    Ok(local.format("%Y-%m-%d %H:%M").to_string())
}

fn display_playout_duration(start: &str, finish: &str) -> Result<String> {
    let seconds = (parse_playout_utc(finish)? - parse_playout_utc(start)?)
        .num_seconds()
        .max(0);
    let hours = seconds / 3600;
    let minutes = (seconds % 3600) / 60;
    let seconds = seconds % 60;
    Ok(if hours > 0 {
        format!("{hours}:{minutes:02}:{seconds:02}")
    } else {
        format!("{minutes}:{seconds:02}")
    })
}

/// construct temporary template/playout/reference state, build in Reset mode,
/// then enrich the builder's actual added rows.
///
/// # Errors
///
/// Returns a typed validation, repository, scheduling, or enrichment failure.
pub async fn preview_block_playout(
    query: PreviewBlockPlayout,
    repository: &dyn ersatzrs_core::scheduling::BlockPreviewRepository,
    builder: &dyn BlockPreviewBuilder,
) -> Result<Vec<PreviewBlockPlayoutRow>> {
    preview_block_playout_at(query, repository, builder, Utc::now()).await
}

/// Execute an unsaved playlist preview through the real classic scheduler.
#[allow(clippy::too_many_lines)]
pub async fn preview_playlist_playout(
    query: PreviewPlaylistPlayout,
    repository: &dyn ersatzrs_core::scheduling::BlockPreviewRepository,
    builder: &dyn BlockPreviewBuilder,
) -> Result<Vec<PreviewPlaylistPlayoutRow>> {
    let mut effective_items = Vec::new();
    let mut included_items = Vec::new();
    for (index, item) in query.items.into_iter().enumerate() {
        let collection_key = playlist_collection_key(&item)?;
        if !item.include_in_program_guide {
            continue;
        }
        let id = i64::try_from(index).unwrap_or_default() + 1;
        let content = format!("playlist-preview-item:{id}");
        effective_items.push(EffectiveBlockItem {
            id,
            block_id: 1,
            content: content.clone(),
            collection_key: collection_key.clone(),
            playback_order: item.playback_order,
            include_in_program_guide: true,
            disable_watermarks: false,
            watermark_ids: Vec::new(),
            graphics_element_ids: Vec::new(),
        });
        included_items.push((content, collection_key, item));
    }
    if included_items.is_empty() {
        return Err(ApplicationError::Validation {
            field: "playlist_items",
            message: "playout preview has no items".to_owned(),
        });
    }

    let mut reference_data = repository
        .load_block_preview_content(&effective_items, &BTreeMap::new())
        .await?;
    let mut playlist_items = Vec::new();
    let mut one_pass_duration = Duration::zero();
    let mut max_items = 0usize;
    for (content, collection_key, item) in included_items {
        let mut media_items = reference_data
            .media_by_key
            .remove(&content)
            .unwrap_or_default();
        for media in &mut media_items {
            media
                .preferred_audio_language_code
                .clone_from(&item.preferred_audio_language_code);
            media
                .preferred_audio_title
                .clone_from(&item.preferred_audio_title);
            media
                .preferred_subtitle_language_code
                .clone_from(&item.preferred_subtitle_language_code);
            media.subtitle_mode = item.subtitle_mode;
        }
        let take = if item.play_all {
            media_items.len()
        } else {
            item.count.unwrap_or(1).max(1)
        };
        if media_items.is_empty() {
            continue;
        }
        max_items = max_items.saturating_add(take);
        one_pass_duration += media_items
            .iter()
            .cycle()
            .take(take)
            .fold(Duration::zero(), |duration, media| {
                duration + media.duration
            });
        playlist_items.push(SchedulingPlaylistItem {
            collection_key,
            playback_order: item.playback_order,
            play_all: item.play_all,
            count: item.count,
            media_items,
        });
    }
    if max_items == 0 || one_pass_duration <= Duration::zero() {
        return Err(ApplicationError::Validation {
            field: "playlist_items",
            message: "playout preview has no items".to_owned(),
        });
    }

    let now = Utc::now();
    let schedule_item = ProgramScheduleItem {
        id: 1,
        program_schedule_id: 1,
        collection_type: CollectionType::Playlist,
        playlist_id: Some(query.playlist_id),
        playout_mode: PlayoutMode::Flood,
        playback_order: PlaybackOrder::Shuffle,
        start_type: StartType::Dynamic,
        fixed_start_time_behavior: Some(FixedStartTimeBehavior::Strict),
        ..ProgramScheduleItem::default()
    };
    let schedule_key = ersatzrs_core::scheduling::classic_schedule_item_key(&schedule_item);
    reference_data.classic_schedule_items = vec![schedule_item];
    reference_data
        .playlist_items_by_key
        .insert(schedule_key, playlist_items);
    reference_data.classic_fixed_start_time_behavior = FixedStartTimeBehavior::Strict;
    let media_items = reference_data.media_items_by_id.clone();
    let result = builder
        .build(SchedulingRequest {
            playout: Playout {
                schedule_kind: PlayoutScheduleKind::Classic,
                seed: 1,
                ..Playout::default()
            },
            start: now,
            finish: now + one_pass_duration + Duration::seconds(1),
            mode: PlayoutBuildMode::Reset,
            reference_data,
        })
        .map_err(|error| ApplicationError::Scheduling {
            operation: "playlist preview",
            message: error.to_string(),
        })?;

    let rows = result
        .added_items
        .into_iter()
        .take(max_items)
        .map(|item| {
            let media =
                media_items
                    .get(&item.media_item_id)
                    .ok_or(ApplicationError::MissingReference {
                        kind: "playlist preview media item",
                        id: item.media_item_id,
                    })?;
            Ok(PreviewPlaylistPlayoutRow {
                start: item.start - now,
                finish: item.finish - now,
                title: media.title.clone().unwrap_or_default(),
                duration: item.finish - item.start,
            })
        })
        .collect::<Result<Vec<_>>>()?;
    if rows.is_empty() {
        return Err(ApplicationError::Validation {
            field: "playlist_items",
            message: "playout preview has no items".to_owned(),
        });
    }
    Ok(rows)
}

fn playlist_collection_key(item: &PreviewPlaylistItem) -> Result<SchedulingCollectionKey> {
    let collection_type = match item.collection_type {
        0 => CollectionType::Collection,
        1 => CollectionType::TelevisionShow,
        2 => CollectionType::TelevisionSeason,
        3 => CollectionType::Artist,
        4 => CollectionType::MultiCollection,
        5 => CollectionType::SmartCollection,
        10 => CollectionType::Movie,
        20 => CollectionType::Episode,
        30 => CollectionType::MusicVideo,
        40 => CollectionType::OtherVideo,
        50 => CollectionType::Song,
        60 => CollectionType::Image,
        70 => CollectionType::RemoteStream,
        _ => {
            return Err(ApplicationError::Validation {
                field: "collection_type",
                message: "is invalid".to_owned(),
            });
        }
    };
    let key = SchedulingCollectionKey {
        collection_type,
        collection_id: item.collection_id,
        multi_collection_id: item.multi_collection_id,
        smart_collection_id: item.smart_collection_id,
        media_item_id: item.media_item_id,
        ..SchedulingCollectionKey::default()
    };
    validate_collection_key(&key)?;
    Ok(key)
}

async fn preview_block_playout_at(
    query: PreviewBlockPlayout,
    repository: &dyn ersatzrs_core::scheduling::BlockPreviewRepository,
    builder: &dyn BlockPreviewBuilder,
    start: DateTime<Utc>,
) -> Result<Vec<PreviewBlockPlayoutRow>> {
    validate_preview_query(&query)?;

    let (playout, effective_blocks, policies) = temporary_block_state(&query, start);
    let temporary_items = effective_blocks
        .iter()
        .flat_map(|block| block.items.iter().cloned())
        .collect::<Vec<_>>();
    let mut reference_data = repository
        .load_block_preview_content(&temporary_items, &policies)
        .await?;
    reference_data.effective_blocks = effective_blocks;

    let media_items = reference_data.media_items_by_id.clone();
    let result = builder
        .build(SchedulingRequest {
            playout,
            start,
            finish: start + Duration::days(1),
            mode: PlayoutBuildMode::Reset,
            reference_data,
        })
        .map_err(|error| ApplicationError::Scheduling {
            operation: "block preview",
            message: error.to_string(),
        })?;

    result
        .added_items
        .into_iter()
        .map(|item| {
            let media =
                media_items
                    .get(&item.media_item_id)
                    .ok_or(ApplicationError::MissingReference {
                        kind: "block preview media item",
                        id: item.media_item_id,
                    })?;
            Ok(PreviewBlockPlayoutRow {
                media_item_id: item.media_item_id,
                start: item.start,
                finish: item.finish,
                title: media.title.clone().unwrap_or_default(),
                duration: item.finish - item.start,
            })
        })
        .collect()
}

fn validate_preview_query(query: &PreviewBlockPlayout) -> Result<()> {
    if query.minutes <= 0 {
        return Err(ApplicationError::Validation {
            field: "minutes",
            message: "must be greater than zero".to_owned(),
        });
    }

    for item in &query.items {
        validate_collection_key(&item.collection_key)?;
    }
    Ok(())
}

fn validate_collection_key(key: &SchedulingCollectionKey) -> Result<()> {
    let (field, valid) = match key.collection_type {
        CollectionType::Collection => ("collection_id", key.collection_id.is_some()),
        CollectionType::TelevisionShow
        | CollectionType::TelevisionSeason
        | CollectionType::Artist
        | CollectionType::Movie
        | CollectionType::Episode
        | CollectionType::MusicVideo
        | CollectionType::OtherVideo
        | CollectionType::Song
        | CollectionType::Image
        | CollectionType::RemoteStream => ("media_item_id", key.media_item_id.is_some()),
        CollectionType::MultiCollection => {
            ("multi_collection_id", key.multi_collection_id.is_some())
        }
        CollectionType::SmartCollection => {
            ("smart_collection_id", key.smart_collection_id.is_some())
        }
        CollectionType::Playlist => ("playlist_id", key.playlist_id.is_some()),
        CollectionType::SearchQuery => (
            "search_query",
            key.search_query
                .as_deref()
                .is_some_and(|query| !query.trim().is_empty()),
        ),
        CollectionType::RerunFirstRun
        | CollectionType::RerunRerun
        | CollectionType::FakeCollection
        | CollectionType::FakePlaylistItem => {
            return Err(ApplicationError::Validation {
                field: "collection_type",
                message: format!(
                    "{:?} is not supported by the real block scheduler",
                    key.collection_type
                ),
            });
        }
    };

    if valid {
        Ok(())
    } else {
        Err(ApplicationError::Validation {
            field,
            message: "is required for this collection type".to_owned(),
        })
    }
}

fn temporary_block_state(
    query: &PreviewBlockPlayout,
    start: DateTime<Utc>,
) -> (
    Playout,
    Vec<EffectiveBlock>,
    BTreeMap<String, SchedulingSmartShufflePolicy>,
) {
    const TEMPORARY_ID: EntityId = 1;

    let mut draft_items = query.items.clone();
    draft_items.sort_by_key(|item| item.index);
    let mut policies = BTreeMap::new();
    let items = draft_items
        .into_iter()
        .enumerate()
        .map(|(index, item)| {
            let id = i64::try_from(index).unwrap_or_default() + 1;
            policies.insert(id.to_string(), item.smart_shuffle_policy);
            EffectiveBlockItem {
                id,
                block_id: TEMPORARY_ID,
                content: format!("block-preview-item:{id}"),
                collection_key: item.collection_key,
                playback_order: item.playback_order,
                include_in_program_guide: item.include_in_program_guide,
                disable_watermarks: item.disable_watermarks,
                watermark_ids: item.watermark_ids,
                graphics_element_ids: item.graphics_element_ids,
            }
        })
        .collect::<Vec<_>>();

    let block = BlockDefinition {
        id: TEMPORARY_ID,
        updated_ticks: 0,
        name: query.name.clone(),
        duration: Duration::minutes(i64::from(query.minutes)),
        stop_scheduling: query.stop_scheduling,
        items,
    };
    let template = BlockTemplate {
        id: TEMPORARY_ID,
        updated_ticks: 0,
        items: vec![BlockTemplateItem {
            id: TEMPORARY_ID,
            block_id: TEMPORARY_ID,
            start_time: Some(NaiveTime::MIN),
            sort_order: 0,
        }],
    };
    let schedule = BlockTemplateSchedule {
        id: TEMPORARY_ID,
        updated_ticks: 0,
        start: None,
        finish: None,
        index: 0,
        days_of_week: (0..=6).collect(),
        days_of_month: (1..=31).collect(),
        months_of_year: (1..=12).collect(),
        limit_to_date_range: false,
        start_month: 1,
        start_day: 1,
        start_year: None,
        end_month: 12,
        end_day: 31,
        end_year: None,
        template,
        blocks: BTreeMap::from([(TEMPORARY_ID, block)]),
        deco: None,
        deco_template_items: Vec::new(),
    };
    let effective_blocks = EffectiveBlock::from_templates(&[schedule], start, 1);
    let playout = Playout {
        id: TEMPORARY_ID,
        schedule_kind: PlayoutScheduleKind::Block,
        ..Playout::default()
    };

    (playout, effective_blocks, policies)
}

/// Request the current state of one long-running operation.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct GetOperationStatus {
    /// Stable operation identity to inspect.
    pub key: OperationKey,
}

impl GetOperationStatus {
    /// Create an operation-status query.
    #[must_use]
    pub const fn new(key: OperationKey) -> Self {
        Self { key }
    }
}

/// Application-level state returned for an operation query.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum OperationStatus {
    /// No worker currently owns the operation lock.
    Idle,
    /// A caller or worker currently owns the operation lock.
    Running,
}

/// Lifecycle state for one accepted scan operation.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum ScanOperationState {
    /// Accepted work is waiting in the FIFO queue.
    Queued,
    /// The sole scanner worker is executing the request.
    Running,
    /// The request completed successfully and the library is idle.
    Completed,
    /// The request failed; the diagnostic remains observable after cleanup.
    Failed {
        /// Storage-neutral worker or adapter diagnostic.
        message: String,
    },
}

/// Application read model for one scan operation.
#[derive(Clone, Debug, PartialEq)]
pub struct ScanOperationSnapshot {
    /// Stable application operation identity.
    pub operation_id: ScanOperationId,
    /// Validated request accepted for execution.
    pub request: ScanRequest,
    /// Current or terminal lifecycle state.
    pub state: ScanOperationState,
    /// Active fractional progress in `[0.0, 1.0]`.
    ///
    /// Terminal operations never retain progress.
    pub progress: Option<f32>,
}

/// Aggregate library status used by polling adapters.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum LibraryScanStatus {
    /// No operation has been observed for this library.
    Idle,
    /// At least one accepted request is waiting.
    Queued,
    /// A request currently owns the shared library exclusion boundary.
    Running,
    /// The latest request completed and the library is idle.
    Completed,
    /// The latest request failed and the library is idle.
    Failed {
        /// Storage-neutral worker or adapter diagnostic.
        message: String,
    },
}

#[cfg(test)]
mod watermark_editor_tests {
    use async_trait::async_trait;
    use ersatzrs_core::repositories::{RepositoryError, Result as RepositoryResult};

    use super::*;

    struct WatermarkNavigationRepository {
        watermarks: Vec<ChannelWatermark>,
        fail_list: bool,
    }

    #[async_trait]
    impl WatermarkRepository for WatermarkNavigationRepository {
        async fn list_admin(&self) -> RepositoryResult<Vec<ChannelWatermark>> {
            if self.fail_list {
                return Err(RepositoryError::unavailable(
                    "list watermark editor navigation",
                    "database closed",
                ));
            }
            Ok(self.watermarks.clone())
        }

        async fn get_admin(&self, id: EntityId) -> RepositoryResult<Option<ChannelWatermark>> {
            Ok(self
                .watermarks
                .iter()
                .find(|watermark| watermark.id == id)
                .cloned())
        }

        async fn name_exists(&self, id: EntityId, name: &str) -> RepositoryResult<bool> {
            Ok(self
                .watermarks
                .iter()
                .any(|watermark| watermark.id != id && watermark.name == name))
        }

        async fn save_admin(&self, watermark: &ChannelWatermark) -> RepositoryResult<EntityId> {
            Ok(watermark.id)
        }

        async fn delete_admin(&self, _id: EntityId) -> RepositoryResult<()> {
            Ok(())
        }

        async fn copy_admin(&self, id: EntityId, _name: &str) -> RepositoryResult<EntityId> {
            Ok(id)
        }
    }

    fn watermark(id: EntityId, name: &str) -> ChannelWatermark {
        ChannelWatermark {
            id,
            name: name.to_owned(),
            mode: ChannelWatermarkMode::Permanent,
            image_source: ChannelWatermarkImageSource::Custom,
            image: Some(format!("{name}.png")),
            original_content_type: Some("image/png".to_owned()),
            location: WatermarkLocation::BottomRight,
            size: WatermarkSize::ActualSize,
            width_percent: 15.0,
            horizontal_margin_percent: 5.0,
            vertical_margin_percent: 5.0,
            frequency_minutes: 15,
            duration_seconds: 15,
            opacity: 100,
            place_within_source_content: false,
            opacity_expression: None,
            z_index: 0,
        }
    }

    #[tokio::test]
    async fn editor_navigation_preserves_repository_order_and_explicit_ends() {
        let repository = WatermarkNavigationRepository {
            watermarks: vec![
                watermark(10, "Alpha"),
                watermark(20, "Bravo"),
                watermark(30, "Charlie"),
            ],
            fail_list: false,
        };

        let first = get_watermark_editor(10, &repository)
            .await
            .expect("first editor query")
            .expect("first watermark");
        assert_eq!(first.previous, None);
        assert_eq!(
            first.next,
            Some(WatermarkNavigationRead {
                id: 20,
                name: "Bravo".to_owned(),
            })
        );

        let middle = get_watermark_editor(20, &repository)
            .await
            .expect("middle editor query")
            .expect("middle watermark");
        assert_eq!(
            middle.previous,
            Some(WatermarkNavigationRead {
                id: 10,
                name: "Alpha".to_owned(),
            })
        );
        assert_eq!(
            middle.next,
            Some(WatermarkNavigationRead {
                id: 30,
                name: "Charlie".to_owned(),
            })
        );

        let last = get_watermark_editor(30, &repository)
            .await
            .expect("last editor query")
            .expect("last watermark");
        assert_eq!(
            last.previous,
            Some(WatermarkNavigationRead {
                id: 20,
                name: "Bravo".to_owned(),
            })
        );
        assert_eq!(last.next, None);
    }

    #[tokio::test]
    async fn editor_navigation_distinguishes_absence_and_repository_failure() {
        let repository = WatermarkNavigationRepository {
            watermarks: vec![watermark(10, "Alpha")],
            fail_list: false,
        };
        assert_eq!(
            get_watermark_editor(99, &repository)
                .await
                .expect("valid absence"),
            None
        );

        let error = get_watermark_editor(
            10,
            &WatermarkNavigationRepository {
                watermarks: Vec::new(),
                fail_list: true,
            },
        )
        .await
        .expect_err("repository failure remains typed");
        assert!(matches!(
            error,
            ApplicationError::Unavailable {
                operation: "list watermark editor navigation",
                ..
            }
        ));
    }
}

/// Query one library's aggregate scan state.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct GetLibraryScanStatus {
    /// Persisted library identity.
    pub library_id: EntityId,
}

impl GetLibraryScanStatus {
    /// Create a library scan status query.
    #[must_use]
    pub const fn new(library_id: EntityId) -> Self {
        Self { library_id }
    }
}

#[cfg(test)]
mod block_preview_tests {
    use std::collections::BTreeMap;
    use std::sync::Mutex;

    use async_trait::async_trait;
    use chrono::{Duration, TimeZone};
    use ersatzrs_core::playout_builder::PlayoutBuilder;
    use ersatzrs_core::repositories::{RepositoryError, Result as RepositoryResult};
    use ersatzrs_core::scheduling::{
        BlockPreviewRepository, EffectiveBlockItem, SchedulingError, SchedulingMediaItem,
        SchedulingReferenceData, SchedulingResult,
    };
    use ersatzrs_domain::{BlockStopScheduling, CollectionType, PlaybackOrder, PlayoutItem};

    use super::*;

    #[derive(Default)]
    struct HydratingRepository {
        calls: Mutex<Vec<Vec<EffectiveBlockItem>>>,
        empty: bool,
        fail: bool,
    }

    #[async_trait]
    impl BlockPreviewRepository for HydratingRepository {
        async fn load_block_preview_content(
            &self,
            items: &[EffectiveBlockItem],
            _policies: &BTreeMap<String, SchedulingSmartShufflePolicy>,
        ) -> RepositoryResult<SchedulingReferenceData> {
            self.calls
                .lock()
                .expect("preview calls lock")
                .push(items.to_vec());
            if self.fail {
                return Err(RepositoryError::unavailable(
                    "load block preview content",
                    "database closed",
                ));
            }

            let mut reference_data = SchedulingReferenceData::default();
            if self.empty {
                return Ok(reference_data);
            }
            for item in items {
                let media_id = 1_000 + item.id;
                let media = SchedulingMediaItem {
                    title: Some(format!("Real media {media_id}")),
                    ..SchedulingMediaItem::new(media_id, Duration::minutes(1))
                };
                reference_data
                    .media_items_by_id
                    .insert(media_id, media.clone());
                reference_data
                    .media_by_key
                    .insert(item.content.clone(), vec![media]);
            }
            Ok(reference_data)
        }
    }

    #[derive(Default)]
    struct CapturingBuilder {
        requests: Mutex<Vec<SchedulingRequest>>,
        output_media_ids: Vec<EntityId>,
        fail: bool,
    }

    impl BlockPreviewBuilder for CapturingBuilder {
        fn build(
            &self,
            request: SchedulingRequest,
        ) -> std::result::Result<SchedulingResult, SchedulingError> {
            self.requests
                .lock()
                .expect("builder requests lock")
                .push(request.clone());
            if self.fail {
                return Err(SchedulingError::InvalidDuration(
                    "builder rejected draft".to_owned(),
                ));
            }
            let start = request.start;
            Ok(SchedulingResult {
                added_items: self
                    .output_media_ids
                    .iter()
                    .enumerate()
                    .map(|(index, media_item_id)| {
                        let row_start =
                            start + Duration::minutes(i64::try_from(index).unwrap_or(0));
                        PlayoutItem {
                            playout_id: request.playout.id,
                            media_item_id: *media_item_id,
                            start: row_start,
                            finish: row_start + Duration::minutes(1),
                            ..PlayoutItem::default()
                        }
                    })
                    .collect(),
                ..SchedulingResult::default()
            })
        }
    }

    fn preview_item(index: i32, collection_key: SchedulingCollectionKey) -> PreviewBlockItem {
        PreviewBlockItem {
            index,
            collection_key,
            playback_order: PlaybackOrder::Chronological,
            include_in_program_guide: true,
            disable_watermarks: false,
            watermark_ids: Vec::new(),
            graphics_element_ids: Vec::new(),
            smart_shuffle_policy: SchedulingSmartShufflePolicy::default(),
        }
    }

    fn query(items: Vec<PreviewBlockItem>) -> PreviewBlockPlayout {
        PreviewBlockPlayout {
            name: "Unsaved preview".to_owned(),
            minutes: 60,
            stop_scheduling: BlockStopScheduling::AfterDurationEnd,
            items,
        }
    }

    async fn run_preview(
        query: PreviewBlockPlayout,
        repository: &dyn BlockPreviewRepository,
        builder: &dyn BlockPreviewBuilder,
    ) -> Result<Vec<PreviewBlockPlayoutRow>> {
        preview_block_playout_at(
            query,
            repository,
            builder,
            Utc.with_ymd_and_hms(2026, 7, 24, 12, 0, 0)
                .single()
                .expect("fixed preview time"),
        )
        .await
    }

    fn collection_key(kind: CollectionType, id: EntityId) -> SchedulingCollectionKey {
        let mut key = SchedulingCollectionKey {
            collection_type: kind,
            ..SchedulingCollectionKey::default()
        };
        match kind {
            CollectionType::Collection => key.collection_id = Some(id),
            CollectionType::MultiCollection => key.multi_collection_id = Some(id),
            CollectionType::SmartCollection => key.smart_collection_id = Some(id),
            CollectionType::Playlist => key.playlist_id = Some(id),
            CollectionType::SearchQuery => key.search_query = Some(format!("title:{id}")),
            _ => key.media_item_id = Some(id),
        }
        key
    }

    #[tokio::test]
    async fn unsaved_draft_builds_temporary_state_and_uses_reset() {
        let repository = HydratingRepository::default();
        let builder = CapturingBuilder {
            output_media_ids: vec![1_001, 1_002],
            ..CapturingBuilder::default()
        };
        let items = vec![
            preview_item(20, collection_key(CollectionType::TelevisionShow, 8)),
            PreviewBlockItem {
                index: 10,
                collection_key: collection_key(CollectionType::Collection, 7),
                playback_order: PlaybackOrder::Shuffle,
                include_in_program_guide: false,
                disable_watermarks: true,
                watermark_ids: vec![3],
                graphics_element_ids: vec![4],
                smart_shuffle_policy: SchedulingSmartShufflePolicy::default(),
            },
        ];

        let rows = run_preview(query(items), &repository, &builder)
            .await
            .expect("preview succeeds");

        let requests = builder.requests.lock().expect("builder requests lock");
        let request = requests.first().expect("one builder request");
        assert_eq!(request.mode, PlayoutBuildMode::Reset);
        assert_eq!(
            request.start,
            Utc.with_ymd_and_hms(2026, 7, 24, 12, 0, 0)
                .single()
                .expect("fixed preview request time")
        );
        assert_eq!(request.playout.schedule_kind, PlayoutScheduleKind::Block);
        assert_eq!(request.reference_data.effective_blocks.len(), 1);
        let block = &request.reference_data.effective_blocks[0];
        assert_eq!(block.name, "Unsaved preview");
        assert_eq!(block.duration, Duration::minutes(60));
        assert_eq!(block.stop_scheduling, BlockStopScheduling::AfterDurationEnd);
        assert_eq!(
            block
                .items
                .iter()
                .map(|item| item.collection_key.collection_type)
                .collect::<Vec<_>>(),
            vec![CollectionType::Collection, CollectionType::TelevisionShow]
        );
        assert!(!block.items[0].include_in_program_guide);
        assert!(block.items[0].disable_watermarks);
        assert_eq!(block.items[0].watermark_ids, vec![3]);
        assert_eq!(block.items[0].graphics_element_ids, vec![4]);
        assert_eq!(
            rows.iter().map(|row| row.media_item_id).collect::<Vec<_>>(),
            vec![1_001, 1_002]
        );
        assert_eq!(rows[0].title, "Real media 1001");
        assert_eq!(rows[0].duration, Duration::minutes(1));
    }

    #[tokio::test]
    async fn every_real_block_collection_kind_uses_the_real_builder_path() {
        let kinds = [
            CollectionType::Collection,
            CollectionType::TelevisionShow,
            CollectionType::TelevisionSeason,
            CollectionType::Artist,
            CollectionType::MultiCollection,
            CollectionType::SmartCollection,
            CollectionType::Playlist,
            CollectionType::SearchQuery,
            CollectionType::Movie,
            CollectionType::Episode,
            CollectionType::MusicVideo,
            CollectionType::OtherVideo,
            CollectionType::Song,
            CollectionType::Image,
            CollectionType::RemoteStream,
        ];
        let items = kinds
            .iter()
            .enumerate()
            .map(|(index, kind)| {
                preview_item(
                    i32::try_from(index).unwrap_or_default(),
                    collection_key(*kind, i64::try_from(index).unwrap_or_default() + 1),
                )
            })
            .collect();
        let repository = HydratingRepository::default();

        let rows = run_preview(query(items), &repository, &PlayoutBuilder::default())
            .await
            .expect("real builder preview succeeds");

        let calls = repository.calls.lock().expect("preview calls lock");
        assert_eq!(
            calls[0]
                .iter()
                .map(|item| item.collection_key.collection_type)
                .collect::<Vec<_>>(),
            kinds
        );
        assert_eq!(rows.len(), kinds.len());
        assert_eq!(
            rows.iter().map(|row| row.media_item_id).collect::<Vec<_>>(),
            (1_001..=1_015).collect::<Vec<_>>()
        );
    }

    #[tokio::test]
    async fn valid_empty_output_is_not_a_failure() {
        let repository = HydratingRepository {
            empty: true,
            ..HydratingRepository::default()
        };

        let rows = run_preview(
            query(vec![preview_item(
                0,
                collection_key(CollectionType::Collection, 7),
            )]),
            &repository,
            &PlayoutBuilder::default(),
        )
        .await
        .expect("empty preview remains valid");

        assert!(rows.is_empty());
    }

    #[tokio::test]
    async fn repository_builder_and_enrichment_failures_remain_typed() {
        let item = preview_item(0, collection_key(CollectionType::Collection, 7));
        let repository_failure = run_preview(
            query(vec![item.clone()]),
            &HydratingRepository {
                fail: true,
                ..HydratingRepository::default()
            },
            &CapturingBuilder::default(),
        )
        .await
        .expect_err("repository failure propagates");
        assert!(matches!(
            repository_failure,
            ApplicationError::Unavailable {
                operation: "load block preview content",
                ..
            }
        ));

        let builder_failure = run_preview(
            query(vec![item.clone()]),
            &HydratingRepository::default(),
            &CapturingBuilder {
                fail: true,
                ..CapturingBuilder::default()
            },
        )
        .await
        .expect_err("builder failure propagates");
        assert!(matches!(
            builder_failure,
            ApplicationError::Scheduling {
                operation: "block preview",
                ..
            }
        ));

        let enrichment_failure = run_preview(
            query(vec![item]),
            &HydratingRepository::default(),
            &CapturingBuilder {
                output_media_ids: vec![9_999],
                ..CapturingBuilder::default()
            },
        )
        .await
        .expect_err("missing enrichment propagates");
        assert!(matches!(
            enrichment_failure,
            ApplicationError::MissingReference {
                kind: "block preview media item",
                id: 9_999
            }
        ));
    }
}
