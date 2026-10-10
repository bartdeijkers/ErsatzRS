use super::*;

fn projected(language: &str, bytes: &[u8]) -> ProjectedTeletextDescriptors {
    let result = project_packed_descriptors(
        Some(language),
        Some(PackedTeletextData::Bytes(bytes)),
        Some(bytes.len()),
    );
    let PackedTeletextProjection::Available(projected) = result else {
        panic!("expected usable fixture projection: {result:?}");
    };
    projected
}

fn ffprobe9_dump(bytes: &[u8]) -> String {
    let mut dump = String::from("\n");
    for chunk in bytes.chunks(60) {
        dump.push_str(&STANDARD.encode(chunk));
        dump.push('\n');
    }
    dump
}

#[test]
fn packed_projection_preserves_exact_200_and_888_language_order() {
    let result = projected("ENG,nld", &[0x0a, 0x00, 0x10, 0x88]);
    assert_eq!(result.entries.len(), 2);
    let first = &result.entries[0];
    assert_eq!(first.projection_ordinal, 0);
    assert_eq!(first.language.as_str(), "eng");
    assert_eq!(first.descriptor_type, 1);
    assert_eq!(first.wire_magazine, 2);
    assert_eq!(first.wire_page, 0);
    assert_eq!(first.display_page.unwrap().get(), 200);
    let second = &result.entries[1];
    assert_eq!(second.projection_ordinal, 1);
    assert_eq!(second.language.as_str(), "nld");
    assert_eq!(second.descriptor_type, 2);
    assert_eq!(second.wire_magazine, 0);
    assert_eq!(second.wire_page, 0x88);
    assert_eq!(second.display_page.unwrap().get(), 888);
    assert_eq!(result.warning, None);
}

#[test]
fn magazine_zero_maps_to_eight_without_changing_wire_identity() {
    let result = projected("und", &[0x08, 0x01]);
    assert_eq!(result.entries[0].wire_magazine, 0);
    assert_eq!(result.entries[0].display_page.unwrap().get(), 801);
    assert_eq!(result.entries[0].language.as_str(), "und");
}

#[test]
fn descriptor_type_five_is_independent_of_ordinary_subtitle_dispositions() {
    // No default/forced flags are inputs: this is exactly the five wire bits.
    let result = projected("eng", &[0x28, 0x88]);
    assert_eq!(result.entries[0].descriptor_type, 5);
    assert_eq!(result.entries[0].display_page.unwrap().get(), 888);
}

#[test]
fn reserved_types_and_hexadecimal_non_display_pages_remain_raw() {
    let result = projected("eng,eng,eng", &[0xff, 0xaf, 0x00, 0x9f, 0x08, 0xf9]);
    assert_eq!(result.entries[0].descriptor_type, 31);
    assert_eq!(result.entries[0].wire_magazine, 7);
    assert_eq!(result.entries[0].wire_page, 0xaf);
    assert_eq!(result.entries[1].descriptor_type, 0);
    assert!(result
        .entries
        .iter()
        .all(|entry| entry.display_page.is_none()));
}

#[test]
fn missing_metadata_is_distinct_from_an_explicit_empty_projection() {
    use TeletextMetadataUnavailable as Missing;
    let cases = [
        (None, None, None, Missing::NotProvided),
        (
            None,
            Some(PackedTeletextData::Bytes(&[])),
            Some(0),
            Missing::MissingLanguage,
        ),
        (Some(""), None, Some(0), Missing::MissingExtradata),
        (
            Some(""),
            Some(PackedTeletextData::Bytes(&[])),
            None,
            Missing::MissingExtradataSize,
        ),
    ];
    for (language, data, size, reason) in cases {
        assert_eq!(
            project_packed_descriptors(language, data, size),
            PackedTeletextProjection::Unavailable(reason),
        );
    }
    for data in [
        PackedTeletextData::Bytes(&[]),
        PackedTeletextData::Ffprobe9Base64("\n"),
    ] {
        assert_eq!(
            project_packed_descriptors(Some(""), Some(data), Some(0)),
            PackedTeletextProjection::Available(ProjectedTeletextDescriptors {
                entries: Vec::new(),
                warning: None,
            }),
        );
    }
}

#[test]
fn an_equal_count_replacement_projects_only_the_supplied_latest_snapshot() {
    let earlier = projected("eng,nld", &[0x0a, 0x00, 0x10, 0x88]);
    let later = projected("deu,fra", &[0x0b, 0x01, 0x14, 0x99]);
    assert_eq!(earlier.entries[0].display_page.unwrap().get(), 200);
    assert_eq!(later.entries.len(), 2);
    assert_eq!(later.entries[0].language.as_str(), "deu");
    assert_eq!(later.entries[0].display_page.unwrap().get(), 301);
    assert_eq!(later.entries[1].language.as_str(), "fra");
    assert_eq!(later.entries[1].display_page.unwrap().get(), 499);
    assert_eq!(later.warning, None);
}

#[test]
fn a_shorter_ffmpeg_replacement_keeps_only_the_language_associated_prefix() {
    // n9.0 leaves pair two from a preceding descriptor in its original buffer.
    let result = projected("deu", &[0x0b, 0x01, 0x10, 0x88]);
    assert_eq!(result.entries.len(), 1);
    assert_eq!(result.entries[0].language.as_str(), "deu");
    assert_eq!(result.entries[0].display_page.unwrap().get(), 301);
    assert_eq!(
        result.warning,
        Some(TeletextProjectionWarning::TrailingUnpairedPairs { count: 1 })
    );
}

#[test]
fn an_unobservable_rejected_larger_descriptor_is_not_reconstructed() {
    // A larger later descriptor can fail inside FFmpeg and leave this earlier
    // snapshot. The decoder cannot infer either that event or missing entries.
    let result = projected("eng", &[0x0a, 0x00]);
    assert_eq!(result.entries.len(), 1);
    assert_eq!(result.entries[0].display_page.unwrap().get(), 200);
    assert_eq!(result.warning, None);
}

#[test]
fn explicit_base64_encoding_matches_the_decoded_fixture() {
    let result = project_packed_descriptors(
        Some("eng,eng"),
        Some(PackedTeletextData::Ffprobe9Base64("\nCgAQiA==\n")),
        Some(4),
    );
    assert_eq!(
        result,
        PackedTeletextProjection::Available(projected("eng,eng", &[0x0a, 0, 0x10, 0x88]))
    );
}

#[test]
fn wrapped_base64_crosses_the_sixty_byte_boundary_without_losing_ordinals() {
    // 64 entries exceed a single wire descriptor's 51-entry ceiling; the API's
    // resource bound is independent of that ceiling and makes no inventory claim.
    let bytes = [0x0a, 0x01].repeat(64);
    let language = vec!["eng"; 64].join(",");
    let dump = ffprobe9_dump(&bytes);
    assert_eq!(
        dump.lines().map(str::len).collect::<Vec<_>>(),
        vec![0, 80, 80, 12]
    );
    let result = project_packed_descriptors(
        Some(&language),
        Some(PackedTeletextData::Ffprobe9Base64(&dump)),
        Some(bytes.len()),
    );
    assert_eq!(
        result,
        PackedTeletextProjection::Available(projected(&language, &bytes))
    );
}

#[test]
fn encoded_text_requires_documented_framing_and_canonical_padding() {
    for dump in [
        "CgAQiA==",
        "CgAQiA==\n",
        "\nCgAQiA==",
        "\n\n",
        "\nCgAQ\niA==\n",
        "\nCgAQiA==\r\n",
        "\nCgAQiA== \n",
    ] {
        assert_eq!(
            project_packed_descriptors(
                Some("eng,eng"),
                Some(PackedTeletextData::Ffprobe9Base64(dump)),
                Some(4)
            ),
            PackedTeletextProjection::Unusable(TeletextProjectionProblem::InvalidBase64Framing),
            "unsupported framing: {dump:?}",
        );
    }
    for dump in [
        "\nCgAQiA??\n",
        "\nCgAQiB==\n",
        "\nCgAQiA--\n",
        "\nCgAQiA==AAAA\n",
    ] {
        assert_eq!(
            project_packed_descriptors(
                Some("eng,eng"),
                Some(PackedTeletextData::Ffprobe9Base64(dump)),
                Some(4)
            ),
            PackedTeletextProjection::Unusable(TeletextProjectionProblem::InvalidBase64),
            "unusable encoding: {dump:?}",
        );
    }
}

#[test]
fn padded_short_chunks_cannot_appear_before_the_final_base64_line() {
    let short_full_width = STANDARD.encode([0u8; 58]);
    assert_eq!(short_full_width.len(), 80);
    let dump = format!("\n{short_full_width}\nCgA=\n");
    assert_eq!(
        project_packed_descriptors(
            Some("eng"),
            Some(PackedTeletextData::Ffprobe9Base64(&dump)),
            Some(60)
        ),
        PackedTeletextProjection::Unusable(TeletextProjectionProblem::InvalidBase64Framing),
    );
}

#[test]
fn inconsistent_projection_inputs_produce_static_typed_problems() {
    let cases: &[(&str, &[u8], usize, TeletextProjectionProblem)] = &[
        (
            "eng",
            &[0x0a, 0],
            1,
            TeletextProjectionProblem::ExtradataSizeMismatch,
        ),
        ("eng", &[0x0a], 1, TeletextProjectionProblem::OddByteCount),
        (
            "eng,nld",
            &[0x0a, 0],
            2,
            TeletextProjectionProblem::MoreLanguagesThanPairs,
        ),
        (
            "",
            &[0x0a, 0],
            2,
            TeletextProjectionProblem::EmptyLanguageForPairs,
        ),
        (
            "en",
            &[0x0a, 0],
            2,
            TeletextProjectionProblem::InvalidLanguage,
        ),
        (
            "eng,",
            &[0x0a, 0, 0x10, 0x88],
            4,
            TeletextProjectionProblem::InvalidLanguage,
        ),
        (
            "n1d",
            &[0x0a, 0],
            2,
            TeletextProjectionProblem::InvalidLanguage,
        ),
        (
            "ééé",
            &[0x0a, 0],
            2,
            TeletextProjectionProblem::InvalidLanguage,
        ),
        (
            " eng",
            &[0x0a, 0],
            2,
            TeletextProjectionProblem::InvalidLanguage,
        ),
        (
            "eng",
            b"eng\x0a\x00",
            5,
            TeletextProjectionProblem::OddByteCount,
        ),
    ];
    for &(language, bytes, size, problem) in cases {
        assert_eq!(
            project_packed_descriptors(
                Some(language),
                Some(PackedTeletextData::Bytes(bytes)),
                Some(size)
            ),
            PackedTeletextProjection::Unusable(problem),
        );
    }
}

#[test]
fn resource_bound_accepts_exact_limit_for_decoded_and_encoded_data() {
    let bytes = [0x0a, 0x00].repeat(MAX_PROJECTED_TELETEXT_ENTRIES);
    let language = vec!["eng"; MAX_PROJECTED_TELETEXT_ENTRIES].join(",");
    assert_eq!(bytes.len(), MAX_PACKED_TELETEXT_BYTES);
    assert_eq!(language.len(), MAX_TELETEXT_LANGUAGE_CSV_BYTES);
    let dump = ffprobe9_dump(&bytes);
    assert_eq!(dump.len(), MAX_FFPROBE9_TELETEXT_BASE64_BYTES);
    let decoded = projected(&language, &bytes);
    assert_eq!(decoded.entries.len(), MAX_PROJECTED_TELETEXT_ENTRIES);
    assert_eq!(
        project_packed_descriptors(
            Some(&language),
            Some(PackedTeletextData::Ffprobe9Base64(&dump)),
            Some(bytes.len())
        ),
        PackedTeletextProjection::Available(decoded)
    );
}

#[test]
fn oversized_inputs_are_rejected_before_normalization_or_decode() {
    let huge_csv = "e".repeat(MAX_TELETEXT_LANGUAGE_CSV_BYTES + 1);
    let huge_bytes = vec![0; MAX_PACKED_TELETEXT_BYTES + 2];
    let huge_dump = "A".repeat(MAX_FFPROBE9_TELETEXT_BASE64_BYTES + 1);
    assert_eq!(
        project_packed_descriptors(
            Some(&huge_csv),
            Some(PackedTeletextData::Bytes(&[])),
            Some(0)
        ),
        PackedTeletextProjection::Unusable(TeletextProjectionProblem::LanguageCsvTooLarge),
    );
    assert_eq!(
        project_packed_descriptors(
            Some("eng"),
            Some(PackedTeletextData::Bytes(&huge_bytes)),
            Some(0)
        ),
        PackedTeletextProjection::Unusable(TeletextProjectionProblem::PackedDataTooLarge),
    );
    assert_eq!(
        project_packed_descriptors(
            Some("eng"),
            Some(PackedTeletextData::Bytes(&[])),
            Some(usize::MAX)
        ),
        PackedTeletextProjection::Unusable(TeletextProjectionProblem::PackedDataTooLarge),
    );
    assert_eq!(
        project_packed_descriptors(
            Some("eng"),
            Some(PackedTeletextData::Ffprobe9Base64(&huge_dump)),
            Some(2)
        ),
        PackedTeletextProjection::Unusable(TeletextProjectionProblem::EncodedDataTooLarge),
    );
}

#[test]
fn default_xxd_dump_is_not_guessed_as_base64() {
    assert_eq!(
        project_packed_descriptors(
            Some("eng,eng"),
            Some(PackedTeletextData::Ffprobe9Base64(
                "\n00000000: 0a00 1088                                ....\n"
            )),
            Some(4)
        ),
        PackedTeletextProjection::Unusable(TeletextProjectionProblem::InvalidBase64Framing),
    );
}

#[test]
fn problem_display_never_contains_supplied_dump_or_language() {
    let PackedTeletextProjection::Unusable(problem) = project_packed_descriptors(
        Some("private-language"),
        Some(PackedTeletextData::Bytes(&[0x0a, 0])),
        Some(2),
    ) else {
        panic!("fixture must fail language validation");
    };
    assert_eq!(
        problem.to_string(),
        "teletext projection contains an unusable language code"
    );
}
