use chrono::{DateTime, Duration, Utc};
use ersatzrs_addon_contract::TeletextPagesRequest;
use ersatzrs_app::teletext_guide::{
    build_request, SuppliedTeletextGuideContext, TeletextGuideChannel, TeletextGuideRequestError,
};
use ersatzrs_core::programme::{ProgrammeOccurrence, ProgrammeSegment};
use ersatzrs_core::xmltv::{XmltvProgramme, XmltvProgrammeContent};

fn instant(value: &str) -> DateTime<Utc> {
    value.parse().unwrap()
}

#[test]
fn public_builder_retains_full_group_and_actual_segments_without_internal_metadata() {
    let occurrences = [ProgrammeOccurrence {
        playout_id: 11,
        first_item_id: 0,
        programme: XmltvProgramme {
            channel_number: "7".to_owned(),
            start: instant("2026-03-28T12:00:00Z"),
            stop: instant("2026-03-28T13:00:00Z"),
            custom_title: None,
            title: "Literal <guide>".to_owned(),
            content: XmltvProgrammeContent {
                sub_title: Some("Original subtitle".to_owned()),
                guids: vec!["excluded-synthetic-internal-identity".to_owned()],
                ..Default::default()
            },
        },
        // The configured guide interval differs from actual content timing.
        segments: vec![ProgrammeSegment {
            item_id: 0,
            media_item_id: None,
            start: instant("2026-03-28T11:40:00Z"),
            finish: instant("2026-03-28T12:05:00Z"),
            in_point: Duration::zero(),
            out_point: Duration::minutes(25),
        }],
    }];
    let channels = [TeletextGuideChannel {
        id: 7,
        number: "7",
        name: "Synthetic channel",
        programmes: &occurrences,
    }];
    let context = SuppliedTeletextGuideContext {
        generation: "epoch",
        locale: "nl-NL",
        supported_locales: &["nl-NL"],
        timezone: "Europe/Amsterdam",
        captured_at: instant("2026-03-28T12:30:00.123456789Z"),
        channels: &channels,
    };
    let bytes = build_request(&context).unwrap();
    let request = TeletextPagesRequest::from_json(&bytes).unwrap();
    assert_eq!(
        (request.window.finish - request.window.start).num_hours(),
        71
    );
    assert_eq!(request.captured_at, context.captured_at);
    let programme = &request.channels[0].programmes[0];
    assert_eq!(programme.identity.first_item_id, 0);
    assert_eq!(programme.title, "Literal <guide>");
    assert_eq!(programme.sub_title.as_deref(), Some("Original subtitle"));
    assert_eq!(
        programme.segments[0].start,
        occurrences[0].segments[0].start
    );
    assert_eq!(programme.segments[0].out_point_milliseconds, 1_500_000);
    assert_eq!(
        request.channels[0].context.current,
        Some(programme.identity)
    );
    assert!(!std::str::from_utf8(&bytes)
        .unwrap()
        .contains("excluded-synthetic-internal-identity"));
}

#[test]
fn public_builder_rejects_noncanonical_locale_without_fallback() {
    let context = SuppliedTeletextGuideContext {
        generation: "epoch",
        locale: "NL-nl",
        supported_locales: &["nl-NL", "en-US"],
        timezone: "Europe/Amsterdam",
        captured_at: instant("2026-03-28T12:30:00Z"),
        channels: &[],
    };
    assert!(matches!(
        build_request(&context),
        Err(TeletextGuideRequestError::UnsupportedLocale)
    ));
}
