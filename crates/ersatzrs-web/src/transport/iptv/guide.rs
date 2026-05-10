use axum::{
    extract::State,
    http::{header, HeaderValue, StatusCode},
    response::{IntoResponse, Response},
};
use chrono::{DateTime, Duration, Utc};
use ersatzrs_core::repositories::ChannelRepository;
use ersatzrs_core::xmltv::XmltvProgramme;
use ersatzrs_domain::Channel;
use quick_xml::events::{BytesDecl, BytesEnd, BytesStart, BytesText, Event};
use quick_xml::writer::Writer;
use std::io::Cursor;

use crate::host::state::AppState;

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
    let programmes =
        match ersatzrs_infra::xmltv::load_xmltv_programmes(&state.pool, now, horizon).await {
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

pub fn build_xmltv(
    channels: &[Channel],
    programmes: &[XmltvProgramme],
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
            XmltvProgramme {
                channel_number: "1".into(),
                start: Utc.with_ymd_and_hms(2026, 4, 17, 20, 0, 0).unwrap(),
                stop: Utc.with_ymd_and_hms(2026, 4, 17, 21, 0, 0).unwrap(),
                title: "Big Buck Bunny".into(),
            },
            XmltvProgramme {
                channel_number: "1".into(),
                start: Utc.with_ymd_and_hms(2026, 4, 17, 21, 0, 0).unwrap(),
                stop: Utc.with_ymd_and_hms(2026, 4, 17, 22, 0, 0).unwrap(),
                title: "Sintel".into(),
            },
            XmltvProgramme {
                channel_number: "2".into(),
                start: Utc.with_ymd_and_hms(2026, 4, 17, 20, 0, 0).unwrap(),
                stop: Utc.with_ymd_and_hms(2026, 4, 17, 20, 30, 0).unwrap(),
                title: "Pingu".into(),
            },
            XmltvProgramme {
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
