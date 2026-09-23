//! Provider binding identity for subtitle artifacts; credentials never enter it.

use ersatzrs_core::repositories::{RemoteMediaSourceKind, RepositoryError, Result};
use serde_json::{json, Value};
use sqlx::Row as _;

use crate::db::DbPool;

use super::{protocol::Source, schedule::ScheduledMedia, source::server_url};

/// Return the configured server and provider item identity behind a selected file.
///
/// Emby/Jellyfin addresses can change without updating scanned media revisions.
/// Plex endpoint failover instead verifies a stable machine identifier; changing
/// an active URI on that same server must not invalidate a completed artifact.
/// A resolved local file needs only its owning provider identity; its filesystem
/// revision is bound separately and remote connection metadata is not required.
/// This value is for hashing only: do not persist or log its source addresses.
///
/// # Errors
/// Returns a redacted error for missing, ambiguous or changed source bindings.
pub(crate) async fn identity(
    pool: &DbPool,
    media: &ScheduledMedia,
    source: &Source,
) -> Result<Value> {
    let local = match source {
        Source::Local { path, .. } if !path.trim().is_empty() => true,
        Source::Http { .. } => false,
        _ => return Err(unavailable()),
    };
    let row = sqlx::query(
        r#"SELECT l."MediaSourceId",
            EXISTS(SELECT 1 FROM "LocalLibrary" WHERE "Id"=l."Id") AS "Local",
            EXISTS(SELECT 1 FROM "PlexLibrary" WHERE "Id"=l."Id") AS "Plex",
            EXISTS(SELECT 1 FROM "JellyfinLibrary" WHERE "Id"=l."Id") AS "Jellyfin",
            EXISTS(SELECT 1 FROM "EmbyLibrary" WHERE "Id"=l."Id") AS "Emby"
        FROM "MediaItem" m JOIN "LibraryPath" p ON p."Id"=m."LibraryPathId"
        JOIN "Library" l ON l."Id"=p."LibraryId"
        JOIN "MediaVersion" v ON v."Id"=? AND
            (v."MovieId"=m."Id" OR v."EpisodeId"=m."Id" OR v."MusicVideoId"=m."Id"
             OR v."OtherVideoId"=m."Id" OR v."SongId"=m."Id" OR v."ImageId"=m."Id")
        JOIN "MediaFile" f ON f."MediaVersionId"=v."Id" AND f."Id"=? AND f."Path"=?
        WHERE m."Id"=?"#,
    )
    .bind(media.media_version_id)
    .bind(media.media_file_id)
    .bind(&media.path)
    .bind(media.item.media_item_id)
    .fetch_optional(pool)
    .await
    .map_err(|_| unavailable())?
    .ok_or_else(unavailable)?;
    let source_id: i64 = row.try_get("MediaSourceId").map_err(|_| unavailable())?;
    let flags = ["Local", "Plex", "Jellyfin", "Emby"].map(|column| row.try_get::<bool, _>(column));
    let flags: Vec<bool> = flags
        .into_iter()
        .collect::<std::result::Result<_, _>>()
        .map_err(|_| unavailable())?;
    if flags.iter().filter(|flag| **flag).count() != 1 {
        return Err(unavailable());
    }
    if local {
        let kind = ["local", "plex", "jellyfin", "emby"]
            .into_iter()
            .zip(&flags)
            .find_map(|(kind, selected)| selected.then_some(kind))
            .ok_or_else(unavailable)?;
        return Ok(json!({ "kind": kind, "source_id": source_id }));
    }
    if flags[0] {
        return Err(unavailable());
    }
    if flags[1] {
        return plex_identity(pool, source_id, media.media_file_id).await;
    }
    let kind = if flags[2] {
        RemoteMediaSourceKind::Jellyfin
    } else {
        RemoteMediaSourceKind::Emby
    };
    remote_identity(pool, source_id, media.item.media_item_id, kind).await
}

async fn plex_identity(pool: &DbPool, source_id: i64, file_id: i64) -> Result<Value> {
    let row = sqlx::query(
        r#"SELECT s."ClientIdentifier", f."Key" FROM "PlexMediaSource" s
        JOIN "PlexMediaFile" f ON f."Id"=? WHERE s."Id"=?"#,
    )
    .bind(file_id)
    .bind(source_id)
    .fetch_optional(pool)
    .await
    .map_err(|_| unavailable())?
    .ok_or_else(unavailable)?;
    let machine: Option<String> = row.try_get("ClientIdentifier").map_err(|_| unavailable())?;
    let key: Option<String> = row.try_get("Key").map_err(|_| unavailable())?;
    let machine = nonempty(machine)?;
    let key = nonempty(key)?;
    Ok(json!({ "kind": "plex", "source_id": source_id,
        "machine_identifier": machine, "file_key": key }))
}

async fn remote_identity(
    pool: &DbPool,
    source_id: i64,
    item_id: i64,
    kind: RemoteMediaSourceKind,
) -> Result<Value> {
    let (address_sql, item_sql, name) = match kind {
        RemoteMediaSourceKind::Jellyfin => (
            r#"SELECT "Address" FROM "JellyfinConnection" WHERE "JellyfinMediaSourceId"=? ORDER BY "Id" LIMIT 1"#,
            r#"SELECT "ItemId" FROM "JellyfinMovie" WHERE "Id"=? UNION ALL SELECT "ItemId" FROM "JellyfinEpisode" WHERE "Id"=?"#,
            "jellyfin",
        ),
        RemoteMediaSourceKind::Emby => (
            r#"SELECT "Address" FROM "EmbyConnection" WHERE "EmbyMediaSourceId"=? ORDER BY "Id" LIMIT 1"#,
            r#"SELECT "ItemId" FROM "EmbyMovie" WHERE "Id"=? UNION ALL SELECT "ItemId" FROM "EmbyEpisode" WHERE "Id"=?"#,
            "emby",
        ),
        RemoteMediaSourceKind::Plex => return Err(unavailable()),
    };
    let address: Option<String> = sqlx::query_scalar(address_sql)
        .bind(source_id)
        .fetch_optional(pool)
        .await
        .map_err(|_| unavailable())?
        .flatten();
    let address = nonempty(address)?;
    server_url(&address).map_err(|_| unavailable())?;
    let ids: Vec<Option<String>> = sqlx::query_scalar(item_sql)
        .bind(item_id)
        .bind(item_id)
        .fetch_all(pool)
        .await
        .map_err(|_| unavailable())?;
    let [Some(item)] = ids.as_slice() else {
        return Err(unavailable());
    };
    if item.trim().is_empty() || matches!(item.as_str(), "." | "..") {
        return Err(unavailable());
    }
    Ok(json!({ "kind": name, "source_id": source_id, "address": address, "item_id": item }))
}

fn nonempty(value: Option<String>) -> Result<String> {
    value
        .filter(|value| !value.trim().is_empty())
        .ok_or_else(unavailable)
}

fn unavailable() -> RepositoryError {
    RepositoryError::unavailable("Next provider identity", "binding unavailable")
}

#[cfg(test)]
#[path = "dynamic_provider_identity_tests.rs"]
mod tests;
