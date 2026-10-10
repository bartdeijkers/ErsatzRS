use base64::{engine::general_purpose::STANDARD, Engine as _};
use serde_json::{json, Value};

use super::*;

fn fixture() -> Value {
    serde_json::from_str(include_str!(
        "../../tests/fixtures/ffprobe_teletext_base64.json"
    ))
    .unwrap()
}

fn snapshot(metadata: &Value) -> TeletextProbeSnapshot {
    super::super::parse_ffprobe_json_with_teletext(
        "synthetic.ts",
        &serde_json::to_vec(metadata).unwrap(),
        TeletextProbeEncoding::Ffprobe9Base64,
    )
    .unwrap()
    .1
}

fn streams(metadata: &Value) -> Vec<ProbedTeletextStream> {
    let TeletextProbeSnapshot::Available(streams) = snapshot(metadata) else {
        panic!("expected available auxiliary snapshot");
    };
    streams
}

#[test]
fn fixture_projects_global_indexes_pids_and_reported_memberships() {
    let rows = streams(&fixture());
    assert_eq!(
        rows.iter().map(|row| row.stream_index).collect::<Vec<_>>(),
        [2, 4]
    );
    assert_eq!(rows[0].pid, TeletextProbePid::Available(0x102));
    assert_eq!(rows[1].pid, TeletextProbePid::Available(0x200));
    let expected = TeletextProbePrograms::Available(vec![ProbedTeletextProgram {
        program_id: 1,
        program_number: 1,
        pmt_pid: 4096,
        pcr_pid: 256,
    }]);
    assert!(rows.iter().all(|row| row.programs == expected));
}

#[test]
fn only_confirmed_container_and_named_subtitle_codec_are_projected() {
    let mut metadata = fixture();
    metadata["format"]["format_name"] = json!("matroska,webm");
    assert_eq!(
        snapshot(&metadata),
        TeletextProbeSnapshot::Unavailable(TeletextProbeUnavailable::UnsupportedContainer)
    );
    metadata["format"]
        .as_object_mut()
        .unwrap()
        .remove("format_name");
    assert_eq!(
        snapshot(&metadata),
        TeletextProbeSnapshot::Unavailable(TeletextProbeUnavailable::MissingContainer)
    );
    metadata = fixture();
    for index in [2, 4] {
        metadata["streams"][index]["codec_name"] = json!("dvb_subtitle");
    }
    assert!(streams(&metadata).is_empty());
    for index in [2, 4] {
        metadata["streams"][index]
            .as_object_mut()
            .unwrap()
            .remove("codec_name");
    }
    assert!(streams(&metadata).is_empty());
    metadata = fixture();
    metadata["streams"][2]
        .as_object_mut()
        .unwrap()
        .remove("codec_type");
    assert_eq!(
        snapshot(&metadata),
        TeletextProbeSnapshot::Unavailable(TeletextProbeUnavailable::MissingCodecType)
    );
    metadata["streams"][2]["codec_type"] = json!("data");
    assert_eq!(
        snapshot(&metadata),
        TeletextProbeSnapshot::Unusable(TeletextProbeProblem::UnexpectedCodecType)
    );
}

#[test]
fn absent_negative_and_duplicate_global_indexes_never_attach_to_zero() {
    let mut metadata = fixture();
    // Even an ordinary stream's broken identity invalidates auxiliary binding.
    metadata["streams"][0]
        .as_object_mut()
        .unwrap()
        .remove("index");
    assert_eq!(
        snapshot(&metadata),
        TeletextProbeSnapshot::Unavailable(TeletextProbeUnavailable::MissingStreamIndex)
    );
    metadata["streams"][0]["index"] = json!(-1);
    assert_eq!(
        snapshot(&metadata),
        TeletextProbeSnapshot::Unusable(TeletextProbeProblem::InvalidStreamIndex)
    );
    metadata["streams"][0]["index"] = json!(2);
    assert_eq!(
        snapshot(&metadata),
        TeletextProbeSnapshot::Unusable(TeletextProbeProblem::DuplicateStreamIndex)
    );
}

#[test]
fn missing_invalid_reserved_null_and_ambiguous_pids_are_explicit() {
    for id in [
        Value::Null,
        json!({"pid": 258}),
        json!(258),
        json!("258"),
        json!("0x+102"),
        json!("0x2000"),
        json!("0x0000"),
        json!("0x000f"),
        json!("0x1fff"),
    ] {
        let mut metadata = fixture();
        metadata["streams"][2]["id"] = id.clone();
        assert_eq!(
            streams(&metadata)[0].pid,
            if id.is_null() {
                TeletextProbePid::Unavailable
            } else {
                TeletextProbePid::Unusable
            }
        );
    }
    let mut metadata = fixture();
    metadata["streams"][0]["id"] = json!("0x102");
    let rows = streams(&metadata);
    assert_eq!(rows[0].pid, TeletextProbePid::Unusable);
    assert_eq!(rows[1].pid, TeletextProbePid::Available(0x200));
    assert!(matches!(
        rows[0].descriptors,
        TeletextProbeDescriptors::Available(_)
    ));
}

#[test]
fn missing_broken_and_unmatched_program_memberships_remain_distinct() {
    let cases = [
        (
            Value::Null,
            TeletextProbePrograms::Unavailable(TeletextProgramsUnavailable::NotProvided),
        ),
        (
            json!(false),
            TeletextProbePrograms::Unusable(TeletextProgramsProblem::InvalidShape),
        ),
        (
            json!([]),
            TeletextProbePrograms::Unavailable(TeletextProgramsUnavailable::NoReportedMembership),
        ),
        (
            json!([{"program_id": 1}]),
            TeletextProbePrograms::Unavailable(
                TeletextProgramsUnavailable::MissingIdentityOrMembership,
            ),
        ),
    ];
    for (programs, expected) in cases {
        let mut metadata = fixture();
        metadata["programs"] = programs;
        assert_eq!(streams(&metadata)[0].programs, expected);
    }
    for (members, expected) in [
        (json!(false), TeletextProgramsProblem::InvalidShape),
        (
            json!([{"index": "2"}]),
            TeletextProgramsProblem::InvalidShape,
        ),
        (
            json!([{"index": 99}]),
            TeletextProgramsProblem::UnknownStreamIndex,
        ),
        (
            json!([{"index": 2}, {"index": 2}]),
            TeletextProgramsProblem::DuplicateMembership,
        ),
    ] {
        let mut metadata = fixture();
        metadata["programs"][0]["streams"] = members;
        assert_eq!(
            streams(&metadata)[0].programs,
            TeletextProbePrograms::Unusable(expected)
        );
    }
    let mut metadata = fixture();
    metadata["programs"][0]["pmt_pid"] = json!({"pid": 4096});
    assert_eq!(
        streams(&metadata)[0].programs,
        TeletextProbePrograms::Unusable(TeletextProgramsProblem::InvalidIdentity)
    );
}

#[test]
fn all_program_memberships_survive_without_selecting_first_program() {
    let mut metadata = fixture();
    let mut second = metadata["programs"][0].clone();
    second["program_id"] = json!(2);
    second["program_num"] = json!(2);
    // A PMT PID is reported, not used as a program uniqueness assumption.
    metadata["programs"]
        .as_array_mut()
        .unwrap()
        .push(second.clone());
    let rows = streams(&metadata);
    let TeletextProbePrograms::Available(programs) = &rows[0].programs else {
        panic!("memberships missing");
    };
    assert_eq!(
        programs
            .iter()
            .map(|program| program.program_id)
            .collect::<Vec<_>>(),
        [1, 2]
    );
    assert_eq!(rows[0].programs, rows[1].programs);
    second["program_num"] = json!(1);
    metadata["programs"][1] = second.clone();
    assert_eq!(
        streams(&metadata)[0].programs,
        TeletextProbePrograms::Unusable(TeletextProgramsProblem::ConflictingProgramIdentity)
    );
    second["program_num"] = json!(2);
    second["program_id"] = json!(1);
    metadata["programs"][1] = second;
    assert_eq!(
        streams(&metadata)[0].programs,
        TeletextProbePrograms::Unusable(TeletextProgramsProblem::ConflictingProgramIdentity)
    );
}

#[test]
fn malformed_optional_descriptor_shapes_do_not_guess_size_or_encoding() {
    let mut metadata = fixture();
    metadata["streams"][2]["extradata"] = json!({"bytes": [10, 0]});
    assert_eq!(
        streams(&metadata)[0].descriptors,
        TeletextProbeDescriptors::Unusable(TeletextDescriptorProblem::InvalidExtradataShape)
    );
    metadata["streams"][2]["extradata"] = json!("\nCgAQiA==\n");
    metadata["streams"][2]["extradata_size"] = json!("4");
    assert_eq!(
        streams(&metadata)[0].descriptors,
        TeletextProbeDescriptors::Unusable(TeletextDescriptorProblem::InvalidSizeShape)
    );
    metadata["streams"][2]["extradata_size"] = json!(u64::MAX);
    assert_eq!(
        streams(&metadata)[0].descriptors,
        TeletextProbeDescriptors::Unusable(TeletextDescriptorProblem::Packed(
            TeletextProjectionProblem::PackedDataTooLarge
        ))
    );
    metadata["streams"][2]["tags"]["language"] = json!("");
    metadata["streams"][2]["extradata"] = json!("\n");
    metadata["streams"][2]["extradata_size"] = json!(0);
    let TeletextProbeDescriptors::Available(projected) = &streams(&metadata)[0].descriptors else {
        panic!("explicit empty metadata unavailable");
    };
    assert!(projected.entries.is_empty());
    metadata["streams"][2]
        .as_object_mut()
        .unwrap()
        .remove("extradata_size");
    assert_eq!(
        streams(&metadata)[0].descriptors,
        TeletextProbeDescriptors::Unavailable(TeletextMetadataUnavailable::MissingExtradataSize)
    );
}

fn ordinary_stream(index: usize) -> Value {
    json!({"index": index, "codec_name": "aac", "codec_type": "audio"})
}

#[test]
fn global_stream_budget_checks_all_rows_at_and_above_boundary() {
    let mut metadata = fixture();
    let rows = metadata["streams"].as_array_mut().unwrap();
    rows.extend((5..MAX_TELETEXT_PROBE_STREAMS).map(ordinary_stream));
    assert_eq!(streams(&metadata).len(), 2);
    metadata["streams"]
        .as_array_mut()
        .unwrap()
        .push(ordinary_stream(MAX_TELETEXT_PROBE_STREAMS));
    assert_eq!(
        snapshot(&metadata),
        TeletextProbeSnapshot::Unusable(TeletextProbeProblem::StreamLimit)
    );
}

fn program(index: usize, member_count: usize) -> Value {
    json!({"program_id": index + 1, "program_num": index + 1, "pmt_pid": 4096, "pcr_pid": 256,
        "streams": (0..member_count).map(|index| json!({"index": index})).collect::<Vec<_>>()})
}

#[test]
fn supplied_program_and_membership_budgets_check_exact_boundaries() {
    let mut metadata = fixture();
    metadata["streams"]
        .as_array_mut()
        .unwrap()
        .extend((5..9).map(ordinary_stream));
    metadata["programs"] = json!((0..MAX_TELETEXT_PROBE_PROGRAMS)
        .map(|index| program(index, 8))
        .collect::<Vec<_>>());
    let rows = streams(&metadata);
    let TeletextProbePrograms::Available(programs) = &rows[0].programs else {
        panic!("boundary memberships rejected");
    };
    assert_eq!(programs.len(), MAX_TELETEXT_PROBE_PROGRAMS);
    metadata["programs"][0]["streams"]
        .as_array_mut()
        .unwrap()
        .push(json!({"index": 8}));
    assert_eq!(
        streams(&metadata)[0].programs,
        TeletextProbePrograms::Unusable(TeletextProgramsProblem::MembershipLimit)
    );
    metadata["programs"] = json!((0..=MAX_TELETEXT_PROBE_PROGRAMS)
        .map(|index| program(index, 1))
        .collect::<Vec<_>>());
    assert_eq!(
        streams(&metadata)[0].programs,
        TeletextProbePrograms::Unusable(TeletextProgramsProblem::ProgramLimit)
    );
}

fn packed_stream(index: usize, pairs: usize) -> Value {
    let bytes = [10, 0].repeat(pairs);
    let mut dump = String::from("\n");
    for chunk in bytes.chunks(60) {
        dump.push_str(&STANDARD.encode(chunk));
        dump.push('\n');
    }
    json!({"index": index, "codec_name": "dvb_teletext", "codec_type": "subtitle",
        "tags": {"language": vec!["eng"; pairs].join(",")}, "extradata": dump, "extradata_size": pairs * 2})
}

#[test]
fn descriptor_budget_bounds_total_decoding_including_stale_tails() {
    let mut metadata = fixture();
    metadata["streams"] = json!((0..4)
        .map(|index| packed_stream(index, 1024))
        .collect::<Vec<_>>());
    metadata.as_object_mut().unwrap().remove("programs");
    assert!(streams(&metadata)
        .iter()
        .all(|row| matches!(row.descriptors, TeletextProbeDescriptors::Available(_))));
    metadata["streams"]
        .as_array_mut()
        .unwrap()
        .push(packed_stream(4, 1));
    assert_eq!(
        snapshot(&metadata),
        TeletextProbeSnapshot::Unusable(TeletextProbeProblem::DescriptorTotalLimit)
    );
}

#[test]
fn shrinking_language_projection_retains_only_fresh_paired_prefix() {
    let mut metadata = fixture();
    metadata["streams"][2]["tags"]["language"] = json!("deu");
    let rows = streams(&metadata);
    let TeletextProbeDescriptors::Available(projected) = &rows[0].descriptors else {
        panic!("fresh prefix missing");
    };
    assert_eq!(projected.entries.len(), 1);
    assert_eq!(projected.entries[0].language.as_str(), "deu");
    assert_eq!(projected.entries[0].display_page.unwrap().get(), 200);
    assert!(projected.warning.is_some());
}

#[test]
fn malformed_packing_and_nondecimal_pages_remain_packed_parser_semantics() {
    let mut metadata = fixture();
    metadata["streams"][2]["extradata"] = json!("00000000: 0a00 1088");
    assert!(matches!(
        streams(&metadata)[0].descriptors,
        TeletextProbeDescriptors::Unusable(TeletextDescriptorProblem::Packed(_))
    ));
    // Raw type 31 and hexadecimal page FA are preserved; display page is absent.
    metadata["streams"][2]["tags"]["language"] = json!("eng");
    metadata["streams"][2]["extradata"] = json!(format!("\n{}\n", STANDARD.encode([0xfa, 0xfa])));
    metadata["streams"][2]["extradata_size"] = json!(2);
    let rows = streams(&metadata);
    let TeletextProbeDescriptors::Available(projected) = &rows[0].descriptors else {
        panic!("raw packed metadata lost");
    };
    assert_eq!(projected.entries[0].descriptor_type, 31);
    assert_eq!(projected.entries[0].wire_page, 0xfa);
    assert!(projected.entries[0].display_page.is_none());
}
