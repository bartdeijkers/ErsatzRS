//! Artwork URL routing — port of `ErsatzTV/Controllers/ArtworkController.cs`.
//!
//! Mirrors the `ErsatzTV` pattern:
//!
//! * Provider-prefixed paths stored in the DB (`plex://…`, `jellyfin://…`,
//!   `emby://…`)
//!   are projected to relative URLs of the form
//!   `jellyfin/{path}?{query}` / `emby/{path}?{query}` by [`relative_proxy_url`].
//! * `<img src="/artwork/posters/{relative}">` and
//!   `<img src="/artwork/fanart/{relative}">` then hit
//!   [`serve_poster`] / [`serve_fanart`], which proxy through to the upstream
//!   Jellyfin / Emby instance using the configured connection address and the
//!   stored API key.
//! * A local absolute filesystem path (what the local scanner actually
//!   stores in `Artwork.Path`) is percent-encoded into one opaque URL
//!   segment and served directly from its source location, gated on an
//!   exact match against a known `Artwork.Path` row — this route is
//!   unauthenticated, so that DB membership check is the only thing
//!   standing between it and arbitrary local file disclosure; do not relax
//!   it to a prefix/containment check without equal care.
//! * A short hash-style filename (local cache, sharded by the first two hex
//!   chars per `FileSystemLayout.PosterCacheFolder` — nothing currently
//!   populates this cache, but the lookup is kept for forward
//!   compatibility) is served from `data/artwork/{kind}/{hash}` after
//!   rejecting any path component that isn't an ordinary filename.
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

    if let Some(rest) = trimmed.strip_prefix("plex://") {
        let (path_segment, query) = split_path_query(rest);
        let mut url = format!("/artwork/{kind_segment}/plex/{path_segment}");
        let joined = build_query(query, [("height", max_height.to_string())]);
        if !joined.is_empty() {
            url.push('?');
            url.push_str(&joined);
        }
        return Some(url);
    }

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

    // A local absolute filesystem path (the scanner's stored `Artwork.Path`)
    // or a plain cache-hash filename. Percent-encode so an absolute path
    // becomes one opaque URL segment; Axum decodes it once before
    // `serve_local_artwork` validates it against known `Artwork.Path` rows.
    Some(format!(
        "/artwork/{kind_segment}/{}",
        urlencoding::encode(trimmed)
    ))
}

/// Which artwork URL kind to build. Picks the URL segment (`posters` /
/// `fanart`) and a sensible default upstream resize.
#[derive(Clone, Copy, Debug)]
pub enum ArtworkUrlKind {
    Poster,
    FanArt,
    Thumbnail,
}

/// Select the URL family and resize budget used by ErsatzTV's media-card
/// mapper for one admin-search kind.
#[must_use]
pub const fn artwork_kind_for_admin_search(
    kind: ersatzrs_core::repositories::AdminSearchKind,
) -> ArtworkUrlKind {
    use ersatzrs_core::repositories::AdminSearchKind as Kind;

    match kind {
        Kind::Movie | Kind::Show | Kind::Season | Kind::Episode => ArtworkUrlKind::Poster,
        Kind::Artist
        | Kind::MusicVideo
        | Kind::OtherVideo
        | Kind::Song
        | Kind::Image
        | Kind::RemoteStream => ArtworkUrlKind::Thumbnail,
    }
}

/// Select the media-card artwork family from the stable search/document kind
/// string used by reusable browser and collection projections.
#[must_use]
pub fn artwork_kind_for_media_kind(media_kind: &str) -> ArtworkUrlKind {
    match media_kind {
        "artist" | "music_video" | "other_video" | "song" | "image" | "remote_stream" => {
            ArtworkUrlKind::Thumbnail
        }
        _ => ArtworkUrlKind::Poster,
    }
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
            // `ErsatzTV`'s MediaCards mapper uses distinct 440px poster and
            // 220px thumbnail hints. Fan art remains full-size in upstream;
            // the existing proxy budget is unchanged here.
            ArtworkUrlKind::Poster => 440,
            ArtworkUrlKind::Thumbnail => 220,
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
    let extras = extras.into_iter().collect::<Vec<_>>();
    let mut parts: Vec<String> = if existing.is_empty() {
        Vec::new()
    } else {
        existing
            .split('&')
            .filter(|part| {
                let key = part.split_once('=').map_or(*part, |(key, _)| key);
                !extras.iter().any(|(extra, _)| *extra == key)
            })
            .map(str::to_string)
            .collect()
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
    if let Some(rest) = full_path.strip_prefix("plex/") {
        return serve_plex_artwork(&state, rest, params).await;
    }
    serve_local_artwork(&state.repositories.artwork, kind, full_path).await
}

/// Reject any path segment that isn't a single, ordinary filename component
/// (no `..`, no root/prefix, no embedded separators once decoded) — this is
/// the local-cache-lookup path, so nothing here should ever escape `base`.
fn is_safe_relative_path(value: &str) -> bool {
    let path = std::path::Path::new(value);
    path.components()
        .all(|component| matches!(component, std::path::Component::Normal(_)))
}

async fn serve_local_artwork(
    repository: &dyn ersatzrs_core::repositories::ArtworkRepository,
    kind: &str,
    file_name: &str,
) -> Response {
    // Axum's `Path<String>` extractor has already percent-decoded the wildcard
    // capture exactly once. Decoding again corrupts valid persisted paths that
    // contain a literal escape-looking component such as `%20`.
    let decoded_path = std::path::Path::new(file_name);

    if decoded_path.is_absolute() {
        // The DB is the trust boundary here: only a path the scanner already
        // persisted as an `Artwork.Path` value may ever be served, so a real
        // lookup failure must surface as an error, not silently become
        // "not found" and mask a genuine database problem.
        match repository.is_known_path(file_name).await {
            Ok(true) => {}
            Ok(false) => return StatusCode::NOT_FOUND.into_response(),
            Err(error) => {
                tracing::warn!(%error, "failed to check known artwork path");
                return StatusCode::INTERNAL_SERVER_ERROR.into_response();
            }
        }
        return match read_artwork_file(decoded_path).await {
            Ok(response) | Err(response) => response,
        };
    }

    if !is_safe_relative_path(file_name) {
        return StatusCode::NOT_FOUND.into_response();
    }
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
        match read_artwork_file(&candidate).await {
            Err(response) if response.status() == StatusCode::NOT_FOUND => {}
            Ok(response) | Err(response) => return response,
        }
    }

    StatusCode::NOT_FOUND.into_response()
}

/// Read an artwork file from a path already validated by the caller.
/// Returns `Err` with a `NOT_FOUND` or `INTERNAL_SERVER_ERROR` response on
/// failure so callers can distinguish "try the next candidate" from
/// "stop and return this error".
async fn read_artwork_file(path: &std::path::Path) -> Result<Response, Response> {
    match tokio::fs::read(path).await {
        Ok(bytes) => {
            let ext = path.extension().and_then(|e| e.to_str()).unwrap_or("jpg");
            let mime = match ext {
                "png" => "image/png",
                "gif" => "image/gif",
                "webp" => "image/webp",
                _ => "image/jpeg",
            };
            Ok((
                [
                    (header::CONTENT_TYPE, mime.to_string()),
                    (header::CACHE_CONTROL, "public, max-age=3600".to_string()),
                ],
                Body::from(bytes),
            )
                .into_response())
        }
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
            Err(StatusCode::NOT_FOUND.into_response())
        }
        Err(error) => {
            tracing::warn!(%error, path = %path.display(), "failed to read local artwork");
            Err(StatusCode::INTERNAL_SERVER_ERROR.into_response())
        }
    }
}

async fn serve_jellyfin_artwork(
    state: &AppState,
    rest: &str,
    params: &HashMap<String, String>,
) -> Response {
    // Need (address, api_key). Pick the first Jellyfin connection — `ErsatzTV`
    // allows only one active source at a time, so this matches its behaviour.
    let address = match state.application.first_jellyfin_address().await {
        Ok(Some(addr)) => addr,
        Ok(None) => return StatusCode::NOT_FOUND.into_response(),
        Err(_) => return StatusCode::INTERNAL_SERVER_ERROR.into_response(),
    };
    let Ok(Some(api_key)) = state
        .application
        .get_optional_setting("jellyfin.current_server_api_key")
        .await
    else {
        return StatusCode::NOT_FOUND.into_response();
    };
    let auth_header = ersatzrs_infra::jellyfin::format_jellyfin_auth_header(api_key);

    let url = format!("{}/{rest}", address.trim_end_matches('/'));
    proxy_request(&url, &[("Authorization", auth_header.as_str())], params).await
}

async fn serve_plex_artwork(
    state: &AppState,
    rest: &str,
    params: &HashMap<String, String>,
) -> Response {
    let Some((source_id, provider_path)) = rest.split_once('/') else {
        return StatusCode::NOT_FOUND.into_response();
    };
    let Ok(source_id) = source_id.parse::<i64>() else {
        return StatusCode::NOT_FOUND.into_response();
    };
    let connection = match state.application.plex_artwork_connection(source_id).await {
        Ok(Some(connection)) => connection,
        Ok(None) => return StatusCode::NOT_FOUND.into_response(),
        Err(_) => return StatusCode::INTERNAL_SERVER_ERROR.into_response(),
    };
    let url = format!(
        "{}/{provider_path}",
        connection.address.trim_end_matches('/')
    );
    proxy_request(&url, &[("X-Plex-Token", connection.token.as_str())], params).await
}

async fn serve_emby_artwork(
    state: &AppState,
    rest: &str,
    params: &HashMap<String, String>,
) -> Response {
    let address = match state.application.first_emby_address().await {
        Ok(Some(addr)) => addr,
        Ok(None) => return StatusCode::NOT_FOUND.into_response(),
        Err(_) => return StatusCode::INTERNAL_SERVER_ERROR.into_response(),
    };
    let Ok(Some(api_key)) = state
        .application
        .get_optional_setting("emby.current_server_api_key")
        .await
    else {
        return StatusCode::NOT_FOUND.into_response();
    };

    let url = format!("{}/{rest}", address.trim_end_matches('/'));
    proxy_request(&url, &[("X-Emby-Token", api_key.as_str())], params).await
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
    fn plex_path_becomes_token_hiding_proxy_url() {
        let url = artwork_url(
            "plex://42/library/metadata/800/thumb",
            ArtworkUrlKind::Thumbnail,
        )
        .unwrap();
        assert_eq!(
            url,
            "/artwork/thumbnails/plex/42/library/metadata/800/thumb?height=220"
        );
        assert!(!url.contains("X-Plex-Token"));
    }

    #[test]
    fn thumbnail_paths_use_thumbnail_family_and_upstream_resize_hints() {
        assert_eq!(
            artwork_url(
                "jellyfin://Items/thumb/Images/Primary?tag=jf",
                ArtworkUrlKind::Thumbnail,
            )
            .as_deref(),
            Some("/artwork/thumbnails/jellyfin/Items/thumb/Images/Primary?tag=jf&fillHeight=220")
        );
        assert_eq!(
            artwork_url(
                "emby://Items/thumb/Images/Primary?tag=emby",
                ArtworkUrlKind::Thumbnail,
            )
            .as_deref(),
            Some("/artwork/thumbnails/emby/Items/thumb/Images/Primary?tag=emby&maxHeight=220")
        );
        assert_eq!(
            artwork_url("thumbnail.jpg", ArtworkUrlKind::Thumbnail).as_deref(),
            Some("/artwork/thumbnails/thumbnail.jpg")
        );
        assert_eq!(
            artwork_url(
                "jellyfin://Items/thumb/Images/Primary?tag=jf&fillHeight=999&quality=90",
                ArtworkUrlKind::Thumbnail,
            )
            .as_deref(),
            Some(
                "/artwork/thumbnails/jellyfin/Items/thumb/Images/Primary?tag=jf&quality=90&fillHeight=220"
            )
        );
    }

    #[test]
    fn every_search_kind_maps_to_the_upstream_artwork_family() {
        use ersatzrs_core::repositories::AdminSearchKind as Kind;

        for kind in [Kind::Movie, Kind::Show, Kind::Season, Kind::Episode] {
            assert!(matches!(
                artwork_kind_for_admin_search(kind),
                ArtworkUrlKind::Poster
            ));
        }
        for kind in [
            Kind::Artist,
            Kind::MusicVideo,
            Kind::OtherVideo,
            Kind::Song,
            Kind::Image,
            Kind::RemoteStream,
        ] {
            assert!(matches!(
                artwork_kind_for_admin_search(kind),
                ArtworkUrlKind::Thumbnail
            ));
        }
        for kind in [
            "artist",
            "music_video",
            "other_video",
            "song",
            "image",
            "remote_stream",
        ] {
            assert!(matches!(
                artwork_kind_for_media_kind(kind),
                ArtworkUrlKind::Thumbnail
            ));
        }
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

    #[test]
    fn local_absolute_path_is_percent_encoded_and_round_trips() {
        let raw = "/media/movies/Big Buck Bunny/poster.jpg";
        let url = artwork_url(raw, ArtworkUrlKind::Poster).unwrap();
        let encoded = url.strip_prefix("/artwork/posters/").unwrap();
        assert!(!encoded.contains('/'), "should be one opaque segment");
        assert_eq!(
            urlencoding::decode(encoded).unwrap().into_owned(),
            raw,
            "must decode back to the exact stored path"
        );
    }

    #[test]
    fn is_safe_relative_path_rejects_traversal_and_absolute_segments() {
        assert!(is_safe_relative_path("abcdef0123456789"));
        assert!(!is_safe_relative_path("../../../etc/passwd"));
        assert!(!is_safe_relative_path("/etc/passwd"));
        assert!(!is_safe_relative_path("a/../../b"));
    }

    async fn test_db() -> ersatzrs_infra::db::DbPool {
        let pool = sqlx::sqlite::SqlitePoolOptions::new()
            .max_connections(1)
            .connect("sqlite::memory:")
            .await
            .expect("create in-memory sqlite pool");
        sqlx::migrate!("../../migrations")
            .run(&pool)
            .await
            .expect("run migrations");
        pool
    }

    #[tokio::test]
    async fn serve_local_artwork_rejects_absolute_paths_not_in_the_database() {
        let pool = test_db().await;
        let repo = ersatzrs_infra::repositories::SqliteArtworkRepository(pool);
        let response = serve_local_artwork(&repo, "posters", "/etc/passwd").await;
        assert_eq!(response.status(), StatusCode::NOT_FOUND);
    }

    #[tokio::test]
    async fn serve_local_artwork_serves_a_path_registered_in_the_artwork_table() {
        let pool = test_db().await;
        let file_path = std::env::temp_dir().join(format!(
            "ersatzrs-artwork-test-{}.jpg",
            uuid::Uuid::new_v4()
        ));
        tokio::fs::write(&file_path, b"fake-jpeg-bytes")
            .await
            .expect("write fixture file");
        let path_str = file_path.to_str().expect("utf-8 path");
        sqlx::query(
            r#"INSERT INTO "Artwork" ("ArtworkKind", "Path", "DateAdded", "DateUpdated")
               VALUES (0, ?, '2026-01-01 00:00:00', '2026-01-01 00:00:00')"#,
        )
        .bind(path_str)
        .execute(&pool)
        .await
        .expect("insert artwork row");

        let repo = ersatzrs_infra::repositories::SqliteArtworkRepository(pool);
        let response = serve_local_artwork(&repo, "posters", path_str).await;

        tokio::fs::remove_file(&file_path).await.ok();

        assert_eq!(response.status(), StatusCode::OK);
    }

    #[tokio::test]
    async fn serve_local_artwork_rejects_traversal_on_the_hash_cache_branch() {
        let pool = test_db().await;
        let repo = ersatzrs_infra::repositories::SqliteArtworkRepository(pool);
        let response = serve_local_artwork(&repo, "posters", "../../../etc/passwd").await;
        assert_eq!(response.status(), StatusCode::NOT_FOUND);
    }

    #[tokio::test]
    async fn serve_local_artwork_preserves_literal_percent_escape_sequences() {
        let pool = test_db().await;
        let repo = ersatzrs_infra::repositories::SqliteArtworkRepository(pool.clone());
        let fixture_root = std::env::temp_dir().join(format!(
            "ersatzrs-artwork-percent-test-{}",
            uuid::Uuid::new_v4()
        ));
        let literal_escape_dir = fixture_root.join("literal%20directory");
        tokio::fs::create_dir_all(&literal_escape_dir)
            .await
            .expect("create fixture directory");
        let file_path = literal_escape_dir.join("poster.jpg");
        tokio::fs::write(&file_path, b"fake-jpeg-bytes")
            .await
            .expect("write fixture file");
        let path_str = file_path.to_str().expect("utf-8 path");
        sqlx::query(
            r#"INSERT INTO "Artwork" ("ArtworkKind", "Path", "DateAdded", "DateUpdated")
               VALUES (0, ?, '2026-01-01 00:00:00', '2026-01-01 00:00:00')"#,
        )
        .bind(path_str)
        .execute(&pool)
        .await
        .expect("insert artwork row");

        let response = serve_local_artwork(&repo, "posters", path_str).await;

        tokio::fs::remove_dir_all(&fixture_root).await.ok();
        assert_eq!(response.status(), StatusCode::OK);
    }
}
