use mpeg2ts_reader::{mpegts_crc, packet::Packet};

use super::{
    psi_framing::SectionAssembler,
    signalling_tests::{page, pages},
    *,
};

#[path = "psi_source_epoch_tests.rs"]
mod source_epoch_tests;

fn service() -> TeletextServiceDescriptor {
    TeletextServiceDescriptor::from_page_set(&pages(vec![page(200, 0, "nld", false)]))
}

pub(super) fn section(
    table: u8,
    id: u16,
    version: u8,
    number: u8,
    last: u8,
    body: &[u8],
) -> Vec<u8> {
    let [hi, lo] = id.to_be_bytes();
    let mut data = vec![table, 0xb0, 0, hi, lo, 0xc1 | (version << 1), number, last];
    data.extend_from_slice(body);
    checksum(&mut data);
    data
}

fn checksum(data: &mut Vec<u8>) {
    let [hi, lo] = u16::try_from(data.len() + 1).unwrap().to_be_bytes();
    data[1] = 0xb0 | hi;
    data[2] = lo;
    data.extend_from_slice(&mpegts_crc::sum32(data).to_be_bytes());
}

pub(super) fn revise(mut data: Vec<u8>, change: impl FnOnce(&mut Vec<u8>)) -> Vec<u8> {
    data.truncate(data.len() - 4);
    change(&mut data);
    checksum(&mut data);
    data
}

pub(super) fn pat(version: u8, number: u8, last: u8, entries: &[(u16, u16)]) -> Vec<u8> {
    let mut body = Vec::new();
    for &(program, pid) in entries {
        let [hi, lo] = program.to_be_bytes();
        let [pid_hi, pid_lo] = pid.to_be_bytes();
        body.extend_from_slice(&[hi, lo, 0xe0 | pid_hi, pid_lo]);
    }
    section(0, 7, version, number, last, &body)
}

pub(super) fn pmt(program: u16, pcr: u16, elementary: u16, descriptors: &[u8]) -> Vec<u8> {
    let [ph, pl] = pcr.to_be_bytes();
    let [dh, dl] = u16::try_from(descriptors.len()).unwrap().to_be_bytes();
    let [eh, el] = elementary.to_be_bytes();
    let mut body = vec![0xe0 | ph, pl, 0xf0 | dh, dl];
    body.extend_from_slice(descriptors);
    body.extend_from_slice(&[0x1b, 0xe0 | eh, el, 0xf0, 0]);
    section(2, program, 0, 0, 0, &body)
}

pub(super) fn packet(pid: u16, start: bool, cc: u8, payload: &[u8]) -> [u8; 188] {
    assert!((1..=184).contains(&payload.len()));
    let [hi, lo] = pid.to_be_bytes();
    let mut raw = [0xff; 188];
    raw[..4].copy_from_slice(&[
        0x47,
        hi | if start { 0x40 } else { 0 },
        lo,
        0x10 | (cc & 15),
    ]);
    if payload.len() < 184 {
        raw[3] |= 0x20;
        raw[4] = u8::try_from(183 - payload.len()).unwrap();
        if raw[4] > 0 {
            raw[5] = 0;
        }
    }
    raw[188 - payload.len()..].copy_from_slice(payload);
    raw
}

pub(super) fn one(pid: u16, cc: u8, section: &[u8]) -> [u8; 188] {
    let mut payload = vec![0];
    payload.extend_from_slice(section);
    packet(pid, true, cc, &payload)
}

fn fragments(discovery: &mut TeletextPsiDiscovery, pid: u16, section: &[u8], first: usize) {
    discovery.push(&one(pid, 15, &section[..first])).unwrap();
    for (index, part) in section[first..].chunks(184).enumerate() {
        discovery
            .push(&packet(pid, false, u8::try_from(index).unwrap(), part))
            .unwrap();
    }
}

fn ready() -> TeletextPsiDiscovery {
    let mut discovery = TeletextPsiDiscovery::new(10, 512).unwrap();
    discovery
        .push(&one(0, 0, &pat(0, 0, 0, &[(10, 3337)])))
        .unwrap();
    discovery
        .push(&one(3337, 0, &pmt(10, 300, 300, &[])))
        .unwrap();
    discovery
}

fn fenced(discovery: &mut TeletextPsiDiscovery, error: TeletextPsiError) {
    assert_eq!(discovery.plan(&service()), Err(error.clone()));
    assert_eq!(
        discovery.push(&packet(8191, false, 0, &[0xff; 184])),
        Err(error)
    );
}

#[test]
fn discovery_uses_pat_identity_and_waits_for_every_advertised_program() {
    let mut discovery = TeletextPsiDiscovery::new(10, 512).unwrap();
    assert_eq!(
        discovery.plan(&service()),
        Err(TeletextPsiError::Incomplete)
    );
    discovery
        .push(&one(0, 0, &pat(0, 0, 0, &[(10, 3337), (11, 3456)])))
        .unwrap();
    discovery
        .push(&one(3337, 0, &pmt(10, 300, 300, &[])))
        .unwrap();
    assert_eq!(
        discovery.plan(&service()),
        Err(TeletextPsiError::Incomplete)
    );
    discovery
        .push(&one(3456, 0, &pmt(11, 301, 302, &[])))
        .unwrap();
    let plan = discovery.plan(&service()).unwrap();
    assert_eq!(
        (plan.program_number(), plan.pmt_pid(), plan.pcr_pid()),
        (10, 3337, 300)
    );
    assert_eq!(plan.source_section(), pmt(10, 300, 300, &[]));
}

#[test]
fn split_common_syntax_headers_and_large_pmt_are_reassembled_at_every_first_packet_cut() {
    let table = pat(0, 0, 0, &[(10, 3337)]);
    for cut in 1..table.len() {
        let mut discovery = TeletextPsiDiscovery::new(10, 512).unwrap();
        fragments(&mut discovery, 0, &table, cut);
        discovery
            .push(&one(3337, 0, &pmt(10, 300, 300, &[])))
            .unwrap();
        assert!(discovery.plan(&service()).is_ok(), "PAT cut {cut}");
    }
    let mut descriptors = vec![0x80, 250];
    descriptors.extend_from_slice(&[9; 250]);
    descriptors.extend_from_slice(&[0x81, 250]);
    descriptors.extend_from_slice(&[7; 250]);
    let table = pmt(10, 300, 300, &descriptors);
    for cut in 1..=183 {
        let mut discovery = TeletextPsiDiscovery::new(10, 512).unwrap();
        discovery
            .push(&one(0, 0, &pat(0, 0, 0, &[(10, 3337)])))
            .unwrap();
        fragments(&mut discovery, 3337, &table, cut);
        assert_eq!(
            discovery.plan(&service()).unwrap().source_section(),
            table,
            "PMT cut {cut}"
        );
    }
}

#[test]
fn pointer_finishes_previous_section_and_dispatches_multiple_new_sections() {
    let first = pat(0, 0, 2, &[(10, 3337)]);
    let second = pat(0, 1, 2, &[(11, 3338)]);
    let third = pat(0, 2, 2, &[(12, 3339)]);
    let mut assembler = SectionAssembler::default();
    assert!(assembler
        .push(&Packet::new(&one(0, 15, &first[..2])))
        .unwrap()
        .is_empty());
    let mut payload = vec![u8::try_from(first.len() - 2).unwrap()];
    payload.extend_from_slice(&first[2..]);
    payload.extend_from_slice(&second);
    payload.extend_from_slice(&third);
    assert_eq!(
        assembler
            .push(&Packet::new(&packet(0, true, 0, &payload)))
            .unwrap(),
        vec![first, second.clone(), third]
    );
    let mut fresh = SectionAssembler::default();
    let mut orphan = vec![3, 2, 3, 4];
    orphan.extend_from_slice(&second);
    assert_eq!(
        fresh
            .push(&Packet::new(&packet(0, true, 4, &orphan)))
            .unwrap(),
        vec![second]
    );
}

#[test]
fn duplicate_payload_and_adaptation_only_packets_do_not_advance_section_continuity() {
    let table = pat(0, 0, 0, &[(10, 3337)]);
    let first = one(0, 15, &table[..5]);
    let mut discovery = TeletextPsiDiscovery::new(10, 512).unwrap();
    discovery.push(&first).unwrap();
    discovery.push(&first).unwrap();
    let mut adaptation = [0xff; 188];
    adaptation[..6].copy_from_slice(&[0x47, 0, 0, 0x2f, 183, 0]);
    discovery.push(&adaptation).unwrap();
    discovery.push(&packet(0, false, 0, &table[5..])).unwrap();
    discovery
        .push(&one(3337, 0, &pmt(10, 300, 300, &[])))
        .unwrap();
    assert!(discovery.plan(&service()).is_ok());
    for raw in [one(0, 2, &table), one(0, 0, &pat(1, 0, 0, &[(10, 3337)]))] {
        let mut discovery = ready();
        assert_eq!(discovery.push(&raw), Err(TeletextPsiError::Continuity));
        fenced(&mut discovery, TeletextPsiError::Continuity);
    }
}

#[test]
fn malformed_sections_pointers_crc_and_discontinuity_permanently_fence_admission() {
    let table = pat(0, 0, 0, &[(10, 3337)]);
    let mut damaged = table.clone();
    damaged[10] ^= 1;
    let mut discontinuous = one(0, 1, &table);
    discontinuous[5] = 0x80;
    let cases = [
        (one(0, 1, &damaged), TeletextPsiError::Crc),
        (packet(0, true, 1, &[2, 0, 0]), TeletextPsiError::Framing),
        (
            packet(0, true, 1, &[0, 0, 0xbf, 0xff]),
            TeletextPsiError::Framing,
        ),
        (
            one(0, 1, &revise(table.clone(), |s| s[6] = 1)),
            TeletextPsiError::Framing,
        ),
        (discontinuous, TeletextPsiError::Continuity),
        (
            one(0, 1, &revise(table.clone(), |s| s[5] &= 0x7f)),
            TeletextPsiError::Framing,
        ),
        (
            one(0, 1, &revise(table.clone(), |s| s[0] = 2)),
            TeletextPsiError::Framing,
        ),
        (
            one(0, 1, &pat(0, 0, 1, &[(10, 3337)])),
            TeletextPsiError::Version,
        ),
    ];
    for (raw, error) in cases {
        let mut discovery = ready();
        assert_eq!(discovery.push(&raw), Err(error.clone()));
        fenced(&mut discovery, error);
    }
    let mut discovery = TeletextPsiDiscovery::new(10, 512).unwrap();
    discovery.push(&one(0, 0, &table[..4])).unwrap();
    assert_eq!(
        discovery.push(&one(0, 1, &table)),
        Err(TeletextPsiError::Framing)
    );
}

#[test]
fn out_of_order_pat_sections_wait_for_complete_current_generation() {
    let mut discovery = TeletextPsiDiscovery::new(10, 512).unwrap();
    discovery
        .push(&one(0, 0, &pat(0, 1, 1, &[(11, 3456)])))
        .unwrap();
    assert_eq!(
        discovery.plan(&service()),
        Err(TeletextPsiError::Incomplete)
    );
    discovery
        .push(&one(0, 1, &pat(0, 0, 1, &[(10, 3337)])))
        .unwrap();
    discovery
        .push(&one(3337, 0, &pmt(10, 300, 300, &[])))
        .unwrap();
    discovery
        .push(&one(3456, 0, &pmt(11, 301, 302, &[])))
        .unwrap();
    let original = discovery.plan(&service()).unwrap();
    let future = revise(pat(1, 0, 0, &[(10, 3500)]), |s| s[5] &= 0xfe);
    discovery.push(&one(0, 2, &future)).unwrap();
    assert_eq!(discovery.plan(&service()).unwrap(), original);
    discovery
        .push(&one(0, 3, &pat(1, 0, 0, &[(10, 3500)])))
        .unwrap();
    assert_eq!(
        discovery.plan(&service()),
        Err(TeletextPsiError::Incomplete)
    );
    discovery
        .push(&one(3500, 8, &pmt(10, 400, 400, &[])))
        .unwrap();
    assert_eq!(discovery.plan(&service()).unwrap().pmt_pid(), 3500);
}

#[test]
fn candidate_is_rejected_when_other_program_pcr_es_or_ca_owns_it() {
    for entries in [vec![(10, 512)], vec![(0, 512), (10, 3337)]] {
        let mut discovery = TeletextPsiDiscovery::new(10, 512).unwrap();
        let error = TeletextPsiError::Multiplex(TeletextMultiplexError::PidCollision { pid: 512 });
        assert_eq!(
            discovery.push(&one(0, 0, &pat(0, 0, 0, &entries))),
            Err(error.clone())
        );
        fenced(&mut discovery, error);
    }
    for (pcr, elementary, descriptors) in [
        (512, 300, vec![]),
        (300, 512, vec![]),
        (300, 300, vec![9, 4, 0, 1, 0xe2, 0]),
    ] {
        let mut discovery = TeletextPsiDiscovery::new(10, 512).unwrap();
        discovery
            .push(&one(0, 0, &pat(0, 0, 0, &[(10, 3337), (11, 3456)])))
            .unwrap();
        discovery
            .push(&one(3337, 0, &pmt(10, 300, 300, &[])))
            .unwrap();
        let error = TeletextPsiError::Multiplex(TeletextMultiplexError::PidCollision { pid: 512 });
        assert_eq!(
            discovery.push(&one(3456, 0, &pmt(11, pcr, elementary, &descriptors))),
            Err(error.clone())
        );
        fenced(&mut discovery, error);
    }
    let mut discovery = ready();
    let error = TeletextPsiError::Multiplex(TeletextMultiplexError::PidCollision { pid: 512 });
    assert_eq!(
        discovery.push(&packet(512, false, 0, &[0; 184])),
        Err(error.clone())
    );
    fenced(&mut discovery, error);
}

#[test]
fn cat_sections_are_complete_and_candidate_ca_ownership_is_checked() {
    let cat = section(1, 0xffff, 0, 0, 1, &[9, 4, 0, 1, 0xe3, 0]);
    let mut discovery = ready();
    discovery.push(&one(1, 0, &cat)).unwrap();
    assert_eq!(
        discovery.plan(&service()),
        Err(TeletextPsiError::Incomplete)
    );
    discovery
        .push(&one(1, 1, &section(1, 0xffff, 0, 1, 1, &[])))
        .unwrap();
    assert!(discovery.plan(&service()).is_ok());
    let collision = section(1, 0xffff, 1, 0, 0, &[9, 4, 0, 1, 0xe2, 0]);
    let error = TeletextPsiError::Multiplex(TeletextMultiplexError::PidCollision { pid: 512 });
    assert_eq!(discovery.push(&one(1, 2, &collision)), Err(error.clone()));
    fenced(&mut discovery, error);
}

#[test]
fn selected_pmt_must_not_be_elementary_pcr_or_ca_data_for_another_program() {
    let mut discovery = ready();
    discovery
        .push(&one(
            1,
            0,
            &section(1, 0xffff, 0, 0, 0, &[9, 4, 0, 1, 0xed, 9]),
        ))
        .unwrap();
    assert_eq!(
        discovery.plan(&service()),
        Err(TeletextPsiError::Multiplex(
            TeletextMultiplexError::PidCollision { pid: 3337 }
        ))
    );
    for (pcr, elementary, descriptors) in [
        (3337, 301, vec![]),
        (301, 3337, vec![]),
        (301, 301, vec![9, 4, 0, 1, 0xed, 9]),
    ] {
        let mut discovery = TeletextPsiDiscovery::new(10, 512).unwrap();
        discovery
            .push(&one(0, 0, &pat(0, 0, 0, &[(10, 3337), (11, 3456)])))
            .unwrap();
        discovery
            .push(&one(3337, 0, &pmt(10, 300, 300, &[])))
            .unwrap();
        discovery
            .push(&one(3456, 0, &pmt(11, pcr, elementary, &descriptors)))
            .unwrap();
        assert_eq!(
            discovery.plan(&service()),
            Err(TeletextPsiError::Multiplex(
                TeletextMultiplexError::PidCollision { pid: 3337 }
            ))
        );
    }
}

#[test]
fn pat_rejects_duplicate_invalid_and_oversized_program_maps() {
    for table in [
        section(0, 7, 0, 0, 0, &[0, 10, 0xed]),
        section(0, 7, 0, 0, 0, &[0, 10, 0x0d, 9]),
    ] {
        let mut discovery = TeletextPsiDiscovery::new(10, 512).unwrap();
        assert_eq!(
            discovery.push(&one(0, 0, &table)),
            Err(TeletextPsiError::Pat)
        );
    }
    for entries in [
        vec![(10, 3337), (10, 3456)],
        vec![(10, 3337), (11, 3337)],
        vec![(10, 1)],
        vec![(0, 16), (0, 17)],
        vec![(0, 8191)],
    ] {
        let mut discovery = TeletextPsiDiscovery::new(10, 512).unwrap();
        assert_eq!(
            discovery.push(&one(0, 0, &pat(0, 0, 0, &entries))),
            Err(TeletextPsiError::Pat)
        );
    }
    let entries: Vec<_> = (1..=129).map(|n| (n, 3000 + n)).collect();
    let table = pat(0, 0, 0, &entries);
    let mut discovery = TeletextPsiDiscovery::new(10, 512).unwrap();
    discovery.push(&one(0, 0, &table[..183])).unwrap();
    let parts: Vec<_> = table[183..].chunks(184).collect();
    for (index, part) in parts.iter().enumerate() {
        let result = discovery.push(&packet(0, false, u8::try_from(index + 1).unwrap(), part));
        if index + 1 == parts.len() {
            assert_eq!(result, Err(TeletextPsiError::Capacity));
        } else {
            result.unwrap();
        }
    }
}

#[test]
fn contradictory_pat_and_pmt_versions_and_wrong_program_identity_are_rejected() {
    let mut discovery = ready();
    assert_eq!(
        discovery.push(&one(0, 1, &pat(0, 0, 0, &[(10, 3456)]))),
        Err(TeletextPsiError::Version)
    );
    let mut discovery = ready();
    assert_eq!(
        discovery.push(&one(3337, 1, &pmt(10, 301, 301, &[]))),
        Err(TeletextPsiError::Version)
    );
    let mut discovery = ready();
    assert_eq!(
        discovery.push(&one(3337, 1, &pmt(11, 300, 300, &[]))),
        Err(TeletextPsiError::Program)
    );
    let mut discovery = ready();
    let updated = revise(pmt(10, 301, 301, &[]), |s| s[5] += 2);
    discovery.push(&one(3337, 1, &updated[..4])).unwrap();
    assert_eq!(
        discovery.plan(&service()),
        Err(TeletextPsiError::Incomplete)
    );
    discovery
        .push(&packet(3337, false, 2, &updated[4..]))
        .unwrap();
    assert_eq!(discovery.plan(&service()).unwrap().pcr_pid(), 301);
}

#[test]
fn discovery_packet_limit_and_absent_program_fail_visibly() {
    let mut discovery = ready();
    let null = packet(8191, false, 0, &[0xff; 184]);
    for _ in 2..32_768 {
        discovery.push(&null).unwrap();
    }
    assert_eq!(discovery.push(&null), Err(TeletextPsiError::Capacity));
    fenced(&mut discovery, TeletextPsiError::Capacity);
    let mut discovery = TeletextPsiDiscovery::new(99, 512).unwrap();
    discovery
        .push(&one(0, 0, &pat(0, 0, 0, &[(10, 3337)])))
        .unwrap();
    assert_eq!(discovery.plan(&service()), Err(TeletextPsiError::Program));
    assert!(TeletextPsiDiscovery::new(0, 512).is_err());
    assert!(TeletextPsiDiscovery::new(10, 8191).is_err());
}
