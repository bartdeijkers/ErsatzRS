//! Media-server inputs from upstream `SyncNextPlayoutHandler::SourceForItem`.
//!
//! Resolve only the owning server's paths. Published remote inputs use the host's
//! internal media route; provider connections and credentials are resolved when
//! that route opens the media. Errors never expose connection values.

use ersatzrs_core::{
    path_replacement::{replace_path, PathPlatform, PathReplacement},
    repositories::{MediaSourceRepository, RemoteMediaSourceKind, RepositoryError, Result},
};
use sqlx::Row;
use url::Url;

use super::{probe_hint, protocol::Source, schedule::ScheduledMedia};
use crate::{
    db::DbPool,
    repositories::{SqliteMediaSourceRepository, SqlxResultExt as _},
};

/// Prefer accessible local media, then resolve a media-server HTTP input.
/// Remote Stream/add-on lifetimes are a separate adapter and remain unsupported.
/// No provider HTTP work occurs while the caller holds the schedule's build locks.
#[cfg(test)]
pub(super) async fn resolve(
    pool: &DbPool,
    media: &ScheduledMedia,
    replacements: &[PathReplacement],
) -> Result<Source> {
    resolve_published(pool, media, replacements, None).await
}

pub(super) async fn resolve_published(
    pool: &DbPool,
    media: &ScheduledMedia,
    local_replacements: &[PathReplacement],
    relay: Option<&super::media::MediaRelay>,
) -> Result<Source> {
    if media.remote_stream_id.is_some() {
        return Err(unavailable("Remote Stream projection is not integrated"));
    }
    let row = sqlx::query(
        r#"SELECT l."MediaSourceId",
        EXISTS(SELECT 1 FROM "LocalLibrary" WHERE "Id" = l."Id") AS "Local",
        EXISTS(SELECT 1 FROM "PlexLibrary" WHERE "Id" = l."Id") AS "Plex",
        EXISTS(SELECT 1 FROM "JellyfinLibrary" WHERE "Id" = l."Id") AS "Jellyfin",
        EXISTS(SELECT 1 FROM "EmbyLibrary" WHERE "Id" = l."Id") AS "Emby"
        FROM "MediaItem" m JOIN "LibraryPath" p ON p."Id" = m."LibraryPathId"
        JOIN "Library" l ON l."Id" = p."LibraryId" WHERE m."Id" = ?"#,
    )
    .bind(media.item.media_item_id)
    .fetch_optional(pool)
    .await
    .repository()?
    .ok_or_else(|| unavailable("media library is unavailable"))?;
    let local: bool = row.try_get("Local").repository()?;
    let plex: bool = row.try_get("Plex").repository()?;
    let jellyfin: bool = row.try_get("Jellyfin").repository()?;
    let emby: bool = row.try_get("Emby").repository()?;
    if [local, plex, jellyfin, emby]
        .into_iter()
        .filter(|v| *v)
        .count()
        != 1
    {
        return Err(unavailable(
            "media library kind is ambiguous or unavailable",
        ));
    }
    if local {
        return with_persisted_hint(
            pool,
            media,
            local_source(replace_path(local_replacements, &media.path, platform())),
        )
        .await;
    }
    let kind = if plex {
        RemoteMediaSourceKind::Plex
    } else if jellyfin {
        RemoteMediaSourceKind::Jellyfin
    } else {
        RemoteMediaSourceKind::Emby
    };
    let source_id: i64 = row.try_get("MediaSourceId").repository()?;
    let replacements = SqliteMediaSourceRepository(pool.clone())
        .list_path_replacements_admin(kind, source_id)
        .await?
        .into_iter()
        .map(|r| PathReplacement {
            source_platform: if r.remote_path.contains('\\') || r.remote_path.contains(':') {
                PathPlatform::Windows
            } else {
                PathPlatform::Unix
            },
            source_path: r.remote_path,
            replacement_path: r.local_path,
        })
        .collect::<Vec<_>>();
    let path = replace_path(&replacements, &media.path, platform());
    if tokio::fs::metadata(&path).await.is_ok_and(|m| m.is_file()) {
        return with_persisted_hint(pool, media, local_source(path)).await;
    }
    if let Some(relay) = relay {
        return with_persisted_hint(pool, media, relay.source(media)?).await;
    }
    resolve_http(pool, media, source_id, kind, None).await
}

pub(super) async fn resolve_http(
    pool: &DbPool,
    media: &ScheduledMedia,
    source_id: i64,
    kind: RemoteMediaSourceKind,
    live: Option<(&super::connections::PlexConnections, tokio::time::Instant)>,
) -> Result<Source> {
    let (uri, header) = match kind {
        RemoteMediaSourceKind::Plex => {
            let config = if let Some((connections, deadline)) = live {
                let transport = connections
                    .resolve(pool, source_id, deadline)
                    .await
                    .map_err(|_| unavailable("Plex connection is unavailable"))?;
                crate::plex::PlexConnectionConfig {
                    uri: transport.address,
                    token: transport.token,
                }
            } else {
                crate::plex::load_active_plex_connection_config(pool, source_id)
                    .await
                    .map_err(|_| unavailable("Plex connection is unavailable"))?
            };
            let key: Option<String> = sqlx::query_scalar(
                r#"SELECT p."Key"
                FROM "PlexMediaFile" p JOIN "MediaFile" f ON f."Id" = p."Id"
                WHERE f."Id" = ? AND f."MediaVersionId" = ? AND f."Path" = ?"#,
            )
            .bind(media.media_file_id)
            .bind(media.media_version_id)
            .bind(&media.path)
            .fetch_optional(pool)
            .await
            .repository()?
            .flatten();
            let key = key
                .filter(|s| !s.trim().is_empty())
                .ok_or_else(|| unavailable("Plex media file key is unavailable"))?;
            let base = server_url(&config.uri)?;
            let uri = base
                .join(&key)
                .map_err(|_| unavailable("invalid Plex media file key"))?;
            if uri.origin() != base.origin()
                || !uri.username().is_empty()
                || uri.password().is_some()
            {
                return Err(unavailable("Plex media file key leaves its server"));
            }
            let source = SqliteMediaSourceRepository(pool.clone())
                .get_plex(source_id)
                .await?;
            let token = scoped_credential(
                pool,
                kind,
                &[format!(
                    "plex.server_auth_token.{}",
                    source.machine_identifier.unwrap_or_default()
                )],
                config.token,
            )
            .await?;
            (uri, format!("X-Plex-Token: {token}"))
        }
        RemoteMediaSourceKind::Jellyfin => {
            let config = crate::jellyfin::load_active_jellyfin_connection_config(pool, source_id)
                .await
                .map_err(|_| unavailable("Jellyfin connection is unavailable"))?;
            let item = provider_item_id(pool, media.item.media_item_id, kind).await?;
            let credential = scoped_credential(
                pool,
                kind,
                &[
                    format!("jellyfin.authorization_header.{}", config.address),
                    format!("jellyfin.api_key.{}", config.address),
                ],
                config.authorization_header,
            )
            .await?;
            (
                video_url(&config.address, &item)?,
                format!(
                    "Authorization: {}",
                    crate::jellyfin::format_jellyfin_auth_header(credential)
                ),
            )
        }
        RemoteMediaSourceKind::Emby => {
            let config = crate::emby::load_active_emby_connection_config(pool, source_id)
                .await
                .map_err(|_| unavailable("Emby connection is unavailable"))?;
            let item = provider_item_id(pool, media.item.media_item_id, kind).await?;
            let credential = scoped_credential(
                pool,
                kind,
                &[format!("emby.api_key.{}", config.address)],
                config.api_key,
            )
            .await?;
            (
                video_url(&config.address, &item)?,
                format!("X-Emby-Token: {credential}"),
            )
        }
    };
    with_persisted_hint(pool, media, http_source(&uri, header)?).await
}

async fn with_persisted_hint(
    pool: &DbPool,
    media: &ScheduledMedia,
    mut source: Source,
) -> Result<Source> {
    let hint = probe_hint::persisted(pool, media.media_version_id).await?;
    match &mut source {
        Source::Local { probe_hint, .. } | Source::Http { probe_hint, .. } => *probe_hint = hint,
        Source::Rtsp { .. } | Source::Dynamic { .. } | Source::Lavfi { .. } => {}
    }
    Ok(source)
}

fn http_source(uri: &Url, header: String) -> Result<Source> {
    let uri = uri.to_string();
    // Next expands templates in both fields. These are already resolved values,
    // so accidental environment interpolation and header injection must fail.
    if uri.contains("{{") || header.contains("{{") || header.chars().any(char::is_control) {
        return Err(unavailable("invalid media-server transport value"));
    }
    Ok(Source::Http {
        probe_hint: None,
        is_live: None,
        uri,
        headers: vec![header],
        user_agent: None,
        keep_alive: Some(false),
        reconnect: Some(true),
        reconnect_delay_max: Some(2),
        timeout_us: Some(10_000_000),
        in_point_ms: None,
        out_point_ms: None,
    })
}

// The administration contract also supports one legacy global credential.
// Never apply that fallback to one of several configured servers. In particular,
// a global Jellyfin Authorization value must not override a source-specific key.
pub(super) async fn scoped_credential(
    pool: &DbPool,
    kind: RemoteMediaSourceKind,
    keys: &[String],
    legacy: String,
) -> Result<String> {
    for key in keys {
        if let Some(value) = sqlx::query_scalar::<_, String>(
            r#"SELECT "Value" FROM "ConfigElement" WHERE "Key" = ? AND "Value" <> ''"#,
        )
        .bind(key)
        .fetch_optional(pool)
        .await
        .repository()?
        {
            return Ok(value);
        }
    }
    let query = match kind {
        RemoteMediaSourceKind::Plex => r#"SELECT COUNT(*) FROM "PlexMediaSource""#,
        RemoteMediaSourceKind::Jellyfin => r#"SELECT COUNT(*) FROM "JellyfinMediaSource""#,
        RemoteMediaSourceKind::Emby => r#"SELECT COUNT(*) FROM "EmbyMediaSource""#,
    };
    let count: i64 = sqlx::query_scalar(query)
        .fetch_one(pool)
        .await
        .repository()?;
    if count == 1 {
        Ok(legacy)
    } else {
        Err(unavailable("source-specific credential is required"))
    }
}

async fn provider_item_id(pool: &DbPool, id: i64, kind: RemoteMediaSourceKind) -> Result<String> {
    let query = match kind {
        RemoteMediaSourceKind::Jellyfin => {
            r#"SELECT "ItemId" FROM "JellyfinMovie" WHERE "Id" = ? UNION ALL SELECT "ItemId" FROM "JellyfinEpisode" WHERE "Id" = ?"#
        }
        RemoteMediaSourceKind::Emby => {
            r#"SELECT "ItemId" FROM "EmbyMovie" WHERE "Id" = ? UNION ALL SELECT "ItemId" FROM "EmbyEpisode" WHERE "Id" = ?"#
        }
        RemoteMediaSourceKind::Plex => return Err(unavailable("unexpected provider item lookup")),
    };
    let ids: Vec<Option<String>> = sqlx::query_scalar(query)
        .bind(id)
        .bind(id)
        .fetch_all(pool)
        .await
        .repository()?;
    match ids.as_slice() {
        [Some(id)] if !id.trim().is_empty() && id != "." && id != ".." => Ok(id.clone()),
        _ => Err(unavailable(
            "provider item identity is ambiguous or unavailable",
        )),
    }
}

pub(super) fn server_url(address: &str) -> Result<Url> {
    let url = Url::parse(address).map_err(|_| unavailable("invalid media-server address"))?;
    if !matches!(url.scheme(), "http" | "https")
        || url.host_str().is_none()
        || !url.username().is_empty()
        || url.password().is_some()
        || url.query().is_some()
        || url.fragment().is_some()
    {
        return Err(unavailable("invalid media-server address"));
    }
    Ok(url)
}

fn video_url(address: &str, item: &str) -> Result<Url> {
    let mut url = server_url(address)?;
    url.path_segments_mut()
        .map_err(|()| unavailable("invalid media-server path"))?
        .pop_if_empty()
        .extend(["Videos", item, "stream"]);
    url.query_pairs_mut().append_pair("static", "true");
    Ok(url)
}

fn local_source(path: String) -> Source {
    Source::Local {
        path,
        in_point_ms: None,
        out_point_ms: None,

        probe_hint: None,
    }
}

const fn platform() -> PathPlatform {
    #[cfg(windows)]
    {
        PathPlatform::Windows
    }
    #[cfg(not(windows))]
    {
        PathPlatform::Unix
    }
}

fn unavailable(message: &'static str) -> RepositoryError {
    RepositoryError::unavailable("Next media source", message)
}

#[cfg(test)]
#[path = "source_tests.rs"]
pub(crate) mod tests;
