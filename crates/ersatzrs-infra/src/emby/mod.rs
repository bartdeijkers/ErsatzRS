use crate::db::DbPool;
use crate::media_server::jellyfin_emby::{
    encode_datetime, ensure_media_file, ensure_media_version, is_anamorphic, item_etag_matches,
    nonempty_strings, replace_chapters, replace_streams, sort_title, JellyfinEmbyProvider,
    PageProgress, ProviderItemTable, VersionMediaItem, VersionOwner, VersionPlayback,
};
use crate::repositories::common::decode_enum_column;
use crate::repositories::SqlxResultExt as _;
use async_trait::async_trait;
use chrono::{DateTime, Duration, Utc};
use ersatzrs_core::repositories::{
    EmbyActor, EmbyCollectionItem, EmbyCollectionMediaItem, EmbyCollectionMediaKind,
    EmbyCollectionMediaPage, EmbyCollectionPage, EmbyConnectionConfig, EmbyItemPage,
    EmbyLibrarySummary, EmbyMediaItem, EmbyPathInfo, EmbyPlaybackMetadata, EmbySeasonItem,
    EmbySeasonPage, EmbyService as EmbyServiceTrait, EmbyShowItem, EmbyShowPage, RepositoryError,
    Result,
};
use ersatzrs_domain::{LibraryMediaKind, MediaChapter, MediaStream, MediaStreamKind};
use reqwest::Client;
use serde::Deserialize;
use sqlx::{Row, Transaction};

const EMBY_PAGE_SIZE: i32 = 100;

mod client;
mod models;
mod persistence;
mod projection;
mod sync;

pub(crate) use sync::load_active_emby_connection_config;

pub use client::EmbyService;
pub use persistence::{replace_emby_path_infos, sync_emby_libraries};
pub use sync::{
    sync_collections_for_media_source, sync_emby_collection_library,
    sync_emby_collection_library_with_progress, sync_emby_movie_library,
    sync_emby_movie_library_with_progress, sync_emby_show_by_id, sync_emby_show_library,
    sync_emby_show_library_with_progress, sync_library_by_id, sync_library_by_id_with_options,
    sync_library_by_id_with_progress,
};

use models::{
    EmbyChapterResponse, EmbyLibraryItemResponse, EmbyLibraryItemsResponse, EmbyLibraryResponse,
    EmbyMediaStreamResponse, EmbyNameRefResponse, EmbyPersonResponse, EmbyPlaybackInfoResponse,
    EmbyProviderIdsResponse,
};
use persistence::{
    replace_collection_items, upsert_emby_collection, upsert_emby_episode, upsert_emby_movie,
    upsert_emby_season, upsert_emby_show,
};
use projection::{
    project_collection, project_collection_media_item, project_episode, project_library,
    project_movie, project_playback, project_season, project_show,
};
