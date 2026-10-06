use super::*;
use ersatzrs_domain::Resolution;
use serde_json::{json, Value};

fn profile() -> PlaybackProfile {
    PlaybackProfile {
        profile: FFmpegProfile {
            video_format: FFmpegProfileVideoFormat::Hevc,
            audio_format: FFmpegProfileAudioFormat::Ac3,
            bit_depth: FFmpegProfileBitDepth::TenBit,
            audio_sample_rate: 48,
            audio_channels: 6,
            audio_bitrate: 384,
            audio_buffer_size: 768,
            video_bitrate: 4000,
            video_buffer_size: 8000,
            normalize_loudness_mode: NormalizeLoudnessMode::LoudNorm,
            target_loudness: Some(-16.0),
            scaling_behavior: ScalingBehavior::Crop,
            deinterlace_video: Some(true),
            ..Default::default()
        },
        resolution: Resolution {
            width: 1920,
            height: 1080,
            ..Default::default()
        },
    }
}

fn config(profile: &PlaybackProfile) -> Result<wire::ChannelConfig, ConfigError> {
    channel_config(
        profile,
        NextEngineTextSubtitleMode::Burn,
        Path::new("playouts/1/current"),
        Path::new("ffmpeg"),
        Path::new("ffprobe"),
    )
}

fn json_config(profile: &PlaybackProfile) -> Value {
    serde_json::to_value(config(profile).unwrap()).unwrap()
}

#[test]
fn persisted_profile_maps_units_loudness_resolution_and_video_policy() {
    let config = json_config(&profile());
    assert_eq!(
        config["normalization"]["audio"],
        json!({
            "format": "ac3", "bitrate_kbps": 384, "buffer_kbps": 768,
            "channels": 6, "sample_rate_hz": 48000, "normalize_loudness": true,
            "loudness": {"integrated_target": -16.0}
        })
    );
    let video = &config["normalization"]["video"];
    assert_eq!(video["format"], "hevc");
    assert_eq!(video["bit_depth"], 10);
    assert_eq!(video["width"], 1920);
    assert_eq!(video["height"], 1080);
    assert_eq!(video["bitrate_kbps"], 4000);
    assert_eq!(video["buffer_kbps"], 8000);
    assert_eq!(video["scaling_mode"], "crop");
    assert_eq!(video["deinterlace"], true);
    assert_eq!(video["accel"], Value::Null);
    assert_eq!(config["ffmpeg"]["ffmpeg_path"], "ffmpeg");
    assert_eq!(config["ffmpeg"]["ffprobe_path"], "ffprobe");
    assert_eq!(config["playout"]["folder"], "playouts/1/current");
}

#[test]
fn unsupported_explicit_output_is_rejected_without_codec_or_hdr_fallback() {
    for format in [
        FFmpegProfileVideoFormat::None,
        FFmpegProfileVideoFormat::Av1,
        FFmpegProfileVideoFormat::Copy,
    ] {
        let mut profile = profile();
        profile.profile.video_format = format;
        assert!(matches!(config(&profile), Err(ConfigError::VideoCodec)));
    }
    for format in [
        FFmpegProfileAudioFormat::None,
        FFmpegProfileAudioFormat::Copy,
        FFmpegProfileAudioFormat::AacLatm,
    ] {
        let mut profile = profile();
        profile.profile.audio_format = format;
        assert!(matches!(config(&profile), Err(ConfigError::AudioCodec)));
    }
    let mut profile = profile();
    profile.profile.output_dynamic_range = OutputDynamicRange::Hdr10Bt2020Pq;
    assert!(matches!(config(&profile), Err(ConfigError::HdrOutput)));
}

#[test]
fn mpeg2_output_forces_eight_bits_and_requires_explicit_positive_bitrate() {
    let mut profile = profile();
    profile.profile.video_format = FFmpegProfileVideoFormat::Mpeg2Video;
    for depth in [
        FFmpegProfileBitDepth::EightBit,
        FFmpegProfileBitDepth::TenBit,
    ] {
        profile.profile.bit_depth = depth;
        let video = &json_config(&profile)["normalization"]["video"];
        assert_eq!(video["format"], "mpeg2video");
        assert_eq!(video["bit_depth"], 8);
        assert_eq!(video["bitrate_kbps"], 4000);
        let decoded: wire::VideoFormat = serde_json::from_value(video["format"].clone()).unwrap();
        assert!(matches!(decoded, wire::VideoFormat::Mpeg2Video));
    }
    for bitrate in [0, -1] {
        profile.profile.video_bitrate = bitrate;
        assert!(matches!(config(&profile), Err(ConfigError::VideoBitrate)));
    }
}

#[test]
fn hardware_and_vaapi_driver_choices_survive_the_wire_mapping() {
    for (accel, name) in [
        (HardwareAccelerationKind::Nvenc, "cuda"),
        (HardwareAccelerationKind::Qsv, "qsv"),
        (HardwareAccelerationKind::Vaapi, "vaapi"),
        (HardwareAccelerationKind::VideoToolbox, "videotoolbox"),
        (HardwareAccelerationKind::Amf, "amf"),
        (HardwareAccelerationKind::Rkmpp, "rkmpp"),
    ] {
        let mut profile = profile();
        profile.profile.hardware_acceleration = accel;
        assert_eq!(
            json_config(&profile)["normalization"]["video"]["accel"],
            name
        );
    }
    let mut profile = profile();
    profile.profile.hardware_acceleration = HardwareAccelerationKind::V4l2m2m;
    assert!(matches!(config(&profile), Err(ConfigError::Accelerator)));
    profile.profile.hardware_acceleration = HardwareAccelerationKind::Vaapi;
    profile.profile.vaapi_device = Some("devices/render-node".to_owned());
    for (driver, name) in [
        (VaapiDriver::Auto, None),
        (VaapiDriver::IHD, Some("ihd")),
        (VaapiDriver::I965, Some("i965")),
        (VaapiDriver::RadeonSI, Some("radeonsi")),
    ] {
        profile.profile.vaapi_driver = driver;
        let config = json_config(&profile);
        let video = &config["normalization"]["video"];
        assert_eq!(
            video.get("vaapi_driver"),
            name.map(serde_json::Value::from).as_ref()
        );
        let decoded: wire::Video = serde_json::from_value(video.clone()).unwrap();
        assert_eq!(serde_json::to_value(decoded).unwrap(), *video);
        assert_eq!(
            config["normalization"]["video"]["vaapi_device"],
            "devices/render-node"
        );
    }
    profile.profile.vaapi_driver = VaapiDriver::Nouveau;
    assert!(matches!(config(&profile), Err(ConfigError::VaapiDriver)));
}

#[test]
fn amf_adapter_wire_preserves_auto_and_validates_explicit_indexes() {
    let mut profile = profile();
    profile.profile.hardware_acceleration = HardwareAccelerationKind::Amf;
    let config = config(&profile).unwrap();
    let automatic = serde_json::to_value(&config.normalization.video).unwrap();
    assert!(automatic.get("amf_device").is_none());
    let decoded: wire::Video = serde_json::from_value(automatic.clone()).unwrap();
    assert!(serde_json::to_value(decoded)
        .unwrap()
        .get("amf_device")
        .is_none());
    for adapter in [Value::Null, json!(0), json!(2), json!(u32::MAX)] {
        let mut explicit = automatic.clone();
        explicit["amf_device"] = adapter.clone();
        let decoded: wire::Video = serde_json::from_value(explicit).unwrap();
        assert_eq!(
            serde_json::to_value(decoded).unwrap()["amf_device"],
            adapter
        );
    }
    for invalid in [
        json!(-1),
        json!(u64::from(u32::MAX) + 1),
        json!(1.5),
        json!("2"),
    ] {
        let mut explicit = automatic.clone();
        explicit["amf_device"] = invalid;
        assert!(serde_json::from_value::<wire::Video>(explicit).is_err());
    }
}

#[test]
fn tonemapping_and_scaling_are_not_silently_reset_to_defaults() {
    for (algorithm, name) in [
        (FFmpegProfileTonemapAlgorithm::Linear, "linear"),
        (FFmpegProfileTonemapAlgorithm::Clip, "clip"),
        (FFmpegProfileTonemapAlgorithm::Gamma, "gamma"),
        (FFmpegProfileTonemapAlgorithm::Reinhard, "reinhard"),
        (FFmpegProfileTonemapAlgorithm::Mobius, "mobius"),
        (FFmpegProfileTonemapAlgorithm::Hable, "hable"),
    ] {
        let mut profile = profile();
        profile.profile.tonemap_algorithm = algorithm;
        let config = json_config(&profile);
        let filters = &config["normalization"]["video"]["filters"];
        assert_eq!(filters["libplacebo"]["tonemapping"], name);
        assert_eq!(filters["tonemap"]["tonemap"], name);
        assert_eq!(filters["tonemap_opencl"]["tonemap"], name);
    }
    for (behavior, name) in [
        (ScalingBehavior::ScaleAndPad, "scale_and_pad"),
        (ScalingBehavior::Stretch, "stretch"),
        (ScalingBehavior::Crop, "crop"),
    ] {
        let mut profile = profile();
        profile.profile.scaling_behavior = behavior;
        assert_eq!(
            json_config(&profile)["normalization"]["video"]["scaling_mode"],
            name
        );
    }
}

#[test]
fn numeric_boundaries_and_inactive_loudness_follow_native_profile_semantics() {
    let mut profile = profile();
    profile.profile.audio_sample_rate = i32::MAX;
    assert!(matches!(config(&profile), Err(ConfigError::SampleRate)));
    profile.profile.audio_sample_rate = 0;
    profile.profile.audio_channels = -1;
    profile.profile.video_buffer_size = 0;
    profile.profile.audio_format = FFmpegProfileAudioFormat::Aac;
    profile.profile.video_format = FFmpegProfileVideoFormat::H264;
    profile.profile.normalize_loudness_mode = NormalizeLoudnessMode::Off;
    profile.profile.target_loudness = Some(f64::NAN);
    let value = json_config(&profile);
    assert_eq!(value["normalization"]["audio"]["format"], "aac");
    assert_eq!(value["normalization"]["video"]["format"], "h264");
    assert!(value["normalization"]["audio"]
        .get("sample_rate_hz")
        .is_none());
    assert!(value["normalization"]["audio"].get("channels").is_none());
    assert!(value["normalization"]["audio"].get("loudness").is_none());
    assert!(value["normalization"]["video"].get("buffer_kbps").is_none());
    profile.profile.normalize_loudness_mode = NormalizeLoudnessMode::LoudNorm;
    for invalid in [f64::NAN, f64::INFINITY, -71.0, -4.0] {
        profile.profile.target_loudness = Some(invalid);
        assert!(matches!(config(&profile), Err(ConfigError::Loudness)));
    }
    profile.profile.target_loudness = None;
    profile.resolution.width = 0;
    assert!(matches!(config(&profile), Err(ConfigError::Resolution)));
    assert!(matches!(
        channel_config(
            &profile,
            NextEngineTextSubtitleMode::Burn,
            Path::new(""),
            Path::new("ffmpeg"),
            Path::new("ffprobe")
        ),
        Err(ConfigError::RuntimePath)
    ));
}

#[test]
fn text_subtitle_normalization_uses_explicit_channel_mode() {
    for (mode, expected) in [
        (NextEngineTextSubtitleMode::Burn, "burn"),
        (NextEngineTextSubtitleMode::Convert, "convert"),
    ] {
        let config = channel_config(
            &profile(),
            mode,
            Path::new("playout"),
            Path::new("ffmpeg"),
            Path::new("ffprobe"),
        )
        .unwrap();
        let json = serde_json::to_value(config).unwrap();
        assert_eq!(json["normalization"]["subtitle"]["mode"], expected);
    }
}

#[test]
fn generated_fallback_errors_are_enabled_in_both_subtitle_modes() {
    for mode in [
        NextEngineTextSubtitleMode::Burn,
        NextEngineTextSubtitleMode::Convert,
    ] {
        let config = channel_config(
            &profile(),
            mode,
            Path::new("playouts/current"),
            Path::new("ffmpeg"),
            Path::new("ffprobe"),
        )
        .unwrap();
        assert_eq!(
            serde_json::to_value(config).unwrap()["fallback"],
            serde_json::json!({"show_error": true})
        );
    }
}
