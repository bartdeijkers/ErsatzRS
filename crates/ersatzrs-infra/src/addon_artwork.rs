//! Shared safety rules for operator- and provider-supplied add-on artwork.

use std::time::Duration;
use std::{io::Write as _, path::Path};

use futures_util::StreamExt as _;
use reqwest::{header::CONTENT_TYPE, redirect::Policy, Client};

const ARTWORK_USER_AGENT: &str = concat!("ErsatzRS/", env!("CARGO_PKG_VERSION"));

/// Maximum accepted size for one artwork file.
pub const MAX_ARTWORK_BYTES: usize = 10 * 1024 * 1024;
/// Opaque URL scheme used inside metadata drafts for database-backed uploads.
pub const UPLOAD_URL_PREFIX: &str = "ersatzrs-upload://";

/// Build the no-redirect, short-timeout client used by unattended artwork
/// publication.
///
/// # Errors
///
/// Returns an error when the HTTP client cannot be constructed.
pub fn client() -> Result<Client, String> {
    Client::builder()
        .user_agent(ARTWORK_USER_AGENT)
        .timeout(Duration::from_secs(10))
        .redirect(Policy::none())
        .build()
        .map_err(|error| format!("create artwork client: {error}"))
}

/// Download one already-authorized URL with strict response and byte bounds.
///
/// # Errors
///
/// Returns an error when the request fails, the response is unsuccessful or
/// too large, or the payload is not a supported image matching its MIME type.
pub async fn download(client: &Client, url: &str) -> Result<(Vec<u8>, &'static str), String> {
    let response = client
        .get(url)
        .send()
        .await
        .map_err(|error| format!("request failed: {error}"))?;
    if !response.status().is_success() {
        return Err(format!("server returned HTTP {}", response.status()));
    }
    if response
        .content_length()
        .is_some_and(|length| length > MAX_ARTWORK_BYTES as u64)
    {
        return Err(format!("content length exceeds {MAX_ARTWORK_BYTES} bytes"));
    }
    let content_type = response
        .headers()
        .get(CONTENT_TYPE)
        .and_then(|value| value.to_str().ok())
        .map(|value| {
            value
                .split(';')
                .next()
                .unwrap_or_default()
                .trim()
                .to_owned()
        })
        .ok_or_else(|| "response has no valid image content type".to_owned())?;
    let mut stream = response.bytes_stream();
    let mut bytes = Vec::new();
    while let Some(chunk) = stream.next().await {
        let chunk = chunk.map_err(|error| format!("read failed: {error}"))?;
        if bytes.len().saturating_add(chunk.len()) > MAX_ARTWORK_BYTES {
            return Err(format!("response exceeds {MAX_ARTWORK_BYTES} bytes"));
        }
        bytes.extend_from_slice(&chunk);
    }
    let (_, extension) = validate_image(&content_type, &bytes)?;
    Ok((bytes, extension))
}

/// Atomically replace one materialized image in its existing directory.
///
/// # Errors
///
/// Returns an error when the staging file cannot be created, written, flushed,
/// or persisted at `path`.
pub fn write_atomically(path: &Path, bytes: &[u8]) -> Result<(), String> {
    if std::fs::read(path).is_ok_and(|current| current == bytes) {
        return Ok(());
    }
    let parent = path.parent().unwrap_or_else(|| Path::new("."));
    let mut temporary = tempfile::NamedTempFile::new_in(parent)
        .map_err(|error| format!("create artwork staging file: {error}"))?;
    temporary
        .write_all(bytes)
        .map_err(|error| format!("write artwork staging file: {error}"))?;
    temporary
        .flush()
        .map_err(|error| format!("flush artwork staging file: {error}"))?;
    temporary
        .persist(path)
        .map_err(|error| format!("publish artwork file: {error}"))?;
    Ok(())
}

/// Validate a bounded JPEG, PNG, or WebP payload and return its canonical MIME
/// type and extension.
///
/// The declared MIME type and the file signature must agree. This keeps an
/// uploaded HTML or executable payload from being written under an image
/// extension during publication.
///
/// # Errors
///
/// Returns an error when the payload is empty or oversized, has an unsupported
/// signature, or does not match the declared MIME type.
pub fn validate_image(
    content_type: &str,
    bytes: &[u8],
) -> Result<(&'static str, &'static str), String> {
    if bytes.is_empty() {
        return Err("the artwork file is empty".to_owned());
    }
    if bytes.len() > MAX_ARTWORK_BYTES {
        return Err(format!(
            "the artwork file exceeds the {MAX_ARTWORK_BYTES} byte limit"
        ));
    }
    let declared = content_type
        .split(';')
        .next()
        .unwrap_or_default()
        .trim()
        .to_ascii_lowercase();
    let detected = if bytes.starts_with(&[0xff, 0xd8, 0xff]) {
        ("image/jpeg", "jpg")
    } else if bytes.starts_with(b"\x89PNG\r\n\x1a\n") {
        ("image/png", "png")
    } else if bytes.len() >= 12 && bytes.starts_with(b"RIFF") && &bytes[8..12] == b"WEBP" {
        ("image/webp", "webp")
    } else {
        return Err("the artwork payload is not a supported JPEG, PNG, or WebP image".to_owned());
    };
    if declared != detected.0 {
        return Err(format!(
            "the declared content type {declared} does not match the image payload"
        ));
    }
    Ok(detected)
}

/// Return the upload row identity from one opaque draft URL.
#[must_use]
pub fn upload_id(value: &str) -> Option<i64> {
    value
        .strip_prefix(UPLOAD_URL_PREFIX)?
        .parse::<i64>()
        .ok()
        .filter(|id| *id > 0)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn downloader_identifies_ersatzrs_to_artwork_hosts() {
        use httpmock::{Method::GET, MockServer};

        let server = MockServer::start_async().await;
        let image = b"\xff\xd8\xfffixture jpeg bytes";
        let request = server
            .mock_async(|when, then| {
                when.method(GET)
                    .path("/episode.jpg")
                    .header("user-agent", ARTWORK_USER_AGENT);
                then.status(200)
                    .header("content-type", "image/jpeg")
                    .body(image);
            })
            .await;

        let client = client().expect("artwork client");
        let (downloaded, extension) =
            download(&client, &format!("{}/episode.jpg", server.base_url()))
                .await
                .expect("identified artwork download");

        assert_eq!(downloaded, image);
        assert_eq!(extension, "jpg");
        request.assert_calls_async(1).await;
    }

    #[test]
    fn validates_declared_image_signature_and_bounds() {
        let mut png = b"\x89PNG\r\n\x1a\n".to_vec();
        png.extend_from_slice(b"fixture");
        assert_eq!(
            validate_image("image/png; charset=binary", &png),
            Ok(("image/png", "png"))
        );
        assert!(validate_image("image/jpeg", &png).is_err());
        assert!(validate_image("text/html", b"<html>").is_err());
        assert!(validate_image("image/png", &vec![0; MAX_ARTWORK_BYTES + 1]).is_err());
    }

    #[test]
    fn upload_urls_are_opaque_positive_identities() {
        assert_eq!(upload_id("ersatzrs-upload://42"), Some(42));
        assert_eq!(upload_id("ersatzrs-upload://0"), None);
        assert_eq!(upload_id("https://example.test/42"), None);
    }
}
