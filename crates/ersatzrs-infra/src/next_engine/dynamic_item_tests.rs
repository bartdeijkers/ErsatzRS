use super::super::protocol::{AudioProbeHint, ProbeHint, Track, Tracks};
use super::*;
use chrono::{Duration, TimeZone};

fn local(start: Option<u64>) -> Source {
    Source::Local {
        path: "synthetic/fixture.mkv".into(),
        in_point_ms: start,
        out_point_ms: Some(90_000),

        probe_hint: None,
    }
}

fn item() -> PlayoutItem {
    let start = Utc.with_ymd_and_hms(2026, 9, 23, 12, 0, 0).unwrap();
    PlayoutItem {
        watermark: None,
        graphics: Vec::new(),
        id: "synthetic".into(),
        start,
        finish: start + Duration::seconds(60),
        source: Some(local(Some(10_000))),
        tracks: Some(Tracks {
            video: Some(Track {
                source: Some(local(Some(10_000))),
                stream_index: Some(0),
            }),
            audio: Some(Track {
                source: None,
                stream_index: Some(2),
            }),
            subtitle: Some(Track {
                source: Some(Source::Http {
                    probe_hint: None,
                    is_live: None,
                    uri: "http://127.0.0.1:8409/synthetic".into(),
                    headers: vec!["X-Fixture: synthetic".into()],
                    user_agent: Some("fixture".into()),
                    keep_alive: None,
                    reconnect: Some(true),
                    reconnect_delay_max: Some(2),
                    timeout_us: Some(500_000),
                    in_point_ms: Some(10_000),
                    out_point_ms: Some(90_000),
                }),
                stream_index: Some(0),
            }),
        }),
    }
}

fn dynamic() -> Source {
    Source::Dynamic {
        uri: "http://127.0.0.1:8409/synthetic-item".into(),
        headers: None,
        user_agent: None,
        timeout_us: Some(1_000_000),
        retry_budget_ms: None,
        retry_backoff_ms: None,
    }
}

#[test]
fn callback_advances_all_concrete_sources_without_changing_finish_or_transport() {
    let original = item();
    let requested = original.start + Duration::milliseconds(12_345);
    let resolved = resolve_at(&original, requested).unwrap();
    assert_eq!(resolved.start, requested);
    assert_eq!(resolved.finish, original.finish);
    let mut expected = serde_json::to_value(&original).unwrap();
    expected["start"] = serde_json::to_value(requested).unwrap();
    expected["source"]["in_point_ms"] = 22_345.into();
    expected["tracks"]["video"]["source"]["in_point_ms"] = 22_345.into();
    expected["tracks"]["subtitle"]["source"]["in_point_ms"] = 22_345.into();
    assert_eq!(serde_json::to_value(resolved).unwrap(), expected);
    assert_eq!(
        serde_json::to_value(resolve_at(&original, original.start).unwrap()).unwrap(),
        serde_json::to_value(&original).unwrap()
    );
}

#[test]
fn generated_silence_and_explicit_subtitles_off_remain_valid() {
    let mut original = item();
    original.source = None;
    let tracks = original.tracks.as_mut().unwrap();
    tracks.subtitle = None;
    tracks.video.as_mut().unwrap().source = Some(local(None));
    tracks.audio.as_mut().unwrap().source = Some(Source::Lavfi {
        params: "anullsrc=channel_layout=stereo:sample_rate=48000".into(),
        probe_hint: Some(ProbeHint {
            video: Vec::new(),
            subtitle: Vec::new(),
            format_name: Some("mpegts".into()),
            duration_ms: Some(60_000),
            audio: vec![AudioProbeHint {
                stream_index: 0,
                codec: "pcm_s16le".into(),
                channels: 2,
            }],
        }),
    });
    let resolved = resolve_at(&original, original.start + Duration::seconds(12)).unwrap();
    let json = serde_json::to_value(resolved).unwrap();
    assert!(json.get("source").is_none());
    assert!(json["tracks"].get("subtitle").is_none());
    assert_eq!(json["tracks"]["video"]["source"]["in_point_ms"], 12_000);
    assert_eq!(
        json["tracks"]["audio"],
        serde_json::to_value(original.tracks.unwrap().audio).unwrap()
    );
}

#[test]
fn recursive_dynamic_is_rejected_at_every_source_location() {
    for position in 0..4 {
        let mut original = item();
        match position {
            0 => original.source = Some(dynamic()),
            1 => {
                original
                    .tracks
                    .as_mut()
                    .unwrap()
                    .video
                    .as_mut()
                    .unwrap()
                    .source = Some(dynamic());
            }
            2 => {
                original
                    .tracks
                    .as_mut()
                    .unwrap()
                    .audio
                    .as_mut()
                    .unwrap()
                    .source = Some(dynamic());
            }
            _ => {
                original
                    .tracks
                    .as_mut()
                    .unwrap()
                    .subtitle
                    .as_mut()
                    .unwrap()
                    .source = Some(dynamic());
            }
        }
        assert!(matches!(
            resolve_at(&original, original.start),
            Err(DynamicItemError::RecursiveSource)
        ));
    }
}

#[test]
fn callbacks_reject_interval_boundaries_and_source_seek_overflow() {
    let original = item();
    for outside in [
        original.start - Duration::milliseconds(1),
        original.finish,
        original.finish + Duration::seconds(1),
    ] {
        assert!(matches!(
            resolve_at(&original, outside),
            Err(DynamicItemError::OutsideInterval)
        ));
    }
    let mut overflow = original.clone();
    overflow.source = Some(Source::Local {
        path: "synthetic".into(),
        in_point_ms: Some(u64::MAX),
        out_point_ms: None,

        probe_hint: None,
    });
    assert!(matches!(
        resolve_at(&overflow, original.start + Duration::milliseconds(1)),
        Err(DynamicItemError::SourceTiming)
    ));
    let mut exhausted = original.clone();
    exhausted.source = Some(Source::Local {
        path: "synthetic".into(),
        in_point_ms: Some(2_000),
        out_point_ms: Some(3_000),

        probe_hint: None,
    });
    assert!(matches!(
        resolve_at(&exhausted, original.start + Duration::seconds(1)),
        Err(DynamicItemError::SourceTiming)
    ));
}

#[test]
fn dynamic_source_wire_fields_match_next_and_omit_absent_options() {
    let source = dynamic();
    let json = serde_json::to_value(source).unwrap();
    assert_eq!(
        json,
        serde_json::json!({ "source_type":"dynamic", "uri":"http://127.0.0.1:8409/synthetic-item", "timeout_us":1_000_000 })
    );
    let decoded: Source = serde_json::from_value(serde_json::json!({ "source_type":"dynamic", "uri":"http://127.0.0.1:8409/synthetic-item", "headers":["X-Fixture: synthetic"], "user_agent":"fixture" })).unwrap();
    assert!(
        matches!(decoded, Source::Dynamic { headers: Some(headers), .. } if headers == ["X-Fixture: synthetic"])
    );
}

#[test]
fn joining_programme_does_not_seek_the_independent_watermark() {
    let mut original = item();
    original.watermark = Some(
        serde_json::from_value(serde_json::json!({
            "source": { "source_type": "local", "path": "synthetic/watermark.png" },
            "stream_index": 0,
            "location": "top_left",
            "width_percent": 12.5,
            "opacity_percent": 75.0
        }))
        .unwrap(),
    );
    original.graphics = vec![
        original.watermark.clone().unwrap(),
        original.watermark.clone().unwrap(),
    ];
    original.graphics[1].opacity_percent = Some(25.0);
    let graphics = serde_json::to_value(&original.graphics).unwrap();
    let resolved = resolve_at(&original, original.start + Duration::seconds(15)).unwrap();
    assert_eq!(serde_json::to_value(&resolved.graphics).unwrap(), graphics);
    let expected = serde_json::to_value(&original.watermark).unwrap();
    let resolved = resolve_at(&original, original.start + Duration::seconds(15)).unwrap();
    assert_eq!(serde_json::to_value(&resolved.watermark).unwrap(), expected);
    assert_ne!(
        serde_json::to_value(&resolved.tracks).unwrap(),
        serde_json::to_value(&original.tracks).unwrap()
    );

    original.watermark.as_mut().unwrap().timing =
        Some(super::super::protocol::WatermarkTiming::Periodic {
            clock: super::super::protocol::PeriodicClock::Wall,
            frequency_ms: 60_000,
            hold_ms: 10_000,
        });
    let expected = serde_json::to_value(&original.watermark).unwrap();
    let resolved = resolve_at(&original, original.start + Duration::seconds(15)).unwrap();
    assert_eq!(serde_json::to_value(&resolved.watermark).unwrap(), expected);
}

#[test]
fn live_callbacks_preserve_tracks_without_seeking_live_inputs() {
    for live in [
        serde_json::json!({"source_type":"http", "uri":"https://example.test/live", "is_live":true, "in_point_ms":90000, "out_point_ms":1}),
        serde_json::json!({"source_type":"rtsp", "uri":"rtsp://example.test/live", "timeout_us":500_000}),
    ] {
        let source: Source = serde_json::from_value(live).unwrap();
        let mut original = item();
        original.source = Some(source.clone());
        let tracks = original.tracks.as_mut().unwrap();
        tracks.video.as_mut().unwrap().source = Some(source.clone());
        tracks.audio.as_mut().unwrap().source = Some(source);
        // The subtitle is a finite sidecar and must still advance independently.
        let requested = original.start + Duration::seconds(12);
        let result = resolve_at(&original, requested).unwrap();
        let json = serde_json::to_value(result).unwrap();
        assert_eq!(json["start"], serde_json::to_value(requested).unwrap());
        assert_eq!(
            json["finish"],
            serde_json::to_value(original.finish).unwrap()
        );
        for source in [
            &json["source"],
            &json["tracks"]["video"]["source"],
            &json["tracks"]["audio"]["source"],
        ] {
            assert!(source.get("in_point_ms").is_none());
            assert!(source.get("out_point_ms").is_none());
        }
        assert_eq!(json["tracks"]["audio"]["stream_index"], 2);
        assert_eq!(json["tracks"]["subtitle"]["source"]["in_point_ms"], 22000);
    }
}

#[test]
fn remote_source_wire_preserves_explicit_liveness_and_omits_absent_options() {
    for live in [None, Some(false), Some(true)] {
        let mut json =
            serde_json::json!({"source_type":"http", "uri":"https://example.test/media"});
        if let Some(live) = live {
            json["is_live"] = live.into();
        }
        let source: Source = serde_json::from_value(json.clone()).unwrap();
        assert_eq!(serde_json::to_value(source).unwrap(), json);
    }
    for timeout in [None, Some(500_000)] {
        let mut json = serde_json::json!({"source_type":"rtsp", "uri":"rtsp://example.test/live"});
        if let Some(timeout) = timeout {
            json["timeout_us"] = timeout.into();
        }
        let source: Source = serde_json::from_value(json.clone()).unwrap();
        assert_eq!(serde_json::to_value(source).unwrap(), json);
    }
}

#[test]
fn explicitly_finite_http_callback_advances_in_point_and_keeps_out_point() {
    let mut original = item();
    original.source = Some(
        serde_json::from_value(serde_json::json!({
            "source_type":"http", "uri":"https://example.test/finite", "is_live":false,
            "in_point_ms":10000, "out_point_ms":90000
        }))
        .unwrap(),
    );
    let resolved = resolve_at(&original, original.start + Duration::seconds(12)).unwrap();
    let source = serde_json::to_value(resolved.source.unwrap()).unwrap();
    assert_eq!(source["in_point_ms"], 22000);
    assert_eq!(source["out_point_ms"], 90000);
    assert_eq!(source["is_live"], false);
}
