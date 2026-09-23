use super::*;

fn rebase(text: &str, format: SubtitleTextFormat, seek: u64) -> Option<String> {
    rebase_text_cues_milliseconds(text.as_bytes().to_vec(), format, seek, 4096).unwrap()
}

#[test]
fn fractional_join_keeps_active_cues_and_drops_expired_cues() {
    let srt = "1\n00:00:00,000 --> 00:00:01,234\nBEFORE\n\n2\n00:00:01,000 --> 00:00:02,345\nACTIVE\n\n3\n00:00:03,000 --> 00:00:04,000\nAFTER\n";
    let out = rebase(srt, SubtitleTextFormat::Srt, 1234).unwrap();
    assert!(!out.contains("BEFORE"));
    assert!(out.contains("00:00:00,000 --> 00:00:01,111\nACTIVE"));
    assert!(out.contains("00:00:01,766 --> 00:00:02,766\nAFTER"));
    assert!(rebase(srt, SubtitleTextFormat::Srt, 4000).is_none());
}

#[test]
fn fractional_webvtt_and_ass_keep_format_structure() {
    let vtt = "WEBVTT\n\nSTYLE\n::cue { color: lime; }\n\nNOTE preserved\n\nidentifier\n00:01.000 --> 00:02.345 align:start position:10%\nACTIVE\n";
    let out = rebase(vtt, SubtitleTextFormat::WebVtt, 1234).unwrap();
    assert!(out.contains("STYLE\n::cue { color: lime; }"));
    assert!(out.contains("NOTE preserved"));
    assert!(out.contains("identifier\n00:00:00.000 --> 00:00:01.111 align:start position:10%"));
    let ass = "[Script Info]\nTitle: Fixture\n[V4+ Styles]\nFormat: Name, Fontname\nStyle: Default,Arial\n[Events]\nFormat: Layer, Start, End, Style, Text\nDialogue: 0,0:00:01.00,0:00:02.34,Default,{\\b1}ACTIVE, comma\n";
    let out = rebase(ass, SubtitleTextFormat::Ass, 1234).unwrap();
    assert!(out.contains("Style: Default,Arial"));
    assert!(out.contains("Dialogue: 0,0:00:00.00,0:00:01.11,Default,{\\b1}ACTIVE, comma"));
}

#[test]
fn bounded_encoding_and_invalid_cues_never_fall_back() {
    let text = "1\n00:00:00,000 --> 00:00:02,000\nACTIVE\n";
    let mut utf16 = vec![0xff, 0xfe];
    utf16.extend(text.encode_utf16().flat_map(u16::to_le_bytes));
    assert!(
        rebase_text_cues_milliseconds(utf16, SubtitleTextFormat::Srt, 1234, 4096)
            .unwrap()
            .unwrap()
            .contains("00:00:00,766")
    );
    for (bytes, seek, limit) in [
        (b"invalid".to_vec(), 0, 4096),
        (text.as_bytes().to_vec(), u64::MAX, 4096),
        (text.as_bytes().to_vec(), 0, 3),
    ] {
        assert!(
            rebase_text_cues_milliseconds(bytes, SubtitleTextFormat::Srt, seek, limit).is_err()
        );
    }
}
