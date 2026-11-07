use super::*;
use ersatzrs_domain::{MediaVersion, VideoScanKind};

pub(crate) fn metadata() -> FfprobeMediaVersion {
    FfprobeMediaVersion {
        field_orders: [(17, "progressive".into())].into_iter().collect(),
        version: MediaVersion {
            r_frame_rate: Some("30000/1001".into()),
            video_scan_kind: VideoScanKind::Progressive,
            sample_aspect_ratio: Some("1:1".into()),
            display_aspect_ratio: Some("16:9".into()),
            ..Default::default()
        },
        streams: vec![
            MediaStream {
                index: 17,
                kind: MediaStreamKind::Video,
                codec: Some("rawvideo".into()),
                width: Some(320),
                height: Some(180),
                pixel_format: Some("gbrp10le".into()),
                color_range: Some("pc".into()),
                color_space: Some("bt2020nc".into()),
                color_transfer: Some("smpte2084".into()),
                color_primaries: Some("bt2020".into()),
                rotation: Some(90),
                max_cll: Some(1000),
                ..Default::default()
            },
            MediaStream {
                index: 23,
                kind: MediaStreamKind::Audio,
                codec: Some("pcm_f32le".into()),
                channels: Some(2),
                ..Default::default()
            },
        ],
        chapters: Vec::new(),
        format_name: None,
        format_metadata: std::collections::BTreeMap::new(),
    }
}

#[test]
fn fresh_retained_indexes_pixel_format_and_color_metadata_reach_wire() {
    let hint = retained(&metadata(), 12_345).unwrap();
    let json = serde_json::to_value(&hint).unwrap();
    assert_eq!(json["duration_ms"], 12_345);
    assert_eq!(json["format_name"], "nut");
    assert_eq!(json["video"][0]["stream_index"], 17);
    assert_eq!(json["audio"][0]["stream_index"], 23);
    assert_eq!(json["audio"][0]["codec"], "pcm_f32le");
    assert_eq!(json["video"][0]["pix_fmt"], "gbrp10le");
    assert_eq!(json["video"][0]["frame_rate"], "30000/1001");
    assert_eq!(json["video"][0]["field_order"], "progressive");
    assert_eq!(json["video"][0]["color_range"], "pc");
    assert_eq!(json["video"][0]["color_space"], "bt2020nc");
    assert_eq!(json["video"][0]["color_transfer"], "smpte2084");
    assert_eq!(json["video"][0]["color_primaries"], "bt2020");
    assert_eq!(json["video"][0]["rotation"], 90);
    assert_eq!(json["video"][0]["has_hdr10_metadata"], true);
    assert!(json["video"][0].get("dv_profile").is_none());
    let decoded: ProbeHint = serde_json::from_value(json.clone()).unwrap();
    assert_eq!(serde_json::to_value(decoded).unwrap(), json);
}

#[test]
fn invalid_retained_metadata_never_fabricates_probe_hints() {
    for case in 0..12 {
        let mut probe = metadata();
        match case {
            0 => probe.streams[0].index = -1,
            1 => probe.streams[1].index = 17,
            2 => probe.streams[0].width = Some(0),
            3 => probe.streams[0].height = Some(-1),
            4 => probe.streams[0].pixel_format = None,
            5 => probe.version.r_frame_rate = Some("1/0".into()),
            6 => probe.version.r_frame_rate = None,
            7 => probe.streams[1].channels = Some(0),
            8 => probe.streams[1].codec = Some("aac".into()),
            9 => {
                probe.streams.remove(1);
            }
            10 => probe.streams[0].kind = MediaStreamKind::Subtitle,
            _ => probe.streams[0].attached_pic = true,
        }
        assert!(
            retained(&probe, 1000).is_err(),
            "invalid metadata case {case}"
        );
    }
    assert!(retained(&metadata(), 0).is_err());
}

#[test]
fn http_and_rtsp_roundtrip_complete_typed_probe_hints() {
    for source_type in ["http", "rtsp"] {
        let json = serde_json::json!({
            "source_type":source_type, "uri":"synthetic://stream",
            "probe_hint":retained(&metadata(), 1000).unwrap()
        });
        let source: super::super::protocol::Source = serde_json::from_value(json.clone()).unwrap();
        assert_eq!(serde_json::to_value(source).unwrap(), json);
    }
}

#[test]
fn fresh_interlaced_field_order_survives_without_sql_or_guesses() {
    for order in ["tt", "bb", "tb", "bt"] {
        let json = serde_json::json!({"streams":[
            {"index":7,"codec_type":"video","codec_name":"rawvideo","width":320,"height":180,"pix_fmt":"yuv420p","r_frame_rate":"25/1","field_order":order},
            {"index":9,"codec_type":"audio","codec_name":"pcm_f32le","channels":2}
        ]});
        let probe = crate::ffprobe::parse_ffprobe_json(
            "synthetic.nut",
            &serde_json::to_vec(&json).unwrap(),
        )
        .unwrap();
        assert_eq!(probe.version.video_scan_kind, VideoScanKind::Interlaced);
        let hint = retained(&probe, 1000).unwrap();
        assert_eq!(hint.video[0].field_order.as_deref(), Some(order));
    }
}

#[test]
fn ordinary_producer_preserves_detected_container_codecs_and_subtitles_without_duration() {
    let mut probe = metadata();
    probe.format_name = Some("matroska,webm".into());
    probe.version.duration = chrono::Duration::seconds(999);
    probe.streams[0].codec = Some("hevc".into());
    probe.streams[1].codec = Some("aac".into());
    probe.streams.push(MediaStream {
        index: 42,
        kind: MediaStreamKind::Subtitle,
        codec: Some("subrip".into()),
        ..Default::default()
    });
    probe.streams.push(MediaStream {
        index: 43,
        kind: MediaStreamKind::Attachment,
        ..Default::default()
    });
    let hint = producer(&probe).unwrap();
    assert_eq!(hint.format_name.as_deref(), Some("matroska,webm"));
    assert_eq!(hint.duration_ms, None);
    assert_eq!(hint.video[0].codec, "hevc");
    assert_eq!(hint.video[0].stream_index, 17);
    assert_eq!(hint.audio[0].codec, "aac");
    assert_eq!(
        hint.subtitle.len(),
        1,
        "font attachments are not selectable tracks"
    );
    assert_eq!(hint.subtitle[0].codec, "subrip");
    assert_eq!(hint.subtitle[0].stream_index, 42);
    assert!(
        retained(&probe, 1000).is_err(),
        "ordinary codecs must not weaken retained NUT validation"
    );
    probe.streams.remove(0);
    assert!(
        producer(&probe).unwrap().video.is_empty(),
        "audio-only producer metadata is valid"
    );
}

#[test]
fn ordinary_producer_excludes_cover_art_without_weakening_playable_stream_validation() {
    for real_video in [false, true] {
        let mut probe = metadata();
        probe.format_name = Some("matroska,webm".into());
        if !real_video {
            probe.streams.remove(0);
        }
        // Artwork requires no playable geometry, rate or codec metadata. Its
        // actual bytes remain untouched in the original producer stream.
        probe.streams.insert(
            0,
            MediaStream {
                index: 0,
                kind: MediaStreamKind::Video,
                attached_pic: true,
                ..Default::default()
            },
        );
        let hint = producer(&probe).unwrap();
        assert_eq!(hint.video.len(), usize::from(real_video));
        assert_eq!(hint.audio[0].stream_index, 23);
        assert_eq!(hint.audio[0].codec, "pcm_f32le");
        assert!(!hint.video.iter().any(|video| video.stream_index == 0));
        if real_video {
            assert_eq!(hint.video[0].stream_index, 17);
            let mut malformed = probe.clone();
            malformed.streams[1].pixel_format = None;
            assert!(producer(&malformed).is_err());
        }
        assert!(
            retained(&probe, 1000).is_err(),
            "retained rawvideo/PCM contract stays strict"
        );
        let mut duplicate = probe.clone();
        duplicate.streams[0].index = 23;
        assert!(
            producer(&duplicate).is_err(),
            "artwork does not excuse duplicate indexes"
        );
        let audio = probe
            .streams
            .iter_mut()
            .find(|stream| stream.kind == MediaStreamKind::Audio)
            .unwrap();
        audio.channels = Some(0);
        assert!(
            producer(&probe).is_err(),
            "playable audio still needs channels"
        );
    }
}

#[tokio::test]
async fn dolby_vision_profiles_survive_persisted_and_producer_hints() {
    let pool = super::super::selection::tests::fixture_pool().await;
    for profile in [None, Some(5), Some(7), Some(8)] {
        sqlx::query(
            r#"UPDATE "MediaStream" SET "DolbyVisionProfile"=? WHERE "MediaVersionId"=100 AND "Index"=0"#,
        )
        .bind(profile)
        .execute(&pool)
        .await
        .unwrap();
        let mut probe = metadata();
        probe.format_name = Some("matroska,webm".into());
        probe.streams[0].codec = Some("hevc".into());
        probe.streams[0].dolby_vision_profile = profile;
        for hint in [
            persisted(&pool, 100).await.unwrap().unwrap(),
            producer(&probe).unwrap(),
        ] {
            let json = serde_json::to_value(&hint).unwrap();
            if let Some(profile) = profile {
                assert_eq!(json["video"][0]["dv_profile"], profile);
            } else {
                assert!(json["video"][0].get("dv_profile").is_none());
            }
            let decoded: super::super::protocol::ProbeHint =
                serde_json::from_value(json.clone()).unwrap();
            assert_eq!(serde_json::to_value(decoded).unwrap(), json);
        }
    }
}

#[tokio::test]
async fn stored_hdr10_absence_remains_unknown_while_positive_evidence_reaches_next() {
    let pool = super::super::selection::tests::fixture_pool().await;
    for (mastering, cll, fall) in [
        (None, None, None),
        (Some("mastering-display"), None, None),
        (None, Some(1000), None),
        (None, None, Some(400)),
    ] {
        sqlx::query(r#"UPDATE "MediaStream" SET "MasteringDisplay"=?,"MaxCll"=?,"MaxFall"=? WHERE "MediaVersionId"=100 AND "Index"=0"#)
            .bind(mastering).bind(cll).bind(fall).execute(&pool).await.unwrap();
        let hint = persisted(&pool, 100).await.unwrap().unwrap();
        assert_eq!(
            hint.video[0].has_hdr10_metadata,
            (mastering.is_some() || cll.is_some() || fall.is_some()).then_some(true)
        );
        let json = serde_json::to_value(hint).unwrap();
        assert_ne!(
            json["video"][0].get("has_hdr10_metadata"),
            Some(&serde_json::json!(false))
        );
    }
    // Fresh frame/stream inspection can still positively establish absence.
    let mut probe = metadata();
    probe.streams[0].max_cll = None;
    assert_eq!(
        retained(&probe, 1000).unwrap().video[0].has_hdr10_metadata,
        Some(false)
    );
}

#[tokio::test]
async fn persisted_hint_preserves_all_selectable_tracks_and_refuses_invalid_geometry() {
    let pool = super::super::selection::tests::fixture_pool().await;
    let hint = persisted(&pool, 100).await.unwrap().unwrap();
    assert_eq!(hint.duration_ms, Some(60_000));
    assert_eq!(
        (hint.video[0].stream_index, hint.video[0].codec.as_str()),
        (0, "h264")
    );
    assert_eq!(
        hint.audio
            .iter()
            .map(|track| track.stream_index)
            .collect::<Vec<_>>(),
        vec![1, 2]
    );
    assert_eq!(
        hint.subtitle
            .iter()
            .map(|track| track.stream_index)
            .collect::<Vec<_>>(),
        vec![3, 4]
    );
    sqlx::query(r#"UPDATE "MediaVersion" SET "Width"=0 WHERE "Id"=100"#)
        .execute(&pool)
        .await
        .unwrap();
    assert!(persisted(&pool, 100).await.unwrap().is_none());
}

#[tokio::test]
async fn persisted_still_image_keeps_image_container_identity() {
    let pool = super::super::selection::tests::fixture_pool().await;
    for sql in [
        r#"INSERT INTO "Image" ("Id") VALUES (10)"#,
        r#"UPDATE "MediaVersion" SET "MovieId"=NULL,"ImageId"=10,"Duration"='00:00:00' WHERE "Id"=100"#,
        r#"UPDATE "MediaStream" SET "Codec"='png',"PixelFormat"='rgb24' WHERE "MediaVersionId"=100 AND "Index"=0"#,
        r#"DELETE FROM "MediaStream" WHERE "MediaVersionId"=100 AND "Index"<>0"#,
    ] {
        sqlx::query(sql).execute(&pool).await.unwrap();
    }
    let hint = persisted(&pool, 100).await.unwrap().unwrap();
    assert_eq!(hint.format_name.as_deref(), Some("png_pipe"));
    assert_eq!(hint.duration_ms, Some(15_000));
    assert_eq!(hint.video[0].pix_fmt, "rgb24");
}

#[test]
fn ordinary_producer_rejects_incomplete_and_ambiguous_prefix_metadata() {
    for case in 0..8 {
        let mut probe = metadata();
        probe.format_name = Some("mpegts".into());
        match case {
            0 => probe.format_name = None,
            1 => probe.format_name = Some(" ".into()),
            2 => probe.streams[0].codec = None,
            3 => probe.streams[1].codec = Some(String::new()),
            4 => probe.streams[1].index = 17,
            5 => probe.streams[0].pixel_format = None,
            6 => {
                let mut second = probe.streams[0].clone();
                second.index = 99;
                probe.streams.push(second);
            }
            _ => probe.streams.clear(),
        }
        assert!(producer(&probe).is_err(), "invalid producer case {case}");
    }
}

#[tokio::test]
async fn persisted_images_and_videos_use_the_selected_container() {
    let (pool, _, _) = crate::playout_duration::tests::image_fixture().await;
    for (codec, format) in [
        ("png", "png_pipe"),
        ("mjpeg", "jpeg_pipe"),
        ("bmp", "bmp_pipe"),
        ("tiff", "tiff_pipe"),
        ("webp", "webp_pipe"),
        ("webp_anim", "webp_anim"),
        ("gif", "gif"),
    ] {
        sqlx::query(
            r#"UPDATE "MediaStream" SET "Codec"=? WHERE "MediaVersionId"=100 AND "Index"=0"#,
        )
        .bind(codec)
        .execute(&pool)
        .await
        .unwrap();
        let hint = persisted(&pool, 100).await.unwrap().unwrap();
        assert_eq!(hint.format_name.as_deref(), Some(format));
        assert_eq!(hint.video[0].codec, codec);
    }
    sqlx::query(
        r#"UPDATE "MediaStream" SET "Codec"='unknown' WHERE "MediaVersionId"=100 AND "Index"=0"#,
    )
    .execute(&pool)
    .await
    .unwrap();
    assert!(persisted(&pool, 100).await.unwrap().is_none());
    let (pool, _, _, _) = super::super::schedule::tests::fixture().await;
    for (path, expected) in [
        ("synthetic.avi", Some("avi")),
        ("synthetic.AVI", Some("avi")),
        ("synthetic.mkv", Some("matroska,webm")),
        ("synthetic.MK3D", Some("matroska,webm")),
        ("synthetic.webm", Some("matroska,webm")),
        ("synthetic.mp4", Some("mov,mp4,m4a,3gp,3g2,mj2")),
        ("synthetic.M4V", Some("mov,mp4,m4a,3gp,3g2,mj2")),
        ("synthetic.mov", Some("mov,mp4,m4a,3gp,3g2,mj2")),
        ("synthetic.WMV", Some("asf")),
        ("synthetic.asf", Some("asf")),
        ("synthetic.ts", Some("mpegts")),
        ("synthetic.M2TS", Some("mpegts")),
        ("synthetic.mts", Some("mpegts")),
        ("synthetic.mpg", Some("mpeg")),
        ("synthetic.MPEG", Some("mpeg")),
        ("synthetic.vob", Some("mpeg")),
        ("synthetic.rmvb", None),
        ("synthetic.avi/file", None),
        ("avi", None),
    ] {
        sqlx::query(r#"UPDATE "MediaFile" SET "Path"=? WHERE "Id"=1"#)
            .bind(path)
            .execute(&pool)
            .await
            .unwrap();
        let hint = persisted(&pool, 100).await.unwrap().unwrap();
        assert_eq!(hint.format_name.as_deref(), expected);
        assert_eq!(hint.audio.len(), 2);
        assert_eq!(hint.subtitle.len(), 2);
    }
}
