//! Private song PNG callback with occurrence and exact-session admission.

use axum::{
    extract::{Query, State},
    http::{header, HeaderMap, Method, StatusCode},
    response::{IntoResponse, Response},
};
use ersatzrs_infra::next_engine::{
    host::NextHost,
    media::{ACCESS_HEADER, SESSION_HEADER, SONG_HEADER},
    song_background::SongBinding,
};

#[derive(serde::Deserialize)]
pub struct SongQuery {
    binding: String,
}

pub async fn get_song_background(
    State(state): State<crate::host::state::AppState>,
    Query(query): Query<SongQuery>,
    method: Method,
    headers: HeaderMap,
) -> Response {
    match tokio::time::timeout(
        std::time::Duration::from_secs(20),
        serve(&state.next_engine, &query.binding, &method, &headers),
    )
    .await
    {
        Ok(response) => response,
        Err(_) => StatusCode::SERVICE_UNAVAILABLE.into_response(),
    }
}

async fn serve(
    host: &NextHost,
    raw_binding: &str,
    method: &Method,
    headers: &HeaderMap,
) -> Response {
    let Ok(access) = single(headers, ACCESS_HEADER) else {
        return StatusCode::UNAUTHORIZED.into_response();
    };
    if !host.media.authorized(access) {
        return StatusCode::UNAUTHORIZED.into_response();
    }
    let Ok(ticket) = single(headers, SONG_HEADER) else {
        return StatusCode::UNAUTHORIZED.into_response();
    };
    if raw_binding.len() > 3072 {
        return StatusCode::BAD_REQUEST.into_response();
    }
    let Ok(binding) = serde_json::from_str::<SongBinding>(raw_binding) else {
        return StatusCode::BAD_REQUEST.into_response();
    };
    if !host
        .media
        .authorized_song(&binding, ticket, chrono::Utc::now())
    {
        return StatusCode::UNAUTHORIZED.into_response();
    }
    if let Err(status) = super::next_media::prepared_request(method, headers) {
        return (status, [(header::ALLOW, "GET")]).into_response();
    }
    let Ok(instance) = single(headers, SESSION_HEADER)
        .and_then(|value| uuid::Uuid::parse_str(value).map_err(|_| StatusCode::BAD_REQUEST))
    else {
        return StatusCode::BAD_REQUEST.into_response();
    };
    let channel = if headers.contains_key("x-etv-channel") {
        match single(headers, "x-etv-channel") {
            Ok(value) => Some(value),
            Err(status) => return status.into_response(),
        }
    } else {
        None
    };
    if host
        .validate_song_session(instance, &binding, channel)
        .await
        .is_err()
    {
        return StatusCode::GONE.into_response();
    }
    match host.render_song_background(instance, &binding).await {
        Ok(reader) => {
            let mut response = super::next_media::prepared_response(reader);
            response.headers_mut().insert(
                header::CONTENT_TYPE,
                header::HeaderValue::from_static("image/png"),
            );
            response
        }
        Err(_) => StatusCode::SERVICE_UNAVAILABLE.into_response(),
    }
}

fn single<'a>(headers: &'a HeaderMap, name: &str) -> Result<&'a str, StatusCode> {
    let mut values = headers.get_all(name).iter();
    let value = values.next().ok_or(StatusCode::BAD_REQUEST)?;
    if values.next().is_some() {
        return Err(StatusCode::BAD_REQUEST);
    }
    value.to_str().map_err(|_| StatusCode::BAD_REQUEST)
}

#[cfg(test)]
mod tests {
    use super::*;
    use ersatzrs_infra::next_engine::{
        canvas::CanvasBinding,
        protocol::{ProbeHint, Source},
    };

    fn hint() -> ProbeHint {
        ProbeHint {
            video: Vec::new(),
            audio: Vec::new(),
            subtitle: Vec::new(),
            format_name: Some("png_pipe".into()),
            duration_ms: Some(30_000),
        }
    }

    fn binding() -> SongBinding {
        let start = chrono::Utc::now();
        SongBinding {
            occurrence: CanvasBinding {
                session: Some(uuid::Uuid::new_v4()),
                playout_start: None,
                video_stream_index: None,
                channel: 1,
                item: 2,
                media: 3,
                version: 4,
                file: 5,
                start,
                finish: start + chrono::TimeDelta::seconds(30),
                in_point_ms: 0,
                out_point_ms: 30_000,
                expires: start + chrono::TimeDelta::minutes(5),
            },
            width: 320,
            height: 180,
            mode: ersatzrs_domain::ChannelSongVideoMode::Default,
        }
    }

    #[tokio::test]
    async fn song_admission_rejects_forgery_and_stopped_sessions_without_database_access() {
        let state = crate::host::state::AppState::for_tests(
            crate::host::state::IptvAccessTokenVerifier::disabled(),
        );
        let host = &state.next_engine;
        host.media
            .configure("127.0.0.1:8409".parse().unwrap())
            .unwrap();
        let binding = binding();
        let raw = serde_json::to_string(&binding).unwrap();
        assert_eq!(
            serve(host, &raw, &Method::GET, &HeaderMap::new())
                .await
                .status(),
            StatusCode::UNAUTHORIZED
        );
        let Source::Http {
            headers: published, ..
        } = host.media.song_source(&binding, hint()).unwrap()
        else {
            panic!("song source")
        };
        let mut headers = HeaderMap::new();
        for entry in published {
            let (name, value) = entry.split_once(": ").unwrap();
            headers.insert(
                name.parse::<header::HeaderName>().unwrap(),
                value.parse().unwrap(),
            );
        }
        assert_eq!(
            serve(host, &raw, &Method::GET, &headers).await.status(),
            StatusCode::GONE
        );
        assert_eq!(
            serve(host, &raw, &Method::HEAD, &headers).await.status(),
            StatusCode::METHOD_NOT_ALLOWED
        );
        let mut ranged = headers.clone();
        ranged.insert(header::RANGE, "bytes=1-".parse().unwrap());
        assert_eq!(
            serve(host, &raw, &Method::GET, &ranged).await.status(),
            StatusCode::RANGE_NOT_SATISFIABLE
        );
        let mut forged = binding.clone();
        forged.width += 1;
        assert_eq!(
            serve(
                host,
                &serde_json::to_string(&forged).unwrap(),
                &Method::GET,
                &headers
            )
            .await
            .status(),
            StatusCode::UNAUTHORIZED
        );
        forged = binding.clone();
        forged.occurrence.session = Some(uuid::Uuid::new_v4());
        assert_eq!(
            serve(
                host,
                &serde_json::to_string(&forged).unwrap(),
                &Method::GET,
                &headers
            )
            .await
            .status(),
            StatusCode::UNAUTHORIZED
        );
        let mut duplicates = headers.clone();
        duplicates.append(ACCESS_HEADER, headers[ACCESS_HEADER].clone());
        assert_eq!(
            serve(host, &raw, &Method::GET, &duplicates).await.status(),
            StatusCode::UNAUTHORIZED
        );
        duplicates = headers.clone();
        duplicates.append(SESSION_HEADER, headers[SESSION_HEADER].clone());
        assert_eq!(
            serve(host, &raw, &Method::GET, &duplicates).await.status(),
            StatusCode::BAD_REQUEST
        );
        let Source::Http {
            headers: canvas, ..
        } = host
            .media
            .canvas_source(&binding.occurrence, hint())
            .unwrap()
        else {
            panic!("canvas source")
        };
        let canvas_ticket = canvas
            .iter()
            .find_map(|value| value.strip_prefix("x-ersatzrs-next-canvas: "))
            .unwrap();
        headers.insert(SONG_HEADER, canvas_ticket.parse().unwrap());
        assert_eq!(
            serve(host, &raw, &Method::GET, &headers).await.status(),
            StatusCode::UNAUTHORIZED
        );
        assert_eq!(state.pool.size(), 0);
    }
}

#[cfg(test)]
#[path = "next_song_background_tests.rs"]
mod runtime_tests;
