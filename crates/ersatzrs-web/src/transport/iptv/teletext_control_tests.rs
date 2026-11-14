use super::super::authored_tests::{actual_units, authored_service_with_counter};
use super::*;
use crate::transport::iptv::{
    retained_stream_buffer::{ReservePolicy, TERMINAL_SUCCESS},
    teletext_binding::{TeletextOutputBinding, TeletextOutputContent},
    teletext_output::{
        forward_teletext_output_with_control,
        tests::{fixture, flatten, policy},
    },
    teletext_retained_output::forward_retained_ts_with_teletext,
};
use std::{
    io,
    pin::Pin,
    sync::{
        atomic::{AtomicBool, AtomicU8, Ordering},
        Mutex,
    },
    task::{Context, Poll, Waker},
};
use tokio::{io::AsyncWrite, sync::Notify, time::Instant};

fn observed_control(
    mut control: TeletextOutputControl,
) -> (
    TeletextOutputControl,
    Arc<ersatzrs_app::ApplicationServices>,
) {
    let state = crate::host::state::AppState::for_tests(
        crate::host::state::IptvAccessTokenVerifier::from_optional_secret(None),
    );
    control.observe_native_output(ObservationContext {
        application: state.application.clone(),
        channel_id: 44,
        initial: ersatzrs_domain::TeletextOutputSelectionObservation {
            sequence: 0,
            generation: None,
            authored: true,
        },
    });
    (control, state.application)
}

fn observed_identity() -> ersatzrs_domain::TeletextAcceptedGeneration {
    ersatzrs_domain::TeletextAcceptedGeneration {
        id: 7,
        request: ersatzrs_domain::TeletextPublicationRequest {
            id: 6,
            channel_id: 44,
            design_id: 5,
            revision: 3,
            digest: "a".repeat(64),
        },
        artifact_version:
            ersatzrs_core::repositories::TELETEXT_EFFECTIVE_LIFECYCLE_GENERATION_ARTIFACT_VERSION,
        artifact_digest: "b".repeat(64),
    }
}

#[tokio::test(start_paused = true)]
async fn native_observations_acknowledge_only_delivered_cycles_and_remove_only_their_owner() {
    use ersatzrs_domain::TeletextOutputObservationPhase as Phase;
    let (updates, control, _) = teletext_output_control_channel();
    let (control, application) = observed_control(control);
    let mut first = PumpControl::new(control.clone());
    let second = PumpControl::new(control);
    let snapshots = || application.teletext_output_observations(44).unwrap();
    assert_eq!(snapshots().outputs.len(), 2);
    assert!(snapshots()
        .outputs
        .iter()
        .all(|output| output.applied.is_none()));
    let original = source_packets();
    let mut gate = test_gate(&original);
    let mut now = Duration::ZERO;
    let end = gate.window_end(8).unwrap().unwrap();
    let (receipt, _) = deliver_prefix(&mut gate, end, 0, &mut now);
    first.acknowledge(&mut gate, &receipt, now, true).unwrap();
    assert!(receipt.contains_complete_cycle());
    let first_state = snapshots()
        .outputs
        .into_iter()
        .find(|output| output.epoch == first.trace_epoch)
        .unwrap();
    assert_eq!(first_state.phase, Phase::Applied);
    assert_eq!(first_state.applied.unwrap().sequence, 0);
    let identity = observed_identity();
    updates.send_replace(Some(
        TeletextOutputUpdate::replace(1, Arc::new(authored_service_with_counter(4)))
            .unwrap()
            .with_generation(&identity),
    ));
    first.observe();
    let state = snapshots()
        .outputs
        .into_iter()
        .find(|output| output.epoch == first.trace_epoch)
        .unwrap();
    assert_eq!(state.phase, Phase::Waiting);
    assert_eq!(state.applied.unwrap().sequence, 0);
    assert_eq!(state.requested.sequence, 1);
    // The next acknowledged old cycle installs the replacement; it is not its delivery.
    let next_end = first.window_end(&gate, 8).unwrap().unwrap();
    let (receipt, _) = deliver_prefix(&mut gate, next_end, end, &mut now);
    first.acknowledge(&mut gate, &receipt, now, true).unwrap();
    assert_eq!(
        snapshots()
            .outputs
            .iter()
            .find(|output| output.epoch == first.trace_epoch)
            .unwrap()
            .phase,
        Phase::Waiting
    );
    let final_end = first.window_end(&gate, 8).unwrap().unwrap();
    let (receipt, _) = deliver_prefix(&mut gate, final_end, next_end, &mut now);
    first.acknowledge(&mut gate, &receipt, now, true).unwrap();
    let states = snapshots();
    let state = states
        .outputs
        .iter()
        .find(|output| output.epoch == first.trace_epoch)
        .unwrap();
    assert_eq!(state.phase, Phase::Applied);
    assert_eq!(state.applied.as_ref().unwrap().sequence, 1);
    let generation = state.applied.as_ref().unwrap().generation.as_ref().unwrap();
    assert_eq!(
        (
            generation.generation_id,
            generation.request_id,
            generation.design_id,
            generation.revision,
            generation.effective
        ),
        (7, 6, 5, 3, true)
    );
    assert!(states
        .outputs
        .iter()
        .find(|output| output.epoch == second.trace_epoch)
        .unwrap()
        .applied
        .is_none());
    drop(first);
    assert_eq!(snapshots().outputs.len(), 1);
    drop(second);
    assert!(snapshots().outputs.is_empty());
    assert!(application
        .teletext_output_observations(45)
        .unwrap()
        .outputs
        .is_empty());
}

struct FlushBarrier {
    bytes: Mutex<Vec<u8>>,
    marker: [u8; 46],
    permitted: AtomicBool,
    reached: Notify,
    waker: Mutex<Option<Waker>>,
}
impl FlushBarrier {
    fn new(service: &EncodedTeletextAuthoredService) -> Arc<Self> {
        let marker = service
            .pages()
            .iter()
            .find(|page| page.resource_header().is_some())
            .unwrap()
            .data_units()[0];
        Arc::new(Self {
            bytes: Mutex::new(Vec::new()),
            marker,
            permitted: AtomicBool::new(false),
            reached: Notify::new(),
            waker: Mutex::new(None),
        })
    }
    fn release(&self) {
        self.permitted.store(true, Ordering::SeqCst);
        if let Some(waker) = self.waker.lock().unwrap().take() {
            waker.wake();
        }
    }
    fn new_cycle_complete(&self) -> bool {
        let units = actual_units(&self.bytes.lock().unwrap());
        units
            .iter()
            .position(|unit| *unit == self.marker)
            .is_some_and(|index| {
                // Literal magazine-2 row-zero address followed by both page-FF
                // Hamming nibbles, in actual EN 300 472 reversed-bit wire order.
                units[index + 1..]
                    .iter()
                    .any(|unit| unit[4..8] == [0x92, 0xa8, 0x57, 0x57])
            })
    }
}
struct Writer(Arc<FlushBarrier>);
impl AsyncWrite for Writer {
    fn poll_write(
        self: Pin<&mut Self>,
        _: &mut Context<'_>,
        bytes: &[u8],
    ) -> Poll<io::Result<usize>> {
        self.0.bytes.lock().unwrap().extend_from_slice(bytes);
        Poll::Ready(Ok(bytes.len()))
    }
    fn poll_flush(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<io::Result<()>> {
        if !self.0.permitted.load(Ordering::SeqCst) && self.0.new_cycle_complete() {
            *self.0.waker.lock().unwrap() = Some(cx.waker().clone());
            self.0.reached.notify_one();
            return Poll::Pending;
        }
        Poll::Ready(Ok(()))
    }
    fn poll_shutdown(self: Pin<&mut Self>, _: &mut Context<'_>) -> Poll<io::Result<()>> {
        Poll::Ready(Ok(()))
    }
}

fn source_packets() -> Vec<[u8; 188]> {
    let mut packets = fixture(50);
    let template = packets[1];
    let mut cc = 1_u8;
    for index in (100..packets.len()).step_by(100) {
        assert_eq!(&packets[index][..3], &[0x47, 0x1f, 0xff]);
        let mut packet = template;
        packet[3] = 0x10 | cc;
        packets[index] = packet;
        cc = (cc + 1) & 0x0f;
    }
    packets
}
fn assert_av_and_extent(bytes: &[u8], original: &[[u8; 188]]) {
    assert_eq!(bytes.len(), original.len() * 188);
    for (before, after) in original.iter().zip(bytes.chunks_exact(188)) {
        let pid = u16::from_be_bytes([before[1] & 0x1f, before[2]]);
        if pid != 0x1fff && pid != 3337 {
            assert_eq!(before.as_slice(), after);
        }
    }
}

fn last_pmt_pids(bytes: &[u8]) -> Vec<u16> {
    let packet = bytes
        .chunks_exact(188)
        .rev()
        .find(|packet| u16::from_be_bytes([packet[1] & 0x1f, packet[2]]) == 3337)
        .unwrap();
    assert_ne!(packet[1] & 0x40, 0);
    assert_eq!(packet[4], 0);
    let section = &packet[5..];
    assert_eq!(section[0], 2);
    let end = 3 + usize::from(u16::from_be_bytes([section[1] & 0x0f, section[2]])) - 4;
    let mut offset = 12 + usize::from(u16::from_be_bytes([section[10] & 0x0f, section[11]]));
    let mut pids = Vec::new();
    while offset < end {
        pids.push(u16::from_be_bytes([
            section[offset + 1] & 0x1f,
            section[offset + 2],
        ]));
        offset += 5 + usize::from(u16::from_be_bytes([
            section[offset + 3] & 0x0f,
            section[offset + 4],
        ]));
    }
    assert_eq!(offset, end);
    pids
}

fn test_gate(packets: &[[u8; 188]]) -> TeletextOutputGate {
    let mut config = policy();
    config.max_buffer_packets = 32_768;
    config.clock.max_window_packets = 32_768;
    config.max_hold = Duration::from_secs(10);
    let mut gate =
        TeletextOutputGate::new_authored_shared(config, Arc::new(authored_service_with_counter(3)))
            .unwrap();
    gate.push(&flatten(packets), Duration::ZERO).unwrap();
    gate
}

fn deliver_prefix(
    gate: &mut TeletextOutputGate,
    end: u64,
    emitted: u64,
    now: &mut Duration,
) -> (TeletextOutputAcknowledgement, Vec<u8>) {
    assert!(gate
        .prepare_window(usize::try_from(end - emitted).unwrap(), *now)
        .unwrap());
    let schedule = gate.pending_schedule().unwrap();
    assert_eq!(schedule.first_packet(), emitted);
    let bytes = flatten(gate.pending_packets(schedule.first_due()).unwrap());
    *now = schedule.last_due();
    let receipt = gate
        .acknowledge_window_with_receipt(schedule, *now)
        .unwrap();
    (receipt, bytes)
}

#[tokio::test(start_paused = true)]
async fn display_refresh_uses_native_cache_checks_and_ack_without_erasing_unchanged_pages() {
    use ersatzrs_ffmpeg::teletext::source_header::{
        observe_dvb_teletext_header, TeletextHeaderObservation,
    };
    for refresh in [false, true] {
        let original = source_packets();
        let mut gate = test_gate(&original);
        let (updates, control, mut status) = teletext_output_control_channel();
        let mut pump = PumpControl::new(control);
        let mut now = Duration::ZERO;
        let first_end = gate.window_end(8).unwrap().unwrap();
        let (first, _) = deliver_prefix(&mut gate, first_end, 0, &mut now);
        assert!(first.contains_complete_cycle());
        pump.acknowledge(&mut gate, &first, now, true).unwrap();
        updates.send_replace(Some(
            TeletextOutputUpdate::replace(1, Arc::new(authored_service_with_counter(3)))
                .unwrap()
                .with_display_refresh(refresh),
        ));
        pump.observe();
        let second_end = pump.window_end(&gate, 8).unwrap().unwrap();
        let (second, _) = deliver_prefix(&mut gate, second_end, first_end, &mut now);
        pump.acknowledge(&mut gate, &second, now, true).unwrap();
        assert_eq!(
            status.borrow_and_update().phase(),
            TeletextOutputTransitionPhase::Waiting
        );
        let final_end = pump.window_end(&gate, 8).unwrap().unwrap();
        let (receipt, bytes) = deliver_prefix(&mut gate, final_end, second_end, &mut now);
        let mut displays = 0;
        for unit in actual_units(&bytes) {
            if let TeletextHeaderObservation::Header(header) =
                observe_dvb_teletext_header(&unit).unwrap()
            {
                if header.display_page().is_some() {
                    displays += 1;
                    assert_eq!(header.controls.erase_page(), !refresh);
                }
            }
        }
        assert!(displays > 0);
        for (before, after) in original
            [usize::try_from(second_end).unwrap()..usize::try_from(final_end).unwrap()]
            .iter()
            .zip(bytes.chunks_exact(188))
        {
            let pid = u16::from_be_bytes([before[1] & 0x1f, before[2]]);
            if pid != 0x1fff && pid != 3337 {
                assert_eq!(before.as_slice(), after);
            }
        }
        pump.acknowledge(&mut gate, &receipt, now, true).unwrap();
        assert_eq!(
            status.borrow_and_update().phase(),
            TeletextOutputTransitionPhase::Applied
        );
    }
}

fn fragment_packet(template: &[u8; 188], cc: u8, first: bool, part: &[u8]) -> [u8; 188] {
    let mut packet = [0xff; 188];
    packet[..4].copy_from_slice(&template[..4]);
    packet[1] = (packet[1] & 0x1f) | if first { 0x40 } else { 0 };
    packet[3] = 0x30 | cc;
    let payload = part.len() + usize::from(first);
    packet[4] = u8::try_from(183 - payload).unwrap();
    packet[5] = 0;
    let start = 5 + usize::from(packet[4]);
    if first {
        packet[start] = 0;
    }
    packet[start + usize::from(first)..].copy_from_slice(part);
    packet
}

fn split_at_boundary(mut packets: Vec<[u8; 188]>, boundary: usize) -> Vec<[u8; 188]> {
    let first = (2..boundary)
        .rev()
        .find(|&index| packets[index][1] == 0x1f && packets[index][2] == 0xff)
        .unwrap();
    let second = (boundary..packets.len())
        .find(|&index| packets[index][1] == 0x1f && packets[index][2] == 0xff)
        .unwrap();
    let template = packets[1];
    let size = 3 + usize::from(u16::from_be_bytes([template[6] & 0x0f, template[7]]));
    let section = &template[5..5 + size];
    let prior = packets[..first]
        .iter()
        .rev()
        .find(|packet| u16::from_be_bytes([packet[1] & 0x1f, packet[2]]) == 3337)
        .unwrap()[3]
        & 0x0f;
    let first_cc = (prior + 1) & 0x0f;
    packets[first] = fragment_packet(&template, first_cc, true, &section[..8]);
    packets[second] = fragment_packet(&template, (first_cc + 1) & 0x0f, false, &section[8..]);
    let mut cc = (first_cc + 2) & 0x0f;
    for packet in &mut packets[second + 1..] {
        if u16::from_be_bytes([packet[1] & 0x1f, packet[2]]) == 3337 {
            packet[3] = 0x10 | cc;
            cc = (cc + 1) & 0x0f;
        }
    }
    packets
}

#[test]
fn split_pmt_at_capped_ff_falls_back_to_growth_then_later_ack_without_dropping_positions() {
    let base = source_packets();
    let mut probe = test_gate(&base);
    let first = probe.window_end(1).unwrap().unwrap();
    let mut now = Duration::ZERO;
    deliver_prefix(&mut probe, first, 0, &mut now);
    let boundary = probe.transition_window_end(8).unwrap().unwrap();
    assert!(boundary < probe.window_end(8).unwrap().unwrap());
    let original = split_at_boundary(base, usize::try_from(boundary).unwrap());
    let mut gate = test_gate(&original);
    let first = gate.window_end(1).unwrap().unwrap();
    now = Duration::ZERO;
    let (_, mut bytes) = deliver_prefix(&mut gate, first, 0, &mut now);
    let next = Arc::new(authored_service_with_counter(4));
    let (sender, control, status) = teletext_output_control_channel();
    sender.send_replace(Some(
        TeletextOutputUpdate::replace(1, Arc::clone(&next)).unwrap(),
    ));
    let mut control = PumpControl::new(control);
    assert_eq!(control.trace_active_sequence, 0);
    let cap = control.window_end(&gate, 8).unwrap().unwrap();
    assert_eq!(cap, boundary);
    assert_eq!(control.trace_active_sequence, 0);
    assert!(!gate
        .prepare_window(usize::try_from(cap - first).unwrap(), now)
        .unwrap());
    assert!(control.preparation_failed(&gate, 8, cap).unwrap());
    let normal = control.window_end(&gate, 8).unwrap().unwrap();
    assert!(normal > cap);
    let (receipt, output) = deliver_prefix(&mut gate, normal, first, &mut now);
    bytes.extend(output);
    assert!(!receipt.is_cycle_boundary());
    control.acknowledge(&mut gate, &receipt, now, true).unwrap();
    assert!(control.desired.is_some());
    assert_eq!(control.trace_active_sequence, 0);
    let end = control.window_end(&gate, 8).unwrap().unwrap();
    let (receipt, output) = deliver_prefix(&mut gate, end, normal, &mut now);
    bytes.extend(output);
    assert!(receipt.is_cycle_boundary());
    assert_eq!(receipt.end_packet(), end);
    control.acknowledge(&mut gate, &receipt, now, true).unwrap();
    assert_eq!(
        status.borrow().phase(),
        TeletextOutputTransitionPhase::Waiting
    );
    assert_eq!(control.trace_active_sequence, 1);
    let final_end = control.window_end(&gate, 8).unwrap().unwrap();
    let (receipt, output) = deliver_prefix(&mut gate, final_end, end, &mut now);
    bytes.extend(output);
    control.acknowledge(&mut gate, &receipt, now, true).unwrap();
    assert_eq!(
        status.borrow().phase(),
        TeletextOutputTransitionPhase::Applied
    );
    assert_eq!(receipt.end_packet(), final_end);
    assert_eq!(control.trace_active_sequence, 1);
    sender.send_replace(Some(TeletextOutputUpdate::withdraw(2).unwrap()));
    control.observe();
    assert_eq!(control.desired.as_ref().unwrap().sequence(), 2);
    assert_eq!(control.trace_active_sequence, 1);
    assert_av_and_extent(&bytes, &original[..usize::try_from(final_end).unwrap()]);
}

#[test]
fn initial_latest_update_and_epoch_scoped_status_are_not_lost_or_aggregate_authority() {
    let (sender, control, status) = teletext_output_control_channel();
    assert!(TeletextOutputUpdate::withdraw(0).is_err());
    sender.send_replace(Some(TeletextOutputUpdate::withdraw(2).unwrap()));
    let shared_trace = control.trace_id();
    assert_eq!(control.clone().trace_id(), shared_trace);
    let mut first = PumpControl::new(control.clone());
    first.observe();
    assert_eq!(status.borrow().sequence(), Some(2));
    assert_eq!(
        status.borrow().phase(),
        TeletextOutputTransitionPhase::Waiting
    );
    let first_epoch = status.borrow().clone();
    let mut second = PumpControl::new(control);
    assert_eq!(first.control.trace_id(), second.control.trace_id());
    assert_ne!(first.trace_epoch, second.trace_epoch);
    second.observe();
    assert!(!status.borrow().same_epoch(&first_epoch));
    sender.send_replace(Some(TeletextOutputUpdate::withdraw(1).unwrap()));
    first.observe();
    assert_eq!(first.desired.as_ref().unwrap().sequence(), 2);
    assert_eq!(
        status.borrow().phase(),
        TeletextOutputTransitionPhase::Rejected(TeletextOutputTransitionRejection::Sequence)
    );
    assert!(status.borrow().same_epoch(&first_epoch));
    assert!(!format!("{:?}", status.borrow()).contains("service"));
}

#[test]
fn closing_sender_retains_unseen_final_update_without_repeated_sequence_rejections() {
    let (sender, control, status) = teletext_output_control_channel();
    let mut pump = PumpControl::new(control);
    pump.observe();
    sender.send_replace(Some(TeletextOutputUpdate::withdraw(1).unwrap()));
    pump.observe();
    sender.send_replace(Some(TeletextOutputUpdate::withdraw(2).unwrap()));
    drop(sender);
    pump.observe();
    assert_eq!(pump.desired.as_ref().unwrap().sequence(), 2);
    assert_eq!(status.borrow().sequence(), Some(2));
    assert_eq!(
        status.borrow().phase(),
        TeletextOutputTransitionPhase::Waiting
    );
    pump.observe();
    assert_eq!(
        status.borrow().phase(),
        TeletextOutputTransitionPhase::Waiting
    );
}

#[tokio::test(start_paused = true)]
async fn direct_replacement_is_waiting_until_new_units_flush_and_complete_cycle_ack() {
    let old = Arc::new(authored_service_with_counter(3));
    let next = Arc::new(authored_service_with_counter(4));
    let (sender, control, status) = teletext_output_control_channel();
    sender.send_replace(Some(
        TeletextOutputUpdate::replace(1, Arc::clone(&next)).unwrap(),
    ));
    let barrier = FlushBarrier::new(&next);
    let mut writer = Writer(Arc::clone(&barrier));
    let original = source_packets();
    let mut source = std::io::Cursor::new(flatten(&original));
    let (count, ()) = tokio::join!(
        forward_teletext_output_with_control(
            &mut source,
            &mut writer,
            policy(),
            TeletextOutputContent::Authored(old),
            Duration::from_millis(40),
            Some(control)
        ),
        async {
            barrier.reached.notified().await;
            assert_eq!(status.borrow().sequence(), Some(1));
            assert_eq!(
                status.borrow().phase(),
                TeletextOutputTransitionPhase::Waiting
            );
            barrier.release();
        },
    );
    assert_eq!(count.unwrap(), u64::try_from(original.len() * 188).unwrap());
    assert_eq!(
        status.borrow().phase(),
        TeletextOutputTransitionPhase::Applied
    );
    assert_eq!(status.borrow().sequence(), Some(1));
    let bytes = barrier.bytes.lock().unwrap();
    assert_av_and_extent(&bytes, &original);
    assert_eq!(last_pmt_pids(&bytes), [300, 512]);
    for page in next
        .pages()
        .iter()
        .filter(|page| page.resource_header().is_some())
    {
        assert!(actual_units(&bytes).contains(&page.data_units()[0]));
    }
}

#[tokio::test(start_paused = true)]
async fn retained_update_during_blocked_new_flush_waits_for_its_cycle_then_withdrawal_pmt_ack() {
    let old = Arc::new(authored_service_with_counter(3));
    let next = Arc::new(authored_service_with_counter(4));
    let (sender, control, status) = teletext_output_control_channel();
    sender.send_replace(Some(
        TeletextOutputUpdate::replace(1, Arc::clone(&next)).unwrap(),
    ));
    let reserve = ReservePolicy::new(5_000_000, Some(Duration::from_secs(1))).unwrap();
    let mut config = policy();
    config.max_buffer_packets = reserve.capacity_bytes().div_ceil(188);
    config.max_hold = Duration::from_secs(9);
    let binding =
        TeletextOutputBinding::new_authored_shared(config, old, Duration::from_millis(40))
            .unwrap()
            .with_control(control);
    let barrier = FlushBarrier::new(&next);
    let mut writer = Writer(Arc::clone(&barrier));
    let original = source_packets();
    let mut source = std::io::Cursor::new(flatten(&original));
    let terminal = AtomicU8::new(TERMINAL_SUCCESS);
    let cancel = Notify::new();
    {
        let pump = forward_retained_ts_with_teletext(
            &mut source,
            &mut writer,
            reserve,
            &terminal,
            Instant::now() + Duration::from_secs(1),
            &cancel,
            &binding,
        );
        tokio::pin!(pump);
        tokio::select! {
            result = &mut pump => panic!("pump ended before the new cycle flush: {result:?}"),
            result = tokio::time::timeout(Duration::from_secs(3), barrier.reached.notified()) => {
                result.expect("new cycle flush must reach its bounded barrier");
            }
        }
        assert_eq!(
            status.borrow().phase(),
            TeletextOutputTransitionPhase::Waiting
        );
        sender.send_replace(Some(TeletextOutputUpdate::withdraw(2).unwrap()));
        assert_eq!(status.borrow().sequence(), Some(1));
        barrier.release();
        pump.await.unwrap();
    }
    assert_eq!(status.borrow().sequence(), Some(2));
    assert_eq!(
        status.borrow().phase(),
        TeletextOutputTransitionPhase::Applied
    );
    let bytes = barrier.bytes.lock().unwrap();
    assert_av_and_extent(&bytes, &original);
    assert_eq!(last_pmt_pids(&bytes), [300]);
}

#[tokio::test(start_paused = true)]
async fn rejected_resource_counter_proposal_keeps_old_complete_output_and_is_nonterminal() {
    let old = Arc::new(authored_service_with_counter(3));
    let stale = Arc::new(authored_service_with_counter(3));
    let (sender, control, status) = teletext_output_control_channel();
    sender.send_replace(Some(TeletextOutputUpdate::replace(1, stale).unwrap()));
    let original = source_packets();
    let mut source = std::io::Cursor::new(flatten(&original));
    let mut bytes = Vec::new();
    forward_teletext_output_with_control(
        &mut source,
        &mut bytes,
        policy(),
        TeletextOutputContent::Authored(Arc::clone(&old)),
        Duration::from_millis(40),
        Some(control),
    )
    .await
    .unwrap();
    assert_eq!(
        status.borrow().phase(),
        TeletextOutputTransitionPhase::Rejected(TeletextOutputTransitionRejection::ResourceUpdate)
    );
    assert_av_and_extent(&bytes, &original);
    for page in old
        .pages()
        .iter()
        .filter(|page| page.resource_header().is_some())
    {
        assert!(actual_units(&bytes).contains(&page.data_units()[0]));
    }
}

#[tokio::test(start_paused = true)]
async fn cancelling_blocked_flush_never_reports_replacement_applied_or_resumes_output() {
    let old = Arc::new(authored_service_with_counter(3));
    let next = Arc::new(authored_service_with_counter(4));
    let (sender, control, status) = teletext_output_control_channel();
    let (control, application) = observed_control(control);
    sender.send_replace(Some(
        TeletextOutputUpdate::replace(1, Arc::clone(&next)).unwrap(),
    ));
    let barrier = FlushBarrier::new(&next);
    let mut writer = Writer(Arc::clone(&barrier));
    let mut source = std::io::Cursor::new(flatten(&source_packets()));
    {
        let pump = forward_teletext_output_with_control(
            &mut source,
            &mut writer,
            policy(),
            TeletextOutputContent::Authored(old),
            Duration::from_millis(40),
            Some(control),
        );
        tokio::pin!(pump);
        tokio::select! {
            result = &mut pump => panic!("flush barrier must hold output: {result:?}"),
            () = barrier.reached.notified() => {}
        }
        assert_eq!(
            status.borrow().phase(),
            TeletextOutputTransitionPhase::Waiting
        );
        let observed = application.teletext_output_observations(44).unwrap();
        assert_eq!(observed.outputs.len(), 1);
        assert_eq!(
            observed.outputs[0].phase,
            ersatzrs_domain::TeletextOutputObservationPhase::Waiting
        );
        assert!(observed.outputs[0]
            .applied
            .as_ref()
            .is_none_or(|applied| applied.sequence == 0));
    }
    let bytes = barrier.bytes.lock().unwrap().len();
    assert!(application
        .teletext_output_observations(44)
        .unwrap()
        .outputs
        .is_empty());
    barrier.release();
    tokio::task::yield_now().await;
    assert_eq!(barrier.bytes.lock().unwrap().len(), bytes);
    assert_eq!(
        status.borrow().phase(),
        TeletextOutputTransitionPhase::Waiting
    );
}

#[test]
fn rejected_pmt_section_capacity_keeps_old_gate_delivering_complete_cycles() {
    let original = source_packets();
    let mut gate = test_gate(&original);
    let (sender, control, status) = teletext_output_control_channel();
    let update =
        TeletextOutputUpdate::replace(1, Arc::new(authored_service_with_counter(4))).unwrap();
    sender.send_replace(Some(update));
    let mut control = PumpControl::new(control);
    control.observe();
    let update = control.desired.take().unwrap();
    control
        .finish_proposal(
            update,
            Err(TeletextOutputError::Pmt(
                TeletextPmtInstallError::Multiplex(TeletextMultiplexError::SectionCapacity),
            )),
            false,
        )
        .unwrap();
    assert_eq!(
        status.borrow().phase(),
        TeletextOutputTransitionPhase::Rejected(TeletextOutputTransitionRejection::Proposal,)
    );
    assert!(control.desired.is_none());
    assert!(control.installed.is_none());
    let end = gate.transition_window_end(8).unwrap().unwrap();
    let mut now = Duration::ZERO;
    let (receipt, bytes) = deliver_prefix(&mut gate, end, 0, &mut now);
    assert!(receipt.contains_complete_cycle());
    assert_av_and_extent(&bytes, &original[..usize::try_from(end).unwrap()]);
    let old = authored_service_with_counter(3);
    let units = actual_units(&bytes);
    for page in old
        .pages()
        .iter()
        .filter(|page| page.resource_header().is_some())
    {
        assert!(units.contains(&page.data_units()[0]));
    }
    assert_eq!(last_pmt_pids(&bytes), [300, 512]);
}

#[test]
fn withdrawal_requeues_only_transient_safe_boundary_errors() {
    for error in [
        TeletextOutputError::Boundary,
        TeletextOutputError::Pending,
        TeletextOutputError::Psi(TeletextPsiError::Incomplete),
        TeletextOutputError::Pmt(TeletextPmtInstallError::Incomplete),
    ] {
        let (sender, control, status) = teletext_output_control_channel();
        sender.send_replace(Some(TeletextOutputUpdate::withdraw(1).unwrap()));
        let mut control = PumpControl::new(control);
        control.observe();
        let update = control.desired.take().unwrap();
        control.finish_proposal(update, Err(error), false).unwrap();
        assert_eq!(control.desired.as_ref().unwrap().sequence(), 1);
        assert!(control.installed.is_none());
        assert_eq!(
            status.borrow().phase(),
            TeletextOutputTransitionPhase::Waiting
        );
    }
}

#[test]
fn failed_withdrawal_is_terminal_instead_of_rejecting_and_continuing_old_output() {
    for error in [
        TeletextOutputError::Configuration,
        TeletextOutputError::Transport(TeletextTransportError::Pid),
        TeletextOutputError::CacheReconciliationRequired,
        TeletextOutputError::Pmt(TeletextPmtInstallError::Reconfiguration),
        TeletextOutputError::Pmt(TeletextPmtInstallError::Multiplex(
            TeletextMultiplexError::SectionCapacity,
        )),
        TeletextOutputError::Closed,
    ] {
        let (sender, control, status) = teletext_output_control_channel();
        sender.send_replace(Some(TeletextOutputUpdate::withdraw(1).unwrap()));
        let mut control = PumpControl::new(control);
        control.observe();
        let update = control.desired.take().unwrap();
        assert_eq!(
            control.finish_proposal(update, Err(error.clone()), false),
            Err(error)
        );
        assert!(control.desired.is_none());
        assert!(control.installed.is_none());
        // There is no recoverable rejection claiming ordinary forwarding may
        // continue: the terminal error is returned to the owning pump.
        assert_eq!(
            status.borrow().phase(),
            TeletextOutputTransitionPhase::Waiting
        );
    }
}

#[path = "teletext_control_tests/expiry.rs"]
mod expiry;
