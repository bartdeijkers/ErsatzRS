use super::*;
use crate::next_engine::protocol::Track;

fn http(uri: &str, headers: &[&str]) -> Source {
    serde_json::from_value(serde_json::json!({"source_type":"http","uri":uri,"headers":headers}))
        .unwrap()
}

#[test]
fn selected_video_requires_the_same_retained_http_bytes_and_credentials() {
    let source = http("http://127.0.0.1/retained/first", &["Access: synthetic"]);
    let mut cut = source.clone();
    if let Source::Http {
        in_point_ms,
        out_point_ms,
        ..
    } = &mut cut
    {
        *in_point_ms = Some(1_000);
        *out_point_ms = Some(8_000);
    }
    let mut video = Track {
        source: Some(cut),
        stream_index: Some(2),
    };
    assert_eq!(selected_video(&source, Some(&video)), Some(2));
    for other in [
        http("http://127.0.0.1/retained/second", &["Access: synthetic"]),
        http("http://127.0.0.1/retained/first", &["Access: changed"]),
        http("http://127.0.0.1/retained/first", &[]),
    ] {
        video.source = Some(other);
        assert_eq!(selected_video(&source, Some(&video)), None);
    }
    video.source = Some(source.clone());
    video.stream_index = None;
    assert_eq!(selected_video(&source, Some(&video)), None);
    video.source = None;
    assert_eq!(selected_video(&source, Some(&video)), None);
    assert_eq!(selected_video(&source, None), None);
}

#[test]
fn local_video_matching_preserves_path_identity_independent_of_program_cuts() {
    let source = Source::Local {
        path: "synthetic/first.ts".into(),
        probe_hint: None,
        in_point_ms: None,
        out_point_ms: None,
    };
    let mut cut = source.clone();
    if let Source::Local { in_point_ms, .. } = &mut cut {
        *in_point_ms = Some(1_000);
    }
    let mut video = Track {
        source: Some(cut),
        stream_index: Some(0),
    };
    assert_eq!(selected_video(&source, Some(&video)), Some(0));
    if let Some(Source::Local { path, .. }) = &mut video.source {
        *path = "synthetic/second.ts".into();
    }
    assert_eq!(selected_video(&source, Some(&video)), None);
}
