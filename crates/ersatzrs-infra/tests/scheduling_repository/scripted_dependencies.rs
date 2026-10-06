use super::scripted_sources::{build, database};
use super::*;
use ersatzrs_core::{
    repositories::PlayoutRepository,
    scheduling::{PlayoutBuildMode, ScriptedSourceResolution},
};
use ersatzrs_infra::scripted_dependencies;

#[tokio::test]
async fn scripted_dependencies_capture_dynamic_empty_selection_and_survive_restart() {
    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().join("sources.lua");
    std::fs::write(&path, "local query = 'library_id:' .. tostring(1) .. ' AND tag:Selected'\nadd_search('new', query)\nadd_search('new', query)\nadd_collection('main', 'pri' .. 'me')\nadd_count('main', 1)").unwrap();
    let url = format!("sqlite://{}", directory.path().join("sources.db").display());
    let pool = database(&url, &path).await;
    let repos = make_repositories(pool.clone());
    let playout = repos.playout.get_with_anchor(903).await.unwrap();
    let start = common::fixed_time();
    let first = build(
        &pool,
        playout.clone(),
        start,
        start + Duration::hours(1),
        PlayoutBuildMode::Reset,
    )
    .await;
    assert_eq!(
        first.scripted_sources.len(),
        2,
        "repeated lookup is one dependency"
    );
    let selected = first
        .scripted_sources
        .iter()
        .find(|dependency| matches!(dependency.source, SchedulingContentSource::Search(_)))
        .unwrap();
    assert_eq!(
        selected.source,
        SchedulingContentSource::Search("library_id:1 AND tag:Selected".to_owned())
    );
    assert!(
        selected.library_fingerprints.is_empty(),
        "empty selections still have a descriptor"
    );
    repos
        .playout
        .apply_build_result(&first, &playout, Duration::zero())
        .await
        .unwrap();
    let published = scripted_dependencies::load(&pool, 903).await.unwrap();
    assert_eq!(published, first.scripted_sources);
    drop(repos);
    pool.close().await;
    let pool = ersatzrs_infra::db::init(&url).await.unwrap();
    assert_eq!(
        scripted_dependencies::load(&pool, 903).await.unwrap(),
        published
    );
    pool.execute(r#"INSERT INTO "Tag" ("MovieMetadataId", "Name") VALUES (200, 'Selected')"#)
        .await
        .unwrap();
    let changed = build(
        &pool,
        playout,
        start,
        start + Duration::hours(1),
        PlayoutBuildMode::Reset,
    )
    .await;
    let latest = changed
        .scripted_sources
        .iter()
        .find(|dependency| dependency.source == selected.source)
        .unwrap();
    assert_ne!(selected.fingerprint, latest.fingerprint);
    assert!(latest.library_fingerprints.contains_key(&1));
    assert_eq!(
        scripted_dependencies::load(&pool, 903).await.unwrap(),
        published,
        "generating a candidate does not change the published dependency set"
    );
}

#[tokio::test]
async fn scripted_dependencies_isolate_library_projections_and_playlist_settings() {
    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().join("sources.lua");
    std::fs::write(
        &path,
        "add_playlist('pl', 'nightly', 'prime')\nadd_count('pl', 1)",
    )
    .unwrap();
    let pool = database("sqlite::memory:", &path).await;
    common::seed_media_path(&pool, 2, 2, 2).await;
    pool.execute(r#"UPDATE "MediaItem" SET "LibraryPathId" = 2 WHERE "Id" = 20"#)
        .await
        .unwrap();
    let repos = make_repositories(pool.clone());
    let playout = repos.playout.get_with_anchor(903).await.unwrap();
    let start = common::fixed_time();
    let first = build(
        &pool,
        playout.clone(),
        start,
        start + Duration::hours(1),
        PlayoutBuildMode::Reset,
    )
    .await;
    assert_eq!(first.scripted_sources.len(), 1);
    let source = &first.scripted_sources[0];
    assert_eq!(source.resolution, ScriptedSourceResolution::Playlist);
    assert_eq!(source.library_fingerprints.len(), 2);
    let unchanged = build(
        &pool,
        playout.clone(),
        start,
        start + Duration::hours(1),
        PlayoutBuildMode::Reset,
    )
    .await;
    assert_eq!(first.scripted_sources, unchanged.scripted_sources);
    repos
        .playout
        .apply_build_result(&first, &playout, Duration::zero())
        .await
        .unwrap();
    let published = scripted_dependencies::load(&pool, 903).await.unwrap();
    pool.execute(r#"UPDATE "MovieMetadata" SET "Title" = 'Changed programme' WHERE "Id" = 200"#)
        .await
        .unwrap();
    let changed = build(
        &pool,
        playout.clone(),
        start,
        start + Duration::hours(1),
        PlayoutBuildMode::Reset,
    )
    .await;
    let latest = &changed.scripted_sources[0];
    assert_ne!(source.fingerprint, latest.fingerprint);
    assert_eq!(
        source.library_fingerprints[&1],
        latest.library_fingerprints[&1]
    );
    assert_ne!(
        source.library_fingerprints[&2],
        latest.library_fingerprints[&2]
    );
    let mut invalid = changed;
    invalid.added_items[0].finish = invalid.added_items[0].start;
    assert!(repos
        .playout
        .apply_build_result(&invalid, &playout, Duration::zero())
        .await
        .is_err());
    assert_eq!(
        scripted_dependencies::load(&pool, 903).await.unwrap(),
        published
    );
}

#[tokio::test]
async fn scripted_continue_preserves_dependencies_for_the_existing_future() {
    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().join("sources.lua");
    std::fs::write(
        &path,
        "add_collection('main', 'prime')\nwhile not is_done() do add_count('main', 1) end",
    )
    .unwrap();
    let pool = database("sqlite::memory:", &path).await;
    let repos = make_repositories(pool.clone());
    let playout = repos.playout.get_with_anchor(903).await.unwrap();
    let start = common::fixed_time();
    let first = build(
        &pool,
        playout.clone(),
        start,
        start + Duration::hours(1),
        PlayoutBuildMode::Reset,
    )
    .await;
    repos
        .playout
        .apply_build_result(&first, &playout, Duration::zero())
        .await
        .unwrap();
    let original = scripted_dependencies::load(&pool, 903).await.unwrap();
    std::fs::write(&path, "if is_done() then return end\nadd_collection('main', 'prime')\nwhile not is_done() do add_count('main', 1) end").unwrap();
    let playout = repos.playout.get_with_anchor(903).await.unwrap();
    let empty = build(
        &pool,
        playout.clone(),
        start,
        start + Duration::minutes(1),
        PlayoutBuildMode::Continue,
    )
    .await;
    assert!(empty.added_items.is_empty());
    assert!(empty.scripted_sources.is_empty());
    repos
        .playout
        .apply_build_result(&empty, &playout, Duration::zero())
        .await
        .unwrap();
    assert_eq!(
        scripted_dependencies::load(&pool, 903).await.unwrap(),
        original
    );
    pool.execute(r#"UPDATE "MovieMetadata" SET "Title" = 'Updated source' WHERE "Id" = 200"#)
        .await
        .unwrap();
    let extended = build(
        &pool,
        playout.clone(),
        start,
        start + Duration::hours(3),
        PlayoutBuildMode::Continue,
    )
    .await;
    assert_ne!(extended.scripted_sources, original);
    repos
        .playout
        .apply_build_result(&extended, &playout, Duration::zero())
        .await
        .unwrap();
    assert_eq!(
        scripted_dependencies::load(&pool, 903).await.unwrap(),
        original,
        "Continue cannot acknowledge a changed source for its retained future"
    );
    let reset = build(
        &pool,
        playout.clone(),
        start,
        start + Duration::hours(3),
        PlayoutBuildMode::Reset,
    )
    .await;
    repos
        .playout
        .apply_build_result(&reset, &playout, Duration::zero())
        .await
        .unwrap();
    assert_eq!(
        scripted_dependencies::load(&pool, 903).await.unwrap(),
        reset.scripted_sources
    );
}
