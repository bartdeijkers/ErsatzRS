use super::{SourceTeletextClockError, SourceTeletextClockObserver, SourceTeletextClockSample};

const PID: u16 = 256;
const WRAP: u64 = (1_u64 << 33) * 300;

fn payload(pid: u16) -> [u8; 188] {
    let mut raw = [0xff; 188];
    raw[..4].copy_from_slice(&[
        0x47,
        u8::try_from(pid >> 8).unwrap(),
        u8::try_from(pid & 0xff).unwrap(),
        0x10,
    ]);
    raw
}

// Independent H.222.0 wire fixture writer: 33-bit base, six reserved bits and
// 9-bit extension. It does not call the dependency's ClockRef encoder.
fn pcr(pid: u16, ticks: u64) -> [u8; 188] {
    assert!(ticks < WRAP);
    let base = ticks / 300;
    let extension = ticks % 300;
    let mut raw = payload(pid);
    raw[3..6].copy_from_slice(&[0x20, 183, 0x10]);
    raw[6] = u8::try_from(base >> 25).unwrap();
    raw[7] = u8::try_from((base >> 17) & 255).unwrap();
    raw[8] = u8::try_from((base >> 9) & 255).unwrap();
    raw[9] = u8::try_from((base >> 1) & 255).unwrap();
    raw[10] = u8::try_from(((base & 1) << 7) | 0x7e | (extension >> 8)).unwrap();
    raw[11] = u8::try_from(extension & 255).unwrap();
    raw
}

fn flagged(pid: u16, with_pcr: bool) -> [u8; 188] {
    let mut raw = pcr(pid, 90_000);
    raw[5] = if with_pcr { 0x90 } else { 0x80 };
    raw
}

fn assert_fenced(observer: &mut SourceTeletextClockObserver, error: SourceTeletextClockError) {
    assert_eq!(observer.report(), Err(error));
    assert_eq!(observer.push(&pcr(PID, 99)), Err(error));
    assert_eq!(observer.push(&payload(8191)), Err(error));
}

#[test]
fn variable_packet_spacing_preserves_actual_positions_and_trailing_packets() {
    let mut observer = SourceTeletextClockObserver::new(PID, 53_200).unwrap();
    let initial = observer.report().unwrap();
    assert_eq!(initial.packets_seen, 0);
    assert_eq!(initial.first, None);
    assert_eq!(initial.last, None);
    assert_eq!(initial.packets_since_last_pcr(), None);
    observer.push(&payload(8191)).unwrap();
    let first = observer.push(&pcr(PID, 18_962_100)).unwrap();
    assert_eq!(
        first.first,
        Some(SourceTeletextClockSample {
            source_packet: 53_201,
            raw_ticks: 18_962_100,
            elapsed_ticks: 0,
        })
    );
    observer.push(&payload(257)).unwrap();
    observer.push(&pcr(300, 999)).unwrap();
    observer.push(&pcr(PID, 18_962_101)).unwrap();
    observer.push(&pcr(PID, 45_962_101)).unwrap();
    observer.push(&payload(8191)).unwrap();
    observer.push(&payload(258)).unwrap();
    let report = observer.report().unwrap();
    assert_eq!(report.first_source_packet, 53_200);
    assert_eq!(report.packets_seen, 8);
    assert_eq!(report.first, first.first);
    assert_eq!(report.pcr_samples, 3);
    assert_eq!(report.wraps, 0);
    assert_eq!(report.packets_since_last_pcr(), Some(2));
    assert_eq!(
        report.last,
        Some(SourceTeletextClockSample {
            source_packet: 53_205,
            raw_ticks: 45_962_101,
            elapsed_ticks: 27_000_001,
        })
    );
}

#[test]
fn absent_and_other_program_pcr_never_invent_a_selected_clock() {
    let mut observer = SourceTeletextClockObserver::new(PID, 0).unwrap();
    for raw in [
        payload(PID),
        payload(8191),
        pcr(257, 100),
        flagged(258, true),
    ] {
        let report = observer.push(&raw).unwrap();
        assert_eq!(report.first, None);
        assert_eq!(report.last, None);
        assert_eq!(report.pcr_samples, 0);
    }
    assert_eq!(observer.report().unwrap().packets_seen, 4);
}

#[test]
fn continuing_observation_exceeds_decoder_window_without_resetting_clock() {
    let mut observer = SourceTeletextClockObserver::new(PID, 0).unwrap();
    observer.push(&pcr(PID, 1_000)).unwrap();
    for _ in 0..70_000 {
        observer.push(&payload(8191)).unwrap();
    }
    let report = observer.push(&pcr(PID, 1_001)).unwrap();
    assert_eq!(report.packets_seen, 70_002);
    assert_eq!(report.first.unwrap().source_packet, 0);
    assert_eq!(report.last.unwrap().source_packet, 70_001);
    assert_eq!(report.last.unwrap().elapsed_ticks, 1);
    // This deliberately imposes no PCR gap or muxrate admission policy.
    assert_eq!(report.pcr_samples, 2);
}

#[test]
fn modulo_wrap_and_extension_carry_preserve_unwrapped_elapsed_ticks() {
    let mut observer = SourceTeletextClockObserver::new(PID, 0).unwrap();
    for ticks in [WRAP - 10, 5, 299, 300] {
        observer.push(&pcr(PID, ticks)).unwrap();
    }
    let report = observer.report().unwrap();
    assert_eq!(report.first.unwrap().raw_ticks, WRAP - 10);
    assert_eq!(report.last.unwrap().raw_ticks, 300);
    assert_eq!(report.last.unwrap().elapsed_ticks, 310);
    assert_eq!(report.wraps, 1);
    assert_eq!(report.pcr_samples, 4);
}

#[test]
fn repeated_backward_and_half_wrap_samples_fence_all_later_reports() {
    for (first, next) in [(100, 100), (100, 99), (0, WRAP / 2), (0, WRAP - 1)] {
        let mut observer = SourceTeletextClockObserver::new(PID, 0).unwrap();
        observer.push(&pcr(PID, first)).unwrap();
        assert_eq!(
            observer.push(&pcr(PID, next)),
            Err(SourceTeletextClockError::ClockOrder)
        );
        assert_fenced(&mut observer, SourceTeletextClockError::ClockOrder);
    }
}

#[test]
fn selected_discontinuity_fences_even_without_a_pcr() {
    for with_pcr in [false, true] {
        let mut observer = SourceTeletextClockObserver::new(PID, 0).unwrap();
        assert_eq!(
            observer.push(&flagged(PID, with_pcr)),
            Err(SourceTeletextClockError::Discontinuity)
        );
        assert_fenced(&mut observer, SourceTeletextClockError::Discontinuity);
    }
}

#[test]
fn source_epoch_allowance_applies_only_to_first_selected_packet_with_or_without_pcr() {
    for with_pcr in [false, true] {
        let mut observer = SourceTeletextClockObserver::new_source_epoch(PID, 50).unwrap();
        observer.push(&flagged(300, true)).unwrap();
        let report = observer.push(&flagged(PID, with_pcr)).unwrap();
        assert_eq!(report.pcr_samples, u64::from(with_pcr));
        let report = observer.push(&pcr(PID, 90_001)).unwrap();
        assert_eq!(report.pcr_samples, 1 + u64::from(with_pcr));
        assert_eq!(
            observer.push(&flagged(PID, false)),
            Err(SourceTeletextClockError::Discontinuity)
        );
        assert_fenced(&mut observer, SourceTeletextClockError::Discontinuity);
    }
}

#[test]
fn first_unflagged_selected_packet_consumes_source_epoch_allowance() {
    for first in [payload(PID), pcr(PID, 1)] {
        let mut observer = SourceTeletextClockObserver::new_source_epoch(PID, 0).unwrap();
        observer.push(&first).unwrap();
        assert_eq!(
            observer.push(&flagged(PID, true)),
            Err(SourceTeletextClockError::Discontinuity)
        );
        assert_fenced(&mut observer, SourceTeletextClockError::Discontinuity);
    }
}

#[test]
fn selected_pcr_reserved_extension_and_truncated_fields_are_checked() {
    let mut reserved = pcr(PID, 0);
    reserved[10] &= !0x02;
    let mut extension = pcr(PID, 0);
    extension[10] |= 1;
    extension[11] = 44; // extension = 300, forbidden despite valid base bits.
    let mut truncated = pcr(PID, 0);
    truncated[3] = 0x30;
    truncated[4] = 6; // Valid adaptation extent, one PCR octet missing.
    for raw in [reserved, extension, truncated] {
        let mut observer = SourceTeletextClockObserver::new(PID, 0).unwrap();
        assert_eq!(observer.push(&raw), Err(SourceTeletextClockError::Pcr));
        assert_fenced(&mut observer, SourceTeletextClockError::Pcr);
    }
}

#[test]
fn invalid_framing_is_rejected_on_every_pid() {
    let mut sync = payload(8191);
    sync[0] = 0;
    let mut transport_error = payload(300);
    transport_error[1] |= 0x80;
    let mut scrambled = payload(300);
    scrambled[3] |= 0x80;
    let mut reserved_control = payload(300);
    reserved_control[3] = 0;
    let mut extent = pcr(300, 0);
    extent[4] = 184;
    for raw in [sync, transport_error, scrambled, reserved_control, extent] {
        let mut observer = SourceTeletextClockObserver::new(PID, 0).unwrap();
        assert_eq!(observer.push(&raw), Err(SourceTeletextClockError::Framing));
        assert_fenced(&mut observer, SourceTeletextClockError::Framing);
    }
}

#[test]
fn selected_pid_and_absolute_exclusive_end_are_bounded() {
    for pid in [0, 1, 31, 8191, u16::MAX] {
        assert_eq!(
            SourceTeletextClockObserver::new(pid, 0).unwrap_err(),
            SourceTeletextClockError::Pid
        );
        assert_eq!(
            SourceTeletextClockObserver::new_source_epoch(pid, 0).unwrap_err(),
            SourceTeletextClockError::Pid
        );
    }
    for pid in [32, 8190] {
        SourceTeletextClockObserver::new(pid, 0).unwrap();
    }
    let mut observer = SourceTeletextClockObserver::new(PID, u64::MAX - 1).unwrap();
    let report = observer.push(&pcr(PID, 100)).unwrap();
    assert_eq!(report.first.unwrap().source_packet, u64::MAX - 1);
    assert_eq!(report.packets_since_last_pcr(), Some(0));
    assert_eq!(
        observer.push(&payload(8191)),
        Err(SourceTeletextClockError::Arithmetic)
    );
    assert_fenced(&mut observer, SourceTeletextClockError::Arithmetic);
}

#[test]
fn elapsed_and_sample_counter_overflows_are_sticky() {
    let mut observer = SourceTeletextClockObserver::new(PID, 0).unwrap();
    observer.push(&pcr(PID, 10)).unwrap();
    observer.report.last.as_mut().unwrap().elapsed_ticks = u64::MAX;
    assert_eq!(
        observer.push(&pcr(PID, 11)),
        Err(SourceTeletextClockError::Arithmetic)
    );
    assert_fenced(&mut observer, SourceTeletextClockError::Arithmetic);

    let mut observer = SourceTeletextClockObserver::new(PID, 0).unwrap();
    observer.report.pcr_samples = u64::MAX;
    assert_eq!(
        observer.push(&pcr(PID, 10)),
        Err(SourceTeletextClockError::Arithmetic)
    );
    assert_fenced(&mut observer, SourceTeletextClockError::Arithmetic);
}
