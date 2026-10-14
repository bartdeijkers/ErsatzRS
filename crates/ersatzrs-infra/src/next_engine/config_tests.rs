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
            "mode": "transcode",
            "format": "ac3", "bitrate_kbps": 384, "buffer_kbps": 768,
            "channels": 6, "sample_rate_hz": 48000, "normalize_loudness": true,
            "loudness": {"integrated_target": -16.0}
        })
    );
    let video = &config["normalization"]["video"];
    assert_eq!(config["version"], wire::CHANNEL_VERSION);
    assert_eq!(video["mode"], "transcode");
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
fn copy_modes_are_independent_and_retain_transcode_fallback_settings() {
    for (audio_copy, video_copy) in [(true, true), (true, false), (false, true)] {
        let mut profile = profile();
        if audio_copy {
            profile.profile.audio_format = FFmpegProfileAudioFormat::Copy;
        }
        if video_copy {
            profile.profile.video_format = FFmpegProfileVideoFormat::Copy;
        }
        let config = json_config(&profile);
        assert_eq!(config["version"], wire::CHANNEL_VERSION);
        let audio = &config["normalization"]["audio"];
        let video = &config["normalization"]["video"];
        assert_eq!(audio["mode"], if audio_copy { "copy" } else { "transcode" });
        assert_eq!(video["mode"], if video_copy { "copy" } else { "transcode" });
        assert_eq!(audio["format"], if audio_copy { "aac" } else { "ac3" });
        assert_eq!(video["format"], if video_copy { "h264" } else { "hevc" });
        assert_eq!(audio["normalize_loudness"], !audio_copy);
        assert_eq!(audio.get("loudness").is_some(), !audio_copy);
        assert_eq!(audio["channels"], 6);
        assert_eq!(audio["sample_rate_hz"], 48000);
        assert_eq!(video["width"], 1920);
        assert_eq!(video["height"], 1080);
        assert_eq!(video["bitrate_kbps"], 4000);
        assert_eq!(video["bit_depth"], 10);
        assert!(audio.get("copy_formats").is_none());
        assert!(video.get("copy_formats").is_none());
    }
}

#[test]
fn audio_copy_ignores_inactive_loudness_but_transcode_validates_it() {
    let mut profile = profile();
    profile.profile.audio_format = FFmpegProfileAudioFormat::Copy;
    profile.profile.target_loudness = Some(f64::NAN);
    let audio = &json_config(&profile)["normalization"]["audio"];
    assert_eq!(audio["normalize_loudness"], false);
    assert!(audio.get("loudness").is_none());
    profile.profile.audio_format = FFmpegProfileAudioFormat::Aac;
    assert!(matches!(config(&profile), Err(ConfigError::Loudness)));
}

#[test]
fn unsupported_explicit_output_is_rejected_without_codec_or_hdr_fallback() {
    for format in [
        FFmpegProfileVideoFormat::None,
        FFmpegProfileVideoFormat::Av1,
    ] {
        let mut profile = profile();
        profile.profile.video_format = format;
        assert!(matches!(config(&profile), Err(ConfigError::VideoCodec)));
    }
    for format in [
        FFmpegProfileAudioFormat::None,
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

#[test]
fn output_frame_rate_normalization_matches_upstream_decimal_and_rational_cases() {
    for (input, expected) in [
        ("24000/1001", "24000/1001"),
        ("25/1", "25/1"),
        ("30", "30"),
        ("23.976", "24000/1001"),
        ("23.98", "24000/1001"),
        ("29.97", "30000/1001"),
        ("59.94", "60000/1001"),
        ("25.00", "25"),
        ("12.5", "12500/1000"),
        (" 25/1 ", "25/1"),
        ("1", "1"),
        ("240", "240"),
        ("4294967295/4294967295", "4294967295/4294967295"),
        ("4294967295/2147483647", "4294967295/2147483647"),
        ("4294967280/17895697", "4294967280/17895697"),
    ] {
        assert_eq!(next_frame_rate(input).as_deref(), Some(expected), "{input}");
    }
    for input in [
        "",
        "0/0",
        "30/0",
        "1/2",
        "0.5",
        "241",
        "1000/1",
        "abc",
        "-30",
        "NaN",
        "inf",
        "1/2/3",
        "4294967296",
        "4294967296/4294967296",
        "6000000000/240000000",
        "4294967295/4294967296",
        "9223372036854775807/9223372036854775807",
        "9223372036854775808",
        "9223372036854775807/1",
    ] {
        assert!(next_frame_rate(input).is_none(), "{input}");
    }
}

#[test]
fn output_frame_rate_target_uses_lowest_above_23_only_for_mixed_rates() {
    for (rates, expected) in [
        (vec![], None),
        (vec!["25"], None),
        (vec!["25", "25"], None),
        (vec!["30", "25", "12.5"], Some("25")),
        (vec!["29.97", "23.98"], Some("24000/1001")),
        (vec!["12.5", "15"], Some("24")),
        (vec!["241", "300"], None),
    ] {
        assert_eq!(
            target_frame_rate(rates.into_iter().map(str::to_owned).collect())
                .unwrap()
                .as_deref(),
            expected
        );
    }
    assert!(target_frame_rate(vec!["invalid".into(), "25".into()]).is_err());
}

#[test]
fn output_frame_rate_is_optional_and_retains_independent_copy_and_subtitle_policy() {
    let mut profile = profile();
    profile.profile.video_format = FFmpegProfileVideoFormat::Copy;
    let mut configuration = config(&profile).unwrap();
    assert!(
        serde_json::to_value(&configuration).unwrap()["normalization"]["video"]
            .get("frame_rate")
            .is_none()
    );
    let baseline = serde_json::to_value(&configuration).unwrap();
    configuration.normalization.video.frame_rate = next_frame_rate("29.97");
    let value = serde_json::to_value(&configuration).unwrap();
    assert_eq!(
        value["version"],
        "https://ersatztv.org/channel/version/0.1.2"
    );
    assert_eq!(value["normalization"]["video"]["frame_rate"], "30000/1001");
    assert_eq!(value["normalization"]["video"]["mode"], "copy");
    assert_eq!(
        value["normalization"]["audio"],
        baseline["normalization"]["audio"]
    );
    assert_eq!(
        value["normalization"]["subtitle"],
        baseline["normalization"]["subtitle"]
    );
    let decoded: wire::ChannelConfig = serde_json::from_value(value.clone()).unwrap();
    assert_eq!(serde_json::to_value(decoded).unwrap(), value);
}

#[tokio::test]
async fn mirrored_output_frame_rate_samples_source_playout_with_requested_profile() {
    use ersatzrs_core::repositories::{
        ChannelRepository, FfmpegProfileRepository, PlayoutRepository,
    };
    use ersatzrs_domain::{Channel, ChannelPlayoutSource, PlayoutItem};
    let (pool, _, source, start) = super::super::schedule::tests::fixture().await;
    let repos = crate::repositories::make_repositories(pool.clone());
    sqlx::query(r#"UPDATE "MediaVersion" SET "RFrameRate"='29.97' WHERE "Id"=100"#)
        .execute(&pool)
        .await
        .unwrap();
    for sql in [
        r#"INSERT INTO "MediaItem" ("Id","LibraryPathId","State") VALUES (11,1,0)"#,
        r#"INSERT INTO "Movie" ("Id") VALUES (11)"#,
        r#"INSERT INTO "MediaVersion" ("Id","DateAdded","DateUpdated","Duration","MovieId","Height","Width","VideoScanKind","RFrameRate") VALUES (101,'2026-09-22','2026-09-22','00:01:00',11,180,320,0,'23.98')"#,
        r#"INSERT INTO "MediaFile" ("Id","MediaVersionId","Path","PathHash") VALUES (3,101,'media/third.mkv','third')"#,
    ] {
        sqlx::query(sql).execute(&pool).await.unwrap();
    }
    repos
        .playout
        .upsert_item(&PlayoutItem {
            id: 3,
            playout_id: 1,
            media_item_id: 11,
            start: start + chrono::Duration::minutes(2),
            finish: start + chrono::Duration::minutes(3),
            ..Default::default()
        })
        .await
        .unwrap();
    repos
        .ffmpeg_profile
        .upsert(&FFmpegProfile {
            id: 2,
            resolution_id: 1,
            normalize_framerate: true,
            ..Default::default()
        })
        .await
        .unwrap();
    let mut mirror = Channel {
        id: 2,
        number: "2".into(),
        name: "Mirror".into(),
        ffmpeg_profile_id: 2,
        playout_source: ChannelPlayoutSource::Mirror,
        mirror_source_channel_id: Some(source.id),
        ..Default::default()
    };
    repos.channel.upsert(&mirror).await.unwrap();
    let requested = crate::playback::load_channel_playback_profile(&pool, mirror.id)
        .await
        .unwrap()
        .unwrap();
    let source_profile = crate::playback::load_channel_playback_profile(&pool, source.id)
        .await
        .unwrap()
        .unwrap();
    assert!(!source_profile.profile.normalize_framerate);
    assert_eq!(
        super::super::schedule::frame_rates(&pool, mirror.id)
            .await
            .unwrap(),
        Vec::<String>::new()
    );
    for offset in [chrono::Duration::hours(1), -chrono::Duration::hours(1)] {
        mirror.playout_offset = Some(offset);
        assert_eq!(
            channel_frame_rate(&pool, &mirror, &requested)
                .await
                .unwrap()
                .as_deref(),
            Some("24000/1001")
        );
    }
    assert_eq!(
        channel_frame_rate(&pool, &source, &source_profile)
            .await
            .unwrap(),
        None
    );
    let mut disabled = requested.clone();
    disabled.profile.normalize_framerate = false;
    pool.close().await;
    assert_eq!(
        channel_frame_rate(&pool, &mirror, &disabled).await.unwrap(),
        None,
        "disabled normalization must not query scheduled source data"
    );
    assert!(channel_frame_rate(&pool, &mirror, &requested)
        .await
        .is_err());
}

#[tokio::test]
async fn selected_output_frame_rate_ignores_unused_versions_but_counts_exact_pins() {
    let (pool, _, channel, _) = super::super::schedule::tests::fixture().await;
    let mut requested = crate::playback::load_channel_playback_profile(&pool, channel.id)
        .await
        .unwrap()
        .unwrap();
    requested.profile.normalize_framerate = true;
    sqlx::query(r#"UPDATE "MediaVersion" SET "RFrameRate"='30' WHERE "Id"=100"#)
        .execute(&pool)
        .await
        .unwrap();
    for sql in [
        r#"INSERT INTO "MediaVersion" ("Id","DateAdded","DateUpdated","Duration","MovieId","Height","Width","VideoScanKind","RFrameRate") VALUES (101,'2026-09-22','2026-09-22','00:01:00',10,180,320,0,'25')"#,
        r#"INSERT INTO "MediaFile" ("Id","MediaVersionId","Path","PathHash") VALUES (3,101,'media/unused-alternate.mkv','alternate')"#,
        r#"INSERT INTO "MediaStream" ("Id","Index","Codec","MediaStreamKind","Language","Channels","Title","Default","Forced","MediaVersionId") VALUES (6,0,'h264',1,NULL,0,NULL,1,0,101),(7,2,'aac',2,'nld',2,'Dutch',0,0,101),(8,4,'subrip',3,'eng',0,'English',1,0,101)"#,
    ] {
        sqlx::query(sql).execute(&pool).await.unwrap();
    }
    assert_eq!(
        super::super::schedule::frame_rates(&pool, channel.id)
            .await
            .unwrap(),
        ["30"],
        "unused25fps alternate must not manufacture a mixed-rate schedule"
    );
    assert_eq!(
        channel_frame_rate(&pool, &channel, &requested)
            .await
            .unwrap(),
        None
    );
    sqlx::query(r#"UPDATE "PlayoutItem" SET "MediaVersionId"=101,"VideoStreamIndex"=0,"AudioStreamIndex"=2,"SubtitleStreamIndex"=4 WHERE "Id"=2"#).execute(&pool).await.unwrap();
    let before: (i64,i32,i32,i32) = sqlx::query_as(r#"SELECT "MediaVersionId","VideoStreamIndex","AudioStreamIndex","SubtitleStreamIndex" FROM "PlayoutItem" WHERE "Id"=2"#).fetch_one(&pool).await.unwrap();
    assert_eq!(before, (101, 0, 2, 4));
    assert_eq!(
        super::super::schedule::frame_rates(&pool, channel.id)
            .await
            .unwrap(),
        ["30", "25"]
    );
    assert_eq!(
        channel_frame_rate(&pool, &channel, &requested)
            .await
            .unwrap()
            .as_deref(),
        Some("25"),
        "the explicitly selected25fps alternate must contribute to the target"
    );
    let after: (i64,i32,i32,i32) = sqlx::query_as(r#"SELECT "MediaVersionId","VideoStreamIndex","AudioStreamIndex","SubtitleStreamIndex" FROM "PlayoutItem" WHERE "Id"=2"#).fetch_one(&pool).await.unwrap();
    assert_eq!(
        after, before,
        "sampling must preserve exact version and international audio/subtitle pins"
    );
    sqlx::query(r#"UPDATE "PlayoutItem" SET "MediaVersionId"=101,"VideoStreamIndex"=0,"AudioStreamIndex"=2,"SubtitleStreamIndex"=4 WHERE "Id"=1"#).execute(&pool).await.unwrap();
    assert_eq!(
        channel_frame_rate(&pool, &channel, &requested)
            .await
            .unwrap(),
        None,
        "homogeneous pinned25fps also passes through"
    );
    pool.close().await;
}

#[tokio::test]
async fn output_frame_rate_omits_mixed_target_rejected_by_packaged_next_integer_bounds() {
    use ersatzrs_core::repositories::PlayoutRepository as _;
    use ersatzrs_domain::PlayoutItem;
    let (pool, _, channel, start) = super::super::schedule::tests::fixture().await;
    let mut requested = crate::playback::load_channel_playback_profile(&pool, channel.id)
        .await
        .unwrap()
        .unwrap();
    requested.profile.normalize_framerate = true;
    requested.profile.audio_format = FFmpegProfileAudioFormat::Aac;
    requested.profile.video_format = FFmpegProfileVideoFormat::H264;
    sqlx::query(r#"UPDATE "MediaVersion" SET "RFrameRate"='30' WHERE "Id"=100"#)
        .execute(&pool)
        .await
        .unwrap();
    for sql in [
        r#"INSERT INTO "MediaItem" ("Id","LibraryPathId","State") VALUES (11,1,0)"#,
        r#"INSERT INTO "Movie" ("Id") VALUES (11)"#,
        r#"INSERT INTO "MediaVersion" ("Id","DateAdded","DateUpdated","Duration","MovieId","Height","Width","VideoScanKind","RFrameRate") VALUES (101,'2026-09-22','2026-09-22','00:01:00',11,180,320,0,'6000000000/240000000')"#,
        r#"INSERT INTO "MediaFile" ("Id","MediaVersionId","Path","PathHash") VALUES (3,101,'media/oversized-rate.mkv','oversized-rate')"#,
    ] {
        sqlx::query(sql).execute(&pool).await.unwrap();
    }
    crate::repositories::make_repositories(pool.clone())
        .playout
        .upsert_item(&PlayoutItem {
            id: 3,
            playout_id: 1,
            media_item_id: 11,
            start: start + chrono::Duration::minutes(2),
            finish: start + chrono::Duration::minutes(3),
            ..Default::default()
        })
        .await
        .unwrap();
    let mut configuration = config(&requested).unwrap();
    configuration.normalization.video.frame_rate = channel_frame_rate(&pool, &channel, &requested)
        .await
        .unwrap();
    let wire = serde_json::to_value(&configuration).unwrap();
    assert!(
        wire["normalization"]["video"].get("frame_rate").is_none(),
        "numerically25fps with an oversized integer must not fail the packaged Next parser"
    );
    assert_eq!(
        wire["version"],
        "https://ersatztv.org/channel/version/0.1.2"
    );
    let retained: String =
        sqlx::query_scalar(r#"SELECT "RFrameRate" FROM "MediaVersion" WHERE "Id"=101"#)
            .fetch_one(&pool)
            .await
            .unwrap();
    assert_eq!(
        retained, "6000000000/240000000",
        "wire rejection must not rewrite scanned metadata"
    );
    pool.close().await;
}

#[test]
fn output_frame_rate_unknown_source_sentinel_is_ranked_but_never_emitted() {
    for rates in [
        vec!["0/0"],
        vec!["0/0", "0/0"],
        vec!["0/0", "25"],
        vec!["25", " 0/0 "],
    ] {
        assert_eq!(
            target_frame_rate(rates.into_iter().map(str::to_owned).collect()).unwrap(),
            None,
            "unknown source timing must not prevent channel startup or become an output target"
        );
    }
    assert!(next_frame_rate("0/0").is_none());
    assert!(
        target_frame_rate(vec!["invalid".into(), "25".into()]).is_err(),
        "other malformed metadata must retain the existing error contract"
    );
}
