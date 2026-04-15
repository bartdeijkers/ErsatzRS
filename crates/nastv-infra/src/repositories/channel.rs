use async_trait::async_trait;
use nastv_core::repositories::{ChannelRepository, Result};
use nastv_domain::{Channel, EntityId};
use sqlx::{Pool, Sqlite};

use super::common::{fetch_data, from_json, load_ids, replace_ids, to_json};

#[derive(Clone)]
pub struct SqliteChannelRepository(pub Pool<Sqlite>);

#[async_trait]
impl ChannelRepository for SqliteChannelRepository {
    async fn get_with_profile(&self, id: EntityId) -> Result<Channel> {
        let mut channel: Channel = from_json(
            "channel",
            id,
            fetch_data(&self.0, "channels", "channel", id).await?,
        )?;
        channel.playout_ids = load_ids(
            &self.0,
            "SELECT id FROM playouts WHERE channel_id = ? ORDER BY id",
            id,
        )
        .await?;
        channel.artwork_ids = load_ids(
            &self.0,
            "SELECT artwork_id FROM channel_artwork WHERE channel_id = ? ORDER BY artwork_id",
            id,
        )
        .await?;
        Ok(channel)
    }

    async fn get_by_number(&self, number: &str) -> Result<Channel> {
        let row =
            sqlx::query_as::<_, (i64, String)>("SELECT id, data FROM channels WHERE number = ?")
                .bind(number)
                .fetch_one(&self.0)
                .await?;
        from_json("channel", row.0, row.1)
    }

    async fn list_enabled(&self) -> Result<Vec<Channel>> {
        let rows = sqlx::query_as::<_, (i64, String)>(
            "SELECT id, data FROM channels WHERE is_enabled = 1 ORDER BY sort_number, number",
        )
        .fetch_all(&self.0)
        .await?;
        rows.into_iter()
            .map(|(id, data)| {
                let mut channel: Channel = from_json("channel", id, data)?;
                channel.playout_ids.clear();
                channel.artwork_ids.clear();
                Ok(channel)
            })
            .collect()
    }

    async fn upsert(&self, channel: &Channel) -> Result<()> {
        let mut tx = self.0.begin().await?;
        sqlx::query(
            "INSERT OR REPLACE INTO channels (
                id, unique_id, number, sort_number, name, ffmpeg_profile_id,
                watermark_id, fallback_filler_id, is_enabled, data
             ) VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?)",
        )
        .bind(channel.id)
        .bind(channel.unique_id.to_string())
        .bind(&channel.number)
        .bind(channel.sort_number)
        .bind(&channel.name)
        .bind(channel.ffmpeg_profile_id)
        .bind(channel.watermark_id)
        .bind(channel.fallback_filler_id)
        .bind(channel.is_enabled)
        .bind(to_json(channel)?)
        .execute(&mut *tx)
        .await?;

        replace_ids(
            &mut tx,
            "DELETE FROM channel_artwork WHERE channel_id = ?",
            "INSERT OR REPLACE INTO channel_artwork (channel_id, artwork_id) VALUES (?, ?)",
            channel.id,
            &channel.artwork_ids,
        )
        .await?;

        tx.commit().await?;
        Ok(())
    }

    async fn delete(&self, id: EntityId) -> Result<()> {
        sqlx::query("DELETE FROM channels WHERE id = ?")
            .bind(id)
            .execute(&self.0)
            .await?;
        Ok(())
    }
}
