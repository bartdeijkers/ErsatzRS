use super::{SourceNativeEpoch, SourceNativeEpochError, SourceNativeWrapBehavior};

const PERIOD: i64 = 8_589_934_592;
const MINUTE: i64 = 5_400_000;

#[test]
fn first_demux_dts_precedes_pts_and_missing_dts_falls_back_without_inventing_origin() {
    let video = SourceNativeEpoch::from_first_demux_timestamps(Some(129_600), Some(126_000));
    assert_eq!(video.reference_90khz(), Some(-5_274_000));
    assert_eq!(video.behavior(), Some(SourceNativeWrapBehavior::AddOffset));
    for dts in [None, Some(i64::MIN)] {
        let audio = SourceNativeEpoch::from_first_demux_timestamps(Some(127_680), dts);
        assert_eq!(audio.reference_90khz(), Some(-5_272_320));
        assert_eq!(audio.apply_timestamp(127_680), Ok(Some(127_680)));
    }
    // Native container origin is1.418667, not any inferred PCR or first-video time.
    assert_eq!(video.apply_pcr_ticks(18_962_100), Ok(Some(18_962_100)));
}

#[test]
fn absent_initializers_and_missing_timestamps_retain_explicit_unknown_state() {
    for pts in [None, Some(i64::MIN)] {
        for dts in [None, Some(i64::MIN)] {
            let unknown = SourceNativeEpoch::from_first_demux_timestamps(pts, dts);
            assert_eq!(unknown, SourceNativeEpoch::unknown());
            assert_eq!(unknown.reference_90khz(), None);
            assert_eq!(unknown.behavior(), None);
            assert_eq!(unknown.apply_timestamp(0), Ok(None));
            assert_eq!(unknown.apply_pcr_ticks(0), Ok(None));
        }
    }
    let known = SourceNativeEpoch::from_first_demux_timestamps(Some(0), None);
    assert_eq!(known.apply_timestamp(i64::MIN), Ok(None));
    assert_eq!(known.apply_timestamp(0), Ok(Some(0)));
}

#[test]
fn native_branch_boundaries_use_both_original_conditions_and_exact_comparisons() {
    for first in [
        0,
        PERIOD - (PERIOD >> 3) - 1,
        PERIOD - (PERIOD >> 3),
        PERIOD - MINUTE - 1,
    ] {
        let policy = SourceNativeEpoch::from_first_demux_timestamps(None, Some(first));
        assert_eq!(policy.behavior(), Some(SourceNativeWrapBehavior::AddOffset));
        assert_eq!(policy.reference_90khz(), Some(first - MINUTE));
    }
    for first in [PERIOD - MINUTE, PERIOD - 1] {
        let policy = SourceNativeEpoch::from_first_demux_timestamps(None, Some(first));
        assert_eq!(
            policy.behavior(),
            Some(SourceNativeWrapBehavior::SubtractOffset)
        );
        assert_eq!(policy.reference_90khz(), Some(first - MINUTE));
    }
}

#[test]
fn add_policy_transforms_only_values_strictly_below_reference() {
    let policy = SourceNativeEpoch::from_first_demux_timestamps(Some(PERIOD - MINUTE - 1), None);
    let reference = PERIOD - 2 * MINUTE - 1;
    assert_eq!(policy.reference_90khz(), Some(reference));
    assert_eq!(
        policy.apply_timestamp(reference - 1),
        Ok(Some(reference - 1 + PERIOD))
    );
    assert_eq!(policy.apply_timestamp(reference), Ok(Some(reference)));
    assert_eq!(
        policy.apply_timestamp(reference + 1),
        Ok(Some(reference + 1))
    );
    assert_eq!(policy.apply_timestamp(0), Ok(Some(PERIOD)));
}

#[test]
fn subtract_policy_preserves_negative_native_values_and_includes_reference() {
    let policy = SourceNativeEpoch::from_first_demux_timestamps(None, Some(PERIOD - 1));
    let reference = PERIOD - MINUTE - 1;
    assert_eq!(
        policy.apply_timestamp(reference - 1),
        Ok(Some(reference - 1))
    );
    assert_eq!(policy.apply_timestamp(reference), Ok(Some(-MINUTE - 1)));
    assert_eq!(policy.apply_timestamp(PERIOD - 1), Ok(Some(-1)));
    assert_eq!(policy.apply_timestamp(0), Ok(Some(0)));
    assert_eq!(policy.apply_timestamp(-1), Ok(Some(-1)));
}

#[test]
fn only_initial_reference_is_masked_not_every_transformed_timestamp() {
    let raw = SourceNativeEpoch::from_first_demux_timestamps(None, Some(100));
    for first in [100, PERIOD + 100, 2 * PERIOD + 100] {
        assert_eq!(
            SourceNativeEpoch::from_first_demux_timestamps(None, Some(first)),
            raw
        );
    }
    assert_eq!(raw.apply_timestamp(PERIOD + 100), Ok(Some(PERIOD + 100)));
    let signed = SourceNativeEpoch::from_first_demux_timestamps(Some(-1), None);
    let wrapped = SourceNativeEpoch::from_first_demux_timestamps(Some(PERIOD - 1), None);
    assert_eq!(signed, wrapped);
}

#[test]
fn shared_program_first_known_membership_wins_and_propagated_copy_stays_exact() {
    let proposed = SourceNativeEpoch::from_first_demux_timestamps(None, Some(126_000));
    let first_program = SourceNativeEpoch::from_first_demux_timestamps(None, Some(PERIOD - 1));
    let second_program = SourceNativeEpoch::from_first_demux_timestamps(None, Some(127_680));
    let unknown = SourceNativeEpoch::unknown();
    assert_eq!(proposed.resolve_program_memberships(&[]), proposed);
    assert_eq!(
        proposed.resolve_program_memberships(&[unknown, unknown]),
        proposed
    );
    let chosen = proposed.resolve_program_memberships(&[unknown, first_program, second_program]);
    assert_eq!(chosen, first_program);
    assert_eq!(
        proposed.resolve_program_memberships(&[second_program, first_program]),
        second_program
    );
    // The owner, not this pure helper, propagates that one chosen value through
    // every affected membership and stream in the real source registry.
    let propagated = [chosen, chosen, chosen];
    assert!(propagated.iter().all(|state| {
        proposed.resolve_program_memberships(&[*state]) == first_program
            && state.apply_timestamp(PERIOD - 1) == Ok(Some(-1))
    }));
}

#[test]
fn no_program_default_stream_inheritance_does_not_guess_unknown_disposition() {
    let proposed = SourceNativeEpoch::from_first_demux_timestamps(None, Some(126_000));
    let default = SourceNativeEpoch::from_first_demux_timestamps(None, Some(PERIOD - 1));
    let unknown = SourceNativeEpoch::unknown();
    assert_eq!(proposed.resolve_default_stream(default), default);
    assert_eq!(proposed.resolve_default_stream(unknown), proposed);
    // n9.0 returns before consulting memberships/default when both timestamps
    // are absent, so these helpers must not initialize a timestamp-less stream.
    assert_eq!(unknown.resolve_default_stream(default), unknown);
    assert_eq!(
        unknown.resolve_program_memberships(&[proposed, default]),
        unknown
    );
}

#[test]
fn pcr_base_uses_same_candidate_rule_and_keeps_every_extension_tick() {
    let subtract = SourceNativeEpoch::from_first_demux_timestamps(None, Some(PERIOD - 1));
    let last_base = u64::try_from(PERIOD - 1).unwrap() * 300;
    assert_eq!(subtract.apply_pcr_ticks(last_base), Ok(Some(-300)));
    assert_eq!(subtract.apply_pcr_ticks(last_base + 299), Ok(Some(-1)));
    assert_eq!(subtract.apply_pcr_ticks(0), Ok(Some(0)));
    assert_eq!(subtract.apply_pcr_ticks(299), Ok(Some(299)));
    assert_eq!(subtract.apply_pcr_ticks(300), Ok(Some(300)));
    let add = SourceNativeEpoch::from_first_demux_timestamps(None, Some(PERIOD - MINUTE - 1));
    assert_eq!(add.apply_pcr_ticks(299), Ok(Some(PERIOD * 300 + 299)));
}

#[test]
fn raw_pcr_numeric_range_is_checked_even_when_epoch_is_unknown() {
    let policy = SourceNativeEpoch::from_first_demux_timestamps(Some(0), None);
    let unknown = SourceNativeEpoch::unknown();
    for raw in [u64::try_from(PERIOD).unwrap() * 300, u64::MAX] {
        assert_eq!(
            policy.apply_pcr_ticks(raw),
            Err(SourceNativeEpochError::PcrRange)
        );
        assert_eq!(
            unknown.apply_pcr_ticks(raw),
            Err(SourceNativeEpochError::PcrRange)
        );
    }
    assert_eq!(policy.apply_pcr_ticks(0), Ok(Some(0)));
}

#[test]
fn signed_extremes_are_checked_without_modulo_masking_of_results() {
    let add = SourceNativeEpoch::from_first_demux_timestamps(Some(0), None);
    let subtract = SourceNativeEpoch::from_first_demux_timestamps(Some(-1), None);
    assert_eq!(add.apply_timestamp(i64::MAX), Ok(Some(i64::MAX)));
    assert_eq!(
        add.apply_timestamp(i64::MIN + 1),
        Ok(Some(i64::MIN + 1 + PERIOD))
    );
    assert_eq!(
        subtract.apply_timestamp(i64::MAX),
        Ok(Some(i64::MAX - PERIOD))
    );
    assert_eq!(
        subtract.apply_timestamp(i64::MIN + 1),
        Ok(Some(i64::MIN + 1))
    );
}
