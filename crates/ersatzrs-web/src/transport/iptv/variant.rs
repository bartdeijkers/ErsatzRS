use std::fmt::Write as _;

use ersatzrs_domain::{FFmpegProfileAudioFormat, FFmpegProfileVideoFormat};

use crate::transport::iptv::access_token::AccessToken;
use crate::transport::iptv::streaming_specs::ChannelStreamingSpec;

/// Builds the HLS multi-variant playlist for a single channel.
///
/// Ports `IptvController.GetMultiVariantPlaylist` from `ErsatzTV`:
/// single-variant, single-line `#EXT-X-STREAM-INF` pointing at the per-session
/// variant playlist at `/iptv/session/{n}/hls.m3u8`.
#[must_use]
pub fn build_multi_variant_playlist(
    scheme: &str,
    host: &str,
    channel_number: &str,
    spec: Option<&ChannelStreamingSpec>,
    access_token: &AccessToken,
) -> String {
    let variant_url = format!(
        "{scheme}://{host}/iptv/session/{channel_number}/hls.m3u8{}",
        access_token.query()
    );
    build_variant_playlist(&variant_url, spec)
}

/// Build the isolated browser-preview master playlist.
#[must_use]
pub fn build_preview_variant_playlist(
    scheme: &str,
    host: &str,
    channel_number: &str,
    spec: Option<&ChannelStreamingSpec>,
    access_token: &AccessToken,
) -> String {
    let variant_url = format!(
        "{scheme}://{host}/iptv/preview/{channel_number}/hls.m3u8{}",
        access_token.query()
    );
    build_variant_playlist(&variant_url, spec)
}

fn build_variant_playlist(variant_url: &str, spec: Option<&ChannelStreamingSpec>) -> String {
    let (resolution, bitrate) = match spec {
        Some(spec) => {
            let codecs = codec_inf(spec);
            let resolution = format!(
                ",RESOLUTION={width}x{height}{codecs}",
                width = spec.width,
                height = spec.height,
            );
            (resolution, effective_bandwidth(spec))
        }
        None => (String::new(), 10_000_000),
    };

    let mut out = String::new();
    out.push_str("#EXTM3U\n");
    out.push_str("#EXT-X-VERSION:3\n");
    let _ = writeln!(out, "#EXT-X-STREAM-INF:BANDWIDTH={bitrate}{resolution}");
    out.push_str(variant_url);
    out.push('\n');
    out
}

/// Add Next's selected-text rendition to an existing channel or preview master.
/// A linear channel can change subtitle language at every programme boundary;
/// do not label that changing rendition with a guessed language.
pub(crate) fn with_next_subtitles(
    playlist: String,
    channel: &ersatzrs_domain::Channel,
    preview: bool,
    access_token: &AccessToken,
) -> String {
    use ersatzrs_domain::{NextEngineTextSubtitleMode, StreamingEngine};
    if channel.streaming_engine != StreamingEngine::Next
        || channel.next_engine_text_subtitle_mode != NextEngineTextSubtitleMode::Convert
    {
        return playlist;
    }
    let scope = if preview { "preview" } else { "session" };
    let url = format!(
        "/iptv/{scope}/{}/live_sub.m3u8{}",
        channel.number,
        access_token.query()
    );
    let media = format!(
        "#EXT-X-MEDIA:TYPE=SUBTITLES,GROUP-ID=\"subs\",NAME=\"Selected subtitles\",DEFAULT=YES,AUTOSELECT=YES,FORCED=NO,URI=\"{url}\"\n"
    );
    playlist
        .replace("#EXT-X-VERSION:3\n", &format!("#EXT-X-VERSION:6\n{media}"))
        .replace(
            "#EXT-X-STREAM-INF:",
            "#EXT-X-STREAM-INF:SUBTITLES=\"subs\",",
        )
}

/// Ports `Mapper.ProjectToSpecsViewModel`:
/// `(VideoBitrate * 1000 + AudioBitrate * 1000) * 1.2`.
///
/// Computed as `total_bps * 12 / 10` so the result is exact integer arithmetic,
/// which also matches the C# `(int)(... * 1.2)` truncation towards zero for
/// any value where `total_bps * 12` does not overflow `i64`.
fn effective_bandwidth(spec: &ChannelStreamingSpec) -> i64 {
    let total_kbps = i64::from(spec.video_bitrate) + i64::from(spec.audio_bitrate);
    let bps = total_kbps.saturating_mul(1_000);
    bps.saturating_mul(12) / 10
}

fn codec_inf(spec: &ChannelStreamingSpec) -> String {
    let video_codec = match spec.video_format {
        FFmpegProfileVideoFormat::Av1 => "av01.0.01M.08",
        FFmpegProfileVideoFormat::Hevc => "hvc1.1.6.L93.B0",
        FFmpegProfileVideoFormat::H264 => "avc1.4D4028",
        _ => "",
    };
    let audio_codec = match spec.audio_format {
        FFmpegProfileAudioFormat::Ac3 => "ac-3",
        FFmpegProfileAudioFormat::Aac | FFmpegProfileAudioFormat::AacLatm => "mp4a.40.2",
        _ => "",
    };

    let mut codecs = Vec::new();
    if !video_codec.is_empty() {
        codecs.push(video_codec);
    }
    if !audio_codec.is_empty() {
        codecs.push(audio_codec);
    }

    if codecs.is_empty() {
        String::new()
    } else {
        format!(",CODECS=\"{}\"", codecs.join(","))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn next_conversion_links_selected_rendition_for_channel_and_preview() {
        use ersatzrs_domain::{Channel, NextEngineTextSubtitleMode, StreamingEngine};
        let mut channel = Channel {
            number: "42".into(),
            streaming_engine: StreamingEngine::Next,
            next_engine_text_subtitle_mode: NextEngineTextSubtitleMode::Convert,
            preferred_subtitle_language_code: Some("nld".into()),
            ..Default::default()
        };
        let token = AccessToken::new(Some("synthetic-token"));
        let master = build_multi_variant_playlist("http", "example.test", "42", None, &token);
        for (preview, scope) in [(false, "session"), (true, "preview")] {
            let converted = with_next_subtitles(master.clone(), &channel, preview, &token);
            assert!(converted.contains("#EXT-X-VERSION:6\n"));
            assert!(converted.contains("TYPE=SUBTITLES,GROUP-ID=\"subs\""));
            assert!(converted.contains("#EXT-X-STREAM-INF:SUBTITLES=\"subs\","));
            assert!(converted.contains(&format!(
                "/iptv/{scope}/42/live_sub.m3u8?access_token=synthetic-token"
            )));
            // Item language overrides can differ from channel preferences.
            assert!(!converted.contains("LANGUAGE="));
            assert!(!converted.contains("English"));
        }
        channel.next_engine_text_subtitle_mode = NextEngineTextSubtitleMode::Burn;
        assert_eq!(
            with_next_subtitles(master.clone(), &channel, false, &token),
            master
        );
        channel.next_engine_text_subtitle_mode = NextEngineTextSubtitleMode::Convert;
        channel.streaming_engine = StreamingEngine::Legacy;
        assert_eq!(
            with_next_subtitles(master.clone(), &channel, false, &token),
            master
        );
    }

    fn h264_aac_1080p() -> ChannelStreamingSpec {
        ChannelStreamingSpec {
            video_format: FFmpegProfileVideoFormat::H264,
            audio_format: FFmpegProfileAudioFormat::Aac,
            width: 1920,
            height: 1080,
            video_bitrate: 10_000,
            audio_bitrate: 192,
        }
    }

    #[test]
    fn build_multi_variant_playlist_h264_aac_1080p() {
        let output = build_multi_variant_playlist(
            "http",
            "example.test:8080",
            "1",
            Some(&h264_aac_1080p()),
            &AccessToken::default(),
        );
        insta::assert_snapshot!("variant_h264_aac_1080p", output);
    }

    #[test]
    fn build_multi_variant_playlist_hevc_ac3_720p_with_token() {
        let spec = ChannelStreamingSpec {
            video_format: FFmpegProfileVideoFormat::Hevc,
            audio_format: FFmpegProfileAudioFormat::Ac3,
            width: 1280,
            height: 720,
            video_bitrate: 5_000,
            audio_bitrate: 384,
        };
        let output = build_multi_variant_playlist(
            "https",
            "example.test",
            "42",
            Some(&spec),
            &AccessToken::new(Some("abc123")),
        );
        insta::assert_snapshot!("variant_hevc_ac3_720p_token", output);
    }

    #[test]
    fn build_multi_variant_playlist_without_spec_falls_back() {
        let output = build_multi_variant_playlist(
            "http",
            "example.test:8080",
            "99",
            None,
            &AccessToken::default(),
        );
        insta::assert_snapshot!("variant_no_spec", output);
    }

    #[test]
    fn browser_preview_playlist_uses_isolated_route_and_h264_aac_codecs() {
        let output = build_preview_variant_playlist(
            "https",
            "example.test",
            "42",
            Some(&h264_aac_1080p()),
            &AccessToken::new(Some("preview-token")),
        );
        assert!(output.contains("CODECS=\"avc1.4D4028,mp4a.40.2\""));
        assert!(output
            .contains("https://example.test/iptv/preview/42/hls.m3u8?access_token=preview-token"));
        assert!(!output.contains("/iptv/session/"));
    }
}
