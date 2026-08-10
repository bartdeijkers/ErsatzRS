//! Batched metadata hydration for XMLTV guide items.
//!
//! Guide windows can contain thousands of distinct songs or videos. Resolve
//! each subtype and its related metadata in bounded `IN` queries so request
//! cost scales with batches, not with per-item sequential probes.

use std::collections::HashMap;

use ersatzrs_core::xmltv::{IconSizeHint, XmltvCredit, XmltvItemKind, XmltvProgrammeContent};
use sqlx::{Pool, Row};

use crate::db::{assert_sql_safe, Db, DbRow};
use crate::metadata::artwork_kind;

pub(crate) type ContentCache = HashMap<i64, (Option<String>, XmltvProgrammeContent)>;

const ID_BATCH_SIZE: usize = 400;

pub(crate) async fn load_content_cache(
    pool: &Pool<Db>,
    media_item_ids: &[i64],
) -> Result<ContentCache, sqlx::Error> {
    let mut cache = HashMap::with_capacity(media_item_ids.len());
    load_movies(pool, media_item_ids, &mut cache).await?;
    load_episodes(pool, media_item_ids, &mut cache).await?;
    load_music_videos(pool, media_item_ids, &mut cache).await?;
    load_songs(pool, media_item_ids, &mut cache).await?;
    load_other_videos(pool, media_item_ids, &mut cache).await?;
    load_remote_streams(pool, media_item_ids, &mut cache).await?;
    Ok(cache)
}

async fn load_movies(
    pool: &Pool<Db>,
    ids: &[i64],
    cache: &mut ContentCache,
) -> Result<(), sqlx::Error> {
    let rows = fetch_rows_for_ids(
        pool,
        r#"SELECT "MovieId" AS "ItemId", "Id" AS "MetadataId", "Title", "Plot", "Year", "ContentRating"
           FROM "MovieMetadata" WHERE "MovieId" IN"#,
        ids,
        r#"ORDER BY "MetadataId""#,
    )
    .await?;
    let refs = insert_base_rows(cache, rows, XmltvItemKind::Movie, |row, content| {
        content.description = nonempty(row.try_get("Plot")?);
        content.year = row.try_get("Year")?;
        content.content_rating = nonempty(row.try_get("ContentRating")?);
        Ok(nonempty(row.try_get("Title")?))
    })?;

    hydrate_common(pool, cache, &refs, "MovieMetadataId", true).await
}

async fn load_episodes(
    pool: &Pool<Db>,
    ids: &[i64],
    cache: &mut ContentCache,
) -> Result<(), sqlx::Error> {
    let rows = fetch_rows_for_ids(
        pool,
        r#"SELECT ep."EpisodeId" AS "ItemId", ep."Id" AS "MetadataId",
                  ep."Title" AS "EpisodeTitle", ep."Plot", ep."EpisodeNumber",
                  season."SeasonNumber", show_meta."Id" AS "RelatedMetadataId",
                  show_meta."Title" AS "ShowTitle", show_meta."Year" AS "ShowYear",
                  show_meta."ContentRating" AS "ShowContentRating"
           FROM "EpisodeMetadata" ep
           JOIN "Episode" episode ON episode."Id" = ep."EpisodeId"
           JOIN "Season" season ON season."Id" = episode."SeasonId"
           JOIN "ShowMetadata" show_meta ON show_meta."ShowId" = season."ShowId"
           WHERE ep."EpisodeId" IN"#,
        ids,
        r#"ORDER BY "MetadataId", "RelatedMetadataId""#,
    )
    .await?;

    let mut episode_refs = Vec::new();
    let mut show_refs = Vec::new();
    for row in rows {
        let item_id: i64 = row.try_get("ItemId")?;
        if cache.contains_key(&item_id) {
            continue;
        }
        let episode_metadata_id: i64 = row.try_get("MetadataId")?;
        let show_metadata_id: i64 = row.try_get("RelatedMetadataId")?;
        cache.insert(
            item_id,
            (
                nonempty(row.try_get("ShowTitle")?),
                XmltvProgrammeContent {
                    kind: XmltvItemKind::Episode,
                    sub_title: nonempty(row.try_get("EpisodeTitle")?),
                    description: nonempty(row.try_get("Plot")?),
                    year: row.try_get("ShowYear")?,
                    content_rating: nonempty(row.try_get("ShowContentRating")?),
                    season_number: row.try_get("SeasonNumber")?,
                    episode_number: row.try_get("EpisodeNumber")?,
                    ..XmltvProgrammeContent::default()
                },
            ),
        );
        episode_refs.push((item_id, episode_metadata_id));
        show_refs.push((item_id, show_metadata_id));
    }

    let show_ids = metadata_ids(&show_refs);
    let episode_ids = metadata_ids(&episode_refs);
    let mut show_genres =
        load_string_map(pool, "Genre", "ShowMetadataId", "Name", &show_ids).await?;
    let mut show_guids =
        load_string_map(pool, "MetadataGuid", "ShowMetadataId", "Guid", &show_ids).await?;
    let mut show_artwork = load_artwork_map(pool, "ShowMetadataId", &show_ids).await?;
    let mut episode_guids = load_string_map(
        pool,
        "MetadataGuid",
        "EpisodeMetadataId",
        "Guid",
        &episode_ids,
    )
    .await?;
    let mut episode_artwork = load_artwork_map(pool, "EpisodeMetadataId", &episode_ids).await?;

    for (item_id, metadata_id) in show_refs {
        if let Some((_, content)) = cache.get_mut(&item_id) {
            content.genres = show_genres.remove(&metadata_id).unwrap_or_default();
            content.guids = show_guids.remove(&metadata_id).unwrap_or_default();
            set_icon(content, show_artwork.remove(&metadata_id));
        }
    }
    for (item_id, metadata_id) in episode_refs {
        if let Some((_, content)) = cache.get_mut(&item_id) {
            content.episode_guids = episode_guids.remove(&metadata_id).unwrap_or_default();
            if let Some((path, hint)) = episode_artwork.remove(&metadata_id) {
                content.thumbnail_path = Some(path);
                content.thumbnail_size_hint = hint;
            }
        }
    }
    Ok(())
}

async fn load_music_videos(
    pool: &Pool<Db>,
    ids: &[i64],
    cache: &mut ContentCache,
) -> Result<(), sqlx::Error> {
    let rows = fetch_rows_for_ids(
        pool,
        r#"SELECT mv_meta."MusicVideoId" AS "ItemId", mv_meta."Id" AS "MetadataId",
                  artist_meta."Id" AS "RelatedMetadataId", artist_meta."Title" AS "ArtistTitle",
                  mv_meta."Title" AS "VideoTitle", mv_meta."Plot", mv_meta."Year",
                  mv_meta."Track", mv_meta."Album", mv_meta."ReleaseDate"
           FROM "MusicVideoMetadata" mv_meta
           JOIN "MusicVideo" video ON video."Id" = mv_meta."MusicVideoId"
           LEFT JOIN "ArtistMetadata" artist_meta ON artist_meta."ArtistId" = video."ArtistId"
           WHERE mv_meta."MusicVideoId" IN"#,
        ids,
        r#"ORDER BY "MetadataId", "RelatedMetadataId""#,
    )
    .await?;

    let mut video_refs = Vec::new();
    let mut artist_refs = Vec::new();
    for row in rows {
        let item_id: i64 = row.try_get("ItemId")?;
        if cache.contains_key(&item_id) {
            continue;
        }
        let metadata_id: i64 = row.try_get("MetadataId")?;
        let related_id: Option<i64> = row.try_get("RelatedMetadataId")?;
        let track = row
            .try_get::<Option<i32>, _>("Track")?
            .map(|value| value.to_string());
        cache.insert(
            item_id,
            (
                nonempty(row.try_get("ArtistTitle")?),
                XmltvProgrammeContent {
                    kind: XmltvItemKind::MusicVideo,
                    sub_title: nonempty(row.try_get("VideoTitle")?),
                    description: nonempty(row.try_get("Plot")?),
                    year: row.try_get("Year")?,
                    track,
                    album: nonempty(row.try_get("Album")?),
                    release_date: nonempty(row.try_get("ReleaseDate")?),
                    ..XmltvProgrammeContent::default()
                },
            ),
        );
        video_refs.push((item_id, metadata_id));
        if let Some(related_id) = related_id {
            artist_refs.push((item_id, related_id));
        }
    }

    let video_ids = metadata_ids(&video_refs);
    let artist_ids = metadata_ids(&artist_refs);
    let mut video_genres =
        load_string_map(pool, "Genre", "MusicVideoMetadataId", "Name", &video_ids).await?;
    let mut artists = load_string_map(
        pool,
        "MusicVideoArtist",
        "MusicVideoMetadataId",
        "Name",
        &video_ids,
    )
    .await?;
    let mut studios =
        load_string_map(pool, "Studio", "MusicVideoMetadataId", "Name", &video_ids).await?;
    let mut directors =
        load_string_map(pool, "Director", "MusicVideoMetadataId", "Name", &video_ids).await?;
    let mut artwork = load_artwork_map(pool, "MusicVideoMetadataId", &video_ids).await?;
    let mut artist_genres =
        load_string_map(pool, "Genre", "ArtistMetadataId", "Name", &artist_ids).await?;

    for (item_id, metadata_id) in video_refs {
        if let Some((_, content)) = cache.get_mut(&item_id) {
            content.genres = video_genres.remove(&metadata_id).unwrap_or_default();
            content.artists = artists.remove(&metadata_id).unwrap_or_default();
            content.studios = studios.remove(&metadata_id).unwrap_or_default();
            content.directors = directors.remove(&metadata_id).unwrap_or_default();
            set_icon(content, artwork.remove(&metadata_id));
        }
    }
    for (item_id, metadata_id) in artist_refs {
        if let Some((_, content)) = cache.get_mut(&item_id) {
            content.artist_genres = artist_genres.remove(&metadata_id).unwrap_or_default();
        }
    }
    Ok(())
}

async fn load_songs(
    pool: &Pool<Db>,
    ids: &[i64],
    cache: &mut ContentCache,
) -> Result<(), sqlx::Error> {
    let rows = fetch_rows_for_ids(
        pool,
        r#"SELECT "SongId" AS "ItemId", "Id" AS "MetadataId", "Title", "Year",
                  "Artists", "AlbumArtists", "Track", "Comment", "Album", "ReleaseDate"
           FROM "SongMetadata" WHERE "SongId" IN"#,
        ids,
        r#"ORDER BY "MetadataId""#,
    )
    .await?;
    let refs = insert_base_rows(cache, rows, XmltvItemKind::Song, |row, content| {
        let artists = decode_string_list(row.try_get("Artists")?);
        content.sub_title = nonempty(row.try_get("Title")?);
        content.year = row.try_get("Year")?;
        content.artists.clone_from(&artists);
        content.album_artists = decode_string_list(row.try_get("AlbumArtists")?);
        content.track = nonempty(row.try_get("Track")?);
        content.comment = nonempty(row.try_get("Comment")?);
        content.album = nonempty(row.try_get("Album")?);
        content.release_date = nonempty(row.try_get("ReleaseDate")?);
        Ok((!artists.is_empty()).then(|| artists.join(", ")))
    })?;

    hydrate_common(pool, cache, &refs, "SongMetadataId", false).await?;
    hydrate_studios(pool, cache, &refs, "SongMetadataId").await
}

async fn load_other_videos(
    pool: &Pool<Db>,
    ids: &[i64],
    cache: &mut ContentCache,
) -> Result<(), sqlx::Error> {
    let rows = fetch_rows_for_ids(
        pool,
        r#"SELECT "OtherVideoId" AS "ItemId", "Id" AS "MetadataId", "Title", "Plot", "Year", "ContentRating"
           FROM "OtherVideoMetadata" WHERE "OtherVideoId" IN"#,
        ids,
        r#"ORDER BY "MetadataId""#,
    )
    .await?;
    let refs = insert_base_rows(cache, rows, XmltvItemKind::OtherVideo, |row, content| {
        content.description = nonempty(row.try_get("Plot")?);
        content.year = row.try_get("Year")?;
        content.content_rating = nonempty(row.try_get("ContentRating")?);
        Ok(nonempty(row.try_get("Title")?))
    })?;

    hydrate_common(pool, cache, &refs, "OtherVideoMetadataId", false).await
}

async fn load_remote_streams(
    pool: &Pool<Db>,
    ids: &[i64],
    cache: &mut ContentCache,
) -> Result<(), sqlx::Error> {
    let rows = fetch_rows_for_ids(
        pool,
        r#"SELECT "RemoteStreamId" AS "ItemId", "Id" AS "MetadataId", "Title", "Plot", "Year",
                  "ReleaseDate", "ContentRating", "CollectionName",
                  "OriginalBroadcasters", "Broadcasters"
           FROM "RemoteStreamMetadata" WHERE "RemoteStreamId" IN"#,
        ids,
        r#"ORDER BY "MetadataId""#,
    )
    .await?;
    let refs = insert_base_rows(cache, rows, XmltvItemKind::RemoteStream, |row, content| {
        content.description = nonempty(row.try_get("Plot")?);
        content.year = row.try_get("Year")?;
        content.release_date = nonempty(row.try_get("ReleaseDate")?);
        content.content_rating = nonempty(row.try_get("ContentRating")?);
        content.collection = nonempty(row.try_get("CollectionName")?);
        content.original_broadcasters = decode_string_list(row.try_get("OriginalBroadcasters")?);
        content.broadcasters = decode_string_list(row.try_get("Broadcasters")?);
        Ok(nonempty(row.try_get("Title")?))
    })?;
    hydrate_common(pool, cache, &refs, "RemoteStreamMetadataId", false).await?;

    let ids = metadata_ids(&refs);
    let mut tags = load_string_map(pool, "Tag", "RemoteStreamMetadataId", "Name", &ids).await?;
    let mut people = load_remote_stream_people(pool, &ids).await?;
    for (item_id, metadata_id) in refs {
        if let Some((_, content)) = cache.get_mut(&item_id) {
            content.tags = tags.remove(&metadata_id).unwrap_or_default();
            content.people = people.remove(&metadata_id).unwrap_or_default();
        }
    }
    Ok(())
}

async fn load_remote_stream_people(
    pool: &Pool<Db>,
    ids: &[i64],
) -> Result<HashMap<i64, Vec<XmltvCredit>>, sqlx::Error> {
    let rows = fetch_rows_for_ids(
        pool,
        r#"SELECT "RemoteStreamMetadataId" AS "MetadataId", "Name", "Role"
           FROM "Actor" WHERE "RemoteStreamMetadataId" IN"#,
        ids,
        r#"ORDER BY "MetadataId", "Order", "Id""#,
    )
    .await?;
    let mut people: HashMap<i64, Vec<XmltvCredit>> = HashMap::new();
    for row in rows {
        let metadata_id: i64 = row.try_get("MetadataId")?;
        let Some(name) = nonempty(row.try_get("Name")?) else {
            continue;
        };
        let role = nonempty(row.try_get("Role")?).unwrap_or_else(|| "person".to_owned());
        people
            .entry(metadata_id)
            .or_default()
            .push(XmltvCredit { name, role });
    }
    Ok(people)
}

fn insert_base_rows<F>(
    cache: &mut ContentCache,
    rows: Vec<DbRow>,
    kind: XmltvItemKind,
    mut populate: F,
) -> Result<Vec<(i64, i64)>, sqlx::Error>
where
    F: FnMut(&DbRow, &mut XmltvProgrammeContent) -> Result<Option<String>, sqlx::Error>,
{
    let mut refs = Vec::new();
    for row in rows {
        let item_id: i64 = row.try_get("ItemId")?;
        if cache.contains_key(&item_id) {
            continue;
        }
        let metadata_id: i64 = row.try_get("MetadataId")?;
        let mut content = XmltvProgrammeContent {
            kind,
            ..XmltvProgrammeContent::default()
        };
        let title = populate(&row, &mut content)?;
        cache.insert(item_id, (title, content));
        refs.push((item_id, metadata_id));
    }
    Ok(refs)
}

async fn hydrate_common(
    pool: &Pool<Db>,
    cache: &mut ContentCache,
    refs: &[(i64, i64)],
    owner_column: &str,
    include_guids: bool,
) -> Result<(), sqlx::Error> {
    let ids = metadata_ids(refs);
    let mut genres = load_string_map(pool, "Genre", owner_column, "Name", &ids).await?;
    let mut artwork = load_artwork_map(pool, owner_column, &ids).await?;
    let mut guids = if include_guids {
        load_string_map(pool, "MetadataGuid", owner_column, "Guid", &ids).await?
    } else {
        HashMap::new()
    };

    for &(item_id, metadata_id) in refs {
        if let Some((_, content)) = cache.get_mut(&item_id) {
            content.genres = genres.remove(&metadata_id).unwrap_or_default();
            content.guids = guids.remove(&metadata_id).unwrap_or_default();
            set_icon(content, artwork.remove(&metadata_id));
        }
    }
    Ok(())
}

async fn hydrate_studios(
    pool: &Pool<Db>,
    cache: &mut ContentCache,
    refs: &[(i64, i64)],
    owner_column: &str,
) -> Result<(), sqlx::Error> {
    let mut studios =
        load_string_map(pool, "Studio", owner_column, "Name", &metadata_ids(refs)).await?;
    for &(item_id, metadata_id) in refs {
        if let Some((_, content)) = cache.get_mut(&item_id) {
            content.studios = studios.remove(&metadata_id).unwrap_or_default();
        }
    }
    Ok(())
}

fn set_icon(content: &mut XmltvProgrammeContent, artwork: Option<(String, IconSizeHint)>) {
    if let Some((path, hint)) = artwork {
        content.icon_path = Some(path);
        content.icon_size_hint = hint;
    }
}

async fn load_string_map(
    pool: &Pool<Db>,
    table: &str,
    owner_column: &str,
    value_column: &str,
    ids: &[i64],
) -> Result<HashMap<i64, Vec<String>>, sqlx::Error> {
    let prefix = format!(
        r#"SELECT "{owner_column}" AS "MetadataId", "{value_column}" AS "Value"
            FROM "{table}" WHERE "{owner_column}" IN"#
    );
    let rows = fetch_rows_for_ids(pool, &prefix, ids, r#"ORDER BY "MetadataId", "Value""#).await?;
    let mut values: HashMap<i64, Vec<String>> = HashMap::new();
    for row in rows {
        let metadata_id: i64 = row.try_get("MetadataId")?;
        if let Some(value) = nonempty(row.try_get("Value")?) {
            let entries = values.entry(metadata_id).or_default();
            if !entries.contains(&value) {
                entries.push(value);
            }
        }
    }
    Ok(values)
}

async fn load_artwork_map(
    pool: &Pool<Db>,
    owner_column: &str,
    ids: &[i64],
) -> Result<HashMap<i64, (String, IconSizeHint)>, sqlx::Error> {
    let prefix = format!(
        r#"SELECT "{owner_column}" AS "MetadataId", "Path", "ArtworkKind"
            FROM "Artwork" WHERE "ArtworkKind" IN ({}, {}) AND "{owner_column}" IN"#,
        artwork_kind::POSTER,
        artwork_kind::THUMBNAIL
    );
    let rows = fetch_rows_for_ids(
        pool,
        &prefix,
        ids,
        r#"ORDER BY "MetadataId", "ArtworkKind""#,
    )
    .await?;
    let mut artwork = HashMap::new();
    for row in rows {
        let metadata_id: i64 = row.try_get("MetadataId")?;
        let Some(path) = nonempty(row.try_get("Path")?) else {
            continue;
        };
        let kind: i32 = row.try_get("ArtworkKind")?;
        let hint = if kind == artwork_kind::POSTER {
            IconSizeHint::Poster
        } else {
            IconSizeHint::Thumbnail
        };
        match artwork.get(&metadata_id) {
            Some((_, IconSizeHint::Poster)) => {}
            _ => {
                artwork.insert(metadata_id, (path, hint));
            }
        }
    }
    Ok(artwork)
}

async fn fetch_rows_for_ids(
    pool: &Pool<Db>,
    prefix: &str,
    ids: &[i64],
    suffix: &str,
) -> Result<Vec<DbRow>, sqlx::Error> {
    let mut rows = Vec::new();
    for chunk in ids.chunks(ID_BATCH_SIZE) {
        if chunk.is_empty() {
            continue;
        }
        let placeholders = chunk.iter().map(|_| "?").collect::<Vec<_>>().join(",");
        let sql = format!("{prefix} ({placeholders}) {suffix}");
        let mut query = sqlx::query(assert_sql_safe(sql));
        for id in chunk {
            query = query.bind(*id);
        }
        rows.extend(query.fetch_all(pool).await?);
    }
    Ok(rows)
}

fn metadata_ids(refs: &[(i64, i64)]) -> Vec<i64> {
    let mut ids = refs.iter().map(|(_, id)| *id).collect::<Vec<_>>();
    ids.sort_unstable();
    ids.dedup();
    ids
}

fn decode_string_list(value: Option<String>) -> Vec<String> {
    let Some(value) = nonempty(value) else {
        return Vec::new();
    };
    serde_json::from_str::<Vec<String>>(&value).map_or_else(
        |_| vec![value],
        |values| {
            values
                .into_iter()
                .filter_map(|value| nonempty(Some(value)))
                .collect()
        },
    )
}

fn nonempty(value: Option<String>) -> Option<String> {
    value.filter(|value| !value.trim().is_empty())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[cfg(not(feature = "mysql"))]
    async fn test_db() -> (tempfile::TempDir, Pool<Db>) {
        let root = tempfile::tempdir().expect("create XMLTV metadata test root");
        let database_url = format!("sqlite:{}", root.path().join("ErsatzRS.db").display());
        let pool = crate::db::init(&database_url)
            .await
            .expect("initialize XMLTV metadata test database");
        (root, pool)
    }

    #[test]
    fn decodes_json_string_lists_and_preserves_legacy_scalar_values() {
        assert_eq!(
            decode_string_list(Some(r#"["Artist One","Artist Two"]"#.to_owned())),
            vec!["Artist One", "Artist Two"]
        );
        assert_eq!(
            decode_string_list(Some("Legacy Artist".to_owned())),
            vec!["Legacy Artist"]
        );
        assert!(decode_string_list(Some("  ".to_owned())).is_empty());
    }

    #[cfg(not(feature = "mysql"))]
    #[tokio::test]
    async fn episode_uses_show_artwork_for_icon_and_episode_artwork_for_thumbnail() {
        let (_root, pool) = test_db().await;
        for statement in [
            r#"INSERT INTO "MediaSource" ("Id") VALUES (10)"#,
            r#"INSERT INTO "LocalMediaSource" ("Id") VALUES (10)"#,
            r#"INSERT INTO "Library" ("Id", "MediaKind", "MediaSourceId", "Name") VALUES (10, 2, 10, 'TV')"#,
            r#"INSERT INTO "LocalLibrary" ("Id") VALUES (10)"#,
            r#"INSERT INTO "LibraryPath" ("Id", "LibraryId", "Path") VALUES (10, 10, '/media')"#,
            r#"INSERT INTO "MediaItem" ("Id", "LibraryPathId") VALUES (20, 10), (21, 10), (22, 10)"#,
            r#"INSERT INTO "Show" ("Id") VALUES (20)"#,
            r#"INSERT INTO "Season" ("Id", "ShowId", "SeasonNumber") VALUES (21, 20, 2)"#,
            r#"INSERT INTO "Episode" ("Id", "SeasonId") VALUES (22, 21)"#,
            r#"INSERT INTO "ShowMetadata" ("Id", "ShowId", "DateAdded", "DateUpdated", "MetadataKind", "Title", "Year", "ContentRating") VALUES (30, 20, '2026-01-01', '2026-01-01', 0, 'The Show', 2026, 'us:TV-14')"#,
            r#"INSERT INTO "EpisodeMetadata" ("Id", "EpisodeId", "DateAdded", "DateUpdated", "MetadataKind", "Title", "Plot", "EpisodeNumber") VALUES (31, 22, '2026-01-01', '2026-01-01', 0, 'The Episode', 'Episode plot', 3)"#,
            r#"INSERT INTO "Artwork" ("ArtworkKind", "ShowMetadataId", "Path", "DateAdded", "DateUpdated") VALUES (0, 30, '/show/poster.jpg', '2026-01-01', '2026-01-01')"#,
            r#"INSERT INTO "Artwork" ("ArtworkKind", "EpisodeMetadataId", "Path", "DateAdded", "DateUpdated") VALUES (1, 31, '/episode/still.jpg', '2026-01-01', '2026-01-01')"#,
        ] {
            sqlx::query(statement)
                .execute(&pool)
                .await
                .expect("seed episode metadata");
        }

        let cache = load_content_cache(&pool, &[22])
            .await
            .expect("load metadata");
        let (title, content) = cache.get(&22).expect("episode content");

        assert_eq!(title.as_deref(), Some("The Show"));
        assert_eq!(content.icon_path.as_deref(), Some("/show/poster.jpg"));
        assert_eq!(content.icon_size_hint, IconSizeHint::Poster);
        assert_eq!(
            content.thumbnail_path.as_deref(),
            Some("/episode/still.jpg")
        );
        assert_eq!(content.thumbnail_size_hint, IconSizeHint::Thumbnail);
        assert_eq!(content.season_number, Some(2));
        assert_eq!(content.episode_number, Some(3));
    }

    #[cfg(not(feature = "mysql"))]
    #[tokio::test]
    async fn movie_uses_thumbnail_when_poster_is_unavailable() {
        let (_root, pool) = test_db().await;
        for statement in [
            r#"INSERT INTO "MediaSource" ("Id") VALUES (10)"#,
            r#"INSERT INTO "LocalMediaSource" ("Id") VALUES (10)"#,
            r#"INSERT INTO "Library" ("Id", "MediaKind", "MediaSourceId", "Name") VALUES (10, 1, 10, 'Movies')"#,
            r#"INSERT INTO "LocalLibrary" ("Id") VALUES (10)"#,
            r#"INSERT INTO "LibraryPath" ("Id", "LibraryId", "Path") VALUES (10, 10, '/media')"#,
            r#"INSERT INTO "MediaItem" ("Id", "LibraryPathId") VALUES (20, 10)"#,
            r#"INSERT INTO "Movie" ("Id") VALUES (20)"#,
            r#"INSERT INTO "MovieMetadata" ("Id", "MovieId", "DateAdded", "DateUpdated", "MetadataKind", "Title") VALUES (30, 20, '2026-01-01', '2026-01-01', 0, 'The Movie')"#,
            r#"INSERT INTO "Artwork" ("ArtworkKind", "MovieMetadataId", "Path", "DateAdded", "DateUpdated") VALUES (1, 30, '/movie/still.jpg', '2026-01-01', '2026-01-01')"#,
        ] {
            sqlx::query(statement)
                .execute(&pool)
                .await
                .expect("seed movie metadata");
        }

        let cache = load_content_cache(&pool, &[20])
            .await
            .expect("load metadata");
        let (_, content) = cache.get(&20).expect("movie content");

        assert_eq!(content.icon_path.as_deref(), Some("/movie/still.jpg"));
        assert_eq!(content.icon_size_hint, IconSizeHint::Thumbnail);
    }

    #[cfg(not(feature = "mysql"))]
    #[tokio::test]
    async fn remote_stream_projects_provider_metadata_without_flattening_roles() {
        let (_root, pool) = test_db().await;
        for statement in [
            r#"INSERT INTO "MediaSource" ("Id") VALUES (10)"#,
            r#"INSERT INTO "LocalMediaSource" ("Id") VALUES (10)"#,
            r#"INSERT INTO "Library" ("Id", "MediaKind", "MediaSourceId", "Name") VALUES (10, 7, 10, 'Streams')"#,
            r#"INSERT INTO "LocalLibrary" ("Id") VALUES (10)"#,
            r#"INSERT INTO "LibraryPath" ("Id", "LibraryId", "Path") VALUES (10, 10, '/streams')"#,
            r#"INSERT INTO "MediaItem" ("Id", "LibraryPathId") VALUES (20, 10)"#,
            r#"INSERT INTO "RemoteStream" ("Id", "IsLive") VALUES (20, 0)"#,
            r#"INSERT INTO "RemoteStreamMetadata" ("Id", "RemoteStreamId", "DateAdded", "DateUpdated", "MetadataKind", "Title", "Plot", "Year", "ReleaseDate", "ContentRating", "CollectionName", "OriginalBroadcasters", "Broadcasters") VALUES (30, 20, '2026-01-01', '2026-01-01', 1, 'Episode', 'Plot', 1992, '1992-05-30T21:15:00Z', 'nl:12', 'Public broadcasters', '["AVRO"]', '["AVROTROS"]')"#,
            r#"INSERT INTO "Genre" ("Name", "RemoteStreamMetadataId") VALUES ('Showbiz', 30)"#,
            r#"INSERT INTO "Tag" ("Name", "RemoteStreamMetadataId") VALUES ('Television', 30)"#,
            r#"INSERT INTO "Actor" ("Name", "Role", "Order", "RemoteStreamMetadataId") VALUES ('Presenter', 'presenter', 0, 30), ('Director', 'director', 1, 30), ('Producer', 'producer', 2, 30)"#,
        ] {
            sqlx::query(statement)
                .execute(&pool)
                .await
                .expect("seed rich remote stream metadata");
        }

        let cache = load_content_cache(&pool, &[20])
            .await
            .expect("load metadata");
        let (_, content) = cache.get(&20).expect("remote stream content");
        assert_eq!(
            content.release_date.as_deref(),
            Some("1992-05-30T21:15:00Z")
        );
        assert_eq!(content.genres, ["Showbiz"]);
        assert_eq!(content.tags, ["Television"]);
        assert_eq!(content.collection.as_deref(), Some("Public broadcasters"));
        assert_eq!(content.original_broadcasters, ["AVRO"]);
        assert_eq!(content.broadcasters, ["AVROTROS"]);
        assert_eq!(content.people.len(), 3);
        assert_eq!(content.people[0].role, "presenter");
        assert_eq!(content.people[1].role, "director");
        assert_eq!(content.people[2].role, "producer");
    }
}
