//! External programme metadata and guide variables without persisted media IDs.

use super::{
    duration_value, insert_optional_string, resolution_value, GraphicsMedia,
    GraphicsTemplateContext,
};
use crate::{
    db::DbPool, external_json::playback::ResolvedExternalMedia, repositories::SqlxResultExt as _,
};
use chrono::{DateTime, Utc};
use ersatzrs_core::repositories::Result;
use serde_json::{json, Map, Value};

pub(crate) fn media(resolved: &ResolvedExternalMedia) -> GraphicsMedia {
    let program = &resolved.occurrence.program;
    let metadata = resolved.plex_metadata.as_deref();
    let episode = metadata.is_none() || program.kind.as_deref() == Some("episode");
    let mut values = Map::new();
    insert_optional_string(
        &mut values,
        "MediaItem_Title",
        metadata
            .map(|m| m.title.clone())
            .or_else(|| program.title.clone()),
    );
    insert_optional_string(
        &mut values,
        "MediaItem_Plot",
        metadata.and_then(|m| m.summary.clone()),
    );
    insert_optional_string(
        &mut values,
        "MediaItem_ReleaseDate",
        metadata.and_then(|m| m.release_date.clone()),
    );
    if episode {
        insert_optional_string(
            &mut values,
            "MediaItem_ShowTitle",
            program.show_title.clone(),
        );
    } else {
        insert_optional_string(
            &mut values,
            "MediaItem_ContentRating",
            metadata.and_then(|m| m.content_rating.clone()),
        );
    }
    values.insert(
        "MediaItem_Resolution".into(),
        resolution_value(
            u32::try_from(resolved.probe.version.width).unwrap_or_default(),
            u32::try_from(resolved.probe.version.height).unwrap_or_default(),
        ),
    );
    values.insert(
        "MediaItem_Duration".into(),
        duration_value(
            resolved
                .probe
                .version
                .duration
                .to_std()
                .map_or(0.0, |duration| duration.as_secs_f64()),
        ),
    );
    GraphicsMedia::Transient(values)
}

pub(super) async fn epg(
    pool: &DbPool,
    context: &GraphicsTemplateContext,
    count: usize,
) -> Result<Vec<Value>> {
    let items = crate::external_json::guide::load_channel_items(
        pool,
        context.channel_id,
        context.content_start_time,
        DateTime::<Utc>::MAX_UTC,
    )
    .await
    .repository()?;
    Ok(items
        .into_iter()
        .take(count)
        .map(|item| {
            json!({
                "Title": item.title,
                "SubTitle": item.content.sub_title.unwrap_or_default(),
                "Description": item.content.description.unwrap_or_default(),
                "Rating": item.content.content_rating.unwrap_or_default(),
                "Categories": item.content.genres,
                "Date": item.content.year.map(|year| year.to_string()).unwrap_or_default(),
                "Start": item.start.to_rfc3339(),
                "Stop": item.finish.to_rfc3339(),
            })
        })
        .collect())
}
