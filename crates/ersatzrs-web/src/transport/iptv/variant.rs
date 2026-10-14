use std::fmt::Write as _;

use ersatzrs_domain::{FFmpegProfileAudioFormat, FFmpegProfileVideoFormat, StreamingEngine};

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
    engine: StreamingEngine,
    access_token: &AccessToken,
) -> String {
    let variant_url = format!(
        "{scheme}://{host}/iptv/session/{channel_number}/hls.m3u8{}",
        access_token.query()
    );
    build_variant_playlist(&variant_url, spec, engine)
}

/// Build the isolated browser-preview master playlist.
#[must_use]
pub fn build_preview_variant_playlist(
    scheme: &str,
    host: &str,
    channel_number: &str,
    spec: Option<&ChannelStreamingSpec>,
    engine: StreamingEngine,
    access_token: &AccessToken,
) -> String {
    let variant_url = format!(
        "{scheme}://{host}/iptv/preview/{channel_number}/hls.m3u8{}",
        access_token.query()
    );
    build_variant_playlist(&variant_url, spec, engine)
}

fn build_variant_playlist(
    variant_url: &str,
    spec: Option<&ChannelStreamingSpec>,
    engine: StreamingEngine,
) -> String {
    let (resolution, bitrate) = match spec {
        Some(spec) => {
            let codecs = codec_inf(spec);
            let resolution = if engine == StreamingEngine::Next
                && spec.video_format == FFmpegProfileVideoFormat::Copy
            {
                codecs
            } else {
                format!(
                    ",RESOLUTION={width}x{height}{codecs}",
                    width = spec.width,
                    height = spec.height,
                )
            };
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
    requested: bool,
) -> String {
    use ersatzrs_domain::{NextEngineTextSubtitleMode, StreamingEngine};
    if !requested
        || channel.streaming_engine != StreamingEngine::Next
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
        "#EXT-X-MEDIA:TYPE=SUBTITLES,GROUP-ID=\"subs\",NAME=\"Selected subtitles\",DEFAULT=NO,AUTOSELECT=YES,FORCED=NO,URI=\"{url}\"\n"
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
        FFmpegProfileVideoFormat::Hevc => "hvc1.1.6.L93.B0",
        FFmpegProfileVideoFormat::H264 => "avc1.4D4028",
        // AV1 level, tier and bit depth cannot be inferred from this projection.
        // Omit CODECS rather than advertising a guessed stream (ADR 0106).
        _ => "",
    };
    let audio_codec = match spec.audio_format {
        FFmpegProfileAudioFormat::Ac3 => "ac-3",
        FFmpegProfileAudioFormat::Aac | FFmpegProfileAudioFormat::AacLatm => "mp4a.40.2",
        _ => "",
    };

    if video_codec.is_empty() || audio_codec.is_empty() {
        String::new()
    } else {
        format!(",CODECS=\"{video_codec},{audio_codec}\"")
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn copied_video_omits_fixed_resolution_in_normal_and_preview_playlists() {
        let mut spec = h264_aac_1080p();
        spec.video_format = FFmpegProfileVideoFormat::Copy;
        for preview in [false, true] {
            let build = if preview {
                build_preview_variant_playlist
            } else {
                build_multi_variant_playlist
            };
            let playlist = build(
                "http",
                "example.test",
                "1",
                Some(&spec),
                StreamingEngine::Next,
                &AccessToken::default(),
            );
            assert!(!playlist.contains("RESOLUTION="));
            assert!(!playlist.contains("CODECS="));
            assert!(playlist.contains("BANDWIDTH="));
            assert!(playlist.contains(if preview {
                "/iptv/preview/1/"
            } else {
                "/iptv/session/1/"
            }));
            let native = build(
                "http",
                "example.test",
                "1",
                Some(&spec),
                StreamingEngine::Legacy,
                &AccessToken::default(),
            );
            assert!(
                native.contains("RESOLUTION=1920x1080"),
                "preserve the native playlist contract"
            );
        }
    }

    #[test]
    fn codec_attribute_requires_both_audio_and_video_identifiers() {
        let mut spec = h264_aac_1080p();
        assert_eq!(codec_inf(&spec), ",CODECS=\"avc1.4D4028,mp4a.40.2\"");
        spec.video_format = FFmpegProfileVideoFormat::Mpeg2Video;
        assert!(
            codec_inf(&spec).is_empty(),
            "MPEG-2 must not advertise audio-only CODECS"
        );
        spec.video_format = FFmpegProfileVideoFormat::H264;
        spec.audio_format = FFmpegProfileAudioFormat::Copy;
        assert!(
            codec_inf(&spec).is_empty(),
            "unknown audio must not advertise video-only CODECS"
        );
        spec.video_format = FFmpegProfileVideoFormat::Hevc;
        spec.audio_format = FFmpegProfileAudioFormat::Ac3;
        assert_eq!(codec_inf(&spec), ",CODECS=\"hvc1.1.6.L93.B0,ac-3\"");
    }

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
        let master = build_multi_variant_playlist(
            "http",
            "example.test",
            "42",
            None,
            StreamingEngine::Legacy,
            &token,
        );
        for (preview, scope) in [(false, "session"), (true, "preview")] {
            let converted = with_next_subtitles(master.clone(), &channel, preview, &token, true);
            assert!(converted.contains("#EXT-X-VERSION:6\n"));
            assert!(converted.contains("DEFAULT=NO,AUTOSELECT=YES"));
            assert_eq!(
                with_next_subtitles(master.clone(), &channel, preview, &token, false),
                master,
                "explicit rendition omission must preserve the audio/video master"
            );
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
            with_next_subtitles(master.clone(), &channel, false, &token, true),
            master
        );
        channel.next_engine_text_subtitle_mode = NextEngineTextSubtitleMode::Convert;
        channel.streaming_engine = StreamingEngine::Legacy;
        assert_eq!(
            with_next_subtitles(master.clone(), &channel, false, &token, true),
            master
        );
    }

    #[test]
    fn av1_variant_omits_unproven_level_and_bit_depth() {
        let mut spec = h264_aac_1080p();
        spec.video_format = FFmpegProfileVideoFormat::Av1;
        let playlist = build_multi_variant_playlist(
            "http",
            "example.test",
            "1",
            Some(&spec),
            StreamingEngine::Legacy,
            &AccessToken::default(),
        );
        assert!(!playlist.contains("CODECS="));
        assert!(playlist.contains("RESOLUTION=1920x1080"));
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
            StreamingEngine::Legacy,
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
            StreamingEngine::Legacy,
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
            StreamingEngine::Legacy,
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
            StreamingEngine::Legacy,
            &AccessToken::new(Some("preview-token")),
        );
        assert!(output.contains("CODECS=\"avc1.4D4028,mp4a.40.2\""));
        assert!(output
            .contains("https://example.test/iptv/preview/42/hls.m3u8?access_token=preview-token"));
        assert!(!output.contains("/iptv/session/"));
    }
}
