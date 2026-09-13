mod addon_media_list;
pub(crate) mod addon_media_list_auto_eligibility;
mod addon_media_list_lifecycle;
pub(crate) use addon_media_list_lifecycle::{
    insert_revision_snapshot_in_tx, load_current_revision_items_in_tx, load_draft_options_in_tx,
    load_valid_revision_snapshot_in_tx, register_option_schema_in_tx, revision_snapshot_digest,
    save_draft_options_tx,
};
pub(crate) mod addon_media_list_reset;
pub(crate) mod addon_publication_fs;
pub(crate) use addon_media_list::{
    artwork_role_value, load_replacement_candidate, stage_editor_artwork_in_tx,
};
mod admin_media;
mod admin_settings;
mod artwork;
mod block;
mod channel;
mod collection;
pub(crate) mod common;
mod config;
mod deco;
mod ffmpeg_profile;
mod filler;
mod graphics;
mod library;
pub(crate) mod managed_library_operation;
mod media_item;
mod media_source;
mod metadata;
mod playout;
mod program_schedule;
mod saved_search;
mod scheduling;
mod television;
mod template;
mod troubleshooting;
mod watermark;

pub use addon_media_list::{project_one_pending_detail_batch, SqliteAddonMediaListRepository};
pub use admin_settings::SqliteAdminSettingsRepository;
pub use artwork::SqliteArtworkRepository;
pub use block::SqliteBlockRepository;
pub use channel::SqliteChannelRepository;
pub use collection::SqliteCollectionRepository;
pub use config::SqliteConfigRepository;
pub use deco::SqliteDecoRepository;
pub use ffmpeg_profile::SqliteFfmpegProfileRepository;
pub(crate) use ffmpeg_profile::{decode_hdr_content_light, decode_hdr_mastering_display};
pub use filler::SqliteFillerRepository;
pub use graphics::SqliteGraphicsRepository;
pub use library::SqliteLibraryRepository;
pub use managed_library_operation::SqliteManagedLibraryOperationRepository;
pub use media_item::SqliteMediaItemRepository;
pub use media_source::SqliteMediaSourceRepository;
pub use metadata::SqliteMetadataRepository;
pub use playout::SqlitePlayoutRepository;
pub use program_schedule::SqliteProgramScheduleRepository;
pub use saved_search::SqliteSavedSearchRepository;
pub use scheduling::SqliteSchedulingContentRepository;
use sqlx::Pool;
pub use television::SqliteTelevisionRepository;
pub use template::SqliteTemplateRepository;
pub use troubleshooting::SqliteTroubleshootingRepository;
pub use watermark::SqliteWatermarkRepository;

/// Translate a concrete `SQLx` failure into the storage-neutral repository
/// vocabulary before it crosses an adapter boundary.
#[must_use]
pub fn map_repository_error(error: &sqlx::Error) -> ersatzrs_core::repositories::RepositoryError {
    common::map_database_error(error)
}

pub(crate) trait SqlxResultExt<T> {
    fn repository(self) -> ersatzrs_core::repositories::Result<T>;
}

impl<T> SqlxResultExt<T> for std::result::Result<T, sqlx::Error> {
    fn repository(self) -> ersatzrs_core::repositories::Result<T> {
        self.map_err(|error| map_repository_error(&error))
    }
}

#[derive(Clone)]
pub struct Repositories {
    pub admin_media: SqliteAdminMediaRepository,
    pub admin_settings: SqliteAdminSettingsRepository,
    pub artwork: SqliteArtworkRepository,
    pub channel: SqliteChannelRepository,
    pub playout: SqlitePlayoutRepository,
    pub program_schedule: SqliteProgramScheduleRepository,
    pub saved_search: SqliteSavedSearchRepository,
    pub block: SqliteBlockRepository,
    pub deco: SqliteDecoRepository,
    pub template: SqliteTemplateRepository,
    pub media_item: SqliteMediaItemRepository,
    pub managed_library_operations: SqliteManagedLibraryOperationRepository,
    pub library: SqliteLibraryRepository,
    pub media_source: SqliteMediaSourceRepository,
    pub collection: SqliteCollectionRepository,
    pub ffmpeg_profile: SqliteFfmpegProfileRepository,
    pub filler: SqliteFillerRepository,
    pub graphics: SqliteGraphicsRepository,
    pub metadata: SqliteMetadataRepository,
    pub config: SqliteConfigRepository,
    pub scheduling: SqliteSchedulingContentRepository,
    pub television: SqliteTelevisionRepository,
    pub addon_media_lists: SqliteAddonMediaListRepository,
    pub troubleshooting: SqliteTroubleshootingRepository,
    pub watermark: SqliteWatermarkRepository,
}

#[must_use]
pub fn make_repositories(pool: Pool<crate::db::Db>) -> Repositories {
    Repositories {
        admin_media: SqliteAdminMediaRepository(pool.clone()),
        admin_settings: SqliteAdminSettingsRepository(pool.clone()),
        artwork: SqliteArtworkRepository(pool.clone()),
        channel: SqliteChannelRepository(pool.clone()),
        playout: SqlitePlayoutRepository(pool.clone()),
        program_schedule: SqliteProgramScheduleRepository(pool.clone()),
        saved_search: SqliteSavedSearchRepository(pool.clone()),
        block: SqliteBlockRepository(pool.clone()),
        deco: SqliteDecoRepository(pool.clone()),
        template: SqliteTemplateRepository(pool.clone()),
        media_item: SqliteMediaItemRepository(pool.clone()),
        managed_library_operations: SqliteManagedLibraryOperationRepository(pool.clone()),
        library: SqliteLibraryRepository(pool.clone()),
        media_source: SqliteMediaSourceRepository(pool.clone()),
        collection: SqliteCollectionRepository(pool.clone()),
        ffmpeg_profile: SqliteFfmpegProfileRepository(pool.clone()),
        filler: SqliteFillerRepository(pool.clone()),
        graphics: SqliteGraphicsRepository(pool.clone()),
        metadata: SqliteMetadataRepository(pool.clone()),
        config: SqliteConfigRepository(pool.clone()),
        scheduling: SqliteSchedulingContentRepository::new(pool.clone()),
        television: SqliteTelevisionRepository(pool.clone()),
        addon_media_lists: SqliteAddonMediaListRepository(pool.clone()),
        troubleshooting: SqliteTroubleshootingRepository(pool.clone()),
        watermark: SqliteWatermarkRepository(pool),
    }
}
pub use admin_media::SqliteAdminMediaRepository;
