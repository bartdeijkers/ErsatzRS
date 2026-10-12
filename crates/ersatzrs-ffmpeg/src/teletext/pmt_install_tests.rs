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

fn adaptation_flags(mut raw: [u8; 188], flags: u8) -> [u8; 188] {
    assert!(raw[3] & 0x20 != 0 && raw[4] >= 16);
    raw[5] = flags;
    // Well-formed clock fields and zero-length ancillary data ensure rejection
    // tests exercise the installer policy rather than malformed field framing.
    raw[6..22].fill(0);
    raw[10] = 0x7e;
    raw[16] = 0x7e;
    raw
}

fn adaptation_only(flags: u8) -> [u8; 188] {
    let mut raw = [0xff; 188];
    raw[..6].copy_from_slice(&[0x47, 0x0d, 0x09, 0x20, 183, flags]);
    raw
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
            announced: false,
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
            announced: false,
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

#[test]
fn strict_constructor_rejects_first_source_pmt_startup_reset_atomically() {
    let plan = plan(&[]);
    let original = vec![
        adaptation_flags(one(3337, 15, plan.source_section()), 0x80),
        null(),
    ];
    let mut window = original.clone();
    let mut installer = TeletextPmtInstaller::new(plan);
    assert_eq!(
        installer.install(&mut window),
        Err(TeletextPmtInstallError::Adaptation)
    );
    assert_eq!(window, original);
    assert_eq!(installer.next_continuity_counter(), None);
}

#[test]
fn source_epoch_strips_only_owned_startup_reset_and_preserves_av_pcr() {
    let plan = plan(&[]);
    let av_pcr = adaptation_flags(packet(300, false, 0, &[7; 100]), 0x90);
    let original = vec![
        av_pcr,
        adaptation_flags(one(3337, 15, plan.source_section()), 0x80),
        av(),
        null(),
        av_pcr,
    ];
    let mut window = original.clone();
    let mut installer = TeletextPmtInstaller::new_source_epoch(plan.clone());
    assert_eq!(installer.install(&mut window).unwrap().repetitions, 1);
    assert_eq!(installer.next_continuity_counter(), Some(1));
    assert_eq!(decoded_tables(&window, &plan).len(), 1);
    for index in [0, 2, 4] {
        assert_eq!(window[index], original[index]);
    }
    for index in [1, 3] {
        assert!(Packet::new(&window[index])
            .adaptation_field()
            .is_none_or(|field| !field.discontinuity_indicator()));
    }
    assert_eq!(window[1][3] & 15, 15);
    assert_eq!(window[3][3] & 15, 0);
}

#[test]
fn source_epoch_capacity_retry_keeps_original_first_flag_and_counter() {
    let plan = plan(&[]);
    let original = vec![
        adaptation_flags(one(3337, 7, plan.source_section()), 0x80),
        av(),
    ];
    let mut window = original.clone();
    let mut installer = TeletextPmtInstaller::new_source_epoch(plan.clone());
    assert_eq!(
        installer.install(&mut window),
        Err(TeletextPmtInstallError::Multiplex(
            TeletextMultiplexError::Capacity {
                needed: 2,
                available: 1,
            }
        ))
    );
    assert_eq!(window, original);
    assert_eq!(installer.next_continuity_counter(), None);
    window.push(null());
    installer.install(&mut window).unwrap();
    assert_eq!(installer.next_continuity_counter(), Some(9));
    assert_eq!(decoded_tables(&window, &plan).len(), 1);
}

#[test]
fn source_epoch_incomplete_retry_keeps_original_first_flag() {
    let plan = plan(&[]);
    let source = plan.source_section();
    let first = adaptation_flags(one(3337, 15, &source[..2]), 0x80);
    let original = vec![first, av()];
    let mut window = original.clone();
    let mut installer = TeletextPmtInstaller::new_source_epoch(plan.clone());
    assert_eq!(
        installer.install(&mut window),
        Err(TeletextPmtInstallError::Incomplete)
    );
    assert_eq!(window, original);
    assert_eq!(installer.next_continuity_counter(), None);
    window.push(packet(3337, false, 0, &source[2..]));
    installer.install(&mut window).unwrap();
    assert_eq!(decoded_tables(&window, &plan).len(), 1);
    assert_eq!(window[1], av());
    assert_eq!(installer.next_continuity_counter(), Some(1));
}

#[test]
fn source_epoch_checks_original_pmt_identity_before_stripping_reset() {
    let plan = plan(&[]);
    let changed = revise(plan.source_section().to_vec(), |section| section[5] += 2);
    let original = vec![adaptation_flags(one(3337, 0, &changed), 0x80), null()];
    let mut window = original.clone();
    let mut installer = TeletextPmtInstaller::new_source_epoch(plan.clone());
    assert_eq!(
        installer.install(&mut window),
        Err(TeletextPmtInstallError::SourceChanged)
    );
    assert_eq!(window, original);
    assert_eq!(installer.next_continuity_counter(), None);
    let mut valid = vec![
        adaptation_flags(one(3337, 0, plan.source_section()), 0x80),
        null(),
    ];
    installer.install(&mut valid).unwrap();
}

#[test]
fn source_epoch_rejects_later_reset_including_duplicate_without_changing_output() {
    let plan = plan(&[]);
    let first = adaptation_flags(one(3337, 0, plan.source_section()), 0x80);
    let mut installer = TeletextPmtInstaller::new_source_epoch(plan.clone());
    installer.install(&mut [first, null()]).unwrap();
    for later in [
        first,
        adaptation_flags(one(3337, 1, plan.source_section()), 0x80),
        adaptation_only(0x80),
    ] {
        let original = vec![later, null()];
        let mut window = original.clone();
        assert_eq!(
            installer.install(&mut window),
            Err(TeletextPmtInstallError::Adaptation)
        );
        assert_eq!(window, original);
        assert_eq!(installer.next_continuity_counter(), Some(2));
    }
    installer
        .install(&mut [one(3337, 1, plan.source_section()), null()])
        .unwrap();
    assert_eq!(installer.next_continuity_counter(), Some(4));
}

#[test]
fn first_pmt_packet_consumes_epoch_exception_even_without_payload_or_flag() {
    let plan = plan(&[]);
    for first in [adaptation_only(0), adaptation_only(0x80)] {
        let mut installer = TeletextPmtInstaller::new_source_epoch(plan.clone());
        let mut window = [first, av()];
        assert_eq!(
            installer.install(&mut window).unwrap().retired_pmt_packets,
            1
        );
        assert_eq!(window, [null(), av()]);
        assert_eq!(installer.next_continuity_counter(), None);
        let original = vec![
            adaptation_flags(one(3337, 0, plan.source_section()), 0x80),
            null(),
        ];
        let mut window = original.clone();
        assert_eq!(
            installer.install(&mut window),
            Err(TeletextPmtInstallError::Adaptation)
        );
        assert_eq!(window, original);
        installer
            .install(&mut [one(3337, 0, plan.source_section()), null()])
            .unwrap();
    }
    let mut installer = TeletextPmtInstaller::new_source_epoch(plan.clone());
    installer
        .install(&mut [one(3337, 0, plan.source_section()), null()])
        .unwrap();
    let original = vec![
        adaptation_flags(one(3337, 1, plan.source_section()), 0x80),
        null(),
    ];
    let mut window = original.clone();
    assert_eq!(
        installer.install(&mut window),
        Err(TeletextPmtInstallError::Adaptation)
    );
    assert_eq!(window, original);
    assert_eq!(installer.next_continuity_counter(), Some(2));
}

#[test]
fn source_epoch_rejects_all_other_adaptation_flags_even_on_first_packet() {
    let plan = plan(&[]);
    // Random access, ES priority, PCR, OPCR, splice, private data, extension.
    for control in [0x40, 0x20, 0x10, 0x08, 0x04, 0x02, 0x01] {
        for flags in [control, 0x80 | control] {
            let original = vec![
                adaptation_flags(one(3337, 0, plan.source_section()), flags),
                null(),
            ];
            let mut window = original.clone();
            let mut installer = TeletextPmtInstaller::new_source_epoch(plan.clone());
            assert_eq!(
                installer.install(&mut window),
                Err(TeletextPmtInstallError::Adaptation)
            );
            assert_eq!(window, original);
            assert_eq!(installer.next_continuity_counter(), None);
        }
    }
}

#[test]
fn source_epoch_late_reset_failure_does_not_consume_initial_permission() {
    let plan = plan(&[]);
    let first = adaptation_flags(one(3337, 0, plan.source_section()), 0x80);
    for later in [
        first,
        adaptation_flags(one(3337, 1, plan.source_section()), 0x80),
    ] {
        let original = vec![first, null(), later, null()];
        let mut window = original.clone();
        let mut installer = TeletextPmtInstaller::new_source_epoch(plan.clone());
        assert_eq!(
            installer.install(&mut window),
            Err(TeletextPmtInstallError::Adaptation)
        );
        assert_eq!(window, original);
        assert_eq!(installer.next_continuity_counter(), None);
        installer.install(&mut [first, null()]).unwrap();
        assert_eq!(installer.next_continuity_counter(), Some(2));
    }
}
