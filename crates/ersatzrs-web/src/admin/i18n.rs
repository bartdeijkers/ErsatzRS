use leptos::prelude::*;

use crate::admin::server_fns::settings::SelectOption;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum UiLanguage {
    English,
    Polish,
    PortugueseBrazil,
    Dutch,
}

impl UiLanguage {
    pub const fn code(self) -> &'static str {
        match self {
            Self::English => "en-us",
            Self::Polish => "pl",
            Self::PortugueseBrazil => "pt-br",
            Self::Dutch => "nl",
        }
    }

    pub const fn label(self) -> &'static str {
        match self {
            Self::English => "English",
            Self::Polish => "Polski",
            Self::PortugueseBrazil => "Português (Brasil)",
            Self::Dutch => "Nederlands",
        }
    }

    pub fn from_config(value: &str) -> Self {
        match value.trim().to_ascii_lowercase().as_str() {
            "pl" => Self::Polish,
            "pt-br" | "pt_br" => Self::PortugueseBrazil,
            "nl" | "nl-nl" | "nl_nl" => Self::Dutch,
            _ => Self::English,
        }
    }
}

pub fn normalize_ui_language(value: &str) -> String {
    UiLanguage::from_config(value).code().to_owned()
}

pub fn supported_ui_language_options() -> Vec<SelectOption> {
    [
        UiLanguage::English,
        UiLanguage::Polish,
        UiLanguage::PortugueseBrazil,
        UiLanguage::Dutch,
    ]
    .into_iter()
    .map(|language| SelectOption {
        value: language.code().to_owned(),
        label: language.label().to_owned(),
    })
    .collect()
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum TextKey {
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
    AddTraktList,
    AddToCollection,
    AddToEllipsis,
    AddToPlaylist,
    AddToSchedule,
    Actions,
    All,
    AreYouSureDelete,
    Actors,
    Artist,
    ArtistNotFound,
    AfterDurationEnd,
    AutomaticRefresh,
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
    Blocks,
    BlockToRemove,
    BlockWillNotFit,
    Cancel,
    Clear,
    Channels,
    Close,
    Collections,
    CollectionDeleted,
    CollectionSaved,
    ComponentGallery,
    ComputingPreview,
    Confirm,
    ConfirmDelete,
    Content,
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
    DeleteTraktList,
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
    HttpMinimumLogLevel,
    Id,
    Images,
    Item,
    ItemActions,
    ItemType,
    Items,
    Language,
    Languages,
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
    Movie,
    MovieNotFound,
    Movies,
    MusicVideo,
    MultiCollections,
    ManualCollections,
    MatchFailed,
    MatchStatus,
    MatchTraktListItems,
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
    SchedulingContext,
    ScheduleItem,
    ScheduleItemActionFailed,
    ScheduleItemFiller,
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
    SearchMedia,
    SearchMoviesOrShows,
    SearchTraktList,
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
    SaveTraktList,
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
    TraktList,
    TraktListActionFailed,
    TraktListAdded,
    TraktListDeleted,
    TraktListMatched,
    TraktListNotFound,
    TraktListSaveFailed,
    TraktListSaved,
    TraktLists,
    TraktListUrl,
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
    NormalizeAudio,
    NormalizeColors,
    NormalizeFramerate,
    NormalizeLoudness,
    NormalizeVideo,
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
    SaveProfile,
    SaveFillerPreset,
    SaveRerunCollection,
    SaveSequentialSchedule,
    SaveTemplate,
    SaveBlock,
    ScaleAndPad,
    ScalingBehavior,
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
    VideoProfile,
    AddAll,
    AddedItemToPlaylist,
    AddedArtistToSchedule,
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
    EditTraktListProperties,
    EnterTraktListUrl,
    EnterSearchQuery,
    Episodes,
    LoadingCollections,
    LoadingPlaylistGroups,
    LoadingPlaylists,
    GeneratePlaylist,
    GeneratePlaylistFromSortedTraktItems,
    MusicVideos,
    NewCollectionName,
    NoMediaMatched,
    NoTraktListsYet,
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
    CollectionsScanFailed,
    CollectionsScanQueued,
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
    DefaultOption,
    DefaultSchedule,
    DeleteChannel,
    DownloadMediaSample,
    DownloadResults,
    DisableWatermarks,
    EditChannel,
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
    Group,
    HealthNoPersistedRun,
    HealthPersistedRunHelp,
    HlsDirect,
    HlsSegmenter,
    Hours,
    IdleBehavior,
    IdleBehaviorHelp,
    Info,
    IsEnabled,
    KeepRunning,
    LastMidFillerHelp,
    LastPage,
    Level,
    LoadingMediaInfo,
    Logs,
    Logo,
    MediaItem,
    Message,
    MissingMediaItemQueryParameter,
    Mirror,
    MirrorSourceChannel,
    MusicVideoCreditsMode,
    MusicVideoCreditsTemplate,
    NoMatchingRecordsFound,
    NoFfmpegProfiles,
    NvidiaCapabilitiesNotCollected,
    Number,
    OnDemand,
    MatchedPointsHelp,
    Pass,
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
    PreviewPlaylistPlayout,
    PreferredAudioTitle,
    PreferredSubtitleLanguage,
    QsvCapabilitiesNotCollected,
    RemoveStoredLogo,
    SaveChannel,
    Second,
    Seconds,
    SelectProfile,
    SequentialSchedule,
    SequentialScheduleValidation,
    ShowDisabled,
    ShowInEpg,
    SlugSeconds,
    SlugSecondsHelp,
    SongVideoMode,
    Song,
    SortByLevel,
    SortByTimestamp,
    StopOnDisconnect,
    StopScheduling,
    StopSchedulingBlockItems,
    StreamSelector,
    StreamSelectorMode,
    StreamingMode,
    Styles,
    SubtitleMode,
    Timestamp,
    Timeline,
    Target,
    Tags,
    Tools,
    TranscodeMode,
    TranscodeModeHelp,
    TroubleshootPlayback,
    TroubleshootChannel,
    TroubleshootingReports,
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
    UpdateListFromTraktDaily,
    ActualSize,
    AddWatermark,
    BottomLeft,
    BottomMiddle,
    BottomRight,
    CopyWatermark,
    CopyOf,
    CurrentImage,
    Custom,
    DeleteWatermark,
    DurationSeconds,
    DangerButton,
    DeleteDialog,
    Decrement,
    EditWatermark,
    DisabledButton,
    Dialogs,
    Frequency,
    GeneratingPreview,
    HorizontalMarginPercent,
    Identity,
    Image,
    ImageSource,
    Intermittent,
    IntermittentTiming,
    LeftMiddle,
    Location,
    MiddleCenter,
    Mode,
    Minutes,
    Moods,
    ModalCancelled,
    ModalConfirmed,
    NewName,
    Opacity,
    OpacityExpression,
    OpacityOneToHundred,
    Permanent,
    PerPass,
    PlaceWithinSourceContent,
    Placement,
    PlacementPreview,
    RightMiddle,
    SaveWatermark,
    Scaled,
    Size,
    TopLeft,
    TopMiddle,
    TopRight,
    UploadFailed,
    UploadImage,
    VerticalMarginPercent,
    WatermarkCopied,
    WatermarkNotFound,
    WatermarkPlacementPreview,
    WatermarkPreview,
    WidthPercent,
    ZIndex,
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
}

pub const ALL_TEXT_KEYS: &[TextKey] = &[
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
    TextKey::AddTraktList,
    TextKey::AddToCollection,
    TextKey::AddToEllipsis,
    TextKey::AddToPlaylist,
    TextKey::AddToSchedule,
    TextKey::Actions,
    TextKey::All,
    TextKey::AreYouSureDelete,
    TextKey::Actors,
    TextKey::Artist,
    TextKey::ArtistNotFound,
    TextKey::AfterDurationEnd,
    TextKey::AutomaticRefresh,
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
    TextKey::Blocks,
    TextKey::BlockToRemove,
    TextKey::BlockWillNotFit,
    TextKey::Cancel,
    TextKey::Clear,
    TextKey::Channels,
    TextKey::Close,
    TextKey::Collections,
    TextKey::CollectionDeleted,
    TextKey::CollectionSaved,
    TextKey::ComponentGallery,
    TextKey::ComputingPreview,
    TextKey::Confirm,
    TextKey::ConfirmDelete,
    TextKey::Content,
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
    TextKey::DeleteTraktList,
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
    TextKey::HttpMinimumLogLevel,
    TextKey::Id,
    TextKey::Images,
    TextKey::Item,
    TextKey::ItemActions,
    TextKey::ItemType,
    TextKey::Items,
    TextKey::Language,
    TextKey::Languages,
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
    TextKey::MatchTraktListItems,
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
    TextKey::NoTraktListsYet,
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
    TextKey::EditFillerPreset,
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
    TextKey::SchedulingContext,
    TextKey::ScheduleItem,
    TextKey::ScheduleItemActionFailed,
    TextKey::ScheduleItemFiller,
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
    TextKey::SearchMedia,
    TextKey::SearchMoviesOrShows,
    TextKey::SearchTraktList,
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
    TextKey::SaveTraktList,
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
    TextKey::TraktList,
    TextKey::TraktListActionFailed,
    TextKey::TraktListAdded,
    TextKey::TraktListDeleted,
    TextKey::TraktListMatched,
    TextKey::TraktListNotFound,
    TextKey::TraktListSaveFailed,
    TextKey::TraktListSaved,
    TextKey::TraktLists,
    TextKey::TraktListUrl,
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
    TextKey::NormalizeAudio,
    TextKey::NormalizeColors,
    TextKey::NormalizeFramerate,
    TextKey::NormalizeLoudness,
    TextKey::NormalizeVideo,
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
    TextKey::SaveProfile,
    TextKey::SaveFillerPreset,
    TextKey::SaveRerunCollection,
    TextKey::SaveSequentialSchedule,
    TextKey::SaveTemplate,
    TextKey::SaveBlock,
    TextKey::ScaleAndPad,
    TextKey::ScalingBehavior,
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
    TextKey::VideoProfile,
    TextKey::AddAll,
    TextKey::AddedItemToPlaylist,
    TextKey::AddedArtistToSchedule,
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
    TextKey::EditTraktListProperties,
    TextKey::EnterTraktListUrl,
    TextKey::EnterSearchQuery,
    TextKey::Episodes,
    TextKey::LoadingCollections,
    TextKey::LoadingPlaylistGroups,
    TextKey::LoadingPlaylists,
    TextKey::GeneratePlaylist,
    TextKey::GeneratePlaylistFromSortedTraktItems,
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
    TextKey::CollectionsScanFailed,
    TextKey::CollectionsScanQueued,
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
    TextKey::DefaultOption,
    TextKey::DefaultSchedule,
    TextKey::DeleteChannel,
    TextKey::DownloadMediaSample,
    TextKey::DownloadResults,
    TextKey::DisableWatermarks,
    TextKey::EditChannel,
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
    TextKey::Group,
    TextKey::HealthNoPersistedRun,
    TextKey::HealthPersistedRunHelp,
    TextKey::HlsDirect,
    TextKey::HlsSegmenter,
    TextKey::Hours,
    TextKey::IdleBehavior,
    TextKey::IdleBehaviorHelp,
    TextKey::Info,
    TextKey::IsEnabled,
    TextKey::KeepRunning,
    TextKey::LastMidFillerHelp,
    TextKey::LastPage,
    TextKey::Level,
    TextKey::LoadingMediaInfo,
    TextKey::Logs,
    TextKey::Logo,
    TextKey::MediaItem,
    TextKey::Message,
    TextKey::MissingMediaItemQueryParameter,
    TextKey::Mirror,
    TextKey::MirrorSourceChannel,
    TextKey::MusicVideoCreditsMode,
    TextKey::MusicVideoCreditsTemplate,
    TextKey::NoMatchingRecordsFound,
    TextKey::NoFfmpegProfiles,
    TextKey::NvidiaCapabilitiesNotCollected,
    TextKey::Number,
    TextKey::OnDemand,
    TextKey::MatchedPointsHelp,
    TextKey::Pass,
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
    TextKey::PreviewPlaylistPlayout,
    TextKey::PreferredAudioTitle,
    TextKey::PreferredSubtitleLanguage,
    TextKey::QsvCapabilitiesNotCollected,
    TextKey::RemoveStoredLogo,
    TextKey::SaveChannel,
    TextKey::Second,
    TextKey::Seconds,
    TextKey::SelectProfile,
    TextKey::SequentialSchedule,
    TextKey::SequentialScheduleValidation,
    TextKey::ShowDisabled,
    TextKey::ShowInEpg,
    TextKey::SlugSeconds,
    TextKey::SlugSecondsHelp,
    TextKey::Song,
    TextKey::SongVideoMode,
    TextKey::SortByLevel,
    TextKey::SortByTimestamp,
    TextKey::StopOnDisconnect,
    TextKey::StopScheduling,
    TextKey::StopSchedulingBlockItems,
    TextKey::StreamSelector,
    TextKey::StreamSelectorMode,
    TextKey::StreamingMode,
    TextKey::Styles,
    TextKey::SubtitleMode,
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
    TextKey::UpdateListFromTraktDaily,
    TextKey::ActualSize,
    TextKey::AddWatermark,
    TextKey::BottomLeft,
    TextKey::BottomMiddle,
    TextKey::BottomRight,
    TextKey::CopyWatermark,
    TextKey::CopyOf,
    TextKey::CurrentImage,
    TextKey::Custom,
    TextKey::DeleteWatermark,
    TextKey::DurationSeconds,
    TextKey::DangerButton,
    TextKey::DeleteDialog,
    TextKey::Decrement,
    TextKey::EditWatermark,
    TextKey::DisabledButton,
    TextKey::Dialogs,
    TextKey::Frequency,
    TextKey::GeneratingPreview,
    TextKey::HorizontalMarginPercent,
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
    TextKey::Opacity,
    TextKey::OpacityExpression,
    TextKey::OpacityOneToHundred,
    TextKey::Permanent,
    TextKey::PerPass,
    TextKey::PlaceWithinSourceContent,
    TextKey::Placement,
    TextKey::PlacementPreview,
    TextKey::RightMiddle,
    TextKey::SaveWatermark,
    TextKey::Scaled,
    TextKey::Size,
    TextKey::TopLeft,
    TextKey::TopMiddle,
    TextKey::TopRight,
    TextKey::UploadFailed,
    TextKey::UploadImage,
    TextKey::VerticalMarginPercent,
    TextKey::WatermarkCopied,
    TextKey::WatermarkNotFound,
    TextKey::WatermarkPlacementPreview,
    TextKey::WatermarkPreview,
    TextKey::WidthPercent,
    TextKey::ZIndex,
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

pub fn media_source_error_message(error: impl std::fmt::Display) -> String {
    let message = error.to_string();
    languages_for(use_i18n()).media_source_error_message(&message)
}

pub fn media_source_error_text(language: UiLanguage, error: &str) -> String {
    languages_for(language).media_source_error_message(error)
}

pub fn trakt_error_message(error: impl std::fmt::Display) -> String {
    let message = error.to_string();
    languages_for(use_i18n()).trakt_error_message(&message)
}

pub fn trakt_error_text(language: UiLanguage, error: &str) -> String {
    languages_for(language).trakt_error_message(error)
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

pub fn text(language: UiLanguage, key: TextKey) -> &'static str {
    languages_for(language).text(key)
}

fn languages_for(language: UiLanguage) -> &'static dyn LanguageStrings {
    match language {
        UiLanguage::English => &crate::admin::languages::en_us::STRINGS,
        UiLanguage::Polish => &crate::admin::languages::pl::STRINGS,
        UiLanguage::PortugueseBrazil => &crate::admin::languages::pt_br::STRINGS,
        UiLanguage::Dutch => &crate::admin::languages::nl::STRINGS,
    }
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
    fn media_item_fallback_label(&self, id: i64) -> String;
    fn schedule_item_fallback_label(&self, id: i64) -> String;
    fn schedule_error_message(&self, error: &str) -> String;
    fn scheduling_content_error_message(&self, error: &str) -> String;
    fn watermark_error_message(&self, error: &str) -> String;
    fn channel_streaming_mode_label(&self, mode: i32) -> String;
    fn channel_language_label(&self, code: &str) -> String;
    fn media_kind_label(&self, kind: i64) -> String;
    fn library_kind_label(&self, kind: &str) -> String;
    fn ffmpeg_tonemap_algorithm_label(&self, algorithm: i32) -> String;
    fn ffmpeg_loudness_mode_label(&self, mode: i32) -> String;
    fn ffmpeg_profile_video_format_label(&self, format: i32, hardware_acceleration: i32) -> String;
    fn ffmpeg_profile_audio_format_label(&self, format: i32) -> String;
    fn ffmpeg_profile_error_message(&self, error: &str) -> String;
    fn settings_error_message(&self, error: &str) -> String;
    fn media_action_error_message(&self, error: &str) -> String;
    fn filler_error_message(&self, error: &str) -> String;
    fn media_browser_error_message(&self, error: &str) -> String;
    fn media_source_error_message(&self, error: &str) -> String;
    fn trakt_error_message(&self, error: &str) -> String;
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
    fn unable_to_load_playback_troubleshooting(&self, error: &dyn std::fmt::Display) -> String;
}

pub fn load_language_initial() -> UiLanguage {
    #[cfg(feature = "ssr")]
    {
        use crate::host::state::AppState;

        let Some(state) = use_context::<AppState>() else {
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
        Ok(handle) => tokio::task::block_in_place(|| {
            handle.block_on(
                crate::admin::server_fns::settings::load_ui_language_for_api(&app_state.pool),
            )
        })
        .unwrap_or(UiLanguage::English),
        Err(_) => UiLanguage::English,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn supported_languages_match_requested_i18n_scope() {
        let values: Vec<String> = supported_ui_language_options()
            .into_iter()
            .map(|option| option.value)
            .collect();

        assert_eq!(values, vec!["en-us", "pl", "pt-br", "nl"]);
    }

    #[test]
    fn all_languages_translate_each_key() {
        let languages = [
            UiLanguage::English,
            UiLanguage::Polish,
            UiLanguage::PortugueseBrazil,
            UiLanguage::Dutch,
        ];
        for language in languages {
            for key in ALL_TEXT_KEYS {
                assert!(!text(language, *key).trim().is_empty());
            }
        }
    }

    #[test]
    fn season_display_names_follow_active_language() {
        assert_eq!(
            languages_for(UiLanguage::English).season_display_name(0),
            "Specials"
        );
        assert_eq!(
            languages_for(UiLanguage::English).season_display_name(2),
            "Season 2"
        );
        assert_eq!(
            languages_for(UiLanguage::Polish).season_display_name(0),
            "Odcinki specjalne"
        );
        assert_eq!(
            languages_for(UiLanguage::PortugueseBrazil).season_display_name(2),
            "Temporada 2"
        );
        assert_eq!(
            languages_for(UiLanguage::Dutch).season_display_name(2),
            "Seizoen 2"
        );
    }

    #[test]
    fn fallback_item_labels_follow_active_language() {
        assert_eq!(
            languages_for(UiLanguage::English).media_item_fallback_label(42),
            "Media Item #42"
        );
        assert_eq!(
            languages_for(UiLanguage::Polish).schedule_item_fallback_label(7),
            "Element harmonogramu #7"
        );
        assert_eq!(
            languages_for(UiLanguage::PortugueseBrazil).media_item_fallback_label(42),
            "Item de mídia #42"
        );
        assert_eq!(
            languages_for(UiLanguage::Dutch).schedule_item_fallback_label(7),
            "Schema-item #7"
        );
    }

    #[test]
    fn log_level_labels_follow_active_language_without_changing_unknown_values() {
        assert_eq!(
            log_level_text(UiLanguage::English, "Information"),
            "Information"
        );
        assert_eq!(log_level_text(UiLanguage::Polish, "Warning"), "Ostrzeżenie");
        assert_eq!(
            log_level_text(UiLanguage::PortugueseBrazil, "Debug"),
            "Depuração"
        );
        assert_eq!(log_level_text(UiLanguage::Dutch, "Fatal"), "Fataal");
        assert_eq!(log_level_text(UiLanguage::Dutch, "TRACE"), "TRACE");
    }

    #[test]
    fn channel_labels_follow_active_language_without_changing_unknown_values() {
        assert_eq!(
            languages_for(UiLanguage::English).channel_streaming_mode_label(1),
            "MPEG-TS (Legacy)"
        );
        assert_eq!(
            languages_for(UiLanguage::Polish).channel_streaming_mode_label(1),
            "MPEG-TS (starszy)"
        );
        assert_eq!(
            languages_for(UiLanguage::PortugueseBrazil).channel_language_label("deu"),
            "Alemão"
        );
        assert_eq!(
            languages_for(UiLanguage::Dutch).channel_language_label("nld"),
            "Nederlands"
        );
        assert_eq!(
            languages_for(UiLanguage::Dutch).channel_streaming_mode_label(99),
            "Onbekend (99)"
        );
        assert_eq!(
            languages_for(UiLanguage::Dutch).channel_language_label("tlh"),
            "tlh"
        );
    }

    #[test]
    fn media_and_library_kind_labels_follow_active_language() {
        assert_eq!(media_kind_text(UiLanguage::English, 3), "Music Videos");
        assert_eq!(media_kind_text(UiLanguage::Polish, 7), "Zdalne strumienie");
        assert_eq!(
            media_kind_text(UiLanguage::PortugueseBrazil, 4),
            "Outros vídeos"
        );
        assert_eq!(media_kind_text(UiLanguage::Dutch, 1), "Films");
        assert_eq!(media_kind_text(UiLanguage::Dutch, 99), "Onbekend");
        assert_eq!(library_kind_text(UiLanguage::Dutch, "Local"), "Lokaal");
        assert_eq!(library_kind_text(UiLanguage::Polish, "Plex"), "Plex");
    }

    #[test]
    fn hardware_acceleration_availability_labels_follow_active_language() {
        assert_eq!(
            hardware_acceleration_option_text(UiLanguage::English, "NVENC", false),
            "NVENC (not available on this system)"
        );
        assert_eq!(
            hardware_acceleration_option_text(UiLanguage::Polish, "NVENC", false),
            "NVENC (niedostępne w tym systemie)"
        );
        assert_eq!(
            hardware_acceleration_option_text(UiLanguage::PortugueseBrazil, "NVENC", false),
            "NVENC (não disponível neste sistema)"
        );
        assert_eq!(
            hardware_acceleration_option_text(UiLanguage::Dutch, "NVENC", false),
            "NVENC (niet beschikbaar op dit systeem)"
        );
        assert_eq!(
            hardware_acceleration_option_text(UiLanguage::Dutch, "VAAPI", true),
            "VAAPI"
        );
    }

    #[test]
    fn ffmpeg_option_labels_follow_active_language() {
        assert_eq!(
            ffmpeg_tonemap_algorithm_text(UiLanguage::English, 1),
            "Clip"
        );
        assert_eq!(
            ffmpeg_tonemap_algorithm_text(UiLanguage::Polish, 0),
            "Liniowy"
        );
        assert_eq!(
            ffmpeg_tonemap_algorithm_text(UiLanguage::PortugueseBrazil, 2),
            "Gama"
        );
        assert_eq!(
            ffmpeg_tonemap_algorithm_text(UiLanguage::Dutch, 99),
            "Onbekend (99)"
        );
        assert_eq!(ffmpeg_loudness_mode_text(UiLanguage::Dutch, 0), "Uit");
        assert_eq!(
            ffmpeg_loudness_mode_text(UiLanguage::PortugueseBrazil, 1),
            "loudnorm"
        );
    }

    #[test]
    fn ffmpeg_profile_list_labels_follow_active_language() {
        assert_eq!(
            ffmpeg_profile_video_format_text(UiLanguage::English, 1, 2),
            "H264 / NVENC"
        );
        assert_eq!(
            ffmpeg_profile_video_format_text(UiLanguage::Polish, 99, 0),
            "Kopiuj"
        );
        assert_eq!(
            ffmpeg_profile_video_format_text(UiLanguage::PortugueseBrazil, 0, 0),
            "Nenhum"
        );
        assert_eq!(
            ffmpeg_profile_video_format_text(UiLanguage::Dutch, 42, 0),
            "Onbekend (42)"
        );
        assert_eq!(
            ffmpeg_profile_audio_format_text(UiLanguage::Dutch, 99),
            "Kopiëren"
        );
    }

    #[test]
    fn ffmpeg_profile_server_errors_follow_active_language_without_changing_unknown_values() {
        assert_eq!(
            ffmpeg_profile_error_text(UiLanguage::English, "Profile name is required."),
            "Profile name is required."
        );
        assert_eq!(
            ffmpeg_profile_error_text(UiLanguage::Polish, "Thread count must be 0 or greater."),
            "Liczba wątków musi wynosić 0 lub więcej."
        );
        assert_eq!(
            ffmpeg_profile_error_text(
                UiLanguage::PortugueseBrazil,
                "An FFmpeg profile named \"main\" already exists."
            ),
            "Já existe um perfil FFmpeg chamado \"main\"."
        );
        assert_eq!(
            ffmpeg_profile_error_text(UiLanguage::Dutch, "Source profile not found."),
            "Bronprofiel niet gevonden."
        );
        assert_eq!(
            ffmpeg_profile_error_text(UiLanguage::Dutch, "custom database failure"),
            "custom database failure"
        );
    }

    #[test]
    fn schedule_server_errors_follow_active_language_without_changing_unknown_values() {
        assert_eq!(
            schedule_error_text(UiLanguage::English, "Schedule name is required!"),
            "Schedule name is required!"
        );
        assert_eq!(
            schedule_error_text(UiLanguage::Polish, "Collection is required."),
            "Kolekcja jest wymagana."
        );
        assert_eq!(
            schedule_error_text(UiLanguage::PortugueseBrazil, "Search Query is required."),
            "A consulta de pesquisa é obrigatória."
        );
        assert_eq!(
            schedule_error_text(
                UiLanguage::Dutch,
                "Fixed schedule items require a start time."
            ),
            "Vaste schema-items vereisen een starttijd."
        );
        assert_eq!(
            schedule_error_text(UiLanguage::Dutch, "custom database failure"),
            "custom database failure"
        );
    }

    #[test]
    fn scheduling_content_server_errors_follow_active_language_without_changing_unknown_values() {
        assert_eq!(
            scheduling_content_error_text(UiLanguage::English, "Block name is required"),
            "Block name is required"
        );
        assert_eq!(
            scheduling_content_error_text(UiLanguage::Polish, "Template group name must be unique"),
            "Nazwa grupy szablonów musi być unikalna"
        );
        assert_eq!(
            scheduling_content_error_text(
                UiLanguage::PortugueseBrazil,
                "Template items cannot cross midnight."
            ),
            "Os itens do modelo não podem atravessar a meia-noite."
        );
        assert_eq!(
            scheduling_content_error_text(
                UiLanguage::Dutch,
                "[Collection] is required for collection type 'Collection'"
            ),
            "Collectie is verplicht voor collectietype Collectie."
        );
        assert_eq!(
            scheduling_content_error_text(UiLanguage::Dutch, "Template 12 does not exist."),
            "Sjabloon bestaat niet."
        );
        assert_eq!(
            scheduling_content_error_text(
                UiLanguage::English,
                "A deco named \"example\" already exists in that deco group"
            ),
            "A deco with that name already exists in that deco group."
        );
        assert_eq!(
            scheduling_content_error_text(
                UiLanguage::Polish,
                "Deco may only contain one [Block Start] break content"
            ),
            "Dekoracja może zawierać tylko jedną treść przerwy na początku bloku."
        );
        assert_eq!(
            scheduling_content_error_text(
                UiLanguage::PortugueseBrazil,
                "Deco from 01:00:00 to 01:30:00 intersects deco from 01:15:00 to 01:45:00"
            ),
            "A decoração se sobrepõe a outra decoração."
        );
        assert_eq!(
            scheduling_content_error_text(
                UiLanguage::Dutch,
                "Break content playlist item does not exist."
            ),
            "Afspeellijst voor onderbrekingsinhoud bestaat niet."
        );
        assert_eq!(
            scheduling_content_error_text(UiLanguage::Dutch, "custom database failure"),
            "custom database failure"
        );
    }

    #[test]
    fn watermark_server_errors_follow_active_language_without_changing_unknown_values() {
        assert_eq!(
            watermark_error_text(UiLanguage::English, "Watermark image is required"),
            "Watermark image is required"
        );
        assert_eq!(
            watermark_error_text(UiLanguage::Polish, "Watermark name must be unique"),
            "Nazwa znaku wodnego musi być unikalna"
        );
        assert_eq!(
            watermark_error_text(
                UiLanguage::PortugueseBrazil,
                "Width must be between 0 and 100%"
            ),
            "A largura deve estar entre 0 e 100%"
        );
        assert_eq!(
            watermark_error_text(UiLanguage::Dutch, "Watermark 12 does not exist."),
            "Watermerk niet gevonden."
        );
        assert_eq!(
            watermark_error_text(UiLanguage::Dutch, "custom database failure"),
            "custom database failure"
        );
    }

    #[test]
    fn settings_server_errors_follow_active_language_without_changing_unknown_values() {
        assert_eq!(
            settings_error_text(
                UiLanguage::English,
                "HLS Segmenter idle timeout must be greater than or equal to 30"
            ),
            "HLS Segmenter idle timeout must be greater than or equal to 30"
        );
        assert_eq!(
            settings_error_text(
                UiLanguage::Polish,
                "Library refresh interval must be 0 (do not refresh) or greater than zero"
            ),
            "Interwał odświeżania biblioteki musi wynosić 0 (bez odświeżania) albo więcej niż zero."
        );
        assert_eq!(
            settings_error_text(
                UiLanguage::PortugueseBrazil,
                "XMLTV days to build (5) cannot be greater than Playout days to build (2)"
            ),
            "Dias de XMLTV para construir (5) não podem ser maiores que dias de playout para construir (2)"
        );
        assert_eq!(
            settings_error_text(UiLanguage::Dutch, "ffmpeg path does not exist"),
            "ffmpeg-pad bestaat niet"
        );
        assert_eq!(
            settings_error_text(UiLanguage::Dutch, "custom database failure"),
            "custom database failure"
        );
    }

    #[test]
    fn media_action_server_errors_follow_active_language_without_changing_unknown_values() {
        assert_eq!(
            media_action_error_text(UiLanguage::English, "Collection name must be unique"),
            "Collection name must be unique"
        );
        assert_eq!(
            media_action_error_text(UiLanguage::Polish, "Collection name is required"),
            "Nazwa kolekcji jest wymagana."
        );
        assert_eq!(
            media_action_error_text(UiLanguage::PortugueseBrazil, "Playlist does not exist."),
            "A playlist não existe."
        );
        assert_eq!(
            media_action_error_text(UiLanguage::Dutch, "Unable to locate media item"),
            "Kan media-item niet vinden."
        );
        assert_eq!(
            media_action_error_text(UiLanguage::Dutch, "custom database failure"),
            "custom database failure"
        );
    }

    #[test]
    fn filler_server_errors_follow_active_language_without_changing_unknown_values() {
        assert_eq!(
            filler_error_text(UiLanguage::English, "Filler preset name must be unique"),
            "Filler preset name must be unique"
        );
        assert_eq!(
            filler_error_text(UiLanguage::Polish, "Duration must not be empty"),
            "Czas trwania nie może być pusty."
        );
        assert_eq!(
            filler_error_text(
                UiLanguage::PortugueseBrazil,
                "Use Chapters As Media Items must not be enabled for fallback filler"
            ),
            "Usar capítulos como itens de mídia não pode estar ativado para preenchimento de fallback."
        );
        assert_eq!(
            filler_error_text(UiLanguage::Dutch, "FillerPreset 42 does not exist."),
            "Fillerpreset bestaat niet."
        );
        assert_eq!(
            filler_error_text(UiLanguage::Dutch, "custom database failure"),
            "custom database failure"
        );
    }

    #[test]
    fn media_browser_server_errors_follow_active_language_without_changing_unknown_values() {
        assert_eq!(
            media_browser_error_text(UiLanguage::English, "MultiCollection 7 does not exist."),
            "Multi-collection does not exist."
        );
        assert_eq!(
            media_browser_error_text(UiLanguage::Polish, "Smart collection query is required"),
            "Zapytanie kolekcji inteligentnej jest wymagane."
        );
        assert_eq!(
            media_browser_error_text(
                UiLanguage::PortugueseBrazil,
                "Playlist name \"example\" is invalid"
            ),
            "O nome da playlist é inválido."
        );
        assert_eq!(
            media_browser_error_text(
                UiLanguage::Dutch,
                "[MultiCollection] is required for collection type 'MultiCollection'"
            ),
            "Multi-collectie is verplicht."
        );
        assert_eq!(
            media_browser_error_text(UiLanguage::Dutch, "[ProgramScheduleId] does not exist."),
            "Schema bestaat niet."
        );
        assert_eq!(
            media_browser_error_text(UiLanguage::Polish, "[MediaItem] does not exist."),
            "Element multimedialny nie istnieje."
        );
        assert_eq!(
            media_browser_error_text(UiLanguage::Dutch, "custom database failure"),
            "custom database failure"
        );
    }

    #[test]
    fn media_source_server_errors_follow_active_language_without_changing_unknown_values() {
        assert_eq!(
            media_source_error_text(UiLanguage::English, "Address is required."),
            "Address is required."
        );
        assert_eq!(
            media_source_error_text(UiLanguage::Polish, "API key is required."),
            "Klucz API jest wymagany."
        );
        assert_eq!(
            media_source_error_text(
                UiLanguage::PortugueseBrazil,
                "Could not reach Jellyfin: connection refused"
            ),
            "Não foi possível acessar o Jellyfin: connection refused"
        );
        assert_eq!(
            media_source_error_text(UiLanguage::Dutch, "'Address' must be a valid URL"),
            "Adres moet een geldige URL zijn."
        );
        assert_eq!(
            media_source_error_text(UiLanguage::Dutch, "custom database failure"),
            "custom database failure"
        );
    }

    #[test]
    fn trakt_server_errors_follow_active_language_without_changing_unknown_values() {
        assert_eq!(
            trakt_error_text(UiLanguage::English, "Invalid Trakt list url"),
            "Invalid Trakt list URL."
        );
        assert_eq!(
            trakt_error_text(
                UiLanguage::Polish,
                "conflict: Trakt ClientId is not configured; set TRAKT__CLIENTID"
            ),
            "Identyfikator klienta Trakt nie jest skonfigurowany; ustaw TRAKT__CLIENTID."
        );
        assert_eq!(
            trakt_error_text(
                UiLanguage::PortugueseBrazil,
                "failed to decode trakt http: status 404 Not Found"
            ),
            "Não foi possível acessar o Trakt: status 404 Not Found"
        );
        assert_eq!(
            trakt_error_text(UiLanguage::Dutch, "trakt list with id 42 not found"),
            "Trakt-lijst bestaat niet."
        );
        assert_eq!(
            trakt_error_text(UiLanguage::Dutch, "custom database failure"),
            "custom database failure"
        );
    }

    #[test]
    fn upload_server_errors_follow_active_language_without_changing_unknown_values() {
        assert_eq!(
            upload_error_text(UiLanguage::English, "Upload exceeds maximum allowed size"),
            "Upload exceeds maximum allowed size."
        );
        assert_eq!(
            upload_error_text(UiLanguage::Polish, "No file field"),
            "Nie znaleziono pola pliku."
        );
        assert_eq!(
            upload_error_text(
                UiLanguage::PortugueseBrazil,
                "Write failed: permission denied"
            ),
            "Falha ao gravar: permission denied"
        );
        assert_eq!(
            upload_error_text(UiLanguage::Dutch, "Read field: connection reset"),
            "Veld lezen mislukt: connection reset"
        );
        assert_eq!(
            upload_error_text(UiLanguage::Dutch, "custom upload failure"),
            "custom upload failure"
        );
    }
}
