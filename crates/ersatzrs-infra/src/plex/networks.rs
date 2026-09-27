//! Plex network discovery and library-scoped tag reconciliation.
use std::collections::{BTreeMap, BTreeSet};

use serde::Deserialize;
use sqlx::Row;

use super::{PlexConnectionConfig, PlexService, PLEX_PAGE_SIZE};
use crate::db::DbPool;
use crate::repositories::SqlxResultExt as _;
use ersatzrs_core::repositories::{RepositoryError, Result};

#[derive(Deserialize)]
struct Response {
    #[serde(rename = "MediaContainer")]
    container: Container,
}

#[derive(Deserialize)]
struct Container {
    #[serde(rename = "totalSize")]
    total: i32,
    #[serde(rename = "Directory", default)]
    networks: Vec<Network>,
    #[serde(rename = "Metadata", default)]
    shows: Vec<Show>,
}

#[derive(Deserialize)]
struct Network {
    tag: String,
    filter: String,
    #[serde(rename = "tagType")]
    tag_type: i32,
}

#[derive(Deserialize)]
struct Show {
    key: Option<String>,
    #[serde(rename = "ratingKey")]
    rating_key: Option<String>,
}

fn invalid(message: &'static str) -> RepositoryError {
    RepositoryError::Decode {
        kind: "plex networks",
        value: message.to_owned(),
    }
}

async fn page(
    service: &PlexService,
    config: &PlexConnectionConfig,
    url: String,
    start: i32,
) -> Result<Response> {
    tokio::time::timeout(
        std::time::Duration::from_secs(30),
        service.get_page(config, url, start, PLEX_PAGE_SIZE),
    )
    .await
    .map_err(|_| RepositoryError::unavailable("Plex networks", "request timed out"))?
}

// Require consistent totals and complete raw pages before publishing removals.
fn next_page(
    start: i32,
    count: usize,
    total: i32,
    expected: &mut Option<i32>,
) -> Result<Option<i32>> {
    if total < 0 || expected.is_some_and(|old| old != total) {
        return Err(invalid("inconsistent page total"));
    }
    *expected = Some(total);
    let count = i32::try_from(count).map_err(|_| invalid("page count overflow"))?;
    let remaining = total
        .checked_sub(start)
        .ok_or_else(|| invalid("invalid page offset"))?;
    if count != remaining.min(PLEX_PAGE_SIZE) || remaining < 0 {
        return Err(invalid("incomplete page"));
    }
    let next = start
        .checked_add(count)
        .ok_or_else(|| invalid("page offset overflow"))?;
    Ok((next < total).then_some(next))
}

/// Refresh Plex network tags for one existing TV library.
/// Returns the number of shows whose network membership changed. Failed discovery
/// leaves tags and the last successful scan timestamp unchanged.
///
/// # Errors
/// Returns an error for an unknown library, failed request, incomplete enumeration
/// or database failure. Provider text and credentials are excluded from diagnostics.
pub async fn sync_plex_networks(
    pool: &DbPool,
    service: &PlexService,
    config: &PlexConnectionConfig,
    library_id: i64,
) -> Result<usize> {
    let key: String = sqlx::query_scalar(
        r#"SELECT p."Key" FROM "PlexLibrary" p JOIN "Library" l ON l."Id" = p."Id"
           WHERE p."Id" = ? AND l."MediaKind" = 2"#,
    )
    .bind(library_id)
    .fetch_optional(pool)
    .await
    .repository()?
    .ok_or(RepositoryError::NotFound {
        kind: "plex TV library",
        id: library_id,
    })?;
    let networks = fetch_networks(service, config).await?;
    let mut incoming = BTreeMap::new();
    for network in networks {
        if network.tag_type != 319 || network.tag.trim().is_empty() {
            return Err(invalid("invalid network tag"));
        }
        let shows = fetch_shows(service, config, &key, &network.filter).await?;
        incoming
            .entry(network.tag)
            .or_insert_with(BTreeSet::new)
            .extend(shows);
    }
    reconcile(pool, library_id, &incoming).await
}

async fn fetch_networks(
    service: &PlexService,
    config: &PlexConnectionConfig,
) -> Result<Vec<Network>> {
    let url = format!("{}/library/tags?type=319", config.uri.trim_end_matches('/'));
    let mut start = 0;
    let mut total = None;
    let mut networks = Vec::new();
    loop {
        let page = page(service, config, url.clone(), start).await?;
        let next = next_page(
            start,
            page.container.networks.len(),
            page.container.total,
            &mut total,
        )?;
        networks.extend(page.container.networks);
        match next {
            Some(next) => start = next,
            None => return Ok(networks),
        }
    }
}

async fn fetch_shows(
    service: &PlexService,
    config: &PlexConnectionConfig,
    key: &str,
    filter: &str,
) -> Result<BTreeSet<String>> {
    // Match GetTagShowContents: decode the network filter's query value and
    // re-encode it as a query parameter; never treat provider filters as URLs.
    let mut parsed =
        reqwest::Url::parse("http://localhost/").map_err(|_| invalid("invalid filter base"))?;
    parsed.set_query(Some(filter.trim_start_matches('?')));
    let value = parsed
        .query_pairs()
        .last()
        .map(|(_, value)| value.into_owned())
        .filter(|value| !value.is_empty())
        .ok_or_else(|| invalid("empty network filter"))?;
    let mut url = reqwest::Url::parse(&config.uri).map_err(|_| invalid("invalid server URI"))?;
    url.path_segments_mut()
        .map_err(|()| invalid("invalid server URI"))?
        .pop_if_empty()
        .extend(["library", "sections", key, "all"]);
    url.query_pairs_mut().clear().append_pair("network", &value);
    let mut start = 0;
    let mut total = None;
    let mut shows = BTreeSet::new();
    loop {
        let page = page(service, config, url.to_string(), start).await?;
        let next = next_page(
            start,
            page.container.shows.len(),
            page.container.total,
            &mut total,
        )?;
        for show in page.container.shows {
            let key = show
                .key
                .or(show.rating_key)
                .ok_or_else(|| invalid("missing show key"))?;
            let key = super::plex_rating_key(&key).ok_or_else(|| invalid("empty show key"))?;
            shows.insert(key.to_owned());
        }
        match next {
            Some(next) => start = next,
            None => return Ok(shows),
        }
    }
}

async fn reconcile(
    pool: &DbPool,
    library_id: i64,
    incoming: &BTreeMap<String, BTreeSet<String>>,
) -> Result<usize> {
    let dependencies = crate::repositories::SqliteSchedulingContentRepository::new(pool.clone())
        .classic_media_dependencies()
        .await?;
    let mut tx = crate::db::begin_write(pool).await.repository()?;
    // Like PlexTelevisionRepository.RemoveAllTags, each returned network owns
    // only its name/type pair. Other network names remain independent.
    let names = serde_json::json!(incoming.keys().collect::<Vec<_>>()).to_string();
    let shows = sqlx::query(
        r#"SELECT p."Key", sm."Id", sm."ShowId" FROM "PlexShow" p
           JOIN "MediaItem" m ON m."Id" = p."Id"
           JOIN "LibraryPath" path ON path."Id" = m."LibraryPathId"
           JOIN "ShowMetadata" sm ON sm."ShowId" = p."Id"
           WHERE path."LibraryId" = ?"#,
    )
    .bind(library_id)
    .fetch_all(&mut *tx)
    .await
    .repository()?;
    let mut changed = BTreeSet::new();
    for show in shows {
        let key: Option<String> = show.try_get("Key").repository()?;
        let Some(key) = key.as_deref().and_then(super::plex_rating_key) else {
            continue;
        };
        let metadata: i64 = show.try_get("Id").repository()?;
        let show_id: i64 = show.try_get("ShowId").repository()?;
        let mut wanted: BTreeSet<&String> = incoming
            .iter()
            .filter(|(_, keys)| keys.contains(key))
            .map(|(name, _)| name)
            .collect();
        let existing = sqlx::query(r#"SELECT "Id", "Name" FROM "Tag" WHERE "ShowMetadataId" = ? AND "ExternalTypeId" = '319'
            AND "Name" IN (SELECT value FROM json_each(?)) ORDER BY "Id""#)
            .bind(metadata).bind(&names).fetch_all(&mut *tx).await.repository()?;
        for tag in existing {
            let name: Option<String> = tag.try_get("Name").repository()?;
            if !name.as_ref().is_some_and(|name| wanted.remove(name)) {
                sqlx::query(r#"DELETE FROM "Tag" WHERE "Id" = ?"#)
                    .bind(tag.try_get::<i64, _>("Id").repository()?)
                    .execute(&mut *tx)
                    .await
                    .repository()?;
                changed.insert(show_id);
            }
        }
        for name in wanted {
            sqlx::query(r#"INSERT INTO "Tag" ("Name", "ShowMetadataId", "ExternalTypeId") VALUES (?, ?, '319')"#)
                .bind(name).bind(metadata).execute(&mut *tx).await.repository()?;
            changed.insert(show_id);
        }
    }
    sqlx::query(r#"UPDATE "PlexLibrary" SET "LastNetworksScan" = ? WHERE "Id" = ?"#)
        .bind(chrono::Utc::now().to_rfc3339())
        .bind(library_id)
        .execute(&mut *tx)
        .await
        .repository()?;
    let changed_count = changed.len();
    crate::provider_collection_refresh::persist_changed_media(
        &mut tx,
        changed,
        &dependencies,
        ersatzrs_domain::ScanPlayoutRefreshReason::ProviderNetworkChanged,
    )
    .await?;
    tx.commit().await.repository()?;
    Ok(changed_count)
}
