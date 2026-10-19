//! Durable programme continuity does not persist a prior Lua profile selection.

use super::{
    assert_no_publication_or_invocation, bindings, build, configuration, publish_result,
    seed_publication_inputs, Fixture, ADDON,
};
use chrono::Utc;
use ersatzrs_core::repositories::PlayoutRepository as _;
use ersatzrs_core::scheduling::{
    PlayoutBuildMode, SchedulingMediaItem, SchedulingReferenceData, SchedulingRequest,
    SchedulingResult, ScriptedTeletextControl,
};
use ersatzrs_domain::{
    TeletextInstallationRead, TeletextOverrideState, TeletextProfileRead, TeletextProfileValue,
};
use ersatzrs_infra::repositories::{SqlitePlayoutRepository, SqliteSchedulingContentRepository};
use std::{collections::BTreeMap, sync::Arc};

#[derive(Debug, PartialEq)]
pub(super) struct Configuration {
    bindings: Vec<(i64, String, i64, bool, i64)>,
    settings: Vec<(String, Option<String>)>,
    profiles: Vec<TeletextProfileRead>,
    installation: TeletextInstallationRead,
}

impl Configuration {
    pub(super) async fn read(fixture: &Fixture) -> Self {
        let app = &fixture.state.application;
        Self {
            bindings: bindings(fixture).await,
            settings: configuration(fixture).await,
            profiles: app.list_teletext_profiles(ADDON).await.unwrap(),
            installation: app.teletext_installation_snapshot(ADDON).await.unwrap(),
        }
    }
}

pub(super) async fn reference(fixture: &Fixture, channel: i64) -> SchedulingReferenceData {
    SqliteSchedulingContentRepository::new(fixture.state.pool.clone())
        .scripted_reference_data(92 + channel, BTreeMap::new(), None)
        .await
        .unwrap()
}

fn assert_candidate(result: &SchedulingResult, channel: i64, profile: i64, theme: &str) {
    assert_eq!(result.added_items.len(), 1);
    assert_eq!(result.teletext_intervals.len(), 1);
    let interval = &result.teletext_intervals[0];
    assert_eq!(interval.start, result.added_items[0].start);
    assert_eq!(interval.finish, result.added_items[0].finish);
    assert!(
        matches!(&interval.state, TeletextOverrideState::Enabled(candidate)
        if candidate.channel_id == channel && candidate.profile_id == profile
        && candidate.effective_settings.iter().any(|setting| setting.key == "THEME"
            && setting.value == TeletextProfileValue::Literal(theme.to_owned())))
    );
}

async fn continue_from_storage(
    fixture: &Fixture,
    start: chrono::DateTime<Utc>,
) -> SchedulingResult {
    let playout = SqlitePlayoutRepository(fixture.state.pool.clone())
        .get_with_anchor(93)
        .await
        .unwrap();
    let mut reference_data = reference(fixture, 1).await;
    let boundary = reference_data.existing_items.last().unwrap().finish;
    assert_eq!(playout.anchor.as_ref().unwrap().next_start, boundary);
    assert_eq!(reference_data.existing_scripted_checkpoints.len(), 1);
    assert_eq!(
        reference_data.existing_scripted_checkpoints[0].checkpoint_date,
        boundary
    );
    assert!(!reference_data.existing_history.is_empty());
    reference_data.media_by_key.insert(
        "clip".to_owned(),
        vec![SchedulingMediaItem::new(71, chrono::Duration::minutes(1))],
    );
    let request = SchedulingRequest {
        playout,
        start,
        finish: boundary + chrono::Duration::minutes(1),
        mode: PlayoutBuildMode::Continue,
        reference_data,
    };
    let controls: Arc<dyn ScriptedTeletextControl> = fixture.state.application.clone();
    let builder = ersatzrs_infra::scripted_scheduling::playout_builder_with_teletext(
        fixture.state.pool.clone(),
        tokio::runtime::Handle::current(),
        None,
        Some(controls),
    );
    tokio::task::spawn_blocking(move || builder.build(request))
        .await
        .unwrap()
        .unwrap()
}

pub(super) async fn assert_no_activation(fixture: &mut Fixture) {
    assert_no_publication_or_invocation(fixture).await;
    for statement in [
        r#"SELECT COUNT(*) FROM "TeletextEffectivePublicationRequestCurrent""#,
        r#"SELECT COUNT(*) FROM "TeletextEffectiveAcceptedGenerationCurrent""#,
    ] {
        let count: i64 = sqlx::query_scalar(statement)
            .fetch_one(&fixture.state.pool)
            .await
            .unwrap();
        assert_eq!(count, 0);
    }
}

#[tokio::test]
async fn actual_lua_continue_after_reopen_restores_programme_checkpoint_but_selects_fresh_base() {
    let mut fixture = Fixture::new_persistent().await;
    let day = fixture.profile("DayBase1", "day", "good").await;
    let night = fixture.profile("NightBase2", "night", "good").await;
    fixture.bind(1, &day, None).await;
    fixture.bind(2, &night, None).await;
    seed_publication_inputs(&fixture).await;
    let configuration = Configuration::read(&fixture).await;
    // The unchanged script selects Night only on Reset. Continue executes On
    // without a profile command, while restoring the accepted source checkpoint.
    let script = format!(
        "if build_mode() == 'reset' then\n\
         assert(teletext_profile('{ADDON}', {}).ok)\n\
         else assert(build_mode() == 'continue') end\n\
         assert(teletext_enabled('{ADDON}', true).ok)\nadd_count('clip', 1)",
        night.id
    );
    let first = build(&fixture, &script, 1, true).await.unwrap();
    assert_candidate(&first, 1, night.id, "night");
    publish_result(&fixture, &first).await;
    let other = build(
        &fixture,
        &format!(
            "assert(teletext_profile('{ADDON}', {}).ok)\n\
             assert(teletext_enabled('{ADDON}', true).ok)\nadd_count('clip', 1)",
            day.id
        ),
        2,
        true,
    )
    .await
    .unwrap();
    assert_candidate(&other, 2, day.id, "day");
    publish_result(&fixture, &other).await;
    let prefix = reference(&fixture, 1).await;
    let other_reference = reference(&fixture, 2).await;
    let script_bytes = std::fs::read(fixture.root.path().join("controls-1.lua")).unwrap();
    assert_no_activation(&mut fixture).await;
    fixture = fixture.reopen().await;
    assert_eq!(reference(&fixture, 1).await, prefix);
    assert_eq!(reference(&fixture, 2).await, other_reference);
    let continued = continue_from_storage(&fixture, first.added_items[0].start).await;
    assert_candidate(&continued, 1, day.id, "day");
    assert!(!continued.clear_items);
    assert_eq!(continued.added_items[0].start, first.added_items[0].finish);
    publish_result(&fixture, &continued).await;
    assert_continued_timeline(&fixture, &prefix, &first, &continued).await;
    assert_eq!(reference(&fixture, 2).await, other_reference);
    let repository = SqlitePlayoutRepository(fixture.state.pool.clone());
    assert_eq!(
        repository
            .published_teletext_intervals_at(2, other.added_items[0].start)
            .await
            .unwrap(),
        other.teletext_intervals
    );
    assert_eq!(Configuration::read(&fixture).await, configuration);
    assert_eq!(
        std::fs::read(fixture.root.path().join("controls-1.lua")).unwrap(),
        script_bytes
    );
    assert_no_activation(&mut fixture).await;
}

async fn assert_continued_timeline(
    fixture: &Fixture,
    prefix: &SchedulingReferenceData,
    first: &SchedulingResult,
    continued: &SchedulingResult,
) {
    let accepted = reference(fixture, 1).await;
    assert_eq!(accepted.existing_items.len(), 2);
    assert_eq!(accepted.existing_items[..1], prefix.existing_items);
    assert_eq!(
        accepted.existing_items[1].start,
        continued.added_items[0].start
    );
    assert_eq!(
        accepted.existing_items[1].finish,
        continued.added_items[0].finish
    );
    for history in &prefix.existing_history {
        assert!(accepted.existing_history.contains(history));
    }
    for checkpoint in &prefix.existing_scripted_checkpoints {
        assert!(accepted.existing_scripted_checkpoints.contains(checkpoint));
    }
    let repository = SqlitePlayoutRepository(fixture.state.pool.clone());
    for result in [first, continued] {
        assert_eq!(
            repository
                .published_teletext_intervals_at(1, result.added_items[0].start)
                .await
                .unwrap(),
            result.teletext_intervals
        );
    }
    assert!(repository
        .published_teletext_intervals_at(1, continued.added_items[0].finish)
        .await
        .unwrap()
        .is_empty());
}
