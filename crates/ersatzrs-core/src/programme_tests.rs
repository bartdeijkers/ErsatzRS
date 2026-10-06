use super::*;
use crate::xmltv::XmltvProgrammeContent;

fn at(seconds: i64) -> DateTime<Utc> {
    "2026-10-05T20:00:00Z".parse::<DateTime<Utc>>().unwrap() + Duration::seconds(seconds)
}

fn row(id: i64, group: i32, start: i64, finish: i64, filler: FillerKind) -> ProgrammeGuideItem {
    ProgrammeGuideItem {
        playout_id: 1,
        item_id: id,
        media_item_id: Some(i64::from(group)),
        schedule_kind: PlayoutScheduleKind::Classic,
        in_point: Duration::zero(),
        out_point: Duration::seconds(finish - start),
        guide: XmltvGuideItem {
            channel_number: "1".into(),
            start: at(start),
            finish: at(finish),
            guide_start: None,
            guide_finish: None,
            guide_group: group,
            filler_kind: filler,
            custom_title: None,
            title: format!("Programme {group}"),
            content: XmltvProgrammeContent::default(),
        },
    }
}

fn split_film() -> Vec<ProgrammeGuideItem> {
    let mut resume = row(4, 1, 65, 115, FillerKind::None);
    resume.in_point = Duration::seconds(50);
    resume.out_point = Duration::seconds(100);
    vec![
        row(1, 1, 0, 10, FillerKind::PreRoll),
        row(2, 1, 10, 60, FillerKind::None),
        row(3, 1, 60, 65, FillerKind::MidRoll),
        resume,
        row(5, 2, 115, 120, FillerKind::PostRoll),
        row(6, 2, 120, 125, FillerKind::PreRoll),
        row(7, 2, 125, 175, FillerKind::None),
    ]
}

#[test]
fn classic_groups_split_films_and_filler_like_upstream() {
    let rows = split_film();
    let programmes = project_occurrences(&rows, XmltvBlockBehavior::SplitTimeEvenly);
    assert_eq!(programmes.len(), 2);
    assert_eq!(programmes[0].first_item_id, 1);
    assert_eq!(programmes[0].programme.start, at(0));
    assert_eq!(programmes[0].programme.stop, at(120));
    assert_eq!(programmes[0].programme.title, "Programme 1");
    assert_eq!(
        programmes[0]
            .segments
            .iter()
            .map(|segment| segment.item_id)
            .collect::<Vec<_>>(),
        [2, 4]
    );
    assert_eq!(programmes[1].programme.start, at(120));
    // The first item's title remains the guide override, never a filler title.
    assert_eq!(programmes[1].programme.title, "Programme 2");
}

#[test]
fn continuing_requires_a_real_resume_and_half_open_boundary() {
    let programmes = project_occurrences(&split_film(), XmltvBlockBehavior::UseActualTimes);
    for (instant, current_id, continuing_id, next_id) in [
        (0, Some(1), None, Some(6)),
        (10, Some(1), None, Some(6)),
        (60, Some(1), Some(1), Some(6)),
        (64, Some(1), Some(1), Some(6)),
        (65, Some(1), None, Some(6)),
        (115, Some(1), None, Some(6)),
        (120, Some(6), None, None),
        (175, None, None, None),
    ] {
        let selection = select_programmes(&programmes, at(instant), at(200), 8);
        assert_eq!(
            selection.current.map(|item| item.first_item_id),
            current_id,
            "current at {instant}"
        );
        assert_eq!(
            selection.continuing.map(|item| item.first_item_id),
            continuing_id,
            "continuing at {instant}"
        );
        assert_eq!(
            selection.next.map(|item| item.first_item_id),
            next_id,
            "next at {instant}"
        );
    }
    let mut rows = split_film();
    // An unrelated film with the same display title is never a continuation.
    rows[3].media_item_id = Some(99);
    let programmes = project_occurrences(&rows, XmltvBlockBehavior::UseActualTimes);
    assert!(select_programmes(&programmes, at(62), at(200), 8)
        .continuing
        .is_none());
}

#[test]
fn block_counts_proven_fragments_once_but_keeps_repeated_films() {
    let mut rows = split_film();
    rows.retain(|row| row.item_id != 6);
    for row in &mut rows {
        row.schedule_kind = PlayoutScheduleKind::Block;
        row.guide.guide_start = Some(at(0));
        row.guide.guide_finish = Some(at(200));
        row.guide.guide_group = 1;
    }
    rows.last_mut().unwrap().media_item_id = Some(1);
    for behavior in [
        XmltvBlockBehavior::SplitTimeEvenly,
        XmltvBlockBehavior::UseActualTimes,
    ] {
        let programmes = project_occurrences(&rows, behavior);
        assert_eq!(programmes.len(), 2, "{behavior:?}");
        assert_eq!(programmes[0].segments.len(), 2);
        assert_eq!(programmes[1].segments.len(), 1);
        assert_eq!(programmes[1].segments[0].in_point, Duration::zero());
        if behavior == XmltvBlockBehavior::SplitTimeEvenly {
            assert_eq!(programmes[0].programme.stop, at(100));
            assert_eq!(programmes[1].programme.start, at(100));
        } else {
            assert_eq!(programmes[0].programme.stop, at(115));
            assert_eq!(programmes[1].programme.start, at(125));
        }
    }
}

#[test]
fn playouts_and_channels_cannot_merge_occurrences() {
    let mut rows = split_film();
    let mut other = row(8, 1, 0, 100, FillerKind::None);
    other.playout_id = 2;
    other.guide.channel_number = "2".into();
    rows.push(other);
    let programmes = project_occurrences(&rows, XmltvBlockBehavior::SplitTimeEvenly);
    assert_eq!(programmes.len(), 3);
    assert_eq!(programmes[2].programme.channel_number, "2");
    assert_eq!(programmes[2].playout_id, 2);
}

#[test]
fn overlapping_programmes_suppress_roles_instead_of_guessing() {
    let rows = [
        row(1, 1, 0, 100, FillerKind::None),
        row(2, 2, 50, 150, FillerKind::None),
    ];
    let programmes = project_occurrences(&rows, XmltvBlockBehavior::UseActualTimes);
    let selection = select_programmes(&programmes, at(60), at(200), 8);
    assert!(selection.ambiguous);
    assert!(selection.current.is_none());
    assert!(selection.next.is_none());
    assert!(selection.later_today.is_empty());
}

#[test]
fn later_today_excludes_local_midnight_and_is_bounded() {
    let rows = (0..50)
        .map(|index| {
            row(
                index + 1,
                i32::try_from(index).unwrap(),
                index * 10,
                index * 10 + 10,
                FillerKind::None,
            )
        })
        .collect::<Vec<_>>();
    let programmes = project_occurrences(&rows, XmltvBlockBehavior::UseActualTimes);
    let selection = select_programmes(&programmes, at(0), at(30), 32);
    assert_eq!(selection.later_today.len(), 2);
    assert_eq!(selection.later_today[1].programme.start, at(20));
    assert_eq!(
        select_programmes(&programmes, at(0), at(600), usize::MAX)
            .later_today
            .len(),
        32
    );
    assert!(select_programmes(&[], at(0), at(600), 8).next.is_none());
}

#[test]
fn historical_local_days_resolve_each_dst_boundary_independently() {
    let clock = ProgrammeClock::named("Europe/Amsterdam").unwrap();
    for (now, start, finish, hours) in [
        (
            "2026-03-29T10:00:00Z",
            "2026-03-28T23:00:00Z",
            "2026-03-29T22:00:00Z",
            23,
        ),
        (
            "2026-10-25T10:00:00Z",
            "2026-10-24T22:00:00Z",
            "2026-10-25T23:00:00Z",
            25,
        ),
        (
            "2026-10-05T22:30:00Z",
            "2026-10-05T22:00:00Z",
            "2026-10-06T22:00:00Z",
            24,
        ),
    ] {
        let parse = |value: &str| value.parse::<DateTime<Utc>>().unwrap();
        let bounds = clock.day_bounds(parse(now)).unwrap();
        assert_eq!(bounds, (parse(start), parse(finish)));
        assert_eq!((bounds.1 - bounds.0).num_hours(), hours);
    }
    assert_eq!(
        clock
            .local_time("2026-10-25T00:30:00Z".parse().unwrap())
            .unwrap(),
        "2026-10-25T02:30:00+02:00"
    );
    assert_eq!(
        clock
            .local_time("2026-10-25T01:30:00Z".parse().unwrap())
            .unwrap(),
        "2026-10-25T02:30:00+01:00"
    );
    let clock = ProgrammeClock::named("America/Sao_Paulo").unwrap();
    let bounds = clock
        .day_bounds("2015-10-18T12:00:00Z".parse().unwrap())
        .unwrap();
    assert_eq!(
        bounds.0,
        "2015-10-18T03:00:00Z".parse::<DateTime<Utc>>().unwrap()
    );
}
