use axum::{
    extract::State,
    http::{header, HeaderValue, StatusCode},
    response::{IntoResponse, Response},
};
use chrono::{DateTime, Duration, Utc};
use ersatzrs_core::repositories::ChannelRepository;
use ersatzrs_domain::Channel;
use ersatzrs_infra::db::DbPool;
use quick_xml::events::{BytesDecl, BytesEnd, BytesStart, BytesText, Event};
use quick_xml::writer::Writer;
use sqlx::Row;
use std::io::Cursor;

use crate::state::AppState;

const UNKNOWN_TITLE: &str = "Unknown";
const GUIDE_WINDOW_HOURS: i64 = 48;

pub async fn get_xmltv_xml(State(state): State<AppState>) -> Response {
    let channels = match state.repositories.channel.list_enabled().await {
        Ok(channels) => channels,
        Err(error) => {
            tracing::error!(%error, "failed to load channels for xmltv guide");
            return StatusCode::INTERNAL_SERVER_ERROR.into_response();
        }
    };

    let now = Utc::now();
    let horizon = now + Duration::hours(GUIDE_WINDOW_HOURS);
    let programmes = match load_programmes(&state.pool, now, horizon).await {
        Ok(programmes) => programmes,
        Err(error) => {
            tracing::error!(%error, "failed to load playout items for xmltv guide");
            return StatusCode::INTERNAL_SERVER_ERROR.into_response();
        }
    };

    match build_xmltv(&channels, &programmes) {
        Ok(body) => {
            let mut response = body.into_response();
            response.headers_mut().insert(
                header::CONTENT_TYPE,
                HeaderValue::from_static("text/xml; charset=utf-8"),
            );
            response
        }
        Err(error) => {
            tracing::error!(%error, "failed to render xmltv xml");
            StatusCode::INTERNAL_SERVER_ERROR.into_response()
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct GuideProgramme {
    pub channel_number: String,
    pub start: DateTime<Utc>,
    pub stop: DateTime<Utc>,
    pub title: String,
}

pub fn build_xmltv(
    channels: &[Channel],
    programmes: &[GuideProgramme],
) -> Result<String, quick_xml::Error> {
    let mut ordered_channels: Vec<&Channel> = channels.iter().filter(|c| c.show_in_epg).collect();
    ordered_channels.sort_by(|a, b| {
        parse_channel_number(&a.number).total_cmp(&parse_channel_number(&b.number))
    });

    let mut writer = Writer::new(Cursor::new(Vec::<u8>::new()));
    writer.write_event(Event::Decl(BytesDecl::new("1.0", Some("utf-8"), None)))?;

    let mut tv_start = BytesStart::new("tv");
    tv_start.push_attribute(("generator-info-name", "ersatzrs"));
    writer.write_event(Event::Start(tv_start))?;

    for channel in &ordered_channels {
        let mut channel_start = BytesStart::new("channel");
        channel_start.push_attribute(("id", channel.number.as_str()));
        writer.write_event(Event::Start(channel_start))?;
        writer.write_event(Event::Start(BytesStart::new("display-name")))?;
        writer.write_event(Event::Text(BytesText::new(&channel.name)))?;
        writer.write_event(Event::End(BytesEnd::new("display-name")))?;
        writer.write_event(Event::End(BytesEnd::new("channel")))?;
    }

    for programme in programmes {
        let mut programme_start = BytesStart::new("programme");
        programme_start.push_attribute(("start", format_xmltv_time(programme.start).as_str()));
        programme_start.push_attribute(("stop", format_xmltv_time(programme.stop).as_str()));
        programme_start.push_attribute(("channel", programme.channel_number.as_str()));
        writer.write_event(Event::Start(programme_start))?;
        writer.write_event(Event::Start(BytesStart::new("title")))?;
        writer.write_event(Event::Text(BytesText::new(&programme.title)))?;
        writer.write_event(Event::End(BytesEnd::new("title")))?;
        writer.write_event(Event::End(BytesEnd::new("programme")))?;
    }

    writer.write_event(Event::End(BytesEnd::new("tv")))?;

    String::from_utf8(writer.into_inner().into_inner())
        .map_err(|error| quick_xml::Error::Io(std::sync::Arc::new(std::io::Error::other(error))))
}

fn parse_channel_number(number: &str) -> f64 {
    number.parse::<f64>().unwrap_or(f64::MAX)
}

fn format_xmltv_time(value: DateTime<Utc>) -> String {
    value.format("%Y%m%d%H%M%S +0000").to_string()
}

async fn load_programmes(
    pool: &DbPool,
    start: DateTime<Utc>,
    finish: DateTime<Utc>,
) -> Result<Vec<GuideProgramme>, sqlx::Error> {
    let rows = sqlx::query(
        r#"SELECT
               c."Number" AS "Number",
               pi."Start" AS "Start",
               pi."Finish" AS "Finish",
               pi."CustomTitle" AS "CustomTitle",
               COALESCE(
                   (SELECT "Title" FROM "MovieMetadata" WHERE "MovieId" = pi."MediaItemId" LIMIT 1),
                   (SELECT "Title" FROM "EpisodeMetadata" WHERE "EpisodeId" = pi."MediaItemId" LIMIT 1),
                   (SELECT "Title" FROM "MusicVideoMetadata" WHERE "MusicVideoId" = pi."MediaItemId" LIMIT 1),
                   (SELECT "Title" FROM "SongMetadata" WHERE "SongId" = pi."MediaItemId" LIMIT 1),
                   (SELECT "Title" FROM "OtherVideoMetadata" WHERE "OtherVideoId" = pi."MediaItemId" LIMIT 1),
                   (SELECT "Title" FROM "RemoteStreamMetadata" WHERE "RemoteStreamId" = pi."MediaItemId" LIMIT 1)
               ) AS "Title"
           FROM "PlayoutItem" pi
           JOIN "Playout" p ON p."Id" = pi."PlayoutId"
           JOIN "Channel" c ON c."Id" = p."ChannelId"
           WHERE c."IsEnabled" = 1
             AND c."ShowInEpg" = 1
             AND pi."Start" < ?
             AND pi."Finish" > ?
           ORDER BY c."Number", pi."Start", pi."Id""#,
    )
    .bind(encode_datetime(finish))
    .bind(encode_datetime(start))
    .fetch_all(pool)
    .await?;

    let mut programmes = Vec::with_capacity(rows.len());
    for row in rows {
        let channel_number: String = row.try_get("Number")?;
        let start_text: String = row.try_get("Start")?;
        let finish_text: String = row.try_get("Finish")?;
        let custom_title: Option<String> = row.try_get("CustomTitle")?;
        let metadata_title: Option<String> = row.try_get("Title")?;

        let start = parse_sqlite_datetime(&start_text).ok_or_else(|| {
            sqlx::Error::Decode(format!("invalid PlayoutItem Start: {start_text}").into())
        })?;
        let stop = parse_sqlite_datetime(&finish_text).ok_or_else(|| {
            sqlx::Error::Decode(format!("invalid PlayoutItem Finish: {finish_text}").into())
        })?;
        let title = custom_title
            .and_then(|t| (!t.trim().is_empty()).then_some(t))
            .or(metadata_title)
            .filter(|t| !t.trim().is_empty())
            .unwrap_or_else(|| UNKNOWN_TITLE.to_string());

        programmes.push(GuideProgramme {
            channel_number,
            start,
            stop,
            title,
        });
    }

    Ok(programmes)
}

fn encode_datetime(value: DateTime<Utc>) -> String {
    value.format("%Y-%m-%d %H:%M:%S").to_string()
}

fn parse_sqlite_datetime(text: &str) -> Option<DateTime<Utc>> {
    chrono::NaiveDateTime::parse_from_str(text, "%Y-%m-%d %H:%M:%S")
        .or_else(|_| chrono::NaiveDateTime::parse_from_str(text, "%Y-%m-%dT%H:%M:%S"))
        .or_else(|_| chrono::NaiveDateTime::parse_from_str(text, "%Y-%m-%d %H:%M:%S%.f"))
        .ok()
        .map(|ndt| DateTime::<Utc>::from_naive_utc_and_offset(ndt, Utc))
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::TimeZone;
    use ersatzrs_domain::Channel;
    use uuid::Uuid;

    fn fixture_channel(number: &str, name: &str) -> Channel {
        let mut channel = Channel::new(Uuid::nil());
        channel.number = number.to_string();
        channel.name = name.to_string();
        channel
    }

    #[test]
    fn build_xmltv_matches_snapshot() {
        let channels = vec![fixture_channel("2", "Kids"), fixture_channel("1", "Movies")];
        let programmes = vec![
            GuideProgramme {
                channel_number: "1".into(),
                start: Utc.with_ymd_and_hms(2026, 4, 17, 20, 0, 0).unwrap(),
                stop: Utc.with_ymd_and_hms(2026, 4, 17, 21, 0, 0).unwrap(),
                title: "Big Buck Bunny".into(),
            },
            GuideProgramme {
                channel_number: "1".into(),
                start: Utc.with_ymd_and_hms(2026, 4, 17, 21, 0, 0).unwrap(),
                stop: Utc.with_ymd_and_hms(2026, 4, 17, 22, 0, 0).unwrap(),
                title: "Sintel".into(),
            },
            GuideProgramme {
                channel_number: "2".into(),
                start: Utc.with_ymd_and_hms(2026, 4, 17, 20, 0, 0).unwrap(),
                stop: Utc.with_ymd_and_hms(2026, 4, 17, 20, 30, 0).unwrap(),
                title: "Pingu".into(),
            },
            GuideProgramme {
                channel_number: "2".into(),
                start: Utc.with_ymd_and_hms(2026, 4, 17, 20, 30, 0).unwrap(),
                stop: Utc.with_ymd_and_hms(2026, 4, 17, 21, 0, 0).unwrap(),
                title: "Unknown".into(),
            },
        ];

        let output = build_xmltv(&channels, &programmes).expect("xmltv render");
        insta::assert_snapshot!(output);
    }
}
