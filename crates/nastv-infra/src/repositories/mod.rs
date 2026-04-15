mod block;
mod channel;
mod collection;
mod common;
mod config;
mod deco;
mod ffmpeg_profile;
mod filler;
mod graphics;
mod library;
mod media_item;
mod media_source;
mod metadata;
mod playout;
mod program_schedule;
mod template;

pub use block::SqliteBlockRepository;
pub use channel::SqliteChannelRepository;
pub use collection::SqliteCollectionRepository;
pub use config::SqliteConfigRepository;
pub use deco::SqliteDecoRepository;
pub use ffmpeg_profile::SqliteFfmpegProfileRepository;
pub use filler::SqliteFillerRepository;
pub use graphics::SqliteGraphicsRepository;
pub use library::SqliteLibraryRepository;
pub use media_item::SqliteMediaItemRepository;
pub use media_source::SqliteMediaSourceRepository;
pub use metadata::SqliteMetadataRepository;
pub use playout::SqlitePlayoutRepository;
pub use program_schedule::SqliteProgramScheduleRepository;
use sqlx::{Pool, Sqlite};
pub use template::SqliteTemplateRepository;

#[derive(Clone)]
pub struct Repositories {
    pub channel: SqliteChannelRepository,
    pub playout: SqlitePlayoutRepository,
    pub program_schedule: SqliteProgramScheduleRepository,
    pub block: SqliteBlockRepository,
    pub deco: SqliteDecoRepository,
    pub template: SqliteTemplateRepository,
    pub media_item: SqliteMediaItemRepository,
    pub library: SqliteLibraryRepository,
    pub media_source: SqliteMediaSourceRepository,
    pub collection: SqliteCollectionRepository,
    pub ffmpeg_profile: SqliteFfmpegProfileRepository,
    pub filler: SqliteFillerRepository,
    pub graphics: SqliteGraphicsRepository,
    pub metadata: SqliteMetadataRepository,
    pub config: SqliteConfigRepository,
}

#[must_use]
pub fn make_repositories(pool: Pool<Sqlite>) -> Repositories {
    Repositories {
        channel: SqliteChannelRepository(pool.clone()),
        playout: SqlitePlayoutRepository(pool.clone()),
        program_schedule: SqliteProgramScheduleRepository(pool.clone()),
        block: SqliteBlockRepository(pool.clone()),
        deco: SqliteDecoRepository(pool.clone()),
        template: SqliteTemplateRepository(pool.clone()),
        media_item: SqliteMediaItemRepository(pool.clone()),
        library: SqliteLibraryRepository(pool.clone()),
        media_source: SqliteMediaSourceRepository(pool.clone()),
        collection: SqliteCollectionRepository(pool.clone()),
        ffmpeg_profile: SqliteFfmpegProfileRepository(pool.clone()),
        filler: SqliteFillerRepository(pool.clone()),
        graphics: SqliteGraphicsRepository(pool.clone()),
        metadata: SqliteMetadataRepository(pool.clone()),
        config: SqliteConfigRepository(pool),
    }
}
