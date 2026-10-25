use super::*;
use serde_json::json;

#[test]
fn external_audio_is_not_a_physical_stream_at_any_index() {
    for index in [-5, -1, 7] {
        let stream = serde_json::from_value(json!({
            "Type":"Audio", "Index":index, "Codec":"aac", "Language":"nld",
            "IsDefault":true, "IsExternal":true
        }))
        .unwrap();
        assert!(project_stream(stream).is_none());
    }
}

#[test]
fn playback_keeps_native_track_identity_and_external_subtitles() {
    let response = serde_json::from_value(json!({"MediaSources":[{
        "Id":"selected-source", "Path":"/provider/synthetic.mkv", "RunTimeTicks":120_000_000,
        "MediaStreams":[
            {"Type":"Video","Index":0,"Codec":"h264","Width":320,"Height":180},
            {"Type":"Audio","Index":1,"Codec":"AAC","Language":"eng","IsDefault":true,"Channels":2},
            {"Type":"Audio","Index":2,"Codec":"aac","Language":"nld","Title":"Dutch","IsForced":true,"Channels":6},
            {"Type":"Audio","Index":-1,"Codec":"aac","IsExternal":true},
            {"Type":"Audio","Index":7,"Codec":"aac","IsExternal":true},
            {"Type":"Subtitle","Index":3,"Codec":"subrip","Language":"eng"},
            {"Type":"Subtitle","Index":-2,"Codec":"subrip","Language":"nld","IsExternal":true,"IsForced":true}
        ]
    }]})).unwrap();
    let actual = project_playback(response).unwrap();
    assert_eq!(
        actual.source_declaration.unwrap().stream_indexes,
        [0, 1, 2, 3]
    );
    assert_eq!(
        actual
            .streams
            .iter()
            .map(|s| (s.index, s.kind))
            .collect::<Vec<_>>(),
        [
            (0, MediaStreamKind::Video),
            (1, MediaStreamKind::Audio),
            (2, MediaStreamKind::Audio),
            (3, MediaStreamKind::Subtitle),
            (-2, MediaStreamKind::ExternalSubtitle)
        ]
    );
    let english = &actual.streams[1];
    assert!(english.default);
    assert_eq!(english.codec.as_deref(), Some("aac"));
    let dutch = &actual.streams[2];
    assert_eq!(
        (
            dutch.language.as_deref(),
            dutch.title.as_deref(),
            dutch.channels
        ),
        (Some("nld"), Some("Dutch"), Some(6))
    );
    assert!(dutch.forced);
    let subtitle = &actual.streams[4];
    assert_eq!(subtitle.language.as_deref(), Some("nld"));
    assert!(subtitle.forced);
}
