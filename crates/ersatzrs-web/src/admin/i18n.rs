use ersatzrs_addon_contract::{AddonOperationError, ADDON_OPERATION_ERROR_MARKER};
use leptos::prelude::*;

use crate::admin::server_fns::settings::SelectOption;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct UiLanguage {
    code: &'static str,
}

pub struct LanguageDefinition {
    pub code: &'static str,
    pub label: &'static str,
    pub aliases: &'static [&'static str],
    pub strings: &'static dyn LanguageStrings,
}

impl UiLanguage {
    #[allow(non_upper_case_globals)]
    pub const English: Self = Self { code: "en-us" };
    #[allow(non_upper_case_globals)]
    pub const Polish: Self = Self { code: "pl" };
    #[allow(non_upper_case_globals)]
    pub const PortugueseBrazil: Self = Self { code: "pt-br" };
    #[allow(non_upper_case_globals)]
    pub const Dutch: Self = Self { code: "nl" };
    #[allow(non_upper_case_globals)]
    pub const French: Self = Self { code: "fr-fr" };

    pub const fn code(self) -> &'static str {
        self.code
    }

    pub fn label(self) -> &'static str {
        language_definition_for(self).label
    }

    pub fn from_config(value: &str) -> Self {
        let normalized = normalize_language_input(value);
        crate::admin::languages::LANGUAGES
            .iter()
            .find(|language| language.matches(&normalized))
            .map_or(Self::English, |language| Self {
                code: language.code,
            })
    }
}

impl LanguageDefinition {
    fn matches(&self, value: &str) -> bool {
        value == self.code || self.aliases.contains(&value)
    }
}

pub fn normalize_ui_language(value: &str) -> String {
    UiLanguage::from_config(value).code().to_owned()
}

pub fn supported_ui_language_options() -> Vec<SelectOption> {
    crate::admin::languages::LANGUAGES
        .iter()
        .map(|language| SelectOption {
            value: language.code.to_owned(),
            label: language.label.to_owned(),
        })
        .collect()
}

fn normalize_language_input(value: &str) -> String {
    value.trim().to_ascii_lowercase()
}

#[derive(Clone, Copy, Debug, Hash, PartialEq, Eq)]
pub enum TextKey {
    ItemDetailsNotRequested,
    ItemDetailsFailed,
    ItemDetailsRetry,
    ItemProviderPlayback,

    ItemPosition,
    ItemPrevious,
    ItemNext,
    ItemBack,
    ItemTools,
    ItemEpisodePosition,
    ItemNumbered,
    ItemUnnumberedHelp,
    ItemNumberingHelp,
    ItemMetadata,
    ItemMetadataHelp,
    ItemSaveHelp,
    ItemEditNumbering,
    ItemReplacementHelp,
    ItemReplacementPending,
    ItemSearchReplacement,
    ItemRemoveReplacement,
    ItemInformation,
    ItemInformationHelp,
    ItemSavedToolsHelp,
    ItemViewMedia,
    ItemSavedValue,
    ItemFromList,
    ItemUseListSetting,
    ItemOverride,
    ItemPackageDefault,
    ItemPreviewOptions,
    ItemAccessActive,
    OpenItemInNewTab,
    BulkItemsToChange,
    BulkScopeHelp,
    BulkDetectSeasons,
    BulkFirstSeason,
    BulkReviewChanges,
    BulkPreviewSaves,
    BulkPreviewNothing,
    BulkApplyNumbering,
    BulkApplyChanges,
    BulkCancelPreview,
    ActiveFilter,

    AddonWorkspaceLengthFilter,
    AddonWorkspaceMinimumLength,
    AddonWorkspaceMaximumLength,
    AddonWorkspaceUnknownLength,
    AddonWorkspaceLengthHelp,
    AddonWorkspaceReviewLatest,
    AddonWorkspaceReviewRetained,
    AddonWorkspaceStreamHelp,
    AddonWorkspaceDownloadHelp,
    AddonWorkspaceNewHelp,
    AddonWorkspaceNoOptions,
    AddonWorkspaceSizeLimit,
    AddonWorkspaceItems,
    AddonWorkspaceSettings,
    AddonWorkspacePlayback,
    AddonWorkspaceActivity,
    AddonWorkspaceSections,
    AddonWorkspaceSaveSettings,
    AddonWorkspaceSaveItem,
    AddonWorkspaceSaveNumbering,
    AddonWorkspacePublish,
    AddonWorkspacePublished,
    AddonWorkspacePending,
    AddonWorkspacePreparationRequired,
    AddonWorkspacePreparing,
    AddonWorkspaceAddIncomplete,
    AddonWorkspacePreparationFailed,
    AddonWorkspaceIncludeSelected,
    AddonWorkspaceExcludeSelected,
    AddonWorkspaceInclusionHelp,
    AddonWorkspacePublicationFailed,
    AddonWorkspacePublicationRestoredHelp,
    AddonWorkspaceParentMetadataPreserved,
    AddonWorkspaceImporting,
    AddonWorkspaceImportingHelp,
    AddonWorkspaceImportSteps,
    AddonWorkspaceChooseProvider,
    AddonWorkspaceExcluded,
    AddonWorkspaceSort,
    AddonWorkspaceAscending,
    AddonWorkspaceDescending,
    AddonWorkspaceItemDate,
    AddonWorkspaceSelectVisible,
    AddonWorkspaceDeselectVisible,
    AddonWorkspaceSelectFirst,
    AddonsNoneInstalled,
    AddonsNoneAvailable,
    AddonsSetupHelp,
    AddonPackageHelp,
    AddonRepositoryPackageHelp,
    AddonPackageUploadHelp,
    AddonRepositoryUploadHelp,
    AddonConfigurationSavedRecheck,
    AddonCheckRequired,
    AddonCheckPending,
    RetryPreparation,
    PreparationFailedItems,
    PreparationFailureFailed,
    PreparationFailureInputChanged,
    PreparationFailureInvalidRequest,
    PreparationFailureCancelled,
    PreparationFailureAttemptsExhausted,
    PreparationFailureStorageFull,
    PreparationFailureUnsupported,
    PreviousSavedRevision,
    AddonWorkspacePreparationActivity,
    AddonWorkspaceBulk,
    AddonWorkspaceSelection,
    AddonWorkspaceOffPage,
    AddonWorkspaceDiscard,
    AddonWorkspaceSelected,
    AddonWorkspaceMatching,
    AddonWorkspacePage,
    AddonWorkspaceAccess,
    AddonWorkspaceRetained,
    PreparationTasks,
    PreparationAffectedItems,
    PreparationIntervals,
    PreparationDownloads,
    PreparationSubtitles,
    PreparationQueued,
    PreparationRunning,
    PreparationWaiting,
    PreparationNotQueued,
    LastActivity,
    AddonAccessUnknownFailure,

    TechnicalDetails,
    AddonPermissionNetworkHelp,
    AddonPermissionMediaHelp,
    AddonPermissionFfmpegHelp,
    AddonPermissionCommandHelp,

    AddonWorkspaceSaveConflict,
    AddonWorkspaceSaveInvalid,
    AddonWorkspaceSaveStorage,
    AddonWorkspaceLoadIncomplete,

    AddonWorkspaceNavigation,
    AddonWorkspaceSeasonOnly,
    AddonWorkspaceSeasonPerYear,
    AddonWorkspaceYearDatesIncomplete,
    AddonWorkspacePageSize,

    AddonAccessTest,
    AddonAccessHelp,
    AddonAccessSaveFirst,
    AddonAccessError,
    AddonAccessNotChecked,
    AddonAccessQueued,
    AddonAccessRunning,
    AddonAccessWaiting,
    AddonAccessCancelled,
    AddonAccessStale,
    AddonAccessAccessible,
    AddonAccessProfileUnavailable,
    AddonAccessProfileLocked,
    AddonAccessDecryptionFailed,
    AddonAccessRestricted,
    AddonLocalPlaybackAvailable,
    AddonDerivedMetadataLimited,
    AddonAccessNextAttempt,
    AddonAccessExpires,
    AddonAccessRefresh,
    AddonOptionsCurrentFilter,
    AddonOptions,
    AddonOptionsDraftHelp,
    AddonOptionsProvisional,
    AddonOptionsListDefault,
    AddonOptionsInherited,
    AddonOptionsOverride,
    AddonOptionsFiniteOnly,
    AddonOptionsResetPackage,
    AddonOptionsResetList,
    AddonOptionsMixed,
    AddonOptionsPreviewHelp,
    AddonOptionsBefore,
    AddonOptionsAfter,
    AddonOptionsConflict,
    AddonOptionsNone,
    Addon,
    PrimaryFeature,
    MovieExtra,
    NeedReview,
    ReviewRecommended,
    PublicationReadiness,
    PublicationBlockers,
    PublicationActivity,
    PublicationJobs,
    PublicationEvents,
    NoPublicationActivity,
    CopyRedactedDiagnostics,
    DownloadRedactedDiagnostics,
    RedactedDiagnosticsCopied,
    PublicationGeneration,
    CorrelationId,
    TargetPublishedRevision,
    RelatedOperation,
    FilesystemTransition,
    RetryClass,
    RecoveryAction,
    OperatingSystemErrorCode,
    PublicationOutcome,
    PublicationExpired,
    EventExistingJobAdopted,
    EventEnqueued,
    EventLeaseWaitStarted,
    EventLeaseAcquired,
    EventLeaseReclaimed,
    EventPhaseAdvanced,
    EventChildScanStarted,
    EventChildScanCompleted,
    EventChildScanFailed,
    EventOriginalFailure,
    EventRecoveryFailure,
    EventRetryScheduled,
    EventRecoveryRequested,
    EventCompleted,
    EventHistoryTruncated,
    TriggerAutomaticRefresh,
    TriggerOperator,
    TriggerRestoreRevision,
    TestSource,
    TestSourceHelp,
    SourceTestQueued,
    SourceTestRunning,
    SourceTestStale,
    SourceAvailable,
    SourceUnsupported,
    SourceTestFailed,
    SourceRevision,
    FirstPageComplete,
    ProviderIdentity,
    ReturnedFirstPageItems,
    ProviderTotalItemCount,
    DraftChanges,
    ExactDraftDiffHelp,
    DraftDiffUnavailable,
    MetadataChanges,
    ExcludedItems,
    Conflicts,
    PendingRemovals,
    PendingRemovalHelp,
    Reappearances,
    RemovalCandidates,
    ConfirmRemoval,
    RemovalConfirmed,
    RemovalConfirmationFailed,
    RevisionHistory,
    RevisionHistoryHelp,
    NoRevisionHistory,
    Revision,
    CompareRevisions,
    RestoreRevision,
    RestoreRevisionWarning,
    RestoreRevisionQueued,
    RestoreRevisionFailed,
    PublishedItems,
    GeneratedPlaylists,
    ActiveOrFuturePlayouts,
    EffectiveOptions,
    Files,
    Artifacts,
    Added,
    Removed,
    Changed,
    FilesystemPreviousBackedUp,
    FilesystemCandidateCommitted,
    FilesystemPreviousRestored,
    FilesystemProjectionRestored,
    FilesystemRevisionVerified,
    FilesystemCleanupCompleted,
    RetryTransientRecovery,
    ErrorFilesystemIo,
    ErrorPermissionDenied,
    ErrorSharingViolation,
    ErrorUnsafeFilesystemState,
    ErrorInvalidPublicationMarker,
    ErrorPublicationIntegrity,
    OutcomePublished,
    OutcomeRestoredAfterFailure,
    ReviewRecommendations,
    OptionalDetails,
    DraftSaved,
    SavedRevision,
    PublishList,
    PreparePublication,
    ProcessedMediaRequired,
    ProcessedMediaQueued,
    PreparingProcessedMedia,
    PreparingPublication,
    Publishing,
    PublicationQueued,
    RestoringPublication,
    Published,
    PublishSavedRevisionHelp,
    ChangesSavedImmediately,
    ReviewItem,
    ShowFullDescription,
    ShowLess,
    ImportAList,
    ImportOverview,
    AddonContextSelected,
    CompletedIn,
    NoPublicationBlockers,
    BlockerOverviewIncomplete,
    BlockerPublicationInProgress,
    BlockerRecoveryRequired,
    BlockerCollectionTitleMissing,
    BlockerInvalidArtwork,
    BlockerNoIncludedItems,
    BlockerMoviePrimaryCount,
    BlockerPlaybackDecisionRequired,
    BlockerPlaybackDetailsUnavailable,
    BlockerPlaybackDetailsFailed,
    BlockerUnavailableWithoutReplacement,
    BlockerItemTitleMissing,
    BlockerSeasonEpisodeMissing,
    BlockerArtistMissing,
    PublishUnsavedChanges,
    PublicationBlocked,
    PublishBlocked,
    RetryRecovery,
    RestorePreviousRevision,
    PublicationPhase,
    PublicationAttempts,
    LastProgress,
    FailureReason,
    RestrictedItemsExcluded,
    RecoveryQueued,
    RecoveryFailed,
    PhaseAwaitingLease,
    PhaseStaging,
    PhaseScanning,
    PhaseCommitting,
    PhaseRestoringFiles,
    PhaseRestoringProjection,
    PhaseReconciling,
    PhaseVerifying,
    PhaseCleaningUp,
    PhaseComplete,
    InstallLeavesDisabled,
    EnableActivatesPermissions,
    FirstAired,
    LoadItemDetailsAfterPublishing,
    LoadItemDetailsHelp,
    OverviewOnlyManualDetailsHelp,
    PlaybackDetailsQueued,
    PublishAgainAfterDetails,
    FetchDetails,
    DetailsQueued,
    DetailsLoading,
    RetryDetails,
    DetailsComplete,
    DetailsSkipped,
    FindingProgrammes,
    CheckingOverview,
    ReadyToReview,
    LoadingItemDetails,
    WaitingForProvider,
    OverviewOnly,
    PauseImport,
    ResumeImport,
    RetryImportNow,
    EmptyImportNeedsRetry,
    EmptyImportRetryHelp,
    CancelImport,
    OtherPageItemsAffected,
    ElapsedTime,
    LastSuccessfulProgress,
    EstimatedRemainingTime,
    PlaybackBehavior,
    UseProviderValue,
    FiniteMedia,
    LiveSource,
    ProviderValueChanged,
    ApplyToSelected,
    ApplyToAllRemoteStreams,
    SelectItem,
    NumberEpisodes,
    NumberEpisodesHelp,
    FirstEpisode,
    NumberToAssign,
    AllRemaining,
    ReplaceExistingEpisodeNumbers,
    AutoDetectSeasons,
    AutoDetectSeasonsHelp,
    MinimumSeasonGapDays,
    NumberingStartsAt,
    FirstIncludedItem,
    StartWithFirstItem,
    StartHere,
    SaveSeasonEpisode,
    BulkEdit,
    Scope,
    AllItems,
    CurrentPage,
    ItemsNeedingReview,
    KeepCurrentValue,
    UndoLastChange,
    Undo,
    ChangeUndone,
    MetadataConflicts,
    MetadataConflictsHelp,
    PreviousProviderValue,
    NewProviderValue,
    OperatorValue,
    Discovered,
    Examined,
    ArchivedSkipped,
    Emitted,
    EditMetadata,
    OneTagPerLine,
    OriginalTitle,
    ReleaseDate,
    Year,
    SortTitle,
    MovieFilesAndExtras,
    MovieFilesAndExtrasHelp,
    EpisodesMetadataHelp,
    MusicVideosMetadataHelp,
    OtherVideosMetadataHelp,
    SongsMetadataHelp,
    ImagesMetadataHelp,
    RemoteStreamsMetadataHelp,
    CastAndCredits,
    OneCreditPerLine,
    Poster,
    Artwork,
    PublishMetadata,
    IncludeItem,
    IncludeInPublication,
    MetadataDraftNotice,
    ApplyToAllItemsPrompt,
    TelevisionEpisode,
    CollectionTypeAuto,
    CurrentLibraryType,
    CollectionTypeChangeHelp,
    NonPlayableItems,
    UnknownItems,
    PlayableItems,
    ApplyToAllItems,
    ChangeAddon,
    ChangeAddonHelp,
    ChangeAddonPrompt,
    AddonAutoDetect,
    AddonAutoDetected,
    AddonMediaListNoAddonForUrl,
    DefaultRemoteStreamAddon,
    DefaultRemoteStreamAddonHelp,
    AddonDetailWait,
    MinimumDetailWait,
    MaximumDetailWait,
    AddonDetailWaitHelp,
    AddonDetailWaitInvalid,
    Addons,
    AddonOverview,
    AddonUpdates,
    AddonSources,
    AddonNotFound,
    BackToAddons,
    AvailableVersion,
    BrowseAddons,
    CheckForUpdates,
    InstallFromFile,
    InstallRepositoryFromFile,
    License,
    UnknownSources,
    UnknownSourcesHelp,
    Verified,
    Unverified,
    Sideloaded,
    GitCheckout,
    OfflineBundle,
    LegacyUnmanaged,
    LegacyAdoptionHelp,
    Enabled,
    Disabled,
    Source,
    ViewSourceCode,
    Version,
    NextVersion,
    NoAddonsMatch,
    AddonsUpToDate,
    AddonRepositories,
    AvailableAddons,
    InstalledAddons,
    AddRepository,
    RepositoryUrl,
    PublicSigningKey,
    DeploymentManagedRepositories,
    LastRefresh,
    NeverRefreshed,
    ChooseFile,
    NoFileSelected,
    EditRepository,
    OfficialRepositoryProtected,
    UnknownSourcesRequired,
    NoRepositories,
    AddonPackage,
    RepositoryPackage,
    NoSigningKeyPinned,
    Refresh,
    Install,
    UpdateAddon,
    Uninstall,
    Configuration,
    SecretReference,
    EnvironmentVariable,
    SecretFile,
    Incompatible,
    PackageUnavailable,
    Official,
    ThirdParty,
    Permissions,
    Capabilities,
    Dependencies,
    Optional,
    MediaReadAccess,
    ExternalCommands,
    ThirdPartyRepositoryWarning,
    AddCustomResolution,
    AddBlock,
    AddBlockGroup,
    AddBlockItem,
    AddBlockToTemplate,
    AddCollection,
    AddContent,
    AddFailed,
    AddFillerPreset,
    AddMultiCollection,
    AddPlaylist,
    AddPlaylistGroup,
    AddPlaylistItem,
    AddRerunCollection,
    AddTemplate,
    AddTemplateGroup,
    AddAddonMediaList,
    AddToCollection,
    AddToEllipsis,
    AddToPlaylist,
    AddToSchedule,
    Actions,
    All,
    AreYouSureDelete,
    Actors,
    AlbumArtist,
    Apply,
    Artist,
    ArtistNotFound,
    AfterDurationEnd,
    AutomaticRefresh,
    AddonDeliveryMode,
    AddonDeliveryStream,
    AddonDeliveryDownloadAhead,
    AddonDeliveryNewVideos,
    AddonDeliveryHelp,
    AddonDeliveryUnavailable,
    AddonDeliveryCutoff,
    AddonDeliveryFreshCutoff,
    AdaptiveRefreshUnavailable,
    RefreshPublicationPolicy,
    RefreshPublicationReview,
    RefreshPublicationAutomatic,
    RefreshPublicationPolicyHelp,
    AutoPublicationHeld,
    PreparationProcessRecoveryRequired,
    PreparationWorkerUnavailable,
    AutoPublicationBaselineUnavailable,
    AutoPublicationExistingChanges,
    AutoPublicationConflicts,
    AutoPublicationRemovals,
    AutoPublicationRestricted,
    AutoPublicationUnavailable,
    AutoPublicationOptionsChanged,
    AutoPublicationPreparationFailed,
    AutoPublicationConcurrentChange,
    RefreshCadenceAdaptive,
    RefreshCadenceDaily,
    RefreshCadenceWeekly,
    RefreshCadenceMonthly,
    RefreshCadenceYearly,
    RefreshCadenceNever,
    LastSuccessfulRefresh,
    NextScheduledRefresh,
    RefreshRetryAt,
    Balanced,
    Back,
    BackToSettings,
    BackToArtists,
    BackToCollections,
    BackToMovies,
    BackToOtherVideos,
    BackToTvShows,
    BeforeDurationEnd,
    BackToTemplates,
    Badge,
    Block,
    BlockGroup,
    BlockCannotBeMoved,
    BlockCopied,
    BlockCreated,
    BlockDeleted,
    BlockGroupCreated,
    BlockGroupDeleted,
    BlockGroupName,
    BlockGroups,
    BlockItem,
    BlockItems,
    BlockName,
    BlockNotFound,
    BlockPreviewFailed,
    Blocks,
    SmartShuffleAfterDurationMinutes,
    SmartShuffleAfterItems,
    SmartShuffleFilterRules,
    SmartShuffleIncludeFiller,
    SmartShuffleInheritedBlock,
    SmartShuffleInactive,
    SmartShuffleOverrideBlock,
    SmartShufflePreset,
    SmartShuffleScope,
    SmartShuffleScopeContentSource,
    SmartShuffleScopePlayout,
    SmartShuffleScopeScheduleItem,
    SmartShuffleValidation,
    ContentParent,
    BlockToRemove,
    BlockWillNotFit,
    Bitrate,
    Cancel,
    Clear,
    Available,
    UnknownAvailability,
    LocalReplacement,
    NoReplacement,
    InvalidReplacement,
    Channels,
    Close,
    Collections,
    CollectionDeleted,
    CollectionSaved,
    SaveCollection,
    ComponentGallery,
    ComputingPreview,
    Confirm,
    ConfirmDelete,
    CommentaryTracks,
    Content,
    ContentVariantAcoustic,
    ContentVariantAlternateVersion,
    ContentVariantBehindTheScenes,
    ContentVariantDeletedScene,
    ContentVariantDirectorsCut,
    ContentVariantExtended,
    ContentVariantExtra,
    ContentVariantFeaturette,
    ContentVariantInterview,
    ContentVariantLive,
    ContentVariantOfficialVideo,
    ContentVariantOther,
    ContentVariantRemaster,
    ContentVariantRemix,
    ContentVariantScene,
    ContentVariantSearchHelp,
    ContentVariantShort,
    ContentVariantSpecialFeatures,
    ContentVariantTrailer,
    ContentVariantUnplugged,
    ContentVariantVersions,
    ContentRatings,
    CustomResolutionHeight,
    CustomResolutionWidth,
    CustomResolutions,
    Dark,
    DaysToBuild,
    AddBreakContent,
    AddDeco,
    AddDecoGroup,
    AddDecoTemplate,
    AddDecoTemplateGroup,
    AtChapterMarkers,
    BackToDecos,
    BackToDecoTemplates,
    BetweenBlockItems,
    BlockFinish,
    BlockStart,
    BreakContent,
    BreakContentBody,
    BreakContentItem,
    BreakContentMode,
    DeadAirFallback,
    DeadAirFallbackBody,
    DeadAirFallbackCollectionType,
    DeadAirFallbackMode,
    Deco,
    DecoCannotBeAdded,
    DecoCannotBeMoved,
    DecoCreated,
    DecoDeleted,
    DecoGroup,
    DecoGroupCreated,
    DecoGroupDeleted,
    DecoGroupName,
    DecoGroups,
    DecoName,
    DecoNotFound,
    DecoSaved,
    Decos,
    DecoTemplate,
    DecoTemplateCreated,
    DecoTemplateDeleted,
    DecoTemplateGroup,
    DecoTemplateGroupCreated,
    DecoTemplateGroupDeleted,
    DecoTemplateGroupName,
    DecoTemplateGroups,
    DecoTemplateName,
    DecoTemplateNotFound,
    DecoTemplateSaved,
    AddDecoToTemplate,
    DecoToRemove,
    DecoTemplates,
    DefaultFiller,
    DefaultFillerBody,
    DefaultFillerCollectionType,
    DefaultFillerMode,
    DefaultFfmpegProfile,
    DefaultFfmpegProfileHelp,
    DefaultMinimumLogLevel,
    DefaultMpegTsScript,
    DefaultMpegTsScriptHelp,
    Decorations,
    Debug,
    Delete,
    DeleteAll,
    DeleteBlock,
    DeleteBlockGroup,
    DeleteCustomResolution,
    DeleteCollection,
    DeleteDeco,
    DeleteDecoGroup,
    DeleteDecoTemplate,
    DeleteDecoTemplateGroup,
    DeleteFillerPreset,
    DeleteMultiCollection,
    DeletePlaylist,
    DeletePlaylistGroup,
    DeleteRerunCollection,
    DeleteTemplate,
    DeleteTemplateGroup,
    DeleteAddonMediaList,
    Disable,
    DisabledCollectionsAlreadyPresent,
    EmptyTrash,
    ExtractEmbeddedSubtitles,
    ErasedPlayoutItemsAndHistory,
    EraseFailed,
    EraseItemsAndHistory,
    ErasePlayoutItemsAndHistoryConfirm,
    Episode,
    FanArt,
    Failed,
    Ffmpeg,
    FileNotFound,
    Finish,
    Fatal,
    FfmpegPath,
    FfmpegPathHelp,
    FfmpegProfiles,
    FfprobePath,
    FfprobePathHelp,
    FillerPreset,
    FillerPresetActionFailed,
    FillerPresetCouldNotBeSaved,
    FillerPresetDeleted,
    FillerPresetExpressionHelp,
    FillerPresetSaved,
    FillerPresets,
    Flags,
    GraphicsElementsMode,
    GlobalFallbackFiller,
    GlobalWatermark,
    GoHome,
    HdHomeRun,
    HealthChecks,
    Height,
    Information,
    HlsDirectOutputFormat,
    HlsSegmenterIdleTimeout,
    HlsSegmenterIdleTimeoutHelp,
    HlsSegmenterInitialSegmentCount,
    HlsSegmenterInitialSegmentCountHelp,
    HomeRunning,
    ReleaseNotes,
    FullChangelog,
    HttpMinimumLogLevel,
    Id,
    Images,
    Item,
    ItemActions,
    ItemType,
    Items,
    Language,
    Languages,
    MusicLabels,
    LeaveEmptyToAutoDetect,
    Libraries,
    LibraryRefreshInterval,
    Light,
    Lists,
    Loading,
    LoadingMembers,
    Local,
    Logging,
    M3u,
    Media,
    MediaInfo,
    MediaSources,
    MpegTsTroubleshooting,
    Movie,
    MovieNotFound,
    Movies,
    MusicVideo,
    MultiCollections,
    ManualCollections,
    MatchFailed,
    MatchStatus,
    MatchAddonMediaListItems,
    Members,
    Merge,
    Music,
    Name,
    NameRequired,
    Networks,
    NewCollection,
    NewCollectionTitle,
    NewFillerPreset,
    NewMultiCollection,
    NewTemplateGroup,
    NewTemplateName,
    NextPage,
    Next24Hours,
    NoFillerPresets,
    NoBlockItems,
    NoItems,
    NoItemsToDisplay,
    No,
    NoCollections,
    NoDecosFound,
    NoDecoTemplatesFound,
    NoMembers,
    NoMultiCollections,
    NoTemplatesFound,
    NotFound,
    OtherVideo,
    OtherVideoNotFound,
    OtherVideos,
    OpenBrowser,
    Playout,
    PlayoutItemsAndHistory,
    PlayoutNotFound,
    Playouts,
    PlayAll,
    Playlist,
    PlaylistAdded,
    PlaylistDeleted,
    PlaylistGroup,
    PlaylistGroupAdded,
    PlaylistGroupDeleted,
    PlaylistGroupName,
    PlaylistGroups,
    PlaylistItem,
    PlaylistItems,
    PlaylistName,
    PlaylistNotFound,
    PlaylistPlayoutPreview,
    PlaylistPreviewEmpty,
    Playlists,
    ProbeForInterlacedFrames,
    PreferredAudioLanguage,
    DefaultSubtitleLanguage,
    FallbackSubtitleLanguage,
    OriginalLanguageOption,
    UndefinedLanguageOption,
    Previous24Hours,
    PreviousPage,
    RerunCollections,
    RerunCollection,
    RerunCollectionActionFailed,
    RerunCollectionDeleted,
    RerunCollectionNameRequired,
    RerunCollectionSaved,
    RerunPlaybackOrder,
    RemoteStreams,
    RemoteStream,
    PublishedOptions,
    PlaybackHealth,
    Configure,
    Watch,
    RemoteStreamPlaylists,
    PlaylistUrl,
    Extractor,
    DestinationFolder,
    SyncInterval,
    MaxItems,
    RemovedEntries,
    CreateOrderedPlaylist,
    SyncNow,
    LastSync,
    NeverSynced,
    ManagedManifestReadOnly,
    Truncated,
    MissingDuration,
    Orphaned,
    Materialized,
    State,
    Upstream,
    NewestFirst,
    Keep,
    ResetChannels,
    RemoveBreakContent,
    RemoveDecoFromTemplate,
    RowsPerPage,
    SaveSettings,
    Save,
    SaveMultiCollection,
    SavePlaylist,
    SaveTroubleshootingReportsToDisk,
    SaveAsSmartCollection,
    SearchQueryRequired,
    Scanner,
    ScanningMinimumLogLevel,
    Scheduling,
    SchedulingMinimumLogLevel,
    StartWithWindows,
    StartWithWindowsHelp,
    Schedules,
    AddSchedule,
    AddScheduleItem,
    AddSmartCollection,
    Album,
    Chronological,
    CollectionSize,
    CollectionType,
    Count,
    CopySchedule,
    CopyScheduleItem,
    CustomTitle,
    DeleteSchedule,
    DeleteScheduleItem,
    DeleteSmartCollection,
    DetailRowsPerPage,
    Director,
    Directors,
    DiscardToFillAttempts,
    Duration,
    Dynamic,
    EditProperties,
    EditScheduleItems,
    EditSmartCollection,
    EnterNameForNewSchedule,
    Fixed,
    FixedStartTimeBehavior,
    Flood,
    Flexible,
    FlexibleStartSoonAsPossible,
    FlexibleStartSoonAsPossibleAfterStartTime,
    FillWithGroupMode,
    GraphicsElements,
    AddGraphicsElement,
    DrawtextRendererOption,
    DrawtextSettings,
    DrawvgRendererOption,
    EditGraphicsElement,
    FontFile,
    FontFileOptionalHelp,
    GraphicsElement,
    GraphicsElementConfigInvalid,
    GraphicsElementKindImage,
    GraphicsElementKindMotion,
    GraphicsElementKindScript,
    GraphicsElementKindSubtitle,
    GraphicsElementKindText,
    GraphicsElementNameTooLong,
    GraphicsElementNotEditable,
    GraphicsElementNotFound,
    GraphicsElementNotText,
    GraphicsElementSaved,
    GraphicsElementSaveFailed,
    GraphicsElementType,
    GraphicsExistingDrawvgHelp,
    GraphicsStreamRestartHelp,
    NoGraphicsElements,
    RefreshPeriodBounds,
    RefreshPeriodMilliseconds,
    ReloadFrameInterval,
    ReloadFrameIntervalBounds,
    Renderer,
    SaveGraphicsElement,
    TextTemplate,
    TextTemplateRequired,
    TextTemplateTooLong,
    GraphicsIds,
    Genres,
    GroupName,
    GroupMultiPartEpisodesHelp,
    GuideMode,
    KeepMultiPartEpisodesTogether,
    Marathon,
    MarathonBatchSize,
    MarathonBatchSizeHelp,
    MarathonGroupBy,
    MarathonShuffleGroups,
    MarathonShuffleItems,
    MidRollFiller,
    MidRoll,
    MoveDown,
    MoveUp,
    ReorderPlaylistItem,
    MultiCollection,
    MultiCollectionDeleted,
    MultiCollectionItems,
    MultiCollectionNameRequired,
    MultiCollectionNameTooLong,
    MultiCollectionSaved,
    MultiEpisodeGroupSize,
    MultiEpisodeShuffle,
    Multiple,
    MultipleCount,
    MultipleCountHelp,
    MultipleMode,
    NewScheduleName,
    NewSmartCollection,
    NoScheduleItems,
    NoSchedules,
    Normal,
    Offline,
    One,
    OrderedGroups,
    Playback,
    PlaybackOrder,
    PlaybackOrderUpdateFailed,
    PlaybackOrderUpdated,
    PlaylistItemSize,
    PlayoutDurationHours,
    PlayoutDurationMinutes,
    PostRollFiller,
    PostRoll,
    PreRollFiller,
    PreRoll,
    Random,
    RandomCount,
    RandomRotation,
    RemainingDurationHelp,
    RandomStartPoint,
    RemoveFailed,
    RemovedFromCollection,
    RemoveFromCollection,
    Rename,
    RenameFailed,
    ReorderFailed,
    RerunFirstRun,
    RerunRerun,
    ReadOnlyTwentyFourHourPlayoutView,
    SaveSchedule,
    SaveScheduleItems,
    SaveSmartCollection,
    Schedule,
    ScheduleActionFailed,
    ScheduleAsGroup,
    ScheduleKind,
    ScheduleKindBlock,
    ScheduleKindClassic,
    ScheduleKindExternalJson,
    ScheduleKindScripted,
    ScheduleKindSequential,
    SchedulingContext,
    ScheduleItem,
    ScheduleItemActionFailed,
    ScheduleItemFiller,
    ScheduleItemFillerHelp,
    ScheduleItemOverrides,
    ScheduleItems,
    ScheduleNotFound,
    ScheduleSaveFailed,
    SearchForDecos,
    SearchForDecoTemplates,
    SearchForSchedules,
    SearchForManualCollections,
    SearchForRerunCollections,
    SearchForSmartCollections,
    SearchForTemplates,
    SearchForBlocks,
    SearchForMultiCollections,
    SearchQuery,
    BlockSearchQueryPlaceholder,
    SearchTitle,
    SearchTitleDisplay,
    SearchSmartCollection,
    Season,
    SeasonEpisode,
    Unnumbered,
    SeasonNotFound,
    Select,
    SelectCollection,
    SelectAll,
    SelectGroupForNewTemplate,
    SelectScheduleItemPrompt,
    SelectSchedule,
    SelectSmartCollection,
    Shuffle,
    ShuffleInOrder,
    ShuffleScheduleItems,
    ShuffleScheduleItemsHelp,
    ShuffledGroups,
    Show,
    SimilarityRules,
    SmartShuffle,
    ShowNotFound,
    Shows,
    Start,
    StartTime,
    StartTimeOnOrAfter,
    StartType,
    Strict,
    StrictAlwaysWaitExactStartTime,
    StrictWaitExactStartTime,
    SavedDecos,
    SavedDecoTemplates,
    TailFiller,
    TailMode,
    Tail,
    TelevisionSeason,
    TelevisionShow,
    TreatCollectionsAsShows,
    TreatCollectionsAsShowsHelp,
    Type,
    TypeToSearch,
    Search,
    Filters,
    ActiveFilters,
    ClearFilters,
    CreateSmartCollection,
    FromYear,
    ToYear,
    CollectionComposer,
    CollectionDetails,
    Rules,
    Overview,
    MatchAll,
    MatchAllHelp,
    MatchAny,
    MatchAnyHelp,
    ExcludeAny,
    ExcludeAnyHelp,
    AddRule,
    EditRule,
    RemoveRule,
    RuleField,
    Genre,
    Studio,
    Network,
    Actor,
    Writer,
    SubtitleLanguage,
    ShowTitle,
    ContentKindLabel,
    SubKindLabel,
    HasCommentary,
    AllFields,
    MatchType,
    MatchValue,
    TextContains,
    ExactPhrase,
    Wildcard,
    Fuzzy,
    Range,
    RangeStart,
    RangeEnd,
    NestedGroup,
    NestedGroupHelp,
    AddNestedGroup,
    EditNestedGroup,
    BackToRules,
    BackToSearch,
    CanonicalQuery,
    CanonicalQueryHelp,
    LivePreview,
    MatchingItems,
    QueryValid,
    QueryInvalid,
    SearchRuleValueRequired,
    SearchGroupRequired,
    SearchWildcardRequired,
    SavedSearches,
    SavedSearchName,
    SavedSearchHelp,
    ChooseSavedSearch,
    LoadSavedSearch,
    SaveSavedSearch,
    UpdateSavedSearch,
    NewSavedSearch,
    DeleteSavedSearch,
    SavedSearchSaved,
    SavedSearchDeleted,
    SavedSearchLoaded,
    SavedSearchNameRequired,
    SavedSearchNameTooLong,
    SavedSearchNameUnique,
    NoSavedSearches,
    RefreshPreview,
    NoRules,
    SearchFiltersHelp,
    SearchMedia,
    SearchMoviesOrShows,
    SearchAddonMediaList,
    SearchingMinimumLogLevel,
    Settings,
    SettingsSaveFailed,
    ShowMediaInfo,
    ScriptedScheduleTimeout,
    SkipMissingItems,
    SkipMissingItemsHelp,
    SmartCollections,
    SmartCollection,
    SmartCollectionDeleted,
    SmartCollectionQueryPlaceholder,
    SmartCollectionSaved,
    SaveAddonMediaList,
    Songs,
    SplitTimeEvenly,
    StreamingMinimumLogLevel,
    Studios,
    Support,
    Template,
    TemplateCopied,
    TemplateCreated,
    TemplateDeleted,
    TemplateGroup,
    TemplateGroupCreated,
    TemplateGroupDeleted,
    TemplateGroupName,
    TemplateGroups,
    TemplateName,
    TemplateNotFound,
    Templates,
    Theme,
    ThisActionCannotBeUndone,
    ThisCollectionHasNoItemsYet,
    Thumbnail,
    Title,
    ToggleNavigation,
    TotalDurationHelp,
    TotalPointsHelp,
    TotalProgressHelp,
    AddonMediaList,
    AddonMediaListActionFailed,
    AddonMediaListAdded,
    AddonMediaListDeleted,
    AddonMediaListMatched,
    AddonMediaListNotFound,
    AddonMediaListSaveFailed,
    AddonMediaListSaved,
    AddonMediaLists,
    AddonToolMaintenance,
    AddonToolMaintenanceNone,
    AddonToolUpdatePermission,
    AddonToolMaintenanceQueued,
    AddonToolMaintenanceRunning,
    AddonToolMaintenanceWaiting,
    AddonToolUpdated,
    AddonToolAlreadyCurrent,
    AddonToolManualUpdate,
    AddonToolManualHelp,
    AddonToolFailedHelp,
    AddonToolUpdateFailedStale,
    AddonToolUpdateVersions,
    AddonToolVersionHelp,
    AddonToolUnavailableHelp,
    AddonToolBusyHelp,
    AddonToolCleanupHelp,
    AddonListDiskWarnings,
    AddonListDiskWarningGb,
    AddonListsCombinedDiskWarningGb,
    AddonListDiskWarningHelp,
    AddonListDiskWarningFailed,
    NextRuntimeAvailability,
    NextOverlayInvalid,
    NextOverlayIncomplete,
    JavascriptAudioSelectionTitle,
    JavascriptAudioSelectionHealthy,
    JavascriptAudioSelectionFailures,
    AddonListDiskUsageTitle,
    AddonListDiskUsageBrief,
    AddonListDiskUsageUnavailable,
    AddonListDiskUsageUnavailableBrief,
    AddonListDiskUsageLine,
    AddonListsCombinedDiskUsageLine,
    AddonMediaListUrl,
    AddonMediaListFolder,
    AddonMediaListFolderHelp,
    DestructiveActions,
    ReconcileEntireSource,
    ReconcileEntireSourceHelp,
    ReconcileQueued,
    ReconcileFailed,
    ResetAndReload,
    ResetAddonMediaListHelp,
    SourceHost,
    ResetRemoves,
    ResetRemovesItems,
    ResetRemovesArtifacts,
    ResetRemovesRuntime,
    ResetRetains,
    ResetRetainsSettings,
    ResetReloadFailureWarning,
    TypeListNameToConfirm,
    ResetQueued,
    ResetAddonMediaListFailed,
    ChapterInput,
    ChapterInputHelp,
    RestoreProviderChapters,
    ManagedByAddon,
    Trash,
    TrashAlreadyEmpty,
    TrashEmpty,
    TrashEmptyConfirm,
    TrashEmptyConfirmHelp,
    TrashSubtitle,
    Troubleshooting,
    TunerCount,
    TvShows,
    Ui,
    Unavailable,
    UseActualTimes,
    UseCustomPlaybackOrder,
    UseChaptersAsMediaItems,
    UseChaptersAsMediaItemsHelp,
    UseGraphicsElementsDuringFiller,
    UseWatermarkDuringFiller,
    UseEmbeddedSubtitles,
    Uuid,
    UntitledItem,
    WindowsStartup,
    WindowsStartupNeedsRepair,
    WindowsStartupUnavailable,
    ActiveTranscodeLimit,
    ActiveTranscodeLimitHelp,
    ActiveTranscodeSuggestionHardware,
    ActiveTranscodeSuggestionSoftware,
    Peak,
    StreamsInUse,
    SuggestedTranscodeLimit,
    Watermarks,
    Width,
    WorkAheadSegmenterLimit,
    WorkAheadSegmenterLimitHelp,
    AccelerationKind,
    AddFfmpegProfile,
    AddProfile,
    AllowBFrames,
    AudioBitrate,
    AudioBufferSize,
    AudioChannels,
    AudioFormat,
    Audio,
    AllowWatermarks,
    BitDepth,
    Copy,
    CopyBlock,
    CopyFailed,
    CopyNameSuffix,
    CopyProfile,
    CopyTemplate,
    Crop,
    DeinterlaceVideo,
    Edit,
    EditBlock,
    EditCollection,
    EditDeco,
    EditDecoTemplate,
    EditFillerPreset,
    EditFfmpegProfile,
    EditMultiCollection,
    EditPlaylist,
    EditRerunCollection,
    EditTemplate,
    FillerKind,
    General,
    HardwareAcceleration,
    HardwareIfPossible,
    NewProfileName,
    NoneOption,
    NumHelp,
    OptionalDisplayTitle,
    TranscodeAudio,
    NormalizeColors,
    NormalizeFramerate,
    NormalizeLoudness,
    TranscodeVideo,
    Off,
    Pad,
    PadNearestFifteenMinutes,
    PadNearestFiveMinutes,
    PadNearestTenMinutes,
    PadNearestThirtyMinutes,
    PadMode,
    PadToNearestMinute,
    PointHelp,
    ProfileCopied,
    ProfileNotFound,
    QsvExtraHardwareFrames,
    Resolution,
    SampleRateKhz,
    SaveDecoTemplate,
    SaveProfile,
    SaveFillerPreset,
    SaveRerunCollection,
    SaveSequentialSchedule,
    SaveTemplate,
    SaveBlock,
    ScaleAndPad,
    ScalingBehavior,
    OutputDynamicRange,
    OutputDynamicRangeCopyHint,
    HdrStaticMetadataHint,
    HdrMasteringDisplay,
    HdrMaxCll,
    HdrMaxFall,
    Software,
    Stretch,
    TargetLoudnessLufs,
    ThreadCountAuto,
    TonemapAlgorithm,
    VaapiDevicePath,
    VaapiDisplay,
    VaapiDriver,
    Video,
    VideoBitrate,
    VideoBufferSize,
    VideoFormat,
    VideoPreset,
    VideoTune,
    VideoProfile,
    ViewCollection,
    ViewPlaylist,
    AddAll,
    AddAllToCollection,
    AddAllToPlaylist,
    AddedItemToPlaylist,
    AddedArtistToSchedule,
    AddedMovieToSchedule,
    AddedSeasonToSchedule,
    AddedShowToSchedule,
    AddingItem,
    AddingItemToPlaylist,
    Artists,
    ClearSelection,
    Collection,
    CollectionNameRequired,
    CollectionNotFound,
    CollectionRenamed,
    EditAddonMediaListProperties,
    EnterAddonMediaListUrl,
    EnterSearchQuery,
    Episodes,
    LoadingCollections,
    LoadingPlaylistGroups,
    LoadingPlaylists,
    GeneratePlaylist,
    GeneratePlaylistFromSortedAddonMediaListItems,
    MusicVideos,
    NewCollectionName,
    NoMediaMatched,
    NoAddonMediaListsYet,
    NoItemsToAdd,
    NoItemsToPreview,
    NoPlaylistsInGroup,
    PleaseChoosePlaylist,
    RequestFailed,
    Released,
    SaveAs,
    Seasons,
    SeeAll,
    SelectGroup,
    SelectPlaylist,
    SelectSearchCollectionHelp,
    SelectSearchPlaylistHelp,
    Selected,
    SelectCollectionToAddThisMovie,
    SelectPlaylistToAddThisMovie,
    Add,
    AddLibrary,
    Address,
    AddressRequired,
    ApiKey,
    ApiKeyRequired,
    AuthorizedRedirecting,
    CheckingEveryTwoSeconds,
    ClickAuthorizePlex,
    ClickPlayoutItemHelp,
    ConfigureConnection,
    Connection,
    DeepScanCollections,
    DeepScanLibrary,
    DeepScan,
    EditLibrary,
    EnterOnePathPerLine,
    Error,
    Expression,
    ExternalCollections,
    Kind,
    Library,
    LibraryKind,
    LibraryPaths,
    LocalLibraries,
    LocalPath,
    MediaKind,
    NoLibrariesConfigured,
    NoPlayoutItemsInWindow,
    NoPlayoutsFound,
    NoSchedulingContext,
    NoPathReplacements,
    Paths,
    PathReplacements,
    PlexSignInHelp,
    Remote,
    RemotePath,
    Remove,
    RemoveBlockFromTemplate,
    RemoveContent,
    Retry,
    Saved,
    Saving,
    QuickScan,
    ScanCollections,
    ScanFailed,
    ScanLibrary,
    ScanQueued,
    Scanning,
    Server,
    ServerAddress,
    SignInToPlex,
    SignOutOfPlex,
    Sync,
    WaitingForPlexAuthorization,
    AddChannel,
    Any,
    BlockPlayoutTroubleshooting,
    BlockPlayoutHistory,
    Categories,
    CategoriesHelp,
    Channel,
    ChannelCouldNotBeSaved,
    ChannelDisabled,
    ChannelHiddenFromEpg,
    ChannelLogo,
    ChannelLogoPreview,
    ChannelLogoUploadFailed,
    Check,
    Continuous,
    DateAndTime,
    DefaultOption,
    DefaultSchedule,
    DeleteChannel,
    DownloadMediaSample,
    DownloadResults,
    DisableWatermarks,
    EditChannel,
    EditChannelNumbers,
    EditNumbers,
    ExternalLogoUrl,
    Fail,
    Fallback,
    FallbackFiller,
    FfmpegProfile,
    FirstPage,
    FirstRunPlaybackOrder,
    Forced,
    GenerateSubtitles,
    Generated,
    Details,
    Full,
    Group,
    FfmpegRuntime,
    FfmpegRuntimeDownloaded,
    FfmpegRuntimeLatestRelease,
    FfmpegRuntimeRelease,
    FfmpegRuntimeSource,
    FfmpegRuntimeSourceDownloaded,
    FfmpegRuntimeSourceManaged,
    FfmpegRuntimeSourceOverride,
    FfmpegRuntimeSourcePath,
    FfmpegRuntimeSourceUnknown,
    FfmpegRuntimeUpdateAvailable,
    FfmpegRuntimeUpdateCheckFailed,
    FfmpegRuntimeUpdateFailed,
    FfmpegRuntimeUpToDate,
    FfmpegRuntimeUpdated,
    FfmpegRuntimeVersion,
    HealthNoPersistedRun,
    HealthPersistedRunHelp,
    HistoryDetails,
    HlsDirect,
    HlsSegmenter,
    Hours,
    IdleBehavior,
    IdleBehaviorHelp,
    Info,
    Import,
    IsEnabled,
    Json,
    KeepRunning,
    Key,
    LastMidFillerHelp,
    LastPage,
    Level,
    LoadingMediaInfo,
    Logs,
    PlaybackSpeed,
    Logo,
    MediaItem,
    Message,
    MissingMediaItemQueryParameter,
    Mirror,
    MirrorOptionsHelp,
    MirrorSourceChannel,
    MusicVideoCreditsMode,
    MusicVideoCreditsTemplate,
    NoMatchingRecordsFound,
    NoFfmpegProfiles,
    NoBlockPlayoutSelected,
    NoBlocks,
    NoHistory,
    NoMessages,
    NvidiaCapabilitiesNotCollected,
    NotApplicable,
    NotAvailable,
    Number,
    OnDemand,
    MatchedPointsHelp,
    Packets,
    Pass,
    ParityDebt,
    PcrFindings,
    Pids,
    PlaybackTroubleshooting,
    Play,
    PlayoutTimeline,
    PlayoutMode,
    PlayoutModeHelp,
    PlayoutOffset,
    PlayoutSource,
    PlayoutSourceHelp,
    Preview,
    PreviewBlockPlayout,
    PreviewChannel,
    ChannelPreview,
    ReloadStream,
    StopPreview,
    OpenFullPlayer,
    BackToChannels,
    NowPlaying,
    UpNext,
    ProgrammeGuide,
    EpgPreview,
    EpgPreviewDescription,
    EpgZoom,
    EpgNow,
    NoEpgChannels,
    ScheduleGap,
    Filler,
    NoProgramming,
    ChannelDisabledPreview,
    ChannelNotAiring,
    ChannelNoPlayout,
    PreviewIdle,
    PreviewConnecting,
    PreviewLive,
    PreviewPaused,
    PreviewRecovering,
    PreviewStopped,
    PreviewUnsupported,
    PreviewAuthorizationFailed,
    PreviewUnavailable,
    PreviewCapacityReached,
    PreviewNetworkError,
    PreviewMediaError,
    PreviewDerivativeNote,
    JavascriptRequiredPlayback,
    ConfirmStopTranscodeSession,
    PreviewPlaylistPlayout,
    PreferredAudioTitle,
    PreferredSubtitleLanguage,
    QsvCapabilitiesNotCollected,
    RawSampleArchived,
    RemoveStoredLogo,
    Report,
    RetainRawSample,
    RunId,
    SaveChannel,
    SaveChannels,
    Second,
    Seconds,
    SelectProfile,
    SequentialSchedule,
    SequentialScheduleValidation,
    Services,
    SeekSeconds,
    Schema,
    ShowDisabled,
    ShowInEpg,
    SlugSeconds,
    SlugSecondsHelp,
    SongVideoMode,
    Song,
    SortById,
    SortByLevel,
    SortByName,
    SortByTimestamp,
    Stable,
    StopOnDisconnect,
    StabilityMeter,
    StopScheduling,
    StopSchedulingBlockItems,
    StopTranscodeSession,
    StartFromBeginning,
    StreamSelector,
    StreamSelectorMode,
    StreamingEngine,
    NextEngineTextSubtitleMode,
    NextEngineTextSubtitleModeHelp,
    InvalidNextEngineTextSubtitleMode,
    SubtitleBurn,
    SubtitleConvertIfPossible,
    PreviewCaptions,
    InvalidStreamingEngine,
    StreamingMode,
    Styles,
    Summary,
    Subtitle,
    SubtitleMode,
    SubtitleModeHelp,
    Timestamp,
    Timeline,
    Target,
    Tags,
    Tools,
    TranscodeMode,
    TranscodeModeHelp,
    TsdDuckVersion,
    TroubleshootPlayback,
    TroubleshootChannel,
    TroubleshootingReports,
    TroubleshootingPlaybackStartDebt,
    UnableToLoadMediaInfo,
    UnableToLoadPlaybackTroubleshooting,
    UploadLogo,
    UploadLogoHelp,
    Warning,
    Watermark,
    WatermarkIds,
    WatermarkMode,
    Window,
    WithProgress,
    Writers,
    Validate,
    Yaml,
    YamlFile,
    Messages,
    CollectionName,
    MediaItemType,
    MediaItemTitle,
    ActualSize,
    AnalysisJson,
    ArchivePath,
    AddWatermark,
    AspectRatioGuides,
    BottomLeft,
    BottomMiddle,
    BottomRight,
    CopyWatermark,
    CopyOf,
    Completed,
    ContinuityFindings,
    CurrentImage,
    Custom,
    ChannelLogoPreviewUnavailable,
    DeleteWatermark,
    DurationSeconds,
    DangerButton,
    DeleteDialog,
    Decrement,
    EditWatermark,
    EnableWatermarkToPreview,
    DisabledButton,
    Dialogs,
    Frequency,
    GeneratingPreview,
    HorizontalMarginPercent,
    GuideFourThree,
    Identity,
    Image,
    ImageSource,
    Intermittent,
    IntermittentTiming,
    LeftMiddle,
    Location,
    MiddleCenter,
    Mode,
    PublicMpegTs,
    Minutes,
    Moods,
    ModalCancelled,
    ModalConfirmed,
    NewName,
    NativeAssetDimensions,
    NextWatermark,
    Opacity,
    OpacityExpression,
    OpacityOneToHundred,
    InternalMpegTs,
    Permanent,
    PerPass,
    PlaceWithinSourceContent,
    Placement,
    PlacementPreview,
    PreviewFailed,
    PreviewReferenceFullHd,
    PreviousWatermark,
    RightMiddle,
    SaveWatermark,
    SampleBytes,
    SampleUrl,
    Scaled,
    SelectCustomImageToPreview,
    Size,
    StandardError,
    StandardOutput,
    Status,
    TopLeft,
    TopMiddle,
    TopRight,
    UploadFailed,
    UploadImage,
    UnsavedChanges,
    UnsavedWatermarkChangesMessage,
    VerticalMarginPercent,
    WatermarkCopied,
    WatermarkNotFound,
    WatermarkPlacementPreview,
    WatermarkPreview,
    WatermarkNavigation,
    WidthPercent,
    ZIndex,
    DiscardChanges,
    XmltvBlockBehavior,
    XmltvTimeZone,
    Xmltv,
    Inherit,
    Override,
    Replace,
    TrimToFit,
    Yes,
    Buttons,
    CheckValue,
    ChannelName,
    ChannelNumber,
    EnterName,
    ErrorOccurred,
    ErrorToast,
    ExampleModal,
    ExampleModalBody,
    FormInputs,
    Icons,
    IncrementBadge,
    InfoToast,
    JustFyi,
    LinkButton,
    NumValue,
    OpenModal,
    OperationSucceeded,
    PagedTableDemo,
    PrimaryButton,
    SecondaryButton,
    SelectValue,
    SomethingMightBeWrong,
    SuccessToast,
    TextButton,
    TextValue,
    Toasts,
    WarningToast,
    YamlScript,
    // Playouts page (Pages/Playouts.razor)
    AddPlayout,
    ClassicScheduleMenu,
    BlockScheduleMenu,
    SequentialScheduleMenu,
    ScriptedScheduleMenu,
    JsonDizqueTvScheduleMenu,
    ResetAllPlayouts,
    PlayoutBuildQueued,
    PlayoutBuildsQueued,
    SearchForPlayouts,
    PlayoutDetail,
    ShowFiller,
    EditPlayout,
    RefreshPlayout,
    ResetPlayout,
    ScheduleReset,
    SchedulePlayoutReset,
    EditExternalJsonFile,
    DeletePlayout,
    Troubleshoot,
    DailyResetTime,
    DoNotAutomaticallyReset,
    SaveChanges,
    ExternalJsonFileLabel,
    EditExternalJsonFilePrompt,
    UnscheduledPlayoutItem,
    DisabledChannelsAlreadyHavePlayout,
    ProgramSchedule,
    SequentialScheduleFileHelp,
    BlockTemplatesAddedLater,
    UnsupportedPlayoutKind,
    ClassicPlayout,
    ClassicPlayoutState,
    ScanRefreshState,
    SubtitleCleanup,
    SubtitleCleanupMessage,
    SubtitleCleanupBrief,
    ArtworkOrphans,
    ArtworkOrphansMessage,
    ArtworkOrphansBrief,
    ArtworkCardinality,
    HlsLiveDelivery,
    BlockPlayout,
    ScriptedPlayout,
    AlternateSchedules,
    EditAlternateSchedules,
    PlayoutTemplates,
    EditTemplates,
    DefaultDeco,
    EnableDefaultDeco,
    SaveDefaultDeco,
    SaveScriptedSchedule,
    EraseItems,
    ErasedPlayoutItems,
    SavedDefaultDeco,
    SavedScriptedSchedule,
    Maintenance,
    SaveAlternateSchedules,
    AddAlternateSchedule,
    SaveTemplates,
    PreviewCalendar,
    EditContent,
    PlayoutNestedCatchAllHelp,
    PlayoutNestedPriorityHelp,
    DaysOfWeek,
    DaysOfMonth,
    Months,
    LimitToDateRange,
    StartDay,
    EndDay,
    Enable,
    Up,
    Down,
    Weekdays,
    Weekends,
    Sunday,
    Monday,
    Tuesday,
    Wednesday,
    Thursday,
    Friday,
    Saturday,
    January,
    February,
    March,
    April,
    May,
    June,
    July,
    August,
    September,
    October,
    November,
    December,
    JanuaryShort,
    FebruaryShort,
    MarchShort,
    AprilShort,
    MayShort,
    JuneShort,
    JulyShort,
    AugustShort,
    SeptemberShort,
    OctoberShort,
    NovemberShort,
    DecemberShort,
}

pub const ALL_TEXT_KEYS: &[TextKey] = &[
    TextKey::ItemDetailsNotRequested,
    TextKey::ItemDetailsFailed,
    TextKey::ItemDetailsRetry,
    TextKey::ItemProviderPlayback,
    TextKey::ItemPosition,
    TextKey::ItemPrevious,
    TextKey::ItemNext,
    TextKey::ItemBack,
    TextKey::ItemTools,
    TextKey::ItemEpisodePosition,
    TextKey::ItemNumbered,
    TextKey::ItemUnnumberedHelp,
    TextKey::ItemNumberingHelp,
    TextKey::ItemMetadata,
    TextKey::ItemMetadataHelp,
    TextKey::ItemSaveHelp,
    TextKey::ItemEditNumbering,
    TextKey::ItemReplacementHelp,
    TextKey::ItemReplacementPending,
    TextKey::ItemSearchReplacement,
    TextKey::ItemRemoveReplacement,
    TextKey::ItemInformation,
    TextKey::ItemInformationHelp,
    TextKey::ItemSavedToolsHelp,
    TextKey::ItemViewMedia,
    TextKey::ItemSavedValue,
    TextKey::ItemFromList,
    TextKey::ItemUseListSetting,
    TextKey::ItemOverride,
    TextKey::ItemPackageDefault,
    TextKey::ItemPreviewOptions,
    TextKey::ItemAccessActive,
    TextKey::OpenItemInNewTab,
    TextKey::BulkItemsToChange,
    TextKey::BulkScopeHelp,
    TextKey::BulkDetectSeasons,
    TextKey::BulkFirstSeason,
    TextKey::BulkReviewChanges,
    TextKey::BulkPreviewSaves,
    TextKey::BulkPreviewNothing,
    TextKey::BulkApplyNumbering,
    TextKey::BulkApplyChanges,
    TextKey::BulkCancelPreview,
    TextKey::ActiveFilter,
    TextKey::AddonWorkspaceLengthFilter,
    TextKey::AddonWorkspaceMinimumLength,
    TextKey::AddonWorkspaceMaximumLength,
    TextKey::AddonWorkspaceUnknownLength,
    TextKey::AddonWorkspaceLengthHelp,
    TextKey::AddonWorkspaceReviewLatest,
    TextKey::AddonWorkspaceReviewRetained,
    TextKey::AddonWorkspaceStreamHelp,
    TextKey::AddonWorkspaceDownloadHelp,
    TextKey::AddonWorkspaceNewHelp,
    TextKey::AddonWorkspaceNoOptions,
    TextKey::AddonWorkspaceSizeLimit,
    TextKey::AddonWorkspaceItems,
    TextKey::AddonWorkspaceSettings,
    TextKey::AddonWorkspacePlayback,
    TextKey::AddonWorkspaceActivity,
    TextKey::AddonWorkspaceSections,
    TextKey::AddonWorkspaceSaveSettings,
    TextKey::AddonWorkspaceSaveItem,
    TextKey::AddonWorkspaceSaveNumbering,
    TextKey::AddonWorkspacePublish,
    TextKey::AddonWorkspacePublished,
    TextKey::AddonWorkspacePending,
    TextKey::AddonWorkspacePreparationRequired,
    TextKey::AddonWorkspacePreparing,
    TextKey::AddonWorkspaceAddIncomplete,
    TextKey::AddonWorkspacePreparationFailed,
    TextKey::AddonWorkspaceIncludeSelected,
    TextKey::AddonWorkspaceExcludeSelected,
    TextKey::AddonWorkspaceInclusionHelp,
    TextKey::AddonWorkspacePublicationFailed,
    TextKey::AddonWorkspacePublicationRestoredHelp,
    TextKey::AddonWorkspaceParentMetadataPreserved,
    TextKey::AddonWorkspaceImporting,
    TextKey::AddonWorkspaceImportingHelp,
    TextKey::AddonWorkspaceImportSteps,
    TextKey::AddonWorkspaceChooseProvider,
    TextKey::AddonWorkspaceExcluded,
    TextKey::AddonWorkspaceSort,
    TextKey::AddonWorkspaceAscending,
    TextKey::AddonWorkspaceDescending,
    TextKey::AddonWorkspaceItemDate,
    TextKey::AddonWorkspaceSelectVisible,
    TextKey::AddonWorkspaceDeselectVisible,
    TextKey::AddonWorkspaceSelectFirst,
    TextKey::AddonsNoneInstalled,
    TextKey::AddonsNoneAvailable,
    TextKey::AddonsSetupHelp,
    TextKey::AddonPackageHelp,
    TextKey::AddonRepositoryPackageHelp,
    TextKey::AddonPackageUploadHelp,
    TextKey::AddonRepositoryUploadHelp,
    TextKey::AddonConfigurationSavedRecheck,
    TextKey::AddonCheckRequired,
    TextKey::AddonCheckPending,
    TextKey::RetryPreparation,
    TextKey::PreparationFailedItems,
    TextKey::PreparationFailureFailed,
    TextKey::PreparationFailureInputChanged,
    TextKey::PreparationFailureInvalidRequest,
    TextKey::PreparationFailureCancelled,
    TextKey::PreparationFailureAttemptsExhausted,
    TextKey::PreparationFailureStorageFull,
    TextKey::PreparationFailureUnsupported,
    TextKey::PreviousSavedRevision,
    TextKey::AddonWorkspacePreparationActivity,
    TextKey::AddonWorkspaceBulk,
    TextKey::AddonWorkspaceSelection,
    TextKey::AddonWorkspaceOffPage,
    TextKey::AddonWorkspaceDiscard,
    TextKey::AddonWorkspaceSelected,
    TextKey::AddonWorkspaceMatching,
    TextKey::AddonWorkspacePage,
    TextKey::AddonWorkspaceAccess,
    TextKey::AddonWorkspaceRetained,
    TextKey::PreparationTasks,
    TextKey::PreparationAffectedItems,
    TextKey::PreparationIntervals,
    TextKey::PreparationDownloads,
    TextKey::PreparationSubtitles,
    TextKey::PreparationQueued,
    TextKey::PreparationRunning,
    TextKey::PreparationWaiting,
    TextKey::PreparationNotQueued,
    TextKey::LastActivity,
    TextKey::AddonAccessUnknownFailure,
    TextKey::TechnicalDetails,
    TextKey::AddonPermissionNetworkHelp,
    TextKey::AddonPermissionMediaHelp,
    TextKey::AddonPermissionFfmpegHelp,
    TextKey::AddonPermissionCommandHelp,
    TextKey::AddonWorkspaceSaveConflict,
    TextKey::AddonWorkspaceSaveInvalid,
    TextKey::AddonWorkspaceSaveStorage,
    TextKey::AddonWorkspaceLoadIncomplete,
    TextKey::AddonWorkspaceNavigation,
    TextKey::AddonWorkspaceSeasonOnly,
    TextKey::AddonWorkspaceSeasonPerYear,
    TextKey::AddonWorkspaceYearDatesIncomplete,
    TextKey::AddonWorkspacePageSize,
    TextKey::AddonAccessTest,
    TextKey::AddonAccessHelp,
    TextKey::AddonAccessSaveFirst,
    TextKey::AddonAccessError,
    TextKey::AddonAccessNotChecked,
    TextKey::AddonAccessQueued,
    TextKey::AddonAccessRunning,
    TextKey::AddonAccessWaiting,
    TextKey::AddonAccessCancelled,
    TextKey::AddonAccessStale,
    TextKey::AddonAccessAccessible,
    TextKey::AddonAccessProfileUnavailable,
    TextKey::AddonAccessProfileLocked,
    TextKey::AddonAccessDecryptionFailed,
    TextKey::AddonAccessRestricted,
    TextKey::AddonLocalPlaybackAvailable,
    TextKey::AddonDerivedMetadataLimited,
    TextKey::AddonAccessNextAttempt,
    TextKey::AddonAccessExpires,
    TextKey::AddonAccessRefresh,
    TextKey::AddonOptionsCurrentFilter,
    TextKey::AddonOptions,
    TextKey::AddonOptionsDraftHelp,
    TextKey::AddonOptionsProvisional,
    TextKey::AddonOptionsListDefault,
    TextKey::AddonOptionsInherited,
    TextKey::AddonOptionsOverride,
    TextKey::AddonOptionsFiniteOnly,
    TextKey::AddonOptionsResetPackage,
    TextKey::AddonOptionsResetList,
    TextKey::AddonOptionsMixed,
    TextKey::AddonOptionsPreviewHelp,
    TextKey::AddonOptionsBefore,
    TextKey::AddonOptionsAfter,
    TextKey::AddonOptionsConflict,
    TextKey::AddonOptionsNone,
    TextKey::Addon,
    TextKey::PrimaryFeature,
    TextKey::MovieExtra,
    TextKey::NeedReview,
    TextKey::ReviewRecommended,
    TextKey::PublicationReadiness,
    TextKey::PublicationBlockers,
    TextKey::PublicationActivity,
    TextKey::PublicationJobs,
    TextKey::PublicationEvents,
    TextKey::NoPublicationActivity,
    TextKey::CopyRedactedDiagnostics,
    TextKey::DownloadRedactedDiagnostics,
    TextKey::RedactedDiagnosticsCopied,
    TextKey::PublicationGeneration,
    TextKey::CorrelationId,
    TextKey::TargetPublishedRevision,
    TextKey::RelatedOperation,
    TextKey::FilesystemTransition,
    TextKey::RetryClass,
    TextKey::RecoveryAction,
    TextKey::OperatingSystemErrorCode,
    TextKey::PublicationOutcome,
    TextKey::PublicationExpired,
    TextKey::EventExistingJobAdopted,
    TextKey::EventEnqueued,
    TextKey::EventLeaseWaitStarted,
    TextKey::EventLeaseAcquired,
    TextKey::EventLeaseReclaimed,
    TextKey::EventPhaseAdvanced,
    TextKey::EventChildScanStarted,
    TextKey::EventChildScanCompleted,
    TextKey::EventChildScanFailed,
    TextKey::EventOriginalFailure,
    TextKey::EventRecoveryFailure,
    TextKey::EventRetryScheduled,
    TextKey::EventRecoveryRequested,
    TextKey::EventCompleted,
    TextKey::EventHistoryTruncated,
    TextKey::TriggerAutomaticRefresh,
    TextKey::TriggerOperator,
    TextKey::TriggerRestoreRevision,
    TextKey::TestSource,
    TextKey::TestSourceHelp,
    TextKey::SourceTestQueued,
    TextKey::SourceTestRunning,
    TextKey::SourceTestStale,
    TextKey::SourceAvailable,
    TextKey::SourceUnsupported,
    TextKey::SourceTestFailed,
    TextKey::SourceRevision,
    TextKey::FirstPageComplete,
    TextKey::ProviderIdentity,
    TextKey::ReturnedFirstPageItems,
    TextKey::ProviderTotalItemCount,
    TextKey::DraftChanges,
    TextKey::ExactDraftDiffHelp,
    TextKey::DraftDiffUnavailable,
    TextKey::MetadataChanges,
    TextKey::ExcludedItems,
    TextKey::Conflicts,
    TextKey::PendingRemovals,
    TextKey::PendingRemovalHelp,
    TextKey::Reappearances,
    TextKey::RemovalCandidates,
    TextKey::ConfirmRemoval,
    TextKey::RemovalConfirmed,
    TextKey::RemovalConfirmationFailed,
    TextKey::RevisionHistory,
    TextKey::RevisionHistoryHelp,
    TextKey::NoRevisionHistory,
    TextKey::Revision,
    TextKey::CompareRevisions,
    TextKey::RestoreRevision,
    TextKey::RestoreRevisionWarning,
    TextKey::RestoreRevisionQueued,
    TextKey::RestoreRevisionFailed,
    TextKey::PublishedItems,
    TextKey::GeneratedPlaylists,
    TextKey::ActiveOrFuturePlayouts,
    TextKey::EffectiveOptions,
    TextKey::Files,
    TextKey::Artifacts,
    TextKey::Added,
    TextKey::Removed,
    TextKey::Changed,
    TextKey::FilesystemPreviousBackedUp,
    TextKey::FilesystemCandidateCommitted,
    TextKey::FilesystemPreviousRestored,
    TextKey::FilesystemProjectionRestored,
    TextKey::FilesystemRevisionVerified,
    TextKey::FilesystemCleanupCompleted,
    TextKey::RetryTransientRecovery,
    TextKey::ErrorFilesystemIo,
    TextKey::ErrorPermissionDenied,
    TextKey::ErrorSharingViolation,
    TextKey::ErrorUnsafeFilesystemState,
    TextKey::ErrorInvalidPublicationMarker,
    TextKey::ErrorPublicationIntegrity,
    TextKey::OutcomePublished,
    TextKey::OutcomeRestoredAfterFailure,
    TextKey::ReviewRecommendations,
    TextKey::OptionalDetails,
    TextKey::DraftSaved,
    TextKey::SavedRevision,
    TextKey::PublishList,
    TextKey::PreparePublication,
    TextKey::ProcessedMediaRequired,
    TextKey::ProcessedMediaQueued,
    TextKey::PreparingProcessedMedia,
    TextKey::PreparingPublication,
    TextKey::Publishing,
    TextKey::PublicationQueued,
    TextKey::RestoringPublication,
    TextKey::Published,
    TextKey::PublishSavedRevisionHelp,
    TextKey::ChangesSavedImmediately,
    TextKey::ReviewItem,
    TextKey::ShowFullDescription,
    TextKey::ShowLess,
    TextKey::ImportAList,
    TextKey::ImportOverview,
    TextKey::AddonContextSelected,
    TextKey::CompletedIn,
    TextKey::NoPublicationBlockers,
    TextKey::BlockerOverviewIncomplete,
    TextKey::BlockerPublicationInProgress,
    TextKey::BlockerRecoveryRequired,
    TextKey::BlockerCollectionTitleMissing,
    TextKey::BlockerInvalidArtwork,
    TextKey::BlockerNoIncludedItems,
    TextKey::BlockerMoviePrimaryCount,
    TextKey::BlockerPlaybackDecisionRequired,
    TextKey::BlockerPlaybackDetailsUnavailable,
    TextKey::BlockerPlaybackDetailsFailed,
    TextKey::BlockerUnavailableWithoutReplacement,
    TextKey::BlockerItemTitleMissing,
    TextKey::BlockerSeasonEpisodeMissing,
    TextKey::BlockerArtistMissing,
    TextKey::PublishUnsavedChanges,
    TextKey::PublicationBlocked,
    TextKey::PublishBlocked,
    TextKey::RetryRecovery,
    TextKey::RestorePreviousRevision,
    TextKey::PublicationPhase,
    TextKey::PublicationAttempts,
    TextKey::LastProgress,
    TextKey::FailureReason,
    TextKey::RestrictedItemsExcluded,
    TextKey::RecoveryQueued,
    TextKey::RecoveryFailed,
    TextKey::PhaseAwaitingLease,
    TextKey::PhaseStaging,
    TextKey::PhaseScanning,
    TextKey::PhaseCommitting,
    TextKey::PhaseRestoringFiles,
    TextKey::PhaseRestoringProjection,
    TextKey::PhaseReconciling,
    TextKey::PhaseVerifying,
    TextKey::PhaseCleaningUp,
    TextKey::PhaseComplete,
    TextKey::InstallLeavesDisabled,
    TextKey::EnableActivatesPermissions,
    TextKey::FirstAired,
    TextKey::LoadItemDetailsAfterPublishing,
    TextKey::LoadItemDetailsHelp,
    TextKey::OverviewOnlyManualDetailsHelp,
    TextKey::PlaybackDetailsQueued,
    TextKey::PublishAgainAfterDetails,
    TextKey::FetchDetails,
    TextKey::DetailsQueued,
    TextKey::DetailsLoading,
    TextKey::RetryDetails,
    TextKey::DetailsComplete,
    TextKey::DetailsSkipped,
    TextKey::FindingProgrammes,
    TextKey::CheckingOverview,
    TextKey::ReadyToReview,
    TextKey::LoadingItemDetails,
    TextKey::WaitingForProvider,
    TextKey::OverviewOnly,
    TextKey::PauseImport,
    TextKey::ResumeImport,
    TextKey::RetryImportNow,
    TextKey::EmptyImportNeedsRetry,
    TextKey::EmptyImportRetryHelp,
    TextKey::CancelImport,
    TextKey::OtherPageItemsAffected,
    TextKey::ElapsedTime,
    TextKey::LastSuccessfulProgress,
    TextKey::EstimatedRemainingTime,
    TextKey::PlaybackBehavior,
    TextKey::UseProviderValue,
    TextKey::FiniteMedia,
    TextKey::LiveSource,
    TextKey::ProviderValueChanged,
    TextKey::ApplyToSelected,
    TextKey::ApplyToAllRemoteStreams,
    TextKey::SelectItem,
    TextKey::NumberEpisodes,
    TextKey::NumberEpisodesHelp,
    TextKey::FirstEpisode,
    TextKey::NumberToAssign,
    TextKey::AllRemaining,
    TextKey::ReplaceExistingEpisodeNumbers,
    TextKey::AutoDetectSeasons,
    TextKey::AutoDetectSeasonsHelp,
    TextKey::MinimumSeasonGapDays,
    TextKey::NumberingStartsAt,
    TextKey::FirstIncludedItem,
    TextKey::StartWithFirstItem,
    TextKey::StartHere,
    TextKey::SaveSeasonEpisode,
    TextKey::BulkEdit,
    TextKey::Scope,
    TextKey::AllItems,
    TextKey::CurrentPage,
    TextKey::ItemsNeedingReview,
    TextKey::KeepCurrentValue,
    TextKey::UndoLastChange,
    TextKey::Undo,
    TextKey::ChangeUndone,
    TextKey::MetadataConflicts,
    TextKey::MetadataConflictsHelp,
    TextKey::PreviousProviderValue,
    TextKey::NewProviderValue,
    TextKey::OperatorValue,
    TextKey::Discovered,
    TextKey::Examined,
    TextKey::ArchivedSkipped,
    TextKey::Emitted,
    TextKey::EditMetadata,
    TextKey::OneTagPerLine,
    TextKey::OriginalTitle,
    TextKey::ReleaseDate,
    TextKey::Year,
    TextKey::SortTitle,
    TextKey::MovieFilesAndExtras,
    TextKey::MovieFilesAndExtrasHelp,
    TextKey::EpisodesMetadataHelp,
    TextKey::MusicVideosMetadataHelp,
    TextKey::OtherVideosMetadataHelp,
    TextKey::SongsMetadataHelp,
    TextKey::ImagesMetadataHelp,
    TextKey::RemoteStreamsMetadataHelp,
    TextKey::CastAndCredits,
    TextKey::OneCreditPerLine,
    TextKey::Poster,
    TextKey::Artwork,
    TextKey::PublishMetadata,
    TextKey::IncludeItem,
    TextKey::IncludeInPublication,
    TextKey::MetadataDraftNotice,
    TextKey::ApplyToAllItemsPrompt,
    TextKey::TelevisionEpisode,
    TextKey::CollectionTypeAuto,
    TextKey::CurrentLibraryType,
    TextKey::CollectionTypeChangeHelp,
    TextKey::NonPlayableItems,
    TextKey::UnknownItems,
    TextKey::PlayableItems,
    TextKey::ApplyToAllItems,
    TextKey::ChangeAddon,
    TextKey::ChangeAddonHelp,
    TextKey::ChangeAddonPrompt,
    TextKey::AddonAutoDetect,
    TextKey::AddonAutoDetected,
    TextKey::AddonMediaListNoAddonForUrl,
    TextKey::DefaultRemoteStreamAddon,
    TextKey::DefaultRemoteStreamAddonHelp,
    TextKey::AddonDetailWait,
    TextKey::MinimumDetailWait,
    TextKey::MaximumDetailWait,
    TextKey::AddonDetailWaitHelp,
    TextKey::AddonDetailWaitInvalid,
    TextKey::Addons,
    TextKey::AddonOverview,
    TextKey::AddonUpdates,
    TextKey::AddonSources,
    TextKey::AddonNotFound,
    TextKey::BackToAddons,
    TextKey::AvailableVersion,
    TextKey::BrowseAddons,
    TextKey::CheckForUpdates,
    TextKey::InstallFromFile,
    TextKey::InstallRepositoryFromFile,
    TextKey::License,
    TextKey::UnknownSources,
    TextKey::UnknownSourcesHelp,
    TextKey::Verified,
    TextKey::Unverified,
    TextKey::Sideloaded,
    TextKey::GitCheckout,
    TextKey::OfflineBundle,
    TextKey::LegacyUnmanaged,
    TextKey::LegacyAdoptionHelp,
    TextKey::Enabled,
    TextKey::Disabled,
    TextKey::Source,
    TextKey::Version,
    TextKey::NextVersion,
    TextKey::NoAddonsMatch,
    TextKey::AddonRepositories,
    TextKey::AvailableAddons,
    TextKey::InstalledAddons,
    TextKey::AddRepository,
    TextKey::RepositoryUrl,
    TextKey::PublicSigningKey,
    TextKey::DeploymentManagedRepositories,
    TextKey::LastRefresh,
    TextKey::NeverRefreshed,
    TextKey::ChooseFile,
    TextKey::NoFileSelected,
    TextKey::EditRepository,
    TextKey::OfficialRepositoryProtected,
    TextKey::UnknownSourcesRequired,
    TextKey::NoRepositories,
    TextKey::AddonPackage,
    TextKey::RepositoryPackage,
    TextKey::NoSigningKeyPinned,
    TextKey::Refresh,
    TextKey::Install,
    TextKey::UpdateAddon,
    TextKey::Uninstall,
    TextKey::Configuration,
    TextKey::SecretReference,
    TextKey::EnvironmentVariable,
    TextKey::SecretFile,
    TextKey::Incompatible,
    TextKey::PackageUnavailable,
    TextKey::Official,
    TextKey::ThirdParty,
    TextKey::Permissions,
    TextKey::Capabilities,
    TextKey::Dependencies,
    TextKey::Optional,
    TextKey::MediaReadAccess,
    TextKey::ExternalCommands,
    TextKey::ThirdPartyRepositoryWarning,
    TextKey::AddCustomResolution,
    TextKey::AddBlock,
    TextKey::AddBlockGroup,
    TextKey::AddBlockItem,
    TextKey::AddBlockToTemplate,
    TextKey::AddCollection,
    TextKey::AddContent,
    TextKey::AddFailed,
    TextKey::AddFillerPreset,
    TextKey::AddMultiCollection,
    TextKey::AddPlaylist,
    TextKey::AddPlaylistGroup,
    TextKey::AddPlaylistItem,
    TextKey::AddRerunCollection,
    TextKey::AddTemplate,
    TextKey::AddTemplateGroup,
    TextKey::AddAddonMediaList,
    TextKey::AddToCollection,
    TextKey::AddToEllipsis,
    TextKey::AddToPlaylist,
    TextKey::AddToSchedule,
    TextKey::Actions,
    TextKey::All,
    TextKey::AreYouSureDelete,
    TextKey::Actors,
    TextKey::AlbumArtist,
    TextKey::Apply,
    TextKey::Artist,
    TextKey::ArtistNotFound,
    TextKey::AfterDurationEnd,
    TextKey::AutomaticRefresh,
    TextKey::AddonDeliveryMode,
    TextKey::AddonDeliveryStream,
    TextKey::AddonDeliveryDownloadAhead,
    TextKey::AddonDeliveryNewVideos,
    TextKey::AddonDeliveryHelp,
    TextKey::AddonDeliveryUnavailable,
    TextKey::AddonDeliveryCutoff,
    TextKey::AddonDeliveryFreshCutoff,
    TextKey::AdaptiveRefreshUnavailable,
    TextKey::RefreshPublicationPolicy,
    TextKey::RefreshPublicationReview,
    TextKey::RefreshPublicationAutomatic,
    TextKey::RefreshPublicationPolicyHelp,
    TextKey::AutoPublicationHeld,
    TextKey::PreparationProcessRecoveryRequired,
    TextKey::PreparationWorkerUnavailable,
    TextKey::AutoPublicationBaselineUnavailable,
    TextKey::AutoPublicationExistingChanges,
    TextKey::AutoPublicationConflicts,
    TextKey::AutoPublicationRemovals,
    TextKey::AutoPublicationRestricted,
    TextKey::AutoPublicationUnavailable,
    TextKey::AutoPublicationOptionsChanged,
    TextKey::AutoPublicationPreparationFailed,
    TextKey::AutoPublicationConcurrentChange,
    TextKey::RefreshCadenceAdaptive,
    TextKey::RefreshCadenceDaily,
    TextKey::RefreshCadenceWeekly,
    TextKey::RefreshCadenceMonthly,
    TextKey::RefreshCadenceYearly,
    TextKey::RefreshCadenceNever,
    TextKey::LastSuccessfulRefresh,
    TextKey::NextScheduledRefresh,
    TextKey::RefreshRetryAt,
    TextKey::Balanced,
    TextKey::Back,
    TextKey::BackToSettings,
    TextKey::BackToArtists,
    TextKey::BackToCollections,
    TextKey::BackToMovies,
    TextKey::BackToOtherVideos,
    TextKey::BackToTvShows,
    TextKey::BeforeDurationEnd,
    TextKey::BackToTemplates,
    TextKey::Badge,
    TextKey::Block,
    TextKey::BlockGroup,
    TextKey::BlockCannotBeMoved,
    TextKey::BlockCopied,
    TextKey::BlockCreated,
    TextKey::BlockDeleted,
    TextKey::BlockGroupCreated,
    TextKey::BlockGroupDeleted,
    TextKey::BlockGroupName,
    TextKey::BlockGroups,
    TextKey::BlockItem,
    TextKey::BlockItems,
    TextKey::BlockName,
    TextKey::BlockNotFound,
    TextKey::BlockPreviewFailed,
    TextKey::Blocks,
    TextKey::SmartShuffleAfterDurationMinutes,
    TextKey::SmartShuffleAfterItems,
    TextKey::SmartShuffleFilterRules,
    TextKey::SmartShuffleIncludeFiller,
    TextKey::SmartShuffleInheritedBlock,
    TextKey::SmartShuffleInactive,
    TextKey::SmartShuffleOverrideBlock,
    TextKey::SmartShufflePreset,
    TextKey::SmartShuffleScope,
    TextKey::SmartShuffleScopeContentSource,
    TextKey::SmartShuffleScopePlayout,
    TextKey::SmartShuffleScopeScheduleItem,
    TextKey::SmartShuffleValidation,
    TextKey::ContentParent,
    TextKey::BlockToRemove,
    TextKey::BlockWillNotFit,
    TextKey::Cancel,
    TextKey::Clear,
    TextKey::Available,
    TextKey::UnknownAvailability,
    TextKey::LocalReplacement,
    TextKey::NoReplacement,
    TextKey::InvalidReplacement,
    TextKey::Channels,
    TextKey::Close,
    TextKey::Collections,
    TextKey::CollectionDeleted,
    TextKey::CollectionSaved,
    TextKey::SaveCollection,
    TextKey::ComponentGallery,
    TextKey::ComputingPreview,
    TextKey::Confirm,
    TextKey::ConfirmDelete,
    TextKey::CommentaryTracks,
    TextKey::Content,
    TextKey::ContentVariantAcoustic,
    TextKey::ContentVariantAlternateVersion,
    TextKey::ContentVariantBehindTheScenes,
    TextKey::ContentVariantDeletedScene,
    TextKey::ContentVariantDirectorsCut,
    TextKey::ContentVariantExtended,
    TextKey::ContentVariantExtra,
    TextKey::ContentVariantFeaturette,
    TextKey::ContentVariantInterview,
    TextKey::ContentVariantLive,
    TextKey::ContentVariantOfficialVideo,
    TextKey::ContentVariantOther,
    TextKey::ContentVariantRemaster,
    TextKey::ContentVariantRemix,
    TextKey::ContentVariantScene,
    TextKey::ContentVariantSearchHelp,
    TextKey::ContentVariantShort,
    TextKey::ContentVariantSpecialFeatures,
    TextKey::ContentVariantTrailer,
    TextKey::ContentVariantUnplugged,
    TextKey::ContentVariantVersions,
    TextKey::ContentRatings,
    TextKey::CustomResolutionHeight,
    TextKey::CustomResolutionWidth,
    TextKey::CustomResolutions,
    TextKey::Dark,
    TextKey::DaysToBuild,
    TextKey::AddBreakContent,
    TextKey::AddDeco,
    TextKey::AddDecoGroup,
    TextKey::AddDecoTemplate,
    TextKey::AddDecoTemplateGroup,
    TextKey::AtChapterMarkers,
    TextKey::BackToDecos,
    TextKey::BackToDecoTemplates,
    TextKey::BetweenBlockItems,
    TextKey::BlockFinish,
    TextKey::BlockStart,
    TextKey::BreakContent,
    TextKey::BreakContentBody,
    TextKey::BreakContentItem,
    TextKey::BreakContentMode,
    TextKey::DeadAirFallback,
    TextKey::DeadAirFallbackBody,
    TextKey::DeadAirFallbackCollectionType,
    TextKey::DeadAirFallbackMode,
    TextKey::Deco,
    TextKey::DecoCannotBeAdded,
    TextKey::DecoCannotBeMoved,
    TextKey::DecoCreated,
    TextKey::DecoDeleted,
    TextKey::DecoGroup,
    TextKey::DecoGroupCreated,
    TextKey::DecoGroupDeleted,
    TextKey::DecoGroupName,
    TextKey::DecoGroups,
    TextKey::DecoName,
    TextKey::DecoNotFound,
    TextKey::DecoSaved,
    TextKey::Decos,
    TextKey::DecoTemplate,
    TextKey::DecoTemplateCreated,
    TextKey::DecoTemplateDeleted,
    TextKey::DecoTemplateGroup,
    TextKey::DecoTemplateGroupCreated,
    TextKey::DecoTemplateGroupDeleted,
    TextKey::DecoTemplateGroupName,
    TextKey::DecoTemplateGroups,
    TextKey::DecoTemplateName,
    TextKey::DecoTemplateNotFound,
    TextKey::DecoTemplateSaved,
    TextKey::AddDecoToTemplate,
    TextKey::DecoToRemove,
    TextKey::DecoTemplates,
    TextKey::DefaultFiller,
    TextKey::DefaultFillerBody,
    TextKey::DefaultFillerCollectionType,
    TextKey::DefaultFillerMode,
    TextKey::DefaultFfmpegProfile,
    TextKey::DefaultFfmpegProfileHelp,
    TextKey::DefaultMinimumLogLevel,
    TextKey::DefaultMpegTsScript,
    TextKey::DefaultMpegTsScriptHelp,
    TextKey::Decorations,
    TextKey::Debug,
    TextKey::Delete,
    TextKey::DeleteAll,
    TextKey::DeleteBlock,
    TextKey::DeleteBlockGroup,
    TextKey::DeleteCustomResolution,
    TextKey::DeleteCollection,
    TextKey::DeleteDeco,
    TextKey::DeleteDecoGroup,
    TextKey::DeleteDecoTemplate,
    TextKey::DeleteDecoTemplateGroup,
    TextKey::DeleteFillerPreset,
    TextKey::DeleteMultiCollection,
    TextKey::DeletePlaylist,
    TextKey::DeletePlaylistGroup,
    TextKey::DeleteRerunCollection,
    TextKey::DeleteTemplate,
    TextKey::DeleteTemplateGroup,
    TextKey::DeleteAddonMediaList,
    TextKey::Disable,
    TextKey::DisabledCollectionsAlreadyPresent,
    TextKey::EmptyTrash,
    TextKey::ExtractEmbeddedSubtitles,
    TextKey::ErasedPlayoutItemsAndHistory,
    TextKey::EraseFailed,
    TextKey::EraseItemsAndHistory,
    TextKey::ErasePlayoutItemsAndHistoryConfirm,
    TextKey::Episode,
    TextKey::FanArt,
    TextKey::Ffmpeg,
    TextKey::FileNotFound,
    TextKey::Finish,
    TextKey::Fatal,
    TextKey::FfmpegPath,
    TextKey::FfmpegPathHelp,
    TextKey::FfmpegProfiles,
    TextKey::FfprobePath,
    TextKey::FfprobePathHelp,
    TextKey::FillerPreset,
    TextKey::FillerPresetActionFailed,
    TextKey::FillerPresetCouldNotBeSaved,
    TextKey::FillerPresetDeleted,
    TextKey::FillerPresetExpressionHelp,
    TextKey::FillerPresetSaved,
    TextKey::FillerPresets,
    TextKey::Flags,
    TextKey::GlobalFallbackFiller,
    TextKey::GlobalWatermark,
    TextKey::GoHome,
    TextKey::HdHomeRun,
    TextKey::HealthChecks,
    TextKey::Height,
    TextKey::Information,
    TextKey::HlsDirectOutputFormat,
    TextKey::HlsSegmenterIdleTimeout,
    TextKey::HlsSegmenterIdleTimeoutHelp,
    TextKey::HlsSegmenterInitialSegmentCount,
    TextKey::HlsSegmenterInitialSegmentCountHelp,
    TextKey::HomeRunning,
    TextKey::ReleaseNotes,
    TextKey::FullChangelog,
    TextKey::HttpMinimumLogLevel,
    TextKey::Id,
    TextKey::Images,
    TextKey::Item,
    TextKey::ItemActions,
    TextKey::ItemType,
    TextKey::Items,
    TextKey::Language,
    TextKey::Languages,
    TextKey::MusicLabels,
    TextKey::LeaveEmptyToAutoDetect,
    TextKey::Libraries,
    TextKey::LibraryRefreshInterval,
    TextKey::Light,
    TextKey::Lists,
    TextKey::Loading,
    TextKey::LoadingMembers,
    TextKey::Local,
    TextKey::Logging,
    TextKey::M3u,
    TextKey::Media,
    TextKey::MediaInfo,
    TextKey::MediaSources,
    TextKey::Movie,
    TextKey::MovieNotFound,
    TextKey::Movies,
    TextKey::MusicVideo,
    TextKey::MultiCollections,
    TextKey::ManualCollections,
    TextKey::MatchFailed,
    TextKey::MatchStatus,
    TextKey::MatchAddonMediaListItems,
    TextKey::Members,
    TextKey::Merge,
    TextKey::MultiCollection,
    TextKey::MultiCollectionDeleted,
    TextKey::MultiCollectionItems,
    TextKey::MultiCollectionNameRequired,
    TextKey::MultiCollectionNameTooLong,
    TextKey::MultiCollectionSaved,
    TextKey::Music,
    TextKey::Name,
    TextKey::NameRequired,
    TextKey::Networks,
    TextKey::NewCollection,
    TextKey::NewCollectionTitle,
    TextKey::NewFillerPreset,
    TextKey::NewMultiCollection,
    TextKey::NewTemplateGroup,
    TextKey::NewTemplateName,
    TextKey::NextPage,
    TextKey::Next24Hours,
    TextKey::NoFillerPresets,
    TextKey::NoBlockItems,
    TextKey::NoItems,
    TextKey::NoItemsToDisplay,
    TextKey::NoAddonMediaListsYet,
    TextKey::No,
    TextKey::NoCollections,
    TextKey::NoDecosFound,
    TextKey::NoDecoTemplatesFound,
    TextKey::NoMembers,
    TextKey::NoMultiCollections,
    TextKey::NoTemplatesFound,
    TextKey::NotFound,
    TextKey::OtherVideo,
    TextKey::OtherVideoNotFound,
    TextKey::OtherVideos,
    TextKey::OpenBrowser,
    TextKey::Playout,
    TextKey::PlayoutItemsAndHistory,
    TextKey::PlayoutNotFound,
    TextKey::Playouts,
    TextKey::PlayAll,
    TextKey::Playlist,
    TextKey::PlaylistAdded,
    TextKey::PlaylistDeleted,
    TextKey::PlaylistGroup,
    TextKey::PlaylistGroupAdded,
    TextKey::PlaylistGroupDeleted,
    TextKey::PlaylistGroupName,
    TextKey::PlaylistGroups,
    TextKey::PlaylistItem,
    TextKey::PlaylistItems,
    TextKey::PlaylistName,
    TextKey::PlaylistNotFound,
    TextKey::PlaylistPlayoutPreview,
    TextKey::PlaylistPreviewEmpty,
    TextKey::Playlists,
    TextKey::ProbeForInterlacedFrames,
    TextKey::PreferredAudioLanguage,
    TextKey::DefaultSubtitleLanguage,
    TextKey::FallbackSubtitleLanguage,
    TextKey::OriginalLanguageOption,
    TextKey::UndefinedLanguageOption,
    TextKey::Previous24Hours,
    TextKey::PreviousPage,
    TextKey::RerunCollections,
    TextKey::RerunCollection,
    TextKey::RerunCollectionActionFailed,
    TextKey::RerunCollectionDeleted,
    TextKey::RerunCollectionNameRequired,
    TextKey::RerunCollectionSaved,
    TextKey::RerunPlaybackOrder,
    TextKey::RemoteStreams,
    TextKey::RemoteStream,
    TextKey::PublishedOptions,
    TextKey::PlaybackHealth,
    TextKey::Configure,
    TextKey::Watch,
    TextKey::RemoteStreamPlaylists,
    TextKey::PlaylistUrl,
    TextKey::Extractor,
    TextKey::DestinationFolder,
    TextKey::SyncInterval,
    TextKey::MaxItems,
    TextKey::RemovedEntries,
    TextKey::CreateOrderedPlaylist,
    TextKey::SyncNow,
    TextKey::LastSync,
    TextKey::NeverSynced,
    TextKey::ManagedManifestReadOnly,
    TextKey::Truncated,
    TextKey::MissingDuration,
    TextKey::Orphaned,
    TextKey::Materialized,
    TextKey::State,
    TextKey::Upstream,
    TextKey::NewestFirst,
    TextKey::Keep,
    TextKey::ResetChannels,
    TextKey::RowsPerPage,
    TextKey::SaveSettings,
    TextKey::Save,
    TextKey::SaveMultiCollection,
    TextKey::SavePlaylist,
    TextKey::SaveTroubleshootingReportsToDisk,
    TextKey::SaveAsSmartCollection,
    TextKey::SearchQueryRequired,
    TextKey::Scanner,
    TextKey::ScanningMinimumLogLevel,
    TextKey::Scheduling,
    TextKey::SchedulingMinimumLogLevel,
    TextKey::StartWithWindows,
    TextKey::StartWithWindowsHelp,
    TextKey::Schedules,
    TextKey::AddSchedule,
    TextKey::AddScheduleItem,
    TextKey::AddSmartCollection,
    TextKey::Album,
    TextKey::Chronological,
    TextKey::CollectionSize,
    TextKey::CollectionType,
    TextKey::Count,
    TextKey::CopySchedule,
    TextKey::CopyScheduleItem,
    TextKey::CustomTitle,
    TextKey::DeleteSchedule,
    TextKey::DeleteScheduleItem,
    TextKey::DeleteSmartCollection,
    TextKey::DetailRowsPerPage,
    TextKey::Director,
    TextKey::Directors,
    TextKey::DiscardToFillAttempts,
    TextKey::Duration,
    TextKey::Dynamic,
    TextKey::EditProperties,
    TextKey::EditScheduleItems,
    TextKey::EditSmartCollection,
    TextKey::EnterNameForNewSchedule,
    TextKey::Fixed,
    TextKey::FixedStartTimeBehavior,
    TextKey::Flood,
    TextKey::Flexible,
    TextKey::FlexibleStartSoonAsPossible,
    TextKey::FlexibleStartSoonAsPossibleAfterStartTime,
    TextKey::FillWithGroupMode,
    TextKey::GraphicsElements,
    TextKey::AddGraphicsElement,
    TextKey::DrawtextRendererOption,
    TextKey::DrawtextSettings,
    TextKey::DrawvgRendererOption,
    TextKey::EditGraphicsElement,
    TextKey::FontFile,
    TextKey::FontFileOptionalHelp,
    TextKey::GraphicsElement,
    TextKey::GraphicsElementConfigInvalid,
    TextKey::GraphicsElementKindImage,
    TextKey::GraphicsElementKindMotion,
    TextKey::GraphicsElementKindScript,
    TextKey::GraphicsElementKindSubtitle,
    TextKey::GraphicsElementKindText,
    TextKey::GraphicsElementNameTooLong,
    TextKey::GraphicsElementNotEditable,
    TextKey::GraphicsElementNotFound,
    TextKey::GraphicsElementNotText,
    TextKey::GraphicsElementSaved,
    TextKey::GraphicsElementSaveFailed,
    TextKey::GraphicsElementType,
    TextKey::GraphicsExistingDrawvgHelp,
    TextKey::GraphicsStreamRestartHelp,
    TextKey::NoGraphicsElements,
    TextKey::RefreshPeriodBounds,
    TextKey::RefreshPeriodMilliseconds,
    TextKey::ReloadFrameInterval,
    TextKey::ReloadFrameIntervalBounds,
    TextKey::Renderer,
    TextKey::SaveGraphicsElement,
    TextKey::TextTemplate,
    TextKey::TextTemplateRequired,
    TextKey::TextTemplateTooLong,
    TextKey::GraphicsElementsMode,
    TextKey::GraphicsIds,
    TextKey::Genres,
    TextKey::GroupName,
    TextKey::GroupMultiPartEpisodesHelp,
    TextKey::GuideMode,
    TextKey::KeepMultiPartEpisodesTogether,
    TextKey::Marathon,
    TextKey::MarathonBatchSize,
    TextKey::MarathonBatchSizeHelp,
    TextKey::MarathonGroupBy,
    TextKey::MarathonShuffleGroups,
    TextKey::MarathonShuffleItems,
    TextKey::MidRollFiller,
    TextKey::MidRoll,
    TextKey::MoveDown,
    TextKey::MoveUp,
    TextKey::ReorderPlaylistItem,
    TextKey::MultiEpisodeGroupSize,
    TextKey::MultiEpisodeShuffle,
    TextKey::Multiple,
    TextKey::MultipleCount,
    TextKey::MultipleCountHelp,
    TextKey::MultipleMode,
    TextKey::NewScheduleName,
    TextKey::NewSmartCollection,
    TextKey::NoScheduleItems,
    TextKey::NoSchedules,
    TextKey::Normal,
    TextKey::Offline,
    TextKey::One,
    TextKey::OrderedGroups,
    TextKey::Playback,
    TextKey::PlaybackOrder,
    TextKey::PlaybackOrderUpdateFailed,
    TextKey::PlaybackOrderUpdated,
    TextKey::PlaylistItemSize,
    TextKey::PlayoutDurationHours,
    TextKey::PlayoutDurationMinutes,
    TextKey::PostRollFiller,
    TextKey::PostRoll,
    TextKey::PreRollFiller,
    TextKey::PreRoll,
    TextKey::Random,
    TextKey::RandomCount,
    TextKey::RandomRotation,
    TextKey::RemainingDurationHelp,
    TextKey::RandomStartPoint,
    TextKey::RemoveFailed,
    TextKey::RemovedFromCollection,
    TextKey::RemoveFromCollection,
    TextKey::Rename,
    TextKey::RenameFailed,
    TextKey::ReorderFailed,
    TextKey::RerunFirstRun,
    TextKey::RerunRerun,
    TextKey::ReadOnlyTwentyFourHourPlayoutView,
    TextKey::SaveSchedule,
    TextKey::SaveScheduleItems,
    TextKey::SaveSmartCollection,
    TextKey::Schedule,
    TextKey::ScheduleActionFailed,
    TextKey::ScheduleAsGroup,
    TextKey::ScheduleKind,
    TextKey::ScheduleKindBlock,
    TextKey::ScheduleKindClassic,
    TextKey::ScheduleKindExternalJson,
    TextKey::ScheduleKindScripted,
    TextKey::ScheduleKindSequential,
    TextKey::SchedulingContext,
    TextKey::ScheduleItem,
    TextKey::ScheduleItemActionFailed,
    TextKey::ScheduleItemFiller,
    TextKey::ScheduleItemFillerHelp,
    TextKey::ScheduleItemOverrides,
    TextKey::ScheduleItems,
    TextKey::ScheduleNotFound,
    TextKey::ScheduleSaveFailed,
    TextKey::SearchForDecos,
    TextKey::SearchForDecoTemplates,
    TextKey::SearchForSchedules,
    TextKey::SearchForManualCollections,
    TextKey::SearchForRerunCollections,
    TextKey::SearchForSmartCollections,
    TextKey::SearchForTemplates,
    TextKey::SearchForBlocks,
    TextKey::SearchForMultiCollections,
    TextKey::SearchQuery,
    TextKey::BlockSearchQueryPlaceholder,
    TextKey::SearchTitle,
    TextKey::SearchTitleDisplay,
    TextKey::SearchSmartCollection,
    TextKey::Season,
    TextKey::SeasonEpisode,
    TextKey::Unnumbered,
    TextKey::SeasonNotFound,
    TextKey::Select,
    TextKey::SelectCollection,
    TextKey::SelectGroupForNewTemplate,
    TextKey::SelectScheduleItemPrompt,
    TextKey::SelectSchedule,
    TextKey::SelectSmartCollection,
    TextKey::Shuffle,
    TextKey::ShuffleInOrder,
    TextKey::ShuffleScheduleItems,
    TextKey::ShuffleScheduleItemsHelp,
    TextKey::SimilarityRules,
    TextKey::SmartShuffle,
    TextKey::ShuffledGroups,
    TextKey::Show,
    TextKey::ShowNotFound,
    TextKey::Shows,
    TextKey::Start,
    TextKey::StartTime,
    TextKey::StartTimeOnOrAfter,
    TextKey::StartType,
    TextKey::Strict,
    TextKey::StrictAlwaysWaitExactStartTime,
    TextKey::StrictWaitExactStartTime,
    TextKey::SavedDecos,
    TextKey::SavedDecoTemplates,
    TextKey::TailFiller,
    TextKey::TailMode,
    TextKey::Tail,
    TextKey::TelevisionSeason,
    TextKey::TelevisionShow,
    TextKey::TreatCollectionsAsShows,
    TextKey::TreatCollectionsAsShowsHelp,
    TextKey::Type,
    TextKey::TypeToSearch,
    TextKey::Search,
    TextKey::Filters,
    TextKey::ActiveFilters,
    TextKey::ClearFilters,
    TextKey::CreateSmartCollection,
    TextKey::FromYear,
    TextKey::ToYear,
    TextKey::CollectionComposer,
    TextKey::CollectionDetails,
    TextKey::Rules,
    TextKey::Overview,
    TextKey::MatchAll,
    TextKey::MatchAllHelp,
    TextKey::MatchAny,
    TextKey::MatchAnyHelp,
    TextKey::ExcludeAny,
    TextKey::ExcludeAnyHelp,
    TextKey::AddRule,
    TextKey::EditRule,
    TextKey::RemoveRule,
    TextKey::RuleField,
    TextKey::Genre,
    TextKey::Studio,
    TextKey::Network,
    TextKey::Actor,
    TextKey::Writer,
    TextKey::SubtitleLanguage,
    TextKey::ShowTitle,
    TextKey::ContentKindLabel,
    TextKey::SubKindLabel,
    TextKey::HasCommentary,
    TextKey::AllFields,
    TextKey::MatchType,
    TextKey::MatchValue,
    TextKey::TextContains,
    TextKey::ExactPhrase,
    TextKey::Wildcard,
    TextKey::Fuzzy,
    TextKey::Range,
    TextKey::RangeStart,
    TextKey::RangeEnd,
    TextKey::NestedGroup,
    TextKey::NestedGroupHelp,
    TextKey::AddNestedGroup,
    TextKey::EditNestedGroup,
    TextKey::BackToRules,
    TextKey::BackToSearch,
    TextKey::CanonicalQuery,
    TextKey::CanonicalQueryHelp,
    TextKey::LivePreview,
    TextKey::MatchingItems,
    TextKey::QueryValid,
    TextKey::QueryInvalid,
    TextKey::SearchRuleValueRequired,
    TextKey::SearchGroupRequired,
    TextKey::SearchWildcardRequired,
    TextKey::SavedSearches,
    TextKey::SavedSearchName,
    TextKey::SavedSearchHelp,
    TextKey::ChooseSavedSearch,
    TextKey::LoadSavedSearch,
    TextKey::SaveSavedSearch,
    TextKey::UpdateSavedSearch,
    TextKey::NewSavedSearch,
    TextKey::DeleteSavedSearch,
    TextKey::SavedSearchSaved,
    TextKey::SavedSearchDeleted,
    TextKey::SavedSearchLoaded,
    TextKey::SavedSearchNameRequired,
    TextKey::SavedSearchNameTooLong,
    TextKey::SavedSearchNameUnique,
    TextKey::NoSavedSearches,
    TextKey::RefreshPreview,
    TextKey::NoRules,
    TextKey::SearchFiltersHelp,
    TextKey::SearchMedia,
    TextKey::SearchMoviesOrShows,
    TextKey::SearchAddonMediaList,
    TextKey::SearchingMinimumLogLevel,
    TextKey::Settings,
    TextKey::SettingsSaveFailed,
    TextKey::ShowMediaInfo,
    TextKey::ScriptedScheduleTimeout,
    TextKey::SkipMissingItems,
    TextKey::SkipMissingItemsHelp,
    TextKey::SmartCollections,
    TextKey::SmartCollection,
    TextKey::SmartCollectionDeleted,
    TextKey::SmartCollectionQueryPlaceholder,
    TextKey::SmartCollectionSaved,
    TextKey::SaveAddonMediaList,
    TextKey::Songs,
    TextKey::SplitTimeEvenly,
    TextKey::StreamingMinimumLogLevel,
    TextKey::Studios,
    TextKey::Support,
    TextKey::Template,
    TextKey::TemplateCopied,
    TextKey::TemplateCreated,
    TextKey::TemplateDeleted,
    TextKey::TemplateGroup,
    TextKey::TemplateGroupCreated,
    TextKey::TemplateGroupDeleted,
    TextKey::TemplateGroupName,
    TextKey::TemplateGroups,
    TextKey::TemplateName,
    TextKey::TemplateNotFound,
    TextKey::Templates,
    TextKey::Theme,
    TextKey::ThisActionCannotBeUndone,
    TextKey::ThisCollectionHasNoItemsYet,
    TextKey::Thumbnail,
    TextKey::Title,
    TextKey::ToggleNavigation,
    TextKey::TotalDurationHelp,
    TextKey::TotalPointsHelp,
    TextKey::TotalProgressHelp,
    TextKey::AddonMediaList,
    TextKey::AddonMediaListActionFailed,
    TextKey::AddonMediaListAdded,
    TextKey::AddonMediaListDeleted,
    TextKey::AddonMediaListMatched,
    TextKey::AddonMediaListNotFound,
    TextKey::AddonMediaListSaveFailed,
    TextKey::AddonMediaListSaved,
    TextKey::AddonMediaLists,
    TextKey::AddonToolMaintenance,
    TextKey::AddonToolMaintenanceNone,
    TextKey::AddonToolUpdatePermission,
    TextKey::AddonToolMaintenanceQueued,
    TextKey::AddonToolMaintenanceRunning,
    TextKey::AddonToolMaintenanceWaiting,
    TextKey::AddonToolUpdated,
    TextKey::AddonToolAlreadyCurrent,
    TextKey::AddonToolManualUpdate,
    TextKey::AddonToolManualHelp,
    TextKey::AddonToolFailedHelp,
    TextKey::AddonToolUpdateFailedStale,
    TextKey::AddonToolUpdateVersions,
    TextKey::AddonToolVersionHelp,
    TextKey::AddonToolUnavailableHelp,
    TextKey::AddonToolBusyHelp,
    TextKey::AddonToolCleanupHelp,
    TextKey::AddonListDiskWarnings,
    TextKey::AddonListDiskWarningGb,
    TextKey::AddonListsCombinedDiskWarningGb,
    TextKey::AddonListDiskWarningHelp,
    TextKey::AddonListDiskWarningFailed,
    TextKey::NextRuntimeAvailability,
    TextKey::NextOverlayInvalid,
    TextKey::NextOverlayIncomplete,
    TextKey::JavascriptAudioSelectionTitle,
    TextKey::JavascriptAudioSelectionHealthy,
    TextKey::JavascriptAudioSelectionFailures,
    TextKey::AddonListDiskUsageTitle,
    TextKey::AddonListDiskUsageBrief,
    TextKey::AddonListDiskUsageUnavailable,
    TextKey::AddonListDiskUsageUnavailableBrief,
    TextKey::AddonListDiskUsageLine,
    TextKey::AddonListsCombinedDiskUsageLine,
    TextKey::AddonMediaListUrl,
    TextKey::AddonMediaListFolder,
    TextKey::AddonMediaListFolderHelp,
    TextKey::DestructiveActions,
    TextKey::ReconcileEntireSource,
    TextKey::ReconcileEntireSourceHelp,
    TextKey::ReconcileQueued,
    TextKey::ReconcileFailed,
    TextKey::ResetAndReload,
    TextKey::ResetAddonMediaListHelp,
    TextKey::SourceHost,
    TextKey::ResetRemoves,
    TextKey::ResetRemovesItems,
    TextKey::ResetRemovesArtifacts,
    TextKey::ResetRemovesRuntime,
    TextKey::ResetRetains,
    TextKey::ResetRetainsSettings,
    TextKey::ResetReloadFailureWarning,
    TextKey::TypeListNameToConfirm,
    TextKey::ResetQueued,
    TextKey::ResetAddonMediaListFailed,
    TextKey::ChapterInput,
    TextKey::ChapterInputHelp,
    TextKey::RestoreProviderChapters,
    TextKey::ManagedByAddon,
    TextKey::Trash,
    TextKey::TrashAlreadyEmpty,
    TextKey::TrashEmpty,
    TextKey::TrashEmptyConfirm,
    TextKey::TrashEmptyConfirmHelp,
    TextKey::TrashSubtitle,
    TextKey::Troubleshooting,
    TextKey::TunerCount,
    TextKey::TvShows,
    TextKey::Ui,
    TextKey::Unavailable,
    TextKey::UseActualTimes,
    TextKey::UseCustomPlaybackOrder,
    TextKey::UseChaptersAsMediaItems,
    TextKey::UseChaptersAsMediaItemsHelp,
    TextKey::UseGraphicsElementsDuringFiller,
    TextKey::UseWatermarkDuringFiller,
    TextKey::UseEmbeddedSubtitles,
    TextKey::Uuid,
    TextKey::UntitledItem,
    TextKey::WindowsStartup,
    TextKey::WindowsStartupNeedsRepair,
    TextKey::WindowsStartupUnavailable,
    TextKey::ActiveTranscodeLimit,
    TextKey::ActiveTranscodeLimitHelp,
    TextKey::ActiveTranscodeSuggestionHardware,
    TextKey::ActiveTranscodeSuggestionSoftware,
    TextKey::Peak,
    TextKey::StreamsInUse,
    TextKey::SuggestedTranscodeLimit,
    TextKey::Watermarks,
    TextKey::Width,
    TextKey::WorkAheadSegmenterLimit,
    TextKey::WorkAheadSegmenterLimitHelp,
    TextKey::AccelerationKind,
    TextKey::AddFfmpegProfile,
    TextKey::AddProfile,
    TextKey::AllowBFrames,
    TextKey::AudioBitrate,
    TextKey::AudioBufferSize,
    TextKey::AudioChannels,
    TextKey::AudioFormat,
    TextKey::Audio,
    TextKey::AllowWatermarks,
    TextKey::BitDepth,
    TextKey::Copy,
    TextKey::CopyBlock,
    TextKey::CopyFailed,
    TextKey::CopyNameSuffix,
    TextKey::CopyProfile,
    TextKey::CopyTemplate,
    TextKey::Crop,
    TextKey::DeinterlaceVideo,
    TextKey::Edit,
    TextKey::EditBlock,
    TextKey::EditCollection,
    TextKey::EditDeco,
    TextKey::EditDecoTemplate,
    TextKey::EditFillerPreset,
    TextKey::EditFfmpegProfile,
    TextKey::EditMultiCollection,
    TextKey::EditPlaylist,
    TextKey::EditRerunCollection,
    TextKey::EditTemplate,
    TextKey::FillerKind,
    TextKey::General,
    TextKey::HardwareAcceleration,
    TextKey::HardwareIfPossible,
    TextKey::NewProfileName,
    TextKey::NoneOption,
    TextKey::NumHelp,
    TextKey::OptionalDisplayTitle,
    TextKey::TranscodeAudio,
    TextKey::NormalizeColors,
    TextKey::NormalizeFramerate,
    TextKey::NormalizeLoudness,
    TextKey::TranscodeVideo,
    TextKey::Off,
    TextKey::Pad,
    TextKey::PadNearestFifteenMinutes,
    TextKey::PadNearestFiveMinutes,
    TextKey::PadNearestTenMinutes,
    TextKey::PadNearestThirtyMinutes,
    TextKey::PadMode,
    TextKey::PadToNearestMinute,
    TextKey::PointHelp,
    TextKey::ProfileCopied,
    TextKey::ProfileNotFound,
    TextKey::QsvExtraHardwareFrames,
    TextKey::Resolution,
    TextKey::SampleRateKhz,
    TextKey::SaveDecoTemplate,
    TextKey::SaveProfile,
    TextKey::SaveFillerPreset,
    TextKey::SaveRerunCollection,
    TextKey::SaveSequentialSchedule,
    TextKey::SaveTemplate,
    TextKey::SaveBlock,
    TextKey::ScaleAndPad,
    TextKey::ScalingBehavior,
    TextKey::OutputDynamicRange,
    TextKey::OutputDynamicRangeCopyHint,
    TextKey::HdrStaticMetadataHint,
    TextKey::HdrMasteringDisplay,
    TextKey::HdrMaxCll,
    TextKey::HdrMaxFall,
    TextKey::Software,
    TextKey::Stretch,
    TextKey::TargetLoudnessLufs,
    TextKey::ThreadCountAuto,
    TextKey::TonemapAlgorithm,
    TextKey::VaapiDevicePath,
    TextKey::VaapiDisplay,
    TextKey::VaapiDriver,
    TextKey::Video,
    TextKey::VideoBitrate,
    TextKey::VideoBufferSize,
    TextKey::VideoFormat,
    TextKey::VideoPreset,
    TextKey::VideoTune,
    TextKey::VideoProfile,
    TextKey::ViewCollection,
    TextKey::ViewPlaylist,
    TextKey::AddAll,
    TextKey::AddAllToCollection,
    TextKey::AddAllToPlaylist,
    TextKey::AddedItemToPlaylist,
    TextKey::AddedArtistToSchedule,
    TextKey::AddedMovieToSchedule,
    TextKey::AddedSeasonToSchedule,
    TextKey::AddedShowToSchedule,
    TextKey::AddingItem,
    TextKey::AddingItemToPlaylist,
    TextKey::Artists,
    TextKey::ClearSelection,
    TextKey::Collection,
    TextKey::CollectionNameRequired,
    TextKey::CollectionNotFound,
    TextKey::CollectionRenamed,
    TextKey::EditAddonMediaListProperties,
    TextKey::EnterAddonMediaListUrl,
    TextKey::EnterSearchQuery,
    TextKey::Episodes,
    TextKey::LoadingCollections,
    TextKey::LoadingPlaylistGroups,
    TextKey::LoadingPlaylists,
    TextKey::GeneratePlaylist,
    TextKey::GeneratePlaylistFromSortedAddonMediaListItems,
    TextKey::MusicVideos,
    TextKey::NewCollectionName,
    TextKey::NoMediaMatched,
    TextKey::NoItemsToAdd,
    TextKey::NoItemsToPreview,
    TextKey::NoPlaylistsInGroup,
    TextKey::PleaseChoosePlaylist,
    TextKey::RequestFailed,
    TextKey::Released,
    TextKey::SaveAs,
    TextKey::Seasons,
    TextKey::SeeAll,
    TextKey::SelectAll,
    TextKey::SelectGroup,
    TextKey::SelectPlaylist,
    TextKey::SelectSearchCollectionHelp,
    TextKey::SelectSearchPlaylistHelp,
    TextKey::Selected,
    TextKey::SelectCollectionToAddThisMovie,
    TextKey::SelectPlaylistToAddThisMovie,
    TextKey::Add,
    TextKey::AddLibrary,
    TextKey::Address,
    TextKey::AddressRequired,
    TextKey::ApiKey,
    TextKey::ApiKeyRequired,
    TextKey::AuthorizedRedirecting,
    TextKey::CheckingEveryTwoSeconds,
    TextKey::ClickAuthorizePlex,
    TextKey::ClickPlayoutItemHelp,
    TextKey::ConfigureConnection,
    TextKey::Connection,
    TextKey::DeepScanCollections,
    TextKey::DeepScanLibrary,
    TextKey::DeepScan,
    TextKey::EditLibrary,
    TextKey::EnterOnePathPerLine,
    TextKey::Error,
    TextKey::Expression,
    TextKey::ExternalCollections,
    TextKey::Kind,
    TextKey::Library,
    TextKey::LibraryKind,
    TextKey::LibraryPaths,
    TextKey::LocalLibraries,
    TextKey::LocalPath,
    TextKey::MediaKind,
    TextKey::NoLibrariesConfigured,
    TextKey::NoPlayoutItemsInWindow,
    TextKey::NoPlayoutsFound,
    TextKey::NoSchedulingContext,
    TextKey::NoPathReplacements,
    TextKey::Paths,
    TextKey::PathReplacements,
    TextKey::PlexSignInHelp,
    TextKey::Remote,
    TextKey::RemotePath,
    TextKey::Remove,
    TextKey::RemoveBlockFromTemplate,
    TextKey::RemoveBreakContent,
    TextKey::RemoveContent,
    TextKey::RemoveDecoFromTemplate,
    TextKey::Retry,
    TextKey::Saved,
    TextKey::Saving,
    TextKey::QuickScan,
    TextKey::ScanCollections,
    TextKey::ScanFailed,
    TextKey::ScanLibrary,
    TextKey::ScanQueued,
    TextKey::Scanning,
    TextKey::Server,
    TextKey::ServerAddress,
    TextKey::SignInToPlex,
    TextKey::SignOutOfPlex,
    TextKey::Sync,
    TextKey::WaitingForPlexAuthorization,
    TextKey::AddChannel,
    TextKey::Any,
    TextKey::BlockPlayoutTroubleshooting,
    TextKey::BlockPlayoutHistory,
    TextKey::Categories,
    TextKey::CategoriesHelp,
    TextKey::Channel,
    TextKey::ChannelCouldNotBeSaved,
    TextKey::ChannelDisabled,
    TextKey::ChannelHiddenFromEpg,
    TextKey::ChannelLogo,
    TextKey::ChannelLogoPreview,
    TextKey::ChannelLogoUploadFailed,
    TextKey::Check,
    TextKey::Continuous,
    TextKey::DateAndTime,
    TextKey::DefaultOption,
    TextKey::DefaultSchedule,
    TextKey::DeleteChannel,
    TextKey::DownloadMediaSample,
    TextKey::DownloadResults,
    TextKey::DisableWatermarks,
    TextKey::EditChannel,
    TextKey::EditChannelNumbers,
    TextKey::EditNumbers,
    TextKey::ExternalLogoUrl,
    TextKey::Fail,
    TextKey::Fallback,
    TextKey::FallbackFiller,
    TextKey::FfmpegProfile,
    TextKey::FirstPage,
    TextKey::FirstRunPlaybackOrder,
    TextKey::Forced,
    TextKey::GenerateSubtitles,
    TextKey::Generated,
    TextKey::Details,
    TextKey::Full,
    TextKey::Group,
    TextKey::FfmpegRuntime,
    TextKey::FfmpegRuntimeDownloaded,
    TextKey::FfmpegRuntimeLatestRelease,
    TextKey::FfmpegRuntimeRelease,
    TextKey::FfmpegRuntimeSource,
    TextKey::FfmpegRuntimeSourceDownloaded,
    TextKey::FfmpegRuntimeSourceManaged,
    TextKey::FfmpegRuntimeSourceOverride,
    TextKey::FfmpegRuntimeSourcePath,
    TextKey::FfmpegRuntimeSourceUnknown,
    TextKey::FfmpegRuntimeUpdateAvailable,
    TextKey::FfmpegRuntimeUpdateCheckFailed,
    TextKey::FfmpegRuntimeUpdateFailed,
    TextKey::FfmpegRuntimeUpToDate,
    TextKey::FfmpegRuntimeUpdated,
    TextKey::FfmpegRuntimeVersion,
    TextKey::HealthNoPersistedRun,
    TextKey::HealthPersistedRunHelp,
    TextKey::HistoryDetails,
    TextKey::HlsDirect,
    TextKey::HlsSegmenter,
    TextKey::Hours,
    TextKey::IdleBehavior,
    TextKey::IdleBehaviorHelp,
    TextKey::Info,
    TextKey::Import,
    TextKey::IsEnabled,
    TextKey::Json,
    TextKey::KeepRunning,
    TextKey::Key,
    TextKey::LastMidFillerHelp,
    TextKey::LastPage,
    TextKey::Level,
    TextKey::LoadingMediaInfo,
    TextKey::Logs,
    TextKey::PlaybackSpeed,
    TextKey::Logo,
    TextKey::MediaItem,
    TextKey::Message,
    TextKey::MissingMediaItemQueryParameter,
    TextKey::Mirror,
    TextKey::MirrorOptionsHelp,
    TextKey::MirrorSourceChannel,
    TextKey::MusicVideoCreditsMode,
    TextKey::MusicVideoCreditsTemplate,
    TextKey::NoMatchingRecordsFound,
    TextKey::NoFfmpegProfiles,
    TextKey::NoBlockPlayoutSelected,
    TextKey::NoBlocks,
    TextKey::NoHistory,
    TextKey::NoMessages,
    TextKey::NvidiaCapabilitiesNotCollected,
    TextKey::Number,
    TextKey::OnDemand,
    TextKey::MatchedPointsHelp,
    TextKey::Packets,
    TextKey::Pass,
    TextKey::ParityDebt,
    TextKey::PlaybackTroubleshooting,
    TextKey::Play,
    TextKey::PlayoutTimeline,
    TextKey::PlayoutMode,
    TextKey::PlayoutModeHelp,
    TextKey::PlayoutOffset,
    TextKey::PlayoutSource,
    TextKey::PlayoutSourceHelp,
    TextKey::Preview,
    TextKey::PreviewBlockPlayout,
    TextKey::PreviewChannel,
    TextKey::ChannelPreview,
    TextKey::ReloadStream,
    TextKey::StopPreview,
    TextKey::OpenFullPlayer,
    TextKey::BackToChannels,
    TextKey::NowPlaying,
    TextKey::UpNext,
    TextKey::ProgrammeGuide,
    TextKey::EpgPreview,
    TextKey::EpgPreviewDescription,
    TextKey::EpgZoom,
    TextKey::EpgNow,
    TextKey::NoEpgChannels,
    TextKey::ScheduleGap,
    TextKey::Filler,
    TextKey::NoProgramming,
    TextKey::ChannelDisabledPreview,
    TextKey::ChannelNotAiring,
    TextKey::ChannelNoPlayout,
    TextKey::PreviewIdle,
    TextKey::PreviewConnecting,
    TextKey::PreviewLive,
    TextKey::PreviewPaused,
    TextKey::PreviewRecovering,
    TextKey::PreviewStopped,
    TextKey::PreviewUnsupported,
    TextKey::PreviewAuthorizationFailed,
    TextKey::PreviewUnavailable,
    TextKey::PreviewCapacityReached,
    TextKey::PreviewNetworkError,
    TextKey::PreviewMediaError,
    TextKey::PreviewDerivativeNote,
    TextKey::JavascriptRequiredPlayback,
    TextKey::ConfirmStopTranscodeSession,
    TextKey::PreviewPlaylistPlayout,
    TextKey::PreferredAudioTitle,
    TextKey::PreferredSubtitleLanguage,
    TextKey::QsvCapabilitiesNotCollected,
    TextKey::RemoveStoredLogo,
    TextKey::SaveChannel,
    TextKey::SaveChannels,
    TextKey::Second,
    TextKey::Seconds,
    TextKey::SelectProfile,
    TextKey::SequentialSchedule,
    TextKey::SequentialScheduleValidation,
    TextKey::SeekSeconds,
    TextKey::Schema,
    TextKey::ShowDisabled,
    TextKey::ShowInEpg,
    TextKey::SlugSeconds,
    TextKey::SlugSecondsHelp,
    TextKey::Song,
    TextKey::SongVideoMode,
    TextKey::SortById,
    TextKey::SortByLevel,
    TextKey::SortByName,
    TextKey::SortByTimestamp,
    TextKey::Stable,
    TextKey::StopOnDisconnect,
    TextKey::StabilityMeter,
    TextKey::StopScheduling,
    TextKey::StopSchedulingBlockItems,
    TextKey::StopTranscodeSession,
    TextKey::StartFromBeginning,
    TextKey::StreamSelector,
    TextKey::StreamSelectorMode,
    TextKey::StreamingEngine,
    TextKey::NextEngineTextSubtitleMode,
    TextKey::NextEngineTextSubtitleModeHelp,
    TextKey::InvalidNextEngineTextSubtitleMode,
    TextKey::SubtitleBurn,
    TextKey::SubtitleConvertIfPossible,
    TextKey::PreviewCaptions,
    TextKey::InvalidStreamingEngine,
    TextKey::StreamingMode,
    TextKey::Styles,
    TextKey::Subtitle,
    TextKey::SubtitleMode,
    TextKey::SubtitleModeHelp,
    TextKey::Timestamp,
    TextKey::Timeline,
    TextKey::Target,
    TextKey::Tags,
    TextKey::Tools,
    TextKey::TranscodeMode,
    TextKey::TranscodeModeHelp,
    TextKey::TroubleshootPlayback,
    TextKey::TroubleshootChannel,
    TextKey::TroubleshootingReports,
    TextKey::TroubleshootingPlaybackStartDebt,
    TextKey::UnableToLoadMediaInfo,
    TextKey::UnableToLoadPlaybackTroubleshooting,
    TextKey::UploadLogo,
    TextKey::UploadLogoHelp,
    TextKey::Warning,
    TextKey::Watermark,
    TextKey::WatermarkIds,
    TextKey::WatermarkMode,
    TextKey::Window,
    TextKey::WithProgress,
    TextKey::Writers,
    TextKey::Validate,
    TextKey::Yaml,
    TextKey::YamlFile,
    TextKey::Messages,
    TextKey::CollectionName,
    TextKey::MediaItemType,
    TextKey::MediaItemTitle,
    TextKey::ActualSize,
    TextKey::AddWatermark,
    TextKey::AspectRatioGuides,
    TextKey::BottomLeft,
    TextKey::BottomMiddle,
    TextKey::BottomRight,
    TextKey::CopyWatermark,
    TextKey::CopyOf,
    TextKey::CurrentImage,
    TextKey::Custom,
    TextKey::ChannelLogoPreviewUnavailable,
    TextKey::DeleteWatermark,
    TextKey::DurationSeconds,
    TextKey::DangerButton,
    TextKey::DeleteDialog,
    TextKey::Decrement,
    TextKey::EditWatermark,
    TextKey::EnableWatermarkToPreview,
    TextKey::DisabledButton,
    TextKey::Dialogs,
    TextKey::Frequency,
    TextKey::GeneratingPreview,
    TextKey::HorizontalMarginPercent,
    TextKey::GuideFourThree,
    TextKey::Identity,
    TextKey::Image,
    TextKey::ImageSource,
    TextKey::Intermittent,
    TextKey::IntermittentTiming,
    TextKey::LeftMiddle,
    TextKey::Location,
    TextKey::MiddleCenter,
    TextKey::Mode,
    TextKey::Minutes,
    TextKey::Moods,
    TextKey::ModalCancelled,
    TextKey::ModalConfirmed,
    TextKey::NewName,
    TextKey::NativeAssetDimensions,
    TextKey::NextWatermark,
    TextKey::Opacity,
    TextKey::OpacityExpression,
    TextKey::OpacityOneToHundred,
    TextKey::Permanent,
    TextKey::PerPass,
    TextKey::PlaceWithinSourceContent,
    TextKey::Placement,
    TextKey::PlacementPreview,
    TextKey::PreviewFailed,
    TextKey::PreviewReferenceFullHd,
    TextKey::PreviousWatermark,
    TextKey::RightMiddle,
    TextKey::SaveWatermark,
    TextKey::Scaled,
    TextKey::SelectCustomImageToPreview,
    TextKey::Size,
    TextKey::TopLeft,
    TextKey::TopMiddle,
    TextKey::TopRight,
    TextKey::UploadFailed,
    TextKey::UploadImage,
    TextKey::UnsavedChanges,
    TextKey::UnsavedWatermarkChangesMessage,
    TextKey::VerticalMarginPercent,
    TextKey::WatermarkCopied,
    TextKey::WatermarkNotFound,
    TextKey::WatermarkPlacementPreview,
    TextKey::WatermarkPreview,
    TextKey::WatermarkNavigation,
    TextKey::WidthPercent,
    TextKey::ZIndex,
    TextKey::DiscardChanges,
    TextKey::XmltvBlockBehavior,
    TextKey::XmltvTimeZone,
    TextKey::Xmltv,
    TextKey::Inherit,
    TextKey::Override,
    TextKey::Replace,
    TextKey::TrimToFit,
    TextKey::Yes,
    TextKey::Buttons,
    TextKey::CheckValue,
    TextKey::ChannelName,
    TextKey::ChannelNumber,
    TextKey::EnterName,
    TextKey::ErrorOccurred,
    TextKey::ErrorToast,
    TextKey::ExampleModal,
    TextKey::ExampleModalBody,
    TextKey::FormInputs,
    TextKey::Icons,
    TextKey::IncrementBadge,
    TextKey::InfoToast,
    TextKey::JustFyi,
    TextKey::LinkButton,
    TextKey::NumValue,
    TextKey::OpenModal,
    TextKey::OperationSucceeded,
    TextKey::PagedTableDemo,
    TextKey::PrimaryButton,
    TextKey::SecondaryButton,
    TextKey::SelectValue,
    TextKey::SomethingMightBeWrong,
    TextKey::SuccessToast,
    TextKey::TextButton,
    TextKey::TextValue,
    TextKey::Toasts,
    TextKey::WarningToast,
    TextKey::YamlScript,
    TextKey::AddPlayout,
    TextKey::ClassicScheduleMenu,
    TextKey::BlockScheduleMenu,
    TextKey::SequentialScheduleMenu,
    TextKey::ScriptedScheduleMenu,
    TextKey::JsonDizqueTvScheduleMenu,
    TextKey::ResetAllPlayouts,
    TextKey::PlayoutBuildQueued,
    TextKey::PlayoutBuildsQueued,
    TextKey::SearchForPlayouts,
    TextKey::PlayoutDetail,
    TextKey::ShowFiller,
    TextKey::EditPlayout,
    TextKey::RefreshPlayout,
    TextKey::ResetPlayout,
    TextKey::ScheduleReset,
    TextKey::SchedulePlayoutReset,
    TextKey::EditExternalJsonFile,
    TextKey::DeletePlayout,
    TextKey::Troubleshoot,
    TextKey::DailyResetTime,
    TextKey::DoNotAutomaticallyReset,
    TextKey::SaveChanges,
    TextKey::ExternalJsonFileLabel,
    TextKey::EditExternalJsonFilePrompt,
    TextKey::UnscheduledPlayoutItem,
    TextKey::DisabledChannelsAlreadyHavePlayout,
    TextKey::ProgramSchedule,
    TextKey::SequentialScheduleFileHelp,
    TextKey::BlockTemplatesAddedLater,
    TextKey::UnsupportedPlayoutKind,
    TextKey::ClassicPlayout,
    TextKey::ClassicPlayoutState,
    TextKey::ScanRefreshState,
    TextKey::SubtitleCleanup,
    TextKey::SubtitleCleanupMessage,
    TextKey::SubtitleCleanupBrief,
    TextKey::ArtworkOrphans,
    TextKey::ArtworkOrphansMessage,
    TextKey::ArtworkOrphansBrief,
    TextKey::ArtworkCardinality,
    TextKey::HlsLiveDelivery,
    TextKey::BlockPlayout,
    TextKey::ScriptedPlayout,
    TextKey::AlternateSchedules,
    TextKey::EditAlternateSchedules,
    TextKey::PlayoutTemplates,
    TextKey::EditTemplates,
    TextKey::DefaultDeco,
    TextKey::EnableDefaultDeco,
    TextKey::SaveDefaultDeco,
    TextKey::SaveScriptedSchedule,
    TextKey::EraseItems,
    TextKey::ErasedPlayoutItems,
    TextKey::SavedDefaultDeco,
    TextKey::SavedScriptedSchedule,
    TextKey::Maintenance,
    TextKey::SaveAlternateSchedules,
    TextKey::AddAlternateSchedule,
    TextKey::SaveTemplates,
    TextKey::PreviewCalendar,
    TextKey::EditContent,
    TextKey::PlayoutNestedCatchAllHelp,
    TextKey::PlayoutNestedPriorityHelp,
    TextKey::DaysOfWeek,
    TextKey::DaysOfMonth,
    TextKey::Months,
    TextKey::LimitToDateRange,
    TextKey::StartDay,
    TextKey::EndDay,
    TextKey::Enable,
    TextKey::Up,
    TextKey::Down,
    TextKey::Weekdays,
    TextKey::Weekends,
    TextKey::Sunday,
    TextKey::Monday,
    TextKey::Tuesday,
    TextKey::Wednesday,
    TextKey::Thursday,
    TextKey::Friday,
    TextKey::Saturday,
    TextKey::January,
    TextKey::February,
    TextKey::March,
    TextKey::April,
    TextKey::May,
    TextKey::June,
    TextKey::July,
    TextKey::August,
    TextKey::September,
    TextKey::October,
    TextKey::November,
    TextKey::December,
    TextKey::JanuaryShort,
    TextKey::FebruaryShort,
    TextKey::MarchShort,
    TextKey::AprilShort,
    TextKey::MayShort,
    TextKey::JuneShort,
    TextKey::JulyShort,
    TextKey::AugustShort,
    TextKey::SeptemberShort,
    TextKey::OctoberShort,
    TextKey::NovemberShort,
    TextKey::DecemberShort,
    TextKey::AnalysisJson,
    TextKey::ArchivePath,
    TextKey::Bitrate,
    TextKey::Completed,
    TextKey::ContinuityFindings,
    TextKey::Failed,
    TextKey::InternalMpegTs,
    TextKey::MpegTsTroubleshooting,
    TextKey::NotApplicable,
    TextKey::NotAvailable,
    TextKey::PcrFindings,
    TextKey::Pids,
    TextKey::PublicMpegTs,
    TextKey::RawSampleArchived,
    TextKey::Report,
    TextKey::RetainRawSample,
    TextKey::RunId,
    TextKey::SampleBytes,
    TextKey::SampleUrl,
    TextKey::Services,
    TextKey::StandardError,
    TextKey::StandardOutput,
    TextKey::Status,
    TextKey::Summary,
    TextKey::TsdDuckVersion,
];

pub fn selected_count(count: usize) -> String {
    languages_for(use_i18n()).selected_count(count)
}

pub fn item_count(count: u64) -> String {
    languages_for(use_i18n()).item_count(count)
}

pub fn range_of(start: u32, end: u32, total: u64) -> String {
    languages_for(use_i18n()).range_of(start, end, total)
}

pub fn playout_reset_prompt(channel: &str, schedule: &str) -> String {
    languages_for(use_i18n()).playout_reset_prompt(channel, schedule)
}

pub fn match_status(matched: u64, total: u64) -> String {
    languages_for(use_i18n()).match_status(matched, total)
}

pub fn rows_per_page(page_size: u32) -> String {
    languages_for(use_i18n()).rows_per_page(page_size)
}

pub fn page_of(page: u32, total: u32) -> String {
    languages_for(use_i18n()).page_of(page, total)
}

pub fn save_failed(error: impl std::fmt::Display) -> String {
    languages_for(use_i18n()).save_failed(&error)
}

pub fn delete_failed(error: impl std::fmt::Display) -> String {
    languages_for(use_i18n()).delete_failed(&error)
}

pub fn empty_trash_failed(error: impl std::fmt::Display) -> String {
    languages_for(use_i18n()).empty_trash_failed(&error)
}

pub fn search_count(total: u32, label: &'static str) -> String {
    languages_for(use_i18n()).search_count(total, label)
}

pub fn selected_items_count(count: usize) -> String {
    languages_for(use_i18n()).selected_items_count(count)
}

pub fn delete_selected_count(count: usize) -> String {
    languages_for(use_i18n()).delete_selected_count(count)
}

pub fn deleted_items_from_database(count: u64) -> String {
    languages_for(use_i18n()).deleted_items_from_database(count)
}

pub fn season_display_name(number: i64) -> String {
    languages_for(use_i18n()).season_display_name(number)
}

pub fn episode_display_subtitle(
    show_title: Option<&str>,
    season_number: Option<i64>,
    episode_number: Option<i64>,
) -> Option<String> {
    episode_display_subtitle_text(use_i18n(), show_title, season_number, episode_number)
}

pub fn episode_display_subtitle_text(
    language: UiLanguage,
    show_title: Option<&str>,
    season_number: Option<i64>,
    episode_number: Option<i64>,
) -> Option<String> {
    if !matches!(
        (season_number, episode_number),
        (Some(season), Some(episode)) if season >= 0 && episode > 0
    ) {
        let unnumbered = text(language, TextKey::Unnumbered);
        let show = show_title.unwrap_or_default().trim();
        return Some(if show.is_empty() {
            unnumbered.to_owned()
        } else {
            format!("{show} - {unnumbered}")
        });
    }
    languages_for(language).episode_display_subtitle(show_title, season_number, episode_number)
}

pub fn media_item_fallback_label(id: i64) -> String {
    languages_for(use_i18n()).media_item_fallback_label(id)
}

pub fn schedule_item_fallback_label(id: i64) -> String {
    languages_for(use_i18n()).schedule_item_fallback_label(id)
}

pub fn schedule_error_message(error: impl std::fmt::Display) -> String {
    let message = error.to_string();
    languages_for(use_i18n()).schedule_error_message(&message)
}

pub fn schedule_error_text(language: UiLanguage, error: &str) -> String {
    languages_for(language).schedule_error_message(error)
}

pub fn scheduling_content_error_message(error: impl std::fmt::Display) -> String {
    let message = error.to_string();
    languages_for(use_i18n()).scheduling_content_error_message(&message)
}

pub fn scheduling_content_error_text(language: UiLanguage, error: &str) -> String {
    languages_for(language).scheduling_content_error_message(error)
}

pub fn watermark_error_message(error: impl std::fmt::Display) -> String {
    let message = error.to_string();
    languages_for(use_i18n()).watermark_error_message(&message)
}

pub fn watermark_error_text(language: UiLanguage, error: &str) -> String {
    languages_for(language).watermark_error_message(error)
}

pub fn graphics_element_error_message(error: impl std::fmt::Display) -> String {
    let error = error.to_string();
    let key = if error.contains("name_required") {
        TextKey::NameRequired
    } else if error.contains("name_too_long") {
        TextKey::GraphicsElementNameTooLong
    } else if error.contains("text_required") {
        TextKey::TextTemplateRequired
    } else if error.contains("text_too_long") {
        TextKey::TextTemplateTooLong
    } else if error.contains("refresh_interval_out_of_range") {
        TextKey::RefreshPeriodBounds
    } else if error.contains("reload_interval_out_of_range") {
        TextKey::ReloadFrameIntervalBounds
    } else if error.contains("graphics_element_not_found") {
        TextKey::GraphicsElementNotFound
    } else if error.contains("graphics_element_not_text") {
        TextKey::GraphicsElementNotText
    } else if error.contains("graphics_element_config_invalid") {
        TextKey::GraphicsElementConfigInvalid
    } else {
        TextKey::GraphicsElementSaveFailed
    };
    tr(key).to_owned()
}

pub fn log_level_label(level: &str) -> String {
    log_level_text(use_i18n(), level)
}

pub fn log_level_text(language: UiLanguage, level: &str) -> String {
    let key = match level {
        "Debug" => TextKey::Debug,
        "Information" => TextKey::Information,
        "Warning" => TextKey::Warning,
        "Error" => TextKey::Error,
        "Fatal" => TextKey::Fatal,
        _ => return level.to_owned(),
    };

    text(language, key).to_owned()
}

pub fn channel_streaming_mode_label(mode: i32) -> String {
    languages_for(use_i18n()).channel_streaming_mode_label(mode)
}

pub fn channel_language_label(code: &str) -> String {
    languages_for(use_i18n()).channel_language_label(code)
}

pub fn media_kind_label(kind: i64) -> String {
    media_kind_text(use_i18n(), kind)
}

pub fn media_kind_text(language: UiLanguage, kind: i64) -> String {
    languages_for(language).media_kind_label(kind)
}

pub fn playout_schedule_kind_label(kind: i32) -> String {
    playout_schedule_kind_text(use_i18n(), kind)
}

pub fn playout_schedule_kind_text(language: UiLanguage, kind: i32) -> String {
    languages_for(language).playout_schedule_kind_label(kind)
}

pub fn library_kind_label(kind: &str) -> String {
    library_kind_text(use_i18n(), kind)
}

pub fn library_kind_text(language: UiLanguage, kind: &str) -> String {
    languages_for(language).library_kind_label(kind)
}

pub fn hardware_acceleration_option_text(
    language: UiLanguage,
    label: &str,
    supported: bool,
) -> String {
    languages_for(language).hardware_acceleration_option_label(label, supported)
}

pub fn hardware_acceleration_option_label(label: &str, supported: bool) -> String {
    hardware_acceleration_option_text(use_i18n(), label, supported)
}

pub fn ffmpeg_tonemap_algorithm_label(algorithm: i32) -> String {
    ffmpeg_tonemap_algorithm_text(use_i18n(), algorithm)
}

pub fn ffmpeg_tonemap_algorithm_text(language: UiLanguage, algorithm: i32) -> String {
    languages_for(language).ffmpeg_tonemap_algorithm_label(algorithm)
}

pub fn ffmpeg_output_dynamic_range_label(range: i32) -> String {
    ffmpeg_output_dynamic_range_text(use_i18n(), range)
}

pub fn ffmpeg_output_dynamic_range_text(language: UiLanguage, range: i32) -> String {
    languages_for(language).ffmpeg_output_dynamic_range_label(range)
}

pub fn ffmpeg_loudness_mode_label(mode: i32) -> String {
    ffmpeg_loudness_mode_text(use_i18n(), mode)
}

pub fn ffmpeg_loudness_mode_text(language: UiLanguage, mode: i32) -> String {
    languages_for(language).ffmpeg_loudness_mode_label(mode)
}

pub fn ffmpeg_profile_video_format_label(format: i32, hardware_acceleration: i32) -> String {
    ffmpeg_profile_video_format_text(use_i18n(), format, hardware_acceleration)
}

pub fn ffmpeg_profile_video_format_text(
    language: UiLanguage,
    format: i32,
    hardware_acceleration: i32,
) -> String {
    languages_for(language).ffmpeg_profile_video_format_label(format, hardware_acceleration)
}

pub fn ffmpeg_profile_audio_format_label(format: i32) -> String {
    ffmpeg_profile_audio_format_text(use_i18n(), format)
}

pub fn ffmpeg_profile_audio_format_text(language: UiLanguage, format: i32) -> String {
    languages_for(language).ffmpeg_profile_audio_format_label(format)
}

pub fn ffmpeg_profile_error_message(error: impl std::fmt::Display) -> String {
    let message = error.to_string();
    languages_for(use_i18n()).ffmpeg_profile_error_message(&message)
}

pub fn ffmpeg_profile_error_text(language: UiLanguage, error: &str) -> String {
    languages_for(language).ffmpeg_profile_error_message(error)
}

pub fn settings_error_text(language: UiLanguage, error: &str) -> String {
    languages_for(language).settings_error_message(error)
}

pub fn media_action_error_text(language: UiLanguage, error: &str) -> String {
    languages_for(language).media_action_error_message(error)
}

pub fn filler_error_message(error: impl std::fmt::Display) -> String {
    let message = error.to_string();
    languages_for(use_i18n()).filler_error_message(&message)
}

pub fn filler_error_text(language: UiLanguage, error: &str) -> String {
    languages_for(language).filler_error_message(error)
}

pub fn media_browser_error_message(error: impl std::fmt::Display) -> String {
    let message = error.to_string();
    languages_for(use_i18n()).media_browser_error_message(&message)
}

pub fn media_browser_error_text(language: UiLanguage, error: &str) -> String {
    languages_for(language).media_browser_error_message(error)
}

pub fn search_query_error_message(error: impl std::fmt::Display) -> String {
    search_query_error_text(use_i18n(), &error.to_string())
}

pub fn search_query_error_text(language: UiLanguage, error: &str) -> String {
    let message = unwrap_server_function_error(error);
    let key = if message.ends_with("search_query.required")
        || message.ends_with("search query is required")
    {
        TextKey::SearchQueryRequired
    } else if message.ends_with("search_query.rule_required")
        || message.ends_with("every search rule needs a field and value")
    {
        TextKey::SearchRuleValueRequired
    } else if message.ends_with("search_query.group_required") {
        TextKey::SearchGroupRequired
    } else if message.ends_with("search_query.wildcard_required") {
        TextKey::SearchWildcardRequired
    } else if message.ends_with("Saved search name is required") {
        TextKey::SavedSearchNameRequired
    } else if message.ends_with("Saved search name must be 80 characters or less") {
        TextKey::SavedSearchNameTooLong
    } else if message.ends_with("Saved search name must be unique") {
        TextKey::SavedSearchNameUnique
    } else {
        return languages_for(language).media_browser_error_message(&message);
    };
    languages_for(language).text(key).to_owned()
}

pub fn media_source_error_message(error: impl std::fmt::Display) -> String {
    let message = error.to_string();
    languages_for(use_i18n()).media_source_error_message(&message)
}

pub fn media_source_error_text(language: UiLanguage, error: &str) -> String {
    languages_for(language).media_source_error_message(error)
}

pub fn addon_media_list_error_message(error: impl std::fmt::Display) -> String {
    addon_media_list_error_text(use_i18n(), &error.to_string())
}

pub fn addon_media_list_error_text(language: UiLanguage, error: &str) -> String {
    let message = unwrap_server_function_error(error);
    if let Some(diagnostic) = parse_addon_operation_error(&message) {
        languages_for(language).addon_operation_error_message(&diagnostic.code, &diagnostic.message)
    } else {
        languages_for(language).addon_media_list_error_message(&message)
    }
}

fn unwrap_server_function_error(error: &str) -> String {
    // Server functions wrap a repository failure in transport plumbing, while
    // the localized messages match the repository wording.
    let error = error.trim();
    error
        .strip_prefix("error running server function: ")
        .unwrap_or(error)
        .to_owned()
}

fn parse_addon_operation_error(error: &str) -> Option<AddonOperationError> {
    let (_, json) = error.rsplit_once(ADDON_OPERATION_ERROR_MARKER)?;
    let diagnostic = serde_json::from_str::<AddonOperationError>(json.trim()).ok()?;
    diagnostic.validate().ok()?;
    Some(diagnostic)
}

pub fn upload_error_text(language: UiLanguage, error: &str) -> String {
    languages_for(language).upload_error_message(error)
}

pub fn emptied_trash_count(count: u64) -> String {
    languages_for(use_i18n()).emptied_trash_count(count)
}

pub fn select_collection_to_add_selected_items(count: usize) -> String {
    languages_for(use_i18n()).select_collection_to_add_selected_items(count)
}

pub fn select_playlist_to_add_selected_items(count: usize) -> String {
    languages_for(use_i18n()).select_playlist_to_add_selected_items(count)
}

pub fn added_items_to_collection(count: usize) -> String {
    languages_for(use_i18n()).added_items_to_collection(count)
}

pub fn added_items_to_playlist(count: usize) -> String {
    languages_for(use_i18n()).added_items_to_playlist(count)
}

pub fn add_to_collection_failed(error: impl std::fmt::Display) -> String {
    languages_for(use_i18n()).add_to_collection_failed(&error)
}

pub fn add_to_playlist_failed(error: impl std::fmt::Display) -> String {
    languages_for(use_i18n()).add_to_playlist_failed(&error)
}

pub fn no_media_source_configured(provider: &'static str) -> String {
    languages_for(use_i18n()).no_media_source_configured(provider)
}

pub fn provider_connection_title(provider: &'static str) -> String {
    languages_for(use_i18n()).provider_connection_title(provider)
}

pub fn provider_libraries_title(provider: &'static str) -> String {
    languages_for(use_i18n()).provider_libraries_title(provider)
}

pub fn provider_path_replacements_title(provider: &'static str) -> String {
    languages_for(use_i18n()).provider_path_replacements_title(provider)
}

pub fn provider_path_label(provider: &'static str) -> String {
    languages_for(use_i18n()).provider_path_label(provider)
}

pub fn health_summary(errors: u32, warnings: u32) -> String {
    languages_for(use_i18n()).health_summary(errors, warnings)
}

const CLASSIC_PLAYOUT_STATE_TITLE: &str = "Classic Playout State";
const SCAN_REFRESH_STATE_TITLE: &str = "Scan Refresh State";
const ARTWORK_CARDINALITY_TITLE: &str = "Artwork Cardinality";
const HLS_LIVE_DELIVERY_TITLE: &str = "HLS Live Delivery";
const ADDONS_HEALTH_TITLE: &str = "Add-ons";
const CLASSIC_PLAYOUT_STATE_MESSAGE_PREFIX: &str =
    "Classic playout state is not safe to Refresh for: ";
const CLASSIC_PLAYOUT_STATE_MESSAGE_SUFFIX: &str = ". Use Reset Playout for legacy or incompatible state; Refresh never clears a window without a complete checkpoint.";
const SCAN_REFRESH_STATE_MESSAGE_PREFIX: &str = "Scan refresh state needs attention: ";
const SCAN_REFRESH_UNRESOLVED_RESOLUTION_SEPARATOR: &str = " unresolved impact resolution(s), ";
const SCAN_REFRESH_STALE_RESOLUTION_SEPARATOR: &str = " stale impact resolution(s), ";
const SCAN_REFRESH_FAILED_INTENT_SEPARATOR: &str = " failed playout refresh intent(s), and ";
const SCAN_REFRESH_STALE_INTENT_SUFFIX: &str = " stale playout refresh intent(s). Review scanner and scheduling logs, then retry the affected scan or playout refresh.";
const ARTWORK_CARDINALITY_MESSAGE_PREFIX: &str = "Artwork cardinality needs attention: ";
const ARTWORK_CARDINALITY_SHOW_SEPARATOR: &str = " duplicate show artwork row(s), ";
const ARTWORK_CARDINALITY_SEASON_SEPARATOR: &str = " duplicate season artwork row(s), and ";
const ARTWORK_CARDINALITY_EPISODE_SEPARATOR: &str =
    " duplicate episode artwork row(s); projected television counts are ";
const ARTWORK_CARDINALITY_PUBLISHED_SEPARATOR: &str = " published add-on list episode item(s), ";
const ARTWORK_CARDINALITY_PROJECTION_SEPARATOR: &str = " projection row(s), ";
const ARTWORK_CARDINALITY_DISTINCT_SEPARATOR: &str = " distinct projected episode(s), ";
const ARTWORK_CARDINALITY_COMPLETE_SEPARATOR: &str = " complete hierarchy projection(s), and ";
const ARTWORK_CARDINALITY_HIERARCHY_SUFFIX: &str =
    " hierarchy episode(s). Refresh or republish the affected add-on television lists.";
const HLS_LIVE_DELIVERY_RECENT_SEPARATOR: &str =
    " channel(s) delivered live HLS media slower than wall time in the last ";
const HLS_LIVE_DELIVERY_ACTIVE_SEPARATOR: &str = " minutes; ";
const HLS_LIVE_DELIVERY_FALLBACK_SEPARATOR: &str = " had not recovered when last observed; ";
const HLS_LIVE_DELIVERY_FALLBACK_HOURS_SEPARATOR: &str =
    " channel(s) are using a verified software-encoding and/or lower-quality fallback for up to ";
const HLS_LIVE_DELIVERY_SUFFIX: &str = " hours. Review the streaming logs and run the delivery-aware TSDuck playback gate before changing provider settings.";

pub fn health_check_title(title: &str) -> String {
    health_check_title_text(use_i18n(), title)
}

pub fn health_check_title_text(language: UiLanguage, title: &str) -> String {
    match title {
        CLASSIC_PLAYOUT_STATE_TITLE => text(language, TextKey::ClassicPlayoutState).to_owned(),
        SCAN_REFRESH_STATE_TITLE => text(language, TextKey::ScanRefreshState).to_owned(),
        "Subtitle Cleanup" => text(language, TextKey::SubtitleCleanup).to_owned(),
        "Artwork Orphans" => text(language, TextKey::ArtworkOrphans).to_owned(),
        ARTWORK_CARDINALITY_TITLE => text(language, TextKey::ArtworkCardinality).to_owned(),
        HLS_LIVE_DELIVERY_TITLE => text(language, TextKey::HlsLiveDelivery).to_owned(),
        ADDONS_HEALTH_TITLE => text(language, TextKey::Addons).to_owned(),
        "Next Runtime Availability" => text(language, TextKey::NextRuntimeAvailability).to_owned(),
        "JavaScript Audio Selection" => {
            text(language, TextKey::JavascriptAudioSelectionTitle).to_owned()
        }
        "Add-on list disk usage" => text(language, TextKey::AddonListDiskUsageTitle).to_owned(),
        _ => title.to_owned(),
    }
}

pub fn health_check_message(title: &str, message: &str, brief_message: &str) -> String {
    health_check_message_text(use_i18n(), title, message, brief_message)
}

/// Offline recovery command template; the operator supplies the configured path.
pub const ADDON_TOOL_OFFLINE_RECOVERY_COMMAND: &str = "./ersatzrs-web --recover-addon-tool --tool-path \"<absolute-executable-path>\" --confirm-all-processes-stopped";

/// Static remediation for bounded maintenance diagnostics, never provider prose.
pub fn addon_tool_maintenance_diagnostic(code: Option<&str>) -> Option<TextKey> {
    code.map(|code| match code {
        "unsupported_installation" => TextKey::AddonToolManualHelp,
        "version_probe_failed" => TextKey::AddonToolVersionHelp,
        "tool_unavailable" => TextKey::AddonToolUnavailableHelp,
        "tool-busy" => TextKey::AddonToolBusyHelp,
        "tool-cleanup-required" => TextKey::AddonToolCleanupHelp,
        _ => TextKey::AddonToolFailedHelp,
    })
}

/// Localize a provider-owned readiness warning without inspecting provider prose.
pub fn addon_check_message(
    code: Option<&str>,
    message: &str,
    tool_update: Option<&ersatzrs_addon_contract::AddonToolUpdateWarning>,
) -> String {
    addon_check_message_text(use_i18n(), code, message, tool_update)
}

fn addon_check_message_text(
    language: UiLanguage,
    code: Option<&str>,
    message: &str,
    tool_update: Option<&ersatzrs_addon_contract::AddonToolUpdateWarning>,
) -> String {
    if code == Some("tool-update-failed-stale") {
        let warning = text(language, TextKey::AddonToolUpdateFailedStale);
        match tool_update.filter(|details| details.validate().is_ok()) {
            Some(details) => format!(
                "{} {warning}",
                text(language, TextKey::AddonToolUpdateVersions)
                    .replace("{program}", &details.program)
                    .replace("{installed}", &details.installed_version)
                    .replace("{latest}", &details.latest_version)
            ),
            None => warning.to_owned(),
        }
    } else {
        message.to_owned()
    }
}

fn addon_list_disk_usage_line(language: UiLanguage, line: &str) -> Option<String> {
    if line
        == "Add-on list disk usage could not be measured completely. Check storage access and retry the health check."
    {
        return Some(text(language, TextKey::AddonListDiskUsageUnavailable).to_owned());
    }
    if line == "Add-on list disk usage could not be measured." {
        return Some(text(language, TextKey::AddonListDiskUsageUnavailableBrief).to_owned());
    }
    let (scope, usage) = line.split_once(": ")?;
    let (used, limit) = usage
        .strip_suffix(" GB.")?
        .split_once(" GB used; warning at ")?;
    // Only the persisted check's numeric wire shape is interpolated. Unknown
    // future diagnostic text retains the same untranslated fallback as other checks.
    let (whole, fraction) = used.split_once('.')?;
    whole.parse::<u64>().ok()?;
    if fraction.len() != 3 || !fraction.bytes().all(|byte| byte.is_ascii_digit()) {
        return None;
    }
    limit.parse::<u32>().ok()?;
    let (key, id) = if scope == "Combined" {
        (TextKey::AddonListsCombinedDiskUsageLine, None)
    } else {
        let id = scope.strip_prefix("List #")?.parse::<i64>().ok()?;
        if id <= 0 {
            return None;
        }
        (TextKey::AddonListDiskUsageLine, Some(id))
    };
    let localized = text(language, key)
        .replace("{used}", used)
        .replace("{limit}", limit);
    Some(id.map_or_else(
        || localized.clone(),
        |id| localized.replace("{id}", &id.to_string()),
    ))
}

fn tool_maintenance_health_message(language: UiLanguage, message: &str) -> String {
    message
        .split("; ")
        .map(|part| {
            // This is host-authored framing around validated structured fields,
            // never a parser for provider diagnostic prose.
            if let Some((addon, details)) =
                part.split_once(" last reported warning readiness (tool-update-failed-stale): ")
            {
                let details =
                    serde_json::from_str::<ersatzrs_addon_contract::AddonToolUpdateWarning>(
                        details,
                    )
                    .ok();
                return format!(
                    "{addon}: {}",
                    addon_check_message_text(
                        language,
                        Some("tool-update-failed-stale"),
                        "",
                        details.as_ref()
                    )
                );
            }
            if let Some(addon) =
                part.strip_suffix(" last reported warning readiness (tool-update-failed-stale)")
            {
                return format!(
                    "{addon}: {}",
                    text(language, TextKey::AddonToolUpdateFailedStale)
                );
            }
            let Some((tool, code)) = part
                .strip_prefix("Tool maintenance ")
                .and_then(|value| value.split_once(": "))
            else {
                return part.to_owned();
            };
            let key = addon_tool_maintenance_diagnostic(Some(code))
                .unwrap_or(TextKey::AddonToolFailedHelp);
            let instruction = format!(
                "{} {tool}: {}",
                text(language, TextKey::AddonToolMaintenance),
                text(language, key)
            );
            if code == "tool-cleanup-required" {
                format!("{instruction}\n{ADDON_TOOL_OFFLINE_RECOVERY_COMMAND}")
            } else {
                instruction
            }
        })
        .collect::<Vec<_>>()
        .join("; ")
}

fn javascript_audio_health_message(language: UiLanguage, message: &str, brief: &str) -> String {
    if brief == "No selector failures recorded" {
        return text(language, TextKey::JavascriptAudioSelectionHealthy).to_owned();
    }
    message
        .split_whitespace()
        .next()
        .and_then(|value| value.parse::<u64>().ok())
        .map_or_else(
            || message.to_owned(),
            |count| {
                text(language, TextKey::JavascriptAudioSelectionFailures)
                    .replace("{count}", &count.to_string())
            },
        )
}

pub fn health_check_message_text(
    language: UiLanguage,
    title: &str,
    message: &str,
    brief_message: &str,
) -> String {
    let displayed_message = if message.is_empty() {
        brief_message
    } else {
        message
    };
    if title == "Next Runtime Availability" {
        let key = match displayed_message {
            "Next configuration overlay is unavailable or invalid" => {
                Some(TextKey::NextOverlayInvalid)
            }
            "Next configuration overlay validation is incomplete" => {
                Some(TextKey::NextOverlayIncomplete)
            }
            _ => None,
        };
        if let Some(key) = key {
            return text(language, key).to_owned();
        }
    }
    if title == "JavaScript Audio Selection" {
        return javascript_audio_health_message(language, displayed_message, brief_message);
    }
    if title == ADDONS_HEALTH_TITLE {
        return tool_maintenance_health_message(language, displayed_message);
    }
    if title == "Add-on list disk usage" {
        if displayed_message == "Add-on list disk usage warning" {
            return text(language, TextKey::AddonListDiskUsageBrief).to_owned();
        }
        if let Some(lines) = displayed_message
            .lines()
            .map(|line| addon_list_disk_usage_line(language, line))
            .collect::<Option<Vec<_>>>()
        {
            return lines.join("\n");
        }
    }
    if title == "Add-on media lists"
        && matches!(
            displayed_message,
            "Automatic publication needs review" | "Automatic processing needs attention"
        )
    {
        return text(language, TextKey::AutoPublicationHeld).to_owned();
    }
    if title == "Add-on media lists"
        && displayed_message
            == "Media preparation needs process recovery. Confirm prior provider processes have stopped before recovering retained storage and retrying preparation."
    {
        return text(language, TextKey::PreparationProcessRecoveryRequired).to_owned();
    }
    if title == "Add-on media lists"
        && displayed_message
            == "Media preparation worker is unavailable. Restart the server before retrying preparation."
    {
        return text(language, TextKey::PreparationWorkerUnavailable).to_owned();
    }
    if title == "Add-on media lists" && matches!(displayed_message,
        "Derived show metadata reached its size limit" |
        "Some episode facets exceed the bounded show metadata union. Episode metadata remains complete.") {
        return text(language, TextKey::AddonDerivedMetadataLimited).to_owned();
    }
    if title == CLASSIC_PLAYOUT_STATE_TITLE {
        if let Some(playouts) = displayed_message
            .strip_prefix(CLASSIC_PLAYOUT_STATE_MESSAGE_PREFIX)
            .and_then(|value| value.strip_suffix(CLASSIC_PLAYOUT_STATE_MESSAGE_SUFFIX))
        {
            return languages_for(language).classic_playout_state_message(playouts);
        }
    }
    if title == SCAN_REFRESH_STATE_TITLE {
        if let Some((unresolved_resolutions, stale_resolutions, failed_intents, stale_intents)) =
            scan_refresh_state_counts(displayed_message)
        {
            return languages_for(language).scan_refresh_state_message(
                unresolved_resolutions,
                stale_resolutions,
                failed_intents,
                stale_intents,
            );
        }
    }
    if let Some(localized) = artwork_health_message(language, title, message, brief_message) {
        return localized;
    }
    if title == HLS_LIVE_DELIVERY_TITLE {
        if let Some((recent_under_runs, minutes, active_under_runs, fallbacks, fallback_hours)) =
            hls_live_delivery_counts(displayed_message)
        {
            return languages_for(language).hls_live_delivery_message(
                recent_under_runs,
                minutes,
                active_under_runs,
                fallbacks,
                fallback_hours,
            );
        }
    }
    displayed_message.to_owned()
}

fn artwork_health_message(
    language: UiLanguage,
    title: &str,
    message: &str,
    brief_message: &str,
) -> Option<String> {
    if title == ARTWORK_CARDINALITY_TITLE {
        let displayed = if message.is_empty() {
            brief_message
        } else {
            message
        };
        let counts = artwork_cardinality_counts(displayed)?;
        return Some(languages_for(language).artwork_cardinality_message(
            [
                counts.duplicate_show_rows,
                counts.duplicate_season_rows,
                counts.duplicate_episode_rows,
            ],
            [
                counts.published_items,
                counts.projection_rows,
                counts.distinct_episodes,
                counts.complete_hierarchy_rows,
                counts.hierarchy_episodes,
            ],
        ));
    }
    if title == "Subtitle Cleanup" {
        let count = brief_message
            .strip_suffix(" subtitle row(s) awaiting cleanup")?
            .parse::<u64>()
            .ok()?;
        let key = if message.is_empty() {
            TextKey::SubtitleCleanupBrief
        } else {
            TextKey::SubtitleCleanupMessage
        };
        return Some(text(language, key).replace("{count}", &count.to_string()));
    }
    if title != "Artwork Orphans" {
        return None;
    }
    let (actors, artwork) = brief_message.split_once(" orphan actor row(s) and ")?;
    let actors = actors.parse::<u64>().ok()?;
    let artwork = artwork
        .strip_suffix(" orphan artwork row(s)")?
        .parse::<u64>()
        .ok()?;
    let key = if message.is_empty() {
        TextKey::ArtworkOrphansBrief
    } else {
        TextKey::ArtworkOrphansMessage
    };
    Some(
        text(language, key)
            .replace("{actors}", &actors.to_string())
            .replace("{artwork}", &artwork.to_string()),
    )
}

fn hls_live_delivery_counts(message: &str) -> Option<(u64, u64, u64, u64, u64)> {
    let (recent_under_runs, message) = message.split_once(HLS_LIVE_DELIVERY_RECENT_SEPARATOR)?;
    let (minutes, message) = message.split_once(HLS_LIVE_DELIVERY_ACTIVE_SEPARATOR)?;
    let (active_under_runs, message) = message.split_once(HLS_LIVE_DELIVERY_FALLBACK_SEPARATOR)?;
    let (fallbacks, message) = message.split_once(HLS_LIVE_DELIVERY_FALLBACK_HOURS_SEPARATOR)?;
    let fallback_hours = message.strip_suffix(HLS_LIVE_DELIVERY_SUFFIX)?;
    Some((
        recent_under_runs.parse().ok()?,
        minutes.parse().ok()?,
        active_under_runs.parse().ok()?,
        fallbacks.parse().ok()?,
        fallback_hours.parse().ok()?,
    ))
}

fn scan_refresh_state_counts(message: &str) -> Option<(u64, u64, u64, u64)> {
    let message = message.strip_prefix(SCAN_REFRESH_STATE_MESSAGE_PREFIX)?;
    let (unresolved_resolutions, message) =
        message.split_once(SCAN_REFRESH_UNRESOLVED_RESOLUTION_SEPARATOR)?;
    let (stale_resolutions, message) =
        message.split_once(SCAN_REFRESH_STALE_RESOLUTION_SEPARATOR)?;
    let (failed_intents, message) = message.split_once(SCAN_REFRESH_FAILED_INTENT_SEPARATOR)?;
    let stale_intents = message.strip_suffix(SCAN_REFRESH_STALE_INTENT_SUFFIX)?;
    Some((
        unresolved_resolutions.parse().ok()?,
        stale_resolutions.parse().ok()?,
        failed_intents.parse().ok()?,
        stale_intents.parse().ok()?,
    ))
}

struct ArtworkCardinalityMessageCounts {
    duplicate_show_rows: i64,
    duplicate_season_rows: i64,
    duplicate_episode_rows: i64,
    published_items: i64,
    projection_rows: i64,
    distinct_episodes: i64,
    complete_hierarchy_rows: i64,
    hierarchy_episodes: i64,
}

fn artwork_cardinality_counts(message: &str) -> Option<ArtworkCardinalityMessageCounts> {
    let message = message.strip_prefix(ARTWORK_CARDINALITY_MESSAGE_PREFIX)?;
    let (duplicate_show_rows, message) = message.split_once(ARTWORK_CARDINALITY_SHOW_SEPARATOR)?;
    let (duplicate_season_rows, message) =
        message.split_once(ARTWORK_CARDINALITY_SEASON_SEPARATOR)?;
    let (duplicate_episode_rows, message) =
        message.split_once(ARTWORK_CARDINALITY_EPISODE_SEPARATOR)?;
    let (published_items, message) = message.split_once(ARTWORK_CARDINALITY_PUBLISHED_SEPARATOR)?;
    let (projection_rows, message) =
        message.split_once(ARTWORK_CARDINALITY_PROJECTION_SEPARATOR)?;
    let (distinct_episodes, message) =
        message.split_once(ARTWORK_CARDINALITY_DISTINCT_SEPARATOR)?;
    let (complete_hierarchy_rows, message) =
        message.split_once(ARTWORK_CARDINALITY_COMPLETE_SEPARATOR)?;
    let hierarchy_episodes = message.strip_suffix(ARTWORK_CARDINALITY_HIERARCHY_SUFFIX)?;
    Some(ArtworkCardinalityMessageCounts {
        duplicate_show_rows: duplicate_show_rows.parse().ok()?,
        duplicate_season_rows: duplicate_season_rows.parse().ok()?,
        duplicate_episode_rows: duplicate_episode_rows.parse().ok()?,
        published_items: published_items.parse().ok()?,
        projection_rows: projection_rows.parse().ok()?,
        distinct_episodes: distinct_episodes.parse().ok()?,
        complete_hierarchy_rows: complete_hierarchy_rows.parse().ok()?,
        hierarchy_episodes: hierarchy_episodes.parse().ok()?,
    })
}

pub fn unable_to_load_playback_troubleshooting(error: impl std::fmt::Display) -> String {
    languages_for(use_i18n()).unable_to_load_playback_troubleshooting(&error)
}

pub fn provide_i18n_context() {
    provide_context(load_language_initial());
}

pub fn use_i18n() -> UiLanguage {
    use_context::<UiLanguage>().unwrap_or(UiLanguage::English)
}

pub fn tr(key: TextKey) -> &'static str {
    text(use_i18n(), key)
}

pub fn content_variant_badge_label(
    content_kind: ersatzrs_domain::ContentKind,
    sub_kind: Option<&str>,
) -> Option<String> {
    let prefix = match content_kind {
        ersatzrs_domain::ContentKind::Primary => return None,
        ersatzrs_domain::ContentKind::Extra => tr(TextKey::ContentVariantExtra),
        ersatzrs_domain::ContentKind::AlternateVersion => {
            tr(TextKey::ContentVariantAlternateVersion)
        }
    };
    let Some(sub_kind) = sub_kind
        .and_then(ersatzrs_domain::normalize_sub_kind_token)
        .filter(|value| !value.trim().is_empty())
    else {
        return Some(prefix.to_owned());
    };
    Some(format!(
        "{prefix}: {}",
        content_variant_sub_kind_label(&sub_kind)
    ))
}

pub fn content_variant_sub_kind_label(token: &str) -> String {
    let key = match token {
        "trailer" => Some(TextKey::ContentVariantTrailer),
        "behind_the_scenes" => Some(TextKey::ContentVariantBehindTheScenes),
        "deleted_scene" => Some(TextKey::ContentVariantDeletedScene),
        "featurette" => Some(TextKey::ContentVariantFeaturette),
        "interview" => Some(TextKey::ContentVariantInterview),
        "scene" => Some(TextKey::ContentVariantScene),
        "short" => Some(TextKey::ContentVariantShort),
        "other" => Some(TextKey::ContentVariantOther),
        "official_video" => Some(TextKey::ContentVariantOfficialVideo),
        "live" => Some(TextKey::ContentVariantLive),
        "unplugged" => Some(TextKey::ContentVariantUnplugged),
        "acoustic" => Some(TextKey::ContentVariantAcoustic),
        "remix" => Some(TextKey::ContentVariantRemix),
        "remaster" => Some(TextKey::ContentVariantRemaster),
        "extended" => Some(TextKey::ContentVariantExtended),
        "directors_cut" => Some(TextKey::ContentVariantDirectorsCut),
        _ => None,
    };
    key.map_or_else(|| token.replace('_', " "), |key| tr(key).to_owned())
}

pub fn text(language: UiLanguage, key: TextKey) -> &'static str {
    languages_for(language).text(key)
}

fn languages_for(language: UiLanguage) -> &'static dyn LanguageStrings {
    language_definition_for(language).strings
}

fn language_definition_for(language: UiLanguage) -> &'static LanguageDefinition {
    crate::admin::languages::LANGUAGES
        .iter()
        .find(|definition| definition.code == language.code())
        .unwrap_or(&crate::admin::languages::LANGUAGES[0])
}

pub trait LanguageStrings: Sync {
    fn text(&self, key: TextKey) -> &'static str;
    fn selected_count(&self, count: usize) -> String;
    fn item_count(&self, count: u64) -> String;
    fn range_of(&self, start: u32, end: u32, total: u64) -> String;
    fn match_status(&self, matched: u64, total: u64) -> String;
    fn rows_per_page(&self, page_size: u32) -> String;
    fn page_of(&self, page: u32, total: u32) -> String;
    fn save_failed(&self, error: &dyn std::fmt::Display) -> String;
    fn delete_failed(&self, error: &dyn std::fmt::Display) -> String;
    fn empty_trash_failed(&self, error: &dyn std::fmt::Display) -> String;
    fn search_count(&self, total: u32, label: &'static str) -> String;
    fn selected_items_count(&self, count: usize) -> String;
    fn delete_selected_count(&self, count: usize) -> String;
    fn deleted_items_from_database(&self, count: u64) -> String;
    fn season_display_name(&self, number: i64) -> String;
    fn episode_display_subtitle(
        &self,
        show_title: Option<&str>,
        season_number: Option<i64>,
        episode_number: Option<i64>,
    ) -> Option<String>;
    fn media_item_fallback_label(&self, id: i64) -> String;
    fn schedule_item_fallback_label(&self, id: i64) -> String;
    fn schedule_error_message(&self, error: &str) -> String;
    fn scheduling_content_error_message(&self, error: &str) -> String;
    fn watermark_error_message(&self, error: &str) -> String;
    fn channel_streaming_mode_label(&self, mode: i32) -> String;
    fn channel_language_label(&self, code: &str) -> String;
    fn media_kind_label(&self, kind: i64) -> String;
    fn playout_schedule_kind_label(&self, kind: i32) -> String;
    fn library_kind_label(&self, kind: &str) -> String;
    fn ffmpeg_tonemap_algorithm_label(&self, algorithm: i32) -> String;
    fn ffmpeg_output_dynamic_range_label(&self, range: i32) -> String;
    fn ffmpeg_loudness_mode_label(&self, mode: i32) -> String;
    fn ffmpeg_profile_video_format_label(&self, format: i32, hardware_acceleration: i32) -> String;
    fn ffmpeg_profile_audio_format_label(&self, format: i32) -> String;
    fn ffmpeg_profile_error_message(&self, error: &str) -> String;
    fn settings_error_message(&self, error: &str) -> String;
    fn media_action_error_message(&self, error: &str) -> String;
    fn filler_error_message(&self, error: &str) -> String;
    fn media_browser_error_message(&self, error: &str) -> String;
    fn media_source_error_message(&self, error: &str) -> String;
    fn addon_media_list_error_message(&self, error: &str) -> String;
    fn addon_operation_error_message(&self, code: &str, detail: &str) -> String;
    fn upload_error_message(&self, error: &str) -> String;
    fn hardware_acceleration_option_label(&self, label: &str, supported: bool) -> String;
    fn emptied_trash_count(&self, count: u64) -> String;
    fn select_collection_to_add_selected_items(&self, count: usize) -> String;
    fn select_playlist_to_add_selected_items(&self, count: usize) -> String;
    fn added_items_to_collection(&self, count: usize) -> String;
    fn added_items_to_playlist(&self, count: usize) -> String;
    fn add_to_collection_failed(&self, error: &dyn std::fmt::Display) -> String;
    fn add_to_playlist_failed(&self, error: &dyn std::fmt::Display) -> String;
    fn no_media_source_configured(&self, provider: &'static str) -> String;
    fn provider_connection_title(&self, provider: &'static str) -> String;
    fn provider_libraries_title(&self, provider: &'static str) -> String;
    fn provider_path_replacements_title(&self, provider: &'static str) -> String;
    fn provider_path_label(&self, provider: &'static str) -> String;
    fn health_summary(&self, errors: u32, warnings: u32) -> String;
    fn classic_playout_state_message(&self, playouts: &str) -> String;
    fn scan_refresh_state_message(
        &self,
        unresolved_resolutions: u64,
        stale_resolutions: u64,
        failed_intents: u64,
        stale_intents: u64,
    ) -> String;
    fn artwork_cardinality_message(
        &self,
        duplicate_rows: [i64; 3],
        projection_counts: [i64; 5],
    ) -> String;
    fn hls_live_delivery_message(
        &self,
        recent_under_runs: u64,
        minutes: u64,
        active_under_runs: u64,
        fallbacks: u64,
        fallback_hours: u64,
    ) -> String;
    fn unable_to_load_playback_troubleshooting(&self, error: &dyn std::fmt::Display) -> String;
    fn playout_reset_prompt(&self, channel: &str, schedule: &str) -> String;
}

pub fn load_language_initial() -> UiLanguage {
    #[cfg(feature = "ssr")]
    {
        use crate::host::state::AppState;

        let Some(state) = use_context::<AppState>() else {
            tracing::warn!("admin language fallback: application state is unavailable");
            return UiLanguage::English;
        };
        load_language_initial_for_state(&state)
    }

    #[cfg(all(not(feature = "ssr"), target_arch = "wasm32"))]
    {
        web_sys::window()
            .and_then(|window| window.document())
            .and_then(|document| document.document_element())
            .and_then(|element| element.get_attribute("lang"))
            .map_or(UiLanguage::English, |language| {
                UiLanguage::from_config(&language)
            })
    }

    #[cfg(all(not(feature = "ssr"), not(target_arch = "wasm32")))]
    {
        UiLanguage::English
    }
}

#[cfg(not(target_arch = "wasm32"))]
pub fn load_language_initial_for_state(app_state: &crate::host::state::AppState) -> UiLanguage {
    match tokio::runtime::Handle::try_current() {
        Ok(handle) => match tokio::task::block_in_place(|| {
            handle.block_on(
                crate::admin::server_fns::settings::load_ui_language_for_api(
                    app_state.application.ports().admin_settings(),
                ),
            )
        }) {
            Ok(language) => language,
            Err(error) => {
                tracing::warn!(%error, "admin language fallback: persisted setting load failed");
                UiLanguage::English
            }
        },
        Err(error) => {
            tracing::warn!(%error, "admin language fallback: async runtime is unavailable");
            UiLanguage::English
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashSet;

    #[test]
    fn supported_languages_follow_the_generated_registry() {
        let languages = crate::admin::languages::LANGUAGES;
        let options = supported_ui_language_options();

        assert_eq!(
            languages.first().map(|language| language.code),
            Some("en-us")
        );
        assert_eq!(options.len(), languages.len());

        let mut codes = HashSet::new();
        for (option, language) in options.iter().zip(languages) {
            assert!(
                codes.insert(language.code),
                "duplicate language code {}",
                language.code
            );
            assert!(!language.label.trim().is_empty());
            assert_eq!(option.value, language.code);
            assert_eq!(option.label, language.label);
            assert_eq!(normalize_ui_language(language.code), language.code);
            for alias in language.aliases {
                assert_eq!(normalize_ui_language(alias), language.code);
            }
        }
    }

    #[test]
    fn all_languages_translate_each_key() {
        let mut keys = HashSet::new();
        for key in ALL_TEXT_KEYS {
            assert!(keys.insert(*key), "duplicate text key: {key:?}");
        }

        for definition in crate::admin::languages::LANGUAGES {
            for key in ALL_TEXT_KEYS {
                assert!(
                    !definition.strings.text(*key).trim().is_empty(),
                    "{} has an empty translation for {key:?}",
                    definition.code
                );
            }
        }
    }

    #[test]
    fn numbered_episode_labels_use_conventional_notation_in_every_language() {
        for definition in crate::admin::languages::LANGUAGES {
            let language = UiLanguage::from_config(definition.code);
            for (season, episode, expected) in
                [(1, 1, "S01E01"), (0, 3, "S00E03"), (12, 123, "S12E123")]
            {
                assert_eq!(
                    episode_display_subtitle_text(language, None, Some(season), Some(episode))
                        .as_deref(),
                    Some(expected),
                    "{}",
                    definition.code
                );
                assert_eq!(
                    episode_display_subtitle_text(
                        language,
                        Some("Show"),
                        Some(season),
                        Some(episode)
                    ),
                    Some(format!("Show - {expected}")),
                    "{}",
                    definition.code
                );
            }
        }
    }

    #[test]
    fn invalid_or_missing_episode_numbers_are_localized_as_unnumbered() {
        for definition in crate::admin::languages::LANGUAGES {
            let language = UiLanguage::from_config(definition.code);
            let unnumbered = definition.strings.text(TextKey::Unnumbered);

            assert_eq!(
                episode_display_subtitle_text(language, None, Some(0), Some(0)).as_deref(),
                Some(unnumbered),
                "{} rendered a zero-number sentinel",
                definition.code
            );
            assert_eq!(
                episode_display_subtitle_text(language, None, Some(-1), Some(3)).as_deref(),
                Some(unnumbered),
                "{} rendered an invalid season",
                definition.code
            );
            assert_eq!(
                episode_display_subtitle_text(language, None, Some(2), Some(0)).as_deref(),
                Some(unnumbered),
                "{} rendered a zero-episode sentinel",
                definition.code
            );
            assert_eq!(
                episode_display_subtitle_text(language, None, None, None).as_deref(),
                Some(unnumbered),
                "{} omitted the unnumbered identity",
                definition.code
            );
            assert!(
                episode_display_subtitle_text(language, Some("Show"), Some(2), None)
                    .is_some_and(|value| value.contains(unnumbered)),
                "{} rendered a partial episode number",
                definition.code
            );
            assert!(
                episode_display_subtitle_text(language, Some("Show"), Some(0), Some(3))
                    .is_some_and(|value| !value.contains(unnumbered)),
                "{} hid a valid season-zero special",
                definition.code
            );
        }
    }

    #[test]
    fn composer_validation_errors_are_localized_for_every_language() {
        let errors = [
            ("search_query.required", TextKey::SearchQueryRequired),
            (
                "invalid query: every search rule needs a field and value",
                TextKey::SearchRuleValueRequired,
            ),
            (
                "error running server function: search_query.group_required",
                TextKey::SearchGroupRequired,
            ),
            (
                "invalid name: Saved search name must be unique",
                TextKey::SavedSearchNameUnique,
            ),
        ];

        for definition in crate::admin::languages::LANGUAGES {
            let language = UiLanguage::from_config(definition.code);
            for (error, key) in errors {
                assert_eq!(
                    search_query_error_text(language, error),
                    definition.strings.text(key),
                    "{} did not localize {error}",
                    definition.code
                );
            }
        }
    }

    #[test]
    fn addon_operation_codes_are_localized_for_every_language() {
        let codes = [
            "missing-setting",
            "missing-secret",
            "provider-unreachable",
            "provider-rejected",
            "provider-rate-limited",
            "not-found",
            "unsupported-url",
            "future-provider-code",
        ];
        for definition in crate::admin::languages::LANGUAGES {
            let language = UiLanguage::from_config(definition.code);
            for code in codes {
                let diagnostic = AddonOperationError::new(code, "safe detail");
                let error = format!("repository failure: {diagnostic}");
                let localized = addon_media_list_error_text(language, &error);
                assert!(
                    localized.contains("safe detail"),
                    "{}: {code}",
                    definition.code
                );
                assert!(!localized.contains(ADDON_OPERATION_ERROR_MARKER));
                assert!(!localized.starts_with(code));
            }
        }
    }

    #[test]
    fn classic_playout_health_text_is_localized_for_every_language() {
        let message = "Classic playout state is not safe to Refresh for: 50 - Health. Use Reset Playout for legacy or incompatible state; Refresh never clears a window without a complete checkpoint.";
        for definition in crate::admin::languages::LANGUAGES {
            let language = UiLanguage::from_config(definition.code);
            let title = health_check_title_text(language, CLASSIC_PLAYOUT_STATE_TITLE);
            let localized = health_check_message_text(
                language,
                CLASSIC_PLAYOUT_STATE_TITLE,
                message,
                "1 Classic playout state error(s)",
            );
            assert!(!title.trim().is_empty());
            assert!(localized.contains("50 - Health"));
            if definition.code != UiLanguage::English.code() {
                assert_ne!(title, CLASSIC_PLAYOUT_STATE_TITLE);
                assert!(!localized.starts_with(CLASSIC_PLAYOUT_STATE_MESSAGE_PREFIX));
            }
        }
    }

    #[test]
    fn automatic_publication_hold_health_is_localized_for_every_language() {
        for definition in crate::admin::languages::LANGUAGES {
            let language = UiLanguage::from_config(definition.code);
            let localized = health_check_message_text(
                language,
                "Add-on media lists",
                "Automatic publication needs review",
                "Automatic publication needs review",
            );
            assert_eq!(localized, text(language, TextKey::AutoPublicationHeld));
            if definition.code != UiLanguage::English.code() {
                assert_ne!(localized, "Automatic publication needs review");
            }
        }
    }

    #[test]
    fn structured_tool_warning_names_program_and_versions_in_every_language() {
        let details = ersatzrs_addon_contract::AddonToolUpdateWarning {
            program: "Example Player".to_owned(),
            installed_version: "1.0".to_owned(),
            latest_version: "1.2".to_owned(),
        };
        let code = Some("tool-update-failed-stale");
        let raw = "SYNTHETIC_PRIVATE_PROVIDER_PROSE";
        let health = format!(
            "org.example.fixture last reported warning readiness (tool-update-failed-stale): {}",
            serde_json::to_string(&details).unwrap()
        );
        for definition in crate::admin::languages::LANGUAGES {
            let language = UiLanguage::from_config(definition.code);
            let localized = addon_check_message_text(language, code, raw, Some(&details));
            for value in ["Example Player", "1.0", "1.2"] {
                assert!(localized.contains(value));
            }
            assert!(!localized.contains('{'));
            assert!(!localized.contains(raw));
            assert_eq!(
                health_check_message_text(language, "Add-ons", &health, ""),
                format!("org.example.fixture: {localized}")
            );
            let invalid = ersatzrs_addon_contract::AddonToolUpdateWarning {
                program: "SYNTHETIC_PRIVATE/path".to_owned(),
                ..details.clone()
            };
            assert_eq!(
                addon_check_message_text(language, code, raw, Some(&invalid)),
                text(language, TextKey::AddonToolUpdateFailedStale)
            );
            assert_eq!(
                addon_check_message_text(language, Some("ready"), "Ready", Some(&details)),
                "Ready"
            );
        }
    }

    #[test]
    fn failed_stale_readiness_is_localized_without_changing_other_findings() {
        let warning =
            "org.example.fixture last reported warning readiness (tool-update-failed-stale)";
        let other = "org.example.other last reported unavailable readiness (missing-command)";
        for definition in crate::admin::languages::LANGUAGES {
            let language = UiLanguage::from_config(definition.code);
            let expected = text(language, TextKey::AddonToolUpdateFailedStale);
            assert_eq!(
                health_check_message_text(language, "Add-ons", warning, ""),
                format!("org.example.fixture: {expected}")
            );
            let combined = format!(
                "Tool maintenance org.example.fixture/YT_DLP_BIN: update_failed; {warning}; {other}"
            );
            let localized = health_check_message_text(language, "Add-ons", &combined, "");
            assert!(localized.contains(expected));
            assert!(localized.contains(text(language, TextKey::AddonToolFailedHelp)));
            assert!(localized.ends_with(other));
            assert!(!localized.contains("tool-update-failed-stale"));
            assert_eq!(
                health_check_message_text(language, "Add-ons", other, ""),
                other
            );
            if language != UiLanguage::English {
                assert_ne!(
                    expected,
                    text(UiLanguage::English, TextKey::AddonToolUpdateFailedStale)
                );
            }
        }
    }

    #[test]
    fn tool_maintenance_health_remediation_is_localized_without_raw_diagnostics() {
        for definition in crate::admin::languages::LANGUAGES {
            let language = UiLanguage::from_config(definition.code);
            for code in [
                "unsupported_installation",
                "update_failed",
                "version_probe_failed",
                "tool_unavailable",
                "tool-busy",
                "tool-cleanup-required",
                "SYNTHETIC_PRIVATE_DIAGNOSTIC",
            ] {
                let message = format!("Tool maintenance org.example.fixture/YT_DLP_BIN: {code}");
                let localized = health_check_message_text(language, "Add-ons", &message, "");
                assert!(localized.contains("org.example.fixture/YT_DLP_BIN"));
                assert!(!localized.contains(code));
                assert!(localized.contains(text(
                    language,
                    addon_tool_maintenance_diagnostic(Some(code)).unwrap()
                )));
                if code == "tool-cleanup-required" {
                    assert!(localized.contains(ADDON_TOOL_OFFLINE_RECOVERY_COMMAND));
                    assert!(localized.contains("--state-dir"));
                    assert!(localized.contains("ersatzrs-web.exe"));
                    if language != UiLanguage::English {
                        assert_ne!(
                            text(language, TextKey::AddonToolCleanupHelp),
                            text(UiLanguage::English, TextKey::AddonToolCleanupHelp)
                        );
                    }
                }
            }
        }
    }

    #[test]
    fn next_overlay_health_is_localized_for_every_language() {
        for definition in crate::admin::languages::LANGUAGES {
            let language = UiLanguage::from_config(definition.code);
            let title = "Next Runtime Availability";
            assert_eq!(
                health_check_title_text(language, title),
                text(language, TextKey::NextRuntimeAvailability)
            );
            for (message, key) in [
                (
                    "Next configuration overlay is unavailable or invalid",
                    TextKey::NextOverlayInvalid,
                ),
                (
                    "Next configuration overlay validation is incomplete",
                    TextKey::NextOverlayIncomplete,
                ),
            ] {
                assert_eq!(
                    health_check_message_text(language, title, message, message),
                    text(language, key)
                );
                assert_eq!(
                    health_check_message_text(language, title, "", message),
                    text(language, key)
                );
            }
            for key in [
                TextKey::NextRuntimeAvailability,
                TextKey::NextOverlayInvalid,
                TextKey::NextOverlayIncomplete,
            ] {
                assert!(!text(language, key).is_empty());
                if language != UiLanguage::English {
                    assert_ne!(text(language, key), text(UiLanguage::English, key));
                }
            }
        }
    }

    #[test]
    fn javascript_audio_health_is_localized_for_every_language() {
        for definition in crate::admin::languages::LANGUAGES {
            let language = UiLanguage::from_config(definition.code);
            let title = health_check_title_text(language, "JavaScript Audio Selection");
            assert_eq!(
                title,
                text(language, TextKey::JavascriptAudioSelectionTitle)
            );
            let healthy = health_check_message_text(
                language,
                "JavaScript Audio Selection",
                "No failures",
                "No selector failures recorded",
            );
            assert_eq!(
                healthy,
                text(language, TextKey::JavascriptAudioSelectionHealthy)
            );
            let warning = health_check_message_text(language, "JavaScript Audio Selection", "12 JavaScript audio-selector loading or execution failures recorded since startup.", "JavaScript audio selection fell back");
            assert!(warning.contains("12"));
            assert!(!warning.contains("{count}"));
            if language != UiLanguage::English {
                assert_ne!(
                    healthy,
                    text(
                        UiLanguage::English,
                        TextKey::JavascriptAudioSelectionHealthy
                    )
                );
            }
        }
    }

    #[test]
    fn addon_list_disk_usage_health_is_localized_for_every_language() {
        let title = "Add-on list disk usage";
        let message = "List #1: 25.000 GB used; warning at 25 GB.\nCombined: 250.000 GB used; warning at 250 GB.";
        for definition in crate::admin::languages::LANGUAGES {
            let language = UiLanguage::from_config(definition.code);
            for (diagnostic, key) in [
                (
                    "Add-on list disk usage could not be measured completely. Check storage access and retry the health check.",
                    TextKey::AddonListDiskUsageUnavailable,
                ),
                (
                    "Add-on list disk usage could not be measured.",
                    TextKey::AddonListDiskUsageUnavailableBrief,
                ),
            ] {
                for (full, brief) in [(diagnostic, ""), ("", diagnostic)] {
                    assert_eq!(
                        health_check_message_text(language, title, full, brief),
                        text(language, key)
                    );
                }
            }
            assert_eq!(
                health_check_title_text(language, title),
                text(language, TextKey::AddonListDiskUsageTitle)
            );
            let localized = health_check_message_text(language, title, message, "");
            assert_eq!(localized.lines().count(), 2);
            assert!(localized.contains("25.000"));
            assert!(localized.contains("250.000"));
            assert!(!localized.contains('{'));
            assert_eq!(
                health_check_message_text(language, title, "", "Add-on list disk usage warning"),
                text(language, TextKey::AddonListDiskUsageBrief)
            );
            if language != UiLanguage::English {
                assert_ne!(localized, message);
            }
            for unknown in [
                "List #x: 25.000 GB used; warning at 25 GB.",
                "Combined: NaN GB used; warning at 250 GB.",
                "future diagnostic",
            ] {
                assert_eq!(
                    health_check_message_text(language, title, unknown, ""),
                    unknown
                );
            }
        }
    }

    #[test]
    fn preparation_worker_unavailable_health_is_localized_for_every_language() {
        let message = "Media preparation worker is unavailable. Restart the server before retrying preparation.";
        for definition in crate::admin::languages::LANGUAGES {
            let language = UiLanguage::from_config(definition.code);
            for (full, brief) in [(message, ""), ("", message)] {
                let localized =
                    health_check_message_text(language, "Add-on media lists", full, brief);
                assert_eq!(
                    localized,
                    text(language, TextKey::PreparationWorkerUnavailable)
                );
                assert_ne!(
                    localized,
                    text(language, TextKey::PreparationProcessRecoveryRequired)
                );
                if language != UiLanguage::English {
                    assert_ne!(localized, message);
                }
            }
        }
    }

    #[test]
    fn preparation_process_recovery_health_is_localized_for_every_language() {
        let message = "Media preparation needs process recovery. Confirm prior provider processes have stopped before recovering retained storage and retrying preparation.";
        for definition in crate::admin::languages::LANGUAGES {
            let language = UiLanguage::from_config(definition.code);
            for (full, brief) in [(message, ""), ("", message)] {
                let localized =
                    health_check_message_text(language, "Add-on media lists", full, brief);
                assert_eq!(
                    localized,
                    text(language, TextKey::PreparationProcessRecoveryRequired)
                );
                assert!(!localized.is_empty());
                if definition.code != UiLanguage::English.code() {
                    assert_ne!(localized, message);
                }
            }
        }
    }

    #[test]
    fn scan_refresh_health_text_is_localized_for_every_language() {
        let message = "Scan refresh state needs attention: 2 unresolved impact resolution(s), 3 stale impact resolution(s), 5 failed playout refresh intent(s), and 7 stale playout refresh intent(s). Review scanner and scheduling logs, then retry the affected scan or playout refresh.";
        for definition in crate::admin::languages::LANGUAGES {
            let language = UiLanguage::from_config(definition.code);
            let title = health_check_title_text(language, SCAN_REFRESH_STATE_TITLE);
            let localized = health_check_message_text(
                language,
                SCAN_REFRESH_STATE_TITLE,
                message,
                "17 scan refresh state error(s)",
            );
            assert!(!title.trim().is_empty());
            for count in ["2", "3", "5", "7"] {
                assert!(
                    localized.contains(count),
                    "{}: {localized}",
                    definition.code
                );
            }
            if definition.code != UiLanguage::English.code() {
                assert_ne!(title, SCAN_REFRESH_STATE_TITLE);
                assert!(!localized.starts_with(SCAN_REFRESH_STATE_MESSAGE_PREFIX));
            }
        }
    }

    #[test]
    fn subtitle_cleanup_health_text_is_localized_for_every_language() {
        let message = "Subtitle cleanup pending: 3 ownerless subtitle or empty-path sidecar row(s). Automatic cleanup runs hourly. Cached subtitle files are preserved.";
        let brief = "3 subtitle row(s) awaiting cleanup";
        for definition in crate::admin::languages::LANGUAGES {
            let language = UiLanguage::from_config(definition.code);
            let title = health_check_title_text(language, "Subtitle Cleanup");
            let localized = health_check_message_text(language, "Subtitle Cleanup", message, brief);
            let localized_brief =
                health_check_message_text(language, "Subtitle Cleanup", "", brief);
            for value in [&localized, &localized_brief] {
                assert!(value.contains('3'));
                assert!(!value.contains("{count}"));
            }
            if language != UiLanguage::English {
                assert_ne!(title, "Subtitle Cleanup");
                assert_ne!(localized, message);
                assert_ne!(localized_brief, brief);
            }
        }
    }

    #[test]
    fn artwork_orphans_health_text_is_localized_for_every_language() {
        let message = "Orphan cleanup pending: 2 actor row(s) and 3 artwork row(s). Automatic cleanup runs hourly. To request cleanup now, use POST /api/maintenance/clean_artwork. Attached records and cached image files are preserved.";
        let brief = "2 orphan actor row(s) and 3 orphan artwork row(s)";
        for definition in crate::admin::languages::LANGUAGES {
            let language = UiLanguage::from_config(definition.code);
            let title = health_check_title_text(language, "Artwork Orphans");
            let localized = health_check_message_text(language, "Artwork Orphans", message, brief);
            let localized_brief = health_check_message_text(language, "Artwork Orphans", "", brief);
            for value in [&localized, &localized_brief] {
                assert!(value.contains('2') && value.contains('3'));
                assert!(!value.contains("{actors}") && !value.contains("{artwork}"));
            }
            assert!(localized.contains("POST /api/maintenance/clean_artwork"));
            if language != UiLanguage::English {
                assert_ne!(title, "Artwork Orphans");
                assert_ne!(localized, message);
                assert_ne!(localized_brief, brief);
            }
        }
    }

    #[test]
    fn artwork_cardinality_health_text_is_localized_for_every_language() {
        let message = "Artwork cardinality needs attention: 2 duplicate show artwork row(s), 3 duplicate season artwork row(s), and 5 duplicate episode artwork row(s); projected television counts are 7 published add-on list episode item(s), 11 projection row(s), 13 distinct projected episode(s), 17 complete hierarchy projection(s), and 19 hierarchy episode(s). Refresh or republish the affected add-on television lists.";
        for definition in crate::admin::languages::LANGUAGES {
            let language = UiLanguage::from_config(definition.code);
            let title = health_check_title_text(language, ARTWORK_CARDINALITY_TITLE);
            let localized = health_check_message_text(
                language,
                ARTWORK_CARDINALITY_TITLE,
                message,
                "10 duplicate artwork row(s) and 4 projected television cardinality mismatch(es)",
            );
            assert!(!title.trim().is_empty());
            for count in ["2", "3", "5", "7", "11", "13", "17", "19"] {
                assert!(
                    localized.contains(count),
                    "{}: {localized}",
                    definition.code
                );
            }
            if definition.code != UiLanguage::English.code() {
                assert_ne!(title, ARTWORK_CARDINALITY_TITLE);
                assert!(!localized.starts_with(ARTWORK_CARDINALITY_MESSAGE_PREFIX));
            }
        }
    }

    #[test]
    fn hls_live_delivery_health_text_is_localized_for_every_language() {
        let message = "2 channel(s) delivered live HLS media slower than wall time in the last 30 minutes; 1 had not recovered when last observed; 3 channel(s) are using a verified software-encoding and/or lower-quality fallback for up to 24 hours. Review the streaming logs and run the delivery-aware TSDuck playback gate before changing provider settings.";
        for definition in crate::admin::languages::LANGUAGES {
            let language = UiLanguage::from_config(definition.code);
            let title = health_check_title_text(language, HLS_LIVE_DELIVERY_TITLE);
            let localized = health_check_message_text(
                language,
                HLS_LIVE_DELIVERY_TITLE,
                message,
                "2 recent live HLS delivery under-run(s)",
            );
            assert!(!title.trim().is_empty());
            for count in ["2", "30", "1", "3", "24"] {
                assert!(
                    localized.contains(count),
                    "{}: {localized}",
                    definition.code
                );
            }
            if definition.code != UiLanguage::English.code() {
                assert_ne!(title, HLS_LIVE_DELIVERY_TITLE);
                assert!(!localized.contains(HLS_LIVE_DELIVERY_SUFFIX));
            }
        }
    }
}
