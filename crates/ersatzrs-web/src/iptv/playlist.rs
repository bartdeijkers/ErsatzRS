use std::fmt::Write as _;

use axum::{
    extract::State,
    http::{header, HeaderMap, HeaderValue, StatusCode},
    response::{IntoResponse, Response},
};
use ersatzrs_core::repositories::ChannelRepository;
use ersatzrs_domain::Channel;

use crate::state::AppState;

pub async fn get_channels_m3u(State(state): State<AppState>, headers: HeaderMap) -> Response {
    let channels = match state.repositories.channel.list_enabled().await {
        Ok(channels) => channels,
        Err(error) => {
            tracing::error!(%error, "failed to load channels for m3u playlist");
            return StatusCode::INTERNAL_SERVER_ERROR.into_response();
        }
    };

    let scheme = request_scheme(&headers);
    let host = request_host(&headers);
    let body = build_m3u(&scheme, &host, &channels);

    let mut response = body.into_response();
    response.headers_mut().insert(
        header::CONTENT_TYPE,
        HeaderValue::from_static("audio/x-mpegurl"),
    );
    response
}

#[must_use]
pub fn build_m3u(scheme: &str, host: &str, channels: &[Channel]) -> String {
    let mut ordered: Vec<&Channel> = channels.iter().collect();
    ordered.sort_by(|a, b| {
        parse_channel_number(&a.number).total_cmp(&parse_channel_number(&b.number))
    });

    let xmltv = format!("{scheme}://{host}/iptv/xmltv.xml");
    let mut out = String::new();
    let _ = writeln!(out, "#EXTM3U url-tvg=\"{xmltv}\" x-tvg-url=\"{xmltv}\"");

    for channel in ordered {
        let logo = format!(
            "{scheme}://{host}/iptv/logos/{}",
            urlencoding::encode(&channel.name)
        );
        let group = channel.group.as_deref().unwrap_or("");
        let name = &channel.name;
        let number = &channel.number;
        let _ = writeln!(
            out,
            "#EXTINF:-1 tvg-id=\"{number}\" tvg-name=\"{name}\" tvg-logo=\"{logo}\" group-title=\"{group}\",{name}"
        );
        let _ = writeln!(out, "{scheme}://{host}/iptv/channel/{number}.ts");
    }

    out
}

fn parse_channel_number(number: &str) -> f64 {
    number.parse::<f64>().unwrap_or(f64::MAX)
}

fn request_scheme(headers: &HeaderMap) -> String {
    headers
        .get("x-forwarded-proto")
        .and_then(|value| value.to_str().ok())
        .map_or_else(
            || "http".to_string(),
            |value| value.split(',').next().unwrap_or(value).trim().to_string(),
        )
}

fn request_host(headers: &HeaderMap) -> String {
    headers
        .get("x-forwarded-host")
        .or_else(|| headers.get(header::HOST))
        .and_then(|value| value.to_str().ok())
        .map_or_else(
            || "localhost".to_string(),
            |value| value.split(',').next().unwrap_or(value).trim().to_string(),
        )
}

#[cfg(test)]
mod tests {
    use super::*;
    use ersatzrs_domain::Channel;
    use uuid::Uuid;

    fn fixture_channel(id: i64, number: &str, name: &str, group: Option<&str>) -> Channel {
        let mut channel = Channel::new(Uuid::nil());
        channel.id = id;
        channel.number = number.to_string();
        channel.name = name.to_string();
        channel.group = group.map(ToString::to_string);
        channel
    }

    #[test]
    fn build_m3u_matches_snapshot() {
        let channels = vec![
            fixture_channel(2, "2", "Kids", Some("Family")),
            fixture_channel(1, "1", "Movies", Some("ErsatzRS")),
        ];

        let output = build_m3u("http", "example.test:8080", &channels);
        insta::assert_snapshot!(output);
    }
}
