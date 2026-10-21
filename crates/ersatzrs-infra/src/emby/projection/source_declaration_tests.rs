//! Source declarations retain provider input without changing ordinary projection.
use serde_json::{json, Value};

fn response() -> Value {
    json!({ "MediaSources": [
        { "Id": "selected-source", "Path": "/provider/synthetic.ts", "MediaStreams": [
            { "Type": "Video", "Codec": "h264", "Index": 0, "Width": 640 },
            { "Type": "Audio", "Codec": "aac", "Index": 1 },
            { "Type": "Subtitle", "Codec": "dvb_teletext", "Index": 2 },
            { "Type": "Subtitle", "Codec": "subrip", "IsExternal": true }
        ] },
        { "Id": "other-source", "Path": "/provider/other.ts", "MediaStreams": [
            { "Type": "Video", "Codec": "hevc", "Index": 0, "Width": 1920 }
        ] }
    ] })
}

#[test]
fn source_declaration_retains_selected_source_and_external_index_fallback() {
    let actual = super::project_playback(serde_json::from_value(response()).unwrap()).unwrap();
    let declaration = actual.source_declaration.as_ref().unwrap();
    assert_eq!(declaration.source_id, "selected-source");
    assert_eq!(declaration.path, "/provider/synthetic.ts");
    assert_eq!(declaration.stream_indexes, [0, 1, 2]);
    assert_eq!(actual.width, 640);
    assert_eq!(actual.streams.len(), 4);
    assert_eq!(actual.streams[3].index, 0);
    assert_eq!(
        actual.streams[3].file_name.as_deref(),
        Some("selected-source")
    );
}

#[test]
fn source_declaration_rejects_incomplete_or_ambiguous_native_indexes() {
    for (pointer, value) in [
        ("/MediaSources/0/Id", Value::Null),
        ("/MediaSources/0/Path", json!(" ")),
        ("/MediaSources/0/MediaStreams/2/Index", Value::Null),
        ("/MediaSources/0/MediaStreams/2/Index", json!(-1)),
        ("/MediaSources/0/MediaStreams/2/Index", json!(1)),
    ] {
        let mut input = response();
        *input.pointer_mut(pointer).unwrap() = value;
        let actual = super::project_playback(serde_json::from_value(input).unwrap()).unwrap();
        assert!(actual.source_declaration.is_none(), "refused {pointer}");
        assert_eq!(
            actual.width, 640,
            "ordinary first-source selection is unchanged"
        );
        assert_eq!(actual.streams.len(), 4);
        assert_eq!(actual.streams[3].index, 0);
    }
    let mut input = response();
    input["MediaSources"][0]["MediaStreams"][2]
        .as_object_mut()
        .unwrap()
        .remove("Index");
    let actual = super::project_playback(serde_json::from_value(input).unwrap()).unwrap();
    assert!(actual.source_declaration.is_none());
    assert_eq!(actual.streams[2].index, 0, "ordinary fallback is unchanged");
}
