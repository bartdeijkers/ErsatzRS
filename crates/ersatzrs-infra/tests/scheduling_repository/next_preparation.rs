//! SQL and Next preparation share a candidate without publishing it early.

use super::{scripted_continuation::snapshot, scripted_refresh::published_fixture};
use super::{scripted_sources::build, *};
use ersatzrs_core::{
    locking::{EntityLocker, PlayoutLockGuard},
    repositories::{ChannelRepository, PlayoutRepository},
    scheduling::PlayoutBuildMode,
};
use ersatzrs_domain::{Channel, ChannelPlayoutSource, StreamingEngine};
use ersatzrs_infra::{
    next_engine::{coordinator::ChannelPublisher, protocol, publication, schedule::ScheduledMedia},
    repositories::SqlitePlayoutRepository,
};
use std::time::{Duration as StdDuration, Instant};

pub(super) async fn project(
    _: Channel,
    media: Vec<ScheduledMedia>,
) -> ersatzrs_core::repositories::Result<Vec<protocol::PlayoutItem>> {
    Ok(media
        .into_iter()
        .map(|media| protocol::PlayoutItem {
            id: media.item.id.to_string(),
            start: media.item.start,
            finish: media.item.finish,
            source: Some(protocol::Source::Local {
                path: media.path,
                in_point_ms: None,
                out_point_ms: None,
                probe_hint: None,
            }),
            tracks: None,
            watermark: None,
            graphics: Vec::new(),
        })
        .collect())
}

#[tokio::test]
async fn prepared_source_and_mirror_keep_old_sql_and_files_until_explicit_publication() {
    let root = tempfile::tempdir().unwrap();
    let url = format!("sqlite://{}", root.path().join("next.db").display());
    let (pool, _, first) = published_fixture(&url, &root.path().join("schedule.lua")).await;
    bind_fixture_files(&pool).await;
    let repos = make_repositories(pool.clone());
    let mut channel = repos.channel.get_with_profile(903).await.unwrap();
    channel.streaming_engine = StreamingEngine::Next;
    repos.channel.upsert(&channel).await.unwrap();
    let mirror = Channel {
        id: 905,
        number: "905".into(),
        playout_source: ChannelPlayoutSource::Mirror,
        mirror_source_channel_id: Some(903),
        playout_offset: Some(Duration::minutes(-5)),
        ..channel.clone()
    };
    repos.channel.upsert(&mirror).await.unwrap();
    let locker = EntityLocker::new();
    let source = ChannelPublisher::new(pool.clone(), locker.clone(), root.path(), 903);
    let mirror = ChannelPublisher::new(pool.clone(), locker.clone(), root.path(), 905);
    let start = first.added_items[0].start - Duration::hours(1);
    let finish = start + Duration::hours(8);
    source.refresh(start, finish, project).await.unwrap();
    mirror.refresh(start, finish, project).await.unwrap();
    let old_source =
        std::fs::read(root.path().join("903").join(publication::PLAYOUT_FILENAME)).unwrap();
    let old_mirror =
        std::fs::read(root.path().join("905").join(publication::PLAYOUT_FILENAME)).unwrap();
    let repository = SqlitePlayoutRepository(pool.clone());
    let playout = repository.get_with_anchor(903).await.unwrap();
    let result = build(
        &pool,
        playout.clone(),
        first.added_items[0].start + Duration::seconds(1),
        finish,
        PlayoutBuildMode::Refresh,
    )
    .await;
    let before = snapshot(&pool, 903).await;
    let _guard = PlayoutLockGuard::try_acquire(&locker, 903).unwrap();
    // First abandon a fully prepared pair, then publish an identical retry.
    for commit in [false, true] {
        let mut pending = repository
            .prepare_build_result(&result, &playout, Duration::zero())
            .await
            .unwrap();
        let candidate = pending.items_in_range(start, finish).await.unwrap();
        let deadline = Instant::now() + StdDuration::from_secs(10);
        let source_file = source
            .prepare_build(&mut pending, start, finish, deadline, project)
            .await
            .unwrap()
            .unwrap();
        let mirror_file = mirror
            .prepare_build(&mut pending, start, finish, deadline, project)
            .await
            .unwrap()
            .unwrap();
        assert_eq!(snapshot(&pool, 903).await, before);
        for (id, old) in [(903, &old_source), (905, &old_mirror)] {
            let folder = root.path().join(id.to_string());
            assert_eq!(
                &std::fs::read(folder.join(publication::PLAYOUT_FILENAME)).unwrap(),
                old
            );
            assert_eq!(std::fs::read_dir(folder).unwrap().count(), 2);
        }
        if commit {
            pending.commit().await.unwrap();
            source_file.publish().await.unwrap();
            mirror_file.publish().await.unwrap();
            for (id, shift) in [(903, Duration::zero()), (905, Duration::minutes(-5))] {
                assert_projection(root.path(), id, shift, &candidate);
            }
        } else {
            drop(source_file);
            drop(mirror_file);
            drop(pending);
            ersatzrs_infra::db::begin_write(&pool)
                .await
                .unwrap()
                .rollback()
                .await
                .unwrap();
            assert_eq!(snapshot(&pool, 903).await, before);
        }
        assert_eq!(
            std::fs::read_dir(root.path().join("903")).unwrap().count(),
            1
        );
        assert_eq!(
            std::fs::read_dir(root.path().join("905")).unwrap().count(),
            1
        );
    }
}

#[tokio::test]
async fn next_preparation_deadline_rolls_back_sql_without_replacing_the_manifest() {
    let root = tempfile::tempdir().unwrap();
    let url = format!("sqlite://{}", root.path().join("expired.db").display());
    let (pool, _, first) = published_fixture(&url, &root.path().join("schedule.lua")).await;
    bind_fixture_files(&pool).await;
    let repos = make_repositories(pool.clone());
    let mut channel = repos.channel.get_with_profile(903).await.unwrap();
    channel.streaming_engine = StreamingEngine::Next;
    repos.channel.upsert(&channel).await.unwrap();
    let locker = EntityLocker::new();
    let publisher = ChannelPublisher::new(pool.clone(), locker.clone(), root.path(), 903);
    let start = first.added_items[0].start;
    let finish = start + Duration::hours(6);
    publisher.refresh(start, finish, project).await.unwrap();
    let filename = root.path().join("903").join(publication::PLAYOUT_FILENAME);
    let old = std::fs::read(&filename).unwrap();
    let repository = SqlitePlayoutRepository(pool.clone());
    let playout = repository.get_with_anchor(903).await.unwrap();
    let mut result = build(
        &pool,
        playout.clone(),
        start + Duration::seconds(1),
        finish,
        PlayoutBuildMode::Refresh,
    )
    .await;
    let before = snapshot(&pool, 903).await;
    let _guard = PlayoutLockGuard::try_acquire(&locker, 903).unwrap();
    let deadline = Instant::now() + StdDuration::from_secs(1);
    result.publication_deadline = Some(deadline);
    let mut pending = repository
        .prepare_build_result(&result, &playout, Duration::zero())
        .await
        .unwrap();
    let error = publisher
        .prepare_build(
            &mut pending,
            start,
            finish,
            deadline + StdDuration::from_mins(1),
            |_, _| std::future::pending(),
        )
        .await
        .err()
        .expect("the SQL deadline bounds a stalled projector");
    assert!(error.to_string().contains("deadline"));
    drop(pending);
    ersatzrs_infra::db::begin_write(&pool)
        .await
        .unwrap()
        .rollback()
        .await
        .unwrap();
    assert_eq!(snapshot(&pool, 903).await, before);
    assert_eq!(std::fs::read(filename).unwrap(), old);
    assert_eq!(
        std::fs::read_dir(root.path().join("903")).unwrap().count(),
        1
    );
}

fn assert_projection(
    root: &std::path::Path,
    id: i64,
    shift: Duration,
    candidate: &[ersatzrs_domain::PlayoutItem],
) {
    let bytes = std::fs::read(
        root.join(id.to_string())
            .join(publication::PLAYOUT_FILENAME),
    )
    .unwrap();
    let published: protocol::Playout = serde_json::from_slice(&bytes).unwrap();
    assert_eq!(published.items.len(), candidate.len());
    for (item, source) in published.items.iter().zip(candidate) {
        assert_eq!(
            (item.id.clone(), item.start, item.finish),
            (
                source.id.to_string(),
                source.start + shift,
                source.finish + shift
            )
        );
    }
}

pub(super) async fn bind_fixture_files(pool: &SqlitePool) {
    pool.execute(
        r#"INSERT INTO "LibraryFolder" ("Id", "LibraryPathId", "Path") VALUES (70, 1, 'fixture')"#,
    )
    .await
    .unwrap();
    // Scheduling fixtures need durations but not files. Publication needs a
    // deterministic binding for every selected version; no media is played here.
    pool.execute(
        r#"INSERT INTO "MediaFile" ("Id", "LibraryFolderId", "MediaVersionId", "Path", "PathHash")
        SELECT 1000 + mv."Id", 70, mv."Id", 'fixture-' || mv."Id" || '.mkv', 'fixture-' || mv."Id"
        FROM "MediaVersion" mv WHERE NOT EXISTS
            (SELECT 1 FROM "MediaFile" mf WHERE mf."MediaVersionId" = mv."Id")"#,
    )
    .await
    .unwrap();
}
