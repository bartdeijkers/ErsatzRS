use super::*;
use crate::teletext::{
    psi_tests::{one, packet, pat, revise, section},
    TeletextMultiplexError, TeletextPsiDiscovery,
};

const PID: u16 = 0x103;

fn source_pmt(program: u16, version: u8, pcr: u16, pid: u16, descriptors: &[u8]) -> Vec<u8> {
    let [ph, pl] = pcr.to_be_bytes();
    let [eh, el] = pid.to_be_bytes();
    let [dh, dl] = u16::try_from(descriptors.len()).unwrap().to_be_bytes();
    // Retain an unknown program descriptor and all ES descriptor records.
    let mut body = vec![0xe0 | ph, pl, 0xf0, 3, 0x80, 1, 0x5a];
    body.extend_from_slice(&[6, 0xe0 | eh, el, 0xf0 | dh, dl]);
    body.extend_from_slice(descriptors);
    section(2, program, version, 0, 0, &body)
}

fn descriptors() -> Vec<u8> {
    vec![
        0x56, 10, b'e', b'n', b'g', 0x09, 0x00, b'n', b'l', b'd', 0x10, 0x88, 0x82, 1, 0x41,
    ]
}

fn ready(pid: u16) -> SourceTeletextPsiObserver {
    let mut observer = SourceTeletextPsiObserver::new(pid).unwrap();
    observer
        .push(&one(0, 0, &pat(0, 0, 0, &[(10, 100)])))
        .unwrap();
    observer
        .push(&one(100, 0, &source_pmt(10, 0, 0x100, PID, &descriptors())))
        .unwrap();
    observer
}

fn assert_fenced(observer: &mut SourceTeletextPsiObserver, error: TeletextPsiError) {
    assert_eq!(observer.observation(), Err(error.clone()));
    assert_eq!(
        observer.push(&packet(0x1fff, false, 0, &[0xff])),
        Err(error)
    );
}

#[test]
fn source_psi_preserves_occupied_pid_exact_descriptors_and_all_program_memberships() {
    let mut observer = ready(PID);
    let report = observer.observation().unwrap();
    assert_eq!(report.requested_pid, PID);
    assert_eq!(report.pat.table_id_extension, 7);
    assert_eq!(report.pat.version, 0);
    assert_eq!(report.pat.sections, [pat(0, 0, 0, &[(10, 100)])]);
    assert_eq!(report.cat, None);
    let memberships: Vec<_> = report.memberships().collect();
    assert_eq!(memberships.len(), 1);
    assert_eq!(memberships[0].0.program_number, 10);
    assert_eq!(memberships[0].0.program_descriptors, [0x80, 1, 0x5a]);
    assert_eq!(memberships[0].1.stream_type, 6);
    assert_eq!(memberships[0].1.descriptors, descriptors());
    assert_eq!(
        memberships[0].0.pmt.sections,
        [source_pmt(10, 0, 0x100, PID, &descriptors())]
    );
    // Actual data on the occupied PID is not an authored-output collision.
    observer
        .push(&packet(PID, true, 0, &[0, 0, 1, 0xbd]))
        .unwrap();
    assert_eq!(observer.observation().unwrap().memberships().count(), 1);
    // Existing authored admission still reserves an ABSENT PID and rejects it.
    let mut authored = TeletextPsiDiscovery::new(10, PID).unwrap();
    authored
        .push(&one(0, 0, &pat(0, 0, 0, &[(10, 100)])))
        .unwrap();
    assert_eq!(
        authored.push(&one(100, 0, &source_pmt(10, 0, 0x100, PID, &descriptors()))),
        Err(TeletextMultiplexError::PidCollision { pid: PID }.into())
    );
}

#[test]
fn source_psi_requires_every_pmt_and_retains_shared_pid_without_selecting_first() {
    let mut observer = SourceTeletextPsiObserver::new(PID).unwrap();
    observer
        .push(&one(0, 0, &pat(0, 0, 0, &[(10, 100), (20, 200)])))
        .unwrap();
    observer
        .push(&one(100, 0, &source_pmt(10, 0, 0x100, PID, &descriptors())))
        .unwrap();
    assert_eq!(observer.observation(), Err(TeletextPsiError::Incomplete));
    observer
        .push(&one(200, 0, &source_pmt(20, 3, 0x101, PID, &[0x83, 0])))
        .unwrap();
    let report = observer.observation().unwrap();
    let memberships: Vec<_> = report
        .memberships()
        .map(|(program, stream)| {
            (
                program.program_number,
                program.pmt_pid,
                program.pmt.version,
                stream.descriptors.clone(),
            )
        })
        .collect();
    assert_eq!(
        memberships,
        [(10, 100, 0, descriptors()), (20, 200, 3, vec![0x83, 0])]
    );
}

#[test]
fn source_psi_absent_pid_and_no_pcr_are_report_only_not_admission_failures() {
    let absent = ready(0x104).observation().unwrap();
    assert_eq!(absent.programs.len(), 1);
    assert_eq!(absent.memberships().count(), 0);
    let mut observer = SourceTeletextPsiObserver::new(PID).unwrap();
    observer
        .push(&one(0, 0, &pat(0, 0, 0, &[(10, 100)])))
        .unwrap();
    let pmt = source_pmt(10, 0, 0x1fff, PID, &[]);
    observer.push(&one(100, 0, &pmt)).unwrap();
    assert_eq!(observer.observation().unwrap().programs[0].pcr_pid, 0x1fff);
    let mut authored = TeletextPsiDiscovery::new(10, 0x104).unwrap();
    authored
        .push(&one(0, 0, &pat(0, 0, 0, &[(10, 100)])))
        .unwrap();
    assert_eq!(
        authored.push(&one(100, 0, &pmt)),
        Err(TeletextMultiplexError::Clock.into())
    );
}

#[test]
fn source_psi_current_pat_change_invalidates_dependents_and_future_pat_does_not() {
    let mut observer = ready(PID);
    let initial = observer.observation().unwrap();
    let future = revise(pat(1, 0, 0, &[(20, 200)]), |bytes| bytes[5] &= !1);
    observer.push(&one(0, 1, &future)).unwrap();
    assert_eq!(observer.observation().unwrap().pat, initial.pat);
    observer
        .push(&one(0, 2, &pat(1, 0, 0, &[(20, 200)])))
        .unwrap();
    assert_eq!(observer.observation(), Err(TeletextPsiError::Incomplete));
    observer
        .push(&one(200, 0, &source_pmt(20, 4, 0x101, PID, &[])))
        .unwrap();
    let current = observer.observation().unwrap();
    assert_eq!(current.pat.version, 1);
    assert_eq!(current.programs.len(), 1);
    assert_eq!(current.programs[0].program_number, 20);
    // The earlier owned snapshot remains historical, not a live readiness token.
    assert_eq!(initial.programs[0].program_number, 10);
}

#[test]
fn source_psi_partial_pat_and_cat_block_snapshots_until_complete() {
    let mut observer = SourceTeletextPsiObserver::new(PID).unwrap();
    observer
        .push(&one(0, 0, &pat(0, 0, 1, &[(10, 100)])))
        .unwrap();
    assert_eq!(observer.observation(), Err(TeletextPsiError::Incomplete));
    observer
        .push(&one(0, 1, &pat(0, 1, 1, &[(20, 200)])))
        .unwrap();
    observer
        .push(&one(100, 0, &source_pmt(10, 0, 0x100, PID, &[])))
        .unwrap();
    observer
        .push(&one(200, 0, &source_pmt(20, 0, 0x101, PID, &[])))
        .unwrap();
    let ca = [9, 4, 0, 1, 0xe1, 0x03]; // The observed PID may also occur in CA metadata.
    observer
        .push(&one(1, 0, &section(1, 1, 0, 0, 1, &ca)))
        .unwrap();
    assert_eq!(observer.observation(), Err(TeletextPsiError::Incomplete));
    observer
        .push(&one(1, 1, &section(1, 1, 0, 1, 1, &[0x80, 0])))
        .unwrap();
    let cat = observer.observation().unwrap().cat.unwrap();
    assert_eq!(cat.sections.len(), 2);
    assert_eq!(cat.sections[0], section(1, 1, 0, 0, 1, &ca));
}

#[test]
fn source_psi_changed_pmt_replaces_records_but_same_version_contradiction_fences() {
    let mut observer = ready(PID);
    let future = revise(source_pmt(10, 1, 0x100, 0x104, &[]), |bytes| bytes[5] &= !1);
    observer.push(&one(100, 1, &future)).unwrap();
    assert_eq!(observer.observation().unwrap().memberships().count(), 1);
    observer
        .push(&one(100, 2, &source_pmt(10, 1, 0x100, 0x104, &[])))
        .unwrap();
    assert_eq!(observer.observation().unwrap().memberships().count(), 0);
    assert_eq!(
        observer.push(&one(100, 3, &source_pmt(10, 1, 0x100, PID, &[]))),
        Err(TeletextPsiError::Version)
    );
    assert_fenced(&mut observer, TeletextPsiError::Version);
}

#[test]
fn source_psi_contradictory_pat_and_cat_are_sticky() {
    let mut observer = ready(PID);
    assert_eq!(
        observer.push(&one(0, 1, &pat(0, 0, 0, &[(20, 200)]))),
        Err(TeletextPsiError::Version)
    );
    assert_fenced(&mut observer, TeletextPsiError::Version);
    let mut observer = ready(PID);
    observer
        .push(&one(1, 0, &section(1, 1, 0, 0, 0, &[])))
        .unwrap();
    assert_eq!(
        observer.push(&one(1, 1, &section(1, 1, 0, 0, 0, &[0x80, 0]))),
        Err(TeletextPsiError::Version)
    );
    assert_fenced(&mut observer, TeletextPsiError::Version);
}

#[test]
fn source_psi_crc_descriptor_program_and_continuity_failures_are_sticky() {
    let mut observer = ready(PID);
    let mut corrupt = pat(1, 0, 0, &[(10, 100)]);
    corrupt[10] ^= 1;
    assert_eq!(
        observer.push(&one(0, 1, &corrupt)),
        Err(TeletextPsiError::Crc)
    );
    assert_fenced(&mut observer, TeletextPsiError::Crc);
    let mut observer = ready(PID);
    let error = TeletextPsiError::Multiplex(TeletextMultiplexError::Descriptor);
    assert_eq!(
        observer.push(&one(100, 1, &source_pmt(10, 1, 0x100, PID, &[0x56, 5, 0]))),
        Err(error.clone())
    );
    assert_fenced(&mut observer, error);
    let mut observer = ready(PID);
    assert_eq!(
        observer.push(&one(100, 1, &source_pmt(11, 1, 0x100, PID, &[]))),
        Err(TeletextPsiError::Program)
    );
    assert_fenced(&mut observer, TeletextPsiError::Program);
    let mut observer = ready(PID);
    assert_eq!(
        observer.push(&one(100, 2, &source_pmt(10, 1, 0x100, PID, &[]))),
        Err(TeletextPsiError::Continuity)
    );
    assert_fenced(&mut observer, TeletextPsiError::Continuity);
}

#[test]
fn source_psi_split_sections_are_incomplete_until_finished_and_exact_duplicates_are_safe() {
    let mut observer = ready(PID);
    let pmt = source_pmt(10, 1, 0x100, PID, &descriptors());
    let first = one(100, 1, &pmt[..6]);
    observer.push(&first).unwrap();
    observer.push(&first).unwrap();
    assert_eq!(observer.observation(), Err(TeletextPsiError::Incomplete));
    observer.push(&packet(100, false, 2, &pmt[6..])).unwrap();
    assert_eq!(observer.observation().unwrap().programs[0].pmt.version, 1);
}

#[test]
fn source_psi_pid_constructor_and_packet_bound_are_exact() {
    for pid in [0, 1, 0x1f, 0x1fff, 0x2000, u16::MAX] {
        assert!(matches!(
            SourceTeletextPsiObserver::new(pid),
            Err(TeletextPsiError::Multiplex(TeletextMultiplexError::Pid))
        ));
    }
    for pid in [0x20, 0x1ffe] {
        assert!(SourceTeletextPsiObserver::new(pid).is_ok());
    }
    let mut observer = SourceTeletextPsiObserver::new(PID).unwrap();
    let null = packet(0x1fff, false, 0, &[0xff]);
    for _ in 0..MAX_DISCOVERY_PACKETS {
        observer.push(&null).unwrap();
    }
    assert_eq!(observer.observation(), Err(TeletextPsiError::Incomplete));
    assert_eq!(observer.push(&null), Err(TeletextPsiError::Capacity));
    assert_fenced(&mut observer, TeletextPsiError::Capacity);
}

#[test]
fn source_psi_program_count_bound_uses_shared_pat_validation() {
    let mut observer = SourceTeletextPsiObserver::new(PID).unwrap();
    let entries: Vec<_> = (1..=129).map(|program| (program, 1000 + program)).collect();
    let table = pat(0, 0, 0, &entries);
    observer.push(&one(0, 0, &table[..183])).unwrap();
    let mut result = Ok(());
    for (index, payload) in table[183..].chunks(184).enumerate() {
        result = observer.push(&packet(0, false, u8::try_from(index + 1).unwrap(), payload));
    }
    assert_eq!(result, Err(TeletextPsiError::Capacity));
    assert_fenced(&mut observer, TeletextPsiError::Capacity);
}

#[test]
fn source_psi_change_witness_survives_a_return_to_the_initial_wire_tables() {
    let mut observer = ready(PID);
    let initial = observer.observation().unwrap();
    assert!(!initial.current_tables_changed);
    assert!(observer.has_complete_current_tables());
    observer
        .push(&one(100, 1, &source_pmt(10, 1, 0x100, PID, &descriptors())))
        .unwrap();
    assert!(observer.observation().unwrap().current_tables_changed);
    observer
        .push(&one(100, 2, &source_pmt(10, 0, 0x100, PID, &descriptors())))
        .unwrap();
    let returned = observer.observation().unwrap();
    assert_eq!(returned.programs, initial.programs);
    assert!(returned.current_tables_changed);
}
