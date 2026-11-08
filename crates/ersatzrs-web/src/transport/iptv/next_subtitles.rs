//! Next's bounded `WebVTT` playlist transport, under the existing IPTV auth layer.

use axum::{
    extract::{Path, Query, State},
    http::StatusCode,
    response::{IntoResponse, Response},
};
use ersatzrs_core::hls::{HlsSessionRegistry, HlsStartupReadiness};
use tokio::io::AsyncReadExt;

use super::{access_token::AccessToken, hls};
use crate::host::state::AppState;

pub async fn get_playlist(
    State(state): State<AppState>,
    Path(number): Path<String>,
    Query(query): Query<hls::HlsQuery>,
) -> Response {
    serve_playlist(
        &*state.hls_sessions,
        &number,
        false,
        AccessToken::for_request(
            query.access_token,
            state.iptv_access_token_verifier.is_enabled(),
        ),
    )
    .await
}

pub async fn get_preview_playlist(
    State(state): State<AppState>,
    Path(number): Path<String>,
    Query(query): Query<hls::HlsQuery>,
) -> Response {
    serve_playlist(
        &*state.preview_hls_sessions,
        &number,
        true,
        AccessToken::for_request(
            query.access_token,
            state.iptv_access_token_verifier.is_enabled(),
        ),
    )
    .await
}

async fn serve_playlist(
    registry: &dyn HlsSessionRegistry,
    number: &str,
    preview: bool,
    access_token: AccessToken,
) -> Response {
    if !hls::is_safe_channel_number_path_component(number) {
        return StatusCode::BAD_REQUEST.into_response();
    }
    let Some(handle) = registry.try_get(number).await else {
        return StatusCode::SERVICE_UNAVAILABLE.into_response();
    };
    handle.touch();
    if handle.startup_readiness() != HlsStartupReadiness::Ready {
        return StatusCode::SERVICE_UNAVAILABLE.into_response();
    }
    let folder = if preview {
        hls::preview_hls_working_dir(number)
    } else {
        hls::hls_working_dir(number)
    };
    let Some(playlist) = read_playlist(&folder.join("live_sub.m3u8")).await else {
        return StatusCode::NOT_FOUND.into_response();
    };
    if handle.startup_readiness() != HlsStartupReadiness::Ready {
        return StatusCode::SERVICE_UNAVAILABLE.into_response();
    }
    hls::hls_response(hls::propagate_playlist_access_token(
        &playlist,
        &access_token,
    ))
}

async fn read_playlist(path: &std::path::Path) -> Option<String> {
    const LIMIT: u64 = 256 * 1024;
    let mut file = tokio::fs::File::open(path).await.ok()?.take(LIMIT + 1);
    let mut bytes = Vec::new();
    file.read_to_end(&mut bytes).await.ok()?;
    if bytes.len() > usize::try_from(LIMIT).ok()? {
        return None;
    }
    let text = String::from_utf8(bytes).ok()?;
    text.starts_with("#EXTM3U").then_some(text)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn subtitle_playlist_rejects_missing_malformed_and_oversized_output() {
        let root = tempfile::tempdir().unwrap();
        let path = root.path().join("live_sub.m3u8");
        assert!(read_playlist(&path).await.is_none());
        for body in [b"not a playlist".to_vec(), vec![b'x'; 256 * 1024 + 1]] {
            tokio::fs::write(&path, body).await.unwrap();
            assert!(read_playlist(&path).await.is_none());
        }
        let playlist = "#EXTM3U\n#EXTINF:4.0,\nlive000001.vtt\n";
        tokio::fs::write(path, playlist).await.unwrap();
        assert_eq!(
            read_playlist(&root.path().join("live_sub.m3u8"))
                .await
                .as_deref(),
            Some(playlist)
        );
    }
}
