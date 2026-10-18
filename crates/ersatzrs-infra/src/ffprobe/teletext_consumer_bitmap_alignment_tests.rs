use serde_json::{json, Value};

use super::{nearest_millisecond, project_bitmap_consumer_origin, NativeSourceBitmapAlignment};
use crate::ffprobe::teletext_consumer_origin::{parse_consumer_origin, NativeSourceConsumerOrigin};

const GROUPS: &[i64] = &[560_000, 25_080_000, 40_080_000];
const CAPTION_ZERO: i64 = 1_480_600;

fn fixture() -> Value {
    json!({
        "format":{"format_name":"mpegts","start_time":"1.418667"},
        "streams":[
            {"index":0,"id":"0x100","codec_type":"video","start_pts":129_600,"time_base":"1/90000"},
            {"index":1,"id":"0x101","codec_type":"audio","start_pts":127_680,"time_base":"1/90000"},
            {"index":2,"id":"0x102","codec_type":"subtitle","start_pts":133_254,"time_base":"1/90000"},
            {"index":3,"id":"0x103","codec_type":"audio","start_pts":127_680,"time_base":"1/90000"}
        ]
    })
}

fn origin(metadata: &Value, enabled: &[i32]) -> NativeSourceConsumerOrigin {
    parse_consumer_origin(&serde_json::to_vec(metadata).unwrap(), enabled).unwrap()
}

fn project(metadata: &Value, seek: i64) -> Option<NativeSourceBitmapAlignment> {
    project_bitmap_consumer_origin(
        &origin(metadata, &[0, 3]),
        &origin(metadata, &[2]),
        CAPTION_ZERO,
        GROUPS,
        seek,
    )
}

fn microsecond_fixture(av_start: i64, caption_start: i64) -> Value {
    let mut value = fixture();
    value["format"]["start_time"] = json!("1.000000");
    value["streams"][0]["start_pts"] = json!(av_start);
    value["streams"][0]["time_base"] = json!("1/1000000");
    value["streams"][2]["start_pts"] = json!(caption_start);
    value["streams"][2]["time_base"] = json!("1/1000000");
    value
}

fn microsecond_projection(
    metadata: &Value,
    caption_zero: i64,
    groups: &[i64],
    seek: i64,
) -> Option<NativeSourceBitmapAlignment> {
    project_bitmap_consumer_origin(
        &origin(metadata, &[0]),
        &origin(metadata, &[2]),
        caption_zero,
        groups,
        seek,
    )
}

#[test]
fn bitmap_projection_preserves_before_first_and_consumed_seek_uncertainty_and_residuals() {
    let before = project(&fixture(), 0).unwrap();
    assert_eq!(
        before.artifact_instep_bounds_microseconds(),
        (-61_933, -61_932)
    );
    assert_eq!(before.active_group_index(), None);
    assert_eq!(before.following_group_index(), Some(0));
    assert_eq!(before.following_delay_microseconds(), Some(622_000));
    assert_eq!(
        before.following_delay_residual_bounds_microseconds(),
        Some((67, 68))
    );

    let active = project(&fixture(), 17_520_000).unwrap();
    assert_eq!(
        active.artifact_instep_bounds_microseconds(),
        (17_458_066, 17_458_068)
    );
    assert_eq!(active.active_group_index(), Some(0));
    assert_eq!(active.following_group_index(), Some(1));
    assert_eq!(active.following_delay_microseconds(), Some(7_622_000));
    assert_eq!(
        active.following_delay_residual_bounds_microseconds(),
        Some((66, 68))
    );
    assert_ne!(
        active.artifact_instep_bounds_microseconds(),
        (17_520_000, 17_520_000)
    );
}

#[test]
fn bitmap_projection_uses_only_actual_enabled_av_tracks_and_their_real_time_base() {
    let mut metadata = fixture();
    metadata["streams"][1]["start_pts"] = json!(0);
    // Unselected English audio must not replace the selected Dutch input owner.
    assert_eq!(project(&metadata, 0), project(&fixture(), 0));
    let video_only = project_bitmap_consumer_origin(
        &origin(&metadata, &[0]),
        &origin(&metadata, &[2]),
        CAPTION_ZERO,
        GROUPS,
        0,
    )
    .unwrap();
    assert_eq!(
        video_only.artifact_instep_bounds_microseconds(),
        (-40_600, -40_600)
    );

    metadata["format"]["start_time"] = json!("1.000000");
    metadata["streams"][0]["start_pts"] = json!(12000);
    metadata["streams"][0]["time_base"] = json!("7/48000");
    metadata["streams"][2]["start_pts"] = json!(9600);
    metadata["streams"][2]["time_base"] = json!("7/48000");
    let actual_base = project_bitmap_consumer_origin(
        &origin(&metadata, &[0]),
        &origin(&metadata, &[2]),
        1_400_000,
        &[0, 560_000, 1_000_000],
        0,
    )
    .unwrap();
    assert_eq!(
        actual_base.artifact_instep_bounds_microseconds(),
        (350_000, 350_000)
    );
    assert_eq!(actual_base.following_delay_microseconds(), Some(210_000));
    assert_eq!(
        actual_base.following_delay_residual_bounds_microseconds(),
        Some((0, 0))
    );
}

#[test]
fn bitmap_projection_preserves_exact_update_and_after_clear_without_inventing_future_delay() {
    let groups = &[0, 1_000_000, 2_000_000];
    let metadata = microsecond_fixture(3_000_000, 2_000_000);
    let exact = microsecond_projection(&metadata, 2_000_000, groups, 0).unwrap();
    assert_eq!(
        exact.artifact_instep_bounds_microseconds(),
        (1_000_000, 1_000_000)
    );
    assert_eq!(exact.active_group_index(), Some(1));
    assert_eq!(exact.following_group_index(), Some(2));
    assert_eq!(exact.following_delay_microseconds(), Some(1_000_000));
    assert_eq!(
        exact.following_delay_residual_bounds_microseconds(),
        Some((0, 0))
    );

    let after = microsecond_projection(
        &microsecond_fixture(5_000_000, 2_000_000),
        2_000_000,
        groups,
        0,
    )
    .unwrap();
    assert_eq!(after.active_group_index(), Some(2));
    assert_eq!(after.following_group_index(), None);
    assert_eq!(after.following_delay_microseconds(), None);
    assert_eq!(after.following_delay_residual_bounds_microseconds(), None);
}

#[test]
fn bitmap_projection_rejects_uncertain_group_or_rounding_boundary_and_zero_rounded_delay() {
    let metadata = microsecond_fixture(0, 2_000_000);
    assert!(microsecond_projection(&metadata, 2_000_000, &[0, 1_000_000], 1_000_000).is_none());
    // Group selection remains stable, but 1499/1500/1501us straddle the
    // following delay's nearest-millisecond half tie.
    assert!(microsecond_projection(&metadata, 2_000_000, &[0, 2_000], 1_000_500).is_none());
    assert!(microsecond_projection(
        &microsecond_fixture(3_999_999, 2_000_000),
        2_000_000,
        &[0, 2_000_000],
        0,
    )
    .is_none());
    // Actual fixture format uncertainty crosses an authored update boundary.
    assert!(project(&fixture(), 25_141_933).is_none());
}

#[test]
fn bitmap_projection_rounding_is_signed_nearest_with_half_ties_away_from_zero() {
    for (input, expected) in [
        (-1500, -2000),
        (-500, -1000),
        (-499, 0),
        (499, 0),
        (500, 1000),
        (1500, 2000),
    ] {
        assert_eq!(nearest_millisecond(input), Some(expected));
    }
    let exact_half = microsecond_projection(
        &microsecond_fixture(3_000_500, 2_000_000),
        2_000_000,
        &[0, 1_002_000],
        0,
    )
    .unwrap();
    assert_eq!(exact_half.following_delay_microseconds(), Some(2000));
    assert_eq!(
        exact_half.following_delay_residual_bounds_microseconds(),
        Some((500, 500))
    );
}

#[test]
fn bitmap_projection_rejects_wrong_format_caption_zero_roles_and_missing_native_starts() {
    let metadata = fixture();
    let av = origin(&metadata, &[0, 3]);
    let caption = origin(&metadata, &[2]);
    assert!(project_bitmap_consumer_origin(&av, &caption, CAPTION_ZERO + 1, GROUPS, 0).is_none());
    for (av_indices, caption_indices) in [
        (&[2][..], &[2][..]),
        (&[0][..], &[0][..]),
        (&[0][..], &[2, 3][..]),
    ] {
        assert!(project_bitmap_consumer_origin(
            &origin(&metadata, av_indices),
            &origin(&metadata, caption_indices),
            CAPTION_ZERO,
            GROUPS,
            0,
        )
        .is_none());
    }
    for (pointer, replacement) in [
        ("/format/start_time", json!("1.418668")),
        ("/format/format_name", json!("matroska,webm")),
        ("/streams/2/start_pts", Value::Null),
        ("/streams/2/time_base", Value::Null),
    ] {
        let mut changed = metadata.clone();
        *changed.pointer_mut(pointer).unwrap() = replacement;
        assert!(
            project_bitmap_consumer_origin(&av, &origin(&changed, &[2]), CAPTION_ZERO, GROUPS, 0,)
                .is_none(),
            "{pointer}"
        );
    }
    let mut no_av_start = metadata;
    no_av_start["streams"][0]["start_pts"] = Value::Null;
    no_av_start["streams"][3]["start_pts"] = Value::Null;
    assert!(project(&no_av_start, 0).is_none());
}

#[test]
fn bitmap_projection_bounds_groups_and_checked_native_arithmetic() {
    let metadata = fixture();
    let av = origin(&metadata, &[0, 3]);
    let caption = origin(&metadata, &[2]);
    for groups in [
        &[][..],
        &[-1000][..],
        &[0, 0][..],
        &[1000, 0][..],
        &[0, 1001][..],
    ] {
        assert!(project_bitmap_consumer_origin(&av, &caption, CAPTION_ZERO, groups, 0).is_none());
    }
    let too_many: Vec<_> = (0..4097).map(|point| point * 1000).collect();
    assert!(project_bitmap_consumer_origin(&av, &caption, CAPTION_ZERO, &too_many, 0).is_none());
    assert!(project(&metadata, -1).is_none());
    assert!(project(&metadata, i64::MAX).is_none());

    let mut near_limit = microsecond_fixture(400, 1000);
    near_limit["format"]["start_time"] = json!("0.000000");
    assert!(microsecond_projection(&near_limit, 1000, &[9_223_372_036_854_775_000], 0,).is_none());
    near_limit["format"]["start_time"] = json!("-0.000001");
    near_limit["streams"][2]["start_pts"] = json!(i64::MAX);
    assert!(microsecond_projection(&near_limit, i64::MAX, GROUPS, 0).is_none());
}
