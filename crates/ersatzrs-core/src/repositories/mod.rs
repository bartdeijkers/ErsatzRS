mod addon_media_list;
mod addon_media_list_editor;
mod addon_media_list_lifecycle;
mod addon_media_list_source_probe;
mod admin_media;
mod admin_search;
mod admin_settings;
mod artwork;
mod block;
mod channel;
mod collection;
mod config;
mod deco;
mod emby;
mod error;
mod ffmpeg_profile;
mod filler;
mod graphics;
mod jellyfin;
mod library;
mod managed_library_operation;
mod media_item;
mod media_source;
mod metadata;
mod playout;
mod program_schedule;
mod saved_search;
mod television;
mod template;
mod troubleshooting;
mod watermark;

pub use addon_media_list::{
    AddonMediaListAdminForm, AddonMediaListAdminRow, AddonMediaListAdminSortDirection,
    AddonMediaListAdminSortField, AddonMediaListArtwork, AddonMediaListArtworkRole,
    AddonMediaListArtworkUpload, AddonMediaListAutoPublicationHold, AddonMediaListImportAction,
    AddonMediaListImportProgress, AddonMediaListItemAdminRow, AddonMediaListItemCounts,
    AddonMediaListItemDetail, AddonMediaListItemReviewEdit, AddonMediaListMetadataConflict,
    AddonMediaListNfoForm, AddonMediaListNfoMetadata, AddonMediaListNfoOwnership,
    AddonMediaListNfoPerson, AddonMediaListPreparationFailure,
    AddonMediaListPreparationFailureReason, AddonMediaListPublication,
    AddonMediaListPublicationBlocker, AddonMediaListPublicationBlockerKind,
    AddonMediaListPublicationContinuation, AddonMediaListPublicationReadiness,
    AddonMediaListReplacementCandidate, AddonMediaListRepository,
};
pub use addon_media_list_editor::{
    AddonMediaListEditorItem, AddonMediaListEditorMetadata, AddonMediaListEditorSave,
    AddonMediaListEditorSettings,
};
pub use addon_media_list_lifecycle::{
    AddonMediaListArtifact, AddonMediaListDraftDiff, AddonMediaListDraftOptions,
    AddonMediaListImportCheckpoint, AddonMediaListImportLifecycle, AddonMediaListItemOptionValues,
    AddonMediaListItemTombstone, AddonMediaListLifecycleJob, AddonMediaListLifecycleRepository,
    AddonMediaListLifecycleSettings, AddonMediaListOptionItem, AddonMediaListOptionSchema,
    AddonMediaListOptionSchemaRef, AddonMediaListOptionValues, AddonMediaListPublishedOptions,
    AddonMediaListReferencePins, AddonMediaListResetImpact, AddonMediaListRevisionArtifacts,
    AddonMediaListRevisionDiff, AddonMediaListRevisionFile, AddonMediaListRevisionFileBlob,
    AddonMediaListRevisionItem, AddonMediaListRevisionRestoreFence, AddonMediaListRevisionSnapshot,
    AddonMediaListRevisionSummary, AddonProcessedArtifactPolicy, AddonToolMaintenanceAttempt,
    CreateAddonMediaListLifecycleJob, ADDON_LIST_DISK_WARNING_GB_BYTES,
    ADDON_LIST_TOTAL_DISK_WARNING_GB_DEFAULT, ADDON_LIST_TOTAL_DISK_WARNING_GB_KEY,
    PROCESSED_ARTIFACT_GLOBAL_BYTE_LIMIT_KEY, PROCESSED_ARTIFACT_MAX_CONCURRENCY_KEY,
    PROCESSED_ARTIFACT_MINIMUM_FREE_BYTES_KEY, PROCESSED_ARTIFACT_RETENTION_SECONDS_KEY,
};
pub use addon_media_list_source_probe::{
    AddonMediaListSourceProbe, AddonMediaListSourceProbeOutcome,
    AddonMediaListSourceProbeRepository, AddonMediaListSourceProbeState,
};
pub use admin_media::{
    AdminActorCard, AdminArtistDetail, AdminArtistMusicVideo, AdminAudioTrack,
    AdminCommentarySubtitle, AdminFacetedMediaDetail, AdminFacetedMediaKind, AdminMediaCard,
    AdminMediaInfoChapter, AdminMediaInfoKind, AdminMediaInfoStream, AdminMediaItemInfo,
    AdminMediaRepository, AdminMovieDetail, AdminMoviePlaybackSource, AdminMoviePlaybackStream,
    AdminOtherVideoDetail, AdminRemoteStreamChannel, AdminRemoteStreamDetail,
    AdminRemoteStreamOwner, AdminRemoteStreamPlaybackHealth, AdminRemoteStreamPlaybackHealthReason,
    AdminRemoteStreamPlaybackHealthStatus, AdminSeasonCard, AdminShowDetail,
};
pub use admin_search::{
    AdminSearchEnrichment, AdminSearchKind, AdminSearchRepository, AdminSearchSnapshot,
};
pub use admin_settings::{
    AdminSettingsRepository, SettingsMutation, SettingsOptionKind, SettingsSelectOption,
};
pub use artwork::ArtworkRepository;
pub use block::{
    BlockAdminRow, BlockEditorOptionAdmin, BlockEditorOptionsAdmin, BlockGroupAdminRow,
    BlockItemAdmin, BlockRepository,
};
pub use channel::{
    ChannelAdminListRow, ChannelEditorForm, ChannelEditorOptions, ChannelEditorState,
    ChannelLogoForm, ChannelMirrorOption, ChannelNumberRow, ChannelNumberSaveResult,
    ChannelNumberUpdate, ChannelRepository, ChannelSaveResult, ChannelSaveSideEffects,
    ChannelSelectOption, ChannelValidationError, ChannelValidationRow, ChannelValidationState,
    ChannelWatchProgramme, ChannelWatchState, EpgPreviewChannelState,
};
pub use collection::{
    CollectionAdminRow, CollectionDetailAdmin, CollectionItemCardAdmin, CollectionMemberAdmin,
    CollectionRepository, MediaItemSuggestionAdmin, MultiCollectionAdmin, MultiCollectionAdminRow,
    MultiCollectionEditorOptions, MultiCollectionItem, MultiCollectionSelectOption,
    PlaybackOverrides, PlaylistAdminRow, PlaylistEditorAdmin, PlaylistEditorOptions,
    PlaylistGroupAdminRow, PlaylistItemAdmin, PlaylistItemInsert, PlaylistItemOption,
    RerunCollectionAdmin, RerunCollectionAdminRow, RerunCollectionEditorOptions,
    RerunCollectionReferenceKind, RerunCollectionSelectOption, SmartCollectionAdminRow,
};
pub use config::ConfigRepository;
pub use deco::{
    DecoAdminRecord, DecoAdminRow, DecoBreakContentAdmin, DecoEditorOptionsAdmin,
    DecoGroupAdminRow, DecoOptionAdmin, DecoPlaylistOptionAdmin, DecoRepository,
    DecoTemplateAdminRow, DecoTemplateGroupAdminRow, DecoTemplateItemAdmin,
};
pub use emby::{
    EmbyActor, EmbyCollectionItem, EmbyCollectionMediaItem, EmbyCollectionMediaKind,
    EmbyCollectionMediaPage, EmbyCollectionPage, EmbyConnectionConfig, EmbyItemPage,
    EmbyLibrarySummary, EmbyMediaItem, EmbyPathInfo, EmbyPlaybackMetadata, EmbySeasonItem,
    EmbySeasonPage, EmbyService, EmbyShowItem, EmbyShowPage,
};
pub use error::{RepositoryError, Result};
pub use ersatzrs_domain::AddonMediaListAutoPublicationHoldReason;
pub use ffmpeg_profile::FfmpegProfileRepository;
pub use filler::FillerRepository;
pub use graphics::GraphicsRepository;
pub use jellyfin::{
    JellyfinActor, JellyfinCollectionItem, JellyfinCollectionMediaItem,
    JellyfinCollectionMediaKind, JellyfinCollectionMediaPage, JellyfinCollectionPage,
    JellyfinConnectionConfig, JellyfinItemPage, JellyfinLibrarySummary, JellyfinMediaItem,
    JellyfinPathInfo, JellyfinPlaybackMetadata, JellyfinSeasonItem, JellyfinSeasonPage,
    JellyfinService, JellyfinShowItem, JellyfinShowPage,
};
pub use library::{
    AdminExternalCollectionRow, AdminLibraryRow, LibraryRepository, LibraryScanTarget,
    MediaStateRevision,
};
pub use managed_library_operation::{
    AddonMediaListPublicationEnqueue, AddonMediaListPublicationEvent,
    AddonMediaListPublicationFailureCause, AddonMediaListPublicationHistory,
    AddonMediaListPublicationHistoryJob, AddonMediaListPublicationJob,
    AddonMediaListPublicationStatus, AddonMediaListReconcileEnqueue, AddonMediaListResetEnqueue,
    ManagedLibraryOperation, ManagedLibraryOperationClaim, ManagedLibraryOperationRepository,
    ManagedLibraryScanSpec,
};
pub use media_item::MediaItemRepository;
pub use media_source::{
    LibraryPreferenceAdmin, LocalLibraryAdmin, LocalLibraryAdminRow, LocalLibraryPathAdmin,
    ManagedAddonMediaSourceAdminRow, MediaSourceRepository, PathReplacementAdmin,
    PlexArtworkConnectionAdmin, RemoteConnectionAdmin, RemoteMediaSourceKind, RemoteSourceAdminRow,
};
pub use metadata::MetadataRepository;
pub use playout::{
    PlayoutAlternateScheduleItemAdmin, PlayoutAlternateSchedulesAdmin, PlayoutCreateAdmin,
    PlayoutCreateChannelAdmin, PlayoutCreateScheduleAdmin, PlayoutDateFilterAdmin,
    PlayoutDecoAdmin, PlayoutDecoGroupAdmin, PlayoutDetailAdminItem, PlayoutKindEditorAdmin,
    PlayoutListAdminRow, PlayoutNestedChildOptionAdmin, PlayoutNestedOptionAdmin,
    PlayoutRepository, PlayoutTemplateItemAdmin, PlayoutTemplatesAdmin, SequentialPlayoutAdmin,
};
pub use program_schedule::{
    ProgramScheduleRepository, ScheduleAdminForm, ScheduleAdminRow, ScheduleFillerOption,
    ScheduleItemAdmin, ScheduleItemOption, ScheduleItemOptions, ScheduleLanguageOption,
    SchedulePlaylistOption,
};
pub use saved_search::{SavedSearchAdminRow, SavedSearchRepository};
pub use television::{EpisodeCard, EpisodeMediaItemInfo, SeasonDetail, TelevisionRepository};
pub use template::{
    TemplateAdminRow, TemplateBlockAdmin, TemplateBlockGroupAdmin, TemplateGroupAdminRow,
    TemplateItemAdmin, TemplateRepository,
};
pub use troubleshooting::{
    BlockPlayoutHistoryAdmin, BlockPlayoutOptionAdmin, MpegTroubleshootingChannelAdmin,
    PlaybackChannelSummaryAdmin, PlaybackMediaSummaryAdmin, PlaybackSubtitleAdmin,
    PlaybackTroubleshootingAdmin, TroubleshootingBlockAdmin, TroubleshootingChannelSummaryAdmin,
    TroubleshootingFfmpegProfileSummaryAdmin, TroubleshootingOptionAdmin,
    TroubleshootingPreviewMediaAdmin, TroubleshootingRepository,
};
pub use watermark::WatermarkRepository;
