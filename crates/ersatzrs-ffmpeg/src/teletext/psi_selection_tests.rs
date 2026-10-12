//! Exact source-program policy over the existing complete ownership parser.

use super::{TeletextProgramSelection, TeletextPsiDiscovery, TeletextPsiError};
use crate::teletext::{
    psi_tests::{one, packet, pat, pmt, revise, section},
    signalling_tests::{page, pages},
    TeletextMultiplexError, TeletextServiceDescriptor,
};

fn service() -> TeletextServiceDescriptor {
    TeletextServiceDescriptor::from_page_set(&pages(vec![page(200, 0, "nld", false)]))
}

fn sole() -> TeletextPsiDiscovery {
    TeletextPsiDiscovery::new_source_epoch_selection(TeletextProgramSelection::SoleProgram, 512)
        .unwrap()
}

fn ready() -> TeletextPsiDiscovery {
    let mut discovery = sole();
    discovery
        .push(&one(0, 0, &pat(0, 0, 0, &[(0, 16), (37, 3337)])))
        .unwrap();
    discovery
        .push(&one(3337, 0, &pmt(37, 300, 301, &[])))
        .unwrap();
    discovery
}

fn flagged(mut raw: [u8; 188]) -> [u8; 188] {
    assert!(raw[3] & 0x20 != 0 && raw[4] > 0);
    raw[5] |= 0x80;
    raw
}

#[test]
fn sole_program_uses_actual_non_one_identity_and_excludes_network_entry() {
    let discovery = ready();
    let plan = discovery.plan(&service()).unwrap();
    assert_eq!(
        (plan.program_number(), plan.pmt_pid(), plan.pcr_pid()),
        (37, 3337, 300)
    );
    assert_eq!(plan.source_section(), pmt(37, 300, 301, &[]));
    let mut monitor = discovery.into_monitor(&service()).unwrap();
    assert!(monitor
        .inspect_window(&[
            one(0, 1, &pat(0, 0, 0, &[(0, 16), (37, 3337)])),
            one(3337, 1, &pmt(37, 300, 301, &[])),
        ])
        .unwrap());
    let changed = one(0, 2, &pat(1, 0, 0, &[(38, 3456)]));
    assert_eq!(
        monitor.inspect_window(&[changed]),
        Err(TeletextPsiError::SourceChanged { pid: 0 })
    );
    assert_eq!(
        monitor.finish(),
        Err(TeletextPsiError::SourceChanged { pid: 0 })
    );
}

#[test]
fn partial_and_future_pat_never_choose_a_program_or_restore_old_pmt_evidence() {
    let mut discovery = sole();
    let future = revise(pat(1, 0, 0, &[(1, 3456)]), |data| data[5] &= 0xfe);
    discovery.push(&one(0, 0, &future)).unwrap();
    assert_eq!(
        discovery.plan(&service()),
        Err(TeletextPsiError::Incomplete)
    );
    discovery
        .push(&one(0, 1, &pat(0, 1, 1, &[(37, 3337)])))
        .unwrap();
    assert_eq!(
        discovery.plan(&service()),
        Err(TeletextPsiError::Incomplete)
    );
    discovery
        .push(&one(0, 2, &pat(0, 0, 1, &[(0, 16)])))
        .unwrap();
    assert_eq!(
        discovery.plan(&service()),
        Err(TeletextPsiError::Incomplete)
    );
    discovery
        .push(&one(3337, 0, &pmt(37, 300, 301, &[])))
        .unwrap();
    let original = discovery.plan(&service()).unwrap();
    discovery.push(&one(0, 3, &future)).unwrap();
    assert_eq!(discovery.plan(&service()).unwrap(), original);
    discovery
        .push(&one(0, 4, &pat(1, 0, 0, &[(41, 3456)])))
        .unwrap();
    assert_eq!(
        discovery.plan(&service()),
        Err(TeletextPsiError::Incomplete)
    );
    discovery
        .push(&one(3456, 0, &pmt(41, 400, 401, &[])))
        .unwrap();
    let new = discovery.plan(&service()).unwrap();
    assert_eq!((new.program_number(), new.pcr_pid()), (41, 400));
}

#[test]
fn multiple_programs_wait_for_all_pmts_and_cat_then_require_explicit_selection() {
    let inputs = [
        one(0, 0, &pat(0, 0, 0, &[(37, 3337), (41, 3456)])),
        one(3337, 0, &pmt(37, 300, 301, &[])),
        one(1, 0, &section(1, 0xffff, 0, 0, 1, &[])),
        one(3456, 0, &pmt(41, 400, 401, &[])),
    ];
    let mut discovery = sole();
    let mut explicit = TeletextPsiDiscovery::new_source_epoch(41, 512).unwrap();
    for raw in inputs {
        discovery.push(&raw).unwrap();
        explicit.push(&raw).unwrap();
        assert_eq!(
            discovery.plan(&service()),
            Err(TeletextPsiError::Incomplete)
        );
    }
    let final_cat = one(1, 1, &section(1, 0xffff, 0, 1, 1, &[]));
    discovery.push(&final_cat).unwrap();
    explicit.push(&final_cat).unwrap();
    assert_eq!(
        discovery.plan(&service()),
        Err(TeletextPsiError::AmbiguousProgram)
    );
    let plan = explicit.plan(&service()).unwrap();
    assert_eq!((plan.program_number(), plan.pmt_pid()), (41, 3456));
}

#[test]
fn empty_and_network_only_pat_are_missing_programs_without_guessing_one() {
    for entries in [vec![], vec![(0, 16)]] {
        let mut discovery = sole();
        discovery.push(&one(0, 0, &pat(0, 0, 0, &entries))).unwrap();
        assert_eq!(discovery.plan(&service()), Err(TeletextPsiError::Program));
    }
}

#[test]
fn candidate_ownership_in_pat_pmt_cat_and_observed_packets_still_fences_sole_policy() {
    let collision = TeletextPsiError::Multiplex(TeletextMultiplexError::PidCollision { pid: 512 });
    for raw in [
        one(0, 0, &pat(0, 0, 0, &[(37, 512)])),
        one(0, 0, &pat(0, 0, 0, &[(0, 512), (37, 3337)])),
        packet(512, false, 0, &[0; 184]),
    ] {
        let mut discovery = sole();
        assert_eq!(discovery.push(&raw), Err(collision.clone()));
        assert_eq!(discovery.plan(&service()), Err(collision.clone()));
    }
    for (pcr, elementary, descriptors) in [
        (512, 301, vec![]),
        (300, 512, vec![]),
        (300, 301, vec![9, 4, 0, 1, 0xe2, 0]),
    ] {
        let mut discovery = sole();
        discovery
            .push(&one(0, 0, &pat(0, 0, 0, &[(37, 3337)])))
            .unwrap();
        assert_eq!(
            discovery.push(&one(3337, 0, &pmt(37, pcr, elementary, &descriptors))),
            Err(collision.clone())
        );
        assert_eq!(discovery.plan(&service()), Err(collision.clone()));
    }
    let mut discovery = ready();
    assert_eq!(
        discovery.push(&one(
            1,
            0,
            &section(1, 0xffff, 0, 0, 0, &[9, 4, 0, 1, 0xe2, 0])
        )),
        Err(collision.clone())
    );
    assert_eq!(discovery.plan(&service()), Err(collision));
}

#[test]
fn sole_epoch_keeps_initial_discontinuity_and_split_cat_rules_after_sealing() {
    let mut discovery = sole();
    discovery
        .push(&flagged(one(0, 0, &pat(0, 0, 0, &[(37, 3337)]))))
        .unwrap();
    discovery
        .push(&flagged(one(3337, 0, &pmt(37, 300, 301, &[]))))
        .unwrap();
    let cat = section(1, 0xffff, 0, 0, 0, &[]);
    discovery.push(&flagged(one(1, 0, &cat[..4]))).unwrap();
    assert_eq!(
        discovery.plan(&service()),
        Err(TeletextPsiError::Incomplete)
    );
    discovery.push(&packet(1, false, 1, &cat[4..])).unwrap();
    let mut monitor = discovery.into_monitor(&service()).unwrap();
    assert_eq!(
        monitor.inspect_window(&[flagged(one(3337, 1, &pmt(37, 300, 301, &[])))]),
        Err(TeletextPsiError::Continuity)
    );
    assert_eq!(monitor.finish(), Err(TeletextPsiError::Continuity));
    let mut discovery = ready();
    assert_eq!(
        discovery.push(&flagged(one(0, 1, &pat(0, 0, 0, &[(37, 3337)])))),
        Err(TeletextPsiError::Continuity)
    );
    assert_eq!(
        discovery.plan(&service()),
        Err(TeletextPsiError::Continuity)
    );
}

#[test]
fn explicit_constructors_keep_zero_rejection_and_identical_complete_plans() {
    assert!(matches!(
        TeletextPsiDiscovery::new(0, 512),
        Err(TeletextPsiError::Program)
    ));
    assert!(matches!(
        TeletextPsiDiscovery::new_source_epoch(0, 512),
        Err(TeletextPsiError::Program)
    ));
    assert!(matches!(
        TeletextPsiDiscovery::new_source_epoch_selection(
            TeletextProgramSelection::Explicit(0),
            512
        ),
        Err(TeletextPsiError::Program)
    ));
    for pid in [0, 1, 8191, 8192] {
        assert!(TeletextPsiDiscovery::new_source_epoch_selection(
            TeletextProgramSelection::SoleProgram,
            pid
        )
        .is_err());
    }
    let mut ordinary = TeletextPsiDiscovery::new(37, 512).unwrap();
    let mut epoch = TeletextPsiDiscovery::new_source_epoch(37, 512).unwrap();
    let mut selected = TeletextPsiDiscovery::new_source_epoch_selection(
        TeletextProgramSelection::Explicit(37),
        512,
    )
    .unwrap();
    for raw in [
        one(0, 0, &pat(0, 0, 0, &[(37, 3337)])),
        one(3337, 0, &pmt(37, 300, 301, &[])),
    ] {
        ordinary.push(&raw).unwrap();
        epoch.push(&raw).unwrap();
        selected.push(&raw).unwrap();
    }
    assert_eq!(
        ordinary.plan(&service()).unwrap(),
        epoch.plan(&service()).unwrap()
    );
    assert_eq!(
        epoch.plan(&service()).unwrap(),
        selected.plan(&service()).unwrap()
    );
}

#[test]
fn future_pmts_and_observed_future_cat_do_not_satisfy_current_ownership_readiness() {
    let mut discovery = sole();
    discovery
        .push(&one(0, 0, &pat(0, 0, 0, &[(37, 3337)])))
        .unwrap();
    let table = pmt(37, 300, 301, &[]);
    let future_pmt = revise(table.clone(), |data| data[5] &= 0xfe);
    discovery.push(&one(3337, 0, &future_pmt)).unwrap();
    assert_eq!(
        discovery.plan(&service()),
        Err(TeletextPsiError::Incomplete)
    );
    discovery.push(&one(3337, 1, &table[..5])).unwrap();
    assert_eq!(
        discovery.plan(&service()),
        Err(TeletextPsiError::Incomplete)
    );
    discovery
        .push(&packet(3337, false, 2, &table[5..]))
        .unwrap();
    let cat = section(1, 0xffff, 0, 0, 0, &[]);
    let future_cat = revise(cat.clone(), |data| data[5] &= 0xfe);
    discovery.push(&one(1, 0, &future_cat)).unwrap();
    assert_eq!(
        discovery.plan(&service()),
        Err(TeletextPsiError::Incomplete)
    );
    discovery.push(&one(1, 1, &cat[..4])).unwrap();
    assert_eq!(
        discovery.plan(&service()),
        Err(TeletextPsiError::Incomplete)
    );
    discovery.push(&packet(1, false, 2, &cat[4..])).unwrap();
    let original = discovery.plan(&service()).unwrap();
    discovery.push(&one(1, 3, &future_cat)).unwrap();
    assert_eq!(discovery.plan(&service()).unwrap(), original);
}
