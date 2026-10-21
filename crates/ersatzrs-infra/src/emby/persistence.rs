use super::*;

pub(super) async fn upsert_emby_movie(
    pool: &DbPool,
    media_source_id: i64,
    library_id: i64,
    library_item_id: &str,
    item: &EmbyMediaItem,
    playback: &EmbyPlaybackMetadata,
    claim: Option<&ersatzrs_core::repositories::ManagedLibraryOperationClaim>,
) -> Result<i64> {
    let mut teletext = crate::media_server::teletext::MappedTeletextWrite::acquire(
        pool,
        claim,
        crate::media_server::teletext::MappedTeletextSource {
            provider: ersatzrs_core::repositories::RemoteMediaSourceKind::Emby,
            id: media_source_id,
        },
        library_id,
        &item.path,
        playback.source_declaration.as_ref(),
        &playback.streams,
    )
    .await?;
    let mut tx = crate::db::begin_write(pool).await.repository()?;
    teletext.check_claim(&mut tx, library_id).await?;
    ensure_emby_library(&mut tx, media_source_id, library_id, library_item_id).await?;
    let library_path_id =
        ensure_library_path(&mut tx, library_id, &format!("emby://{library_id}")).await?;
    let item_id = ensure_emby_movie_item(&mut tx, library_path_id, item).await?;
    let movie_metadata_id = upsert_movie_metadata(&mut tx, item_id, item).await?;
    let version_id =
        ensure_media_version(&mut tx, VersionOwner::Movie(item_id), item, playback).await?;
    ensure_media_file(
        &mut tx,
        VersionOwner::Movie(item_id),
        version_id,
        &item.path,
        playback.source_declaration.as_ref(),
        &playback.streams,
    )
    .await?;
    replace_streams(&mut tx, version_id, &playback.streams).await?;
    crate::metadata::update_metadata_subtitles(
        &mut tx,
        crate::metadata::SubtitleMetadataOwner::Movie(movie_metadata_id),
        &playback.streams,
    )
    .await?;
    replace_chapters(&mut tx, version_id, &item.chapters).await?;
    teletext.persist(&mut tx, library_id, version_id).await?;
    tx.commit().await.repository()?;
    Ok(item_id)
}

pub(super) async fn upsert_emby_show(
    pool: &DbPool,
    media_source_id: i64,
    library_id: i64,
    library_item_id: &str,
    item: &EmbyShowItem,
) -> Result<i64> {
    let mut tx = crate::db::begin_write(pool).await.repository()?;
    ensure_emby_library(&mut tx, media_source_id, library_id, library_item_id).await?;
    let library_path_id =
        ensure_library_path(&mut tx, library_id, &format!("emby://{library_id}")).await?;
    let id = ensure_emby_show_item(&mut tx, library_path_id, item).await?;
    upsert_show_metadata(&mut tx, id, item).await?;
    tx.commit().await.repository()?;
    Ok(id)
}

pub(super) async fn upsert_emby_season(
    pool: &DbPool,
    media_source_id: i64,
    library_id: i64,
    library_item_id: &str,
    show_id: i64,
    item: &EmbySeasonItem,
) -> Result<i64> {
    let mut tx = crate::db::begin_write(pool).await.repository()?;
    ensure_emby_library(&mut tx, media_source_id, library_id, library_item_id).await?;
    let library_path_id =
        ensure_library_path(&mut tx, library_id, &format!("emby://{library_id}")).await?;
    let id = ensure_emby_season_item(&mut tx, library_path_id, show_id, item).await?;
    upsert_season_metadata(&mut tx, id, item).await?;
    tx.commit().await.repository()?;
    Ok(id)
}

#[allow(
    clippy::too_many_arguments,
    reason = "Retains provider leaf context and its genuine managed scan claim"
)]
pub(super) async fn upsert_emby_episode(
    pool: &DbPool,
    media_source_id: i64,
    library_id: i64,
    library_item_id: &str,
    season_id: i64,
    item: &EmbyMediaItem,
    playback: &EmbyPlaybackMetadata,
    claim: Option<&ersatzrs_core::repositories::ManagedLibraryOperationClaim>,
) -> Result<()> {
    let mut teletext = crate::media_server::teletext::MappedTeletextWrite::acquire(
        pool,
        claim,
        crate::media_server::teletext::MappedTeletextSource {
            provider: ersatzrs_core::repositories::RemoteMediaSourceKind::Emby,
            id: media_source_id,
        },
        library_id,
        &item.path,
        playback.source_declaration.as_ref(),
        &playback.streams,
    )
    .await?;
    let mut tx = crate::db::begin_write(pool).await.repository()?;
    teletext.check_claim(&mut tx, library_id).await?;
    ensure_emby_library(&mut tx, media_source_id, library_id, library_item_id).await?;
    let library_path_id =
        ensure_library_path(&mut tx, library_id, &format!("emby://{library_id}")).await?;
    let item_id = ensure_emby_episode_item(&mut tx, library_path_id, season_id, item).await?;
    let episode_metadata_id = upsert_episode_metadata(&mut tx, item_id, item).await?;
    let version_id =
        ensure_media_version(&mut tx, VersionOwner::Episode(item_id), item, playback).await?;
    ensure_media_file(
        &mut tx,
        VersionOwner::Episode(item_id),
        version_id,
        &item.path,
        playback.source_declaration.as_ref(),
        &playback.streams,
    )
    .await?;
    replace_streams(&mut tx, version_id, &playback.streams).await?;
    // Port of MediaServerTelevisionLibraryScanner.UpdateSubtitles
    // (ErsatzTV.Scanner/Core/Metadata/MediaServerTelevisionLibraryScanner.cs:832):
    // populate Subtitle rows so a later deep-scan extraction can query them.
    crate::metadata::update_episode_subtitles(&mut tx, episode_metadata_id, &playback.streams)
        .await?;
    replace_chapters(&mut tx, version_id, &item.chapters).await?;
    teletext.persist(&mut tx, library_id, version_id).await?;
    tx.commit().await.repository()?;
    Ok(())
}

pub(super) async fn upsert_emby_collection(
    tx: &mut Transaction<'_, crate::db::Db>,
    media_source_id: i64,
    item: &EmbyCollectionItem,
) -> Result<i64> {
    crate::provider_collections::upsert_tx(
        tx,
        crate::collection_tags::Provider::Emby,
        media_source_id,
        &item.item_id,
        item.etag.as_deref(),
        &item.name,
    )
    .await
}

pub(super) async fn replace_collection_items(
    tx: &mut Transaction<'_, crate::db::Db>,
    collection_id: i64,
    media_source_id: i64,
    items: &[EmbyCollectionMediaItem],
) -> Result<()> {
    let previous = crate::collection_tags::members(tx, collection_id).await?;
    let mut incoming = Vec::new();
    for item in items {
        if let Some(id) = find_emby_media_item(tx, media_source_id, item).await? {
            incoming.push(id);
        }
    }
    crate::provider_collections::replace_members(tx, collection_id, &incoming).await?;
    crate::collection_tags::reconcile(
        tx,
        crate::collection_tags::Provider::Emby,
        collection_id,
        &previous,
    )
    .await?;
    Ok(())
}

pub(super) async fn find_emby_media_item(
    tx: &mut Transaction<'_, crate::db::Db>,
    media_source_id: i64,
    item: &EmbyCollectionMediaItem,
) -> Result<Option<i64>> {
    let table = match item.kind {
        EmbyCollectionMediaKind::Movie => "EmbyMovie",
        EmbyCollectionMediaKind::Show => "EmbyShow",
        EmbyCollectionMediaKind::Season => "EmbySeason",
        EmbyCollectionMediaKind::Episode => "EmbyEpisode",
    };
    let sql = format!(
        r#"SELECT p."Id" FROM "{table}" p
        JOIN "MediaItem" m ON m."Id" = p."Id"
        JOIN "LibraryPath" path ON path."Id" = m."LibraryPathId"
        JOIN "Library" l ON l."Id" = path."LibraryId"
        WHERE p."ItemId" = ? AND l."MediaSourceId" = ?"#
    );
    Ok(sqlx::query(crate::db::assert_sql_safe(&sql))
        .bind(&item.item_id)
        .bind(media_source_id)
        .fetch_optional(&mut **tx)
        .await
        .repository()?
        .map(|row| row.get("Id")))
}

/// Synchronizes the set of Emby libraries for a media source.
///
/// Adds incoming libraries that have no `ItemId` match, deletes existing
/// libraries that the server no longer reports (cascading through
/// `Library`/`MediaItem`/`LibraryPath`), and refreshes the `MediaKind`,
/// `Name`, and `EmbyPathInfo` rows for libraries that the server still
/// reports. Mirrors `MediaSourceRepository.UpdateLibraries` for Emby in
/// `ErsatzTV`. The `MediaKind` is only updated when `ShouldSyncItems` is
/// false to avoid invalidating an in-progress sync.
///
/// # Errors
///
/// Returns an error when the database write fails.
pub async fn sync_emby_libraries(
    pool: &DbPool,
    media_source_id: i64,
    incoming: &[EmbyLibrarySummary],
) -> Result<()> {
    let existing_rows = sqlx::query(
        r#"SELECT "el"."Id" AS "Id", "el"."ItemId" AS "ItemId",
                  "el"."ShouldSyncItems" AS "ShouldSyncItems",
                  "l"."MediaKind" AS "MediaKind"
           FROM "EmbyLibrary" AS "el"
           JOIN "Library" AS "l" ON "l"."Id" = "el"."Id"
           WHERE "l"."MediaSourceId" = ?"#,
    )
    .bind(media_source_id)
    .fetch_all(pool)
    .await
    .repository()?;

    let mut existing: Vec<(i64, Option<String>, bool, i32)> = existing_rows
        .into_iter()
        .map(|row| {
            (
                row.get::<i64, _>("Id"),
                row.try_get::<Option<String>, _>("ItemId")
                    .unwrap_or_default(),
                row.try_get::<i64, _>("ShouldSyncItems").unwrap_or_default() != 0,
                row.try_get::<i32, _>("MediaKind").unwrap_or_default(),
            )
        })
        .collect();

    let mut tx = crate::db::begin_write(pool).await.repository()?;
    for incoming_library in incoming {
        let position = existing.iter().position(|(_, item_id, _, _)| {
            item_id.as_deref() == Some(incoming_library.item_id.as_str())
        });
        if let Some(index) = position {
            let (id, _, should_sync, current_kind) = existing.swap_remove(index);
            let target_kind = encode_library_media_kind(incoming_library.media_kind);
            if !should_sync && target_kind != current_kind {
                sqlx::query(r#"UPDATE "Library" SET "MediaKind" = ?, "Name" = ? WHERE "Id" = ?"#)
                    .bind(target_kind)
                    .bind(&incoming_library.name)
                    .bind(id)
                    .execute(&mut *tx)
                    .await
                    .repository()?;
            } else {
                sqlx::query(r#"UPDATE "Library" SET "Name" = ? WHERE "Id" = ?"#)
                    .bind(&incoming_library.name)
                    .bind(id)
                    .execute(&mut *tx)
                    .await
                    .repository()?;
            }
            replace_emby_path_infos_in_tx(&mut tx, id, &incoming_library.path_infos).await?;
        } else {
            let result = sqlx::query(
                r#"INSERT INTO "Library" ("Name", "MediaKind", "MediaSourceId")
                   VALUES (?, ?, ?)"#,
            )
            .bind(&incoming_library.name)
            .bind(encode_library_media_kind(incoming_library.media_kind))
            .bind(media_source_id)
            .execute(&mut *tx)
            .await
            .repository()?;
            let id = crate::db::last_insert_id(result);
            sqlx::query(
                r#"INSERT INTO "EmbyLibrary" ("Id", "ItemId", "ShouldSyncItems")
                   VALUES (?, ?, 0)"#,
            )
            .bind(id)
            .bind(&incoming_library.item_id)
            .execute(&mut *tx)
            .await
            .repository()?;
            sqlx::query(r#"INSERT INTO "LibraryPath" ("LibraryId", "Path") VALUES (?, ?)"#)
                .bind(id)
                .bind(format!("emby://{id}"))
                .execute(&mut *tx)
                .await
                .repository()?;
            replace_emby_path_infos_in_tx(&mut tx, id, &incoming_library.path_infos).await?;
        }
    }

    for (id, _, _, _) in existing {
        sqlx::query(r#"DELETE FROM "Library" WHERE "Id" = ?"#)
            .bind(id)
            .execute(&mut *tx)
            .await
            .repository()?;
    }

    tx.commit().await.repository()?;
    Ok(())
}

pub(super) fn encode_library_media_kind(kind: LibraryMediaKind) -> i32 {
    match kind {
        LibraryMediaKind::Movies => 1,
        LibraryMediaKind::Shows => 2,
        LibraryMediaKind::MusicVideos => 3,
        LibraryMediaKind::OtherVideos => 4,
        LibraryMediaKind::Songs => 5,
        LibraryMediaKind::Images => 6,
        LibraryMediaKind::RemoteStreams => 7,
    }
}

pub(super) async fn replace_emby_path_infos_in_tx(
    tx: &mut Transaction<'_, crate::db::Db>,
    library_id: i64,
    path_infos: &[EmbyPathInfo],
) -> Result<()> {
    sqlx::query(r#"DELETE FROM "EmbyPathInfo" WHERE "EmbyLibraryId" = ?"#)
        .bind(library_id)
        .execute(&mut **tx)
        .await
        .repository()?;
    for info in path_infos {
        sqlx::query(
            r#"INSERT INTO "EmbyPathInfo" ("EmbyLibraryId", "Path", "NetworkPath")
               VALUES (?, ?, ?)"#,
        )
        .bind(library_id)
        .bind(&info.path)
        .bind(&info.network_path)
        .execute(&mut **tx)
        .await
        .repository()?;
    }
    Ok(())
}

/// Replaces all `EmbyPathInfo` rows for a library with the supplied set.
///
/// Used to mirror `ErsatzTV` path mapping for SMB/NFS shares where the Emby
/// server reports its local path and the network path that `ErsatzRS` should
/// rewrite onto. Network-only path infos with empty `NetworkPath` are
/// filtered out by the projection.
///
/// # Errors
///
/// Returns an error when the database write fails.
pub async fn replace_emby_path_infos(
    pool: &DbPool,
    library_id: i64,
    path_infos: &[EmbyPathInfo],
) -> Result<()> {
    let mut tx = crate::db::begin_write(pool).await.repository()?;
    sqlx::query(r#"DELETE FROM "EmbyPathInfo" WHERE "EmbyLibraryId" = ?"#)
        .bind(library_id)
        .execute(&mut *tx)
        .await
        .repository()?;
    for info in path_infos {
        sqlx::query(
            r#"INSERT INTO "EmbyPathInfo" ("EmbyLibraryId", "Path", "NetworkPath")
               VALUES (?, ?, ?)"#,
        )
        .bind(library_id)
        .bind(&info.path)
        .bind(&info.network_path)
        .execute(&mut *tx)
        .await
        .repository()?;
    }
    tx.commit().await.repository()?;
    Ok(())
}

pub(super) async fn ensure_emby_library(
    tx: &mut Transaction<'_, crate::db::Db>,
    media_source_id: i64,
    library_id: i64,
    library_item_id: &str,
) -> Result<()> {
    sqlx::query(r#"INSERT OR IGNORE INTO "MediaSource" ("Id") VALUES (?)"#)
        .bind(media_source_id)
        .execute(&mut **tx)
        .await
        .repository()?;
    sqlx::query(r#"INSERT OR IGNORE INTO "EmbyMediaSource" ("Id") VALUES (?)"#)
        .bind(media_source_id)
        .execute(&mut **tx)
        .await
        .repository()?;
    sqlx::query(
        r#"INSERT OR IGNORE INTO "Library" ("Id", "Name", "MediaKind", "MediaSourceId")
           VALUES (?, 'Emby', 0, ?)"#,
    )
    .bind(library_id)
    .bind(media_source_id)
    .execute(&mut **tx)
    .await
    .repository()?;
    sqlx::query(
        r#"INSERT OR IGNORE INTO "EmbyLibrary" ("Id", "ItemId", "ShouldSyncItems")
           VALUES (?, ?, 1)"#,
    )
    .bind(library_id)
    .bind(library_item_id)
    .execute(&mut **tx)
    .await
    .repository()?;
    Ok(())
}

pub(super) async fn ensure_library_path(
    tx: &mut Transaction<'_, crate::db::Db>,
    library_id: i64,
    path: &str,
) -> Result<i64> {
    if let Some(row) = sqlx::query(r#"SELECT "Id" FROM "LibraryPath" WHERE "LibraryId" = ?"#)
        .bind(library_id)
        .fetch_optional(&mut **tx)
        .await
        .repository()?
    {
        return Ok(row.get("Id"));
    }
    let result = sqlx::query(r#"INSERT INTO "LibraryPath" ("LibraryId", "Path") VALUES (?, ?)"#)
        .bind(library_id)
        .bind(path)
        .execute(&mut **tx)
        .await
        .repository()?;
    Ok(crate::db::last_insert_id(result))
}

pub(super) async fn ensure_emby_movie_item(
    tx: &mut Transaction<'_, crate::db::Db>,
    library_path_id: i64,
    item: &EmbyMediaItem,
) -> Result<i64> {
    if let Some(row) = sqlx::query(
        r#"SELECT "EmbyMovie"."Id"
           FROM "EmbyMovie"
           INNER JOIN "MediaItem" ON "MediaItem"."Id" = "EmbyMovie"."Id"
           INNER JOIN "LibraryPath" existing_path
             ON existing_path."Id" = "MediaItem"."LibraryPathId"
           INNER JOIN "LibraryPath" current_path ON current_path."Id" = ?
           WHERE "EmbyMovie"."ItemId" = ?
             AND existing_path."LibraryId" = current_path."LibraryId""#,
    )
    .bind(library_path_id)
    .bind(&item.item_id)
    .fetch_optional(&mut **tx)
    .await
    .repository()?
    {
        let id = row.get("Id");
        sqlx::query(r#"UPDATE "EmbyMovie" SET "Etag" = ? WHERE "Id" = ?"#)
            .bind(&item.etag)
            .bind(id)
            .execute(&mut **tx)
            .await
            .repository()?;
        sqlx::query(r#"UPDATE "MediaItem" SET "LibraryPathId" = ?, "State" = 0 WHERE "Id" = ?"#)
            .bind(library_path_id)
            .bind(id)
            .execute(&mut **tx)
            .await
            .repository()?;
        return Ok(id);
    }

    let result = sqlx::query(r#"INSERT INTO "MediaItem" ("LibraryPathId", "State") VALUES (?, 0)"#)
        .bind(library_path_id)
        .execute(&mut **tx)
        .await
        .repository()?;
    let id = crate::db::last_insert_id(result);
    sqlx::query(r#"INSERT INTO "Movie" ("Id") VALUES (?)"#)
        .bind(id)
        .execute(&mut **tx)
        .await
        .repository()?;
    sqlx::query(r#"INSERT INTO "EmbyMovie" ("Id", "ItemId", "Etag") VALUES (?, ?, ?)"#)
        .bind(id)
        .bind(&item.item_id)
        .bind(&item.etag)
        .execute(&mut **tx)
        .await
        .repository()?;
    Ok(id)
}

pub(super) async fn upsert_movie_metadata(
    tx: &mut Transaction<'_, crate::db::Db>,
    movie_id: i64,
    item: &EmbyMediaItem,
) -> Result<i64> {
    let existing_id: Option<i64> =
        sqlx::query_scalar(r#"SELECT "Id" FROM "MovieMetadata" WHERE "MovieId" = ? LIMIT 1"#)
            .bind(movie_id)
            .fetch_optional(&mut **tx)
            .await
            .repository()?;
    let now = Utc::now();

    let metadata_id = if let Some(id) = existing_id {
        sqlx::query(
            r#"UPDATE "MovieMetadata"
               SET "ContentRating" = ?, "DateAdded" = ?, "DateUpdated" = ?,
                   "MetadataKind" = 2, "OriginalTitle" = NULL, "Outline" = NULL,
                   "Plot" = ?, "ReleaseDate" = ?, "SortTitle" = ?,
                   "Tagline" = ?, "Title" = ?, "Year" = ?
               WHERE "Id" = ?"#,
        )
        .bind(&item.content_rating)
        .bind(encode_datetime(item.date_created))
        .bind(encode_datetime(now))
        .bind(&item.plot)
        .bind(item.release_date.map(encode_datetime))
        .bind(&item.sort_title)
        .bind(&item.tagline)
        .bind(&item.name)
        .bind(item.year)
        .bind(id)
        .execute(&mut **tx)
        .await
        .repository()?;
        id
    } else {
        let result = sqlx::query(
            r#"INSERT INTO "MovieMetadata" (
                "ContentRating", "DateAdded", "DateUpdated", "MetadataKind",
                "MovieId", "OriginalTitle", "Outline", "Plot", "ReleaseDate",
                "SortTitle", "Tagline", "Title", "Year"
            ) VALUES (?, ?, ?, 2, ?, NULL, NULL, ?, ?, ?, ?, ?, ?)"#,
        )
        .bind(&item.content_rating)
        .bind(encode_datetime(item.date_created))
        .bind(encode_datetime(now))
        .bind(movie_id)
        .bind(&item.plot)
        .bind(item.release_date.map(encode_datetime))
        .bind(&item.sort_title)
        .bind(&item.tagline)
        .bind(&item.name)
        .bind(item.year)
        .execute(&mut **tx)
        .await
        .repository()?;
        crate::db::last_insert_id(result)
    };

    replace_movie_artwork(tx, metadata_id, item).await?;
    replace_movie_guids(tx, metadata_id, &item.provider_guids).await?;
    crate::metadata::replace_strings(tx, "Genre", "MovieMetadataId", metadata_id, &item.genres)
        .await?;
    crate::metadata::replace_strings(tx, "Tag", "MovieMetadataId", metadata_id, &item.tags).await?;
    replace_movie_actors(tx, metadata_id, &item.actors).await?;
    replace_movie_people(tx, "Director", metadata_id, &item.directors).await?;
    replace_movie_people(tx, "Writer", metadata_id, &item.writers).await?;
    Ok(metadata_id)
}

pub(super) async fn replace_movie_actors(
    tx: &mut Transaction<'_, crate::db::Db>,
    metadata_id: i64,
    actors: &[EmbyActor],
) -> Result<()> {
    let people: Vec<crate::metadata::PersonRef> = actors
        .iter()
        .map(|actor| crate::metadata::PersonRef {
            name: actor.name.clone(),
            role: actor.role.clone(),
            order: actor.order,
        })
        .collect();
    crate::metadata::replace_people(tx, "Actor", "MovieMetadataId", metadata_id, &people).await
}

pub(super) async fn replace_movie_people(
    tx: &mut Transaction<'_, crate::db::Db>,
    table: &'static str,
    metadata_id: i64,
    names: &[String],
) -> Result<()> {
    let people: Vec<crate::metadata::PersonRef> = names
        .iter()
        .map(|name| crate::metadata::PersonRef::new(name.clone()))
        .collect();
    crate::metadata::replace_people(tx, table, "MovieMetadataId", metadata_id, &people).await
}

pub(super) async fn replace_movie_artwork(
    tx: &mut Transaction<'_, crate::db::Db>,
    metadata_id: i64,
    item: &EmbyMediaItem,
) -> Result<()> {
    let mut items = Vec::with_capacity(2);
    if let Some(path) = &item.poster_artwork_path {
        items.push(crate::metadata::ArtworkRef {
            kind: crate::metadata::artwork_kind::POSTER,
            path: path.clone(),
            date_added: item.date_created,
        });
    }
    if let Some(path) = &item.fanart_artwork_path {
        items.push(crate::metadata::ArtworkRef {
            kind: crate::metadata::artwork_kind::FAN_ART,
            path: path.clone(),
            date_added: item.date_created,
        });
    }
    crate::metadata::replace_artwork(
        tx,
        "MovieMetadataId",
        metadata_id,
        &[
            crate::metadata::artwork_kind::POSTER,
            crate::metadata::artwork_kind::FAN_ART,
        ],
        &items,
    )
    .await
}

pub(super) async fn replace_movie_guids(
    tx: &mut Transaction<'_, crate::db::Db>,
    metadata_id: i64,
    guids: &[String],
) -> Result<()> {
    crate::metadata::replace_guids(tx, "MovieMetadataId", metadata_id, guids).await
}

pub(super) async fn upsert_show_metadata(
    tx: &mut Transaction<'_, crate::db::Db>,
    show_id: i64,
    item: &EmbyShowItem,
) -> Result<()> {
    let existing_id: Option<i64> =
        sqlx::query_scalar(r#"SELECT "Id" FROM "ShowMetadata" WHERE "ShowId" = ? LIMIT 1"#)
            .bind(show_id)
            .fetch_optional(&mut **tx)
            .await
            .repository()?;
    let now = Utc::now();

    let metadata_id = if let Some(id) = existing_id {
        sqlx::query(
            r#"UPDATE "ShowMetadata"
               SET "ContentRating" = ?, "DateAdded" = ?, "DateUpdated" = ?,
                   "MetadataKind" = 2, "OriginalTitle" = NULL, "Outline" = NULL,
                   "Plot" = ?, "ReleaseDate" = ?, "SortTitle" = ?,
                   "Tagline" = ?, "Title" = ?, "Year" = ?
               WHERE "Id" = ?"#,
        )
        .bind(&item.content_rating)
        .bind(encode_datetime(item.date_created))
        .bind(encode_datetime(now))
        .bind(&item.plot)
        .bind(item.release_date.map(encode_datetime))
        .bind(&item.sort_title)
        .bind(&item.tagline)
        .bind(&item.name)
        .bind(item.year)
        .bind(id)
        .execute(&mut **tx)
        .await
        .repository()?;
        id
    } else {
        let result = sqlx::query(
            r#"INSERT INTO "ShowMetadata" (
                "ContentRating", "DateAdded", "DateUpdated", "MetadataKind",
                "ShowId", "OriginalTitle", "Outline", "Plot", "ReleaseDate",
                "SortTitle", "Tagline", "Title", "Year"
            ) VALUES (?, ?, ?, 2, ?, NULL, NULL, ?, ?, ?, ?, ?, ?)"#,
        )
        .bind(&item.content_rating)
        .bind(encode_datetime(item.date_created))
        .bind(encode_datetime(now))
        .bind(show_id)
        .bind(&item.plot)
        .bind(item.release_date.map(encode_datetime))
        .bind(&item.sort_title)
        .bind(&item.tagline)
        .bind(&item.name)
        .bind(item.year)
        .execute(&mut **tx)
        .await
        .repository()?;
        crate::db::last_insert_id(result)
    };

    replace_show_artwork(tx, metadata_id, item).await?;
    crate::metadata::replace_guids(tx, "ShowMetadataId", metadata_id, &item.provider_guids).await?;
    crate::metadata::replace_strings(tx, "Genre", "ShowMetadataId", metadata_id, &item.genres)
        .await?;
    crate::metadata::replace_strings(tx, "Tag", "ShowMetadataId", metadata_id, &item.tags).await?;
    crate::metadata::replace_strings(tx, "Studio", "ShowMetadataId", metadata_id, &item.studios)
        .await?;
    replace_show_actors(tx, metadata_id, &item.actors).await?;
    Ok(())
}

pub(super) async fn replace_show_artwork(
    tx: &mut Transaction<'_, crate::db::Db>,
    metadata_id: i64,
    item: &EmbyShowItem,
) -> Result<()> {
    let mut items = Vec::with_capacity(3);
    if let Some(path) = &item.poster_artwork_path {
        items.push(crate::metadata::ArtworkRef {
            kind: crate::metadata::artwork_kind::POSTER,
            path: path.clone(),
            date_added: item.date_created,
        });
    }
    if let Some(path) = &item.thumb_artwork_path {
        items.push(crate::metadata::ArtworkRef {
            kind: crate::metadata::artwork_kind::THUMBNAIL,
            path: path.clone(),
            date_added: item.date_created,
        });
    }
    if let Some(path) = &item.fanart_artwork_path {
        items.push(crate::metadata::ArtworkRef {
            kind: crate::metadata::artwork_kind::FAN_ART,
            path: path.clone(),
            date_added: item.date_created,
        });
    }
    crate::metadata::replace_artwork(
        tx,
        "ShowMetadataId",
        metadata_id,
        &[
            crate::metadata::artwork_kind::POSTER,
            crate::metadata::artwork_kind::THUMBNAIL,
            crate::metadata::artwork_kind::FAN_ART,
        ],
        &items,
    )
    .await
}

pub(super) async fn replace_show_actors(
    tx: &mut Transaction<'_, crate::db::Db>,
    metadata_id: i64,
    actors: &[EmbyActor],
) -> Result<()> {
    let people: Vec<crate::metadata::PersonRef> = actors
        .iter()
        .map(|actor| crate::metadata::PersonRef {
            name: actor.name.clone(),
            role: actor.role.clone(),
            order: actor.order,
        })
        .collect();
    crate::metadata::replace_people(tx, "Actor", "ShowMetadataId", metadata_id, &people).await
}

pub(super) async fn upsert_season_metadata(
    tx: &mut Transaction<'_, crate::db::Db>,
    season_id: i64,
    item: &EmbySeasonItem,
) -> Result<()> {
    let existing_id: Option<i64> =
        sqlx::query_scalar(r#"SELECT "Id" FROM "SeasonMetadata" WHERE "SeasonId" = ? LIMIT 1"#)
            .bind(season_id)
            .fetch_optional(&mut **tx)
            .await
            .repository()?;
    let now = Utc::now();

    let metadata_id = if let Some(id) = existing_id {
        sqlx::query(
            r#"UPDATE "SeasonMetadata"
               SET "DateAdded" = ?, "DateUpdated" = ?, "MetadataKind" = 2,
                   "OriginalTitle" = NULL, "Outline" = NULL, "ReleaseDate" = ?,
                   "SortTitle" = ?, "Title" = ?, "Year" = ?
               WHERE "Id" = ?"#,
        )
        .bind(encode_datetime(item.date_created))
        .bind(encode_datetime(now))
        .bind(item.release_date.map(encode_datetime))
        .bind(&item.sort_title)
        .bind(&item.name)
        .bind(item.year)
        .bind(id)
        .execute(&mut **tx)
        .await
        .repository()?;
        id
    } else {
        let result = sqlx::query(
            r#"INSERT INTO "SeasonMetadata" (
                "DateAdded", "DateUpdated", "MetadataKind", "SeasonId",
                "OriginalTitle", "Outline", "ReleaseDate", "SortTitle", "Title", "Year"
            ) VALUES (?, ?, 2, ?, NULL, NULL, ?, ?, ?, ?)"#,
        )
        .bind(encode_datetime(item.date_created))
        .bind(encode_datetime(now))
        .bind(season_id)
        .bind(item.release_date.map(encode_datetime))
        .bind(&item.sort_title)
        .bind(&item.name)
        .bind(item.year)
        .execute(&mut **tx)
        .await
        .repository()?;
        crate::db::last_insert_id(result)
    };

    replace_season_artwork(tx, metadata_id, item).await?;
    crate::metadata::replace_guids(tx, "SeasonMetadataId", metadata_id, &item.provider_guids)
        .await?;
    Ok(())
}

pub(super) async fn replace_season_artwork(
    tx: &mut Transaction<'_, crate::db::Db>,
    metadata_id: i64,
    item: &EmbySeasonItem,
) -> Result<()> {
    let mut items = Vec::with_capacity(2);
    if let Some(path) = &item.poster_artwork_path {
        items.push(crate::metadata::ArtworkRef {
            kind: crate::metadata::artwork_kind::POSTER,
            path: path.clone(),
            date_added: item.date_created,
        });
    }
    if let Some(path) = &item.fanart_artwork_path {
        items.push(crate::metadata::ArtworkRef {
            kind: crate::metadata::artwork_kind::FAN_ART,
            path: path.clone(),
            date_added: item.date_created,
        });
    }
    crate::metadata::replace_artwork(
        tx,
        "SeasonMetadataId",
        metadata_id,
        &[
            crate::metadata::artwork_kind::POSTER,
            crate::metadata::artwork_kind::FAN_ART,
        ],
        &items,
    )
    .await
}

pub(super) async fn upsert_episode_metadata(
    tx: &mut Transaction<'_, crate::db::Db>,
    episode_id: i64,
    item: &EmbyMediaItem,
) -> Result<i64> {
    let existing_id: Option<i64> =
        sqlx::query_scalar(r#"SELECT "Id" FROM "EpisodeMetadata" WHERE "EpisodeId" = ? LIMIT 1"#)
            .bind(episode_id)
            .fetch_optional(&mut **tx)
            .await
            .repository()?;
    let now = Utc::now();
    let episode_number = item.episode_number.unwrap_or_default();

    let metadata_id = if let Some(id) = existing_id {
        sqlx::query(
            r#"UPDATE "EpisodeMetadata"
               SET "DateAdded" = ?, "DateUpdated" = ?, "EpisodeNumber" = ?,
                   "MetadataKind" = 2, "OriginalTitle" = NULL, "Outline" = NULL,
                   "Plot" = ?, "ReleaseDate" = ?, "SortTitle" = ?,
                   "Tagline" = ?, "Title" = ?, "Year" = ?
               WHERE "Id" = ?"#,
        )
        .bind(encode_datetime(item.date_created))
        .bind(encode_datetime(now))
        .bind(episode_number)
        .bind(&item.plot)
        .bind(item.release_date.map(encode_datetime))
        .bind(&item.sort_title)
        .bind(&item.tagline)
        .bind(&item.name)
        .bind(item.year)
        .bind(id)
        .execute(&mut **tx)
        .await
        .repository()?;
        id
    } else {
        let result = sqlx::query(
            r#"INSERT INTO "EpisodeMetadata" (
                "DateAdded", "DateUpdated", "EpisodeNumber", "EpisodeId",
                "MetadataKind", "OriginalTitle", "Outline", "Plot", "ReleaseDate",
                "SortTitle", "Tagline", "Title", "Year"
            ) VALUES (?, ?, ?, ?, 2, NULL, NULL, ?, ?, ?, ?, ?, ?)"#,
        )
        .bind(encode_datetime(item.date_created))
        .bind(encode_datetime(now))
        .bind(episode_number)
        .bind(episode_id)
        .bind(&item.plot)
        .bind(item.release_date.map(encode_datetime))
        .bind(&item.sort_title)
        .bind(&item.tagline)
        .bind(&item.name)
        .bind(item.year)
        .execute(&mut **tx)
        .await
        .repository()?;
        crate::db::last_insert_id(result)
    };

    replace_episode_artwork(tx, metadata_id, item).await?;
    crate::metadata::replace_guids(tx, "EpisodeMetadataId", metadata_id, &item.provider_guids)
        .await?;
    crate::metadata::replace_strings(tx, "Genre", "EpisodeMetadataId", metadata_id, &item.genres)
        .await?;
    crate::metadata::replace_strings(tx, "Tag", "EpisodeMetadataId", metadata_id, &item.tags)
        .await?;
    replace_episode_actors(tx, metadata_id, &item.actors).await?;
    replace_episode_people(tx, "Director", metadata_id, &item.directors).await?;
    replace_episode_people(tx, "Writer", metadata_id, &item.writers).await?;
    Ok(metadata_id)
}

pub(super) async fn replace_episode_artwork(
    tx: &mut Transaction<'_, crate::db::Db>,
    metadata_id: i64,
    item: &EmbyMediaItem,
) -> Result<()> {
    let mut items = Vec::with_capacity(1);
    if let Some(path) = &item.thumb_artwork_path {
        items.push(crate::metadata::ArtworkRef {
            kind: crate::metadata::artwork_kind::THUMBNAIL,
            path: path.clone(),
            date_added: item.date_created,
        });
    }
    crate::metadata::replace_artwork(
        tx,
        "EpisodeMetadataId",
        metadata_id,
        &[crate::metadata::artwork_kind::THUMBNAIL],
        &items,
    )
    .await
}

pub(super) async fn replace_episode_actors(
    tx: &mut Transaction<'_, crate::db::Db>,
    metadata_id: i64,
    actors: &[EmbyActor],
) -> Result<()> {
    let people: Vec<crate::metadata::PersonRef> = actors
        .iter()
        .map(|actor| crate::metadata::PersonRef {
            name: actor.name.clone(),
            role: actor.role.clone(),
            order: actor.order,
        })
        .collect();
    crate::metadata::replace_people(tx, "Actor", "EpisodeMetadataId", metadata_id, &people).await
}

pub(super) async fn replace_episode_people(
    tx: &mut Transaction<'_, crate::db::Db>,
    table: &'static str,
    metadata_id: i64,
    names: &[String],
) -> Result<()> {
    let people: Vec<crate::metadata::PersonRef> = names
        .iter()
        .map(|name| crate::metadata::PersonRef::new(name.clone()))
        .collect();
    crate::metadata::replace_people(tx, table, "EpisodeMetadataId", metadata_id, &people).await
}

pub(super) async fn ensure_emby_show_item(
    tx: &mut Transaction<'_, crate::db::Db>,
    library_path_id: i64,
    item: &EmbyShowItem,
) -> Result<i64> {
    if let Some(row) = sqlx::query(
        r#"SELECT "EmbyShow"."Id"
           FROM "EmbyShow"
           INNER JOIN "MediaItem" ON "MediaItem"."Id" = "EmbyShow"."Id"
           INNER JOIN "LibraryPath" existing_path
             ON existing_path."Id" = "MediaItem"."LibraryPathId"
           INNER JOIN "LibraryPath" current_path ON current_path."Id" = ?
           WHERE "EmbyShow"."ItemId" = ?
             AND existing_path."LibraryId" = current_path."LibraryId""#,
    )
    .bind(library_path_id)
    .bind(&item.item_id)
    .fetch_optional(&mut **tx)
    .await
    .repository()?
    {
        let id = row.get("Id");
        sqlx::query(r#"UPDATE "EmbyShow" SET "Etag" = ? WHERE "Id" = ?"#)
            .bind(&item.etag)
            .bind(id)
            .execute(&mut **tx)
            .await
            .repository()?;
        sqlx::query(r#"UPDATE "MediaItem" SET "LibraryPathId" = ?, "State" = 0 WHERE "Id" = ?"#)
            .bind(library_path_id)
            .bind(id)
            .execute(&mut **tx)
            .await
            .repository()?;
        return Ok(id);
    }

    let result = sqlx::query(r#"INSERT INTO "MediaItem" ("LibraryPathId", "State") VALUES (?, 0)"#)
        .bind(library_path_id)
        .execute(&mut **tx)
        .await
        .repository()?;
    let id = crate::db::last_insert_id(result);
    sqlx::query(r#"INSERT INTO "Show" ("Id") VALUES (?)"#)
        .bind(id)
        .execute(&mut **tx)
        .await
        .repository()?;
    sqlx::query(r#"INSERT INTO "EmbyShow" ("Id", "ItemId", "Etag") VALUES (?, ?, ?)"#)
        .bind(id)
        .bind(&item.item_id)
        .bind(&item.etag)
        .execute(&mut **tx)
        .await
        .repository()?;
    Ok(id)
}

pub(super) async fn ensure_emby_season_item(
    tx: &mut Transaction<'_, crate::db::Db>,
    library_path_id: i64,
    show_id: i64,
    item: &EmbySeasonItem,
) -> Result<i64> {
    if let Some(row) = sqlx::query(
        r#"SELECT "EmbySeason"."Id"
           FROM "EmbySeason"
           INNER JOIN "MediaItem" ON "MediaItem"."Id" = "EmbySeason"."Id"
           INNER JOIN "LibraryPath" existing_path
             ON existing_path."Id" = "MediaItem"."LibraryPathId"
           INNER JOIN "LibraryPath" current_path ON current_path."Id" = ?
           WHERE "EmbySeason"."ItemId" = ?
             AND existing_path."LibraryId" = current_path."LibraryId""#,
    )
    .bind(library_path_id)
    .bind(&item.item_id)
    .fetch_optional(&mut **tx)
    .await
    .repository()?
    {
        let id = row.get("Id");
        sqlx::query(r#"UPDATE "EmbySeason" SET "Etag" = ? WHERE "Id" = ?"#)
            .bind(&item.etag)
            .bind(id)
            .execute(&mut **tx)
            .await
            .repository()?;
        sqlx::query(r#"UPDATE "Season" SET "ShowId" = ?, "SeasonNumber" = ? WHERE "Id" = ?"#)
            .bind(show_id)
            .bind(item.season_number)
            .bind(id)
            .execute(&mut **tx)
            .await
            .repository()?;
        sqlx::query(r#"UPDATE "MediaItem" SET "LibraryPathId" = ?, "State" = 0 WHERE "Id" = ?"#)
            .bind(library_path_id)
            .bind(id)
            .execute(&mut **tx)
            .await
            .repository()?;
        return Ok(id);
    }

    let result = sqlx::query(r#"INSERT INTO "MediaItem" ("LibraryPathId", "State") VALUES (?, 0)"#)
        .bind(library_path_id)
        .execute(&mut **tx)
        .await
        .repository()?;
    let id = crate::db::last_insert_id(result);
    sqlx::query(r#"INSERT INTO "Season" ("Id", "ShowId", "SeasonNumber") VALUES (?, ?, ?)"#)
        .bind(id)
        .bind(show_id)
        .bind(item.season_number)
        .execute(&mut **tx)
        .await
        .repository()?;
    sqlx::query(r#"INSERT INTO "EmbySeason" ("Id", "ItemId", "Etag") VALUES (?, ?, ?)"#)
        .bind(id)
        .bind(&item.item_id)
        .bind(&item.etag)
        .execute(&mut **tx)
        .await
        .repository()?;
    Ok(id)
}

pub(super) async fn ensure_emby_episode_item(
    tx: &mut Transaction<'_, crate::db::Db>,
    library_path_id: i64,
    season_id: i64,
    item: &EmbyMediaItem,
) -> Result<i64> {
    if let Some(row) = sqlx::query(
        r#"SELECT "EmbyEpisode"."Id"
           FROM "EmbyEpisode"
           INNER JOIN "MediaItem" ON "MediaItem"."Id" = "EmbyEpisode"."Id"
           INNER JOIN "LibraryPath" existing_path
             ON existing_path."Id" = "MediaItem"."LibraryPathId"
           INNER JOIN "LibraryPath" current_path ON current_path."Id" = ?
           WHERE "EmbyEpisode"."ItemId" = ?
             AND existing_path."LibraryId" = current_path."LibraryId""#,
    )
    .bind(library_path_id)
    .bind(&item.item_id)
    .fetch_optional(&mut **tx)
    .await
    .repository()?
    {
        let id = row.get("Id");
        sqlx::query(r#"UPDATE "EmbyEpisode" SET "Etag" = ? WHERE "Id" = ?"#)
            .bind(&item.etag)
            .bind(id)
            .execute(&mut **tx)
            .await
            .repository()?;
        sqlx::query(r#"UPDATE "Episode" SET "SeasonId" = ? WHERE "Id" = ?"#)
            .bind(season_id)
            .bind(id)
            .execute(&mut **tx)
            .await
            .repository()?;
        sqlx::query(r#"UPDATE "MediaItem" SET "LibraryPathId" = ?, "State" = 0 WHERE "Id" = ?"#)
            .bind(library_path_id)
            .bind(id)
            .execute(&mut **tx)
            .await
            .repository()?;
        return Ok(id);
    }

    let result = sqlx::query(r#"INSERT INTO "MediaItem" ("LibraryPathId", "State") VALUES (?, 0)"#)
        .bind(library_path_id)
        .execute(&mut **tx)
        .await
        .repository()?;
    let id = crate::db::last_insert_id(result);
    sqlx::query(r#"INSERT INTO "Episode" ("Id", "SeasonId") VALUES (?, ?)"#)
        .bind(id)
        .bind(season_id)
        .execute(&mut **tx)
        .await
        .repository()?;
    sqlx::query(r#"INSERT INTO "EmbyEpisode" ("Id", "ItemId", "Etag") VALUES (?, ?, ?)"#)
        .bind(id)
        .bind(&item.item_id)
        .bind(&item.etag)
        .execute(&mut **tx)
        .await
        .repository()?;
    Ok(id)
}

impl VersionMediaItem for EmbyMediaItem {
    fn date_created(&self) -> DateTime<Utc> {
        self.date_created
    }
}

impl VersionPlayback for EmbyPlaybackMetadata {
    fn duration(&self) -> Duration {
        self.duration
    }

    fn width(&self) -> i32 {
        self.width
    }

    fn height(&self) -> i32 {
        self.height
    }

    fn sample_aspect_ratio(&self) -> Option<&str> {
        self.sample_aspect_ratio.as_deref()
    }

    fn display_aspect_ratio(&self) -> Option<&str> {
        self.display_aspect_ratio.as_deref()
    }

    fn r_frame_rate(&self) -> Option<&str> {
        self.r_frame_rate.as_deref()
    }
}
