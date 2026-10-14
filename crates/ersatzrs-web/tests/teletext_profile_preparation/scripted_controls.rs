//! Actual application/SQL controls injected into the owned Scripted worker.
//! Candidate intervals are not accepted teletext generation activation.

use super::*;
use ersatzrs_core::scheduling::{
    PlayoutBuildMode, SchedulingMediaItem, SchedulingReferenceData, SchedulingRequest,
    SchedulingResult, ScriptedTeletextControl,
};
use ersatzrs_domain::{Playout, PlayoutScheduleKind, TeletextOverrideState};
use std::sync::Arc;

async fn bindings(fixture: &Fixture) -> Vec<(i64, String, i64, bool, i64)> {
    sqlx::query_as(
        r#"SELECT "ChannelId","AddonId","ProfileId","Enabled","Revision"
        FROM "ChannelTeletextBinding" ORDER BY "ChannelId","AddonId""#,
    )
    .fetch_all(&fixture.state.pool)
    .await
    .unwrap()
}

async fn configuration(fixture: &Fixture) -> Vec<(String, Option<String>)> {
    sqlx::query_as(r#"SELECT "Key","Value" FROM "ConfigElement" ORDER BY "Key""#)
        .fetch_all(&fixture.state.pool)
        .await
        .unwrap()
}

async fn assert_no_publication_or_invocation(fixture: &mut Fixture) {
    for statement in [
        r#"SELECT COUNT(*) FROM "TeletextPublicationRequest""#,
        r#"SELECT COUNT(*) FROM "TeletextPublicationRequestCurrent""#,
        r#"SELECT COUNT(*) FROM "TeletextAcceptedGeneration""#,
        r#"SELECT COUNT(*) FROM "TeletextAcceptedGenerationCurrent""#,
    ] {
        let count: i64 = sqlx::query_scalar(statement)
            .fetch_one(&fixture.state.pool)
            .await
            .unwrap();
        assert_eq!(count, 0);
    }
    assert!(fixture.playout.try_recv().is_err());
    for theme in ["day", "night"] {
        assert!(!fixture.root.path().join(format!("calls-{theme}")).exists());
    }
}

async fn build(
    fixture: &Fixture,
    script: &str,
    channel_id: i64,
    inject: bool,
) -> Result<SchedulingResult, ersatzrs_core::scheduling::SchedulingError> {
    build_at(
        fixture,
        script,
        channel_id,
        inject,
        Utc.with_ymd_and_hms(2026, 10, 14, 12, 0, 0).unwrap(),
    )
    .await
}

pub(super) async fn build_at(
    fixture: &Fixture,
    script: &str,
    channel_id: i64,
    inject: bool,
    start: chrono::DateTime<Utc>,
) -> Result<SchedulingResult, ersatzrs_core::scheduling::SchedulingError> {
    let file = fixture
        .root
        .path()
        .join(format!("controls-{channel_id}.lua"));
    std::fs::write(&file, script).unwrap();
    let builder = if inject {
        let controls: Arc<dyn ScriptedTeletextControl> = fixture.state.application.clone();
        ersatzrs_infra::scripted_scheduling::playout_builder_with_teletext(
            fixture.state.pool.clone(),
            tokio::runtime::Handle::current(),
            None,
            Some(controls),
        )
    } else {
        ersatzrs_infra::scripted_scheduling::playout_builder(
            fixture.state.pool.clone(),
            tokio::runtime::Handle::current(),
            None,
        )
    };
    let request = SchedulingRequest {
        playout: Playout {
            id: 92 + channel_id,
            channel_id,
            schedule_kind: PlayoutScheduleKind::Scripted,
            schedule_file: Some(file.to_str().unwrap().to_owned()),
            ..Playout::default()
        },
        start,
        finish: start + chrono::Duration::minutes(3),
        mode: PlayoutBuildMode::Reset,
        reference_data: SchedulingReferenceData {
            media_by_key: BTreeMap::from([(
                "clip".to_owned(),
                vec![SchedulingMediaItem::new(71, chrono::Duration::minutes(1))],
            )]),
            ..SchedulingReferenceData::default()
        },
    };
    tokio::task::spawn_blocking(move || builder.build(request))
        .await
        .unwrap()
}

#[tokio::test]
async fn application_scripted_candidate_keeps_base_and_preserves_typed_rejections() {
    let mut fixture = Fixture::new().await;
    let day = fixture.profile("Base", "day", "good").await;
    let night = fixture.profile("Candidate", "night", "good").await;
    fixture.bind(1, &day, None).await;
    let before = bindings(&fixture).await;
    let settings = configuration(&fixture).await;
    let application = &fixture.state.application;
    let candidate = saved(
        application
            .teletext_candidate(1, ADDON, night.id)
            .await
            .unwrap(),
    );
    assert_eq!(candidate.channel_id, 1);
    assert_eq!(candidate.profile_id, night.id);
    assert_eq!(candidate.profile_revision, night.revision);
    assert!(candidate.effective_settings.iter().any(|setting| {
        setting.key == "THEME" && setting.value == TeletextProfileValue::Literal("night".to_owned())
    }));
    for (channel, profile, expected) in [
        (2, night.id, TeletextProfileRejection::MissingBinding),
        (99, night.id, TeletextProfileRejection::MissingChannel),
        (1, 99_999, TeletextProfileRejection::MissingProfile),
    ] {
        assert_eq!(
            application
                .teletext_candidate(channel, ADDON, profile)
                .await
                .unwrap(),
            TeletextProfileMutation::Rejected(expected)
        );
    }
    assert_eq!(bindings(&fixture).await, before);
    assert_eq!(configuration(&fixture).await, settings);
    assert_no_publication_or_invocation(&mut fixture).await;
}

#[tokio::test]
async fn application_scripted_off_remains_available_for_unhealthy_package() {
    let mut fixture = Fixture::new().await;
    let day = fixture.profile("Base", "day", "good").await;
    fixture.bind(1, &day, None).await;
    fixture
        .state
        .application
        .disable_addon(ADDON.to_owned())
        .await
        .unwrap();
    let before = bindings(&fixture).await;
    let settings = configuration(&fixture).await;
    let base = saved(
        fixture
            .state
            .application
            .teletext_base(1, ADDON)
            .await
            .unwrap(),
    );
    assert_eq!(base.channel_id, 1);
    assert_eq!(base.profile_id, day.id);
    assert!(base.enabled);
    assert_eq!(
        fixture
            .state
            .application
            .teletext_candidate(1, ADDON, day.id)
            .await
            .unwrap(),
        TeletextProfileMutation::Rejected(TeletextProfileRejection::InstallationUnavailable)
    );
    let script = format!(
        "local on=teletext_enabled('{ADDON}', true)\nassert(not on.ok and on.code=='installation_unavailable')\n\
         assert(teletext_enabled('{ADDON}', false).ok)\nadd_count('clip', 1)"
    );
    let result = build(&fixture, &script, 1, true).await.unwrap();
    assert_eq!(result.added_items.len(), 1);
    assert_eq!(result.teletext_intervals.len(), 1);
    assert!(matches!(
        &result.teletext_intervals[0].state,
        TeletextOverrideState::Off { channel_id: 1, addon_id } if addon_id == ADDON
    ));
    assert_eq!(bindings(&fixture).await, before);
    assert_eq!(configuration(&fixture).await, settings);
    assert_no_publication_or_invocation(&mut fixture).await;
}

#[tokio::test]
async fn owned_scripted_worker_uses_shared_application_and_never_fabricates_missing_port() {
    let mut fixture = Fixture::new().await;
    let day = fixture.profile("Base", "day", "good").await;
    let night = fixture.profile("Candidate", "night", "good").await;
    fixture.bind(1, &day, None).await;
    let before = bindings(&fixture).await;
    let settings = configuration(&fixture).await;
    let script = format!(
        "assert(teletext_profile('{ADDON}', {}).ok)\n\
         assert(teletext_enabled('{ADDON}', true).ok)\nadd_count('clip', 1)\n\
         assert(teletext_enabled('{ADDON}', false).ok)\nadd_count('clip', 1)",
        night.id
    );
    let result = build(&fixture, &script, 1, true).await.unwrap();
    assert_eq!(result.added_items.len(), 2);
    assert_eq!(result.teletext_intervals.len(), 2);
    assert!(matches!(&result.teletext_intervals[0].state,
        TeletextOverrideState::Enabled(candidate) if candidate.channel_id == 1 && candidate.profile_id == night.id));
    assert!(matches!(&result.teletext_intervals[1].state,
        TeletextOverrideState::Off { channel_id: 1, addon_id } if addon_id == ADDON));
    assert_eq!(
        result.teletext_intervals[0].finish,
        result.teletext_intervals[1].start
    );
    assert_eq!(
        result.teletext_intervals[1].finish,
        result.added_items.last().unwrap().finish
    );
    let missing = format!(
        "local result=teletext_enabled('{ADDON}', false)\n\
         assert(not result.ok and result.code=='unavailable')\nadd_count('clip', 1)"
    );
    assert!(build(&fixture, &missing, 1, false)
        .await
        .unwrap()
        .teletext_intervals
        .is_empty());
    let foreign = format!(
        "local result=teletext_profile('{ADDON}', {})\n\
         assert(not result.ok and result.code=='missing_binding')\nadd_count('clip', 1)",
        night.id
    );
    assert!(build(&fixture, &foreign, 2, true)
        .await
        .unwrap()
        .teletext_intervals
        .is_empty());
    assert!(build(
        &fixture,
        &(script + "\nerror('synthetic rollback')"),
        1,
        true
    )
    .await
    .is_err());
    assert_eq!(bindings(&fixture).await, before);
    assert_eq!(configuration(&fixture).await, settings);
    assert_no_publication_or_invocation(&mut fixture).await;
}

pub(super) async fn seed_publication_inputs(fixture: &Fixture) {
    for statement in [
        r#"INSERT INTO "MediaSource" ("Id") VALUES (71)"#,
        r#"INSERT INTO "Library" ("Id","MediaSourceId","MediaKind") VALUES (71,71,0)"#,
        r#"INSERT INTO "LibraryPath" ("Id","LibraryId") VALUES (71,71)"#,
        r#"INSERT INTO "MediaItem" ("Id","LibraryPathId") VALUES (71,71)"#,
    ] {
        sqlx::query(statement)
            .execute(&fixture.state.pool)
            .await
            .unwrap();
    }
    for channel_id in [1, 2] {
        let file = fixture
            .root
            .path()
            .join(format!("controls-{channel_id}.lua"));
        sqlx::query(r#"INSERT INTO "Playout" ("Id","ChannelId","ScheduleKind","ScheduleFile","Seed") VALUES (?,?,4,?,0)"#)
            .bind(92 + channel_id).bind(channel_id).bind(file.to_str().unwrap())
            .execute(&fixture.state.pool).await.unwrap();
    }
}

pub(super) async fn publish_result(fixture: &Fixture, result: &SchedulingResult) {
    use ersatzrs_core::repositories::PlayoutRepository as _;
    let repository =
        ersatzrs_infra::repositories::SqlitePlayoutRepository(fixture.state.pool.clone());
    let playout = repository
        .get_with_anchor(result.added_items[0].playout_id)
        .await
        .unwrap();
    let start = result.added_items.first().unwrap().start;
    let finish = result.added_items.last().unwrap().finish;
    assert_eq!(
        result.scripted_checkpoints.last().unwrap().checkpoint_date,
        finish
    );
    let mut pending = repository
        .prepare_build_result(result, &playout, chrono::Duration::zero())
        .await
        .unwrap();
    let prepared = pending.items_in_range(start, finish).await.unwrap();
    assert_eq!(
        prepared
            .iter()
            .map(|item| (item.media_item_id, item.start, item.finish))
            .collect::<Vec<_>>(),
        result
            .added_items
            .iter()
            .map(|item| (item.media_item_id, item.start, item.finish))
            .collect::<Vec<_>>()
    );
    pending.commit().await.unwrap();
}

async fn assert_published(
    fixture: &Fixture,
    channel: i64,
    expected: &[ersatzrs_domain::TeletextOverrideInterval],
) {
    use ersatzrs_core::repositories::PlayoutRepository as _;
    let repository =
        ersatzrs_infra::repositories::SqlitePlayoutRepository(fixture.state.pool.clone());
    assert_eq!(
        repository
            .published_teletext_intervals_at(channel, expected[0].start)
            .await
            .unwrap(),
        expected[..1]
    );
    assert_eq!(
        repository
            .published_teletext_intervals_at(channel, expected[0].finish)
            .await
            .unwrap(),
        expected[1..]
    );
    assert!(repository
        .published_teletext_intervals_at(channel, expected[1].finish)
        .await
        .unwrap()
        .is_empty());
    let items = repository
        .items_in_range(92 + channel, expected[0].start, expected[1].finish)
        .await
        .unwrap();
    assert_eq!(items.len(), 2);
    assert!(items
        .iter()
        .all(|item| item.media_item_id == 71 && item.playout_id == 92 + channel));
}

#[tokio::test]
async fn application_lua_intervals_publish_with_real_timeline_and_survive_reopen_without_replay() {
    let mut fixture = Fixture::new_persistent().await;
    let day = fixture.profile("Day", "day", "good").await;
    let night = fixture.profile("Night", "night", "good").await;
    fixture.bind(1, &day, None).await;
    fixture.bind(2, &night, None).await;
    seed_publication_inputs(&fixture).await;
    let base = bindings(&fixture).await;
    let settings = configuration(&fixture).await;
    let mut intervals = Vec::new();
    for (channel, target, theme) in [(1, &night, "night"), (2, &day, "day")] {
        let script = format!(
            "assert(teletext_profile('{ADDON}', {}).ok)\n\
             assert(teletext_enabled('{ADDON}', true).ok)\nadd_count('clip', 1)\n\
             assert(teletext_enabled('{ADDON}', false).ok)\nadd_count('clip', 1)",
            target.id
        );
        let result = build(&fixture, &script, channel, true).await.unwrap();
        assert_eq!(result.teletext_intervals.len(), 2);
        assert!(
            matches!(&result.teletext_intervals[0].state, TeletextOverrideState::Enabled(value)
            if value.channel_id == channel && value.profile_id == target.id
            && value.effective_settings.iter().any(|setting| setting.key == "THEME"
                && setting.value == TeletextProfileValue::Literal(theme.to_owned())))
        );
        assert!(
            matches!(&result.teletext_intervals[1].state, TeletextOverrideState::Off { channel_id, addon_id }
            if *channel_id == channel && addon_id == ADDON)
        );
        publish_result(&fixture, &result).await;
        assert_published(&fixture, channel, &result.teletext_intervals).await;
        intervals.push(result.teletext_intervals);
    }
    assert_eq!(bindings(&fixture).await, base);
    assert_eq!(configuration(&fixture).await, settings);
    assert_no_publication_or_invocation(&mut fixture).await;
    fixture = fixture.reopen().await;
    for (index, channel) in [1, 2].into_iter().enumerate() {
        assert_published(&fixture, channel, &intervals[index]).await;
    }
    let handled = format!(
        "local missing=teletext_profile('{ADDON}', 999999)\n\
         assert(not missing.ok and missing.code=='missing_profile')\nadd_count('clip', 1)"
    );
    let rejected = build(&fixture, &handled, 1, true).await.unwrap();
    assert!(rejected.teletext_intervals.is_empty());
    let uncaught = format!(
        "assert(teletext_profile('{ADDON}', {}).ok)\nadd_count('clip', 1)\n\
         error('synthetic rollback after candidate')",
        night.id
    );
    assert!(build(&fixture, &uncaught, 1, true).await.is_err());
    for (index, channel) in [1, 2].into_iter().enumerate() {
        assert_published(&fixture, channel, &intervals[index]).await;
    }
    assert_eq!(bindings(&fixture).await, base);
    assert_eq!(configuration(&fixture).await, settings);
    assert_no_publication_or_invocation(&mut fixture).await;
}
