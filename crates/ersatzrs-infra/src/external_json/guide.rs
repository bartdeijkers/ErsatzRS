//! RefreshChannelDataHandler.CollectExternalJsonItems guide projection.

use chrono::{DateTime, Duration, Utc};
use ersatzrs_core::{
    repositories::ArtworkReference,
    xmltv::{IconSizeHint, XmltvGuideItem, XmltvItemKind, XmltvProgrammeContent},
};
use ersatzrs_domain::{ArtworkKind, FillerKind};
use sqlx::Row;

use crate::{db::DbPool, repositories::common::try_decode_signed_duration_text};

pub(crate) async fn load_items(
    pool: &DbPool,
    start: DateTime<Utc>,
    finish: DateTime<Utc>,
) -> Result<Vec<XmltvGuideItem>, sqlx::Error> {
    load_filtered_items(pool, None, start, finish).await
}

pub(crate) async fn load_channel_items(
    pool: &DbPool,
    channel_id: i64,
    start: DateTime<Utc>,
    finish: DateTime<Utc>,
) -> Result<Vec<XmltvGuideItem>, sqlx::Error> {
    load_filtered_items(pool, Some(channel_id), start, finish).await
}

async fn load_filtered_items(
    pool: &DbPool,
    channel_id: Option<i64>,
    start: DateTime<Utc>,
    finish: DateTime<Utc>,
) -> Result<Vec<XmltvGuideItem>, sqlx::Error> {
    let rows = sqlx::query(
        r#"SELECT c."Number", c."PlayoutSource", c."PlayoutOffset",
                  p."Id", p."ScheduleFile"
           FROM "Channel" c
           JOIN "Playout" p ON p."ChannelId" = CASE
               WHEN c."PlayoutSource" = 1 THEN c."MirrorSourceChannelId"
               ELSE c."Id" END
           WHERE c."IsEnabled" = 1 AND c."ShowInEpg" = 1
             AND p."ScheduleKind" IN (20, 'ExternalJson')
             AND (? IS NULL OR c."Id" = ?)
           ORDER BY c."Number", p."Id""#,
    )
    .bind(channel_id)
    .bind(channel_id)
    .fetch_all(pool)
    .await?;
    let mut items = Vec::new();
    for row in rows {
        let playout_id: i64 = row.try_get("Id")?;
        let path: Option<String> = row.try_get("ScheduleFile")?;
        let path = path.unwrap_or_default();
        let schedule = super::snapshot::load_playout(pool, playout_id, std::path::Path::new(&path))
            .await
            .map_err(|error| {
                tracing::warn!(playout_id, %error, "Unable to read external JSON guide");
                sqlx::Error::Decode(Box::new(error))
            })?;
        let Some(schedule) = schedule else { continue };
        let offset = if row.try_get::<i32, _>("PlayoutSource")? == 1 {
            row.try_get::<Option<String>, _>("PlayoutOffset")?
                .as_deref()
                .map(try_decode_signed_duration_text)
                .transpose()
                .map_err(|error| sqlx::Error::Decode(Box::new(error)))?
                .unwrap_or_default()
        } else {
            Duration::zero()
        };
        let channel_number: String = row.try_get("Number")?;
        for (index, item) in schedule.items.into_iter().enumerate() {
            if i64::from(item.program.duration)
                < i64::from(schedule.guide_minimum_duration_seconds) * 1000
            {
                continue;
            }
            let shifted_start = shift(item.start, offset)?;
            let shifted_finish = shift(item.finish, offset)?;
            if shifted_start >= finish || shifted_finish <= start {
                continue;
            }
            let (title, content) = content(item.program);
            items.push(XmltvGuideItem {
                channel_number: channel_number.clone(),
                start: shifted_start,
                finish: shifted_finish,
                guide_start: None,
                guide_finish: None,
                guide_group: i32::try_from(index)
                    .map_err(|error| sqlx::Error::Decode(Box::new(error)))?,
                filler_kind: FillerKind::None,
                custom_title: None,
                title,
                content,
            });
        }
    }
    Ok(items)
}

fn shift(time: DateTime<Utc>, offset: Duration) -> Result<DateTime<Utc>, sqlx::Error> {
    time.checked_add_signed(offset)
        .ok_or_else(|| sqlx::Error::Decode(Box::new(super::ExternalJsonError::TimeRange)))
}

fn content(program: super::ExternalJsonProgram) -> (String, XmltvProgrammeContent) {
    let episode = program.kind.as_deref() == Some("episode");
    let title = if episode {
        program.show_title
    } else {
        program.title.clone()
    };
    let content = XmltvProgrammeContent {
        kind: if episode {
            XmltvItemKind::Episode
        } else {
            XmltvItemKind::Movie
        },
        sub_title: episode.then_some(program.title).flatten(),
        year: if episode { None } else { program.year },
        season_number: episode.then_some(program.season),
        episode_number: episode.then_some(program.episode),
        icon_artwork: program
            .icon
            .filter(|icon| !icon.trim().is_empty())
            .map(|path| ArtworkReference {
                // Upstream uses transient artwork: external HTTP URLs pass through
                // without introducing persisted media or provider credentials.
                id: 0,
                kind: if episode {
                    ArtworkKind::Thumbnail
                } else {
                    ArtworkKind::Poster
                },
                path,
            }),
        icon_size_hint: if episode {
            IconSizeHint::Thumbnail
        } else {
            IconSizeHint::Poster
        },
        ..XmltvProgrammeContent::default()
    };
    (
        title
            .filter(|title| !title.trim().is_empty())
            .unwrap_or_else(|| "Unknown".into()),
        content,
    )
}
