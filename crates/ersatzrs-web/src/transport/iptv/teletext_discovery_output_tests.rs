use super::super::{
    retained_stream_buffer::TERMINAL_SUCCESS,
    teletext_optional_output::{
        forward_bound_retained_teletext_output, forward_bound_teletext_output,
    },
    teletext_output::tests::{authored_set, fixture, flatten, policy as gate_policy},
};
use super::*;
use std::time::Duration;

fn repair_crc(packet: &mut [u8; 188], extent: usize) {
    let mut crc = u32::MAX;
    for &byte in &packet[5..5 + extent - 4] {
        crc ^= u32::from(byte) << 24;
        for _ in 0..8 {
            crc = if crc & 0x8000_0000 == 0 {
                crc << 1
            } else {
                (crc << 1) ^ 0x04c1_1db7
            };
        }
    }
    packet[5 + extent - 4..5 + extent].copy_from_slice(&crc.to_be_bytes());
}

fn actual_source(frames: u64) -> Vec<u8> {
    let mut packets = fixture(frames);
    packets[0][14] = 37;
    repair_crc(&mut packets[0], 16);
    packets[1][9] = 37;
    packets[1][13..15].copy_from_slice(&[0xe3, 0x85]);
    repair_crc(&mut packets[1], 21);
    for packet in &mut packets {
        if packet[1..3] == [1, 44] {
            packet[1..3].copy_from_slice(&[3, 0x85]);
        }
    }
    flatten(&packets)
}

fn binding(retained: Option<ReservePolicy>, discovered: bool) -> TeletextOutputBinding {
    let mut config = gate_policy();
    config.program = if discovered { 999 } else { 37 };
    config.clock.pcr_pid = if discovered { 123 } else { 901 };
    if let Some(policy) = retained {
        config.max_buffer_packets = policy.capacity_bytes().div_ceil(188);
        config.max_hold = Duration::from_secs(9);
    }
    let binding =
        TeletextOutputBinding::new(config, authored_set(), Duration::from_millis(100)).unwrap();
    let binding = if let Some(policy) = retained {
        binding
            .with_optional_startup(policy.capacity_bytes())
            .unwrap()
    } else {
        binding
    };
    if discovered {
        binding
            .with_source_discovery(1024 * 188, Duration::from_secs(1))
            .unwrap()
    } else {
        binding
    }
}

#[tokio::test(start_paused = true)]
async fn direct_final_dispatch_discovers_actual_nonfixture_ids_and_matches_strict_output() {
    let original = actual_source(30);
    let mut expected = Vec::new();
    forward_bound_teletext_output(
        &mut std::io::Cursor::new(original.clone()),
        &mut expected,
        &binding(None, false),
    )
    .await
    .unwrap();
    let mut source = std::io::Cursor::new(original.clone());
    let mut actual = Vec::new();
    let count = forward_bound_teletext_output(&mut source, &mut actual, &binding(None, true))
        .await
        .unwrap();
    assert_eq!(actual, expected);
    assert_ne!(actual, original);
    assert_eq!(count, u64::try_from(original.len()).unwrap());
    assert_eq!(source.position(), count);
    for (before, after) in original.chunks_exact(188).zip(actual.chunks_exact(188)) {
        if before[1..3] == [3, 0x85] {
            assert_eq!(before, after);
        }
    }
}

#[tokio::test(start_paused = true)]
async fn retained_final_dispatch_discovers_actual_ids_and_keeps_same_reserve_output() {
    let policy = ReservePolicy::new(5_000_000, Some(Duration::from_secs(1))).unwrap();
    let original = actual_source(50);
    let terminal = AtomicU8::new(TERMINAL_SUCCESS);
    let cancel = Notify::new();
    let mut expected = Vec::new();
    forward_bound_retained_teletext_output(
        &mut std::io::Cursor::new(original.clone()),
        &mut expected,
        policy,
        &terminal,
        Instant::now() + Duration::from_secs(2),
        &cancel,
        &binding(Some(policy), false),
    )
    .await
    .unwrap();
    let mut source = std::io::Cursor::new(original.clone());
    let mut actual = Vec::new();
    forward_bound_retained_teletext_output(
        &mut source,
        &mut actual,
        policy,
        &terminal,
        Instant::now() + Duration::from_secs(2),
        &cancel,
        &binding(Some(policy), true),
    )
    .await
    .unwrap();
    assert_eq!(actual, expected);
    assert_ne!(actual, original);
    assert_eq!(source.position(), u64::try_from(original.len()).unwrap());
}

#[tokio::test(start_paused = true)]
async fn late_probe_collision_partial_cap_and_actual_clock_rejection_preserve_raw_playback() {
    let original = actual_source(30);
    for mode in 0..3 {
        let mut bytes = original.clone();
        let mut config = binding(None, false).config();
        let maximum = if mode == 1 { 191 } else { 1024 * 188 };
        if mode == 0 {
            bytes[20 * 188 + 1..20 * 188 + 3].copy_from_slice(&[2, 0]);
        }
        if mode == 2 {
            config.clock.muxrate_bps = 10_000_000;
        }
        let selected =
            TeletextOutputBinding::new(config, authored_set(), Duration::from_millis(100))
                .unwrap()
                .with_source_discovery(maximum, Duration::from_secs(1))
                .unwrap();
        let mut output = Vec::new();
        let mut source = std::io::Cursor::new(bytes.clone());
        let count = forward_bound_teletext_output(&mut source, &mut output, &selected)
            .await
            .unwrap();
        assert_eq!(output, bytes, "mode {mode}");
        assert_eq!(count, u64::try_from(bytes.len()).unwrap());
        assert_eq!(source.position(), count);
    }
}

#[tokio::test(start_paused = true)]
async fn seeded_setup_rejection_retains_prefix_and_continuation_for_both_outputs() {
    let original = actual_source(30);
    let now = Instant::now();
    let mut output = Vec::new();
    let mut source = std::io::Cursor::new(original[191..].to_vec());
    let prefix = TeletextStartupPrefix::new(original[..191].to_vec(), now, now).unwrap();
    let result = forward_optional_seeded_teletext_output(
        &mut source,
        &mut output,
        &binding(None, false),
        prefix,
        1,
    )
    .await
    .unwrap();
    assert!(result.rejection.is_some());
    assert_eq!(output, original);
    let policy = ReservePolicy::new(5_000_000, Some(Duration::from_secs(1))).unwrap();
    let original = actual_source(50);
    let mut output = Vec::new();
    let mut source = std::io::Cursor::new(original[191..].to_vec());
    let prefix =
        TeletextStartupPrefix::new(original[..191].to_vec(), Instant::now(), Instant::now())
            .unwrap();
    let selected = binding(Some(policy), false);
    let result = forward_optional_seeded_retained_output(
        &mut source,
        &mut output,
        policy,
        &AtomicU8::new(TERMINAL_SUCCESS),
        Instant::now() + Duration::from_secs(2),
        &Notify::new(),
        SeededOptionalTeletextStartup {
            binding: &selected,
            maximum_bytes: 1,
            prefix,
        },
    )
    .await
    .unwrap();
    assert!(result.rejection.is_some());
    assert_eq!(output, original);
}

#[test]
fn discovery_bounds_reject_before_changing_binding_and_prefix_errors_keep_owned_bytes() {
    assert!(binding(None, false)
        .with_optional_startup(188)
        .unwrap()
        .with_source_discovery(0, Duration::from_secs(1))
        .is_err());
    let selected = binding(None, false)
        .with_optional_startup(1880)
        .unwrap()
        .with_source_discovery(188, Duration::from_secs(1))
        .unwrap();
    assert_eq!(selected.optional_startup_bytes(), Some(1880));
    assert!(selected.with_optional_startup(187).is_err());
    for (maximum, duration) in [
        (0, Duration::from_secs(1)),
        (32768 * 188 + 1, Duration::from_secs(1)),
        (188, Duration::ZERO),
    ] {
        assert!(binding(None, false)
            .with_source_discovery(maximum, duration)
            .is_err());
    }
    let bytes = vec![0x47, 0x81];
    let now = Instant::now();
    match TeletextStartupPrefix::new_preserving(bytes.clone(), now, now + Duration::from_secs(1)) {
        Err((TeletextPumpError::Configuration, returned)) => assert_eq!(returned, bytes),
        _ => panic!("future source timestamp must retain the rejected owned bytes"),
    }
}

#[test]
fn retained_discovery_budget_charges_distinct_journal_and_probe_capacities() {
    let policy = ReservePolicy::new(12_500_000, None).unwrap();
    let mut config = gate_policy();
    config.clock.muxrate_bps = 12_500_000;
    config.max_buffer_packets = policy.capacity_bytes().div_ceil(188);
    config.max_hold = Duration::from_secs(9);
    let selected = TeletextOutputBinding::new(config, authored_set(), Duration::from_secs(1))
        .unwrap()
        .with_optional_startup(policy.capacity_bytes())
        .unwrap()
        .with_source_discovery(1024 * 188, Duration::from_secs(1))
        .unwrap();
    assert!(retained_discovery_budget(policy, &selected, policy.capacity_bytes()).is_ok());
    let (base, _) = allocation_model(selected.config()).unwrap();
    assert!(
        base + 2 * policy.capacity_bytes() + 256 * 188 > 32 * 1024 * 1024,
        "the previous doubled-journal assumption rejects this valid budget"
    );
}
