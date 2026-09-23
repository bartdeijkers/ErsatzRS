use super::*;
use serde_json::json;

#[test]
fn wire_playout_retains_timing_seek_and_explicit_track_indices() {
    let wire = json!({
        "version": PLAYOUT_VERSION,
        "items": [{
            "id": "42", "start": "2026-09-22T20:00:00Z",
            "finish": "2026-09-22T20:10:00Z",
            "source": {
                "source_type": "local", "path": "media/Épisode 1.mkv",
                "in_point_ms": 1500, "out_point_ms": 601_500
            },
            "tracks": {
                "video": {"stream_index": 0},
                "audio": {"stream_index": 3},
                "subtitle": {
                    "source": {"source_type": "local", "path": "subtitles/episode.nl.srt"},
                    "stream_index": 0
                }
            }
        }]
    });
    let parsed: Playout = serde_json::from_value(wire.clone()).unwrap();
    assert_eq!(serde_json::to_value(parsed).unwrap(), wire);
}

#[test]
fn source_contract_keeps_http_options_and_rejects_unknown_source_kinds() {
    let wire = json!({
        "source_type": "http", "uri": "https://example.invalid/episode.mkv",
        "headers": ["Accept: video/*"], "user_agent": "fixture-player",
        "reconnect": true, "reconnect_delay_max": 5, "timeout_us": 1_000_000,
        "in_point_ms": 250, "out_point_ms": 2000
    });
    let source: Source = serde_json::from_value(wire.clone()).unwrap();
    assert_eq!(serde_json::to_value(source).unwrap(), wire);
    for keep_alive in [false, true] {
        let mut explicit = wire.clone();
        explicit["keep_alive"] = json!(keep_alive);
        let source: Source = serde_json::from_value(explicit.clone()).unwrap();
        assert_eq!(serde_json::to_value(source).unwrap(), explicit);
    }
    assert!(serde_json::from_value::<Source>(json!({
        "source_type": "unknown", "path": "media/episode.mkv"
    }))
    .is_err());
    let lavfi: Source = serde_json::from_value(json!({
        "source_type": "lavfi", "params": "anullsrc=channel_layout=stereo"
    }))
    .unwrap();
    assert!(matches!(lavfi, Source::Lavfi { .. }));
}

#[test]
fn config_uses_upstream_lowercase_codecs_and_optional_acceleration() {
    let wire = json!({
        "playout": {"folder": "playouts/1/current"},
        "ffmpeg": {"ffmpeg_path": "ffmpeg", "ffprobe_path": "ffprobe"},
        "normalization": {
            "subtitle": {"mode": "burn"},
            "audio": {
                "format": "ac3", "bitrate_kbps": 192, "buffer_kbps": 384,
                "channels": 2, "sample_rate_hz": 48000, "normalize_loudness": true,
                "loudness": {"integrated_target": -16.0}
            },
            "video": {
                "format": "hevc", "bit_depth": 10, "width": 1920, "height": 1080,
                "scaling_mode": "scale_and_pad", "deinterlace": false,
                "bitrate_kbps": 2000, "buffer_kbps": 4000, "accel": "vaapi",
                "filters": {
                    "libplacebo": {"tonemapping": "linear"},
                    "tonemap": {"tonemap": "linear"},
                    "tonemap_opencl": {"tonemap": "linear"}
                },
                "vaapi_driver": "ihd"
            }
        }
    });
    let parsed: ChannelConfig = serde_json::from_value(wire.clone()).unwrap();
    assert_eq!(serde_json::to_value(parsed).unwrap(), wire);
}
