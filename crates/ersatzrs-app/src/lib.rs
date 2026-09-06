//! Storage- and transport-neutral application services.
//!
//! Commands and queries enter through this crate, which coordinates core
//! ports without depending on SQL, HTTP, scheduler implementations, `FFmpeg`,
//! or platform adapters. The native web host constructs [`ApplicationServices`]
//! from concrete infrastructure ports.
#![allow(clippy::missing_errors_doc)]

pub mod addon_media_list_options;
pub mod commands;
pub mod error;
pub mod operations;
pub mod ports;
pub mod queries;

pub use error::{ApplicationError, Result};
pub use ersatzrs_domain::{ScanPlayoutRefresh, ScanPlayoutRefreshReason};
pub use operations::OperationCoordinator;
pub use ports::{
    AddonIconRead, AddonItemAccessRead, AddonItemAccessState, AddonOverviewRead,
    AddonRepositoryRead, AddonRuntime, AddonRuntimeError, AddonSettingEdit, AddonSettingRead,
    AdminFfmpegRuntimeRead, AdminHealthRead, AdminHealthRuntime, AdminHealthRuntimeError,
    AdminTroubleshootingRuntime, AdminTroubleshootingRuntimeError, ApplicationPorts,
    AvailableAddonRead, BlockPreviewBuilder, ChannelRuntime, ChannelRuntimeError,
    InstalledAddonRead, MediaSourceRuntime, MediaSourceRuntimeError, PlaybackPreviewRequest,
    PlayoutBuildQueue, PlayoutBuildQueueError, PlexPinFlow, RemoteServerInfo,
    RemoteStreamPlaylistEdit, RemoteStreamPlaylistEntryRead, RemoteStreamPlaylistRead,
    RemoteStreamPlaylistRuntime, RemoteStreamPlaylistRuntimeError,
    RemoteStreamPlaylistTroubleshootingRead, ScanRuntime, ScanRuntimeError, ScanRuntimeReport,
    ScanWorkQueue, ScanWorkQueueError, SettingsFfmpegPaths, SettingsRuntime, SettingsRuntimeError,
    SourceAddonRouting, WindowsStartupStatus,
};

/// Native application services assembled by a composition root.
///
/// The port set is intentionally limited to the playout and library
/// repositories required by the immediately following Phase 7B vertical
/// slices. Additional ports belong beside the use case that first needs them.
#[derive(Clone)]
pub struct ApplicationServices {
    ports: ApplicationPorts,
    operations: OperationCoordinator,
}

impl ApplicationServices {
    /// Construct the application container from injected core ports and the
    /// process-wide operation coordinator.
    #[must_use]
    pub fn new(ports: ApplicationPorts, operations: OperationCoordinator) -> Self {
        Self { ports, operations }
    }

    /// Core repository ports used by application use cases.
    #[must_use]
    pub const fn ports(&self) -> &ApplicationPorts {
        &self.ports
    }

    /// Shared process-local coordination for long-running operations.
    #[must_use]
    pub const fn operations(&self) -> &OperationCoordinator {
        &self.operations
    }

    pub async fn list_remote_stream_playlists(
        &self,
        library_id: i64,
    ) -> std::result::Result<Vec<RemoteStreamPlaylistRead>, RemoteStreamPlaylistRuntimeError> {
        self.ports.remote_stream_playlists().list(library_id).await
    }

    pub async fn list_remote_stream_playlist_addons(
        &self,
    ) -> std::result::Result<Vec<String>, RemoteStreamPlaylistRuntimeError> {
        self.ports.remote_stream_playlists().addon_ids().await
    }

    pub async fn list_remote_stream_tags(
        &self,
    ) -> std::result::Result<Vec<String>, RemoteStreamPlaylistRuntimeError> {
        self.ports.remote_stream_playlists().tags().await
    }

    pub async fn addon_overview(
        &self,
    ) -> std::result::Result<AddonOverviewRead, AddonRuntimeError> {
        self.ports.addons().overview().await
    }

    pub async fn addon_installed_icon(
        &self,
        addon_id: String,
    ) -> std::result::Result<Option<AddonIconRead>, AddonRuntimeError> {
        self.ports.addons().installed_icon(addon_id).await
    }

    pub async fn addon_source_icon(
        &self,
        source_id: String,
        addon_id: String,
    ) -> std::result::Result<Option<AddonIconRead>, AddonRuntimeError> {
        self.ports.addons().source_icon(source_id, addon_id).await
    }

    pub async fn scan_addon_repositories(&self) -> std::result::Result<(), AddonRuntimeError> {
        self.ports.addons().scan_repositories().await
    }

    pub async fn remove_addon_repository(
        &self,
        repository_id: String,
    ) -> std::result::Result<(), AddonRuntimeError> {
        self.ports.addons().remove_repository(repository_id).await
    }

    pub async fn install_addon(
        &self,
        repository_id: String,
        addon_id: String,
        adopt_legacy: bool,
    ) -> std::result::Result<(), AddonRuntimeError> {
        self.ports
            .addons()
            .install(repository_id, addon_id, adopt_legacy)
            .await
    }

    pub async fn set_unverified_addon_sources(
        &self,
        enabled: bool,
    ) -> std::result::Result<(), AddonRuntimeError> {
        self.ports.addons().set_unverified_sources(enabled).await
    }

    /// Select the add-on that should process one operator-supplied source URL.
    pub async fn resolve_source_addon(
        &self,
        url: String,
        required_capabilities: Vec<String>,
    ) -> std::result::Result<SourceAddonRouting, AddonRuntimeError> {
        self.ports
            .addons()
            .resolve_source_addon(url, required_capabilities)
            .await
    }

    /// The add-on used for source URLs no installed package claims.
    pub async fn default_remote_stream_addon(
        &self,
    ) -> std::result::Result<Option<String>, AddonRuntimeError> {
        self.ports.addons().default_remote_stream_addon().await
    }

    /// Persist or clear the default remote-stream add-on.
    pub async fn set_default_remote_stream_addon(
        &self,
        addon_id: Option<String>,
    ) -> std::result::Result<(), AddonRuntimeError> {
        self.ports
            .addons()
            .set_default_remote_stream_addon(addon_id)
            .await
    }

    pub async fn install_addon_from_package(
        &self,
        archive: Vec<u8>,
    ) -> std::result::Result<(), AddonRuntimeError> {
        self.ports.addons().install_from_package(archive).await
    }

    pub async fn install_addon_repository_bundle(
        &self,
        archive: Vec<u8>,
    ) -> std::result::Result<(), AddonRuntimeError> {
        self.ports.addons().install_repository_bundle(archive).await
    }

    pub async fn save_addon_setting(
        &self,
        setting: AddonSettingEdit,
    ) -> std::result::Result<(), AddonRuntimeError> {
        self.ports.addons().save_setting(setting).await
    }

    pub async fn check_addon(
        &self,
        addon_id: String,
    ) -> std::result::Result<String, AddonRuntimeError> {
        self.ports.addons().check(addon_id).await
    }

    pub async fn enable_addon(
        &self,
        addon_id: String,
    ) -> std::result::Result<String, AddonRuntimeError> {
        self.ports.addons().enable(addon_id).await
    }

    pub async fn disable_addon(
        &self,
        addon_id: String,
    ) -> std::result::Result<(), AddonRuntimeError> {
        self.ports.addons().disable(addon_id).await
    }

    pub async fn uninstall_addon(
        &self,
        addon_id: String,
        force: bool,
    ) -> std::result::Result<(), AddonRuntimeError> {
        self.ports.addons().uninstall(addon_id, force).await
    }

    pub async fn save_remote_stream_playlist(
        &self,
        editor: RemoteStreamPlaylistEdit,
    ) -> std::result::Result<String, RemoteStreamPlaylistRuntimeError> {
        self.ports.remote_stream_playlists().save(editor).await
    }

    pub async fn delete_remote_stream_playlist(
        &self,
        library_id: i64,
        root_index: usize,
        relative_path: String,
    ) -> std::result::Result<(), RemoteStreamPlaylistRuntimeError> {
        self.ports
            .remote_stream_playlists()
            .delete(library_id, root_index, relative_path)
            .await
    }

    pub async fn exclude_remote_stream_playlist_entry(
        &self,
        library_id: i64,
        root_index: usize,
        relative_path: String,
        entry_id: String,
    ) -> std::result::Result<(), RemoteStreamPlaylistRuntimeError> {
        self.ports
            .remote_stream_playlists()
            .exclude_entry(library_id, root_index, relative_path, entry_id)
            .await
    }

    pub async fn troubleshoot_remote_stream_playlist(
        &self,
        library_id: i64,
        root_index: usize,
        relative_path: String,
    ) -> std::result::Result<
        RemoteStreamPlaylistTroubleshootingRead,
        RemoteStreamPlaylistRuntimeError,
    > {
        self.ports
            .remote_stream_playlists()
            .troubleshoot(library_id, root_index, relative_path)
            .await
    }

    /// Load the complete admin season-detail projection.
    pub async fn get_season_detail(
        &self,
        id: ersatzrs_domain::EntityId,
    ) -> Result<Option<queries::SeasonDetailReadModel>> {
        queries::get_season_detail(id, self.ports.television(), self.ports.admin_media()).await
    }

    /// Page through channel admin rows.
    pub async fn list_channels(
        &self,
        query: queries::ListChannels,
    ) -> Result<Vec<ersatzrs_core::repositories::ChannelAdminListRow>> {
        queries::list_channels(query, self.ports.channels()).await
    }

    /// Count channel admin rows.
    pub async fn count_channels(&self, show_disabled: bool) -> Result<u64> {
        queries::count_channels(show_disabled, self.ports.channels()).await
    }

    /// Load channel-number editor rows.
    pub async fn list_channel_numbers(
        &self,
    ) -> Result<Vec<ersatzrs_core::repositories::ChannelNumberRow>> {
        queries::list_channel_numbers(self.ports.channels()).await
    }

    /// Load a complete channel create/edit state.
    pub async fn get_channel_editor(
        &self,
        query: queries::GetChannelEditor,
    ) -> Result<Option<ersatzrs_core::repositories::ChannelEditorState>> {
        queries::get_channel_editor(query, self.ports.channels()).await
    }

    /// Load the bounded channel-watch projection used by admin preview surfaces.
    pub async fn get_channel_watch(
        &self,
        query: queries::GetChannelWatch,
    ) -> Result<Option<ersatzrs_core::repositories::ChannelWatchState>> {
        queries::get_channel_watch(query, self.ports.channels()).await
    }

    /// Load the aggregate admin EPG preview.
    pub async fn get_epg_preview(
        &self,
        query: queries::GetEpgPreview,
    ) -> Result<queries::EpgPreviewData> {
        queries::get_epg_preview(query, self.ports.channels(), self.ports.channel_runtime()).await
    }

    /// Save a channel and preserve its asynchronous subtitle side effect.
    pub async fn save_channel(&self, command: commands::SaveChannel) -> Result<i64> {
        commands::save_channel(command, self.ports.channels(), self.ports.channel_runtime()).await
    }

    /// Atomically update submitted channel numbers.
    pub async fn save_channel_numbers(
        &self,
        command: commands::SaveChannelNumbers,
    ) -> Result<Vec<i64>> {
        commands::save_channel_numbers(command, self.ports.channels()).await
    }

    /// Delete one channel.
    pub async fn delete_channel(&self, command: commands::DeleteChannel) -> Result<()> {
        commands::delete_channel(command, self.ports.channels()).await
    }

    /// Load `FFmpeg` profile options used by channel transports.
    pub async fn list_channel_ffmpeg_profiles(
        &self,
    ) -> Result<Vec<ersatzrs_core::repositories::ChannelSelectOption>> {
        queries::list_channel_ffmpeg_profiles(self.ports.channels()).await
    }

    /// Load filler preset options used by channel transports.
    pub async fn list_channel_filler_presets(
        &self,
    ) -> Result<Vec<ersatzrs_core::repositories::ChannelSelectOption>> {
        queries::list_channel_filler_presets(self.ports.channels()).await
    }

    /// List active channel streaming sessions.
    pub async fn list_active_channel_numbers(&self) -> Result<Vec<String>> {
        queries::list_active_channel_numbers(self.ports.channel_runtime()).await
    }

    /// Stop one active channel streaming session.
    pub async fn stop_channel_session(&self, number: &str) -> Result<bool> {
        queries::stop_channel_session(number, self.ports.channel_runtime()).await
    }

    /// List add-on media-list admin rows and persist the chosen page size.
    pub async fn list_addon_media_lists(
        &self,
        query: queries::ListAddonMediaLists,
    ) -> Result<(
        Vec<ersatzrs_core::repositories::AddonMediaListAdminRow>,
        u64,
    )> {
        queries::list_addon_media_lists(query, self.ports.addon_media_lists()).await
    }

    /// Load one Add-on media list editor state.
    pub async fn get_addon_media_list(
        &self,
        id: i64,
    ) -> Result<Option<ersatzrs_core::repositories::AddonMediaListAdminForm>> {
        queries::get_addon_media_list(id, self.ports.addon_media_lists()).await
    }

    /// Load the ordered provider items and replacement state.
    pub async fn list_addon_media_list_items(
        &self,
        id: i64,
    ) -> Result<Vec<ersatzrs_core::repositories::AddonMediaListItemAdminRow>> {
        queries::list_addon_media_list_items(id, self.ports.addon_media_lists()).await
    }

    /// Load blockers, recommendations, detail progress, and the saved revision
    /// used by the add-on-list publication command.
    pub async fn addon_media_list_publication_readiness(
        &self,
        id: i64,
    ) -> Result<ersatzrs_core::repositories::AddonMediaListPublicationReadiness> {
        queries::addon_media_list_publication_readiness(
            id,
            self.ports.addon_media_lists(),
            self.ports.managed_library_operations(),
        )
        .await
    }

    /// Load retained redacted causal history for one add-on list.
    pub async fn addon_media_list_publication_history(
        &self,
        id: i64,
    ) -> Result<ersatzrs_core::repositories::AddonMediaListPublicationHistory> {
        queries::addon_media_list_publication_history(id, self.ports.managed_library_operations())
            .await
    }

    pub async fn get_addon_media_list_item(
        &self,
        list_id: i64,
        item_id: i64,
    ) -> Result<Option<ersatzrs_core::repositories::AddonMediaListItemDetail>> {
        queries::get_addon_media_list_item(list_id, item_id, self.ports.addon_media_lists()).await
    }

    pub async fn get_addon_media_list_show_nfo(
        &self,
        id: i64,
    ) -> Result<ersatzrs_core::repositories::AddonMediaListNfoForm> {
        queries::get_addon_media_list_show_nfo(id, self.ports.addon_media_lists()).await
    }

    /// Load one database-backed artwork upload for an editor preview.
    pub async fn get_addon_media_list_artwork_upload(
        &self,
        upload_id: i64,
    ) -> Result<Option<ersatzrs_core::repositories::AddonMediaListArtworkUpload>> {
        queries::get_addon_media_list_artwork_upload(upload_id, self.ports.addon_media_lists())
            .await
    }

    /// Apply bounded retention to completed publication event streams.
    pub async fn prune_addon_media_list_publication_history(&self) -> Result<u64> {
        Ok(self
            .ports
            .managed_library_operations()
            .prune_publication_history(chrono::Utc::now())
            .await?)
    }

    /// Apply bounded retention to complete, unpinned publication revisions.
    pub async fn prune_addon_media_list_revisions(&self) -> Result<u64> {
        Ok(self
            .ports
            .addon_media_list_lifecycle()
            .prune_all_unpinned_revisions(chrono::Utc::now())
            .await?)
    }

    /// Compare the saved draft with the exact current publication snapshot.
    pub async fn addon_media_list_saved_draft_diff(
        &self,
        id: i64,
    ) -> Result<ersatzrs_core::repositories::AddonMediaListDraftDiff> {
        queries::addon_media_list_saved_draft_diff(id, self.ports.addon_media_list_lifecycle())
            .await
    }

    /// Load the destructive-reset dependency counts shown to the operator.
    pub async fn addon_media_list_reset_impact(
        &self,
        id: i64,
    ) -> Result<ersatzrs_core::repositories::AddonMediaListResetImpact> {
        queries::addon_media_list_reset_impact(id, self.ports.addon_media_list_lifecycle()).await
    }

    /// Load bounded publication revision history.
    pub async fn addon_media_list_revision_history(
        &self,
        id: i64,
    ) -> Result<Vec<ersatzrs_core::repositories::AddonMediaListRevisionSummary>> {
        queries::addon_media_list_revision_history(id, self.ports.addon_media_list_lifecycle())
            .await
    }

    /// Compare two exact retained publication revisions.
    pub async fn compare_addon_media_list_revisions(
        &self,
        id: i64,
        left_revision: i32,
        right_revision: i32,
    ) -> Result<ersatzrs_core::repositories::AddonMediaListRevisionDiff> {
        queries::compare_addon_media_list_revisions(
            id,
            left_revision,
            right_revision,
            self.ports.addon_media_list_lifecycle(),
        )
        .await
    }

    /// Search available local video versions for a replacement.
    pub async fn search_addon_media_list_replacements(
        &self,
        query: &str,
    ) -> Result<Vec<ersatzrs_core::repositories::AddonMediaListReplacementCandidate>> {
        queries::search_addon_media_list_replacements(query, self.ports.addon_media_lists()).await
    }

    /// Load the configured add-on media-list page size.
    pub async fn get_addon_media_lists_page_size(&self) -> Result<u32> {
        queries::get_addon_media_lists_page_size(self.ports.addon_media_lists()).await
    }

    /// Import or refresh one Add-on media list.
    pub async fn add_addon_media_list(&self, command: commands::AddAddonMediaList) -> Result<i64> {
        commands::add_addon_media_list(command, self.ports.addon_media_lists()).await
    }

    /// Load the managed Local library created for one Add-on media list.
    pub async fn addon_media_list_library_id(&self, id: i64) -> Result<Option<i64>> {
        Ok(self
            .ports
            .addon_media_lists()
            .managed_library_id(id)
            .await?)
    }

    /// Re-run local matching for one Add-on media list.
    pub async fn match_addon_media_list(
        &self,
        command: commands::MatchAddonMediaList,
    ) -> Result<()> {
        commands::match_addon_media_list(command, self.ports.addon_media_lists()).await
    }

    /// Queue a durable, non-mutating source header and first-page probe.
    pub async fn test_addon_media_list_source(
        &self,
        command: commands::TestAddonMediaListSource,
    ) -> Result<i64> {
        commands::test_addon_media_list_source(command, self.ports.addon_media_list_source_probes())
            .await
    }

    /// Load the latest source probe with its current saved-input freshness verdict.
    pub async fn addon_media_list_source_probe(
        &self,
        id: i64,
    ) -> Result<Option<ersatzrs_core::repositories::AddonMediaListSourceProbe>> {
        if id <= 0 {
            return Err(ApplicationError::Validation {
                field: "addon_media_list_id",
                message: "must be greater than zero".to_owned(),
            });
        }
        Ok(self
            .ports
            .addon_media_list_source_probes()
            .source_probe_status(id)
            .await?)
    }

    /// Publish one reviewed add-on media-list snapshot.
    pub async fn publish_addon_media_list(
        &self,
        command: commands::PublishAddonMediaList,
    ) -> Result<commands::PublishAddonMediaListOutcome> {
        commands::publish_addon_media_list(
            command,
            self.ports.addon_media_lists(),
            self.ports.managed_library_operations(),
        )
        .await
    }

    /// Restore one retained complete revision through the normal publication lease.
    pub async fn restore_addon_media_list_revision(
        &self,
        command: commands::RestoreAddonMediaListRevision,
    ) -> Result<commands::RestoreAddonMediaListRevisionOutcome> {
        commands::restore_addon_media_list_revision(
            command,
            self.ports.addon_media_lists(),
            self.ports.managed_library_operations(),
        )
        .await
    }

    /// Atomically enqueue or coalesce a destructive add-on-list reset/reload.
    pub async fn reset_addon_media_list(
        &self,
        command: commands::ResetAddonMediaList,
    ) -> Result<commands::ResetAddonMediaListOutcome> {
        commands::reset_addon_media_list(command, self.ports.managed_library_operations()).await
    }

    /// Atomically enqueue or coalesce a non-destructive full-source reconcile.
    pub async fn reconcile_addon_media_list(
        &self,
        command: commands::ReconcileAddonMediaList,
    ) -> Result<commands::ReconcileAddonMediaListOutcome> {
        commands::reconcile_addon_media_list(command, self.ports.managed_library_operations()).await
    }

    /// Confirm one first-observation reconcile absence as a removal candidate.
    pub async fn confirm_addon_media_list_removal(
        &self,
        command: commands::ConfirmAddonMediaListRemoval,
    ) -> Result<()> {
        commands::confirm_addon_media_list_removal(
            command,
            self.ports.addon_media_lists(),
            self.ports.addon_media_list_lifecycle(),
        )
        .await
    }

    /// Retry or restore one exact add-on media-list publication generation.
    pub async fn recover_addon_media_list_publication(
        &self,
        command: commands::RecoverAddonMediaListPublication,
    ) -> Result<ersatzrs_core::repositories::AddonMediaListPublicationStatus> {
        commands::recover_addon_media_list_publication(
            command,
            self.ports.managed_library_operations(),
        )
        .await
    }

    /// Include or exclude one provider item from publication.
    pub async fn set_addon_media_list_item_included(
        &self,
        command: commands::SetAddonMediaListItemIncluded,
    ) -> Result<()> {
        commands::set_addon_media_list_item_included(command, self.ports.addon_media_lists()).await
    }

    /// Set the primary/extra role of one source for the next publication.
    pub async fn set_addon_media_list_item_role(
        &self,
        command: commands::SetAddonMediaListItemRole,
    ) -> Result<()> {
        commands::set_addon_media_list_item_role(command, self.ports.addon_media_lists()).await
    }

    /// Save user-editable Add-on media list settings.
    pub async fn update_addon_media_list(
        &self,
        command: commands::UpdateAddonMediaList,
    ) -> Result<()> {
        commands::update_addon_media_list(command, self.ports.addon_media_lists()).await
    }

    /// Queue one item for deferred provider detail enrichment.
    pub async fn request_addon_media_list_item_details(
        &self,
        command: commands::RequestAddonMediaListItemDetails,
    ) -> Result<()> {
        commands::request_addon_media_list_item_details(command, self.ports.addon_media_lists())
            .await
    }

    /// Pause, resume, retry, or cancel the current overview import.
    pub async fn control_addon_media_list_import(
        &self,
        command: commands::ControlAddonMediaListImport,
    ) -> Result<()> {
        commands::control_addon_media_list_import(command, self.ports.addon_media_lists()).await
    }

    /// Load one page of provider items in one availability group.
    pub async fn list_addon_media_list_items_page(
        &self,
        id: ersatzrs_domain::EntityId,
        availability: ersatzrs_domain::AddonMediaListItemAvailability,
        page: u32,
        page_size: Option<u32>,
    ) -> Result<Vec<ersatzrs_core::repositories::AddonMediaListItemAdminRow>> {
        queries::list_addon_media_list_items_page(
            id,
            availability,
            page,
            page_size,
            self.ports.addon_media_lists(),
        )
        .await
    }

    /// Count the provider items of one list per availability group.
    pub async fn addon_media_list_item_counts(
        &self,
        id: ersatzrs_domain::EntityId,
    ) -> Result<ersatzrs_core::repositories::AddonMediaListItemCounts> {
        queries::addon_media_list_item_counts(id, self.ports.addon_media_lists()).await
    }

    /// Set the descriptive classification of one entry.
    pub async fn set_addon_media_list_item_content_kind(
        &self,
        command: commands::SetAddonMediaListItemContentKind,
    ) -> Result<()> {
        commands::set_addon_media_list_item_content_kind(command, self.ports.addon_media_lists())
            .await
    }

    /// Set or restore the playback behavior of one Remote Stream entry.
    pub async fn set_addon_media_list_item_liveness(
        &self,
        command: commands::SetAddonMediaListItemLiveness,
    ) -> Result<()> {
        commands::set_addon_media_list_item_liveness(command, self.ports.addon_media_lists()).await
    }

    /// Apply or restore playback behavior for selected or all Remote Stream entries.
    pub async fn apply_addon_media_list_liveness(
        &self,
        command: commands::ApplyAddonMediaListLiveness,
    ) -> Result<u64> {
        commands::apply_addon_media_list_liveness(command, self.ports.addon_media_lists()).await
    }

    /// Apply one classification to every entry of a list.
    pub async fn apply_addon_media_list_content_kind(
        &self,
        command: commands::ApplyAddonMediaListContentKind,
    ) -> Result<u64> {
        commands::apply_addon_media_list_content_kind(command, self.ports.addon_media_lists()).await
    }

    /// Apply exact quick-review snapshots atomically.
    pub async fn apply_addon_media_list_item_review_edits(
        &self,
        command: commands::ApplyAddonMediaListItemReviewEdits,
    ) -> Result<u64> {
        commands::apply_addon_media_list_item_review_edits(command, self.ports.addon_media_lists())
            .await
    }

    /// Assign consecutive episode numbers to included top-level entries.
    pub async fn assign_addon_media_list_episode_numbers(
        &self,
        command: commands::AssignAddonMediaListEpisodeNumbers,
    ) -> Result<u64> {
        commands::assign_addon_media_list_episode_numbers(command, self.ports.addon_media_lists())
            .await
    }

    /// Move one Add-on media list to a different add-on and re-import it.
    pub async fn change_addon_media_list_addon(
        &self,
        command: commands::ChangeAddonMediaListAddon,
    ) -> Result<()> {
        commands::change_addon_media_list_addon(command, self.ports.addon_media_lists()).await
    }

    /// Delete one Add-on media list.
    pub async fn delete_addon_media_list(
        &self,
        command: commands::DeleteAddonMediaList,
    ) -> Result<()> {
        commands::delete_addon_media_list(command, self.ports.addon_media_lists()).await
    }

    /// Set or clear the exact local version replacing one unavailable item.
    pub async fn set_addon_media_list_item_replacement(
        &self,
        command: commands::SetAddonMediaListItemReplacement,
    ) -> Result<()> {
        commands::set_addon_media_list_item_replacement(command, self.ports.addon_media_lists())
            .await
    }

    pub async fn request_addon_media_list_presentation(
        &self,
        command: commands::RequestAddonMediaListPresentation,
    ) -> Result<()> {
        commands::request_addon_media_list_presentation(command, self.ports.addon_media_lists())
            .await
    }

    pub async fn save_addon_media_list_show_nfo(
        &self,
        command: commands::SaveAddonMediaListNfo,
    ) -> Result<()> {
        commands::save_addon_media_list_show_nfo(command, self.ports.addon_media_lists()).await
    }

    pub async fn save_addon_media_list_item_nfo(
        &self,
        command: commands::SaveAddonMediaListNfo,
    ) -> Result<()> {
        commands::save_addon_media_list_item_nfo(command, self.ports.addon_media_lists()).await
    }

    pub async fn reset_addon_media_list_nfo(
        &self,
        list_id: i64,
        item_id: Option<i64>,
    ) -> Result<()> {
        commands::reset_addon_media_list_nfo(list_id, item_id, self.ports.addon_media_lists()).await
    }

    pub async fn update_addon_media_list_item_chapters(
        &self,
        command: commands::UpdateAddonMediaListItemChapters,
    ) -> Result<()> {
        commands::update_addon_media_list_item_chapters(command, self.ports.addon_media_lists())
            .await
    }

    /// Load one optional admin setting.
    pub async fn get_optional_setting(&self, key: &str) -> Result<Option<String>> {
        queries::get_optional_setting(key, self.ports.admin_settings()).await
    }

    /// Load a string admin setting.
    pub async fn get_string_setting(&self, key: &str, default: &str) -> Result<String> {
        queries::get_string_setting(key, default, self.ports.admin_settings()).await
    }

    /// Load a strictly decoded integer admin setting.
    pub async fn get_i32_setting(&self, key: &str, default: i32) -> Result<i32> {
        queries::get_i32_setting(key, default, self.ports.admin_settings()).await
    }

    /// Load a strictly decoded optional identity setting.
    pub async fn get_optional_id_setting(&self, key: &str) -> Result<Option<i64>> {
        queries::get_optional_id_setting(key, self.ports.admin_settings()).await
    }

    /// Load a strictly decoded boolean admin setting.
    pub async fn get_bool_setting(&self, key: &str, default: bool) -> Result<bool> {
        queries::get_bool_setting(key, default, self.ports.admin_settings()).await
    }

    /// Load typed settings select options.
    pub async fn list_settings_options(
        &self,
        kind: ersatzrs_core::repositories::SettingsOptionKind,
    ) -> Result<Vec<ersatzrs_core::repositories::SettingsSelectOption>> {
        queries::list_settings_options(kind, self.ports.admin_settings()).await
    }

    /// Load playout ids used by settings side effects.
    pub async fn list_settings_playout_ids(&self) -> Result<Vec<i64>> {
        queries::list_settings_playout_ids(self.ports.admin_settings()).await
    }

    /// Page through `FFmpeg` profile admin rows.
    pub async fn list_ffmpeg_profiles(
        &self,
        page: u32,
        page_size: u32,
    ) -> Result<Vec<queries::FfmpegProfileListRead>> {
        queries::list_ffmpeg_profiles(page, page_size, self.ports.ffmpeg_profiles()).await
    }

    /// Count `FFmpeg` profiles.
    pub async fn count_ffmpeg_profiles(&self) -> Result<u64> {
        queries::count_ffmpeg_profiles(self.ports.ffmpeg_profiles()).await
    }

    /// Load resolution options for `FFmpeg` profile editors.
    pub async fn list_ffmpeg_resolutions(&self) -> Result<Vec<(i64, String)>> {
        queries::list_ffmpeg_resolutions(self.ports.ffmpeg_profiles()).await
    }

    /// Load one `FFmpeg` profile editor model with valid absence.
    pub async fn get_ffmpeg_profile(
        &self,
        id: i64,
    ) -> Result<Option<commands::FfmpegProfileAdminForm>> {
        queries::get_ffmpeg_profile(id, self.ports.ffmpeg_profiles()).await
    }

    /// Validate and persist one `FFmpeg` profile.
    pub async fn save_ffmpeg_profile(&self, command: commands::SaveFfmpegProfile) -> Result<i64> {
        commands::save_ffmpeg_profile(command, self.ports.ffmpeg_profiles()).await
    }

    /// Delete one `FFmpeg` profile.
    pub async fn delete_ffmpeg_profile(
        &self,
        command: commands::DeleteFfmpegProfile,
    ) -> Result<()> {
        commands::delete_ffmpeg_profile(command, self.ports.ffmpeg_profiles()).await
    }

    /// Copy one `FFmpeg` profile.
    pub async fn copy_ffmpeg_profile(&self, command: commands::CopyFfmpegProfile) -> Result<i64> {
        commands::copy_ffmpeg_profile(command, self.ports.ffmpeg_profiles()).await
    }

    /// Load the configured filler-preset page size.
    pub async fn get_filler_presets_page_size(&self) -> Result<u32> {
        queries::get_filler_presets_page_size(self.ports.admin_settings()).await
    }

    /// Page through filler-preset rows.
    pub async fn list_filler_presets(
        &self,
        page: u32,
        page_size: u32,
    ) -> Result<(Vec<queries::FillerPresetListRead>, u64)> {
        queries::list_filler_presets(
            page,
            page_size,
            self.ports.fillers(),
            self.ports.admin_settings(),
        )
        .await
    }

    /// Load one filler-preset editor model with valid absence.
    pub async fn get_filler_preset(
        &self,
        id: i64,
    ) -> Result<Option<commands::FillerPresetAdminForm>> {
        queries::get_filler_preset(id, self.ports.fillers()).await
    }

    /// Load filler-preset reference options.
    pub async fn get_filler_editor_options(&self) -> Result<queries::FillerEditorOptionsRead> {
        queries::get_filler_editor_options(self.ports.collections()).await
    }

    /// Validate and persist one filler preset.
    pub async fn save_filler_preset(&self, command: commands::SaveFillerPreset) -> Result<i64> {
        commands::save_filler_preset(command, self.ports.fillers()).await
    }

    /// Delete one filler preset.
    pub async fn delete_filler_preset(&self, command: commands::DeleteFillerPreset) -> Result<()> {
        commands::delete_filler_preset(command, self.ports.fillers()).await
    }

    /// Page through watermark admin rows.
    pub async fn list_watermarks(
        &self,
        page: u32,
        page_size: u32,
    ) -> Result<Vec<queries::WatermarkListRead>> {
        queries::list_watermarks(page, page_size, self.ports.watermarks()).await
    }

    /// Count watermark admin rows.
    pub async fn count_watermarks(&self) -> Result<u64> {
        queries::count_watermarks(self.ports.watermarks()).await
    }

    /// Load one watermark editor model with valid absence.
    pub async fn get_watermark(&self, id: i64) -> Result<Option<commands::WatermarkAdminForm>> {
        queries::get_watermark(id, self.ports.watermarks()).await
    }

    /// Load one watermark editor model and its deterministic comparison neighbors.
    pub async fn get_watermark_editor(
        &self,
        id: i64,
    ) -> Result<Option<queries::WatermarkEditorRead>> {
        queries::get_watermark_editor(id, self.ports.watermarks()).await
    }

    /// Validate and persist one watermark.
    pub async fn save_watermark(&self, command: commands::SaveWatermark) -> Result<i64> {
        commands::save_watermark(command, self.ports.watermarks()).await
    }

    /// Delete one watermark.
    pub async fn delete_watermark(&self, command: commands::DeleteWatermark) -> Result<()> {
        commands::delete_watermark(command, self.ports.watermarks()).await
    }

    /// Copy one watermark.
    pub async fn copy_watermark(&self, command: commands::CopyWatermark) -> Result<i64> {
        commands::copy_watermark(command, self.ports.watermarks()).await
    }

    /// List template groups with their template counts.
    pub async fn list_template_groups(
        &self,
    ) -> Result<Vec<ersatzrs_core::repositories::TemplateGroupAdminRow>> {
        queries::list_template_groups(self.ports.templates()).await
    }

    /// Search template list rows.
    pub async fn list_templates(
        &self,
        search: &str,
    ) -> Result<Vec<ersatzrs_core::repositories::TemplateAdminRow>> {
        queries::list_templates(search, self.ports.templates()).await
    }

    /// Create one template group.
    pub async fn create_template_group(
        &self,
        command: commands::CreateTemplateGroup,
    ) -> Result<i64> {
        commands::create_template_group(command, self.ports.templates()).await
    }

    /// Create one empty template.
    pub async fn create_template(&self, command: commands::CreateTemplate) -> Result<i64> {
        commands::create_template(command, self.ports.templates()).await
    }

    /// Copy one template and its item graph.
    pub async fn copy_template(&self, command: commands::CopyTemplate) -> Result<i64> {
        commands::copy_template(command, self.ports.templates()).await
    }

    /// Delete one template group.
    pub async fn delete_template_group(
        &self,
        command: commands::DeleteTemplateGroup,
    ) -> Result<()> {
        commands::delete_template_group(command, self.ports.templates()).await
    }

    /// Delete one template.
    pub async fn delete_template(&self, command: commands::DeleteTemplate) -> Result<()> {
        commands::delete_template(command, self.ports.templates()).await
    }

    /// Load one template row with valid absence.
    pub async fn get_template(
        &self,
        id: i64,
    ) -> Result<Option<ersatzrs_core::repositories::TemplateAdminRow>> {
        queries::get_template(id, self.ports.templates()).await
    }

    /// Load template editor items.
    pub async fn get_template_items(
        &self,
        template_id: i64,
    ) -> Result<Vec<ersatzrs_core::repositories::TemplateItemAdmin>> {
        queries::get_template_items(template_id, self.ports.templates()).await
    }

    /// Load block-group choices for the template editor.
    pub async fn list_template_block_groups(
        &self,
    ) -> Result<Vec<ersatzrs_core::repositories::TemplateBlockGroupAdmin>> {
        queries::list_template_block_groups(self.ports.templates()).await
    }

    /// Load blocks for one template-editor group.
    pub async fn list_template_blocks(
        &self,
        group_id: i64,
    ) -> Result<Vec<ersatzrs_core::repositories::TemplateBlockAdmin>> {
        queries::list_template_blocks(group_id, self.ports.templates()).await
    }

    /// Validate and atomically replace one template item graph.
    pub async fn save_template(&self, command: commands::SaveTemplate) -> Result<()> {
        commands::save_template(command, self.ports.templates()).await
    }

    /// List block groups.
    pub async fn list_block_groups(
        &self,
    ) -> Result<Vec<ersatzrs_core::repositories::BlockGroupAdminRow>> {
        queries::list_block_groups(self.ports.blocks()).await
    }

    /// Search block list rows.
    pub async fn list_blocks(
        &self,
        search: &str,
    ) -> Result<Vec<ersatzrs_core::repositories::BlockAdminRow>> {
        queries::list_blocks(search, self.ports.blocks()).await
    }

    /// Create one block group.
    pub async fn create_block_group(&self, command: commands::CreateBlockGroup) -> Result<i64> {
        commands::create_block_group(command, self.ports.blocks()).await
    }

    /// Delete one block group.
    pub async fn delete_block_group(&self, command: commands::DeleteBlockGroup) -> Result<()> {
        commands::delete_block_group(command, self.ports.blocks()).await
    }

    /// Create one empty block.
    pub async fn create_block(&self, command: commands::CreateBlock) -> Result<i64> {
        commands::create_block(command, self.ports.blocks()).await
    }

    /// Copy one block and its item graph.
    pub async fn copy_block(&self, command: commands::CopyBlock) -> Result<i64> {
        commands::copy_block(command, self.ports.blocks()).await
    }

    /// Delete one block and its attached Smart Shuffle state.
    pub async fn delete_block(&self, command: commands::DeleteBlock) -> Result<()> {
        commands::delete_block(command, self.ports.blocks()).await
    }

    /// Load one block row with valid absence.
    pub async fn get_block(
        &self,
        id: i64,
    ) -> Result<Option<ersatzrs_core::repositories::BlockAdminRow>> {
        queries::get_block(id, self.ports.blocks()).await
    }

    /// Load block editor items.
    pub async fn get_block_items(
        &self,
        block_id: i64,
    ) -> Result<Vec<commands::BlockItemAdminForm>> {
        queries::get_block_items(block_id, self.ports.blocks()).await
    }

    /// Search one family of block editor reference options.
    pub async fn search_block_options(
        &self,
        kind: queries::BlockOptionKind,
        prefix: &str,
    ) -> Result<Vec<ersatzrs_core::repositories::BlockEditorOptionAdmin>> {
        queries::search_block_options(kind, prefix, self.ports.blocks()).await
    }

    /// Load all block editor reference options.
    pub async fn load_block_editor_options(
        &self,
    ) -> Result<ersatzrs_core::repositories::BlockEditorOptionsAdmin> {
        queries::load_block_editor_options(self.ports.blocks()).await
    }

    /// Validate and atomically replace one block item graph.
    pub async fn save_block(&self, command: commands::SaveBlock) -> Result<()> {
        commands::save_block(command, self.ports.blocks()).await
    }

    /// List deco groups.
    pub async fn list_deco_groups(
        &self,
    ) -> Result<Vec<ersatzrs_core::repositories::DecoGroupAdminRow>> {
        queries::list_deco_groups(self.ports.decos()).await
    }

    /// Search deco list rows.
    pub async fn list_decos(
        &self,
        search: &str,
    ) -> Result<Vec<ersatzrs_core::repositories::DecoAdminRow>> {
        queries::list_decos(search, self.ports.decos()).await
    }

    /// Create one deco group.
    pub async fn create_deco_group(&self, command: commands::CreateDecoGroup) -> Result<i64> {
        commands::create_deco_group(command, self.ports.decos()).await
    }

    /// Create one deco.
    pub async fn create_deco(&self, command: commands::CreateDeco) -> Result<i64> {
        commands::create_deco(command, self.ports.decos()).await
    }

    /// Delete one deco group.
    pub async fn delete_deco_group(&self, command: commands::DeleteDecoGroup) -> Result<()> {
        commands::delete_deco_group(command, self.ports.decos()).await
    }

    /// Delete one deco.
    pub async fn delete_deco(&self, command: commands::DeleteDeco) -> Result<()> {
        commands::delete_deco(command, self.ports.decos()).await
    }

    /// Load one complete deco editor form with valid absence.
    pub async fn get_deco(&self, id: i64) -> Result<Option<commands::DecoAdminForm>> {
        queries::get_deco(id, self.ports.decos()).await
    }

    /// Load deco editor reference options.
    pub async fn load_deco_editor_options(
        &self,
    ) -> Result<ersatzrs_core::repositories::DecoEditorOptionsAdmin> {
        queries::load_deco_editor_options(self.ports.decos()).await
    }

    /// Validate and atomically replace one deco graph.
    pub async fn save_deco(&self, command: commands::SaveDeco) -> Result<()> {
        commands::save_deco(command, self.ports.decos()).await
    }

    /// List deco-template groups.
    pub async fn list_deco_template_groups(
        &self,
    ) -> Result<Vec<ersatzrs_core::repositories::DecoTemplateGroupAdminRow>> {
        queries::list_deco_template_groups(self.ports.decos()).await
    }

    /// Search deco-template list rows.
    pub async fn list_deco_templates(
        &self,
        search: &str,
    ) -> Result<Vec<ersatzrs_core::repositories::DecoTemplateAdminRow>> {
        queries::list_deco_templates(search, self.ports.decos()).await
    }

    /// Create one deco-template group.
    pub async fn create_deco_template_group(
        &self,
        command: commands::CreateDecoTemplateGroup,
    ) -> Result<i64> {
        commands::create_deco_template_group(command, self.ports.decos()).await
    }

    /// Create one empty deco template.
    pub async fn create_deco_template(&self, command: commands::CreateDecoTemplate) -> Result<i64> {
        commands::create_deco_template(command, self.ports.decos()).await
    }

    /// Delete one deco-template group.
    pub async fn delete_deco_template_group(
        &self,
        command: commands::DeleteDecoTemplateGroup,
    ) -> Result<()> {
        commands::delete_deco_template_group(command, self.ports.decos()).await
    }

    /// Delete one deco template.
    pub async fn delete_deco_template(&self, command: commands::DeleteDecoTemplate) -> Result<()> {
        commands::delete_deco_template(command, self.ports.decos()).await
    }

    /// Load one deco-template row with valid absence.
    pub async fn get_deco_template(
        &self,
        id: i64,
    ) -> Result<Option<ersatzrs_core::repositories::DecoTemplateAdminRow>> {
        queries::get_deco_template(id, self.ports.decos()).await
    }

    /// Load one deco template's item graph.
    pub async fn get_deco_template_items(
        &self,
        id: i64,
    ) -> Result<Vec<ersatzrs_core::repositories::DecoTemplateItemAdmin>> {
        queries::get_deco_template_items(id, self.ports.decos()).await
    }

    /// Validate and atomically replace one deco-template item graph.
    pub async fn save_deco_template(&self, command: commands::SaveDecoTemplate) -> Result<()> {
        commands::save_deco_template(command, self.ports.decos()).await
    }

    /// Load the complete `FFmpeg` settings read model.
    pub async fn load_ffmpeg_settings(&self) -> Result<queries::FfmpegSettingsReadModel> {
        queries::load_ffmpeg_settings(self.ports.admin_settings()).await
    }

    /// Load logging settings.
    pub async fn load_logging_settings(&self) -> Result<queries::LoggingSettings> {
        queries::load_logging_settings(self.ports.admin_settings()).await
    }

    /// Load HDHR settings and ensure a stable device UUID.
    pub async fn load_hdhr_settings(&self) -> Result<queries::HdhrSettings> {
        queries::load_hdhr_settings(self.ports.admin_settings()).await
    }

    /// Load scanner settings.
    pub async fn load_scanner_settings(&self) -> Result<queries::ScannerSettings> {
        queries::load_scanner_settings(self.ports.admin_settings()).await
    }

    /// Load the global fair-use delay window for add-on item details.
    pub async fn load_addon_detail_wait_settings(
        &self,
    ) -> Result<queries::AddonDetailWaitSettings> {
        queries::load_addon_detail_wait_settings(self.ports.admin_settings()).await
    }

    /// Load playout settings.
    pub async fn load_playout_settings(&self) -> Result<queries::PlayoutSettings> {
        queries::load_playout_settings(self.ports.admin_settings()).await
    }

    /// Load UI settings.
    pub async fn load_ui_settings(&self) -> Result<queries::UiSettings> {
        queries::load_ui_settings(self.ports.admin_settings()).await
    }

    /// Inspect the current user's Windows startup registration.
    #[must_use]
    pub fn windows_startup_status(&self) -> WindowsStartupStatus {
        self.ports.settings_runtime().windows_startup_status()
    }

    /// Load XMLTV settings.
    pub async fn load_xmltv_settings(&self) -> Result<queries::XmltvSettings> {
        queries::load_xmltv_settings(self.ports.admin_settings()).await
    }

    /// Page through manual collection admin rows.
    pub async fn list_collections(
        &self,
        query: queries::ListCollections,
    ) -> Result<(Vec<ersatzrs_core::repositories::CollectionAdminRow>, u64)> {
        queries::list_collections(query, self.ports.collections()).await
    }

    /// Load one manual collection editor.
    pub async fn get_collection(
        &self,
        id: ersatzrs_domain::EntityId,
    ) -> Result<Option<ersatzrs_core::repositories::CollectionAdminRow>> {
        queries::get_collection(id, self.ports.collections()).await
    }

    /// List all manual collections for selectors.
    pub async fn list_all_collections(
        &self,
    ) -> Result<Vec<ersatzrs_core::repositories::CollectionAdminRow>> {
        queries::list_all_collections(self.ports.collections()).await
    }

    /// Page through smart collection admin rows.
    pub async fn list_smart_collections(
        &self,
        query: queries::ListCollections,
    ) -> Result<(
        Vec<ersatzrs_core::repositories::SmartCollectionAdminRow>,
        u64,
    )> {
        queries::list_smart_collections(query, self.ports.collections()).await
    }

    /// Load one smart collection editor.
    pub async fn get_smart_collection(
        &self,
        id: ersatzrs_domain::EntityId,
    ) -> Result<Option<ersatzrs_core::repositories::SmartCollectionAdminRow>> {
        queries::get_smart_collection(id, self.ports.collections()).await
    }

    /// List reusable advanced-search queries.
    pub async fn list_saved_searches(
        &self,
    ) -> Result<Vec<ersatzrs_core::repositories::SavedSearchAdminRow>> {
        queries::list_saved_searches(self.ports.saved_searches()).await
    }

    /// Load one reusable advanced-search query.
    pub async fn get_saved_search(
        &self,
        id: ersatzrs_domain::EntityId,
    ) -> Result<Option<ersatzrs_core::repositories::SavedSearchAdminRow>> {
        queries::get_saved_search(id, self.ports.saved_searches()).await
    }

    /// Load the persisted multi-collection page size.
    pub async fn get_multi_collections_page_size(&self) -> Result<u32> {
        queries::get_multi_collections_page_size(self.ports.admin_settings()).await
    }

    /// Persist the page size and page through multi-collection admin rows.
    pub async fn list_multi_collections(
        &self,
        query: queries::ListCollections,
    ) -> Result<(
        Vec<ersatzrs_core::repositories::MultiCollectionAdminRow>,
        u64,
    )> {
        queries::list_multi_collections(
            query,
            self.ports.collections(),
            self.ports.admin_settings(),
        )
        .await
    }

    /// Load one multi-collection editor.
    pub async fn get_multi_collection(
        &self,
        id: ersatzrs_domain::EntityId,
    ) -> Result<Option<ersatzrs_core::repositories::MultiCollectionAdmin>> {
        queries::get_multi_collection(id, self.ports.collections()).await
    }

    /// Load multi-collection selector reference data.
    pub async fn get_multi_collection_editor_options(
        &self,
    ) -> Result<ersatzrs_core::repositories::MultiCollectionEditorOptions> {
        queries::get_multi_collection_editor_options(self.ports.collections()).await
    }

    /// Load the persisted rerun-collection page size.
    pub async fn get_rerun_collections_page_size(&self) -> Result<u32> {
        queries::get_rerun_collections_page_size(self.ports.admin_settings()).await
    }

    /// Persist the page size and page through rerun-collection admin rows.
    pub async fn list_rerun_collections(
        &self,
        query: queries::ListCollections,
    ) -> Result<(
        Vec<ersatzrs_core::repositories::RerunCollectionAdminRow>,
        u64,
    )> {
        queries::list_rerun_collections(
            query,
            self.ports.collections(),
            self.ports.admin_settings(),
        )
        .await
    }

    /// Load one rerun-collection editor.
    pub async fn get_rerun_collection(
        &self,
        id: ersatzrs_domain::EntityId,
    ) -> Result<Option<ersatzrs_core::repositories::RerunCollectionAdmin>> {
        queries::get_rerun_collection(id, self.ports.collections()).await
    }

    /// Load rerun-collection selector reference data.
    pub async fn get_rerun_collection_editor_options(
        &self,
    ) -> Result<ersatzrs_core::repositories::RerunCollectionEditorOptions> {
        queries::get_rerun_collection_editor_options(self.ports.collections()).await
    }

    /// List playlist groups and their child playlists.
    pub async fn list_playlist_groups(
        &self,
    ) -> Result<Vec<ersatzrs_core::repositories::PlaylistGroupAdminRow>> {
        queries::list_playlist_groups(self.ports.collections()).await
    }

    /// Load one playlist editor.
    pub async fn get_playlist_editor(
        &self,
        id: ersatzrs_domain::EntityId,
    ) -> Result<Option<ersatzrs_core::repositories::PlaylistEditorAdmin>> {
        queries::get_playlist_editor(id, self.ports.collections()).await
    }

    /// Load playlist editor selector reference data.
    pub async fn get_playlist_editor_options(
        &self,
    ) -> Result<ersatzrs_core::repositories::PlaylistEditorOptions> {
        queries::get_playlist_editor_options(self.ports.collections()).await
    }

    /// Suggest movie and show targets for a manual collection.
    pub async fn suggest_media_items(
        &self,
        prefix: &str,
    ) -> Result<Vec<ersatzrs_core::repositories::MediaItemSuggestionAdmin>> {
        queries::suggest_media_items(prefix, self.ports.collections()).await
    }

    /// Load one manual collection detail projection.
    pub async fn get_collection_detail(
        &self,
        id: ersatzrs_domain::EntityId,
    ) -> Result<Option<ersatzrs_core::repositories::CollectionDetailAdmin>> {
        queries::get_collection_detail(id, self.ports.collections()).await
    }

    /// List movie and show members used by collection dialogs.
    pub async fn list_collection_members(
        &self,
        id: ersatzrs_domain::EntityId,
    ) -> Result<Vec<ersatzrs_core::repositories::CollectionMemberAdmin>> {
        queries::list_collection_members(id, self.ports.collections()).await
    }

    /// Load persisted list and detail page sizes for schedule admin pages.
    pub async fn get_schedules_page_sizes(&self) -> Result<(u32, u32)> {
        queries::get_schedules_page_sizes(self.ports.admin_settings()).await
    }

    /// Persist the schedule-detail page size.
    pub async fn save_schedules_detail_page_size(&self, page_size: u32) -> Result<()> {
        queries::save_schedules_detail_page_size(page_size, self.ports.admin_settings()).await
    }

    /// Page through program schedule admin rows.
    pub async fn list_schedules(
        &self,
        query: queries::ListCollections,
    ) -> Result<(Vec<ersatzrs_core::repositories::ScheduleAdminRow>, u64)> {
        queries::list_schedules(
            query,
            self.ports.program_schedules(),
            self.ports.admin_settings(),
        )
        .await
    }

    pub async fn list_all_schedules(
        &self,
    ) -> Result<Vec<ersatzrs_core::repositories::ScheduleAdminRow>> {
        let (rows, _) = self
            .ports
            .program_schedules()
            .list_admin("", 0, u32::MAX)
            .await?;
        Ok(rows)
    }

    /// Load one program schedule editor, including the create default.
    pub async fn get_schedule(
        &self,
        id: ersatzrs_domain::EntityId,
    ) -> Result<Option<ersatzrs_core::repositories::ScheduleAdminForm>> {
        queries::get_schedule(id, self.ports.program_schedules()).await
    }

    /// Load the editable items for one program schedule.
    pub async fn list_schedule_items(
        &self,
        id: ersatzrs_domain::EntityId,
    ) -> Result<Vec<ersatzrs_core::repositories::ScheduleItemAdmin>> {
        queries::list_schedule_items(id, self.ports.program_schedules()).await
    }

    /// Load select options used by the schedule-item editor.
    pub async fn get_schedule_item_options(
        &self,
    ) -> Result<ersatzrs_core::repositories::ScheduleItemOptions> {
        queries::get_schedule_item_options(self.ports.program_schedules()).await
    }

    /// Validate and save one manual collection.
    pub async fn save_collection(
        &self,
        command: commands::SaveCollection,
    ) -> Result<ersatzrs_domain::EntityId> {
        commands::save_collection(command, self.ports.collections()).await
    }

    /// Delete one manual collection.
    pub async fn delete_collection(&self, command: commands::DeleteCollection) -> Result<()> {
        commands::delete_collection(command, self.ports.collections()).await
    }

    /// Validate, save, and refresh one smart collection.
    pub async fn save_smart_collection(
        &self,
        command: commands::SaveSmartCollection,
    ) -> Result<ersatzrs_domain::EntityId> {
        commands::save_smart_collection(
            command,
            self.ports.collections(),
            self.ports.playout_builds(),
            &self.operations,
        )
        .await
    }

    /// Delete one smart collection.
    pub async fn delete_smart_collection(
        &self,
        command: commands::DeleteSmartCollection,
    ) -> Result<()> {
        commands::delete_smart_collection(command, self.ports.collections()).await
    }

    /// Validate and persist a reusable advanced-search query.
    pub async fn save_saved_search(
        &self,
        command: commands::SaveSavedSearch,
    ) -> Result<ersatzrs_domain::EntityId> {
        commands::save_saved_search(command, self.ports.saved_searches()).await
    }

    /// Delete a reusable advanced-search query.
    pub async fn delete_saved_search(&self, command: commands::DeleteSavedSearch) -> Result<()> {
        commands::delete_saved_search(command, self.ports.saved_searches()).await
    }

    /// Validate, transactionally save, and refresh one multi-collection.
    pub async fn save_multi_collection(
        &self,
        command: commands::SaveMultiCollection,
    ) -> Result<ersatzrs_domain::EntityId> {
        commands::save_multi_collection(
            command,
            self.ports.collections(),
            self.ports.playout_builds(),
            &self.operations,
        )
        .await
    }

    /// Delete one multi-collection.
    pub async fn delete_multi_collection(
        &self,
        command: commands::DeleteMultiCollection,
    ) -> Result<()> {
        commands::delete_multi_collection(command, self.ports.collections()).await
    }

    /// Validate, save, and refresh one rerun collection.
    pub async fn save_rerun_collection(
        &self,
        command: commands::SaveRerunCollection,
    ) -> Result<ersatzrs_domain::EntityId> {
        commands::save_rerun_collection(
            command,
            self.ports.collections(),
            self.ports.playout_builds(),
            &self.operations,
        )
        .await
    }

    /// Delete one rerun collection.
    pub async fn delete_rerun_collection(
        &self,
        command: commands::DeleteRerunCollection,
    ) -> Result<()> {
        commands::delete_rerun_collection(command, self.ports.collections()).await
    }

    /// Validate and create one playlist group.
    pub async fn create_playlist_group(
        &self,
        command: commands::CreatePlaylistGroup,
    ) -> Result<ersatzrs_core::repositories::PlaylistGroupAdminRow> {
        commands::create_playlist_group(command, self.ports.collections()).await
    }

    /// Validate and create one playlist.
    pub async fn create_playlist(
        &self,
        command: commands::CreatePlaylist,
    ) -> Result<ersatzrs_core::repositories::PlaylistAdminRow> {
        commands::create_playlist(command, self.ports.collections()).await
    }

    /// Delete one non-system playlist group.
    pub async fn delete_playlist_group(
        &self,
        command: commands::DeletePlaylistGroup,
    ) -> Result<()> {
        commands::delete_playlist_group(command, self.ports.collections()).await
    }

    /// Delete one non-system playlist.
    pub async fn delete_playlist(&self, command: commands::DeletePlaylist) -> Result<()> {
        commands::delete_playlist(command, self.ports.collections()).await
    }

    /// Validate and transactionally replace one playlist definition.
    pub async fn save_playlist(&self, command: commands::SavePlaylist) -> Result<()> {
        commands::save_playlist(command, self.ports.collections()).await
    }

    /// Add media items to a manual collection and refresh affected playouts.
    pub async fn add_collection_items(&self, command: commands::AddCollectionItems) -> Result<u64> {
        commands::add_collection_items(
            command,
            self.ports.collections(),
            self.ports.playout_builds(),
            &self.operations,
        )
        .await
    }

    /// Add one media item to an existing collection or atomically create its collection.
    pub async fn add_media_item_to_collection(
        &self,
        command: commands::AddMediaItemToCollection,
    ) -> Result<ersatzrs_domain::EntityId> {
        commands::add_media_item_to_collection(
            command,
            self.ports.collections(),
            self.ports.playout_builds(),
            &self.operations,
        )
        .await
    }

    /// Append typed media items to a playlist.
    pub async fn add_playlist_items(&self, command: commands::AddPlaylistItems) -> Result<u64> {
        commands::add_playlist_items(command, self.ports.collections()).await
    }

    /// Toggle custom manual-collection ordering and refresh affected playouts.
    pub async fn set_collection_custom_order(
        &self,
        command: commands::SetCollectionCustomOrder,
    ) -> Result<()> {
        commands::set_collection_custom_order(
            command,
            self.ports.collections(),
            self.ports.playout_builds(),
            &self.operations,
        )
        .await
    }

    /// Remove selected manual-collection rows and refresh affected playouts.
    pub async fn remove_collection_rows(
        &self,
        command: commands::RemoveCollectionRows,
    ) -> Result<()> {
        commands::remove_collection_rows(
            command,
            self.ports.collections(),
            self.ports.playout_builds(),
            &self.operations,
        )
        .await
    }

    /// Persist custom manual-collection order and refresh affected playouts.
    pub async fn update_collection_custom_order(
        &self,
        command: commands::UpdateCollectionCustomOrder,
    ) -> Result<()> {
        commands::update_collection_custom_order(
            command,
            self.ports.collections(),
            self.ports.playout_builds(),
            &self.operations,
        )
        .await
    }

    /// Validate and save a program schedule, refreshing affected playouts on runtime changes.
    pub async fn save_schedule(
        &self,
        command: commands::SaveSchedule,
    ) -> Result<ersatzrs_domain::EntityId> {
        commands::save_schedule(
            command,
            self.ports.program_schedules(),
            self.ports.playout_builds(),
            &self.operations,
        )
        .await
    }

    /// Delete a program schedule and its item policy configuration.
    pub async fn delete_schedule(&self, command: commands::DeleteSchedule) -> Result<()> {
        commands::delete_schedule(command, self.ports.program_schedules()).await
    }

    /// Copy a complete program schedule graph.
    pub async fn copy_schedule(
        &self,
        command: commands::CopySchedule,
    ) -> Result<ersatzrs_domain::EntityId> {
        commands::copy_schedule(command, self.ports.program_schedules()).await
    }

    /// Validate and atomically replace schedule items, then refresh affected playouts.
    pub async fn replace_schedule_items(
        &self,
        command: commands::ReplaceScheduleItems,
    ) -> Result<()> {
        commands::replace_schedule_items(
            command,
            self.ports.program_schedules(),
            self.ports.playout_builds(),
            &self.operations,
        )
        .await
    }

    pub async fn append_schedule_media_item(
        &self,
        command: commands::AppendScheduleMediaItem,
    ) -> Result<()> {
        commands::append_schedule_media_item(
            command,
            self.ports.program_schedules(),
            self.ports.media_items(),
            self.ports.playout_builds(),
            &self.operations,
        )
        .await
    }

    /// Add one default playback row and refresh affected playouts.
    pub async fn add_default_collection_item(
        &self,
        command: commands::ChangeCollectionItem,
    ) -> Result<()> {
        commands::add_default_collection_item(
            command,
            self.ports.collections(),
            self.ports.playout_builds(),
            &self.operations,
        )
        .await
    }

    /// Remove collection rows and refresh affected playouts.
    pub async fn remove_collection_item(
        &self,
        command: commands::ChangeCollectionItem,
    ) -> Result<()> {
        commands::remove_collection_item(
            command,
            self.ports.collections(),
            self.ports.playout_builds(),
            &self.operations,
        )
        .await
    }

    /// Persist one admin settings value.
    pub async fn set_admin_setting(&self, command: commands::SetAdminSetting) -> Result<()> {
        commands::set_admin_setting(command, self.ports.admin_settings()).await
    }

    /// Persist or clear one optional admin setting.
    pub async fn set_optional_admin_setting(
        &self,
        command: commands::SetOptionalAdminSetting,
    ) -> Result<()> {
        commands::set_optional_admin_setting(command, self.ports.admin_settings()).await
    }

    /// Create one custom `FFmpeg` resolution.
    pub async fn create_custom_resolution(
        &self,
        command: commands::CreateCustomResolution,
    ) -> Result<()> {
        commands::create_custom_resolution(command, self.ports.admin_settings()).await
    }

    /// Delete one custom `FFmpeg` resolution.
    pub async fn delete_custom_resolution(
        &self,
        command: commands::DeleteCustomResolution,
    ) -> Result<()> {
        commands::delete_custom_resolution(command, self.ports.admin_settings()).await
    }

    /// Validate, atomically save, and apply logging settings.
    pub async fn save_logging_settings(
        &self,
        command: commands::SaveLoggingSettings,
    ) -> Result<()> {
        commands::save_logging_settings(
            command,
            self.ports.admin_settings(),
            self.ports.settings_runtime(),
        )
        .await
    }

    /// Validate, atomically save, and apply HDHR settings.
    pub async fn save_hdhr_settings(&self, command: commands::SaveHdhrSettings) -> Result<()> {
        commands::save_hdhr_settings(
            command,
            self.ports.admin_settings(),
            self.ports.settings_runtime(),
        )
        .await
    }

    /// Validate and atomically save scanner settings.
    pub async fn save_scanner_settings(
        &self,
        command: commands::SaveScannerSettings,
    ) -> Result<()> {
        commands::save_scanner_settings(command, self.ports.admin_settings()).await
    }

    /// Validate and atomically save the global add-on detail pacing window.
    pub async fn save_addon_detail_wait_settings(
        &self,
        command: commands::SaveAddonDetailWaitSettings,
    ) -> Result<()> {
        commands::save_addon_detail_wait_settings(command, self.ports.admin_settings()).await
    }

    /// Validate, atomically save, and apply playout settings.
    pub async fn save_playout_settings(
        &self,
        command: commands::SavePlayoutSettings,
    ) -> Result<()> {
        commands::save_playout_settings(
            command,
            self.ports.admin_settings(),
            self.ports.playout_builds(),
            &self.operations,
        )
        .await
    }

    /// Atomically save transport-normalized UI settings.
    pub async fn save_ui_settings(&self, command: commands::SaveUiSettings) -> Result<()> {
        commands::save_ui_settings(command, self.ports.admin_settings()).await
    }

    /// Apply the current user's Windows startup registration.
    pub fn set_windows_startup(&self, enabled: bool) -> Result<()> {
        commands::set_windows_startup(enabled, self.ports.settings_runtime())
    }

    /// Validate, atomically save, and apply XMLTV settings.
    pub async fn save_xmltv_settings(&self, command: commands::SaveXmltvSettings) -> Result<()> {
        commands::save_xmltv_settings(
            command,
            self.ports.admin_settings(),
            self.ports.playout_builds(),
            &self.operations,
        )
        .await
    }

    /// Validate, atomically save, and apply `FFmpeg` settings.
    pub async fn save_ffmpeg_settings(&self, command: commands::SaveFfmpegSettings) -> Result<()> {
        commands::save_ffmpeg_settings(
            command,
            self.ports.admin_settings(),
            self.ports.settings_runtime(),
        )
        .await
    }

    pub async fn list_playouts(
        &self,
        query: &str,
        page: u32,
        page_size: u32,
    ) -> Result<Vec<ersatzrs_core::repositories::PlayoutListAdminRow>> {
        queries::list_playouts(query, page, page_size, self.ports.playouts()).await
    }

    pub async fn count_playouts(&self) -> Result<u64> {
        queries::count_playouts(self.ports.playouts()).await
    }

    pub async fn count_playout_warnings(&self) -> Result<u32> {
        queries::count_playout_warnings(self.ports.playouts()).await
    }

    pub async fn list_admin_libraries(
        &self,
    ) -> Result<Vec<ersatzrs_core::repositories::AdminLibraryRow>> {
        self.ports
            .libraries()
            .list_admin()
            .await
            .map_err(Into::into)
    }

    pub async fn list_external_collections(
        &self,
    ) -> Result<Vec<ersatzrs_core::repositories::AdminExternalCollectionRow>> {
        self.ports
            .libraries()
            .list_external_collections_admin()
            .await
            .map_err(Into::into)
    }

    pub fn request_external_collections_sync(
        &self,
        media_source_id: i64,
        kind: &str,
        deep_scan: bool,
    ) -> Result<bool> {
        use ersatzrs_core::repositories::RemoteMediaSourceKind;

        if media_source_id <= 0 {
            return Err(ApplicationError::Validation {
                field: "media_source_id",
                message: "must be greater than zero".to_owned(),
            });
        }
        let kind = match kind {
            "Plex" => RemoteMediaSourceKind::Plex,
            "Jellyfin" => RemoteMediaSourceKind::Jellyfin,
            "Emby" => RemoteMediaSourceKind::Emby,
            value => {
                return Err(ApplicationError::Validation {
                    field: "media_source_kind",
                    message: format!("unsupported value {value}"),
                });
            }
        };
        let key = operations::OperationKey::ExternalCollections {
            provider: match kind {
                RemoteMediaSourceKind::Plex => "Plex",
                RemoteMediaSourceKind::Jellyfin => "Jellyfin",
                RemoteMediaSourceKind::Emby => "Emby",
            }
            .to_owned(),
            media_source_id,
        };
        let Some(guard) = self
            .operations
            .begin(commands::BeginOperation::new(key))
            .into_guard()
        else {
            return Ok(false);
        };
        self.ports
            .media_source_runtime()
            .request_external_collections_sync(kind, media_source_id, deep_scan, guard);
        Ok(true)
    }

    pub async fn admin_health_issue_counts(&self) -> Result<(u32, u32)> {
        self.ports
            .admin_health_runtime()
            .issue_counts()
            .await
            .map_err(|error| ApplicationError::Unavailable {
                operation: "load admin health issue counts",
                message: error.to_string(),
            })
    }

    pub async fn run_admin_health_checks(&self) -> Result<AdminHealthRead> {
        self.ports
            .admin_health_runtime()
            .run_health_checks()
            .await
            .map_err(|error| ApplicationError::Unavailable {
                operation: "run admin health checks",
                message: error.to_string(),
            })
    }

    pub async fn admin_ffmpeg_runtime(&self) -> AdminFfmpegRuntimeRead {
        self.ports.admin_health_runtime().ffmpeg_runtime().await
    }

    pub async fn list_block_playout_options(
        &self,
    ) -> Result<Vec<ersatzrs_core::repositories::BlockPlayoutOptionAdmin>> {
        self.ports
            .troubleshooting()
            .list_block_playout_options_admin()
            .await
            .map_err(Into::into)
    }

    pub async fn list_blocks_for_playout(
        &self,
        playout_id: i64,
    ) -> Result<Vec<ersatzrs_core::repositories::TroubleshootingBlockAdmin>> {
        self.ports
            .troubleshooting()
            .list_blocks_for_playout_admin(playout_id)
            .await
            .map_err(Into::into)
    }

    pub async fn list_block_playout_history(
        &self,
        playout_id: i64,
        block_id: i64,
    ) -> Result<Vec<ersatzrs_core::repositories::BlockPlayoutHistoryAdmin>> {
        self.ports
            .troubleshooting()
            .list_block_playout_history_admin(playout_id, block_id)
            .await
            .map_err(Into::into)
    }

    pub async fn load_playback_troubleshooting(
        &self,
        media_item_id: i64,
        channel_id: i64,
    ) -> Result<queries::PlaybackTroubleshootingReadModel> {
        let data = self
            .ports
            .troubleshooting()
            .load_playback_context_admin(media_item_id, channel_id)
            .await?;
        let stream_selectors = self
            .ports
            .troubleshooting_runtime()
            .list_stream_selectors()
            .await
            .map_err(|error| ApplicationError::Unavailable {
                operation: "load playback stream selectors",
                message: error.to_string(),
            })?;
        Ok(queries::PlaybackTroubleshootingReadModel {
            data,
            stream_selectors,
        })
    }

    pub async fn list_mpeg_troubleshooting_channels(
        &self,
    ) -> Result<Vec<ersatzrs_core::repositories::MpegTroubleshootingChannelAdmin>> {
        self.ports
            .troubleshooting()
            .list_mpeg_channels_admin()
            .await
            .map_err(Into::into)
    }

    pub async fn load_troubleshooting_report_data(
        &self,
    ) -> Result<queries::TroubleshootingReportReadModel> {
        let ffmpeg_settings = self.load_ffmpeg_settings().await?.form;
        let channels = self
            .ports
            .troubleshooting()
            .list_report_channels_admin()
            .await?;
        let ffmpeg_profiles = self
            .ports
            .troubleshooting()
            .list_report_ffmpeg_profiles_admin()
            .await?;
        Ok(queries::TroubleshootingReportReadModel {
            ffmpeg_settings,
            channels,
            ffmpeg_profiles,
        })
    }

    pub async fn start_playback_preview(
        &self,
        request: PlaybackPreviewRequest,
    ) -> std::result::Result<(), AdminTroubleshootingRuntimeError> {
        self.ports
            .troubleshooting_runtime()
            .start_playback_preview(request)
            .await
    }

    pub async fn build_troubleshooting_archive(
        &self,
    ) -> std::result::Result<Option<Vec<u8>>, AdminTroubleshootingRuntimeError> {
        self.ports
            .troubleshooting_runtime()
            .build_troubleshooting_archive()
            .await
    }

    pub async fn build_media_sample_archive(
        &self,
        media_item_id: i64,
    ) -> std::result::Result<Option<Vec<u8>>, AdminTroubleshootingRuntimeError> {
        if media_item_id <= 0 {
            return Err(AdminTroubleshootingRuntimeError::NotFound(
                "media item does not exist".to_owned(),
            ));
        }
        self.ports
            .troubleshooting_runtime()
            .build_media_sample_archive(media_item_id)
            .await
    }

    pub async fn media_state_revision(&self) -> Result<String> {
        queries::media_state_revision(self.ports.libraries()).await
    }

    pub async fn admin_search_snapshot(
        &self,
        query: &str,
    ) -> Result<ersatzrs_core::repositories::AdminSearchSnapshot> {
        queries::admin_search_snapshot(query, self.ports.admin_search()).await
    }

    pub async fn admin_search_documents(
        &self,
        query: &str,
    ) -> Result<Vec<ersatzrs_core::search::SearchDocument>> {
        queries::admin_search_documents(query, self.ports.admin_search()).await
    }

    pub async fn plex_artwork_connection(
        &self,
        source_id: i64,
    ) -> Result<Option<ersatzrs_core::repositories::PlexArtworkConnectionAdmin>> {
        self.ports
            .media_sources()
            .plex_artwork_connection_admin(source_id)
            .await
            .map_err(Into::into)
    }

    pub async fn first_jellyfin_address(&self) -> Result<Option<String>> {
        self.ports
            .media_sources()
            .first_jellyfin_address_admin()
            .await
            .map_err(Into::into)
    }

    pub async fn first_emby_address(&self) -> Result<Option<String>> {
        self.ports
            .media_sources()
            .first_emby_address_admin()
            .await
            .map_err(Into::into)
    }

    pub async fn list_local_libraries(
        &self,
    ) -> Result<Vec<ersatzrs_core::repositories::LocalLibraryAdminRow>> {
        self.ports
            .media_sources()
            .list_local_libraries_admin()
            .await
            .map_err(Into::into)
    }

    pub async fn list_managed_addon_media_sources(
        &self,
    ) -> Result<Vec<ersatzrs_core::repositories::ManagedAddonMediaSourceAdminRow>> {
        self.ports
            .media_sources()
            .list_managed_addon_media_sources_admin()
            .await
            .map_err(Into::into)
    }

    pub async fn get_local_library(
        &self,
        id: i64,
    ) -> Result<Option<ersatzrs_core::repositories::LocalLibraryAdmin>> {
        if id <= 0 {
            return Err(ApplicationError::Validation {
                field: "library_id",
                message: "must be greater than zero".to_owned(),
            });
        }
        self.ports
            .media_sources()
            .get_local_library_admin(id)
            .await
            .map_err(Into::into)
    }

    pub async fn save_local_library(
        &self,
        mut library: ersatzrs_core::repositories::LocalLibraryAdmin,
    ) -> Result<i64> {
        library.name = library.name.trim().to_owned();
        if library.name.is_empty() {
            return Err(ApplicationError::Validation {
                field: "name",
                message: "is required".to_owned(),
            });
        }
        if !matches!(library.media_kind, 1..=7) {
            return Err(ApplicationError::Validation {
                field: "media_kind",
                message: "is invalid".to_owned(),
            });
        }
        let mut seen = std::collections::HashSet::new();
        let mut paths = Vec::new();
        for path in library.paths {
            let trimmed = path.path.trim();
            if trimmed.is_empty() {
                continue;
            }
            let uri_path = trimmed
                .strip_prefix("file://localhost")
                .or_else(|| trimmed.strip_prefix("file://"))
                .unwrap_or(trimmed);
            let decoded = urlencoding::decode(uri_path)
                .map_or_else(|_| uri_path.to_owned(), std::borrow::Cow::into_owned);
            let path_buf = std::path::Path::new(&decoded);
            let normalized =
                dunce::canonicalize(path_buf).unwrap_or_else(|_| path_buf.to_path_buf());
            let normalized = normalized
                .to_string_lossy()
                .trim_end_matches(['/', '\\'])
                .to_owned();
            if !std::path::Path::new(&normalized).is_dir() {
                return Err(ApplicationError::Validation {
                    field: "path",
                    message: format!("does not exist: {normalized}"),
                });
            }
            if !normalized.is_empty() && seen.insert(normalized.clone()) {
                paths.push(ersatzrs_core::repositories::LocalLibraryPathAdmin {
                    id: path.id,
                    path: normalized,
                });
            }
        }
        library.paths = paths;
        self.ports
            .media_sources()
            .save_local_library_admin(&library)
            .await
            .map_err(Into::into)
    }

    pub async fn delete_local_library(&self, id: i64) -> Result<()> {
        if id <= 0 {
            return Err(ApplicationError::Validation {
                field: "library_id",
                message: "must be greater than zero".to_owned(),
            });
        }
        self.ports
            .media_sources()
            .delete_local_library_admin(id)
            .await
            .map_err(Into::into)
    }

    pub async fn list_remote_sources(
        &self,
        kind: ersatzrs_core::repositories::RemoteMediaSourceKind,
    ) -> Result<Vec<ersatzrs_core::repositories::RemoteSourceAdminRow>> {
        self.ports
            .media_sources()
            .list_remote_sources_admin(kind)
            .await
            .map_err(Into::into)
    }

    pub async fn get_remote_connection(
        &self,
        kind: ersatzrs_core::repositories::RemoteMediaSourceKind,
    ) -> Result<Option<ersatzrs_core::repositories::RemoteConnectionAdmin>> {
        self.ports
            .media_sources()
            .get_remote_connection_admin(kind)
            .await
            .map_err(Into::into)
    }

    pub async fn save_remote_connection(
        &self,
        kind: ersatzrs_core::repositories::RemoteMediaSourceKind,
        mut connection: ersatzrs_core::repositories::RemoteConnectionAdmin,
        server_name: &str,
        operating_system: &str,
        api_key: &str,
    ) -> Result<i64> {
        connection.address = connection.address.trim().trim_end_matches('/').to_owned();
        if connection.address.is_empty() {
            return Err(ApplicationError::Validation {
                field: "address",
                message: "is required".to_owned(),
            });
        }
        if api_key.trim().is_empty() {
            return Err(ApplicationError::Validation {
                field: "api_key",
                message: "is required".to_owned(),
            });
        }
        let id = self
            .ports
            .media_sources()
            .save_remote_connection_admin(kind, &connection, server_name, operating_system)
            .await?;
        let key = match kind {
            ersatzrs_core::repositories::RemoteMediaSourceKind::Jellyfin => {
                "jellyfin.current_server_api_key"
            }
            ersatzrs_core::repositories::RemoteMediaSourceKind::Emby => {
                "emby.current_server_api_key"
            }
            ersatzrs_core::repositories::RemoteMediaSourceKind::Plex => {
                return Err(ApplicationError::Validation {
                    field: "kind",
                    message: "Plex uses PIN authentication".to_owned(),
                });
            }
        };
        self.ports
            .admin_settings()
            .set_string(key, api_key.trim())
            .await?;
        Ok(id)
    }

    pub async fn delete_remote_source(&self, id: i64) -> Result<()> {
        if id <= 0 {
            return Err(ApplicationError::Validation {
                field: "source_id",
                message: "must be greater than zero".to_owned(),
            });
        }
        self.ports
            .media_sources()
            .delete_remote_source_admin(id)
            .await
            .map_err(Into::into)
    }

    pub async fn list_library_preferences(
        &self,
        kind: ersatzrs_core::repositories::RemoteMediaSourceKind,
        source_id: i64,
    ) -> Result<Vec<ersatzrs_core::repositories::LibraryPreferenceAdmin>> {
        self.ports
            .media_sources()
            .list_library_preferences_admin(kind, source_id)
            .await
            .map_err(Into::into)
    }

    pub async fn save_library_preferences(
        &self,
        kind: ersatzrs_core::repositories::RemoteMediaSourceKind,
        preferences: &[ersatzrs_core::repositories::LibraryPreferenceAdmin],
    ) -> Result<()> {
        if preferences.iter().any(|preference| preference.id <= 0) {
            return Err(ApplicationError::Validation {
                field: "library_id",
                message: "must be greater than zero".to_owned(),
            });
        }
        self.ports
            .media_sources()
            .save_library_preferences_admin(kind, preferences)
            .await
            .map_err(Into::into)
    }

    pub async fn list_path_replacements(
        &self,
        kind: ersatzrs_core::repositories::RemoteMediaSourceKind,
        source_id: i64,
    ) -> Result<Vec<ersatzrs_core::repositories::PathReplacementAdmin>> {
        self.ports
            .media_sources()
            .list_path_replacements_admin(kind, source_id)
            .await
            .map_err(Into::into)
    }

    pub async fn save_path_replacements(
        &self,
        kind: ersatzrs_core::repositories::RemoteMediaSourceKind,
        source_id: i64,
        paths: &[ersatzrs_core::repositories::PathReplacementAdmin],
    ) -> Result<()> {
        if source_id <= 0 {
            return Err(ApplicationError::Validation {
                field: "source_id",
                message: "must be greater than zero".to_owned(),
            });
        }
        self.ports
            .media_sources()
            .save_path_replacements_admin(kind, source_id, paths)
            .await
            .map_err(Into::into)
    }

    pub async fn sign_out_plex(&self) -> Result<()> {
        self.ports
            .media_sources()
            .delete_all_plex_sources_admin()
            .await
            .map_err(Into::into)
    }

    pub async fn save_jellyfin_connection(
        &self,
        connection: ersatzrs_core::repositories::RemoteConnectionAdmin,
        api_key: &str,
    ) -> Result<i64> {
        let address = connection.address.trim().trim_end_matches('/').to_owned();
        if address.is_empty() {
            return Err(ApplicationError::Validation {
                field: "address",
                message: "is required".to_owned(),
            });
        }
        if api_key.trim().is_empty() {
            return Err(ApplicationError::Validation {
                field: "api_key",
                message: "is required".to_owned(),
            });
        }
        let info = self
            .ports
            .media_source_runtime()
            .validate_jellyfin(&address, api_key.trim())
            .await
            .map_err(|error| ApplicationError::Unavailable {
                operation: "validate jellyfin connection",
                message: error.to_string(),
            })?;
        let id = self
            .save_remote_connection(
                ersatzrs_core::repositories::RemoteMediaSourceKind::Jellyfin,
                ersatzrs_core::repositories::RemoteConnectionAdmin {
                    source_id: connection.source_id,
                    address: address.clone(),
                },
                &info.server_name,
                &info.operating_system,
                api_key,
            )
            .await?;
        let _ = self
            .ports
            .media_source_runtime()
            .refresh_jellyfin_libraries(id, &address, api_key.trim())
            .await;
        Ok(id)
    }

    pub async fn save_emby_connection(
        &self,
        connection: ersatzrs_core::repositories::RemoteConnectionAdmin,
        api_key: &str,
    ) -> Result<i64> {
        self.save_remote_connection(
            ersatzrs_core::repositories::RemoteMediaSourceKind::Emby,
            connection,
            "Emby",
            "",
            api_key,
        )
        .await
    }

    pub async fn save_library_preferences_and_sync(
        &self,
        kind: ersatzrs_core::repositories::RemoteMediaSourceKind,
        preferences: &[ersatzrs_core::repositories::LibraryPreferenceAdmin],
    ) -> Result<()> {
        self.save_library_preferences(kind, preferences).await?;
        self.ports.media_source_runtime().request_library_sync(
            kind,
            preferences
                .iter()
                .filter(|preference| preference.should_sync)
                .map(|preference| preference.id)
                .collect(),
        );
        Ok(())
    }

    pub async fn has_plex_token(&self) -> Result<bool> {
        Ok(self
            .get_optional_setting("plex.current_server_auth_token")
            .await?
            .is_some_and(|value| !value.is_empty()))
    }

    pub async fn start_plex_pin_flow(&self) -> Result<(String, String)> {
        let pin = self
            .ports
            .media_source_runtime()
            .start_plex_pin()
            .await
            .map_err(|error| ApplicationError::Unavailable {
                operation: "start plex pin flow",
                message: error.to_string(),
            })?;
        self.ports
            .admin_settings()
            .set_string("plex.pending_pin_code", &pin.code)
            .await?;
        Ok((pin.id, pin.auth_url))
    }

    pub async fn check_plex_pin(&self, pin_id: &str) -> Result<Option<String>> {
        let pin_code = self.get_string_setting("plex.pending_pin_code", "").await?;
        let token = self
            .ports
            .media_source_runtime()
            .complete_plex_pin(pin_id, &pin_code)
            .await
            .map_err(|error| ApplicationError::Unavailable {
                operation: "complete plex pin flow",
                message: error.to_string(),
            })?;
        if let Some(token) = &token {
            self.ports
                .admin_settings()
                .set_string("plex.user_auth_token", token)
                .await?;
        }
        Ok(token)
    }

    pub async fn delete_media_items(&self, ids: &[i64]) -> Result<u64> {
        if ids.iter().any(|id| *id <= 0) {
            return Err(ApplicationError::Validation {
                field: "media_item_id",
                message: "must be greater than zero".to_owned(),
            });
        }
        self.ports
            .media_items()
            .delete_admin(ids)
            .await
            .map_err(Into::into)
    }

    pub async fn empty_media_trash(&self) -> Result<u64> {
        self.ports
            .media_items()
            .empty_trash_admin()
            .await
            .map_err(Into::into)
    }

    pub async fn get_movie_detail(
        &self,
        id: i64,
    ) -> Result<Option<ersatzrs_core::repositories::AdminMovieDetail>> {
        if id <= 0 {
            return Err(ApplicationError::Validation {
                field: "movie_id",
                message: "must be greater than zero".to_owned(),
            });
        }
        self.ports
            .admin_media()
            .get_movie_detail_admin(id)
            .await
            .map_err(Into::into)
    }

    pub async fn get_show_detail(
        &self,
        id: i64,
    ) -> Result<Option<ersatzrs_core::repositories::AdminShowDetail>> {
        if id <= 0 {
            return Err(ApplicationError::Validation {
                field: "show_id",
                message: "must be greater than zero".to_owned(),
            });
        }
        self.ports
            .admin_media()
            .get_show_detail_admin(id)
            .await
            .map_err(Into::into)
    }

    pub async fn get_artist_detail(
        &self,
        id: i64,
    ) -> Result<Option<ersatzrs_core::repositories::AdminArtistDetail>> {
        if id <= 0 {
            return Err(ApplicationError::Validation {
                field: "artist_id",
                message: "must be greater than zero".to_owned(),
            });
        }
        self.ports
            .admin_media()
            .get_artist_detail_admin(id)
            .await
            .map_err(Into::into)
    }

    pub async fn get_other_video_detail(
        &self,
        id: i64,
    ) -> Result<Option<ersatzrs_core::repositories::AdminOtherVideoDetail>> {
        if id <= 0 {
            return Err(ApplicationError::Validation {
                field: "other_video_id",
                message: "must be greater than zero".to_owned(),
            });
        }
        self.ports
            .admin_media()
            .get_other_video_detail_admin(id)
            .await
            .map_err(Into::into)
    }

    pub async fn get_faceted_media_detail(
        &self,
        id: i64,
        kind: ersatzrs_core::repositories::AdminFacetedMediaKind,
    ) -> Result<Option<ersatzrs_core::repositories::AdminFacetedMediaDetail>> {
        if id <= 0 {
            return Err(ApplicationError::Validation {
                field: "media_item_id",
                message: "must be greater than zero".to_owned(),
            });
        }
        self.ports
            .admin_media()
            .get_faceted_media_detail_admin(id, kind)
            .await
            .map_err(Into::into)
    }

    pub async fn get_remote_stream_detail(
        &self,
        id: i64,
    ) -> Result<Option<ersatzrs_core::repositories::AdminRemoteStreamDetail>> {
        if id <= 0 {
            return Err(ApplicationError::Validation {
                field: "remote_stream_id",
                message: "must be greater than zero".to_owned(),
            });
        }
        self.ports
            .admin_media()
            .get_remote_stream_detail_admin(id)
            .await
            .map_err(Into::into)
    }

    pub async fn get_media_item_info(
        &self,
        id: i64,
    ) -> Result<Option<ersatzrs_core::repositories::AdminMediaItemInfo>> {
        if id <= 0 {
            return Err(ApplicationError::Validation {
                field: "media_item_id",
                message: "must be greater than zero".to_owned(),
            });
        }
        self.ports
            .admin_media()
            .get_media_item_info_admin(id)
            .await
            .map_err(Into::into)
    }

    pub async fn list_media_extras(
        &self,
        id: i64,
    ) -> Result<Vec<ersatzrs_core::repositories::AdminMediaCard>> {
        self.ports
            .admin_media()
            .list_extras_admin(id)
            .await
            .map_err(Into::into)
    }

    pub async fn list_playout_detail_items(
        &self,
        id: i64,
        show_filler: bool,
        page: u32,
        page_size: u32,
    ) -> Result<Vec<queries::PlayoutDetailReadModel>> {
        queries::list_playout_detail_items(id, show_filler, page, page_size, self.ports.playouts())
            .await
    }

    pub async fn count_playout_detail_items(&self, id: i64, show_filler: bool) -> Result<u64> {
        queries::count_playout_detail_items(id, show_filler, self.ports.playouts()).await
    }

    pub async fn list_playout_create_channels(
        &self,
    ) -> Result<Vec<ersatzrs_core::repositories::PlayoutCreateChannelAdmin>> {
        self.ports
            .playouts()
            .list_create_channels_admin()
            .await
            .map_err(Into::into)
    }

    pub async fn list_playout_create_schedules(
        &self,
    ) -> Result<Vec<ersatzrs_core::repositories::PlayoutCreateScheduleAdmin>> {
        self.ports
            .playouts()
            .list_create_schedules_admin()
            .await
            .map_err(Into::into)
    }

    pub async fn create_playout(&self, command: commands::CreatePlayout) -> Result<i64> {
        commands::create_playout(
            command,
            self.ports.playouts(),
            self.ports.playout_builds(),
            &self.operations,
        )
        .await
    }

    pub async fn delete_playout(&self, command: commands::DeletePlayout) -> Result<()> {
        commands::delete_playout(command, self.ports.playouts()).await
    }

    pub async fn reset_all_playouts(&self) -> Result<commands::PlayoutBuildBatchOutcome> {
        commands::reset_all_playouts(
            self.ports.playouts(),
            self.ports.playout_builds(),
            &self.operations,
        )
        .await
    }

    pub async fn save_playout_daily_reset(
        &self,
        command: commands::SavePlayoutDailyReset,
    ) -> Result<()> {
        commands::save_playout_daily_reset(command, self.ports.playouts()).await
    }

    pub async fn save_playout_schedule_file(
        &self,
        command: commands::SavePlayoutScheduleFile,
    ) -> Result<()> {
        commands::save_playout_schedule_file(
            command,
            self.ports.playouts(),
            self.ports.playout_builds(),
            &self.operations,
        )
        .await
    }

    pub async fn get_playout_kind_editor(
        &self,
        id: i64,
        kind: i32,
    ) -> Result<Option<ersatzrs_core::repositories::PlayoutKindEditorAdmin>> {
        queries::get_playout_kind_editor(id, kind, self.ports.playouts()).await
    }

    pub async fn save_playout_default_deco(
        &self,
        command: commands::SavePlayoutDefaultDeco,
    ) -> Result<()> {
        commands::save_playout_default_deco(command, self.ports.playouts()).await
    }

    pub async fn get_playout_alternate_schedules(
        &self,
        id: i64,
    ) -> Result<Option<ersatzrs_core::repositories::PlayoutAlternateSchedulesAdmin>> {
        queries::get_playout_alternate_schedules(id, self.ports.playouts()).await
    }

    pub async fn save_playout_alternate_schedules(
        &self,
        command: commands::SavePlayoutAlternateSchedules,
    ) -> Result<()> {
        commands::save_playout_alternate_schedules(
            command,
            self.ports.playouts(),
            self.ports.playout_builds(),
            &self.operations,
        )
        .await
    }

    pub async fn get_playout_templates(
        &self,
        id: i64,
    ) -> Result<Option<ersatzrs_core::repositories::PlayoutTemplatesAdmin>> {
        queries::get_playout_templates(id, self.ports.playouts()).await
    }

    pub async fn save_playout_templates(
        &self,
        command: commands::SavePlayoutTemplates,
    ) -> Result<()> {
        commands::save_playout_templates(command, self.ports.playouts()).await
    }

    pub async fn erase_playout_items(&self, command: commands::DeletePlayout) -> Result<()> {
        commands::erase_playout_items(command, self.ports.playouts()).await
    }

    pub async fn get_sequential_playout(
        &self,
        id: i64,
    ) -> Result<Option<ersatzrs_core::repositories::SequentialPlayoutAdmin>> {
        queries::get_sequential_playout(id, self.ports.playouts()).await
    }

    pub async fn erase_playout_items_and_history(
        &self,
        command: commands::DeletePlayout,
    ) -> Result<()> {
        commands::erase_playout_items_and_history(command, self.ports.playouts()).await
    }

    pub async fn process_scheduling_context(&self, serialized: &str) -> Result<String> {
        queries::process_scheduling_context(serialized, self.ports.playouts()).await
    }

    pub async fn get_playout_show_filler(&self) -> Result<bool> {
        self.ports
            .admin_settings()
            .get_optional_string("pages.playouts.detail_show_filler")
            .await
            .map(|value| value.is_some_and(|value| value.trim().eq_ignore_ascii_case("true")))
            .map_err(Into::into)
    }

    pub async fn set_playout_show_filler(&self, show_filler: bool) -> Result<()> {
        self.ports
            .admin_settings()
            .set_string(
                "pages.playouts.detail_show_filler",
                if show_filler { "true" } else { "false" },
            )
            .await
            .map_err(Into::into)
    }

    /// Admit one playout build through the single application-owned queue
    /// boundary.
    pub fn request_playout_build(
        &self,
        command: commands::RequestPlayoutBuild,
    ) -> commands::RequestPlayoutBuildOutcome {
        commands::request_playout_build(command, self.ports.playout_builds(), &self.operations)
    }

    /// Preview an unsaved block through the real block scheduling builder.
    ///
    /// # Errors
    ///
    /// Returns a typed validation, repository, scheduling, or enrichment
    /// failure from the preview vertical slice.
    pub async fn preview_block_playout(
        &self,
        query: queries::PreviewBlockPlayout,
    ) -> Result<Vec<queries::PreviewBlockPlayoutRow>> {
        queries::preview_block_playout(
            query,
            self.ports.block_previews(),
            self.ports.block_preview_builder(),
        )
        .await
    }

    /// Preview an unsaved playlist through the real classic scheduler.
    pub async fn preview_playlist_playout(
        &self,
        query: queries::PreviewPlaylistPlayout,
    ) -> Result<Vec<queries::PreviewPlaylistPlayoutRow>> {
        queries::preview_playlist_playout(
            query,
            self.ports.block_previews(),
            self.ports.block_preview_builder(),
        )
        .await
    }

    /// Validate and admit one full library scan.
    pub async fn request_library_scan(
        &self,
        command: commands::RequestLibraryScan,
    ) -> commands::RequestScanOutcome {
        commands::request_library_scan(
            command,
            self.ports.libraries(),
            self.ports.managed_library_operations(),
            self.ports.scan_work(),
            &self.operations,
        )
        .await
    }

    /// Validate and admit one targeted show scan.
    pub async fn request_show_scan(
        &self,
        command: commands::RequestShowScan,
    ) -> commands::RequestScanOutcome {
        commands::request_show_scan(
            command,
            self.ports.libraries(),
            self.ports.managed_library_operations(),
            self.ports.scan_work(),
            &self.operations,
        )
        .await
    }

    /// Execute one accepted queue token through the injected native runtime.
    pub async fn execute_scan_work(
        &self,
        work: commands::ScanWorkRequest,
    ) -> commands::ExecuteScanOutcome {
        execute_scan_work_with_runtime(
            work,
            self.ports.scan_runtime(),
            self.ports.playout_builds(),
            &self.operations,
        )
        .await
    }

    /// Claim the oldest eligible managed-library operation across restarts.
    pub async fn claim_managed_library_operation(
        &self,
    ) -> Result<Option<ersatzrs_core::repositories::ManagedLibraryOperationClaim>> {
        self.ports
            .managed_library_operations()
            .claim_next(chrono::Utc::now())
            .await
            .map_err(Into::into)
    }

    /// Extend the durable lease while external scan or filesystem work runs.
    pub async fn heartbeat_managed_library_operation(
        &self,
        claim: &ersatzrs_core::repositories::ManagedLibraryOperationClaim,
    ) -> Result<bool> {
        self.ports
            .managed_library_operations()
            .heartbeat(claim, chrono::Utc::now())
            .await
            .map_err(Into::into)
    }

    /// Execute one fenced durable operation through the shared native runtime.
    pub async fn execute_managed_library_operation(
        &self,
        claim: ersatzrs_core::repositories::ManagedLibraryOperationClaim,
    ) -> commands::ExecuteManagedLibraryOperationOutcome {
        execute_managed_library_operation(self, &claim).await
    }

    /// Persist or identify the exact durable Local scans startup must finish
    /// before recovering scan-triggered playout refreshes.
    pub async fn prepare_scan_playout_recovery(&self) -> Result<Vec<ersatzrs_domain::EntityId>> {
        let interrupted = self.interrupted_scan_library_ids().await?;
        let mut replay_operation_ids = std::collections::BTreeSet::new();
        for library_id in &interrupted {
            let scan = ersatzrs_core::repositories::ManagedLibraryScanSpec {
                provider: ersatzrs_domain::MediaSourceKind::Local,
                show_id: None,
                deep_scan: true,
            };
            let operation = if let Some(operation) = self
                .ports
                .managed_library_operations()
                .enqueue_scan(
                    ersatzrs_domain::ManagedLibraryOperationKind::StartupReplayScan,
                    *library_id,
                    scan,
                    true,
                )
                .await?
            {
                operation
            } else if let Some(operation) = self
                .ports
                .managed_library_operations()
                .active_scan_operation(*library_id)
                .await?
            {
                operation
            } else if !self
                .interrupted_scan_library_ids()
                .await?
                .contains(library_id)
            {
                continue;
            } else {
                return Err(ApplicationError::Unavailable {
                    operation: "identify interrupted library scan replay",
                    message: format!(
                        "library {library_id} remained interrupted without an active durable scan"
                    ),
                });
            };
            replay_operation_ids.insert(operation.id);
        }
        Ok(replay_operation_ids.into_iter().collect())
    }

    /// Load one durable operation while startup waits for its terminal state.
    pub async fn managed_library_operation(
        &self,
        operation_id: ersatzrs_domain::EntityId,
    ) -> Result<Option<ersatzrs_core::repositories::ManagedLibraryOperation>> {
        self.ports
            .managed_library_operations()
            .operation(operation_id)
            .await
            .map_err(Into::into)
    }

    /// Claim one exact durable operation during the startup recovery barrier.
    pub async fn claim_managed_library_operation_by_id(
        &self,
        operation_id: ersatzrs_domain::EntityId,
    ) -> Result<Option<ersatzrs_core::repositories::ManagedLibraryOperationClaim>> {
        self.ports
            .managed_library_operations()
            .claim_operation(operation_id, chrono::Utc::now())
            .await
            .map_err(Into::into)
    }

    /// Recover durable playout refreshes after startup replay scans are
    /// terminal. Any remaining interrupted marker rejects preflight.
    pub async fn recover_scan_playout_refreshes(&self) -> Result<usize> {
        let unresolved = self.interrupted_scan_library_ids().await?;
        if !unresolved.is_empty() {
            return Err(ApplicationError::Unavailable {
                operation: "resolve interrupted library scan replay",
                message: format!("durable replay ended without resolving libraries {unresolved:?}"),
            });
        }
        let report = self
            .ports
            .scan_runtime()
            .recover_playout_refreshes()
            .await
            .map_err(|error| ApplicationError::Unavailable {
                operation: "recover scan-triggered playout refreshes",
                message: error.to_string(),
            })?;
        let count = report.playout_refreshes.len();
        commands::require_scan_playout_refreshes(
            report,
            self.ports.playout_builds(),
            &self.operations,
        )?;
        Ok(count)
    }

    async fn interrupted_scan_library_ids(&self) -> Result<Vec<ersatzrs_domain::EntityId>> {
        self.ports
            .scan_runtime()
            .interrupted_scan_library_ids()
            .await
            .map_err(|error| ApplicationError::Unavailable {
                operation: "load interrupted library scans",
                message: error.to_string(),
            })
    }

    /// Re-admit pending durable scan-triggered Refresh intents during the
    /// scheduler cadence without replaying currently building work.
    pub async fn retry_pending_scan_playout_refreshes(&self) -> Result<usize> {
        let report = self
            .ports
            .scan_runtime()
            .pending_playout_refreshes()
            .await
            .map_err(|error| ApplicationError::Unavailable {
                operation: "retry pending scan-triggered playout refreshes",
                message: error.to_string(),
            })?;
        let count = report.playout_refreshes.len();
        commands::require_scan_playout_refreshes(
            report,
            self.ports.playout_builds(),
            &self.operations,
        )?;
        Ok(count)
    }

    /// Mark the native scan worker available for new commands.
    pub fn scan_worker_started(&self) {
        self.operations.scan_activity().worker_started();
    }

    /// Fail outstanding work and reject admission after worker shutdown.
    pub fn scan_worker_stopped(&self, message: impl Into<String>) {
        self.operations.scan_activity().worker_stopped(message);
    }

    /// Query every retained scan operation.
    #[must_use]
    pub fn scan_operations(&self) -> Vec<queries::ScanOperationSnapshot> {
        self.operations.scan_activity().snapshots()
    }

    /// Query distinct libraries with queued or running scan work.
    #[must_use]
    pub fn active_library_scans(&self) -> Vec<ersatzrs_domain::EntityId> {
        self.operations.scan_activity().active_library_ids()
    }

    /// Query active fractional progress by library.
    #[must_use]
    pub fn scan_progress(&self) -> Vec<(ersatzrs_domain::EntityId, f32)> {
        self.operations.scan_activity().progress_by_library()
    }

    /// Query one library's aggregate active or latest terminal state.
    #[must_use]
    pub fn library_scan_status(
        &self,
        query: queries::GetLibraryScanStatus,
    ) -> queries::LibraryScanStatus {
        self.operations.scan_activity().library_status(query)
    }
}

#[allow(clippy::too_many_lines)]
async fn execute_managed_library_operation(
    application: &ApplicationServices,
    claim: &ersatzrs_core::repositories::ManagedLibraryOperationClaim,
) -> commands::ExecuteManagedLibraryOperationOutcome {
    use commands::ExecuteManagedLibraryOperationOutcome as Outcome;
    use ersatzrs_core::repositories::AddonMediaListPublicationContinuation;
    use ersatzrs_domain::{
        AddonMediaListPublicationOutcome, AddonMediaListPublicationPhase,
        ManagedLibraryOperationState,
    };

    match managed_operation_execution_path(claim.operation.kind) {
        ManagedOperationExecutionPath::ResetReload => {
            return execute_addon_media_list_reset_reload(application, claim).await;
        }
        ManagedOperationExecutionPath::Reconcile => {
            return execute_addon_media_list_reconcile(application, claim).await;
        }
        ManagedOperationExecutionPath::Scan => {
            return execute_durable_scan(application, claim).await
        }
        ManagedOperationExecutionPath::Publication => {}
    }

    for _ in 0..24 {
        let job = match application
            .ports
            .managed_library_operations()
            .publication_job(claim.operation.id)
            .await
        {
            Ok(Some(job)) => job,
            Ok(None) => {
                let _ = application
                    .ports
                    .managed_library_operations()
                    .fail_operation(
                        claim,
                        "publication_job_missing",
                        "publication job is missing",
                    )
                    .await;
                return Outcome::Failed {
                    message: "publication job is missing".to_owned(),
                };
            }
            Err(error) => {
                return Outcome::Failed {
                    message: error.to_string(),
                };
            }
        };

        match job.phase {
            AddonMediaListPublicationPhase::AwaitingLease => {
                if !transition_publication(
                    application,
                    claim,
                    job.phase,
                    AddonMediaListPublicationPhase::Staging,
                )
                .await
                {
                    return Outcome::LeaseLost;
                }
            }
            AddonMediaListPublicationPhase::Staging => {
                if let Some(source_revision) = job.source_published_revision {
                    let restored = application
                        .ports
                        .addon_media_list_lifecycle()
                        .materialize_revision_as_draft(
                            &ersatzrs_core::repositories::AddonMediaListRevisionRestoreFence {
                                list_id: job.list_id,
                                source_revision,
                                target_draft_revision: job.saved_revision,
                                operation_id: job.operation_id,
                                generation: job.generation,
                                owner_token: claim.operation.owner_token.clone(),
                                lease_owner: claim.lease_owner.clone(),
                                active_at: chrono::Utc::now(),
                            },
                        )
                        .await;
                    if let Err(error) = restored {
                        let message = error.to_string();
                        let _ = application
                            .ports
                            .managed_library_operations()
                            .fail_publication(
                                claim,
                                "publication_restore_materialization_failed",
                                "retained revision restore materialization failed",
                                None,
                            )
                            .await;
                        return Outcome::Failed { message };
                    }
                }
                let staged = application
                    .ports
                    .addon_media_lists()
                    .stage_publication_revision(
                        job.list_id,
                        job.saved_revision,
                        job.operation_id,
                        job.generation,
                        &claim.operation.owner_token,
                        &claim.lease_owner,
                    )
                    .await;
                if let Err(error) = staged {
                    let message = error.to_string();
                    let cause = error.publication_failure_cause();
                    let _ = application
                        .ports
                        .managed_library_operations()
                        .fail_publication(
                            claim,
                            "publication_staging_failed",
                            "publication staging failed",
                            cause,
                        )
                        .await;
                    return Outcome::Failed { message };
                }
                if !transition_publication(
                    application,
                    claim,
                    job.phase,
                    AddonMediaListPublicationPhase::Scanning,
                )
                .await
                {
                    return Outcome::LeaseLost;
                }
            }
            AddonMediaListPublicationPhase::Scanning => {
                let child = match application
                    .ports
                    .managed_library_operations()
                    .ensure_publication_child_scan(claim, job.list_id)
                    .await
                {
                    Ok(child) => child,
                    Err(error) => {
                        return Outcome::Failed {
                            message: error.to_string(),
                        };
                    }
                };
                if child.state != ManagedLibraryOperationState::Completed {
                    let child_claim = ersatzrs_core::repositories::ManagedLibraryOperationClaim {
                        operation: child,
                        lease_owner: claim.lease_owner.clone(),
                    };
                    match execute_durable_scan(application, &child_claim).await {
                        Outcome::Completed => {}
                        Outcome::Failed { message } => {
                            let recorded = application
                                .ports
                                .managed_library_operations()
                                .record_publication_failure(
                                    claim,
                                    job.phase,
                                    "publication_scan_failed",
                                    "publication child scan failed",
                                    None,
                                )
                                .await
                                .unwrap_or(false);
                            if !recorded {
                                return Outcome::LeaseLost;
                            }
                            tracing::warn!(
                                operation_id = claim.operation.id,
                                error = %message,
                                "add-on publication child scan failed; recovery will restore the prior revision"
                            );
                            continue;
                        }
                        Outcome::Deferred | Outcome::LeaseLost => return Outcome::LeaseLost,
                    }
                }
                if !transition_publication(
                    application,
                    claim,
                    job.phase,
                    AddonMediaListPublicationPhase::Committing,
                )
                .await
                {
                    return Outcome::LeaseLost;
                }
            }
            AddonMediaListPublicationPhase::Committing => {
                let continuation = match application
                    .ports
                    .addon_media_lists()
                    .commit_staged_publication_revision(
                        job.list_id,
                        job.target_published_revision,
                        job.operation_id,
                        job.generation,
                        &claim.operation.owner_token,
                        &claim.lease_owner,
                    )
                    .await
                {
                    Ok(continuation) => continuation,
                    Err(error) => {
                        let recorded = application
                            .ports
                            .managed_library_operations()
                            .record_publication_failure(
                                claim,
                                job.phase,
                                "publication_commit_failed",
                                "publication commit failed",
                                error.publication_failure_cause(),
                            )
                            .await
                            .unwrap_or(false);
                        if !recorded {
                            return Outcome::LeaseLost;
                        }
                        tracing::warn!(
                            operation_id = claim.operation.id,
                            error = %error,
                            "add-on publication commit failed; recovery will restore the prior revision"
                        );
                        continue;
                    }
                };
                if let Ok(report) = application
                    .ports
                    .scan_runtime()
                    .pending_playout_refreshes()
                    .await
                {
                    if let Err(error) = commands::require_scan_playout_refreshes(
                        report,
                        application.ports.playout_builds(),
                        &application.operations,
                    ) {
                        tracing::warn!(
                            operation_id = claim.operation.id,
                            error = %error,
                            "published add-on list has deferred playout refresh admission"
                        );
                    }
                }
                if continuation == AddonMediaListPublicationContinuation::CompletedLegacy {
                    return complete_publication_operation(
                        application,
                        claim,
                        AddonMediaListPublicationOutcome::Published,
                    )
                    .await;
                }
                match application
                    .ports
                    .managed_library_operations()
                    .begin_publication_verification(
                        claim,
                        job.phase,
                        AddonMediaListPublicationOutcome::Published,
                    )
                    .await
                {
                    Ok(true) => {}
                    Ok(false) => return Outcome::LeaseLost,
                    Err(error) => {
                        return Outcome::Failed {
                            message: error.to_string(),
                        };
                    }
                }
            }
            AddonMediaListPublicationPhase::RestoringFiles => {
                let original = job
                    .original_failure_diagnostic
                    .as_deref()
                    .unwrap_or("publication failed");
                if let Err(error) = application
                    .ports
                    .addon_media_lists()
                    .restore_staged_publication_revision(
                        job.list_id,
                        original,
                        job.operation_id,
                        job.generation,
                        &claim.operation.owner_token,
                        &claim.lease_owner,
                    )
                    .await
                {
                    let diagnostic = error.to_string();
                    return schedule_publication_retry(
                        application,
                        claim,
                        &diagnostic,
                        error.publication_failure_cause(),
                    )
                    .await;
                }
                if !transition_publication(
                    application,
                    claim,
                    job.phase,
                    AddonMediaListPublicationPhase::RestoringProjection,
                )
                .await
                {
                    return Outcome::LeaseLost;
                }
            }
            AddonMediaListPublicationPhase::RestoringProjection => {
                let continuation = match application
                    .ports
                    .addon_media_lists()
                    .reconcile_restored_publication_revision(
                        job.list_id,
                        job.operation_id,
                        job.generation,
                        &claim.operation.owner_token,
                        &claim.lease_owner,
                    )
                    .await
                {
                    Ok(continuation) => continuation,
                    Err(error) => {
                        let diagnostic = error.to_string();
                        return schedule_publication_retry(
                            application,
                            claim,
                            &diagnostic,
                            error.publication_failure_cause(),
                        )
                        .await;
                    }
                };
                if continuation == AddonMediaListPublicationContinuation::CompletedLegacy {
                    return complete_publication_operation(
                        application,
                        claim,
                        AddonMediaListPublicationOutcome::RestoredAfterFailure,
                    )
                    .await;
                }
                if !transition_publication(
                    application,
                    claim,
                    job.phase,
                    AddonMediaListPublicationPhase::Reconciling,
                )
                .await
                {
                    return Outcome::LeaseLost;
                }
            }
            AddonMediaListPublicationPhase::Reconciling => {
                // Re-enter the idempotent repository step at this durable
                // boundary. AMM-J-01 could exit after reconciliation cleared
                // the journal but before the operation was completed; the
                // typed continuation distinguishes that terminal legacy state
                // from an AMM-J-02 journal that still requires verification.
                let continuation = match application
                    .ports
                    .addon_media_lists()
                    .reconcile_restored_publication_revision(
                        job.list_id,
                        job.operation_id,
                        job.generation,
                        &claim.operation.owner_token,
                        &claim.lease_owner,
                    )
                    .await
                {
                    Ok(continuation) => continuation,
                    Err(error) => {
                        let diagnostic = error.to_string();
                        return schedule_publication_retry(
                            application,
                            claim,
                            &diagnostic,
                            error.publication_failure_cause(),
                        )
                        .await;
                    }
                };
                if continuation == AddonMediaListPublicationContinuation::CompletedLegacy {
                    return complete_publication_operation(
                        application,
                        claim,
                        AddonMediaListPublicationOutcome::RestoredAfterFailure,
                    )
                    .await;
                }
                match application
                    .ports
                    .managed_library_operations()
                    .begin_publication_verification(
                        claim,
                        job.phase,
                        AddonMediaListPublicationOutcome::RestoredAfterFailure,
                    )
                    .await
                {
                    Ok(true) => {}
                    Ok(false) => return Outcome::LeaseLost,
                    Err(error) => {
                        return Outcome::Failed {
                            message: error.to_string(),
                        };
                    }
                }
            }
            AddonMediaListPublicationPhase::Verifying => {
                let child = match application
                    .ports
                    .managed_library_operations()
                    .ensure_publication_verification_scan(claim, job.list_id)
                    .await
                {
                    Ok(child) => child,
                    Err(error) => {
                        let diagnostic = error.to_string();
                        return schedule_publication_retry(
                            application,
                            claim,
                            &diagnostic,
                            error.publication_failure_cause(),
                        )
                        .await;
                    }
                };
                if child.state != ManagedLibraryOperationState::Completed {
                    let child_claim = ersatzrs_core::repositories::ManagedLibraryOperationClaim {
                        operation: child,
                        lease_owner: claim.lease_owner.clone(),
                    };
                    match execute_durable_scan(application, &child_claim).await {
                        Outcome::Completed => {}
                        Outcome::Failed { message } => {
                            tracing::warn!(
                                operation_id = claim.operation.id,
                                error = %message,
                                "add-on publication verification scan failed; recovery will retry"
                            );
                            return schedule_publication_retry(application, claim, &message, None)
                                .await;
                        }
                        Outcome::Deferred | Outcome::LeaseLost => return Outcome::LeaseLost,
                    }
                }
                let Some(outcome) = job.outcome else {
                    return Outcome::Failed {
                        message: "publication verification has no selected revision outcome"
                            .to_owned(),
                    };
                };
                if let Err(error) = application
                    .ports
                    .addon_media_lists()
                    .verify_staged_publication_revision(
                        job.list_id,
                        outcome,
                        job.operation_id,
                        job.generation,
                        &claim.operation.owner_token,
                        &claim.lease_owner,
                    )
                    .await
                {
                    let diagnostic = error.to_string();
                    return schedule_publication_retry(
                        application,
                        claim,
                        &diagnostic,
                        error.publication_failure_cause(),
                    )
                    .await;
                }
                if !transition_publication(
                    application,
                    claim,
                    job.phase,
                    AddonMediaListPublicationPhase::CleaningUp,
                )
                .await
                {
                    return Outcome::LeaseLost;
                }
            }
            AddonMediaListPublicationPhase::CleaningUp => {
                let Some(outcome) = job.outcome else {
                    return Outcome::Failed {
                        message: "publication cleanup has no selected revision outcome".to_owned(),
                    };
                };
                if let Err(error) = application
                    .ports
                    .addon_media_lists()
                    .finalize_staged_publication_revision(
                        job.list_id,
                        outcome,
                        job.operation_id,
                        job.generation,
                        &claim.operation.owner_token,
                        &claim.lease_owner,
                    )
                    .await
                {
                    let diagnostic = error.to_string();
                    return schedule_publication_retry(
                        application,
                        claim,
                        &diagnostic,
                        error.publication_failure_cause(),
                    )
                    .await;
                }
                return match application
                    .ports
                    .managed_library_operations()
                    .complete_publication(claim, outcome)
                    .await
                {
                    Ok(true) => Outcome::Completed,
                    Ok(false) => Outcome::LeaseLost,
                    Err(error) => Outcome::Failed {
                        message: error.to_string(),
                    },
                };
            }
            AddonMediaListPublicationPhase::Complete => {
                return match application
                    .ports
                    .managed_library_operations()
                    .complete_publication(
                        claim,
                        job.outcome
                            .unwrap_or(AddonMediaListPublicationOutcome::Published),
                    )
                    .await
                {
                    Ok(true) => Outcome::Completed,
                    Ok(false) => Outcome::LeaseLost,
                    Err(error) => Outcome::Failed {
                        message: error.to_string(),
                    },
                };
            }
        }
    }

    Outcome::Failed {
        message: "publication exceeded its bounded phase-advance loop".to_owned(),
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum ManagedOperationExecutionPath {
    Scan,
    Publication,
    ResetReload,
    Reconcile,
}

const fn managed_operation_execution_path(
    kind: ersatzrs_domain::ManagedLibraryOperationKind,
) -> ManagedOperationExecutionPath {
    match kind {
        ersatzrs_domain::ManagedLibraryOperationKind::Reset => {
            ManagedOperationExecutionPath::ResetReload
        }
        ersatzrs_domain::ManagedLibraryOperationKind::Reconcile => {
            ManagedOperationExecutionPath::Reconcile
        }
        ersatzrs_domain::ManagedLibraryOperationKind::Publication
        | ersatzrs_domain::ManagedLibraryOperationKind::Recovery => {
            ManagedOperationExecutionPath::Publication
        }
        _ => ManagedOperationExecutionPath::Scan,
    }
}

async fn complete_publication_operation(
    application: &ApplicationServices,
    claim: &ersatzrs_core::repositories::ManagedLibraryOperationClaim,
    outcome: ersatzrs_domain::AddonMediaListPublicationOutcome,
) -> commands::ExecuteManagedLibraryOperationOutcome {
    use commands::ExecuteManagedLibraryOperationOutcome as Outcome;

    match application
        .ports
        .managed_library_operations()
        .complete_publication(claim, outcome)
        .await
    {
        Ok(true) => Outcome::Completed,
        Ok(false) => Outcome::LeaseLost,
        Err(error) => Outcome::Failed {
            message: error.to_string(),
        },
    }
}

async fn transition_publication(
    application: &ApplicationServices,
    claim: &ersatzrs_core::repositories::ManagedLibraryOperationClaim,
    expected: ersatzrs_domain::AddonMediaListPublicationPhase,
    next: ersatzrs_domain::AddonMediaListPublicationPhase,
) -> bool {
    application
        .ports
        .managed_library_operations()
        .transition_publication(claim, expected, next)
        .await
        .unwrap_or(false)
}

async fn schedule_publication_retry(
    application: &ApplicationServices,
    claim: &ersatzrs_core::repositories::ManagedLibraryOperationClaim,
    diagnostic: &str,
    cause: Option<ersatzrs_core::repositories::AddonMediaListPublicationFailureCause>,
) -> commands::ExecuteManagedLibraryOperationOutcome {
    use commands::ExecuteManagedLibraryOperationOutcome as Outcome;

    let attempt = u32::try_from(claim.operation.attempt_count.max(1)).unwrap_or(1);
    let base = 1_i64
        .checked_shl(attempt.saturating_sub(1).min(6))
        .unwrap_or(50)
        .min(50);
    let jitter_window = (base / 5).max(1);
    let operation = claim.operation.id.unsigned_abs();
    let jitter = i64::try_from(
        (operation + u64::from(attempt)) % u64::try_from(jitter_window + 1).unwrap_or(2),
    )
    .unwrap_or(0);
    let next = chrono::Utc::now() + chrono::Duration::seconds((base + jitter).min(60));
    match application
        .ports
        .managed_library_operations()
        .schedule_publication_recovery_retry(
            claim,
            "publication recovery will retry after a transient failure",
            cause,
            next,
        )
        .await
    {
        Ok(true) => {
            tracing::warn!(
                operation_id = claim.operation.id,
                error = %diagnostic,
                retry_at = %next,
                "add-on publication recovery deferred"
            );
            Outcome::Deferred
        }
        Ok(false) => Outcome::LeaseLost,
        Err(error) => Outcome::Failed {
            message: error.to_string(),
        },
    }
}

#[allow(clippy::too_many_lines)] // Keep the durable phase transitions visible as one state machine.
async fn execute_addon_media_list_reset_reload(
    application: &ApplicationServices,
    claim: &ersatzrs_core::repositories::ManagedLibraryOperationClaim,
) -> commands::ExecuteManagedLibraryOperationOutcome {
    use commands::ExecuteManagedLibraryOperationOutcome as Outcome;
    use ersatzrs_domain::{AddonMediaListLifecyclePhase as Phase, ManagedLibraryOperationState};

    for _ in 0..16 {
        let job = match application
            .ports
            .managed_library_operations()
            .reset_reload_job(claim.operation.id)
            .await
        {
            Ok(Some(job)) => job,
            Ok(None) => {
                let message = "reset/reload lifecycle job is missing".to_owned();
                let _ = application
                    .ports
                    .managed_library_operations()
                    .fail_operation(claim, "reset_job_missing", &message)
                    .await;
                return Outcome::Failed { message };
            }
            Err(error) => {
                return Outcome::Failed {
                    message: error.to_string(),
                }
            }
        };
        let next = match job.phase {
            Phase::AwaitingLease => Some(Phase::Quiescing),
            Phase::Quiescing => Some(Phase::UnpublishingFiles),
            Phase::UnpublishingFiles => Some(Phase::UnpublishingProjection),
            Phase::UnpublishingProjection => {
                if let Err(error) = application
                    .ports
                    .managed_library_operations()
                    .apply_reset_reload_phase(claim, &job, job.phase)
                    .await
                {
                    return fail_reset_operation(application, claim, error.to_string()).await;
                }
                let child = match application
                    .ports
                    .managed_library_operations()
                    .ensure_reset_reload_child_scan(claim, job.list_id)
                    .await
                {
                    Ok(child) => child,
                    Err(error) => {
                        return fail_reset_operation(application, claim, error.to_string()).await;
                    }
                };
                if child.state != ManagedLibraryOperationState::Completed {
                    let child_claim = ersatzrs_core::repositories::ManagedLibraryOperationClaim {
                        operation: child,
                        lease_owner: claim.lease_owner.clone(),
                    };
                    match execute_durable_scan(application, &child_claim).await {
                        Outcome::Completed => {}
                        Outcome::Deferred | Outcome::LeaseLost => return Outcome::LeaseLost,
                        Outcome::Failed { message } => {
                            return fail_reset_operation(application, claim, message).await;
                        }
                    }
                }
                Some(Phase::ClearingProviderState)
            }
            Phase::ClearingProviderState => Some(Phase::ClearingCheckpoint),
            Phase::ClearingCheckpoint => Some(Phase::VerifyingEmpty),
            Phase::VerifyingEmpty => Some(Phase::QueueingInitialImport),
            Phase::QueueingInitialImport => {
                let import_job_id = match application
                    .ports
                    .managed_library_operations()
                    .queue_reset_reload_import(claim, &job)
                    .await
                {
                    Ok(id) => id,
                    Err(error) => {
                        return fail_reset_operation(application, claim, error.to_string()).await;
                    }
                };
                return match application
                    .ports
                    .managed_library_operations()
                    .complete_reset_reload(claim, import_job_id)
                    .await
                {
                    Ok(true) => Outcome::Completed,
                    Ok(false) => Outcome::LeaseLost,
                    Err(error) => Outcome::Failed {
                        message: error.to_string(),
                    },
                };
            }
            Phase::Complete => {
                return match application
                    .ports
                    .managed_library_operations()
                    .complete_operation(claim)
                    .await
                {
                    Ok(true) => Outcome::Completed,
                    Ok(false) => Outcome::LeaseLost,
                    Err(error) => Outcome::Failed {
                        message: error.to_string(),
                    },
                };
            }
            Phase::EnumeratingSource | Phase::ComparingCandidate => {
                return fail_reset_operation(
                    application,
                    claim,
                    "reset/reload entered a reconcile-only phase".to_owned(),
                )
                .await;
            }
        };
        let Some(next) = next else { continue };
        if job.phase != Phase::AwaitingLease && job.phase != Phase::UnpublishingProjection {
            if let Err(error) = application
                .ports
                .managed_library_operations()
                .apply_reset_reload_phase(claim, &job, job.phase)
                .await
            {
                return fail_reset_operation(application, claim, error.to_string()).await;
            }
        }
        match application
            .ports
            .managed_library_operations()
            .transition_reset_reload(claim, job.phase, next)
            .await
        {
            Ok(true) => {}
            Ok(false) => return Outcome::LeaseLost,
            Err(error) => {
                return Outcome::Failed {
                    message: error.to_string(),
                }
            }
        }
    }
    Outcome::Deferred
}

async fn fail_reset_operation(
    application: &ApplicationServices,
    claim: &ersatzrs_core::repositories::ManagedLibraryOperationClaim,
    message: String,
) -> commands::ExecuteManagedLibraryOperationOutcome {
    let _ = application
        .ports
        .managed_library_operations()
        .fail_operation(claim, "reset_reload_failed", "reset/reload failed")
        .await;
    commands::ExecuteManagedLibraryOperationOutcome::Failed { message }
}

#[allow(clippy::too_many_lines)] // Keep the short durable phase machine visible in one place.
async fn execute_addon_media_list_reconcile(
    application: &ApplicationServices,
    claim: &ersatzrs_core::repositories::ManagedLibraryOperationClaim,
) -> commands::ExecuteManagedLibraryOperationOutcome {
    use commands::ExecuteManagedLibraryOperationOutcome as Outcome;
    use ersatzrs_domain::AddonMediaListLifecyclePhase as Phase;

    for _ in 0..4 {
        let job = match application
            .ports
            .managed_library_operations()
            .reconcile_job(claim.operation.id)
            .await
        {
            Ok(Some(job)) => job,
            Ok(None) => {
                return fail_reconcile_operation(
                    application,
                    claim,
                    "reconcile lifecycle job is missing".to_owned(),
                )
                .await;
            }
            Err(error) => {
                return Outcome::Failed {
                    message: error.to_string(),
                }
            }
        };
        match job.phase {
            Phase::AwaitingLease => {
                match application
                    .ports
                    .managed_library_operations()
                    .transition_reconcile(claim, job.phase, Phase::EnumeratingSource)
                    .await
                {
                    Ok(true) => {}
                    Ok(false) => return Outcome::LeaseLost,
                    Err(error) => {
                        return Outcome::Failed {
                            message: error.to_string(),
                        }
                    }
                }
            }
            Phase::EnumeratingSource => {
                if let Err(error) = application
                    .ports
                    .managed_library_operations()
                    .queue_reconcile_import(claim, &job)
                    .await
                {
                    return fail_reconcile_operation(application, claim, error.to_string()).await;
                }
                match application
                    .ports
                    .managed_library_operations()
                    .transition_reconcile(claim, job.phase, Phase::ComparingCandidate)
                    .await
                {
                    Ok(true) => {}
                    Ok(false) => return Outcome::LeaseLost,
                    Err(error) => {
                        return Outcome::Failed {
                            message: error.to_string(),
                        }
                    }
                }
            }
            Phase::ComparingCandidate => {
                let Some(candidate) = job.candidate_import_job_id else {
                    return fail_reconcile_operation(
                        application,
                        claim,
                        "reconcile candidate import is missing".to_owned(),
                    )
                    .await;
                };
                return match application
                    .ports
                    .managed_library_operations()
                    .complete_reconcile(claim, candidate)
                    .await
                {
                    Ok(true) => Outcome::Completed,
                    Ok(false) => Outcome::LeaseLost,
                    Err(error) => Outcome::Failed {
                        message: error.to_string(),
                    },
                };
            }
            Phase::Complete => {
                return match application
                    .ports
                    .managed_library_operations()
                    .complete_operation(claim)
                    .await
                {
                    Ok(true) => Outcome::Completed,
                    Ok(false) => Outcome::LeaseLost,
                    Err(error) => Outcome::Failed {
                        message: error.to_string(),
                    },
                };
            }
            _ => {
                return fail_reconcile_operation(
                    application,
                    claim,
                    "reconcile entered a reset-only phase".to_owned(),
                )
                .await;
            }
        }
    }
    Outcome::Deferred
}

async fn fail_reconcile_operation(
    application: &ApplicationServices,
    claim: &ersatzrs_core::repositories::ManagedLibraryOperationClaim,
    message: String,
) -> commands::ExecuteManagedLibraryOperationOutcome {
    let _ = application
        .ports
        .managed_library_operations()
        .fail_operation(claim, "reconcile_failed", "source reconciliation failed")
        .await;
    commands::ExecuteManagedLibraryOperationOutcome::Failed { message }
}

async fn execute_durable_scan(
    application: &ApplicationServices,
    claim: &ersatzrs_core::repositories::ManagedLibraryOperationClaim,
) -> commands::ExecuteManagedLibraryOperationOutcome {
    use commands::ExecuteManagedLibraryOperationOutcome as Outcome;

    let Some(scan) = claim.operation.scan else {
        let message = "managed-library operation has no executable scan payload".to_owned();
        let _ = application
            .ports
            .managed_library_operations()
            .fail_operation(claim, "scan_payload_missing", &message)
            .await;
        return Outcome::Failed { message };
    };
    let request = scan.show_id.map_or(
        commands::ScanRequest::Library {
            provider: scan.provider,
            library_id: claim.operation.library_id,
            deep_scan: scan.deep_scan,
        },
        |show_id| commands::ScanRequest::Show {
            provider: scan.provider,
            library_id: claim.operation.library_id,
            show_id,
            deep_scan: scan.deep_scan,
        },
    );
    let operation_id = match u64::try_from(claim.operation.id) {
        Ok(value) => commands::ScanOperationId(value),
        Err(error) => {
            return Outcome::Failed {
                message: error.to_string(),
            };
        }
    };
    application
        .operations
        .scan_activity()
        .observe_persisted(operation_id, request);
    let outcome = application
        .execute_scan_work(commands::ScanWorkRequest {
            operation_id,
            request,
        })
        .await;
    match outcome {
        commands::ExecuteScanOutcome::Completed => match application
            .ports
            .managed_library_operations()
            .complete_operation(claim)
            .await
        {
            Ok(true) => Outcome::Completed,
            Ok(false) => Outcome::LeaseLost,
            Err(error) => Outcome::Failed {
                message: error.to_string(),
            },
        },
        commands::ExecuteScanOutcome::Failed { message } => {
            let _ = application
                .ports
                .managed_library_operations()
                .fail_operation(claim, "scan_failed", "managed-library scan failed")
                .await;
            Outcome::Failed { message }
        }
        commands::ExecuteScanOutcome::Missing => Outcome::Failed {
            message: "persisted scan was not visible to the progress registry".to_owned(),
        },
    }
}

struct ScanProgressReporter {
    registry: operations::ScanOperationRegistry,
    operation_id: commands::ScanOperationId,
}

impl ersatzrs_domain::ScanProgressSink for ScanProgressReporter {
    fn report(&self, fraction: f32) {
        self.registry.report_progress(self.operation_id, fraction);
    }
}

#[async_trait::async_trait]
impl ersatzrs_core::maintenance::ScheduledLibraryScanRequester for ApplicationServices {
    async fn request_scheduled_library_scan(
        &self,
        library_id: ersatzrs_domain::EntityId,
    ) -> ersatzrs_core::maintenance::ScheduledScanRequestOutcome {
        use ersatzrs_core::maintenance::ScheduledScanRequestOutcome;

        match commands::request_scheduled_library_scan(
            commands::RequestScheduledLibraryScan::new(library_id),
            self.ports.libraries(),
            self.ports.managed_library_operations(),
            self.ports.scan_work(),
            &self.operations,
        )
        .await
        {
            commands::RequestScheduledScanOutcome::Accepted { .. } => {
                ScheduledScanRequestOutcome::Accepted
            }
            commands::RequestScheduledScanOutcome::AlreadyActive => {
                ScheduledScanRequestOutcome::AlreadyActive
            }
            commands::RequestScheduledScanOutcome::Rejected(reason) => {
                ScheduledScanRequestOutcome::Rejected {
                    message: format!("{reason:?}"),
                }
            }
            commands::RequestScheduledScanOutcome::Unavailable { message } => {
                ScheduledScanRequestOutcome::Unavailable { message }
            }
        }
    }
}

async fn execute_scan_work_with_runtime(
    work: commands::ScanWorkRequest,
    runtime: &dyn ScanRuntime,
    playout_builds: &dyn PlayoutBuildQueue,
    operations: &OperationCoordinator,
) -> commands::ExecuteScanOutcome {
    let registry = operations.scan_activity();
    let Some(guard) = registry.start_guard(work.operation_id) else {
        return commands::ExecuteScanOutcome::Missing;
    };
    let progress = ScanProgressReporter {
        registry,
        operation_id: work.operation_id,
    };
    let result = match work.request {
        commands::ScanRequest::Library {
            provider,
            library_id,
            deep_scan,
        } => {
            runtime
                .scan_library(provider, library_id, deep_scan, &progress)
                .await
        }
        commands::ScanRequest::Show {
            provider,
            library_id,
            show_id,
            deep_scan,
        } => {
            let result = runtime
                .scan_show(provider, library_id, show_id, deep_scan, &progress)
                .await;
            match result {
                Ok(()) if deep_scan => runtime
                    .extract_embedded_show_subtitles(show_id)
                    .await
                    .map(|()| ScanRuntimeReport::default()),
                Ok(()) => Ok(ScanRuntimeReport::default()),
                Err(error) => Err(error),
            }
        }
    };

    match result.and_then(|report| {
        commands::require_scan_playout_refreshes(report, playout_builds, operations)
            .map_err(|error| ScanRuntimeError::new(error.to_string()))
    }) {
        Ok(()) => {
            guard.complete();
            commands::ExecuteScanOutcome::Completed
        }
        Err(error) => {
            let message = error.to_string();
            guard.fail(message.clone());
            commands::ExecuteScanOutcome::Failed { message }
        }
    }
}

impl std::fmt::Debug for ApplicationServices {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter
            .debug_struct("ApplicationServices")
            .field("ports", &self.ports)
            .field("operations", &self.operations)
            .finish()
    }
}

#[cfg(test)]
mod scan_execution_tests {
    use std::sync::Mutex;

    use async_trait::async_trait;
    use ersatzrs_core::locking::{EntityLocker, PlayoutBuildActivity};
    use ersatzrs_core::scheduling::{PlayoutBuildMode, PlayoutBuildRequest};
    use ersatzrs_domain::{
        EntityId, MediaSourceKind, ScanPlayoutRefresh, ScanPlayoutRefreshReason, ScanProgressSink,
    };

    use super::{
        execute_scan_work_with_runtime, managed_operation_execution_path,
        ManagedOperationExecutionPath, OperationCoordinator, PlayoutBuildQueue,
        PlayoutBuildQueueError, ScanRuntime, ScanRuntimeError, ScanRuntimeReport,
    };
    use crate::commands::{ExecuteScanOutcome, ScanRequest, ScanWorkRequest};
    use crate::queries::{GetLibraryScanStatus, LibraryScanStatus, ScanOperationState};

    #[test]
    fn lifecycle_operations_do_not_use_the_generic_scan_worker() {
        assert_eq!(
            managed_operation_execution_path(ersatzrs_domain::ManagedLibraryOperationKind::Reset),
            ManagedOperationExecutionPath::ResetReload
        );
        assert_eq!(
            managed_operation_execution_path(
                ersatzrs_domain::ManagedLibraryOperationKind::Reconcile
            ),
            ManagedOperationExecutionPath::Reconcile
        );
    }

    #[derive(Default)]
    struct FakeRuntime {
        calls: Mutex<Vec<String>>,
        fail_scan: bool,
        report: ScanRuntimeReport,
    }

    impl FakeRuntime {
        fn failing() -> Self {
            Self {
                calls: Mutex::default(),
                fail_scan: true,
                report: ScanRuntimeReport::default(),
            }
        }

        fn reporting(playout_refreshes: Vec<ScanPlayoutRefresh>) -> Self {
            Self {
                calls: Mutex::default(),
                fail_scan: false,
                report: ScanRuntimeReport { playout_refreshes },
            }
        }
    }

    #[async_trait]
    impl ScanRuntime for FakeRuntime {
        async fn scan_library(
            &self,
            provider: MediaSourceKind,
            library_id: EntityId,
            deep_scan: bool,
            progress: &dyn ScanProgressSink,
        ) -> Result<ScanRuntimeReport, ScanRuntimeError> {
            self.calls
                .lock()
                .expect("calls")
                .push(format!("library:{provider:?}:{library_id}:{deep_scan}"));
            progress.report(-1.0);
            progress.report(0.5);
            progress.report(2.0);
            if self.fail_scan {
                Err(ScanRuntimeError::new("library failed"))
            } else {
                Ok(self.report.clone())
            }
        }

        async fn scan_show(
            &self,
            provider: MediaSourceKind,
            library_id: EntityId,
            show_id: EntityId,
            deep_scan: bool,
            progress: &dyn ScanProgressSink,
        ) -> Result<(), ScanRuntimeError> {
            self.calls.lock().expect("calls").push(format!(
                "show:{provider:?}:{library_id}:{show_id}:{deep_scan}"
            ));
            progress.report(0.75);
            if self.fail_scan {
                Err(ScanRuntimeError::new("show failed"))
            } else {
                Ok(())
            }
        }

        async fn extract_embedded_show_subtitles(
            &self,
            show_id: EntityId,
        ) -> Result<(), ScanRuntimeError> {
            self.calls
                .lock()
                .expect("calls")
                .push(format!("subtitles:{show_id}"));
            Ok(())
        }
    }

    #[derive(Default)]
    struct RecordingPlayoutQueue {
        requests: Mutex<Vec<PlayoutBuildRequest>>,
    }

    impl PlayoutBuildQueue for RecordingPlayoutQueue {
        fn enqueue(&self, request: PlayoutBuildRequest) -> Result<(), PlayoutBuildQueueError> {
            self.requests.lock().expect("requests").push(request);
            Ok(())
        }
    }

    struct UnavailablePlayoutQueue;

    impl PlayoutBuildQueue for UnavailablePlayoutQueue {
        fn enqueue(&self, _request: PlayoutBuildRequest) -> Result<(), PlayoutBuildQueueError> {
            Err(PlayoutBuildQueueError::unavailable("scheduler stopped"))
        }
    }

    fn coordinator() -> OperationCoordinator {
        OperationCoordinator::new(EntityLocker::default(), PlayoutBuildActivity::default())
    }

    fn admitted(operations: &OperationCoordinator, request: ScanRequest) -> ScanWorkRequest {
        let mut work = None;
        operations
            .scan_activity()
            .admit(request, |token| {
                work = Some(token);
                Ok::<(), ()>(())
            })
            .expect("accepted");
        work.expect("queue token")
    }

    #[tokio::test]
    async fn worker_execution_records_running_progress_and_success_cleanup() {
        let operations = coordinator();
        let runtime = FakeRuntime::default();
        let queue = RecordingPlayoutQueue::default();
        let work = admitted(
            &operations,
            ScanRequest::Library {
                provider: MediaSourceKind::Plex,
                library_id: 20,
                deep_scan: true,
            },
        );

        assert_eq!(
            execute_scan_work_with_runtime(work, &runtime, &queue, &operations).await,
            ExecuteScanOutcome::Completed
        );
        assert_eq!(
            operations
                .scan_activity()
                .library_status(GetLibraryScanStatus::new(20)),
            LibraryScanStatus::Completed
        );
        let snapshot = operations.scan_activity().snapshots().remove(0);
        assert!(matches!(snapshot.state, ScanOperationState::Completed));
        assert_eq!(snapshot.progress, None);
        assert_eq!(
            runtime.calls.lock().expect("calls").as_slice(),
            ["library:Plex:20:true"]
        );
    }

    #[tokio::test]
    async fn failed_execution_remains_observable_without_active_progress() {
        let operations = coordinator();
        let runtime = FakeRuntime::failing();
        let queue = RecordingPlayoutQueue::default();
        let work = admitted(
            &operations,
            ScanRequest::Library {
                provider: MediaSourceKind::Emby,
                library_id: 21,
                deep_scan: false,
            },
        );

        assert_eq!(
            execute_scan_work_with_runtime(work, &runtime, &queue, &operations).await,
            ExecuteScanOutcome::Failed {
                message: "library failed".to_owned(),
            }
        );
        assert!(operations.scan_activity().progress_by_library().is_empty());
        assert!(matches!(
            operations
                .scan_activity()
                .library_status(GetLibraryScanStatus::new(21)),
            LibraryScanStatus::Failed { ref message } if message == "library failed"
        ));
    }

    #[tokio::test]
    async fn library_scan_coalesces_duplicate_dependency_refreshes() {
        let operations = coordinator();
        let runtime = FakeRuntime::reporting(vec![
            ScanPlayoutRefresh {
                playout_id: 80,
                reason: ScanPlayoutRefreshReason::RemoteStreamMembershipAdded,
            },
            ScanPlayoutRefresh {
                playout_id: 80,
                reason: ScanPlayoutRefreshReason::RemoteStreamMembershipRemoved,
            },
            ScanPlayoutRefresh {
                playout_id: 81,
                reason: ScanPlayoutRefreshReason::RemoteStreamMembershipReordered,
            },
        ]);
        let queue = RecordingPlayoutQueue::default();
        let work = admitted(
            &operations,
            ScanRequest::Library {
                provider: MediaSourceKind::Local,
                library_id: 24,
                deep_scan: false,
            },
        );

        assert_eq!(
            execute_scan_work_with_runtime(work, &runtime, &queue, &operations).await,
            ExecuteScanOutcome::Completed
        );
        let requests = queue.requests.lock().expect("requests");
        assert_eq!(requests.len(), 2);
        assert_eq!(requests[0].playout_id, 80);
        assert_eq!(requests[0].mode, PlayoutBuildMode::Refresh);
        assert_eq!(requests[1].playout_id, 81);
        assert_eq!(requests[1].mode, PlayoutBuildMode::Refresh);
    }

    #[tokio::test]
    async fn dependency_refresh_queue_failure_keeps_scan_failure_observable() {
        let operations = coordinator();
        let runtime = FakeRuntime::reporting(vec![ScanPlayoutRefresh {
            playout_id: 80,
            reason: ScanPlayoutRefreshReason::RemoteStreamMembershipAdded,
        }]);
        let work = admitted(
            &operations,
            ScanRequest::Library {
                provider: MediaSourceKind::Local,
                library_id: 25,
                deep_scan: false,
            },
        );

        let outcome =
            execute_scan_work_with_runtime(work, &runtime, &UnavailablePlayoutQueue, &operations)
                .await;
        assert!(matches!(
            outcome,
            ExecuteScanOutcome::Failed { ref message }
                if message.contains("remote-stream-membership-added")
                    && message.contains("scheduler stopped")
        ));
        assert!(matches!(
            operations
                .scan_activity()
                .library_status(GetLibraryScanStatus::new(25)),
            LibraryScanStatus::Failed { ref message }
                if message.contains("scheduler stopped")
        ));
    }

    #[tokio::test]
    async fn deep_show_extracts_subtitles_only_after_success() {
        let operations = coordinator();
        let runtime = FakeRuntime::default();
        let queue = RecordingPlayoutQueue::default();
        let successful = admitted(
            &operations,
            ScanRequest::Show {
                provider: MediaSourceKind::Jellyfin,
                library_id: 22,
                show_id: 220,
                deep_scan: true,
            },
        );
        assert_eq!(
            execute_scan_work_with_runtime(successful, &runtime, &queue, &operations).await,
            ExecuteScanOutcome::Completed
        );
        assert_eq!(
            runtime.calls.lock().expect("calls").as_slice(),
            ["show:Jellyfin:22:220:true", "subtitles:220"]
        );

        let failing_runtime = FakeRuntime::failing();
        let failed = admitted(
            &operations,
            ScanRequest::Show {
                provider: MediaSourceKind::Plex,
                library_id: 23,
                show_id: 230,
                deep_scan: true,
            },
        );
        assert!(matches!(
            execute_scan_work_with_runtime(failed, &failing_runtime, &queue, &operations).await,
            ExecuteScanOutcome::Failed { .. }
        ));
        assert_eq!(
            failing_runtime.calls.lock().expect("calls").as_slice(),
            ["show:Plex:23:230:true"]
        );
    }
}
