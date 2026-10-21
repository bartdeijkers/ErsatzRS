use ersatzrs_domain::*;

use super::*;

pub(super) fn draft(number: u16, subcode: u16, caption: bool) -> TeletextPageDraft {
    TeletextPageDraft {
        address: TeletextPageAddress {
            page: TeletextPageNumber::new(number).unwrap(),
            subcode: TeletextSubcode::new(subcode).unwrap(),
        },
        kind: if caption {
            TeletextPageKind::Subtitle
        } else {
            TeletextPageKind::FullPage
        },
        language: TeletextLanguage::new("nld".into()).unwrap(),
        national_option: TeletextNationalOption::default(),
        rows: vec![TeletextRow {
            row: 1,
            runs: vec![TeletextRun {
                column: if caption { 2 } else { 1 },
                style: TeletextStyle {
                    boxed: caption,
                    ..TeletextStyle::default()
                },
                content: TeletextRunContent::Text {
                    text: if caption {
                        "PES caption"
                    } else {
                        "PES café Noël"
                    }
                    .into(),
                },
            }],
        }],
        navigation: Vec::new(),
    }
}

pub(super) fn set(pages: Vec<TeletextPageDraft>) -> EncodedTeletextPageSet {
    encode_teletext_page_set(
        &TeletextPageSetDraft {
            initial_page: pages[0].address.page,
            language: pages[0].language.clone(),
            pages,
        },
        TeletextEncodingRegion::WesternLatin,
    )
    .unwrap()
}

fn packet_number(unit: &[u8; 46]) -> u8 {
    let values: Vec<u8> = (0..16).map(super::packets::hamming_8_4).collect();
    let first = values
        .iter()
        .position(|&value| value == unit[4].reverse_bits())
        .unwrap();
    let second = values
        .iter()
        .position(|&value| value == unit[5].reverse_bits())
        .unwrap();
    u8::try_from((first | (second << 4)) >> 3).unwrap()
}

#[test]
fn carousel_preserves_subpages_order_erasure_interval_and_ff_termination() {
    let encoded = set(vec![
        draft(200, 1, false),
        draft(200, 2, false),
        draft(888, 0, true),
    ]);
    let expected: Vec<_> = encoded
        .pages()
        .iter()
        .flat_map(EncodedTeletextPage::data_units)
        .copied()
        .collect();
    let mut carousel = TeletextCarousel::new(encoded);
    assert!(carousel.is_cycle_boundary());
    let mut actual = Vec::new();
    let mut previous = None;
    loop {
        let batch = carousel.next_batch().unwrap();
        if let Some(previous) = previous {
            assert_eq!(batch.presentation_ticks() - previous, 1800);
        }
        previous = Some(batch.presentation_ticks());
        assert!(!batch.data_units().is_empty() && batch.data_units().len() <= 31);
        if batch.kind() == TeletextCarouselBatchKind::Terminator {
            let unit = &batch.data_units()[0];
            assert_eq!(packet_number(unit), 0);
            assert_eq!(unit[6].reverse_bits(), super::packets::hamming_8_4(15));
            assert_eq!(unit[7].reverse_bits(), super::packets::hamming_8_4(15));
            assert!(carousel.is_cycle_boundary());
            break;
        }
        assert!(!carousel.is_cycle_boundary());
        if batch.kind() == TeletextCarouselBatchKind::Header {
            assert_eq!(batch.data_units().len(), 1);
            assert_eq!(packet_number(&batch.data_units()[0]), 0);
        }
        actual.extend_from_slice(batch.data_units());
    }
    assert_eq!(actual, expected);
}

#[test]
fn packet_order_puts_links_and_character_tables_before_display_rows() {
    let mut page = draft(200, 0, false);
    page.navigation.push(TeletextNavigationLink {
        role: TeletextNavigationRole::Index,
        target: TeletextNavigationTarget {
            page: page.address.page,
            subcode: None,
        },
    });
    let encoded = set(vec![page]);
    let numbers: Vec<_> = encoded.pages()[0]
        .data_units()
        .iter()
        .map(packet_number)
        .collect();
    assert_eq!(&numbers[..4], &[0, 27, 28, 26]);
    assert_eq!(&numbers[4..], &(1_u8..25).collect::<Vec<_>>());
}

#[test]
fn replacements_cannot_mix_generations_or_reset_the_programme_clock() {
    let original = set(vec![draft(200, 0, false)]);
    let replacement = set(vec![draft(201, 0, false)]);
    let expected = replacement.pages()[0].data_units()[0];
    let mut carousel = TeletextCarousel::new(original);
    assert_eq!(carousel.next_batch().unwrap().presentation_ticks(), 0);
    assert_eq!(
        carousel.replace_at_boundary(replacement.clone()),
        Err(TeletextTransportError::MidCycleReplacement)
    );
    assert_eq!(
        carousel.next_batch().unwrap().kind(),
        TeletextCarouselBatchKind::Page
    );
    assert_eq!(
        carousel.next_batch().unwrap().kind(),
        TeletextCarouselBatchKind::Terminator
    );
    carousel.replace_at_boundary(replacement).unwrap();
    let batch = carousel.next_batch().unwrap();
    assert_eq!(batch.presentation_ticks(), 5400);
    assert_eq!(batch.data_units(), &[expected]);
}

fn decoded_header_nibble(unit: &[u8; 46], column: usize) -> u8 {
    // EN 300 706 Hamming 8/4 codewords, independent of the encoder function.
    const CODEWORDS: [u8; 16] = [
        0x15, 0x02, 0x49, 0x5e, 0x64, 0x73, 0x38, 0x2f, 0xd0, 0xc7, 0x8c, 0x9b, 0xa1, 0xb6, 0xfd,
        0xea,
    ];
    u8::try_from(
        CODEWORDS
            .iter()
            .position(|&word| word == unit[6 + column].reverse_bits())
            .expect("valid Hamming-protected page header"),
    )
    .unwrap()
}

fn decoded_header_subcode(unit: &[u8; 46]) -> u16 {
    u16::from(decoded_header_nibble(unit, 2))
        | (u16::from(decoded_header_nibble(unit, 3) & 7) << 4)
        | (u16::from(decoded_header_nibble(unit, 4)) << 8)
        | (u16::from(decoded_header_nibble(unit, 5) & 3) << 12)
}

fn carousel_cycle_headers(carousel: &mut TeletextCarousel) -> Vec<[u8; 46]> {
    let mut headers = Vec::new();
    loop {
        let batch = carousel.next_batch().unwrap();
        match batch.kind() {
            TeletextCarouselBatchKind::Header => headers.push(batch.data_units()[0]),
            TeletextCarouselBatchKind::Terminator => return headers,
            TeletextCarouselBatchKind::Page | TeletextCarouselBatchKind::Magazine => (),
        }
    }
}

#[test]
fn initial_headers_erase_once_and_repetitions_retain_subcodes_and_all_other_flags() {
    for subcode in [0, 0x15, 0x3f7e] {
        for kind in [
            TeletextPageKind::FullPage,
            TeletextPageKind::Newsflash,
            TeletextPageKind::Subtitle,
        ] {
            let mut page = draft(200, subcode, kind != TeletextPageKind::FullPage);
            page.kind = kind;
            page.national_option = TeletextNationalOption::new(3).unwrap();
            let encoded = set(vec![page]);
            let original = encoded.pages()[0].data_units()[0];
            let mut carousel = TeletextCarousel::new(encoded);
            assert_eq!(carousel_cycle_headers(&mut carousel), vec![original]);
            assert_ne!(decoded_header_nibble(&original, 3) & 8, 0);
            for _ in 0..3 {
                let headers = carousel_cycle_headers(&mut carousel);
                assert_eq!(headers.len(), 1);
                let repeated = headers[0];
                assert_eq!(decoded_header_nibble(&repeated, 3) & 8, 0);
                assert_eq!(decoded_header_subcode(&repeated), subcode);
                for index in 0..46 {
                    if index != 9 {
                        assert_eq!(repeated[index], original[index]);
                    }
                }
            }
        }
    }
}

#[test]
fn every_subpage_erases_in_the_first_cycle_then_repeats_without_mutating_encoded_units() {
    let encoded = set(vec![draft(200, 0, false), draft(200, 0x23, false)]);
    let originals: Vec<_> = encoded
        .pages()
        .iter()
        .map(|page| page.data_units()[0])
        .collect();
    let mut carousel = TeletextCarousel::new(encoded.clone());
    assert_eq!(carousel_cycle_headers(&mut carousel), originals);
    let repeated = carousel_cycle_headers(&mut carousel);
    for (header, subcode) in repeated.iter().zip([0, 0x23]) {
        assert_eq!(decoded_header_nibble(header, 3) & 8, 0);
        assert_eq!(decoded_header_subcode(header), subcode);
    }
    carousel.replace_at_boundary(encoded).unwrap();
    // Replacement resets first-cycle erasure using the same encoded value;
    // copied repeat headers must not have mutated that retained page set.
    assert_eq!(carousel_cycle_headers(&mut carousel), originals);
}

#[test]
fn replacement_restarts_first_cycle_erasure_for_zero_and_nonzero_subcodes_without_clock_reset() {
    for subcode in [0, 0x2a, 0x3f7e] {
        let mut carousel = TeletextCarousel::new(set(vec![draft(200, subcode, false)]));
        carousel_cycle_headers(&mut carousel);
        carousel_cycle_headers(&mut carousel);
        let replacement = set(vec![draft(201, subcode, false)]);
        let expected = replacement.pages()[0].data_units()[0];
        carousel.replace_at_boundary(replacement).unwrap();
        let header = carousel.next_batch().unwrap();
        assert_eq!(header.presentation_ticks(), 10_800);
        assert_eq!(header.data_units(), &[expected]);
        assert_ne!(decoded_header_nibble(&header.data_units()[0], 3) & 8, 0);
        assert_eq!(decoded_header_subcode(&header.data_units()[0]), subcode);
        assert_eq!(
            carousel.next_batch().unwrap().kind(),
            TeletextCarouselBatchKind::Page
        );
        assert_eq!(
            carousel.next_batch().unwrap().kind(),
            TeletextCarouselBatchKind::Terminator
        );
        assert_eq!(
            decoded_header_nibble(&carousel.next_batch().unwrap().data_units()[0], 3) & 8,
            0
        );
    }
}

#[test]
fn timestamp_wrap_is_explicit_and_reserved_pids_are_rejected() {
    assert_eq!(
        TeletextTimestamp::new(1 << 33),
        Err(TeletextTransportError::Timestamp)
    );
    let end = TeletextTimestamp::new((1 << 33) - 1).unwrap();
    assert_eq!(end.wrapping_add(2).ticks(), 1);
    assert_eq!(end.wrapping_add(u64::MAX).ticks(), (1 << 33) - 2);
    for pid in [0, 1, 31, 0x1fff, 0x2000, u16::MAX] {
        assert!(matches!(
            TeletextPacketizer::new(pid),
            Err(TeletextTransportError::Pid)
        ));
    }
    for pid in [32, 256, 0x1ffe] {
        assert_eq!(TeletextPacketizer::new(pid).unwrap().pid(), pid);
    }
}

#[test]
fn private_pes_has_fixed_header_exact_pts_and_structured_stuffing() {
    let mut carousel = TeletextCarousel::new(set(vec![draft(200, 0, false)]));
    let header = carousel.next_batch().unwrap();
    let mut packetizer = TeletextPacketizer::new(0x100).unwrap();
    let packets = packetizer.packetize(&header, TeletextTimestamp::new((1 << 33) - 1).unwrap());
    assert_eq!(packets.len(), 1);
    assert_eq!(&packets[0][..4], &[0x47, 0x41, 0, 0x10]);
    let pes = &packets[0][4..];
    assert_eq!(&pes[..9], &[0, 0, 1, 0xbd, 0, 178, 0x84, 0x80, 0x24]);
    assert_eq!(&pes[9..14], &[0x2f, 255, 255, 255, 255]);
    assert!(pes[14..45].iter().all(|&byte| byte == 255));
    assert_eq!(pes[45], 0x10);
    assert_eq!(&pes[46..92], &header.data_units()[0]);
    for unit in pes[92..].chunks_exact(46) {
        assert_eq!(&unit[..2], &[255, 44]);
        assert!(unit[2..].iter().all(|&byte| byte == 255));
    }
}

#[test]
fn largest_page_is_split_into_bounded_pes_without_losing_units_or_continuity() {
    let mut page = draft(200, 0, false);
    page.rows = (1..=5)
        .map(|row| TeletextRow {
            row,
            runs: vec![TeletextRun {
                column: 0,
                style: TeletextStyle::default(),
                content: TeletextRunContent::Text {
                    text: "é".repeat(40),
                },
            }],
        })
        .collect();
    page.rows.push(TeletextRow {
        row: 6,
        runs: vec![TeletextRun {
            column: 0,
            style: TeletextStyle::default(),
            content: TeletextRunContent::Text { text: "é".into() },
        }],
    });
    let mut carousel = TeletextCarousel::new(set(vec![page]));
    let mut packetizer = TeletextPacketizer::new(0x1ffe).unwrap();
    let mut counter = 0_u8;
    let mut largest = 0;
    for _ in 0..20 {
        let batch = carousel.next_batch().unwrap();
        let packets = packetizer.packetize(
            &batch,
            TeletextTimestamp::new(batch.presentation_ticks()).unwrap(),
        );
        largest = largest.max(packets.len());
        assert!(packets.len() <= 8);
        let mut pes = Vec::new();
        for (index, packet) in packets.iter().enumerate() {
            assert_eq!(packet[1] & 0x40 != 0, index == 0);
            assert_eq!(packet[3], 0x10 | counter);
            counter = (counter + 1) & 15;
            pes.extend_from_slice(&packet[4..]);
        }
        assert_eq!(
            usize::from(u16::from_be_bytes([pes[4], pes[5]])),
            pes.len() - 6
        );
        let actual: Vec<_> = pes[46..]
            .chunks_exact(46)
            .filter(|unit| unit[0] != 255)
            .collect();
        assert_eq!(actual.len(), batch.data_units().len());
        for (actual, expected) in actual.iter().zip(batch.data_units()) {
            assert_eq!(*actual, expected);
        }
    }
    assert_eq!(largest, 8);
}

async fn packetized_fixture() -> Vec<u8> {
    let python = crate::locator::first_non_empty_env(&["ERSATZRS_TELETEXT_TEST_PYTHON"])
        .expect("set the Python executable");
    let script = test_manifest_dir!().join("../../scripts/calibration/wrap-authored-teletext.py");
    let mut carousel = TeletextCarousel::new(set(vec![draft(200, 1, false), draft(888, 0, true)]));
    let mut packetizer = TeletextPacketizer::new(0x100).unwrap();
    let anchor = TeletextTimestamp::new(90_000).unwrap();
    let mut raw = Vec::new();
    let mut cycles = 0;
    while cycles < 8 {
        let batch = carousel.next_batch().unwrap();
        raw.extend(
            packetizer
                .packetize(&batch, anchor.wrapping_add(batch.presentation_ticks()))
                .into_iter()
                .flatten(),
        );
        if batch.kind() == TeletextCarouselBatchKind::Terminator {
            cycles += 1;
        }
    }
    let stream = crate::teletext_probe::run_probe(
        &python,
        &[script.to_str().unwrap(), "--packetized"],
        &raw,
        128 * 1024,
    )
    .await
    .unwrap();
    if let Some(path) = crate::locator::first_non_empty_env(&["ERSATZRS_TELETEXT_TEST_ARTIFACT"]) {
        std::fs::write(path, &stream).unwrap();
    }
    stream
}

#[tokio::test]
#[ignore = "requires explicit FFmpeg 9/libzvbi and Python executables"]
async fn real_runtime_decodes_carousel_and_packetized_pes() {
    let ffmpeg = crate::locator::first_non_empty_env(&["ERSATZRS_TELETEXT_TEST_FFMPEG"])
        .expect("set the exact FFmpeg executable");
    let stream = packetized_fixture().await;
    let output = crate::teletext_probe::run_probe(
        &ffmpeg,
        crate::teletext_probe::TEXT_PROBE_ARGUMENTS,
        &stream,
        64 * 1024,
    )
    .await
    .unwrap();
    let text = String::from_utf8(output).unwrap();
    assert!(
        text.contains("PES café Noël"),
        "authored page missing: {text}"
    );
    assert!(text.contains("PES caption"), "caption missing: {text}");
}

#[tokio::test]
#[ignore = "requires explicit TSDuck tsanalyze and Python executables"]
async fn real_tsduck_checks_packetized_pes_and_detects_counter_damage() {
    let tsanalyze = crate::locator::first_non_empty_env(&["ERSATZRS_TELETEXT_TEST_TSANALYZE"])
        .expect("set the TSDuck tsanalyze executable");
    let stream = packetized_fixture().await;
    let report = analyze(&tsanalyze, &stream).await;
    assert_eq!(report["ts"]["packets"]["invalid-syncs"], 0);
    assert_eq!(report["ts"]["packets"]["transport-errors"], 0);
    assert_eq!(report["ts"]["pids"]["unreferenced"], 0);
    let pid = report["pids"]
        .as_array()
        .unwrap()
        .iter()
        .find(|pid| pid["id"] == 256)
        .unwrap();
    assert_eq!(pid["pes-stream-id"], 0xbd);
    assert_eq!(pid["invalid-pes-prefix"], 0);
    assert_eq!(pid["packets"]["discontinuities"], 0);
    assert_eq!(pid["packets"]["pts-leap"], 0);
    assert_eq!(pid["packets"]["pcr-leap"], 0);
    assert!(pid["pes"].as_u64().unwrap() > 0);
    // Prove that this independent check actually detects the counter invariant.
    let mut damaged = stream.clone();
    let packet = damaged
        .chunks_exact_mut(188)
        .filter(|packet| packet[1] & 0x1f == 1 && packet[2] == 0 && packet[3] & 0x30 == 0x10)
        .nth(10)
        .unwrap();
    packet[3] ^= 3;
    let report = analyze(&tsanalyze, &damaged).await;
    let pid = report["pids"]
        .as_array()
        .unwrap()
        .iter()
        .find(|pid| pid["id"] == 256)
        .unwrap();
    assert!(pid["packets"]["discontinuities"].as_u64().unwrap() > 0);
}

async fn analyze(executable: &str, stream: &[u8]) -> serde_json::Value {
    let output = crate::teletext_probe::run_probe(executable, &["--json"], stream, 64 * 1024)
        .await
        .unwrap();
    serde_json::from_slice(&output).unwrap()
}
