use super::*;
use crate::{
    MediaSourceKind, SourceTeletextBinding, SourceTeletextBindingIdentity,
    SourceTeletextDescriptors, SourceTeletextObservation, SourceTeletextObservedPages,
    SourceTeletextPid, SourceTeletextPrograms, SourceTeletextProvenance, SourceTeletextRevision,
};

fn window() -> SourceTeletextHeaderWindow {
    SourceTeletextHeaderWindow {
        transport_stream_id: 1,
        pat_version: 31,
        psi_fingerprint: [0xa5; 32],
        packet_count: 32_768,
        complete_pes: 2,
        unobserved_prefix: true,
        ended_at_eof: false,
        programs: vec![SourceTeletextHeaderProgram {
            program_number: 1,
            pmt_pid: 0x20,
            pcr_pid: 0x1fff,
            pmt_version: 31,
            stream_type: 6,
            descriptors: vec![0xff; 1_024],
        }],
        headers: vec![SourceTeletextObservedHeader {
            wire_magazine: 0,
            wire_page: 0xfe,
            subcode: 0x3f7f,
            controls: 0x7ff,
            subtitle_data_unit: true,
            field_parity: true,
            line_offset: 22,
            corrected_address_bytes: 2,
            corrected_header_bytes: 8,
            completion_packet: 32_767,
            pes_ordinal: 1,
            unit_ordinal: 32_726,
            pts: Some((1_u64 << 33) - 1),
        }],
    }
}

#[test]
fn source_header_window_boundaries_roundtrip_without_inventing_page_semantics() {
    let mut value = window();
    value.validate().unwrap();
    let encoded = serde_json::to_string(&value).unwrap();
    assert_eq!(
        serde_json::from_str::<SourceTeletextHeaderWindow>(&encoded).unwrap(),
        value
    );
    value.headers[0].unit_ordinal = 0;
    for ordinal in 1..1_024 {
        let mut repeated = value.headers[0].clone();
        repeated.unit_ordinal = ordinal;
        value.headers.push(repeated);
    }
    value.validate().unwrap();
    value.headers.clear();
    value.complete_pes = 0;
    value.validate().unwrap();
    value.packet_count = 1;
    value.validate().unwrap();
    let program = value.programs[0].clone();
    for number in 2..=128 {
        let mut member = program.clone();
        member.program_number = number;
        value.programs.push(member);
    }
    value.validate().unwrap();
    value.programs.push(program);
    assert_eq!(value.validate(), Err(SourceTeletextError::HeaderWindow));
}

#[test]
fn source_header_window_rejects_malformed_fields_in_values_and_serde() {
    let invalid: &[fn(&mut SourceTeletextHeaderWindow)] = &[
        |v| v.packet_count = 0,
        |v| v.packet_count = 32_769,
        |v| v.complete_pes = v.packet_count + 1,
        |v| v.pat_version = 32,
        |v| v.programs.clear(),
        |v| v.programs[0].program_number = 0,
        |v| v.programs[0].pmt_pid = 0x1f,
        |v| v.programs[0].pmt_pid = 0x1fff,
        |v| v.programs[0].pcr_pid = 0x2000,
        |v| v.programs[0].pcr_pid = 0x1f,
        |v| v.programs[0].pmt_version = 32,
        |v| v.programs[0].descriptors.push(0),
        |v| v.programs.push(v.programs[0].clone()),
        |v| v.headers[0].wire_magazine = 8,
        |v| v.headers[0].wire_page = 0xff,
        |v| v.headers[0].subcode = 0x80,
        |v| v.headers[0].subcode = 0x4000,
        |v| v.headers[0].controls = 0x800,
        |v| v.headers[0].line_offset = 6,
        |v| v.headers[0].line_offset = 23,
        |v| v.headers[0].corrected_address_bytes = 3,
        |v| v.headers[0].corrected_header_bytes = 9,
        |v| v.headers[0].completion_packet = v.packet_count,
        |v| v.headers[0].pes_ordinal = v.complete_pes,
        |v| v.headers[0].unit_ordinal = 32_727,
        |v| v.headers[0].pts = Some(1_u64 << 33),
        |v| v.headers.resize(1_025, v.headers[0].clone()),
    ];
    for mutate in invalid {
        let mut value = window();
        mutate(&mut value);
        assert!(value.validate().is_err());
        assert!(serde_json::from_str::<SourceTeletextHeaderWindow>(
            &serde_json::to_string(&value).unwrap()
        )
        .is_err());
    }
    let mut value = serde_json::to_value(window()).unwrap();
    value["unknown"] = serde_json::json!(true);
    assert!(serde_json::from_value::<SourceTeletextHeaderWindow>(value).is_err());
}

#[test]
fn source_header_window_retains_repeats_but_requires_strict_occurrence_order() {
    let mut value = window();
    value.headers[0].completion_packet = 4;
    value.headers[0].pes_ordinal = 0;
    value.headers[0].unit_ordinal = 0;
    let mut repeated = value.headers[0].clone();
    repeated.unit_ordinal = 1;
    value.headers.push(repeated);
    value.validate().unwrap();
    let valid = value.clone();
    value.headers[1].unit_ordinal = 0;
    assert_eq!(value.validate(), Err(SourceTeletextError::HeaderOrder));
    value = valid.clone();
    value.headers[1].completion_packet = 5;
    assert_eq!(value.validate(), Err(SourceTeletextError::HeaderOrder));
    value = valid;
    value.headers[1].completion_packet = 5;
    value.headers[1].pes_ordinal = 1;
    value.headers[1].unit_ordinal = 0;
    value.validate().unwrap();
    value.headers.reverse();
    assert_eq!(value.validate(), Err(SourceTeletextError::HeaderOrder));
}

#[test]
fn source_header_observation_requires_actual_bound_parent_and_pid() {
    let mut observation = SourceTeletextObservation::new(
        SourceTeletextBinding::Unavailable,
        3,
        SourceTeletextProvenance::Ffprobe9PackedDescriptorProjection,
        SourceTeletextPid::Reported(0x103),
        SourceTeletextPrograms::Unavailable,
        SourceTeletextDescriptors::Unavailable,
    )
    .unwrap();
    assert_eq!(
        observation.observed_pages,
        SourceTeletextObservedPages::NotAcquired
    );
    observation.observed_pages = SourceTeletextObservedPages::HeadersObserved(Box::new(window()));
    assert_eq!(
        observation.validate(),
        Err(SourceTeletextError::HeaderBinding)
    );
    observation.binding = SourceTeletextBinding::Bound(
        SourceTeletextBindingIdentity::new(
            1,
            MediaSourceKind::Local,
            2,
            3,
            4,
            SourceTeletextRevision::new("observed-fixture".into()).unwrap(),
        )
        .unwrap(),
    );
    observation.validate().unwrap();
    for pid in [
        SourceTeletextPid::Unavailable,
        SourceTeletextPid::Unusable,
        SourceTeletextPid::Reported(0x1f),
        SourceTeletextPid::Reported(0x1fff),
    ] {
        observation.pid = pid;
        assert_eq!(
            observation.validate(),
            Err(SourceTeletextError::HeaderBinding)
        );
    }
}
