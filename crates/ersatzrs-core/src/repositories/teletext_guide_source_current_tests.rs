use super::*;
use crate::{
    programme::ProgrammeSegment,
    xmltv::{XmltvProgramme, XmltvProgrammeContent},
};
use chrono::{Duration, TimeZone as _};

fn fence(at: DateTime<Utc>) -> TeletextGuideSourceFence {
    let (start, finish) = ProgrammeClock::named("Europe/Amsterdam")
        .unwrap()
        .calendar_day_bounds(at, NonZeroU8::new(3).unwrap())
        .unwrap();
    TeletextGuideSourceFence {
        version: 1,
        context_digest_version: 2,
        context_digest: [7; 32],
        channel_ids: vec![1],
        captured_at: at,
        start,
        finish,
        locale: "nl".to_owned(),
        timezone: "Europe/Amsterdam".to_owned(),
    }
}

fn programme(at: DateTime<Utc>) -> ProgrammeOccurrence {
    ProgrammeOccurrence {
        playout_id: 1,
        first_item_id: 1,
        programme: XmltvProgramme {
            channel_number: "1".to_owned(),
            start: at - Duration::minutes(10),
            stop: at + Duration::minutes(20),
            title: "Synthetic guide".to_owned(),
            custom_title: None,
            content: XmltvProgrammeContent::default(),
        },
        segments: vec![
            ProgrammeSegment {
                item_id: 1,
                media_item_id: Some(1),
                start: at - Duration::minutes(10),
                finish: at + Duration::minutes(5),
                in_point: Duration::zero(),
                out_point: Duration::minutes(15),
            },
            ProgrammeSegment {
                item_id: 2,
                media_item_id: Some(1),
                start: at + Duration::minutes(10),
                finish: at + Duration::minutes(20),
                in_point: Duration::minutes(15),
                out_point: Duration::minutes(25),
            },
        ],
    }
}

#[test]
fn current_guide_uses_original_segment_boundary_without_reload_renewal() {
    let at = Utc.with_ymd_and_hms(2026, 3, 28, 12, 0, 0).unwrap();
    let source = fence(at);
    let programmes = [programme(at)];
    let first = source.qualify_current(&programmes, at).unwrap();
    assert_eq!(first.valid_until, at + Duration::minutes(5));
    assert_eq!(
        source
            .qualify_current(&programmes, at + Duration::minutes(4))
            .unwrap(),
        first
    );
    assert_eq!(
        source.qualify_current(&programmes, at + Duration::minutes(5)),
        Err(TeletextGuideQualificationError::Expired)
    );
    assert_eq!(
        source.qualify_current(&programmes, at - Duration::nanoseconds(1)),
        Err(TeletextGuideQualificationError::ClockRewind)
    );
}

#[test]
fn current_guide_includes_programme_starts_and_stops_and_excludes_capture_boundary() {
    let at = Utc.with_ymd_and_hms(2026, 3, 28, 12, 0, 0).unwrap();
    let source = fence(at);
    let mut item = programme(at);
    item.segments.clear();
    item.programme.start = at;
    assert_eq!(
        source.qualify_current([&item], at).unwrap().valid_until,
        item.programme.stop
    );
    item.programme.start = at + Duration::minutes(1);
    assert_eq!(
        source.qualify_current([&item], at).unwrap().valid_until,
        item.programme.start
    );
}

#[test]
fn current_guide_empty_lists_end_at_named_midnight_across_both_dst_transitions() {
    for (month, day, expected_hour) in [(3, 29, 22), (10, 25, 23)] {
        let at = Utc.with_ymd_and_hms(2026, month, day, 0, 0, 0).unwrap();
        let source = fence(at);
        let until = Utc
            .with_ymd_and_hms(2026, month, day, expected_hour, 0, 0)
            .unwrap();
        let qualification = source.qualify_current(std::iter::empty(), at).unwrap();
        assert_eq!(qualification.valid_until, until);
        assert_eq!(
            qualification.validate_current(until),
            Err(TeletextGuideQualificationError::Expired)
        );
    }
}

#[test]
fn current_guide_requires_raw_context_version_and_valid_finite_boundary() {
    let at = Utc.with_ymd_and_hms(2026, 3, 28, 12, 0, 0).unwrap();
    let mut source = fence(at);
    source.context_digest_version = 1;
    assert_eq!(
        source.qualify_current(std::iter::empty(), at),
        Err(TeletextGuideQualificationError::ContextVersion)
    );
    source.context_digest_version = 2;
    for valid_until in [at, at + Duration::days(2)] {
        let qualification = TeletextGuideGenerationQualification {
            source: source.clone(),
            valid_until,
        };
        assert_eq!(
            qualification.validate_current(at),
            Err(TeletextGuideQualificationError::Boundary)
        );
    }
    source.channel_ids.clear();
    source.validate().unwrap(); // Historical empty/static context remains valid.
    assert_eq!(
        source.qualify_current(std::iter::empty(), at),
        Err(TeletextGuideQualificationError::Source(
            TeletextGuideSourceFenceError::Channels
        ))
    );
}
