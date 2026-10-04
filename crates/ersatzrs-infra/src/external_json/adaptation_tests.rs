use super::*;
use chrono::{Duration, TimeZone};

fn start() -> DateTime<Utc> {
    Utc.with_ymd_and_hms(2026, 10, 4, 23, 59, 0).unwrap()
}

fn bytes(at: DateTime<Utc>, programmes: &[(&str, i32)]) -> Vec<u8> {
    serde_json::to_vec(&serde_json::json!({
        "startTime": at.to_rfc3339(),
        "guideMinimumDurationSeconds": 5,
        "programs": programmes.iter().map(|(title, duration)| serde_json::json!({
            "title": title, "duration": duration,
            "file": format!("media/{title}.mkv"), "type": "movie"
        })).collect::<Vec<_>>()
    }))
    .unwrap()
}

fn schedule(at: DateTime<Utc>, programmes: &[(&str, i32)]) -> ExternalJsonSchedule {
    super::super::parse(&bytes(at, programmes))
        .unwrap()
        .unwrap()
}

fn old() -> ExternalJsonSchedule {
    schedule(
        start(),
        &[
            ("A", 60_000),
            ("B", 60_000),
            ("C", 60_000),
            ("D", 60_000),
            ("E", 60_000),
            ("F", 60_000),
        ],
    )
}

fn titles(schedule: &ExternalJsonSchedule) -> Vec<&str> {
    schedule
        .items
        .iter()
        .map(|item| item.program.title.as_deref().unwrap())
        .collect()
}

fn refusal(result: Result<Adaptation, AdaptationError>, expected: AdaptationError) {
    assert_eq!(
        result
            .err()
            .expect("unsafe external adaptation was accepted"),
        expected
    );
}

#[test]
fn preserving_refresh_and_reset_keep_complete_prefetched_programmes_without_retiming() {
    let accepted = old();
    let candidate = schedule(
        start(),
        &[
            ("NewPast", 60_000),
            ("NewCurrent", 60_000),
            ("NewPrefetch", 60_000),
            ("NewFuture", 90_000),
            ("Next", 30_000),
        ],
    );
    for mode in [PlayoutBuildMode::Refresh, PlayoutBuildMode::Reset] {
        let result = reconcile(
            Some(&accepted),
            candidate.clone(),
            mode,
            PlayoutPreservation::KeepCurrent,
            start() + Duration::seconds(150),
        )
        .unwrap();
        assert_eq!(result.effective_from, Some(start() + Duration::minutes(3)));
        assert_eq!(
            titles(&result.schedule),
            ["A", "B", "C", "NewFuture", "Next"]
        );
        assert!(result.schedule.items[..3] == accepted.items[..3]);
        assert!(result.schedule.items[3..] == candidate.items[3..]);
        assert_eq!(
            result.schedule.items[3].finish,
            start() + Duration::seconds(270)
        );
    }
}

#[test]
fn differing_durations_defer_to_a_shared_boundary_instead_of_cutting_an_opening() {
    let accepted = old();
    let candidate = schedule(
        start(),
        &[
            ("X", 45_000),
            ("Y", 75_000),
            ("Z", 75_000),
            ("Q", 45_000),
            ("NewOpening", 30_000),
        ],
    );
    let result = reconcile(
        Some(&accepted),
        candidate.clone(),
        PlayoutBuildMode::Refresh,
        PlayoutPreservation::KeepCurrent,
        start() + Duration::seconds(150),
    )
    .unwrap();
    assert_eq!(result.effective_from, Some(start() + Duration::minutes(4)));
    assert_eq!(titles(&result.schedule), ["A", "B", "C", "D", "NewOpening"]);
    assert!(result.schedule.items[4] == candidate.items[4]);
    let incompatible = schedule(start(), &[("X", 179_000), ("Y", 20_000), ("Z", 20_000)]);
    refusal(
        reconcile(
            Some(&accepted),
            incompatible,
            PlayoutBuildMode::Reset,
            PlayoutPreservation::KeepCurrent,
            start() + Duration::seconds(150),
        ),
        AdaptationError::NoSafeBoundary,
    );
}

#[test]
fn continue_only_appends_an_exact_adjoining_suffix_and_keeps_pruned_past() {
    let accepted = old();
    let candidate = schedule(
        start() + Duration::minutes(4),
        &[("E", 60_000), ("F", 60_000), ("G", 1_234), ("H", 2_345)],
    );
    let result = reconcile(
        Some(&accepted),
        candidate.clone(),
        PlayoutBuildMode::Continue,
        PlayoutPreservation::KeepCurrent,
        start(),
    )
    .unwrap();
    assert_eq!(result.effective_from, Some(start() + Duration::minutes(6)));
    assert!(result.schedule.items[..6] == accepted.items);
    assert!(result.schedule.items[6..] == candidate.items[2..]);
    assert_eq!(
        result.schedule.items[7].finish,
        start() + Duration::milliseconds(363_579)
    );
    for (candidate, error) in [
        (
            schedule(
                start() + Duration::minutes(5),
                &[("Changed", 60_000), ("G", 60_000)],
            ),
            AdaptationError::ChangedPrefix,
        ),
        (
            schedule(start(), &[("A", 60_000)]),
            AdaptationError::ShortenedHorizon,
        ),
        (
            schedule(start() + Duration::minutes(7), &[("G", 60_000)]),
            AdaptationError::DisconnectedTail,
        ),
    ] {
        refusal(
            reconcile(
                Some(&accepted),
                candidate,
                PlayoutBuildMode::Continue,
                PlayoutPreservation::KeepCurrent,
                start(),
            ),
            error,
        );
    }
}

#[test]
fn missing_baseline_and_ambiguous_policy_are_refused_but_explicit_replace_is_valid() {
    for mode in [
        PlayoutBuildMode::Continue,
        PlayoutBuildMode::Refresh,
        PlayoutBuildMode::Reset,
    ] {
        refusal(
            reconcile(None, old(), mode, PlayoutPreservation::KeepCurrent, start()),
            AdaptationError::MissingBaseline,
        );
        refusal(
            reconcile(
                None,
                old(),
                mode,
                PlayoutPreservation::EngineDefault,
                start(),
            ),
            AdaptationError::InvalidPolicy,
        );
    }
    refusal(
        reconcile(
            None,
            old(),
            PlayoutBuildMode::Continue,
            PlayoutPreservation::ReplaceCurrent,
            start(),
        ),
        AdaptationError::InvalidPolicy,
    );
    for mode in [PlayoutBuildMode::Refresh, PlayoutBuildMode::Reset] {
        let candidate = old();
        let result = reconcile(
            None,
            candidate.clone(),
            mode,
            PlayoutPreservation::ReplaceCurrent,
            start(),
        )
        .unwrap();
        assert!(result.schedule == candidate);
    }
}

#[test]
fn no_change_is_idempotent_and_empty_candidate_withdraws_only_unprotected_future() {
    let accepted = old();
    let result = reconcile(
        Some(&accepted),
        accepted.clone(),
        PlayoutBuildMode::Refresh,
        PlayoutPreservation::KeepCurrent,
        start() + Duration::days(1),
    )
    .unwrap();
    assert!(result.schedule == accepted);
    assert!(!result.changed);
    assert_eq!(result.effective_from, None);
    let mut changed_past = accepted.clone();
    changed_past.items[0].program.title = Some("Changed past".into());
    let result = reconcile(
        Some(&accepted),
        changed_past,
        PlayoutBuildMode::Refresh,
        PlayoutPreservation::KeepCurrent,
        start() + Duration::seconds(150),
    )
    .unwrap();
    assert!(!result.changed);
    assert_eq!(result.effective_from, None);
    let result = reconcile(
        Some(&accepted),
        schedule(start(), &[]),
        PlayoutBuildMode::Reset,
        PlayoutPreservation::KeepCurrent,
        start() + Duration::seconds(150),
    )
    .unwrap();
    assert!(result.schedule.items == accepted.items[..3]);
    assert!(result.changed);
    assert_eq!(result.effective_from, Some(start() + Duration::minutes(3)));
    let result = reconcile(
        None,
        schedule(start(), &[]),
        PlayoutBuildMode::Reset,
        PlayoutPreservation::ReplaceCurrent,
        start(),
    )
    .unwrap();
    assert!(result.schedule.items.is_empty());
    let result = reconcile(
        Some(&accepted),
        schedule(start(), &[]),
        PlayoutBuildMode::Reset,
        PlayoutPreservation::ReplaceCurrent,
        start(),
    )
    .unwrap();
    assert!(result.changed);
    assert_eq!(result.effective_from, Some(start()));
}

#[test]
fn preservation_refuses_gaps_and_keeps_the_upcoming_reserved_programme() {
    let accepted = old();
    let candidate = schedule(start() + Duration::minutes(7), &[("Late", 60_000)]);
    refusal(
        reconcile(
            Some(&accepted),
            candidate,
            PlayoutBuildMode::Refresh,
            PlayoutPreservation::KeepCurrent,
            start() + Duration::minutes(2),
        ),
        AdaptationError::NoSafeBoundary,
    );
    let candidate = schedule(start(), &[("Replacement", 60_000), ("Later", 60_000)]);
    let result = reconcile(
        Some(&accepted),
        candidate,
        PlayoutBuildMode::Refresh,
        PlayoutPreservation::KeepCurrent,
        start() - Duration::seconds(10),
    )
    .unwrap();
    assert_eq!(titles(&result.schedule), ["A", "Later"]);
    assert_eq!(result.effective_from, Some(start() + Duration::minutes(1)));
}

#[test]
fn source_identity_guide_policy_and_interval_validation_protect_the_baseline() {
    let accepted = old();
    let mut changed = accepted.clone();
    changed.items[0].program.file = Some("media/replacement.mkv".into());
    refusal(
        reconcile(
            Some(&accepted),
            changed,
            PlayoutBuildMode::Continue,
            PlayoutPreservation::KeepCurrent,
            start(),
        ),
        AdaptationError::ChangedPrefix,
    );
    let mut changed = accepted.clone();
    changed.guide_minimum_duration_seconds = 120;
    refusal(
        reconcile(
            Some(&accepted),
            changed,
            PlayoutBuildMode::Refresh,
            PlayoutPreservation::KeepCurrent,
            start(),
        ),
        AdaptationError::ChangedGuideVisibility,
    );
    let mut malformed = accepted.clone();
    malformed.items[0].finish = start();
    refusal(
        reconcile(
            Some(&accepted),
            malformed,
            PlayoutBuildMode::Reset,
            PlayoutPreservation::ReplaceCurrent,
            start(),
        ),
        AdaptationError::InvalidTimeline,
    );
    for duration in [0, -1] {
        assert!(matches!(
            super::super::parse(&bytes(start(), &[("Invalid", duration)])),
            Err(super::super::ExternalJsonError::Duration)
        ));
    }
}

#[tokio::test]
async fn bounded_file_replacement_is_reconciled_against_the_retained_baseline() {
    let root = tempfile::tempdir().unwrap();
    let path = root.path().join("channel.json");
    tokio::fs::write(&path, bytes(start(), &[("A", 60_000), ("B", 60_000)]))
        .await
        .unwrap();
    let accepted = super::super::load(&path).await.unwrap().unwrap();
    tokio::fs::write(
        &path,
        bytes(start(), &[("Replacement", 60_000), ("New", 60_000)]),
    )
    .await
    .unwrap();
    let candidate = super::super::load(&path).await.unwrap().unwrap();
    let result = reconcile(
        Some(&accepted),
        candidate,
        PlayoutBuildMode::Refresh,
        PlayoutPreservation::KeepCurrent,
        start() + Duration::seconds(1),
    )
    .unwrap();
    assert_eq!(titles(&result.schedule), ["A", "New"]);
    assert_eq!(titles(&accepted), ["A", "B"]);
    tokio::fs::write(&path, b"{broken").await.unwrap();
    assert!(super::super::load(&path).await.is_err());
    assert_eq!(titles(&accepted), ["A", "B"]);
}
