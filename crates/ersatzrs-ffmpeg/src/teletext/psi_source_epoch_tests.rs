use super::*;

fn flagged(mut raw: [u8; 188]) -> [u8; 188] {
    assert!(raw[3] & 0x20 != 0 && raw[4] > 0);
    raw[5] |= 0x80;
    raw
}

fn adaptation_only(pid: u16, cc: u8, discontinuity: bool) -> [u8; 188] {
    let [hi, lo] = pid.to_be_bytes();
    let mut raw = [0xff; 188];
    raw[..6].copy_from_slice(&[
        0x47,
        hi,
        lo,
        0x20 | cc,
        183,
        if discontinuity { 0x80 } else { 0 },
    ]);
    raw
}

fn epoch_ready(cat: bool) -> TeletextPsiDiscovery {
    let mut discovery = TeletextPsiDiscovery::new_source_epoch(10, 512).unwrap();
    discovery
        .push(&one(0, 0, &pat(0, 0, 0, &[(10, 3337)])))
        .unwrap();
    discovery
        .push(&one(3337, 0, &pmt(10, 300, 300, &[])))
        .unwrap();
    if cat {
        discovery
            .push(&one(1, 0, &section(1, 0xffff, 0, 0, 0, &[])))
            .unwrap();
    }
    discovery
}

#[test]
fn new_source_epoch_admits_only_initial_flags_without_skipping_all_program_readiness() {
    let mut discovery = TeletextPsiDiscovery::new_source_epoch(10, 512).unwrap();
    discovery
        .push(&flagged(one(
            0,
            0,
            &pat(0, 0, 0, &[(10, 3337), (11, 3456)]),
        )))
        .unwrap();
    discovery
        .push(&flagged(one(1, 0, &section(1, 0xffff, 0, 0, 0, &[]))))
        .unwrap();
    discovery
        .push(&flagged(one(3337, 0, &pmt(10, 300, 300, &[]))))
        .unwrap();
    assert_eq!(
        discovery.plan(&service()),
        Err(TeletextPsiError::Incomplete)
    );
    discovery
        .push(&flagged(one(3456, 0, &pmt(11, 301, 302, &[]))))
        .unwrap();
    assert_eq!(discovery.plan(&service()).unwrap().pmt_pid(), 3337);
    let mut monitor = discovery.into_monitor(&service()).unwrap();
    assert!(monitor
        .inspect_window(&[
            one(0, 1, &pat(0, 0, 0, &[(10, 3337), (11, 3456)])),
            one(3337, 1, &pmt(10, 300, 300, &[])),
            one(3456, 1, &pmt(11, 301, 302, &[])),
        ])
        .unwrap());
    monitor.finish().unwrap();
}

#[test]
fn ordinary_constructor_still_rejects_initial_pat_cat_and_pmt_flags() {
    for raw in [
        one(0, 0, &pat(0, 0, 0, &[(10, 3337)])),
        one(1, 0, &section(1, 0xffff, 0, 0, 0, &[])),
        one(3337, 0, &pmt(10, 300, 300, &[])),
    ] {
        let mut discovery = TeletextPsiDiscovery::new(10, 512).unwrap();
        if u16::from(Packet::new(&raw).pid()) == 3337 {
            discovery
                .push(&one(0, 0, &pat(0, 0, 0, &[(10, 3337)])))
                .unwrap();
        }
        assert_eq!(
            discovery.push(&flagged(raw)),
            Err(TeletextPsiError::Continuity)
        );
        fenced(&mut discovery, TeletextPsiError::Continuity);
    }
}

#[test]
fn later_pat_pmt_and_cat_discontinuities_permanently_fence_source_epoch_admission() {
    for raw in [
        one(0, 1, &pat(0, 0, 0, &[(10, 3337)])),
        one(3337, 1, &pmt(10, 300, 300, &[])),
        one(1, 1, &section(1, 0xffff, 0, 0, 0, &[])),
    ] {
        let mut discovery = epoch_ready(true);
        assert_eq!(
            discovery.push(&flagged(raw)),
            Err(TeletextPsiError::Continuity)
        );
        fenced(&mut discovery, TeletextPsiError::Continuity);
    }
}

#[test]
fn adaptation_only_first_packet_consumes_allowance_even_without_a_flag_or_pat_entry() {
    for pid in [0, 3337] {
        for initial_flag in [false, true] {
            let mut discovery = TeletextPsiDiscovery::new_source_epoch(10, 512).unwrap();
            discovery
                .push(&adaptation_only(pid, 7, initial_flag))
                .unwrap();
            if pid != 0 {
                discovery
                    .push(&one(0, 0, &pat(0, 0, 0, &[(10, 3337)])))
                    .unwrap();
            }
            let table = if pid == 0 {
                pat(0, 0, 0, &[(10, 3337)])
            } else {
                pmt(10, 300, 300, &[])
            };
            assert_eq!(
                discovery.push(&flagged(one(pid, 7, &table))),
                Err(TeletextPsiError::Continuity)
            );
            fenced(&mut discovery, TeletextPsiError::Continuity);
        }
    }
    let mut discovery = TeletextPsiDiscovery::new_source_epoch(10, 512).unwrap();
    discovery.push(&adaptation_only(0, 7, true)).unwrap();
    discovery
        .push(&one(0, 7, &pat(0, 0, 0, &[(10, 3337)])))
        .unwrap();
    discovery
        .push(&one(3337, 0, &pmt(10, 300, 300, &[])))
        .unwrap();
    assert!(discovery.plan(&service()).is_ok());
}

#[test]
fn pat_version_reassembly_does_not_restore_a_pmt_pids_initial_allowance() {
    let mut discovery = epoch_ready(false);
    discovery
        .push(&one(0, 1, &pat(1, 0, 0, &[(10, 3337)])))
        .unwrap();
    assert_eq!(
        discovery.push(&flagged(one(3337, 1, &pmt(10, 300, 300, &[])))),
        Err(TeletextPsiError::Continuity)
    );
    fenced(&mut discovery, TeletextPsiError::Continuity);
}

#[test]
fn sealed_epoch_never_admits_a_later_flag_including_first_observed_cat_and_exact_duplicate() {
    for raw in [
        one(0, 1, &pat(0, 0, 0, &[(10, 3337)])),
        one(3337, 1, &pmt(10, 300, 300, &[])),
        one(1, 0, &section(1, 0xffff, 0, 0, 0, &[])),
    ] {
        let mut monitor = epoch_ready(false).into_monitor(&service()).unwrap();
        assert_eq!(
            monitor.inspect_window(&[flagged(raw)]),
            Err(TeletextPsiError::Continuity)
        );
        assert_eq!(monitor.finish(), Err(TeletextPsiError::Continuity));
        assert_eq!(
            monitor.inspect_window(&[]),
            Err(TeletextPsiError::Continuity)
        );
    }
    let mut discovery = TeletextPsiDiscovery::new_source_epoch(10, 512).unwrap();
    let initial = flagged(one(3337, 0, &pmt(10, 300, 300, &[])));
    discovery
        .push(&one(0, 0, &pat(0, 0, 0, &[(10, 3337)])))
        .unwrap();
    discovery.push(&initial).unwrap();
    let mut monitor = discovery.into_monitor(&service()).unwrap();
    assert_eq!(
        monitor.inspect_window(&[initial]),
        Err(TeletextPsiError::Continuity)
    );
}

#[test]
fn initial_flag_does_not_waive_pointer_start_crc_or_split_section_readiness() {
    let table = pat(0, 0, 0, &[(10, 3337)]);
    let valid = flagged(one(0, 0, &table));
    let mut pointer = valid;
    pointer[188 - table.len() - 1] = 184;
    let mut no_start = valid;
    no_start[1] &= !0x40;
    let mut crc = valid;
    crc[187] ^= 1;
    for (raw, error) in [
        (pointer, TeletextPsiError::Framing),
        (no_start, TeletextPsiError::Framing),
        (crc, TeletextPsiError::Crc),
    ] {
        let mut discovery = TeletextPsiDiscovery::new_source_epoch(10, 512).unwrap();
        assert_eq!(discovery.push(&raw), Err(error.clone()));
        fenced(&mut discovery, error);
    }
    let mut discovery = TeletextPsiDiscovery::new_source_epoch(10, 512).unwrap();
    discovery.push(&flagged(one(0, 15, &table[..2]))).unwrap();
    assert_eq!(
        discovery.plan(&service()),
        Err(TeletextPsiError::Incomplete)
    );
    discovery.push(&packet(0, false, 0, &table[2..])).unwrap();
    discovery
        .push(&one(3337, 0, &pmt(10, 300, 300, &[])))
        .unwrap();
    assert!(discovery.plan(&service()).is_ok());
}

#[test]
fn initial_flag_does_not_reset_continuity_after_the_first_source_packet() {
    let mut discovery = TeletextPsiDiscovery::new_source_epoch(10, 512).unwrap();
    let table = pat(0, 0, 0, &[(10, 3337)]);
    discovery.push(&flagged(one(0, 15, &table))).unwrap();
    discovery.push(&one(0, 0, &table)).unwrap();
    assert_eq!(
        discovery.push(&one(0, 2, &table)),
        Err(TeletextPsiError::Continuity)
    );
    fenced(&mut discovery, TeletextPsiError::Continuity);
}
