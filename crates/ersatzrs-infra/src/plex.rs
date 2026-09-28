use crate::db::DbPool;
use crate::repositories::common::decode_enum_column;
use crate::repositories::common::upsert_sql;
use crate::repositories::SqlxResultExt as _;
use chrono::{DateTime, Duration, TimeZone, Utc};
use ersatzrs_core::repositories::{RepositoryError, Result};
use ersatzrs_domain::{LibraryMediaKind, MediaStream, MediaStreamKind, VideoScanKind};
use reqwest::{
    header::{HeaderValue, ACCEPT},
    Client, Url,
};
use serde::Deserialize;
use sha2::{Digest, Sha256};
use sqlx::{Row, Transaction};

const PLEX_PAGE_SIZE: i32 = 100;
const APPLICATION_JSON: HeaderValue = HeaderValue::from_static("application/json");
const PLEX_TV_BASE: &str = "https://plex.tv";
const PLEX_APP_NAME: &str = "ErsatzRS";
const PLEX_CLIENT_IDENTIFIER_KEY: &str = "plex.client_identifier";
pub const PLEX_PENDING_PIN_CODE_KEY: &str = "plex.pending_pin_code";
pub const PLEX_USER_AUTH_TOKEN_KEY: &str = "plex.user_auth_token";
const PLEX_SERVER_AUTH_TOKEN_PREFIX: &str = "plex.server_auth_token.";
const PLEX_CURRENT_SERVER_AUTH_TOKEN_KEY: &str = "plex.current_server_auth_token";

mod authentication;
mod networks;
mod tag_ownership;
pub use networks::sync_plex_networks;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PlexConnectionConfig {
    pub uri: String,
    pub token: String,
}

/// Result of starting a Plex PIN auth flow.
#[derive(Debug, Clone)]
pub struct PlexAuthPin {
    pub id: String,
    pub code: String,
    pub auth_url: String,
}

/// A Plex server discovered via the plex.tv resources API.
#[derive(Debug, Clone)]
pub struct PlexServerResource {
    pub name: String,
    pub client_identifier: String,
    pub server_auth_token: String,
    pub uris: Vec<String>,
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
    pub original_title: Option<String>,
    pub year: Option<i32>,
    pub release_date: Option<String>,
    pub content_rating: Option<String>,
    pub summary: Option<String>,
    pub tagline: Option<String>,
    pub studios: Vec<String>,
    pub music_labels: Vec<String>,
    pub genres: Vec<String>,
    pub labels: Vec<PlexLabel>,
    pub guids: Vec<String>,
    pub actors: Vec<PlexPerson>,
    pub directors: Vec<PlexPerson>,
    pub writers: Vec<PlexPerson>,
    pub thumb: Option<String>,
    pub art: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PlexPerson {
    pub name: String,
    pub role: Option<String>,
    pub thumb: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PlexLabel {
    pub name: String,
    pub external_id: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PlexShowItem {
    pub key: String,
    pub etag: Option<String>,
    pub title: String,
    pub date_added: DateTime<Utc>,
    pub date_updated: DateTime<Utc>,
    pub labels: Vec<PlexLabel>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PlexSeasonItem {
    pub key: String,
    pub etag: Option<String>,
    pub title: String,
    pub season_number: i32,
    pub date_added: DateTime<Utc>,
    pub date_updated: DateTime<Utc>,
    pub labels: Vec<PlexLabel>,
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
    /// Number of raw response entries before supported-kind projection.
    pub received_count: usize,
    pub items: Vec<T>,
}

#[derive(Debug, Clone)]
pub struct PlexService {
    client: Client,
    plex_tv_base: String,
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
            plex_tv_base: PLEX_TV_BASE.to_owned(),
        }
    }

    /// List movie, show, and Other Video libraries from Plex.
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
            received_count: response.media_container.metadata.len(),
            items: response
                .media_container
                .metadata
                .into_iter()
                .filter_map(project_movie_item)
                .collect(),
        })
    }

    /// Fetch one page from a Plex Other Videos library.
    ///
    /// # Errors
    ///
    /// Returns an error when the HTTP request or response decoding fails.
    pub async fn fetch_other_video_page(
        &self,
        config: &PlexConnectionConfig,
        library_key: &str,
        start_index: i32,
        limit: i32,
    ) -> Result<PlexPage<PlexMediaItem>> {
        self.fetch_movie_page(config, library_key, start_index, limit)
            .await
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
            received_count: response.media_container.metadata.len(),
            items: response
                .media_container
                .metadata
                .into_iter()
                .filter_map(project_show)
                .collect(),
        })
    }

    /// Fetch a single Plex show by rating key.
    ///
    /// Mirrors ErsatzTV's `PlexServerApiClient.GetSingleShow`, which resolves
    /// `/library/metadata/{ratingKey}` before scanning that show's children.
    ///
    /// # Errors
    ///
    /// Returns an error when the HTTP request or response decoding fails.
    pub async fn fetch_single_show(
        &self,
        config: &PlexConnectionConfig,
        show_key: &str,
    ) -> Result<Option<PlexShowItem>> {
        let rating_key = plex_rating_key(show_key).ok_or_else(|| RepositoryError::Decode {
            kind: "plex show key",
            value: format!("unable to parse rating key from {show_key}"),
        })?;
        let url = format!(
            "{}/library/metadata/{rating_key}",
            config.uri.trim_end_matches('/')
        );
        let response: PlexMetadataResponse = self.get(config, url).await?;
        Ok(response
            .media_container
            .metadata
            .into_iter()
            .find_map(project_show))
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
            received_count: response.media_container.metadata.len(),
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
            received_count: response.media_container.metadata.len(),
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
            received_count: response.media_container.metadata.len(),
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

        if response.media_container.metadata.iter().any(|item| {
            item.key
                .as_deref()
                .or(item.rating_key.as_deref())
                .is_none_or(|id| id.trim().is_empty())
                || item
                    .item_type
                    .as_deref()
                    .is_none_or(|kind| kind.trim().is_empty())
        }) {
            return Err(crate::provider_collections::invalid_discovery());
        }
        Ok(PlexPage {
            total_record_count: response.media_container.total_size,
            received_count: response.media_container.metadata.len(),
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
        let key = plex_rating_key(key).ok_or_else(|| RepositoryError::Decode {
            kind: "plex metadata key",
            value: format!("unable to parse rating key from {key}"),
        })?;
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

    // ── Plex TV (plex.tv) OAuth / server-discovery API ───────────────────────

    /// Gets or creates a persistent client identifier in `ConfigElement`.
    ///
    /// Mirrors `IPlexSecretStore.GetClientIdentifier()`.
    ///
    /// # Errors
    ///
    /// Returns a database error when the read or write fails.
    pub async fn get_or_create_client_identifier(pool: &DbPool) -> Result<String> {
        if let Some(id) = sqlx::query_scalar::<_, String>(
            r#"SELECT "Value" FROM "ConfigElement" WHERE "Key" = ?"#,
        )
        .bind(PLEX_CLIENT_IDENTIFIER_KEY)
        .fetch_optional(pool)
        .await
        .repository()?
        .filter(|v| !v.is_empty())
        {
            return Ok(id);
        }
        let id = uuid::Uuid::new_v4().to_string();
        sqlx::query(
            r#"REPLACE INTO "ConfigElement" ("Key", "Value") VALUES (?, ?)
               "#,
        )
        .bind(PLEX_CLIENT_IDENTIFIER_KEY)
        .bind(&id)
        .execute(pool)
        .await
        .repository()?;
        Ok(id)
    }

    /// Initiates the Plex PIN auth flow. Returns a [`PlexAuthPin`] containing
    /// the PIN id, code, and the URL the user must visit to authorize.
    ///
    /// Mirrors `IPlexTvApiClient.StartPinFlow()`.
    ///
    /// # Errors
    ///
    /// Returns an error when the HTTP request to plex.tv fails.
    pub async fn start_pin_flow(&self, client_identifier: &str) -> Result<PlexAuthPin> {
        #[derive(Deserialize)]
        struct PinResponse {
            id: i64,
            code: String,
        }
        let resp: PinResponse = self
            .client
            .post(format!("{}/api/v2/pins", self.plex_tv_base))
            .timeout(std::time::Duration::from_secs(10))
            .header(ACCEPT, APPLICATION_JSON)
            .header("X-Plex-Product", PLEX_APP_NAME)
            .header("X-Plex-Version", env!("CARGO_PKG_VERSION"))
            .header("X-Plex-Client-Identifier", client_identifier)
            .query(&[("strong", "true")])
            .send()
            .await
            .map_err(provider_error)?
            .error_for_status()
            .map_err(provider_error)?
            .json()
            .await
            .map_err(provider_error)?;
        let auth_url = format!(
            "https://app.plex.tv/auth#?clientID={client_identifier}&code={code}&context[device][product]={PLEX_APP_NAME}",
            code = resp.code,
        );
        Ok(PlexAuthPin {
            id: resp.id.to_string(),
            code: resp.code,
            auth_url,
        })
    }

    /// Checks whether the Plex PIN has been authorized.
    ///
    /// Returns `Some(user_auth_token)` when the user has authorized,
    /// `None` if still pending.
    ///
    /// Mirrors `IPlexTvApiClient.TryCompletePinFlow()`.
    ///
    /// # Errors
    ///
    /// Returns an error when the HTTP request to plex.tv fails.
    pub async fn complete_pin_flow(
        &self,
        pin_id: &str,
        pin_code: &str,
        client_identifier: &str,
    ) -> Result<Option<String>> {
        #[derive(Deserialize)]
        #[serde(rename_all = "camelCase")]
        struct PinStatusResponse {
            #[serde(default)]
            auth_token: Option<String>,
        }
        let resp: PinStatusResponse = self
            .client
            .get(format!("{}/api/v2/pins/{pin_id}", self.plex_tv_base))
            .timeout(std::time::Duration::from_secs(10))
            .header(ACCEPT, APPLICATION_JSON)
            .header("X-Plex-Client-Identifier", client_identifier)
            .query(&[("code", pin_code)])
            .send()
            .await
            .map_err(provider_error)?
            .error_for_status()
            .map_err(provider_error)?
            .json()
            .await
            .map_err(provider_error)?;
        Ok(resp.auth_token.filter(|token| !token.trim().is_empty()))
    }

    /// Discovers Plex media servers accessible with `user_auth_token`.
    ///
    /// Filters to owned servers only, matching
    /// `SynchronizePlexMediaSourcesHandler` behavior.
    ///
    /// # Errors
    ///
    /// Returns an error when the HTTP request to plex.tv fails.
    pub async fn get_servers(
        &self,
        user_auth_token: &str,
        client_identifier: &str,
    ) -> Result<Vec<PlexServerResource>> {
        #[derive(Deserialize)]
        #[serde(rename_all = "camelCase")]
        struct ResourceResponse {
            name: String,
            client_identifier: String,
            access_token: String,
            provides: String,
            #[serde(default)]
            owned: bool,
            #[serde(default)]
            connections: Vec<ResourceConnectionResponse>,
        }
        #[derive(Deserialize)]
        struct ResourceConnectionResponse {
            uri: String,
        }
        let resources: Vec<ResourceResponse> = self
            .client
            .get(format!("{}/api/v2/resources", self.plex_tv_base))
            .timeout(std::time::Duration::from_secs(10))
            .header(ACCEPT, APPLICATION_JSON)
            .header("X-Plex-Token", user_auth_token)
            .header("X-Plex-Client-Identifier", client_identifier)
            .query(&[("includeHttps", "1"), ("includeIPv6", "1")])
            .send()
            .await
            .map_err(provider_error)?
            .error_for_status()
            .map_err(provider_error)?
            .json()
            .await
            .map_err(provider_error)?;
        Ok(resources
            .into_iter()
            .filter(|r| r.provides.split(',').any(|p| p.trim() == "server"))
            .filter(|r| r.owned)
            .map(|r| PlexServerResource {
                name: r.name,
                client_identifier: r.client_identifier,
                server_auth_token: r.access_token,
                uris: r.connections.into_iter().map(|c| c.uri).collect(),
            })
            .collect())
    }

    /// Upserts `PlexMediaSource` and `PlexConnection` rows for each discovered
    /// server, storing per-server auth tokens in `ConfigElement` atomically.
    /// Missing servers retain their content and receive a first-missing timestamp.
    /// Call only after a successful complete inventory, never for authentication
    /// failure or an unavailable provider.
    ///
    /// Mirrors `SynchronizePlexMediaSourcesHandler.SynchronizeAllServers`.
    ///
    /// # Errors
    ///
    /// Returns a database error on any write failure.
    pub async fn upsert_sources(pool: &DbPool, servers: &[PlexServerResource]) -> Result<()> {
        let mut tx = crate::db::begin_write(pool).await.repository()?;
        reconcile_plex_sources(&mut tx, servers).await?;
        tx.commit().await.repository()
    }
}

async fn reconcile_plex_sources(
    tx: &mut Transaction<'_, crate::db::Db>,
    servers: &[PlexServerResource],
) -> Result<()> {
    let existing_sources: Vec<(i64, Option<String>)> =
        sqlx::query_as(r#"SELECT "Id", "ClientIdentifier" FROM "PlexMediaSource""#)
            .fetch_all(&mut **tx)
            .await
            .repository()?;
    for server in servers {
        upsert_discovered_plex_source(tx, server).await?;
    }
    // A successful inventory can omit an unclaimed server. Retain its
    // libraries/media and preserve the first absence time until it returns.
    // Upstream: SynchronizePlexMediaSourcesHandler (#2990).
    let discovered: std::collections::HashSet<_> = servers
        .iter()
        .map(|server| server.client_identifier.as_str())
        .collect();
    let now = Utc::now();
    for (id, identifier) in existing_sources {
        if !identifier
            .as_deref()
            .is_some_and(|id| discovered.contains(id))
        {
            sqlx::query(
                r#"UPDATE "PlexMediaSource"
                   SET "MissingSince" = COALESCE("MissingSince", ?)
                   WHERE "Id" = ?"#,
            )
            .bind(now)
            .bind(id)
            .execute(&mut **tx)
            .await
            .repository()?;
        }
    }
    Ok(())
}

// One server write belongs to the inventory transaction; no partial credentials
// or source records are published if any later inventory write fails.
async fn upsert_discovered_plex_source(
    tx: &mut Transaction<'_, crate::db::Db>,
    server: &PlexServerResource,
) -> Result<()> {
    let existing: Option<i64> =
        sqlx::query_scalar(r#"SELECT "Id" FROM "PlexMediaSource" WHERE "ClientIdentifier" = ?"#)
            .bind(&server.client_identifier)
            .fetch_optional(&mut **tx)
            .await
            .repository()?;

    let ms_id = if let Some(id) = existing {
        sqlx::query(r#"UPDATE "PlexMediaSource" SET "ServerName" = ?, "MissingSince" = NULL WHERE "Id" = ?"#)
            .bind(&server.name)
            .bind(id)
            .execute(&mut **tx)
            .await
            .repository()?;
        id
    } else {
        let ms = sqlx::query(r#"INSERT INTO "MediaSource" DEFAULT VALUES"#)
            .execute(&mut **tx)
            .await
            .repository()?;
        let ms_id = crate::db::last_insert_id(ms);
        sqlx::query(
            r#"INSERT INTO "PlexMediaSource"
               ("Id", "ServerName", "ClientIdentifier")
               VALUES (?, ?, ?)"#,
        )
        .bind(ms_id)
        .bind(&server.name)
        .bind(&server.client_identifier)
        .execute(&mut **tx)
        .await
        .repository()?;
        ms_id
    };

    for uri in &server.uris {
        let already: bool = sqlx::query_scalar::<_, i64>(
            r#"SELECT COUNT(*) FROM "PlexConnection"
               WHERE "PlexMediaSourceId" = ? AND "Uri" = ?"#,
        )
        .bind(ms_id)
        .bind(uri)
        .fetch_one(&mut **tx)
        .await
        .repository()?
            > 0;
        if !already {
            sqlx::query(
                r#"INSERT INTO "PlexConnection"
                   ("IsActive", "Uri", "PlexMediaSourceId")
                   VALUES (0, ?, ?)"#,
            )
            .bind(uri)
            .bind(ms_id)
            .execute(&mut **tx)
            .await
            .repository()?;
        }
    }

    if let Some(first_uri) = server.uris.first() {
        sqlx::query(
            r#"UPDATE "PlexConnection" SET "IsActive" = 1
               WHERE "PlexMediaSourceId" = ? AND "Uri" = ?"#,
        )
        .bind(ms_id)
        .bind(first_uri)
        .execute(&mut **tx)
        .await
        .repository()?;
    }

    sqlx::query(
        r#"REPLACE INTO "ConfigElement" ("Key", "Value") VALUES (?, ?)
           "#,
    )
    .bind(format!(
        "{PLEX_SERVER_AUTH_TOKEN_PREFIX}{}",
        server.client_identifier
    ))
    .bind(&server.server_auth_token)
    .execute(&mut **tx)
    .await
    .repository()?;

    sqlx::query(
        r#"REPLACE INTO "ConfigElement" ("Key", "Value") VALUES (?, ?)
           "#,
    )
    .bind(PLEX_CURRENT_SERVER_AUTH_TOKEN_KEY)
    .bind(&server.server_auth_token)
    .execute(&mut **tx)
    .await
    .repository()?;
    Ok(())
}

/// Sync Plex movies into media rows.
///
/// # Errors
///
/// Returns an error when Plex requests or database writes fail.
pub async fn sync_plex_movie_library(
    pool: &DbPool,
    service: &PlexService,
    config: &PlexConnectionConfig,
    media_source_id: i64,
    library_id: i64,
    library_key: &str,
    deep_scan: bool,
) -> Result<usize> {
    sync_plex_movie_library_with_progress(
        pool,
        service,
        config,
        media_source_id,
        library_id,
        library_key,
        deep_scan,
        None,
    )
    .await
}

/// Variant of [`sync_plex_movie_library`] that emits `(synced / total_size)`
/// to `progress` after each item upsert. Use this from the web admin so the
/// libraries page can render a percentage ring.
///
/// # Errors
///
/// Returns an error when config loading, the Plex API, or database writes fail.
#[allow(
    clippy::too_many_arguments,
    clippy::cast_precision_loss,
    clippy::cast_possible_truncation
)]
pub async fn sync_plex_movie_library_with_progress(
    pool: &DbPool,
    service: &PlexService,
    config: &PlexConnectionConfig,
    media_source_id: i64,
    library_id: i64,
    library_key: &str,
    deep_scan: bool,
    progress: Option<&dyn ersatzrs_domain::ScanProgressSink>,
) -> Result<usize> {
    let mut synced = 0;
    let mut start_index = 0;
    let mut present_item_ids = std::collections::HashSet::new();
    let mut variant_candidates = Vec::new();
    let mut page = service
        .fetch_movie_page(config, library_key, start_index, PLEX_PAGE_SIZE)
        .await?;
    let total_size = page.total_record_count;

    if let Some(sink) = progress {
        sink.report(0.0);
    }

    while start_index < total_size {
        for item in &page.items {
            present_item_ids.insert(item.key.clone());
            let item_id = if !deep_scan
                && item_etag_matches(
                    pool,
                    "PlexMovie",
                    library_id,
                    &item.key,
                    item.etag.as_deref(),
                )
                .await?
            {
                synced += 1;
                crate::media_server::provider_media_item_id(
                    pool,
                    "PlexMovie",
                    "Key",
                    library_id,
                    &item.key,
                )
                .await
                .repository()?
            } else {
                let item_id =
                    upsert_plex_movie(pool, config, media_source_id, library_id, library_key, item)
                        .await?;
                synced += 1;
                Some(item_id)
            };
            if let Some(item_id) = item_id {
                variant_candidates.push(crate::media_server::ProviderVariantCandidate {
                    item_id,
                    title: item.title.clone(),
                    path: Some(item.path.clone()),
                    disambiguator: String::new(),
                    tags: Vec::new(),
                });
            }
            if let Some(sink) = progress {
                if total_size > 0 {
                    sink.report(synced as f32 / total_size as f32);
                }
            }
        }
        start_index += PLEX_PAGE_SIZE;
        if start_index >= total_size {
            break;
        }
        page = service
            .fetch_movie_page(config, library_key, start_index, PLEX_PAGE_SIZE)
            .await?;
    }

    finalize_plex_movie_library_sync(
        pool,
        media_source_id,
        library_id,
        &present_item_ids,
        &variant_candidates,
    )
    .await?;

    if let Some(sink) = progress {
        sink.report(1.0);
    }
    Ok(synced)
}

async fn finalize_plex_movie_library_sync(
    pool: &DbPool,
    media_source_id: i64,
    library_id: i64,
    present_item_ids: &std::collections::HashSet<String>,
    variant_candidates: &[crate::media_server::ProviderVariantCandidate],
) -> Result<()> {
    // Trash movies no longer present on the server, then flag those still on
    // the server whose local file is missing (RemoteOnly — Plex supports
    // remote streaming).
    crate::media_server::flag_missing_media_server_items(
        pool,
        "PlexMovie",
        "Key",
        library_id,
        present_item_ids,
    )
    .await
    .repository()?;
    let replacements = crate::media_server::load_media_server_path_replacements(
        pool,
        "PlexPathReplacement",
        "PlexPath",
        "PlexMediaSourceId",
        media_source_id,
    )
    .await
    .repository()?;
    crate::media_server::flag_unavailable_media_server_items(
        pool,
        "PlexMovie",
        "Key",
        library_id,
        present_item_ids,
        &replacements,
        true,
    )
    .await
    .repository()?;
    crate::media_server::apply_provider_content_variants(pool, variant_candidates)
        .await
        .repository()?;
    Ok(())
}

/// Sync a Plex Other Videos library into the schema-supported provider graph.
///
/// # Errors
///
/// Returns an error when the Plex request, response mapping, or database write
/// fails.
pub async fn sync_plex_other_video_library(
    pool: &DbPool,
    service: &PlexService,
    config: &PlexConnectionConfig,
    media_source_id: i64,
    library_id: i64,
    library_key: &str,
    deep_scan: bool,
) -> Result<usize> {
    sync_plex_other_video_library_with_progress(
        pool,
        service,
        config,
        media_source_id,
        library_id,
        library_key,
        deep_scan,
        None,
    )
    .await
}

/// Sync a Plex Other Videos library and report scan progress.
///
/// # Errors
///
/// Returns an error when the Plex request, response mapping, or database write
/// fails.
#[allow(
    clippy::too_many_arguments,
    clippy::cast_precision_loss,
    clippy::too_many_lines
)]
pub async fn sync_plex_other_video_library_with_progress(
    pool: &DbPool,
    service: &PlexService,
    config: &PlexConnectionConfig,
    media_source_id: i64,
    library_id: i64,
    library_key: &str,
    deep_scan: bool,
    progress: Option<&dyn ersatzrs_domain::ScanProgressSink>,
) -> Result<usize> {
    let mut synced = 0;
    let mut start_index = 0;
    let mut present_item_ids = std::collections::HashSet::new();
    let mut variant_candidates = Vec::new();
    let mut page = service
        .fetch_other_video_page(config, library_key, start_index, PLEX_PAGE_SIZE)
        .await?;
    let total_size = page.total_record_count;
    if let Some(sink) = progress {
        sink.report(0.0);
    }
    while start_index < total_size {
        for item in &page.items {
            present_item_ids.insert(item.key.clone());
            let item_id = if !deep_scan
                && item_etag_matches(
                    pool,
                    "PlexOtherVideo",
                    library_id,
                    &item.key,
                    item.etag.as_deref(),
                )
                .await?
            {
                crate::media_server::provider_media_item_id(
                    pool,
                    "PlexOtherVideo",
                    "Key",
                    library_id,
                    &item.key,
                )
                .await
                .repository()?
            } else {
                Some(
                    upsert_plex_other_video(
                        pool,
                        config,
                        media_source_id,
                        library_id,
                        library_key,
                        item,
                    )
                    .await?,
                )
            };
            synced += 1;
            if let Some(item_id) = item_id {
                variant_candidates.push(crate::media_server::ProviderVariantCandidate {
                    item_id,
                    title: item.title.clone(),
                    path: Some(item.path.clone()),
                    disambiguator: String::new(),
                    tags: Vec::new(),
                });
            }
            if let Some(sink) = progress {
                if total_size > 0 {
                    sink.report(synced as f32 / total_size as f32);
                }
            }
        }
        start_index += PLEX_PAGE_SIZE;
        if start_index < total_size {
            page = service
                .fetch_other_video_page(config, library_key, start_index, PLEX_PAGE_SIZE)
                .await?;
        }
    }
    crate::media_server::flag_missing_media_server_items(
        pool,
        "PlexOtherVideo",
        "Key",
        library_id,
        &present_item_ids,
    )
    .await
    .repository()?;
    let replacements = crate::media_server::load_media_server_path_replacements(
        pool,
        "PlexPathReplacement",
        "PlexPath",
        "PlexMediaSourceId",
        media_source_id,
    )
    .await
    .repository()?;
    crate::media_server::flag_unavailable_media_server_items(
        pool,
        "PlexOtherVideo",
        "Key",
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

/// Sync Plex shows, seasons, and episodes into media rows.
///
/// # Errors
///
/// Returns an error when Plex requests or database writes fail.
pub async fn sync_plex_show_library(
    pool: &DbPool,
    service: &PlexService,
    config: &PlexConnectionConfig,
    media_source_id: i64,
    library_id: i64,
    library_key: &str,
    deep_scan: bool,
) -> Result<usize> {
    sync_plex_show_library_with_progress(
        pool,
        service,
        config,
        media_source_id,
        library_id,
        library_key,
        deep_scan,
        None,
    )
    .await
}

/// Variant of [`sync_plex_show_library`] that emits progress at show
/// granularity (`shows_completed / show_total_size`). Episode-granular
/// progress would require a 3-deep nested upfront-count pass which is not
/// worth the additional network round-trips.
///
/// # Errors
///
/// Returns an error when config loading, the Plex API, or database writes fail.
#[allow(
    clippy::too_many_arguments,
    clippy::too_many_lines,
    clippy::cast_precision_loss,
    clippy::cast_possible_truncation
)]
pub async fn sync_plex_show_library_with_progress(
    pool: &DbPool,
    service: &PlexService,
    config: &PlexConnectionConfig,
    media_source_id: i64,
    library_id: i64,
    library_key: &str,
    deep_scan: bool,
    progress: Option<&dyn ersatzrs_domain::ScanProgressSink>,
) -> Result<usize> {
    let mut synced = 0;
    let mut shows_completed: usize = 0;
    let mut show_start_index = 0;
    let mut present_shows = std::collections::HashSet::new();
    let mut present_seasons = std::collections::HashSet::new();
    let mut present_episodes = std::collections::HashSet::new();
    let mut shows = service
        .fetch_show_page(config, library_key, show_start_index, PLEX_PAGE_SIZE)
        .await?;
    let show_total_size = shows.total_record_count;

    if let Some(sink) = progress {
        sink.report(0.0);
    }

    while show_start_index < show_total_size {
        for show in &shows.items {
            present_shows.insert(show.key.clone());
            synced += sync_one_plex_show(
                pool,
                service,
                config,
                media_source_id,
                library_id,
                library_key,
                show,
                deep_scan,
                &mut present_seasons,
                &mut present_episodes,
            )
            .await?;
            shows_completed += 1;
            if let Some(sink) = progress {
                if show_total_size > 0 {
                    sink.report(shows_completed as f32 / show_total_size as f32);
                }
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

    // Trash shows/seasons/episodes no longer present on the server.
    for (table, present) in [
        ("PlexShow", &present_shows),
        ("PlexSeason", &present_seasons),
        ("PlexEpisode", &present_episodes),
    ] {
        crate::media_server::flag_missing_media_server_items(
            pool, table, "Key", library_id, present,
        )
        .await
        .repository()?;
    }
    // Flag episodes still on the server whose local file is missing
    // (RemoteOnly — Plex supports remote streaming).
    let replacements = crate::media_server::load_media_server_path_replacements(
        pool,
        "PlexPathReplacement",
        "PlexPath",
        "PlexMediaSourceId",
        media_source_id,
    )
    .await
    .repository()?;
    crate::media_server::flag_unavailable_media_server_items(
        pool,
        "PlexEpisode",
        "Key",
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

#[allow(clippy::too_many_arguments)]
async fn sync_one_plex_show(
    pool: &DbPool,
    service: &PlexService,
    config: &PlexConnectionConfig,
    media_source_id: i64,
    library_id: i64,
    library_key: &str,
    show: &PlexShowItem,
    deep_scan: bool,
    present_seasons: &mut std::collections::HashSet<String>,
    present_episodes: &mut std::collections::HashSet<String>,
) -> Result<usize> {
    // Ports the body of
    // `$ERSATZTV_SRC/ErsatzTV.Scanner/Core/Plex/PlexTelevisionLibraryScanner.cs::ScanSingleShowInternal`.
    let mut synced = 0;
    let show_id = upsert_plex_show(pool, media_source_id, library_id, library_key, show).await?;
    let mut season_start_index = 0;
    let mut seasons = service
        .fetch_season_page(config, &show.key, season_start_index, PLEX_PAGE_SIZE)
        .await?;
    let season_total_size = seasons.total_record_count;

    while season_start_index < season_total_size {
        for season in &seasons.items {
            present_seasons.insert(season.key.clone());
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
                .fetch_episode_page(config, &season.key, episode_start_index, PLEX_PAGE_SIZE)
                .await?;
            let episode_total_size = episodes.total_record_count;

            while episode_start_index < episode_total_size {
                for episode in &episodes.items {
                    present_episodes.insert(episode.key.clone());
                    if !deep_scan
                        && item_etag_matches(
                            pool,
                            "PlexEpisode",
                            library_id,
                            &episode.key,
                            episode.etag.as_deref(),
                        )
                        .await?
                    {
                        crate::media_server::reconcile_episode_season(
                            pool,
                            crate::media_server::EpisodeProvider::Plex,
                            library_id,
                            &episode.key,
                            season_id,
                        )
                        .await
                        .repository()?;
                        synced += 1;
                        continue;
                    }

                    upsert_plex_episode(
                        pool,
                        media_source_id,
                        library_id,
                        library_key,
                        season_id,
                        episode,
                        config,
                    )
                    .await?;
                    synced += 1;
                }
                episode_start_index += PLEX_PAGE_SIZE;
                if episode_start_index >= episode_total_size {
                    break;
                }
                episodes = service
                    .fetch_episode_page(config, &season.key, episode_start_index, PLEX_PAGE_SIZE)
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

    Ok(synced)
}

/// Sync Plex collections and their memberships.
///
/// # Errors
///
/// Returns an error when Plex requests or database writes fail.
pub async fn sync_plex_collection_library(
    pool: &DbPool,
    service: &PlexService,
    config: &PlexConnectionConfig,
    media_source_id: i64,
    library_key: &str,
) -> Result<usize> {
    sync_plex_collection_library_with_progress(
        pool,
        service,
        config,
        media_source_id,
        library_key,
        None,
    )
    .await
}

/// Variant of [`sync_plex_collection_library`] that reports
/// `(start_index + items_seen_in_page) / total_size` after each page.
///
/// # Errors
///
/// Returns an error when Plex requests or database writes fail.
#[allow(clippy::cast_precision_loss, clippy::cast_possible_truncation)]
pub async fn sync_plex_collection_library_with_progress(
    pool: &DbPool,
    service: &PlexService,
    config: &PlexConnectionConfig,
    media_source_id: i64,
    library_key: &str,
    progress: Option<&dyn ersatzrs_domain::ScanProgressSink>,
) -> Result<usize> {
    use crate::provider_collections::{invalid_discovery, request, CompletePages};
    let library_section_id = library_key
        .parse::<i32>()
        .map_err(|_| invalid_discovery())?;
    let mut pages = CompletePages::new(PLEX_PAGE_SIZE);
    let mut gathered = Vec::new();
    let mut keys = std::collections::HashSet::new();
    if let Some(sink) = progress {
        sink.report(0.0);
    }
    loop {
        let page =
            request(service.fetch_collection_page(config, pages.start(), PLEX_PAGE_SIZE)).await?;
        let more = pages.advance(page.total_record_count, page.received_count)?;
        if page.received_count != page.items.len() {
            return Err(invalid_discovery());
        }
        for collection in page.items {
            if collection.library_section_id != library_section_id {
                continue;
            }
            if !keys.insert(collection.key.clone()) {
                return Err(invalid_discovery());
            }
            let mut member_pages = CompletePages::new(PLEX_PAGE_SIZE);
            let mut all_items = Vec::new();
            loop {
                let items = request(service.fetch_collection_items_page(
                    config,
                    &collection.key,
                    member_pages.start(),
                    PLEX_PAGE_SIZE,
                ))
                .await?;
                let more_members =
                    member_pages.advance(items.total_record_count, items.received_count)?;
                all_items.extend(items.items);
                if !more_members {
                    break;
                }
            }
            gathered.push((collection, all_items));
        }
        if let Some(sink) = progress {
            if more && page.total_record_count > 0 {
                sink.report(pages.start() as f32 / page.total_record_count as f32);
            }
        }
        if !more {
            break;
        }
    }
    let dependencies = crate::repositories::SqliteSchedulingContentRepository::new(pool.clone())
        .classic_media_dependencies()
        .await?;
    let mut tx = crate::db::begin_write(pool).await.repository()?;
    let before = crate::provider_collection_refresh::PublicationState::capture(
        &mut tx,
        crate::collection_tags::Provider::Plex,
        media_source_id,
    )
    .await?;
    let mut discovered = Vec::new();
    for (collection, items) in &gathered {
        let id = upsert_plex_collection(&mut tx, media_source_id, collection).await?;
        replace_plex_collection_items(&mut tx, id, media_source_id, items).await?;
        discovered.push(id);
    }
    crate::provider_collections::reconcile_root(
        &mut tx,
        crate::collection_tags::Provider::Plex,
        media_source_id,
        library_key,
        &discovered,
    )
    .await?;
    let after = crate::provider_collection_refresh::PublicationState::capture(
        &mut tx,
        crate::collection_tags::Provider::Plex,
        media_source_id,
    )
    .await?;
    before
        .persist_changes(&mut tx, after, &dependencies)
        .await?;
    tx.commit().await.repository()?;
    if let Some(sink) = progress {
        sink.report(1.0);
    }
    Ok(discovered.len())
}

async fn upsert_plex_movie(
    pool: &DbPool,
    config: &PlexConnectionConfig,
    media_source_id: i64,
    library_id: i64,
    library_key: &str,
    item: &PlexMediaItem,
) -> Result<i64> {
    let mut tx = crate::db::begin_write(pool).await.repository()?;
    ensure_plex_library(&mut tx, media_source_id, library_id, library_key).await?;
    let library_path_id =
        ensure_library_path(&mut tx, library_id, &format!("plex://{library_id}")).await?;
    let item_id = ensure_plex_movie_item(&mut tx, library_path_id, item).await?;
    let movie_metadata_id =
        upsert_plex_movie_metadata(&mut tx, item_id, media_source_id, item).await?;
    let version_id = ensure_media_version(&mut tx, VersionOwner::Movie(item_id), item).await?;
    ensure_media_file(&mut tx, version_id, item).await?;
    let streams = resolve_plex_external_subtitle_paths(&item.streams, config);
    replace_streams(&mut tx, version_id, &streams).await?;
    crate::metadata::update_metadata_subtitles(
        &mut tx,
        crate::metadata::SubtitleMetadataOwner::Movie(movie_metadata_id),
        &streams,
    )
    .await?;
    tx.commit().await.repository()?;
    Ok(item_id)
}

async fn upsert_plex_other_video(
    pool: &DbPool,
    config: &PlexConnectionConfig,
    media_source_id: i64,
    library_id: i64,
    library_key: &str,
    item: &PlexMediaItem,
) -> Result<i64> {
    let mut tx = crate::db::begin_write(pool).await.repository()?;
    ensure_plex_library(&mut tx, media_source_id, library_id, library_key).await?;
    let library_path_id =
        ensure_library_path(&mut tx, library_id, &format!("plex://{library_id}")).await?;
    let item_id = ensure_simple_media_item(
        &mut tx,
        "OtherVideo",
        "PlexOtherVideo",
        "Key",
        library_path_id,
        &item.key,
        item.etag.as_ref(),
    )
    .await?;
    let metadata_id =
        upsert_plex_other_video_metadata(&mut tx, item_id, media_source_id, item).await?;
    let version_id = ensure_media_version(&mut tx, VersionOwner::OtherVideo(item_id), item).await?;
    ensure_media_file(&mut tx, version_id, item).await?;
    let streams = resolve_plex_external_subtitle_paths(&item.streams, config);
    replace_streams(&mut tx, version_id, &streams).await?;
    crate::metadata::update_metadata_subtitles(
        &mut tx,
        crate::metadata::SubtitleMetadataOwner::OtherVideo(metadata_id),
        &streams,
    )
    .await?;
    tx.commit().await.repository()?;
    Ok(item_id)
}

async fn upsert_plex_show(
    pool: &DbPool,
    media_source_id: i64,
    library_id: i64,
    library_key: &str,
    item: &PlexShowItem,
) -> Result<i64> {
    let mut tx = crate::db::begin_write(pool).await.repository()?;
    ensure_plex_library(&mut tx, media_source_id, library_id, library_key).await?;
    let library_path_id =
        ensure_library_path(&mut tx, library_id, &format!("plex://{library_id}")).await?;
    let id = ensure_plex_show_item(&mut tx, library_path_id, item).await?;
    tag_ownership::replace_container_labels(
        &mut tx,
        tag_ownership::ContainerKind::Show,
        id,
        &item.title,
        (item.date_added, item.date_updated),
        &item.labels,
    )
    .await?;
    tx.commit().await.repository()?;
    Ok(id)
}

async fn upsert_plex_season(
    pool: &DbPool,
    media_source_id: i64,
    library_id: i64,
    library_key: &str,
    show_id: i64,
    item: &PlexSeasonItem,
) -> Result<i64> {
    let mut tx = crate::db::begin_write(pool).await.repository()?;
    ensure_plex_library(&mut tx, media_source_id, library_id, library_key).await?;
    let library_path_id =
        ensure_library_path(&mut tx, library_id, &format!("plex://{library_id}")).await?;
    let id = ensure_plex_season_item(&mut tx, library_path_id, show_id, item).await?;
    tag_ownership::replace_container_labels(
        &mut tx,
        tag_ownership::ContainerKind::Season,
        id,
        &item.title,
        (item.date_added, item.date_updated),
        &item.labels,
    )
    .await?;
    tx.commit().await.repository()?;
    Ok(id)
}

async fn upsert_plex_episode(
    pool: &DbPool,
    media_source_id: i64,
    library_id: i64,
    library_key: &str,
    season_id: i64,
    item: &PlexMediaItem,
    config: &PlexConnectionConfig,
) -> Result<()> {
    let mut tx = crate::db::begin_write(pool).await.repository()?;
    ensure_plex_library(&mut tx, media_source_id, library_id, library_key).await?;
    let library_path_id =
        ensure_library_path(&mut tx, library_id, &format!("plex://{library_id}")).await?;
    let item_id = ensure_plex_episode_item(&mut tx, library_path_id, season_id, item).await?;
    let episode_metadata_id =
        upsert_plex_episode_metadata(&mut tx, item_id, media_source_id, item).await?;
    let version_id = ensure_media_version(&mut tx, VersionOwner::Episode(item_id), item).await?;
    ensure_media_file(&mut tx, version_id, item).await?;
    let streams = resolve_plex_external_subtitle_paths(&item.streams, config);
    replace_streams(&mut tx, version_id, &streams).await?;
    // Port of MediaServerTelevisionLibraryScanner.UpdateSubtitles
    // (ErsatzTV.Scanner/Core/Metadata/MediaServerTelevisionLibraryScanner.cs:832):
    // Plex subtitle streams are diffed into Subtitle rows after stream writes.
    crate::metadata::update_episode_subtitles(&mut tx, episode_metadata_id, &streams).await?;
    tx.commit().await.repository()?;
    Ok(())
}

fn project_library(response: PlexLibraryResponse) -> Option<PlexLibrarySummary> {
    let media_kind = match response.library_type.as_str() {
        "movie"
            if response
                .agent
                .eq_ignore_ascii_case("com.plexapp.agents.none")
                || response
                    .scanner
                    .eq_ignore_ascii_case("Plex Video Files Scanner") =>
        {
            LibraryMediaKind::OtherVideos
        }
        "movie" => LibraryMediaKind::Movies,
        "other" => LibraryMediaKind::OtherVideos,
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
        date_added: unix_time(item.added_at),
        date_updated: unix_time(item.updated_at),
        labels: item
            .music_label
            .into_iter()
            .filter_map(project_label)
            .collect(),
    })
}

fn plex_rating_key(key: &str) -> Option<&str> {
    key.trim_matches('/')
        .split('/')
        .map(str::trim)
        .rfind(|segment| !segment.is_empty() && *segment != "children")
}

fn project_season(item: PlexMetadataItemResponse) -> Option<PlexSeasonItem> {
    Some(PlexSeasonItem {
        key: item.key.or(item.rating_key)?,
        etag: item.updated_at.map(|value| value.to_string()),
        title: item.title.unwrap_or_default(),
        season_number: item.index.unwrap_or_default(),
        date_added: unix_time(item.added_at),
        date_updated: unix_time(item.updated_at),
        labels: item
            .music_label
            .into_iter()
            .filter_map(project_label)
            .collect(),
    })
}

fn project_collection(item: PlexCollectionMetadataItemResponse) -> Option<PlexCollectionItem> {
    Some(PlexCollectionItem {
        key: item.rating_key?,
        etag: item.updated_at.map(|value| value.to_string()),
        title: item.title.unwrap_or_default(),
        library_section_id: item.library_section_id?,
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
        original_title: item.original_title,
        year: item.year,
        release_date: item.originally_available_at,
        content_rating: item.content_rating,
        summary: item.summary,
        tagline: item.tagline,
        studios: item.studio.into_iter().collect(),
        music_labels: plex_tag_values(item.music_label.clone()),
        genres: plex_tag_values(item.genre),
        labels: item
            .music_label
            .into_iter()
            .filter_map(project_label)
            .collect(),
        guids: item.guid.into_iter().filter_map(|guid| guid.id).collect(),
        actors: item.role.into_iter().filter_map(project_person).collect(),
        directors: item
            .director
            .into_iter()
            .filter_map(project_person)
            .collect(),
        writers: item.writer.into_iter().filter_map(project_person).collect(),
        thumb: item.thumb,
        art: item.art,
    })
}

fn project_movie_item(item: PlexMetadataItemResponse) -> Option<PlexMediaItem> {
    let mut item = project_media_item(item)?;
    // ErsatzTV's PlexEtag.ForMovie includes Title from row 68 onward. The
    // version prefix makes existing timestamp-only ETags refresh once, while
    // later title-only edits refresh movies and Other Videos on normal scans.
    item.etag = Some(format!(
        "title-v1:{}:{}",
        item.etag.as_deref().unwrap_or_default(),
        sha256_hex(&item.title)
    ));
    Some(item)
}

fn plex_tag_values(values: Vec<PlexTagResponse>) -> Vec<String> {
    ersatzrs_core::metadata_facets::normalize_facet_values(
        values.into_iter().filter_map(|value| value.tag),
    )
}

fn project_label(value: PlexTagResponse) -> Option<PlexLabel> {
    let name = value.tag?.trim().to_owned();
    if name.is_empty() {
        return None;
    }
    Some(PlexLabel {
        name,
        external_id: value.id.map(|id| match id {
            PlexTagId::Text(value) => value,
            PlexTagId::Number(value) => value.to_string(),
        }),
    })
}

fn project_person(value: PlexPersonResponse) -> Option<PlexPerson> {
    let name = value.tag?.trim().to_owned();
    if name.is_empty() {
        return None;
    }
    Some(PlexPerson {
        name,
        role: value.role.filter(|role| !role.trim().is_empty()),
        thumb: value.thumb.filter(|thumb| !thumb.trim().is_empty()),
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
    let is_video = matches!(kind, MediaStreamKind::Video);
    let mut projected = MediaStream {
        index: stream.index.unwrap_or(stream.id),
        kind,
        codec: stream.codec.map(|codec| codec.to_ascii_lowercase()),
        profile: stream
            .profile
            .as_ref()
            .map(|value| value.to_ascii_lowercase()),
        language: stream.language_code,
        title: stream.title,
        file_name: (kind == MediaStreamKind::ExternalSubtitle)
            .then_some(stream.key)
            .flatten(),
        default: stream.default,
        forced: stream.forced,
        channels: stream.channels,
        width: stream.width,
        height: stream.height,
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
            stream.color_trc.map(|value| value.to_ascii_lowercase())
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
    };
    if is_video {
        let detected_range = stream.video_range_type.as_deref().or_else(|| {
            stream
                .dovi_present
                .is_some_and(|value| value != 0)
                .then_some("DOVI")
        });
        crate::media_server::jellyfin_emby::apply_provider_dolby_vision_metadata(
            &mut projected,
            "Plex",
            detected_range,
            stream.dovi_profile,
            stream.dovi_bl_compat_id,
            stream.dovi_bl_present,
            stream.dovi_el_present,
            stream.dovi_rpu_present,
        );
    }
    Some(projected)
}

fn resolve_plex_external_subtitle_paths(
    streams: &[MediaStream],
    config: &PlexConnectionConfig,
) -> Vec<MediaStream> {
    streams
        .iter()
        .map(|stream| {
            let mut stream = stream.clone();
            if stream.kind == MediaStreamKind::ExternalSubtitle {
                stream.file_name = stream
                    .file_name
                    .as_deref()
                    .and_then(|key| plex_stream_url(config, key));
            }
            stream
        })
        .collect()
}

fn plex_stream_url(config: &PlexConnectionConfig, key: &str) -> Option<String> {
    let key = key.trim();
    if key.is_empty() {
        return None;
    }

    let mut url = if let Ok(url) = Url::parse(key) {
        url
    } else {
        Url::parse(&config.uri).ok()?.join(key).ok()?
    };

    if !config.token.trim().is_empty()
        && !url
            .query_pairs()
            .any(|(name, _)| name.eq_ignore_ascii_case("X-Plex-Token"))
    {
        url.query_pairs_mut()
            .append_pair("X-Plex-Token", &config.token);
    }

    Some(url.to_string())
}

/// Synchronizes the set of Plex libraries for a media source.
///
/// Adds incoming libraries that have no `Key` match, deletes existing
/// libraries that the server no longer reports (cascading through
/// `Library`/`MediaItem`/`LibraryPath`), and refreshes the `MediaKind` and
/// `Name` for libraries that the server still reports. Mirrors
/// `MediaSourceRepository.UpdateLibraries` for Plex in `ErsatzTV`. The
/// `MediaKind` is only updated when `ShouldSyncItems` is false to avoid
/// invalidating an in-progress sync.
///
/// # Errors
///
/// Returns an error when the database write fails.
pub async fn sync_plex_libraries(
    pool: &DbPool,
    media_source_id: i64,
    incoming: &[PlexLibrarySummary],
) -> Result<()> {
    let mut tx = crate::db::begin_write(pool).await.repository()?;
    let missing: bool = sqlx::query_scalar(
        r#"SELECT EXISTS(SELECT 1 FROM "PlexMediaSource"
           WHERE "Id" = ? AND "MissingSince" IS NOT NULL)"#,
    )
    .bind(media_source_id)
    .fetch_one(&mut *tx)
    .await
    .repository()?;
    if missing {
        return Ok(());
    }
    let existing_rows = sqlx::query(
        r#"SELECT "pl"."Id" AS "Id", "pl"."Key" AS "Key",
                  "pl"."ShouldSyncItems" AS "ShouldSyncItems",
                  "l"."MediaKind" AS "MediaKind"
           FROM "PlexLibrary" AS "pl"
           JOIN "Library" AS "l" ON "l"."Id" = "pl"."Id"
           WHERE "l"."MediaSourceId" = ?"#,
    )
    .bind(media_source_id)
    .fetch_all(&mut *tx)
    .await
    .repository()?;

    let mut existing: Vec<(i64, Option<String>, bool, i32)> = existing_rows
        .into_iter()
        .map(|row| {
            (
                row.get::<i64, _>("Id"),
                row.try_get::<Option<String>, _>("Key").unwrap_or_default(),
                row.try_get::<i64, _>("ShouldSyncItems").unwrap_or_default() != 0,
                row.try_get::<i32, _>("MediaKind").unwrap_or_default(),
            )
        })
        .collect();

    for incoming_library in incoming {
        let position = existing
            .iter()
            .position(|(_, key, _, _)| key.as_deref() == Some(incoming_library.key.as_str()));
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
                r#"INSERT INTO "PlexLibrary" ("Id", "Key", "ShouldSyncItems")
                   VALUES (?, ?, 0)"#,
            )
            .bind(id)
            .bind(&incoming_library.key)
            .execute(&mut *tx)
            .await
            .repository()?;
            sqlx::query(r#"INSERT INTO "LibraryPath" ("LibraryId", "Path") VALUES (?, ?)"#)
                .bind(id)
                .bind(format!("plex://{id}"))
                .execute(&mut *tx)
                .await
                .repository()?;
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

async fn ensure_plex_library(
    tx: &mut Transaction<'_, crate::db::Db>,
    media_source_id: i64,
    library_id: i64,
    key: &str,
) -> Result<()> {
    sqlx::query(r#"INSERT OR IGNORE INTO "MediaSource" ("Id") VALUES (?)"#)
        .bind(media_source_id)
        .execute(&mut **tx)
        .await
        .repository()?;
    sqlx::query(r#"INSERT OR IGNORE INTO "PlexMediaSource" ("Id") VALUES (?)"#)
        .bind(media_source_id)
        .execute(&mut **tx)
        .await
        .repository()?;
    sqlx::query(
        r#"INSERT OR IGNORE INTO "Library" ("Id", "Name", "MediaKind", "MediaSourceId")
           VALUES (?, 'Plex', 0, ?)"#,
    )
    .bind(library_id)
    .bind(media_source_id)
    .execute(&mut **tx)
    .await
    .repository()?;
    sqlx::query(
        r#"INSERT OR IGNORE INTO "PlexLibrary" ("Id", "Key", "ShouldSyncItems")
           VALUES (?, ?, 1)"#,
    )
    .bind(library_id)
    .bind(key)
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

async fn ensure_plex_movie_item(
    tx: &mut Transaction<'_, crate::db::Db>,
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
    tx: &mut Transaction<'_, crate::db::Db>,
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
    tx: &mut Transaction<'_, crate::db::Db>,
    library_path_id: i64,
    show_id: i64,
    item: &PlexSeasonItem,
) -> Result<i64> {
    if let Some(row) = sqlx::query(
        r#"SELECT "PlexSeason"."Id"
           FROM "PlexSeason"
           INNER JOIN "MediaItem" ON "MediaItem"."Id" = "PlexSeason"."Id"
           INNER JOIN "LibraryPath" existing_path
             ON existing_path."Id" = "MediaItem"."LibraryPathId"
           INNER JOIN "LibraryPath" current_path ON current_path."Id" = ?
           WHERE "PlexSeason"."Key" = ?
             AND existing_path."LibraryId" = current_path."LibraryId""#,
    )
    .bind(library_path_id)
    .bind(&item.key)
    .fetch_optional(&mut **tx)
    .await
    .repository()?
    {
        let id = row.get("Id");
        sqlx::query(r#"UPDATE "PlexSeason" SET "Etag" = ? WHERE "Id" = ?"#)
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
    let id = insert_media_item(tx, library_path_id).await?;
    sqlx::query(r#"INSERT INTO "Season" ("Id", "ShowId", "SeasonNumber") VALUES (?, ?, ?)"#)
        .bind(id)
        .bind(show_id)
        .bind(item.season_number)
        .execute(&mut **tx)
        .await
        .repository()?;
    sqlx::query(r#"INSERT INTO "PlexSeason" ("Id", "Key", "Etag") VALUES (?, ?, ?)"#)
        .bind(id)
        .bind(&item.key)
        .bind(&item.etag)
        .execute(&mut **tx)
        .await
        .repository()?;
    Ok(id)
}

async fn ensure_plex_episode_item(
    tx: &mut Transaction<'_, crate::db::Db>,
    library_path_id: i64,
    season_id: i64,
    item: &PlexMediaItem,
) -> Result<i64> {
    if let Some(row) = sqlx::query(
        r#"SELECT "PlexEpisode"."Id"
           FROM "PlexEpisode"
           INNER JOIN "MediaItem" ON "MediaItem"."Id" = "PlexEpisode"."Id"
           INNER JOIN "LibraryPath" existing_path
             ON existing_path."Id" = "MediaItem"."LibraryPathId"
           INNER JOIN "LibraryPath" current_path ON current_path."Id" = ?
           WHERE "PlexEpisode"."Key" = ?
             AND existing_path."LibraryId" = current_path."LibraryId""#,
    )
    .bind(library_path_id)
    .bind(&item.key)
    .fetch_optional(&mut **tx)
    .await
    .repository()?
    {
        let id = row.get("Id");
        sqlx::query(r#"UPDATE "PlexEpisode" SET "Etag" = ? WHERE "Id" = ?"#)
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
    let id = insert_media_item(tx, library_path_id).await?;
    sqlx::query(r#"INSERT INTO "Episode" ("Id", "SeasonId") VALUES (?, ?)"#)
        .bind(id)
        .bind(season_id)
        .execute(&mut **tx)
        .await
        .repository()?;
    sqlx::query(r#"INSERT INTO "PlexEpisode" ("Id", "Key", "Etag") VALUES (?, ?, ?)"#)
        .bind(id)
        .bind(&item.key)
        .bind(&item.etag)
        .execute(&mut **tx)
        .await
        .repository()?;
    Ok(id)
}

async fn upsert_plex_movie_metadata(
    tx: &mut Transaction<'_, crate::db::Db>,
    movie_id: i64,
    media_source_id: i64,
    item: &PlexMediaItem,
) -> Result<i64> {
    let existing_id: Option<i64> =
        sqlx::query_scalar(r#"SELECT "Id" FROM "MovieMetadata" WHERE "MovieId" = ? LIMIT 1"#)
            .bind(movie_id)
            .fetch_optional(&mut **tx)
            .await
            .repository()?;
    let date_added = encode_datetime(item.date_added);
    let date_updated = encode_datetime(item.date_updated);
    let ratings =
        ersatzrs_core::metadata_facets::normalize_content_ratings(item.content_rating.iter());
    let rating = (!ratings.is_empty()).then(|| ratings.join("/"));

    let metadata_id = if let Some(id) = existing_id {
        sqlx::query(
            r#"UPDATE "MovieMetadata"
               SET "DateAdded" = ?, "DateUpdated" = ?, "MetadataKind" = 2,
                   "SortTitle" = ?, "Title" = ?, "OriginalTitle" = ?, "Year" = ?,
                   "ReleaseDate" = ?, "ContentRating" = ?, "Plot" = ?, "Tagline" = ?
               WHERE "Id" = ?"#,
        )
        .bind(&date_added)
        .bind(&date_updated)
        .bind(&item.title)
        .bind(&item.title)
        .bind(&item.original_title)
        .bind(item.year)
        .bind(&item.release_date)
        .bind(&rating)
        .bind(&item.summary)
        .bind(&item.tagline)
        .bind(id)
        .execute(&mut **tx)
        .await
        .repository()?;
        id
    } else {
        let result = sqlx::query(
            r#"INSERT INTO "MovieMetadata" (
                "DateAdded", "DateUpdated", "MetadataKind", "MovieId", "SortTitle", "Title",
                "OriginalTitle", "Year", "ReleaseDate", "ContentRating", "Plot", "Tagline"
            ) VALUES (?, ?, 2, ?, ?, ?, ?, ?, ?, ?, ?, ?)"#,
        )
        .bind(&date_added)
        .bind(&date_updated)
        .bind(movie_id)
        .bind(&item.title)
        .bind(&item.title)
        .bind(&item.original_title)
        .bind(item.year)
        .bind(&item.release_date)
        .bind(&rating)
        .bind(&item.summary)
        .bind(&item.tagline)
        .execute(&mut **tx)
        .await
        .repository()?;
        crate::db::last_insert_id(result)
    };
    replace_plex_metadata_facets(tx, "MovieMetadataId", metadata_id, media_source_id, item).await?;
    Ok(metadata_id)
}

#[allow(clippy::too_many_lines)]
async fn upsert_plex_other_video_metadata(
    tx: &mut Transaction<'_, crate::db::Db>,
    other_video_id: i64,
    media_source_id: i64,
    item: &PlexMediaItem,
) -> Result<i64> {
    let existing_id: Option<i64> = sqlx::query_scalar(
        r#"SELECT "Id" FROM "OtherVideoMetadata" WHERE "OtherVideoId" = ? LIMIT 1"#,
    )
    .bind(other_video_id)
    .fetch_optional(&mut **tx)
    .await
    .repository()?;
    let ratings =
        ersatzrs_core::metadata_facets::normalize_content_ratings(item.content_rating.iter());
    let rating = (!ratings.is_empty()).then(|| ratings.join("/"));
    let date_added = encode_datetime(item.date_added);
    let date_updated = encode_datetime(item.date_updated);
    let metadata_id = if let Some(id) = existing_id {
        sqlx::query(
            r#"UPDATE "OtherVideoMetadata"
               SET "DateAdded" = ?, "DateUpdated" = ?, "MetadataKind" = 2,
                   "Title" = ?, "OriginalTitle" = ?, "SortTitle" = ?, "Year" = ?,
                   "ReleaseDate" = ?, "ContentRating" = ?, "Plot" = ?, "Tagline" = ?
               WHERE "Id" = ?"#,
        )
        .bind(&date_added)
        .bind(&date_updated)
        .bind(&item.title)
        .bind(&item.original_title)
        .bind(&item.title)
        .bind(item.year)
        .bind(&item.release_date)
        .bind(&rating)
        .bind(&item.summary)
        .bind(&item.tagline)
        .bind(id)
        .execute(&mut **tx)
        .await
        .repository()?;
        id
    } else {
        let result = sqlx::query(
            r#"INSERT INTO "OtherVideoMetadata" (
                "OtherVideoId", "DateAdded", "DateUpdated", "MetadataKind", "Title",
                "OriginalTitle", "SortTitle", "Year", "ReleaseDate", "ContentRating",
                "Plot", "Tagline"
            ) VALUES (?, ?, ?, 2, ?, ?, ?, ?, ?, ?, ?, ?)"#,
        )
        .bind(other_video_id)
        .bind(&date_added)
        .bind(&date_updated)
        .bind(&item.title)
        .bind(&item.original_title)
        .bind(&item.title)
        .bind(item.year)
        .bind(&item.release_date)
        .bind(&rating)
        .bind(&item.summary)
        .bind(&item.tagline)
        .execute(&mut **tx)
        .await
        .repository()?;
        crate::db::last_insert_id(result)
    };
    replace_plex_metadata_facets(
        tx,
        "OtherVideoMetadataId",
        metadata_id,
        media_source_id,
        item,
    )
    .await?;
    Ok(metadata_id)
}

async fn replace_plex_metadata_facets(
    tx: &mut Transaction<'_, crate::db::Db>,
    owner_column: &'static str,
    metadata_id: i64,
    media_source_id: i64,
    item: &PlexMediaItem,
) -> Result<()> {
    for (table, values) in [
        ("Genre", &item.genres),
        ("Studio", &item.studios),
        ("MusicLabel", &item.music_labels),
    ] {
        crate::metadata::replace_strings(tx, table, owner_column, metadata_id, values).await?;
    }
    tag_ownership::replace_labels(tx, owner_column, metadata_id, &item.labels).await?;
    let people = |values: &[PlexPerson]| {
        values
            .iter()
            .enumerate()
            .map(|(index, person)| {
                crate::metadata::PersonRef::with_role(
                    person.name.clone(),
                    person.role.clone(),
                    i32::try_from(index).ok(),
                )
            })
            .collect::<Vec<_>>()
    };
    crate::metadata::replace_people(
        tx,
        "Actor",
        owner_column,
        metadata_id,
        &people(&item.actors),
    )
    .await?;
    crate::metadata::replace_people(
        tx,
        "Director",
        owner_column,
        metadata_id,
        &people(&item.directors),
    )
    .await?;
    crate::metadata::replace_people(
        tx,
        "Writer",
        owner_column,
        metadata_id,
        &people(&item.writers),
    )
    .await?;
    crate::metadata::replace_guids(tx, owner_column, metadata_id, &item.guids).await?;
    let artwork = [
        item.thumb.as_ref().map(|path| crate::metadata::ArtworkRef {
            kind: crate::metadata::artwork_kind::THUMBNAIL,
            path: format!("plex://{media_source_id}/{}", path.trim_start_matches('/')),
            date_added: item.date_added,
        }),
        item.art.as_ref().map(|path| crate::metadata::ArtworkRef {
            kind: crate::metadata::artwork_kind::FAN_ART,
            path: format!("plex://{media_source_id}/{}", path.trim_start_matches('/')),
            date_added: item.date_added,
        }),
    ]
    .into_iter()
    .flatten()
    .collect::<Vec<_>>();
    crate::metadata::replace_artwork(
        tx,
        owner_column,
        metadata_id,
        &[
            crate::metadata::artwork_kind::THUMBNAIL,
            crate::metadata::artwork_kind::FAN_ART,
        ],
        &artwork,
    )
    .await?;
    Ok(())
}

async fn upsert_plex_episode_metadata(
    tx: &mut Transaction<'_, crate::db::Db>,
    episode_id: i64,
    media_source_id: i64,
    item: &PlexMediaItem,
) -> Result<i64> {
    let existing_id: Option<i64> =
        sqlx::query_scalar(r#"SELECT "Id" FROM "EpisodeMetadata" WHERE "EpisodeId" = ? LIMIT 1"#)
            .bind(episode_id)
            .fetch_optional(&mut **tx)
            .await
            .repository()?;
    let date_added = encode_datetime(item.date_added);
    let date_updated = encode_datetime(item.date_updated);
    let ratings =
        ersatzrs_core::metadata_facets::normalize_content_ratings(item.content_rating.iter());
    let rating = (!ratings.is_empty()).then(|| ratings.join("/"));

    let metadata_id = if let Some(id) = existing_id {
        sqlx::query(
            r#"UPDATE "EpisodeMetadata"
               SET "DateAdded" = ?, "DateUpdated" = ?, "EpisodeNumber" = ?,
                   "MetadataKind" = 2, "SortTitle" = ?, "Title" = ?,
                   "OriginalTitle" = ?, "Year" = ?, "ReleaseDate" = ?,
                   "ContentRating" = ?, "Plot" = ?, "Tagline" = ?
               WHERE "Id" = ?"#,
        )
        .bind(&date_added)
        .bind(&date_updated)
        .bind(item.episode_number)
        .bind(&item.title)
        .bind(&item.title)
        .bind(&item.original_title)
        .bind(item.year)
        .bind(&item.release_date)
        .bind(&rating)
        .bind(&item.summary)
        .bind(&item.tagline)
        .bind(id)
        .execute(&mut **tx)
        .await
        .repository()?;
        id
    } else {
        let result = sqlx::query(
            r#"INSERT INTO "EpisodeMetadata" (
                "DateAdded", "DateUpdated", "EpisodeNumber", "EpisodeId",
                "MetadataKind", "SortTitle", "Title", "OriginalTitle", "Year",
                "ReleaseDate", "ContentRating", "Plot", "Tagline"
            ) VALUES (?, ?, ?, ?, 2, ?, ?, ?, ?, ?, ?, ?, ?)"#,
        )
        .bind(&date_added)
        .bind(&date_updated)
        .bind(item.episode_number)
        .bind(episode_id)
        .bind(&item.title)
        .bind(&item.title)
        .bind(&item.original_title)
        .bind(item.year)
        .bind(&item.release_date)
        .bind(&rating)
        .bind(&item.summary)
        .bind(&item.tagline)
        .execute(&mut **tx)
        .await
        .repository()?;
        crate::db::last_insert_id(result)
    };
    replace_plex_metadata_facets(tx, "EpisodeMetadataId", metadata_id, media_source_id, item)
        .await?;
    Ok(metadata_id)
}

async fn upsert_plex_collection(
    tx: &mut Transaction<'_, crate::db::Db>,
    media_source_id: i64,
    item: &PlexCollectionItem,
) -> Result<i64> {
    crate::provider_collections::upsert_tx(
        tx,
        crate::collection_tags::Provider::Plex,
        media_source_id,
        &item.key,
        item.etag.as_deref(),
        &item.title,
    )
    .await
}

async fn replace_plex_collection_items(
    tx: &mut Transaction<'_, crate::db::Db>,
    collection_id: i64,
    media_source_id: i64,
    items: &[PlexCollectionMediaItem],
) -> Result<()> {
    let previous = crate::collection_tags::members(tx, collection_id).await?;
    let mut incoming = Vec::new();
    for item in items {
        if let Some(id) = find_plex_media_item(tx, media_source_id, item).await? {
            incoming.push(id);
        }
    }
    crate::provider_collections::replace_members(tx, collection_id, &incoming).await?;
    crate::collection_tags::reconcile(
        tx,
        crate::collection_tags::Provider::Plex,
        collection_id,
        &previous,
    )
    .await?;
    Ok(())
}

async fn find_plex_media_item(
    tx: &mut Transaction<'_, crate::db::Db>,
    media_source_id: i64,
    item: &PlexCollectionMediaItem,
) -> Result<Option<i64>> {
    let table = match item.kind {
        PlexCollectionMediaKind::Movie => "PlexMovie",
        PlexCollectionMediaKind::Show => "PlexShow",
        PlexCollectionMediaKind::Season => "PlexSeason",
        PlexCollectionMediaKind::Episode => "PlexEpisode",
    };
    let sql = format!(
        r#"SELECT p."Id" FROM "{table}" p
        JOIN "MediaItem" m ON m."Id" = p."Id"
        JOIN "LibraryPath" path ON path."Id" = m."LibraryPathId"
        JOIN "Library" l ON l."Id" = path."LibraryId"
        WHERE p."Key" = ? AND l."MediaSourceId" = ?"#
    );
    Ok(sqlx::query(crate::db::assert_sql_safe(&sql))
        .bind(&item.key)
        .bind(media_source_id)
        .fetch_optional(&mut **tx)
        .await
        .repository()?
        .map(|row| row.get("Id")))
}

async fn item_etag_matches(
    pool: &DbPool,
    table: &str,
    library_id: i64,
    key: &str,
    etag: Option<&str>,
) -> Result<bool> {
    let sql = format!(
        r#"SELECT "{table}"."Etag" AS "Etag", "MediaItem"."State" AS "State"
           FROM "{table}"
           INNER JOIN "MediaItem" ON "MediaItem"."Id" = "{table}"."Id"
           INNER JOIN "LibraryPath" ON "LibraryPath"."Id" = "MediaItem"."LibraryPathId"
           WHERE "{table}"."Key" = ? AND "LibraryPath"."LibraryId" = ?"#
    );
    let Some(row) = sqlx::query(crate::db::assert_sql_safe(&sql))
        .bind(key)
        .bind(library_id)
        .fetch_optional(pool)
        .await
        .repository()?
    else {
        return Ok(false);
    };

    let existing_etag: Option<String> = row.try_get("Etag").repository()?;
    let state: i64 = row.try_get("State").repository()?;
    Ok(
        crate::media_server::provider_item_state_allows_etag_skip(state)
            && existing_etag.as_deref() == etag,
    )
}

async fn ensure_simple_media_item(
    tx: &mut Transaction<'_, crate::db::Db>,
    base_table: &str,
    provider_table: &str,
    provider_key_column: &str,
    library_path_id: i64,
    key: &str,
    etag: Option<&String>,
) -> Result<i64> {
    let select_sql = format!(
        r#"SELECT "{provider_table}"."Id"
           FROM "{provider_table}"
           INNER JOIN "MediaItem" ON "MediaItem"."Id" = "{provider_table}"."Id"
           INNER JOIN "LibraryPath" existing_path
             ON existing_path."Id" = "MediaItem"."LibraryPathId"
           INNER JOIN "LibraryPath" current_path ON current_path."Id" = ?
           WHERE "{provider_table}"."{provider_key_column}" = ?
             AND existing_path."LibraryId" = current_path."LibraryId""#
    );
    if let Some(row) = sqlx::query(crate::db::assert_sql_safe(&select_sql))
        .bind(library_path_id)
        .bind(key)
        .fetch_optional(&mut **tx)
        .await
        .repository()?
    {
        let id = row.get("Id");
        let update_sql = format!(r#"UPDATE "{provider_table}" SET "Etag" = ? WHERE "Id" = ?"#);
        sqlx::query(crate::db::assert_sql_safe(&update_sql))
            .bind(etag)
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
    let id = insert_media_item(tx, library_path_id).await?;
    let base_sql = format!(r#"INSERT INTO "{base_table}" ("Id") VALUES (?)"#);
    sqlx::query(crate::db::assert_sql_safe(&base_sql))
        .bind(id)
        .execute(&mut **tx)
        .await
        .repository()?;
    let provider_sql = format!(
        r#"INSERT INTO "{provider_table}" ("Id", "{provider_key_column}", "Etag") VALUES (?, ?, ?)"#
    );
    sqlx::query(crate::db::assert_sql_safe(&provider_sql))
        .bind(id)
        .bind(key)
        .bind(etag)
        .execute(&mut **tx)
        .await
        .repository()?;
    Ok(id)
}

async fn insert_media_item(
    tx: &mut Transaction<'_, crate::db::Db>,
    library_path_id: i64,
) -> Result<i64> {
    let result = sqlx::query(r#"INSERT INTO "MediaItem" ("LibraryPathId", "State") VALUES (?, 0)"#)
        .bind(library_path_id)
        .execute(&mut **tx)
        .await
        .repository()?;
    Ok(crate::db::last_insert_id(result))
}

#[derive(Debug, Clone, Copy)]
enum VersionOwner {
    Movie(i64),
    Episode(i64),
    OtherVideo(i64),
}

async fn ensure_media_version(
    tx: &mut Transaction<'_, crate::db::Db>,
    owner: VersionOwner,
    item: &PlexMediaItem,
) -> Result<i64> {
    let (column, item_id) = match owner {
        VersionOwner::Movie(item_id) => ("MovieId", item_id),
        VersionOwner::Episode(item_id) => ("EpisodeId", item_id),
        VersionOwner::OtherVideo(item_id) => ("OtherVideoId", item_id),
    };
    let select_sql = format!(r#"SELECT "Id" FROM "MediaVersion" WHERE "{column}" = ?"#);
    if let Some(row) = sqlx::query(crate::db::assert_sql_safe(&select_sql))
        .bind(item_id)
        .fetch_optional(&mut **tx)
        .await
        .repository()?
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
    let result = sqlx::query(crate::db::assert_sql_safe(&insert_sql))
        .bind(encode_datetime(item.date_added))
        .bind(encode_datetime(item.date_updated))
        .bind(&item.display_aspect_ratio)
        .bind(encode_duration(item.duration))
        .bind(item.height)
        .bind(item_id)
        .bind(&item.r_frame_rate)
        .bind(&item.sample_aspect_ratio)
        .bind(item.video_scan_kind.db_value())
        .bind(item.width)
        .execute(&mut **tx)
        .await
        .repository()?;
    Ok(crate::db::last_insert_id(result))
}

async fn update_media_version(
    tx: &mut Transaction<'_, crate::db::Db>,
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
    .bind(item.video_scan_kind.db_value())
    .bind(item.width)
    .bind(id)
    .execute(&mut **tx)
    .await
    .repository()?;
    Ok(())
}

async fn ensure_media_file(
    tx: &mut Transaction<'_, crate::db::Db>,
    version_id: i64,
    item: &PlexMediaItem,
) -> Result<()> {
    sqlx::query(upsert_sql!(
        r#"INSERT INTO "MediaFile" ("MediaVersionId", "Path", "PathHash")
           VALUES (?, ?, ?)"#,
        &[&["PathHash"]],
    ))
    .bind(version_id)
    .bind(&item.path)
    .bind(sha256_hex(&item.path))
    .execute(&mut **tx)
    .await
    .repository()?;
    // The upsert updates an existing row in place rather than deleting and
    // re-inserting it, so there is no fresh rowid to read. Resolve the real id
    // by the natural key instead.
    let file_id: i64 = sqlx::query_scalar(r#"SELECT "Id" FROM "MediaFile" WHERE "PathHash" = ?"#)
        .bind(sha256_hex(&item.path))
        .fetch_one(&mut **tx)
        .await
        .repository()?;
    sqlx::query(r#"REPLACE INTO "PlexMediaFile" ("Id", "PlexId", "Key") VALUES (?, ?, ?)"#)
        .bind(file_id)
        .bind(item.plex_part_id)
        .bind(&item.part_key)
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
                "BitsPerRawSample", "AttachedPic", "FileName", "MimeType",
                "Rotation", "MasteringDisplay", "MaxCll", "MaxFall",
                "DolbyVisionProfile", "DolbyVisionBlCompatibilityId",
                "DolbyVisionBlPresent", "DolbyVisionElPresent", "DolbyVisionRpuPresent",
                "DolbyVisionMetadataState", "DolbyVisionMetadataHint"
            ) VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)"#,
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
        .bind(stream.rotation)
        .bind(&stream.mastering_display)
        .bind(stream.max_cll)
        .bind(stream.max_fall)
        .bind(stream.dolby_vision_profile)
        .bind(stream.dolby_vision_bl_compatibility_id)
        .bind(stream.dolby_vision_bl_present)
        .bind(stream.dolby_vision_el_present)
        .bind(stream.dolby_vision_rpu_present)
        .bind(stream.dolby_vision_metadata_state.as_i32())
        .bind(&stream.dolby_vision_metadata_hint)
        .execute(&mut **tx)
        .await
        .repository()?;
    }
    Ok(())
}

async fn plex_source_is_missing(pool: &DbPool, media_source_id: i64) -> Result<bool> {
    sqlx::query_scalar(
        r#"SELECT EXISTS(SELECT 1 FROM "PlexMediaSource"
           WHERE "Id" = ? AND "MissingSince" IS NOT NULL)"#,
    )
    .bind(media_source_id)
    .fetch_one(pool)
    .await
    .repository()
}

/// Refresh library metadata for an available Plex server without scanning its media.
///
/// # Errors
/// Returns a provider/storage error or rejects a missing server.
pub async fn refresh_plex_libraries(pool: &DbPool, media_source_id: i64) -> Result<()> {
    if plex_source_is_missing(pool, media_source_id).await? {
        return Err(RepositoryError::unavailable(
            "Plex libraries",
            "server is missing",
        ));
    }
    let config = load_active_plex_connection_config(pool, media_source_id).await?;
    let service = PlexService::new();
    let libraries = tokio::time::timeout(
        std::time::Duration::from_secs(10),
        service.list_libraries(&config),
    )
    .await
    .map_err(|_| RepositoryError::unavailable("Plex libraries", "request timed out"))??;
    sync_plex_libraries(pool, media_source_id, &libraries).await
}

/// Loads all configuration from the database and synchronizes one Plex library.
///
/// Dispatches to movie, show, or collection sync based on `MediaKind`.
/// Mirrors the orchestration in `scan_plex` in `ersatzrs-scanner`.
///
/// # Errors
///
/// Returns an error when config loading, the Plex API, or database writes fail.
pub async fn sync_library_by_id(pool: &DbPool, library_id: i64) -> Result<usize> {
    sync_library_by_id_with_options(pool, library_id, false).await
}

/// Loads all configuration from the database and synchronizes one Plex library.
///
/// `deep_scan` bypasses provider etag skip and refreshes existing rows even
/// when the upstream etag is unchanged.
///
/// # Errors
///
/// Returns an error when config loading, the Plex API, or database writes fail.
pub async fn sync_library_by_id_with_options(
    pool: &DbPool,
    library_id: i64,
    deep_scan: bool,
) -> Result<usize> {
    sync_library_by_id_with_progress(pool, library_id, deep_scan, None).await
}

/// Variant of [`sync_library_by_id_with_options`] that forwards a progress
/// sink to the underlying branch fn (`sync_plex_movie_library_with_progress`,
/// etc.). Used by the web admin to drive the libraries page ring.
///
/// # Errors
///
/// Returns an error when config loading, the Plex API, or database writes fail.
#[allow(clippy::too_many_lines)]
pub async fn sync_library_by_id_with_progress(
    pool: &DbPool,
    library_id: i64,
    deep_scan: bool,
    progress: Option<&dyn ersatzrs_domain::ScanProgressSink>,
) -> Result<usize> {
    let row = sqlx::query(
        r#"SELECT l."MediaKind", l."MediaSourceId"
           FROM "Library" l
           WHERE l."Id" = ?
           LIMIT 1"#,
    )
    .bind(library_id)
    .fetch_optional(pool)
    .await
    .repository()?
    .ok_or_else(|| RepositoryError::Decode {
        kind: "plex library",
        value: format!("library {library_id} not found"),
    })?;
    let kind: i64 = row.try_get("MediaKind").unwrap_or(0);
    let media_source_id: i64 = row.try_get("MediaSourceId").unwrap_or(0);

    if plex_source_is_missing(pool, media_source_id).await? {
        return Ok(0);
    }

    let conn_row = sqlx::query(
        r#"SELECT pc."Uri", pms."ClientIdentifier"
           FROM "PlexConnection" pc
           JOIN "PlexMediaSource" pms ON pms."Id" = pc."PlexMediaSourceId"
           WHERE pc."PlexMediaSourceId" = ? AND pc."IsActive" = 1
           ORDER BY pc."Id"
           LIMIT 1"#,
    )
    .bind(media_source_id)
    .fetch_optional(pool)
    .await
    .repository()?
    .ok_or_else(|| RepositoryError::Decode {
        kind: "plex connection",
        value: format!("no active connection for source {media_source_id}"),
    })?;
    let uri: String = conn_row.try_get("Uri").unwrap_or_default();
    let client_identifier: String = conn_row.try_get("ClientIdentifier").unwrap_or_default();

    let token = load_plex_token(pool, &client_identifier).await?;
    let config = PlexConnectionConfig { uri, token };

    let library_key: String =
        sqlx::query_scalar(r#"SELECT "Key" FROM "PlexLibrary" WHERE "Id" = ?"#)
            .bind(library_id)
            .fetch_optional(pool)
            .await
            .repository()?
            .and_then(|v: Option<String>| v)
            .ok_or_else(|| RepositoryError::Decode {
                kind: "plex library",
                value: format!("PlexLibrary row {library_id} not found"),
            })?;

    let service = PlexService::new();
    match kind {
        2 => {
            let synced = sync_plex_show_library_with_progress(
                pool,
                &service,
                &config,
                media_source_id,
                library_id,
                &library_key,
                deep_scan,
                progress,
            )
            .await?;
            sync_plex_networks(pool, &service, &config, library_id).await?;
            Ok(synced)
        }
        1 => {
            sync_plex_movie_library_with_progress(
                pool,
                &service,
                &config,
                media_source_id,
                library_id,
                &library_key,
                deep_scan,
                progress,
            )
            .await
        }
        4 => {
            sync_plex_other_video_library_with_progress(
                pool,
                &service,
                &config,
                media_source_id,
                library_id,
                &library_key,
                deep_scan,
                progress,
            )
            .await
        }
        _ => {
            sync_plex_collection_library_with_progress(
                pool,
                &service,
                &config,
                media_source_id,
                &library_key,
                progress,
            )
            .await
        }
    }
}

/// Synchronize one Plex show by the local `Show.Id`.
///
/// Mirrors ErsatzTV's `SynchronizePlexShowByIdHandler`: resolve the active
/// Plex connection/token, map the local show id to the provider key/title via
/// the Plex show row, fetch `/library/metadata/{ratingKey}`, then scan that
/// show's seasons and episodes without library cleanup.
/// C# source:
/// `$ERSATZTV_SRC/ErsatzTV.Scanner/Application/Plex/Commands/SynchronizePlexShowByIdHandler.cs`
/// and
/// `$ERSATZTV_SRC/ErsatzTV.Infrastructure/Data/Repositories/PlexTelevisionRepository.cs`.
///
/// # Errors
///
/// Returns an error when config loading, show lookup, Plex requests, or
/// database writes fail.
pub async fn sync_plex_show_by_id(
    pool: &DbPool,
    library_id: i64,
    show_id: i64,
    deep_scan: bool,
    progress: Option<&dyn ersatzrs_domain::ScanProgressSink>,
) -> Result<usize> {
    let target = load_target_plex_show_scan(pool, library_id, show_id).await?;
    if plex_source_is_missing(pool, target.media_source_id).await? {
        return Ok(0);
    }
    if !target.should_sync_items {
        tracing::warn!(
            library_id,
            show_id,
            show_title = %target.show_title,
            "Plex show scan skipped because library item sync is disabled"
        );
        return Ok(0);
    }

    let config = load_active_plex_connection_config(pool, target.media_source_id).await?;
    let service = PlexService::new();

    if let Some(sink) = progress {
        sink.report(0.0);
    }

    let Some(show) = service.fetch_single_show(&config, &target.show_key).await? else {
        tracing::warn!(
            library_id,
            show_id,
            show_key = %target.show_key,
            show_title = %target.show_title,
            "No Plex show found during targeted scan"
        );
        if let Some(sink) = progress {
            sink.report(1.0);
        }
        return Ok(0);
    };

    tracing::info!(
        library_id,
        show_id,
        show_key = %show.key,
        show_title = %target.show_title,
        deep_scan,
        "Starting targeted Plex show scan"
    );

    // Targeted single-show scan does no library-level cleanup (matching
    // ErsatzTV), so the present-id collectors are discarded here.
    let synced = sync_one_plex_show(
        pool,
        &service,
        &config,
        target.media_source_id,
        library_id,
        &target.library_key,
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

struct TargetPlexShowScan {
    media_source_id: i64,
    library_key: String,
    should_sync_items: bool,
    show_key: String,
    show_title: String,
}

async fn load_target_plex_show_scan(
    pool: &DbPool,
    library_id: i64,
    show_id: i64,
) -> Result<TargetPlexShowScan> {
    let row = sqlx::query(
        r#"SELECT l."MediaKind",
                  l."MediaSourceId",
                  pl."Key" AS "LibraryKey",
                  pl."ShouldSyncItems",
                  psh."Key" AS "ShowKey",
                  COALESCE(
                      (SELECT sm."Title"
                       FROM "ShowMetadata" sm
                       WHERE sm."ShowId" = psh."Id"
                       ORDER BY sm."Id"
                       LIMIT 1),
                      'Unknown Show'
                  ) AS "ShowTitle"
           FROM "Library" l
           JOIN "PlexLibrary" pl ON pl."Id" = l."Id"
           JOIN "PlexShow" psh ON psh."Id" = ?
           JOIN "MediaItem" mi ON mi."Id" = psh."Id"
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
        kind: "plex show",
        value: format!("show {show_id} does not exist in library {library_id}"),
    })?;

    let kind: LibraryMediaKind = decode_enum_column("plex library media kind", &row, "MediaKind")?;
    if kind != LibraryMediaKind::Shows {
        return Err(RepositoryError::Decode {
            kind: "plex library",
            value: format!("library {library_id} is not a TV show library"),
        });
    }

    let should_sync_items: i64 = row.try_get("ShouldSyncItems").unwrap_or(1);
    let show_title: String = row
        .try_get("ShowTitle")
        .unwrap_or_else(|_| "Unknown Show".to_owned());
    let media_source_id: i64 = row.try_get("MediaSourceId").unwrap_or(0);
    let library_key: String = row.try_get("LibraryKey").unwrap_or_default();
    let show_key: String = row.try_get("ShowKey").unwrap_or_default();

    Ok(TargetPlexShowScan {
        media_source_id,
        library_key,
        should_sync_items: should_sync_items != 0,
        show_key,
        show_title,
    })
}

pub(crate) async fn load_active_plex_connection_config(
    pool: &DbPool,
    media_source_id: i64,
) -> Result<PlexConnectionConfig> {
    let conn_row = sqlx::query(
        r#"SELECT pc."Uri", pms."ClientIdentifier"
           FROM "PlexConnection" pc
           JOIN "PlexMediaSource" pms ON pms."Id" = pc."PlexMediaSourceId"
           WHERE pc."PlexMediaSourceId" = ? AND pc."IsActive" = 1
           ORDER BY pc."Id"
           LIMIT 1"#,
    )
    .bind(media_source_id)
    .fetch_optional(pool)
    .await
    .repository()?
    .ok_or_else(|| RepositoryError::Decode {
        kind: "plex connection",
        value: format!("no active connection for source {media_source_id}"),
    })?;
    let uri: String = conn_row.try_get("Uri").unwrap_or_default();
    let client_identifier: String = conn_row.try_get("ClientIdentifier").unwrap_or_default();
    let token = load_plex_token(pool, &client_identifier).await?;
    Ok(PlexConnectionConfig { uri, token })
}

/// Sync Plex collections for every library on `media_source_id` that has
/// `ShouldSyncItems = 1`. Mirrors ETV's `SynchronizePlexCollections` command
/// dispatched from the External Collections row on the libraries page.
///
/// `_deep_scan` matches the UI button contract;
/// `sync_plex_collection_library` does not currently distinguish deep from
/// shallow scans, so the flag is reserved for future use.
///
/// # Errors
///
/// Returns an error when config loading, the Plex API, or database writes fail.
pub async fn sync_collections_for_media_source(
    pool: &DbPool,
    media_source_id: i64,
    _deep_scan: bool,
) -> Result<usize> {
    if plex_source_is_missing(pool, media_source_id).await? {
        return Ok(0);
    }
    let library_ids: Vec<i64> = sqlx::query_scalar(
        r#"SELECT l."Id"
           FROM "Library" l
           JOIN "PlexLibrary" pl ON pl."Id" = l."Id"
           WHERE l."MediaSourceId" = ? AND pl."ShouldSyncItems" = 1
           ORDER BY l."Id""#,
    )
    .bind(media_source_id)
    .fetch_all(pool)
    .await
    .repository()?;

    if library_ids.is_empty() {
        return Ok(0);
    }

    let conn_row = sqlx::query(
        r#"SELECT pc."Uri", pms."ClientIdentifier"
           FROM "PlexConnection" pc
           JOIN "PlexMediaSource" pms ON pms."Id" = pc."PlexMediaSourceId"
           WHERE pc."PlexMediaSourceId" = ? AND pc."IsActive" = 1
           ORDER BY pc."Id"
           LIMIT 1"#,
    )
    .bind(media_source_id)
    .fetch_optional(pool)
    .await
    .repository()?
    .ok_or_else(|| RepositoryError::Decode {
        kind: "plex connection",
        value: format!("no active connection for source {media_source_id}"),
    })?;
    let uri: String = conn_row.try_get("Uri").unwrap_or_default();
    let client_identifier: String = conn_row.try_get("ClientIdentifier").unwrap_or_default();
    let token = load_plex_token(pool, &client_identifier).await?;
    let config = PlexConnectionConfig { uri, token };
    let service = PlexService::new();

    let mut total = 0;
    for library_id in library_ids {
        let library_key: Option<String> =
            sqlx::query_scalar(r#"SELECT "Key" FROM "PlexLibrary" WHERE "Id" = ?"#)
                .bind(library_id)
                .fetch_optional(pool)
                .await
                .repository()?
                .and_then(|v: Option<String>| v);
        let Some(library_key) = library_key else {
            continue;
        };
        total +=
            sync_plex_collection_library(pool, &service, &config, media_source_id, &library_key)
                .await?;
    }
    Ok(total)
}

pub(crate) async fn load_plex_token(pool: &DbPool, client_identifier: &str) -> Result<String> {
    for key in &[
        format!("{PLEX_SERVER_AUTH_TOKEN_PREFIX}{client_identifier}"),
        PLEX_CURRENT_SERVER_AUTH_TOKEN_KEY.to_owned(),
    ] {
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
        kind: "plex config",
        value: "auth token not found".into(),
    })
}

#[allow(clippy::needless_pass_by_value)]
fn provider_error(error: reqwest::Error) -> RepositoryError {
    if error.status() == Some(reqwest::StatusCode::UNAUTHORIZED) {
        return RepositoryError::unavailable("Plex authorization", "Plex rejected the credentials");
    }
    RepositoryError::Decode {
        kind: "plex http",
        value: error.without_url().to_string(),
    }
}

fn unix_time(value: Option<i64>) -> DateTime<Utc> {
    Utc.timestamp_opt(value.unwrap_or_default(), 0)
        .single()
        .unwrap_or_else(Utc::now)
}

fn sha256_hex(value: &str) -> String {
    let digest = Sha256::digest(value.as_bytes());
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
    #[serde(default)]
    agent: String,
    #[serde(default)]
    scanner: String,
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
    #[serde(rename = "totalSize")]
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
    #[serde(rename = "totalSize")]
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
    original_title: Option<String>,
    year: Option<i32>,
    originally_available_at: Option<String>,
    content_rating: Option<String>,
    summary: Option<String>,
    tagline: Option<String>,
    studio: Option<String>,
    thumb: Option<String>,
    art: Option<String>,
    #[serde(default, rename = "Genre")]
    genre: Vec<PlexTagResponse>,
    #[serde(default, rename = "Label")]
    music_label: Vec<PlexTagResponse>,
    #[serde(default, rename = "Guid")]
    guid: Vec<PlexGuidResponse>,
    #[serde(default, rename = "Role")]
    role: Vec<PlexPersonResponse>,
    #[serde(default, rename = "Director")]
    director: Vec<PlexPersonResponse>,
    #[serde(default, rename = "Writer")]
    writer: Vec<PlexPersonResponse>,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
struct PlexTagResponse {
    tag: Option<String>,
    id: Option<PlexTagId>,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(untagged)]
enum PlexTagId {
    Text(String),
    Number(i64),
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct PlexGuidResponse {
    id: Option<String>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct PlexPersonResponse {
    tag: Option<String>,
    role: Option<String>,
    thumb: Option<String>,
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
    profile: Option<String>,
    channels: Option<i32>,
    width: Option<i32>,
    height: Option<i32>,
    pixel_aspect_ratio: Option<String>,
    scan_type: Option<String>,
    frame_rate: Option<String>,
    bit_depth: Option<i32>,
    color_range: Option<String>,
    color_space: Option<String>,
    color_trc: Option<String>,
    color_primaries: Option<String>,
    video_range_type: Option<String>,
    #[serde(rename = "DOVIPresent")]
    dovi_present: Option<i32>,
    #[serde(rename = "DOVIProfile")]
    dovi_profile: Option<i32>,
    #[serde(rename = "DOVIBLCompatID")]
    dovi_bl_compat_id: Option<i32>,
    #[serde(rename = "DOVIBLPresent")]
    dovi_bl_present: Option<i32>,
    #[serde(rename = "DOVIELPresent")]
    dovi_el_present: Option<i32>,
    #[serde(rename = "DOVIRPUPresent")]
    dovi_rpu_present: Option<i32>,
}

#[cfg(test)]
mod stream_projection_tests {
    use ersatzrs_domain::DolbyVisionMetadataState;

    use super::{project_stream, PlexStreamResponse};

    #[test]
    fn plex_positive_but_partial_dolby_fixture_is_rejected_safely() {
        let response: PlexStreamResponse = serde_json::from_str(
            r#"{
                "streamType":1, "codec":"hevc", "profile":"main 10",
                "DOVIPresent":1, "DOVIProfile":5
            }"#,
        )
        .expect("deserialize Plex stream");

        let stream = project_stream(response).expect("project video stream");
        assert_eq!(
            stream.dolby_vision_metadata_state,
            DolbyVisionMetadataState::Incomplete
        );
        assert_eq!(stream.dolby_vision_profile, Some(5));
        assert!(stream
            .dolby_vision_metadata_hint
            .as_deref()
            .is_some_and(|hint| hint.contains("Plex")));
    }
}
