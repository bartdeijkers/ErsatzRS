use std::collections::BTreeSet;

use async_trait::async_trait;
use chrono::{DateTime, Duration, Utc};
use nastv_core::repositories::Result;
use nastv_core::scheduling::{
    SchedulingCollectionKey, SchedulingContentRepository, SchedulingContentSource,
    SchedulingMediaItem, SchedulingPlaylistItem,
};
use nastv_domain::{CollectionType, EntityId};
use sqlx::sqlite::SqliteRow;
use sqlx::{Pool, Row, Sqlite};

use super::common::{decode_enum, decode_message};

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
            "SELECT id FROM channel_watermarks WHERE name LIKE ? ORDER BY id LIMIT 1",
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
            "SELECT id
             FROM graphics_elements
             WHERE path = ? OR path = ? OR path LIKE ?
             ORDER BY id
             LIMIT 1",
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
            "SELECT id FROM collections WHERE name = ? ORDER BY id LIMIT 1",
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
            "SELECT media_item_id
             FROM collection_items
             WHERE collection_id = ?
             ORDER BY sort_order, id",
        )
        .bind(collection_id)
        .fetch_all(&self.0)
        .await?;

        self.expand_media_ids(ids, 0).await
    }

    async fn multi_collection_items_by_name(&self, name: &str) -> Result<Vec<SchedulingMediaItem>> {
        let multi_collection_id = sqlx::query_scalar::<_, EntityId>(
            "SELECT id FROM multi_collections WHERE name = ? ORDER BY id LIMIT 1",
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

        let rows = sqlx::query(
            "SELECT collection_type, collection_id, media_item_id, playlist_id
             FROM multi_collection_items
             WHERE multi_collection_id = ?
             ORDER BY sort_order, id",
        )
        .bind(multi_collection_id)
        .fetch_all(&self.0)
        .await?;

        let mut items = Vec::new();
        let mut seen = BTreeSet::new();
        for row in rows {
            let child_items = Box::pin(self.items_for_collection_key_row(&row, depth + 1)).await?;
            append_unique(&mut items, &mut seen, child_items);
        }

        let smart_ids = sqlx::query_scalar::<_, EntityId>(
            "SELECT smart_collection_id
             FROM multi_collection_smart_items
             WHERE multi_collection_id = ?
             ORDER BY sort_order, id",
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
            "SELECT id, query FROM smart_collections WHERE name = ? ORDER BY id LIMIT 1",
        )
        .bind(name)
        .fetch_optional(&self.0)
        .await?;

        match row {
            Some(row) => {
                let query = row.try_get::<String, _>("query")?;
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
            "SELECT query FROM smart_collections WHERE id = ? ORDER BY id LIMIT 1",
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
        let playlist_id = sqlx::query_scalar::<_, EntityId>(
            "SELECT id FROM playlists WHERE name = ? ORDER BY id LIMIT 1",
        )
        .bind(name)
        .fetch_optional(&self.0)
        .await?;

        let Some(playlist_id) = playlist_id else {
            return Ok(Vec::new());
        };

        let rows = if let Some(group) = group {
            sqlx::query(
                "SELECT pi.*
                 FROM playlist_items pi
                 JOIN playlist_groups pg ON pg.id = pi.playlist_group_id
                 WHERE pi.playlist_id = ? AND pg.name = ?
                 ORDER BY pi.sort_order, pi.id",
            )
            .bind(playlist_id)
            .bind(group)
            .fetch_all(&self.0)
            .await?
        } else {
            sqlx::query(
                "SELECT *
                 FROM playlist_items
                 WHERE playlist_id = ?
                 ORDER BY sort_order, id",
            )
            .bind(playlist_id)
            .fetch_all(&self.0)
            .await?
        };

        let mut result = Vec::with_capacity(rows.len());
        for row in rows {
            let media_items = Box::pin(self.items_for_collection_key_row(&row, 0)).await?;
            result.push(SchedulingPlaylistItem {
                collection_key: collection_key_from_row(&row)?,
                playback_order: decode_enum(
                    "playlist item playback_order",
                    &row.try_get::<String, _>("playback_order")?,
                )?,
                play_all: row.try_get::<bool, _>("play_all")?,
                count: row
                    .try_get::<Option<i64>, _>("count")?
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
        let collection_type: CollectionType = decode_enum(
            "collection type",
            &row.try_get::<String, _>("collection_type")?,
        )?;

        match collection_type {
            CollectionType::Collection => {
                if let Some(id) = optional_id(row, "collection_id")? {
                    self.items_for_collection_id(id).await
                } else {
                    Ok(Vec::new())
                }
            }
            CollectionType::TelevisionShow => {
                if let Some(id) = optional_id(row, "media_item_id")? {
                    self.items_for_show_id(id).await
                } else {
                    Ok(Vec::new())
                }
            }
            CollectionType::TelevisionSeason => {
                if let Some(id) = optional_id(row, "media_item_id")? {
                    self.items_for_season_id(id).await
                } else {
                    Ok(Vec::new())
                }
            }
            CollectionType::Artist => {
                if let Some(id) = optional_id(row, "media_item_id")? {
                    self.items_for_artist_id(id).await
                } else {
                    Ok(Vec::new())
                }
            }
            CollectionType::MultiCollection => {
                if let Some(id) = optional_id(row, "multi_collection_id")? {
                    Box::pin(self.items_for_multi_collection_id(id, depth)).await
                } else {
                    Ok(Vec::new())
                }
            }
            CollectionType::SmartCollection => {
                if let Some(id) = optional_id(row, "smart_collection_id")? {
                    self.items_for_smart_collection_id(id).await
                } else {
                    Ok(Vec::new())
                }
            }
            CollectionType::Playlist => {
                if let Some(id) = optional_id(row, "playlist_id")? {
                    Box::pin(self.playlist_media_items_by_id(id)).await
                } else {
                    Ok(Vec::new())
                }
            }
            CollectionType::SearchQuery => {
                if let Some(query) = row
                    .try_get::<Option<String>, _>("search_query")
                    .ok()
                    .flatten()
                {
                    self.search_items(&query).await
                } else {
                    Ok(Vec::new())
                }
            }
            CollectionType::Movie
            | CollectionType::Episode
            | CollectionType::MusicVideo
            | CollectionType::OtherVideo
            | CollectionType::Song
            | CollectionType::Image
            | CollectionType::RemoteStream => {
                if let Some(id) = optional_id(row, "media_item_id")? {
                    self.media_item_by_id(id).await
                } else {
                    Ok(Vec::new())
                }
            }
            CollectionType::RerunFirstRun
            | CollectionType::RerunRerun
            | CollectionType::FakeCollection
            | CollectionType::FakePlaylistItem => Ok(Vec::new()),
        }
    }

    async fn playlist_media_items_by_id(
        &self,
        playlist_id: EntityId,
    ) -> Result<Vec<SchedulingMediaItem>> {
        let name = sqlx::query_scalar::<_, String>("SELECT name FROM playlists WHERE id = ?")
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
        exists_by_id(&self.0, "shows", id).await
    }

    async fn is_season(&self, id: EntityId) -> Result<bool> {
        exists_by_id(&self.0, "seasons", id).await
    }

    async fn is_artist(&self, id: EntityId) -> Result<bool> {
        exists_by_id(&self.0, "artists", id).await
    }

    async fn items_for_show_id(&self, show_id: EntityId) -> Result<Vec<SchedulingMediaItem>> {
        let ids = sqlx::query_scalar::<_, EntityId>(
            "SELECT id
             FROM episodes
             WHERE show_id = ?
             ORDER BY season_number, episode_number, id",
        )
        .bind(show_id)
        .fetch_all(&self.0)
        .await?;
        self.load_media_items(ids).await
    }

    async fn items_for_season_id(&self, season_id: EntityId) -> Result<Vec<SchedulingMediaItem>> {
        let ids = sqlx::query_scalar::<_, EntityId>(
            "SELECT id
             FROM episodes
             WHERE season_id = ?
             ORDER BY season_number, episode_number, id",
        )
        .bind(season_id)
        .fetch_all(&self.0)
        .await?;
        self.load_media_items(ids).await
    }

    async fn items_for_artist_id(&self, artist_id: EntityId) -> Result<Vec<SchedulingMediaItem>> {
        let ids = sqlx::query_scalar::<_, EntityId>(
            "SELECT id FROM music_videos WHERE artist_id = ? ORDER BY id",
        )
        .bind(artist_id)
        .fetch_all(&self.0)
        .await?;
        self.load_media_items(ids).await
    }

    async fn show_ids_by_name(&self, name: &str) -> Result<Vec<EntityId>> {
        sqlx::query_scalar::<_, EntityId>(
            "SELECT s.id
             FROM shows s
             JOIN show_metadata sm ON sm.item_id = s.id
             WHERE sm.title = ?
             ORDER BY s.id",
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
        let show_id = sqlx::query_scalar::<_, EntityId>(
            "SELECT sm.item_id
             FROM metadata_guids mg
             JOIN show_metadata sm ON sm.id = mg.metadata_id
             WHERE mg.metadata_kind = 'Show' AND mg.source = ? AND mg.guid = ?
             ORDER BY sm.item_id
             LIMIT 1",
        )
        .bind(source)
        .bind(value)
        .fetch_optional(&self.0)
        .await?;

        match show_id {
            Some(id) => self.items_for_show_id(id).await,
            None => Ok(Vec::new()),
        }
    }

    async fn search_items(&self, query: &str) -> Result<Vec<SchedulingMediaItem>> {
        let (operator, term) = parse_title_query(query);
        let pattern = match operator {
            SearchOperator::Exact => term.clone(),
            SearchOperator::Contains => format!("%{term}%"),
        };
        let comparator = match operator {
            SearchOperator::Exact => "=",
            SearchOperator::Contains => "LIKE",
        };
        let sql = format!(
            "SELECT item_id FROM (
                SELECT item_id, title FROM movie_metadata
                UNION ALL SELECT item_id, title FROM episode_metadata
                UNION ALL SELECT item_id, title FROM music_video_metadata
                UNION ALL SELECT item_id, title FROM other_video_metadata
                UNION ALL SELECT item_id, title FROM song_metadata
                UNION ALL SELECT item_id, title FROM image_metadata
                UNION ALL SELECT item_id, title FROM remote_stream_metadata
             )
             WHERE title {comparator} ?
             ORDER BY item_id"
        );

        let ids = sqlx::query_scalar::<_, EntityId>(&sql)
            .bind(pattern)
            .fetch_all(&self.0)
            .await?;
        self.expand_media_ids(ids, 0).await
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
            "SELECT
                mi.id,
                COALESCE(MAX(mv.duration_ms), 0) AS duration_ms,
                COALESCE(
                    em.title, mm.title, mvm.title, ovm.title, som.title, im.title, rsm.title,
                    shm.title, sem.title, am.title
                ) AS title,
                e.show_id,
                e.season_id,
                e.season_number,
                e.episode_number,
                COALESCE(
                    em.release_date, mm.release_date, mvm.release_date, ovm.release_date,
                    som.release_date, im.release_date, rsm.release_date, shm.release_date,
                    sem.release_date, am.release_date
                ) AS release_date
             FROM media_items mi
             LEFT JOIN media_versions mv ON mv.media_item_id = mi.id
             LEFT JOIN movies m ON m.id = mi.id
             LEFT JOIN movie_metadata mm ON mm.item_id = m.id
             LEFT JOIN episodes e ON e.id = mi.id
             LEFT JOIN episode_metadata em ON em.item_id = e.id
             LEFT JOIN music_videos mvitem ON mvitem.id = mi.id
             LEFT JOIN music_video_metadata mvm ON mvm.item_id = mvitem.id
             LEFT JOIN other_videos ov ON ov.id = mi.id
             LEFT JOIN other_video_metadata ovm ON ovm.item_id = ov.id
             LEFT JOIN songs so ON so.id = mi.id
             LEFT JOIN song_metadata som ON som.item_id = so.id
             LEFT JOIN images image ON image.id = mi.id
             LEFT JOIN image_metadata im ON im.item_id = image.id
             LEFT JOIN remote_streams rs ON rs.id = mi.id
             LEFT JOIN remote_stream_metadata rsm ON rsm.item_id = rs.id
             LEFT JOIN shows sh ON sh.id = mi.id
             LEFT JOIN show_metadata shm ON shm.item_id = sh.id
             LEFT JOIN seasons se ON se.id = mi.id
             LEFT JOIN season_metadata sem ON sem.item_id = se.id
             LEFT JOIN artists a ON a.id = mi.id
             LEFT JOIN artist_metadata am ON am.item_id = a.id
             WHERE mi.id = ?
             GROUP BY mi.id",
        )
        .bind(id)
        .fetch_optional(&self.0)
        .await?;

        row.map(|row| map_scheduling_media_item(&row)).transpose()
    }
}

fn collection_key_from_row(row: &SqliteRow) -> Result<SchedulingCollectionKey> {
    Ok(SchedulingCollectionKey {
        collection_type: decode_enum(
            "collection type",
            &row.try_get::<String, _>("collection_type")?,
        )?,
        collection_id: optional_id(row, "collection_id")?,
        multi_collection_id: optional_id(row, "multi_collection_id")?,
        smart_collection_id: optional_id(row, "smart_collection_id")?,
        media_item_id: optional_id(row, "media_item_id")?,
        playlist_id: optional_id(row, "playlist_id")?,
        search_query: row
            .try_get::<Option<String>, _>("search_query")
            .ok()
            .flatten(),
        fake_collection_key: None,
    })
}

fn optional_id(row: &SqliteRow, column: &str) -> Result<Option<EntityId>> {
    row.try_get::<Option<EntityId>, _>(column)
        .map_err(Into::into)
}

fn map_scheduling_media_item(row: &SqliteRow) -> Result<SchedulingMediaItem> {
    Ok(SchedulingMediaItem {
        id: row.try_get("id")?,
        duration: Duration::milliseconds(row.try_get::<i64, _>("duration_ms")?),
        title: row.try_get("title")?,
        show_id: row.try_get("show_id")?,
        season_id: row.try_get("season_id")?,
        season: row.try_get("season_number")?,
        episode: row.try_get("episode_number")?,
        release_date: row
            .try_get::<Option<String>, _>("release_date")?
            .map(|value| parse_datetime("media item release_date", &value))
            .transpose()?,
    })
}

fn parse_datetime(kind: &'static str, value: &str) -> Result<DateTime<Utc>> {
    DateTime::parse_from_rfc3339(value)
        .map(|parsed| parsed.with_timezone(&Utc))
        .map_err(|source| decode_message(kind, &source.to_string()))
}

async fn exists_by_id(pool: &Pool<Sqlite>, table: &str, id: EntityId) -> Result<bool> {
    let sql = format!("SELECT EXISTS(SELECT 1 FROM {table} WHERE id = ?)");
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

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum SearchOperator {
    Exact,
    Contains,
}

fn parse_title_query(query: &str) -> (SearchOperator, String) {
    let trimmed = query.trim();
    for marker in ["title = ", "title == ", "title:"] {
        if let Some(value) = trimmed.strip_prefix(marker) {
            return (SearchOperator::Exact, unquote(value.trim()));
        }
    }
    if let Some(value) = trimmed.strip_prefix("title contains ") {
        return (SearchOperator::Contains, unquote(value.trim()));
    }
    (SearchOperator::Contains, unquote(trimmed))
}

fn unquote(value: &str) -> String {
    value.trim_matches('"').trim_matches('\'').trim().to_owned()
}

fn normalize_graphics_path(path: &str) -> String {
    let normalized = path.replace('\\', "/");
    if normalized.contains("templates/graphics-elements/") {
        normalized
    } else {
        format!("templates/graphics-elements/{normalized}")
    }
}
