use std::time::Duration;

use super::{buffer::MAX_RETAINED_PACKETS, TeletextBufferError, TeletextPacketBuffer};

fn ms(value: u64) -> Duration {
    Duration::from_millis(value)
}

fn buffer(packets: usize) -> TeletextPacketBuffer {
    TeletextPacketBuffer::new(packets, ms(100)).unwrap()
}

fn packet(marker: u8) -> [u8; 188] {
    let mut raw = [marker; 188];
    raw[..4].copy_from_slice(&[0x47, 0x1f, 0xff, 0x10 | (marker & 0x0f)]);
    raw
}

fn append_fixture_packets(buffer: &mut TeletextPacketBuffer, mut packets: usize, now: Duration) {
    let chunk = vec![0xff; 256 * 188];
    while packets != 0 {
        let count = packets.min(256);
        buffer.push(&chunk[..count * 188], now).unwrap();
        packets -= count;
    }
}

fn assert_failed(buffer: &mut TeletextPacketBuffer, error: TeletextBufferError) {
    assert_eq!(buffer.held_bytes(), 0);
    assert_eq!(buffer.window(), Err(error));
    assert_eq!(buffer.push(&packet(9), ms(200)), Err(error));
    assert_eq!(buffer.release(0, Duration::ZERO), Err(error));
    assert_eq!(buffer.check_deadline(ms(300)), Err(error));
    assert_eq!(buffer.finish(ms(300)), Err(error));
}

#[test]
fn configuration_validates_both_bounds() {
    for capacity in [0, MAX_RETAINED_PACKETS + 1, usize::MAX] {
        assert!(matches!(
            TeletextPacketBuffer::new(capacity, ms(1)),
            Err(TeletextBufferError::Configuration {
                field: "max_buffer_packets"
            })
        ));
    }
    assert!(matches!(
        TeletextPacketBuffer::new(1, Duration::ZERO),
        Err(TeletextBufferError::Configuration { field: "max_hold" })
    ));
    assert!(TeletextPacketBuffer::new(1, Duration::from_nanos(1)).is_ok());
    assert!(TeletextPacketBuffer::new(32_768, Duration::MAX).is_ok());
    assert!(TeletextPacketBuffer::new(32_769, Duration::MAX).is_ok());
    assert!(TeletextPacketBuffer::new(MAX_RETAINED_PACKETS, Duration::MAX).is_ok());
}

#[test]
fn arbitrary_byte_split_positions_preserve_packet_order_and_partial_tail() {
    let expected = [packet(0), packet(1), packet(2), packet(3)];
    let bytes = expected.concat();
    // Every possible first read position includes header and payload splits.
    for split in 0..=bytes.len() {
        let mut buffer = buffer(4);
        buffer.push(&bytes[..split], ms(0)).unwrap();
        assert_eq!(buffer.window().unwrap(), &expected[..split / 188]);
        assert_eq!(buffer.held_bytes(), split);
        for chunk in bytes[split..].chunks(7) {
            buffer.push(chunk, ms(1)).unwrap();
        }
        assert_eq!(buffer.window().unwrap(), &expected);
        assert_eq!(buffer.release(4, ms(2)).unwrap(), expected);
        buffer.finish(ms(3)).unwrap();
    }
}

#[test]
fn buffer_groups_raw_bytes_without_claiming_transport_validation() {
    let mut buffer = buffer(1);
    buffer.push(&[0; 188], ms(0)).unwrap();
    assert_eq!(buffer.window().unwrap(), &[[0; 188]]);
    assert_eq!(buffer.release(1, ms(1)).unwrap(), vec![[0; 188]]);
}

#[test]
fn complete_packets_and_tail_share_one_byte_capacity() {
    let mut buffer = buffer(2);
    buffer.push(&packet(1), ms(0)).unwrap();
    buffer.push(&packet(2)[..187], ms(1)).unwrap();
    assert_eq!(buffer.held_bytes(), 375);
    assert_eq!(buffer.window().unwrap(), &[packet(1)]);
    buffer.push(&packet(2)[187..], ms(2)).unwrap();
    assert_eq!(buffer.held_bytes(), 376);
    assert_eq!(
        buffer.push(&[0x47], ms(3)),
        Err(TeletextBufferError::Capacity)
    );
    assert_failed(&mut buffer, TeletextBufferError::Capacity);
}

#[test]
fn partial_tail_prevents_an_oversized_append_from_releasing_any_prefix() {
    let mut buffer = buffer(2);
    buffer.push(&packet(1)[..1], ms(0)).unwrap();
    // This append alone would fit, but its byte plus the retained tail would not.
    assert_eq!(
        buffer.push(&[packet(2), packet(3)].concat(), ms(1)),
        Err(TeletextBufferError::Capacity)
    );
    assert_failed(&mut buffer, TeletextBufferError::Capacity);
}

#[test]
fn maximum_capacity_accepts_exact_limit_and_rejects_one_more_byte() {
    let mut buffer = buffer(MAX_RETAINED_PACKETS);
    append_fixture_packets(&mut buffer, MAX_RETAINED_PACKETS, ms(0));
    assert_eq!(buffer.window().unwrap().len(), MAX_RETAINED_PACKETS);
    assert_eq!(buffer.held_bytes(), MAX_RETAINED_PACKETS * 188);
    assert!(buffer.held_bytes() <= 32 * 1024 * 1024);
    assert!(buffer.held_bytes() + 188 > 32 * 1024 * 1024);
    assert_eq!(
        buffer.push(&[0xff], ms(1)),
        Err(TeletextBufferError::Capacity)
    );
    assert_failed(&mut buffer, TeletextBufferError::Capacity);
}

#[test]
fn eight_seconds_at_eight_mbit_retains_more_than_a_processing_window_including_partial_tail() {
    let transport_bytes = 8_000_000_usize;
    let packets = transport_bytes.div_ceil(188);
    assert!(packets > 32_768);
    let mut buffer = TeletextPacketBuffer::new(packets, Duration::from_secs(9)).unwrap();
    append_fixture_packets(&mut buffer, transport_bytes / 188, Duration::ZERO);
    let tail = packet(5);
    let partial = transport_bytes % 188;
    buffer.push(&tail[..partial], Duration::ZERO).unwrap();
    assert_eq!(buffer.held_bytes(), transport_bytes);
    assert_eq!(buffer.window().unwrap().len(), transport_bytes / 188);
    buffer.check_deadline(Duration::from_secs(8)).unwrap();
    // Late completion and a large release must preserve the tail's first age.
    buffer
        .push(&tail[partial..], Duration::from_secs(8))
        .unwrap();
    assert_eq!(buffer.window().unwrap().last(), Some(&tail));
    let released = buffer.release(32_768, Duration::from_secs(8)).unwrap();
    assert_eq!(released.len(), 32_768);
    assert_eq!(buffer.held_bytes(), (packets - 32_768) * 188);
    assert_eq!(buffer.window().unwrap().last(), Some(&tail));
    assert_eq!(
        buffer.check_deadline(Duration::from_secs(9)),
        Err(TeletextBufferError::Deadline)
    );
    assert_failed(&mut buffer, TeletextBufferError::Deadline);
}

#[test]
fn maximum_retention_accounts_for_partial_tail_before_copying_an_oversized_append() {
    let mut buffer = buffer(MAX_RETAINED_PACKETS);
    append_fixture_packets(&mut buffer, MAX_RETAINED_PACKETS - 1, ms(0));
    buffer.push(&packet(5)[..187], ms(1)).unwrap();
    assert_eq!(buffer.held_bytes(), MAX_RETAINED_PACKETS * 188 - 1);
    assert_eq!(buffer.window().unwrap().len(), MAX_RETAINED_PACKETS - 1);
    assert_eq!(
        buffer.push(&[0x47, 0xff], ms(2)),
        Err(TeletextBufferError::Capacity)
    );
    assert_failed(&mut buffer, TeletextBufferError::Capacity);
}

#[test]
fn large_prefix_release_preserves_the_next_arrival_instead_of_resetting_retention_age() {
    let capacity = 42_554;
    let first_group = 32_769;
    let mut buffer = TeletextPacketBuffer::new(capacity, Duration::from_secs(9)).unwrap();
    append_fixture_packets(&mut buffer, first_group, Duration::ZERO);
    append_fixture_packets(&mut buffer, capacity - first_group, Duration::from_secs(1));
    let released = buffer.release(first_group, Duration::from_secs(8)).unwrap();
    assert_eq!(released.len(), first_group);
    assert_eq!(buffer.window().unwrap().len(), capacity - first_group);
    buffer.check_deadline(ms(9_999)).unwrap();
    assert_eq!(
        buffer.check_deadline(ms(10_000)),
        Err(TeletextBufferError::Deadline)
    );
    assert_failed(&mut buffer, TeletextBufferError::Deadline);
}

#[test]
fn oversized_initial_append_is_permanent() {
    let mut buffer = buffer(1);
    assert_eq!(
        buffer.push(&[0; 189], ms(0)),
        Err(TeletextBufferError::Capacity)
    );
    assert_failed(&mut buffer, TeletextBufferError::Capacity);
}

#[test]
fn idle_deadline_expires_at_exact_boundary() {
    let mut buffer = buffer(1);
    buffer.push(&packet(1), ms(10)).unwrap();
    buffer.check_deadline(ms(109)).unwrap();
    assert_eq!(
        buffer.check_deadline(ms(110)),
        Err(TeletextBufferError::Deadline)
    );
    assert_failed(&mut buffer, TeletextBufferError::Deadline);
}

#[test]
fn idle_partial_packet_expires_from_its_first_byte() {
    let mut buffer = buffer(1);
    buffer.push(&[0x47], ms(10)).unwrap();
    buffer.push(&[0x1f], ms(109)).unwrap();
    assert_eq!(
        buffer.check_deadline(ms(110)),
        Err(TeletextBufferError::Deadline)
    );
    assert_failed(&mut buffer, TeletextBufferError::Deadline);
}

#[test]
fn completing_a_split_packet_does_not_restart_its_age() {
    let mut buffer = buffer(1);
    let raw = packet(1);
    buffer.push(&raw[..1], ms(10)).unwrap();
    buffer.push(&raw[1..], ms(109)).unwrap();
    assert_eq!(buffer.window().unwrap(), &[raw]);
    assert_eq!(
        buffer.release(1, ms(110)),
        Err(TeletextBufferError::Deadline)
    );
    assert_failed(&mut buffer, TeletextBufferError::Deadline);
}

#[test]
fn append_cannot_complete_an_already_expired_tail() {
    let mut buffer = buffer(1);
    buffer.push(&packet(1)[..187], ms(0)).unwrap();
    assert_eq!(
        buffer.push(&[0xff], ms(100)),
        Err(TeletextBufferError::Deadline)
    );
    assert_failed(&mut buffer, TeletextBufferError::Deadline);
}

#[test]
fn release_preserves_remaining_packet_arrival() {
    let mut buffer = buffer(2);
    buffer.push(&packet(1), ms(0)).unwrap();
    buffer.push(&packet(2), ms(50)).unwrap();
    assert_eq!(buffer.release(1, ms(99)).unwrap(), vec![packet(1)]);
    assert_eq!(buffer.window().unwrap(), &[packet(2)]);
    buffer.check_deadline(ms(149)).unwrap();
    assert_eq!(
        buffer.check_deadline(ms(150)),
        Err(TeletextBufferError::Deadline)
    );
    assert_failed(&mut buffer, TeletextBufferError::Deadline);
}

#[test]
fn release_preserves_partial_tail_age_and_frees_only_complete_prefix() {
    let mut buffer = buffer(2);
    buffer.push(&packet(1), ms(0)).unwrap();
    buffer.push(&packet(2)[..187], ms(50)).unwrap();
    buffer.release(1, ms(99)).unwrap();
    assert_eq!(buffer.held_bytes(), 187);
    buffer.push(&packet(2)[187..], ms(149)).unwrap();
    assert_eq!(
        buffer.release(1, ms(150)),
        Err(TeletextBufferError::Deadline)
    );
    assert_failed(&mut buffer, TeletextBufferError::Deadline);
}

#[test]
fn release_rechecks_deadline_before_moving_any_packet() {
    let mut buffer = buffer(2);
    buffer
        .push(&[packet(1), packet(2)].concat(), ms(0))
        .unwrap();
    assert_eq!(
        buffer.release(1, ms(100)),
        Err(TeletextBufferError::Deadline)
    );
    assert_failed(&mut buffer, TeletextBufferError::Deadline);
}

#[test]
fn invalid_prefix_length_permanently_drops_complete_and_partial_bytes() {
    let mut buffer = buffer(2);
    buffer.push(&packet(1), ms(0)).unwrap();
    buffer.push(&[0x47], ms(1)).unwrap();
    assert_eq!(
        buffer.release(2, ms(2)),
        Err(TeletextBufferError::ReleasePrefix)
    );
    assert_failed(&mut buffer, TeletextBufferError::ReleasePrefix);
}

#[test]
fn zero_release_keeps_bytes_and_capacity_can_be_reused_after_a_real_release() {
    let mut buffer = buffer(1);
    buffer.push(&packet(1), ms(0)).unwrap();
    assert!(buffer.release(0, ms(1)).unwrap().is_empty());
    assert_eq!(buffer.held_bytes(), 188);
    assert_eq!(buffer.release(1, ms(2)).unwrap(), vec![packet(1)]);
    buffer.push(&packet(2), ms(100)).unwrap();
    assert_eq!(buffer.release(1, ms(199)).unwrap(), vec![packet(2)]);
    buffer.finish(ms(200)).unwrap();
}

#[test]
fn empty_input_and_idle_checks_track_monotonic_time_without_creating_an_age() {
    let mut buffer = buffer(1);
    buffer.push(&[], ms(100)).unwrap();
    buffer.check_deadline(Duration::MAX).unwrap();
    buffer.push(&packet(1), Duration::MAX).unwrap();
    assert_eq!(buffer.release(1, Duration::MAX).unwrap(), vec![packet(1)]);
    buffer.finish(Duration::MAX).unwrap();
}

#[test]
fn each_timed_operation_rejects_backwards_time_and_preserves_first_error() {
    for operation in 0..4 {
        let mut buffer = buffer(2);
        buffer.push(&packet(1), ms(10)).unwrap();
        let result = match operation {
            0 => buffer.push(&[], ms(9)),
            1 => buffer.check_deadline(ms(9)),
            2 => buffer.release(0, ms(9)).map(|_| ()),
            _ => buffer.finish(ms(9)),
        };
        assert_eq!(result, Err(TeletextBufferError::TimeOrder));
        assert_failed(&mut buffer, TeletextBufferError::TimeOrder);
    }
}

#[test]
fn eof_partial_packet_drops_all_held_output() {
    let mut buffer = buffer(2);
    buffer.push(&packet(1), ms(0)).unwrap();
    buffer.push(&[0x47], ms(1)).unwrap();
    assert_eq!(
        buffer.finish(ms(2)),
        Err(TeletextBufferError::PartialPacket)
    );
    assert_failed(&mut buffer, TeletextBufferError::PartialPacket);
}

#[test]
fn eof_requires_owner_to_drain_complete_packets() {
    let mut buffer = buffer(1);
    buffer.push(&packet(1), ms(0)).unwrap();
    assert_eq!(
        buffer.finish(ms(1)),
        Err(TeletextBufferError::RetainedBytes)
    );
    assert_failed(&mut buffer, TeletextBufferError::RetainedBytes);
}

#[test]
fn eof_checks_deadline_before_reporting_undrained_bytes() {
    let mut buffer = buffer(1);
    buffer.push(&[0x47], ms(0)).unwrap();
    assert_eq!(buffer.finish(ms(100)), Err(TeletextBufferError::Deadline));
    assert_failed(&mut buffer, TeletextBufferError::Deadline);
}

#[test]
fn successful_eof_is_idempotent_and_cannot_reopen() {
    let mut buffer = buffer(1);
    buffer.push(&packet(1), ms(0)).unwrap();
    buffer.release(1, ms(1)).unwrap();
    buffer.finish(ms(2)).unwrap();
    buffer.finish(ms(3)).unwrap();
    assert!(buffer.window().unwrap().is_empty());
    assert_eq!(buffer.push(&[], ms(4)), Err(TeletextBufferError::Closed));
    assert_failed(&mut buffer, TeletextBufferError::Closed);
}

#[test]
fn closed_generation_rejects_release_and_idle_mutation() {
    for operation in 0..2 {
        let mut buffer = buffer(1);
        buffer.finish(ms(0)).unwrap();
        let result = if operation == 0 {
            buffer.release(0, ms(1)).map(|_| ())
        } else {
            buffer.check_deadline(ms(1))
        };
        assert_eq!(result, Err(TeletextBufferError::Closed));
        assert_failed(&mut buffer, TeletextBufferError::Closed);
    }
}

#[test]
fn repeated_eof_still_rejects_backwards_time() {
    let mut buffer = buffer(1);
    buffer.finish(ms(10)).unwrap();
    assert_eq!(buffer.finish(ms(9)), Err(TeletextBufferError::TimeOrder));
    assert_failed(&mut buffer, TeletextBufferError::TimeOrder);
}

#[test]
fn first_arrival_query_tracks_prefix_release_partial_tail_and_sticky_failure() {
    let mut buffer = buffer(3);
    assert_eq!(buffer.first_arrival().unwrap(), None);
    buffer.push(&packet(1), ms(10)).unwrap();
    buffer.push(&packet(2), ms(20)).unwrap();
    buffer.push(&[0x47], ms(30)).unwrap();
    assert_eq!(buffer.first_arrival().unwrap(), Some(ms(10)));
    buffer.release(1, ms(50)).unwrap();
    assert_eq!(buffer.first_arrival().unwrap(), Some(ms(20)));
    buffer.release(1, ms(60)).unwrap();
    assert_eq!(buffer.first_arrival().unwrap(), Some(ms(30)));
    assert_eq!(
        buffer.finish(ms(70)),
        Err(TeletextBufferError::PartialPacket)
    );
    assert_eq!(
        buffer.first_arrival(),
        Err(TeletextBufferError::PartialPacket)
    );
    assert_failed(&mut buffer, TeletextBufferError::PartialPacket);
}
