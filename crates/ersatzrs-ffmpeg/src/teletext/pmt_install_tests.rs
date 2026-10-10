use mpeg2ts_reader::{mpegts_crc, packet::Packet};

use super::{
    psi_framing::SectionAssembler,
    psi_tests::{one, packet, pmt, revise},
    signalling_tests::{page, pages},
    *,
};

fn plan(descriptors: &[u8]) -> TeletextPmtPlan {
    let mut values = vec![page(200, 0, "nld", false), page(888, 0, "nld", true)];
    values.extend((850..=880).map(|number| page(number, 0, "nld", true)));
    let service = TeletextServiceDescriptor::from_page_set(&pages(values));
    TeletextPmtPlan::new(&pmt(10, 300, 300, descriptors), 3337, 512, &service).unwrap()
}

fn null() -> [u8; 188] {
    packet(8191, false, 0, &[0xff; 184])
}
fn av() -> [u8; 188] {
    packet(300, false, 0, &[7; 184])
}

fn decoded_tables(window: &[[u8; 188]], plan: &TeletextPmtPlan) -> Vec<Vec<u8>> {
    let mut assembler = SectionAssembler::default();
    let mut result = Vec::new();
    for raw in window {
        let packet = Packet::new(raw);
        if u16::from(packet.pid()) == plan.pmt_pid() {
            result.extend(assembler.push(&packet).unwrap());
        }
    }
    for section in &result {
        assert_eq!(section, plan.section());
        assert_eq!(mpegts_crc::sum32(section), 0);
    }
    result
}

#[test]
fn growing_pmt_uses_only_source_and_null_slots_without_moving_av() {
    let plan = plan(&[]);
    assert_eq!(plan.packets(0).unwrap().len(), 2);
    let original = vec![
        av(),
        one(3337, 15, plan.source_section()),
        av(),
        null(),
        av(),
    ];
    let mut window = original.clone();
    let mut installer = TeletextPmtInstaller::new(plan.clone());
    assert_eq!(
        installer.install(&mut window).unwrap(),
        TeletextPmtInstallReport {
            repetitions: 1,
            pmt_packets: 2,
            consumed_null_packets: 1,
            retired_pmt_packets: 0,
        }
    );
    assert_eq!(installer.next_continuity_counter(), Some(1));
    for index in [0, 2, 4] {
        assert_eq!(window[index], original[index]);
    }
    assert_eq!(decoded_tables(&window, &plan).len(), 1);
}

#[test]
fn capacity_failure_is_atomic_and_can_retry_with_more_original_input() {
    let plan = plan(&[]);
    let mut installer = TeletextPmtInstaller::new(plan.clone());
    let original = vec![one(3337, 0, plan.source_section()), av()];
    let mut window = original.clone();
    assert_eq!(
        installer.install(&mut window),
        Err(TeletextPmtInstallError::Multiplex(
            TeletextMultiplexError::Capacity {
                needed: 2,
                available: 1
            }
        ))
    );
    assert_eq!(window, original);
    assert_eq!(installer.next_continuity_counter(), None);
    window.push(null());
    assert_eq!(installer.install(&mut window).unwrap().pmt_packets, 2);
    assert_eq!(installer.next_continuity_counter(), Some(2));
}

#[test]
fn growth_capacity_must_exist_before_the_next_source_transmission() {
    let plan = plan(&[]);
    let original = vec![
        one(3337, 0, plan.source_section()),
        av(),
        one(3337, 1, plan.source_section()),
        null(),
        null(),
    ];
    let mut window = original.clone();
    let mut installer = TeletextPmtInstaller::new(plan);
    assert_eq!(
        installer.install(&mut window),
        Err(TeletextPmtInstallError::Multiplex(
            TeletextMultiplexError::Capacity {
                needed: 2,
                available: 1
            }
        ))
    );
    assert_eq!(window, original);
    assert_eq!(installer.next_continuity_counter(), None);
}

#[test]
fn output_continuity_persists_independently_of_source_packet_count() {
    let plan = plan(&[]);
    let mut installer = TeletextPmtInstaller::new(plan.clone());
    for (source_cc, expected) in [(14, 0), (15, 2), (0, 4)] {
        let mut window = vec![one(3337, source_cc, plan.source_section()), null()];
        installer.install(&mut window).unwrap();
        assert_eq!(installer.next_continuity_counter(), Some(expected));
        assert_eq!(decoded_tables(&window, &plan).len(), 1);
        assert_eq!(window[0][3] & 15, (expected + 14) & 15);
        assert_eq!(window[1][3] & 15, (expected + 15) & 15);
    }
    let original = vec![one(3337, 2, plan.source_section()), null()];
    let mut window = original.clone();
    assert_eq!(
        installer.install(&mut window),
        Err(TeletextPmtInstallError::Psi(TeletextPsiError::Continuity))
    );
    assert_eq!(window, original);
    assert_eq!(installer.next_continuity_counter(), Some(4));
}

#[test]
fn incomplete_sections_are_retained_and_retried_without_advancing_state() {
    let plan = plan(&[]);
    for split in 1..plan.source_section().len() {
        let first = one(3337, 15, &plan.source_section()[..split]);
        let mut installer = TeletextPmtInstaller::new(plan.clone());
        let mut window = vec![first, av()];
        let original = window.clone();
        assert_eq!(
            installer.install(&mut window),
            Err(TeletextPmtInstallError::Incomplete)
        );
        assert_eq!(window, original);
        assert_eq!(installer.next_continuity_counter(), None);
        window.push(packet(3337, false, 0, &plan.source_section()[split..]));
        assert_eq!(installer.install(&mut window).unwrap().pmt_packets, 2);
        assert_eq!(decoded_tables(&window, &plan).len(), 1);
        assert_eq!(window[1], av());
    }
}

#[test]
fn source_fragmentation_and_duplicates_retire_only_surplus_owned_packets() {
    let plan = plan(&[]);
    let source = plan.source_section();
    let first = one(3337, 0, &source[..1]);
    let original = vec![
        first,
        av(),
        first,
        packet(3337, false, 1, &source[1..3]),
        packet(3337, false, 2, &source[3..8]),
        packet(3337, false, 3, &source[8..]),
        av(),
    ];
    let mut window = original.clone();
    let mut installer = TeletextPmtInstaller::new(plan.clone());
    assert_eq!(
        installer.install(&mut window).unwrap(),
        TeletextPmtInstallReport {
            repetitions: 1,
            pmt_packets: 2,
            consumed_null_packets: 0,
            retired_pmt_packets: 3,
        }
    );
    assert_eq!(window[1], original[1]);
    assert_eq!(window[6], original[6]);
    assert_eq!(decoded_tables(&window, &plan).len(), 1);
    for index in [3, 4, 5] {
        assert_eq!(u16::from(Packet::new(&window[index]).pid()), 8191);
    }
}

#[test]
fn multiple_sections_and_pointer_completion_retain_every_table_repetition() {
    let plan = plan(&[]);
    let source = plan.source_section();
    let mut combined = source.to_vec();
    combined.extend_from_slice(source);
    let mut cases = vec![vec![one(3337, 0, &combined), null(), null(), null()]];
    let mut payload = vec![u8::try_from(source.len() - 2).unwrap()];
    payload.extend_from_slice(&source[2..]);
    payload.extend_from_slice(source);
    cases.push(vec![
        one(3337, 15, &source[..2]),
        packet(3337, true, 0, &payload),
        null(),
        null(),
    ]);
    for mut window in cases {
        let mut installer = TeletextPmtInstaller::new(plan.clone());
        let report = installer.install(&mut window).unwrap();
        assert_eq!(report.repetitions, 2);
        assert_eq!(report.pmt_packets, 4);
        assert_eq!(decoded_tables(&window, &plan).len(), 2);
    }
}

#[test]
fn fragmented_large_source_pmt_grows_into_an_additional_null_slot() {
    let mut descriptors = vec![0x80, 250];
    descriptors.extend_from_slice(&[9; 250]);
    let plan = plan(&descriptors);
    assert_eq!(plan.packets(0).unwrap().len(), 3);
    let source = plan.source_section();
    let original = vec![
        one(3337, 0, &source[..183]),
        av(),
        packet(3337, false, 1, &source[183..]),
        null(),
        av(),
    ];
    let mut window = original.clone();
    let mut installer = TeletextPmtInstaller::new(plan.clone());
    let report = installer.install(&mut window).unwrap();
    assert_eq!((report.pmt_packets, report.consumed_null_packets), (3, 1));
    assert_eq!(window[1], original[1]);
    assert_eq!(window[4], original[4]);
    assert_eq!(decoded_tables(&window, &plan).len(), 1);
}

#[test]
fn changed_future_corrupt_and_late_invalid_source_never_partially_install() {
    let plan = plan(&[]);
    let mut bad = null();
    bad[0] = 0;
    let mut occupied = null();
    occupied[1] = 2;
    occupied[2] = 0;
    for tail in [
        one(
            3337,
            1,
            &revise(plan.source_section().to_vec(), |s| s[5] += 2),
        ),
        one(
            3337,
            1,
            &revise(plan.source_section().to_vec(), |s| s[5] &= 0xfe),
        ),
        bad,
        occupied,
    ] {
        let original = vec![one(3337, 0, plan.source_section()), null(), tail, null()];
        let mut window = original.clone();
        let mut installer = TeletextPmtInstaller::new(plan.clone());
        assert!(installer.install(&mut window).is_err());
        assert_eq!(window, original);
        assert_eq!(installer.next_continuity_counter(), None);
    }
    let corrupt = revise(plan.source_section().to_vec(), |s| s[0] = 3);
    let mut original = one(3337, 0, &corrupt);
    original[187] ^= 1;
    let mut window = vec![original, null()];
    let mut installer = TeletextPmtInstaller::new(plan);
    assert_eq!(
        installer.install(&mut window),
        Err(TeletextPmtInstallError::SourceChanged)
    );
}

#[test]
fn adaptation_control_data_and_orphan_continuations_are_rejected() {
    let plan = plan(&[]);
    let mut pcr = one(3337, 0, plan.source_section());
    pcr[5] = 0x10;
    pcr[6..12].copy_from_slice(&[0, 0, 0, 0, 0x7e, 0]);
    let original = vec![pcr, null()];
    let mut window = original.clone();
    let mut installer = TeletextPmtInstaller::new(plan.clone());
    assert_eq!(
        installer.install(&mut window),
        Err(TeletextPmtInstallError::Adaptation)
    );
    assert_eq!(window, original);
    let mut window = vec![packet(3337, false, 0, plan.source_section()), null()];
    assert_eq!(
        installer.install(&mut window),
        Err(TeletextPmtInstallError::Psi(TeletextPsiError::Framing))
    );
}

#[test]
fn empty_and_unowned_windows_pass_without_fake_installation_or_mutation() {
    let plan = plan(&[]);
    let mut installer = TeletextPmtInstaller::new(plan);
    assert_eq!(
        installer.install(&mut []).unwrap(),
        TeletextPmtInstallReport::default()
    );
    let original = vec![av(), null(), av()];
    let mut window = original.clone();
    assert_eq!(
        installer.install(&mut window).unwrap(),
        TeletextPmtInstallReport::default()
    );
    assert_eq!(window, original);
    assert_eq!(installer.next_continuity_counter(), None);
    assert_eq!(
        installer.install(&mut vec![null(); 32_769]),
        Err(TeletextPmtInstallError::Multiplex(
            TeletextMultiplexError::Window
        ))
    );
}
