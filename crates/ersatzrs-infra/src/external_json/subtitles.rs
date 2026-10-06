//! Request-owned subtitle preparation for programmes without library records.

use std::path::PathBuf;

use tokio::{sync::oneshot, time::Instant};

use crate::{
    metadata::subtitle_kind,
    next_engine::{burn_subtitles, protocol::Source},
    playback::PlaybackSubtitle,
};

static EXTRACTIONS: tokio::sync::Semaphore = tokio::sync::Semaphore::const_new(2);

/// Failures never contain source URLs, provider credentials or temporary paths.
#[derive(Debug, thiserror::Error, PartialEq, Eq)]
pub enum PreparationError {
    #[error("transient subtitle binding is unsupported")]
    Unsupported,
    #[error("transient subtitle preparation deadline expired")]
    Deadline,
    #[error("transient subtitle preparation was cancelled")]
    Cancelled,
    #[error("transient subtitle preparation failed")]
    Preparation,
}

/// Owns the prepared file. Playback supervisors must retain this until their
/// consuming `FFmpeg` process has stopped, including disconnect and retry paths.
/// No Debug implementation: paths may be private.
pub struct PreparedSubtitle {
    subtitle: PlaybackSubtitle,
    _directory: tempfile::TempDir,
}

impl PreparedSubtitle {
    #[must_use]
    pub fn subtitle(&self) -> &PlaybackSubtitle {
        &self.subtitle
    }

    /// Download a selected Plex sidecar using the current server credential.
    /// Only same-server `/library/streams/` resources are accepted, redirects are
    /// disabled, and both announced and streamed bodies are limited to 32 MiB.
    /// Dropping the future closes HTTP and releases its temporary directory.
    ///
    /// # Errors
    /// Returns a redacted error for invalid bindings, unavailable sidecars,
    /// oversized or empty responses, and the caller's startup deadline.
    pub async fn fetch_plex(
        pool: &crate::db::DbPool,
        source_id: i64,
        subtitle: PlaybackSubtitle,
        deadline: Instant,
    ) -> Result<Self, PreparationError> {
        tokio::time::timeout_at(deadline, fetch_plex(pool, source_id, subtitle))
            .await
            .map_err(|_| PreparationError::Deadline)?
    }

    /// Extract only the selected embedded text track, preserving timestamps and
    /// ASS styling. No library rows or shared subtitle cache are created.
    ///
    /// Dropping this future signals a bounded worker to kill and reap `FFmpeg`
    /// before releasing the temporary output. Admission consumes the same deadline
    /// as extraction. At most two extractions run simultaneously.
    ///
    /// # Errors
    /// Returns a redacted error for unsupported tracks, expired deadlines or
    /// extraction failures. Process cleanup can take up to five extra seconds.
    pub async fn extract(
        source: Source,
        subtitle: PlaybackSubtitle,
        ffmpeg: PathBuf,
        deadline: Instant,
    ) -> Result<Self, PreparationError> {
        if subtitle.subtitle_kind != subtitle_kind::EMBEDDED
            || subtitle.is_image()
            || subtitle.stream_index < 0
        {
            return Err(PreparationError::Unsupported);
        }
        let (cancel, cancellation) = oneshot::channel();
        let worker = tokio::spawn(prepare(source, subtitle, ffmpeg, deadline, cancellation));
        let result = worker.await.map_err(|_| PreparationError::Preparation);
        // Keep the sender across the await. An earlier request drop closes it
        // while the worker still owns both child and temporary directory.
        drop(cancel);
        result?
    }
}

async fn fetch_plex(
    pool: &crate::db::DbPool,
    source_id: i64,
    subtitle: PlaybackSubtitle,
) -> Result<PreparedSubtitle, PreparationError> {
    const MAX_BYTES: usize = 32 * 1024 * 1024;
    if source_id <= 0 || subtitle.subtitle_kind != subtitle_kind::SIDECAR {
        return Err(PreparationError::Unsupported);
    }
    let permit = EXTRACTIONS
        .acquire()
        .await
        .map_err(|_| PreparationError::Preparation)?;
    let config = crate::plex::load_active_plex_connection_config(pool, source_id)
        .await
        .map_err(|_| PreparationError::Preparation)?;
    let uri = subtitle_uri(&config.uri, subtitle.path.as_deref().unwrap_or_default())?;
    let client = reqwest::Client::builder()
        .redirect(reqwest::redirect::Policy::none())
        .timeout(std::time::Duration::from_secs(15))
        .build()
        .map_err(|_| PreparationError::Preparation)?;
    let mut response = client
        .get(uri)
        .header("X-Plex-Token", &config.token)
        .send()
        .await
        .map_err(|_| PreparationError::Preparation)?;
    if !response.status().is_success()
        || response
            .content_length()
            .is_some_and(|len| len > MAX_BYTES as u64)
    {
        return Err(PreparationError::Preparation);
    }
    let mut bytes = Vec::new();
    while let Some(chunk) = response
        .chunk()
        .await
        .map_err(|_| PreparationError::Preparation)?
    {
        if chunk.len() > MAX_BYTES.saturating_sub(bytes.len()) {
            return Err(PreparationError::Preparation);
        }
        bytes.extend_from_slice(&chunk);
    }
    if bytes.is_empty() {
        return Err(PreparationError::Preparation);
    }
    // A blocking file writer owns its directory and admission permit. If the
    // request disappears, it still finishes closing the file before cleanup.
    tokio::task::spawn_blocking(move || {
        let _permit = permit;
        write_sidecar(subtitle, &bytes)
    })
    .await
    .map_err(|_| PreparationError::Preparation)?
}

fn write_sidecar(
    mut subtitle: PlaybackSubtitle,
    bytes: &[u8],
) -> Result<PreparedSubtitle, PreparationError> {
    let directory = tempfile::Builder::new()
        .prefix("ersatzrs-external-subtitle-")
        .tempdir()
        .map_err(|_| PreparationError::Preparation)?;
    // Preserve a recognized extension for text/bitmap input detection without
    // trusting provider-supplied filenames as local paths.
    let extension = match subtitle
        .codec
        .as_deref()
        .unwrap_or_default()
        .to_ascii_lowercase()
        .as_str()
    {
        "ass" | "ssa" => "ass",
        "webvtt" | "vtt" => "vtt",
        "hdmv_pgs_subtitle" | "pgssub" | "pgs" => "sup",
        "dvd_subtitle" | "dvdsub" | "vobsub" => "sub",
        _ => "srt",
    };
    let path = directory.path().join(format!("selected.{extension}"));
    std::fs::write(&path, bytes).map_err(|_| PreparationError::Preparation)?;
    subtitle.path = Some(
        path.to_str()
            .ok_or(PreparationError::Preparation)?
            .to_owned(),
    );
    subtitle.is_extracted = true;
    Ok(PreparedSubtitle {
        subtitle,
        _directory: directory,
    })
}

fn subtitle_uri(base: &str, key: &str) -> Result<url::Url, PreparationError> {
    let base = url::Url::parse(base).map_err(|_| PreparationError::Unsupported)?;
    if !matches!(base.scheme(), "http" | "https")
        || base.host_str().is_none()
        || !base.username().is_empty()
        || base.password().is_some()
        || base.query().is_some()
        || base.fragment().is_some()
        || !key.starts_with("/library/streams/")
        || key.contains(['\r', '\n', '\\'])
    {
        return Err(PreparationError::Unsupported);
    }
    let uri = base.join(key).map_err(|_| PreparationError::Unsupported)?;
    if uri.origin() != base.origin()
        || !uri.path().starts_with("/library/streams/")
        || uri.path() == "/library/streams/"
        || uri.fragment().is_some()
    {
        return Err(PreparationError::Unsupported);
    }
    Ok(uri)
}

async fn prepare(
    source: Source,
    mut subtitle: PlaybackSubtitle,
    ffmpeg: PathBuf,
    deadline: Instant,
    mut cancellation: oneshot::Receiver<()>,
) -> Result<PreparedSubtitle, PreparationError> {
    let _permit = tokio::select! {
        biased;
        _ = &mut cancellation => return Err(PreparationError::Cancelled),
        () = tokio::time::sleep_until(deadline) => return Err(PreparationError::Deadline),
        permit = EXTRACTIONS.acquire() => permit.map_err(|_| PreparationError::Preparation)?,
    };
    let directory = tempfile::Builder::new()
        .prefix("ersatzrs-external-subtitle-")
        .tempdir()
        .map_err(|_| PreparationError::Preparation)?;
    let codec = subtitle.codec.as_deref().unwrap_or_default();
    let ass = codec.eq_ignore_ascii_case("ass") || codec.eq_ignore_ascii_case("ssa");
    let path = directory
        .path()
        .join(if ass { "selected.ass" } else { "selected.srt" });
    let index = u32::try_from(subtitle.stream_index).map_err(|_| PreparationError::Unsupported)?;
    if let Err(error) = burn_subtitles::extract_cancellable(
        &source,
        index,
        codec,
        &ffmpeg,
        &path,
        deadline,
        cancellation,
    )
    .await
    {
        return Err(match error {
            burn_subtitles::BurnError::Cancelled => PreparationError::Cancelled,
            burn_subtitles::BurnError::Deadline => PreparationError::Deadline,
            burn_subtitles::BurnError::Cleanup => {
                // Never unlink a file whose writer may still own it. Managed
                // process drop retries process-tree cleanup.
                let _retained = directory.keep();
                tracing::warn!("transient subtitle cleanup unconfirmed; temporary output retained");
                PreparationError::Preparation
            }
            _ => PreparationError::Preparation,
        });
    }
    subtitle.path = Some(
        path.to_str()
            .ok_or(PreparationError::Preparation)?
            .to_owned(),
    );
    subtitle.is_extracted = true;
    subtitle.subtitle_kind = subtitle_kind::SIDECAR;
    subtitle.codec = Some(if ass { "ass" } else { "subrip" }.into());
    Ok(PreparedSubtitle {
        subtitle,
        _directory: directory,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn plex_sidecar_uri_cannot_escape_the_streams_endpoint() {
        for key in [
            "https://other.invalid/library/streams/1",
            "//other.invalid/library/streams/1",
            "/library/streams/../parts/1",
            "/library/streams/%2e%2e/parts/1",
            "/library/streams/1#fragment",
            "/library/streams/",
            "/library/streams/1\r\nX: value",
            "/library/streams/..\\parts/1",
        ] {
            assert!(subtitle_uri("http://fixture.invalid:8096", key).is_err());
        }
        let uri = subtitle_uri(
            "http://fixture.invalid:8096",
            "/library/streams/1?encoding=utf-8",
        )
        .unwrap();
        assert_eq!(
            uri.as_str(),
            "http://fixture.invalid:8096/library/streams/1?encoding=utf-8"
        );
        assert!(subtitle_uri(
            "http://fixture.invalid?token=synthetic",
            "/library/streams/1"
        )
        .is_err());
    }
}
