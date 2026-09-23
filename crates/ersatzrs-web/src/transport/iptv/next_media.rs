//! Capability-protected media transport for the optional Next engine.

use axum::{
    body::Body,
    extract::{Path, State},
    http::{header, HeaderMap, Method, StatusCode},
    response::{IntoResponse, Response},
};
use ersatzrs_infra::{
    db::DbPool,
    next_engine::media::{MediaRelay, ACCESS_HEADER, SESSION_HEADER},
};
use futures_util::TryStreamExt as _;
use tokio::io::{AsyncRead, AsyncReadExt as _};

use crate::host::state::AppState;

pub async fn get_media(
    State(state): State<AppState>,
    Path(binding): Path<(i64, i64, i64)>,
    headers: HeaderMap,
) -> Response {
    serve(&state.next_engine.media, &state.pool, binding, &headers).await
}

/// One GET consumes one producer; HEAD and seeks must not claim its reader.
pub async fn get_prepared(
    State(state): State<AppState>,
    Path((instance, transfer)): Path<(uuid::Uuid, uuid::Uuid)>,
    method: Method,
    headers: HeaderMap,
) -> Response {
    if !headers
        .get(ACCESS_HEADER)
        .and_then(|value| value.to_str().ok())
        .is_some_and(|value| state.next_engine.media.authorized(value))
    {
        return StatusCode::UNAUTHORIZED.into_response();
    }
    if let Err(status) = prepared_request(&method, &headers) {
        return (status, [(header::ALLOW, "GET")]).into_response();
    }
    match state.next_engine.claim_prepared(instance, transfer).await {
        Ok(reader) => prepared_response(reader),
        Err(_) => StatusCode::GONE.into_response(),
    }
}

fn prepared_request(method: &Method, headers: &HeaderMap) -> Result<(), StatusCode> {
    if method != Method::GET {
        return Err(StatusCode::METHOD_NOT_ALLOWED);
    }
    let mut ranges = headers.get_all(header::RANGE).iter();
    // FFmpeg can send an initial bytes=0- request even for non-seekable inputs.
    // Respond 200 with no length/range promises; every real seek is refused.
    if let Some(range) = ranges.next() {
        if !range
            .to_str()
            .is_ok_and(|value| value.trim().eq_ignore_ascii_case("bytes=0-"))
            || ranges.next().is_some()
        {
            return Err(StatusCode::RANGE_NOT_SATISFIABLE);
        }
    }
    Ok(())
}

fn prepared_response(reader: impl AsyncRead + Unpin + Send + 'static) -> Response {
    let chunks = futures_util::stream::try_unfold(reader, |mut reader| async move {
        let mut bytes = vec![0; 64 * 1024];
        let length = reader.read(&mut bytes).await?;
        if length == 0 {
            return Ok::<_, std::io::Error>(None);
        }
        bytes.truncate(length);
        Ok(Some((bytes, reader)))
    });
    (
        [
            (header::CONTENT_TYPE, "application/octet-stream"),
            (header::CACHE_CONTROL, "no-store"),
        ],
        Body::from_stream(chunks),
    )
        .into_response()
}

/// Next sends the requested programme time in authenticated internal headers.
pub async fn get_item(
    State(state): State<AppState>,
    Path((channel, item, digest, audio)): Path<(i64, i64, String, String)>,
    headers: HeaderMap,
) -> Response {
    if !headers
        .get(ACCESS_HEADER)
        .and_then(|value| value.to_str().ok())
        .is_some_and(|value| state.next_engine.media.authorized(value))
    {
        return StatusCode::UNAUTHORIZED.into_response();
    }
    let Some((number, instance, now, until)) = dynamic_headers(&headers) else {
        return StatusCode::BAD_REQUEST.into_response();
    };
    let audio = if audio == "off" {
        None
    } else {
        let Ok(index) = audio.parse::<u32>() else {
            return StatusCode::BAD_REQUEST.into_response();
        };
        Some(index)
    };
    match state
        .next_engine
        .resolve_dynamic(channel, item, &digest, audio, number, instance, now, until)
        .await
    {
        Ok(item) => ([(header::CACHE_CONTROL, "no-store")], axum::Json(item)).into_response(),
        Err(_) => StatusCode::SERVICE_UNAVAILABLE.into_response(),
    }
}

fn dynamic_headers(
    headers: &HeaderMap,
) -> Option<(
    &str,
    uuid::Uuid,
    chrono::DateTime<chrono::Utc>,
    chrono::DateTime<chrono::Utc>,
)> {
    let number = headers.get("x-etv-channel")?.to_str().ok()?;
    let instance = uuid::Uuid::parse_str(headers.get(SESSION_HEADER)?.to_str().ok()?).ok()?;
    let now = chrono::DateTime::parse_from_rfc3339(headers.get("x-etv-now")?.to_str().ok()?)
        .ok()?
        .with_timezone(&chrono::Utc);
    let until = chrono::DateTime::parse_from_rfc3339(headers.get("x-etv-until")?.to_str().ok()?)
        .ok()?
        .with_timezone(&chrono::Utc);
    (until > now).then_some((number, instance, now, until))
}

async fn serve(
    relay: &MediaRelay,
    pool: &DbPool,
    binding: (i64, i64, i64),
    headers: &HeaderMap,
) -> Response {
    if !headers
        .get(ACCESS_HEADER)
        .and_then(|value| value.to_str().ok())
        .is_some_and(|value| relay.authorized(value))
    {
        return StatusCode::UNAUTHORIZED.into_response();
    }
    let Ok(range) = headers
        .get(header::RANGE)
        .map(|value| value.to_str())
        .transpose()
    else {
        return StatusCode::BAD_REQUEST.into_response();
    };
    let Ok(upstream) = relay.open(pool, binding, range).await else {
        return StatusCode::SERVICE_UNAVAILABLE.into_response();
    };
    relay_response(upstream)
}

fn relay_response(upstream: reqwest::Response) -> Response {
    let mut response = Response::new(Body::empty());
    *response.status_mut() = upstream.status();
    for name in [
        header::CONTENT_TYPE,
        header::CONTENT_LENGTH,
        header::CONTENT_RANGE,
        header::ACCEPT_RANGES,
    ] {
        if let Some(value) = upstream.headers().get(&name) {
            response.headers_mut().insert(name, value.clone());
        }
    }
    response.headers_mut().insert(
        header::CACHE_CONTROL,
        "no-store".parse().expect("static header"),
    );
    *response.body_mut() = Body::from_stream(
        upstream
            .bytes_stream()
            .map_err(|_| std::io::Error::other("Next media read failed")),
    );
    response
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn prepared_requests_refuse_head_and_seeks_before_claiming() {
        let mut headers = HeaderMap::new();
        assert_eq!(prepared_request(&Method::GET, &headers), Ok(()));
        assert_eq!(
            prepared_request(&Method::HEAD, &headers),
            Err(StatusCode::METHOD_NOT_ALLOWED)
        );
        headers.insert(header::RANGE, "bytes=0-".parse().unwrap());
        assert_eq!(prepared_request(&Method::GET, &headers), Ok(()));
        for range in ["bytes=1-", "bytes=0-99", "bytes=-10", "bytes=0-,100-"] {
            headers.insert(header::RANGE, range.parse().unwrap());
            assert_eq!(
                prepared_request(&Method::GET, &headers),
                Err(StatusCode::RANGE_NOT_SATISFIABLE)
            );
        }
        headers.insert(header::RANGE, "bytes=0-".parse().unwrap());
        headers.append(header::RANGE, "bytes=0-".parse().unwrap());
        assert_eq!(
            prepared_request(&Method::GET, &headers),
            Err(StatusCode::RANGE_NOT_SATISFIABLE)
        );
    }

    #[tokio::test]
    async fn prepared_response_preserves_bytes_without_seek_or_cache_promises() {
        let bytes = vec![42u8; 150_000];
        let response = prepared_response(std::io::Cursor::new(bytes.clone()));
        assert_eq!(response.status(), StatusCode::OK);
        assert_eq!(response.headers()[header::CACHE_CONTROL], "no-store");
        assert!(!response.headers().contains_key(header::CONTENT_LENGTH));
        assert!(!response.headers().contains_key(header::ACCEPT_RANGES));
        assert_eq!(
            axum::body::to_bytes(response.into_body(), bytes.len())
                .await
                .unwrap()
                .as_ref(),
            bytes
        );
    }

    #[tokio::test]
    async fn prepared_response_propagates_producer_failure() {
        let reader = tokio_util::io::StreamReader::new(futures_util::stream::iter([
            Ok::<_, std::io::Error>(axum::body::Bytes::from_static(b"prefix")),
            Err(std::io::Error::other("synthetic producer failure")),
        ]));
        assert!(
            axum::body::to_bytes(prepared_response(reader).into_body(), 1024)
                .await
                .is_err()
        );
    }

    #[test]
    fn dynamic_callback_headers_require_a_valid_positive_interval() {
        let mut headers = HeaderMap::new();
        assert!(dynamic_headers(&headers).is_none());
        headers.insert("x-etv-channel", "preview-1".parse().unwrap());
        headers.insert("x-etv-now", "2026-09-23T12:00:00Z".parse().unwrap());
        headers.insert("x-etv-until", "2026-09-23T12:01:00Z".parse().unwrap());
        assert!(
            dynamic_headers(&headers).is_none(),
            "legacy callbacks lack process identity"
        );
        headers.insert(SESSION_HEADER, "invalid".parse().unwrap());
        assert!(dynamic_headers(&headers).is_none());
        let instance = uuid::Uuid::new_v4();
        headers.insert(SESSION_HEADER, instance.to_string().parse().unwrap());
        let (channel, actual_instance, start, finish) = dynamic_headers(&headers).unwrap();
        assert_eq!(actual_instance, instance);
        assert_eq!(channel, "preview-1");
        assert_eq!((finish - start).num_seconds(), 60);
        headers.insert("x-etv-until", "2026-09-23T12:00:00Z".parse().unwrap());
        assert!(dynamic_headers(&headers).is_none());
        headers.insert("x-etv-now", "invalid".parse().unwrap());
        assert!(dynamic_headers(&headers).is_none());
    }

    #[tokio::test]
    async fn range_responses_preserve_media_headers_without_forwarding_provider_headers() {
        for (status, range, body) in [(206, "bytes 2-4/6", "cde"), (416, "bytes */6", "")] {
            let upstream = axum::http::Response::builder()
                .status(status)
                .header(header::CONTENT_TYPE, "video/x-matroska")
                .header(header::CONTENT_RANGE, range)
                .header(header::CONTENT_LENGTH, body.len())
                .header(header::ACCEPT_RANGES, "bytes")
                .header(header::SET_COOKIE, "synthetic-placeholder")
                .header("x-provider-private", "synthetic-placeholder")
                .body(body)
                .unwrap();
            let response = relay_response(upstream.into());
            assert_eq!(response.status().as_u16(), status);
            assert_eq!(response.headers()[header::CONTENT_RANGE], range);
            assert_eq!(
                response.headers()[header::CONTENT_LENGTH],
                body.len().to_string()
            );
            assert_eq!(response.headers()[header::CACHE_CONTROL], "no-store");
            assert!(!response.headers().contains_key(header::SET_COOKIE));
            assert!(!response.headers().contains_key("x-provider-private"));
            assert_eq!(
                axum::body::to_bytes(response.into_body(), 10)
                    .await
                    .unwrap()
                    .as_ref(),
                body.as_bytes()
            );
        }
    }

    #[tokio::test]
    async fn unauthorized_requests_do_not_open_the_database() {
        let pool = sqlx::sqlite::SqlitePoolOptions::new()
            .connect_lazy("sqlite::memory:")
            .unwrap();
        pool.close().await;
        let relay = MediaRelay::new();
        let mut headers = HeaderMap::new();
        assert_eq!(
            serve(&relay, &pool, (10, 100, 1), &headers).await.status(),
            StatusCode::UNAUTHORIZED
        );
        headers.insert(ACCESS_HEADER, "invalid".parse().unwrap());
        assert_eq!(
            serve(&relay, &pool, (10, 100, 1), &headers).await.status(),
            StatusCode::UNAUTHORIZED
        );
    }
}

#[cfg(test)]
#[path = "next_prepared_runtime_tests.rs"]
mod prepared_runtime_tests;

// Installed POSIX fixture add-on; native Windows behavior remains covered by portable route tests.
#[cfg(all(test, unix))]
#[path = "next_routed_runtime_tests.rs"]
mod routed_runtime_tests;
