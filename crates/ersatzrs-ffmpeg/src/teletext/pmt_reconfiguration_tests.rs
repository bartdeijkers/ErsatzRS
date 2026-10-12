use super::{TeletextPmtInstallError, TeletextPmtInstaller};
use crate::teletext::{
    psi_framing::SectionAssembler,
    psi_tests::{one, packet, pmt, revise},
    signalling_tests::{page, pages},
    TeletextMultiplexError, TeletextPmtPlan, TeletextPsiError, TeletextServiceDescriptor,
};
use mpeg2ts_reader::{mpegts_crc, packet::Packet};

fn descriptor(number: u16) -> TeletextServiceDescriptor {
    TeletextServiceDescriptor::from_page_set(&pages(vec![page(number, 0, "eng", false)]))
}

fn plan() -> TeletextPmtPlan {
    TeletextPmtPlan::new(&pmt(10, 300, 300, &[]), 3337, 512, &descriptor(200)).unwrap()
}

fn av() -> [u8; 188] {
    packet(300, false, 0, &[7; 184])
}

fn null() -> [u8; 188] {
    packet(8191, false, 0, &[0xff; 184])
}

fn tables(window: &[[u8; 188]]) -> Vec<Vec<u8>> {
    let mut assembler = SectionAssembler::default();
    let mut result = Vec::new();
    for raw in window {
        let packet = Packet::new(raw);
        if u16::from(packet.pid()) == 3337 {
            result.extend(assembler.push(&packet).unwrap());
        }
    }
    for section in &result {
        assert_eq!(mpegts_crc::sum32(section), 0);
    }
    result
}

#[test]
fn first_reconfiguration_announces_from_null_capacity_without_source_repetition() {
    let first = plan();
    let mut installer = TeletextPmtInstaller::new(first);
    let next = installer
        .prepare_reconfiguration(Some(&descriptor(201)))
        .unwrap();
    installer.reconfigure(next.clone()).unwrap();
    let mut window = [av(), null(), av()];
    let report = installer.install(&mut window).unwrap();
    assert!(report.announced);
    assert_eq!(report.repetitions, 0);
    assert_eq!(report.pmt_packets, 1);
    assert_eq!(report.consumed_null_packets, 1);
    assert_eq!(report.retired_pmt_packets, 0);
    assert_eq!(window[0], av());
    assert_eq!(window[2], av());
    assert_eq!(window[1][3] & 15, 0);
    assert_eq!(tables(&window), [next.section().to_vec()]);
    assert_eq!(installer.next_continuity_counter(), Some(1));
    let mut unchanged = [av(), null()];
    assert!(!installer.install(&mut unchanged).unwrap().announced);
    assert_eq!(unchanged, [av(), null()]);
}

#[test]
fn source_and_output_continuity_survive_change_withdrawal_and_readdition() {
    let first = plan();
    let mut installer = TeletextPmtInstaller::new(first.clone());
    let mut initial = [one(3337, 14, first.source_section())];
    installer.install(&mut initial).unwrap();
    assert_eq!(initial[0][3] & 15, 14);
    for (selection, source_cc, output_cc) in [(Some(201), 15, 15), (None, 0, 0), (Some(202), 1, 1)]
    {
        let selected = selection.map(descriptor);
        let next = installer
            .prepare_reconfiguration(selected.as_ref())
            .unwrap();
        installer.reconfigure(next.clone()).unwrap();
        let mut window = [av(), one(3337, source_cc, first.source_section()), av()];
        let report = installer.install(&mut window).unwrap();
        assert!(report.announced);
        assert_eq!(report.repetitions, 1);
        assert_eq!(window[1][3] & 15, output_cc);
        assert_eq!(tables(&window), [next.section().to_vec()]);
        assert_eq!(window[0], av());
        assert_eq!(window[2], av());
    }
    let before_cc = installer.next_continuity_counter();
    let mut wrong = [one(3337, 3, first.source_section())];
    let original = wrong;
    assert_eq!(
        installer.install(&mut wrong),
        Err(TeletextPmtInstallError::Psi(TeletextPsiError::Continuity))
    );
    assert_eq!(wrong, original);
    assert_eq!(installer.next_continuity_counter(), before_cc);
}

#[test]
fn announcement_capacity_and_invalid_packet_failures_leave_pending_plan_atomic() {
    let mut installer = TeletextPmtInstaller::new(plan());
    let next = installer.prepare_reconfiguration(None).unwrap();
    installer.reconfigure(next.clone()).unwrap();
    let mut no_capacity = [av()];
    assert_eq!(
        installer.install(&mut no_capacity),
        Err(TeletextPmtInstallError::Multiplex(
            TeletextMultiplexError::Capacity {
                needed: 1,
                available: 0
            }
        ))
    );
    assert_eq!(no_capacity, [av()]);
    assert_eq!(installer.next_continuity_counter(), None);
    assert!(installer.pending_announcement);
    let mut malformed = [null()];
    malformed[0][0] = 0;
    let original = malformed;
    assert!(installer.install(&mut malformed).is_err());
    assert_eq!(malformed, original);
    assert_eq!(installer.plan, next);
    assert!(installer.pending_announcement);
    let mut retry = [null(), av()];
    assert!(installer.install(&mut retry).unwrap().announced);
    assert_eq!(tables(&retry), [next.section().to_vec()]);
}

#[test]
fn foreign_ownership_and_stale_plans_are_rejected_without_mutation() {
    let first = plan();
    let mut installer = TeletextPmtInstaller::new(first.clone());
    let service = descriptor(201);
    let changed_source = revise(first.source_section().to_vec(), |section| section[4] = 11);
    let foreign = [
        first.clone(),
        TeletextPmtPlan::new(first.source_section(), 3338, 512, &service)
            .unwrap()
            .reconfigure(None)
            .unwrap(),
        TeletextPmtPlan::new(first.source_section(), 3337, 513, &service)
            .unwrap()
            .reconfigure(None)
            .unwrap(),
        TeletextPmtPlan::new(&changed_source, 3337, 512, &service)
            .unwrap()
            .reconfigure(None)
            .unwrap(),
    ];
    for proposed in foreign {
        assert_eq!(
            installer.reconfigure(proposed),
            Err(TeletextPmtInstallError::Reconfiguration)
        );
        assert_eq!(installer.plan, first);
        assert_eq!(installer.next_continuity_counter(), None);
        assert!(!installer.pending_announcement);
    }
    let next = installer.prepare_reconfiguration(None).unwrap();
    installer.reconfigure(next.clone()).unwrap();
    let mut announced = [null()];
    installer.install(&mut announced).unwrap();
    assert_eq!(
        installer.reconfigure(next.clone()),
        Err(TeletextPmtInstallError::Reconfiguration)
    );
    assert_eq!(installer.plan, next);
}

#[test]
fn pending_source_fragment_blocks_preparation_and_commit_without_resetting_it() {
    let first = plan();
    let mut installer = TeletextPmtInstaller::new(first.clone());
    let fragment = one(3337, 7, &first.source_section()[..8]);
    installer.input.push(&Packet::new(&fragment)).unwrap();
    assert!(installer.input.is_pending());
    assert_eq!(
        installer.prepare_reconfiguration(None),
        Err(TeletextPmtInstallError::Incomplete)
    );
    assert_eq!(
        installer.reconfigure(first.reconfigure(None).unwrap()),
        Err(TeletextPmtInstallError::Incomplete)
    );
    assert!(installer.input.is_pending());
    assert_eq!(installer.plan, first);
    assert!(!installer.pending_announcement);
}

#[test]
fn withdrawal_keeps_reserved_pid_future_source_and_window_limit_fences() {
    let first = plan();
    let mut installer = TeletextPmtInstaller::new(first.clone());
    installer
        .reconfigure(first.reconfigure(None).unwrap())
        .unwrap();
    let mut occupied = [packet(512, false, 0, &[0; 184]), null()];
    let original = occupied;
    assert_eq!(
        installer.install(&mut occupied),
        Err(TeletextPmtInstallError::Multiplex(
            TeletextMultiplexError::PidCollision { pid: 512 }
        ))
    );
    assert_eq!(occupied, original);
    let future = revise(first.source_section().to_vec(), |section| section[5] &= !1);
    let mut changed = [one(3337, 0, &future), null()];
    let original = changed;
    assert_eq!(
        installer.install(&mut changed),
        Err(TeletextPmtInstallError::SourceChanged)
    );
    assert_eq!(changed, original);
    let mut oversized = vec![null(); 32_769];
    assert_eq!(
        installer.install(&mut oversized),
        Err(TeletextPmtInstallError::Multiplex(
            TeletextMultiplexError::Window
        ))
    );
    assert!(oversized.iter().all(|raw| raw == &null()));
    assert!(installer.pending_announcement);
    assert_eq!(installer.next_continuity_counter(), None);
}

#[test]
fn synthetic_announcement_preserves_first_source_startup_reset_ownership() {
    let first = plan();
    let mut installer = TeletextPmtInstaller::new_source_epoch(first.clone());
    let next = installer.prepare_reconfiguration(None).unwrap();
    installer.reconfigure(next).unwrap();
    installer.install(&mut [null()]).unwrap();
    assert!(installer.first_source_epoch_packet);
    let mut initial = one(3337, 12, first.source_section());
    assert!(initial[3] & 0x20 != 0);
    initial[5] = 0x80;
    let mut window = [initial];
    installer.install(&mut window).unwrap();
    assert!(!installer.first_source_epoch_packet);
    assert_eq!(window[0][3] & 15, 1);
    let mut later = one(3337, 13, first.source_section());
    later[5] = 0x80;
    assert_eq!(
        installer.install(&mut [later]),
        Err(TeletextPmtInstallError::Adaptation)
    );
    assert_eq!(installer.next_continuity_counter(), Some(2));
}

#[test]
fn synthetic_announcement_uses_only_null_slots_and_keeps_existing_source_cc() {
    let first = plan();
    let mut installer = TeletextPmtInstaller::new(first.clone());
    installer
        .install(&mut [one(3337, 9, first.source_section())])
        .unwrap();
    let next = installer.prepare_reconfiguration(None).unwrap();
    installer.reconfigure(next.clone()).unwrap();
    let mut adaptation = [0xff; 188];
    adaptation[..6].copy_from_slice(&[0x47, 0x0d, 0x09, 0x29, 183, 0]);
    let mut no_null = [adaptation, av()];
    let original = no_null;
    assert_eq!(
        installer.install(&mut no_null),
        Err(TeletextPmtInstallError::Multiplex(
            TeletextMultiplexError::Capacity {
                needed: 1,
                available: 0
            }
        ))
    );
    assert_eq!(no_null, original);
    assert_eq!(installer.next_continuity_counter(), Some(10));
    let mut announced = [av(), null()];
    assert!(installer.install(&mut announced).unwrap().announced);
    assert_eq!(announced[0], av());
    assert_eq!(announced[1][3] & 15, 10);
    assert_eq!(tables(&announced), [next.section().to_vec()]);
    let mut source = [one(3337, 10, first.source_section())];
    assert!(!installer.install(&mut source).unwrap().announced);
    assert_eq!(source[0][3] & 15, 11);
}
