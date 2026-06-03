//! Artwork URL routing — port of `ErsatzTV/Controllers/ArtworkController.cs`.
//!
//! Mirrors the `ErsatzTV` pattern:
//!
//! * Provider-prefixed paths stored in the DB (`jellyfin://…`, `emby://…`)
//!   are projected to relative URLs of the form
//!   `jellyfin/{path}?{query}` / `emby/{path}?{query}` by [`relative_proxy_url`].
//! * `<img src="/artwork/posters/{relative}">` and
//!   `<img src="/artwork/fanart/{relative}">` then hit
//!   [`serve_poster`] / [`serve_fanart`], which proxy through to the upstream
//!   Jellyfin / Emby instance using the configured connection address and the
//!   stored API key.
//! * For plain hash paths (local cache, currently not yet written) the same
//!   routes serve from a local cache folder under `data/artwork/{kind}/{hash}`,
//!   matching `FileSystemLayout.PosterCacheFolder`.
//!
//! `ArtworkKind` numeric encoding matches `ErsatzTV`: `0 = Poster`,
//! `1 = Thumbnail`, `2 = Logo`, `3 = FanArt`, `4 = Watermark`.

use axum::{
    body::Body,
    extract::{Path, Query, State},
    http::{header, StatusCode},
    response::{IntoResponse, Response},
};
use serde::Deserialize;
use sqlx::Row;
use std::collections::HashMap;

use ersatzrs_infra::file_system_layout::data_path;

use crate::host::state::AppState;

/// Convert a DB-stored artwork path (`jellyfin://Items/.../Primary?tag=X`, a
/// raw SHA-1 hash filename, or `http(s)://…`) into a URL suitable for an
/// `<img src=…>` rendered on the admin page.
///
/// Mirrors `ErsatzTV/Application/MediaCards/Mapper.cs` (`GetPoster`,
/// `GetThumbnail`, actor `ProjectToViewModel`) and `Movies/Mapper.cs`
/// (`Artwork`).
///
/// `kind` is the URL-segment kind (`"posters"` or `"fanart"`). `max_height`
/// is appended as `fillHeight=N` (Jellyfin) / `maxHeight=N` (Emby) to size
/// thumbnails server-side, matching `ErsatzTV`.
#[must_use]
pub fn artwork_url(path: &str, kind: ArtworkUrlKind) -> Option<String> {
    let trimmed = path.trim();
    if trimmed.is_empty() {
        return None;
    }
    if trimmed.starts_with("http://") || trimmed.starts_with("https://") {
        return Some(trimmed.to_string());
    }

    let kind_segment = kind.url_segment();
    let max_height = kind.max_height();

    if let Some(rest) = trimmed.strip_prefix("jellyfin://") {
        let (path_segment, query) = split_path_query(rest);
        let mut url = format!("/artwork/{kind_segment}/jellyfin/{path_segment}");
        let mut joined = build_query(query, [("fillHeight", max_height.to_string())]);
        if !joined.is_empty() {
            url.push('?');
            url.push_str(&joined);
        }
        // Move out to avoid clippy warning.
        joined.clear();
        return Some(url);
    }
    if let Some(rest) = trimmed.strip_prefix("emby://") {
        let (path_segment, query) = split_path_query(rest);
        let mut url = format!("/artwork/{kind_segment}/emby/{path_segment}");
        let joined = build_query(query, [("maxHeight", max_height.to_string())]);
        if !joined.is_empty() {
            url.push('?');
            url.push_str(&joined);
        }
        return Some(url);
    }

    // Plain hash filename — served from local cache.
    Some(format!("/artwork/{kind_segment}/{trimmed}"))
}

/// Which artwork URL kind to build. Picks the URL segment (`posters` /
/// `fanart`) and a sensible default upstream resize.
#[derive(Clone, Copy, Debug)]
pub enum ArtworkUrlKind {
    Poster,
    FanArt,
    Thumbnail,
}

impl ArtworkUrlKind {
    fn url_segment(self) -> &'static str {
        match self {
            ArtworkUrlKind::Poster => "posters",
            ArtworkUrlKind::FanArt => "fanart",
            ArtworkUrlKind::Thumbnail => "thumbnails",
        }
    }

    fn max_height(self) -> i32 {
        match self {
            // `ErsatzTV`'s MediaCards `GetPoster` uses 440. Fan art is full-size
            // (no resize parameter on ETV either). Actor thumbnails reuse the
            // poster resize budget.
            ArtworkUrlKind::Poster | ArtworkUrlKind::Thumbnail => 440,
            ArtworkUrlKind::FanArt => 1080,
        }
    }
}

fn split_path_query(rest: &str) -> (&str, &str) {
    match rest.split_once('?') {
        Some((path, query)) => (path, query),
        None => (rest, ""),
    }
}

fn build_query<I>(existing: &str, extras: I) -> String
where
    I: IntoIterator<Item = (&'static str, String)>,
{
    let mut parts: Vec<String> = if existing.is_empty() {
        Vec::new()
    } else {
        existing.split('&').map(str::to_string).collect()
    };
    for (k, v) in extras {
        parts.push(format!("{k}={v}"));
    }
    parts.join("&")
}

// ---------------------------------------------------------------------------
// Axum handlers
// ---------------------------------------------------------------------------

/// `/artwork/posters/{*path}` — serves a poster image either from the local
/// cache (plain hash) or via the upstream Jellyfin / Emby proxy.
pub async fn serve_poster(
    State(state): State<AppState>,
    Path(full_path): Path<String>,
    Query(params): Query<HashMap<String, String>>,
) -> Response {
    serve_artwork(state, "posters", &full_path, &params).await
}

/// `/artwork/fanart/{*path}` — same as [`serve_poster`] for fan-art.
pub async fn serve_fanart(
    State(state): State<AppState>,
    Path(full_path): Path<String>,
    Query(params): Query<HashMap<String, String>>,
) -> Response {
    serve_artwork(state, "fanart", &full_path, &params).await
}

/// `/artwork/thumbnails/{*path}` — same as [`serve_poster`] for actor /
/// thumbnail artwork (`ErsatzTV` `ArtworkKind.Thumbnail`).
pub async fn serve_thumbnail(
    State(state): State<AppState>,
    Path(full_path): Path<String>,
    Query(params): Query<HashMap<String, String>>,
) -> Response {
    serve_artwork(state, "thumbnails", &full_path, &params).await
}

#[derive(Debug, Deserialize)]
#[allow(dead_code)]
struct CommonResizeParams {
    #[serde(default)]
    fill_height: Option<i32>,
    #[serde(default)]
    max_height: Option<i32>,
}

async fn serve_artwork(
    state: AppState,
    kind: &str,
    full_path: &str,
    params: &HashMap<String, String>,
) -> Response {
    if let Some(rest) = full_path.strip_prefix("jellyfin/") {
        return serve_jellyfin_artwork(&state, rest, params).await;
    }
    if let Some(rest) = full_path.strip_prefix("emby/") {
        return serve_emby_artwork(&state, rest, params).await;
    }
    serve_local_artwork(kind, full_path).await
}

async fn serve_local_artwork(kind: &str, file_name: &str) -> Response {
    let base = match kind {
        "posters" => data_path("artwork/posters"),
        "fanart" => data_path("artwork/fanart"),
        "thumbnails" => data_path("artwork/thumbnails"),
        _ => return StatusCode::NOT_FOUND.into_response(),
    };
    // Hash filenames are sharded by the first two hex chars in `ErsatzTV`
    // (`FileSystemLayout.PosterCacheFolder/{hash[..2]}/{hash}`). Honour both
    // forms so existing on-disk caches keep working.
    let direct = base.join(file_name);
    let sharded = if file_name.len() >= 2 {
        Some(base.join(&file_name[..2]).join(file_name))
    } else {
        None
    };

    for candidate in [direct, sharded.unwrap_or_default()] {
        if candidate.as_os_str().is_empty() {
            continue;
        }
        if let Ok(bytes) = tokio::fs::read(&candidate).await {
            let ext = candidate
                .extension()
                .and_then(|e| e.to_str())
                .unwrap_or("jpg");
            let mime = match ext {
                "png" => "image/png",
                "gif" => "image/gif",
                "webp" => "image/webp",
                _ => "image/jpeg",
            };
            return (
                [
                    (header::CONTENT_TYPE, mime.to_string()),
                    (header::CACHE_CONTROL, "public, max-age=3600".to_string()),
                ],
                Body::from(bytes),
            )
                .into_response();
        }
    }

    StatusCode::NOT_FOUND.into_response()
}

async fn serve_jellyfin_artwork(
    state: &AppState,
    rest: &str,
    params: &HashMap<String, String>,
) -> Response {
    // Need (address, api_key). Pick the first Jellyfin connection — `ErsatzTV`
    // allows only one active source at a time, so this matches its behaviour.
    let address = match query_first_jellyfin_address(&state.pool).await {
        Ok(Some(addr)) => addr,
        Ok(None) => return StatusCode::NOT_FOUND.into_response(),
        Err(_) => return StatusCode::INTERNAL_SERVER_ERROR.into_response(),
    };
    let Ok(Some(api_key)) =
        query_config_value(&state.pool, "jellyfin.current_server_api_key").await
    else {
        return StatusCode::NOT_FOUND.into_response();
    };
    let auth_header = ersatzrs_infra::jellyfin::format_jellyfin_auth_header(api_key);

    let url = format!("{}/{rest}", address.trim_end_matches('/'));
    proxy_request(&url, &[("Authorization", auth_header.as_str())], params).await
}

async fn serve_emby_artwork(
    state: &AppState,
    rest: &str,
    params: &HashMap<String, String>,
) -> Response {
    let address = match query_first_emby_address(&state.pool).await {
        Ok(Some(addr)) => addr,
        Ok(None) => return StatusCode::NOT_FOUND.into_response(),
        Err(_) => return StatusCode::INTERNAL_SERVER_ERROR.into_response(),
    };
    let Ok(Some(api_key)) = query_config_value(&state.pool, "emby.current_server_api_key").await
    else {
        return StatusCode::NOT_FOUND.into_response();
    };

    let url = format!("{}/{rest}", address.trim_end_matches('/'));
    proxy_request(&url, &[("X-Emby-Token", api_key.as_str())], params).await
}

async fn query_config_value(
    pool: &ersatzrs_infra::db::DbPool,
    key: &str,
) -> Result<Option<String>, sqlx::Error> {
    let value: Option<String> =
        sqlx::query_scalar(r#"SELECT "Value" FROM "ConfigElement" WHERE "Key" = ?"#)
            .bind(key)
            .fetch_optional(pool)
            .await?
            .and_then(|v: Option<String>| v)
            .filter(|v| !v.is_empty());
    Ok(value)
}

async fn query_first_jellyfin_address(
    pool: &ersatzrs_infra::db::DbPool,
) -> Result<Option<String>, sqlx::Error> {
    let row = sqlx::query(r#"SELECT "Address" FROM "JellyfinConnection" ORDER BY "Id" LIMIT 1"#)
        .fetch_optional(pool)
        .await?;
    Ok(row.and_then(|r| r.try_get::<Option<String>, _>("Address").ok().flatten()))
}

async fn query_first_emby_address(
    pool: &ersatzrs_infra::db::DbPool,
) -> Result<Option<String>, sqlx::Error> {
    let row = sqlx::query(r#"SELECT "Address" FROM "EmbyConnection" ORDER BY "Id" LIMIT 1"#)
        .fetch_optional(pool)
        .await?;
    Ok(row.and_then(|r| r.try_get::<Option<String>, _>("Address").ok().flatten()))
}

async fn proxy_request(
    base_url: &str,
    headers: &[(&str, &str)],
    params: &HashMap<String, String>,
) -> Response {
    let Ok(client) = reqwest::Client::builder()
        .timeout(std::time::Duration::from_secs(15))
        .build()
    else {
        return StatusCode::INTERNAL_SERVER_ERROR.into_response();
    };

    let mut builder = client.get(base_url);
    for (k, v) in headers {
        builder = builder.header(*k, *v);
    }
    if !params.is_empty() {
        let q: Vec<(&str, &str)> = params
            .iter()
            .map(|(k, v)| (k.as_str(), v.as_str()))
            .collect();
        builder = builder.query(&q);
    }

    match builder.send().await {
        Ok(response) => {
            let status = response.status();
            if !status.is_success() {
                return StatusCode::from_u16(status.as_u16())
                    .unwrap_or(StatusCode::BAD_GATEWAY)
                    .into_response();
            }
            let content_type = response
                .headers()
                .get(reqwest::header::CONTENT_TYPE)
                .and_then(|v| v.to_str().ok())
                .unwrap_or("image/jpeg")
                .to_string();
            let stream = response.bytes_stream();
            let body = Body::from_stream(stream);
            (
                [
                    (header::CONTENT_TYPE, content_type),
                    (header::CACHE_CONTROL, "public, max-age=3600".to_string()),
                ],
                body,
            )
                .into_response()
        }
        Err(_) => StatusCode::BAD_GATEWAY.into_response(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn jellyfin_artwork_path_becomes_proxy_url_with_fill_height() {
        let path = "jellyfin://Items/abc123/Images/Primary?tag=deadbeef";
        let url = artwork_url(path, ArtworkUrlKind::Poster).unwrap();
        assert_eq!(
            url,
            "/artwork/posters/jellyfin/Items/abc123/Images/Primary?tag=deadbeef&fillHeight=440"
        );
    }

    #[test]
    fn jellyfin_fanart_path_becomes_fanart_proxy_url() {
        let path = "jellyfin://Items/abc123/Images/Backdrop?tag=cafef00d";
        let url = artwork_url(path, ArtworkUrlKind::FanArt).unwrap();
        assert!(url.starts_with("/artwork/fanart/jellyfin/Items/abc123/Images/Backdrop?"));
        assert!(url.contains("tag=cafef00d"));
        assert!(url.contains("fillHeight=1080"));
    }

    #[test]
    fn emby_path_becomes_emby_proxy_url() {
        let path = "emby://Items/42/Images/Primary?tag=ff";
        let url = artwork_url(path, ArtworkUrlKind::Poster).unwrap();
        assert_eq!(
            url,
            "/artwork/posters/emby/Items/42/Images/Primary?tag=ff&maxHeight=440"
        );
    }

    #[test]
    fn http_path_passes_through() {
        let path = "https://example.com/poster.jpg";
        let url = artwork_url(path, ArtworkUrlKind::Poster).unwrap();
        assert_eq!(url, "https://example.com/poster.jpg");
    }

    #[test]
    fn plain_hash_path_routes_through_local_cache() {
        let url = artwork_url("ABCDEF0123456789", ArtworkUrlKind::Poster).unwrap();
        assert_eq!(url, "/artwork/posters/ABCDEF0123456789");
    }

    #[test]
    fn empty_path_yields_none() {
        assert!(artwork_url("", ArtworkUrlKind::Poster).is_none());
        assert!(artwork_url("   ", ArtworkUrlKind::FanArt).is_none());
    }
}
