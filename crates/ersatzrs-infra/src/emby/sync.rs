use super::*;

/// Fetch Emby movie pages and write mapped movie rows.
///
/// # Errors
///
/// Returns an error when the HTTP client fails or any database write fails.
pub async fn sync_emby_movie_library<S>(
    pool: &DbPool,
    service: &S,
    config: &EmbyConnectionConfig,
    media_source_id: i64,
    library_id: i64,
    library_item_id: &str,
    deep_scan: bool,
) -> Result<usize>
where
    S: EmbyServiceTrait,
{
    sync_emby_movie_library_with_progress(
        pool,
        service,
        config,
        media_source_id,
        library_id,
        library_item_id,
        deep_scan,
        None,
    )
    .await
}

/// Variant of [`sync_emby_movie_library`] that emits
/// `(synced / total_record_count)` after each item upsert.
///
/// # Errors
///
/// Returns an error when the HTTP client fails or any database write fails.
#[allow(
    clippy::too_many_arguments,
    clippy::too_many_lines,
    clippy::cast_precision_loss,
    clippy::cast_possible_truncation
)]
pub async fn sync_emby_movie_library_with_progress<S>(
    pool: &DbPool,
    service: &S,
    config: &EmbyConnectionConfig,
    media_source_id: i64,
    library_id: i64,
    library_item_id: &str,
    deep_scan: bool,
    progress: Option<&dyn ersatzrs_domain::ScanProgressSink>,
) -> Result<usize>
where
    S: EmbyServiceTrait,
{
    let mut synced = 0;
    let mut pages = PageProgress::new(EMBY_PAGE_SIZE);
    let mut present_item_ids = std::collections::HashSet::new();
    let mut variant_candidates = Vec::new();

    loop {
        let page = service
            .fetch_movie_page(config, library_item_id, pages.start_index(), EMBY_PAGE_SIZE)
            .await?;
        let total_record_count = page.total_record_count;
        let page_len = i32::try_from(page.items.len()).unwrap_or(i32::MAX);
        pages.report_initial(progress);
        for item in &page.items {
            present_item_ids.insert(item.item_id.clone());
            let item_id = if !deep_scan
                && item_etag_matches(
                    pool,
                    ProviderItemTable::EmbyMovie,
                    library_id,
                    &item.item_id,
                    item.etag.as_deref(),
                )
                .await?
            {
                synced += 1;
                crate::media_server::provider_media_item_id(
                    pool,
                    "EmbyMovie",
                    "ItemId",
                    library_id,
                    &item.item_id,
                )
                .await
                .repository()?
            } else {
                let playback = service.stream_metadata(config, &item.item_id).await?;
                let item_id = upsert_emby_movie(
                    pool,
                    media_source_id,
                    library_id,
                    library_item_id,
                    item,
                    &playback,
                )
                .await?;
                synced += 1;
                Some(item_id)
            };
            if let Some(item_id) = item_id {
                variant_candidates.push(crate::media_server::ProviderVariantCandidate {
                    item_id,
                    title: item.name.clone(),
                    path: Some(item.path.clone()),
                    disambiguator: String::new(),
                    tags: item.tags.clone(),
                });
            }
            PageProgress::report(progress, synced, total_record_count);
        }

        if !pages.advance(page_len, total_record_count) {
            break;
        }
    }

    // Trash movies no longer present on the server, then flag those still on
    // the server whose local file is missing (RemoteOnly — Emby supports
    // remote streaming).
    crate::media_server::flag_missing_media_server_items(
        pool,
        "EmbyMovie",
        "ItemId",
        library_id,
        &present_item_ids,
    )
    .await
    .repository()?;
    let replacements = crate::media_server::load_media_server_path_replacements(
        pool,
        "EmbyPathReplacement",
        "EmbyPath",
        "EmbyMediaSourceId",
        media_source_id,
    )
    .await
    .repository()?;
    crate::media_server::flag_unavailable_media_server_items(
        pool,
        "EmbyMovie",
        "ItemId",
        library_id,
        &present_item_ids,
        &replacements,
        true,
    )
    .await
    .repository()?;
    crate::media_server::apply_provider_content_variants(pool, &variant_candidates)
        .await
        .repository()?;

    if let Some(sink) = progress {
        sink.report(1.0);
    }
    Ok(synced)
}

/// Fetch Emby shows, seasons, and episodes and write mapped media rows.
///
/// # Errors
///
/// Returns an error when the HTTP client fails or any database write fails.
pub async fn sync_emby_show_library<S>(
    pool: &DbPool,
    service: &S,
    config: &EmbyConnectionConfig,
    media_source_id: i64,
    library_id: i64,
    library_item_id: &str,
    deep_scan: bool,
) -> Result<usize>
where
    S: EmbyServiceTrait,
{
    sync_emby_show_library_with_progress(
        pool,
        service,
        config,
        media_source_id,
        library_id,
        library_item_id,
        deep_scan,
        None,
    )
    .await
}

/// Variant of [`sync_emby_show_library`] that reports show-level progress
/// (`shows_completed / show_total_count`).
///
/// # Errors
///
/// Returns an error when the HTTP client fails or any database write fails.
#[allow(
    clippy::too_many_arguments,
    clippy::too_many_lines,
    clippy::cast_precision_loss,
    clippy::cast_possible_truncation
)]
pub async fn sync_emby_show_library_with_progress<S>(
    pool: &DbPool,
    service: &S,
    config: &EmbyConnectionConfig,
    media_source_id: i64,
    library_id: i64,
    library_item_id: &str,
    deep_scan: bool,
    progress: Option<&dyn ersatzrs_domain::ScanProgressSink>,
) -> Result<usize>
where
    S: EmbyServiceTrait,
{
    let mut synced = 0;
    let mut shows_completed: usize = 0;
    let mut pages = PageProgress::new(EMBY_PAGE_SIZE);
    let mut present_shows = std::collections::HashSet::new();
    let mut present_seasons = std::collections::HashSet::new();
    let mut present_episodes = std::collections::HashSet::new();

    loop {
        let shows = service
            .fetch_show_page(config, library_item_id, pages.start_index(), EMBY_PAGE_SIZE)
            .await?;
        let show_total_count = shows.total_record_count;
        let show_page_len = i32::try_from(shows.items.len()).unwrap_or(i32::MAX);
        pages.report_initial(progress);
        for show in &shows.items {
            present_shows.insert(show.item_id.clone());
            synced += sync_one_emby_show(
                pool,
                service,
                config,
                media_source_id,
                library_id,
                library_item_id,
                show,
                deep_scan,
                &mut present_seasons,
                &mut present_episodes,
            )
            .await?;
            shows_completed += 1;
            PageProgress::report(progress, shows_completed, show_total_count);
        }

        if !pages.advance(show_page_len, show_total_count) {
            break;
        }
    }

    // Trash shows/seasons/episodes no longer present on the server.
    for (table, present) in [
        ("EmbyShow", &present_shows),
        ("EmbySeason", &present_seasons),
        ("EmbyEpisode", &present_episodes),
    ] {
        crate::media_server::flag_missing_media_server_items(
            pool, table, "ItemId", library_id, present,
        )
        .await
        .repository()?;
    }
    // Flag episodes still on the server whose local file is missing
    // (RemoteOnly — Emby supports remote streaming).
    let replacements = crate::media_server::load_media_server_path_replacements(
        pool,
        "EmbyPathReplacement",
        "EmbyPath",
        "EmbyMediaSourceId",
        media_source_id,
    )
    .await
    .repository()?;
    crate::media_server::flag_unavailable_media_server_items(
        pool,
        "EmbyEpisode",
        "ItemId",
        library_id,
        &present_episodes,
        &replacements,
        true,
    )
    .await
    .repository()?;

    if let Some(sink) = progress {
        sink.report(1.0);
    }
    Ok(synced)
}

#[allow(clippy::too_many_arguments)]
async fn sync_one_emby_show<S>(
    pool: &DbPool,
    service: &S,
    config: &EmbyConnectionConfig,
    media_source_id: i64,
    library_id: i64,
    library_item_id: &str,
    show: &EmbyShowItem,
    deep_scan: bool,
    present_seasons: &mut std::collections::HashSet<String>,
    present_episodes: &mut std::collections::HashSet<String>,
) -> Result<usize>
where
    S: EmbyServiceTrait,
{
    // Ports the body of
    // `$ERSATZTV_SRC/ErsatzTV.Scanner/Core/Emby/EmbyTelevisionLibraryScanner.cs::ScanSingleShowInternal`.
    let mut synced = 0;
    let show_id =
        upsert_emby_show(pool, media_source_id, library_id, library_item_id, show).await?;
    let mut season_start_index = 0;
    loop {
        let seasons = service
            .fetch_season_page(config, &show.item_id, season_start_index, EMBY_PAGE_SIZE)
            .await?;
        let season_total_count = seasons.total_record_count;
        let season_page_len = i32::try_from(seasons.items.len()).unwrap_or(i32::MAX);
        for season in &seasons.items {
            present_seasons.insert(season.item_id.clone());
            let season_id = upsert_emby_season(
                pool,
                media_source_id,
                library_id,
                library_item_id,
                show_id,
                season,
            )
            .await?;
            let mut episode_start_index = 0;
            loop {
                let episodes = service
                    .fetch_episode_page(
                        config,
                        &show.item_id,
                        &season.item_id,
                        episode_start_index,
                        EMBY_PAGE_SIZE,
                    )
                    .await?;
                let episode_total_count = episodes.total_record_count;
                let episode_page_len = i32::try_from(episodes.items.len()).unwrap_or(i32::MAX);
                for episode in &episodes.items {
                    present_episodes.insert(episode.item_id.clone());
                    if !deep_scan
                        && item_etag_matches(
                            pool,
                            ProviderItemTable::EmbyEpisode,
                            library_id,
                            &episode.item_id,
                            episode.etag.as_deref(),
                        )
                        .await?
                    {
                        synced += 1;
                        continue;
                    }

                    let playback = service.stream_metadata(config, &episode.item_id).await?;
                    upsert_emby_episode(
                        pool,
                        media_source_id,
                        library_id,
                        library_item_id,
                        season_id,
                        episode,
                        &playback,
                    )
                    .await?;
                    synced += 1;
                }

                episode_start_index += EMBY_PAGE_SIZE;
                if episode_start_index >= episode_total_count || episode_page_len == 0 {
                    break;
                }
            }
        }

        season_start_index += EMBY_PAGE_SIZE;
        if season_start_index >= season_total_count || season_page_len == 0 {
            break;
        }
    }

    Ok(synced)
}

/// Fetch Emby collections and write collection membership rows.
///
/// # Errors
///
/// Returns an error when the HTTP client fails or any database write fails.
pub async fn sync_emby_collection_library<S>(
    pool: &DbPool,
    service: &S,
    config: &EmbyConnectionConfig,
    media_source_id: i64,
    parent_id: &str,
) -> Result<usize>
where
    S: EmbyServiceTrait,
{
    sync_emby_collection_library_with_progress(
        pool,
        service,
        config,
        media_source_id,
        parent_id,
        None,
    )
    .await
}

/// Variant of [`sync_emby_collection_library`] that emits
/// `(synced / total_record_count)` after each collection upsert.
///
/// # Errors
///
/// Returns an error when the HTTP client fails or any database write fails.
#[allow(clippy::cast_precision_loss, clippy::cast_possible_truncation)]
pub async fn sync_emby_collection_library_with_progress<S>(
    pool: &DbPool,
    service: &S,
    config: &EmbyConnectionConfig,
    media_source_id: i64,
    parent_id: &str,
    progress: Option<&dyn ersatzrs_domain::ScanProgressSink>,
) -> Result<usize>
where
    S: EmbyServiceTrait,
{
    use crate::provider_collections::{invalid_discovery, request, CompletePages};
    let mut pages = CompletePages::new(EMBY_PAGE_SIZE);
    let mut gathered = Vec::new();
    let mut keys = std::collections::HashSet::new();
    if let Some(sink) = progress {
        sink.report(0.0);
    }
    loop {
        let page = request(service.fetch_collection_page(
            config,
            parent_id,
            pages.start(),
            EMBY_PAGE_SIZE,
        ))
        .await?;
        let more = pages.advance(page.total_record_count, page.received_count)?;
        if page.received_count != page.items.len() {
            return Err(invalid_discovery());
        }
        for collection in page.items {
            if !keys.insert(collection.item_id.clone()) {
                return Err(invalid_discovery());
            }
            let mut member_pages = CompletePages::new(EMBY_PAGE_SIZE);
            let mut all_items = Vec::new();
            loop {
                let items = request(service.fetch_collection_items_page(
                    config,
                    &collection.item_id,
                    member_pages.start(),
                    EMBY_PAGE_SIZE,
                ))
                .await?;
                let more_members =
                    member_pages.advance(items.total_record_count, items.received_count)?;
                all_items.extend(items.items);
                if !more_members {
                    break;
                }
            }
            gathered.push((collection, all_items));
        }
        if more {
            PageProgress::report(progress, gathered.len(), page.total_record_count);
        }
        if !more {
            break;
        }
    }
    let dependencies = crate::repositories::SqliteSchedulingContentRepository::new(pool.clone())
        .classic_media_dependencies()
        .await?;
    let mut tx = crate::db::begin_write(pool).await.repository()?;
    let before = crate::provider_collection_refresh::PublicationState::capture(
        &mut tx,
        crate::collection_tags::Provider::Emby,
        media_source_id,
    )
    .await?;
    let mut discovered = Vec::new();
    for (collection, items) in &gathered {
        let id = upsert_emby_collection(&mut tx, media_source_id, collection).await?;
        replace_collection_items(&mut tx, id, media_source_id, items).await?;
        discovered.push(id);
    }
    crate::provider_collections::reconcile_root(
        &mut tx,
        crate::collection_tags::Provider::Emby,
        media_source_id,
        parent_id,
        &discovered,
    )
    .await?;
    let after = crate::provider_collection_refresh::PublicationState::capture(
        &mut tx,
        crate::collection_tags::Provider::Emby,
        media_source_id,
    )
    .await?;
    before
        .persist_changes(&mut tx, after, &dependencies)
        .await?;
    tx.commit().await.repository()?;
    if let Some(sink) = progress {
        sink.report(1.0);
    }
    Ok(discovered.len())
}

/// Loads configuration from the database and synchronizes one Emby library.
///
/// Dispatches to movie or show sync based on `MediaKind`.
///
/// # Errors
///
/// Returns an error when config loading, the Emby API, or database writes fail.
pub async fn sync_library_by_id(pool: &DbPool, library_id: i64) -> Result<usize> {
    sync_library_by_id_with_options(pool, library_id, false).await
}

/// Loads configuration from the database and synchronizes one Emby library.
///
/// `deep_scan` bypasses provider etag skip and refreshes existing rows even
/// when the upstream etag is unchanged.
///
/// # Errors
///
/// Returns an error when config loading, the Emby API, or database writes fail.
pub async fn sync_library_by_id_with_options(
    pool: &DbPool,
    library_id: i64,
    deep_scan: bool,
) -> Result<usize> {
    sync_library_by_id_with_progress(pool, library_id, deep_scan, None).await
}

/// Variant of [`sync_library_by_id_with_options`] that forwards a progress
/// sink to the underlying branch fn.
///
/// # Errors
///
/// Returns an error when config loading, the Emby API, or database writes
/// fail.
pub async fn sync_library_by_id_with_progress(
    pool: &DbPool,
    library_id: i64,
    deep_scan: bool,
    progress: Option<&dyn ersatzrs_domain::ScanProgressSink>,
) -> Result<usize> {
    let row = sqlx::query(
        r#"SELECT l."MediaKind", l."MediaSourceId"
           FROM "Library" l WHERE l."Id" = ? LIMIT 1"#,
    )
    .bind(library_id)
    .fetch_optional(pool)
    .await
    .repository()?
    .ok_or_else(|| RepositoryError::Decode {
        kind: "emby library",
        value: format!("library {library_id} not found"),
    })?;
    let kind: LibraryMediaKind = decode_enum_column("emby library media kind", &row, "MediaKind")?;
    let media_source_id: i64 = row.try_get("MediaSourceId").unwrap_or(0);

    let address: String = sqlx::query_scalar(
        r#"SELECT "Address" FROM "EmbyConnection"
           WHERE "EmbyMediaSourceId" = ? ORDER BY "Id" LIMIT 1"#,
    )
    .bind(media_source_id)
    .fetch_optional(pool)
    .await
    .repository()?
    .and_then(|v: Option<String>| v)
    .ok_or_else(|| RepositoryError::Decode {
        kind: "emby connection",
        value: format!("no connection for source {media_source_id}"),
    })?;

    let api_key = load_emby_api_key(pool, &address).await?;
    let config = EmbyConnectionConfig { address, api_key };

    let parent_id: String =
        sqlx::query_scalar(r#"SELECT "ItemId" FROM "EmbyLibrary" WHERE "Id" = ?"#)
            .bind(library_id)
            .fetch_optional(pool)
            .await
            .repository()?
            .and_then(|v: Option<String>| v)
            .ok_or_else(|| RepositoryError::Decode {
                kind: "emby library",
                value: format!("EmbyLibrary row {library_id} not found"),
            })?;

    let service = EmbyService::new();
    if kind == LibraryMediaKind::Shows {
        sync_emby_show_library_with_progress(
            pool,
            &service,
            &config,
            media_source_id,
            library_id,
            &parent_id,
            deep_scan,
            progress,
        )
        .await
    } else {
        sync_emby_movie_library_with_progress(
            pool,
            &service,
            &config,
            media_source_id,
            library_id,
            &parent_id,
            deep_scan,
            progress,
        )
        .await
    }
}

/// Synchronize one Emby show by the local `Show.Id`.
///
/// Mirrors ErsatzTV's `SynchronizeEmbyShowByIdHandler`: resolve the active
/// Emby connection/api key, map the local show id to the provider item id +
/// title via the Emby show row, fetch the single show via the `/Items`
/// endpoint, then scan that show's seasons and episodes without library
/// cleanup.
/// C# source:
/// `$ERSATZTV_SRC/ErsatzTV.Scanner/Application/Emby/Commands/SynchronizeEmbyShowByIdHandler.cs`
/// and
/// `$ERSATZTV_SRC/ErsatzTV.Infrastructure/Data/Repositories/EmbyTelevisionRepository.cs::GetShowTitleItemId`.
///
/// # Errors
///
/// Returns an error when config loading, show lookup, Emby requests, or
/// database writes fail.
pub async fn sync_emby_show_by_id(
    pool: &DbPool,
    library_id: i64,
    show_id: i64,
    deep_scan: bool,
    progress: Option<&dyn ersatzrs_domain::ScanProgressSink>,
) -> Result<usize> {
    let target = load_target_emby_show_scan(pool, library_id, show_id).await?;
    if !target.should_sync_items {
        tracing::warn!(
            library_id,
            show_id,
            show_title = %target.show_title,
            "Emby show scan skipped because library item sync is disabled"
        );
        return Ok(0);
    }

    let config = load_active_emby_connection_config(pool, target.media_source_id).await?;
    let service = EmbyService::new();

    if let Some(sink) = progress {
        sink.report(0.0);
    }

    let Some(show) = service
        .fetch_single_show(&config, &target.library_item_id, &target.show_item_id)
        .await?
    else {
        tracing::warn!(
            library_id,
            show_id,
            show_item_id = %target.show_item_id,
            show_title = %target.show_title,
            "No Emby show found during targeted scan"
        );
        if let Some(sink) = progress {
            sink.report(1.0);
        }
        return Ok(0);
    };

    tracing::info!(
        library_id,
        show_id,
        show_item_id = %show.item_id,
        show_title = %target.show_title,
        deep_scan,
        "Starting targeted Emby show scan"
    );

    // Targeted single-show scan does no library-level cleanup (matching
    // ErsatzTV), so the present-id collectors are discarded here.
    let synced = sync_one_emby_show(
        pool,
        &service,
        &config,
        target.media_source_id,
        library_id,
        &target.library_item_id,
        &show,
        deep_scan,
        &mut std::collections::HashSet::new(),
        &mut std::collections::HashSet::new(),
    )
    .await?;

    if let Some(sink) = progress {
        sink.report(1.0);
    }
    Ok(synced)
}

struct TargetEmbyShowScan {
    media_source_id: i64,
    library_item_id: String,
    should_sync_items: bool,
    show_item_id: String,
    show_title: String,
}

async fn load_target_emby_show_scan(
    pool: &DbPool,
    library_id: i64,
    show_id: i64,
) -> Result<TargetEmbyShowScan> {
    let row = sqlx::query(
        r#"SELECT l."MediaKind",
                  l."MediaSourceId",
                  el."ItemId" AS "LibraryItemId",
                  el."ShouldSyncItems",
                  esh."ItemId" AS "ShowItemId",
                  COALESCE(
                      (SELECT sm."Title"
                       FROM "ShowMetadata" sm
                       WHERE sm."ShowId" = esh."Id"
                       ORDER BY sm."Id"
                       LIMIT 1),
                      'Unknown Show'
                  ) AS "ShowTitle"
           FROM "Library" l
           JOIN "EmbyLibrary" el ON el."Id" = l."Id"
           JOIN "EmbyShow" esh ON esh."Id" = ?
           JOIN "MediaItem" mi ON mi."Id" = esh."Id"
           JOIN "LibraryPath" lp ON lp."Id" = mi."LibraryPathId"
           WHERE l."Id" = ? AND lp."LibraryId" = l."Id"
           LIMIT 1"#,
    )
    .bind(show_id)
    .bind(library_id)
    .fetch_optional(pool)
    .await
    .repository()?
    .ok_or_else(|| RepositoryError::Decode {
        kind: "emby show",
        value: format!("show {show_id} does not exist in library {library_id}"),
    })?;

    let kind: LibraryMediaKind = decode_enum_column("emby library media kind", &row, "MediaKind")?;
    if kind != LibraryMediaKind::Shows {
        return Err(RepositoryError::Decode {
            kind: "emby library",
            value: format!("library {library_id} is not a TV show library"),
        });
    }

    let should_sync_items: i64 = row.try_get("ShouldSyncItems").unwrap_or(1);
    let show_title: String = row
        .try_get("ShowTitle")
        .unwrap_or_else(|_| "Unknown Show".to_owned());
    let media_source_id: i64 = row.try_get("MediaSourceId").unwrap_or(0);
    let library_item_id: String = row.try_get("LibraryItemId").unwrap_or_default();
    let show_item_id: String = row.try_get("ShowItemId").unwrap_or_default();

    Ok(TargetEmbyShowScan {
        media_source_id,
        library_item_id,
        should_sync_items: should_sync_items != 0,
        show_item_id,
        show_title,
    })
}

pub(crate) async fn load_active_emby_connection_config(
    pool: &DbPool,
    media_source_id: i64,
) -> Result<EmbyConnectionConfig> {
    let address: String = sqlx::query_scalar(
        r#"SELECT "Address" FROM "EmbyConnection"
           WHERE "EmbyMediaSourceId" = ? ORDER BY "Id" LIMIT 1"#,
    )
    .bind(media_source_id)
    .fetch_optional(pool)
    .await
    .repository()?
    .and_then(|v: Option<String>| v)
    .ok_or_else(|| RepositoryError::Decode {
        kind: "emby connection",
        value: format!("no connection for source {media_source_id}"),
    })?;

    let api_key = load_emby_api_key(pool, &address).await?;
    Ok(EmbyConnectionConfig { address, api_key })
}

/// Sync Emby collections for every library on `media_source_id` that has
/// `ShouldSyncItems = 1`. Mirrors ETV's `SynchronizeEmbyCollections` command
/// dispatched from the External Collections row on the libraries page.
///
/// `_deep_scan` matches the UI button contract;
/// `sync_emby_collection_library` does not currently distinguish deep from
/// shallow scans, so the flag is reserved for future use.
///
/// # Errors
///
/// Returns an error when config loading, the Emby API, or database writes
/// fail.
pub async fn sync_collections_for_media_source(
    pool: &DbPool,
    media_source_id: i64,
    _deep_scan: bool,
) -> Result<usize> {
    let library_ids: Vec<i64> = sqlx::query_scalar(
        r#"SELECT l."Id"
           FROM "Library" l
           JOIN "EmbyLibrary" el ON el."Id" = l."Id"
           WHERE l."MediaSourceId" = ? AND el."ShouldSyncItems" = 1
           ORDER BY l."Id""#,
    )
    .bind(media_source_id)
    .fetch_all(pool)
    .await
    .repository()?;

    if library_ids.is_empty() {
        return Ok(0);
    }

    let address: String = sqlx::query_scalar(
        r#"SELECT "Address" FROM "EmbyConnection"
           WHERE "EmbyMediaSourceId" = ? ORDER BY "Id" LIMIT 1"#,
    )
    .bind(media_source_id)
    .fetch_optional(pool)
    .await
    .repository()?
    .and_then(|v: Option<String>| v)
    .ok_or_else(|| RepositoryError::Decode {
        kind: "emby connection",
        value: format!("no connection for source {media_source_id}"),
    })?;

    let api_key = load_emby_api_key(pool, &address).await?;
    let config = EmbyConnectionConfig { address, api_key };
    let service = EmbyService::new();

    let mut total = 0;
    for library_id in library_ids {
        let parent_id: Option<String> =
            sqlx::query_scalar(r#"SELECT "ItemId" FROM "EmbyLibrary" WHERE "Id" = ?"#)
                .bind(library_id)
                .fetch_optional(pool)
                .await
                .repository()?
                .and_then(|v: Option<String>| v);
        let Some(parent_id) = parent_id else {
            continue;
        };
        total += sync_emby_collection_library(pool, &service, &config, media_source_id, &parent_id)
            .await?;
    }
    Ok(total)
}

async fn load_emby_api_key(pool: &DbPool, address: &str) -> Result<String> {
    let keys = [
        format!("emby.api_key.{address}"),
        "emby.current_server_api_key".to_owned(),
    ];
    for key in &keys {
        if let Some(v) = sqlx::query_scalar::<_, String>(
            r#"SELECT "Value" FROM "ConfigElement" WHERE "Key" = ?"#,
        )
        .bind(key)
        .fetch_optional(pool)
        .await
        .repository()?
        .filter(|v| !v.is_empty())
        {
            return Ok(v);
        }
    }
    Err(RepositoryError::Decode {
        kind: "emby config",
        value: "api key not found".into(),
    })
}
