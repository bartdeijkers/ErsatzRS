//! Constructor prerequisites only: accepted metadata is not raw-finish authority.

use ersatzrs_domain::{
    MediaSourceKind, SourceTeletextBindingIdentity, SourceTeletextRevision, TeletextPageNumber,
};
use serde_json::{json, Value};

use super::*;
use crate::ffprobe::{teletext_demux::parse_source_demux, teletext_origin::parse_source_origin};

fn request() -> SourceTeletextPageDecodeRequest {
    SourceTeletextPageDecodeRequest {
        binding: SourceTeletextBindingIdentity::new(
            11,
            MediaSourceKind::Local,
            12,
            13,
            14,
            SourceTeletextRevision::new("synthetic-current-observation".into()).unwrap(),
        )
        .unwrap(),
        stream_index: 7,
        pid: 258,
        program_number: 1,
        page: TeletextPageNumber::new(888).unwrap(),
    }
}

fn origin() -> Value {
    json!({
        "format": {"format_name":"mpegts","start_time":"1.418667"},
        "streams":[
            {"index":4,"id":"0x100","codec_type":"video","start_pts":129_600,"time_base":"1/90000"}
        ]
    })
}

fn trace() -> Value {
    json!({
        "streams":[
            {"index":4,"id":"0x100","codec_type":"video","time_base":"1/90000"},
            {"index":7,"id":"0x102","codec_type":"subtitle","time_base":"1/90000"}
        ],
        "programs":[
            {"program_num":1,"pmt_pid":4096,"pcr_pid":256,"streams":[{"index":4},{"index":7}]}
        ],
        "packets":[
            {"stream_index":4,"pts":129_600,"dts":126_000,"pos":"564","flags":"___"},
            {"stream_index":4,"pts":133_200,"dts":129_600,"pos":"1128","flags":"K__"}
        ]
    })
}

fn collector(
    request: &SourceTeletextPageDecodeRequest,
    origin: &Value,
    trace: &Value,
) -> Option<NativeClockCollector> {
    let origin = parse_source_origin(&serde_json::to_vec(origin).unwrap()).unwrap();
    let trace = parse_source_demux(&serde_json::to_vec(trace).unwrap()).unwrap();
    NativeClockCollector::new(request, &origin, trace)
}

#[test]
fn known_clean_first_global_av_selects_only_the_requested_numeric_source() {
    for flags in ["___", "K__"] {
        let mut trace = trace();
        trace["packets"][0]["flags"] = json!(flags);
        let candidate = collector(&request(), &origin(), &trace).unwrap();
        let selected = candidate.selection();
        assert_eq!(
            (selected.pid, selected.program, selected.pmt, selected.pcr),
            (258, 1, 4096, 256)
        );
        // No raw packet was supplied and no finished evidence is constructed.
        // Actual PSI, first PES and native/raw agreement are proved elsewhere.
    }
}

#[test]
fn unknown_or_non_mpegts_native_format_cannot_construct_a_clock_candidate() {
    for format in [
        Value::Null,
        json!("N/A"),
        json!("matroska,webm"),
        json!("mpegtsraw"),
    ] {
        let mut native = origin();
        native["format"]["format_name"] = format;
        assert!(collector(&request(), &native, &trace()).is_none());
    }
    let mut absent = origin();
    absent["format"]
        .as_object_mut()
        .unwrap()
        .remove("format_name");
    assert!(collector(&request(), &absent, &trace()).is_none());
    let mut invalid = origin();
    invalid["format"]["format_name"] = json!("invalid/format");
    assert!(parse_source_origin(&serde_json::to_vec(&invalid).unwrap()).is_err());
}

#[test]
fn first_unknown_or_corrupt_discard_packet_is_not_replaced_by_later_clean_av() {
    for flags in [
        Value::Null,
        json!("N/A"),
        json!("KX_"),
        json!("K___"),
        json!("__C"),
        json!("_D_"),
        json!("KDC"),
    ] {
        let mut metadata = trace();
        metadata["packets"][0]["flags"] = flags;
        assert!(collector(&request(), &origin(), &metadata).is_none());
    }
    let mut missing = trace();
    missing["packets"][0]
        .as_object_mut()
        .unwrap()
        .remove("flags");
    assert!(collector(&request(), &origin(), &missing).is_none());
}

#[test]
fn actual_first_global_non_av_or_untimed_packet_has_no_first_av_fallback() {
    let mut caption_first = trace();
    caption_first["packets"][0]["stream_index"] = json!(7);
    assert!(collector(&request(), &origin(), &caption_first).is_none());
    for unknown in [Value::Null, json!("N/A")] {
        let mut untimed = trace();
        untimed["packets"][0]["pts"] = unknown.clone();
        untimed["packets"][0]["dts"] = unknown;
        assert!(collector(&request(), &origin(), &untimed).is_none());
    }
    let mut missing = trace();
    missing["packets"][0].as_object_mut().unwrap().remove("pts");
    missing["packets"][0].as_object_mut().unwrap().remove("dts");
    assert!(collector(&request(), &origin(), &missing).is_none());
    let mut empty = trace();
    empty["packets"] = json!([]);
    assert!(collector(&request(), &origin(), &empty).is_none());
}

#[test]
fn first_position_must_be_observed_and_ts188_aligned_without_later_fallback() {
    for position in [Value::Null, json!("N/A"), json!("-1"), json!("565")] {
        let mut metadata = trace();
        metadata["packets"][0]["pos"] = position;
        assert!(collector(&request(), &origin(), &metadata).is_none());
    }
    let mut missing = trace();
    missing["packets"][0].as_object_mut().unwrap().remove("pos");
    assert!(collector(&request(), &origin(), &missing).is_none());
}

#[test]
fn first_av_identity_and_native_90khz_timebase_must_be_explicit() {
    for (field, value) in [
        ("index", json!(3)),
        ("id", json!("0x101")),
        ("codec_type", json!("audio")),
    ] {
        let mut native = origin();
        native["streams"][0][field] = value;
        assert!(collector(&request(), &native, &trace()).is_none());
    }
    let mut no_av = origin();
    no_av["streams"] = json!([]);
    assert!(collector(&request(), &no_av, &trace()).is_none());
    for base in [Value::Null, json!("N/A"), json!("1/48000")] {
        let mut metadata = trace();
        metadata["streams"][0]["time_base"] = base;
        assert!(collector(&request(), &origin(), &metadata).is_none());
    }
    let mut missing = trace();
    missing["streams"][0]
        .as_object_mut()
        .unwrap()
        .remove("time_base");
    assert!(collector(&request(), &origin(), &missing).is_none());
}

#[test]
fn exact_caption_index_pid_program_and_membership_are_required() {
    for (index, pid, program) in [(4, 258, 1), (7, 259, 1), (7, 258, 2)] {
        let mut request = request();
        request.stream_index = index;
        request.pid = pid;
        request.program_number = program;
        assert!(collector(&request, &origin(), &trace()).is_none());
    }
    let mut missing_caption_member = trace();
    missing_caption_member["programs"][0]["streams"] = json!([{"index":4}]);
    assert!(collector(&request(), &origin(), &missing_caption_member).is_none());
    let mut missing_initializer_member = trace();
    missing_initializer_member["programs"][0]["streams"] = json!([{"index":7}]);
    assert!(collector(&request(), &origin(), &missing_initializer_member).is_none());
}

#[test]
fn program_inventory_cannot_omit_an_unselected_global_stream() {
    let mut metadata = trace();
    metadata["streams"].as_array_mut().unwrap().push(json!({
        "index":9,"id":"0x101","codec_type":"data"
    }));
    // Both the initializer and selected caption still belong to the program.
    assert!(collector(&request(), &origin(), &metadata).is_none());
    metadata["programs"][0]["streams"]
        .as_array_mut()
        .unwrap()
        .push(json!({"index":9}));
    assert!(collector(&request(), &origin(), &metadata).is_some());
}

#[test]
fn every_trace_av_identity_requires_a_matching_ordinary_native_origin() {
    let mut metadata = trace();
    metadata["streams"].as_array_mut().unwrap().push(json!({
        "index":9,"id":"0x101","codec_type":"audio","time_base":"1/90000"
    }));
    metadata["programs"][0]["streams"]
        .as_array_mut()
        .unwrap()
        .push(json!({"index":9}));
    assert!(collector(&request(), &origin(), &metadata).is_none());

    let mut native = origin();
    native["streams"].as_array_mut().unwrap().push(json!({
        "index":9,"id":"0x101","codec_type":"audio",
        "start_pts":127_680,"time_base":"1/90000"
    }));
    assert!(collector(&request(), &native, &metadata).is_some());
    for (field, value) in [
        ("index", json!(8)),
        ("id", json!("0x103")),
        ("codec_type", json!("video")),
    ] {
        let mut aliased = native.clone();
        aliased["streams"][1][field] = value;
        assert!(collector(&request(), &aliased, &metadata).is_none());
    }
}

#[test]
fn multiple_programs_and_explicit_no_pcr_remain_unknown() {
    let mut multiple = trace();
    multiple["programs"].as_array_mut().unwrap().push(json!({
        "program_num":2,"pmt_pid":4097,"pcr_pid":256,"streams":[{"index":4},{"index":7}]
    }));
    assert!(collector(&request(), &origin(), &multiple).is_none());
    let mut absent = trace();
    absent["programs"] = json!([]);
    assert!(collector(&request(), &origin(), &absent).is_none());
    let mut no_pcr = trace();
    no_pcr["programs"][0]["pcr_pid"] = json!(0x1fff);
    assert!(collector(&request(), &origin(), &no_pcr).is_none());
}
