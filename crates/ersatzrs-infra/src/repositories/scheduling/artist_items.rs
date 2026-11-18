//! Batch artist expansion without changing search-hit or per-artist video order.
use super::{
    decode_message, BTreeMap, BTreeSet, EntityId, Result, SchedulingMediaItem, SearchDocument,
    SqliteSchedulingContentRepository,
};
use crate::repositories::SqlxResultExt as _;
use sqlx::Row;

impl SqliteSchedulingContentRepository {
    pub(super) async fn search_artist_items(
        &self,
        hits: &[SearchDocument],
    ) -> Result<(
        BTreeMap<EntityId, Vec<EntityId>>,
        BTreeMap<EntityId, SchedulingMediaItem>,
    )> {
        let artists = hits
            .iter()
            .filter(|hit| hit.media_kind == "artist")
            .map(|hit| hit.media_item_id)
            .collect::<BTreeSet<_>>();
        if artists.is_empty() {
            return Ok((BTreeMap::new(), BTreeMap::new()));
        }
        let ids = serde_json::to_string(&artists)
            .map_err(|error| decode_message("search artist identities", &error.to_string()))?;
        let rows = sqlx::query(
            r#"SELECT "Id", "ArtistId" FROM "MusicVideo"
            WHERE "ArtistId" IN (SELECT "value" FROM json_each(?)) ORDER BY "ArtistId", "Id""#,
        )
        .bind(ids)
        .fetch_all(&self.0)
        .await
        .repository()?;
        let mut membership: BTreeMap<EntityId, Vec<EntityId>> = BTreeMap::new();
        let mut video_ids = BTreeSet::new();
        for row in rows {
            let id = row.try_get::<EntityId, _>("Id").repository()?;
            membership
                .entry(row.try_get("ArtistId").repository()?)
                .or_default()
                .push(id);
            video_ids.insert(id);
        }
        video_ids.extend(
            hits.iter()
                .filter(|hit| hit.media_kind == "music_video")
                .map(|hit| hit.media_item_id),
        );
        let videos = self
            .load_media_items(video_ids.into_iter().collect())
            .await?
            .into_iter()
            .map(|item| (item.id, item))
            .collect();
        Ok((membership, videos))
    }
}
