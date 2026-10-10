use ersatzrs_domain::*;
use mpeg2ts_reader::{mpegts_crc, packet::Packet, psi::pmt::PmtSection};

use super::*;

pub(super) fn page(number: u16, subcode: u16, language: &str, caption: bool) -> TeletextPageDraft {
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
        language: TeletextLanguage::new(language.into()).unwrap(),
        national_option: TeletextNationalOption::default(),
        rows: vec![TeletextRow {
            row: 1,
            runs: vec![TeletextRun {
                column: 2,
                style: TeletextStyle {
                    boxed: caption,
                    ..TeletextStyle::default()
                },
                content: TeletextRunContent::Text {
                    text: if caption {
                        "Signalled caption"
                    } else {
                        "Signalled café"
                    }
                    .into(),
                },
            }],
        }],
        navigation: Vec::new(),
    }
}

pub(super) fn pages(values: Vec<TeletextPageDraft>) -> EncodedTeletextPageSet {
    encode_teletext_page_set(
        &TeletextPageSetDraft {
            initial_page: values[0].address.page,
            language: values[0].language.clone(),
            pages: values,
        },
        TeletextEncodingRegion::WesternLatin,
    )
    .unwrap()
}

fn pmt(version: u8, program_descriptors: &[u8], streams: &[u8]) -> Vec<u8> {
    let mut section = vec![
        2,
        0xb0,
        0,
        0,
        1,
        0xc1 | (version << 1),
        0,
        0,
        0xe1,
        0,
        0xf0,
        u8::try_from(program_descriptors.len()).unwrap(),
    ];
    section.extend_from_slice(program_descriptors);
    section.extend_from_slice(streams);
    crc(&mut section);
    section
}

fn crc(section: &mut Vec<u8>) {
    let length = u16::try_from(section.len() + 1).unwrap().to_be_bytes();
    section[1] = 0xb0 | length[0];
    section[2] = length[1];
    section.extend_from_slice(&mpegts_crc::sum32(section).to_be_bytes());
}

fn plan() -> TeletextPmtPlan {
    let service =
        TeletextServiceDescriptor::from_page_set(&pages(vec![page(200, 0, "nld", false)]));
    TeletextPmtPlan::new(
        &pmt(0, &[], &[0x1b, 0xe1, 0, 0xf0, 0, 0x0f, 0xe1, 1, 0xf0, 0]),
        4096,
        512,
        &service,
    )
    .unwrap()
}

fn null() -> [u8; 188] {
    let mut raw = [0xff; 188];
    raw[..4].copy_from_slice(&[0x47, 0x1f, 0xff, 0x10]);
    raw
}

#[test]
fn descriptors_retain_languages_page_roles_and_deduplicate_subpages() {
    let encoded = pages(vec![
        page(200, 1, "nld", false),
        page(200, 2, "nld", false),
        page(201, 0, "nld", false),
        page(888, 1, "deu", true),
        page(888, 2, "deu", true),
    ]);
    let descriptors = TeletextServiceDescriptor::from_page_set(&encoded);
    assert_eq!(
        descriptors.bytes(),
        &[0x56, 10, b'd', b'e', b'u', 0x10, 0x88, b'n', b'l', b'd', 0x0a, 0]
    );
    let many = pages(
        (200..=299)
            .map(|number| page(number, 0, "eng", true))
            .collect(),
    );
    let descriptor = TeletextServiceDescriptor::from_page_set(&many);
    assert_eq!(descriptor.bytes()[..2], [0x56, 255]);
    assert_eq!(descriptor.bytes()[257..259], [0x56, 245]);
    assert_eq!(descriptor.bytes().len(), 504);
}

#[test]
fn pmt_preserves_existing_bytes_pcr_and_wraps_version_with_valid_crc() {
    let original = pmt(
        31,
        &[0x80, 3, 1, 2, 3],
        &[0x1b, 0xe1, 0, 0xf0, 3, 0x81, 1, 9],
    );
    let service =
        TeletextServiceDescriptor::from_page_set(&pages(vec![page(200, 0, "nld", false)]));
    let planned = TeletextPmtPlan::new(&original, 4096, 512, &service).unwrap();
    assert_eq!(planned.source_section(), &original);
    assert_eq!(planned.program_number(), 1);
    assert_eq!(planned.pcr_pid(), 256);
    assert_eq!(planned.section()[5], 0xc1);
    assert_eq!(
        &planned.section()[8..original.len() - 4],
        &original[8..original.len() - 4]
    );
    assert_eq!(mpegts_crc::sum32(planned.section()), 0);
    let parsed =
        PmtSection::from_bytes(&planned.section()[8..planned.section().len() - 4]).unwrap();
    assert_eq!(
        parsed
            .streams()
            .map(|stream| u16::from(stream.elementary_pid()))
            .collect::<Vec<_>>(),
        [256, 512]
    );
}

#[test]
fn source_pmt_failures_never_become_partial_success() {
    let service =
        TeletextServiceDescriptor::from_page_set(&pages(vec![page(200, 0, "nld", false)]));
    let valid = pmt(0, &[], &[0x1b, 0xe1, 0, 0xf0, 0]);
    for size in 0..valid.len() {
        assert!(TeletextPmtPlan::new(&valid[..size], 4096, 512, &service).is_err());
    }
    let mut broken = valid.clone();
    broken[12] ^= 1;
    assert_eq!(
        TeletextPmtPlan::new(&broken, 4096, 512, &service),
        Err(TeletextMultiplexError::Crc)
    );
    for streams in [
        &[0x1b, 0xe1, 0, 0xf0][..],
        &[0x1b, 0xe1, 0, 0xf0, 1],
        &[0x1b, 0xe1, 0, 0xf0, 0, 1],
    ] {
        assert_eq!(
            TeletextPmtPlan::new(&pmt(0, &[], streams), 4096, 512, &service),
            Err(TeletextMultiplexError::Pmt)
        );
    }
    let invalid_descriptor = pmt(0, &[], &[0x1b, 0xe1, 0, 0xf0, 1, 0x80]);
    assert_eq!(
        TeletextPmtPlan::new(&invalid_descriptor, 4096, 512, &service),
        Err(TeletextMultiplexError::Descriptor)
    );
    for offset in [5, 6, 7, 8, 10] {
        let mut broken = valid[..valid.len() - 4].to_vec();
        broken[offset] ^= if offset == 5 { 1 } else { 0x20 };
        crc(&mut broken);
        assert!(
            TeletextPmtPlan::new(&broken, 4096, 512, &service).is_err(),
            "offset {offset}"
        );
    }
}

#[test]
fn pmt_checks_elementary_clock_table_and_ca_pid_collisions() {
    let service =
        TeletextServiceDescriptor::from_page_set(&pages(vec![page(200, 0, "nld", false)]));
    let original = pmt(0, &[], &[0x1b, 0xe1, 1, 0xf0, 0]);
    for pid in [256, 257, 4096] {
        assert_eq!(
            TeletextPmtPlan::new(&original, 4096, pid, &service),
            Err(TeletextMultiplexError::PidCollision { pid })
        );
    }
    let original = pmt(0, &[9, 4, 0, 1, 0xe2, 0], &[0x1b, 0xe1, 1, 0xf0, 0]);
    assert_eq!(
        TeletextPmtPlan::new(&original, 4096, 512, &service),
        Err(TeletextMultiplexError::PidCollision { pid: 512 })
    );
    for pid in [0, 31, 8191, u16::MAX] {
        assert_eq!(
            TeletextPmtPlan::new(&original, 4096, pid, &service),
            Err(TeletextMultiplexError::Pid)
        );
    }
}

#[test]
fn oversized_signalling_is_rejected_and_multpacket_pmt_has_complete_payload() {
    let service = TeletextServiceDescriptor::from_page_set(&pages(
        (200..=399)
            .map(|number| page(number, 0, "eng", true))
            .collect(),
    ));
    assert_eq!(
        TeletextPmtPlan::new(&pmt(0, &[], &[]), 4096, 512, &service),
        Err(TeletextMultiplexError::SectionCapacity)
    );
    let service = TeletextServiceDescriptor::from_page_set(&pages(
        (200..=299)
            .map(|number| page(number, 0, "eng", true))
            .collect(),
    ));
    let planned = TeletextPmtPlan::new(&pmt(0, &[], &[]), 4096, 512, &service).unwrap();
    let packets = planned.packets(15).unwrap();
    assert_eq!(packets.len(), 3);
    let mut payload = Vec::new();
    for (index, raw) in packets.iter().enumerate() {
        let parsed = Packet::new(raw);
        assert_eq!(u16::from(parsed.pid()), 4096);
        assert_eq!(parsed.payload_unit_start_indicator(), index == 0);
        assert_eq!(
            parsed.continuity_counter().count(),
            (15 + u8::try_from(index).unwrap()) & 15
        );
        payload.extend_from_slice(parsed.payload().unwrap());
    }
    assert_eq!(payload[0], 0);
    assert_eq!(&payload[1..=planned.section().len()], planned.section());
    assert!(payload[1 + planned.section().len()..]
        .iter()
        .all(|&byte| byte == 255));
    assert_eq!(planned.packets(16), Err(TeletextMultiplexError::Continuity));
}

#[test]
fn null_capacity_failure_preserves_window_and_packetizer_counter() {
    let plan = plan();
    let mut carousel = TeletextCarousel::new(pages(vec![page(200, 0, "nld", false)]));
    let batch = carousel.next_batch().unwrap();
    let pts = TeletextTimestamp::new(90_000).unwrap();
    let mut packetizer = TeletextPacketizer::new(512).unwrap();
    let mut source = null();
    source[1] = 1;
    source[2] = 0; // Non-null AV packet.
    let mut window = [source];
    assert_eq!(
        plan.replace_null_packets(&mut window, &batch, pts, &mut packetizer),
        Err(TeletextMultiplexError::Capacity {
            needed: 1,
            available: 0
        })
    );
    assert_eq!(window, [source]);
    let mut window = [source, null(), source];
    assert_eq!(
        plan.replace_null_packets(&mut window, &batch, pts, &mut packetizer)
            .unwrap(),
        [1]
    );
    assert_eq!(window[0], source);
    assert_eq!(window[2], source);
    assert_eq!(window[1][3], 0x10);
}

#[test]
fn bad_or_occupied_packet_after_available_nulls_aborts_before_mutation() {
    let plan = plan();
    let mut carousel = TeletextCarousel::new(pages(vec![page(200, 0, "nld", false)]));
    let batch = carousel.next_batch().unwrap();
    let pts = TeletextTimestamp::new(90_000).unwrap();
    let mut packetizer = TeletextPacketizer::new(512).unwrap();
    for invalid in [
        [0, 0, 0, 0x10],
        [0x47, 0x80, 0, 0x10],
        [0x47, 0, 0, 0xd0],
        [0x47, 0, 0, 0],
        [0x47, 0, 0, 0x20],
    ] {
        let mut bad = null();
        bad[..4].copy_from_slice(&invalid);
        let mut window = [null(), bad];
        let original = window;
        assert_eq!(
            plan.replace_null_packets(&mut window, &batch, pts, &mut packetizer),
            Err(TeletextMultiplexError::Packet)
        );
        assert_eq!(window, original);
    }
    let mut occupied = null();
    occupied[1] = 2;
    occupied[2] = 0;
    let mut window = [null(), occupied];
    let original = window;
    assert_eq!(
        plan.replace_null_packets(&mut window, &batch, pts, &mut packetizer),
        Err(TeletextMultiplexError::PidCollision { pid: 512 })
    );
    assert_eq!(window, original);
    let mut window = [null()];
    plan.replace_null_packets(&mut window, &batch, pts, &mut packetizer)
        .unwrap();
    assert_eq!(window[0][3], 0x10);
}
