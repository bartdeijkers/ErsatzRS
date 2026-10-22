//! Program definitions share transport framing, but not identity or version state.

use super::*;

fn shared() -> SourceTeletextPsiObserver {
    let mut observer = SourceTeletextPsiObserver::new(PID).unwrap();
    observer
        .push(&one(0, 0, &pat(0, 0, 0, &[(10, 100), (20, 100)])))
        .unwrap();
    observer
        .push(&one(100, 0, &source_pmt(10, 0, 0x100, PID, &descriptors())))
        .unwrap();
    observer
        .push(&one(100, 1, &source_pmt(20, 3, 0x101, PID, &[0x83, 0])))
        .unwrap();
    observer
}

#[test]
fn shared_pmt_requires_each_definition_and_versions_change_independently() {
    let mut observer = SourceTeletextPsiObserver::new(PID).unwrap();
    observer
        .push(&one(0, 0, &pat(0, 0, 0, &[(10, 100), (20, 100)])))
        .unwrap();
    let first = one(100, 0, &source_pmt(10, 0, 0x100, PID, &descriptors()));
    observer.push(&first).unwrap();
    observer.push(&first).unwrap(); // An exact physical duplicate is harmless.
    assert_eq!(observer.observation(), Err(TeletextPsiError::Incomplete));
    assert!(!observer.has_complete_current_tables());
    observer
        .push(&one(100, 1, &source_pmt(20, 3, 0x101, PID, &[0x83, 0])))
        .unwrap();
    let initial = observer.observation().unwrap();
    assert_eq!(initial.programs.len(), 2);
    assert_eq!(initial.memberships().count(), 2);
    assert!(initial
        .programs
        .iter()
        .all(|program| program.pmt_pid == 100));
    assert!(!initial.current_tables_changed);
    let first_program = initial
        .programs
        .iter()
        .find(|program| program.program_number == 10)
        .unwrap();
    let second_program = initial
        .programs
        .iter()
        .find(|program| program.program_number == 20)
        .unwrap();
    assert_eq!(first_program.pmt.version, 0);
    assert_eq!(second_program.pmt.version, 3);
    observer
        .push(&one(100, 2, &source_pmt(10, 0, 0x100, PID, &descriptors())))
        .unwrap(); // A retransmitted definition also preserves the other program.
    let repeated = observer.observation().unwrap();
    assert_eq!(repeated.programs, initial.programs);
    assert!(!repeated.current_tables_changed);
    let changed = source_pmt(20, 4, 0x101, PID, &[0x83, 1, 0x42]);
    let update = one(100, 3, &changed);
    observer.push(&update).unwrap();
    observer.push(&update).unwrap();
    let updated = observer.observation().unwrap();
    assert_eq!(updated.programs.len(), 2);
    assert!(updated.current_tables_changed);
    assert_eq!(
        updated
            .programs
            .iter()
            .find(|program| program.program_number == 10)
            .unwrap(),
        first_program
    );
    let second = updated
        .programs
        .iter()
        .find(|program| program.program_number == 20)
        .unwrap();
    assert_eq!(second.pmt.version, 4);
    assert_eq!(second.pmt.sections, [changed]);
}

#[test]
fn shared_pmt_same_program_same_version_contradictions_are_sticky() {
    for (program, version, pcr) in [(10, 0, 0x100), (20, 3, 0x101)] {
        let mut observer = shared();
        assert_eq!(
            observer.push(&one(
                100,
                2,
                &source_pmt(program, version, pcr, PID, &[0x83, 1, 0x42]),
            )),
            Err(TeletextPsiError::Version)
        );
        assert_fenced(&mut observer, TeletextPsiError::Version);
    }
}

#[test]
fn shared_pmt_new_pat_generation_reacquires_every_definition_even_on_retained_pid() {
    for pmt_pid in [100, 200] {
        let mut observer = shared();
        observer
            .push(&one(0, 1, &pat(1, 0, 0, &[(10, pmt_pid), (20, pmt_pid)])))
            .unwrap();
        assert_eq!(observer.observation(), Err(TeletextPsiError::Incomplete));
        let first_cc = if pmt_pid == 100 { 2 } else { 0 };
        observer
            .push(&one(
                pmt_pid,
                first_cc,
                &source_pmt(10, 0, 0x100, PID, &descriptors()),
            ))
            .unwrap();
        assert_eq!(observer.observation(), Err(TeletextPsiError::Incomplete));
        observer
            .push(&one(
                pmt_pid,
                first_cc + 1,
                &source_pmt(20, 3, 0x101, PID, &[0x83, 0]),
            ))
            .unwrap();
        let rebound = observer.observation().unwrap();
        assert_eq!(rebound.pat.version, 1);
        assert_eq!(rebound.programs.len(), 2);
        assert!(rebound
            .programs
            .iter()
            .all(|program| program.pmt_pid == pmt_pid));
        assert!(rebound.current_tables_changed);
    }
}

#[test]
fn shared_pmt_does_not_allow_duplicate_programs_or_network_pid_collisions() {
    for entries in [
        vec![(10, 100), (10, 100)],
        vec![(10, 100), (10, 200)],
        vec![(0, 100), (10, 100)],
        vec![(10, 100), (0, 100)],
    ] {
        let mut observer = SourceTeletextPsiObserver::new(PID).unwrap();
        assert_eq!(
            observer.push(&one(0, 0, &pat(0, 0, 0, &entries))),
            Err(TeletextPsiError::Pat)
        );
        assert_fenced(&mut observer, TeletextPsiError::Pat);
    }
}

#[test]
fn shared_pmt_program_bound_counts_definitions_instead_of_physical_pids() {
    for count in [128, 129] {
        let mut observer = SourceTeletextPsiObserver::new(PID).unwrap();
        let entries: Vec<_> = (1..=count).map(|program| (program, 100)).collect();
        let table = pat(0, 0, 0, &entries);
        observer.push(&one(0, 0, &table[..183])).unwrap();
        let mut result = Ok(());
        for (index, payload) in table[183..].chunks(184).enumerate() {
            result = observer.push(&packet(0, false, u8::try_from(index + 1).unwrap(), payload));
        }
        if count == 129 {
            assert_eq!(result, Err(TeletextPsiError::Capacity));
            assert_fenced(&mut observer, TeletextPsiError::Capacity);
            continue;
        }
        result.unwrap();
        for program in 1..=count {
            observer
                .push(&one(
                    100,
                    u8::try_from((program - 1) % 16).unwrap(),
                    &source_pmt(program, 0, 0x100, PID, &[]),
                ))
                .unwrap();
            if program < count {
                assert_eq!(observer.observation(), Err(TeletextPsiError::Incomplete));
            }
        }
        let report = observer.observation().unwrap();
        assert_eq!(report.programs.len(), 128);
        assert_eq!(report.memberships().count(), 128);
    }
}

#[test]
fn shared_pmt_keeps_zero_section_and_last_section_guards() {
    for (number, last) in [(1, 1), (0, 1)] {
        let mut observer = SourceTeletextPsiObserver::new(PID).unwrap();
        observer
            .push(&one(0, 0, &pat(0, 0, 0, &[(10, 100), (20, 100)])))
            .unwrap();
        let malformed = revise(source_pmt(10, 0, 0x100, PID, &[]), |bytes| {
            bytes[6] = number;
            bytes[7] = last;
        });
        let error = TeletextPsiError::Multiplex(TeletextMultiplexError::Pmt);
        assert_eq!(observer.push(&one(100, 0, &malformed)), Err(error.clone()));
        assert_fenced(&mut observer, error);
    }
}
