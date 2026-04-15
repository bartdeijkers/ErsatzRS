use async_trait::async_trait;
use nastv_core::repositories::{MediaSourceRepository, Result};
use nastv_domain::{
    EmbyMediaSource, EntityId, JellyfinMediaSource, MediaSource, MediaSourceKind, PlexMediaSource,
};
use sqlx::{Pool, Sqlite};

use super::common::{fetch_data, from_json, to_json};

#[derive(Clone)]
pub struct SqliteMediaSourceRepository(pub Pool<Sqlite>);

#[async_trait]
impl MediaSourceRepository for SqliteMediaSourceRepository {
    async fn get_plex(&self, id: EntityId) -> Result<PlexMediaSource> {
        from_json(
            "plex media source",
            id,
            fetch_data(&self.0, "plex_media_sources", "plex media source", id).await?,
        )
    }

    async fn get_jellyfin(&self, id: EntityId) -> Result<JellyfinMediaSource> {
        from_json(
            "jellyfin media source",
            id,
            fetch_data(
                &self.0,
                "jellyfin_media_sources",
                "jellyfin media source",
                id,
            )
            .await?,
        )
    }

    async fn get_emby(&self, id: EntityId) -> Result<EmbyMediaSource> {
        from_json(
            "emby media source",
            id,
            fetch_data(&self.0, "emby_media_sources", "emby media source", id).await?,
        )
    }

    async fn list_all(&self) -> Result<Vec<MediaSource>> {
        let rows = sqlx::query_as::<_, (i64, String)>(
            "SELECT id, data FROM media_sources ORDER BY name, id",
        )
        .fetch_all(&self.0)
        .await?;
        rows.into_iter()
            .map(|(id, data)| from_json("media source", id, data))
            .collect()
    }

    async fn upsert_plex(&self, source: &PlexMediaSource) -> Result<()> {
        upsert_source(
            &self.0,
            &source.media_source,
            MediaSourceKind::Plex,
            "plex_media_sources",
            source,
        )
        .await
    }

    async fn upsert_jellyfin(&self, source: &JellyfinMediaSource) -> Result<()> {
        upsert_source(
            &self.0,
            &source.media_source,
            MediaSourceKind::Jellyfin,
            "jellyfin_media_sources",
            source,
        )
        .await
    }

    async fn upsert_emby(&self, source: &EmbyMediaSource) -> Result<()> {
        upsert_source(
            &self.0,
            &source.media_source,
            MediaSourceKind::Emby,
            "emby_media_sources",
            source,
        )
        .await
    }
}

async fn upsert_source<T>(
    pool: &Pool<Sqlite>,
    base: &MediaSource,
    kind: MediaSourceKind,
    provider_table: &str,
    provider: &T,
) -> Result<()>
where
    T: serde::Serialize + Sync,
{
    let mut base = base.clone();
    base.kind = kind;
    let mut tx = pool.begin().await?;
    sqlx::query(
        "INSERT OR REPLACE INTO media_sources (id, name, kind, last_scan, data)
         VALUES (?, ?, ?, ?, ?)",
    )
    .bind(base.id)
    .bind(&base.name)
    .bind(format!("{kind:?}"))
    .bind(base.last_scan.map(|value| value.to_rfc3339()))
    .bind(to_json(&base)?)
    .execute(&mut *tx)
    .await?;

    let sql = format!("INSERT OR REPLACE INTO {provider_table} (id, data) VALUES (?, ?)");
    sqlx::query(&sql)
        .bind(base.id)
        .bind(to_json(provider)?)
        .execute(&mut *tx)
        .await?;

    tx.commit().await?;
    Ok(())
}
