//! An explicit source origin grants one physical-packet exception per PID.

use super::*;

fn flagged(mut raw: [u8; 188]) -> [u8; 188] {
    assert!(raw[3] & 0x20 != 0 && raw[4] > 0);
    raw[5] |= 0x80;
    raw
}

fn adaptation(pid: u16, flagged: bool) -> [u8; 188] {
    let [hi, lo] = pid.to_be_bytes();
    let mut raw = [0xff; 188];
    raw[..6].copy_from_slice(&[0x47, hi, lo, 0x20, 183, if flagged { 0x80 } else { 0 }]);
    raw
}

fn epoch_ready() -> SourceTeletextPsiObserver {
    let mut observer = SourceTeletextPsiObserver::new_source_epoch(PID).unwrap();
    observer
        .push(&flagged(one(0, 0, &pat(0, 0, 0, &[(10, 100)]))))
        .unwrap();
    observer
        .push(&flagged(one(100, 0, &source_pmt(10, 0, 0x100, PID, &[]))))
        .unwrap();
    observer
}

#[test]
fn explicit_epoch_admits_initial_psi_flags_but_requires_every_shared_definition() {
    let mut observer = SourceTeletextPsiObserver::new_source_epoch(PID).unwrap();
    observer
        .push(&flagged(one(0, 0, &pat(0, 0, 0, &[(10, 100), (20, 100)]))))
        .unwrap();
    observer
        .push(&flagged(one(1, 0, &section(1, 0xffff, 0, 0, 0, &[]))))
        .unwrap();
    observer
        .push(&flagged(one(100, 0, &source_pmt(10, 0, 0x100, PID, &[]))))
        .unwrap();
    assert_eq!(observer.observation(), Err(TeletextPsiError::Incomplete));
    observer
        .push(&one(100, 1, &source_pmt(20, 3, 0x101, PID, &[])))
        .unwrap();
    let report = observer.observation().unwrap();
    assert_eq!(report.programs.len(), 2);
    assert_eq!(report.memberships().count(), 2);
    assert_eq!(report.cat.unwrap().sections.len(), 1);
    assert!(!report.current_tables_changed);
}

#[test]
fn ordinary_source_constructor_still_rejects_initial_psi_flags() {
    for (pid, table) in [
        (0, pat(0, 0, 0, &[(10, 100)])),
        (1, section(1, 0xffff, 0, 0, 0, &[])),
        (100, source_pmt(10, 0, 0x100, PID, &[])),
    ] {
        let mut observer = SourceTeletextPsiObserver::new(PID).unwrap();
        if pid == 100 {
            observer
                .push(&one(0, 0, &pat(0, 0, 0, &[(10, 100)])))
                .unwrap();
        }
        assert_eq!(
            observer.push(&flagged(one(pid, 0, &table))),
            Err(TeletextPsiError::Continuity)
        );
        assert_fenced(&mut observer, TeletextPsiError::Continuity);
    }
}

#[test]
fn every_physical_packet_consumes_its_pid_allowance_even_before_pat_discovery() {
    for pid in [0, 1, 100] {
        for initial_flag in [false, true] {
            let mut observer = SourceTeletextPsiObserver::new_source_epoch(PID).unwrap();
            observer.push(&adaptation(pid, initial_flag)).unwrap();
            let table = match pid {
                0 => pat(0, 0, 0, &[(10, 100)]),
                1 => section(1, 0xffff, 0, 0, 0, &[]),
                _ => {
                    observer
                        .push(&one(0, 0, &pat(0, 0, 0, &[(10, 100)])))
                        .unwrap();
                    source_pmt(10, 0, 0x100, PID, &[])
                }
            };
            assert_eq!(
                observer.push(&flagged(one(pid, 1, &table))),
                Err(TeletextPsiError::Continuity)
            );
            assert_fenced(&mut observer, TeletextPsiError::Continuity);
        }
    }
    let mut observer = SourceTeletextPsiObserver::new_source_epoch(PID).unwrap();
    let pmt = source_pmt(10, 0, 0x100, PID, &[]);
    observer.push(&one(100, 0, &pmt)).unwrap();
    observer
        .push(&one(0, 0, &pat(0, 0, 0, &[(10, 100)])))
        .unwrap();
    assert_eq!(
        observer.push(&flagged(one(100, 1, &pmt))),
        Err(TeletextPsiError::Continuity)
    );
    assert_fenced(&mut observer, TeletextPsiError::Continuity);
}

#[test]
fn exact_flagged_duplicates_and_later_flags_remain_terminal() {
    for raw in [
        flagged(one(0, 0, &pat(0, 0, 0, &[(10, 100)]))),
        flagged(one(100, 0, &source_pmt(10, 0, 0x100, PID, &[]))),
        flagged(one(0, 1, &pat(0, 0, 0, &[(10, 100)]))),
        flagged(one(100, 1, &source_pmt(10, 0, 0x100, PID, &[]))),
    ] {
        let mut observer = epoch_ready();
        assert_eq!(observer.push(&raw), Err(TeletextPsiError::Continuity));
        assert_fenced(&mut observer, TeletextPsiError::Continuity);
    }
}

#[test]
fn pat_generation_and_continuation_never_restore_a_seen_pids_allowance() {
    for replace_pat in [false, true] {
        let mut observer = epoch_ready();
        observer.begin_continuation_window().unwrap();
        if replace_pat {
            observer
                .push(&one(0, 1, &pat(1, 0, 0, &[(10, 100)])))
                .unwrap();
            assert_eq!(observer.observation(), Err(TeletextPsiError::Incomplete));
        }
        assert_eq!(
            observer.push(&flagged(one(100, 1, &source_pmt(10, 0, 0x100, PID, &[])))),
            Err(TeletextPsiError::Continuity)
        );
        assert_eq!(
            observer.begin_continuation_window(),
            Err(TeletextPsiError::Continuity)
        );
        assert_fenced(&mut observer, TeletextPsiError::Continuity);
    }
}

#[test]
fn initial_psi_exception_keeps_crc_pointer_split_and_counter_fences() {
    let table = pat(0, 0, 0, &[(10, 100)]);
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
        let mut observer = SourceTeletextPsiObserver::new_source_epoch(PID).unwrap();
        assert_eq!(observer.push(&raw), Err(error.clone()));
        assert_fenced(&mut observer, error);
    }
    for cc in [0, 1] {
        let mut observer = SourceTeletextPsiObserver::new_source_epoch(PID).unwrap();
        observer.push(&flagged(one(0, 15, &table[..2]))).unwrap();
        assert_eq!(observer.observation(), Err(TeletextPsiError::Incomplete));
        let result = observer.push(&packet(0, false, cc, &table[2..]));
        if cc == 1 {
            assert_eq!(result, Err(TeletextPsiError::Continuity));
            assert_fenced(&mut observer, TeletextPsiError::Continuity);
        } else {
            result.unwrap();
            observer
                .push(&one(100, 0, &source_pmt(10, 0, 0x100, PID, &[])))
                .unwrap();
            assert!(observer.has_complete_current_tables());
        }
    }
}
