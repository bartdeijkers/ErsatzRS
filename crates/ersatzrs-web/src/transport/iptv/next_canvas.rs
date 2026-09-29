//! Private Next canvas boundary; upstream `InternalController.GetGraphicsCanvas`.

use axum::http::{HeaderMap, StatusCode};
use ersatzrs_infra::next_engine::canvas::CanvasRequest;

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
