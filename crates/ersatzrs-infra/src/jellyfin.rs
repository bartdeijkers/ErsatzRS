use crate::db::DbPool;
use crate::repositories::common::decode_enum_column;
use async_trait::async_trait;
use chrono::{DateTime, Duration, Utc};
use ersatzrs_core::repositories::{
    JellyfinCollectionItem, JellyfinCollectionMediaItem, JellyfinCollectionMediaKind,
    JellyfinCollectionMediaPage, JellyfinCollectionPage, JellyfinConnectionConfig,
    JellyfinItemPage, JellyfinLibrarySummary, JellyfinMediaItem, JellyfinPlaybackMetadata,
    JellyfinSeasonItem, JellyfinSeasonPage, JellyfinService as JellyfinServiceTrait,
    JellyfinShowItem, JellyfinShowPage, RepositoryError, Result,
};
use ersatzrs_domain::{
    LibraryMediaKind, MediaChapter, MediaStream, MediaStreamKind, VideoScanKind,
};
use reqwest::Client;
use serde::Deserialize;
use sha2::{Digest, Sha256};
use sqlx::{Row, Transaction};

const JELLYFIN_PAGE_SIZE: i32 = 100;

#[derive(Debug, Clone)]
pub struct JellyfinService {
    client: Client,
}

impl Default for JellyfinService {
    fn default() -> Self {
        Self::new()
    }
}

impl JellyfinService {
    #[must_use]
    pub fn new() -> Self {
        Self {
            client: Client::new(),
        }
    }
}

#[async_trait]
impl JellyfinServiceTrait for JellyfinService {
    async fn list_libraries(
        &self,
        config: &JellyfinConnectionConfig,
    ) -> Result<Vec<JellyfinLibrarySummary>> {
        let url = format!(
            "{}/Library/VirtualFolders",
            config.address.trim_end_matches('/')
        );
        let response: Vec<JellyfinLibraryResponse> = self
            .client
            .get(url)
            .header("Authorization", &config.authorization_header)
            .send()
            .await
            .map_err(provider_error)?
            .error_for_status()
            .map_err(provider_error)?
            .json()
            .await
            .map_err(provider_error)?;

        Ok(response.into_iter().filter_map(project_library).collect())
    }

    async fn fetch_movie_page(
        &self,
        config: &JellyfinConnectionConfig,
        parent_id: &str,
        start_index: i32,
        limit: i32,
    ) -> Result<JellyfinItemPage> {
        let url = format!("{}/Items", config.address.trim_end_matches('/'));
        let start_index = start_index.to_string();
        let limit = limit.to_string();
        let response: JellyfinLibraryItemsResponse = self
            .client
            .get(url)
            .header("Authorization", &config.authorization_header)
            .query(&[
                ("sortOrder", "Ascending"),
                ("sortBy", "SortName"),
                ("parentId", parent_id),
                (
                    "fields",
                    "Path,Genres,Tags,DateCreated,Etag,Overview,Taglines,Studios,People,OfficialRating,ProviderIds,Chapters",
                ),
                ("includeItemTypes", "Movie"),
                ("recursive", "true"),
                ("filters", "IsNotFolder"),
                ("startIndex", &start_index),
                ("limit", &limit),
            ])
            .send()
            .await
            .map_err(provider_error)?
            .error_for_status()
            .map_err(provider_error)?
            .json()
            .await
            .map_err(provider_error)?;

        Ok(JellyfinItemPage {
            total_record_count: response.total_record_count,
            items: response
                .items
                .into_iter()
                .filter_map(project_movie)
                .collect(),
        })
    }

    async fn fetch_show_page(
        &self,
        config: &JellyfinConnectionConfig,
        parent_id: &str,
        start_index: i32,
        limit: i32,
    ) -> Result<JellyfinShowPage> {
        let response = self
            .fetch_items(
                config,
                parent_id,
                "Path,Genres,Tags,DateCreated,Etag,Overview,Taglines,Studios,OfficialRating,ProviderIds",
                "Series",
                start_index,
                limit,
            )
            .await?;
        Ok(JellyfinShowPage {
            total_record_count: response.total_record_count,
            items: response
                .items
                .into_iter()
                .filter_map(project_show)
                .collect(),
        })
    }

    async fn fetch_season_page(
        &self,
        config: &JellyfinConnectionConfig,
        parent_id: &str,
        start_index: i32,
        limit: i32,
    ) -> Result<JellyfinSeasonPage> {
        let response = self
            .fetch_items(
                config,
                parent_id,
                "Path,DateCreated,Etag,Taglines,ProviderIds",
                "Season",
                start_index,
                limit,
            )
            .await?;
        Ok(JellyfinSeasonPage {
            total_record_count: response.total_record_count,
            items: response
                .items
                .into_iter()
                .filter_map(project_season)
                .collect(),
        })
    }

    async fn fetch_episode_page(
        &self,
        config: &JellyfinConnectionConfig,
        parent_id: &str,
        start_index: i32,
        limit: i32,
    ) -> Result<JellyfinItemPage> {
        let response = self
            .fetch_items(
                config,
                parent_id,
                "Path,Genres,Tags,DateCreated,Etag,Overview,ProviderIds,People,Chapters",
                "Episode",
                start_index,
                limit,
            )
            .await?;
        Ok(JellyfinItemPage {
            total_record_count: response.total_record_count,
            items: response
                .items
                .into_iter()
                .filter_map(project_movie)
                .collect(),
        })
    }

    async fn fetch_collection_page(
        &self,
        config: &JellyfinConnectionConfig,
        parent_id: &str,
        start_index: i32,
        limit: i32,
    ) -> Result<JellyfinCollectionPage> {
        let response = self
            .fetch_items(config, parent_id, "Etag", "BoxSet", start_index, limit)
            .await?;
        Ok(JellyfinCollectionPage {
            total_record_count: response.total_record_count,
            items: response
                .items
                .into_iter()
                .filter_map(project_collection)
                .collect(),
        })
    }

    async fn fetch_collection_items_page(
        &self,
        config: &JellyfinConnectionConfig,
        parent_id: &str,
        start_index: i32,
        limit: i32,
    ) -> Result<JellyfinCollectionMediaPage> {
        let response = self
            .fetch_items(
                config,
                parent_id,
                "Etag",
                "Movie,Series,Season,Episode",
                start_index,
                limit,
            )
            .await?;
        Ok(JellyfinCollectionMediaPage {
            total_record_count: response.total_record_count,
            items: response
                .items
                .into_iter()
                .filter_map(project_collection_media_item)
                .collect(),
        })
    }

    async fn stream_metadata(
        &self,
        config: &JellyfinConnectionConfig,
        item_id: &str,
    ) -> Result<JellyfinPlaybackMetadata> {
        let url = format!(
            "{}/Items/{}/PlaybackInfo",
            config.address.trim_end_matches('/'),
            item_id
        );
        let response: JellyfinPlaybackInfoResponse = self
            .client
            .get(url)
            .header("Authorization", &config.authorization_header)
            .send()
            .await
            .map_err(provider_error)?
            .error_for_status()
            .map_err(provider_error)?
            .json()
            .await
            .map_err(provider_error)?;

        project_playback(response).ok_or_else(|| RepositoryError::Decode {
            kind: "jellyfin playback info",
            value: "missing media source".to_owned(),
        })
    }
}

impl JellyfinService {
    async fn fetch_items(
        &self,
        config: &JellyfinConnectionConfig,
        parent_id: &str,
        fields: &str,
        include_item_types: &str,
        start_index: i32,
        limit: i32,
    ) -> Result<JellyfinLibraryItemsResponse> {
        let url = format!("{}/Items", config.address.trim_end_matches('/'));
        let start_index = start_index.to_string();
        let limit = limit.to_string();
        self.client
            .get(url)
            .header("Authorization", &config.authorization_header)
            .query(&[
                ("sortOrder", "Ascending"),
                ("sortBy", "SortName"),
                ("parentId", parent_id),
                ("fields", fields),
                ("includeItemTypes", include_item_types),
                ("recursive", "true"),
                ("startIndex", &start_index),
                ("limit", &limit),
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
}

/// Fetch Jellyfin movie pages and write mapped movie rows.
///
/// # Errors
///
/// Returns an error when the HTTP client fails or any database write fails.
pub async fn sync_jellyfin_movie_library<S>(
    pool: &DbPool,
    service: &S,
    config: &JellyfinConnectionConfig,
    media_source_id: i64,
    library_id: i64,
    library_item_id: &str,
    deep_scan: bool,
) -> Result<usize>
where
    S: JellyfinServiceTrait,
{
    let mut synced = 0;
    let mut start_index = 0;

    loop {
        let page = service
            .fetch_movie_page(config, library_item_id, start_index, JELLYFIN_PAGE_SIZE)
            .await?;
        let total_record_count = page.total_record_count;
        let page_len = i32::try_from(page.items.len()).unwrap_or(i32::MAX);
        for item in &page.items {
            if !deep_scan
                && item_etag_matches(
                    pool,
                    "JellyfinMovie",
                    library_id,
                    &item.item_id,
                    item.etag.as_deref(),
                )
                .await?
            {
                synced += 1;
                continue;
            }

            let playback = service.stream_metadata(config, &item.item_id).await?;
            upsert_jellyfin_movie(
                pool,
                media_source_id,
                library_id,
                library_item_id,
                item,
                &playback,
            )
            .await?;
            synced += 1;
        }

        start_index += JELLYFIN_PAGE_SIZE;
        if start_index >= total_record_count || page_len == 0 {
            break;
        }
    }

    Ok(synced)
}

/// Fetch Jellyfin shows, seasons, and episodes and write mapped media rows.
///
/// # Errors
///
/// Returns an error when the HTTP client fails or any database write fails.
pub async fn sync_jellyfin_show_library<S>(
    pool: &DbPool,
    service: &S,
    config: &JellyfinConnectionConfig,
    media_source_id: i64,
    library_id: i64,
    library_item_id: &str,
    deep_scan: bool,
) -> Result<usize>
where
    S: JellyfinServiceTrait,
{
    let mut synced = 0;
    let mut show_start_index = 0;

    loop {
        let shows = service
            .fetch_show_page(
                config,
                library_item_id,
                show_start_index,
                JELLYFIN_PAGE_SIZE,
            )
            .await?;
        let show_total_count = shows.total_record_count;
        let show_page_len = i32::try_from(shows.items.len()).unwrap_or(i32::MAX);
        for show in &shows.items {
            let show_id =
                upsert_jellyfin_show(pool, media_source_id, library_id, library_item_id, show)
                    .await?;
            let mut season_start_index = 0;
            loop {
                let seasons = service
                    .fetch_season_page(
                        config,
                        &show.item_id,
                        season_start_index,
                        JELLYFIN_PAGE_SIZE,
                    )
                    .await?;
                let season_total_count = seasons.total_record_count;
                let season_page_len = i32::try_from(seasons.items.len()).unwrap_or(i32::MAX);
                for season in &seasons.items {
                    let season_id = upsert_jellyfin_season(
                        pool,
                        media_source_id,
                        library_id,
                        library_item_id,
                        show_id,
                        season,
                    )
                    .await?;
                    let mut episode_start_index = 0;
                    loop {
                        let episodes = service
                            .fetch_episode_page(
                                config,
                                &season.item_id,
                                episode_start_index,
                                JELLYFIN_PAGE_SIZE,
                            )
                            .await?;
                        let episode_total_count = episodes.total_record_count;
                        let episode_page_len =
                            i32::try_from(episodes.items.len()).unwrap_or(i32::MAX);
                        for episode in &episodes.items {
                            if !deep_scan
                                && item_etag_matches(
                                    pool,
                                    "JellyfinEpisode",
                                    library_id,
                                    &episode.item_id,
                                    episode.etag.as_deref(),
                                )
                                .await?
                            {
                                synced += 1;
                                continue;
                            }

                            let playback =
                                service.stream_metadata(config, &episode.item_id).await?;
                            upsert_jellyfin_episode(
                                pool,
                                media_source_id,
                                library_id,
                                library_item_id,
                                season_id,
                                episode,
                                &playback,
                            )
                            .await?;
                            synced += 1;
                        }

                        episode_start_index += JELLYFIN_PAGE_SIZE;
                        if episode_start_index >= episode_total_count || episode_page_len == 0 {
                            break;
                        }
                    }
                }

                season_start_index += JELLYFIN_PAGE_SIZE;
                if season_start_index >= season_total_count || season_page_len == 0 {
                    break;
                }
            }
        }

        show_start_index += JELLYFIN_PAGE_SIZE;
        if show_start_index >= show_total_count || show_page_len == 0 {
            break;
        }
    }

    Ok(synced)
}

/// Fetch Jellyfin collections and write collection membership rows.
///
/// # Errors
///
/// Returns an error when the HTTP client fails or any database write fails.
pub async fn sync_jellyfin_collection_library<S>(
    pool: &DbPool,
    service: &S,
    config: &JellyfinConnectionConfig,
    media_source_id: i64,
    parent_id: &str,
) -> Result<usize>
where
    S: JellyfinServiceTrait,
{
    let mut synced = 0;
    let mut collection_start_index = 0;

    loop {
        let page = service
            .fetch_collection_page(
                config,
                parent_id,
                collection_start_index,
                JELLYFIN_PAGE_SIZE,
            )
            .await?;
        let total_record_count = page.total_record_count;
        let page_len = i32::try_from(page.items.len()).unwrap_or(i32::MAX);
        for collection in &page.items {
            let collection_id =
                upsert_jellyfin_collection(pool, media_source_id, collection).await?;
            let mut item_start_index = 0;
            let mut all_items = Vec::new();

            loop {
                let items = service
                    .fetch_collection_items_page(
                        config,
                        &collection.item_id,
                        item_start_index,
                        JELLYFIN_PAGE_SIZE,
                    )
                    .await?;
                let item_total_count = items.total_record_count;
                let item_page_len = i32::try_from(items.items.len()).unwrap_or(i32::MAX);
                all_items.extend(items.items);

                item_start_index += JELLYFIN_PAGE_SIZE;
                if item_start_index >= item_total_count || item_page_len == 0 {
                    break;
                }
            }

            replace_collection_items(pool, collection_id, &all_items).await?;
            synced += 1;
        }

        collection_start_index += JELLYFIN_PAGE_SIZE;
        if collection_start_index >= total_record_count || page_len == 0 {
            break;
        }
    }

    Ok(synced)
}

fn project_library(response: JellyfinLibraryResponse) -> Option<JellyfinLibrarySummary> {
    let media_kind = match response.collection_type?.to_ascii_lowercase().as_str() {
        "movies" => LibraryMediaKind::Movies,
        "tvshows" => LibraryMediaKind::Shows,
        _ => return None,
    };
    Some(JellyfinLibrarySummary {
        item_id: response.item_id?,
        name: response.name.unwrap_or_default(),
        media_kind,
    })
}

/// Loads configuration from the database and synchronizes one Jellyfin library.
///
/// Dispatches to movie, show, or collection sync based on `MediaKind`.
///
/// # Errors
///
/// Returns an error when config loading, the Jellyfin API, or database writes fail.
pub async fn sync_library_by_id(pool: &DbPool, library_id: i64) -> Result<usize> {
    sync_library_by_id_with_options(pool, library_id, false).await
}

/// Loads configuration from the database and synchronizes one Jellyfin library.
///
/// `deep_scan` bypasses provider etag skip and refreshes existing rows even
/// when the upstream etag is unchanged.
///
/// # Errors
///
/// Returns an error when config loading, the Jellyfin API, or database writes fail.
pub async fn sync_library_by_id_with_options(
    pool: &DbPool,
    library_id: i64,
    deep_scan: bool,
) -> Result<usize> {
    let row = sqlx::query(
        r#"SELECT l."MediaKind", l."MediaSourceId"
           FROM "Library" l WHERE l."Id" = ? LIMIT 1"#,
    )
    .bind(library_id)
    .fetch_optional(pool)
    .await?
    .ok_or_else(|| RepositoryError::Decode {
        kind: "jellyfin library",
        value: format!("library {library_id} not found"),
    })?;
    let kind: LibraryMediaKind =
        decode_enum_column("jellyfin library media kind", &row, "MediaKind")?;
    let media_source_id: i64 = row.try_get("MediaSourceId").unwrap_or(0);

    let address: String = sqlx::query_scalar(
        r#"SELECT "Address" FROM "JellyfinConnection"
           WHERE "JellyfinMediaSourceId" = ? ORDER BY "Id" LIMIT 1"#,
    )
    .bind(media_source_id)
    .fetch_optional(pool)
    .await?
    .and_then(|v: Option<String>| v)
    .ok_or_else(|| RepositoryError::Decode {
        kind: "jellyfin connection",
        value: format!("no connection for source {media_source_id}"),
    })?;

    let api_key = load_jellyfin_api_key(pool, &address).await?;
    let authorization_header = format_jellyfin_auth_header(api_key);
    let config = JellyfinConnectionConfig {
        address,
        authorization_header,
    };

    let parent_id: String =
        sqlx::query_scalar(r#"SELECT "ItemId" FROM "JellyfinLibrary" WHERE "Id" = ?"#)
            .bind(library_id)
            .fetch_optional(pool)
            .await?
            .and_then(|v: Option<String>| v)
            .ok_or_else(|| RepositoryError::Decode {
                kind: "jellyfin library",
                value: format!("JellyfinLibrary row {library_id} not found"),
            })?;

    let service = JellyfinService::new();
    if kind == LibraryMediaKind::Shows {
        sync_jellyfin_show_library(
            pool,
            &service,
            &config,
            media_source_id,
            library_id,
            &parent_id,
            deep_scan,
        )
        .await
    } else {
        sync_jellyfin_movie_library(
            pool,
            &service,
            &config,
            media_source_id,
            library_id,
            &parent_id,
            deep_scan,
        )
        .await
    }
}

async fn load_jellyfin_api_key(pool: &DbPool, address: &str) -> Result<String> {
    let keys = [
        format!("jellyfin.authorization_header.{address}"),
        "jellyfin.current_authorization_header".to_owned(),
        format!("jellyfin.api_key.{address}"),
        "jellyfin.current_server_api_key".to_owned(),
    ];
    for key in &keys {
        if let Some(v) = sqlx::query_scalar::<_, String>(
            r#"SELECT "Value" FROM "ConfigElement" WHERE "Key" = ?"#,
        )
        .bind(key)
        .fetch_optional(pool)
        .await?
        .filter(|v| !v.is_empty())
        {
            return Ok(v);
        }
    }
    Err(RepositoryError::Decode {
        kind: "jellyfin config",
        value: "api key not found".into(),
    })
}

fn format_jellyfin_auth_header(value: String) -> String {
    if value.contains("Token=") || value.starts_with("MediaBrowser ") {
        value
    } else {
        format!("MediaBrowser Token=\"{value}\"")
    }
}

fn project_show(item: JellyfinLibraryItemResponse) -> Option<JellyfinShowItem> {
    Some(JellyfinShowItem {
        item_id: item.id?,
        etag: item.etag,
        name: item.name.unwrap_or_default(),
    })
}

fn project_season(item: JellyfinLibraryItemResponse) -> Option<JellyfinSeasonItem> {
    Some(JellyfinSeasonItem {
        item_id: item.id?,
        etag: item.etag,
        name: item.name.unwrap_or_default(),
        season_number: item.index_number.unwrap_or_default(),
    })
}

fn project_collection(item: JellyfinLibraryItemResponse) -> Option<JellyfinCollectionItem> {
    Some(JellyfinCollectionItem {
        item_id: item.id?,
        etag: item.etag,
        name: item.name.unwrap_or_default(),
    })
}

fn project_collection_media_item(
    item: JellyfinLibraryItemResponse,
) -> Option<JellyfinCollectionMediaItem> {
    if item.location_type.as_deref() != Some("FileSystem") {
        return None;
    }
    let kind = match item.item_type.as_deref()? {
        "Movie" => JellyfinCollectionMediaKind::Movie,
        "Series" => JellyfinCollectionMediaKind::Show,
        "Season" => JellyfinCollectionMediaKind::Season,
        "Episode" => JellyfinCollectionMediaKind::Episode,
        _ => return None,
    };
    Some(JellyfinCollectionMediaItem {
        item_id: item.id?,
        kind,
    })
}

fn project_movie(item: JellyfinLibraryItemResponse) -> Option<JellyfinMediaItem> {
    if item.location_type.as_deref() != Some("FileSystem") {
        return None;
    }
    let path = item.path?;
    if path.to_ascii_lowercase().ends_with(".strm") {
        return None;
    }
    let duration = Duration::nanoseconds(item.run_time_ticks * 100);
    Some(JellyfinMediaItem {
        item_id: item.id?,
        etag: item.etag,
        name: item.name.unwrap_or_default(),
        path,
        date_created: item.date_created.unwrap_or_else(Utc::now),
        duration,
        chapters: project_chapters(item.chapters, duration),
    })
}

fn project_chapters(
    chapters: Vec<JellyfinChapterResponse>,
    duration: Duration,
) -> Vec<MediaChapter> {
    let mut chapters: Vec<MediaChapter> = chapters
        .into_iter()
        .enumerate()
        .map(|(index, chapter)| MediaChapter {
            chapter_id: i64::try_from(index).unwrap_or_default(),
            title: chapter.name.unwrap_or_default(),
            start_time: Duration::nanoseconds(chapter.start_position_ticks * 100),
            ..MediaChapter::default()
        })
        .collect();
    chapters.sort_by_key(|chapter| chapter.start_time);
    for index in 0..chapters.len() {
        chapters[index].chapter_id = i64::try_from(index).unwrap_or_default();
        chapters[index].end_time = chapters
            .get(index + 1)
            .map_or(duration, |next| next.start_time);
    }
    chapters
}

fn project_playback(response: JellyfinPlaybackInfoResponse) -> Option<JellyfinPlaybackMetadata> {
    let media_source = response.media_sources.into_iter().next()?;
    let video = media_source
        .media_streams
        .iter()
        .find(|stream| stream.stream_type.as_deref() == Some("Video"))?;
    let width = video.width.unwrap_or(1);
    let height = video.height.unwrap_or(1);
    let display_aspect_ratio = video.aspect_ratio.clone().filter(|value| !value.is_empty());
    let sample_aspect_ratio = Some(
        if display_aspect_ratio
            .as_deref()
            .is_some_and(|aspect| is_anamorphic(width, height, aspect))
        {
            "0:0".to_owned()
        } else {
            "1:1".to_owned()
        },
    );

    Some(JellyfinPlaybackMetadata {
        duration: Duration::nanoseconds(media_source.run_time_ticks * 100),
        width,
        height,
        sample_aspect_ratio,
        display_aspect_ratio,
        r_frame_rate: video.real_frame_rate.map(|rate| format!("{rate:.2}")),
        streams: media_source
            .media_streams
            .into_iter()
            .filter_map(project_stream)
            .collect(),
    })
}

fn project_stream(stream: JellyfinMediaStreamResponse) -> Option<MediaStream> {
    let kind = match stream.stream_type.as_deref()? {
        "Video" => MediaStreamKind::Video,
        "Audio" => MediaStreamKind::Audio,
        "Subtitle" if stream.is_external => MediaStreamKind::ExternalSubtitle,
        "Subtitle" => MediaStreamKind::Subtitle,
        _ => return None,
    };

    Some(MediaStream {
        index: stream.index,
        kind,
        codec: stream.codec.map(|codec| codec.to_ascii_lowercase()),
        language: stream.language,
        title: stream.title,
        default: stream.is_default,
        forced: stream.is_forced,
        channels: stream.channels,
        width: stream.width,
        height: stream.height,
        ..MediaStream::default()
    })
}

fn is_anamorphic(width: i32, height: i32, aspect_ratio: &str) -> bool {
    let Some((num, den)) = aspect_ratio.split_once(':') else {
        return false;
    };
    let Ok(num) = num.parse::<f64>() else {
        return false;
    };
    let Ok(den) = den.parse::<f64>() else {
        return false;
    };
    den != 0.0 && ((f64::from(width) / f64::from(height)) - (num / den)).abs() > 0.01
}

async fn upsert_jellyfin_movie(
    pool: &DbPool,
    media_source_id: i64,
    library_id: i64,
    library_item_id: &str,
    item: &JellyfinMediaItem,
    playback: &JellyfinPlaybackMetadata,
) -> Result<()> {
    let mut tx = pool.begin().await?;
    ensure_jellyfin_library(&mut tx, media_source_id, library_id, library_item_id).await?;
    let library_path_id =
        ensure_library_path(&mut tx, library_id, &format!("jellyfin://{library_id}")).await?;
    let item_id = ensure_jellyfin_movie_item(&mut tx, library_path_id, item).await?;
    let version_id =
        ensure_media_version(&mut tx, VersionOwner::Movie(item_id), item, playback).await?;
    ensure_media_file(&mut tx, version_id, &item.path).await?;
    replace_streams(&mut tx, version_id, &playback.streams).await?;
    replace_chapters(&mut tx, version_id, &item.chapters).await?;
    tx.commit().await?;
    Ok(())
}

async fn upsert_jellyfin_show(
    pool: &DbPool,
    media_source_id: i64,
    library_id: i64,
    library_item_id: &str,
    item: &JellyfinShowItem,
) -> Result<i64> {
    let mut tx = pool.begin().await?;
    ensure_jellyfin_library(&mut tx, media_source_id, library_id, library_item_id).await?;
    let library_path_id =
        ensure_library_path(&mut tx, library_id, &format!("jellyfin://{library_id}")).await?;
    let id = ensure_jellyfin_show_item(&mut tx, library_path_id, item).await?;
    tx.commit().await?;
    Ok(id)
}

async fn upsert_jellyfin_season(
    pool: &DbPool,
    media_source_id: i64,
    library_id: i64,
    library_item_id: &str,
    show_id: i64,
    item: &JellyfinSeasonItem,
) -> Result<i64> {
    let mut tx = pool.begin().await?;
    ensure_jellyfin_library(&mut tx, media_source_id, library_id, library_item_id).await?;
    let library_path_id =
        ensure_library_path(&mut tx, library_id, &format!("jellyfin://{library_id}")).await?;
    let id = ensure_jellyfin_season_item(&mut tx, library_path_id, show_id, item).await?;
    tx.commit().await?;
    Ok(id)
}

async fn upsert_jellyfin_episode(
    pool: &DbPool,
    media_source_id: i64,
    library_id: i64,
    library_item_id: &str,
    season_id: i64,
    item: &JellyfinMediaItem,
    playback: &JellyfinPlaybackMetadata,
) -> Result<()> {
    let mut tx = pool.begin().await?;
    ensure_jellyfin_library(&mut tx, media_source_id, library_id, library_item_id).await?;
    let library_path_id =
        ensure_library_path(&mut tx, library_id, &format!("jellyfin://{library_id}")).await?;
    let item_id = ensure_jellyfin_episode_item(&mut tx, library_path_id, season_id, item).await?;
    let version_id =
        ensure_media_version(&mut tx, VersionOwner::Episode(item_id), item, playback).await?;
    ensure_media_file(&mut tx, version_id, &item.path).await?;
    replace_streams(&mut tx, version_id, &playback.streams).await?;
    replace_chapters(&mut tx, version_id, &item.chapters).await?;
    tx.commit().await?;
    Ok(())
}

async fn upsert_jellyfin_collection(
    pool: &DbPool,
    media_source_id: i64,
    item: &JellyfinCollectionItem,
) -> Result<i64> {
    let mut tx = pool.begin().await?;
    sqlx::query(r#"INSERT OR IGNORE INTO "MediaSource" ("Id") VALUES (?)"#)
        .bind(media_source_id)
        .execute(&mut *tx)
        .await?;
    sqlx::query(r#"INSERT OR IGNORE INTO "JellyfinMediaSource" ("Id") VALUES (?)"#)
        .bind(media_source_id)
        .execute(&mut *tx)
        .await?;

    let id = if let Some(row) =
        sqlx::query(r#"SELECT "Id" FROM "JellyfinCollection" WHERE "ItemId" = ?"#)
            .bind(&item.item_id)
            .fetch_optional(&mut *tx)
            .await?
    {
        let id = row.get("Id");
        sqlx::query(r#"UPDATE "JellyfinCollection" SET "Etag" = ?, "Name" = ? WHERE "Id" = ?"#)
            .bind(&item.etag)
            .bind(&item.name)
            .bind(id)
            .execute(&mut *tx)
            .await?;
        sqlx::query(r#"UPDATE "Collection" SET "Name" = ? WHERE "Id" = ?"#)
            .bind(&item.name)
            .bind(id)
            .execute(&mut *tx)
            .await?;
        id
    } else {
        let result = sqlx::query(
            r#"INSERT INTO "Collection" ("Name", "UseCustomPlaybackOrder") VALUES (?, 0)"#,
        )
        .bind(&item.name)
        .execute(&mut *tx)
        .await?;
        let id = crate::db::last_insert_id(result);
        sqlx::query(
            r#"INSERT INTO "JellyfinCollection" ("Id", "ItemId", "Etag", "Name")
               VALUES (?, ?, ?, ?)"#,
        )
        .bind(id)
        .bind(&item.item_id)
        .bind(&item.etag)
        .bind(&item.name)
        .execute(&mut *tx)
        .await?;
        id
    };

    tx.commit().await?;
    Ok(id)
}

async fn replace_collection_items(
    pool: &DbPool,
    collection_id: i64,
    items: &[JellyfinCollectionMediaItem],
) -> Result<()> {
    let mut tx = pool.begin().await?;
    sqlx::query(r#"DELETE FROM "CollectionItem" WHERE "CollectionId" = ?"#)
        .bind(collection_id)
        .execute(&mut *tx)
        .await?;
    for (index, item) in items.iter().enumerate() {
        if let Some(media_item_id) = find_jellyfin_media_item(&mut tx, item).await? {
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

async fn find_jellyfin_media_item(
    tx: &mut Transaction<'_, crate::db::Db>,
    item: &JellyfinCollectionMediaItem,
) -> Result<Option<i64>> {
    let table = match item.kind {
        JellyfinCollectionMediaKind::Movie => "JellyfinMovie",
        JellyfinCollectionMediaKind::Show => "JellyfinShow",
        JellyfinCollectionMediaKind::Season => "JellyfinSeason",
        JellyfinCollectionMediaKind::Episode => "JellyfinEpisode",
    };
    let sql = format!(r#"SELECT "Id" FROM "{table}" WHERE "ItemId" = ?"#);
    Ok(sqlx::query(&sql)
        .bind(&item.item_id)
        .fetch_optional(&mut **tx)
        .await?
        .map(|row| row.get("Id")))
}

async fn item_etag_matches(
    pool: &DbPool,
    table: &str,
    library_id: i64,
    item_id: &str,
    etag: Option<&str>,
) -> Result<bool> {
    let sql = format!(
        r#"SELECT "{table}"."Etag" AS "Etag", "MediaItem"."State" AS "State"
           FROM "{table}"
           INNER JOIN "MediaItem" ON "MediaItem"."Id" = "{table}"."Id"
           INNER JOIN "LibraryPath" ON "LibraryPath"."Id" = "MediaItem"."LibraryPathId"
           WHERE "{table}"."ItemId" = ? AND "LibraryPath"."LibraryId" = ?"#
    );
    let Some(row) = sqlx::query(&sql)
        .bind(item_id)
        .bind(library_id)
        .fetch_optional(pool)
        .await?
    else {
        return Ok(false);
    };

    let existing_etag: Option<String> = row.try_get("Etag")?;
    let state: i64 = row.try_get("State")?;
    Ok(state == 0 && existing_etag.as_deref() == etag)
}

async fn ensure_jellyfin_library(
    tx: &mut Transaction<'_, crate::db::Db>,
    media_source_id: i64,
    library_id: i64,
    library_item_id: &str,
) -> Result<()> {
    sqlx::query(r#"INSERT OR IGNORE INTO "MediaSource" ("Id") VALUES (?)"#)
        .bind(media_source_id)
        .execute(&mut **tx)
        .await?;
    sqlx::query(r#"INSERT OR IGNORE INTO "JellyfinMediaSource" ("Id") VALUES (?)"#)
        .bind(media_source_id)
        .execute(&mut **tx)
        .await?;
    sqlx::query(
        r#"INSERT OR IGNORE INTO "Library" ("Id", "Name", "MediaKind", "MediaSourceId")
           VALUES (?, 'Jellyfin', 0, ?)"#,
    )
    .bind(library_id)
    .bind(media_source_id)
    .execute(&mut **tx)
    .await?;
    sqlx::query(
        r#"INSERT OR IGNORE INTO "JellyfinLibrary" ("Id", "ItemId", "ShouldSyncItems")
           VALUES (?, ?, 1)"#,
    )
    .bind(library_id)
    .bind(library_item_id)
    .execute(&mut **tx)
    .await?;
    Ok(())
}

async fn ensure_library_path(
    tx: &mut Transaction<'_, crate::db::Db>,
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
    Ok(crate::db::last_insert_id(result))
}

async fn ensure_jellyfin_movie_item(
    tx: &mut Transaction<'_, crate::db::Db>,
    library_path_id: i64,
    item: &JellyfinMediaItem,
) -> Result<i64> {
    if let Some(row) = sqlx::query(
        r#"SELECT "JellyfinMovie"."Id"
           FROM "JellyfinMovie"
           INNER JOIN "MediaItem" ON "MediaItem"."Id" = "JellyfinMovie"."Id"
           INNER JOIN "LibraryPath" existing_path
             ON existing_path."Id" = "MediaItem"."LibraryPathId"
           INNER JOIN "LibraryPath" current_path ON current_path."Id" = ?
           WHERE "JellyfinMovie"."ItemId" = ?
             AND existing_path."LibraryId" = current_path."LibraryId""#,
    )
    .bind(library_path_id)
    .bind(&item.item_id)
    .fetch_optional(&mut **tx)
    .await?
    {
        let id = row.get("Id");
        sqlx::query(r#"UPDATE "JellyfinMovie" SET "Etag" = ? WHERE "Id" = ?"#)
            .bind(&item.etag)
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

    let result = sqlx::query(r#"INSERT INTO "MediaItem" ("LibraryPathId", "State") VALUES (?, 0)"#)
        .bind(library_path_id)
        .execute(&mut **tx)
        .await?;
    let id = crate::db::last_insert_id(result);
    sqlx::query(r#"INSERT INTO "Movie" ("Id") VALUES (?)"#)
        .bind(id)
        .execute(&mut **tx)
        .await?;
    sqlx::query(r#"INSERT INTO "JellyfinMovie" ("Id", "ItemId", "Etag") VALUES (?, ?, ?)"#)
        .bind(id)
        .bind(&item.item_id)
        .bind(&item.etag)
        .execute(&mut **tx)
        .await?;
    Ok(id)
}

async fn ensure_jellyfin_show_item(
    tx: &mut Transaction<'_, crate::db::Db>,
    library_path_id: i64,
    item: &JellyfinShowItem,
) -> Result<i64> {
    if let Some(row) = sqlx::query(
        r#"SELECT "JellyfinShow"."Id"
           FROM "JellyfinShow"
           INNER JOIN "MediaItem" ON "MediaItem"."Id" = "JellyfinShow"."Id"
           INNER JOIN "LibraryPath" existing_path
             ON existing_path."Id" = "MediaItem"."LibraryPathId"
           INNER JOIN "LibraryPath" current_path ON current_path."Id" = ?
           WHERE "JellyfinShow"."ItemId" = ?
             AND existing_path."LibraryId" = current_path."LibraryId""#,
    )
    .bind(library_path_id)
    .bind(&item.item_id)
    .fetch_optional(&mut **tx)
    .await?
    {
        let id = row.get("Id");
        sqlx::query(r#"UPDATE "JellyfinShow" SET "Etag" = ? WHERE "Id" = ?"#)
            .bind(&item.etag)
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

    let result = sqlx::query(r#"INSERT INTO "MediaItem" ("LibraryPathId", "State") VALUES (?, 0)"#)
        .bind(library_path_id)
        .execute(&mut **tx)
        .await?;
    let id = crate::db::last_insert_id(result);
    sqlx::query(r#"INSERT INTO "Show" ("Id") VALUES (?)"#)
        .bind(id)
        .execute(&mut **tx)
        .await?;
    sqlx::query(r#"INSERT INTO "JellyfinShow" ("Id", "ItemId", "Etag") VALUES (?, ?, ?)"#)
        .bind(id)
        .bind(&item.item_id)
        .bind(&item.etag)
        .execute(&mut **tx)
        .await?;
    Ok(id)
}

async fn ensure_jellyfin_season_item(
    tx: &mut Transaction<'_, crate::db::Db>,
    library_path_id: i64,
    show_id: i64,
    item: &JellyfinSeasonItem,
) -> Result<i64> {
    if let Some(row) = sqlx::query(
        r#"SELECT "JellyfinSeason"."Id"
           FROM "JellyfinSeason"
           INNER JOIN "MediaItem" ON "MediaItem"."Id" = "JellyfinSeason"."Id"
           INNER JOIN "LibraryPath" existing_path
             ON existing_path."Id" = "MediaItem"."LibraryPathId"
           INNER JOIN "LibraryPath" current_path ON current_path."Id" = ?
           WHERE "JellyfinSeason"."ItemId" = ?
             AND existing_path."LibraryId" = current_path."LibraryId""#,
    )
    .bind(library_path_id)
    .bind(&item.item_id)
    .fetch_optional(&mut **tx)
    .await?
    {
        let id = row.get("Id");
        sqlx::query(r#"UPDATE "JellyfinSeason" SET "Etag" = ? WHERE "Id" = ?"#)
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
        sqlx::query(r#"UPDATE "MediaItem" SET "LibraryPathId" = ?, "State" = 0 WHERE "Id" = ?"#)
            .bind(library_path_id)
            .bind(id)
            .execute(&mut **tx)
            .await?;
        return Ok(id);
    }

    let result = sqlx::query(r#"INSERT INTO "MediaItem" ("LibraryPathId", "State") VALUES (?, 0)"#)
        .bind(library_path_id)
        .execute(&mut **tx)
        .await?;
    let id = crate::db::last_insert_id(result);
    sqlx::query(r#"INSERT INTO "Season" ("Id", "ShowId", "SeasonNumber") VALUES (?, ?, ?)"#)
        .bind(id)
        .bind(show_id)
        .bind(item.season_number)
        .execute(&mut **tx)
        .await?;
    sqlx::query(r#"INSERT INTO "JellyfinSeason" ("Id", "ItemId", "Etag") VALUES (?, ?, ?)"#)
        .bind(id)
        .bind(&item.item_id)
        .bind(&item.etag)
        .execute(&mut **tx)
        .await?;
    Ok(id)
}

async fn ensure_jellyfin_episode_item(
    tx: &mut Transaction<'_, crate::db::Db>,
    library_path_id: i64,
    season_id: i64,
    item: &JellyfinMediaItem,
) -> Result<i64> {
    if let Some(row) = sqlx::query(
        r#"SELECT "JellyfinEpisode"."Id"
           FROM "JellyfinEpisode"
           INNER JOIN "MediaItem" ON "MediaItem"."Id" = "JellyfinEpisode"."Id"
           INNER JOIN "LibraryPath" existing_path
             ON existing_path."Id" = "MediaItem"."LibraryPathId"
           INNER JOIN "LibraryPath" current_path ON current_path."Id" = ?
           WHERE "JellyfinEpisode"."ItemId" = ?
             AND existing_path."LibraryId" = current_path."LibraryId""#,
    )
    .bind(library_path_id)
    .bind(&item.item_id)
    .fetch_optional(&mut **tx)
    .await?
    {
        let id = row.get("Id");
        sqlx::query(r#"UPDATE "JellyfinEpisode" SET "Etag" = ? WHERE "Id" = ?"#)
            .bind(&item.etag)
            .bind(id)
            .execute(&mut **tx)
            .await?;
        sqlx::query(r#"UPDATE "Episode" SET "SeasonId" = ? WHERE "Id" = ?"#)
            .bind(season_id)
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

    let result = sqlx::query(r#"INSERT INTO "MediaItem" ("LibraryPathId", "State") VALUES (?, 0)"#)
        .bind(library_path_id)
        .execute(&mut **tx)
        .await?;
    let id = crate::db::last_insert_id(result);
    sqlx::query(r#"INSERT INTO "Episode" ("Id", "SeasonId") VALUES (?, ?)"#)
        .bind(id)
        .bind(season_id)
        .execute(&mut **tx)
        .await?;
    sqlx::query(r#"INSERT INTO "JellyfinEpisode" ("Id", "ItemId", "Etag") VALUES (?, ?, ?)"#)
        .bind(id)
        .bind(&item.item_id)
        .bind(&item.etag)
        .execute(&mut **tx)
        .await?;
    Ok(id)
}

#[derive(Debug, Clone, Copy)]
enum VersionOwner {
    Movie(i64),
    Episode(i64),
}

async fn ensure_media_version(
    tx: &mut Transaction<'_, crate::db::Db>,
    owner: VersionOwner,
    item: &JellyfinMediaItem,
    playback: &JellyfinPlaybackMetadata,
) -> Result<i64> {
    let (column, item_id) = match owner {
        VersionOwner::Movie(item_id) => ("MovieId", item_id),
        VersionOwner::Episode(item_id) => ("EpisodeId", item_id),
    };
    let sql = format!(r#"SELECT "Id" FROM "MediaVersion" WHERE "{column}" = ?"#);
    if let Some(row) = sqlx::query(&sql)
        .bind(item_id)
        .fetch_optional(&mut **tx)
        .await?
    {
        let id = row.get("Id");
        update_media_version(tx, id, item, playback).await?;
        return Ok(id);
    }

    let sql = format!(
        r#"INSERT INTO "MediaVersion" (
            "DateAdded", "DateUpdated", "DisplayAspectRatio", "Duration", "Height",
            "Name", "{column}", "RFrameRate", "SampleAspectRatio",
            "VideoScanKind", "Width"
        ) VALUES (?, ?, ?, ?, ?, 'Main', ?, ?, ?, ?, ?)"#
    );
    let result = sqlx::query(&sql)
        .bind(encode_datetime(item.date_created))
        .bind(encode_datetime(Utc::now()))
        .bind(&playback.display_aspect_ratio)
        .bind(encode_duration(playback.duration))
        .bind(playback.height)
        .bind(item_id)
        .bind(&playback.r_frame_rate)
        .bind(&playback.sample_aspect_ratio)
        .bind(encode_video_scan_kind(VideoScanKind::Progressive))
        .bind(playback.width)
        .execute(&mut **tx)
        .await?;
    Ok(crate::db::last_insert_id(result))
}

async fn update_media_version(
    tx: &mut Transaction<'_, crate::db::Db>,
    id: i64,
    item: &JellyfinMediaItem,
    playback: &JellyfinPlaybackMetadata,
) -> Result<()> {
    sqlx::query(
        r#"UPDATE "MediaVersion"
           SET "DateAdded" = ?, "DateUpdated" = ?, "DisplayAspectRatio" = ?,
               "Duration" = ?, "Height" = ?, "RFrameRate" = ?,
               "SampleAspectRatio" = ?, "Width" = ?
           WHERE "Id" = ?"#,
    )
    .bind(encode_datetime(item.date_created))
    .bind(encode_datetime(Utc::now()))
    .bind(&playback.display_aspect_ratio)
    .bind(encode_duration(playback.duration))
    .bind(playback.height)
    .bind(&playback.r_frame_rate)
    .bind(&playback.sample_aspect_ratio)
    .bind(playback.width)
    .bind(id)
    .execute(&mut **tx)
    .await?;
    Ok(())
}

async fn ensure_media_file(
    tx: &mut Transaction<'_, crate::db::Db>,
    version_id: i64,
    path: &str,
) -> Result<()> {
    sqlx::query(
        r#"REPLACE INTO "MediaFile" ("MediaVersionId", "Path", "PathHash")
           VALUES (?, ?, ?)"#,
    )
    .bind(version_id)
    .bind(path)
    .bind(path_hash(path))
    .execute(&mut **tx)
    .await?;
    Ok(())
}

async fn replace_streams(
    tx: &mut Transaction<'_, crate::db::Db>,
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

async fn replace_chapters(
    tx: &mut Transaction<'_, crate::db::Db>,
    version_id: i64,
    chapters: &[MediaChapter],
) -> Result<()> {
    sqlx::query(r#"DELETE FROM "MediaChapter" WHERE "MediaVersionId" = ?"#)
        .bind(version_id)
        .execute(&mut **tx)
        .await?;
    for chapter in chapters {
        sqlx::query(
            r#"INSERT INTO "MediaChapter" (
                "MediaVersionId", "ChapterId", "StartTime", "EndTime", "Title"
            ) VALUES (?, ?, ?, ?, ?)"#,
        )
        .bind(version_id)
        .bind(chapter.chapter_id)
        .bind(encode_duration(chapter.start_time))
        .bind(encode_duration(chapter.end_time))
        .bind(&chapter.title)
        .execute(&mut **tx)
        .await?;
    }
    Ok(())
}

#[allow(clippy::needless_pass_by_value)]
fn provider_error(error: reqwest::Error) -> RepositoryError {
    RepositoryError::Decode {
        kind: "jellyfin http",
        value: error.to_string(),
    }
}

fn path_hash(path: &str) -> String {
    let digest = Sha256::digest(path.as_bytes());
    let mut hash = String::with_capacity(digest.len() * 2);

    for byte in digest {
        use std::fmt::Write as _;

        write!(&mut hash, "{byte:02x}").expect("writing to String should not fail");
    }

    hash
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
#[serde(rename_all = "PascalCase")]
struct JellyfinLibraryResponse {
    name: Option<String>,
    collection_type: Option<String>,
    item_id: Option<String>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "PascalCase")]
struct JellyfinLibraryItemsResponse {
    #[serde(default)]
    items: Vec<JellyfinLibraryItemResponse>,
    #[serde(rename = "TotalRecordCount")]
    total_record_count: i32,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "PascalCase")]
struct JellyfinLibraryItemResponse {
    name: Option<String>,
    id: Option<String>,
    etag: Option<String>,
    path: Option<String>,
    #[serde(default)]
    date_created: Option<DateTime<Utc>>,
    #[serde(default)]
    run_time_ticks: i64,
    location_type: Option<String>,
    #[serde(rename = "Type")]
    item_type: Option<String>,
    index_number: Option<i32>,
    #[serde(default)]
    chapters: Vec<JellyfinChapterResponse>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "PascalCase")]
struct JellyfinPlaybackInfoResponse {
    #[serde(default)]
    media_sources: Vec<JellyfinMediaSourceResponse>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "PascalCase")]
struct JellyfinMediaSourceResponse {
    #[serde(default)]
    run_time_ticks: i64,
    #[serde(default)]
    media_streams: Vec<JellyfinMediaStreamResponse>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "PascalCase")]
struct JellyfinMediaStreamResponse {
    #[serde(rename = "Type")]
    stream_type: Option<String>,
    codec: Option<String>,
    language: Option<String>,
    height: Option<i32>,
    width: Option<i32>,
    #[serde(default)]
    index: i32,
    #[serde(default)]
    is_default: bool,
    #[serde(default)]
    is_forced: bool,
    #[serde(default)]
    is_external: bool,
    aspect_ratio: Option<String>,
    channels: Option<i32>,
    real_frame_rate: Option<f64>,
    title: Option<String>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "PascalCase")]
struct JellyfinChapterResponse {
    #[serde(default)]
    start_position_ticks: i64,
    name: Option<String>,
}
