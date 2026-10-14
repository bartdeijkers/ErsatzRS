//! Accepted scheduling transactions, not Lua producer or native activation proof.

use super::*;
use crate::repositories::{common::encode_enum, SqlitePlayoutRepository};
use chrono::{DateTime, Duration, TimeZone as _, Utc};
use ersatzrs_core::repositories::PlayoutRepository as _;
use ersatzrs_core::scheduling::{SchedulingResult, SchedulingSmartShuffleState};
use ersatzrs_domain::{
    FillerKind, Playout, PlayoutAnchor, PlayoutItem, PlayoutScheduleKind,
    PlayoutScriptedCheckpoint, TeletextOverrideInterval, TeletextOverrideState,
};

fn at(minute: i64) -> DateTime<Utc> {
    Utc.with_ymd_and_hms(2026, 1, 1, 0, 0, 0).unwrap() + Duration::minutes(minute)
}

fn item(start: i64, finish: i64) -> PlayoutItem {
    PlayoutItem {
        id: 0,
        media_item_id: 1,
        start: at(start),
        finish: at(finish),
        guide_start: Some(at(start)),
        guide_finish: Some(at(finish)),
        custom_title: None,
        guide_group: 0,
        filler_kind: FillerKind::None,
        playout_id: 1,
        in_point: Duration::zero(),
        out_point: at(finish) - at(start),
        chapter_title: None,
        watermark_ids: Vec::new(),
        disable_watermarks: false,
        preferred_audio_language_code: None,
        preferred_audio_title: None,
        preferred_subtitle_language_code: None,
        subtitle_mode: None,
        pinned_stream_selection: None,
        block_key: None,
        collection_key: None,
        collection_etag: None,
        graphics_element_ids: Vec::new(),
        graphics_elements: Vec::new(),
        scheduling_context: None,
    }
}

fn build(items: Vec<PlayoutItem>, intervals: Vec<TeletextOverrideInterval>) -> SchedulingResult {
    let finish = items.last().unwrap().finish;
    SchedulingResult {
        anchor: Some(PlayoutAnchor { next_start: finish, ..PlayoutAnchor::default() }),
        scripted_checkpoints: items.iter().map(|item| PlayoutScriptedCheckpoint {
            playout_id: 1, checkpoint_date: item.finish,
            context: serde_json::json!({"version":1,"next_start":item.finish,
                "guide_group":0,"smart_shuffle":SchedulingSmartShuffleState::default(),"sources":{}}).to_string(),
        }).collect(),
        added_items: items,
        teletext_intervals: intervals,
        ..SchedulingResult::default()
    }
}

fn interval(start: i64, finish: i64, state: TeletextOverrideState) -> TeletextOverrideInterval {
    TeletextOverrideInterval {
        start: at(start),
        finish: at(finish),
        state,
    }
}

fn off() -> TeletextOverrideState {
    TeletextOverrideState::Off {
        channel_id: 1,
        addon_id: ADDON.to_owned(),
    }
}

async fn setup(fixture: &Fixture) -> (SqlitePlayoutRepository, Playout, TeletextOverrideState) {
    let base = fixture.profile("Base", "day").await;
    let target = fixture.profile("Target", "night").await;
    fixture.bind(1, &base).await;
    fixture.bind(2, &base).await;
    let candidate = saved(
        fixture
            .repo
            .candidate_snapshot(1, ADDON, target.id)
            .await
            .unwrap(),
    );
    for statement in [
        r#"INSERT INTO "MediaSource" ("Id") VALUES (1)"#,
        r#"INSERT INTO "Library" ("Id", "MediaSourceId", "MediaKind") VALUES (1, 1, 0)"#,
        r#"INSERT INTO "LibraryPath" ("Id", "LibraryId") VALUES (1, 1)"#,
        r#"INSERT INTO "MediaItem" ("Id", "LibraryPathId") VALUES (1, 1)"#,
    ] {
        sqlx::query(statement)
            .execute(&fixture.repo.0)
            .await
            .unwrap();
    }
    sqlx::query(
        r#"INSERT INTO "Playout" ("Id", "ChannelId", "ScheduleKind", "Seed") VALUES (1, 1, ?, 7)"#,
    )
    .bind(encode_enum(PlayoutScheduleKind::Scripted))
    .execute(&fixture.repo.0)
    .await
    .unwrap();
    (
        SqlitePlayoutRepository(fixture.repo.0.clone()),
        Playout {
            id: 1,
            channel_id: 1,
            schedule_kind: PlayoutScheduleKind::Scripted,
            seed: 7,
            ..Playout::default()
        },
        TeletextOverrideState::Enabled(candidate),
    )
}

async fn count(repo: &SqlitePlayoutRepository) -> i64 {
    sqlx::query_scalar(r#"SELECT COUNT(*) FROM "PlayoutTeletextInterval""#)
        .fetch_one(&repo.0)
        .await
        .unwrap()
}

#[tokio::test]
async fn intervals_publish_half_open_with_items_and_off_uses_only_membership() {
    let fixture = Fixture::new().await;
    let (repo, playout, on) = setup(&fixture).await;
    let intervals = vec![interval(0, 2, on.clone()), interval(2, 4, off())];
    repo.apply_build_result(
        &build(vec![item(0, 4)], intervals.clone()),
        &playout,
        Duration::zero(),
    )
    .await
    .unwrap();
    assert_eq!(
        repo.published_teletext_intervals_at(1, at(0))
            .await
            .unwrap(),
        intervals[..1]
    );
    assert_eq!(
        repo.published_teletext_intervals_at(1, at(2))
            .await
            .unwrap(),
        intervals[1..]
    );
    assert!(repo
        .published_teletext_intervals_at(1, at(4))
        .await
        .unwrap()
        .is_empty());
    sqlx::query(r#"UPDATE "AddonInstallation" SET "Enabled" = 0"#)
        .execute(&repo.0)
        .await
        .unwrap();
    assert!(repo
        .published_teletext_intervals_at(1, at(1))
        .await
        .is_err());
    assert_eq!(
        repo.published_teletext_intervals_at(1, at(3))
            .await
            .unwrap(),
        intervals[1..]
    );
    assert!(matches!(
        fixture
            .repo
            .scripted_base_selection(1, ADDON)
            .await
            .unwrap(),
        M::Saved(_)
    ));
    let mut next = build(vec![item(4, 8)], vec![interval(4, 8, off())]);
    next.scripted_checkpoints.insert(
        0,
        build(vec![item(0, 4)], Vec::new())
            .scripted_checkpoints
            .remove(0),
    );
    repo.apply_build_result(&next, &playout, Duration::zero())
        .await
        .unwrap();
    assert_eq!(count(&repo).await, 3);
}

#[tokio::test]
async fn stale_profile_or_late_interval_rejection_rolls_back_complete_previous_build() {
    for mode in 0..4 {
        let fixture = Fixture::new().await;
        let (repo, playout, on) = setup(&fixture).await;
        let previous = build(
            vec![item(0, 4)],
            if mode == 3 {
                Vec::new()
            } else {
                vec![interval(0, 4, on.clone())]
            },
        );
        repo.apply_build_result(&previous, &playout, Duration::zero())
            .await
            .unwrap();
        let items = repo.items_in_range(1, at(0), at(10)).await.unwrap();
        let mut replacement = build(vec![item(0, 8)], vec![interval(0, 8, on.clone())]);
        replacement.clear_items = true;
        match mode {
            0 => {
                sqlx::query(r#"UPDATE "TeletextProfile" SET "Revision" = "Revision" + 1 WHERE "Name" = 'Target'"#).execute(&repo.0).await.unwrap();
            }
            1 => replacement.teletext_intervals[0].finish = at(9),
            2 => replacement.teletext_intervals.push(interval(1, 3, off())),
            _ => {
                replacement.clear_items = false;
                replacement.added_items.clear();
                replacement.anchor = previous.anchor.clone();
                replacement.scripted_checkpoints = previous.scripted_checkpoints.clone();
                replacement.teletext_intervals = vec![interval(0, 4, on.clone())];
            }
        }
        assert!(repo
            .apply_build_result(&replacement, &playout, Duration::zero())
            .await
            .is_err());
        assert_eq!(repo.items_in_range(1, at(0), at(10)).await.unwrap(), items);
        assert_eq!(count(&repo).await, i64::from(mode != 3));
        let finish: Option<String> =
            sqlx::query_scalar(r#"SELECT MAX("Finish") FROM "PlayoutTeletextInterval""#)
                .fetch_one(&repo.0)
                .await
                .unwrap();
        assert_eq!(
            finish,
            (mode != 3).then(|| crate::repositories::common::encode_datetime(at(4)))
        );
        assert_eq!(
            repo.get_with_anchor(1)
                .await
                .unwrap()
                .anchor
                .unwrap()
                .next_start,
            at(4)
        );
    }
}

#[tokio::test]
async fn prepared_interval_cancellation_and_base_cas_changes_preserve_authority_boundaries() {
    let fixture = Fixture::new().await;
    let (repo, playout, on) = setup(&fixture).await;
    let proposed = build(vec![item(0, 4)], vec![interval(0, 4, on.clone())]);
    let pending = repo
        .prepare_build_result(&proposed, &playout, Duration::zero())
        .await
        .unwrap();
    drop(pending);
    assert_eq!(count(&repo).await, 0);
    assert!(repo
        .items_in_range(1, at(0), at(8))
        .await
        .unwrap()
        .is_empty());
    let base = fixture.repo.channel_bindings(1).await.unwrap()[0]
        .binding
        .clone();
    saved(
        fixture
            .repo
            .disable_binding(1, ADDON, base.revision)
            .await
            .unwrap(),
    );
    repo.apply_build_result(&proposed, &playout, Duration::zero())
        .await
        .unwrap();
    assert_eq!(
        repo.published_teletext_intervals_at(1, at(1))
            .await
            .unwrap(),
        proposed.teletext_intervals
    );
    sqlx::query(r#"UPDATE "AddonSetting" SET "Value" = 'changed' WHERE "Key" = 'ZONE'"#)
        .execute(&repo.0)
        .await
        .unwrap();
    assert!(repo
        .published_teletext_intervals_at(1, at(1))
        .await
        .is_err());
    assert!(repo
        .published_teletext_intervals_at(1, at(4))
        .await
        .unwrap()
        .is_empty());
    assert_eq!(count(&repo).await, 1);
}

#[tokio::test]
async fn refresh_seam_clips_crossing_override_then_continue_and_cleanup_preserve_prefix() {
    let fixture = Fixture::new().await;
    let (repo, playout, on) = setup(&fixture).await;
    let initial = build(
        vec![item(0, 4), item(4, 8)],
        vec![interval(0, 8, on.clone())],
    );
    repo.apply_build_result(&initial, &playout, Duration::zero())
        .await
        .unwrap();
    let mut refresh = build(vec![item(4, 12)], vec![interval(4, 12, off())]);
    refresh.clear_items = true;
    refresh.retain_through = Some(at(4));
    refresh
        .scripted_checkpoints
        .insert(0, initial.scripted_checkpoints[0].clone());
    repo.apply_build_result(&refresh, &playout, Duration::hours(1))
        .await
        .unwrap();
    assert_eq!(
        repo.published_teletext_intervals_at(1, at(3))
            .await
            .unwrap(),
        [interval(0, 4, on)]
    );
    assert_eq!(
        repo.published_teletext_intervals_at(1, at(4))
            .await
            .unwrap(),
        [interval(4, 12, off())]
    );
    let mut continuation = build(vec![item(12, 16)], Vec::new());
    continuation
        .scripted_checkpoints
        .splice(0..0, refresh.scripted_checkpoints.clone());
    repo.apply_build_result(&continuation, &playout, Duration::zero())
        .await
        .unwrap();
    assert!(repo
        .published_teletext_intervals_at(1, at(12))
        .await
        .unwrap()
        .is_empty());
    assert_eq!(count(&repo).await, 2);
    repo.delete_items_before(1, at(13)).await.unwrap();
    assert_eq!(count(&repo).await, 0);
}

#[tokio::test]
async fn output_channel_mirror_wrong_identity_and_malformed_active_snapshot_fail_closed() {
    let fixture = Fixture::new().await;
    let (repo, playout, on) = setup(&fixture).await;
    repo.apply_build_result(
        &build(vec![item(0, 4)], vec![interval(0, 4, on.clone())]),
        &playout,
        Duration::zero(),
    )
    .await
    .unwrap();
    sqlx::query(r#"UPDATE "Channel" SET "MirrorSourceChannelId" = 1 WHERE "Id" = 2"#)
        .execute(&repo.0)
        .await
        .unwrap();
    assert!(repo
        .published_teletext_intervals_at(2, at(1))
        .await
        .unwrap()
        .is_empty());
    let mut wrong = playout.clone();
    wrong.channel_id = 2;
    let mut next = build(vec![item(4, 8)], vec![interval(4, 8, off())]);
    next.scripted_checkpoints.insert(
        0,
        build(vec![item(0, 4)], Vec::new())
            .scripted_checkpoints
            .remove(0),
    );
    assert!(repo
        .apply_build_result(&next, &wrong, Duration::zero())
        .await
        .is_err());
    let encoded: String =
        sqlx::query_scalar(r#"SELECT "CandidateSnapshot" FROM "PlayoutTeletextInterval""#)
            .fetch_one(&repo.0)
            .await
            .unwrap();
    let mut snapshot: serde_json::Value = serde_json::from_str(&encoded).unwrap();
    snapshot["version"] = serde_json::json!(99);
    sqlx::query(r#"UPDATE "PlayoutTeletextInterval" SET "CandidateSnapshot" = ?"#)
        .bind(snapshot.to_string())
        .execute(&repo.0)
        .await
        .unwrap();
    assert!(repo
        .published_teletext_intervals_at(1, at(1))
        .await
        .is_err());
    assert!(repo
        .published_teletext_intervals_at(1, at(4))
        .await
        .unwrap()
        .is_empty());
    assert_eq!(count(&repo).await, 1);
    reject_aggregate_before_framing(&repo).await;
}

async fn reject_aggregate_before_framing(repo: &SqlitePlayoutRepository) {
    // Each malformed TEXT fits the individual schema guard; their combined
    // serialized material must be rejected before body fetch or JSON framing.
    let half = (6 * ersatzrs_domain::MAX_TELETEXT_OVERRIDE_SNAPSHOT_BYTES).midpoint(64 * 1024) + 1;
    for addon in [
        "org.example.aggregate.first",
        "org.example.aggregate.second",
    ] {
        sqlx::query(
            r#"INSERT INTO "PlayoutTeletextInterval"
            ("PlayoutId", "AddonId", "Start", "Finish", "State", "CandidateSnapshot")
            VALUES (1, ?, ?, ?, 1, CAST(zeroblob(?) AS TEXT))"#,
        )
        .bind(addon)
        .bind(crate::repositories::common::encode_datetime(at(0)))
        .bind(crate::repositories::common::encode_datetime(at(4)))
        .bind(i64::try_from(half).unwrap())
        .execute(&repo.0)
        .await
        .unwrap();
    }
    assert!(
        matches!(
            repo.published_teletext_intervals_at(1, at(1)).await,
            Err(RepositoryError::Conflict(_))
        ),
        "aggregate admission precedes malformed snapshot decode"
    );
}

#[tokio::test]
async fn accepted_interval_snapshot_survives_reopen_with_unresolved_values_and_no_replay() {
    use sqlx::sqlite::{SqliteConnectOptions, SqliteJournalMode};
    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().join("intervals.sqlite");
    let options = SqliteConnectOptions::new()
        .filename(&path)
        .create_if_missing(true)
        .foreign_keys(true)
        .journal_mode(SqliteJournalMode::Wal);
    let pool = SqlitePoolOptions::new()
        .max_connections(1)
        .connect_with(options.clone())
        .await
        .unwrap();
    let fixture = Fixture::from_pool(pool).await;
    let mut manifest = fixture.manifest.clone();
    let mut location = definition("LOCATION", AddonSettingKind::SecretReference, None);
    location.required = false;
    manifest.settings.push(location);
    fixture.store_manifest(&manifest).await;
    let (repo, playout, on) = setup(&fixture).await;
    let TeletextOverrideState::Enabled(candidate) = on else {
        panic!("expected candidate")
    };
    let mut save = fixture
        .save(
            "Target",
            vec![
                literal("THEME", "night"),
                TeletextProfileSetting {
                    key: "LOCATION".to_owned(),
                    value: TeletextProfileValue::SecretReference {
                        kind: TeletextSecretReferenceKind::Environment,
                        reference: "ERSATZRS_SYNTHETIC_TELETEXT_LOCATION".to_owned(),
                    },
                },
            ],
        )
        .await;
    save.existing = Some((candidate.profile_id, candidate.profile_revision));
    saved(fixture.repo.save_profile(&save).await.unwrap());
    let on = TeletextOverrideState::Enabled(saved(
        fixture
            .repo
            .candidate_snapshot(1, ADDON, candidate.profile_id)
            .await
            .unwrap(),
    ));
    assert!(!format!("{on:?}").contains("ERSATZRS_SYNTHETIC_TELETEXT_LOCATION"));
    let proposed = build(vec![item(0, 4)], vec![interval(0, 4, on)]);
    repo.apply_build_result(&proposed, &playout, Duration::zero())
        .await
        .unwrap();
    repo.0.close().await;
    let reopened = SqlitePlayoutRepository(
        SqlitePoolOptions::new()
            .max_connections(1)
            .connect_with(options)
            .await
            .unwrap(),
    );
    assert_eq!(
        reopened
            .published_teletext_intervals_at(1, at(1))
            .await
            .unwrap(),
        proposed.teletext_intervals
    );
    assert!(reopened
        .published_teletext_intervals_at(1, at(4))
        .await
        .unwrap()
        .is_empty());
    assert_eq!(count(&reopened).await, 1);
    assert_eq!(
        reopened
            .items_in_range(1, at(0), at(8))
            .await
            .unwrap()
            .len(),
        1
    );
    reopened.0.close().await;
}
