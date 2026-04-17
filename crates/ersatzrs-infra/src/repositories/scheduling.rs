use std::collections::{BTreeMap, BTreeSet};

use async_trait::async_trait;
use chrono::{DateTime, Duration, NaiveTime, Utc};
use ersatzrs_core::repositories::Result;
use ersatzrs_core::scheduling::{
    BlockDefinition, BlockTemplate, BlockTemplateItem, BlockTemplateSchedule, EffectiveBlock,
    EffectiveBlockItem, EffectiveBreakContent, EffectiveDeco, EffectiveDecoSelection,
    EffectiveDecoTemplateItem, SchedulingCollectionKey, SchedulingContentRepository,
    SchedulingContentSource, SchedulingMediaItem, SchedulingPlaylistItem, SchedulingReferenceData,
};
use ersatzrs_core::search::{SearchDocument, SearchExpression, SearchProviderGuid};
use ersatzrs_domain::{CollectionType, DecoMode, EntityId, PlayoutHistory, PlayoutItem};
use sqlx::sqlite::SqliteRow;
use sqlx::{Pool, Row, Sqlite};

use super::common::{
    decode_datetime, decode_duration_text, decode_enum_column, decode_message,
    decode_optional_datetime, decode_optional_enum_column, load_ids,
};

#[derive(Clone)]
pub struct SqliteSchedulingContentRepository(pub Pool<Sqlite>);

#[async_trait]
impl SchedulingContentRepository for SqliteSchedulingContentRepository {
    async fn media_items_for_source(
        &self,
        source: &SchedulingContentSource,
    ) -> Result<Vec<SchedulingMediaItem>> {
        self.media_items_for_source(source).await
    }

    async fn playlist_items_for_source(
        &self,
        source: &SchedulingContentSource,
    ) -> Result<Option<Vec<SchedulingPlaylistItem>>> {
        self.playlist_items_for_source(source).await
    }

    async fn watermark_id_by_name(&self, name: &str) -> Result<Option<EntityId>> {
        self.watermark_id_by_name(name).await
    }

    async fn graphics_element_id_by_path(&self, path: &str) -> Result<Option<EntityId>> {
        self.graphics_element_id_by_path(path).await
    }
}

impl SqliteSchedulingContentRepository {
    /// Prepare SQL-backed reference data for block scheduling.
    ///
    /// # Errors
    ///
    /// Returns an error when a `SQLite` query fails or a persisted enum/date
    /// value cannot be decoded.
    pub async fn block_reference_data(
        &self,
        playout_id: EntityId,
        start: DateTime<Utc>,
        days_to_build: i64,
        max_playout_offset: Duration,
    ) -> Result<SchedulingReferenceData> {
        let mut media_by_key = BTreeMap::new();
        let schedules = self
            .block_template_schedules(playout_id, &mut media_by_key)
            .await?;
        let effective_blocks = EffectiveBlock::from_templates(&schedules, start, days_to_build);

        Ok(SchedulingReferenceData {
            media_by_key,
            effective_blocks,
            existing_items: self.playout_items(playout_id).await?,
            existing_history: self.playout_history(playout_id).await?,
            max_playout_offset,
            ..SchedulingReferenceData::default()
        })
    }

    /// Resolve playable media items for a scheduling content source.
    ///
    /// # Errors
    ///
    /// Returns an error when a `SQLite` query fails or a persisted enum/date
    /// value cannot be decoded.
    pub async fn media_items_for_source(
        &self,
        source: &SchedulingContentSource,
    ) -> Result<Vec<SchedulingMediaItem>> {
        match source {
            SchedulingContentSource::DirectKey(_) => Ok(Vec::new()),
            SchedulingContentSource::Collection(name) => self.collection_items_by_name(name).await,
            SchedulingContentSource::MultiCollection(name) => {
                self.multi_collection_items_by_name(name).await
            }
            SchedulingContentSource::SmartCollection(name) => {
                self.smart_collection_items_by_name(name).await
            }
            SchedulingContentSource::Playlist { group, name } => {
                self.playlist_media_items(group.as_deref(), name).await
            }
            SchedulingContentSource::Search(query) => self.search_items(query).await,
            SchedulingContentSource::Show { name, guids } => {
                let mut items = Vec::new();
                let mut seen = BTreeSet::new();

                if let Some(name) = name {
                    for show_id in self.show_ids_by_name(name).await? {
                        let show_items = self.items_for_show_id(show_id).await?;
                        append_unique(&mut items, &mut seen, show_items);
                    }
                }

                for guid in guids {
                    if !seen.is_empty() && name.is_none() {
                        break;
                    }
                    let show_items = self.show_items_by_guid(&guid.source, &guid.value).await?;
                    append_unique(&mut items, &mut seen, show_items);
                }

                Ok(items)
            }
            SchedulingContentSource::Marathon {
                name,
                guids,
                searches,
                ..
            } => {
                let mut items =
                    Box::pin(self.media_items_for_source(&SchedulingContentSource::Show {
                        name: name.clone(),
                        guids: guids.clone(),
                    }))
                    .await?;
                let mut seen = items.iter().map(|item| item.id).collect::<BTreeSet<_>>();
                for query in searches {
                    let search_items = self.search_items(query).await?;
                    append_unique(&mut items, &mut seen, search_items);
                }
                Ok(items)
            }
            SchedulingContentSource::MediaItem(id) => self.media_item_by_id(*id).await,
        }
    }

    /// Resolve structured playlist children for a playlist content source.
    ///
    /// # Errors
    ///
    /// Returns an error when a `SQLite` query fails or a playlist child row
    /// contains an invalid enum, date, or count value.
    pub async fn playlist_items_for_source(
        &self,
        source: &SchedulingContentSource,
    ) -> Result<Option<Vec<SchedulingPlaylistItem>>> {
        let SchedulingContentSource::Playlist { group, name } = source else {
            return Ok(None);
        };

        self.playlist_items(group.as_deref(), name).await.map(Some)
    }

    /// Find the first channel watermark whose name contains the supplied value.
    ///
    /// # Errors
    ///
    /// Returns an error when the `SQLite` lookup fails.
    pub async fn watermark_id_by_name(&self, name: &str) -> Result<Option<EntityId>> {
        sqlx::query_scalar::<_, EntityId>(
            r#"SELECT "Id" FROM "ChannelWatermark" WHERE "Name" LIKE ? ORDER BY "Id" LIMIT 1"#,
        )
        .bind(format!("%{name}%"))
        .fetch_optional(&self.0)
        .await
        .map_err(Into::into)
    }

    /// Find a graphics element by exact or template-relative path.
    ///
    /// # Errors
    ///
    /// Returns an error when the `SQLite` lookup fails.
    pub async fn graphics_element_id_by_path(&self, path: &str) -> Result<Option<EntityId>> {
        let normalized = normalize_graphics_path(path);
        sqlx::query_scalar::<_, EntityId>(
            r#"SELECT "Id"
             FROM "GraphicsElement"
             WHERE "Path" = ? OR "Path" = ? OR "Path" LIKE ?
             ORDER BY "Id"
             LIMIT 1"#,
        )
        .bind(path)
        .bind(&normalized)
        .bind(format!("%/{normalized}"))
        .fetch_optional(&self.0)
        .await
        .map_err(Into::into)
    }

    async fn collection_items_by_name(&self, name: &str) -> Result<Vec<SchedulingMediaItem>> {
        let collection_id = sqlx::query_scalar::<_, EntityId>(
            r#"SELECT "Id" FROM "Collection" WHERE "Name" = ? ORDER BY "Id" LIMIT 1"#,
        )
        .bind(name)
        .fetch_optional(&self.0)
        .await?;

        match collection_id {
            Some(id) => self.items_for_collection_id(id).await,
            None => Ok(Vec::new()),
        }
    }

    async fn items_for_collection_id(
        &self,
        collection_id: EntityId,
    ) -> Result<Vec<SchedulingMediaItem>> {
        let ids = sqlx::query_scalar::<_, EntityId>(
            r#"SELECT "MediaItemId"
             FROM "CollectionItem"
             WHERE "CollectionId" = ?
             ORDER BY "MediaItemId""#,
        )
        .bind(collection_id)
        .fetch_all(&self.0)
        .await?;

        self.expand_media_ids(ids, 0).await
    }

    async fn multi_collection_items_by_name(&self, name: &str) -> Result<Vec<SchedulingMediaItem>> {
        let multi_collection_id = sqlx::query_scalar::<_, EntityId>(
            r#"SELECT "Id" FROM "MultiCollection" WHERE "Name" = ? ORDER BY "Id" LIMIT 1"#,
        )
        .bind(name)
        .fetch_optional(&self.0)
        .await?;

        match multi_collection_id {
            Some(id) => self.items_for_multi_collection_id(id, 0).await,
            None => Ok(Vec::new()),
        }
    }

    async fn items_for_multi_collection_id(
        &self,
        multi_collection_id: EntityId,
        depth: usize,
    ) -> Result<Vec<SchedulingMediaItem>> {
        if depth > 16 {
            return Ok(Vec::new());
        }

        // MultiCollectionItem has composite PK (MultiCollectionId, CollectionId), no id/sort_order
        let collection_ids = sqlx::query_scalar::<_, EntityId>(
            r#"SELECT "CollectionId"
             FROM "MultiCollectionItem"
             WHERE "MultiCollectionId" = ?
             ORDER BY "CollectionId""#,
        )
        .bind(multi_collection_id)
        .fetch_all(&self.0)
        .await?;

        let mut items = Vec::new();
        let mut seen = BTreeSet::new();
        for collection_id in collection_ids {
            let child_items = self.items_for_collection_id(collection_id).await?;
            append_unique(&mut items, &mut seen, child_items);
        }

        // MultiCollectionSmartItem has composite PK (MultiCollectionId, SmartCollectionId)
        let smart_ids = sqlx::query_scalar::<_, EntityId>(
            r#"SELECT "SmartCollectionId"
             FROM "MultiCollectionSmartItem"
             WHERE "MultiCollectionId" = ?
             ORDER BY "SmartCollectionId""#,
        )
        .bind(multi_collection_id)
        .fetch_all(&self.0)
        .await?;

        for smart_id in smart_ids {
            let smart_items = self.items_for_smart_collection_id(smart_id).await?;
            append_unique(&mut items, &mut seen, smart_items);
        }

        Ok(items)
    }

    async fn smart_collection_items_by_name(&self, name: &str) -> Result<Vec<SchedulingMediaItem>> {
        let row = sqlx::query(
            r#"SELECT "Id", "Query" FROM "SmartCollection" WHERE "Name" = ? ORDER BY "Id" LIMIT 1"#,
        )
        .bind(name)
        .fetch_optional(&self.0)
        .await?;

        match row {
            Some(row) => {
                let query = row.try_get::<String, _>("Query")?;
                self.search_items(&query).await
            }
            None => Ok(Vec::new()),
        }
    }

    async fn items_for_smart_collection_id(
        &self,
        smart_collection_id: EntityId,
    ) -> Result<Vec<SchedulingMediaItem>> {
        let query = sqlx::query_scalar::<_, String>(
            r#"SELECT "Query" FROM "SmartCollection" WHERE "Id" = ? ORDER BY "Id" LIMIT 1"#,
        )
        .bind(smart_collection_id)
        .fetch_optional(&self.0)
        .await?;

        match query {
            Some(query) => self.search_items(&query).await,
            None => Ok(Vec::new()),
        }
    }

    async fn playlist_media_items(
        &self,
        group: Option<&str>,
        name: &str,
    ) -> Result<Vec<SchedulingMediaItem>> {
        let playlist_items = Box::pin(self.playlist_items(group, name)).await?;
        let mut items = Vec::new();
        let mut seen = BTreeSet::new();
        for playlist_item in playlist_items {
            append_unique(&mut items, &mut seen, playlist_item.media_items);
        }
        Ok(items)
    }

    async fn playlist_items(
        &self,
        group: Option<&str>,
        name: &str,
    ) -> Result<Vec<SchedulingPlaylistItem>> {
        let playlist_id = if let Some(group) = group {
            sqlx::query_scalar::<_, EntityId>(
                r#"SELECT "p"."Id"
                 FROM "Playlist" AS "p"
                 JOIN "PlaylistGroup" AS "pg" ON "pg"."Id" = "p"."PlaylistGroupId"
                 WHERE "p"."Name" = ? AND "pg"."Name" = ?
                 ORDER BY "p"."Id"
                 LIMIT 1"#,
            )
            .bind(name)
            .bind(group)
            .fetch_optional(&self.0)
            .await?
        } else {
            sqlx::query_scalar::<_, EntityId>(
                r#"SELECT "Id" FROM "Playlist" WHERE "Name" = ? ORDER BY "Id" LIMIT 1"#,
            )
            .bind(name)
            .fetch_optional(&self.0)
            .await?
        };

        let Some(playlist_id) = playlist_id else {
            return Ok(Vec::new());
        };

        let rows = sqlx::query(
            r#"SELECT *
             FROM "PlaylistItem"
             WHERE "PlaylistId" = ?
             ORDER BY "Index", "Id""#,
        )
        .bind(playlist_id)
        .fetch_all(&self.0)
        .await?;

        let mut result = Vec::with_capacity(rows.len());
        for row in rows {
            let media_items = Box::pin(self.items_for_collection_key_row(&row, 0)).await?;
            result.push(SchedulingPlaylistItem {
                collection_key: collection_key_from_row(&row)?,
                playback_order: decode_enum_column(
                    "playlist item playback_order",
                    &row,
                    "PlaybackOrder",
                )?,
                play_all: row.try_get::<bool, _>("PlayAll")?,
                count: row
                    .try_get::<Option<i64>, _>("Count")?
                    .map(usize::try_from)
                    .transpose()
                    .map_err(|error| decode_message("playlist item count", &error.to_string()))?,
                media_items,
            });
        }

        Ok(result)
    }

    async fn items_for_collection_key_row(
        &self,
        row: &SqliteRow,
        depth: usize,
    ) -> Result<Vec<SchedulingMediaItem>> {
        let collection_key = collection_key_from_row(row)?;
        self.items_for_collection_key(&collection_key, depth).await
    }

    async fn playlist_media_items_by_id(
        &self,
        playlist_id: EntityId,
    ) -> Result<Vec<SchedulingMediaItem>> {
        let name =
            sqlx::query_scalar::<_, String>(r#"SELECT "Name" FROM "Playlist" WHERE "Id" = ?"#)
                .bind(playlist_id)
                .fetch_optional(&self.0)
                .await?;

        match name {
            Some(name) => Box::pin(self.playlist_media_items(None, &name)).await,
            None => Ok(Vec::new()),
        }
    }

    async fn media_item_by_id(&self, id: EntityId) -> Result<Vec<SchedulingMediaItem>> {
        self.load_media_item(id)
            .await
            .map(|item| item.into_iter().collect())
    }

    async fn expand_media_ids(
        &self,
        ids: Vec<EntityId>,
        depth: usize,
    ) -> Result<Vec<SchedulingMediaItem>> {
        if depth > 16 {
            return Ok(Vec::new());
        }

        let mut result = Vec::new();
        let mut seen = BTreeSet::new();
        for id in ids {
            let items = if self.is_show(id).await? {
                self.items_for_show_id(id).await?
            } else if self.is_season(id).await? {
                self.items_for_season_id(id).await?
            } else if self.is_artist(id).await? {
                self.items_for_artist_id(id).await?
            } else {
                self.media_item_by_id(id).await?
            };
            append_unique(&mut result, &mut seen, items);
        }
        Ok(result)
    }

    async fn is_show(&self, id: EntityId) -> Result<bool> {
        exists_by_id(&self.0, "Show", id).await
    }

    async fn is_season(&self, id: EntityId) -> Result<bool> {
        exists_by_id(&self.0, "Season", id).await
    }

    async fn is_artist(&self, id: EntityId) -> Result<bool> {
        exists_by_id(&self.0, "Artist", id).await
    }

    async fn items_for_show_id(&self, show_id: EntityId) -> Result<Vec<SchedulingMediaItem>> {
        // Episodes are linked via Season -> Show. Season has SeasonNumber.
        // EpisodeMetadata has EpisodeNumber. We need to join through Season for ordering.
        let ids = sqlx::query_scalar::<_, EntityId>(
            r#"SELECT "e"."Id"
             FROM "Episode" AS "e"
             JOIN "Season" AS "s" ON "s"."Id" = "e"."SeasonId"
             LEFT JOIN "EpisodeMetadata" AS "em" ON "em"."EpisodeId" = "e"."Id"
             WHERE "s"."ShowId" = ?
             ORDER BY "s"."SeasonNumber", "em"."EpisodeNumber", "e"."Id""#,
        )
        .bind(show_id)
        .fetch_all(&self.0)
        .await?;
        self.load_media_items(ids).await
    }

    async fn items_for_season_id(&self, season_id: EntityId) -> Result<Vec<SchedulingMediaItem>> {
        let ids = sqlx::query_scalar::<_, EntityId>(
            r#"SELECT "e"."Id"
             FROM "Episode" AS "e"
             LEFT JOIN "EpisodeMetadata" AS "em" ON "em"."EpisodeId" = "e"."Id"
             WHERE "e"."SeasonId" = ?
             ORDER BY "em"."EpisodeNumber", "e"."Id""#,
        )
        .bind(season_id)
        .fetch_all(&self.0)
        .await?;
        self.load_media_items(ids).await
    }

    async fn items_for_artist_id(&self, artist_id: EntityId) -> Result<Vec<SchedulingMediaItem>> {
        // MusicVideo has ArtistId FK; join through MusicVideoMetadata for ordering
        let ids = sqlx::query_scalar::<_, EntityId>(
            r#"SELECT "mv"."Id"
             FROM "MusicVideo" AS "mv"
             WHERE "mv"."ArtistId" = ?
             ORDER BY "mv"."Id""#,
        )
        .bind(artist_id)
        .fetch_all(&self.0)
        .await?;
        self.load_media_items(ids).await
    }

    async fn show_ids_by_name(&self, name: &str) -> Result<Vec<EntityId>> {
        sqlx::query_scalar::<_, EntityId>(
            r#"SELECT "s"."Id"
             FROM "Show" AS "s"
             JOIN "ShowMetadata" AS "sm" ON "sm"."ShowId" = "s"."Id"
             WHERE "sm"."Title" = ?
             ORDER BY "s"."Id""#,
        )
        .bind(name)
        .fetch_all(&self.0)
        .await
        .map_err(Into::into)
    }

    async fn show_items_by_guid(
        &self,
        source: &str,
        value: &str,
    ) -> Result<Vec<SchedulingMediaItem>> {
        // MetadataGuid has ShowMetadataId FK (nullable); join to ShowMetadata to get ShowId
        let show_id = sqlx::query_scalar::<_, EntityId>(
            r#"SELECT "sm"."ShowId"
             FROM "MetadataGuid" AS "mg"
             JOIN "ShowMetadata" AS "sm" ON "sm"."Id" = "mg"."ShowMetadataId"
             WHERE "mg"."Guid" = ?
             ORDER BY "sm"."ShowId"
             LIMIT 1"#,
        )
        .bind(format!("{source}://{value}"))
        .fetch_optional(&self.0)
        .await?;

        // Also try plain guid format
        let show_id = if show_id.is_none() {
            sqlx::query_scalar::<_, EntityId>(
                r#"SELECT "sm"."ShowId"
                 FROM "MetadataGuid" AS "mg"
                 JOIN "ShowMetadata" AS "sm" ON "sm"."Id" = "mg"."ShowMetadataId"
                 WHERE "mg"."Guid" LIKE ?
                 ORDER BY "sm"."ShowId"
                 LIMIT 1"#,
            )
            .bind(format!("%{value}%"))
            .fetch_optional(&self.0)
            .await?
        } else {
            show_id
        };

        match show_id {
            Some(id) => self.items_for_show_id(id).await,
            None => Ok(Vec::new()),
        }
    }

    async fn search_items(&self, query: &str) -> Result<Vec<SchedulingMediaItem>> {
        let expression = SearchExpression::parse(query);
        let documents = self.search_documents().await?;
        let mut hits = documents
            .into_iter()
            .filter(|document| expression.matches(document))
            .collect::<Vec<_>>();

        hits.sort_by(|left, right| {
            search_sort_title(left)
                .cmp(&search_sort_title(right))
                .then_with(|| left.media_item_id.cmp(&right.media_item_id))
        });

        let mut items = Vec::new();
        let mut seen = BTreeSet::new();
        for hit in hits {
            let hit_items = match hit.media_kind.as_str() {
                "show" => self.items_for_show_id(hit.media_item_id).await?,
                "season" => self.items_for_season_id(hit.media_item_id).await?,
                "artist" => self.items_for_artist_id(hit.media_item_id).await?,
                _ => self.media_item_by_id(hit.media_item_id).await?,
            };
            append_unique(&mut items, &mut seen, hit_items);
        }

        Ok(items)
    }

    async fn search_documents(&self) -> Result<Vec<SearchDocument>> {
        let rows = sqlx::query(SEARCH_DOCUMENT_SQL).fetch_all(&self.0).await?;

        let mut documents = Vec::with_capacity(rows.len());
        for row in rows {
            documents.push(self.search_document_from_row(&row).await?);
        }

        Ok(documents)
    }

    async fn search_document_from_row(&self, row: &SqliteRow) -> Result<SearchDocument> {
        let metadata_kind = row.try_get::<String, _>("metadata_kind")?;
        let metadata_id = row.try_get::<EntityId, _>("metadata_id")?;
        let show_id = row.try_get::<Option<EntityId>, _>("show_id")?;
        let release_date = row
            .try_get::<Option<String>, _>("release_date")?
            .filter(|value| !value.trim().is_empty())
            .map(|value| parse_datetime("search release_date", &value))
            .transpose()?;
        let item_id = row.try_get::<EntityId, _>("item_id")?;
        let stream_languages = self
            .stream_search_fields(&metadata_kind, metadata_id, item_id)
            .await?;

        Ok(SearchDocument {
            media_item_id: item_id,
            media_kind: row.try_get("media_kind")?,
            metadata_kind: metadata_kind.clone(),
            title: row.try_get("title")?,
            sort_title: row.try_get("sort_title")?,
            original_title: row.try_get("original_title")?,
            year: row.try_get("year")?,
            release_date,
            genres: self
                .metadata_lookup_values_fk(
                    "Genre",
                    "MovieMetadataId",
                    "EpisodeMetadataId",
                    "ShowMetadataId",
                    &metadata_kind,
                    metadata_id,
                )
                .await?,
            tags: self
                .metadata_lookup_values_fk(
                    "Tag",
                    "MovieMetadataId",
                    "EpisodeMetadataId",
                    "ShowMetadataId",
                    &metadata_kind,
                    metadata_id,
                )
                .await?,
            studios: self
                .metadata_lookup_values_fk(
                    "Studio",
                    "MovieMetadataId",
                    "EpisodeMetadataId",
                    "ShowMetadataId",
                    &metadata_kind,
                    metadata_id,
                )
                .await?,
            actors: self
                .metadata_actor_values(&metadata_kind, metadata_id)
                .await?,
            directors: self
                .metadata_credit_values("Director", &metadata_kind, metadata_id)
                .await?,
            writers: self
                .metadata_credit_values("Writer", &metadata_kind, metadata_id)
                .await?,
            languages: self
                .language_names_for_codes(&stream_languages.audio_languages)
                .await?,
            language_tags: stream_languages.audio_languages,
            subtitle_languages: self
                .language_names_for_codes(&stream_languages.subtitle_languages)
                .await?,
            subtitle_language_tags: stream_languages.subtitle_languages,
            video_codecs: stream_languages.video_codecs,
            audio_codecs: stream_languages.audio_codecs,
            subtitle_codecs: stream_languages.subtitle_codecs,
            width: stream_languages.width,
            height: stream_languages.height,
            show_title: row.try_get("show_title")?,
            show_genres: self.show_lookup_values("Genre", show_id).await?,
            show_tags: self.show_lookup_values("Tag", show_id).await?,
            show_studios: self.show_lookup_values("Studio", show_id).await?,
            season_number: row.try_get("season_number")?,
            episode_number: row.try_get("episode_number")?,
            provider_guids: self.metadata_guids(&metadata_kind, metadata_id).await?,
        })
    }

    /// Look up names from tables that use nullable FK columns per metadata type.
    /// `ErsatzTV` uses `"Genre"."MovieMetadataId"`, `"Genre"."EpisodeMetadataId"`, etc.
    async fn metadata_lookup_values_fk(
        &self,
        table: &str,
        movie_fk: &str,
        episode_fk: &str,
        show_fk: &str,
        metadata_kind: &str,
        metadata_id: EntityId,
    ) -> Result<Vec<String>> {
        let fk_column = match metadata_kind {
            "Movie" => movie_fk,
            "Episode" => episode_fk,
            "Show" => show_fk,
            "Season" => "SeasonMetadataId",
            "MusicVideo" => "MusicVideoMetadataId",
            "Artist" => "ArtistMetadataId",
            "OtherVideo" => "OtherVideoMetadataId",
            "Song" => "SongMetadataId",
            "Image" => "ImageMetadataId",
            "RemoteStream" => "RemoteStreamMetadataId",
            _ => return Ok(Vec::new()),
        };
        let sql =
            format!(r#"SELECT "Name" FROM "{table}" WHERE "{fk_column}" = ? ORDER BY "Name""#);
        sqlx::query_scalar::<_, String>(&sql)
            .bind(metadata_id)
            .fetch_all(&self.0)
            .await
            .map_err(Into::into)
    }

    async fn metadata_actor_values(
        &self,
        metadata_kind: &str,
        metadata_id: EntityId,
    ) -> Result<Vec<String>> {
        let fk_column = match metadata_kind {
            "Movie" => "MovieMetadataId",
            "Episode" => "EpisodeMetadataId",
            "Show" => "ShowMetadataId",
            "Season" => "SeasonMetadataId",
            "MusicVideo" => "MusicVideoMetadataId",
            "Artist" => "ArtistMetadataId",
            "OtherVideo" => "OtherVideoMetadataId",
            "Song" => "SongMetadataId",
            _ => return Ok(Vec::new()),
        };
        let sql =
            format!(r#"SELECT "Name" FROM "Actor" WHERE "{fk_column}" = ? ORDER BY "Order", "Id""#);
        sqlx::query_scalar::<_, String>(&sql)
            .bind(metadata_id)
            .fetch_all(&self.0)
            .await
            .map_err(Into::into)
    }

    async fn metadata_credit_values(
        &self,
        table: &str,
        metadata_kind: &str,
        metadata_id: EntityId,
    ) -> Result<Vec<String>> {
        let fk_column = match (table, metadata_kind) {
            ("Director" | "Writer", "Movie") => "MovieMetadataId",
            ("Director" | "Writer", "Episode") => "EpisodeMetadataId",
            ("Director", "MusicVideo") => "MusicVideoMetadataId",
            ("Director" | "Writer", "OtherVideo") => "OtherVideoMetadataId",
            _ => return Ok(Vec::new()),
        };
        let sql = format!(r#"SELECT "Name" FROM "{table}" WHERE "{fk_column}" = ? ORDER BY "Id""#);
        sqlx::query_scalar::<_, String>(&sql)
            .bind(metadata_id)
            .fetch_all(&self.0)
            .await
            .map_err(Into::into)
    }

    async fn stream_search_fields(
        &self,
        metadata_kind: &str,
        metadata_id: EntityId,
        item_id: EntityId,
    ) -> Result<StreamSearchFields> {
        let mut result = StreamSearchFields::default();

        let Some(version_fk) = media_version_fk(metadata_kind) else {
            return Ok(result);
        };
        let version_sql = format!(
            r#"SELECT "Id", "Width", "Height"
             FROM "MediaVersion"
             WHERE "{version_fk}" = ?
             ORDER BY "Id""#
        );
        let version_rows = sqlx::query(&version_sql)
            .bind(item_id)
            .fetch_all(&self.0)
            .await?;
        let version_ids = version_rows
            .iter()
            .map(|row| row.try_get::<EntityId, _>("Id"))
            .collect::<std::result::Result<Vec<_>, _>>()?;

        if let Some(row) = version_rows.first() {
            result.width = row.try_get("Width").ok();
            result.height = row.try_get("Height").ok();
        }

        for version_id in version_ids {
            let rows = sqlx::query(
                r#"SELECT "MediaStreamKind", "Codec", "Language"
                 FROM "MediaStream"
                 WHERE "MediaVersionId" = ?
                 ORDER BY "Index", "Id""#,
            )
            .bind(version_id)
            .fetch_all(&self.0)
            .await?;
            for row in rows {
                let kind = media_stream_kind_from_row(&row);
                let codec = row.try_get::<Option<String>, _>("Codec")?;
                let language = row.try_get::<Option<String>, _>("Language")?;
                match kind.as_deref() {
                    Some("video") => push_unique_optional(&mut result.video_codecs, codec),
                    Some("audio") => {
                        push_unique_optional(&mut result.audio_codecs, codec);
                        push_unique_optional(&mut result.audio_languages, language);
                    }
                    Some("subtitle" | "external_subtitle") => {
                        push_unique_optional(&mut result.subtitle_codecs, codec);
                        push_unique_optional(&mut result.subtitle_languages, language);
                    }
                    _ => {}
                }
            }
        }

        let subtitle_fk = metadata_fk(metadata_kind);
        if let Some(subtitle_fk) = subtitle_fk {
            let subtitle_sql = format!(
                r#"SELECT "Codec", "Language"
                 FROM "Subtitle"
                 WHERE "{subtitle_fk}" = ?
                 ORDER BY "StreamIndex", "Id""#
            );
            let rows = sqlx::query(&subtitle_sql)
                .bind(metadata_id)
                .fetch_all(&self.0)
                .await?;
            for row in rows {
                push_unique_optional(
                    &mut result.subtitle_codecs,
                    row.try_get::<Option<String>, _>("Codec")?,
                );
                push_unique_optional(
                    &mut result.subtitle_languages,
                    row.try_get::<Option<String>, _>("Language")?,
                );
            }
        }

        Ok(result)
    }

    async fn language_names_for_codes(&self, codes: &[String]) -> Result<Vec<String>> {
        let mut names = Vec::new();
        for code in codes {
            let rows = sqlx::query_scalar::<_, String>(
                r#"SELECT "EnglishName"
                 FROM "LanguageCode"
                 WHERE "ThreeCode1" = ? OR "ThreeCode2" = ? OR "TwoCode" = ?
                 ORDER BY "Id""#,
            )
            .bind(code)
            .bind(code)
            .bind(code)
            .fetch_all(&self.0)
            .await?;
            for row in rows {
                push_unique(&mut names, row);
            }
            push_unique(&mut names, code.clone());
        }
        Ok(names)
    }

    async fn show_lookup_values(
        &self,
        table: &str,
        show_id: Option<EntityId>,
    ) -> Result<Vec<String>> {
        let Some(show_id) = show_id else {
            return Ok(Vec::new());
        };

        let Some(metadata_id) = sqlx::query_scalar::<_, EntityId>(
            r#"SELECT "Id" FROM "ShowMetadata" WHERE "ShowId" = ?"#,
        )
        .bind(show_id)
        .fetch_optional(&self.0)
        .await?
        else {
            return Ok(Vec::new());
        };

        self.metadata_lookup_values_fk(
            table,
            "MovieMetadataId",
            "EpisodeMetadataId",
            "ShowMetadataId",
            "Show",
            metadata_id,
        )
        .await
    }

    async fn metadata_guids(
        &self,
        metadata_kind: &str,
        metadata_id: EntityId,
    ) -> Result<Vec<SearchProviderGuid>> {
        let fk_column = match metadata_kind {
            "Movie" => "MovieMetadataId",
            "Episode" => "EpisodeMetadataId",
            "Show" => "ShowMetadataId",
            "Season" => "SeasonMetadataId",
            "MusicVideo" => "MusicVideoMetadataId",
            "Artist" => "ArtistMetadataId",
            "OtherVideo" => "OtherVideoMetadataId",
            "Song" => "SongMetadataId",
            "Image" => "ImageMetadataId",
            "RemoteStream" => "RemoteStreamMetadataId",
            _ => return Ok(Vec::new()),
        };
        let sql =
            format!(r#"SELECT "Guid" FROM "MetadataGuid" WHERE "{fk_column}" = ? ORDER BY "Guid""#);
        let guids = sqlx::query_scalar::<_, String>(&sql)
            .bind(metadata_id)
            .fetch_all(&self.0)
            .await?;

        // ErsatzTV stores GUIDs as "source://value" or similar format
        Ok(guids
            .into_iter()
            .map(|guid| {
                if let Some((source, value)) = guid.split_once("://") {
                    SearchProviderGuid {
                        source: source.to_owned(),
                        value: value.to_owned(),
                    }
                } else {
                    SearchProviderGuid {
                        source: String::new(),
                        value: guid,
                    }
                }
            })
            .collect())
    }

    async fn load_media_items(&self, ids: Vec<EntityId>) -> Result<Vec<SchedulingMediaItem>> {
        let mut result = Vec::new();
        for id in ids {
            if let Some(item) = self.load_media_item(id).await? {
                result.push(item);
            }
        }
        Ok(result)
    }

    async fn load_media_item(&self, id: EntityId) -> Result<Option<SchedulingMediaItem>> {
        let row = sqlx::query(
            r#"SELECT
                "mi"."Id",
                COALESCE(
                    (SELECT MAX("mv"."Duration") FROM "MediaVersion" AS "mv" WHERE
                        "mv"."MovieId" = "mi"."Id" OR "mv"."EpisodeId" = "mi"."Id" OR
                        "mv"."MusicVideoId" = "mi"."Id" OR "mv"."OtherVideoId" = "mi"."Id" OR
                        "mv"."SongId" = "mi"."Id" OR "mv"."ImageId" = "mi"."Id" OR
                        "mv"."RemoteStreamId" = "mi"."Id"),
                    '00:00:00'
                ) AS "duration_text",
                COALESCE(
                    (SELECT "em"."Title" FROM "EpisodeMetadata" AS "em" WHERE "em"."EpisodeId" = "mi"."Id" LIMIT 1),
                    (SELECT "mm"."Title" FROM "MovieMetadata" AS "mm" WHERE "mm"."MovieId" = "mi"."Id" LIMIT 1),
                    (SELECT "mvm"."Title" FROM "MusicVideoMetadata" AS "mvm" WHERE "mvm"."MusicVideoId" = "mi"."Id" LIMIT 1),
                    (SELECT "ovm"."Title" FROM "OtherVideoMetadata" AS "ovm" WHERE "ovm"."OtherVideoId" = "mi"."Id" LIMIT 1),
                    (SELECT "som"."Title" FROM "SongMetadata" AS "som" WHERE "som"."SongId" = "mi"."Id" LIMIT 1),
                    (SELECT "shm"."Title" FROM "ShowMetadata" AS "shm" WHERE "shm"."ShowId" = "mi"."Id" LIMIT 1),
                    (SELECT "sem"."Title" FROM "SeasonMetadata" AS "sem" WHERE "sem"."SeasonId" = "mi"."Id" LIMIT 1),
                    (SELECT "am"."Title" FROM "ArtistMetadata" AS "am" WHERE "am"."ArtistId" = "mi"."Id" LIMIT 1)
                ) AS "title",
                (SELECT "s"."ShowId" FROM "Episode" AS "e" JOIN "Season" AS "s" ON "s"."Id" = "e"."SeasonId" WHERE "e"."Id" = "mi"."Id" LIMIT 1) AS "show_id",
                (SELECT "e"."SeasonId" FROM "Episode" AS "e" WHERE "e"."Id" = "mi"."Id" LIMIT 1) AS "season_id",
                (SELECT "s"."SeasonNumber" FROM "Episode" AS "e" JOIN "Season" AS "s" ON "s"."Id" = "e"."SeasonId" WHERE "e"."Id" = "mi"."Id" LIMIT 1) AS "season_number",
                (SELECT "em"."EpisodeNumber" FROM "EpisodeMetadata" AS "em" WHERE "em"."EpisodeId" = "mi"."Id" LIMIT 1) AS "episode_number",
                COALESCE(
                    (SELECT "em"."ReleaseDate" FROM "EpisodeMetadata" AS "em" WHERE "em"."EpisodeId" = "mi"."Id" LIMIT 1),
                    (SELECT "mm"."ReleaseDate" FROM "MovieMetadata" AS "mm" WHERE "mm"."MovieId" = "mi"."Id" LIMIT 1),
                    (SELECT "mvm"."ReleaseDate" FROM "MusicVideoMetadata" AS "mvm" WHERE "mvm"."MusicVideoId" = "mi"."Id" LIMIT 1)
                ) AS "release_date"
             FROM "MediaItem" AS "mi"
             WHERE "mi"."Id" = ?"#,
        )
        .bind(id)
        .fetch_optional(&self.0)
        .await?;

        row.map(|row| map_scheduling_media_item(&row)).transpose()
    }

    async fn block_template_schedules(
        &self,
        playout_id: EntityId,
        media_by_key: &mut BTreeMap<String, Vec<SchedulingMediaItem>>,
    ) -> Result<Vec<BlockTemplateSchedule>> {
        let rows = sqlx::query(
            r#"SELECT *
             FROM "PlayoutTemplate"
             WHERE "PlayoutId" = ?
             ORDER BY "Index", "TemplateId""#,
        )
        .bind(playout_id)
        .fetch_all(&self.0)
        .await?;

        let deco = self
            .effective_deco_for_playout(playout_id, media_by_key)
            .await?;
        let mut schedules = Vec::with_capacity(rows.len());
        for row in rows {
            let template_id = row.try_get::<EntityId, _>("TemplateId")?;
            let template = self.block_template(template_id).await?;
            let mut blocks = BTreeMap::new();
            for item in &template.items {
                if let std::collections::btree_map::Entry::Vacant(entry) =
                    blocks.entry(item.block_id)
                {
                    entry.insert(self.block_definition(item.block_id, media_by_key).await?);
                }
            }
            let deco_template_items = match row.try_get::<Option<EntityId>, _>("DecoTemplateId")? {
                Some(deco_template_id) => {
                    self.effective_deco_template_items(deco_template_id, media_by_key)
                        .await?
                }
                None => Vec::new(),
            };

            // DateUpdated is TEXT in ErsatzTV; parse as i64 (stored as string ticks) or 0
            let updated_ticks: i64 = row
                .try_get::<String, _>("DateUpdated")
                .ok()
                .and_then(|s| s.parse::<i64>().ok())
                .unwrap_or(0);

            schedules.push(BlockTemplateSchedule {
                id: template_id,
                updated_ticks,
                start: None,
                finish: None,
                index: row.try_get("Index")?,
                days_of_week: decode_integer_list(
                    "playout template DaysOfWeek",
                    &row.try_get::<String, _>("DaysOfWeek").unwrap_or_default(),
                )?,
                days_of_month: decode_integer_list(
                    "playout template DaysOfMonth",
                    &row.try_get::<String, _>("DaysOfMonth").unwrap_or_default(),
                )?,
                months_of_year: decode_integer_list(
                    "playout template MonthsOfYear",
                    &row.try_get::<String, _>("MonthsOfYear").unwrap_or_default(),
                )?,
                limit_to_date_range: row.try_get("LimitToDateRange")?,
                start_month: row.try_get("StartMonth")?,
                start_day: row.try_get("StartDay")?,
                start_year: row.try_get("StartYear")?,
                end_month: row.try_get("EndMonth")?,
                end_day: row.try_get("EndDay")?,
                end_year: row.try_get("EndYear")?,
                template,
                blocks,
                deco: deco.clone(),
                deco_template_items,
            });
        }

        Ok(schedules)
    }

    async fn block_template(&self, template_id: EntityId) -> Result<BlockTemplate> {
        let rows = sqlx::query(
            r#"SELECT "Id", "BlockId", "StartTime"
             FROM "TemplateItem"
             WHERE "TemplateId" = ?
             ORDER BY "StartTime", "Id""#,
        )
        .bind(template_id)
        .fetch_all(&self.0)
        .await?;

        let items = rows
            .iter()
            .map(|row| {
                Ok(BlockTemplateItem {
                    id: row.try_get("Id")?,
                    block_id: row.try_get("BlockId")?,
                    start_time: decode_optional_time_of_day_text(
                        "template item StartTime",
                        row.try_get::<Option<String>, _>("StartTime")?,
                    )?,
                    sort_order: 0,
                })
            })
            .collect::<Result<Vec<_>>>()?;

        // DateUpdated is TEXT in ErsatzTV; parse as i64 or 0
        let updated_ticks: i64 = sqlx::query_scalar::<_, String>(
            r#"SELECT "DateUpdated" FROM "Template" WHERE "Id" = ?"#,
        )
        .bind(template_id)
        .fetch_optional(&self.0)
        .await?
        .and_then(|s| s.parse::<i64>().ok())
        .unwrap_or(0);

        Ok(BlockTemplate {
            id: template_id,
            updated_ticks,
            items,
        })
    }

    async fn block_definition(
        &self,
        block_id: EntityId,
        media_by_key: &mut BTreeMap<String, Vec<SchedulingMediaItem>>,
    ) -> Result<BlockDefinition> {
        let row = sqlx::query(
            r#"SELECT "Id", "Name", "DateUpdated", "Minutes", "StopScheduling"
             FROM "Block"
             WHERE "Id" = ?"#,
        )
        .bind(block_id)
        .fetch_one(&self.0)
        .await?;

        // DateUpdated is TEXT; Minutes is INTEGER (minutes, not ms)
        let updated_ticks: i64 = row
            .try_get::<String, _>("DateUpdated")
            .ok()
            .and_then(|s| s.parse::<i64>().ok())
            .unwrap_or(0);
        let minutes: i64 = row.try_get("Minutes")?;
        let duration = Duration::minutes(minutes);

        Ok(BlockDefinition {
            id: row.try_get("Id")?,
            updated_ticks,
            name: row.try_get("Name")?,
            duration,
            stop_scheduling: decode_enum_column("block stop_scheduling", &row, "StopScheduling")?,
            items: self.effective_block_items(block_id, media_by_key).await?,
        })
    }

    async fn effective_block_items(
        &self,
        block_id: EntityId,
        media_by_key: &mut BTreeMap<String, Vec<SchedulingMediaItem>>,
    ) -> Result<Vec<EffectiveBlockItem>> {
        let rows = sqlx::query(
            r#"SELECT *
             FROM "BlockItem"
             WHERE "BlockId" = ?
             ORDER BY "Index", "Id""#,
        )
        .bind(block_id)
        .fetch_all(&self.0)
        .await?;

        let mut items = Vec::with_capacity(rows.len());
        for row in rows {
            let id = row.try_get::<EntityId, _>("Id")?;
            let content = block_item_content_key(id);
            media_by_key.insert(
                content.clone(),
                Box::pin(self.items_for_collection_key_row(&row, 0)).await?,
            );
            items.push(EffectiveBlockItem {
                id,
                block_id,
                content,
                collection_key: collection_key_from_row(&row)?,
                playback_order: decode_enum_column(
                    "block item playback_order",
                    &row,
                    "PlaybackOrder",
                )?,
                include_in_program_guide: row.try_get("IncludeInProgramGuide").unwrap_or(true),
                disable_watermarks: row.try_get("DisableWatermarks").unwrap_or(false),
                watermark_ids: load_ids(
                    &self.0,
                    r#"SELECT "WatermarkId"
                     FROM "BlockItemWatermark"
                     WHERE "BlockItemId" = ?
                     ORDER BY "WatermarkId""#,
                    id,
                )
                .await?,
                graphics_element_ids: load_ids(
                    &self.0,
                    r#"SELECT "GraphicsElementId"
                     FROM "BlockItemGraphicsElement"
                     WHERE "BlockItemId" = ?
                     ORDER BY "GraphicsElementId""#,
                    id,
                )
                .await?,
            });
        }

        Ok(items)
    }

    async fn items_for_collection_key(
        &self,
        collection_key: &SchedulingCollectionKey,
        depth: usize,
    ) -> Result<Vec<SchedulingMediaItem>> {
        match collection_key.collection_type {
            CollectionType::Collection => match collection_key.collection_id {
                Some(id) => self.items_for_collection_id(id).await,
                None => Ok(Vec::new()),
            },
            CollectionType::TelevisionShow => match collection_key.media_item_id {
                Some(id) => self.items_for_show_id(id).await,
                None => Ok(Vec::new()),
            },
            CollectionType::TelevisionSeason => match collection_key.media_item_id {
                Some(id) => self.items_for_season_id(id).await,
                None => Ok(Vec::new()),
            },
            CollectionType::Artist => match collection_key.media_item_id {
                Some(id) => self.items_for_artist_id(id).await,
                None => Ok(Vec::new()),
            },
            CollectionType::MultiCollection => match collection_key.multi_collection_id {
                Some(id) => Box::pin(self.items_for_multi_collection_id(id, depth)).await,
                None => Ok(Vec::new()),
            },
            CollectionType::SmartCollection => match collection_key.smart_collection_id {
                Some(id) => self.items_for_smart_collection_id(id).await,
                None => Ok(Vec::new()),
            },
            CollectionType::Playlist => match collection_key.playlist_id {
                Some(id) => Box::pin(self.playlist_media_items_by_id(id)).await,
                None => Ok(Vec::new()),
            },
            CollectionType::SearchQuery => match collection_key.search_query.as_deref() {
                Some(query) => self.search_items(query).await,
                None => Ok(Vec::new()),
            },
            CollectionType::Movie
            | CollectionType::Episode
            | CollectionType::MusicVideo
            | CollectionType::OtherVideo
            | CollectionType::Song
            | CollectionType::Image
            | CollectionType::RemoteStream => match collection_key.media_item_id {
                Some(id) => self.media_item_by_id(id).await,
                None => Ok(Vec::new()),
            },
            CollectionType::RerunFirstRun
            | CollectionType::RerunRerun
            | CollectionType::FakeCollection
            | CollectionType::FakePlaylistItem => Ok(Vec::new()),
        }
    }

    async fn effective_deco_for_playout(
        &self,
        playout_id: EntityId,
        media_by_key: &mut BTreeMap<String, Vec<SchedulingMediaItem>>,
    ) -> Result<Option<EffectiveDeco>> {
        let deco_id = sqlx::query_scalar::<_, EntityId>(
            r#"SELECT "DecoId" FROM "Playout" WHERE "Id" = ? AND "DecoId" IS NOT NULL"#,
        )
        .bind(playout_id)
        .fetch_optional(&self.0)
        .await?;

        let Some(deco_id) = deco_id else {
            return Ok(None);
        };

        self.effective_deco(deco_id, media_by_key).await
    }

    async fn effective_deco(
        &self,
        deco_id: EntityId,
        media_by_key: &mut BTreeMap<String, Vec<SchedulingMediaItem>>,
    ) -> Result<Option<EffectiveDeco>> {
        let row = sqlx::query(
            r#"SELECT "BreakContentMode", "DefaultFillerCollectionType", "DefaultFillerCollectionId",
                    "DefaultFillerMediaItemId", "DefaultFillerMultiCollectionId",
                    "DefaultFillerSmartCollectionId", "DefaultFillerTrimToFit"
             FROM "Deco"
             WHERE "Id" = ?"#,
        )
        .bind(deco_id)
        .fetch_one(&self.0)
        .await?;
        let mode: DecoMode = decode_enum_column("deco mode", &row, "BreakContentMode")?;
        if mode == DecoMode::Disable {
            return Ok(None);
        }

        let rows = sqlx::query(
            r#"SELECT *
             FROM "DecoBreakContent"
             WHERE "DecoId" = ?
             ORDER BY "Id""#,
        )
        .bind(deco_id)
        .fetch_all(&self.0)
        .await?;

        let mut break_content = Vec::with_capacity(rows.len());
        for row in rows {
            let id = row.try_get::<EntityId, _>("Id")?;
            let content = deco_break_content_key(id);
            media_by_key.insert(
                content.clone(),
                self.items_for_deco_break_content_row(&row).await?,
            );
            break_content.push(EffectiveBreakContent {
                id,
                placement: decode_enum_column("deco break placement", &row, "Placement")?,
                content,
                collection_key: collection_key_from_deco_break_content_row(&row)?,
            });
        }

        let (default_filler_content, default_filler_collection_key) = self
            .effective_default_filler(deco_id, &row, media_by_key)
            .await?;

        Ok(Some(EffectiveDeco {
            id: deco_id,
            default_filler_content,
            default_filler_collection_key,
            default_filler_trim_to_fit: row.try_get("DefaultFillerTrimToFit")?,
            break_content,
        }))
    }

    async fn effective_deco_template_items(
        &self,
        deco_template_id: EntityId,
        media_by_key: &mut BTreeMap<String, Vec<SchedulingMediaItem>>,
    ) -> Result<Vec<EffectiveDecoTemplateItem>> {
        let rows = sqlx::query(
            r#"SELECT "dti"."StartTime", "dti"."EndTime", "dti"."DecoId", "d"."BreakContentMode"
             FROM "DecoTemplateItem" AS "dti"
             JOIN "Deco" AS "d" ON "d"."Id" = "dti"."DecoId"
             WHERE "dti"."DecoTemplateId" = ?
             ORDER BY "dti"."StartTime", "dti"."Id""#,
        )
        .bind(deco_template_id)
        .fetch_all(&self.0)
        .await?;

        let mut result = Vec::with_capacity(rows.len());
        for row in rows {
            let deco_id = row.try_get::<EntityId, _>("DecoId")?;
            let mode: DecoMode = decode_enum_column(
                "deco template item BreakContentMode",
                &row,
                "BreakContentMode",
            )?;
            let selection = match mode {
                DecoMode::Inherit => EffectiveDecoSelection::Inherit,
                DecoMode::Override => EffectiveDecoSelection::Override(
                    self.effective_deco(deco_id, media_by_key).await?,
                ),
                DecoMode::Disable | DecoMode::Merge => EffectiveDecoSelection::Disable,
            };
            result.push(EffectiveDecoTemplateItem {
                start_time: decode_time_of_day_text(
                    "deco template item StartTime",
                    &row.try_get::<String, _>("StartTime")?,
                )?,
                end_time: decode_time_of_day_text(
                    "deco template item EndTime",
                    &row.try_get::<String, _>("EndTime")?,
                )?,
                selection,
            });
        }

        Ok(result)
    }

    async fn effective_default_filler(
        &self,
        deco_id: EntityId,
        row: &SqliteRow,
        media_by_key: &mut BTreeMap<String, Vec<SchedulingMediaItem>>,
    ) -> Result<(Option<String>, Option<SchedulingCollectionKey>)> {
        let collection_type = decode_optional_enum_column(
            "deco default_filler_collection_type",
            row,
            "DefaultFillerCollectionType",
        )?;
        let Some(collection_type) = collection_type else {
            return Ok((None, None));
        };

        let collection_key = SchedulingCollectionKey {
            collection_type,
            collection_id: row.try_get("DefaultFillerCollectionId")?,
            media_item_id: row.try_get("DefaultFillerMediaItemId")?,
            multi_collection_id: row.try_get("DefaultFillerMultiCollectionId")?,
            smart_collection_id: row.try_get("DefaultFillerSmartCollectionId")?,
            ..SchedulingCollectionKey::default()
        };
        let items = self.items_for_collection_key(&collection_key, 0).await?;
        if items.is_empty() {
            return Ok((None, None));
        }

        let content = deco_default_filler_key(deco_id);
        media_by_key.insert(content.clone(), items);
        Ok((Some(content), Some(collection_key)))
    }

    async fn items_for_deco_break_content_row(
        &self,
        row: &SqliteRow,
    ) -> Result<Vec<SchedulingMediaItem>> {
        let collection_key = collection_key_from_deco_break_content_row(row)?;
        self.items_for_collection_key(&collection_key, 0).await
    }

    async fn playout_items(&self, playout_id: EntityId) -> Result<Vec<PlayoutItem>> {
        let rows = sqlx::query(
            r#"SELECT *
             FROM "PlayoutItem"
             WHERE "PlayoutId" = ?
             ORDER BY "Start", "Id""#,
        )
        .bind(playout_id)
        .fetch_all(&self.0)
        .await?;

        let mut items = Vec::with_capacity(rows.len());
        for row in rows {
            let id = row.try_get::<EntityId, _>("Id")?;
            let mut item = map_playout_item(&row)?;
            item.watermark_ids = load_ids(
                &self.0,
                r#"SELECT "WatermarkId"
                 FROM "PlayoutItemWatermark"
                 WHERE "PlayoutItemId" = ?
                 ORDER BY "WatermarkId""#,
                id,
            )
            .await?;
            item.graphics_element_ids = load_ids(
                &self.0,
                r#"SELECT "GraphicsElementId"
                 FROM "PlayoutItemGraphicsElement"
                 WHERE "PlayoutItemId" = ?
                 ORDER BY "GraphicsElementId""#,
                id,
            )
            .await?;
            items.push(item);
        }

        Ok(items)
    }

    async fn playout_history(&self, playout_id: EntityId) -> Result<Vec<PlayoutHistory>> {
        let rows = sqlx::query(
            r#"SELECT *
             FROM "PlayoutHistory"
             WHERE "PlayoutId" = ?
             ORDER BY "When", "Id""#,
        )
        .bind(playout_id)
        .fetch_all(&self.0)
        .await?;

        rows.iter().map(map_playout_history).collect()
    }
}

const SEARCH_DOCUMENT_SQL: &str = r#"
SELECT *
FROM (
    SELECT
        "mm"."MovieId" AS "item_id",
        'movie' AS "media_kind",
        'Movie' AS "metadata_kind",
        "mm"."Id" AS "metadata_id",
        "mm"."Title" AS "title",
        "mm"."OriginalTitle" AS "original_title",
        "mm"."SortTitle" AS "sort_title",
        "mm"."Year" AS "year",
        "mm"."ReleaseDate" AS "release_date",
        NULL AS "show_id",
        NULL AS "show_title",
        NULL AS "season_number",
        NULL AS "episode_number"
    FROM "MovieMetadata" AS "mm"
    UNION ALL
    SELECT
        "sm"."ShowId" AS "item_id",
        'show' AS "media_kind",
        'Show' AS "metadata_kind",
        "sm"."Id" AS "metadata_id",
        "sm"."Title" AS "title",
        "sm"."OriginalTitle" AS "original_title",
        "sm"."SortTitle" AS "sort_title",
        "sm"."Year" AS "year",
        "sm"."ReleaseDate" AS "release_date",
        "sm"."ShowId" AS "show_id",
        "sm"."Title" AS "show_title",
        NULL AS "season_number",
        NULL AS "episode_number"
    FROM "ShowMetadata" AS "sm"
    UNION ALL
    SELECT
        "sem"."SeasonId" AS "item_id",
        'season' AS "media_kind",
        'Season' AS "metadata_kind",
        "sem"."Id" AS "metadata_id",
        "sem"."Title" AS "title",
        "sem"."OriginalTitle" AS "original_title",
        "sem"."SortTitle" AS "sort_title",
        "sem"."Year" AS "year",
        "sem"."ReleaseDate" AS "release_date",
        "se"."ShowId" AS "show_id",
        "shm"."Title" AS "show_title",
        "se"."SeasonNumber" AS "season_number",
        NULL AS "episode_number"
    FROM "SeasonMetadata" AS "sem"
    JOIN "Season" AS "se" ON "se"."Id" = "sem"."SeasonId"
    LEFT JOIN "ShowMetadata" AS "shm" ON "shm"."ShowId" = "se"."ShowId"
    UNION ALL
    SELECT
        "em"."EpisodeId" AS "item_id",
        'episode' AS "media_kind",
        'Episode' AS "metadata_kind",
        "em"."Id" AS "metadata_id",
        "em"."Title" AS "title",
        "em"."OriginalTitle" AS "original_title",
        "em"."SortTitle" AS "sort_title",
        "em"."Year" AS "year",
        "em"."ReleaseDate" AS "release_date",
        "s"."ShowId" AS "show_id",
        "shm"."Title" AS "show_title",
        "s"."SeasonNumber" AS "season_number",
        "em"."EpisodeNumber" AS "episode_number"
    FROM "EpisodeMetadata" AS "em"
    JOIN "Episode" AS "e" ON "e"."Id" = "em"."EpisodeId"
    JOIN "Season" AS "s" ON "s"."Id" = "e"."SeasonId"
    LEFT JOIN "ShowMetadata" AS "shm" ON "shm"."ShowId" = "s"."ShowId"
    UNION ALL
    SELECT
        "mvm"."MusicVideoId" AS "item_id",
        'music_video' AS "media_kind",
        'MusicVideo' AS "metadata_kind",
        "mvm"."Id" AS "metadata_id",
        "mvm"."Title" AS "title",
        "mvm"."OriginalTitle" AS "original_title",
        "mvm"."SortTitle" AS "sort_title",
        "mvm"."Year" AS "year",
        "mvm"."ReleaseDate" AS "release_date",
        NULL AS "show_id",
        NULL AS "show_title",
        NULL AS "season_number",
        NULL AS "episode_number"
    FROM "MusicVideoMetadata" AS "mvm"
    UNION ALL
    SELECT
        "am"."ArtistId" AS "item_id",
        'artist' AS "media_kind",
        'Artist' AS "metadata_kind",
        "am"."Id" AS "metadata_id",
        "am"."Title" AS "title",
        "am"."OriginalTitle" AS "original_title",
        "am"."SortTitle" AS "sort_title",
        "am"."Year" AS "year",
        "am"."ReleaseDate" AS "release_date",
        NULL AS "show_id",
        NULL AS "show_title",
        NULL AS "season_number",
        NULL AS "episode_number"
    FROM "ArtistMetadata" AS "am"
    UNION ALL
    SELECT
        "ovm"."OtherVideoId" AS "item_id",
        'other_video' AS "media_kind",
        'OtherVideo' AS "metadata_kind",
        "ovm"."Id" AS "metadata_id",
        "ovm"."Title" AS "title",
        "ovm"."OriginalTitle" AS "original_title",
        "ovm"."SortTitle" AS "sort_title",
        "ovm"."Year" AS "year",
        "ovm"."ReleaseDate" AS "release_date",
        NULL AS "show_id",
        NULL AS "show_title",
        NULL AS "season_number",
        NULL AS "episode_number"
    FROM "OtherVideoMetadata" AS "ovm"
    UNION ALL
    SELECT
        "som"."SongId" AS "item_id",
        'song' AS "media_kind",
        'Song' AS "metadata_kind",
        "som"."Id" AS "metadata_id",
        "som"."Title" AS "title",
        "som"."OriginalTitle" AS "original_title",
        "som"."SortTitle" AS "sort_title",
        "som"."Year" AS "year",
        "som"."ReleaseDate" AS "release_date",
        NULL AS "show_id",
        NULL AS "show_title",
        NULL AS "season_number",
        NULL AS "episode_number"
    FROM "SongMetadata" AS "som"
)
ORDER BY COALESCE("sort_title", "title"), "item_id""#;

fn collection_key_from_row(row: &SqliteRow) -> Result<SchedulingCollectionKey> {
    Ok(SchedulingCollectionKey {
        collection_type: decode_enum_column("collection type", row, "CollectionType")?,
        collection_id: optional_id(row, "CollectionId")?,
        multi_collection_id: optional_id(row, "MultiCollectionId")?,
        smart_collection_id: optional_id(row, "SmartCollectionId")?,
        media_item_id: optional_id(row, "MediaItemId")?,
        playlist_id: optional_id(row, "PlaylistId")?,
        search_query: row
            .try_get::<Option<String>, _>("SearchQuery")
            .ok()
            .flatten(),
        fake_collection_key: None,
    })
}

fn collection_key_from_deco_break_content_row(row: &SqliteRow) -> Result<SchedulingCollectionKey> {
    Ok(SchedulingCollectionKey {
        collection_type: decode_enum_column(
            "deco break content collection type",
            row,
            "CollectionType",
        )?,
        collection_id: optional_id(row, "CollectionId")?,
        media_item_id: optional_id(row, "MediaItemId")?,
        playlist_id: optional_id(row, "PlaylistId")?,
        ..SchedulingCollectionKey::default()
    })
}

fn optional_id(row: &SqliteRow, column: &str) -> Result<Option<EntityId>> {
    match row.try_get::<Option<EntityId>, _>(column) {
        Ok(value) => Ok(value),
        Err(sqlx::Error::ColumnNotFound(_)) => Ok(None),
        Err(error) => Err(error.into()),
    }
}

fn map_scheduling_media_item(row: &SqliteRow) -> Result<SchedulingMediaItem> {
    let duration_text: String = row.try_get("duration_text").unwrap_or_default();
    let duration = decode_duration_text(&duration_text);
    Ok(SchedulingMediaItem {
        id: row.try_get("Id")?,
        duration,
        title: row.try_get("title")?,
        show_id: row.try_get("show_id")?,
        season_id: row.try_get("season_id")?,
        season: row.try_get("season_number")?,
        episode: row.try_get("episode_number")?,
        release_date: row
            .try_get::<Option<String>, _>("release_date")?
            .filter(|value| !value.trim().is_empty())
            .map(|value| parse_datetime("media item release_date", &value))
            .transpose()?,
    })
}

fn map_playout_item(row: &SqliteRow) -> Result<PlayoutItem> {
    Ok(PlayoutItem {
        id: row.try_get("Id")?,
        media_item_id: row.try_get("MediaItemId")?,
        start: decode_datetime("playout item start", &row.try_get::<String, _>("Start")?)?,
        finish: decode_datetime("playout item finish", &row.try_get::<String, _>("Finish")?)?,
        guide_start: decode_optional_datetime(
            "playout item guide_start",
            row.try_get("GuideStart")?,
        )?,
        guide_finish: decode_optional_datetime(
            "playout item guide_finish",
            row.try_get("GuideFinish")?,
        )?,
        custom_title: row.try_get("CustomTitle")?,
        guide_group: row.try_get("GuideGroup")?,
        filler_kind: decode_enum_column("playout item filler_kind", row, "FillerKind")?,
        playout_id: row.try_get("PlayoutId")?,
        in_point: decode_duration_text(&row.try_get::<String, _>("InPoint")?),
        out_point: decode_duration_text(&row.try_get::<String, _>("OutPoint")?),
        chapter_title: row.try_get("ChapterTitle")?,
        watermark_ids: Vec::new(),
        disable_watermarks: row.try_get("DisableWatermarks")?,
        preferred_audio_language_code: row.try_get("PreferredAudioLanguageCode")?,
        preferred_audio_title: row.try_get("PreferredAudioTitle")?,
        preferred_subtitle_language_code: row.try_get("PreferredSubtitleLanguageCode")?,
        subtitle_mode: decode_optional_enum_column(
            "playout item subtitle_mode",
            row,
            "SubtitleMode",
        )?,
        block_key: row.try_get("BlockKey")?,
        collection_key: row.try_get("CollectionKey")?,
        collection_etag: row.try_get("CollectionEtag")?,
        graphics_element_ids: Vec::new(),
        scheduling_context: row.try_get("SchedulingContext")?,
    })
}

fn map_playout_history(row: &SqliteRow) -> Result<PlayoutHistory> {
    Ok(PlayoutHistory {
        id: row.try_get("Id")?,
        playout_id: row.try_get("PlayoutId")?,
        block_id: row.try_get("BlockId")?,
        playback_order: decode_enum_column("playout history playback_order", row, "PlaybackOrder")?,
        key: row.try_get("Key")?,
        child_key: row.try_get("ChildKey")?,
        when: decode_datetime("playout history When", &row.try_get::<String, _>("When")?)?,
        finish: decode_optional_datetime("playout history Finish", row.try_get("Finish")?)?,
        index: row.try_get("Index")?,
        details: row.try_get("Details")?,
        is_current_child: row.try_get("IsCurrentChild")?,
    })
}

fn block_item_content_key(id: EntityId) -> String {
    format!("block-item:{id}")
}

fn deco_break_content_key(id: EntityId) -> String {
    format!("deco-break-content:{id}")
}

fn deco_default_filler_key(id: EntityId) -> String {
    format!("deco-default-filler:{id}")
}

fn decode_integer_list(kind: &'static str, value: &str) -> Result<Vec<i32>> {
    if value.is_empty() {
        return Ok(Vec::new());
    }
    serde_json::from_str(value).map_err(|source| decode_message(kind, &source.to_string()))
}

fn decode_optional_time_of_day_text(
    kind: &'static str,
    value: Option<String>,
) -> Result<Option<NaiveTime>> {
    value
        .map(|inner| decode_time_of_day_text(kind, &inner))
        .transpose()
}

fn decode_time_of_day_text(kind: &'static str, value: &str) -> Result<NaiveTime> {
    NaiveTime::parse_from_str(value, "%H:%M:%S")
        .or_else(|_| NaiveTime::parse_from_str(value, "%H:%M:%S%.f"))
        .map_err(|source| decode_message(kind, &source.to_string()))
}

fn parse_datetime(kind: &'static str, value: &str) -> Result<DateTime<Utc>> {
    DateTime::parse_from_rfc3339(value)
        .map(|parsed| parsed.with_timezone(&Utc))
        .or_else(|_| {
            chrono::NaiveDateTime::parse_from_str(value, "%Y-%m-%d %H:%M:%S%.f")
                .map(|parsed| parsed.and_utc())
        })
        .or_else(|_| {
            chrono::NaiveDate::parse_from_str(value, "%Y-%m-%d")
                .map(|parsed| parsed.and_hms_opt(0, 0, 0).unwrap().and_utc())
        })
        .map_err(|source| decode_message(kind, &source.to_string()))
}

async fn exists_by_id(pool: &Pool<Sqlite>, table: &str, id: EntityId) -> Result<bool> {
    let sql = format!(r#"SELECT EXISTS(SELECT 1 FROM "{table}" WHERE "Id" = ?)"#);
    sqlx::query_scalar::<_, bool>(&sql)
        .bind(id)
        .fetch_one(pool)
        .await
        .map_err(Into::into)
}

fn append_unique(
    target: &mut Vec<SchedulingMediaItem>,
    seen: &mut BTreeSet<EntityId>,
    items: Vec<SchedulingMediaItem>,
) {
    for item in items {
        if seen.insert(item.id) {
            target.push(item);
        }
    }
}

#[derive(Debug, Clone, Default)]
struct StreamSearchFields {
    audio_languages: Vec<String>,
    subtitle_languages: Vec<String>,
    video_codecs: Vec<String>,
    audio_codecs: Vec<String>,
    subtitle_codecs: Vec<String>,
    width: Option<i32>,
    height: Option<i32>,
}

fn metadata_fk(metadata_kind: &str) -> Option<&'static str> {
    match metadata_kind {
        "Movie" => Some("MovieMetadataId"),
        "Episode" => Some("EpisodeMetadataId"),
        "Show" => Some("ShowMetadataId"),
        "Season" => Some("SeasonMetadataId"),
        "MusicVideo" => Some("MusicVideoMetadataId"),
        "Artist" => Some("ArtistMetadataId"),
        "OtherVideo" => Some("OtherVideoMetadataId"),
        "Song" => Some("SongMetadataId"),
        "Image" => Some("ImageMetadataId"),
        "RemoteStream" => Some("RemoteStreamMetadataId"),
        _ => None,
    }
}

fn media_version_fk(metadata_kind: &str) -> Option<&'static str> {
    match metadata_kind {
        "Movie" => Some("MovieId"),
        "Episode" => Some("EpisodeId"),
        "MusicVideo" => Some("MusicVideoId"),
        "OtherVideo" => Some("OtherVideoId"),
        "Song" => Some("SongId"),
        "Image" => Some("ImageId"),
        "RemoteStream" => Some("RemoteStreamId"),
        _ => None,
    }
}

fn media_stream_kind_from_row(row: &SqliteRow) -> Option<String> {
    if let Ok(value) = row.try_get::<i64, _>("MediaStreamKind") {
        return match value {
            1 => Some("video".to_owned()),
            2 => Some("audio".to_owned()),
            3 => Some("subtitle".to_owned()),
            5 => Some("external_subtitle".to_owned()),
            _ => None,
        };
    }
    row.try_get::<String, _>("MediaStreamKind")
        .ok()
        .map(|value| value.to_ascii_lowercase())
}

fn push_unique_optional(target: &mut Vec<String>, value: Option<String>) {
    if let Some(value) = value.filter(|value| !value.trim().is_empty()) {
        push_unique(target, value);
    }
}

fn push_unique(target: &mut Vec<String>, value: String) {
    if !target
        .iter()
        .any(|existing| existing.eq_ignore_ascii_case(&value))
    {
        target.push(value);
    }
}

fn search_sort_title(document: &SearchDocument) -> String {
    document
        .sort_title
        .as_deref()
        .or(document.title.as_deref())
        .unwrap_or_default()
        .to_ascii_lowercase()
}

fn normalize_graphics_path(path: &str) -> String {
    let normalized = path.replace('\\', "/");
    if normalized.contains("templates/graphics-elements/") {
        normalized
    } else {
        format!("templates/graphics-elements/{normalized}")
    }
}
