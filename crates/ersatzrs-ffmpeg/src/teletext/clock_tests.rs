use super::{TeletextClockConfig, TeletextClockError, TeletextClockReport, TeletextClockValidator};

const WRAP: u64 = (1_u64 << 33) * 300;
const FIVE_PACKETS: u64 = 40_608;

fn policy() -> TeletextClockConfig {
    TeletextClockConfig {
        pcr_pid: 256,
        muxrate_bps: 5_000_000,
        rate_tolerance_ppm: 0,
        pcr_jitter_ticks: 0,
        max_pcr_gap_packets: 1_000,
        max_pcr_gap_ticks: 27_000_000,
        min_pcr_intervals: 2,
        max_window_packets: 128,
    }
}

fn payload(pid: u16) -> [u8; 188] {
    let mut packet = [0xff; 188];
    packet[0] = 0x47;
    packet[1] = u8::try_from(pid >> 8).unwrap();
    packet[2] = u8::try_from(pid & 0xff).unwrap();
    packet[3] = 0x10;
    packet
}

fn pcr(pid: u16, ticks: u64) -> [u8; 188] {
    let base = ticks / 300;
    let extension = u16::try_from(ticks % 300).unwrap();
    let mut packet = payload(pid);
    packet[3] = 0x20;
    packet[4] = 183;
    packet[5] = 0x10;
    packet[6] = u8::try_from(base >> 25).unwrap();
    packet[7] = u8::try_from((base >> 17) & 0xff).unwrap();
    packet[8] = u8::try_from((base >> 9) & 0xff).unwrap();
    packet[9] = u8::try_from((base >> 1) & 0xff).unwrap();
    packet[10] =
        u8::try_from((base & 1) << 7).unwrap() | 0x7e | u8::try_from(extension >> 8).unwrap();
    packet[11] = u8::try_from(extension & 0xff).unwrap();
    packet
}

fn interval(first: u64, packets: usize, delta: u64) -> Vec<[u8; 188]> {
    let mut window = vec![payload(8191); packets + 1];
    window[0] = pcr(256, first);
    window[packets] = pcr(256, (first + delta) % WRAP);
    window
}

fn assert_fenced(validator: &mut TeletextClockValidator, error: TeletextClockError) {
    assert_eq!(validator.report(), Err(error));
    assert_eq!(validator.observe(&[]), Err(error));
    assert_eq!(validator.observe(&[pcr(256, 0)]), Err(error));
}

#[test]
fn readiness_requires_intervals_and_counts_all_pids_across_windows() {
    let mut validator = TeletextClockValidator::new(policy()).unwrap();
    assert_eq!(validator.report(), Ok(TeletextClockReport::default()));
    assert_eq!(validator.observe(&[]).unwrap().packets_seen, 0);
    assert!(!validator.observe(&[pcr(256, 100)]).unwrap().ready);
    // The other program's PCR is not our clock, but does consume packet space.
    let middle = [payload(257), pcr(300, 900), payload(8191), payload(258)];
    assert!(!validator.observe(&middle).unwrap().ready);
    let first = validator.observe(&[pcr(256, 100 + FIVE_PACKETS)]).unwrap();
    assert_eq!(first.pcr_samples, 2);
    assert_eq!(first.validated_intervals, 1);
    assert!(!first.ready);
    validator.observe(&middle[..2]).unwrap();
    validator.observe(&middle[2..]).unwrap();
    let ready = validator
        .observe(&[pcr(256, 100 + 2 * FIVE_PACKETS)])
        .unwrap();
    assert_eq!(
        ready,
        TeletextClockReport {
            packets_seen: 11,
            pcr_samples: 3,
            validated_intervals: 2,
            ready: true,
        }
    );
    assert_eq!(validator.observe(&[]), Ok(ready));
}

#[test]
fn declared_rate_mismatch_and_packet_loss_are_sticky() {
    for muxrate in [4_000_000, 6_000_000] {
        let mut validator = TeletextClockValidator::new(TeletextClockConfig {
            muxrate_bps: muxrate,
            ..policy()
        })
        .unwrap();
        assert_eq!(
            validator.observe(&interval(0, 5, FIVE_PACKETS)),
            Err(TeletextClockError::Muxrate)
        );
        assert_fenced(&mut validator, TeletextClockError::Muxrate);
    }
    let mut validator = TeletextClockValidator::new(policy()).unwrap();
    let mut missing_packet = interval(0, 5, FIVE_PACKETS);
    missing_packet.remove(2);
    assert_eq!(
        validator.observe(&missing_packet),
        Err(TeletextClockError::Muxrate)
    );
    assert_fenced(&mut validator, TeletextClockError::Muxrate);
}

#[test]
fn valid_base_wrap_and_extension_carry_are_not_clock_reversal() {
    for first in [WRAP - 100, 299] {
        let mut validator = TeletextClockValidator::new(policy()).unwrap();
        let observed = validator
            .observe(&interval(first, 5, FIVE_PACKETS))
            .unwrap();
        assert_eq!(observed.pcr_samples, 2);
        assert_eq!(observed.validated_intervals, 1);
    }
}

#[test]
fn repeated_reversed_and_ambiguous_clocks_cannot_recover() {
    for (first, delta) in [(100, 0), (100, WRAP - 1), (100, WRAP / 2), (0, WRAP - 1)] {
        let mut validator = TeletextClockValidator::new(policy()).unwrap();
        assert_eq!(
            validator.observe(&interval(first, 5, delta)),
            Err(TeletextClockError::ClockOrder)
        );
        assert_fenced(&mut validator, TeletextClockError::ClockOrder);
    }
    let mut validator = TeletextClockValidator::new(policy()).unwrap();
    let repeated = pcr(256, 0);
    validator.observe(&[repeated]).unwrap();
    assert_eq!(
        validator.observe(&[repeated]),
        Err(TeletextClockError::ClockOrder)
    );
}

#[test]
fn discontinuity_is_rejected_with_and_without_selected_pcr() {
    for has_pcr in [true, false] {
        let mut validator = TeletextClockValidator::new(policy()).unwrap();
        let mut bad = pcr(256, 0);
        bad[5] = if has_pcr { 0x90 } else { 0x80 };
        assert_eq!(
            validator.observe(&[bad]),
            Err(TeletextClockError::Discontinuity)
        );
        assert_fenced(&mut validator, TeletextClockError::Discontinuity);
    }
    // Another program's clock/discontinuity never becomes the selected clock.
    let mut validator = TeletextClockValidator::new(policy()).unwrap();
    let mut other = pcr(300, 0);
    other[5] |= 0x80;
    assert_eq!(validator.observe(&[other]).unwrap().pcr_samples, 0);
}

#[test]
fn startup_and_ongoing_packet_gaps_cross_window_boundaries() {
    for startup in [true, false] {
        let mut validator = TeletextClockValidator::new(TeletextClockConfig {
            max_pcr_gap_packets: 5,
            ..policy()
        })
        .unwrap();
        if !startup {
            validator.observe(&[pcr(256, 0)]).unwrap();
        }
        validator.observe(&[payload(8191); 3]).unwrap();
        validator.observe(&[payload(8191); 2]).unwrap();
        assert_eq!(
            validator.observe(&[payload(300)]),
            Err(TeletextClockError::PcrGap)
        );
        assert_fenced(&mut validator, TeletextClockError::PcrGap);
    }
}

#[test]
fn pcr_at_packet_gap_boundary_is_accepted_and_late_pcr_is_rejected() {
    let config = TeletextClockConfig {
        max_pcr_gap_packets: 5,
        ..policy()
    };
    let mut exact = TeletextClockValidator::new(config).unwrap();
    exact.observe(&interval(0, 5, FIVE_PACKETS)).unwrap();
    let mut late = TeletextClockValidator::new(config).unwrap();
    assert_eq!(
        late.observe(&interval(0, 6, FIVE_PACKETS)),
        Err(TeletextClockError::PcrGap)
    );
}

#[test]
fn forward_clock_gap_uses_explicit_tick_bound() {
    let config = TeletextClockConfig {
        max_pcr_gap_ticks: FIVE_PACKETS,
        ..policy()
    };
    let mut exact = TeletextClockValidator::new(config).unwrap();
    exact.observe(&interval(0, 5, FIVE_PACKETS)).unwrap();
    let mut late = TeletextClockValidator::new(config).unwrap();
    assert_eq!(
        late.observe(&interval(0, 5, FIVE_PACKETS + 1)),
        Err(TeletextClockError::PcrGap)
    );
    assert_fenced(&mut late, TeletextClockError::PcrGap);
}

#[test]
fn explicit_absolute_jitter_tolerance_has_an_exact_boundary() {
    for delta in [FIVE_PACKETS - 14, FIVE_PACKETS + 14] {
        let mut allowed = TeletextClockValidator::new(TeletextClockConfig {
            pcr_jitter_ticks: 14,
            ..policy()
        })
        .unwrap();
        allowed.observe(&interval(0, 5, delta)).unwrap();
    }
    for delta in [FIVE_PACKETS - 15, FIVE_PACKETS + 15] {
        let mut rejected = TeletextClockValidator::new(TeletextClockConfig {
            pcr_jitter_ticks: 14,
            ..policy()
        })
        .unwrap();
        assert_eq!(
            rejected.observe(&interval(0, 5, delta)),
            Err(TeletextClockError::Muxrate)
        );
    }
}

#[test]
fn explicit_relative_tolerance_is_not_integer_truncated_pcr_time() {
    let config = TeletextClockConfig {
        rate_tolerance_ppm: 1_000,
        ..policy()
    };
    for delta in [FIVE_PACKETS - 40, FIVE_PACKETS + 40] {
        let mut allowed = TeletextClockValidator::new(config).unwrap();
        allowed.observe(&interval(0, 5, delta)).unwrap();
    }
    for delta in [FIVE_PACKETS - 41, FIVE_PACKETS + 41] {
        let mut rejected = TeletextClockValidator::new(config).unwrap();
        assert_eq!(
            rejected.observe(&interval(0, 5, delta)),
            Err(TeletextClockError::Muxrate)
        );
    }
}

#[test]
fn relative_tolerance_bounds_measured_bitrate_against_declared_bitrate() {
    let config = TeletextClockConfig {
        rate_tolerance_ppm: 500_000,
        ..policy()
    };
    // The same physical distance at a shorter interval increases measured
    // bitrate. Exactly +50% and -50% are admitted, using the declared rate
    // rather than the nominal interval as the relative-error denominator.
    for delta in [27_072, 81_216] {
        let mut allowed = TeletextClockValidator::new(config).unwrap();
        assert_eq!(
            allowed
                .observe(&interval(0, 5, delta))
                .unwrap()
                .validated_intervals,
            1
        );
    }
    // One tick outside each asymmetric boundary, and +100% measured bitrate.
    for delta in [27_071, 81_217, 20_304] {
        let mut rejected = TeletextClockValidator::new(config).unwrap();
        assert_eq!(
            rejected.observe(&interval(0, 5, delta)),
            Err(TeletextClockError::Muxrate)
        );
        assert_fenced(&mut rejected, TeletextClockError::Muxrate);
    }
}

#[test]
fn realistic_high_muxrate_and_large_cross_products_use_wide_arithmetic() {
    let mut gigabit = TeletextClockValidator::new(TeletextClockConfig {
        muxrate_bps: 1_000_000_000,
        ..policy()
    })
    .unwrap();
    // 125 * 188 * 8 * 27 MHz / 1 Gbit/s = exactly 5076 ticks.
    gigabit.observe(&interval(0, 125, 5_076)).unwrap();
    let mut huge = TeletextClockValidator::new(TeletextClockConfig {
        muxrate_bps: u64::MAX,
        max_pcr_gap_ticks: 1_000_000_000_000,
        ..policy()
    })
    .unwrap();
    // The product exceeds u64 but is safely classified as a rate mismatch.
    assert_eq!(
        huge.observe(&interval(0, 5, 1_000_000_000_000)),
        Err(TeletextClockError::Muxrate)
    );
}

#[test]
fn invalid_selected_pcr_extension_reserved_bits_and_truncation_are_fenced() {
    let mut extension = pcr(256, 0);
    extension[10] |= 1;
    extension[11] = 44; // 300 is representable in 9 bits but invalid in PCR.
    let mut reserved = pcr(256, 0);
    reserved[10] &= !2;
    let mut truncated = pcr(256, 0);
    truncated[3] = 0x30;
    truncated[4] = 6; // Flags plus only five of the six PCR bytes.
    for bad in [extension, reserved, truncated] {
        let mut validator = TeletextClockValidator::new(policy()).unwrap();
        assert_eq!(validator.observe(&[bad]), Err(TeletextClockError::Pcr));
        assert_fenced(&mut validator, TeletextClockError::Pcr);
    }
}

#[test]
fn malformed_framing_on_any_pid_is_fenced() {
    for pid in [256, 300, 8191] {
        for (offset, value) in [(0, 0), (1, 0x80), (3, 0), (3, 0x90), (3, 0x20)] {
            let mut validator = TeletextClockValidator::new(policy()).unwrap();
            let mut bad = payload(pid);
            bad[offset] = value;
            assert_eq!(validator.observe(&[bad]), Err(TeletextClockError::Framing));
            assert_fenced(&mut validator, TeletextClockError::Framing);
        }
        let mut validator = TeletextClockValidator::new(policy()).unwrap();
        let mut bad = pcr(pid, 0);
        bad[3] = 0x30;
        bad[4] = 183; // Adaptation + payload must leave a payload byte.
        assert_eq!(validator.observe(&[bad]), Err(TeletextClockError::Framing));
    }
}

#[test]
fn oversized_window_is_rejected_before_consumption_and_cannot_be_retried() {
    let mut validator = TeletextClockValidator::new(TeletextClockConfig {
        max_window_packets: 1,
        ..policy()
    })
    .unwrap();
    assert_eq!(
        validator.observe(&[payload(8191); 2]),
        Err(TeletextClockError::Capacity)
    );
    assert_fenced(&mut validator, TeletextClockError::Capacity);
}

#[test]
fn invalid_configuration_is_rejected_at_construction() {
    let mut cases = Vec::new();
    for pcr_pid in [0, 31, 8191, 8192] {
        cases.push((
            TeletextClockConfig {
                pcr_pid,
                ..policy()
            },
            "pcr_pid",
        ));
    }
    cases.extend([
        (
            TeletextClockConfig {
                muxrate_bps: 0,
                ..policy()
            },
            "muxrate_bps",
        ),
        (
            TeletextClockConfig {
                rate_tolerance_ppm: 1_000_000,
                ..policy()
            },
            "rate_tolerance_ppm",
        ),
        (
            TeletextClockConfig {
                max_pcr_gap_packets: 0,
                ..policy()
            },
            "max_pcr_gap_packets",
        ),
        (
            TeletextClockConfig {
                max_pcr_gap_ticks: 0,
                ..policy()
            },
            "max_pcr_gap_ticks",
        ),
        (
            TeletextClockConfig {
                max_pcr_gap_ticks: WRAP / 2,
                ..policy()
            },
            "max_pcr_gap_ticks",
        ),
        (
            TeletextClockConfig {
                pcr_jitter_ticks: 27_000_000,
                ..policy()
            },
            "pcr_jitter_ticks",
        ),
        (
            TeletextClockConfig {
                min_pcr_intervals: 0,
                ..policy()
            },
            "min_pcr_intervals",
        ),
        (
            TeletextClockConfig {
                max_window_packets: 0,
                ..policy()
            },
            "max_window_packets",
        ),
        (
            TeletextClockConfig {
                max_window_packets: 32_769,
                ..policy()
            },
            "max_window_packets",
        ),
    ]);
    for (config, field) in cases {
        assert!(matches!(
            TeletextClockValidator::new(config),
            Err(TeletextClockError::Configuration { field: invalid }) if invalid == field
        ));
    }
}

#[test]
fn bounded_streaming_windows_have_no_startup_or_lifetime_packet_ceiling() {
    let mut validator = TeletextClockValidator::new(TeletextClockConfig {
        max_window_packets: 5,
        max_pcr_gap_packets: 5,
        ..policy()
    })
    .unwrap();
    validator.observe(&[pcr(256, 0)]).unwrap();
    for sample in 1..=20_000 {
        let mut window = [payload(8191); 5];
        window[4] = pcr(256, sample * FIVE_PACKETS);
        validator.observe(&window).unwrap();
    }
    assert_eq!(
        validator.report().unwrap(),
        TeletextClockReport {
            packets_seen: 100_001,
            pcr_samples: 20_001,
            validated_intervals: 20_000,
            ready: true,
        }
    );
}
