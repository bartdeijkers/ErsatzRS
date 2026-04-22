use std::collections::HashMap;
use std::fmt::Write as _;

use axum::{
    extract::{Query, State},
    http::{header, HeaderMap, HeaderValue, StatusCode},
    response::{IntoResponse, Response},
};
use base64::engine::general_purpose::STANDARD as BASE64;
use base64::Engine;
use ersatzrs_core::repositories::ChannelRepository;
use ersatzrs_domain::{Channel, FFmpegProfileAudioFormat, FFmpegProfileVideoFormat, StreamingMode};
use serde::Deserialize;

use crate::host::state::AppState;
use crate::transport::iptv::access_token::AccessToken;
use crate::transport::iptv::streaming_specs::{self, ChannelStreamingSpec};

#[derive(Debug, Deserialize, Default)]
pub struct PlaylistQuery {
    #[serde(default)]
    pub access_token: Option<String>,
}

pub async fn get_channels_m3u(
    State(state): State<AppState>,
    headers: HeaderMap,
    Query(query): Query<PlaylistQuery>,
) -> Response {
    let channels = match state.repositories.channel.list_enabled().await {
        Ok(channels) => channels,
        Err(error) => {
            tracing::error!(%error, "failed to load channels for m3u playlist");
            return StatusCode::INTERNAL_SERVER_ERROR.into_response();
        }
    };

    let specs = match streaming_specs::load_all_specs(&state.pool).await {
        Ok(specs) => specs,
        Err(error) => {
            tracing::warn!(%error, "failed to load channel streaming specs; continuing without codec tags");
            HashMap::new()
        }
    };

    let scheme = request_scheme(&headers);
    let host = request_host(&headers);
    let user_agent = headers
        .get(header::USER_AGENT)
        .and_then(|value| value.to_str().ok())
        .unwrap_or("");
    let access_token = AccessToken::new(query.access_token);

    let body = build_m3u(&scheme, &host, user_agent, &access_token, &channels, &specs);

    let mut response = body.into_response();
    response.headers_mut().insert(
        header::CONTENT_TYPE,
        HeaderValue::from_static("audio/x-mpegurl"),
    );
    response
}

#[must_use]
pub fn build_m3u(
    scheme: &str,
    host: &str,
    user_agent: &str,
    access_token: &AccessToken,
    channels: &[Channel],
    specs: &HashMap<i64, ChannelStreamingSpec>,
) -> String {
    let mut ordered: Vec<&Channel> = channels.iter().collect();
    ordered.sort_by(|a, b| {
        parse_channel_number(&a.number).total_cmp(&parse_channel_number(&b.number))
    });

    let token_uri = access_token.query();
    let token_uri_amp = access_token.query_amp();
    let xmltv = format!("{scheme}://{host}/iptv/xmltv.xml{token_uri}");

    let mut out = String::new();
    let _ = writeln!(out, "#EXTM3U url-tvg=\"{xmltv}\" x-tvg-url=\"{xmltv}\"");

    let is_kodi = user_agent
        .get(..4)
        .is_some_and(|prefix| prefix.eq_ignore_ascii_case("kodi"));

    for channel in ordered {
        if is_kodi {
            out.push_str("#KODIPROP:inputstream=inputstream.ffmpegdirect\n");
            let mime = match channel.streaming_mode {
                StreamingMode::TransportStream | StreamingMode::TransportStreamHybrid => {
                    "video/mp2t"
                }
                _ => "application/x-mpegURL",
            };
            let _ = writeln!(out, "#KODIPROP:mimetype={mime}");
            out.push_str("#KODIPROP:inputstream.ffmpegdirect.open_mode=ffmpeg\n");
        }

        let logo = format!(
            "{scheme}://{host}/iptv/logos/gen?text={}{token_uri_amp}",
            channel.web_encoded_name()
        );
        let short_id = short_unique_id(channel.unique_id);
        let tvg_id = channel_identifier(&channel.number);
        let format_suffix = match channel.streaming_mode {
            StreamingMode::HttpLiveStreamingDirect => {
                format!("m3u8?mode=hls-direct{token_uri_amp}")
            }
            StreamingMode::HttpLiveStreamingSegmenter => {
                format!("m3u8?mode=segmenter{token_uri_amp}")
            }
            StreamingMode::TransportStreamHybrid => format!("ts{token_uri}"),
            StreamingMode::TransportStream => format!("ts?mode=ts-legacy{token_uri_amp}"),
        };

        let spec = specs.get(&channel.id);
        let vcodec = spec
            .map(|s| video_format_name(s.video_format))
            .unwrap_or_default();
        let acodec = spec
            .map(|s| audio_format_name(s.audio_format))
            .unwrap_or_default();
        let group = channel.group.as_deref().unwrap_or("");
        let name = &channel.name;
        let number = &channel.number;

        let _ = writeln!(
            out,
            "#EXTINF:0 tvg-id=\"{tvg_id}\" channel-id=\"{short_id}\" channel-number=\"{number}\" CUID=\"{short_id}\" tvg-chno=\"{number}\" tvg-name=\"{name}\" tvg-logo=\"{logo}\" group-title=\"{group}\" tvc-stream-vcodec=\"{vcodec}\" tvc-stream-acodec=\"{acodec}\", {name}"
        );
        let _ = writeln!(
            out,
            "{scheme}://{host}/iptv/channel/{number}.{format_suffix}"
        );
    }

    out
}

fn parse_channel_number(number: &str) -> f64 {
    number.parse::<f64>().unwrap_or(f64::MAX)
}

/// URL-safe short identifier for a channel.
///
/// Ports the `shortUniqueId` snippet from `ErsatzTV`'s `ChannelPlaylist.cs`:
/// base64 of the UUID bytes, stripped `=` padding, `/` → `_`, `+` → `-`.
fn short_unique_id(unique_id: uuid::Uuid) -> String {
    let encoded = BASE64.encode(unique_id.as_bytes());
    encoded
        .trim_end_matches('=')
        .replace('/', "_")
        .replace('+', "-")
}

/// Ports `ErsatzTV.Core.Iptv.ChannelIdentifier.FromNumber`.
///
/// The `ErsatzTV` algorithm is `(int)(decimal.Parse(channelNumber) * 100)`,
/// i.e. channel numbers with at most two decimals (per
/// `Channel.NUMBER_PATTERN`). We mirror the cast by scaling to cents and
/// rounding, then saturating into `i32` so malformed input can never alias a
/// valid identifier.
fn channel_identifier(channel_number: &str) -> String {
    let scaled = (channel_number.parse::<f64>().unwrap_or(0.0) * 100.0).round();
    let mut number: i32 = if scaled.is_finite() {
        #[allow(clippy::cast_possible_truncation)] // clamped to i32 range above
        let truncated = scaled.clamp(f64::from(i32::MIN), f64::from(i32::MAX)) as i32;
        truncated
    } else {
        0
    };
    let mut id: i32 = 0;
    while number != 0 {
        id += (number % 10) + 48;
        number /= 10;
    }

    let instance_id = instance_id();
    if instance_id.is_empty() {
        format!("C{channel_number}.{id}.ersatztv.org")
    } else {
        format!("C{channel_number}.{id}.{instance_id}.ersatztv.org")
    }
}

fn instance_id() -> String {
    #[allow(clippy::disallowed_methods)]
    std::env::var("ETV_INSTANCE_ID").unwrap_or_default()
}

fn video_format_name(format: FFmpegProfileVideoFormat) -> &'static str {
    match format {
        FFmpegProfileVideoFormat::None => "none",
        FFmpegProfileVideoFormat::H264 => "h264",
        FFmpegProfileVideoFormat::Hevc => "hevc",
        FFmpegProfileVideoFormat::Mpeg2Video => "mpeg2video",
        FFmpegProfileVideoFormat::Av1 => "av1",
        FFmpegProfileVideoFormat::Copy => "copy",
    }
}

fn audio_format_name(format: FFmpegProfileAudioFormat) -> &'static str {
    match format {
        FFmpegProfileAudioFormat::None => "none",
        FFmpegProfileAudioFormat::Aac => "aac",
        FFmpegProfileAudioFormat::Ac3 => "ac3",
        FFmpegProfileAudioFormat::AacLatm => "aaclatm",
        FFmpegProfileAudioFormat::Copy => "copy",
    }
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

    fn fixture_channel(
        id: i64,
        number: &str,
        name: &str,
        group: Option<&str>,
        streaming_mode: StreamingMode,
        unique_id: Uuid,
    ) -> Channel {
        let mut channel = Channel::new(unique_id);
        channel.id = id;
        channel.number = number.to_string();
        channel.name = name.to_string();
        channel.group = group.map(ToString::to_string);
        channel.streaming_mode = streaming_mode;
        channel
    }

    fn fixture_specs() -> HashMap<i64, ChannelStreamingSpec> {
        let mut specs = HashMap::new();
        specs.insert(
            1,
            ChannelStreamingSpec {
                video_format: FFmpegProfileVideoFormat::H264,
                audio_format: FFmpegProfileAudioFormat::Aac,
                width: 1920,
                height: 1080,
                video_bitrate: 10_000,
                audio_bitrate: 192,
            },
        );
        specs.insert(
            2,
            ChannelStreamingSpec {
                video_format: FFmpegProfileVideoFormat::Hevc,
                audio_format: FFmpegProfileAudioFormat::Ac3,
                width: 1280,
                height: 720,
                video_bitrate: 5_000,
                audio_bitrate: 384,
            },
        );
        specs.insert(
            3,
            ChannelStreamingSpec {
                video_format: FFmpegProfileVideoFormat::H264,
                audio_format: FFmpegProfileAudioFormat::Aac,
                width: 1920,
                height: 1080,
                video_bitrate: 10_000,
                audio_bitrate: 192,
            },
        );
        specs.insert(
            4,
            ChannelStreamingSpec {
                video_format: FFmpegProfileVideoFormat::H264,
                audio_format: FFmpegProfileAudioFormat::Aac,
                width: 1920,
                height: 1080,
                video_bitrate: 10_000,
                audio_bitrate: 192,
            },
        );
        specs
    }

    #[test]
    fn build_m3u_sorts_by_number_and_emits_mode_aware_urls() {
        let channels = vec![
            fixture_channel(
                2,
                "2",
                "Kids",
                Some("Family"),
                StreamingMode::HttpLiveStreamingSegmenter,
                Uuid::from_u128(0x0102_0304_0506_0708_090A_0B0C_0D0E_0F10),
            ),
            fixture_channel(
                1,
                "1",
                "Movies",
                Some("ErsatzRS"),
                StreamingMode::TransportStream,
                Uuid::from_u128(0x0000_0000_0000_0000_0000_0000_0000_0001),
            ),
            fixture_channel(
                3,
                "3",
                "Direct",
                None,
                StreamingMode::HttpLiveStreamingDirect,
                Uuid::from_u128(0x0000_0000_0000_0000_0000_0000_0000_0002),
            ),
            fixture_channel(
                4,
                "4",
                "Hybrid",
                None,
                StreamingMode::TransportStreamHybrid,
                Uuid::from_u128(0x0000_0000_0000_0000_0000_0000_0000_0003),
            ),
        ];
        let specs = fixture_specs();

        let output = build_m3u(
            "http",
            "example.test:8080",
            "",
            &AccessToken::default(),
            &channels,
            &specs,
        );
        insta::assert_snapshot!("m3u_mode_aware", output);
    }

    #[test]
    fn build_m3u_emits_kodi_headers_for_kodi_user_agent() {
        let channels = vec![fixture_channel(
            1,
            "1",
            "Movies",
            Some("ErsatzRS"),
            StreamingMode::HttpLiveStreamingSegmenter,
            Uuid::from_u128(0x0000_0000_0000_0000_0000_0000_0000_0001),
        )];
        let specs = fixture_specs();

        let output = build_m3u(
            "http",
            "example.test:8080",
            "Kodi/20.2 (Linux 6.6; x86_64)",
            &AccessToken::default(),
            &channels,
            &specs,
        );
        insta::assert_snapshot!("m3u_kodi_headers", output);
    }

    #[test]
    fn build_m3u_propagates_access_token() {
        let channels = vec![fixture_channel(
            1,
            "1",
            "Movies",
            None,
            StreamingMode::HttpLiveStreamingSegmenter,
            Uuid::from_u128(0x0000_0000_0000_0000_0000_0000_0000_0001),
        )];
        let specs = fixture_specs();

        let output = build_m3u(
            "http",
            "example.test:8080",
            "",
            &AccessToken::new(Some("secret")),
            &channels,
            &specs,
        );
        insta::assert_snapshot!("m3u_access_token", output);
    }

    #[test]
    fn channel_identifier_matches_ersatztv_algorithm() {
        // ErsatzTV: number * 100, sum digits + 48
        // "1"   -> 100 -> (1+48)+(0+48)+(0+48) = 145
        // "1.5" -> 150 -> (1+48)+(5+48)+(0+48) = 150
        // "2"   -> 200 -> (2+48)+(0+48)+(0+48) = 146
        assert_eq!(channel_identifier("1"), "C1.145.ersatztv.org");
        assert_eq!(channel_identifier("1.5"), "C1.5.150.ersatztv.org");
        assert_eq!(channel_identifier("2"), "C2.146.ersatztv.org");
    }

    #[test]
    fn short_unique_id_matches_ersatztv_encoding() {
        // ErsatzTV uses the .NET `Guid.ToByteArray()` mixed-endian layout
        // (first three fields little-endian). We feed the big-endian UUID
        // bytes; the exact string is not required to match ErsatzTV byte-
        // for-byte because ErsatzRS writes the same value on both sides of
        // the wire (M3U channel-id / CUID only). We just verify the set of
        // URL-safe characters and no trailing padding.
        let encoded = short_unique_id(Uuid::from_u128(0x0102_0304_0506_0708_090A_0B0C_0D0E_0F10));
        assert!(!encoded.contains('='));
        assert!(!encoded.contains('+'));
        assert!(!encoded.contains('/'));
    }
}
