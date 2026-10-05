//! Read-only historical text resolution using the broadcast variable builder.

use ersatzrs_core::repositories::{
    ChannelRepository, GraphicsTextPreview, GraphicsTextPreviewRequest, RepositoryError, Result,
};
use ersatzrs_domain::TextGraphicsRenderer;
use serde_json::{Map, Value};
use sqlx::Row;

use crate::{
    channel_playout::ChannelPlayoutContext,
    db::DbPool,
    dynamic_text::{DynamicTextRenderContext, DynamicTextTemplate},
    graphics_template::{
        graphics_template_variables, render_graphics_template, GraphicsMedia,
        GraphicsTemplateContext,
    },
    repositories::{
        common::{decode_datetime, encode_datetime, try_decode_duration_text},
        make_repositories, SqlxResultExt as _,
    },
};

pub(crate) async fn preview(
    pool: &DbPool,
    request: &GraphicsTextPreviewRequest,
) -> Result<GraphicsTextPreview> {
    if request.channel_id <= 0
        || request.template.len() > 64 * 1024
        || !(16..=8192).contains(&request.width)
        || !(16..=8192).contains(&request.height)
        || request.epg_entries > 256
    {
        return Err(RepositoryError::Validation {
            field: "graphics preview",
            message: "invalid channel, resolution or template bounds".into(),
        });
    }
    let context = load_context(pool, request).await?;
    let overrides: Option<String> = if let Some(id) = request.element_id {
        sqlx::query_scalar(r#"SELECT "Variables" FROM "PlayoutItemGraphicsElement" WHERE "PlayoutItemId" = ? AND "GraphicsElementId" = ?"#)
            .bind(context.playout_item_id).bind(id).fetch_optional(pool).await.repository()?.flatten()
    } else {
        None
    };
    let variables =
        graphics_template_variables(pool, &context, request.epg_entries, overrides.as_deref())
            .await?;
    let text = match request.renderer {
        TextGraphicsRenderer::Drawtext => DynamicTextTemplate::compile(request.template.clone())
            .and_then(|template| {
                template.render(
                    &variables,
                    DynamicTextRenderContext {
                        now: request.render_time,
                        playback_elapsed_seconds: context.stream_seek_seconds,
                        playback_duration_seconds: context.duration_seconds,
                    },
                )
            })
            .map_err(|error| RepositoryError::Validation {
                field: "graphics preview template",
                message: error.to_string(),
            })?,
        TextGraphicsRenderer::Drawvg => render_graphics_template(&request.template, &variables)?,
    };
    let programme_context: Map<_, _> = [
        "CurrentProgramme",
        "NextProgramme",
        "ContinuingProgramme",
        "LaterToday",
        "Programme_ContextAmbiguous",
    ]
    .into_iter()
    .map(|key| {
        (
            key.to_owned(),
            variables.get(key).cloned().unwrap_or(Value::Null),
        )
    })
    .collect();
    Ok(GraphicsTextPreview {
        text,
        local_time: variables["Programme_LocalTime"]
            .as_str()
            .unwrap_or_default()
            .to_owned(),
        programme_context: Value::Object(programme_context),
    })
}

async fn load_context(
    pool: &DbPool,
    request: &GraphicsTextPreviewRequest,
) -> Result<GraphicsTemplateContext> {
    let channel = make_repositories(pool.clone())
        .channel
        .get_with_profile(request.channel_id)
        .await?;
    let projection = ChannelPlayoutContext::for_channel(&channel);
    let instant = encode_datetime(projection.source_time(request.render_time)?);
    // Resolve occurrence identity before joining versions/files, so duplicate
    // metadata or alternate files cannot masquerade as overlapping programmes.
    let rows = sqlx::query(
        r#"SELECT pi."Id", pi."MediaItemId", pi."Start", pi."Finish", pi."InPoint", pi."MediaVersionId"
           FROM "PlayoutItem" pi JOIN "Playout" p ON p."Id" = pi."PlayoutId"
           WHERE p."ChannelId" = ? AND pi."Start" <= ? AND pi."Finish" > ?
             AND p."ScheduleKind" NOT IN (20, 'ExternalJson')
           ORDER BY pi."Start", pi."Id" LIMIT 2"#,
    ).bind(projection.source_channel_id).bind(&instant).bind(&instant).fetch_all(pool).await.repository()?;
    if rows.len() != 1 {
        return Err(RepositoryError::Conflict(
            if rows.is_empty() {
                "graphics-preview-no-item"
            } else {
                "graphics-preview-ambiguous"
            }
            .into(),
        ));
    }
    let row = &rows[0];
    let item_id: i64 = row.try_get("Id").repository()?;
    let media_id: i64 = row.try_get("MediaItemId").repository()?;
    let start = projection.target_time(decode_datetime(
        "graphics preview start",
        &row.try_get::<String, _>("Start").repository()?,
    )?)?;
    let finish = projection.target_time(decode_datetime(
        "graphics preview finish",
        &row.try_get::<String, _>("Finish").repository()?,
    )?)?;
    let in_point = try_decode_duration_text(&row.try_get::<String, _>("InPoint").repository()?)?;
    let pinned: Option<i64> = row.try_get("MediaVersionId").repository()?;
    let media = load_media(pool, media_id, pinned).await?;
    let rate: Option<String> = media.try_get("RFrameRate").repository()?;
    let seconds = |duration: chrono::Duration| duration.as_seconds_f64();
    Ok(GraphicsTemplateContext {
        channel_id: channel.id,
        channel_number: channel.number,
        render_time: request.render_time,
        playout_item_id: Some(item_id),
        media: GraphicsMedia::Persisted(media_id),
        media_path: media.try_get("Path").repository()?,
        channel_start_time: start,
        content_start_time: start,
        content_finish_time: Some(finish),
        stream_seek_seconds: seconds(in_point + request.render_time.signed_duration_since(start)),
        duration_seconds: seconds(finish.signed_duration_since(start)),
        frame_width: request.width,
        frame_height: request.height,
        scaled_frame_width: request.width,
        scaled_frame_height: request.height,
        frame_rate: rate.as_deref().and_then(parse_frame_rate),
        r_frame_rate: rate,
    })
}

async fn load_media(pool: &DbPool, media_id: i64, pinned: Option<i64>) -> Result<crate::db::DbRow> {
    let media = sqlx::query(
        r#"SELECT mf."Path", mv."RFrameRate" FROM "MediaVersion" mv
           JOIN "MediaFile" mf ON mf."MediaVersionId" = mv."Id"
           WHERE (mv."MovieId" = ? OR mv."EpisodeId" = ? OR mv."MusicVideoId" = ?
              OR mv."OtherVideoId" = ? OR mv."RemoteStreamId" = ? OR mv."SongId" = ? OR mv."ImageId" = ?)
             AND (? IS NULL OR mv."Id" = ?) AND mf."Path" IS NOT NULL
           ORDER BY mv."Id", mf."Id" LIMIT 1"#,
    ).bind(media_id).bind(media_id).bind(media_id).bind(media_id).bind(media_id).bind(media_id).bind(media_id)
        .bind(pinned).bind(pinned).fetch_optional(pool).await.repository()?;
    media.ok_or_else(|| RepositoryError::Conflict("graphics-preview-no-media".into()))
}

fn parse_frame_rate(value: &str) -> Option<f64> {
    let (numerator, denominator) = value.split_once('/')?;
    let rate = numerator.parse::<f64>().ok()? / denominator.parse::<f64>().ok()?;
    (rate.is_finite() && rate > 0.0).then_some(rate)
}
