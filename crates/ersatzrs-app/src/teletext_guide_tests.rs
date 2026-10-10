use ersatzrs_core::xmltv::{XmltvProgramme, XmltvProgrammeContent};

use super::*;

fn at(seconds: i64) -> DateTime<Utc> {
    "2026-10-05T12:00:00Z".parse::<DateTime<Utc>>().unwrap() + Duration::seconds(seconds)
}

fn occurrence(id: i64, start: i64, stop: i64) -> ProgrammeOccurrence {
    ProgrammeOccurrence {
        playout_id: 1,
        first_item_id: id,
        programme: XmltvProgramme {
            channel_number: "1".to_owned(),
            start: at(start),
            stop: at(stop),
            title: "Café <b>programme</b>".to_owned(),
            custom_title: Some("Custom".to_owned()),
            content: XmltvProgrammeContent {
                sub_title: Some("Episode".to_owned()),
                description: Some("Literal description".to_owned()),
                ..Default::default()
            },
        },
        segments: vec![ProgrammeSegment {
            item_id: id,
            media_item_id: None,
            start: at(start),
            finish: at(stop),
            in_point: Duration::zero(),
            out_point: Duration::seconds(stop - start),
        }],
    }
}

fn channel(programmes: &[ProgrammeOccurrence]) -> TeletextGuideChannel<'_> {
    TeletextGuideChannel {
        id: 1,
        number: "1",
        name: "Guide channel",
        programmes,
    }
}

fn context<'a>(channels: &'a [TeletextGuideChannel<'a>]) -> SuppliedTeletextGuideContext<'a> {
    SuppliedTeletextGuideContext {
        generation: "generation-1",
        locale: "nl",
        supported_locales: &["nl", "en-us"],
        timezone: "Europe/Amsterdam",
        captured_at: at(0),
        channels,
    }
}

fn request(context: &SuppliedTeletextGuideContext<'_>) -> wire::TeletextPagesRequest {
    wire::TeletextPagesRequest::from_json(&build_request(context).unwrap()).unwrap()
}

#[test]
fn teletext_guide_preserves_external_zero_ids_literal_metadata_and_unknown_offsets() {
    let mut programmes = vec![occurrence(0, -10, 50)];
    programmes[0].segments[0].out_point = Duration::zero();
    let channels = [channel(&programmes)];
    let wire = request(&context(&channels));
    let item = &wire.channels[0].programmes[0];
    assert_eq!(item.identity.first_item_id, 0);
    assert_eq!(item.segments[0].item_id, 0);
    assert_eq!(item.segments[0].media_item_id, None);
    assert_eq!(item.segments[0].in_point_milliseconds, 0);
    assert_eq!(item.segments[0].out_point_milliseconds, 0);
    assert_eq!(item.title, "Café <b>programme</b>");
    assert_eq!(item.custom_title.as_deref(), Some("Custom"));
    assert_eq!(item.sub_title.as_deref(), Some("Episode"));
    assert_eq!(item.description.as_deref(), Some("Literal description"));
    assert_eq!(wire.channels[0].context.current, Some(item.identity));
}

#[test]
fn teletext_guide_keeps_preroll_identity_full_group_and_actual_segments() {
    let mut item = occurrence(10, -100_000, 200_000);
    item.segments = vec![ProgrammeSegment {
        item_id: 11,
        media_item_id: Some(9),
        start: at(-110_000),
        finish: at(210_000),
        in_point: Duration::milliseconds(2_005),
        out_point: Duration::milliseconds(3_006),
    }];
    let programmes = [item];
    let channels = [channel(&programmes)];
    let wire = request(&context(&channels));
    let item = &wire.channels[0].programmes[0];
    assert_eq!(item.identity.first_item_id, 10);
    assert_eq!(item.start, at(-100_000));
    assert_eq!(item.stop, at(200_000));
    assert_eq!(item.segments[0].item_id, 11);
    assert_eq!(item.segments[0].start, at(-110_000));
    assert_eq!(item.segments[0].finish, at(210_000));
    assert_eq!(item.segments[0].in_point_milliseconds, 2_005);
    assert_eq!(item.segments[0].out_point_milliseconds, 3_006);
}

#[test]
fn teletext_guide_uses_civil_three_dates_and_exact_host_locale_membership() {
    let channels = [];
    let mut input = context(&channels);
    for (capture, start, finish) in [
        (
            "2026-03-28T12:00:00.123456789Z",
            "2026-03-27T23:00:00Z",
            "2026-03-30T22:00:00Z",
        ),
        (
            "2026-10-24T12:00:00.999999999Z",
            "2026-10-23T22:00:00Z",
            "2026-10-26T23:00:00Z",
        ),
    ] {
        input.captured_at = capture.parse().unwrap();
        let wire = request(&input);
        assert_eq!(wire.captured_at, input.captured_at);
        assert_eq!(wire.window.start, start.parse::<DateTime<Utc>>().unwrap());
        assert_eq!(wire.window.finish, finish.parse::<DateTime<Utc>>().unwrap());
        assert!(wire.channels.is_empty());
        assert_eq!(wire.locale, "nl");
        assert_eq!(wire.timezone, "Europe/Amsterdam");
    }
    for locale in ["NL", " nl", "nl-NL", "unknown", ""] {
        input.locale = locale;
        assert!(matches!(
            build_request(&input),
            Err(TeletextGuideRequestError::UnsupportedLocale)
        ));
    }
    input.locale = "nl";
    input.timezone = "Invalid/GuideTimezone";
    assert!(matches!(
        build_request(&input),
        Err(TeletextGuideRequestError::Timezone)
    ));
    input.timezone = "Europe/Amsterdam";
    input.captured_at = DateTime::<Utc>::MAX_UTC;
    assert!(matches!(
        build_request(&input),
        Err(TeletextGuideRequestError::CalendarWindow)
    ));
}

#[test]
fn teletext_guide_roles_retain_continuation_and_ambiguity_without_guessing() {
    let mut item = occurrence(1, -60, 100);
    item.segments = vec![
        ProgrammeSegment {
            item_id: 2,
            media_item_id: Some(9),
            start: at(-60),
            finish: at(-10),
            in_point: Duration::zero(),
            out_point: Duration::seconds(50),
        },
        ProgrammeSegment {
            item_id: 3,
            media_item_id: Some(9),
            start: at(10),
            finish: at(60),
            in_point: Duration::seconds(50),
            out_point: Duration::seconds(100),
        },
    ];
    let mut programmes = vec![item, occurrence(4, 110, 150)];
    let channels = [channel(&programmes)];
    let wire = request(&context(&channels));
    let roles = &wire.channels[0].context;
    assert_eq!(roles.continuing, roles.current);
    assert_eq!(roles.current.unwrap().first_item_id, 1);
    assert_eq!(roles.next.unwrap().first_item_id, 4);
    assert!(!roles.ambiguous);
    programmes.push(occurrence(5, 120, 160));
    let channels = [channel(&programmes)];
    let wire = request(&context(&channels));
    let roles = &wire.channels[0].context;
    assert!(roles.ambiguous);
    assert_eq!(roles.current.unwrap().first_item_id, 1);
    assert_eq!(roles.continuing, roles.current);
    assert_eq!(roles.next, None);
    assert!(roles.later_today.is_empty());
}

#[test]
fn teletext_guide_role_limit_and_midnight_do_not_shorten_full_guide() {
    let mut programmes: Vec<_> = (0..40)
        .map(|id| occurrence(id, 10 + id * 60, 50 + id * 60))
        .collect();
    // Local midnight is ten hours after capture; it is not Later Today.
    programmes.push(occurrence(40, 36_000, 36_100));
    // Both exact window boundaries are excluded by half-open intersection.
    programmes.push(occurrence(41, -60_000, -50_400));
    programmes.push(occurrence(42, 208_800, 208_900));
    let channels = [channel(&programmes)];
    let wire = request(&context(&channels));
    assert_eq!(wire.channels[0].programmes.len(), 41);
    let roles = &wire.channels[0].context;
    assert_eq!(roles.later_today.len(), 32);
    assert_eq!(roles.next.unwrap().first_item_id, 0);
    assert!(roles.later_today.iter().all(|item| item.first_item_id < 40));
}

#[test]
fn teletext_guide_rejects_identity_collisions_but_preserves_cross_channel_mirrors() {
    let programmes = [occurrence(0, -10, 50)];
    let mut channels = [channel(&programmes), channel(&programmes)];
    assert!(matches!(
        build_request(&context(&channels)),
        Err(TeletextGuideRequestError::ChannelIdentity)
    ));
    channels[1].id = 2;
    assert_eq!(request(&context(&channels)).channels.len(), 2);
    channels[0].id = 0;
    assert!(matches!(
        build_request(&context(&channels)),
        Err(TeletextGuideRequestError::ChannelIdentity)
    ));
    let mut wrong_channel = programmes.clone();
    wrong_channel[0].programme.channel_number = "another-channel".to_owned();
    let channels = [channel(&wrong_channel)];
    assert!(matches!(
        build_request(&context(&channels)),
        Err(TeletextGuideRequestError::OccurrenceIdentity)
    ));
    let repeated = [programmes[0].clone(), programmes[0].clone()];
    let channels = [channel(&repeated)];
    assert!(matches!(
        build_request(&context(&channels)),
        Err(TeletextGuideRequestError::OccurrenceIdentity)
    ));
    for (playout_id, item_id) in [(0, 0), (1, -1)] {
        let mut invalid = programmes.clone();
        invalid[0].playout_id = playout_id;
        invalid[0].first_item_id = item_id;
        let channels = [channel(&invalid)];
        assert!(matches!(
            build_request(&context(&channels)),
            Err(TeletextGuideRequestError::OccurrenceIdentity)
        ));
    }
}

#[test]
fn teletext_guide_rejects_inexact_or_invalid_source_offsets_without_truncation() {
    for offset in [Duration::microseconds(1), Duration::nanoseconds(999_999)] {
        let mut programmes = [occurrence(0, -10, 50)];
        programmes[0].segments[0].in_point = offset;
        let channels = [channel(&programmes)];
        assert!(matches!(
            build_request(&context(&channels)),
            Err(TeletextGuideRequestError::SourceOffsetPrecision)
        ));
    }
    for (in_point, out_point, media) in [
        (Duration::milliseconds(-1), Duration::seconds(10), None),
        (Duration::seconds(20), Duration::seconds(10), None),
        (Duration::zero(), Duration::zero(), Some(1)),
    ] {
        let mut programmes = [occurrence(0, -10, 50)];
        programmes[0].segments[0].in_point = in_point;
        programmes[0].segments[0].out_point = out_point;
        programmes[0].segments[0].media_item_id = media;
        let channels = [channel(&programmes)];
        assert!(matches!(
            build_request(&context(&channels)),
            Err(TeletextGuideRequestError::SourceOffsets)
        ));
    }
    assert_eq!(exact_milliseconds(Duration::MAX).unwrap(), i64::MAX);
}

#[test]
fn teletext_guide_rejects_invalid_selected_intervals_and_segments() {
    let mut programmes = [occurrence(1, -10, 50)];
    programmes[0].programme.stop = programmes[0].programme.start;
    let channels = [channel(&programmes)];
    assert!(matches!(
        build_request(&context(&channels)),
        Err(TeletextGuideRequestError::ProgrammeInterval)
    ));
    let mut programmes = [occurrence(1, -10, 50)];
    let repeated = programmes[0].segments[0].clone();
    programmes[0].segments.push(repeated);
    let channels = [channel(&programmes)];
    assert!(matches!(
        build_request(&context(&channels)),
        Err(TeletextGuideRequestError::SegmentIdentity)
    ));
    programmes[0].segments[1].item_id = 2;
    let channels = [channel(&programmes)];
    assert!(matches!(
        build_request(&context(&channels)),
        Err(TeletextGuideRequestError::SegmentInterval)
    ));
}

#[test]
fn teletext_guide_enforces_counts_before_cloning_and_serialized_bytes_without_partial_output() {
    let programmes: Vec<_> = (0..10_001).map(|id| occurrence(id, -10, 50)).collect();
    let channels = [channel(&programmes)];
    assert!(matches!(
        build_request(&context(&channels)),
        Err(TeletextGuideRequestError::Contract(
            wire::TeletextPagesRequestError::OccurrenceCount
        ))
    ));
    let channels: Vec<_> = (1..=257)
        .map(|id| TeletextGuideChannel {
            id,
            number: "1",
            name: "Guide",
            programmes: &[],
        })
        .collect();
    assert!(matches!(
        build_request(&context(&channels)),
        Err(TeletextGuideRequestError::Contract(
            wire::TeletextPagesRequestError::ChannelCount
        ))
    ));
    let mut programmes = [occurrence(0, -10, 50)];
    programmes[0].programme.title = "\n".repeat(wire::MAX_TELETEXT_PAGES_INPUT_BYTES / 2);
    let channels = [channel(&programmes)];
    let error = build_request(&context(&channels)).unwrap_err();
    assert!(matches!(
        error,
        TeletextGuideRequestError::Contract(wire::TeletextPagesRequestError::InputTooLarge)
    ));
    assert!(!format!("{error:?}").contains("Guide channel"));
}
