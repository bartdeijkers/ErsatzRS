//! Provider declarations never turn Plex's ordinary ID fallback into native identity.
use serde_json::{json, Value};

fn response() -> Value {
    json!({ "key": "fixture-item", "Media": [
        { "id": 5, "Part": [{ "id": 7, "file": "/provider/synthetic.ts", "Stream": [
            { "streamType": 1, "codec": "h264", "id": 40, "index": 0 },
            { "streamType": 2, "codec": "aac", "id": 41, "index": 1 },
            { "streamType": 3, "codec": "dvb_teletext", "id": 42, "index": 2 },
            { "streamType": 3, "codec": "subrip", "id": 43, "key": "/sidecar" }
        ] }] }
    ] })
}

#[test]
fn source_declaration_retains_selected_part_and_external_index_fallback() {
    let actual = super::project_media_item(serde_json::from_value(response()).unwrap()).unwrap();
    let declaration = actual.source_declaration.as_ref().unwrap();
    assert_eq!(declaration.source_id, "7");
    assert_eq!(declaration.path, "/provider/synthetic.ts");
    assert_eq!(declaration.stream_indexes, [0, 1, 2]);
    assert_eq!(actual.plex_part_id, 7);
    assert_eq!(actual.streams.len(), 4);
    assert_eq!(actual.streams[3].index, 43);
    assert_eq!(actual.streams[3].file_name.as_deref(), Some("/sidecar"));
}

#[test]
fn source_declaration_rejects_incomplete_or_ambiguous_native_indexes() {
    for (pointer, value) in [
        ("/Media/0/Part/0/id", json!(0)),
        ("/Media/0/Part/0/file", json!(" ")),
        ("/Media/0/Part/0/Stream/2/index", Value::Null),
        ("/Media/0/Part/0/Stream/2/index", json!(-1)),
        ("/Media/0/Part/0/Stream/2/index", json!(1)),
    ] {
        let mut input = response();
        *input.pointer_mut(pointer).unwrap() = value;
        let actual = super::project_media_item(serde_json::from_value(input).unwrap()).unwrap();
        assert!(actual.source_declaration.is_none(), "refused {pointer}");
        assert_eq!(actual.streams.len(), 4);
        assert_eq!(actual.streams[3].index, 43);
    }
    let mut input = response();
    input["Media"][0]["Part"][0]["Stream"][2]
        .as_object_mut()
        .unwrap()
        .remove("index");
    let actual = super::project_media_item(serde_json::from_value(input).unwrap()).unwrap();
    assert!(actual.source_declaration.is_none());
    assert_eq!(
        actual.streams[2].index, 42,
        "ordinary fallback is unchanged"
    );
}
