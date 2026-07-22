use crate::db::DbPool;
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
                "Path,Genres,Tags,DateCreated,Etag,Overview,Taglines,Studios,People,OfficialRating,ProviderIds,PremiereDate,ImageTags",
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
                "Path,DateCreated,Etag,ProviderIds,PremiereDate,ImageTags",
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
                "Path,Genres,Tags,DateCreated,Etag,Overview,ProviderIds,People,PremiereDate,ImageTags,Chapters",
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
                .filter_map(project_episode)
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
    /// Fetch a single Jellyfin show by item id, scoped to its parent library.
    ///
    /// Mirrors ErsatzTV's `JellyfinApiClient.GetSingleShow`, which calls
    /// `/Items?parentId=<library>&ids=<show>&recursive=false&limit=1` and
    /// projects the first hit. Returns `None` when Jellyfin returns an empty
    /// list (deleted show, wrong library, etc.).
    ///
    /// # Errors
    ///
    /// Returns an error when the HTTP request or response decoding fails.
    pub async fn fetch_single_show(
        &self,
        config: &JellyfinConnectionConfig,
        library_item_id: &str,
        show_item_id: &str,
    ) -> Result<Option<JellyfinShowItem>> {
        let url = format!("{}/Items", config.address.trim_end_matches('/'));
        let response: JellyfinLibraryItemsResponse = self
            .client
            .get(url)
            .header("Authorization", &config.authorization_header)
            .query(&[
                ("sortOrder", "Ascending"),
                ("sortBy", "SortName"),
                ("parentId", library_item_id),
                (
                    "fields",
                    "Path,Genres,Tags,DateCreated,Etag,Overview,Taglines,Studios,People,OfficialRating,ProviderIds,PremiereDate,ImageTags",
                ),
                ("includeItemTypes", "Series"),
                ("recursive", "false"),
                ("startIndex", "0"),
                ("limit", "1"),
                ("ids", show_item_id),
            ])
            .send()
            .await
            .map_err(provider_error)?
            .error_for_status()
            .map_err(provider_error)?
            .json()
            .await
            .map_err(provider_error)?;

        Ok(response.items.into_iter().find_map(project_show))
    }

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
    sync_jellyfin_movie_library_with_progress(
        pool,
        service,
        config,
        media_source_id,
        library_id,
        library_item_id,
        deep_scan,
        None,
    )
    .await
}

/// Variant of [`sync_jellyfin_movie_library`] that emits
/// `(synced / total_record_count)` after each item upsert.
///
/// # Errors
///
/// Returns an error when the HTTP client fails or any database write fails.
#[allow(
    clippy::too_many_arguments,
    clippy::too_many_lines,
    clippy::cast_precision_loss,
    clippy::cast_possible_truncation
)]
pub async fn sync_jellyfin_movie_library_with_progress<S>(
    pool: &DbPool,
    service: &S,
    config: &JellyfinConnectionConfig,
    media_source_id: i64,
    library_id: i64,
    library_item_id: &str,
    deep_scan: bool,
    progress: Option<&dyn ersatzrs_domain::ScanProgressSink>,
) -> Result<usize>
where
    S: JellyfinServiceTrait,
{
    let mut synced = 0;
    let mut start_index = 0;
    let mut reported_initial = false;
    let mut present_item_ids = std::collections::HashSet::new();
    let mut variant_candidates = Vec::new();

    loop {
        let page = service
            .fetch_movie_page(config, library_item_id, start_index, JELLYFIN_PAGE_SIZE)
            .await?;
        let total_record_count = page.total_record_count;
        let page_len = i32::try_from(page.items.len()).unwrap_or(i32::MAX);
        if !reported_initial {
            if let Some(sink) = progress {
                sink.report(0.0);
            }
            reported_initial = true;
        }
        for item in &page.items {
            present_item_ids.insert(item.item_id.clone());
            let item_id = if !deep_scan
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
                crate::media_server::provider_media_item_id(
                    pool,
                    "JellyfinMovie",
                    "ItemId",
                    library_id,
                    &item.item_id,
                )
                .await
                .repository()?
            } else {
                let playback = service.stream_metadata(config, &item.item_id).await?;
                let item_id = upsert_jellyfin_movie(
                    pool,
                    media_source_id,
                    library_id,
                    library_item_id,
                    item,
                    &playback,
                )
                .await?;
                synced += 1;
                Some(item_id)
            };
            if let Some(item_id) = item_id {
                variant_candidates.push(crate::media_server::ProviderVariantCandidate {
                    item_id,
                    title: item.name.clone(),
                    path: Some(item.path.clone()),
                    disambiguator: String::new(),
                    tags: item.tags.clone(),
                });
            }
            if let Some(sink) = progress {
                if total_record_count > 0 {
                    sink.report(synced as f32 / total_record_count as f32);
                }
            }
        }

        start_index += JELLYFIN_PAGE_SIZE;
        if start_index >= total_record_count || page_len == 0 {
            break;
        }
    }

    // Trash movies no longer present on the server, then flag those still on
    // the server whose local file is missing (RemoteOnly — Jellyfin supports
    // remote streaming).
    crate::media_server::flag_missing_media_server_items(
        pool,
        "JellyfinMovie",
        "ItemId",
        library_id,
        &present_item_ids,
    )
    .await
    .repository()?;
    let replacements = crate::media_server::load_media_server_path_replacements(
        pool,
        "JellyfinPathReplacement",
        "JellyfinPath",
        "JellyfinMediaSourceId",
        media_source_id,
    )
    .await
    .repository()?;
    crate::media_server::flag_unavailable_media_server_items(
        pool,
        "JellyfinMovie",
        "ItemId",
        library_id,
        &present_item_ids,
        &replacements,
        true,
    )
    .await
    .repository()?;
    crate::media_server::apply_provider_content_variants(pool, &variant_candidates)
        .await
        .repository()?;

    if let Some(sink) = progress {
        sink.report(1.0);
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
    sync_jellyfin_show_library_with_progress(
        pool,
        service,
        config,
        media_source_id,
        library_id,
        library_item_id,
        deep_scan,
        None,
    )
    .await
}

/// Variant of [`sync_jellyfin_show_library`] that reports show-level
/// progress (`shows_completed / show_total_count`). Episode-granular progress
/// would require a 3-deep upfront-count pass.
///
/// # Errors
///
/// Returns an error when the HTTP client fails or any database write fails.
#[allow(
    clippy::too_many_arguments,
    clippy::too_many_lines,
    clippy::cast_precision_loss,
    clippy::cast_possible_truncation
)]
pub async fn sync_jellyfin_show_library_with_progress<S>(
    pool: &DbPool,
    service: &S,
    config: &JellyfinConnectionConfig,
    media_source_id: i64,
    library_id: i64,
    library_item_id: &str,
    deep_scan: bool,
    progress: Option<&dyn ersatzrs_domain::ScanProgressSink>,
) -> Result<usize>
where
    S: JellyfinServiceTrait,
{
    let mut synced = 0;
    let mut shows_completed: usize = 0;
    let mut show_start_index = 0;
    let mut reported_initial = false;
    let mut present_shows = std::collections::HashSet::new();
    let mut present_seasons = std::collections::HashSet::new();
    let mut present_episodes = std::collections::HashSet::new();

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
        if !reported_initial {
            if let Some(sink) = progress {
                sink.report(0.0);
            }
            reported_initial = true;
        }
        for show in &shows.items {
            present_shows.insert(show.item_id.clone());
            synced += sync_one_jellyfin_show(
                pool,
                service,
                config,
                media_source_id,
                library_id,
                library_item_id,
                show,
                deep_scan,
                &mut present_seasons,
                &mut present_episodes,
            )
            .await?;
            shows_completed += 1;
            if let Some(sink) = progress {
                if show_total_count > 0 {
                    sink.report(shows_completed as f32 / show_total_count as f32);
                }
            }
        }

        show_start_index += JELLYFIN_PAGE_SIZE;
        if show_start_index >= show_total_count || show_page_len == 0 {
            break;
        }
    }

    // Trash shows/seasons/episodes no longer present on the server.
    for (table, present) in [
        ("JellyfinShow", &present_shows),
        ("JellyfinSeason", &present_seasons),
        ("JellyfinEpisode", &present_episodes),
    ] {
        crate::media_server::flag_missing_media_server_items(
            pool, table, "ItemId", library_id, present,
        )
        .await
        .repository()?;
    }
    // Flag episodes still on the server whose local file is missing
    // (RemoteOnly — Jellyfin supports remote streaming).
    let replacements = crate::media_server::load_media_server_path_replacements(
        pool,
        "JellyfinPathReplacement",
        "JellyfinPath",
        "JellyfinMediaSourceId",
        media_source_id,
    )
    .await
    .repository()?;
    crate::media_server::flag_unavailable_media_server_items(
        pool,
        "JellyfinEpisode",
        "ItemId",
        library_id,
        &present_episodes,
        &replacements,
        true,
    )
    .await
    .repository()?;

    if let Some(sink) = progress {
        sink.report(1.0);
    }
    Ok(synced)
}

/// Port of the per-show body of
/// `$ERSATZTV_SRC/ErsatzTV.Scanner/Core/Jellyfin/JellyfinTelevisionLibraryScanner.cs::ScanSingleShowInternal`:
/// upsert the show row, page through its seasons and episodes, honour
/// `deep_scan`/etag skip on each episode. Called both from the library-level
/// sync and from [`sync_jellyfin_show_by_id`].
#[allow(clippy::too_many_arguments)]
async fn sync_one_jellyfin_show<S>(
    pool: &DbPool,
    service: &S,
    config: &JellyfinConnectionConfig,
    media_source_id: i64,
    library_id: i64,
    library_item_id: &str,
    show: &JellyfinShowItem,
    deep_scan: bool,
    present_seasons: &mut std::collections::HashSet<String>,
    present_episodes: &mut std::collections::HashSet<String>,
) -> Result<usize>
where
    S: JellyfinServiceTrait,
{
    let mut synced = 0;
    let show_id =
        upsert_jellyfin_show(pool, media_source_id, library_id, library_item_id, show).await?;
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
            present_seasons.insert(season.item_id.clone());
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
                let episode_page_len = i32::try_from(episodes.items.len()).unwrap_or(i32::MAX);
                for episode in &episodes.items {
                    present_episodes.insert(episode.item_id.clone());
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

                    let playback = service.stream_metadata(config, &episode.item_id).await?;
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
    Ok(synced)
}

/// Synchronize one Jellyfin show by the local `Show.Id`.
///
/// Mirrors ErsatzTV's
/// `$ERSATZTV_SRC/ErsatzTV.Scanner/Application/Jellyfin/Commands/SynchronizeJellyfinShowByIdHandler.cs`:
/// resolve the active connection/api-key, map the local show id to the
/// provider item id + title via `JellyfinShow` + `ShowMetadata` (mirrors
/// `IJellyfinTelevisionRepository.GetShowTitleItemId`), fetch the one show
/// from `/Items?ids=<itemId>` (mirrors `IJellyfinApiClient.GetSingleShow`),
/// then scan that show's seasons and episodes without library cleanup.
///
/// # Errors
///
/// Returns an error when config loading, show lookup, Jellyfin requests, or
/// database writes fail.
pub async fn sync_jellyfin_show_by_id(
    pool: &DbPool,
    library_id: i64,
    show_id: i64,
    deep_scan: bool,
    progress: Option<&dyn ersatzrs_domain::ScanProgressSink>,
) -> Result<usize> {
    let target = load_target_jellyfin_show_scan(pool, library_id, show_id).await?;
    if !target.should_sync_items {
        tracing::warn!(
            library_id,
            show_id,
            show_title = %target.show_title,
            "Jellyfin show scan skipped because library item sync is disabled"
        );
        return Ok(0);
    }

    let config = load_active_jellyfin_connection_config(pool, target.media_source_id).await?;
    let service = JellyfinService::new();

    if let Some(sink) = progress {
        sink.report(0.0);
    }

    let Some(show) = service
        .fetch_single_show(&config, &target.library_item_id, &target.show_item_id)
        .await?
    else {
        tracing::warn!(
            library_id,
            show_id,
            show_item_id = %target.show_item_id,
            show_title = %target.show_title,
            "No Jellyfin show found during targeted scan"
        );
        if let Some(sink) = progress {
            sink.report(1.0);
        }
        return Ok(0);
    };

    tracing::info!(
        library_id,
        show_id,
        show_item_id = %show.item_id,
        show_title = %target.show_title,
        deep_scan,
        "Starting targeted Jellyfin show scan"
    );

    // Targeted single-show scan does no library-level cleanup (matching
    // ErsatzTV), so the present-id collectors are discarded here.
    let synced = sync_one_jellyfin_show(
        pool,
        &service,
        &config,
        target.media_source_id,
        library_id,
        &target.library_item_id,
        &show,
        deep_scan,
        &mut std::collections::HashSet::new(),
        &mut std::collections::HashSet::new(),
    )
    .await?;

    if let Some(sink) = progress {
        sink.report(1.0);
    }
    Ok(synced)
}

struct TargetJellyfinShowScan {
    media_source_id: i64,
    library_item_id: String,
    should_sync_items: bool,
    show_item_id: String,
    show_title: String,
}

async fn load_target_jellyfin_show_scan(
    pool: &DbPool,
    library_id: i64,
    show_id: i64,
) -> Result<TargetJellyfinShowScan> {
    let row = sqlx::query(
        r#"SELECT l."MediaKind",
                  l."MediaSourceId",
                  jl."ItemId" AS "LibraryItemId",
                  jl."ShouldSyncItems",
                  jsh."ItemId" AS "ShowItemId",
                  COALESCE(
                      (SELECT sm."Title"
                       FROM "ShowMetadata" sm
                       WHERE sm."ShowId" = jsh."Id"
                       ORDER BY sm."Id"
                       LIMIT 1),
                      'Unknown Show'
                  ) AS "ShowTitle"
           FROM "Library" l
           JOIN "JellyfinLibrary" jl ON jl."Id" = l."Id"
           JOIN "JellyfinShow" jsh ON jsh."Id" = ?
           JOIN "MediaItem" mi ON mi."Id" = jsh."Id"
           JOIN "LibraryPath" lp ON lp."Id" = mi."LibraryPathId"
           WHERE l."Id" = ? AND lp."LibraryId" = l."Id"
           LIMIT 1"#,
    )
    .bind(show_id)
    .bind(library_id)
    .fetch_optional(pool)
    .await
    .repository()?
    .ok_or_else(|| RepositoryError::Decode {
        kind: "jellyfin show",
        value: format!("show {show_id} does not exist in library {library_id}"),
    })?;

    let kind: LibraryMediaKind =
        decode_enum_column("jellyfin library media kind", &row, "MediaKind")?;
    if kind != LibraryMediaKind::Shows {
        return Err(RepositoryError::Decode {
            kind: "jellyfin library",
            value: format!("library {library_id} is not a TV show library"),
        });
    }

    let should_sync_items: i64 = row.try_get("ShouldSyncItems").unwrap_or(1);
    let show_title: String = row
        .try_get("ShowTitle")
        .unwrap_or_else(|_| "Unknown Show".to_owned());
    let media_source_id: i64 = row.try_get("MediaSourceId").unwrap_or(0);
    let library_item_id: String = row.try_get("LibraryItemId").unwrap_or_default();
    let show_item_id: String = row.try_get("ShowItemId").unwrap_or_default();

    Ok(TargetJellyfinShowScan {
        media_source_id,
        library_item_id,
        should_sync_items: should_sync_items != 0,
        show_item_id,
        show_title,
    })
}

async fn load_active_jellyfin_connection_config(
    pool: &DbPool,
    media_source_id: i64,
) -> Result<JellyfinConnectionConfig> {
    let address: String = sqlx::query_scalar(
        r#"SELECT "Address" FROM "JellyfinConnection"
           WHERE "JellyfinMediaSourceId" = ? ORDER BY "Id" LIMIT 1"#,
    )
    .bind(media_source_id)
    .fetch_optional(pool)
    .await
    .repository()?
    .and_then(|v: Option<String>| v)
    .ok_or_else(|| RepositoryError::Decode {
        kind: "jellyfin connection",
        value: format!("no connection for source {media_source_id}"),
    })?;

    let api_key = load_jellyfin_api_key(pool, &address).await?;
    let authorization_header = format_jellyfin_auth_header(api_key);
    Ok(JellyfinConnectionConfig {
        address,
        authorization_header,
    })
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
    sync_jellyfin_collection_library_with_progress(
        pool,
        service,
        config,
        media_source_id,
        parent_id,
        None,
    )
    .await
}

/// Variant of [`sync_jellyfin_collection_library`] that reports
/// `(synced / total_record_count)` after each collection upsert.
///
/// # Errors
///
/// Returns an error when the HTTP client fails or any database write fails.
#[allow(clippy::cast_precision_loss, clippy::cast_possible_truncation)]
pub async fn sync_jellyfin_collection_library_with_progress<S>(
    pool: &DbPool,
    service: &S,
    config: &JellyfinConnectionConfig,
    media_source_id: i64,
    parent_id: &str,
    progress: Option<&dyn ersatzrs_domain::ScanProgressSink>,
) -> Result<usize>
where
    S: JellyfinServiceTrait,
{
    let mut synced = 0;
    let mut collection_start_index = 0;
    let mut reported_initial = false;

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
        if !reported_initial {
            if let Some(sink) = progress {
                sink.report(0.0);
            }
            reported_initial = true;
        }
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
            if let Some(sink) = progress {
                if total_record_count > 0 {
                    sink.report(synced as f32 / total_record_count as f32);
                }
            }
        }

        collection_start_index += JELLYFIN_PAGE_SIZE;
        if collection_start_index >= total_record_count || page_len == 0 {
            break;
        }
    }

    if let Some(sink) = progress {
        sink.report(1.0);
    }
    Ok(synced)
}

fn project_library(response: JellyfinLibraryResponse) -> Option<JellyfinLibrarySummary> {
    let media_kind = match response.collection_type?.to_ascii_lowercase().as_str() {
        "movies" => LibraryMediaKind::Movies,
        "tvshows" => LibraryMediaKind::Shows,
        _ => return None,
    };
    let path_infos = response
        .library_options
        .map(|options| {
            options
                .path_infos
                .into_iter()
                .filter_map(|info| {
                    let network_path = info.network_path?;
                    if network_path.trim().is_empty() {
                        return None;
                    }
                    Some(JellyfinPathInfo {
                        path: info.path.unwrap_or_default(),
                        network_path,
                    })
                })
                .collect()
        })
        .unwrap_or_default();
    Some(JellyfinLibrarySummary {
        item_id: response.item_id?,
        name: response.name.unwrap_or_default(),
        media_kind,
        path_infos,
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
    sync_library_by_id_with_progress(pool, library_id, deep_scan, None).await
}

/// Variant of [`sync_library_by_id_with_options`] that forwards a progress
/// sink to the underlying branch fn.
///
/// # Errors
///
/// Returns an error when config loading, the Jellyfin API, or database writes
/// fail.
pub async fn sync_library_by_id_with_progress(
    pool: &DbPool,
    library_id: i64,
    deep_scan: bool,
    progress: Option<&dyn ersatzrs_domain::ScanProgressSink>,
) -> Result<usize> {
    let row = sqlx::query(
        r#"SELECT l."MediaKind", l."MediaSourceId"
           FROM "Library" l WHERE l."Id" = ? LIMIT 1"#,
    )
    .bind(library_id)
    .fetch_optional(pool)
    .await
    .repository()?
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
    .await
    .repository()?
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
            .await
            .repository()?
            .and_then(|v: Option<String>| v)
            .ok_or_else(|| RepositoryError::Decode {
                kind: "jellyfin library",
                value: format!("JellyfinLibrary row {library_id} not found"),
            })?;

    let service = JellyfinService::new();
    if kind == LibraryMediaKind::Shows {
        sync_jellyfin_show_library_with_progress(
            pool,
            &service,
            &config,
            media_source_id,
            library_id,
            &parent_id,
            deep_scan,
            progress,
        )
        .await
    } else {
        sync_jellyfin_movie_library_with_progress(
            pool,
            &service,
            &config,
            media_source_id,
            library_id,
            &parent_id,
            deep_scan,
            progress,
        )
        .await
    }
}

/// Sync Jellyfin collections for every library on `media_source_id` that has
/// `ShouldSyncItems = 1`. Mirrors ETV's `SynchronizeJellyfinCollections`
/// command dispatched from the External Collections row on the libraries
/// page.
///
/// `_deep_scan` matches the UI button contract;
/// `sync_jellyfin_collection_library` does not currently distinguish deep
/// from shallow scans, so the flag is reserved for future use.
///
/// # Errors
///
/// Returns an error when config loading, the Jellyfin API, or database writes
/// fail.
pub async fn sync_collections_for_media_source(
    pool: &DbPool,
    media_source_id: i64,
    _deep_scan: bool,
) -> Result<usize> {
    let library_ids: Vec<i64> = sqlx::query_scalar(
        r#"SELECT l."Id"
           FROM "Library" l
           JOIN "JellyfinLibrary" jl ON jl."Id" = l."Id"
           WHERE l."MediaSourceId" = ? AND jl."ShouldSyncItems" = 1
           ORDER BY l."Id""#,
    )
    .bind(media_source_id)
    .fetch_all(pool)
    .await
    .repository()?;

    if library_ids.is_empty() {
        return Ok(0);
    }

    let address: String = sqlx::query_scalar(
        r#"SELECT "Address" FROM "JellyfinConnection"
           WHERE "JellyfinMediaSourceId" = ? ORDER BY "Id" LIMIT 1"#,
    )
    .bind(media_source_id)
    .fetch_optional(pool)
    .await
    .repository()?
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
    let service = JellyfinService::new();

    let mut total = 0;
    for library_id in library_ids {
        let parent_id: Option<String> =
            sqlx::query_scalar(r#"SELECT "ItemId" FROM "JellyfinLibrary" WHERE "Id" = ?"#)
                .bind(library_id)
                .fetch_optional(pool)
                .await
                .repository()?
                .and_then(|v: Option<String>| v);
        let Some(parent_id) = parent_id else {
            continue;
        };
        total +=
            sync_jellyfin_collection_library(pool, &service, &config, media_source_id, &parent_id)
                .await?;
    }
    Ok(total)
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
        .await
        .repository()?
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

/// Builds a Jellyfin `Authorization` header from a raw API key.
///
/// Mirrors `JellyfinConnectionParameters.AuthorizationHeader` in `ErsatzTV`
/// (`MediaBrowser Token=<api_key>`). Already-formatted headers pass through.
#[must_use]
pub fn format_jellyfin_auth_header(value: String) -> String {
    if value.contains("Token=") || value.starts_with("MediaBrowser ") {
        value
    } else {
        format!("MediaBrowser Token=\"{value}\"")
    }
}

/// Server identity returned by Jellyfin's `/System/Info` endpoint. Used to
/// validate a freshly-saved API key/address pair before persisting it as the
/// active connection.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct JellyfinServerInformation {
    pub server_name: String,
    pub operating_system: String,
}

/// Calls `/System/Info` on the Jellyfin server and returns the basic server
/// identity. Mirrors `JellyfinApiClient.GetServerInformation` in `ErsatzTV`;
/// used by the connection editor to validate the API key before saving.
///
/// # Errors
///
/// Returns an error when the request fails, times out (5 s like `ErsatzTV`),
/// or the response cannot be parsed.
pub async fn fetch_server_information(
    address: &str,
    api_key: &str,
) -> std::result::Result<JellyfinServerInformation, String> {
    #[derive(Deserialize)]
    #[serde(rename_all = "PascalCase")]
    struct SystemInfoResponse {
        #[serde(default)]
        server_name: Option<String>,
        #[serde(default)]
        operating_system: Option<String>,
    }

    let url = format!("{}/System/Info", address.trim_end_matches('/'));
    let header = format_jellyfin_auth_header(api_key.to_owned());
    let client = Client::builder()
        .timeout(std::time::Duration::from_secs(5))
        .build()
        .map_err(|e| e.to_string())?;

    let response = client
        .get(url)
        .header("Authorization", header)
        .send()
        .await
        .map_err(|e| e.to_string())?;
    if !response.status().is_success() {
        return Err(format!("Jellyfin returned HTTP {}", response.status()));
    }
    let body: SystemInfoResponse = response.json().await.map_err(|e| e.to_string())?;
    Ok(JellyfinServerInformation {
        server_name: body.server_name.unwrap_or_else(|| "Jellyfin".to_string()),
        operating_system: body.operating_system.unwrap_or_default(),
    })
}

fn project_show(item: JellyfinLibraryItemResponse) -> Option<JellyfinShowItem> {
    let name = item.name.unwrap_or_default();
    let release_date = item.premiere_date;
    let tagline = item
        .taglines
        .iter()
        .find(|tagline| !tagline.trim().is_empty())
        .cloned();
    let item_id = item.id.as_deref().unwrap_or_default();
    let poster_artwork_path = item
        .image_tags
        .primary
        .as_deref()
        .filter(|tag| !tag.trim().is_empty())
        .map(|tag| format!("jellyfin://Items/{item_id}/Images/Primary?tag={tag}"));
    let thumb_artwork_path = item
        .image_tags
        .thumb
        .as_deref()
        .filter(|tag| !tag.trim().is_empty())
        .map(|tag| format!("jellyfin://Items/{item_id}/Images/Thumb?tag={tag}"));
    let fanart_artwork_path = item
        .backdrop_image_tags
        .iter()
        .find(|tag| !tag.trim().is_empty())
        .map(|tag| format!("jellyfin://Items/{item_id}/Images/Backdrop?tag={tag}"));
    let provider_guids = guids_from_provider_ids(&item.provider_ids);
    let genres = nonempty_strings(item.genres);
    let tags = nonempty_strings(item.tags);
    let studios = item
        .studios
        .into_iter()
        .filter_map(|s| s.name)
        .filter(|name| !name.trim().is_empty())
        .collect();
    let actors = project_actors(&item.people);
    Some(JellyfinShowItem {
        item_id: item.id?,
        etag: item.etag,
        sort_title: sort_title(&name),
        plot: item.overview,
        year: item.production_year,
        tagline,
        content_rating: item.official_rating,
        release_date,
        poster_artwork_path,
        thumb_artwork_path,
        fanart_artwork_path,
        provider_guids,
        genres,
        tags,
        studios,
        actors,
        name,
        date_created: item.date_created.unwrap_or_else(Utc::now),
    })
}

fn nonempty_strings(values: Vec<String>) -> Vec<String> {
    values
        .into_iter()
        .filter(|value| !value.trim().is_empty())
        .collect()
}

fn project_actors(people: &[JellyfinPersonResponse]) -> Vec<JellyfinActor> {
    people
        .iter()
        .filter(|person| {
            person
                .person_type
                .as_deref()
                .is_some_and(|person_type| person_type.eq_ignore_ascii_case("actor"))
        })
        .enumerate()
        .filter_map(|(idx, person)| {
            let name = person
                .name
                .as_ref()
                .filter(|name| !name.trim().is_empty())?
                .clone();
            let role = person
                .role
                .as_ref()
                .filter(|role| !role.trim().is_empty())
                .cloned();
            Some(JellyfinActor {
                name,
                role,
                order: Some(i32::try_from(idx).unwrap_or(i32::MAX)),
            })
        })
        .collect()
}

fn project_season(item: JellyfinLibraryItemResponse) -> Option<JellyfinSeasonItem> {
    let name = item.name.unwrap_or_default();
    let item_id = item.id.as_deref().unwrap_or_default();
    let poster_artwork_path = item
        .image_tags
        .primary
        .as_deref()
        .filter(|tag| !tag.trim().is_empty())
        .map(|tag| format!("jellyfin://Items/{item_id}/Images/Primary?tag={tag}"));
    let fanart_artwork_path = item
        .backdrop_image_tags
        .iter()
        .find(|tag| !tag.trim().is_empty())
        .map(|tag| format!("jellyfin://Items/{item_id}/Images/Backdrop?tag={tag}"));
    let provider_guids = guids_from_provider_ids(&item.provider_ids);
    Some(JellyfinSeasonItem {
        item_id: item.id?,
        etag: item.etag,
        sort_title: sort_title(&name),
        year: item.production_year,
        release_date: item.premiere_date,
        poster_artwork_path,
        fanart_artwork_path,
        provider_guids,
        date_created: item.date_created.unwrap_or_else(Utc::now),
        name,
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
    let name = item.name.unwrap_or_default();
    let release_date = item.premiere_date;
    let tagline = item
        .taglines
        .iter()
        .find(|tagline| !tagline.trim().is_empty())
        .cloned();
    let poster_artwork_path = item
        .image_tags
        .primary
        .filter(|tag| !tag.trim().is_empty())
        .map(|tag| {
            format!(
                "jellyfin://Items/{}/Images/Primary?tag={tag}",
                item.id.as_deref().unwrap_or_default()
            )
        });
    let fanart_artwork_path = item
        .backdrop_image_tags
        .iter()
        .find(|tag| !tag.trim().is_empty())
        .map(|tag| {
            format!(
                "jellyfin://Items/{}/Images/Backdrop?tag={tag}",
                item.id.as_deref().unwrap_or_default()
            )
        });
    let provider_guids = guids_from_provider_ids(&item.provider_ids);
    let genres = nonempty_strings(item.genres);
    let tags = nonempty_strings(item.tags);
    let actors = project_actors(&item.people);
    let directors = project_person_names(&item.people, "Director");
    let writers = project_person_names(&item.people, "Writer");
    Some(JellyfinMediaItem {
        item_id: item.id?,
        etag: item.etag,
        sort_title: sort_title(&name),
        plot: item.overview,
        year: item.production_year,
        tagline,
        content_rating: item.official_rating,
        release_date,
        episode_number: None,
        poster_artwork_path,
        thumb_artwork_path: None,
        fanart_artwork_path,
        provider_guids,
        genres,
        tags,
        actors,
        directors,
        writers,
        name,
        path,
        date_created: item.date_created.unwrap_or_else(Utc::now),
        duration,
        chapters: project_chapters(item.chapters, duration),
    })
}

fn project_episode(item: JellyfinLibraryItemResponse) -> Option<JellyfinMediaItem> {
    if item.location_type.as_deref() != Some("FileSystem") {
        return None;
    }
    let path = item.path?;
    if path.to_ascii_lowercase().ends_with(".strm") {
        return None;
    }
    let duration = Duration::nanoseconds(item.run_time_ticks * 100);
    let name = item.name.unwrap_or_default();
    let item_id = item.id.as_deref().unwrap_or_default();
    let thumb_artwork_path = item
        .image_tags
        .primary
        .as_deref()
        .filter(|tag| !tag.trim().is_empty())
        .map(|tag| format!("jellyfin://Items/{item_id}/Images/Primary?tag={tag}"));
    let provider_guids = guids_from_provider_ids(&item.provider_ids);
    let genres = nonempty_strings(item.genres);
    let tags = nonempty_strings(item.tags);
    let actors = project_actors(&item.people);
    let directors = project_person_names(&item.people, "Director");
    let writers = project_person_names(&item.people, "Writer");
    Some(JellyfinMediaItem {
        item_id: item.id?,
        etag: item.etag,
        sort_title: sort_title(&name),
        plot: item.overview,
        year: item.production_year,
        tagline: None,
        content_rating: None,
        release_date: item.premiere_date,
        episode_number: item.index_number,
        poster_artwork_path: None,
        thumb_artwork_path,
        fanart_artwork_path: None,
        provider_guids,
        genres,
        tags,
        actors,
        directors,
        writers,
        name,
        path,
        date_created: item.date_created.unwrap_or_else(Utc::now),
        duration,
        chapters: project_chapters(item.chapters, duration),
    })
}

fn project_person_names(people: &[JellyfinPersonResponse], person_type: &str) -> Vec<String> {
    people
        .iter()
        .filter(|person| {
            person
                .person_type
                .as_deref()
                .is_some_and(|value| value.eq_ignore_ascii_case(person_type))
        })
        .filter_map(|person| person.name.as_ref())
        .filter(|name| !name.trim().is_empty())
        .cloned()
        .collect()
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

    let is_video = matches!(kind, MediaStreamKind::Video);
    Some(MediaStream {
        index: stream.index,
        kind,
        codec: stream.codec.map(|codec| codec.to_ascii_lowercase()),
        profile: stream
            .profile
            .as_ref()
            .map(|value| value.to_ascii_lowercase()),
        language: stream.language,
        title: stream.title,
        default: stream.is_default,
        forced: stream.is_forced,
        channels: stream.channels,
        width: stream.width,
        height: stream.height,
        pixel_format: if is_video {
            stream.pixel_format.map(|value| value.to_ascii_lowercase())
        } else {
            None
        },
        color_range: if is_video {
            stream.color_range.map(|value| value.to_ascii_lowercase())
        } else {
            None
        },
        color_space: if is_video {
            stream.color_space.map(|value| value.to_ascii_lowercase())
        } else {
            None
        },
        color_transfer: if is_video {
            stream
                .color_transfer
                .map(|value| value.to_ascii_lowercase())
        } else {
            None
        },
        color_primaries: if is_video {
            stream
                .color_primaries
                .map(|value| value.to_ascii_lowercase())
        } else {
            None
        },
        bits_per_raw_sample: if is_video {
            stream.bit_depth.unwrap_or_default()
        } else {
            0
        },
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
) -> Result<i64> {
    let mut tx = pool.begin().await.repository()?;
    ensure_jellyfin_library(&mut tx, media_source_id, library_id, library_item_id).await?;
    let library_path_id =
        ensure_library_path(&mut tx, library_id, &format!("jellyfin://{library_id}")).await?;
    let item_id = ensure_jellyfin_movie_item(&mut tx, library_path_id, item).await?;
    let movie_metadata_id = upsert_movie_metadata(&mut tx, item_id, item).await?;
    let version_id =
        ensure_media_version(&mut tx, VersionOwner::Movie(item_id), item, playback).await?;
    ensure_media_file(&mut tx, version_id, &item.path).await?;
    replace_streams(&mut tx, version_id, &playback.streams).await?;
    crate::metadata::update_metadata_subtitles(
        &mut tx,
        crate::metadata::SubtitleMetadataOwner::Movie(movie_metadata_id),
        &playback.streams,
    )
    .await?;
    replace_chapters(&mut tx, version_id, &item.chapters).await?;
    tx.commit().await.repository()?;
    Ok(item_id)
}

async fn upsert_jellyfin_show(
    pool: &DbPool,
    media_source_id: i64,
    library_id: i64,
    library_item_id: &str,
    item: &JellyfinShowItem,
) -> Result<i64> {
    let mut tx = pool.begin().await.repository()?;
    ensure_jellyfin_library(&mut tx, media_source_id, library_id, library_item_id).await?;
    let library_path_id =
        ensure_library_path(&mut tx, library_id, &format!("jellyfin://{library_id}")).await?;
    let id = ensure_jellyfin_show_item(&mut tx, library_path_id, item).await?;
    upsert_show_metadata(&mut tx, id, item).await?;
    tx.commit().await.repository()?;
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
    let mut tx = pool.begin().await.repository()?;
    ensure_jellyfin_library(&mut tx, media_source_id, library_id, library_item_id).await?;
    let library_path_id =
        ensure_library_path(&mut tx, library_id, &format!("jellyfin://{library_id}")).await?;
    let id = ensure_jellyfin_season_item(&mut tx, library_path_id, show_id, item).await?;
    upsert_season_metadata(&mut tx, id, item).await?;
    tx.commit().await.repository()?;
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
    let mut tx = pool.begin().await.repository()?;
    ensure_jellyfin_library(&mut tx, media_source_id, library_id, library_item_id).await?;
    let library_path_id =
        ensure_library_path(&mut tx, library_id, &format!("jellyfin://{library_id}")).await?;
    let item_id = ensure_jellyfin_episode_item(&mut tx, library_path_id, season_id, item).await?;
    let episode_metadata_id = upsert_episode_metadata(&mut tx, item_id, item).await?;
    let version_id =
        ensure_media_version(&mut tx, VersionOwner::Episode(item_id), item, playback).await?;
    ensure_media_file(&mut tx, version_id, &item.path).await?;
    replace_streams(&mut tx, version_id, &playback.streams).await?;
    // Port of MediaServerTelevisionLibraryScanner.UpdateSubtitles
    // (ErsatzTV.Scanner/Core/Metadata/MediaServerTelevisionLibraryScanner.cs:832):
    // populate Subtitle rows so a later deep-scan extraction can query them.
    crate::metadata::update_episode_subtitles(&mut tx, episode_metadata_id, &playback.streams)
        .await?;
    replace_chapters(&mut tx, version_id, &item.chapters).await?;
    tx.commit().await.repository()?;
    Ok(())
}

async fn upsert_jellyfin_collection(
    pool: &DbPool,
    media_source_id: i64,
    item: &JellyfinCollectionItem,
) -> Result<i64> {
    let mut tx = pool.begin().await.repository()?;
    sqlx::query(r#"INSERT OR IGNORE INTO "MediaSource" ("Id") VALUES (?)"#)
        .bind(media_source_id)
        .execute(&mut *tx)
        .await
        .repository()?;
    sqlx::query(r#"INSERT OR IGNORE INTO "JellyfinMediaSource" ("Id") VALUES (?)"#)
        .bind(media_source_id)
        .execute(&mut *tx)
        .await
        .repository()?;

    let id = if let Some(row) =
        sqlx::query(r#"SELECT "Id" FROM "JellyfinCollection" WHERE "ItemId" = ?"#)
            .bind(&item.item_id)
            .fetch_optional(&mut *tx)
            .await
            .repository()?
    {
        let id = row.get("Id");
        sqlx::query(r#"UPDATE "JellyfinCollection" SET "Etag" = ?, "Name" = ? WHERE "Id" = ?"#)
            .bind(&item.etag)
            .bind(&item.name)
            .bind(id)
            .execute(&mut *tx)
            .await
            .repository()?;
        sqlx::query(r#"UPDATE "Collection" SET "Name" = ? WHERE "Id" = ?"#)
            .bind(&item.name)
            .bind(id)
            .execute(&mut *tx)
            .await
            .repository()?;
        id
    } else {
        let result = sqlx::query(
            r#"INSERT INTO "Collection" ("Name", "UseCustomPlaybackOrder") VALUES (?, 0)"#,
        )
        .bind(&item.name)
        .execute(&mut *tx)
        .await
        .repository()?;
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
        .await
        .repository()?;
        id
    };

    tx.commit().await.repository()?;
    Ok(id)
}

async fn replace_collection_items(
    pool: &DbPool,
    collection_id: i64,
    items: &[JellyfinCollectionMediaItem],
) -> Result<()> {
    let mut tx = pool.begin().await.repository()?;
    sqlx::query(r#"DELETE FROM "CollectionItem" WHERE "CollectionId" = ?"#)
        .bind(collection_id)
        .execute(&mut *tx)
        .await
        .repository()?;
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
            .await
            .repository()?;
        }
    }
    tx.commit().await.repository()?;
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
    Ok(sqlx::query(crate::db::assert_sql_safe(&sql))
        .bind(&item.item_id)
        .fetch_optional(&mut **tx)
        .await
        .repository()?
        .map(|row| row.get("Id")))
}

async fn item_etag_matches(
    pool: &DbPool,
    table: &str,
    library_id: i64,
    item_id: &str,
    etag: Option<&str>,
) -> Result<bool> {
    // Pair each provider subtype table with the metadata table it owns.
    // The etag short-circuit must fail when the matching metadata row is
    // missing so a rescan after a parity slice that newly writes that row
    // actually populates it instead of skipping the item as "unchanged".
    let (metadata_select, metadata_join) = match table {
        "JellyfinMovie" => (
            r#""MovieMetadata"."Id" AS "MetadataId""#,
            r#"LEFT JOIN "MovieMetadata" ON "MovieMetadata"."MovieId" = "JellyfinMovie"."Id""#,
        ),
        "JellyfinEpisode" => (
            r#""EpisodeMetadata"."Id" AS "MetadataId""#,
            r#"LEFT JOIN "EpisodeMetadata" ON "EpisodeMetadata"."EpisodeId" = "JellyfinEpisode"."Id""#,
        ),
        _ => (r#"NULL AS "MetadataId""#, ""),
    };
    let sql = format!(
        r#"SELECT "{table}"."Etag" AS "Etag", "MediaItem"."State" AS "State",
                  {metadata_select}
           FROM "{table}"
           INNER JOIN "MediaItem" ON "MediaItem"."Id" = "{table}"."Id"
           INNER JOIN "LibraryPath" ON "LibraryPath"."Id" = "MediaItem"."LibraryPathId"
           {metadata_join}
           WHERE "{table}"."ItemId" = ? AND "LibraryPath"."LibraryId" = ?"#
    );
    let Some(row) = sqlx::query(crate::db::assert_sql_safe(&sql))
        .bind(item_id)
        .bind(library_id)
        .fetch_optional(pool)
        .await
        .repository()?
    else {
        return Ok(false);
    };

    let existing_etag: Option<String> = row.try_get("Etag").repository()?;
    let state: i64 = row.try_get("State").repository()?;
    let has_metadata = !matches!(table, "JellyfinMovie" | "JellyfinEpisode")
        || row
            .try_get::<Option<i64>, _>("MetadataId")
            .repository()?
            .is_some();
    Ok(
        crate::media_server::provider_item_state_allows_etag_skip(state)
            && has_metadata
            && existing_etag.as_deref() == etag,
    )
}

/// Synchronizes the set of Jellyfin libraries for a media source.
///
/// Adds incoming libraries that have no `ItemId` match, deletes existing
/// libraries that the server no longer reports (cascading through
/// `Library`/`MediaItem`/`LibraryPath`), and refreshes the `MediaKind`,
/// `Name`, and `JellyfinPathInfo` rows for libraries that the server still
/// reports. Mirrors `MediaSourceRepository.UpdateLibraries` for Jellyfin in
/// `ErsatzTV`. The `MediaKind` is only updated when `ShouldSyncItems` is
/// false to avoid invalidating an in-progress sync.
///
/// # Errors
///
/// Returns an error when the database write fails.
pub async fn sync_jellyfin_libraries(
    pool: &DbPool,
    media_source_id: i64,
    incoming: &[JellyfinLibrarySummary],
) -> Result<()> {
    let existing_rows = sqlx::query(
        r#"SELECT "jl"."Id" AS "Id", "jl"."ItemId" AS "ItemId",
                  "jl"."ShouldSyncItems" AS "ShouldSyncItems",
                  "l"."MediaKind" AS "MediaKind"
           FROM "JellyfinLibrary" AS "jl"
           JOIN "Library" AS "l" ON "l"."Id" = "jl"."Id"
           WHERE "l"."MediaSourceId" = ?"#,
    )
    .bind(media_source_id)
    .fetch_all(pool)
    .await
    .repository()?;

    let mut existing: Vec<(i64, Option<String>, bool, i32)> = existing_rows
        .into_iter()
        .map(|row| {
            (
                row.get::<i64, _>("Id"),
                row.try_get::<Option<String>, _>("ItemId")
                    .unwrap_or_default(),
                row.try_get::<i64, _>("ShouldSyncItems").unwrap_or_default() != 0,
                row.try_get::<i32, _>("MediaKind").unwrap_or_default(),
            )
        })
        .collect();

    let mut tx = pool.begin().await.repository()?;
    for incoming_library in incoming {
        let position = existing.iter().position(|(_, item_id, _, _)| {
            item_id.as_deref() == Some(incoming_library.item_id.as_str())
        });
        if let Some(index) = position {
            let (id, _, should_sync, current_kind) = existing.swap_remove(index);
            let target_kind = encode_library_media_kind(incoming_library.media_kind);
            if !should_sync && target_kind != current_kind {
                sqlx::query(r#"UPDATE "Library" SET "MediaKind" = ?, "Name" = ? WHERE "Id" = ?"#)
                    .bind(target_kind)
                    .bind(&incoming_library.name)
                    .bind(id)
                    .execute(&mut *tx)
                    .await
                    .repository()?;
            } else {
                sqlx::query(r#"UPDATE "Library" SET "Name" = ? WHERE "Id" = ?"#)
                    .bind(&incoming_library.name)
                    .bind(id)
                    .execute(&mut *tx)
                    .await
                    .repository()?;
            }
            replace_jellyfin_path_infos_in_tx(&mut tx, id, &incoming_library.path_infos).await?;
        } else {
            let result = sqlx::query(
                r#"INSERT INTO "Library" ("Name", "MediaKind", "MediaSourceId")
                   VALUES (?, ?, ?)"#,
            )
            .bind(&incoming_library.name)
            .bind(encode_library_media_kind(incoming_library.media_kind))
            .bind(media_source_id)
            .execute(&mut *tx)
            .await
            .repository()?;
            let id = crate::db::last_insert_id(result);
            sqlx::query(
                r#"INSERT INTO "JellyfinLibrary" ("Id", "ItemId", "ShouldSyncItems")
                   VALUES (?, ?, 0)"#,
            )
            .bind(id)
            .bind(&incoming_library.item_id)
            .execute(&mut *tx)
            .await
            .repository()?;
            sqlx::query(r#"INSERT INTO "LibraryPath" ("LibraryId", "Path") VALUES (?, ?)"#)
                .bind(id)
                .bind(format!("jellyfin://{id}"))
                .execute(&mut *tx)
                .await
                .repository()?;
            replace_jellyfin_path_infos_in_tx(&mut tx, id, &incoming_library.path_infos).await?;
        }
    }

    for (id, _, _, _) in existing {
        sqlx::query(r#"DELETE FROM "Library" WHERE "Id" = ?"#)
            .bind(id)
            .execute(&mut *tx)
            .await
            .repository()?;
    }

    tx.commit().await.repository()?;
    Ok(())
}

fn encode_library_media_kind(kind: LibraryMediaKind) -> i32 {
    match kind {
        LibraryMediaKind::Movies => 1,
        LibraryMediaKind::Shows => 2,
        LibraryMediaKind::MusicVideos => 3,
        LibraryMediaKind::OtherVideos => 4,
        LibraryMediaKind::Songs => 5,
        LibraryMediaKind::Images => 6,
        LibraryMediaKind::RemoteStreams => 7,
    }
}

async fn replace_jellyfin_path_infos_in_tx(
    tx: &mut Transaction<'_, crate::db::Db>,
    library_id: i64,
    path_infos: &[JellyfinPathInfo],
) -> Result<()> {
    sqlx::query(r#"DELETE FROM "JellyfinPathInfo" WHERE "JellyfinLibraryId" = ?"#)
        .bind(library_id)
        .execute(&mut **tx)
        .await
        .repository()?;
    for info in path_infos {
        sqlx::query(
            r#"INSERT INTO "JellyfinPathInfo" ("JellyfinLibraryId", "Path", "NetworkPath")
               VALUES (?, ?, ?)"#,
        )
        .bind(library_id)
        .bind(&info.path)
        .bind(&info.network_path)
        .execute(&mut **tx)
        .await
        .repository()?;
    }
    Ok(())
}

/// Replaces all `JellyfinPathInfo` rows for a library with the supplied set.
///
/// Used to mirror `ErsatzTV` path mapping for SMB/NFS shares where the Jellyfin
/// server reports its local path and the network path that `ErsatzRS` should
/// rewrite onto. Network-only path infos with empty `NetworkPath` are
/// filtered out by the projection.
///
/// # Errors
///
/// Returns an error when the database write fails.
pub async fn replace_jellyfin_path_infos(
    pool: &DbPool,
    library_id: i64,
    path_infos: &[JellyfinPathInfo],
) -> Result<()> {
    let mut tx = pool.begin().await.repository()?;
    sqlx::query(r#"DELETE FROM "JellyfinPathInfo" WHERE "JellyfinLibraryId" = ?"#)
        .bind(library_id)
        .execute(&mut *tx)
        .await
        .repository()?;
    for info in path_infos {
        sqlx::query(
            r#"INSERT INTO "JellyfinPathInfo" ("JellyfinLibraryId", "Path", "NetworkPath")
               VALUES (?, ?, ?)"#,
        )
        .bind(library_id)
        .bind(&info.path)
        .bind(&info.network_path)
        .execute(&mut *tx)
        .await
        .repository()?;
    }
    tx.commit().await.repository()?;
    Ok(())
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
        .await
        .repository()?;
    sqlx::query(r#"INSERT OR IGNORE INTO "JellyfinMediaSource" ("Id") VALUES (?)"#)
        .bind(media_source_id)
        .execute(&mut **tx)
        .await
        .repository()?;
    sqlx::query(
        r#"INSERT OR IGNORE INTO "Library" ("Id", "Name", "MediaKind", "MediaSourceId")
           VALUES (?, 'Jellyfin', 0, ?)"#,
    )
    .bind(library_id)
    .bind(media_source_id)
    .execute(&mut **tx)
    .await
    .repository()?;
    sqlx::query(
        r#"INSERT OR IGNORE INTO "JellyfinLibrary" ("Id", "ItemId", "ShouldSyncItems")
           VALUES (?, ?, 1)"#,
    )
    .bind(library_id)
    .bind(library_item_id)
    .execute(&mut **tx)
    .await
    .repository()?;
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
        .await
        .repository()?
    {
        return Ok(row.get("Id"));
    }
    let result = sqlx::query(r#"INSERT INTO "LibraryPath" ("LibraryId", "Path") VALUES (?, ?)"#)
        .bind(library_id)
        .bind(path)
        .execute(&mut **tx)
        .await
        .repository()?;
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
    .await
    .repository()?
    {
        let id = row.get("Id");
        sqlx::query(r#"UPDATE "JellyfinMovie" SET "Etag" = ? WHERE "Id" = ?"#)
            .bind(&item.etag)
            .bind(id)
            .execute(&mut **tx)
            .await
            .repository()?;
        sqlx::query(r#"UPDATE "MediaItem" SET "LibraryPathId" = ?, "State" = 0 WHERE "Id" = ?"#)
            .bind(library_path_id)
            .bind(id)
            .execute(&mut **tx)
            .await
            .repository()?;
        return Ok(id);
    }

    let result = sqlx::query(r#"INSERT INTO "MediaItem" ("LibraryPathId", "State") VALUES (?, 0)"#)
        .bind(library_path_id)
        .execute(&mut **tx)
        .await
        .repository()?;
    let id = crate::db::last_insert_id(result);
    sqlx::query(r#"INSERT INTO "Movie" ("Id") VALUES (?)"#)
        .bind(id)
        .execute(&mut **tx)
        .await
        .repository()?;
    sqlx::query(r#"INSERT INTO "JellyfinMovie" ("Id", "ItemId", "Etag") VALUES (?, ?, ?)"#)
        .bind(id)
        .bind(&item.item_id)
        .bind(&item.etag)
        .execute(&mut **tx)
        .await
        .repository()?;
    Ok(id)
}

async fn upsert_movie_metadata(
    tx: &mut Transaction<'_, crate::db::Db>,
    movie_id: i64,
    item: &JellyfinMediaItem,
) -> Result<i64> {
    let existing_id: Option<i64> =
        sqlx::query_scalar(r#"SELECT "Id" FROM "MovieMetadata" WHERE "MovieId" = ? LIMIT 1"#)
            .bind(movie_id)
            .fetch_optional(&mut **tx)
            .await
            .repository()?;
    let now = Utc::now();

    let metadata_id = if let Some(id) = existing_id {
        sqlx::query(
            r#"UPDATE "MovieMetadata"
               SET "ContentRating" = ?, "DateAdded" = ?, "DateUpdated" = ?,
                   "MetadataKind" = 2, "OriginalTitle" = NULL, "Outline" = NULL,
                   "Plot" = ?, "ReleaseDate" = ?, "SortTitle" = ?,
                   "Tagline" = ?, "Title" = ?, "Year" = ?
               WHERE "Id" = ?"#,
        )
        .bind(&item.content_rating)
        .bind(encode_datetime(item.date_created))
        .bind(encode_datetime(now))
        .bind(&item.plot)
        .bind(item.release_date.map(encode_datetime))
        .bind(&item.sort_title)
        .bind(&item.tagline)
        .bind(&item.name)
        .bind(item.year)
        .bind(id)
        .execute(&mut **tx)
        .await
        .repository()?;
        id
    } else {
        let result = sqlx::query(
            r#"INSERT INTO "MovieMetadata" (
                "ContentRating", "DateAdded", "DateUpdated", "MetadataKind",
                "MovieId", "OriginalTitle", "Outline", "Plot", "ReleaseDate",
                "SortTitle", "Tagline", "Title", "Year"
            ) VALUES (?, ?, ?, 2, ?, NULL, NULL, ?, ?, ?, ?, ?, ?)"#,
        )
        .bind(&item.content_rating)
        .bind(encode_datetime(item.date_created))
        .bind(encode_datetime(now))
        .bind(movie_id)
        .bind(&item.plot)
        .bind(item.release_date.map(encode_datetime))
        .bind(&item.sort_title)
        .bind(&item.tagline)
        .bind(&item.name)
        .bind(item.year)
        .execute(&mut **tx)
        .await
        .repository()?;
        crate::db::last_insert_id(result)
    };

    replace_movie_artwork(tx, metadata_id, item).await?;
    replace_movie_guids(tx, metadata_id, &item.provider_guids).await?;
    Ok(metadata_id)
}

async fn replace_movie_artwork(
    tx: &mut Transaction<'_, crate::db::Db>,
    metadata_id: i64,
    item: &JellyfinMediaItem,
) -> Result<()> {
    let mut items = Vec::with_capacity(2);
    if let Some(path) = &item.poster_artwork_path {
        items.push(crate::metadata::ArtworkRef {
            kind: crate::metadata::artwork_kind::POSTER,
            path: path.clone(),
            date_added: item.date_created,
        });
    }
    if let Some(path) = &item.fanart_artwork_path {
        items.push(crate::metadata::ArtworkRef {
            kind: crate::metadata::artwork_kind::FAN_ART,
            path: path.clone(),
            date_added: item.date_created,
        });
    }
    crate::metadata::replace_artwork(
        tx,
        "MovieMetadataId",
        metadata_id,
        &[
            crate::metadata::artwork_kind::POSTER,
            crate::metadata::artwork_kind::FAN_ART,
        ],
        &items,
    )
    .await
}

async fn replace_movie_guids(
    tx: &mut Transaction<'_, crate::db::Db>,
    metadata_id: i64,
    guids: &[String],
) -> Result<()> {
    crate::metadata::replace_guids(tx, "MovieMetadataId", metadata_id, guids).await
}

/// Insert or update a `ShowMetadata` row + artwork + provider guids for a
/// Jellyfin show. Mirrors `upsert_movie_metadata` for shows. Ports the
/// `ProjectToShowMetadata` projection from `JellyfinApiClient.cs` plus the
/// EF-backed `Add/UpdateShow` flow in `JellyfinTelevisionRepository`.
/// Without this, `list_shows` returns zero rows because it joins on
/// `ShowMetadata`.
async fn upsert_show_metadata(
    tx: &mut Transaction<'_, crate::db::Db>,
    show_id: i64,
    item: &JellyfinShowItem,
) -> Result<()> {
    let existing_id: Option<i64> =
        sqlx::query_scalar(r#"SELECT "Id" FROM "ShowMetadata" WHERE "ShowId" = ? LIMIT 1"#)
            .bind(show_id)
            .fetch_optional(&mut **tx)
            .await
            .repository()?;
    let now = Utc::now();

    let metadata_id = if let Some(id) = existing_id {
        sqlx::query(
            r#"UPDATE "ShowMetadata"
               SET "ContentRating" = ?, "DateAdded" = ?, "DateUpdated" = ?,
                   "MetadataKind" = 2, "OriginalTitle" = NULL, "Outline" = NULL,
                   "Plot" = ?, "ReleaseDate" = ?, "SortTitle" = ?,
                   "Tagline" = ?, "Title" = ?, "Year" = ?
               WHERE "Id" = ?"#,
        )
        .bind(&item.content_rating)
        .bind(encode_datetime(item.date_created))
        .bind(encode_datetime(now))
        .bind(&item.plot)
        .bind(item.release_date.map(encode_datetime))
        .bind(&item.sort_title)
        .bind(&item.tagline)
        .bind(&item.name)
        .bind(item.year)
        .bind(id)
        .execute(&mut **tx)
        .await
        .repository()?;
        id
    } else {
        let result = sqlx::query(
            r#"INSERT INTO "ShowMetadata" (
                "ContentRating", "DateAdded", "DateUpdated", "MetadataKind",
                "ShowId", "OriginalTitle", "Outline", "Plot", "ReleaseDate",
                "SortTitle", "Tagline", "Title", "Year"
            ) VALUES (?, ?, ?, 2, ?, NULL, NULL, ?, ?, ?, ?, ?, ?)"#,
        )
        .bind(&item.content_rating)
        .bind(encode_datetime(item.date_created))
        .bind(encode_datetime(now))
        .bind(show_id)
        .bind(&item.plot)
        .bind(item.release_date.map(encode_datetime))
        .bind(&item.sort_title)
        .bind(&item.tagline)
        .bind(&item.name)
        .bind(item.year)
        .execute(&mut **tx)
        .await
        .repository()?;
        crate::db::last_insert_id(result)
    };

    replace_show_artwork(tx, metadata_id, item).await?;
    replace_show_guids(tx, metadata_id, &item.provider_guids).await?;
    replace_show_genres(tx, metadata_id, &item.genres).await?;
    replace_show_tags(tx, metadata_id, &item.tags).await?;
    replace_show_studios(tx, metadata_id, &item.studios).await?;
    replace_show_actors(tx, metadata_id, &item.actors).await?;
    Ok(())
}

async fn replace_show_artwork(
    tx: &mut Transaction<'_, crate::db::Db>,
    metadata_id: i64,
    item: &JellyfinShowItem,
) -> Result<()> {
    let mut items = Vec::with_capacity(3);
    if let Some(path) = &item.poster_artwork_path {
        items.push(crate::metadata::ArtworkRef {
            kind: crate::metadata::artwork_kind::POSTER,
            path: path.clone(),
            date_added: item.date_created,
        });
    }
    if let Some(path) = &item.thumb_artwork_path {
        items.push(crate::metadata::ArtworkRef {
            kind: crate::metadata::artwork_kind::THUMBNAIL,
            path: path.clone(),
            date_added: item.date_created,
        });
    }
    if let Some(path) = &item.fanart_artwork_path {
        items.push(crate::metadata::ArtworkRef {
            kind: crate::metadata::artwork_kind::FAN_ART,
            path: path.clone(),
            date_added: item.date_created,
        });
    }
    crate::metadata::replace_artwork(
        tx,
        "ShowMetadataId",
        metadata_id,
        &[
            crate::metadata::artwork_kind::POSTER,
            crate::metadata::artwork_kind::THUMBNAIL,
            crate::metadata::artwork_kind::FAN_ART,
        ],
        &items,
    )
    .await
}

async fn replace_show_guids(
    tx: &mut Transaction<'_, crate::db::Db>,
    metadata_id: i64,
    guids: &[String],
) -> Result<()> {
    crate::metadata::replace_guids(tx, "ShowMetadataId", metadata_id, guids).await
}

async fn replace_show_genres(
    tx: &mut Transaction<'_, crate::db::Db>,
    metadata_id: i64,
    genres: &[String],
) -> Result<()> {
    crate::metadata::replace_strings(tx, "Genre", "ShowMetadataId", metadata_id, genres).await
}

async fn replace_show_tags(
    tx: &mut Transaction<'_, crate::db::Db>,
    metadata_id: i64,
    tags: &[String],
) -> Result<()> {
    crate::metadata::replace_strings(tx, "Tag", "ShowMetadataId", metadata_id, tags).await
}

async fn replace_show_studios(
    tx: &mut Transaction<'_, crate::db::Db>,
    metadata_id: i64,
    studios: &[String],
) -> Result<()> {
    crate::metadata::replace_strings(tx, "Studio", "ShowMetadataId", metadata_id, studios).await
}

async fn replace_show_actors(
    tx: &mut Transaction<'_, crate::db::Db>,
    metadata_id: i64,
    actors: &[JellyfinActor],
) -> Result<()> {
    let people: Vec<crate::metadata::PersonRef> = actors
        .iter()
        .map(|actor| crate::metadata::PersonRef {
            name: actor.name.clone(),
            role: actor.role.clone(),
            order: actor.order,
        })
        .collect();
    crate::metadata::replace_people(tx, "Actor", "ShowMetadataId", metadata_id, &people).await
}

async fn upsert_season_metadata(
    tx: &mut Transaction<'_, crate::db::Db>,
    season_id: i64,
    item: &JellyfinSeasonItem,
) -> Result<()> {
    let existing_id: Option<i64> =
        sqlx::query_scalar(r#"SELECT "Id" FROM "SeasonMetadata" WHERE "SeasonId" = ? LIMIT 1"#)
            .bind(season_id)
            .fetch_optional(&mut **tx)
            .await
            .repository()?;
    let now = Utc::now();

    let metadata_id = if let Some(id) = existing_id {
        sqlx::query(
            r#"UPDATE "SeasonMetadata"
               SET "DateAdded" = ?, "DateUpdated" = ?, "MetadataKind" = 2,
                   "OriginalTitle" = NULL, "Outline" = NULL, "ReleaseDate" = ?,
                   "SortTitle" = ?, "Title" = ?, "Year" = ?
               WHERE "Id" = ?"#,
        )
        .bind(encode_datetime(item.date_created))
        .bind(encode_datetime(now))
        .bind(item.release_date.map(encode_datetime))
        .bind(&item.sort_title)
        .bind(&item.name)
        .bind(item.year)
        .bind(id)
        .execute(&mut **tx)
        .await
        .repository()?;
        id
    } else {
        let result = sqlx::query(
            r#"INSERT INTO "SeasonMetadata" (
                "DateAdded", "DateUpdated", "MetadataKind", "SeasonId",
                "OriginalTitle", "Outline", "ReleaseDate", "SortTitle", "Title", "Year"
            ) VALUES (?, ?, 2, ?, NULL, NULL, ?, ?, ?, ?)"#,
        )
        .bind(encode_datetime(item.date_created))
        .bind(encode_datetime(now))
        .bind(season_id)
        .bind(item.release_date.map(encode_datetime))
        .bind(&item.sort_title)
        .bind(&item.name)
        .bind(item.year)
        .execute(&mut **tx)
        .await
        .repository()?;
        crate::db::last_insert_id(result)
    };

    replace_season_artwork(tx, metadata_id, item).await?;
    crate::metadata::replace_guids(tx, "SeasonMetadataId", metadata_id, &item.provider_guids)
        .await?;
    Ok(())
}

async fn replace_season_artwork(
    tx: &mut Transaction<'_, crate::db::Db>,
    metadata_id: i64,
    item: &JellyfinSeasonItem,
) -> Result<()> {
    let mut items = Vec::with_capacity(2);
    if let Some(path) = &item.poster_artwork_path {
        items.push(crate::metadata::ArtworkRef {
            kind: crate::metadata::artwork_kind::POSTER,
            path: path.clone(),
            date_added: item.date_created,
        });
    }
    if let Some(path) = &item.fanart_artwork_path {
        items.push(crate::metadata::ArtworkRef {
            kind: crate::metadata::artwork_kind::FAN_ART,
            path: path.clone(),
            date_added: item.date_created,
        });
    }
    crate::metadata::replace_artwork(
        tx,
        "SeasonMetadataId",
        metadata_id,
        &[
            crate::metadata::artwork_kind::POSTER,
            crate::metadata::artwork_kind::FAN_ART,
        ],
        &items,
    )
    .await
}

async fn upsert_episode_metadata(
    tx: &mut Transaction<'_, crate::db::Db>,
    episode_id: i64,
    item: &JellyfinMediaItem,
) -> Result<i64> {
    let existing_id: Option<i64> =
        sqlx::query_scalar(r#"SELECT "Id" FROM "EpisodeMetadata" WHERE "EpisodeId" = ? LIMIT 1"#)
            .bind(episode_id)
            .fetch_optional(&mut **tx)
            .await
            .repository()?;
    let now = Utc::now();
    let episode_number = item.episode_number.unwrap_or_default();

    let metadata_id = if let Some(id) = existing_id {
        sqlx::query(
            r#"UPDATE "EpisodeMetadata"
               SET "DateAdded" = ?, "DateUpdated" = ?, "EpisodeNumber" = ?,
                   "MetadataKind" = 2, "OriginalTitle" = NULL, "Outline" = NULL,
                   "Plot" = ?, "ReleaseDate" = ?, "SortTitle" = ?,
                   "Tagline" = ?, "Title" = ?, "Year" = ?
               WHERE "Id" = ?"#,
        )
        .bind(encode_datetime(item.date_created))
        .bind(encode_datetime(now))
        .bind(episode_number)
        .bind(&item.plot)
        .bind(item.release_date.map(encode_datetime))
        .bind(&item.sort_title)
        .bind(&item.tagline)
        .bind(&item.name)
        .bind(item.year)
        .bind(id)
        .execute(&mut **tx)
        .await
        .repository()?;
        id
    } else {
        let result = sqlx::query(
            r#"INSERT INTO "EpisodeMetadata" (
                "DateAdded", "DateUpdated", "EpisodeNumber", "EpisodeId",
                "MetadataKind", "OriginalTitle", "Outline", "Plot", "ReleaseDate",
                "SortTitle", "Tagline", "Title", "Year"
            ) VALUES (?, ?, ?, ?, 2, NULL, NULL, ?, ?, ?, ?, ?, ?)"#,
        )
        .bind(encode_datetime(item.date_created))
        .bind(encode_datetime(now))
        .bind(episode_number)
        .bind(episode_id)
        .bind(&item.plot)
        .bind(item.release_date.map(encode_datetime))
        .bind(&item.sort_title)
        .bind(&item.tagline)
        .bind(&item.name)
        .bind(item.year)
        .execute(&mut **tx)
        .await
        .repository()?;
        crate::db::last_insert_id(result)
    };

    replace_episode_artwork(tx, metadata_id, item).await?;
    crate::metadata::replace_guids(tx, "EpisodeMetadataId", metadata_id, &item.provider_guids)
        .await?;
    crate::metadata::replace_strings(tx, "Genre", "EpisodeMetadataId", metadata_id, &item.genres)
        .await?;
    crate::metadata::replace_strings(tx, "Tag", "EpisodeMetadataId", metadata_id, &item.tags)
        .await?;
    replace_episode_actors(tx, metadata_id, &item.actors).await?;
    replace_episode_people(tx, "Director", metadata_id, &item.directors).await?;
    replace_episode_people(tx, "Writer", metadata_id, &item.writers).await?;
    Ok(metadata_id)
}

async fn replace_episode_artwork(
    tx: &mut Transaction<'_, crate::db::Db>,
    metadata_id: i64,
    item: &JellyfinMediaItem,
) -> Result<()> {
    let mut items = Vec::with_capacity(1);
    if let Some(path) = &item.thumb_artwork_path {
        items.push(crate::metadata::ArtworkRef {
            kind: crate::metadata::artwork_kind::THUMBNAIL,
            path: path.clone(),
            date_added: item.date_created,
        });
    }
    crate::metadata::replace_artwork(
        tx,
        "EpisodeMetadataId",
        metadata_id,
        &[crate::metadata::artwork_kind::THUMBNAIL],
        &items,
    )
    .await
}

async fn replace_episode_actors(
    tx: &mut Transaction<'_, crate::db::Db>,
    metadata_id: i64,
    actors: &[JellyfinActor],
) -> Result<()> {
    let people: Vec<crate::metadata::PersonRef> = actors
        .iter()
        .map(|actor| crate::metadata::PersonRef {
            name: actor.name.clone(),
            role: actor.role.clone(),
            order: actor.order,
        })
        .collect();
    crate::metadata::replace_people(tx, "Actor", "EpisodeMetadataId", metadata_id, &people).await
}

async fn replace_episode_people(
    tx: &mut Transaction<'_, crate::db::Db>,
    table: &'static str,
    metadata_id: i64,
    names: &[String],
) -> Result<()> {
    let people: Vec<crate::metadata::PersonRef> = names
        .iter()
        .map(|name| crate::metadata::PersonRef::new(name.clone()))
        .collect();
    crate::metadata::replace_people(tx, table, "EpisodeMetadataId", metadata_id, &people).await
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
    .await
    .repository()?
    {
        let id = row.get("Id");
        sqlx::query(r#"UPDATE "JellyfinShow" SET "Etag" = ? WHERE "Id" = ?"#)
            .bind(&item.etag)
            .bind(id)
            .execute(&mut **tx)
            .await
            .repository()?;
        sqlx::query(r#"UPDATE "MediaItem" SET "LibraryPathId" = ?, "State" = 0 WHERE "Id" = ?"#)
            .bind(library_path_id)
            .bind(id)
            .execute(&mut **tx)
            .await
            .repository()?;
        return Ok(id);
    }

    let result = sqlx::query(r#"INSERT INTO "MediaItem" ("LibraryPathId", "State") VALUES (?, 0)"#)
        .bind(library_path_id)
        .execute(&mut **tx)
        .await
        .repository()?;
    let id = crate::db::last_insert_id(result);
    sqlx::query(r#"INSERT INTO "Show" ("Id") VALUES (?)"#)
        .bind(id)
        .execute(&mut **tx)
        .await
        .repository()?;
    sqlx::query(r#"INSERT INTO "JellyfinShow" ("Id", "ItemId", "Etag") VALUES (?, ?, ?)"#)
        .bind(id)
        .bind(&item.item_id)
        .bind(&item.etag)
        .execute(&mut **tx)
        .await
        .repository()?;
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
    .await
    .repository()?
    {
        let id = row.get("Id");
        sqlx::query(r#"UPDATE "JellyfinSeason" SET "Etag" = ? WHERE "Id" = ?"#)
            .bind(&item.etag)
            .bind(id)
            .execute(&mut **tx)
            .await
            .repository()?;
        sqlx::query(r#"UPDATE "Season" SET "ShowId" = ?, "SeasonNumber" = ? WHERE "Id" = ?"#)
            .bind(show_id)
            .bind(item.season_number)
            .bind(id)
            .execute(&mut **tx)
            .await
            .repository()?;
        sqlx::query(r#"UPDATE "MediaItem" SET "LibraryPathId" = ?, "State" = 0 WHERE "Id" = ?"#)
            .bind(library_path_id)
            .bind(id)
            .execute(&mut **tx)
            .await
            .repository()?;
        return Ok(id);
    }

    let result = sqlx::query(r#"INSERT INTO "MediaItem" ("LibraryPathId", "State") VALUES (?, 0)"#)
        .bind(library_path_id)
        .execute(&mut **tx)
        .await
        .repository()?;
    let id = crate::db::last_insert_id(result);
    sqlx::query(r#"INSERT INTO "Season" ("Id", "ShowId", "SeasonNumber") VALUES (?, ?, ?)"#)
        .bind(id)
        .bind(show_id)
        .bind(item.season_number)
        .execute(&mut **tx)
        .await
        .repository()?;
    sqlx::query(r#"INSERT INTO "JellyfinSeason" ("Id", "ItemId", "Etag") VALUES (?, ?, ?)"#)
        .bind(id)
        .bind(&item.item_id)
        .bind(&item.etag)
        .execute(&mut **tx)
        .await
        .repository()?;
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
    .await
    .repository()?
    {
        let id = row.get("Id");
        sqlx::query(r#"UPDATE "JellyfinEpisode" SET "Etag" = ? WHERE "Id" = ?"#)
            .bind(&item.etag)
            .bind(id)
            .execute(&mut **tx)
            .await
            .repository()?;
        sqlx::query(r#"UPDATE "Episode" SET "SeasonId" = ? WHERE "Id" = ?"#)
            .bind(season_id)
            .bind(id)
            .execute(&mut **tx)
            .await
            .repository()?;
        sqlx::query(r#"UPDATE "MediaItem" SET "LibraryPathId" = ?, "State" = 0 WHERE "Id" = ?"#)
            .bind(library_path_id)
            .bind(id)
            .execute(&mut **tx)
            .await
            .repository()?;
        return Ok(id);
    }

    let result = sqlx::query(r#"INSERT INTO "MediaItem" ("LibraryPathId", "State") VALUES (?, 0)"#)
        .bind(library_path_id)
        .execute(&mut **tx)
        .await
        .repository()?;
    let id = crate::db::last_insert_id(result);
    sqlx::query(r#"INSERT INTO "Episode" ("Id", "SeasonId") VALUES (?, ?)"#)
        .bind(id)
        .bind(season_id)
        .execute(&mut **tx)
        .await
        .repository()?;
    sqlx::query(r#"INSERT INTO "JellyfinEpisode" ("Id", "ItemId", "Etag") VALUES (?, ?, ?)"#)
        .bind(id)
        .bind(&item.item_id)
        .bind(&item.etag)
        .execute(&mut **tx)
        .await
        .repository()?;
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
    if let Some(row) = sqlx::query(crate::db::assert_sql_safe(&sql))
        .bind(item_id)
        .fetch_optional(&mut **tx)
        .await
        .repository()?
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
    let result = sqlx::query(crate::db::assert_sql_safe(&sql))
        .bind(encode_datetime(item.date_created))
        .bind(encode_datetime(Utc::now()))
        .bind(&playback.display_aspect_ratio)
        .bind(encode_duration(playback.duration))
        .bind(playback.height)
        .bind(item_id)
        .bind(&playback.r_frame_rate)
        .bind(&playback.sample_aspect_ratio)
        .bind(VideoScanKind::Progressive.db_value())
        .bind(playback.width)
        .execute(&mut **tx)
        .await
        .repository()?;
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
    .await
    .repository()?;
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
    .await
    .repository()?;
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
        .await
        .repository()?;
    for stream in streams {
        sqlx::query(
            r#"INSERT INTO "MediaStream" (
                "Index", "Codec", "Profile", "MediaStreamKind", "Language", "Channels",
                "Title", "Default", "Forced", "MediaVersionId",
                "PixelFormat", "ColorRange", "ColorSpace", "ColorTransfer", "ColorPrimaries",
                "BitsPerRawSample", "AttachedPic", "FileName", "MimeType"
            ) VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)"#,
        )
        .bind(stream.index)
        .bind(&stream.codec)
        .bind(&stream.profile)
        .bind(encode_stream_kind(stream.kind))
        .bind(&stream.language)
        .bind(stream.channels.unwrap_or_default())
        .bind(&stream.title)
        .bind(stream.default)
        .bind(stream.forced)
        .bind(version_id)
        .bind(&stream.pixel_format)
        .bind(&stream.color_range)
        .bind(&stream.color_space)
        .bind(&stream.color_transfer)
        .bind(&stream.color_primaries)
        .bind(stream.bits_per_raw_sample)
        .bind(stream.attached_pic)
        .bind(&stream.file_name)
        .bind(&stream.mime_type)
        .execute(&mut **tx)
        .await
        .repository()?;
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
        .await
        .repository()?;
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
        .await
        .repository()?;
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

fn sort_title(title: &str) -> String {
    let lower = title.to_ascii_lowercase();
    if lower.starts_with("the ") {
        title[4..].to_owned()
    } else if lower.starts_with("a ") {
        title[2..].to_owned()
    } else if lower.starts_with("an ") {
        title[3..].to_owned()
    } else if title.starts_with('Æ') {
        title.replace('Æ', "E")
    } else {
        title.to_owned()
    }
}

fn guids_from_provider_ids(provider_ids: &JellyfinProviderIdsResponse) -> Vec<String> {
    let mut result = Vec::new();
    if let Some(value) = provider_ids
        .imdb
        .as_deref()
        .filter(|value| !value.trim().is_empty())
    {
        result.push(format!("imdb://{value}"));
    }
    if let Some(value) = provider_ids
        .tmdb
        .as_deref()
        .filter(|value| !value.trim().is_empty())
    {
        result.push(format!("tmdb://{value}"));
    }
    if let Some(value) = provider_ids
        .tvdb
        .as_deref()
        .filter(|value| !value.trim().is_empty())
    {
        result.push(format!("tvdb://{value}"));
    }
    result
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
    // Matches ErsatzTV.Core/Domain/MediaItem/MediaStreamKind.cs: 1-based enum.
    match kind {
        MediaStreamKind::Video => 1,
        MediaStreamKind::Audio => 2,
        MediaStreamKind::Subtitle => 3,
        MediaStreamKind::Attachment => 4,
        MediaStreamKind::ExternalSubtitle => 5,
    }
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "PascalCase")]
struct JellyfinLibraryResponse {
    name: Option<String>,
    collection_type: Option<String>,
    item_id: Option<String>,
    library_options: Option<JellyfinLibraryOptionsResponse>,
}

#[derive(Debug, Deserialize, Default)]
#[serde(rename_all = "PascalCase")]
struct JellyfinLibraryOptionsResponse {
    #[serde(default)]
    path_infos: Vec<JellyfinPathInfoResponse>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "PascalCase")]
struct JellyfinPathInfoResponse {
    path: Option<String>,
    network_path: Option<String>,
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
    official_rating: Option<String>,
    #[serde(default)]
    date_created: Option<DateTime<Utc>>,
    #[serde(default)]
    run_time_ticks: i64,
    #[serde(default)]
    production_year: Option<i32>,
    #[serde(default)]
    provider_ids: JellyfinProviderIdsResponse,
    #[serde(default)]
    premiere_date: Option<DateTime<Utc>>,
    location_type: Option<String>,
    overview: Option<String>,
    #[serde(default)]
    taglines: Vec<String>,
    #[serde(default)]
    image_tags: JellyfinImageTagsResponse,
    #[serde(default)]
    backdrop_image_tags: Vec<String>,
    #[serde(rename = "Type")]
    item_type: Option<String>,
    index_number: Option<i32>,
    #[serde(default)]
    chapters: Vec<JellyfinChapterResponse>,
    #[serde(default)]
    genres: Vec<String>,
    #[serde(default)]
    tags: Vec<String>,
    #[serde(default)]
    studios: Vec<JellyfinNameRefResponse>,
    #[serde(default)]
    people: Vec<JellyfinPersonResponse>,
}

#[derive(Debug, Default, Deserialize)]
#[serde(rename_all = "PascalCase")]
struct JellyfinProviderIdsResponse {
    imdb: Option<String>,
    tmdb: Option<String>,
    tvdb: Option<String>,
}

#[derive(Debug, Default, Deserialize)]
#[serde(rename_all = "PascalCase")]
struct JellyfinImageTagsResponse {
    primary: Option<String>,
    thumb: Option<String>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "PascalCase")]
struct JellyfinNameRefResponse {
    name: Option<String>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "PascalCase")]
struct JellyfinPersonResponse {
    name: Option<String>,
    role: Option<String>,
    #[serde(rename = "Type")]
    person_type: Option<String>,
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
    profile: Option<String>,
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
    pixel_format: Option<String>,
    color_range: Option<String>,
    color_space: Option<String>,
    color_transfer: Option<String>,
    color_primaries: Option<String>,
    bit_depth: Option<i32>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "PascalCase")]
struct JellyfinChapterResponse {
    #[serde(default)]
    start_position_ticks: i64,
    name: Option<String>,
}
