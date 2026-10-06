//! Bounded reads used by the existing producer-owned dynamic text controller.

use chrono::{DateTime, Utc};
use serde_json::Value;
use sqlx::Row;

use crate::{
    db::DbPool,
    dynamic_text::DynamicTextVariables,
    graphics_template::{graphics_template_variables, GraphicsMedia, GraphicsTemplateContext},
};

pub(crate) struct GraphicsMetadataRefresh {
    pub pool: DbPool,
    pub context: GraphicsTemplateContext,
    pub element_id: i64,
    pub epg_entries: usize,
    pub startup_overrides: Option<String>,
}

#[async_trait::async_trait]
impl DynamicTextVariables for GraphicsMetadataRefresh {
    async fn load(&self, render_time: DateTime<Utc>) -> Result<Value, String> {
        let mut context = self.context.clone();
        context.render_time = render_time;
        let overrides = match (context.playout_item_id, &context.media) {
            (Some(item_id), GraphicsMedia::Persisted(media_id)) => {
                // Reject a deleted/reused occurrence rather than applying another
                // item's variables. Attachment removal deliberately removes its override.
                let row = sqlx::query(
                    r#"SELECT ge."Variables", pi."Start", pi."Finish", c."PlayoutSource", c."PlayoutOffset"
                       FROM "PlayoutItem" pi
                       JOIN "Playout" p ON p."Id" = pi."PlayoutId"
                       JOIN "Channel" c ON c."Id" = ?
                       LEFT JOIN "PlayoutItemGraphicsElement" ge
                         ON ge."PlayoutItemId" = pi."Id" AND ge."GraphicsElementId" = ?
                       WHERE pi."Id" = ? AND pi."MediaItemId" = ?
                         AND p."ChannelId" = CASE WHEN c."PlayoutSource" = 1 THEN c."MirrorSourceChannelId" ELSE c."Id" END"#,
                )
                .bind(context.channel_id)
                .bind(self.element_id)
                .bind(item_id)
                .bind(media_id)
                .fetch_optional(&self.pool)
                .await
                .map_err(|error| error.to_string())?;
                let row = row.ok_or_else(|| {
                    "graphics programme occurrence is no longer available".to_owned()
                })?;
                let offset = if row
                    .try_get::<i32, _>("PlayoutSource")
                    .map_err(|error| error.to_string())?
                    == 1
                {
                    row.try_get::<Option<String>, _>("PlayoutOffset")
                        .map_err(|error| error.to_string())?
                        .as_deref()
                        .map(crate::repositories::common::try_decode_signed_duration_text)
                        .transpose()
                        .map_err(|error| error.to_string())?
                        .unwrap_or_default()
                } else {
                    chrono::Duration::zero()
                };
                let timestamp = |column| -> Result<DateTime<Utc>, String> {
                    let value: String = row.try_get(column).map_err(|error| error.to_string())?;
                    crate::repositories::common::decode_datetime("graphics occurrence", &value)
                        .map_err(|error| error.to_string())?
                        .checked_add_signed(offset)
                        .ok_or_else(|| "graphics occurrence time overflows".to_owned())
                };
                if timestamp("Start")? != context.content_start_time
                    || context
                        .content_finish_time
                        .is_some_and(|finish| timestamp("Finish").ok() != Some(finish))
                {
                    return Err("graphics programme occurrence identity changed".to_owned());
                }
                row.try_get("Variables")
                    .map_err(|error| error.to_string())?
            }
            _ => self.startup_overrides.clone(),
        };
        graphics_template_variables(&self.pool, &context, self.epg_entries, overrides.as_deref())
            .await
            .map_err(|error| error.to_string())
    }
}
