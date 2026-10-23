use super::*;
use axum::{body::to_bytes, http::HeaderValue};

fn headers(range: Option<&str>) -> HeaderMap {
    let mut headers = HeaderMap::new();
    if let Some(range) = range {
        headers.insert(header::RANGE, range.parse().unwrap());
    }
    headers
}

#[test]
fn source_ranges_cover_full_closed_open_suffix_and_clipped_requests() {
    for (value, offset, length, partial) in [
        (None, 0, 10, false),
        (Some("bytes=0-"), 0, 10, true),
        (Some("bytes=3-6"), 3, 4, true),
        (Some("bytes=3-"), 3, 7, true),
        (Some("bytes=9-9"), 9, 1, true),
        (Some("bytes=3-999"), 3, 7, true),
        (Some("bytes=-4"), 6, 4, true),
        (Some("bytes=-999"), 0, 10, true),
        (Some(" bytes=0003-0006 "), 3, 4, true),
    ] {
        assert_eq!(
            select_range(&headers(value), 10),
            Ok(SelectedRange {
                offset,
                length,
                partial
            }),
            "{value:?}",
        );
    }
    assert_eq!(
        select_range(&headers(None), 0),
        Ok(SelectedRange {
            offset: 0,
            length: 0,
            partial: false
        }),
    );
    assert_eq!(
        select_range(&headers(Some("bytes=0-")), u64::MAX),
        Ok(SelectedRange {
            offset: 0,
            length: u64::MAX,
            partial: true
        }),
    );
}

#[test]
fn source_ranges_refuse_duplicate_multipart_invalid_and_unsatisfiable_values() {
    for value in [
        "",
        "bytes=",
        "bytes=-",
        "bytes=-0",
        "bytes=10-",
        "bytes=8-7",
        "bytes=0-1,4-5",
        "bytes=0-,4-",
        "items=0-1",
        "bytes=+1-2",
        "bytes=1-+2",
        "bytes=1 -2",
        "bytes=1- 2",
        "bytes=--1",
        "bytes=0-1-2",
        "bytes=18446744073709551616-",
        "bytes=-18446744073709551616",
        "bytes=0-18446744073709551616",
    ] {
        assert!(
            select_range(&headers(Some(value)), 10).is_err(),
            "{value:?}"
        );
    }
    let mut duplicate = headers(Some("bytes=0-"));
    duplicate.append(header::RANGE, HeaderValue::from_static("bytes=0-"));
    assert!(select_range(&duplicate, 10).is_err());
    let mut non_ascii = HeaderMap::new();
    non_ascii.insert(header::RANGE, HeaderValue::from_bytes(&[0xff]).unwrap());
    assert!(select_range(&non_ascii, 10).is_err());
    assert!(select_range(&headers(Some("bytes=0-")), 0).is_err());
    let rejected = unsatisfied(10);
    assert_eq!(rejected.status(), StatusCode::RANGE_NOT_SATISFIABLE);
    assert_eq!(rejected.headers()[header::CONTENT_RANGE], "bytes */10");
    assert_eq!(rejected.headers()[header::CONTENT_LENGTH], "0");
    assert_eq!(rejected.headers()[header::CACHE_CONTROL], "no-store");
}

#[tokio::test]
async fn source_response_preserves_exact_bytes_and_head_headers_without_a_reader() {
    let range = select_range(&headers(Some("bytes=3-6")), 10).unwrap();
    let response = source_response(
        super::super::next_media::prepared_response(&b"3456"[..]),
        range,
        10,
    );
    assert_eq!(response.status(), StatusCode::PARTIAL_CONTENT);
    assert_eq!(response.headers()[header::CONTENT_LENGTH], "4");
    assert_eq!(response.headers()[header::CONTENT_RANGE], "bytes 3-6/10");
    assert_eq!(response.headers()[header::ACCEPT_RANGES], "bytes");
    assert_eq!(response.headers()[header::CACHE_CONTROL], "no-store");
    assert_eq!(
        to_bytes(response.into_body(), 32).await.unwrap(),
        &b"3456"[..]
    );

    // HEAD constructs only an empty response, carrying the GET metadata.
    let head = source_response(StatusCode::OK.into_response(), range, 10);
    assert_eq!(head.status(), StatusCode::PARTIAL_CONTENT);
    assert_eq!(head.headers()[header::CONTENT_LENGTH], "4");
    assert_eq!(head.headers()[header::CONTENT_RANGE], "bytes 3-6/10");
    assert!(to_bytes(head.into_body(), 32).await.unwrap().is_empty());

    let full = source_response(
        super::super::next_media::prepared_response(&b"0123456789"[..]),
        select_range(&HeaderMap::new(), 10).unwrap(),
        10,
    );
    assert_eq!(full.status(), StatusCode::OK);
    assert_eq!(full.headers()[header::CONTENT_LENGTH], "10");
    assert!(!full.headers().contains_key(header::CONTENT_RANGE));
    assert_eq!(
        to_bytes(full.into_body(), 32).await.unwrap(),
        &b"0123456789"[..]
    );
}

#[tokio::test]
async fn source_admission_rejects_missing_duplicate_or_invalid_capabilities_before_work() {
    use ersatzrs_infra::next_engine::protocol::{ProbeHint, Source};
    let pool = sqlx::sqlite::SqlitePoolOptions::new()
        .connect_lazy("sqlite::memory:")
        .unwrap();
    pool.close().await;
    let directory = tempfile::tempdir().unwrap();
    let host = NextHost::new(
        pool,
        ersatzrs_core::locking::EntityLocker::default(),
        directory.path().join("playout"),
    );
    host.media
        .configure("127.0.0.1:8409".parse().unwrap())
        .unwrap();
    let instance = uuid::Uuid::new_v4();
    let source = uuid::Uuid::new_v4();
    let Source::Http {
        headers: granted, ..
    } = host
        .media
        .prepared_source(
            instance,
            source,
            ProbeHint {
                single_use: None,
                video: Vec::new(),
                audio: Vec::new(),
                subtitle: Vec::new(),
                format_name: None,
                duration_ms: None,
            },
            false,
        )
        .unwrap()
    else {
        panic!("HTTP capability source required")
    };
    let token = granted[0].split_once(": ").unwrap().1;
    let mut valid = HeaderMap::new();
    valid.insert(ACCESS_HEADER, token.parse().unwrap());
    let mut duplicate = valid.clone();
    duplicate.append(ACCESS_HEADER, token.parse().unwrap());
    let mut invalid = HeaderMap::new();
    invalid.insert(ACCESS_HEADER, HeaderValue::from_static("invalid"));
    let mut non_ascii = HeaderMap::new();
    non_ascii.insert(ACCESS_HEADER, HeaderValue::from_bytes(&[0xff]).unwrap());
    for headers in [HeaderMap::new(), duplicate, invalid, non_ascii] {
        let response = serve(&host, instance, source, &Method::GET, &headers).await;
        assert_eq!(response.status(), StatusCode::UNAUTHORIZED);
        assert_eq!(response.headers()[header::CACHE_CONTROL], "no-store");
    }
    let post = serve(&host, instance, source, &Method::POST, &valid).await;
    assert_eq!(post.status(), StatusCode::METHOD_NOT_ALLOWED);
    assert_eq!(post.headers()[header::ALLOW], "GET, HEAD");
    for method in [Method::GET, Method::HEAD] {
        let gone = serve(&host, instance, source, &method, &valid).await;
        assert_eq!(gone.status(), StatusCode::GONE);
        assert_eq!(gone.headers()[header::CACHE_CONTROL], "no-store");
    }
}
