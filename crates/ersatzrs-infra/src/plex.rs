use chrono::{DateTime, Duration, TimeZone, Utc};
use ersatzrs_core::repositories::{RepositoryError, Result};
use ersatzrs_domain::{LibraryMediaKind, MediaStream, MediaStreamKind, VideoScanKind};
use reqwest::{
    header::{HeaderValue, ACCEPT},
    Client,
};
use serde::Deserialize;
use sqlx::{Row, Sqlite, SqlitePool, Transaction};

const PLEX_PAGE_SIZE: i32 = 100;
const APPLICATION_JSON: HeaderValue = HeaderValue::from_static("application/json");

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PlexConnectionConfig {
    pub uri: String,
    pub token: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PlexLibrarySummary {
    pub key: String,
    pub name: String,
    pub media_kind: LibraryMediaKind,
}

#[derive(Debug, Clone, PartialEq)]
pub struct PlexMediaItem {
    pub key: String,
    pub etag: Option<String>,
    pub title: String,
    pub path: String,
    pub part_key: Option<String>,
    pub plex_part_id: i32,
    pub date_added: DateTime<Utc>,
    pub date_updated: DateTime<Utc>,
    pub duration: Duration,
    pub width: i32,
    pub height: i32,
    pub sample_aspect_ratio: Option<String>,
    pub display_aspect_ratio: Option<String>,
    pub r_frame_rate: Option<String>,
    pub video_scan_kind: VideoScanKind,
    pub streams: Vec<MediaStream>,
    pub season_number: i32,
    pub episode_number: i32,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PlexShowItem {
    pub key: String,
    pub etag: Option<String>,
    pub title: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PlexSeasonItem {
    pub key: String,
    pub etag: Option<String>,
    pub title: String,
    pub season_number: i32,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PlexCollectionItem {
    pub key: String,
    pub etag: Option<String>,
    pub title: String,
    pub library_section_id: i32,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PlexCollectionMediaKind {
    Movie,
    Show,
    Season,
    Episode,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PlexCollectionMediaItem {
    pub key: String,
    pub kind: PlexCollectionMediaKind,
}

#[derive(Debug, Clone, PartialEq)]
pub struct PlexPage<T> {
    pub total_record_count: i32,
    pub items: Vec<T>,
}

#[derive(Debug, Clone)]
pub struct PlexService {
    client: Client,
}

impl Default for PlexService {
    fn default() -> Self {
        Self::new()
    }
}

impl PlexService {
    #[must_use]
    pub fn new() -> Self {
        Self {
            client: Client::new(),
        }
    }

    /// List movie and show libraries from Plex.
    ///
    /// # Errors
    ///
    /// Returns an error when the HTTP request or response decoding fails.
    pub async fn list_libraries(
        &self,
        config: &PlexConnectionConfig,
    ) -> Result<Vec<PlexLibrarySummary>> {
        let url = format!("{}/library/sections", config.uri.trim_end_matches('/'));
        let response: PlexLibrariesResponse = self.get(config, url).await?;
        Ok(response
            .media_container
            .directory
            .into_iter()
            .filter_map(project_library)
            .collect())
    }

    /// Fetch one page of Plex movie items.
    ///
    /// # Errors
    ///
    /// Returns an error when the HTTP request or response decoding fails.
    pub async fn fetch_movie_page(
        &self,
        config: &PlexConnectionConfig,
        library_key: &str,
        start_index: i32,
        limit: i32,
    ) -> Result<PlexPage<PlexMediaItem>> {
        let response = self
            .fetch_section_page(config, library_key, start_index, limit)
            .await?;
        Ok(PlexPage {
            total_record_count: response.media_container.total_size,
            items: response
                .media_container
                .metadata
                .into_iter()
                .filter_map(project_media_item)
                .collect(),
        })
    }

    /// Fetch one page of Plex show items.
    ///
    /// # Errors
    ///
    /// Returns an error when the HTTP request or response decoding fails.
    pub async fn fetch_show_page(
        &self,
        config: &PlexConnectionConfig,
        library_key: &str,
        start_index: i32,
        limit: i32,
    ) -> Result<PlexPage<PlexShowItem>> {
        let response = self
            .fetch_section_page(config, library_key, start_index, limit)
            .await?;
        Ok(PlexPage {
            total_record_count: response.media_container.total_size,
            items: response
                .media_container
                .metadata
                .into_iter()
                .filter_map(project_show)
                .collect(),
        })
    }

    /// Fetch one page of Plex seasons for a show rating key.
    ///
    /// # Errors
    ///
    /// Returns an error when the HTTP request or response decoding fails.
    pub async fn fetch_season_page(
        &self,
        config: &PlexConnectionConfig,
        show_key: &str,
        start_index: i32,
        limit: i32,
    ) -> Result<PlexPage<PlexSeasonItem>> {
        let response = self
            .fetch_children_page(config, show_key, start_index, limit)
            .await?;
        Ok(PlexPage {
            total_record_count: response.media_container.total_size,
            items: response
                .media_container
                .metadata
                .into_iter()
                .filter_map(project_season)
                .collect(),
        })
    }

    /// Fetch one page of Plex episodes for a season rating key.
    ///
    /// # Errors
    ///
    /// Returns an error when the HTTP request or response decoding fails.
    pub async fn fetch_episode_page(
        &self,
        config: &PlexConnectionConfig,
        season_key: &str,
        start_index: i32,
        limit: i32,
    ) -> Result<PlexPage<PlexMediaItem>> {
        let response = self
            .fetch_children_page(config, season_key, start_index, limit)
            .await?;
        Ok(PlexPage {
            total_record_count: response.media_container.total_size,
            items: response
                .media_container
                .metadata
                .into_iter()
                .filter_map(project_media_item)
                .collect(),
        })
    }

    /// Fetch one page of Plex collections.
    ///
    /// # Errors
    ///
    /// Returns an error when the HTTP request or response decoding fails.
    pub async fn fetch_collection_page(
        &self,
        config: &PlexConnectionConfig,
        start_index: i32,
        limit: i32,
    ) -> Result<PlexPage<PlexCollectionItem>> {
        let url = format!("{}/library/all", config.uri.trim_end_matches('/'));
        let start_index = start_index.to_string();
        let limit = limit.to_string();
        let response: PlexCollectionResponse = self
            .client
            .get(url)
            .header(ACCEPT, APPLICATION_JSON)
            .header("X-Plex-Token", &config.token)
            .query(&[
                ("type", "18"),
                ("X-Plex-Container-Start", start_index.as_str()),
                ("X-Plex-Container-Size", limit.as_str()),
            ])
            .send()
            .await
            .map_err(provider_error)?
            .error_for_status()
            .map_err(provider_error)?
            .json()
            .await
            .map_err(provider_error)?;

        Ok(PlexPage {
            total_record_count: response.media_container.total_size,
            items: response
                .media_container
                .metadata
                .into_iter()
                .filter_map(project_collection)
                .collect(),
        })
    }

    /// Fetch one page of Plex collection members.
    ///
    /// # Errors
    ///
    /// Returns an error when the HTTP request or response decoding fails.
    pub async fn fetch_collection_items_page(
        &self,
        config: &PlexConnectionConfig,
        collection_key: &str,
        start_index: i32,
        limit: i32,
    ) -> Result<PlexPage<PlexCollectionMediaItem>> {
        let url = format!(
            "{}/library/collections/{collection_key}/children",
            config.uri.trim_end_matches('/')
        );
        let response: PlexCollectionItemsResponse =
            self.get_page(config, url, start_index, limit).await?;

        Ok(PlexPage {
            total_record_count: response.media_container.total_size,
            items: response
                .media_container
                .metadata
                .into_iter()
                .filter_map(project_collection_media_item)
                .collect(),
        })
    }

    async fn fetch_section_page(
        &self,
        config: &PlexConnectionConfig,
        library_key: &str,
        start_index: i32,
        limit: i32,
    ) -> Result<PlexMetadataResponse> {
        let url = format!(
            "{}/library/sections/{library_key}/all",
            config.uri.trim_end_matches('/')
        );
        self.get_page(config, url, start_index, limit).await
    }

    async fn fetch_children_page(
        &self,
        config: &PlexConnectionConfig,
        key: &str,
        start_index: i32,
        limit: i32,
    ) -> Result<PlexMetadataResponse> {
        let key = key.trim_matches('/').rsplit('/').next().unwrap_or(key);
        let url = format!(
            "{}/library/metadata/{key}/children",
            config.uri.trim_end_matches('/')
        );
        self.get_page(config, url, start_index, limit).await
    }

    async fn get_page<T>(
        &self,
        config: &PlexConnectionConfig,
        url: String,
        start_index: i32,
        limit: i32,
    ) -> Result<T>
    where
        T: for<'de> Deserialize<'de>,
    {
        self.client
            .get(url)
            .header(ACCEPT, APPLICATION_JSON)
            .header("X-Plex-Token", &config.token)
            .query(&[
                ("X-Plex-Container-Start", start_index.to_string()),
                ("X-Plex-Container-Size", limit.to_string()),
            ])
            .send()
            .await
            .map_err(provider_error)?
            .error_for_status()
            .map_err(provider_error)?
            .json()
            .await
            .map_err(provider_error)
    }

    async fn get<T>(&self, config: &PlexConnectionConfig, url: String) -> Result<T>
    where
        T: for<'de> Deserialize<'de>,
    {
        self.client
            .get(url)
            .header(ACCEPT, APPLICATION_JSON)
            .header("X-Plex-Token", &config.token)
            .send()
            .await
            .map_err(provider_error)?
            .error_for_status()
            .map_err(provider_error)?
            .json()
            .await
            .map_err(provider_error)
    }
}

/// Sync Plex movies into media rows.
///
/// # Errors
///
/// Returns an error when Plex requests or database writes fail.
pub async fn sync_plex_movie_library(
    pool: &SqlitePool,
    service: &PlexService,
    config: &PlexConnectionConfig,
    media_source_id: i64,
    library_id: i64,
    library_key: &str,
) -> Result<usize> {
    let mut synced = 0;
    let mut start_index = 0;
    let mut page = service
        .fetch_movie_page(config, library_key, start_index, PLEX_PAGE_SIZE)
        .await?;
    let total_size = page.total_record_count;

    while start_index < total_size {
        for item in &page.items {
            upsert_plex_movie(pool, media_source_id, library_id, library_key, item).await?;
            synced += 1;
        }
        start_index += PLEX_PAGE_SIZE;
        if start_index >= total_size {
            break;
        }
        page = service
            .fetch_movie_page(config, library_key, start_index, PLEX_PAGE_SIZE)
            .await?;
    }

    Ok(synced)
}

/// Sync Plex shows, seasons, and episodes into media rows.
///
/// # Errors
///
/// Returns an error when Plex requests or database writes fail.
pub async fn sync_plex_show_library(
    pool: &SqlitePool,
    service: &PlexService,
    config: &PlexConnectionConfig,
    media_source_id: i64,
    library_id: i64,
    library_key: &str,
) -> Result<usize> {
    let mut synced = 0;
    let mut show_start_index = 0;
    let mut shows = service
        .fetch_show_page(config, library_key, show_start_index, PLEX_PAGE_SIZE)
        .await?;
    let show_total_size = shows.total_record_count;

    while show_start_index < show_total_size {
        for show in &shows.items {
            let show_id =
                upsert_plex_show(pool, media_source_id, library_id, library_key, show).await?;
            let mut season_start_index = 0;
            let mut seasons = service
                .fetch_season_page(config, &show.key, season_start_index, PLEX_PAGE_SIZE)
                .await?;
            let season_total_size = seasons.total_record_count;

            while season_start_index < season_total_size {
                for season in &seasons.items {
                    let season_id = upsert_plex_season(
                        pool,
                        media_source_id,
                        library_id,
                        library_key,
                        show_id,
                        season,
                    )
                    .await?;
                    let mut episode_start_index = 0;
                    let mut episodes = service
                        .fetch_episode_page(
                            config,
                            &season.key,
                            episode_start_index,
                            PLEX_PAGE_SIZE,
                        )
                        .await?;
                    let episode_total_size = episodes.total_record_count;

                    while episode_start_index < episode_total_size {
                        for episode in &episodes.items {
                            upsert_plex_episode(
                                pool,
                                media_source_id,
                                library_id,
                                library_key,
                                season_id,
                                episode,
                            )
                            .await?;
                            synced += 1;
                        }
                        episode_start_index += PLEX_PAGE_SIZE;
                        if episode_start_index >= episode_total_size {
                            break;
                        }
                        episodes = service
                            .fetch_episode_page(
                                config,
                                &season.key,
                                episode_start_index,
                                PLEX_PAGE_SIZE,
                            )
                            .await?;
                    }
                }
                season_start_index += PLEX_PAGE_SIZE;
                if season_start_index >= season_total_size {
                    break;
                }
                seasons = service
                    .fetch_season_page(config, &show.key, season_start_index, PLEX_PAGE_SIZE)
                    .await?;
            }
        }
        show_start_index += PLEX_PAGE_SIZE;
        if show_start_index >= show_total_size {
            break;
        }
        shows = service
            .fetch_show_page(config, library_key, show_start_index, PLEX_PAGE_SIZE)
            .await?;
    }

    Ok(synced)
}

/// Sync Plex collections and their memberships.
///
/// # Errors
///
/// Returns an error when Plex requests or database writes fail.
pub async fn sync_plex_collection_library(
    pool: &SqlitePool,
    service: &PlexService,
    config: &PlexConnectionConfig,
    media_source_id: i64,
    library_key: &str,
) -> Result<usize> {
    let library_section_id = library_key.parse::<i32>().unwrap_or_default();
    let mut synced = 0;
    let mut start_index = 0;
    let mut page = service
        .fetch_collection_page(config, start_index, PLEX_PAGE_SIZE)
        .await?;
    let total_size = page.total_record_count;

    while start_index < total_size {
        for collection in &page.items {
            if collection.library_section_id != library_section_id {
                continue;
            }

            let collection_id = upsert_plex_collection(pool, media_source_id, collection).await?;
            let mut item_start_index = 0;
            let items = service
                .fetch_collection_items_page(
                    config,
                    &collection.key,
                    item_start_index,
                    PLEX_PAGE_SIZE,
                )
                .await?;
            let item_total_size = items.total_record_count;
            let mut all_items = items.items;

            while item_start_index + PLEX_PAGE_SIZE < item_total_size {
                item_start_index += PLEX_PAGE_SIZE;
                let next_page = service
                    .fetch_collection_items_page(
                        config,
                        &collection.key,
                        item_start_index,
                        PLEX_PAGE_SIZE,
                    )
                    .await?;
                all_items.extend(next_page.items);
            }
            replace_plex_collection_items(pool, collection_id, &all_items).await?;
            synced += 1;
        }

        start_index += PLEX_PAGE_SIZE;
        if start_index >= total_size {
            break;
        }
        page = service
            .fetch_collection_page(config, start_index, PLEX_PAGE_SIZE)
            .await?;
    }

    Ok(synced)
}

async fn upsert_plex_movie(
    pool: &SqlitePool,
    media_source_id: i64,
    library_id: i64,
    library_key: &str,
    item: &PlexMediaItem,
) -> Result<()> {
    let mut tx = pool.begin().await?;
    ensure_plex_library(&mut tx, media_source_id, library_id, library_key).await?;
    let library_path_id =
        ensure_library_path(&mut tx, library_id, &format!("plex://{library_id}")).await?;
    let item_id = ensure_plex_movie_item(&mut tx, library_path_id, item).await?;
    let version_id = ensure_media_version(&mut tx, VersionOwner::Movie(item_id), item).await?;
    ensure_media_file(&mut tx, version_id, item).await?;
    replace_streams(&mut tx, version_id, &item.streams).await?;
    tx.commit().await?;
    Ok(())
}

async fn upsert_plex_show(
    pool: &SqlitePool,
    media_source_id: i64,
    library_id: i64,
    library_key: &str,
    item: &PlexShowItem,
) -> Result<i64> {
    let mut tx = pool.begin().await?;
    ensure_plex_library(&mut tx, media_source_id, library_id, library_key).await?;
    let library_path_id =
        ensure_library_path(&mut tx, library_id, &format!("plex://{library_id}")).await?;
    let id = ensure_plex_show_item(&mut tx, library_path_id, item).await?;
    tx.commit().await?;
    Ok(id)
}

async fn upsert_plex_season(
    pool: &SqlitePool,
    media_source_id: i64,
    library_id: i64,
    library_key: &str,
    show_id: i64,
    item: &PlexSeasonItem,
) -> Result<i64> {
    let mut tx = pool.begin().await?;
    ensure_plex_library(&mut tx, media_source_id, library_id, library_key).await?;
    let library_path_id =
        ensure_library_path(&mut tx, library_id, &format!("plex://{library_id}")).await?;
    let id = ensure_plex_season_item(&mut tx, library_path_id, show_id, item).await?;
    tx.commit().await?;
    Ok(id)
}

async fn upsert_plex_episode(
    pool: &SqlitePool,
    media_source_id: i64,
    library_id: i64,
    library_key: &str,
    season_id: i64,
    item: &PlexMediaItem,
) -> Result<()> {
    let mut tx = pool.begin().await?;
    ensure_plex_library(&mut tx, media_source_id, library_id, library_key).await?;
    let library_path_id =
        ensure_library_path(&mut tx, library_id, &format!("plex://{library_id}")).await?;
    let item_id = ensure_plex_episode_item(&mut tx, library_path_id, season_id, item).await?;
    let version_id = ensure_media_version(&mut tx, VersionOwner::Episode(item_id), item).await?;
    ensure_media_file(&mut tx, version_id, item).await?;
    replace_streams(&mut tx, version_id, &item.streams).await?;
    tx.commit().await?;
    Ok(())
}

fn project_library(response: PlexLibraryResponse) -> Option<PlexLibrarySummary> {
    let media_kind = match response.library_type.as_str() {
        "movie" => LibraryMediaKind::Movies,
        "show" => LibraryMediaKind::Shows,
        _ => return None,
    };
    Some(PlexLibrarySummary {
        key: response.key,
        name: response.title,
        media_kind,
    })
}

fn project_show(item: PlexMetadataItemResponse) -> Option<PlexShowItem> {
    Some(PlexShowItem {
        key: item.key.or(item.rating_key)?,
        etag: item.updated_at.map(|value| value.to_string()),
        title: item.title.unwrap_or_default(),
    })
}

fn project_season(item: PlexMetadataItemResponse) -> Option<PlexSeasonItem> {
    Some(PlexSeasonItem {
        key: item.key.or(item.rating_key)?,
        etag: item.updated_at.map(|value| value.to_string()),
        title: item.title.unwrap_or_default(),
        season_number: item.index.unwrap_or_default(),
    })
}

fn project_collection(item: PlexCollectionMetadataItemResponse) -> Option<PlexCollectionItem> {
    Some(PlexCollectionItem {
        key: item.rating_key?,
        etag: item.updated_at.map(|value| value.to_string()),
        title: item.title.unwrap_or_default(),
        library_section_id: item.library_section_id.unwrap_or_default(),
    })
}

fn project_collection_media_item(
    item: PlexCollectionMediaItemResponse,
) -> Option<PlexCollectionMediaItem> {
    let kind = match item.item_type.as_deref()? {
        "movie" => PlexCollectionMediaKind::Movie,
        "show" => PlexCollectionMediaKind::Show,
        "season" => PlexCollectionMediaKind::Season,
        "episode" => PlexCollectionMediaKind::Episode,
        _ => return None,
    };

    Some(PlexCollectionMediaItem {
        key: item.key.or(item.rating_key)?,
        kind,
    })
}

fn project_media_item(item: PlexMetadataItemResponse) -> Option<PlexMediaItem> {
    let media = item
        .media
        .into_iter()
        .filter(|media| !media.part.is_empty())
        .max_by_key(|media| media.id)?;
    let part = media.part.into_iter().next()?;
    let video = part
        .stream
        .iter()
        .find(|stream| stream.stream_type == Some(1));
    let width = video.and_then(|stream| stream.width).unwrap_or(media.width);
    let height = video
        .and_then(|stream| stream.height)
        .unwrap_or(media.height);
    Some(PlexMediaItem {
        key: item.key.or(item.rating_key)?,
        etag: item.updated_at.map(|value| value.to_string()),
        title: item.title.unwrap_or_default(),
        path: part.file?,
        part_key: part.key,
        plex_part_id: part.id,
        date_added: unix_time(item.added_at),
        date_updated: unix_time(item.updated_at),
        duration: Duration::milliseconds(i64::from(media.duration)),
        width,
        height,
        sample_aspect_ratio: video
            .and_then(|stream| stream.pixel_aspect_ratio.clone())
            .or_else(|| Some("1:1".to_owned())),
        display_aspect_ratio: (media.aspect_ratio > 0.0)
            .then(|| format!("{:.2}", media.aspect_ratio)),
        r_frame_rate: video.and_then(|stream| stream.frame_rate.clone()),
        video_scan_kind: video.and_then(|stream| stream.scan_type.as_deref()).map_or(
            VideoScanKind::Unknown,
            |scan| match scan {
                "interlaced" => VideoScanKind::Interlaced,
                "progressive" => VideoScanKind::Progressive,
                _ => VideoScanKind::Unknown,
            },
        ),
        streams: part.stream.into_iter().filter_map(project_stream).collect(),
        season_number: 0,
        episode_number: item.index.unwrap_or_default(),
    })
}

fn project_stream(stream: PlexStreamResponse) -> Option<MediaStream> {
    let kind = match stream.stream_type? {
        1 => MediaStreamKind::Video,
        2 => MediaStreamKind::Audio,
        3 if stream.index.is_none() && stream.key.is_some() => MediaStreamKind::ExternalSubtitle,
        3 if !stream.embedded_in_video => MediaStreamKind::Subtitle,
        _ => return None,
    };
    Some(MediaStream {
        index: stream.index.unwrap_or(stream.id),
        kind,
        codec: stream.codec.map(|codec| codec.to_ascii_lowercase()),
        language: stream.language_code,
        title: stream.title,
        default: stream.default,
        forced: stream.forced,
        channels: stream.channels,
        width: stream.width,
        height: stream.height,
        ..MediaStream::default()
    })
}

async fn ensure_plex_library(
    tx: &mut Transaction<'_, Sqlite>,
    media_source_id: i64,
    library_id: i64,
    key: &str,
) -> Result<()> {
    sqlx::query(r#"INSERT OR IGNORE INTO "MediaSource" ("Id") VALUES (?)"#)
        .bind(media_source_id)
        .execute(&mut **tx)
        .await?;
    sqlx::query(r#"INSERT OR IGNORE INTO "PlexMediaSource" ("Id") VALUES (?)"#)
        .bind(media_source_id)
        .execute(&mut **tx)
        .await?;
    sqlx::query(
        r#"INSERT OR IGNORE INTO "Library" ("Id", "Name", "MediaKind", "MediaSourceId")
           VALUES (?, 'Plex', 0, ?)"#,
    )
    .bind(library_id)
    .bind(media_source_id)
    .execute(&mut **tx)
    .await?;
    sqlx::query(
        r#"INSERT OR IGNORE INTO "PlexLibrary" ("Id", "Key", "ShouldSyncItems")
           VALUES (?, ?, 1)"#,
    )
    .bind(library_id)
    .bind(key)
    .execute(&mut **tx)
    .await?;
    Ok(())
}

async fn ensure_library_path(
    tx: &mut Transaction<'_, Sqlite>,
    library_id: i64,
    path: &str,
) -> Result<i64> {
    if let Some(row) = sqlx::query(r#"SELECT "Id" FROM "LibraryPath" WHERE "LibraryId" = ?"#)
        .bind(library_id)
        .fetch_optional(&mut **tx)
        .await?
    {
        return Ok(row.get("Id"));
    }
    let result = sqlx::query(r#"INSERT INTO "LibraryPath" ("LibraryId", "Path") VALUES (?, ?)"#)
        .bind(library_id)
        .bind(path)
        .execute(&mut **tx)
        .await?;
    Ok(result.last_insert_rowid())
}

async fn ensure_plex_movie_item(
    tx: &mut Transaction<'_, Sqlite>,
    library_path_id: i64,
    item: &PlexMediaItem,
) -> Result<i64> {
    ensure_simple_media_item(
        tx,
        "Movie",
        "PlexMovie",
        "Key",
        library_path_id,
        &item.key,
        item.etag.as_ref(),
    )
    .await
}

async fn ensure_plex_show_item(
    tx: &mut Transaction<'_, Sqlite>,
    library_path_id: i64,
    item: &PlexShowItem,
) -> Result<i64> {
    ensure_simple_media_item(
        tx,
        "Show",
        "PlexShow",
        "Key",
        library_path_id,
        &item.key,
        item.etag.as_ref(),
    )
    .await
}

async fn ensure_plex_season_item(
    tx: &mut Transaction<'_, Sqlite>,
    library_path_id: i64,
    show_id: i64,
    item: &PlexSeasonItem,
) -> Result<i64> {
    if let Some(row) = sqlx::query(r#"SELECT "Id" FROM "PlexSeason" WHERE "Key" = ?"#)
        .bind(&item.key)
        .fetch_optional(&mut **tx)
        .await?
    {
        let id = row.get("Id");
        sqlx::query(r#"UPDATE "PlexSeason" SET "Etag" = ? WHERE "Id" = ?"#)
            .bind(&item.etag)
            .bind(id)
            .execute(&mut **tx)
            .await?;
        sqlx::query(r#"UPDATE "Season" SET "ShowId" = ?, "SeasonNumber" = ? WHERE "Id" = ?"#)
            .bind(show_id)
            .bind(item.season_number)
            .bind(id)
            .execute(&mut **tx)
            .await?;
        return Ok(id);
    }
    let id = insert_media_item(tx, library_path_id).await?;
    sqlx::query(r#"INSERT INTO "Season" ("Id", "ShowId", "SeasonNumber") VALUES (?, ?, ?)"#)
        .bind(id)
        .bind(show_id)
        .bind(item.season_number)
        .execute(&mut **tx)
        .await?;
    sqlx::query(r#"INSERT INTO "PlexSeason" ("Id", "Key", "Etag") VALUES (?, ?, ?)"#)
        .bind(id)
        .bind(&item.key)
        .bind(&item.etag)
        .execute(&mut **tx)
        .await?;
    Ok(id)
}

async fn ensure_plex_episode_item(
    tx: &mut Transaction<'_, Sqlite>,
    library_path_id: i64,
    season_id: i64,
    item: &PlexMediaItem,
) -> Result<i64> {
    if let Some(row) = sqlx::query(r#"SELECT "Id" FROM "PlexEpisode" WHERE "Key" = ?"#)
        .bind(&item.key)
        .fetch_optional(&mut **tx)
        .await?
    {
        let id = row.get("Id");
        sqlx::query(r#"UPDATE "PlexEpisode" SET "Etag" = ? WHERE "Id" = ?"#)
            .bind(&item.etag)
            .bind(id)
            .execute(&mut **tx)
            .await?;
        sqlx::query(r#"UPDATE "Episode" SET "SeasonId" = ? WHERE "Id" = ?"#)
            .bind(season_id)
            .bind(id)
            .execute(&mut **tx)
            .await?;
        return Ok(id);
    }
    let id = insert_media_item(tx, library_path_id).await?;
    sqlx::query(r#"INSERT INTO "Episode" ("Id", "SeasonId") VALUES (?, ?)"#)
        .bind(id)
        .bind(season_id)
        .execute(&mut **tx)
        .await?;
    sqlx::query(r#"INSERT INTO "PlexEpisode" ("Id", "Key", "Etag") VALUES (?, ?, ?)"#)
        .bind(id)
        .bind(&item.key)
        .bind(&item.etag)
        .execute(&mut **tx)
        .await?;
    Ok(id)
}

async fn upsert_plex_collection(
    pool: &SqlitePool,
    media_source_id: i64,
    item: &PlexCollectionItem,
) -> Result<i64> {
    let mut tx = pool.begin().await?;
    sqlx::query(r#"INSERT OR IGNORE INTO "MediaSource" ("Id") VALUES (?)"#)
        .bind(media_source_id)
        .execute(&mut *tx)
        .await?;
    sqlx::query(r#"INSERT OR IGNORE INTO "PlexMediaSource" ("Id") VALUES (?)"#)
        .bind(media_source_id)
        .execute(&mut *tx)
        .await?;

    let id = if let Some(row) = sqlx::query(r#"SELECT "Id" FROM "PlexCollection" WHERE "Key" = ?"#)
        .bind(&item.key)
        .fetch_optional(&mut *tx)
        .await?
    {
        let id = row.get("Id");
        sqlx::query(r#"UPDATE "PlexCollection" SET "Etag" = ?, "Name" = ? WHERE "Id" = ?"#)
            .bind(&item.etag)
            .bind(&item.title)
            .bind(id)
            .execute(&mut *tx)
            .await?;
        sqlx::query(r#"UPDATE "Collection" SET "Name" = ? WHERE "Id" = ?"#)
            .bind(&item.title)
            .bind(id)
            .execute(&mut *tx)
            .await?;
        id
    } else {
        let result = sqlx::query(
            r#"INSERT INTO "Collection" ("Name", "UseCustomPlaybackOrder") VALUES (?, 0)"#,
        )
        .bind(&item.title)
        .execute(&mut *tx)
        .await?;
        let id = result.last_insert_rowid();
        sqlx::query(
            r#"INSERT INTO "PlexCollection" ("Id", "Key", "Etag", "Name")
               VALUES (?, ?, ?, ?)"#,
        )
        .bind(id)
        .bind(&item.key)
        .bind(&item.etag)
        .bind(&item.title)
        .execute(&mut *tx)
        .await?;
        id
    };

    tx.commit().await?;
    Ok(id)
}

async fn replace_plex_collection_items(
    pool: &SqlitePool,
    collection_id: i64,
    items: &[PlexCollectionMediaItem],
) -> Result<()> {
    let mut tx = pool.begin().await?;
    sqlx::query(r#"DELETE FROM "CollectionItem" WHERE "CollectionId" = ?"#)
        .bind(collection_id)
        .execute(&mut *tx)
        .await?;
    for (index, item) in items.iter().enumerate() {
        if let Some(media_item_id) = find_plex_media_item(&mut tx, item).await? {
            sqlx::query(
                r#"INSERT INTO "CollectionItem" ("CollectionId", "MediaItemId", "CustomIndex")
                   VALUES (?, ?, ?)"#,
            )
            .bind(collection_id)
            .bind(media_item_id)
            .bind(i32::try_from(index).unwrap_or_default())
            .execute(&mut *tx)
            .await?;
        }
    }
    tx.commit().await?;
    Ok(())
}

async fn find_plex_media_item(
    tx: &mut Transaction<'_, Sqlite>,
    item: &PlexCollectionMediaItem,
) -> Result<Option<i64>> {
    let table = match item.kind {
        PlexCollectionMediaKind::Movie => "PlexMovie",
        PlexCollectionMediaKind::Show => "PlexShow",
        PlexCollectionMediaKind::Season => "PlexSeason",
        PlexCollectionMediaKind::Episode => "PlexEpisode",
    };
    let sql = format!(r#"SELECT "Id" FROM "{table}" WHERE "Key" = ?"#);
    Ok(sqlx::query(&sql)
        .bind(&item.key)
        .fetch_optional(&mut **tx)
        .await?
        .map(|row| row.get("Id")))
}

async fn ensure_simple_media_item(
    tx: &mut Transaction<'_, Sqlite>,
    base_table: &str,
    provider_table: &str,
    provider_key_column: &str,
    library_path_id: i64,
    key: &str,
    etag: Option<&String>,
) -> Result<i64> {
    let select_sql =
        format!(r#"SELECT "Id" FROM "{provider_table}" WHERE "{provider_key_column}" = ?"#);
    if let Some(row) = sqlx::query(&select_sql)
        .bind(key)
        .fetch_optional(&mut **tx)
        .await?
    {
        let id = row.get("Id");
        let update_sql = format!(r#"UPDATE "{provider_table}" SET "Etag" = ? WHERE "Id" = ?"#);
        sqlx::query(&update_sql)
            .bind(etag)
            .bind(id)
            .execute(&mut **tx)
            .await?;
        sqlx::query(r#"UPDATE "MediaItem" SET "LibraryPathId" = ?, "State" = 0 WHERE "Id" = ?"#)
            .bind(library_path_id)
            .bind(id)
            .execute(&mut **tx)
            .await?;
        return Ok(id);
    }
    let id = insert_media_item(tx, library_path_id).await?;
    let base_sql = format!(r#"INSERT INTO "{base_table}" ("Id") VALUES (?)"#);
    sqlx::query(&base_sql).bind(id).execute(&mut **tx).await?;
    let provider_sql = format!(
        r#"INSERT INTO "{provider_table}" ("Id", "{provider_key_column}", "Etag") VALUES (?, ?, ?)"#
    );
    sqlx::query(&provider_sql)
        .bind(id)
        .bind(key)
        .bind(etag)
        .execute(&mut **tx)
        .await?;
    Ok(id)
}

async fn insert_media_item(tx: &mut Transaction<'_, Sqlite>, library_path_id: i64) -> Result<i64> {
    let result = sqlx::query(r#"INSERT INTO "MediaItem" ("LibraryPathId", "State") VALUES (?, 0)"#)
        .bind(library_path_id)
        .execute(&mut **tx)
        .await?;
    Ok(result.last_insert_rowid())
}

#[derive(Debug, Clone, Copy)]
enum VersionOwner {
    Movie(i64),
    Episode(i64),
}

async fn ensure_media_version(
    tx: &mut Transaction<'_, Sqlite>,
    owner: VersionOwner,
    item: &PlexMediaItem,
) -> Result<i64> {
    let (column, item_id) = match owner {
        VersionOwner::Movie(item_id) => ("MovieId", item_id),
        VersionOwner::Episode(item_id) => ("EpisodeId", item_id),
    };
    let select_sql = format!(r#"SELECT "Id" FROM "MediaVersion" WHERE "{column}" = ?"#);
    if let Some(row) = sqlx::query(&select_sql)
        .bind(item_id)
        .fetch_optional(&mut **tx)
        .await?
    {
        let id = row.get("Id");
        update_media_version(tx, id, item).await?;
        return Ok(id);
    }
    let insert_sql = format!(
        r#"INSERT INTO "MediaVersion" (
            "DateAdded", "DateUpdated", "DisplayAspectRatio", "Duration", "Height",
            "Name", "{column}", "RFrameRate", "SampleAspectRatio", "VideoScanKind", "Width"
        ) VALUES (?, ?, ?, ?, ?, 'Main', ?, ?, ?, ?, ?)"#
    );
    let result = sqlx::query(&insert_sql)
        .bind(encode_datetime(item.date_added))
        .bind(encode_datetime(item.date_updated))
        .bind(&item.display_aspect_ratio)
        .bind(encode_duration(item.duration))
        .bind(item.height)
        .bind(item_id)
        .bind(&item.r_frame_rate)
        .bind(&item.sample_aspect_ratio)
        .bind(encode_video_scan_kind(item.video_scan_kind))
        .bind(item.width)
        .execute(&mut **tx)
        .await?;
    Ok(result.last_insert_rowid())
}

async fn update_media_version(
    tx: &mut Transaction<'_, Sqlite>,
    id: i64,
    item: &PlexMediaItem,
) -> Result<()> {
    sqlx::query(
        r#"UPDATE "MediaVersion"
           SET "DateAdded" = ?, "DateUpdated" = ?, "DisplayAspectRatio" = ?,
               "Duration" = ?, "Height" = ?, "RFrameRate" = ?,
               "SampleAspectRatio" = ?, "VideoScanKind" = ?, "Width" = ?
           WHERE "Id" = ?"#,
    )
    .bind(encode_datetime(item.date_added))
    .bind(encode_datetime(item.date_updated))
    .bind(&item.display_aspect_ratio)
    .bind(encode_duration(item.duration))
    .bind(item.height)
    .bind(&item.r_frame_rate)
    .bind(&item.sample_aspect_ratio)
    .bind(encode_video_scan_kind(item.video_scan_kind))
    .bind(item.width)
    .bind(id)
    .execute(&mut **tx)
    .await?;
    Ok(())
}

async fn ensure_media_file(
    tx: &mut Transaction<'_, Sqlite>,
    version_id: i64,
    item: &PlexMediaItem,
) -> Result<()> {
    let result = sqlx::query(
        r#"INSERT OR REPLACE INTO "MediaFile" ("MediaVersionId", "Path", "PathHash")
           VALUES (?, ?, ?)"#,
    )
    .bind(version_id)
    .bind(&item.path)
    .bind(path_hash(&item.path))
    .execute(&mut **tx)
    .await?;
    let file_id = result.last_insert_rowid();
    sqlx::query(
        r#"INSERT OR REPLACE INTO "PlexMediaFile" ("Id", "PlexId", "Key") VALUES (?, ?, ?)"#,
    )
    .bind(file_id)
    .bind(item.plex_part_id)
    .bind(&item.part_key)
    .execute(&mut **tx)
    .await?;
    Ok(())
}

async fn replace_streams(
    tx: &mut Transaction<'_, Sqlite>,
    version_id: i64,
    streams: &[MediaStream],
) -> Result<()> {
    sqlx::query(r#"DELETE FROM "MediaStream" WHERE "MediaVersionId" = ?"#)
        .bind(version_id)
        .execute(&mut **tx)
        .await?;
    for stream in streams {
        sqlx::query(
            r#"INSERT INTO "MediaStream" (
                "Index", "Codec", "MediaStreamKind", "Language", "Channels",
                "Title", "Default", "Forced", "MediaVersionId"
            ) VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?)"#,
        )
        .bind(stream.index)
        .bind(&stream.codec)
        .bind(encode_stream_kind(stream.kind))
        .bind(&stream.language)
        .bind(stream.channels.unwrap_or_default())
        .bind(&stream.title)
        .bind(stream.default)
        .bind(stream.forced)
        .bind(version_id)
        .execute(&mut **tx)
        .await?;
    }
    Ok(())
}

#[allow(clippy::needless_pass_by_value)]
fn provider_error(error: reqwest::Error) -> RepositoryError {
    RepositoryError::Decode {
        kind: "plex http",
        value: error.to_string(),
    }
}

fn unix_time(value: Option<i64>) -> DateTime<Utc> {
    Utc.timestamp_opt(value.unwrap_or_default(), 0)
        .single()
        .unwrap_or_else(Utc::now)
}

fn path_hash(path: &str) -> String {
    let mut hash = 0xcbf2_9ce4_8422_2325_u64;
    for byte in path.as_bytes() {
        hash ^= u64::from(*byte);
        hash = hash.wrapping_mul(0x0000_0100_0000_01b3);
    }
    format!("{hash:016x}")
}

fn encode_datetime(value: DateTime<Utc>) -> String {
    value.naive_utc().format("%Y-%m-%d %H:%M:%S").to_string()
}

fn encode_duration(value: Duration) -> String {
    let total_seconds = value.num_seconds().max(0);
    let hours = total_seconds / 3600;
    let minutes = (total_seconds % 3600) / 60;
    let seconds = total_seconds % 60;
    format!("{hours:02}:{minutes:02}:{seconds:02}")
}

fn encode_stream_kind(kind: MediaStreamKind) -> i32 {
    match kind {
        MediaStreamKind::Video => 0,
        MediaStreamKind::Audio => 1,
        MediaStreamKind::Subtitle => 2,
        MediaStreamKind::Attachment => 3,
        MediaStreamKind::ExternalSubtitle => 4,
    }
}

fn encode_video_scan_kind(kind: VideoScanKind) -> i32 {
    match kind {
        VideoScanKind::Progressive => 0,
        VideoScanKind::Interlaced => 1,
        VideoScanKind::Unknown => 2,
    }
}

#[derive(Debug, Deserialize)]
struct PlexLibrariesResponse {
    #[serde(rename = "MediaContainer")]
    media_container: PlexLibraryContainerResponse,
}

#[derive(Debug, Deserialize)]
struct PlexLibraryContainerResponse {
    #[serde(rename = "Directory", default)]
    directory: Vec<PlexLibraryResponse>,
}

#[derive(Debug, Deserialize)]
struct PlexLibraryResponse {
    #[serde(rename = "Key")]
    key: String,
    #[serde(rename = "Title")]
    title: String,
    #[serde(rename = "type")]
    library_type: String,
}

#[derive(Debug, Deserialize)]
struct PlexMetadataResponse {
    #[serde(rename = "MediaContainer")]
    media_container: PlexMetadataContainerResponse,
}

#[derive(Debug, Deserialize)]
struct PlexMetadataContainerResponse {
    #[serde(rename = "totalSize", default)]
    total_size: i32,
    #[serde(rename = "Metadata", default)]
    metadata: Vec<PlexMetadataItemResponse>,
}

#[derive(Debug, Deserialize)]
struct PlexCollectionResponse {
    #[serde(rename = "MediaContainer")]
    media_container: PlexCollectionContainerResponse,
}

#[derive(Debug, Deserialize)]
struct PlexCollectionContainerResponse {
    #[serde(rename = "totalSize", default)]
    total_size: i32,
    #[serde(rename = "Metadata", default)]
    metadata: Vec<PlexCollectionMetadataItemResponse>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct PlexCollectionMetadataItemResponse {
    rating_key: Option<String>,
    title: Option<String>,
    updated_at: Option<i64>,
    library_section_id: Option<i32>,
}

#[derive(Debug, Deserialize)]
struct PlexCollectionItemsResponse {
    #[serde(rename = "MediaContainer")]
    media_container: PlexCollectionItemsContainerResponse,
}

#[derive(Debug, Deserialize)]
struct PlexCollectionItemsContainerResponse {
    #[serde(rename = "totalSize", default)]
    total_size: i32,
    #[serde(rename = "Metadata", default)]
    metadata: Vec<PlexCollectionMediaItemResponse>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct PlexCollectionMediaItemResponse {
    key: Option<String>,
    rating_key: Option<String>,
    #[serde(rename = "type")]
    item_type: Option<String>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct PlexMetadataItemResponse {
    key: Option<String>,
    rating_key: Option<String>,
    title: Option<String>,
    #[serde(default)]
    media: Vec<PlexMediaResponse>,
    added_at: Option<i64>,
    updated_at: Option<i64>,
    index: Option<i32>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct PlexMediaResponse {
    #[serde(default)]
    id: i32,
    #[serde(default)]
    duration: i32,
    #[serde(default)]
    width: i32,
    #[serde(default)]
    height: i32,
    #[serde(default)]
    aspect_ratio: f64,
    #[serde(default)]
    part: Vec<PlexPartResponse>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct PlexPartResponse {
    #[serde(default)]
    id: i32,
    key: Option<String>,
    file: Option<String>,
    #[serde(default)]
    stream: Vec<PlexStreamResponse>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct PlexStreamResponse {
    #[serde(default)]
    id: i32,
    index: Option<i32>,
    key: Option<String>,
    #[serde(default)]
    default: bool,
    #[serde(default)]
    forced: bool,
    #[serde(default)]
    embedded_in_video: bool,
    language_code: Option<String>,
    title: Option<String>,
    stream_type: Option<i32>,
    codec: Option<String>,
    channels: Option<i32>,
    width: Option<i32>,
    height: Option<i32>,
    pixel_aspect_ratio: Option<String>,
    scan_type: Option<String>,
    frame_rate: Option<String>,
}
