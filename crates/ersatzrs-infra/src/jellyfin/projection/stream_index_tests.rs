//! Jellyfin indexes address a mixed provider list; `FFmpeg` addresses the physical file.
use super::*;
use serde_json::json;

#[test]
fn external_stream_positions_map_each_physical_index_without_losing_provider_ids() {
    for (native, external) in [
        ([2, 3, 4], [0, 1]),
        ([0, 1, 2], [3, 4]),
        ([1, 3, 4], [0, 2]),
    ] {
        let mut streams = vec![
            json!({"Type":"Video","Index":native[0],"Codec":"h264","Width":320,"Height":180}),
            json!({"Type":"Audio","Index":native[1],"Codec":"AAC","Language":"nld","Title":"Nederlands","Channels":6,"IsDefault":true}),
            json!({"Type":"Subtitle","Index":native[2],"Codec":"subrip","Language":"eng","IsForced":true}),
            json!({"Type":"Subtitle","Index":external[0],"Codec":"subrip","IsExternal":true,"Language":"nld"}),
            json!({"Type":"Audio","Index":external[1],"Codec":"aac","IsExternal":true}),
        ];
        // List order is not evidence of source index order.
        streams.reverse();
        let response=serde_json::from_value(json!({"MediaSources":[{"Id":"selected","Path":"/provider/synthetic.mkv","MediaStreams":streams}]})).unwrap();
        let actual = project_playback(response).unwrap();
        let mut physical = actual
            .streams
            .iter()
            .filter(|s| s.kind != MediaStreamKind::ExternalSubtitle)
            .map(|s| s.index)
            .collect::<Vec<_>>();
        physical.sort_unstable();
        assert_eq!(physical, [0, 1, 2], "external={external:?}");
        let mut declared = actual.source_declaration.unwrap().stream_indexes;
        declared.sort_unstable();
        assert_eq!(declared, [0, 1, 2]);
        let sidecar = actual
            .streams
            .iter()
            .find(|s| s.kind == MediaStreamKind::ExternalSubtitle)
            .unwrap();
        assert_eq!(
            sidecar.index, external[0],
            "sidecar downloads retain their provider index"
        );
        let audio = actual
            .streams
            .iter()
            .find(|s| s.kind == MediaStreamKind::Audio)
            .unwrap();
        assert_eq!(
            (
                audio.language.as_deref(),
                audio.title.as_deref(),
                audio.channels,
                audio.default
            ),
            (Some("nld"), Some("Nederlands"), Some(6), true)
        );
        assert!(actual
            .streams
            .iter()
            .any(|s| s.kind == MediaStreamKind::Subtitle && s.forced));
    }
}

#[test]
fn normalization_keeps_gaps_and_does_not_count_negative_or_duplicate_external_ids() {
    let mut streams = serde_json::from_value::<Vec<JellyfinMediaStreamResponse>>(json!([
        {"Type":"Video","Index":5},
        {"Type":"Audio","Index":9},
        {"Type":"Subtitle","Index":0,"IsExternal":true},
        {"Type":"Audio","Index":2,"IsExternal":true},
        {"Type":"Subtitle","Index":2,"IsExternal":true},
        {"Type":"Audio","Index":-1,"IsExternal":true},
        {"Type":"Subtitle","IsExternal":true}
    ]))
    .unwrap();
    normalize_source_stream_indexes(&mut streams);
    assert_eq!(
        streams.iter().map(|s| s.index).collect::<Vec<_>>(),
        [Some(3), Some(7), Some(0), Some(2), Some(2), Some(-1), None]
    );
}
