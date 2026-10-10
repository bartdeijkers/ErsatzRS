use super::{
    psi_tests::{one, packet, pat, pmt, revise, section},
    signalling_tests::{page, pages},
    *,
};

fn service() -> TeletextServiceDescriptor {
    TeletextServiceDescriptor::from_page_set(&pages(vec![page(200, 0, "nld", false)]))
}

fn admission(cat: bool) -> TeletextPsiDiscovery {
    let mut discovery = TeletextPsiDiscovery::new(10, 512).unwrap();
    discovery
        .push(&one(0, 0, &pat(0, 0, 0, &[(10, 3337), (11, 3456)])))
        .unwrap();
    discovery
        .push(&one(3337, 0, &pmt(10, 300, 300, &[])))
        .unwrap();
    discovery
        .push(&one(3456, 0, &pmt(11, 301, 301, &[])))
        .unwrap();
    if cat {
        discovery
            .push(&one(
                1,
                0,
                &section(1, 0xffff, 0, 0, 0, &[9, 4, 0, 1, 0xe3, 0]),
            ))
            .unwrap();
    }
    discovery
}

fn monitor() -> TeletextPsiMonitor {
    admission(false).into_monitor(&service()).unwrap()
}

#[test]
fn admission_cannot_carry_unvalidated_orphan_duplicates_into_monitor() {
    for pid in [0, 3337, 3456] {
        for orphan in [
            packet(pid, false, 1, &[0x42; 184]),
            packet(pid, true, 1, &[1, 0x42, 0xff]),
        ] {
            let mut discovery = admission(false);
            assert_eq!(discovery.push(&orphan), Err(TeletextPsiError::Framing));
            assert_eq!(discovery.push(&orphan), Err(TeletextPsiError::Framing));
            assert!(matches!(
                discovery.into_monitor(&service()),
                Err(TeletextPsiError::Framing)
            ));
        }
    }
}

fn fenced(monitor: &mut TeletextPsiMonitor, error: TeletextPsiError) {
    assert_eq!(monitor.inspect_window(&[]), Err(error.clone()));
    assert_eq!(monitor.finish(), Err(error.clone()));
    assert_eq!(
        monitor.inspect_window(&[packet(8191, false, 0, &[0xff; 184])]),
        Err(error)
    );
}

#[test]
fn only_complete_admission_can_be_sealed_and_plan_remains_source_bound() {
    let discovery = TeletextPsiDiscovery::new(10, 512).unwrap();
    assert!(matches!(
        discovery.into_monitor(&service()),
        Err(TeletextPsiError::Incomplete)
    ));
    let discovery = admission(false);
    let original = discovery.plan(&service()).unwrap();
    let mut monitor = discovery.into_monitor(&service()).unwrap();
    assert_eq!(monitor.plan(), &original);
    assert_eq!(monitor.inspect_window(&[]), Ok(true));
    assert_eq!(
        monitor.inspect_window(&[one(3337, 1, &pmt(10, 300, 300, &[]))]),
        Ok(true)
    );
    assert_eq!(monitor.plan(), &original);
}

#[test]
fn monitoring_continues_beyond_startup_limit_through_repeated_counter_wraps() {
    let mut monitor = monitor();
    let pat = pat(0, 0, 0, &[(10, 3337), (11, 3456)]);
    let selected = pmt(10, 300, 300, &[]);
    let other = pmt(11, 301, 301, &[]);
    for turn in 1..=20_000 {
        let cc = u8::try_from(turn & 15).unwrap();
        assert_eq!(
            monitor.inspect_window(&[
                one(0, cc, &pat),
                one(3337, cc, &selected),
                one(3456, cc, &other)
            ]),
            Ok(true)
        );
    }
    monitor.finish().unwrap();
}

#[test]
fn fragmented_headers_across_windows_hold_readiness_until_exact_section_completes() {
    let table = pmt(10, 300, 300, &[]);
    for cut in 1..table.len() {
        let mut monitor = monitor();
        assert_eq!(
            monitor.inspect_window(&[one(3337, 1, &table[..cut])]),
            Ok(false)
        );
        assert_eq!(
            monitor.inspect_window(&[packet(300, false, 0, &[0; 184])]),
            Ok(false)
        );
        assert_eq!(
            monitor.inspect_window(&[packet(3337, false, 2, &table[cut..])]),
            Ok(true)
        );
        monitor.finish().unwrap();
    }
}

#[test]
fn every_observed_table_must_complete_before_output_can_be_released() {
    let mut monitor = monitor();
    let pat = pat(0, 0, 0, &[(10, 3337), (11, 3456)]);
    let other = pmt(11, 301, 301, &[]);
    assert_eq!(
        monitor.inspect_window(&[one(0, 1, &pat[..2]), one(3456, 1, &other[..4])]),
        Ok(false)
    );
    assert_eq!(
        monitor.inspect_window(&[packet(0, false, 2, &pat[2..])]),
        Ok(false)
    );
    assert_eq!(
        monitor.inspect_window(&[packet(3456, false, 2, &other[4..])]),
        Ok(true)
    );
}

#[test]
fn changed_pat_selected_or_other_program_cannot_replace_admitted_ownership() {
    let cases = [
        (0, pat(1, 0, 0, &[(10, 3337), (11, 3456)])),
        (0, pat(0, 0, 0, &[(10, 3337), (11, 512)])),
        (3337, revise(pmt(10, 300, 300, &[]), |s| s[5] += 2)),
        (3337, pmt(10, 400, 400, &[])),
        (3456, pmt(11, 301, 512, &[])),
    ];
    for (pid, changed) in cases {
        let mut monitor = monitor();
        let error = TeletextPsiError::SourceChanged { pid };
        assert_eq!(
            monitor.inspect_window(&[one(pid, 1, &changed)]),
            Err(error.clone())
        );
        fenced(&mut monitor, error);
    }
}

#[test]
fn changed_tail_is_rejected_after_held_prefix_without_admitting_partial_metadata() {
    let mut monitor = monitor();
    let changed = pmt(11, 301, 512, &[]);
    assert_eq!(
        monitor.inspect_window(&[one(3456, 1, &changed[..9])]),
        Ok(false)
    );
    let error = TeletextPsiError::SourceChanged { pid: 3456 };
    assert_eq!(
        monitor.inspect_window(&[packet(3456, false, 2, &changed[9..])]),
        Err(error.clone())
    );
    fenced(&mut monitor, error);
}

#[test]
fn cat_appearance_or_changed_ca_ownership_requires_new_admission() {
    for prior in [false, true] {
        let mut monitor = admission(prior).into_monitor(&service()).unwrap();
        let cat = section(1, 0xffff, 1, 0, 0, &[9, 4, 0, 1, 0xe2, 0]);
        let error = TeletextPsiError::SourceChanged { pid: 1 };
        assert_eq!(
            monitor.inspect_window(&[one(1, u8::from(prior), &cat)]),
            Err(error.clone())
        );
        fenced(&mut monitor, error);
    }
}

#[test]
fn future_tables_do_not_replace_current_ownership_and_selected_future_pmt_is_not_rewritten() {
    let mut monitor = monitor();
    let original = monitor.plan().clone();
    for (pid, table) in [
        (0, pat(1, 0, 0, &[(10, 3500)])),
        (1, section(1, 0xffff, 1, 0, 0, &[])),
        (3456, pmt(11, 301, 512, &[])),
    ] {
        let future = revise(table, |s| s[5] &= 0xfe);
        let cc = u8::from(pid != 1);
        assert_eq!(monitor.inspect_window(&[one(pid, cc, &future)]), Ok(true));
        assert_eq!(monitor.plan(), &original);
    }
    let future = revise(pmt(10, 300, 300, &[]), |s| s[5] &= 0xfe);
    let error = TeletextPsiError::SourceChanged { pid: 3337 };
    assert_eq!(
        monitor.inspect_window(&[one(3337, 1, &future)]),
        Err(error.clone())
    );
    fenced(&mut monitor, error);
}

#[test]
fn observed_pid_collision_crc_pointer_and_packet_errors_are_permanent() {
    let table = pmt(10, 300, 300, &[]);
    let mut bad_crc = table.clone();
    bad_crc[10] ^= 1;
    let mut bad_packet = packet(8191, false, 0, &[0xff; 184]);
    bad_packet[0] = 0;
    for (raw, error) in [
        (
            packet(512, false, 0, &[0; 184]),
            TeletextPsiError::Multiplex(TeletextMultiplexError::PidCollision { pid: 512 }),
        ),
        (one(3337, 1, &bad_crc), TeletextPsiError::Crc),
        (packet(3337, true, 1, &[2, 0, 0]), TeletextPsiError::Framing),
        (
            bad_packet,
            TeletextPsiError::Multiplex(TeletextMultiplexError::Packet),
        ),
    ] {
        let mut monitor = monitor();
        let original = [packet(300, false, 0, &[7; 184]), raw];
        let window = original;
        assert_eq!(monitor.inspect_window(&window), Err(error.clone()));
        assert_eq!(window, original);
        fenced(&mut monitor, error);
    }
}

#[test]
fn duplicates_adaptation_only_and_source_continuity_are_carried_across_admission() {
    let table = pmt(10, 300, 300, &[]);
    let mut monitor = monitor();
    let mut adaptation = [0xff; 188];
    adaptation[..6].copy_from_slice(&[0x47, 0x0d, 9, 0x20, 183, 0]);
    assert_eq!(
        monitor.inspect_window(&[one(3337, 0, &table), adaptation, one(3337, 1, &table)]),
        Ok(true)
    );
    let error = TeletextPsiError::Continuity;
    assert_eq!(
        monitor.inspect_window(&[one(3337, 3, &table)]),
        Err(error.clone())
    );
    fenced(&mut monitor, error);
    let mut replayed = monitor_after_admission();
    let error = TeletextPsiError::Continuity;
    assert_eq!(
        replayed.inspect_window(&[one(3337, 0, &table)]),
        Err(error.clone())
    );
    fenced(&mut replayed, error);
}

fn monitor_after_admission() -> TeletextPsiMonitor {
    let mut discovery = admission(false);
    discovery
        .push(&one(3337, 1, &pmt(10, 300, 300, &[])))
        .unwrap();
    discovery.into_monitor(&service()).unwrap()
}

#[test]
fn missing_payload_start_cannot_hide_a_new_section_after_admission() {
    let mut monitor = monitor();
    let table = pmt(10, 300, 300, &[]);
    let mut raw = one(3337, 1, &table);
    raw[1] &= 0xbf;
    assert_eq!(
        monitor.inspect_window(&[raw]),
        Err(TeletextPsiError::Framing)
    );
    fenced(&mut monitor, TeletextPsiError::Framing);
}

#[test]
fn eof_and_window_bounds_cannot_reopen_or_release_an_incomplete_generation() {
    let mut complete = monitor();
    complete.finish().unwrap();
    complete.finish().unwrap();
    assert_eq!(complete.inspect_window(&[]), Err(TeletextPsiError::Closed));
    let table = pmt(10, 300, 300, &[]);
    let mut incomplete = monitor();
    assert_eq!(
        incomplete.inspect_window(&[one(3337, 1, &table[..3])]),
        Ok(false)
    );
    assert_eq!(incomplete.finish(), Err(TeletextPsiError::Incomplete));
    assert_eq!(
        incomplete.inspect_window(&[packet(3337, false, 2, &table[3..])]),
        Err(TeletextPsiError::Incomplete)
    );
    let mut oversized = monitor();
    assert_eq!(
        oversized.inspect_window(&vec![packet(8191, false, 0, &[0xff; 184]); 32_769]),
        Err(TeletextPsiError::Capacity)
    );
    fenced(&mut oversized, TeletextPsiError::Capacity);
}
