use super::*;
use ersatzrs_domain::ChannelWatermark;
use serde_json::json;

fn selected() -> ResolvedPlaybackWatermark {
    ResolvedPlaybackWatermark {
        watermark: ChannelWatermark {
            mode: ChannelWatermarkMode::Permanent,
            size: WatermarkSize::Scaled,
            width_percent: 12.5,
            horizontal_margin_percent: 2.5,
            vertical_margin_percent: 3.5,
            opacity: 75,
            ..Default::default()
        },
        image_path: "synthetic-watermark.png".into(),
    }
}

#[test]
fn permanent_watermark_wire_preserves_percentages_and_omits_source_seeks() {
    let projected = project(&[selected()]).unwrap().unwrap();
    let value = serde_json::to_value(projected).unwrap();
    assert_eq!(
        value,
        json!({
            "source": {"source_type":"local", "path":"synthetic-watermark.png"},
            "stream_index":0, "location":"bottom_right", "width_percent":12.5,
            "horizontal_margin_percent":2.5, "vertical_margin_percent":3.5,
            "opacity_percent":75.0,
        })
    );
    let decoded: protocol::Watermark = serde_json::from_value(value.clone()).unwrap();
    assert_eq!(serde_json::to_value(decoded).unwrap(), value);
}

#[test]
fn every_anchor_matches_next_wire_names() {
    for (location, expected) in [
        (WatermarkLocation::TopLeft, "top_left"),
        (WatermarkLocation::TopMiddle, "top_center"),
        (WatermarkLocation::TopRight, "top_right"),
        (WatermarkLocation::LeftMiddle, "center_left"),
        (WatermarkLocation::MiddleCenter, "center"),
        (WatermarkLocation::RightMiddle, "center_right"),
        (WatermarkLocation::BottomLeft, "bottom_left"),
        (WatermarkLocation::BottomMiddle, "bottom_center"),
        (WatermarkLocation::BottomRight, "bottom_right"),
    ] {
        let mut selection = selected();
        selection.watermark.location = location;
        let projected = project(&[selection]).unwrap().unwrap();
        assert_eq!(
            serde_json::to_value(projected).unwrap()["location"],
            expected
        );
    }
}

#[test]
fn actual_size_omits_width_and_valid_percentage_endpoints_are_preserved() {
    let mut selection = selected();
    selection.watermark.size = WatermarkSize::ActualSize;
    selection.watermark.width_percent = f64::NAN; // Inactive field is not projected.
    let projected = project(&[selection]).unwrap().unwrap();
    assert!(serde_json::to_value(projected)
        .unwrap()
        .get("width_percent")
        .is_none());
    for (percent, opacity) in [(0.0, 0), (100.0, 100)] {
        let mut selection = selected();
        selection.watermark.width_percent = percent;
        selection.watermark.horizontal_margin_percent = percent;
        selection.watermark.vertical_margin_percent = percent;
        selection.watermark.opacity = opacity;
        let projected = project(&[selection]).unwrap().unwrap();
        assert_eq!(projected.width_percent, Some(percent));
        assert_eq!(projected.opacity_percent, Some(percent));
    }
}

#[test]
fn invalid_active_percentages_and_empty_paths_fail_redacted() {
    for invalid in [-0.1, 100.1, f64::NAN, f64::INFINITY, f64::NEG_INFINITY] {
        for field in 0..3 {
            let mut selection = selected();
            match field {
                0 => selection.watermark.width_percent = invalid,
                1 => selection.watermark.horizontal_margin_percent = invalid,
                _ => selection.watermark.vertical_margin_percent = invalid,
            }
            let error = project(&[selection]).err().unwrap();
            assert_eq!(error.to_string(), unavailable().to_string());
        }
    }
    for opacity in [-1, 101] {
        let mut selection = selected();
        selection.watermark.opacity = opacity;
        assert!(project(&[selection]).is_err());
    }
    let mut selection = selected();
    selection.image_path = " ".into();
    assert!(project(&[selection]).is_err());
}

#[test]
fn absent_and_disabled_selections_are_empty_but_unsupported_active_modes_fail() {
    assert!(project(&[]).unwrap().is_none());
    let mut disabled = selected();
    disabled.watermark.mode = ChannelWatermarkMode::None;
    assert!(project(&[disabled.clone()]).unwrap().is_none());
    assert!(project(&[disabled, selected()]).unwrap().is_some());
    assert!(project(&[selected(), selected()]).is_err());
    let mut selection = selected();
    selection.watermark.mode = ChannelWatermarkMode::OpacityExpression;
    assert!(project(&[selection]).is_err());
}

#[test]
fn old_playout_json_omits_watermark_and_http_watermark_wire_remains_supported() {
    let old = json!({"id":"synthetic-item", "start":"2026-09-23T12:00:00Z", "finish":"2026-09-23T12:01:00Z"});
    let item: protocol::PlayoutItem = serde_json::from_value(old.clone()).unwrap();
    assert!(item.watermark.is_none());
    assert_eq!(serde_json::to_value(item).unwrap(), old);
    let mut new = old;
    new["watermark"] = json!({"source":{"source_type":"http", "uri":"https://example.test/logo.png"},"location":"center"});
    let item: protocol::PlayoutItem = serde_json::from_value(new).unwrap();
    let watermark = item.watermark.unwrap();
    assert!(matches!(watermark.source, protocol::Source::Http { .. }));
    assert!(watermark.width_percent.is_none());
    assert!(watermark.opacity_percent.is_none());
}

#[test]
fn intermittent_timing_uses_wall_clock_and_preserves_next_defaults() {
    let mut selection = selected();
    selection.watermark.mode = ChannelWatermarkMode::Intermittent;
    selection.watermark.frequency_minutes = 5;
    selection.watermark.duration_seconds = 30;
    let projected = project(&[selection]).unwrap().unwrap();
    let value = serde_json::to_value(projected).unwrap();
    assert_eq!(
        value["timing"],
        json!({
            "timing_type": "periodic", "clock": "wall",
            "frequency_ms": 300_000, "hold_ms": 30_000
        })
    );
    let decoded: protocol::Watermark = serde_json::from_value(value.clone()).unwrap();
    assert_eq!(serde_json::to_value(decoded).unwrap(), value);
}

#[test]
fn intermittent_timing_rejects_unintended_permanent_output_and_widens_units() {
    for (minutes, seconds) in [(-1, 10), (0, 10), (1, -1), (1, 0), (1, 59), (1, 60)] {
        let mut selection = selected();
        selection.watermark.mode = ChannelWatermarkMode::Intermittent;
        selection.watermark.frequency_minutes = minutes;
        selection.watermark.duration_seconds = seconds;
        assert_eq!(
            project(&[selection]).err().unwrap().to_string(),
            unavailable().to_string()
        );
    }
    for (minutes, seconds) in [(1, 1), (1, 58), (i32::MAX, i32::MAX)] {
        let mut selection = selected();
        selection.watermark.mode = ChannelWatermarkMode::Intermittent;
        selection.watermark.frequency_minutes = minutes;
        selection.watermark.duration_seconds = seconds;
        let value = serde_json::to_value(project(&[selection]).unwrap().unwrap()).unwrap();
        assert_eq!(
            value["timing"]["frequency_ms"],
            u64::try_from(minutes).unwrap() * 60_000
        );
        assert_eq!(
            value["timing"]["hold_ms"],
            u64::try_from(seconds).unwrap() * 1_000
        );
    }
    let mut permanent = selected();
    permanent.watermark.frequency_minutes = -1;
    permanent.watermark.duration_seconds = -1;
    assert!(
        serde_json::to_value(project(&[permanent]).unwrap().unwrap())
            .unwrap()
            .get("timing")
            .is_none()
    );
}
