use super::{
    async_trait, project_collection, project_collection_media_item, project_episode,
    project_library, project_movie, project_playback, project_season, project_show, Client,
    Deserialize, JellyfinCollectionMediaPage, JellyfinCollectionPage, JellyfinConnectionConfig,
    JellyfinEmbyProvider, JellyfinItemPage, JellyfinLibraryItemsResponse, JellyfinLibraryResponse,
    JellyfinLibrarySummary, JellyfinPlaybackInfoResponse, JellyfinPlaybackMetadata,
    JellyfinSeasonPage, JellyfinServiceTrait, JellyfinShowItem, JellyfinShowPage, RepositoryError,
    Result,
};

#[allow(clippy::needless_pass_by_value)]
fn provider_error(error: reqwest::Error) -> RepositoryError {
    crate::media_server::jellyfin_emby::provider_error(JellyfinEmbyProvider::Jellyfin, error)
}

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
