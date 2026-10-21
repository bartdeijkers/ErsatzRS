//! Provider declarations and native layouts must agree before acquisition can bind.
use super::{declaration_matches, is_teletext, native_tracks_match};
use ersatzrs_core::repositories::ProviderMediaSourceDeclaration;
use ersatzrs_domain::{MediaStream, MediaStreamKind};

fn track(index: i32, kind: MediaStreamKind, codec: &str) -> MediaStream {
    MediaStream {
        index,
        kind,
        codec: Some(codec.to_owned()),
        ..MediaStream::default()
    }
}

fn tracks() -> Vec<MediaStream> {
    vec![
        track(0, MediaStreamKind::Video, "h264"),
        track(1, MediaStreamKind::Audio, "aac"),
        track(2, MediaStreamKind::Subtitle, "dvb_teletext"),
    ]
}

fn declaration() -> ProviderMediaSourceDeclaration {
    ProviderMediaSourceDeclaration::new(
        "selected-source".to_owned(),
        "/provider/synthetic.ts".to_owned(),
        [Some(0), Some(1), Some(2)],
    )
    .unwrap()
}

#[test]
fn exact_native_layout_preserves_provider_sidecars_without_using_their_indexes() {
    let mut provider = tracks();
    let native = provider.clone();
    provider.push(track(0, MediaStreamKind::ExternalSubtitle, "subrip"));
    provider.reverse();
    let declaration = declaration();
    assert!(declaration_matches(&declaration, &provider));
    assert!(native_tracks_match(&provider, &native));
    assert_eq!(
        provider.iter().filter(|stream| is_teletext(stream)).count(),
        1
    );
    let external = track(2, MediaStreamKind::ExternalSubtitle, "dvb_teletext");
    assert!(!is_teletext(&external));
    assert!(!declaration_matches(&declaration, &[external]));
}

#[test]
fn incomplete_shifted_and_ambiguous_declarations_cannot_bind() {
    let provider = tracks();
    for indexes in [
        vec![],
        vec![0, 1],
        vec![0, 1, 3],
        vec![0, 1, 2, 3],
        vec![0, 1, 2, 2],
    ] {
        let mut declaration = declaration();
        declaration.stream_indexes = indexes;
        assert!(!declaration_matches(&declaration, &provider));
    }
    let mut duplicate = provider.clone();
    duplicate.push(provider[2].clone());
    let mut declaration = declaration();
    declaration.stream_indexes.push(2);
    assert!(!declaration_matches(&declaration, &duplicate));
    let mut negative = provider;
    negative[0].index = -1;
    declaration.stream_indexes = vec![-1, 1, 2];
    assert!(!declaration_matches(&declaration, &negative));
}

#[test]
fn native_correspondence_requires_every_index_kind_and_codec() {
    let provider = tracks();
    let mut shifted = provider.clone();
    shifted[2].index = 3;
    let mut missing = provider.clone();
    missing.pop();
    let mut extra = provider.clone();
    extra.push(track(3, MediaStreamKind::Audio, "aac"));
    let mut duplicate = provider.clone();
    duplicate[2].index = 1;
    let mut wrong_kind = provider.clone();
    wrong_kind[2].kind = MediaStreamKind::Audio;
    let mut wrong_codec = provider.clone();
    wrong_codec[2].codec = Some("subrip".to_owned());
    for native in [shifted, missing, extra, duplicate, wrong_kind, wrong_codec] {
        assert!(!native_tracks_match(&provider, &native));
    }
    assert!(declaration_matches(&declaration(), &provider));
    assert!(native_tracks_match(&provider, &provider));
}
