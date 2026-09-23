//! Match selected image subtitles to one fresh producer stream without reopening it.
use super::{protocol, unavailable, Result};

pub(super) fn project(
    selected: &crate::playback::PlaybackSubtitle,
    probe: &crate::ffprobe::FfprobeMediaVersion,
    source: protocol::Source,
) -> Result<protocol::Track> {
    let codec = selected.codec.as_deref().ok_or_else(unavailable)?;
    let mut matches = probe.streams.iter().filter(|stream| {
        stream.kind == ersatzrs_domain::MediaStreamKind::Subtitle
            && stream
                .codec
                .as_deref()
                .is_some_and(|fresh| image_codec(fresh) == image_codec(codec))
            && normalized_language(stream.language.as_deref())
                == normalized_language(selected.language.as_deref())
            && normalized_text(stream.title.as_deref())
                == normalized_text(selected.title.as_deref())
            && stream.default == selected.default
            && stream.forced == selected.forced
    });
    let stream = matches.next().ok_or_else(unavailable)?;
    if matches.next().is_some() {
        return Err(unavailable());
    }
    Ok(protocol::Track {
        source: Some(source),
        stream_index: Some(u32::try_from(stream.index).map_err(|_| unavailable())?),
    })
}

fn normalized_text(text: Option<&str>) -> Option<String> {
    text.map(str::trim)
        .filter(|text| !text.is_empty())
        .map(str::to_ascii_lowercase)
}
fn normalized_language(text: Option<&str>) -> Option<String> {
    normalized_text(text)
        .map(|text| ersatzrs_core::language_codes::canonical_language_code(&text).unwrap_or(text))
}
fn image_codec(codec: &str) -> String {
    match codec.trim().to_ascii_lowercase().as_str() {
        "pgs" | "pgssub" | "hdmv_pgs_subtitle" => "hdmv_pgs_subtitle".into(),
        "dvdsub" | "vobsub" | "dvd_subtitle" => "dvd_subtitle".into(),
        codec => codec.into(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn fresh_image_identity_remaps_index_and_rejects_ambiguous_or_missing_tracks() {
        let mut selected = crate::playback::PlaybackSubtitle {
            id: 1,
            subtitle_kind: 0,
            title: Some("Dutch".into()),
            stream_index: 4,
            codec: Some("pgssub".into()),
            default: false,
            forced: false,
            sdh: false,
            language: Some("nl".into()),
            is_extracted: false,
            path: None,
        };
        let mut probe = crate::next_engine::probe_hint::tests::metadata();
        probe.streams.push(ersatzrs_domain::MediaStream {
            index: 7,
            kind: ersatzrs_domain::MediaStreamKind::Subtitle,
            codec: Some("hdmv_pgs_subtitle".into()),
            title: Some("Dutch".into()),
            language: Some("nld".into()),
            ..Default::default()
        });
        let source = protocol::Source::Local {
            path: "fixture.mkv".into(),
            in_point_ms: None,
            out_point_ms: None,
        };
        for alias in ["pgs", "pgssub", "hdmv_pgs_subtitle"] {
            selected.codec = Some(alias.into());
            assert_eq!(
                project(&selected, &probe, source.clone())
                    .unwrap()
                    .stream_index,
                Some(7),
                "native image alias {alias} must identify the same fresh stream"
            );
        }
        let mut duplicate = probe.streams.last().unwrap().clone();
        duplicate.index = 8;
        probe.streams.push(duplicate);
        assert!(project(&selected, &probe, source.clone()).is_err());
        probe.streams.pop();
        probe.streams.last_mut().unwrap().language = Some("eng".into());
        assert!(project(&selected, &probe, source).is_err());
    }
}
