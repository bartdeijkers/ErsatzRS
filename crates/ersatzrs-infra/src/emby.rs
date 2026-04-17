use async_trait::async_trait;
use chrono::{DateTime, Duration, Utc};
use ersatzrs_core::repositories::{
    EmbyCollectionItem, EmbyCollectionMediaItem, EmbyCollectionMediaKind, EmbyCollectionMediaPage,
    EmbyCollectionPage, EmbyConnectionConfig, EmbyItemPage, EmbyLibrarySummary, EmbyMediaItem,
    EmbyPlaybackMetadata, EmbySeasonItem, EmbySeasonPage, EmbyService as EmbyServiceTrait,
    EmbyShowItem, EmbyShowPage, RepositoryError, Result,
};
use ersatzrs_domain::{
    LibraryMediaKind, MediaChapter, MediaStream, MediaStreamKind, VideoScanKind,
};
use reqwest::Client;
use serde::Deserialize;
use sqlx::{Row, Sqlite, SqlitePool, Transaction};

#[derive(Debug, Clone)]
pub struct EmbyService {
    client: Client,
}

impl Default for EmbyService {
    fn default() -> Self {
        Self::new()
    }
}

impl EmbyService {
    #[must_use]
    pub fn new() -> Self {
        Self {
            client: Client::new(),
        }
    }
}

#[async_trait]
impl EmbyServiceTrait for EmbyService {
    async fn list_libraries(
        &self,
        config: &EmbyConnectionConfig,
    ) -> Result<Vec<EmbyLibrarySummary>> {
        let url = format!(
            "{}/Library/VirtualFolders",
            config.address.trim_end_matches('/')
        );
        let response: Vec<EmbyLibraryResponse> = self
            .client
            .get(url)
            .header("X-Emby-Token", &config.api_key)
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
        config: &EmbyConnectionConfig,
        parent_id: &str,
        start_index: i32,
        limit: i32,
    ) -> Result<EmbyItemPage> {
        let url = format!("{}/Items", config.address.trim_end_matches('/'));
        let start_index = start_index.to_string();
        let limit = limit.to_string();
        let response: EmbyLibraryItemsResponse = self
            .client
            .get(url)
            .header("X-Emby-Token", &config.api_key)
            .query(&[
                ("sortOrder", "Ascending"),
                ("sortBy", "SortName"),
                ("parentId", parent_id),
                (
                    "fields",
                    "Path,Genres,Tags,DateCreated,Etag,Overview,Taglines,Studios,People,ProductionYear,PremiereDate,MediaSources,OfficialRating,ProviderIds,Chapters",
                ),
                ("includeItemTypes", "Movie"),
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
            .map_err(provider_error)?;

        Ok(EmbyItemPage {
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
        config: &EmbyConnectionConfig,
        parent_id: &str,
        start_index: i32,
        limit: i32,
    ) -> Result<EmbyShowPage> {
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
        Ok(EmbyShowPage {
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
        config: &EmbyConnectionConfig,
        parent_id: &str,
        start_index: i32,
        limit: i32,
    ) -> Result<EmbySeasonPage> {
        let url = format!(
            "{}/Shows/{parent_id}/Seasons",
            config.address.trim_end_matches('/')
        );
        let start_index = start_index.to_string();
        let limit = limit.to_string();
        let response: EmbyLibraryItemsResponse = self
            .client
            .get(url)
            .header("X-Emby-Token", &config.api_key)
            .query(&[
                ("sortOrder", "Ascending"),
                ("sortBy", "SortName"),
                ("fields", "Path,DateCreated,Etag,Taglines,ProviderIds"),
                ("startIndex", start_index.as_str()),
                ("limit", limit.as_str()),
            ])
            .send()
            .await
            .map_err(provider_error)?
            .error_for_status()
            .map_err(provider_error)?
            .json()
            .await
            .map_err(provider_error)?;
        Ok(EmbySeasonPage {
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
        config: &EmbyConnectionConfig,
        show_id: &str,
        season_id: &str,
        start_index: i32,
        limit: i32,
    ) -> Result<EmbyItemPage> {
        let url = format!(
            "{}/Shows/{show_id}/Episodes",
            config.address.trim_end_matches('/')
        );
        let start_index = start_index.to_string();
        let limit = limit.to_string();
        let response: EmbyLibraryItemsResponse = self
            .client
            .get(url)
            .header("X-Emby-Token", &config.api_key)
            .query(&[
                ("seasonId", season_id),
                (
                    "fields",
                    "Path,Genres,Tags,DateCreated,Etag,Overview,ProductionYear,PremiereDate,MediaSources,LocationType,ProviderIds,People,Chapters",
                ),
                ("sortOrder", "Ascending"),
                ("sortBy", "SortName"),
                ("startIndex", start_index.as_str()),
                ("limit", limit.as_str()),
            ])
            .send()
            .await
            .map_err(provider_error)?
            .error_for_status()
            .map_err(provider_error)?
            .json()
            .await
            .map_err(provider_error)?;
        Ok(EmbyItemPage {
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
        config: &EmbyConnectionConfig,
        parent_id: &str,
        start_index: i32,
        limit: i32,
    ) -> Result<EmbyCollectionPage> {
        let response = self
            .fetch_items(config, parent_id, "Etag", "BoxSet", start_index, limit)
            .await?;
        Ok(EmbyCollectionPage {
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
        config: &EmbyConnectionConfig,
        parent_id: &str,
        start_index: i32,
        limit: i32,
    ) -> Result<EmbyCollectionMediaPage> {
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
        Ok(EmbyCollectionMediaPage {
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
        config: &EmbyConnectionConfig,
        item_id: &str,
    ) -> Result<EmbyPlaybackMetadata> {
        let url = format!(
            "{}/Items/{}/PlaybackInfo",
            config.address.trim_end_matches('/'),
            item_id
        );
        let response: EmbyPlaybackInfoResponse = self
            .client
            .get(url)
            .header("X-Emby-Token", &config.api_key)
            .send()
            .await
            .map_err(provider_error)?
            .error_for_status()
            .map_err(provider_error)?
            .json()
            .await
            .map_err(provider_error)?;

        project_playback(response).ok_or_else(|| RepositoryError::Decode {
            kind: "emby playback info",
            value: "missing media source".to_owned(),
        })
    }
}

impl EmbyService {
    async fn fetch_items(
        &self,
        config: &EmbyConnectionConfig,
        parent_id: &str,
        fields: &str,
        include_item_types: &str,
        start_index: i32,
        limit: i32,
    ) -> Result<EmbyLibraryItemsResponse> {
        let url = format!("{}/Items", config.address.trim_end_matches('/'));
        let start_index = start_index.to_string();
        let limit = limit.to_string();
        self.client
            .get(url)
            .header("X-Emby-Token", &config.api_key)
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

/// Fetch one Emby movie page and write mapped movie rows.
///
/// # Errors
///
/// Returns an error when the HTTP client fails or any database write fails.
pub async fn sync_emby_movie_library<S>(
    pool: &SqlitePool,
    service: &S,
    config: &EmbyConnectionConfig,
    media_source_id: i64,
    library_id: i64,
    library_item_id: &str,
) -> Result<usize>
where
    S: EmbyServiceTrait,
{
    let page = service
        .fetch_movie_page(config, library_item_id, 0, 100)
        .await?;
    for item in &page.items {
        let playback = service.stream_metadata(config, &item.item_id).await?;
        upsert_emby_movie(
            pool,
            media_source_id,
            library_id,
            library_item_id,
            item,
            &playback,
        )
        .await?;
    }
    Ok(page.items.len())
}

/// Fetch Emby shows, seasons, and episodes and write mapped media rows.
///
/// # Errors
///
/// Returns an error when the HTTP client fails or any database write fails.
pub async fn sync_emby_show_library<S>(
    pool: &SqlitePool,
    service: &S,
    config: &EmbyConnectionConfig,
    media_source_id: i64,
    library_id: i64,
    library_item_id: &str,
) -> Result<usize>
where
    S: EmbyServiceTrait,
{
    let shows = service
        .fetch_show_page(config, library_item_id, 0, 100)
        .await?;
    let mut synced = 0;
    for show in &shows.items {
        let show_id =
            upsert_emby_show(pool, media_source_id, library_id, library_item_id, show).await?;
        let seasons = service
            .fetch_season_page(config, &show.item_id, 0, 100)
            .await?;
        for season in &seasons.items {
            let season_id = upsert_emby_season(
                pool,
                media_source_id,
                library_id,
                library_item_id,
                show_id,
                season,
            )
            .await?;
            let episodes = service
                .fetch_episode_page(config, &show.item_id, &season.item_id, 0, 100)
                .await?;
            for episode in &episodes.items {
                let playback = service.stream_metadata(config, &episode.item_id).await?;
                upsert_emby_episode(
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
        }
    }
    Ok(synced)
}

/// Fetch Emby collections and write collection membership rows.
///
/// # Errors
///
/// Returns an error when the HTTP client fails or any database write fails.
pub async fn sync_emby_collection_library<S>(
    pool: &SqlitePool,
    service: &S,
    config: &EmbyConnectionConfig,
    media_source_id: i64,
    parent_id: &str,
) -> Result<usize>
where
    S: EmbyServiceTrait,
{
    let page = service
        .fetch_collection_page(config, parent_id, 0, 100)
        .await?;
    for collection in &page.items {
        let collection_id = upsert_emby_collection(pool, media_source_id, collection).await?;
        let items = service
            .fetch_collection_items_page(config, &collection.item_id, 0, 100)
            .await?;
        replace_collection_items(pool, collection_id, &items.items).await?;
    }
    Ok(page.items.len())
}

fn project_library(response: EmbyLibraryResponse) -> Option<EmbyLibrarySummary> {
    let media_kind = match response.collection_type?.to_ascii_lowercase().as_str() {
        "movies" => LibraryMediaKind::Movies,
        "tvshows" => LibraryMediaKind::Shows,
        _ => return None,
    };
    Some(EmbyLibrarySummary {
        item_id: response.item_id?,
        name: response.name.unwrap_or_default(),
        media_kind,
    })
}

fn project_show(item: EmbyLibraryItemResponse) -> Option<EmbyShowItem> {
    Some(EmbyShowItem {
        item_id: item.id?,
        etag: item.etag,
        name: item.name.unwrap_or_default(),
    })
}

fn project_season(item: EmbyLibraryItemResponse) -> Option<EmbySeasonItem> {
    Some(EmbySeasonItem {
        item_id: item.id?,
        etag: item.etag,
        name: item.name.unwrap_or_default(),
        season_number: item.index_number.unwrap_or_default(),
    })
}

fn project_collection(item: EmbyLibraryItemResponse) -> Option<EmbyCollectionItem> {
    Some(EmbyCollectionItem {
        item_id: item.id?,
        etag: item.etag,
        name: item.name.unwrap_or_default(),
    })
}

fn project_collection_media_item(item: EmbyLibraryItemResponse) -> Option<EmbyCollectionMediaItem> {
    if item.location_type.as_deref() != Some("FileSystem") {
        return None;
    }
    let kind = match item.item_type.as_deref()? {
        "Movie" => EmbyCollectionMediaKind::Movie,
        "Series" => EmbyCollectionMediaKind::Show,
        "Season" => EmbyCollectionMediaKind::Season,
        "Episode" => EmbyCollectionMediaKind::Episode,
        _ => return None,
    };
    Some(EmbyCollectionMediaItem {
        item_id: item.id?,
        kind,
    })
}

fn project_movie(item: EmbyLibraryItemResponse) -> Option<EmbyMediaItem> {
    if item.location_type.as_deref() != Some("FileSystem") {
        return None;
    }
    let path = item.path?;
    if path.to_ascii_lowercase().ends_with(".strm") {
        return None;
    }
    let duration = Duration::nanoseconds(item.run_time_ticks * 100);
    Some(EmbyMediaItem {
        item_id: item.id?,
        etag: item.etag,
        name: item.name.unwrap_or_default(),
        path,
        date_created: item.date_created.unwrap_or_else(Utc::now),
        duration,
        chapters: project_chapters(item.chapters, duration),
    })
}

fn project_chapters(chapters: Vec<EmbyChapterResponse>, duration: Duration) -> Vec<MediaChapter> {
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

fn project_playback(response: EmbyPlaybackInfoResponse) -> Option<EmbyPlaybackMetadata> {
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

    Some(EmbyPlaybackMetadata {
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

fn project_stream(stream: EmbyMediaStreamResponse) -> Option<MediaStream> {
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

async fn upsert_emby_movie(
    pool: &SqlitePool,
    media_source_id: i64,
    library_id: i64,
    library_item_id: &str,
    item: &EmbyMediaItem,
    playback: &EmbyPlaybackMetadata,
) -> Result<()> {
    let mut tx = pool.begin().await?;
    ensure_emby_library(&mut tx, media_source_id, library_id, library_item_id).await?;
    let library_path_id =
        ensure_library_path(&mut tx, library_id, &format!("emby://{library_id}")).await?;
    let item_id = ensure_emby_movie_item(&mut tx, library_path_id, item).await?;
    let version_id =
        ensure_media_version(&mut tx, VersionOwner::Movie(item_id), item, playback).await?;
    ensure_media_file(&mut tx, version_id, &item.path).await?;
    replace_streams(&mut tx, version_id, &playback.streams).await?;
    replace_chapters(&mut tx, version_id, &item.chapters).await?;
    tx.commit().await?;
    Ok(())
}

async fn upsert_emby_show(
    pool: &SqlitePool,
    media_source_id: i64,
    library_id: i64,
    library_item_id: &str,
    item: &EmbyShowItem,
) -> Result<i64> {
    let mut tx = pool.begin().await?;
    ensure_emby_library(&mut tx, media_source_id, library_id, library_item_id).await?;
    let library_path_id =
        ensure_library_path(&mut tx, library_id, &format!("emby://{library_id}")).await?;
    let id = ensure_emby_show_item(&mut tx, library_path_id, item).await?;
    tx.commit().await?;
    Ok(id)
}

async fn upsert_emby_season(
    pool: &SqlitePool,
    media_source_id: i64,
    library_id: i64,
    library_item_id: &str,
    show_id: i64,
    item: &EmbySeasonItem,
) -> Result<i64> {
    let mut tx = pool.begin().await?;
    ensure_emby_library(&mut tx, media_source_id, library_id, library_item_id).await?;
    let library_path_id =
        ensure_library_path(&mut tx, library_id, &format!("emby://{library_id}")).await?;
    let id = ensure_emby_season_item(&mut tx, library_path_id, show_id, item).await?;
    tx.commit().await?;
    Ok(id)
}

async fn upsert_emby_episode(
    pool: &SqlitePool,
    media_source_id: i64,
    library_id: i64,
    library_item_id: &str,
    season_id: i64,
    item: &EmbyMediaItem,
    playback: &EmbyPlaybackMetadata,
) -> Result<()> {
    let mut tx = pool.begin().await?;
    ensure_emby_library(&mut tx, media_source_id, library_id, library_item_id).await?;
    let library_path_id =
        ensure_library_path(&mut tx, library_id, &format!("emby://{library_id}")).await?;
    let item_id = ensure_emby_episode_item(&mut tx, library_path_id, season_id, item).await?;
    let version_id =
        ensure_media_version(&mut tx, VersionOwner::Episode(item_id), item, playback).await?;
    ensure_media_file(&mut tx, version_id, &item.path).await?;
    replace_streams(&mut tx, version_id, &playback.streams).await?;
    replace_chapters(&mut tx, version_id, &item.chapters).await?;
    tx.commit().await?;
    Ok(())
}

async fn upsert_emby_collection(
    pool: &SqlitePool,
    media_source_id: i64,
    item: &EmbyCollectionItem,
) -> Result<i64> {
    let mut tx = pool.begin().await?;
    sqlx::query(r#"INSERT OR IGNORE INTO "MediaSource" ("Id") VALUES (?)"#)
        .bind(media_source_id)
        .execute(&mut *tx)
        .await?;
    sqlx::query(r#"INSERT OR IGNORE INTO "EmbyMediaSource" ("Id") VALUES (?)"#)
        .bind(media_source_id)
        .execute(&mut *tx)
        .await?;

    let id = if let Some(row) =
        sqlx::query(r#"SELECT "Id" FROM "EmbyCollection" WHERE "ItemId" = ?"#)
            .bind(&item.item_id)
            .fetch_optional(&mut *tx)
            .await?
    {
        let id = row.get("Id");
        sqlx::query(r#"UPDATE "EmbyCollection" SET "Etag" = ?, "Name" = ? WHERE "Id" = ?"#)
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
        let id = result.last_insert_rowid();
        sqlx::query(
            r#"INSERT INTO "EmbyCollection" ("Id", "ItemId", "Etag", "Name")
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
    pool: &SqlitePool,
    collection_id: i64,
    items: &[EmbyCollectionMediaItem],
) -> Result<()> {
    let mut tx = pool.begin().await?;
    sqlx::query(r#"DELETE FROM "CollectionItem" WHERE "CollectionId" = ?"#)
        .bind(collection_id)
        .execute(&mut *tx)
        .await?;
    for (index, item) in items.iter().enumerate() {
        if let Some(media_item_id) = find_emby_media_item(&mut tx, item).await? {
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

async fn find_emby_media_item(
    tx: &mut Transaction<'_, Sqlite>,
    item: &EmbyCollectionMediaItem,
) -> Result<Option<i64>> {
    let table = match item.kind {
        EmbyCollectionMediaKind::Movie => "EmbyMovie",
        EmbyCollectionMediaKind::Show => "EmbyShow",
        EmbyCollectionMediaKind::Season => "EmbySeason",
        EmbyCollectionMediaKind::Episode => "EmbyEpisode",
    };
    let sql = format!(r#"SELECT "Id" FROM "{table}" WHERE "ItemId" = ?"#);
    Ok(sqlx::query(&sql)
        .bind(&item.item_id)
        .fetch_optional(&mut **tx)
        .await?
        .map(|row| row.get("Id")))
}

async fn ensure_emby_library(
    tx: &mut Transaction<'_, Sqlite>,
    media_source_id: i64,
    library_id: i64,
    library_item_id: &str,
) -> Result<()> {
    sqlx::query(r#"INSERT OR IGNORE INTO "MediaSource" ("Id") VALUES (?)"#)
        .bind(media_source_id)
        .execute(&mut **tx)
        .await?;
    sqlx::query(r#"INSERT OR IGNORE INTO "EmbyMediaSource" ("Id") VALUES (?)"#)
        .bind(media_source_id)
        .execute(&mut **tx)
        .await?;
    sqlx::query(
        r#"INSERT OR IGNORE INTO "Library" ("Id", "Name", "MediaKind", "MediaSourceId")
           VALUES (?, 'Emby', 0, ?)"#,
    )
    .bind(library_id)
    .bind(media_source_id)
    .execute(&mut **tx)
    .await?;
    sqlx::query(
        r#"INSERT OR IGNORE INTO "EmbyLibrary" ("Id", "ItemId", "ShouldSyncItems")
           VALUES (?, ?, 1)"#,
    )
    .bind(library_id)
    .bind(library_item_id)
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

async fn ensure_emby_movie_item(
    tx: &mut Transaction<'_, Sqlite>,
    library_path_id: i64,
    item: &EmbyMediaItem,
) -> Result<i64> {
    if let Some(row) = sqlx::query(r#"SELECT "Id" FROM "EmbyMovie" WHERE "ItemId" = ?"#)
        .bind(&item.item_id)
        .fetch_optional(&mut **tx)
        .await?
    {
        let id = row.get("Id");
        sqlx::query(r#"UPDATE "EmbyMovie" SET "Etag" = ? WHERE "Id" = ?"#)
            .bind(&item.etag)
            .bind(id)
            .execute(&mut **tx)
            .await?;
        return Ok(id);
    }

    let result = sqlx::query(r#"INSERT INTO "MediaItem" ("LibraryPathId", "State") VALUES (?, 0)"#)
        .bind(library_path_id)
        .execute(&mut **tx)
        .await?;
    let id = result.last_insert_rowid();
    sqlx::query(r#"INSERT INTO "Movie" ("Id") VALUES (?)"#)
        .bind(id)
        .execute(&mut **tx)
        .await?;
    sqlx::query(r#"INSERT INTO "EmbyMovie" ("Id", "ItemId", "Etag") VALUES (?, ?, ?)"#)
        .bind(id)
        .bind(&item.item_id)
        .bind(&item.etag)
        .execute(&mut **tx)
        .await?;
    Ok(id)
}

async fn ensure_emby_show_item(
    tx: &mut Transaction<'_, Sqlite>,
    library_path_id: i64,
    item: &EmbyShowItem,
) -> Result<i64> {
    if let Some(row) = sqlx::query(r#"SELECT "Id" FROM "EmbyShow" WHERE "ItemId" = ?"#)
        .bind(&item.item_id)
        .fetch_optional(&mut **tx)
        .await?
    {
        let id = row.get("Id");
        sqlx::query(r#"UPDATE "EmbyShow" SET "Etag" = ? WHERE "Id" = ?"#)
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
    let id = result.last_insert_rowid();
    sqlx::query(r#"INSERT INTO "Show" ("Id") VALUES (?)"#)
        .bind(id)
        .execute(&mut **tx)
        .await?;
    sqlx::query(r#"INSERT INTO "EmbyShow" ("Id", "ItemId", "Etag") VALUES (?, ?, ?)"#)
        .bind(id)
        .bind(&item.item_id)
        .bind(&item.etag)
        .execute(&mut **tx)
        .await?;
    Ok(id)
}

async fn ensure_emby_season_item(
    tx: &mut Transaction<'_, Sqlite>,
    library_path_id: i64,
    show_id: i64,
    item: &EmbySeasonItem,
) -> Result<i64> {
    if let Some(row) = sqlx::query(r#"SELECT "Id" FROM "EmbySeason" WHERE "ItemId" = ?"#)
        .bind(&item.item_id)
        .fetch_optional(&mut **tx)
        .await?
    {
        let id = row.get("Id");
        sqlx::query(r#"UPDATE "EmbySeason" SET "Etag" = ? WHERE "Id" = ?"#)
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
    let id = result.last_insert_rowid();
    sqlx::query(r#"INSERT INTO "Season" ("Id", "ShowId", "SeasonNumber") VALUES (?, ?, ?)"#)
        .bind(id)
        .bind(show_id)
        .bind(item.season_number)
        .execute(&mut **tx)
        .await?;
    sqlx::query(r#"INSERT INTO "EmbySeason" ("Id", "ItemId", "Etag") VALUES (?, ?, ?)"#)
        .bind(id)
        .bind(&item.item_id)
        .bind(&item.etag)
        .execute(&mut **tx)
        .await?;
    Ok(id)
}

async fn ensure_emby_episode_item(
    tx: &mut Transaction<'_, Sqlite>,
    library_path_id: i64,
    season_id: i64,
    item: &EmbyMediaItem,
) -> Result<i64> {
    if let Some(row) = sqlx::query(r#"SELECT "Id" FROM "EmbyEpisode" WHERE "ItemId" = ?"#)
        .bind(&item.item_id)
        .fetch_optional(&mut **tx)
        .await?
    {
        let id = row.get("Id");
        sqlx::query(r#"UPDATE "EmbyEpisode" SET "Etag" = ? WHERE "Id" = ?"#)
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
    let id = result.last_insert_rowid();
    sqlx::query(r#"INSERT INTO "Episode" ("Id", "SeasonId") VALUES (?, ?)"#)
        .bind(id)
        .bind(season_id)
        .execute(&mut **tx)
        .await?;
    sqlx::query(r#"INSERT INTO "EmbyEpisode" ("Id", "ItemId", "Etag") VALUES (?, ?, ?)"#)
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
    tx: &mut Transaction<'_, Sqlite>,
    owner: VersionOwner,
    item: &EmbyMediaItem,
    playback: &EmbyPlaybackMetadata,
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
    Ok(result.last_insert_rowid())
}

async fn update_media_version(
    tx: &mut Transaction<'_, Sqlite>,
    id: i64,
    item: &EmbyMediaItem,
    playback: &EmbyPlaybackMetadata,
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
    tx: &mut Transaction<'_, Sqlite>,
    version_id: i64,
    path: &str,
) -> Result<()> {
    sqlx::query(
        r#"INSERT OR REPLACE INTO "MediaFile" ("MediaVersionId", "Path", "PathHash")
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

async fn replace_chapters(
    tx: &mut Transaction<'_, Sqlite>,
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
        kind: "emby http",
        value: error.to_string(),
    }
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
#[serde(rename_all = "PascalCase")]
struct EmbyLibraryResponse {
    name: Option<String>,
    collection_type: Option<String>,
    item_id: Option<String>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "PascalCase")]
struct EmbyLibraryItemsResponse {
    #[serde(default)]
    items: Vec<EmbyLibraryItemResponse>,
    #[serde(rename = "TotalRecordCount")]
    total_record_count: i32,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "PascalCase")]
struct EmbyLibraryItemResponse {
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
    chapters: Vec<EmbyChapterResponse>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "PascalCase")]
struct EmbyPlaybackInfoResponse {
    #[serde(default)]
    media_sources: Vec<EmbyMediaSourceResponse>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "PascalCase")]
struct EmbyMediaSourceResponse {
    #[serde(default)]
    run_time_ticks: i64,
    #[serde(default)]
    media_streams: Vec<EmbyMediaStreamResponse>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "PascalCase")]
struct EmbyMediaStreamResponse {
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
struct EmbyChapterResponse {
    #[serde(default)]
    start_position_ticks: i64,
    name: Option<String>,
}
