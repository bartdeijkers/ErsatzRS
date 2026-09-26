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
        image: crate::playback::PlaybackWatermarkImage::Local("synthetic-watermark.png".into()),
    }
}

#[test]
fn channel_logo_url_projects_an_http_source_without_media_seeks() {
    let mut selected = selected();
    let uri = "https://example.test/logo%20image.png?revision=2";
    selected.image = PlaybackWatermarkImage::Http(uri.to_owned());
    let projected = project(&[selected]).unwrap().remove(0);
    let source = serde_json::to_value(projected.source).unwrap();
    assert_eq!(source["source_type"], "http");
    assert_eq!(source["uri"], uri);
    assert!(source.get("path").is_none());
    assert!(source.get("in_point_ms").is_none());
    assert!(source.get("out_point_ms").is_none());
}

#[test]
fn permanent_watermark_wire_preserves_percentages_and_omits_source_seeks() {
    let projected = project(&[selected()]).unwrap().remove(0);
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
        let projected = project(&[selection]).unwrap().remove(0);
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
    let projected = project(&[selection]).unwrap().remove(0);
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
        let projected = project(&[selection]).unwrap().remove(0);
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
    selection.image = crate::playback::PlaybackWatermarkImage::Local(" ".into());
    assert!(project(&[selection]).is_err());
}

#[test]
fn absent_and_disabled_selections_are_empty_but_unsupported_active_modes_fail() {
    assert!(project(&[]).unwrap().is_empty());
    let mut disabled = selected();
    disabled.watermark.mode = ChannelWatermarkMode::None;
    assert!(project(&[disabled.clone()]).unwrap().is_empty());
    assert_eq!(project(&[disabled, selected()]).unwrap().len(), 1);
    assert_eq!(project(&[selected(), selected()]).unwrap().len(), 2);
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
    let projected = project(&[selection]).unwrap().remove(0);
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
        let value = serde_json::to_value(project(&[selection]).unwrap().remove(0)).unwrap();
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
        serde_json::to_value(project(&[permanent]).unwrap().remove(0))
            .unwrap()
            .get("timing")
            .is_none()
    );
}

#[test]
fn source_content_placement_preserves_every_anchor_and_legacy_omission() {
    for location in [
        WatermarkLocation::TopLeft,
        WatermarkLocation::TopMiddle,
        WatermarkLocation::TopRight,
        WatermarkLocation::LeftMiddle,
        WatermarkLocation::MiddleCenter,
        WatermarkLocation::RightMiddle,
        WatermarkLocation::BottomLeft,
        WatermarkLocation::BottomMiddle,
        WatermarkLocation::BottomRight,
    ] {
        for within in [false, true] {
            let mut selection = selected();
            selection.watermark.location = location;
            selection.watermark.place_within_source_content = within;
            let projected = project(&[selection]).unwrap().remove(0);
            assert_eq!(projected.within_source_content, within.then_some(true));
            let wire = serde_json::to_value(projected).unwrap();
            assert_eq!(
                wire.get("within_source_content"),
                within.then_some(&json!(true))
            );
            let restored: protocol::Watermark = serde_json::from_value(wire.clone()).unwrap();
            assert_eq!(serde_json::to_value(restored).unwrap(), wire);
        }
    }
    let explicit_false = json!({"source":{"source_type":"local","path":"logo.png"},
        "location":"top_left","within_source_content":false});
    let restored: protocol::Watermark = serde_json::from_value(explicit_false.clone()).unwrap();
    assert_eq!(restored.within_source_content, Some(false));
    assert_eq!(serde_json::to_value(restored).unwrap(), explicit_false);
}

#[test]
fn channel_logo_watermark_projects_the_resolved_artwork_path() {
    let mut selection = selected();
    selection.watermark.image_source = ersatzrs_domain::ChannelWatermarkImageSource::ChannelLogo;
    selection.watermark.image = None;
    selection.image =
        crate::playback::PlaybackWatermarkImage::Local("synthetic/iptv/logos/channel.png".into());
    let projected = project(&[selection]).unwrap().remove(0);
    assert_eq!(
        serde_json::to_value(projected).unwrap()["source"],
        json!({"source_type": "local", "path": "synthetic/iptv/logos/channel.png"})
    );
}

#[test]
fn multiple_layers_are_stably_ordered_and_preserve_independent_timing() {
    let mut upper = selected();
    upper.watermark.z_index = 20;
    upper.watermark.location = WatermarkLocation::TopLeft;
    let mut lower = selected();
    lower.watermark.z_index = -2;
    lower.watermark.mode = ChannelWatermarkMode::Intermittent;
    lower.watermark.frequency_minutes = 1;
    lower.watermark.duration_seconds = 10;
    lower.watermark.location = WatermarkLocation::BottomLeft;
    let mut equal = upper.clone();
    equal.watermark.location = WatermarkLocation::TopRight;
    let layers = project(&[upper, lower, equal]).unwrap();
    let wire = serde_json::to_value(layers).unwrap();
    assert_eq!(wire[0]["location"], "bottom_left");
    assert_eq!(wire[1]["location"], "top_left");
    assert_eq!(wire[2]["location"], "top_right");
    assert_eq!(wire[0]["timing"]["hold_ms"], 10_000);
    assert!(wire[1].get("timing").is_none());
    assert!(wire[2].get("timing").is_none());
}

#[test]
fn unsupported_layer_does_not_silently_drop_other_overlays() {
    let mut invalid = selected();
    invalid.watermark.mode = ChannelWatermarkMode::OpacityExpression;
    assert!(project(&[selected(), invalid]).is_err());
}
