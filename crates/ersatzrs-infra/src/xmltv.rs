//! SQL-backed XMLTV guide projection.

use chrono::{DateTime, Duration, Utc};
use ersatzrs_core::programme::{project_occurrences, ProgrammeGuideItem, ProgrammeOccurrence};
use ersatzrs_core::xmltv::{
    XmltvBlockBehavior, XmltvGuideItem, XmltvProgramme, XmltvProgrammeContent,
};
use ersatzrs_domain::{FillerKind, CONFIG_KEY_XMLTV_BLOCK_BEHAVIOR};
use sqlx::{Pool, Row};

use crate::db::Db;
use crate::repositories::common::{
    decode_enum_column, decode_optional_datetime, try_decode_signed_duration_text,
};
use crate::xmltv_metadata::load_content_cache;

pub use crate::xmltv_read::{
    GuideExpansionLimits, GuideExpansionUsage, GuideReadBudget, GuideReadLimits, GuideReadUsage,
};
pub use crate::xmltv_snapshot::{
    capture_guide_snapshot, CapturedGuideChannel, GuideSnapshot, GuideSnapshotError,
    GuideSnapshotFailure, GuideSnapshotSource,
};

/// Metadata cache under the existing XMLTV hydration semantics.
pub type ProgrammeContentCache =
    std::collections::HashMap<i64, (Option<String>, XmltvProgrammeContent)>;

/// Read metadata through the caller's exact transaction and aggregate source budget.
///
/// This additive boundary preserves legacy metadata selection and hydration.
/// Source bounds apply before row retrieval and cache allocation. An expansion
/// policy additionally checks decoded lists and copies before allocation;
/// source and expansion usage remain separate from complete guide projection.
/// The transaction token pins preflight and retrieval to the same database
/// snapshot; an autocommit connection cannot call this boundary. The caller owns
/// the encompassing transaction and complete guide context.
///
/// # Errors
/// Returns database/decode, checked arithmetic or source-budget failures without
/// returning a partial cache. Successful earlier reads remain charged to budget.
pub async fn load_programme_content_in_transaction(
    transaction: &mut sqlx::Transaction<'_, Db>,
    media_item_ids: &[i64],
    budget: &mut GuideReadBudget,
) -> Result<ProgrammeContentCache, sqlx::Error> {
    let mut read = crate::xmltv_read::Read::snapshot(transaction, budget);
    crate::xmltv_metadata::load_content_cache_with_reader(&mut read, media_item_ids).await
}

const UNKNOWN_TITLE: &str = "Unknown";

/// Load XMLTV programme rows and apply the persisted XMLTV block behavior.
///
/// # Errors
///
/// Returns an error when a database row cannot be loaded or decoded.
pub async fn load_xmltv_programmes(
    pool: &Pool<Db>,
    start: DateTime<Utc>,
    finish: DateTime<Utc>,
) -> Result<Vec<XmltvProgramme>, sqlx::Error> {
    let behavior = load_xmltv_block_behavior(pool).await?;
    let items = load_guide_items(pool, start, finish, None).await?;
    Ok(project_occurrences(&items, behavior)
        .into_iter()
        .map(|occurrence| occurrence.programme)
        .filter(|programme| programme.start < finish && programme.stop > start)
        .map(|mut programme| {
            if programme.title.trim().is_empty() {
                UNKNOWN_TITLE.clone_into(&mut programme.title);
            }
            programme
        })
        .collect())
}

/// Load one channel's guide occurrences for native graphics or historical preview.
/// Hidden/disabled channels remain inspectable; no timeline or metadata is written.
///
/// # Errors
/// Returns a database/decode error, including when the bounded row limit is exceeded.
pub(crate) async fn load_channel_programmes(
    pool: &Pool<Db>,
    channel_id: i64,
    start: DateTime<Utc>,
    finish: DateTime<Utc>,
) -> Result<Vec<ProgrammeOccurrence>, sqlx::Error> {
    let behavior = load_xmltv_block_behavior(pool).await?;
    let items = load_guide_items(pool, start, finish, Some(channel_id)).await?;
    Ok(project_occurrences(&items, behavior))
}

async fn load_xmltv_block_behavior(pool: &Pool<Db>) -> Result<XmltvBlockBehavior, sqlx::Error> {
    let value =
        sqlx::query_scalar::<_, String>(r#"SELECT "Value" FROM "ConfigElement" WHERE "Key" = ?"#)
            .bind(CONFIG_KEY_XMLTV_BLOCK_BEHAVIOR)
            .fetch_optional(pool)
            .await?;

    Ok(value
        .as_deref()
        .and_then(XmltvBlockBehavior::parse)
        .unwrap_or_default())
}

#[allow(clippy::too_many_lines)]
async fn load_guide_items(
    pool: &Pool<Db>,
    start: DateTime<Utc>,
    finish: DateTime<Utc>,
    channel_id: Option<i64>,
) -> Result<Vec<ProgrammeGuideItem>, sqlx::Error> {
    // Project complete stored occurrences before applying the guide window.
    // An arbitrary time padding can truncate a long, split programme and invent
    // a new programme start. Native reads are scoped and capped at 8192 rows.
    let mut query = sqlx::QueryBuilder::new(
        r#"SELECT
               c."Number" AS "Number",
               c."PlayoutSource" AS "PlayoutSource",
               c."PlayoutOffset" AS "PlayoutOffset",
               p."Id" AS "PlayoutId",
               p."ScheduleKind" AS "ScheduleKind",
               pi."Id" AS "ItemId",
               pi."MediaItemId" AS "MediaItemId",
               pi."InPoint" AS "InPoint",
               pi."OutPoint" AS "OutPoint",
               pi."Start" AS "Start",
               pi."Finish" AS "Finish",
               pi."GuideStart" AS "GuideStart",
               pi."GuideFinish" AS "GuideFinish",
               pi."GuideGroup" AS "GuideGroup",
               pi."FillerKind" AS "FillerKind",
               pi."CustomTitle" AS "CustomTitle"
           FROM "Channel" c
           JOIN "Playout" p ON p."ChannelId" = CASE
               WHEN c."PlayoutSource" = 1 THEN c."MirrorSourceChannelId"
               ELSE c."Id"
           END
           JOIN "PlayoutItem" pi ON pi."PlayoutId" = p."Id"
           WHERE p."ScheduleKind" NOT IN (20, 'ExternalJson')"#,
    );
    if let Some(id) = channel_id {
        query.push(" AND c.\"Id\" = ").push_bind(id);
    } else {
        query.push(" AND c.\"IsEnabled\" = 1 AND c.\"ShowInEpg\" = 1");
    }
    query.push(" ORDER BY c.\"Number\", p.\"Id\", pi.\"Start\", pi.\"Id\"");
    if channel_id.is_some() {
        query.push(" LIMIT 8193");
    }
    let rows = query.build().fetch_all(pool).await?;
    if channel_id.is_some() && rows.len() > 8192 {
        return Err(sqlx::Error::Decode(
            "programme context exceeds 8192 source rows".into(),
        ));
    }

    let mut media_item_ids = rows
        .iter()
        .map(|row| row.try_get("MediaItemId"))
        .collect::<Result<Vec<i64>, _>>()?;
    media_item_ids.sort_unstable();
    media_item_ids.dedup();
    let content_cache = load_content_cache(pool, &media_item_ids).await?;

    let mut items = Vec::with_capacity(rows.len());
    for row in rows {
        items.push(materialize_guide_row(&row, &content_cache)?);
    }

    items.extend(
        crate::external_json::guide::load_programme_items(pool, channel_id, start, finish).await?,
    );
    Ok(items)
}

pub(crate) fn materialize_guide_row(
    row: &crate::db::DbRow,
    content_cache: &crate::xmltv_metadata::ContentCache,
) -> Result<ProgrammeGuideItem, sqlx::Error> {
    let start_text: String = row.try_get("Start")?;
    let finish_text: String = row.try_get("Finish")?;
    let custom_title = row
        .try_get::<Option<String>, _>("CustomTitle")?
        .filter(|value| !value.trim().is_empty());
    let media_item_id: i64 = row.try_get("MediaItemId")?;

    let source_start = parse_datetime(&start_text).ok_or_else(|| {
        sqlx::Error::Decode(format!("invalid PlayoutItem Start: {start_text}").into())
    })?;
    let source_finish = parse_datetime(&finish_text).ok_or_else(|| {
        sqlx::Error::Decode(format!("invalid PlayoutItem Finish: {finish_text}").into())
    })?;

    let (metadata_title, content) = content_cache
        .get(&media_item_id)
        .cloned()
        .unwrap_or_else(|| (None, XmltvProgrammeContent::default()));
    let title = metadata_title
        .filter(|title| !title.trim().is_empty())
        .unwrap_or_default();

    let filler_kind: FillerKind = decode_enum_column("xmltv filler_kind", row, "FillerKind")
        .map_err(|error| sqlx::Error::Decode(error.to_string().into()))?;

    let playout_source: i32 = row.try_get("PlayoutSource")?;
    let offset = if playout_source == 1 {
        row.try_get::<Option<String>, _>("PlayoutOffset")?
            .as_deref()
            .map(try_decode_signed_duration_text)
            .transpose()
            .map_err(|error| sqlx::Error::Decode(error.to_string().into()))?
            .unwrap_or_default()
    } else {
        Duration::zero()
    };
    let start_at = source_start.checked_add_signed(offset).ok_or_else(|| {
        sqlx::Error::Decode("XMLTV mirror start overflows supported range".into())
    })?;
    let finish_at = source_finish.checked_add_signed(offset).ok_or_else(|| {
        sqlx::Error::Decode("XMLTV mirror finish overflows supported range".into())
    })?;
    // Retain the padding rows until grouping is complete. Discarding them
    // here truncates split films and moves their guide start during previews.
    let project_optional = |value: Option<DateTime<Utc>>| {
        value
            .map(|value| {
                value.checked_add_signed(offset).ok_or_else(|| {
                    sqlx::Error::Decode("XMLTV mirror guide time overflows supported range".into())
                })
            })
            .transpose()
    };
    let guide_start = decode_optional_datetime("xmltv guide_start", row.try_get("GuideStart")?)
        .map_err(|error| sqlx::Error::Decode(error.to_string().into()))?;
    let guide_finish = decode_optional_datetime("xmltv guide_finish", row.try_get("GuideFinish")?)
        .map_err(|error| sqlx::Error::Decode(error.to_string().into()))?;

    Ok(ProgrammeGuideItem {
        playout_id: row.try_get("PlayoutId")?,
        item_id: row.try_get("ItemId")?,
        media_item_id: Some(media_item_id),
        schedule_kind: decode_enum_column("programme schedule kind", row, "ScheduleKind")
            .map_err(|error| sqlx::Error::Decode(error.to_string().into()))?,
        in_point: try_decode_signed_duration_text(&row.try_get::<String, _>("InPoint")?)
            .map_err(|error| sqlx::Error::Decode(error.to_string().into()))?,
        out_point: try_decode_signed_duration_text(&row.try_get::<String, _>("OutPoint")?)
            .map_err(|error| sqlx::Error::Decode(error.to_string().into()))?,
        guide: XmltvGuideItem {
            channel_number: row.try_get("Number")?,
            start: start_at,
            finish: finish_at,
            guide_start: project_optional(guide_start)?,
            guide_finish: project_optional(guide_finish)?,
            guide_group: row.try_get("GuideGroup")?,
            filler_kind,
            custom_title,
            title,
            content,
        },
    })
}

pub(crate) fn parse_datetime(text: &str) -> Option<DateTime<Utc>> {
    chrono::NaiveDateTime::parse_from_str(text, "%Y-%m-%d %H:%M:%S")
        .or_else(|_| chrono::NaiveDateTime::parse_from_str(text, "%Y-%m-%dT%H:%M:%S"))
        .or_else(|_| chrono::NaiveDateTime::parse_from_str(text, "%Y-%m-%d %H:%M:%S%.f"))
        .ok()
        .map(|ndt| DateTime::<Utc>::from_naive_utc_and_offset(ndt, Utc))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_missing_or_named_block_behavior() {
        assert_eq!(
            XmltvBlockBehavior::parse("SplitTimeEvenly"),
            Some(XmltvBlockBehavior::SplitTimeEvenly)
        );
        assert_eq!(
            XmltvBlockBehavior::parse("UseActualTimes"),
            Some(XmltvBlockBehavior::UseActualTimes)
        );
    }

    #[test]
    fn parses_sql_datetime_shapes() {
        assert!(parse_datetime("2026-04-17 20:00:00").is_some());
        assert!(parse_datetime("2026-04-17T20:00:00").is_some());
        assert!(parse_datetime("2026-04-17 20:00:00.123").is_some());
    }
}
