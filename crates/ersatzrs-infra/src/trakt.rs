use async_trait::async_trait;
use ersatzrs_core::repositories::{RepositoryError, Result};
use ersatzrs_core::trakt::{TraktApiClient as TraktApiClientTrait, TraktListItemWithGuids};
use ersatzrs_domain::{TraktList, TraktListItemKind};
use reqwest::{
    header::{HeaderValue, ACCEPT},
    Client,
};
use serde::Deserialize;

const TRAKT_BASE_URL: &str = "https://api.trakt.tv";
const APPLICATION_JSON: HeaderValue = HeaderValue::from_static("application/json");

/// HTTP client for the Trakt list API ported from
/// `ErsatzTV.Infrastructure/Trakt/TraktApiClient.cs`.
#[derive(Debug, Clone)]
pub struct TraktApiClient {
    client: Client,
    base_url: String,
    client_id: String,
}

/// Placeholder client for synchronizer operations that do not call Trakt.
#[derive(Debug, Clone, Copy, Default)]
pub struct NoopTraktApiClient;

#[async_trait]
impl TraktApiClientTrait for NoopTraktApiClient {
    async fn get_user_list(&self, _user: &str, _list: &str) -> Result<TraktList> {
        Err(RepositoryError::Conflict(
            "Trakt API client is not configured for this operation".to_string(),
        ))
    }

    async fn get_user_list_items(
        &self,
        _user: &str,
        _list: &str,
    ) -> Result<Vec<TraktListItemWithGuids>> {
        Err(RepositoryError::Conflict(
            "Trakt API client is not configured for this operation".to_string(),
        ))
    }
}

impl TraktApiClient {
    /// Build a Trakt API client from environment configuration.
    ///
    /// `TRAKT__CLIENTID` matches `ErsatzTV`'s documented environment variable.
    /// `TRAKT:ClientId` and `ErsatzRS_TRAKT__CLIENTID` are accepted for local
    /// parity with .NET configuration and the `ErsatzRS` config prefix.
    ///
    /// # Errors
    ///
    /// Returns an error when no client id is configured.
    pub fn from_env() -> Result<Self> {
        #[allow(clippy::disallowed_methods)]
        let client_id = std::env::var("TRAKT__CLIENTID")
            .ok()
            .or_else(|| std::env::var("TRAKT:ClientId").ok())
            .or_else(|| std::env::var("ErsatzRS_TRAKT__CLIENTID").ok())
            .filter(|value| !value.trim().is_empty())
            .ok_or_else(|| {
                RepositoryError::Conflict(
                    "Trakt ClientId is not configured; set TRAKT__CLIENTID".to_string(),
                )
            })?;

        #[allow(clippy::disallowed_methods)]
        let base_url = std::env::var("TRAKT__BASEURL")
            .ok()
            .or_else(|| std::env::var("ErsatzRS_TRAKT__BASEURL").ok())
            .filter(|value| !value.trim().is_empty())
            .unwrap_or_else(|| TRAKT_BASE_URL.to_string());

        Ok(Self::new(base_url, client_id))
    }

    #[must_use]
    pub fn new(base_url: String, client_id: String) -> Self {
        Self {
            client: Client::new(),
            base_url,
            client_id,
        }
    }

    async fn get_json<T>(&self, path: &str) -> Result<T>
    where
        T: for<'de> Deserialize<'de>,
    {
        let url = format!("{}{}", self.base_url.trim_end_matches('/'), path);
        self.client
            .get(url)
            .header(ACCEPT, APPLICATION_JSON)
            .header("trakt-api-version", "2")
            .header("trakt-api-key", &self.client_id)
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

#[async_trait]
impl TraktApiClientTrait for TraktApiClient {
    async fn get_user_list(&self, user: &str, list: &str) -> Result<TraktList> {
        let path = if user.eq_ignore_ascii_case("official") {
            format!("/lists/{list}")
        } else {
            format!("/users/{user}/lists/{list}")
        };
        let response: TraktListResponse = self.get_json(&path).await?;

        Ok(TraktList {
            id: 0,
            trakt_id: response.ids.trakt,
            user: response
                .user
                .ids
                .slug
                .unwrap_or_else(|| response.user.username.unwrap_or_default()),
            list: response.ids.slug.unwrap_or_else(|| list.to_string()),
            name: response.name.unwrap_or_default(),
            description: response.description,
            item_count: response.item_count,
            auto_refresh: false,
            generate_playlist: false,
            playlist_id: None,
            last_update: None,
            last_match: None,
            item_ids: Vec::new(),
        })
    }

    async fn get_user_list_items(
        &self,
        user: &str,
        list: &str,
    ) -> Result<Vec<TraktListItemWithGuids>> {
        let path = if user.eq_ignore_ascii_case("official") {
            format!("/lists/{list}/items")
        } else {
            format!("/users/{user}/lists/{list}/items")
        };
        let response: Vec<TraktListItemResponse> = self.get_json(&path).await?;

        Ok(response.into_iter().filter_map(project_item).collect())
    }
}

fn project_item(item: TraktListItemResponse) -> Option<TraktListItemWithGuids> {
    match item.item_type.to_ascii_lowercase().as_str() {
        "movie" => {
            let movie = item.movie?;
            let title = movie.title.unwrap_or_default();
            Some(TraktListItemWithGuids {
                trakt_id: item.id,
                rank: item.rank,
                display_title: format!("{} ({})", title, display_year(movie.year)),
                title,
                year: movie.year,
                season: 0,
                episode: 0,
                kind: TraktListItemKind::Movie,
                guids: guids_from_ids(&movie.ids),
            })
        }
        "show" => {
            let show = item.show?;
            let title = show.title.unwrap_or_default();
            Some(TraktListItemWithGuids {
                trakt_id: item.id,
                rank: item.rank,
                display_title: format!("{} ({})", title, display_year(show.year)),
                title,
                year: show.year,
                season: 0,
                episode: 0,
                kind: TraktListItemKind::Show,
                guids: guids_from_ids(&show.ids),
            })
        }
        "season" => {
            let show = item.show?;
            let season = item.season?;
            let title = show.title.unwrap_or_default();
            Some(TraktListItemWithGuids {
                trakt_id: item.id,
                rank: item.rank,
                display_title: format!(
                    "{} ({}) S{:02}",
                    title,
                    display_year(show.year),
                    season.number
                ),
                title,
                year: show.year,
                season: season.number,
                episode: 0,
                kind: TraktListItemKind::Season,
                guids: guids_from_ids(&season.ids),
            })
        }
        "episode" => {
            let show = item.show?;
            let episode = item.episode?;
            let title = show.title.unwrap_or_default();
            Some(TraktListItemWithGuids {
                trakt_id: item.id,
                rank: item.rank,
                display_title: format!(
                    "{} ({}) S{:02}E{:02}",
                    title,
                    display_year(show.year),
                    episode.season,
                    episode.number
                ),
                title,
                year: show.year,
                season: episode.season,
                episode: episode.number,
                kind: TraktListItemKind::Episode,
                guids: guids_from_ids(&episode.ids),
            })
        }
        _ => None,
    }
}

fn display_year(year: Option<i32>) -> String {
    year.map_or_else(String::new, |value| value.to_string())
}

fn guids_from_ids(ids: &TraktListItemIds) -> Vec<String> {
    let mut result = Vec::new();
    if let Some(imdb) = ids.imdb.as_ref().filter(|value| !value.trim().is_empty()) {
        result.push(format!("imdb://{imdb}"));
    }
    if let Some(tmdb) = ids.tmdb {
        result.push(format!("tmdb://{tmdb}"));
    }
    if let Some(tvdb) = ids.tvdb {
        result.push(format!("tvdb://{tvdb}"));
    }
    result
}

#[allow(clippy::needless_pass_by_value)]
fn provider_error(error: reqwest::Error) -> RepositoryError {
    RepositoryError::Decode {
        kind: "trakt http",
        value: error.to_string(),
    }
}

#[derive(Debug, Deserialize)]
struct TraktListResponse {
    name: Option<String>,
    description: Option<String>,
    #[serde(default)]
    item_count: i32,
    ids: TraktListIds,
    user: TraktUser,
}

#[derive(Debug, Default, Deserialize)]
struct TraktListIds {
    #[serde(default)]
    trakt: i32,
    slug: Option<String>,
}

#[derive(Debug, Deserialize)]
struct TraktUser {
    username: Option<String>,
    #[serde(default)]
    ids: TraktListIds,
}

#[derive(Debug, Deserialize)]
struct TraktListItemResponse {
    #[serde(default)]
    rank: i32,
    #[serde(default)]
    id: i32,
    #[serde(rename = "type", default)]
    item_type: String,
    movie: Option<TraktListItemMovie>,
    show: Option<TraktListItemShow>,
    season: Option<TraktListItemSeason>,
    episode: Option<TraktListItemEpisode>,
}

#[derive(Debug, Deserialize)]
struct TraktListItemMovie {
    title: Option<String>,
    year: Option<i32>,
    #[serde(default)]
    ids: TraktListItemIds,
}

#[derive(Debug, Deserialize)]
struct TraktListItemShow {
    title: Option<String>,
    year: Option<i32>,
    #[serde(default)]
    ids: TraktListItemIds,
}

#[derive(Debug, Deserialize)]
struct TraktListItemSeason {
    #[serde(default)]
    number: i32,
    #[serde(default)]
    ids: TraktListItemIds,
}

#[derive(Debug, Deserialize)]
struct TraktListItemEpisode {
    #[serde(default)]
    season: i32,
    #[serde(default)]
    number: i32,
    #[serde(default)]
    ids: TraktListItemIds,
}

#[derive(Debug, Default, Deserialize)]
struct TraktListItemIds {
    tvdb: Option<i32>,
    imdb: Option<String>,
    tmdb: Option<i32>,
}

#[cfg(test)]
mod tests {
    use ersatzrs_core::trakt::TraktApiClient as _;
    use httpmock::Method::GET;
    use httpmock::MockServer;

    use super::TraktApiClient;

    #[tokio::test]
    async fn fetches_user_list_with_ersatztv_headers() {
        let server = MockServer::start_async().await;
        let mock = server
            .mock_async(|when, then| {
                when.method(GET)
                    .path("/users/me/lists/favorites")
                    .header("accept", "application/json")
                    .header("trakt-api-version", "2")
                    .header("trakt-api-key", "client-id");
                then.status(200).json_body_obj(&serde_json::json!({
                    "name": "Favorites",
                    "description": "A list",
                    "item_count": 2,
                    "ids": { "trakt": 42, "slug": "favorites" },
                    "user": { "username": "me", "ids": { "slug": "me-slug" } }
                }));
            })
            .await;

        let client = TraktApiClient::new(server.base_url(), "client-id".to_string());
        let list = client.get_user_list("me", "favorites").await.unwrap();

        mock.assert_async().await;
        assert_eq!(list.trakt_id, 42);
        assert_eq!(list.user, "me-slug");
        assert_eq!(list.list, "favorites");
        assert_eq!(list.item_count, 2);
    }

    #[tokio::test]
    async fn maps_items_and_provider_guids() {
        let server = MockServer::start_async().await;
        server
            .mock_async(|when, then| {
                when.method(GET).path("/lists/popular/items");
                then.status(200).json_body_obj(&serde_json::json!([
                    {
                        "rank": 1,
                        "id": 100,
                        "type": "movie",
                        "movie": {
                            "title": "Movie",
                            "year": 1999,
                            "ids": { "trakt": 10, "imdb": "tt1", "tmdb": 20, "tvdb": 30 }
                        }
                    },
                    {
                        "rank": 2,
                        "id": 101,
                        "type": "episode",
                        "show": {
                            "title": "Show",
                            "year": 2001,
                            "ids": { "trakt": 11 }
                        },
                        "episode": {
                            "season": 3,
                            "number": 4,
                            "ids": { "trakt": 12, "tvdb": 40 }
                        }
                    }
                ]));
            })
            .await;

        let client = TraktApiClient::new(server.base_url(), "client-id".to_string());
        let items = client
            .get_user_list_items("official", "popular")
            .await
            .unwrap();

        assert_eq!(items.len(), 2);
        assert_eq!(items[0].display_title, "Movie (1999)");
        assert_eq!(items[0].guids, ["imdb://tt1", "tmdb://20", "tvdb://30"]);
        assert_eq!(items[1].display_title, "Show (2001) S03E04");
        assert_eq!(items[1].guids, ["tvdb://40"]);
    }
}
