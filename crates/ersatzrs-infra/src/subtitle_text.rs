//! Bounded, lossless source decoding for ordinary burned text subtitles.
//! Publication-verified artifacts retain their separate strict Unicode path.
//! See ADR 0132; a `.charset` companion or HTTP charset overrides detection.

use std::path::Path;
use std::time::Duration;

use encoding_rs::Encoding;
use sha2::{Digest as _, Sha256};
use tokio::io::{AsyncReadExt as _, AsyncWriteExt as _};

use crate::subtitles::decode_subtitle_text;

pub(crate) const MAX_TEXT_BYTES: usize = 8 * 1024 * 1024;
const MAX_LABEL_BYTES: usize = 128;

/// Safe diagnostic: neither source paths nor HTTP URLs enter error messages.
#[derive(Debug, thiserror::Error)]
pub enum SubtitlePreparationError {
    /// The source or its character-set declaration could not be read.
    #[error("subtitle source could not be read; check availability and permissions")]
    Unavailable,
    /// Source, declaration or decoded text exceeds its bound.
    #[error("subtitle source exceeds the preparation size or time limit")]
    Limit,
    /// The declared charset is unknown, inconsistent or cannot decode the bytes.
    #[error("subtitle character-set declaration is invalid or conflicts with the source")]
    Declaration,
    /// Automatic detection cannot safely prepare this input.
    #[error(
        "subtitle encoding is uncertain or invalid; convert it to UTF-8 or declare its charset"
    )]
    Encoding,
    /// Prepared output could not be retained safely.
    #[error("subtitle cache could not be written; check permissions and free disk space")]
    Cache,
    /// Neither cue-aware parsing nor the normalized `FFmpeg` fallback succeeded.
    #[error("subtitle cues could not be prepared; validate the subtitle timing and syntax")]
    Cues,
}

pub(crate) struct SubtitleText {
    pub(crate) text: String,
    pub(crate) fingerprint: String,
}

/// Local file reads are capped even when a source grows after metadata lookup.
pub(crate) async fn read_bounded(
    path: &Path,
    limit: usize,
) -> Result<Vec<u8>, SubtitlePreparationError> {
    let file = tokio::fs::File::open(path)
        .await
        .map_err(|_| SubtitlePreparationError::Unavailable)?;
    if !file
        .metadata()
        .await
        .map_err(|_| SubtitlePreparationError::Unavailable)?
        .is_file()
    {
        return Err(SubtitlePreparationError::Unavailable);
    }
    let mut bytes = Vec::new();
    file.take((limit + 1) as u64)
        .read_to_end(&mut bytes)
        .await
        .map_err(|_| SubtitlePreparationError::Unavailable)?;
    if bytes.len() > limit {
        return Err(SubtitlePreparationError::Limit);
    }
    Ok(bytes)
}

async fn local_label(path: &str) -> Result<Option<String>, SubtitlePreparationError> {
    let companion = format!("{path}.charset");
    match tokio::fs::metadata(&companion).await {
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(None),
        Err(_) => Err(SubtitlePreparationError::Unavailable),
        Ok(_) => String::from_utf8(read_bounded(Path::new(&companion), MAX_LABEL_BYTES).await?)
            .map(|label| Some(label.trim_start_matches('\u{feff}').to_owned()))
            .map_err(|_| SubtitlePreparationError::Declaration),
    }
}

fn http_label(value: &str) -> Result<Option<String>, SubtitlePreparationError> {
    let mut label = None;
    for parameter in value.split(';').skip(1) {
        let Some((name, value)) = parameter.split_once('=') else {
            continue;
        };
        if name.trim().eq_ignore_ascii_case("charset") {
            let value = value.trim().trim_matches('"');
            if label.is_some() || value.is_empty() || value.len() > MAX_LABEL_BYTES {
                return Err(SubtitlePreparationError::Declaration);
            }
            label = Some(value.to_owned());
        }
    }
    Ok(label)
}

pub(crate) async fn read_source(path: &str) -> Result<SubtitleText, SubtitlePreparationError> {
    tokio::time::timeout(Duration::from_secs(20), read_source_inner(path))
        .await
        .map_err(|_| SubtitlePreparationError::Limit)?
}

async fn read_source_inner(path: &str) -> Result<SubtitleText, SubtitlePreparationError> {
    let (bytes, label) = if crate::playback::subtitle_is_http(path) {
        let mut response = reqwest::Client::new()
            .get(path)
            .send()
            .await
            .and_then(reqwest::Response::error_for_status)
            .map_err(|_| SubtitlePreparationError::Unavailable)?;
        let label = response
            .headers()
            .get(reqwest::header::CONTENT_TYPE)
            .map(|value| {
                value
                    .to_str()
                    .map_err(|_| SubtitlePreparationError::Declaration)
                    .and_then(http_label)
            })
            .transpose()?
            .flatten();
        if response
            .content_length()
            .is_some_and(|length| length > MAX_TEXT_BYTES as u64)
        {
            return Err(SubtitlePreparationError::Limit);
        }
        let mut bytes = Vec::new();
        while let Some(chunk) = response
            .chunk()
            .await
            .map_err(|_| SubtitlePreparationError::Unavailable)?
        {
            if chunk.len() > MAX_TEXT_BYTES - bytes.len() {
                return Err(SubtitlePreparationError::Limit);
            }
            bytes.extend_from_slice(&chunk);
        }
        (bytes, label)
    } else {
        (
            read_bounded(Path::new(path), MAX_TEXT_BYTES).await?,
            local_label(path).await?,
        )
    };
    decode_source(&bytes, label.as_deref())
}

pub(crate) fn decode_source(
    bytes: &[u8],
    label: Option<&str>,
) -> Result<SubtitleText, SubtitlePreparationError> {
    if bytes.len() > MAX_TEXT_BYTES {
        return Err(SubtitlePreparationError::Limit);
    }
    let declared = label
        .map(|label| {
            Encoding::for_label_no_replacement(label.trim().as_bytes())
                .ok_or(SubtitlePreparationError::Declaration)
        })
        .transpose()?;
    let (text, encoding) = if let Some((bom, _)) = Encoding::for_bom(bytes) {
        if declared.is_some_and(|encoding| encoding != bom) {
            return Err(SubtitlePreparationError::Declaration);
        }
        (
            decode_subtitle_text(bytes.to_vec()).map_err(|_| SubtitlePreparationError::Encoding)?,
            bom,
        )
    } else if let Some(encoding) = declared {
        (
            decode_strict(bytes, encoding).ok_or(SubtitlePreparationError::Declaration)?,
            encoding,
        )
    } else if let Ok(text) = std::str::from_utf8(bytes) {
        (text.to_owned(), encoding_rs::UTF_8)
    } else {
        // Short or tied guesses are not evidence for a legacy encoding. The
        // detector is probabilistic; an explicit label is always available.
        if bytes.iter().filter(|byte| !byte.is_ascii()).count() < 4 {
            return Err(SubtitlePreparationError::Encoding);
        }
        let mut detector = chardetng::EncodingDetector::new();
        detector.feed(bytes, true);
        let (encoding, assessed) = detector.guess_assess(None, false);
        if !assessed {
            return Err(SubtitlePreparationError::Encoding);
        }
        (
            decode_strict(bytes, encoding).ok_or(SubtitlePreparationError::Encoding)?,
            encoding,
        )
    };
    // Reject binary/control data and ASCII-compatible ISO-2022 escapes unless
    // an explicit ISO-2022-JP declaration has decoded them to actual text.
    if text
        .chars()
        .any(|ch| ch.is_control() && !matches!(ch, '\n' | '\r' | '\t'))
    {
        return Err(SubtitlePreparationError::Encoding);
    }
    if text.len() > MAX_TEXT_BYTES {
        return Err(SubtitlePreparationError::Limit);
    }
    let mut hash = Sha256::new();
    hash.update(b"ersatzrs:subtitle-text:v1\0");
    hash.update(encoding.name().as_bytes());
    hash.update([0]);
    hash.update(bytes);
    let fingerprint = hash
        .finalize()
        .iter()
        .fold(String::new(), |mut output, byte| {
            use std::fmt::Write as _;
            write!(output, "{byte:02x}").expect("string write");
            output
        });
    Ok(SubtitleText { text, fingerprint })
}

fn decode_strict(bytes: &[u8], encoding: &'static Encoding) -> Option<String> {
    encoding
        .decode_without_bom_handling_and_without_replacement(bytes)
        .map(std::borrow::Cow::into_owned)
}

/// Validate the cached bytes and atomically replace any partial/foreign entry.
pub(crate) async fn cache_text(path: &Path, text: &str) -> Result<(), SubtitlePreparationError> {
    if text.len() > MAX_TEXT_BYTES {
        return Err(SubtitlePreparationError::Limit);
    }
    if read_bounded(path, MAX_TEXT_BYTES)
        .await
        .is_ok_and(|bytes| bytes == text.as_bytes())
    {
        return Ok(());
    }
    let parent = path.parent().ok_or(SubtitlePreparationError::Cache)?;
    tokio::fs::create_dir_all(parent)
        .await
        .map_err(|_| SubtitlePreparationError::Cache)?;
    let temporary =
        tempfile::NamedTempFile::new_in(parent).map_err(|_| SubtitlePreparationError::Cache)?;
    let mut file = tokio::fs::File::from_std(
        temporary
            .reopen()
            .map_err(|_| SubtitlePreparationError::Cache)?,
    );
    file.write_all(text.as_bytes())
        .await
        .map_err(|_| SubtitlePreparationError::Cache)?;
    file.flush()
        .await
        .map_err(|_| SubtitlePreparationError::Cache)?;
    drop(file);
    temporary
        .persist(path)
        .map_err(|_| SubtitlePreparationError::Cache)?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn legacy_subtitle_detection_preserves_international_text() {
        for (encoding, line) in [
            (
                encoding_rs::WINDOWS_1252,
                "Une élève prépare déjà un café très agréable.",
            ),
            (
                encoding_rs::WINDOWS_1251,
                "Это русский текст субтитров, который должен остаться читаемым.",
            ),
            (
                encoding_rs::SHIFT_JIS,
                "これは字幕の文字コードを確認するための日本語の文章です。",
            ),
        ] {
            let text = format!("1\n00:00:42,000 --> 00:00:46,000\n{}\n", line.repeat(8));
            let (bytes, _, errors) = encoding.encode(&text);
            assert!(!errors);
            let decoded = decode_source(&bytes, None).unwrap();
            assert_eq!(decoded.text, text, "{}", encoding.name());
        }
    }

    #[test]
    fn explicit_charset_resolves_short_ambiguous_text_without_replacement() {
        let bytes = b"1\n00:00:00,000 --> 00:00:01,000\ncaf\xe9\n";
        assert!(matches!(
            decode_source(bytes, None),
            Err(SubtitlePreparationError::Encoding)
        ));
        assert!(decode_source(bytes, Some("windows-1252"))
            .unwrap()
            .text
            .contains("café"));
        assert!(decode_source(bytes, Some("utf-8")).is_err());
        assert!(decode_source(bytes, Some("unknown")).is_err());
        assert!(decode_source(bytes, Some("replacement")).is_err());
        assert!(decode_source(&[0x82], Some("shift_jis")).is_err());
    }

    #[test]
    fn unicode_boms_and_valid_utf8_are_preserved_and_malformed_unicode_is_refused() {
        // A literal replacement glyph already present in valid Unicode is
        // source text. Strict decoding must never introduce one for bad bytes.
        let text = "1\n00:00:00,000 --> 00:00:01,000\nŻółć 日本語 العربية �\n";
        assert_eq!(decode_source(text.as_bytes(), None).unwrap().text, text);
        for (bom, little_endian) in [([0xff, 0xfe], true), ([0xfe, 0xff], false)] {
            let mut bytes = bom.to_vec();
            for unit in text.encode_utf16() {
                bytes.extend_from_slice(&if little_endian {
                    unit.to_le_bytes()
                } else {
                    unit.to_be_bytes()
                });
            }
            assert_eq!(decode_source(&bytes, None).unwrap().text, text);
            assert!(decode_source(&bytes, Some("windows-1252")).is_err());
        }
        let mut bom_utf8 = vec![0xef, 0xbb, 0xbf];
        bom_utf8.extend_from_slice(text.as_bytes());
        assert_eq!(decode_source(&bom_utf8, None).unwrap().text, text);
        for bytes in [
            vec![0xff, 0xfe, 0x00],
            vec![0xff, 0xfe, 0x00, 0xd8],
            vec![0xef, 0xbb, 0xbf, 0xff],
            b"bad\0data".to_vec(),
            b"\x1b$Bbinary".to_vec(),
        ] {
            assert!(decode_source(&bytes, None).is_err());
        }
    }

    #[test]
    fn cache_identity_includes_bytes_and_resolved_encoding() {
        let bytes = b"caf\xe9";
        let western = decode_source(bytes, Some("windows-1252")).unwrap();
        let cyrillic = decode_source(bytes, Some("windows-1251")).unwrap();
        assert_ne!(western.fingerprint, cyrillic.fingerprint);
        assert_ne!(
            western.fingerprint,
            decode_source(b"changed", None).unwrap().fingerprint
        );
        assert_eq!(
            western.fingerprint,
            decode_source(bytes, Some("latin1")).unwrap().fingerprint
        );
        assert!(decode_source(&vec![b'a'; MAX_TEXT_BYTES + 1], None).is_err());
    }

    #[tokio::test]
    async fn local_override_and_atomic_cache_follow_changes_without_touching_source() {
        let root = tempfile::tempdir().unwrap();
        let source = root.path().join("synthetic.srt");
        let bytes = b"1\n00:00:42,000 --> 00:00:46,000\ncaf\xe9\n";
        tokio::fs::write(&source, bytes).await.unwrap();
        let label = root.path().join("synthetic.srt.charset");
        tokio::fs::write(&label, "\u{feff}windows-1252\n")
            .await
            .unwrap();
        let prepared = read_source(source.to_str().unwrap()).await.unwrap();
        let output = root.path().join("cache/normalized.srt");
        cache_text(&output, &prepared.text).await.unwrap();
        tokio::fs::write(&output, "partial").await.unwrap();
        let writes = (0..8).map(|_| cache_text(&output, &prepared.text));
        for result in futures_util::future::join_all(writes).await {
            result.unwrap();
        }
        assert_eq!(
            tokio::fs::read_to_string(&output).await.unwrap(),
            prepared.text
        );
        tokio::fs::write(&label, "windows-1251").await.unwrap();
        let changed = read_source(source.to_str().unwrap()).await.unwrap();
        assert_ne!(prepared.fingerprint, changed.fingerprint);
        assert_eq!(tokio::fs::read(&source).await.unwrap(), bytes);
        tokio::fs::write(&label, "x".repeat(MAX_LABEL_BYTES + 1))
            .await
            .unwrap();
        assert!(read_source(source.to_str().unwrap()).await.is_err());
    }

    #[tokio::test]
    async fn http_charset_and_size_bounds_are_honored_and_errors_are_redacted() {
        use httpmock::{prelude::GET, MockServer};
        let server = MockServer::start_async().await;
        let mock = server
            .mock_async(|when, then| {
                when.method(GET)
                    .path("/subtitle")
                    .query_param("token", "private-test-token");
                then.status(200)
                    .header("Content-Type", "application/x-subrip; charset=windows-1252")
                    .body(b"1\n00:00:42,000 --> 00:00:46,000\ncaf\xe9\n");
            })
            .await;
        let path = format!("{}/subtitle?token=private-test-token", server.base_url());
        assert!(read_source(&path).await.unwrap().text.contains("café"));
        mock.assert_async().await;
        let oversized = server
            .mock_async(|when, then| {
                when.method(GET).path("/large");
                then.status(200).body(vec![b'a'; MAX_TEXT_BYTES + 1]);
            })
            .await;
        assert!(matches!(
            read_source(&format!("{}/large", server.base_url())).await,
            Err(SubtitlePreparationError::Limit)
        ));
        oversized.assert_async().await;
        let error = read_source(&format!(
            "{}/missing?token=private-test-token",
            server.base_url()
        ))
        .await
        .err()
        .unwrap()
        .to_string();
        assert!(!error.contains("private-test-token"));
        assert!(!error.contains(&server.base_url()));
        assert!(http_label("text/plain; charset=utf-8; charset=windows-1252").is_err());
    }
}
