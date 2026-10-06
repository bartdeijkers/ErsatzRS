//! Plex sidecars bind to their current owning server, never a persisted token URL.
use super::*;

pub(super) fn stream_path(value: &str) -> Option<String> {
    let value = value.trim();
    if value.is_empty() || value.contains(['\r', '\n', '\\']) {
        return None;
    }
    let url = if value.starts_with('/') && !value.starts_with("//") {
        Url::parse("http://subtitle.invalid")
            .ok()?
            .join(value)
            .ok()?
    } else {
        let url = Url::parse(value).ok()?;
        if !matches!(url.scheme(), "http" | "https")
            || !url.username().is_empty()
            || url.password().is_some()
        {
            return None;
        }
        url
    };
    let path = url.path();
    (path.starts_with("/library/streams/")
        && path != "/library/streams/"
        && url.fragment().is_none())
    .then(|| path.to_owned())
}

pub(super) async fn binding(
    pool: &DbPool,
    subtitle_id: i64,
) -> Result<Option<ProviderSubtitleBinding>> {
    let rows = sqlx::query(
        r#"
        WITH Owner AS (
            SELECT S."Id", M."MovieId" AS ItemId FROM "Subtitle" S
            JOIN "MovieMetadata" M ON M."Id"=S."MovieMetadataId" WHERE S."Id"=?1
            UNION ALL
            SELECT S."Id", E."EpisodeId" FROM "Subtitle" S
            JOIN "EpisodeMetadata" E ON E."Id"=S."EpisodeMetadataId" WHERE S."Id"=?1
            UNION ALL
            SELECT S."Id", V."OtherVideoId" FROM "Subtitle" S
            JOIN "OtherVideoMetadata" V ON V."Id"=S."OtherVideoMetadataId" WHERE S."Id"=?1
        )
        SELECT L."MediaSourceId", O.ItemId, S."StreamIndex", S."Path", S."Codec"
        FROM Owner O JOIN "Subtitle" S ON S."Id"=O."Id"
        JOIN "MediaItem" I ON I."Id"=O.ItemId
        JOIN "LibraryPath" LP ON LP."Id"=I."LibraryPathId"
        JOIN "Library" L ON L."Id"=LP."LibraryId"
        JOIN "PlexLibrary" PL ON PL."Id"=L."Id"
        JOIN "PlexMediaSource" PMS ON PMS."Id"=L."MediaSourceId"
        WHERE S."SubtitleKind"=1
    "#,
    )
    .bind(subtitle_id)
    .fetch_all(pool)
    .await
    .repository()?;
    let row = match rows.as_slice() {
        [] => return Ok(None),
        [row] => row,
        _ => return Err(unavailable()),
    };
    let path: Option<String> = row.try_get("Path").repository()?;
    let path = path
        .as_deref()
        .and_then(stream_path)
        .ok_or_else(unavailable)?;
    let codec: Option<String> = row.try_get("Codec").repository()?;
    let extension = match codec.as_deref() {
        Some("subrip" | "srt" | "mov_text") => "srt",
        Some("ass") => "ass",
        Some("ssa") => "ssa",
        Some("webvtt" | "vtt") => "vtt",
        _ => return Err(unavailable()),
    };
    Ok(Some(ProviderSubtitleBinding {
        kind: RemoteMediaSourceKind::Plex,
        media_source_id: row.try_get("MediaSourceId").repository()?,
        item_id: row.try_get::<i64, _>("ItemId").repository()?.to_string(),
        source_id: Some(path),
        stream_index: row.try_get("StreamIndex").repository()?,
        extension,
    }))
}

pub(super) async fn connection(
    pool: &DbPool,
    source_id: i64,
) -> Result<crate::plex::PlexConnectionConfig> {
    // Subtitle preparation can precede the first video open. Reuse the bounded
    // machine-identity resolver so a stale active endpoint cannot receive the
    // subtitle request or prevent trying another connection for this owner.
    let connections =
        crate::next_engine::connections::PlexConnections::new().map_err(|_| unavailable())?;
    let transport = connections
        .resolve(
            pool,
            source_id,
            tokio::time::Instant::now() + std::time::Duration::from_secs(15),
        )
        .await
        .map_err(|_| unavailable())?;
    Ok(crate::plex::PlexConnectionConfig {
        uri: transport.address,
        token: transport.token,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn plex_stream_identity_removes_legacy_query_and_rejects_unrelated_paths() {
        for path in [
            "/library/streams/123",
            "https://old.example.test/library/streams/123?X-Plex-Token=synthetic-old",
        ] {
            assert_eq!(stream_path(path).as_deref(), Some("/library/streams/123"));
        }
        for path in [
            "",
            " ",
            "/library/streams/",
            "/library/streams/../parts/1",
            "//foreign.test/library/streams/1",
            "file:///library/streams/1",
            "/library/streams/1#fragment",
        ] {
            assert!(stream_path(path).is_none());
        }
        let binding = ProviderSubtitleBinding {
            kind: RemoteMediaSourceKind::Plex,
            media_source_id: 1,
            item_id: "10".to_owned(),
            source_id: stream_path(
                "https://old.example.test/library/streams/123?X-Plex-Token=synthetic-old",
            ),
            stream_index: 2,
            extension: "srt",
        };
        let endpoint = binding
            .endpoint("https://current.example.test:32400/")
            .unwrap();
        assert_eq!(
            endpoint.as_str(),
            "https://current.example.test:32400/library/streams/123"
        );
    }
}

#[cfg(test)]
#[path = "provider_subtitles_plex_tests.rs"]
mod ownership_tests;
