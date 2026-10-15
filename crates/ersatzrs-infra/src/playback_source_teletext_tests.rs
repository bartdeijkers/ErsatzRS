use super::*;
use ersatzrs_domain::{
    MediaSourceKind, SourceTeletextBindingIdentity, SourceTeletextRevision, TeletextLanguage,
    TeletextPageNumber,
};
use ersatzrs_ffmpeg::teletext::source_descriptors::SourceTeletextWireDescriptor;

fn candidate(page: u16, language: &str, role: u8) -> SourceTeletextCaptionCandidate {
    SourceTeletextCaptionCandidate {
        request: SourceTeletextPageDecodeRequest {
            binding: SourceTeletextBindingIdentity::new(
                11,
                MediaSourceKind::Local,
                20,
                30,
                40,
                SourceTeletextRevision::new("synthetic-observed-revision".into()).unwrap(),
            )
            .unwrap(),
            stream_index: 3,
            pid: 0x103,
            program_number: 1,
            page: TeletextPageNumber::new(page).unwrap(),
        },
        language: TeletextLanguage::new(language.into()).unwrap(),
        descriptor_type: role,
        psi_fingerprint: [7; 32],
        descriptor_occurrences: vec![SourceTeletextWireDescriptor {
            descriptor_tag: 0x56,
            descriptor_ordinal: 0,
            entry_ordinal: 0,
            language: TeletextLanguage::new(language.into()).unwrap(),
            descriptor_type: role,
            wire_magazine: u8::try_from(page / 100 % 8).unwrap(),
            wire_page: u8::try_from((page % 100 / 10) * 16 + page % 10).unwrap(),
        }],
    }
}

fn window() -> SourceTeletextDecodeWindowEvidence {
    SourceTeletextDecodeWindowEvidence {
        source_packets: 100,
        decoder_packets: 100,
        skipped_selected_prefix_packets: 0,
        ended_at_eof: true,
        decoder_sha256: [8; 32],
        psi_fingerprint: [7; 32],
        program_number: 1,
        pmt_pid: 0x1000,
        pcr_pid: 0x100,
    }
}

fn embedded() -> PlaybackSubtitle {
    PlaybackSubtitle {
        id: 101,
        subtitle_kind: subtitle_kind::EMBEDDED,
        title: Some("Original source title".into()),
        stream_index: 3,
        codec: Some("dvb_teletext".into()),
        default: false,
        forced: false,
        sdh: false,
        language: Some("eng,eng".into()),
        is_extracted: false,
        path: None,
    }
}

fn sidecar() -> PlaybackSubtitle {
    PlaybackSubtitle {
        id: 102,
        subtitle_kind: subtitle_kind::SIDECAR,
        stream_index: 8,
        codec: Some("srt".into()),
        language: Some("nld".into()),
        path: Some("synthetic.nld.srt".into()),
        ..embedded()
    }
}

fn selected(
    rows: &[PlaybackSubtitle],
    sources: &[&SourceTeletextCaptionCandidate],
    languages: &[&str],
    mode: ChannelSubtitleMode,
) -> Option<(i64, Option<usize>)> {
    let choices = metadata_choices(rows, 20, StreamingMode::TransportStream, false, sources);
    let languages = languages
        .iter()
        .map(|language| (*language).to_owned())
        .collect::<Vec<_>>();
    let index = select_subtitle_preference_index(
        choices
            .iter()
            .enumerate()
            .map(|(index, choice)| (index, choice.row, choice.language)),
        &languages,
        mode,
    )?;
    Some((choices[index].row.id, choices[index].source))
}

#[test]
fn exact_request_and_psi_qualify_without_inventing_charset_or_readiness() {
    let source = candidate(888, "eng", 2);
    assert_eq!(qualify(&source, &source.request, &window()), Ok(()));
    let mut different_window = window();
    different_window.psi_fingerprint[0] ^= 1;
    assert_eq!(
        qualify(&source, &source.request, &different_window),
        Err(SourceCaptionQualificationError::Psi)
    );
    different_window = window();
    different_window.program_number = 2;
    assert_eq!(
        qualify(&source, &source.request, &different_window),
        Err(SourceCaptionQualificationError::Psi)
    );
}

#[test]
fn qualification_rejects_each_changed_source_request_identity() {
    let source = candidate(888, "eng", 2);
    let original = source.request.clone();
    let mut changed = Vec::new();
    for field in 0..5 {
        let mut request = original.clone();
        match field {
            0 => request.binding.media_source_id += 1,
            1 => request.binding.media_version_id += 1,
            2 => request.binding.media_file_id += 1,
            3 => request.binding.media_stream_id += 1,
            _ => {
                request.binding.source_revision =
                    SourceTeletextRevision::new("different-observed-revision".into()).unwrap();
            }
        }
        changed.push(request);
    }
    for field in 0..4 {
        let mut request = original.clone();
        match field {
            0 => request.stream_index += 1,
            1 => request.pid += 1,
            2 => request.program_number += 1,
            _ => request.page = TeletextPageNumber::new(889).unwrap(),
        }
        changed.push(request);
    }
    for request in changed {
        assert_eq!(
            qualify(&source, &request, &window()),
            Err(SourceCaptionQualificationError::Identity)
        );
    }
}

#[test]
fn qualification_rejects_missing_conflicting_or_non_subtitle_provenance() {
    let original = candidate(888, "eng", 2);
    for field in 0..6 {
        let mut source = original.clone();
        match field {
            0 => source.descriptor_occurrences.clear(),
            1 => source.descriptor_type = 1,
            2 => source.descriptor_occurrences[0].descriptor_tag = 0x45,
            3 => {
                source.descriptor_occurrences[0].language =
                    TeletextLanguage::new("nld".into()).unwrap();
            }
            4 => source.descriptor_occurrences[0].descriptor_type = 5,
            _ => source.descriptor_occurrences[0].wire_page = 0x89,
        }
        assert_eq!(
            qualify(&source, &original.request, &window()),
            Err(SourceCaptionQualificationError::Descriptor)
        );
    }
}

#[test]
fn page_language_overlays_packed_stream_language_before_shared_ranking() {
    let rows = [embedded(), sidecar()];
    let source = candidate(888, "eng", 2);
    assert_eq!(
        selected(&rows, &[&source], &["eng"], ChannelSubtitleMode::Any),
        Some((101, Some(0)))
    );
    assert_eq!(
        selected(&rows, &[&source], &["nld", "eng"], ChannelSubtitleMode::Any),
        Some((102, None))
    );
    assert_eq!(rows[0].language.as_deref(), Some("eng,eng"));
    assert_eq!(rows[0].title.as_deref(), Some("Original source title"));
}

#[test]
fn descriptor_role_never_synthesizes_forced_or_default_metadata() {
    let mut ordinary = sidecar();
    ordinary.default = true;
    ordinary.forced = true;
    ordinary.language = Some("eng".into());
    for role in [2, 5] {
        let source = candidate(888, "eng", role);
        let rows = [embedded(), ordinary.clone()];
        assert_eq!(
            selected(&rows, &[&source], &["eng"], ChannelSubtitleMode::Forced),
            Some((102, None))
        );
        assert_eq!(
            selected(&rows, &[&source], &["eng"], ChannelSubtitleMode::Default),
            Some((102, None))
        );
        assert!(!rows[0].default);
        assert!(!rows[0].forced);
    }
    let mut original = embedded();
    original.default = true;
    original.forced = true;
    let source = candidate(888, "eng", 5);
    let rows = [original];
    let choices = metadata_choices(&rows, 20, StreamingMode::TransportStream, false, &[&source]);
    assert!(choices[0].row.default && choices[0].row.forced);
    assert!(source.is_sdh());
}

#[test]
fn affirmative_sdh_role_preserves_real_metadata_without_changing_identity_or_flags() {
    let mut row = embedded();
    row.sdh = true;
    row.default = true;
    row.forced = true;
    row.is_extracted = true;
    row.path = Some("existing-extraction.srt".into());
    let original = row.clone();
    let ordinary_role = candidate(888, "eng", 2);
    let selected = selected_metadata(&row, Some(&ordinary_role));
    assert!(selected.sdh && selected.default && selected.forced);
    assert_eq!(selected.id, original.id);
    assert_eq!(selected.title, original.title);
    assert_eq!(selected.codec, original.codec);
    assert_eq!(selected.path, original.path);
    assert_eq!(selected.is_extracted, original.is_extracted);
    assert_eq!(selected.language.as_deref(), Some("eng"));
    row.sdh = false;
    let sdh_role = candidate(888, "eng", 5);
    assert!(selected_metadata(&row, Some(&sdh_role)).sdh);
    assert!(!selected_metadata(&row, Some(&ordinary_role)).sdh);
}

#[test]
fn extracted_teletext_cannot_bypass_missing_or_ambiguous_exact_page_qualification() {
    let mut row = embedded();
    row.is_extracted = true;
    row.path = Some("existing-extraction.srt".into());
    let rows = [row, sidecar()];
    let first = candidate(888, "eng", 2);
    let other = candidate(889, "eng", 2);
    assert_eq!(
        selected(&rows, &[], &[], ChannelSubtitleMode::Any),
        Some((102, None))
    );
    assert_eq!(
        selected(&rows, &[&first, &other], &[], ChannelSubtitleMode::Any),
        Some((102, None))
    );
}

#[test]
fn qualified_choice_bound_is_inclusive_and_not_a_page_inventory_claim() {
    assert!(check_qualified_count(MAX_QUALIFIED_SOURCE_CAPTIONS).is_ok());
    assert!(check_qualified_count(MAX_QUALIFIED_SOURCE_CAPTIONS + 1).is_err());
}

#[test]
fn conflicting_pages_programs_roles_and_files_exclude_source_but_keep_sidecar() {
    let first = candidate(888, "eng", 2);
    let rows = [embedded(), sidecar()];
    for field in 0..4 {
        let mut other = first.clone();
        match field {
            0 => other = candidate(889, "eng", 2),
            1 => other.request.program_number = 2,
            2 => other = candidate(888, "eng", 5),
            _ => other.request.binding.media_file_id += 1,
        }
        assert_eq!(
            selected(&rows, &[&first, &other], &[], ChannelSubtitleMode::Any),
            Some((102, None))
        );
    }
}

#[test]
fn equivalent_duplicate_choices_fold_but_distinct_languages_rank_normally() {
    let first = candidate(888, "eng", 2);
    let duplicate = first.clone();
    let dutch = candidate(889, "nld", 2);
    let rows = [embedded(), sidecar()];
    assert_eq!(
        selected(
            &rows,
            &[&first, &duplicate, &dutch],
            &["eng"],
            ChannelSubtitleMode::Any
        ),
        Some((101, Some(0)))
    );
    assert_eq!(
        selected(
            &rows,
            &[&first, &duplicate, &dutch],
            &["nld"],
            ChannelSubtitleMode::Any
        ),
        Some((101, Some(2)))
    );
}

#[test]
fn equivalent_iso_aliases_fold_one_page_but_do_not_hide_different_pages() {
    let french = candidate(888, "fra", 2);
    let french_alias = candidate(888, "fre", 2);
    let rows = [embedded(), sidecar()];
    for sources in [[&french, &french_alias], [&french_alias, &french]] {
        assert_eq!(
            selected(&rows, &sources, &["fr"], ChannelSubtitleMode::Any),
            Some((101, Some(0)))
        );
    }
    let other_page = candidate(889, "fre", 2);
    assert_eq!(
        selected(
            &rows,
            &[&french, &other_page],
            &[],
            ChannelSubtitleMode::Any
        ),
        Some((102, None))
    );
    assert_eq!(french.language.as_str(), "fra");
    assert_eq!(french_alias.language.as_str(), "fre");
}

#[test]
fn foreign_version_and_ambiguous_metadata_never_gain_source_eligibility() {
    let mut foreign = candidate(888, "eng", 2);
    foreign.request.binding.media_version_id = 21;
    let rows = [embedded(), sidecar()];
    assert_eq!(
        selected(&rows, &[&foreign], &[], ChannelSubtitleMode::Any),
        Some((102, None))
    );
    let source = candidate(888, "eng", 2);
    let mut duplicate = embedded();
    duplicate.id = 103;
    assert_eq!(
        selected(
            &[embedded(), duplicate, sidecar()],
            &[&source],
            &[],
            ChannelSubtitleMode::Any
        ),
        Some((102, None))
    );
    let mut bitmap = embedded();
    bitmap.codec = Some("hdmv_pgs_subtitle".into());
    assert_eq!(
        selected(
            &[bitmap, sidecar()],
            &[&source],
            &[],
            ChannelSubtitleMode::Any
        ),
        Some((101, None))
    );
}

#[test]
fn direct_preserves_ordinary_candidates_without_enabling_qualified_sources() {
    let rows = [embedded(), sidecar()];
    let source = candidate(888, "eng", 2);
    let choices = metadata_choices(
        &rows,
        20,
        StreamingMode::HttpLiveStreamingDirect,
        false,
        &[&source],
    );
    assert_eq!(choices.len(), 2);
    assert!(choices.iter().all(|choice| choice.source.is_none()));
    assert_eq!(choices[0].language, Some("eng,eng"));
}

#[tokio::test]
async fn explicit_off_returns_before_any_sql_on_a_closed_pool() {
    let pool = sqlx::sqlite::SqlitePoolOptions::new()
        .connect_lazy("sqlite::memory:")
        .unwrap();
    pool.close().await;
    let selected = select_native_playback_subtitle_for_version(
        &pool,
        1,
        2,
        &["eng".into()],
        ChannelSubtitleMode::None,
        StreamingMode::TransportStream,
        &[],
    )
    .await
    .unwrap();
    assert!(selected.is_none());
}
