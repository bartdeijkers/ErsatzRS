//! Numeric owner-policy checks, not native generation or source acceptance.

use ersatzrs_ffmpeg::{
    mpegts_muxrate, mpegts_output, mpegts_pcr_period, pipe_output,
    teletext::TeletextServiceDescriptor, CommandGenerator, FfmpegCommand, InputFile,
    MpegTsMuxingOptions,
};

use super::*;

fn limits() -> TeletextSourcePolicyLimits {
    TeletextSourcePolicyLimits {
        elementary_pid: 512,
        rate_tolerance_ppm: 17,
        pcr_jitter_ticks: 9,
        pcr_gap_slack_ticks: 270_000,
        pcr_gap_slack_packets: 3,
        min_pcr_intervals: 2,
        max_window_packets: 512,
        max_buffer_packets: 1024,
        max_hold: Duration::from_millis(600),
        playout_delay: Duration::from_millis(100),
        max_lateness: Duration::from_millis(50),
        first_byte_timeout: Duration::from_millis(200),
        probe_maximum_bytes: 16 * 188,
        probe_maximum_time: Duration::from_millis(150),
        startup_maximum_bytes: 64 * 188,
    }
}

fn command(muxrate_bps: i64, pcr_period_ms: u32) -> FfmpegCommand {
    let muxing = MpegTsMuxingOptions {
        muxrate_bps,
        pcr_period_ms,
    };
    CommandGenerator::generate_command(
        &[InputFile::video("synthetic.mkv")],
        &[
            mpegts_muxrate(muxing),
            mpegts_pcr_period(muxing),
            mpegts_output(true),
            pipe_output(),
        ],
        false,
    )
}

fn plan(elementary_pid: u16) -> TeletextPmtPlan {
    // Independently authored PMT: program 37, PCR/video PID 901, no descriptors.
    let mut pmt = vec![
        2, 0xb0, 18, 0, 37, 0xc1, 0, 0, 0xe3, 0x85, 0xf0, 0, 0x1b, 0xe3, 0x85, 0xf0, 0,
    ];
    let mut crc = u32::MAX;
    for &byte in &pmt {
        crc ^= u32::from(byte) << 24;
        for _ in 0..8 {
            crc = if crc & 0x8000_0000 == 0 {
                crc << 1
            } else {
                (crc << 1) ^ 0x04c1_1db7
            };
        }
    }
    pmt.extend_from_slice(&crc.to_be_bytes());
    let pages = super::super::teletext_output::tests::authored_set();
    TeletextPmtPlan::new(
        &pmt,
        3337,
        elementary_pid,
        &TeletextServiceDescriptor::from_page_set(&pages),
    )
    .unwrap()
}

#[test]
fn checked_owner_limits_retain_exact_scalars_without_content() {
    let original = limits();
    let policy = TeletextSourcePolicy::new(original).unwrap();
    assert_eq!(policy.limits(), original);
    let actual = command(5_000_000, 20);
    let qualified = policy
        .qualify(actual.mpegts_output_policy().unwrap())
        .unwrap();
    assert_eq!(qualified.limits(), original);
}

#[test]
fn actual_non_one_program_and_pcr_combine_with_independent_gap_math() {
    let policy = TeletextSourcePolicy::new(limits()).unwrap();
    let actual = command(5_000_000, 20);
    let qualified = policy
        .qualify(actual.mpegts_output_policy().unwrap())
        .unwrap();
    let config = qualified.output_config(&plan(512)).unwrap();
    assert_eq!(config.program, 37);
    assert_eq!(config.clock.pcr_pid, 901);
    assert_eq!(config.elementary_pid, 512);
    assert_eq!(config.clock.muxrate_bps, 5_000_000);
    // 20 ms + explicit 10 ms slack = 810000 ticks. 30ms at 5Mbps
    // contains 150000/1504 = 99.734... packets; ceil100 + owner slack3.
    assert_eq!(config.clock.max_pcr_gap_ticks, 810_000);
    assert_eq!(config.clock.max_pcr_gap_packets, 103);
    assert_eq!(config.clock.rate_tolerance_ppm, 17);
    assert_eq!(config.clock.pcr_jitter_ticks, 9);
    assert_eq!(config.clock.min_pcr_intervals, 2);
    assert_eq!(config.clock.max_window_packets, 512);
    assert!(ersatzrs_ffmpeg::teletext::TeletextClockValidator::new(config.clock).is_ok());
    assert_eq!(config.max_buffer_packets, 1024);
    assert_eq!(config.max_hold, Duration::from_millis(600));
    assert_eq!(config.playout_delay, Duration::from_millis(100));
    assert_eq!(config.max_lateness, Duration::from_millis(50));
}

#[test]
fn period_and_rate_are_from_each_actual_command() {
    let policy = TeletextSourcePolicy::new(limits()).unwrap();
    for (rate, period, ticks, packets) in [
        (9_830_400, 20, 810_000, 200),  // 294912/1504 -> ceil197 + 3.
        (1_504_000, 40, 1_350_000, 53), // Exactly 1000 packets/s *50ms +3.
        (12_500_000, 10, 540_000, 170), // 250000/1504 -> ceil167 +3.
    ] {
        let actual = command(rate, period);
        let config = policy
            .qualify(actual.mpegts_output_policy().unwrap())
            .unwrap()
            .output_config(&plan(512))
            .unwrap();
        assert_eq!(config.clock.max_pcr_gap_ticks, ticks);
        assert_eq!(config.clock.max_pcr_gap_packets, packets);
        assert_eq!(config.clock.muxrate_bps, u64::try_from(rate).unwrap());
    }
}

#[test]
fn selected_authored_pid_cannot_be_relabelled_after_discovery() {
    let actual = command(5_000_000, 20);
    let qualified = TeletextSourcePolicy::new(limits())
        .unwrap()
        .qualify(actual.mpegts_output_policy().unwrap())
        .unwrap();
    assert!(matches!(
        qualified.output_config(&plan(513)),
        Err(TeletextSourcePolicyError::ElementaryPidMismatch)
    ));
}

#[test]
fn source_independent_invalid_bounds_fail_before_any_source_exists() {
    type Mutation = fn(&mut TeletextSourcePolicyLimits);
    let cases: &[Mutation] = &[
        |l| l.elementary_pid = 31,
        |l| l.elementary_pid = 8191,
        |l| l.rate_tolerance_ppm = 1_000_000,
        |l| l.min_pcr_intervals = 0,
        |l| l.max_window_packets = 0,
        |l| l.max_window_packets = 32_769,
        |l| l.max_buffer_packets = 0,
        |l| l.max_buffer_packets = 32 * 1024 * 1024 / 188 + 1,
        |l| l.max_buffer_packets = usize::MAX,
        |l| l.max_hold = l.playout_delay,
        |l| l.playout_delay = Duration::ZERO,
        |l| l.max_lateness = l.max_hold,
        |l| l.first_byte_timeout = Duration::ZERO,
        |l| l.probe_maximum_time = Duration::ZERO,
        |l| l.probe_maximum_bytes = 0,
        |l| l.probe_maximum_bytes = 32_768 * 188 + 1,
        |l| l.startup_maximum_bytes = 0,
        |l| l.startup_maximum_bytes = 1024 * 188 + 1,
        |l| l.startup_maximum_bytes = l.probe_maximum_bytes - 1,
        |l| l.first_byte_timeout = Duration::MAX,
    ];
    for mutation in cases {
        let mut candidate = limits();
        mutation(&mut candidate);
        assert!(
            TeletextSourcePolicy::new(candidate).is_err(),
            "{candidate:?}"
        );
    }
}

#[test]
fn command_qualification_rejects_overflow_ambiguous_gaps_and_invalid_jitter() {
    let actual = command(5_000_000, 20);
    for (slack, packet_slack, jitter, error) in [
        (u64::MAX, 0, 0, TeletextSourcePolicyError::Arithmetic),
        (0, u64::MAX, 0, TeletextSourcePolicyError::Arithmetic),
        (
            1_288_490_188_800 - 540_000,
            0,
            0,
            TeletextSourcePolicyError::Configuration("max_pcr_gap_ticks"),
        ),
        (
            0,
            0,
            540_000,
            TeletextSourcePolicyError::Configuration("pcr_jitter_ticks"),
        ),
    ] {
        let mut candidate = limits();
        candidate.pcr_gap_slack_ticks = slack;
        candidate.pcr_gap_slack_packets = packet_slack;
        candidate.pcr_jitter_ticks = jitter;
        let policy = TeletextSourcePolicy::new(candidate).unwrap();
        assert_eq!(
            policy.qualify(actual.mpegts_output_policy().unwrap()),
            Err(error)
        );
    }
}

#[test]
fn rate_dependent_carousel_and_hold_bounds_are_checked_before_probe() {
    let policy = TeletextSourcePolicy::new(limits()).unwrap();
    for rate in [75_199, i64::MAX] {
        let actual = command(rate, 20);
        assert_eq!(
            policy.qualify(actual.mpegts_output_policy().unwrap()),
            Err(TeletextSourcePolicyError::Configuration(
                "carousel_interval"
            ))
        );
    }
    let mut candidate = limits();
    // At 5Mbps each packet is 300800ns. One 20ms interval has 66
    // complete packets, so exactly 100ms + 19.8528ms cannot be held.
    candidate.max_hold = Duration::from_nanos(119_852_800);
    let actual = command(5_000_000, 20);
    assert_eq!(
        TeletextSourcePolicy::new(candidate)
            .unwrap()
            .qualify(actual.mpegts_output_policy().unwrap()),
        Err(TeletextSourcePolicyError::Configuration("max_hold"))
    );
    candidate.max_hold += Duration::from_nanos(1);
    assert!(TeletextSourcePolicy::new(candidate)
        .unwrap()
        .qualify(actual.mpegts_output_policy().unwrap())
        .is_ok());
}
