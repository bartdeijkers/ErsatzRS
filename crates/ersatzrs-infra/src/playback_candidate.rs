//! Snapshot overlay inputs through the candidate's own SQL connection.

use chrono::{DateTime, Utc};
use ersatzrs_core::repositories::Result;
use ersatzrs_domain::{Channel, StreamingMode};

use super::{
    active_template_deco_id_from_connection, load_playback_watermark_context,
    load_playout_item_graphics, load_playout_item_watermark_ids, select_context_graphics,
    select_context_watermarks, PlaybackWatermarkContext, ResolvedPlaybackGraphicsElement,
    ResolvedPlaybackWatermark,
};
use crate::db::DbPool;

/// No Debug: graphics variables may contain private programme metadata.
#[derive(Clone)]
pub(crate) struct CandidateOverlayContext {
    context: PlaybackWatermarkContext,
    pub(super) template_deco_id: Option<i64>,
    pub(super) watermark_ids: Vec<i64>,
    pub(super) graphics: Vec<(i64, Option<String>)>,
}

impl CandidateOverlayContext {
    pub(crate) async fn read(
        connection: &mut sqlx::SqliteConnection,
        item_id: i64,
        source_time: DateTime<Utc>,
    ) -> Result<Self> {
        let context = load_playback_watermark_context(&mut *connection, item_id).await?;
        let template_deco_id =
            active_template_deco_id_from_connection(connection, context.playout_id, source_time)
                .await?;
        Ok(Self {
            context,
            template_deco_id,
            watermark_ids: load_playout_item_watermark_ids(&mut *connection, item_id).await?,
            graphics: load_playout_item_graphics(connection, item_id).await?,
        })
    }

    pub(crate) async fn resolve(
        &self,
        pool: &DbPool,
        channel: &Channel,
        disable_watermarks: bool,
        source_time: DateTime<Utc>,
    ) -> Result<(
        Vec<ResolvedPlaybackWatermark>,
        Vec<ResolvedPlaybackGraphicsElement>,
    )> {
        let watermarks = if disable_watermarks
            || channel.streaming_mode == StreamingMode::HttpLiveStreamingDirect
        {
            Vec::new()
        } else {
            select_context_watermarks(pool, channel, self.context, None, source_time, Some(self))
                .await?
        };
        let graphics =
            select_context_graphics(pool, self.context, None, source_time, Some(self)).await?;
        Ok((watermarks, graphics))
    }
}
