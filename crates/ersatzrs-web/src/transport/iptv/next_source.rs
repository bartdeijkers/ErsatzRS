//! Capability-protected, seekable access to one session-owned source snapshot.

use axum::{
    extract::{Path, State},
    http::{header, HeaderMap, Method, StatusCode},
    response::{IntoResponse, Response},
};
use ersatzrs_infra::next_engine::{host::NextHost, media::ACCESS_HEADER};

use crate::host::state::AppState;

pub async fn get_source(
    State(state): State<AppState>,
    Path((instance, source)): Path<(uuid::Uuid, uuid::Uuid)>,
    method: Method,
    headers: HeaderMap,
) -> Response {
    serve(&state.next_engine, instance, source, &method, &headers).await
}

async fn serve(
    host: &NextHost,
    instance: uuid::Uuid,
    source: uuid::Uuid,
    method: &Method,
    headers: &HeaderMap,
) -> Response {
    let mut access = headers.get_all(ACCESS_HEADER).iter();
    if !access
        .next()
        .and_then(|value| value.to_str().ok())
        .is_some_and(|value| host.media.authorized(value))
        || access.next().is_some()
    {
        return empty(StatusCode::UNAUTHORIZED);
    }
    if !matches!(*method, Method::GET | Method::HEAD) {
        return (
            StatusCode::METHOD_NOT_ALLOWED,
            [
                (header::ALLOW, "GET, HEAD"),
                (header::CACHE_CONTROL, "no-store"),
            ],
        )
            .into_response();
    }
    let Ok(total) = host.retained_source_length(instance, source).await else {
        return empty(StatusCode::GONE);
    };
    let Ok(range) = select_range(headers, total) else {
        return unsatisfied(total);
    };
    let response = if method == Method::HEAD {
        StatusCode::OK.into_response()
    } else {
        let Ok(reader) = host
            .open_retained_source(instance, source, range.offset, range.length)
            .await
        else {
            return empty(StatusCode::GONE);
        };
        super::next_media::prepared_response(reader)
    };
    source_response(response, range, total)
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct SelectedRange {
    offset: u64,
    length: u64,
    partial: bool,
}

fn select_range(headers: &HeaderMap, total: u64) -> Result<SelectedRange, ()> {
    let mut ranges = headers.get_all(header::RANGE).iter();
    let Some(value) = ranges.next() else {
        return Ok(SelectedRange {
            offset: 0,
            length: total,
            partial: false,
        });
    };
    if ranges.next().is_some() || total == 0 {
        return Err(());
    }
    let (unit, range) = value
        .to_str()
        .map_err(|_| ())?
        .trim()
        .split_once('=')
        .ok_or(())?;
    if !unit.eq_ignore_ascii_case("bytes") {
        return Err(());
    }
    let (first, last) = range.split_once('-').ok_or(())?;
    let (offset, end) = if first.is_empty() {
        let suffix = decimal(last)?;
        if suffix == 0 {
            return Err(());
        }
        (total.saturating_sub(suffix), total - 1)
    } else {
        let offset = decimal(first)?;
        let end = if last.is_empty() {
            total - 1
        } else {
            decimal(last)?.min(total - 1)
        };
        if offset >= total || end < offset {
            return Err(());
        }
        (offset, end)
    };
    Ok(SelectedRange {
        offset,
        length: end - offset + 1,
        partial: true,
    })
}

fn decimal(value: &str) -> Result<u64, ()> {
    if value.is_empty() || !value.bytes().all(|byte| byte.is_ascii_digit()) {
        return Err(());
    }
    value.parse().map_err(|_| ())
}

fn source_response(response: Response, range: SelectedRange, total: u64) -> Response {
    let mut builder = Response::builder()
        .status(if range.partial {
            StatusCode::PARTIAL_CONTENT
        } else {
            StatusCode::OK
        })
        .header(header::CONTENT_TYPE, "application/octet-stream")
        .header(header::CONTENT_LENGTH, range.length)
        .header(header::ACCEPT_RANGES, "bytes")
        .header(header::CACHE_CONTROL, "no-store");
    if range.partial {
        builder = builder.header(
            header::CONTENT_RANGE,
            format!(
                "bytes {}-{}/{total}",
                range.offset,
                range.offset + range.length - 1
            ),
        );
    }
    builder
        .body(response.into_body())
        .unwrap_or_else(|_| empty(StatusCode::INTERNAL_SERVER_ERROR))
}

fn empty(status: StatusCode) -> Response {
    (status, [(header::CACHE_CONTROL, "no-store")]).into_response()
}

fn unsatisfied(total: u64) -> Response {
    Response::builder()
        .status(StatusCode::RANGE_NOT_SATISFIABLE)
        .header(header::CONTENT_RANGE, format!("bytes */{total}"))
        .header(header::ACCEPT_RANGES, "bytes")
        .header(header::CONTENT_LENGTH, 0)
        .header(header::CACHE_CONTROL, "no-store")
        .body(axum::body::Body::empty())
        .unwrap_or_else(|_| empty(StatusCode::INTERNAL_SERVER_ERROR))
}

#[cfg(test)]
mod tests;
