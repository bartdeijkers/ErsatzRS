//! Expiry successor failures must not resurrect the displaced old scope.

use super::*;

struct Boundary {
    gate: TeletextOutputGate,
    original: Vec<[u8; 188]>,
    bytes: Vec<u8>,
    now: Duration,
    receipt: TeletextOutputAcknowledgement,
}
impl Boundary {
    fn new() -> Self {
        let original = source_packets();
        let mut gate = test_gate(&original);
        let mut now = Duration::ZERO;
        let mut emitted = 0;
        let mut bytes = Vec::new();
        for _ in 0..8 {
            let end = gate.transition_window_end(8).unwrap().unwrap();
            let (receipt, output) = deliver_prefix(&mut gate, end, emitted, &mut now);
            bytes.extend(output);
            emitted = end;
            if receipt.contains_complete_cycle() && receipt.is_cycle_boundary() {
                return Self {
                    gate,
                    original,
                    bytes,
                    now,
                    receipt,
                };
            }
        }
        panic!("bounded fixture must acknowledge its genuine complete old cycle");
    }
    fn forward(&mut self, control: &mut PumpControl) {
        let first = self.receipt.end_packet();
        let end = control.window_end(&self.gate, 8).unwrap().unwrap();
        let (receipt, output) = deliver_prefix(&mut self.gate, end, first, &mut self.now);
        self.bytes.extend(output);
        control
            .acknowledge(&mut self.gate, &receipt, self.now, true)
            .unwrap();
        self.receipt = receipt;
        assert_av_and_extent(&self.bytes, &self.original[..usize::try_from(end).unwrap()]);
    }
}

#[test]
fn expiry_coalesced_success_replaces_at_same_ack_without_absent_pmt() {
    let mut boundary = Boundary::new();
    let next = Arc::new(authored_service_with_counter(4));
    let (sender, control, status) = teletext_output_control_channel();
    sender.send_replace(Some(TeletextOutputUpdate::withdraw(1).unwrap()));
    sender.send_replace(Some(
        TeletextOutputUpdate::replace_after_expiry(2, Arc::clone(&next)).unwrap(),
    ));
    let mut control = PumpControl::new(control);
    control
        .acknowledge(&mut boundary.gate, &boundary.receipt, boundary.now, true)
        .unwrap();
    assert!(boundary.gate.authored_payload_selected());
    assert!(!boundary.gate.signalling_transition_pending());
    assert_eq!(status.borrow().sequence(), Some(2));
    assert_eq!(
        status.borrow().phase(),
        TeletextOutputTransitionPhase::Waiting
    );
    boundary.forward(&mut control);
    assert_eq!(
        status.borrow().phase(),
        TeletextOutputTransitionPhase::Applied
    );
    let units = actual_units(&boundary.bytes);
    for page in next
        .pages()
        .iter()
        .filter(|page| page.resource_header().is_some())
    {
        assert!(units.contains(&page.data_units()[0]));
    }
    for packet in boundary
        .bytes
        .chunks_exact(188)
        .filter(|packet| u16::from_be_bytes([packet[1] & 0x1f, packet[2]]) == 3337)
    {
        assert_eq!(last_pmt_pids(packet), [300, 512]);
    }
}

#[test]
fn expiry_rejected_resource_withdraws_same_ack_while_public_replace_retains_old() {
    for expiry in [false, true] {
        let mut boundary = Boundary::new();
        let stale = Arc::new(authored_service_with_counter(3));
        let update = if expiry {
            TeletextOutputUpdate::replace_after_expiry(1, stale).unwrap()
        } else {
            TeletextOutputUpdate::replace(1, stale).unwrap()
        };
        let (sender, control, status) = teletext_output_control_channel();
        sender.send_replace(Some(update));
        let mut control = PumpControl::new(control);
        control
            .acknowledge(&mut boundary.gate, &boundary.receipt, boundary.now, true)
            .unwrap();
        let rejected = TeletextOutputTransitionPhase::Rejected(
            TeletextOutputTransitionRejection::ResourceUpdate,
        );
        assert_eq!(status.borrow().phase(), rejected);
        assert_eq!(boundary.gate.authored_payload_selected(), !expiry);
        assert_eq!(boundary.gate.signalling_transition_pending(), expiry);
        assert!(control.installed.is_none());
        let first = boundary.bytes.len();
        boundary.forward(&mut control);
        assert_eq!(status.borrow().phase(), rejected);
        assert_eq!(actual_units(&boundary.bytes[first..]).is_empty(), expiry);
        assert_eq!(
            last_pmt_pids(&boundary.bytes),
            if expiry { vec![300] } else { vec![300, 512] }
        );
    }
}

#[test]
fn expiry_cache_and_proposal_refusals_withdraw_real_gate_without_applied_successor() {
    for (error, reason) in [
        (
            TeletextOutputError::CacheReconciliationRequired,
            TeletextOutputTransitionRejection::CacheReconciliation,
        ),
        (
            TeletextOutputError::Pmt(TeletextPmtInstallError::Multiplex(
                TeletextMultiplexError::SectionCapacity,
            )),
            TeletextOutputTransitionRejection::Proposal,
        ),
    ] {
        let mut boundary = Boundary::new();
        let (sender, control, status) = teletext_output_control_channel();
        sender.send_replace(Some(
            TeletextOutputUpdate::replace_after_expiry(
                1,
                Arc::new(authored_service_with_counter(4)),
            )
            .unwrap(),
        ));
        let mut control = PumpControl::new(control);
        control.observe();
        let update = control.desired.take().unwrap();
        // Exercise the existing classified refusal with a real, current ACK;
        // actual native ResourceUpdate rejection is covered separately above.
        let result = Err(error);
        control
            .withdraw_rejected_expiry(
                &mut boundary.gate,
                &boundary.receipt,
                boundary.now,
                &update,
                &result,
            )
            .unwrap();
        control.finish_proposal(update, result, false).unwrap();
        assert!(!boundary.gate.authored_payload_selected());
        assert!(control.installed.is_none());
        assert_eq!(
            status.borrow().phase(),
            TeletextOutputTransitionPhase::Rejected(reason)
        );
        let first = boundary.bytes.len();
        boundary.forward(&mut control);
        assert!(actual_units(&boundary.bytes[first..]).is_empty());
        assert_eq!(last_pmt_pids(&boundary.bytes), [300]);
        assert_eq!(
            status.borrow().phase(),
            TeletextOutputTransitionPhase::Rejected(reason)
        );
    }
}

#[test]
fn expiry_foreign_ack_and_failed_fallback_are_terminal_without_applied() {
    let mut boundary = Boundary::new();
    let foreign = Boundary::new();
    let (sender, control, status) = teletext_output_control_channel();
    sender.send_replace(Some(
        TeletextOutputUpdate::replace_after_expiry(1, Arc::new(authored_service_with_counter(4)))
            .unwrap(),
    ));
    let mut control = PumpControl::new(control);
    assert_eq!(
        control.acknowledge(&mut boundary.gate, &foreign.receipt, boundary.now, true,),
        Err(TeletextOutputError::Acknowledgement)
    );
    assert_eq!(
        status.borrow().phase(),
        TeletextOutputTransitionPhase::Waiting
    );
    assert!(control.installed.is_none());
    let update =
        TeletextOutputUpdate::replace_after_expiry(2, Arc::new(authored_service_with_counter(4)))
            .unwrap();
    assert_eq!(
        control.withdraw_rejected_expiry(
            &mut boundary.gate,
            &foreign.receipt,
            boundary.now,
            &update,
            &Err(TeletextOutputError::CacheReconciliationRequired),
        ),
        Err(TeletextOutputError::Acknowledgement)
    );
    assert!(boundary.gate.authored_payload_selected());
    assert_eq!(
        status.borrow().phase(),
        TeletextOutputTransitionPhase::Waiting
    );
}

#[test]
fn expiry_eof_keeps_withdrawal_and_never_reports_rejected_base_applied() {
    let mut boundary = Boundary::new();
    let (sender, control, status) = teletext_output_control_channel();
    sender.send_replace(Some(
        TeletextOutputUpdate::replace_after_expiry(1, Arc::new(authored_service_with_counter(4)))
            .unwrap(),
    ));
    let mut control = PumpControl::new(control);
    control.source_ended();
    assert!(matches!(
        control.desired.as_ref().unwrap().action,
        Action::Withdraw
    ));
    let rejected =
        TeletextOutputTransitionPhase::Rejected(TeletextOutputTransitionRejection::SourceEnded);
    assert_eq!(status.borrow().phase(), rejected);
    control
        .acknowledge(&mut boundary.gate, &boundary.receipt, boundary.now, true)
        .unwrap();
    assert!(!boundary.gate.authored_payload_selected());
    assert!(control.installed.is_none());
    let first = boundary.bytes.len();
    boundary.forward(&mut control);
    assert!(actual_units(&boundary.bytes[first..]).is_empty());
    assert_eq!(last_pmt_pids(&boundary.bytes), [300]);
    assert_eq!(status.borrow().phase(), rejected);
}

#[test]
fn expiry_transient_boundary_refusal_keeps_safe_retry_without_false_status() {
    for error in [
        TeletextOutputError::Boundary,
        TeletextOutputError::Pending,
        TeletextOutputError::Psi(TeletextPsiError::Incomplete),
        TeletextOutputError::Pmt(TeletextPmtInstallError::Incomplete),
    ] {
        let (sender, control, status) = teletext_output_control_channel();
        sender.send_replace(Some(
            TeletextOutputUpdate::replace_after_expiry(
                1,
                Arc::new(authored_service_with_counter(4)),
            )
            .unwrap(),
        ));
        let mut control = PumpControl::new(control);
        control.observe();
        let update = control.desired.take().unwrap();
        control.finish_proposal(update, Err(error), false).unwrap();
        assert!(control.desired.as_ref().unwrap().withdraw_if_rejected);
        assert!(control.installed.is_none());
        assert_eq!(
            status.borrow().phase(),
            TeletextOutputTransitionPhase::Waiting
        );
    }
    assert!(TeletextOutputUpdate::replace_after_expiry(
        0,
        Arc::new(authored_service_with_counter(4)),
    )
    .is_err());
}
