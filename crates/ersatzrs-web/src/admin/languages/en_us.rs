use crate::admin::i18n::{LanguageStrings, TextKey};

pub const SORT_ORDER: u16 = 10;
pub const CODE: &str = "en-us";
pub const LABEL: &str = "English";
pub const ALIASES: &[&str] = &["en", "en_us", "en-us"];

pub struct EnglishStrings;

pub static STRINGS: EnglishStrings = EnglishStrings;

fn episode_display_subtitle(
    show_title: Option<&str>,
    season_number: Option<i64>,
    episode_number: Option<i64>,
) -> Option<String> {
    let show = show_title.unwrap_or("").trim();
    match (season_number, episode_number) {
        (Some(season), Some(episode)) if !show.is_empty() => {
            Some(format!("{show} - S{season:02}E{episode:02}"))
        }
        (Some(season), Some(episode)) => Some(format!("S{season:02}E{episode:02}")),
        _ => (!show.is_empty()).then(|| show.to_owned()),
    }
}

impl LanguageStrings for EnglishStrings {
    fn text(&self, key: TextKey) -> &'static str {
        text(key)
    }

    fn selected_count(&self, count: usize) -> String {
        format!("{count} selected")
    }

    fn item_count(&self, count: u64) -> String {
        if count == 1 {
            "1 item".to_owned()
        } else {
            format!("{count} items")
        }
    }

    fn range_of(&self, start: u32, end: u32, total: u64) -> String {
        format!("{start}-{end} of {total}")
    }

    fn match_status(&self, matched: u64, total: u64) -> String {
        format!("{matched} of {total}")
    }

    fn rows_per_page(&self, page_size: u32) -> String {
        format!("Rows per page: {page_size}")
    }

    fn page_of(&self, page: u32, total: u32) -> String {
        format!("Page {page} of {total}")
    }

    fn save_failed(&self, error: &dyn std::fmt::Display) -> String {
        format!("Save failed: {error}")
    }

    fn delete_failed(&self, error: &dyn std::fmt::Display) -> String {
        format!("Delete failed: {error}")
    }

    fn empty_trash_failed(&self, error: &dyn std::fmt::Display) -> String {
        format!("Empty trash failed: {error}")
    }

    fn search_count(&self, total: u32, label: &'static str) -> String {
        format!("{total} {label}")
    }

    fn selected_items_count(&self, count: usize) -> String {
        if count == 1 {
            "1 Item Selected".to_owned()
        } else {
            format!("{count} Items Selected")
        }
    }

    fn delete_selected_count(&self, count: usize) -> String {
        format!("Delete Selected ({count})")
    }

    fn deleted_items_from_database(&self, count: u64) -> String {
        if count == 1 {
            "Deleted 1 item from the database.".to_owned()
        } else {
            format!("Deleted {count} items from the database.")
        }
    }

    fn season_display_name(&self, number: i64) -> String {
        if number == 0 {
            "Specials".to_owned()
        } else {
            format!("Season {number}")
        }
    }

    fn episode_display_subtitle(
        &self,
        show_title: Option<&str>,
        season_number: Option<i64>,
        episode_number: Option<i64>,
    ) -> Option<String> {
        episode_display_subtitle(show_title, season_number, episode_number)
    }

    fn media_item_fallback_label(&self, id: i64) -> String {
        format!("Media Item #{id}")
    }

    fn schedule_item_fallback_label(&self, id: i64) -> String {
        format!("Schedule Item #{id}")
    }

    fn schedule_error_message(&self, error: &str) -> String {
        schedule_error_message(error)
    }

    fn scheduling_content_error_message(&self, error: &str) -> String {
        scheduling_content_error_message(error)
    }

    fn watermark_error_message(&self, error: &str) -> String {
        watermark_error_message(error)
    }

    fn channel_streaming_mode_label(&self, mode: i32) -> String {
        match mode {
            1 => "MPEG-TS (Legacy)".to_owned(),
            2 => "HLS Direct".to_owned(),
            4 => "HLS Segmenter".to_owned(),
            5 => "MPEG-TS".to_owned(),
            _ => format!("Unknown ({mode})"),
        }
    }

    fn channel_language_label(&self, code: &str) -> String {
        match code.trim().to_ascii_lowercase().as_str() {
            "" => String::new(),
            "eng" | "en" => "English".to_owned(),
            "nld" | "dut" | "nl" => "Dutch".to_owned(),
            "deu" | "ger" | "de" => "German".to_owned(),
            "fra" | "fre" | "fr" => "French".to_owned(),
            "spa" | "es" => "Spanish".to_owned(),
            "ita" | "it" => "Italian".to_owned(),
            "jpn" | "ja" => "Japanese".to_owned(),
            "kor" | "ko" => "Korean".to_owned(),
            "zho" | "chi" | "zh" => "Chinese".to_owned(),
            value => value.to_owned(),
        }
    }

    fn media_kind_label(&self, kind: i64) -> String {
        match kind {
            1 => text(TextKey::Movies),
            2 => text(TextKey::Shows),
            3 => text(TextKey::MusicVideos),
            4 => text(TextKey::OtherVideos),
            5 => text(TextKey::Songs),
            6 => text(TextKey::Images),
            7 => text(TextKey::RemoteStreams),
            _ => "Unknown",
        }
        .to_owned()
    }

    fn playout_schedule_kind_label(&self, kind: i32) -> String {
        match kind {
            1 => text(TextKey::ScheduleKindClassic),
            2 => text(TextKey::ScheduleKindBlock),
            3 => text(TextKey::ScheduleKindSequential),
            4 => text(TextKey::ScheduleKindScripted),
            20 => text(TextKey::ScheduleKindExternalJson),
            _ => "",
        }
        .to_owned()
    }

    fn library_kind_label(&self, kind: &str) -> String {
        match kind {
            "Local" => text(TextKey::Local).to_owned(),
            "Plex" | "Jellyfin" | "Emby" => kind.to_owned(),
            _ => kind.to_owned(),
        }
    }

    fn hardware_acceleration_option_label(&self, label: &str, supported: bool) -> String {
        if supported {
            label.to_owned()
        } else {
            format!("{label} (not available on this system)")
        }
    }

    fn ffmpeg_tonemap_algorithm_label(&self, algorithm: i32) -> String {
        match algorithm {
            0 => "Linear",
            1 => "Clip",
            2 => "Gamma",
            3 => "Reinhard",
            4 => "Mobius",
            5 => "Hable",
            _ => return format!("Unknown ({algorithm})"),
        }
        .to_owned()
    }

    fn ffmpeg_output_dynamic_range_label(&self, range: i32) -> String {
        match range {
            0 => "SDR (BT.709)",
            1 => "HDR10 (BT.2020 PQ)",
            _ => return format!("Unknown ({range})"),
        }
        .to_owned()
    }

    fn ffmpeg_loudness_mode_label(&self, mode: i32) -> String {
        match mode {
            0 => text(TextKey::Off),
            1 => "loudnorm",
            _ => return format!("Unknown ({mode})"),
        }
        .to_owned()
    }

    fn ffmpeg_profile_video_format_label(&self, format: i32, hardware_acceleration: i32) -> String {
        let label = ffmpeg_profile_video_format_label(format);
        if hardware_acceleration == 0 {
            label
        } else {
            format!(
                "{} / {}",
                label,
                ffmpeg_profile_hardware_acceleration_label(hardware_acceleration)
            )
        }
    }

    fn ffmpeg_profile_audio_format_label(&self, format: i32) -> String {
        ffmpeg_profile_audio_format_label(format)
    }

    fn ffmpeg_profile_error_message(&self, error: &str) -> String {
        ffmpeg_profile_error_message(error)
    }

    fn settings_error_message(&self, error: &str) -> String {
        settings_error_message(error)
    }

    fn media_action_error_message(&self, error: &str) -> String {
        media_action_error_message(error)
    }

    fn filler_error_message(&self, error: &str) -> String {
        filler_error_message(error)
    }

    fn media_browser_error_message(&self, error: &str) -> String {
        media_browser_error_message(error)
    }

    fn media_source_error_message(&self, error: &str) -> String {
        media_source_error_message(error)
    }

    fn addon_media_list_error_message(&self, error: &str) -> String {
        addon_media_list_error_message(error)
    }

    fn addon_operation_error_message(&self, code: &str, detail: &str) -> String {
        addon_operation_error_message(code, detail)
    }

    fn upload_error_message(&self, error: &str) -> String {
        upload_error_message(error)
    }

    fn emptied_trash_count(&self, count: u64) -> String {
        format!("Emptied trash: deleted {count} items.")
    }

    fn select_collection_to_add_selected_items(&self, count: usize) -> String {
        format!("Select the collection to add the {count} selected items")
    }

    fn select_playlist_to_add_selected_items(&self, count: usize) -> String {
        format!("Select the playlist to add the {count} selected items")
    }

    fn added_items_to_collection(&self, count: usize) -> String {
        format!("Added {count} items to collection")
    }

    fn added_items_to_playlist(&self, count: usize) -> String {
        format!("Added {count} items to playlist")
    }

    fn add_to_collection_failed(&self, error: &dyn std::fmt::Display) -> String {
        format!("Add to collection failed: {error}")
    }

    fn add_to_playlist_failed(&self, error: &dyn std::fmt::Display) -> String {
        format!("Add to playlist failed: {error}")
    }

    fn no_media_source_configured(&self, provider: &'static str) -> String {
        format!("No {provider} source configured. Click Configure Connection to add one.")
    }

    fn provider_connection_title(&self, provider: &'static str) -> String {
        format!("{provider} Connection")
    }

    fn provider_libraries_title(&self, provider: &'static str) -> String {
        format!("{provider} Libraries")
    }

    fn provider_path_replacements_title(&self, provider: &'static str) -> String {
        format!("{provider} Path Replacements")
    }

    fn provider_path_label(&self, provider: &'static str) -> String {
        format!("{provider} Path")
    }

    fn health_summary(&self, errors: u32, warnings: u32) -> String {
        format!(
            "{} error{}, {} warning{}",
            errors,
            if errors == 1 { "" } else { "s" },
            warnings,
            if warnings == 1 { "" } else { "s" },
        )
    }

    fn classic_playout_state_message(&self, playouts: &str) -> String {
        format!(
            "Classic playout state is not safe to Refresh for: {playouts}. Use Reset Playout for legacy or incompatible state; Refresh never clears a window without a complete checkpoint."
        )
    }

    fn unable_to_load_playback_troubleshooting(&self, error: &dyn std::fmt::Display) -> String {
        format!("Unable to load playback troubleshooting context: {error}")
    }

    fn scan_refresh_state_message(
        &self,
        unresolved_resolutions: u64,
        stale_resolutions: u64,
        failed_intents: u64,
        stale_intents: u64,
    ) -> String {
        format!(
            "Scan refresh state needs attention: {unresolved_resolutions} unresolved impact resolution(s), {stale_resolutions} stale impact resolution(s), {failed_intents} failed playout refresh intent(s), and {stale_intents} stale playout refresh intent(s). Review scanner and scheduling logs, then retry the affected scan or playout refresh."
        )
    }

    fn hls_live_delivery_message(
        &self,
        recent: u64,
        minutes: u64,
        active: u64,
        fallbacks: u64,
        fallback_hours: u64,
    ) -> String {
        format!(
            "{recent} channel(s) delivered live HLS media slower than wall time in the last {minutes} minutes; {active} had not recovered when last observed; {fallbacks} channel(s) are using a verified software-encoding and/or lower-quality fallback for up to {fallback_hours} hours. Review the streaming logs and run the delivery-aware TSDuck playback gate before changing provider settings."
        )
    }

    fn artwork_cardinality_message(
        &self,
        duplicate_rows: [i64; 3],
        projection_counts: [i64; 5],
    ) -> String {
        let [show_duplicate_rows, season_duplicate_rows, episode_duplicate_rows] = duplicate_rows;
        let [published_items, projection_rows, distinct_episodes, complete_hierarchy_rows, hierarchy_episodes] =
            projection_counts;
        format!(
            "Artwork cardinality needs attention: {show_duplicate_rows} duplicate show artwork row(s), {season_duplicate_rows} duplicate season artwork row(s), and {episode_duplicate_rows} duplicate episode artwork row(s); projected television counts are {published_items} published add-on list episode item(s), {projection_rows} projection row(s), {distinct_episodes} distinct projected episode(s), {complete_hierarchy_rows} complete hierarchy projection(s), and {hierarchy_episodes} hierarchy episode(s). Refresh or republish the affected add-on television lists."
        )
    }

    fn playout_reset_prompt(&self, channel: &str, schedule: &str) -> String {
        format!(
            "Enter the time that the playout on channel {channel} with schedule {schedule} should reset every day"
        )
    }
}

pub fn text(key: TextKey) -> &'static str {
    match key {
        TextKey::ItemDetailsNotRequested => "Details not fetched",
        TextKey::ItemDetailsFailed => "Detail fetch failed",
        TextKey::ItemDetailsRetry => "Details waiting to retry",
        TextKey::ItemProviderPlayback => "Provider playback",
        TextKey::ItemPosition => "Position",
        TextKey::ItemPrevious => "Previous",
        TextKey::ItemNext => "Next",
        TextKey::ItemBack => "Back to items",
        TextKey::ItemTools => "Item tools",
        TextKey::ItemEpisodePosition => "Episode position",
        TextKey::ItemNumbered => "Season & episode",
        TextKey::ItemUnnumberedHelp => "This item will have no season or episode number when saved.",
        TextKey::ItemNumberingHelp => "A numbered item needs both values. Season 0 is for specials.",
        TextKey::ItemMetadata => "Summary, genres and tags",
        TextKey::ItemMetadataHelp => "Additional titles, languages and credits",
        TextKey::ItemSaveHelp => "Save your changes to apply them automatically.",
        TextKey::ItemEditNumbering => "Edit numbering in list",
        TextKey::ItemReplacementHelp => "Selecting or removing a replacement changes this draft. Use Save item to keep it.",
        TextKey::ItemReplacementPending => "Replacement change is unsaved.",
        TextKey::ItemSearchReplacement => "Search local media",
        TextKey::ItemRemoveReplacement => "Remove replacement",
        TextKey::ItemInformation => "Media information & provenance",
        TextKey::ItemInformationHelp => "Audio, subtitles, stream details and provider options",
        TextKey::ItemSavedToolsHelp => "These actions use the saved item. Unsaved edits are not included.",
        TextKey::ItemViewMedia => "View media details",
        TextKey::ItemSavedValue => "Saved",
        TextKey::AddonWorkspaceSelectFirst => "Select items for editing first.",
        TextKey::AddonsNoneInstalled => "No add-ons are installed yet.",
        TextKey::AddonsNoneAvailable => "No add-ons are available from your sources yet.",
        TextKey::AddonsSetupHelp => "Open Sources to add a repository or install from a file.",
        TextKey::AddonPackageHelp => "Use this row for a ZIP containing one add-on.",
        TextKey::AddonRepositoryPackageHelp => "Use this row for a repository ZIP, such as ErsatzRS-addons.zip, which contains a catalogue of add-ons.",
        TextKey::AddonPackageUploadHelp => "Could not install this add-on package. If this is a repository ZIP, use the Repository package row below.",
        TextKey::AddonRepositoryUploadHelp => "Could not add this repository package. If this ZIP contains one add-on, use the Add-on package row above.",
        TextKey::AddonConfigurationSavedRecheck => "Saved. The add-on is disabled until you check and enable it again.",
        TextKey::AddonCheckRequired => "Configuration changed. Check the add-on again before enabling it.",
        TextKey::AddonCheckPending => "Checking add-on…",
        TextKey::AddonWorkspaceIncludeSelected => "Include selected items",
        TextKey::AddonWorkspaceExcludeSelected => "Exclude selected items",
        TextKey::AddonWorkspaceInclusionHelp => "Save selection changes to apply them automatically.",
        TextKey::AddonWorkspacePublicationFailed => "Publication failed",
        TextKey::AddonWorkspacePublicationRestoredHelp => "The previous revision was restored. Saved changes are retained and retried automatically. Activity shows technical failures.",

        TextKey::AddonWorkspaceParentMetadataPreserved => "Processing stopped to preserve edited or invalid parent NFO metadata. Reconcile the file before retrying.",

        TextKey::AddonWorkspaceImporting => "Importing list…",
        TextKey::AddonWorkspaceImportingHelp => "The list will open automatically when discovery finishes. Without JavaScript, use Refresh.",
        TextKey::AddonWorkspaceImportSteps => "Import the list. Metadata and required media are processed and published automatically.",
        TextKey::AddonWorkspaceChooseProvider => "Choose an add-on below if none is detected for this URL.",
        TextKey::AddonWorkspaceExcluded => "Excluded from publication",
        TextKey::AddonWorkspaceSort => "Sort order",
        TextKey::AddonWorkspaceAscending => "Ascending",
        TextKey::AddonWorkspaceDescending => "Descending",
        TextKey::AddonWorkspaceItemDate => "Date",
        TextKey::AddonWorkspaceSelectVisible => "Select visible items",
        TextKey::AddonWorkspaceDeselectVisible => "Deselect visible items",
        TextKey::ItemFromList => "From list",
        TextKey::ItemUseListSetting => "Use list setting",
        TextKey::ItemOverride => "Item override",
        TextKey::ItemPackageDefault => "Package default; not yet activated",
        TextKey::ItemPreviewOptions => "Preview option changes",
        TextKey::ItemAccessActive => "An access check is already in progress.",
        TextKey::OpenItemInNewTab => "Open in a new tab",
        TextKey::BulkItemsToChange => "Items to change",
        TextKey::BulkScopeHelp => "Numbering, season detection and media kind changes use these items. The length filter applies to numbering and season detection.",
        TextKey::BulkDetectSeasons => "Detect seasons",
        TextKey::BulkFirstSeason => "First season",
        TextKey::BulkReviewChanges => "Review changes",
        TextKey::BulkPreviewSaves => "Applying saves these values; saved changes are published automatically.",
        TextKey::BulkPreviewNothing => "This proposal changes nothing.",
        TextKey::BulkApplyNumbering => "Apply numbering changes",
        TextKey::BulkApplyChanges => "Apply changes",
        TextKey::BulkCancelPreview => "Cancel preview",
        TextKey::ActiveFilter => "Active filter",
        TextKey::AddonWorkspaceLengthFilter => "Filter by media length",
        TextKey::AddonWorkspaceMinimumLength => "Minimum length",
        TextKey::AddonWorkspaceMaximumLength => "Maximum length",
        TextKey::AddonWorkspaceUnknownLength => "Include unknown lengths",
        TextKey::AddonWorkspaceLengthHelp => "Minutes:seconds. Leave blank for no limit.",
        TextKey::AddonWorkspaceReviewLatest => "Review latest saved version",
        TextKey::AddonWorkspaceReviewRetained => "Your input is retained. Review it against the latest saved values before saving. Other open tasks must be reviewed separately.",
        TextKey::AddonWorkspaceStreamHelp => "Play directly from the source.",
        TextKey::AddonWorkspaceDownloadHelp => "Prepare each video before it plays.",
        TextKey::AddonWorkspaceNoOptions => "This add-on does not provide access or segment settings.",
        TextKey::AddonWorkspaceNewHelp => "Download newly imported videos; stream existing ones.",
        TextKey::AddonWorkspaceSizeLimit => "This form is too large. Go back to your input and save a smaller task.",
        TextKey::AddonWorkspaceItems => "Items",
        TextKey::AddonWorkspaceSettings => "List settings",
        TextKey::AddonWorkspacePlayback => "Playback & access",
        TextKey::AddonWorkspaceActivity => "Activity & recovery",
        TextKey::AddonWorkspaceSections => "List sections",
        TextKey::AddonWorkspaceSaveSettings => "Save settings",
        TextKey::AddonWorkspaceSaveItem => "Save item",
        TextKey::AddonWorkspaceSaveNumbering => "Save numbering",
        TextKey::AddonWorkspacePublish => "Publish saved changes",
        TextKey::AddonWorkspacePublished => "Published list is up to date",
        TextKey::AddonWorkspacePending => "Saved changes are being applied automatically",
        TextKey::AddonWorkspacePreparationRequired => "Required media is prepared automatically",
        TextKey::AddonWorkspacePreparing => "Preparing media for publication",
        TextKey::AddonWorkspaceAddIncomplete => "The list was added, but some of its settings were not saved. Open List settings, check them and save again.",
        TextKey::AddonWorkspacePreparationFailed => "Media preparation failed",
        TextKey::RetryPreparation => "Retry preparation",
        TextKey::PreparationFailedItems => "Preparation tasks",
        TextKey::PreparationFailureFailed => "The add-on could not prepare the media. Retry preparation, and check the add-on if it keeps failing.",
        TextKey::PreparationFailureInputChanged => "The preparation request no longer matched the saved list or the installed add-on. Retry preparation.",
        TextKey::PreparationFailureInvalidRequest => "The saved options do not match the installed add-on. Review Playback & access, save, and retry preparation.",
        TextKey::PreparationFailureCancelled => "Preparation was cancelled. Retry preparation to start again.",
        TextKey::PreparationFailureAttemptsExhausted => "Preparation stopped after repeated failed attempts. Retry preparation, and check the add-on if it keeps failing.",
        TextKey::PreparationFailureStorageFull => "There was not enough storage space to prepare the media. Free up space, then retry preparation.",
        TextKey::PreparationFailureUnsupported => "The add-on cannot prepare this media.",
        TextKey::PreviousSavedRevision => "Previous saved revision",
        TextKey::AddonWorkspacePreparationActivity => "Media preparation",
        TextKey::AddonWorkspaceBulk => "Edit multiple items",
        TextKey::AddonWorkspaceSelection => "Selected for editing",
        TextKey::AddonWorkspaceOffPage => "On other pages",
        TextKey::AddonWorkspaceDiscard => "Discard edits",
        TextKey::AddonWorkspaceSelected => "Selected items",
        TextKey::AddonWorkspaceMatching => "All matching items",
        TextKey::AddonWorkspacePage => "Current page",
        TextKey::AddonWorkspaceAccess => "Access and segments",
        TextKey::AddonWorkspaceSaveConflict => "The saved settings changed. Review the current version before saving.",
        TextKey::AddonWorkspaceSaveInvalid => "These settings could not be validated. Review your input and try again.",
        TextKey::AddonWorkspaceSaveStorage => "The settings could not be stored. Your input is retained; try Save again.",
        TextKey::AddonWorkspaceLoadIncomplete => "Some editor information could not be loaded consistently. Your input is retained. Review the current version to try again.",
        TextKey::TechnicalDetails => "Technical details",
        TextKey::AddonPermissionNetworkHelp => "Connect to the listed network hosts",
        TextKey::AddonPermissionMediaHelp => "Read media available to ErsatzRS",
        TextKey::AddonPermissionFfmpegHelp => "Run the managed FFmpeg runtime",
        TextKey::AddonPermissionCommandHelp => "Run the listed external commands",
        TextKey::PreparationTasks => "Preparation tasks",
        TextKey::PreparationAffectedItems => "Affected items",
        TextKey::PreparationIntervals => "Interval / SponsorBlock tasks",
        TextKey::PreparationDownloads => "Download tasks",
        TextKey::PreparationSubtitles => "Subtitle tasks",
        TextKey::PreparationQueued => "Queued",
        TextKey::PreparationRunning => "Running",
        TextKey::PreparationWaiting => "Waiting",
        TextKey::PreparationNotQueued => "Not queued",
        TextKey::LastActivity => "Last activity",
        TextKey::AddonAccessUnknownFailure => "The cause of this access failure is unknown. Check the saved access settings, test the source in your browser, then retry the access check.",
        TextKey::AddonWorkspaceRetained => "Changes could not be saved. Your input has been retained; review the errors or reload the current revision.",
        TextKey::AddonWorkspaceNavigation => "Navigation retains unsaved edits. Saved changes are applied automatically.",
        TextKey::AddonWorkspaceSeasonOnly => "Change season only",
        TextKey::AddonWorkspaceSeasonPerYear => "New season each calendar year",
        TextKey::AddonWorkspaceYearDatesIncomplete => "Upload dates are incomplete. Wait for details or enter the missing dates before grouping seasons by year.",
        TextKey::AddonWorkspacePageSize => "Items per page",

        TextKey::AddonAccessTest => "Test access",
        TextKey::AddonAccessHelp => {
            "Check this item using its saved draft options. Unsaved changes are not included; published playback is unchanged."
        }
        TextKey::AddonAccessSaveFirst => {
            "Save an option first to activate the draft settings before testing access."
        }
        TextKey::AddonAccessError => {
            "The access check could not be loaded or changed. Refresh the item and try again."
        }
        TextKey::AddonAccessNotChecked => "Not checked",
        TextKey::AddonAccessQueued => "Access check queued",
        TextKey::AddonAccessRunning => "Checking access",
        TextKey::AddonAccessWaiting => "Waiting to retry access check",
        TextKey::AddonAccessCancelled => "Access check cancelled",
        TextKey::AddonAccessStale => "Access evidence expired or its settings changed",
        TextKey::AddonAccessAccessible => "Accessible",
        TextKey::AddonAccessProfileUnavailable => "The selected browser profile is unavailable to the server. Review the saved browser profile and check access again.",
        TextKey::AddonAccessProfileLocked => "The browser profile is locked. Close the browser using that profile, then retry the access check.",
        TextKey::AddonAccessDecryptionFailed => "The server could not decrypt the browser credentials. Review the browser profile and the account running the server, then retry.",
        TextKey::AddonDerivedMetadataLimited => "Show metadata reached its size limit. Episode metadata remains complete.",
        TextKey::AddonLocalPlaybackAvailable => "Local copy playable",
        TextKey::AddonAccessRestricted => "The provider restricted access. Open the source in your browser to review account or regional restrictions, then retry the access check.",
        TextKey::AddonAccessNextAttempt => "Next attempt:",
        TextKey::AddonAccessExpires => "Evidence expires:",
        TextKey::AddonAccessRefresh => "Refresh status",
        TextKey::AddonOptionsCurrentFilter => "Current filter",
        TextKey::AddonOptions => "Provider options",
        TextKey::AddonOptionsDraftHelp => "Saved options are applied automatically. Current playback remains available during processing.",
        TextKey::AddonOptionsProvisional => {
            "These package defaults are provisional. Saving an option activates them for this draft."
        }
        TextKey::AddonOptionsListDefault => "List default",
        TextKey::AddonOptionsInherited => "Inherited",
        TextKey::AddonOptionsOverride => "Override",
        TextKey::AddonOptionsFiniteOnly => {
            "Unavailable for live or unknown-duration items: this option requires finite media. Direct playback remains available."
        }
        TextKey::AddonOptionsResetPackage => "Reset to package default",
        TextKey::AddonOptionsResetList => "Reset to list default",
        TextKey::AddonOptionsMixed => "Mixed values — choose a value",
        TextKey::AddonOptionsPreviewHelp => {
            "Review these exact items before applying. An intervening draft or schema change requires a new preview."
        }
        TextKey::AddonOptionsBefore => "Before",
        TextKey::AddonOptionsAfter => "After",
        TextKey::AddonOptionsConflict => {
            "Options could not be saved or loaded. The draft, selection, or package schema may have changed. Reload the editor and try again."
        }
        TextKey::AddonOptionsNone => "This package does not declare options.",
        TextKey::Addon => "Add-on",
        TextKey::PrimaryFeature => "Main feature",
        TextKey::MovieExtra => "Movie extra",
        TextKey::NeedReview => "Notices",

        TextKey::ReviewRecommended => "Review recommended",
        TextKey::PublicationReadiness => "Publication readiness",
        TextKey::PublicationBlockers => "Blockers",
        TextKey::PublicationActivity => "Activity",
        TextKey::PublicationJobs => "publication jobs",
        TextKey::PublicationEvents => "events",
        TextKey::NoPublicationActivity => "No publication activity has been recorded yet.",
        TextKey::CopyRedactedDiagnostics => "Copy redacted diagnostics",
        TextKey::DownloadRedactedDiagnostics => "Download redacted diagnostics",
        TextKey::RedactedDiagnosticsCopied => "Redacted diagnostics copied.",
        TextKey::PublicationGeneration => "Generation",
        TextKey::CorrelationId => "Correlation ID",
        TextKey::TargetPublishedRevision => "Target published revision",
        TextKey::RelatedOperation => "Related operation",
        TextKey::FilesystemTransition => "Filesystem transition",
        TextKey::RetryClass => "Retry class",
        TextKey::RecoveryAction => "Recovery action",
        TextKey::OperatingSystemErrorCode => "Operating-system error code",
        TextKey::PublicationOutcome => "Outcome",
        TextKey::PublicationExpired => "Expired",
        TextKey::EventExistingJobAdopted => "Existing publication adopted",
        TextKey::EventEnqueued => "Publication queued",
        TextKey::EventLeaseWaitStarted => "Waiting for library access",
        TextKey::EventLeaseAcquired => "Library access acquired",
        TextKey::EventLeaseReclaimed => {
            "Publication resumed after an interruption or expired lease"
        }
        TextKey::EventPhaseAdvanced => "Publication phase advanced",
        TextKey::EventChildScanStarted => "Verification scan started",
        TextKey::EventChildScanCompleted => "Verification scan completed",
        TextKey::EventChildScanFailed => "Verification scan failed",
        TextKey::EventOriginalFailure => "Original publication failure",
        TextKey::EventRecoveryFailure => "Recovery failure",
        TextKey::EventRetryScheduled => "Recovery retry scheduled",
        TextKey::EventRecoveryRequested => "Recovery requested",
        TextKey::EventCompleted => "Publication completed",
        TextKey::EventHistoryTruncated => "Older activity was truncated",
        TextKey::TriggerAutomaticRefresh => "Scheduled refresh",
        TextKey::TriggerOperator => "Requested by an operator",
        TextKey::TriggerRestoreRevision => "Requested by revision restore",
        TextKey::TestSource => "Test source",
        TextKey::TestSourceHelp => "Check the saved source without importing or changing the list.",
        TextKey::SourceTestQueued => "Source test queued",
        TextKey::SourceTestRunning => "Source test running…",
        TextKey::SourceTestStale => {
            "The source test is stale because its saved inputs no longer match the current list, add-on package, or defaults."
        }
        TextKey::SourceAvailable => "Source is available",
        TextKey::SourceUnsupported => "Source is not supported",
        TextKey::SourceTestFailed => "Source test failed",
        TextKey::SourceRevision => "Source revision",
        TextKey::FirstPageComplete => "First page complete",
        TextKey::ProviderIdentity => "Provider identity",
        TextKey::ReturnedFirstPageItems => "Items returned on the first page",
        TextKey::ProviderTotalItemCount => "Provider total item count",
        TextKey::DraftChanges => "Saved draft changes",
        TextKey::ExactDraftDiffHelp => {
            "Exact saved draft differences from the current published revision."
        }
        TextKey::DraftDiffUnavailable => {
            "An exact draft comparison is unavailable until a complete revision is published."
        }
        TextKey::MetadataChanges => "Metadata changes",
        TextKey::ExcludedItems => "Excluded items",
        TextKey::Conflicts => "Conflicts",
        TextKey::PendingRemovals => "Pending removals",
        TextKey::PendingRemovalHelp => {
            "Missing once in a complete reconciliation. Confirm removal now, or wait for a second complete observation at least 24 hours later."
        }
        TextKey::Reappearances => "Reappearances",
        TextKey::RemovalCandidates => "Removal candidates",
        TextKey::ConfirmRemoval => "Confirm removal",
        TextKey::RemovalConfirmed => "Removal confirmed.",
        TextKey::RemovalConfirmationFailed => "Removal confirmation failed.",
        TextKey::RevisionHistory => "Revision history",
        TextKey::RevisionHistoryHelp => {
            "Restore replaces the current draft and publishes the selected snapshot as a new revision."
        }
        TextKey::NoRevisionHistory => "No published revisions are retained yet.",
        TextKey::Revision => "Revision",
        TextKey::CompareRevisions => "Compare revisions",
        TextKey::RestoreRevision => "Restore revision",
        TextKey::RestoreRevisionWarning => "This replaces the current draft and publishes revision",
        TextKey::RestoreRevisionQueued => "Revision restore queued",
        TextKey::RestoreRevisionFailed => "Could not restore the revision.",
        TextKey::PublishedItems => "Published items",
        TextKey::GeneratedPlaylists => "Generated playlists",
        TextKey::ActiveOrFuturePlayouts => "Active or future playouts",
        TextKey::EffectiveOptions => "Effective options",
        TextKey::Files => "Files",
        TextKey::Artifacts => "Artifacts",
        TextKey::Added => "Added",
        TextKey::Removed => "Removed",
        TextKey::Changed => "Changed",
        TextKey::FilesystemPreviousBackedUp => "Previous revision backed up",
        TextKey::FilesystemCandidateCommitted => "Candidate revision committed",
        TextKey::FilesystemPreviousRestored => "Previous revision restored",
        TextKey::FilesystemProjectionRestored => "Database projection restored",
        TextKey::FilesystemRevisionVerified => "Revision verified",
        TextKey::FilesystemCleanupCompleted => "Publication cleanup completed",
        TextKey::RetryTransientRecovery => "Transient recovery",
        TextKey::ErrorFilesystemIo => "Filesystem I/O",
        TextKey::ErrorPermissionDenied => "Permission denied",
        TextKey::ErrorSharingViolation => "File sharing violation",
        TextKey::ErrorUnsafeFilesystemState => "Unsafe filesystem state",
        TextKey::ErrorInvalidPublicationMarker => "Invalid publication marker",
        TextKey::ErrorPublicationIntegrity => "Publication integrity check",
        TextKey::OutcomePublished => "Published",
        TextKey::OutcomeRestoredAfterFailure => "Previous revision restored after failure",
        TextKey::ReviewRecommendations => "Review recommendations",
        TextKey::OptionalDetails => "Optional details",
        TextKey::DraftSaved => "Draft saved",
        TextKey::SavedRevision => "Saved revision",
        TextKey::PublishList => "Publish list",
        TextKey::PreparePublication => "Prepare publication",
        TextKey::ProcessedMediaRequired => "Preparation tasks",
        TextKey::ProcessedMediaQueued => "Media preparation queued",
        TextKey::PreparingProcessedMedia => "Preparing media files",
        TextKey::PreparingPublication => "Preparing publication",
        TextKey::Publishing => "Publishing…",
        TextKey::PublicationQueued => "Publication queued",
        TextKey::RestoringPublication => "Restoring previous revision…",
        TextKey::Published => "Published",
        TextKey::PublishSavedRevisionHelp => {
            "Publish uses the exact saved revision. Save list changes first."
        }
        TextKey::ChangesSavedImmediately => {
            "Item and bulk changes are saved immediately and can be undone."
        }
        TextKey::ReviewItem => "Review item",
        TextKey::ShowFullDescription => "Show full description",
        TextKey::ShowLess => "Show less",
        TextKey::ImportAList => "Import a list",
        TextKey::ImportOverview => "Import overview",
        TextKey::AddonContextSelected => "Selected add-on",
        TextKey::CompletedIn => "Completed in",
        TextKey::NoPublicationBlockers => "No publication blockers.",
        TextKey::BlockerOverviewIncomplete => "The complete overview must be verified.",
        TextKey::BlockerPublicationInProgress => "Publication is already in progress.",
        TextKey::BlockerRecoveryRequired => "A previous publication still requires recovery.",
        TextKey::BlockerCollectionTitleMissing => "The collection needs a title.",
        TextKey::BlockerInvalidArtwork => "Artwork must use a valid bounded HTTPS URL.",
        TextKey::BlockerNoIncludedItems => "Include at least one item.",
        TextKey::BlockerMoviePrimaryCount => "Movie publication needs exactly one main movie.",
        TextKey::BlockerPlaybackDecisionRequired => "Choose finite or live playback behavior.",
        TextKey::BlockerPlaybackDetailsUnavailable => {
            "Required playback details are unavailable from this add-on."
        }
        TextKey::BlockerPlaybackDetailsFailed => {
            "Provider details do not contain a positive duration."
        }
        TextKey::BlockerUnavailableWithoutReplacement => {
            "The unavailable item needs a valid local replacement."
        }
        TextKey::BlockerItemTitleMissing => "The item needs a title.",
        TextKey::BlockerSeasonEpisodeMissing => "Enter a valid season and episode number.",
        TextKey::BlockerArtistMissing => "Enter at least one artist.",
        TextKey::PublishUnsavedChanges => "Save changes before publishing this revision.",
        TextKey::PublicationBlocked => "Publication is blocked.",
        TextKey::PublishBlocked => "Publish blocked",
        TextKey::RetryRecovery => "Retry recovery",
        TextKey::RestorePreviousRevision => "Restore previous revision",
        TextKey::PublicationPhase => "Publication phase",
        TextKey::PublicationAttempts => "Attempts",
        TextKey::LastProgress => "Last progress",
        TextKey::FailureReason => "Original reason",
        TextKey::RestrictedItemsExcluded => "Restricted items excluded from this revision",
        TextKey::RecoveryQueued => "Recovery queued",
        TextKey::RecoveryFailed => "Recovery could not be queued.",
        TextKey::PhaseAwaitingLease => "Waiting for library access",
        TextKey::PhaseStaging => "Staging revision",
        TextKey::PhaseScanning => "Scanning staged revision",
        TextKey::PhaseCommitting => "Committing revision",
        TextKey::PhaseRestoringFiles => "Restoring files",
        TextKey::PhaseRestoringProjection => "Restoring database projection",
        TextKey::PhaseReconciling => "Reconciling restored revision",
        TextKey::PhaseVerifying => "Verifying revision",
        TextKey::PhaseCleaningUp => "Cleaning up publication files",
        TextKey::PhaseComplete => "Complete",
        TextKey::InstallLeavesDisabled => {
            "Installing leaves this add-on disabled until you enable it."
        }
        TextKey::EnableActivatesPermissions => {
            "Enabling activates the add-on's declared permissions."
        }
        TextKey::FirstAired => "First aired",
        TextKey::LoadItemDetailsAfterPublishing => "Load item details automatically",
        TextKey::LoadItemDetailsHelp => {
            "Import the overview first. Playback-required technical details are loaded before the first publication; remaining extended metadata follows at a controlled rate."
        }
        TextKey::OverviewOnlyManualDetailsHelp => {
            "Import the overview only and request descriptive details manually. Playback-required technical details are still prepared automatically before publication."
        }
        TextKey::PlaybackDetailsQueued => "Playback details queued",
        TextKey::PublishAgainAfterDetails => "Completed details are applied automatically.",

        TextKey::FetchDetails => "Fetch details",
        TextKey::DetailsQueued => "Details queued",
        TextKey::DetailsLoading => "Loading details",
        TextKey::RetryDetails => "Retry details",
        TextKey::DetailsComplete => "Details complete",
        TextKey::DetailsSkipped => "Details skipped",
        TextKey::FindingProgrammes => "Finding programmes",
        TextKey::CheckingOverview => "Checking overview",
        TextKey::ReadyToReview => "Processing automatically",

        TextKey::LoadingItemDetails => "Loading item details",
        TextKey::WaitingForProvider => "Waiting for provider",
        TextKey::OverviewOnly => "Overview only",
        TextKey::PauseImport => "Pause import",
        TextKey::ResumeImport => "Resume import",
        TextKey::RetryImportNow => "Retry import now",
        TextKey::EmptyImportNeedsRetry => "Empty import needs retry",
        TextKey::EmptyImportRetryHelp => {
            "This completed import contains no items. Update the add-on, then retry; your list settings and edits will be preserved."
        }
        TextKey::CancelImport => "Cancel import",
        TextKey::OtherPageItemsAffected => "Additional affected items are on other pages",
        TextKey::ElapsedTime => "Elapsed time",
        TextKey::LastSuccessfulProgress => "Last successful progress",
        TextKey::EstimatedRemainingTime => "Estimated time remaining",
        TextKey::PlaybackBehavior => "Playback behavior",
        TextKey::UseProviderValue => "Use provider value",
        TextKey::FiniteMedia => "Finite media",
        TextKey::LiveSource => "Live source",
        TextKey::ProviderValueChanged => "Provider value changed",
        TextKey::ApplyToSelected => "Apply to selected",
        TextKey::ApplyToAllRemoteStreams => "Apply to all remote streams",
        TextKey::SelectItem => "Select item",
        TextKey::NumberEpisodes => "Number episodes",
        TextKey::NumberEpisodesHelp => {
            "Assign one season and consecutive episode numbers in the add-on's original order. Excluded items and chapter fragments are skipped."
        }
        TextKey::FirstEpisode => "First episode",
        TextKey::NumberToAssign => "Number to assign",
        TextKey::AllRemaining => "All remaining",
        TextKey::ReplaceExistingEpisodeNumbers => "Replace existing episode numbers",
        TextKey::AutoDetectSeasons => "Auto-detect seasons from first-air dates",
        TextKey::AutoDetectSeasonsHelp => {
            "A new season starts when the gap between two known first-air dates reaches the configured minimum. Items without a date continue in the current season."
        }
        TextKey::MinimumSeasonGapDays => "Minimum gap between seasons (days)",
        TextKey::NumberingStartsAt => "Numbering starts at",
        TextKey::FirstIncludedItem => "First included item",
        TextKey::StartWithFirstItem => "Start with first item",
        TextKey::StartHere => "Start here",
        TextKey::SaveSeasonEpisode => "Save season and episode",
        TextKey::BulkEdit => "Bulk edit",
        TextKey::Scope => "Scope",
        TextKey::AllItems => "All items",
        TextKey::CurrentPage => "Current page",
        TextKey::ItemsNeedingReview => "Items with notices",

        TextKey::KeepCurrentValue => "Keep current value",
        TextKey::UndoLastChange => "Undo last change",
        TextKey::Undo => "Undo",
        TextKey::ChangeUndone => "Change undone",
        TextKey::MetadataConflicts => "Metadata conflicts",
        TextKey::MetadataConflictsHelp => "The provider changed fields you customized. Your saved values remain in use.",
TextKey::PreviousProviderValue => "Previous provider value",
        TextKey::NewProviderValue => "New provider value",
        TextKey::OperatorValue => "Operator value",
        TextKey::Discovered => "Discovered",
        TextKey::Examined => "Examined",
        TextKey::ArchivedSkipped => "Archived skipped",
        TextKey::Emitted => "Emitted",
        TextKey::EditMetadata => "Edit metadata",
        TextKey::OneTagPerLine => "One tag per line",
        TextKey::OriginalTitle => "Original title",
        TextKey::ReleaseDate => "Release date",
        TextKey::Year => "Year",
        TextKey::SortTitle => "Sort title",
        TextKey::MovieFilesAndExtras => "Movie files and extras",
        TextKey::MovieFilesAndExtrasHelp => {
            "Assign exactly one main movie. Source order does not decide the role, so a trailer may appear first."
        }
        TextKey::EpisodesMetadataHelp => {
            "Review episode identity and numbering; provider order remains unchanged."
        }
        TextKey::MusicVideosMetadataHelp => {
            "Artist, album and track belong to each video, so mixed-artist lists remain valid."
        }
        TextKey::OtherVideosMetadataHelp => {
            "Use this type for independent videos rather than one movie with extras."
        }
        TextKey::SongsMetadataHelp => {
            "Track, disc, artist and album identity are edited per song; list order does not replace track numbering."
        }
        TextKey::ImagesMetadataHelp => {
            "Caption and creator are editable; format, dimensions and source identity remain factual source data."
        }
        TextKey::RemoteStreamsMetadataHelp => {
            "Edit presentation metadata here; connectivity and protocol remain provider source facts."
        }
        TextKey::CastAndCredits => "Cast and credits",
        TextKey::OneCreditPerLine => "One person per line: name | role",
        TextKey::Poster => "Poster",
        TextKey::Artwork => "Artwork",
        TextKey::PublishMetadata => "Publish",
        TextKey::IncludeItem => "Include",
        TextKey::IncludeInPublication => "Include in publication",
        TextKey::MetadataDraftNotice => "Saved metadata and inclusion changes are applied automatically.",
TextKey::ApplyToAllItemsPrompt => {
            "Every entry is reclassified with the selected media kind, replacing values the add-on derived or you set by hand."
        }
        TextKey::TelevisionEpisode => "Television Episode",
        TextKey::CollectionTypeAuto => "Determine from the items",
        TextKey::CurrentLibraryType => "Current library type",
        TextKey::CollectionTypeChangeHelp => "Save to reorganize existing items automatically. Order, inclusion and metadata edits are retained.",
TextKey::NonPlayableItems => "Non-playable items",
        TextKey::UnknownItems => "Unknown items",
        TextKey::PlayableItems => "Playable items",
        TextKey::ApplyToAllItems => "Apply to all items",
        TextKey::ChangeAddon => "Change add-on",
        TextKey::ChangeAddonHelp => "Select a different add-on to re-import the list through it.",
        TextKey::ChangeAddonPrompt => {
            "The list is re-imported through the selected add-on. Item metadata, replacements, and chapter corrections are derived again from the new provider."
        }
        TextKey::AddonAutoDetect => "Determine from the URL",
        TextKey::AddonAutoDetected => "Selected automatically from the URL.",
        TextKey::AddonMediaListNoAddonForUrl => {
            "No installed add-on handles this URL. Choose one, or set a default remote stream processor."
        }
        TextKey::DefaultRemoteStreamAddon => "Default remote stream processor",
        TextKey::DefaultRemoteStreamAddonHelp => {
            "Used for HTTP and HTTPS sources that no installed add-on claims. The add-on must still be permitted to contact the host."
        }
        TextKey::AddonDetailWait => "Item detail pacing",
        TextKey::MinimumDetailWait => "Minimum detail wait",
        TextKey::MaximumDetailWait => "Maximum detail wait",
        TextKey::AddonDetailWaitHelp => {
            "After each item-detail attempt, ErsatzRS waits a newly randomized number of seconds in this global range before any list can use the same provider host."
        }
        TextKey::AddonDetailWaitInvalid => {
            "Enter positive whole seconds and make sure the minimum does not exceed the maximum."
        }
        TextKey::Addons => "Add-ons",
        TextKey::AddonOverview => "Overview",
        TextKey::AddonUpdates => "Updates",
        TextKey::AddonSources => "Sources",
        TextKey::AddonNotFound => "Add-on not found",
        TextKey::BackToAddons => "Back to add-ons",
        TextKey::AvailableVersion => "Available version",
        TextKey::BrowseAddons => "Browse add-ons",
        TextKey::CheckForUpdates => "Check for updates",
        TextKey::InstallFromFile => "Install from file",
        TextKey::InstallRepositoryFromFile => "Install repository from file",
        TextKey::License => "License",
        TextKey::UnknownSources => "Unknown sources",
        TextKey::UnknownSourcesHelp => {
            "Allow add-ons from local Git checkouts and ZIP files. Add-ons run with the ErsatzRS service user's access."
        }
        TextKey::Verified => "Verified",
        TextKey::Unverified => "Unverified",
        TextKey::Sideloaded => "Sideloaded",
        TextKey::GitCheckout => "Git checkout",
        TextKey::OfflineBundle => "Offline bundle",
        TextKey::LegacyUnmanaged => "Legacy catalog · unmanaged",
        TextKey::LegacyAdoptionHelp => {
            "This replaces the unmanaged catalog installation with the selected local source while preserving its settings and data."
        }
        TextKey::Enabled => "Enabled",
        TextKey::Disabled => "Disabled",
        TextKey::Source => "Source",
        TextKey::ViewSourceCode => "View source code",
        TextKey::Version => "Version",
        TextKey::NextVersion => "Next engine version",
        TextKey::NoAddonsMatch => "No add-ons match these filters.",
        TextKey::AddonsUpToDate => "Everything is up to date.",
        TextKey::AddonRepositories => "Add-on repositories",
        TextKey::AvailableAddons => "Available add-ons",
        TextKey::InstalledAddons => "Installed add-ons",
        TextKey::AddRepository => "Add repository",
        TextKey::LastRefresh => "Last scanned",
        TextKey::NeverRefreshed => "Never",
        TextKey::ChooseFile => "Choose file…",
        TextKey::NoFileSelected => "No file selected",
        TextKey::EditRepository => "Edit repository",
        TextKey::OfficialRepositoryProtected => "The official repository cannot be removed.",
        TextKey::UnknownSourcesRequired => "Enable unknown sources first.",
        TextKey::NoRepositories => {
            "No local repositories found. Clone a repository into the checkout folder above, then refresh."
        }
        TextKey::AddonPackage => "Add-on package",
        TextKey::RepositoryPackage => "Repository package",
        TextKey::NoSigningKeyPinned => "No public signing key is pinned.",
        TextKey::RepositoryUrl => "Repository URL",
        TextKey::PublicSigningKey => "Public signing key",
        TextKey::DeploymentManagedRepositories => {
            "Repositories are managed by deployment configuration."
        }
        TextKey::Refresh => "Refresh",
        TextKey::Install => "Install",
        TextKey::UpdateAddon => "Update",
        TextKey::Uninstall => "Uninstall",
        TextKey::Configuration => "Configuration",
        TextKey::SecretReference => "Secret reference",
        TextKey::EnvironmentVariable => "Environment variable",
        TextKey::SecretFile => "Secret file",
        TextKey::Incompatible => "Incompatible",
        TextKey::PackageUnavailable => "Package unavailable",
        TextKey::Official => "Official",
        TextKey::ThirdParty => "Third-party repository",
        TextKey::Permissions => "Permissions",
        TextKey::Capabilities => "Capabilities",
        TextKey::Dependencies => "Dependencies",
        TextKey::Optional => "Optional",
        TextKey::MediaReadAccess => "Media read access",
        TextKey::ExternalCommands => "External commands",
        TextKey::ThirdPartyRepositoryWarning => {
            "Only load local repositories and ZIP files you trust; add-ons run as the ErsatzRS service user."
        }
        TextKey::AddCustomResolution => "Add Custom Resolution",
        TextKey::AddBlock => "Add Block",
        TextKey::AddBlockGroup => "Add Block Group",
        TextKey::AddBlockItem => "Add Block Item",
        TextKey::AddBlockToTemplate => "Add Block To Template",
        TextKey::AddCollection => "Add Collection",
        TextKey::AddContent => "Add Content",
        TextKey::AddFailed => "Add failed",
        TextKey::AddFillerPreset => "Add Filler Preset",
        TextKey::AddMultiCollection => "Add Multi Collection",
        TextKey::AddPlaylist => "Add Playlist",
        TextKey::AddPlaylistGroup => "Add Playlist Group",
        TextKey::AddPlaylistItem => "Add Playlist Item",
        TextKey::AddRerunCollection => "Add Rerun Collection",
        TextKey::AddTemplate => "Add Template",
        TextKey::AddTemplateGroup => "Add Template Group",
        TextKey::AddAddonMediaList => "Add add-on list",
        TextKey::AddToCollection => "Add To Collection",
        TextKey::AddToEllipsis => "Add To ...",
        TextKey::AddToPlaylist => "Add To Playlist",
        TextKey::AddToSchedule => "Add To Schedule",
        TextKey::Actions => "Actions",
        TextKey::All => "All",
        TextKey::AreYouSureDelete => "Are you sure you want to delete",
        TextKey::Actors => "Actors",
        TextKey::Artist => "Artist",
        TextKey::ArtistNotFound => "Artist not found",
        TextKey::AfterDurationEnd => "After Duration End",
        TextKey::AutomaticRefresh => "Automatic Refresh",
        TextKey::AddonDeliveryMode => "Delivery",
        TextKey::AddonDeliveryStream => "Stream",
        TextKey::AddonDeliveryDownloadAhead => "Download ahead",
        TextKey::AddonDeliveryNewVideos => "Download ahead — new videos only",
        TextKey::AddonDeliveryHelp => "Delivery changes take effect after the next publication. New videos only downloads videos first imported after the saved cutoff; already-known videos stream. Re-enabling this mode establishes a fresh cutoff. Only finite media can download ahead. Live items and items whose duration is unknown continue streaming.",
        TextKey::AddonDeliveryUnavailable => "Downloading requires a compatible, enabled add-on and finite media.",
        TextKey::AddonDeliveryCutoff => "New videos cutoff",
        TextKey::AddonDeliveryFreshCutoff => "A fresh cutoff is recorded when you save.",
        TextKey::AdaptiveRefreshUnavailable => {
            "Adaptive refresh is unavailable because this add-on no longer supports source checks. Choose another cadence to resume automatic refresh."
        }
        TextKey::RefreshPublicationPolicy => "Refresh publication",
        TextKey::RefreshPublicationReview => "Review every refresh",
        TextKey::RefreshPublicationAutomatic => "Automatically apply saved changes",
        TextKey::RefreshPublicationPolicyHelp => "Imports, refreshes, metadata and saved edits are applied automatically.",
        TextKey::AutoPublicationHeld => "Automatic processing needs attention",
        TextKey::PreparationWorkerUnavailable => {
            "Media preparation worker is unavailable. Restart the server before retrying preparation."
        }
        TextKey::PreparationProcessRecoveryRequired => {
            "Media preparation needs process recovery. Confirm prior provider processes have stopped before recovering retained storage and retrying preparation."
        }
        TextKey::AutoPublicationBaselineUnavailable => "A published baseline is unavailable.",
        TextKey::AutoPublicationExistingChanges => "Existing items have changes.",
        TextKey::AutoPublicationConflicts => "Metadata or replacement conflicts need review.",
        TextKey::AutoPublicationRemovals => "Items were removed.",
        TextKey::AutoPublicationRestricted => "Some additions have access restrictions.",
        TextKey::AutoPublicationUnavailable => "Some additions are unavailable.",
        TextKey::AutoPublicationOptionsChanged => "Publication options changed.",
        TextKey::AutoPublicationPreparationFailed => "Publication preparation failed.",
        TextKey::AutoPublicationConcurrentChange => "The list changed during preparation.",
        TextKey::RefreshCadenceAdaptive => "Adaptive",
        TextKey::RefreshCadenceDaily => "Daily",
        TextKey::RefreshCadenceWeekly => "Weekly",
        TextKey::RefreshCadenceMonthly => "Monthly",
        TextKey::RefreshCadenceYearly => "Yearly",
        TextKey::RefreshCadenceNever => "Never",
        TextKey::LastSuccessfulRefresh => "Last successful refresh",
        TextKey::NextScheduledRefresh => "Next scheduled refresh",
        TextKey::RefreshRetryAt => "Retry eligible at",
        TextKey::Back => "Back",
        TextKey::BackToSettings => "Back to settings",
        TextKey::BackToArtists => "Back to artists",
        TextKey::BackToCollections => "Back to collections",
        TextKey::BackToMovies => "Back to movies",
        TextKey::BackToOtherVideos => "Back to other videos",
        TextKey::BackToTvShows => "Back to TV shows",
        TextKey::BeforeDurationEnd => "Before Duration End",
        TextKey::BackToTemplates => "Back to templates",
        TextKey::Block => "Block",
        TextKey::BlockGroup => "Block Group",
        TextKey::BlockCannotBeMoved => {
            "Block cannot be moved there because it would overlap or cross midnight."
        }
        TextKey::BlockCopied => "Block copied.",
        TextKey::BlockCreated => "Block created.",
        TextKey::BlockDeleted => "Block deleted.",
        TextKey::BlockGroupCreated => "Block group created.",
        TextKey::BlockGroupDeleted => "Block group deleted.",
        TextKey::BlockGroupName => "Block Group Name",
        TextKey::BlockGroups => "Block Groups",
        TextKey::BlockItem => "Block Item",
        TextKey::BlockItems => "Block Items",
        TextKey::BlockName => "Block Name",
        TextKey::BlockNotFound => "Block not found.",
        TextKey::BlockPreviewFailed => {
            "Could not generate the block preview. Check the block items and try again."
        }
        TextKey::Blocks => "Blocks",
        TextKey::BlockToRemove => "Block To Remove",
        TextKey::BlockWillNotFit => "Block will not fit at or after the selected start time.",
        TextKey::Cancel => "Cancel",
        TextKey::Clear => "Clear",
        TextKey::Available => "Available",
        TextKey::UnknownAvailability => "Unknown",
        TextKey::LocalReplacement => "Local Replacement",
        TextKey::NoReplacement => "No replacement",
        TextKey::InvalidReplacement => "Invalid replacement",
        TextKey::Channels => "Channels",
        TextKey::Close => "Close",
        TextKey::Collections => "Collections",
        TextKey::CollectionDeleted => "Collection deleted.",
        TextKey::CollectionSaved => "Collection saved.",
        TextKey::SaveCollection => "Save Collection",
        TextKey::ComputingPreview => "Computing preview...",
        TextKey::Confirm => "Confirm",
        TextKey::ConfirmDelete => "Confirm Delete",
        TextKey::CommentaryTracks => "Commentary Tracks",
        TextKey::Content => "Content",
        TextKey::ContentVariantAcoustic => "Acoustic",
        TextKey::ContentVariantAlternateVersion => "Version",
        TextKey::ContentVariantBehindTheScenes => "Behind the Scenes",
        TextKey::ContentVariantDeletedScene => "Deleted Scene",
        TextKey::ContentVariantDirectorsCut => "Director's Cut",
        TextKey::ContentVariantExtended => "Extended",
        TextKey::ContentVariantExtra => "Extra",
        TextKey::ContentVariantFeaturette => "Featurette",
        TextKey::ContentVariantInterview => "Interview",
        TextKey::ContentVariantLive => "Live",
        TextKey::ContentVariantOfficialVideo => "Official Video",
        TextKey::ContentVariantOther => "Other",
        TextKey::ContentVariantRemaster => "Remaster",
        TextKey::ContentVariantRemix => "Remix",
        TextKey::ContentVariantScene => "Scene",
        TextKey::ContentVariantSearchHelp => {
            "Variant tokens: content_kind:extra, content_kind:alternate_version, sub_kind:featurette, sub_kind:unplugged, parent_title:\"Movie Title\", audio_title:commentary, subtitle_title:commentary, has_commentary:true"
        }
        TextKey::ContentVariantShort => "Short",
        TextKey::ContentVariantSpecialFeatures => "Extras / Special Features",
        TextKey::ContentVariantTrailer => "Trailer",
        TextKey::ContentVariantUnplugged => "Unplugged",
        TextKey::ContentVariantVersions => "Versions",
        TextKey::ContentRatings => "Content Ratings",
        TextKey::CustomResolutionHeight => "Custom resolution height",
        TextKey::CustomResolutionWidth => "Custom resolution width",
        TextKey::CustomResolutions => "Custom Resolutions",
        TextKey::Dark => "Dark",
        TextKey::DaysToBuild => "Days To Build",
        TextKey::AddBreakContent => "Add Break Content",
        TextKey::AddDeco => "Add Deco",
        TextKey::AddDecoGroup => "Add Deco Group",
        TextKey::AddDecoTemplate => "Add Deco Template",
        TextKey::AddDecoTemplateGroup => "Add Deco Template Group",
        TextKey::AtChapterMarkers => "At Chapter Markers",
        TextKey::BackToDecos => "Back to decos",
        TextKey::BackToDecoTemplates => "Back to deco templates",
        TextKey::BetweenBlockItems => "Between Block Items",
        TextKey::BlockFinish => "Block Finish",
        TextKey::BlockStart => "Block Start",
        TextKey::BreakContent => "Break Content",
        TextKey::BreakContentBody => {
            "Break content can be used to play specific items (using playlists), and to pad blocks to their full configured duration."
        }
        TextKey::BreakContentItem => "Break Content Item",
        TextKey::BreakContentMode => "Break Content Mode",
        TextKey::DeadAirFallback => "Dead Air Fallback",
        TextKey::DeadAirFallbackBody => {
            "When no playout item is found for the current time, one item will be randomly selected from this collection and looped and trimmed to exactly fit until the start of the next playout item. This replaces the Channel is Offline image that would otherwise display."
        }
        TextKey::DeadAirFallbackCollectionType => "Dead Air Fallback Collection Type",
        TextKey::DeadAirFallbackMode => "Dead Air Fallback Mode",
        TextKey::Deco => "Deco",
        TextKey::DecoCannotBeAdded => {
            "Deco cannot be added because it would overlap or cross midnight."
        }
        TextKey::DecoCannotBeMoved => {
            "Deco cannot be moved there because it would overlap or cross midnight."
        }
        TextKey::DecoCreated => "Deco created.",
        TextKey::DecoDeleted => "Deco deleted.",
        TextKey::DecoGroup => "Deco Group",
        TextKey::DecoGroupCreated => "Deco group created.",
        TextKey::DecoGroupDeleted => "Deco group deleted.",
        TextKey::DecoGroupName => "Deco Group Name",
        TextKey::DecoGroups => "Deco Groups",
        TextKey::DecoName => "Deco Name",
        TextKey::DecoNotFound => "Deco not found.",
        TextKey::DecoSaved => "Deco saved.",
        TextKey::Decos => "Decos",
        TextKey::DecoTemplate => "Deco Template",
        TextKey::DecoTemplateCreated => "Deco template created.",
        TextKey::DecoTemplateDeleted => "Deco template deleted.",
        TextKey::DecoTemplateGroup => "Deco Template Group",
        TextKey::DecoTemplateGroupCreated => "Deco template group created.",
        TextKey::DecoTemplateGroupDeleted => "Deco template group deleted.",
        TextKey::DecoTemplateGroupName => "Deco Template Group Name",
        TextKey::DecoTemplateGroups => "Deco Template Groups",
        TextKey::DecoTemplateName => "Deco Template Name",
        TextKey::DecoTemplateNotFound => "Deco template not found.",
        TextKey::DecoTemplateSaved => "Deco template saved.",
        TextKey::AddDecoToTemplate => "Add Deco To Template",
        TextKey::DecoToRemove => "Deco To Remove",
        TextKey::DefaultFiller => "Default Filler",
        TextKey::DefaultFillerBody => {
            "After all blocks have been scheduled, a second pass will be made to fill unscheduled time using random items from this collection."
        }
        TextKey::DefaultFillerCollectionType => "Default Filler Collection Type",
        TextKey::DefaultFillerMode => "Default Filler Mode",
        TextKey::DeleteDeco => "Delete Deco",
        TextKey::DeleteDecoGroup => "Delete Deco Group",
        TextKey::DeleteDecoTemplate => "Delete Deco Template",
        TextKey::DeleteDecoTemplateGroup => "Delete Deco Template Group",
        TextKey::Disable => "Disable",
        TextKey::EditDeco => "Edit Deco",
        TextKey::EditDecoTemplate => "Edit Deco Template",
        TextKey::GraphicsElementsMode => "Graphics Elements Mode",
        TextKey::Inherit => "Inherit",
        TextKey::Merge => "Merge",
        TextKey::NoDecosFound => "No decos found.",
        TextKey::NoDecoTemplatesFound => "No deco templates found.",
        TextKey::Override => "Override",
        TextKey::RemoveBreakContent => "Remove Break Content",
        TextKey::RemoveDecoFromTemplate => "Remove Deco From Template",
        TextKey::Replace => "Replace",
        TextKey::SavedDecos => "Saved Decos",
        TextKey::SavedDecoTemplates => "Saved Deco Templates",
        TextKey::SearchForDecos => "Search for decos",
        TextKey::SearchForDecoTemplates => "Search for deco templates",
        TextKey::TrimToFit => "Trim To Fit",
        TextKey::UseGraphicsElementsDuringFiller => "Use Graphics Elements During Filler",
        TextKey::UseWatermarkDuringFiller => "Use Watermark During Filler",
        TextKey::WatermarkMode => "Watermark Mode",
        TextKey::DecoTemplates => "Deco Templates",
        TextKey::DefaultFfmpegProfile => "Default FFmpeg Profile",
        TextKey::DefaultFfmpegProfileHelp => "The FFmpeg Profile to use when creating new channels",
        TextKey::DefaultMinimumLogLevel => "Default Minimum Log Level",
        TextKey::DefaultMpegTsScript => "Default MPEG-TS Script",
        TextKey::DefaultMpegTsScriptHelp => "The MPEG-TS script to use when streaming",
        TextKey::Decorations => "Decorations",
        TextKey::Debug => "Debug",
        TextKey::Delete => "Delete",
        TextKey::DeleteAll => "Delete All",
        TextKey::DeleteBlock => "Delete Block",
        TextKey::DeleteBlockGroup => "Delete Block Group",
        TextKey::DeleteCustomResolution => "Delete custom resolution",
        TextKey::DeleteCollection => "Delete Collection",
        TextKey::DeleteFillerPreset => "Delete Filler Preset",
        TextKey::DeleteMultiCollection => "Delete Multi Collection",
        TextKey::DeletePlaylist => "Delete Playlist",
        TextKey::DeletePlaylistGroup => "Delete Playlist Group",
        TextKey::DeleteRerunCollection => "Delete Rerun Collection",
        TextKey::DeleteTemplate => "Delete Template",
        TextKey::DeleteTemplateGroup => "Delete Template Group",
        TextKey::DeleteAddonMediaList => "Delete add-on list",
        TextKey::DisabledCollectionsAlreadyPresent => {
            "Disabled collections are already present in this multi collection"
        }
        TextKey::EmptyTrash => "Empty Trash",
        TextKey::ExtractEmbeddedSubtitles => "Extract Embedded (Text) Subtitles",
        TextKey::ErasedPlayoutItemsAndHistory => "Erased playout items and history.",
        TextKey::EraseFailed => "Erase failed",
        TextKey::EraseItemsAndHistory => "Erase Items and History",
        TextKey::ErasePlayoutItemsAndHistoryConfirm => {
            "This will delete all playout items and history for this sequential playout. Continue?"
        }
        TextKey::Episode => "Episode",
        TextKey::FanArt => "fan art",
        TextKey::Ffmpeg => "FFmpeg",
        TextKey::FileNotFound => "File not found",
        TextKey::Finish => "Finish",
        TextKey::Fatal => "Fatal",
        TextKey::FfmpegPath => "FFmpeg Path",
        TextKey::FfmpegPathHelp => "The full path to the ffmpeg executable file",
        TextKey::FfmpegProfiles => "FFmpeg Profiles",
        TextKey::FfprobePath => "FFprobe Path",
        TextKey::FfprobePathHelp => "The full path to the ffprobe executable file",
        TextKey::FillerPreset => "Filler Preset",
        TextKey::FillerPresetActionFailed => "Filler preset action failed.",
        TextKey::FillerPresetCouldNotBeSaved => {
            "Filler preset could not be saved. Check the form values and try again."
        }
        TextKey::FillerPresetDeleted => "Filler preset deleted.",
        TextKey::FillerPresetExpressionHelp => {
            "For mid-roll filler, only add filler when this expression evaluates to true for a given mid-roll point."
        }
        TextKey::FillerPresetSaved => "Filler preset saved.",
        TextKey::FillerPresets => "Filler Presets",
        TextKey::Flags => "Flags",
        TextKey::GlobalFallbackFiller => "Global Fallback Filler",
        TextKey::GlobalWatermark => "Global Watermark",
        TextKey::GoHome => "Go home",
        TextKey::HdHomeRun => "HDHomeRun",
        TextKey::HealthChecks => "Health Checks",
        TextKey::Height => "Height",
        TextKey::HlsDirectOutputFormat => "HLS Direct Output Format",
        TextKey::HlsSegmenterIdleTimeout => "HLS Segmenter Idle Timeout",
        TextKey::HlsSegmenterIdleTimeoutHelp => {
            "The number of seconds to continue transcoding a channel while no requests have been received from any client"
        }
        TextKey::HlsSegmenterInitialSegmentCount => "HLS Segmenter Initial Segment Count",
        TextKey::HlsSegmenterInitialSegmentCountHelp => {
            "Delays stream start until the specified number of (4-second) segments have been transcoded; a larger number will mean slower initial playback, but potentially less buffering"
        }
        TextKey::HomeRunning => "IPTV server is running.",
        TextKey::ReleaseNotes => "Release notes",
        TextKey::FullChangelog => "Full changelog",
        TextKey::HttpMinimumLogLevel => "Request Logging Minimum Log Level",
        TextKey::Id => "Id",
        TextKey::Images => "Images",
        TextKey::Item => "Item",
        TextKey::ItemActions => "Item actions",
        TextKey::ItemType => "Item Type",
        TextKey::Items => "Items",
        TextKey::Language => "Language",
        TextKey::Languages => "Languages",
        TextKey::MusicLabels => "Music Labels",
        TextKey::LeaveEmptyToAutoDetect => "Leave empty to auto-detect",
        TextKey::Libraries => "Libraries",
        TextKey::LibraryRefreshInterval => "Library Refresh Interval",
        TextKey::Light => "Light",
        TextKey::Lists => "Lists",
        TextKey::Loading => "Loading...",
        TextKey::LoadingMembers => "Loading members...",
        TextKey::Local => "Local",
        TextKey::Logging => "Logging",
        TextKey::M3u => "M3U",
        TextKey::Media => "Media",
        TextKey::MediaInfo => "Media Info",
        TextKey::MediaSources => "Media Sources",
        TextKey::Movie => "Movie",
        TextKey::MovieNotFound => "Movie not found",
        TextKey::Movies => "Movies",
        TextKey::MusicVideo => "Music Video",
        TextKey::MultiCollections => "Multi Collections",
        TextKey::ManualCollections => "Manual Collections",
        TextKey::MatchFailed => "Match failed",
        TextKey::MatchStatus => "Match Status",
        TextKey::MatchAddonMediaListItems => "Match add-on list items",
        TextKey::Members => "Members",
        TextKey::MultiCollection => "Multi Collection",
        TextKey::MultiCollectionDeleted => "Multi collection deleted.",
        TextKey::MultiCollectionItems => "Multi Collection Items",
        TextKey::MultiCollectionNameRequired => "Multi-collection name is required!",
        TextKey::MultiCollectionNameTooLong => {
            "Multi-collection name cannot be longer than 50 characters."
        }
        TextKey::MultiCollectionSaved => "Multi collection saved.",
        TextKey::Music => "Music",
        TextKey::Name => "Name",
        TextKey::NameRequired => "Name is required",
        TextKey::Networks => "Networks",
        TextKey::NewCollection => "(New Collection)",
        TextKey::NewCollectionTitle => "New Collection",
        TextKey::NewFillerPreset => "New Filler Preset",
        TextKey::NewMultiCollection => "New Multi Collection",
        TextKey::NewTemplateGroup => "New Template Group",
        TextKey::NewTemplateName => "New Template Name",
        TextKey::Next24Hours => "Next 24h",
        TextKey::NextPage => "Next page",
        TextKey::NoFillerPresets => "No filler presets yet.",
        TextKey::NoBlockItems => "No block items.",
        TextKey::NoItems => "No items.",
        TextKey::NoItemsToDisplay => "No items to display.",
        TextKey::NoAddonMediaListsYet => "No add-on lists yet.",
        TextKey::No => "No",
        TextKey::NoCollections => "No collections yet.",
        TextKey::NoMembers => "No members yet.",
        TextKey::NoMultiCollections => "No multi collections yet.",
        TextKey::NoTemplatesFound => "No templates found.",
        TextKey::NotFound => "Page not found.",
        TextKey::OtherVideo => "Other Video",
        TextKey::OtherVideoNotFound => "Other video not found",
        TextKey::OtherVideos => "Other Videos",
        TextKey::OpenBrowser => "Open browser",
        TextKey::Playout => "Playout",
        TextKey::PlayoutItemsAndHistory => "Playout Items and History",
        TextKey::PlayoutNotFound => "Playout not found.",
        TextKey::Playouts => "Playouts",
        TextKey::AddPlayout => "Add Playout",
        TextKey::ClassicScheduleMenu => "Classic Schedule",
        TextKey::BlockScheduleMenu => "Block Schedule",
        TextKey::SequentialScheduleMenu => "Sequential Schedule",
        TextKey::ScriptedScheduleMenu => "Scripted Schedule",
        TextKey::JsonDizqueTvScheduleMenu => "JSON (dizqueTV) Schedule",
        TextKey::ResetAllPlayouts => "Reset All Playouts",
        TextKey::PlayoutBuildQueued => "Playout rebuild queued",
        TextKey::PlayoutBuildsQueued => "playout rebuilds queued",
        TextKey::SearchForPlayouts => "Search for playouts",
        TextKey::PlayoutDetail => "Playout Detail",
        TextKey::ShowFiller => "Show Filler",
        TextKey::EditPlayout => "Edit Playout",
        TextKey::RefreshPlayout => "Refresh Playout",
        TextKey::ResetPlayout => "Reset Playout",
        TextKey::ScheduleReset => "Schedule Reset",
        TextKey::SchedulePlayoutReset => "Schedule Playout Reset",
        TextKey::EditExternalJsonFile => "Edit External Json File",
        TextKey::DeletePlayout => "Delete Playout",
        TextKey::Troubleshoot => "Troubleshoot",
        TextKey::DailyResetTime => "Daily Reset Time",
        TextKey::DoNotAutomaticallyReset => "Do not automatically reset",
        TextKey::SaveChanges => "Save Changes",
        TextKey::ExternalJsonFileLabel => "External Json File",
        TextKey::EditExternalJsonFilePrompt => "Edit the playout's external json file",
        TextKey::UnscheduledPlayoutItem => "UNSCHEDULED",
        TextKey::DisabledChannelsAlreadyHavePlayout => "Disabled channels already have a playout",
        TextKey::ProgramSchedule => "Program Schedule",
        TextKey::SequentialScheduleFileHelp => {
            "The full path to the sequential schedule (YAML) file"
        }
        TextKey::BlockTemplatesAddedLater => "Block templates are added later",
        TextKey::UnsupportedPlayoutKind => "Unsupported playout kind.",
        TextKey::ClassicPlayout => "Classic Playout",
        TextKey::ClassicPlayoutState => "Classic Playout State",
        TextKey::ScanRefreshState => "Scan Refresh State",
        TextKey::SubtitleCleanup => "Subtitle Cleanup",
        TextKey::SubtitleCleanupMessage => "Subtitle cleanup pending: {count} ownerless subtitle or empty-path sidecar row(s). Automatic cleanup runs hourly. Cached subtitle files are preserved.",
        TextKey::SubtitleCleanupBrief => "{count} subtitle row(s) awaiting cleanup",
        TextKey::ArtworkOrphans => "Artwork Orphans",
        TextKey::ArtworkOrphansMessage => "Orphan cleanup pending: {actors} actor row(s) and {artwork} artwork row(s). Automatic cleanup runs hourly. To request cleanup now, use POST /api/maintenance/clean_artwork. Attached records and referenced cached images are preserved.",
        TextKey::ArtworkOrphansBrief => "{actors} orphan actor row(s) and {artwork} orphan artwork row(s)",
        TextKey::ArtworkCardinality => "Artwork Cardinality",
        TextKey::Av1OutputCompatibility => "AV1 output requires native HLS. Change the channel engine/mode or select H.264/HEVC in its FFmpeg profile.",
        TextKey::HlsLiveDelivery => "HLS Live Delivery",
        TextKey::BlockPlayout => "Block Playout",
        TextKey::ScriptedPlayout => "Scripted Schedule",
        TextKey::AlternateSchedules => "Alternate Schedules",
        TextKey::EditAlternateSchedules => "Edit Alternate Schedules",
        TextKey::PlayoutTemplates => "Playout Templates",
        TextKey::EditTemplates => "Edit Templates",
        TextKey::DefaultDeco => "Default Deco",
        TextKey::EnableDefaultDeco => "Enable Default Deco",
        TextKey::SaveDefaultDeco => "Save Default Deco",
        TextKey::SaveScriptedSchedule => "Save Scripted Schedule",
        TextKey::EraseItems => "Erase Items",
        TextKey::ErasedPlayoutItems => "Erased playout items.",
        TextKey::SavedDefaultDeco => "Saved default deco.",
        TextKey::SavedScriptedSchedule => "Saved scripted schedule.",
        TextKey::Maintenance => "Maintenance",
        TextKey::SaveAlternateSchedules => "Save Alternate Schedules",
        TextKey::AddAlternateSchedule => "Add Alternate Schedule",
        TextKey::SaveTemplates => "Save Templates",
        TextKey::PreviewCalendar => "Preview Calendar",
        TextKey::EditContent => "Edit Content",
        TextKey::PlayoutNestedCatchAllHelp => {
            "In priority order from top to bottom. The bottom entry will always match all days and all months as a catch-all."
        }
        TextKey::PlayoutNestedPriorityHelp => "In priority order from top to bottom.",
        TextKey::DaysOfWeek => "Days of the Week",
        TextKey::DaysOfMonth => "Days of the Month",
        TextKey::Months => "Months",
        TextKey::LimitToDateRange => "Limit to Date Range",
        TextKey::StartDay => "Start Day",
        TextKey::EndDay => "End Day",
        TextKey::Enable => "Enable",
        TextKey::Up => "Up",
        TextKey::Down => "Down",
        TextKey::Weekdays => "Weekdays",
        TextKey::Weekends => "Weekends",
        TextKey::Sunday => "Sunday",
        TextKey::Monday => "Monday",
        TextKey::Tuesday => "Tuesday",
        TextKey::Wednesday => "Wednesday",
        TextKey::Thursday => "Thursday",
        TextKey::Friday => "Friday",
        TextKey::Saturday => "Saturday",
        TextKey::January => "January",
        TextKey::February => "February",
        TextKey::March => "March",
        TextKey::April => "April",
        TextKey::May => "May",
        TextKey::June => "June",
        TextKey::July => "July",
        TextKey::August => "August",
        TextKey::September => "September",
        TextKey::October => "October",
        TextKey::November => "November",
        TextKey::December => "December",
        TextKey::JanuaryShort => "Jan",
        TextKey::FebruaryShort => "Feb",
        TextKey::MarchShort => "Mar",
        TextKey::AprilShort => "Apr",
        TextKey::MayShort => "May",
        TextKey::JuneShort => "Jun",
        TextKey::JulyShort => "Jul",
        TextKey::AugustShort => "Aug",
        TextKey::SeptemberShort => "Sep",
        TextKey::OctoberShort => "Oct",
        TextKey::NovemberShort => "Nov",
        TextKey::DecemberShort => "Dec",
        TextKey::PlayAll => "Play All",
        TextKey::Playlist => "Playlist",
        TextKey::PlaylistAdded => "Playlist added.",
        TextKey::PlaylistDeleted => "Playlist deleted.",
        TextKey::PlaylistGroup => "Playlist Group",
        TextKey::PlaylistGroupAdded => "Playlist group added.",
        TextKey::PlaylistGroupDeleted => "Playlist group deleted.",
        TextKey::PlaylistGroupName => "Playlist Group Name",
        TextKey::PlaylistGroups => "Playlist Groups",
        TextKey::PlaylistItem => "Playlist Item",
        TextKey::PlaylistItems => "Playlist Items",
        TextKey::PlaylistName => "Playlist Name",
        TextKey::PlaylistNotFound => "Playlist not found.",
        TextKey::PlaylistPlayoutPreview => "Playlist Playout Preview",
        TextKey::PlaylistPreviewEmpty => "Playlist playout preview has no items",
        TextKey::Playlists => "Playlists",
        TextKey::ProbeForInterlacedFrames => "Probe For Interlaced Frames",
        TextKey::PreferredAudioLanguage => "Preferred Audio Language",
        TextKey::DefaultSubtitleLanguage => "Default Subtitle Language",
        TextKey::FallbackSubtitleLanguage => "Fallback Subtitle Language",
        TextKey::OriginalLanguageOption => "Original (und)",
        TextKey::UndefinedLanguageOption => "Undefined (und)",
        TextKey::Previous24Hours => "Prev 24h",
        TextKey::PreviousPage => "Previous page",
        TextKey::RerunCollections => "Rerun Collections",
        TextKey::RerunCollection => "Rerun Collection",
        TextKey::RerunCollectionActionFailed => "Rerun collection action failed.",
        TextKey::RerunCollectionDeleted => "Rerun collection deleted.",
        TextKey::RerunCollectionNameRequired => "Rerun collection name is required!",
        TextKey::RerunCollectionSaved => "Rerun collection saved.",
        TextKey::RerunPlaybackOrder => "Rerun Playback Order",
        TextKey::RemoteStreams => "Remote Streams",
        TextKey::RemoteStream => "Remote Stream",
        TextKey::PublishedOptions => "Published options",
        TextKey::PlaybackHealth => "Playback health",
        TextKey::Configure => "Configure",
        TextKey::Watch => "Watch",
        TextKey::RemoteStreamPlaylists => "Remote Stream playlists",
        TextKey::PlaylistUrl => "Playlist URL",
        TextKey::Extractor => "Extractor",
        TextKey::DestinationFolder => "Destination folder",
        TextKey::SyncInterval => "Sync interval",
        TextKey::MaxItems => "Maximum items",
        TextKey::RemovedEntries => "Removed entries",
        TextKey::CreateOrderedPlaylist => "Create an ordered playlist",
        TextKey::SyncNow => "Sync now",
        TextKey::LastSync => "Last sync",
        TextKey::NeverSynced => "Never synced",
        TextKey::ManagedManifestReadOnly => "This hand-authored manifest is read-only here.",
        TextKey::Truncated => "Truncated",
        TextKey::MissingDuration => "Missing duration",
        TextKey::Orphaned => "Orphaned",
        TextKey::Materialized => "Materialized",
        TextKey::State => "State",
        TextKey::Upstream => "Upstream",
        TextKey::NewestFirst => "Newest first",
        TextKey::Keep => "Keep",
        TextKey::ResetChannels => "Reset Channels",
        TextKey::RowsPerPage => "Rows per page",
        TextKey::Save => "Save",
        TextKey::SaveMultiCollection => "Save Multi Collection",
        TextKey::SavePlaylist => "Save Playlist",
        TextKey::SaveTroubleshootingReportsToDisk => "Save Troubleshooting Reports To Disk",
        TextKey::SaveAsSmartCollection => "Save As Smart Collection",
        TextKey::SearchQueryRequired => "Search query is required",
        TextKey::SaveSettings => "Save Settings",
        TextKey::Scanner => "Scanner",
        TextKey::ScanningMinimumLogLevel => "Scanning Minimum Log Level",
        TextKey::Scheduling => "Scheduling",
        TextKey::SchedulingMinimumLogLevel => "Scheduling Minimum Log Level",
        TextKey::StartWithWindows => "Start ErsatzRS when I sign in to Windows",
        TextKey::StartWithWindowsHelp => {
            "Start the tray launcher automatically for the current Windows user."
        }
        TextKey::Schedules => "Schedules",
        TextKey::AddSchedule => "Add Schedule",
        TextKey::AddScheduleItem => "Add Schedule Item",
        TextKey::AddSmartCollection => "Add Smart Collection",
        TextKey::Album => "Album",
        TextKey::Chronological => "Chronological",
        TextKey::CollectionSize => "Collection Size",
        TextKey::CollectionType => "Collection Type",
        TextKey::Count => "Count",
        TextKey::CopySchedule => "Copy Schedule",
        TextKey::CopyScheduleItem => "Copy Schedule Item",
        TextKey::CustomTitle => "Custom Title",
        TextKey::DeleteSchedule => "Delete Schedule",
        TextKey::DeleteScheduleItem => "Delete Schedule Item",
        TextKey::DeleteSmartCollection => "Delete Smart Collection",
        TextKey::DetailRowsPerPage => "Detail rows per page",
        TextKey::Director => "Director",
        TextKey::Directors => "Directors",
        TextKey::DiscardToFillAttempts => "Discard To Fill Attempts",
        TextKey::Duration => "Duration",
        TextKey::Dynamic => "Dynamic",
        TextKey::EditFillerPreset => "Edit Filler Preset",
        TextKey::EditProperties => "Edit Properties",
        TextKey::EditScheduleItems => "Edit Schedule Items",
        TextKey::EditSmartCollection => "Edit Smart Collection",
        TextKey::EnterNameForNewSchedule => "Enter a name for the new Schedule",
        TextKey::Fixed => "Fixed",
        TextKey::FixedStartTimeBehavior => "Fixed Start Time Behavior",
        TextKey::Flood => "Flood",
        TextKey::Flexible => "Flexible",
        TextKey::FlexibleStartSoonAsPossible => "Flexible - Start As Soon As Possible",
        TextKey::FlexibleStartSoonAsPossibleAfterStartTime => {
            "Flexible - Start As Soon As Possible After Start Time"
        }
        TextKey::FillWithGroupMode => "Fill With Group Mode (Show or Artist)",
        TextKey::GraphicsElements => "Graphics Elements",
        TextKey::AddGraphicsElement => "Add Graphics Element",
        TextKey::DrawtextRendererOption => "Drawtext — dynamic/file-backed",
        TextKey::DrawtextSettings => "Drawtext Settings",
        TextKey::DrawvgRendererOption => "Drawvg — static/vector",
        TextKey::EditGraphicsElement => "Edit Graphics Element",
        TextKey::FontFile => "Font File",
        TextKey::FontFileOptionalHelp => {
            "Optional. Leave empty to use the packaged Roboto font fallback."
        }
        TextKey::GraphicsElement => "Graphics Element",
        TextKey::GraphicsElementConfigInvalid => {
            "This text graphics element has invalid configuration and was not changed."
        }
        TextKey::GraphicsElementKindImage => "Image",
        TextKey::GraphicsElementKindMotion => "Motion",
        TextKey::GraphicsElementKindScript => "Script",
        TextKey::GraphicsElementKindSubtitle => "Subtitle",
        TextKey::GraphicsElementKindText => "Text",
        TextKey::GraphicsElementNameTooLong => "Name must be 50 characters or fewer.",
        TextKey::GraphicsElementNotEditable => "Not editable here",
        TextKey::GraphicsElementNotFound => "Graphics element not found.",
        TextKey::GraphicsElementNotText => "Only Text graphics elements can be edited here.",
        TextKey::GraphicsElementSaved => "Graphics element saved.",
        TextKey::GraphicsElementSaveFailed => "Graphics element could not be saved.",
        TextKey::GraphicsElementType => "Type",
        TextKey::GraphicsExistingDrawvgHelp => {
            "Existing elements and new elements use Drawvg by default."
        }
        TextKey::GraphicsStreamRestartHelp => {
            "Changes affect newly started streams and do not restart active streams."
        }
        TextKey::NoGraphicsElements => "No graphics elements found.",
        TextKey::RefreshPeriodBounds => {
            "Refresh period must be between 250 and 60000 milliseconds."
        }
        TextKey::RefreshPeriodMilliseconds => "Refresh Period (milliseconds)",
        TextKey::ReloadFrameInterval => "Reload Frame Interval",
        TextKey::ReloadFrameIntervalBounds => "Reload frame interval must be at least 1.",
        TextKey::Renderer => "Renderer",
        TextKey::SaveGraphicsElement => "Save Graphics Element",
        TextKey::TextTemplate => "Text Template",
        TextKey::TextTemplateRequired => "Text template is required.",
        TextKey::TextTemplateTooLong => "Text template must be valid UTF-8 text up to 64 KiB.",
        TextKey::GraphicsIds => "Graphics IDs",
        TextKey::Genres => "Genres",
        TextKey::GroupName => "Group Name",
        TextKey::GroupMultiPartEpisodesHelp => "Group multi-part episodes when shuffling",
        TextKey::GuideMode => "Guide Mode",
        TextKey::KeepMultiPartEpisodesTogether => "Keep Multi-Part Episodes Together",
        TextKey::Marathon => "Marathon",
        TextKey::MarathonBatchSize => "Marathon Batch Size",
        TextKey::MarathonBatchSizeHelp => {
            "How many items to play from each group before advancing; empty or zero will play all items"
        }
        TextKey::MarathonGroupBy => "Marathon Group By",
        TextKey::MarathonShuffleGroups => "Marathon Shuffle Groups",
        TextKey::MarathonShuffleItems => "Marathon Shuffle Items",
        TextKey::MidRollFiller => "Mid-Roll Filler",
        TextKey::MidRoll => "Mid-Roll",
        TextKey::MoveDown => "Move Down",
        TextKey::MoveUp => "Move Up",
        TextKey::ReorderPlaylistItem => "Drag to reorder playlist item",
        TextKey::MultiEpisodeGroupSize => "Multi-Episode Group Size",
        TextKey::MultiEpisodeShuffle => "Multi-Episode Shuffle",
        TextKey::Multiple => "Multiple",
        TextKey::MultipleCount => "Multiple Count",
        TextKey::MultipleCountHelp => {
            "Can be an expression with 'count' (collection size) and 'random' (0 to count-1)"
        }
        TextKey::MultipleMode => "Multiple Mode",
        TextKey::NewScheduleName => "New Schedule Name",
        TextKey::NewSmartCollection => "New Smart Collection",
        TextKey::NoScheduleItems => "No schedule items.",
        TextKey::NoSchedules => "No schedules.",
        TextKey::Normal => "Normal",
        TextKey::Offline => "Offline",
        TextKey::One => "One",
        TextKey::OrderedGroups => "Ordered Groups",
        TextKey::Playback => "Playback",
        TextKey::PlaybackOrder => "Playback Order",
        TextKey::PlaybackOrderUpdateFailed => "Playback order update failed",
        TextKey::PlaybackOrderUpdated => "Playback order updated.",
        TextKey::PlaylistItemSize => "Playlist Item Size",
        TextKey::PlayoutDurationHours => "Playout Duration Hours",
        TextKey::PlayoutDurationMinutes => "Playout Duration Minutes",
        TextKey::PostRollFiller => "Post-Roll Filler",
        TextKey::PostRoll => "Post-Roll",
        TextKey::PreRollFiller => "Pre-Roll Filler",
        TextKey::PreRoll => "Pre-Roll",
        TextKey::Random => "Random",
        TextKey::RandomCount => "Random Count",
        TextKey::RandomRotation => "Random Rotation",
        TextKey::RemainingDurationHelp => {
            "duration of the content after this mid-roll point, in seconds"
        }
        TextKey::RandomStartPoint => "Random Start Point",
        TextKey::RemoveFailed => "Remove failed",
        TextKey::RemovedFromCollection => "Removed from collection.",
        TextKey::RemoveFromCollection => "Remove From Collection",
        TextKey::Rename => "Rename",
        TextKey::RenameFailed => "Rename failed",
        TextKey::ReorderFailed => "Reorder failed",
        TextKey::RerunFirstRun => "Rerun First Run",
        TextKey::RerunRerun => "Rerun Rerun",
        TextKey::ReadOnlyTwentyFourHourPlayoutView => {
            "Read-only 24-hour view of scheduled playout items."
        }
        TextKey::SaveSchedule => "Save Schedule",
        TextKey::SaveScheduleItems => "Save Schedule Items",
        TextKey::SaveSmartCollection => "Save Smart Collection",
        TextKey::Schedule => "Schedule",
        TextKey::ScheduleActionFailed => "Schedule action failed.",
        TextKey::ScheduleAsGroup => "Schedule As Group",
        TextKey::ScheduleKind => "Schedule Kind",
        TextKey::ScheduleKindBlock => "Block",
        TextKey::ScheduleKindClassic => "Classic",
        TextKey::ScheduleKindExternalJson => "JSON (dizqueTV)",
        TextKey::ScheduleKindScripted => "Scripted",
        TextKey::ScheduleKindSequential => "Sequential",
        TextKey::SchedulingContext => "Scheduling Context",
        TextKey::ScheduleItem => "Schedule Item",
        TextKey::ScheduleItemActionFailed => "Schedule item action failed.",
        TextKey::ScheduleItemFiller => "Schedule Item Filler",
        TextKey::ScheduleItemFillerHelp => {
            "Pre-, mid-, and post-roll filler applies to every media item selected by this schedule item. To play content once after a Multiple block, add it as the next schedule item."
        }
        TextKey::ScheduleItemOverrides => "Schedule Item Overrides",
        TextKey::ScheduleItems => "Schedule Items",
        TextKey::ScheduleNotFound => "Schedule not found.",
        TextKey::ScheduleSaveFailed => "Schedule save failed.",
        TextKey::SmartShuffle => "Smart Shuffle",
        TextKey::SmartShufflePreset => "Spacing preset",
        TextKey::Balanced => "Balanced",
        TextKey::SmartShuffleScope => "Share spacing across",
        TextKey::SmartShuffleScopePlayout => "Playout",
        TextKey::SmartShuffleScopeScheduleItem => "Schedule Item",
        TextKey::SmartShuffleValidation => {
            "Choose a spacing threshold and at least one similarity rule."
        }
        TextKey::SmartShuffleScopeContentSource => "Content Source",
        TextKey::SmartShuffleAfterItems => "Videos between similar items",
        TextKey::SmartShuffleAfterDurationMinutes => "Minutes between similar items",
        TextKey::SmartShuffleIncludeFiller => "Include filler",
        TextKey::SmartShuffleInheritedBlock => {
            "This item has no Smart Shuffle override and keeps any legacy playout policy."
        }
        TextKey::SmartShuffleInactive => {
            "Smart Shuffle is inactive because this playback order is not randomized."
        }
        TextKey::SmartShuffleOverrideBlock => "Override for this Block item",
        TextKey::SimilarityRules => "Similarity Rules",
        TextKey::AlbumArtist => "Album Artist",
        TextKey::Apply => "Apply",
        TextKey::ContentParent => "Parent or Variant",
        TextKey::SmartShuffleFilterRules => "Filter Rules",
        TextKey::SearchForSchedules => "Search for schedules",
        TextKey::SearchForManualCollections => "Search for manual collections",
        TextKey::SearchForRerunCollections => "Search for rerun collections",
        TextKey::SearchForSmartCollections => "Search for smart collections",
        TextKey::SearchForTemplates => "Search for templates",
        TextKey::SearchForBlocks => "Search for blocks",
        TextKey::SearchForMultiCollections => "Search for multi collections",
        TextKey::SearchQuery => "Search Query",
        TextKey::BlockSearchQueryPlaceholder => "e.g. genre:action",
        TextKey::SearchTitle => "Search Title",
        TextKey::SearchTitleDisplay => "Search Title (display)",
        TextKey::SearchSmartCollection => "Search Smart Collection",
        TextKey::Season => "Season",
        TextKey::SeasonEpisode => "Season, Episode",
        TextKey::Unnumbered => "Unnumbered",
        TextKey::SeasonNotFound => "Season not found",
        TextKey::Select => "Select",
        TextKey::SelectCollection => "Select collection",
        TextKey::SelectGroupForNewTemplate => "Select a group for the new template",
        TextKey::SelectScheduleItemPrompt => "Select a schedule item to edit, or add a new item.",
        TextKey::SelectSchedule => "Select a schedule",
        TextKey::SelectSmartCollection => "Select smart collection",
        TextKey::Shuffle => "Shuffle",
        TextKey::ShuffleInOrder => "Shuffle In Order",
        TextKey::ShuffleScheduleItems => "Shuffle Schedule Items",
        TextKey::ShuffleScheduleItemsHelp => "Note: this disables fixed start times and flood mode",
        TextKey::ShuffledGroups => "Shuffled Groups",
        TextKey::Show => "Show",
        TextKey::ShowNotFound => "Show not found",
        TextKey::Shows => "Shows",
        TextKey::Start => "Start",
        TextKey::StartTime => "Start Time",
        TextKey::StartTimeOnOrAfter => "Start Time On Or After",
        TextKey::StartType => "Start Type",
        TextKey::Strict => "Strict",
        TextKey::StrictAlwaysWaitExactStartTime => "Strict - Always Wait For Exact Start Time",
        TextKey::StrictWaitExactStartTime => "Strict - Wait For Exact Start Time",
        TextKey::TailFiller => "Tail Filler",
        TextKey::TailMode => "Tail Mode",
        TextKey::Tail => "Tail",
        TextKey::TelevisionSeason => "Television Season",
        TextKey::TelevisionShow => "Television Show",
        TextKey::TreatCollectionsAsShows => "Treat Collections As Shows",
        TextKey::TreatCollectionsAsShowsHelp => "This is useful for multi-part crossover episodes",
        TextKey::Type => "Type",
        TextKey::TypeToSearch => "Type to search...",
        TextKey::Search => "Search",
        TextKey::Filters => "Filters",
        TextKey::ActiveFilters => "active filters",
        TextKey::ClearFilters => "Clear filters",
        TextKey::CreateSmartCollection => "Create smart collection",
        TextKey::FromYear => "From year",
        TextKey::ToYear => "To year",
        TextKey::CollectionComposer => "Smart collection composer",
        TextKey::CollectionDetails => "Collection details",
        TextKey::Rules => "Rules",
        TextKey::Overview => "Overview",
        TextKey::MatchAll => "Match all",
        TextKey::MatchAllHelp => "Every rule in this group must match.",
        TextKey::MatchAny => "Match any",
        TextKey::MatchAnyHelp => "At least one rule in this group must match.",
        TextKey::ExcludeAny => "Exclude any",
        TextKey::ExcludeAnyHelp => "Items matching any rule in this group are excluded.",
        TextKey::AddRule => "Add rule",
        TextKey::EditRule => "Edit rule",
        TextKey::RemoveRule => "Remove rule",
        TextKey::RuleField => "Field",
        TextKey::Genre => "Genre",
        TextKey::Studio => "Studio",
        TextKey::Network => "Network",
        TextKey::Actor => "Actor",
        TextKey::Writer => "Writer",
        TextKey::SubtitleLanguage => "Subtitle language",
        TextKey::ShowTitle => "Show title",
        TextKey::ContentKindLabel => "Content kind",
        TextKey::SubKindLabel => "Subtype",
        TextKey::HasCommentary => "Has commentary",
        TextKey::AllFields => "All fields",
        TextKey::MatchType => "Match type",
        TextKey::MatchValue => "Value",
        TextKey::TextContains => "Text contains",
        TextKey::ExactPhrase => "Exact phrase",
        TextKey::Wildcard => "Wildcard",
        TextKey::Fuzzy => "Fuzzy",
        TextKey::Range => "Range",
        TextKey::RangeStart => "Range start",
        TextKey::RangeEnd => "Range end",
        TextKey::NestedGroup => "Nested group",
        TextKey::NestedGroupHelp => {
            "Edit this group in focus mode to keep one boolean level visible at a time."
        }
        TextKey::AddNestedGroup => "Add nested group",
        TextKey::EditNestedGroup => "Edit nested group",
        TextKey::BackToRules => "Back to rules",
        TextKey::BackToSearch => "Back to search",
        TextKey::CanonicalQuery => "Canonical query",
        TextKey::CanonicalQueryHelp => {
            "This is the query used by search, scheduling, and the saved collection."
        }
        TextKey::LivePreview => "Live preview",
        TextKey::MatchingItems => "matching items",
        TextKey::QueryValid => "Query valid",
        TextKey::QueryInvalid => "Query needs attention",
        TextKey::SearchRuleValueRequired => "Every search rule needs a value.",
        TextKey::SearchGroupRequired => "Add at least one rule to this group.",
        TextKey::SearchWildcardRequired => "A wildcard rule must contain * or ?.",
        TextKey::SavedSearches => "Saved searches",
        TextKey::SavedSearchName => "Saved search name",
        TextKey::SavedSearchHelp => {
            "Save this complex query so you can recall and modify it later."
        }
        TextKey::ChooseSavedSearch => "Choose a saved search",
        TextKey::LoadSavedSearch => "Load saved search",
        TextKey::SaveSavedSearch => "Save search",
        TextKey::UpdateSavedSearch => "Update saved search",
        TextKey::NewSavedSearch => "New saved search",
        TextKey::DeleteSavedSearch => "Delete saved search",
        TextKey::SavedSearchSaved => "Saved search saved.",
        TextKey::SavedSearchDeleted => "Saved search deleted.",
        TextKey::SavedSearchLoaded => "Saved search loaded.",
        TextKey::SavedSearchNameRequired => "Saved search name is required.",
        TextKey::SavedSearchNameTooLong => "Saved search name must be 80 characters or less.",
        TextKey::SavedSearchNameUnique => "Saved search name must be unique.",
        TextKey::NoSavedSearches => "No saved searches yet.",
        TextKey::RefreshPreview => "Refresh preview",
        TextKey::NoRules => "No rules in this group.",
        TextKey::SearchFiltersHelp => {
            "Common filters only. Use the smart collection composer for nested logic."
        }
        TextKey::SearchMedia => "Search media",
        TextKey::SearchMoviesOrShows => "Search movies or shows...",
        TextKey::SearchAddonMediaList => "Search add-on list",
        TextKey::SearchingMinimumLogLevel => "Searching Minimum Log Level",
        TextKey::Settings => "Settings",
        TextKey::SettingsSaveFailed => "Settings Save Failed",
        TextKey::ShowMediaInfo => "Show Media Info",
        TextKey::ScriptedScheduleTimeout => "Scripted Schedule Timeout",
        TextKey::SkipMissingItems => "Skip Missing Items",
        TextKey::SkipMissingItemsHelp => {
            "Controls whether file-not-found or unavailable items should be included in playouts"
        }
        TextKey::SmartCollections => "Smart Collections",
        TextKey::SmartCollection => "Smart Collection",
        TextKey::SmartCollectionDeleted => "Smart collection deleted.",
        TextKey::SmartCollectionQueryPlaceholder => "e.g. genre:action OR genre:comedy",
        TextKey::SmartCollectionSaved => "Smart collection saved.",
        TextKey::SaveAddonMediaList => "Save add-on list",
        TextKey::Songs => "Songs",
        TextKey::SplitTimeEvenly => "Split Time Evenly",
        TextKey::StreamingMinimumLogLevel => "Streaming Minimum Log Level",
        TextKey::Studios => "Studios",
        TextKey::Support => "Support",
        TextKey::Template => "Template",
        TextKey::TemplateCopied => "Template copied.",
        TextKey::TemplateCreated => "Template created.",
        TextKey::TemplateDeleted => "Template deleted.",
        TextKey::TemplateGroup => "Template Group",
        TextKey::TemplateGroupCreated => "Template group created.",
        TextKey::TemplateGroupDeleted => "Template group deleted.",
        TextKey::TemplateGroupName => "Template Group Name",
        TextKey::TemplateGroups => "Template Groups",
        TextKey::TemplateName => "Template Name",
        TextKey::TemplateNotFound => "Template not found.",
        TextKey::Templates => "Templates",
        TextKey::Theme => "Theme",
        TextKey::ThisActionCannotBeUndone => "This action cannot be undone.",
        TextKey::ThisCollectionHasNoItemsYet => "This collection has no items yet.",
        TextKey::Thumbnail => "thumbnail",
        TextKey::Title => "Title",
        TextKey::ToggleNavigation => "Toggle navigation",
        TextKey::TotalDurationHelp => "total duration of the content, in seconds",
        TextKey::TotalPointsHelp => "total number of potential mid-roll points",
        TextKey::TotalProgressHelp => "normalized position from 0 to 1",
        TextKey::AddonMediaList => "Add-on list",
        TextKey::AddonMediaListActionFailed => "add-on list action failed.",
        TextKey::AddonMediaListAdded => "add-on list added.",
        TextKey::AddonMediaListDeleted => "add-on list deleted.",
        TextKey::AddonMediaListMatched => "add-on list matched.",
        TextKey::AddonMediaListNotFound => "add-on list not found.",
        TextKey::AddonMediaListSaveFailed => "add-on list save failed.",
        TextKey::AddonMediaListSaved => "add-on list saved.",
        TextKey::AddonMediaLists => "Add-on lists",
        TextKey::AddonToolMaintenance => "Tool maintenance",
        TextKey::AddonToolMaintenanceNone => "No maintenance attempt yet.",
        TextKey::AddonToolUpdatePermission => "Allow native updates of configured executables",
        TextKey::AddonToolMaintenanceQueued => "Queued",
        TextKey::AddonToolMaintenanceRunning => "Running",
        TextKey::AddonToolMaintenanceWaiting => "Waiting to retry",
        TextKey::AddonToolUpdated => "Updated",
        TextKey::AddonToolAlreadyCurrent => "Already current",
        TextKey::AddonToolManualUpdate => "Manual update required",
        TextKey::AddonToolManualHelp => "Update the configured tool manually using its original installation method, then check the add-on.",
        TextKey::AddonToolFailedHelp => "The tool update failed. Check tool availability and network access; automatic maintenance will retry later.",
        TextKey::AddonToolUpdateFailedStale => "The program could not be updated and a newer release is available. Automatic maintenance will retry. To update it now, use its original installation method, then check the add-on.",
        TextKey::AddonToolUpdateVersions => "{program}: installed {installed}, latest {latest}.",
        TextKey::AddonToolVersionHelp => "The tool version could not be read. Check the configured executable and its permissions.",
        TextKey::AddonToolUnavailableHelp => "The configured tool is unavailable. Check the executable setting and installation.",
        TextKey::AddonToolBusyHelp => "Waiting for the tool to become idle; active playback is not interrupted.",
        TextKey::AddonToolCleanupHelp => "Stop the server, scanner, and all related provider and tool processes. From the server installation directory, run the offline recovery command for this tool with explicit confirmation, replacing <absolute-executable-path> with its configured absolute path. On Windows use ersatzrs-web.exe. If using a custom profile, also supply --state-dir with its absolute directory. Only then restart. This clears the process safety block; it does not repair the executable.",
        TextKey::AddonListDiskWarnings => "Disk usage warnings",
        TextKey::AddonListDiskWarningGb => "This list: warn at (GB)",
        TextKey::AddonListsCombinedDiskWarningGb => "All add-on lists combined: warn at (GB)",
        TextKey::AddonListDiskWarningHelp => "Whole decimal GB (1 GB = 1,000,000,000 bytes). 0 disables this warning independently. Reaching or exceeding the threshold only shows a Health warning; it does not stop downloads or delete files.",
        TextKey::AddonListDiskWarningFailed => "Could not load or save the disk usage warning. Reload the page and try again.",
        TextKey::NextRuntimeAvailability => "Next runtime availability",
        TextKey::NextOverlayInvalid => "Next configuration overlay is unavailable or invalid",
        TextKey::NextOverlayIncomplete => "Next configuration overlay validation is incomplete",
        TextKey::JavascriptAudioSelectionTitle => "JavaScript audio selection",
        TextKey::JavascriptAudioSelectionHealthy => "No selector failures recorded since startup. Scripts that have not run are not validated.",
        TextKey::JavascriptAudioSelectionFailures => "{count} selector failures since startup; playback used built-in selection. Check scripts/audio-stream-selector and the ADR 0096 limits. The counter resets on restart.",
        TextKey::AddonListDiskUsageTitle => "Add-on list disk usage",
        TextKey::AddonListDiskUsageBrief => "Add-on list disk usage warning",
        TextKey::AddonListDiskUsageUnavailable => "Add-on list disk usage could not be measured completely. Check storage access and retry the health check.",
        TextKey::AddonListDiskUsageUnavailableBrief => "Add-on list disk usage could not be measured.",
        TextKey::AddonListDiskUsageLine => "List #{id}: {used} GB used; warning at {limit} GB.",
        TextKey::AddonListsCombinedDiskUsageLine => "Combined: {used} GB used; warning at {limit} GB.",
        TextKey::AddonMediaListUrl => "Add-on list URL",
        TextKey::AddonMediaListFolder => "Storage folder",
        TextKey::AddonMediaListFolderHelp => "Optional; derived from the list name",
        TextKey::DestructiveActions => "Destructive actions",
        TextKey::ReconcileEntireSource => "Reconcile entire source",
        TextKey::ReconcileEntireSourceHelp => {
            "Compare a complete source snapshot without changing the current publication or your item settings."
        }
        TextKey::ReconcileQueued => "Full-source reconciliation queued.",
        TextKey::ReconcileFailed => "Full-source reconciliation failed.",
        TextKey::ResetAndReload => "Reset and reload",
        TextKey::ResetAddonMediaListHelp => {
            "Remove every generated result for this list, then queue a complete import."
        }
        TextKey::SourceHost => "Source host",
        TextKey::ResetRemoves => "This reset removes",
        TextKey::ResetRemovesItems => {
            "Imported items, provider metadata, operator overrides, episode numbering, and replacements."
        }
        TextKey::ResetRemovesArtifacts => {
            "Generated media, NFO files, artwork, subtitles, caches, and archives."
        }
        TextKey::ResetRemovesRuntime => {
            "Jobs, checkpoints, revision artifacts, generated Playlist membership, and projected media."
        }
        TextKey::ResetRetains => "This reset retains",
        TextKey::ResetRetainsSettings => {
            "The list name, add-on/source/library binding, generated Playlist identity, refresh and publication policies, artifact quota, and other operator settings."
        }
        TextKey::ResetReloadFailureWarning => {
            "If the reload fails, channels and the generated Playlist remain empty."
        }
        TextKey::TypeListNameToConfirm => "Type the current list name to confirm",
        TextKey::ResetQueued => "Reset and reload queued.",
        TextKey::ResetAddonMediaListFailed => "Reset and reload failed.",
        TextKey::ChapterInput => "Chapters",
        TextKey::ChapterInputHelp => "Optional, one per line: 0:00 Introduction",
        TextKey::RestoreProviderChapters => "Restore provider chapters",
        TextKey::ManagedByAddon => "Managed by an add-on",
        TextKey::Trash => "Trash",
        TextKey::TrashAlreadyEmpty => "Trash is already empty.",
        TextKey::TrashEmpty => "Trash is empty.",
        TextKey::TrashEmptyConfirm => "Delete every file-not-found item from the database?",
        TextKey::TrashEmptyConfirmHelp => {
            "This permanently removes the currently missing media items and their dependent metadata rows."
        }
        TextKey::TrashSubtitle => {
            "Items whose source files are currently missing from disk or the provider."
        }
        TextKey::Troubleshooting => "Troubleshooting",
        TextKey::TunerCount => "Tuner Count",
        TextKey::HdhrDisabledByJwt => "HDHomeRun endpoints are disabled because IPTV JWT protection is enabled. These settings have no effect.",
        TextKey::TvShows => "TV Shows",
        TextKey::Ui => "UI",
        TextKey::UnavailableLocalMediaMessage => "There are {count} items that are unavailable because ErsatzRS cannot find them on disk, including the following: {items}",
        TextKey::UnavailableRemoteStreamsMessage => "There are {count} Remote Streams that are unavailable because their provider reports them as not playable, including the following: {items}",
        TextKey::Unavailable => "Unavailable",
        TextKey::UseActualTimes => "Use Actual Times",
        TextKey::UseCustomPlaybackOrder => "Use Custom Playback Order",
        TextKey::UseChaptersAsMediaItems => "Use Chapters As Media Items",
        TextKey::UseChaptersAsMediaItemsHelp => {
            "Schedule individual chapters instead of entire files"
        }
        TextKey::UseEmbeddedSubtitles => "Use Embedded Subtitles",
        TextKey::Uuid => "UUID",
        TextKey::UntitledItem => "Untitled item",
        TextKey::WindowsStartup => "Windows startup",
        TextKey::WindowsStartupNeedsRepair => {
            "The saved startup command points to another location. Apply this setting to repair it."
        }
        TextKey::WindowsStartupUnavailable => {
            "Windows startup is unavailable because the packaged launcher could not be found or the registration could not be read."
        }
        TextKey::ActiveTranscodeLimit => "Maximum Simultaneous Streaming Transcodes",
        TextKey::ActiveTranscodeLimitHelp => {
            "Hard ceiling for streaming FFmpeg transcodes. When all slots are busy, a new viewer gets an all-streams-in-use response until a slot frees; active streams are not bypassed over the limit."
        }
        TextKey::ActiveTranscodeSuggestionHardware => {
            "Hardware encoder detected; this is a suggested value, not an automatic change."
        }
        TextKey::ActiveTranscodeSuggestionSoftware => {
            "Software encoding only; this is a suggested value, not an automatic change."
        }
        TextKey::Peak => "peak",
        TextKey::StreamsInUse => "Streams in use",
        TextKey::SuggestedTranscodeLimit => "Suggested value",
        TextKey::Watermarks => "Watermarks",
        TextKey::Width => "Width",
        TextKey::WorkAheadSegmenterLimit => "Work-Ahead HLS Transcodes",
        TextKey::WorkAheadSegmenterLimitHelp => {
            "How many HLS transcode slots may run ahead at maximum speed to build buffer. Work-ahead always yields to watched streams."
        }
        TextKey::AccelerationKind => "Acceleration Kind",
        TextKey::AddFfmpegProfile => "Add FFmpeg Profile",
        TextKey::AddProfile => "Add Profile",
        TextKey::AllowBFrames => "Allow B-Frames",
        TextKey::AllowWatermarks => "Allow Watermarks",
        TextKey::Audio => "Audio",
        TextKey::AudioBitrate => "Audio Bitrate (kBit/s)",
        TextKey::AudioBufferSize => "Audio Buffer Size (kBit)",
        TextKey::AudioChannels => "Channels",
        TextKey::AudioFormat => "Audio Format",
        TextKey::BitDepth => "Bit Depth",
        TextKey::Copy => "Copy",
        TextKey::CopyBlock => "Copy Block",
        TextKey::CopyFailed => "Copy failed",
        TextKey::CopyNameSuffix => "Copy",
        TextKey::CopyProfile => "Copy Profile",
        TextKey::CopyTemplate => "Copy Template",
        TextKey::Crop => "Crop",
        TextKey::DeinterlaceVideo => "Deinterlace Video",
        TextKey::Edit => "Edit",
        TextKey::EditBlock => "Edit Block",
        TextKey::EditCollection => "Edit Collection",
        TextKey::EditFfmpegProfile => "Edit FFmpeg Profile",
        TextKey::EditMultiCollection => "Edit Multi Collection",
        TextKey::EditRerunCollection => "Edit Rerun Collection",
        TextKey::EditTemplate => "Edit Template",
        TextKey::Expression => "Expression",
        TextKey::Fallback => "Fallback",
        TextKey::FillerKind => "Filler Kind",
        TextKey::General => "General",
        TextKey::HardwareAcceleration => "Hardware Acceleration",
        TextKey::HardwareIfPossible => "Hardware If Possible",
        TextKey::ProfileDeleteDefault => "The default FFmpeg profile cannot be deleted.",
        TextKey::ProfileDeleteInUse => "This FFmpeg profile is used by a channel and cannot be deleted.",
        TextKey::ProfileDeleteFailed => "The FFmpeg profile could not be deleted.",
        TextKey::NewProfileName => "New Profile Name",
        TextKey::NoneOption => "(None)",
        TextKey::NumHelp => "the mid-roll point number, starting with 1",
        TextKey::OptionalDisplayTitle => "Optional display title...",
        TextKey::TranscodeAudio => "Transcode Audio",
        TextKey::NormalizeColors => "Normalize Colors",
        TextKey::NormalizeFramerate => "Normalize Framerate",
        TextKey::NormalizeLoudness => "Normalize Loudness",
        TextKey::TranscodeVideo => "Transcode Video",
        TextKey::Off => "Off",
        TextKey::Pad => "Pad",
        TextKey::PadNearestFifteenMinutes => "15 (:00, :15, :30, :45)",
        TextKey::PadNearestFiveMinutes => "5 (:00, :05, :10, :15, :20, etc)",
        TextKey::PadNearestTenMinutes => "10 (:00, :10, :20, :30, :40, :50)",
        TextKey::PadNearestThirtyMinutes => "30 (:00, :30)",
        TextKey::PadMode => "Pad Mode",
        TextKey::PadToNearestMinute => "Pad To Nearest Minute",
        TextKey::PointHelp => "the position of the mid-roll point, in seconds",
        TextKey::ProfileCopied => "Profile copied.",
        TextKey::ProfileNotFound => "Profile not found.",
        TextKey::QsvExtraHardwareFrames => "QSV Extra Hardware Frames",
        TextKey::Resolution => "Resolution",
        TextKey::SampleRateKhz => "Sample Rate (kHz)",
        TextKey::SaveDecoTemplate => "Save Deco Template",
        TextKey::SaveProfile => "Save Profile",
        TextKey::SaveFillerPreset => "Save Filler Preset",
        TextKey::SaveRerunCollection => "Save Rerun Collection",
        TextKey::SaveSequentialSchedule => "Save Sequential Schedule",
        TextKey::SaveTemplate => "Save Template",
        TextKey::SaveBlock => "Save Block",
        TextKey::ScaleAndPad => "Scale and Pad",
        TextKey::ScalingBehavior => "Scaling Behavior",
        TextKey::Software => "Software",
        TextKey::Stretch => "Stretch",
        TextKey::TargetLoudnessLufs => "Target Loudness (LUFS)",
        TextKey::ThreadCountAuto => "Thread Count (0 = auto)",
        TextKey::OutputDynamicRange => "Output Dynamic Range",
        TextKey::OutputDynamicRangeCopyHint => {
            "Video is copied, so the output keeps the source dynamic range."
        }
        TextKey::HdrStaticMetadataHint => {
            "Leave blank to use the documented HDR10 defaults. These apply only when the source carries no HDR metadata of its own; a source's own mastering display and light levels always win."
        }
        TextKey::HdrMasteringDisplay => "Mastering Display",
        TextKey::HdrMaxCll => "MaxCLL (cd/m²)",
        TextKey::HdrMaxFall => "MaxFALL (cd/m²)",
        TextKey::TonemapAlgorithm => "Tonemap Algorithm",
        TextKey::VaapiDevicePath => "VAAPI Device Path",
        TextKey::VaapiDisplay => "VAAPI Display",
        TextKey::VaapiDriver => "VAAPI Driver",
        TextKey::Video => "Video",
        TextKey::VideoBitrate => "Video Bitrate (kBit/s)",
        TextKey::VideoBufferSize => "Video Buffer Size (kBit)",
        TextKey::VideoFormat => "Video Format",
        TextKey::VideoPreset => "Video Preset",
        TextKey::VideoTune => "Video Tune",
        TextKey::VideoProfile => "Video Profile",
        TextKey::ViewCollection => "View Collection",
        TextKey::ViewPlaylist => "View Playlist",
        TextKey::AddAll => "Add All",
        TextKey::AddAllToCollection => "Add All to Collection",
        TextKey::AddAllToPlaylist => "Add All to Playlist",
        TextKey::AddedItemToPlaylist => "Added item to playlist.",
        TextKey::AddedArtistToSchedule => "Added artist to schedule",
        TextKey::AddedMovieToSchedule => "Added movie to schedule",
        TextKey::AddedSeasonToSchedule => "Added season to schedule",
        TextKey::AddedShowToSchedule => "Added show to schedule",
        TextKey::AddingItem => "Adding item...",
        TextKey::AddingItemToPlaylist => "Adding item to playlist...",
        TextKey::Artists => "Artists",
        TextKey::ClearSelection => "Clear Selection",
        TextKey::Collection => "Collection",
        TextKey::CollectionNameRequired => "Collection name is required",
        TextKey::CollectionNotFound => "Collection not found",
        TextKey::CollectionRenamed => "Collection renamed.",
        TextKey::EditAddonMediaListProperties => "Edit add-on List Properties",
        TextKey::EnterAddonMediaListUrl => "Enter add-on List URL",
        TextKey::EnterSearchQuery => "Enter a search query.",
        TextKey::Episodes => "Episodes",
        TextKey::LoadingCollections => "Loading collections...",
        TextKey::LoadingPlaylistGroups => "Loading playlist groups...",
        TextKey::LoadingPlaylists => "Loading playlists...",
        TextKey::GeneratePlaylist => "Create playlist in original order",
        TextKey::GeneratePlaylistFromSortedAddonMediaListItems => {
            "Keep a generated playlist in the provider's original order after each publish"
        }
        TextKey::MusicVideos => "Music Videos",
        TextKey::NewCollectionName => "New Collection Name",
        TextKey::NoMediaMatched => "No media matched.",
        TextKey::NoItemsToAdd => "No items to add.",
        TextKey::NoItemsToPreview => "No items to preview.",
        TextKey::NoPlaylistsInGroup => "No playlists in this group.",
        TextKey::PleaseChoosePlaylist => "Please choose a playlist.",
        TextKey::RequestFailed => "Request failed",
        TextKey::Released => "Released",
        TextKey::SaveAs => "Save As",
        TextKey::Seasons => "Seasons",
        TextKey::SeeAll => "See All >>",
        TextKey::SelectAll => "Select All",
        TextKey::SelectGroup => "(Select a group)",
        TextKey::SelectPlaylist => "(Select a playlist)",
        TextKey::SelectSearchCollectionHelp => {
            "Select the collection to add the search results to."
        }
        TextKey::SelectSearchPlaylistHelp => "Select the playlist to add the search results to.",
        TextKey::Selected => "Selected",
        TextKey::SelectCollectionToAddThisMovie => "Select a collection to add this movie.",
        TextKey::SelectPlaylistToAddThisMovie => "Select a playlist to add this movie.",
        TextKey::Add => "Add",
        TextKey::AddLibrary => "Add Library",
        TextKey::Address => "Address",
        TextKey::AddressRequired => "Address is required.",
        TextKey::ApiKey => "API Key",
        TextKey::ApiKeyRequired => "API key is required.",
        TextKey::AuthorizedRedirecting => "Authorized - redirecting...",
        TextKey::CheckingEveryTwoSeconds => "Checking every 2 seconds...",
        TextKey::ClickAuthorizePlex => "Click here to authorize on Plex.tv",
        TextKey::ClickPlayoutItemHelp => {
            "Click a playout item to inspect its title, times, and scheduling context."
        }
        TextKey::ConfigureConnection => "Configure Connection",
        TextKey::Connection => "Connection",
        TextKey::DeepScanCollections => "Deep Scan Collections",
        TextKey::DeepScanLibrary => "Deep Scan Library",
        TextKey::DeepScan => "Deep Scan",
        TextKey::EditLibrary => "Edit Library",
        TextKey::EditPlaylist => "Edit Playlist",
        TextKey::EnterOnePathPerLine => "Enter one path per line.",
        TextKey::Error => "Error",
        TextKey::ExternalCollections => "External Collections",
        TextKey::Information => "Information",
        TextKey::Kind => "Kind",
        TextKey::Library => "Library",
        TextKey::LibraryKind => "Library Kind",
        TextKey::LibraryPaths => "Library Paths",
        TextKey::LocalLibraryPathAbsolute => "Path [{path}] must be an absolute path.",
        TextKey::LocalLibraryPathExists => "Path [{path}] must be an existing directory.",
        TextKey::LocalLibraryPathConflict => "Path [{path}] overlaps another library path.",
        TextKey::LocalLibraries => "Local Libraries",
        TextKey::LocalPath => "Local Path",
        TextKey::MediaKind => "Media Kind",
        TextKey::NoLibrariesConfigured => "No libraries configured. Add a media source first.",
        TextKey::NoSchedulingContext => "No scheduling context",
        TextKey::NoPlayoutItemsInWindow => "No playout items in this 24-hour window",
        TextKey::NoPlayoutsFound => "No playouts found.",
        TextKey::NoPathReplacements => "No path replacements.",
        TextKey::Paths => "Paths",
        TextKey::PathReplacements => "Path Replacements",
        TextKey::PlexSignInHelp => "Sign in to Plex to add a media source.",
        TextKey::Remote => "Remote",
        TextKey::RemotePath => "Remote Path",
        TextKey::Remove => "Remove",
        TextKey::RemoveBlockFromTemplate => "Remove Block From Template",
        TextKey::RemoveContent => "Remove Content",
        TextKey::Retry => "Retry",
        TextKey::Saved => "Saved.",
        TextKey::Saving => "Saving...",
        TextKey::QuickScan => "Quick Scan",
        TextKey::ScanCollections => "Scan Collections",
        TextKey::ScanFailed => "Scan failed",
        TextKey::ScanLibrary => "Scan Library",
        TextKey::ScanQueued => "Scan queued",
        TextKey::Scanning => "Scanning...",
        TextKey::Server => "Server",
        TextKey::ServerAddress => "Server Address",
        TextKey::PlexRefreshFailed => "Could not refresh Plex libraries. Check the server connection and try again.",
        TextKey::ReauthenticatePlex => "Re-authenticate with Plex",
        TextKey::PlexMissingServer => "Server missing from Plex",
        TextKey::PlexMissingHelp => "This server is no longer listed in your Plex account. Re-claim it in Plex or remove it here. Content is retained and scans are paused.",
        TextKey::PlexSignedOutHelp => "Sign in again to restore Plex access. Your media sources and synchronized content are retained.",
        TextKey::PlexRemoveServerHelp => "Removing this server deletes its libraries and synchronized content from ErsatzRS. Files on the Plex server are not deleted.",
        TextKey::PlexSignInFailed => "Plex sign-in failed or expired. Try again.",
        TextKey::SignInToPlex => "Sign in to Plex",
        TextKey::SignOutOfPlex => "Sign out of Plex",
        TextKey::Sync => "Sync",
        TextKey::WaitingForPlexAuthorization => "Waiting for Plex authorization.",
        TextKey::AddChannel => "Add Channel",
        TextKey::Any => "Any",
        TextKey::BlockPlayoutTroubleshooting => "Block Playout Troubleshooting",
        TextKey::BlockPlayoutHistory => "Block Playout History",
        TextKey::Categories => "Categories",
        TextKey::CategoriesHelp => "Comma-separated list of categories",
        TextKey::Channel => "Channel",
        TextKey::ChannelCouldNotBeSaved => {
            "Channel could not be saved. Check the form values and try again."
        }
        TextKey::ChannelDisabled => "Channel disabled",
        TextKey::ChannelHiddenFromEpg => "Channel hidden from EPG",
        TextKey::ChannelLogo => "Channel Logo",
        TextKey::ChannelLogoPreview => "Channel logo preview",
        TextKey::ChannelLogoUploadFailed => {
            "Channel logo upload failed. Check the file and try again."
        }
        TextKey::Check => "Check",
        TextKey::Continuous => "Continuous",
        TextKey::DateAndTime => "Date and Time",
        TextKey::DefaultOption => "Default",
        TextKey::DefaultSchedule => "Default Schedule",
        TextKey::DeleteChannel => "Delete channel",
        TextKey::DownloadMediaSample => "Download Media Sample",
        TextKey::DownloadResults => "Download Results",
        TextKey::DisableWatermarks => "Disable Watermarks",
        TextKey::EditChannel => "Edit Channel",
        TextKey::EditChannelNumbers => "Edit Channel Numbers",
        TextKey::EditNumbers => "Edit Numbers",
        TextKey::ExternalLogoUrl => "External Logo URL",
        TextKey::Fail => "Fail",
        TextKey::FallbackFiller => "Fallback Filler",
        TextKey::FfmpegProfile => "FFmpeg Profile",
        TextKey::FirstPage => "First page",
        TextKey::FirstRunPlaybackOrder => "First Run Playback Order",
        TextKey::Forced => "Forced",
        TextKey::GenerateSubtitles => "Generate Subtitles",
        TextKey::Generated => "Generated",
        TextKey::Details => "Details",
        TextKey::Full => "Full",
        TextKey::Group => "Group",
        TextKey::FfmpegRuntime => "FFmpeg Runtime",
        TextKey::FfmpegRuntimeDownloaded => "FFmpeg runtime downloaded",
        TextKey::FfmpegRuntimeLatestRelease => "Latest Release",
        TextKey::FfmpegRuntimeRelease => "Installed Release",
        TextKey::FfmpegRuntimeSource => "Source",
        TextKey::FfmpegRuntimeSourceDownloaded => "Downloaded",
        TextKey::FfmpegRuntimeSourceManaged => "Managed",
        TextKey::FfmpegRuntimeSourceOverride => "Override",
        TextKey::FfmpegRuntimeSourcePath => "PATH",
        TextKey::FfmpegRuntimeSourceUnknown => "Unknown",
        TextKey::FfmpegRuntimeUpdateAvailable => "Update available",
        TextKey::FfmpegRuntimeUpdateCheckFailed => "Update check failed",
        TextKey::FfmpegRuntimeUpdateFailed => "FFmpeg runtime update failed",
        TextKey::FfmpegRuntimeUpToDate => "Up to date",
        TextKey::FfmpegRuntimeUpdated => "FFmpeg runtime updated",
        TextKey::FfmpegRuntimeVersion => "Version",
        TextKey::HealthNoPersistedRun => "No health checks are applicable right now.",
        TextKey::HealthPersistedRunHelp => {
            "Results are cached for five minutes. Refresh to check again now. This page also refreshes when media scans update item state."
        }
        TextKey::HistoryDetails => "History Details",
        TextKey::HlsDirect => "HLS Direct",
        TextKey::HlsSegmenter => "HLS Segmenter",
        TextKey::Hours => "hours",
        TextKey::IdleBehavior => "Idle Behavior",
        TextKey::IdleBehaviorHelp => {
            "Controls how the transcoding process behaves after all clients have disconnected"
        }
        TextKey::Info => "Info",
        TextKey::Import => "Import",
        TextKey::IsEnabled => "Is Enabled",
        TextKey::Json => "JSON",
        TextKey::KeepRunning => "Keep Running",
        TextKey::Key => "Key",
        TextKey::LastMidFillerHelp => "seconds since last mid-roll filler",
        TextKey::LastPage => "Last page",
        TextKey::Level => "Level",
        TextKey::LoadingMediaInfo => "Loading media info...",
        TextKey::Logs => "Logs",
        TextKey::PlaybackSpeed => "Speed",
        TextKey::Logo => "Logo",
        TextKey::MediaItem => "Media item",
        TextKey::Message => "Message",
        TextKey::MissingMediaItemQueryParameter => "Missing mediaItem query parameter",
        TextKey::Mirror => "Mirror",
        TextKey::MirrorOptionsHelp => "Available only when Playout Source is set to Mirror.",
        TextKey::MirrorSourceChannel => "Mirror Source Channel",
        TextKey::MusicVideoCreditsMode => "Music Video Credits Mode",
        TextKey::MusicVideoCreditsTemplate => "Music Video Credits Template",
        TextKey::NoMatchingRecordsFound => "No matching records found",
        TextKey::NoFfmpegProfiles => "No FFmpeg profiles found. Create one first.",
        TextKey::NoBlockPlayoutSelected => "Select a block playout to inspect its blocks.",
        TextKey::NoBlocks => "No blocks found.",
        TextKey::NoHistory => "No history found.",
        TextKey::NoMessages => "No messages.",
        TextKey::NvidiaCapabilitiesNotCollected => {
            "NVIDIA capabilities have not been collected yet."
        }
        TextKey::Number => "Number",
        TextKey::OnDemand => "On Demand",
        TextKey::MatchedPointsHelp => {
            "number of mid-roll points that have already matched the expression"
        }
        TextKey::Pass => "Pass",
        TextKey::ParityDebt => "ParityDebt",
        TextKey::PlaybackTroubleshooting => "Playback Troubleshooting",
        TextKey::Play => "Play",
        TextKey::PlayoutTimeline => "Playout Timeline",
        TextKey::PlayoutMode => "Playout Mode",
        TextKey::PlayoutModeHelp => "Controls the progression of the channel's playout",
        TextKey::PlayoutOffset => "Playout Offset",
        TextKey::PlayoutSource => "Playout Source",
        TextKey::PlayoutSourceHelp => {
            "Controls the source of the channel's content. This cannot be changed when the channel already has a playout."
        }
        TextKey::Preview => "Preview",
        TextKey::PreviewBlockPlayout => "Preview Block Playout",
        TextKey::PreviewChannel => "Preview channel",
        TextKey::ChannelPreview => "Channel Preview",
        TextKey::ReloadStream => "Reload Stream",
        TextKey::StopPreview => "Stop Preview",
        TextKey::OpenFullPlayer => "Open Full Player",
        TextKey::BackToChannels => "Back to channels",
        TextKey::NowPlaying => "Now Playing",
        TextKey::UpNext => "Up Next",
        TextKey::ProgrammeGuide => "Programme Guide",
        TextKey::EpgPreview => "EPG Preview",
        TextKey::EpgPreviewDescription => {
            "Browse the published guide with two hours of history and 24 hours ahead."
        }
        TextKey::EpgZoom => "Zoom",
        TextKey::EpgNow => "Now",
        TextKey::NoEpgChannels => "No enabled channels are visible in the EPG.",
        TextKey::ScheduleGap => "Schedule gap",
        TextKey::Filler => "Filler",
        TextKey::NoProgramming => "No programming is scheduled in this window.",
        TextKey::ChannelDisabledPreview => "This channel is disabled.",
        TextKey::ChannelNotAiring => "This channel is not currently airing.",
        TextKey::ChannelNoPlayout => "This channel has no playout.",
        TextKey::PreviewIdle => "The preview starts muted automatically.",
        TextKey::PreviewConnecting => "Connecting to the live stream…",
        TextKey::PreviewLive => "Live playback is running.",
        TextKey::PreviewPaused => "Preview paused.",
        TextKey::PreviewRecovering => "Recovering playback…",
        TextKey::PreviewStopped => "Preview stopped.",
        TextKey::PreviewUnsupported => "This browser cannot play HLS video.",
        TextKey::PreviewAuthorizationFailed => "Preview authorization failed.",
        TextKey::PreviewQueued => "Preview queued",
        TextKey::PreviewRunning => "Preview is rendering",
        TextKey::PreviewCompleted => "Rendering completed",
        TextKey::PreviewCancelling => "Stopping preview; waiting for cleanup",
        TextKey::PreviewProgrammeSeek => "Seconds from programme start",
        TextKey::PreviewProgrammeSeekHelp => "Leave empty to join at the selected UTC time. Seeking stays within the selected programme.",
        TextKey::PreviewTimestamp => "Preview timestamp (UTC):",
        TextKey::PreviewAlreadyRunning => "A diagnostic preview is already running. Wait for it to finish before starting another.",
        TextKey::PreviewCleanupUnconfirmed => "Preview cleanup could not be confirmed. Restart the server before trying again.",
        TextKey::PreviewConfigurationInvalid => "The channel streaming mode is invalid. Save a supported mode in the channel editor and retry.",
        TextKey::PreviewStartupFailed => "The preview could not start. Check the server log for the startup error and verify the media and FFmpeg profile.",
        TextKey::PreviewUnavailable => "The live stream is unavailable.",
        TextKey::PreviewCapacityReached => "All transcode slots are currently in use.",
        TextKey::PreviewNetworkError => "The preview lost its network connection.",
        TextKey::PreviewMediaError => {
            "The browser could not decode this channel's configured output."
        }
        TextKey::PreviewDerivativeNote => {
            "Admin preview uses a browser-compatible H.264/AAC derivative; the configured channel output is unchanged."
        }
        TextKey::JavascriptRequiredPlayback => "JavaScript is required for interactive playback.",
        TextKey::ConfirmStopTranscodeSession => {
            "Stop the shared transcode session for this channel?"
        }
        TextKey::PreviewPlaylistPlayout => "Preview Playlist Playout",
        TextKey::PreferredAudioTitle => "Preferred Audio Title",
        TextKey::PreferredSubtitleLanguage => "Preferred Subtitle Language",
        TextKey::QsvCapabilitiesNotCollected => "QSV capabilities have not been collected yet.",
        TextKey::RemoveStoredLogo => "Remove Stored Logo",
        TextKey::SaveChannel => "Save Channel",
        TextKey::SaveChannels => "Save Channels",
        TextKey::Second => "second",
        TextKey::Seconds => "seconds",
        TextKey::SelectProfile => "- Select profile -",
        TextKey::SequentialSchedule => "Sequential Schedule",
        TextKey::SequentialScheduleValidation => "Sequential Schedule Validation",
        TextKey::SeekSeconds => "Seek Seconds",
        TextKey::Schema => "Schema",
        TextKey::ShowDisabled => "Show Disabled",
        TextKey::ShowInEpg => "Show In EPG",
        TextKey::SlugSeconds => "Slug Seconds",
        TextKey::SlugSecondsHelp => {
            "Seconds of black video and silent audio to insert between every playout item"
        }
        TextKey::Song => "Song",
        TextKey::SongVideoMode => "Song Video Mode",
        TextKey::SortById => "Sort by ID",
        TextKey::SortByLevel => "Sort by Level",
        TextKey::SortByName => "Sort by Name",
        TextKey::SortByTimestamp => "Sort by Timestamp",
        TextKey::StopOnDisconnect => "Stop On Disconnect",
        TextKey::StopScheduling => "Stop Scheduling",
        TextKey::StopSchedulingBlockItems => "Stop scheduling block items",
        TextKey::StopTranscodeSession => "Stop Transcode Session",
        TextKey::StartFromBeginning => "Start From Beginning",
        TextKey::StreamSelector => "Stream Selector",
        TextKey::StreamSelectorMode => "Stream Selector Mode",
        TextKey::InvalidNextEngineTextSubtitleMode => "Invalid Next engine text subtitle mode",
        TextKey::NextEngineTextSubtitleModeHelp => "Converted text subtitles are available through HLS only. Select Burn for subtitles in MPEG-TS.",
        TextKey::NextEngineTextSubtitleMode => "Next Engine Text Subtitle Mode",
        TextKey::SubtitleBurn => "Burn",
        TextKey::SubtitleConvertIfPossible => "Convert If Possible",
        TextKey::PreviewCaptions => "Captions",
        TextKey::StreamingEngine => "Streaming Engine",
        TextKey::InvalidStreamingEngine => "Invalid streaming engine",
        TextKey::StreamingMode => "Streaming Mode",
        TextKey::Styles => "Styles",
        TextKey::Subtitle => "Subtitle",
        TextKey::SubtitleMode => "Subtitle Mode",
        TextKey::SubtitleModeHelp => {
            "Forced shows only forced-narrative tracks. To always show subtitles in your \
             preferred language, choose Any."
        }
        TextKey::Timestamp => "Timestamp",
        TextKey::Timeline => "Timeline",
        TextKey::Target => "Target",
        TextKey::Tags => "Tags",
        TextKey::Tools => "Tools",
        TextKey::TranscodeMode => "Transcode Mode",
        TextKey::TranscodeModeHelp => "Controls when the transcoding process is active",
        TextKey::TroubleshootPlayback => "Troubleshoot Playback",
        TextKey::TroubleshootChannel => "Troubleshoot channel",
        TextKey::TroubleshootingReports => "Troubleshooting reports",
        TextKey::MpegTsTroubleshooting => "MPEG-TS Troubleshooting",
        TextKey::InternalMpegTs => "Internal MPEG-TS",
        TextKey::PublicMpegTs => "Public MPEG-TS",
        TextKey::RetainRawSample => "Retain raw sample",
        TextKey::TsdDuckVersion => "TSDuck version",
        TextKey::Report => "Report",
        TextKey::RunId => "Run ID",
        TextKey::Status => "Status",
        TextKey::Summary => "Summary",
        TextKey::SampleBytes => "Sample bytes",
        TextKey::RawSampleArchived => "Raw sample archived",
        TextKey::ArchivePath => "Archive path",
        TextKey::SampleUrl => "Sample URL",
        TextKey::StabilityMeter => "Stability meter",
        TextKey::Stable => "Stable",
        TextKey::Packets => "packets",
        TextKey::Services => "Services",
        TextKey::Pids => "PIDs",
        TextKey::ContinuityFindings => "Continuity findings",
        TextKey::PcrFindings => "PCR findings",
        TextKey::Bitrate => "Bitrate",
        TextKey::AnalysisJson => "Analysis JSON",
        TextKey::StandardOutput => "Standard output",
        TextKey::StandardError => "Standard error",
        TextKey::NotAvailable => "Not available",
        TextKey::NotApplicable => "Not applicable",
        TextKey::Completed => "Completed",
        TextKey::Failed => "Failed",
        TextKey::TroubleshootingPlaybackStartDebt => {
            "Media-item and channel playback preview now write FFmpeg reports, logs, media_info.json, troubleshooting_info.json, selected stream-selector YAML, capability files, execute stream-selector rules, and redirect to /iptv/session/.troubleshooting/live.m3u8."
        }
        TextKey::UnableToLoadMediaInfo => "Unable to load media info",
        TextKey::UnableToLoadPlaybackTroubleshooting => {
            "Unable to load playback troubleshooting context"
        }
        TextKey::UploadLogo => "Upload Logo",
        TextKey::UploadLogoHelp => {
            "Choose a local image to replace the stored channel logo on save"
        }
        TextKey::Warning => "Warning",
        TextKey::Watermark => "Watermark",
        TextKey::WatermarkIds => "Watermark IDs",
        TextKey::Window => "Window",
        TextKey::WithProgress => "With Progress",
        TextKey::Writers => "Writers",
        TextKey::Validate => "Validate",
        TextKey::Yaml => "YAML",
        TextKey::YamlFile => "YAML File",
        TextKey::Messages => "Messages",
        TextKey::CollectionName => "Collection Name",
        TextKey::MediaItemType => "Media Item Type",
        TextKey::MediaItemTitle => "Media Item Title",
        TextKey::ActualSize => "Actual Size",
        TextKey::AddWatermark => "Add Watermark",
        TextKey::AspectRatioGuides => "Aspect ratio guides",
        TextKey::BottomLeft => "Bottom Left",
        TextKey::BottomMiddle => "Bottom Middle",
        TextKey::BottomRight => "Bottom Right",
        TextKey::CopyWatermark => "Copy Watermark",
        TextKey::CopyOf => "Copy of",
        TextKey::CurrentImage => "Current image",
        TextKey::Custom => "Custom",
        TextKey::ChannelLogoPreviewUnavailable => {
            "Channel logo previews require a channel context. Choose a custom image to preview here."
        }
        TextKey::DeleteWatermark => "Delete watermark",
        TextKey::DurationSeconds => "Duration (seconds)",
        TextKey::EditWatermark => "Edit Watermark",
        TextKey::EnableWatermarkToPreview => "Enable the watermark to see its placement.",
        TextKey::Frequency => "Frequency",
        TextKey::GeneratingPreview => "Generating preview...",
        TextKey::HorizontalMarginPercent => "Horizontal Margin (%)",
        TextKey::GuideFourThree => "4:3 safe frame",
        TextKey::Identity => "Identity",
        TextKey::Image => "Image",
        TextKey::ImageSource => "Image Source",
        TextKey::Intermittent => "Intermittent",
        TextKey::IntermittentTiming => "Intermittent Timing",
        TextKey::LeftMiddle => "Left Middle",
        TextKey::Location => "Location",
        TextKey::MiddleCenter => "Middle Center",
        TextKey::Mode => "Mode",
        TextKey::Minutes => "minutes",
        TextKey::Moods => "Moods",
        TextKey::NewName => "New Name",
        TextKey::NativeAssetDimensions => "Native asset and rendered size",
        TextKey::NextWatermark => "Next watermark",
        TextKey::Opacity => "Opacity",
        TextKey::OpacityExpression => "Opacity Expression",
        TextKey::OpacityOneToHundred => "Opacity (1-100)",
        TextKey::Permanent => "Permanent",
        TextKey::PerPass => "per pass",
        TextKey::PlaceWithinSourceContent => "Place Within Source Content",
        TextKey::Placement => "Placement",
        TextKey::PlacementPreview => "Placement Preview",
        TextKey::PreviewFailed => "Preview failed",
        TextKey::PreviewReferenceFullHd => "Full HD reference frame · 1920×1080",
        TextKey::PreviousWatermark => "Previous watermark",
        TextKey::RightMiddle => "Right Middle",
        TextKey::SaveWatermark => "Save Watermark",
        TextKey::Scaled => "Scaled",
        TextKey::SelectCustomImageToPreview => "Select or upload a custom image to preview.",
        TextKey::Size => "Size",
        TextKey::TopLeft => "Top Left",
        TextKey::TopMiddle => "Top Middle",
        TextKey::TopRight => "Top Right",
        TextKey::UploadFailed => "Upload failed.",
        TextKey::UploadImage => "Upload image",
        TextKey::UnsavedChanges => "Unsaved changes",
        TextKey::UnsavedWatermarkChangesMessage => {
            "Discard your watermark edits and open the selected watermark?"
        }
        TextKey::VerticalMarginPercent => "Vertical Margin (%)",
        TextKey::WatermarkCopied => "Watermark copied.",
        TextKey::WatermarkNotFound => "Watermark not found.",
        TextKey::WatermarkPlacementPreview => "watermark placement preview",
        TextKey::WatermarkPreview => "watermark preview",
        TextKey::WatermarkNavigation => "Watermark navigation",
        TextKey::WidthPercent => "Width (%)",
        TextKey::ZIndex => "Z-Index",
        TextKey::DiscardChanges => "Discard changes",
        TextKey::XmltvBlockBehavior => "XMLTV Block Behavior",
        TextKey::XmltvTimeZone => "XMLTV Time Zone",
        TextKey::Xmltv => "XMLTV",
        TextKey::Badge => "Badge",
        TextKey::Buttons => "Buttons",
        TextKey::CheckValue => "Check",
        TextKey::ChannelName => "Channel Name",
        TextKey::ChannelNumber => "Channel Number",
        TextKey::ComponentGallery => "ErsatzRS Component Gallery",
        TextKey::DangerButton => "Danger",
        TextKey::DeleteDialog => "Delete Dialog",
        TextKey::Decrement => "Decrement",
        TextKey::Dialogs => "Dialogs",
        TextKey::DisabledButton => "Disabled",
        TextKey::EnterName => "Enter name",
        TextKey::ErrorOccurred => "An error occurred.",
        TextKey::ErrorToast => "Error toast",
        TextKey::ExampleModal => "Example Modal",
        TextKey::ExampleModalBody => "This is an example modal dialog body.",
        TextKey::FormInputs => "Form Inputs",
        TextKey::Icons => "Icons",
        TextKey::IncrementBadge => "Increment badge",
        TextKey::InfoToast => "Info toast",
        TextKey::JustFyi => "Just FYI.",
        TextKey::LinkButton => "Link Button",
        TextKey::ModalCancelled => "Cancelled.",
        TextKey::ModalConfirmed => "Confirmed!",
        TextKey::NumValue => "Num",
        TextKey::OpenModal => "Open Modal",
        TextKey::OperationSucceeded => "Operation succeeded.",
        TextKey::PagedTableDemo => "PagedTable (5 rows per page, 7 total)",
        TextKey::PrimaryButton => "Primary",
        TextKey::SecondaryButton => "Secondary",
        TextKey::SelectValue => "Select",
        TextKey::SomethingMightBeWrong => "Something might be wrong.",
        TextKey::SuccessToast => "Success toast",
        TextKey::TextButton => "Text",
        TextKey::TextValue => "Text",
        TextKey::Toasts => "Toasts",
        TextKey::WarningToast => "Warning toast",
        TextKey::YamlScript => "YAML Script",
        TextKey::Yes => "Yes",
    }
}

fn schedule_error_message(error: &str) -> String {
    match error.trim() {
        "Schedule name is required!" => "Schedule name is required!".to_owned(),
        "New Schedule Name is required." => "New Schedule Name is required.".to_owned(),
        "Schedule name must be 50 characters or less." => {
            "Schedule name must be 50 characters or less.".to_owned()
        }
        "Schedule name must be unique" => "Schedule name must be unique".to_owned(),
        "Schedule does not exist" => "Schedule does not exist".to_owned(),
        "[ProgramScheduleId] does not exist." => "[ProgramScheduleId] does not exist.".to_owned(),
        "A collection must use the same playback order in every schedule item." => {
            "A collection must use the same playback order in every schedule item.".to_owned()
        }
        "Shuffle In Order cannot be used with Fill With Group Mode." => {
            "Shuffle In Order cannot be used with Fill With Group Mode.".to_owned()
        }
        "Fixed schedule items require a start time." => {
            "Fixed schedule items require a start time.".to_owned()
        }
        "Multi Collection items can only use Shuffle or Shuffle In Order." => {
            "Multi Collection items can only use Shuffle or Shuffle In Order.".to_owned()
        }
        "Playlist Item Size can only be used with Playlist schedule items." => {
            "Playlist Item Size can only be used with Playlist schedule items.".to_owned()
        }
        "Multiple playout mode with Count requires a count expression." => {
            "Multiple playout mode with Count requires a count expression.".to_owned()
        }
        "Duration playout mode requires a duration between 1 minute and 24 hours." => {
            "Duration playout mode requires a duration between 1 minute and 24 hours.".to_owned()
        }
        "Duration playout mode requires discard-to-fill attempts." => {
            "Duration playout mode requires discard-to-fill attempts.".to_owned()
        }
        "Tail Mode Filler requires a tail filler preset." => {
            "Tail Mode Filler requires a tail filler preset.".to_owned()
        }
        "Tail filler can only be set when Tail Mode is Filler." => {
            "Tail filler can only be set when Tail Mode is Filler.".to_owned()
        }
        "Collection is required." => "Collection is required.".to_owned(),
        "Media item is required." => "Media item is required.".to_owned(),
        "Multi Collection is required." => "Multi Collection is required.".to_owned(),
        "Smart Collection is required." => "Smart Collection is required.".to_owned(),
        "Playlist is required." => "Playlist is required.".to_owned(),
        "Rerun Collection is required." => "Rerun Collection is required.".to_owned(),
        "Search Query is required." => "Search Query is required.".to_owned(),
        "Only one pre-, mid-, or post-roll filler can pad to the nearest minute." => {
            "Only one pre-, mid-, or post-roll filler can pad to the nearest minute.".to_owned()
        }
        "Padding filler requires a fallback filler." => {
            "Padding filler requires a fallback filler.".to_owned()
        }
        _ => error.to_owned(),
    }
}

fn scheduling_content_error_message(error: &str) -> String {
    let trimmed = error.trim();
    if trimmed.starts_with("Block group name \"") && trimmed.ends_with("\" is invalid") {
        return trimmed.to_owned();
    }
    if trimmed.starts_with("Block name \"") && trimmed.ends_with("\" is invalid") {
        return trimmed.to_owned();
    }
    if trimmed.starts_with("Template name \"") && trimmed.ends_with("\" is invalid") {
        return trimmed.to_owned();
    }
    if trimmed.starts_with("A template named \"")
        && trimmed.ends_with("\" already exists in that template group")
    {
        return trimmed.to_owned();
    }
    if trimmed.starts_with("Block from ") && trimmed.contains(" intersects block from ") {
        return trimmed.to_owned();
    }
    if trimmed.starts_with("TemplateGroup ") && trimmed.ends_with(" does not exist.") {
        return trimmed.to_owned();
    }
    if trimmed.starts_with("Template ") && trimmed.ends_with(" does not exist.") {
        return trimmed.to_owned();
    }
    if trimmed.starts_with("DecoGroup ") && trimmed.ends_with(" does not exist.") {
        return "Deco group does not exist.".to_owned();
    }
    if trimmed.starts_with("Deco ") && trimmed.ends_with(" does not exist.") {
        return "Deco does not exist.".to_owned();
    }
    if trimmed.starts_with("DecoTemplateGroup ") && trimmed.ends_with(" does not exist.") {
        return "Deco template group does not exist.".to_owned();
    }
    if trimmed.starts_with("DecoTemplate ") && trimmed.ends_with(" does not exist.") {
        return "Deco template does not exist.".to_owned();
    }
    if trimmed.starts_with("Deco name \"") && trimmed.ends_with("\" is invalid") {
        return "Deco name must be 50 characters or fewer.".to_owned();
    }
    if trimmed.starts_with("A deco named \"")
        && trimmed.ends_with("\" already exists in that deco group")
    {
        return "A deco with that name already exists in that deco group.".to_owned();
    }
    if trimmed.starts_with("Deco template name \"") && trimmed.ends_with("\" is invalid") {
        return "Deco template name must be 50 characters or fewer.".to_owned();
    }
    if trimmed.starts_with("A deco template named \"")
        && trimmed.ends_with("\" already exists in that deco template group")
    {
        return "A deco template with that name already exists in that deco template group."
            .to_owned();
    }
    if trimmed.starts_with("Watermark ") && trimmed.ends_with(" does not exist.") {
        return "Watermark does not exist.".to_owned();
    }
    if trimmed.starts_with("Graphics element ") && trimmed.ends_with(" does not exist.") {
        return "Graphics element does not exist.".to_owned();
    }
    if trimmed.starts_with("Deco from ") && trimmed.contains(" intersects deco from ") {
        return "Deco overlaps another deco.".to_owned();
    }

    match trimmed {
        "Block does not exist." => "Block does not exist.".to_owned(),
        "Block group name is required" => "Block group name is required".to_owned(),
        "Block group name must be unique" => "Block group name must be unique".to_owned(),
        "Block name is required" => "Block name is required".to_owned(),
        "A block named with that value already exists in that block group" => {
            "A block named with that value already exists in that block group".to_owned()
        }
        "Block duration must be between 5 minutes and 24 hours" => {
            "Block duration must be between 5 minutes and 24 hours".to_owned()
        }
        "[Collection] is required for collection type 'Collection'" => {
            "[Collection] is required for collection type 'Collection'".to_owned()
        }
        "[MediaItem] is required for collection type 'TelevisionShow'" => {
            "[MediaItem] is required for collection type 'TelevisionShow'".to_owned()
        }
        "[MediaItem] is required for collection type 'TelevisionSeason'" => {
            "[MediaItem] is required for collection type 'TelevisionSeason'".to_owned()
        }
        "[MultiCollection] is required for collection type 'MultiCollection'" => {
            "[MultiCollection] is required for collection type 'MultiCollection'".to_owned()
        }
        "[SmartCollection] is required for collection type 'SmartCollection'" => {
            "[SmartCollection] is required for collection type 'SmartCollection'".to_owned()
        }
        "[SearchQuery] is required for collection type 'SearchQuery'" => {
            "[SearchQuery] is required for collection type 'SearchQuery'".to_owned()
        }
        "[CollectionType] is invalid" => "[CollectionType] is invalid".to_owned(),
        "[BlockId] does not exist." => "[BlockId] does not exist.".to_owned(),
        "Template does not exist." => "Template does not exist.".to_owned(),
        "Template group name is required" => "Template group name is required".to_owned(),
        "Template group name must be 50 characters or fewer" => {
            "Template group name must be 50 characters or fewer".to_owned()
        }
        "Template group name must be unique" => "Template group name must be unique".to_owned(),
        "Template group does not exist." => "Template group does not exist.".to_owned(),
        "[TemplateId] does not exist." => "[TemplateId] does not exist.".to_owned(),
        "Template name is required" => "Template name is required".to_owned(),
        "Template items cannot cross midnight." => {
            "Template items cannot cross midnight.".to_owned()
        }
        "Deco group name is required" => "Deco group name is required".to_owned(),
        "Deco group name must be 50 characters or fewer" => {
            "Deco group name must be 50 characters or fewer".to_owned()
        }
        "Deco group name must be unique" => "Deco group name must be unique".to_owned(),
        "Deco group does not exist." => "Deco group does not exist.".to_owned(),
        "Deco name is required" => "Deco name is required".to_owned(),
        "Deco does not exist" => "Deco does not exist.".to_owned(),
        "Default filler has unsupported collection type." => {
            "Default filler has unsupported collection type.".to_owned()
        }
        "Dead air fallback has unsupported collection type." => {
            "Dead air fallback has unsupported collection type.".to_owned()
        }
        "Break content has unsupported collection type." => {
            "Break content has unsupported collection type.".to_owned()
        }
        "Default filler must have valid item" => "Default filler must have a valid item".to_owned(),
        "Dead air fallback must have valid item" => {
            "Dead air fallback must have a valid item".to_owned()
        }
        "Break content collection must have valid item" => {
            "Break content collection must have a valid item".to_owned()
        }
        "Break content multi collection must have valid item" => {
            "Break content multi-collection must have a valid item".to_owned()
        }
        "Break content smart collection must have valid item" => {
            "Break content smart collection must have a valid item".to_owned()
        }
        "Break content media item must have valid item" => {
            "Break content media item must have a valid item".to_owned()
        }
        "Break content playlist must have valid item" => {
            "Break content playlist must have a valid item".to_owned()
        }
        "Default filler item does not exist." => "Default filler item does not exist.".to_owned(),
        "Dead air fallback item does not exist." => {
            "Dead air fallback item does not exist.".to_owned()
        }
        "Break content collection item does not exist." => {
            "Break content collection item does not exist.".to_owned()
        }
        "Break content multi collection item does not exist." => {
            "Break content multi-collection item does not exist.".to_owned()
        }
        "Break content smart collection item does not exist." => {
            "Break content smart collection item does not exist.".to_owned()
        }
        "Break content media item item does not exist." => {
            "Break content media item does not exist.".to_owned()
        }
        "Break content playlist item does not exist." => {
            "Break content playlist does not exist.".to_owned()
        }
        "Deco may only contain one [Block Start] break content" => {
            "Deco may only contain one Block Start break content.".to_owned()
        }
        "Deco may only contain one [Between Block Items] break content" => {
            "Deco may only contain one Between Block Items break content.".to_owned()
        }
        "Deco may only contain one [At Chapter Markers] break content" => {
            "Deco may only contain one At Chapter Markers break content.".to_owned()
        }
        "Deco may only contain one [Block Finish] break content" => {
            "Deco may only contain one Block Finish break content.".to_owned()
        }
        "Deco template group name is required" => "Deco template group name is required".to_owned(),
        "Deco template group name must be 50 characters or fewer" => {
            "Deco template group name must be 50 characters or fewer".to_owned()
        }
        "Deco template group name must be unique" => {
            "Deco template group name must be unique".to_owned()
        }
        "Deco template group does not exist." => "Deco template group does not exist.".to_owned(),
        "[DecoTemplateId] does not exist." => "Deco template does not exist.".to_owned(),
        "Deco template name is required" => "Deco template name is required".to_owned(),
        "[DecoId] does not exist." => "Deco does not exist.".to_owned(),
        "Deco template items cannot be empty." => "Deco template items cannot be empty.".to_owned(),
        _ => error.to_owned(),
    }
}

fn watermark_error_message(error: &str) -> String {
    let trimmed = error.trim();
    if trimmed.starts_with("Watermark ") && trimmed.ends_with(" does not exist.") {
        return "Watermark not found.".to_owned();
    }

    match trimmed {
        "Name is required" => "Name is required".to_owned(),
        "Name must be 50 characters or fewer" => "Name must be 50 characters or fewer".to_owned(),
        "Watermark image is required" | "Watermark image is required!" => {
            "Watermark image is required".to_owned()
        }
        "Width must be between 0 and 100%" => "Width must be between 0 and 100%".to_owned(),
        "Horizontal margin must be between 0 and 50%" => {
            "Horizontal margin must be between 0 and 50%".to_owned()
        }
        "Vertical margin must be between 0 and 50%" => {
            "Vertical margin must be between 0 and 50%".to_owned()
        }
        "Duration must be positive and less than frequency window" => {
            "Duration must be positive and less than frequency window".to_owned()
        }
        "Opacity must be between 1 and 100" => "Opacity must be between 1 and 100".to_owned(),
        "Watermark name must be unique" | "ChannelWatermark name must be unique" => {
            "Watermark name must be unique".to_owned()
        }
        "Watermark not found" | "Source watermark not found" | "Watermark does not exist." => {
            "Watermark not found.".to_owned()
        }
        _ => error.to_owned(),
    }
}

fn ffmpeg_profile_hardware_acceleration_label(value: i32) -> String {
    match value {
        1 => "QSV",
        2 => "NVENC",
        3 => "VAAPI",
        4 => "VideoToolbox",
        5 => "AMF",
        6 => "V4L2M2M",
        7 => "RKMPP",
        _ => return format!("Unknown ({value})"),
    }
    .to_owned()
}

fn ffmpeg_profile_video_format_label(format: i32) -> String {
    match format {
        0 => "None",
        1 => "H264",
        2 => "H265 (HEVC)",
        3 => "MPEG-2",
        4 => "AV1",
        99 => text(TextKey::Copy),
        _ => return format!("Unknown ({format})"),
    }
    .to_owned()
}

fn ffmpeg_profile_audio_format_label(format: i32) -> String {
    match format {
        0 => "None",
        1 => "AAC",
        2 => "AC3",
        3 => "AAC (LATM)",
        99 => text(TextKey::Copy),
        _ => return format!("Unknown ({format})"),
    }
    .to_owned()
}

fn settings_error_message(error: &str) -> String {
    let trimmed = error.trim();
    let trimmed = trimmed
        .strip_prefix("update Windows startup registration is unavailable: ")
        .unwrap_or(trimmed);

    if let Some((current, limit)) = xmltv_days_error_parts(trimmed) {
        return format!(
            "XMLTV days to build ({current}) cannot be greater than Playout days to build ({limit})"
        );
    }
    if let Some(name) = trimmed.strip_suffix(" path does not exist") {
        return format!("{name} path does not exist");
    }
    if let Some(name) = trimmed
        .strip_prefix("Unable to verify ")
        .and_then(|value| value.strip_suffix(" version"))
    {
        return format!("Unable to verify {name} version");
    }
    if let Some(value) = trimmed.strip_prefix("Invalid option: ") {
        return format!("Invalid option: {value}");
    }
    if trimmed.starts_with("Resolution ") && trimmed.ends_with(" does not exist.") {
        return "Resolution does not exist.".to_owned();
    }

    match trimmed {
        "Windows startup registration is not supported on this platform" => {
            "Windows startup registration is not supported on this platform".to_owned()
        }
        "ErsatzRS.exe was not found next to ersatzrs-web.exe" => {
            "ErsatzRS.exe was not found next to ersatzrs-web.exe".to_owned()
        }
        "Unable to update Windows startup registration" => {
            "Unable to update Windows startup registration".to_owned()
        }
        "Tuner count must be greater than zero" => {
            "Tuner count must be greater than zero".to_owned()
        }
        "Library refresh interval must be 0 (do not refresh) or greater than zero" => {
            "Library refresh interval must be 0 (do not refresh) or greater than zero".to_owned()
        }
        "Library refresh interval must be less than 1,000,000. Use 0 to disable automatic refresh" => {
            "Library refresh interval must be less than 1,000,000. Use 0 to disable automatic refresh".to_owned()
        }
        "Playout days to build must be greater than zero" => {
            "Playout days to build must be greater than zero".to_owned()
        }
        "Scripted schedule timeout seconds must be at least 10" => {
            "Scripted schedule timeout seconds must be at least 10".to_owned()
        }
        "XMLTV days to build must be greater than zero" => {
            "XMLTV days to build must be greater than zero".to_owned()
        }
        "Resolution width or height is invalid" => {
            "Resolution width or height is invalid".to_owned()
        }
        "Resolution width and height must be unique" => {
            "Resolution width and height must be unique".to_owned()
        }
        "HLS Segmenter idle timeout must be greater than or equal to 30" => {
            "HLS Segmenter idle timeout must be greater than or equal to 30".to_owned()
        }
        "Work-Ahead HLS Segmenter limit must be greater than or equal to 0" => {
            "Work-Ahead HLS Segmenter limit must be greater than or equal to 0".to_owned()
        }
        "Active HLS transcode limit must be greater than or equal to 1" => {
            "Active HLS transcode limit must be greater than or equal to 1".to_owned()
        }
        "HLS Segmenter initial segment count must be greater than or equal to 1" => {
            "HLS Segmenter initial segment count must be greater than or equal to 1".to_owned()
        }
        _ => error.to_owned(),
    }
}

fn media_action_error_message(error: &str) -> String {
    match error.trim() {
        "Collection name is required" => "Collection name is required".to_owned(),
        "Collection name must be 50 characters or less" => {
            "Collection name must be 50 characters or less".to_owned()
        }
        "Collection name must be unique" => "Collection name must be unique".to_owned(),
        "Media item does not exist" => "Media item does not exist".to_owned(),
        "Collection does not exist" | "Collection does not exist." => {
            "Collection does not exist.".to_owned()
        }
        "Playlist does not exist" | "Playlist does not exist." => {
            "Playlist does not exist.".to_owned()
        }
        "Unable to locate media item" => "Unable to locate media item".to_owned(),
        "Media action failed" => "Media action failed".to_owned(),
        _ => error.to_owned(),
    }
}

fn filler_error_message(error: &str) -> String {
    let trimmed = error.trim();
    if trimmed.starts_with("FillerPreset ") && trimmed.ends_with(" does not exist.") {
        return "Filler preset does not exist.".to_owned();
    }

    match trimmed {
        "Filler preset does not exist" => "Filler preset does not exist.".to_owned(),
        "Filler preset name must be unique" => "Filler preset name must be unique".to_owned(),
        "Name must not be empty" => "Name must not be empty".to_owned(),
        "Name must be 50 characters or fewer" => "Name must be 50 characters or fewer".to_owned(),
        "Filler kind must not be None" => "Filler kind must not be None".to_owned(),
        "Filler mode must not be None" => "Filler mode must not be None".to_owned(),
        "Count must be greater than or equal to 0" => {
            "Count must be greater than or equal to 0".to_owned()
        }
        "Duration must not be empty" => "Duration must not be empty".to_owned(),
        "Pad To Nearest Minute must not be empty" => {
            "Pad To Nearest Minute must not be empty".to_owned()
        }
        "Use Chapters As Media Items must not be enabled for fallback filler" => {
            "Use Chapters As Media Items must not be enabled for fallback filler".to_owned()
        }
        "Collection must not be empty" => "Collection must not be empty".to_owned(),
        "Multi Collection must not be empty" => "Multi Collection must not be empty".to_owned(),
        "Smart Collection must not be empty" => "Smart Collection must not be empty".to_owned(),
        "Media item must not be empty" => "Media item must not be empty".to_owned(),
        _ => error.to_owned(),
    }
}

fn media_browser_error_message(error: &str) -> String {
    let trimmed = error.trim();
    if trimmed.ends_with("Collection does not contain the selected item.") {
        return "Collection does not contain the selected item.".to_owned();
    }
    if trimmed.contains("collection with id ") && trimmed.ends_with(" not found") {
        return "Collection does not exist.".to_owned();
    }
    if trimmed == "Pinned stream selection is invalid or no longer available." {
        return trimmed.to_owned();
    }
    if trimmed == "Movie was not found." {
        return trimmed.to_owned();
    }
    if trimmed.starts_with("Collection ") && trimmed.ends_with(" is already present.") {
        return "Collection is already present.".to_owned();
    }
    if trimmed.starts_with("Collection ") && trimmed.ends_with(" does not exist.") {
        return "Collection does not exist.".to_owned();
    }
    if trimmed.starts_with("Smart collection ") && trimmed.ends_with(" is already present.") {
        return "Smart collection is already present.".to_owned();
    }
    if trimmed.starts_with("Smart collection ") && trimmed.ends_with(" does not exist.") {
        return "Smart collection does not exist.".to_owned();
    }
    if trimmed.starts_with("MultiCollection ") && trimmed.ends_with(" does not exist.") {
        return "Multi-collection does not exist.".to_owned();
    }
    if trimmed.starts_with("Rerun collection ") && trimmed.ends_with(" does not exist.") {
        return "Rerun collection does not exist.".to_owned();
    }
    if trimmed.starts_with("Referenced item ") && trimmed.ends_with(" does not exist.") {
        return "Referenced item does not exist.".to_owned();
    }
    if trimmed.starts_with("PlaylistGroup ") && trimmed.ends_with(" does not exist.") {
        return "Playlist group does not exist.".to_owned();
    }
    if trimmed.starts_with("Playlist ") && trimmed.ends_with(" does not exist.") {
        return "Playlist does not exist.".to_owned();
    }
    if trimmed.starts_with("Playlist name \"") && trimmed.ends_with("\" is invalid") {
        return "Playlist name is invalid.".to_owned();
    }
    if let Some(error) = trimmed.strip_prefix("Playlist preview failed: ") {
        return format!("Playlist preview failed: {error}");
    }
    if trimmed.starts_with("Media item ") && trimmed.ends_with(" does not exist.") {
        return "Media item does not exist.".to_owned();
    }

    match trimmed {
        "Collection name is required" => "Collection name is required".to_owned(),
        "Collection name must be 50 characters or less" => {
            "Collection name must be 50 characters or less".to_owned()
        }
        "Collection name must be unique" => "Collection name must be unique".to_owned(),
        "Collection does not exist." => "Collection does not exist.".to_owned(),
        "Smart collection name is required" => "Smart collection name is required".to_owned(),
        "Smart collection query is required" => "Smart collection query is required".to_owned(),
        "SmartCollection name must be unique" => "Smart collection name must be unique".to_owned(),
        "MultiCollection does not exist." => "Multi-collection does not exist.".to_owned(),
        "Multi-collection name is required!" => "Multi-collection name is required.".to_owned(),
        "Multi-collection name cannot be longer than 50 characters." => {
            "Multi-collection name cannot be longer than 50 characters.".to_owned()
        }
        "MultiCollection name must be unique" => "Multi-collection name must be unique".to_owned(),
        "Each multi-collection item must reference exactly one collection." => {
            "Each multi-collection item must reference exactly one collection.".to_owned()
        }
        "Rerun collection does not exist." => "Rerun collection does not exist.".to_owned(),
        "[Name] is an empty string" | "[Name] must not be empty." => {
            "Name must not be empty.".to_owned()
        }
        "[Name] must not be longer than 50" | "[Name] must not be longer than 50." => {
            "Name must not be longer than 50 characters.".to_owned()
        }
        "Rerun collection name must be unique" => "Rerun collection name must be unique".to_owned(),
        "Unsupported collection type." => "Unsupported collection type.".to_owned(),
        "[ProgramScheduleId] does not exist." => "Schedule does not exist.".to_owned(),
        "[MediaItem] does not exist." => "Media item does not exist.".to_owned(),
        "[PlaylistGroupId] does not exist." => "Playlist group does not exist.".to_owned(),
        "Cannot delete system playlist group" => "Cannot delete system playlist group".to_owned(),
        "Cannot delete system (generated) playlist" => {
            "Cannot delete system (generated) playlist".to_owned()
        }
        "[PlaylistId] does not exist." => "Playlist does not exist.".to_owned(),
        "Cannot save system (generated) playlist" => {
            "Cannot save system (generated) playlist".to_owned()
        }
        "Playlist playout preview has no items" => {
            "Playlist playout preview has no items".to_owned()
        }
        "Playlist name is required" => "Playlist name is required".to_owned(),
        "[Collection] is required for collection type 'Collection'" => {
            "Collection is required.".to_owned()
        }
        "[MultiCollection] is required for collection type 'MultiCollection'" => {
            "Multi-collection is required.".to_owned()
        }
        "[SmartCollection] is required for collection type 'SmartCollection'" => {
            "Smart collection is required.".to_owned()
        }
        "[CollectionType] is invalid" => "Collection type is invalid.".to_owned(),
        _ => error.to_owned(),
    }
}

fn media_source_error_message(error: &str) -> String {
    let trimmed = error.trim();
    if let Some(error) = trimmed.strip_prefix("Could not reach Jellyfin: ") {
        return format!("Could not reach Jellyfin: {error}");
    }

    match trimmed {
        "Address is required." => "Address is required.".to_owned(),
        "API key is required." => "API key is required.".to_owned(),
        "'Address' must be a valid URL" => "'Address' must be a valid URL".to_owned(),
        _ => error.to_owned(),
    }
}

fn addon_operation_error_message(code: &str, detail: &str) -> String {
    let summary = match code {
        "missing-setting" => "Configure the required add-on setting.",
        "missing-secret" => "Configure the required secret reference in the add-on settings.",
        "provider-unreachable" => "The add-on could not reach its provider.",
        "provider-rejected" => "The provider rejected the add-on request.",
        "provider-rate-limited" => "The provider temporarily rate-limited the add-on.",
        "not-found" => "The requested add-on item was not found.",
        "unsupported-url" => "The add-on does not support this URL.",
        _ => "The add-on reported a failure.",
    };
    if detail.trim().is_empty() {
        summary.to_owned()
    } else {
        format!("{summary} {detail}")
    }
}

fn addon_media_list_error_message(error: &str) -> String {
    let trimmed = error.trim();
    let normalized = trimmed.strip_prefix("conflict: ").unwrap_or(trimmed);
    if let Some(error) = normalized.strip_prefix("failed to decode media-list add-on output: ") {
        return format!("Add-on returned invalid list data: {error}");
    }
    if normalized.starts_with("add-on media list with id ") && normalized.ends_with(" not found") {
        return "add-on list does not exist.".to_owned();
    }

    if let Some(folder) =
        normalized.strip_prefix("another list of this add-on already uses the folder ")
    {
        return format!("Another list of this add-on already uses the folder \"{folder}\".");
    }
    if let Some(folder) = normalized.strip_suffix(" is not usable as a folder name") {
        return format!("\"{folder}\" is not usable as a folder name.");
    }

    match normalized {
        "Invalid add-on list url" => "Invalid add-on list URL.".to_owned(),
        "add-on API client is not configured for this operation" => {
            "add-on API client is not configured for this operation.".to_owned()
        }
        _ => error.to_owned(),
    }
}

fn upload_error_message(error: &str) -> String {
    let trimmed = error.trim();
    if let Some(error) = trimmed.strip_prefix("Cannot create upload directory: ") {
        return format!("Cannot create upload directory: {error}");
    }
    if let Some(error) = trimmed.strip_prefix("Write failed: ") {
        return format!("Write failed: {error}");
    }
    if let Some(error) = trimmed.strip_prefix("Read field: ") {
        return format!("Read field: {error}");
    }

    match trimmed {
        "Upload exceeds maximum allowed size" => "Upload exceeds maximum allowed size.".to_owned(),
        "No file field" => "No file field found.".to_owned(),
        _ => error.to_owned(),
    }
}

fn xmltv_days_error_parts(error: &str) -> Option<(&str, &str)> {
    let rest = error.strip_prefix("XMLTV days to build (")?;
    let (current, rest) = rest.split_once(") cannot be greater than Playout days to build (")?;
    let limit = rest.strip_suffix(')')?;
    Some((current, limit))
}

fn ffmpeg_profile_error_message(error: &str) -> String {
    let trimmed = error.trim();
    if trimmed.starts_with("An FFmpeg profile named \"") && trimmed.ends_with("\" already exists.")
    {
        return trimmed.to_owned();
    }

    match trimmed {
        "Profile name is required." => "Profile name is required.".to_owned(),
        "Profile name must be 50 characters or fewer." => {
            "Profile name must be 50 characters or fewer.".to_owned()
        }
        "Thread count must be 0 or greater." => "Thread count must be 0 or greater.".to_owned(),
        "QSV supports formats (h264, hevc, mpeg2video)" => {
            "QSV supports formats (h264, hevc, mpeg2video).".to_owned()
        }
        "NVENC supports formats (h264, hevc, av1)" => {
            "NVENC supports formats (h264, hevc, av1).".to_owned()
        }
        "VAAPI supports formats (h264, hevc, av1, mpeg2video)" => {
            "VAAPI supports formats (h264, hevc, av1, mpeg2video).".to_owned()
        }
        "VideoToolbox supports formats (h264, hevc)" => {
            "VideoToolbox supports formats (h264, hevc).".to_owned()
        }
        "Amf supports formats (h264, hevc)" => "Amf supports formats (h264, hevc).".to_owned(),
        "V4L2 M2M supports formats (h264, hevc)" => {
            "V4L2 M2M supports formats (h264, hevc).".to_owned()
        }
        "Rkmpp supports formats (h264, hevc)" => "Rkmpp supports formats (h264, hevc).".to_owned(),
        "HDR10 output requires hevc or av1; h264 and mpeg2video cannot carry HDR10 static metadata" => {
            "HDR10 output requires hevc or av1; h264 and mpeg2video cannot carry HDR10 static metadata.".to_owned()
        }
        "HDR10 output requires 10-bit; 8-bit cannot represent a PQ signal without banding" => {
            "HDR10 output requires 10-bit; 8-bit cannot represent a PQ signal without banding.".to_owned()
        }
        "Mastering display must look like G(8500,39850)B(6550,2300)R(35400,14600)WP(15635,16450)L(10000000,1), with chromaticities up to 50000 and max luminance above min" => {
            "Mastering display must look like G(8500,39850)B(6550,2300)R(35400,14600)WP(15635,16450)L(10000000,1), with chromaticities up to 50000 and max luminance above min.".to_owned()
        }
        "MaxCLL and MaxFALL must both be set (or both left empty), at most 65535, with MaxFALL no greater than MaxCLL" => {
            "MaxCLL and MaxFALL must both be set (or both left empty), at most 65535, with MaxFALL no greater than MaxCLL.".to_owned()
        }
        "Mpeg2Video does not support 10-bit content" => {
            "Mpeg2Video does not support 10-bit content.".to_owned()
        }
        "VideoProfile must be high10 with 10-bit h264" => {
            "Video Profile must be high10 with 10-bit h264.".to_owned()
        }
        "VideoProfile must be high444p with NVIDIA 10-bit h264" => {
            "Video Profile must be high444p with NVIDIA 10-bit h264.".to_owned()
        }
        "VideoProfile cannot be high10 with 8-bit h264" => {
            "Video Profile cannot be high10 with 8-bit h264.".to_owned()
        }
        "Source profile not found." => "Source profile not found.".to_owned(),
        "No FFmpeg resolutions are available; database bootstrap is incomplete" => {
            "No FFmpeg resolutions are available; database bootstrap is incomplete".to_owned()
        }
        _ => error.to_owned(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::admin::i18n::{
        addon_media_list_error_text, episode_display_subtitle_text, ffmpeg_profile_error_text,
        ffmpeg_profile_video_format_text, ffmpeg_tonemap_algorithm_text, filler_error_text,
        hardware_acceleration_option_text, log_level_text, media_action_error_text,
        media_browser_error_text, media_kind_text, media_source_error_text,
        playout_schedule_kind_text, schedule_error_text, scheduling_content_error_text,
        settings_error_text, upload_error_text, watermark_error_text, UiLanguage,
    };

    #[test]
    fn season_display_names() {
        assert_eq!(STRINGS.season_display_name(0), "Specials");
        assert_eq!(STRINGS.season_display_name(2), "Season 2");
    }

    #[test]
    fn episode_display_subtitles() {
        assert_eq!(
            episode_display_subtitle_text(UiLanguage::English, Some("Show"), Some(2), Some(5))
                .as_deref(),
            Some("Show - S02E05")
        );
        assert_eq!(
            episode_display_subtitle_text(UiLanguage::English, None, None, None).as_deref(),
            Some("Unnumbered")
        );
    }

    #[test]
    fn fallback_item_labels() {
        assert_eq!(STRINGS.media_item_fallback_label(42), "Media Item #42");
    }

    #[test]
    fn log_level_labels() {
        assert_eq!(
            log_level_text(UiLanguage::English, "Information"),
            "Information"
        );
    }

    #[test]
    fn channel_labels() {
        assert_eq!(STRINGS.channel_streaming_mode_label(1), "MPEG-TS (Legacy)");
    }

    #[test]
    fn media_kind_labels() {
        assert_eq!(media_kind_text(UiLanguage::English, 3), "Music Videos");
    }

    #[test]
    fn playout_schedule_kind_labels() {
        assert_eq!(
            playout_schedule_kind_text(UiLanguage::English, 3),
            "Sequential"
        );
    }

    #[test]
    fn hardware_acceleration_labels() {
        assert_eq!(
            hardware_acceleration_option_text(UiLanguage::English, "NVENC", false),
            "NVENC (not available on this system)"
        );
    }

    #[test]
    fn ffmpeg_option_labels() {
        assert_eq!(
            ffmpeg_tonemap_algorithm_text(UiLanguage::English, 1),
            "Clip"
        );
        assert_eq!(
            ffmpeg_profile_video_format_text(UiLanguage::English, 1, 2),
            "H264 / NVENC"
        );
    }

    #[test]
    fn deco_template_action_labels() {
        assert_eq!(
            STRINGS.text(TextKey::AddDecoToTemplate),
            "Add Deco To Template"
        );
    }

    #[test]
    fn server_errors_pass_through_unchanged() {
        assert_eq!(
            ffmpeg_profile_error_text(UiLanguage::English, "Profile name is required."),
            "Profile name is required."
        );
        assert_eq!(
            schedule_error_text(UiLanguage::English, "Schedule name is required!"),
            "Schedule name is required!"
        );
        assert_eq!(
            scheduling_content_error_text(UiLanguage::English, "Block name is required"),
            "Block name is required"
        );
        assert_eq!(
            scheduling_content_error_text(
                UiLanguage::English,
                "A deco named \"example\" already exists in that deco group"
            ),
            "A deco with that name already exists in that deco group."
        );
        assert_eq!(
            watermark_error_text(UiLanguage::English, "Watermark image is required"),
            "Watermark image is required"
        );
        assert_eq!(
            settings_error_text(
                UiLanguage::English,
                "HLS Segmenter idle timeout must be greater than or equal to 30"
            ),
            "HLS Segmenter idle timeout must be greater than or equal to 30"
        );
        assert_eq!(
            media_action_error_text(UiLanguage::English, "Collection name must be unique"),
            "Collection name must be unique"
        );
        assert_eq!(
            filler_error_text(UiLanguage::English, "Filler preset name must be unique"),
            "Filler preset name must be unique"
        );
        assert_eq!(
            media_browser_error_text(UiLanguage::English, "MultiCollection 7 does not exist."),
            "Multi-collection does not exist."
        );
        assert_eq!(
            media_source_error_text(UiLanguage::English, "Address is required."),
            "Address is required."
        );
        assert_eq!(
            addon_media_list_error_text(UiLanguage::English, "Invalid add-on list url"),
            "Invalid add-on list URL."
        );
        // Server functions wrap the repository message in transport plumbing.
        assert_eq!(
            addon_media_list_error_text(
                UiLanguage::English,
                "error running server function: conflict: another list of this add-on already uses the folder teleac"
            ),
            "Another list of this add-on already uses the folder \"teleac\"."
        );
        assert_eq!(
            addon_media_list_error_text(
                UiLanguage::English,
                "conflict: /// is not usable as a folder name"
            ),
            "\"///\" is not usable as a folder name."
        );
        assert_eq!(
            upload_error_text(UiLanguage::English, "Upload exceeds maximum allowed size"),
            "Upload exceeds maximum allowed size."
        );
    }

    #[test]
    fn movie_not_found_error_is_localized_for_every_culture() {
        let error = "Movie was not found.";
        assert_eq!(
            media_browser_error_text(UiLanguage::English, error),
            "Movie was not found."
        );
        assert_eq!(
            media_browser_error_text(UiLanguage::Dutch, error),
            "Film is niet gevonden."
        );
        assert_eq!(
            media_browser_error_text(UiLanguage::French, error),
            "Le film est introuvable."
        );
        assert_eq!(
            media_browser_error_text(UiLanguage::Polish, error),
            "Nie znaleziono filmu."
        );
        assert_eq!(
            media_browser_error_text(UiLanguage::PortugueseBrazil, error),
            "O filme não foi encontrado."
        );
    }
}
