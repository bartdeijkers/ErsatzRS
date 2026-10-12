//! Source-independent preflight must precede late binding/gate allocation.

use super::super::teletext_source_policy::TeletextSourcePolicyLimits;
use super::*;
use ersatzrs_ffmpeg::{build_mpegts_concat_command, OutputMetadata};

fn limits(reserve: ReservePolicy) -> TeletextSourcePolicyLimits {
    TeletextSourcePolicyLimits {
        elementary_pid: 512,
        rate_tolerance_ppm: 0,
        pcr_jitter_ticks: 1,
        pcr_gap_slack_ticks: 540_000,
        pcr_gap_slack_packets: 1,
        min_pcr_intervals: 2,
        max_window_packets: 1024,
        max_buffer_packets: reserve.capacity_bytes().div_ceil(188),
        max_hold: Duration::from_secs(10),
        playout_delay: Duration::from_millis(100),
        max_lateness: Duration::from_millis(200),
        first_byte_timeout: Duration::from_secs(1),
        probe_maximum_bytes: 1024 * 188,
        probe_maximum_time: Duration::from_millis(900),
        startup_maximum_bytes: reserve.capacity_bytes(),
    }
}

fn qualified(limits: TeletextSourcePolicyLimits, rate: i64) -> TeletextQualifiedSourcePolicy {
    let command =
        build_mpegts_concat_command("synthetic.ffconcat", &OutputMetadata::default(), Some(rate));
    TeletextSourcePolicy::new(limits)
        .unwrap()
        .qualify(command.mpegts_output_policy().unwrap())
        .unwrap()
}

#[test]
fn retained_preflight_supports_distinct_actual_rates_without_transport_ids() {
    for rate in [5_000_000, 9_830_400, 12_500_000] {
        let reserve = ReservePolicy::new(rate, Some(Duration::from_secs(3))).unwrap();
        retained_preflight(
            reserve,
            qualified(limits(reserve), i64::try_from(rate).unwrap()),
            rate,
        )
        .unwrap();
    }
}

#[test]
fn late_factory_rejects_rate_hold_capacity_and_overlap_before_a_gate_exists() {
    let reserve = ReservePolicy::new(5_000_000, Some(Duration::from_secs(3))).unwrap();
    let normal = limits(reserve);
    for mut changed in [normal; 4].into_iter().enumerate() {
        let rate = match changed.0 {
            0 => 9_830_400,
            1 => {
                changed.1.max_hold = Duration::from_secs(8);
                5_000_000
            }
            2 => {
                changed.1.max_buffer_packets = 1024;
                changed.1.startup_maximum_bytes = 1024 * 188;
                changed.1.probe_maximum_bytes = 188;
                5_000_000
            }
            3 => {
                changed.1.max_buffer_packets = 150_000;
                changed.1.startup_maximum_bytes = 150_000 * 188;
                5_000_000
            }
            _ => unreachable!(),
        };
        assert!(
            retained_preflight(
                reserve,
                qualified(changed.1, rate),
                u64::try_from(rate).unwrap()
            )
            .is_err(),
            "case {}",
            changed.0
        );
    }
}

#[test]
fn probe_journal_overlap_is_separate_from_reduced_processing_space() {
    let rate = 13_000_000;
    let reserve = ReservePolicy::new(rate, Some(Duration::from_secs(3))).unwrap();
    let normal = limits(reserve);
    assert!(retained_preflight(
        reserve,
        qualified(normal, i64::try_from(rate).unwrap()),
        rate
    )
    .is_ok());
    let huge_probe = TeletextSourcePolicyLimits {
        probe_maximum_bytes: 32_768 * 188,
        ..normal
    };
    // The gate/journal fit alone; simultaneously retaining the maximum probe
    // consumes the additional payload that the late constructor must charge.
    assert!(retained_preflight(
        reserve,
        qualified(huge_probe, i64::try_from(rate).unwrap()),
        rate
    )
    .is_err());
    assert!(retained_seed_preflight(normal, usize::MAX).is_err());
}
