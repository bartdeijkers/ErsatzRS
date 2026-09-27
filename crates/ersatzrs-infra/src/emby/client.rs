use super::{
    async_trait, project_collection, project_collection_media_item, project_episode,
    project_library, project_movie, project_playback, project_season, project_show, Client,
    EmbyCollectionMediaPage, EmbyCollectionPage, EmbyConnectionConfig, EmbyItemPage,
    EmbyLibraryItemsResponse, EmbyLibraryResponse, EmbyLibrarySummary, EmbyPlaybackInfoResponse,
    EmbyPlaybackMetadata, EmbySeasonPage, EmbyServiceTrait, EmbyShowItem, EmbyShowPage,
    JellyfinEmbyProvider, RepositoryError, Result,
};

#[allow(clippy::needless_pass_by_value)]
fn provider_error(error: reqwest::Error) -> RepositoryError {
    crate::media_server::jellyfin_emby::provider_error(JellyfinEmbyProvider::Emby, error)
}

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
                "Path,Genres,Tags,DateCreated,Etag,Overview,Taglines,Studios,People,ProductionYear,PremiereDate,MediaSources,OfficialRating,ProviderIds,ImageTags",
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
                (
                    "fields",
                    "Path,DateCreated,Etag,ProviderIds,PremiereDate,ImageTags",
                ),
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
                    "Path,Genres,Tags,DateCreated,Etag,Overview,ProductionYear,PremiereDate,MediaSources,LocationType,ProviderIds,People,ImageTags,Chapters",
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
                .filter_map(project_episode)
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
            received_count: response.items.len(),
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
        if response.items.iter().any(|item| {
            item.id.as_deref().is_none_or(|id| id.trim().is_empty())
                || item
                    .item_type
                    .as_deref()
                    .is_none_or(|kind| kind.trim().is_empty())
                || (matches!(
                    item.item_type.as_deref(),
                    Some("Movie" | "Series" | "Season" | "Episode")
                ) && item.location_type.is_none())
        }) {
            return Err(crate::provider_collections::invalid_discovery());
        }
        Ok(EmbyCollectionMediaPage {
            total_record_count: response.total_record_count,
            received_count: response.items.len(),
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

    /// Fetch a single Emby show by its provider item id, scoped to a parent
    /// library.
    ///
    /// Mirrors ErsatzTV's `EmbyApiClient.GetSingleShow`, which queries
    /// `/Items?parentId=<library>&includeItemTypes=Series&ids=<showId>&recursive=false&limit=1`
    /// and projects the first item.
    ///
    /// # Errors
    ///
    /// Returns an error when the HTTP request or response decoding fails.
    pub async fn fetch_single_show(
        &self,
        config: &EmbyConnectionConfig,
        library_item_id: &str,
        show_item_id: &str,
    ) -> Result<Option<EmbyShowItem>> {
        let url = format!("{}/Items", config.address.trim_end_matches('/'));
        let response: EmbyLibraryItemsResponse = self
            .client
            .get(url)
            .header("X-Emby-Token", &config.api_key)
            .query(&[
                ("sortOrder", "Ascending"),
                ("sortBy", "SortName"),
                ("parentId", library_item_id),
                (
                    "fields",
                    "Path,Genres,Tags,DateCreated,Etag,Overview,Taglines,Studios,People,ProductionYear,PremiereDate,MediaSources,OfficialRating,ProviderIds,ImageTags",
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
}
