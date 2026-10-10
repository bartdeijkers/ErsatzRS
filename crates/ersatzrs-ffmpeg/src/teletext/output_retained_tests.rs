use super::tests::{bytes, config, end, expose_and_ack, set, source};
use super::*;

fn retained() -> TeletextOutputGate {
    let mut gate = TeletextOutputGate::new_retained(config(), set()).unwrap();
    gate.push(&bytes(&source(3)), Duration::ZERO).unwrap();
    gate
}

#[test]
fn reserve_holding_does_not_start_the_delivery_deadline() {
    let mut gate = retained();
    assert_eq!(gate.next_deadline().unwrap(), Some(config().max_hold));
    gate.check_deadline(Duration::from_millis(400)).unwrap();
    assert_eq!(gate.retained_packets().unwrap(), end(3));
    assert!(gate.pending.is_none());
}

#[test]
fn retained_delivery_start_preserves_original_arrival_age_and_physical_positions() {
    let mut gate = retained();
    let start = Duration::from_millis(200);
    gate.start_delivery(start).unwrap();
    gate.prepare_window(end(3), start).unwrap();
    let schedule = gate.pending_schedule().unwrap();
    assert_eq!(schedule.first_packet(), 0);
    assert_eq!(schedule.first_due(), start + config().playout_delay);
    assert_eq!(gate.pending.as_ref().unwrap().first_arrival, Duration::ZERO);
    let last = schedule.last_due();
    expose_and_ack(&mut gate);
    gate.finish(last).unwrap();
}

#[test]
fn starting_delivery_cannot_rescue_expired_reserve_bytes() {
    let mut gate = retained();
    assert_eq!(
        gate.start_delivery(config().max_hold),
        Err(TeletextOutputError::Buffer(TeletextBufferError::Deadline))
    );
    assert!(gate.pending.is_none());
    assert!(gate.retained_packets().is_err());
}

#[test]
fn start_does_not_rebase_the_oldest_packet_holding_expiry() {
    let mut gate = retained();
    gate.start_delivery(Duration::from_millis(200)).unwrap();
    assert!(matches!(
        gate.check_deadline(config().max_hold),
        Err(TeletextOutputError::Buffer(TeletextBufferError::Deadline))
    ));
}

#[test]
fn a_repeated_start_fences_even_a_prepared_window() {
    for pending in [false, true] {
        let mut gate = retained();
        gate.start_delivery(Duration::ZERO).unwrap();
        if pending {
            gate.prepare_window(end(3), Duration::ZERO).unwrap();
        }
        assert_eq!(
            gate.start_delivery(Duration::ZERO),
            Err(TeletextOutputError::Configuration)
        );
        assert!(gate.pending.is_none());
        assert!(gate.retained_packets().is_err());
    }
}

#[test]
fn ordinary_output_cannot_rebase_its_delivery_clock() {
    let mut gate = TeletextOutputGate::new(config(), set()).unwrap();
    gate.push(&bytes(&source(3)), Duration::ZERO).unwrap();
    assert_eq!(
        gate.start_delivery(Duration::ZERO),
        Err(TeletextOutputError::Configuration)
    );
    assert!(gate.pending.is_none());
}

#[test]
fn unready_retained_start_and_preparation_before_start_fail_without_exposure() {
    let mut empty = TeletextOutputGate::new_retained(config(), set()).unwrap();
    assert_eq!(
        empty.start_delivery(Duration::ZERO),
        Err(TeletextOutputError::Configuration)
    );
    for final_window in [false, true] {
        let mut gate = retained();
        let result = if final_window {
            gate.prepare_final_window(Duration::ZERO)
        } else {
            gate.prepare_window(end(3), Duration::ZERO)
        };
        assert_eq!(result, Err(TeletextOutputError::DeliveryNotStarted));
        assert!(gate.pending.is_none());
    }
}

#[test]
fn retained_start_does_not_weaken_source_admission_or_clock_validation() {
    let mut gate = TeletextOutputGate::new_retained(config(), set()).unwrap();
    let mut malformed = bytes(&source(3));
    malformed[0] = 0;
    let error = gate.push(&malformed, Duration::ZERO).unwrap_err();
    assert_eq!(gate.start_delivery(Duration::ZERO), Err(error));
}

#[test]
fn retained_readiness_waits_for_complete_signalling_and_verified_clock_without_fencing() {
    let mut gate = TeletextOutputGate::new_retained(config(), set()).unwrap();
    let raw = bytes(&source(3));
    assert_eq!(gate.source_ready(), Ok(false));
    // A real read can end inside the first PMT packet. Querying readiness must
    // let the owner continue filling, rather than start or reject that epoch.
    gate.push(&raw[..250], Duration::ZERO).unwrap();
    assert_eq!(gate.source_ready(), Ok(false));
    gate.push(&raw[250..3 * 188], Duration::from_millis(1))
        .unwrap();
    assert_eq!(gate.source_ready(), Ok(false));
    gate.push(&raw[3 * 188..], Duration::from_millis(2))
        .unwrap();
    assert_eq!(gate.source_ready(), Ok(true));
    gate.start_delivery(Duration::from_millis(3)).unwrap();
    gate.prepare_window(end(3), Duration::from_millis(3))
        .unwrap();
    let last = gate.pending_schedule().unwrap().last_due();
    expose_and_ack(&mut gate);
    gate.finish(last).unwrap();
    assert!(gate.source_ready().is_err());
}
