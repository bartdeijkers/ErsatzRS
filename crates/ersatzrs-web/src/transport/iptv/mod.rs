pub mod access_token;
pub mod guide;
pub mod hls;
pub mod identifier;
pub mod logos;
pub mod next_canvas;
pub mod next_media;
pub mod next_subtitles;
pub mod playlist;
mod retained_stream_buffer;
pub mod stream;
pub mod streaming_specs;
pub mod variant;

use axum::{
    extract::{Path, Query, State},
    http::{HeaderMap, StatusCode},
    response::{IntoResponse, Response},
};

use crate::host::state::AppState;

/// Dispatches `/iptv/channel/{file}` between the MPEG-TS stream and the HLS
/// playlist based on the file extension. Ports the routing fanout from
/// `IptvController.GetTransportStreamVideo` / `GetHttpLiveStreamingVideo`.
pub async fn dispatch_channel_request(
    State(state): State<AppState>,
    Path(file): Path<String>,
    headers: HeaderMap,
    Query(query): Query<hls::HlsQuery>,
) -> Response {
    if let Some(channel_number) = file.strip_suffix(".m3u8") {
        return hls::serve_channel_m3u8(&state, channel_number, &headers, &query).await;
    }
    if let Some(channel_number) = file.strip_suffix(".ts") {
        return stream::serve_transport_stream(&state, channel_number, &headers, &query).await;
    }
    StatusCode::NOT_FOUND.into_response()
}

pub async fn get_concat_playlist(
    State(state): State<AppState>,
    Path(channel_number): Path<String>,
    headers: HeaderMap,
    Query(query): Query<hls::HlsQuery>,
) -> Response {
    stream::serve_concat_playlist(&state, &channel_number, &headers, &query).await
}

pub async fn get_internal_stream(
    State(state): State<AppState>,
    Path(channel_number): Path<String>,
) -> Response {
    stream::serve_internal_legacy_stream(&state, &channel_number).await
}
