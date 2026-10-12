use super::{context_digest, GuideSnapshotDigestError, GuideSnapshotDigestLimits, Writer};
use crate::{
    xmltv_read::{GuideExpansionUsage, GuideReadUsage},
    xmltv_snapshot::{
        CapturedGuideChannel, GuideSnapshot, GuideSnapshotComparison, GuideSnapshotPathAssociation,
        GuideSnapshotSource,
    },
};
use chrono::{Duration, TimeZone as _, Utc};
use ersatzrs_core::{
    programme::{ProgrammeOccurrence, ProgrammeSegment},
    repositories::ArtworkReference,
    xmltv::{
        IconSizeHint, XmltvBlockBehavior, XmltvCredit, XmltvItemKind, XmltvProgramme,
        XmltvProgrammeContent,
    },
};
use ersatzrs_domain::{ArtworkKind, ChannelPlayoutSource, PlayoutScheduleKind};
use sha2::{Digest as _, Sha256};

type Change = fn(&mut GuideSnapshot);

fn limits() -> GuideSnapshotDigestLimits {
    GuideSnapshotDigestLimits {
        max_fields: 10_000,
        max_bytes: 1024 * 1024,
    }
}
fn empty() -> GuideSnapshot {
    GuideSnapshot {
        channels: Vec::new(),
        page_language: None,
        raw_block_behavior: None,
        block_behavior: XmltvBlockBehavior::SplitTimeEvenly,
        start: Utc.timestamp_opt(0, 0).unwrap(),
        finish: Utc.timestamp_opt(1, 0).unwrap(),
        source_usage: GuideReadUsage::default(),
        expansion_usage: GuideExpansionUsage::default(),
    }
}
fn fixture() -> GuideSnapshot {
    let mut snapshot = empty();
    let start = snapshot.start;
    let finish = snapshot.finish;
    snapshot.channels.push(CapturedGuideChannel {
        id: 1,
        number: "1".into(),
        name: "Synthetic guide".into(),
        playout_source: ChannelPlayoutSource::Generated,
        source_channel_id: 1,
        playout_offset: Duration::zero(),
        sources: vec![GuideSnapshotSource::Sql {
            playout_id: 10,
            schedule_kind: PlayoutScheduleKind::Classic,
            raw_rows: 1,
        }],
        path_associations: vec![GuideSnapshotPathAssociation {
            playout_id: 10,
            configured_schedule_file: None,
            accepted_schedule_file: None,
        }],
        occurrences: vec![ProgrammeOccurrence {
            playout_id: 10,
            first_item_id: 20,
            programme: XmltvProgramme {
                channel_number: "1".into(),
                start,
                stop: finish,
                custom_title: None,
                title: "Synthetic title".into(),
                content: XmltvProgrammeContent::default(),
            },
            segments: vec![ProgrammeSegment {
                item_id: 20,
                media_item_id: Some(30),
                start,
                finish,
                in_point: Duration::zero(),
                out_point: Duration::seconds(1),
            }],
        }],
    });
    snapshot
}
fn content(snapshot: &mut GuideSnapshot) -> &mut XmltvProgrammeContent {
    &mut snapshot.channels[0].occurrences[0].programme.content
}
fn artwork() -> ArtworkReference {
    ArtworkReference {
        id: 5,
        kind: ArtworkKind::Poster,
        path: "artwork/synthetic.png".into(),
    }
}
fn assert_changes(mutators: &[Change]) {
    let original = fixture();
    let expected = original.context_digest(limits()).unwrap();
    for (index, change) in mutators.iter().enumerate() {
        let mut mutant = fixture();
        change(&mut mutant);
        assert_ne!(
            original.compare_context(&mutant),
            GuideSnapshotComparison::Unchanged,
            "case {index}"
        );
        assert_ne!(
            mutant.context_digest(limits()).unwrap(),
            expected,
            "case {index}"
        );
    }
}

#[test]
fn empty_context_has_literal_versioned_framing_and_inclusive_limits() {
    // Independent Python struct.pack/hashlib encoding of the documented eight
    // frames: domain, version, two instants, two absent options, enum, sequence.
    let expected = [
        0x16, 0x34, 0x0a, 0x45, 0xdc, 0x13, 0x2a, 0x36, 0x2b, 0x20, 0x7b, 0xde, 0x0b, 0x23, 0x52,
        0x87, 0x81, 0x6c, 0x90, 0x86, 0x05, 0xf7, 0x98, 0x72, 0xcc, 0xa2, 0x07, 0x1d, 0x43, 0x35,
        0xf6, 0x88,
    ];
    let exact = GuideSnapshotDigestLimits {
        max_fields: 8,
        max_bytes: 139,
    };
    assert_eq!(empty().context_digest(exact).unwrap(), expected);
    assert_eq!(
        empty().context_digest(GuideSnapshotDigestLimits {
            max_fields: 7,
            ..exact
        }),
        Err(GuideSnapshotDigestError::Limit {
            dimension: "fields",
            count: 8,
            maximum: 7
        })
    );
    assert_eq!(
        empty().context_digest(GuideSnapshotDigestLimits {
            max_bytes: 138,
            ..exact
        }),
        Err(GuideSnapshotDigestError::Limit {
            dimension: "bytes",
            count: 139,
            maximum: 138
        })
    );
    assert!(empty()
        .context_digest(GuideSnapshotDigestLimits {
            max_fields: 0,
            max_bytes: 0
        })
        .is_err());
}

#[test]
fn identical_context_ignores_only_admission_counters() {
    let original = fixture();
    let mut same = fixture();
    same.source_usage = GuideReadUsage {
        records: 999,
        source_bytes: 444,
    };
    same.expansion_usage = GuideExpansionUsage {
        nodes: 1000,
        bytes: 555,
    };
    assert_eq!(
        original.compare_context(&same),
        GuideSnapshotComparison::Unchanged
    );
    assert_eq!(
        original.context_digest(limits()).unwrap(),
        same.context_digest(limits()).unwrap()
    );
}

#[test]
fn global_and_channel_context_fields_are_distinct() {
    assert_changes(&[
        |s| s.start += Duration::nanoseconds(1),
        |s| s.finish += Duration::nanoseconds(1),
        |s| s.page_language = Some(String::new()),
        |s| s.raw_block_behavior = Some(String::new()),
        |s| s.block_behavior = XmltvBlockBehavior::UseActualTimes,
        |s| s.channels[0].id = 2,
        |s| s.channels[0].number = "2".into(),
        |s| s.channels[0].name = "Other synthetic".into(),
        |s| s.channels[0].playout_source = ChannelPlayoutSource::Mirror,
        |s| s.channels[0].source_channel_id = 2,
        |s| s.channels[0].playout_offset = Duration::nanoseconds(-1),
        |s| {
            s.channels[0].sources[0] = GuideSnapshotSource::Sql {
                playout_id: 11,
                schedule_kind: PlayoutScheduleKind::Classic,
                raw_rows: 1,
            };
        },
        |s| {
            s.channels[0].sources[0] = GuideSnapshotSource::Sql {
                playout_id: 10,
                schedule_kind: PlayoutScheduleKind::Block,
                raw_rows: 1,
            };
        },
        |s| {
            s.channels[0].sources[0] = GuideSnapshotSource::Sql {
                playout_id: 10,
                schedule_kind: PlayoutScheduleKind::Classic,
                raw_rows: 2,
            };
        },
        |s| {
            s.channels[0].sources[0] = GuideSnapshotSource::Accepted {
                playout_id: 10,
                revision: 1,
                raw_programs: 1,
            };
        },
        |s| s.channels[0].path_associations[0].playout_id = 11,
        |s| s.channels[0].path_associations[0].configured_schedule_file = Some(String::new()),
        |s| s.channels[0].path_associations[0].accepted_schedule_file = Some(String::new()),
    ]);
}

#[test]
fn full_occurrence_programme_and_segment_fields_are_distinct() {
    assert_changes(&[
        |s| s.channels[0].occurrences[0].playout_id = 11,
        |s| s.channels[0].occurrences[0].first_item_id = 21,
        |s| s.channels[0].occurrences[0].programme.channel_number = "2".into(),
        |s| s.channels[0].occurrences[0].programme.start += Duration::nanoseconds(1),
        |s| s.channels[0].occurrences[0].programme.stop += Duration::nanoseconds(1),
        |s| s.channels[0].occurrences[0].programme.custom_title = Some(String::new()),
        |s| s.channels[0].occurrences[0].programme.title = "Other synthetic".into(),
        |s| s.channels[0].occurrences[0].segments[0].item_id = 21,
        |s| s.channels[0].occurrences[0].segments[0].media_item_id = None,
        |s| s.channels[0].occurrences[0].segments[0].start += Duration::nanoseconds(1),
        |s| s.channels[0].occurrences[0].segments[0].finish += Duration::nanoseconds(1),
        |s| s.channels[0].occurrences[0].segments[0].in_point = Duration::nanoseconds(-1),
        |s| s.channels[0].occurrences[0].segments[0].out_point = Duration::nanoseconds(1),
    ]);
}

#[test]
fn every_metadata_field_family_including_absent_artwork_hints_is_distinct() {
    assert_changes(&[
        |s| content(s).kind = XmltvItemKind::Episode,
        |s| content(s).sub_title = Some(String::new()),
        |s| content(s).description = Some(String::new()),
        |s| content(s).year = Some(0),
        |s| content(s).genres = vec![String::new()],
        |s| content(s).tags = vec![String::new()],
        |s| content(s).artist_genres = vec![String::new()],
        |s| content(s).content_rating = Some(String::new()),
        |s| content(s).icon_artwork = Some(artwork()),
        |s| content(s).icon_size_hint = IconSizeHint::Thumbnail,
        |s| content(s).thumbnail_artwork = Some(artwork()),
        |s| content(s).thumbnail_size_hint = IconSizeHint::Thumbnail,
        |s| content(s).season_number = Some(0),
        |s| content(s).episode_number = Some(0),
        |s| content(s).guids = vec![String::new()],
        |s| content(s).episode_guids = vec![String::new()],
        |s| content(s).track = Some(String::new()),
        |s| content(s).album = Some(String::new()),
        |s| content(s).release_date = Some(String::new()),
        |s| content(s).collection = Some(String::new()),
        |s| {
            content(s).people = vec![XmltvCredit {
                name: String::new(),
                role: String::new(),
            }];
        },
        |s| content(s).original_broadcasters = vec![String::new()],
        |s| content(s).broadcasters = vec![String::new()],
        |s| content(s).artists = vec![String::new()],
        |s| content(s).album_artists = vec![String::new()],
        |s| content(s).comment = Some(String::new()),
        |s| content(s).studios = vec![String::new()],
        |s| content(s).directors = vec![String::new()],
    ]);
}

#[test]
fn nested_artwork_credit_and_accepted_source_values_are_distinct() {
    let mut original = fixture();
    content(&mut original).icon_artwork = Some(artwork());
    content(&mut original).people = vec![XmltvCredit {
        name: "Synthetic person".into(),
        role: "actor".into(),
    }];
    original.channels[0].sources = vec![GuideSnapshotSource::Accepted {
        playout_id: 10,
        revision: 1,
        raw_programs: 1,
    }];
    let mutators: &[Change] = &[
        |s| content(s).icon_artwork.as_mut().unwrap().id = 6,
        |s| content(s).icon_artwork.as_mut().unwrap().kind = ArtworkKind::Logo,
        |s| content(s).icon_artwork.as_mut().unwrap().path = "artwork/other.png".into(),
        |s| content(s).people[0].name = "Other person".into(),
        |s| content(s).people[0].role = "director".into(),
        |s| {
            s.channels[0].sources[0] = GuideSnapshotSource::Accepted {
                playout_id: 11,
                revision: 1,
                raw_programs: 1,
            };
        },
        |s| {
            s.channels[0].sources[0] = GuideSnapshotSource::Accepted {
                playout_id: 10,
                revision: 2,
                raw_programs: 1,
            };
        },
        |s| {
            s.channels[0].sources[0] = GuideSnapshotSource::Accepted {
                playout_id: 10,
                revision: 1,
                raw_programs: 2,
            };
        },
    ];
    for change in mutators {
        let mut mutant = fixture();
        content(&mut mutant).icon_artwork = Some(artwork());
        content(&mut mutant).people = content(&mut original).people.clone();
        mutant.channels[0].sources = original.channels[0].sources.clone();
        change(&mut mutant);
        assert_ne!(
            original.compare_context(&mutant),
            GuideSnapshotComparison::Unchanged
        );
        assert_ne!(
            original.context_digest(limits()).unwrap(),
            mutant.context_digest(limits()).unwrap()
        );
    }
}

#[test]
fn ordered_vectors_and_length_framing_prevent_concatenation_aliases() {
    let mut first = fixture();
    let mut second = fixture();
    content(&mut first).genres = vec!["ab".into(), "c".into()];
    content(&mut second).genres = vec!["a".into(), "bc".into()];
    assert_ne!(
        first.context_digest(limits()).unwrap(),
        second.context_digest(limits()).unwrap()
    );
    content(&mut second).genres = vec!["c".into(), "ab".into()];
    assert_ne!(
        first.context_digest(limits()).unwrap(),
        second.context_digest(limits()).unwrap()
    );
    for source in [&mut first, &mut second] {
        let mut extra = fixture().channels.remove(0);
        extra.id = 2;
        source.channels.push(extra);
    }
    content(&mut second).genres = content(&mut first).genres.clone();
    assert_eq!(
        first.context_digest(limits()).unwrap(),
        second.context_digest(limits()).unwrap()
    );
    second.channels.swap(0, 1);
    assert_ne!(
        first.context_digest(limits()).unwrap(),
        second.context_digest(limits()).unwrap()
    );
}

fn ordered_fixture() -> GuideSnapshot {
    let mut snapshot = fixture();
    let channel = &mut snapshot.channels[0];
    channel.sources.push(GuideSnapshotSource::Accepted {
        playout_id: 11,
        revision: 2,
        raw_programs: 1,
    });
    channel
        .path_associations
        .push(GuideSnapshotPathAssociation {
            playout_id: 11,
            configured_schedule_file: Some("sources/configured.json".into()),
            accepted_schedule_file: Some("sources/accepted.json".into()),
        });
    let mut extra = channel.occurrences[0].clone();
    extra.first_item_id = 21;
    channel.occurrences.push(extra);
    let mut extra = channel.occurrences[0].segments[0].clone();
    extra.item_id = 21;
    channel.occurrences[0].segments.push(extra);
    channel.occurrences[0].programme.content.people = vec![
        XmltvCredit {
            name: "First".into(),
            role: "actor".into(),
        },
        XmltvCredit {
            name: "Second".into(),
            role: "director".into(),
        },
    ];
    snapshot
}

#[test]
fn source_path_occurrence_segment_and_credit_order_is_preserved() {
    let original = ordered_fixture();
    let mutators: &[Change] = &[
        |s| s.channels[0].sources.swap(0, 1),
        |s| s.channels[0].path_associations.swap(0, 1),
        |s| s.channels[0].occurrences.swap(0, 1),
        |s| s.channels[0].occurrences[0].segments.swap(0, 1),
        |s| content(s).people.swap(0, 1),
        |s| {
            s.channels[0].path_associations[1].configured_schedule_file =
                Some("sources/other.json".into());
        },
        |s| {
            s.channels[0].path_associations[1].accepted_schedule_file =
                Some("sources/other.json".into());
        },
    ];
    for change in mutators {
        let mut mutant = ordered_fixture();
        change(&mut mutant);
        assert_ne!(
            original.compare_context(&mutant),
            GuideSnapshotComparison::Unchanged
        );
        assert_ne!(
            original.context_digest(limits()).unwrap(),
            mutant.context_digest(limits()).unwrap()
        );
    }
}

#[test]
fn exact_negative_duration_precision_and_large_offsets_do_not_collapse() {
    let mut first = fixture();
    let mut second = fixture();
    first.channels[0].playout_offset = Duration::nanoseconds(-1);
    second.channels[0].playout_offset = Duration::nanoseconds(-2);
    assert_eq!(
        first.channels[0].playout_offset.num_milliseconds(),
        second.channels[0].playout_offset.num_milliseconds()
    );
    assert_ne!(
        first.context_digest(limits()).unwrap(),
        second.context_digest(limits()).unwrap()
    );
    first.channels[0].playout_offset = Duration::MAX;
    assert!(first.context_digest(limits()).is_ok());
}

#[test]
fn failed_field_admission_never_updates_hash_or_one_counter() {
    let mut writer = Writer {
        hash: Sha256::new(),
        limits: GuideSnapshotDigestLimits {
            max_fields: 1,
            max_bytes: 9,
        },
        fields: 0,
        bytes: 0,
    };
    let before = writer.hash.clone().finalize();
    assert_eq!(
        writer.frame(1, b"private"),
        Err(GuideSnapshotDigestError::Limit {
            dimension: "bytes",
            count: 16,
            maximum: 9
        })
    );
    assert_eq!((writer.fields, writer.bytes), (0, 0));
    assert_eq!(writer.hash.clone().finalize(), before);
    writer.bytes = usize::MAX;
    assert_eq!(
        writer.frame(1, b""),
        Err(GuideSnapshotDigestError::Arithmetic)
    );
    assert_eq!((writer.fields, writer.bytes), (0, usize::MAX));
    assert_eq!(writer.hash.clone().finalize(), before);
    writer.bytes = 0;
    writer.fields = usize::MAX;
    assert_eq!(
        writer.frame(1, b""),
        Err(GuideSnapshotDigestError::Arithmetic)
    );
    assert_eq!(writer.hash.finalize(), before);
    assert_eq!(
        context_digest(&fixture(), limits()).unwrap(),
        fixture().context_digest(limits()).unwrap()
    );
}
