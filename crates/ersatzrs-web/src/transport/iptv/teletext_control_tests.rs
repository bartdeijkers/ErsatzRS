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
    let cap = control.window_end(&gate, 8).unwrap().unwrap();
    assert_eq!(cap, boundary);
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
    let end = control.window_end(&gate, 8).unwrap().unwrap();
    let (receipt, output) = deliver_prefix(&mut gate, end, normal, &mut now);
    bytes.extend(output);
    assert!(receipt.is_cycle_boundary());
    control.acknowledge(&mut gate, &receipt, now, true).unwrap();
    assert_eq!(
        status.borrow().phase(),
        TeletextOutputTransitionPhase::Waiting
    );
    let final_end = control.window_end(&gate, 8).unwrap().unwrap();
    let (receipt, output) = deliver_prefix(&mut gate, final_end, end, &mut now);
    bytes.extend(output);
    control.acknowledge(&mut gate, &receipt, now, true).unwrap();
    assert_eq!(
        status.borrow().phase(),
        TeletextOutputTransitionPhase::Applied
    );
    assert_av_and_extent(&bytes, &original[..usize::try_from(final_end).unwrap()]);
}

#[test]
fn initial_latest_update_and_epoch_scoped_status_are_not_lost_or_aggregate_authority() {
    let (sender, control, status) = teletext_output_control_channel();
    assert!(TeletextOutputUpdate::withdraw(0).is_err());
    sender.send_replace(Some(TeletextOutputUpdate::withdraw(2).unwrap()));
    let mut first = PumpControl::new(control.clone());
    first.observe();
    assert_eq!(status.borrow().sequence(), Some(2));
    assert_eq!(
        status.borrow().phase(),
        TeletextOutputTransitionPhase::Waiting
    );
    let first_epoch = status.borrow().clone();
    let mut second = PumpControl::new(control);
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
    }
    let bytes = barrier.bytes.lock().unwrap().len();
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
