//! Metadata captured from `FFprobe` 9 and the redistributable synthetic wrapper
//! transport. The fixture retains both audio languages, source subtitle pages
//! and a separate authored service; it is not a complete transmitted-page list.

use ersatzrs_domain::MediaStreamKind;
use ersatzrs_infra::ffprobe::{
    parse_ffprobe_json, parse_ffprobe_json_with_teletext,
    teletext::{
        project_packed_descriptors, PackedTeletextData, PackedTeletextProjection,
        ProjectedTeletextDescriptors,
    },
    teletext_probe::{TeletextProbeEncoding, TeletextProbeSnapshot},
    FfprobeMediaVersion,
};

fn fixture() -> serde_json::Value {
    serde_json::from_str(include_str!("fixtures/ffprobe_teletext_base64.json")).unwrap()
}

fn assert_ordinary_equivalent(mut actual: FfprobeMediaVersion, expected: &FfprobeMediaVersion) {
    // Fresh scan timestamps are the only intentionally variable fields.
    actual.version.date_added = expected.version.date_added;
    actual.version.date_updated = expected.version.date_updated;
    assert_eq!(&actual, expected);
}

fn enriched_fixture() -> serde_json::Value {
    let mut metadata = fixture();
    metadata["format"]["duration"] = serde_json::json!("12.5");
    metadata["format"]["tags"] =
        serde_json::json!({"TITLE": "Synthetic programme", "comment": "Fixture only"});
    metadata["chapters"] = serde_json::json!([{"id": 0, "start_time": "1.0", "end_time": "3.0", "tags": {"title": "Chapter one"}}]);
    metadata["streams"][0]["field_order"] = serde_json::json!("progressive");
    metadata["streams"][0]["color_transfer"] = serde_json::json!("smpte2084");
    metadata["streams"][1]["disposition"]["default"] = serde_json::json!(1);
    metadata["streams"][2]["disposition"]["forced"] = serde_json::json!(1);
    metadata
}

fn descriptors(stream: &serde_json::Value) -> ProjectedTeletextDescriptors {
    let projection = project_packed_descriptors(
        stream["tags"]["language"].as_str(),
        stream["extradata"]
            .as_str()
            .map(PackedTeletextData::Ffprobe9Base64),
        stream["extradata_size"]
            .as_u64()
            .map(|size| usize::try_from(size).unwrap()),
    );
    let PackedTeletextProjection::Available(projected) = projection else {
        panic!("synthetic FFprobe metadata must project: {projection:?}");
    };
    projected
}

#[test]
fn ffprobe_teletext_fixture_keeps_source_pages_and_authored_language_separate() {
    let metadata = fixture();
    let source = descriptors(&metadata["streams"][2]);
    assert!(source.warning.is_none());
    assert_eq!(source.entries.len(), 2);
    for (ordinal, descriptor_type, magazine, page, display) in
        [(0, 1, 2, 0x00, 200), (1, 2, 0, 0x88, 888)]
    {
        let entry = &source.entries[ordinal];
        assert_eq!(entry.projection_ordinal, ordinal);
        assert_eq!(entry.language.as_str(), "eng");
        assert_eq!(entry.descriptor_type, descriptor_type);
        assert_eq!(entry.wire_magazine, magazine);
        assert_eq!(entry.wire_page, page);
        assert_eq!(entry.display_page.unwrap().get(), display);
    }
    let authored = descriptors(&metadata["streams"][4]);
    assert!(authored.warning.is_none());
    assert_eq!(authored.entries.len(), 1);
    assert_eq!(authored.entries[0].language.as_str(), "nld");
    assert_eq!(authored.entries[0].descriptor_type, 1);
    assert_eq!(authored.entries[0].display_page.unwrap().get(), 200);
}

#[test]
fn ffprobe_teletext_metadata_preserves_existing_audio_and_subtitle_projection() {
    let metadata = enriched_fixture();
    let (full, additional) = parse_ffprobe_json_with_teletext(
        "synthetic.ts",
        &serde_json::to_vec(&metadata).unwrap(),
        TeletextProbeEncoding::Ffprobe9Base64,
    )
    .unwrap();
    assert!(matches!(additional, TeletextProbeSnapshot::Available(_)));
    let legacy =
        parse_ffprobe_json("synthetic.ts", &serde_json::to_vec(&metadata).unwrap()).unwrap();
    assert_ordinary_equivalent(legacy, &full);
    let mut ordinary = metadata.clone();
    ordinary.as_object_mut().unwrap().remove("programs");
    for stream in ordinary["streams"].as_array_mut().unwrap() {
        for key in ["id", "extradata", "extradata_size"] {
            stream.as_object_mut().unwrap().remove(key);
        }
    }
    let stripped =
        parse_ffprobe_json("synthetic.ts", &serde_json::to_vec(&ordinary).unwrap()).unwrap();
    assert_ordinary_equivalent(stripped, &full);
    assert_eq!(full.chapters.len(), 1);
    assert_eq!(
        full.field_orders.get(&0).map(String::as_str),
        Some("progressive")
    );
    assert_eq!(
        full.format_metadata.get("title").map(String::as_str),
        Some("Synthetic programme")
    );
    assert_eq!(full.streams.len(), 5);
    assert_eq!(full.streams[1].kind, MediaStreamKind::Audio);
    assert_eq!(full.streams[1].language.as_deref(), Some("eng"));
    assert_eq!(full.streams[3].kind, MediaStreamKind::Audio);
    assert_eq!(full.streams[3].language.as_deref(), Some("nld"));
    assert_eq!(full.streams[2].kind, MediaStreamKind::Subtitle);
    assert_eq!(full.streams[2].codec.as_deref(), Some("dvb_teletext"));
    assert_eq!(full.streams[2].language.as_deref(), Some("eng,eng"));
    assert_eq!(full.streams[4].kind, MediaStreamKind::Subtitle);
    assert_eq!(full.streams[4].language.as_deref(), Some("nld"));
}

#[test]
fn unusable_additional_teletext_metadata_does_not_erase_ordinary_tracks() {
    let mut metadata = enriched_fixture();
    let original =
        parse_ffprobe_json("synthetic.ts", &serde_json::to_vec(&metadata).unwrap()).unwrap();
    metadata["programs"] = serde_json::json!(false);
    for stream in metadata["streams"].as_array_mut().unwrap() {
        if stream["codec_name"] == "dvb_teletext" {
            stream["extradata"] = serde_json::json!({"unusable": true});
            stream["extradata_size"] = serde_json::json!("unusable");
            stream["id"] = serde_json::json!({"unusable": true});
        }
    }
    let (preserved, additional) = parse_ffprobe_json_with_teletext(
        "synthetic.ts",
        &serde_json::to_vec(&metadata).unwrap(),
        TeletextProbeEncoding::Ffprobe9Base64,
    )
    .unwrap();
    assert_ordinary_equivalent(preserved, &original);
    assert!(matches!(additional, TeletextProbeSnapshot::Available(_)));
}
