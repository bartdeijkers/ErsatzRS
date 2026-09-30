//! Render-time graphics template variables.
//!
//! Source map:
//! - `$ERSATZTV_SRC/ErsatzTV.Infrastructure/Streaming/Graphics/GraphicsElementLoader.cs`
//!   initializes common graphics variables, media item variables, and EPG variables before
//!   rendering YAML with Scriban.
//! - `$ERSATZTV_SRC/ErsatzTV.Core/Metadata/*TemplateDataKey.cs` defines the persisted key
//!   names (`MediaItem_Duration`, `Epg`, `Channel_StartTime`, etc.).
use crate::repositories::SqlxResultExt as _;

#[path = "graphics_template_external.rs"]
mod external;
pub(crate) use external::media as external_media;

use std::collections::BTreeMap;

use chrono::{DateTime, Utc};
use ersatzrs_core::repositories::{RepositoryError, Result};
use serde_json::{Map, Number, Value};
use sqlx::Row;

use crate::db::{DbPool, DbRow};
use crate::repositories::common::{decode_datetime, encode_datetime};

#[derive(Debug, Clone, PartialEq)]
pub enum GraphicsMedia {
    /// Resolve library metadata when preparing graphics.
    Persisted(i64),
    /// Retain the selected programme's metadata without a library lookup.
    Transient(Map<String, Value>),
}

#[derive(Debug, Clone, PartialEq)]
pub struct GraphicsTemplateContext {
    pub channel_id: i64,
    pub channel_number: String,
    pub media: GraphicsMedia,
    pub media_path: String,
    pub channel_start_time: DateTime<Utc>,
    pub content_start_time: DateTime<Utc>,
    /// Scheduled finish, independent of the current rendering chunk.
    /// None retains the duration-derived stop for inputs without a known finish.
    pub content_finish_time: Option<DateTime<Utc>>,
    pub stream_seek_seconds: f64,
    pub duration_seconds: f64,
    pub frame_width: u32,
    pub frame_height: u32,
    pub scaled_frame_width: u32,
    pub scaled_frame_height: u32,
    pub r_frame_rate: Option<String>,
    pub frame_rate: Option<f64>,
}

/// Build ErsatzTV-compatible graphics template variables and apply per-item overrides.
///
/// # Errors
///
/// Returns database errors from media/EPG queries, decode errors from persisted date
/// values, or decode errors when `per_item_variables` is not a JSON object.
pub async fn graphics_template_variables(
    pool: &DbPool,
    context: &GraphicsTemplateContext,
    epg_entries: usize,
    per_item_variables: Option<&str>,
) -> Result<Value> {
    let mut root = Map::new();
    insert_common_variables(&mut root, context);
    let media = match &context.media {
        GraphicsMedia::Persisted(id) => load_media_item_template_data(pool, *id).await?,
        GraphicsMedia::Transient(data) => Some(data.clone()),
    };
    if let Some(media) = media {
        merge_object(&mut root, media);
    }
    if epg_entries > 0 {
        root.insert(
            "Epg".to_owned(),
            Value::Array(load_epg_template_data(pool, context, epg_entries).await?),
        );
    }
    if let Some(overrides) = parse_variable_overrides(per_item_variables)? {
        merge_object(&mut root, overrides);
    }
    Ok(Value::Object(root))
}

/// Render `{{ ... }}` graphics placeholders from a variable map.
///
/// # Errors
///
/// Returns a decode error when a placeholder references a missing value or an
/// unsupported expression.
pub fn render_graphics_template(template: &str, variables: &Value) -> Result<String> {
    let mut rendered = String::with_capacity(template.len());
    let mut rest = template;

    while let Some(start) = rest.find("{{") {
        let (before, after_start) = rest.split_at(start);
        rendered.push_str(before);
        let Some(end) = after_start.find("}}") else {
            return Err(RepositoryError::Decode {
                kind: "graphics template",
                value: "unclosed placeholder".to_owned(),
            });
        };
        let expression = after_start[2..end].trim();
        rendered.push_str(&render_expression(expression, variables)?);
        rest = &after_start[end + 2..];
    }

    rendered.push_str(rest);
    Ok(rendered)
}

/// Evaluate a numeric graphics expression.
///
/// # Errors
///
/// Returns a decode error when the expression cannot be resolved to a number.
pub fn evaluate_graphics_number(expression: &str, variables: &Value) -> Result<f64> {
    eval_numeric_expression(expression.trim(), variables)
}

/// Flatten top-level scalar values for SVG template rendering.
#[must_use]
pub fn scalar_template_variables(variables: &Value) -> BTreeMap<String, String> {
    let mut result = BTreeMap::new();
    let Some(object) = variables.as_object() else {
        return result;
    };
    for (key, value) in object {
        if let Some(rendered) = value_to_template_string(value) {
            result.insert(key.clone(), rendered);
        }
    }
    result
}

fn insert_common_variables(root: &mut Map<String, Value>, context: &GraphicsTemplateContext) {
    root.insert(
        "Resolution".to_owned(),
        resolution_value(context.frame_width, context.frame_height),
    );
    root.insert(
        "ScaledResolution".to_owned(),
        resolution_value(context.scaled_frame_width, context.scaled_frame_height),
    );
    if let Some(r_frame_rate) = &context.r_frame_rate {
        root.insert("RFrameRate".to_owned(), Value::String(r_frame_rate.clone()));
    }
    if let Some(frame_rate) = context.frame_rate {
        root.insert("FrameRate".to_owned(), number_value(frame_rate));
    }
    root.insert(
        "Channel_Number".to_owned(),
        Value::String(context.channel_number.clone()),
    );
    root.insert(
        "Channel_StartTime".to_owned(),
        Value::String(context.channel_start_time.to_rfc3339()),
    );
    root.insert(
        "MediaItem_SeekSeconds".to_owned(),
        number_value(context.stream_seek_seconds),
    );
    root.insert(
        "MediaItem_StreamSeek".to_owned(),
        duration_value(context.stream_seek_seconds),
    );
    root.insert(
        "MediaItem_Start".to_owned(),
        Value::String(context.content_start_time.to_rfc3339()),
    );
    root.insert(
        "MediaItem_Stop".to_owned(),
        Value::String(
            context
                .content_finish_time
                .unwrap_or_else(|| {
                    context.content_start_time + duration_millis(context.duration_seconds)
                })
                .to_rfc3339(),
        ),
    );
    root.insert(
        "MediaItem_Path".to_owned(),
        Value::String(context.media_path.clone()),
    );
    root.insert(
        "MediaItem_Duration".to_owned(),
        duration_value(context.duration_seconds),
    );
}

async fn load_media_item_template_data(
    pool: &DbPool,
    media_item_id: i64,
) -> Result<Option<Map<String, Value>>> {
    let row = sqlx::query(
        r#"SELECT mv."Width", mv."Height", mv."Duration",
                  COALESCE(mm."Title", em."Title", mvm."Title", ovm."Title", rsm."Title", sm."Title") AS "Title",
                  COALESCE(mm."Plot", em."Plot", mvm."Plot", ovm."Plot") AS "Plot",
                  COALESCE(mm."ReleaseDate", em."ReleaseDate", mvm."ReleaseDate", ovm."ReleaseDate") AS "ReleaseDate",
                  mm."ContentRating" AS "ContentRating",
                  smd."Title" AS "ShowTitle",
                  smd."Year" AS "ShowYear",
                  smd."ContentRating" AS "ShowContentRating",
                  mvm."Track" AS "Track",
                  mvm."Album" AS "Album",
                  amd."Title" AS "Artist"
           FROM "MediaItem" mi
           LEFT JOIN "Movie" movie ON movie."Id" = mi."Id"
           LEFT JOIN "MovieMetadata" mm ON mm."MovieId" = movie."Id"
           LEFT JOIN "Episode" episode ON episode."Id" = mi."Id"
           LEFT JOIN "EpisodeMetadata" em ON em."EpisodeId" = episode."Id"
           LEFT JOIN "Season" season ON season."Id" = episode."SeasonId"
           LEFT JOIN "Show" show ON show."Id" = season."ShowId"
           LEFT JOIN "ShowMetadata" smd ON smd."ShowId" = show."Id"
           LEFT JOIN "MusicVideo" music_video ON music_video."Id" = mi."Id"
           LEFT JOIN "MusicVideoMetadata" mvm ON mvm."MusicVideoId" = music_video."Id"
           LEFT JOIN "ArtistMetadata" amd ON amd."ArtistId" = music_video."ArtistId"
           LEFT JOIN "OtherVideo" other_video ON other_video."Id" = mi."Id"
           LEFT JOIN "OtherVideoMetadata" ovm ON ovm."OtherVideoId" = other_video."Id"
           LEFT JOIN "RemoteStream" remote_stream ON remote_stream."Id" = mi."Id"
           LEFT JOIN "RemoteStreamMetadata" rsm ON rsm."RemoteStreamId" = remote_stream."Id"
           LEFT JOIN "Song" song ON song."Id" = mi."Id"
           LEFT JOIN "SongMetadata" sm ON sm."SongId" = song."Id"
           LEFT JOIN "MediaVersion" mv
               ON mv."MovieId" = mi."Id"
               OR mv."EpisodeId" = mi."Id"
               OR mv."MusicVideoId" = mi."Id"
               OR mv."OtherVideoId" = mi."Id"
               OR mv."RemoteStreamId" = mi."Id"
               OR mv."SongId" = mi."Id"
               OR mv."ImageId" = mi."Id"
           WHERE mi."Id" = ?
           ORDER BY mv."Id", mm."Id", em."Id", mvm."Id", ovm."Id", rsm."Id", sm."Id"
           LIMIT 1"#,
    )
    .bind(media_item_id)
    .fetch_optional(pool)
    .await.repository()?;

    let Some(row) = row else {
        return Ok(None);
    };

    Ok(Some(media_item_template_data_from_row(&row)?))
}

fn media_item_template_data_from_row(row: &DbRow) -> Result<Map<String, Value>> {
    let mut result = Map::new();
    insert_optional_string(
        &mut result,
        "MediaItem_Title",
        row.try_get("Title").repository()?,
    );
    insert_optional_string(
        &mut result,
        "MediaItem_Plot",
        row.try_get("Plot").repository()?,
    );
    insert_optional_string(
        &mut result,
        "MediaItem_ReleaseDate",
        row.try_get("ReleaseDate").repository()?,
    );
    insert_optional_string(
        &mut result,
        "MediaItem_ContentRating",
        row.try_get("ContentRating").repository()?,
    );
    insert_optional_string(
        &mut result,
        "MediaItem_ShowTitle",
        row.try_get("ShowTitle").repository()?,
    );
    if let Some(year) = row.try_get::<Option<i32>, _>("ShowYear").repository()? {
        result.insert("MediaItem_ShowYear".to_owned(), Value::Number(year.into()));
    }
    insert_optional_string(
        &mut result,
        "MediaItem_ShowContentRating",
        row.try_get("ShowContentRating").repository()?,
    );
    if let Some(track) = row.try_get::<Option<i32>, _>("Track").repository()? {
        result.insert("MediaItem_Track".to_owned(), Value::Number(track.into()));
    }
    insert_optional_string(
        &mut result,
        "MediaItem_Album",
        row.try_get("Album").repository()?,
    );
    insert_optional_string(
        &mut result,
        "MediaItem_Artist",
        row.try_get("Artist").repository()?,
    );
    let width = row.try_get::<Option<i32>, _>("Width").repository()?;
    let height = row.try_get::<Option<i32>, _>("Height").repository()?;
    if let (Some(width), Some(height)) = (width, height) {
        result.insert(
            "MediaItem_Resolution".to_owned(),
            resolution_value(
                u32::try_from(width).unwrap_or_default(),
                u32::try_from(height).unwrap_or_default(),
            ),
        );
    }
    if let Some(duration) = row.try_get::<Option<String>, _>("Duration").repository()? {
        let seconds = parse_duration_seconds(&duration);
        result.insert("MediaItem_Duration".to_owned(), duration_value(seconds));
    }
    Ok(result)
}

async fn load_epg_template_data(
    pool: &DbPool,
    context: &GraphicsTemplateContext,
    count: usize,
) -> Result<Vec<Value>> {
    if matches!(context.media, GraphicsMedia::Transient(_)) {
        return external::epg(pool, context, count).await;
    }
    let rows = sqlx::query(
        r#"SELECT pi."Start", pi."Finish",
                  COALESCE(pi."CustomTitle", mm."Title", em."Title", mvm."Title", ovm."Title", rsm."Title", sm."Title", '') AS "Title"
           FROM "PlayoutItem" pi
           JOIN "Playout" p ON p."Id" = pi."PlayoutId"
           LEFT JOIN "MovieMetadata" mm ON mm."MovieId" = pi."MediaItemId"
           LEFT JOIN "EpisodeMetadata" em ON em."EpisodeId" = pi."MediaItemId"
           LEFT JOIN "MusicVideoMetadata" mvm ON mvm."MusicVideoId" = pi."MediaItemId"
           LEFT JOIN "OtherVideoMetadata" ovm ON ovm."OtherVideoId" = pi."MediaItemId"
           LEFT JOIN "RemoteStreamMetadata" rsm ON rsm."RemoteStreamId" = pi."MediaItemId"
           LEFT JOIN "SongMetadata" sm ON sm."SongId" = pi."MediaItemId"
           WHERE p."ChannelId" = ? AND pi."Finish" > ?
           ORDER BY pi."Start", pi."Id"
           LIMIT ?"#,
    )
    .bind(context.channel_id)
    .bind(encode_datetime(context.content_start_time))
    .bind(i64::try_from(count).unwrap_or(i64::MAX))
    .fetch_all(pool)
    .await.repository()?;

    let mut result = Vec::with_capacity(rows.len());
    for row in rows {
        let start = decode_datetime(
            "playout item start",
            &row.try_get::<String, _>("Start").repository()?,
        )?;
        let stop = decode_datetime(
            "playout item finish",
            &row.try_get::<String, _>("Finish").repository()?,
        )?;
        let mut item = Map::new();
        item.insert(
            "Title".to_owned(),
            Value::String(row.try_get("Title").repository()?),
        );
        item.insert("SubTitle".to_owned(), Value::String(String::new()));
        item.insert("Description".to_owned(), Value::String(String::new()));
        item.insert("Rating".to_owned(), Value::String(String::new()));
        item.insert("Categories".to_owned(), Value::Array(Vec::new()));
        item.insert("Date".to_owned(), Value::String(String::new()));
        item.insert("Start".to_owned(), Value::String(start.to_rfc3339()));
        item.insert("Stop".to_owned(), Value::String(stop.to_rfc3339()));
        result.push(Value::Object(item));
    }
    Ok(result)
}

fn parse_variable_overrides(value: Option<&str>) -> Result<Option<Map<String, Value>>> {
    let Some(value) = value.map(str::trim).filter(|value| !value.is_empty()) else {
        return Ok(None);
    };
    let value: Value = serde_json::from_str(value).map_err(|error| RepositoryError::Decode {
        kind: "graphics element variables",
        value: error.to_string(),
    })?;
    value
        .as_object()
        .cloned()
        .map(Some)
        .ok_or_else(|| RepositoryError::Decode {
            kind: "graphics element variables",
            value: "expected JSON object".to_owned(),
        })
}

fn merge_object(target: &mut Map<String, Value>, source: Map<String, Value>) {
    for (key, value) in source {
        target.insert(key, value);
    }
}

fn render_expression(expression: &str, variables: &Value) -> Result<String> {
    if contains_numeric_operator(expression) {
        let value = eval_numeric_expression(expression, variables)?;
        return Ok(format_number(value));
    }
    let value = resolve_path(expression, variables).ok_or_else(|| RepositoryError::Decode {
        kind: "graphics template",
        value: format!("unknown variable {expression}"),
    })?;
    value_to_template_string(value).ok_or_else(|| RepositoryError::Decode {
        kind: "graphics template",
        value: format!("{expression} is not a scalar value"),
    })
}

fn eval_numeric_expression(expression: &str, variables: &Value) -> Result<f64> {
    let tokens = expression.split_whitespace().collect::<Vec<_>>();
    match tokens.as_slice() {
        [single] => numeric_term(single, variables),
        [left, op @ ("+" | "-"), right] => {
            let left = numeric_term(left, variables)?;
            let right = numeric_term(right, variables)?;
            Ok(if *op == "+" {
                left + right
            } else {
                left - right
            })
        }
        _ => Err(RepositoryError::Decode {
            kind: "graphics numeric expression",
            value: expression.to_owned(),
        }),
    }
}

fn numeric_term(term: &str, variables: &Value) -> Result<f64> {
    if let Ok(value) = term.parse::<f64>() {
        return Ok(value);
    }
    let value = resolve_path(term, variables).ok_or_else(|| RepositoryError::Decode {
        kind: "graphics numeric expression",
        value: format!("unknown variable {term}"),
    })?;
    value.as_f64().ok_or_else(|| RepositoryError::Decode {
        kind: "graphics numeric expression",
        value: format!("{term} is not numeric"),
    })
}

fn resolve_path<'a>(path: &str, variables: &'a Value) -> Option<&'a Value> {
    let mut current = variables;
    for part in path.split('.') {
        if part.is_empty() {
            return None;
        }
        let mut field = part;
        loop {
            if let Some((before, after)) = field.split_once('[') {
                if !before.is_empty() {
                    current = current.get(before)?;
                }
                let (index, rest) = after.split_once(']')?;
                current = current.get(index.parse::<usize>().ok()?)?;
                field = rest;
                if field.is_empty() {
                    break;
                }
            } else {
                current = current.get(field)?;
                break;
            }
        }
    }
    Some(current)
}

fn contains_numeric_operator(expression: &str) -> bool {
    let tokens = expression.split_whitespace().collect::<Vec<_>>();
    matches!(tokens.as_slice(), [_, "+" | "-", _])
}

fn value_to_template_string(value: &Value) -> Option<String> {
    match value {
        Value::Null => Some(String::new()),
        Value::Bool(value) => Some(value.to_string()),
        Value::Number(value) => Some(value.to_string()),
        Value::String(value) => Some(value.clone()),
        Value::Array(_) | Value::Object(_) => None,
    }
}

fn insert_optional_string(target: &mut Map<String, Value>, key: &str, value: Option<String>) {
    if let Some(value) = value {
        target.insert(key.to_owned(), Value::String(value));
    }
}

fn resolution_value(width: u32, height: u32) -> Value {
    let mut value = Map::new();
    value.insert("Width".to_owned(), Value::Number(width.into()));
    value.insert("Height".to_owned(), Value::Number(height.into()));
    Value::Object(value)
}

fn duration_value(total_seconds: f64) -> Value {
    let mut value = Map::new();
    value.insert("TotalSeconds".to_owned(), number_value(total_seconds));
    value.insert(
        "TotalMinutes".to_owned(),
        number_value(total_seconds / 60.0),
    );
    value.insert(
        "TotalHours".to_owned(),
        number_value(total_seconds / 3600.0),
    );
    Value::Object(value)
}

fn duration_millis(total_seconds: f64) -> chrono::Duration {
    let millis = (total_seconds * 1000.0).round();
    chrono::Duration::milliseconds(millis.to_string().parse::<i64>().unwrap_or_default())
}

fn number_value(value: f64) -> Value {
    Value::Number(Number::from_f64(value).unwrap_or_else(|| Number::from(0)))
}

fn format_number(value: f64) -> String {
    if value.fract().abs() < f64::EPSILON {
        format!("{value:.0}")
    } else {
        value.to_string()
    }
}

fn parse_duration_seconds(value: &str) -> f64 {
    let (day_text, time_text) = value.rsplit_once('.').map_or(("", value), |(left, right)| {
        if left.contains(':') {
            ("", value)
        } else {
            (left, right)
        }
    });
    let mut parts = time_text.split(':');
    let hours = parts
        .next()
        .and_then(|value| value.parse::<f64>().ok())
        .unwrap_or_default();
    let minutes = parts
        .next()
        .and_then(|value| value.parse::<f64>().ok())
        .unwrap_or_default();
    let seconds = parts
        .next()
        .and_then(|value| value.parse::<f64>().ok())
        .unwrap_or_default();
    let days = day_text.parse::<f64>().unwrap_or_default();
    days.mul_add(
        86_400.0,
        hours.mul_add(3600.0, minutes.mul_add(60.0, seconds)),
    )
}

#[cfg(test)]
mod tests {
    use serde_json::json;

    use super::{evaluate_graphics_number, render_graphics_template};

    #[test]
    fn renders_dotted_and_indexed_placeholders() {
        let variables = json!({
            "MediaItem_Title": "Movie",
            "Epg": [{ "Title": "Now" }]
        });

        let rendered =
            render_graphics_template("{{ MediaItem_Title }} / {{ Epg[0].Title }}", &variables)
                .unwrap();

        assert_eq!(rendered, "Movie / Now");
    }

    #[test]
    fn evaluates_duration_arithmetic() {
        let variables = json!({
            "MediaItem_Duration": { "TotalSeconds": 120.0 }
        });

        let value =
            evaluate_graphics_number("MediaItem_Duration.TotalSeconds - 5", &variables).unwrap();
        assert!((value - 115.0).abs() < f64::EPSILON);
    }
}
