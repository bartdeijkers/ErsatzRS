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
    JellyfinActor, JellyfinCollectionItem, JellyfinCollectionMediaItem,
    JellyfinCollectionMediaKind, JellyfinCollectionMediaPage, JellyfinCollectionPage,
    JellyfinConnectionConfig, JellyfinItemPage, JellyfinLibrarySummary, JellyfinMediaItem,
    JellyfinPathInfo, JellyfinPlaybackMetadata, JellyfinSeasonItem, JellyfinSeasonPage,
    JellyfinService as JellyfinServiceTrait, JellyfinShowItem, JellyfinShowPage, RepositoryError,
    Result,
};
use ersatzrs_domain::{LibraryMediaKind, MediaChapter, MediaStream, MediaStreamKind};
use reqwest::Client;
use serde::Deserialize;
use sqlx::{Row, Transaction};

const JELLYFIN_PAGE_SIZE: i32 = 100;

mod client;
mod models;
mod persistence;
mod projection;
mod sync;

pub(crate) use sync::load_active_jellyfin_connection_config;

pub use client::{
    fetch_server_information, format_jellyfin_auth_header, JellyfinServerInformation,
    JellyfinService,
};
pub use persistence::{replace_jellyfin_path_infos, sync_jellyfin_libraries};
pub use sync::{
    sync_collections_for_media_source, sync_jellyfin_collection_library,
    sync_jellyfin_collection_library_with_progress, sync_jellyfin_movie_library,
    sync_jellyfin_movie_library_with_progress, sync_jellyfin_show_by_id,
    sync_jellyfin_show_by_id_with_managed_claim, sync_jellyfin_show_library,
    sync_jellyfin_show_library_with_progress, sync_library_by_id,
    sync_library_by_id_with_managed_claim, sync_library_by_id_with_options,
    sync_library_by_id_with_progress,
};

use models::{
    JellyfinChapterResponse, JellyfinLibraryItemResponse, JellyfinLibraryItemsResponse,
    JellyfinLibraryResponse, JellyfinMediaStreamResponse, JellyfinPersonResponse,
    JellyfinPlaybackInfoResponse, JellyfinProviderIdsResponse,
};
use persistence::{
    replace_collection_items, upsert_jellyfin_collection, upsert_jellyfin_episode,
    upsert_jellyfin_movie, upsert_jellyfin_season, upsert_jellyfin_show,
};
use projection::{
    project_collection, project_collection_media_item, project_episode, project_library,
    project_movie, project_playback, project_season, project_show,
};
