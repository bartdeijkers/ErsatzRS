//! Artwork URL routing — port of `ErsatzTV/Controllers/ArtworkController.cs`.
//!
//! Provider routes carry a persisted artwork ID. The handler resolves the row,
//! verifies its provider prefix and derives the outbound path, query and resize
//! from stored data. Request query parameters never select provider operations.
//!
//! Registered absolute local paths remain database-gated. Local cache names
//! must contain at least two ASCII hex characters; the optional `.jpg` route
//! suffix is stripped before looking up the direct or sharded cache file.
//!
//! `ArtworkKind` numeric encoding matches `ErsatzTV`: `0 = Poster`,
//! `1 = Thumbnail`, `2 = Logo`, `3 = FanArt`, `4 = Watermark`.

use axum::{
    body::Body,
    extract::{Path, State},
    http::{header, StatusCode},
    response::{IntoResponse, Response},
};
use ersatzrs_core::repositories::ArtworkRepository as _;
use std::collections::HashMap;

use ersatzrs_infra::file_system_layout::data_path;

use crate::host::state::AppState;

#[cfg(test)]
#[path = "artwork_http_tests.rs"]
mod http_tests;

/// Project provider artwork by persisted identity, keeping its provider path
/// and query on the server. Local/external image locations retain their current
/// serving behavior.
#[must_use]
pub fn artwork_reference_url(
    artwork: &ersatzrs_core::repositories::ArtworkReference,
    kind: ArtworkUrlKind,
) -> Option<String> {
    for provider in ["plex", "jellyfin", "emby"] {
        if stored_provider_path(&artwork.path, provider).is_some() {
            return Some(format!(
                "/artwork/{}/{provider}/{}",
                kind.url_segment(),
                artwork.id
            ));
        }
    }
    local_artwork_url(&artwork.path, kind)
}

fn local_artwork_url(path: &str, kind: ArtworkUrlKind) -> Option<String> {
    let path = path.trim();
    if path.is_empty() {
        None
    } else if path.starts_with("http://") || path.starts_with("https://") {
        Some(path.to_owned())
    } else {
        Some(format!(
            "/artwork/{}/{}",
            kind.url_segment(),
            urlencoding::encode(path)
        ))
    }
}

fn stored_provider_path<'a>(path: &'a str, provider: &str) -> Option<&'a str> {
    let prefix = format!("{provider}://");
    if path
        .get(..prefix.len())
        .is_some_and(|value| value.eq_ignore_ascii_case(&prefix))
    {
        return path.get(prefix.len()..);
    }
    // Imported Plex artwork uses upstream's `plex/{source-id}/...` spelling.
    if provider == "plex"
        && path
            .get(..5)
            .is_some_and(|prefix| prefix.eq_ignore_ascii_case("plex/"))
    {
        return path.get(5..);
    }
    None
}

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
#[cfg(test)]
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

    #[cfg(test)]
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

#[cfg(test)]
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

/// Resolve upstream's local-cache redirect without exposing stored provider or
/// filesystem locations. Provider images use their explicit ID proxy routes.
pub async fn redirect_artwork(State(state): State<AppState>, Path(id): Path<String>) -> Response {
    use ersatzrs_domain::ArtworkKind;
    let Ok(id) = id.parse::<i64>() else {
        return StatusCode::NOT_FOUND.into_response();
    };
    let artwork = match state.repositories.artwork.find_by_id(id).await {
        Ok(Some(artwork)) => artwork,
        Ok(None) => return StatusCode::NOT_FOUND.into_response(),
        Err(_) => return StatusCode::INTERNAL_SERVER_ERROR.into_response(),
    };
    if cache_hash(&artwork.path) != Some(artwork.path.as_str()) {
        return StatusCode::NOT_FOUND.into_response();
    }
    let base = match artwork.kind {
        ArtworkKind::Poster => "/artwork/posters",
        ArtworkKind::Thumbnail => "/artwork/thumbnails",
        ArtworkKind::FanArt => "/artwork/fanart",
        ArtworkKind::Logo => "/iptv/logos",
        ArtworkKind::Watermark => "/artwork/watermarks",
    };
    (
        StatusCode::FOUND,
        [(header::LOCATION, format!("{base}/{}", artwork.path))],
    )
        .into_response()
}

/// `/artwork/posters/{*path}` — serves a poster image either from the local
/// cache (plain hash) or via the upstream Jellyfin / Emby proxy.
pub async fn serve_poster(
    State(state): State<AppState>,
    Path(full_path): Path<String>,
) -> Response {
    serve_artwork(state, "posters", &full_path).await
}

/// `/artwork/fanart/{*path}` — same as [`serve_poster`] for fan-art.
pub async fn serve_fanart(
    State(state): State<AppState>,
    Path(full_path): Path<String>,
) -> Response {
    serve_artwork(state, "fanart", &full_path).await
}

/// `/artwork/thumbnails/{*path}` — same as [`serve_poster`] for actor /
/// thumbnail artwork (`ErsatzTV` `ArtworkKind.Thumbnail`).
pub async fn serve_thumbnail(
    State(state): State<AppState>,
    Path(full_path): Path<String>,
) -> Response {
    serve_artwork(state, "thumbnails", &full_path).await
}

async fn serve_artwork(state: AppState, kind: &str, full_path: &str) -> Response {
    for provider in ["jellyfin", "emby", "plex"] {
        let prefix = format!("{provider}/");
        if let Some(id) = full_path.strip_prefix(&prefix) {
            let Ok(id) = id.parse::<i64>() else {
                return StatusCode::NOT_FOUND.into_response();
            };
            let artwork = match state.repositories.artwork.find_by_id(id).await {
                Ok(Some(artwork)) => artwork,
                Ok(None) => return StatusCode::NOT_FOUND.into_response(),
                Err(_) => return StatusCode::INTERNAL_SERVER_ERROR.into_response(),
            };
            let Some((path, params)) = provider_artwork_request(&artwork, provider) else {
                return StatusCode::NOT_FOUND.into_response();
            };
            return match provider {
                "jellyfin" => serve_jellyfin_artwork(&state, &path, &params).await,
                "emby" => serve_emby_artwork(&state, &path, &params).await,
                _ => serve_plex_artwork(&state, &path, &params).await,
            };
        }
    }
    serve_local_artwork(&state.repositories.artwork, kind, full_path).await
}

/// Only persisted artwork chooses the outbound path, query and resize. Caller
/// query parameters are never passed to this function or to the provider.
fn provider_artwork_request(
    artwork: &ersatzrs_core::repositories::ArtworkReference,
    provider: &str,
) -> Option<(String, HashMap<String, String>)> {
    use ersatzrs_domain::ArtworkKind;
    let stored = stored_provider_path(&artwork.path, provider)?;
    let (path, query) = split_path_query(stored);
    if path.is_empty() || path.starts_with('/') || path.contains('\\') {
        return None;
    }
    let mut params: HashMap<String, String> = serde_urlencoded::from_str(query).ok()?;
    if provider == "plex" {
        let (source, image) = path.split_once('/')?;
        source.parse::<i64>().ok()?;
        if image.is_empty() {
            return None;
        }
        let dimensions = match artwork.kind {
            ArtworkKind::Poster => Some((440, 304)),
            ArtworkKind::Thumbnail => Some((220, 392)),
            ArtworkKind::FanArt => None,
            _ => return None,
        };
        if let Some((height, width)) = dimensions {
            let image_url = if query.is_empty() {
                format!("/{image}")
            } else {
                format!("/{image}?{query}")
            };
            params = HashMap::from([
                ("url".to_owned(), image_url),
                ("height".to_owned(), height.to_string()),
                ("width".to_owned(), width.to_string()),
                ("minSize".to_owned(), "1".to_owned()),
                ("upscale".to_owned(), "0".to_owned()),
            ]);
            return Some((format!("{source}/photo/:/transcode"), params));
        }
    } else {
        let height = match artwork.kind {
            ArtworkKind::Poster => Some("440"),
            ArtworkKind::Thumbnail => Some("220"),
            _ => None,
        };
        if let Some(height) = height {
            let key = if provider == "jellyfin" {
                "fillHeight"
            } else {
                "maxHeight"
            };
            params.insert(key.to_owned(), height.to_owned());
        }
    }
    Some((path.to_owned(), params))
}

/// Match upstream's hex route constraint, including its optional `.jpg`
/// route suffix. The returned hash is safe to shard by its first two bytes.
fn cache_hash(value: &str) -> Option<&str> {
    let hash = value.strip_suffix(".jpg").unwrap_or(value);
    (hash.len() >= 2 && hash.bytes().all(|byte| byte.is_ascii_hexdigit())).then_some(hash)
}

/// Upstream cache names are hexadecimal. Native uploads use UUID filenames
/// with an image extension; retain that existing on-disk format while rejecting
/// decoded separators, absolute paths and arbitrary filenames before disk I/O.
pub(super) fn is_cached_image_filename(filename: &str) -> bool {
    let (stem, extension) = filename
        .rsplit_once('.')
        .map_or((filename, None), |(stem, extension)| {
            (stem, Some(extension))
        });
    let is_hex = stem.len() >= 2 && stem.bytes().all(|byte| byte.is_ascii_hexdigit());
    if is_hex && matches!(extension, None | Some("jpg")) {
        return true;
    }
    let is_native_uuid =
        uuid::Uuid::parse_str(stem).is_ok_and(|id| id.hyphenated().to_string() == stem);
    is_native_uuid
        && matches!(
            extension,
            Some("png" | "jpg" | "jpeg" | "gif" | "webp" | "jxl" | "svg" | "bin")
        )
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

    let Some(file_name) = cache_hash(file_name) else {
        return StatusCode::NOT_FOUND.into_response();
    };
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
    let sharded = base.join(&file_name[..2]).join(file_name);

    for candidate in [direct, sharded] {
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
        .redirect(reqwest::redirect::Policy::none())
        .timeout(std::time::Duration::from_secs(15))
        .build()
    else {
        return StatusCode::INTERNAL_SERVER_ERROR.into_response();
    };

    let response = tokio::time::timeout(
        std::time::Duration::from_secs(15),
        open_provider_artwork(&client, base_url, headers, params),
    )
    .await;
    match response {
        Ok(Ok(response)) => {
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
        _ => StatusCode::BAD_GATEWAY.into_response(),
    }
}

/// Follow provider redirects without carrying credentials across origins, even
/// when a subsequent redirect returns to the original provider.
async fn open_provider_artwork(
    client: &reqwest::Client,
    base_url: &str,
    headers: &[(&str, &str)],
    params: &HashMap<String, String>,
) -> Result<reqwest::Response, ()> {
    let mut url = reqwest::Url::parse_with_params(base_url, params).map_err(|_| ())?;
    let mut forward_credentials = true;
    for hop in 0..=5 {
        if !matches!(url.scheme(), "http" | "https")
            || !url.username().is_empty()
            || url.password().is_some()
        {
            return Err(());
        }
        let mut request = client.get(url.clone());
        if forward_credentials {
            for (name, value) in headers {
                request = request.header(*name, *value);
            }
        }
        let response = request.send().await.map_err(|_| ())?;
        if !matches!(response.status().as_u16(), 301 | 302 | 303 | 307 | 308) {
            return Ok(response);
        }
        if hop == 5 {
            return Err(());
        }
        let location = response
            .headers()
            .get(reqwest::header::LOCATION)
            .and_then(|value| value.to_str().ok())
            .ok_or(())?;
        let next = url.join(location).map_err(|_| ())?;
        forward_credentials &= next.origin() == url.origin();
        url = next;
    }
    Err(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn provider_identity_hides_stored_paths_and_uses_persisted_resize_kind() {
        use ersatzrs_core::repositories::ArtworkReference;
        use ersatzrs_domain::ArtworkKind;
        for (provider, key) in [("jellyfin", "fillHeight"), ("emby", "maxHeight")] {
            let mut artwork = ArtworkReference {
                id: 42,
                kind: ArtworkKind::Poster,
                path: format!(
                    "{}://Items/fixture/Images/Primary?tag=stored&{key}=999",
                    provider.to_uppercase()
                ),
            };
            assert_eq!(
                artwork_reference_url(&artwork, ArtworkUrlKind::Poster).unwrap(),
                format!("/artwork/posters/{provider}/42")
            );
            let (path, params) = provider_artwork_request(&artwork, provider).unwrap();
            assert_eq!(path, "Items/fixture/Images/Primary");
            assert_eq!(params.get("tag").unwrap(), "stored");
            assert_eq!(params.get(key).unwrap(), "440");
            artwork.kind = ArtworkKind::Thumbnail;
            assert_eq!(
                provider_artwork_request(&artwork, provider).unwrap().1[key],
                "220"
            );
            assert!(provider_artwork_request(&artwork, "plex").is_none());
            artwork.kind = ArtworkKind::FanArt;
            artwork.path = format!("{provider}://Items/fixture/Images/Backdrop?tag=stored");
            assert!(!provider_artwork_request(&artwork, provider)
                .unwrap()
                .1
                .contains_key(key));
        }
    }

    #[test]
    fn plex_identity_preserves_source_and_builds_fixed_transcode_request() {
        use ersatzrs_core::repositories::ArtworkReference;
        use ersatzrs_domain::ArtworkKind;
        for prefix in ["plex://7/", "plex/7/", "PLEX://7/", "PLEX/7/"] {
            let mut artwork = ArtworkReference {
                id: 42,
                kind: ArtworkKind::Poster,
                path: format!("{prefix}library/metadata/1/thumb/2?tag=stored"),
            };
            assert_eq!(
                artwork_reference_url(&artwork, ArtworkUrlKind::Poster).as_deref(),
                Some("/artwork/posters/plex/42")
            );
            let (path, params) = provider_artwork_request(&artwork, "plex").unwrap();
            assert_eq!(path, "7/photo/:/transcode");
            assert_eq!(params["url"], "/library/metadata/1/thumb/2?tag=stored");
            assert_eq!(params["height"], "440");
            assert_eq!(params["width"], "304");
            assert_eq!(params["minSize"], "1");
            assert_eq!(params["upscale"], "0");
            artwork.kind = ArtworkKind::Thumbnail;
            let (_, params) = provider_artwork_request(&artwork, "plex").unwrap();
            assert_eq!(params["height"], "220");
            assert_eq!(params["width"], "392");
            artwork.kind = ArtworkKind::FanArt;
            let (path, params) = provider_artwork_request(&artwork, "plex").unwrap();
            assert_eq!(path, "7/library/metadata/1/thumb/2");
            assert_eq!(params["tag"], "stored");
            assert!(!params.contains_key("height"));
        }
    }

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
    fn cache_hash_requires_at_least_two_ascii_hex_characters() {
        for value in ["ab", "ABCDEF0123456789", "abcdef0123456789"] {
            assert_eq!(cache_hash(value), Some(value));
            assert_eq!(cache_hash(&format!("{value}.jpg")), Some(value));
        }
        for value in [
            "",
            "a",
            "a.jpg",
            "../../../etc/passwd",
            "/etc/passwd",
            "ab/cd",
            "ab\\cd",
            "ab.png",
            "ab.jpg.jpg",
            "abcg",
            "€",
            "éé",
            " ab",
        ] {
            assert_eq!(
                cache_hash(value),
                None,
                "accepted invalid cache name {value}"
            );
        }
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

        assert_eq!(response.status(), StatusCode::OK);
        let original_modified = std::fs::metadata(&file_path).unwrap().modified().unwrap();
        tokio::fs::remove_file(&file_path).await.unwrap();
        assert_eq!(
            serve_local_artwork(&repo, "posters", path_str)
                .await
                .status(),
            StatusCode::NOT_FOUND
        );
        // Native scans register the original directly: restoring it with the
        // same mtime must be visible without a scan/cache clone fast path.
        tokio::fs::write(&file_path, b"recovered-image")
            .await
            .unwrap();
        std::fs::File::options()
            .write(true)
            .open(&file_path)
            .unwrap()
            .set_modified(original_modified)
            .unwrap();
        let restored = serve_local_artwork(&repo, "posters", path_str).await;
        assert_eq!(restored.status(), StatusCode::OK);
        assert_eq!(
            axum::body::to_bytes(restored.into_body(), 1024)
                .await
                .unwrap(),
            b"recovered-image"[..]
        );
        tokio::fs::remove_file(&file_path).await.unwrap();
    }

    #[tokio::test]
    async fn serve_local_artwork_rejects_traversal_on_the_hash_cache_branch() {
        let pool = test_db().await;
        let repo = ersatzrs_infra::repositories::SqliteArtworkRepository(pool);
        for name in ["../../../etc/passwd", "ab/cd", "€", "éé", "a", ""] {
            let response = serve_local_artwork(&repo, "posters", name).await;
            assert_eq!(response.status(), StatusCode::NOT_FOUND, "{name}");
        }
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
