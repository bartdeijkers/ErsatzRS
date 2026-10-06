use super::*;
use ersatzrs_domain::{GraphicsElement, GraphicsElementConfig};

#[tokio::test]
async fn canvas_projection_supports_every_kind_and_issues_scoped_nonprobing_sources() {
    let (pool, _, channel, _) = super::super::schedule::tests::fixture().await;
    let mut media =
        super::super::schedule::bind_media(&pool, super::super::selection::tests::item())
            .await
            .unwrap();
    let resolution = Resolution {
        width: 640,
        height: 360,
        ..Resolution::default()
    };
    let mut item = protocol::PlayoutItem {
        id: "fixture".into(),
        start: media.item.start,
        finish: media.item.finish,
        source: None,
        tracks: None,
        watermark: None,
        graphics: Vec::new(),
    };
    let relay = MediaRelay::new();
    relay.configure("127.0.0.1:18419".parse().unwrap()).unwrap();
    assert!(
        project(&pool, &channel, &media, &resolution, &mut item, &[], &[])
            .await
            .unwrap()
            .is_none()
    );
    for config in [
        GraphicsElementConfig::Image(ersatzrs_domain::ImageGraphicsConfig::default()),
        GraphicsElementConfig::Text(ersatzrs_domain::TextGraphicsConfig::default()),
        GraphicsElementConfig::Script(ersatzrs_domain::ScriptGraphicsConfig::default()),
        GraphicsElementConfig::Motion(ersatzrs_domain::MotionGraphicsConfig::default()),
        GraphicsElementConfig::Subtitle(ersatzrs_domain::SubtitleGraphicsConfig::default()),
    ] {
        let graphics = [ResolvedPlaybackGraphicsElement {
            element: GraphicsElement {
                kind: config.kind(),
                config,
                ..GraphicsElement::default()
            },
            variables: None,
        }];
        let plan = project(
            &pool,
            &channel,
            &media,
            &resolution,
            &mut item,
            &[],
            &graphics,
        )
        .await
        .unwrap()
        .unwrap();
        assert_published_plan(&plan, &relay, item.start);
    }
    media.item.id = 0;
    let graphics = [ResolvedPlaybackGraphicsElement {
        element: GraphicsElement::default(),
        variables: None,
    }];
    let plan = project(
        &pool,
        &channel,
        &media,
        &resolution,
        &mut item,
        &[],
        &graphics,
    )
    .await
    .unwrap()
    .unwrap();
    assert!(plan.publish(&relay, None, item.start).is_err());
    assert!(plan
        .publish(&relay, Some(uuid::Uuid::new_v4()), item.finish)
        .is_err());
}

fn assert_published_plan(plan: &CanvasProjection, relay: &MediaRelay, start: DateTime<Utc>) {
    let stable = serde_json::to_value(plan).unwrap();
    let first = plan.publish(relay, None, start).unwrap();
    let second = plan.publish(relay, None, start).unwrap();
    assert_eq!(
        serde_json::to_value(&first).unwrap(),
        serde_json::to_value(second).unwrap()
    );
    assert_eq!(first.kind, Some(protocol::GraphicsLayerKind::Canvas));
    let protocol::Source::Http {
        uri,
        headers,
        probe_hint: Some(hint),
        in_point_ms,
        reconnect,
        ..
    } = first.source
    else {
        panic!("finite HTTP canvas with probe hint");
    };
    assert_eq!(in_point_ms, None);
    assert_eq!(reconnect, Some(false));
    assert_eq!(hint.format_name.as_deref(), Some("nut"));
    assert_eq!((hint.video[0].width, hint.video[0].height), (640, 360));
    assert_eq!(hint.video[0].codec, "ffv1");
    assert_eq!(hint.video[0].pix_fmt, "bgra");
    let url = url::Url::parse(&uri).unwrap();
    let binding: CanvasBinding = serde_json::from_str(
        &url.query_pairs()
            .find(|(key, _)| key == "binding")
            .unwrap()
            .1,
    )
    .unwrap();
    let ticket = headers
        .iter()
        .find_map(|header| header.strip_prefix("x-ersatzrs-next-canvas: "))
        .unwrap();
    assert!(relay.authorized_canvas(&binding, ticket, start));
    let instance = uuid::Uuid::new_v4();
    let later = plan
        .publish(relay, Some(instance), start + TimeDelta::seconds(10))
        .unwrap();
    let encoded = serde_json::to_value(later).unwrap();
    let url = url::Url::parse(encoded["source"]["uri"].as_str().unwrap()).unwrap();
    let binding: CanvasBinding = serde_json::from_str(
        &url.query_pairs()
            .find(|(key, _)| key == "binding")
            .unwrap()
            .1,
    )
    .unwrap();
    assert_eq!(binding.session, Some(instance));
    assert_eq!(binding.playout_start, Some(start + TimeDelta::seconds(10)));
    assert_eq!(
        stable,
        serde_json::to_value(plan).unwrap(),
        "issuance must not mutate publication identity"
    );
}

#[tokio::test]
async fn plain_watermarks_bypass_canvas_but_mixed_and_expression_layers_do_not() {
    let (pool, _, channel, _) = super::super::schedule::tests::fixture().await;
    let media = super::super::schedule::bind_media(&pool, super::super::selection::tests::item())
        .await
        .unwrap();
    let resolution = Resolution {
        width: 640,
        height: 360,
        ..Default::default()
    };
    let mut item = protocol::PlayoutItem {
        id: "fixture".into(),
        start: media.item.start,
        finish: media.item.finish,
        source: None,
        tracks: None,
        watermark: None,
        graphics: Vec::new(),
    };
    let mut watermarks = plain_watermark_fixture();
    assert!(project(
        &pool,
        &channel,
        &media,
        &resolution,
        &mut item,
        &watermarks,
        &[]
    )
    .await
    .unwrap()
    .is_none());
    assert_eq!(item.graphics.len(), 2);
    assert!(item
        .graphics
        .iter()
        .all(|layer| layer.kind == Some(protocol::GraphicsLayerKind::Media)));
    assert!(matches!(
        item.graphics[0].source,
        protocol::Source::Local { .. }
    ));
    assert_eq!(item.graphics[0].width_percent, None);
    assert!(item.graphics[0].timing.is_none());
    assert!(matches!(
        item.graphics[1].source,
        protocol::Source::Http { .. }
    ));
    assert_eq!(item.graphics[1].width_percent, Some(15.0));
    assert_eq!(item.graphics[1].opacity_percent, Some(80.0));
    assert!(matches!(
        item.graphics[1].timing,
        Some(protocol::WatermarkTiming::Periodic {
            frequency_ms: 300_000,
            hold_ms: 10_000,
            ..
        })
    ));
    let graphics = [ResolvedPlaybackGraphicsElement {
        element: GraphicsElement::default(),
        variables: None,
    }];
    assert!(project(
        &pool,
        &channel,
        &media,
        &resolution,
        &mut item,
        &watermarks,
        &graphics
    )
    .await
    .unwrap()
    .is_some());
    watermarks[0].watermark.mode = ChannelWatermarkMode::OpacityExpression;
    assert!(project(
        &pool,
        &channel,
        &media,
        &resolution,
        &mut item,
        &watermarks,
        &[]
    )
    .await
    .unwrap()
    .is_some());
}

fn plain_watermark_fixture() -> Vec<ResolvedPlaybackWatermark> {
    use crate::playback::PlaybackWatermarkImage;
    use ersatzrs_domain::{ChannelWatermark, WatermarkSize};
    vec![
        ResolvedPlaybackWatermark {
            watermark: ChannelWatermark {
                mode: ChannelWatermarkMode::Intermittent,
                z_index: 2,
                frequency_minutes: 5,
                duration_seconds: 10,
                size: WatermarkSize::Scaled,
                width_percent: 15.0,
                opacity: 80,
                ..Default::default()
            },
            image: PlaybackWatermarkImage::Http("https://example.test/logo.png".into()),
        },
        ResolvedPlaybackWatermark {
            watermark: ChannelWatermark {
                mode: ChannelWatermarkMode::Permanent,
                z_index: 1,
                size: WatermarkSize::ActualSize,
                ..Default::default()
            },
            image: PlaybackWatermarkImage::Local("logo.png".into()),
        },
    ]
}
