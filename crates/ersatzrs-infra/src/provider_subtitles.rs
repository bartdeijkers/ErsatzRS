//! Provider sidecar identities used by playback, independent of credentials.

use ersatzrs_core::repositories::{RemoteMediaSourceKind, RepositoryError, Result};
use sha2::{Digest as _, Sha256};
use sqlx::Row;
use std::{fmt::Write as _, path::Path};
use tokio::io::{AsyncReadExt as _, AsyncWriteExt as _};
use url::Url;

use crate::{db::DbPool, repositories::SqlxResultExt as _};

#[path = "provider_subtitles_plex.rs"]
mod plex;

const MAX_BYTES: usize = 32 * 1024 * 1024;

/// Bounded provider response ready for a playback-owned subtitle artifact.
pub struct ProviderSubtitleContent {
    pub bytes: Vec<u8>,
    pub extension: &'static str,
}

/// Prepare a provider sidecar as a complete local playback artifact.
/// Existing local/embedded paths are returned unchanged. Stored provider
/// rows retain their original identity; only the returned playback value changes.
///
/// # Errors
/// Fails closed on provider retrieval or atomic artifact publication failure.
pub async fn prepare(
    pool: &DbPool,
    subtitle: &crate::playback::PlaybackSubtitle,
    cache: &Path,
) -> Result<crate::playback::PlaybackSubtitle> {
    let original = binding(pool, subtitle.id).await?;
    if original.as_ref().is_some_and(|binding| {
        binding.stream_index != subtitle.stream_index
            || binding.source_id
                != if binding.kind == RemoteMediaSourceKind::Plex {
                    subtitle.path.as_deref().and_then(plex::stream_path)
                } else {
                    subtitle.path.clone()
                }
    }) {
        return Err(unavailable());
    }
    let Some(content) = fetch(pool, subtitle.id).await? else {
        if original.is_some() {
            return Err(unavailable());
        }
        return Ok(subtitle.clone());
    };
    if binding(pool, subtitle.id).await? != original {
        return Err(unavailable());
    }
    let mut digest = Sha256::new();
    digest.update(b"ersatzrs:provider-subtitle:v1\0");
    digest.update(content.extension.as_bytes());
    digest.update(&content.bytes);
    let mut key = String::with_capacity(64);
    for byte in digest.finalize() {
        write!(key, "{byte:02x}").map_err(|_| unavailable())?;
    }
    tokio::fs::create_dir_all(cache)
        .await
        .map_err(|_| unavailable())?;
    let path = cache.join(format!("provider-{key}.{}", content.extension));
    let mut prepared = subtitle.clone();
    prepared.path = Some(path.to_str().ok_or_else(unavailable)?.to_owned());
    prepared.codec = Some(content.extension.to_owned());
    if let Ok(file) = tokio::fs::File::open(&path).await {
        let mut existing = Vec::new();
        file.take(MAX_BYTES as u64 + 1)
            .read_to_end(&mut existing)
            .await
            .map_err(|_| unavailable())?;
        if existing == content.bytes {
            // In particular, do not replace a file an active Windows FFmpeg
            // process may hold open. Identical content already is complete.
            return Ok(prepared);
        }
    }
    // Keep the temporary file owned until all bytes are written. A cancelled
    // request drops it; readers can only discover the atomically published file.
    let temporary = tempfile::NamedTempFile::new_in(cache).map_err(|_| unavailable())?;
    let mut file = tokio::fs::File::from_std(temporary.reopen().map_err(|_| unavailable())?);
    file.write_all(&content.bytes)
        .await
        .map_err(|_| unavailable())?;
    file.flush().await.map_err(|_| unavailable())?;
    drop(file);
    temporary.persist(&path).map_err(|_| unavailable())?;
    Ok(prepared)
}

/// Retrieve an external subtitle using its owning server's current credentials.
/// Redirects are refused; credentials never enter the URL or returned errors.
/// Local and embedded/generated subtitles return `None`.
///
/// # Errors
/// Fails closed on invalid ownership, authentication, HTTP failure, responses
/// exceeding 32 MiB, or the 15-second deadline (including response consumption).
pub async fn fetch(pool: &DbPool, subtitle_id: i64) -> Result<Option<ProviderSubtitleContent>> {
    tokio::time::timeout(
        std::time::Duration::from_secs(15),
        fetch_inner(pool, subtitle_id),
    )
    .await
    .map_err(|_| unavailable())?
}

async fn fetch_inner(pool: &DbPool, subtitle_id: i64) -> Result<Option<ProviderSubtitleContent>> {
    let Some(binding) = binding(pool, subtitle_id).await? else {
        return Ok(None);
    };
    let (address, header, credential) = match binding.kind {
        RemoteMediaSourceKind::Jellyfin => {
            let config = crate::jellyfin::load_active_jellyfin_connection_config(
                pool,
                binding.media_source_id,
            )
            .await
            .map_err(|_| unavailable())?;
            let credential = crate::media_server::scoped_credential(
                pool,
                binding.kind,
                &[
                    format!("jellyfin.authorization_header.{}", config.address),
                    format!("jellyfin.api_key.{}", config.address),
                ],
                config.authorization_header,
            )
            .await
            .map_err(|_| unavailable())?;
            (
                config.address,
                "Authorization",
                crate::jellyfin::format_jellyfin_auth_header(credential),
            )
        }
        RemoteMediaSourceKind::Emby => {
            let config =
                crate::emby::load_active_emby_connection_config(pool, binding.media_source_id)
                    .await
                    .map_err(|_| unavailable())?;
            let credential = crate::media_server::scoped_credential(
                pool,
                binding.kind,
                &[format!("emby.api_key.{}", config.address)],
                config.api_key,
            )
            .await
            .map_err(|_| unavailable())?;
            (config.address, "X-Emby-Token", credential)
        }
        RemoteMediaSourceKind::Plex => {
            let config = plex::connection(pool, binding.media_source_id).await?;
            (config.uri, "X-Plex-Token", config.token)
        }
    };
    let client = reqwest::Client::builder()
        .redirect(reqwest::redirect::Policy::none())
        .build()
        .map_err(|_| unavailable())?;
    let mut response = client
        .get(binding.endpoint(&address)?)
        .header(header, credential)
        .send()
        .await
        .map_err(|_| unavailable())?;
    if !response.status().is_success()
        || response
            .content_length()
            .is_some_and(|size| size > MAX_BYTES as u64)
    {
        return Err(unavailable());
    }
    let mut bytes = Vec::new();
    while let Some(chunk) = response.chunk().await.map_err(|_| unavailable())? {
        if chunk.len() > MAX_BYTES.saturating_sub(bytes.len()) {
            return Err(unavailable());
        }
        bytes.extend_from_slice(&chunk);
    }
    Ok(Some(ProviderSubtitleContent {
        bytes,
        extension: binding.extension,
    }))
}

/// Exact provider ownership of a persisted external subtitle.
/// This binding contains no credentials; resolve those when opening the source.
#[derive(PartialEq, Eq)]
pub struct ProviderSubtitleBinding {
    pub kind: RemoteMediaSourceKind,
    pub media_source_id: i64,
    item_id: String,
    source_id: Option<String>,
    stream_index: i32,
    extension: &'static str,
}

impl ProviderSubtitleBinding {
    /// Build the upstream subtitle endpoint below a configured server base path.
    /// Provider identifiers are encoded as individual path segments.
    ///
    /// # Errors
    /// Rejects invalid server addresses and incomplete provider identities.
    pub fn endpoint(&self, address: &str) -> Result<Url> {
        let mut url = Url::parse(address).map_err(|_| unavailable())?;
        if !matches!(url.scheme(), "http" | "https")
            || url.host_str().is_none()
            || !url.username().is_empty()
            || url.password().is_some()
            || url.query().is_some()
            || url.fragment().is_some()
        {
            return Err(unavailable());
        }
        if self.kind == RemoteMediaSourceKind::Plex {
            let path = self.source_id.as_deref().ok_or_else(unavailable)?;
            let path = plex::stream_path(path).ok_or_else(unavailable)?;
            url.set_path(&path);
            return Ok(url);
        }
        let source = match self.kind {
            RemoteMediaSourceKind::Jellyfin => &self.item_id,
            RemoteMediaSourceKind::Emby => self.source_id.as_ref().ok_or_else(unavailable)?,
            RemoteMediaSourceKind::Plex => return Err(unavailable()),
        };
        if [&self.item_id, source]
            .iter()
            .any(|id| id.trim().is_empty() || matches!(id.as_str(), "." | ".."))
            || self.stream_index < 0
        {
            return Err(unavailable());
        }
        {
            let mut path = url.path_segments_mut().map_err(|()| unavailable())?;
            path.pop_if_empty()
                .extend(["Videos", &self.item_id, source, "Subtitles"])
                .push(&self.stream_index.to_string());
            // ErsatzRS stores the raw provider index, without upstream's offset.
            if self.kind == RemoteMediaSourceKind::Jellyfin {
                path.push(&self.stream_index.to_string());
            }
            path.push(&format!("Stream.{}", self.extension));
        }
        Ok(url)
    }
}

/// Find a sidecar's owning Plex/Jellyfin/Emby server and provider item identity.
/// Local files and embedded/generated subtitles keep their existing paths.
///
/// # Errors
/// Returns a redacted error for missing or ambiguous provider ownership.
pub async fn binding(pool: &DbPool, subtitle_id: i64) -> Result<Option<ProviderSubtitleBinding>> {
    if let Some(binding) = plex::binding(pool, subtitle_id).await? {
        return Ok(Some(binding));
    }
    let rows = sqlx::query(
        r#"
        WITH Owner AS (
            SELECT S."Id", M."MovieId" AS ItemId, 0 AS ItemKind FROM "Subtitle" S
            JOIN "MovieMetadata" M ON M."Id" = S."MovieMetadataId" WHERE S."Id" = ?
            UNION ALL
            SELECT S."Id", E."EpisodeId", 1 FROM "Subtitle" S
            JOIN "EpisodeMetadata" E ON E."Id" = S."EpisodeMetadataId" WHERE S."Id" = ?
        )
        SELECT L."MediaSourceId", S."StreamIndex", S."Path", S."Codec",
            JL."Id" AS Jellyfin, EL."Id" AS Emby,
            COALESCE(JM."ItemId", JE."ItemId") AS JellyfinItem,
            COALESCE(EM."ItemId", EE."ItemId") AS EmbyItem
        FROM Owner O JOIN "Subtitle" S ON S."Id" = O."Id"
        JOIN "MediaItem" I ON I."Id" = O.ItemId
        JOIN "LibraryPath" P ON P."Id" = I."LibraryPathId"
        JOIN "Library" L ON L."Id" = P."LibraryId"
        LEFT JOIN "JellyfinLibrary" JL ON JL."Id" = L."Id"
        LEFT JOIN "EmbyLibrary" EL ON EL."Id" = L."Id"
        LEFT JOIN "JellyfinMovie" JM ON JM."Id" = O.ItemId AND O.ItemKind = 0
        LEFT JOIN "JellyfinEpisode" JE ON JE."Id" = O.ItemId AND O.ItemKind = 1
        LEFT JOIN "EmbyMovie" EM ON EM."Id" = O.ItemId AND O.ItemKind = 0
        LEFT JOIN "EmbyEpisode" EE ON EE."Id" = O.ItemId AND O.ItemKind = 1
        WHERE S."SubtitleKind" = 1 AND (JL."Id" IS NOT NULL OR EL."Id" IS NOT NULL)
    "#,
    )
    .bind(subtitle_id)
    .bind(subtitle_id)
    .fetch_all(pool)
    .await
    .repository()?;
    let row = match rows.as_slice() {
        [] => return Ok(None),
        [row] => row,
        _ => return Err(unavailable()),
    };
    let jellyfin = row
        .try_get::<Option<i64>, _>("Jellyfin")
        .repository()?
        .is_some();
    let emby = row
        .try_get::<Option<i64>, _>("Emby")
        .repository()?
        .is_some();
    let (kind, column) = match (jellyfin, emby) {
        (true, false) => (RemoteMediaSourceKind::Jellyfin, "JellyfinItem"),
        (false, true) => (RemoteMediaSourceKind::Emby, "EmbyItem"),
        _ => return Err(unavailable()),
    };
    let codec: Option<String> = row.try_get("Codec").repository()?;
    let extension = match codec.as_deref() {
        Some("subrip" | "srt" | "mov_text") => "srt",
        Some("ass") => "ass",
        Some("ssa") => "ssa",
        Some("webvtt" | "vtt") => "vtt",
        _ => return Err(unavailable()),
    };
    Ok(Some(ProviderSubtitleBinding {
        kind,
        media_source_id: row.try_get("MediaSourceId").repository()?,
        item_id: row
            .try_get::<Option<String>, _>(column)
            .repository()?
            .ok_or_else(unavailable)?,
        source_id: row.try_get("Path").repository()?,
        stream_index: row.try_get("StreamIndex").repository()?,
        extension,
    }))
}

fn unavailable() -> RepositoryError {
    RepositoryError::Decode {
        kind: "provider subtitle",
        value: "provider subtitle binding is unavailable".into(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn endpoint_keeps_provider_identifiers_inside_path_segments() {
        let binding = ProviderSubtitleBinding {
            kind: RemoteMediaSourceKind::Emby,
            media_source_id: 1,
            item_id: "item/with?delimiters".into(),
            source_id: Some("source#one".into()),
            stream_index: 3,
            extension: "srt",
        };
        let url = binding.endpoint("https://example.test/base/").unwrap();
        assert_eq!(url.host_str(), Some("example.test"));
        assert_eq!(url.query(), None);
        assert_eq!(url.fragment(), None);
        assert_eq!(
            url.path(),
            "/base/Videos/item%2Fwith%3Fdelimiters/source%23one/Subtitles/3/Stream.srt"
        );
        for address in [
            "file:///tmp/server",
            "https://example.test/?key=value",
            "https://example.test/#fragment",
        ] {
            assert!(binding.endpoint(address).is_err());
        }
    }

    #[test]
    fn endpoint_rejects_missing_or_traversing_provider_identity() {
        let mut binding = ProviderSubtitleBinding {
            kind: RemoteMediaSourceKind::Emby,
            media_source_id: 1,
            item_id: "movie".into(),
            source_id: None,
            stream_index: 2,
            extension: "srt",
        };
        assert!(binding.endpoint("https://example.test").is_err());
        binding.source_id = Some("..".into());
        assert!(binding.endpoint("https://example.test").is_err());
        binding.source_id = Some("source".into());
        binding.stream_index = -1;
        assert!(binding.endpoint("https://example.test").is_err());
    }
}
