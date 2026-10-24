//! Private Next canvas boundary; upstream `InternalController.GetGraphicsCanvas`.

use axum::{
    extract::{Query, State},
    http::{header, HeaderMap, Method, StatusCode},
    response::{IntoResponse, Response},
};
use ersatzrs_infra::next_engine::{
    canvas::{CanvasBinding, CanvasRequest},
    host::NextHost,
    media::{ACCESS_HEADER, CANVAS_HEADER, SESSION_HEADER},
};

#[derive(serde::Deserialize)]
pub struct CanvasQuery {
    binding: String,
}

pub async fn get_canvas(
    State(state): State<crate::host::state::AppState>,
    Query(query): Query<CanvasQuery>,
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
    let Ok(ticket) = single(headers, CANVAS_HEADER) else {
        return StatusCode::UNAUTHORIZED.into_response();
    };
    if raw_binding.len() > 2048 {
        return StatusCode::BAD_REQUEST.into_response();
    }
    let Ok(binding) = serde_json::from_str::<CanvasBinding>(raw_binding) else {
        return StatusCode::BAD_REQUEST.into_response();
    };
    if !host
        .media
        .authorized_canvas(&binding, ticket, chrono::Utc::now())
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
            Ok(channel) => channel,
            Err(status) => return status.into_response(),
        }
    } else {
        ""
    };
    let timing = match parse_request(headers, channel) {
        Ok(timing) => timing,
        Err(status) => return status.into_response(),
    };
    let Ok(session) = host.canvas_session(instance, &binding, &timing).await else {
        return StatusCode::GONE.into_response();
    };
    if let Err(status) = parse_request(headers, &session.request.number) {
        return status.into_response();
    }
    match host.render_canvas(instance, &binding, &timing).await {
        Ok(reader) => super::next_media::prepared_response(reader),
        Err(_) => StatusCode::SERVICE_UNAVAILABLE.into_response(),
    }
}

/// Validate the exact Next canvas headers before admitting any rendering work.
///
/// # Errors
/// Returns HTTP 400 for missing/duplicate timing headers or a channel mismatch.
pub fn parse_request(headers: &HeaderMap, channel: &str) -> Result<CanvasRequest, StatusCode> {
    if headers.contains_key("x-etv-channel") && single(headers, "x-etv-channel")? != channel {
        return Err(StatusCode::BAD_REQUEST);
    }
    CanvasRequest::parse(
        single(headers, "x-etv-offset-ms")?,
        single(headers, "x-etv-duration-ms")?,
        single(headers, "x-etv-frame-rate")?,
    )
    .map_err(|_| StatusCode::BAD_REQUEST)
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
    use axum::http::HeaderValue;

    #[tokio::test]
    async fn canvas_boundary_rejects_invalid_admission_before_rendering() {
        use ersatzrs_infra::next_engine::protocol::{ProbeHint, Source};
        let state = crate::host::state::AppState::for_tests(
            crate::host::state::IptvAccessTokenVerifier::disabled(),
        );
        let host = &state.next_engine;
        host.media
            .configure("127.0.0.1:8409".parse().unwrap())
            .unwrap();
        let start = chrono::Utc::now();
        let binding = CanvasBinding {
            playout_start: None,
            region: None,
            video_stream_index: None,
            session: Some(uuid::Uuid::new_v4()),
            channel: 1,
            item: 2,
            media: 3,
            version: 4,
            file: 5,
            start,
            finish: start + chrono::TimeDelta::minutes(1),
            in_point_ms: 0,
            out_point_ms: 60_000,
            expires: start + chrono::TimeDelta::minutes(2),
        };
        let Source::Http {
            headers: signed, ..
        } = host
            .media
            .canvas_source(
                &binding,
                ProbeHint {
                    single_use: None,
                    video: Vec::new(),
                    audio: Vec::new(),
                    subtitle: Vec::new(),
                    format_name: Some("nut".into()),
                    duration_ms: Some(60_000),
                },
            )
            .unwrap()
        else {
            panic!("canvas requires HTTP");
        };
        let mut valid = headers();
        for line in signed {
            let (name, value) = line.split_once(": ").unwrap();
            valid.insert(
                name.parse::<axum::http::HeaderName>().unwrap(),
                value.parse().unwrap(),
            );
        }
        let raw = serde_json::to_string(&binding).unwrap();
        for name in [ACCESS_HEADER, CANVAS_HEADER, SESSION_HEADER] {
            let mut duplicate = valid.clone();
            duplicate.append(name, duplicate[name].clone());
            let response = serve(host, &raw, &Method::GET, &duplicate).await;
            assert_eq!(
                response.status(),
                if name == SESSION_HEADER {
                    StatusCode::BAD_REQUEST
                } else {
                    StatusCode::UNAUTHORIZED
                }
            );
        }
        assert_eq!(
            serve(host, &raw, &Method::HEAD, &valid).await.status(),
            StatusCode::METHOD_NOT_ALLOWED
        );
        let mut range = valid.clone();
        range.insert(header::RANGE, HeaderValue::from_static("bytes=1-"));
        assert_eq!(
            serve(host, &raw, &Method::GET, &range).await.status(),
            StatusCode::RANGE_NOT_SATISFIABLE
        );
        let mut changed = binding.clone();
        changed.channel += 1;
        assert_eq!(
            serve(
                host,
                &serde_json::to_string(&changed).unwrap(),
                &Method::GET,
                &valid
            )
            .await
            .status(),
            StatusCode::UNAUTHORIZED
        );
        assert_eq!(
            serve(host, &raw, &Method::GET, &valid).await.status(),
            StatusCode::GONE
        );
    }

    fn headers() -> HeaderMap {
        HeaderMap::from_iter([
            (
                "x-etv-offset-ms".parse().unwrap(),
                HeaderValue::from_static("0"),
            ),
            (
                "x-etv-duration-ms".parse().unwrap(),
                HeaderValue::from_static("44000"),
            ),
            (
                "x-etv-frame-rate".parse().unwrap(),
                HeaderValue::from_static("24000/1001"),
            ),
        ])
    }

    #[test]
    fn canvas_headers_reject_duplicates_missing_values_and_channel_mismatch() {
        for name in ["x-etv-offset-ms", "x-etv-duration-ms", "x-etv-frame-rate"] {
            let mut missing = headers();
            missing.remove(name);
            assert_eq!(parse_request(&missing, "1"), Err(StatusCode::BAD_REQUEST));
            let mut duplicate = headers();
            duplicate.append(name, duplicate[name].clone());
            assert_eq!(parse_request(&duplicate, "1"), Err(StatusCode::BAD_REQUEST));
        }
        let mut request = headers();
        assert!(parse_request(&request, "1").is_ok());
        request.insert("x-etv-channel", HeaderValue::from_static("1"));
        assert!(parse_request(&request, "1").is_ok());
        assert_eq!(parse_request(&request, "2"), Err(StatusCode::BAD_REQUEST));
        request.append("x-etv-channel", HeaderValue::from_static("1"));
        assert_eq!(parse_request(&request, "1"), Err(StatusCode::BAD_REQUEST));
    }
}
