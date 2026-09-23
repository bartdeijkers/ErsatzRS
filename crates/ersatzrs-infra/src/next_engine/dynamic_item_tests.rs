use super::super::protocol::{AudioOnlyProbeHint, AudioProbeHint, Track, Tracks};
use super::*;
use chrono::{Duration, TimeZone};

fn local(start: Option<u64>) -> Source {
    Source::Local {
        path: "synthetic/fixture.mkv".into(),
        in_point_ms: start,
        out_point_ms: Some(90_000),
    }
}

fn item() -> PlayoutItem {
    let start = Utc.with_ymd_and_hms(2026, 9, 23, 12, 0, 0).unwrap();
    PlayoutItem {
        watermark: None,
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
        probe_hint: Some(AudioOnlyProbeHint {
            video: [],
            subtitle: [],
            format_name: "mpegts".into(),
            duration_ms: 60_000,
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
