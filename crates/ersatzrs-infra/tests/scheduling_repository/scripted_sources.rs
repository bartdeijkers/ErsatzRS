use super::*;
use ersatzrs_core::{
    repositories::PlayoutRepository,
    scheduling::{PlayoutBuildMode, SchedulingResult},
};

async fn build(
    pool: &SqlitePool,
    playout: Playout,
    start: chrono::DateTime<Utc>,
    finish: chrono::DateTime<Utc>,
    mode: PlayoutBuildMode,
) -> SchedulingResult {
    let repos = make_repositories(pool.clone());
    let reference_data = repos
        .scheduling
        .scripted_reference_data(playout.id, BTreeMap::new(), None)
        .await
        .unwrap();
    assert!(reference_data.collections_by_name.is_empty());
    assert!(reference_data.media_by_show_guid.is_empty());
    let builder = ersatzrs_infra::scripted_scheduling::playout_builder(
        pool.clone(),
        tokio::runtime::Handle::current(),
        None,
    );
    tokio::task::spawn_blocking(move || {
        builder.build(SchedulingRequest {
            playout,
            start,
            finish,
            mode,
            reference_data,
        })
    })
    .await
    .unwrap()
    .unwrap()
}

async fn database(url: &str, script_path: &std::path::Path) -> SqlitePool {
    let pool = ersatzrs_infra::db::init(url).await.unwrap();
    pool.execute(r#"INSERT INTO "Resolution" ("Id", "Name", "Width", "Height") VALUES (1, '1920x1080', 1920, 1080)"#).await.unwrap();
    seed_scheduling_fixture(&pool).await;
    let repos = make_repositories(pool.clone());
    common::seed_profile_and_channel(&repos, 903).await;
    sqlx::query(r#"INSERT INTO "Playout" ("Id", "ChannelId", "ScheduleKind", "Seed", "ScheduleFile") VALUES (903, 903, 'Scripted', 42, ?)"#)
        .bind(script_path.to_str().unwrap()).execute(&pool).await.unwrap();
    pool.execute(r#"UPDATE "CollectionItem" SET "PreferredAudioLanguageCode" = 'nld', "PreferredSubtitleLanguageCode" = 'eng', "SubtitleMode" = 2"#).await.unwrap();
    pool.execute(r#"UPDATE "PlaylistItem" SET "PreferredAudioLanguageCode" = 'nld', "PreferredSubtitleLanguageCode" = 'eng', "SubtitleMode" = 2"#).await.unwrap();
    pool
}

#[tokio::test]
async fn sql_scripted_sources_continue_after_restart_without_injected_membership() {
    for register in [
        "add_playlist('pl', 'night' .. 'ly', 'prime')",
        "add_collection('a', 'pri' .. 'me', 'chronological')\nadd_smart_collection('b', 'smart-bonus', 'chronological')\ncreate_playlist('pl', {a=1, b=1})",
        "add_marathon('pl', {tvdb='show-guid'}, {}, 'show')",
    ] {
        let directory = tempfile::tempdir().unwrap();
        let path = directory.path().join("schedule.lua");
        std::fs::write(&path, format!("{register}\nwatermark_on('bug')\ngraphics_on_path('lower-third.svg')\nwhile not is_done() do add_count('pl', 1) end\n")).unwrap();
        let url = format!("sqlite://{}", directory.path().join("scripted.db").display());
        let pool = database(&url, &path).await;
        let repos = make_repositories(pool.clone());
        let playout = repos.playout.get_with_anchor(903).await.unwrap();
        let start = common::fixed_time();
        let finish = start + Duration::hours(6);
        let full = build(&pool, playout.clone(), start, finish, PlayoutBuildMode::Reset).await;
        assert!(full.added_items.len() > 5);
        let first = build(&pool, playout.clone(), start, start + Duration::minutes(77), PlayoutBuildMode::Reset).await;
        let split = first.added_items.last().unwrap().finish;
        repos.playout.apply_build_result(&first, &playout, Duration::zero()).await.unwrap();
        drop(repos);
        pool.close().await;

        let pool = ersatzrs_infra::db::init(&url).await.unwrap();
        let repos = make_repositories(pool.clone());
        let playout = repos.playout.get_with_anchor(903).await.unwrap();
        let continued = build(&pool, playout.clone(), split, finish, PlayoutBuildMode::Continue).await;
        let expected = &full.added_items[first.added_items.len()..];
        assert_eq!(continued.added_items.len(), expected.len());
        for (actual, expected) in continued.added_items.iter().zip(expected) {
            assert_eq!((actual.media_item_id, actual.start, actual.finish), (expected.media_item_id, expected.start, expected.finish));
            assert_eq!(actual.preferred_audio_language_code, expected.preferred_audio_language_code);
            assert_eq!(actual.preferred_subtitle_language_code, expected.preferred_subtitle_language_code);
            assert_eq!(actual.subtitle_mode, expected.subtitle_mode);
            assert_eq!(actual.watermark_ids, [501]);
            assert_eq!(actual.graphics_elements, expected.graphics_elements);
            assert!(!actual.graphics_elements.is_empty());
        }
        repos.playout.apply_build_result(&continued, &playout, Duration::zero()).await.unwrap();
        drop(repos);
        pool.close().await;
    }
}

#[tokio::test]
async fn sql_scripted_registration_resolves_remaining_source_kinds() {
    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().join("schedule.lua");
    let url = format!("sqlite://{}", directory.path().join("sources.db").display());
    let pool = database(&url, &path).await;
    let repos = make_repositories(pool.clone());
    let playout = repos.playout.get_with_anchor(903).await.unwrap();
    for (script, expected) in [
        ("add_multi_collection('a', 'multi-prime', 'chronological')\nadd_all('a')", vec![12, 13, 20, 21]),
        ("add_search('a', 'title contains Pilot')\nadd_all('a')", vec![12]),
        ("add_show('a', {tvdb='show-guid'}, 'chronological')\nadd_all('a')", vec![12, 13]),
        ("add_playlist('a', 'nightly', 'prime')\nadd_count('a', 1)\nadd_playlist('a', 'nightly', 'prime')\nadd_count('a', 1)", vec![20, 12]),
    ] {
        std::fs::write(&path, script).unwrap();
        let result = build(&pool, playout.clone(), common::fixed_time(), common::fixed_time() + Duration::hours(6), PlayoutBuildMode::Reset).await;
        assert_eq!(result.added_items.iter().map(|item| item.media_item_id).collect::<Vec<_>>(), expected, "{script}");
    }
    pool.close().await;
}

#[tokio::test]
async fn sql_scripted_lookup_failure_or_cancellation_returns_no_partial_build() {
    for cancelled in [false, true] {
        let directory = tempfile::tempdir().unwrap();
        let path = directory.path().join("schedule.lua");
        std::fs::write(&path, "add_collection('a', 'prime')\nadd_count('a', 1)").unwrap();
        let url = format!("sqlite://{}", directory.path().join("failure.db").display());
        let pool = database(&url, &path).await;
        let repos = make_repositories(pool.clone());
        let playout = repos.playout.get_with_anchor(903).await.unwrap();
        let reference_data = repos
            .scheduling
            .scripted_reference_data(903, BTreeMap::new(), None)
            .await
            .unwrap();
        let builder = ersatzrs_infra::scripted_scheduling::playout_builder(
            pool.clone(),
            tokio::runtime::Handle::current(),
            Some(std::sync::Arc::new(move || cancelled)),
        );
        drop(repos);
        if !cancelled {
            pool.close().await;
        }
        let error = tokio::task::spawn_blocking(move || {
            builder.build(SchedulingRequest {
                playout,
                start: common::fixed_time(),
                finish: common::fixed_time() + Duration::hours(1),
                mode: PlayoutBuildMode::Continue,
                reference_data,
            })
        })
        .await
        .unwrap()
        .unwrap_err();
        assert!(error.to_string().contains("scripted source"));
        assert!(!error.to_string().contains(&url));
        pool.close().await;
    }
}
